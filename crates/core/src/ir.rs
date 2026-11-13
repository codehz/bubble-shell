//! Node descriptors. Little-endian. `BNOD` version 3 wraps one descriptor;
//! `Spawn` ops reuse the same body layout. Version 3 appends a TLV extension
//! block after the text. Lengths are not in the body; they arrive as `SetProp`.

use std::io::{Cursor, Read};

pub const NODE_VERSION: u16 = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Extension {
    pub tag: u8,
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    View = 1,
    Text = 2,
    Image = 3,
    Field = 4,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeDesc {
    pub id: u32,
    pub parent: u32,
    pub kind: Kind,
    /// Bit 0 of the descriptor scroll byte; bit 1 is `scroll_x`.
    pub scroll_y: bool,
    pub scroll_x: bool,
    /// `0xAARRGGBB`. Authored colors normally arrive as deferred `SetProp`
    /// values instead, which also accept CSS color strings.
    pub background: u32,
    /// `0xAARRGGBB`; same deferral rule as `background`.
    pub color: u32,
    pub font_size: f32,
    pub radius: f32,
    pub opacity: f32,
    pub handler: u32,
    /// Points applied at insert. Not in the `BNOD` body; authored lengths use `SetProp`.
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub padding: [f32; 4],
    pub flex_grow: f32,
    pub flex_shrink: f32,
    /// 0 column, 1 row, 2 column-reverse, 3 row-reverse.
    pub direction: u8,
    pub text: String,
    pub extensions: Vec<Extension>,
}

impl NodeDesc {
    pub fn view(id: u32, parent: u32) -> Self {
        Self {
            id,
            parent,
            kind: Kind::View,
            scroll_y: false,
            scroll_x: false,
            background: 0,
            color: 0xffff_ffff,
            font_size: 16.0,
            radius: 0.0,
            opacity: 1.0,
            handler: 0,
            width: None,
            height: None,
            padding: [0.0; 4],
            flex_grow: 0.0,
            flex_shrink: 0.0,
            direction: 0,
            text: String::new(),
            extensions: Vec::new(),
        }
    }

    pub fn text(id: u32, parent: u32, text: impl Into<String>) -> Self {
        let mut node = Self::view(id, parent);
        node.kind = Kind::Text;
        node.text = text.into();
        node
    }

    /// `BNOD` version 3 envelope for one node.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"BNOD");
        out.extend_from_slice(&NODE_VERSION.to_le_bytes());
        self.write_body(&mut out);
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut cur = Cursor::new(bytes);
        let mut magic = [0u8; 4];
        cur.read_exact(&mut magic)
            .map_err(|_| "truncated node header")?;
        if &magic != b"BNOD" {
            return Err("bad node magic".into());
        }
        let version = read_u16(&mut cur)?;
        if version != NODE_VERSION {
            return Err(format!("unsupported node version {version}"));
        }
        let node = Self::read_body(&mut cur, version)?;
        if cur.position() != bytes.len() as u64 {
            return Err("trailing node bytes".into());
        }
        Ok(node)
    }

    /// Shared by the `BNOD` envelope and `Spawn` payloads; no magic or version.
    pub fn write_body(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.id.to_le_bytes());
        out.extend_from_slice(&self.parent.to_le_bytes());
        out.push(self.kind as u8);
        out.push(u8::from(self.scroll_y) | (u8::from(self.scroll_x) << 1));
        out.extend_from_slice(&self.background.to_le_bytes());
        out.extend_from_slice(&self.color.to_le_bytes());
        push_f32(out, self.font_size);
        push_f32(out, self.radius);
        push_f32(out, self.opacity);
        out.extend_from_slice(&self.handler.to_le_bytes());
        push_f32(out, self.flex_grow);
        push_f32(out, self.flex_shrink);
        out.push(self.direction);
        let text = self.text.as_bytes();
        out.extend_from_slice(&(text.len() as u32).to_le_bytes());
        out.extend_from_slice(text);
        let mut extensions = Vec::new();
        for extension in &self.extensions {
            extensions.push(extension.tag);
            extensions.extend_from_slice(&(extension.data.len() as u32).to_le_bytes());
            extensions.extend_from_slice(&extension.data);
        }
        out.extend_from_slice(&(extensions.len() as u32).to_le_bytes());
        out.extend_from_slice(&extensions);
    }

    pub fn read_body(cur: &mut Cursor<&[u8]>, version: u16) -> Result<Self, String> {
        let id = read_u32(cur)?;
        let parent = read_u32(cur)?;
        let kind = match read_u8(cur)? {
            1 => Kind::View,
            2 => Kind::Text,
            3 => Kind::Image,
            4 => Kind::Field,
            other => return Err(format!("bad node kind {other}")),
        };
        let scroll_flags = read_u8(cur)?;
        if scroll_flags & !0b11 != 0 {
            return Err(format!("bad scroll flags {scroll_flags:#04x}"));
        }
        let scroll_y = scroll_flags & 1 != 0;
        let scroll_x = scroll_flags & 2 != 0;
        let background = read_u32(cur)?;
        let color = read_u32(cur)?;
        let font_size = read_f32(cur)?;
        let radius = read_f32(cur)?;
        let opacity = read_f32(cur)?;
        let handler = read_u32(cur)?;
        let flex_grow = read_f32(cur)?;
        let flex_shrink = read_f32(cur)?;
        let direction = read_u8(cur)?;
        if direction > 3 {
            return Err(format!("bad flex direction {direction}"));
        }
        let text_len = read_u32(cur)? as usize;
        let mut text_bytes = vec![0u8; text_len];
        cur.read_exact(&mut text_bytes)
            .map_err(|_| "truncated node text")?;
        let text = String::from_utf8(text_bytes).map_err(|_| "text is not utf-8")?;
        let mut extensions = Vec::new();
        if version >= 2 {
            let ext_len = read_u32(cur)? as usize;
            let mut ext_bytes = vec![0u8; ext_len];
            cur.read_exact(&mut ext_bytes)
                .map_err(|_| "truncated node extensions")?;
            let mut ext_cur = Cursor::new(ext_bytes.as_slice());
            while ext_cur.position() < ext_len as u64 {
                let tag = read_u8(&mut ext_cur)?;
                let len = read_u32(&mut ext_cur)? as usize;
                let mut data = vec![0u8; len];
                ext_cur
                    .read_exact(&mut data)
                    .map_err(|_| "truncated extension payload")?;
                extensions.push(Extension { tag, data });
            }
        }
        Ok(Self {
            id,
            parent,
            kind,
            scroll_y,
            scroll_x,
            background,
            color,
            font_size,
            radius,
            opacity,
            handler,
            width: None,
            height: None,
            padding: [0.0; 4],
            flex_grow,
            flex_shrink,
            direction,
            text,
            extensions,
        })
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
