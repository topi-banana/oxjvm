//! Big-endian primitives for the class-file codec.

use alloc::vec::Vec;

use crate::error::ParseError;

/// A bounds-checked, big-endian cursor over class-file bytes.
#[derive(Debug, Clone)]
pub struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    /// Wrap a byte slice.
    #[must_use]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    /// Current byte offset.
    #[must_use]
    pub const fn position(&self) -> usize {
        self.pos
    }

    /// Bytes not yet consumed.
    #[must_use]
    pub const fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }

    /// Whether every byte has been consumed.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.pos == self.bytes.len()
    }

    fn take(&mut self, len: usize, context: &'static str) -> Result<&'a [u8], ParseError> {
        if self.remaining() < len {
            return Err(ParseError::UnexpectedEof {
                context,
                needed: len,
                remaining: self.remaining(),
            });
        }
        let slice = &self.bytes[self.pos..self.pos + len];
        self.pos += len;
        Ok(slice)
    }

    /// Read one unsigned byte.
    pub fn u1(&mut self, context: &'static str) -> Result<u8, ParseError> {
        Ok(self.take(1, context)?[0])
    }

    /// Read one signed byte.
    pub fn i1(&mut self, context: &'static str) -> Result<i8, ParseError> {
        Ok(self.u1(context)? as i8)
    }

    /// Read a big-endian `u2`.
    pub fn u2(&mut self, context: &'static str) -> Result<u16, ParseError> {
        let b = self.take(2, context)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    /// Read a big-endian `i2`.
    pub fn i2(&mut self, context: &'static str) -> Result<i16, ParseError> {
        Ok(self.u2(context)? as i16)
    }

    /// Read a big-endian `u4`.
    pub fn u4(&mut self, context: &'static str) -> Result<u32, ParseError> {
        let b = self.take(4, context)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// Read a big-endian `i4`.
    pub fn i4(&mut self, context: &'static str) -> Result<i32, ParseError> {
        Ok(self.u4(context)? as i32)
    }

    /// Read a big-endian `u8`.
    pub fn u8(&mut self, context: &'static str) -> Result<u64, ParseError> {
        let b = self.take(8, context)?;
        Ok(u64::from_be_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    /// Read a big-endian `i8`.
    pub fn i8(&mut self, context: &'static str) -> Result<i64, ParseError> {
        Ok(self.u8(context)? as i64)
    }

    /// Read exactly `len` bytes.
    pub fn bytes(&mut self, len: usize, context: &'static str) -> Result<&'a [u8], ParseError> {
        self.take(len, context)
    }

    /// Borrow all remaining bytes without consuming them.
    pub fn peek_rest(&self) -> &'a [u8] {
        &self.bytes[self.pos..]
    }

    /// Consume all remaining bytes.
    pub fn rest(&mut self) -> &'a [u8] {
        let slice = &self.bytes[self.pos..];
        self.pos = self.bytes.len();
        slice
    }

    /// Consume exactly `len` bytes into an owned vector.
    pub fn vec(&mut self, len: usize, context: &'static str) -> Result<Vec<u8>, ParseError> {
        Ok(self.take(len, context)?.to_vec())
    }
}

/// A growable big-endian output buffer.
#[derive(Debug, Default, Clone)]
pub struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    /// Create an empty writer.
    #[must_use]
    pub const fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    /// Create a writer with pre-allocated capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(capacity),
        }
    }

    /// Number of bytes written so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether nothing has been written.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Write a `u1`.
    pub fn u1(&mut self, value: u8) {
        self.bytes.push(value);
    }

    /// Write an `i1`.
    pub fn i1(&mut self, value: i8) {
        self.bytes.push(value as u8);
    }

    /// Write a big-endian `u2`.
    pub fn u2(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    /// Write a big-endian `i2`.
    pub fn i2(&mut self, value: i16) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    /// Write a big-endian `u4`.
    pub fn u4(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    /// Write a big-endian `i4`.
    pub fn i4(&mut self, value: i32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    /// Write a big-endian `u8`.
    pub fn u8(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    /// Write a big-endian `i8`.
    pub fn i8(&mut self, value: i64) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    /// Write raw bytes.
    pub fn bytes(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }

    /// Finish and return the buffer.
    #[must_use]
    pub fn into_vec(self) -> Vec<u8> {
        self.bytes
    }

    /// Borrow the buffer.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }
}
