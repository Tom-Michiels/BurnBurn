//! Fixtures of the integration tests of the presentation layer and the two hosts (feature `host`):
//! the controller rig and laser lab, the fake camera, the golf-5/6 panel documents and the fake GRBL
//! serial port. They used to be shared through `#[path]` between test binaries of different crates;
//! `cargo xtask path-attrs` now forbids that.
//!
//! Like the rest of this crate: use only from `tests/` integration tests (see the crate docs).

pub mod camera;
pub mod g5;
pub mod g6;
pub mod grbl_fake;
pub mod rig;
