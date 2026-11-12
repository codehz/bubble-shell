//! One native shell, driven through the C ABI. The process holds a single
//! instance, so a [`Session`] destroys any previous shell on create and on drop.

use std::ffi::{CStr, CString};
use std::ptr;
use std::time::{Duration, Instant};

use bubble_ffi::{
    bubble_create, bubble_destroy, bubble_eval, bubble_faults, bubble_finish_request,
    bubble_font_count, bubble_font_epoch, bubble_frame, bubble_generation, bubble_last_error,
    bubble_load, bubble_node_box, bubble_node_text, bubble_pointer, bubble_poll_request,
    bubble_post_eval, bubble_register_font, bubble_scroll, bubble_scroll_x_of, bubble_scroll_y_of,
    bubble_set_visible, bubble_status,
};

fn c_string(text: &str) -> Result<CString, String> {
    CString::new(text).map_err(|_| "text contains a nul".into())
}

fn last_error() -> String {
    unsafe {
        let ptr = bubble_last_error();
        if ptr.is_null() {
            return String::new();
        }
        CStr::from_ptr(ptr).to_string_lossy().into_owned()
    }
}

/// The shell a scenario steps. Platform painters are out of scope: this reads
/// the same tree and paint bytes the host would replay.
pub struct Session {
    open: bool,
}

impl Session {
    pub fn create() -> Result<Self, String> {
        unsafe { bubble_destroy() };
        let code = unsafe { bubble_create(ptr::null()) };
        if code != 0 {
            return Err(last_error());
        }
        Ok(Self { open: true })
    }

    pub fn load(&mut self, manifest_json: &str, entry: &[u8]) -> Result<(), String> {
        let manifest = c_string(manifest_json)?;
        let code = unsafe { bubble_load(manifest.as_ptr(), entry.as_ptr(), entry.len() as u32) };
        if code != 0 {
            Err(last_error())
        } else {
            Ok(())
        }
    }

    pub fn frame(&mut self, width: f32, height: f32) -> Result<Vec<u8>, String> {
        let mut paint = ptr::null();
        let mut len = 0u32;
        let code = unsafe { bubble_frame(width, height, &mut paint, &mut len) };
        if code != 0 {
            return Err(last_error());
        }
        if paint.is_null() || len == 0 {
            return Ok(Vec::new());
        }
        Ok(unsafe { std::slice::from_raw_parts(paint, len as usize) }.to_vec())
    }

    pub fn pointer(&mut self, action: u32, id: u32, x: f32, y: f32, time_ms: f64) -> i32 {
        unsafe {
            bubble_pointer(
                action,
                id,
                x,
                y,
                time_ms,
                0,
                if action == 2 { 0 } else { 1 },
            )
        }
    }

    /// A complete tap: down then up one sample later.
    pub fn pointer_down(&mut self, x: f32, y: f32) {
        self.pointer(0, 1, x, y, 0.0);
        self.pointer(2, 1, x, y, 20.0);
    }

    pub fn scroll(&mut self, x: f32, y: f32, dx: f32, dy: f32) {
        unsafe { bubble_scroll(x, y, dx, dy) };
    }

    pub fn set_visible(&mut self, visible: bool) {
        unsafe { bubble_set_visible(if visible { 1 } else { 0 }) };
    }

    pub fn register_font(&mut self, family: &str, bytes: &[u8]) -> Result<(), String> {
        let family = c_string(family)?;
        unsafe { bubble_register_font(family.as_ptr(), bytes.as_ptr(), bytes.len() as u32) };
        Ok(())
    }

    pub fn scroll_x_of(&mut self, handle: u32) -> f32 {
        unsafe { bubble_scroll_x_of(handle) }
    }

    pub fn scroll_y_of(&mut self, handle: u32) -> f32 {
        unsafe { bubble_scroll_y_of(handle) }
    }

    pub fn node_text(&mut self, handle: u32) -> Result<String, String> {
        let mut buf = [0i8; 256];
        let code = unsafe { bubble_node_text(handle, buf.as_mut_ptr(), buf.len() as u32) };
        if code != 0 {
            return Err(last_error());
        }
        Ok(unsafe { CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned() })
    }

    pub fn node_box(&mut self, handle: u32) -> Result<(f32, f32, f32, f32), String> {
        let (mut x, mut y, mut w, mut h) = (0.0, 0.0, 0.0, 0.0);
        let code = unsafe { bubble_node_box(handle, &mut x, &mut y, &mut w, &mut h) };
        if code != 0 {
            Err(last_error())
        } else {
            Ok((x, y, w, h))
        }
    }
    pub fn eval(&mut self, source: &str) -> Result<String, String> {
        let source = c_string(source)?;
        let mut buf = [0i8; 512];
        let code = unsafe { bubble_eval(source.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) };
        if code != 0 {
            return Err(last_error());
        }
        Ok(unsafe { CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned() })
    }

    pub fn post_eval(&mut self, source: &str) -> Result<(), String> {
        let source = c_string(source)?;
        unsafe { bubble_post_eval(source.as_ptr()) };
        Ok(())
    }

    pub fn generation(&self) -> u32 {
        unsafe { bubble_generation() }
    }

    pub fn faults(&self) -> u32 {
        unsafe { bubble_faults() }
    }

    pub fn status(&self) -> String {
        let mut buf = [0i8; 64];
        let code = unsafe { bubble_status(buf.as_mut_ptr(), buf.len() as u32) };
        if code != 0 {
            return last_error();
        }
        unsafe { CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned() }
    }

    pub fn font_epoch(&self) -> u32 {
        unsafe { bubble_font_epoch() }
    }

    pub fn font_count(&self) -> u32 {
        unsafe { bubble_font_count() }
    }

    pub fn poll_request(&mut self) -> Result<Option<HostRequest>, String> {
        let mut id = 0u32;
        let mut method = [0i8; 64];
        let mut params = [0i8; 512];
        let code = unsafe {
            bubble_poll_request(
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
            return Err(last_error());
        }
        Ok(Some(HostRequest {
            id,
            method: unsafe {
                CStr::from_ptr(method.as_ptr())
                    .to_string_lossy()
                    .into_owned()
            },
            params: unsafe {
                CStr::from_ptr(params.as_ptr())
                    .to_string_lossy()
                    .into_owned()
            },
        }))
    }

    pub fn finish_request(&mut self, id: u32, ok: bool, result: &str) -> Result<(), String> {
        let result = c_string(result)?;
        unsafe { bubble_finish_request(id, if ok { 1 } else { 0 }, result.as_ptr()) };
        Ok(())
    }

    /// Poll `pred` until it is true or `timeout` elapses. Sleeps 10ms.
    pub fn wait_until(
        &mut self,
        timeout: Duration,
        mut pred: impl FnMut(&mut Self) -> bool,
    ) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if pred(self) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// Layout one frame without borrowing a [`Session`]. The UI thread and the
/// script thread already share the process-global shell this way.
pub fn frame_detached(width: f32, height: f32) -> Result<(), String> {
    let mut paint = ptr::null();
    let mut len = 0u32;
    let code = unsafe { bubble_frame(width, height, &mut paint, &mut len) };
    if code != 0 {
        Err(last_error())
    } else {
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if self.open {
            unsafe { bubble_destroy() };
            self.open = false;
        }
    }
}

#[derive(Debug)]
pub struct HostRequest {
    pub id: u32,
    pub method: String,
    pub params: String,
}
