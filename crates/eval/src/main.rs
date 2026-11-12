fn main() {
    let which = std::env::args().nth(1);
    let report = match which.as_deref() {
        None => bubble_eval::run_m1(),
        Some("ios") => bubble_eval::ios().unwrap_or_else(failed),
        Some("android") => bubble_eval::android().unwrap_or_else(failed),
        Some("gestures-ios") => bubble_eval::gestures_ios().unwrap_or_else(failed),
        Some("gestures-android") => bubble_eval::gestures_android().unwrap_or_else(failed),
        Some("editing-ios") => bubble_eval::editing_ios().unwrap_or_else(failed),
        Some("editing-android") => bubble_eval::editing_android().unwrap_or_else(failed),
        Some("focus-android") => bubble_eval::focus_android().unwrap_or_else(failed),
        Some("focus-ios") => bubble_eval::focus_ios().unwrap_or_else(failed),
        Some("editing-web") => bubble_eval::web().unwrap_or_else(failed),
        Some(other) => failed(format!(
            "unknown target {other}; use ios, android, gestures-ios, gestures-android, editing-ios, editing-android, editing-web, focus-android or focus-ios"
        )),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("report serializes")
    );
    if !report.ok {
        std::process::exit(1);
    }
}

fn failed(message: String) -> bubble_eval::SuiteReport {
    bubble_eval::SuiteReport {
        suite: "device".into(),
        ok: false,
        scenarios: vec![bubble_eval::ScenarioReport {
            name: "device".into(),
            ok: false,
            elapsed_ms: 0,
            checks: vec![bubble_eval::Check {
                name: "run".into(),
                ok: false,
                detail: message,
            }],
        }],
    }
}
