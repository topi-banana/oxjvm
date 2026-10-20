//! The filesystem boundary behind `java.io` and `java.nio.file`.
//!
//! The VM is `no_std` and never touches the operating system itself; hosts that have a real
//! filesystem implement [`FileSystem`] and return it from [`Host::filesystem`](crate::Host::
//! filesystem). Every method has a conservative default, so a host without a filesystem simply
//! inherits "not found".

use alloc::string::String;
use alloc::vec::Vec;

/// What a path denotes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    /// A regular file.
    File,
    /// A directory.
    Directory,
    /// Something else (socket, device, ...).
    Other,
}

/// The result of a `stat`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileMetadata {
    /// What the path denotes.
    pub kind: FileKind,
    /// Size in bytes.
    pub len: u64,
    /// Last modification time, milliseconds since the Unix epoch (0 when unknown).
    pub modified_millis: i64,
    /// Whether the path is read-only.
    pub readonly: bool,
}

/// Filesystem operations a host can provide to the guest.
pub trait FileSystem {
    /// `stat` a path; `None` when it does not exist or cannot be inspected.
    fn metadata(&mut self, _path: &str) -> Option<FileMetadata> {
        None
    }

    /// Read a file's contents; `None` when it cannot be read.
    fn read(&mut self, _path: &str) -> Option<Vec<u8>> {
        None
    }

    /// Write (or, with `append`, append) bytes; returns whether the write succeeded.
    fn write(&mut self, _path: &str, _bytes: &[u8], _append: bool) -> bool {
        false
    }

    /// List a directory's entry names; `None` when it is not a directory.
    fn list(&mut self, _path: &str) -> Option<Vec<String>> {
        None
    }

    /// Create one directory; false when the parent is missing or creation fails.
    fn create_dir(&mut self, _path: &str) -> bool {
        false
    }

    /// Create a directory and any missing parents.
    fn create_dir_all(&mut self, _path: &str) -> bool {
        false
    }

    /// Remove a file.
    fn remove_file(&mut self, _path: &str) -> bool {
        false
    }

    /// Remove an empty directory.
    fn remove_dir(&mut self, _path: &str) -> bool {
        false
    }

    /// Rename (or move) a path.
    fn rename(&mut self, _from: &str, _to: &str) -> bool {
        false
    }

    /// Resolve to an absolute, symlink-free path; `None` when it cannot be resolved.
    fn canonicalize(&mut self, _path: &str) -> Option<String> {
        None
    }

    /// Set the last modification time, milliseconds since the Unix epoch.
    fn set_modified(&mut self, _path: &str, _millis: i64) -> bool {
        false
    }

    /// Create a symbolic link at `link` pointing at `target`.
    fn symlink(&mut self, _link: &str, _target: &str) -> bool {
        false
    }

    /// Read the target of a symbolic link.
    fn read_link(&mut self, _path: &str) -> Option<String> {
        None
    }

    /// Create a hard link at `link` for `target`.
    fn hard_link(&mut self, _link: &str, _target: &str) -> bool {
        false
    }
}

/// A [`FileSystem`] with every operation denied.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoFileSystem;

impl FileSystem for NoFileSystem {}
