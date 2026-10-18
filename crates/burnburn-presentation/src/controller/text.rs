//! Text input: fonts, `InsertText` and the shared font source of every session's evaluator.

use std::sync::Arc;

use burnburn_document::text::{FontSize, TextCommand, TextId, TextObject};
use burnburn_document::{CommandPayload, DerivedOwner, GeometrySource};
use burnburn_types::{DocumentId, Length, RequestId};

use burnburn_text::{FontData, FontError, FontRef, FontSource};

use crate::view_model::MessageKey;

use super::{AppController, AppUpdate, DEFAULT_TEXT_MM, apply_edit, error};

impl AppController {
    /// Fonts for text input (`UiIntent::InsertText`) and the face new text uses.
    pub fn attach_fonts(&mut self, fonts: Arc<dyn FontSource + Send + Sync>, default_font: FontRef) {
        let shared = SharedFonts(fonts);
        for s in self.sessions.values_mut() {
            s.fonts = Some(shared.clone());
        }
        self.fonts = Some((shared, default_font));
    }
}

impl AppController {
    /// One committed text: extends the selected text object, or creates one at the origin.
    pub(super) fn insert_text(&mut self, document: DocumentId, text: String) -> AppUpdate {
        if text.is_empty() {
            return AppUpdate::default();
        }
        let Some((_, font)) = self.fonts.clone() else { return error(Some(document), MessageKey::NoFontAvailable) };
        let request = self.next_id::<RequestId>();
        self.with_session(document, |s| {
            let snapshot = s.doc.snapshot();
            let selected: Vec<TextId> = s
                .selection
                .ids()
                .iter()
                .filter_map(|id| match snapshot.object(*id).map(|o| o.source()) {
                    Some(GeometrySource::Derived(k)) => match k.owner {
                        DerivedOwner::Text(t) => Some(t),
                        _ => None,
                    },
                    _ => None,
                })
                .collect();
            let edit = match selected.as_slice() {
                [first, rest @ ..] if rest.iter().all(|t| t == first) => {
                    let Some(current) = snapshot.text(*first) else { return AppUpdate::default() };
                    let mut source = current.clone();
                    source.content.push_str(&text);
                    TextCommand::Set { text: *first, source: Box::new(source) }
                }
                _ => {
                    let Ok(size) = Length::from_mm(DEFAULT_TEXT_MM)
                        .map_err(|_| ())
                        .and_then(|l| FontSize::from_mm(l).map_err(|_| ()))
                    else {
                        return error(Some(document), MessageKey::CommandRejected);
                    };
                    TextCommand::Create { text: Box::new(TextObject::new(text, font, size)) }
                }
            };
            apply_edit(s, document, CommandPayload::Text(edit), request)
        })
    }

    /// Content of every text object of `document` (tests and accessibility), in id order.
    pub fn text_contents(&self, document: DocumentId) -> Vec<String> {
        self.sessions
            .get(&document)
            .map_or_else(Vec::new, |s| s.doc.snapshot().text_layer().texts().map(|(_, t)| t.content.clone()).collect())
    }
}

/// The attached font source, shared by every session's text evaluator.
#[derive(Clone)]
pub(super) struct SharedFonts(Arc<dyn FontSource + Send + Sync>);

impl FontSource for SharedFonts {
    fn load(&self, font: &FontRef) -> Result<Arc<FontData>, FontError> {
        self.0.load(font)
    }
}
