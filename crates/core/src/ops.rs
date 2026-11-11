//! `BSOP` version 1. Little-endian batch with a string table.
//! `Turn` coalesces repeated `SetText` / `SetProp` before encoding.
//! Applied batches still run in order.

use std::io::{Cursor, Read};

use crate::ir::NodeDesc;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
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
    /// `template_id` selects the mounted template node's defaults.
    CreateNode {
        handle: u32,
        template_id: u32,
        parent: u32,
        index: u32,
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
    /// Instant vertical scroll. `y` is clamped to the container's range.
    ScrollTo {
        handle: u32,
        y: f32,
    },
    /// Full descriptor for a dynamic node. Appended under `desc.parent`.
    Spawn {
        desc: NodeDesc,
    },
    /// Full descriptor inserted at `index` under `desc.parent`.
    SpawnAt {
        desc: NodeDesc,
        index: u32,
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
                }
            }
            Op::SetText { handle, text } => {
                let text_id = intern(text);
                payload.push(2);
                payload.extend_from_slice(&handle.to_le_bytes());
                payload.extend_from_slice(&text_id.to_le_bytes());
            }
            Op::CreateNode {
                handle,
                template_id,
                parent,
                index,
            } => {
                payload.push(3);
                payload.extend_from_slice(&handle.to_le_bytes());
                payload.extend_from_slice(&template_id.to_le_bytes());
                payload.extend_from_slice(&parent.to_le_bytes());
                payload.extend_from_slice(&index.to_le_bytes());
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
            Op::ScrollTo { handle, y } => {
                payload.push(7);
                payload.extend_from_slice(&handle.to_le_bytes());
                payload.extend_from_slice(&y.to_le_bytes());
            }
            Op::Spawn { desc } => {
                payload.push(8);
                desc.write_body(&mut payload);
            }
            Op::SpawnAt { desc, index } => {
                payload.push(9);
                desc.write_body(&mut payload);
                payload.extend_from_slice(&index.to_le_bytes());
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
    out.extend_from_slice(&1u16.to_le_bytes());
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
    if version != 1 {
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
                let value = match read_u8(&mut payload_cur)? {
                    0 => Value::Null,
                    1 => Value::Bool(read_u8(&mut payload_cur)? != 0),
                    2 => Value::Int(read_i64(&mut payload_cur)?),
                    3 => Value::Float(read_f64(&mut payload_cur)?),
                    4 => {
                        let value_id = read_u32(&mut payload_cur)? as usize;
                        Value::String(
                            strings
                                .get(value_id)
                                .ok_or("prop string out of range")?
                                .clone(),
                        )
                    }
                    other => return Err(format!("unsupported prop value tag {other}")),
                };
                ops.push(Op::SetProp { handle, key, value });
            }
            2 => {
                let handle = read_u32(&mut payload_cur)?;
                let text_id = read_u32(&mut payload_cur)? as usize;
                let text = strings.get(text_id).ok_or("text out of range")?.clone();
                ops.push(Op::SetText { handle, text });
            }
            3 => {
                ops.push(Op::CreateNode {
                    handle: read_u32(&mut payload_cur)?,
                    template_id: read_u32(&mut payload_cur)?,
                    parent: read_u32(&mut payload_cur)?,
                    index: read_u32(&mut payload_cur)?,
                });
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
                y: read_f32(&mut payload_cur)?,
            }),
            8 => ops.push(Op::Spawn {
                desc: NodeDesc::read_body(&mut payload_cur)?,
            }),
            9 => ops.push(Op::SpawnAt {
                desc: NodeDesc::read_body(&mut payload_cur)?,
                index: read_u32(&mut payload_cur)?,
            }),
            other => return Err(format!("unknown op tag {other}")),
        }
    }
    if payload_cur.position() != payload.len() as u64 {
        return Err("op payload length mismatch".into());
    }
    Ok((seq, ops))
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
