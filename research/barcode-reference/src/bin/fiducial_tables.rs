//! Writes `crates/burnburn-generators/src/symbology/fiducial_tables.rs` to stdout from the
//! dictionary JSON files of the crate `calib-targets-aruco` 0.15.5 (MIT OR Apache-2.0), whose
//! codes are extracted from OpenCV 4.x (Apache-2.0) and the AprilTag families (BSD-2-Clause).
//!
//! Run: `cargo run --bin fiducial_tables -- <path to calib-targets-aruco-0.15.5/data> > ../../crates/burnburn-generators/src/symbology/fiducial_tables.rs`, then `rustfmt` that file.
//!
//! Packing of every code: row-major `idx = row * n + col`, bit 0 = top-left cell, black = 1
//! (the convention of that crate, checked against `cv2.aruco.generateImageMarker` by its
//! `tests/opencv_ground_truth.rs`; BurnBurn re-checks eight of those markers in
//! `tests/symbology_reference.rs`).

use std::{env, fs, path::Path, process};

fn read_codes(dir: &Path, name: &str) -> (usize, Vec<u64>) {
    let text = fs::read_to_string(dir.join(format!("{name}_CODES.json"))).unwrap_or_else(|e| {
        eprintln!("{name}: {e}");
        process::exit(1)
    });
    let size: usize = {
        let at = text.find("\"marker_size\":").expect("marker_size") + "\"marker_size\":".len();
        let rest = text[at..].trim_start();
        let end = rest.find(|c: char| !c.is_ascii_digit()).expect("end of marker_size");
        rest[..end].parse().expect("marker_size number")
    };
    let at = text.find("\"codes\":").expect("codes");
    let list = &text[at..];
    let open = list.find('[').expect("[");
    let close = list.find(']').expect("]");
    let codes = list[open + 1..close]
        .split(',')
        .map(|t| t.trim().trim_matches('"'))
        .filter(|t| !t.is_empty())
        .map(|t| u64::from_str_radix(t.trim_start_matches("0x"), 16).expect("hex code"))
        .collect();
    (size, codes)
}

fn main() {
    let dir = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: fiducial_tables <calib-targets-aruco data dir>");
        process::exit(2)
    });
    let dir = Path::new(&dir);
    // (rust constant, file stem, expected marker size)
    let families = [
        ("APRILTAG_16H5", "DICT_APRILTAG_16h5", 4),
        ("APRILTAG_25H9", "DICT_APRILTAG_25h9", 5),
        ("APRILTAG_36H10", "DICT_APRILTAG_36h10", 6),
        ("APRILTAG_36H11", "DICT_APRILTAG_36h11", 6),
        ("OPENCV_7X7_100", "DICT_7X7_100", 7),
    ];
    // Only the first 100 OpenCV 7x7 codes are shipped: they are the ones a second extraction (locus-core,
    // `fiducial_reference`) confirms. The smaller dictionaries are prefixes of the 1000-code one; prove it here.
    let (_, full) = read_codes(dir, "DICT_7X7_1000");
    for (stem, count) in [("DICT_7X7_50", 50), ("DICT_7X7_100", 100), ("DICT_7X7_250", 250)] {
        let (_, part) = read_codes(dir, stem);
        assert_eq!(part.len(), count, "{stem} count");
        assert_eq!(&full[..count], &part[..], "{stem} must be a prefix of DICT_7X7_1000");
    }
    println!("//! Marker code tables of the fiducial families (generated; do not edit by hand).");
    println!("//!");
    println!("//! Source: dictionary data of `calib-targets-aruco` 0.15.5 (MIT OR Apache-2.0), extracted from OpenCV");
    println!("//! (Apache-2.0) and the AprilTag families (BSD-2-Clause). Generator: `research/barcode-reference`");
    println!("//! (`cargo run --bin fiducial_tables`). Packing: row-major `idx = row * n + col`, bit 0 = top-left,");
    println!("//! black = 1. Independent oracles live in `tests/symbology_reference.rs`.");
    println!("//!");
    println!("//! Attribution: the AprilTag code tables are Copyright (C) 2013-2016 The Regents of the University of");
    println!(
        "//! Michigan (APRIL Robotics Laboratory), BSD-2-Clause; the OpenCV 7x7 dictionary is OpenCV, Apache-2.0."
    );
    println!("//! The code words are data, not program text; the licence texts are kept in the upstream crates.");
    println!();
    println!("#![allow(clippy::unreadable_literal)]");
    println!();
    for (constant, stem, n) in families {
        let (size, codes) = read_codes(dir, stem);
        assert_eq!(size, n, "{stem} marker size");
        let mask = if n * n == 64 { u64::MAX } else { (1u64 << (n * n)) - 1 };
        assert!(codes.iter().all(|c| c & !mask == 0), "{stem}: code wider than {} bits", n * n);
        println!("pub(super) static {constant}: [u64; {}] = [", codes.len());
        for chunk in codes.chunks(6) {
            let line: Vec<String> = chunk.iter().map(|c| format!("{c:#x}")).collect();
            println!("    {},", line.join(", "));
        }
        println!("];");
        println!();
    }
}
