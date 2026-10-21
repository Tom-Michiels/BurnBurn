//! Zip container v1: `manifest.json`, `document.json`, `assets/sha256-<hex>`, optional `previews/…`.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};
use std::sync::Arc;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

use crate::dto::v1;
use crate::error::{LoadError, SaveError};
use crate::map::{asset_key, document_from_dto, document_to_dto, parse_asset_key};
use crate::project::{Asset, AssetStore, Project, ProjectUuid};

/// The schema version this build writes and fully understands. Schema 2 (IM13) adds the
/// process layer; schema 1 files are read and their placeholder operations migrated.
pub const SCHEMA_VERSION: u32 = 2;
const FORMAT: &str = "burnburn-project";
const MANIFEST: &str = "manifest.json";
const DOCUMENT: &str = "document.json";
/// Frozen machine profiles (IM28); only present when the project has some.
const PROFILES: &str = "profiles.json";
/// Frozen production revisions (IM28); only present when the project has some.
const REVISIONS: &str = "revisions.json";
/// Material-library provenance of recipes (W2-L6, C08); only present when the project has some.
const LIBRARY_LINKS: &str = "library-links.json";
const ASSET_DIR: &str = "assets/";
const PREVIEW_DIR: &str = "previews/";

/// Read limits (part of the container contract; fuzz targets rely on them).
pub(crate) const MAX_CONTAINER_BYTES: u64 = 2 << 30;
const MAX_ENTRIES: usize = 4096;
const MAX_JSON_BYTES: u64 = 64 << 20;
const MAX_ASSET_BYTES: u64 = 1 << 30;
const MAX_TOTAL_BYTES: u64 = 2 << 30;

fn options(method: CompressionMethod) -> SimpleFileOptions {
    // Fixed timestamp: identical projects give identical bytes.
    SimpleFileOptions::default().compression_method(method).last_modified_time(DateTime::default())
}

/// Encodes a project into container bytes. Entry order is fixed; output is deterministic.
pub fn encode_container(project: &Project) -> Result<Vec<u8>, SaveError> {
    let enc = |e: &dyn std::fmt::Display| SaveError::Encode(e.to_string());
    let manifest = v1::Manifest {
        format: FORMAT.into(),
        schema: SCHEMA_VERSION,
        app: env!("CARGO_PKG_VERSION").into(),
        project: project.uuid.to_string(),
        document: DOCUMENT.into(),
        assets: project
            .assets
            .iter()
            .map(|(h, a)| (asset_key(h), v1::AssetEntry { media: a.media.clone(), bytes: a.bytes.len() as u64 }))
            .collect(),
    };
    let document = document_to_dto(&project.document)?;
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let json = options(CompressionMethod::Deflated);
    zip.start_file(MANIFEST, json).map_err(|e| enc(&e))?;
    zip.write_all(&serde_json::to_vec_pretty(&manifest).map_err(|e| enc(&e))?).map_err(|e| enc(&e))?;
    zip.start_file(DOCUMENT, json).map_err(|e| enc(&e))?;
    zip.write_all(&serde_json::to_vec_pretty(&document).map_err(|e| enc(&e))?).map_err(|e| enc(&e))?;
    if !project.profiles.is_empty() {
        let dtos: Vec<_> = project.profiles.iter().map(crate::profile::profile_to_dto).collect();
        zip.start_file(PROFILES, json).map_err(|e| enc(&e))?;
        zip.write_all(&serde_json::to_vec_pretty(&dtos).map_err(|e| enc(&e))?).map_err(|e| enc(&e))?;
    }
    if !project.revisions.is_empty() {
        let mut dtos = Vec::with_capacity(project.revisions.len());
        for r in &project.revisions {
            dtos.push(RevisionDto {
                number: r.number,
                label: r.label.clone(),
                project_hash: r.project_hash.to_hex(),
                document: document_to_dto(&r.document)?,
            });
        }
        zip.start_file(REVISIONS, json).map_err(|e| enc(&e))?;
        zip.write_all(&serde_json::to_vec_pretty(&dtos).map_err(|e| enc(&e))?).map_err(|e| enc(&e))?;
    }
    if !project.library_links.is_empty() {
        let dtos: Vec<LinkDto> = project
            .library_links
            .iter()
            .map(|l| LinkDto { recipe: l.recipe, library: l.library.clone(), entry: l.entry, revision: l.revision })
            .collect();
        zip.start_file(LIBRARY_LINKS, json).map_err(|e| enc(&e))?;
        zip.write_all(&serde_json::to_vec_pretty(&dtos).map_err(|e| enc(&e))?).map_err(|e| enc(&e))?;
    }
    for (h, a) in project.assets.iter() {
        zip.start_file(format!("{ASSET_DIR}{}", asset_key(h)), options(CompressionMethod::Stored))
            .map_err(|e| enc(&e))?;
        zip.write_all(&a.bytes).map_err(|e| enc(&e))?;
    }
    Ok(zip.finish().map_err(|e| enc(&e))?.into_inner())
}

fn json_error(entry: &'static str, e: &serde_json::Error) -> LoadError {
    let detail = e.to_string();
    if detail.starts_with("unknown field") || detail.starts_with("unknown variant") {
        LoadError::UnknownField { entry, detail }
    } else {
        LoadError::Json { entry, detail }
    }
}

/// Decodes and fully validates container bytes.
pub fn decode_container(bytes: &[u8]) -> Result<Project, LoadError> {
    let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(|e| LoadError::Container(e.to_string()))?;
    if zip.len() > MAX_ENTRIES {
        return Err(LoadError::Limit("entry count"));
    }
    let mut manifest_bytes = None;
    let mut document_bytes = None;
    let mut profile_bytes = None;
    let mut revision_bytes = None;
    let mut link_bytes = None;
    let mut asset_bytes: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| LoadError::Container(e.to_string()))?;
        let name = f.name().map_err(|e| LoadError::Container(e.to_string()))?.into_owned();
        let (slot, limit): (Option<&mut Option<Vec<u8>>>, u64) = match name.as_str() {
            MANIFEST => (Some(&mut manifest_bytes), MAX_JSON_BYTES),
            DOCUMENT => (Some(&mut document_bytes), MAX_JSON_BYTES),
            PROFILES => (Some(&mut profile_bytes), MAX_JSON_BYTES),
            REVISIONS => (Some(&mut revision_bytes), MAX_JSON_BYTES),
            LIBRARY_LINKS => (Some(&mut link_bytes), MAX_JSON_BYTES),
            n if n.starts_with(ASSET_DIR) && !n[ASSET_DIR.len()..].contains('/') => (None, MAX_ASSET_BYTES),
            n if n.starts_with(PREVIEW_DIR) && !n.contains("..") => continue,
            _ => return Err(LoadError::UnexpectedEntry(name)),
        };
        let mut data = Vec::new();
        // Never trust the declared size: read at most limit + 1 bytes.
        (&mut f).take(limit + 1).read_to_end(&mut data).map_err(|e| LoadError::Container(e.to_string()))?;
        if data.len() as u64 > limit {
            return Err(LoadError::Limit("entry size"));
        }
        total += data.len() as u64;
        if total > MAX_TOTAL_BYTES {
            return Err(LoadError::Limit("total size"));
        }
        match slot {
            Some(s) if s.is_some() => return Err(LoadError::UnexpectedEntry(name)),
            Some(s) => *s = Some(data),
            None => {
                if asset_bytes.insert(name.clone(), data).is_some() {
                    return Err(LoadError::UnexpectedEntry(name));
                }
            }
        }
    }
    let manifest_bytes = manifest_bytes.ok_or(LoadError::MissingEntry(MANIFEST))?;

    // Version gate before the strict parse: a newer manifest may carry fields we do not know.
    let loose: serde_json::Value = serde_json::from_slice(&manifest_bytes).map_err(|e| json_error(MANIFEST, &e))?;
    if loose.get("format").and_then(|v| v.as_str()) != Some(FORMAT) {
        return Err(LoadError::NotAProject);
    }
    let schema = loose.get("schema").and_then(serde_json::Value::as_u64).ok_or(LoadError::NotAProject)?;
    if schema > u64::from(SCHEMA_VERSION) {
        return Err(LoadError::NewerSchema {
            found: u32::try_from(schema).unwrap_or(u32::MAX),
            supported: SCHEMA_VERSION,
        });
    }
    if schema < 1 {
        return Err(LoadError::InvalidValue {
            path: "manifest.schema".into(),
            reason: format!("unknown schema {schema}"),
        });
    }
    let manifest: v1::Manifest = serde_json::from_slice(&manifest_bytes).map_err(|e| json_error(MANIFEST, &e))?;
    if manifest.document != DOCUMENT {
        return Err(LoadError::InvalidValue { path: "manifest.document".into(), reason: manifest.document });
    }
    let uuid = ProjectUuid::parse(&manifest.project)
        .ok_or_else(|| LoadError::InvalidValue { path: "manifest.project".into(), reason: manifest.project.clone() })?;

    let mut assets = AssetStore::default();
    for (key, entry) in &manifest.assets {
        let hash = parse_asset_key(|| format!("manifest.assets.{key}"), key)?;
        let data = asset_bytes.remove(&format!("{ASSET_DIR}{key}")).ok_or(LoadError::MissingAsset(hash))?;
        if crate::asset_hash(&data) != hash || data.len() as u64 != entry.bytes {
            return Err(LoadError::AssetHashMismatch(hash));
        }
        assets.insert_verified(hash, Asset { media: entry.media.clone(), bytes: Arc::from(data) });
    }
    if let Some(stray) = asset_bytes.into_keys().next() {
        return Err(LoadError::UnexpectedEntry(stray));
    }

    let document_bytes = document_bytes.ok_or(LoadError::MissingEntry(DOCUMENT))?;
    let dto: v1::Document = serde_json::from_slice(&document_bytes).map_err(|e| json_error(DOCUMENT, &e))?;
    let schema = u32::try_from(schema).unwrap_or(u32::MAX);
    let document = document_from_dto(&dto, schema, &|h| assets.get(h).is_some())?;
    let mut project =
        Project { uuid, document, assets, profiles: Vec::new(), revisions: Vec::new(), library_links: Vec::new() };
    if let Some(bytes) = profile_bytes {
        let dtos: Vec<crate::profile::ProfileDto> =
            serde_json::from_slice(&bytes).map_err(|e| json_error(PROFILES, &e))?;
        for (i, d) in dtos.iter().enumerate() {
            project.profiles.push(crate::profile::profile_from_dto(i, d)?);
        }
    }
    if let Some(bytes) = link_bytes {
        let dtos: Vec<LinkDto> = serde_json::from_slice(&bytes).map_err(|e| json_error(LIBRARY_LINKS, &e))?;
        if dtos.is_empty() || dtos.len() > MAX_ENTRIES {
            return Err(LoadError::InvalidValue { path: "library-links".into(), reason: "empty or too long".into() });
        }
        for l in dtos {
            let known = project.document.process().recipes().any(|r| burnburn_types::StableId::raw(r.id()) == l.recipe);
            if !known {
                return Err(LoadError::InvalidValue {
                    path: "library-links".into(),
                    reason: format!("no recipe {} in this project", l.recipe),
                });
            }
            project.library_links.push(crate::LibraryLink {
                recipe: l.recipe,
                library: l.library,
                entry: l.entry,
                revision: l.revision,
            });
        }
    }
    if let Some(bytes) = revision_bytes {
        let dtos: Vec<RevisionDto> = serde_json::from_slice(&bytes).map_err(|e| json_error(REVISIONS, &e))?;
        for (i, d) in dtos.iter().enumerate() {
            let bad = |reason: String| LoadError::InvalidValue { path: format!("revisions[{i}]"), reason };
            if project.revisions.last().is_some_and(|r| r.number >= d.number) {
                return Err(bad(format!("revision number {} is not increasing", d.number)));
            }
            let document = document_from_dto(&d.document, schema, &|h| project.assets.get(h).is_some())?;
            let stored = burnburn_types::ContentHash::from_hex(&d.project_hash).map_err(|e| bad(e.to_string()))?;
            let actual = crate::project_hash(&project.frozen_project(document.clone()))
                .map_err(|_| bad("revision document is not finite".into()))?;
            if actual != stored {
                return Err(bad("frozen revision does not match its recorded hash".into()));
            }
            project.revisions.push(crate::FrozenRevision {
                number: d.number,
                label: d.label.clone(),
                document,
                project_hash: stored,
            });
        }
    }
    Ok(project)
}

/// One frozen revision on disk: a full document plus its recorded project hash.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RevisionDto {
    number: u32,
    label: String,
    project_hash: String,
    document: v1::Document,
}

/// One library link on disk.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LinkDto {
    recipe: u64,
    library: String,
    entry: u64,
    revision: u32,
}

/// The schema version in the manifest of container `bytes`, without loading the project.
///
/// Only `manifest.json` is read. A file that is not a BurnBurn container (or whose manifest does
/// not say so) is an error; a newer schema is *not* (that is what the caller wants to know).
pub fn container_schema(bytes: &[u8]) -> Result<u32, LoadError> {
    let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(|e| LoadError::Container(e.to_string()))?;
    let entry = zip.by_name(MANIFEST).map_err(|_| LoadError::MissingEntry(MANIFEST))?;
    let mut data = Vec::new();
    entry.take(MAX_JSON_BYTES + 1).read_to_end(&mut data).map_err(|e| LoadError::Container(e.to_string()))?;
    if data.len() as u64 > MAX_JSON_BYTES {
        return Err(LoadError::Limit("manifest size"));
    }
    let loose: serde_json::Value = serde_json::from_slice(&data).map_err(|e| json_error(MANIFEST, &e))?;
    if loose.get("format").and_then(|v| v.as_str()) != Some(FORMAT) {
        return Err(LoadError::NotAProject);
    }
    let schema = loose.get("schema").and_then(serde_json::Value::as_u64).ok_or(LoadError::NotAProject)?;
    Ok(u32::try_from(schema).unwrap_or(u32::MAX))
}
