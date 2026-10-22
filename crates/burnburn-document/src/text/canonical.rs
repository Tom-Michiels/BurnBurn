//! Canonical form of a text source (ADR 0004): used for the project hash (io) and as the base
//! of the evaluator inputs hash (`burnburn-text` appends the resolved font bytes' hashes).

use burnburn_types::{CanonicalEncode, CanonicalWriter};

use super::{Align, TextMode, TextObject, VAlign};

fn font_ref(r: &super::FontRef, out: &mut CanonicalWriter) {
    out.str(&r.family);
    out.tag(r.style.tag());
    match r.asset {
        None => out.u64(0),
        Some(h) => {
            out.u64(1);
            out.bytes(h.as_bytes());
        }
    }
}

impl CanonicalEncode for TextObject {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.str(&self.content);
        font_ref(&self.font, out);
        out.u64(self.fallback.len() as u64);
        for r in &self.fallback {
            font_ref(r, out);
        }
        out.f64(self.size.em_mm());
        out.tag(match self.layout.align {
            Align::Left => "left",
            Align::Center => "center",
            Align::Right => "right",
            Align::Justify => "justify",
        });
        out.f64(self.layout.line_spacing);
        out.f64(self.layout.letter_spacing_mm);
        out.bool(self.layout.kerning);
        out.bool(self.layout.ligatures);
        for c in self.placement.coefficients() {
            out.f64(c);
        }
        out.tag(match self.mode {
            TextMode::Outline => "outline",
            TextMode::SingleLine => "single-line",
        });
        // Options added later are written only when set, so text that does not use them keeps the
        // hash (and the project hash) it always had.
        let l = &self.layout;
        if l.uppercase || l.valign != VAlign::Baseline || l.v_space_mm != 0.0 || l.welded {
            out.tag("layout-options");
            out.bool(l.uppercase);
            out.tag(match l.valign {
                VAlign::Baseline => "baseline",
                VAlign::Top => "top",
                VAlign::Middle => "middle",
                VAlign::Bottom => "bottom",
            });
            out.f64(l.v_space_mm);
            out.bool(l.welded);
        }
        if let Some(v) = self.variable {
            out.tag("variable-text");
            out.u64(u64::from_le_bytes(v.offset.to_le_bytes()));
        }
        if let Some(p) = self.path {
            out.tag("path-binding");
            out.u64(burnburn_types::StableId::raw(p.guide));
            out.f64(p.start_mm);
            out.f64(p.offset_mm);
            out.bool(p.reverse);
            out.bool(p.warp);
        }
    }
}
