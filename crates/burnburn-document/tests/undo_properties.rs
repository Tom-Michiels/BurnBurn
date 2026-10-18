//! Property: arbitrary command/undo/redo sequences match an independent model.
//!
//! The model tracks axis-aligned rectangles analytically (min corner, width, height) and a
//! plain stack-of-states history. It never calls the document's own geometry code.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use std::collections::{BTreeMap, BTreeSet};

use burnburn_document::{Anchor, Axis, CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Affine, Point, Rect, Vector};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum Op {
    Create { w: u16, h: u16 },
    Resize { pick: usize, w: u16 },
    Move { pick: usize, dx: i16 },
    Rename { pick: usize, tag: u8 },
    Delete { pick: usize },
    Undo,
    Redo,
    StaleRename { pick: usize },
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (1u16..500, 1u16..500).prop_map(|(w, h)| Op::Create { w, h }),
        (any::<usize>(), 1u16..500).prop_map(|(pick, w)| Op::Resize { pick, w }),
        (any::<usize>(), any::<i16>()).prop_map(|(pick, dx)| Op::Move { pick, dx }),
        (any::<usize>(), any::<u8>()).prop_map(|(pick, tag)| Op::Rename { pick, tag }),
        any::<usize>().prop_map(|pick| Op::Delete { pick }),
        Just(Op::Undo),
        Just(Op::Undo),
        Just(Op::Redo),
        any::<usize>().prop_map(|pick| Op::StaleRename { pick }),
    ]
}

/// (name, min x, min y, width, height) in mm.
type ModelObj = (String, f64, f64, f64, f64);
type Model = BTreeMap<u64, ModelObj>;

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn assert_matches(s: &DocumentSession, model: &Model) {
    let snap = s.snapshot();
    let ids: Vec<u64> = snap.objects().map(|(id, _)| id.raw()).collect();
    assert_eq!(ids, model.keys().copied().collect::<Vec<_>>());
    for (id, (name, x, y, w, h)) in model {
        let obj = snap.object(GeometryId::from_raw(*id)).unwrap();
        assert_eq!(obj.name(), name);
        let b = obj.bounds().unwrap();
        let close = |a: f64, b: f64| (a - b).abs() <= 1e-9 * (1.0 + b.abs());
        assert!(close(b.min().x, *x) && close(b.min().y, *y), "{id}: {b:?} vs ({x}, {y})");
        assert!(close(b.width(), *w) && close(b.height(), *h), "{id}: {b:?} vs {w} x {h}");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn random_undo_redo_sequences_match_model(ops in proptest::collection::vec(op(), 1..40)) {
        let mut s = DocumentSession::new(DocumentId::from_raw(7));
        let mut past: Vec<Model> = Vec::new();
        let mut future: Vec<Model> = Vec::new();
        let mut model = Model::new();
        let mut next_id = 1u64;
        let mut issued = BTreeSet::new();

        for op in ops {
            let before_rev = s.revision();
            let pick = |p: usize, m: &Model| m.keys().nth(p % m.len().max(1)).copied();
            let mk = |payload| DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
            let mut next_model = None;
            let result = match op {
                Op::Create { w, h } => {
                    let rect = Rect::new(Point::new(0.0, 0.0), mm(f64::from(w)), mm(f64::from(h))).unwrap();
                    let r = s.apply(mk(CommandPayload::CreateRectangle { rect, name: String::new() }));
                    if let Ok(out) = &r {
                        let id = out.changes.created[0].raw();
                        prop_assert_eq!(id, next_id, "deterministic, monotone ids");
                        prop_assert!(issued.insert(id), "id {} reused", id);
                        next_id += 1;
                        let mut m = model.clone();
                        m.insert(id, (String::new(), 0.0, 0.0, f64::from(w), f64::from(h)));
                        next_model = Some(m);
                    }
                    r.map(|_| ())
                }
                Op::Resize { pick: p, w } => match pick(p, &model) {
                    None => continue,
                    Some(id) => {
                        let payload = CommandPayload::SetDimension { target: GeometryId::from_raw(id), axis: Axis::Width, value: mm(f64::from(w)), anchor: Anchor::Min };
                        let mut m = model.clone();
                        if let Some(o) = m.get_mut(&id) { o.3 = f64::from(w); }
                        next_model = Some(m);
                        s.apply(mk(payload)).map(|_| ())
                    }
                },
                Op::Move { pick: p, dx } => match pick(p, &model) {
                    None => continue,
                    Some(id) => {
                        let t = Affine::translate(Vector::new(f64::from(dx), 0.0)).unwrap();
                        let mut m = model.clone();
                        if let Some(o) = m.get_mut(&id) { o.1 += f64::from(dx); }
                        next_model = Some(m);
                        s.apply(mk(CommandPayload::Transform { targets: vec![GeometryId::from_raw(id)], transform: t })).map(|_| ())
                    }
                },
                Op::Rename { pick: p, tag } => match pick(p, &model) {
                    None => continue,
                    Some(id) => {
                        let name = format!("n{tag}");
                        let mut m = model.clone();
                        if let Some(o) = m.get_mut(&id) { o.0 = name.clone(); }
                        next_model = Some(m);
                        s.apply(mk(CommandPayload::Rename { target: GeometryId::from_raw(id), name })).map(|_| ())
                    }
                },
                Op::Delete { pick: p } => match pick(p, &model) {
                    None => continue,
                    Some(id) => {
                        let mut m = model.clone();
                        m.remove(&id);
                        next_model = Some(m);
                        s.apply(mk(CommandPayload::Delete { targets: vec![GeometryId::from_raw(id)] })).map(|_| ())
                    }
                },
                Op::Undo => {
                    let r = s.undo(s.revision());
                    prop_assert_eq!(r.is_ok(), !past.is_empty());
                    if let Some(prev) = past.pop() {
                        future.push(std::mem::replace(&mut model, prev));
                    }
                    r.map(|_| ())
                }
                Op::Redo => {
                    let r = s.redo(s.revision());
                    prop_assert_eq!(r.is_ok(), !future.is_empty());
                    if let Some(next) = future.pop() {
                        past.push(std::mem::replace(&mut model, next));
                    }
                    r.map(|_| ())
                }
                Op::StaleRename { pick: p } => match pick(p, &model) {
                    None => continue,
                    Some(id) => {
                        let mut c = mk(CommandPayload::Rename { target: GeometryId::from_raw(id), name: "stale".into() });
                        c.expected = burnburn_types::RevisionId::INITIAL;
                        let r = s.apply(c);
                        prop_assert_eq!(r.is_err(), before_rev != burnburn_types::RevisionId::INITIAL);
                        if r.is_ok() {
                            let mut m = model.clone();
                            if let Some(o) = m.get_mut(&id) { o.0 = "stale".into(); }
                            next_model = Some(m);
                        }
                        r.map(|_| ())
                    }
                },
            };
            if let Some(m) = next_model.filter(|_| result.is_ok()) {
                past.push(std::mem::replace(&mut model, m));
                future.clear();
            }
            if result.is_ok() {
                prop_assert!(s.revision() > before_rev, "every committed change is a newer revision");
            } else {
                prop_assert_eq!(s.revision(), before_rev, "a failed step changes no revision");
            }
            assert_matches(&s, &model);
        }
    }
}
