//! Every `scripts/*.script` file runs against its fixture app. The gesture
//! choreography the DSL cannot express (pan, long press) runs as a Rust test
//! against the same driver.

use std::path::PathBuf;

use bubble_testing::driver::{Selector, Session};
use bubble_testing::script::run_file;

fn scripts_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts")
}

#[test]
fn every_script_runs() {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(scripts_dir())
        .expect("scripts directory lists")
        .map(|entry| entry.expect("script entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "script"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no scripts found");
    for path in paths {
        if let Err(error) = run_file(&path) {
            let name = path.file_name().unwrap().to_string_lossy();
            panic!("{name}: {error}");
        }
    }
}

/// Pan and long press need timed pointer samples the script DSL does not
/// describe; they drive the same fixture through the driver directly.
#[test]
fn pan_and_long_press_reach_the_script() {
    let app = bubble_testing::bundle::app("gestures").expect("gestures fixture builds");
    let session = Session::start(&app).expect("session starts");

    // Pan: down, a sweep of moves past minDistance, up.
    let (x, y) = session
        .center(&Selector::Id("pan-area".into()))
        .expect("pan area resolves");
    let started = std::time::Instant::now();
    let ms = |since: std::time::Instant| since.elapsed().as_secs_f64() * 1000.0;
    session.pointer(0, x, y, ms(started));
    for step in 1..=6 {
        session.pointer(1, x + 20.0 * step as f32, y, ms(started));
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
    session.pointer(2, x + 120.0, y, ms(started));
    session
        .wait_for("panned")
        .expect("pan end reaches the script");

    // Long press: down and hold past the fixture's 400 ms duration.
    let (x, y) = session
        .center(&Selector::Id("long-area".into()))
        .expect("long press area resolves");
    let started = std::time::Instant::now();
    session.pointer(0, x, y, ms(started));
    session
        .wait_for("long-pressed")
        .expect("long press fires while the pointer is down");
    session.pointer(2, x, y, ms(started));
}
