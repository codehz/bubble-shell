use bubble_core::{NodeDesc, Template};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

const HEADER: u32 = 2;
const BUTTON_HANDLER: u32 = 1;

fn main() {
    let template = demo();
    let ir = template.encode();
    let js = format!(
        r#"let count = 0;
let seq = 1;
self.onTap = (handlerId) => {{
  if (handlerId !== {BUTTON_HANDLER}) return;
  count += 1;
  const text = "Count: " + count;
  const bytes = new TextEncoder().encode(text);
  const len = wasm.encode_text_op(seq++, {HEADER}, pass(bytes), bytes.length);
  const copy = new Uint8Array(memory().buffer, wasm.scratch_ptr(), len).slice();
  postMessage({{ type: "ops", buffer: copy.buffer }}, [copy.buffer]);
}};
"#
    );
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../web/bundle");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("template.ir.bin"), &ir).unwrap();
    fs::write(dir.join("main.js"), js.as_bytes()).unwrap();
    let manifest = serde_manifest(&ir, js.as_bytes());
    fs::write(dir.join("manifest.json"), manifest).unwrap();
}

fn demo() -> Template {
    let mut root = NodeDesc::view(1, 0);
    root.background = 0xff11_1111;
    root.padding = [16.0, 16.0, 16.0, 16.0];

    let mut header = NodeDesc::text(HEADER, 1, "Count: 0");
    header.font_size = 28.0;
    header.color = 0xffff_ffff;
    header.padding = [0.0, 0.0, 0.0, 8.0];

    let mut button = NodeDesc::view(3, 1);
    button.background = 0xff3b_82f6;
    button.radius = 8.0;
    button.handler = BUTTON_HANDLER;
    button.padding = [16.0, 10.0, 16.0, 10.0];

    let mut label = NodeDesc::text(4, 3, "Increment");
    label.font_size = 18.0;
    label.color = 0xffff_ffff;

    let mut scroller = NodeDesc::view(5, 1);
    scroller.scroll_y = true;
    scroller.flex_grow = 1.0;
    scroller.flex_shrink = 1.0;
    scroller.padding = [0.0, 12.0, 0.0, 0.0];

    let column = NodeDesc::view(6, 5);
    let mut nodes = vec![root, header, button, label, scroller, column];
    for index in 0..40 {
        let mut row = NodeDesc::text(7 + index, 6, format!("Row {index:02}"));
        row.font_size = 18.0;
        row.color = 0xffdd_dddd;
        row.padding = [0.0, 6.0, 0.0, 6.0];
        nodes.push(row);
    }
    Template { nodes }
}

fn serde_manifest(ir: &[u8], js: &[u8]) -> String {
    format!(
        r#"{{
  "schema": 1,
  "id": "dev.bubble.m0",
  "version": 1,
  "minRuntime": "0.1.0",
  "platforms": ["web"],
  "entry": "main.js",
  "files": [
    {{ "path": "template.ir.bin", "sha256": "{ir_hash}", "size": {ir_size} }},
    {{ "path": "main.js", "sha256": "{js_hash}", "size": {js_size} }}
  ],
  "keyId": "dev",
  "signature": "cGxhY2Vob2xkZXI"
}}
"#,
        ir_hash = hex(&Sha256::digest(ir)),
        ir_size = ir.len(),
        js_hash = hex(&Sha256::digest(js)),
        js_size = js.len(),
    )
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0xf) as usize] as char);
    }
    out
}
