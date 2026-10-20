//! Bundles (W2-L6, File ▸ Bundles ▸ Export/Import Bundle, P07): selected resources in one file
//! for transfer between computers — a material library (a chosen subset), an art library (a
//! chosen subset), machine profiles and templates.
//!
//! A bundle is a zip: `bundle.json` (format, schema, one line per entry with kind, size and
//! content hash), `materials.json`, `art.burnburnart`, `profiles.json` and
//! `templates/<name>.burnburntemplate`. Every entry must be listed and every listed entry
//! present and matching its hash; nested files are validated by their own readers.
//!
//! Importing is two steps. [`preview_import`] shows, per item, whether it is new, identical or in
//! conflict with what is local — for machine profiles with the names of the differing fields
//! ([`crate::compare_profile`]). [`apply_import`] then changes only the libraries it is given, by
//! an explicit [`ConflictChoice`]; profiles and templates live outside this crate and are
//! **returned**, never applied: a profile that differs locally is only handed back for
//! replacement when the user named it in [`ImportChoices::replace_profiles`].

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read, Write};

use burnburn_job::MachineProfile;
use burnburn_types::ContentHash;
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

use crate::art_library::{ArtItem, ArtLibrary, decode_art_library, encode_art_library};
use crate::asset_hash;
use crate::container::decode_container;
use crate::error::{LoadError, SaveError};
use crate::material_library::{LibraryQuery, MaterialLibrary, decode_material_library, encode_material_library};
use crate::profile::{ProfileComparison, ProfileDto, compare_profile, profile_from_dto, profile_to_dto};

pub const BUNDLE_SCHEMA: u32 = 1;
pub const BUNDLE_EXTENSION: &str = "burnburnbundle";
const FORMAT: &str = "burnburn-bundle";
const MANIFEST: &str = "bundle.json";
const MATERIALS: &str = "materials.json";
const ART: &str = "art.burnburnart";
const PROFILES: &str = "profiles.json";
const TEMPLATE_DIR: &str = "templates/";
const TEMPLATE_EXT: &str = ".burnburntemplate";
const MAX_ENTRIES: usize = 4096;
const MAX_JSON_BYTES: u64 = 64 << 20;
const MAX_BLOB_BYTES: u64 = 1 << 30;
const MAX_TOTAL_BYTES: u64 = 2 << 30;
const MAX_TEMPLATES: usize = 256;
/// Suffix of an incoming item kept next to a conflicting local one.
const KEEP_BOTH_SUFFIX: &str = " (bundel)";

/// A project template travelling in a bundle: its file stem and the container bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleTemplate {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// What to put in a bundle. Libraries are passed already reduced to the user's selection
/// ([`MaterialLibrary::subset`], [`ArtLibrary::subset`]).
#[derive(Debug, Clone, Copy, Default)]
pub struct BundleContents<'a> {
    pub materials: Option<&'a MaterialLibrary>,
    pub art: Option<&'a ArtLibrary>,
    pub profiles: &'a [MachineProfile],
    pub templates: &'a [BundleTemplate],
}

/// A decoded, validated bundle.
#[derive(Debug, Clone, PartialEq)]
pub struct Bundle {
    pub materials: Option<MaterialLibrary>,
    pub art: Option<ArtLibrary>,
    pub profiles: Vec<MachineProfile>,
    pub templates: Vec<BundleTemplate>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestDto {
    format: String,
    schema: u32,
    app: String,
    entries: Vec<EntryDto>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryDto {
    path: String,
    kind: String,
    bytes: u64,
    hash: String,
}

fn options() -> SimpleFileOptions {
    // Fixed timestamp: identical bundles give identical bytes.
    SimpleFileOptions::default().compression_method(CompressionMethod::Stored).last_modified_time(DateTime::default())
}

/// A template name that is safe as a file stem inside the bundle.
fn valid_template_name(n: &str) -> bool {
    let t = n.trim();
    !t.is_empty()
        && t == n
        && n.chars().count() <= 100
        && !n.starts_with('.')
        && !n.contains("..")
        && !n.chars().any(|c| c.is_control() || matches!(c, '/' | '\\'))
}

fn template_path(name: &str) -> String {
    format!("{TEMPLATE_DIR}{name}{TEMPLATE_EXT}")
}

/// Bundle → zip bytes (deterministic).
pub fn encode_bundle(contents: &BundleContents<'_>) -> Result<Vec<u8>, SaveError> {
    let enc = |e: &dyn std::fmt::Display| SaveError::Encode(e.to_string());
    let mut files: Vec<(String, &'static str, Vec<u8>)> = Vec::new();
    if let Some(m) = contents.materials {
        files.push((MATERIALS.into(), "materials", encode_material_library(m)?));
    }
    if let Some(a) = contents.art {
        files.push((ART.into(), "art", encode_art_library(a)?));
    }
    if !contents.profiles.is_empty() {
        let dtos: Vec<ProfileDto> = contents.profiles.iter().map(profile_to_dto).collect();
        files.push((PROFILES.into(), "profiles", serde_json::to_vec_pretty(&dtos).map_err(|e| enc(&e))?));
    }
    if contents.templates.len() > MAX_TEMPLATES {
        return Err(SaveError::Invalid(format!("at most {MAX_TEMPLATES} templates per bundle")));
    }
    let mut names = BTreeSet::new();
    for t in contents.templates {
        if !valid_template_name(&t.name) || !names.insert(t.name.clone()) {
            return Err(SaveError::Invalid(format!("template name {:?} is not usable", t.name)));
        }
        decode_container(&t.bytes).map_err(|e| SaveError::Invalid(format!("template {:?}: {e}", t.name)))?;
        files.push((template_path(&t.name), "template", t.bytes.clone()));
    }
    let manifest = ManifestDto {
        format: FORMAT.into(),
        schema: BUNDLE_SCHEMA,
        app: env!("CARGO_PKG_VERSION").into(),
        entries: files
            .iter()
            .map(|(path, kind, data)| EntryDto {
                path: path.clone(),
                kind: (*kind).into(),
                bytes: data.len() as u64,
                hash: asset_hash(data).to_hex(),
            })
            .collect(),
    };
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file(MANIFEST, options()).map_err(|e| enc(&e))?;
    zip.write_all(&serde_json::to_vec_pretty(&manifest).map_err(|e| enc(&e))?).map_err(|e| enc(&e))?;
    for (path, _, data) in &files {
        zip.start_file(path, options()).map_err(|e| enc(&e))?;
        zip.write_all(data).map_err(|e| enc(&e))?;
    }
    Ok(zip.finish().map_err(|e| enc(&e))?.into_inner())
}

fn nested(entry: &str, e: LoadError) -> LoadError {
    LoadError::InvalidValue { path: entry.to_owned(), reason: e.to_string() }
}

fn json_error(e: &serde_json::Error) -> LoadError {
    let detail = e.to_string();
    if detail.starts_with("unknown field") || detail.starts_with("unknown variant") {
        LoadError::UnknownField { entry: MANIFEST, detail }
    } else {
        LoadError::Json { entry: MANIFEST, detail }
    }
}

/// Zip bytes → bundle, fully validated.
pub fn decode_bundle(bytes: &[u8]) -> Result<Bundle, LoadError> {
    let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(|e| LoadError::Container(e.to_string()))?;
    if zip.len() > MAX_ENTRIES {
        return Err(LoadError::Limit("entry count"));
    }
    let mut manifest_bytes = None;
    let mut blobs: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut total = 0u64;
    for k in 0..zip.len() {
        let mut f = zip.by_index(k).map_err(|e| LoadError::Container(e.to_string()))?;
        let name = f.name().map_err(|e| LoadError::Container(e.to_string()))?.into_owned();
        let allowed = name == MANIFEST
            || name == MATERIALS
            || name == ART
            || name == PROFILES
            || name
                .strip_prefix(TEMPLATE_DIR)
                .and_then(|r| r.strip_suffix(TEMPLATE_EXT))
                .is_some_and(valid_template_name);
        if !allowed {
            return Err(LoadError::UnexpectedEntry(name));
        }
        let limit = if name == ART || name.starts_with(TEMPLATE_DIR) { MAX_BLOB_BYTES } else { MAX_JSON_BYTES };
        let mut data = Vec::new();
        (&mut f).take(limit + 1).read_to_end(&mut data).map_err(|e| LoadError::Container(e.to_string()))?;
        if data.len() as u64 > limit {
            return Err(LoadError::Limit("entry size"));
        }
        total += data.len() as u64;
        if total > MAX_TOTAL_BYTES {
            return Err(LoadError::Limit("total size"));
        }
        if name == MANIFEST {
            if manifest_bytes.replace(data).is_some() {
                return Err(LoadError::UnexpectedEntry(name));
            }
        } else if blobs.insert(name.clone(), data).is_some() {
            return Err(LoadError::UnexpectedEntry(name));
        }
    }
    let manifest_bytes = manifest_bytes.ok_or(LoadError::MissingEntry(MANIFEST))?;
    let loose: serde_json::Value = serde_json::from_slice(&manifest_bytes).map_err(|e| json_error(&e))?;
    if loose.get("format").and_then(serde_json::Value::as_str) != Some(FORMAT) {
        return Err(LoadError::NotAProject);
    }
    let schema = loose.get("schema").and_then(serde_json::Value::as_u64).ok_or(LoadError::NotAProject)?;
    if schema > u64::from(BUNDLE_SCHEMA) {
        return Err(LoadError::NewerSchema {
            found: u32::try_from(schema).unwrap_or(u32::MAX),
            supported: BUNDLE_SCHEMA,
        });
    }
    if schema < 1 {
        return Err(LoadError::InvalidValue { path: "schema".into(), reason: format!("unknown schema {schema}") });
    }
    let manifest: ManifestDto = serde_json::from_value(loose).map_err(|e| json_error(&e))?;

    let mut bundle = Bundle { materials: None, art: None, profiles: Vec::new(), templates: Vec::new() };
    let mut listed = BTreeSet::new();
    for (k, e) in manifest.entries.iter().enumerate() {
        let at = |f: &str| format!("entries[{k}].{f}");
        let bad = |f: &str, reason: String| LoadError::InvalidValue { path: at(f), reason };
        if !listed.insert(e.path.clone()) {
            return Err(bad("path", format!("{:?} is listed twice", e.path)));
        }
        let data = blobs
            .remove(&e.path)
            .ok_or_else(|| bad("path", format!("{:?} is listed but missing from the file", e.path)))?;
        let hash = ContentHash::from_hex(&e.hash).map_err(|x| bad("hash", x.to_string()))?;
        if asset_hash(&data) != hash || data.len() as u64 != e.bytes {
            return Err(LoadError::AssetHashMismatch(hash));
        }
        match (e.kind.as_str(), e.path.as_str()) {
            ("materials", MATERIALS) => {
                bundle.materials = Some(decode_material_library(&data).map_err(|x| nested(MATERIALS, x))?);
            }
            ("art", ART) => bundle.art = Some(decode_art_library(&data).map_err(|x| nested(ART, x))?),
            ("profiles", PROFILES) => {
                let dtos: Vec<ProfileDto> = serde_json::from_slice(&data).map_err(|x| json_error(&x))?;
                for (i, d) in dtos.iter().enumerate() {
                    bundle.profiles.push(profile_from_dto(i, d).map_err(|x| nested(PROFILES, x))?);
                }
            }
            ("template", path) if path.starts_with(TEMPLATE_DIR) => {
                let name = path.trim_start_matches(TEMPLATE_DIR).trim_end_matches(TEMPLATE_EXT).to_owned();
                decode_container(&data).map_err(|x| nested(path, x))?;
                bundle.templates.push(BundleTemplate { name, bytes: data });
            }
            (kind, path) => return Err(bad("kind", format!("kind {kind:?} does not fit entry {path:?}"))),
        }
    }
    if let Some(unlisted) = blobs.into_keys().next() {
        return Err(LoadError::UnexpectedEntry(unlisted));
    }
    Ok(bundle)
}

/// What the user already has; the libraries and, read-only, profiles and templates.
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalState<'a> {
    pub materials: Option<&'a MaterialLibrary>,
    pub art: Option<&'a ArtLibrary>,
    pub profiles: &'a [MachineProfile],
    pub templates: &'a [BundleTemplate],
}

/// How an incoming item relates to the local one with the same key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Nothing local has this key.
    New,
    /// The same key with the same content.
    Identical,
    /// The same key with other content.
    Conflict,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MaterialPreview {
    pub bundle_entry: u64,
    pub name: String,
    pub material: String,
    pub thickness_mm: f64,
    pub machine: String,
    pub change: Change,
    pub local_entry: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtPreview {
    pub bundle_item: u64,
    pub name: String,
    pub group: String,
    pub change: Change,
    pub local_item: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplatePreview {
    pub name: String,
    pub change: Change,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileChange {
    /// No local profile with this machine id.
    New,
    Identical,
    /// Same machine id, other content; these fields differ.
    Differs {
        fields: Vec<&'static str>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfilePreview {
    pub id: String,
    pub bundle_revision: u32,
    pub local_revision: Option<u32>,
    pub change: ProfileChange,
}

/// Everything an import would do, before it does anything.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImportPreview {
    pub materials: Vec<MaterialPreview>,
    pub art: Vec<ArtPreview>,
    pub profiles: Vec<ProfilePreview>,
    pub templates: Vec<TemplatePreview>,
}

/// The local material entry with the same key: material, machine, thickness (±0.05 mm) and name.
fn local_material<'a>(
    local: Option<&'a MaterialLibrary>,
    e: &crate::material_library::MaterialEntry,
) -> Option<&'a crate::material_library::MaterialEntry> {
    let d = e.draft();
    local?
        .find(&LibraryQuery {
            material: Some(&d.material),
            thickness_mm: Some(d.thickness.mm()),
            machine: Some(&d.machine),
        })
        .into_iter()
        .find(|l| l.draft().name == d.name)
}

fn local_art<'a>(local: Option<&'a ArtLibrary>, item: &ArtItem) -> Option<&'a ArtItem> {
    local?.items().iter().find(|l| l.group() == item.group() && l.name() == item.name())
}

fn local_profile<'a>(local: &'a [MachineProfile], p: &MachineProfile) -> Option<&'a MachineProfile> {
    local.iter().find(|l| l.revision().id == p.revision().id)
}

pub fn preview_import(bundle: &Bundle, local: &LocalState<'_>) -> ImportPreview {
    let mut preview = ImportPreview::default();
    for e in bundle.materials.iter().flat_map(|m| m.entries()) {
        let found = local_material(local.materials, e);
        let change = match found {
            None => Change::New,
            Some(l) if l.draft() == e.draft() => Change::Identical,
            Some(_) => Change::Conflict,
        };
        preview.materials.push(MaterialPreview {
            bundle_entry: e.id(),
            name: e.draft().name.clone(),
            material: e.draft().material.clone(),
            thickness_mm: e.draft().thickness.mm(),
            machine: e.draft().machine.clone(),
            change,
            local_entry: found.map(|l| l.id()),
        });
    }
    for item in bundle.art.iter().flat_map(|a| a.items()) {
        let found = local_art(local.art, item);
        let change = match found {
            None => Change::New,
            Some(l) if l.hash() == item.hash() => Change::Identical,
            Some(_) => Change::Conflict,
        };
        preview.art.push(ArtPreview {
            bundle_item: item.id(),
            name: item.name().to_owned(),
            group: item.group().to_owned(),
            change,
            local_item: found.map(ArtItem::id),
        });
    }
    for p in &bundle.profiles {
        let found = local_profile(local.profiles, p);
        let change = match found.map(|l| compare_profile(p, l)) {
            None | Some(ProfileComparison::OtherMachine) => ProfileChange::New,
            Some(ProfileComparison::Identical) => ProfileChange::Identical,
            Some(ProfileComparison::Differs { fields }) => ProfileChange::Differs { fields },
        };
        preview.profiles.push(ProfilePreview {
            id: p.revision().id.clone(),
            bundle_revision: p.revision().revision,
            local_revision: found.map(|l| l.revision().revision),
            change,
        });
    }
    for t in &bundle.templates {
        let change = match local.templates.iter().find(|l| l.name == t.name) {
            None => Change::New,
            Some(l) if l.bytes == t.bytes => Change::Identical,
            Some(_) => Change::Conflict,
        };
        preview.templates.push(TemplatePreview { name: t.name.clone(), change });
    }
    preview
}

/// What to do with an incoming item that conflicts with a local one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConflictChoice {
    /// Keep the local item (the default: nothing is overwritten unasked).
    #[default]
    Skip,
    /// The local item takes the incoming content.
    Replace,
    /// Keep both; the incoming one gets the suffix " (bundel)".
    KeepBoth,
}

#[derive(Debug, Clone, Default)]
pub struct ImportChoices {
    pub conflicts: ConflictChoice,
    /// Machine ids whose local profile may be replaced by the bundle's. Empty = none.
    pub replace_profiles: BTreeSet<String>,
}

/// What an import changed in the libraries and what it hands back to the caller.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImportOutcome {
    pub materials_added: Vec<u64>,
    pub materials_replaced: Vec<u64>,
    pub art_added: Vec<u64>,
    /// Ids of the local items that were replaced (the new item has a new id, listed in `art_added`).
    pub art_replaced: Vec<u64>,
    /// Templates to write into the user's template folder (the caller owns that store).
    pub templates_to_write: Vec<BundleTemplate>,
    /// Profiles without a local counterpart.
    pub profiles_to_add: Vec<MachineProfile>,
    /// Differing profiles the user named for replacement.
    pub profiles_to_replace: Vec<MachineProfile>,
    /// Machine ids whose profile differs and was *not* named: left alone.
    pub profiles_pending: Vec<String>,
    /// Incoming items left out (identical, or in conflict under `Skip`).
    pub skipped: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyError {
    Failed(String),
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ApplyError::Failed(m) = self;
        write!(f, "import failed: {m}")
    }
}

impl std::error::Error for ApplyError {}

/// What an import is applied to: the local libraries (changed in place) and, read-only, the local
/// machine profiles and templates (their stores are outside this crate).
pub struct ImportTarget<'a> {
    pub materials: &'a mut MaterialLibrary,
    pub art: &'a mut ArtLibrary,
    pub profiles: &'a [MachineProfile],
    pub templates: &'a [BundleTemplate],
}

fn suffixed(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let mut candidate = format!("{name}{KEEP_BOTH_SUFFIX}");
    let mut n = 2;
    while taken(&candidate) {
        candidate = format!("{name}{KEEP_BOTH_SUFFIX} {n}");
        n += 1;
    }
    candidate
}

/// Applies the bundle: libraries are changed by `choices`, everything outside them is returned.
/// Call [`preview_import`] first and show it.
pub fn apply_import(
    bundle: &Bundle,
    target: ImportTarget<'_>,
    choices: &ImportChoices,
) -> Result<ImportOutcome, ApplyError> {
    let fail = |e: &dyn std::fmt::Display| ApplyError::Failed(e.to_string());
    let mut out = ImportOutcome::default();
    for e in bundle.materials.iter().flat_map(|m| m.entries()) {
        let found = local_material(Some(&*target.materials), e).map(|l| (l.id(), l.draft() == e.draft()));
        match found {
            None => out.materials_added.push(target.materials.add(e.draft().clone()).map_err(|x| fail(&x))?),
            Some((_, true)) => out.skipped += 1,
            Some((local_id, false)) => match choices.conflicts {
                ConflictChoice::Skip => out.skipped += 1,
                ConflictChoice::Replace => {
                    target.materials.update(local_id, e.draft().clone()).map_err(|x| fail(&x))?;
                    out.materials_replaced.push(local_id);
                }
                ConflictChoice::KeepBoth => {
                    let mut draft = e.draft().clone();
                    draft.name =
                        suffixed(&draft.name, |n| target.materials.entries().iter().any(|l| l.draft().name == n));
                    out.materials_added.push(target.materials.add(draft).map_err(|x| fail(&x))?);
                }
            },
        }
    }
    for item in bundle.art.iter().flat_map(|a| a.items()) {
        let found = local_art(Some(&*target.art), item).map(|l| (l.id(), l.hash() == item.hash()));
        match found {
            None => out.art_added.push(target.art.add_copy(item, item.name()).map_err(|x| fail(&x))?),
            Some((_, true)) => out.skipped += 1,
            Some((local_id, false)) => match choices.conflicts {
                ConflictChoice::Skip => out.skipped += 1,
                ConflictChoice::Replace => {
                    target.art.remove_item(local_id);
                    out.art_added.push(target.art.add_copy(item, item.name()).map_err(|x| fail(&x))?);
                    out.art_replaced.push(local_id);
                }
                ConflictChoice::KeepBoth => {
                    let name = suffixed(item.name(), |n| {
                        target.art.items().iter().any(|l| l.group() == item.group() && l.name() == n)
                    });
                    out.art_added.push(target.art.add_copy(item, &name).map_err(|x| fail(&x))?);
                }
            },
        }
    }
    for t in &bundle.templates {
        match target.templates.iter().find(|l| l.name == t.name) {
            None => out.templates_to_write.push(t.clone()),
            Some(l) if l.bytes == t.bytes => out.skipped += 1,
            Some(_) => match choices.conflicts {
                ConflictChoice::Skip => out.skipped += 1,
                ConflictChoice::Replace => out.templates_to_write.push(t.clone()),
                ConflictChoice::KeepBoth => {
                    let name = suffixed(&t.name, |n| target.templates.iter().any(|l| l.name == n));
                    out.templates_to_write.push(BundleTemplate { name, bytes: t.bytes.clone() });
                }
            },
        }
    }
    for p in &bundle.profiles {
        let id = p.revision().id.clone();
        match local_profile(target.profiles, p).map(|l| compare_profile(p, l)) {
            None | Some(ProfileComparison::OtherMachine) => out.profiles_to_add.push(p.clone()),
            Some(ProfileComparison::Identical) => out.skipped += 1,
            Some(ProfileComparison::Differs { .. }) => {
                if choices.replace_profiles.contains(&id) {
                    out.profiles_to_replace.push(p.clone());
                } else {
                    out.profiles_pending.push(id);
                }
            }
        }
    }
    Ok(out)
}
