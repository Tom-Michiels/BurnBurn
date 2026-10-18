//! System fonts for canvas text input (`AppController::attach_fonts`): the first readable face of
//! a host-supplied candidate list, read with plain std. No font-folder scan, no substitution.

use std::path::PathBuf;
use std::sync::Arc;

use burnburn_text::{FontData, FontRef, FontSource, FontStyle, MemoryFontSource};

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
