//! DR-13: geometry that can reach project or plan hashes must be bit-identical on macOS and
//! Windows. Transcendental functions therefore go through `libm` (pure Rust), never the OS libm.

#![allow(clippy::unwrap_used, clippy::unreachable)] // test helpers and let-else in tests

use std::path::Path as FsPath;

/// Method calls that dispatch to the platform's libm. `sqrt` and + − × ÷ are IEEE-exact and allowed.
const PLATFORM_CALLS: [&str; 13] = [
    ".sin_cos(",
    ".sin(",
    ".cos(",
    ".tan(",
    ".acos(",
    ".asin(",
    ".atan(",
    ".atan2(",
    ".hypot(",
    ".cbrt(",
    ".exp(",
    ".ln(",
    ".powf(",
];

#[test]
fn geometry_sources_route_transcendentals_through_libm() {
    let dir = FsPath::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "rs") || path.file_name().is_some_and(|n| n == "math.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for (i, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            if PLATFORM_CALLS.iter().any(|c| code.contains(c)) {
                offenders.push(format!("{}:{}: {}", path.display(), i + 1, line.trim()));
            }
        }
    }
    assert!(offenders.is_empty(), "use crate::math instead:\n{}", offenders.join("\n"));
}

/// Recorded on macOS arm64 with `libm` 0.2.16 (see docs/dev-notes/IM09.md). On another platform a
/// different bit pattern means the arithmetic is not platform independent.
#[test]
fn arc_to_cubic_bits_are_platform_independent() {
    use burnburn_geometry::{NodeEdit, Path, Point, Segment};
    let circle = Path::circle(Point::new(1.25, -3.5), 7.3).unwrap();
    let cubic = circle.edit_nodes(&NodeEdit::ConvertToCubic { subpath: 0 }).unwrap();
    let Segment::Cubic { c1, c2, to } = cubic.subpaths()[0].segments()[1] else {
        unreachable!("conversion yields cubics")
    };
    let bits: Vec<u64> = [c1.x, c1.y, c2.x, c2.y, to.x, to.y].iter().map(|v| v.to_bits()).collect();
    assert_eq!(bits, GOLDEN, "{:?}", [c1, c2, to]);
}

const GOLDEN: [u64; 6] = [
    13836818239174108998,
    4615739258092021350,
    13841869749718561586,
    4602964155299372328,
    13841869749718561587,
    13838435755002691582,
];
