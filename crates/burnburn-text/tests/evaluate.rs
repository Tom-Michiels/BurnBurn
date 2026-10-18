//! Text in the document (DR-22): the source is document data, `TextEvaluator` delivers the
//! outlines in the same transaction, undo restores both, and a missing font keeps the last valid
//! outlines with a `FontMissing` diagnosis (contracts-phase-c §IM18, contract §11).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use burnburn_document::text::{TextCommand, TextId};
use burnburn_document::{
    CommandPayload, DerivedOwner, DocumentCommand, DocumentSession, EvalDiagnosis, EvalStatus, GeometrySource,
};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{
    FontData, FontRef, FontSize, FontStyle, MemoryFontSource, TextEvaluator, TextMode, TextObject, inputs_hash, shape,
};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};

fn font(name: &str) -> Arc<FontData> {
    Arc::new(FontData::from_bytes(read_fixture("fonts", name).unwrap()).unwrap())
}

fn setup() -> (MemoryFontSource, FontRef, TextObject) {
    let mut src = MemoryFontSource::new();
    let r = src.insert("Liberation Sans", FontStyle::Regular, font("LiberationSans-Regular.ttf"));
    let obj = TextObject::new("Naam\n\nPlaat 7", r.clone(), FontSize::from_mm(Length::from_mm(10.0).unwrap()).unwrap());
    (src, r, obj)
}

fn cmd(s: &DocumentSession, c: TextCommand) -> DocumentCommand {
    DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(1),
        payload: CommandPayload::Text(c),
    }
}

/// Derived objects of `text`, by output key.
fn outputs(s: &DocumentSession, text: TextId) -> Vec<(u64, GeometryId, burnburn_geometry::Path)> {
    let snap = s.snapshot();
    snap.objects()
        .filter_map(|(id, o)| match o.source() {
            GeometrySource::Derived(k) if k.owner == DerivedOwner::Text(text) => {
                Some((k.local, id, (**o.path()).clone()))
            }
            _ => None,
        })
        .collect()
}

fn create(s: &mut DocumentSession, ev: &TextEvaluator<MemoryFontSource>, obj: &TextObject) -> TextId {
    let out = s.apply_with(cmd(s, TextCommand::Create { text: Box::new(obj.clone()) }), &[ev]).unwrap();
    *out.changes.texts.first().unwrap()
}

#[test]
fn text_source_is_document_data_and_outlines_follow_in_the_same_transaction() {
    let (src, _, obj) = setup();
    let shaped = shape(&obj, &src).unwrap();
    let ev = TextEvaluator::new(src);
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let id = create(&mut s, &ev, &obj);
    assert_eq!(s.snapshot().text(id), Some(&obj));
    let out = outputs(&s, id);
    assert_eq!(out.iter().map(|o| o.0).collect::<Vec<_>>(), [0, 2], "one object per non-empty line");
    assert_eq!(out[0].2, shaped.line_outlines[0]);
    assert_eq!(out[1].2, shaped.line_outlines[2]);
    let record = s.snapshot().derived_record(DerivedOwner::Text(id)).cloned().unwrap();
    assert_eq!(record.status, EvalStatus::Valid);
    assert_eq!(record.inputs, inputs_hash(&obj, ev.fonts()));
    assert!(!s.snapshot().production_blocked(out[0].1));
}

#[test]
fn editing_text_keeps_object_ids_and_undo_restores_source_and_outlines() {
    let (src, _, obj) = setup();
    let ev = TextEvaluator::new(src);
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let id = create(&mut s, &ev, &obj);
    let before = outputs(&s, id);

    let mut edited = obj.clone();
    edited.content = "Naam!\n\nPlaat 8".into();
    let rev = s.revision();
    s.apply_with(cmd(&s, TextCommand::Set { text: id, source: Box::new(edited.clone()) }), &[&ev]).unwrap();
    assert_eq!(s.revision(), rev.next().unwrap(), "one revision for source + outlines");
    let after = outputs(&s, id);
    assert_eq!(after.iter().map(|o| o.1).collect::<Vec<_>>(), before.iter().map(|o| o.1).collect::<Vec<_>>());
    assert_ne!(after[0].2, before[0].2);

    s.undo(s.revision()).unwrap();
    assert_eq!(s.snapshot().text(id), Some(&obj));
    assert_eq!(outputs(&s, id), before);
    s.redo(s.revision()).unwrap();
    assert_eq!(s.snapshot().text(id), Some(&edited));
    assert_eq!(outputs(&s, id), after);

    s.apply_with(cmd(&s, TextCommand::Delete { text: id }), &[&ev]).unwrap();
    assert!(outputs(&s, id).is_empty() && s.snapshot().text(id).is_none());
    s.undo(s.revision()).unwrap();
    assert_eq!(outputs(&s, id), after);
}

#[test]
fn missing_font_on_reopen_keeps_last_outlines_and_blocks_production() {
    let (src, r, obj) = setup();
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let id = create(&mut s, &TextEvaluator::new(src), &obj);
    let before = outputs(&s, id);

    // Reopened on a machine without the font: re-evaluation (here: a letter-spacing edit) fails.
    let reopened = TextEvaluator::new(MemoryFontSource::new());
    let mut edited = obj.clone();
    edited.layout.letter_spacing_mm = 1.0;
    s.apply_with(cmd(&s, TextCommand::Set { text: id, source: Box::new(edited.clone()) }), &[&reopened]).unwrap();
    assert_eq!(s.snapshot().text(id), Some(&edited), "the definition is what the user asked for");
    assert_eq!(outputs(&s, id), before, "last valid outlines stay");
    let record = s.snapshot().derived_record(DerivedOwner::Text(id)).cloned().unwrap();
    assert_eq!(record.status, EvalStatus::Failed { diagnosis: EvalDiagnosis::FontMissing { font: r } });
    assert!(before.iter().all(|o| s.snapshot().production_blocked(o.1)), "stale outlines never reach a job");
}

#[test]
fn missing_glyphs_are_valid_outlines_plus_a_reported_list() {
    let (src, _, mut obj) = setup();
    obj.content = "A日".into();
    let ev = TextEvaluator::new(src);
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let id = create(&mut s, &ev, &obj);
    let snap = s.snapshot();
    assert_eq!(snap.derived_record(DerivedOwner::Text(id)).unwrap().status, EvalStatus::Valid);
    assert_eq!(snap.text_layer().missing(id).iter().map(|m| m.ch).collect::<String>(), "日");
    assert_eq!(outputs(&s, id).len(), 1);
}

#[test]
fn invalid_layout_is_refused_by_the_document() {
    let (src, _, mut obj) = setup();
    let ev = TextEvaluator::new(src);
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    obj.layout.line_spacing = -1.0;
    let rev = s.revision();
    assert!(s.apply_with(cmd(&s, TextCommand::Create { text: Box::new(obj) }), &[&ev]).is_err());
    assert_eq!(s.revision(), rev);
}

#[test]
fn inputs_hash_covers_source_layout_and_font_bytes() {
    let (src, r, obj) = setup();
    let base = inputs_hash(&obj, &src).unwrap();
    assert_eq!(inputs_hash(&obj.clone(), &src), Some(base), "deterministic");

    let mut changed = Vec::new();
    let mut o = obj.clone();
    o.content.push('!');
    changed.push(o);
    let mut o = obj.clone();
    o.layout.letter_spacing_mm = 0.5;
    changed.push(o);
    let mut o = obj.clone();
    o.layout.kerning = false;
    changed.push(o);
    let mut o = obj.clone();
    o.size = FontSize::from_points(12.0).unwrap();
    changed.push(o);
    let mut o = obj.clone();
    o.mode = TextMode::SingleLine;
    changed.push(o);
    for o in &changed {
        assert_ne!(inputs_hash(o, &src), Some(base), "{o:?}");
    }

    // Same family name, other bytes: a by-name reference resolves to different outlines, so the
    // hash must differ even though the stored FontRef is identical.
    let mut o = obj.clone();
    o.font = FontRef::new("Liberation Sans", FontStyle::Regular);
    let mut other = MemoryFontSource::new();
    other.insert("Liberation Sans", FontStyle::Regular, font("BIZUDPGothic-Regular.ttf"));
    assert_ne!(inputs_hash(&o, &src), inputs_hash(&o, &other));
    assert!(r.asset.is_some());
    assert_eq!(inputs_hash(&obj, &MemoryFontSource::new()), None);
}
