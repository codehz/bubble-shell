//! Install a host, launch it on a booted simulator, and read the JSON report
//! the painter writes. The steps live here, not in a shell script.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::report::{Check, ScenarioReport, SuiteReport};

#[derive(Deserialize)]
struct DeviceReport {
    suite: String,
    ok: bool,
    checks: Vec<Check>,
}

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(cmd: &mut Command) -> Result<String, String> {
    let output = cmd.output().map_err(|err| format!("{cmd:?}: {err}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        return Err(format!("{cmd:?} exited {}\n{stdout}{stderr}", output.status));
    }
    Ok(stdout)
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::copy(from, to).map(|_| ()).map_err(|err| format!("copy {from:?}: {err}"))
}

pub fn ios() -> Result<SuiteReport, String> {
    let root = workspace();
    run(Command::new("cargo")
        .args(["build", "-p", "bubble-ffi", "--release", "--target", "aarch64-apple-ios-sim"])
        .current_dir(&root))?;
    let sdk = run(Command::new("xcrun").args(["--sdk", "iphonesimulator", "--show-sdk-path"]))?;
    let sdk = sdk.trim();
    let app = root.join("examples/ios/BubbleShell.app");
    let _ = std::fs::remove_dir_all(&app);
    std::fs::create_dir_all(&app).map_err(|err| err.to_string())?;
    copy(&root.join("examples/ios/Info.plist"), &app.join("Info.plist"))?;
    copy(&root.join("examples/web/bundle/manifest.json"), &app.join("manifest.json"))?;
    copy(&root.join("examples/web/bundle/main.js"), &app.join("main.js"))?;
    let font = Path::new("/System/Library/Fonts/Supplemental/Arial.ttf");
    if font.exists() {
        copy(font, &app.join("HostFont.ttf"))?;
    }
    run(Command::new("swiftc")
        .args([
            "-sdk",
            sdk,
            "-target",
            "arm64-apple-ios17.0-simulator",
            "-O",
            "-framework",
            "UIKit",
            "-framework",
            "CoreText",
            "-framework",
            "CoreGraphics",
            "-L",
        ])
        .arg(root.join("target/aarch64-apple-ios-sim/release"))
        .args(["-lbubble_ffi", "-import-objc-header", "/dev/null", "-o"])
        .arg(app.join("BubbleShell"))
        .arg(root.join("examples/ios/Sources/main.swift")))?;
    run(Command::new("xcrun").args(["simctl", "install", "booted"]).arg(&app))?;
    let _ = run(Command::new("xcrun").args(["simctl", "terminate", "booted", "dev.bubble.shell"]));
    run(Command::new("xcrun").args(["simctl", "launch", "booted", "dev.bubble.shell"]))?;
    let container = run(Command::new("xcrun").args([
        "simctl",
        "get_app_container",
        "booted",
        "dev.bubble.shell",
        "data",
    ]))?;
    let result = PathBuf::from(container.trim()).join("Documents/m1-result.json");
    let body = wait_file(&result, Duration::from_secs(20))?;
    Ok(decode("m1.ios", &body))
}

pub fn android() -> Result<SuiteReport, String> {
    let root = workspace();
    let sdk = std::env::var("ANDROID_HOME").map_err(|_| "ANDROID_HOME is unset".to_string())?;
    let ndk = PathBuf::from(&sdk).join("ndk/27.2.12479018");
    let pre = ndk.join("toolchains/llvm/prebuilt/darwin-arm64");
    let pre = if pre.is_dir() { pre } else { ndk.join("toolchains/llvm/prebuilt/darwin-x86_64") };
    let cc = pre.join("bin/aarch64-linux-android24-clang");
    let ar = pre.join("bin/llvm-ar");
    run(Command::new("cargo")
        .args(["build", "-p", "bubble-ffi", "--release", "--target", "aarch64-linux-android"])
        .env("CC_aarch64_linux_android", &cc)
        .env("AR_aarch64_linux_android", &ar)
        .env("CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER", &cc)
        .env(
            "BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android",
            format!("--sysroot={}", pre.join("sysroot").display()),
        )
        .current_dir(&root))?;
    let kotlin = std::env::var("KOTLIN_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cache/bubble-kotlinc"));
    let kotlinc = kotlin.join("bin/kotlinc");
    let bt = PathBuf::from(&sdk).join("build-tools/36.1.0");
    let android_jar = PathBuf::from(&sdk).join("platforms/android-35/android.jar");
    let out = root.join("examples/android/build");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(out.join("apk/lib/arm64-v8a")).map_err(|err| err.to_string())?;
    copy(&root.join("examples/web/bundle/manifest.json"), &out.join("apk/assets/manifest.json"))
        .or_else(|_| {
            std::fs::create_dir_all(out.join("apk/assets")).map_err(|err| err.to_string())?;
            copy(&root.join("examples/web/bundle/manifest.json"), &out.join("apk/assets/manifest.json"))
        })?;
    copy(&root.join("examples/web/bundle/main.js"), &out.join("apk/assets/main.js"))?;
    let font = Path::new("/System/Library/Fonts/Supplemental/Arial.ttf");
    if font.exists() {
        copy(font, &out.join("apk/assets/HostFont.ttf"))?;
    }
    copy(
        &root.join("target/aarch64-linux-android/release/libbubble_ffi.so"),
        &out.join("apk/lib/arm64-v8a/libbubble_ffi.so"),
    )?;
    let java = root.join("examples/android/app/src/main/java/dev/bubble/shell");
    run(Command::new(&kotlinc)
        .args(["-jvm-target", "17", "-classpath"])
        .arg(&android_jar)
        .args(["-d"])
        .arg(out.join("classes.jar"))
        .arg(java.join("BubbleView.kt"))
        .arg(java.join("BubbleNative.kt"))
        .arg(java.join("MainActivity.kt")))?;
    run(Command::new(bt.join("d8"))
        .args(["--min-api", "24", "--lib"])
        .arg(&android_jar)
        .args(["--output"])
        .arg(out.join("apk"))
        .arg(out.join("classes.jar"))
        .arg(kotlin.join("lib/kotlin-stdlib.jar")))?;
    run(Command::new(bt.join("aapt2"))
        .args(["link", "-o"])
        .arg(out.join("base.apk"))
        .args(["-I"])
        .arg(&android_jar)
        .args(["--manifest"])
        .arg(root.join("examples/android/app/src/main/AndroidManifest.xml"))
        .args(["--min-sdk-version", "24", "--target-sdk-version", "35"]))?;
    run(Command::new("unzip").args(["-q", "-o"]).arg(out.join("base.apk")).args(["-d"]).arg(out.join("apk")))?;
    let unsigned = out.join("BubbleShell-unsigned.apk");
    let _ = std::fs::remove_file(&unsigned);
    run(Command::new("zip").args(["-qr", "-n", "arsc"]).arg(&unsigned).arg(".").current_dir(out.join("apk")))?;
    let aligned = out.join("BubbleShell-aligned.apk");
    run(Command::new(bt.join("zipalign")).args(["-p", "-f", "4"]).arg(&unsigned).arg(&aligned))?;
    let keystore = root.join("examples/android/debug.keystore");
    if !keystore.exists() {
        run(Command::new("keytool").args([
            "-genkeypair",
            "-keystore",
            keystore.to_str().unwrap_or_default(),
            "-storepass",
            "android",
            "-keypass",
            "android",
            "-alias",
            "androiddebugkey",
            "-keyalg",
            "RSA",
            "-keysize",
            "2048",
            "-validity",
            "10000",
            "-dname",
            "CN=BubbleShell",
        ]))?;
    }
    let apk = out.join("BubbleShell.apk");
    run(Command::new(bt.join("apksigner")).args([
        "sign",
        "--ks",
        keystore.to_str().unwrap_or_default(),
        "--ks-pass",
        "pass:android",
        "--key-pass",
        "pass:android",
        "--out",
        apk.to_str().unwrap_or_default(),
    ]).arg(&aligned))?;
    run(Command::new("adb").args(["install", "-r"]).arg(&apk))?;
    let _ = run(Command::new("adb").args(["shell", "am", "force-stop", "dev.bubble.shell"]));
    let _ = run(Command::new("adb").args(["logcat", "-c"]));
    run(Command::new("adb").args(["shell", "am", "start", "-n", "dev.bubble.shell/.MainActivity"]))?;
    let body = wait_logcat(Duration::from_secs(25))?;
    Ok(decode("m1.android", &body))
}

fn decode(name: &str, body: &str) -> SuiteReport {
    match serde_json::from_str::<DeviceReport>(body) {
        Ok(report) => SuiteReport {
            suite: report.suite,
            ok: report.ok,
            scenarios: vec![ScenarioReport {
                name: name.into(),
                ok: report.ok,
                elapsed_ms: 0,
                checks: report.checks,
            }],
        },
        Err(err) => SuiteReport {
            suite: name.into(),
            ok: false,
            scenarios: vec![ScenarioReport {
                name: name.into(),
                ok: false,
                elapsed_ms: 0,
                checks: vec![Check {
                    name: "parse report".into(),
                    ok: false,
                    detail: format!("{err}: {body}"),
                }],
            }],
        },
    }
}

fn wait_file(path: &Path, timeout: Duration) -> Result<String, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(body) = std::fs::read_to_string(path) {
            if !body.is_empty() {
                return Ok(body);
            }
        }
        if Instant::now() >= deadline {
            return Err(format!("timed out waiting for {}", path.display()));
        }
        thread::sleep(Duration::from_millis(200));
    }
}

fn wait_logcat(timeout: Duration) -> Result<String, String> {
    let deadline = Instant::now() + timeout;
    loop {
        let output = Command::new("adb").args(["logcat", "-d", "-s", "BUBBLE_EVAL:I"]).output().map_err(|err| err.to_string())?;
        let text = String::from_utf8_lossy(&output.stdout);
        if let Some(json) = text.lines().rev().find_map(|line| line.split("BUBBLE_EVAL").nth(1)) {
            let json = json.trim().trim_start_matches(':').trim();
            if json.starts_with('{') {
                return Ok(json.to_string());
            }
        }
        if Instant::now() >= deadline {
            return Err(format!("timed out waiting for BUBBLE_EVAL\n{text}"));
        }
        thread::sleep(Duration::from_millis(200));
    }
}
