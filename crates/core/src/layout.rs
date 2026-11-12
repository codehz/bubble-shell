//! Platform text layout snapshot.
//!
//! Hosts shape and wrap with the same object they draw (StaticLayout, Core Text,
//! or the canvas line breaker) and return this snapshot. Editing geometry — caret,
//! hit testing, selection — is read only from the snapshot. A snapshot whose key
//! does not match the current field is rejected, so a stale revision cannot drive IME.

use crate::editing::utf16_len;

pub const LAYOUT_MAGIC: u32 = 0x314C_5342;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutKey {
    pub revision: u64,
    pub width: f32,
    pub font_size: f32,
    pub font_weight: u16,
    pub line_height: f32,
    pub align: u8,
    pub density: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutRequest<'a> {
    pub text: &'a str,
    pub key: LayoutKey,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretEdge {
    pub x: f32,
    pub y: f32,
    pub h: f32,
}

/// Visual run. `start`/`end` are UTF-16 offsets. `level` is 0 for LTR and 1 for RTL.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutRun {
    pub start: u32,
    pub end: u32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub level: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutSnapshot {
    pub key: LayoutKey,
    pub units: u32,
    pub carets: Vec<CaretEdge>,
    pub runs: Vec<LayoutRun>,
}

impl LayoutSnapshot {
    pub fn matches(&self, key: &LayoutKey) -> bool {
        self.key.revision == key.revision
            && self.key.font_weight == key.font_weight
            && self.key.align == key.align
            && near(self.key.width, key.width)
            && near(self.key.font_size, key.font_size)
            && near(self.key.line_height, key.line_height)
            && near(self.key.density, key.density)
    }

    pub fn caret(&self, unit: u32) -> Option<CaretEdge> {
        self.carets.get(unit as usize).copied().or_else(|| self.carets.last().copied())
    }

    /// Nearest UTF-16 boundary. The caller snaps surrogate interiors.
    pub fn hit(&self, x: f32, y: f32) -> u32 {
        let mut best = 0u32;
        let mut best_d = f32::MAX;
        for (index, caret) in self.carets.iter().enumerate() {
            let dy = if y < caret.y {
                caret.y - y
            } else if y > caret.y + caret.h {
                y - (caret.y + caret.h)
            } else {
                0.0
            };
            let dx = caret.x - x;
            let dist = dx * dx + dy * dy * 16.0;
            if dist < best_d {
                best_d = dist;
                best = index as u32;
            }
        }
        best
    }

    /// Selection rectangles in snapshot coordinates. One rect per overlapping run,
    /// so a bidi selection is not assumed to be one monotonic span.
    pub fn selection_rects(&self, start: u32, end: u32) -> Vec<[f32; 4]> {
        let (lo, hi) = if start <= end { (start, end) } else { (end, start) };
        if lo == hi {
            return Vec::new();
        }
        let mut rects = Vec::new();
        for run in &self.runs {
            let from = lo.max(run.start);
            let to = hi.min(run.end);
            if from >= to {
                continue;
            }
            let x0 = self.boundary_x(run, from);
            let x1 = self.boundary_x(run, to);
            let left = x0.min(x1);
            let width = (x0 - x1).abs();
            if width > 0.0 {
                rects.push([left, run.y, width, run.h]);
            }
        }
        rects
    }

    /// Logical `unit == run.end` is the trailing visual edge. A soft wrap reuses
    /// that UTF-16 offset as the next line's first caret, so the run width is
    /// the authority for its own end.
    fn boundary_x(&self, run: &LayoutRun, unit: u32) -> f32 {
        if unit == run.end {
            return if run.level % 2 == 1 { run.x } else { run.x + run.w };
        }
        self.carets.get(unit as usize).map(|caret| caret.x).unwrap_or(run.x)
    }
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() <= 0.05 || (a.is_nan() && b.is_nan())
}

/// Snap `unit` to a scalar-value boundary. The middle of a surrogate pair jumps
/// to the nearer edge so IME geometry never lands inside a non-BMP character.
pub fn snap_unit(text: &str, unit: u32) -> u32 {
    let mut seen = 0u32;
    for ch in text.chars() {
        let next = seen + ch.len_utf16() as u32;
        if unit <= seen {
            return seen;
        }
        if unit < next {
            return if unit - seen <= next - unit { seen } else { next };
        }
        seen = next;
    }
    seen
}

pub fn encode_layout(snapshot: &LayoutSnapshot) -> Vec<u8> {
    let mut out = Vec::with_capacity(40 + snapshot.carets.len() * 12 + snapshot.runs.len() * 28);
    out.extend_from_slice(&LAYOUT_MAGIC.to_le_bytes());
    out.extend_from_slice(&snapshot.units.to_le_bytes());
    out.extend_from_slice(&(snapshot.runs.len() as u32).to_le_bytes());
    push_f32(&mut out, snapshot.key.width);
    push_f32(&mut out, snapshot.key.font_size);
    push_f32(&mut out, snapshot.key.line_height);
    push_f32(&mut out, snapshot.key.density);
    out.extend_from_slice(&snapshot.key.font_weight.to_le_bytes());
    out.push(snapshot.key.align);
    out.push(0);
    out.extend_from_slice(&snapshot.key.revision.to_le_bytes());
    for caret in &snapshot.carets {
        push_f32(&mut out, caret.x);
        push_f32(&mut out, caret.y);
        push_f32(&mut out, caret.h);
    }
    for run in &snapshot.runs {
        out.extend_from_slice(&run.start.to_le_bytes());
        out.extend_from_slice(&run.end.to_le_bytes());
        push_f32(&mut out, run.x);
        push_f32(&mut out, run.y);
        push_f32(&mut out, run.w);
        push_f32(&mut out, run.h);
        out.push(run.level);
        out.extend_from_slice(&[0, 0, 0]);
    }
    out
}

pub fn decode_layout(bytes: &[u8]) -> Result<LayoutSnapshot, String> {
    if bytes.len() < 40 {
        return Err("layout snapshot is truncated".into());
    }
    let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    if magic != LAYOUT_MAGIC {
        return Err("layout snapshot magic mismatch".into());
    }
    let units = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let run_count = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let key = LayoutKey {
        width: f32::from_le_bytes(bytes[12..16].try_into().unwrap()),
        font_size: f32::from_le_bytes(bytes[16..20].try_into().unwrap()),
        line_height: f32::from_le_bytes(bytes[20..24].try_into().unwrap()),
        density: f32::from_le_bytes(bytes[24..28].try_into().unwrap()),
        font_weight: u16::from_le_bytes(bytes[28..30].try_into().unwrap()),
        align: bytes[30],
        revision: u64::from_le_bytes(bytes[32..40].try_into().unwrap()),
    };
    let caret_count = units as usize + 1;
    let caret_bytes = caret_count * 12;
    let run_bytes = run_count * 28;
    if bytes.len() != 40 + caret_bytes + run_bytes {
        return Err("layout snapshot length mismatch".into());
    }
    let mut cursor = 40;
    let mut carets = Vec::with_capacity(caret_count);
    for _ in 0..caret_count {
        let x = f32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
        let y = f32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap());
        let h = f32::from_le_bytes(bytes[cursor + 8..cursor + 12].try_into().unwrap());
        if !x.is_finite() || !y.is_finite() || !h.is_finite() {
            return Err("layout caret is not finite".into());
        }
        carets.push(CaretEdge { x, y, h });
        cursor += 12;
    }
    let mut runs = Vec::with_capacity(run_count);
    for _ in 0..run_count {
        let start = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
        let end = u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap());
        let x = f32::from_le_bytes(bytes[cursor + 8..cursor + 12].try_into().unwrap());
        let y = f32::from_le_bytes(bytes[cursor + 12..cursor + 16].try_into().unwrap());
        let w = f32::from_le_bytes(bytes[cursor + 16..cursor + 20].try_into().unwrap());
        let h = f32::from_le_bytes(bytes[cursor + 20..cursor + 24].try_into().unwrap());
        let level = bytes[cursor + 24];
        if start > end || end > units || level > 1 || !x.is_finite() || !y.is_finite() || !w.is_finite() || !h.is_finite() {
            return Err("layout run is invalid".into());
        }
        runs.push(LayoutRun { start, end, x, y, w, h, level });
        cursor += 28;
    }
    Ok(LayoutSnapshot { key, units, carets, runs })
}

fn push_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// LTR wrap used when a host has no shaper (core tests, native fallback).
/// Platform editors must not use this for IME geometry.
pub fn wrapping_layout(req: &LayoutRequest<'_>, mut advance: impl FnMut(&str) -> f32) -> LayoutSnapshot {
    let lh = if req.key.line_height.is_finite() && req.key.line_height > 0.0 {
        req.key.line_height
    } else {
        0.0
    };
    let limit = if req.key.width.is_finite() && req.key.width > 0.0 {
        req.key.width
    } else {
        f32::MAX
    };
    let mut lines: Vec<(u32, u32, String)> = Vec::new();
    let mut base = 0u32;
    let paragraphs: Vec<&str> = req.text.split('\n').collect();
    let paragraphs = if paragraphs.is_empty() { vec![""] } else { paragraphs };
    for (index, para) in paragraphs.iter().enumerate() {
        let wrapped = wrap_paragraph(para, limit, &mut advance);
        let mut local = 0u32;
        for (byte_start, byte_end) in wrapped {
            let slice = &para[byte_start..byte_end];
            let len = utf16_len(slice);
            lines.push((base + local, base + local + len, slice.to_owned()));
            local += len;
        }
        base += utf16_len(para);
        if index + 1 != paragraphs.len() {
            base += 1;
        }
    }
    if lines.is_empty() {
        lines.push((0, 0, String::new()));
    }
    let units = utf16_len(req.text);
    let mut carets = vec![CaretEdge { x: 0.0, y: 0.0, h: lh }; units as usize + 1];
    let mut runs = Vec::new();
    for (index, (start, end, text)) in lines.iter().enumerate() {
        let y = index as f32 * lh;
        let measured = advance(text);
        let shift = align_shift(req.key.align, req.key.width, measured);
        let mut col = 0u32;
        let mut byte = 0usize;
        carets[*start as usize] = CaretEdge { x: shift, y, h: lh };
        for ch in text.chars() {
            byte += ch.len_utf8();
            col += ch.len_utf16() as u32;
            let x = shift + advance(&text[..byte]);
            let at = (*start + col) as usize;
            if at < carets.len() {
                carets[at] = CaretEdge { x, y, h: lh };
            }
        }
        if *end > *start {
            runs.push(LayoutRun {
                start: *start,
                end: *end,
                x: shift,
                y,
                w: measured,
                h: lh,
                level: 0,
            });
        }
        let _ = end;
    }
    // A trailing newline has no run. Its caret stays on the line before the break
    // only when that boundary was written; the following line owns the next caret.
    LayoutSnapshot {
        key: req.key,
        units,
        carets,
        runs,
    }
}

fn align_shift(align: u8, width: f32, measured: f32) -> f32 {
    if !width.is_finite() || width <= 0.0 || measured >= width {
        return 0.0;
    }
    match align {
        1 => (width - measured) * 0.5,
        2 => width - measured,
        _ => 0.0,
    }
}

fn wrap_paragraph(para: &str, limit: f32, advance: &mut impl FnMut(&str) -> f32) -> Vec<(usize, usize)> {
    if para.is_empty() {
        return vec![(0, 0)];
    }
    let mut lines = Vec::new();
    let mut start = 0usize;
    let mut cursor = 0usize;
    let mut last_space: Option<usize> = None;
    while cursor < para.len() {
        let ch = para[cursor..].chars().next().expect("char");
        let next = cursor + ch.len_utf8();
        let width = advance(&para[start..next]);
        if width > limit && cursor > start {
            if let Some(space) = last_space.filter(|space| *space > start) {
                lines.push((start, space));
                start = space + 1;
                last_space = None;
                cursor = start;
                continue;
            }
            lines.push((start, cursor));
            start = cursor;
            last_space = None;
            continue;
        }
        if ch == ' ' {
            last_space = Some(cursor);
        }
        cursor = next;
    }
    lines.push((start, para.len()));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(width: f32, revision: u64) -> LayoutKey {
        LayoutKey {
            revision,
            width,
            font_size: 10.0,
            font_weight: 400,
            line_height: 12.0,
            align: 0,
            density: 1.0,
        }
    }

    fn advance(text: &str) -> f32 {
        text.chars().count() as f32 * 10.0
    }

    #[test]
    fn wrap_moves_the_caret_to_the_next_line() {
        let req = LayoutRequest { text: "abcd", key: key(25.0, 3) };
        let snapshot = wrapping_layout(&req, advance);
        assert!(snapshot.matches(&req.key));
        let third = snapshot.caret(2).unwrap();
        assert!(third.y >= 12.0);
        assert_eq!(snapshot.hit(0.0, 14.0), 2);
        let rects = snapshot.selection_rects(1, 4);
        assert!(rects.len() >= 2);
    }

    #[test]
    fn bidi_selection_is_not_one_logical_span() {
        let mut snapshot = wrapping_layout(
            &LayoutRequest { text: "abCD", key: key(200.0, 1) },
            advance,
        );
        snapshot.runs.clear();
        snapshot.runs.push(LayoutRun { start: 0, end: 2, x: 0.0, y: 0.0, w: 20.0, h: 12.0, level: 0 });
        snapshot.runs.push(LayoutRun { start: 2, end: 4, x: 40.0, y: 0.0, w: 20.0, h: 12.0, level: 1 });
        snapshot.carets[2].x = 60.0;
        snapshot.carets[3].x = 60.0;
        snapshot.carets[4].x = 40.0;
        let rects = snapshot.selection_rects(1, 4);
        assert_eq!(rects.len(), 2);
        assert!(rects[0][0] < 20.0);
        assert!(rects[1][0] >= 40.0);
    }

    #[test]
    fn stale_revision_does_not_match() {
        let snapshot = wrapping_layout(&LayoutRequest { text: "a", key: key(40.0, 2) }, advance);
        let mut next = key(40.0, 3);
        assert!(!snapshot.matches(&next));
        next.revision = 2;
        next.width = 80.0;
        assert!(!snapshot.matches(&next));
    }

    #[test]
    fn surrogate_interior_snaps_to_a_scalar_boundary() {
        assert_eq!(snap_unit("a😀b", 2), 1);
        assert_eq!(snap_unit("a😀b", 1), 1);
        assert_eq!(snap_unit("a😀b", 3), 3);
    }

    #[test]
    fn round_trip_preserves_runs() {
        let snapshot = wrapping_layout(&LayoutRequest { text: "hi\nthere", key: key(100.0, 9) }, advance);
        let decoded = decode_layout(&encode_layout(&snapshot)).unwrap();
        assert_eq!(decoded, snapshot);
    }
}