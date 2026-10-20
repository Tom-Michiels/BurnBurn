//! The Material Library panel (M217): search, apply to layers, links and updates (C08).
//!
//! Applying copies; it never links in a way that makes a layer follow the library. The panel
//! shows which layers were copied from a recipe and which recipes came from a library entry, and
//! a library update is only ever *offered*.

use burnburn_document::CommandPayload;
use burnburn_document::process::{OperationMode, ProcessCommand, RecipeDraft, RecipeId};
use burnburn_io::LibraryLink;
use burnburn_io::material_library::{
    LibraryError, LibraryQuery, LinkStatus, MaterialLibrary, link_status, refresh_command,
};
use burnburn_types::{DocumentId, OperationId, RequestId, StableId};

use super::{AppController, AppUpdate, Libraries, LoadState};
use crate::controller::Session;
use crate::controller::import::{apply_with_outcome, merge};
use crate::controller::layers::kind_of;
use crate::controller::machine::Machine;
use crate::field::{format_mm, parse_length};
use crate::files::{
    FilesIntent, FilesMessage, MaterialFilter, MaterialLinkStatus, MaterialOperationRow, MaterialOrigin,
    MaterialPanelViewModel, MaterialRow,
};
use crate::view_model::MessageKey;

/// Window state of the panel.
#[derive(Debug, Default)]
pub(in crate::controller) struct MaterialUi {
    filter: MaterialFilter,
    selected: Option<u64>,
    issue: Option<MessageKey>,
}

fn issue(message: FilesMessage) -> Option<MessageKey> {
    Some(MessageKey::Files(message))
}

/// The thickness filter as a number: empty is "any", anything else must be a length.
fn thickness_of(text: &str) -> Result<Option<f64>, ()> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    parse_length(text).map(|l| Some(l.mm())).map_err(|_| ())
}

impl AppController {
    pub(super) fn material_intent(&mut self, intent: FilesIntent) -> AppUpdate {
        match intent {
            FilesIntent::OpenMaterialLibrary { document } => {
                self.ensure_libraries();
                self.with_session(document, |s| {
                    s.files.material.get_or_insert_with(MaterialUi::default);
                    AppUpdate { view_changed: true, ..AppUpdate::default() }
                })
            }
            FilesIntent::CloseMaterialLibrary { document } => self.with_session(document, |s| {
                s.files.material = None;
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }),
            FilesIntent::SetMaterialFilter { document, material, thickness, machine } => {
                let parsed = thickness_of(&thickness);
                self.with_material_ui(document, |ui| match parsed {
                    Ok(_) => {
                        ui.filter = MaterialFilter { material, thickness_text: thickness, machine };
                        ui.issue = None;
                    }
                    Err(()) => ui.issue = issue(FilesMessage::MaterialInvalid),
                })
            }
            FilesIntent::SelectMaterialEntry { document, entry } => self.with_material_ui(document, |ui| {
                ui.selected = entry;
                ui.issue = None;
            }),
            FilesIntent::ApplyMaterial { document, entry, operations } => {
                self.apply_material(document, entry, operations)
            }
            FilesIntent::UpdateLinkedRecipe { document, recipe, reapply } => {
                self.update_linked_recipe(document, recipe, reapply)
            }
            FilesIntent::SaveMaterial { document, operation, name, material, thickness, notes } => {
                self.save_material(document, operation, name, material, &thickness, notes)
            }
            FilesIntent::UpdateMaterialFromOperation { document, entry, operation } => {
                self.update_material_from_operation(document, entry, operation)
            }
            FilesIntent::RemoveMaterial { document, entry } => {
                let removed = self.libs.materials.as_mut().is_some_and(|l| l.remove(entry));
                let mut update = self.with_material_ui(document, |ui| {
                    if ui.selected == Some(entry) {
                        ui.selected = None;
                    }
                });
                if removed {
                    update.effects.extend(self.flush_materials());
                }
                update
            }
            _ => AppUpdate::default(),
        }
    }

    fn with_material_ui(&mut self, document: DocumentId, f: impl FnOnce(&mut MaterialUi)) -> AppUpdate {
        self.with_session(document, |s| match s.files.material.as_mut() {
            Some(ui) => {
                f(ui);
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }
            None => AppUpdate::default(),
        })
    }

    fn material_issue(&mut self, document: DocumentId, message: Option<FilesMessage>) -> AppUpdate {
        self.with_material_ui(document, |ui| ui.issue = message.map(MessageKey::Files))
    }

    /// The mode and settings of a layer, for storing in the library.
    fn operation_of(
        &self,
        document: DocumentId,
        operation: OperationId,
    ) -> Option<(OperationMode, burnburn_document::process::ProcessSettings)> {
        let snapshot = self.sessions.get(&document)?.doc.snapshot();
        let op = snapshot.process().operation(operation)?;
        Some((op.mode(), op.settings()))
    }

    fn save_material(
        &mut self,
        document: DocumentId,
        operation: OperationId,
        name: String,
        material: String,
        thickness: &str,
        notes: String,
    ) -> AppUpdate {
        let Some((mode, settings)) = self.operation_of(document, operation) else {
            return self.material_issue(document, Some(FilesMessage::MaterialUnknown));
        };
        let Ok(thickness) = parse_length(thickness) else {
            return self.material_issue(document, Some(FilesMessage::MaterialInvalid));
        };
        let machine = self.machine.as_ref().map(|m| m.profile.revision().id.clone()).unwrap_or_default();
        let draft = RecipeDraft { name, material, thickness, machine, notes, mode, settings };
        let Some(library) = self.libs.materials.as_mut() else { return AppUpdate::default() };
        match library.add(draft) {
            Ok(id) => {
                let mut update = self.with_material_ui(document, |ui| {
                    ui.selected = Some(id);
                    ui.issue = None;
                });
                update.effects.extend(self.flush_materials());
                update
            }
            Err(LibraryError::ImageModeNotAllowed) => {
                self.material_issue(document, Some(FilesMessage::MaterialImageLayer))
            }
            Err(_) => self.material_issue(document, Some(FilesMessage::MaterialInvalid)),
        }
    }

    fn update_material_from_operation(
        &mut self,
        document: DocumentId,
        entry: u64,
        operation: OperationId,
    ) -> AppUpdate {
        let Some((mode, settings)) = self.operation_of(document, operation) else {
            return self.material_issue(document, Some(FilesMessage::MaterialUnknown));
        };
        let Some(library) = self.libs.materials.as_mut() else { return AppUpdate::default() };
        let Some(current) = library.get(entry) else {
            return self.material_issue(document, Some(FilesMessage::MaterialUnknown));
        };
        let draft = RecipeDraft { mode, settings, ..current.draft().clone() };
        match library.update(entry, draft) {
            Ok(_) => {
                let mut update = self.material_issue(document, None);
                update.effects.extend(self.flush_materials());
                update
            }
            Err(LibraryError::ImageModeNotAllowed) => {
                self.material_issue(document, Some(FilesMessage::MaterialImageLayer))
            }
            Err(_) => self.material_issue(document, Some(FilesMessage::MaterialUnknown)),
        }
    }

    /// Copies the entry into the project (one recipe per entry revision, reused when it is there
    /// already) and applies that recipe to the layers, one history entry each.
    fn apply_material(&mut self, document: DocumentId, entry: u64, operations: Vec<OperationId>) -> AppUpdate {
        let Some(library) = self.libs.materials.as_ref() else { return AppUpdate::default() };
        let Some(found) = library.get(entry) else {
            return self.material_issue(document, Some(FilesMessage::MaterialUnknown));
        };
        let (library_id, revision, add) = (library.id().to_owned(), found.revision(), found.add_recipe_command());
        if operations.is_empty() {
            return AppUpdate::default();
        }
        let mut requests = (0..=operations.len()).map(|_| self.next_id::<RequestId>()).collect::<Vec<_>>().into_iter();
        let mut update = self.with_session(document, |s| {
            let snapshot = s.doc.snapshot();
            for op in &operations {
                match snapshot.process().operation(*op).map(|o| o.mode()) {
                    None => {
                        set_issue(s, FilesMessage::MaterialUnknown);
                        return AppUpdate { view_changed: true, ..AppUpdate::default() };
                    }
                    Some(OperationMode::Image(_)) => {
                        set_issue(s, FilesMessage::MaterialImageLayer);
                        return AppUpdate { view_changed: true, ..AppUpdate::default() };
                    }
                    Some(_) => {}
                }
            }
            let mut update = AppUpdate::default();
            let existing = s
                .library_links
                .iter()
                .find(|l| {
                    l.library == library_id
                        && l.entry == entry
                        && l.revision == revision
                        && snapshot.process().recipe(RecipeId::from_raw(l.recipe)).is_some()
                })
                .map(|l| RecipeId::from_raw(l.recipe));
            let recipe = match existing {
                Some(recipe) => recipe,
                None => {
                    let (u, outcome) =
                        apply_with_outcome(s, document, CommandPayload::Process(add), requests.next().flatten());
                    merge(&mut update, u);
                    let Some(recipe) = outcome.and_then(|o| o.changes.recipes.first().copied()) else {
                        return update;
                    };
                    s.library_links.push(LibraryLink { recipe: recipe.raw(), library: library_id, entry, revision });
                    recipe
                }
            };
            for operation in operations {
                let command = ProcessCommand::ApplyRecipe { recipe, operation };
                let (u, _) =
                    apply_with_outcome(s, document, CommandPayload::Process(command), requests.next().flatten());
                merge(&mut update, u);
            }
            if let Some(ui) = s.files.material.as_mut() {
                ui.issue = None;
            }
            update.view_changed = true;
            update
        });
        update.view_changed = true;
        update
    }

    /// Brings the entry's current content into the project recipe; `reapply` copies it on into the
    /// layers that were set from that recipe. Each step is its own history entry.
    fn update_linked_recipe(&mut self, document: DocumentId, recipe: u64, reapply: bool) -> AppUpdate {
        let recipe_id = RecipeId::from_raw(recipe);
        let Some(library) = self.libs.materials.as_ref() else { return AppUpdate::default() };
        let link =
            self.sessions.get(&document).and_then(|s| s.library_links.iter().find(|l| l.recipe == recipe).cloned());
        let refresh = link.as_ref().and_then(|l| refresh_command(l, library, recipe_id));
        let latest = link.as_ref().and_then(|l| library.get(l.entry)).map(|e| e.revision());
        let (Some(refresh), Some(latest)) = (refresh, latest) else {
            return self.material_issue(document, Some(FilesMessage::MaterialUnknown));
        };
        let mut requests = (0..64).map(|_| self.next_id::<RequestId>()).collect::<Vec<_>>().into_iter();
        let mut update = self.with_session(document, |s| {
            let mut update = AppUpdate::default();
            let (u, outcome) =
                apply_with_outcome(s, document, CommandPayload::Process(refresh), requests.next().flatten());
            merge(&mut update, u);
            if outcome.is_none() {
                return update;
            }
            if let Some(link) = s.library_links.iter_mut().find(|l| l.recipe == recipe) {
                link.revision = latest;
            }
            if reapply {
                let snapshot = s.doc.snapshot();
                let targets: Vec<OperationId> = snapshot
                    .process()
                    .operations()
                    .filter(|o| o.recipe().is_some_and(|r| r.recipe == recipe_id))
                    .map(|o| o.id())
                    .collect();
                for operation in targets {
                    let command = ProcessCommand::ApplyRecipe { recipe: recipe_id, operation };
                    let (u, _) =
                        apply_with_outcome(s, document, CommandPayload::Process(command), requests.next().flatten());
                    merge(&mut update, u);
                }
            }
            if let Some(ui) = s.files.material.as_mut() {
                ui.issue = None;
            }
            update
        });
        update.view_changed = true;
        update
    }
}

fn set_issue(s: &mut Session, message: FilesMessage) {
    if let Some(ui) = s.files.material.as_mut() {
        ui.issue = issue(message);
    }
}

fn link_view(status: LinkStatus) -> MaterialLinkStatus {
    match status {
        LinkStatus::UpToDate => MaterialLinkStatus::UpToDate,
        LinkStatus::UpdateAvailable { applied, latest } => MaterialLinkStatus::UpdateAvailable { applied, latest },
        LinkStatus::LibraryOlder { applied, latest } => MaterialLinkStatus::LibraryOlder { applied, latest },
        LinkStatus::EntryMissing => MaterialLinkStatus::EntryMissing,
        _ => MaterialLinkStatus::OtherLibrary,
    }
}

pub(super) fn panel_view(s: &Session, machine: Option<&Machine>, libs: &Libraries) -> Option<MaterialPanelViewModel> {
    let ui = s.files.material.as_ref()?;
    let library: Option<&MaterialLibrary> = libs.materials.as_ref();
    let loading = libs.state != LoadState::Ready || library.is_none();
    let wanted_thickness = thickness_of(&ui.filter.thickness_text).ok().flatten();
    let query = LibraryQuery {
        material: ui.filter.material.as_deref(),
        thickness_mm: wanted_thickness,
        machine: ui.filter.machine.as_deref(),
    };
    let entries: Vec<MaterialRow> = library
        .map(|l| {
            l.find(&query)
                .into_iter()
                .map(|e| {
                    let d = e.draft();
                    MaterialRow {
                        id: e.id(),
                        name: d.name.clone(),
                        material: d.material.clone(),
                        thickness_mm: d.thickness.mm(),
                        machine: d.machine.clone(),
                        revision: e.revision(),
                        mode: kind_of(d.mode),
                        speed: format_mm(d.settings.speed.get_mm_per_s()),
                        power: format_mm(d.settings.power.max().get()),
                        passes: d.settings.passes.get().to_string(),
                        notes: d.notes.clone(),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let mut thicknesses_mm: Vec<f64> = library
        .map(|l| {
            l.find(&LibraryQuery { material: ui.filter.material.as_deref(), ..LibraryQuery::default() })
                .iter()
                .map(|e| e.draft().thickness.mm())
                .collect()
        })
        .unwrap_or_default();
    thicknesses_mm.sort_by(f64::total_cmp);
    thicknesses_mm.dedup_by(|a, b| (*a - *b).abs() < 0.05);
    let mut machines: Vec<String> = library
        .map(|l| l.entries().iter().map(|e| e.draft().machine.clone()).filter(|m| !m.is_empty()).collect())
        .unwrap_or_default();
    machines.sort();
    machines.dedup();

    let snapshot = s.doc.snapshot();
    let process = snapshot.process();
    let operations = process
        .operations()
        .map(|op| {
            let origin = match op.recipe() {
                None => MaterialOrigin::Manual,
                Some(r) => {
                    let name = process.recipe(r.recipe).map(|m| m.name().to_owned()).unwrap_or_default();
                    match s.library_links.iter().find(|l| l.recipe == r.recipe.raw()) {
                        None => {
                            MaterialOrigin::Copied { recipe: r.recipe.raw(), recipe_name: name, revision: r.revision }
                        }
                        Some(link) => MaterialOrigin::Linked {
                            recipe: r.recipe.raw(),
                            recipe_name: name,
                            revision: r.revision,
                            entry: link.entry,
                            status: library
                                .map_or(MaterialLinkStatus::OtherLibrary, |l| link_view(link_status(link, l))),
                        },
                    }
                }
            };
            MaterialOperationRow {
                id: op.id(),
                name: op.name().to_owned(),
                applicable: !matches!(op.mode(), OperationMode::Image(_)),
                origin,
            }
        })
        .collect();

    let library_issue = if libs.materials_unreadable {
        issue(FilesMessage::LibraryUnreadable)
    } else if libs.material_writer.failed {
        issue(FilesMessage::LibrarySaveFailed)
    } else {
        None
    };
    Some(MaterialPanelViewModel {
        loading,
        persisted: libs.persisted(),
        issue: ui.issue.or(library_issue),
        materials: library.map(MaterialLibrary::materials).unwrap_or_default(),
        thicknesses_mm,
        machines,
        filter: ui.filter.clone(),
        entries,
        selected: ui.selected,
        operations,
        active_machine: machine.map(|m| m.profile.revision().id.clone()),
    })
}
