//! `BTIR` version 1. Little-endian. Parents appear before children. Ids are dense from 1.

use std::io::{Cursor, Read};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    View = 1,
    Text = 2,
}

#[derive(Clone, Debug)]
pub struct NodeDesc {
    pub id: u32,
    pub parent: u32,
    pub kind: Kind,
    pub scroll_y: bool,
    pub background: u32,
    pub color: u32,
    pub font_size: f32,
    pub radius: f32,
    pub handler: u32,
    /// `None` is auto.
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub row: bool,
    pub padding: [f32; 4],
    pub text: String,
}

impl NodeDesc {
    pub fn view(id: u32, parent: u32) -> Self {
        Self {
            id,
            parent,
            kind: Kind::View,
            scroll_y: false,
            background: 0,
            color: 0xffff_ffff,
            font_size: 16.0,
            radius: 0.0,
            handler: 0,
            width: None,
            height: None,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            row: false,
            padding: [0.0; 4],
            text: String::new(),
        }
    }

    pub fn text(id: u32, parent: u32, text: impl Into<String>) -> Self {
        let mut node = Self::view(id, parent);
        node.kind = Kind::Text;
        node.text = text.into();
        node
    }
}

#[derive(Clone, Debug)]
pub struct Template {
    pub nodes: Vec<NodeDesc>,
}

impl Template {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"BTIR");
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&(self.nodes.len() as u32).to_le_bytes());
        for node in &self.nodes {
            out.extend_from_slice(&node.id.to_le_bytes());
            out.extend_from_slice(&node.parent.to_le_bytes());
            out.push(node.kind as u8);
            out.push(u8::from(node.scroll_y));
            out.extend_from_slice(&node.background.to_le_bytes());
            out.extend_from_slice(&node.color.to_le_bytes());
            push_f32(&mut out, node.font_size);
            push_f32(&mut out, node.radius);
            out.extend_from_slice(&node.handler.to_le_bytes());
            push_f32(&mut out, node.width.unwrap_or(f32::NAN));
            push_f32(&mut out, node.height.unwrap_or(f32::NAN));
            push_f32(&mut out, node.flex_grow);
            push_f32(&mut out, node.flex_shrink);
            out.push(u8::from(node.row));
            for value in node.padding {
                push_f32(&mut out, value);
            }
            let text = node.text.as_bytes();
            out.extend_from_slice(&(text.len() as u32).to_le_bytes());
            out.extend_from_slice(text);
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut cur = Cursor::new(bytes);
        let mut magic = [0u8; 4];
        cur.read_exact(&mut magic).map_err(|_| "truncated header")?;
        if &magic != b"BTIR" {
            return Err("bad template magic".into());
        }
        let version = read_u16(&mut cur)?;
        if version != 1 {
            return Err(format!("unsupported template version {version}"));
        }
        let count = read_u32(&mut cur)? as usize;
        let mut nodes = Vec::with_capacity(count);
        for index in 0..count {
            let id = read_u32(&mut cur)?;
            let parent = read_u32(&mut cur)?;
            let kind = match read_u8(&mut cur)? {
                1 => Kind::View,
                2 => Kind::Text,
                other => return Err(format!("bad node kind {other}")),
            };
            let scroll_y = match read_u8(&mut cur)? {
                0 => false,
                1 => true,
                other => return Err(format!("bad scroll flag {other}")),
            };
            let background = read_u32(&mut cur)?;
            let color = read_u32(&mut cur)?;
            let font_size = read_f32(&mut cur)?;
            let radius = read_f32(&mut cur)?;
            let handler = read_u32(&mut cur)?;
            let width = optional_f32(read_f32(&mut cur)?);
            let height = optional_f32(read_f32(&mut cur)?);
            let flex_grow = read_f32(&mut cur)?;
            let flex_shrink = read_f32(&mut cur)?;
            let row = match read_u8(&mut cur)? {
                0 => false,
                1 => true,
                other => return Err(format!("bad flex direction {other}")),
            };
            let mut padding = [0f32; 4];
            for slot in &mut padding {
                *slot = read_f32(&mut cur)?;
            }
            let text_len = read_u32(&mut cur)? as usize;
            let mut text_bytes = vec![0u8; text_len];
            cur.read_exact(&mut text_bytes).map_err(|_| "truncated text")?;
            let text = String::from_utf8(text_bytes).map_err(|_| "text is not utf-8")?;
            if id as usize != index + 1 {
                return Err(format!("node id {id} is not dense (expected {})", index + 1));
            }
            if index == 0 && parent != 0 {
                return Err("root parent must be 0".into());
            }
            if index > 0 && (parent == 0 || parent >= id) {
                return Err(format!("node {id} parent {parent} is missing"));
            }
            nodes.push(NodeDesc {
                id,
                parent,
                kind,
                scroll_y,
                background,
                color,
                font_size,
                radius,
                handler,
                width,
                height,
                flex_grow,
                flex_shrink,
                row,
                padding,
                text,
            });
        }
        if cur.position() != bytes.len() as u64 {
            return Err("trailing template bytes".into());
        }
        Ok(Self { nodes })
    }
}

fn optional_f32(value: f32) -> Option<f32> {
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn push_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn read_u8(cur: &mut Cursor<&[u8]>) -> Result<u8, String> {
    let mut buf = [0u8; 1];
    cur.read_exact(&mut buf).map_err(|_| "truncated template")?;
    Ok(buf[0])
}

fn read_u16(cur: &mut Cursor<&[u8]>) -> Result<u16, String> {
    let mut buf = [0u8; 2];
    cur.read_exact(&mut buf).map_err(|_| "truncated template")?;
    Ok(u16::from_le_bytes(buf))
}

fn read_u32(cur: &mut Cursor<&[u8]>) -> Result<u32, String> {
    let mut buf = [0u8; 4];
    cur.read_exact(&mut buf).map_err(|_| "truncated template")?;
    Ok(u32::from_le_bytes(buf))
}

fn read_f32(cur: &mut Cursor<&[u8]>) -> Result<f32, String> {
    let mut buf = [0u8; 4];
    cur.read_exact(&mut buf).map_err(|_| "truncated template")?;
    Ok(f32::from_le_bytes(buf))
}
