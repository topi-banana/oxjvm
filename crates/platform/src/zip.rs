//! A minimal, dependency-free ZIP reader for `.jar` files.
//!
//! Only reading is supported, and only what a JAR needs: central-directory discovery, STORE and
//! DEFLATE members, CRC-32 verification, and the UTF-8/ASCII subset of name encodings that class
//! entries use. ZIP64 is not modelled.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use crate::inflate;

/// A malformed ZIP archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZipError {
    /// The end-of-central-directory record was not found.
    NoCentralDirectory,
    /// A header signature or field was invalid.
    Malformed(&'static str),
    /// A member was compressed with an unsupported method.
    UnsupportedCompression(u16),
    /// CRC-32 verification failed.
    CrcMismatch {
        /// The name of the member.
        name: String,
        /// The CRC recorded in the archive.
        expected: u32,
        /// The CRC computed over the extracted bytes.
        actual: u32,
    },
    /// DEFLATE decompression failed.
    Inflate(inflate::InflateError),
    /// The member's name is not valid UTF-8/ASCII.
    BadName,
}

impl fmt::Display for ZipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoCentralDirectory => {
                f.write_str("zip end-of-central-directory record not found")
            }
            Self::Malformed(what) => write!(f, "malformed zip archive: {what}"),
            Self::UnsupportedCompression(method) => {
                write!(f, "unsupported zip compression method {method}")
            }
            Self::CrcMismatch {
                name,
                expected,
                actual,
            } => write!(
                f,
                "zip member {name} crc mismatch: expected {expected:#010x}, computed {actual:#010x}"
            ),
            Self::Inflate(error) => write!(f, "zip member could not be inflated: {error}"),
            Self::BadName => f.write_str("zip member name is not valid text"),
        }
    }
}

impl From<inflate::InflateError> for ZipError {
    fn from(error: inflate::InflateError) -> Self {
        Self::Inflate(error)
    }
}

/// One central-directory entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipEntry {
    /// The member name, slash-separated.
    pub name: String,
    /// Compression method (`0` store, `8` deflate).
    pub compression: u16,
    /// CRC-32 of the uncompressed bytes.
    pub crc32: u32,
    /// Compressed size in bytes.
    pub compressed_size: u32,
    /// Uncompressed size in bytes.
    pub uncompressed_size: u32,
    /// Offset of the local file header.
    pub local_header_offset: u32,
}

/// A parsed ZIP archive borrowing its bytes.
#[derive(Debug, Clone)]
pub struct ZipArchive<'a> {
    bytes: &'a [u8],
    entries: Vec<ZipEntry>,
}

const EOCD_SIGNATURE: u32 = 0x0605_4B50;
const CENTRAL_SIGNATURE: u32 = 0x0201_4B50;
const LOCAL_SIGNATURE: u32 = 0x0403_4B50;

impl<'a> ZipArchive<'a> {
    /// Parse an archive's central directory.
    ///
    /// # Errors
    ///
    /// Returns [`ZipError`] when the archive is malformed.
    pub fn new(bytes: &'a [u8]) -> Result<Self, ZipError> {
        let eocd = find_eocd(bytes).ok_or(ZipError::NoCentralDirectory)?;
        let entry_count = read_u16(bytes, eocd + 10)? as usize;
        let directory_size = read_u32(bytes, eocd + 12)? as usize;
        let directory_offset = read_u32(bytes, eocd + 16)? as usize;
        let directory_end = directory_offset
            .checked_add(directory_size)
            .filter(|&end| end <= bytes.len())
            .ok_or(ZipError::Malformed("central directory extends past end"))?;
        let mut entries = Vec::with_capacity(entry_count);
        let mut offset = directory_offset;
        while offset + 46 <= directory_end {
            if read_u32(bytes, offset)? != CENTRAL_SIGNATURE {
                return Err(ZipError::Malformed("central directory signature"));
            }
            let compression = read_u16(bytes, offset + 10)?;
            let crc32 = read_u32(bytes, offset + 16)?;
            let compressed_size = read_u32(bytes, offset + 20)?;
            let uncompressed_size = read_u32(bytes, offset + 24)?;
            let name_len = read_u16(bytes, offset + 28)? as usize;
            let extra_len = read_u16(bytes, offset + 30)? as usize;
            let comment_len = read_u16(bytes, offset + 32)? as usize;
            let local_header_offset = read_u32(bytes, offset + 42)?;
            let name_end = offset + 46 + name_len;
            if name_end > directory_end {
                return Err(ZipError::Malformed("central directory name"));
            }
            let name_bytes = &bytes[offset + 46..name_end];
            // Class-file names are ASCII; reject non-ASCII rather than guessing at CP437.
            let name = core::str::from_utf8(name_bytes)
                .map_err(|_| ZipError::BadName)?
                .to_string();
            entries.push(ZipEntry {
                name,
                compression,
                crc32,
                compressed_size,
                uncompressed_size,
                local_header_offset,
            });
            offset = name_end + extra_len + comment_len;
        }
        if entries.len() != entry_count {
            return Err(ZipError::Malformed("central directory entry count"));
        }
        Ok(Self { bytes, entries })
    }

    /// All members, in central-directory order.
    #[must_use]
    pub fn entries(&self) -> &[ZipEntry] {
        &self.entries
    }

    /// Find a member by exact name.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&ZipEntry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    /// Extract a member, verifying its CRC-32.
    ///
    /// # Errors
    ///
    /// Returns [`ZipError`] when the member is missing, malformed, or fails verification.
    pub fn read(&self, name: &str) -> Result<Option<Vec<u8>>, ZipError> {
        let Some(entry) = self.find(name) else {
            return Ok(None);
        };
        let data = self.read_entry(entry)?;
        Ok(Some(data))
    }

    /// Extract one entry, verifying its CRC-32.
    ///
    /// # Errors
    ///
    /// Returns [`ZipError`] when the entry is malformed or fails verification.
    pub fn read_entry(&self, entry: &ZipEntry) -> Result<Vec<u8>, ZipError> {
        let header = entry.local_header_offset as usize;
        if read_u32(self.bytes, header)? != LOCAL_SIGNATURE {
            return Err(ZipError::Malformed("local file header signature"));
        }
        let name_len = read_u16(self.bytes, header + 26)? as usize;
        let extra_len = read_u16(self.bytes, header + 28)? as usize;
        let data_start = header
            .checked_add(30 + name_len + extra_len)
            .ok_or(ZipError::Malformed("local file header length"))?;
        let data_end = data_start
            .checked_add(entry.compressed_size as usize)
            .filter(|&end| end <= self.bytes.len())
            .ok_or(ZipError::Malformed("member data extends past end"))?;
        let compressed = &self.bytes[data_start..data_end];
        let data = match entry.compression {
            0 => compressed.to_vec(),
            8 => inflate::inflate(compressed, entry.uncompressed_size as usize)?,
            other => return Err(ZipError::UnsupportedCompression(other)),
        };
        if data.len() != entry.uncompressed_size as usize {
            return Err(ZipError::Malformed("uncompressed size mismatch"));
        }
        let actual = crc32(&data);
        if actual != entry.crc32 {
            return Err(ZipError::CrcMismatch {
                name: entry.name.clone(),
                expected: entry.crc32,
                actual,
            });
        }
        Ok(data)
    }
}

/// CRC-32 (IEEE 802.3), computed bit-by-bit with the reversed polynomial `0xEDB88320`.
#[must_use]
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn find_eocd(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 22 {
        return None;
    }
    let start = bytes.len().saturating_sub(22 + 65_535);
    let mut offset = bytes.len() - 22;
    loop {
        if read_u32(bytes, offset).ok()? == EOCD_SIGNATURE {
            return Some(offset);
        }
        if offset == start {
            return None;
        }
        offset -= 1;
    }
}

fn read_u16(bytes: &[u8], at: usize) -> Result<u16, ZipError> {
    let slice = bytes
        .get(at..at + 2)
        .ok_or(ZipError::Malformed("truncated field"))?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], at: usize) -> Result<u32, ZipError> {
    let slice = bytes
        .get(at..at + 4)
        .ok_or(ZipError::Malformed("truncated field"))?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}
