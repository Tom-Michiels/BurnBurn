//! LF3 (Q8): counts agree with their noun and numbers follow the UI language, in the texts the
//! panels build.

use burnburn_host_models::l10n::Locale;
use burnburn_host_models::panels::cam_note;
use burnburn_host_models::panels_files::export_objects_text;
use burnburn_presentation::DialogNote;

#[test]
fn an_array_extent_is_written_in_the_ui_language_without_noise_decimals() {
    let note = DialogNote::ArrayExtent { copies: 8, width_um: 85_000, height_um: 28_500 };
    assert_eq!(cam_note(note, Locale::Nl), "8 kopieën (met het origineel), samen 85 × 28,5 mm.");
    assert_eq!(cam_note(note, Locale::En), "8 copies (with the original), together 85 × 28.5 mm.");
}

#[test]
fn a_large_array_extent_is_grouped_not_a_wall_of_zeros() {
    let note = DialogNote::ArrayExtent { copies: 1, width_um: 250_000_000, height_um: 160_000_000 };
    assert_eq!(cam_note(note, Locale::Nl), "1 kopie (met het origineel), samen 250.000 × 160.000 mm.");
}

#[test]
fn the_export_sheet_counts_objects_in_the_singular_and_the_plural() {
    assert_eq!(export_objects_text(1, Locale::Nl), "1 object wordt geëxporteerd.");
    assert_eq!(export_objects_text(2, Locale::Nl), "2 objecten worden geëxporteerd.");
    assert_eq!(export_objects_text(1, Locale::En), "1 object will be exported.");
    assert_eq!(export_objects_text(3, Locale::En), "3 objects will be exported.");
}
