//! `java.util.Base64` and `java.util.HexFormat`.
//!
//! Both are small, self-contained encoders; the VM keeps them in Rust and
//! returns ordinary `String`/`byte[]` objects.

use alloc::string::String;
use alloc::vec::Vec;
use oxjvm_classfile::flags::*;

use crate::{class, field, method, object, receiver, ref_arg};
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(triple >> 18) as usize & 63]);
        out.push(ALPHABET[(triple >> 12) as usize & 63]);
        out.push(if chunk.len() > 1 {
            ALPHABET[(triple >> 6) as usize & 63]
        } else {
            b'='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[triple as usize & 63]
        } else {
            b'='
        });
    }
    out
}

fn base64_decode_text(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue,
        } as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    out
}

fn bytes_array(vm: &mut Vm<'_>, bytes: &[u8]) -> Result<ObjectRef, VmError> {
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, bytes.len())?;
    for (index, byte) in bytes.iter().enumerate() {
        vm.array_set(array, index as i32, Value::Int(i32::from(*byte)), false)?;
    }
    Ok(array)
}

fn byte_array_of(vm: &mut Vm<'_>, reference: ObjectRef) -> Result<Vec<u8>, VmError> {
    let length = vm.array_length(reference)?;
    let mut bytes = Vec::with_capacity(length);
    for index in 0..length {
        bytes.push(vm.array_get(reference, index as i32)?.as_int() as u8);
    }
    Ok(bytes)
}

// -------------------------------------------------------------------------------------------
// Base64
// -------------------------------------------------------------------------------------------

const BASE64_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "getEncoder",
        "()Ljava/util/Base64$Encoder;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/Base64$Encoder")?;
            object(vm.new_instance(class)?)
        },
    ),
    method(
        "getDecoder",
        "()Ljava/util/Base64$Decoder;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/Base64$Decoder")?;
            object(vm.new_instance(class)?)
        },
    ),
    method(
        "getUrlEncoder",
        "()Ljava/util/Base64$Encoder;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/Base64$Encoder")?;
            object(vm.new_instance(class)?)
        },
    ),
    method(
        "getUrlDecoder",
        "()Ljava/util/Base64$Decoder;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/Base64$Decoder")?;
            object(vm.new_instance(class)?)
        },
    ),
];

pub(crate) const BASE64: oxjvm_vm::NativeClass = class(
    "java/util/Base64",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &BASE64_METHODS,
    None,
);

const BASE64_ENCODER_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method("encode", "([B)[B", ACC_PUBLIC, |vm, _, args| {
        let bytes = byte_array_of(vm, ref_arg(args, 1))?;
        let encoded = base64_encode_bytes(&bytes);
        object(bytes_array(vm, &encoded)?)
    }),
    method(
        "encodeToString",
        "([B)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let bytes = byte_array_of(vm, ref_arg(args, 1))?;
            let encoded = base64_encode_bytes(&bytes);
            let text = String::from_utf8_lossy(&encoded).into_owned();
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "withoutPadding",
        "()Ljava/util/Base64$Encoder;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
];

pub(crate) const BASE64_ENCODER: oxjvm_vm::NativeClass = class(
    "java/util/Base64$Encoder",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &BASE64_ENCODER_METHODS,
    None,
);

const BASE64_DECODER_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method("decode", "([B)[B", ACC_PUBLIC, |vm, _, args| {
        let text = String::from_utf8_lossy(&byte_array_of(vm, ref_arg(args, 1))?).into_owned();
        let decoded = base64_decode_text(&text);
        object(bytes_array(vm, &decoded)?)
    }),
    method(
        "decode",
        "(Ljava/lang/String;)[B",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 1));
            let decoded = base64_decode_text(&text);
            object(bytes_array(vm, &decoded)?)
        },
    ),
];

pub(crate) const BASE64_DECODER: oxjvm_vm::NativeClass = class(
    "java/util/Base64$Decoder",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &BASE64_DECODER_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// HexFormat
// -------------------------------------------------------------------------------------------

fn hex_delimiter(vm: &Vm<'_>, this: ObjectRef) -> String {
    vm.read_string_field(this, vm.class_of(this), "delimiter")
        .unwrap_or_default()
}

fn hex_format_text(delimiter: &str, bytes: &[u8], uppercase: bool) -> String {
    let mut out = String::new();
    for (index, byte) in bytes.iter().enumerate() {
        if index > 0 {
            out.push_str(delimiter);
        }
        if uppercase {
            out.push_str(&alloc::format!("{byte:02X}"));
        } else {
            out.push_str(&alloc::format!("{byte:02x}"));
        }
    }
    out
}

const HEX_FORMAT_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "of",
        "()Ljava/util/HexFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/HexFormat")?;
            object(vm.new_instance(class)?)
        },
    ),
    method(
        "ofDelimiter",
        "(Ljava/lang/String;)Ljava/util/HexFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let class = vm.resolve_class("java/util/HexFormat")?;
            let instance = vm.new_instance(class)?;
            let (declaring, field) = vm.find_field(class, "delimiter", "Ljava/lang/String;")?;
            vm.set_instance_ref(instance, declaring, field, ref_arg(args, 0));
            object(instance)
        },
    ),
    method(
        "formatHex",
        "([B)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let delimiter = hex_delimiter(vm, receiver(args));
            let uppercase = crate::lang::read_int_field(vm, receiver(args), "upperCase") != 0;
            let bytes = byte_array_of(vm, ref_arg(args, 1))?;
            let text = hex_format_text(&delimiter, &bytes, uppercase);
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "withUpperCase",
        "()Ljava/util/HexFormat;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            crate::lang::write_int_field(vm, this, "upperCase", 1);
            object(this)
        },
    ),
    method(
        "parseHex",
        "(Ljava/lang/CharSequence;)[B",
        ACC_PUBLIC,
        |vm, _, args| {
            let delimiter = hex_delimiter(vm, receiver(args));
            let mut text = vm.string_or_empty(ref_arg(args, 1));
            if !delimiter.is_empty() {
                text = text.replace(&delimiter, "");
            }
            let mut bytes = Vec::new();
            let digits: Vec<u8> = text
                .bytes()
                .filter(|byte| byte.is_ascii_hexdigit())
                .collect();
            for pair in digits.chunks(2) {
                if pair.len() < 2 {
                    break;
                }
                let high = char::from(pair[0]).to_digit(16).unwrap_or(0) as u8;
                let low = char::from(pair[1]).to_digit(16).unwrap_or(0) as u8;
                bytes.push((high << 4) | low);
            }
            object(bytes_array(vm, &bytes)?)
        },
    ),
];

pub(crate) const HEX_FORMAT: oxjvm_vm::NativeClass = class(
    "java/util/HexFormat",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("delimiter", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("upperCase", "I", ACC_PRIVATE, None),
    ],
    &HEX_FORMAT_METHODS,
    None,
);
