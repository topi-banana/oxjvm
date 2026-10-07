//! The in-house zip reader and inflate guard rails, built from hand-written archive bytes.

use oxjvm_platform::inflate::{InflateError, inflate};
use oxjvm_platform::zip::{ZipArchive, ZipError, crc32};

/// Build a stored-method ZIP with the given members.
fn stored_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let offset = out.len() as u32;
        let crc = crc32(data);
        // Local file header.
        out.extend_from_slice(&0x0403_4B50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes()); // version
        out.extend_from_slice(&0u16.to_le_bytes()); // flags
        out.extend_from_slice(&0u16.to_le_bytes()); // stored
        out.extend_from_slice(&0u16.to_le_bytes()); // time
        out.extend_from_slice(&0u16.to_le_bytes()); // date
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        // Central directory entry.
        central.extend_from_slice(&0x0201_4B50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes()); // version made by
        central.extend_from_slice(&20u16.to_le_bytes()); // version needed
        central.extend_from_slice(&0u16.to_le_bytes()); // flags
        central.extend_from_slice(&0u16.to_le_bytes()); // stored
        central.extend_from_slice(&0u16.to_le_bytes()); // time
        central.extend_from_slice(&0u16.to_le_bytes()); // date
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes()); // extra
        central.extend_from_slice(&0u16.to_le_bytes()); // comment
        central.extend_from_slice(&0u16.to_le_bytes()); // disk
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
        central.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let directory_offset = out.len() as u32;
    let directory_size = central.len() as u32;
    out.extend_from_slice(&central);
    // End of central directory.
    out.extend_from_slice(&0x0605_4B50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&directory_size.to_le_bytes());
    out.extend_from_slice(&directory_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[test]
fn reads_stored_members_and_verifies_crc() {
    let archive_bytes = stored_zip(&[
        ("demo/Hello.class", b"first"),
        ("demo/Other.class", b"second payload"),
    ]);
    let archive = ZipArchive::new(&archive_bytes).expect("parse");
    assert_eq!(archive.entries().len(), 2);
    assert_eq!(
        archive.read("demo/Hello.class").expect("read"),
        Some(b"first".to_vec())
    );
    assert_eq!(
        archive.read("demo/Other.class").expect("read"),
        Some(b"second payload".to_vec())
    );
    assert_eq!(archive.read("missing").expect("read"), None);
}

#[test]
fn detects_a_corrupt_member() {
    let mut archive_bytes = stored_zip(&[("A.class", b"hello")]);
    // Flip a payload byte so the CRC no longer matches.
    let position = archive_bytes
        .windows(5)
        .position(|window| window == b"hello")
        .expect("payload");
    archive_bytes[position] ^= 0xFF;
    let archive = ZipArchive::new(&archive_bytes).expect("parse");
    match archive.read("A.class") {
        Err(ZipError::CrcMismatch { name, .. }) => assert_eq!(name, "A.class"),
        other => panic!("expected a CRC mismatch, got {other:?}"),
    }
}

#[test]
fn rejects_non_archives() {
    assert!(matches!(
        ZipArchive::new(b"not a zip"),
        Err(ZipError::NoCentralDirectory)
    ));
}

#[test]
fn inflate_reports_truncation() {
    // A dynamic-Huffman block header with no code description behind it.
    assert!(matches!(
        inflate(&[0b0000_0101], 1024),
        Err(InflateError::Truncated | InflateError::BadHuffmanCode)
    ));
    // A stored block claiming more bytes than exist.
    assert!(matches!(
        inflate(&[0b0000_0001, 0x05, 0x00, 0xFA, 0xFF], 1024),
        Err(InflateError::Truncated)
    ));
}
