//! Names the model stores in Dutch and the engines' English hints, shown in the UI language (LF3, Q7).
//!
//! The document stores user data, not localisation keys (`presentation::commit::base_name`): a new
//! rectangle is "Rechthoek 1" in the file. Hosts show it through [`display_name`], which translates
//! only a name that is exactly a default name (base word, optional counter, optional link marker);
//! a name the user typed is never touched.

use crate::l10n::Locale;

/// Dutch default base word and its English form, for objects, sketches and documents.
const BASES: &[(&str, &str)] = &[
    ("Naamloos", "Untitled"),
    ("Hersteld", "Recovered"),
    ("Schets", "Sketch"),
    ("Rechthoek", "Rectangle"),
    ("Driehoek", "Triangle"),
    ("Vijfhoek", "Pentagon"),
    ("Achthoek", "Octagon"),
    ("Ellips", "Ellipse"),
    ("Cirkel", "Circle"),
    ("Lijn", "Line"),
    ("Polylijn", "Polyline"),
    ("Veelhoek", "Polygon"),
    ("Ster", "Star"),
    ("Dubbele ster", "Double Star"),
    ("Afgeronde rechthoek", "Rounded Rectangle"),
];

/// `name` as shown in `locale`: a default name ("Rechthoek 2", "Naamloos", "Schets 1") follows the
/// UI language, every other name is returned as is.
pub fn display_name(name: &str, locale: Locale) -> String {
    if locale == Locale::Nl {
        return name.to_owned();
    }
    // The link marker the object list appends ("Rechthoek 1 (×2)") is kept.
    let (core, marker) = match name.rfind(" (\u{d7}") {
        Some(at) if name.ends_with(')') => name.split_at(at),
        _ => (name, ""),
    };
    if core == "Naamloos" {
        return format!("Untitled{marker}");
    }
    let Some((base, counter)) = core.rsplit_once(' ') else { return name.to_owned() };
    if counter.is_empty() || !counter.chars().all(|c| c.is_ascii_digit()) {
        return name.to_owned();
    }
    match BASES.iter().find(|(nl, _)| *nl == base) {
        Some((_, en)) => format!("{en} {counter}{marker}"),
        None => name.to_owned(),
    }
}

/// Dutch forms of the hints `burnburn_generators` writes in English.
const HINTS: &[(&str, &str)] = &[
    ("digits", "cijfers"),
    ("marker id 0-29", "markernummer 0-29"),
    ("marker id 0-34", "markernummer 0-34"),
    ("marker id 0-2319", "markernummer 0-2319"),
    ("marker id 0-586", "markernummer 0-586"),
    ("marker id 0-99", "markernummer 0-99"),
    ("not available", "niet beschikbaar"),
    (
        "text (ISO 8859-1) or bytes; error correction is fixed (about 23 % + 11 bits)",
        "tekst (ISO 8859-1) of bytes; de foutcorrectie ligt vast (ongeveer 23 % + 11 bits)",
    ),
    (
        "text or bytes, Han Xin versions 1-3 only (23, 25 or 27 modules; about 10 to 40 characters)",
        "tekst of bytes, alleen Han Xin-versies 1 tot 3 (23, 25 of 27 modules; ongeveer 10 tot 40 tekens)",
    ),
    (
        "text (ISO 8859-1) or bytes; ECC 200, error correction is fixed",
        "tekst (ISO 8859-1) of bytes; ECC 200, de foutcorrectie ligt vast",
    ),
    (
        "start letter A-D, data 0-9 - $ : / . +, stop letter A-D",
        "startletter A-D, gegevens 0-9 - $ : / . +, stopletter A-D",
    ),
    ("ASCII 32-127", "ASCII 32-127"),
    ("GS1 element string (01)09501101530003(10)LOT", "GS1-elementstring (01)09501101530003(10)LOT"),
    ("0-9 A-Z - . space $ / + %", "0-9 A-Z - . spatie $ / + %"),
    (
        "7 or 12 digits (check digit added), or 8 or 13 with check digit",
        "7 of 12 cijfers (het controlecijfer wordt toegevoegd), of 8 of 13 met controlecijfer",
    ),
    ("ISBN-13 (978/979) or ISBN-10, hyphens allowed", "ISBN-13 (978/979) of ISBN-10, koppeltekens toegestaan"),
    ("hexadecimal digits 0-9 A-F", "hexadecimale cijfers 0-9 A-F"),
    (
        "11 digits (check digit added) or 12 with check digit",
        "11 cijfers (het controlecijfer wordt toegevoegd) of 12 met controlecijfer",
    ),
    (
        "6, 7 or 8 digits (zero-suppressed, number system 0 or 1)",
        "6, 7 of 8 cijfers (nulonderdrukking, nummersysteem 0 of 1)",
    ),
    (
        "17 characters, no I O Q; check digit verified for North American VINs",
        "17 tekens, geen I O Q; het controlecijfer wordt gecontroleerd voor Noord-Amerikaanse VIN's",
    ),
    ("text (ISO 8859-1) or bytes", "tekst (ISO 8859-1) of bytes"),
    (
        "text (ISO 8859-1) or bytes; error correction is fixed by the size",
        "tekst (ISO 8859-1) of bytes; de foutcorrectie volgt uit de grootte",
    ),
    ("text or bytes, up to version 10", "tekst of bytes, tot versie 10"),
    ("text or bytes, up to 35 digits", "tekst of bytes, tot 35 cijfers"),
    ("text or bytes", "tekst of bytes"),
];

/// The barcode panel's payload hint, which the symbology engine writes in English, in `locale`.
/// A hint without a translation is shown as the engine wrote it.
pub fn payload_hint_text(hint: &str, locale: Locale) -> String {
    if locale == Locale::En {
        return hint.to_owned();
    }
    HINTS.iter().find(|(en, _)| *en == hint).map_or_else(|| hint.to_owned(), |(_, nl)| (*nl).to_owned())
}
