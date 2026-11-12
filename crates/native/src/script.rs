//! QuickJS on a private thread. Script calls only touch the signal graph and
//! the op buffer; the tree is owned by the host/frame thread, so a dead loop
//! cannot stall layout.

use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use bubble_core::{
    validate_bundle_hashes, ComputationKind, GestureKind, NodeDesc, Op, Sched, Turn, Value,
    DEFERRED_PROPS, SPAWN_PROPS,
};
use rquickjs::{Array, Context, Ctx, Function, Object, Runtime, TypedArray, Value as JsValue};
use serde::Deserialize;

use crate::Shared;

const MEMORY_LIMIT: usize = 64 * 1024 * 1024;
const STACK_LIMIT: usize = 256 * 1024;
const INTERRUPT_MS: u64 = 1000;
const RUNTIME_JS: &str = include_str!("../../../examples/web/runtime.js");

static INTERRUPT: AtomicBool = AtomicBool::new(false);
static JS_SINCE_MS: AtomicU64 = AtomicU64::new(0);
static STOP_WATCH: AtomicBool = AtomicBool::new(false);
static MONO_BASE: LazyLock<Instant> = LazyLock::new(Instant::now);

thread_local! {
    static TURN: RefCell<Option<Turn>> = const { RefCell::new(None) };
    static ACTIVE: Cell<*mut Turn> = const { Cell::new(std::ptr::null_mut()) };
    static SHARED: RefCell<Option<Arc<Shared>>> = const { RefCell::new(None) };
    static ERROR: RefCell<String> = const { RefCell::new(String::new()) };
    static TIMERS: RefCell<Vec<Timer>> = const { RefCell::new(Vec::new()) };
    static REQUESTS: RefCell<Option<Arc<Mutex<std::collections::VecDeque<crate::HostRequest>>>>> =
        const { RefCell::new(None) };
    static NEXT_REQUEST: RefCell<u32> = const { RefCell::new(1) };
}

struct Timer {
    id: u32,
    at: Instant,
}

pub enum Cmd {
    Load {
        manifest: String,
        entry: Vec<u8>,
        ack: Sender<Result<(), String>>,
    },
    /// Encoded `BSGE` recognition batch from the frame thread. `epoch`
    /// identifies the mounted script instance; stale batches are dropped.
    Gesture {
        epoch: u32,
        bytes: Vec<u8>,
    },
    /// Encoded `BSED` editing batch. `epoch` matches `Gesture`.
    Edit {
        epoch: u32,
        bytes: Vec<u8>,
    },
    Eval {
        source: String,
        ack: Option<Sender<Result<String, String>>>,
    },
    Settle {
        id: u32,
        ok: bool,
        payload: String,
    },
    Wake,
    Shutdown,
}

pub fn start(shared: Arc<Shared>) -> (Sender<Cmd>, thread::JoinHandle<()>) {
    STOP_WATCH.store(false, Ordering::Relaxed);
    INTERRUPT.store(false, Ordering::Relaxed);
    thread::spawn(watchdog);
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = thread::Builder::new()
        .name("bubble-script".into())
        .spawn(move || script_main(rx, shared))
        .expect("spawn script thread");
    (tx, handle)
}

pub fn stop_watchdog() {
    STOP_WATCH.store(true, Ordering::Relaxed);
    INTERRUPT.store(true, Ordering::Relaxed);
}

fn mono_ms() -> u64 {
    MONO_BASE.elapsed().as_millis() as u64
}

fn watchdog() {
    while !STOP_WATCH.load(Ordering::Relaxed) {
        let started = JS_SINCE_MS.load(Ordering::Relaxed);
        if started != 0 && mono_ms().saturating_sub(started) > INTERRUPT_MS {
            INTERRUPT.store(true, Ordering::Relaxed);
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn enter_js() {
    INTERRUPT.store(false, Ordering::Relaxed);
    JS_SINCE_MS.store(mono_ms(), Ordering::Relaxed);
}

fn leave_js() {
    JS_SINCE_MS.store(0, Ordering::Relaxed);
}

struct Vm {
    context: Context,
    // Declared after `context` so it drops second. QuickJS requires that order.
    #[allow(dead_code)]
    runtime: Runtime,
}

struct Session {
    shared: Arc<Shared>,
    vm: Option<Vm>,
    bundle: Option<(String, Vec<u8>)>,
}

fn script_main(rx: Receiver<Cmd>, shared: Arc<Shared>) {
    TURN.with(|slot| *slot.borrow_mut() = Some(Turn::new()));
    SHARED.with(|slot| *slot.borrow_mut() = Some(Arc::clone(&shared)));
    REQUESTS.with(|slot| *slot.borrow_mut() = Some(Arc::clone(&shared.requests)));
    let mut session = Session {
        shared,
        vm: None,
        bundle: None,
    };
    loop {
        let fired = session.vm.as_ref().map(|vm| session.fire_due(vm));
        if let Some(Err(err)) = fired {
            if fatal(&err) {
                let _ = session.recover();
            } else {
                session.note_fault(&err);
            }
        }
        let timeout = session.next_timeout();
        let cmd = match timeout {
            Some(duration) => match rx.recv_timeout(duration) {
                Ok(cmd) => cmd,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            },
            None => match rx.recv() {
                Ok(cmd) => cmd,
                Err(_) => break,
            },
        };
        if !session.handle(cmd) {
            break;
        }
    }
}

impl Session {
    fn handle(&mut self, cmd: Cmd) -> bool {
        match cmd {
            Cmd::Shutdown => false,
            Cmd::Wake => true,
            Cmd::Load {
                manifest,
                entry,
                ack,
            } => {
                let result = self.load_bundle(manifest, entry);
                let _ = ack.send(result);
                true
            }
            Cmd::Gesture { epoch, bytes } => {
                if epoch != self.shared.input_epoch.load(Ordering::SeqCst) {
                    return true;
                }
                if self.dispatch_gestures(&bytes).is_ok() {
                    self.shared.generation.fetch_add(1, Ordering::Relaxed);
                }
                true
            }
            Cmd::Edit { epoch, bytes } => {
                if epoch != self.shared.input_epoch.load(Ordering::SeqCst) {
                    return true;
                }
                if self.dispatch_edits(&bytes).is_ok() {
                    self.shared.generation.fetch_add(1, Ordering::Relaxed);
                }
                true
            }
            Cmd::Eval { source, ack } => {
                let result = self.eval_user(&source);
                if let Some(ack) = ack {
                    let _ = ack.send(result);
                }
                true
            }
            Cmd::Settle { id, ok, payload } => {
                if let Err(err) = self.settle(id, ok, &payload) {
                    if fatal(&err) {
                        let _ = self.recover();
                    } else {
                        self.note_fault(&err);
                    }
                }
                true
            }
        }
    }

    fn load_bundle(&mut self, manifest: String, entry: Vec<u8>) -> Result<(), String> {
        check_manifest(&manifest, &entry)?;
        self.bundle = Some((manifest, entry));
        self.boot_vm()?;
        self.shared.set_status("running");
        Ok(())
    }

    fn boot_vm(&mut self) -> Result<(), String> {
        self.shared.input_epoch.fetch_add(1, Ordering::SeqCst);
        let Some((manifest, entry)) = self.bundle.clone() else {
            return Err("no bundle loaded".into());
        };
        let source =
            String::from_utf8(entry).map_err(|_| "bundle entry is not utf-8".to_string())?;
        let _ = manifest;
        self.vm = Some(make_vm()?);
        let vm = self.vm.as_ref().expect("vm");
        let loaded = guarded(|| install_and_run(vm, &source));
        match loaded {
            Ok(()) => Ok(()),
            Err(err) => {
                self.vm = None;
                Err(err)
            }
        }
    }

    fn eval_user(&mut self, source: &str) -> Result<String, String> {
        let Some(vm) = self.vm.as_ref() else {
            return Err("bundle is not loaded".into());
        };
        match guarded(|| eval_in(vm, source)) {
            Ok(text) => Ok(text),
            Err(err) => {
                if fatal(&err) {
                    self.recover()?;
                    Err(if INTERRUPT.load(Ordering::Relaxed) {
                        "interrupted".into()
                    } else {
                        err
                    })
                } else {
                    Err(err)
                }
            }
        }
    }

    fn dispatch_gestures(&mut self, bytes: &[u8]) -> Result<(), String> {
        let Some(vm) = self.vm.as_ref() else {
            return Ok(());
        };
        match guarded(|| gesture_in(vm, bytes)) {
            Ok(()) => Ok(()),
            Err(err) => {
                if fatal(&err) {
                    self.recover()?;
                } else {
                    self.note_fault(&err);
                }
                Err(err)
            }
        }
    }

    fn dispatch_edits(&mut self, bytes: &[u8]) -> Result<(), String> {
        let Some(vm) = self.vm.as_ref() else {
            return Ok(());
        };
        match guarded(|| edit_in(vm, bytes)) {
            Ok(()) => Ok(()),
            Err(err) => {
                if fatal(&err) {
                    self.recover()?;
                } else {
                    self.note_fault(&err);
                }
                Err(err)
            }
        }
    }

    fn settle(&mut self, id: u32, ok: bool, payload: &str) -> Result<(), String> {
        let Some(vm) = self.vm.as_ref() else {
            return Ok(());
        };
        guarded(|| settle_in(vm, id, ok, payload))
    }

    fn recover(&mut self) -> Result<(), String> {
        self.shared.faults.fetch_add(1, Ordering::Relaxed);
        self.vm = None;
        TIMERS.with(|slot| slot.borrow_mut().clear());
        self.boot_vm()?;
        self.shared.set_status("running");
        Ok(())
    }

    fn note_fault(&self, err: &str) {
        self.shared.faults.fetch_add(1, Ordering::Relaxed);
        self.shared.set_status("fault");
        self.shared.set_error(err);
    }

    fn fire_due(&self, vm: &Vm) -> Result<(), String> {
        if !self.shared.visible.load(Ordering::Relaxed) {
            return Ok(());
        }
        let now = Instant::now();
        let due: Vec<u32> = TIMERS.with(|slot| {
            let mut timers = slot.borrow_mut();
            let mut due = Vec::new();
            timers.retain(|timer| {
                if timer.at <= now {
                    due.push(timer.id);
                    false
                } else {
                    true
                }
            });
            due
        });
        if due.is_empty() {
            return Ok(());
        }
        guarded(|| {
            enter_js();
            let result = vm.context.with(|ctx| {
                for id in due {
                    let fire: Function = ctx.globals().get("__fireTimer")?;
                    fire.call::<_, ()>((id,))?;
                }
                pump(&ctx);
                Ok(())
            });
            leave_js();
            result.map_err(|err: rquickjs::Error| err.to_string())
        })
    }

    fn next_timeout(&self) -> Option<Duration> {
        if !self.shared.visible.load(Ordering::Relaxed) {
            return None;
        }
        let now = Instant::now();
        TIMERS.with(|slot| {
            slot.borrow()
                .iter()
                .map(|timer| timer.at.saturating_duration_since(now))
                .min()
        })
    }
}

/// A fault that leaves the VM unsafe to reuse: an interrupt or a memory
/// allocation failure. Both rebuild the context and remount the bundle.
fn fatal(err: &str) -> bool {
    INTERRUPT.load(Ordering::Relaxed)
        || err.contains("interrupted")
        || err.to_ascii_lowercase().contains("out of memory")
}

fn guarded<T>(body: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)) {
        Ok(result) => result,
        Err(_) => Err("script thread panicked".into()),
    }
}

fn make_vm() -> Result<Vm, String> {
    let runtime = Runtime::new().map_err(|err| err.to_string())?;
    runtime.set_memory_limit(MEMORY_LIMIT);
    runtime.set_max_stack_size(STACK_LIMIT);
    runtime.set_interrupt_handler(Some(Box::new(|| INTERRUPT.load(Ordering::Relaxed))));
    let context = Context::full(&runtime).map_err(|err| err.to_string())?;
    Ok(Vm { context, runtime })
}

fn install_and_run(vm: &Vm, source: &str) -> Result<(), String> {
    enter_js();
    let result = vm.context.with(|ctx| {
        bind_api(&ctx)?;
        let promise = rquickjs::Module::evaluate(ctx.clone(), "../runtime.js", RUNTIME_JS)?;
        finish_promise(&ctx, &promise)?;
        let set_api: Function = ctx.globals().get("__bubbleSetApi")?;
        let api: Object = ctx.globals().get("__bubble")?;
        set_api.call::<_, ()>((api,))?;
        ctx.eval::<(), _>(PRELUDE)?;
        let main = rquickjs::Module::evaluate(ctx.clone(), "main.js", source)?;
        finish_promise(&ctx, &main)?;
        pump(&ctx);
        Ok(())
    });
    leave_js();
    result.map_err(|err: rquickjs::Error| format_eval(vm, err))
}

fn eval_in(vm: &Vm, source: &str) -> Result<String, String> {
    enter_js();
    let result = vm.context.with(|ctx| {
        let value: JsValue = ctx.eval(source)?;
        pump(&ctx);
        Ok(value_string(&ctx, value))
    });
    leave_js();
    result.map_err(|err: rquickjs::Error| format_eval(vm, err))
}

fn gesture_in(vm: &Vm, bytes: &[u8]) -> Result<(), String> {
    enter_js();
    let result = vm.context.with(|ctx| {
        let array = TypedArray::new(ctx.clone(), bytes.to_vec())?;
        let dispatch: Function = ctx.globals().get("__bubbleGesture")?;
        dispatch.call::<_, ()>((array,))?;
        pump(&ctx);
        Ok(())
    });
    leave_js();
    result.map_err(|err: rquickjs::Error| format_eval(vm, err))
}

fn edit_in(vm: &Vm, bytes: &[u8]) -> Result<(), String> {
    enter_js();
    let result = vm.context.with(|ctx| {
        let array = TypedArray::new(ctx.clone(), bytes.to_vec())?;
        let dispatch: Function = ctx.globals().get("__bubbleEdit")?;
        dispatch.call::<_, ()>((array,))?;
        pump(&ctx);
        Ok(())
    });
    leave_js();
    result.map_err(|err: rquickjs::Error| format_eval(vm, err))
}

fn settle_in(vm: &Vm, id: u32, ok: bool, payload: &str) -> Result<(), String> {
    enter_js();
    let result = vm.context.with(|ctx| {
        let settle: Function = ctx.globals().get("__settleHost")?;
        settle.call::<_, ()>((id, ok, payload))?;
        pump(&ctx);
        Ok(())
    });
    leave_js();
    result.map_err(|err: rquickjs::Error| format_eval(vm, err))
}

fn format_eval(vm: &Vm, err: rquickjs::Error) -> String {
    if err.is_exception() {
        vm.context.with(|ctx| {
            let value = ctx.catch();
            let text = value_string(&ctx, value);
            if text.is_empty() {
                "exception".into()
            } else {
                text
            }
        })
    } else {
        err.to_string()
    }
}

fn finish_promise(ctx: &Ctx, promise: &rquickjs::Promise) -> rquickjs::Result<()> {
    for _ in 0..128 {
        if promise.state() != rquickjs::promise::PromiseState::Pending {
            break;
        }
        if !ctx.execute_pending_job() {
            break;
        }
    }
    match promise.result::<JsValue>() {
        None => Err(rquickjs::Error::new_from_js("promise", "settled")),
        Some(Ok(_)) => Ok(()),
        Some(Err(err)) => Err(err),
    }
}

fn pump(ctx: &Ctx) {
    for _ in 0..64 {
        if !ctx.execute_pending_job() {
            break;
        }
    }
}

fn value_string(ctx: &Ctx, value: JsValue) -> String {
    if value.is_undefined() {
        return "undefined".into();
    }
    if value.is_null() {
        return "null".into();
    }
    if let Some(text) = value.as_string() {
        return text.to_string().unwrap_or_default();
    }
    if let Some(int) = value.as_int() {
        return int.to_string();
    }
    if let Some(number) = value.as_float() {
        return number.to_string();
    }
    if let Some(flag) = value.as_bool() {
        return flag.to_string();
    }
    if let Some(object) = value.as_object() {
        let name = object.get::<_, String>("name").unwrap_or_default();
        let message = object.get::<_, String>("message").unwrap_or_default();
        let stack = object.get::<_, String>("stack").unwrap_or_default();
        if !name.is_empty() || !message.is_empty() {
            return format!("{name}: {message}\n{stack}");
        }
    }
    let _ = ctx;
    "value".into()
}

fn bind_api(ctx: &Ctx) -> rquickjs::Result<()> {
    let api = Object::new(ctx.clone())?;
    api.set("boot", Function::new(ctx.clone(), api_boot)?)?;
    api.set("spawn_props", Function::new(ctx.clone(), api_spawn_props)?)?;
    api.set(
        "deferred_props",
        Function::new(ctx.clone(), api_deferred_props)?,
    )?;
    api.set("error", Function::new(ctx.clone(), api_error)?)?;
    api.set(
        "signal_alloc",
        Function::new(ctx.clone(), api_signal_alloc)?,
    )?;
    api.set("signal_read", Function::new(ctx.clone(), api_signal_read)?)?;
    api.set(
        "signal_notify",
        Function::new(ctx.clone(), api_signal_notify)?,
    )?;
    api.set("alloc_node", Function::new(ctx.clone(), api_alloc_node)?)?;
    api.set("mount_node", Function::new(ctx.clone(), api_mount_node)?)?;
    api.set(
        "dispose_node",
        Function::new(ctx.clone(), api_dispose_node)?,
    )?;
    api.set("on_cleanup", Function::new(ctx.clone(), api_on_cleanup)?)?;
    api.set("run_init", Function::new(ctx.clone(), api_run_init)?)?;
    api.set("run_updates", Function::new(ctx.clone(), api_run_updates)?)?;
    api.set("spawn", Function::new(ctx.clone(), api_spawn)?)?;
    api.set("spawn_at", Function::new(ctx.clone(), api_spawn_at)?)?;
    api.set("dispose", Function::new(ctx.clone(), api_dispose)?)?;
    api.set("move_node", Function::new(ctx.clone(), api_move_node)?)?;
    api.set("set_prop", Function::new(ctx.clone(), api_set_prop)?)?;
    api.set(
        "queue_text_utf8",
        Function::new(ctx.clone(), api_queue_text)?,
    )?;
    api.set("bind_event", Function::new(ctx.clone(), api_bind)?)?;
    api.set("focus", Function::new(ctx.clone(), api_focus)?)?;
    api.set(
        "set_editing_value",
        Function::new(ctx.clone(), api_set_editing)?,
    )?;
    api.set(
        "spawn_gesture",
        Function::new(ctx.clone(), api_spawn_gesture)?,
    )?;
    api.set(
        "set_gesture_prop",
        Function::new(ctx.clone(), api_set_gesture_prop)?,
    )?;
    api.set(
        "bind_gesture_event",
        Function::new(ctx.clone(), api_bind_gesture_event)?,
    )?;
    api.set(
        "dispose_gesture",
        Function::new(ctx.clone(), api_dispose_gesture)?,
    )?;
    api.set(
        "move_gesture",
        Function::new(ctx.clone(), api_move_gesture)?,
    )?;
    api.set("commit", Function::new(ctx.clone(), api_commit)?)?;
    api.set("request", Function::new(ctx.clone(), api_request)?)?;
    api.set("schedule", Function::new(ctx.clone(), api_schedule)?)?;
    api.set("cancel", Function::new(ctx.clone(), api_cancel)?)?;
    ctx.globals().set("__bubble", api)?;
    Ok(())
}

fn with_turn<T>(body: impl FnOnce(&mut Turn) -> Result<T, String>) -> Result<T, String> {
    let active = ACTIVE.with(|cell| cell.get());
    if !active.is_null() {
        return body(unsafe { &mut *active });
    }
    TURN.with(|slot| {
        let mut slot = slot.borrow_mut();
        let turn = slot
            .as_mut()
            .ok_or_else(|| "shell is not mounted".to_string())?;
        let ptr = turn as *mut Turn;
        ACTIVE.with(|cell| cell.set(ptr));
        let result = body(unsafe { &mut *ptr });
        ACTIVE.with(|cell| cell.set(std::ptr::null_mut()));
        result
    })
}

fn set_api_error(message: String) {
    ERROR.with(|slot| *slot.borrow_mut() = message);
}

fn api_boot() -> i32 {
    let result = with_turn(|turn| {
        *turn = Turn::new();
        Ok(())
    });
    match result {
        Ok(()) => {
            if let Some(shared) = SHARED.with(|slot| slot.borrow().clone()) {
                if let Ok(mut queue) = shared.pending.lock() {
                    queue.clear();
                }
                shared.reset.store(true, Ordering::SeqCst);
            }
            0
        }
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

fn api_error() -> String {
    ERROR.with(|slot| slot.borrow().clone())
}

fn api_spawn_props(ctx: Ctx) -> rquickjs::Result<Array> {
    let array = Array::new(ctx)?;
    for (index, name) in SPAWN_PROPS.iter().enumerate() {
        array.set(index, *name)?;
    }
    Ok(array)
}

fn api_deferred_props(ctx: Ctx) -> rquickjs::Result<Array> {
    let array = Array::new(ctx)?;
    for (index, name) in DEFERRED_PROPS.iter().enumerate() {
        array.set(index, *name)?;
    }
    Ok(array)
}

fn api_signal_alloc() -> u32 {
    with_turn(|turn| Ok(turn.alloc_signal())).unwrap_or(0)
}

fn api_signal_read(ctx: Ctx, id: f64) -> i32 {
    graph_call(&ctx, |turn, host| {
        turn.read(id as u32, host)?;
        note_graph(turn)
    })
}

fn api_signal_notify(ctx: Ctx, id: f64) -> i32 {
    graph_call(&ctx, |turn, host| {
        turn.notify(id as u32, host)?;
        note_graph(turn)
    })
}

fn api_alloc_node(kind: f64) -> u32 {
    let kind = match kind as u32 {
        1 => ComputationKind::Memo,
        2 => ComputationKind::Effect,
        3 => ComputationKind::UserEffect,
        other => {
            set_api_error(format!("bad computation kind {other}"));
            return 0;
        }
    };
    match with_turn(|turn| turn.alloc_computation(kind)) {
        Ok(id) => id,
        Err(err) => {
            set_api_error(err);
            0
        }
    }
}

fn api_mount_node(ctx: Ctx, id: f64) -> i32 {
    graph_call(&ctx, |turn, host| {
        turn.mount_computation(id as u32, host)?;
        note_graph(turn)
    })
}

fn api_dispose_node(ctx: Ctx, id: f64) -> i32 {
    graph_call(&ctx, |turn, host| {
        turn.dispose_computation(id as u32, host);
        note_graph(turn)
    })
}

fn api_on_cleanup(id: f64) -> i32 {
    match with_turn(|turn| Ok(turn.on_cleanup(id as u32))) {
        Ok(true) => 0,
        _ => 1,
    }
}

fn api_run_init(ctx: Ctx) -> i32 {
    graph_call(&ctx, |turn, host| {
        turn.run_scheduled(true, host);
        note_graph(turn)
    })
}

fn api_run_updates(ctx: Ctx) -> i32 {
    graph_call(&ctx, |turn, host| {
        turn.run_scheduled(false, host);
        note_graph(turn)
    })
}

fn note_graph(turn: &mut Turn) -> Result<(), String> {
    match turn.take_graph_error() {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

fn graph_call(ctx: &Ctx, body: impl FnOnce(&mut Turn, &mut JsHost) -> Result<(), String>) -> i32 {
    let mut host = JsHost { ctx: ctx.clone() };
    match with_turn(|turn| body(turn, &mut host)) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

struct JsHost<'a> {
    ctx: Ctx<'a>,
}

impl Sched for JsHost<'_> {
    fn body(&mut self) {
        let Ok(func) = self.ctx.globals().get::<_, Function>("__bubbleBody") else {
            return;
        };
        let _ = func.call::<_, ()>(());
    }
    fn run(&mut self, id: u32) -> i32 {
        let Ok(func) = self.ctx.globals().get::<_, Function>("__bubbleRun") else {
            return -1;
        };
        func.call((id,)).unwrap_or(-1)
    }
    fn cleanup(&mut self, id: u32) {
        let Ok(func) = self.ctx.globals().get::<_, Function>("__bubbleCleanup") else {
            return;
        };
        let _ = func.call::<_, ()>((id,));
    }
}

fn read_spawn(parent: f64, spec: TypedArray<'_, u8>) -> Result<NodeDesc, String> {
    let bytes = unsafe { spec.as_slice() };
    let mut desc = NodeDesc::decode(bytes)?;
    desc.parent = parent as u32;
    Ok(desc)
}

fn api_spawn(parent: f64, spec: TypedArray<'_, u8>) -> u32 {
    let desc = match read_spawn(parent, spec) {
        Ok(desc) => desc,
        Err(err) => {
            set_api_error(err);
            return 0;
        }
    };
    match with_turn(|turn| turn.spawn(desc)) {
        Ok(handle) => handle,
        Err(err) => {
            set_api_error(err);
            0
        }
    }
}

fn api_spawn_at(parent: f64, index: f64, spec: TypedArray<'_, u8>) -> u32 {
    let desc = match read_spawn(parent, spec) {
        Ok(desc) => desc,
        Err(err) => {
            set_api_error(err);
            return 0;
        }
    };
    match with_turn(|turn| turn.spawn_at(desc, index as u32)) {
        Ok(handle) => handle,
        Err(err) => {
            set_api_error(err);
            0
        }
    }
}

fn api_dispose(handle: f64) -> i32 {
    match with_turn(|turn| {
        turn.dispose(handle as u32);
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

fn api_move_node(handle: f64, parent: f64, index: f64) -> i32 {
    match with_turn(|turn| {
        turn.move_node(handle as u32, parent as u32, index as u32);
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

fn api_set_prop(handle: f64, key: String, tag: f64, number: f64, text: String) -> i32 {
    let value = match tag as u32 {
        0 => Value::Null,
        1 => Value::Bool(number != 0.0),
        2 => Value::Int(number as i64),
        3 => Value::Float(number),
        4 => Value::String(text),
        other => {
            set_api_error(format!("unsupported prop tag {other}"));
            return 1;
        }
    };
    match with_turn(|turn| {
        turn.set_prop(handle as u32, key, value.clone());
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

fn api_queue_text(handle: f64, text: String) -> i32 {
    match with_turn(|turn| {
        turn.push(Op::SetText {
            handle: handle as u32,
            text,
        });
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

fn api_bind(handle: f64, handler: f64) -> i32 {
    match with_turn(|turn| {
        turn.bind_event(handle as u32, handler as u32);
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

fn api_focus(handle: f64) -> i32 {
    match with_turn(|turn| {
        turn.push(Op::Focus {
            handle: handle as u32,
        });
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

fn api_set_editing(
    handle: f64,
    epoch: f64,
    value: String,
    sel_start: f64,
    sel_end: f64,
    cancel: f64,
) -> i32 {
    match with_turn(|turn| {
        turn.push(Op::SetEditingValue {
            handle: handle as u32,
            epoch: epoch as u64,
            value,
            sel_start: sel_start as u32,
            sel_end: sel_end as u32,
            cancel: cancel != 0.0,
        });
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

fn api_spawn_gesture(target: f64, kind: f64, index: f64) -> u32 {
    let Some(kind) = GestureKind::parse(kind as u32) else {
        set_api_error(format!("bad gesture kind {kind}"));
        return 0;
    };
    match with_turn(|turn| turn.spawn_gesture(target as u32, kind, index as u32)) {
        Ok(handle) => handle,
        Err(err) => {
            set_api_error(err);
            0
        }
    }
}

fn api_set_gesture_prop(handle: f64, key: String, tag: f64, number: f64, text: String) -> i32 {
    let value = match tag as u32 {
        0 => Value::Null,
        1 => Value::Bool(number != 0.0),
        2 => Value::Int(number as i64),
        3 => Value::Float(number),
        4 => Value::String(text),
        other => {
            set_api_error(format!("unsupported prop tag {other}"));
            return 1;
        }
    };
    match with_turn(|turn| {
        turn.set_gesture_prop(handle as u32, key, value.clone());
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

fn api_bind_gesture_event(handle: f64, slot: f64, handler: f64) -> i32 {
    if !(1..=9).contains(&(slot as u32)) {
        set_api_error(format!("bad gesture event slot {slot}"));
        return 1;
    }
    match with_turn(|turn| {
        turn.bind_gesture_event(handle as u32, slot as u8, handler as u32);
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

fn api_dispose_gesture(handle: f64) -> i32 {
    match with_turn(|turn| {
        turn.dispose_gesture(handle as u32);
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

fn api_move_gesture(handle: f64, target: f64, index: f64) -> i32 {
    match with_turn(|turn| {
        turn.move_gesture(handle as u32, target as u32, index as u32);
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

fn api_commit() -> i32 {
    let result = with_turn(|turn| turn.commit());
    match result {
        Ok(bytes) => {
            let len = bytes.len();
            let Some(shared) = SHARED.with(|slot| slot.borrow().clone()) else {
                set_api_error("shell is not mounted".into());
                return 0;
            };
            let pushed = match shared.pending.lock() {
                Ok(mut queue) => {
                    queue.push_back(bytes);
                    Ok(())
                }
                Err(_) => Err("op queue poisoned".to_string()),
            };
            match pushed {
                Ok(()) => len as i32,
                Err(err) => {
                    set_api_error(err);
                    0
                }
            }
        }
        Err(err) => {
            set_api_error(err);
            0
        }
    }
}

fn api_request(method: String, params: String) -> u32 {
    let id = NEXT_REQUEST.with(|slot| {
        let mut next = slot.borrow_mut();
        let id = *next;
        *next += 1;
        id
    });
    REQUESTS.with(|slot| {
        if let Some(queue) = slot.borrow().as_ref() {
            if let Ok(mut queue) = queue.lock() {
                queue.push_back(crate::HostRequest { id, method, params });
            }
        }
    });
    id
}

fn api_schedule(id: f64, ms: f64) {
    let delay = Duration::from_millis(ms.max(0.0) as u64);
    TIMERS.with(|slot| {
        slot.borrow_mut().push(Timer {
            id: id as u32,
            at: Instant::now() + delay,
        });
    });
}

fn api_cancel(id: f64) {
    let id = id as u32;
    TIMERS.with(|slot| slot.borrow_mut().retain(|timer| timer.id != id));
}

fn check_manifest(manifest: &str, entry: &[u8]) -> Result<(), String> {
    let parsed: ManifestFile = serde_json::from_str(manifest).map_err(|err| err.to_string())?;
    if parsed.entry.is_empty() {
        return Err("manifest entry is empty".into());
    }
    let file = parsed
        .files
        .iter()
        .find(|file| file.path == parsed.entry)
        .ok_or_else(|| format!("manifest entry {} is not listed", parsed.entry))?;
    validate_bundle_hashes(manifest, &[(&file.path, entry)])
}

#[derive(Deserialize)]
struct ManifestFile {
    entry: String,
    files: Vec<ManifestPath>,
}

#[derive(Deserialize)]
struct ManifestPath {
    path: String,
}

const PRELUDE: &str = r#"
const __hostPending = new Map();
globalThis.bubbleHost = {
  call(method, params) {
    return new Promise((resolve, reject) => {
      const id = globalThis.__bubble.request(method, JSON.stringify(params ?? null));
      __hostPending.set(id, { resolve, reject });
    });
  }
};
globalThis.__settleHost = (id, ok, payload) => {
  const slot = __hostPending.get(id);
  __hostPending.delete(id);
  if (!slot) return;
  let value = payload;
  try { value = JSON.parse(payload); } catch (err) {}
  if (ok) slot.resolve(value);
  else slot.reject(new Error(String(payload)));
};
const __timers = new Map();
let __nextTimer = 1;
globalThis.setTimeout = (fn, ms) => {
  const id = __nextTimer++;
  __timers.set(id, fn);
  globalThis.__bubble.schedule(id, ms ?? 0);
  return id;
};
globalThis.clearTimeout = (id) => {
  __timers.delete(id);
  globalThis.__bubble.cancel(id);
};
globalThis.__fireTimer = (id) => {
  const fn = __timers.get(id);
  __timers.delete(id);
  if (typeof fn === "function") fn();
};
"#;
