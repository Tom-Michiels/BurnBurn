//! The SolidCraft-style tool strip in the real window: a button with an image for every catalog
//! id (variants in the group's drop-down), tooltips with shortcut or reason, clicks that choose
//! the tool in the controller, variant choice from the drop-down and icons drawn again when the
//! window turns dark.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("tool_strip: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("tool_strip_has_an_icon_per_tool_chooses_tools_and_follows_the_appearance", scenario())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use burnburn_editor::ToolKind;
    use burnburn_icons::Theme;
    use burnburn_macos::appkit::image_pixels;
    use burnburn_macos::toolbar::{reason_text, tag_for_id};
    use burnburn_presentation::{Action, Enablement, ToolCatalog};
    use common::expect;
    use objc2_app_kit::{
        NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSControlStateValueOn,
        NSImage,
    };

    fn has_ink(image: &NSImage) -> bool {
        image_pixels(image).is_some_and(|(w, h, px)| {
            w > 0
                && h > 0
                && px.as_chunks::<4>().0.iter().any(|p| p[3] > 0)
                && px.as_chunks::<4>().0.iter().any(|p| p[3] == 0)
        })
    }

    let s = common::start()?;
    let doc = s.new_document()?;
    let window = s.app.window(doc).ok_or("no window")?;
    // SAFETY: reading an immutable AppKit constant.
    let aqua = NSAppearance::appearanceNamed(unsafe { NSAppearanceNameAqua }).ok_or("no Aqua appearance")?;
    // SAFETY: as above.
    let dark = NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua }).ok_or("no DarkAqua appearance")?;
    window.setAppearance(Some(&aqua));
    s.pump(0.2);

    // Panel titles in the window's language (Dutch in tests).
    let titles = s.app.with_window(doc, |w| w.strip.panel_titles()).ok_or("no strip")?;
    expect(
        || format!("panel titles {titles:?}"),
        titles == ["MAKEN", "WIJZIGEN", "PATROON", "BEPERKINGEN", "INVOEGEN", "SELECTEREN"],
    )?;

    // Every catalog id: a button (single tool, or the group's face) or a drop-down entry, with a
    // non-empty image when artwork exists, else a non-empty title.
    let mut checked = 0;
    for spec in ToolCatalog::standard().specs() {
        let (image, title) = s
            .app
            .with_window(doc, |w| {
                let face = w.strip.button(spec.id);
                if let Some(b) = face {
                    return (b.image(), b.title().to_string());
                }
                let tag = tag_for_id(spec.id).unwrap_or(-1);
                let item = spec.group.and_then(|g| w.strip.menu(g)).and_then(|m| m.itemWithTag(tag));
                match item {
                    Some(i) => (i.image(), i.title().to_string()),
                    None => (None, String::new()),
                }
            })
            .ok_or("no window parts")?;
        if burnburn_icons::has_icon(spec.icon) {
            expect(|| format!("{} has no visible image", spec.id), image.as_deref().is_some_and(has_ink))?;
        } else {
            expect(|| format!("{} has neither artwork nor a title", spec.id), !title.trim().is_empty())?;
        }
        checked += 1;
    }
    expect(|| format!("{checked} ids checked"), checked == ToolCatalog::standard().specs().len())?;

    // Tooltips: label, shortcut and explanation; a disabled tool says why.
    let tip = |id: &str| -> Result<String, String> {
        s.app
            .with_window(doc, |w| w.strip.button(id).and_then(|b| b.toolTip()).map(|t| t.to_string()))
            .flatten()
            .ok_or(format!("no tooltip for {id}"))
    };
    let rect_tip = tip("sketch.rectangle.two_point")?;
    expect(
        || format!("rectangle tooltip {rect_tip:?}"),
        rect_tip.contains("(R) — ") && rect_tip.contains("Rechthoek"),
    )?;
    let vm = s.app.view();
    let disabled = vm
        .toolbar
        .buttons()
        .find(|b| matches!(b.enablement, Enablement::Disabled(_)) && b.icon.starts_with("sketch.constraint"))
        .ok_or("no disabled constraint tool")?;
    let Enablement::Disabled(reason) = disabled.enablement else { return Err("not disabled".into()) };
    let id = disabled.id;
    let enabled = s.app.with_window(doc, |w| w.strip.button(id).map(|b| b.isEnabled())).flatten().ok_or("no button")?;
    let disabled_tip = tip(id)?;
    expect(|| format!("{id} must be disabled"), !enabled)?;
    expect(
        || format!("{id} tooltip {disabled_tip:?}"),
        disabled_tip.ends_with(reason_text(reason, burnburn_macos::l10n::Locale::Nl)),
    )?;

    // A click on the rectangle button chooses the rectangle tool; it shows as pressed.
    let click = |id: &str| -> Result<(), String> {
        let b = s.app.with_window(doc, |w| w.strip.button(id)).flatten().ok_or(format!("no button {id}"))?;
        // SAFETY: a click on our own button, as the user's click would send it.
        unsafe { b.performClick(None) };
        s.pump(0.1);
        Ok(())
    };
    click("sketch.rectangle.two_point")?;
    expect(|| "rectangle tool active".into(), s.doc(doc).is_some_and(|d| d.tool == ToolKind::Rectangle))?;
    let pressed = |id: &str| -> bool {
        s.app.with_window(doc, |w| w.strip.button(id).map(|b| b.state() == NSControlStateValueOn)).flatten()
            == Some(true)
    };
    expect(|| "rectangle button pressed".into(), pressed("sketch.rectangle.two_point"))?;
    expect(|| "select button released".into(), !pressed("select"))?;

    // The variant 'center' from the drop-down chooses that tool and becomes the group's face.
    let menu = s.app.with_window(doc, |w| w.strip.menu("sketch.rectangle")).flatten().ok_or("no rectangle menu")?;
    let center_tag = tag_for_id("sketch.rectangle.center").ok_or("no tag")?;
    let index = menu.indexOfItemWithTag(center_tag);
    expect(|| format!("centre entry index {index}"), index >= 0)?;
    expect(|| "centre entry enabled".into(), menu.itemAtIndex(index).is_some_and(|i| i.isEnabled()))?;
    menu.performActionForItemAtIndex(index);
    s.pump(0.1);
    let want = ToolCatalog::standard()
        .get("sketch.rectangle.center")
        .and_then(|sp| if let Action::Tool(k) = sp.action { Some(k) } else { None })
        .ok_or("centre has no tool")?;
    expect(|| format!("tool is {want:?}"), s.doc(doc).is_some_and(|d| d.tool == want))?;
    expect(|| "group face follows".into(), pressed("sketch.rectangle.center"))?;

    // Choosing another tool keeps the remembered variant on the face.
    click("select")?;
    expect(
        || "face stays on the centre variant".into(),
        s.app.with_window(doc, |w| w.strip.button("sketch.rectangle.center").is_some()).unwrap_or(false),
    )?;

    // Dark appearance draws every icon again; light draws them back.
    let face = "select";
    let snapshot = || -> Result<(Vec<u8>, Option<Theme>, *const NSImage), String> {
        s.app
            .with_window(doc, |w| {
                let image = w.strip.button(face)?.image()?;
                let ptr: *const NSImage = &*image;
                Some((image_pixels(&image)?.2, w.strip.drawn_theme(), ptr))
            })
            .flatten()
            .ok_or("no image".to_owned())
    };
    let (light_px, light_theme, light_ptr) = snapshot()?;
    expect(|| format!("drawn in light, was {light_theme:?}"), light_theme == Some(Theme::Light))?;
    window.setAppearance(Some(&dark));
    s.wait(2.0, "the icons to be drawn dark", || {
        s.app.with_window(doc, |w| w.strip.drawn_theme()).flatten() == Some(Theme::Dark)
    })?;
    let (dark_px, dark_theme, dark_ptr) = snapshot()?;
    expect(|| format!("drawn in dark, was {dark_theme:?}"), dark_theme == Some(Theme::Dark))?;
    expect(|| "the dark image is a new image".into(), dark_ptr != light_ptr)?;
    expect(|| "dark pixels differ from light pixels".into(), dark_px != light_px)?;
    window.setAppearance(Some(&aqua));
    s.wait(2.0, "the icons to be drawn light again", || {
        s.app.with_window(doc, |w| w.strip.drawn_theme()).flatten() == Some(Theme::Light)
    })?;
    let (again_px, _, _) = snapshot()?;
    expect(|| "light pixels are back".into(), again_px == light_px)?;
    Ok(())
}
