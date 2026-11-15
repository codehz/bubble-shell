//! Shared-variable expression engine.
//!
//! The compiler turns style-declaration expressions (a `when` predicate and
//! patch values) into compact stack programs. A program reads variables from
//! the shell's [`VarTable`](crate::state_style::VarTable) and produces one
//! small tagged value: `Num`, `Color`, or `Bool`. Strings never enter the
//! engine: color strings are resolved at decode, and enum names (textAlign /
//! fit) are mapped to their numeric codes by the runtime before encoding.
//!
//! Validation is static per program: one abstract interpretation over the
//! type lattice `N` (number), `C` (color), `B` (bool) proves the stack shape
//! and the slot type. Arithmetic never produces non-finite values; division by
//! zero is an evaluation error instead of a NaN.

use crate::color::Color;

/// Opcode set. Instruction operands: `Const` reads a byte index into the
/// constant table, `Load` reads a u32 variable id, `Ease` reads a u8 easing
/// selector; every other instruction takes no operands.
pub const OP_CONST: u8 = 0;
pub const OP_LOAD: u8 = 1;
pub const OP_ADD: u8 = 2;
pub const OP_SUB: u8 = 3;
pub const OP_MUL: u8 = 4;
pub const OP_DIV: u8 = 5;
pub const OP_NEG: u8 = 6;
pub const OP_LT: u8 = 7;
pub const OP_LE: u8 = 8;
pub const OP_GT: u8 = 9;
pub const OP_GE: u8 = 10;
pub const OP_EQ: u8 = 11;
pub const OP_NE: u8 = 12;
pub const OP_AND: u8 = 13;
pub const OP_OR: u8 = 14;
pub const OP_NOT: u8 = 15;
pub const OP_SELECT: u8 = 16;
pub const OP_MIX: u8 = 17;
pub const OP_LERP: u8 = 18;
pub const OP_CLAMP: u8 = 19;
pub const OP_MIN: u8 = 20;
pub const OP_MAX: u8 = 21;
pub const OP_ABS: u8 = 22;
/// Easing selector `0` is linear; more curves arrive with the animation stage.
pub const OP_EASE: u8 = 23;

/// One engine value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ExprValue {
    Num(f32),
    Color(Color),
    Bool(bool),
}

impl ExprValue {
    fn kind(&self) -> Type {
        match self {
            ExprValue::Num(_) => Type::N,
            ExprValue::Color(_) => Type::C,
            ExprValue::Bool(_) => Type::B,
        }
    }
}

/// Constant table entry. `ColorString` is CSS parsed once at decode time.
#[derive(Clone, Debug, PartialEq)]
pub enum ExprConst {
    Num(f32),
    Color(Color),
    Bool(bool),
    ColorString(String),
}

/// A compiled program: constants plus stack instructions.
#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    consts: Vec<ExprConst>,
    code: Vec<Instr>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Instr {
    Const(u8),
    Load(u32),
    Op(u8),
    Ease(u8),
}

/// Type lattice for the static check. Exposed because variable types feed
/// the static validation from outside the module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    N,
    C,
    B,
}

#[derive(Clone, Debug)]
pub struct ProgramError(pub String);

impl std::fmt::Display for ProgramError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn err<T>(message: impl Into<String>) -> Result<T, ProgramError> {
    Err(ProgramError(message.into()))
}

/// Per-opcode stack and type rule. One table is the reference for arity and
/// result types: `result_type` validates against it, so the static check and
/// the runtime instruction set cannot silently diverge when opcodes change.
/// `eval` keeps per-opcode compute arms but must mirror each entry's pops.
#[derive(Clone, Copy)]
struct OpSpec {
    what: &'static str,
    /// Stack inputs consumed, in pop order.
    pops: u8,
    /// Cross-operand type rule.
    rule: OpRule,
}

#[derive(Clone, Copy)]
enum OpRule {
    /// Every pop must be `operand`; pushes `result`.
    Fixed { operand: Type, result: Type },
    /// Two pops of the same type; pushes bool (EQ, NE).
    Equality,
    /// Two same-type branch pops under a bool condition; pushes the branch
    /// type (SELECT).
    Branches,
    /// A numeric t over two same-type number-or-color pops; pushes their type
    /// (MIX).
    Mix,
}

const OP_SPECS: &[OpSpec] = &[
    OpSpec { what: "arithmetic", pops: 2, rule: OpRule::Fixed { operand: Type::N, result: Type::N } }, // OP_ADD
    OpSpec { what: "arithmetic", pops: 2, rule: OpRule::Fixed { operand: Type::N, result: Type::N } }, // OP_SUB
    OpSpec { what: "arithmetic", pops: 2, rule: OpRule::Fixed { operand: Type::N, result: Type::N } }, // OP_MUL
    OpSpec { what: "arithmetic", pops: 2, rule: OpRule::Fixed { operand: Type::N, result: Type::N } }, // OP_DIV
    OpSpec { what: "neg", pops: 1, rule: OpRule::Fixed { operand: Type::N, result: Type::N } }, // OP_NEG
    OpSpec { what: "comparison", pops: 2, rule: OpRule::Fixed { operand: Type::N, result: Type::B } }, // OP_LT
    OpSpec { what: "comparison", pops: 2, rule: OpRule::Fixed { operand: Type::N, result: Type::B } }, // OP_LE
    OpSpec { what: "comparison", pops: 2, rule: OpRule::Fixed { operand: Type::N, result: Type::B } }, // OP_GT
    OpSpec { what: "comparison", pops: 2, rule: OpRule::Fixed { operand: Type::N, result: Type::B } }, // OP_GE
    OpSpec { what: "equality", pops: 2, rule: OpRule::Equality }, // OP_EQ
    OpSpec { what: "equality", pops: 2, rule: OpRule::Equality }, // OP_NE
    OpSpec { what: "logical", pops: 2, rule: OpRule::Fixed { operand: Type::B, result: Type::B } }, // OP_AND
    OpSpec { what: "logical", pops: 2, rule: OpRule::Fixed { operand: Type::B, result: Type::B } }, // OP_OR
    OpSpec { what: "not", pops: 1, rule: OpRule::Fixed { operand: Type::B, result: Type::B } }, // OP_NOT
    OpSpec { what: "select", pops: 3, rule: OpRule::Branches }, // OP_SELECT
    OpSpec { what: "mix", pops: 3, rule: OpRule::Mix }, // OP_MIX
    OpSpec { what: "lerp", pops: 3, rule: OpRule::Fixed { operand: Type::N, result: Type::N } }, // OP_LERP
    OpSpec { what: "lerp", pops: 3, rule: OpRule::Fixed { operand: Type::N, result: Type::N } }, // OP_CLAMP
    OpSpec { what: "min", pops: 2, rule: OpRule::Fixed { operand: Type::N, result: Type::N } }, // OP_MIN
    OpSpec { what: "min", pops: 2, rule: OpRule::Fixed { operand: Type::N, result: Type::N } }, // OP_MAX
    OpSpec { what: "neg", pops: 1, rule: OpRule::Fixed { operand: Type::N, result: Type::N } }, // OP_ABS
];

/// Opcodes `OP_ADD..=OP_MAX` are contiguous and table order matches.
fn op_spec(op: u8) -> Result<&'static OpSpec, ProgramError> {
    OP_SPECS
        .get(op.wrapping_sub(OP_ADD) as usize)
        .ok_or_else(|| ProgramError(format!("unknown program opcode {op}")))
}

impl Program {
    /// Validate constants and instruction stream. Callers then prove the
    /// slot type with [`Self::result_type`], passing the types of the
    /// variables the program may load.
    pub fn new(consts: Vec<ExprConst>, code: &[u8]) -> Result<Self, ProgramError> {
        if consts.len() > 256 {
            return err("program has more than 256 constants");
        }
        if code.is_empty() {
            return err("program is empty");
        }
        for value in &consts {
            if let ExprConst::Num(value) = value {
                if !value.is_finite() {
                    return err("program constants must be finite");
                }
            }
        }
        let mut instrs = Vec::with_capacity(code.len() / 2 + 1);
        let mut cursor = 0;
        while cursor < code.len() {
            let op = code[cursor];
            cursor += 1;
            let instr = match op {
                OP_CONST => {
                    let index = *code
                        .get(cursor)
                        .ok_or_else(|| ProgramError("truncated program".into()))?;
                    cursor += 1;
                    if index as usize >= consts.len() {
                        return err("constant index out of range");
                    }
                    Instr::Const(index)
                }
                OP_LOAD => {
                    if cursor + 4 > code.len() {
                        return err("truncated program");
                    }
                    let id = u32::from_le_bytes([
                        code[cursor],
                        code[cursor + 1],
                        code[cursor + 2],
                        code[cursor + 3],
                    ]);
                    cursor += 4;
                    Instr::Load(id)
                }
                OP_EASE => {
                    let selector = *code
                        .get(cursor)
                        .ok_or_else(|| ProgramError("truncated program".into()))?;
                    cursor += 1;
                    Instr::Ease(selector)
                }
                2..=OP_MAX => Instr::Op(op),
                other => return err(format!("unknown program opcode {other}")),
            };
            instrs.push(instr);
        }
        Ok(Self {
            consts,
            code: instrs,
        })
    }

    /// Variable ids the program loads, deduplicated.
    pub fn used_vars(&self) -> Vec<u32> {
        let mut out = Vec::new();
        for instr in &self.code {
            if let Instr::Load(id) = instr {
                if !out.contains(id) {
                    out.push(*id);
                }
            }
        }
        out
    }

    /// Static type of the program result under the given variable types.
    pub fn result_type(
        &self,
        var_types: &dyn Fn(u32) -> Option<Type>,
    ) -> Result<Type, ProgramError> {
        let mut stack: Vec<Type> = Vec::new();
        let push = |stack: &mut Vec<Type>, kind: Type| -> Result<(), ProgramError> {
            stack.push(kind);
            Ok(())
        };
        let pop_as = |stack: &mut Vec<Type>, want: Type, what: &str| -> Result<(), ProgramError> {
            let got = stack
                .pop()
                .ok_or_else(|| ProgramError(format!("{what} underflows the stack")))?;
            if got != want {
                return err(format!("{what} expects {want:?}, got {got:?}"));
            }
            Ok(())
        };
        for instr in &self.code {
            match instr {
                Instr::Const(index) => {
                    let kind = match &self.consts[*index as usize] {
                        ExprConst::Num(_) => Type::N,
                        ExprConst::Color(_) | ExprConst::ColorString(_) => Type::C,
                        ExprConst::Bool(_) => Type::B,
                    };
                    push(&mut stack, kind)?;
                }
                Instr::Load(id) => {
                    let kind = var_types(*id)
                        .ok_or_else(|| ProgramError(format!("program loads unknown var {id}")))?;
                    push(&mut stack, kind)?;
                }
                Instr::Ease(_) => {
                    pop_as(&mut stack, Type::N, "ease")?;
                    push(&mut stack, Type::N)?;
                }
                Instr::Op(op) => {
                    let spec = op_spec(*op)?;
                    let mut popped: Vec<Type> = Vec::with_capacity(spec.pops as usize);
                    for _ in 0..spec.pops {
                        let got = stack
                            .pop()
                            .ok_or_else(|| ProgramError(format!("{} underflows the stack", spec.what)))?;
                        popped.push(got);
                    }
                    match spec.rule {
                        OpRule::Fixed { operand, result } => {
                            for got in &popped {
                                if *got != operand {
                                    return err(format!(
                                        "{} expects {operand:?}, got {got:?}",
                                        spec.what
                                    ));
                                }
                            }
                            stack.push(result);
                        }
                        OpRule::Equality => {
                            if popped[0] != popped[1] {
                                return err("equality compares different types");
                            }
                            stack.push(Type::B);
                        }
                        OpRule::Branches => {
                            // Pop order: branch b, branch a, condition.
                            if popped[2] != Type::B {
                                return err(format!(
                                    "{} expects {:?}, got {:?}",
                                    spec.what,
                                    Type::B,
                                    popped[2]
                                ));
                            }
                            if popped[0] != popped[1] {
                                return err("select branches have different types");
                            }
                            stack.push(popped[1]);
                        }
                        OpRule::Mix => {
                            // Pop order: t, b, a.
                            if popped[0] != Type::N {
                                return err(format!(
                                    "{} expects {:?}, got {:?}",
                                    spec.what,
                                    Type::N,
                                    popped[0]
                                ));
                            }
                            if popped[1] != popped[2]
                                || (popped[1] != Type::N && popped[1] != Type::C)
                            {
                                return err("mix interpolates two numbers or two colors");
                            }
                            stack.push(popped[2]);
                        }
                    }
                }
            }
        }
        if stack.len() != 1 {
            return err("program does not leave exactly one value");
        }
        Ok(stack[0])
    }

    /// Evaluate against current variable values. Errors fail the op that
    /// carried the program; later re-evaluations treat an error as "no match".
    pub fn eval(&self, vars: &dyn Fn(u32) -> Option<ExprValue>) -> Result<ExprValue, ProgramError> {
        let mut stack: Vec<ExprValue> = Vec::new();
        let pop = |stack: &mut Vec<ExprValue>| -> Result<ExprValue, ProgramError> {
            stack
                .pop()
                .ok_or_else(|| ProgramError("stack underflow".into()))
        };
        let num = |value: ExprValue, what: &str| -> Result<f32, ProgramError> {
            match value {
                ExprValue::Num(value) if value.is_finite() => Ok(value),
                _ => Err(ProgramError(format!("{what} expects a finite number"))),
            }
        };
        let color = |value: ExprValue, what: &str| -> Result<Color, ProgramError> {
            match value {
                ExprValue::Color(value) => Ok(value),
                _ => Err(ProgramError(format!("{what} expects a color"))),
            }
        };
        let boolean = |value: ExprValue, what: &str| -> Result<bool, ProgramError> {
            match value {
                ExprValue::Bool(value) => Ok(value),
                _ => Err(ProgramError(format!("{what} expects a bool"))),
            }
        };
        for instr in &self.code {
            match instr {
                Instr::Const(index) => {
                    let value = match &self.consts[*index as usize] {
                        ExprConst::Num(value) => ExprValue::Num(*value),
                        ExprConst::Color(value) => ExprValue::Color(*value),
                        ExprConst::Bool(value) => ExprValue::Bool(*value),
                        ExprConst::ColorString(text) => ExprValue::Color(
                            crate::color::parse(text).map_err(|e| ProgramError(e))?,
                        ),
                    };
                    stack.push(value);
                }
                Instr::Load(id) => {
                    let value = vars(*id)
                        .ok_or_else(|| ProgramError(format!("var {id} is not available")))?;
                    stack.push(value);
                }
                Instr::Ease(selector) => {
                    let t = num(pop(&mut stack)?, "ease")?;
                    let eased = match selector {
                        0 => t,
                        other => return err(format!("unknown easing selector {other}")),
                    };
                    stack.push(ExprValue::Num(eased));
                }
                Instr::Op(op) => match *op {
                    OP_ADD => {
                        let (b, a) = (pop(&mut stack)?, pop(&mut stack)?);
                        let value = num(a, "add")? + num(b, "add")?;
                        stack.push(ExprValue::Num(value));
                    }
                    OP_SUB => {
                        let (b, a) = (pop(&mut stack)?, pop(&mut stack)?);
                        stack.push(ExprValue::Num(num(a, "sub")? - num(b, "sub")?));
                    }
                    OP_MUL => {
                        let (b, a) = (pop(&mut stack)?, pop(&mut stack)?);
                        stack.push(ExprValue::Num(num(a, "mul")? * num(b, "mul")?));
                    }
                    OP_DIV => {
                        let (b, a) = (pop(&mut stack)?, pop(&mut stack)?);
                        let (a, b) = (num(a, "div")?, num(b, "div")?);
                        if b == 0.0 {
                            return err("division by zero");
                        }
                        stack.push(ExprValue::Num(a / b));
                    }
                    OP_NEG => {
                        let a = pop(&mut stack)?;
                        stack.push(ExprValue::Num(-num(a, "neg")?));
                    }
                    OP_LT | OP_LE | OP_GT | OP_GE => {
                        let (b, a) = (pop(&mut stack)?, pop(&mut stack)?);
                        let (a, b) = (num(a, "comparison")?, num(b, "comparison")?);
                        let result = match *op {
                            OP_LT => a < b,
                            OP_LE => a <= b,
                            OP_GT => a > b,
                            _ => a >= b,
                        };
                        stack.push(ExprValue::Bool(result));
                    }
                    OP_EQ | OP_NE => {
                        let (b, a) = (pop(&mut stack)?, pop(&mut stack)?);
                        let equal = match (a, b) {
                            (ExprValue::Num(a), ExprValue::Num(b)) => a == b,
                            (ExprValue::Color(a), ExprValue::Color(b)) => a == b,
                            (ExprValue::Bool(a), ExprValue::Bool(b)) => a == b,
                            _ => return err("equality compares different types"),
                        };
                        stack.push(ExprValue::Bool(equal == (*op == OP_EQ)));
                    }
                    OP_AND => {
                        let (b, a) = (pop(&mut stack)?, pop(&mut stack)?);
                        stack.push(ExprValue::Bool(boolean(a, "and")? && boolean(b, "and")?));
                    }
                    OP_OR => {
                        let (b, a) = (pop(&mut stack)?, pop(&mut stack)?);
                        stack.push(ExprValue::Bool(boolean(a, "or")? || boolean(b, "or")?));
                    }
                    OP_NOT => {
                        let a = pop(&mut stack)?;
                        stack.push(ExprValue::Bool(!boolean(a, "not")?));
                    }
                    OP_SELECT => {
                        let (b, a) = (pop(&mut stack)?, pop(&mut stack)?);
                        let cond = boolean(pop(&mut stack)?, "select")?;
                        if a.kind() != b.kind() {
                            return err("select branches have different types");
                        }
                        stack.push(if cond { a } else { b });
                    }
                    OP_MIX => {
                        let t = num(pop(&mut stack)?, "mix")?;
                        let (b, a) = (pop(&mut stack)?, pop(&mut stack)?);
                        let value = match (a.kind(), b.kind()) {
                            (Type::N, Type::N) => ExprValue::Num(
                                num(a, "mix")? + (num(b, "mix")? - num(a, "mix")?) * t,
                            ),
                            (Type::C, Type::C) => {
                                ExprValue::Color(color(a, "mix")?.mix(&color(b, "mix")?, t))
                            }
                            _ => return err("mix interpolates two numbers or two colors"),
                        };
                        stack.push(value);
                    }
                    OP_LERP => {
                        let (t, b, a) = (pop(&mut stack)?, pop(&mut stack)?, pop(&mut stack)?);
                        let (a, b, t) = (num(a, "lerp")?, num(b, "lerp")?, num(t, "lerp")?);
                        stack.push(ExprValue::Num(a + (b - a) * t));
                    }
                    OP_CLAMP => {
                        let (hi, lo, v) = (pop(&mut stack)?, pop(&mut stack)?, pop(&mut stack)?);
                        let (v, lo, hi) = (num(v, "clamp")?, num(lo, "clamp")?, num(hi, "clamp")?);
                        if hi < lo {
                            return err("clamp low bound exceeds high bound");
                        }
                        stack.push(ExprValue::Num(v.clamp(lo, hi)));
                    }
                    OP_MIN | OP_MAX => {
                        let (b, a) = (pop(&mut stack)?, pop(&mut stack)?);
                        let (a, b) = (num(a, "min")?, num(b, "min")?);
                        stack.push(ExprValue::Num(if *op == OP_MIN {
                            a.min(b)
                        } else {
                            a.max(b)
                        }));
                    }
                    OP_ABS => {
                        let a = pop(&mut stack)?;
                        stack.push(ExprValue::Num(num(a, "abs")?.abs()));
                    }
                    other => return err(format!("unknown program opcode {other}")),
                },
            }
        }
        let value = stack
            .pop()
            .ok_or_else(|| ProgramError("stack underflow".into()))?;
        if let ExprValue::Num(value) = value {
            if !value.is_finite() {
                return err("program produced a non-finite value");
            }
        }
        Ok(value)
    }

    /// Wire encoding: u8 const count, tagged constants, u16 instruction
    /// count, then instructions. Inverse of [`decode_program`].
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(3 + self.code.len() * 3);
        out.push(self.consts.len() as u8);
        for value in &self.consts {
            match value {
                ExprConst::Num(value) => {
                    out.push(1);
                    out.extend_from_slice(&value.to_le_bytes());
                }
                ExprConst::Color(value) => {
                    out.push(2);
                    out.extend_from_slice(&value.to_argb().to_le_bytes());
                }
                ExprConst::Bool(value) => {
                    out.push(3);
                    out.push(u8::from(*value));
                }
                ExprConst::ColorString(text) => {
                    out.push(4);
                    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
                    out.extend_from_slice(text.as_bytes());
                }
            }
        }
        out.extend_from_slice(&(self.code.len() as u16).to_le_bytes());
        for instr in &self.code {
            match instr {
                Instr::Const(index) => {
                    out.push(OP_CONST);
                    out.push(*index);
                }
                Instr::Load(id) => {
                    out.push(OP_LOAD);
                    out.extend_from_slice(&id.to_le_bytes());
                }
                Instr::Op(op) => out.push(*op),
                Instr::Ease(selector) => {
                    out.push(OP_EASE);
                    out.push(*selector);
                }
            }
        }
        out
    }
}

/// Decode a wire program. Inverse of [`Program::encode`].
pub fn decode_program(bytes: &[u8]) -> Result<Program, ProgramError> {
    let mut cursor = 0;
    let read = |cursor: &mut usize| -> Result<u8, ProgramError> {
        let byte = *bytes
            .get(*cursor)
            .ok_or_else(|| ProgramError("truncated program".into()))?;
        *cursor += 1;
        Ok(byte)
    };
    let read_u32 = |cursor: &mut usize| -> Result<u32, ProgramError> {
        if *cursor + 4 > bytes.len() {
            return err("truncated program");
        }
        let value = u32::from_le_bytes([
            bytes[*cursor],
            bytes[*cursor + 1],
            bytes[*cursor + 2],
            bytes[*cursor + 3],
        ]);
        *cursor += 4;
        Ok(value)
    };
    let const_count = read(&mut cursor)?;
    let mut consts = Vec::with_capacity(const_count as usize);
    for _ in 0..const_count {
        let tag = read(&mut cursor)?;
        let value = match tag {
            1 => {
                if cursor + 4 > bytes.len() {
                    return err("truncated program");
                }
                let mut buf = [0u8; 4];
                buf.copy_from_slice(&bytes[cursor..cursor + 4]);
                cursor += 4;
                ExprConst::Num(f32::from_le_bytes(buf))
            }
            2 => {
                if cursor + 4 > bytes.len() {
                    return err("truncated program");
                }
                let mut buf = [0u8; 4];
                buf.copy_from_slice(&bytes[cursor..cursor + 4]);
                cursor += 4;
                ExprConst::Color(Color::from_argb(u32::from_le_bytes(buf)))
            }
            3 => ExprConst::Bool(read(&mut cursor)? != 0),
            4 => {
                let len = read_u32(&mut cursor)? as usize;
                if cursor + len > bytes.len() {
                    return err("truncated program");
                }
                let text = std::str::from_utf8(&bytes[cursor..cursor + len])
                    .map_err(|_| ProgramError("program color string is not utf-8".into()))?;
                cursor += len;
                ExprConst::ColorString(text.to_owned())
            }
            other => return err(format!("unknown program const tag {other}")),
        };
        consts.push(value);
    }
    if cursor + 2 > bytes.len() {
        return err("truncated program");
    }
    let instr_count = u16::from_le_bytes([bytes[cursor], bytes[cursor + 1]]) as usize;
    cursor += 2;
    let code = bytes
        .get(cursor..)
        .ok_or_else(|| ProgramError("truncated program".into()))?;
    Program::new(consts, code).map_err(|error| {
        let mut message = error.0;
        if message.contains("truncated program") && instr_count != code.len() {
            message = format!("program expects {instr_count} instructions");
        }
        ProgramError(message)
    })
}

/// Test-facing builder so contract tests can assemble programs without
/// hand-writing bytecode.
#[derive(Default)]
pub struct ProgramBuilder {
    consts: Vec<ExprConst>,
    code: Vec<Instr>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ease {
    Linear,
}

impl ProgramBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn const_num(&mut self, value: f32) -> &mut Self {
        self.push_const(ExprConst::Num(value))
    }

    pub fn const_color(&mut self, value: Color) -> &mut Self {
        self.push_const(ExprConst::Color(value))
    }

    pub fn const_bool(&mut self, value: bool) -> &mut Self {
        self.push_const(ExprConst::Bool(value))
    }

    pub fn const_color_string(&mut self, value: &str) -> &mut Self {
        self.push_const(ExprConst::ColorString(value.to_owned()))
    }

    fn push_const(&mut self, value: ExprConst) -> &mut Self {
        if let Some(index) = self.consts.iter().position(|existing| existing == &value) {
            self.code.push(Instr::Const(index as u8));
        } else {
            self.code.push(Instr::Const(self.consts.len() as u8));
            self.consts.push(value);
        }
        self
    }

    pub fn load(&mut self, var: u32) -> &mut Self {
        self.code.push(Instr::Load(var));
        self
    }

    pub fn op(&mut self, opcode: u8) -> &mut Self {
        self.code.push(Instr::Op(opcode));
        self
    }

    pub fn ease(&mut self, easing: Ease) -> &mut Self {
        let selector = match easing {
            Ease::Linear => 0,
        };
        self.code.push(Instr::Ease(selector));
        self
    }

    pub fn build(&self) -> Result<Program, ProgramError> {
        let mut bytes = Vec::with_capacity(self.code.len() * 3);
        for instr in &self.code {
            match instr {
                Instr::Const(index) => {
                    bytes.push(OP_CONST);
                    bytes.push(*index);
                }
                Instr::Load(id) => {
                    bytes.push(OP_LOAD);
                    bytes.extend_from_slice(&id.to_le_bytes());
                }
                Instr::Op(op) => bytes.push(*op),
                Instr::Ease(selector) => {
                    bytes.push(OP_EASE);
                    bytes.push(*selector);
                }
            }
        }
        Program::new(self.consts.clone(), &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var_types(_id: u32) -> Option<Type> {
        None
    }

    #[test]
    fn rejects_stack_underflow_and_unknown_ops() {
        // ADD with one operand underflows the stack.
        let mut builder = ProgramBuilder::new();
        builder.const_num(1.0).op(OP_ADD);
        assert!(
            builder
                .build()
                .and_then(|program| program.result_type(&|_| None).map(|_| ()))
                .is_err(),
            "an underflowing add is invalid"
        );

        let mut builder = ProgramBuilder::new();
        builder.const_num(1.0).op(99);
        assert!(builder.build().is_err());

        let mut builder = ProgramBuilder::new();
        builder.const_num(1.0);
        let program = builder.build().unwrap();
        assert!(program.result_type(&var_types).is_ok());
    }

    #[test]
    fn decodes_round_trip() {
        let mut builder = ProgramBuilder::new();
        builder.const_num(0.5).load(7).op(OP_MUL);
        let program = builder.build().unwrap();
        let bytes = program.encode();
        let decoded = decode_program(&bytes).unwrap();
        assert_eq!(decoded, program);
    }
}
