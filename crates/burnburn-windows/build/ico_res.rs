//! Turns a Windows `.ico` file into a compiled Win32 resource file (`.res`) with one `RT_ICON` per
//! image and one `RT_GROUP_ICON` (id 1, the lowest id: the icon Explorer and the taskbar show).
//!
//! Pure Rust without dependencies: the MSVC linker takes a `.res` on its command line and turns it
//! into the `.rsrc` section of the executable. Shared by `build.rs` and the headless test
//! `tests/exe_icon_resource.rs`; it contains no Windows API and builds on every host.

/// Resource type of one icon image.
pub const RT_ICON: u16 = 3;
/// Resource type of the icon directory that groups the images.
pub const RT_GROUP_ICON: u16 = 14;
/// Resource id of the application icon group (the lowest id wins in Explorer).
pub const APP_ICON_ID: u16 = 1;

/// What is wrong with an `.ico` file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IcoError {
    /// Shorter than its directory says.
    Truncated,
    /// Not an icon directory (reserved field or type wrong).
    NotAnIcon,
    /// An icon directory without images.
    Empty,
    /// An image whose bytes lie outside the file.
    ImageOutOfRange { index: usize },
}

impl std::fmt::Display for IcoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IcoError::Truncated => write!(f, "the icon file is shorter than its directory"),
            IcoError::NotAnIcon => write!(f, "not an .ico file (bad directory header)"),
            IcoError::Empty => write!(f, "the .ico file holds no images"),
            IcoError::ImageOutOfRange { index } => write!(f, "image {index} lies outside the .ico file"),
        }
    }
}

struct Entry<'a> {
    width: u8,
    height: u8,
    colors: u8,
    planes: u16,
    bits: u16,
    data: &'a [u8],
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(at)?, *b.get(at + 1)?]))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes([*b.get(at)?, *b.get(at + 1)?, *b.get(at + 2)?, *b.get(at + 3)?]))
}

fn parse_ico(ico: &[u8]) -> Result<Vec<Entry<'_>>, IcoError> {
    let header = (u16_at(ico, 0), u16_at(ico, 2), u16_at(ico, 4));
    let (Some(reserved), Some(kind), Some(count)) = header else { return Err(IcoError::Truncated) };
    if reserved != 0 || kind != 1 {
        return Err(IcoError::NotAnIcon);
    }
    if count == 0 {
        return Err(IcoError::Empty);
    }
    let mut entries = Vec::with_capacity(usize::from(count));
    for index in 0..usize::from(count) {
        let at = 6 + index * 16;
        let dir = ico.get(at..at + 16).ok_or(IcoError::Truncated)?;
        let size = u32_at(dir, 8).ok_or(IcoError::Truncated)? as usize;
        let offset = u32_at(dir, 12).ok_or(IcoError::Truncated)? as usize;
        let data = offset
            .checked_add(size)
            .and_then(|end| ico.get(offset..end))
            .ok_or(IcoError::ImageOutOfRange { index })?;
        entries.push(Entry {
            width: dir[0],
            height: dir[1],
            colors: dir[2],
            planes: u16_at(dir, 4).ok_or(IcoError::Truncated)?,
            bits: u16_at(dir, 6).ok_or(IcoError::Truncated)?,
            data,
        });
    }
    Ok(entries)
}

/// One resource record: header (32 bytes for ordinal type and name) and data, both DWORD-aligned.
fn record(out: &mut Vec<u8>, kind: u16, id: u16, memory_flags: u16, data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_le_bytes()); // DataSize
    out.extend_from_slice(&32u32.to_le_bytes()); // HeaderSize
    out.extend_from_slice(&0xFFFFu16.to_le_bytes()); // TYPE: ordinal
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(&0xFFFFu16.to_le_bytes()); // NAME: ordinal
    out.extend_from_slice(&id.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // DataVersion
    out.extend_from_slice(&memory_flags.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // LanguageId: neutral
    out.extend_from_slice(&0u32.to_le_bytes()); // Version
    out.extend_from_slice(&0u32.to_le_bytes()); // Characteristics
    out.extend_from_slice(data);
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

/// MOVEABLE | PURE | DISCARDABLE, as the resource compiler writes icons.
const ICON_MEMORY_FLAGS: u16 = 0x1030;

/// The `.res` bytes for `ico`: the mandatory empty first record, the images as `RT_ICON` 1..=n and
/// the group `RT_GROUP_ICON` [`APP_ICON_ID`].
pub fn icon_resource(ico: &[u8]) -> Result<Vec<u8>, IcoError> {
    let entries = parse_ico(ico)?;
    let mut out = Vec::new();
    record(&mut out, 0, 0, 0, &[]);
    let mut group = Vec::new();
    group.extend_from_slice(&0u16.to_le_bytes());
    group.extend_from_slice(&1u16.to_le_bytes());
    group.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    for (i, e) in entries.iter().enumerate() {
        let id = i as u16 + 1;
        record(&mut out, RT_ICON, id, ICON_MEMORY_FLAGS, e.data);
        group.extend_from_slice(&[e.width, e.height, e.colors, 0]);
        group.extend_from_slice(&e.planes.to_le_bytes());
        group.extend_from_slice(&e.bits.to_le_bytes());
        group.extend_from_slice(&(e.data.len() as u32).to_le_bytes());
        group.extend_from_slice(&id.to_le_bytes());
    }
    record(&mut out, RT_GROUP_ICON, APP_ICON_ID, ICON_MEMORY_FLAGS, &group);
    Ok(out)
}
