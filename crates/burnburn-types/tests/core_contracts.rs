//! Public contract: the core builds and runs in a plain test process, with no native GUI,
//! GPU or platform SDK linked in.

use burnburn_types::Length;

#[test]
fn core_contracts_run_without_native_gui() {
    let width = Length::from_mm(120.0).expect("finite");
    assert_eq!(width.mm(), 120.0);
}
