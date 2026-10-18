//! Text DTO ↔ document text layer. Loading validates numbers and goes through
//! `TextLayer::from_parts` / `DocumentSnapshot::with_texts`.

use burnburn_document::text::{
    Align, FontRef, FontSize, FontStyle, TextId, TextLayer, TextLayout, TextMode, TextObject,
};
use burnburn_document::{DerivedOwner, DerivedRecord, DocumentSnapshot, EvalDiagnosis, EvalStatus};
use burnburn_geometry::Affine;
use burnburn_types::{ContentHash, Length, StableId};

use crate::LoadError;
use crate::dto::v1::Num;
use crate::map::{asset_key, parse_asset_key};
use crate::text_dto as d;

fn font_to(r: &FontRef) -> d::Font {
    d::Font {
        family: r.family.clone(),
        style: match r.style {
            FontStyle::Regular => d::Style::Regular,
            FontStyle::Bold => d::Style::Bold,
            FontStyle::Italic => d::Style::Italic,
            FontStyle::BoldItalic => d::Style::BoldItalic,
        },
        asset: r.asset.as_ref().map(asset_key),
    }
}

fn status_to(s: &EvalStatus) -> d::Status {
    match s {
        EvalStatus::Valid => d::Status::Valid,
        EvalStatus::Failed { diagnosis: EvalDiagnosis::FontMissing { font } } => {
            d::Status::FontMissing { font: font_to(font) }
        }
        // Other failures are stored as "definition invalid": still blocked, re-evaluated.
        EvalStatus::Failed { .. } => d::Status::InvalidDefinition,
        _ => d::Status::Pending,
    }
}

pub(crate) fn texts_to_dto(doc: &DocumentSnapshot) -> Vec<d::Text> {
    let layer = doc.text_layer();
    layer
        .texts()
        .map(|(id, t)| {
            let c = t.placement.coefficients();
            d::Text {
                id: id.raw(),
                content: t.content.clone(),
                font: font_to(&t.font),
                fallback: t.fallback.iter().map(font_to).collect(),
                em_mm: Num(t.size.em_mm()),
                align: match t.layout.align {
                    Align::Left => d::Align::Left,
                    Align::Center => d::Align::Center,
                    Align::Right => d::Align::Right,
                },
                line_spacing: Num(t.layout.line_spacing),
                letter_spacing_mm: Num(t.layout.letter_spacing_mm),
                kerning: t.layout.kerning,
                ligatures: t.layout.ligatures,
                placement: c.map(Num),
                mode: match t.mode {
                    TextMode::Outline => d::Mode::Outline,
                    TextMode::SingleLine => d::Mode::SingleLine,
                },
                evaluation: layer
                    .record(DerivedOwner::Text(id))
                    .map(|r| d::Evaluation { inputs: r.inputs.map(|h| h.to_hex()), status: status_to(&r.status) }),
            }
        })
        .collect()
}

fn invalid(path: String, reason: impl Into<String>) -> LoadError {
    LoadError::InvalidValue { path, reason: reason.into() }
}

fn font_from(path: &dyn Fn() -> String, f: &d::Font) -> Result<FontRef, LoadError> {
    let style = match f.style {
        d::Style::Regular => FontStyle::Regular,
        d::Style::Bold => FontStyle::Bold,
        d::Style::Italic => FontStyle::Italic,
        d::Style::BoldItalic => FontStyle::BoldItalic,
    };
    let asset = match &f.asset {
        None => None,
        Some(key) => Some(parse_asset_key(|| format!("{}.asset", path()), key)?),
    };
    Ok(FontRef { family: f.family.clone(), style, asset })
}

fn finite(path: String, n: Num) -> Result<f64, LoadError> {
    if n.0.is_finite() { Ok(n.0) } else { Err(invalid(path, "not a finite number")) }
}

fn text_from(ti: usize, t: &d::Text) -> Result<TextObject, LoadError> {
    let p = |f: &str| format!("texts[{ti}].{f}");
    let font = font_from(&|| p("font"), &t.font)?;
    let fallback = t
        .fallback
        .iter()
        .enumerate()
        .map(|(i, f)| font_from(&|| p(&format!("fallback[{i}]")), f))
        .collect::<Result<Vec<_>, _>>()?;
    let em = Length::from_mm(finite(p("em_mm"), t.em_mm)?).map_err(|e| invalid(p("em_mm"), e.to_string()))?;
    let size = FontSize::from_mm(em).map_err(|e| invalid(p("em_mm"), e.to_string()))?;
    let mut coeffs = [0.0; 6];
    for (i, n) in t.placement.iter().enumerate() {
        coeffs[i] = finite(p(&format!("placement[{i}]")), *n)?;
    }
    let placement = Affine::new(coeffs).map_err(|e| invalid(p("placement"), e.to_string()))?;
    let layout = TextLayout {
        align: match t.align {
            d::Align::Left => Align::Left,
            d::Align::Center => Align::Center,
            d::Align::Right => Align::Right,
        },
        line_spacing: finite(p("line_spacing"), t.line_spacing)?,
        letter_spacing_mm: finite(p("letter_spacing_mm"), t.letter_spacing_mm)?,
        kerning: t.kerning,
        ligatures: t.ligatures,
    };
    let mut obj = TextObject::new(t.content.clone(), font, size);
    obj.fallback = fallback;
    obj.layout = layout;
    obj.placement = placement;
    obj.mode = match t.mode {
        d::Mode::Outline => TextMode::Outline,
        d::Mode::SingleLine => TextMode::SingleLine,
    };
    Ok(obj)
}

pub(crate) fn texts_from_dto(texts: &[d::Text]) -> Result<TextLayer, LoadError> {
    let mut objs = Vec::with_capacity(texts.len());
    let mut records = Vec::new();
    for (ti, t) in texts.iter().enumerate() {
        let id = TextId::from_raw(t.id);
        objs.push((id, text_from(ti, t)?));
        if let Some(ev) = &t.evaluation {
            let inputs = match &ev.inputs {
                None => None,
                Some(hex) => Some(
                    ContentHash::from_hex(hex)
                        .map_err(|e| invalid(format!("texts[{ti}].evaluation.inputs"), e.to_string()))?,
                ),
            };
            let status = match &ev.status {
                d::Status::Valid => EvalStatus::Valid,
                d::Status::Pending => EvalStatus::Pending,
                d::Status::InvalidDefinition => EvalStatus::Failed { diagnosis: EvalDiagnosis::InvalidDefinition },
                d::Status::FontMissing { font } => EvalStatus::Failed {
                    diagnosis: EvalDiagnosis::FontMissing {
                        font: font_from(&|| format!("texts[{ti}].evaluation.status.font"), font)?,
                    },
                },
            };
            records.push((DerivedOwner::Text(id), DerivedRecord { inputs, status }));
        }
    }
    TextLayer::from_parts(objs, records).map_err(|e| invalid("texts".into(), e.to_string()))
}
