#![allow(unused_unsafe)]
//! Headless evaluation of a Bubble Shell.
//!
//! A [`Session`] drives the native C ABI with no window. Scenarios record
//! [`Check`]s into a [`SuiteReport`] instead of shelling out to a platform
//! build. Device painters stay outside this crate: a later host driver can
//! implement the same steps.

mod device;
mod driver;
mod report;
mod suite;

pub use device::{
    android, editing_android, editing_ios, focus_android, gestures_android, gestures_ios, ios, web,
};
pub use report::{Check, Probe, ScenarioReport, SuiteReport};
pub use suite::m1;

/// Run the native milestone scenarios and fold them into one report.
pub fn run_m1() -> SuiteReport {
    let scenarios = m1();
    SuiteReport {
        suite: "m1".into(),
        ok: scenarios.iter().all(|scenario| scenario.ok),
        scenarios,
    }
}
