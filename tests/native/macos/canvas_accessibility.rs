//! IM20 accessibility: canvas objects are NSAccessibility elements (layout items with the object's
//! name, size and on-screen frame) inside the canvas layout area; inspector fields and the object
//! list carry their own labels. Read in-process through the NSAccessibility API (no AX permission).

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("canvas_accessibility: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("canvas_objects_are_native_accessibility_elements", scenario())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use burnburn_presentation::FieldKey;
    use common::expect;
    use objc2::Message;
    use objc2::runtime::AnyObject;
    use objc2_app_kit::NSAccessibility;
    use objc2_foundation::NSString;

    let s = common::start()?;
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (0.0, 0.0), (120.0, 80.0))?;
    let canvas = s.app.with_window(doc, |w| w.canvas_view.retain()).ok_or("no canvas")?;
    let role = canvas.accessibilityRole().map(|r| r.to_string());
    expect(|| format!("canvas role {role:?}"), role.as_deref() == Some("AXLayoutArea"))?;
    let label = canvas.accessibilityLabel().map(|l| l.to_string());
    expect(|| format!("canvas label {label:?}"), label.as_deref() == Some("Ontwerpcanvas"))?;

    let children = canvas.accessibilityChildren().ok_or("no accessibility children")?;
    expect(|| format!("one element, got {}", children.count()), children.count() == 1)?;
    let element: objc2::rc::Retained<AnyObject> = children.objectAtIndex(0);
    let element = element
        .downcast::<objc2_app_kit::NSAccessibilityElement>()
        .map_err(|_| "children are NSAccessibilityElements".to_owned())?;
    let (role, label, value, selected, frame) = (
        element.accessibilityRole(),
        element.accessibilityLabel(),
        element.accessibilityValue(),
        element.isAccessibilitySelected(),
        element.accessibilityFrame(),
    );
    let role = role.map(|r| r.to_string());
    expect(|| format!("element role {role:?}"), role.as_deref() == Some("AXLayoutItem"))?;
    let label = label.map(|l| l.to_string());
    expect(|| format!("element label {label:?}"), label.as_deref() == Some("Rechthoek 1"))?;
    let value = value.and_then(|v| v.downcast::<NSString>().ok()).map(|v| v.to_string());
    expect(|| format!("element value {value:?}"), value.as_deref() == Some("120 × 80 mm"))?;
    expect(|| "the new rectangle is announced as selected".into(), selected)?;
    // 120 × 80 mm at the default 2 pt/mm, in screen points.
    expect(
        || format!("frame {frame:?}"),
        (frame.size.width - 240.0).abs() < 0.5 && (frame.size.height - 160.0).abs() < 0.5,
    )?;
    let window_frame = s.app.window(doc).ok_or("no window")?.frame();
    expect(
        || format!("element frame {frame:?} lies inside the window {window_frame:?}"),
        frame.origin.x >= window_frame.origin.x && frame.origin.y >= window_frame.origin.y,
    )?;

    let field_label = s
        .app
        .with_window(doc, |w| w.field(FieldKey::Width).and_then(|f| f.accessibilityLabel()).map(|l| l.to_string()))
        .flatten();
    expect(|| format!("width field label {field_label:?}"), field_label.as_deref() == Some("Breedte (mm)"))?;
    let table_label = s.app.with_window(doc, |w| w.table.accessibilityLabel().map(|l| l.to_string())).flatten();
    expect(|| format!("table label {table_label:?}"), table_label.as_deref() == Some("Objectenlijst"))?;
    Ok(())
}
