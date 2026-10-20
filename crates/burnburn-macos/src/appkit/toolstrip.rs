//! The tool strip above the canvas (SolidCraft's ribbon in AppKit form): panels with a title,
//! icon buttons per tool, a small arrow with a drop-down for variant groups, tooltips with the
//! shortcut and the reason a tool is disabled.
//!
//! The strip only shows: every state (selected, enabled, the variant on a group's face) comes
//! from `ToolbarViewModel`, every click becomes a `NativeEvent` with a catalog id. Icons are the
//! RGBA images of `burnburn-icons` (2x on Retina), drawn again when the effective appearance or
//! the backing scale changes.
//!
//! `unsafe` is limited to the bitmap copy and the control wiring (SAFETY comments).

use std::cell::{Cell, RefCell};
use std::ptr;
use std::rc::Weak;

use burnburn_icons::{RgbaImage, Theme};
use burnburn_presentation::{GroupViewModel, PanelId, ToolbarViewModel};
use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly, Message, sel};
use objc2_app_kit::{
    NSAccessibility, NSAppearanceCustomization, NSAutoresizingMaskOptions, NSBezelStyle, NSBitmapImageRep, NSBox,
    NSBoxType, NSButton, NSButtonType, NSCellImagePosition, NSColor, NSControlStateValueOff, NSControlStateValueOn,
    NSDeviceRGBColorSpace, NSFont, NSImage, NSMenu, NSMenuItem, NSScrollView, NSTextAlignment, NSTextField, NSView,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

use super::app::HostApp;
use super::classes::ToolStripView;
use crate::l10n::Locale;
use crate::toolbar::{panel_title, tag_for_id, tool_label, tooltip};

/// Height of the strip: icon row plus the panel titles.
pub(crate) const STRIP_HEIGHT: f64 = 62.0;
const BUTTON_W: f64 = 30.0;
const BUTTON_H: f64 = 28.0;
const TEXT_BUTTON_W: f64 = 66.0;
const CARET_W: f64 = 11.0;
const GAP: f64 = 2.0;
const PANEL_PAD: f64 = 8.0;
const ICON_PT: u32 = 20;
const MENU_ICON_PT: u32 = 16;
const ROW_Y: f64 = 24.0;

fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}

/// One button of the strip: a single tool, or a group's face with its arrow.
struct Entry {
    group: Option<&'static str>,
    main: Retained<NSButton>,
    caret: Option<Retained<NSButton>>,
    /// The tool a single button is for (a group's first member); constant.
    home: &'static str,
    /// The id the button shows now.
    face: Cell<&'static str>,
}

pub struct ToolStrip {
    scroll: Retained<NSScrollView>,
    content: Retained<ToolStripView>,
    entries: Vec<Entry>,
    /// Panel titles, in panel order.
    titles: Vec<(PanelId, Retained<NSTextField>)>,
    target: Retained<AnyObject>,
    locale: Locale,
    /// Appearance and scale the images were drawn for.
    drawn: Cell<Option<(Theme, u32)>>,
    model: RefCell<ToolbarViewModel>,
    /// The user's shortcut for a tool id (`None`: removed), when it is not the catalogue's.
    shortcuts: RefCell<std::collections::BTreeMap<String, Option<String>>>,
}

impl ToolStrip {
    #[allow(unsafe_code)]
    pub(crate) fn new(
        mtm: MainThreadMarker,
        app: Weak<HostApp>,
        document: DocumentId,
        target: &AnyObject,
        locale: Locale,
        width: f64,
    ) -> ToolStrip {
        let layout = ToolbarViewModel::build(
            burnburn_presentation::ToolCatalog::standard(),
            burnburn_presentation::ToolState { document: false, operator: false, in_sketch: false },
            burnburn_presentation::ActiveTool { tool: None, trim: false, polygon: None },
            &burnburn_presentation::VariantMemory::new(),
        );
        let content = ToolStripView::new(mtm, rect(0.0, 0.0, 10.0, STRIP_HEIGHT), app, document);
        let mut entries = Vec::new();
        let mut titles = Vec::new();
        let mut x = PANEL_PAD;
        for panel in &layout.panels {
            if panel.groups.is_empty() {
                continue;
            }
            let panel_x = x;
            for group in &panel.groups {
                let text_only = !burnburn_icons::has_icon(group.active.icon);
                let main_w = if text_only { TEXT_BUTTON_W } else { BUTTON_W };
                let main = new_button(mtm, target, sel!(toolButton:), rect(x, ROW_Y, main_w, BUTTON_H));
                let tag = tag_for_id(group.active.id).unwrap_or(-1);
                main.setTag(tag);
                content.addSubview(&main);
                x += main_w;
                let caret = group.group.map(|_| {
                    let c = new_button(mtm, target, sel!(variantMenu:), rect(x, ROW_Y, CARET_W, BUTTON_H));
                    c.setTitle(&NSString::from_str("\u{25BE}"));
                    c.setFont(Some(&NSFont::systemFontOfSize(9.0)));
                    c.setTag(tag);
                    c.setBezelStyle(NSBezelStyle::Toolbar);
                    content.addSubview(&c);
                    x += CARET_W;
                    c
                });
                x += GAP;
                entries.push(Entry {
                    group: group.group,
                    home: group.active.id,
                    main,
                    caret,
                    face: Cell::new(group.active.id),
                });
            }
            let panel_w = (x - panel_x - GAP).max(64.0);
            x = x.max(panel_x + panel_w + GAP);
            let title = NSTextField::labelWithString(&NSString::from_str(&panel_title(panel.panel, locale)), mtm);
            title.setFrame(rect(panel_x, 4.0, panel_w, 14.0));
            title.setAlignment(NSTextAlignment::Center);
            title.setFont(Some(&NSFont::systemFontOfSize(9.0)));
            title.setTextColor(Some(&NSColor::secondaryLabelColor()));
            content.addSubview(&title);
            titles.push((panel.panel, title));
            x += PANEL_PAD;
            let rule =
                NSBox::initWithFrame(NSBox::alloc(mtm), rect(x - PANEL_PAD / 2.0, 8.0, 1.0, STRIP_HEIGHT - 16.0));
            rule.setBoxType(NSBoxType::Separator);
            content.addSubview(&rule);
            x += PANEL_PAD / 2.0;
        }
        content.setFrameSize(NSSize::new(x, STRIP_HEIGHT));

        let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(0.0, 0.0, width, STRIP_HEIGHT));
        scroll.setHasHorizontalScroller(true);
        scroll.setHasVerticalScroller(false);
        scroll.setAutohidesScrollers(true);
        scroll.setDrawsBackground(false);
        scroll.setAutoresizingMask(NSAutoresizingMaskOptions(
            NSAutoresizingMaskOptions::ViewMinYMargin.0 | NSAutoresizingMaskOptions::ViewWidthSizable.0,
        ));
        scroll.setDocumentView(Some(&content));
        ToolStrip {
            scroll,
            content,
            entries,
            titles,
            target: target.retain(),
            locale,
            drawn: Cell::new(None),
            model: RefCell::new(layout),
            shortcuts: RefCell::new(std::collections::BTreeMap::new()),
        }
    }

    /// The view to put into the window.
    pub(crate) fn view(&self) -> &NSView {
        &self.scroll
    }

    /// The shortcuts the user chose for tools (id → text, `None` = no shortcut); the next
    /// `refresh` writes them into the tooltips.
    pub(crate) fn set_shortcuts(&self, shortcuts: std::collections::BTreeMap<String, Option<String>>) {
        if let Ok(mut current) = self.shortcuts.try_borrow_mut() {
            *current = shortcuts;
        }
    }

    fn shortcut_text(&self, id: &str, default: Option<&'static str>) -> Option<String> {
        match self.shortcuts.try_borrow().ok().and_then(|m| m.get(id).cloned()) {
            Some(chosen) => chosen,
            None => default.map(str::to_owned),
        }
    }

    /// Copy the view model into the buttons; draw the icons again when the appearance or the
    /// backing scale changed since they were drawn.
    pub(crate) fn refresh(&self, vm: &ToolbarViewModel) {
        if let Ok(mut model) = self.model.try_borrow_mut() {
            *model = vm.clone();
        }
        let (theme, scale) = (self.current_theme(), self.current_scale());
        let restyle = self.drawn.get() != Some((theme, scale));
        for e in &self.entries {
            let found = vm.panels.iter().flat_map(|p| p.groups.iter()).find(|g| match e.group {
                Some(name) => g.group == Some(name),
                None => g.group.is_none() && g.active.id == e.home,
            });
            let Some(g) = found else { continue };
            let face = &g.active;
            let changed = e.face.get() != face.id;
            e.face.set(face.id);
            let tag = tag_for_id(face.id).unwrap_or(-1);
            e.main.setTag(tag);
            e.main.setEnabled(face.enablement.is_enabled());
            e.main.setState(if g.selected() { NSControlStateValueOn } else { NSControlStateValueOff });
            let shortcut = self.shortcut_text(face.id, face.shortcut);
            let tip = NSString::from_str(&tooltip(face.id, shortcut.as_deref(), face.enablement, self.locale));
            e.main.setToolTip(Some(&tip));
            let label = NSString::from_str(&tool_label(face.id, self.locale));
            e.main.setAccessibilityLabel(Some(&label));
            if let Some(caret) = &e.caret {
                caret.setTag(tag);
                caret.setEnabled(g.variants.iter().any(|v| v.enablement.is_enabled()));
                caret.setToolTip(Some(&tip));
                caret.setAccessibilityLabel(Some(&label));
            }
            if changed || restyle {
                match icon_image(face.icon, ICON_PT, scale, theme) {
                    Some(image) => {
                        e.main.setImage(Some(&image));
                        e.main.setTitle(&NSString::from_str(""));
                        e.main.setImagePosition(NSCellImagePosition::ImageOnly);
                    }
                    None => {
                        // No artwork for this tool yet: its name, never a blank button.
                        e.main.setImage(None);
                        e.main.setFont(Some(&NSFont::systemFontOfSize(10.0)));
                        e.main.setTitle(&label);
                        e.main.setImagePosition(NSCellImagePosition::NoImage);
                    }
                }
            }
        }
        self.drawn.set(Some((theme, scale)));
    }

    /// Clear the weak targets before the window controller goes.
    #[allow(unsafe_code)]
    pub(crate) fn clear_targets(&self) {
        for e in &self.entries {
            // SAFETY: a nil target cannot dangle.
            unsafe { e.main.setTarget(None) };
            if let Some(c) = &e.caret {
                // SAFETY: as above.
                unsafe { c.setTarget(None) };
            }
        }
    }

    /// The button currently showing `id` (a single tool, or a group's face).
    pub fn button(&self, id: &str) -> Option<Retained<NSButton>> {
        self.entries.iter().find(|e| e.face.get() == id).map(|e| e.main.clone())
    }

    /// The arrow button of a variant group.
    pub fn arrow(&self, group: &str) -> Option<Retained<NSButton>> {
        self.entries.iter().find(|e| e.group == Some(group)).and_then(|e| e.caret.clone())
    }

    /// The drop-down of a group, built from the last view model (what the arrow opens).
    pub fn menu(&self, group: &str) -> Option<Retained<NSMenu>> {
        let mtm = MainThreadMarker::new()?;
        let model = self.model.try_borrow().ok()?;
        let g = model.group(group)?;
        Some(self.build_menu(mtm, g))
    }

    /// Titles of the panels, as shown.
    pub fn panel_titles(&self) -> Vec<String> {
        self.titles.iter().map(|(_, t)| t.stringValue().to_string()).collect()
    }

    /// Theme the icons are drawn in now.
    pub fn drawn_theme(&self) -> Option<Theme> {
        self.drawn.get().map(|(t, _)| t)
    }

    #[allow(unsafe_code)]
    fn build_menu(&self, mtm: MainThreadMarker, group: &GroupViewModel) -> Retained<NSMenu> {
        let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(""));
        menu.setAutoenablesItems(false);
        let (theme, scale) = (self.current_theme(), self.current_scale());
        for v in &group.variants {
            let label = tool_label(v.id, self.locale);
            // SAFETY: a freshly allocated item; `variantChosen:` takes the sender (`v@:@`) on the
            // window controller, which outlives its menus (the target is weak).
            let item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &NSString::from_str(&label),
                    Some(sel!(variantChosen:)),
                    &NSString::from_str(""),
                )
            };
            // SAFETY: as above; the controller is kept alive by the window for as long as it is shown.
            unsafe { item.setTarget(Some(&self.target)) };
            item.setTag(tag_for_id(v.id).unwrap_or(-1));
            item.setEnabled(v.enablement.is_enabled());
            item.setState(if v.id == group.active.id { NSControlStateValueOn } else { NSControlStateValueOff });
            let shortcut = self.shortcut_text(v.id, v.shortcut);
            item.setToolTip(Some(&NSString::from_str(&tooltip(v.id, shortcut.as_deref(), v.enablement, self.locale))));
            if let Some(image) = icon_image(v.icon, MENU_ICON_PT, scale, theme) {
                item.setImage(Some(&image));
            }
            menu.addItem(&item);
        }
        menu
    }

    fn current_theme(&self) -> Theme {
        theme_of(&self.content)
    }

    fn current_scale(&self) -> u32 {
        scale_of(&self.content)
    }
}

/// Light or dark, from the view's effective appearance.
pub(crate) fn theme_of(view: &NSView) -> Theme {
    if view.effectiveAppearance().name().to_string().contains("Dark") { Theme::Dark } else { Theme::Light }
}

/// Backing scale as a whole number (1, 2 or 3).
pub(crate) fn scale_of(view: &NSView) -> u32 {
    view.window().map_or(2.0, |w| w.backingScaleFactor()).round().clamp(1.0, 3.0) as u32
}

#[allow(unsafe_code)]
fn new_button(
    mtm: MainThreadMarker,
    target: &AnyObject,
    action: objc2::runtime::Sel,
    frame: NSRect,
) -> Retained<NSButton> {
    // SAFETY: `action` is a window-controller method taking the sender (`v@:@`); the target is
    // weak and the window keeps the controller alive while the button exists.
    let b =
        unsafe { NSButton::buttonWithTitle_target_action(&NSString::from_str(""), Some(target), Some(action), mtm) };
    b.setFrame(frame);
    b.setButtonType(NSButtonType::PushOnPushOff);
    b.setBezelStyle(NSBezelStyle::Toolbar);
    b.setImagePosition(NSCellImagePosition::ImageOnly);
    b
}

/// The icon as an image `points` wide, drawn from `scale * points` pixels. `None` without artwork.
pub(crate) fn icon_image(id: &str, points: u32, scale: u32, theme: Theme) -> Option<Retained<NSImage>> {
    let rgba = burnburn_icons::render(id, points, scale as f32, theme).ok()?;
    ns_image(&rgba, f64::from(points))
}

/// Straight-alpha RGBA to the premultiplied bytes of a bitmap representation.
fn premultiply(dst: &mut [u8], src: &[u8]) {
    let (dst, _) = dst.as_chunks_mut::<4>();
    let (src, _) = src.as_chunks::<4>();
    for (d, s) in dst.iter_mut().zip(src) {
        let a = u32::from(s[3]);
        for i in 0..3 {
            d[i] = ((u32::from(s[i]) * a + 127) / 255) as u8;
        }
        d[3] = s[3];
    }
}

#[allow(unsafe_code)]
fn ns_image(rgba: &RgbaImage, points: f64) -> Option<Retained<NSImage>> {
    let (w, h) = (isize::try_from(rgba.width).ok()?, isize::try_from(rgba.height).ok()?);
    let len = usize::try_from(w).ok()?.checked_mul(usize::try_from(h).ok()?)?.checked_mul(4)?;
    if w == 0 || h == 0 || rgba.pixels.len() != len {
        return None;
    }
    // SAFETY: null planes make the representation allocate its own buffer of `h` rows of
    // `w * 4` bytes; the colour space name is an immutable AppKit constant.
    let rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            ptr::null_mut(),
            w,
            h,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            w * 4,
            32,
        )
    }?;
    let data = rep.bitmapData();
    if data.is_null() {
        return None;
    }
    // SAFETY: the representation owns `len` bytes at `data` (chunky RGBA, no row padding because
    // bytesPerRow is exactly `w * 4`); nothing else reads or writes them during this copy.
    let dst = unsafe { std::slice::from_raw_parts_mut(data, len) };
    premultiply(dst, &rgba.pixels);
    rep.setSize(NSSize::new(points, points));
    let image = NSImage::initWithSize(NSImage::alloc(), NSSize::new(points, points));
    image.addRepresentation(&rep);
    Some(image)
}

/// Pixel size and straight-looking RGBA bytes of an image's first bitmap representation (tests).
#[allow(unsafe_code)]
pub fn image_pixels(image: &NSImage) -> Option<(usize, usize, Vec<u8>)> {
    let reps = image.representations();
    let rep = reps.iter().find_map(|r| r.downcast::<NSBitmapImageRep>().ok())?;
    let (w, h) = (usize::try_from(rep.pixelsWide()).ok()?, usize::try_from(rep.pixelsHigh()).ok()?);
    let stride = usize::try_from(rep.bytesPerRow()).ok()?;
    let data = rep.bitmapData();
    if data.is_null() || w == 0 || h == 0 {
        return None;
    }
    // SAFETY: the representation owns `bytesPerRow * pixelsHigh` readable bytes at `bitmapData`.
    let bytes = unsafe { std::slice::from_raw_parts(data, stride * h) };
    let mut out = Vec::with_capacity(w * h * 4);
    for row in bytes.chunks_exact(stride) {
        out.extend_from_slice(&row[..w * 4]);
    }
    Some((w, h, out))
}
