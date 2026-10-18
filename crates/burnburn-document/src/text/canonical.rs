//! Canonical form of a text source (ADR 0004): used for the project hash (io) and as the base
//! of the evaluator inputs hash (`burnburn-text` appends the resolved font bytes' hashes).

use burnburn_types::{CanonicalEncode, CanonicalWriter};

use super::{Align, TextMode, TextObject};

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
    }
}
