//! A raw DEFLATE (RFC 1951) decompressor, written from the specification and kept dependency-free
//! so `no_std` wasm hosts can open JAR entries.
//!
//! Only decompression is implemented — the runtime never produces archives.

use alloc::vec::Vec;
use core::fmt;

/// A malformed DEFLATE stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InflateError {
    /// The stream ended in the middle of a block.
    Truncated,
    /// A stored-block length field disagreed with its one's complement.
    StoredLengthMismatch,
    /// A dynamic-Huffman code description was invalid.
    BadCodeDescription,
    /// A Huffman code was over-subscribed or incomplete.
    BadHuffmanCode,
    /// A length/distance symbol was invalid.
    BadSymbol,
    /// A built-in guard tripped (e.g. a distance beyond the window).
    InvalidDistance,
    /// The caller's capacity limit was exceeded.
    OutputLimit,
}

impl fmt::Display for InflateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Truncated => "deflate stream truncated",
            Self::StoredLengthMismatch => "deflate stored block length mismatch",
            Self::BadCodeDescription => "deflate dynamic Huffman description invalid",
            Self::BadHuffmanCode => "deflate Huffman code invalid",
            Self::BadSymbol => "deflate symbol invalid",
            Self::InvalidDistance => "deflate back-reference out of window",
            Self::OutputLimit => "deflate output exceeds the caller's limit",
        })
    }
}

/// Decompress a raw DEFLATE stream.
///
/// `limit` caps the output size in bytes (the zip reader passes the central directory's
/// uncompressed size, which must be exact).
///
/// # Errors
///
/// Returns [`InflateError`] on any malformed input or when the output would exceed `limit`.
pub fn inflate(data: &[u8], limit: usize) -> Result<Vec<u8>, InflateError> {
    let mut reader = BitReader::new(data);
    let mut out = Vec::with_capacity(limit.min(1 << 20));
    loop {
        let final_block = reader.bits(1)? == 1;
        match reader.bits(2)? {
            0 => inflate_stored(&mut reader, &mut out, limit)?,
            1 => {
                let (lit, dist) = fixed_tables();
                inflate_huffman(&mut reader, &mut out, limit, &lit, &dist)?;
            }
            2 => {
                let (lit, dist) = dynamic_tables(&mut reader)?;
                inflate_huffman(&mut reader, &mut out, limit, &lit, &dist)?;
            }
            _ => return Err(InflateError::BadSymbol),
        }
        if final_block {
            return Ok(out);
        }
    }
}

/// A canonical Huffman decoding table.
struct Huffman {
    /// `counts[len]` is the number of codes of bit length `len`.
    counts: [u16; 16],
    /// Symbols ordered by code.
    symbols: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Result<Self, InflateError> {
        let mut counts = [0u16; 16];
        for &len in lengths {
            if len > 15 {
                return Err(InflateError::BadHuffmanCode);
            }
            counts[usize::from(len)] += 1;
        }
        counts[0] = 0;
        let mut left = 1i32;
        for count in counts.iter().skip(1) {
            left <<= 1;
            left -= i32::from(*count);
            if left < 0 {
                return Err(InflateError::BadHuffmanCode);
            }
        }
        let mut offsets = [0u16; 16];
        for len in 1..15 {
            offsets[len + 1] = offsets[len] + counts[len];
        }
        let mut symbols = alloc::vec![0u16; lengths.iter().filter(|&&l| l != 0).count()];
        for (symbol, &len) in lengths.iter().enumerate() {
            if len != 0 {
                symbols[usize::from(offsets[usize::from(len)])] = symbol as u16;
                offsets[usize::from(len)] += 1;
            }
        }
        Ok(Self { counts, symbols })
    }

    fn decode(&self, reader: &mut BitReader<'_>) -> Result<u16, InflateError> {
        let mut code = 0i32;
        let mut first = 0i32;
        let mut index = 0i32;
        for len in 1..16 {
            code |= reader.bits(1)? as i32;
            let count = i32::from(self.counts[len]);
            if code - first < count {
                let symbol = self.symbols[(index + (code - first)) as usize];
                return Ok(symbol);
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err(InflateError::BadHuffmanCode)
    }
}

const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

fn fixed_tables() -> (Huffman, Huffman) {
    let mut lit_lengths = [0u8; 288];
    for (symbol, length) in lit_lengths.iter_mut().enumerate() {
        *length = match symbol {
            0..=143 => 8,
            144..=255 => 9,
            256..=279 => 7,
            _ => 8,
        };
    }
    let lit = Huffman::new(&lit_lengths).expect("fixed literal table is valid");
    let dist = Huffman::new(&[5u8; 30]).expect("fixed distance table is valid");
    (lit, dist)
}

fn dynamic_tables(reader: &mut BitReader<'_>) -> Result<(Huffman, Huffman), InflateError> {
    const ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];
    let hlit = reader.bits(5)? as usize + 257;
    let hdist = reader.bits(5)? as usize + 1;
    let hclen = reader.bits(4)? as usize + 4;
    if hlit > 286 || hdist > 30 {
        return Err(InflateError::BadCodeDescription);
    }
    let mut code_lengths = [0u8; 19];
    for &index in ORDER.iter().take(hclen) {
        code_lengths[index] = reader.bits(3)? as u8;
    }
    let code_table = Huffman::new(&code_lengths)?;
    let mut lengths = alloc::vec![0u8; hlit + hdist];
    let mut i = 0;
    while i < lengths.len() {
        let symbol = code_table.decode(reader)?;
        match symbol {
            0..=15 => {
                lengths[i] = symbol as u8;
                i += 1;
            }
            16 => {
                if i == 0 {
                    return Err(InflateError::BadCodeDescription);
                }
                let repeat = 3 + reader.bits(2)? as usize;
                let previous = lengths[i - 1];
                for _ in 0..repeat {
                    if i >= lengths.len() {
                        return Err(InflateError::BadCodeDescription);
                    }
                    lengths[i] = previous;
                    i += 1;
                }
            }
            17 => {
                let repeat = 3 + reader.bits(3)? as usize;
                i = i
                    .checked_add(repeat)
                    .filter(|&end| end <= lengths.len())
                    .ok_or(InflateError::BadCodeDescription)?;
            }
            18 => {
                let repeat = 11 + reader.bits(7)? as usize;
                i = i
                    .checked_add(repeat)
                    .filter(|&end| end <= lengths.len())
                    .ok_or(InflateError::BadCodeDescription)?;
            }
            _ => return Err(InflateError::BadCodeDescription),
        }
    }
    let lit = Huffman::new(&lengths[..hlit])?;
    let dist = Huffman::new(&lengths[hlit..])?;
    Ok((lit, dist))
}

fn inflate_stored(
    reader: &mut BitReader<'_>,
    out: &mut Vec<u8>,
    limit: usize,
) -> Result<(), InflateError> {
    reader.align();
    let len = reader.u16_le()? as usize;
    let complement = reader.u16_le()? as usize;
    if len != (!complement & 0xFFFF) {
        return Err(InflateError::StoredLengthMismatch);
    }
    if out.len() + len > limit {
        return Err(InflateError::OutputLimit);
    }
    let bytes = reader.take(len)?;
    out.extend_from_slice(bytes);
    Ok(())
}

fn inflate_huffman(
    reader: &mut BitReader<'_>,
    out: &mut Vec<u8>,
    limit: usize,
    lit: &Huffman,
    dist: &Huffman,
) -> Result<(), InflateError> {
    loop {
        let symbol = lit.decode(reader)?;
        match symbol {
            0..=255 => {
                if out.len() >= limit {
                    return Err(InflateError::OutputLimit);
                }
                out.push(symbol as u8);
            }
            256 => return Ok(()),
            257..=285 => {
                let index = usize::from(symbol - 257);
                let length =
                    usize::from(LENGTH_BASE[index]) + reader.bits(LENGTH_EXTRA[index])? as usize;
                let distance_symbol = dist.decode(reader)?;
                let distance_index = usize::from(distance_symbol);
                if distance_index >= DIST_BASE.len() {
                    return Err(InflateError::BadSymbol);
                }
                let distance = usize::from(DIST_BASE[distance_index])
                    + reader.bits(DIST_EXTRA[distance_index])? as usize;
                if distance == 0 || distance > out.len() {
                    return Err(InflateError::InvalidDistance);
                }
                if out.len() + length > limit {
                    return Err(InflateError::OutputLimit);
                }
                let start = out.len() - distance;
                for k in 0..length {
                    let byte = out[start + k];
                    out.push(byte);
                }
            }
            _ => return Err(InflateError::BadSymbol),
        }
    }
}

struct BitReader<'a> {
    data: &'a [u8],
    position: usize,
    bit: u32,
    value: u32,
}

impl<'a> BitReader<'a> {
    const fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            position: 0,
            bit: 0,
            value: 0,
        }
    }

    fn bits(&mut self, count: u8) -> Result<u32, InflateError> {
        while self.bit < u32::from(count) {
            let byte = *self
                .data
                .get(self.position)
                .ok_or(InflateError::Truncated)?;
            self.position += 1;
            self.value |= u32::from(byte) << self.bit;
            self.bit += 8;
        }
        let result = self.value & ((1 << count) - 1);
        self.value >>= count;
        self.bit -= u32::from(count);
        Ok(result)
    }

    fn align(&mut self) {
        self.value = 0;
        self.bit = 0;
    }

    fn u16_le(&mut self) -> Result<u16, InflateError> {
        let lo = self.bits(8)? as u16;
        let hi = self.bits(8)? as u16;
        Ok(lo | (hi << 8))
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], InflateError> {
        let end = self
            .position
            .checked_add(len)
            .ok_or(InflateError::Truncated)?;
        let slice = self
            .data
            .get(self.position..end)
            .ok_or(InflateError::Truncated)?;
        self.position = end;
        Ok(slice)
    }
}
