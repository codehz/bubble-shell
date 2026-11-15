//! UI-thread programs ("worklets"): shared-value bytecode that runs inside
//! the shell's UI call instead of crossing to the script thread.
//!
//! A [`UiProgram`] is a small stack machine: constants, event fields, captured
//! shared values, calls to other programs, and shared-value writes. It reuses
//! the expression opcodes `OP_ADD..OP_ABS` (see [`crate::expr`]) so style
//! declarations and worklets share numeric semantics. Validation is static
//! per program at definition time; runtime faults (type mismatch, underflow,
//! unknown ids) abort the current call, bump `ui_faults`, and never corrupt
//! the tree.

use std::collections::HashSet;

use crate::color::Color;
use crate::expr::{ExprConst, ExprValue, OP_ABS, OP_ADD};
use crate::state_style::{AnimSpec, AnimState, EASE_LINEAR};

pub const OP_PUSH_CONST: u8 = 0;
pub const OP_PUSH_PARAM: u8 = 1;
pub const OP_PUSH_CAPTURE: u8 = 32;
pub const OP_LOAD_EVENT: u8 = 33;
pub const OP_STORE_SHARED: u8 = 34;
pub const OP_STORE_ANIM: u8 = 35;
pub const OP_CANCEL_ANIM: u8 = 36;
pub const OP_STORE_LOCAL: u8 = 37;
pub const OP_LOAD_LOCAL: u8 = 38;
pub const OP_CALL: u8 = 39;
pub const OP_RETURN: u8 = 40;
pub const OP_SCHEDULE_JS: u8 = 41;
pub const OP_JUMP: u8 = 42;
pub const OP_JUMP_IF_FALSE: u8 = 43;
pub const OP_POP: u8 = 44;
pub const OP_FLOOR: u8 = 45;
pub const OP_CEIL: u8 = 46;
pub const OP_ROUND: u8 = 47;
pub const OP_SQRT: u8 = 48;
pub const OP_POW: u8 = 49;

/// Maximum live operand slots per frame; the static check rejects anything
/// deeper.
const STACK_LIMIT: usize = 32;
/// Local slots per call frame.
const LOCALS: usize = 16;
/// Call depth cap; deeper nests fault.
const CALL_DEPTH: usize = 8;
/// Live slots across every nested frame. Each level owns its arguments, its
/// locals, and a full operand stack; nesting must not share one frame's
/// budget.
const STACK_TOTAL: usize = CALL_DEPTH * (LOCALS + STACK_LIMIT * 2);
/// Shared-value writes per call, bounding a runaway worklet.
const STORES_PER_CALL: usize = 64;

/// Gesture event field a `LOAD_EVENT` pushes.
const EVENT_FIELDS: u8 = 14;

/// One value on the UI stack. `Text` exists only to travel to
/// `scheduleOnJS`; stores, calls, and arithmetic reject it.
#[derive(Clone, Debug, PartialEq)]
pub enum UiValue {
    Num(f32),
    Color(Color),
    Bool(bool),
    Text(String),
}

impl UiValue {
    pub(crate) fn as_expr(self) -> Result<ExprValue, String> {
        match self {
            UiValue::Num(value) => Ok(ExprValue::Num(value)),
            UiValue::Color(value) => Ok(ExprValue::Color(value)),
            UiValue::Bool(value) => Ok(ExprValue::Bool(value)),
            UiValue::Text(_) => Err("a string cannot be stored or returned".into()),
        }
    }

    pub(crate) fn from_expr(value: ExprValue) -> Self {
        match value {
            ExprValue::Num(value) => UiValue::Num(value),
            ExprValue::Color(value) => UiValue::Color(value),
            ExprValue::Bool(value) => UiValue::Bool(value),
        }
    }

    /// JS→UI call argument. Strings only travel back to `scheduleOnJS`; a
    /// string arg faults when the worklet consumes it. Value kinds with no
    /// UI-thread representation fail the batch.
    pub(crate) fn try_from_value(value: &crate::ops::Value) -> Result<Self, String> {
        Ok(match value {
            crate::ops::Value::Bool(value) => UiValue::Bool(*value),
            crate::ops::Value::Int(value) => UiValue::Num(*value as f32),
            crate::ops::Value::Float(value) => UiValue::Num(*value as f32),
            crate::ops::Value::String(value) => UiValue::Text(value.clone()),
            other => return Err(format!("ui call argument {other:?} is not supported")),
        })
    }

    fn finite(&self) -> bool {
        match self {
            UiValue::Num(value) => value.is_finite(),
            _ => true,
        }
    }
}

/// One captured binding of a UI program.
#[derive(Clone, Debug, PartialEq)]
pub enum UiCapture {
    Num(f32),
    Color(Color),
    Bool(bool),
    /// Shared value id; loads push its current value, stores write it.
    Shared(u32),
    /// Another UI program id.
    Function(u32),
    /// JS-side handler id for `scheduleOnJS`.
    JsCallback(u32),
    Text(String),
}

/// One queued call. JS→UI calls live in `Shell.ui_calls`; UI→JS
/// notifications in `Shell.js_calls`.
#[derive(Clone, Debug)]
pub struct UiCall {
    pub function: u32,
    pub args: Vec<UiValue>,
}

/// `BSUE` version 1: magic, version, count, then `callback u32 + argc u8 +
/// tagged args` records. Args tags: 1 bool, 2 float, 3 string, 4 color
/// (`0xAARRGGBB` u32).
pub fn encode_js_calls(calls: &[UiCall]) -> Vec<u8> {
    let mut out = Vec::with_capacity(10 + calls.len() * 16);
    out.extend_from_slice(b"BSUE");
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(calls.len() as u32).to_le_bytes());
    for call in calls {
        out.extend_from_slice(&call.function.to_le_bytes());
        out.push(call.args.len().min(255) as u8);
        for arg in &call.args {
            match arg {
                UiValue::Bool(value) => {
                    out.push(1);
                    out.push(u8::from(*value));
                }
                UiValue::Num(value) => {
                    out.push(2);
                    out.extend_from_slice(&value.to_le_bytes());
                }
                UiValue::Text(value) => {
                    out.push(3);
                    out.extend_from_slice(&(value.len() as u32).to_le_bytes());
                    out.extend_from_slice(value.as_bytes());
                }
                UiValue::Color(value) => {
                    out.push(4);
                    out.extend_from_slice(&value.to_argb().to_le_bytes());
                }
            }
        }
    }
    out
}

/// A compiled UI program: constants, captures, and stack bytecode.
#[derive(Clone, Debug)]
pub struct UiProgram {
    pub params: u8,
    pub captures: Vec<UiCapture>,
    pub consts: Vec<ExprConst>,
    pub code: Vec<u8>,
}

/// One instruction, decoded for validation and execution.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Instr {
    PushConst(u8),
    PushParam(u8),
    PushCapture(u16),
    LoadEvent(u8),
    StoreShared(u16),
    StoreAnim(u16),
    CancelAnim(u16),
    StoreLocal(u8),
    LoadLocal(u8),
    Call(u16, u8),
    Return,
    ScheduleJs(u16, u8),
    Jump(u16),
    JumpIfFalse(u16),
    Pop,
    Op(u8),
}

/// Decode one instruction at `*cursor`, advancing it past the encoding.
fn decode_one(code: &[u8], cursor: &mut usize) -> Result<Instr, String> {
    let op = *code.get(*cursor).ok_or("truncated ui program")?;
    *cursor += 1;
    let u8_at = |cursor: &mut usize| -> Result<u8, String> {
        let byte = *code.get(*cursor).ok_or("truncated ui program")?;
        *cursor += 1;
        Ok(byte)
    };
    let u16_at = |cursor: &mut usize| -> Result<u16, String> {
        if *cursor + 2 > code.len() {
            return Err("truncated ui program".into());
        }
        let value = u16::from_le_bytes([code[*cursor], code[*cursor + 1]]);
        *cursor += 2;
        Ok(value)
    };
    Ok(match op {
        OP_PUSH_CONST => Instr::PushConst(u8_at(cursor)?),
        OP_PUSH_PARAM => Instr::PushParam(u8_at(cursor)?),
        OP_PUSH_CAPTURE => Instr::PushCapture(u16_at(cursor)?),
        OP_LOAD_EVENT => {
            let field = u8_at(cursor)?;
            if field >= EVENT_FIELDS {
                return Err(format!("unknown event field {field}"));
            }
            Instr::LoadEvent(field)
        }
        OP_STORE_SHARED => Instr::StoreShared(u16_at(cursor)?),
        OP_STORE_ANIM => Instr::StoreAnim(u16_at(cursor)?),
        OP_CANCEL_ANIM => Instr::CancelAnim(u16_at(cursor)?),
        OP_STORE_LOCAL | OP_LOAD_LOCAL => {
            let slot = u8_at(cursor)?;
            if slot >= LOCALS as u8 {
                return Err(format!("local slot {slot} is out of range"));
            }
            if op == OP_STORE_LOCAL {
                Instr::StoreLocal(slot)
            } else {
                Instr::LoadLocal(slot)
            }
        }
        OP_CALL => {
            let capture = u16_at(cursor)?;
            let argc = u8_at(cursor)?;
            Instr::Call(capture, argc)
        }
        OP_RETURN => Instr::Return,
        OP_SCHEDULE_JS => {
            let capture = u16_at(cursor)?;
            let argc = u8_at(cursor)?;
            if argc > 4 {
                return Err("scheduleOnJS takes at most 4 arguments".into());
            }
            Instr::ScheduleJs(capture, argc)
        }
        OP_JUMP => Instr::Jump(u16_at(cursor)?),
        OP_JUMP_IF_FALSE => Instr::JumpIfFalse(u16_at(cursor)?),
        OP_POP => Instr::Pop,
        OP_ADD..=OP_ABS => Instr::Op(op),
        OP_FLOOR..=OP_POW => Instr::Op(op),
        other => return Err(format!("unknown ui program opcode {other}")),
    })
}

/// Stack effect `(pops, pushes)` of one instruction for the static check.
fn effect_of(instr: &Instr) -> (usize, usize) {
    match instr {
        Instr::PushConst(_)
        | Instr::PushParam(_)
        | Instr::PushCapture(_)
        | Instr::LoadEvent(_)
        | Instr::LoadLocal(_) => (0, 1),
        Instr::Op(op) => {
            // The shared expression table stops at OP_ABS; the worklet-only
            // FLOOR..POW range adds unary math plus a binary POW.
            let pops = match crate::expr::op_pops(*op) {
                Some(pops) => usize::from(pops),
                None if *op == OP_POW => 2,
                None => 1,
            };
            (pops, 1)
        }
        Instr::StoreShared(_) => (1, 0),
        // to, spec code, and two spec parameters.
        Instr::StoreAnim(_) => (4, 0),
        Instr::CancelAnim(_) => (0, 0),
        Instr::StoreLocal(_) => (1, 0),
        Instr::Pop => (1, 0),
        Instr::Call(_, argc) => (*argc as usize, 1),
        Instr::Return => (1, 0),
        Instr::ScheduleJs(_, argc) => (*argc as usize, 0),
        Instr::Jump(_) => (0, 0),
        Instr::JumpIfFalse(_) => (1, 0),
    }
}

impl UiProgram {
    /// Validate constants, operands, captures, and control flow. Runs once
    /// when the defining op applies; a failure fails that op batch.
    pub fn validate(&self) -> Result<(), String> {
        if self.code.is_empty() {
            return Err("ui program is empty".into());
        }
        if self.consts.len() > 255 {
            return Err("ui program has more than 255 constants".into());
        }
        for value in &self.consts {
            if let ExprConst::Num(value) = value {
                if !value.is_finite() {
                    return Err("ui program constants must be finite".into());
                }
            }
        }
        let mut instrs: Vec<(usize, usize, Instr)> = Vec::with_capacity(self.code.len() / 2 + 1);
        let mut cursor = 0;
        while cursor < self.code.len() {
            let offset = cursor;
            let instr = decode_one(self.code.as_slice(), &mut cursor)?;
            instrs.push((offset, cursor, instr));
        }
        let starts: HashSet<usize> = instrs.iter().map(|(start, _, _)| *start).collect();
        for (_, _, instr) in &instrs {
            if let Instr::Jump(target) | Instr::JumpIfFalse(target) = instr {
                if *target as usize >= self.code.len() || !starts.contains(&(*target as usize)) {
                    return Err(format!("jump target {target} is not an instruction"));
                }
            }
        }
        // Abstract dataflow over stack height: proves no underflow at any
        // reachable point and bounds the live stack. Heights only grow, so
        // the fixpoint terminates.
        let mut heights: Vec<Option<usize>> = vec![None; self.code.len()];
        let mut queue: std::collections::VecDeque<(usize, usize)> =
            std::collections::VecDeque::new();
        heights[0] = Some(0);
        queue.push_back((0, 0));
        let mut reachable_return = false;
        while let Some((offset, height)) = queue.pop_front() {
            let Some((_, end, instr)) = instrs.iter().find(|(start, _, _)| *start == offset) else {
                continue;
            };
            let (pops, pushes) = effect_of(instr);
            if pops > height {
                return Err("ui program underflows its stack".into());
            }
            let height = height - pops + pushes;
            if height > STACK_LIMIT {
                return Err("ui program stack exceeds 32 slots".into());
            }
            let next = |heights: &mut Vec<Option<usize>>,
                        queue: &mut std::collections::VecDeque<(usize, usize)>,
                        target: usize,
                        height: usize| {
                if let Some(slot) = heights.get_mut(target) {
                    if slot.is_none_or(|seen| seen < height) {
                        *slot = Some(height);
                        queue.push_back((target, height));
                    }
                }
            };
            match instr {
                Instr::Return => reachable_return = true,
                Instr::Jump(target) => next(&mut heights, &mut queue, *target as usize, height),
                Instr::JumpIfFalse(target) => {
                    next(&mut heights, &mut queue, *target as usize, height);
                    next(&mut heights, &mut queue, *end, height);
                }
                _ => next(&mut heights, &mut queue, *end, height),
            }
        }
        if !reachable_return {
            return Err("ui program has no reachable return".into());
        }
        for (_, _, instr) in &instrs {
            match instr {
                Instr::PushConst(index) => {
                    if *index as usize >= self.consts.len() {
                        return Err("ui program constant index out of range".into());
                    }
                }
                Instr::PushParam(index) => {
                    if *index >= self.params {
                        return Err(format!("ui program param {index} is out of range"));
                    }
                }
                Instr::PushCapture(index) => {
                    let Some(capture) = self.captures.get(*index as usize) else {
                        return Err("ui program capture index out of range".into());
                    };
                    if !matches!(
                        capture,
                        UiCapture::Num(_)
                            | UiCapture::Color(_)
                            | UiCapture::Bool(_)
                            | UiCapture::Shared(_)
                            | UiCapture::Text(_)
                    ) {
                        return Err("only shared values, strings, and primitives can be pushed".into());
                    }
                }
                Instr::StoreShared(index) | Instr::StoreAnim(index) | Instr::CancelAnim(index) => {
                    let Some(capture) = self.captures.get(*index as usize) else {
                        return Err("ui program capture index out of range".into());
                    };
                    if !matches!(capture, UiCapture::Shared(_)) {
                        return Err("a shared store target must be a shared value".into());
                    }
                }
                Instr::Call(index, _) => {
                    let Some(capture) = self.captures.get(*index as usize) else {
                        return Err("ui program capture index out of range".into());
                    };
                    if !matches!(capture, UiCapture::Function(_)) {
                        return Err("a call target must be a worklet".into());
                    }
                }
                Instr::ScheduleJs(index, _) => {
                    let Some(capture) = self.captures.get(*index as usize) else {
                        return Err("ui program capture index out of range".into());
                    };
                    if !matches!(capture, UiCapture::JsCallback(_)) {
                        return Err("scheduleOnJS target must be a JS handler".into());
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Event fields for `LOAD_EVENT`.
pub const EVENT_X: u8 = 0;
pub const EVENT_Y: u8 = 1;
pub const EVENT_DELTA_X: u8 = 2;
pub const EVENT_DELTA_Y: u8 = 3;
pub const EVENT_TRANSLATION_X: u8 = 4;
pub const EVENT_TRANSLATION_Y: u8 = 5;
pub const EVENT_SCALE: u8 = 6;
pub const EVENT_ROTATION: u8 = 7;
pub const EVENT_VELOCITY_X: u8 = 8;
pub const EVENT_VELOCITY_Y: u8 = 9;
pub const EVENT_TIME: u8 = 10;
pub const EVENT_SEQUENCE: u8 = 11;
pub const EVENT_REASON: u8 = 12;
pub const EVENT_TARGET: u8 = 13;

/// Store target kind for a shared write.
enum Store {
    Immediate,
    Anim(AnimSpec),
}

impl crate::shell::Shell {
    /// Queue one JS-initiated UI call. Applied by the next `drain_ui_work`.
    pub(crate) fn queue_ui_call(&mut self, call: UiCall) {
        self.ui_calls.push_back(call);
    }

    /// JS handler notifications queued by `scheduleOnJS`.
    pub fn take_js_calls(&mut self) -> Vec<UiCall> {
        self.js_calls.drain(..).collect()
    }

    /// Whether timers, animations, queued calls, or pending gestures still
    /// need frame ticks.
    pub fn pending(&self) -> bool {
        self.recognizer.pending()
            || !self.ui_calls.is_empty()
            || !self.js_calls.is_empty()
            || self.vars.any_animating()
    }

    /// Faults recorded since the last drain, plus the newest message.
    pub fn take_ui_faults(&mut self) -> (u32, String) {
        let faults = std::mem::take(&mut self.ui_faults);
        let error = std::mem::take(&mut self.ui_last_error);
        (faults, error)
    }

    /// Run one UI program with `args`. A fault records the message, bumps
    /// `ui_faults`, and aborts only this call.
    pub(crate) fn run_ui_function(
        &mut self,
        id: u32,
        args: &[ExprValue],
    ) -> Result<ExprValue, String> {
        if self.ui_stack.capacity() < STACK_LIMIT {
            self.ui_stack.reserve(STACK_LIMIT);
        }
        if args.len() > 255 {
            return Err(self.fault(format!("ui call {id} has too many arguments")));
        }
        let base = self.ui_stack.len();
        if base + args.len() + LOCALS > STACK_TOTAL {
            return Err(self.fault("ui stack limit reached"));
        }
        for arg in args {
            if matches!(arg, ExprValue::Num(value) if !value.is_finite()) {
                return Err(self.fault(format!("ui call {id} argument is not finite")));
            }
        }
        self.ui_stack.extend(args.iter().map(|arg| UiValue::from_expr(*arg)));
        let locals_start = self.ui_stack.len();
        self.ui_stack.resize(locals_start + LOCALS, UiValue::Num(0.0));
        let result = self.run_frame(id, args.len() as u8, base, locals_start, 0);
        self.ui_stack.truncate(base);
        result
    }

    fn fault(&mut self, message: impl Into<String>) -> String {
        let message = message.into();
        self.ui_faults += 1;
        self.ui_last_error = message.clone();
        message
    }

    /// One call frame: `base` points at the args, `locals_start` at the
    /// local slots. The return value replaces the args at `base`.
    fn run_frame(
        &mut self,
        id: u32,
        params: u8,
        base: usize,
        locals_start: usize,
        depth: usize,
    ) -> Result<ExprValue, String> {
        if depth >= CALL_DEPTH {
            return Err(self.fault("ui call depth exceeded"));
        }
        let Some(program) = self.ui_programs.get(id as usize).and_then(Option::as_ref) else {
            return Err(self.fault(format!("unknown ui function {id}")));
        };
        if program.params != params {
            return Err(self.fault(format!(
                "ui function {id} expects {} arguments, got {params}",
                program.params
            )));
        }
        let program = program.clone();
        let mut cursor = 0usize;
        let code = program.code.as_slice();
        let mut stores = 0usize;
        macro_rules! pop {
            () => {{
                let top = self.ui_stack.len().saturating_sub(1);
                if top < base {
                    return Err(self.fault("stack underflow"));
                }
                let value = self.ui_stack[top].clone();
                self.ui_stack.truncate(top);
                value
            }};
        }
        while cursor < code.len() {
            let instr = match decode_one(code, &mut cursor) {
                Ok(instr) => instr,
                Err(message) => return Err(self.fault(message)),
            };
            match instr {
                Instr::PushConst(index) => {
                    let Some(constant) = program.consts.get(index as usize) else {
                        return Err(self.fault("constant index out of range"));
                    };
                    let value = match constant {
                        ExprConst::Num(value) => UiValue::Num(*value),
                        ExprConst::Color(value) => UiValue::Color(*value),
                        ExprConst::Bool(value) => UiValue::Bool(*value),
                        ExprConst::ColorString(text) => UiValue::Color(
                            crate::color::parse(text)
                                .map_err(|error| self.fault(format!("bad color constant: {error}")))?,
                        ),
                    };
                    self.ui_stack.push(value);
                }
                Instr::PushParam(index) => {
                    if index >= params {
                        return Err(self.fault(format!("param {index} is out of range")));
                    }
                    let pushed = self.ui_stack.get(base + index as usize).cloned();
                    self.ui_stack.push(pushed.unwrap_or(UiValue::Num(0.0)));
                }
                Instr::PushCapture(index) => {
                    let Some(capture) = program.captures.get(index as usize) else {
                        return Err(self.fault("capture index out of range"));
                    };
                    let pushed = match capture {
                        UiCapture::Num(value) => UiValue::Num(*value),
                        UiCapture::Color(value) => UiValue::Color(*value),
                        UiCapture::Bool(value) => UiValue::Bool(*value),
                        UiCapture::Text(value) => UiValue::Text(value.clone()),
                        UiCapture::Shared(var) => {
                            let value = self
                                .vars
                                .get(*var)
                                .map(|var| var.value)
                                .ok_or_else(|| self.fault(format!("unknown shared value {var}")))?;
                            UiValue::from_expr(value)
                        }
                        UiCapture::Function(_) | UiCapture::JsCallback(_) => {
                            return Err(self.fault("capture cannot be pushed"))
                        }
                    };
                    self.ui_stack.push(pushed);
                }
                Instr::LoadEvent(field) => {
                    let Some(event) = self.ui_event.as_ref() else {
                        return Err(self.fault("no event context in this call"));
                    };
                    let pushed = match field {
                        EVENT_X => event.x,
                        EVENT_Y => event.y,
                        EVENT_DELTA_X => event.delta_x,
                        EVENT_DELTA_Y => event.delta_y,
                        EVENT_TRANSLATION_X => event.translation_x,
                        EVENT_TRANSLATION_Y => event.translation_y,
                        EVENT_SCALE => event.scale,
                        EVENT_ROTATION => event.rotation,
                        EVENT_VELOCITY_X => event.velocity_x,
                        EVENT_VELOCITY_Y => event.velocity_y,
                        EVENT_TIME => event.time_ms as f32,
                        EVENT_SEQUENCE => event.sequence as f32,
                        EVENT_REASON => event.reason as f32,
                        EVENT_TARGET => event.target as f32,
                        other => return Err(self.fault(format!("unknown event field {other}"))),
                    };
                    if !pushed.is_finite() {
                        return Err(self.fault("event field is not finite"));
                    }
                    self.ui_stack.push(UiValue::Num(pushed));
                }
                Instr::StoreShared(index) => {
                    stores += 1;
                    if stores > STORES_PER_CALL {
                        return Err(self.fault("too many shared stores in one call"));
                    }
                    let to = pop!();
                    let target = shared_target(&program, index);
                    self.store_shared(target, to, Store::Immediate)?;
                }
                Instr::StoreAnim(index) => {
                    stores += 1;
                    if stores > STORES_PER_CALL {
                        return Err(self.fault("too many shared stores in one call"));
                    }
                    // Stack layout, top first: to, spec code, then two spec
                    // parameters. The compiler pushes parameters first.
                    let to = pop!();
                    let code_value = pop!();
                    let spec = match code_value {
                        UiValue::Num(0.0) => {
                            let easing = pop!();
                            let duration = pop!();
                            let (UiValue::Num(easing), UiValue::Num(duration)) = (easing, duration)
                            else {
                                return Err(self.fault("tween spec expects numbers"));
                            };
                            if easing != f32::from(EASE_LINEAR) && easing != 1.0 {
                                return Err(self.fault(format!("unknown easing code {easing}")));
                            }
                            if !(duration.is_finite() && duration > 0.0) {
                                return Err(self.fault("tween duration must be positive"));
                            }
                            AnimSpec::Tween {
                                duration_ms: duration,
                                easing: easing as u8,
                            }
                        }
                        UiValue::Num(1.0) => {
                            let damping = pop!();
                            let stiffness = pop!();
                            let (UiValue::Num(stiffness), UiValue::Num(damping)) =
                                (stiffness, damping)
                            else {
                                return Err(self.fault("spring spec expects numbers"));
                            };
                            if !(stiffness.is_finite() && stiffness > 0.0) {
                                return Err(self.fault("spring stiffness must be positive"));
                            }
                            if !(damping.is_finite() && damping >= 0.0) {
                                return Err(self.fault("spring damping must not be negative"));
                            }
                            AnimSpec::Spring { stiffness, damping }
                        }
                        other => {
                            return Err(self.fault(format!("unknown anim spec code {other:?}")))
                        }
                    };
                    let target = shared_target(&program, index);
                    self.store_shared(target, to, Store::Anim(spec))?;
                }
                Instr::CancelAnim(index) => {
                    let Some(capture) = program.captures.get(index as usize) else {
                        return Err(self.fault("capture index out of range"));
                    };
                    let UiCapture::Shared(var) = capture else {
                        return Err(self.fault("cancelAnimation target must be a shared value"));
                    };
                    if let Some(var_slot) = self.vars.get_mut(*var) {
                        var_slot.anim_state = None;
                    } else {
                        return Err(self.fault(format!("unknown shared value {var}")));
                    }
                }
                Instr::StoreLocal(slot) => {
                    let popped = pop!();
                    let index = locals_start + slot as usize;
                    if let Some(entry) = self.ui_stack.get_mut(index) {
                        *entry = popped;
                    } else {
                        return Err(self.fault("local slot out of range"));
                    }
                }
                Instr::LoadLocal(slot) => {
                    let index = locals_start + slot as usize;
                    let loaded = self
                        .ui_stack
                        .get(index)
                        .cloned()
                        .ok_or_else(|| self.fault("local slot out of range"))?;
                    self.ui_stack.push(loaded);
                }
                Instr::Call(index, argc) => {
                    let Some(capture) = program.captures.get(index as usize) else {
                        return Err(self.fault("capture index out of range"));
                    };
                    let UiCapture::Function(callee) = capture else {
                        return Err(self.fault("call target must be a worklet"));
                    };
                    let callee = *callee;
                    if argc as usize > self.ui_stack.len() - base {
                        return Err(self.fault("call underflows the stack"));
                    }
                    let mut call_args = Vec::with_capacity(argc as usize);
                    for _ in 0..argc {
                        call_args.push(pop!());
                    }
                    // Pops ran top-first; restore call order.
                    call_args.reverse();
                    let mut frame_args = Vec::with_capacity(argc as usize);
                    for arg in call_args {
                        frame_args.push(arg.as_expr().map_err(|error| self.fault(error))?);
                    }
                    if self.ui_stack.len() + frame_args.len() + LOCALS > STACK_TOTAL {
                        return Err(self.fault("ui stack limit reached"));
                    }
                    let frame_base = self.ui_stack.len();
                    for arg in frame_args {
                        self.ui_stack.push(UiValue::from_expr(arg));
                    }
                    let frame_locals = self.ui_stack.len();
                    self.ui_stack.resize(frame_locals + LOCALS, UiValue::Num(0.0));
                    // The caller's argc is the truth: a callee that declares
                    // another arity faults instead of reading local slots.
                    let returned =
                        self.run_frame(callee, argc, frame_base, frame_locals, depth + 1)?;
                    self.ui_stack.truncate(frame_base);
                    self.ui_stack.push(UiValue::from_expr(returned));
                }
                Instr::Return => {
                    if self.ui_stack.len() <= base {
                        return Err(self.fault("return underflows the stack"));
                    }
                    let returned = pop!();
                    return returned.as_expr().map_err(|error| self.fault(error));
                }
                Instr::ScheduleJs(index, argc) => {
                    let Some(capture) = program.captures.get(index as usize) else {
                        return Err(self.fault("capture index out of range"));
                    };
                    let UiCapture::JsCallback(callback) = capture else {
                        return Err(self.fault("scheduleOnJS target must be a JS handler"));
                    };
                    if argc as usize > self.ui_stack.len() - base {
                        return Err(self.fault("scheduleOnJS underflows the stack"));
                    }
                    let mut call_args = Vec::with_capacity(argc as usize);
                    for _ in 0..argc {
                        call_args.push(pop!());
                    }
                    // Pops ran top-first; restore call order.
                    call_args.reverse();
                    self.js_calls.push_back(UiCall {
                        function: *callback,
                        args: call_args,
                    });
                }
                Instr::Jump(target) => {
                    cursor = target as usize;
                }
                Instr::Pop => {
                    let _ = pop!();
                }
                Instr::JumpIfFalse(target) => {
                    let condition = pop!();
                    let UiValue::Bool(condition) = condition else {
                        return Err(self.fault("a branch condition must be a bool"));
                    };
                    if !condition {
                        cursor = target as usize;
                    }
                }
                Instr::Op(op) => self.eval_op(op, base)?,
            }
            if self.ui_stack.len() > base + usize::from(params) + LOCALS + STACK_LIMIT {
                return Err(self.fault("ui stack limit reached"));
            }
        }
        // No explicit return: the value is 0.
        Ok(ExprValue::Num(0.0))
    }

    /// Shared-value write shared by STORE_SHARED and STORE_ANIM. A
    /// source-driven target records a fault and ends the call.
    fn store_shared(&mut self, target: Option<u32>, to: UiValue, store: Store) -> Result<(), String> {
        let Some(var) = target else {
            return Err(self.fault("store target is not a shared value"));
        };
        let to = match to.as_expr() {
            Ok(value) => value,
            Err(error) => return Err(self.fault(error)),
        };
        if let ExprValue::Num(value) = to {
            if !value.is_finite() {
                return Err(self.fault("stored value is not finite"));
            }
        }
        let Some(var_slot) = self.vars.get_mut(var) else {
            return Err(self.fault(format!("unknown shared value {var}")));
        };
        if var_slot.source != crate::state_style::SOURCE_NONE {
            let error = format!("shared value {var} is source-driven and cannot be written");
            return Err(self.fault(error));
        }
        if var_slot.value == to {
            var_slot.anim_state = None;
            return Ok(());
        }
        match store {
            Store::Immediate => {
                var_slot.anim_state = None;
                var_slot.value = to;
            }
            Store::Anim(spec) => {
                let from = var_slot.value.clone();
                var_slot.anim_state = Some(AnimState::new(spec, from, to, self.now_ms));
            }
        }
        self.mark_var_dirty(var);
        Ok(())
    }

    /// One arithmetic, comparison, or logic op over the shared stack. The
    /// result is pushed in place of its operands.
    fn eval_op(&mut self, op: u8, base: usize) -> Result<(), String> {
        use crate::expr::*;
        macro_rules! pop {
            ($what:expr) => {{
                let top = self.ui_stack.len().saturating_sub(1);
                if top < base {
                    return Err(self.fault("stack underflow"));
                }
                let value = self.ui_stack[top].clone();
                self.ui_stack.truncate(top);
                value
            }};
        }
        macro_rules! num {
            ($value:expr, $what:expr) => {{
                match $value {
                    UiValue::Num(value) if value.is_finite() => value,
                    other => {
                        return Err(
                            self.fault(format!("{} expects a finite number, got {other:?}", $what))
                        )
                    }
                }
            }};
        }
        macro_rules! boolean {
            ($value:expr, $what:expr) => {{
                match $value {
                    UiValue::Bool(value) => value,
                    other => return Err(self.fault(format!("{} expects a bool, got {other:?}", $what))),
                }
            }};
        }
        macro_rules! push {
            ($value:expr) => {{
                let value: UiValue = $value;
                if !value.finite() {
                    return Err(self.fault("operator produced a non-finite value"));
                }
                self.ui_stack.push(value);
            }};
        }
        match op {
            OP_ADD => {
                let (b, a) = (pop!("add"), pop!("add"));
                let (a, b) = (num!(a, "add"), num!(b, "add"));
                push!(UiValue::Num(a + b));
            }
            OP_SUB => {
                let (b, a) = (pop!("sub"), pop!("sub"));
                let (a, b) = (num!(a, "sub"), num!(b, "sub"));
                push!(UiValue::Num(a - b));
            }
            OP_MUL => {
                let (b, a) = (pop!("mul"), pop!("mul"));
                let (a, b) = (num!(a, "mul"), num!(b, "mul"));
                push!(UiValue::Num(a * b));
            }
            OP_DIV => {
                let (b, a) = (pop!("div"), pop!("div"));
                let (a, b) = (num!(a, "div"), num!(b, "div"));
                if b == 0.0 {
                    return Err(self.fault("division by zero"));
                }
                push!(UiValue::Num(a / b));
            }
            OP_NEG => {
                let a = pop!("neg");
                push!(UiValue::Num(-num!(a, "neg")));
            }
            OP_LT => {
                let (b, a) = (pop!("lt"), pop!("lt"));
                let (a, b) = (num!(a, "lt"), num!(b, "lt"));
                push!(UiValue::Bool(a < b));
            }
            OP_LE => {
                let (b, a) = (pop!("le"), pop!("le"));
                let (a, b) = (num!(a, "le"), num!(b, "le"));
                push!(UiValue::Bool(a <= b));
            }
            OP_GT => {
                let (b, a) = (pop!("gt"), pop!("gt"));
                let (a, b) = (num!(a, "gt"), num!(b, "gt"));
                push!(UiValue::Bool(a > b));
            }
            OP_GE => {
                let (b, a) = (pop!("ge"), pop!("ge"));
                let (a, b) = (num!(a, "ge"), num!(b, "ge"));
                push!(UiValue::Bool(a >= b));
            }
            OP_EQ | OP_NE => {
                let (b, a) = (pop!("eq"), pop!("eq"));
                let equal = match (a, b) {
                    (UiValue::Num(a), UiValue::Num(b)) => a == b,
                    (UiValue::Color(a), UiValue::Color(b)) => a == b,
                    (UiValue::Bool(a), UiValue::Bool(b)) => a == b,
                    (UiValue::Text(a), UiValue::Text(b)) => a == b,
                    _ => return Err(self.fault("equality compares different types")),
                };
                push!(UiValue::Bool(equal == (op == OP_EQ)));
            }
            OP_AND => {
                let (b, a) = (pop!("and"), pop!("and"));
                push!(UiValue::Bool(boolean!(a, "and") && boolean!(b, "and")));
            }
            OP_OR => {
                let (b, a) = (pop!("or"), pop!("or"));
                push!(UiValue::Bool(boolean!(a, "or") || boolean!(b, "or")));
            }
            OP_NOT => {
                let a = pop!("not");
                push!(UiValue::Bool(!boolean!(a, "not")));
            }
            OP_SELECT => {
                let (b, a) = (pop!("select"), pop!("select"));
                let condition = pop!("select");
                let condition = boolean!(condition, "select");
                if std::mem::discriminant(&a) != std::mem::discriminant(&b) {
                    return Err(self.fault("select branches have different types"));
                }
                push!(if condition { a } else { b });
            }
            OP_MIX => {
                let t = num!(pop!("mix"), "mix");
                let (b, a) = (pop!("mix"), pop!("mix"));
                let pushed = match (&a, &b) {
                    (UiValue::Num(a), UiValue::Num(b)) => UiValue::Num(a + (b - a) * t),
                    (UiValue::Color(a), UiValue::Color(b)) => UiValue::Color(a.mix(b, t)),
                    _ => return Err(self.fault("mix interpolates two numbers or two colors")),
                };
                push!(pushed);
            }
            OP_LERP => {
                let (t, b, a) = (pop!("lerp"), pop!("lerp"), pop!("lerp"));
                let (a, b, t) = (num!(a, "lerp"), num!(b, "lerp"), num!(t, "lerp"));
                push!(UiValue::Num(a + (b - a) * t));
            }
            OP_CLAMP => {
                let (hi, lo, v) = (pop!("clamp"), pop!("clamp"), pop!("clamp"));
                let (v, lo, hi) = (num!(v, "clamp"), num!(lo, "clamp"), num!(hi, "clamp"));
                if hi < lo {
                    return Err(self.fault("clamp low bound exceeds high bound"));
                }
                push!(UiValue::Num(v.clamp(lo, hi)));
            }
            OP_MIN | OP_MAX => {
                let (b, a) = (pop!("min"), pop!("min"));
                let (a, b) = (num!(a, "min"), num!(b, "min"));
                push!(UiValue::Num(if op == OP_MIN { a.min(b) } else { a.max(b) }));
            }
            OP_ABS => {
                let a = pop!("abs");
                push!(UiValue::Num(num!(a, "abs").abs()));
            }
            OP_FLOOR => {
                let a = pop!("floor");
                push!(UiValue::Num(num!(a, "floor").floor()));
            }
            OP_CEIL => {
                let a = pop!("ceil");
                push!(UiValue::Num(num!(a, "ceil").ceil()));
            }
            OP_ROUND => {
                let a = pop!("round");
                push!(UiValue::Num(num!(a, "round").round()));
            }
            OP_SQRT => {
                let a = pop!("sqrt");
                let value = num!(a, "sqrt");
                if value < 0.0 {
                    return Err(self.fault("sqrt expects a non-negative number"));
                }
                push!(UiValue::Num(value.sqrt()));
            }
            OP_POW => {
                let (b, a) = (pop!("pow"), pop!("pow"));
                let (a, b) = (num!(a, "pow"), num!(b, "pow"));
                push!(UiValue::Num(a.powf(b)));
            }
            other => return Err(self.fault(format!("unknown program opcode {other}"))),
        }
        Ok(())
    }
}

fn shared_target(program: &UiProgram, index: u16) -> Option<u32> {
    match program.captures.get(index as usize) {
        Some(UiCapture::Shared(var)) => Some(*var),
        _ => None,
    }
}
