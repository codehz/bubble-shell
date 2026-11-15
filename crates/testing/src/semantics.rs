//! Semantics snapshot fetch and query.
//!
//! Selectors resolve against the same JSON the iOS/Android bridges read:
//! `SemanticsNode.test_id` for `id:` and node label/value for `text:`.

use bubble_core::SemanticsSnapshot;

/// Fetch the current semantics snapshot of `shell`.
pub fn snapshot(shell: *mut bubble_ffi::BubbleShell) -> Result<SemanticsSnapshot, String> {
    // Counter-style fixtures with a few hundred nodes stay well under 256 KiB.
    let mut buf = vec![0 as libc_char; 256 * 1024];
    let code = unsafe { bubble_ffi::bubble_semantics(shell, buf.as_mut_ptr(), buf.len() as u32) };
    if code < 0 {
        return Err(last_error(shell));
    }
    let text = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    serde_json::from_str(&text).map_err(|error| format!("semantics snapshot: {error}"))
}

#[allow(non_camel_case_types)]
type libc_char = std::os::raw::c_char;

/// Short snapshot excerpt for failure messages: every node as
/// `handle role "label"/"value" id`.
pub fn excerpt(snapshot: &SemanticsSnapshot) -> String {
    snapshot
        .nodes
        .iter()
        .map(|node| {
            let id = if node.test_id.is_empty() {
                String::new()
            } else {
                format!(" id:{}", node.test_id)
            };
            let value = if node.value.is_empty() {
                node.label.clone()
            } else {
                format!("{}={}", node.label, node.value)
            };
            format!("{} {} \"{}\"{}", node.handle, node.role.as_str(), value, id)
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Handle of the node matching a `id:` selector, or an error naming what the
/// snapshot contains.
pub fn by_id(snapshot: &SemanticsSnapshot, id: &str) -> Result<u32, String> {
    snapshot
        .nodes
        .iter()
        .find(|node| node.test_id == id)
        .map(|node| node.handle)
        .ok_or_else(|| format!("no node with testID {id}; snapshot: {}", excerpt(snapshot)))
}

/// Handle of the first visible node whose label or value is `text`. Hidden
/// nodes (for example outside an active modal) stay out of text matching.
pub fn by_text(snapshot: &SemanticsSnapshot, text: &str) -> Option<u32> {
    snapshot
        .nodes
        .iter()
        .find(|node| !node.hidden && (node.label == text || node.value == text))
        .map(|node| node.handle)
}

/// Whether any visible node carries `text` as label or value.
pub fn present(snapshot: &SemanticsSnapshot, text: &str) -> bool {
    by_text(snapshot, text).is_some()
}

fn last_error(shell: *mut bubble_ffi::BubbleShell) -> String {
    unsafe {
        let ptr = bubble_ffi::bubble_last_error(shell);
        if ptr.is_null() {
            return String::new();
        }
        std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned()
    }
}
