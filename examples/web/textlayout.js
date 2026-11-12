// BLS1 layout snapshot. Line breaks match canvas measurement. A line that contains
// RTL uses DOM ranges so caret x is visual, not a logical prefix width.

const MAGIC = 0x314c5342;

let probe;

function probeElement() {
  if (typeof document === "undefined") return null;
  if (!probe) {
    probe = document.createElement("div");
    probe.style.position = "absolute";
    probe.style.left = "-99999px";
    probe.style.top = "0";
    probe.style.visibility = "hidden";
    probe.style.padding = "0";
    probe.style.margin = "0";
    probe.style.whiteSpace = "pre";
    document.body.appendChild(probe);
  }
  return probe;
}

function utf16Length(text) {
  return text.length;
}

function hasRtl(text) {
  return /[\u0590-\u08FF\uFB1D-\uFDFF\uFE70-\uFEFF]/.test(text);
}

export function wrapLines(text, maxWidth, measure) {
  const lines = [];
  let unit = 0;
  const paragraphs = text.split("\n");
  for (let p = 0; p < paragraphs.length; p += 1) {
    const para = paragraphs[p];
    if (maxWidth === null || !(maxWidth > 0)) {
      lines.push({ text: para, start: unit });
    } else {
      let line = "";
      let start = unit;
      for (const char of para) {
        const next = line + char;
        if (line && measure(next) > maxWidth) {
          const space = line.lastIndexOf(" ");
          if (space > 0) {
            lines.push({ text: line.slice(0, space), start });
            const skipped = line.slice(0, space + 1);
            start += skipped.length;
            line = line.slice(space + 1) + char;
          } else {
            lines.push({ text: line, start });
            start += line.length;
            line = char;
          }
        } else {
          line = next;
        }
      }
      lines.push({ text: line, start });
    }
    unit += para.length;
    if (p + 1 < paragraphs.length) unit += 1;
  }
  return lines.length ? lines : [{ text: "", start: 0 }];
}

function visualOffsets(line, font, lineHeight) {
  const el = probeElement();
  if (!el || !line) return null;
  el.style.font = font;
  el.style.lineHeight = `${lineHeight}px`;
  el.textContent = line;
  const node = el.firstChild;
  if (!node) return null;
  const base = el.getBoundingClientRect();
  const xs = [];
  for (let i = 0; i <= line.length; i += 1) {
    const range = document.createRange();
    range.setStart(node, i);
    range.setEnd(node, i);
    const box = range.getClientRects()[0] || range.getBoundingClientRect();
    xs.push(box.left - base.left);
  }
  return xs;
}

function putU32(view, offset, value) {
  view.setUint32(offset, value >>> 0, true);
  return offset + 4;
}
function putF32(view, offset, value) {
  view.setFloat32(offset, value, true);
  return offset + 4;
}

export function layoutSnapshot({
  text,
  width,
  fontSize,
  lineHeight,
  weight,
  align,
  density,
  revision,
  measure,
  font,
}) {
  const lh = lineHeight > 0 ? lineHeight : fontSize * 1.2;
  const lines = wrapLines(text, width > 0 ? width : null, measure);
  const units = utf16Length(text);
  const carets = Array.from({ length: units + 1 }, () => ({ x: 0, y: 0, h: lh }));
  const runs = [];
  lines.forEach((line, index) => {
    const y = index * lh;
    const logical = measure(line.text);
    let shift = 0;
    if (width > 0 && logical < width) {
      if (align === 1) shift = (width - logical) / 2;
      else if (align === 2) shift = width - logical;
    }
    const rtl = hasRtl(line.text);
    const visual = rtl ? visualOffsets(line.text, font, lh) : null;
    line.x = shift;
    line.y = y;
    const end = line.start + line.text.length;
    const xAt = (col) => {
      if (visual) return shift + visual[col];
      return shift + measure(line.text.slice(0, col));
    };
    carets[line.start] = { x: xAt(0), y, h: lh };
    let col = 0;
    for (const char of line.text) {
      col += char.length;
      const at = line.start + col;
      if (at < carets.length) carets[at] = { x: xAt(col), y, h: lh };
    }
    if (end > line.start) {
      if (!visual) {
        runs.push({ start: line.start, end, x: shift, y, w: logical, h: lh, level: 0 });
      } else {
        let i = 0;
        while (i < line.text.length) {
          let j = i;
          const step0 = line.text.codePointAt(j) > 0xffff ? 2 : 1;
          let level = xAt(Math.min(j + step0, line.text.length)) < xAt(j) - 0.01 ? 1 : 0;
          while (j < line.text.length) {
            const step = line.text.codePointAt(j) > 0xffff ? 2 : 1;
            const next = Math.min(j + step, line.text.length);
            const stepLevel = xAt(next) < xAt(j) - 0.01 ? 1 : 0;
            if (j > i && stepLevel !== level) break;
            level = stepLevel;
            j = next;
          }
          if (j === i) break;
          const x0 = xAt(i);
          const x1 = xAt(j);
          runs.push({
            start: line.start + i,
            end: line.start + j,
            x: Math.min(x0, x1),
            y,
            w: Math.abs(x1 - x0),
            h: lh,
            level,
          });
          i = j;
        }
      }
    }
  });
  const bytes = new Uint8Array(40 + carets.length * 12 + runs.length * 28);
  const view = new DataView(bytes.buffer);
  let offset = 0;
  offset = putU32(view, offset, MAGIC);
  offset = putU32(view, offset, units);
  offset = putU32(view, offset, runs.length);
  offset = putF32(view, offset, width);
  offset = putF32(view, offset, fontSize);
  offset = putF32(view, offset, lineHeight);
  offset = putF32(view, offset, density);
  view.setUint16(offset, weight, true);
  offset += 2;
  view.setUint8(offset, align);
  offset += 1;
  view.setUint8(offset, 0);
  offset += 1;
  view.setUint32(offset, revision >>> 0, true);
  view.setUint32(offset + 4, Math.floor(revision / 2 ** 32) >>> 0, true);
  offset += 8;
  for (const caret of carets) {
    offset = putF32(view, offset, caret.x);
    offset = putF32(view, offset, caret.y);
    offset = putF32(view, offset, caret.h);
  }
  for (const run of runs) {
    offset = putU32(view, offset, run.start);
    offset = putU32(view, offset, run.end);
    offset = putF32(view, offset, run.x);
    offset = putF32(view, offset, run.y);
    offset = putF32(view, offset, run.w);
    offset = putF32(view, offset, run.h);
    view.setUint8(offset, run.level);
    offset += 4;
  }
  return { bytes, lines, carets, runs, lineHeight: lh };
}