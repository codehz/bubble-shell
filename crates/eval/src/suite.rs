//! Built-in milestone scenarios. Each one owns a [`Session`] for its whole run.
//! They assert the native core, not a device painter.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use crate::driver::{frame_detached, Session};
use crate::report::{Probe, ScenarioReport};

const MANIFEST: &str = include_str!("../../../examples/web/bundle/manifest.json");
const ENTRY: &[u8] = include_bytes!("../../../examples/web/bundle/main.js");

fn load_counter(session: &mut Session, probe: &mut Probe) -> bool {
    match session.load(MANIFEST, ENTRY) {
        Ok(()) => true,
        Err(err) => {
            probe.check("load counter bundle", false, err);
            false
        }
    }
}

fn open(probe: &mut Probe) -> Option<Session> {
    match Session::create() {
        Ok(session) => Some(session),
        Err(err) => {
            probe.check("create", false, err);
            None
        }
    }
}

fn counter(probe: &mut Probe) {
    let Some(mut session) = open(probe) else {
        return;
    };
    if !load_counter(&mut session, probe) {
        return;
    }
    match session.frame(320.0, 180.0) {
        Ok(paint) => probe.check(
            "first frame paint",
            paint.len() > 16,
            format!("{} bytes", paint.len()),
        ),
        Err(err) => probe.check("first frame paint", false, err),
    }
    match session.node_text(3) {
        Ok(text) => probe.eq("label before tap", text, "Count: 0".to_string()),
        Err(err) => probe.check("label before tap", false, err),
    }
    let (x, y, w, h) = match session.node_box(4) {
        Ok(box_) => box_,
        Err(err) => {
            probe.check("button box", false, err);
            return;
        }
    };
    probe.check(
        "button has size",
        w > 0.0 && h > 0.0,
        format!("{w}x{h} at {x},{y}"),
    );
    let generation = session.generation();
    session.pointer_down(x + w / 2.0, y + h / 2.0);
    let tapped = session.wait_until(Duration::from_secs(2), |session| {
        session.generation() != generation
    });
    probe.check(
        "tap reached script",
        tapped,
        format!("generation {generation}"),
    );
    match session.node_text(3) {
        Ok(text) => probe.eq("label after tap", text, "Count: 1".to_string()),
        Err(err) => probe.check("label after tap", false, err),
    }
    let (sx, sy, _, _) = match session.node_box(6) {
        Ok(box_) => box_,
        Err(err) => {
            probe.check("scroller box", false, err);
            return;
        }
    };
    session.scroll(sx + 1.0, sy + 1.0, 0.0, 40.0);
    let offset = session.scroll_y_of(6);
    let horizontal = session.scroll_x_of(6);
    probe.check("scroll offset", offset > 0.0, format!("{offset}"));
    probe.check(
        "vertical only stays at x=0",
        horizontal == 0.0,
        format!("{horizontal}"),
    );
    if let Err(err) = session.register_font("Demo", b"font-bytes") {
        probe.check("register font", false, err);
        return;
    }
    probe.eq("font count", session.font_count(), 1);
    probe.eq("font epoch", session.font_epoch(), 1);
}

fn dead_loop(probe: &mut Probe) {
    let Some(mut session) = open(probe) else {
        return;
    };
    if !load_counter(&mut session, probe) {
        return;
    }
    let frames = AtomicU32::new(0);
    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !stop.load(Ordering::Relaxed) {
                if frame_detached(200.0, 200.0).is_err() {
                    break;
                }
                frames.fetch_add(1, Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(8));
            }
        });
        let _ = session.post_eval("while (true) {}");
        let deadline = Instant::now() + Duration::from_secs(4);
        while session.faults() == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        stop.store(true, Ordering::Relaxed);
    });
    probe.check(
        "interrupt recorded",
        session.faults() >= 1,
        session.status(),
    );
    probe.eq(
        "status after interrupt",
        session.status(),
        "running".to_string(),
    );
    probe.check(
        "frames during hang",
        frames.load(Ordering::Relaxed) >= 10,
        format!("{}", frames.load(Ordering::Relaxed)),
    );
    match session.eval("1 + 1") {
        Ok(value) => probe.eq("eval after recovery", value, "2".to_string()),
        Err(err) => probe.check("eval after recovery", false, err),
    }
}

fn timers_and_host(probe: &mut Probe) {
    let Some(mut session) = open(probe) else {
        return;
    };
    if !load_counter(&mut session, probe) {
        return;
    }
    session.set_visible(false);
    match session
        .eval("globalThis.ticks = 0; setTimeout(() => { globalThis.ticks = 1 }, 0); 'armed'")
    {
        Ok(value) => probe.eq("timer armed", value, "armed".to_string()),
        Err(err) => {
            probe.check("timer armed", false, err);
            return;
        }
    }
    match session.eval("String(globalThis.ticks)") {
        Ok(value) => probe.eq("timer paused while hidden", value, "0".to_string()),
        Err(err) => probe.check("timer paused while hidden", false, err),
    }
    session.set_visible(true);
    let mut seen = String::new();
    let fired = session.wait_until(Duration::from_secs(2), |session| {
        seen = session.eval("String(globalThis.ticks)").unwrap_or_default();
        seen == "1"
    });
    probe.check("timer fires when visible", fired, seen);
    match session.eval(
        "bubbleHost.call('toast', { message: 'hi' }).then((value) => { globalThis.hostValue = value.ok }); 'queued'",
    ) {
        Ok(value) => probe.eq("host call queued", value, "queued".to_string()),
        Err(err) => {
            probe.check("host call queued", false, err);
            return;
        }
    }
    match session.poll_request() {
        Ok(Some(request)) => {
            probe.eq("host method", request.method, "toast".to_string());
            probe.eq(
                "host params",
                request.params,
                "{\"message\":\"hi\"}".to_string(),
            );
            if let Err(err) = session.finish_request(request.id, true, "{\"ok\":true}") {
                probe.check("finish host request", false, err);
                return;
            }
        }
        other => {
            probe.check("host request polled", false, format!("{other:?}"));
            return;
        }
    }
    let mut host = String::new();
    let settled = session.wait_until(Duration::from_secs(2), |session| {
        host = session
            .eval("String(globalThis.hostValue)")
            .unwrap_or_default();
        host == "true"
    });
    probe.check("host result settled", settled, host);
}

pub fn m1() -> Vec<ScenarioReport> {
    [
        ("m1.counter", counter as fn(&mut Probe)),
        ("m1.dead_loop", dead_loop),
        ("m1.timers_and_host", timers_and_host),
    ]
    .into_iter()
    .map(|(name, run)| {
        let started = Instant::now();
        let mut probe = Probe::new();
        run(&mut probe);
        let failed = probe.failed();
        ScenarioReport {
            name: name.to_string(),
            ok: !failed,
            elapsed_ms: started.elapsed().as_millis(),
            checks: probe.into_checks(),
        }
    })
    .collect()
}
