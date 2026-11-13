use std::ffi::{CStr, CString};
use std::ptr;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use bubble_ffi::{
    bubble_bind, bubble_create, bubble_current, bubble_destroy, bubble_edit, bubble_edit_batch_begin, bubble_edit_batch_end, bubble_send, bubble_set_params, bubble_set_storage_dir,
    bubble_edit_commit, bubble_edit_compose, bubble_edit_delete_surrounding,
    bubble_edit_finish_composition, bubble_edit_replace, bubble_edit_snapshot, bubble_eval,
    bubble_faults, bubble_finish_request, bubble_font_count, bubble_font_epoch, bubble_frame,
    bubble_generation, bubble_last_error, bubble_load, bubble_node_box, bubble_node_text,
    bubble_pointer, bubble_poll_request, bubble_post_eval, bubble_register_font, bubble_scroll,
    bubble_scroll_y_of, bubble_set_visible, bubble_status,
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

fn load_fixture(name: &str) {
    let (manifest, entry): (&str, &[u8]) = match name {
        "counter" => (
            include_str!("../../../examples/web/fixtures/counter.json"),
            include_bytes!("../../../examples/web/fixtures/counter.js"),
        ),
        "editing" => (
            include_str!("../../../examples/web/fixtures/editing.json"),
            include_bytes!("../../../examples/web/fixtures/editing.js"),
        ),
        other => panic!("unknown fixture {other}"),
    };
    let manifest = c(manifest);
    let code = bubble_load(manifest.as_ptr(), entry.as_ptr(), entry.len() as u32);
    assert_eq!(code, 0, "{}", error());
}

fn load_counter() {
    load_fixture("counter");
}

fn load_editing() {
    load_fixture("editing");
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
    let (tx, ty) = (x + w / 2.0, y + h / 2.0);
    assert_eq!(bubble_pointer(0, 1, tx, ty, 0.0, 0, 1), 0, "{}", error());
    assert_eq!(bubble_pointer(2, 1, tx, ty, 20.0, 0, 0), 0, "{}", error());
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
    let shell = bubble_current();
    load_counter();
    let frames = std::sync::atomic::AtomicU32::new(0);
    let stop = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            assert_eq!(bubble_bind(shell), 0);
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
fn two_shells_stay_independent_and_a_bad_manifest_is_rejected() {
    let _guard = exclusive();
    bubble_destroy();
    assert_eq!(bubble_create(ptr::null()), 0, "{}", error());
    let first = bubble_current();
    assert!(first > 0);
    assert_eq!(bubble_create(ptr::null()), 0, "{}", error());
    let second = bubble_current();
    assert_ne!(first, second);
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
    assert_eq!(bubble_bind(first), 0);
    assert_eq!(bubble_current(), first);
    bubble_destroy();
    assert_eq!(bubble_bind(second), 0);
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

#[test]
fn edit_batch_writes_through_the_focused_session() {
    let _guard = exclusive();
    bubble_destroy();
    assert_eq!(bubble_create(ptr::null()), 0, "{}", error());
    load_editing();
    let mut paint = ptr::null();
    let mut len = 0u32;
    assert_eq!(
        bubble_frame(320.0, 240.0, &mut paint, &mut len),
        0,
        "{}",
        error()
    );
    assert_eq!(bubble_edit(0, ptr::null(), 0, 4, 0), 0, "{}", error());
    let (mut session, mut revision) = (0u64, 0u64);
    assert_eq!(
        bubble_edit_snapshot(
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
    assert_eq!(bubble_edit_batch_begin(session), 0, "{}", error());
    let a = CString::new("a").unwrap();
    let b = CString::new("b").unwrap();
    assert_eq!(
        bubble_edit(1, a.as_ptr() as *const u8, 1, 0, 0),
        0,
        "{}",
        error()
    );
    assert_eq!(
        bubble_edit(1, b.as_ptr() as *const u8, 1, 0, 0),
        0,
        "{}",
        error()
    );
    assert_eq!(text(4), "ab");
    let generation = bubble_generation();
    assert_eq!(bubble_edit_batch_end(session), 0, "{}", error());
    assert_eq!(text(4), "ab");
    let deadline = Instant::now() + Duration::from_secs(2);
    while bubble_generation() == generation && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_ne!(bubble_generation(), generation, "{}", status());
    assert_ne!(bubble_edit_batch_end(session), 0);
}

fn edit_snapshot() -> (u64, u64, u32, u32, u32, u32, u32) {
    let (mut session, mut revision) = (0u64, 0u64);
    let (mut anchor, mut active) = (0u32, 0u32);
    let (mut comp_start, mut comp_end, mut composing) = (0u32, 0u32, 0u32);
    assert_eq!(
        bubble_edit_snapshot(
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
    let _guard = exclusive();
    bubble_destroy();
    assert_eq!(bubble_create(ptr::null()), 0, "{}", error());
    load_editing();
    let mut paint = ptr::null();
    let mut len = 0u32;
    assert_eq!(
        bubble_frame(320.0, 240.0, &mut paint, &mut len),
        0,
        "{}",
        error()
    );
    assert_eq!(bubble_edit(0, ptr::null(), 0, 4, 0), 0, "{}", error());
    let (session, _, _, _, _, _, _) = edit_snapshot();
    assert_ne!(session, 0);

    let ab = CString::new("ab").unwrap();
    assert_eq!(
        bubble_edit_replace(session, 0, 0, ab.as_ptr() as *const u8, 2, 1, 0,),
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
        bubble_edit_replace(session, 2, 2, mark.as_ptr() as *const u8, 1, 1, 1,),
        0,
        "{}",
        error()
    );
    let (_, _, anchor, active, comp_start, comp_end, composing) = edit_snapshot();
    assert_eq!((anchor, active), (3, 3));
    assert_eq!((comp_start, comp_end, composing), (2, 3, 1));
    assert_eq!(text(4), "abX");

    assert_eq!(bubble_edit_finish_composition(session), 0, "{}", error());
    let (_, _, _, _, _, comp_end, composing) = edit_snapshot();
    assert_eq!(comp_end, u32::MAX);
    assert_eq!(composing, 0);

    assert_eq!(
        bubble_edit_delete_surrounding(session, 1, 0),
        0,
        "{}",
        error()
    );
    let (_, _, anchor, active, _, _, _) = edit_snapshot();
    assert_eq!((anchor, active), (2, 2));
    assert_eq!(text(4), "ab");

    let mark = CString::new("X").unwrap();
    assert_eq!(
        bubble_edit_compose(session, mark.as_ptr() as *const u8, 1, 1),
        0,
        "{}",
        error()
    );
    let (_, _, _, _, comp_start, comp_end, composing) = edit_snapshot();
    assert_eq!((comp_start, comp_end, composing), (2, 3, 1));

    let commit = CString::new("Y").unwrap();
    assert_eq!(
        bubble_edit_commit(session, commit.as_ptr() as *const u8, 1, 1),
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
        bubble_edit_delete_surrounding(session + 1, 1, 0),
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
    let result = bubble_load(manifest.as_ptr(), code.as_ptr(), code.len() as u32);
    if result == 0 {
        Ok(())
    } else {
        Err(error())
    }
}

#[test]
fn host_modules_resolve_to_one_runtime_instance() {
    let _guard = exclusive();
    bubble_destroy();
    assert_eq!(bubble_create(ptr::null()), 0, "{}", error());
    try_load_source(
        r#"import * as shell from "bubble-shell";
import * as rt from "bubble:rt";
if (shell !== rt) throw new Error("bubble-shell and bubble:rt must be one module");
const count = shell.signal(41);
shell.mount(function App() {
  shell.primitive("View", {}, () => {
    shell.primitive("Text", {}, () => "N=" + (count() + 1));
  });
});
"#,
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let mut paint = ptr::null();
    let mut len = 0u32;
    assert_eq!(
        bubble_frame(200.0, 100.0, &mut paint, &mut len),
        0,
        "{}",
        error()
    );
    assert_eq!(text(3), "N=42");
}

#[test]
fn relative_imports_are_rejected_by_the_loader() {
    let _guard = exclusive();
    bubble_destroy();
    assert_eq!(bubble_create(ptr::null()), 0, "{}", error());
    let error = try_load_source(
        "import { helper } from \"./helper.js\";\nhelper();\n",
    )
    .unwrap_err();
    assert!(error.contains("relative imports"), "{error}");
}

#[test]
fn storage_params_and_host_events_roundtrip() {
    let _guard = exclusive();
    bubble_destroy();
    assert_eq!(bubble_create(ptr::null()), 0, "{}", error());
    let params = c(r#"{"id":"42"}"#);
    assert_eq!(bubble_set_params(params.as_ptr()), 0, "{}", error());
    let early = c("ping");
    let body = c(r#"{"n":1}"#);
    assert_eq!(bubble_send(early.as_ptr(), body.as_ptr()), 0, "{}", error());
    load_counter();
    assert_eq!(eval("JSON.stringify(bubbleHost.params)"), r#"{"id":"42"}"#);
    assert_eq!(
        eval("bubbleStorage.set('k','v'); bubbleStorage.get('k')"),
        "v"
    );
    eval("bubbleHost.on('ping', (data) => { globalThis.seen = data.n })");
    assert_eq!(eval("String(globalThis.seen)"), "1");
    let later = c(r#"{"n":2}"#);
    assert_eq!(bubble_send(early.as_ptr(), later.as_ptr()), 0, "{}", error());
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline && eval("String(globalThis.seen)") != "2" {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(eval("String(globalThis.seen)"), "2");
    bubble_destroy();
}

#[test]
fn storage_reloads_from_the_host_directory() {
    let _guard = exclusive();
    let dir = std::env::temp_dir().join(format!("bubble-storage-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = c(dir.to_str().unwrap());
    bubble_destroy();
    assert_eq!(bubble_create(ptr::null()), 0, "{}", error());
    assert_eq!(bubble_set_storage_dir(path.as_ptr()), 0, "{}", error());
    try_load_source("export const ready = true;\n").unwrap();
    assert_eq!(eval("bubbleStorage.set('k','v'); bubbleStorage.get('k')"), "v");
    bubble_destroy();
    assert_eq!(bubble_create(ptr::null()), 0, "{}", error());
    assert_eq!(bubble_set_storage_dir(path.as_ptr()), 0, "{}", error());
    try_load_source("export const ready = true;\n").unwrap();
    assert_eq!(eval("bubbleStorage.get('k')"), "v");
    bubble_destroy();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Paint for this scenario contains only `Fill` commands: one background plus,
/// when the takeover declaration is disabled, the four focus-ring strips.
fn frame_fills(width: f32, height: f32) -> Vec<[u8; 4]> {
    let mut paint = ptr::null();
    let mut len = 0u32;
    assert_eq!(
        bubble_frame(width, height, &mut paint, &mut len),
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

const STATE_STYLE_APP: &str = r##"
import * as shell from "bubble-shell";

shell.mount(function App() {
  const open = shell.signal(false);
  const green = shell.signal(true);
  const items = shell.signal(["a", "b"]);
  const blue = shell.signal(true);
  const tone = shell.signal("#123456");
  const when = shell.signal("focused");
  const marker = shell.signal(true);
  const ladder = shell.signal([]);
  const gate = shell.signal(false);
  globalThis.sigs = { open, green, items, blue, tone, when, marker, ladder, gate };

  const view = shell.primitive(
    "View",
    { backgroundColor: "#ffffff", role: "button", width: 200, height: 100 },
    () => {
      shell.show(() => open(), null, () => {
        shell.primitive("StateStyle", { when: "focused", backgroundColor: "#ff0000" });
      });
      shell.primitive("StateStyle", {
        when: "focused",
        enabled: shell.dynamic(() => green()),
        backgroundColor: "#00ff00",
      });
      // The color is chosen when the group is created, so a reorder only moves
      // the declarations and the authored order decides the winner.
      shell.forEach(() => items(), (item) => item, (read) => {
        shell.primitive("StateStyle", {
          when: "focused",
          backgroundColor: read() === "a" ? "#0000ff" : "#ffff00",
        });
      });
      shell.primitive("StateStyle", {
        when: shell.dynamic(() => when()),
        enabled: shell.dynamic(() => blue()),
        backgroundColor: shell.dynamic(() => tone()),
      });
      shell.indexEach(() => ladder(), (read, index) => {
        shell.primitive("StateStyle", {
          when: "focused",
          backgroundColor: index === 0 ? "#111111" : "#222222",
        });
      });
      shell.switchFlow(null, [
        [
          () => gate(),
          () => {
            shell.primitive("StateStyle", {
              when: "focused",
              backgroundColor: "#333333",
            });
          },
        ],
      ]);
      shell.primitive("StateStyle", {
        when: "focused",
        enabled: shell.dynamic(() => marker()),
      });
    },
  );
  globalThis.appView = view;
});

globalThis.appFocus = () => {
  shell.focusField(globalThis.appView);
  shell.flush();
};

globalThis.appStep = (fn) => {
  shell.batch(fn);
  shell.flush();
};
"##;

#[test]
fn state_style_scopes_keep_authored_order_and_cleanup_bindings() {
    const RED: [u8; 4] = [0xff, 0x00, 0x00, 0xff];
    const GREEN: [u8; 4] = [0x00, 0xff, 0x00, 0xff];
    const BLUE: [u8; 4] = [0x00, 0x00, 0xff, 0xff];
    const YELLOW: [u8; 4] = [0xff, 0xff, 0x00, 0xff];
    const TONE: [u8; 4] = [0x12, 0x34, 0x56, 0xff];
    const RING: [u8; 4] = [77, 163, 255, 242];

    let _guard = exclusive();
    bubble_destroy();
    assert_eq!(bubble_create(ptr::null()), 0, "{}", error());
    try_load_source(STATE_STYLE_APP).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(frame_fills(200.0, 100.0).len(), 1);
    eval("appFocus()");
    assert_eq!(frame_fills(200.0, 100.0), vec![TONE]);

    // The Show opens in front of the static declaration: the later one still
    // wins, so the empty branch cannot append itself to the end.
    eval("appStep(() => sigs.open.set(true))");
    assert_eq!(frame_fills(200.0, 100.0), vec![TONE]);

    // With the trailing declaration disabled the list's last group wins, not
    // the Show's declaration.
    eval("appStep(() => sigs.blue.set(false))");
    assert_eq!(frame_fills(200.0, 100.0), vec![YELLOW]);
    eval("appStep(() => sigs.green.set(false))");
    assert_eq!(frame_fills(200.0, 100.0), vec![YELLOW]);

    // Reordering the list reorders its declarations.
    eval("appStep(() => sigs.items.set(['b', 'a']))");
    assert_eq!(frame_fills(200.0, 100.0), vec![BLUE]);
    eval("appStep(() => sigs.items.set([]))");
    assert_eq!(frame_fills(200.0, 100.0), vec![RED]);
    eval("appStep(() => sigs.green.set(true))");
    assert_eq!(frame_fills(200.0, 100.0), vec![GREEN]);

    // Dynamic conditions and values apply on the next commit.
    eval("appStep(() => sigs.when.set('pressed'))");
    assert_eq!(frame_fills(200.0, 100.0), vec![GREEN]);
    eval("appStep(() => { sigs.when.set('focused'); sigs.blue.set(true); })");
    assert_eq!(frame_fills(200.0, 100.0), vec![TONE]);
    eval("appStep(() => sigs.tone.set(null))");
    assert_eq!(frame_fills(200.0, 100.0), vec![GREEN]);
    eval("appStep(() => sigs.tone.set('#abcdef'))");
    assert_eq!(frame_fills(200.0, 100.0), vec![[0xab, 0xcd, 0xef, 0xff]]);

    // Index keeps one scope per position and Switch keeps one per arm, so a
    // later declaration still wins over both.
    eval("appStep(() => { sigs.ladder.set([0, 0]); sigs.blue.set(false); })");
    assert_eq!(frame_fills(200.0, 100.0), vec![[0x22, 0x22, 0x22, 0xff]]);
    eval("appStep(() => sigs.gate.set(true))");
    assert_eq!(frame_fills(200.0, 100.0), vec![[0x33, 0x33, 0x33, 0xff]]);
    eval("appStep(() => sigs.ladder.set([0]))");
    assert_eq!(frame_fills(200.0, 100.0), vec![[0x33, 0x33, 0x33, 0xff]]);
    eval("appStep(() => sigs.gate.set(false))");
    assert_eq!(frame_fills(200.0, 100.0), vec![[0x11, 0x11, 0x11, 0xff]]);
    // With the Show's branch left as the only painting declaration, it wins.
    eval("appStep(() => { sigs.ladder.set([]); sigs.green.set(false); })");
    assert_eq!(frame_fills(200.0, 100.0), vec![RED]);

    // The trailing empty focused declaration takes the ring over: while it
    // matches there is no ring, and without any matching declaration the ring
    // comes back.
    eval("appStep(() => { sigs.marker.set(false); sigs.blue.set(false); sigs.green.set(false); sigs.open.set(false); sigs.items.set([]); })");
    assert_eq!(
        frame_fills(200.0, 100.0),
        vec![
            [0xff, 0xff, 0xff, 0xff],
            RING,
            RING,
            RING,
            RING
        ]
    );
    eval("appStep(() => { sigs.marker.set(true); sigs.blue.set(true); sigs.green.set(true); })");
    assert_eq!(frame_fills(200.0, 100.0), vec![[0xab, 0xcd, 0xef, 0xff]]);

    // Deleting a list group drops its declaration and its bindings.
    eval("appStep(() => { sigs.open.set(false); sigs.green.set(false); sigs.blue.set(false); sigs.items.set(['a', 'b']); })");
    assert_eq!(frame_fills(200.0, 100.0), vec![YELLOW]);
    eval("appStep(() => sigs.items.set(['a']))");
    assert_eq!(frame_fills(200.0, 100.0), vec![BLUE]);
    eval("appStep(() => sigs.items.set(['a']))");
    assert_eq!(frame_fills(200.0, 100.0), vec![BLUE]);
    assert_eq!(bubble_faults(), 0, "{}", error());
}


