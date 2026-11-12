//! Single-threaded wasm ABI. The host copies bytes in and paint/op bytes out.
//! `host_measure` writes two f32s at `out_ptr` and must not grow wasm memory.
//!
//! The script side (`Turn`) only queues ops. `commit` pushes a batch into the
//! pending queue; layout/paint/hit-test drain that queue into the `Shell`
//! first. This keeps the tree-owning code on one side of the JS boundary.

// These are raw wasm exports: JS callers cannot express Rust's `unsafe`
// contract, so pointer validity is documented per function instead.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

use bubble_core::{
    decode_layout, encode_edit_events, encode_events, encode_paint, encode_set_text,
    validate_bundle_hashes, A11yAction, ComputationKind, EditCommand, EditEvent, EditMenuAction,
    GestureEvent, GestureKind, Key, LayoutRequest, NodeDesc, Op, PointerAction, PointerInput,
    Sched, Shell, Turn, Value, DEFERRED_PROPS, SPAWN_PROPS,
};

thread_local! {
    static TURN: RefCell<Option<Turn>> = const { RefCell::new(None) };
    static ACTIVE: Cell<*mut Turn> = const { Cell::new(std::ptr::null_mut()) };
    static SHELL: RefCell<Option<Shell>> = const { RefCell::new(None) };
    static PENDING: RefCell<VecDeque<Vec<u8>>> = const { RefCell::new(VecDeque::new()) };
    static SCRATCH: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static ERROR: RefCell<String> = const { RefCell::new(String::new()) };
    static GESTURES: RefCell<Vec<GestureEvent>> = const { RefCell::new(Vec::new()) };
    static EDITS: RefCell<Vec<EditEvent>> = const { RefCell::new(Vec::new()) };
}

#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn host_measure(
        text_ptr: u32,
        text_len: u32,
        font_size: f32,
        line_height: f32,
        weight: u32,
        mode: u32,
        avail: f32,
        out_ptr: u32,
    );
    fn host_layout(
        text_ptr: u32,
        text_len: u32,
        width: f32,
        font_size: f32,
        line_height: f32,
        weight: u32,
        align: u32,
        density: f32,
        revision_lo: u32,
        revision_hi: u32,
        out_ptr: u32,
        out_cap: u32,
    ) -> i32;
    fn host_run(id: u32) -> i32;
    fn host_cleanup(id: u32);
    fn host_body();
}

struct WasmHost;
impl Sched for WasmHost {
    fn body(&mut self) {
        unsafe { host_body() }
    }
    fn run(&mut self, id: u32) -> i32 {
        unsafe { host_run(id) }
    }
    fn cleanup(&mut self, id: u32) {
        unsafe { host_cleanup(id) }
    }
}

fn set_error(message: String) {
    ERROR.with(|slot| *slot.borrow_mut() = message);
}

fn wasm_layout(req: &LayoutRequest<'_>) -> Result<bubble_core::LayoutSnapshot, String> {
    let mut cap = (req.text.len() as u32)
        .saturating_mul(64)
        .saturating_add(4096);
    for _ in 0..2 {
        let mut buf = vec![0u8; cap as usize];
        let n = unsafe {
            host_layout(
                req.text.as_ptr() as u32,
                req.text.len() as u32,
                req.key.width,
                req.key.font_size,
                req.key.line_height,
                u32::from(req.key.font_weight),
                u32::from(req.key.align),
                req.key.density,
                req.key.revision as u32,
                (req.key.revision >> 32) as u32,
                buf.as_mut_ptr() as u32,
                cap,
            )
        };
        if n < 0 {
            return Err("host layout failed".into());
        }
        if n as u32 > cap {
            cap = n as u32;
            continue;
        }
        return decode_layout(&buf[..n as usize]);
    }
    Err("host layout did not fit".into())
}

fn store_scratch(bytes: Vec<u8>) -> u32 {
    let len = bytes.len() as u32;
    SCRATCH.with(|slot| *slot.borrow_mut() = bytes);
    len
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

fn note_graph(turn: &mut Turn) -> Result<(), String> {
    match turn.take_graph_error() {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

fn with_shell<T>(body: impl FnOnce(&mut Shell) -> Result<T, String>) -> Result<T, String> {
    SHELL.with(|slot| {
        let mut slot = slot.borrow_mut();
        let shell = slot
            .as_mut()
            .ok_or_else(|| "shell is not mounted".to_string())?;
        body(shell)
    })
}

/// Apply every committed batch in order. Call before any read of the tree.
fn drain_pending() -> Result<(), String> {
    let batches: Vec<Vec<u8>> = PENDING.with(|queue| queue.borrow_mut().drain(..).collect());
    if batches.is_empty() {
        return Ok(());
    }
    with_shell(|shell| {
        for bytes in batches {
            shell.apply_ops(&bytes)?;
        }
        collect_events(shell);
        Ok(())
    })
}

fn collect_events(shell: &mut Shell) {
    let events = shell.take_gesture_events();
    if !events.is_empty() {
        GESTURES.with(|queue| queue.borrow_mut().extend(events));
    }
    let edits = shell.take_edit_events();
    if !edits.is_empty() {
        EDITS.with(|queue| queue.borrow_mut().extend(edits));
    }
}

/// `action` 0 down, 1 move, 2 up, 3 cancel. Returns 0 on success.
#[no_mangle]
pub extern "C" fn pointer(
    action: u32,
    id: u32,
    x: f32,
    y: f32,
    time_ms: f64,
    pointer_kind: u32,
    buttons: u32,
) -> i32 {
    let Some(action) = PointerAction::parse(action) else {
        set_error(format!("bad pointer action {action}"));
        return 1;
    };
    let result = drain_pending().and_then(|()| {
        with_shell(|shell| {
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
            collect_events(shell);
            Ok(())
        })
    });
    match result {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

/// Advance UI-thread gesture timers (long press, double-tap window).
#[no_mangle]
pub extern "C" fn tick(time_ms: f64) -> i32 {
    let result = drain_pending().and_then(|()| {
        with_shell(|shell| {
            shell.advance_gestures(time_ms);
            collect_events(shell);
            Ok(())
        })
    });
    match result {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn set_gesture_trace(enabled: u32) {
    if let Err(err) = drain_pending().and_then(|()| {
        with_shell(|shell| {
            shell.set_gesture_trace(enabled != 0);
            Ok(())
        })
    }) {
        set_error(err);
    }
}

/// Newline-separated negotiation trace lines. Empty when tracing is off.
#[no_mangle]
pub extern "C" fn take_gesture_trace() -> u32 {
    drain_pending().ok();
    let lines = with_shell(|shell| Ok(shell.take_gesture_trace())).unwrap_or_default();
    store_scratch(lines.join("\n").into_bytes())
}

#[no_mangle]
pub extern "C" fn gesture_pending() -> i32 {
    drain_pending().ok();
    with_shell(|shell| Ok(shell.gestures_pending()))
        .map(i32::from)
        .unwrap_or(0)
}

/// Encoded `BSGE` batch of events since the last drain; 0 bytes when empty.
#[no_mangle]
pub extern "C" fn take_gesture_events() -> u32 {
    let events = GESTURES.with(|queue| std::mem::take(&mut *queue.borrow_mut()));
    if events.is_empty() {
        store_scratch(Vec::new())
    } else {
        store_scratch(encode_events(&events))
    }
}

/// UI-thread keyboard intent. Actions match the native ABI: 0 Tab,
/// 1 Shift+Tab, 2 Enter, 3 Space, 4 Escape, 5 Left, 6 Right, 7 Home, 8 End.
/// Returns 1 handled, 0 not, -1 error.
#[no_mangle]
pub extern "C" fn key(action: u32, time_ms: f64) -> i32 {
    let Some(key) = Key::parse(action) else {
        set_error(format!("bad key action {action}"));
        return -1;
    };
    let result = drain_pending().and_then(|()| {
        with_shell(|shell| {
            let handled = shell.key(key, time_ms);
            collect_events(shell);
            Ok(handled)
        })
    });
    match result {
        Ok(handled) => i32::from(handled),
        Err(err) => {
            set_error(err);
            -1
        }
    }
}

/// Accessibility semantics snapshot as UTF-8 JSON in scratch.
#[no_mangle]
pub extern "C" fn semantics() -> u32 {
    drain_pending().ok();
    let json = with_shell(|shell| Ok(shell.semantics_json())).unwrap_or_default();
    store_scratch(json.into_bytes())
}

/// Screen-reader action: 0 focus, 1 activate, 2 set value, 3/4 forward/
/// backward, 5/6 left/right, 7/8 up/down.
#[no_mangle]
pub extern "C" fn a11y_action(handle: u32, action: u32, ptr: *const u8, len: u32) -> i32 {
    let Some(action) = A11yAction::parse(action) else {
        set_error(format!("bad a11y action {action}"));
        return 1;
    };
    let action = match action {
        A11yAction::SetValue(_) => {
            let text = if ptr.is_null() || len == 0 {
                String::new()
            } else {
                match std::str::from_utf8(unsafe { std::slice::from_raw_parts(ptr, len as usize) })
                {
                    Ok(text) => text.to_owned(),
                    Err(_) => {
                        set_error("a11y value is not utf-8".into());
                        return 1;
                    }
                }
            };
            A11yAction::SetValue(text)
        }
        other => other,
    };
    let result = drain_pending().and_then(|()| {
        with_shell(|shell| {
            shell.a11y_action(handle, action)?;
            collect_events(shell);
            Ok(())
        })
    });
    match result {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

/// Edit menu command: 0 cut, 1 copy, 2 paste, 3 select all. Writes any
/// clipboard text to scratch and returns 0 unchanged, 1 changed, -1 error.
#[no_mangle]
pub extern "C" fn edit_menu(action: u32, ptr: *const u8, len: u32) -> i32 {
    let Some(action) = EditMenuAction::parse(action) else {
        set_error(format!("bad menu action {action}"));
        return -1;
    };
    let clipboard = if ptr.is_null() || len == 0 {
        String::new()
    } else {
        match std::str::from_utf8(unsafe { std::slice::from_raw_parts(ptr, len as usize) }) {
            Ok(text) => text.to_owned(),
            Err(_) => {
                set_error("clipboard is not utf-8".into());
                return -1;
            }
        }
    };
    let result = drain_pending().and_then(|()| {
        with_shell(|shell| {
            let menu = shell.edit_menu(action, &clipboard)?;
            collect_events(shell);
            store_scratch(menu.clipboard.unwrap_or_default().into_bytes());
            Ok(menu.changed)
        })
    });
    match result {
        Ok(changed) => i32::from(changed),
        Err(err) => {
            set_error(err);
            -1
        }
    }
}

/// Start and end selection handle boxes as 8 little-endian f32 values in
/// scratch. Returns the byte length (0 when the selection is collapsed).
#[no_mangle]
pub extern "C" fn selection_handles() -> u32 {
    let result = drain_pending().and_then(|()| {
        with_shell(|shell| {
            let Some([a, b]) = shell.selection_handles() else {
                return Ok(Vec::new());
            };
            let mut bytes = Vec::with_capacity(32);
            for value in [a.0, a.1, a.2, a.3, b.0, b.1, b.2, b.3] {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            Ok(bytes)
        })
    });
    match result {
        Ok(bytes) => store_scratch(bytes),
        Err(err) => {
            set_error(err);
            0
        }
    }
}

/// Same action codes as `bubble_edit`.
#[no_mangle]
pub extern "C" fn edit(action: u32, ptr: *const u8, len: u32, a: u32, b: u32) -> i32 {
    let text = if ptr.is_null() || len == 0 {
        String::new()
    } else {
        match std::str::from_utf8(unsafe { std::slice::from_raw_parts(ptr, len as usize) }) {
            Ok(text) => text.to_owned(),
            Err(_) => {
                set_error("edit text is not utf-8".into());
                return 1;
            }
        }
    };
    let result = drain_pending().and_then(|()| {
        with_shell(|shell| {
            match action {
                0 => shell.focus(a)?,
                1 => shell.edit(EditCommand::Insert(text))?,
                2 => shell.edit(EditCommand::Backspace)?,
                3 => shell.edit(EditCommand::DeleteForward)?,
                4 => shell.edit(EditCommand::SetSelection { start: a, end: b })?,
                5 => shell.edit(EditCommand::CompositionStart)?,
                6 => shell.edit(EditCommand::CompositionUpdate(text))?,
                7 => shell.edit(EditCommand::CompositionEnd(text))?,
                8 => shell.edit(EditCommand::Undo)?,
                9 => shell.edit(EditCommand::Redo)?,
                other => return Err(format!("bad edit action {other}")),
            }
            collect_events(shell);
            Ok(())
        })
    });
    match result {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn edit_batch_begin(session: u64) -> i32 {
    let result = drain_pending().and_then(|()| {
        with_shell(|shell| {
            wasm_session(shell, session)?;
            shell.begin_edit_batch()?;
            collect_events(shell);
            Ok(())
        })
    });
    match result {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn edit_batch_end(session: u64) -> i32 {
    let result = drain_pending().and_then(|()| {
        with_shell(|shell| {
            wasm_session(shell, session)?;
            shell.end_edit_batch()?;
            collect_events(shell);
            Ok(())
        })
    });
    match result {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn edit_batch_abort(session: u64) -> i32 {
    let result = drain_pending().and_then(|()| {
        with_shell(|shell| {
            if shell
                .focused_state()
                .is_ok_and(|state| state.session.get() == session)
            {
                shell.abort_edit_batch();
            }
            collect_events(shell);
            Ok(())
        })
    });
    match result {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

fn wasm_session(shell: &Shell, session: u64) -> Result<(), String> {
    let state = shell.focused_state()?;
    if state.session.get() != session {
        return Err("stale editing session".into());
    }
    Ok(())
}

fn edit_text(ptr: *const u8, len: u32) -> Result<String, String> {
    if ptr.is_null() || len == 0 {
        return Ok(String::new());
    }
    std::str::from_utf8(unsafe { std::slice::from_raw_parts(ptr, len as usize) })
        .map(str::to_owned)
        .map_err(|_| "edit text is not utf-8".into())
}

/// 36-byte focused snapshot: session u64, revision u64, anchor, active,
/// composition start, composition end (u32::MAX when absent), composing.
#[no_mangle]
pub extern "C" fn edit_snapshot() -> u32 {
    let result = drain_pending().and_then(|()| {
        with_shell(|shell| {
            let state = shell.focused_state()?;
            let mut out = Vec::with_capacity(36);
            out.extend_from_slice(&state.session.get().to_le_bytes());
            out.extend_from_slice(&state.revision.to_le_bytes());
            out.extend_from_slice(&state.selection.anchor.offset.to_le_bytes());
            out.extend_from_slice(&state.selection.active.offset.to_le_bytes());
            let (start, end) = state.composition.unwrap_or((0, u32::MAX));
            out.extend_from_slice(&start.to_le_bytes());
            out.extend_from_slice(&end.to_le_bytes());
            out.extend_from_slice(&u32::from(state.composing).to_le_bytes());
            Ok(out)
        })
    });
    match result {
        Ok(bytes) => store_scratch(bytes),
        Err(err) => {
            set_error(err);
            store_scratch(Vec::new())
        }
    }
}

#[no_mangle]
pub extern "C" fn edit_commit(
    session: u64,
    ptr: *const u8,
    len: u32,
    new_cursor_position: i32,
) -> i32 {
    let result = drain_pending().and_then(|()| {
        let text = edit_text(ptr, len)?;
        with_shell(|shell| {
            wasm_session(shell, session)?;
            shell.commit_text(&text, new_cursor_position)?;
            collect_events(shell);
            Ok(())
        })
    });
    match result {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn edit_compose(
    session: u64,
    ptr: *const u8,
    len: u32,
    new_cursor_position: i32,
) -> i32 {
    let result = drain_pending().and_then(|()| {
        let text = edit_text(ptr, len)?;
        with_shell(|shell| {
            wasm_session(shell, session)?;
            shell.compose_text(&text, new_cursor_position)?;
            collect_events(shell);
            Ok(())
        })
    });
    match result {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn edit_replace(
    session: u64,
    start: u32,
    end: u32,
    ptr: *const u8,
    len: u32,
    new_cursor_position: i32,
    composing: u32,
) -> i32 {
    let result = drain_pending().and_then(|()| {
        let text = edit_text(ptr, len)?;
        with_shell(|shell| {
            wasm_session(shell, session)?;
            shell.replace_with_cursor(start, end, &text, new_cursor_position, composing != 0)?;
            collect_events(shell);
            Ok(())
        })
    });
    match result {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn edit_delete_surrounding(session: u64, before: u32, after: u32) -> i32 {
    let result = drain_pending().and_then(|()| {
        with_shell(|shell| {
            wasm_session(shell, session)?;
            shell.delete_surrounding(before, after)?;
            collect_events(shell);
            Ok(())
        })
    });
    match result {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn edit_delete_surrounding_code_points(
    session: u64,
    before: u32,
    after: u32,
) -> i32 {
    let result = drain_pending().and_then(|()| {
        with_shell(|shell| {
            wasm_session(shell, session)?;
            shell.delete_surrounding_code_points(before, after)?;
            collect_events(shell);
            Ok(())
        })
    });
    match result {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn edit_set_composition(session: u64, start: u32, end: u32) -> i32 {
    let result = drain_pending().and_then(|()| {
        with_shell(|shell| {
            wasm_session(shell, session)?;
            shell.set_composition_region(start, end)?;
            collect_events(shell);
            Ok(())
        })
    });
    match result {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn edit_finish_composition(session: u64) -> i32 {
    let result = drain_pending().and_then(|()| {
        with_shell(|shell| {
            wasm_session(shell, session)?;
            shell.finish_composition()?;
            collect_events(shell);
            Ok(())
        })
    });
    match result {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn edit_select(session: u64, anchor: u32, active: u32) -> i32 {
    let result = drain_pending().and_then(|()| {
        with_shell(|shell| {
            wasm_session(shell, session)?;
            shell.edit(EditCommand::SetSelection {
                start: anchor,
                end: active,
            })?;
            collect_events(shell);
            Ok(())
        })
    });
    match result {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn edit_undo(session: u64) -> i32 {
    let result = drain_pending().and_then(|()| {
        with_shell(|shell| {
            wasm_session(shell, session)?;
            shell.edit(EditCommand::Undo)?;
            collect_events(shell);
            Ok(())
        })
    });
    match result {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn edit_redo(session: u64) -> i32 {
    let result = drain_pending().and_then(|()| {
        with_shell(|shell| {
            wasm_session(shell, session)?;
            shell.edit(EditCommand::Redo)?;
            collect_events(shell);
            Ok(())
        })
    });
    match result {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

/// Deepest text field under the point, or `0`.
#[no_mangle]
pub extern "C" fn field_at(x: f32, y: f32) -> u32 {
    drain_pending().ok();
    with_shell(|shell| Ok(shell.field_at(x, y))).unwrap_or(0)
}

/// Focus the field under the point and place the caret. Returns the handle, or 0.
#[no_mangle]
pub extern "C" fn focus_at(x: f32, y: f32) -> u32 {
    let result = drain_pending().and_then(|()| {
        with_shell(|shell| {
            let handle = shell.focus_point_with(x, y, &mut wasm_layout)?;
            collect_events(shell);
            Ok(handle)
        })
    });
    match result {
        Ok(handle) => handle,
        Err(err) => {
            set_error(err);
            0
        }
    }
}

/// Nearest UTF-16 offset in the focused field. `u32::MAX` when nothing is focused.
#[no_mangle]
pub extern "C" fn unit_at(x: f32, y: f32) -> u32 {
    let result = drain_pending()
        .and_then(|()| with_shell(|shell| shell.unit_at_with(x, y, &mut wasm_layout)));
    match result {
        Ok(Some(unit)) => unit,
        _ => u32::MAX,
    }
}

/// Writes x, y, width, height of the focused caret into scratch. Returns 16, or 0.
#[no_mangle]
pub extern "C" fn caret_box() -> u32 {
    drain_pending().ok();
    let Some((x, y, w, h)) = with_shell(|shell| {
        shell.refresh_caret(&mut wasm_layout)?;
        Ok(shell.caret_box())
    })
    .ok()
    .flatten() else {
        return store_scratch(Vec::new());
    };
    let mut bytes = Vec::with_capacity(16);
    for value in [x, y, w, h] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    store_scratch(bytes)
}

#[no_mangle]
pub extern "C" fn field_flags(handle: u32) -> u32 {
    drain_pending().ok();
    with_shell(|shell| shell.field_flags(handle)).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn focused() -> u32 {
    drain_pending().ok();
    with_shell(|shell| Ok(shell.focused())).unwrap_or(0)
}

/// Four little-endian UTF-16 offsets: selection start, end, mark start, mark end.
/// Mark end is `u32::MAX` when no composition is open. Returns 16, or 0.
#[no_mangle]
pub extern "C" fn selection() -> u32 {
    drain_pending().ok();
    let Some((start, end, mark_start, mark_end)) = with_shell(|shell| {
        let handle = shell.focused();
        if handle == 0 {
            return Ok(None);
        }
        let (_, state) = shell.field(handle)?;
        let (mark_start, mark_end) = state.composition.unwrap_or((0, u32::MAX));
        Ok(Some((
            state.sel_start(),
            state.sel_end(),
            mark_start,
            mark_end,
        )))
    })
    .ok()
    .flatten() else {
        return store_scratch(Vec::new());
    };
    let mut bytes = Vec::with_capacity(16);
    for value in [start, end, mark_start, mark_end] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    store_scratch(bytes)
}

/// UTF-8 value of the focused field. Returns 0 when nothing is focused.
#[no_mangle]
pub extern "C" fn focused_text() -> u32 {
    drain_pending().ok();
    let text = with_shell(|shell| {
        let handle = shell.focused();
        if handle == 0 {
            return Ok(String::new());
        }
        shell.field(handle).map(|(value, _)| value)
    });
    match text {
        Ok(value)
            if !value.is_empty() || with_shell(|shell| Ok(shell.focused())).unwrap_or(0) != 0 =>
        {
            store_scratch(value.into_bytes())
        }
        _ => store_scratch(Vec::new()),
    }
}

/// Encoded `BSED` batch since the last drain; 0 bytes when empty.
#[no_mangle]
pub extern "C" fn take_edit_events() -> u32 {
    let events = EDITS.with(|queue| std::mem::take(&mut *queue.borrow_mut()));
    if events.is_empty() {
        store_scratch(Vec::new())
    } else {
        store_scratch(encode_edit_events(&events))
    }
}

#[no_mangle]
pub extern "C" fn focus(handle: u32) -> i32 {
    match with_turn(|turn| {
        turn.push(Op::Focus { handle });
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn set_editing_value(
    handle: u32,
    epoch: f64,
    ptr: *const u8,
    len: u32,
    sel_start: u32,
    sel_end: u32,
    cancel: u32,
) -> i32 {
    let value = if ptr.is_null() || len == 0 {
        String::new()
    } else {
        match std::str::from_utf8(unsafe { std::slice::from_raw_parts(ptr, len as usize) }) {
            Ok(text) => text.to_owned(),
            Err(_) => {
                set_error("editing value is not utf-8".into());
                return 1;
            }
        }
    };
    match with_turn(|turn| {
        turn.push(Op::SetEditingValue {
            handle,
            epoch: epoch as u64,
            value,
            sel_start,
            sel_end,
            cancel: cancel != 0,
        });
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn alloc(size: u32) -> *mut u8 {
    let mut buffer = vec![0u8; size as usize];
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    ptr
}

#[no_mangle]
pub extern "C" fn dealloc(ptr: *mut u8, size: u32) {
    if ptr.is_null() || size == 0 {
        return;
    }
    unsafe { drop(Vec::from_raw_parts(ptr, size as usize, size as usize)) }
}

#[no_mangle]
pub extern "C" fn error_ptr() -> *const u8 {
    ERROR.with(|slot| slot.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn error_len() -> u32 {
    ERROR.with(|slot| slot.borrow().len() as u32)
}

#[no_mangle]
pub extern "C" fn scratch_ptr() -> *const u8 {
    SCRATCH.with(|slot| slot.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn scratch_len() -> u32 {
    SCRATCH.with(|slot| slot.borrow().len() as u32)
}

/// Newline-separated creation-time property names. The JS runtime validates
/// node props against this canonical list.
#[no_mangle]
pub extern "C" fn spawn_props() -> u32 {
    store_scratch(SPAWN_PROPS.join("\n").into_bytes())
}

/// Newline-separated properties applied as `SetProp` right after `Spawn`.
#[no_mangle]
pub extern "C" fn deferred_props() -> u32 {
    store_scratch(DEFERRED_PROPS.join("\n").into_bytes())
}

#[no_mangle]
pub extern "C" fn boot() -> i32 {
    match Shell::blank() {
        Ok(shell) => {
            TURN.with(|slot| *slot.borrow_mut() = Some(Turn::new()));
            SHELL.with(|slot| *slot.borrow_mut() = Some(shell));
            PENDING.with(|queue| queue.borrow_mut().clear());
            GESTURES.with(|queue| queue.borrow_mut().clear());
            0
        }
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn set_viewport(width: f32, height: f32) {
    if let Err(err) = with_shell(|shell| {
        shell.set_viewport(width, height);
        Ok(())
    }) {
        set_error(err);
    }
}

#[no_mangle]
pub extern "C" fn next_handle() -> u32 {
    with_turn(|turn| Ok(turn.next_handle())).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn signal_alloc() -> u32 {
    with_turn(|turn| Ok(turn.alloc_signal())).unwrap_or(0)
}

/// Records a read. Returns 0, or 1 when `id` is not a signal.
#[no_mangle]
pub extern "C" fn signal_read(id: u32) -> i32 {
    match with_turn(|turn| {
        turn.read(id, &mut WasmHost)?;
        note_graph(turn)
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn signal_notify(id: u32) -> i32 {
    match with_turn(|turn| {
        turn.notify(id, &mut WasmHost)?;
        note_graph(turn)
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

/// `kind`: 1 memo, 2 render effect, 3 user effect.
#[no_mangle]
pub extern "C" fn alloc_node(kind: u32) -> u32 {
    let kind = match kind {
        1 => ComputationKind::Memo,
        2 => ComputationKind::Effect,
        3 => ComputationKind::UserEffect,
        _ => {
            set_error(format!("bad computation kind {kind}"));
            return 0;
        }
    };
    match with_turn(|turn| turn.alloc_computation(kind)) {
        Ok(id) => id,
        Err(err) => {
            set_error(err);
            0
        }
    }
}

#[no_mangle]
pub extern "C" fn mount_node(id: u32) -> i32 {
    match with_turn(|turn| {
        turn.mount_computation(id, &mut WasmHost)?;
        note_graph(turn)
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn dispose_node(id: u32) -> i32 {
    match with_turn(|turn| {
        turn.dispose_computation(id, &mut WasmHost);
        note_graph(turn)
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

/// Stores cleanup `id` on the running owner. Returns 0, or 1 when there is
/// no owner and the host should run it now.
#[no_mangle]
pub extern "C" fn on_cleanup(id: u32) -> i32 {
    match with_turn(|turn| Ok(turn.on_cleanup(id))) {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn run_init() -> i32 {
    scheduled(true)
}

#[no_mangle]
pub extern "C" fn run_updates() -> i32 {
    scheduled(false)
}

fn scheduled(init: bool) -> i32 {
    match with_turn(|turn| {
        turn.run_scheduled(init, &mut WasmHost);
        note_graph(turn)
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

/// `ptr/len` is a `BNOD` version 1 descriptor. The handle is allocated by Rust.
fn read_spawn(parent: u32, ptr: *const u8, len: u32) -> Result<NodeDesc, String> {
    if ptr.is_null() || len == 0 {
        return Err("spawn descriptor is empty".into());
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    let mut desc = NodeDesc::decode(bytes)?;
    desc.parent = parent;
    Ok(desc)
}

#[no_mangle]
pub extern "C" fn spawn(parent: u32, ptr: *const u8, len: u32) -> u32 {
    let desc = match read_spawn(parent, ptr, len) {
        Ok(desc) => desc,
        Err(err) => {
            set_error(err);
            return 0;
        }
    };
    match with_turn(|turn| turn.spawn(desc)) {
        Ok(handle) => handle,
        Err(err) => {
            set_error(err);
            0
        }
    }
}

#[no_mangle]
pub extern "C" fn spawn_at(parent: u32, index: u32, ptr: *const u8, len: u32) -> u32 {
    let desc = match read_spawn(parent, ptr, len) {
        Ok(desc) => desc,
        Err(err) => {
            set_error(err);
            return 0;
        }
    };
    match with_turn(|turn| turn.spawn_at(desc, index)) {
        Ok(handle) => handle,
        Err(err) => {
            set_error(err);
            0
        }
    }
}

#[no_mangle]
pub extern "C" fn dispose(handle: u32) -> i32 {
    match with_turn(|turn| {
        turn.dispose(handle);
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn move_node(handle: u32, parent: u32, index: u32) -> i32 {
    match with_turn(|turn| {
        turn.move_node(handle, parent, index);
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn set_prop(
    handle: u32,
    key_ptr: *const u8,
    key_len: u32,
    tag: u32,
    number: f64,
    text_ptr: *const u8,
    text_len: u32,
) -> i32 {
    let key_bytes = unsafe { std::slice::from_raw_parts(key_ptr, key_len as usize) };
    let Ok(key) = std::str::from_utf8(key_bytes) else {
        set_error("prop key is not utf-8".into());
        return 1;
    };
    let value = match tag {
        0 => Value::Null,
        1 => Value::Bool(number != 0.0),
        2 => Value::Int(number as i64),
        3 => Value::Float(number),
        4 => {
            let bytes = unsafe { std::slice::from_raw_parts(text_ptr, text_len as usize) };
            let Ok(text) = std::str::from_utf8(bytes) else {
                set_error("prop string is not utf-8".into());
                return 1;
            };
            Value::String(text.to_owned())
        }
        other => {
            set_error(format!("unsupported prop tag {other}"));
            return 1;
        }
    };
    match with_turn(|turn| {
        turn.set_prop(handle, key, value.clone());
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn queue_text(handle: u32, ptr: *const u8, len: u32) -> i32 {
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    let Ok(text) = std::str::from_utf8(bytes) else {
        set_error("op text is not utf-8".into());
        return 1;
    };
    match with_turn(|turn| {
        turn.push(Op::SetText {
            handle,
            text: text.to_owned(),
        });
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn bind_event(handle: u32, handler: u32) -> i32 {
    match with_turn(|turn| {
        turn.bind_event(handle, handler);
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn spawn_gesture(target: u32, kind: u32, index: u32) -> u32 {
    let Some(kind) = GestureKind::parse(kind) else {
        set_error(format!("bad gesture kind {kind}"));
        return 0;
    };
    match with_turn(|turn| turn.spawn_gesture(target, kind, index)) {
        Ok(handle) => handle,
        Err(err) => {
            set_error(err);
            0
        }
    }
}

fn read_value(tag: u32, number: f64, text_ptr: *const u8, text_len: u32) -> Result<Value, String> {
    Ok(match tag {
        0 => Value::Null,
        1 => Value::Bool(number != 0.0),
        2 => Value::Int(number as i64),
        3 => Value::Float(number),
        4 => {
            let bytes = unsafe { std::slice::from_raw_parts(text_ptr, text_len as usize) };
            let text = std::str::from_utf8(bytes).map_err(|_| "prop string is not utf-8")?;
            Value::String(text.to_owned())
        }
        other => return Err(format!("unsupported prop tag {other}")),
    })
}

#[no_mangle]
pub extern "C" fn set_gesture_prop(
    handle: u32,
    key_ptr: *const u8,
    key_len: u32,
    tag: u32,
    number: f64,
    text_ptr: *const u8,
    text_len: u32,
) -> i32 {
    let key_bytes = unsafe { std::slice::from_raw_parts(key_ptr, key_len as usize) };
    let Ok(key) = std::str::from_utf8(key_bytes) else {
        set_error("gesture key is not utf-8".into());
        return 1;
    };
    let value = match read_value(tag, number, text_ptr, text_len) {
        Ok(value) => value,
        Err(err) => {
            set_error(err);
            return 1;
        }
    };
    match with_turn(|turn| {
        turn.set_gesture_prop(handle, key, value.clone());
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn bind_gesture_event(handle: u32, slot: u32, handler: u32) -> i32 {
    if !(1..=9).contains(&slot) {
        set_error(format!("bad gesture event slot {slot}"));
        return 1;
    }
    match with_turn(|turn| {
        turn.bind_gesture_event(handle, slot as u8, handler);
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn dispose_gesture(handle: u32) -> i32 {
    match with_turn(|turn| {
        turn.dispose_gesture(handle);
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn move_gesture(handle: u32, target: u32, index: u32) -> i32 {
    match with_turn(|turn| {
        turn.move_gesture(handle, target, index);
        Ok(())
    }) {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn commit() -> u32 {
    let result = with_turn(|turn| turn.commit());
    match result {
        Ok(bytes) => {
            let len = bytes.len() as u32;
            PENDING.with(|queue| queue.borrow_mut().push_back(bytes));
            len
        }
        Err(err) => {
            set_error(err);
            0
        }
    }
}

#[no_mangle]
pub extern "C" fn layout() -> i32 {
    let result = drain_pending().and_then(|()| {
        with_shell(|shell| {
            shell.compute_layout(
                &mut |text, font_size, line_height, font_weight, max_width| {
                    let mut out = [0f32; 2];
                    unsafe {
                        host_measure(
                            text.as_ptr() as u32,
                            text.len() as u32,
                            font_size,
                            line_height,
                            font_weight as u32,
                            u32::from(max_width.is_some()),
                            max_width.unwrap_or(0.0),
                            out.as_mut_ptr() as u32,
                        );
                    }
                    (out[0], out[1])
                },
            )?;
            shell.refresh_caret(&mut wasm_layout)
        })
    });
    match result {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn apply_ops(ptr: *const u8, len: u32) -> i32 {
    if let Err(err) = drain_pending() {
        set_error(err);
        return 1;
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    match with_shell(|shell| shell.apply_ops(bytes).map(|_| ())) {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn tap(x: f32, y: f32) -> u32 {
    if let Err(err) = drain_pending() {
        set_error(err);
        return 0;
    }
    with_shell(|shell| Ok(shell.tap(x, y)))
        .ok()
        .flatten()
        .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn scroll_wheel(x: f32, y: f32, dx: f32, dy: f32) -> i32 {
    if let Err(err) = drain_pending() {
        set_error(err);
        return 0;
    }
    match with_shell(|shell| Ok(shell.scroll_wheel(x, y, dx, dy))) {
        Ok(changed) => i32::from(changed),
        Err(err) => {
            set_error(err);
            0
        }
    }
}

#[no_mangle]
pub extern "C" fn paint() -> u32 {
    if let Err(err) = drain_pending() {
        set_error(err);
        return 0;
    }
    let bytes = with_shell(|shell| Ok(encode_paint(&shell.paint()))).unwrap_or_default();
    store_scratch(bytes)
}

#[no_mangle]
pub extern "C" fn encode_text_op(seq: u32, handle: u32, ptr: *const u8, len: u32) -> u32 {
    let text = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    let Ok(text) = std::str::from_utf8(text) else {
        set_error("op text is not utf-8".into());
        return 0;
    };
    store_scratch(encode_set_text(seq, handle, text))
}

/// `files` is a sequence of `u32 path_len + path + u32 data_len + data`.
#[no_mangle]
pub extern "C" fn validate(
    manifest_ptr: *const u8,
    manifest_len: u32,
    files_ptr: *const u8,
    files_len: u32,
) -> i32 {
    let manifest = unsafe { std::slice::from_raw_parts(manifest_ptr, manifest_len as usize) };
    let files = unsafe { std::slice::from_raw_parts(files_ptr, files_len as usize) };
    let Ok(manifest) = std::str::from_utf8(manifest) else {
        set_error("manifest is not utf-8".into());
        return 1;
    };
    let parsed = match parse_files(files) {
        Ok(files) => files,
        Err(err) => {
            set_error(err);
            return 1;
        }
    };
    let refs: Vec<(&str, &[u8])> = parsed
        .iter()
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
        .collect();
    match validate_bundle_hashes(manifest, &refs) {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

fn parse_files(mut bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut files = Vec::new();
    while !bytes.is_empty() {
        if bytes.len() < 4 {
            return Err("truncated file table".into());
        }
        let path_len = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
        bytes = &bytes[4..];
        if bytes.len() < path_len + 4 {
            return Err("truncated file path".into());
        }
        let path = std::str::from_utf8(&bytes[..path_len])
            .map_err(|_| "file path is not utf-8")?
            .to_owned();
        bytes = &bytes[path_len..];
        let data_len = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
        bytes = &bytes[4..];
        if bytes.len() < data_len {
            return Err("truncated file bytes".into());
        }
        files.push((path, bytes[..data_len].to_vec()));
        bytes = &bytes[data_len..];
    }
    Ok(files)
}
