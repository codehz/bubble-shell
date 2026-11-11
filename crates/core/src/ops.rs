//! `BSOP` version 1. Little-endian batch with a string table.
//! M0 applies batches in order. Last `SetText` / `SetProp` for a key wins,
//! which matches per-frame last-write-wins without a separate coalesce pass.

use std::io::{Cursor, Read};

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    SetPropColor { handle: u32, key: String, rgba: u32 },
    SetText { handle: u32, text: String },
    /// `template_id` selects the mounted template node's defaults.
    CreateNode {
        handle: u32,
        template_id: u32,
        parent: u32,
        index: u32,
    },
    DisposeNode { handle: u32 },
    /// Final child index after the node is detached from its old parent.
    MoveNode { handle: u32, parent: u32, index: u32 },
    /// `handler` 0 clears the binding.
    BindEvent { handle: u32, handler: u32 },
}

pub fn encode_set_text(seq: u32, handle: u32, text: &str) -> Vec<u8> {
    encode(seq, &[Op::SetText { handle, text: text.to_owned() }])
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
            Op::MoveNode { handle, parent, index } => {
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
    cur.read_exact(&mut magic).map_err(|_| "truncated op header")?;
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
    cur.read_exact(&mut table_bytes).map_err(|_| "truncated string table")?;
    let strings = parse_strings(&table_bytes)?;
    let mut payload = vec![0u8; payload_len];
    cur.read_exact(&mut payload).map_err(|_| "truncated op payload")?;
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
