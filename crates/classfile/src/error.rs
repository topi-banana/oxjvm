//! Parse errors for the class-file codec.

use core::fmt;

/// A failure to decode a JVM class file.
///
/// Every variant carries enough information to name the structure that failed, mirroring the
/// checked reads of JVMS ch. 4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// The stream ended before a required field was complete.
    UnexpectedEof {
        /// Human-readable name of the field or structure being read.
        context: &'static str,
        /// Number of bytes the reader still needed.
        needed: usize,
        /// Number of bytes that remained.
        remaining: usize,
    },
    /// The leading `u4` was not `0xCAFEBABE`.
    BadMagic(u32),
    /// A `CONSTANT_*` tag byte is not one of the seventeen defined tags.
    InvalidConstantTag {
        /// Index of the malformed pool entry.
        index: u16,
        /// The unknown tag.
        tag: u8,
    },
    /// `constant_pool_count` was zero, which JVMS forbids.
    EmptyConstantPool,
    /// A reference into the constant pool is out of range or points at the wrong kind of entry.
    BadConstantPoolIndex {
        /// The index that was used.
        index: u16,
        /// What the caller expected to find there.
        expected: &'static str,
    },
    /// A modified-UTF-8 string is malformed, or contains an unpaired surrogate.
    InvalidUtf8 {
        /// Index of the offending pool entry.
        index: u16,
    },
    /// A class file version this runtime does not model.
    UnsupportedVersion {
        /// The major version.
        major: u16,
        /// The minor version.
        minor: u16,
    },
    /// A field carries the wrong class-file version banner.
    Other(&'static str),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof {
                context,
                needed,
                remaining,
            } => write!(
                f,
                "unexpected end of class file while reading {context}: needed {needed} byte(s), {remaining} remaining"
            ),
            Self::BadMagic(magic) => write!(f, "bad class file magic: {magic:#010x}"),
            Self::InvalidConstantTag { index, tag } => {
                write!(f, "invalid constant pool tag {tag:#04x} at index {index}")
            }
            Self::EmptyConstantPool => f.write_str("constant_pool_count must be at least 1"),
            Self::BadConstantPoolIndex { index, expected } => {
                write!(f, "constant pool index {index} is not {expected}")
            }
            Self::InvalidUtf8 { index } => {
                write!(f, "constant pool entry {index} is not valid modified UTF-8")
            }
            Self::UnsupportedVersion { major, minor } => {
                write!(f, "unsupported class file version {major}.{minor}")
            }
            Self::Other(message) => f.write_str(message),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ParseError {}
