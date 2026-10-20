//! W2-L1 (bewerken), headless: the toolbar's Uitlijnen control (align and distribute segments).

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_macos::panels::{ARRANGE_TOOLBAR, arrange_toolbar_command, arrange_toolbar_enabled};
use burnburn_presentation::{CommandKey, HeadlessEditorHarness};

#[test]
fn the_control_has_the_six_align_and_two_distribute_commands_in_menu_order() {
    assert_eq!(
        ARRANGE_TOOLBAR,
        [
            CommandKey::AlignLeft,
            CommandKey::AlignCenterX,
            CommandKey::AlignRight,
            CommandKey::AlignTop,
            CommandKey::AlignCenterY,
            CommandKey::AlignBottom,
            CommandKey::DistributeHorizontal,
            CommandKey::DistributeVertical,
        ]
    );
    for (i, key) in ARRANGE_TOOLBAR.iter().enumerate() {
        assert_eq!(arrange_toolbar_command(i as isize), Some(*key));
    }
    assert_eq!(arrange_toolbar_command(8), None);
    assert_eq!(arrange_toolbar_command(-1), None, "AppKit reports -1 when no segment is selected");
}

/// Enablement comes from the same command table as the menu: align needs two objects, distribute three.
#[test]
fn segments_are_enabled_exactly_as_the_view_model_enables_the_commands() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    assert_eq!(arrange_toolbar_enabled(&h.view()), vec![false; 8], "no document");
    h.invoke(doc, CommandKey::ToolRectangle);
    for i in 0..3 {
        let x = f64::from(i) * 60.0;
        h.drag(doc, (x, 0.0), (x + 40.0, 30.0));
    }
    h.invoke(doc, CommandKey::SelectAll);
    let vm = h.view();
    let expected: Vec<bool> = ARRANGE_TOOLBAR.iter().map(|k| vm.commands[k].enabled).collect();
    assert_eq!(arrange_toolbar_enabled(&vm), expected);
    assert!(expected.iter().all(|e| *e), "three selected objects can be aligned and distributed: {expected:?}");
}
