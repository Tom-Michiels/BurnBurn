//! X04: memory pressure. The undo history has a byte budget (an estimate of the memory the steps
//! retain). When it is exceeded the OLDEST steps are forgotten and the commit reports it with a
//! typed warning; the document itself is never touched and the newest step stays undoable.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_document::{
    CommandOutcome, CommandPayload, CommandWarning, DocumentCommand, DocumentSession, DocumentSnapshot,
    GeometryCommand, HistoryBudget, ImportItem,
};
use burnburn_geometry::{Affine, Path, Point, Vector};
use burnburn_types::{ContentHash, DocumentId, GeometryId, RequestId, StableId};

fn run(s: &mut DocumentSession, payload: CommandPayload) -> CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

fn session() -> (DocumentSession, Vec<GeometryId>) {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let items: Vec<ImportItem> = (0..200)
        .map(|i| {
            let pts: Vec<Point> = (0..40).map(|k| Point::new(i as f64 + (k as f64).cos(), (k as f64).sin())).collect();
            ImportItem { name: format!("o{i}"), entity: None, path: Path::polyline(&pts, true).unwrap() }
        })
        .collect();
    let asset = ContentHash::from_bytes([1; 32]);
    let ids = run(&mut s, CommandPayload::Edit(GeometryCommand::Import { asset, items })).changes.created;
    (s, ids)
}

fn nudge(s: &mut DocumentSession, ids: &[GeometryId], dx: f64) -> CommandOutcome {
    let m = Affine::translate(Vector::new(dx, 0.0)).unwrap();
    run(s, CommandPayload::Transform { targets: ids.to_vec(), transform: m })
}

fn content(snap: &DocumentSnapshot) -> Vec<(GeometryId, Path)> {
    snap.objects().map(|(id, o)| (id, (**o.path()).clone())).collect()
}

fn trimmed(out: &CommandOutcome) -> usize {
    out.warnings
        .iter()
        .map(|w| match w {
            CommandWarning::HistoryTrimmed { dropped_steps } => *dropped_steps,
            _ => 0,
        })
        .sum()
}

#[test]
fn history_is_bounded_by_the_budget_without_touching_the_document() {
    let (mut limited, ids) = session();
    let (mut free, _) = session();
    let step = {
        let before = limited.history_bytes();
        nudge(&mut limited, &ids, 1.0);
        nudge(&mut free, &ids, 1.0);
        limited.history_bytes() - before
    };
    assert!(step > 200 * 40 * 16, "one step must be estimated at least at the size of the new paths: {step}");
    limited.set_history_budget(HistoryBudget { max_bytes: limited.history_bytes() + 3 * step });

    let mut dropped = 0;
    for k in 2..40 {
        let a = nudge(&mut limited, &ids, f64::from(k));
        nudge(&mut free, &ids, f64::from(k));
        dropped += trimmed(&a);
        assert!(limited.history_bytes() <= limited.history_budget().max_bytes, "step {k}");
    }
    assert!(dropped > 30, "most of the 40 steps had to be forgotten ({dropped})");
    assert_eq!(content(&limited.snapshot()), content(&free.snapshot()), "the document is identical");
    let kept = limited.undo_labels().len();
    assert!((3..=6).contains(&kept), "a few recent steps remain undoable: {kept}");
    assert_eq!(free.undo_labels().len(), 40, "without a budget nothing is forgotten");

    // Every kept step can be undone, in order; then the history is exhausted.
    for _ in 0..kept {
        let rev = limited.revision();
        limited.undo(rev).unwrap();
    }
    assert!(!limited.can_undo());
}

#[test]
fn the_newest_step_stays_undoable_even_with_a_zero_budget() {
    let (mut s, ids) = session();
    s.set_history_budget(HistoryBudget { max_bytes: 0 });
    let out = nudge(&mut s, &ids, 5.0);
    assert_eq!(s.undo_labels().len(), 1);
    assert!(trimmed(&out) >= 1, "the import step was forgotten, and said so");
    let rev = s.revision();
    s.undo(rev).unwrap();
    assert!(!s.can_undo());
}

#[test]
fn the_default_budget_does_not_trim_ordinary_documents() {
    let (mut s, ids) = session();
    for k in 0..30 {
        assert_eq!(trimmed(&nudge(&mut s, &ids, f64::from(k))), 0);
    }
    assert_eq!(s.undo_labels().len(), 31);
}
