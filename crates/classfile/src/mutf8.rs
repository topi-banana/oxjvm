//! Java *modified UTF-8* (`CONSTANT_Utf8`, JVMS 4.4.7).
//!
//! Modified UTF-8 differs from UTF-8 in exactly two ways:
//!
//! * the null code unit `U+0000` is encoded as the two bytes `0xC0 0x80`, never as `0x00`; and
//! * supplementary characters are each encoded as a *pair* of three-byte sequences — one per
//!   UTF-16 surrogate — instead of a single four-byte sequence.
//!
//! The decoder therefore produces a Rust `String` (which cannot hold lone surrogates) and the
//! encoder reproduces the canonical Java encoding for every scalar value, including lonely
//! surrogate halves by transcoding them to the replacement character would be wrong: such input is
//! rejected as malformed instead of silently rewritten.

use alloc::string::String;
use alloc::vec::Vec;

use crate::error::ParseError;

/// Decode modified UTF-8 into a Rust string.
///
/// # Errors
///
/// Returns [`ParseError::InvalidUtf8`] when `bytes` is not well-formed modified UTF-8, or when it
/// contains an unpaired UTF-16 surrogate (which a Rust `String` cannot represent).
pub fn decode(bytes: &[u8], index: u16) -> Result<String, ParseError> {
    let mut out = String::with_capacity(bytes.len());
    let mut units: Vec<u16> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i];
        let (unit, len) = if b0 & 0x80 == 0 {
            if b0 == 0 {
                return Err(ParseError::InvalidUtf8 { index });
            }
            (u16::from(b0), 1)
        } else if b0 & 0xE0 == 0xC0 {
            (u16::from(b0 & 0x1F) << 6, 2)
        } else if b0 & 0xF0 == 0xE0 {
            (u16::from(b0 & 0x0F) << 12, 3)
        } else {
            return Err(ParseError::InvalidUtf8 { index });
        };
        if i + len > bytes.len() {
            return Err(ParseError::InvalidUtf8 { index });
        }
        let mut unit = unit;
        for k in 1..len {
            let b = bytes[i + k];
            if b & 0xC0 != 0x80 {
                return Err(ParseError::InvalidUtf8 { index });
            }
            unit |= u16::from(b & 0x3F) << (6 * (len - 1 - k));
        }
        units.push(unit);
        i += len;
    }

    // Combine CESU-8 surrogate pairs; reject lone surrogates.
    let mut k = 0;
    while k < units.len() {
        let unit = units[k];
        if (0xD800..0xDC00).contains(&unit) {
            let Some(&low) = units.get(k + 1) else {
                return Err(ParseError::InvalidUtf8 { index });
            };
            if !(0xDC00..0xE000).contains(&low) {
                return Err(ParseError::InvalidUtf8 { index });
            }
            let scalar = 0x1_0000 + ((u32::from(unit) - 0xD800) << 10) + (u32::from(low) - 0xDC00);
            let Some(ch) = char::from_u32(scalar) else {
                return Err(ParseError::InvalidUtf8 { index });
            };
            out.push(ch);
            k += 2;
        } else if (0xDC00..0xE000).contains(&unit) {
            return Err(ParseError::InvalidUtf8 { index });
        } else {
            let Some(ch) = char::from_u32(u32::from(unit)) else {
                return Err(ParseError::InvalidUtf8 { index });
            };
            out.push(ch);
            k += 1;
        }
    }
    Ok(out)
}

/// Encode a Rust string as Java modified UTF-8.
#[must_use]
pub fn encode(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for ch in text.chars() {
        let code = ch as u32;
        if code == 0 {
            out.extend_from_slice(&[0xC0, 0x80]);
        } else if code < 0x80 {
            out.push(code as u8);
        } else if code < 0x800 {
            out.push(0xC0 | (code >> 6) as u8);
            out.push(0x80 | (code & 0x3F) as u8);
        } else if code < 0x1_0000 {
            out.push(0xE0 | (code >> 12) as u8);
            out.push(0x80 | ((code >> 6) & 0x3F) as u8);
            out.push(0x80 | (code & 0x3F) as u8);
        } else {
            let v = code - 0x1_0000;
            let high = 0xD800 + ((v >> 10) as u16);
            let low = 0xDC00 + ((v & 0x3FF) as u16);
            for unit in [high, low] {
                let unit = u32::from(unit);
                out.push(0xE0 | (unit >> 12) as u8);
                out.push(0x80 | ((unit >> 6) & 0x3F) as u8);
                out.push(0x80 | (unit & 0x3F) as u8);
            }
        }
    }
    out
}

/// Decode modified UTF-8 leniently, mapping unpaired surrogates to `U+FFFD`.
///
/// Used only where a best-effort display string is wanted (diagnostics); the strict decoder
/// remains the one on the parse path.
#[must_use]
pub fn decode_lossy(bytes: &[u8]) -> String {
    match decode(bytes, 0) {
        Ok(text) => text,
        Err(_) => {
            let mut out = String::new();
            let mut i = 0;
            while i < bytes.len() {
                let b = bytes[i];
                if b & 0x80 == 0 {
                    if b == 0 {
                        out.push('\u{FFFD}');
                    } else {
                        out.push(b as char);
                    }
                    i += 1;
                } else if b & 0xE0 == 0xC0 && i + 1 < bytes.len() {
                    let c = ((u16::from(b & 0x1F) << 6) | u16::from(bytes[i + 1] & 0x3F)) as u32;
                    if let Some(ch) = char::from_u32(c) {
                        out.push(ch);
                    } else {
                        out.push('\u{FFFD}');
                    }
                    i += 2;
                } else if b & 0xF0 == 0xE0 && i + 2 < bytes.len() {
                    let c = ((u32::from(b & 0x0F) << 12)
                        | (u32::from(bytes[i + 1] & 0x3F) << 6)
                        | u32::from(bytes[i + 2] & 0x3F)) as u32;
                    if let Some(ch) = char::from_u32(c) {
                        out.push(ch);
                    } else {
                        out.push('\u{FFFD}');
                    }
                    i += 3;
                } else {
                    out.push('\u{FFFD}');
                    i += 1;
                }
            }
            out
        }
    }
}
