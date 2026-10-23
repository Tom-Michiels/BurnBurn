//! LFB-rest: the BurnBurn icon is embedded in the Windows executable. The build script turns
//! `res/burnburn.ico` into a compiled resource file that the MSVC linker puts in `.rsrc`. This test
//! reads that resource file back with an independent reader (the layout from the Win32 `.res`
//! specification, not the writer's code) and compares it with the images in the `.ico`. The PE of a
//! linked executable can only be inspected on a Windows runner (not covered here).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

#[path = "../build/ico_res.rs"]
mod ico_res;

use ico_res::{APP_ICON_ID, IcoError, RT_GROUP_ICON, RT_ICON, icon_resource};

const ICO: &[u8] = include_bytes!("../res/burnburn.ico");

struct Record {
    kind: u16,
    id: u16,
    data: Vec<u8>,
}

fn le16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Reads a `.res` the way CVTRES does: records of DataSize, HeaderSize, ordinal TYPE and NAME, a
/// fixed tail, then the data; everything DWORD-aligned.
fn read_res(res: &[u8]) -> Vec<Record> {
    let mut at = 0;
    let mut out = Vec::new();
    while at < res.len() {
        assert_eq!(at % 4, 0, "records start DWORD-aligned");
        let data_size = le32(res, at) as usize;
        let header_size = le32(res, at + 4) as usize;
        assert_eq!(header_size, 32, "ordinal type and name make a 32-byte header");
        assert_eq!(le16(res, at + 8), 0xFFFF, "TYPE is an ordinal");
        assert_eq!(le16(res, at + 12), 0xFFFF, "NAME is an ordinal");
        let (kind, id) = (le16(res, at + 10), le16(res, at + 14));
        let start = at + header_size;
        out.push(Record { kind, id, data: res[start..start + data_size].to_vec() });
        at = (start + data_size + 3) & !3;
    }
    assert_eq!(at, res.len(), "no trailing bytes");
    out
}

/// The images of an `.ico`: (width, height, bits, bytes).
fn ico_images(ico: &[u8]) -> Vec<(u8, u8, u16, Vec<u8>)> {
    (0..usize::from(le16(ico, 4)))
        .map(|i| {
            let d = 6 + i * 16;
            let (size, offset) = (le32(ico, d + 8) as usize, le32(ico, d + 12) as usize);
            (ico[d], ico[d + 1], le16(ico, d + 6), ico[offset..offset + size].to_vec())
        })
        .collect()
}

#[test]
fn the_resource_file_holds_every_image_of_the_icon_and_a_group_that_points_at_them() {
    let images = ico_images(ICO);
    assert!(images.len() >= 4, "the shipped icon has several sizes: {}", images.len());
    let records = read_res(&icon_resource(ICO).unwrap());

    // The first record is the empty header CVTRES requires.
    assert_eq!((records[0].kind, records[0].id, records[0].data.len()), (0, 0, 0));
    let icons: Vec<&Record> = records.iter().filter(|r| r.kind == RT_ICON).collect();
    assert_eq!(icons.len(), images.len(), "one RT_ICON per image");
    for (record, (_, _, _, bytes)) in icons.iter().zip(&images) {
        assert_eq!(&record.data, bytes, "RT_ICON {} carries the image bytes unchanged", record.id);
    }
    let ids: std::collections::BTreeSet<u16> = icons.iter().map(|r| r.id).collect();
    assert_eq!(ids.len(), icons.len(), "icon ids are unique");

    let groups: Vec<&Record> = records.iter().filter(|r| r.kind == RT_GROUP_ICON).collect();
    assert_eq!(groups.len(), 1);
    let group = groups[0];
    assert_eq!(group.id, APP_ICON_ID);
    assert_eq!(APP_ICON_ID, 1, "the lowest id: the icon Explorer shows");
    assert_eq!((le16(&group.data, 0), le16(&group.data, 2)), (0, 1), "GRPICONDIR: reserved 0, type 1");
    assert_eq!(usize::from(le16(&group.data, 4)), images.len());
    assert_eq!(group.data.len(), 6 + 14 * images.len());
    for (i, (w, h, bits, bytes)) in images.iter().enumerate() {
        let e = 6 + i * 14;
        assert_eq!((group.data[e], group.data[e + 1], le16(&group.data, e + 6)), (*w, *h, *bits));
        assert_eq!(le32(&group.data, e + 8) as usize, bytes.len(), "image size in the group");
        let target = le16(&group.data, e + 12);
        let icon = icons.iter().find(|r| r.id == target).expect("the group refers to an existing RT_ICON");
        assert_eq!(&icon.data, bytes, "entry {i} points at its own image");
    }
}

#[test]
fn a_damaged_icon_file_is_refused_with_a_reason() {
    assert_eq!(icon_resource(&[]), Err(IcoError::Truncated));
    assert_eq!(icon_resource(&[0, 0, 2, 0, 1, 0]), Err(IcoError::NotAnIcon), "a cursor is not an icon");
    assert_eq!(icon_resource(&[0, 0, 1, 0, 0, 0]), Err(IcoError::Empty));
    // The directory announces two images but holds one entry.
    assert_eq!(icon_resource(&ICO[..6 + 16]).map(|_| ()), Err(IcoError::ImageOutOfRange { index: 0 }));
    let mut short = ICO[..6].to_vec();
    short[4] = 9;
    assert_eq!(icon_resource(&short), Err(IcoError::Truncated));
    // An image that starts beyond the end of the file.
    let mut bad = ICO.to_vec();
    bad[6 + 12..6 + 16].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(icon_resource(&bad).map(|_| ()), Err(IcoError::ImageOutOfRange { index: 0 }));
}

/// The build script wrote the resource file and told us where (`cargo:rustc-env`): it is the
/// resource of the shipped icon, byte for byte.
#[test]
fn the_build_script_produced_the_resource_file_of_the_shipped_icon() {
    let path = env!("BURNBURN_ICON_RES"); // set by build.rs: a missing variable fails the compile
    let written = std::fs::read(path).expect("the resource file exists");
    assert_eq!(written, icon_resource(ICO).unwrap());
    assert!(!read_res(&written).is_empty());
}
