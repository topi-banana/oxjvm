//! Byte-exact round-trips over real `javac` output, plus structural negative tests.
//!
//! The fixtures are class files produced by `javac` (JDK 17/21) and copied from the sibling
//! `jals-classpath` test corpus. Any parser that is *not* byte-exact, or that mis-handles one of
//! the standard attributes these files carry, fails here.

use std::fs;
use std::path::{Path, PathBuf};

use oxjvm_classfile::ClassFile;

fn fixture_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).expect("fixtures directory") {
            let entry = entry.expect("fixture entry");
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "class") {
                out.push(path);
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut out = Vec::new();
    walk(&root, &mut out);
    out.sort();
    assert!(!out.is_empty(), "no fixtures found");
    out
}

#[test]
fn round_trips_every_fixture_byte_for_byte() {
    for path in fixture_files() {
        let bytes = fs::read(&path).expect("read fixture");
        let class = ClassFile::read(&bytes).unwrap_or_else(|error| {
            panic!("{} failed to parse: {error}", path.display());
        });
        let rewritten = class.write();
        assert_eq!(
            rewritten.len(),
            bytes.len(),
            "{} changed length",
            path.display()
        );
        assert_eq!(
            rewritten,
            bytes,
            "{} did not round-trip byte-exactly",
            path.display()
        );
    }
}

#[test]
fn round_trips_a_second_time_stably() {
    for path in fixture_files() {
        let bytes = fs::read(&path).expect("read fixture");
        let first = ClassFile::read(&bytes).expect("parse").write();
        let second = ClassFile::read(&first).expect("reparse").write();
        assert_eq!(first, second, "{} reparse unstable", path.display());
    }
}

#[test]
fn reports_all_fixture_class_names() {
    let mut names = Vec::new();
    for path in fixture_files() {
        let bytes = fs::read(&path).expect("read fixture");
        let class = ClassFile::read(&bytes).expect("parse");
        names.push(class.this_name().expect("this name").to_string());
    }
    assert!(names.iter().any(|name| name == "demo/Concat"));
    assert!(names.iter().any(|name| name == "demo/Tries"));
}

#[test]
fn rejects_bad_magic() {
    let error = ClassFile::read(&[0xDE, 0xAD, 0xBE, 0xEF]).unwrap_err();
    assert!(matches!(error, oxjvm_classfile::ParseError::BadMagic(_)));
}

#[test]
fn rejects_truncated_input() {
    let path = fixture_files().into_iter().next().expect("a fixture");
    let bytes = fs::read(path).expect("read fixture");
    for cut in [1usize, 4, 8, 9, 16] {
        let error = ClassFile::read(&bytes[..cut]).unwrap_err();
        assert!(
            matches!(error, oxjvm_classfile::ParseError::UnexpectedEof { .. }),
            "cut at {cut} gave {error:?}"
        );
    }
}

#[test]
fn decodes_every_instruction_of_every_fixture() {
    for path in fixture_files() {
        let bytes = fs::read(&path).expect("read fixture");
        let class = ClassFile::read(&bytes).expect("parse");
        for method in &class.methods {
            let Some(code) = method.attributes.iter().find_map(|a| match &a.data {
                oxjvm_classfile::AttributeData::Code(code) => Some(code),
                _ => None,
            }) else {
                continue;
            };
            let mut pc = 0;
            while pc < code.code.len() {
                let decoded = oxjvm_classfile::opcode::decode(&code.code, pc)
                    .unwrap_or_else(|| panic!("{} undecodable at {pc}", path.display()));
                assert!(decoded.next_pc > pc);
                pc = decoded.next_pc;
            }
            assert_eq!(pc, code.code.len(), "{} trailing bytes", path.display());
        }
    }
}

#[test]
fn modified_utf8_round_trips_every_shape() {
    use oxjvm_classfile::mutf8::{decode, encode};
    let cases = [
        "",
        "ascii",
        "null\u{0000}inside",
        "日本語",
        "emoji 😀 works",
        "\u{FFFF}",
        "\u{10000}",
        "mixed: a\u{0000}日😀",
    ];
    for text in cases {
        let bytes = encode(text);
        let decoded = decode(&bytes, 0).expect("decode");
        assert_eq!(decoded, text, "{text:?}");
        assert_eq!(encode(&decoded), bytes, "{text:?} re-encode");
    }
    // A lone NUL byte is not valid modified UTF-8.
    assert!(decode(&[0x00], 0).is_err());
    // A lone surrogate is not representable.
    assert!(decode(&[0xED, 0xA0, 0x80], 0).is_err());
}
