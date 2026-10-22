//! Test-only support for BurnBurn: a manually driven [`FakeClock`], a generic scripted fake
//! peer ([`ScriptedPeer`]) and checked access to the fixtures under `test-data/`.
//!
//! # Use only from `tests/` integration tests
//!
//! Add this crate as a `[dev-dependencies]` entry and use it from `crates/<crate>/tests/*.rs`,
//! never from `#[cfg(test)]` unit tests inside `src/`. This crate depends on `burnburn-types`
//! (and later possibly other lower crates). A unit test of such a crate is a *separate*
//! compilation of that crate, so the types it sees are not the types this crate was built
//! against (two copies of the crate in one test binary, and a dev-dependency cycle). Integration
//! tests link the normal library build, so there is exactly one copy. `cargo xtask layers`
//! rejects this crate as a normal (non-dev) dependency.
//!
//! This crate deliberately does not depend on `burnburn-machine` or `burnburn-ruida`: protocol
//! crates wrap [`ScriptedPeer`] with their own request/response types in their own tests.

mod clock;
pub mod fixtures;
#[cfg(feature = "host")]
pub mod host;
mod peer;

pub use clock::FakeClock;
pub use peer::{PeerError, ScriptedPeer, SendOutcome, SentRecord, Step};
