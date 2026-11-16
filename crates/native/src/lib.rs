//! C ABI around one QuickJS script thread and one retained tree per shell.
//!
//! `bubble_create` returns an opaque pointer. Every other call takes that
//! pointer; shells share no process-wide binding. The host must not destroy a
//! shell while another call on it is running. `bubble_frame`, hit testing, and
//! scrolling run on the caller thread. Script evaluation does not: the script
//! thread only produces op batches, and the caller applies them.
//! A one-second interrupt rebuilds that shell's context and remounts its bundle.

// These are C ABI exports: foreign callers cannot express Rust's `unsafe`
// contract, so pointer validity is documented per function instead.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

mod script;

use std::collections::{HashMap, VecDeque};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use bubble_core::{
    call_host_layout, check_edit_session, encode_edit_events, encode_events, encode_paint,
    encode_var_events, read_abi_utf8, wrapping_layout, A11yAction, EditCommand, EditEvent,
    EditMenuAction, GestureEvent, Key, LayoutRequest, LayoutSnapshot, PointerAction, PointerInput,
    SessionId, Shell, VarEvent,
};

use script::Cmd;

pub struct HostRequest {
    pub id: u32,
    pub method: String,
    pub params: String,
}

struct HostFns {
    measure: Option<MeasureFn>,
    layout: Option<LayoutFn>,
    ctx: usize,
}

unsafe impl Send for HostFns {}
unsafe impl Sync for HostFns {}

type MeasureFn =
    extern "C" fn(*mut c_void, *const u8, u32, f32, f32, u16, f32, i32, *mut f32, *mut f32);

type LayoutFn = extern "C" fn(
    *mut c_void,
    *const u8,
    u32,
    f32,
    f32,
    f32,
    u16,
    u8,
    f32,
    u64,
    *mut u8,
    u32,
) -> i32;

/// Taffy's compact lengths contain tag bits that are not heap pointers in this
/// build. The tree is only locked by the caller/frame thread.
pub struct ShellSlot(Mutex<Shell>);

unsafe impl Send for ShellSlot {}
unsafe impl Sync for ShellSlot {}

impl std::ops::Deref for ShellSlot {
    type Target = Mutex<Shell>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct Shared {
    shell: ShellSlot,
    pending: Mutex<VecDeque<Vec<u8>>>,
    reset: AtomicBool,
    viewport: Mutex<(f32, f32)>,
    host: HostFns,
    last_error: Mutex<String>,
    paint: Mutex<Vec<u8>>,
    status: Mutex<String>,
    faults: AtomicU32,
    generation: AtomicU32,
    visible: AtomicBool,
    fonts: Mutex<Vec<(String, Vec<u8>)>>,
    epoch: AtomicU32,
    input_epoch: AtomicU32,
    requests: Arc<Mutex<VecDeque<HostRequest>>>,
    commands: Mutex<Option<Sender<Cmd>>>,
    interrupt: AtomicBool,
    js_since_ms: AtomicU64,
    stop_watch: AtomicBool,
    storage: Mutex<HashMap<String, String>>,
    storage_dir: Mutex<Option<PathBuf>>,
    storage_id: Mutex<String>,
    params: Mutex<String>,
    has_back_handler: AtomicBool,
}

impl Shared {
    fn set_status(&self, status: &str) {
        if let Ok(mut slot) = self.status.lock() {
            *slot = status.to_owned();
        }
    }

    fn set_error(&self, message: &str) {
        if let Ok(mut slot) = self.last_error.lock() {
            *slot = message.to_owned();
        }
    }
}

impl Shared {
    fn bind_storage(&self, manifest: &str) {
        let id = serde_json::from_str::<serde_json::Value>(manifest)
            .ok()
            .and_then(|value| {
                value
                    .get("id")
                    .and_then(|id| id.as_str())
                    .map(str::to_string)
            })
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| "default".into());
        if let Ok(mut slot) = self.storage_id.lock() {
            *slot = id.clone();
        }
        let Some(dir) = self.storage_dir.lock().ok().and_then(|dir| dir.clone()) else {
            return;
        };
        let loaded = std::fs::read_to_string(storage_file(&dir, &id))
            .ok()
            .and_then(|text| serde_json::from_str::<HashMap<String, String>>(&text).ok())
            .unwrap_or_default();
        if let Ok(mut map) = self.storage.lock() {
            *map = loaded;
        }
    }

    fn persist_storage(&self) {
        let Some(dir) = self.storage_dir.lock().ok().and_then(|dir| dir.clone()) else {
            return;
        };
        let id = self
            .storage_id
            .lock()
            .map(|id| id.clone())
            .unwrap_or_else(|_| "default".into());
        let Ok(map) = self.storage.lock() else {
            return;
        };
        let Ok(text) = serde_json::to_string(&*map) else {
            return;
        };
        drop(map);
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(storage_file(&dir, &id), text);
    }
}

/// Opaque shell. The host passes this pointer to every call and must not
/// destroy it while another call on it is running. Shells share no state.
pub struct BubbleShell {
    shared: Arc<Shared>,
    tx: Sender<Cmd>,
    thread: Mutex<Option<JoinHandle<()>>>,
    /// Borrowed by `bubble_last_error` until the next fallible call or destroy.
    last_error: Mutex<Option<CString>>,
}

fn shell_ref<'a>(shell: *mut BubbleShell) -> Result<&'a BubbleShell, String> {
    if shell.is_null() {
        return Err("shell is null".into());
    }
    Ok(unsafe { &*shell })
}

fn engine(shell: *mut BubbleShell) -> Result<EngineRef, String> {
    let shell = shell_ref(shell)?;
    Ok(EngineRef {
        shared: Arc::clone(&shell.shared),
        tx: shell.tx.clone(),
    })
}

struct EngineRef {
    shared: Arc<Shared>,
    tx: Sender<Cmd>,
}

fn store_error(shell: *mut BubbleShell, message: &str) {
    let Ok(shell) = shell_ref(shell) else {
        return;
    };
    shell.shared.set_error(message);
    if let Ok(mut slot) = shell.last_error.lock() {
        *slot = CString::new(message.replace('\0', " ")).ok();
    }
}

fn owned_error(message: &str) -> *mut c_char {
    CString::new(message.replace('\0', " "))
        .unwrap_or_else(|_| CString::new("invalid error").expect("static error"))
        .into_raw()
}

fn finish(shell: *mut BubbleShell, result: Result<(), String>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(err) => {
            store_error(shell, &err);
            1
        }
    }
}

fn finish_error(shell: *mut BubbleShell, result: Result<(), String>) -> *mut c_char {
    match result {
        Ok(()) => std::ptr::null_mut(),
        Err(err) => {
            store_error(shell, &err);
            owned_error(&err)
        }
    }
}

fn catch<T>(body: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)) {
        Ok(result) => result,
        Err(_) => Err("panic at the ffi boundary".into()),
    }
}

/// Apply committed op batches on the caller/frame thread. A reset request
/// swaps in a fresh root before the new batch is applied.
fn drain_pending(shared: &Shared) -> Result<(), String> {
    let reset = shared.reset.swap(false, Ordering::SeqCst);
    let (events, edits, vars) = {
        let mut queue = shared
            .pending
            .lock()
            .map_err(|_| "op queue poisoned".to_string())?;
        let mut shell = lock_or(&shared.shell, "tree lock poisoned")?;
        if reset {
            *shell = Shell::blank()?;
        }
        while let Some(bytes) = queue.pop_front() {
            shell.apply_ops(&bytes)?;
        }
        (
            shell.take_gesture_events(),
            shell.take_edit_events(),
            shell.take_var_events(),
        )
    };
    send_events(shared, events);
    send_edits(shared, edits);
    send_vars(shared, vars);
    // UI-thread faults surface through the shared fault counter and the last
    // error the host reads.
    {
        let mut shell = lock_or(&shared.shell, "tree lock poisoned")?;
        let (faults, last_error) = shell.take_ui_faults();
        if faults != 0 {
            shared.faults.fetch_add(faults, Ordering::Relaxed);
            if !last_error.is_empty() {
                shared.set_error(&last_error);
            }
        }
        let js_calls = shell.take_js_calls();
        if !js_calls.is_empty() {
            let bytes = bubble_core::encode_js_calls(&js_calls);
            if let Ok(slot) = shared.commands.lock() {
                if let Some(tx) = slot.as_ref() {
                    let _ = tx.send(Cmd::UiCalls { bytes });
                }
            }
        }
    }
    Ok(())
}

/// Hand recognition events to the script thread. The batch is already
/// encoded so the frame thread never touches JS state.
fn send_events(shared: &Shared, events: Vec<GestureEvent>) {
    if events.is_empty() {
        return;
    }
    let Ok(slot) = shared.commands.lock() else {
        return;
    };
    if let Some(tx) = slot.as_ref() {
        let _ = tx.send(Cmd::Gesture {
            epoch: shared.input_epoch.load(Ordering::SeqCst),
            bytes: encode_events(&events),
        });
    }
}

fn send_edits(shared: &Shared, events: Vec<EditEvent>) {
    if events.is_empty() {
        return;
    }
    let Ok(slot) = shared.commands.lock() else {
        return;
    };
    if let Some(tx) = slot.as_ref() {
        let _ = tx.send(Cmd::Edit {
            epoch: shared.input_epoch.load(Ordering::SeqCst),
            bytes: encode_edit_events(&events),
        });
    }
}

/// Hand settled-variable events to the script thread.
fn send_vars(shared: &Shared, events: Vec<VarEvent>) {
    if events.is_empty() {
        return;
    }
    let Ok(slot) = shared.commands.lock() else {
        return;
    };
    if let Some(tx) = slot.as_ref() {
        let _ = tx.send(Cmd::Vars {
            bytes: encode_var_events(&events),
        });
    }
}

#[no_mangle]
pub extern "C" fn bubble_create(host: *const BubbleHost) -> *mut BubbleShell {
    let result = catch(|| {
        let (measure, layout, ctx) = unsafe {
            if host.is_null() {
                (None, None, 0)
            } else {
                ((*host).measure, (*host).layout, (*host).ctx as usize)
            }
        };
        let shell = Shell::blank()?;
        let shared = Arc::new(Shared {
            shell: ShellSlot(Mutex::new(shell)),
            pending: Mutex::new(VecDeque::new()),
            reset: AtomicBool::new(false),
            viewport: Mutex::new((0.0, 0.0)),
            host: HostFns {
                measure,
                layout,
                ctx,
            },
            last_error: Mutex::new(String::new()),
            paint: Mutex::new(Vec::new()),
            status: Mutex::new("created".into()),
            faults: AtomicU32::new(0),
            generation: AtomicU32::new(0),
            visible: AtomicBool::new(true),
            fonts: Mutex::new(Vec::new()),
            epoch: AtomicU32::new(0),
            input_epoch: AtomicU32::new(1),
            requests: Arc::new(Mutex::new(VecDeque::new())),
            commands: Mutex::new(None),
            interrupt: AtomicBool::new(false),
            js_since_ms: AtomicU64::new(0),
            stop_watch: AtomicBool::new(false),
            storage: Mutex::new(HashMap::new()),
            storage_dir: Mutex::new(None),
            storage_id: Mutex::new("default".into()),
            params: Mutex::new("null".into()),
            has_back_handler: AtomicBool::new(false),
        });
        let (tx, thread) = script::start(Arc::clone(&shared));
        if let Ok(mut slot) = shared.commands.lock() {
            *slot = Some(tx.clone());
        }
        Ok(Box::into_raw(Box::new(BubbleShell {
            shared,
            tx,
            thread: Mutex::new(Some(thread)),
            last_error: Mutex::new(None),
        })))
    });
    match result {
        Ok(shell) => shell,
        Err(_) => std::ptr::null_mut(),
    }
}

/// JSON value read by `host.params` on the next load of the bound shell.
#[no_mangle]
pub extern "C" fn bubble_set_params(shell: *mut BubbleShell, json: *const c_char) -> *mut c_char {
    let result = catch(|| {
        let engine = engine(shell)?;
        let json = cstr(json)?;
        serde_json::from_str::<serde_json::Value>(&json).map_err(|err| err.to_string())?;
        let mut slot = engine
            .shared
            .params
            .lock()
            .map_err(|_| "params lock poisoned".to_string())?;
        *slot = json;
        Ok(())
    });
    finish_error(shell, result)
}

/// Directory for `bubble.storage.<manifest id>.json`. Null keeps storage in memory.
#[no_mangle]
pub extern "C" fn bubble_set_storage_dir(
    shell: *mut BubbleShell,
    path: *const c_char,
) -> *mut c_char {
    let result = catch(|| {
        let engine = engine(shell)?;
        let dir = if path.is_null() {
            None
        } else {
            let text = cstr(path)?;
            if text.is_empty() {
                None
            } else {
                Some(PathBuf::from(text))
            }
        };
        let mut slot = engine
            .shared
            .storage_dir
            .lock()
            .map_err(|_| "storage lock poisoned".to_string())?;
        *slot = dir;
        Ok(())
    });
    finish_error(shell, result)
}

fn storage_file(dir: &std::path::Path, id: &str) -> PathBuf {
    let safe: String = id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    dir.join(format!("bubble.storage.{safe}.json"))
}

/// Deliver `payload` JSON to `host.on(name)` listeners. Kept until a bundle is listening.
#[no_mangle]
pub extern "C" fn bubble_send(
    shell: *mut BubbleShell,
    name: *const c_char,
    payload: *const c_char,
) -> *mut c_char {
    let result = catch(|| {
        let engine = engine(shell)?;
        let name = cstr(name)?;
        let payload = if payload.is_null() {
            "null".to_string()
        } else {
            cstr(payload)?
        };
        serde_json::from_str::<serde_json::Value>(&payload).map_err(|err| err.to_string())?;
        engine
            .tx
            .send(Cmd::Emit { name, payload })
            .map_err(|_| "script thread is gone".to_string())?;
        Ok(())
    });
    finish_error(shell, result)
}

#[no_mangle]
pub extern "C" fn bubble_destroy(shell: *mut BubbleShell) {
    if shell.is_null() {
        return;
    }
    let shell = unsafe { Box::from_raw(shell) };
    script::stop(&shell.shared);
    let _ = shell.tx.send(Cmd::Shutdown);
    let thread = shell.thread.lock().ok().and_then(|mut guard| guard.take());
    drop(shell);
    if let Some(thread) = thread {
        let _ = thread.join();
    }
}

#[no_mangle]
pub extern "C" fn bubble_load(
    shell: *mut BubbleShell,
    manifest_json: *const c_char,
    entry: *const u8,
    entry_len: u32,
) -> *mut c_char {
    let result = catch(|| {
        let engine = engine(shell)?;
        let manifest = cstr(manifest_json)?;
        if entry.is_null() && entry_len != 0 {
            return Err("entry pointer is null".into());
        }
        let bytes = unsafe { std::slice::from_raw_parts(entry, entry_len as usize) }.to_vec();
        {
            let mut pending = engine
                .shared
                .pending
                .lock()
                .map_err(|_| "op queue poisoned".to_string())?;
            pending.clear();
            let mut shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
            *shell = Shell::blank()?;
            engine.shared.reset.store(false, Ordering::SeqCst);
        }
        let (ack_tx, ack_rx) = std::sync::mpsc::channel();
        engine
            .tx
            .send(Cmd::Load {
                manifest,
                entry: bytes,
                ack: ack_tx,
            })
            .map_err(|_| "script thread is gone".to_string())?;
        ack_rx
            .recv()
            .map_err(|_| "script thread dropped the load".to_string())?
    });
    finish_error(shell, result)
}

#[no_mangle]
pub extern "C" fn bubble_frame(
    shell: *mut BubbleShell,
    width: f32,
    height: f32,
    paint: *mut *const u8,
    paint_len: *mut u32,
) -> i32 {
    let result = catch(|| {
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        if let Ok(mut viewport) = engine.shared.viewport.lock() {
            *viewport = (width, height);
        }
        let (edits, bytes) = {
            let mut shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
            shell.set_viewport(width, height);
            let host = &engine.shared.host;
            shell.compute_layout(
                &mut |text, font_size, line_height, font_weight, max_width| {
                    host_measure(host, text, font_size, line_height, font_weight, max_width)
                },
            )?;
            shell.refresh_caret(&mut |req| field_layout(host, req))?;
            let bytes = encode_paint(&shell.paint());
            let edits = shell.take_edit_events();
            (edits, bytes)
        };
        send_edits(&engine.shared, edits);
        let mut slot = engine
            .shared
            .paint
            .lock()
            .map_err(|_| "paint lock poisoned".to_string())?;
        *slot = bytes;
        if !paint.is_null() {
            unsafe { *paint = slot.as_ptr() };
        }
        if !paint_len.is_null() {
            unsafe { *paint_len = slot.len() as u32 };
        }
        Ok(())
    });
    finish(shell, result)
}

/// One normalized pointer sample. `action` 0 down, 1 move, 2 up, 3 cancel.
/// `pointer_kind` 0 touch, 1 mouse, 2 pen. `buttons` bit 0 is the primary
/// button. Time is the host's monotonic clock in milliseconds.
#[no_mangle]
pub extern "C" fn bubble_pointer(
    shell: *mut BubbleShell,
    action: u32,
    id: u32,
    x: f32,
    y: f32,
    time_ms: f64,
    pointer_kind: u32,
    buttons: u32,
) -> i32 {
    let result = catch(|| {
        let action =
            PointerAction::parse(action).ok_or_else(|| format!("bad pointer action {action}"))?;
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        let (events, edits) = {
            let mut shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
            let host = &engine.shared.host;
            shell.pointer_with_layout(
                PointerInput::from_abi(action, id, x, y, time_ms, pointer_kind, buttons),
                Some(&mut |req| field_layout(host, req)),
            );
            (shell.take_gesture_events(), shell.take_edit_events())
        };
        send_events(&engine.shared, events);
        send_edits(&engine.shared, edits);
        Ok(())
    });
    finish(shell, result)
}

/// Pressed feedback only: no recognition, no tap, no scroll. Editing hosts use
/// it when their own pointer path owns the sample, so one physical touch is
/// never fed to both entry points.
#[no_mangle]
pub extern "C" fn bubble_visual_pointer(
    shell: *mut BubbleShell,
    action: u32,
    id: u32,
    x: f32,
    y: f32,
    time_ms: f64,
    pointer_kind: u32,
    buttons: u32,
) -> i32 {
    let result = catch(|| {
        let action =
            PointerAction::parse(action).ok_or_else(|| format!("bad pointer action {action}"))?;
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        let mut shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
        shell.visual_pointer(PointerInput::from_abi(
            action,
            id,
            x,
            y,
            time_ms,
            pointer_kind,
            buttons,
        ));
        Ok(())
    });
    finish(shell, result)
}

/// UI-thread timer tick for long press and the double-tap window.
#[no_mangle]
pub extern "C" fn bubble_tick(shell: *mut BubbleShell, time_ms: f64) -> i32 {
    let result = catch(|| {
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        let events = {
            let mut shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
            shell.advance_gestures(time_ms);
            shell.take_gesture_events()
        };
        send_events(&engine.shared, events);
        Ok(())
    });
    finish(shell, result)
}

fn lock_or<'a, T>(mutex: &'a Mutex<T>, err: &str) -> Result<std::sync::MutexGuard<'a, T>, String> {
    mutex.lock().map_err(|_| err.to_string())
}

fn edit_op(
    shell: *mut BubbleShell,
    session: u64,
    op: impl FnOnce(&mut Shell, SessionId) -> Result<(), String>,
) -> i32 {
    let result = catch(|| {
        let eng = engine(shell)?;
        drain_pending(&eng.shared)?;
        let edits = {
            let mut sh = lock_or(&eng.shared.shell, "tree lock poisoned")?;
            let sid = check_edit_session(&sh, session)?;
            op(&mut sh, sid)?;
            sh.take_edit_events()
        };
        send_edits(&eng.shared, edits);
        Ok(())
    });
    finish(shell, result)
}

/// Mutate the focused field, or focus one. `action` 0 focuses `a` (`0` blurs).
/// 1 inserts UTF-8 text. 2 backspace. 3 delete forward. 4 sets the UTF-16
/// selection to `a`..`b`. 5 starts composition. 6 updates it. 7 ends it.
#[no_mangle]
pub extern "C" fn bubble_edit(
    shell: *mut BubbleShell,
    action: u32,
    text: *const u8,
    text_len: u32,
    a: u32,
    b: u32,
) -> i32 {
    let result = catch(|| {
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        let text = read_abi_utf8(text, text_len, "edit text")?;
        let command = EditCommand::parse(action, text, a, b)?;
        let events = {
            let mut shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
            if action == 0 {
                shell.focus(a)?;
            } else if let Some(command) = command {
                shell.edit(command)?;
            }
            shell.take_edit_events()
        };
        send_edits(&engine.shared, events);
        Ok(())
    });
    finish(shell, result)
}

/// Open a nested edit batch on the focused field. Events are published by
/// `bubble_edit_batch_end`.
#[no_mangle]
pub extern "C" fn bubble_edit_batch_begin(shell: *mut BubbleShell, session: u64) -> i32 {
    edit_op(shell, session, |sh, _sid| sh.begin_edit_batch())
}

/// Close one batch level. The outermost level emits one delta event.
#[no_mangle]
pub extern "C" fn bubble_edit_batch_end(shell: *mut BubbleShell, session: u64) -> i32 {
    edit_op(shell, session, |sh, _sid| sh.end_edit_batch())
}

/// Read the focused editing snapshot. `comp_end` is `u32::MAX` when no
/// composition range is active; `composing` is 0 or 1.
#[no_mangle]
pub extern "C" fn bubble_edit_snapshot(
    shell: *mut BubbleShell,
    session: *mut u64,
    revision: *mut u64,
    anchor: *mut u32,
    active: *mut u32,
    comp_start: *mut u32,
    comp_end: *mut u32,
    composing: *mut u32,
) -> i32 {
    let result = catch(|| {
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        let state = {
            let shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
            shell.focused_state()?
        };
        unsafe {
            if !session.is_null() {
                *session = state.session.get();
            }
            if !revision.is_null() {
                *revision = state.revision;
            }
            if !anchor.is_null() {
                *anchor = state.selection.anchor.offset;
            }
            if !active.is_null() {
                *active = state.selection.active.offset;
            }
            let (start, end) = state.composition.unwrap_or((0, u32::MAX));
            if !comp_start.is_null() {
                *comp_start = start;
            }
            if !comp_end.is_null() {
                *comp_end = end;
            }
            if !composing.is_null() {
                *composing = u32::from(state.composing);
            }
        }
        Ok(())
    });
    finish(shell, result)
}

/// Replace `start..end` with UTF-8 text and apply Android's
/// `newCursorPosition`. `composing` 1 keeps/starts composition.
#[no_mangle]
pub extern "C" fn bubble_edit_replace(
    shell: *mut BubbleShell,
    session: u64,
    start: u32,
    end: u32,
    text: *const u8,
    text_len: u32,
    new_cursor_position: i32,
    composing: u32,
) -> i32 {
    let result = catch(|| {
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        let text = read_abi_utf8(text, text_len, "edit text")?;
        let events = {
            let mut shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
            check_edit_session(&shell, session)?;
            shell.replace_with_cursor(start, end, &text, new_cursor_position, composing != 0)?;
            shell.take_edit_events()
        };
        send_edits(&engine.shared, events);
        Ok(())
    });
    finish(shell, result)
}

#[no_mangle]
pub extern "C" fn bubble_edit_commit(
    shell: *mut BubbleShell,
    session: u64,
    text: *const u8,
    text_len: u32,
    new_cursor_position: i32,
) -> i32 {
    let text = match catch(|| read_abi_utf8(text, text_len, "edit text")) {
        Ok(text) => text,
        Err(err) => return finish(shell, Err(err)),
    };
    edit_op(shell, session, move |sh, _sid| {
        sh.commit_text(&text, new_cursor_position)
    })
}

#[no_mangle]
pub extern "C" fn bubble_edit_compose(
    shell: *mut BubbleShell,
    session: u64,
    text: *const u8,
    text_len: u32,
    new_cursor_position: i32,
) -> i32 {
    let text = match catch(|| read_abi_utf8(text, text_len, "edit text")) {
        Ok(text) => text,
        Err(err) => return finish(shell, Err(err)),
    };
    edit_op(shell, session, move |sh, _sid| {
        sh.compose_text(&text, new_cursor_position)
    })
}

#[no_mangle]
pub extern "C" fn bubble_edit_undo(shell: *mut BubbleShell, session: u64) -> i32 {
    edit_op(shell, session, |sh, _sid| sh.edit(EditCommand::Undo))
}

#[no_mangle]
pub extern "C" fn bubble_edit_redo(shell: *mut BubbleShell, session: u64) -> i32 {
    edit_op(shell, session, |sh, _sid| sh.edit(EditCommand::Redo))
}

/// Delete UTF-16 units around the focused selection.
#[no_mangle]
pub extern "C" fn bubble_edit_delete_surrounding(
    shell: *mut BubbleShell,
    session: u64,
    before: u32,
    after: u32,
) -> i32 {
    edit_op(shell, session, |sh, _sid| {
        sh.delete_surrounding(before, after)
    })
}

/// Delete code points around the focused selection.
#[no_mangle]
pub extern "C" fn bubble_edit_delete_surrounding_code_points(
    shell: *mut BubbleShell,
    session: u64,
    before: u32,
    after: u32,
) -> i32 {
    edit_op(shell, session, |sh, _sid| {
        sh.delete_surrounding_code_points(before, after)
    })
}

#[no_mangle]
pub extern "C" fn bubble_edit_set_composition(
    shell: *mut BubbleShell,
    session: u64,
    start: u32,
    end: u32,
) -> i32 {
    edit_op(shell, session, |sh, _sid| {
        sh.set_composition_region(start, end)
    })
}

#[no_mangle]
pub extern "C" fn bubble_edit_finish_composition(shell: *mut BubbleShell, session: u64) -> i32 {
    edit_op(shell, session, |sh, _sid| sh.finish_composition())
}

#[no_mangle]
pub extern "C" fn bubble_edit_select(
    shell: *mut BubbleShell,
    session: u64,
    anchor: u32,
    active: u32,
) -> i32 {
    edit_op(shell, session, |sh, _sid| {
        sh.edit(EditCommand::SetSelection {
            start: anchor,
            end: active,
        })
    })
}

/// Abort any open edit batch without committing it.
#[no_mangle]
pub extern "C" fn bubble_edit_batch_abort(shell: *mut BubbleShell, session: u64) -> i32 {
    let result = catch(|| {
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        let events = {
            let mut shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
            if shell
                .focused_state()
                .is_ok_and(|state| state.session.get() == session)
            {
                shell.abort_edit_batch();
            }
            shell.take_edit_events()
        };
        send_edits(&engine.shared, events);
        Ok(())
    });
    finish(shell, result)
}

/// UI-thread keyboard intent. Actions: 0 Tab, 1 Shift+Tab, 2 Enter, 3 Space,
/// 4 Escape, 5 Left, 6 Right, 7 Home, 8 End. Returns 1 handled, 0 not, -1 error.
#[no_mangle]
pub extern "C" fn bubble_key(shell: *mut BubbleShell, action: u32, time_ms: f64) -> i32 {
    let result = catch(|| {
        let key = Key::parse(action).ok_or_else(|| format!("bad key action {action}"))?;
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        let (handled, events) = {
            let mut shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
            let handled = shell.key(key, time_ms);
            (handled, shell.take_gesture_events())
        };
        send_events(&engine.shared, events);
        Ok(handled)
    });
    match result {
        Ok(handled) => i32::from(handled),
        Err(err) => {
            store_error(shell, &err);
            -1
        }
    }
}

/// Accessibility semantics snapshot as UTF-8 JSON.
#[no_mangle]
pub extern "C" fn bubble_semantics(shell: *mut BubbleShell, buf: *mut c_char, cap: u32) -> i32 {
    copy_text(shell, buf, cap, |engine| {
        drain_pending(&engine.shared)?;
        let shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
        Ok(shell.semantics_json())
    })
}

/// Route a screen-reader action. `action` 0 focus, 1 activate, 2 set value,
/// 3/4 forward/backward, 5/6 left/right, 7/8 up/down.
#[no_mangle]
pub extern "C" fn bubble_a11y_action(
    shell: *mut BubbleShell,
    handle: u32,
    action: u32,
    text: *const u8,
    text_len: u32,
) -> i32 {
    let result = catch(|| {
        let action =
            A11yAction::parse(action).ok_or_else(|| format!("bad a11y action {action}"))?;
        let action = match action {
            A11yAction::SetValue(_) => {
                A11yAction::SetValue(read_abi_utf8(text, text_len, "edit text")?)
            }
            other => other,
        };
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        let events = {
            let mut shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
            shell.a11y_action(handle, action)?;
            (shell.take_edit_events(), shell.take_gesture_events())
        };
        send_edits(&engine.shared, events.0);
        send_events(&engine.shared, events.1);
        Ok(())
    });
    finish(shell, result)
}

/// Edit menu command. `action` 0 cut, 1 copy, 2 paste, 3 select all. The
/// clipboard text is read from `clip` and, for cut/copy, written to `out`.
/// Returns 1 changed, 0 unchanged, -1 error.
#[no_mangle]
pub extern "C" fn bubble_edit_menu(
    shell: *mut BubbleShell,
    action: u32,
    clip: *const u8,
    clip_len: u32,
    out: *mut c_char,
    out_cap: u32,
) -> i32 {
    let result = catch(|| {
        let action =
            EditMenuAction::parse(action).ok_or_else(|| format!("bad menu action {action}"))?;
        let clipboard = read_abi_utf8(clip, clip_len, "edit text")?;
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        let (menu, events) = {
            let mut shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
            let menu = shell.edit_menu(action, &clipboard)?;
            (menu, shell.take_edit_events())
        };
        send_edits(&engine.shared, events);
        if let Some(text) = &menu.clipboard {
            write_buf(out, out_cap, text);
        } else {
            write_buf(out, out_cap, "");
        }
        Ok(menu.changed)
    });
    match result {
        Ok(changed) => i32::from(changed),
        Err(err) => {
            store_error(shell, &err);
            -1
        }
    }
}

/// Start and end selection handles. Returns 0 when a range is selected.
#[no_mangle]
pub extern "C" fn bubble_selection_handles(
    shell: *mut BubbleShell,
    start_x: *mut f32,
    start_y: *mut f32,
    start_w: *mut f32,
    start_h: *mut f32,
    end_x: *mut f32,
    end_y: *mut f32,
    end_w: *mut f32,
    end_h: *mut f32,
) -> i32 {
    let result = catch(|| {
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        let mut shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
        let host = &engine.shared.host;
        shell.refresh_caret(&mut |req| field_layout(host, req))?;
        let handles = shell.selection_handles();
        let Some([a, b]) = handles else {
            return Err("no selection".into());
        };
        unsafe {
            if !start_x.is_null() {
                *start_x = a.0;
            }
            if !start_y.is_null() {
                *start_y = a.1;
            }
            if !start_w.is_null() {
                *start_w = a.2;
            }
            if !start_h.is_null() {
                *start_h = a.3;
            }
            if !end_x.is_null() {
                *end_x = b.0;
            }
            if !end_y.is_null() {
                *end_y = b.1;
            }
            if !end_w.is_null() {
                *end_w = b.2;
            }
            if !end_h.is_null() {
                *end_h = b.3;
            }
        }
        Ok(())
    });
    finish(shell, result)
}

/// Deepest text field under the point, or `0`.
#[no_mangle]
pub extern "C" fn bubble_field_at(shell: *mut BubbleShell, x: f32, y: f32) -> u32 {
    let result = catch(|| {
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        let shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
        Ok(shell.field_at(x, y))
    });
    result.unwrap_or(0)
}

/// Focus the field under the point and place the caret. Returns the handle, or 0.
#[no_mangle]
pub extern "C" fn bubble_focus_at(shell: *mut BubbleShell, x: f32, y: f32) -> u32 {
    let result = catch(|| {
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        let events = {
            let mut shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
            let host = &engine.shared.host;
            let handle = shell.focus_point_with(x, y, &mut |req| field_layout(host, req))?;
            let events = shell.take_edit_events();
            Ok::<_, String>((handle, events))
        }?;
        send_edits(&engine.shared, events.1);
        Ok(events.0)
    });
    result.unwrap_or(0)
}

/// Focused caret: x, y, width, height. Returns 0 when a field is focused.
#[no_mangle]
pub extern "C" fn bubble_caret(
    shell: *mut BubbleShell,
    x: *mut f32,
    y: *mut f32,
    w: *mut f32,
    h: *mut f32,
) -> i32 {
    let result = catch(|| {
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        let mut shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
        let host = &engine.shared.host;
        shell.refresh_caret(&mut |req| field_layout(host, req))?;
        let Some((cx, cy, cw, ch)) = shell.caret_box() else {
            return Err("no caret".into());
        };
        if !x.is_null() {
            unsafe { *x = cx };
        }
        if !y.is_null() {
            unsafe { *y = cy };
        }
        if !w.is_null() {
            unsafe { *w = cw };
        }
        if !h.is_null() {
            unsafe { *h = ch };
        }
        Ok(())
    });
    finish(shell, result)
}

/// `bit0` multiline, `bit1` secure, bits 8..15 keyboard kind.
#[no_mangle]
pub extern "C" fn bubble_field_flags(shell: *mut BubbleShell, handle: u32) -> u32 {
    let result = catch(|| {
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        let shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
        shell.field_flags(handle)
    });
    result.unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn bubble_focused(shell: *mut BubbleShell) -> u32 {
    let result = catch(|| {
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        let shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
        Ok(shell.focused())
    });
    result.unwrap_or(0)
}

/// UTF-16 selection of the focused field. `mark_end` is `u32::MAX` when not composing.
#[no_mangle]
pub extern "C" fn bubble_selection(
    shell: *mut BubbleShell,
    start: *mut u32,
    end: *mut u32,
    mark_start: *mut u32,
    mark_end: *mut u32,
) -> i32 {
    let result = catch(|| {
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        let shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
        let handle = shell.focused();
        if handle == 0 {
            return Err("no focused field".into());
        }
        let (_, state) = shell.field(handle)?;
        unsafe {
            if !start.is_null() {
                *start = state.sel_start();
            }
            if !end.is_null() {
                *end = state.sel_end();
            }
            let (ms, me) = state.composition.unwrap_or((0, u32::MAX));
            if !mark_start.is_null() {
                *mark_start = ms;
            }
            if !mark_end.is_null() {
                *mark_end = me;
            }
        }
        Ok(())
    });
    finish(shell, result)
}

fn host_measure(
    host: &HostFns,
    text: &str,
    font_size: f32,
    line_height: f32,
    font_weight: u16,
    max_width: Option<f32>,
) -> (f32, f32) {
    if let Some(measure) = host.measure {
        let mut w = 0.0f32;
        let mut h = 0.0f32;
        measure(
            host.ctx as *mut std::ffi::c_void,
            text.as_ptr(),
            text.len() as u32,
            font_size,
            line_height,
            font_weight,
            max_width.unwrap_or(0.0),
            i32::from(max_width.is_some()),
            &mut w,
            &mut h,
        );
        (w, h)
    } else {
        fallback_measure(text, font_size, line_height)
    }
}
fn field_layout(host: &HostFns, req: &LayoutRequest<'_>) -> Result<LayoutSnapshot, String> {
    if let Some(layout) = host.layout {
        return call_host_layout(req, |cap, buf| {
            layout(
                host.ctx as *mut c_void,
                req.text.as_ptr(),
                req.text.len() as u32,
                req.key.width,
                req.key.font_size,
                req.key.line_height,
                req.key.font_weight,
                req.key.align,
                req.key.density,
                req.key.revision,
                buf.as_mut_ptr(),
                cap,
            )
        });
    }
    #[cfg(target_os = "android")]
    if let Some(snapshot) = android::layout_snapshot(req) {
        return snapshot;
    }
    Ok(wrapping_layout(req, |line| {
        host_measure(
            host,
            line,
            req.key.font_size,
            req.key.line_height,
            req.key.font_weight,
            None,
        )
        .0
    }))
}

#[no_mangle]
pub extern "C" fn bubble_unit_at(shell: *mut BubbleShell, x: f32, y: f32, unit: *mut u32) -> i32 {
    let result = catch(|| {
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        let shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
        let host = &engine.shared.host;
        let Some(offset) = shell.unit_at_with(x, y, &mut |req| field_layout(host, req))? else {
            return Err("no focused field".into());
        };
        if !unit.is_null() {
            unsafe { *unit = offset };
        }
        Ok(())
    });
    finish(shell, result)
}

#[no_mangle]
pub extern "C" fn bubble_caret_at(
    shell: *mut BubbleShell,
    unit: u32,
    x: *mut f32,
    y: *mut f32,
    w: *mut f32,
    h: *mut f32,
) -> i32 {
    let result = catch(|| {
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        let shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
        let host = &engine.shared.host;
        let Some((cx, cy, cw, ch)) =
            shell.caret_rect_at_with(unit, &mut |req| field_layout(host, req))?
        else {
            return Err("no focused field".into());
        };
        unsafe {
            if !x.is_null() {
                *x = cx;
            }
            if !y.is_null() {
                *y = cy;
            }
            if !w.is_null() {
                *w = cw;
            }
            if !h.is_null() {
                *h = ch;
            }
        }
        Ok(())
    });
    finish(shell, result)
}

#[no_mangle]
pub extern "C" fn bubble_selection_count(shell: *mut BubbleShell) -> u32 {
    let result = catch(|| {
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        let mut shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
        let host = &engine.shared.host;
        shell.refresh_caret(&mut |req| field_layout(host, req))?;
        Ok(shell.selection_rects().len() as u32)
    });
    result.unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn bubble_selection_rect(
    shell: *mut BubbleShell,
    index: u32,
    x: *mut f32,
    y: *mut f32,
    w: *mut f32,
    h: *mut f32,
) -> i32 {
    let result = catch(|| {
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        let mut shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
        let host = &engine.shared.host;
        shell.refresh_caret(&mut |req| field_layout(host, req))?;
        let rect = shell
            .selection_rects()
            .get(index as usize)
            .copied()
            .ok_or_else(|| format!("selection rect {index} is out of range"))?;
        unsafe {
            if !x.is_null() {
                *x = rect.0;
            }
            if !y.is_null() {
                *y = rect.1;
            }
            if !w.is_null() {
                *w = rect.2;
            }
            if !h.is_null() {
                *h = rect.3;
            }
        }
        Ok(())
    });
    finish(shell, result)
}

#[no_mangle]
pub extern "C" fn bubble_scroll(shell: *mut BubbleShell, x: f32, y: f32, dx: f32, dy: f32) {
    let _ = catch(|| {
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        let mut shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
        shell.scroll_wheel(x, y, dx, dy);
        Ok(())
    });
}

#[no_mangle]
pub extern "C" fn bubble_set_gesture_trace(shell: *mut BubbleShell, enabled: i32) {
    let _ = catch(|| {
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        let mut shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
        shell.set_gesture_trace(enabled != 0);
        Ok(())
    });
}

/// Newline-separated trace lines. Empty when tracing is off.
#[no_mangle]
pub extern "C" fn bubble_take_gesture_trace(
    shell: *mut BubbleShell,
    buf: *mut c_char,
    cap: u32,
) -> i32 {
    copy_text(shell, buf, cap, |engine| {
        drain_pending(&engine.shared)?;
        let mut shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
        Ok(shell.take_gesture_trace().join("\n"))
    })
}

#[no_mangle]
pub extern "C" fn bubble_set_visible(shell: *mut BubbleShell, visible: i32) {
    if let Ok(engine) = engine(shell) {
        engine.shared.visible.store(visible != 0, Ordering::Relaxed);
        if visible == 0 {
            if let Ok(mut shell) = engine.shared.shell.lock() {
                let events = shell.cancel_gestures(bubble_core::REASON_HOST_CANCEL);
                drop(shell);
                send_events(&engine.shared, events);
            }
        }
        let _ = engine.tx.send(Cmd::Wake);
    }
}

#[no_mangle]
pub extern "C" fn bubble_register_font(
    shell: *mut BubbleShell,
    family: *const c_char,
    data: *const u8,
    len: u32,
) {
    let result = catch(|| {
        let engine = engine(shell)?;
        let family = cstr(family)?;
        if data.is_null() && len != 0 {
            return Err("font pointer is null".into());
        }
        let bytes = unsafe { std::slice::from_raw_parts(data, len as usize) }.to_vec();
        let mut fonts = engine
            .shared
            .fonts
            .lock()
            .map_err(|_| "font lock poisoned".to_string())?;
        fonts.push((family, bytes));
        engine.shared.epoch.fetch_add(1, Ordering::Relaxed);
        Ok(())
    });
    let _ = finish(shell, result);
}

#[no_mangle]
pub extern "C" fn bubble_font_epoch(shell: *mut BubbleShell) -> u32 {
    engine(shell)
        .map(|engine| engine.shared.epoch.load(Ordering::Relaxed))
        .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn bubble_font_count(shell: *mut BubbleShell) -> u32 {
    engine(shell)
        .ok()
        .and_then(|engine| {
            engine
                .shared
                .fonts
                .lock()
                .ok()
                .map(|fonts| fonts.len() as u32)
        })
        .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn bubble_node_text(
    shell: *mut BubbleShell,
    handle: u32,
    buf: *mut c_char,
    cap: u32,
) -> i32 {
    copy_text(shell, buf, cap, |engine| {
        drain_pending(&engine.shared)?;
        let shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
        shell.text(handle).map(|text| text.to_owned())
    })
}

#[no_mangle]
pub extern "C" fn bubble_node_box(
    shell: *mut BubbleShell,
    handle: u32,
    x: *mut f32,
    y: *mut f32,
    w: *mut f32,
    h: *mut f32,
) -> i32 {
    let result = catch(|| {
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        let shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
        let (nx, ny, nw, nh) = shell.node_box(handle)?;
        unsafe {
            if !x.is_null() {
                *x = nx;
            }
            if !y.is_null() {
                *y = ny;
            }
            if !w.is_null() {
                *w = nw;
            }
            if !h.is_null() {
                *h = nh;
            }
        }
        Ok(())
    });
    finish(shell, result)
}

#[no_mangle]
pub extern "C" fn bubble_scroll_x_of(shell: *mut BubbleShell, handle: u32) -> f32 {
    scroll_of(shell, handle).map_or(f32::NAN, |(x, _)| x)
}

#[no_mangle]
pub extern "C" fn bubble_scroll_y_of(shell: *mut BubbleShell, handle: u32) -> f32 {
    scroll_of(shell, handle).map_or(f32::NAN, |(_, y)| y)
}

fn scroll_of(shell: *mut BubbleShell, handle: u32) -> Option<(f32, f32)> {
    let engine = engine(shell).ok()?;
    drain_pending(&engine.shared).ok()?;
    let shell = engine.shared.shell.lock().ok()?;
    shell.scroll_of(handle).ok()
}

#[no_mangle]
pub extern "C" fn bubble_eval(
    shell: *mut BubbleShell,
    source: *const c_char,
    buf: *mut c_char,
    cap: u32,
) -> i32 {
    let result = catch(|| {
        let engine = engine(shell)?;
        let source = cstr(source)?;
        let (ack_tx, ack_rx) = std::sync::mpsc::channel();
        engine
            .tx
            .send(Cmd::Eval {
                source,
                ack: Some(ack_tx),
            })
            .map_err(|_| "script thread is gone".to_string())?;
        let text = ack_rx
            .recv()
            .map_err(|_| "script thread dropped eval".to_string())??;
        write_buf(buf, cap, &text);
        Ok(())
    });
    finish(shell, result)
}

#[no_mangle]
pub extern "C" fn bubble_post_eval(shell: *mut BubbleShell, source: *const c_char) {
    let result = catch(|| {
        let engine = engine(shell)?;
        let source = cstr(source)?;
        engine
            .tx
            .send(Cmd::Eval { source, ack: None })
            .map_err(|_| "script thread is gone".to_string())?;
        Ok(())
    });
    let _ = finish(shell, result);
}

#[no_mangle]
pub extern "C" fn bubble_generation(shell: *mut BubbleShell) -> u32 {
    engine(shell)
        .map(|engine| engine.shared.generation.load(Ordering::Relaxed))
        .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn bubble_faults(shell: *mut BubbleShell) -> u32 {
    engine(shell)
        .map(|engine| engine.shared.faults.load(Ordering::Relaxed))
        .unwrap_or(0)
}

/// Whether timers, animations, queued UI calls, or pending gestures still
/// need frame ticks. Drains pending op batches first, so a queued JS→UI call
/// counts once applied.
#[no_mangle]
pub extern "C" fn bubble_pending(shell: *mut BubbleShell) -> i32 {
    let result = catch(|| {
        let engine = engine(shell)?;
        drain_pending(&engine.shared)?;
        let shell = lock_or(&engine.shared.shell, "tree lock poisoned")?;
        Ok(i32::from(shell.pending()))
    });
    result.unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn bubble_status(shell: *mut BubbleShell, buf: *mut c_char, cap: u32) -> i32 {
    copy_text(shell, buf, cap, |engine| {
        engine
            .shared
            .status
            .lock()
            .map(|status| status.clone())
            .map_err(|_| "status lock poisoned".into())
    })
}

#[no_mangle]
pub extern "C" fn bubble_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(unsafe { CString::from_raw(s) });
    }
}

/// Borrowed last error for an int-returning call, or null.
/// Invalidated by the next fallible call on `shell` and by `bubble_destroy`.
#[no_mangle]
pub extern "C" fn bubble_last_error(shell: *mut BubbleShell) -> *const c_char {
    let Ok(shell) = shell_ref(shell) else {
        return std::ptr::null();
    };
    // UI-thread faults have no return code; the tree drain records the newest
    // message in the shared slot. Read it before taking the shell slot so the
    // two locks never nest.
    let shared_error = shell
        .shared
        .last_error
        .lock()
        .ok()
        .filter(|shared| !shared.is_empty())
        .map(|shared| shared.replace('\0', " "));
    let Ok(mut slot) = shell.last_error.lock() else {
        return std::ptr::null();
    };
    if slot.is_none() {
        if let Some(message) = shared_error {
            *slot = CString::new(message).ok();
        }
    }
    slot.as_ref()
        .map(|text| text.as_ptr())
        .unwrap_or(std::ptr::null())
}

/// Returns 1 when the loaded bundle has registered a back handler, 0 otherwise.
#[no_mangle]
pub extern "C" fn bubble_has_back_handler(shell: *mut BubbleShell) -> i32 {
    engine(shell)
        .map(|engine| engine.shared.has_back_handler.load(Ordering::Relaxed) as i32)
        .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn bubble_poll_request(
    shell: *mut BubbleShell,
    id: *mut u32,
    method: *mut c_char,
    method_cap: u32,
    params: *mut c_char,
    params_cap: u32,
) -> i32 {
    let result = catch(|| {
        let engine = engine(shell)?;
        let mut queue = engine
            .shared
            .requests
            .lock()
            .map_err(|_| "request lock poisoned".to_string())?;
        let Some(request) = queue.pop_front() else {
            return Ok(0);
        };
        if !id.is_null() {
            unsafe { *id = request.id };
        }
        write_buf(method, method_cap, &request.method);
        write_buf(params, params_cap, &request.params);
        Ok(1)
    });
    match result {
        Ok(code) => code,
        Err(err) => {
            store_error(shell, &err);
            -1
        }
    }
}

#[no_mangle]
pub extern "C" fn bubble_finish_request(
    shell: *mut BubbleShell,
    id: u32,
    ok: i32,
    result: *const c_char,
) {
    let settled = catch(|| {
        let engine = engine(shell)?;
        let payload = if result.is_null() {
            String::new()
        } else {
            cstr(result)?
        };
        engine
            .tx
            .send(Cmd::Settle {
                id,
                ok: ok != 0,
                payload,
            })
            .map_err(|_| "script thread is gone".to_string())?;
        Ok(())
    });
    let _ = finish(shell, settled);
}

#[repr(C)]
pub struct BubbleHost {
    pub measure: Option<MeasureFn>,
    pub ctx: *mut c_void,
    pub layout: Option<LayoutFn>,
}

fn cstr(ptr: *const c_char) -> Result<String, String> {
    if ptr.is_null() {
        return Err("string pointer is null".into());
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map(|text| text.to_owned())
        .map_err(|_| "string is not utf-8".into())
}

fn write_buf(buf: *mut c_char, cap: u32, text: &str) {
    if buf.is_null() || cap == 0 {
        return;
    }
    let bytes = text.as_bytes();
    let take = bytes.len().min(cap as usize - 1);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf.cast(), take);
        *buf.add(take) = 0;
    }
}

fn copy_text(
    shell: *mut BubbleShell,
    buf: *mut c_char,
    cap: u32,
    body: impl FnOnce(EngineRef) -> Result<String, String>,
) -> i32 {
    let result = catch(|| {
        let engine = engine(shell)?;
        let text = body(engine)?;
        write_buf(buf, cap, &text);
        Ok(())
    });
    finish(shell, result)
}

fn fallback_measure(text: &str, font_size: f32, line_height: f32) -> (f32, f32) {
    if text.is_empty() {
        return (0.0, 0.0);
    }
    let width = text.chars().count() as f32 * font_size * 0.5;
    let line = if line_height > 0.0 {
        line_height
    } else {
        font_size * 1.25
    };
    (width, line)
}

#[cfg(target_os = "android")]
mod android {
    use std::os::raw::c_void;
    use std::ptr;
    use std::sync::OnceLock;

    use jni::objects::{JByteArray, JClass, JObject, JString, JValue};
    use jni::sys::{jfloat, jint, jlong};
    use jni::{JNIEnv, JavaVM};

    use super::BubbleHost;

    static VM: OnceLock<JavaVM> = OnceLock::new();

    pub(super) fn layout_snapshot(
        req: &bubble_core::LayoutRequest<'_>,
    ) -> Option<Result<bubble_core::LayoutSnapshot, String>> {
        let vm = VM.get()?;
        let mut env = vm.get_env().ok()?;
        Some(invoke_layout(&mut env, req))
    }

    fn invoke_layout(
        env: &mut JNIEnv<'_>,
        req: &bubble_core::LayoutRequest<'_>,
    ) -> Result<bubble_core::LayoutSnapshot, String> {
        let text = env
            .byte_array_from_slice(req.text.as_bytes())
            .map_err(|err| err.to_string())?;
        let value = env
            .call_static_method(
                "dev/bubble/shell/BubbleView",
                "layoutSnapshot",
                "([BFFFFIIJ)[B",
                &[
                    JValue::Object(&text),
                    JValue::Float(req.key.width),
                    JValue::Float(req.key.font_size),
                    JValue::Float(req.key.line_height),
                    JValue::Float(req.key.density),
                    JValue::Int(i32::from(req.key.font_weight)),
                    JValue::Int(i32::from(req.key.align)),
                    JValue::Long(req.key.revision as i64),
                ],
            )
            .map_err(|err| err.to_string())?;
        let array = value.l().map_err(|err| err.to_string())?;
        let bytes = env
            .convert_byte_array(JByteArray::from(array))
            .map_err(|err| err.to_string())?;
        let snapshot = bubble_core::decode_layout(&bytes)?;
        if !snapshot.matches(&req.key) {
            return Err("stale layout snapshot".into());
        }
        Ok(snapshot)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_create<'a>(
        env: JNIEnv<'a>,
        _class: JClass<'a>,
    ) -> jlong {
        if let Ok(vm) = env.get_java_vm() {
            let _ = VM.set(vm);
        }
        super::bubble_create(ptr::null()) as jlong
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_destroy<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
    ) {
        super::bubble_destroy(shell as *mut super::BubbleShell);
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_pollRequest<'a>(
        env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
    ) -> JString<'a> {
        let mut id = 0u32;
        let mut method = vec![0u8; 128];
        let mut params = vec![0u8; 8192];
        let code = super::bubble_poll_request(
            shell as *mut super::BubbleShell,
            &mut id,
            method.as_mut_ptr().cast(),
            method.len() as u32,
            params.as_mut_ptr().cast(),
            params.len() as u32,
        );
        if code != 1 {
            return JObject::null().into();
        }
        let method = cstr_lossy(&method);
        let params = cstr_lossy(&params);
        let body = serde_json::json!([id, method, params]).to_string();
        env.new_string(body)
            .unwrap_or_else(|_| JObject::null().into())
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_finishRequest<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        id: jint,
        ok: jint,
        result: JString<'a>,
    ) {
        let Ok(result) = env.get_string(&result) else {
            return;
        };
        let result = super::CString_from(&result.to_string_lossy());
        super::bubble_finish_request(
            shell as *mut super::BubbleShell,
            id as u32,
            ok,
            result.as_ptr(),
        );
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_setParams<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        json: JString<'a>,
    ) -> JString<'a> {
        let Ok(json) = env.get_string(&json) else {
            return env
                .new_string("params are not a string")
                .unwrap_or_else(|_| JObject::null().into());
        };
        let json = super::CString_from(&json.to_string_lossy());
        let err = super::bubble_set_params(shell as *mut super::BubbleShell, json.as_ptr());
        jni_error(&mut env, err)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_setStorageDir<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        path: JString<'a>,
    ) -> JString<'a> {
        let owned;
        let path_ptr = if path.is_null() {
            std::ptr::null()
        } else {
            let Ok(path) = env.get_string(&path) else {
                return env
                    .new_string("storage path is not a string")
                    .unwrap_or_else(|_| JObject::null().into());
            };
            owned = super::CString_from(&path.to_string_lossy());
            owned.as_ptr()
        };
        let err = super::bubble_set_storage_dir(shell as *mut super::BubbleShell, path_ptr);
        jni_error(&mut env, err)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_load<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        manifest: JString<'a>,
        entry: JByteArray<'a>,
    ) -> JString<'a> {
        let Ok(manifest) = env.get_string(&manifest) else {
            return env
                .new_string("manifest is not a string")
                .unwrap_or_else(|_| JObject::null().into());
        };
        let manifest = super::CString_from(&manifest.to_string_lossy());
        let Ok(bytes) = env.convert_byte_array(entry) else {
            return env
                .new_string("entry is not a byte array")
                .unwrap_or_else(|_| JObject::null().into());
        };
        let err = super::bubble_load(
            shell as *mut super::BubbleShell,
            manifest.as_ptr(),
            bytes.as_ptr().cast(),
            bytes.len() as u32,
        );
        jni_error(&mut env, err)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_frame<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        width: jfloat,
        height: jfloat,
    ) -> JByteArray<'a> {
        let mut ptr = ptr::null();
        let mut len = 0u32;
        if super::bubble_frame(
            shell as *mut super::BubbleShell,
            width,
            height,
            &mut ptr,
            &mut len,
        ) != 0
            || ptr.is_null()
        {
            return env
                .new_byte_array(0)
                .unwrap_or_else(|_| JObject::null().into());
        }
        let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
        let Ok(array) = env.byte_array_from_slice(bytes) else {
            return JObject::null().into();
        };
        array
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_pointer(
        _env: JNIEnv,
        _class: JClass,
        shell: jlong,
        action: jint,
        id: jint,
        x: jfloat,
        y: jfloat,
        time_ms: jni::sys::jdouble,
        pointer_kind: jint,
        buttons: jint,
    ) -> jint {
        super::bubble_pointer(
            shell as *mut super::BubbleShell,
            action as u32,
            id as u32,
            x,
            y,
            time_ms,
            pointer_kind as u32,
            buttons as u32,
        )
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_visualPointer(
        _env: JNIEnv,
        _class: JClass,
        shell: jlong,
        action: jint,
        id: jint,
        x: jfloat,
        y: jfloat,
        time_ms: jni::sys::jdouble,
        pointer_kind: jint,
        buttons: jint,
    ) -> jint {
        super::bubble_visual_pointer(
            shell as *mut super::BubbleShell,
            action as u32,
            id as u32,
            x,
            y,
            time_ms,
            pointer_kind as u32,
            buttons as u32,
        )
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_tick(
        _env: JNIEnv,
        _class: JClass,
        shell: jlong,
        time_ms: jni::sys::jdouble,
    ) -> jint {
        super::bubble_tick(shell as *mut super::BubbleShell, time_ms)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_scroll<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        x: jfloat,
        y: jfloat,
        dx: jfloat,
        dy: jfloat,
    ) {
        super::bubble_scroll(shell as *mut super::BubbleShell, x, y, dx, dy);
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_edit<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        action: jint,
        text: JByteArray<'a>,
        a: jint,
        b: jint,
    ) -> jint {
        let bytes = env.convert_byte_array(text).unwrap_or_default();
        super::bubble_edit(
            shell as *mut super::BubbleShell,
            action as u32,
            bytes.as_ptr(),
            bytes.len() as u32,
            a as u32,
            b as u32,
        )
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_editBatchBegin<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        session: jlong,
    ) -> jint {
        super::bubble_edit_batch_begin(shell as *mut super::BubbleShell, session as u64)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_editBatchEnd<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        session: jlong,
    ) -> jint {
        super::bubble_edit_batch_end(shell as *mut super::BubbleShell, session as u64)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_editBatchAbort<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        session: jlong,
    ) -> jint {
        super::bubble_edit_batch_abort(shell as *mut super::BubbleShell, session as u64)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_editSnapshot<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
    ) -> jni::objects::JLongArray<'a> {
        let (mut session, mut revision) = (0u64, 0u64);
        let (mut anchor, mut active) = (0u32, 0u32);
        let (mut comp_start, mut comp_end, mut composing) = (0u32, 0u32, 0u32);
        let code = super::bubble_edit_snapshot(
            shell as *mut super::BubbleShell,
            &mut session,
            &mut revision,
            &mut anchor,
            &mut active,
            &mut comp_start,
            &mut comp_end,
            &mut composing,
        );
        if code != 0 {
            return env.new_long_array(0).expect("long array");
        }
        let values = [
            session as i64,
            revision as i64,
            anchor as i64,
            active as i64,
            comp_start as i64,
            comp_end as i64,
            composing as i64,
        ];
        let array = env.new_long_array(7).expect("long array");
        let _ = env.set_long_array_region(&array, 0, &values);
        array
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_editCommit<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        session: jlong,
        text: JByteArray<'a>,
        new_cursor_position: jint,
    ) -> jint {
        let bytes = env.convert_byte_array(text).unwrap_or_default();
        super::bubble_edit_commit(
            shell as *mut super::BubbleShell,
            session as u64,
            bytes.as_ptr(),
            bytes.len() as u32,
            new_cursor_position,
        )
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_editCompose<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        session: jlong,
        text: JByteArray<'a>,
        new_cursor_position: jint,
    ) -> jint {
        let bytes = env.convert_byte_array(text).unwrap_or_default();
        super::bubble_edit_compose(
            shell as *mut super::BubbleShell,
            session as u64,
            bytes.as_ptr(),
            bytes.len() as u32,
            new_cursor_position,
        )
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_editUndo<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        session: jlong,
    ) -> jint {
        super::bubble_edit_undo(shell as *mut super::BubbleShell, session as u64)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_editRedo<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        session: jlong,
    ) -> jint {
        super::bubble_edit_redo(shell as *mut super::BubbleShell, session as u64)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_editDeleteSurrounding<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        session: jlong,
        before: jint,
        after: jint,
    ) -> jint {
        super::bubble_edit_delete_surrounding(
            shell as *mut super::BubbleShell,
            session as u64,
            before as u32,
            after as u32,
        )
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_editDeleteSurroundingCodePoints<
        'a,
    >(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        session: jlong,
        before: jint,
        after: jint,
    ) -> jint {
        super::bubble_edit_delete_surrounding_code_points(
            shell as *mut super::BubbleShell,
            session as u64,
            before as u32,
            after as u32,
        )
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_editSetComposition<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        session: jlong,
        start: jint,
        end: jint,
    ) -> jint {
        super::bubble_edit_set_composition(
            shell as *mut super::BubbleShell,
            session as u64,
            start as u32,
            end as u32,
        )
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_editFinishComposition<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        session: jlong,
    ) -> jint {
        super::bubble_edit_finish_composition(shell as *mut super::BubbleShell, session as u64)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_editSelect<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        session: jlong,
        anchor: jint,
        active: jint,
    ) -> jint {
        super::bubble_edit_select(
            shell as *mut super::BubbleShell,
            session as u64,
            anchor as u32,
            active as u32,
        )
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_unitAt<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        x: jfloat,
        y: jfloat,
    ) -> jint {
        let mut unit = u32::MAX;
        if super::bubble_unit_at(shell as *mut super::BubbleShell, x, y, &mut unit) == 0 {
            unit as jint
        } else {
            -1
        }
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_fieldAt<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        x: jfloat,
        y: jfloat,
    ) -> jint {
        super::bubble_field_at(shell as *mut super::BubbleShell, x, y) as jint
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_focusAt<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        x: jfloat,
        y: jfloat,
    ) -> jint {
        super::bubble_focus_at(shell as *mut super::BubbleShell, x, y) as jint
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_fieldFlags<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        handle: jint,
    ) -> jint {
        super::bubble_field_flags(shell as *mut super::BubbleShell, handle as u32) as jint
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_scrollXOf<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        handle: jint,
    ) -> jfloat {
        super::bubble_scroll_x_of(shell as *mut super::BubbleShell, handle as u32)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_scrollYOf<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        handle: jint,
    ) -> jfloat {
        super::bubble_scroll_y_of(shell as *mut super::BubbleShell, handle as u32)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_setVisible<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        visible: jint,
    ) {
        super::bubble_set_visible(shell as *mut super::BubbleShell, visible);
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_registerFont<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        family: JString<'a>,
        data: JByteArray<'a>,
    ) {
        let Ok(family) = env.get_string(&family) else {
            return;
        };
        let family = super::CString_from(&family.to_string_lossy());
        let Ok(bytes) = env.convert_byte_array(data) else {
            return;
        };
        super::bubble_register_font(
            shell as *mut super::BubbleShell,
            family.as_ptr(),
            bytes.as_ptr().cast(),
            bytes.len() as u32,
        );
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_nodeText<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        handle: jint,
    ) -> JString<'a> {
        let mut buf = [0 as std::os::raw::c_char; 65536];
        let code = super::bubble_node_text(
            shell as *mut super::BubbleShell,
            handle as u32,
            buf.as_mut_ptr(),
            buf.len() as u32,
        );
        let text = if code == 0 {
            unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }
                .to_string_lossy()
                .into_owned()
        } else {
            String::new()
        };
        env.new_string(text)
            .unwrap_or_else(|_| env.new_string("").unwrap())
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_key<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        action: jint,
        time_ms: jni::sys::jdouble,
    ) -> jint {
        super::bubble_key(shell as *mut super::BubbleShell, action as u32, time_ms)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_semantics<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
    ) -> JString<'a> {
        let mut buf = vec![0 as std::os::raw::c_char; 262144];
        let code = super::bubble_semantics(
            shell as *mut super::BubbleShell,
            buf.as_mut_ptr(),
            buf.len() as u32,
        );
        let text = if code == 0 {
            unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }
                .to_string_lossy()
                .into_owned()
        } else {
            String::new()
        };
        env.new_string(text)
            .unwrap_or_else(|_| env.new_string("").unwrap())
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_a11yAction<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        handle: jint,
        action: jint,
        text: JString<'a>,
    ) -> jint {
        let Ok(text) = env.get_string(&text) else {
            return 1;
        };
        let bytes = text.to_string_lossy();
        super::bubble_a11y_action(
            shell as *mut super::BubbleShell,
            handle as u32,
            action as u32,
            bytes.as_ptr(),
            bytes.len() as u32,
        )
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_editMenu<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        action: jint,
        clip: JString<'a>,
    ) -> JString<'a> {
        let Ok(clip) = env.get_string(&clip) else {
            return env.new_string("").unwrap();
        };
        let bytes = clip.to_string_lossy();
        let mut out = vec![0 as std::os::raw::c_char; 65536];
        let _ = super::bubble_edit_menu(
            shell as *mut super::BubbleShell,
            action as u32,
            bytes.as_ptr(),
            bytes.len() as u32,
            out.as_mut_ptr(),
            out.len() as u32,
        );
        let text = unsafe { std::ffi::CStr::from_ptr(out.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        env.new_string(text)
            .unwrap_or_else(|_| env.new_string("").unwrap())
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_selectionHandles<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
    ) -> jni::objects::JFloatArray<'a> {
        let (mut sx, mut sy, mut sw, mut sh) = (0.0f32, 0.0, 0.0, 0.0);
        let (mut ex, mut ey, mut ew, mut eh) = (0.0f32, 0.0, 0.0, 0.0);
        let array = if super::bubble_selection_handles(
            shell as *mut super::BubbleShell,
            &mut sx,
            &mut sy,
            &mut sw,
            &mut sh,
            &mut ex,
            &mut ey,
            &mut ew,
            &mut eh,
        ) == 0
        {
            let array = env.new_float_array(8).expect("float array");
            let _ = env.set_float_array_region(&array, 0, &[sx, sy, sw, sh, ex, ey, ew, eh]);
            array
        } else {
            env.new_float_array(0).expect("float array")
        };
        array
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_focused<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
    ) -> jint {
        super::bubble_focused(shell as *mut super::BubbleShell) as jint
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_focusedText<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
    ) -> JString<'a> {
        let mut buf = vec![0 as std::os::raw::c_char; 65536];
        let handle = super::bubble_focused(shell as *mut super::BubbleShell);
        let text = if handle == 0 {
            String::new()
        } else {
            let _ = super::bubble_node_text(
                shell as *mut super::BubbleShell,
                handle,
                buf.as_mut_ptr(),
                buf.len() as u32,
            );
            unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }
                .to_string_lossy()
                .into_owned()
        };
        env.new_string(text)
            .unwrap_or_else(|_| env.new_string("").unwrap())
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_selection<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
    ) -> jni::objects::JIntArray<'a> {
        let (mut start, mut end, mut mark_start, mut mark_end) = (0u32, 0, 0, 0);
        let _ = super::bubble_selection(
            shell as *mut super::BubbleShell,
            &mut start,
            &mut end,
            &mut mark_start,
            &mut mark_end,
        );
        let array = env.new_int_array(4).expect("int array");
        let _ = env.set_int_array_region(
            &array,
            0,
            &[start as i32, end as i32, mark_start as i32, mark_end as i32],
        );
        array
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_caret<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
    ) -> jni::objects::JFloatArray<'a> {
        let (mut x, mut y, mut w, mut h) = (0.0f32, 0.0, 0.0, 0.0);
        let array = if super::bubble_caret(
            shell as *mut super::BubbleShell,
            &mut x,
            &mut y,
            &mut w,
            &mut h,
        ) == 0
        {
            let array = env.new_float_array(4).expect("float array");
            let _ = env.set_float_array_region(&array, 0, &[x, y, w, h]);
            array
        } else {
            env.new_float_array(0).expect("float array")
        };
        array
    }
    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_nodeBox<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        handle: jint,
    ) -> jni::objects::JFloatArray<'a> {
        let (mut x, mut y, mut w, mut h) = (0.0f32, 0.0, 0.0, 0.0);
        let _ = super::bubble_node_box(
            shell as *mut super::BubbleShell,
            handle as u32,
            &mut x,
            &mut y,
            &mut w,
            &mut h,
        );
        let array = env.new_float_array(4).expect("float array");
        let _ = env.set_float_array_region(&array, 0, &[x, y, w, h]);
        array
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_postEval<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        source: JString<'a>,
    ) {
        let Ok(source) = env.get_string(&source) else {
            return;
        };
        let source = super::CString_from(&source.to_string_lossy());
        super::bubble_post_eval(shell as *mut super::BubbleShell, source.as_ptr());
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_faults<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
    ) -> jint {
        super::bubble_faults(shell as *mut super::BubbleShell) as jint
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_generation<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
    ) -> jint {
        super::bubble_generation(shell as *mut super::BubbleShell) as jint
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_fontEpoch<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
    ) -> jint {
        super::bubble_font_epoch(shell as *mut super::BubbleShell) as jint
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_hasBackHandler<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
    ) -> jint {
        super::bubble_has_back_handler(shell as *mut super::BubbleShell)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_send<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        shell: jlong,
        name: JString<'a>,
        payload: JString<'a>,
    ) -> JString<'a> {
        let Ok(name) = env.get_string(&name) else {
            return env
                .new_string("event name is not a string")
                .unwrap_or_else(|_| JObject::null().into());
        };
        let Ok(payload) = env.get_string(&payload) else {
            return env
                .new_string("event payload is not a string")
                .unwrap_or_else(|_| JObject::null().into());
        };
        let name = super::CString_from(&name.to_string_lossy());
        let payload = super::CString_from(&payload.to_string_lossy());
        let err = super::bubble_send(
            shell as *mut super::BubbleShell,
            name.as_ptr(),
            payload.as_ptr(),
        );
        jni_error(&mut env, err)
    }

    fn jni_error<'a>(env: &mut JNIEnv<'a>, err: *mut std::os::raw::c_char) -> JString<'a> {
        if err.is_null() {
            return JObject::null().into();
        }
        let text = unsafe { std::ffi::CStr::from_ptr(err) }
            .to_string_lossy()
            .into_owned();
        super::bubble_string_free(err);
        env.new_string(text)
            .unwrap_or_else(|_| JObject::null().into())
    }

    fn cstr_lossy(bytes: &[u8]) -> String {
        let end = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len());
        String::from_utf8_lossy(&bytes[..end]).into_owned()
    }

    #[allow(dead_code)]
    fn _host_size() -> usize {
        std::mem::size_of::<BubbleHost>() + std::mem::size_of::<*mut c_void>()
    }

    #[allow(dead_code)]
    fn _unused_long() -> jlong {
        0
    }
}

#[cfg(target_os = "android")]
fn CString_from(text: &str) -> std::ffi::CString {
    std::ffi::CString::new(text.replace('\0', " "))
        .unwrap_or_else(|_| std::ffi::CString::new("").unwrap())
}
