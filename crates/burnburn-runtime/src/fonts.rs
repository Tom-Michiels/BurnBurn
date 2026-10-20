//! System fonts for canvas text input (`AppController::attach_fonts`): the first readable face of
//! a host-supplied candidate list, read with plain std. No font-folder scan, no substitution.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use burnburn_text::{FontData, FontError, FontRef, FontSource, FontStyle, MemoryFontSource, font_names};

/// A font source with one registered face and the reference new text uses.
pub struct SystemFont {
    pub source: Arc<dyn FontSource + Send + Sync>,
    pub font: FontRef,
}

/// The first candidate that is a readable font.
/// The family is the file stem; the reference embeds the face's asset hash, so a project never
/// resolves it to a different face later.
pub fn first_readable_font(candidates: &[PathBuf]) -> Option<SystemFont> {
    candidates.iter().find_map(|path| {
        let data = FontData::from_bytes(std::fs::read(path).ok()?).ok()?;
        let family = path.file_stem()?.to_str()?;
        let mut source = MemoryFontSource::new();
        let font = source.insert(family, FontStyle::Regular, Arc::new(data));
        Some(SystemFont { source: Arc::new(source), font })
    })
}

/// Fonts every supported OS ships, most suitable first (Latin outlines for engraving).
pub fn default_font_candidates() -> Vec<PathBuf> {
    let paths: &[&str] = if cfg!(target_os = "macos") {
        &["/System/Library/Fonts/Supplemental/Arial.ttf", "/System/Library/Fonts/Supplemental/Verdana.ttf"]
    } else if cfg!(windows) {
        &["C:\\Windows\\Fonts\\arial.ttf", "C:\\Windows\\Fonts\\segoeui.ttf"]
    } else {
        &["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"]
    };
    paths.iter().map(PathBuf::from).collect()
}

/// Largest font file the catalogue reads (bytes); anything bigger is not offered.
const MAX_FONT_BYTES: u64 = 64 * 1024 * 1024;

/// Every TrueType/OpenType file in some directories, by the family and style it declares (W3-UI-tekst:
/// the text toolbar's font choice). The index is built once, on first use or in the background;
/// a face is read and parsed only when text asks for it. A face that is not listed is
/// [`FontError::NotFound`] — never another face with a similar name. A reference that names an
/// embedded asset resolves only to a face this catalogue has already loaded with that hash.
pub struct SystemFontCatalog {
    dirs: Vec<PathBuf>,
    index: OnceLock<BTreeMap<(String, FontStyle), PathBuf>>,
    loaded: Mutex<BTreeMap<(String, FontStyle), Arc<FontData>>>,
}

/// System-internal families (macOS names them with a leading dot: ".SF Compact", ".LastResort")
/// are for the OS's own interface, not a choice for engraved text.
fn is_hidden(family: &str) -> bool {
    family.starts_with('.')
}

fn scan(dirs: &[PathBuf]) -> BTreeMap<(String, FontStyle), PathBuf> {
    let mut index = BTreeMap::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        paths.sort();
        for path in paths {
            let is_font = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("ttf") || e.eq_ignore_ascii_case("otf"));
            let small = std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= MAX_FONT_BYTES);
            if !is_font || !small {
                continue;
            }
            let Some(names) = std::fs::read(&path).ok().and_then(|bytes| font_names(&bytes)) else { continue };
            if is_hidden(&names.0) {
                continue;
            }
            // The first directory and file name win: a duplicate never replaces what is listed.
            index.entry(names).or_insert(path);
        }
    }
    index
}

impl SystemFontCatalog {
    /// A catalogue over `dirs` (searched in this order, not recursively).
    pub fn new(dirs: Vec<PathBuf>) -> Arc<SystemFontCatalog> {
        Arc::new(SystemFontCatalog { dirs, index: OnceLock::new(), loaded: Mutex::new(BTreeMap::new()) })
    }

    /// The font folders every supported OS ships (and the user's own on macOS).
    pub fn system() -> Arc<SystemFontCatalog> {
        SystemFontCatalog::new(system_font_dirs())
    }

    /// Builds the index on a background thread, so the first use does not wait for it.
    pub fn scan_in_background(self: &Arc<Self>) {
        let catalog = Arc::clone(self);
        let _ = std::thread::Builder::new().name("font-catalogue".into()).spawn(move || {
            catalog.index();
        });
    }

    fn index(&self) -> &BTreeMap<(String, FontStyle), PathBuf> {
        self.index.get_or_init(|| scan(&self.dirs))
    }

    /// The face new text starts with: the first of some common families the catalogue has, else
    /// its first family. `None` for an empty catalogue.
    pub fn default_font(&self) -> Option<FontRef> {
        let families = self.families();
        let preferred = ["Arial", "Helvetica", "Liberation Sans", "DejaVu Sans", "Segoe UI"];
        let family = preferred.iter().find(|p| families.iter().any(|f| f == *p)).map(|p| (*p).to_owned());
        let family = family.or_else(|| families.into_iter().next())?;
        self.index()
            .contains_key(&(family.clone(), FontStyle::Regular))
            .then(|| FontRef::new(family, FontStyle::Regular))
    }
}

impl FontSource for SystemFontCatalog {
    fn load(&self, font: &FontRef) -> Result<Arc<FontData>, FontError> {
        let key = (font.family.clone(), font.style);
        let mut loaded = self.loaded.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(hash) = font.asset {
            return loaded
                .values()
                .find(|d| d.hash() == hash)
                .cloned()
                .ok_or_else(|| FontError::NotFound(font.clone()));
        }
        if let Some(face) = loaded.get(&key) {
            return Ok(Arc::clone(face));
        }
        let path = self.index().get(&key).ok_or_else(|| FontError::NotFound(font.clone()))?;
        let bytes = std::fs::read(path).map_err(|_| FontError::NotFound(font.clone()))?;
        let face = Arc::new(FontData::from_bytes(bytes)?);
        loaded.insert(key, Arc::clone(&face));
        Ok(face)
    }

    fn available_styles(&self, family: &str) -> Vec<FontStyle> {
        self.index().keys().filter(|(f, _)| f == family).map(|(_, s)| *s).collect()
    }

    fn families(&self) -> Vec<String> {
        let mut families: Vec<String> = self.index().keys().map(|(f, _)| f.clone()).collect();
        families.dedup();
        families
    }
}

/// Font folders to catalogue, system ones first.
pub fn system_font_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = if cfg!(target_os = "macos") {
        vec!["/System/Library/Fonts/Supplemental".into(), "/System/Library/Fonts".into(), "/Library/Fonts".into()]
    } else if cfg!(windows) {
        vec!["C:\\Windows\\Fonts".into()]
    } else {
        vec!["/usr/share/fonts/truetype/dejavu".into(), "/usr/share/fonts/truetype/liberation".into()]
    };
    if cfg!(target_os = "macos")
        && let Some(home) = std::env::var_os("HOME")
    {
        dirs.push(PathBuf::from(home).join("Library/Fonts"));
    }
    dirs
}

#[cfg(test)]
mod tests {
    use super::is_hidden;

    #[test]
    fn dot_families_are_system_internal_and_others_are_not() {
        assert!(is_hidden(".SF Compact") && is_hidden(".LastResort"));
        assert!(!is_hidden("Arial") && !is_hidden("SF Mono"));
    }
}
