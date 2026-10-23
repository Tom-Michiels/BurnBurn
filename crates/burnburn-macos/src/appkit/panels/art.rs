//! The Art Library panel (M210): a floating utility panel per document window with the groups,
//! a search, the items with the thumbnail of the chosen one, and the buttons that insert it into
//! the document, add a drawing file or the selection, and organise groups and items.
//!
//! The panel copies `panels_files::art_panel` into widgets; every control calls one of the intent
//! builders next to it. Dragging a thumbnail onto the canvas is not part of this panel yet: the
//! Plaats button inserts at the centre of the view. The widgets come from a form
//! (`files_form::art_form`, LF6b): sections with a heading, the buttons of a section in one row.

#![allow(unsafe_code)] // a panel is owned by its Retained (SAFETY below)

use std::rc::{Rc, Weak};

use burnburn_host_models::form::ButtonRole;
use burnburn_host_models::panels::files_form::{art as id, art_form};
use burnburn_presentation::AppViewModel;
use burnburn_types::DocumentId;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSAccessibility, NSButton, NSImageView, NSPanel, NSPopUpButton, NSTextField, NSWindow};
use objc2_foundation::NSString;

use super::files_ui::{FilesTarget, Link, open_file, set_popup, unwire};
use super::form_surface::{FormSurface, form_panel};
use super::image::show_bitmap;
use super::widgets::set_text;
use crate::appkit::app::HostApp;
use crate::appkit::window::DocWindow;
use crate::l10n::Locale;
use crate::l10n_files::{FilesText as T, files_text};
use crate::panels_files::{
    ART_FILE_TYPES, art_add_file_intents, art_add_group_intent, art_add_selection_intents, art_group_intent,
    art_item_intent, art_move_item_intent, art_panel, art_place_intent, art_remove_group_intent,
    art_remove_item_intent, art_rename_group_intent, art_rename_item_intent, art_retry_intents, art_search_intent,
};
use crate::panels_image::BitmapImage;

pub(crate) const WIDTH: f64 = 500.0;

pub(crate) struct ArtSection {
    pub(crate) panel: Retained<NSPanel>,
    surface: FormSurface,
    pub(crate) shown: bool,
    pub(crate) status: Retained<NSTextField>,
    pub(crate) groups: Retained<NSPopUpButton>,
    pub(crate) search: Retained<NSTextField>,
    pub(crate) items: Retained<NSPopUpButton>,
    pub(crate) thumbnail: Retained<NSImageView>,
    pub(crate) detail: Retained<NSTextField>,
    pub(crate) place: Retained<NSButton>,
    pub(crate) scale_prompt: Retained<NSTextField>,
    pub(crate) scale_unit: Retained<NSPopUpButton>,
    pub(crate) scale_retry: Retained<NSButton>,
    pub(crate) name: Retained<NSTextField>,
    pub(crate) add_file: Retained<NSButton>,
    pub(crate) add_selection: Retained<NSButton>,
    pub(crate) new_group: Retained<NSButton>,
    pub(crate) rename_group: Retained<NSButton>,
    pub(crate) remove_group: Retained<NSButton>,
    pub(crate) rename_item: Retained<NSButton>,
    pub(crate) move_item: Retained<NSButton>,
    pub(crate) remove_item: Retained<NSButton>,
    pub(crate) close: Retained<NSButton>,
    shown_thumbnail: Option<BitmapImage>,
}

pub(crate) fn art_section(
    mtm: MainThreadMarker,
    window: Retained<NSWindow>,
    target: &FilesTarget,
    app: Weak<HostApp>,
    document: DocumentId,
    locale: Locale,
) -> ArtSection {
    let spec = art_form(locale);
    let surface = FormSurface::new(mtm, &[(&spec, WIDTH)]);
    let panel = form_panel(mtm, &surface, files_text(T::ArtTitle, locale));
    let thumbnail = surface.image(id::THUMBNAIL);
    thumbnail.setAccessibilityLabel(Some(&NSString::from_str(files_text(T::ArtTitle, locale))));
    let section = ArtSection {
        panel,
        shown: false,
        status: surface.text(id::STATUS),
        groups: surface.popup(id::GROUPS),
        search: surface.text(id::SEARCH),
        items: surface.popup(id::ITEMS),
        thumbnail,
        detail: surface.text(id::DETAIL),
        place: surface.button(id::PLACE),
        scale_prompt: surface.text(id::SCALE_PROMPT),
        scale_unit: surface.popup(id::SCALE_UNIT),
        scale_retry: surface.button(id::SCALE_RETRY),
        name: surface.text(id::NAME),
        add_file: surface.button(id::ADD),
        add_selection: surface.button(id::ADD + 1),
        new_group: surface.button(id::GROUP_ACTIONS),
        rename_group: surface.button(id::GROUP_ACTIONS + 1),
        remove_group: surface.button(id::GROUP_ACTIONS + 2),
        rename_item: surface.button(id::ITEM_ACTIONS),
        move_item: surface.button(id::ITEM_ACTIONS + 1),
        remove_item: surface.button(id::ITEM_ACTIONS + 2),
        close: surface.dialog_button(ButtonRole::Cancel),
        shown_thumbnail: None,
        surface,
    };
    section.wire(mtm, window, target, app, document, locale);
    section
}

impl ArtSection {
    fn wire(
        &self,
        mtm: MainThreadMarker,
        window: Retained<NSWindow>,
        target: &FilesTarget,
        app: Weak<HostApp>,
        document: DocumentId,
        locale: Locale,
    ) {
        let link = Rc::new(Link { app, document });
        {
            let (link, groups) = (link.clone(), self.groups.clone());
            target.wire(&self.groups, move || {
                link.run(|app, doc, vm| {
                    if let Some(i) = art_group_intent(doc, vm, groups.indexOfSelectedItem()) {
                        app.dispatch(i);
                    }
                });
            });
        }
        {
            let (link, search) = (link.clone(), self.search.clone());
            target.wire(&self.search, move || {
                link.run(|app, doc, vm| {
                    if let Some(i) = art_search_intent(doc, vm, &search.stringValue().to_string()) {
                        app.dispatch(i);
                    }
                });
            });
        }
        {
            let (link, items) = (link.clone(), self.items.clone());
            target.wire(&self.items, move || {
                link.run(|app, doc, vm| {
                    if let Some(i) = art_item_intent(doc, vm, items.indexOfSelectedItem()) {
                        app.dispatch(i);
                    }
                });
            });
        }
        let plain =
            |button: &NSButton, build: fn(DocumentId, &AppViewModel) -> Option<burnburn_presentation::UiIntent>| {
                let link = link.clone();
                target.wire(button, move || {
                    link.run(|app, doc, vm| {
                        if let Some(i) = build(doc, vm) {
                            app.dispatch(i);
                        }
                    });
                });
            };
        plain(&self.place, art_place_intent);
        plain(&self.remove_item, art_remove_item_intent);
        plain(&self.remove_group, art_remove_group_intent);
        let named =
            |button: &NSButton,
             build: fn(DocumentId, &AppViewModel, &str) -> Option<burnburn_presentation::UiIntent>| {
                let (link, name) = (link.clone(), self.name.clone());
                target.wire(button, move || {
                    link.run(|app, doc, vm| {
                        if let Some(i) = build(doc, vm, &name.stringValue().to_string()) {
                            app.dispatch(i);
                        }
                    });
                });
            };
        named(&self.rename_item, art_rename_item_intent);
        named(&self.rename_group, art_rename_group_intent);
        {
            let (link, name) = (link.clone(), self.name.clone());
            target.wire(&self.new_group, move || {
                link.run(|app, doc, _| {
                    app.dispatch(art_add_group_intent(doc, &name.stringValue().to_string()));
                });
            });
        }
        {
            let (link, groups) = (link.clone(), self.groups.clone());
            target.wire(&self.move_item, move || {
                link.run(|app, doc, vm| {
                    if let Some(i) = art_move_item_intent(doc, vm, groups.indexOfSelectedItem()) {
                        app.dispatch(i);
                    }
                });
            });
        }
        {
            let (link, name) = (link.clone(), self.name.clone());
            target.wire(&self.add_selection, move || {
                link.run(|app, doc, vm| {
                    for i in art_add_selection_intents(doc, vm, &name.stringValue().to_string(), locale) {
                        app.dispatch(i);
                    }
                });
            });
        }
        {
            let (link, name, unit) = (link.clone(), self.name.clone(), self.scale_unit.clone());
            target.wire(&self.scale_retry, move || {
                link.run(|app, doc, vm| {
                    for i in
                        art_retry_intents(doc, vm, &name.stringValue().to_string(), unit.indexOfSelectedItem(), locale)
                    {
                        app.dispatch(i);
                    }
                });
            });
        }
        {
            let (link, name, unit) = (link.clone(), self.name.clone(), self.scale_unit.clone());
            target.wire(&self.add_file, move || {
                let (link, name, unit, window) = (link.clone(), name.clone(), unit.clone(), window.clone());
                open_file(
                    mtm,
                    Some(&window),
                    files_text(T::PanelArtOpen, locale),
                    &ART_FILE_TYPES,
                    "art file panel completion",
                    move |path| {
                        let Some(path) = path else { return };
                        link.run(|app, doc, vm| {
                            for i in art_add_file_intents(
                                doc,
                                vm,
                                &path,
                                &name.stringValue().to_string(),
                                unit.indexOfSelectedItem(),
                                locale,
                            ) {
                                app.dispatch(i);
                            }
                        });
                    },
                );
            });
        }
        {
            let link = link.clone();
            target.wire(&self.close, move || {
                link.run(|app, doc, _| {
                    app.dispatch(burnburn_presentation::UiIntent::Invoke {
                        document: Some(doc),
                        command: burnburn_presentation::CommandKey::ShowArtLibrary,
                    });
                });
            });
        }
    }

    pub(crate) fn clear_targets(&self) {
        let controls: [&objc2_app_kit::NSControl; 20] = [
            &self.groups,
            &self.search,
            &self.items,
            &self.place,
            &self.scale_unit,
            &self.scale_retry,
            &self.name,
            &self.add_file,
            &self.add_selection,
            &self.new_group,
            &self.rename_group,
            &self.remove_group,
            &self.rename_item,
            &self.move_item,
            &self.remove_item,
            &self.close,
            &self.status,
            &self.detail,
            &self.scale_prompt,
            &self.thumbnail,
        ];
        for c in controls {
            unwire(c);
        }
        if self.shown {
            self.panel.orderOut(None);
        }
    }
}

impl DocWindow {
    /// Show, update or hide the Art Library panel as presentation's panel comes and goes.
    pub(crate) fn refresh_art(&mut self, vm: &AppViewModel, document: DocumentId) {
        let locale = self.locale;
        let Some(model) = art_panel(vm, document, locale) else {
            if self.files.art.shown {
                self.files.art.shown = false;
                let panel = self.files.art.panel.clone();
                if let Err(e) = crate::appkit::objc_try("hiding the art panel", || panel.orderOut(None)) {
                    eprintln!("burnburn: {e}");
                }
            }
            return;
        };
        let a = &mut self.files.art;
        set_text(&a.status, &model.status);
        set_popup(&a.groups, &model.groups, true);
        let typing = a.panel.firstResponder().is_some_and(|r| {
            let object: &objc2::runtime::AnyObject = &r;
            object.downcast_ref::<objc2_app_kit::NSText>().is_some()
        });
        if !typing && a.search.stringValue().to_string() != model.search {
            a.search.setStringValue(&NSString::from_str(&model.search));
        }
        set_popup(&a.items, &model.items, true);
        show_bitmap(&a.thumbnail, &mut a.shown_thumbnail, model.thumbnail.as_ref());
        set_text(&a.detail, &model.detail);
        a.place.setEnabled(model.can_place);
        a.rename_item.setEnabled(model.can_edit_item);
        a.remove_item.setEnabled(model.can_edit_item);
        a.move_item.setEnabled(model.can_edit_item);
        a.rename_group.setEnabled(model.can_edit_group);
        a.remove_group.setEnabled(model.can_edit_group);
        let waiting = model.scale_prompt.is_some();
        set_text(&a.scale_prompt, model.scale_prompt.as_deref().unwrap_or(""));
        a.surface.set_hidden(id::SCALE_PROMPT, !waiting);
        a.surface.set_hidden(id::SCALE_UNIT, !waiting);
        a.surface.set_hidden(id::SCALE_RETRY, !waiting);
        a.surface.fit_window(&a.panel);
        if !a.shown {
            a.shown = true;
            let panel = a.panel.clone();
            let frame = self.window.frame();
            if let Err(e) = crate::appkit::objc_try("showing the art panel", || {
                panel.setFrameTopLeftPoint(objc2_foundation::NSPoint::new(
                    frame.origin.x + frame.size.width - WIDTH - 36.0,
                    frame.origin.y + frame.size.height - 76.0,
                ));
                panel.orderFront(None);
            }) {
                eprintln!("burnburn: {e}");
            }
        }
    }
}
