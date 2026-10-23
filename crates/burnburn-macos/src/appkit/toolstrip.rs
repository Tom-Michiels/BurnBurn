//! The tool groups of the document window (SolidCraft's ribbon in AppKit form), as the items of
//! the window's one `NSToolbar` (LF7): a view per panel holding the icon buttons of its tools, a
//! small arrow with a drop-down for variant groups, tooltips with the shortcut and the reason a
//! tool is disabled. The toolbar names each item by its panel title and offers a menu with the
//! panel's tools while the item does not fit (overflow).
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
    NSAccessibility, NSAppearanceCustomization, NSBezelStyle, NSBitmapImageRep, NSButton, NSButtonType,
    NSCellImagePosition, NSControlStateValueOff, NSControlStateValueOn, NSDeviceRGBColorSpace, NSFont, NSImage, NSMenu,
    NSMenuItem, NSView,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

use super::app::HostApp;
use super::classes::ToolStripView;
use crate::l10n::Locale;
use crate::toolbar::{panel_title, tag_for_id, tool_label, tooltip};

/// Height of a panel's view in the toolbar (the toolbar item label sits under it).
pub(crate) const PANEL_HEIGHT: f64 = 30.0;
const BUTTON_W: f64 = 30.0;
const BUTTON_H: f64 = 28.0;
const TEXT_BUTTON_W: f64 = 66.0;
const CARET_W: f64 = 11.0;
const GAP: f64 = 2.0;
const ICON_PT: u32 = 20;
const MENU_ICON_PT: u32 = 16;
const ROW_Y: f64 = 1.0;

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

/// One panel of the ribbon: the view that becomes a toolbar item, its title and overflow menu.
struct PanelEntry {
    panel: PanelId,
    title: String,
    view: Retained<ToolStripView>,
    /// The item the toolbar shows in its overflow menu: the panel's tools as a submenu.
    overflow: Retained<NSMenuItem>,
    /// Overflow entries with the catalog id they stand for.
    overflow_items: Vec<(&'static str, Retained<NSMenuItem>)>,
}

pub struct ToolStrip {
    panels: Vec<PanelEntry>,
    entries: Vec<Entry>,
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
    ) -> ToolStrip {
        let layout = ToolbarViewModel::build(
            burnburn_presentation::ToolCatalog::standard(),
            burnburn_presentation::ToolState { document: false, operator: false, in_sketch: false },
            burnburn_presentation::ActiveTool { tool: None, trim: false, polygon: None },
            &burnburn_presentation::VariantMemory::new(),
        );
        let mut entries = Vec::new();
        let mut panels = Vec::new();
        for panel in &layout.panels {
            if panel.groups.is_empty() {
                continue;
            }
            let view = ToolStripView::new(mtm, rect(0.0, 0.0, 10.0, PANEL_HEIGHT), app.clone(), document);
            let mut x = 0.0;
            for group in &panel.groups {
                let text_only = !burnburn_icons::has_icon(group.active.icon);
                let main_w = if text_only { TEXT_BUTTON_W } else { BUTTON_W };
                let main = new_button(mtm, target, sel!(toolButton:), rect(x, ROW_Y, main_w, BUTTON_H));
                let tag = tag_for_id(group.active.id).unwrap_or(-1);
                main.setTag(tag);
                view.addSubview(&main);
                x += main_w;
                let caret = group.group.map(|_| {
                    let c = new_button(mtm, target, sel!(variantMenu:), rect(x, ROW_Y, CARET_W, BUTTON_H));
                    // SF Symbol "chevron.down" instead of a glyph character in the title.
                    c.setImage(symbol_image("chevron.down", &tool_label(group.active.id, locale)).as_deref());
                    c.setTitle(&NSString::from_str(""));
                    c.setImagePosition(NSCellImagePosition::ImageOnly);
                    c.setTag(tag);
                    c.setBezelStyle(NSBezelStyle::Toolbar);
                    view.addSubview(&c);
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
            view.setFrameSize(NSSize::new((x - GAP).max(1.0), PANEL_HEIGHT));
            let title = panel_title(panel.panel, locale);
            let (overflow, overflow_items) = overflow_menu(mtm, target, &title, panel, locale);
            panels.push(PanelEntry { panel: panel.panel, title, view, overflow, overflow_items });
        }
        ToolStrip {
            panels,
            entries,
            target: target.retain(),
            locale,
            drawn: Cell::new(None),
            model: RefCell::new(layout),
            shortcuts: RefCell::new(std::collections::BTreeMap::new()),
        }
    }

    /// The view of panel `index` (what the toolbar shows as its item).
    pub fn panel_view(&self, index: usize) -> Option<Retained<NSView>> {
        self.panels.get(index).map(|p| Retained::into_super(p.view.clone()))
    }

    /// What the toolbar item with `identifier` is made of.
    pub(crate) fn panel_item(&self, identifier: &str) -> Option<PanelItem> {
        self.panels.iter().find(|p| panel_item_id(p.panel) == identifier).map(|p| PanelItem {
            title: p.title.clone(),
            view: Retained::into_super(p.view.clone()),
            overflow: p.overflow.clone(),
        })
    }

    /// Identifiers of the panel items, in panel order.
    pub(crate) fn panel_item_ids(&self) -> Vec<String> {
        self.panels.iter().map(|p| panel_item_id(p.panel)).collect()
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
        for p in &self.panels {
            for (id, item) in &p.overflow_items {
                if let Some(b) = vm.button(id) {
                    item.setEnabled(b.enablement.is_enabled());
                    item.setState(if b.selected { NSControlStateValueOn } else { NSControlStateValueOff });
                }
            }
        }
        self.drawn.set(Some((theme, scale)));
    }

    /// Clear the weak targets before the window controller goes.
    #[allow(unsafe_code)]
    pub(crate) fn clear_targets(&self) {
        for p in &self.panels {
            for (_, item) in &p.overflow_items {
                // SAFETY: a nil target cannot dangle.
                unsafe { item.setTarget(None) };
            }
        }
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

    /// Titles of the panels, as shown (the toolbar item labels).
    pub fn panel_titles(&self) -> Vec<String> {
        self.panels.iter().map(|p| p.title.clone()).collect()
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
        self.panels.first().map_or(Theme::Light, |p| theme_of(&p.view))
    }

    fn current_scale(&self) -> u32 {
        self.panels.first().map_or(2, |p| scale_of(&p.view))
    }
}

/// What a toolbar item for a ribbon panel is made of.
pub(crate) struct PanelItem {
    pub(crate) title: String,
    pub(crate) view: Retained<NSView>,
    pub(crate) overflow: Retained<NSMenuItem>,
}

/// Toolbar item identifier of a ribbon panel.
pub(crate) fn panel_item_id(panel: PanelId) -> String {
    format!("app.burnburn.toolbar.panel.{}", panel.key())
}

/// The overflow entry of a panel: its title with a submenu of the panel's tools (every variant),
/// each choosing the tool like its button. The entries follow the view model in `refresh`.
#[allow(unsafe_code)]
fn overflow_menu(
    mtm: MainThreadMarker,
    target: &AnyObject,
    title: &str,
    panel: &burnburn_presentation::PanelViewModel,
    locale: Locale,
) -> (Retained<NSMenuItem>, Vec<(&'static str, Retained<NSMenuItem>)>) {
    let submenu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(title));
    submenu.setAutoenablesItems(false);
    let mut items = Vec::new();
    for group in &panel.groups {
        let tools: Vec<&burnburn_presentation::ToolButton> =
            if group.variants.is_empty() { vec![&group.active] } else { group.variants.iter().collect() };
        for tool in tools {
            // SAFETY: a freshly allocated item; `toolButton:` takes the sender (`v@:@`) on the
            // window controller, which outlives its menus (the target is weak).
            let item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &NSString::from_str(&tool_label(tool.id, locale)),
                    Some(sel!(toolButton:)),
                    &NSString::from_str(""),
                )
            };
            // SAFETY: as above; the window clears the weak target in Drop.
            unsafe { item.setTarget(Some(target)) };
            item.setTag(tag_for_id(tool.id).unwrap_or(-1));
            submenu.addItem(&item);
            items.push((tool.id, item));
        }
    }
    let overflow = NSMenuItem::new(mtm);
    overflow.setTitle(&NSString::from_str(title));
    overflow.setSubmenu(Some(&submenu));
    (overflow, items)
}

/// An SF Symbol with its accessibility description (LF7/LF8: no glyph characters as titles).
/// Names used by the document window: `chevron.down` (variant arrows), `arrow.left.and.right` and
/// `arrow.up.and.down` (flip), `square.fill` (layer colour swatch).
pub(crate) fn symbol_image(name: &str, description: &str) -> Option<Retained<NSImage>> {
    NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str(name),
        Some(&NSString::from_str(description)),
    )
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
