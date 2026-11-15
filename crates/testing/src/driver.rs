//! A [`Session`] drives one shell through the C ABI, exactly like an iOS or
//! Android host would: load a bundle, pump frames and gesture ticks from a
//! detached thread, then send pointer/edit input and read semantics.
//!
//! The session holds the shell pointer and passes it to every call. The pump
//! thread uses the same pointer for `bubble_tick` and `bubble_frame`.

use std::ffi::{CStr, CString};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use bubble_core::SemanticsSnapshot;

use crate::bundle::App;
use crate::semantics as sem;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(2);
const FAULT_TIMEOUT: Duration = Duration::from_secs(4);
const VIEWPORT: (f32, f32) = (390.0, 844.0);

fn c_string(text: &str) -> Result<CString, String> {
    CString::new(text).map_err(|_| "text contains a nul".into())
}

fn last_error(shell: *mut bubble_ffi::BubbleShell) -> String {
    unsafe {
        let ptr = bubble_ffi::bubble_last_error(shell);
        if ptr.is_null() {
            return String::new();
        }
        CStr::from_ptr(ptr).to_string_lossy().into_owned()
    }
}

/// The app a scenario steps, from bundle load to assertions.
pub struct Session {
    shell: usize,
    opened: Instant,
    stop: Arc<AtomicBool>,
    pump: Option<JoinHandle<()>>,
}

impl Session {
    fn ptr(&self) -> *mut bubble_ffi::BubbleShell {
        self.shell as *mut bubble_ffi::BubbleShell
    }

    /// Destroy any shell left by a previous test, create one, and load the
    /// fixture bundle. Starts the frame/gesture pump thread.
    pub fn start(app: &App) -> Result<Self, String> {
        let created = unsafe { bubble_ffi::bubble_create(ptr::null()) };
        if created.is_null() {
            return Err("bubble_create failed".into());
        }
        let mut session = Self {
            shell: created as usize,
            opened: Instant::now(),
            stop: Arc::new(AtomicBool::new(false)),
            pump: None,
        };
        session.load(app)?;
        let mut paint: *const u8 = ptr::null();
        let mut len = 0u32;
        unsafe {
            bubble_ffi::bubble_frame(session.ptr(), VIEWPORT.0, VIEWPORT.1, &mut paint, &mut len);
        }
        session.pump = Some(session.spawn_pump());
        Ok(session)
    }

    fn load(&mut self, app: &App) -> Result<(), String> {
        let manifest = c_string(&app.manifest)?;
        let err = unsafe {
            bubble_ffi::bubble_load(
                self.ptr(),
                manifest.as_ptr(),
                app.entry.as_ptr(),
                app.entry.len() as u32,
            )
        };
        if !err.is_null() {
            let message = unsafe { CStr::from_ptr(err) }
                .to_string_lossy()
                .into_owned();
            unsafe { bubble_ffi::bubble_string_free(err) };
            return Err(message);
        }
        Ok(())
    }

    fn spawn_pump(&self) -> JoinHandle<()> {
        let shell = self.shell;
        let stop = Arc::clone(&self.stop);
        let opened = self.opened;
        std::thread::spawn(move || {
            let shell = shell as *mut bubble_ffi::BubbleShell;
            while !stop.load(Ordering::Relaxed) {
                let now_ms = opened.elapsed().as_secs_f64() * 1000.0;
                unsafe { bubble_ffi::bubble_tick(shell, now_ms) };
                let mut paint: *const u8 = ptr::null();
                let mut len = 0u32;
                unsafe {
                    bubble_ffi::bubble_frame(shell, VIEWPORT.0, VIEWPORT.1, &mut paint, &mut len);
                }
                std::thread::sleep(Duration::from_millis(8));
            }
        })
    }

    fn now_ms(&self) -> f64 {
        self.opened.elapsed().as_secs_f64() * 1000.0
    }

    /// Low-level pointer sample: action 0 down, 1 move, 2 up, 3 cancel.
    /// Used directly by Rust-level gesture scenarios.
    pub fn pointer(&self, action: u32, x: f32, y: f32, time_ms: f64) {
        unsafe {
            bubble_ffi::bubble_pointer(
                self.ptr(),
                action,
                1,
                x,
                y,
                time_ms,
                0,
                u32::from(action != 2),
            );
        }
    }

    /// Tap the center of the node the selector resolves to.
    pub fn tap(&self, selector: &Selector) -> Result<(), String> {
        let (x, y) = self.center(selector)?;
        let now = self.now_ms();
        self.pointer(0, x, y, now);
        self.pointer(2, x, y, now + 20.0);
        Ok(())
    }

    /// Scroll by `dx`, `dy` from the center of the node.
    pub fn scroll(&self, selector: &Selector, dx: f32, dy: f32) -> Result<(), String> {
        let (x, y) = self.center(selector)?;
        unsafe { bubble_ffi::bubble_scroll(self.ptr(), x, y, dx, dy) };
        Ok(())
    }

    /// Focus the field and write `text` through the edit transaction path.
    pub fn type_text(&self, selector: &Selector, text: &str) -> Result<(), String> {
        let snapshot = self.semantics()?;
        let handle = self.resolve(&snapshot, selector)?;
        // Wire codes: 0 focus, 1 activate (focuses a field), 2 set-value.
        if unsafe { bubble_ffi::bubble_a11y_action(self.ptr(), handle, 1, ptr::null(), 0) } < 0 {
            return Err(last_error(self.ptr()));
        }
        let text = c_string(text)?;
        if unsafe {
            bubble_ffi::bubble_edit(
                self.ptr(),
                1,
                text.as_ptr() as *const u8,
                text.as_bytes().len() as u32,
                0,
                0,
            )
        } < 0
        {
            return Err(last_error(self.ptr()));
        }
        Ok(())
    }

    /// Current semantics snapshot.
    pub fn semantics(&self) -> Result<SemanticsSnapshot, String> {
        sem::snapshot(self.ptr())
    }

    /// Text content of the node, via the same accessor hosts use.
    pub fn text_of(&self, selector: &Selector) -> Result<String, String> {
        let handle = self.resolve(&self.semantics()?, selector)?;
        let mut buf = vec![0 as libc_char; 256];
        let code = unsafe {
            bubble_ffi::bubble_node_text(self.ptr(), handle, buf.as_mut_ptr(), buf.len() as u32)
        };
        if code != 0 {
            return Err(last_error(self.ptr()));
        }
        Ok(unsafe { CStr::from_ptr(buf.as_ptr()) }
            .to_string_lossy()
            .into_owned())
    }

    fn resolve(&self, snapshot: &SemanticsSnapshot, selector: &Selector) -> Result<u32, String> {
        match selector {
            Selector::Id(id) => sem::by_id(snapshot, id),
            Selector::Text(text) => sem::by_text(snapshot, text).ok_or_else(|| {
                format!(
                    "no node with text {text:?}; snapshot: {}",
                    sem::excerpt(snapshot)
                )
            }),
        }
    }

    pub fn scroll_y_of(&self, selector: &Selector) -> Result<f32, String> {
        let handle = self.resolve(&self.semantics()?, selector)?;
        Ok(unsafe { bubble_ffi::bubble_scroll_y_of(self.ptr(), handle) })
    }

    /// Center of the node's semantics bounds.
    pub fn center(&self, selector: &Selector) -> Result<(f32, f32), String> {
        let node = self
            .semantics()?
            .nodes
            .into_iter()
            .find(|node| match selector {
                Selector::Id(id) => node.test_id == *id,
                Selector::Text(text) => node.label == *text || node.value == *text,
            })
            .ok_or_else(|| format!("selector {selector:?} matched no node"))?;
        let [x, y, w, h] = node.bounds;
        Ok((x + w / 2.0, y + h / 2.0))
    }

    /// Run JS synchronously and return its result.
    pub fn eval(&self, source: &str) -> Result<String, String> {
        let source = c_string(source)?;
        let mut buf = vec![0 as libc_char; 1024];
        let code = unsafe {
            bubble_ffi::bubble_eval(
                self.ptr(),
                source.as_ptr(),
                buf.as_mut_ptr(),
                buf.len() as u32,
            )
        };
        if code != 0 {
            return Err(last_error(self.ptr()));
        }
        Ok(unsafe { CStr::from_ptr(buf.as_ptr()) }
            .to_string_lossy()
            .into_owned())
    }

    /// Queue JS to run on the script thread.
    pub fn post_eval(&self, source: &str) -> Result<(), String> {
        let source = c_string(source)?;
        unsafe { bubble_ffi::bubble_post_eval(self.ptr(), source.as_ptr()) };
        Ok(())
    }

    pub fn set_visible(&self, visible: bool) {
        unsafe { bubble_ffi::bubble_set_visible(self.ptr(), i32::from(visible)) };
    }

    pub fn generation(&self) -> u32 {
        unsafe { bubble_ffi::bubble_generation(self.ptr()) }
    }

    pub fn faults(&self) -> u32 {
        unsafe { bubble_ffi::bubble_faults(self.ptr()) }
    }

    pub fn status(&self) -> String {
        let mut buf = vec![0 as libc_char; 64];
        let code =
            unsafe { bubble_ffi::bubble_status(self.ptr(), buf.as_mut_ptr(), buf.len() as u32) };
        if code != 0 {
            return last_error(self.ptr());
        }
        unsafe { CStr::from_ptr(buf.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    }

    /// Next queued `bubbleHost.call`, or `None`.
    pub fn poll_request(&self) -> Result<Option<HostRequest>, String> {
        let mut id = 0u32;
        let mut method = vec![0 as libc_char; 64];
        let mut params = vec![0 as libc_char; 1024];
        let code = unsafe {
            bubble_ffi::bubble_poll_request(
                self.ptr(),
                &mut id,
                method.as_mut_ptr(),
                method.len() as u32,
                params.as_mut_ptr(),
                params.len() as u32,
            )
        };
        if code == 0 {
            return Ok(None);
        }
        if code < 0 {
            return Err(last_error(self.ptr()));
        }
        Ok(Some(HostRequest {
            id,
            method: unsafe { CStr::from_ptr(method.as_ptr()) }
                .to_string_lossy()
                .into_owned(),
            params: unsafe { CStr::from_ptr(params.as_ptr()) }
                .to_string_lossy()
                .into_owned(),
        }))
    }

    pub fn finish_request(&self, id: u32, ok: bool, result: &str) -> Result<(), String> {
        let result = c_string(result)?;
        unsafe {
            bubble_ffi::bubble_finish_request(self.ptr(), id, i32::from(ok), result.as_ptr())
        };
        Ok(())
    }

    /// Wait until `text` appears in semantics as a label or value.
    pub fn wait_for(&self, text: &str) -> Result<(), String> {
        self.wait_for_within(text, DEFAULT_TIMEOUT)
    }

    pub fn wait_for_within(&self, text: &str, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        loop {
            let snapshot = self.semantics()?;
            if sem::present(&snapshot, text) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "text {text:?} did not appear within {timeout:?}; snapshot: {}",
                    sem::excerpt(&snapshot)
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Wait until no node carries `text`.
    pub fn wait_absent(&self, text: &str) -> Result<(), String> {
        let deadline = Instant::now() + DEFAULT_TIMEOUT;
        loop {
            let snapshot = self.semantics()?;
            if !sem::present(&snapshot, text) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "text {text:?} is still present after {DEFAULT_TIMEOUT:?}; snapshot: {}",
                    sem::excerpt(&snapshot)
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Wait until the recorded fault count reaches `count`.
    pub fn wait_faults(&self, count: u32) -> Result<(), String> {
        let deadline = Instant::now() + FAULT_TIMEOUT;
        loop {
            if self.faults() >= count {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "faults did not reach {count} within {FAULT_TIMEOUT:?}; status {}",
                    self.status()
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Wait until the shell status equals `status`.
    pub fn wait_status(&self, status: &str) -> Result<(), String> {
        let deadline = Instant::now() + DEFAULT_TIMEOUT;
        loop {
            let current = self.status();
            if current == status {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(format!("status stayed {current:?}, wanted {status:?}"));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Poll until a host request arrives and reply `result` to it.
    pub fn reply_next_request(&self, result: &str) -> Result<(), String> {
        let deadline = Instant::now() + DEFAULT_TIMEOUT;
        loop {
            match self.poll_request()? {
                Some(request) => return self.finish_request(request.id, true, result),
                None if Instant::now() >= deadline => {
                    return Err("no host request arrived".into());
                }
                None => std::thread::sleep(Duration::from_millis(10)),
            }
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(pump) = self.pump.take() {
            let _ = pump.join();
        }
        let shell = self.ptr();
        if !shell.is_null() {
            unsafe { bubble_ffi::bubble_destroy(shell) };
        }
    }
}

#[derive(Debug)]
pub struct HostRequest {
    pub id: u32,
    pub method: String,
    pub params: String,
}

/// `id:` for `testID` and `text:` for label/value.
#[derive(Clone, Debug, PartialEq)]
pub enum Selector {
    Id(String),
    Text(String),
}

impl Selector {
    pub fn id(&self) -> &str {
        match self {
            Self::Id(id) => id,
            Self::Text(_) => "text selector",
        }
    }
}

#[allow(non_camel_case_types)]
type libc_char = std::os::raw::c_char;
