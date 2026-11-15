//! Shared ABI glue for the native and wasm bridges.
//!
//! Both bridges expose the same action codes, value tags, animation codes,
//! and host-layout retry protocol. These helpers keep that surface in one
//! place so the two bridges cannot drift apart on parsing or marshalling.

use crate::editing::{EditCommand, SessionId};
use crate::gesture::{PointerAction, PointerInput};
use crate::layout::{decode_layout, LayoutRequest, LayoutSnapshot};
use crate::ops::Value;
use crate::shell::Shell;
use crate::state_style::AnimSpec;

impl EditCommand {
    /// Bridge edit action codes shared by the native and wasm entry points:
    /// 0 focuses `a`, 1..9 map to edit commands.
    pub fn parse(action: u32, text: String, a: u32, b: u32) -> Result<Option<Self>, String> {
        Ok(match action {
            0 => None,
            1 => Some(Self::Insert(text)),
            2 => Some(Self::Backspace),
            3 => Some(Self::DeleteForward),
            4 => Some(Self::SetSelection { start: a, end: b }),
            5 => Some(Self::CompositionStart),
            6 => Some(Self::CompositionUpdate(text)),
            7 => Some(Self::CompositionEnd(text)),
            8 => Some(Self::Undo),
            9 => Some(Self::Redo),
            other => return Err(format!("bad edit action {other}")),
        })
    }
}

impl Value {
    /// Wire value tag: 0 null, 1 bool, 2 int, 3 float, 4 string. `text` is
    /// only consulted for tag 4, so callers pass an already-decoded string or
    /// the error that explains why it is absent.
    pub fn from_abi(tag: u32, number: f64, text: Result<String, String>) -> Result<Self, String> {
        Ok(match tag {
            0 => Self::Null,
            1 => Self::Bool(number != 0.0),
            2 => Self::Int(number as i64),
            3 => Self::Float(number),
            4 => Self::String(text?),
            other => return Err(format!("unsupported prop tag {other}")),
        })
    }
}

impl PointerInput {
    /// Wire pointer sample for the pointer and visual-pointer entry points.
    /// The hit path is filled in by the shell from its own tree.
    pub fn from_abi(
        action: PointerAction,
        id: u32,
        x: f32,
        y: f32,
        time_ms: f64,
        kind: u32,
        buttons: u32,
    ) -> Self {
        Self {
            action,
            id,
            x,
            y,
            time_ms,
            buttons,
            pointer_kind: kind.min(255) as u8,
            path: Vec::new(),
        }
    }
}

impl AnimSpec {
    /// Wire animation spec: 2 tween (`a` duration ms, `c` easing),
    /// 3 spring (`a` stiffness, `b` damping); matches the runtime's
    /// `animParts`. Code 0 means "no animation" and stays with the caller.
    pub fn from_abi(anim: u32, a: f64, b: f64, c: u32) -> Result<Self, String> {
        Ok(match anim {
            2 => Self::Tween {
                duration_ms: a as f32,
                easing: c.min(255) as u8,
            },
            3 => Self::Spring {
                stiffness: a as f32,
                damping: b as f32,
            },
            other => return Err(format!("unknown anim spec code {other}")),
        })
    }
}

/// `kind`: 1 memo, 2 render effect, 3 user effect.
pub fn parse_computation_kind(kind: u32) -> Result<crate::ComputationKind, String> {
    Ok(match kind {
        1 => crate::ComputationKind::Memo,
        2 => crate::ComputationKind::Effect,
        3 => crate::ComputationKind::UserEffect,
        other => return Err(format!("bad computation kind {other}")),
    })
}

/// The focused field's editing session, or an error when `session` is stale.
pub fn check_edit_session(shell: &Shell, session: u64) -> Result<SessionId, String> {
    let state = shell.focused_state()?;
    if state.session.get() != session {
        return Err("stale editing session".into());
    }
    Ok(state.session)
}

/// Read a host-provided UTF-8 buffer. Null or empty reads as empty.
pub fn read_abi_utf8(ptr: *const u8, len: u32, context: &str) -> Result<String, String> {
    let bytes = if ptr.is_null() || len == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(ptr, len as usize) }.to_vec()
    };
    String::from_utf8(bytes).map_err(|_| format!("{context} is not utf-8"))
}

/// Host layout protocol shared by the bridges: guess a buffer capacity from
/// the text length, call the host, retry once with the reported size, then
/// decode. `call` writes into `buf` and returns the encoded length, a larger
/// needed capacity, or negative on failure.
pub fn call_host_layout<F>(req: &LayoutRequest<'_>, mut call: F) -> Result<LayoutSnapshot, String>
where
    F: FnMut(u32, &mut Vec<u8>) -> i32,
{
    let mut cap = (req.text.len() as u32)
        .saturating_mul(64)
        .saturating_add(4096);
    for _ in 0..2 {
        let mut buf = vec![0u8; cap as usize];
        let n = call(cap, &mut buf);
        if n < 0 {
            return Err("host layout failed".into());
        }
        if n as u32 > cap {
            cap = n as u32;
            continue;
        }
        return decode_layout(&buf[..n as usize]);
    }
    Err("host layout did not fit".into())
}
