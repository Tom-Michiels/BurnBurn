//! Shared fixture access: hash-checked reads from `test-data/ruida/` via `burnburn-test-support`.
#![allow(clippy::unwrap_used, dead_code)] // test helpers outside #[test] fns

use burnburn_test_support::fixtures::read_ruida_fixture;

/// A real LightBurn export, checked against `test-data/ruida/manifest.json`.
pub fn fixture(name: &str) -> Vec<u8> {
    read_ruida_fixture(name).unwrap()
}

/// The independent Python decode (`rd_inspect.py --json`) for a fixture.
pub fn oracle(name: &str) -> serde_json::Value {
    serde_json::from_slice(&read_ruida_fixture(&format!("oracle/{name}")).unwrap()).unwrap()
}
