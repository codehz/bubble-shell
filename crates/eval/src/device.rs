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
    #[serde(default)]
    elapsed_ms: u128,
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
        return Err(format!(
            "{cmd:?} exited {}\n{stdout}{stderr}",
            output.status
        ));
    }
    Ok(stdout)
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::copy(from, to)
        .map(|_| ())
        .map_err(|err| format!("copy {from:?}: {err}"))
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|err| err.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            copy(&entry.path(), &target)?;
        }
    }
    Ok(())
}

fn install_fixtures(root: &Path, destination: &Path) -> Result<(), String> {
    copy_dir(
        &root.join("examples/web/fixtures"),
        &destination.join("fixtures"),
    )
}

pub fn ios() -> Result<SuiteReport, String> {
    ios_device(false)
}

pub fn gestures_ios() -> Result<SuiteReport, String> {
    ios_device(true)
}

fn build_ios(root: &Path) -> Result<PathBuf, String> {
    run(Command::new("cargo")
        .args([
            "build",
            "-p",
            "bubble-ffi",
            "--release",
            "--target",
            "aarch64-apple-ios-sim",
        ])
        .current_dir(root))?;
    let sdk = run(Command::new("xcrun").args(["--sdk", "iphonesimulator", "--show-sdk-path"]))?;
    let sdk = sdk.trim();
    let app = root.join("examples/ios/BubbleShell.app");
    let _ = std::fs::remove_dir_all(&app);
    std::fs::create_dir_all(&app).map_err(|err| err.to_string())?;
    copy(
        &root.join("examples/ios/Info.plist"),
        &app.join("Info.plist"),
    )?;
    install_fixtures(root, &app)?;
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
        .arg(root.join("sdk/ios/Shell.swift"))
        .arg(root.join("examples/ios/Sources/main.swift")))?;
    run(Command::new("xcrun")
        .args(["simctl", "install", "booted"])
        .arg(&app))?;
    Ok(app)
}

fn ios_container() -> Result<PathBuf, String> {
    let container = run(Command::new("xcrun").args([
        "simctl",
        "get_app_container",
        "booted",
        "dev.bubble.shell",
        "data",
    ]))?;
    Ok(PathBuf::from(container.trim()))
}

pub fn editing_ios() -> Result<SuiteReport, String> {
    let root = workspace();
    build_ios(&root)?;
    let result = ios_container()?.join("Documents/edit-result.json");
    let _ = std::fs::remove_file(&result);
    run(Command::new("xcrun")
        .args([
            "simctl",
            "launch",
            "--terminate-running-process",
            "booted",
            "dev.bubble.shell",
        ])
        .env("SIMCTL_CHILD_BUBBLE_EDIT", "1")
        .env("SIMCTL_CHILD_BUBBLE_FIXTURE", "editing"))?;
    let body = wait_file(&result, Duration::from_secs(40))?;
    Ok(decode("edit.ios", &body))
}

fn ios_device(gestures: bool) -> Result<SuiteReport, String> {
    let root = workspace();
    build_ios(&root)?;
    let result = ios_container()?.join(if gestures {
        "Documents/gestures-result.json"
    } else {
        "Documents/m1-result.json"
    });
    // The install keeps the data container, so a previous run's report would
    // otherwise be read before the new process writes its own.
    let _ = std::fs::remove_file(&result);
    let udid = booted_simulator()?;
    let mut test = Command::new("xcodebuild");
    if let Ok(only) = std::env::var("BUBBLE_GESTURE_ONLY") {
        test.env("TEST_RUNNER_BUBBLE_GESTURE_ONLY", only);
    }
    let tested = run(test
        .args(["test", "-quiet", "-project"])
        .arg(root.join("examples/ios/BubbleShellUITests.xcodeproj"))
        .args(["-scheme", "BubbleShellUITests", "-destination"])
        .arg(format!("platform=iOS Simulator,id={udid}"))
        .args(["-derivedDataPath"])
        .arg(root.join("target/ios-uitest"))
        .arg("-parallel-testing-enabled")
        .arg("NO")
        .arg(if gestures {
            "-only-testing:BubbleShellUITests/BubbleShellUITests/testGestureAcceptance"
        } else {
            "-only-testing:BubbleShellUITests/BubbleShellUITests/testSwipeScrollsTheList"
        }));
    if let Err(err) = tested {
        if gestures {
            if let Ok(body) = std::fs::read_to_string(&result) {
                let mut report = decode("gestures.ios", &body);
                report.ok = false;
                if let Some(scenario) = report.scenarios.first_mut() {
                    scenario.ok = false;
                    scenario.checks.push(Check {
                        name: "XCTest execution".into(),
                        ok: false,
                        detail: err,
                    });
                }
                return Ok(report);
            }
        }
        return Err(err);
    }
    let body = wait_file(&result, Duration::from_secs(40))?;
    Ok(decode(
        if gestures { "gestures.ios" } else { "m1.ios" },
        &body,
    ))
}

pub fn android() -> Result<SuiteReport, String> {
    android_device(false)
}

pub fn gestures_android() -> Result<SuiteReport, String> {
    android_device(true)
}

fn android_tools(root: &Path) -> Result<std::collections::HashMap<String, String>, String> {
    let text = run(Command::new("sh")
        .arg(root.join("sdk/android/env.sh"))
        .current_dir(root))?;
    let mut tools = std::collections::HashMap::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        tools.insert(key.to_string(), value.to_string());
    }
    for key in ["CC", "AR", "SYSROOT", "BUILD_TOOLS", "ANDROID_JAR", "ANDROID_PLATFORM", "KOTLINC", "KOTLIN_LIB"] {
        if !tools.contains_key(key) {
            return Err(format!("sdk/android/env.sh did not report {key}"));
        }
    }
    Ok(tools)
}

fn build_android(root: &Path) -> Result<PathBuf, String> {
    let tools = android_tools(root)?;
    let cc = tools["CC"].clone();
    let ar = tools["AR"].clone();
    let sysroot = tools["SYSROOT"].clone();
    run(Command::new("cargo")
        .args([
            "build",
            "-p",
            "bubble-ffi",
            "--release",
            "--target",
            "aarch64-linux-android",
        ])
        .env("CC_aarch64_linux_android", &cc)
        .env("AR_aarch64_linux_android", &ar)
        .env("CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER", &cc)
        .env(
            "BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android",
            format!("--sysroot={sysroot}"),
        )
        .current_dir(root))?;
    let kotlinc = &tools["KOTLINC"];
    let bt = PathBuf::from(&tools["BUILD_TOOLS"]);
    let android_jar = &tools["ANDROID_JAR"];
    let platform = &tools["ANDROID_PLATFORM"];
    let out = root.join("examples/android/build");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(out.join("apk/lib/arm64-v8a")).map_err(|err| err.to_string())?;
    install_fixtures(root, &out.join("apk/assets"))?;
    let font = Path::new("/System/Library/Fonts/Supplemental/Arial.ttf");
    if font.exists() {
        copy(font, &out.join("apk/assets/HostFont.ttf"))?;
    }
    copy(
        &root.join("target/aarch64-linux-android/release/libbubble_ffi.so"),
        &out.join("apk/lib/arm64-v8a/libbubble_ffi.so"),
    )?;
    let sdk_src = root.join("sdk/android/src/dev/bubble/shell");
    let app_src = root.join("examples/android/app/src/main/java/dev/bubble/shell");
    run(Command::new(kotlinc)
        .args(["-jvm-target", "17", "-classpath", android_jar, "-d"])
        .arg(out.join("classes.jar"))
        .arg(sdk_src.join("BubbleView.kt"))
        .arg(sdk_src.join("BubbleNative.kt"))
        .arg(sdk_src.join("FieldLayout.kt"))
        .arg(sdk_src.join("HostCalls.kt"))
        .arg(app_src.join("MainActivity.kt")))?;
    run(Command::new(bt.join("d8"))
        .args(["--min-api", "24", "--lib", android_jar, "--output"])
        .arg(out.join("apk"))
        .arg(out.join("classes.jar"))
        .arg(PathBuf::from(&tools["KOTLIN_LIB"]).join("kotlin-stdlib.jar")))?;
    run(Command::new(bt.join("aapt2"))
        .args(["link", "-o"])
        .arg(out.join("base.apk"))
        .args(["-I", android_jar, "--manifest"])
        .arg(root.join("examples/android/app/src/main/AndroidManifest.xml"))
        .args(["--min-sdk-version", "24", "--target-sdk-version", platform]))?;
    run(Command::new("unzip")
        .args(["-q", "-o"])
        .arg(out.join("base.apk"))
        .args(["-d"])
        .arg(out.join("apk")))?;
    let unsigned = out.join("BubbleShell-unsigned.apk");
    let _ = std::fs::remove_file(&unsigned);
    run(Command::new("zip")
        .args(["-qr", "-n", "arsc"])
        .arg(&unsigned)
        .arg(".")
        .current_dir(out.join("apk")))?;
    let aligned = out.join("BubbleShell-aligned.apk");
    run(Command::new(bt.join("zipalign"))
        .args(["-p", "-f", "4"])
        .arg(&unsigned)
        .arg(&aligned))?;
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
    run(Command::new(bt.join("apksigner"))
        .args([
            "sign",
            "--ks",
            keystore.to_str().unwrap_or_default(),
            "--ks-pass",
            "pass:android",
            "--key-pass",
            "pass:android",
            "--out",
            apk.to_str().unwrap_or_default(),
        ])
        .arg(&aligned))?;
    run(Command::new("adb").args(["install", "-r"]).arg(&apk))?;
    let _ = run(Command::new("adb").args(["shell", "am", "force-stop", "dev.bubble.shell"]));
    let _ = run(Command::new("adb").args(["logcat", "-c"]));
    Ok(out)
}

fn install_android_tests(root: &Path, out: &Path) -> Result<(), String> {
    run(Command::new("sh")
        .arg(root.join("examples/android/build-tests.sh"))
        .current_dir(root))?;
    run(Command::new("adb")
        .args(["install", "-r"])
        .arg(out.join("BubbleShellTests.apk")))?;
    Ok(())
}

pub fn editing_android() -> Result<SuiteReport, String> {
    let root = workspace();
    let out = build_android(&root)?;
    install_android_tests(&root, &out)?;
    let _ = run(Command::new("adb").args([
        "shell",
        "run-as",
        "dev.bubble.shell",
        "rm",
        "-f",
        "files/edit-result.json",
    ]));
    let output = run(Command::new("adb").args([
        "shell",
        "am",
        "instrument",
        "-w",
        "dev.bubble.shell.test/.EditInstrumentation",
    ]))?;
    let body = run(Command::new("adb").args([
        "shell",
        "run-as",
        "dev.bubble.shell",
        "cat",
        "files/edit-result.json",
    ]))
    .map_err(|err| format!("{err}\n{output}"))?;
    Ok(decode("edit.android", &body))
}

fn android_device(gestures: bool) -> Result<SuiteReport, String> {
    let root = workspace();
    let out = build_android(&root)?;
    if gestures {
        install_android_tests(&root, &out)?;
        run(Command::new("adb").args([
            "shell",
            "run-as",
            "dev.bubble.shell",
            "rm",
            "-f",
            "files/gestures-result.json",
        ]))?;
        let output = run(Command::new("adb").args([
            "shell",
            "am",
            "instrument",
            "-w",
            "dev.bubble.shell.test/.ShellInstrumentation",
        ]))?;
        let body = run(Command::new("adb").args([
            "shell",
            "run-as",
            "dev.bubble.shell",
            "cat",
            "files/gestures-result.json",
        ]))
        .map_err(|err| format!("{err}\n{output}"))?;
        return Ok(decode("gestures.android", &body));
    }
    run(Command::new("adb").args([
        "shell",
        "am",
        "start",
        "-n",
        "dev.bubble.shell/.MainActivity",
        "--ez",
        "bubble_eval",
        "true",
    ]))?;
    let (left, top, right, bottom) = wait_scroll_rect(Duration::from_secs(10))?;
    let x = (left + right) / 2;
    let start = top + (bottom - top) * 7 / 10;
    let end = top + (bottom - top) * 3 / 10;
    run(Command::new("adb")
        .args(["shell"])
        .arg(format!("input swipe {x} {start} {x} {end} 300")))?;
    let body = wait_logcat(Duration::from_secs(20))?;
    Ok(decode("m1.android", &body))
}

pub fn web() -> Result<SuiteReport, String> {
    let root = workspace();
    let output = Command::new("node")
        .arg("--test")
        .arg(root.join("examples/web/ime.test.mjs"))
        .arg(root.join("examples/web/a11y.test.mjs"))
        .output()
        .map_err(|err| format!("node --test: {err}"))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let ok = output.status.success();
    let detail = text
        .lines()
        .rev()
        .take(12)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    let scenario = ScenarioReport {
        name: "edit.web".into(),
        ok,
        elapsed_ms: 0,
        checks: vec![Check {
            name: "web ime and accessibility key mapping".into(),
            ok,
            detail,
        }],
    };
    Ok(SuiteReport {
        suite: "edit.web".into(),
        ok,
        scenarios: vec![scenario],
    })
}

fn decode(name: &str, body: &str) -> SuiteReport {
    match serde_json::from_str::<DeviceReport>(body) {
        Ok(report) => SuiteReport {
            suite: report.suite,
            ok: report.ok,
            scenarios: vec![ScenarioReport {
                name: name.into(),
                ok: report.ok,
                elapsed_ms: report.elapsed_ms,
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

fn booted_simulator() -> Result<String, String> {
    let output = run(Command::new("xcrun").args(["simctl", "list", "devices", "booted", "-j"]))?;
    let value: serde_json::Value =
        serde_json::from_str(&output).map_err(|err| format!("simctl list: {err}"))?;
    value
        .get("devices")
        .and_then(|devices| devices.as_object())
        .and_then(|devices| {
            devices
                .values()
                .filter_map(|list| list.as_array())
                .flatten()
                .find(|device| {
                    device.get("state").and_then(|state| state.as_str()) == Some("Booted")
                })
                .and_then(|device| device.get("udid").and_then(|udid| udid.as_str()))
                .map(str::to_owned)
        })
        .ok_or_else(|| "no booted simulator".to_string())
}

fn wait_scroll_rect(timeout: Duration) -> Result<(i64, i64, i64, i64), String> {
    let deadline = Instant::now() + timeout;
    loop {
        let output = Command::new("adb")
            .args(["logcat", "-d", "-s", "BUBBLE_EVAL:I"])
            .output()
            .map_err(|err| err.to_string())?;
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines().rev() {
            let Some(rest) = line.split("BUBBLE_SCROLL").nth(1) else {
                continue;
            };
            let numbers: Vec<i64> = rest
                .split_whitespace()
                .filter_map(|token| token.trim_start_matches(':').parse().ok())
                .collect();
            if numbers.len() >= 4 {
                return Ok((numbers[0], numbers[1], numbers[2], numbers[3]));
            }
        }
        if Instant::now() >= deadline {
            return Err(format!("timed out waiting for BUBBLE_SCROLL\n{text}"));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn wait_logcat(timeout: Duration) -> Result<String, String> {
    let deadline = Instant::now() + timeout;
    loop {
        let output = Command::new("adb")
            .args(["logcat", "-d", "-s", "BUBBLE_EVAL:I"])
            .output()
            .map_err(|err| err.to_string())?;
        let text = String::from_utf8_lossy(&output.stdout);
        if let Some(json) = text
            .lines()
            .rev()
            .find_map(|line| line.split("BUBBLE_EVAL").nth(1))
        {
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
