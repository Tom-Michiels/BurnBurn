//! Learned recognition templates (G8-cam-werk, W06): the grey pixels of a template the operator
//! marked in a camera picture, kept between sessions with the exact camera configuration (device,
//! resolution, pixel format, frame rate, lens profile) it was learned in. A template is only searched
//! in pictures of that configuration; the cut geometry is never part of it.
//!
//! JSON (`format`, `schema`, `next_id`, `templates`), written atomically through the [`FileSystem`]
//! port. The pixels are 8-bit grey values, two hex digits each, row by row. A template that would
//! mean nothing (no name, a size outside 4 to 4096, a pixel count that does not match, a flat grey
//! with no pattern, a camera configuration without a camera) is never written, and a file that
//! holds one is a typed error and never an empty library. Names are unique (case ignored); ids are
//! never reused, also not after a removal. A newer schema is reported with its version and the file is
//! left alone.

use std::collections::BTreeSet;
use std::path::Path;

use burnburn_image::calib::CameraConfigKey;
use serde::{Deserialize, Serialize};

use crate::camera_calibration_library::{ConfigDto, config_dto, config_of};
use crate::error::{LoadError, SaveError};
use crate::fs::FileSystem;
use crate::store::write_atomic;

pub const RECOGNITION_LIBRARY_SCHEMA: u32 = 1;
const FORMAT: &str = "burnburn-recognition-templates";
const MAX_TEMPLATES: usize = 1_000;
const MIN_SIDE: u32 = 4;
const MAX_SIDE: u32 = 4096;
/// A template whose grey values vary less than this (standard deviation of 255 levels) has no
/// pattern to look for (the same limit as the locator).
const MIN_DEVIATION: f64 = 4.0;
const MAX_BYTES: u64 = 256 << 20;

/// One learned template as it is stored.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateRecord {
    pub id: u64,
    pub name: String,
    /// The camera configuration the template was learned in.
    pub config: CameraConfigKey,
    pub width: u32,
    pub height: u32,
    /// `width * height` grey values, row by row.
    pub gray8: Vec<u8>,
}

/// The templates and the next id to hand out.
#[derive(Debug, Clone, PartialEq)]
pub struct RecognitionLibrary {
    pub templates: Vec<TemplateRecord>,
    pub next_id: u64,
}

/// Why a template cannot be kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateProblem {
    NoName,
    TooSmall,
    TooLarge,
    /// The number of grey values is not `width * height`.
    SizeMismatch,
    /// (Almost) one grey: there is no pattern to look for.
    Flat,
}

fn problem_of(width: u32, height: u32, gray8: &[u8]) -> Option<TemplateProblem> {
    if width < MIN_SIDE || height < MIN_SIDE {
        return Some(TemplateProblem::TooSmall);
    }
    if width > MAX_SIDE || height > MAX_SIDE {
        return Some(TemplateProblem::TooLarge);
    }
    if gray8.len() != width as usize * height as usize {
        return Some(TemplateProblem::SizeMismatch);
    }
    let n = gray8.len() as f64;
    let mean = gray8.iter().map(|v| f64::from(*v)).sum::<f64>() / n;
    let deviation = (gray8.iter().map(|v| (f64::from(*v) - mean).powi(2)).sum::<f64>() / n).sqrt();
    (deviation < MIN_DEVIATION).then_some(TemplateProblem::Flat)
}

impl RecognitionLibrary {
    pub fn empty() -> Self {
        RecognitionLibrary { templates: Vec::new(), next_id: 1 }
    }

    pub fn find(&self, id: u64) -> Option<&TemplateRecord> {
        self.templates.iter().find(|t| t.id == id)
    }

    /// Keeps a template. A name that is in use (case ignored) is the same template taught again: it
    /// keeps its id and takes the new pixels and configuration. Returns the id.
    pub fn upsert(
        &mut self,
        name: &str,
        config: CameraConfigKey,
        width: u32,
        height: u32,
        gray8: Vec<u8>,
    ) -> Result<u64, TemplateProblem> {
        let name = name.trim();
        if name.is_empty() {
            return Err(TemplateProblem::NoName);
        }
        if let Some(problem) = problem_of(width, height, &gray8) {
            return Err(problem);
        }
        let key = name.to_lowercase();
        if let Some(existing) = self.templates.iter_mut().find(|t| t.name.trim().to_lowercase() == key) {
            *existing = TemplateRecord { id: existing.id, name: name.to_owned(), config, width, height, gray8 };
            return Ok(existing.id);
        }
        let id = self.next_id;
        self.next_id = id.saturating_add(1);
        self.templates.push(TemplateRecord { id, name: name.to_owned(), config, width, height, gray8 });
        Ok(id)
    }

    /// Removes a template. Its id is not handed out again. Whether there was one.
    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.templates.len();
        self.templates.retain(|t| t.id != id);
        self.templates.len() != before
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TemplateDto {
    id: u64,
    name: String,
    config: ConfigDto,
    width: u32,
    height: u32,
    gray8_hex: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileDto {
    format: String,
    schema: u32,
    /// Absent in a file written without it: then it continues after the highest id.
    #[serde(default)]
    next_id: Option<u64>,
    templates: Vec<TemplateDto>,
}

fn json_error(e: &serde_json::Error) -> LoadError {
    let detail = e.to_string();
    if detail.starts_with("unknown field") || detail.starts_with("unknown variant") {
        LoadError::UnknownField { entry: "recognition templates", detail }
    } else {
        LoadError::Json { entry: "recognition templates", detail }
    }
}

fn invalid(path: String, reason: &str) -> LoadError {
    LoadError::InvalidValue { path, reason: reason.to_owned() }
}

fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(char::from(DIGITS[usize::from(b >> 4)]));
        out.push(char::from(DIGITS[usize::from(b & 15)]));
    }
    out
}

fn from_hex(text: &str) -> Option<Vec<u8>> {
    let digit = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    };
    let bytes = text.as_bytes();
    if bytes.len() % 2 != 0 {
        return None;
    }
    bytes.chunks(2).map(|pair| Some(digit(pair[0])? << 4 | digit(pair[1])?)).collect()
}

/// Does this library mean something? The same checks a file is read with and written after.
pub fn validate_recognition_library(library: &RecognitionLibrary) -> Result<(), LoadError> {
    if library.templates.len() > MAX_TEMPLATES {
        return Err(LoadError::Limit("recognition templates"));
    }
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    for (i, t) in library.templates.iter().enumerate() {
        let at = |field: &str| format!("templates[{i}].{field}");
        if t.name.trim().is_empty() {
            return Err(invalid(at("name"), "a template has a name"));
        }
        if let Some(problem) = problem_of(t.width, t.height, &t.gray8) {
            let reason = match problem {
                TemplateProblem::TooSmall => "a template is at least 4 x 4 pixels",
                TemplateProblem::TooLarge => "a template is at most 4096 pixels on a side",
                TemplateProblem::SizeMismatch => "the number of grey values is width times height",
                TemplateProblem::Flat => "a template has a pattern, not one grey",
                TemplateProblem::NoName => "a template has a name",
            };
            return Err(invalid(at("gray8_hex"), reason));
        }
        let c = &t.config;
        if c.device.0.trim().is_empty() {
            return Err(invalid(at("config.device"), "a template belongs to a camera"));
        }
        if c.mode.width == 0 || c.mode.height == 0 {
            return Err(invalid(at("config.width"), "a picture has a size"));
        }
        if c.mode.frame_rate_mhz == 0 {
            return Err(invalid(at("config.frame_rate_mhz"), "a frame rate is above zero"));
        }
        if c.lens.trim().is_empty() {
            return Err(invalid(at("config.lens"), "a lens profile has a name"));
        }
        if !ids.insert(t.id) {
            return Err(invalid(at("id"), "two templates with one id"));
        }
        if !names.insert(t.name.trim().to_lowercase()) {
            return Err(invalid(at("name"), "two templates with one name"));
        }
    }
    let after_highest = library.templates.iter().map(|t| t.id).max().map_or(1, |m| m.saturating_add(1));
    if library.next_id < after_highest {
        return Err(invalid("next_id".into(), "the next id would be one that is in use"));
    }
    Ok(())
}

/// Bytes → templates, fully validated.
pub fn decode_recognition_library(bytes: &[u8]) -> Result<RecognitionLibrary, LoadError> {
    let loose: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| json_error(&e))?;
    if loose.get("format").and_then(serde_json::Value::as_str) != Some(FORMAT) {
        return Err(LoadError::NotAProject);
    }
    let schema = loose.get("schema").and_then(serde_json::Value::as_u64).ok_or(LoadError::NotAProject)?;
    if schema > u64::from(RECOGNITION_LIBRARY_SCHEMA) {
        return Err(LoadError::NewerSchema {
            found: u32::try_from(schema).unwrap_or(u32::MAX),
            supported: RECOGNITION_LIBRARY_SCHEMA,
        });
    }
    if schema < 1 {
        return Err(invalid("schema".into(), &format!("unknown schema {schema}")));
    }
    let dto: FileDto = serde_json::from_value(loose).map_err(|e| json_error(&e))?;
    if dto.templates.len() > MAX_TEMPLATES {
        return Err(LoadError::Limit("recognition templates"));
    }
    let after_highest = dto.templates.iter().map(|t| t.id).max().map_or(1, |m| m.saturating_add(1));
    let mut templates = Vec::with_capacity(dto.templates.len());
    for (i, t) in dto.templates.into_iter().enumerate() {
        let gray8 = from_hex(&t.gray8_hex)
            .ok_or_else(|| invalid(format!("templates[{i}].gray8_hex"), "grey values are pairs of hex digits"))?;
        // The configuration is read with the checks of the camera calibrations.
        let config = config_of(&format!("templates[{i}].config"), t.config)?;
        templates.push(TemplateRecord { id: t.id, name: t.name, config, width: t.width, height: t.height, gray8 });
    }
    let library = RecognitionLibrary { next_id: dto.next_id.unwrap_or(after_highest), templates };
    validate_recognition_library(&library)?;
    Ok(library)
}

/// Templates → bytes (compact JSON). Not validated here: [`save_recognition_library`] decodes what it wrote.
pub fn encode_recognition_library(library: &RecognitionLibrary) -> Result<Vec<u8>, SaveError> {
    let dto = FileDto {
        format: FORMAT.to_owned(),
        schema: RECOGNITION_LIBRARY_SCHEMA,
        next_id: Some(library.next_id),
        templates: library
            .templates
            .iter()
            .map(|t| TemplateDto {
                id: t.id,
                name: t.name.clone(),
                config: config_dto(&t.config),
                width: t.width,
                height: t.height,
                gray8_hex: to_hex(&t.gray8),
            })
            .collect(),
    };
    serde_json::to_vec(&dto).map_err(|e| SaveError::Encode(e.to_string()))
}

/// Atomic save. What is written is decoded again first, so a library that would not load (a
/// duplicate name, a flat template) is never written and the old file stays.
pub fn save_recognition_library(
    fs: &dyn FileSystem,
    path: &Path,
    library: &RecognitionLibrary,
) -> Result<(), SaveError> {
    let bytes = encode_recognition_library(library)?;
    decode_recognition_library(&bytes).map_err(|e| SaveError::Invalid(e.to_string()))?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs.create_dir_all(dir).map_err(SaveError::Fs)?;
    }
    write_atomic(fs, path, &bytes)
}

pub fn load_recognition_library(fs: &dyn FileSystem, path: &Path) -> Result<RecognitionLibrary, LoadError> {
    let bytes = fs.read(path, MAX_BYTES).map_err(LoadError::Fs)?;
    decode_recognition_library(&bytes)
}
