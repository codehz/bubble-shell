fn main() {
    let which = std::env::args().nth(1);
    let report = match which.as_deref() {
        None => bubble_eval::run_m1(),
        Some("ios") => bubble_eval::ios().unwrap_or_else(failed),
        Some("android") => bubble_eval::android().unwrap_or_else(failed),
        Some(other) => failed(format!("unknown target {other}; use ios or android")),
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
