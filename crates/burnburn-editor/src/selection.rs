use std::cmp::Reverse;

use burnburn_document::DocumentSnapshot;
use burnburn_geometry::{Bounds, HitTestTolerance, Point, SpatialIndex};
use burnburn_types::{GeometryId, Length};

/// The selection of one window/document session. Never document data and never shared.
/// Order is selection order: the first id is the align/distribute reference (E04).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Selection {
    ids: Vec<GeometryId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionChange {
    Replace(Vec<GeometryId>),
    Toggle(GeometryId),
    /// Adds ids that are not selected yet (Shift + rubber band).
    Extend(Vec<GeometryId>),
    Clear,
}

impl Selection {
    pub fn ids(&self) -> &[GeometryId] {
        &self.ids
    }

    pub fn contains(&self, id: GeometryId) -> bool {
        self.ids.contains(&id)
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// The single selected object, if exactly one is selected (drives the inspector).
    pub fn single(&self) -> Option<GeometryId> {
        match self.ids.as_slice() {
            [one] => Some(*one),
            _ => None,
        }
    }

    /// The reference object for align/distribute: the first one selected.
    pub fn reference(&self) -> Option<GeometryId> {
        self.ids.first().copied()
    }

    pub fn apply(&mut self, change: &SelectionChange) {
        match change {
            SelectionChange::Replace(ids) => {
                self.ids.clear();
                self.extend(ids);
            }
            SelectionChange::Extend(ids) => self.extend(ids),
            SelectionChange::Toggle(id) => {
                if let Some(pos) = self.ids.iter().position(|x| x == id) {
                    self.ids.remove(pos);
                } else {
                    self.ids.push(*id);
                }
            }
            SelectionChange::Clear => self.ids.clear(),
        }
    }

    fn extend(&mut self, ids: &[GeometryId]) {
        for id in ids {
            if !self.ids.contains(id) {
                self.ids.push(*id);
            }
        }
    }

    /// Drops ids that no longer exist (after delete or undo of a create).
    pub fn retain_existing(&mut self, snapshot: &DocumentSnapshot) {
        self.ids.retain(|id| snapshot.object(*id).is_some());
    }

    /// Union of the selected objects' bounds.
    pub fn bounds(&self, snapshot: &DocumentSnapshot) -> Option<Bounds> {
        self.ids.iter().filter_map(|id| snapshot.object(*id)?.bounds()).reduce(Bounds::union)
    }
}

/// Objects under a world point, best first: outline hits by distance, then interiors from the
/// smallest up (domain `burnburn_geometry::hit_test`, outline-accurate); equal candidates are
/// ordered topmost (drawn last, see `DocumentSnapshot::draw_order`) first. Locked and hidden
/// objects are not picked. `tolerance_mm` comes from screen points.
pub fn hit_test(snapshot: &DocumentSnapshot, at: Point, tolerance_mm: f64) -> Vec<GeometryId> {
    let Some(tol) = Length::from_mm(tolerance_mm).ok().and_then(|l| HitTestTolerance::new(l).ok()) else {
        return Vec::new();
    };
    // The key's first part makes the domain's tie-break pick the topmost object.
    let order = snapshot.draw_order();
    let index = SpatialIndex::build(
        order
            .iter()
            .enumerate()
            .filter(|(_, id)| snapshot.is_selectable(**id))
            .filter_map(|(rank, id)| snapshot.object(*id)?.bounds().map(|b| ((Reverse(rank), *id), b))),
    );
    burnburn_geometry::hit_test(&index, |(_, id)| snapshot.object(id).map(|o| o.path().as_ref()), at, &tol)
        .into_iter()
        .map(|c| c.key.1)
        .collect()
}

/// Objects whose bounds lie entirely inside `area` (rubber-band selection), in drawing order.
/// Locked and hidden objects are left out.
pub fn objects_inside(snapshot: &DocumentSnapshot, area: Bounds) -> Vec<GeometryId> {
    snapshot
        .draw_order()
        .into_iter()
        .filter(|id| snapshot.is_selectable(*id))
        .filter_map(|id| snapshot.object(id)?.bounds().map(|b| (id, b)))
        .filter(|(_, b)| area.contains(b.min()) && area.contains(b.max()))
        .map(|(id, _)| id)
        .collect()
}

/// Selection cycling (Alt-click, E03): the candidate after the currently selected one in a
/// topmost-first hit list, wrapping around. Returns the id and its 0-based position.
pub fn next_in_cycle(hits: &[GeometryId], selection: &Selection) -> Option<(GeometryId, usize)> {
    let next = match selection.single().and_then(|cur| hits.iter().position(|h| *h == cur)) {
        Some(pos) => (pos + 1) % hits.len(),
        None => 0,
    };
    hits.get(next).map(|id| (*id, next))
}

#[cfg(test)]
mod tests {
    use super::*;
    use burnburn_document::{GeometryObject, GeometrySource, ObjectKind};
    use burnburn_geometry::{Path, Rect};
    use burnburn_types::StableId;

    pub(crate) fn rect_obj(raw: u64, x: f64, y: f64, w: f64, h: f64) -> GeometryObject {
        let rect = Rect::new(Point::new(x, y), Length::from_mm(w).unwrap(), Length::from_mm(h).unwrap()).unwrap();
        GeometryObject::new(
            GeometryId::from_raw(raw),
            format!("r{raw}"),
            ObjectKind::Rectangle,
            GeometrySource::Drawn,
            Path::rectangle(rect),
        )
    }

    fn id(raw: u64) -> GeometryId {
        GeometryId::from_raw(raw)
    }

    #[test]
    fn hit_test_returns_overlapping_objects_topmost_first() {
        let snap = DocumentSnapshot::from_parts(
            vec![rect_obj(1, 0.0, 0.0, 50.0, 50.0), rect_obj(2, 20.0, 20.0, 50.0, 50.0)],
            3,
        )
        .unwrap();
        assert_eq!(hit_test(&snap, Point::new(30.0, 30.0), 1.5), vec![id(2), id(1)]);
        assert_eq!(hit_test(&snap, Point::new(5.0, 5.0), 1.5), vec![id(1)]);
        assert_eq!(hit_test(&snap, Point::new(71.0, 30.0), 1.5), vec![id(2)], "within tolerance");
        assert!(hit_test(&snap, Point::new(72.0, 30.0), 1.5).is_empty());
    }

    /// B2: a diagonal line is hit on its outline, not anywhere in its bounding box; a small
    /// shape inside a plate wins over the plate.
    #[test]
    fn hits_are_outline_accurate_and_inner_shapes_come_first() {
        let diag = GeometryObject::new(
            id(1),
            "l".into(),
            ObjectKind::Path,
            GeometrySource::Drawn,
            Path::line(Point::new(0.0, 0.0), Point::new(40.0, 40.0)).unwrap(),
        );
        let snap = DocumentSnapshot::from_parts(
            vec![diag, rect_obj(2, 100.0, 0.0, 100.0, 100.0), rect_obj(3, 140.0, 40.0, 10.0, 10.0)],
            4,
        )
        .unwrap();
        assert!(hit_test(&snap, Point::new(30.0, 5.0), 1.0).is_empty(), "inside the bbox, far from the line");
        assert_eq!(hit_test(&snap, Point::new(20.5, 20.0), 1.0), vec![id(1)]);
        assert_eq!(hit_test(&snap, Point::new(145.0, 45.0), 1.0), vec![id(3), id(2)], "hole/plate: inner first");
    }

    #[test]
    fn selection_toggle_replace_extend_and_retain_existing() {
        let mut s = Selection::default();
        let (a, b, c) = (id(1), id(2), id(3));
        s.apply(&SelectionChange::Replace(vec![a, a, b]));
        assert_eq!(s.ids(), &[a, b]);
        s.apply(&SelectionChange::Extend(vec![b, c]));
        assert_eq!(s.ids(), &[a, b, c]);
        assert_eq!(s.reference(), Some(a));
        s.apply(&SelectionChange::Toggle(a));
        s.apply(&SelectionChange::Toggle(c));
        assert_eq!(s.single(), Some(b));
        let snap = DocumentSnapshot::from_parts(vec![rect_obj(1, 0.0, 0.0, 1.0, 1.0)], 3).unwrap();
        s.retain_existing(&snap);
        assert!(s.is_empty());
    }

    #[test]
    fn rubber_band_selects_only_objects_fully_inside() {
        let snap = DocumentSnapshot::from_parts(
            vec![
                rect_obj(1, 0.0, 0.0, 10.0, 10.0),
                rect_obj(2, 20.0, 0.0, 10.0, 10.0),
                rect_obj(3, 5.0, 5.0, 40.0, 4.0),
            ],
            4,
        )
        .unwrap();
        let area = Bounds::from_corners(Point::new(-1.0, -1.0), Point::new(31.0, 11.0));
        assert_eq!(objects_inside(&snap, area), vec![id(1), id(2)], "object 3 sticks out on the right");
        let none = Bounds::from_corners(Point::new(100.0, 100.0), Point::new(110.0, 110.0));
        assert!(objects_inside(&snap, none).is_empty());
    }

    #[test]
    fn alt_click_cycles_through_overlapping_candidates_and_wraps() {
        let hits = [id(3), id(2), id(1)];
        let mut s = Selection::default();
        assert_eq!(next_in_cycle(&hits, &s), Some((id(3), 0)), "nothing selected: topmost first");
        s.apply(&SelectionChange::Replace(vec![id(3)]));
        assert_eq!(next_in_cycle(&hits, &s), Some((id(2), 1)));
        s.apply(&SelectionChange::Replace(vec![id(1)]));
        assert_eq!(next_in_cycle(&hits, &s), Some((id(3), 0)), "wraps after the bottom one");
        s.apply(&SelectionChange::Replace(vec![id(9)]));
        assert_eq!(next_in_cycle(&hits, &s), Some((id(3), 0)), "selection elsewhere: start at the top");
        assert_eq!(next_in_cycle(&[], &s), None);
    }
}
