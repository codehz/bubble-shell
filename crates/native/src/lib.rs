//! C ABI around one QuickJS script thread and one retained tree.
//!
//! `bubble_frame`, hit testing, and scrolling run on the caller thread.
//! Script evaluation does not. A one-second interrupt rebuilds the context
//! and remounts the loaded bundle.

mod paint;
mod script;

use std::collections::VecDeque;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use bubble_core::{Shell, Turn};

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
    i32,
    *mut f32,
    *mut f32,
);

pub struct ShareTurn(Turn);

// Taffy packs lengths in a tagged pointer. This build does not enable `calc`,
// so those bits are not heap pointers and the tree can move between threads.
unsafe impl Send for ShareTurn {}

pub struct Shared {
    turn: Arc<Mutex<ShareTurn>>,
    host: HostFns,
    last_error: Mutex<String>,
    paint: Mutex<Vec<u8>>,
    status: Mutex<String>,
    faults: AtomicU32,
    generation: AtomicU32,
    visible: AtomicBool,
    fonts: Mutex<Vec<(String, Vec<u8>)>>,
    epoch: AtomicU32,
    requests: Arc<Mutex<VecDeque<HostRequest>>>,
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
    let guard = ENGINE.lock().map_err(|_| "engine lock poisoned".to_string())?;
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

#[no_mangle]
pub extern "C" fn bubble_create(host: *const BubbleHost) -> i32 {
    let result = catch(|| {
        let mut slot = ENGINE.lock().map_err(|_| "engine lock poisoned".to_string())?;
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
        let turn = ShareTurn(Turn::new(Shell::blank().map_err(|err| err)?));
        let shared = Arc::new(Shared {
            turn: Arc::new(Mutex::new(turn)),
            host: HostFns { measure, ctx },
            last_error: Mutex::new(String::new()),
            paint: Mutex::new(Vec::new()),
            status: Mutex::new("created".into()),
            faults: AtomicU32::new(0),
            generation: AtomicU32::new(0),
            visible: AtomicBool::new(true),
            fonts: Mutex::new(Vec::new()),
            epoch: AtomicU32::new(0),
            requests: Arc::new(Mutex::new(VecDeque::new())),
        });
        let (tx, thread) = script::start(Arc::clone(&shared));
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
pub extern "C" fn bubble_load(manifest_json: *const c_char, entry: *const u8, entry_len: u32) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        let manifest = cstr(manifest_json)?;
        if entry.is_null() && entry_len != 0 {
            return Err("entry pointer is null".into());
        }
        let bytes = unsafe { std::slice::from_raw_parts(entry, entry_len as usize) }.to_vec();
        let (ack_tx, ack_rx) = std::sync::mpsc::channel();
        engine
            .tx
            .send(Cmd::Load { manifest, entry: bytes, ack: ack_tx })
            .map_err(|_| "script thread is gone".to_string())?;
        ack_rx.recv().map_err(|_| "script thread dropped the load".to_string())?
    });
    finish(result)
}

#[no_mangle]
pub extern "C" fn bubble_frame(width: f32, height: f32, paint: *mut *const u8, paint_len: *mut u32) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        let mut turn = engine.shared.turn.lock().map_err(|_| "tree lock poisoned".to_string())?;
        turn.0.shell_mut().set_viewport(width, height);
        let host = &engine.shared.host;
        turn.0.shell_mut().layout(&mut |text, font_size, max_width| {
            if let Some(measure) = host.measure {
                let mut w = 0.0f32;
                let mut h = 0.0f32;
                let has_max = max_width.is_some();
                measure(
                    host.ctx as *mut c_void,
                    text.as_ptr(),
                    text.len() as u32,
                    font_size,
                    max_width.unwrap_or(0.0),
                    i32::from(has_max),
                    &mut w,
                    &mut h,
                );
                (w, h)
            } else {
                fallback_measure(text, font_size)
            }
        })?;
        let bytes = paint::encode_paint(&turn.0.shell().paint());
        drop(turn);
        let mut slot = engine.shared.paint.lock().map_err(|_| "paint lock poisoned".to_string())?;
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

#[no_mangle]
pub extern "C" fn bubble_pointer_down(x: f32, y: f32) {
    let _ = catch(|| {
        let engine = engine()?;
        let handler = {
            let turn = engine.shared.turn.lock().map_err(|_| "tree lock poisoned".to_string())?;
            turn.0.shell().tap(x, y)
        };
        if let Some(handler) = handler {
            engine.tx.send(Cmd::Press { handler }).map_err(|_| "script thread is gone".to_string())?;
        }
        Ok(())
    });
}

#[no_mangle]
pub extern "C" fn bubble_scroll(x: f32, y: f32, dy: f32) {
    let _ = catch(|| {
        let engine = engine()?;
        let mut turn = engine.shared.turn.lock().map_err(|_| "tree lock poisoned".to_string())?;
        turn.0.shell_mut().scroll_wheel(x, y, dy);
        Ok(())
    });
}

#[no_mangle]
pub extern "C" fn bubble_set_visible(visible: i32) {
    if let Ok(engine) = engine() {
        engine.shared.visible.store(visible != 0, Ordering::Relaxed);
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
        let mut fonts = engine.shared.fonts.lock().map_err(|_| "font lock poisoned".to_string())?;
        fonts.push((family, bytes));
        engine.shared.epoch.fetch_add(1, Ordering::Relaxed);
        Ok(())
    });
    let _ = finish(result);
}

#[no_mangle]
pub extern "C" fn bubble_font_epoch() -> u32 {
    engine().map(|engine| engine.shared.epoch.load(Ordering::Relaxed)).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn bubble_font_count() -> u32 {
    engine()
        .ok()
        .and_then(|engine| engine.shared.fonts.lock().ok().map(|fonts| fonts.len() as u32))
        .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn bubble_node_text(handle: u32, buf: *mut c_char, cap: u32) -> i32 {
    copy_text(buf, cap, |engine| {
        let turn = engine.shared.turn.lock().map_err(|_| "tree lock poisoned".to_string())?;
        turn.0.shell().text(handle).map(|text| text.to_owned())
    })
}

#[no_mangle]
pub extern "C" fn bubble_node_box(handle: u32, x: *mut f32, y: *mut f32, w: *mut f32, h: *mut f32) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        let turn = engine.shared.turn.lock().map_err(|_| "tree lock poisoned".to_string())?;
        let (nx, ny, nw, nh) = turn.0.shell().node_box(handle)?;
        unsafe {
            if !x.is_null() { *x = nx; }
            if !y.is_null() { *y = ny; }
            if !w.is_null() { *w = nw; }
            if !h.is_null() { *h = nh; }
        }
        Ok(())
    });
    finish(result)
}

#[no_mangle]
pub extern "C" fn bubble_scroll_of(handle: u32) -> f32 {
    engine()
        .ok()
        .and_then(|engine| {
            let turn = engine.shared.turn.lock().ok()?;
            turn.0.shell().scroll_of(handle).ok()
        })
        .unwrap_or(f32::NAN)
}

#[no_mangle]
pub extern "C" fn bubble_eval(source: *const c_char, buf: *mut c_char, cap: u32) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        let source = cstr(source)?;
        let (ack_tx, ack_rx) = std::sync::mpsc::channel();
        engine
            .tx
            .send(Cmd::Eval { source, ack: Some(ack_tx) })
            .map_err(|_| "script thread is gone".to_string())?;
        let text = ack_rx.recv().map_err(|_| "script thread dropped eval".to_string())??;
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
    engine().map(|engine| engine.shared.generation.load(Ordering::Relaxed)).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn bubble_faults() -> u32 {
    engine().map(|engine| engine.shared.faults.load(Ordering::Relaxed)).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn bubble_status(buf: *mut c_char, cap: u32) -> i32 {
    copy_text(buf, cap, |engine| {
        engine.shared.status.lock().map(|status| status.clone()).map_err(|_| "status lock poisoned".into())
    })
}

#[no_mangle]
pub extern "C" fn bubble_last_error() -> *const c_char {
    ERROR_C.lock().ok().and_then(|slot| slot.as_ref().map(|text| text.as_ptr())).unwrap_or(std::ptr::null())
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
        let mut queue = engine.shared.requests.lock().map_err(|_| "request lock poisoned".to_string())?;
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
        let payload = if result.is_null() { String::new() } else { cstr(result)? };
        engine
            .tx
            .send(Cmd::Settle { id, ok: ok != 0, payload })
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

fn copy_text(buf: *mut c_char, cap: u32, body: impl FnOnce(EngineRef) -> Result<String, String>) -> i32 {
    let result = catch(|| {
        let engine = engine()?;
        let text = body(engine)?;
        write_buf(buf, cap, &text);
        Ok(())
    });
    finish(result)
}

fn fallback_measure(text: &str, font_size: f32) -> (f32, f32) {
    if text.is_empty() {
        return (0.0, 0.0);
    }
    let width = text.chars().count() as f32 * font_size * 0.5;
    (width, font_size * 1.25)
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
        let Ok(manifest) = env.get_string(&manifest) else { return 1 };
        let manifest = super::CString_from(&manifest.to_string_lossy());
        let Ok(bytes) = env.convert_byte_array(entry) else { return 1 };
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
            return env.new_byte_array(0).unwrap_or_else(|_| JObject::null().into());
        }
        let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
        let Ok(array) = env.byte_array_from_slice(bytes) else {
            return JObject::null().into();
        };
        array
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_pointerDown<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        x: jfloat,
        y: jfloat,
    ) {
        super::bubble_pointer_down(x, y);
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_scroll<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        x: jfloat,
        y: jfloat,
        dy: jfloat,
    ) {
        super::bubble_scroll(x, y, dy);
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_scrollOf<'a>(
        _env: JNIEnv<'a>,
        _class: JClass<'a>,
        handle: jint,
    ) -> jfloat {
        super::bubble_scroll_of(handle as u32)
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
        let Ok(family) = env.get_string(&family) else { return };
        let family = super::CString_from(&family.to_string_lossy());
        let Ok(bytes) = env.convert_byte_array(data) else { return };
        super::bubble_register_font(family.as_ptr(), bytes.as_ptr().cast(), bytes.len() as u32);
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_bubble_shell_BubbleNative_nodeText<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        handle: jint,
    ) -> JString<'a> {
        let mut buf = [0 as std::os::raw::c_char; 256];
        let code = super::bubble_node_text(handle as u32, buf.as_mut_ptr(), buf.len() as u32);
        let text = if code == 0 {
            unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned()
        } else {
            String::new()
        };
        env.new_string(text).unwrap_or_else(|_| env.new_string("").unwrap())
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
        let Ok(source) = env.get_string(&source) else { return };
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
    std::ffi::CString::new(text.replace('\0', " ")).unwrap_or_else(|_| std::ffi::CString::new("").unwrap())
}
