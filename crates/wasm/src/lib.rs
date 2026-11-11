//! Single-threaded wasm ABI. The host copies bytes in and paint/op bytes out.
//! `host_measure` writes two f32s at `out_ptr` and must not grow wasm memory.
//!
//! The script side (`Turn`) only queues ops. `commit` pushes a batch into the
//! pending queue; layout/paint/hit-test drain that queue into the `Shell`
//! first. This keeps the tree-owning code on one side of the JS boundary.

use std::cell::RefCell;
use std::collections::VecDeque;

use bubble_core::{
    encode_paint, encode_set_text, validate_bundle_hashes, NodeDesc, Op, Shell, Turn, Value,
    SPAWN_PROPS,
};

thread_local! {
    static TURN: RefCell<Option<Turn>> = const { RefCell::new(None) };
    static SHELL: RefCell<Option<Shell>> = const { RefCell::new(None) };
    static PENDING: RefCell<VecDeque<Vec<u8>>> = const { RefCell::new(VecDeque::new()) };
    static SCRATCH: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static ERROR: RefCell<String> = const { RefCell::new(String::new()) };
}

#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn host_measure(
        text_ptr: u32,
        text_len: u32,
        font_size: f32,
        mode: u32,
        avail: f32,
        out_ptr: u32,
    );
}

fn set_error(message: String) {
    ERROR.with(|slot| *slot.borrow_mut() = message);
}

fn store_scratch(bytes: Vec<u8>) -> u32 {
    let len = bytes.len() as u32;
    SCRATCH.with(|slot| *slot.borrow_mut() = bytes);
    len
}

fn with_turn<T>(body: impl FnOnce(&mut Turn) -> Result<T, String>) -> Result<T, String> {
    TURN.with(|slot| {
        let mut slot = slot.borrow_mut();
        let turn = slot
            .as_mut()
            .ok_or_else(|| "shell is not mounted".to_string())?;
        body(turn)
    })
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
        Ok(())
    })
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

#[no_mangle]
pub extern "C" fn boot() -> i32 {
    match Shell::blank() {
        Ok(shell) => {
            TURN.with(|slot| *slot.borrow_mut() = Some(Turn::new()));
            SHELL.with(|slot| *slot.borrow_mut() = Some(shell));
            PENDING.with(|queue| queue.borrow_mut().clear());
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
pub extern "C" fn signal_new(initial: f64) -> u32 {
    with_turn(|turn| Ok(turn.signal(initial))).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn signal_get(id: u32) -> f64 {
    match with_turn(|turn| turn.get(id)) {
        Ok(value) => value,
        Err(err) => {
            set_error(err);
            f64::NAN
        }
    }
}

#[no_mangle]
pub extern "C" fn signal_set(id: u32, value: f64) -> i32 {
    match with_turn(|turn| turn.set(id, value)) {
        Ok(()) => 0,
        Err(err) => {
            set_error(err);
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn track_begin(binding: u32) {
    let _ = with_turn(|turn| {
        turn.track(binding);
        Ok(())
    });
}

#[no_mangle]
pub extern "C" fn track_end() {
    let _ = with_turn(|turn| {
        turn.untrack();
        Ok(())
    });
}

/// Writes `u32 count + count * u32` into scratch. Returns the byte length.
#[no_mangle]
pub extern "C" fn take_dirty() -> u32 {
    let ids = with_turn(|turn| Ok(turn.take_dirty())).unwrap_or_default();
    let mut bytes = Vec::with_capacity(4 + ids.len() * 4);
    bytes.extend_from_slice(&(ids.len() as u32).to_le_bytes());
    for id in ids {
        bytes.extend_from_slice(&id.to_le_bytes());
    }
    store_scratch(bytes)
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
            shell.layout(&mut |text, font_size, max_width| {
                let mut out = [0f32; 2];
                unsafe {
                    host_measure(
                        text.as_ptr() as u32,
                        text.len() as u32,
                        font_size,
                        u32::from(max_width.is_some()),
                        max_width.unwrap_or(0.0),
                        out.as_mut_ptr() as u32,
                    );
                }
                (out[0], out[1])
            })
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
pub extern "C" fn scroll_wheel(x: f32, y: f32, dy: f32) -> i32 {
    if let Err(err) = drain_pending() {
        set_error(err);
        return 0;
    }
    match with_shell(|shell| Ok(shell.scroll_wheel(x, y, dy))) {
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
