fn main() {
    let report = bubble_eval::run_m1();
    println!("{}", serde_json::to_string_pretty(&report).expect("report serializes"));
    if !report.ok {
        std::process::exit(1);
    }
}
