//! Family and style a font file declares (the system font catalogue lists faces by these).
//! Reference values come from the fixtures' manifest and from `reference_metrics.py` (name table
//! read with `struct`, independent of skrifa): Liberation Sans Regular and BIZ UDPGothic Regular.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{FontStyle, font_names};

#[test]
fn a_font_declares_its_family_and_style() {
    let liberation = read_fixture("fonts", "LiberationSans-Regular.ttf").unwrap();
    assert_eq!(font_names(&liberation), Some(("Liberation Sans".to_owned(), FontStyle::Regular)));
    let biz = read_fixture("fonts", "BIZUDPGothic-Regular.ttf").unwrap();
    assert_eq!(font_names(&biz), Some(("BIZ UDPGothic".to_owned(), FontStyle::Regular)));
}

#[test]
fn bytes_that_are_not_a_font_have_no_names() {
    assert_eq!(font_names(b"{\"schema\": 1}"), None);
    assert_eq!(font_names(&[]), None);
    let mut truncated = read_fixture("fonts", "LiberationSans-Regular.ttf").unwrap();
    truncated.truncate(200);
    assert_eq!(font_names(&truncated), None);
}
