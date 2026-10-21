//! W4-UI-machine: a gray8 bitmap (the Adjust Image preview) as a picture for the canvas.

#![allow(clippy::unwrap_used)] // test code

use burnburn_editor::BedPicture;

#[test]
fn gray8_expands_to_opaque_rgba_in_the_same_pixel_order() {
    let pic = BedPicture::from_gray8(7, 2, 2, &[0, 255, 128, 10]).unwrap();

    assert_eq!((pic.id, pic.width, pic.height), (7, 2, 2));
    assert_eq!(
        &*pic.rgba,
        &[0, 0, 0, 255, 255, 255, 255, 255, 128, 128, 128, 255, 10, 10, 10, 255],
        "burn = 0 is black, white = 255, alpha always opaque"
    );
}

#[test]
fn gray8_of_the_wrong_length_or_an_empty_size_is_not_a_picture() {
    assert!(BedPicture::from_gray8(1, 2, 2, &[0, 0, 0]).is_none());
    assert!(BedPicture::from_gray8(1, 2, 2, &[0; 5]).is_none());
    assert!(BedPicture::from_gray8(1, 0, 2, &[]).is_none());
    assert!(BedPicture::from_gray8(1, 2, 0, &[]).is_none());
}
