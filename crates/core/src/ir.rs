//! `BTIR` version 1. Little-endian. Parents appear before children. Ids are dense from 1.
//!
//! A single node uses the `BNOD` version 1 envelope. The body layout is shared
//! with `BTIR`, so `Spawn` ops and templates cannot drift.

use std::io::{Cursor, Read};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    View = 1,
    Text = 2,
}

#[derive(Clone, Debug, PartialEq)]
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

    /// `BNOD` version 1 envelope for one node.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"BNOD");
        out.extend_from_slice(&1u16.to_le_bytes());
        self.write_body(&mut out);
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut cur = Cursor::new(bytes);
        let mut magic = [0u8; 4];
        cur.read_exact(&mut magic).map_err(|_| "truncated node header")?;
        if &magic != b"BNOD" {
            return Err("bad node magic".into());
        }
        let version = read_u16(&mut cur)?;
        if version != 1 {
            return Err(format!("unsupported node version {version}"));
        }
        let node = Self::read_body(&mut cur)?;
        if cur.position() != bytes.len() as u64 {
            return Err("trailing node bytes".into());
        }
        Ok(node)
    }

    /// Shared with `BTIR`; no magic or version.
    pub fn write_body(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.id.to_le_bytes());
        out.extend_from_slice(&self.parent.to_le_bytes());
        out.push(self.kind as u8);
        out.push(u8::from(self.scroll_y));
        out.extend_from_slice(&self.background.to_le_bytes());
        out.extend_from_slice(&self.color.to_le_bytes());
        push_f32(out, self.font_size);
        push_f32(out, self.radius);
        out.extend_from_slice(&self.handler.to_le_bytes());
        push_f32(out, self.width.unwrap_or(f32::NAN));
        push_f32(out, self.height.unwrap_or(f32::NAN));
        push_f32(out, self.flex_grow);
        push_f32(out, self.flex_shrink);
        out.push(u8::from(self.row));
        for value in self.padding {
            push_f32(out, value);
        }
        let text = self.text.as_bytes();
        out.extend_from_slice(&(text.len() as u32).to_le_bytes());
        out.extend_from_slice(text);
    }

    pub fn read_body(cur: &mut Cursor<&[u8]>) -> Result<Self, String> {
        let id = read_u32(cur)?;
        let parent = read_u32(cur)?;
        let kind = match read_u8(cur)? {
            1 => Kind::View,
            2 => Kind::Text,
            other => return Err(format!("bad node kind {other}")),
        };
        let scroll_y = match read_u8(cur)? {
            0 => false,
            1 => true,
            other => return Err(format!("bad scroll flag {other}")),
        };
        let background = read_u32(cur)?;
        let color = read_u32(cur)?;
        let font_size = read_f32(cur)?;
        let radius = read_f32(cur)?;
        let handler = read_u32(cur)?;
        let width = optional_f32(read_f32(cur)?);
        let height = optional_f32(read_f32(cur)?);
        let flex_grow = read_f32(cur)?;
        let flex_shrink = read_f32(cur)?;
        let row = match read_u8(cur)? {
            0 => false,
            1 => true,
            other => return Err(format!("bad flex direction {other}")),
        };
        let mut padding = [0f32; 4];
        for slot in &mut padding {
            *slot = read_f32(cur)?;
        }
        let text_len = read_u32(cur)? as usize;
        let mut text_bytes = vec![0u8; text_len];
        cur.read_exact(&mut text_bytes).map_err(|_| "truncated node text")?;
        let text = String::from_utf8(text_bytes).map_err(|_| "text is not utf-8")?;
        Ok(Self {
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
        })
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
            node.write_body(&mut out);
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
            let node = NodeDesc::read_body(&mut cur)?;
            if node.id as usize != index + 1 {
                return Err(format!(
                    "node id {} is not dense (expected {})",
                    node.id,
                    index + 1
                ));
            }
            if index == 0 && node.parent != 0 {
                return Err("root parent must be 0".into());
            }
            if index > 0 && (node.parent == 0 || node.parent >= node.id) {
                return Err(format!("node {} parent {} is missing", node.id, node.parent));
            }
            nodes.push(node);
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
    cur.read_exact(&mut buf).map_err(|_| "truncated node")?;
    Ok(buf[0])
}

fn read_u16(cur: &mut Cursor<&[u8]>) -> Result<u16, String> {
    let mut buf = [0u8; 2];
    cur.read_exact(&mut buf).map_err(|_| "truncated node")?;
    Ok(u16::from_le_bytes(buf))
}

fn read_u32(cur: &mut Cursor<&[u8]>) -> Result<u32, String> {
    let mut buf = [0u8; 4];
    cur.read_exact(&mut buf).map_err(|_| "truncated node")?;
    Ok(u32::from_le_bytes(buf))
}

fn read_f32(cur: &mut Cursor<&[u8]>) -> Result<f32, String> {
    let mut buf = [0u8; 4];
    cur.read_exact(&mut buf).map_err(|_| "truncated node")?;
    Ok(f32::from_le_bytes(buf))
}
