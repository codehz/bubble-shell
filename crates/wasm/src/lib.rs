//! Single-threaded wasm ABI. The host copies bytes in and paint/op bytes out.
//! `host_measure` writes two f32s at `out_ptr` and must not grow wasm memory.

use bubble_core::{encode_set_text, validate_bundle_hashes, PaintCmd, Shell};
use std::cell::RefCell;

thread_local! {
    static SHELL: RefCell<Option<Shell>> = const { RefCell::new(None) };
    static SCRATCH: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static ERROR: RefCell<String> = const { RefCell::new(String::new()) };
}
#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn host_measure(text_ptr: u32, text_len: u32, font_size: f32, mode: u32, avail: f32, out_ptr: u32);
}
fn set_error(message: String) {
    ERROR.with(|slot| *slot.borrow_mut() = message);
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
    unsafe { drop(Vec::from_raw_parts(ptr, size as usize, size as usize)) };
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

#[no_mangle]
pub extern "C" fn mount(ptr: *const u8, len: u32) -> i32 {
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    match Shell::mount(bytes) {
        Ok(shell) => {
            SHELL.with(|slot| *slot.borrow_mut() = Some(shell));
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
    SHELL.with(|slot| {
        if let Some(shell) = slot.borrow_mut().as_mut() {
            shell.set_viewport(width, height);
        }
    });
}

#[no_mangle]
pub extern "C" fn layout() -> i32 {
    let result = SHELL.with(|slot| {
        let mut shell = slot.borrow_mut();
        let Some(shell) = shell.as_mut() else {
            return Err("shell is not mounted".to_string());
        };
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
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    let result = SHELL.with(|slot| {
        let mut shell = slot.borrow_mut();
        let Some(shell) = shell.as_mut() else {
            return Err("shell is not mounted".to_string());
        };
        shell.apply_ops(bytes).map(|_| ())
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
pub extern "C" fn tap(x: f32, y: f32) -> u32 {
    SHELL.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|shell| shell.tap(x, y))
            .unwrap_or(0)
    })
}

#[no_mangle]
pub extern "C" fn scroll_wheel(x: f32, y: f32, dy: f32) -> i32 {
    SHELL.with(|slot| {
        let mut shell = slot.borrow_mut();
        let Some(shell) = shell.as_mut() else {
            return 0;
        };
        i32::from(shell.scroll_wheel(x, y, dy))
    })
}

#[no_mangle]
pub extern "C" fn paint() -> u32 {
    let bytes = SHELL.with(|slot| {
        let shell = slot.borrow();
        let Some(shell) = shell.as_ref() else {
            return Vec::new();
        };
        encode_paint(&shell.paint())
    });
    let len = bytes.len() as u32;
    SCRATCH.with(|slot| *slot.borrow_mut() = bytes);
    len
}

#[no_mangle]
pub extern "C" fn encode_text_op(seq: u32, handle: u32, ptr: *const u8, len: u32) -> u32 {
    let text = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    let Ok(text) = std::str::from_utf8(text) else {
        set_error("op text is not utf-8".into());
        return 0;
    };
    let bytes = encode_set_text(seq, handle, text);
    let len = bytes.len() as u32;
    SCRATCH.with(|slot| *slot.borrow_mut() = bytes);
    len
}

/// `files` is a sequence of `u32 path_len + path + u32 data_len + data`.
#[no_mangle]
pub extern "C" fn validate(manifest_ptr: *const u8, manifest_len: u32, files_ptr: *const u8, files_len: u32) -> i32 {
    let manifest = unsafe { std::slice::from_raw_parts(manifest_ptr, manifest_len as usize) };
    let Ok(manifest) = std::str::from_utf8(manifest) else {
        set_error("manifest is not utf-8".into());
        return 1;
    };
    let blob = unsafe { std::slice::from_raw_parts(files_ptr, files_len as usize) };
    match parse_files(blob) {
        Ok(files) => {
            let refs: Vec<(&str, &[u8])> = files.iter().map(|(path, data)| (path.as_str(), data.as_slice())).collect();
            match validate_bundle_hashes(manifest, &refs) {
                Ok(()) => 0,
                Err(err) => {
                    set_error(err);
                    1
                }
            }
        }
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
        let path = std::str::from_utf8(&bytes[..path_len]).map_err(|_| "file path is not utf-8")?.to_owned();
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

fn encode_paint(cmds: &[PaintCmd]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(cmds.len() as u32).to_le_bytes());
    for cmd in cmds {
        match cmd {
            PaintCmd::Fill { x, y, w, h, radius, color } => {
                out.push(1);
                push_f32(&mut out, *x);
                push_f32(&mut out, *y);
                push_f32(&mut out, *w);
                push_f32(&mut out, *h);
                push_f32(&mut out, *radius);
                out.extend_from_slice(&color.to_le_bytes());
            }
            PaintCmd::Text { x, y, w, font_size, color, text } => {
                out.push(2);
                push_f32(&mut out, *x);
                push_f32(&mut out, *y);
                push_f32(&mut out, *w);
                push_f32(&mut out, *font_size);
                out.extend_from_slice(&color.to_le_bytes());
                let bytes = text.as_bytes();
                out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
                out.extend_from_slice(bytes);
            }
            PaintCmd::PushClip { x, y, w, h } => {
                out.push(3);
                push_f32(&mut out, *x);
                push_f32(&mut out, *y);
                push_f32(&mut out, *w);
                push_f32(&mut out, *h);
            }
            PaintCmd::PopClip => out.push(4),
        }
    }
    out
}

fn push_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}
