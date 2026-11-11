use std::ffi::{CStr, CString};
use std::ptr;
use std::time::{Duration, Instant};

use bubble_ffi::{
    bubble_create, bubble_destroy, bubble_eval, bubble_faults, bubble_finish_request, bubble_font_count,
    bubble_font_epoch, bubble_frame, bubble_generation, bubble_last_error, bubble_load, bubble_node_box,
    bubble_node_text, bubble_pointer_down, bubble_poll_request, bubble_post_eval, bubble_register_font,
    bubble_scroll, bubble_scroll_of, bubble_set_visible, bubble_status,
};

fn c(text: &str) -> CString {
    CString::new(text).unwrap()
}

fn error() -> String {
    unsafe {
        let ptr = bubble_last_error();
        if ptr.is_null() {
            return String::new();
        }
        CStr::from_ptr(ptr).to_string_lossy().into_owned()
    }
}

fn text(handle: u32) -> String {
    let mut buf = [0i8; 256];
    let code = unsafe { bubble_node_text(handle, buf.as_mut_ptr(), buf.len() as u32) };
    assert_eq!(code, 0, "{}", error());
    unsafe { CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned() }
}

fn status() -> String {
    let mut buf = [0i8; 64];
    assert_eq!(unsafe { bubble_status(buf.as_mut_ptr(), buf.len() as u32) }, 0, "{}", error());
    unsafe { CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned() }
}

fn eval(source: &str) -> String {
    let source = c(source);
    let mut buf = [0i8; 128];
    let code = unsafe { bubble_eval(source.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) };
    assert_eq!(code, 0, "{}", error());
    unsafe { CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned() }
}

fn load_counter() {
    let manifest = include_str!("../../../examples/web/bundle/manifest.json");
    let entry = include_bytes!("../../../examples/web/bundle/main.js");
    let manifest = c(manifest);
    let code = unsafe { bubble_load(manifest.as_ptr(), entry.as_ptr(), entry.len() as u32) };
    assert_eq!(code, 0, "{}", error());
}

#[test]
fn counter_tap_scroll_and_font_epoch() {
    unsafe { bubble_destroy() };
    assert_eq!(unsafe { bubble_create(ptr::null()) }, 0, "{}", error());
    load_counter();
    let mut paint = ptr::null();
    let mut len = 0u32;
    assert_eq!(unsafe { bubble_frame(320.0, 180.0, &mut paint, &mut len) }, 0, "{}", error());
    assert!(len > 16, "paint bytes {len}");
    assert_eq!(text(3), "Count: 0");
    let (mut x, mut y, mut w, mut h) = (0.0, 0.0, 0.0, 0.0);
    assert_eq!(unsafe { bubble_node_box(4, &mut x, &mut y, &mut w, &mut h) }, 0, "{}", error());
    assert!(w > 0.0 && h > 0.0, "button box {x},{y} {w}x{h}");
    let generation = unsafe { bubble_generation() };
    unsafe { bubble_pointer_down(x + w / 2.0, y + h / 2.0) };
    let deadline = Instant::now() + Duration::from_secs(2);
    while unsafe { bubble_generation() } == generation && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_ne!(unsafe { bubble_generation() }, generation, "tap did not reach the script");
    assert_eq!(text(3), "Count: 1");
    let (mut sx, mut sy, mut sw, mut sh) = (0.0, 0.0, 0.0, 0.0);
    assert_eq!(unsafe { bubble_node_box(6, &mut sx, &mut sy, &mut sw, &mut sh) }, 0);
    unsafe { bubble_scroll(sx + 1.0, sy + 1.0, 40.0) };
    let scrolled = unsafe { bubble_scroll_of(6) };
    assert!(scrolled > 0.0, "scroll offset {scrolled}");
    let family = c("Demo");
    unsafe { bubble_register_font(family.as_ptr(), b"font-bytes".as_ptr(), 10) };
    assert_eq!(unsafe { bubble_font_count() }, 1);
    assert_eq!(unsafe { bubble_font_epoch() }, 1);
    unsafe { bubble_destroy() };
}

#[test]
fn dead_loop_is_interrupted_without_blocking_frames() {
    unsafe { bubble_destroy() };
    assert_eq!(unsafe { bubble_create(ptr::null()) }, 0, "{}", error());
    load_counter();
    let frames = std::sync::atomic::AtomicU32::new(0);
    let stop = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                let mut paint = ptr::null();
                let mut len = 0;
                let code = unsafe { bubble_frame(200.0, 200.0, &mut paint, &mut len) };
                assert_eq!(code, 0, "{}", error());
                frames.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(8));
            }
        });
        let source = c("while (true) {}");
        unsafe { bubble_post_eval(source.as_ptr()) };
        let deadline = Instant::now() + Duration::from_secs(4);
        while unsafe { bubble_faults() } == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    assert!(unsafe { bubble_faults() } >= 1, "status {}", status());
    assert_eq!(status(), "running");
    assert!(frames.load(std::sync::atomic::Ordering::Relaxed) >= 10, "ui frames stalled");
    assert_eq!(eval("1 + 1"), "2");
    unsafe { bubble_destroy() };
}

#[test]
fn timers_pause_while_hidden_and_host_calls_roundtrip() {
    unsafe { bubble_destroy() };
    assert_eq!(unsafe { bubble_create(ptr::null()) }, 0, "{}", error());
    load_counter();
    unsafe { bubble_set_visible(0) };
    assert_eq!(eval("globalThis.ticks = 0; setTimeout(() => { globalThis.ticks = 1 }, 0); 'armed'"), "armed");
    assert_eq!(eval("String(globalThis.ticks)"), "0");
    unsafe { bubble_set_visible(1) };
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
    let polled = unsafe {
        bubble_poll_request(
            std::ptr::from_mut(&mut id),
            method.as_mut_ptr(),
            method.len() as u32,
            params.as_mut_ptr(),
            params.len() as u32,
        )
    };
    assert_eq!(polled, 1, "{}", error());
    let method = unsafe { CStr::from_ptr(method.as_ptr()).to_string_lossy().into_owned() };
    assert_eq!(method, "toast");
    let payload = c("{\"ok\":true}");
    unsafe { bubble_finish_request(id, 1, payload.as_ptr()) };
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
    unsafe { bubble_destroy() };
}
