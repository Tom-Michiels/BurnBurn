//! The machine hub window (G5-machine-rest): a floating utility window with four sections (device,
//! production, history, calibration) chosen in a pop-up. It copies `crate::hub_panel::HubPanel` into
//! widgets and reports each use of a control as a tag; the host turns the tag into a
//! `HubEvent`. It decides nothing.

use std::rc::Weak;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAccessibility, NSBackingStoreType, NSButton, NSColor, NSControl, NSFloatingWindowLevel, NSPopUpButton,
    NSScrollView, NSTextField, NSTextView, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSObject, NSString};

use crate::appkit::app::HostApp;
use crate::appkit::classes::ffi_guard;
use crate::appkit::panels::image::{caption, note};
use crate::appkit::panels::widgets::{button, field, rect, set_text};
use crate::hub_panel::{HubPanel, PROOF_KINDS, ProofForm, proof_kind_title};
use crate::l10n::Locale;
use crate::l10n_machine::{MachineText as T, machine_text};

pub(crate) const TAG_SECTION: isize = 1;
pub(crate) const TAG_DEVICE: isize = 2;
pub(crate) const TAG_STOP: isize = 3;
pub(crate) const TAG_REPEAT: isize = 4;
pub(crate) const TAG_CONFIRM: isize = 5;
pub(crate) const TAG_CANCEL: isize = 6;
pub(crate) const TAG_RECORD: isize = 7;
pub(crate) const TAG_REMOVE: isize = 8;

pub(crate) struct HubIvars {
    app: Weak<HostApp>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnHubTarget"]
    #[ivars = HubIvars]
    pub(crate) struct HubTarget;

    impl HubTarget {
        /// Any control of the hub window; its tag says which.
        #[unsafe(method(hubAction:))]
        fn hub_action(&self, sender: Option<&AnyObject>) {
            ffi_guard("hubAction:", (), || {
                let Some(control) = sender.and_then(|s| s.downcast_ref::<NSControl>()) else { return };
                if let Some(app) = self.ivars().app.upgrade() {
                    app.hub_action(control.tag());
                }
            });
        }
    }
);

impl HubTarget {
    fn new(mtm: MainThreadMarker, app: Weak<HostApp>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(HubIvars { app });
        // SAFETY: NSObject's designated initialiser on a freshly allocated instance with ivars set.
        unsafe { msg_send![super(this), init] }
    }
}

pub(crate) struct HubWindow {
    pub(crate) window: Retained<NSWindow>,
    _target: Retained<HubTarget>,
    chooser: Retained<NSPopUpButton>,
    sections: Vec<Retained<NSView>>,
    devices: Retained<NSPopUpButton>,
    support: Retained<NSTextView>,
    upload: Retained<NSTextField>,
    stop: Retained<NSButton>,
    queue_counts: Retained<NSTextField>,
    queue: Retained<NSTextView>,
    history: Retained<NSPopUpButton>,
    repeat_button: Retained<NSButton>,
    repeat_text: Retained<NSTextView>,
    confirm: Retained<NSButton>,
    cancel: Retained<NSButton>,
    kind: Retained<NSPopUpButton>,
    fields: [Retained<NSTextField>; 6],
    problem: Retained<NSTextField>,
    proofs: Retained<NSPopUpButton>,
    remove: Retained<NSButton>,
    pushed: Pushed,
}

/// What the pop-ups hold now, so a refresh does not rebuild a menu the user may have open.
#[derive(Default)]
struct Pushed {
    devices: Vec<String>,
    history: Vec<String>,
    proofs: Vec<String>,
}

const W: f64 = 760.0;
const H: f64 = 560.0;

fn text_area(mtm: MainThreadMarker, parent: &NSView, frame: objc2_foundation::NSRect) -> Retained<NSTextView> {
    let scroll: Retained<NSScrollView> = NSTextView::scrollableTextView(mtm);
    scroll.setFrame(frame);
    scroll.setHasVerticalScroller(true);
    let view = scroll
        .documentView()
        .and_then(|v| v.downcast::<NSTextView>().ok())
        .unwrap_or_else(|| NSTextView::initWithFrame(NSTextView::alloc(mtm), frame));
    view.setEditable(false);
    view.setSelectable(true);
    parent.addSubview(&scroll);
    view
}

fn set_view_text(view: &NSTextView, text: &str) {
    if view.string().to_string() != text {
        view.setString(&NSString::from_str(text));
    }
}

fn set_items(popup: &NSPopUpButton, items: &[String], selected: Option<usize>, last: &mut Vec<String>) {
    if last != items {
        popup.removeAllItems();
        for item in items {
            popup.addItemWithTitle(&NSString::from_str(item));
        }
        *last = items.to_vec();
    }
    if let Some(i) = selected
        && popup.indexOfSelectedItem() != i as isize
    {
        popup.selectItemAtIndex(i as isize);
    }
}

#[allow(unsafe_code)]
pub(crate) fn build_hub(mtm: MainThreadMarker, app: Weak<HostApp>, locale: Locale, float: bool) -> HubWindow {
    let text = |t| machine_text(t, locale);
    let style = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Miniaturizable;
    // SAFETY: a freshly allocated window with a valid rect, style and the buffered backing.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            rect(0.0, 0.0, W, H),
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: ownership stays with the caller's Retained.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(&NSString::from_str(text(T::HubTitle)));
    if float {
        window.setLevel(NSFloatingWindowLevel);
    }
    let content = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, W, H));
    window.setContentView(Some(&content));
    window.center();
    let target = HubTarget::new(mtm, app);
    let t: &AnyObject = &target;
    let wire = |c: &NSControl, tag: isize| c.setTag(tag);

    // Section chooser.
    let chooser =
        NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), rect(16.0, H - 40.0, 220.0, 26.0), false);
    for s in [T::TabDevice, T::TabProduction, T::TabHistory, T::TabCalibration] {
        chooser.addItemWithTitle(&NSString::from_str(text(s)));
    }
    chooser.setAccessibilityLabel(Some(&NSString::from_str(text(T::HubTitle))));
    wire(&chooser, TAG_SECTION);
    // SAFETY: `hubAction:` takes the sender (`v@:@`); the target is kept by `HubWindow`.
    unsafe { chooser.setTarget(Some(t)) };
    // SAFETY: as above.
    unsafe { chooser.setAction(Some(sel!(hubAction:))) };
    content.addSubview(&chooser);

    let section = || {
        let v = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, W, H - 52.0));
        content.addSubview(&v);
        v
    };
    let action = |title: &str, tag: isize, frame| {
        let b = button(mtm, title, t, sel!(hubAction:), frame);
        b.setTag(tag);
        b
    };
    let pop = |tag: isize, frame| {
        let p = NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), frame, false);
        wire(&p, tag);
        // SAFETY: as above.
        unsafe { p.setTarget(Some(t)) };
        // SAFETY: as above.
        unsafe { p.setAction(Some(sel!(hubAction:))) };
        p
    };
    let area_h = H - 52.0;

    // 0: device.
    let s0 = section();
    caption(mtm, &s0, text(T::DeviceLabel), rect(16.0, area_h - 30.0, 140.0, 20.0));
    let devices = pop(TAG_DEVICE, rect(160.0, area_h - 34.0, 420.0, 26.0));
    s0.addSubview(&devices);
    let hint = note(mtm, &s0, rect(16.0, area_h - 84.0, W - 32.0, 44.0), &NSColor::secondaryLabelColor());
    set_text(&hint, text(T::DeviceNote));
    caption(mtm, &s0, text(T::SupportHeader), rect(16.0, area_h - 110.0, W - 32.0, 20.0));
    let support = text_area(mtm, &s0, rect(16.0, 16.0, W - 32.0, area_h - 130.0));

    // 1: production.
    let s1 = section();
    caption(mtm, &s1, text(T::UploadHeader), rect(16.0, area_h - 30.0, 200.0, 20.0));
    let upload = note(mtm, &s1, rect(16.0, area_h - 90.0, W - 200.0, 56.0), &NSColor::controlTextColor());
    let stop = action(text(T::StopUpload), TAG_STOP, rect(W - 170.0, area_h - 60.0, 154.0, 28.0));
    s1.addSubview(&stop);
    caption(mtm, &s1, text(T::QueueHeader), rect(16.0, area_h - 120.0, W - 32.0, 20.0));
    let queue_counts = note(mtm, &s1, rect(16.0, area_h - 170.0, W - 32.0, 44.0), &NSColor::controlTextColor());
    let queue_note = note(mtm, &s1, rect(16.0, area_h - 206.0, W - 32.0, 32.0), &NSColor::secondaryLabelColor());
    set_text(&queue_note, text(T::QueueNote));
    let queue = text_area(mtm, &s1, rect(16.0, 16.0, W - 32.0, area_h - 224.0));

    // 2: history.
    let s2 = section();
    caption(mtm, &s2, text(T::HistoryHeader), rect(16.0, area_h - 30.0, 300.0, 20.0));
    let history = pop(0, rect(16.0, area_h - 62.0, W - 200.0, 26.0));
    s2.addSubview(&history);
    let repeat_button = action(text(T::RepeatButton), TAG_REPEAT, rect(W - 170.0, area_h - 62.0, 154.0, 28.0));
    s2.addSubview(&repeat_button);
    let repeat_text = text_area(mtm, &s2, rect(16.0, 60.0, W - 32.0, area_h - 140.0));
    let confirm = action(text(T::RepeatConfirm), TAG_CONFIRM, rect(16.0, 16.0, 220.0, 28.0));
    let cancel = action(text(T::RepeatCancel), TAG_CANCEL, rect(244.0, 16.0, 120.0, 28.0));
    s2.addSubview(&confirm);
    s2.addSubview(&cancel);
    let repeat_note = note(mtm, &s2, rect(372.0, 8.0, W - 388.0, 44.0), &NSColor::secondaryLabelColor());
    set_text(&repeat_note, text(T::RepeatNote));

    // 3: calibration.
    let s3 = section();
    caption(mtm, &s3, text(T::CalibrationHeader), rect(16.0, area_h - 30.0, 300.0, 20.0));
    let kind = pop(0, rect(160.0, area_h - 64.0, 300.0, 26.0));
    for k in PROOF_KINDS {
        kind.addItemWithTitle(&NSString::from_str(proof_kind_title(k, locale)));
    }
    caption(mtm, &s3, text(T::LabelKind), rect(16.0, area_h - 60.0, 140.0, 20.0));
    s3.addSubview(&kind);
    let mut y = area_h - 96.0;
    let mut fields = Vec::new();
    for (i, label_text) in
        [T::LabelMaterial, T::LabelSpeed, T::LabelValue, T::LabelValueY, T::LabelMethod, T::LabelDate]
            .into_iter()
            .enumerate()
    {
        let (col, row) = (i / 3, i % 3);
        let x = 16.0 + col as f64 * 370.0;
        y = area_h - 96.0 - row as f64 * 30.0;
        caption(mtm, &s3, text(label_text), rect(x, y, 140.0, 20.0));
        let f = field(mtm, rect(x + 144.0, y - 2.0, 200.0, 24.0));
        f.setAccessibilityLabel(Some(&NSString::from_str(text(label_text))));
        s3.addSubview(&f);
        fields.push(f);
    }
    let record = action(text(T::RecordProof), TAG_RECORD, rect(16.0, y - 40.0, 180.0, 28.0));
    s3.addSubview(&record);
    let problem = note(mtm, &s3, rect(204.0, y - 44.0, W - 220.0, 36.0), &NSColor::systemRedColor());
    let proofs = pop(0, rect(16.0, y - 84.0, W - 200.0, 26.0));
    s3.addSubview(&proofs);
    let remove = action(text(T::RemoveProof), TAG_REMOVE, rect(W - 170.0, y - 84.0, 154.0, 28.0));
    s3.addSubview(&remove);
    let calibration_note = note(mtm, &s3, rect(16.0, 16.0, W - 32.0, 60.0), &NSColor::secondaryLabelColor());
    set_text(&calibration_note, text(T::CalibrationNote));

    let fields: [Retained<NSTextField>; 6] = fields.try_into().unwrap_or_else(|_: Vec<Retained<NSTextField>>| {
        // Six fields were pushed above; keep the fixed-size array honest without a panic.
        std::array::from_fn(|_| field(mtm, rect(0.0, 0.0, 0.0, 0.0)))
    });
    let mut w = HubWindow {
        window,
        _target: target,
        chooser,
        sections: vec![s0, s1, s2, s3],
        devices,
        support,
        upload,
        stop,
        queue_counts,
        queue,
        history,
        repeat_button,
        repeat_text,
        confirm,
        cancel,
        kind,
        fields,
        problem,
        proofs,
        remove,
        pushed: Pushed::default(),
    };
    w.show_section(0);
    w
}

impl HubWindow {
    pub(crate) fn show_section(&mut self, index: usize) {
        for (i, s) in self.sections.iter().enumerate() {
            s.setHidden(i != index);
        }
    }

    pub(crate) fn section_popup_changed(&mut self, selected: isize) {
        self.show_section(usize::try_from(selected).unwrap_or(0));
    }

    pub(crate) fn selected_device(&self) -> Option<usize> {
        usize::try_from(self.devices.indexOfSelectedItem()).ok()
    }

    pub(crate) fn section_selected(&self) -> isize {
        self.chooser.indexOfSelectedItem()
    }

    pub(crate) fn selected_history(&self) -> Option<usize> {
        usize::try_from(self.history.indexOfSelectedItem()).ok()
    }

    pub(crate) fn selected_proof(&self) -> Option<usize> {
        usize::try_from(self.proofs.indexOfSelectedItem()).ok()
    }

    /// The form as typed.
    pub(crate) fn form(&self) -> ProofForm {
        let s = |i: usize| self.fields[i].stringValue().to_string();
        ProofForm {
            kind: usize::try_from(self.kind.indexOfSelectedItem()).unwrap_or(usize::MAX),
            material: s(0),
            speed: s(1),
            value: s(2),
            value_y: s(3),
            method: s(4),
            date: s(5),
        }
    }

    pub(crate) fn apply(&mut self, p: &HubPanel, locale: Locale) {
        let text = |t| machine_text(t, locale);
        let none = |v: &[String], t| if v.is_empty() { vec![text(t).to_owned()] } else { v.to_vec() };
        let (devices, history, proofs) =
            (none(&p.devices, T::NoDevices), none(&p.history, T::HistoryNone), none(&p.proofs, T::ProofNone));
        set_items(&self.devices, &devices, p.active_device, &mut self.pushed.devices);
        self.devices.setEnabled(!p.devices.is_empty());
        set_view_text(&self.support, &p.support.join("\n"));
        set_text(&self.upload, &p.upload);
        self.stop.setEnabled(p.stop_enabled);
        set_text(&self.queue_counts, &p.queue_counts);
        set_view_text(&self.queue, &p.queue.join("\n"));
        set_items(&self.history, &history, None, &mut self.pushed.history);
        self.history.setEnabled(!p.history.is_empty());
        self.repeat_button.setEnabled(!p.history.is_empty());
        let (shown, can_confirm) = match &p.repeat {
            Some(r) => {
                let mut lines = vec![r.header.clone()];
                lines.extend(r.changes.iter().map(|c| format!("  {c}")));
                lines.extend(r.refusal.clone());
                (lines.join("\n"), r.can_confirm)
            }
            None => (String::new(), false),
        };
        set_view_text(&self.repeat_text, &shown);
        self.confirm.setEnabled(can_confirm);
        self.cancel.setEnabled(p.repeat.is_some());
        set_items(&self.proofs, &proofs, None, &mut self.pushed.proofs);
        self.proofs.setEnabled(!p.proofs.is_empty());
        self.remove.setEnabled(!p.proofs.is_empty());
        set_text(&self.problem, &p.proof_problem);
    }
}
