#[test]
fn m1_suite_passes() {
    let report = bubble_eval::run_m1();
    assert!(
        report.ok,
        "{}",
        serde_json::to_string_pretty(&report).unwrap()
    );
}
