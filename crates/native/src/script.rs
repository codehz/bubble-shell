//! quickjs-oxide (pure Rust, zero `unsafe`) on a private thread. Script calls
//! only touch the signal graph and the op buffer; the tree is owned by the
//! host/frame thread, so a dead loop cannot stall layout.

use std::cell::{Cell, RefCell};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use bubble_core::{
    validate_bundle_hashes, AnimSpec, ComputationKind, GestureKind, Kind, NodeDesc, Op, Sched,
    TransitionSpec, Turn, Value,
};
use quickjs_oxide::engine::api::core::{
    CallableRef, Context, HeapLimits, Host, Js, JsString, ModuleImportAttributes, ModuleLoader,
    ModuleScope, ModuleSource, NativeErrorKind, ObjectRef, PromiseState, PropertyKey, Runtime,
    RuntimeError, RuntimeOptions, Scope, TypedArrayKind, Value as JsValue,
};
use serde::Deserialize;

use crate::Shared;

const MEMORY_LIMIT: usize = 64 * 1024 * 1024;
const INTERRUPT_MS: u64 = 1000;
const RUNTIME_JS: &str = include_str!("../js/runtime.js");
/// Both author-facing and compiler-facing specifiers resolve to this module id,
/// so the runtime is evaluated exactly once per context.
const RUNTIME_ID: &str = "bubble-shell";

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
    /// Encoded `BSVE` settled-variable batch. No epoch: variables belong to
    /// the current mount and settle events are dropped on reset.
    Vars {
        bytes: Vec<u8>,
    },
    /// Encoded `BSUE` UI→JS call batch. No epoch, like `Vars`.
    UiCalls {
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
    Emit {
        name: String,
        payload: String,
    },
    Wake,
    Shutdown,
}

pub fn start(shared: Arc<Shared>) -> (Sender<Cmd>, thread::JoinHandle<()>) {
    shared.stop_watch.store(false, Ordering::Relaxed);
    shared.interrupt.store(false, Ordering::Relaxed);
    let watch = Arc::clone(&shared);
    thread::spawn(move || watchdog(watch));
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = thread::Builder::new()
        .name("bubble-script".into())
        .spawn(move || script_main(rx, shared))
        .expect("spawn script thread");
    (tx, handle)
}

pub fn stop(shared: &Shared) {
    shared.stop_watch.store(true, Ordering::Relaxed);
    shared.interrupt.store(true, Ordering::Relaxed);
}

fn mono_ms() -> u64 {
    MONO_BASE.elapsed().as_millis() as u64
}

fn flags() -> Option<Arc<Shared>> {
    SHARED.with(|slot| slot.borrow().clone())
}

fn watchdog(shared: Arc<Shared>) {
    while !shared.stop_watch.load(Ordering::Relaxed) {
        let started = shared.js_since_ms.load(Ordering::Relaxed);
        if started != 0 && mono_ms().saturating_sub(started) > INTERRUPT_MS {
            shared.interrupt.store(true, Ordering::Relaxed);
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn enter_js() {
    if let Some(shared) = flags() {
        shared.interrupt.store(false, Ordering::Relaxed);
        shared.js_since_ms.store(mono_ms(), Ordering::Relaxed);
    }
}

fn leave_js() {
    if let Some(shared) = flags() {
        shared.js_since_ms.store(0, Ordering::Relaxed);
    }
}

struct Vm {
    context: Context,
    // `Context` clones the runtime's `Rc`, so this only pins the lifetime;
    // both drop together.
    runtime: Runtime,
}

struct Session {
    shared: Arc<Shared>,
    vm: Option<Vm>,
    bundle: Option<(String, Vec<u8>)>,
    early_events: Vec<(String, String)>,
}

fn script_main(rx: Receiver<Cmd>, shared: Arc<Shared>) {
    TURN.with(|slot| *slot.borrow_mut() = Some(Turn::new()));
    SHARED.with(|slot| *slot.borrow_mut() = Some(Arc::clone(&shared)));
    REQUESTS.with(|slot| *slot.borrow_mut() = Some(Arc::clone(&shared.requests)));
    let mut session = Session {
        shared,
        vm: None,
        bundle: None,
        early_events: Vec::new(),
    };
    loop {
        let fired = session.fire_due();
        if let Err(err) = fired {
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
            Cmd::Vars { bytes } => {
                if self.dispatch_vars(&bytes).is_ok() {
                    self.shared.generation.fetch_add(1, Ordering::Relaxed);
                }
                true
            }
            Cmd::UiCalls { bytes } => {
                if self.dispatch_ui_calls(&bytes).is_ok() {
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
            Cmd::Emit { name, payload } => {
                if let Err(err) = self.emit(&name, &payload) {
                    self.note_fault(&err);
                }
                true
            }
        }
    }

    fn load_bundle(&mut self, manifest: String, entry: Vec<u8>) -> Result<(), String> {
        check_manifest(&manifest, &entry)?;
        self.shared.bind_storage(&manifest);
        self.bundle = Some((manifest, entry));
        self.boot_vm()?;
        let pending = std::mem::take(&mut self.early_events);
        if let Some(vm) = self.vm.as_mut() {
            for (name, payload) in pending {
                host_emit(vm, &name, &payload)?;
            }
        }
        self.shared.set_status("running");
        Ok(())
    }

    fn emit(&mut self, name: &str, payload: &str) -> Result<(), String> {
        let Some(vm) = self.vm.as_mut() else {
            self.early_events
                .push((name.to_string(), payload.to_string()));
            return Ok(());
        };
        host_emit(vm, name, payload)
    }

    fn boot_vm(&mut self) -> Result<(), String> {
        self.shared.input_epoch.fetch_add(1, Ordering::SeqCst);
        self.shared.has_back_handler.store(false, Ordering::Relaxed);
        let Some((manifest, entry)) = self.bundle.clone() else {
            return Err("no bundle loaded".into());
        };
        let source =
            String::from_utf8(entry).map_err(|_| "bundle entry is not utf-8".to_string())?;
        let entry_name = serde_json::from_str::<serde_json::Value>(&manifest)
            .ok()
            .and_then(|value| {
                value
                    .get("entry")
                    .and_then(|entry| entry.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "main.js".to_string());
        self.vm = Some(make_vm(&self.shared)?);
        let loaded = {
            let vm = self.vm.as_mut().expect("vm");
            guarded(|| install_and_run(vm, &source, &entry_name))
        };
        match loaded {
            Ok(()) => Ok(()),
            Err(err) => {
                self.shared.has_back_handler.store(false, Ordering::Relaxed);
                self.vm = None;
                Err(err)
            }
        }
    }

    fn eval_user(&mut self, source: &str) -> Result<String, String> {
        let Some(vm) = self.vm.as_mut() else {
            return Err("bundle is not loaded".into());
        };
        match guarded(|| eval_in(vm, source)) {
            Ok(text) => Ok(text),
            Err(err) => {
                if fatal(&err) {
                    self.recover()?;
                    Err(if self.shared.interrupt.load(Ordering::Relaxed) {
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
        let Some(vm) = self.vm.as_mut() else {
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
        let Some(vm) = self.vm.as_mut() else {
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

    fn dispatch_vars(&mut self, bytes: &[u8]) -> Result<(), String> {
        let Some(vm) = self.vm.as_mut() else {
            return Ok(());
        };
        match guarded(|| vars_in(vm, bytes)) {
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

    fn dispatch_ui_calls(&mut self, bytes: &[u8]) -> Result<(), String> {
        let Some(vm) = self.vm.as_mut() else {
            return Ok(());
        };
        match guarded(|| ui_calls_in(vm, bytes)) {
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
        let Some(vm) = self.vm.as_mut() else {
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

    fn fire_due(&mut self) -> Result<(), String> {
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
        let Some(vm) = self.vm.as_mut() else {
            return Ok(());
        };
        guarded(|| {
            enter_js();
            let result = (|| -> Result<(), RuntimeError> {
                for id in due {
                    let fire = js_callable(&mut vm.context, "__fireTimer")?;
                    vm.context.call(
                        &fire,
                        JsValue::Undefined,
                        &[JsValue::number(f64::from(id))],
                    )?;
                }
                pump(vm)
            })();
            leave_js();
            result.map_err(|err| err.to_string())
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

/// A fault that leaves the VM unsafe to reuse: an interrupt, a memory
/// allocation failure, or a host-function panic (the runtime is poisoned).
/// All rebuild the context and remount the bundle.
fn fatal(err: &str) -> bool {
    flags().is_some_and(|shared| shared.interrupt.load(Ordering::Relaxed))
        || err.contains("interrupted")
        || err.to_ascii_lowercase().contains("out of memory")
        || err.contains("poisoned")
        || err.contains("panicked")
}

fn guarded<T>(body: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)) {
        Ok(result) => result,
        Err(_) => Err("script thread panicked".into()),
    }
}

/// Resolve host modules and reject anything a release bundle must not import.
/// Errors are values to throw, made through the loader's scope.
#[derive(Debug, Default)]
struct BubbleLoader;

impl ModuleLoader for BubbleLoader {
    fn normalize(
        &mut self,
        scope: &mut ModuleScope<'_>,
        _referrer: &str,
        specifier: &str,
        _attributes: &ModuleImportAttributes,
    ) -> Result<String, JsValue> {
        if specifier.starts_with('.') || specifier.starts_with('/') {
            return Err(loader_error(
                scope,
                "release bundles must not contain relative imports",
            ));
        }
        match specifier {
            "bubble-shell" | "bubble:rt" => Ok(RUNTIME_ID.to_string()),
            other => Ok(other.to_string()),
        }
    }

    fn load(
        &mut self,
        scope: &mut ModuleScope<'_>,
        name: &str,
        _attributes: &ModuleImportAttributes,
    ) -> Result<ModuleSource, JsValue> {
        if name == RUNTIME_ID {
            Ok(ModuleSource::source(RUNTIME_JS))
        } else {
            Err(loader_error(scope, "unknown native module"))
        }
    }
}

/// The error value a loader throws.
fn loader_error(scope: &mut ModuleScope<'_>, message: &str) -> JsValue {
    scope
        .new_native_error(NativeErrorKind::Reference, message)
        .unwrap_or(JsValue::Undefined)
}

fn make_vm(shared: &Arc<Shared>) -> Result<Vm, String> {
    let options = RuntimeOptions::from(HeapLimits {
        max_bytes: MEMORY_LIMIT,
    });
    let runtime = Runtime::with_host(
        BubbleHost {
            interrupt: Arc::clone(shared),
        },
        options,
    );
    runtime
        .set_module_loader(BubbleLoader)
        .map_err(|err| err.to_string())?;
    let context = runtime.context();
    Ok(Vm { context, runtime })
}

/// The engine's host: the watchdog interrupts through the shared flag, so
/// `enter_js`'s clear and the watchdog's set drive it exactly as before.
struct BubbleHost {
    interrupt: Arc<Shared>,
}

impl Host for BubbleHost {
    fn interrupt(&mut self) -> bool {
        self.interrupt.interrupt.load(Ordering::Relaxed)
    }
}

fn install_and_run(vm: &mut Vm, source: &str, entry: &str) -> Result<(), String> {
    enter_js();
    let result = run_install(vm, source, entry);
    leave_js();
    result.map_err(|err| format_eval(vm, err))
}

fn run_install(vm: &mut Vm, source: &str, entry: &str) -> Result<(), RuntimeError> {
    let promise = {
        let ctx = &mut vm.context;
        bind_api(ctx)?;
        let runtime_module =
            ctx.compile_module_bytes_with_filename(RUNTIME_JS.as_bytes(), RUNTIME_ID)?;
        ctx.execute_module(&runtime_module)?
    };
    finish_promise(vm, promise)?;
    let promise = {
        let ctx = &mut vm.context;
        let set_api = js_callable(ctx, "__bubbleSetApi")?;
        let api_key = PropertyKey::from_js_string(JsString::from_static("__bubble"));
        let global = ctx.global_object()?;
        let api = match ctx.get_property(&global, &api_key)? {
            JsValue::Object(api) => api,
            _ => return Err(RuntimeError::Invariant("__bubble is an object")),
        };
        ctx.call(&set_api, JsValue::Undefined, &[JsValue::Object(api)])?;
        ctx.eval(PRELUDE)?;
        let params = SHARED.with(|slot| {
            slot.borrow()
                .as_ref()
                .and_then(|shared| shared.params.lock().ok().map(|params| params.clone()))
        });
        if let Some(params) = params {
            ctx.eval(&format!("globalThis.__bubbleHostParams = {params};"))?;
        }
        let main = ctx.compile_module_bytes_with_filename(source.as_bytes(), entry)?;
        ctx.execute_module(&main)?
    };
    finish_promise(vm, promise)?;
    let promise = {
        let ctx = &mut vm.context;
        let entry = serde_json::to_string(entry).expect("an entry name is a JSON string");
        ctx.eval(&format!(
            r#"(async () => {{
  const entry = await import({entry});
  if (entry.default === undefined) throw new Error("entry module has no default export");
  const runtime = await import("bubble-shell");
  runtime.mount(entry.default);
}})();"#
        ))?
    };
    finish_promise(vm, promise)?;
    pump(vm)
}

fn eval_in(vm: &mut Vm, source: &str) -> Result<String, String> {
    enter_js();
    let result = (|| -> Result<String, RuntimeError> {
        let value = vm.context.eval(source)?;
        pump(vm)?;
        Ok(value_string(&mut vm.context, &value))
    })();
    leave_js();
    result.map_err(|err| format_eval(vm, err))
}

fn gesture_in(vm: &mut Vm, bytes: &[u8]) -> Result<(), String> {
    enter_js();
    let result = (|| -> Result<(), RuntimeError> {
        let array = byte_array_value(vm, bytes)?;
        let dispatch = js_callable(&mut vm.context, "__bubbleGesture")?;
        vm.context.call(&dispatch, JsValue::Undefined, &[array])?;
        pump(vm)
    })();
    leave_js();
    result.map_err(|err| format_eval(vm, err))
}

fn edit_in(vm: &mut Vm, bytes: &[u8]) -> Result<(), String> {
    enter_js();
    let result = (|| -> Result<(), RuntimeError> {
        let array = byte_array_value(vm, bytes)?;
        let dispatch = js_callable(&mut vm.context, "__bubbleEdit")?;
        vm.context.call(&dispatch, JsValue::Undefined, &[array])?;
        pump(vm)
    })();
    leave_js();
    result.map_err(|err| format_eval(vm, err))
}

fn vars_in(vm: &mut Vm, bytes: &[u8]) -> Result<(), String> {
    enter_js();
    let result = (|| -> Result<(), RuntimeError> {
        let array = byte_array_value(vm, bytes)?;
        let dispatch = js_callable(&mut vm.context, "__bubbleVar")?;
        vm.context.call(&dispatch, JsValue::Undefined, &[array])?;
        pump(vm)
    })();
    leave_js();
    result.map_err(|err| format_eval(vm, err))
}

fn ui_calls_in(vm: &mut Vm, bytes: &[u8]) -> Result<(), String> {
    enter_js();
    let result = (|| -> Result<(), RuntimeError> {
        let array = byte_array_value(vm, bytes)?;
        let dispatch = js_callable(&mut vm.context, "__bubbleUiCalls")?;
        vm.context.call(&dispatch, JsValue::Undefined, &[array])?;
        pump(vm)
    })();
    leave_js();
    result.map_err(|err| format_eval(vm, err))
}

fn settle_in(vm: &mut Vm, id: u32, ok: bool, payload: &str) -> Result<(), String> {
    enter_js();
    let result = (|| -> Result<(), RuntimeError> {
        let settle = js_callable(&mut vm.context, "__settleHost")?;
        vm.context.call(
            &settle,
            JsValue::Undefined,
            &[
                JsValue::number(f64::from(id)),
                JsValue::Bool(ok),
                JsValue::String(JsString::try_from_utf8(payload)?),
            ],
        )?;
        pump(vm)
    })();
    leave_js();
    result.map_err(|err| format_eval(vm, err))
}

fn format_eval(vm: &mut Vm, err: RuntimeError) -> String {
    match err {
        RuntimeError::Exception(value) => {
            let text = value_string(&mut vm.context, &value);
            if text.is_empty() {
                "exception".into()
            } else {
                text
            }
        }
        other => other.to_string(),
    }
}

/// A module's evaluation promise: drain the queue while it stays pending,
/// then report fulfillment or the rejection reason.
fn finish_promise(vm: &Vm, promise: JsValue) -> Result<(), RuntimeError> {
    let JsValue::Object(object) = &promise else {
        return Ok(());
    };
    for _ in 0..128 {
        let snapshot = vm
            .runtime
            .promise_snapshot(object)?
            .ok_or(RuntimeError::Invariant("not a promise"))?;
        if snapshot.state() != PromiseState::Pending {
            break;
        }
        if !vm.runtime.run_pending_job()?.executed() {
            break;
        }
    }
    let snapshot = vm
        .runtime
        .promise_snapshot(object)?
        .ok_or(RuntimeError::Invariant("not a promise"))?;
    match snapshot.state() {
        PromiseState::Fulfilled => Ok(()),
        PromiseState::Rejected => Err(RuntimeError::Exception(snapshot.result().clone())),
        PromiseState::Pending => Err(RuntimeError::Invariant("promise did not settle")),
    }
}

fn pump(vm: &Vm) -> Result<(), RuntimeError> {
    for _ in 0..64 {
        if !vm.runtime.run_pending_job()?.executed() {
            break;
        }
    }
    Ok(())
}

fn value_string(js: &mut impl Js, value: &JsValue) -> String {
    match value {
        JsValue::Undefined => return "undefined".into(),
        JsValue::Null => return "null".into(),
        JsValue::String(text) => return text.to_utf8_lossy(),
        JsValue::Int(int) => return int.to_string(),
        JsValue::Float(number) => return number.to_string(),
        JsValue::Bool(flag) => return flag.to_string(),
        JsValue::Object(object) => {
            let name = property_string(js, object, "name");
            let message = property_string(js, object, "message");
            let stack = property_string(js, object, "stack");
            if !name.is_empty() || !message.is_empty() {
                return format!("{name}: {message}\n{stack}");
            }
        }
        _ => {}
    }
    "value".into()
}

fn property_string(js: &mut impl Js, object: &ObjectRef, key: &'static str) -> String {
    let key = PropertyKey::from_js_string(JsString::from_static(key));
    match js.get_property(object, &key) {
        Ok(JsValue::String(text)) => text.to_utf8_lossy(),
        _ => String::new(),
    }
}

/// A `Uint8Array` view over a copy of `bytes`, as a JS argument.
fn byte_array_value(vm: &mut Vm, bytes: &[u8]) -> Result<JsValue, RuntimeError> {
    let buffer = vm.context.new_array_buffer(bytes)?;
    let array = vm
        .context
        .new_typed_array(TypedArrayKind::Uint8, &buffer, 0, None)?;
    Ok(JsValue::Object(array))
}

/// Declare one `__bubble` API function. Param list mirrors the closure's
/// typed JS arguments; the trailing call names the `api_*` helper and its
/// argument list. Variants: plain value return, `scope` (scope first arg),
/// `void` (unit return), `string` (returned String wrapped as JS string).
macro_rules! def {
    ($ctx:expr, $api:expr, $name:literal, $arity:expr, ($($p:ident : $t:ty),*) => $fn:ident($($a:expr),*)) => {
        $ctx.define_function($api, $name, $arity, |_: &mut Scope<'_>, $($p : $t),*| {
            Ok(JsValue::number(f64::from($fn($($a),*))))
        })?;
    };
    ($ctx:expr, $api:expr, $name:literal, $arity:expr, scope($($p:ident : $t:ty),*) => $fn:ident($($a:expr),*)) => {
        $ctx.define_function($api, $name, $arity, |scope: &mut Scope<'_>, $($p : $t),*| {
            Ok(JsValue::number(f64::from($fn(scope, $($a),*))))
        })?;
    };
    ($ctx:expr, $api:expr, $name:literal, $arity:expr, void($($p:ident : $t:ty),*) => $fn:ident($($a:expr),*)) => {
        $ctx.define_function($api, $name, $arity, |_: &mut Scope<'_>, $($p : $t),*| {
            $fn($($a),*);
            Ok(())
        })?;
    };
    ($ctx:expr, $api:expr, $name:literal, $arity:expr, string($($p:ident : $t:ty),*) => $fn:ident($($a:expr),*)) => {
        $ctx.define_function($api, $name, $arity, |_: &mut Scope<'_>, $($p : $t),*| {
            Ok(JsValue::String(JsString::try_from_utf8(&$fn($($a),*))?))
        })?;
    };
}

fn bind_api(ctx: &Context) -> Result<(), RuntimeError> {
    let api = ctx.new_object()?;
    def!(ctx, &api, "boot", 0, () => api_boot());
    def!(ctx, &api, "error", 0, string() => api_error());
    def!(ctx, &api, "signal_alloc", 0, () => api_signal_alloc());
    def!(ctx, &api, "signal_read", 1, scope(id: f64) => api_signal_read(id));
    def!(ctx, &api, "signal_notify", 1, scope(id: f64) => api_signal_notify(id));
    def!(ctx, &api, "alloc_node", 1, (kind: f64) => api_alloc_node(kind));
    def!(ctx, &api, "mount_node", 1, scope(id: f64) => api_mount_node(id));
    def!(ctx, &api, "dispose_node", 1, scope(id: f64) => api_dispose_node(id));
    def!(ctx, &api, "on_cleanup", 1, (id: f64) => api_on_cleanup(id));
    def!(ctx, &api, "run_init", 0, scope() => api_run_init());
    def!(ctx, &api, "run_updates", 0, scope() => api_run_updates());
    def!(ctx, &api, "spawn", 3, (parent: f64, kind: f64, text: String) => api_spawn(parent, kind, text));
    def!(ctx, &api, "spawn_at", 4, (parent: f64, index: f64, kind: f64, text: String) => api_spawn_at(parent, index, kind, text));
    def!(ctx, &api, "dispose", 1, (handle: f64) => api_dispose(handle));
    def!(ctx, &api, "move_node", 3, (handle: f64, parent: f64, index: f64) => api_move_node(handle, parent, index));
    def!(ctx, &api, "set_prop", 5, (handle: f64, key: String, tag: f64, number: f64, text: String) => api_set_prop(handle, key, tag, number, text));
    def!(ctx, &api, "queue_text", 2, (handle: f64, text: String) => api_queue_text(handle, text));
    def!(ctx, &api, "bind_event", 2, (handle: f64, handler: f64) => api_bind(handle, handler));
    def!(ctx, &api, "focus", 1, (handle: f64) => api_focus(handle));
    def!(ctx, &api, "set_editing_value", 6, (handle: f64, epoch: f64, value: String, sel_start: f64, sel_end: f64, cancel: f64) => api_set_editing(handle, epoch, value, sel_start, sel_end, cancel));
    def!(ctx, &api, "spawn_gesture", 3, (target: f64, kind: f64, index: f64) => api_spawn_gesture(target, kind, index));
    def!(ctx, &api, "set_gesture_prop", 5, (handle: f64, key: String, tag: f64, number: f64, text: String) => api_set_gesture_prop(handle, key, tag, number, text));
    def!(ctx, &api, "bind_gesture_event", 3, (handle: f64, slot: f64, handler: f64) => api_bind_gesture_event(handle, slot, handler));
    def!(ctx, &api, "dispose_gesture", 1, (handle: f64) => api_dispose_gesture(handle));
    def!(ctx, &api, "move_gesture", 3, (handle: f64, target: f64, index: f64) => api_move_gesture(handle, target, index));
    def!(ctx, &api, "spawn_scrollbar", 2, (target: f64, index: f64) => api_spawn_scrollbar(target, index));
    def!(ctx, &api, "set_scrollbar_prop", 5, (handle: f64, key: String, tag: f64, number: f64, text: String) => api_set_scrollbar_prop(handle, key, tag, number, text));
    def!(ctx, &api, "dispose_scrollbar", 1, (handle: f64) => api_dispose_scrollbar(handle));
    def!(ctx, &api, "move_scrollbar", 3, (handle: f64, target: f64, index: f64) => api_move_scrollbar(handle, target, index));
    def!(ctx, &api, "spawn_decoration", 3, (target: f64, kind: f64, index: f64) => api_spawn_decoration(target, kind, index));
    def!(ctx, &api, "set_decoration_prop", 5, (handle: f64, key: String, tag: f64, number: f64, text: String) => api_set_decoration_prop(handle, key, tag, number, text));
    def!(ctx, &api, "set_decoration_prop_program", 3, scope(id: f64, key: String, bytes: JsValue) => api_set_decoration_prop_program(id, key, bytes));
    def!(ctx, &api, "move_decoration", 3, (id: f64, target: f64, index: f64) => api_move_decoration(id, target, index));
    def!(ctx, &api, "dispose_decoration", 1, (id: f64) => api_dispose_decoration(id));
    def!(ctx, &api, "create_var", 3, (source: f64, initial_tag: f64, initial_number: f64) => api_create_var(source, initial_tag, initial_number));
    def!(ctx, &api, "bind_var", 2, (var: f64, target: f64) => api_bind_var(var, target));
    def!(ctx, &api, "set_var", 7, (var: f64, tag: f64, number: f64, anim: f64, a: f64, b: f64, c: f64) => api_set_var(var, tag, number, anim, a, b, c));
    def!(ctx, &api, "set_style_transition", 5, (handle: f64, slot: f64, enabled: f64, duration: f64, easing: f64) => api_set_style_transition(handle, slot, enabled, duration, easing));
    def!(ctx, &api, "settle_var", 1, (var: f64) => api_settle_var(var));
    def!(ctx, &api, "dispose_var", 1, (var: f64) => api_dispose_var(var));
    def!(ctx, &api, "define_ui_function", 3, scope(params: f64, bytes: JsValue, reuse: f64) => api_define_ui_function(params, bytes, reuse));
    def!(ctx, &api, "bind_gesture_program", 3, (handle: f64, slot: f64, function: f64) => api_bind_gesture_program(handle, slot, function));
    def!(ctx, &api, "run_ui_function", 2, scope(function: f64, bytes: JsValue) => api_run_ui_function(function, bytes));
    def!(ctx, &api, "cancel_anim", 1, (var: f64) => api_cancel_anim(var));
    def!(ctx, &api, "spawn_style_decl", 3, (target: f64, component: f64, index: f64) => api_spawn_style_decl(target, component, index));
    def!(ctx, &api, "set_style_prop", 5, (decl: f64, key: String, tag: f64, number: f64, text: String) => api_set_style_prop(decl, key, tag, number, text));
    def!(ctx, &api, "set_style_prop_program", 3, scope(decl: f64, key: String, bytes: JsValue) => api_set_style_prop_program(decl, key, bytes));
    def!(ctx, &api, "move_style_decl", 3, (decl: f64, target: f64, index: f64) => api_move_style_decl(decl, target, index));
    def!(ctx, &api, "dispose_style_decl", 1, (decl: f64) => api_dispose_style_decl(decl));
    def!(ctx, &api, "commit", 0, () => api_commit());
    def!(ctx, &api, "request", 2, (method: String, params: String) => api_request(method, params));
    def!(ctx, &api, "storage_get", 1, string(key: String) => api_storage_get(key));
    def!(ctx, &api, "storage_set", 2, void(key: String, value: String) => api_storage_set(key, value));
    def!(ctx, &api, "storage_remove", 1, void(key: String) => api_storage_remove(key));
    def!(ctx, &api, "storage_keys", 0, string() => api_storage_keys());
    def!(ctx, &api, "schedule", 2, void(id: f64, ms: f64) => api_schedule(id, ms));
    def!(ctx, &api, "cancel", 1, void(id: f64) => api_cancel(id));
    def!(ctx, &api, "set_back_handler", 1, void(has: f64) => api_set_back_handler(has));
    let key = PropertyKey::from_js_string(JsString::from_static("__bubble"));
    let global = ctx.global_object()?;
    if !ctx.set_property(&global, &key, JsValue::Object(api))? {
        return Err(RuntimeError::Invariant("__bubble is writable"));
    }
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

fn api_signal_alloc() -> u32 {
    with_turn(|turn| Ok(turn.alloc_signal())).unwrap_or(0)
}

fn api_signal_read(scope: &mut Scope<'_>, id: f64) -> i32 {
    graph_call(scope, |turn, host| {
        turn.read(id as u32, host)?;
        note_graph(turn)
    })
}

fn api_signal_notify(scope: &mut Scope<'_>, id: f64) -> i32 {
    graph_call(scope, |turn, host| {
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

fn api_mount_node(scope: &mut Scope<'_>, id: f64) -> i32 {
    graph_call(scope, |turn, host| {
        turn.mount_computation(id as u32, host)?;
        note_graph(turn)
    })
}

fn api_dispose_node(scope: &mut Scope<'_>, id: f64) -> i32 {
    graph_call(scope, |turn, host| {
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

fn api_run_init(scope: &mut Scope<'_>) -> i32 {
    graph_call(scope, |turn, host| {
        turn.run_scheduled(true, host);
        note_graph(turn)
    })
}

fn api_run_updates(scope: &mut Scope<'_>) -> i32 {
    graph_call(scope, |turn, host| {
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

fn graph_call(
    scope: &mut Scope<'_>,
    body: impl FnOnce(&mut Turn, &mut JsHost<'_, '_>) -> Result<(), String>,
) -> i32 {
    let mut host = JsHost { scope };
    match with_turn(|turn| body(turn, &mut host)) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

struct JsHost<'a, 'js> {
    scope: &'a mut Scope<'js>,
}

impl Sched for JsHost<'_, '_> {
    fn body(&mut self) {
        if let Ok(func) = js_callable(self.scope, "__bubbleBody") {
            let _ = self.scope.call(&func, JsValue::Undefined, &[]);
        }
    }
    fn run(&mut self, id: u32) -> i32 {
        let Ok(func) = js_callable(self.scope, "__bubbleRun") else {
            return -1;
        };
        match self
            .scope
            .call(&func, JsValue::Undefined, &[JsValue::number(f64::from(id))])
        {
            Ok(value) => value.as_number().map_or(-1, |number| number as i32),
            Err(_) => -1,
        }
    }
    fn cleanup(&mut self, id: u32) {
        if let Ok(func) = js_callable(self.scope, "__bubbleCleanup") {
            let _ = self
                .scope
                .call(&func, JsValue::Undefined, &[JsValue::number(f64::from(id))]);
        }
    }
}

/// A function value on the global object, for `Js` implementors; an error
/// when missing or not callable, as `globals().get::<Function>` did.
fn js_callable(js: &mut impl Js, name: &'static str) -> Result<CallableRef, RuntimeError> {
    let global = js.global_object()?;
    let key = PropertyKey::from_js_string(JsString::from_static(name));
    match js.get_property(&global, &key)? {
        JsValue::Object(object) => js
            .as_callable(&object)?
            .ok_or(RuntimeError::Invariant("a callable global")),
        _ => Err(RuntimeError::Invariant("a callable global")),
    }
}

fn read_kind(kind: f64) -> Result<Kind, String> {
    Kind::from_u8(kind as u8)
}

fn api_spawn(parent: f64, kind: f64, text: String) -> u32 {
    let kind = match read_kind(kind) {
        Ok(kind) => kind,
        Err(err) => {
            set_api_error(err);
            return 0;
        }
    };
    let mut desc = NodeDesc::view(0, parent as u32);
    desc.kind = kind;
    desc.text = text;
    match with_turn(|turn| turn.spawn(desc)) {
        Ok(handle) => handle,
        Err(err) => {
            set_api_error(err);
            0
        }
    }
}

fn api_spawn_at(parent: f64, index: f64, kind: f64, text: String) -> u32 {
    let kind = match read_kind(kind) {
        Ok(kind) => kind,
        Err(err) => {
            set_api_error(err);
            return 0;
        }
    };
    let mut desc = NodeDesc::view(0, parent as u32);
    desc.kind = kind;
    desc.text = text;
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
fn api_spawn_scrollbar(target: f64, index: f64) -> u32 {
    match with_turn(|turn| turn.spawn_scrollbar(target as u32, index as u32)) {
        Ok(id) => id,
        Err(err) => {
            set_api_error(err);
            0
        }
    }
}

fn api_set_scrollbar_prop(handle: f64, key: String, tag: f64, number: f64, text: String) -> i32 {
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
        turn.set_scrollbar_prop(handle as u32, key, value.clone());
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

fn api_dispose_scrollbar(handle: f64) -> i32 {
    match with_turn(|turn| {
        turn.dispose_scrollbar(handle as u32);
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

fn api_move_scrollbar(handle: f64, target: f64, index: f64) -> i32 {
    match with_turn(|turn| {
        turn.move_scrollbar(handle as u32, target as u32, index as u32);
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

fn api_create_var(source: f64, initial_tag: f64, initial_number: f64) -> u32 {
    let initial = match initial_tag as u32 {
        0 => Value::Null,
        1 => Value::Bool(initial_number != 0.0),
        2 => Value::Int(initial_number as i64),
        3 => Value::Float(initial_number),
        other => {
            set_api_error(format!("unsupported var initial tag {other}"));
            return 0;
        }
    };
    match with_turn(|turn| turn.create_var(source as u8, initial)) {
        Ok(var) => var,
        Err(err) => {
            set_api_error(err);
            0
        }
    }
}

/// Shared wire decoding of an animation spec for the var APIs. `anim` 0
/// immediate, 2 tween (a=duration ms, c=easing code), 3 spring (a=stiffness,
/// b=damping); matches the runtime's `animParts`.
fn anim_spec(anim: u32, a: f64, b: f64, c: u32) -> Result<Option<AnimSpec>, String> {
    Ok(match anim {
        0 => None,
        2 => Some(AnimSpec::Tween {
            duration_ms: a as f32,
            easing: c.min(255) as u8,
        }),
        3 => Some(AnimSpec::Spring {
            stiffness: a as f32,
            damping: b as f32,
        }),
        other => return Err(format!("unknown anim spec code {other}")),
    })
}

fn api_bind_var(var: f64, target: f64) -> i32 {
    match with_turn(|turn| {
        turn.bind_var(var as u32, target as u32);
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}
fn api_set_style_transition(
    handle: f64,
    slot: f64,
    enabled: f64,
    duration: f64,
    easing: f64,
) -> i32 {
    let slot = match u8::try_from(slot as u64) {
        Ok(narrow) if narrow as f64 == slot => narrow,
        _ => {
            set_api_error(format!("bad transition slot {slot}"));
            return 1;
        }
    };
    let spec = if enabled == 0.0 {
        None
    } else {
        if easing != 0.0 && easing != 1.0 {
            set_api_error("transition easing must be \"linear\" or \"easeInOut\"".to_string());
            return 1;
        }
        if !duration.is_finite() || duration < 0.0 || duration > f32::MAX as f64 {
            set_api_error(
                "transition duration must be a finite non-negative number".to_string(),
            );
            return 1;
        }
        match TransitionSpec::new(duration as f32, easing as u8) {
            Ok(spec) => Some(spec),
            Err(err) => {
                set_api_error(err);
                return 1;
            }
        }
    };
    match with_turn(|turn| {
        turn.set_style_transition(handle as u32, slot, spec);
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}


fn api_set_var(var: f64, tag: f64, number: f64, anim: f64, a: f64, b: f64, c: f64) -> i32 {
    let value = match tag as u32 {
        0 => Value::Null,
        1 => Value::Bool(number != 0.0),
        2 => Value::Int(number as i64),
        3 => Value::Float(number),
        other => {
            set_api_error(format!("unsupported var value tag {other}"));
            return 1;
        }
    };
    let spec = match anim_spec(anim as u32, a, b, c as u32) {
        Ok(spec) => spec,
        Err(err) => {
            set_api_error(err);
            return 1;
        }
    };
    match with_turn(|turn| {
        turn.set_var(var as u32, value, spec);
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

fn api_settle_var(var: f64) -> i32 {
    match with_turn(|turn| {
        turn.settle_var(var as u32);
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

fn api_dispose_var(var: f64) -> i32 {
    match with_turn(|turn| {
        turn.dispose_var(var as u32);
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

/// Program bytes arrive as a JS array of byte numbers; programs are small.
fn api_define_ui_function(scope: &mut Scope<'_>, params: f64, bytes: JsValue, reuse: f64) -> u32 {
    if params.fract() != 0.0 || !(0.0..=255.0).contains(&params) {
        set_api_error(format!("bad worklet parameter count {params}"));
        return 0;
    }
    if reuse.fract() != 0.0 || !(0.0..=u32::MAX as f64).contains(&reuse) {
        set_api_error(format!("bad worklet site id {reuse}"));
        return 0;
    }
    let data = match byte_array(scope, &bytes) {
        Some(data) => data,
        None => {
            set_api_error("ui program is not a byte array".into());
            return 0;
        }
    };
    let (captures, consts, code) = match bubble_core::decode_ui_function_payload(&data) {
        Ok(parts) => parts,
        Err(error) => {
            set_api_error(error);
            return 0;
        }
    };
    match with_turn(|turn| {
        turn.define_ui_function(params as u8, captures, consts, code, reuse as u32)
    }) {
        Ok(id) => id,
        Err(err) => {
            set_api_error(err);
            0
        }
    }
}

fn api_bind_gesture_program(handle: f64, slot: f64, function: f64) -> i32 {
    if !(1..=9).contains(&(slot as u32)) {
        set_api_error(format!("bad gesture event slot {slot}"));
        return 1;
    }
    match with_turn(|turn| {
        turn.bind_gesture_program(handle as u32, slot as u8, function as u32);
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

fn api_run_ui_function(scope: &mut Scope<'_>, function: f64, bytes: JsValue) -> i32 {
    let data = match byte_array(scope, &bytes) {
        Some(data) => data,
        None => {
            set_api_error("ui args are not a byte array".into());
            return 1;
        }
    };
    let args = match bubble_core::decode_ui_args(&data) {
        Ok(args) => args,
        Err(error) => {
            set_api_error(error);
            return 1;
        }
    };
    match with_turn(|turn| {
        turn.run_ui_function(function as u32, args);
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

fn api_cancel_anim(var: f64) -> i32 {
    match with_turn(|turn| {
        turn.cancel_animation(var as u32);
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

fn api_spawn_style_decl(target: f64, component: f64, index: f64) -> u32 {
    match with_turn(|turn| turn.spawn_style_decl(target as u32, component as u8, index as u32)) {
        Ok(decl) => decl,
        Err(err) => {
            set_api_error(err);
            0
        }
    }
}

fn api_set_style_prop(decl: f64, key: String, tag: f64, number: f64, text: String) -> i32 {
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
        turn.set_style_decl_prop(decl as u32, key, value.clone());
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

/// Program bytes arrive as a JS array of byte numbers; programs are small.
fn api_set_style_prop_program(
    scope: &mut Scope<'_>,
    decl: f64,
    key: String,
    bytes: JsValue,
) -> i32 {
    let data = match byte_array(scope, &bytes) {
        Some(data) => data,
        None => {
            set_api_error("style program is not a byte array".into());
            return 1;
        }
    };
    let program = match bubble_core::decode_program(&data) {
        Ok(program) => program,
        Err(error) => {
            set_api_error(error.to_string());
            return 1;
        }
    };
    match with_turn(|turn| {
        turn.set_style_decl_prop(decl as u32, key, Value::Program(program));
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

/// A JS array of byte numbers; any non-numeric item rejects the whole array.
fn byte_array(scope: &mut Scope<'_>, value: &JsValue) -> Option<Vec<u8>> {
    let JsValue::Object(object) = value else {
        return None;
    };
    let key = PropertyKey::from_js_string(JsString::from_static("length"));
    let length = scope.get_property(object, &key).ok()?;
    let length = scope.to_number(&length).ok()?;
    let length = length.max(0.0) as usize;
    let mut data = Vec::with_capacity(length);
    for index in 0..length {
        let item = scope
            .get_property(object, &PropertyKey::index(index as u32))
            .ok()?;
        let number = scope.to_number(&item).ok()?;
        data.push(number as u8);
    }
    Some(data)
}

fn api_move_style_decl(decl: f64, target: f64, index: f64) -> i32 {
    match with_turn(|turn| {
        turn.move_style_decl(decl as u32, target as u32, index as u32);
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

fn api_dispose_style_decl(decl: f64) -> i32 {
    match with_turn(|turn| {
        turn.dispose_style_decl(decl as u32);
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}
fn api_spawn_decoration(target: f64, kind: f64, index: f64) -> u32 {
    match with_turn(|turn| turn.spawn_decoration(target as u32, kind as u8, index as u32)) {
        Ok(id) => id,
        Err(err) => {
            set_api_error(err);
            0
        }
    }
}

fn api_set_decoration_prop(handle: f64, key: String, tag: f64, number: f64, text: String) -> i32 {
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
        turn.set_decoration_prop(handle as u32, key, value.clone());
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

fn api_set_decoration_prop_program(
    scope: &mut Scope<'_>,
    id: f64,
    key: String,
    bytes: JsValue,
) -> i32 {
    let data = match byte_array(scope, &bytes) {
        Some(data) => data,
        None => {
            set_api_error("decoration program is not a byte array".into());
            return 1;
        }
    };
    let program = match bubble_core::decode_program(&data) {
        Ok(program) => program,
        Err(error) => {
            set_api_error(error.to_string());
            return 1;
        }
    };
    match with_turn(|turn| {
        turn.set_decoration_prop(id as u32, key, Value::Program(program));
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

fn api_move_decoration(id: f64, target: f64, index: f64) -> i32 {
    match with_turn(|turn| {
        turn.move_decoration(id as u32, target as u32, index as u32);
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_api_error(err);
            1
        }
    }
}

fn api_dispose_decoration(id: f64) -> i32 {
    match with_turn(|turn| {
        turn.dispose_decoration(id as u32);
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

fn host_emit(vm: &mut Vm, name: &str, payload: &str) -> Result<(), String> {
    enter_js();
    let result = (|| -> Result<(), RuntimeError> {
        let emit = js_callable(&mut vm.context, "__bubbleHostEmit")?;
        vm.context.call(
            &emit,
            JsValue::Undefined,
            &[
                JsValue::String(JsString::try_from_utf8(name)?),
                JsValue::String(JsString::try_from_utf8(payload)?),
            ],
        )?;
        pump(vm)
    })();
    leave_js();
    result.map_err(|err| format_eval(vm, err))
}

fn api_storage_get(key: String) -> String {
    let Some(shared) = flags() else {
        return "null".into();
    };
    let Ok(map) = shared.storage.lock() else {
        return "null".into();
    };
    map.get(&key)
        .and_then(|value| serde_json::to_string(value).ok())
        .unwrap_or_else(|| "null".into())
}

fn api_storage_set(key: String, value: String) {
    let Some(shared) = flags() else {
        return;
    };
    if let Ok(mut map) = shared.storage.lock() {
        map.insert(key, value);
    }
    shared.persist_storage();
}

fn api_storage_remove(key: String) {
    let Some(shared) = flags() else {
        return;
    };
    if let Ok(mut map) = shared.storage.lock() {
        map.remove(&key);
    }
    shared.persist_storage();
}

fn api_storage_keys() -> String {
    let Some(shared) = flags() else {
        return "[]".into();
    };
    let Ok(map) = shared.storage.lock() else {
        return "[]".into();
    };
    let keys: Vec<&String> = map.keys().collect();
    serde_json::to_string(&keys).unwrap_or_else(|_| "[]".into())
}

fn api_set_back_handler(has: f64) {
    if let Some(shared) = flags() {
        shared.has_back_handler.store(has != 0.0, Ordering::Relaxed);
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
const __hostListeners = new Map();
const __hostQueued = [];
globalThis.__bubbleHostParams = null;
globalThis.bubbleHost = {
  get params() { return globalThis.__bubbleHostParams; },
  call(method, params) {
    return new Promise((resolve, reject) => {
      const id = globalThis.__bubble.request(method, JSON.stringify(params ?? null));
      __hostPending.set(id, { resolve, reject });
    });
  },
  post(method, params) {
    void this.call(method, params).catch(() => {});
  },
  on(name, fn) {
    let list = __hostListeners.get(name);
    if (!list) {
      list = [];
      __hostListeners.set(name, list);
    }
    list.push(fn);
    for (let i = __hostQueued.length - 1; i >= 0; i -= 1) {
      if (__hostQueued[i].name !== name) continue;
      const [event] = __hostQueued.splice(i, 1);
      fn(event.payload);
    }
  }
};
globalThis.__bubbleHostEmit = (name, payload) => {
  let value = payload;
  try { value = JSON.parse(payload); } catch (err) {}
  const list = __hostListeners.get(name);
  if (!list || list.length === 0) {
    __hostQueued.push({ name, payload: value });
    return;
  }
  for (const fn of list) fn(value);
};
globalThis.bubbleStorage = {
  get(key) { return JSON.parse(globalThis.__bubble.storage_get(String(key))); },
  set(key, value) { globalThis.__bubble.storage_set(String(key), String(value)); },
  remove(key) { globalThis.__bubble.storage_remove(String(key)); },
  keys() { return JSON.parse(globalThis.__bubble.storage_keys()); }
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
let __backHandler = null;
globalThis.bubbleHost.onBack = (fn) => {
  __backHandler = fn;
  globalThis.__bubble.set_back_handler(1);
};
globalThis.bubbleHost.offBack = () => {
  __backHandler = null;
  globalThis.__bubble.set_back_handler(0);
};
globalThis.__bubbleHostEmit = (function(_orig) {
  return (name, payload) => {
    if (name === 'back' && __backHandler) {
      let value = payload;
      try { value = JSON.parse(payload); } catch (err) {}
      __backHandler(value);
      return;
    }
    _orig(name, payload);
  };
})(globalThis.__bubbleHostEmit);
"#;
