#![no_std]
#![deny(unsafe_code)]
//! Host boundaries for the `oxjvm` runtime, plus the pure-Rust archive utilities the hosts need.
//!
//! The VM is a pure in-memory machine: it never opens a file, reads a clock, or prints a byte on
//! its own. Everything outside the heap arrives through [`Host`], which the CLI, the wasm entry
//! points, and the test suite implement. The two archive modules ([`inflate`], [`zip`]) are pure
//! algorithms kept here so every host — including `wasm32-unknown-unknown` — can read `.jar`
//! files without depending on a filesystem or a decompression library.

extern crate alloc;

pub mod inflate;
pub mod zip;

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// The output stream a write targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    /// `System.out`.
    Stdout,
    /// `System.err`.
    Stderr,
}

/// A host-side failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    /// An I/O operation failed; the message is for diagnostics only.
    Io(String),
    /// The guest requested termination with this status (`System.exit`).
    Exit(i32),
    /// The operation is not supported by this host.
    Unsupported(&'static str),
}

impl core::fmt::Display for HostError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io(message) => write!(f, "host I/O error: {message}"),
            Self::Exit(status) => write!(f, "guest requested exit with status {status}"),
            Self::Unsupported(what) => write!(f, "host does not support {what}"),
        }
    }
}

/// The environment the VM runs against.
///
/// Every method has a conservative default so a minimal in-memory host (tests, wasm) implements
/// only what its programs use.
pub trait Host {
    /// Load the class file for an internal name (`java/lang/String`).
    ///
    /// Returning `None` means "not found on this class path"; the VM turns that into
    /// `ClassNotFoundException`/`NoClassDefFoundError` at the guest boundary.
    fn load_class(&mut self, internal_name: &str) -> Option<Vec<u8>> {
        let _ = internal_name;
        None
    }

    /// Write bytes to one of the standard streams.
    ///
    /// # Errors
    ///
    /// Returns [`HostError`] when the underlying stream fails.
    fn write(&mut self, stream: Stream, bytes: &[u8]) -> Result<(), HostError> {
        let _ = (stream, bytes);
        Ok(())
    }

    /// Flush one of the standard streams.
    ///
    /// # Errors
    ///
    /// Returns [`HostError`] when the underlying stream fails.
    fn flush(&mut self, stream: Stream) -> Result<(), HostError> {
        let _ = stream;
        Ok(())
    }

    /// Wall-clock milliseconds since the Unix epoch.
    fn current_time_millis(&mut self) -> i64 {
        0
    }

    /// A high-resolution monotonic nanosecond counter.
    fn nano_time(&mut self) -> i64 {
        0
    }

    /// Sleep for a while, if the host can. Cooperative threads treat this as a scheduling hint.
    ///
    /// # Errors
    ///
    /// Returns [`HostError`] when sleeping fails.
    fn sleep_millis(&mut self, millis: i64) -> Result<(), HostError> {
        let _ = millis;
        Ok(())
    }

    /// Terminate the guest with a status. The default reports the status through
    /// [`HostError::Exit`], which the VM propagates as [`crate`]-level termination.
    ///
    /// # Errors
    ///
    /// Always returns an error carrying the status by default.
    fn exit(&mut self, status: i32) -> Result<(), HostError> {
        Err(HostError::Exit(status))
    }

    /// A seed for `Math.random`/`java.util.Random`.
    fn random_seed(&mut self) -> u64 {
        0x9E37_79B9_7F4A_7C15
    }

    /// A system property value (`System.getProperty`), if the host knows it.
    fn property(&mut self, key: &str) -> Option<String> {
        let _ = key;
        None
    }
}

/// An in-memory class path, keyed by internal class name.
#[derive(Debug, Default, Clone)]
pub struct MemoryClasses {
    classes: BTreeMap<String, Vec<u8>>,
}

impl MemoryClasses {
    /// An empty class path.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert class-file bytes under an internal name.
    pub fn insert(&mut self, internal_name: &str, bytes: Vec<u8>) {
        self.classes.insert(internal_name.into(), bytes);
    }

    /// Insert class-file bytes under the name stored inside the class file itself.
    ///
    /// # Errors
    ///
    /// Returns a parse error when the header cannot be read.
    pub fn insert_parsed(&mut self, bytes: Vec<u8>) -> Result<(), oxjvm_classfile::ParseError> {
        let class = oxjvm_classfile::ClassFile::read(&bytes)?;
        let name = class.this_name()?.to_string();
        self.classes.insert(name, bytes);
        Ok(())
    }

    /// Look up a class by internal name.
    #[must_use]
    pub fn get(&self, internal_name: &str) -> Option<&[u8]> {
        self.classes.get(internal_name).map(Vec::as_slice)
    }

    /// Number of classes held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.classes.len()
    }

    /// Whether the class path is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.classes.is_empty()
    }
}

impl Host for MemoryClasses {
    fn load_class(&mut self, internal_name: &str) -> Option<Vec<u8>> {
        self.classes.get(internal_name).cloned()
    }
}
