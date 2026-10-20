//! Material Library (W2-L6, C08): reusable settings per material and thickness.
//!
//! An entry is a [`RecipeDraft`] (name, material, thickness, machine, notes, mode, settings)
//! with a stable id and a revision. Entries are **copied** into a project as ordinary project
//! recipes (`ProcessCommand::AddRecipe`, then `ApplyRecipe` onto operations); the project keeps
//! its own snapshot. A [`LibraryLink`] remembers where a recipe came from so a later library
//! update can be *reported* ([`link_status`]) — never applied: `library_update_does_not_change_
//! existing_job_settings`. Bringing an update in takes explicit commands ([`refresh_command`],
//! then `ApplyRecipe` again).
//!
//! The file is JSON (`format`, `schema`, `id`, `next_id`, `entries`), written through the
//! [`FileSystem`] port (atomic). The schema is versioned: a newer file is refused with its
//! version, older ones go through a migration chain (none are needed at schema 1). Entries hold
//! no file paths, so moving the library or the projects breaks nothing; an image source belongs
//! to one project and is therefore not allowed in a library.

use std::collections::BTreeSet;
use std::path::Path;

use burnburn_document::process::{OperationMode, ProcessCommand, RecipeDraft, RecipeId};
use burnburn_types::{Length, StableId};
use serde::{Deserialize, Serialize};

use crate::dto::v1;
use crate::error::{LoadError, SaveError};
use crate::fs::FileSystem;
use crate::process_map::{mode_from_dto, mode_to_dto, settings_from_dto, settings_to_dto};
use crate::project::LibraryLink;
use crate::store::write_atomic;

pub const MATERIAL_LIBRARY_SCHEMA: u32 = 1;
const FORMAT: &str = "burnburn-material-library";
const MAX_ENTRIES: usize = 10_000;
const MAX_BYTES: u64 = 64 << 20;
/// Thickness values closer than this are the same sheet (mm).
const THICKNESS_TOLERANCE_MM: f64 = 0.05;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LibraryError {
    /// An image operation names a source image of one project.
    ImageModeNotAllowed,
    UnknownEntry(u64),
    /// Name or material empty, or thickness not positive.
    InvalidDraft(String),
    TooManyEntries,
}

impl std::fmt::Display for LibraryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LibraryError::ImageModeNotAllowed => f.write_str("an image operation cannot be stored in a library"),
            LibraryError::UnknownEntry(id) => write!(f, "no library entry {id}"),
            LibraryError::InvalidDraft(m) => write!(f, "invalid library entry: {m}"),
            LibraryError::TooManyEntries => write!(f, "a library holds at most {MAX_ENTRIES} entries"),
        }
    }
}

impl std::error::Error for LibraryError {}

#[derive(Debug, Clone, PartialEq)]
pub struct MaterialEntry {
    id: u64,
    revision: u32,
    draft: RecipeDraft,
}

impl MaterialEntry {
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Starts at 1 and increases with every update.
    pub fn revision(&self) -> u32 {
        self.revision
    }

    pub fn draft(&self) -> &RecipeDraft {
        &self.draft
    }

    /// The command that copies this entry into a project as a new recipe.
    pub fn add_recipe_command(&self) -> ProcessCommand {
        ProcessCommand::AddRecipe { draft: self.draft.clone() }
    }
}

/// What to look for; `None` fields match everything.
#[derive(Debug, Clone, Default)]
pub struct LibraryQuery<'a> {
    pub material: Option<&'a str>,
    /// Matches entries within 0.05 mm.
    pub thickness_mm: Option<f64>,
    pub machine: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MaterialLibrary {
    id: String,
    next_id: u64,
    entries: Vec<MaterialEntry>,
}

fn check_draft(d: &RecipeDraft) -> Result<(), LibraryError> {
    if d.name.trim().is_empty() {
        return Err(LibraryError::InvalidDraft("name is empty".into()));
    }
    if d.material.trim().is_empty() {
        return Err(LibraryError::InvalidDraft("material is empty".into()));
    }
    if !(d.thickness.mm() > 0.0) {
        return Err(LibraryError::InvalidDraft("thickness must be positive".into()));
    }
    if matches!(d.mode, OperationMode::Image(_)) {
        return Err(LibraryError::ImageModeNotAllowed);
    }
    Ok(())
}

impl MaterialLibrary {
    /// An empty library; `id` identifies it in project links (for example a generated uuid).
    pub fn new(id: &str) -> Self {
        MaterialLibrary { id: id.to_owned(), next_id: 1, entries: Vec::new() }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn entries(&self) -> &[MaterialEntry] {
        &self.entries
    }

    pub fn get(&self, id: u64) -> Option<&MaterialEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn add(&mut self, draft: RecipeDraft) -> Result<u64, LibraryError> {
        check_draft(&draft)?;
        if self.entries.len() >= MAX_ENTRIES {
            return Err(LibraryError::TooManyEntries);
        }
        let id = self.next_id;
        self.next_id += 1;
        self.entries.push(MaterialEntry { id, revision: 1, draft });
        Ok(id)
    }

    /// Replaces the entry's content; returns the new revision. Projects that copied it earlier
    /// are not touched.
    pub fn update(&mut self, id: u64, draft: RecipeDraft) -> Result<u32, LibraryError> {
        check_draft(&draft)?;
        let entry = self.entries.iter_mut().find(|e| e.id == id).ok_or(LibraryError::UnknownEntry(id))?;
        entry.revision = entry.revision.saturating_add(1);
        entry.draft = draft;
        Ok(entry.revision)
    }

    /// Removes an entry; its id is never reused.
    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        self.entries.len() != before
    }

    /// Entries matching `query`, ordered by material, thickness, machine, name, id.
    pub fn find(&self, query: &LibraryQuery<'_>) -> Vec<&MaterialEntry> {
        let mut found: Vec<&MaterialEntry> = self
            .entries
            .iter()
            .filter(|e| query.material.is_none_or(|m| e.draft.material == m))
            .filter(|e| query.machine.is_none_or(|m| e.draft.machine == m))
            .filter(|e| query.thickness_mm.is_none_or(|t| (e.draft.thickness.mm() - t).abs() <= THICKNESS_TOLERANCE_MM))
            .collect();
        found.sort_by(|a, b| {
            (&a.draft.material, a.draft.thickness.mm(), &a.draft.machine, &a.draft.name, a.id)
                .partial_cmp(&(&b.draft.material, b.draft.thickness.mm(), &b.draft.machine, &b.draft.name, b.id))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        found
    }

    /// Material names, sorted and unique.
    pub fn materials(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.draft.material.clone()).collect::<BTreeSet<_>>().into_iter().collect()
    }

    /// A library holding copies of the given entries (same ids and revisions), for bundles.
    pub fn subset(&self, ids: &[u64]) -> MaterialLibrary {
        MaterialLibrary {
            id: self.id.clone(),
            next_id: self.next_id,
            entries: self.entries.iter().filter(|e| ids.contains(&e.id)).cloned().collect(),
        }
    }
}

/// How a project recipe relates to the library entry it was copied from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LinkStatus {
    UpToDate,
    /// The entry has a newer revision than the one copied.
    UpdateAvailable {
        applied: u32,
        latest: u32,
    },
    /// The library holds an older revision than the one copied (restored from a backup?).
    LibraryOlder {
        applied: u32,
        latest: u32,
    },
    EntryMissing,
    /// The link was made against another library.
    OtherLibrary,
}

/// The link for a recipe just created from `entry` (`None` when the entry does not exist).
pub fn link_for(library: &MaterialLibrary, entry: u64, recipe: RecipeId) -> Option<LibraryLink> {
    let e = library.get(entry)?;
    Some(LibraryLink { recipe: recipe.raw(), library: library.id.clone(), entry, revision: e.revision })
}

pub fn link_status(link: &LibraryLink, library: &MaterialLibrary) -> LinkStatus {
    if link.library != library.id {
        return LinkStatus::OtherLibrary;
    }
    match library.get(link.entry) {
        None => LinkStatus::EntryMissing,
        Some(e) if e.revision == link.revision => LinkStatus::UpToDate,
        Some(e) if e.revision > link.revision => {
            LinkStatus::UpdateAvailable { applied: link.revision, latest: e.revision }
        }
        Some(e) => LinkStatus::LibraryOlder { applied: link.revision, latest: e.revision },
    }
}

/// The explicit command that brings the entry's current content into the project recipe. It
/// changes the recipe only; operations pick the values up with a further `ApplyRecipe`.
pub fn refresh_command(link: &LibraryLink, library: &MaterialLibrary, recipe: RecipeId) -> Option<ProcessCommand> {
    if link.library != library.id || link.recipe != recipe.raw() {
        return None;
    }
    let e = library.get(link.entry)?;
    Some(ProcessCommand::UpdateRecipe { recipe, draft: e.draft.clone() })
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileDto {
    format: String,
    schema: u32,
    id: String,
    next_id: u64,
    entries: Vec<v1::Recipe>,
}

/// Library → bytes (deterministic JSON).
pub fn encode_material_library(library: &MaterialLibrary) -> Result<Vec<u8>, SaveError> {
    let mut entries = Vec::with_capacity(library.entries.len());
    for e in &library.entries {
        entries.push(v1::Recipe {
            id: e.id,
            name: e.draft.name.clone(),
            material: e.draft.material.clone(),
            thickness_mm: v1::Num(e.draft.thickness.mm()),
            machine: e.draft.machine.clone(),
            revision: e.revision,
            notes: e.draft.notes.clone(),
            mode: mode_to_dto(e.draft.mode)?,
            settings: settings_to_dto(&e.draft.settings),
        });
    }
    let dto = FileDto {
        format: FORMAT.into(),
        schema: MATERIAL_LIBRARY_SCHEMA,
        id: library.id.clone(),
        next_id: library.next_id,
        entries,
    };
    serde_json::to_vec_pretty(&dto).map_err(|e| SaveError::Encode(e.to_string()))
}

type Migration = fn(serde_json::Value) -> Result<serde_json::Value, LoadError>;

/// Applies `steps[from - 1 ..]` in order (`steps[i]` migrates schema `i + 1` to `i + 2`), so a
/// file of any older schema reaches the current one. The real chain is empty at schema 1.
fn migrate_with(
    mut value: serde_json::Value,
    from: u32,
    to: u32,
    steps: &[Migration],
) -> Result<serde_json::Value, LoadError> {
    for schema in from..to {
        let step = steps.get(schema as usize - 1).ok_or_else(|| LoadError::InvalidValue {
            path: "schema".into(),
            reason: format!("no migration from schema {schema}"),
        })?;
        value = step(value)?;
    }
    Ok(value)
}

fn json_error(e: &serde_json::Error) -> LoadError {
    let detail = e.to_string();
    if detail.starts_with("unknown field") || detail.starts_with("unknown variant") {
        LoadError::UnknownField { entry: "material library", detail }
    } else {
        LoadError::Json { entry: "material library", detail }
    }
}

/// Bytes → library, fully validated.
pub fn decode_material_library(bytes: &[u8]) -> Result<MaterialLibrary, LoadError> {
    let loose: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| json_error(&e))?;
    if loose.get("format").and_then(serde_json::Value::as_str) != Some(FORMAT) {
        return Err(LoadError::NotAProject);
    }
    let schema = loose.get("schema").and_then(serde_json::Value::as_u64).ok_or(LoadError::NotAProject)?;
    if schema > u64::from(MATERIAL_LIBRARY_SCHEMA) {
        return Err(LoadError::NewerSchema {
            found: u32::try_from(schema).unwrap_or(u32::MAX),
            supported: MATERIAL_LIBRARY_SCHEMA,
        });
    }
    if schema < 1 {
        return Err(LoadError::InvalidValue { path: "schema".into(), reason: format!("unknown schema {schema}") });
    }
    let value = migrate_with(loose, schema as u32, MATERIAL_LIBRARY_SCHEMA, &[])?;
    let dto: FileDto = serde_json::from_value(value).map_err(|e| json_error(&e))?;
    if dto.entries.len() > MAX_ENTRIES {
        return Err(LoadError::Limit("library entries"));
    }
    let mut seen = BTreeSet::new();
    let mut entries = Vec::with_capacity(dto.entries.len());
    for (i, r) in dto.entries.iter().enumerate() {
        let bad =
            |field: &str, reason: String| LoadError::InvalidValue { path: format!("entries[{i}].{field}"), reason };
        if !seen.insert(r.id) {
            return Err(bad("id", format!("duplicate id {}", r.id)));
        }
        if r.id >= dto.next_id {
            return Err(LoadError::InvalidValue {
                path: "next_id".into(),
                reason: format!("must exceed every entry id (entry {} has {})", i, r.id),
            });
        }
        if r.revision == 0 {
            return Err(bad("revision", "revisions start at 1".into()));
        }
        let thickness_path = || format!("entries[{i}].thickness_mm");
        let thickness = Length::from_mm(r.thickness_mm.0)
            .map_err(|e| LoadError::InvalidValue { path: thickness_path(), reason: e.to_string() })?;
        let mode = mode_from_dto(&|| format!("entries[{i}].mode"), &r.mode)?;
        let settings = settings_from_dto(&|| format!("entries[{i}].settings"), &r.settings)?;
        let draft = RecipeDraft {
            name: r.name.clone(),
            material: r.material.clone(),
            thickness,
            machine: r.machine.clone(),
            notes: r.notes.clone(),
            mode,
            settings,
        };
        check_draft(&draft).map_err(|e| bad("entry", e.to_string()))?;
        entries.push(MaterialEntry { id: r.id, revision: r.revision, draft });
    }
    Ok(MaterialLibrary { id: dto.id, next_id: dto.next_id, entries })
}

/// Atomic save through the filesystem port.
pub fn save_material_library(fs: &dyn FileSystem, path: &Path, library: &MaterialLibrary) -> Result<(), SaveError> {
    let bytes = encode_material_library(library)?;
    decode_material_library(&bytes).map_err(|e| SaveError::Invalid(e.to_string()))?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs.create_dir_all(dir).map_err(SaveError::Fs)?;
    }
    write_atomic(fs, path, &bytes)
}

pub fn load_material_library(fs: &dyn FileSystem, path: &Path) -> Result<MaterialLibrary, LoadError> {
    let bytes = fs.read(path, MAX_BYTES).map_err(LoadError::Fs)?;
    decode_material_library(&bytes)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn bump(mut v: serde_json::Value) -> Result<serde_json::Value, LoadError> {
        v["steps"] = json!(v["steps"].as_u64().unwrap_or(0) + 1);
        Ok(v)
    }

    #[test]
    fn migration_chain_runs_each_step_once_in_order_and_refuses_gaps() {
        let steps: [Migration; 2] = [bump, bump];
        // Schema 1 → 3 uses both steps, 2 → 3 only the second, 3 → 3 none.
        assert_eq!(migrate_with(json!({}), 1, 3, &steps).unwrap()["steps"], 2);
        assert_eq!(migrate_with(json!({}), 2, 3, &steps).unwrap()["steps"], 1);
        assert_eq!(migrate_with(json!({}), 3, 3, &steps).unwrap()["steps"], serde_json::Value::Null);
        // A missing step is a typed error, never a silent skip.
        assert!(matches!(migrate_with(json!({}), 1, 4, &steps), Err(LoadError::InvalidValue { .. })));
    }
}
