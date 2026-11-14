//! `bubble dev`: build CommonJS module factories and push hot updates.

use std::collections::{BTreeSet, HashMap};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

use bubble_bundler::{build_dev, bundle_entry, render_dev_bundle, BundleOptions, DevChunk};

const RUNTIME_JS: &str = include_str!("../../native/js/runtime.js");
const HOT_JS: &str = include_str!("../js/hot.js");

const DEFAULT_MIN_RUNTIME: &str = "0.1.0";
const PLATFORMS: &[&str] = &["web", "ios", "android"];

pub fn release_manifest(
    id: Option<&str>,
    name: &str,
    code: &str,
    assets: &[bubble_bundler::BundledAsset],
) -> String {
    let id = id
        .map(str::to_string)
        .unwrap_or_else(|| format!("com.bubble.{name}"));
    let files = vec![serde_json::json!({
        "path": format!("{name}.js"),
        "sha256": bubble_bundler::content_hash(code.as_bytes()),
        "size": code.len() as u64,
    })];
    let assets: Vec<_> = assets
        .iter()
        .map(|asset| {
            serde_json::json!({
                "path": asset.path,
                "sha256": asset.sha256,
                "size": asset.size,
            })
        })
        .collect();
    let manifest = serde_json::json!({
        "schema": 1,
        "id": id,
        "version": 1,
        "minRuntime": DEFAULT_MIN_RUNTIME,
        "platforms": PLATFORMS,
        "entry": format!("{name}.js"),
        "files": files,
        "assets": assets,
        "keyId": "m0-placeholder",
        "signature": "m0-placeholder",
    });
    format!(
        "{}\n",
        serde_json::to_string_pretty(&manifest).expect("manifest serializes")
    )
}

pub struct DevOptions {
    pub entry: PathBuf,
    pub port: u16,
}

#[derive(Default)]
struct Shared {
    bundle: String,
    chunks: HashMap<String, String>,
    assets: HashMap<String, Vec<u8>>,
    sources: Vec<PathBuf>,
    clients: Vec<Arc<Mutex<TcpStream>>>,
    failed: bool,
    native_body: String,
}

pub fn run(options: DevOptions) -> Result<(), String> {
    let entry = std::fs::canonicalize(&options.entry)
        .map_err(|error| format!("{}: {error}", options.entry.display()))?;
    let sdk = web_sdk()?;
    let wasm = wasm_shell()?;

    let initial = build_dev(&entry).map_err(|error| error.message)?;
    let native_body = native_json(&entry)?;
    let shared = Arc::new(Mutex::new(Shared {
        bundle: render_dev_bundle(&initial),
        chunks: initial
            .chunks
            .iter()
            .map(|chunk| (chunk.id.clone(), chunk.code.clone()))
            .collect(),
        assets: asset_map(&initial.assets),
        sources: initial.sources.clone(),
        clients: Vec::new(),
        failed: false,
        native_body,
    }));

    let listener = TcpListener::bind(("127.0.0.1", options.port))
        .map_err(|error| format!("listen on port {}: {error}", options.port))?;
    println!(
        "bubble dev on http://127.0.0.1:{} ({} -> {})",
        options.port,
        entry.display(),
        initial.entry_id
    );

    spawn_watcher(shared.clone(), entry.clone());
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            continue;
        };
        let shared = shared.clone();
        let sdk = sdk.clone();
        let wasm = wasm.clone();
        thread::spawn(move || {
            let _ = handle_connection(stream, &shared, &sdk, &wasm);
        });
    }
    Ok(())
}

fn asset_map(assets: &[bubble_bundler::BundledAsset]) -> HashMap<String, Vec<u8>> {
    assets
        .iter()
        .map(|asset| (asset.path.clone(), asset.bytes.clone()))
        .collect()
}

fn spawn_watcher(shared: Arc<Mutex<Shared>>, entry: PathBuf) {
    thread::spawn(move || {
        // Watched files accumulate across rebuilds: a failed build may have
        // already imported new files, and they must still trigger a rebuild.
        let mut known: BTreeSet<PathBuf> = shared
            .lock()
            .unwrap()
            .sources
            .iter()
            .cloned()
            .chain(std::iter::once(entry.clone()))
            .collect();
        let mut stamps = stamps_known(&known);
        loop {
            thread::sleep(Duration::from_millis(150));
            let next = stamps_known(&known);
            if next == stamps {
                continue;
            }
            match build_dev(&entry) {
                Ok(build) => {
                    let mut changes = Vec::new();
                    for chunk in &build.chunks {
                        let changed = shared
                            .lock()
                            .unwrap()
                            .chunks
                            .get(&chunk.id)
                            .is_none_or(|code| code != &chunk.code);
                        if changed {
                            changes.push(chunk.clone());
                        }
                    }
                    let removed = {
                        let known = &shared.lock().unwrap().chunks;
                        let fresh: Vec<_> = build.chunks.iter().map(|c| &c.id).collect();
                        known.keys().any(|id| !fresh.contains(&id))
                    };
                    let (bundle, assets, message) = {
                        (
                            render_dev_bundle(&build),
                            asset_map(&build.assets),
                            if removed {
                                Message::Reload
                            } else if changes.is_empty() {
                                Message::None
                            } else {
                                Message::Update(changes)
                            },
                        )
                    };
                    let was_failed = {
                        let mut state = shared.lock().unwrap();
                        let was_failed = state.failed;
                        state.failed = false;
                        state.bundle = bundle;
                        state.assets = assets;
                        state.sources = build.sources.clone();
                        state.chunks = build
                            .chunks
                            .iter()
                            .map(|chunk| (chunk.id.clone(), chunk.code.clone()))
                            .collect();
                        was_failed
                    };
                    match native_json(&entry) {
                        Ok(body) => shared.lock().unwrap().native_body = body,
                        Err(error) => eprintln!("native bundle failed:\n{error}"),
                    }
                    match message {
                        Message::Update(chunks) => broadcast(&shared, &update_json(&chunks)),
                        Message::Reload => broadcast(&shared, &reload_json()),
                        Message::None if was_failed => broadcast(&shared, "{\"type\":\"clear\"}"),
                        Message::None => {}
                    }
                    println!("rebuilt {} module(s)", build.chunks.len());
                    known.extend(build.sources);
                    stamps = stamps_known(&known);
                }
                Err(error) => {
                    eprintln!("build failed:\n{}", error.message);
                    shared.lock().unwrap().failed = true;
                    broadcast(&shared, &error_json(&error.message));
                    known.extend(error.sources);
                    stamps = stamps_known(&known);
                }
            }
        }
    });
}

#[derive(Clone)]
enum Message {
    None,
    Reload,
    Update(Vec<DevChunk>),
}

fn stamps_known(known: &BTreeSet<PathBuf>) -> Vec<(PathBuf, Option<SystemTime>, u64)> {
    stamps_of(&known.iter().cloned().collect::<Vec<_>>())
}

fn stamps_of(sources: &[PathBuf]) -> Vec<(PathBuf, Option<SystemTime>, u64)> {
    sources
        .iter()
        .map(|path| {
            let metadata = std::fs::metadata(path).ok();
            let modified = metadata.as_ref().and_then(|meta| meta.modified().ok());
            let len = metadata.map(|meta| meta.len()).unwrap_or(0);
            (path.clone(), modified, len)
        })
        .collect()
}

fn update_json(chunks: &[DevChunk]) -> String {
    let modules: Vec<_> = chunks
        .iter()
        .map(|chunk| serde_json::json!({ "id": chunk.id, "code": chunk.code }).to_string())
        .collect();
    format!(
        "{{\"type\":\"update\",\"modules\":[{}]}}",
        modules.join(",")
    )
}

fn reload_json() -> String {
    "{\"type\":\"reload\"}".to_string()
}

fn error_json(message: &str) -> String {
    serde_json::json!({ "type": "error", "message": message }).to_string()
}

fn broadcast(shared: &Arc<Mutex<Shared>>, message: &str) {
    let clients = shared.lock().unwrap().clients.clone();
    let mut dead = Vec::new();
    for (index, client) in clients.iter().enumerate() {
        let mut stream = client.lock().unwrap();
        if write_text_frame(&mut stream, message).is_err() {
            dead.push(index);
        }
    }
    if !dead.is_empty() {
        let mut state = shared.lock().unwrap();
        for index in dead.into_iter().rev() {
            state.clients.remove(index);
        }
    }
}

fn write_text_frame(stream: &mut TcpStream, text: &str) -> std::io::Result<()> {
    let payload = text.as_bytes();
    let mut header = Vec::with_capacity(10);
    header.push(0x81);
    if payload.len() < 126 {
        header.push(payload.len() as u8);
    } else if payload.len() <= u16::MAX as usize {
        header.push(126);
        header.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    } else {
        header.push(127);
        header.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    }
    stream.write_all(&header)?;
    stream.write_all(payload)?;
    stream.flush()
}

fn serve_native(stream: &mut TcpStream, shared: &Arc<Mutex<Shared>>) -> std::io::Result<()> {
    // The watcher prebuilds the release bundle; requests never trigger a build.
    let body = shared.lock().unwrap().native_body.clone();
    if body.is_empty() {
        return respond(stream, 500, "text/plain", b"native bundle not ready");
    }
    respond(stream, 200, "application/json", body.as_bytes())
}

fn native_json(entry: &Path) -> Result<String, String> {
    let bundled = bundle_entry(&BundleOptions {
        entry: entry.to_path_buf(),
        source_map: false,
    })?;
    let manifest = release_manifest(None, &bundled.name, &bundled.code, &bundled.assets);
    Ok(serde_json::json!({
        "manifest": manifest,
        "entry": bundled.code,
    })
    .to_string())
}

fn handle_connection(
    mut stream: TcpStream,
    shared: &Arc<Mutex<Shared>>,
    sdk: &Path,
    wasm: &Path,
) -> std::io::Result<()> {
    let request = read_request(&mut stream)?;
    let Some(line) = request.lines().next() else {
        return Ok(());
    };
    let mut parts = line.split(' ');
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    if method != "GET" {
        return respond(&mut stream, 405, "text/plain", b"method not allowed");
    }
    let path = target.split('?').next().unwrap_or("/");

    if path == "/__bubble/ws" {
        return upgrade_websocket(stream, &request, shared);
    }
    if path == "/" {
        return respond(
            &mut stream,
            200,
            "text/html; charset=utf-8",
            DEV_PAGE.as_bytes(),
        );
    }
    if path == "/hot.js" {
        return respond(
            &mut stream,
            200,
            "text/javascript; charset=utf-8",
            HOT_JS.as_bytes(),
        );
    }
    if path == "/runtime.js" {
        return respond(
            &mut stream,
            200,
            "text/javascript; charset=utf-8",
            RUNTIME_JS.as_bytes(),
        );
    }
    if path == "/__bubble/native.json" {
        return serve_native(&mut stream, shared);
    }
    if path == "/__bubble/dev.js" {
        let bundle = shared.lock().unwrap().bundle.clone();
        return respond(
            &mut stream,
            200,
            "text/javascript; charset=utf-8",
            bundle.as_bytes(),
        );
    }
    if let Some(asset) = path.strip_prefix("/__bubble/") {
        let assets = shared.lock().unwrap().assets.clone();
        return match assets.get(asset) {
            Some(bytes) => respond(&mut stream, 200, mime_for(asset), bytes),
            None => respond(&mut stream, 404, "text/plain", b"asset not found"),
        };
    }
    serve_static(&mut stream, sdk, wasm, path)
}

const DEV_PAGE: &str = r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>Bubble Dev</title>
    <style>
      html, body { margin: 0; height: 100%; background: #111; }
      canvas { width: 100%; height: 100%; display: block; touch-action: none; }
      canvas:focus { outline: none; }
    </style>
    <script type="importmap">
      {
        "imports": {
          "bubble-shell": "/runtime.js",
          "bubble:rt": "/runtime.js",
          "bubble:hot": "/hot.js"
        }
      }
    </script>
  </head>
  <body>
    <canvas></canvas>
    <script>
      window.__bubbleDev = {
        bundle: "/__bubble/dev.js",
        base: "/__bubble/",
        ws: (location.protocol === "https:" ? "wss://" : "ws://") + location.host + "/__bubble/ws",
      };
    </script>
    <script type="module" src="/src/host.js"></script>
  </body>
</html>
"#;

fn read_request(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") || buffer.len() > 16 * 1024 {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

fn header<'a>(request: &'a str, name: &str) -> Option<&'a str> {
    request.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

fn upgrade_websocket(
    mut stream: TcpStream,
    request: &str,
    shared: &Arc<Mutex<Shared>>,
) -> std::io::Result<()> {
    let Some(key) = header(request, "sec-websocket-key") else {
        return respond(&mut stream, 400, "text/plain", b"missing websocket key");
    };
    let accept = websocket_accept(key);
    write!(
        stream,
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
    )?;
    stream.flush()?;
    let client = Arc::new(Mutex::new(stream.try_clone()?));
    shared.lock().unwrap().clients.push(client);

    // Keep the connection open; browsers never send application frames here.
    let mut scratch = [0u8; 512];
    loop {
        match stream.read(&mut scratch) {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                if scratch[..read]
                    .first()
                    .is_some_and(|byte| byte & 0x0f == 0x8)
                {
                    break;
                }
            }
        }
    }
    Ok(())
}

fn websocket_accept(key: &str) -> String {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(key.as_bytes());
    hasher.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    base64(&hasher.finalize())
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        out.push(TABLE[(b[0] >> 2) as usize] as char);
        out.push(TABLE[(((b[0] & 0x03) << 4) | (b[1] >> 4)) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(((b[1] & 0x0f) << 2) | (b[2] >> 6)) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(b[2] & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// Web host modules and the wasm shell belong to this toolchain, not the app.
/// Paths are fixed at compile time so `bubble dev` does not take a project root.
fn toolchain_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn require_toolchain(path: PathBuf, hint: &str) -> Result<PathBuf, String> {
    std::fs::canonicalize(&path).map_err(|_| format!("{hint} ({})", path.display()))
}

fn web_sdk() -> Result<PathBuf, String> {
    require_toolchain(toolchain_root().join("sdk/web/src"), "web sdk not found")
}

fn wasm_shell() -> Result<PathBuf, String> {
    require_toolchain(
        toolchain_root().join("target/wasm32-unknown-unknown/release/bubble_wasm.wasm"),
        "wasm shell not found; cargo build -p bubble-wasm --target wasm32-unknown-unknown --release",
    )
}

const SDK_MODULES: &[&str] = &["host.js", "a11y.js", "ime.js", "textlayout.js"];

fn static_path(sdk: &Path, wasm: &Path, url_path: &str) -> Option<PathBuf> {
    if url_path == "/bubble.wasm" {
        return Some(wasm.to_path_buf());
    }
    let name = url_path.strip_prefix("/src/")?;
    SDK_MODULES.contains(&name).then(|| sdk.join(name))
}

fn serve_static(
    stream: &mut TcpStream,
    sdk: &Path,
    wasm: &Path,
    url_path: &str,
) -> std::io::Result<()> {
    let Some(file) = static_path(sdk, wasm, url_path) else {
        return respond(stream, 404, "text/plain", b"not found");
    };
    match std::fs::read(&file) {
        Ok(bytes) => respond(stream, 200, mime_for(url_path), &bytes),
        Err(_) => respond(stream, 404, "text/plain", b"not found"),
    }
}

fn respond(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()
}

fn mime_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "html" => "text/html; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "wasm" => "application/wasm",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "css" => "text/css; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::{static_path, toolchain_root};

    #[test]
    fn web_host_comes_from_the_toolchain_sdk() {
        let sdk = std::fs::canonicalize(toolchain_root().join("sdk/web/src")).unwrap();
        let wasm = std::path::PathBuf::from("bubble_wasm.wasm");
        let host = static_path(&sdk, &wasm, "/src/host.js").unwrap();
        let text = std::fs::read_to_string(host).unwrap();
        assert!(text.contains("from \"./a11y.js\""));
        assert!(static_path(&sdk, &wasm, "/src/../host.js").is_none());
        assert!(static_path(&sdk, &wasm, "/sdk.js").is_none());
        assert_eq!(
            static_path(&sdk, &wasm, "/bubble.wasm").as_deref(),
            Some(wasm.as_path())
        );
    }
}
