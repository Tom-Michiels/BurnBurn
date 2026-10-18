//! IM32 colour tracing (trace side): deterministic k-means palette and colour regions whose common
//! boundaries exist once in the topology (shared structure, not two independent contours).

#![allow(clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::sync::Arc;

use burnburn_geometry::Point;
use burnburn_image::{PixelScale, SourceImage};
use burnburn_trace::{ColourRequest, RegionGraph, colour_regions, quantize};
use burnburn_types::{CancellationToken, StableId, TaskId};

const S: f64 = 0.1;
const WHITE: [u8; 3] = [255, 255, 255];
const RED: [u8; 3] = [220, 30, 30];
const BLUE: [u8; 3] = [30, 40, 200];
const GREEN: [u8; 3] = [40, 180, 60];

/// 120×80 px, hard edges: red block x 1–6 mm, blue block x 6–11 mm (y 1–7 mm, y-up), green disc
/// inside the blue block.
fn flag() -> SourceImage {
    let (w, h) = (120u32, 80u32);
    let mut rgba = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let (mx, my) = ((f64::from(x) + 0.5) * S, (f64::from(h - y) - 0.5) * S);
            let c = if (mx - 8.5).hypot(my - 4.0) <= 1.2 {
                GREEN
            } else if (1.0..6.0).contains(&mx) && (1.0..7.0).contains(&my) {
                RED
            } else if (6.0..11.0).contains(&mx) && (1.0..7.0).contains(&my) {
                BLUE
            } else {
                WHITE
            };
            rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    SourceImage::from_rgba8(w, h, rgba).unwrap()
}

fn request(source: SourceImage) -> ColourRequest {
    ColourRequest {
        task: TaskId::from_raw(1),
        source: Arc::new(source),
        scale: PixelScale::uniform_mm_per_pixel(S).unwrap(),
        k: 4,
        seed: 7,
        tolerance_mm: 0.05,
    }
}

fn region_of(g: &RegionGraph, colour: [u8; 3]) -> burnburn_trace::RegionId {
    let idx = g.palette.colours.iter().position(|&c| c == colour).unwrap();
    let regions: Vec<_> = g.regions.iter().filter(|r| r.colour == idx).collect();
    assert_eq!(regions.len(), 1, "one region of {colour:?}");
    regions[0].id
}

#[test]
fn shared_colour_boundary_is_stored_once_in_the_region_topology() {
    let g = colour_regions(&request(flag()), &CancellationToken::never()).unwrap();
    let palette: BTreeSet<[u8; 3]> = g.palette.colours.iter().copied().collect();
    assert_eq!(palette, BTreeSet::from([WHITE, RED, BLUE, GREEN]), "exact colours of a hard-edged image");
    assert_eq!(g.regions.len(), 4);
    let (white, red, blue, green) =
        (region_of(&g, WHITE), region_of(&g, RED), region_of(&g, BLUE), region_of(&g, GREEN));

    let between = |a, b| -> Vec<&burnburn_trace::BoundaryEdge> {
        g.boundaries
            .iter()
            .filter(|e| (e.left == a && e.right == Some(b)) || (e.left == b && e.right == Some(a)))
            .collect()
    };
    let shared = between(red, blue);
    assert_eq!(shared.len(), 1, "red|blue boundary exists once");
    let shared = shared[0];
    assert!(!shared.path.is_closed());
    // It is the vertical line x = 6 mm from y = 1 to 7 mm.
    let (a, b) = (shared.path.start(), shared.path.end());
    for p in [a, b] {
        assert!((p.x - 6.0).abs() < 1e-9, "{p:?}");
    }
    assert!(((a.y - b.y).abs() - 6.0).abs() < 1e-9);
    assert_eq!(between(blue, green).len(), 1, "island boundary once");
    assert!(between(blue, green)[0].path.is_closed());
    assert!(between(red, green).is_empty());
    // Every boundary separates two different regions; the outside is `None`.
    for e in &g.boundaries {
        assert_ne!(Some(e.left), e.right);
    }
    let outside: Vec<_> = g.boundaries.iter().filter(|e| e.right.is_none()).collect();
    assert_eq!(outside.len(), 1);
    assert_eq!(outside[0].left, white);

    // Both regions refer to the one shared boundary, in opposite directions.
    let uses =
        |r| g.region(r).unwrap().boundaries.iter().filter(|u| u.boundary == shared.id).copied().collect::<Vec<_>>();
    let (ur, ub) = (uses(red), uses(blue));
    assert_eq!((ur.len(), ub.len()), (1, 1));
    assert_ne!(ur[0].reversed, ub[0].reversed);
    // Region outlines are assembled from the shared geometry: the boundary's nodes are nodes of both.
    let nodes = |r| -> Vec<Point> {
        let path = g.region_path(r).unwrap();
        path.subpaths()
            .iter()
            .flat_map(|s| std::iter::once(s.start()).chain(s.segments().iter().map(|seg| seg.end())))
            .collect()
    };
    let shared_nodes: Vec<Point> =
        std::iter::once(shared.path.start()).chain(shared.path.segments().iter().map(|s| s.end())).collect();
    for r in [red, blue] {
        let n = nodes(r);
        assert!(shared_nodes.iter().all(|p| n.contains(p)), "region {r:?} reuses the shared geometry");
    }
    // Areas: red 5×6 mm, blue 5×6 minus the green disc.
    let area = |r| g.region(r).unwrap().area_mm2;
    assert!((area(red) - 30.0).abs() < 0.2);
    assert!((area(blue) + area(green) - 30.0).abs() < 0.2);
    assert!(g.region_path(red).unwrap().signed_area().abs() > 29.0);
}

#[test]
fn quantization_is_deterministic_for_a_fixed_seed_and_regions_are_reproducible() {
    // Anti-aliased colour discs: many mixed colours, k-means must still be reproducible.
    let (w, h) = (90u32, 60u32);
    let mut rgba = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0u32; 3];
            for sy in 0..4 {
                for sx in 0..4 {
                    let (mx, my) = (
                        (f64::from(x) + (f64::from(sx) + 0.5) / 4.0) * S,
                        (f64::from(h - y) - (f64::from(sy) + 0.5) / 4.0) * S,
                    );
                    let c = if (mx - 2.5).hypot(my - 3.0) < 2.0 {
                        RED
                    } else if (mx - 6.5).hypot(my - 3.0) < 2.0 {
                        BLUE
                    } else {
                        WHITE
                    };
                    for i in 0..3 {
                        acc[i] += u32::from(c[i]);
                    }
                }
            }
            rgba.extend_from_slice(&[(acc[0] / 16) as u8, (acc[1] / 16) as u8, (acc[2] / 16) as u8, 255]);
        }
    }
    let img = SourceImage::from_rgba8(w, h, rgba).unwrap();
    let p1 = quantize(&img, 3, 42, &CancellationToken::never()).unwrap();
    let p2 = quantize(&img, 3, 42, &CancellationToken::never()).unwrap();
    assert_eq!(p1, p2);
    assert_eq!(p1.colours.len(), 3);
    for want in [WHITE, RED, BLUE] {
        let d = p1
            .colours
            .iter()
            .map(|c| (0..3).map(|i| (i32::from(c[i]) - i32::from(want[i])).pow(2)).sum::<i32>())
            .min()
            .unwrap();
        assert!(d < 30 * 30, "palette near {want:?}: {:?}", p1.colours);
    }
    let mut req = request(img);
    req.k = 3;
    req.seed = 42;
    let g1 = colour_regions(&req, &CancellationToken::never()).unwrap();
    let g2 = colour_regions(&req, &CancellationToken::never()).unwrap();
    assert_eq!(g1, g2, "same source, k and seed → identical graph");
    assert!(g1.boundaries.iter().all(|e| Some(e.left) != e.right));

    let cancel = burnburn_types::CancellationSource::new();
    cancel.cancel();
    assert!(matches!(colour_regions(&req, &cancel.token()), Err(burnburn_trace::TraceError::Cancelled(_))));
    assert!(matches!(
        quantize(&req.source, 0, 1, &CancellationToken::never()),
        Err(burnburn_trace::TraceError::InvalidRecipe(_))
    ));
}
