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
    let cache = match vm.read_static_ref_if_set(charset_class, "cache", "Ljava/util/HashMap;") {
        Some(cache) => cache,
        None => {
            charset_clinit(vm)?;
            vm.read_static_ref_if_set(charset_class, "cache", "Ljava/util/HashMap;")
                .ok_or_else(|| VmError::internal("Charset cache not initialized"))?
        }
    };
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

const CHARSET_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
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
    method(
        "newEncoder",
        "()Ljava/nio/charset/CharsetEncoder;",
        ACC_PUBLIC,
        |vm, _, args| new_charset_codec(vm, receiver(args), "java/nio/charset/OjvmCharsetEncoder"),
    ),
    method(
        "newDecoder",
        "()Ljava/nio/charset/CharsetDecoder;",
        ACC_PUBLIC,
        |vm, _, args| new_charset_codec(vm, receiver(args), "java/nio/charset/OjvmCharsetDecoder"),
    ),
    method(
        "encode",
        "(Ljava/lang/String;)Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = name_of(vm, receiver(args));
            let text = vm.string_or_empty(ref_arg(args, 1));
            let bytes = encode(&text, &name);
            object(crate::buffer::byte_buffer_from_bytes(vm, &bytes)?)
        },
    ),
    method(
        "decode",
        "(Ljava/nio/ByteBuffer;)Ljava/nio/CharBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = name_of(vm, receiver(args));
            let bytes = crate::buffer::byte_buffer_remaining_bytes(vm, ref_arg(args, 1))?;
            let text = alloc::string::String::from_utf16_lossy(&decode(&bytes, &name));
            object(crate::buffer::char_buffer_from_text(vm, &text)?)
        },
    ),
];

fn new_charset_codec(
    vm: &mut Vm<'_>,
    charset: ObjectRef,
    class_name: &str,
) -> Result<Value, VmError> {
    let class = vm.resolve_class(class_name)?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "charset", "Ljava/lang/String;")?;
    let name = name_of(vm, charset);
    let text = vm.make_string(&name)?;
    vm.set_instance_ref(instance, declaring, field, text);
    object(instance)
}

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

// -------------------------------------------------------------------------------------------
// CharsetEncoder / CharsetDecoder / CoderResult
// -------------------------------------------------------------------------------------------

const CODER_RESULT_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method("isUnderflow", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(field_int(vm, receiver(args), "kind") == 1)
    }),
    method("isOverflow", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(field_int(vm, receiver(args), "kind") == 2)
    }),
    method("isError", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(field_int(vm, receiver(args), "kind") == 3)
    }),
    method("throwException", "()V", ACC_PUBLIC, |vm, _, args| {
        let kind = field_int(vm, receiver(args), "kind");
        if kind == 1 {
            Err(vm.throw_new("java/nio/BufferUnderflowException", None))
        } else if kind == 2 {
            Err(vm.throw_new("java/nio/BufferOverflowException", None))
        } else {
            Err(vm.throw_new(
                "java/nio/charset/MalformedInputException",
                Some("malformed input"),
            ))
        }
    }),
    method("length", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let kind = field_int(vm, receiver(args), "kind");
            let text = match kind {
                1 => "UNDERFLOW",
                2 => "OVERFLOW",
                _ => "malformed input",
            };
            object(vm.make_string(text)?)
        },
    ),
];

fn coder_result_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/nio/charset/CoderResult")?;
    for (name, kind) in [("UNDERFLOW", 1), ("OVERFLOW", 2)] {
        let instance = vm.new_instance(class)?;
        set_field_int(vm, instance, "kind", kind);
        vm.set_static_value(
            class,
            name,
            "Ljava/nio/charset/CoderResult;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

pub(crate) const CODER_RESULT: oxjvm_vm::NativeClass = class(
    "java/nio/charset/CoderResult",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("kind", "I", ACC_PRIVATE, None),
        field(
            "UNDERFLOW",
            "Ljava/nio/charset/CoderResult;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "OVERFLOW",
            "Ljava/nio/charset/CoderResult;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &CODER_RESULT_METHODS,
    Some(coder_result_clinit),
);

const CHARSET_ENCODER_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method(
        "charset",
        "()Ljava/nio/charset/Charset;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = vm
                .read_string_field(receiver(args), vm.class_of(receiver(args)), "charset")
                .unwrap_or_default();
            object(charset_for_name(vm, &name)?)
        },
    ),
    method(
        "encode",
        "(Ljava/nio/CharBuffer;)Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = vm
                .read_string_field(receiver(args), vm.class_of(receiver(args)), "charset")
                .unwrap_or_default();
            let text = crate::buffer::char_buffer_text(vm, ref_arg(args, 1))?;
            let bytes = encode(&text, &name);
            object(crate::buffer::byte_buffer_from_bytes(vm, &bytes)?)
        },
    ),
    method(
        "encode",
        "(Ljava/nio/CharBuffer;Ljava/nio/ByteBuffer;Z)Ljava/nio/charset/CoderResult;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = vm
                .read_string_field(receiver(args), vm.class_of(receiver(args)), "charset")
                .unwrap_or_default();
            let text = crate::buffer::char_buffer_text(vm, ref_arg(args, 1))?;
            let bytes = encode(&text, &name);
            let target = ref_arg(args, 2);
            let (class, put) =
                vm.resolve_virtual_method(target, "put", "([B)Ljava/nio/ByteBuffer;")?;
            let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, bytes.len())?;
            for (index, byte) in bytes.iter().enumerate() {
                vm.array_set(
                    array,
                    index as i32,
                    Value::Int(i32::from(*byte as i8)),
                    true,
                )?;
            }
            vm.invoke_method(class, put, vec![Value::Ref(target), Value::Ref(array)])?;
            object(coder_result(vm, 1)?)
        },
    ),
    method("maxBytesPerChar", "()F", ACC_PUBLIC, |_, _, _| {
        Ok(Value::Float(4.0))
    }),
    method("averageBytesPerChar", "()F", ACC_PUBLIC, |_, _, _| {
        Ok(Value::Float(1.0))
    }),
    method("replacement", "()[B", ACC_PUBLIC, |vm, _, _| {
        object(crate::buffer::byte_buffer_from_bytes(vm, b"?")?)
    }),
    method(
        "reset",
        "()Ljava/nio/charset/CharsetEncoder;",
        ACC_PUBLIC,
        |_vm, _, args| object(receiver(args)),
    ),
    method(
        "flush",
        "()Ljava/nio/charset/CoderResult;",
        ACC_PUBLIC,
        |vm, _, _| object(coder_result(vm, 1)?),
    ),
    method(
        "canEncode",
        "(Ljava/lang/CharSequence;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(true),
    ),
];

fn coder_result(vm: &mut Vm<'_>, kind: i32) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/nio/charset/CoderResult")?;
    vm.ensure_initialized(class)?;
    let name = if kind == 1 { "UNDERFLOW" } else { "OVERFLOW" };
    Ok(vm
        .read_static_ref_if_set(class, name, "Ljava/nio/charset/CoderResult;")
        .unwrap_or(ObjectRef::NULL))
}

pub(crate) const CHARSET_ENCODER: oxjvm_vm::NativeClass = class(
    "java/nio/charset/CharsetEncoder",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[field("charset", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &CHARSET_ENCODER_METHODS,
    None,
);

pub(crate) const OJVM_CHARSET_ENCODER: oxjvm_vm::NativeClass = class(
    "java/nio/charset/OjvmCharsetEncoder",
    Some("java/nio/charset/CharsetEncoder"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[],
    &[],
    None,
);

const CHARSET_DECODER_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "charset",
        "()Ljava/nio/charset/Charset;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = vm
                .read_string_field(receiver(args), vm.class_of(receiver(args)), "charset")
                .unwrap_or_default();
            object(charset_for_name(vm, &name)?)
        },
    ),
    method(
        "decode",
        "(Ljava/nio/ByteBuffer;)Ljava/nio/CharBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = vm
                .read_string_field(receiver(args), vm.class_of(receiver(args)), "charset")
                .unwrap_or_default();
            let bytes = crate::buffer::byte_buffer_remaining_bytes(vm, ref_arg(args, 1))?;
            let text = alloc::string::String::from_utf16_lossy(&decode(&bytes, &name));
            object(crate::buffer::char_buffer_from_text(vm, &text)?)
        },
    ),
    method("maxCharsPerByte", "()F", ACC_PUBLIC, |_, _, _| {
        Ok(Value::Float(1.0))
    }),
    method("averageCharsPerByte", "()F", ACC_PUBLIC, |_, _, _| {
        Ok(Value::Float(1.0))
    }),
    method(
        "replacement",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("\u{FFFD}")?),
    ),
    method(
        "reset",
        "()Ljava/nio/charset/CharsetDecoder;",
        ACC_PUBLIC,
        |_vm, _, args| object(receiver(args)),
    ),
    method(
        "decode",
        "(Ljava/nio/ByteBuffer;Ljava/nio/CharBuffer;Z)Ljava/nio/charset/CoderResult;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = vm
                .read_string_field(receiver(args), vm.class_of(receiver(args)), "charset")
                .unwrap_or_default();
            let bytes = crate::buffer::byte_buffer_remaining_bytes(vm, ref_arg(args, 1))?;
            let text = alloc::string::String::from_utf16_lossy(&decode(&bytes, &name));
            let target = ref_arg(args, 2);
            let (class, put) = vm.resolve_virtual_method(
                target,
                "put",
                "(Ljava/lang/String;)Ljava/nio/CharBuffer;",
            )?;
            let string = vm.make_string(&text)?;
            vm.invoke_method(class, put, vec![Value::Ref(target), Value::Ref(string)])?;
            object(coder_result(vm, 1)?)
        },
    ),
];

pub(crate) const CHARSET_DECODER: oxjvm_vm::NativeClass = class(
    "java/nio/charset/CharsetDecoder",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[field("charset", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &CHARSET_DECODER_METHODS,
    None,
);

pub(crate) const OJVM_CHARSET_DECODER: oxjvm_vm::NativeClass = class(
    "java/nio/charset/OjvmCharsetDecoder",
    Some("java/nio/charset/CharsetDecoder"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[],
    &[],
    None,
);

fn field_int(vm: &Vm<'_>, this: ObjectRef, name: &str) -> i32 {
    let class = vm.class_of(this);
    vm.find_field_opt(class, name, "I")
        .and_then(|(declaring, field)| vm.read_instance_int(this, declaring, field))
        .unwrap_or(0)
}

fn set_field_int(vm: &mut Vm<'_>, this: ObjectRef, name: &str, value: i32) {
    let class = vm.class_of(this);
    if let Some((declaring, field)) = vm.find_field_opt(class, name, "I") {
        vm.set_instance_int(this, declaring, field, value);
    }
}
