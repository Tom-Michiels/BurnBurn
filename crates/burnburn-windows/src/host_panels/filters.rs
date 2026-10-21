//! File type filters of the dialogs (`IFileDialog::SetFileTypes`), shared by the families.
//! The extension lists are the macOS host's (`BITMAP_TYPES`, `TRACE_TYPES`, the import panel's list).

use super::FileFilter;

/// Picture types the bitmap flow offers. BMP and TIFF are listed so the decoder's honest refusal
/// reaches the user instead of the file silently not being selectable.
pub const BITMAP_TYPES: [&str; 6] = ["png", "jpg", "jpeg", "bmp", "tif", "tiff"];

/// Picture types the tracer reads.
pub const TRACE_TYPES: [&str; 3] = ["png", "jpg", "jpeg"];

/// Drawing types the import flow reads (vector first, then pictures).
pub const DRAWING_TYPES: [&str; 8] = ["dxf", "svg", "plt", "hpgl", "hgl", "pdf", "ai", "lbrn2"];

fn filter(name: &str, extensions: &[&'static str]) -> FileFilter {
    FileFilter { name: name.to_owned(), extensions: extensions.to_vec() }
}

pub fn bitmap_filter() -> FileFilter {
    filter("Afbeeldingen / Images", &BITMAP_TYPES)
}

pub fn trace_filter() -> FileFilter {
    filter("PNG, JPEG", &TRACE_TYPES)
}

pub fn png_filter() -> FileFilter {
    filter("PNG", &["png"])
}

pub fn drawing_filter() -> FileFilter {
    filter("Tekeningen / Drawings", &DRAWING_TYPES)
}

/// The import dialog: `bitmap_first` puts the pictures first (Beeld ▸ Beeld importeren…).
pub fn import_filters(bitmap_first: bool) -> Vec<FileFilter> {
    if bitmap_first { vec![bitmap_filter(), drawing_filter()] } else { vec![drawing_filter(), bitmap_filter()] }
}
