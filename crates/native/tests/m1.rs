use std::ffi::{CStr, CString};
use std::ptr;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use bubble_ffi::{
    bubble_create, bubble_destroy, bubble_eval, bubble_faults, bubble_finish_request,
    bubble_font_count, bubble_font_epoch, bubble_frame, bubble_generation, bubble_last_error,
    bubble_load, bubble_node_box, bubble_node_text, bubble_pointer_down, bubble_poll_request,
    bubble_post_eval, bubble_register_font, bubble_scroll, bubble_scroll_y_of, bubble_set_visible,
    bubble_status,
};

// The C ABI owns one process-global shell. Cargo runs tests in one process in
// parallel by default, so the scenarios take turns instead of racing on it.
static SHELL: Mutex<()> = Mutex::new(());

fn exclusive() -> MutexGuard<'static, ()> {
    SHELL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn c(text: &str) -> CString {
    CString::new(text).unwrap()
}

fn error() -> String {
    let ptr = bubble_last_error();
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

fn text(handle: u32) -> String {
    let mut buf = [0i8; 256];
    let code = bubble_node_text(handle, buf.as_mut_ptr(), buf.len() as u32);
    assert_eq!(code, 0, "{}", error());
    unsafe { CStr::from_ptr(buf.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

fn status() -> String {
    let mut buf = [0i8; 64];
    assert_eq!(
        bubble_status(buf.as_mut_ptr(), buf.len() as u32),
        0,
        "{}",
        error()
    );
    unsafe { CStr::from_ptr(buf.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

fn try_eval(source: &str) -> Result<String, String> {
    let source = c(source);
    let mut buf = [0i8; 128];
    let code = bubble_eval(source.as_ptr(), buf.as_mut_ptr(), buf.len() as u32);
    if code != 0 {
        return Err(error());
    }
    Ok(unsafe { CStr::from_ptr(buf.as_ptr()) }
        .to_string_lossy()
        .into_owned())
}

fn eval(source: &str) -> String {
    try_eval(source).unwrap_or_else(|err| panic!("{err}"))
}

fn load_counter() {
    let manifest = include_str!("../../../examples/web/bundle/manifest.json");
    let entry = include_bytes!("../../../examples/web/bundle/main.js");
    let manifest = c(manifest);
    let code = bubble_load(manifest.as_ptr(), entry.as_ptr(), entry.len() as u32);
    assert_eq!(code, 0, "{}", error());
}

#[test]
fn counter_tap_scroll_and_font_epoch() {
    let _guard = exclusive();
    bubble_destroy();
    assert_eq!(bubble_create(ptr::null()), 0, "{}", error());
    load_counter();
    let mut paint = ptr::null();
    let mut len = 0u32;
    assert_eq!(
        bubble_frame(320.0, 180.0, &mut paint, &mut len),
        0,
        "{}",
        error()
    );
    assert!(len > 16, "paint bytes {len}");
    assert_eq!(text(3), "Count: 0");
    let (mut x, mut y, mut w, mut h) = (0.0, 0.0, 0.0, 0.0);
    assert_eq!(
        bubble_node_box(4, &mut x, &mut y, &mut w, &mut h),
        0,
        "{}",
        error()
    );
    assert!(w > 0.0 && h > 0.0, "button box {x},{y} {w}x{h}");
    let generation = bubble_generation();
    bubble_pointer_down(x + w / 2.0, y + h / 2.0);
    let deadline = Instant::now() + Duration::from_secs(2);
    while bubble_generation() == generation && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_ne!(
        bubble_generation(),
        generation,
        "tap did not reach the script"
    );
    assert_eq!(text(3), "Count: 1");
    let (mut sx, mut sy, mut sw, mut sh) = (0.0, 0.0, 0.0, 0.0);
    assert_eq!(bubble_node_box(6, &mut sx, &mut sy, &mut sw, &mut sh), 0);
    bubble_scroll(sx + 1.0, sy + 1.0, 0.0, 40.0);
    let scrolled = bubble_scroll_y_of(6);
    assert!(scrolled > 0.0, "scroll offset {scrolled}");
    let family = c("Demo");
    bubble_register_font(family.as_ptr(), b"font-bytes".as_ptr(), 10);
    assert_eq!(bubble_font_count(), 1);
    assert_eq!(bubble_font_epoch(), 1);
    bubble_destroy();
}

#[test]
fn dead_loop_is_interrupted_without_blocking_frames() {
    let _guard = exclusive();
    bubble_destroy();
    assert_eq!(bubble_create(ptr::null()), 0, "{}", error());
    load_counter();
    let frames = std::sync::atomic::AtomicU32::new(0);
    let stop = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                let mut paint = ptr::null();
                let mut len = 0;
                let code = bubble_frame(200.0, 200.0, &mut paint, &mut len);
                assert_eq!(code, 0, "{}", error());
                frames.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(8));
            }
        });
        let source = c("while (true) {}");
        bubble_post_eval(source.as_ptr());
        let deadline = Instant::now() + Duration::from_secs(4);
        while bubble_faults() == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    assert!(bubble_faults() >= 1, "status {}", status());
    assert_eq!(status(), "running");
    assert!(
        frames.load(std::sync::atomic::Ordering::Relaxed) >= 10,
        "ui frames stalled"
    );
    assert_eq!(eval("1 + 1"), "2");
    bubble_destroy();
}

#[test]
fn timers_pause_while_hidden_and_host_calls_roundtrip() {
    let _guard = exclusive();
    bubble_destroy();
    assert_eq!(bubble_create(ptr::null()), 0, "{}", error());
    load_counter();
    bubble_set_visible(0);
    assert_eq!(
        eval("globalThis.ticks = 0; setTimeout(() => { globalThis.ticks = 1 }, 0); 'armed'"),
        "armed"
    );
    assert_eq!(eval("String(globalThis.ticks)"), "0");
    bubble_set_visible(1);
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut seen = String::new();
    while Instant::now() < deadline {
        seen = eval("String(globalThis.ticks)");
        if seen == "1" {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(seen, "1");
    assert_eq!(
        eval("bubbleHost.call('toast', { message: 'hi' }).then((value) => { globalThis.hostValue = value.ok }); 'queued'"),
        "queued"
    );
    let mut id = 0u32;
    let mut method = [0i8; 32];
    let mut params = [0i8; 128];
    let polled = bubble_poll_request(
        std::ptr::from_mut(&mut id),
        method.as_mut_ptr(),
        method.len() as u32,
        params.as_mut_ptr(),
        params.len() as u32,
    );
    assert_eq!(polled, 1, "{}", error());
    let method = unsafe { CStr::from_ptr(method.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    assert_eq!(method, "toast");
    let params = unsafe { CStr::from_ptr(params.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    assert_eq!(params, "{\"message\":\"hi\"}");
    let payload = c("{\"ok\":true}");
    bubble_finish_request(id, 1, payload.as_ptr());
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut host = String::new();
    while Instant::now() < deadline {
        host = eval("String(globalThis.hostValue)");
        if host == "true" {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(host, "true", "host payload {host}");
    bubble_destroy();
}

#[test]
fn a_second_shell_or_a_bad_manifest_is_rejected() {
    let _guard = exclusive();
    bubble_destroy();
    assert_eq!(bubble_create(ptr::null()), 0, "{}", error());
    assert_eq!(bubble_create(ptr::null()), 1);
    assert!(error().contains("already exists"), "{}", error());
    let manifest = c(r#"{"entry":"main.js","files":[]}"#);
    let entry = b"x";
    assert_eq!(
        bubble_load(manifest.as_ptr(), entry.as_ptr(), entry.len() as u32),
        1
    );
    assert!(!error().is_empty());
    load_counter();
    let mut paint = ptr::null();
    let mut len = 0u32;
    assert_eq!(
        bubble_frame(320.0, 180.0, &mut paint, &mut len),
        0,
        "{}",
        error()
    );
    assert_eq!(text(3), "Count: 0");
    bubble_destroy();
}

#[test]
fn memory_limit_fault_rebuilds_the_shell() {
    let _guard = exclusive();
    bubble_destroy();
    assert_eq!(bubble_create(ptr::null()), 0, "{}", error());
    load_counter();
    let err = try_eval("new ArrayBuffer(96 * 1024 * 1024); 'allocated'").unwrap_err();
    assert!(err.to_ascii_lowercase().contains("memory"), "{err}");
    assert_eq!(bubble_faults(), 1, "status {}", status());
    assert_eq!(status(), "running");
    assert_eq!(text(3), "Count: 0");
    assert_eq!(eval("1 + 1"), "2");
    bubble_destroy();
}
