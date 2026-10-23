//! Screens on the existing view models, natively: the tool strip, the X/Y/B/H
//! inspector and the plan preview panel (plan id, estimate, current/out of date) with its
//! Compile and Export buttons. Export opens the RD save sheet; cancelling it writes nothing.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("panels_and_preview: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("toolbar_inspector_and_plan_preview_follow_the_view_model", scenario())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use burnburn_document::process::{
        Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, Speed,
    };
    use burnburn_editor::ToolKind;
    use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
    use burnburn_macos::panels::INSPECTOR_FIELDS;
    use burnburn_presentation::{FieldKey, UiIntent};
    use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
    use common::expect;
    use objc2::Message;

    let s = common::start()?;
    let doc = s.new_document()?;

    // Tool strip: clicking the ellipse button chooses the ellipse tool; a menu command moves the
    // pressed state along (details in tool_strip.rs).
    let ellipse = s.app.with_window(doc, |w| w.strip.button("sketch.ellipse")).flatten().ok_or("no ellipse button")?;
    // SAFETY: a click on our own button, as the user's click would send it.
    unsafe { ellipse.performClick(None) };
    s.pump(0.1);
    expect(|| "ellipse tool active".into(), s.doc(doc).is_some_and(|d| d.tool == ToolKind::EllipseAxes))?;
    s.app.dispatch(UiIntent::Invoke { document: Some(doc), command: burnburn_presentation::CommandKey::ToolRectangle });
    s.pump(0.1);
    let rect_pressed = s
        .app
        .with_window(doc, |w| {
            w.strip.button("sketch.rectangle.two_point").map(|b| b.state() == objc2_app_kit::NSControlStateValueOn)
        })
        .flatten();
    expect(|| "strip follows the tool".into(), rect_pressed == Some(true))?;

    s.drag_mm(doc, (10.0, 20.0), (130.0, 100.0))?;
    for key in INSPECTOR_FIELDS {
        let shown = s.app.with_window(doc, |w| w.field(key).map(|f| f.stringValue().to_string())).flatten();
        let model = s.doc(doc).and_then(|d| d.inspector.and_then(|i| i.field(key).map(|f| f.text.clone())));
        expect(|| format!("{key:?}: field {shown:?} vs model {model:?}"), shown.is_some() && shown == model)?;
    }

    // Preview panel before compiling.
    let title = s.app.with_window(doc, |w| w.plan_labels[0].stringValue().to_string()).unwrap_or_default();
    expect(|| format!("no plan yet: {title:?}"), title == "Geen plan")?;
    s.app.dispatch(UiIntent::SetMachineProfile {
        profile: Box::new(
            MachineProfile::new(
                "research-1500x900",
                1,
                MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).map_err(|e| format!("{e:?}"))?,
                1000.0,
                400.0,
                PowerRatio::new(0.0).map_err(|e| format!("{e:?}"))?,
                PowerRatio::new(1.0).map_err(|e| format!("{e:?}"))?,
            )
            .map_err(|e| format!("{e:?}"))?,
        ),
        ruida: RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged },
    });
    let p = PowerPercent::new(20.0).map_err(|e| format!("{e:?}"))?;
    s.app.dispatch(UiIntent::AddOperation {
        document: doc,
        draft: OperationDraft {
            name: "Snijden".into(),
            mode: OperationMode::Line { kerf: Kerf::NONE },
            settings: ProcessSettings {
                speed: Speed::mm_per_s(100.0).map_err(|e| format!("{e:?}"))?,
                power: PowerRange::new(p, p).map_err(|e| format!("{e:?}"))?,
                passes: Passes::new(1).map_err(|e| format!("{e:?}"))?,
                air_assist: true,
            },
            output_enabled: true,
        },
    });
    let compile = s.app.with_window(doc, |w| w.compile_button.retain()).ok_or("no compile button")?;
    s.pump(0.1);
    expect(|| "compile enabled".into(), compile.isEnabled())?;
    // SAFETY: clicking our own button with no sender, as the user does.
    unsafe { compile.performClick(None) };
    s.wait(5.0, "the compiled plan", || s.doc(doc).is_some_and(|d| d.plan.is_some()))?;
    s.pump(0.2);
    let plan = s.doc(doc).and_then(|d| d.plan).ok_or("no plan")?;
    let labels: Vec<String> = s
        .app
        .with_window(doc, |w| w.plan_labels.iter().map(|l| l.stringValue().to_string()).collect())
        .unwrap_or_default();
    expect(
        || format!("labels {labels:?}"),
        labels.first().is_some_and(|t| t.starts_with("Plan van ") && t.contains(" \u{b7} ")),
    )?;
    // The hash is not the visible text (LF3 Q4); the title carries it as a tooltip.
    expect(|| format!("plan id row {labels:?}"), labels.get(1).is_some_and(String::is_empty))?;
    let tooltip = s.app.with_window(doc, |w| w.plan_labels[0].toolTip().map(|t| t.to_string())).flatten();
    expect(|| format!("tooltip {tooltip:?} vs {}", plan.id), tooltip == Some(format!("Plan-id: {}", plan.id)))?;
    expect(|| format!("estimate {labels:?}"), labels.get(2).is_some_and(|e| e.ends_with("(schatting)")))?;
    expect(|| format!("state {labels:?}"), labels.get(3).map(String::as_str) == Some("Actueel"))?;
    // AppKit widgets are captured even while the screen is locked; the Metal canvas only when
    // the window is really on screen.
    s.wait_on_screen(doc)?;
    s.pump(0.3);
    s.app.with_window(doc, |w| common::screenshot(w.window));

    // An edit makes the shown plan out of date, in words.
    s.type_into_field(doc, FieldKey::Width, "150")?;
    s.key(doc, "\r", 36)?;
    s.pump(0.2);
    let state = s.app.with_window(doc, |w| w.plan_labels[3].stringValue().to_string()).unwrap_or_default();
    expect(|| format!("stale state {state:?}"), state.starts_with("Verouderd"))?;

    // Export opens the RD save sheet; cancelling writes nothing.
    let export = s.app.with_window(doc, |w| w.export_button.retain()).ok_or("no export button")?;
    s.app.dispatch(UiIntent::Invoke { document: Some(doc), command: burnburn_presentation::CommandKey::Compile });
    s.wait(5.0, "a fresh plan", || s.doc(doc).is_some_and(|d| d.plan.as_ref().is_some_and(|p| !p.stale)))?;
    s.pump(0.2);
    // SAFETY: as above.
    unsafe { export.performClick(None) };
    s.wait(2.0, "the export sheet", || {
        s.app.with_window(doc, |w| w.export_panel.is_some() && w.window.attachedSheet().is_some()).unwrap_or(false)
    })?;
    let panel = s.app.with_window(doc, |w| w.export_panel.map(|p| p.retain())).flatten().ok_or("no panel")?;
    // SAFETY: the panel's own cancel action, as the Cancel button sends it.
    unsafe { panel.cancel(None) };
    s.wait(2.0, "the sheet to close", || {
        s.app.with_window(doc, |w| w.window.attachedSheet().is_none()).unwrap_or(false)
    })?;
    let files = std::fs::read_dir(&s.dir).map(|d| d.count()).unwrap_or(0);
    expect(|| format!("nothing exported, found {files} files"), files == 0)?;
    Ok(())
}
