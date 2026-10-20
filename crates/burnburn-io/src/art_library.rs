//! Art Library (W2-L6): reusable vector assets in named groups, with thumbnails.
//!
//! An item is the **original file** (SVG, DXF, HP-GL/PLT or PDF/AI) stored byte for byte with its
//! content hash; adding it proves it imports. Placing re-runs the normal import ([`ArtLibrary::place`]),
//! so a placed item behaves exactly like importing the file, and the caller stores the bytes as
//! the project asset. Thumbnails are derived on demand (a small SVG), never stored, so they can
//! not go stale. The library file is a zip (`library.json` + `items/<id>`) written atomically;
//! it contains no file paths, so moving the library or the original files breaks nothing.
//! Groups are a flat, ordered list of names.

use std::collections::BTreeSet;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::sync::Arc;

use burnburn_geometry::Bounds;
use burnburn_types::ContentHash;
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

use crate::asset_hash;
use crate::error::{LoadError, SaveError};
use crate::export::{ExportItem, SvgOptions, export_svg_with};
use crate::fs::FileSystem;
use crate::import::{DxfScale, ImportDraft, ImportError, PdfPages, parse_dxf, parse_hpgl, parse_pdf, parse_svg};
use crate::store::write_atomic;

pub const ART_LIBRARY_SCHEMA: u32 = 1;
const FORMAT: &str = "burnburn-art-library";
const MANIFEST: &str = "library.json";
const ITEM_DIR: &str = "items/";
const MAX_ITEMS: usize = 10_000;
const MAX_ITEM_BYTES: u64 = 64 << 20;
const MAX_TOTAL_BYTES: u64 = 1 << 30;
const MAX_JSON_BYTES: u64 = 32 << 20;
const MAX_NAME_CHARS: usize = 200;
/// Pixel size of a thumbnail.
const THUMBNAIL_PX: f64 = 64.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtFormat {
    Svg,
    Dxf,
    Hpgl,
    /// PDF and Illustrator (first page).
    Pdf,
}

impl ArtFormat {
    fn tag(self) -> &'static str {
        match self {
            ArtFormat::Svg => "svg",
            ArtFormat::Dxf => "dxf",
            ArtFormat::Hpgl => "plt",
            ArtFormat::Pdf => "pdf",
        }
    }

    fn from_tag(tag: &str) -> Option<Self> {
        Some(match tag {
            "svg" => ArtFormat::Svg,
            "dxf" => ArtFormat::Dxf,
            "plt" => ArtFormat::Hpgl,
            "pdf" => ArtFormat::Pdf,
            _ => return None,
        })
    }

    fn media(self) -> &'static str {
        match self {
            ArtFormat::Svg => "image/svg+xml",
            ArtFormat::Dxf => "image/vnd.dxf",
            ArtFormat::Hpgl => "application/vnd.hp-hpgl",
            ArtFormat::Pdf => "application/pdf",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ArtError {
    /// A name is empty, too long or contains control characters.
    InvalidName,
    DuplicateGroup(String),
    UnknownGroup(String),
    /// The group still holds items (and no group to move them to was given).
    GroupNotEmpty(String),
    UnknownItem(u64),
    /// A DXF without a unit needs an explicit scale first.
    ScaleRequired,
    /// The file does not import (message of the importer).
    Import(String),
    /// The file imports but holds no geometry.
    NothingToImport,
    TooMany,
}

impl std::fmt::Display for ArtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArtError::InvalidName => f.write_str("invalid name"),
            ArtError::DuplicateGroup(g) => write!(f, "group {g:?} exists"),
            ArtError::UnknownGroup(g) => write!(f, "no group {g:?}"),
            ArtError::GroupNotEmpty(g) => write!(f, "group {g:?} still has items"),
            ArtError::UnknownItem(id) => write!(f, "no art item {id}"),
            ArtError::ScaleRequired => f.write_str("the drawing has no unit; choose a scale"),
            ArtError::Import(m) => write!(f, "cannot import: {m}"),
            ArtError::NothingToImport => f.write_str("the file holds no geometry"),
            ArtError::TooMany => write!(f, "a library holds at most {MAX_ITEMS} items"),
        }
    }
}

impl std::error::Error for ArtError {}

/// What to add.
#[derive(Debug, Clone)]
pub struct NewArt<'a> {
    pub name: &'a str,
    pub group: &'a str,
    pub tags: Vec<String>,
    pub format: ArtFormat,
    pub bytes: &'a [u8],
    /// For a DXF without `$INSUNITS`; remembered with the item.
    pub dxf_scale: Option<DxfScale>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArtItem {
    id: u64,
    name: String,
    group: String,
    tags: Vec<String>,
    format: ArtFormat,
    hash: ContentHash,
    bytes: Arc<[u8]>,
    dxf_mm_per_unit: Option<f64>,
}

impl ArtItem {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn group(&self) -> &str {
        &self.group
    }
    pub fn tags(&self) -> &[String] {
        &self.tags
    }
    pub fn format(&self) -> ArtFormat {
        self.format
    }
    /// Asset hash of the stored bytes.
    pub fn hash(&self) -> ContentHash {
        self.hash
    }
}

/// An item ready to go into a project: the import draft plus the original file to store as asset.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtPlacement {
    pub draft: ImportDraft,
    pub media: &'static str,
    pub bytes: Arc<[u8]>,
    pub hash: ContentHash,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArtLibrary {
    id: String,
    next_id: u64,
    groups: Vec<String>,
    items: Vec<ArtItem>,
}

fn clean_name(s: &str) -> Result<String, ArtError> {
    let t = s.trim();
    if t.is_empty() || t.chars().count() > MAX_NAME_CHARS || t.chars().any(char::is_control) {
        return Err(ArtError::InvalidName);
    }
    Ok(t.to_owned())
}

fn draft_of(format: ArtFormat, bytes: &[u8], dxf_mm_per_unit: Option<f64>) -> Result<ImportDraft, ArtError> {
    let import = |e: &dyn std::fmt::Display| ArtError::Import(e.to_string());
    let draft = match format {
        ArtFormat::Svg => parse_svg(bytes).map_err(|e| import(&e))?,
        ArtFormat::Hpgl => parse_hpgl(bytes).map_err(|e| import(&e))?,
        ArtFormat::Pdf => {
            parse_pdf(bytes).map_err(|e| import(&e))?.to_draft(PdfPages::One(0)).map_err(|e| import(&e))?
        }
        ArtFormat::Dxf => {
            let scale = dxf_mm_per_unit.and_then(DxfScale::mm_per_unit);
            parse_dxf(bytes).map_err(|e| import(&e))?.to_draft(scale).map_err(|e| match e {
                ImportError::ScaleRequired => ArtError::ScaleRequired,
                other => import(&other),
            })?
        }
    };
    if draft.items.is_empty() { Err(ArtError::NothingToImport) } else { Ok(draft) }
}

impl ArtLibrary {
    /// An empty library; `id` identifies it (for example a generated uuid).
    pub fn new(id: &str) -> Self {
        ArtLibrary { id: id.to_owned(), next_id: 1, groups: Vec::new(), items: Vec::new() }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// Groups in display order.
    pub fn groups(&self) -> &[String] {
        &self.groups
    }

    pub fn items(&self) -> &[ArtItem] {
        &self.items
    }

    pub fn item(&self, id: u64) -> Option<&ArtItem> {
        self.items.iter().find(|i| i.id == id)
    }

    fn item_mut(&mut self, id: u64) -> Result<&mut ArtItem, ArtError> {
        self.items.iter_mut().find(|i| i.id == id).ok_or(ArtError::UnknownItem(id))
    }

    fn require_group(&self, name: &str) -> Result<(), ArtError> {
        if self.groups.iter().any(|g| g == name) { Ok(()) } else { Err(ArtError::UnknownGroup(name.to_owned())) }
    }

    pub fn add_group(&mut self, name: &str) -> Result<(), ArtError> {
        let name = clean_name(name)?;
        if self.groups.contains(&name) {
            return Err(ArtError::DuplicateGroup(name));
        }
        self.groups.push(name);
        Ok(())
    }

    /// Renames a group (position kept); its items follow.
    pub fn rename_group(&mut self, old: &str, new: &str) -> Result<(), ArtError> {
        self.require_group(old)?;
        let new = clean_name(new)?;
        if new != old && self.groups.contains(&new) {
            return Err(ArtError::DuplicateGroup(new));
        }
        for g in &mut self.groups {
            if g == old {
                g.clone_from(&new);
            }
        }
        for i in &mut self.items {
            if i.group == old {
                i.group.clone_from(&new);
            }
        }
        Ok(())
    }

    /// Removes a group. Items in it move to `move_to`; without a target a non-empty group is refused.
    pub fn remove_group(&mut self, name: &str, move_to: Option<&str>) -> Result<(), ArtError> {
        self.require_group(name)?;
        let occupied = self.items.iter().any(|i| i.group == name);
        if occupied {
            match move_to {
                Some(target) if target != name => {
                    self.require_group(target)?;
                    for i in &mut self.items {
                        if i.group == name {
                            i.group = target.to_owned();
                        }
                    }
                }
                _ => return Err(ArtError::GroupNotEmpty(name.to_owned())),
            }
        }
        self.groups.retain(|g| g != name);
        Ok(())
    }

    /// Adds an item after proving the file imports; nothing is added on failure.
    pub fn add_item(&mut self, new: NewArt<'_>) -> Result<u64, ArtError> {
        let name = clean_name(new.name)?;
        self.require_group(new.group)?;
        if self.items.len() >= MAX_ITEMS {
            return Err(ArtError::TooMany);
        }
        let dxf_mm_per_unit = new.dxf_scale.map(|s| s.factor());
        draft_of(new.format, new.bytes, dxf_mm_per_unit)?;
        let id = self.next_id;
        self.next_id += 1;
        self.items.push(ArtItem {
            id,
            name,
            group: new.group.to_owned(),
            tags: new.tags,
            format: new.format,
            hash: asset_hash(new.bytes),
            bytes: Arc::from(new.bytes),
            dxf_mm_per_unit,
        });
        Ok(id)
    }

    /// Adds a copy of `item` (from another library, for bundle import) under `name`, creating its
    /// group when missing. The file is validated again like any added item.
    pub(crate) fn add_copy(&mut self, item: &ArtItem, name: &str) -> Result<u64, ArtError> {
        match self.add_group(&item.group) {
            Ok(()) | Err(ArtError::DuplicateGroup(_)) => {}
            Err(e) => return Err(e),
        }
        self.add_item(NewArt {
            name,
            group: &item.group,
            tags: item.tags.clone(),
            format: item.format,
            bytes: &item.bytes,
            dxf_scale: item.dxf_mm_per_unit.and_then(DxfScale::mm_per_unit),
        })
    }

    pub fn move_item(&mut self, id: u64, group: &str) -> Result<(), ArtError> {
        self.require_group(group)?;
        self.item_mut(id)?.group = group.to_owned();
        Ok(())
    }

    pub fn rename_item(&mut self, id: u64, name: &str) -> Result<(), ArtError> {
        let name = clean_name(name)?;
        self.item_mut(id)?.name = name;
        Ok(())
    }

    pub fn set_tags(&mut self, id: u64, tags: Vec<String>) -> Result<(), ArtError> {
        self.item_mut(id)?.tags = tags;
        Ok(())
    }

    /// Removes an item; its id is never reused.
    pub fn remove_item(&mut self, id: u64) -> bool {
        let before = self.items.len();
        self.items.retain(|i| i.id != id);
        self.items.len() != before
    }

    pub fn items_in(&self, group: &str) -> Vec<&ArtItem> {
        self.items.iter().filter(|i| i.group == group).collect()
    }

    /// Items whose name, tags or group contain `text` (case-insensitive), in library order.
    pub fn search(&self, text: &str) -> Vec<&ArtItem> {
        let q = text.trim().to_lowercase();
        self.items
            .iter()
            .filter(|i| {
                i.name.to_lowercase().contains(&q)
                    || i.group.to_lowercase().contains(&q)
                    || i.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .collect()
    }

    /// A small SVG of the drawing (64 × 64 px, proportions kept), derived from the stored file.
    pub fn thumbnail(&self, id: u64) -> Result<String, ArtError> {
        let item = self.item(id).ok_or(ArtError::UnknownItem(id))?;
        let draft = draft_of(item.format, &item.bytes, item.dxf_mm_per_unit)?;
        let bounds = draft.items.iter().filter_map(|i| i.path.bounds()).reduce(Bounds::union);
        let size = bounds.map_or(1.0, |b| b.width().max(b.height()));
        let items: Vec<ExportItem<'_>> = draft.items.iter().map(|i| ExportItem::new(&i.name, &i.path)).collect();
        let options = SvgOptions {
            fit_px: Some((THUMBNAIL_PX, THUMBNAIL_PX)),
            // About one pixel wide whatever the drawing's size.
            stroke_mm: (size / THUMBNAIL_PX).max(1e-6),
            ..SvgOptions::default()
        };
        export_svg_with(&items, &options).map(|(svg, _)| svg).map_err(|_| ArtError::NothingToImport)
    }

    /// The item as an import, plus the original bytes to store as the project asset.
    pub fn place(&self, id: u64) -> Result<ArtPlacement, ArtError> {
        let item = self.item(id).ok_or(ArtError::UnknownItem(id))?;
        let draft = draft_of(item.format, &item.bytes, item.dxf_mm_per_unit)?;
        Ok(ArtPlacement { draft, media: item.format.media(), bytes: item.bytes.clone(), hash: item.hash })
    }

    /// A library of the chosen items only (same ids), with just the groups that still hold one.
    pub fn subset(&self, ids: &[u64]) -> ArtLibrary {
        let items: Vec<ArtItem> = self.items.iter().filter(|i| ids.contains(&i.id)).cloned().collect();
        let groups = self.groups.iter().filter(|g| items.iter().any(|i| &i.group == *g)).cloned().collect();
        ArtLibrary { id: self.id.clone(), next_id: self.next_id, groups, items }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileDto {
    format: String,
    schema: u32,
    id: String,
    next_id: u64,
    groups: Vec<String>,
    items: Vec<ItemDto>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemDto {
    id: u64,
    name: String,
    group: String,
    tags: Vec<String>,
    format: String,
    hash: String,
    bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dxf_mm_per_unit: Option<f64>,
}

fn options() -> SimpleFileOptions {
    // Fixed timestamp: identical libraries give identical bytes.
    SimpleFileOptions::default().compression_method(CompressionMethod::Stored).last_modified_time(DateTime::default())
}

/// Library → zip bytes (deterministic).
pub fn encode_art_library(library: &ArtLibrary) -> Result<Vec<u8>, SaveError> {
    let enc = |e: &dyn std::fmt::Display| SaveError::Encode(e.to_string());
    let dto = FileDto {
        format: FORMAT.into(),
        schema: ART_LIBRARY_SCHEMA,
        id: library.id.clone(),
        next_id: library.next_id,
        groups: library.groups.clone(),
        items: library
            .items
            .iter()
            .map(|i| ItemDto {
                id: i.id,
                name: i.name.clone(),
                group: i.group.clone(),
                tags: i.tags.clone(),
                format: i.format.tag().into(),
                hash: i.hash.to_hex(),
                bytes: i.bytes.len() as u64,
                dxf_mm_per_unit: i.dxf_mm_per_unit,
            })
            .collect(),
    };
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file(MANIFEST, options()).map_err(|e| enc(&e))?;
    zip.write_all(&serde_json::to_vec_pretty(&dto).map_err(|e| enc(&e))?).map_err(|e| enc(&e))?;
    for i in &library.items {
        zip.start_file(format!("{ITEM_DIR}{}", i.id), options()).map_err(|e| enc(&e))?;
        zip.write_all(&i.bytes).map_err(|e| enc(&e))?;
    }
    Ok(zip.finish().map_err(|e| enc(&e))?.into_inner())
}

fn json_error(e: &serde_json::Error) -> LoadError {
    let detail = e.to_string();
    if detail.starts_with("unknown field") || detail.starts_with("unknown variant") {
        LoadError::UnknownField { entry: MANIFEST, detail }
    } else {
        LoadError::Json { entry: MANIFEST, detail }
    }
}

/// Zip bytes → library, fully validated (hashes, sizes, groups, ids, entry names).
pub fn decode_art_library(bytes: &[u8]) -> Result<ArtLibrary, LoadError> {
    let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(|e| LoadError::Container(e.to_string()))?;
    if zip.len() > MAX_ITEMS + 1 {
        return Err(LoadError::Limit("entry count"));
    }
    let mut manifest = None;
    let mut files: std::collections::BTreeMap<u64, Vec<u8>> = std::collections::BTreeMap::new();
    let mut total = 0u64;
    for k in 0..zip.len() {
        let mut f = zip.by_index(k).map_err(|e| LoadError::Container(e.to_string()))?;
        let name = f.name().map_err(|e| LoadError::Container(e.to_string()))?.into_owned();
        let (limit, item_id) = if name == MANIFEST {
            (MAX_JSON_BYTES, None)
        } else if let Some(id) =
            name.strip_prefix(ITEM_DIR).filter(|r| !r.is_empty() && r.bytes().all(|b| b.is_ascii_digit()))
        {
            (MAX_ITEM_BYTES, id.parse::<u64>().ok())
        } else {
            return Err(LoadError::UnexpectedEntry(name));
        };
        let mut data = Vec::new();
        (&mut f).take(limit + 1).read_to_end(&mut data).map_err(|e| LoadError::Container(e.to_string()))?;
        if data.len() as u64 > limit {
            return Err(LoadError::Limit("entry size"));
        }
        total += data.len() as u64;
        if total > MAX_TOTAL_BYTES {
            return Err(LoadError::Limit("total size"));
        }
        match item_id {
            None => {
                if manifest.replace(data).is_some() {
                    return Err(LoadError::UnexpectedEntry(name));
                }
            }
            Some(id) => {
                if files.insert(id, data).is_some() {
                    return Err(LoadError::UnexpectedEntry(name));
                }
            }
        }
    }
    let manifest = manifest.ok_or(LoadError::MissingEntry(MANIFEST))?;
    let loose: serde_json::Value = serde_json::from_slice(&manifest).map_err(|e| json_error(&e))?;
    if loose.get("format").and_then(serde_json::Value::as_str) != Some(FORMAT) {
        return Err(LoadError::NotAProject);
    }
    let schema = loose.get("schema").and_then(serde_json::Value::as_u64).ok_or(LoadError::NotAProject)?;
    if schema > u64::from(ART_LIBRARY_SCHEMA) {
        return Err(LoadError::NewerSchema {
            found: u32::try_from(schema).unwrap_or(u32::MAX),
            supported: ART_LIBRARY_SCHEMA,
        });
    }
    if schema < 1 {
        return Err(LoadError::InvalidValue { path: "schema".into(), reason: format!("unknown schema {schema}") });
    }
    let dto: FileDto = serde_json::from_value(loose).map_err(|e| json_error(&e))?;
    let bad = |path: String, reason: String| LoadError::InvalidValue { path, reason };
    let mut groups: Vec<String> = Vec::with_capacity(dto.groups.len());
    for (k, g) in dto.groups.iter().enumerate() {
        let clean = clean_name(g).map_err(|e| bad(format!("groups[{k}]"), e.to_string()))?;
        if clean != *g || groups.contains(g) {
            return Err(bad(format!("groups[{k}]"), "duplicate or untrimmed group name".into()));
        }
        groups.push(g.clone());
    }
    let mut seen = BTreeSet::new();
    let mut items = Vec::with_capacity(dto.items.len());
    for (k, d) in dto.items.iter().enumerate() {
        let at = |f: &str| format!("items[{k}].{f}");
        if !seen.insert(d.id) {
            return Err(bad(at("id"), format!("duplicate id {}", d.id)));
        }
        if d.id >= dto.next_id {
            return Err(bad("next_id".into(), format!("must exceed every item id (item {})", d.id)));
        }
        let name = clean_name(&d.name).map_err(|e| bad(at("name"), e.to_string()))?;
        if !groups.contains(&d.group) {
            return Err(bad(at("group"), format!("no group {:?}", d.group)));
        }
        let format = ArtFormat::from_tag(&d.format)
            .ok_or_else(|| bad(at("format"), format!("unknown format {:?}", d.format)))?;
        let data = files.remove(&d.id).ok_or(LoadError::MissingEntry("items/<id>"))?;
        let hash = ContentHash::from_hex(&d.hash).map_err(|e| bad(at("hash"), e.to_string()))?;
        if asset_hash(&data) != hash || data.len() as u64 != d.bytes {
            return Err(LoadError::AssetHashMismatch(hash));
        }
        if d.dxf_mm_per_unit.is_some_and(|v| !(v.is_finite() && v > 0.0)) {
            return Err(bad(at("dxf_mm_per_unit"), "must be positive".into()));
        }
        items.push(ArtItem {
            id: d.id,
            name,
            group: d.group.clone(),
            tags: d.tags.clone(),
            format,
            hash,
            bytes: Arc::from(data),
            dxf_mm_per_unit: d.dxf_mm_per_unit,
        });
    }
    if let Some(stray) = files.keys().next() {
        return Err(LoadError::UnexpectedEntry(format!("{ITEM_DIR}{stray}")));
    }
    Ok(ArtLibrary { id: dto.id, next_id: dto.next_id, groups, items })
}

/// Atomic save through the filesystem port.
pub fn save_art_library(fs: &dyn FileSystem, path: &Path, library: &ArtLibrary) -> Result<(), SaveError> {
    let bytes = encode_art_library(library)?;
    decode_art_library(&bytes).map_err(|e| SaveError::Invalid(e.to_string()))?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs.create_dir_all(dir).map_err(SaveError::Fs)?;
    }
    write_atomic(fs, path, &bytes)
}

pub fn load_art_library(fs: &dyn FileSystem, path: &Path) -> Result<ArtLibrary, LoadError> {
    let bytes = fs.read(path, MAX_TOTAL_BYTES + MAX_JSON_BYTES).map_err(LoadError::Fs)?;
    decode_art_library(&bytes)
}
