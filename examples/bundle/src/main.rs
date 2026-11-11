mod compile;

use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

const HEADER: u32 = 2;
const COLUMN: u32 = 6;
const ROW: u32 = 7;
const BUTTON_HANDLER: u32 = 1;

fn main() {
    let template = compile::compile(include_str!("../app.tsx")).unwrap();
    let ir = template.encode();
    let js = format!(
        r#"const [count, setCount] = bubble.signal(0);
const [added, setAdded] = bubble.signal(0);
bubble.effect(() => {{
  bubble.setText({HEADER}, "Count: " + count());
}});
self.onTap = (handlerId) => {{
  if (handlerId !== {BUTTON_HANDLER}) return;
  setCount(count() + 1);
  const row = bubble.create({ROW}, {COLUMN}, 0);
  setAdded(added() + 1);
  bubble.setText(row, "Added " + added());
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
