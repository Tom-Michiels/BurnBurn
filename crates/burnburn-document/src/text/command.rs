//! Text commands: one document transaction each, through `CommandPayload::Text`.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use burnburn_types::IdAllocator;

use super::{MissingGlyph, TextId, TextObject};
use crate::patch::{DerivedOwner, DerivedRecord};

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TextCommand {
    /// Adds a text object; its outlines follow from the evaluator in the same transaction.
    Create { text: Box<TextObject> },
    /// Replaces the source of a text object (content, font chain, size, layout, placement, mode).
    /// Committed text only: IME composition never reaches the document.
    Set { text: TextId, source: Box<TextObject> },
    /// Removes the text object and its derived outlines.
    Delete { text: TextId },
}

/// Undo label for text commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TextLabel {
    Create,
    Edit,
    Delete,
}

impl TextCommand {
    pub fn label(&self) -> TextLabel {
        match self {
            TextCommand::Create { .. } => TextLabel::Create,
            TextCommand::Set { .. } => TextLabel::Edit,
            TextCommand::Delete { .. } => TextLabel::Delete,
        }
    }
}

/// Why a text edit was refused. The document is unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TextError {
    UnknownText(TextId),
    /// Line spacing must be finite and positive, letter spacing finite.
    InvalidLayout,
    /// A stored record belongs to another owner or to a text that is not in the layer.
    InvalidRecord,
    IdsExhausted,
}

impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextError::UnknownText(t) => write!(f, "no text object {t}"),
            TextError::InvalidLayout => f.write_str("text layout values are out of range"),
            TextError::InvalidRecord => f.write_str("text evaluation record does not match a text object"),
            TextError::IdsExhausted => f.write_str("text ids exhausted"),
        }
    }
}

impl std::error::Error for TextError {}

/// All text objects of a document, plus the derived-output bookkeeping per owner (§11).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextLayer {
    pub(crate) texts: BTreeMap<TextId, Arc<TextObject>>,
    pub(crate) derived: BTreeMap<DerivedOwner, DerivedRecord>,
    /// Missing glyphs of the last valid evaluation, per text (only non-empty lists).
    pub(crate) missing: BTreeMap<TextId, Vec<MissingGlyph>>,
}

fn check(obj: &TextObject) -> Result<(), TextError> {
    let l = &obj.layout;
    if l.line_spacing.is_finite() && l.line_spacing > 0.0 && l.letter_spacing_mm.is_finite() {
        Ok(())
    } else {
        Err(TextError::InvalidLayout)
    }
}

impl TextLayer {
    pub fn new() -> Self {
        TextLayer::default()
    }

    pub fn text(&self, id: TextId) -> Option<&TextObject> {
        self.texts.get(&id).map(|t| t.as_ref())
    }

    pub fn texts(&self) -> impl ExactSizeIterator<Item = (TextId, &TextObject)> {
        self.texts.iter().map(|(k, v)| (*k, v.as_ref()))
    }

    pub fn record(&self, owner: DerivedOwner) -> Option<&DerivedRecord> {
        self.derived.get(&owner)
    }

    pub fn records(&self) -> impl ExactSizeIterator<Item = (DerivedOwner, &DerivedRecord)> {
        self.derived.iter().map(|(k, v)| (*k, v))
    }

    pub fn is_empty(&self) -> bool {
        self.texts.is_empty()
    }

    /// Characters the last valid evaluation of `id` could not draw (empty when all are drawn).
    pub fn missing(&self, id: TextId) -> &[MissingGlyph] {
        self.missing.get(&id).map_or(&[], |m| m.as_slice())
    }

    /// Stored layer (open/recovery). Records must belong to texts of the layer.
    pub fn from_parts(
        texts: Vec<(TextId, TextObject)>,
        records: Vec<(DerivedOwner, DerivedRecord)>,
    ) -> Result<Self, TextError> {
        let mut layer = TextLayer::new();
        for (id, t) in texts {
            check(&t)?;
            if layer.texts.insert(id, Arc::new(t)).is_some() {
                return Err(TextError::UnknownText(id));
            }
        }
        for (owner, r) in records {
            match owner {
                DerivedOwner::Text(id) if layer.texts.contains_key(&id) => {
                    layer.derived.insert(owner, r);
                }
                _ => return Err(TextError::InvalidRecord),
            }
        }
        Ok(layer)
    }
}

/// What a text command may touch.
pub(crate) struct TextTxn<'a> {
    pub layer: &'a mut TextLayer,
    pub ids: &'a mut IdAllocator,
    pub changed: &'a mut Vec<TextId>,
}

/// What the session must do after a text command.
pub(crate) enum TextEffect {
    None,
    /// The text is gone: remove its derived outputs.
    Deleted(TextId),
}

pub(crate) fn apply(cmd: TextCommand, txn: TextTxn<'_>) -> Result<TextEffect, TextError> {
    let TextTxn { layer, ids, changed } = txn;
    match cmd {
        TextCommand::Create { text } => {
            check(&text)?;
            let id: TextId = ids.allocate().map_err(|_| TextError::IdsExhausted)?;
            layer.texts.insert(id, Arc::from(text));
            changed.push(id);
            Ok(TextEffect::None)
        }
        TextCommand::Set { text, source } => {
            check(&source)?;
            let slot = layer.texts.get_mut(&text).ok_or(TextError::UnknownText(text))?;
            *slot = Arc::from(source);
            if !changed.contains(&text) {
                changed.push(text);
            }
            Ok(TextEffect::None)
        }
        TextCommand::Delete { text } => {
            layer.texts.remove(&text).ok_or(TextError::UnknownText(text))?;
            layer.derived.remove(&DerivedOwner::Text(text));
            layer.missing.remove(&text);
            changed.push(text);
            Ok(TextEffect::Deleted(text))
        }
    }
}
