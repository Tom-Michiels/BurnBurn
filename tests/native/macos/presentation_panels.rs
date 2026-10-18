//! The integration-round panels in the real window: the inspector's rotation field (degrees, one
//! turn per commit), the operations panel (add with raw text, edit a table cell), final canvas
//! text becoming one text object, and the bed-camera section with a fake camera port: showing it
//! asks and starts nothing; only the access button asks.
//!
//! Table-cell editing is driven through the table's data source (`tableView:setObjectValue:…`), as
//! AppKit calls it when a cell edit ends; typing into the cell editor itself is not driven here.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("presentation_panels: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("rotation_operations_text_and_camera_panels_drive_presentation", scenario())
}

#[cfg(target_os = "macos")]
mod fake {
    use std::sync::{Arc, Mutex};

    use burnburn_presentation::{
        CameraAuthorization, CameraDevice, CameraDeviceId, CameraError, CameraFrame, CameraMode, CameraPixelFormat,
        CameraPort,
    };

    #[derive(Default)]
    pub struct Camera {
        pub log: Mutex<Vec<String>>,
    }

    impl CameraPort for Camera {
        fn devices(&self) -> Result<Vec<CameraDevice>, CameraError> {
            Ok(vec![CameraDevice {
                id: CameraDeviceId("fake".into()),
                name: "Testcamera".into(),
                modes: vec![CameraMode {
                    width: 640,
                    height: 480,
                    fps_milli: 30_000,
                    pixel_format: CameraPixelFormat::Bgra8,
                }],
            }])
        }
        fn authorization(&self) -> CameraAuthorization {
            CameraAuthorization::NotDetermined
        }
        fn request_authorization(&self) {
            self.log.lock().map(|mut l| l.push("request".into())).ok();
        }
        fn start(&self, _device: &CameraDeviceId, _mode: &CameraMode) -> Result<(), CameraError> {
            self.log.lock().map(|mut l| l.push("start".into())).ok();
            Ok(())
        }
        fn stop(&self, _device: &CameraDeviceId) {}
        fn latest_frame(&self, _device: &CameraDeviceId) -> Option<Arc<CameraFrame>> {
            None
        }
    }
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use std::sync::Arc;

    use burnburn_presentation::{FieldKey, UiIntent};
    use common::expect;
    use objc2::{Message, msg_send};
    use objc2_app_kit::NSAccessibility;
    use objc2_foundation::{NSRange, NSString};

    let s = common::start()?;
    let camera = Arc::new(fake::Camera::default());
    s.app.attach_camera(camera.clone());
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (10.0, 10.0), (130.0, 90.0))?;

    // Rotation: a fifth inspector field in degrees; "90" + Return turns 120 × 80 into 80 × 120.
    let caption = s
        .app
        .with_window(doc, |w| w.field(FieldKey::Rotation).and_then(|f| f.accessibilityLabel()).map(|l| l.to_string()))
        .flatten();
    expect(|| format!("rotation field caption {caption:?}"), caption.as_deref() == Some("Rotatie (°)"))?;
    s.app.take_dispatched();
    s.type_into_field(doc, FieldKey::Rotation, "90")?;
    s.key(doc, "\r", 36)?;
    s.pump(0.2);
    let size = |key: FieldKey| s.app.with_window(doc, |w| w.field(key).map(|f| f.stringValue().to_string())).flatten();
    expect(
        || format!("turned size {:?} × {:?}", size(FieldKey::Width), size(FieldKey::Height)),
        size(FieldKey::Width).as_deref() == Some("80") && size(FieldKey::Height).as_deref() == Some("120"),
    )?;
    expect(
        || format!("rotation shows 0 again: {:?}", size(FieldKey::Rotation)),
        size(FieldKey::Rotation).as_deref() == Some("0"),
    )?;
    let commits = s.commit_field_intents();
    expect(|| format!("one rotation commit, got {commits:?}"), commits.len() == 1)?;

    // Operations: name, kind, speed and power as typed, then the Add button.
    s.app.with_window(doc, |w| {
        w.operation_inputs[0].setStringValue(&NSString::from_str("Snijden"));
        w.operation_inputs[1].setStringValue(&NSString::from_str("100 mm/s"));
        w.operation_inputs[2].setStringValue(&NSString::from_str("35,5 %"));
        w.operation_kind.selectItemAtIndex(0);
    });
    let add = s.app.with_window(doc, |w| w.operation_add.retain()).ok_or("no add button")?;
    // SAFETY: clicking our own button with no sender, as the user does.
    unsafe { add.performClick(None) };
    s.pump(0.2);
    let sent = s.app.take_dispatched();
    expect(
        || format!("add sends raw text, got {sent:?}"),
        sent.iter().any(|i| matches!(i, UiIntent::AddOperationText { speed, power, .. } if speed == "100 mm/s" && power == "35,5 %")),
    )?;
    let rows = s.app.with_window(doc, |w| w.operations_table.numberOfRows()).unwrap_or(0);
    expect(|| format!("one operation row, got {rows}"), rows == 1)?;
    let cell = |column: &str| -> Option<String> {
        s.app
            .with_window(doc, |w| {
                let table = w.operations_table;
                let col = table.tableColumnWithIdentifier(&NSString::from_str(column))?;
                // SAFETY: reading the table's (weak) data source; the window keeps it alive.
                let source = unsafe { table.dataSource() }?;
                // SAFETY: the table's own data source, asked as NSTableView asks it.
                let value: Option<objc2::rc::Retained<objc2::runtime::AnyObject>> =
                    unsafe { msg_send![&*source, tableView: table, objectValueForTableColumn: &*col, row: 0isize] };
                value.and_then(|v| v.downcast::<NSString>().ok()).map(|v| v.to_string())
            })
            .flatten()
    };
    expect(
        || format!("row {:?} {:?} {:?} {:?}", cell("name"), cell("kind"), cell("speed"), cell("power")),
        cell("name").as_deref() == Some("Snijden")
            && cell("kind").as_deref() == Some("Lijn")
            && cell("speed").as_deref() == Some("100")
            && cell("power").as_deref() == Some("35.5"),
    )?;
    // Ending a cell edit: the data source receives the typed text (outside `with_window`, as the
    // edit re-enters the host).
    let table = s.app.with_window(doc, |w| w.operations_table.retain()).ok_or("no operations table")?;
    let column = table.tableColumnWithIdentifier(&NSString::from_str("speed")).ok_or("no speed column")?;
    // SAFETY: reading the table's (weak) data source; the window keeps it alive.
    let source = unsafe { table.dataSource() }.ok_or("no data source")?;
    let text = NSString::from_str("250");
    // SAFETY: as AppKit calls it when the cell editor ends.
    let _: () = unsafe {
        msg_send![&*source, tableView: &*table, setObjectValue: &*text, forTableColumn: &*column, row: 0isize]
    };
    s.pump(0.2);
    expect(|| format!("edited speed {:?}", cell("speed")), cell("speed").as_deref() == Some("250"))?;

    // Canvas text: the final text of the input route becomes one text object.
    let canvas = s.app.with_window(doc, |w| w.canvas_view.retain()).ok_or("no canvas")?;
    s.app.window(doc).ok_or("no window")?.makeFirstResponder(Some(&canvas));
    let revision = s.doc(doc).map(|d| d.revision);
    let none = NSRange::new(usize::MAX as _, 0);
    // SAFETY: NSTextInputClient message to our own canvas view, as the input method sends it.
    unsafe {
        let _: () = msg_send![&canvas, insertText: &*NSString::from_str("Hallo"), replacementRange: none];
    }
    s.pump(0.2);
    let texts = s.app.text_contents(doc);
    expect(|| format!("one text object 'Hallo', got {texts:?}"), texts == ["Hallo"])?;
    expect(|| "one revision for one commit".into(), s.doc(doc).map(|d| d.revision) != revision)?;

    // Camera: shown with its state; nothing was asked or started by opening the window.
    let labels = s
        .app
        .with_window(doc, |w| w.camera_labels.iter().map(|l| l.stringValue().to_string()).collect::<Vec<_>>())
        .unwrap_or_default();
    expect(
        || format!("camera labels {labels:?}"),
        labels.first().map(String::as_str) == Some("Nog geen toegang gevraagd"),
    )?;
    let enabled = s.app.with_window(doc, |w| w.camera_buttons.map(|b| b.isEnabled())).ok_or("no camera buttons")?;
    expect(|| format!("access/start/stop enabled {enabled:?}"), enabled == [true, false, false])?;
    expect(|| "nothing asked or started on open".into(), camera.log.lock().map(|l| l.is_empty()).unwrap_or(false))?;
    let access = s.app.with_window(doc, |w| w.camera_buttons[0].retain()).ok_or("no access button")?;
    // SAFETY: as above.
    unsafe { access.performClick(None) };
    s.pump(0.2);
    let log = camera.log.lock().map(|l| l.clone()).unwrap_or_default();
    expect(|| format!("the click asks once, got {log:?}"), log == ["request"])?;

    s.wait_on_screen(doc)?;
    s.app.with_window(doc, |w| common::screenshot(w.window));
    Ok(())
}
