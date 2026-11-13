//! Install a host, launch it on a booted simulator, and read the JSON report
//! the painter writes. iOS is built with xcodebuild; Android with Gradle.

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



pub fn ios() -> Result<SuiteReport, String> {
    ios_device(false)
}

pub fn gestures_ios() -> Result<SuiteReport, String> {
    ios_device(true)
}

fn build_ios(root: &Path) -> Result<PathBuf, String> {
    let derived = root.join("examples/ios/DerivedData");
    run(Command::new("xcodebuild")
        .arg("-project")
        .arg(root.join("sdk/ios/BubbleShell.xcodeproj"))
        .args([
            "-scheme",
            "BubbleShell",
            "-configuration",
            "Release",
            "-destination",
            "generic/platform=iOS Simulator",
            "-derivedDataPath",
        ])
        .arg(&derived)
        .arg("CODE_SIGNING_ALLOWED=NO")
        .current_dir(root))?;
    let app = derived.join("Build/Products/Release-iphonesimulator/BubbleShell.app");
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

fn apk(root: &Path, tail: &str) -> PathBuf {
    root.join("examples/android/app/build/outputs").join(tail)
}

fn build_android(root: &Path) -> Result<(), String> {
    run(Command::new(root.join("sdk/android/gradlew"))
        .args([":demo:assembleDebug", ":demo:assembleDebugAndroidTest"])
        .current_dir(root.join("sdk/android")))?;
    run(Command::new("adb")
        .args(["install", "-r"])
        .arg(apk(root, "apk/debug/BubbleShell-debug.apk")))?;
    let _ = run(Command::new("adb").args(["shell", "am", "force-stop", "dev.bubble.shell"]));
    let _ = run(Command::new("adb").args(["logcat", "-c"]));
    Ok(())
}

fn install_android_tests(root: &Path) -> Result<(), String> {
    run(Command::new("adb").args(["install", "-r"]).arg(apk(
        root,
        "apk/androidTest/debug/BubbleShell-debug-androidTest.apk",
    )))?;
    Ok(())
}

pub fn editing_android() -> Result<SuiteReport, String> {
    let root = workspace();
    build_android(&root)?;
    install_android_tests(&root)?;
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
    build_android(&root)?;
    if gestures {
        install_android_tests(&root)?;
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
