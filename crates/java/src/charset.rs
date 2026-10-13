//! `java.nio.charset`: `Charset` and `StandardCharsets`.
//!
//! Only the charsets exercised by the suites are provided (UTF-8, US-ASCII,
//! ISO-8859-1 and the UTF-16 family); `forName` and the `StandardCharsets`
//! constants share one cache so lookups are identity-stable.

use alloc::vec;
use alloc::vec::Vec;
use oxjvm_classfile::flags::*;

use crate::{boolean, class, field, int, method, object, receiver, ref_arg, string_arg};
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

/// (canonical name, additional normalized aliases)
const CHARSETS: &[(&str, &[&str])] = &[
    ("UTF-8", &["utf8", "unicode11utf8"]),
    (
        "US-ASCII",
        &[
            "ascii",
            "usascii",
            "iso646us",
            "ansix3.41968",
            "cp367",
            "csascii",
        ],
    ),
    (
        "ISO-8859-1",
        &["iso88591", "latin1", "l1", "819", "cp819", "isoir100"],
    ),
    ("UTF-16", &["utf16", "unicode"]),
    ("UTF-16BE", &["utf16be", "unicodebigunmarked", "xutf16be"]),
    (
        "UTF-16LE",
        &["utf16le", "unicodelittleunmarked", "xutf16le"],
    ),
];

/// Normalize a charset name the way the JDK does for lookups: case-insensitive,
/// ignoring `-` and `_`.
fn normalize(name: &str) -> alloc::string::String {
    name.chars()
        .filter(|c| *c != '-' && *c != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

/// The canonical name for `name`, or `None` when the charset is unknown.
pub(crate) fn canonical_name(name: &str) -> Option<&'static str> {
    let key = normalize(name);
    for (canonical, aliases) in CHARSETS {
        if normalize(canonical) == key || aliases.contains(&key.as_str()) {
            return Some(canonical);
        }
    }
    None
}

/// Encode `text` with the canonical charset `canonical`.
pub(crate) fn encode(text: &str, canonical: &str) -> Vec<u8> {
    match canonical {
        "US-ASCII" => text
            .chars()
            .map(|c| if c.is_ascii() { c as u8 } else { b'?' })
            .collect(),
        "ISO-8859-1" => text
            .chars()
            .map(|c| if (c as u32) < 0x100 { c as u8 } else { b'?' })
            .collect(),
        "UTF-16" => {
            let mut bytes = alloc::vec![0xFE, 0xFF];
            for unit in text.encode_utf16() {
                bytes.extend_from_slice(&unit.to_be_bytes());
            }
            bytes
        }
        "UTF-16BE" => text.encode_utf16().flat_map(u16::to_be_bytes).collect(),
        "UTF-16LE" => text.encode_utf16().flat_map(u16::to_le_bytes).collect(),
        // UTF-8 and anything unknown.
        _ => text.as_bytes().to_vec(),
    }
}

/// Decode `bytes` with the canonical charset `canonical` into UTF-16 units.
pub(crate) fn decode(bytes: &[u8], canonical: &str) -> Vec<u16> {
    match canonical {
        "US-ASCII" | "ISO-8859-1" => bytes.iter().map(|byte| u16::from(*byte)).collect(),
        "UTF-16" => {
            if bytes.len() >= 2 {
                match (bytes[0], bytes[1]) {
                    (0xFE, 0xFF) => return decode(&bytes[2..], "UTF-16BE"),
                    (0xFF, 0xFE) => return decode(&bytes[2..], "UTF-16LE"),
                    _ => {}
                }
            }
            decode(bytes, "UTF-16BE")
        }
        "UTF-16BE" => utf16_units(bytes, u16::from_be_bytes),
        "UTF-16LE" => utf16_units(bytes, u16::from_le_bytes),
        _ => alloc::string::String::from_utf8_lossy(bytes)
            .encode_utf16()
            .collect(),
    }
}

fn utf16_units(bytes: &[u8], combine: fn([u8; 2]) -> u16) -> Vec<u16> {
    bytes
        .chunks_exact(2)
        .map(|pair| combine([pair[0], pair[1]]))
        .collect()
}

/// The canonical name of a `Charset` object.
pub(crate) fn name_of(vm: &Vm<'_>, charset: ObjectRef) -> alloc::string::String {
    if charset.is_null() {
        return alloc::string::String::new();
    }
    vm.read_string_field(charset, vm.class_of(charset), "name")
        .unwrap_or_default()
}

/// Look up (or create and cache) the `Charset` for `name`.
pub(crate) fn for_name(vm: &mut Vm<'_>, name: &str) -> Result<Option<ObjectRef>, VmError> {
    let Some(canonical) = canonical_name(name) else {
        return Ok(None);
    };
    let charset_class = vm.resolve_class("java/nio/charset/Charset")?;
    let map_class = vm.resolve_class("java/util/HashMap")?;
    let cache = vm
        .read_static_ref_if_set(charset_class, "cache", "Ljava/util/HashMap;")
        .ok_or_else(|| VmError::internal("Charset cache not initialized"))?;
    let key = vm.make_string(canonical)?;
    let (get_class, get) = vm
        .find_method(map_class, "get", "(Ljava/lang/Object;)Ljava/lang/Object;")
        .ok_or_else(|| VmError::internal("HashMap.get missing"))?;
    let existing = vm
        .invoke_method(get_class, get, vec![Value::Ref(cache), Value::Ref(key)])?
        .as_ref();
    if !existing.is_null() {
        return Ok(Some(existing));
    }
    let instance = vm.new_instance(charset_class)?;
    let (declaring, field) = vm.find_field(charset_class, "name", "Ljava/lang/String;")?;
    vm.set_instance_ref(instance, declaring, field, key);
    let (put_class, put) = vm
        .find_method(
            map_class,
            "put",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        )
        .ok_or_else(|| VmError::internal("HashMap.put missing"))?;
    vm.invoke_method(
        put_class,
        put,
        vec![Value::Ref(cache), Value::Ref(key), Value::Ref(instance)],
    )?;
    Ok(Some(instance))
}

fn charset_for_name(vm: &mut Vm<'_>, name: &str) -> Result<ObjectRef, VmError> {
    match for_name(vm, name)? {
        Some(charset) => Ok(charset),
        None => Err(vm.throw_new("java/nio/charset/UnsupportedCharsetException", Some(name))),
    }
}

fn charset_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let charset_class = vm.resolve_class("java/nio/charset/Charset")?;
    let map_class = vm.resolve_class("java/util/HashMap")?;
    let cache = vm.new_instance(map_class)?;
    let (declaring, ctor) = vm
        .find_method(map_class, "<init>", "()V")
        .ok_or_else(|| VmError::internal("HashMap() missing"))?;
    vm.invoke_method(declaring, ctor, vec![Value::Ref(cache)])?;
    vm.set_static_value(
        charset_class,
        "cache",
        "Ljava/util/HashMap;",
        Value::Ref(cache),
    );
    Ok(())
}

const CHARSET_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method(
        "forName",
        "(Ljava/lang/String;)Ljava/nio/charset/Charset;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = string_arg(vm, args[0])?;
            object(charset_for_name(vm, &name)?)
        },
    ),
    method(
        "isSupported",
        "(Ljava/lang/String;)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = match args[0] {
                Value::Ref(reference) => vm.string_value(reference),
                _ => None,
            };
            boolean(name.is_some_and(|name| canonical_name(&name).is_some()))
        },
    ),
    method(
        "defaultCharset",
        "()Ljava/nio/charset/Charset;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(charset_for_name(vm, "UTF-8")?),
    ),
    method(
        "availableCharsets",
        "()Ljava/util/SortedMap;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let map_class = vm.resolve_class("java/util/HashMap")?;
            let map = vm.new_instance(map_class)?;
            let (declaring, ctor) = vm
                .find_method(map_class, "<init>", "()V")
                .ok_or_else(|| VmError::internal("HashMap() missing"))?;
            vm.invoke_method(declaring, ctor, vec![Value::Ref(map)])?;
            let (put_class, put) = vm
                .find_method(
                    map_class,
                    "put",
                    "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
                )
                .ok_or_else(|| VmError::internal("HashMap.put missing"))?;
            for (canonical, _) in CHARSETS {
                let key = vm.make_string(canonical)?;
                let value = charset_for_name(vm, canonical)?;
                vm.invoke_method(
                    put_class,
                    put,
                    vec![Value::Ref(map), Value::Ref(key), Value::Ref(value)],
                )?;
            }
            object(map)
        },
    ),
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let name = name_of(vm, receiver(args));
        object(vm.make_string(&name)?)
    }),
    method(
        "displayName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = name_of(vm, receiver(args));
            object(vm.make_string(&name)?)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = name_of(vm, receiver(args));
            object(vm.make_string(&name)?)
        },
    ),
    method("canEncode", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() || vm.class_of(receiver(args)) != vm.class_of(other) {
                return boolean(false);
            }
            boolean(name_of(vm, receiver(args)) == name_of(vm, other))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let name = name_of(vm, receiver(args));
        let mut hash = 0i32;
        for unit in name.encode_utf16() {
            hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
        }
        int(hash)
    }),
];

pub(crate) const CHARSET: oxjvm_vm::NativeClass = class(
    "java/nio/charset/Charset",
    Some("java/lang/Object"),
    &["java/lang/Comparable"],
    ACC_PUBLIC,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field(
            "cache",
            "Ljava/util/HashMap;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
    ],
    &CHARSET_METHODS,
    Some(charset_clinit),
);

// -------------------------------------------------------------------------------------------
// StandardCharsets
// -------------------------------------------------------------------------------------------

fn standard_charset_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/nio/charset/StandardCharsets")?;
    for (field_name, canonical) in [
        ("UTF_8", "UTF-8"),
        ("UTF_16", "UTF-16"),
        ("UTF_16BE", "UTF-16BE"),
        ("UTF_16LE", "UTF-16LE"),
        ("US_ASCII", "US-ASCII"),
        ("ISO_8859_1", "ISO-8859-1"),
    ] {
        let charset = charset_for_name(vm, canonical)?;
        vm.set_static_value(
            class,
            field_name,
            "Ljava/nio/charset/Charset;",
            Value::Ref(charset),
        );
    }
    Ok(())
}

const STANDARD_CHARSETS_METHODS: [oxjvm_vm::NativeMethodDef; 0] = [];

pub(crate) const STANDARD_CHARSETS: oxjvm_vm::NativeClass = class(
    "java/nio/charset/StandardCharsets",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field(
            "UTF_8",
            "Ljava/nio/charset/Charset;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "UTF_16",
            "Ljava/nio/charset/Charset;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "UTF_16BE",
            "Ljava/nio/charset/Charset;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "UTF_16LE",
            "Ljava/nio/charset/Charset;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "US_ASCII",
            "Ljava/nio/charset/Charset;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "ISO_8859_1",
            "Ljava/nio/charset/Charset;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &STANDARD_CHARSETS_METHODS,
    Some(standard_charset_clinit),
);
