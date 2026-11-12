//! C ABI around one QuickJS script thread and one retained tree.
//!
//! `bubble_frame`, hit testing, and scrolling run on the caller thread.
//! Script evaluation does not. The script thread only produces op batches;
//! the caller/frame thread owns the `Shell` and applies pending batches.
//! A one-second interrupt rebuilds the context and remounts the loaded bundle.

mod script;

use std::collections::VecDeque;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use bubble_core::{
    encode_edit_events, encode_events, encode_paint, EditCommand, EditEvent, GestureEvent,
    PointerAction, PointerInput, Shell,
};

use script::Cmd;

pub struct HostRequest {
    pub id: u32,
    pub method: String,
    pub params: String,
}

struct HostFns {
    measure: Option<MeasureFn>,
    ctx: usize,
}

unsafe impl Send for HostFns {}
unsafe impl Sync for HostFns {}

type MeasureFn = extern "C" fn(
    *mut c_void,
    *const u8,
    u32,
    f32,
    f32,
    u16,
    f32,
    i32,
    *mut f32,
    *mut f32,
);

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

struct Engine {
    shared: Arc<Shared>,
    tx: Sender<Cmd>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

static ENGINE: Mutex<Option<Engine>> = Mutex::new(None);
static ERROR_C: Mutex<Option<CString>> = Mutex::new(None);

fn engine() -> Result<EngineRef, String> {
    let guard = ENGINE
        .lock()
        .map_err(|_| "engine lock poisoned".to_string())?;
    let engine = guard.as_ref().ok_or("shell is not created")?;
    Ok(EngineRef {
        shared: Arc::clone(&engine.shared),
        tx: engine.tx.clone(),
    })
}

struct EngineRef {
    shared: Arc<Shared>,
    tx: Sender<Cmd>,
}

fn store_error(message: &str) {
    if let Ok(mut slot) = ERROR_C.lock() {
        *slot = CString::new(message.replace('\0', " ")).ok();
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
    let (events, edits) = {
        let mut queue = shared
            .pending
            .lock()
            .map_err(|_| "op queue poisoned".to_string())?;
        let mut shell = shared
            .shell
            .lock()
            .map_err(|_| "tree lock poisoned".to_string())?;
        if reset {
            *shell = Shell::blank()?;
        }
        while let Some(bytes) = queue.pop_front() {
            shell.apply_ops(&bytes)?;
        }
        (shell.take_gesture_events(), shell.take_edit_events())
    };
    send_events(shared, events);
    send_edits(shared, edits);
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

#[no_mangle]
pub extern "C" fn bubble_create(host: *const BubbleHost) -> i32 {
    let result = catch(|| {
        let mut slot = ENGINE
            .lock()
            .map_err(|_| "engine lock poisoned".to_string())?;
        if slot.is_some() {
            return Err("a shell already exists".into());
        }
        let (measure, ctx) = unsafe {
            if host.is_null() {
                (None, 0)
            } else {
                ((*host).measure, (*host).ctx as usize)
            }
        };
        let shell = Shell::blank().map_err(|err| err)?;
        let shared = Arc::new(Shared {
            shell: ShellSlot(Mutex::new(shell)),
            pending: Mutex::new(VecDeque::new()),
            reset: AtomicBool::new(false),
            viewport: Mutex::new((0.0, 0.0)),
            host: HostFns { measure, ctx },
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
        });
        let (tx, thread) = script::start(Arc::clone(&shared));
        if let Ok(mut slot) = shared.commands.lock() {
            *slot = Some(tx.clone());
        }
        *slot = Some(Engine {
            shared,
            tx,
            thread: Mutex::new(Some(thread)),
        });
        Ok(())
    });
    finish(result)
}

#[no_mangle]
pub extern "C" fn bubble_destroy() {
    let taken = ENGINE.lock().ok().and_then(|mut slot| slot.take());
    if let Some(engine) = taken {
        script::stop_watchdog();
        let _ = engine.tx.send(Cmd::Shutdown);
        if let Ok(mut thread) = engine.thread.lock() {
            if let Some(thread) = thread.take() {
                let _ = thread.join();
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn bubble_load(
    manifest_json: *const c_char,
    entry: *const u8,
    entry_len: u32,
) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
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
            let mut shell = engine
                .shared
                .shell
                .lock()
                .map_err(|_| "tree lock poisoned".to_string())?;
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
    finish(result)
}

#[no_mangle]
pub extern "C" fn bubble_frame(
    width: f32,
    height: f32,
    paint: *mut *const u8,
    paint_len: *mut u32,
) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        if let Ok(mut viewport) = engine.shared.viewport.lock() {
            *viewport = (width, height);
        }
        let mut shell = engine
            .shared
            .shell
            .lock()
            .map_err(|_| "tree lock poisoned".to_string())?;
        shell.set_viewport(width, height);
        let host = &engine.shared.host;
        shell.layout(&mut |text, font_size, line_height, font_weight, max_width| {
            if let Some(measure) = host.measure {
                let mut w = 0.0f32;
                let mut h = 0.0f32;
                let has_max = max_width.is_some();
                measure(
                    host.ctx as *mut c_void,
                    text.as_ptr(),
                    text.len() as u32,
                    font_size,
                    line_height,
                    font_weight,
                    max_width.unwrap_or(0.0),
                    i32::from(has_max),
                    &mut w,
                    &mut h,
                );
                (w, h)
            } else {
                fallback_measure(text, font_size, line_height)
            }
        })?;
        let bytes = encode_paint(&shell.paint());
        drop(shell);
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
    finish(result)
}

/// One normalized pointer sample. `action` 0 down, 1 move, 2 up, 3 cancel.
/// `pointer_kind` 0 touch, 1 mouse, 2 pen. `buttons` bit 0 is the primary
/// button. Time is the host's monotonic clock in milliseconds.
#[no_mangle]
pub extern "C" fn bubble_pointer(
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
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let events = {
            let mut shell = engine
                .shared
                .shell
                .lock()
                .map_err(|_| "tree lock poisoned".to_string())?;
            shell.pointer(PointerInput {
                action,
                id,
                x,
                y,
                time_ms,
                buttons,
                pointer_kind: pointer_kind.min(255) as u8,
                path: Vec::new(),
            });
            shell.take_gesture_events()
        };
        send_events(&engine.shared, events);
        Ok(())
    });
    finish(result)
}

/// UI-thread timer tick for long press and the double-tap window.
#[no_mangle]
pub extern "C" fn bubble_tick(time_ms: f64) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let events = {
            let mut shell = engine
                .shared
                .shell
                .lock()
                .map_err(|_| "tree lock poisoned".to_string())?;
            shell.advance_gestures(time_ms);
            shell.take_gesture_events()
        };
        send_events(&engine.shared, events);
        Ok(())
    });
    finish(result)
}

/// Mutate the focused field, or focus one. `action` 0 focuses `a` (`0` blurs).
/// 1 inserts UTF-8 text. 2 backspace. 3 delete forward. 4 sets the UTF-16
/// selection to `a`..`b`. 5 starts composition. 6 updates it. 7 ends it.
#[no_mangle]
pub extern "C" fn bubble_edit(
    action: u32,
    text: *const u8,
    text_len: u32,
    a: u32,
    b: u32,
) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let bytes = if text.is_null() || text_len == 0 {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(text, text_len as usize) }.to_vec()
        };
        let text = String::from_utf8(bytes).map_err(|_| "edit text is not utf-8".to_string())?;
        let command = match action {
            0 => None,
            1 => Some(EditCommand::Insert(text)),
            2 => Some(EditCommand::Backspace),
            3 => Some(EditCommand::DeleteForward),
            4 => Some(EditCommand::SetSelection { start: a, end: b }),
            5 => Some(EditCommand::CompositionStart),
            6 => Some(EditCommand::CompositionUpdate(text)),
            7 => Some(EditCommand::CompositionEnd(text)),
            8 => Some(EditCommand::Undo),
            9 => Some(EditCommand::Redo),
            other => return Err(format!("bad edit action {other}")),
        };
        let events = {
            let mut shell = engine
                .shared
                .shell
                .lock()
                .map_err(|_| "tree lock poisoned".to_string())?;
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
    finish(result)
}

/// Open a nested edit batch on the focused field. Events are published by
/// `bubble_edit_batch_end`.
#[no_mangle]
pub extern "C" fn bubble_edit_batch_begin(session: u64) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let events = {
            let mut shell = engine
                .shared
                .shell
                .lock()
                .map_err(|_| "tree lock poisoned".to_string())?;
            check_edit_session(&shell, session)?;
            shell.begin_edit_batch()?;
            shell.take_edit_events()
        };
        send_edits(&engine.shared, events);
        Ok(())
    });
    finish(result)
}

/// Close one batch level. The outermost level emits one delta event.
#[no_mangle]
pub extern "C" fn bubble_edit_batch_end(session: u64) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let events = {
            let mut shell = engine
                .shared
                .shell
                .lock()
                .map_err(|_| "tree lock poisoned".to_string())?;
            check_edit_session(&shell, session)?;
            shell.end_edit_batch()?;
            shell.take_edit_events()
        };
        send_edits(&engine.shared, events);
        Ok(())
    });
    finish(result)
}

/// Read the focused editing snapshot. `comp_end` is `u32::MAX` when no
/// composition range is active; `composing` is 0 or 1.
#[no_mangle]
pub extern "C" fn bubble_edit_snapshot(
    session: *mut u64,
    revision: *mut u64,
    anchor: *mut u32,
    active: *mut u32,
    comp_start: *mut u32,
    comp_end: *mut u32,
    composing: *mut u32,
) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let state = {
            let shell = engine
                .shared
                .shell
                .lock()
                .map_err(|_| "tree lock poisoned".to_string())?;
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
    finish(result)
}

fn check_edit_session(shell: &Shell, session: u64) -> Result<(), String> {
    let state = shell.focused_state()?;
    if state.session.get() != session {
        return Err("stale editing session".into());
    }
    Ok(())
}

fn read_utf8(text: *const u8, len: u32) -> Result<String, String> {
    let bytes = if text.is_null() || len == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(text, len as usize) }.to_vec()
    };
    String::from_utf8(bytes).map_err(|_| "edit text is not utf-8".to_string())
}

/// Replace `start..end` with UTF-8 text and apply Android's
/// `newCursorPosition`. `composing` 1 keeps/starts composition.
#[no_mangle]
pub extern "C" fn bubble_edit_replace(
    session: u64,
    start: u32,
    end: u32,
    text: *const u8,
    text_len: u32,
    new_cursor_position: i32,
    composing: u32,
) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let text = read_utf8(text, text_len)?;
        let events = {
            let mut shell = engine
                .shared
                .shell
                .lock()
                .map_err(|_| "tree lock poisoned".to_string())?;
            check_edit_session(&shell, session)?;
            shell.replace_with_cursor(
                start,
                end,
                &text,
                new_cursor_position,
                composing != 0,
            )?;
            shell.take_edit_events()
        };
        send_edits(&engine.shared, events);
        Ok(())
    });
    finish(result)
}

#[no_mangle]
pub extern "C" fn bubble_edit_commit(
    session: u64,
    text: *const u8,
    text_len: u32,
    new_cursor_position: i32,
) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let text = read_utf8(text, text_len)?;
        let events = {
            let mut shell = engine
                .shared
                .shell
                .lock()
                .map_err(|_| "tree lock poisoned".to_string())?;
            check_edit_session(&shell, session)?;
            shell.commit_text(&text, new_cursor_position)?;
            shell.take_edit_events()
        };
        send_edits(&engine.shared, events);
        Ok(())
    });
    finish(result)
}

#[no_mangle]
pub extern "C" fn bubble_edit_compose(
    session: u64,
    text: *const u8,
    text_len: u32,
    new_cursor_position: i32,
) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let text = read_utf8(text, text_len)?;
        let events = {
            let mut shell = engine
                .shared
                .shell
                .lock()
                .map_err(|_| "tree lock poisoned".to_string())?;
            check_edit_session(&shell, session)?;
            shell.compose_text(&text, new_cursor_position)?;
            shell.take_edit_events()
        };
        send_edits(&engine.shared, events);
        Ok(())
    });
    finish(result)
}

#[no_mangle]
pub extern "C" fn bubble_edit_undo(session: u64) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let events = {
            let mut shell = engine
                .shared
                .shell
                .lock()
                .map_err(|_| "tree lock poisoned".to_string())?;
            check_edit_session(&shell, session)?;
            shell.edit(EditCommand::Undo)?;
            shell.take_edit_events()
        };
        send_edits(&engine.shared, events);
        Ok(())
    });
    finish(result)
}

#[no_mangle]
pub extern "C" fn bubble_edit_redo(session: u64) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let events = {
            let mut shell = engine
                .shared
                .shell
                .lock()
                .map_err(|_| "tree lock poisoned".to_string())?;
            check_edit_session(&shell, session)?;
            shell.edit(EditCommand::Redo)?;
            shell.take_edit_events()
        };
        send_edits(&engine.shared, events);
        Ok(())
    });
    finish(result)
}

/// Delete UTF-16 units around the focused selection.
#[no_mangle]
pub extern "C" fn bubble_edit_delete_surrounding(
    session: u64,
    before: u32,
    after: u32,
) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let events = {
            let mut shell = engine
                .shared
                .shell
                .lock()
                .map_err(|_| "tree lock poisoned".to_string())?;
            check_edit_session(&shell, session)?;
            shell.delete_surrounding(before, after)?;
            shell.take_edit_events()
        };
        send_edits(&engine.shared, events);
        Ok(())
    });
    finish(result)
}

/// Delete code points around the focused selection.
#[no_mangle]
pub extern "C" fn bubble_edit_delete_surrounding_code_points(
    session: u64,
    before: u32,
    after: u32,
) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let events = {
            let mut shell = engine
                .shared
                .shell
                .lock()
                .map_err(|_| "tree lock poisoned".to_string())?;
            check_edit_session(&shell, session)?;
            shell.delete_surrounding_code_points(before, after)?;
            shell.take_edit_events()
        };
        send_edits(&engine.shared, events);
        Ok(())
    });
    finish(result)
}

#[no_mangle]
pub extern "C" fn bubble_edit_set_composition(session: u64, start: u32, end: u32) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let events = {
            let mut shell = engine
                .shared
                .shell
                .lock()
                .map_err(|_| "tree lock poisoned".to_string())?;
            check_edit_session(&shell, session)?;
            shell.set_composition_region(start, end)?;
            shell.take_edit_events()
        };
        send_edits(&engine.shared, events);
        Ok(())
    });
    finish(result)
}

#[no_mangle]
pub extern "C" fn bubble_edit_finish_composition(session: u64) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let events = {
            let mut shell = engine
                .shared
                .shell
                .lock()
                .map_err(|_| "tree lock poisoned".to_string())?;
            check_edit_session(&shell, session)?;
            shell.finish_composition()?;
            shell.take_edit_events()
        };
        send_edits(&engine.shared, events);
        Ok(())
    });
    finish(result)
}

#[no_mangle]
pub extern "C" fn bubble_edit_select(session: u64, anchor: u32, active: u32) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let events = {
            let mut shell = engine
                .shared
                .shell
                .lock()
                .map_err(|_| "tree lock poisoned".to_string())?;
            check_edit_session(&shell, session)?;
            shell.edit(EditCommand::SetSelection {
                start: anchor,
                end: active,
            })?;
            shell.take_edit_events()
        };
        send_edits(&engine.shared, events);
        Ok(())
    });
    finish(result)
}

/// Abort any open edit batch without committing it.
#[no_mangle]
pub extern "C" fn bubble_edit_batch_abort(session: u64) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let events = {
            let mut shell = engine
                .shared
                .shell
                .lock()
                .map_err(|_| "tree lock poisoned".to_string())?;
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
    finish(result)
}

/// Deepest text field under the point, or `0`.
#[no_mangle]
pub extern "C" fn bubble_field_at(x: f32, y: f32) -> u32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let shell = engine
            .shared
            .shell
            .lock()
            .map_err(|_| "tree lock poisoned".to_string())?;
        Ok(shell.field_at(x, y))
    });
    result.unwrap_or(0)
}

/// Focus the field under the point and place the caret. Returns the handle, or 0.
#[no_mangle]
pub extern "C" fn bubble_focus_at(x: f32, y: f32) -> u32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let events = {
            let mut shell = engine
                .shared
                .shell
                .lock()
                .map_err(|_| "tree lock poisoned".to_string())?;
            let host = &engine.shared.host;
            let handle = shell.focus_point(x, y, &mut |text, font_size, line_height, font_weight, max_width| {
                if let Some(measure) = host.measure {
                    let mut w = 0.0f32;
                    let mut h = 0.0f32;
                    let has_max = max_width.is_some();
                    measure(
                        host.ctx as *mut c_void,
                        text.as_ptr(),
                        text.len() as u32,
                        font_size,
                        line_height,
                        font_weight,
                        max_width.unwrap_or(0.0),
                        i32::from(has_max),
                        &mut w,
                        &mut h,
                    );
                    (w, h)
                } else {
                    fallback_measure(text, font_size, line_height)
                }
            })?;
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
    x: *mut f32,
    y: *mut f32,
    w: *mut f32,
    h: *mut f32,
) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let shell = engine
            .shared
            .shell
            .lock()
            .map_err(|_| "tree lock poisoned".to_string())?;
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
    finish(result)
}

/// `bit0` multiline, `bit1` secure, bits 8..15 keyboard kind.
#[no_mangle]
pub extern "C" fn bubble_field_flags(handle: u32) -> u32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let shell = engine
            .shared
            .shell
            .lock()
            .map_err(|_| "tree lock poisoned".to_string())?;
        shell.field_flags(handle)
    });
    result.unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn bubble_focused() -> u32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let shell = engine
            .shared
            .shell
            .lock()
            .map_err(|_| "tree lock poisoned".to_string())?;
        Ok(shell.focused())
    });
    result.unwrap_or(0)
}

/// UTF-16 selection of the focused field. `mark_end` is `u32::MAX` when not composing.
#[no_mangle]
pub extern "C" fn bubble_selection(
    start: *mut u32,
    end: *mut u32,
    mark_start: *mut u32,
    mark_end: *mut u32,
) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let shell = engine
            .shared
            .shell
            .lock()
            .map_err(|_| "tree lock poisoned".to_string())?;
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
    finish(result)
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

#[no_mangle]
pub extern "C" fn bubble_unit_at(x: f32, y: f32, unit: *mut u32) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let shell = engine
            .shared
            .shell
            .lock()
            .map_err(|_| "tree lock poisoned".to_string())?;
        let host = &engine.shared.host;
        let Some(offset) = shell.unit_at(x, y, &mut |text, font_size, line_height, font_weight, max_width| {
            host_measure(host, text, font_size, line_height, font_weight, max_width)
        })?
        else {
            return Err("no focused field".into());
        };
        if !unit.is_null() {
            unsafe { *unit = offset };
        }
        Ok(())
    });
    finish(result)
}

#[no_mangle]
pub extern "C" fn bubble_caret_at(unit: u32, x: *mut f32, y: *mut f32, w: *mut f32, h: *mut f32) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let shell = engine
            .shared
            .shell
            .lock()
            .map_err(|_| "tree lock poisoned".to_string())?;
        let host = &engine.shared.host;
        let Some((cx, cy, cw, ch)) = shell.caret_rect_at(unit, &mut |text, font_size, line_height, font_weight, max_width| {
            host_measure(host, text, font_size, line_height, font_weight, max_width)
        })?
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
    finish(result)
}

#[no_mangle]
pub extern "C" fn bubble_selection_count() -> u32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let shell = engine
            .shared
            .shell
            .lock()
            .map_err(|_| "tree lock poisoned".to_string())?;
        Ok(shell.selection_rects().len() as u32)
    });
    result.unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn bubble_selection_rect(
    index: u32,
    x: *mut f32,
    y: *mut f32,
    w: *mut f32,
    h: *mut f32,
) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let shell = engine
            .shared
            .shell
            .lock()
            .map_err(|_| "tree lock poisoned".to_string())?;
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
    finish(result)
}

#[no_mangle]
pub extern "C" fn bubble_scroll(x: f32, y: f32, dx: f32, dy: f32) {
    let _ = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let mut shell = engine
            .shared
            .shell
            .lock()
            .map_err(|_| "tree lock poisoned".to_string())?;
        shell.scroll_wheel(x, y, dx, dy);
        Ok(())
    });
}

#[no_mangle]
pub extern "C" fn bubble_set_gesture_trace(enabled: i32) {
    let _ = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let mut shell = engine
            .shared
            .shell
            .lock()
            .map_err(|_| "tree lock poisoned".to_string())?;
        shell.set_gesture_trace(enabled != 0);
        Ok(())
    });
}

/// Newline-separated trace lines. Empty when tracing is off.
#[no_mangle]
pub extern "C" fn bubble_take_gesture_trace(buf: *mut c_char, cap: u32) -> i32 {
    copy_text(buf, cap, |engine| {
        drain_pending(&engine.shared)?;
        let mut shell = engine
            .shared
            .shell
            .lock()
            .map_err(|_| "tree lock poisoned".to_string())?;
        Ok(shell.take_gesture_trace().join("\n"))
    })
}

#[no_mangle]
pub extern "C" fn bubble_set_visible(visible: i32) {
    if let Ok(engine) = engine() {
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
pub extern "C" fn bubble_register_font(family: *const c_char, data: *const u8, len: u32) {
    let result = catch(|| {
        let engine = engine()?;
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
    let _ = finish(result);
}

#[no_mangle]
pub extern "C" fn bubble_font_epoch() -> u32 {
    engine()
        .map(|engine| engine.shared.epoch.load(Ordering::Relaxed))
        .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn bubble_font_count() -> u32 {
    engine()
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
pub extern "C" fn bubble_node_text(handle: u32, buf: *mut c_char, cap: u32) -> i32 {
    copy_text(buf, cap, |engine| {
        drain_pending(&engine.shared)?;
        let shell = engine
            .shared
            .shell
            .lock()
            .map_err(|_| "tree lock poisoned".to_string())?;
        shell.text(handle).map(|text| text.to_owned())
    })
}

#[no_mangle]
pub extern "C" fn bubble_node_box(
    handle: u32,
    x: *mut f32,
    y: *mut f32,
    w: *mut f32,
    h: *mut f32,
) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        drain_pending(&engine.shared)?;
        let shell = engine
            .shared
            .shell
            .lock()
            .map_err(|_| "tree lock poisoned".to_string())?;
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
    finish(result)
}

#[no_mangle]
pub extern "C" fn bubble_scroll_x_of(handle: u32) -> f32 {
    scroll_of(handle).map_or(f32::NAN, |(x, _)| x)
}

#[no_mangle]
pub extern "C" fn bubble_scroll_y_of(handle: u32) -> f32 {
    scroll_of(handle).map_or(f32::NAN, |(_, y)| y)
}

fn scroll_of(handle: u32) -> Option<(f32, f32)> {
    let engine = engine().ok()?;
    drain_pending(&engine.shared).ok()?;
    let shell = engine.shared.shell.lock().ok()?;
    shell.scroll_of(handle).ok()
}

#[no_mangle]
pub extern "C" fn bubble_eval(source: *const c_char, buf: *mut c_char, cap: u32) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
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
    finish(result)
}

#[no_mangle]
pub extern "C" fn bubble_post_eval(source: *const c_char) {
    let result = catch(|| {
        let engine = engine()?;
        let source = cstr(source)?;
        engine
            .tx
            .send(Cmd::Eval { source, ack: None })
            .map_err(|_| "script thread is gone".to_string())?;
        Ok(())
    });
    let _ = finish(result);
}

#[no_mangle]
pub extern "C" fn bubble_generation() -> u32 {
    engine()
        .map(|engine| engine.shared.generation.load(Ordering::Relaxed))
        .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn bubble_faults() -> u32 {
    engine()
        .map(|engine| engine.shared.faults.load(Ordering::Relaxed))
        .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn bubble_status(buf: *mut c_char, cap: u32) -> i32 {
    copy_text(buf, cap, |engine| {
        engine
            .shared
            .status
            .lock()
            .map(|status| status.clone())
            .map_err(|_| "status lock poisoned".into())
    })
}

#[no_mangle]
pub extern "C" fn bubble_last_error() -> *const c_char {
    ERROR_C
        .lock()
        .ok()
        .and_then(|slot| slot.as_ref().map(|text| text.as_ptr()))
        .unwrap_or(std::ptr::null())
}

#[no_mangle]
pub extern "C" fn bubble_poll_request(
    id: *mut u32,
    method: *mut c_char,
    method_cap: u32,
    params: *mut c_char,
    params_cap: u32,
) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
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
            store_error(&err);
            -1
        }
    }
}

#[no_mangle]
pub extern "C" fn bubble_finish_request(id: u32, ok: i32, result: *const c_char) {
    let settled = catch(|| {
        let engine = engine()?;
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
    let _ = finish(settled);
}

#[repr(C)]
pub struct BubbleHost {
    pub measure: Option<MeasureFn>,
    pub ctx: *mut c_void,
}

fn finish(result: Result<(), String>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(err) => {
            store_error(&err);
            if let Ok(engine) = engine() {
                engine.shared.set_error(&err);
            }
            1
        }
    }
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
    buf: *mut c_char,
    cap: u32,
    body: impl FnOnce(EngineRef) -> Result<String, String>,
) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        let text = body(engine)?;
        write_buf(buf, cap, &text);
        Ok(())
    });
    finish(result)
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

    use jni::objects::{JByteArray, JClass, JObject, JString};
    use jni::sys::{jfloat, jint, jlong};
    use jni::JNIEnv;

    use super::BubbleHost;

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_create<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
    ) -> jint {
        super::bubble_create(ptr::null())
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_destroy<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
    ) {
        super::bubble_destroy();
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_load<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        manifest: JString<'a>,
        entry: JByteArray<'a>,
    ) -> jint {
        let Ok(manifest) = env.get_string(&manifest) else {
            return 1;
        };
        let manifest = super::CString_from(&manifest.to_string_lossy());
        let Ok(bytes) = env.convert_byte_array(entry) else {
            return 1;
        };
        super::bubble_load(manifest.as_ptr(), bytes.as_ptr().cast(), bytes.len() as u32)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_frame<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        width: jfloat,
        height: jfloat,
    ) -> JByteArray<'a> {
        let mut ptr = ptr::null();
        let mut len = 0u32;
        if super::bubble_frame(width, height, &mut ptr, &mut len) != 0 || ptr.is_null() {
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
        action: jint,
        id: jint,
        x: jfloat,
        y: jfloat,
        time_ms: jni::sys::jdouble,
        pointer_kind: jint,
        buttons: jint,
    ) -> jint {
        super::bubble_pointer(
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
        time_ms: jni::sys::jdouble,
    ) -> jint {
        super::bubble_tick(time_ms)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_scroll<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        x: jfloat,
        y: jfloat,
        dx: jfloat,
        dy: jfloat,
    ) {
        super::bubble_scroll(x, y, dx, dy);
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_edit<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        action: jint,
        text: JByteArray<'a>,
        a: jint,
        b: jint,
    ) -> jint {
        let bytes = env.convert_byte_array(text).unwrap_or_default();
        super::bubble_edit(
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
        session: jlong,
    ) -> jint {
        super::bubble_edit_batch_begin(session as u64)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_editBatchEnd<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        session: jlong,
    ) -> jint {
        super::bubble_edit_batch_end(session as u64)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_editBatchAbort<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        session: jlong,
    ) -> jint {
        super::bubble_edit_batch_abort(session as u64)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_editSnapshot<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
    ) -> jni::objects::JLongArray<'a> {
        let (mut session, mut revision) = (0u64, 0u64);
        let (mut anchor, mut active) = (0u32, 0u32);
        let (mut comp_start, mut comp_end, mut composing) = (0u32, 0u32, 0u32);
        let code = super::bubble_edit_snapshot(
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
        session: jlong,
        text: JByteArray<'a>,
        new_cursor_position: jint,
    ) -> jint {
        let bytes = env.convert_byte_array(text).unwrap_or_default();
        super::bubble_edit_commit(
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
        session: jlong,
        text: JByteArray<'a>,
        new_cursor_position: jint,
    ) -> jint {
        let bytes = env.convert_byte_array(text).unwrap_or_default();
        super::bubble_edit_compose(
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
        session: jlong,
    ) -> jint {
        super::bubble_edit_undo(session as u64)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_editRedo<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        session: jlong,
    ) -> jint {
        super::bubble_edit_redo(session as u64)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_editDeleteSurrounding<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        session: jlong,
        before: jint,
        after: jint,
    ) -> jint {
        super::bubble_edit_delete_surrounding(session as u64, before as u32, after as u32)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_editDeleteSurroundingCodePoints<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        session: jlong,
        before: jint,
        after: jint,
    ) -> jint {
        super::bubble_edit_delete_surrounding_code_points(
            session as u64,
            before as u32,
            after as u32,
        )
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_editSetComposition<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        session: jlong,
        start: jint,
        end: jint,
    ) -> jint {
        super::bubble_edit_set_composition(session as u64, start as u32, end as u32)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_editFinishComposition<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        session: jlong,
    ) -> jint {
        super::bubble_edit_finish_composition(session as u64)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_editSelect<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        session: jlong,
        anchor: jint,
        active: jint,
    ) -> jint {
        super::bubble_edit_select(session as u64, anchor as u32, active as u32)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_fieldAt<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        x: jfloat,
        y: jfloat,
    ) -> jint {
        super::bubble_field_at(x, y) as jint
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_focusAt<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        x: jfloat,
        y: jfloat,
    ) -> jint {
        super::bubble_focus_at(x, y) as jint
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_fieldFlags<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        handle: jint,
    ) -> jint {
        super::bubble_field_flags(handle as u32) as jint
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_scrollXOf<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        handle: jint,
    ) -> jfloat {
        super::bubble_scroll_x_of(handle as u32)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_scrollYOf<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        handle: jint,
    ) -> jfloat {
        super::bubble_scroll_y_of(handle as u32)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_setVisible<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        visible: jint,
    ) {
        super::bubble_set_visible(visible);
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_registerFont<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
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
        super::bubble_register_font(family.as_ptr(), bytes.as_ptr().cast(), bytes.len() as u32);
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_nodeText<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        handle: jint,
    ) -> JString<'a> {
        let mut buf = [0 as std::os::raw::c_char; 65536];
        let code = super::bubble_node_text(handle as u32, buf.as_mut_ptr(), buf.len() as u32);
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
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_focused<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
    ) -> jint {
        super::bubble_focused() as jint
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_focusedText<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
    ) -> JString<'a> {
        let mut buf = vec![0 as std::os::raw::c_char; 65536];
        let handle = super::bubble_focused();
        let text = if handle == 0 {
            String::new()
        } else {
            let _ = super::bubble_node_text(handle, buf.as_mut_ptr(), buf.len() as u32);
            unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }
                .to_string_lossy()
                .into_owned()
        };
        env.new_string(text).unwrap_or_else(|_| env.new_string("").unwrap())
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_selection<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
    ) -> jni::objects::JIntArray<'a> {
        let (mut start, mut end, mut mark_start, mut mark_end) = (0u32, 0, 0, 0);
        let _ = super::bubble_selection(&mut start, &mut end, &mut mark_start, &mut mark_end);
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
    ) -> jni::objects::JFloatArray<'a> {
        let (mut x, mut y, mut w, mut h) = (0.0f32, 0.0, 0.0, 0.0);
        let array = if super::bubble_caret(&mut x, &mut y, &mut w, &mut h) == 0 {
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
        handle: jint,
    ) -> jni::objects::JFloatArray<'a> {
        let (mut x, mut y, mut w, mut h) = (0.0f32, 0.0, 0.0, 0.0);
        let _ = super::bubble_node_box(handle as u32, &mut x, &mut y, &mut w, &mut h);
        let array = env.new_float_array(4).expect("float array");
        let _ = env.set_float_array_region(&array, 0, &[x, y, w, h]);
        array
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_postEval<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        source: JString<'a>,
    ) {
        let Ok(source) = env.get_string(&source) else {
            return;
        };
        let source = super::CString_from(&source.to_string_lossy());
        super::bubble_post_eval(source.as_ptr());
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_faults<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
    ) -> jint {
        super::bubble_faults() as jint
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_generation<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
    ) -> jint {
        super::bubble_generation() as jint
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_fontEpoch<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
    ) -> jint {
        super::bubble_font_epoch() as jint
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
