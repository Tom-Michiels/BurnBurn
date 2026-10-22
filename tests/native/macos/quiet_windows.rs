//! Quiet native tests (BURNBURN_NATIVE_QUIET=1, the default of `cargo xtask verify`): the host's
//! windows — the document window and the panels it opens — are fully transparent, let clicks
//! through to the apps below, stay at the normal window level and never make the test app the
//! active one, so the computer stays usable while the native tests run.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::process::ExitCode;

use burnburn_macos::adapter::MenuItem;
use burnburn_macos::appkit::HostOptions;
use burnburn_presentation::CommandKey;
use common::*;
use objc2_app_kit::{NSApplication, NSNormalWindowLevel};

fn scenario() -> Result<(), String> {
    let s = start_with(|o: &mut HostOptions| {
        o.quiet_windows = true;
        o.float_windows = false;
    })?;
    s.new_document()?;
    s.menu(MenuItem::Command(CommandKey::ShowMaterialLibrary))?;
    s.pump(0.3);
    let app = NSApplication::sharedApplication(s.mtm);
    let windows = app.windows();
    expect(|| "the host opened windows".into(), windows.count() >= 2)?;
    for w in windows.iter() {
        let title = w.title().to_string();
        expect(|| format!("{title}: transparent (alpha {})", w.alphaValue()), w.alphaValue() == 0.0)?;
        expect(|| format!("{title}: lets clicks through"), w.ignoresMouseEvents())?;
        expect(|| format!("{title}: normal level (not floating)"), w.level() == NSNormalWindowLevel)?;
    }
    expect(|| "the test app never takes the focus".into(), !app.isActive())?;
    Ok(())
}

fn main() -> ExitCode {
    report("quiet_windows_are_transparent_and_take_no_focus", scenario())
}
