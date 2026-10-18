//! Solved sketch → production contours. Construction entities and points are left out. Lines
//! and arcs that share an end point entity are chained (in entity order, deterministic); a chain
//! that returns to its start is a closed subpath. Each circle is one closed full-circle arc.
//! Every contour is one output, keyed by its lowest entity id.

use std::collections::BTreeMap;
use std::f64::consts::TAU;

use burnburn_document::sketch::{Entity, EntityId};
use burnburn_geometry::{EllipticalArc, GeometryError, Path, PathBuilder, Point};
use burnburn_types::Angle;

use crate::evaluate::SolvedSketch;

#[derive(Debug, Clone, Copy)]
struct Edge {
    entity: EntityId,
    from: EntityId,
    to: EntityId,
}

fn angle(rad: f64) -> Result<Angle, GeometryError> {
    Angle::from_radians(rad).map_err(|_| GeometryError::NotFinite)
}

/// Circular arc from `from` to `to` around `center`. `libm::atan2` keeps the bits identical on
/// every platform (DR-13).
fn arc_between(center: Point, from: Point, to: Point, clockwise: bool) -> Result<EllipticalArc, GeometryError> {
    let a0 = libm::atan2(from.y - center.y, from.x - center.x);
    let a1 = libm::atan2(to.y - center.y, to.x - center.x);
    let mut sweep = a1 - a0;
    if clockwise {
        if sweep >= 0.0 {
            sweep -= TAU;
        }
    } else if sweep <= 0.0 {
        sweep += TAU;
    }
    EllipticalArc::circle(center, center.distance(from), angle(a0)?, angle(sweep)?)
}

pub(crate) fn outputs(s: &SolvedSketch) -> Result<Vec<(u64, Path)>, GeometryError> {
    let pt = |id: EntityId| s.point(id).ok_or(GeometryError::NotFinite);
    let mut out: Vec<(u64, Path)> = Vec::new();
    let mut edges = Vec::new();
    for (id, e) in s.entities() {
        if e.construction {
            continue;
        }
        match e.entity {
            Entity::Line { start, end } | Entity::Arc { start, end, .. } => {
                edges.push(Edge { entity: id, from: start, to: end })
            }
            Entity::Circle { center, radius } => {
                let arc = EllipticalArc::circle(pt(center)?, radius, angle(0.0)?, angle(TAU)?)?;
                out.push((
                    u64::from(id.raw()),
                    PathBuilder::new().move_to(arc.start_point()).arc(arc).close().build()?,
                ));
            }
            _ => {}
        }
    }
    let mut degree: BTreeMap<EntityId, usize> = BTreeMap::new();
    for e in &edges {
        *degree.entry(e.from).or_default() += 1;
        *degree.entry(e.to).or_default() += 1;
    }
    let deg = |n: EntityId| degree.get(&n).copied().unwrap_or(0);
    // Open chains start at a dangling end; the rest are loops.
    let mut starts: Vec<(usize, EntityId)> = Vec::new();
    for (k, e) in edges.iter().enumerate() {
        if deg(e.from) == 1 {
            starts.push((k, e.from));
        } else if deg(e.to) == 1 {
            starts.push((k, e.to));
        }
    }
    starts.extend(edges.iter().enumerate().map(|(k, e)| (k, e.from)));
    let mut used = vec![false; edges.len()];
    for (k, first) in starts {
        if used[k] {
            continue;
        }
        let mut chain: Vec<(usize, bool)> = Vec::new();
        let (mut ek, mut at) = (k, first);
        let closed = loop {
            used[ek] = true;
            let e = edges[ek];
            let forward = e.from == at;
            chain.push((ek, forward));
            let end = if forward { e.to } else { e.from };
            if end == first {
                break true;
            }
            match (0..edges.len()).find(|&j| !used[j] && (edges[j].from == end || edges[j].to == end)) {
                Some(j) => (ek, at) = (j, end),
                None => break false,
            }
        };
        let key = chain.iter().map(|(i, _)| u64::from(edges[*i].entity.raw())).min().unwrap_or(0);
        let mut b = PathBuilder::new();
        let count = chain.len();
        for (pos, (i, forward)) in chain.into_iter().enumerate() {
            let e = edges[i];
            let (a, z) = if forward { (pt(e.from)?, pt(e.to)?) } else { (pt(e.to)?, pt(e.from)?) };
            if pos == 0 {
                b = b.move_to(a);
            }
            match s.entities.get(&e.entity).map(|x| x.entity) {
                Some(Entity::Arc { center, start, end, clockwise }) => {
                    let arc = arc_between(pt(center)?, pt(start)?, pt(end)?, clockwise)?;
                    b = b.arc(if forward { arc } else { arc.reversed() });
                }
                // A closed chain's last line is the implicit closing segment.
                _ if closed && pos + 1 == count && count > 1 => {}
                _ => b = b.line_to(z),
            }
        }
        if closed {
            b = b.close();
        }
        out.push((key, b.build()?));
    }
    out.sort_by_key(|(k, _)| *k);
    Ok(out)
}
