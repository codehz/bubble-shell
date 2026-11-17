use std::cell::Cell;
use std::ffi::{CStr, CString};
use std::ptr;
use std::time::{Duration, Instant};

use bubble_ffi::{
    bubble_create, bubble_destroy, bubble_edit, bubble_edit_batch_begin, bubble_edit_batch_end,
    bubble_edit_commit, bubble_edit_compose, bubble_edit_delete_surrounding,
    bubble_edit_finish_composition, bubble_edit_replace, bubble_edit_snapshot, bubble_eval,
    bubble_faults, bubble_finish_request, bubble_font_count, bubble_font_epoch, bubble_frame,
    bubble_generation, bubble_has_back_handler, bubble_last_error, bubble_load, bubble_node_box,
    bubble_node_text, bubble_pointer, bubble_poll_request, bubble_post_eval, bubble_register_font,
    bubble_scroll, bubble_scroll_y_of, bubble_send, bubble_set_params, bubble_set_storage_dir,
    bubble_set_visible, bubble_status, bubble_string_free, bubble_tick, BubbleShell,
};

thread_local! {
    static CURRENT: Cell<*mut BubbleShell> = const { Cell::new(ptr::null_mut()) };
}

fn shell() -> *mut BubbleShell {
    CURRENT.get()
}

struct Open;

impl Drop for Open {
    fn drop(&mut self) {
        close();
    }
}

fn close() {
    let current = CURRENT.replace(ptr::null_mut());
    if !current.is_null() {
        bubble_destroy(current);
    }
}

fn reopen() {
    close();
    let created = bubble_create(ptr::null());
    assert!(!created.is_null(), "bubble_create failed");
    CURRENT.set(created);
}

fn ffi_ok(err: *mut std::os::raw::c_char) {
    if err.is_null() {
        return;
    }
    let message = unsafe { CStr::from_ptr(err) }
        .to_string_lossy()
        .into_owned();
    bubble_string_free(err);
    panic!("{message}");
}

fn c(text: &str) -> CString {
    CString::new(text).unwrap()
}

/// Poll a script global until it equals `expected`; the script thread handles
/// host events asynchronously, so settled-variable promises resolve there.
fn wait_for(name: &str, expected: i64, context: &str) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if eval(&format!("Number(globalThis.{name})")) == expected.to_string() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{context}: {name} never matched; value={:?} dispatch={:?} faults={} err={}",
            eval(&format!("String(globalThis.{name})")),
            eval("String(globalThis.__varDispatch ?? 'none')"),
            bubble_faults(shell(),),
            error()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn error() -> String {
    let ptr = bubble_last_error(shell());
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

fn text(handle: u32) -> String {
    let mut buf = [0i8; 256];
    let code = bubble_node_text(shell(), handle, buf.as_mut_ptr(), buf.len() as u32);
    assert_eq!(code, 0, "{}", error());
    unsafe { CStr::from_ptr(buf.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

fn status() -> String {
    let mut buf = [0i8; 64];
    assert_eq!(
        bubble_status(shell(), buf.as_mut_ptr(), buf.len() as u32),
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
    let code = bubble_eval(shell(), source.as_ptr(), buf.as_mut_ptr(), buf.len() as u32);
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

fn load_fixture(name: &str) {
    let app = bubble_testing::bundle::app(name).unwrap_or_else(|err| panic!("{name}: {err}"));
    let manifest = c(&app.manifest);
    let err = bubble_load(
        shell(),
        manifest.as_ptr(),
        app.entry.as_ptr(),
        app.entry.len() as u32,
    );
    if !err.is_null() {
        let message = unsafe { CStr::from_ptr(err) }
            .to_string_lossy()
            .into_owned();
        bubble_string_free(err);
        panic!("{name}: {message}");
    }
}

fn load_counter() {
    load_fixture("counter");
}

fn load_editing() {
    load_fixture("editing");
}

#[test]
fn counter_tap_scroll_and_font_epoch() {
    let _guard = Open;
    reopen();
    load_counter();
    let mut paint = ptr::null();
    let mut len = 0u32;
    assert_eq!(
        bubble_frame(shell(), 320.0, 180.0, &mut paint, &mut len),
        0,
        "{}",
        error()
    );
    assert!(len > 16, "paint bytes {len}");
    assert_eq!(text(3), "Count: 0");
    let (mut x, mut y, mut w, mut h) = (0.0, 0.0, 0.0, 0.0);
    assert_eq!(
        bubble_node_box(shell(), 4, &mut x, &mut y, &mut w, &mut h),
        0,
        "{}",
        error()
    );
    assert!(w > 0.0 && h > 0.0, "button box {x},{y} {w}x{h}");
    let generation = bubble_generation(shell());
    let (tx, ty) = (x + w / 2.0, y + h / 2.0);
    assert_eq!(
        bubble_pointer(shell(), 0, 1, tx, ty, 0.0, 0, 1),
        0,
        "{}",
        error()
    );
    assert_eq!(
        bubble_pointer(shell(), 2, 1, tx, ty, 20.0, 0, 0),
        0,
        "{}",
        error()
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    while bubble_generation(shell()) == generation && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_ne!(
        bubble_generation(shell(),),
        generation,
        "tap did not reach the script"
    );
    assert_eq!(text(3), "Count: 1");
    let (mut sx, mut sy, mut sw, mut sh) = (0.0, 0.0, 0.0, 0.0);
    assert_eq!(
        bubble_node_box(shell(), 6, &mut sx, &mut sy, &mut sw, &mut sh),
        0
    );
    bubble_scroll(shell(), sx + 1.0, sy + 1.0, 0.0, 40.0);
    let scrolled = bubble_scroll_y_of(shell(), 6);
    assert!(scrolled > 0.0, "scroll offset {scrolled}");
    let family = c("Demo");
    bubble_register_font(shell(), family.as_ptr(), b"font-bytes".as_ptr(), 10);
    assert_eq!(bubble_font_count(shell(),), 1);
    assert_eq!(bubble_font_epoch(shell(),), 1);
    close();
}

#[test]
fn dead_loop_is_interrupted_without_blocking_frames() {
    let _guard = Open;
    reopen();
    let shell_addr = shell() as usize;
    load_counter();
    let frames = std::sync::atomic::AtomicU32::new(0);
    let stop = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            CURRENT.set(shell_addr as *mut BubbleShell);
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                let mut paint = ptr::null();
                let mut len = 0;
                let code = bubble_frame(shell(), 200.0, 200.0, &mut paint, &mut len);
                assert_eq!(code, 0, "{}", error());
                frames.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(8));
            }
        });
        let source = c("while (true) {}");
        bubble_post_eval(shell(), source.as_ptr());
        let deadline = Instant::now() + Duration::from_secs(4);
        while bubble_faults(shell()) == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    assert!(bubble_faults(shell()) >= 1, "status {}", status());
    assert_eq!(status(), "running");
    assert!(
        frames.load(std::sync::atomic::Ordering::Relaxed) >= 10,
        "ui frames stalled"
    );
    assert_eq!(eval("1 + 1"), "2");
    close();
}

#[test]
fn timers_pause_while_hidden_and_host_calls_roundtrip() {
    let _guard = Open;
    reopen();
    load_counter();
    bubble_set_visible(shell(), 0);
    assert_eq!(
        eval("globalThis.ticks = 0; setTimeout(() => { globalThis.ticks = 1 }, 0); 'armed'"),
        "armed"
    );
    assert_eq!(eval("String(globalThis.ticks)"), "0");
    bubble_set_visible(shell(), 1);
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
        shell(),
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
    bubble_finish_request(shell(), id, 1, payload.as_ptr());
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
    close();
}

#[test]
fn two_shells_stay_independent_and_a_bad_manifest_is_rejected() {
    let _guard = Open;
    let first = bubble_create(ptr::null());
    let second = bubble_create(ptr::null());
    assert!(!first.is_null() && !second.is_null() && first != second);
    CURRENT.set(second);
    let manifest = c(r#"{"entry":"main.js","files":[]}"#);
    let entry = b"x";
    let err = bubble_load(
        second,
        manifest.as_ptr(),
        entry.as_ptr(),
        entry.len() as u32,
    );
    assert!(!err.is_null());
    bubble_string_free(err);
    assert!(!error().is_empty());
    load_counter();
    let mut paint = ptr::null();
    let mut len = 0u32;
    assert_eq!(
        bubble_frame(shell(), 320.0, 180.0, &mut paint, &mut len),
        0,
        "{}",
        error()
    );
    assert_eq!(text(3), "Count: 0");
    bubble_destroy(first);
    close();
}

#[test]
fn memory_limit_fault_rebuilds_the_shell() {
    let _guard = Open;
    reopen();
    load_counter();
    let err = try_eval("new ArrayBuffer(96 * 1024 * 1024); 'allocated'").unwrap_err();
    assert!(err.to_ascii_lowercase().contains("memory"), "{err}");
    assert_eq!(bubble_faults(shell(),), 1, "status {}", status());
    assert_eq!(status(), "running");
    assert_eq!(text(3), "Count: 0");
    assert_eq!(eval("1 + 1"), "2");
    close();
}

#[test]
fn edit_batch_writes_through_the_focused_session() {
    let _guard = Open;
    reopen();
    load_editing();
    let mut paint = ptr::null();
    let mut len = 0u32;
    assert_eq!(
        bubble_frame(shell(), 320.0, 240.0, &mut paint, &mut len),
        0,
        "{}",
        error()
    );
    assert_eq!(
        bubble_edit(shell(), 0, ptr::null(), 0, 4, 0),
        0,
        "{}",
        error()
    );
    let (mut session, mut revision) = (0u64, 0u64);
    assert_eq!(
        bubble_edit_snapshot(
            shell(),
            &mut session,
            &mut revision,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        ),
        0,
        "{}",
        error()
    );
    assert_ne!(session, 0);
    assert_eq!(bubble_edit_batch_begin(shell(), session), 0, "{}", error());
    let a = CString::new("a").unwrap();
    let b = CString::new("b").unwrap();
    assert_eq!(
        bubble_edit(shell(), 1, a.as_ptr() as *const u8, 1, 0, 0),
        0,
        "{}",
        error()
    );
    assert_eq!(
        bubble_edit(shell(), 1, b.as_ptr() as *const u8, 1, 0, 0),
        0,
        "{}",
        error()
    );
    assert_eq!(text(4), "ab");
    let generation = bubble_generation(shell());
    assert_eq!(bubble_edit_batch_end(shell(), session), 0, "{}", error());
    assert_eq!(text(4), "ab");
    let deadline = Instant::now() + Duration::from_secs(2);
    while bubble_generation(shell()) == generation && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_ne!(bubble_generation(shell(),), generation, "{}", status());
    assert_ne!(bubble_edit_batch_end(shell(), session), 0);
}

fn edit_snapshot() -> (u64, u64, u32, u32, u32, u32, u32) {
    let (mut session, mut revision) = (0u64, 0u64);
    let (mut anchor, mut active) = (0u32, 0u32);
    let (mut comp_start, mut comp_end, mut composing) = (0u32, 0u32, 0u32);
    assert_eq!(
        bubble_edit_snapshot(
            shell(),
            &mut session,
            &mut revision,
            &mut anchor,
            &mut active,
            &mut comp_start,
            &mut comp_end,
            &mut composing,
        ),
        0,
        "{}",
        error()
    );
    (
        session, revision, anchor, active, comp_start, comp_end, composing,
    )
}

#[test]
fn ime_transactions_apply_new_cursor_and_composition() {
    let _guard = Open;
    reopen();
    load_editing();
    let mut paint = ptr::null();
    let mut len = 0u32;
    assert_eq!(
        bubble_frame(shell(), 320.0, 240.0, &mut paint, &mut len),
        0,
        "{}",
        error()
    );
    assert_eq!(
        bubble_edit(shell(), 0, ptr::null(), 0, 4, 0),
        0,
        "{}",
        error()
    );
    let (session, _, _, _, _, _, _) = edit_snapshot();
    assert_ne!(session, 0);

    let ab = CString::new("ab").unwrap();
    assert_eq!(
        bubble_edit_replace(shell(), session, 0, 0, ab.as_ptr() as *const u8, 2, 1, 0,),
        0,
        "{}",
        error()
    );
    let (_, _, anchor, active, _comp_start, comp_end, composing) = edit_snapshot();
    assert_eq!((anchor, active), (2, 2));
    assert_eq!(comp_end, u32::MAX);
    assert_eq!(composing, 0);
    assert_eq!(text(4), "ab");

    let mark = CString::new("X").unwrap();
    assert_eq!(
        bubble_edit_replace(shell(), session, 2, 2, mark.as_ptr() as *const u8, 1, 1, 1,),
        0,
        "{}",
        error()
    );
    let (_, _, anchor, active, comp_start, comp_end, composing) = edit_snapshot();
    assert_eq!((anchor, active), (3, 3));
    assert_eq!((comp_start, comp_end, composing), (2, 3, 1));
    assert_eq!(text(4), "abX");

    assert_eq!(
        bubble_edit_finish_composition(shell(), session),
        0,
        "{}",
        error()
    );
    let (_, _, _, _, _, comp_end, composing) = edit_snapshot();
    assert_eq!(comp_end, u32::MAX);
    assert_eq!(composing, 0);

    assert_eq!(
        bubble_edit_delete_surrounding(shell(), session, 1, 0),
        0,
        "{}",
        error()
    );
    let (_, _, anchor, active, _, _, _) = edit_snapshot();
    assert_eq!((anchor, active), (2, 2));
    assert_eq!(text(4), "ab");

    let mark = CString::new("X").unwrap();
    assert_eq!(
        bubble_edit_compose(shell(), session, mark.as_ptr() as *const u8, 1, 1),
        0,
        "{}",
        error()
    );
    let (_, _, _, _, comp_start, comp_end, composing) = edit_snapshot();
    assert_eq!((comp_start, comp_end, composing), (2, 3, 1));

    let commit = CString::new("Y").unwrap();
    assert_eq!(
        bubble_edit_commit(shell(), session, commit.as_ptr() as *const u8, 1, 1),
        0,
        "{}",
        error()
    );
    let (_, _, anchor, active, _, comp_end, composing) = edit_snapshot();
    assert_eq!((anchor, active), (3, 3));
    assert_eq!(comp_end, u32::MAX);
    assert_eq!(composing, 0);
    assert_eq!(text(4), "abY");

    assert_ne!(
        bubble_edit_delete_surrounding(shell(), session + 1, 1, 0),
        0,
        "stale session must be rejected"
    );
}

/// Load a hand-written bundle with a manifest that matches its hash.
fn try_load_source(code: &str) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(code.as_bytes());
    let hash: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    let manifest = format!(
        "{{\"schema\":1,\"entry\":\"main.js\",\"files\":[{{\"path\":\"main.js\",\"sha256\":\"{hash}\",\"size\":{}}}],\"signature\":\"test\"}}",
        code.len()
    );
    let manifest = c(&manifest);
    let err = bubble_load(shell(), manifest.as_ptr(), code.as_ptr(), code.len() as u32);
    if err.is_null() {
        Ok(())
    } else {
        let message = unsafe { CStr::from_ptr(err) }
            .to_string_lossy()
            .into_owned();
        bubble_string_free(err);
        Err(message)
    }
}

#[test]
fn host_modules_resolve_to_one_runtime_instance() {
    let _guard = Open;
    reopen();
    try_load_source(
        r#"import * as shell from "bubble-shell";
import * as rt from "bubble:rt";
if (shell !== rt) throw new Error("bubble-shell and bubble:rt must be one module");
const count = shell.signal(41);
export default function App() {
  shell.primitive("View", {}, () => {
    shell.primitive("Text", {}, () => "N=" + (count() + 1));
  });
}
"#,
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let mut paint = ptr::null();
    let mut len = 0u32;
    assert_eq!(
        bubble_frame(shell(), 200.0, 100.0, &mut paint, &mut len),
        0,
        "{}",
        error()
    );
    assert_eq!(text(3), "N=42");
}

#[test]
fn relative_imports_are_rejected_by_the_loader() {
    let _guard = Open;
    reopen();
    let error =
        try_load_source("import { helper } from \"./helper.js\";\nhelper();\n").unwrap_err();
    assert!(error.contains("relative imports"), "{error}");
}

#[test]
fn storage_params_and_host_events_roundtrip() {
    let _guard = Open;
    reopen();
    let params = c(r#"{"id":"42"}"#);
    ffi_ok(bubble_set_params(shell(), params.as_ptr()));
    let early = c("ping");
    let body = c(r#"{"n":1}"#);
    ffi_ok(bubble_send(shell(), early.as_ptr(), body.as_ptr()));
    load_counter();
    assert_eq!(eval("JSON.stringify(bubbleHost.params)"), r#"{"id":"42"}"#);
    assert_eq!(
        eval("bubbleStorage.set('k','v'); bubbleStorage.get('k')"),
        "v"
    );
    eval("bubbleHost.on('ping', (data) => { globalThis.seen = data.n })");
    assert_eq!(eval("String(globalThis.seen)"), "1");
    let later = c(r#"{"n":2}"#);
    ffi_ok(bubble_send(shell(), early.as_ptr(), later.as_ptr()));
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline && eval("String(globalThis.seen)") != "2" {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(eval("String(globalThis.seen)"), "2");
    close();
}

#[test]
fn back_handler_gates_back_events() {
    let _guard = Open;
    reopen();
    assert_eq!(bubble_has_back_handler(shell(),), 0);
    try_load_source("export const ready = true;\nexport default function App() {}\n").unwrap();
    assert_eq!(bubble_has_back_handler(shell(),), 0);
    eval(
        "globalThis.backs = 0; globalThis.seen = 0; bubbleHost.onBack((value) => { globalThis.backs += 1; globalThis.backValue = value; }); bubbleHost.on('ping', (data) => { globalThis.seen = data.n })",
    );
    assert_eq!(bubble_has_back_handler(shell(),), 1);
    let name = c("back");
    let payload = c("null");
    ffi_ok(bubble_send(shell(), name.as_ptr(), payload.as_ptr()));
    let ping = c("ping");
    let ping_body = c(r#"{"n":7}"#);
    ffi_ok(bubble_send(shell(), ping.as_ptr(), ping_body.as_ptr()));
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline
        && (eval("String(globalThis.backs)") != "1" || eval("String(globalThis.seen)") != "7")
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(eval("String(globalThis.backs)"), "1");
    assert_eq!(eval("String(globalThis.backValue)"), "null");
    assert_eq!(eval("String(globalThis.seen)"), "7");
    eval("bubbleHost.offBack()");
    assert_eq!(bubble_has_back_handler(shell(),), 0);
    let other = c(r#"{"n":1}"#);
    let later = c(r#"{"n":8}"#);
    ffi_ok(bubble_send(shell(), name.as_ptr(), other.as_ptr()));
    ffi_ok(bubble_send(shell(), ping.as_ptr(), later.as_ptr()));
    let after = Instant::now() + Duration::from_secs(1);
    while Instant::now() < after && eval("String(globalThis.seen)") != "8" {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(eval("String(globalThis.seen)"), "8");
    assert_eq!(eval("String(globalThis.backs)"), "1");
    eval("bubbleHost.onBack(() => {})");
    assert_eq!(bubble_has_back_handler(shell(),), 1);
    try_load_source("export const ready = true;\nexport default function App() {}\n").unwrap();
    assert_eq!(bubble_has_back_handler(shell(),), 0);
    close();
}

#[test]
fn storage_reloads_from_the_host_directory() {
    let _guard = Open;
    let dir = std::env::temp_dir().join(format!("bubble-storage-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = c(dir.to_str().unwrap());
    reopen();
    ffi_ok(bubble_set_storage_dir(shell(), path.as_ptr()));
    try_load_source("export const ready = true;\nexport default function App() {}\n").unwrap();
    assert_eq!(
        eval("bubbleStorage.set('k','v'); bubbleStorage.get('k')"),
        "v"
    );
    reopen();
    ffi_ok(bubble_set_storage_dir(shell(), path.as_ptr()));
    try_load_source("export const ready = true;\nexport default function App() {}\n").unwrap();
    assert_eq!(eval("bubbleStorage.get('k')"), "v");
    close();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Paint for this scenario contains only `Fill` commands: one background plus,
/// when the takeover declaration is disabled, the four focus-ring strips.
fn frame_fills(width: f32, height: f32) -> Vec<[u8; 4]> {
    let mut paint = ptr::null();
    let mut len = 0u32;
    assert_eq!(
        bubble_frame(shell(), width, height, &mut paint, &mut len),
        0,
        "{}",
        error()
    );
    let bytes = unsafe { std::slice::from_raw_parts(paint, len as usize) };
    let count = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
    assert_eq!(
        bytes.len(),
        4 + count * 37,
        "every command must be one 37-byte Fill"
    );
    let channel = |offset: usize| {
        let value = f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        (value.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    let mut out = Vec::new();
    for index in 0..count {
        let base = 4 + index * 37;
        assert_eq!(bytes[base], 1, "unexpected paint command");
        out.push([
            channel(base + 21),
            channel(base + 25),
            channel(base + 29),
            channel(base + 33),
        ]);
    }
    out
}

const STYLE_DECL_APP: &str = r##"
import * as shell from "bubble-shell";

export default function App() {
  const open = shell.signal(false);
  const items = shell.signal(["a", "b"]);
  const ladder = shell.signal([]);
  const gate = shell.signal(false);
  globalThis.sigs = { open, items, ladder, gate };

  const viewRef = shell.ref();
  const focused = shell.createSharedValue({ source: "focused", target: viewRef });
  const view = shell.primitive(
    "View",
    { ref: viewRef, role: "button", backgroundColor: "#ffffff", width: 200, height: 100 },
    () => {
      shell.show(() => open(), null, () => {
        shell.primitive("ViewStyle", {
          target: viewRef,
          when: focused,
          backgroundColor: "#ff0000",
        });
      });
      shell.forEach(() => items(), (item) => item, (read) => {
        shell.primitive("ViewStyle", {
          target: viewRef,
          when: true,
          backgroundColor: read() === "a" ? "#0000ff" : "#ffff00",
        });
      });
      shell.indexEach(() => ladder(), (read, index) => {
        shell.primitive("ViewStyle", {
          target: viewRef,
          when: true,
          backgroundColor: index === 0 ? "#111111" : "#222222",
        });
      });
      shell.switchFlow(null, [
        [
          () => gate(),
          () => {
            shell.primitive("ViewStyle", {
              target: viewRef,
              when: true,
              backgroundColor: "#333333",
            });
            // The focused declaration patches nothing: it takes the default
            // focus ring over while it matches, and closing the arm gives the
            // ring back without blurring.
            shell.primitive("TextStyle", { target: viewRef, when: focused });
          },
        ],
      ]);
    },
  );
  globalThis.appView = viewRef.current;
}

globalThis.appFocus = () => {
  shell.focusField(globalThis.appView);
  shell.flush();
};

globalThis.appBlur = () => {
  shell.focusField(0);
  shell.flush();
};

globalThis.appStep = (fn) => {
  shell.batch(fn);
  shell.flush();
};
"##;

#[test]
fn style_decl_scopes_keep_authored_order_and_cleanup() {
    const BLUE: [u8; 4] = [0x00, 0x00, 0xff, 0xff];
    const YELLOW: [u8; 4] = [0xff, 0xff, 0x00, 0xff];
    const RING: [u8; 4] = [77, 163, 255, 242];
    let _guard = Open;
    reopen();
    try_load_source(STYLE_DECL_APP).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(frame_fills(200.0, 100.0).len(), 1);
    eval("appFocus()");

    // Later declarations win: the For list's last item paints over the Show
    // branch's declaration, so an empty branch cannot append itself to the
    // end when it rebuilds.
    eval("appStep(() => sigs.open.set(true))");
    assert_eq!(frame_fills(200.0, 100.0), vec![YELLOW]);

    // Reordering the list reorders its declarations.
    eval("appStep(() => sigs.items.set(['b', 'a']))");
    assert_eq!(frame_fills(200.0, 100.0), vec![BLUE]);
    eval("appStep(() => sigs.open.set(false))");
    eval("appStep(() => sigs.items.set([]))");
    // Every list group is disposed; nothing patches the background now, and
    // the focused declaration went with the closed arm, so the default ring
    // returns while the node stays focused.
    assert_eq!(
        frame_fills(200.0, 100.0),
        vec![[0xff, 0xff, 0xff, 0xff], RING, RING, RING, RING]
    );

    // Index keeps one scope per position and Switch keeps one per arm, so a
    // later declaration still wins over both.
    eval("appStep(() => sigs.ladder.set([0, 0]))");
    assert_eq!(
        frame_fills(200.0, 100.0),
        vec![[0x22, 0x22, 0x22, 0xff], RING, RING, RING, RING]
    );
    eval("appStep(() => sigs.gate.set(true))");
    assert_eq!(frame_fills(200.0, 100.0), vec![[0x33, 0x33, 0x33, 0xff]]);
    eval("appStep(() => sigs.ladder.set([0]))");
    assert_eq!(frame_fills(200.0, 100.0), vec![[0x33, 0x33, 0x33, 0xff]]);
    // Closing the arm drops its declarations: the focused declaration goes
    // with it, so the default ring returns while the node stays focused.
    eval("appStep(() => sigs.gate.set(false))");
    assert_eq!(
        frame_fills(200.0, 100.0),
        vec![[0x11, 0x11, 0x11, 0xff], RING, RING, RING, RING]
    );
    eval("appStep(() => sigs.ladder.set([]))");
    assert_eq!(
        frame_fills(200.0, 100.0),
        vec![[0xff, 0xff, 0xff, 0xff], RING, RING, RING, RING]
    );

    // Deleting a list group drops its declaration; the surviving group still
    // paints at its authored slot and the ring stays on.
    eval("appStep(() => sigs.items.set(['a', 'b']))");
    assert_eq!(
        frame_fills(200.0, 100.0),
        vec![YELLOW, RING, RING, RING, RING]
    );
    eval("appStep(() => sigs.items.set(['a']))");
    assert_eq!(
        frame_fills(200.0, 100.0),
        vec![BLUE, RING, RING, RING, RING]
    );
    // Reopening the arm rebuilds at its authored slot: its declaration paints
    // over the list again and the focused declaration takes the ring over.
    eval("appStep(() => sigs.gate.set(true))");
    assert_eq!(frame_fills(200.0, 100.0), vec![[0x33, 0x33, 0x33, 0xff]]);
    eval("appStep(() => sigs.items.set(['a', 'b']))");
    assert_eq!(frame_fills(200.0, 100.0), vec![[0x33, 0x33, 0x33, 0xff]]);
    eval("appStep(() => sigs.gate.set(false))");
    assert_eq!(
        frame_fills(200.0, 100.0),
        vec![YELLOW, RING, RING, RING, RING]
    );
    assert_eq!(bubble_faults(shell(),), 0, "{}", error());
    close();
}

const SCRIPT_VAR_APP: &str = r##"
import * as shell from "bubble-shell";

globalThis.settled = 0;

const viewRef = shell.ref();
export default function App() {
  // Signals live inside the mount: boot resets the reactive graph.
  const count = shell.signal(0);
  const on = shell.signal(false);
  globalThis.sigs = { count, on };
  const t = shell.createSharedValue({ value: false });
  globalThis.t = t;
  // A signal can drive the variable: each render effect rerun encodes one
  // SetVar op that lands on the UI thread at the frame boundary.
  t.bind(() => count() !== 0);
  t.settle().then(() => { globalThis.settled += 1; });
  shell.primitive(
    "View",
    { ref: viewRef, backgroundColor: "#ffffff", width: 200, height: 100 },
    () => {
      shell.primitive("ViewStyle", {
        target: viewRef,
        when: t,
        backgroundColor: "#00ff00",
      });
    },
  );
}

globalThis.appStep = (fn) => {
  shell.batch(fn);
  shell.flush();
};
"##;

#[test]
fn script_vars_set_bind_and_settle() {
    let _guard = Open;
    reopen();
    try_load_source(SCRIPT_VAR_APP).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(frame_fills(200.0, 100.0), vec![[0xff, 0xff, 0xff, 0xff]]);

    // A direct write drives the declaration and settles.
    eval("appStep(() => t.set(true))");
    assert_eq!(frame_fills(200.0, 100.0), vec![[0x00, 0xff, 0x00, 0xff]]);
    eval("appStep(() => { t.set(false); t.settle().then(() => { globalThis.settled += 1; }); })");
    assert_eq!(frame_fills(200.0, 100.0), vec![[0xff, 0xff, 0xff, 0xff]]);
    // The mount's initial request and this one both report.
    wait_for("settled", 2, "every settle request reports once");

    // A later request reports on its own.
    eval("appStep(() => { t.set(true); t.settle().then(() => { globalThis.settled += 1; }); })");
    assert_eq!(frame_fills(200.0, 100.0), vec![[0x00, 0xff, 0x00, 0xff]]);
    wait_for("settled", 3, "settle requests report one for one");

    // Writing through bind follows the signal on the frame boundary.
    eval("appStep(() => { sigs.count.set(1); sigs.on.set(true); })");
    assert_eq!(frame_fills(200.0, 100.0), vec![[0x00, 0xff, 0x00, 0xff]]);
    eval("appStep(() => sigs.count.set(0))");
    assert_eq!(frame_fills(200.0, 100.0), vec![[0xff, 0xff, 0xff, 0xff]]);
    assert_eq!(bubble_faults(shell(),), 0, "{}", error());
    close();
}

const SCRIPT_ANIM_APP: &str = r##"
import * as shell from "bubble-shell";

globalThis.settled = 0;

const viewRef = shell.ref();
export default function App() {
  const t = shell.createSharedValue(0);
  globalThis.t = t;
  globalThis.withTiming = shell.withTiming;
  t.settle().then(() => { globalThis.settled += 1; });
  shell.primitive(
    "View",
    { ref: viewRef, backgroundColor: "#ffffff", width: 200, height: 100 },
    () => {
      shell.primitive("ViewStyle", {
        target: viewRef,
        when: true,
        // The patch value follows the animated variable for free.
        backgroundColor: ["mix", 0xff0000ff, 0xff00ff00, t],
      });
    },
  );
}

globalThis.appStep = (fn) => {
  shell.batch(fn);
  shell.flush();
};
"##;

#[test]
fn animated_var_drives_paint_and_settles_at_convergence() {
    let _guard = Open;
    reopen();
    try_load_source(SCRIPT_ANIM_APP).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(frame_fills(200.0, 100.0), vec![[0x00, 0x00, 0xff, 0xff]]);

    // The write starts the tween; the first paint still shows the old value.
    eval("appStep(() => { t.value = withTiming(1, { duration: 100, easing: \"linear\" }); })");
    assert_eq!(frame_fills(200.0, 100.0), vec![[0x00, 0x00, 0xff, 0xff]]);

    // Mid-tween the patch interpolates; the settle event waits for the end.
    bubble_tick(shell(), 50.0);
    assert_eq!(frame_fills(200.0, 100.0), vec![[0x00, 0x80, 0x80, 0xff]]);

    // Convergence snaps onto the target and reports once.
    bubble_tick(shell(), 160.0);
    assert_eq!(frame_fills(200.0, 100.0), vec![[0x00, 0xff, 0x00, 0xff]]);
    wait_for("settled", 1, "settle event after the tween converged");
    assert_eq!(bubble_faults(shell(),), 0, "{}", error());
    close();
}

const GESTURE_ORDER_APP: &str = r##"
import * as shell from "bubble-shell";

export default function App() {
  const open = shell.signal(false);
  globalThis.branchOpen = open;
  globalThis.hits = [];
  shell.primitive(
    "View",
    { backgroundColor: "#ffffff", width: 100, height: 100, role: "button" },
    () => {
      shell.show(() => open(), null, () => {
        shell.primitive("TapGesture", {
          onTap: () => globalThis.hits.push("branch"),
        });
      });
      shell.primitive("TapGesture", {
        onTap: () => globalThis.hits.push("static"),
      });
    },
  );
  globalThis.step = (fn) => {
    shell.batch(fn);
    shell.flush();
  };
}
"##;

/// Dispatch one tap and wait until a gesture handler has run `expected` times.
fn tap_and_wait(x: f32, y: f32, expected: usize) -> String {
    assert_eq!(
        bubble_pointer(shell(), 0, 1, x, y, 0.0, 0, 1),
        0,
        "{}",
        error()
    );
    assert_eq!(
        bubble_pointer(shell(), 2, 1, x, y, 20.0, 0, 0),
        0,
        "{}",
        error()
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let hits = eval("globalThis.hits.join(',')");
        if hits.split(',').filter(|hit| !hit.is_empty()).count() >= expected {
            return hits;
        }
        assert!(Instant::now() < deadline, "tap handler did not run: {hits}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn gesture_scopes_keep_authored_order_across_branch_rebuild() {
    let _guard = Open;
    reopen();
    try_load_source(GESTURE_ORDER_APP).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(frame_fills(100.0, 100.0).len(), 1);

    // Two tap declarations share the view. The later declaration wins, and a
    // rebuilt Show branch must return to its authored slot, not append itself.
    assert_eq!(tap_and_wait(50.0, 50.0, 1), "static");
    eval("step(() => branchOpen.set(true))");
    assert_eq!(tap_and_wait(50.0, 50.0, 2), "static,static");
    eval("step(() => branchOpen.set(false))");
    assert_eq!(tap_and_wait(50.0, 50.0, 3), "static,static,static");
    close();
}

/// Parse one paint frame and return the colors of every `Fill` command,
/// tolerating clips, opacity, and transforms around them.
fn frame_fill_colors(width: f32, height: f32) -> Vec<[u8; 4]> {
    let mut paint = ptr::null();
    let mut len = 0u32;
    assert_eq!(
        bubble_frame(shell(), width, height, &mut paint, &mut len),
        0,
        "{}",
        error()
    );
    let bytes = unsafe { std::slice::from_raw_parts(paint, len as usize) };
    let count = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
    let channel = |offset: usize| {
        let value = f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        (value.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    let mut offset = 4;
    let mut out = Vec::new();
    for _ in 0..count {
        match bytes[offset] {
            1 => {
                out.push([
                    channel(offset + 21),
                    channel(offset + 25),
                    channel(offset + 29),
                    channel(offset + 33),
                ]);
                offset += 37;
            }
            2 => {
                let text_len =
                    u32::from_le_bytes(bytes[offset + 43..offset + 47].try_into().unwrap())
                        as usize;
                offset += 47 + text_len;
            }
            3 => offset += 21,
            4 | 6 | 10 => offset += 1,
            5 => offset += 5,
            7 => {
                let src_len =
                    u32::from_le_bytes(bytes[offset + 25..offset + 29].try_into().unwrap())
                        as usize;
                offset += 29 + src_len;
            }
            9 => offset += 25,
            other => panic!("unexpected paint command {other}"),
        }
    }
    assert_eq!(offset, bytes.len(), "paint frame must decode exactly");
    out
}

/// Parse one paint frame and return every `PushTransform` payload
/// `(cx, cy, translateX, translateY, scale, rotate)` in order.
fn frame_transforms(width: f32, height: f32) -> Vec<[f32; 6]> {
    let mut paint = ptr::null();
    let mut len = 0u32;
    assert_eq!(
        bubble_frame(shell(), width, height, &mut paint, &mut len),
        0,
        "{}",
        error()
    );
    let bytes = unsafe { std::slice::from_raw_parts(paint, len as usize) };
    let count = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
    let f32_at = |offset: usize| f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    let mut offset = 4;
    let mut out = Vec::new();
    for _ in 0..count {
        match bytes[offset] {
            1 => offset += 37,
            2 => {
                let text_len =
                    u32::from_le_bytes(bytes[offset + 43..offset + 47].try_into().unwrap())
                        as usize;
                offset += 47 + text_len;
            }
            3 => offset += 21,
            4 | 6 | 10 => offset += 1,
            5 => offset += 5,
            7 => {
                let src_len =
                    u32::from_le_bytes(bytes[offset + 25..offset + 29].try_into().unwrap())
                        as usize;
                offset += 29 + src_len;
            }
            9 => {
                out.push([
                    f32_at(offset + 1),
                    f32_at(offset + 5),
                    f32_at(offset + 9),
                    f32_at(offset + 13),
                    f32_at(offset + 17),
                    f32_at(offset + 21),
                ]);
                offset += 25;
            }
            other => panic!("unexpected paint command {other}"),
        }
    }
    assert_eq!(offset, bytes.len(), "paint frame must decode exactly");
    out
}

const SCRIPT_PAN_APP: &str = r##"
import * as shell from "bubble-shell";

const viewRef = shell.ref();
const panRef = shell.ref();
export default function App() {
  const tx = shell.createSharedValue({ source: "gesture.translationX", target: panRef });
  shell.primitive(
    "View",
    { ref: viewRef, backgroundColor: "#ffffff", width: 100, height: 100 },
    () => {
      shell.primitive("PanGesture", { ref: panRef, onPanUpdate: () => {} });
      shell.primitive("ViewStyle", { target: viewRef, when: true, translateX: tx });
    },
  );
}
"##;

#[test]
fn pan_var_drives_transform_follow() {
    let _guard = Open;
    reopen();
    try_load_source(SCRIPT_PAN_APP).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(frame_fills(200.0, 100.0), vec![[0xff, 0xff, 0xff, 0xff]]);

    // Idle: the unit transform is skipped entirely.
    assert_eq!(frame_transforms(200.0, 100.0), Vec::<[f32; 6]>::new());

    // Drag right: the pan var tracks the live translation.
    assert_eq!(
        bubble_pointer(shell(), 0, 1, 50.0, 50.0, 0.0, 0, 1),
        0,
        "{}",
        error()
    );
    assert_eq!(
        bubble_pointer(shell(), 1, 1, 80.0, 50.0, 16.0, 0, 1),
        0,
        "{}",
        error()
    );
    let transforms = frame_transforms(200.0, 100.0);
    assert_eq!(transforms.len(), 1, "one transform around the node center");
    let [cx, cy, tx, ty, scale, rotate] = transforms[0];
    assert_eq!((cx, cy), (50.0, 50.0));
    assert_eq!((tx, ty), (30.0, 0.0));
    assert_eq!((scale, rotate), (1.0, 0.0));

    // Lift: the sequence ends and the node returns to rest.
    assert_eq!(
        bubble_pointer(shell(), 2, 1, 80.0, 50.0, 32.0, 0, 1),
        0,
        "{}",
        error()
    );
    assert_eq!(frame_transforms(200.0, 100.0), Vec::<[f32; 6]>::new());
    assert_eq!(bubble_faults(shell(),), 0, "{}", error());
    close();
}

const SCRIPT_SCROLL_APP: &str = r##"
import * as shell from "bubble-shell";

const containerRef = shell.ref();
const childRef = shell.ref();
export default function App() {
  shell.primitive(
    "View",
    { ref: containerRef, overflowY: "scroll", width: 100, height: 100 },
    () => {
      const sy = shell.createSharedValue({ source: "scroll.y", target: containerRef });
      shell.primitive("View", {
        ref: childRef,
        width: 100,
        height: 200,
        backgroundColor: "#ffffff",
      }, () => {
        shell.primitive("ViewStyle", {
          target: childRef,
          when: true,
          backgroundColor: ["mix", 0xffff0000, 0xff00ff00, ["div", sy, 200]],
        });
      });
    },
  );
}
"##;

#[test]
fn scroll_var_tracks_offsets() {
    let _guard = Open;
    reopen();
    try_load_source(SCRIPT_SCROLL_APP).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        frame_fill_colors(200.0, 200.0),
        vec![[0xff, 0x00, 0x00, 0xff]]
    );

    // Scrolling the container drives the color patch: half the content
    // height means the mix lands midway between red and green.
    bubble_scroll(shell(), 50.0, 50.0, 0.0, 100.0);
    assert_eq!(
        frame_fill_colors(200.0, 200.0),
        vec![[0x80, 0x80, 0x00, 0xff]]
    );
    assert_eq!(bubble_faults(shell(),), 0, "{}", error());
    close();
}

const SCRIPT_PINCH_APP: &str = r##"
import * as shell from "bubble-shell";

const viewRef = shell.ref();
const pinchRef = shell.ref();
export default function App() {
  const scale = shell.createSharedValue({ source: "gesture.scale", target: pinchRef });
  shell.primitive(
    "View",
    { ref: viewRef, backgroundColor: "#ffffff", width: 100, height: 100 },
    () => {
      shell.primitive("TransformGesture", {
        ref: pinchRef,
        mode: ["pinch"],
        onTransformUpdate: () => {},
      });
      shell.primitive("ViewStyle", { target: viewRef, when: true, scale });
    },
  );
}
"##;

#[test]
fn pinch_var_drives_scale_transform() {
    let _guard = Open;
    reopen();
    try_load_source(SCRIPT_PINCH_APP).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        frame_fill_colors(200.0, 200.0),
        vec![[0xff, 0xff, 0xff, 0xff]]
    );
    assert_eq!(frame_transforms(200.0, 200.0), Vec::<[f32; 6]>::new());

    // Two fingers spread from 10 to 20 apart: scale 2 around the center.
    assert_eq!(
        bubble_pointer(shell(), 0, 1, 50.0, 50.0, 0.0, 0, 2),
        0,
        "{}",
        error()
    );
    assert_eq!(
        bubble_pointer(shell(), 0, 2, 60.0, 50.0, 0.0, 0, 2),
        0,
        "{}",
        error()
    );
    assert_eq!(
        bubble_pointer(shell(), 1, 1, 45.0, 50.0, 16.0, 0, 2),
        0,
        "{}",
        error()
    );
    assert_eq!(
        bubble_pointer(shell(), 1, 1, 40.0, 50.0, 32.0, 0, 2),
        0,
        "{}",
        error()
    );
    let transforms = frame_transforms(200.0, 200.0);
    assert_eq!(transforms.len(), 1, "one transform while pinching");
    let [cx, cy, tx, ty, scale, rotate] = transforms[0];
    assert_eq!((cx, cy), (50.0, 50.0));
    assert_eq!((tx, ty), (0.0, 0.0));
    assert_eq!(scale, 2.0, "distance doubled from 10 to 20");
    assert_eq!(rotate, 0.0);
    assert_eq!(bubble_faults(shell(),), 0, "{}", error());
    close();
}
