//! Byte format of journal segments (contracts-phase-c §IM17, ADR 0008 amendment 1).
//!
//! Segment = 8-byte header `b"BBJ\0" ‖ u16 LE version (1) ‖ u16 LE 0`, then records.
//! Record = `u32 LE magic 0xB0B0_4A31 ‖ u32 LE len ‖ u32 LE crc32(payload) ‖ payload[len]`.

pub const SEGMENT_MAGIC: [u8; 4] = *b"BBJ\0";
pub const FORMAT_VERSION: u16 = 1;
pub const RECORD_MAGIC: u32 = 0xB0B0_4A31;
pub const HEADER_LEN: usize = 8;
pub const RECORD_HEADER_LEN: usize = 12;

pub fn segment_header() -> [u8; HEADER_LEN] {
    let mut h = [0u8; HEADER_LEN];
    h[..4].copy_from_slice(&SEGMENT_MAGIC);
    h[4..6].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
    h
}

pub fn encode_record(payload: &[u8]) -> Option<Vec<u8>> {
    let len = u32::try_from(payload.len()).ok()?;
    let mut out = Vec::with_capacity(RECORD_HEADER_LEN + payload.len());
    out.extend_from_slice(&RECORD_MAGIC.to_le_bytes());
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&crc32fast::hash(payload).to_le_bytes());
    out.extend_from_slice(payload);
    Some(out)
}

/// How a segment ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegmentEnd {
    Clean,
    /// A short or damaged *final* record: nothing follows it in this segment.
    TornTail {
        offset: u64,
        bytes: u64,
    },
    /// Damage with more data after it, or an invalid header: not explainable by a crash.
    Corrupt {
        offset: u64,
        reason: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedSegment<'a> {
    /// `(offset, payload)` of every intact record before the end.
    pub records: Vec<(u64, &'a [u8])>,
    pub end: SegmentEnd,
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at + 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// Splits a segment into records. Never panics on arbitrary bytes.
pub fn scan_segment(bytes: &[u8], max_record: u32) -> ScannedSegment<'_> {
    let mut records = Vec::new();
    let header_ok = bytes.len() >= HEADER_LEN
        && bytes[..4] == SEGMENT_MAGIC
        && u16::from_le_bytes([bytes[4], bytes[5]]) == FORMAT_VERSION
        && bytes[6..8] == [0, 0];
    if !header_ok {
        return ScannedSegment { records, end: SegmentEnd::Corrupt { offset: 0, reason: "segment header" } };
    }
    let mut at = HEADER_LEN;
    let total = bytes.len();
    while at < total {
        let offset = at as u64;
        let rest = (total - at) as u64;
        let torn = SegmentEnd::TornTail { offset, bytes: rest };
        if total - at < RECORD_HEADER_LEN {
            return ScannedSegment { records, end: torn };
        }
        let (Some(magic), Some(len), Some(crc)) = (u32_at(bytes, at), u32_at(bytes, at + 4), u32_at(bytes, at + 8))
        else {
            return ScannedSegment { records, end: torn };
        };
        let body = at + RECORD_HEADER_LEN;
        let end = body.saturating_add(len as usize);
        let is_last = end >= total;
        let bad = |reason| if is_last { torn.clone() } else { SegmentEnd::Corrupt { offset, reason } };
        // A crash leaves a *prefix* of a record: with a full 12-byte header present, magic and
        // length are intact. A wrong magic or an oversized length is damage, not a torn write.
        if magic != RECORD_MAGIC {
            return ScannedSegment { records, end: SegmentEnd::Corrupt { offset, reason: "record magic" } };
        }
        if len > max_record {
            return ScannedSegment { records, end: SegmentEnd::Corrupt { offset, reason: "record length" } };
        }
        let Some(payload) = bytes.get(body..end) else {
            return ScannedSegment { records, end: torn };
        };
        if crc32fast::hash(payload) != crc {
            return ScannedSegment { records, end: bad("record checksum") };
        }
        records.push((offset, payload));
        at = end;
    }
    ScannedSegment { records, end: SegmentEnd::Clean }
}
