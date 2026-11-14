#![allow(unused_unsafe)]
//! Headless test driver for a Bubble Shell.
//!
//! Two layers, mirroring `nsh-testing` + `tests/compat`:
//!
//! - [`driver`] is the Rust driver: a [`driver::Session`] owns one shell,
//!   pumps frames from a detached thread, and resolves selectors against the
//!   accessibility semantics snapshot.
//! - [`script`] is the declarative layer: line-based `*.script` files that
//!   drive one fixture app and assert on visible text.
//!
//! The driver talks to the same C ABI the iOS and Android hosts embed, so a
//! passing scenario exercises exactly the surface a device would load.

pub mod bundle;
pub mod driver;
pub mod manifest;
pub mod script;
pub mod semantics;
