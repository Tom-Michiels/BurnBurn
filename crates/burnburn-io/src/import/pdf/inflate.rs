//! zlib/deflate decoder (RFC 1950/1951) with an output cap, for PDF streams. Own implementation
//! so the importer adds no dependency; the structure follows the canonical-Huffman "puff"
//! decoder. Any malformed input is a typed error, never a panic, and the output never exceeds
//! `max_out` bytes (compression bombs).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InflateError {
    /// Not a zlib header, bad checksum bits or a preset dictionary.
    Header,
    /// Truncated or invalid deflate data.
    Data,
    /// The output would exceed the cap.
    TooLarge,
}

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    bit_buf: u32,
    bit_count: u32,
}

impl<'a> Bits<'a> {
    fn need(&mut self, n: u32) -> Result<(), InflateError> {
        while self.bit_count < n {
            let byte = *self.data.get(self.pos).ok_or(InflateError::Data)?;
            self.pos += 1;
            self.bit_buf |= u32::from(byte) << self.bit_count;
            self.bit_count += 8;
        }
        Ok(())
    }

    fn take(&mut self, n: u32) -> Result<u32, InflateError> {
        if n == 0 {
            return Ok(0);
        }
        self.need(n)?;
        let v = self.bit_buf & ((1u32 << n) - 1);
        self.bit_buf >>= n;
        self.bit_count -= n;
        Ok(v)
    }
}

struct Huffman {
    count: [u16; 16],
    symbol: Vec<u16>,
}

impl Huffman {
    /// `lengths[i]` is the code length of symbol `i` (0 = unused). Over-subscribed sets are
    /// rejected; incomplete sets are allowed (as zlib allows them for a single distance code).
    fn new(lengths: &[u8]) -> Result<Self, InflateError> {
        let mut count = [0u16; 16];
        for l in lengths {
            count[usize::from(*l)] += 1;
        }
        let mut left: i32 = 1;
        for c in &count[1..] {
            left = (left << 1) - i32::from(*c);
            if left < 0 {
                return Err(InflateError::Data);
            }
        }
        let mut offs = [0u16; 16];
        for len in 1..15 {
            offs[len + 1] = offs[len] + count[len];
        }
        let mut symbol = vec![0u16; lengths.len()];
        for (sym, l) in lengths.iter().enumerate() {
            if *l != 0 {
                symbol[usize::from(offs[usize::from(*l)])] = sym as u16;
                offs[usize::from(*l)] += 1;
            }
        }
        Ok(Huffman { count, symbol })
    }

    fn decode(&self, bits: &mut Bits<'_>) -> Result<u16, InflateError> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= bits.take(1)? as i32;
            let count = i32::from(self.count[len]);
            if code - count < first {
                return self.symbol.get((index + (code - first)) as usize).copied().ok_or(InflateError::Data);
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err(InflateError::Data)
    }
}

const LEN_BASE: [u16; 29] =
    [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEN_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145,
    8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] =
    [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

/// Decodes a zlib stream (the Adler-32 trailer is not verified: PDF readers tolerate it being
/// absent or wrong). Output is capped at `max_out` bytes.
pub(super) fn zlib_inflate(data: &[u8], max_out: usize) -> Result<Vec<u8>, InflateError> {
    let (cmf, flg) = (*data.first().ok_or(InflateError::Header)?, *data.get(1).ok_or(InflateError::Header)?);
    if cmf & 0x0f != 8 || (u16::from(cmf) * 256 + u16::from(flg)) % 31 != 0 || flg & 0x20 != 0 {
        return Err(InflateError::Header);
    }
    inflate(&data[2..], max_out)
}

/// Decodes a raw deflate stream.
pub(super) fn inflate(data: &[u8], max_out: usize) -> Result<Vec<u8>, InflateError> {
    let mut bits = Bits { data, pos: 0, bit_buf: 0, bit_count: 0 };
    let mut out: Vec<u8> = Vec::new();
    loop {
        let last = bits.take(1)? == 1;
        match bits.take(2)? {
            0 => {
                // Stored block: skip to the byte boundary, then LEN / NLEN.
                bits.bit_buf = 0;
                bits.bit_count = 0;
                let head = data.get(bits.pos..bits.pos + 4).ok_or(InflateError::Data)?;
                let len = usize::from(u16::from_le_bytes([head[0], head[1]]));
                if head[2] != !head[0] || head[3] != !head[1] {
                    return Err(InflateError::Data);
                }
                bits.pos += 4;
                let body = data.get(bits.pos..bits.pos + len).ok_or(InflateError::Data)?;
                if out.len() + len > max_out {
                    return Err(InflateError::TooLarge);
                }
                out.extend_from_slice(body);
                bits.pos += len;
            }
            1 => {
                let mut lengths = [0u8; 288];
                for (i, l) in lengths.iter_mut().enumerate() {
                    *l = match i {
                        0..=143 => 8,
                        144..=255 => 9,
                        256..=279 => 7,
                        _ => 8,
                    };
                }
                let lit = Huffman::new(&lengths)?;
                let dist = Huffman::new(&[5u8; 30])?;
                codes(&mut bits, &mut out, &lit, &dist, max_out)?;
            }
            2 => {
                let nlen = bits.take(5)? as usize + 257;
                let ndist = bits.take(5)? as usize + 1;
                let ncode = bits.take(4)? as usize + 4;
                if nlen > 286 || ndist > 30 {
                    return Err(InflateError::Data);
                }
                const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
                let mut cl = [0u8; 19];
                for &slot in ORDER.iter().take(ncode) {
                    cl[slot] = bits.take(3)? as u8;
                }
                let clh = Huffman::new(&cl)?;
                let mut lengths = vec![0u8; nlen + ndist];
                let mut i = 0;
                while i < nlen + ndist {
                    let sym = clh.decode(&mut bits)?;
                    if sym < 16 {
                        lengths[i] = sym as u8;
                        i += 1;
                    } else {
                        let (value, repeat) = match sym {
                            16 => (*lengths.get(i.wrapping_sub(1)).ok_or(InflateError::Data)?, 3 + bits.take(2)?),
                            17 => (0, 3 + bits.take(3)?),
                            _ => (0, 11 + bits.take(7)?),
                        };
                        if i + repeat as usize > nlen + ndist {
                            return Err(InflateError::Data);
                        }
                        for _ in 0..repeat {
                            lengths[i] = value;
                            i += 1;
                        }
                    }
                }
                if lengths[256] == 0 {
                    return Err(InflateError::Data);
                }
                let lit = Huffman::new(&lengths[..nlen])?;
                let dist = Huffman::new(&lengths[nlen..])?;
                codes(&mut bits, &mut out, &lit, &dist, max_out)?;
            }
            _ => return Err(InflateError::Data),
        }
        if last {
            return Ok(out);
        }
    }
}

fn codes(
    bits: &mut Bits<'_>,
    out: &mut Vec<u8>,
    lit: &Huffman,
    dist: &Huffman,
    max_out: usize,
) -> Result<(), InflateError> {
    loop {
        let sym = usize::from(lit.decode(bits)?);
        match sym {
            0..=255 => {
                if out.len() >= max_out {
                    return Err(InflateError::TooLarge);
                }
                out.push(sym as u8);
            }
            256 => return Ok(()),
            257..=285 => {
                let i = sym - 257;
                let len = usize::from(LEN_BASE[i]) + bits.take(u32::from(LEN_EXTRA[i]))? as usize;
                let d = usize::from(dist.decode(bits)?);
                if d >= 30 {
                    return Err(InflateError::Data);
                }
                let distance = usize::from(DIST_BASE[d]) + bits.take(u32::from(DIST_EXTRA[d]))? as usize;
                if distance > out.len() {
                    return Err(InflateError::Data);
                }
                if out.len() + len > max_out {
                    return Err(InflateError::TooLarge);
                }
                for _ in 0..len {
                    out.push(out[out.len() - distance]);
                }
            }
            _ => return Err(InflateError::Data),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::io::{Cursor, Write};

    use proptest::prelude::*;

    use super::*;

    /// Raw deflate bytes from the zip crate's encoder (an oracle independent of this decoder).
    fn deflate(data: &[u8], level: i64) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(level));
        zip.start_file("x", opts).unwrap();
        zip.write_all(data).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(zip.finish().unwrap().into_inner())).unwrap();
        let mut raw = Vec::new();
        std::io::copy(&mut archive.by_index_raw(0).unwrap(), &mut raw).unwrap();
        raw
    }

    #[test]
    fn stored_fixed_and_dynamic_blocks_roundtrip_against_an_independent_encoder() {
        let text: Vec<u8> =
            b"the quick brown fox jumps over the lazy dog. ".iter().copied().cycle().take(5000).collect();
        let noise: Vec<u8> = (0u32..4000).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect();
        for data in [Vec::new(), b"a".to_vec(), b"abcabcabcabc".to_vec(), text, noise] {
            for level in [1, 6, 9] {
                let raw = deflate(&data, level);
                assert_eq!(inflate(&raw, 1 << 20).unwrap(), data, "level {level}, {} bytes", data.len());
            }
        }
    }

    #[test]
    fn the_farthest_distance_codes_are_decoded() {
        // 30 000 bytes of noise repeated once: the second copy refers back 30 000 bytes (distance code 29).
        let noise: Vec<u8> = (0u32..30_000).map(|i| (i.wrapping_mul(2_246_822_519) >> 11) as u8).collect();
        let data = [noise.clone(), noise].concat();
        for level in [1, 9] {
            assert_eq!(inflate(&deflate(&data, level), 1 << 20).unwrap(), data, "level {level}");
        }
    }

    #[test]
    fn stored_blocks_and_their_complement_check() {
        let mut raw = vec![0x01, 3, 0, !3, 0xff, b'a', b'b', b'c'];
        assert_eq!(inflate(&raw, 10).unwrap(), b"abc");
        // Final stored block after a non-final one.
        let mut two = vec![0x00, 1, 0, !1, 0xff, b'x'];
        two.extend_from_slice(&[0x01, 2, 0, !2, 0xff, b'y', b'z']);
        assert_eq!(inflate(&two, 10).unwrap(), b"xyz");
        raw[3] = 0;
        assert_eq!(inflate(&raw, 10), Err(InflateError::Data), "LEN/NLEN mismatch");
    }

    #[test]
    fn zlib_wrapper_is_checked_and_the_output_cap_is_enforced() {
        let raw = deflate(&[7u8; 10_000], 6);
        let mut z = vec![0x78, 0x9c];
        z.extend_from_slice(&raw);
        assert_eq!(zlib_inflate(&z, 10_000).unwrap().len(), 10_000);
        assert_eq!(zlib_inflate(&z, 9_999), Err(InflateError::TooLarge));
        assert_eq!(zlib_inflate(&[0x78, 0x9d], 10), Err(InflateError::Header), "bad FCHECK");
        assert_eq!(zlib_inflate(&[0x79, 0x9c], 10), Err(InflateError::Header), "not deflate");
        assert_eq!(zlib_inflate(&[0x78, 0xbb], 10), Err(InflateError::Header), "preset dictionary");
        assert_eq!(zlib_inflate(&z[..z.len() / 2], 1 << 20), Err(InflateError::Data), "truncated");
    }

    proptest! {
        #[test]
        fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
            let _ = inflate(&bytes, 1 << 16);
            let _ = zlib_inflate(&bytes, 1 << 16);
        }

        #[test]
        fn roundtrip_of_random_compressible_data(seed in proptest::collection::vec(0u8..8, 0..3000)) {
            let raw = deflate(&seed, 6);
            prop_assert_eq!(inflate(&raw, 1 << 20).unwrap(), seed);
        }
    }
}
