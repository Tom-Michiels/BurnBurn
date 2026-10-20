//! What the file panels show and send, without AppKit: the import report, the Material and Art
//! Library panels, the bundle sheet and the export dialog (W3-UI-bestanden).
//!
//! Every function here takes the presentation view model plus the raw widget values (indices and
//! unparsed text, DR-05) and gives texts, enablement or a `UiIntent`. The AppKit panels in
//! `appkit/panels/` only copy these into widgets and call the intent builders.

use std::path::{Path, PathBuf};

use burnburn_io::art_library::ArtFormat;
use burnburn_io::export::ExportLoss;
use burnburn_io::import::{CutKind, NoteKind};
use burnburn_presentation::{
    AppViewModel, ArtPanelViewModel, BundleChange, BundleConflict, BundleDialogViewModel, BundleKind, BundleMode,
    BundleProfileChange, ExportCurves, ExportDialogViewModel, ExportField, ExportFormat, ExportLayers,
    ExportOptionsInput, ExportUnit, FilesIntent, ImportKind, ImportPages, ImportUnit, ImportViewModel, Lbrn2Warning,
    MaterialLinkStatus, MaterialOrigin, MaterialPanelViewModel, MessageKey, OperationKind, TemplateScope, UiIntent,
};
use burnburn_types::DocumentId;

use crate::l10n::{Locale, message};
use crate::l10n_files::{FilesText as T, files_text};
use crate::panels_image::{BitmapImage, fill, number};

/// A pop-up: the item titles and the item to show selected.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Popup {
    pub items: Vec<String>,
    pub selected: Option<usize>,
}

fn t(text: T, locale: Locale) -> &'static str {
    files_text(text, locale)
}

fn index(i: isize) -> Option<usize> {
    usize::try_from(i).ok()
}

fn message_line(key: Option<MessageKey>, locale: Locale) -> String {
    key.and_then(|k| message(k, locale)).unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------
// Import report: pages and the details of every format.
// ---------------------------------------------------------------------------------------------

/// The page pop-up of a PDF/AI import: one item per page, then "all pages".
pub fn import_pages_popup(import: &ImportViewModel, locale: Locale) -> Popup {
    if import.pages.is_empty() {
        return Popup::default();
    }
    let mut items: Vec<String> = import
        .pages
        .iter()
        .enumerate()
        .map(|(i, p)| {
            fill(
                t(T::ImportPageItem, locale),
                &[&(i + 1).to_string(), &number(p.width_mm, 1, locale), &number(p.height_mm, 1, locale)],
            )
        })
        .collect();
    items.push(t(T::ImportAllPages, locale).to_owned());
    let selected = import.page_choice.map(|c| match c {
        ImportPages::One(i) => i.min(import.pages.len()),
        ImportPages::All => import.pages.len(),
    });
    Popup { items, selected }
}

/// The page choice of pop-up item `item` (the last one is "all pages").
pub fn import_pages_intent(document: DocumentId, vm: &AppViewModel, item: isize) -> Option<UiIntent> {
    let import = vm.document(document)?.import.as_ref()?;
    let item = index(item)?;
    let pages = if item == import.pages.len() {
        ImportPages::All
    } else if item < import.pages.len() {
        ImportPages::One(item)
    } else {
        return None;
    };
    Some(UiIntent::Files(FilesIntent::SetImportPages { document, pages }))
}

/// The line under the size that says where the millimetres come from, for the formats without a
/// unit question (the DXF and SVG lines live in `panels::import`).
pub fn import_unit_line(import: &ImportViewModel, locale: Locale) -> Option<String> {
    use burnburn_io::import::{HpglUnitBasis, UnitDecision};
    match (import.kind, import.units) {
        (ImportKind::Hpgl, Some(UnitDecision::Hpgl { basis: HpglUnitBasis::ScaledByIpSc, .. })) => {
            Some(t(T::UnitPlotterScaled, locale).to_owned())
        }
        (ImportKind::Hpgl, _) => Some(t(T::UnitPlotter, locale).to_owned()),
        (ImportKind::Pdf | ImportKind::Illustrator, _) => Some(t(T::UnitPdf, locale).to_owned()),
        (ImportKind::Lbrn2, _) => Some(t(T::UnitLightBurn, locale).to_owned()),
        _ => None,
    }
}

fn kind_name(kind: CutKind, locale: Locale) -> &'static str {
    match kind {
        CutKind::Line => t(T::KindLine, locale),
        CutKind::Fill => t(T::KindFill, locale),
        _ => t(T::KindImage, locale),
    }
}

fn note_kind_name(kind: NoteKind, locale: Locale) -> &'static str {
    match kind {
        NoteKind::UnknownElement => t(T::NoteUnknownElement, locale),
        NoteKind::UnknownAttribute => t(T::NoteUnknownAttribute, locale),
        NoteKind::UnmappedSetting => t(T::NoteUnmappedSetting, locale),
        NoteKind::UnsupportedShape => t(T::NoteUnsupportedShape, locale),
        NoteKind::NotConverted => t(T::NoteNotConverted, locale),
        _ => t(T::NoteInvalidValue, locale),
    }
}

/// The report below the import summary: proposed layers (plotter pens, PDF colours) or, for a
/// LightBurn project, every layer with what was and was not taken over, the warnings, bitmaps and
/// everything that was left out.
pub fn import_details(import: &ImportViewModel, locale: Locale) -> String {
    let mut lines: Vec<String> = Vec::new();
    if import.busy {
        lines.push(t(T::ImportReading, locale).to_owned());
    }
    if !import.layers.is_empty() {
        lines.push(t(T::LayersHeader, locale).to_owned());
        for layer in &import.layers {
            lines.push(fill(t(T::LayerLine, locale), &[&layer.name, &layer.items.to_string()]));
        }
    }
    let Some(report) = &import.lbrn2 else { return lines.join("\n") };
    for warning in &report.warnings {
        lines.push(match warning {
            Lbrn2Warning::OtherDevice { file, active } => fill(
                t(T::WarnDevice, locale),
                &[file, active.as_deref().unwrap_or_else(|| t(T::WarnNoProfile, locale))],
            ),
            Lbrn2Warning::MirroredOrigin { x, y } => {
                let mut axes = Vec::new();
                if *x {
                    axes.push(t(T::MirrorX, locale));
                }
                if *y {
                    axes.push(t(T::MirrorY, locale));
                }
                fill(t(T::WarnMirror, locale), &[&axes.join(" + ")])
            }
        });
    }
    if !report.layers.is_empty() {
        lines.push(t(T::ProjectLayersHeader, locale).to_owned());
        for layer in &report.layers {
            let template = if layer.taken { T::ProjectLayerTaken } else { T::ProjectLayerSkipped };
            lines.push(fill(
                t(template, locale),
                &[&layer.name, kind_name(layer.kind, locale), &layer.shapes.to_string()],
            ));
            if !layer.unmapped.is_empty() {
                let list: Vec<String> = layer
                    .unmapped
                    .iter()
                    .map(|u| match &u.value {
                        Some(v) => format!("{}={v}", u.name),
                        None => u.name.clone(),
                    })
                    .collect();
                lines.push(fill(t(T::ProjectUnmapped, locale), &[&list.join(", ")]));
            }
            for a in &layer.assumptions {
                lines.push(fill(t(T::ProjectAssumed, locale), &[a.field, &a.value]));
            }
        }
    }
    if !report.images.is_empty() {
        lines.push(t(T::ImagesHeader, locale).to_owned());
        for image in &report.images {
            use burnburn_presentation::Lbrn2ImageState as S;
            lines.push(match &image.state {
                S::Embedded => {
                    let (w, h) = image.size_mm.unwrap_or((0.0, 0.0));
                    fill(t(T::ImageEmbedded, locale), &[&image.entity, &number(w, 1, locale), &number(h, 1, locale)])
                }
                S::Linked(file) => fill(t(T::ImageLinked, locale), &[&image.entity, file]),
                _ => fill(t(T::ImageUnreadable, locale), &[&image.entity]),
            });
        }
    }
    if !report.notes.is_empty() {
        lines.push(t(T::NotesHeader, locale).to_owned());
        for n in &report.notes {
            lines.push(fill(t(T::NoteLine, locale), &[note_kind_name(n.kind, locale), &n.name, &n.count.to_string()]));
        }
    }
    for f in &report.fonts {
        lines.push(fill(t(T::FontLine, locale), &[&f.family, &f.texts.to_string(), &f.with_outlines.to_string()]));
    }
    if let Some(notes) = &report.project_notes {
        lines.push(t(T::ProjectNotesHeader, locale).to_owned());
        lines.push(notes.clone());
    }
    lines.join("\n")
}

// ---------------------------------------------------------------------------------------------
// Material Library panel.
// ---------------------------------------------------------------------------------------------

/// The Material Library panel's texts and states for the window's document.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialPanel {
    /// Loading, "not saved", or the issue of the last input; empty when there is nothing to say.
    pub status: String,
    pub materials: Popup,
    pub thickness: String,
    pub machines: Popup,
    pub entries: Popup,
    pub detail: String,
    pub operations: Popup,
    /// Whether an entry can be applied to the layer in the same pop-up slot.
    pub applicable: Vec<bool>,
}

fn mode_name(mode: OperationKind, locale: Locale) -> &'static str {
    match mode {
        OperationKind::Line => t(T::ModeLine, locale),
        OperationKind::Fill => t(T::ModeFill, locale),
        OperationKind::OffsetFill => t(T::ModeOffsetFill, locale),
        OperationKind::Image => t(T::ModeImage, locale),
        _ => t(T::ModeOther, locale),
    }
}

fn link_text(status: MaterialLinkStatus, locale: Locale) -> String {
    match status {
        MaterialLinkStatus::UpToDate => t(T::LinkUpToDate, locale).to_owned(),
        MaterialLinkStatus::UpdateAvailable { applied, latest } => {
            fill(t(T::LinkUpdate, locale), &[&applied.to_string(), &latest.to_string()])
        }
        MaterialLinkStatus::LibraryOlder { applied, latest } => {
            fill(t(T::LinkOlder, locale), &[&applied.to_string(), &latest.to_string()])
        }
        MaterialLinkStatus::EntryMissing => t(T::LinkMissing, locale).to_owned(),
        _ => t(T::LinkOther, locale).to_owned(),
    }
}

/// Where a layer's settings came from, in words: by hand, copied from a recipe, or copied from a
/// recipe that was copied from a library entry (with whether the library has moved on since).
pub fn origin_text(origin: &MaterialOrigin, locale: Locale) -> String {
    match origin {
        MaterialOrigin::Manual => t(T::OriginManual, locale).to_owned(),
        MaterialOrigin::Copied { recipe_name, revision, .. } => {
            fill(t(T::OriginCopied, locale), &[recipe_name, &revision.to_string()])
        }
        MaterialOrigin::Linked { recipe_name, revision, entry, status, .. } => fill(
            t(T::OriginLinked, locale),
            &[recipe_name, &revision.to_string(), &entry.to_string(), &link_text(*status, locale)],
        ),
    }
}

pub fn material_panel(vm: &AppViewModel, document: DocumentId, locale: Locale) -> Option<MaterialPanel> {
    let p: &MaterialPanelViewModel = vm.document(document)?.files.material.as_ref()?;
    let status = if p.loading {
        t(T::Loading, locale).to_owned()
    } else if p.issue.is_some() {
        message_line(p.issue, locale)
    } else if !p.persisted {
        t(T::NotSaved, locale).to_owned()
    } else {
        String::new()
    };
    let mut materials = vec![t(T::AllMaterials, locale).to_owned()];
    materials.extend(p.materials.iter().cloned());
    let material_sel = match &p.filter.material {
        Some(m) => p.materials.iter().position(|x| x == m).map(|i| i + 1),
        None => Some(0),
    };
    let mut machines = vec![t(T::AllMachines, locale).to_owned()];
    machines.extend(p.machines.iter().cloned());
    let machine_sel = match &p.filter.machine {
        Some(m) => p.machines.iter().position(|x| x == m).map(|i| i + 1),
        None => Some(0),
    };
    let entries: Vec<String> = if p.entries.is_empty() {
        vec![t(T::NoEntries, locale).to_owned()]
    } else {
        p.entries
            .iter()
            .map(|e| {
                fill(
                    t(T::EntryItem, locale),
                    &[&e.name, &e.material, &number(e.thickness_mm, 2, locale), &e.revision.to_string()],
                )
            })
            .collect()
    };
    let selected_entry = p.selected.and_then(|id| p.entries.iter().position(|e| e.id == id));
    let detail = selected_entry
        .map(|i| {
            let e = &p.entries[i];
            let mut text = fill(t(T::EntryDetail, locale), &[mode_name(e.mode, locale), &e.speed, &e.power, &e.passes]);
            if !e.notes.trim().is_empty() {
                text.push('\n');
                text.push_str(&e.notes);
            }
            text
        })
        .unwrap_or_default();
    let operations: Vec<String> = if p.operations.is_empty() {
        vec![t(T::NoLayers, locale).to_owned()]
    } else {
        p.operations
            .iter()
            .map(|o| {
                if o.applicable {
                    format!("{} — {}", o.name, origin_text(&o.origin, locale))
                } else {
                    format!("{} {}", o.name, t(T::LayerNotApplicable, locale))
                }
            })
            .collect()
    };
    Some(MaterialPanel {
        status,
        materials: Popup { items: materials, selected: material_sel },
        thickness: p.filter.thickness_text.clone(),
        machines: Popup { items: machines, selected: machine_sel },
        entries: Popup { items: entries, selected: selected_entry.or((!p.entries.is_empty()).then_some(0)) },
        detail,
        operations: Popup { items: operations, selected: (!p.operations.is_empty()).then_some(0) },
        applicable: p.operations.iter().map(|o| o.applicable).collect(),
    })
}

/// Which buttons of the Material panel can act now, for the layer in pop-up slot `operation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MaterialActions {
    pub apply: bool,
    /// Update the recipe the layer came from (a linked layer whose entry still exists).
    pub update: bool,
    pub overwrite: bool,
    pub remove: bool,
    pub save: bool,
}

pub fn material_actions(vm: &AppViewModel, document: DocumentId, operation: isize) -> MaterialActions {
    let Some(p) = vm.document(document).and_then(|d| d.files.material.as_ref()) else {
        return MaterialActions::default();
    };
    let op = index(operation).and_then(|i| p.operations.get(i));
    let entry = p.selected.is_some_and(|id| p.entries.iter().any(|e| e.id == id));
    let usable = op.is_some_and(|o| o.applicable);
    let linked = op.is_some_and(|o| {
        matches!(
            &o.origin,
            MaterialOrigin::Linked { status, .. }
                if !matches!(status, MaterialLinkStatus::EntryMissing | MaterialLinkStatus::OtherLibrary)
        )
    });
    MaterialActions {
        apply: !p.loading && entry && usable,
        update: !p.loading && linked,
        overwrite: !p.loading && entry && usable,
        remove: !p.loading && entry,
        save: !p.loading && usable,
    }
}

pub fn material_filter_intent(
    document: DocumentId,
    vm: &AppViewModel,
    material: isize,
    thickness: &str,
    machine: isize,
) -> Option<UiIntent> {
    let p = vm.document(document)?.files.material.as_ref()?;
    let pick = |names: &[String], item: isize| match index(item)? {
        0 => Some(None),
        i => names.get(i - 1).map(|n| Some(n.clone())),
    };
    Some(UiIntent::Files(FilesIntent::SetMaterialFilter {
        document,
        material: pick(&p.materials, material)?,
        thickness: thickness.to_owned(),
        machine: pick(&p.machines, machine)?,
    }))
}

pub fn material_select_intent(document: DocumentId, vm: &AppViewModel, item: isize) -> Option<UiIntent> {
    let p = vm.document(document)?.files.material.as_ref()?;
    let entry = p.entries.get(index(item)?)?.id;
    Some(UiIntent::Files(FilesIntent::SelectMaterialEntry { document, entry: Some(entry) }))
}

pub fn material_apply_intent(document: DocumentId, vm: &AppViewModel, operation: isize) -> Option<UiIntent> {
    let actions = material_actions(vm, document, operation);
    let p = vm.document(document)?.files.material.as_ref()?;
    let op = p.operations.get(index(operation)?)?;
    actions.apply.then_some(UiIntent::Files(FilesIntent::ApplyMaterial {
        document,
        entry: p.selected?,
        operations: vec![op.id],
    }))
}

/// Update the recipe behind the layer; `reapply` copies it on into the layers set from it.
pub fn material_update_intent(
    document: DocumentId,
    vm: &AppViewModel,
    operation: isize,
    reapply: bool,
) -> Option<UiIntent> {
    if !material_actions(vm, document, operation).update {
        return None;
    }
    let p = vm.document(document)?.files.material.as_ref()?;
    let MaterialOrigin::Linked { recipe, .. } = &p.operations.get(index(operation)?)?.origin else { return None };
    Some(UiIntent::Files(FilesIntent::UpdateLinkedRecipe { document, recipe: *recipe, reapply }))
}

pub fn material_save_intent(
    document: DocumentId,
    vm: &AppViewModel,
    operation: isize,
    name: &str,
    material: &str,
    thickness: &str,
) -> Option<UiIntent> {
    if !material_actions(vm, document, operation).save {
        return None;
    }
    let p = vm.document(document)?.files.material.as_ref()?;
    let op = p.operations.get(index(operation)?)?;
    Some(UiIntent::Files(FilesIntent::SaveMaterial {
        document,
        operation: op.id,
        name: name.to_owned(),
        material: material.to_owned(),
        thickness: thickness.to_owned(),
        notes: String::new(),
    }))
}

pub fn material_overwrite_intent(document: DocumentId, vm: &AppViewModel, operation: isize) -> Option<UiIntent> {
    if !material_actions(vm, document, operation).overwrite {
        return None;
    }
    let p = vm.document(document)?.files.material.as_ref()?;
    let op = p.operations.get(index(operation)?)?;
    Some(UiIntent::Files(FilesIntent::UpdateMaterialFromOperation { document, entry: p.selected?, operation: op.id }))
}

pub fn material_remove_intent(document: DocumentId, vm: &AppViewModel) -> Option<UiIntent> {
    let p = vm.document(document)?.files.material.as_ref()?;
    Some(UiIntent::Files(FilesIntent::RemoveMaterial { document, entry: p.selected? }))
}

// ---------------------------------------------------------------------------------------------
// Art Library panel.
// ---------------------------------------------------------------------------------------------

/// Units the DXF pop-up of the Art Library offers (a DXF without a unit waits for one).
pub const ART_UNITS: [ImportUnit; 3] = [ImportUnit::Millimeter, ImportUnit::Centimeter, ImportUnit::Inch];

#[derive(Debug, Clone, PartialEq)]
pub struct ArtPanel {
    pub status: String,
    pub groups: Popup,
    pub search: String,
    pub items: Popup,
    pub thumbnail: Option<BitmapImage>,
    pub detail: String,
    /// "{file} names no unit…" while a DXF waits for its unit.
    pub scale_prompt: Option<String>,
    pub can_place: bool,
    pub can_edit_item: bool,
    pub can_edit_group: bool,
}

fn thumbnail_bitmap(t: &burnburn_presentation::ArtThumbnail) -> BitmapImage {
    let rgba = t.coverage.iter().flat_map(|c| [255 - c, 255 - c, 255 - c, 255]).collect();
    BitmapImage { width: t.width, height: t.height, rgba }
}

fn format_name(format: ArtFormat) -> &'static str {
    match format {
        ArtFormat::Svg => "SVG",
        ArtFormat::Dxf => "DXF",
        ArtFormat::Hpgl => "HP-GL",
        _ => "PDF/AI",
    }
}

pub fn art_panel(vm: &AppViewModel, document: DocumentId, locale: Locale) -> Option<ArtPanel> {
    let p: &ArtPanelViewModel = vm.document(document)?.files.art.as_ref()?;
    let status = if p.loading {
        t(T::Loading, locale).to_owned()
    } else if p.busy && p.issue.is_none() {
        t(T::ArtBusy, locale).to_owned()
    } else if p.issue.is_some() {
        message_line(p.issue, locale)
    } else if !p.persisted {
        t(T::NotSaved, locale).to_owned()
    } else {
        String::new()
    };
    let mut groups = vec![t(T::AllGroups, locale).to_owned()];
    groups.extend(p.groups.iter().map(|g| fill(t(T::GroupItem, locale), &[&g.name, &g.items.to_string()])));
    let group_sel = match &p.group {
        Some(g) => p.groups.iter().position(|x| &x.name == g).map(|i| i + 1),
        None => Some(0),
    };
    let items: Vec<String> = if p.items.is_empty() {
        vec![t(T::NoItems, locale).to_owned()]
    } else {
        p.items.iter().map(|i| fill(t(T::ArtItem, locale), &[&i.name, &i.group])).collect()
    };
    let selected = p.selected.and_then(|id| p.items.iter().position(|i| i.id == id));
    let chosen = selected.map(|i| &p.items[i]);
    Some(ArtPanel {
        status,
        groups: Popup { items: groups, selected: group_sel },
        search: p.search.clone(),
        items: Popup { items, selected: selected.or((!p.items.is_empty()).then_some(0)) },
        thumbnail: chosen.and_then(|i| i.thumbnail.as_deref()).map(thumbnail_bitmap),
        detail: chosen.map(|i| fill(t(T::ArtDetail, locale), &[format_name(i.format)])).unwrap_or_default(),
        scale_prompt: p.needs_scale.as_ref().map(|f| {
            let name = f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            fill(t(T::ArtNeedsScale, locale), &[&name])
        }),
        can_place: !p.loading && chosen.is_some(),
        can_edit_item: !p.loading && chosen.is_some(),
        can_edit_group: !p.loading && p.group.is_some(),
    })
}

fn art_view(vm: &AppViewModel, document: DocumentId) -> Option<&ArtPanelViewModel> {
    vm.document(document)?.files.art.as_ref()
}

pub fn art_group_intent(document: DocumentId, vm: &AppViewModel, item: isize) -> Option<UiIntent> {
    let p = art_view(vm, document)?;
    let group = match index(item)? {
        0 => None,
        i => Some(p.groups.get(i - 1)?.name.clone()),
    };
    Some(UiIntent::Files(FilesIntent::SelectArtGroup { document, group }))
}

pub fn art_search_intent(document: DocumentId, vm: &AppViewModel, text: &str) -> Option<UiIntent> {
    (art_view(vm, document)?.search != text)
        .then(|| UiIntent::Files(FilesIntent::SetArtSearch { document, text: text.to_owned() }))
}

pub fn art_item_intent(document: DocumentId, vm: &AppViewModel, item: isize) -> Option<UiIntent> {
    let id = art_view(vm, document)?.items.get(index(item)?)?.id;
    Some(UiIntent::Files(FilesIntent::SelectArtItem { document, item: Some(id) }))
}

fn selected_item(vm: &AppViewModel, document: DocumentId) -> Option<u64> {
    let p = art_view(vm, document)?;
    p.selected.filter(|id| p.items.iter().any(|i| i.id == *id))
}

pub fn art_place_intent(document: DocumentId, vm: &AppViewModel) -> Option<UiIntent> {
    Some(UiIntent::Files(FilesIntent::PlaceArt { document, item: selected_item(vm, document)?, at: None }))
}

pub fn art_remove_item_intent(document: DocumentId, vm: &AppViewModel) -> Option<UiIntent> {
    Some(UiIntent::Files(FilesIntent::RemoveArtItem { document, item: selected_item(vm, document)? }))
}

pub fn art_rename_item_intent(document: DocumentId, vm: &AppViewModel, name: &str) -> Option<UiIntent> {
    Some(UiIntent::Files(FilesIntent::RenameArtItem {
        document,
        item: selected_item(vm, document)?,
        name: name.to_owned(),
    }))
}

/// Moves the chosen item into the group chosen in the group pop-up (not "all groups").
pub fn art_move_item_intent(document: DocumentId, vm: &AppViewModel, group_item: isize) -> Option<UiIntent> {
    let p = art_view(vm, document)?;
    let group = p.groups.get(index(group_item)?.checked_sub(1)?)?.name.clone();
    Some(UiIntent::Files(FilesIntent::MoveArtItem { document, item: selected_item(vm, document)?, group }))
}

pub fn art_add_group_intent(document: DocumentId, name: &str) -> UiIntent {
    UiIntent::Files(FilesIntent::AddArtGroup { document, name: name.to_owned() })
}

pub fn art_rename_group_intent(document: DocumentId, vm: &AppViewModel, new: &str) -> Option<UiIntent> {
    let old = art_view(vm, document)?.group.clone()?;
    Some(UiIntent::Files(FilesIntent::RenameArtGroup { document, old, new: new.to_owned() }))
}

pub fn art_remove_group_intent(document: DocumentId, vm: &AppViewModel) -> Option<UiIntent> {
    let name = art_view(vm, document)?.group.clone()?;
    Some(UiIntent::Files(FilesIntent::RemoveArtGroup { document, name }))
}

/// Adding a file or the selection: into the group shown, else the first group; a library without
/// any group gets "Algemeen"/"General" first. The unit is only sent for a DXF that asked for one.
pub fn art_add_file_intents(
    document: DocumentId,
    vm: &AppViewModel,
    path: &Path,
    name: &str,
    unit_item: isize,
    locale: Locale,
) -> Vec<UiIntent> {
    let Some(p) = art_view(vm, document) else { return Vec::new() };
    let (group, mut intents) = target_group(document, p, locale);
    let dxf_unit = (p.needs_scale.as_deref() == Some(path))
        .then(|| index(unit_item).and_then(|i| ART_UNITS.get(i)).copied())
        .flatten();
    intents.push(UiIntent::Files(FilesIntent::AddArtFromFile {
        document,
        path: path.to_path_buf(),
        group,
        name: name.to_owned(),
        dxf_unit,
    }));
    intents
}

pub fn art_add_selection_intents(document: DocumentId, vm: &AppViewModel, name: &str, locale: Locale) -> Vec<UiIntent> {
    let Some(p) = art_view(vm, document) else { return Vec::new() };
    let (group, mut intents) = target_group(document, p, locale);
    let name = if name.trim().is_empty() { "Selectie".to_owned() } else { name.to_owned() };
    intents.push(UiIntent::Files(FilesIntent::AddArtFromSelection { document, name, group }));
    intents
}

fn target_group(document: DocumentId, p: &ArtPanelViewModel, locale: Locale) -> (String, Vec<UiIntent>) {
    if let Some(g) = p.group.clone().or_else(|| p.groups.first().map(|g| g.name.clone())) {
        return (g, Vec::new());
    }
    let general = match locale {
        Locale::Nl => "Algemeen",
        Locale::En => "General",
    };
    (general.to_owned(), vec![art_add_group_intent(document, general)])
}

// ---------------------------------------------------------------------------------------------
// Bundle sheet.
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct BundleKindRow {
    pub kind: BundleKind,
    pub title: String,
    /// All, some or none of the kind is in the bundle: a tri-state check box.
    pub state: CheckState,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckState {
    Off,
    Mixed,
    On,
}

fn check_state(selected: usize, total: usize) -> CheckState {
    match (selected, total) {
        (0, _) => CheckState::Off,
        (s, t) if s == t => CheckState::On,
        _ => CheckState::Mixed,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BundleProfileChoice {
    pub id: String,
    pub title: String,
    pub replace: bool,
    /// Only a profile that differs from the local one can be replaced.
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BundlePanel {
    pub title: String,
    pub import: bool,
    pub status: String,
    /// Export: the four kinds with their check boxes.
    pub kinds: Vec<BundleKindRow>,
    /// Import: the preview, line by line.
    pub preview: String,
    pub conflict: Popup,
    pub profiles: Vec<BundleProfileChoice>,
    pub busy: bool,
    pub can_export: bool,
    pub can_apply: bool,
    /// Import after applying: the report.
    pub report: String,
}

/// Choices of the conflict pop-up, top to bottom.
pub const CONFLICTS: [BundleConflict; 3] = [BundleConflict::Skip, BundleConflict::Replace, BundleConflict::KeepBoth];

fn change_name(change: BundleChange, locale: Locale) -> &'static str {
    match change {
        BundleChange::New => t(T::ChangeNew, locale),
        BundleChange::Identical => t(T::ChangeIdentical, locale),
        _ => t(T::ChangeConflict, locale),
    }
}

pub fn bundle_panel(vm: &AppViewModel, document: DocumentId, locale: Locale) -> Option<BundlePanel> {
    let d: &BundleDialogViewModel = vm.document(document)?.files.bundle.as_ref()?;
    let status = if d.issue.is_some() {
        message_line(d.issue, locale)
    } else if d.loading {
        match &d.mode {
            BundleMode::Import(_) => t(T::BundleReading, locale).to_owned(),
            _ => t(T::Loading, locale).to_owned(),
        }
    } else if d.busy {
        t(T::BundleBusy, locale).to_owned()
    } else if let BundleMode::Export(e) = &d.mode
        && let Some(path) = &e.written
    {
        fill(t(T::BundleWritten, locale), &[&path.display().to_string()])
    } else {
        String::new()
    };
    let mut panel = BundlePanel {
        title: String::new(),
        import: false,
        status,
        kinds: Vec::new(),
        preview: String::new(),
        conflict: Popup::default(),
        profiles: Vec::new(),
        busy: d.busy,
        can_export: false,
        can_apply: false,
        report: String::new(),
    };
    match &d.mode {
        BundleMode::Export(e) => {
            panel.title = t(T::BundleExportTitle, locale).to_owned();
            let row = |kind, label: T, selected: usize, total: usize| BundleKindRow {
                kind,
                title: fill(t(T::KindCount, locale), &[t(label, locale), &selected.to_string(), &total.to_string()]),
                state: check_state(selected, total),
                enabled: total > 0 && !d.loading && !d.busy,
            };
            let count = |v: &[burnburn_presentation::BundleChoice<_>]| v.iter().filter(|c| c.selected).count();
            panel.kinds = vec![
                row(
                    BundleKind::Materials,
                    T::KindMaterials,
                    e.materials.iter().filter(|c| c.selected).count(),
                    e.materials.len(),
                ),
                row(BundleKind::Art, T::KindArt, e.art.iter().filter(|c| c.selected).count(), e.art.len()),
                row(BundleKind::Profiles, T::KindProfiles, count(&e.profiles), e.profiles.len()),
                row(BundleKind::Templates, T::KindTemplates, count(&e.templates), e.templates.len()),
            ];
            panel.can_export = !d.loading && !d.busy && panel.kinds.iter().any(|k| k.state != CheckState::Off);
        }
        BundleMode::Import(i) => {
            panel.import = true;
            panel.title = t(T::BundleImportTitle, locale).to_owned();
            let mut lines = vec![i.file.clone()];
            let mut section = |header: T, rows: Vec<(String, BundleChange)>| {
                if !rows.is_empty() {
                    lines.push(t(header, locale).to_owned());
                    for (label, change) in rows {
                        lines.push(fill(t(T::BundleRow, locale), &[&label, change_name(change, locale)]));
                    }
                }
            };
            section(T::BundleSectionMaterials, i.materials.iter().map(|r| (r.label.clone(), r.change)).collect());
            section(T::BundleSectionArt, i.art.iter().map(|r| (r.label.clone(), r.change)).collect());
            section(T::BundleSectionTemplates, i.templates.iter().map(|r| (r.label.clone(), r.change)).collect());
            if !i.profiles.is_empty() {
                lines.push(t(T::BundleSectionProfiles, locale).to_owned());
                for p in &i.profiles {
                    let local = p.local_revision.map(|r| r.to_string()).unwrap_or_default();
                    lines.push(match &p.change {
                        BundleProfileChange::New => {
                            fill(t(T::BundleProfileNew, locale), &[&p.id, &p.bundle_revision.to_string()])
                        }
                        BundleProfileChange::Identical => {
                            fill(t(T::BundleProfileIdentical, locale), &[&p.id, &p.bundle_revision.to_string(), &local])
                        }
                        BundleProfileChange::Differs { fields } => fill(
                            t(T::BundleProfileDiffers, locale),
                            &[&p.id, &p.bundle_revision.to_string(), &local, &fields.join(", ")],
                        ),
                    });
                }
                lines.push(t(T::ProfileKept, locale).to_owned());
            }
            panel.preview = lines.join("\n");
            panel.conflict = Popup {
                items: vec![
                    t(T::ConflictSkip, locale).to_owned(),
                    t(T::ConflictReplace, locale).to_owned(),
                    t(T::ConflictKeepBoth, locale).to_owned(),
                ],
                selected: CONFLICTS.iter().position(|c| *c == i.conflict),
            };
            panel.profiles = i
                .profiles
                .iter()
                .map(|p| BundleProfileChoice {
                    id: p.id.clone(),
                    title: fill(t(T::ReplaceProfile, locale), &[&p.id]),
                    replace: p.replace,
                    enabled: matches!(p.change, BundleProfileChange::Differs { .. }) && i.report.is_none(),
                })
                .collect();
            if let Some(r) = &i.report {
                let mut text = fill(
                    t(T::BundleReport, locale),
                    &[
                        &r.materials_added.to_string(),
                        &r.materials_replaced.to_string(),
                        &r.art_added.to_string(),
                        &r.art_replaced.to_string(),
                        &r.templates_written.to_string(),
                        &r.profiles_added.to_string(),
                        &r.profiles_replaced.to_string(),
                        &r.skipped.to_string(),
                    ],
                );
                if !r.profiles_pending.is_empty() {
                    text.push('\n');
                    text.push_str(&fill(t(T::BundleProfilesPending, locale), &[&r.profiles_pending.join(", ")]));
                }
                panel.report = text;
            }
            panel.can_apply = !d.loading && !d.busy && i.report.is_none();
        }
    }
    Some(panel)
}

pub fn bundle_kind_intent(document: DocumentId, vm: &AppViewModel, row: isize, on: bool) -> Option<UiIntent> {
    let panel = bundle_panel(vm, document, Locale::En)?;
    let kind = panel.kinds.get(index(row)?)?.kind;
    Some(UiIntent::Files(FilesIntent::SetBundleKind { document, kind, selected: on }))
}

pub fn bundle_conflict_intent(document: DocumentId, item: isize) -> Option<UiIntent> {
    let choice = *CONFLICTS.get(index(item)?)?;
    Some(UiIntent::Files(FilesIntent::SetBundleConflict { document, choice }))
}

pub fn bundle_profile_intent(document: DocumentId, vm: &AppViewModel, slot: usize, replace: bool) -> Option<UiIntent> {
    let panel = bundle_panel(vm, document, Locale::En)?;
    let profile = panel.profiles.get(slot).filter(|p| p.enabled)?.id.clone();
    Some(UiIntent::Files(FilesIntent::SetBundleProfileReplace { document, profile, replace }))
}

pub fn bundle_apply_intent(document: DocumentId) -> UiIntent {
    UiIntent::Files(FilesIntent::ApplyBundleImport { document })
}

pub fn bundle_write_intent(document: DocumentId, path: PathBuf) -> UiIntent {
    UiIntent::Files(FilesIntent::ExportBundle { document, path })
}

pub fn bundle_open_intent(document: DocumentId, path: PathBuf) -> UiIntent {
    UiIntent::Files(FilesIntent::OpenBundleImport { document, path })
}

pub fn bundle_close_intent(document: DocumentId) -> UiIntent {
    UiIntent::Files(FilesIntent::CloseBundle { document })
}

// ---------------------------------------------------------------------------------------------
// Export dialog.
// ---------------------------------------------------------------------------------------------

/// What the export dialog's controls hold (indices and unparsed text).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportForm {
    pub selection_only: bool,
    pub unit: isize,
    pub layers: isize,
    pub use_colors: bool,
    pub stroke: String,
    pub decimals: String,
    pub curves: isize,
    pub tolerance: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportPanel {
    pub title: String,
    pub objects: String,
    pub units: Popup,
    pub layers: Popup,
    pub curves: Popup,
    pub selection_only: bool,
    pub use_colors: bool,
    pub stroke: String,
    pub decimals: String,
    pub tolerance: String,
    /// Which controls apply to the format.
    pub show_stroke: bool,
    pub show_decimals: bool,
    pub show_curves: bool,
    pub show_tolerance: bool,
    pub field_error: Option<ExportField>,
    pub losses: String,
    pub show_acknowledge: bool,
    pub acknowledged: bool,
    pub acknowledge_title: String,
    pub status: String,
    pub can_export: bool,
}

fn unit_items(format: ExportFormat) -> &'static [ExportUnit] {
    match format {
        ExportFormat::Svg => &[ExportUnit::Millimeter, ExportUnit::Inch, ExportUnit::Pixel],
        _ => &[ExportUnit::Millimeter, ExportUnit::Inch],
    }
}

fn layer_items(format: ExportFormat) -> &'static [ExportLayers] {
    match format {
        ExportFormat::Svg => &[ExportLayers::Object, ExportLayers::Layer],
        _ => &[ExportLayers::Object, ExportLayers::Layer, ExportLayers::Single],
    }
}

const CURVE_ITEMS: [ExportCurves; 2] = [ExportCurves::Splines, ExportCurves::Polylines];

/// One line about what an export would lose.
pub fn loss_text(loss: &ExportLoss, locale: Locale) -> String {
    let mm = |v: f64| number(v, 4, locale);
    match loss {
        ExportLoss::EllipticalArcAsSpline { max_deviation_mm, .. } => {
            fill(t(T::LossElliptical, locale), &[&mm(*max_deviation_mm)])
        }
        ExportLoss::CircularArcAsSpline { max_deviation_mm, .. } => {
            fill(t(T::LossCircular, locale), &[&mm(*max_deviation_mm)])
        }
        ExportLoss::ConstraintsDropped { count, .. } => fill(t(T::LossConstraints, locale), &[&count.to_string()]),
        ExportLoss::ModifiersApplied { count, .. } => fill(t(T::LossModifiers, locale), &[&count.to_string()]),
        ExportLoss::ParametricFlattened { .. } => t(T::LossParametric, locale).to_owned(),
        ExportLoss::TextAsOutlines { .. } => t(T::LossText, locale).to_owned(),
        ExportLoss::CurvesFlattened { max_deviation_mm, .. } => {
            fill(t(T::LossFlattened, locale), &[&mm(*max_deviation_mm)])
        }
        ExportLoss::ColorApproximated { .. } => t(T::LossColor, locale).to_owned(),
        _ => String::new(),
    }
}

pub fn export_panel(vm: &AppViewModel, document: DocumentId, locale: Locale) -> Option<ExportPanel> {
    let d: &ExportDialogViewModel = vm.document(document)?.files.export.as_ref()?;
    let svg = d.format == ExportFormat::Svg;
    let o = &d.options;
    let popup = |items: Vec<String>, selected: Option<usize>| Popup { items, selected };
    let units = unit_items(d.format);
    let layers = layer_items(d.format);
    let layer_title = |l: ExportLayers| match (l, svg) {
        (ExportLayers::Object, true) => t(T::LayersObjectSvg, locale),
        (ExportLayers::Object, false) => t(T::LayersObject, locale),
        (ExportLayers::Layer, true) => t(T::LayersLayerSvg, locale),
        (ExportLayers::Layer, false) => t(T::LayersLayer, locale),
        _ => t(T::LayersSingle, locale),
    };
    let unit_title = |u: ExportUnit| match u {
        ExportUnit::Millimeter => t(T::UnitMm, locale),
        ExportUnit::Inch => t(T::UnitInch, locale),
        _ => t(T::UnitPixel, locale),
    };
    let losses = if d.checking {
        t(T::ExportChecking, locale).to_owned()
    } else if d.losses.is_empty() {
        t(T::ExportNoLoss, locale).to_owned()
    } else {
        let mut lines = vec![t(T::ExportLossHeader, locale).to_owned()];
        lines.extend(d.losses.iter().map(|l| {
            fill(
                t(T::LossLine, locale),
                &[if l.object.is_empty() { "—" } else { &l.object }, &loss_text(&l.loss, locale)],
            )
        }));
        lines.join("\n")
    };
    let status = if d.issue.is_some() {
        message_line(d.issue, locale)
    } else if let Some(path) = &d.written {
        fill(t(T::ExportWritten, locale), &[&path.display().to_string()])
    } else {
        String::new()
    };
    Some(ExportPanel {
        title: t(if svg { T::ExportSvgTitle } else { T::ExportDxfTitle }, locale).to_owned(),
        objects: fill(t(T::ExportObjects, locale), &[&d.objects.to_string()]),
        units: popup(
            units.iter().map(|u| unit_title(*u).to_owned()).collect(),
            units.iter().position(|u| *u == o.unit),
        ),
        layers: popup(
            layers.iter().map(|l| layer_title(*l).to_owned()).collect(),
            layers.iter().position(|l| *l == o.layers),
        ),
        curves: popup(
            vec![t(T::CurvesSplines, locale).to_owned(), t(T::CurvesPolylines, locale).to_owned()],
            CURVE_ITEMS.iter().position(|c| *c == o.curves),
        ),
        selection_only: o.selection_only,
        use_colors: o.use_colors,
        stroke: o.stroke_mm.clone(),
        decimals: o.decimals.clone(),
        tolerance: o.tolerance_mm.clone(),
        show_stroke: svg,
        show_decimals: svg,
        show_curves: !svg,
        show_tolerance: !svg && o.curves == ExportCurves::Polylines,
        field_error: d.field_error,
        losses,
        show_acknowledge: d.needs_acknowledgement,
        acknowledged: d.acknowledged,
        acknowledge_title: t(T::ExportAcknowledge, locale).to_owned(),
        status,
        can_export: d.objects > 0 && !d.checking && !d.busy && (!d.needs_acknowledgement || d.acknowledged),
    })
}

/// The options for `form`, or nothing when a pop-up shows no item.
pub fn export_options_intent(document: DocumentId, vm: &AppViewModel, form: &ExportForm) -> Option<UiIntent> {
    let d = vm.document(document)?.files.export.as_ref()?;
    let options = ExportOptionsInput {
        selection_only: form.selection_only,
        unit: *unit_items(d.format).get(index(form.unit)?)?,
        layers: *layer_items(d.format).get(index(form.layers)?)?,
        use_colors: form.use_colors,
        stroke_mm: form.stroke.clone(),
        decimals: form.decimals.clone(),
        curves: CURVE_ITEMS.get(index(form.curves)?).copied().unwrap_or_default(),
        tolerance_mm: form.tolerance.clone(),
    };
    (options != d.options)
        .then(|| UiIntent::Files(FilesIntent::SetExportOptions { document, options: Box::new(options) }))
}

pub fn export_acknowledge_intent(document: DocumentId, acknowledged: bool) -> UiIntent {
    UiIntent::Files(FilesIntent::SetExportAcknowledged { document, acknowledged })
}

pub fn export_write_intent(document: DocumentId, path: PathBuf) -> UiIntent {
    UiIntent::Files(FilesIntent::ExportDrawing { document, path })
}

pub fn export_close_intent(document: DocumentId) -> UiIntent {
    UiIntent::Files(FilesIntent::CloseExport { document })
}

/// Suggested file name for an export of the window's document.
pub fn export_file_name(title: &str, format: ExportFormat) -> String {
    let stem = title.strip_suffix(".burnburn").unwrap_or(title);
    format!("{stem}.{}", if format == ExportFormat::Svg { "svg" } else { "dxf" })
}

// ---------------------------------------------------------------------------------------------
// File panels: templates and what completes them.
// ---------------------------------------------------------------------------------------------

/// Items of the "content" pop-up of the Save As Template panel, top to bottom.
pub const TEMPLATE_SCOPES: [TemplateScope; 2] = [TemplateScope::Everything, TemplateScope::SettingsOnly];

pub fn template_scope_titles(locale: Locale) -> Vec<String> {
    vec![t(T::TemplateEverything, locale).to_owned(), t(T::TemplateSettingsOnly, locale).to_owned()]
}

pub fn template_save_intent(document: DocumentId, path: PathBuf, scope_item: isize) -> Option<UiIntent> {
    let scope = *index(scope_item).and_then(|i| TEMPLATE_SCOPES.get(i))?;
    Some(UiIntent::Files(FilesIntent::SaveAsTemplate { document, path, scope }))
}

pub fn template_open_intent(path: PathBuf) -> UiIntent {
    UiIntent::Files(FilesIntent::NewFromTemplate { path })
}

/// What choosing recent template `n` of `recent` does.
pub fn recent_template_intent(n: usize, recent: &[PathBuf]) -> Option<UiIntent> {
    recent.get(n).cloned().map(template_open_intent)
}
