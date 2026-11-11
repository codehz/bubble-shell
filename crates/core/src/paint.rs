//! Versioned binary encoding of `PaintCmd` for host painters.
//!
//! `tag u8` + little-endian payloads; the list is prefixed with its command
//! count. Hosts decode the same bytes on web, iOS, and Android.

use crate::shell::PaintCmd;

pub fn encode_paint(cmds: &[PaintCmd]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(cmds.len() as u32).to_le_bytes());
    for cmd in cmds {
        match cmd {
            PaintCmd::Fill {
                x,
                y,
                w,
                h,
                radius,
                color,
            } => {
                out.push(1);
                push_f32(&mut out, *x);
                push_f32(&mut out, *y);
                push_f32(&mut out, *w);
                push_f32(&mut out, *h);
                push_f32(&mut out, *radius);
                out.extend_from_slice(&color.to_le_bytes());
            }
            PaintCmd::Text {
                x,
                y,
                w,
                h,
                font_size,
                color,
                line_height,
                weight,
                align,
                text,
            } => {
                out.push(2);
                push_f32(&mut out, *x);
                push_f32(&mut out, *y);
                push_f32(&mut out, *w);
                push_f32(&mut out, *h);
                push_f32(&mut out, *font_size);
                out.extend_from_slice(&color.to_le_bytes());
                push_f32(&mut out, *line_height);
                out.extend_from_slice(&weight.to_le_bytes());
                out.push(*align);
                let bytes = text.as_bytes();
                out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
                out.extend_from_slice(bytes);
            }
            PaintCmd::Image {
                x,
                y,
                w,
                h,
                radius,
                fit,
                src,
            } => {
                out.push(7);
                push_f32(&mut out, *x);
                push_f32(&mut out, *y);
                push_f32(&mut out, *w);
                push_f32(&mut out, *h);
                push_f32(&mut out, *radius);
                out.push(*fit);
                let bytes = src.as_bytes();
                out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
                out.extend_from_slice(bytes);
            }
            PaintCmd::PushClip { x, y, w, h } => {
                out.push(3);
                push_f32(&mut out, *x);
                push_f32(&mut out, *y);
                push_f32(&mut out, *w);
                push_f32(&mut out, *h);
            }
            PaintCmd::PopClip => out.push(4),
            PaintCmd::PushOpacity { opacity } => {
                out.push(5);
                push_f32(&mut out, *opacity);
            }
            PaintCmd::PopOpacity => out.push(6),
        }
    }
    out
}

fn push_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}
