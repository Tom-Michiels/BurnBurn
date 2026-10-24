//! The empty state and the progress row (LF17): what the host draws for
//! `burnburn_host_models::empty_state`. A centred SF Symbol (32 pt, tertiary label colour), one
//! sentence and at most one button over a list that has nothing to show; and, in the status bar, a
//! progress indicator (a bar when the task reports progress, a spinner when it does not) with a
//! cancel button where the task can be cancelled. The host decides nothing: the models say when
//! something is empty or running and which intent the button sends.

use std::cell::RefCell;
use std::rc::Weak;

use burnburn_host_models::empty_state::{EmptyState, ProgressRow};
use burnburn_presentation::UiIntent;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAccessibility, NSAutoresizingMaskOptions, NSButton, NSColor, NSControlSize, NSFont, NSImage, NSImageScaling,
    NSImageSymbolConfiguration, NSImageView, NSLayoutAttribute, NSLayoutConstraint, NSLayoutRelation, NSLineBreakMode,
    NSProgressIndicator, NSProgressIndicatorStyle, NSStackView, NSTextAlignment, NSTextField,
    NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_foundation::{NSArray, NSObject, NSString};

use crate::appkit::app::HostApp;
use crate::appkit::classes::ffi_guard;
use crate::appkit::panels::widgets::{push_button, rect, set_text};
use crate::appkit::toolstrip::symbol_image;

/// The size of the symbol (HIG: a large, quiet glyph).
pub(crate) const SYMBOL_POINTS: f64 = 32.0;
/// Air between the symbol, the sentence and the button.
const SPACING: f64 = 8.0;
/// Air between the overlay's edge and the sentence.
const EDGE: f64 = 16.0;

pub(crate) struct IntentIvars {
    app: Weak<HostApp>,
    intent: RefCell<Option<UiIntent>>,
}

define_class!(
    /// The target of an empty state's button and of a progress row's cancel button: it sends the
    /// intent the model named, and nothing else.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnIntentTarget"]
    #[ivars = IntentIvars]
    pub(crate) struct IntentTarget;

    impl IntentTarget {
        #[unsafe(method(intentAction:))]
        fn intent_action(&self, _sender: Option<&AnyObject>) {
            ffi_guard("intentAction:", (), || {
                let ivars = self.ivars();
                let intent = ivars.intent.try_borrow().ok().and_then(|i| i.clone());
                if let (Some(intent), Some(app)) = (intent, ivars.app.upgrade()) {
                    app.dispatch(intent);
                }
            });
        }
    }
);

impl IntentTarget {
    pub(crate) fn new(mtm: MainThreadMarker, app: Weak<HostApp>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(IntentIvars { app, intent: RefCell::new(None) });
        // SAFETY: NSObject's designated initialiser on a freshly allocated instance with ivars set.
        unsafe { msg_send![super(this), init] }
    }

    pub(crate) fn set_intent(&self, intent: Option<UiIntent>) {
        if let Ok(mut slot) = self.ivars().intent.try_borrow_mut() {
            *slot = intent;
        }
    }

    /// Points `button` at this target.
    #[allow(unsafe_code)]
    pub(crate) fn wire(&self, button: &NSButton) {
        let any: &AnyObject = self.as_ref();
        // SAFETY: the target is weak; its owner clears it before it goes (`unwire`), and the action
        // takes the sender (`v@:@`).
        unsafe { button.setTarget(Some(any)) };
        // SAFETY: as above.
        unsafe { button.setAction(Some(sel!(intentAction:))) };
    }
}

/// An SF Symbol at the empty-state size.
pub(crate) fn big_symbol(name: &str, description: &str) -> Option<Retained<NSImage>> {
    let image = symbol_image(name, description)?;
    let configuration = NSImageSymbolConfiguration::configurationWithPointSize_weight(SYMBOL_POINTS, 0.0);
    image.imageWithSymbolConfiguration(&configuration).or(Some(image))
}

#[allow(unsafe_code)]
fn constrain(
    view: &NSView,
    attribute: NSLayoutAttribute,
    relation: NSLayoutRelation,
    to: Option<&NSView>,
    constant: f64,
) {
    // SAFETY: a constraint between two views of one hierarchy (or one view and a constant); the
    // constraint belongs to the common ancestor and goes with it.
    let constraint = unsafe {
        NSLayoutConstraint::constraintWithItem_attribute_relatedBy_toItem_attribute_multiplier_constant(
            view.as_ref(),
            attribute,
            relation,
            to.map(AsRef::as_ref),
            if to.is_some() { attribute } else { NSLayoutAttribute::NotAnAttribute },
            1.0,
            constant,
        )
    };
    constraint.setActive(true);
}

/// The widgets of an empty state: symbol, sentence and button, stacked and centred.
pub(crate) fn empty_stack(mtm: MainThreadMarker, text_width: f64) -> Retained<NSStackView> {
    let image = NSImageView::initWithFrame(NSImageView::alloc(mtm), rect(0.0, 0.0, SYMBOL_POINTS, SYMBOL_POINTS));
    image.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
    image.setContentTintColor(Some(&NSColor::tertiaryLabelColor()));
    image.setAccessibilityElement(false);
    // A symbol's own size is a little more than its point size: the view keeps to 32 x 32.
    constrain(&image, NSLayoutAttribute::Width, NSLayoutRelation::Equal, None, SYMBOL_POINTS);
    constrain(&image, NSLayoutAttribute::Height, NSLayoutRelation::Equal, None, SYMBOL_POINTS);
    let label = NSTextField::wrappingLabelWithString(&NSString::from_str(""), mtm);
    label.setSelectable(false);
    label.setAlignment(NSTextAlignment::Center);
    label.setFont(Some(&NSFont::systemFontOfSize(13.0)));
    label.setTextColor(Some(&NSColor::secondaryLabelColor()));
    label.setPreferredMaxLayoutWidth(text_width);
    label.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
    let button = push_button(mtm, "");
    button.setHidden(true);
    let views: [&NSView; 3] = [&image, &label, &button];
    let stack = NSStackView::stackViewWithViews(&NSArray::from_slice(&views), mtm);
    stack.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
    stack.setSpacing(SPACING);
    stack.setAlignment(NSLayoutAttribute::CenterX);
    // Air above and below, so a list under it is not touched.
    stack.setEdgeInsets(objc2_foundation::NSEdgeInsets { top: 16.0, left: 0.0, bottom: 16.0, right: 0.0 });
    constrain(&label, NSLayoutAttribute::Width, NSLayoutRelation::LessThanOrEqual, None, text_width);
    stack
}

fn parts(stack: &NSStackView) -> Option<(Retained<NSImageView>, Retained<NSTextField>, Retained<NSButton>)> {
    let views = stack.arrangedSubviews();
    let image = views.iter().next()?.downcast::<NSImageView>().ok()?;
    let label = views.iter().nth(1)?.downcast::<NSTextField>().ok()?;
    let button = views.iter().nth(2)?.downcast::<NSButton>().ok()?;
    Some((image, label, button))
}

/// The sentence of an empty-state stack.
pub(crate) fn sentence(stack: &NSStackView) -> Option<String> {
    parts(stack).map(|(_, label, _)| label.stringValue().to_string())
}

/// The symbol and sentence of an empty-state stack (only what changed is written).
pub(crate) fn set_empty(stack: &NSStackView, symbol: &str, text: &str) {
    let Some((image, label, _)) = parts(stack) else { return };
    let shown = image.accessibilityLabel().map(|l| l.to_string());
    if shown.as_deref() != Some(symbol) {
        image.setImage(big_symbol(symbol, "").as_deref());
        image.setAccessibilityLabel(Some(&NSString::from_str(symbol)));
    }
    set_text(&label, text);
}

/// An empty state laid over a list: hidden while the list has something to show.
pub(crate) struct EmptyOverlay {
    root: Retained<NSView>,
    stack: Retained<NSStackView>,
    target: Retained<IntentTarget>,
    /// Height of the column headers above the rows (0 for a plain list).
    header: f64,
}

impl EmptyOverlay {
    /// Puts the overlay over `parent` (a scroll view or any list container), full size.
    pub(crate) fn new(mtm: MainThreadMarker, app: Weak<HostApp>, parent: &NSView, text_width: f64) -> Self {
        // Over a table in a scroll view the block is centred in the rows' area, below the column
        // headers (which the clip view shares through its top content inset).
        let header = parent
            .downcast_ref::<objc2_app_kit::NSScrollView>()
            .and_then(|scroll| scroll.documentView())
            .and_then(|doc| doc.downcast::<objc2_app_kit::NSTableView>().ok())
            .and_then(|table| table.headerView())
            .map_or(0.0, |h| h.frame().size.height);
        let root = NSView::initWithFrame(NSView::alloc(mtm), parent.bounds());
        root.setAutoresizingMask(NSAutoresizingMaskOptions(
            NSAutoresizingMaskOptions::ViewWidthSizable.0 | NSAutoresizingMaskOptions::ViewHeightSizable.0,
        ));
        let stack = empty_stack(mtm, text_width);
        stack.setTranslatesAutoresizingMaskIntoConstraints(false);
        root.addSubview(&stack);
        constrain(&stack, NSLayoutAttribute::CenterX, NSLayoutRelation::Equal, Some(&root), 0.0);
        constrain(&stack, NSLayoutAttribute::CenterY, NSLayoutRelation::Equal, Some(&root), header / 2.0);
        constrain(&stack, NSLayoutAttribute::Width, NSLayoutRelation::LessThanOrEqual, Some(&root), -2.0 * EDGE);
        let target = IntentTarget::new(mtm, app);
        if let Some((_, _, button)) = parts(&stack) {
            target.wire(&button);
        }
        root.setHidden(true);
        parent.addSubview(&root);
        EmptyOverlay { root, stack, target, header }
    }

    /// Shows `state`, or hides the overlay when the list is not empty.
    pub(crate) fn show(&self, state: Option<&EmptyState>) {
        let Some(state) = state else {
            if !self.root.isHidden() {
                self.root.setHidden(true);
            }
            return;
        };
        set_empty(&self.stack, state.symbol, &state.text);
        if let Some((_, _, button)) = parts(&self.stack) {
            match &state.action {
                Some(action) => {
                    if button.title().to_string() != action.label {
                        button.setTitle(&NSString::from_str(&action.label));
                    }
                    self.target.set_intent(Some(action.intent.clone()));
                    button.setHidden(false);
                }
                None => {
                    self.target.set_intent(None);
                    button.setHidden(true);
                }
            }
        }
        // A short area (the rows under a table header) keeps the sentence and the button and drops
        // the symbol, so the block never reaches over the headers.
        if let Some((image, _, _)) = parts(&self.stack) {
            image.setHidden(false);
            let room = self.root.bounds().size.height - self.header - 2.0 * EDGE;
            if self.stack.fittingSize().height > room {
                image.setHidden(true);
            }
        }
        if self.root.isHidden() {
            self.root.setHidden(false);
        }
    }

    pub(crate) fn is_shown(&self) -> bool {
        !self.root.isHidden()
    }

    /// The text of the sentence (for the native test).
    pub(crate) fn text(&self) -> String {
        parts(&self.stack).map(|(_, l, _)| l.stringValue().to_string()).unwrap_or_default()
    }

    /// The symbol name on the image (for the native test).
    pub(crate) fn symbol(&self) -> String {
        parts(&self.stack).and_then(|(i, _, _)| i.accessibilityLabel()).map(|s| s.to_string()).unwrap_or_default()
    }

    pub(crate) fn has_image(&self) -> bool {
        parts(&self.stack).is_some_and(|(i, _, _)| i.image().is_some())
    }

    /// The action button, visible only when the model offers an action.
    pub(crate) fn button(&self) -> Option<Retained<NSButton>> {
        parts(&self.stack).map(|(_, _, b)| b).filter(|b| !b.isHidden())
    }

    pub(crate) fn clear_target(&self) {
        if let Some((_, _, button)) = parts(&self.stack) {
            // SAFETY: removing the target of a control this overlay owns.
            #[allow(unsafe_code)]
            unsafe {
                button.setTarget(None)
            };
        }
    }
}

/// What an empty-state overlay shows now, for the native tests.
pub struct EmptyProbe {
    pub shown: bool,
    pub symbol: String,
    pub text: String,
    pub has_image: bool,
    pub button_title: Option<String>,
    /// The block (symbol, sentence, button) in window coordinates: x, y, width, height.
    pub block_in_window: [f64; 4],
    button: Option<Retained<NSButton>>,
}

impl EmptyProbe {
    /// Presses the button like a user (`performClick:`); `false` when there is none.
    pub fn click(&self) -> bool {
        match &self.button {
            Some(b) => {
                // SAFETY: a click on a live button on the main thread, as a user would.
                #[allow(unsafe_code)]
                unsafe {
                    b.performClick(None)
                };
                true
            }
            None => false,
        }
    }
}

impl EmptyOverlay {
    pub(crate) fn probe(&self) -> EmptyProbe {
        let button = self.button();
        EmptyProbe {
            shown: self.is_shown(),
            symbol: self.symbol(),
            text: self.text(),
            has_image: self.has_image(),
            button_title: button.as_ref().map(|b| b.title().to_string()),
            block_in_window: {
                let r = self.stack.convertRect_toView(self.stack.bounds(), None);
                [r.origin.x, r.origin.y, r.size.width, r.size.height]
            },
            button,
        }
    }
}

/// What the progress row shows now, for the native tests.
pub struct ProgressProbe {
    pub shown: bool,
    pub title: String,
    pub indeterminate: bool,
    pub value: f64,
    pub cancel_shown: bool,
    cancel: Retained<NSButton>,
}

impl ProgressProbe {
    /// Presses the cancel button like a user.
    pub fn click_cancel(&self) {
        // SAFETY: a click on a live button on the main thread, as a user would.
        #[allow(unsafe_code)]
        unsafe {
            self.cancel.performClick(None)
        };
    }
}

/// The progress row of the status bar: title, bar or spinner, cancel button.
pub(crate) struct ProgressWidgets {
    pub(crate) view: Retained<NSView>,
    pub(crate) title: Retained<NSTextField>,
    pub(crate) indicator: Retained<NSProgressIndicator>,
    pub(crate) cancel: Retained<NSButton>,
    target: Retained<IntentTarget>,
}

const TITLE_WIDTH: f64 = 170.0;
const BAR_WIDTH: f64 = 120.0;
const CANCEL_WIDTH: f64 = 18.0;

impl ProgressWidgets {
    /// The row at the left of the middle of the status bar, hidden until a task runs.
    pub(crate) fn new(mtm: MainThreadMarker, app: Weak<HostApp>, bar: &NSView, x: f64, height: f64) -> Self {
        let width = TITLE_WIDTH + BAR_WIDTH + CANCEL_WIDTH + 12.0;
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect(x, 0.0, width, height));
        view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMaxXMargin);
        let title = NSTextField::labelWithString(&NSString::from_str(""), mtm);
        title.setFrame(rect(0.0, 3.0, TITLE_WIDTH, height - 8.0));
        title.setFont(Some(&NSFont::systemFontOfSize(11.0)));
        title.setTextColor(Some(&NSColor::secondaryLabelColor()));
        title.setAlignment(NSTextAlignment::Right);
        title.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        view.addSubview(&title);
        let indicator = NSProgressIndicator::initWithFrame(
            NSProgressIndicator::alloc(mtm),
            rect(TITLE_WIDTH + 6.0, 5.0, BAR_WIDTH, 12.0),
        );
        indicator.setStyle(NSProgressIndicatorStyle::Bar);
        indicator.setControlSize(NSControlSize::Small);
        indicator.setMinValue(0.0);
        indicator.setMaxValue(1.0);
        indicator.setIndeterminate(true);
        view.addSubview(&indicator);
        let cancel = push_button(mtm, "");
        cancel.setBordered(false);
        cancel.setImage(symbol_image("xmark.circle.fill", "").as_deref());
        cancel.setImagePosition(objc2_app_kit::NSCellImagePosition::ImageOnly);
        cancel.setFrame(rect(TITLE_WIDTH + BAR_WIDTH + 12.0, 2.0, CANCEL_WIDTH, height - 4.0));
        cancel.setHidden(true);
        view.addSubview(&cancel);
        let target = IntentTarget::new(mtm, app);
        target.wire(&cancel);
        view.setHidden(true);
        bar.addSubview(&view);
        ProgressWidgets { view, title, indicator, cancel, target }
    }

    /// Shows the first running task (the one that began first); hides the row when none runs.
    pub(crate) fn show(&self, rows: &[ProgressRow], cancel_label: &str) {
        let Some(row) = rows.first() else {
            if !self.view.isHidden() {
                #[allow(unsafe_code)]
                // SAFETY: stopping the animation of an indicator this row owns.
                unsafe {
                    self.indicator.stopAnimation(None)
                };
                self.view.setHidden(true);
            }
            return;
        };
        let extra = rows.len().saturating_sub(1);
        let title = if extra > 0 { format!("{} (+{extra})", row.title) } else { row.title.clone() };
        set_text(&self.title, &title);
        match row.fraction {
            Some(fraction) => {
                if self.indicator.isIndeterminate() {
                    self.indicator.setIndeterminate(false);
                }
                self.indicator.setDoubleValue(fraction.clamp(0.0, 1.0));
            }
            None => {
                if !self.indicator.isIndeterminate() {
                    self.indicator.setIndeterminate(true);
                }
                #[allow(unsafe_code)]
                // SAFETY: starting the animation of an indicator this row owns.
                unsafe {
                    self.indicator.startAnimation(None)
                };
            }
        }
        match &row.cancel {
            Some(cancel) => {
                self.target.set_intent(Some(cancel.intent.clone()));
                let label = NSString::from_str(if cancel.label.is_empty() { cancel_label } else { &cancel.label });
                self.cancel.setToolTip(Some(&label));
                self.cancel.setAccessibilityLabel(Some(&label));
                self.cancel.setHidden(false);
            }
            None => {
                self.target.set_intent(None);
                self.cancel.setHidden(true);
            }
        }
        if self.view.isHidden() {
            self.view.setHidden(false);
        }
    }

    pub(crate) fn probe(&self) -> ProgressProbe {
        ProgressProbe {
            shown: !self.view.isHidden(),
            title: self.title.stringValue().to_string(),
            indeterminate: self.indicator.isIndeterminate(),
            value: self.indicator.doubleValue(),
            cancel_shown: !self.cancel.isHidden(),
            cancel: self.cancel.clone(),
        }
    }

    pub(crate) fn clear_target(&self) {
        #[allow(unsafe_code)]
        // SAFETY: removing the target of a control this row owns.
        unsafe {
            self.cancel.setTarget(None)
        };
    }
}
