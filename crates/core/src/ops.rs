//! `BSOP` version 5. Little-endian batch with a string table.
//! `Turn` coalesces repeated `SetText` / `SetProp` before encoding.
//! Applied batches still run in order.

use std::io::{Cursor, Read};

use crate::color::Color;
use crate::expr::{decode_program, ExprConst, Program};
use crate::ir::{Kind, NodeDesc};
use crate::state_style::AnimSpec;
use crate::ui::UiCapture;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    /// Compiled expression program. Only style declaration props accept it;
    /// every other consumer rejects it.
    Program(Program),
}

impl Value {
    pub fn as_bool(&self) -> Result<bool, String> {
        match self {
            Value::Bool(value) => Ok(*value),
            _ => Err("property value is not a bool".into()),
        }
    }

    pub fn as_f32(&self) -> Result<f32, String> {
        match self {
            Value::Int(value) => Ok(*value as f32),
            Value::Float(value) => Ok(*value as f32),
            _ => Err("property value is not a number".into()),
        }
    }

    pub fn optional_f32(&self) -> Result<Option<f32>, String> {
        match self {
            Value::Null => Ok(None),
            other => Ok(Some(other.as_f32()?)),
        }
    }

    /// Numeric colors are `0xAARRGGBB`; strings are parsed as CSS colors.
    pub fn as_color(&self) -> Result<Color, String> {
        match self {
            Value::Int(value) => u32::try_from(*value)
                .map(Color::from_argb)
                .map_err(|_| format!("invalid numeric color {value}")),
            Value::Float(value)
                if value.is_finite() && *value >= 0.0 && *value <= u32::MAX as f64 =>
            {
                Ok(Color::from_argb(*value as u32))
            }
            Value::String(text) => crate::color::parse(text),
            _ => Err("property value is not a color".into()),
        }
    }

    pub fn as_u32(&self) -> Result<u32, String> {
        match self {
            Value::Int(value) if *value >= 0 && *value <= u32::MAX as i64 => Ok(*value as u32),
            Value::Float(value) if *value >= 0.0 && *value <= u32::MAX as f64 => Ok(*value as u32),
            _ => Err("property value is not a 32-bit unsigned integer".into()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    SetPropColor {
        handle: u32,
        key: String,
        rgba: u32,
    },
    SetProp {
        handle: u32,
        key: String,
        value: Value,
    },
    SetText {
        handle: u32,
        text: String,
    },
    DisposeNode {
        handle: u32,
    },
    /// Final child index after the node is detached from its old parent.
    MoveNode {
        handle: u32,
        parent: u32,
        index: u32,
    },
    /// `handler` 0 clears the binding.
    BindEvent {
        handle: u32,
        handler: u32,
    },
    /// Instant scroll. Each offset is clamped to that axis' range; a disabled
    /// axis clamps to zero.
    ScrollTo {
        handle: u32,
        x: f32,
        y: f32,
    },
    /// Node creation. The handle is allocated by `Turn`; `text` is the initial
    /// content for text and field nodes.
    Spawn {
        handle: u32,
        parent: u32,
        kind: Kind,
        text: String,
    },
    /// Node creation at `index`; used by fragment anchors.
    SpawnAt {
        handle: u32,
        parent: u32,
        kind: Kind,
        text: String,
        index: u32,
    },
    /// Virtual gesture declaration attached to `target`; `index` is its
    /// position among that target's declarations.
    SpawnGesture {
        gesture: u32,
        target: u32,
        kind: u32,
        index: u32,
    },
    /// One recognition property. Unknown keys are rejected by the shell.
    SetGestureProp {
        gesture: u32,
        key: String,
        value: Value,
    },
    MoveGesture {
        gesture: u32,
        target: u32,
        index: u32,
    },
    DisposeGesture {
        gesture: u32,
    },
    /// `slot` is the gesture event kind; handler 0 clears the binding.
    BindGestureEvent {
        gesture: u32,
        slot: u8,
        handler: u32,
    },
    /// `handle` 0 clears focus.
    Focus {
        handle: u32,
    },
    /// Controlled text. `epoch` is the revision the script observed.
    /// `cancel` ends an open composition before applying.
    SetEditingValue {
        handle: u32,
        epoch: u64,
        value: String,
        sel_start: u32,
        sel_end: u32,
        cancel: bool,
    },
    /// UI-thread variable. `source` 0 means script-driven; `target` is bound
    /// by a later `BindVar` op when the reference resolves.
    CreateVar {
        var: u32,
        source: u8,
        initial: Value,
    },
    /// Attach a variable's source target; `target` 0 detaches.
    BindVar {
        var: u32,
        target: u32,
    },
    DisposeVar {
        var: u32,
    },
    /// Script-driven variable write, applied when the batch lands on the UI
    /// thread. Writing a source-linked variable is an error. The trailing
    /// anim payload chooses the transition: 0 immediate, 2 tween (duration
    /// f32, easing u8), 3 spring (stiffness f32, damping f32).
    SetVar {
        var: u32,
        value: Value,
        anim: Option<AnimSpec>,
    },
    /// Ask the shell to emit a settle event for `var` once its pending writes
    /// converge. With no animation configured that is immediate.
    SettleVar {
        var: u32,
    },
    /// Register one UI program ("worklet"): params, captures, constants, and
    /// stack bytecode. Validation fails the batch.
    DefineUiFunction {
        id: u32,
        params: u8,
        captures: Vec<UiCapture>,
        consts: Vec<ExprConst>,
        code: Vec<u8>,
    },
    /// Bind a UI program to one gesture event slot; `function` 0 clears.
    /// Registering a program clears that slot's script handler and vice versa.
    BindGestureProgram {
        gesture: u32,
        slot: u8,
        function: u32,
    },
    /// Run a UI program with JS-supplied arguments on the UI thread.
    RunUiFunction {
        function: u32,
        args: Vec<Value>,
    },
    /// Stop a variable's in-flight animation; the value stays where it is.
    CancelVarAnim {
        var: u32,
    },
    /// Virtual style declaration attached to `target`; `index` is its
    /// position among that target's declarations. `component` 1 ViewStyle,
    /// 2 TextStyle.
    SpawnStyleDecl {
        decl: u32,
        target: u32,
        component: u8,
        index: u32,
    },
    /// One declaration value: `when` or a paint override. `Value::Program`
    /// carries a compiled expression; `Null` clears a patch slot.
    SetStyleDeclProp {
        decl: u32,
        key: String,
        value: Value,
    },
    /// Final declaration index after it leaves its old slot.
    MoveStyleDecl {
        decl: u32,
        target: u32,
        index: u32,
    },
    DisposeStyleDecl {
        decl: u32,
    },
}

pub fn encode_set_text(seq: u32, handle: u32, text: &str) -> Vec<u8> {
    encode(
        seq,
        &[Op::SetText {
            handle,
            text: text.to_owned(),
        }],
    )
}

pub fn encode_set_prop_color(seq: u32, handle: u32, key: &str, rgba: u32) -> Vec<u8> {
    encode(
        seq,
        &[Op::SetPropColor {
            handle,
            key: key.to_owned(),
            rgba,
        }],
    )
}

/// Ops that create a node from a full descriptor: `Spawn` carries identity and
/// text, non-default fields follow as `SetProp`, and a handler becomes
/// `BindEvent`.
pub fn spawn_ops(desc: NodeDesc) -> Vec<Op> {
    let handle = desc.id;
    let mut ops = vec![Op::Spawn {
        handle,
        parent: desc.parent,
        kind: desc.kind,
        text: desc.text.clone(),
    }];
    push_desc_props(&mut ops, handle, &desc);
    ops
}

pub fn spawn_at_ops(desc: NodeDesc, index: u32) -> Vec<Op> {
    let handle = desc.id;
    let mut ops = vec![Op::SpawnAt {
        handle,
        parent: desc.parent,
        kind: desc.kind,
        text: desc.text.clone(),
        index,
    }];
    push_desc_props(&mut ops, handle, &desc);
    ops
}

fn push_desc_props(ops: &mut Vec<Op>, handle: u32, desc: &NodeDesc) {
    let default = NodeDesc::view(0, 0);
    let mut prop = |key: &str, value: Value| {
        ops.push(Op::SetProp {
            handle,
            key: key.into(),
            value,
        });
    };
    if desc.background != default.background {
        prop("backgroundColor", Value::Int(desc.background as i64));
    }
    if desc.color != default.color {
        prop("color", Value::Int(desc.color as i64));
    }
    if desc.font_size != default.font_size {
        prop("fontSize", Value::Float(desc.font_size as f64));
    }
    if desc.radius != default.radius {
        prop("radius", Value::Float(desc.radius as f64));
    }
    if desc.opacity != default.opacity {
        prop("opacity", Value::Float(desc.opacity as f64));
    }
    if desc.flex_grow != default.flex_grow {
        prop("flexGrow", Value::Float(desc.flex_grow as f64));
    }
    if desc.flex_shrink != default.flex_shrink {
        prop("flexShrink", Value::Float(desc.flex_shrink as f64));
    }
    if desc.direction != default.direction {
        prop(
            "flexDirection",
            Value::String(direction_name(desc.direction).into()),
        );
    }
    if desc.scroll_x {
        prop("overflowX", Value::String("scroll".into()));
    }
    if desc.scroll_y {
        prop("overflowY", Value::String("scroll".into()));
    }
    if let Some(width) = desc.width {
        prop("width", Value::Float(width as f64));
    }
    if let Some(height) = desc.height {
        prop("height", Value::Float(height as f64));
    }
    if desc.padding != default.padding {
        prop("paddingStart", Value::Float(desc.padding[0] as f64));
        prop("paddingEnd", Value::Float(desc.padding[1] as f64));
        prop("paddingTop", Value::Float(desc.padding[2] as f64));
        prop("paddingBottom", Value::Float(desc.padding[3] as f64));
    }
    if desc.handler != 0 {
        ops.push(Op::BindEvent {
            handle,
            handler: desc.handler,
        });
    }
}

fn direction_name(direction: u8) -> &'static str {
    match direction {
        1 => "row",
        2 => "column-reverse",
        3 => "row-reverse",
        _ => "column",
    }
}

/// Animation spec payload: code byte then parameters.
fn push_anim_spec(payload: &mut Vec<u8>, spec: &AnimSpec) {
    match spec {
        AnimSpec::Tween {
            duration_ms,
            easing,
        } => {
            payload.push(2);
            payload.extend_from_slice(&duration_ms.to_le_bytes());
            payload.push(*easing);
        }
        AnimSpec::Spring { stiffness, damping } => {
            payload.push(3);
            payload.extend_from_slice(&stiffness.to_le_bytes());
            payload.extend_from_slice(&damping.to_le_bytes());
        }
    }
}

/// `0` immediate, `2` tween (duration f32, easing u8), `3` spring (f32, f32).
fn read_anim_spec(cur: &mut Cursor<&[u8]>) -> Result<Option<AnimSpec>, String> {
    Ok(match read_u8(cur)? {
        0 => None,
        code => Some(read_anim_body(cur, code)?),
    })
}

/// `2` tween (duration f32, easing u8), `3` spring (f32, f32).
fn read_anim_body(cur: &mut Cursor<&[u8]>, code: u8) -> Result<AnimSpec, String> {
    Ok(match code {
        2 => AnimSpec::Tween {
            duration_ms: read_f32(cur)?,
            easing: read_u8(cur)?,
        },
        3 => AnimSpec::Spring {
            stiffness: read_f32(cur)?,
            damping: read_f32(cur)?,
        },
        other => return Err(format!("unknown anim spec code {other}")),
    })
}

/// One value payload: tag then payload, with strings interned.
fn write_value(payload: &mut Vec<u8>, value: &Value, intern: &mut impl FnMut(&str) -> u32) {
    match value {
        Value::Null => payload.push(0),
        Value::Bool(value) => {
            payload.push(1);
            payload.push(u8::from(*value));
        }
        Value::Int(value) => {
            payload.push(2);
            payload.extend_from_slice(&value.to_le_bytes());
        }
        Value::Float(value) => {
            payload.push(3);
            payload.extend_from_slice(&value.to_le_bytes());
        }
        Value::String(value) => {
            let value_id = intern(value);
            payload.push(4);
            payload.extend_from_slice(&value_id.to_le_bytes());
        }
        Value::Program(program) => {
            let bytes = program.encode();
            payload.push(5);
            payload.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            payload.extend_from_slice(&bytes);
        }
    }
}

/// Captures, constants, and bytecode of one UI program, as one payload.
/// Used by the `DefineUiFunction` op and the `define_ui_function` bridges.
pub fn encode_ui_function_payload(
    captures: &[UiCapture],
    consts: &[ExprConst],
    code: &[u8],
) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&(captures.len() as u16).to_le_bytes());
    for capture in captures {
        match capture {
            UiCapture::Num(value) => {
                payload.push(1);
                payload.extend_from_slice(&value.to_le_bytes());
            }
            UiCapture::Color(value) => {
                payload.push(2);
                payload.extend_from_slice(&value.to_argb().to_le_bytes());
            }
            UiCapture::Bool(value) => {
                payload.push(3);
                payload.push(u8::from(*value));
            }
            UiCapture::Shared(value) => {
                payload.push(4);
                payload.extend_from_slice(&value.to_le_bytes());
            }
            UiCapture::Function(value) => {
                payload.push(5);
                payload.extend_from_slice(&value.to_le_bytes());
            }
            UiCapture::JsCallback(value) => {
                payload.push(6);
                payload.extend_from_slice(&value.to_le_bytes());
            }
            UiCapture::Text(value) => {
                payload.push(7);
                payload.extend_from_slice(&(value.len() as u32).to_le_bytes());
                payload.extend_from_slice(value.as_bytes());
            }
        }
    }
    payload.push(consts.len().min(255) as u8);
    for value in consts {
        match value {
            ExprConst::Num(value) => {
                payload.push(1);
                payload.extend_from_slice(&value.to_le_bytes());
            }
            ExprConst::Color(value) => {
                payload.push(2);
                payload.extend_from_slice(&value.to_argb().to_le_bytes());
            }
            ExprConst::Bool(value) => {
                payload.push(3);
                payload.push(u8::from(*value));
            }
            ExprConst::ColorString(text) => {
                payload.push(4);
                payload.extend_from_slice(&(text.len() as u32).to_le_bytes());
                payload.extend_from_slice(text.as_bytes());
            }
        }
    }
    payload.extend_from_slice(&(code.len() as u32).to_le_bytes());
    payload.extend_from_slice(code);
    payload
}

/// Inverse of [`encode_ui_function_payload`].
pub fn decode_ui_function_payload(
    bytes: &[u8],
) -> Result<(Vec<UiCapture>, Vec<ExprConst>, Vec<u8>), String> {
    let mut at = 0usize;
    let u8_at = |at: &mut usize| -> Result<u8, String> {
        let byte = *bytes.get(*at).ok_or("truncated ui function payload")?;
        *at += 1;
        Ok(byte)
    };
    let u16_at = |at: &mut usize| -> Result<u16, String> {
        if *at + 2 > bytes.len() {
            return Err("truncated ui function payload".into());
        }
        let value = u16::from_le_bytes([bytes[*at], bytes[*at + 1]]);
        *at += 2;
        Ok(value)
    };
    let u32_at = |at: &mut usize| -> Result<u32, String> {
        if *at + 4 > bytes.len() {
            return Err("truncated ui function payload".into());
        }
        let value = u32::from_le_bytes(bytes[*at..*at + 4].try_into().unwrap());
        *at += 4;
        Ok(value)
    };
    let f32_at = |at: &mut usize| -> Result<f32, String> {
        if *at + 4 > bytes.len() {
            return Err("truncated ui function payload".into());
        }
        let value = f32::from_le_bytes(bytes[*at..*at + 4].try_into().unwrap());
        *at += 4;
        Ok(value)
    };
    let text_at = |at: &mut usize| -> Result<String, String> {
        let len = u32_at(at)? as usize;
        if *at + len > bytes.len() {
            return Err("truncated ui function payload".into());
        }
        let text = std::str::from_utf8(&bytes[*at..*at + len])
            .map_err(|_| "ui function text is not utf-8")?;
        *at += len;
        Ok(text.to_owned())
    };
    let capture_count = u16_at(&mut at)? as usize;
    let mut captures = Vec::with_capacity(capture_count);
    for _ in 0..capture_count {
        let tag = u8_at(&mut at)?;
        let capture = match tag {
            1 => UiCapture::Num(f32_at(&mut at)?),
            2 => UiCapture::Color(Color::from_argb(u32_at(&mut at)?)),
            3 => UiCapture::Bool(u8_at(&mut at)? != 0),
            4 => UiCapture::Shared(u32_at(&mut at)?),
            5 => UiCapture::Function(u32_at(&mut at)?),
            6 => UiCapture::JsCallback(u32_at(&mut at)?),
            7 => UiCapture::Text(text_at(&mut at)?),
            other => return Err(format!("unknown ui capture tag {other}")),
        };
        captures.push(capture);
    }
    let const_count = u8_at(&mut at)? as usize;
    let mut consts = Vec::with_capacity(const_count);
    for _ in 0..const_count {
        let tag = u8_at(&mut at)?;
        let value = match tag {
            1 => ExprConst::Num(f32_at(&mut at)?),
            2 => ExprConst::Color(Color::from_argb(u32_at(&mut at)?)),
            3 => ExprConst::Bool(u8_at(&mut at)? != 0),
            4 => ExprConst::ColorString(text_at(&mut at)?),
            other => return Err(format!("unknown ui const tag {other}")),
        };
        consts.push(value);
    }
    let code_len = u32_at(&mut at)? as usize;
    if at + code_len > bytes.len() {
        return Err("truncated ui program code".into());
    }
    let code = bytes[at..at + code_len].to_vec();
    Ok((captures, consts, code))
}

/// JS→UI call arguments: `bool u8`, `float f32`, `string len u32 + bytes`.
pub fn encode_ui_args(args: &[Value]) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(args.len().min(255) as u8);
    for value in args {
        match value {
            Value::Bool(value) => {
                out.push(1);
                out.push(u8::from(*value));
            }
            Value::Int(value) => {
                out.push(2);
                out.extend_from_slice(&(*value as f32).to_le_bytes());
            }
            Value::Float(value) => {
                out.push(2);
                out.extend_from_slice(&(*value as f32).to_le_bytes());
            }
            Value::String(value) => {
                out.push(3);
                out.extend_from_slice(&(value.len() as u32).to_le_bytes());
                out.extend_from_slice(value.as_bytes());
            }
            _ => {
                out.push(2);
                out.extend_from_slice(&0.0f32.to_le_bytes());
            }
        }
    }
    out
}

/// Inverse of [`encode_ui_args`]. Strings become `Value::String`; the shell
/// faults when a worklet consumes one.
pub fn decode_ui_args(bytes: &[u8]) -> Result<Vec<Value>, String> {
    let mut at = 0usize;
    let argc = *bytes.get(at).ok_or("truncated ui args")? as usize;
    at += 1;
    let mut args = Vec::with_capacity(argc);
    for _ in 0..argc {
        let tag = *bytes.get(at).ok_or("truncated ui args")?;
        at += 1;
        args.push(match tag {
            1 => {
                let value = *bytes.get(at).ok_or("truncated ui args")? != 0;
                at += 1;
                Value::Bool(value)
            }
            2 => {
                if at + 4 > bytes.len() {
                    return Err("truncated ui args".into());
                }
                let value = f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
                at += 4;
                Value::Float(f64::from(value))
            }
            3 => {
                let len = u32::from_le_bytes(
                    bytes
                        .get(at..at + 4)
                        .ok_or("truncated ui args")?
                        .try_into()
                        .unwrap(),
                ) as usize;
                at += 4;
                if at + len > bytes.len() {
                    return Err("truncated ui args".into());
                }
                let text = std::str::from_utf8(&bytes[at..at + len])
                    .map_err(|_| "ui arg text is not utf-8")?;
                at += len;
                Value::String(text.to_owned())
            }
            other => return Err(format!("unknown ui arg tag {other}")),
        });
    }
    Ok(args)
}

pub fn encode(seq: u32, ops: &[Op]) -> Vec<u8> {
    let mut strings: Vec<String> = Vec::new();
    let mut intern = |value: &str| -> u32 {
        if let Some(index) = strings.iter().position(|existing| existing == value) {
            return index as u32;
        }
        strings.push(value.to_owned());
        (strings.len() - 1) as u32
    };

    let mut payload = Vec::new();
    for op in ops {
        match op {
            Op::SetPropColor { handle, key, rgba } => {
                let key_id = intern(key);
                payload.push(1);
                payload.extend_from_slice(&handle.to_le_bytes());
                payload.extend_from_slice(&key_id.to_le_bytes());
                payload.push(5);
                payload.extend_from_slice(&rgba.to_le_bytes());
            }
            Op::SetProp { handle, key, value } => {
                let key_id = intern(key);
                payload.push(10);
                payload.extend_from_slice(&handle.to_le_bytes());
                payload.extend_from_slice(&key_id.to_le_bytes());
                write_value(&mut payload, value, &mut intern);
            }
            Op::SetText { handle, text } => {
                let text_id = intern(text);
                payload.push(2);
                payload.extend_from_slice(&handle.to_le_bytes());
                payload.extend_from_slice(&text_id.to_le_bytes());
            }
            Op::DisposeNode { handle } => {
                payload.push(4);
                payload.extend_from_slice(&handle.to_le_bytes());
            }
            Op::MoveNode {
                handle,
                parent,
                index,
            } => {
                payload.push(5);
                payload.extend_from_slice(&handle.to_le_bytes());
                payload.extend_from_slice(&parent.to_le_bytes());
                payload.extend_from_slice(&index.to_le_bytes());
            }
            Op::BindEvent { handle, handler } => {
                payload.push(6);
                payload.extend_from_slice(&handle.to_le_bytes());
                payload.extend_from_slice(&handler.to_le_bytes());
            }
            Op::ScrollTo { handle, x, y } => {
                payload.push(7);
                payload.extend_from_slice(&handle.to_le_bytes());
                payload.extend_from_slice(&x.to_le_bytes());
                payload.extend_from_slice(&y.to_le_bytes());
            }
            Op::Spawn {
                handle,
                parent,
                kind,
                text,
            } => {
                let text_id = intern(text);
                payload.push(8);
                payload.extend_from_slice(&handle.to_le_bytes());
                payload.extend_from_slice(&parent.to_le_bytes());
                payload.push(*kind as u8);
                payload.extend_from_slice(&text_id.to_le_bytes());
            }
            Op::SpawnAt {
                handle,
                parent,
                kind,
                text,
                index,
            } => {
                let text_id = intern(text);
                payload.push(9);
                payload.extend_from_slice(&handle.to_le_bytes());
                payload.extend_from_slice(&parent.to_le_bytes());
                payload.push(*kind as u8);
                payload.extend_from_slice(&text_id.to_le_bytes());
                payload.extend_from_slice(&index.to_le_bytes());
            }
            Op::SpawnGesture {
                gesture,
                target,
                kind,
                index,
            } => {
                payload.push(11);
                payload.extend_from_slice(&gesture.to_le_bytes());
                payload.extend_from_slice(&target.to_le_bytes());
                payload.push(*kind as u8);
                payload.extend_from_slice(&index.to_le_bytes());
            }
            Op::SetGestureProp {
                gesture,
                key,
                value,
            } => {
                let key_id = intern(key);
                payload.push(12);
                payload.extend_from_slice(&gesture.to_le_bytes());
                payload.extend_from_slice(&key_id.to_le_bytes());
                write_value(&mut payload, value, &mut intern);
            }
            Op::MoveGesture {
                gesture,
                target,
                index,
            } => {
                payload.push(13);
                payload.extend_from_slice(&gesture.to_le_bytes());
                payload.extend_from_slice(&target.to_le_bytes());
                payload.extend_from_slice(&index.to_le_bytes());
            }
            Op::DisposeGesture { gesture } => {
                payload.push(14);
                payload.extend_from_slice(&gesture.to_le_bytes());
            }
            Op::BindGestureEvent {
                gesture,
                slot,
                handler,
            } => {
                payload.push(15);
                payload.extend_from_slice(&gesture.to_le_bytes());
                payload.push(*slot);
                payload.extend_from_slice(&handler.to_le_bytes());
            }
            Op::Focus { handle } => {
                payload.push(16);
                payload.extend_from_slice(&handle.to_le_bytes());
            }
            Op::SetEditingValue {
                handle,
                epoch,
                value,
                sel_start,
                sel_end,
                cancel,
            } => {
                let value_id = intern(value);
                payload.push(17);
                payload.extend_from_slice(&handle.to_le_bytes());
                payload.extend_from_slice(&epoch.to_le_bytes());
                payload.extend_from_slice(&value_id.to_le_bytes());
                payload.extend_from_slice(&sel_start.to_le_bytes());
                payload.extend_from_slice(&sel_end.to_le_bytes());
                payload.push(u8::from(*cancel));
            }
            Op::CreateVar {
                var,
                source,
                initial,
            } => {
                payload.push(22);
                payload.extend_from_slice(&var.to_le_bytes());
                payload.push(*source);
                write_value(&mut payload, initial, &mut intern);
            }
            Op::BindVar { var, target } => {
                payload.push(23);
                payload.extend_from_slice(&var.to_le_bytes());
                payload.extend_from_slice(&target.to_le_bytes());
            }
            Op::DisposeVar { var } => {
                payload.push(24);
                payload.extend_from_slice(&var.to_le_bytes());
            }
            Op::SetVar { var, value, anim } => {
                payload.push(25);
                payload.extend_from_slice(&var.to_le_bytes());
                write_value(&mut payload, value, &mut intern);
                match anim {
                    None => payload.push(0),
                    Some(spec) => push_anim_spec(&mut payload, spec),
                }
            }
            Op::SettleVar { var } => {
                payload.push(26);
                payload.extend_from_slice(&var.to_le_bytes());
            }
            Op::BindGestureProgram {
                gesture,
                slot,
                function,
            } => {
                payload.push(27);
                payload.extend_from_slice(&gesture.to_le_bytes());
                payload.push(*slot);
                payload.extend_from_slice(&function.to_le_bytes());
            }
            Op::RunUiFunction { function, args } => {
                payload.push(28);
                payload.extend_from_slice(&function.to_le_bytes());
                payload.push(args.len().min(255) as u8);
                for value in args {
                    write_value(&mut payload, value, &mut intern);
                }
            }
            Op::CancelVarAnim { var } => {
                payload.push(30);
                payload.extend_from_slice(&var.to_le_bytes());
            }
            Op::DefineUiFunction {
                id,
                params,
                captures,
                consts,
                code,
            } => {
                payload.push(29);
                payload.extend_from_slice(&id.to_le_bytes());
                payload.push(*params);
                let program = encode_ui_function_payload(captures, consts, code);
                payload.extend_from_slice(&(program.len() as u32).to_le_bytes());
                payload.extend_from_slice(&program);
            }
            Op::SpawnStyleDecl {
                decl,
                target,
                component,
                index,
            } => {
                payload.push(18);
                payload.extend_from_slice(&decl.to_le_bytes());
                payload.extend_from_slice(&target.to_le_bytes());
                payload.push(*component);
                payload.extend_from_slice(&index.to_le_bytes());
            }
            Op::SetStyleDeclProp { decl, key, value } => {
                let key_id = intern(key);
                payload.push(19);
                payload.extend_from_slice(&decl.to_le_bytes());
                payload.extend_from_slice(&key_id.to_le_bytes());
                write_value(&mut payload, value, &mut intern);
            }
            Op::MoveStyleDecl {
                decl,
                target,
                index,
            } => {
                payload.push(20);
                payload.extend_from_slice(&decl.to_le_bytes());
                payload.extend_from_slice(&target.to_le_bytes());
                payload.extend_from_slice(&index.to_le_bytes());
            }
            Op::DisposeStyleDecl { decl } => {
                payload.push(21);
                payload.extend_from_slice(&decl.to_le_bytes());
            }
        }
    }

    let mut table = Vec::new();
    for value in &strings {
        let bytes = value.as_bytes();
        table.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        table.extend_from_slice(bytes);
    }

    let mut out = Vec::new();
    out.extend_from_slice(b"BSOP");
    out.extend_from_slice(&5u16.to_le_bytes());
    out.extend_from_slice(&seq.to_le_bytes());
    out.extend_from_slice(&(ops.len() as u32).to_le_bytes());
    out.extend_from_slice(&(table.len() as u32).to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&table);
    out.extend_from_slice(&payload);
    out
}

pub fn decode_ops(bytes: &[u8]) -> Result<(u32, Vec<Op>), String> {
    let mut cur = Cursor::new(bytes);
    let mut magic = [0u8; 4];
    cur.read_exact(&mut magic)
        .map_err(|_| "truncated op header")?;
    if &magic != b"BSOP" {
        return Err("bad op magic".into());
    }
    let version = read_u16(&mut cur)?;
    if version != 5 {
        return Err(format!("unsupported op version {version}"));
    }
    let seq = read_u32(&mut cur)?;
    let op_count = read_u32(&mut cur)? as usize;
    let table_len = read_u32(&mut cur)? as usize;
    let payload_len = read_u32(&mut cur)? as usize;
    let mut table_bytes = vec![0u8; table_len];
    cur.read_exact(&mut table_bytes)
        .map_err(|_| "truncated string table")?;
    let strings = parse_strings(&table_bytes)?;
    let mut payload = vec![0u8; payload_len];
    cur.read_exact(&mut payload)
        .map_err(|_| "truncated op payload")?;
    if cur.position() != bytes.len() as u64 {
        return Err("trailing op bytes".into());
    }
    let mut payload_cur = Cursor::new(payload.as_slice());
    let mut ops = Vec::with_capacity(op_count);
    for _ in 0..op_count {
        let tag = read_u8(&mut payload_cur)?;
        match tag {
            1 => {
                let handle = read_u32(&mut payload_cur)?;
                let key_id = read_u32(&mut payload_cur)? as usize;
                let value_tag = read_u8(&mut payload_cur)?;
                if value_tag != 5 {
                    return Err(format!("unsupported prop value tag {value_tag}"));
                }
                let rgba = read_u32(&mut payload_cur)?;
                let key = strings.get(key_id).ok_or("prop key out of range")?.clone();
                ops.push(Op::SetPropColor { handle, key, rgba });
            }
            10 => {
                let handle = read_u32(&mut payload_cur)?;
                let key_id = read_u32(&mut payload_cur)? as usize;
                let key = strings.get(key_id).ok_or("prop key out of range")?.clone();
                let value = read_value(&mut payload_cur, &strings)?;
                ops.push(Op::SetProp { handle, key, value });
            }
            2 => {
                let handle = read_u32(&mut payload_cur)?;
                let text_id = read_u32(&mut payload_cur)? as usize;
                let text = strings.get(text_id).ok_or("text out of range")?.clone();
                ops.push(Op::SetText { handle, text });
            }
            4 => ops.push(Op::DisposeNode {
                handle: read_u32(&mut payload_cur)?,
            }),
            5 => ops.push(Op::MoveNode {
                handle: read_u32(&mut payload_cur)?,
                parent: read_u32(&mut payload_cur)?,
                index: read_u32(&mut payload_cur)?,
            }),
            6 => ops.push(Op::BindEvent {
                handle: read_u32(&mut payload_cur)?,
                handler: read_u32(&mut payload_cur)?,
            }),
            7 => ops.push(Op::ScrollTo {
                handle: read_u32(&mut payload_cur)?,
                x: read_f32(&mut payload_cur)?,
                y: read_f32(&mut payload_cur)?,
            }),
            8 => {
                let handle = read_u32(&mut payload_cur)?;
                let parent = read_u32(&mut payload_cur)?;
                let kind = Kind::from_u8(read_u8(&mut payload_cur)?)?;
                let text_id = read_u32(&mut payload_cur)? as usize;
                let text = strings
                    .get(text_id)
                    .ok_or("spawn text out of range")?
                    .clone();
                ops.push(Op::Spawn {
                    handle,
                    parent,
                    kind,
                    text,
                });
            }
            9 => {
                let handle = read_u32(&mut payload_cur)?;
                let parent = read_u32(&mut payload_cur)?;
                let kind = Kind::from_u8(read_u8(&mut payload_cur)?)?;
                let text_id = read_u32(&mut payload_cur)? as usize;
                let text = strings
                    .get(text_id)
                    .ok_or("spawn text out of range")?
                    .clone();
                let index = read_u32(&mut payload_cur)?;
                ops.push(Op::SpawnAt {
                    handle,
                    parent,
                    kind,
                    text,
                    index,
                });
            }
            11 => {
                let gesture = read_u32(&mut payload_cur)?;
                let target = read_u32(&mut payload_cur)?;
                let kind = read_u8(&mut payload_cur)? as u32;
                let index = read_u32(&mut payload_cur)?;
                ops.push(Op::SpawnGesture {
                    gesture,
                    target,
                    kind,
                    index,
                });
            }
            12 => {
                let gesture = read_u32(&mut payload_cur)?;
                let key_id = read_u32(&mut payload_cur)? as usize;
                let key = strings
                    .get(key_id)
                    .ok_or("gesture key out of range")?
                    .clone();
                let value = read_value(&mut payload_cur, &strings)?;
                ops.push(Op::SetGestureProp {
                    gesture,
                    key,
                    value,
                });
            }
            13 => ops.push(Op::MoveGesture {
                gesture: read_u32(&mut payload_cur)?,
                target: read_u32(&mut payload_cur)?,
                index: read_u32(&mut payload_cur)?,
            }),
            14 => ops.push(Op::DisposeGesture {
                gesture: read_u32(&mut payload_cur)?,
            }),
            15 => {
                let gesture = read_u32(&mut payload_cur)?;
                let slot = read_u8(&mut payload_cur)?;
                let handler = read_u32(&mut payload_cur)?;
                ops.push(Op::BindGestureEvent {
                    gesture,
                    slot,
                    handler,
                });
            }
            16 => ops.push(Op::Focus {
                handle: read_u32(&mut payload_cur)?,
            }),
            17 => {
                let handle = read_u32(&mut payload_cur)?;
                let epoch = read_u64(&mut payload_cur)?;
                let value_id = read_u32(&mut payload_cur)? as usize;
                let value = strings
                    .get(value_id)
                    .ok_or("editing value out of range")?
                    .clone();
                let sel_start = read_u32(&mut payload_cur)?;
                let sel_end = read_u32(&mut payload_cur)?;
                let cancel = read_u8(&mut payload_cur)? != 0;
                ops.push(Op::SetEditingValue {
                    handle,
                    epoch,
                    value,
                    sel_start,
                    sel_end,
                    cancel,
                });
            }
            22 => {
                let var = read_u32(&mut payload_cur)?;
                let source = read_u8(&mut payload_cur)?;
                let initial = read_value(&mut payload_cur, &strings)?;
                ops.push(Op::CreateVar {
                    var,
                    source,
                    initial,
                });
            }
            23 => ops.push(Op::BindVar {
                var: read_u32(&mut payload_cur)?,
                target: read_u32(&mut payload_cur)?,
            }),
            24 => ops.push(Op::DisposeVar {
                var: read_u32(&mut payload_cur)?,
            }),
            25 => {
                let var = read_u32(&mut payload_cur)?;
                let value = read_value(&mut payload_cur, &strings)?;
                let anim = read_anim_spec(&mut payload_cur)?;
                ops.push(Op::SetVar { var, value, anim });
            }
            26 => ops.push(Op::SettleVar {
                var: read_u32(&mut payload_cur)?,
            }),
            27 => {
                let gesture = read_u32(&mut payload_cur)?;
                let slot = read_u8(&mut payload_cur)?;
                let function = read_u32(&mut payload_cur)?;
                ops.push(Op::BindGestureProgram {
                    gesture,
                    slot,
                    function,
                });
            }
            28 => {
                let function = read_u32(&mut payload_cur)?;
                let argc = read_u8(&mut payload_cur)? as usize;
                let mut args = Vec::with_capacity(argc);
                for _ in 0..argc {
                    args.push(read_value(&mut payload_cur, &strings)?);
                }
                ops.push(Op::RunUiFunction { function, args });
            }
            30 => ops.push(Op::CancelVarAnim {
                var: read_u32(&mut payload_cur)?,
            }),
            29 => {
                let id = read_u32(&mut payload_cur)?;
                let params = read_u8(&mut payload_cur)?;
                let len = read_u32(&mut payload_cur)? as usize;
                let start = payload_cur.position() as usize;
                let rest = payload_cur.get_ref();
                let end = start
                    .checked_add(len)
                    .filter(|end| *end <= rest.len())
                    .ok_or("truncated ui program")?;
                let (captures, consts, code) = decode_ui_function_payload(&rest[start..end])?;
                payload_cur.set_position(end as u64);
                ops.push(Op::DefineUiFunction {
                    id,
                    params,
                    captures,
                    consts,
                    code,
                });
            }
            18 => ops.push(Op::SpawnStyleDecl {
                decl: read_u32(&mut payload_cur)?,
                target: read_u32(&mut payload_cur)?,
                component: read_u8(&mut payload_cur)?,
                index: read_u32(&mut payload_cur)?,
            }),
            19 => {
                let decl = read_u32(&mut payload_cur)?;
                let key_id = read_u32(&mut payload_cur)? as usize;
                let key = strings
                    .get(key_id)
                    .ok_or("style declaration key out of range")?
                    .clone();
                let value = read_value(&mut payload_cur, &strings)?;
                ops.push(Op::SetStyleDeclProp { decl, key, value });
            }
            20 => ops.push(Op::MoveStyleDecl {
                decl: read_u32(&mut payload_cur)?,
                target: read_u32(&mut payload_cur)?,
                index: read_u32(&mut payload_cur)?,
            }),
            21 => ops.push(Op::DisposeStyleDecl {
                decl: read_u32(&mut payload_cur)?,
            }),
            other => return Err(format!("unknown op tag {other}")),
        }
    }
    if payload_cur.position() != payload.len() as u64 {
        return Err("op payload length mismatch".into());
    }
    Ok((seq, ops))
}

fn read_value(cur: &mut Cursor<&[u8]>, strings: &[String]) -> Result<Value, String> {
    Ok(match read_u8(cur)? {
        0 => Value::Null,
        1 => Value::Bool(read_u8(cur)? != 0),
        2 => Value::Int(read_i64(cur)?),
        3 => Value::Float(read_f64(cur)?),
        4 => {
            let value_id = read_u32(cur)? as usize;
            Value::String(
                strings
                    .get(value_id)
                    .ok_or("value string out of range")?
                    .clone(),
            )
        }
        5 => {
            let len = read_u32(cur)? as usize;
            let mut buf = vec![0u8; len];
            cur.read_exact(&mut buf)
                .map_err(|_| "truncated program value")?;
            Value::Program(decode_program(&buf).map_err(|e| e.to_string())?)
        }
        other => return Err(format!("unsupported prop value tag {other}")),
    })
}

fn parse_strings(bytes: &[u8]) -> Result<Vec<String>, String> {
    let mut cur = Cursor::new(bytes);
    let mut strings = Vec::new();
    while cur.position() < bytes.len() as u64 {
        let len = read_u32(&mut cur)? as usize;
        let mut buf = vec![0u8; len];
        cur.read_exact(&mut buf).map_err(|_| "truncated string")?;
        strings.push(String::from_utf8(buf).map_err(|_| "string is not utf-8")?);
    }
    Ok(strings)
}

fn read_u8(cur: &mut Cursor<&[u8]>) -> Result<u8, String> {
    let mut buf = [0u8; 1];
    cur.read_exact(&mut buf).map_err(|_| "truncated ops")?;
    Ok(buf[0])
}

fn read_u16(cur: &mut Cursor<&[u8]>) -> Result<u16, String> {
    let mut buf = [0u8; 2];
    cur.read_exact(&mut buf).map_err(|_| "truncated ops")?;
    Ok(u16::from_le_bytes(buf))
}

fn read_u32(cur: &mut Cursor<&[u8]>) -> Result<u32, String> {
    let mut buf = [0u8; 4];
    cur.read_exact(&mut buf).map_err(|_| "truncated ops")?;
    Ok(u32::from_le_bytes(buf))
}

fn read_u64(cur: &mut Cursor<&[u8]>) -> Result<u64, String> {
    let mut buf = [0u8; 8];
    cur.read_exact(&mut buf).map_err(|_| "truncated ops")?;
    Ok(u64::from_le_bytes(buf))
}

fn read_i64(cur: &mut Cursor<&[u8]>) -> Result<i64, String> {
    let mut buf = [0u8; 8];
    cur.read_exact(&mut buf).map_err(|_| "truncated ops")?;
    Ok(i64::from_le_bytes(buf))
}

fn read_f64(cur: &mut Cursor<&[u8]>) -> Result<f64, String> {
    let mut buf = [0u8; 8];
    cur.read_exact(&mut buf).map_err(|_| "truncated ops")?;
    Ok(f64::from_le_bytes(buf))
}

fn read_f32(cur: &mut Cursor<&[u8]>) -> Result<f32, String> {
    let mut buf = [0u8; 4];
    cur.read_exact(&mut buf).map_err(|_| "truncated ops")?;
    Ok(f32::from_le_bytes(buf))
}
