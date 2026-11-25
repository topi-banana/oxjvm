//! `java.util.Base64` and `java.util.HexFormat`.
//!
//! Both are small, self-contained encoders; the VM keeps them in Rust and
//! returns ordinary `String`/`byte[]` objects.

use alloc::string::String;
use alloc::vec::Vec;
use oxjvm_classfile::flags::*;

use crate::lang::{read_int_field, write_int_field};
use crate::{class, field, method, object, receiver, ref_arg, void};
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn base64_encode_with(bytes: &[u8], alphabet: &[u8; 64]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(alphabet[(triple >> 18) as usize & 63]);
        out.push(alphabet[(triple >> 12) as usize & 63]);
        out.push(if chunk.len() > 1 {
            alphabet[(triple >> 6) as usize & 63]
        } else {
            b'='
        });
        out.push(if chunk.len() > 2 {
            alphabet[triple as usize & 63]
        } else {
            b'='
        });
    }
    out
}

/// The final encoder output: alphabet selection, optional padding removal, MIME wrapping.
fn base64_encoder_output(vm: &Vm<'_>, encoder: ObjectRef, bytes: &[u8]) -> Vec<u8> {
    let alphabet = if read_int_field(vm, encoder, "url") != 0 {
        URL_ALPHABET
    } else {
        ALPHABET
    };
    let mut encoded = base64_encode_with(bytes, alphabet);
    if read_int_field(vm, encoder, "noPadding") != 0 {
        while encoded.last() == Some(&b'=') {
            encoded.pop();
        }
    }
    let (line_length, separator) = base64_mime_parts(vm, encoder);
    base64_encode_mime(&encoded, line_length, &separator)
}

/// `Encoder.withoutPadding`: a copy of the encoder that omits trailing `=`.
fn base64_without_padding(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/util/Base64$Encoder")?;
    let instance = vm.new_instance(class)?;
    write_int_field(vm, instance, "mime", read_int_field(vm, this, "mime"));
    write_int_field(
        vm,
        instance,
        "lineLength",
        read_int_field(vm, this, "lineLength"),
    );
    write_int_field(vm, instance, "url", read_int_field(vm, this, "url"));
    write_int_field(vm, instance, "noPadding", 1);
    if let Some(separator) =
        vm.read_ref_field(this, vm.class_of(this), "lineSep", "Ljava/lang/String;")
    {
        if let Some((declaring, field)) = vm.find_field_opt(class, "lineSep", "Ljava/lang/String;")
        {
            vm.set_instance_ref(instance, declaring, field, separator);
        }
    }
    object(instance)
}

/// Wrap MIME base64 output into lines of `line_length` characters.
fn base64_encode_mime(encoded: &[u8], line_length: i32, separator: &str) -> Vec<u8> {
    if line_length <= 0 {
        return encoded.to_vec();
    }
    let line = line_length as usize;
    let mut out = Vec::new();
    let mut chunks = encoded.chunks(line).peekable();
    while let Some(chunk) = chunks.next() {
        out.extend_from_slice(chunk);
        if chunks.peek().is_some() {
            out.extend_from_slice(separator.as_bytes());
        }
    }
    out
}

/// Build a MIME encoder instance with the given line length and separator.
fn base64_mime_encoder(
    vm: &mut Vm<'_>,
    line_length: i32,
    separator: &str,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/Base64$Encoder")?;
    let instance = vm.new_instance(class)?;
    write_int_field(vm, instance, "mime", 1);
    // MIME line length is rounded down to the nearest multiple of 4; a resulting
    // length of 0 disables line separation entirely.
    write_int_field(vm, instance, "lineLength", (line_length.max(0) / 4) * 4);
    let text = vm.make_string(separator)?;
    if let Some((declaring, field)) = vm.find_field_opt(class, "lineSep", "Ljava/lang/String;") {
        vm.set_instance_ref(instance, declaring, field, text);
    }
    Ok(instance)
}

/// The MIME line length/separator of an encoder (0/"" for the basic encoders).
fn base64_mime_parts(vm: &Vm<'_>, encoder: ObjectRef) -> (i32, String) {
    if read_int_field(vm, encoder, "mime") == 0 {
        return (0, String::new());
    }
    let separator = vm
        .read_ref_field(
            encoder,
            vm.class_of(encoder),
            "lineSep",
            "Ljava/lang/String;",
        )
        .map(|text| vm.string_or_empty(text))
        .unwrap_or_default();
    (read_int_field(vm, encoder, "lineLength"), separator)
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
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
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

/// Write `bytes` into an existing destination array, returning the count written.
fn fill_byte_array(vm: &mut Vm<'_>, dst: ObjectRef, bytes: &[u8]) -> Result<i32, VmError> {
    let length = vm.array_length(dst)?;
    if bytes.len() > length {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("Output byte array is too small for encoding all input bytes"),
        ));
    }
    for (index, byte) in bytes.iter().enumerate() {
        vm.array_set(dst, index as i32, Value::Int(i32::from(*byte as i8)), false)?;
    }
    Ok(bytes.len() as i32)
}

// -------------------------------------------------------------------------------------------
// Base64
// -------------------------------------------------------------------------------------------

const BASE64_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
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
            let instance = vm.new_instance(class)?;
            write_int_field(vm, instance, "url", 1);
            object(instance)
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
    method(
        "getMimeEncoder",
        "()Ljava/util/Base64$Encoder;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(base64_mime_encoder(vm, 76, "\r\n")?),
    ),
    method(
        "getMimeEncoder",
        "(I[B)Ljava/util/Base64$Encoder;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let line_length = crate::int_arg(args, 0);
            let separator_ref = ref_arg(args, 1);
            if separator_ref.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let separator_bytes = byte_array_of(vm, separator_ref)?;
            for byte in &separator_bytes {
                if ALPHABET.contains(byte) || *byte == b'=' {
                    return Err(vm.throw_new(
                        "java/lang/IllegalArgumentException",
                        Some("lineSeparator contains base64 alphabet character"),
                    ));
                }
            }
            let separator = String::from_utf8_lossy(&separator_bytes).into_owned();
            object(base64_mime_encoder(vm, line_length, &separator)?)
        },
    ),
    method(
        "getMimeDecoder",
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

const BASE64_ENCODER_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method("encode", "([B)[B", ACC_PUBLIC, |vm, _, args| {
        let bytes = byte_array_of(vm, ref_arg(args, 1))?;
        let wrapped = base64_encoder_output(vm, receiver(args), &bytes);
        object(bytes_array(vm, &wrapped)?)
    }),
    method("encode", "([B[B)I", ACC_PUBLIC, |vm, _, args| {
        let bytes = byte_array_of(vm, ref_arg(args, 1))?;
        let wrapped = base64_encoder_output(vm, receiver(args), &bytes);
        let written = fill_byte_array(vm, ref_arg(args, 2), &wrapped)?;
        Ok(Value::Int(written))
    }),
    method(
        "encode",
        "(Ljava/nio/ByteBuffer;)Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let bytes = crate::buffer::byte_buffer_consume_remaining(vm, ref_arg(args, 1))?;
            let wrapped = base64_encoder_output(vm, receiver(args), &bytes);
            object(crate::buffer::byte_buffer_from_bytes(vm, &wrapped)?)
        },
    ),
    method(
        "encodeToString",
        "([B)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let bytes = byte_array_of(vm, ref_arg(args, 1))?;
            let wrapped = base64_encoder_output(vm, receiver(args), &bytes);
            let text = String::from_utf8_lossy(&wrapped).into_owned();
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "withoutPadding",
        "()Ljava/util/Base64$Encoder;",
        ACC_PUBLIC,
        |vm, _, args| base64_without_padding(vm, receiver(args)),
    ),
    method(
        "wrap",
        "(Ljava/io/OutputStream;)Ljava/io/OutputStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = vm.resolve_class("java/util/Base64$EncOutputStream")?;
            let instance = vm.new_instance(class)?;
            let (declaring, field) = vm.find_field(class, "out", "Ljava/io/OutputStream;")?;
            vm.set_instance_ref(instance, declaring, field, ref_arg(args, 1));
            let (declaring, field) = vm.find_field(class, "enc", "Ljava/util/Base64$Encoder;")?;
            vm.set_instance_ref(instance, declaring, field, receiver(args));
            object(instance)
        },
    ),
];

pub(crate) const BASE64_ENCODER: oxjvm_vm::NativeClass = class(
    "java/util/Base64$Encoder",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("mime", "I", ACC_PRIVATE, None),
        field("lineLength", "I", ACC_PRIVATE, None),
        field("lineSep", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("url", "I", ACC_PRIVATE, None),
        field("noPadding", "I", ACC_PRIVATE, None),
    ],
    &BASE64_ENCODER_METHODS,
    None,
);

/// Accumulate bytes written to a wrapping Base64 output stream.
fn enc_os_append(vm: &mut Vm<'_>, this: ObjectRef, bytes: &[u8]) -> Result<(), VmError> {
    let class = vm.class_of(this);
    let existing = vm
        .read_ref_field(this, class, "buf", "[B")
        .unwrap_or(ObjectRef::NULL);
    let mut combined = if existing.is_null() {
        Vec::new()
    } else {
        byte_array_of(vm, existing)?
    };
    combined.extend_from_slice(bytes);
    let array = bytes_array(vm, &combined)?;
    let (declaring, field) = vm.find_field(class, "buf", "[B")?;
    vm.set_instance_ref(this, declaring, field, array);
    Ok(())
}

/// On close, encode everything written so far and flush it to the wrapped stream.
fn enc_os_close(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Value, VmError> {
    let class = vm.class_of(this);
    if read_int_field(vm, this, "closed") != 0 {
        return void();
    }
    write_int_field(vm, this, "closed", 1);
    let buf = vm
        .read_ref_field(this, class, "buf", "[B")
        .unwrap_or(ObjectRef::NULL);
    let input = if buf.is_null() {
        Vec::new()
    } else {
        byte_array_of(vm, buf)?
    };
    let enc = vm
        .read_ref_field(this, class, "enc", "Ljava/util/Base64$Encoder;")
        .unwrap_or(ObjectRef::NULL);
    let encoded = base64_encoder_output(vm, enc, &input);
    let out = vm
        .read_ref_field(this, class, "out", "Ljava/io/OutputStream;")
        .unwrap_or(ObjectRef::NULL);
    let array = bytes_array(vm, &encoded)?;
    let (write_class, write_method) = vm.resolve_virtual_method(out, "write", "([B)V")?;
    vm.invoke_method(
        write_class,
        write_method,
        alloc::vec![Value::Ref(out), Value::Ref(array)],
    )?;
    let (close_class, close_method) = vm.resolve_virtual_method(out, "close", "()V")?;
    vm.invoke_method(close_class, close_method, alloc::vec![Value::Ref(out)])?;
    void()
}

const BASE64_ENC_OUTPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("write", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let byte = crate::int_arg(args, 1) as u8;
        enc_os_append(vm, receiver(args), &[byte])?;
        void()
    }),
    method("write", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = byte_array_of(vm, ref_arg(args, 1))?;
        enc_os_append(vm, receiver(args), &bytes)?;
        void()
    }),
    method("write", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = byte_array_of(vm, ref_arg(args, 1))?;
        let off = crate::int_arg(args, 2);
        let len = crate::int_arg(args, 3);
        if off < 0 || len < 0 || off as usize + len as usize > bytes.len() {
            return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
        }
        enc_os_append(
            vm,
            receiver(args),
            &bytes[off as usize..(off + len) as usize],
        )?;
        void()
    }),
    method("flush", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        enc_os_close(vm, receiver(args))
    }),
];

pub(crate) const BASE64_ENC_OUTPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/util/Base64$EncOutputStream",
    Some("java/io/OutputStream"),
    &[],
    ACC_PUBLIC,
    &[
        field("out", "Ljava/io/OutputStream;", ACC_PRIVATE, None),
        field("enc", "Ljava/util/Base64$Encoder;", ACC_PRIVATE, None),
        field("buf", "[B", ACC_PRIVATE, None),
        field("closed", "I", ACC_PRIVATE, None),
    ],
    &BASE64_ENC_OUTPUT_STREAM_METHODS,
    None,
);

const BASE64_DECODER_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method("decode", "([B)[B", ACC_PUBLIC, |vm, _, args| {
        let text = String::from_utf8_lossy(&byte_array_of(vm, ref_arg(args, 1))?).into_owned();
        let decoded = base64_decode_text(&text);
        object(bytes_array(vm, &decoded)?)
    }),
    method("decode", "([B[B)I", ACC_PUBLIC, |vm, _, args| {
        let text = String::from_utf8_lossy(&byte_array_of(vm, ref_arg(args, 1))?).into_owned();
        let decoded = base64_decode_text(&text);
        let written = fill_byte_array(vm, ref_arg(args, 2), &decoded)?;
        Ok(Value::Int(written))
    }),
    method(
        "decode",
        "(Ljava/nio/ByteBuffer;)Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let bytes = crate::buffer::byte_buffer_consume_remaining(vm, ref_arg(args, 1))?;
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let decoded = base64_decode_text(&text);
            object(crate::buffer::byte_buffer_from_bytes(vm, &decoded)?)
        },
    ),
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

/// Read a `HexFormat` string field as UTF-16 units (empty when unset/null).
fn hex_field_units(vm: &Vm<'_>, this: ObjectRef, name: &str) -> Vec<u16> {
    let class = vm.class_of(this);
    vm.read_ref_field(this, class, name, "Ljava/lang/String;")
        .and_then(|r| vm.string_utf16(r).map(<[u16]>::to_vec))
        .unwrap_or_default()
}

/// UTF-16 units of any `CharSequence`; non-`String` values (StringBuilder,
/// CharBuffer, ...) are rendered through their `toString()`.
fn hex_sequence_units(vm: &mut Vm<'_>, reference: ObjectRef) -> Result<Vec<u16>, VmError> {
    if let Some(units) = vm.string_utf16(reference) {
        return Ok(units.to_vec());
    }
    let (class, method) =
        vm.resolve_virtual_method(reference, "toString", "()Ljava/lang/String;")?;
    let text = vm.invoke_method(class, method, alloc::vec![Value::Ref(reference)])?;
    Ok(vm
        .string_utf16(text.as_ref())
        .map(<[u16]>::to_vec)
        .unwrap_or_default())
}

fn hex_fields(vm: &Vm<'_>, this: ObjectRef) -> (Vec<u16>, Vec<u16>, Vec<u16>, bool) {
    (
        hex_field_units(vm, this, "delimiter"),
        hex_field_units(vm, this, "prefix"),
        hex_field_units(vm, this, "suffix"),
        read_int_field(vm, this, "upperCase") != 0,
    )
}

/// Build a fresh immutable `HexFormat` with the given components.
fn hex_new(
    vm: &mut Vm<'_>,
    delimiter: &[u16],
    prefix: &[u16],
    suffix: &[u16],
    uppercase: bool,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/HexFormat")?;
    let instance = vm.new_instance(class)?;
    for (name, units) in [
        ("delimiter", delimiter),
        ("prefix", prefix),
        ("suffix", suffix),
    ] {
        let text = vm.make_string_utf16(units.to_vec())?;
        let (declaring, field) = vm.find_field(class, name, "Ljava/lang/String;")?;
        vm.set_instance_ref(instance, declaring, field, text);
    }
    write_int_field(vm, instance, "upperCase", i32::from(uppercase));
    Ok(instance)
}

/// The hexadecimal character for a nibble, in the configured case.
fn hex_digit_char(nibble: u32, uppercase: bool) -> u16 {
    let n = nibble & 0xf;
    if n < 10 {
        u16::from(b'0') + n as u16
    } else {
        u16::from(if uppercase { b'A' } else { b'a' }) + (n - 10) as u16
    }
}

fn hex_push_ascii(out: &mut Vec<u16>, text: &str) {
    out.extend(text.bytes().map(u16::from));
}

/// `prefix + two hex digits + suffix` per byte, joined by `delimiter`.
fn hex_format_units(
    delimiter: &[u16],
    prefix: &[u16],
    suffix: &[u16],
    bytes: &[u8],
    uppercase: bool,
) -> Vec<u16> {
    let mut out = Vec::new();
    for (index, byte) in bytes.iter().enumerate() {
        if index > 0 {
            out.extend_from_slice(delimiter);
        }
        out.extend_from_slice(prefix);
        out.push(hex_digit_char(u32::from(*byte) >> 4, uppercase));
        out.push(hex_digit_char(u32::from(*byte), uppercase));
        out.extend_from_slice(suffix);
    }
    out
}

/// The value of a single hexadecimal digit, or `NumberFormatException`.
fn hex_from_digit(vm: &mut Vm<'_>, ch: u16) -> Result<i32, VmError> {
    let c = u32::from(ch);
    let value = match c {
        0x30..=0x39 => c - 0x30,
        0x41..=0x46 => c - 0x41 + 10,
        0x61..=0x66 => c - 0x61 + 10,
        _ => {
            let shown = char::from_u32(c).unwrap_or('?');
            return Err(vm.throw_new(
                "java/lang/NumberFormatException",
                Some(&alloc::format!(
                    "not a hexadecimal digit: \"{shown}\" = {c}"
                )),
            ));
        }
    };
    Ok(value as i32)
}

fn hex_is_digit(ch: i32) -> bool {
    ((ch as u32) >> 7) == 0 && matches!(ch, 0x30..=0x39 | 0x41..=0x46 | 0x61..=0x66)
}

/// `Objects.checkFromToIndex`: reject an invalid `[from, to)` range.
fn hex_check_from_to(vm: &mut Vm<'_>, from: i32, to: i32, length: i32) -> Result<(), VmError> {
    if from < 0 || to > length || from > to {
        return Err(vm.throw_new(
            "java/lang/IndexOutOfBoundsException",
            Some(&alloc::format!(
                "Range [{from}, {to}) out of bounds for length {length}"
            )),
        ));
    }
    Ok(())
}

/// Verify a literal (prefix/suffix/between) appears at `index`.
fn hex_check_literal(
    vm: &mut Vm<'_>,
    units: &[u16],
    index: usize,
    literal: &[u16],
) -> Result<(), VmError> {
    if literal.is_empty() {
        return Ok(());
    }
    if index + literal.len() > units.len() {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("missing prefix or suffix"),
        ));
    }
    for (i, &expected) in literal.iter().enumerate() {
        if units[index + i] != expected {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some(&alloc::format!(
                    "found: \"{}\", expected: \"{}\", index: {}",
                    String::from_utf16_lossy(&units[index..index + literal.len()]),
                    String::from_utf16_lossy(literal),
                    index + i
                )),
            ));
        }
    }
    Ok(())
}

/// Strict `parseHex` over UTF-16 units with the configured components.
fn hex_parse(
    vm: &mut Vm<'_>,
    units: &[u16],
    delimiter: &[u16],
    prefix: &[u16],
    suffix: &[u16],
) -> Result<Vec<u8>, VmError> {
    if units.is_empty() {
        return Ok(Vec::new());
    }
    if delimiter.is_empty() && prefix.is_empty() && suffix.is_empty() {
        if units.len() % 2 != 0 {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some(&alloc::format!("string length not even: {}", units.len())),
            ));
        }
        let mut bytes = Vec::with_capacity(units.len() / 2);
        for pair in units.chunks(2) {
            let high = hex_from_digit(vm, pair[0])?;
            let low = hex_from_digit(vm, pair[1])?;
            bytes.push(((high << 4) | low) as u8);
        }
        return Ok(bytes);
    }
    let value_chars = prefix.len() + 2 + suffix.len();
    let stride = value_chars + delimiter.len();
    if units.len() < value_chars || (units.len() - value_chars) % stride != 0 {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("extra or missing delimiters or values consisting of prefix, two hexadecimal digits, and suffix"),
        ));
    }
    hex_check_literal(vm, units, 0, prefix)?;
    hex_check_literal(vm, units, units.len() - suffix.len(), suffix)?;
    let mut between = suffix.to_vec();
    between.extend_from_slice(delimiter);
    between.extend_from_slice(prefix);
    let count = (units.len() - value_chars) / stride + 1;
    let mut bytes = Vec::with_capacity(count);
    let mut offset = prefix.len();
    for i in 0..count {
        let high = hex_from_digit(vm, units[offset])?;
        let low = hex_from_digit(vm, units[offset + 1])?;
        bytes.push(((high << 4) | low) as u8);
        if i < count - 1 {
            hex_check_literal(vm, units, offset + 2, &between)?;
            offset += 2 + between.len();
        }
    }
    Ok(bytes)
}

/// Accumulate up to `limit` hex digits into a value.
fn hex_from_digits(
    vm: &mut Vm<'_>,
    units: &[u16],
    from: i32,
    to: i32,
    limit: i32,
) -> Result<i64, VmError> {
    hex_check_from_to(vm, from, to, units.len() as i32)?;
    let length = to - from;
    if length > limit {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some(&alloc::format!(
                "string length greater than {limit}: {length}"
            )),
        ));
    }
    let mut value = 0i64;
    for i in 0..length {
        value = (value << 4) + i64::from(hex_from_digit(vm, units[(from + i) as usize])?);
    }
    Ok(value)
}

/// Render `value` as exactly `digits` hexadecimal characters.
fn hex_to_digits(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    value: u64,
    digits: usize,
) -> Result<ObjectRef, VmError> {
    let uppercase = read_int_field(vm, this, "upperCase") != 0;
    let mut units = alloc::vec![0u16; digits];
    let mut rest = value;
    for slot in units.iter_mut().rev() {
        *slot = hex_digit_char((rest & 0xf) as u32, uppercase);
        rest >>= 4;
    }
    vm.make_string_utf16(units)
}

/// Wrap an `IOException` thrown by an `Appendable` as `UncheckedIOException`.
fn hex_wrap_io(vm: &mut Vm<'_>, exc: ObjectRef) -> VmError {
    let io_class = match vm.resolve_class("java/io/IOException") {
        Ok(class) => class,
        Err(error) => return error,
    };
    if !vm.is_instance(exc, io_class) {
        return VmError::Thrown(exc);
    }
    let class = match vm.resolve_class("java/io/UncheckedIOException") {
        Ok(class) => class,
        Err(error) => return error,
    };
    let object = match vm.new_instance(class) {
        Ok(object) => object,
        Err(error) => return error,
    };
    if let Some((declaring, method)) = vm.find_method(class, "<init>", "(Ljava/io/IOException;)V") {
        if let Err(error) = vm.invoke_method(
            declaring,
            method,
            alloc::vec![Value::Ref(object), Value::Ref(exc)],
        ) {
            return error;
        }
    }
    VmError::Thrown(object)
}

fn hex_append_char(vm: &mut Vm<'_>, out: ObjectRef, ch: u16) -> Result<(), VmError> {
    let (class, method) = vm.resolve_virtual_method(out, "append", "(C)Ljava/lang/Appendable;")?;
    match vm.invoke_method(
        class,
        method,
        alloc::vec![Value::Ref(out), Value::Int(i32::from(ch))],
    ) {
        Ok(_) => Ok(()),
        Err(VmError::Thrown(exc)) => Err(hex_wrap_io(vm, exc)),
        Err(error) => Err(error),
    }
}

fn hex_append_units(vm: &mut Vm<'_>, out: ObjectRef, units: &[u16]) -> Result<(), VmError> {
    if units.is_empty() {
        return Ok(());
    }
    let text = vm.make_string_utf16(units.to_vec())?;
    let (class, method) = vm.resolve_virtual_method(
        out,
        "append",
        "(Ljava/lang/CharSequence;)Ljava/lang/Appendable;",
    )?;
    match vm.invoke_method(
        class,
        method,
        alloc::vec![Value::Ref(out), Value::Ref(text)],
    ) {
        Ok(_) => Ok(()),
        Err(VmError::Thrown(exc)) => Err(hex_wrap_io(vm, exc)),
        Err(error) => Err(error),
    }
}

/// `formatHex(byte[], from, to)` core: null/range checks then render.
fn hex_format_range(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    bytes_ref: ObjectRef,
    from: i32,
    to: i32,
) -> Result<Value, VmError> {
    if bytes_ref.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", Some("bytes")));
    }
    let bytes = byte_array_of(vm, bytes_ref)?;
    hex_check_from_to(vm, from, to, bytes.len() as i32)?;
    let (delimiter, prefix, suffix, uppercase) = hex_fields(vm, this);
    let units = hex_format_units(
        &delimiter,
        &prefix,
        &suffix,
        &bytes[from as usize..to as usize],
        uppercase,
    );
    object(vm.make_string_utf16(units)?)
}

/// `parseHex(CharSequence, from, to)` core.
fn hex_parse_range(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    string_ref: ObjectRef,
    from: i32,
    to: i32,
) -> Result<Value, VmError> {
    if string_ref.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", Some("string")));
    }
    let units = hex_sequence_units(vm, string_ref)?;
    hex_check_from_to(vm, from, to, units.len() as i32)?;
    let (delimiter, prefix, suffix, _) = hex_fields(vm, this);
    let bytes = hex_parse(
        vm,
        &units[from as usize..to as usize],
        &delimiter,
        &prefix,
        &suffix,
    )?;
    object(bytes_array(vm, &bytes)?)
}

/// JDK `String.hashCode` over UTF-16 units.
fn hex_string_hash(units: &[u16]) -> i32 {
    let mut hash = 0i32;
    for &unit in units {
        hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
    }
    hash
}

/// Replace newline/carriage-return with their escaped two-character forms.
fn hex_escape_nl(units: &[u16]) -> Vec<u16> {
    let mut out = Vec::new();
    for &unit in units {
        match unit {
            0x0A => {
                out.push(0x5C);
                out.push(0x6E);
            }
            0x0D => {
                out.push(0x5C);
                out.push(0x72);
            }
            _ => out.push(unit),
        }
    }
    out
}

const HEX_FORMAT_METHODS: [oxjvm_vm::NativeMethodDef; 36] = [
    method(
        "of",
        "()Ljava/util/HexFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(hex_new(vm, &[], &[], &[], false)?),
    ),
    method(
        "ofDelimiter",
        "(Ljava/lang/String;)Ljava/util/HexFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let delimiter = ref_arg(args, 0);
            if delimiter.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("delimiter")));
            }
            let units = vm
                .string_utf16(delimiter)
                .map(<[u16]>::to_vec)
                .unwrap_or_default();
            object(hex_new(vm, &units, &[], &[], false)?)
        },
    ),
    method(
        "withDelimiter",
        "(Ljava/lang/String;)Ljava/util/HexFormat;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let delimiter = ref_arg(args, 1);
            if delimiter.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("delimiter")));
            }
            let units = vm
                .string_utf16(delimiter)
                .map(<[u16]>::to_vec)
                .unwrap_or_default();
            let (_, prefix, suffix, uppercase) = hex_fields(vm, this);
            object(hex_new(vm, &units, &prefix, &suffix, uppercase)?)
        },
    ),
    method(
        "withPrefix",
        "(Ljava/lang/String;)Ljava/util/HexFormat;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let prefix = ref_arg(args, 1);
            if prefix.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("prefix")));
            }
            let units = vm
                .string_utf16(prefix)
                .map(<[u16]>::to_vec)
                .unwrap_or_default();
            let (delimiter, _, suffix, uppercase) = hex_fields(vm, this);
            object(hex_new(vm, &delimiter, &units, &suffix, uppercase)?)
        },
    ),
    method(
        "withSuffix",
        "(Ljava/lang/String;)Ljava/util/HexFormat;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let suffix = ref_arg(args, 1);
            if suffix.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("suffix")));
            }
            let units = vm
                .string_utf16(suffix)
                .map(<[u16]>::to_vec)
                .unwrap_or_default();
            let (delimiter, prefix, _, uppercase) = hex_fields(vm, this);
            object(hex_new(vm, &delimiter, &prefix, &units, uppercase)?)
        },
    ),
    method(
        "withUpperCase",
        "()Ljava/util/HexFormat;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if read_int_field(vm, this, "upperCase") != 0 {
                return object(this);
            }
            let (delimiter, prefix, suffix, _) = hex_fields(vm, this);
            object(hex_new(vm, &delimiter, &prefix, &suffix, true)?)
        },
    ),
    method(
        "withLowerCase",
        "()Ljava/util/HexFormat;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if read_int_field(vm, this, "upperCase") == 0 {
                return object(this);
            }
            let (delimiter, prefix, suffix, _) = hex_fields(vm, this);
            object(hex_new(vm, &delimiter, &prefix, &suffix, false)?)
        },
    ),
    method(
        "delimiter",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let units = hex_field_units(vm, receiver(args), "delimiter");
            object(vm.make_string_utf16(units)?)
        },
    ),
    method(
        "prefix",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let units = hex_field_units(vm, receiver(args), "prefix");
            object(vm.make_string_utf16(units)?)
        },
    ),
    method(
        "suffix",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let units = hex_field_units(vm, receiver(args), "suffix");
            object(vm.make_string_utf16(units)?)
        },
    ),
    method("isUpperCase", "()Z", ACC_PUBLIC, |vm, _, args| {
        crate::boolean(read_int_field(vm, receiver(args), "upperCase") != 0)
    }),
    method(
        "formatHex",
        "([B)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let bytes_ref = ref_arg(args, 1);
            if bytes_ref.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("bytes")));
            }
            let length = vm.array_length(bytes_ref)? as i32;
            hex_format_range(vm, this, bytes_ref, 0, length)
        },
    ),
    method(
        "formatHex",
        "([BII)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            hex_format_range(
                vm,
                receiver(args),
                ref_arg(args, 1),
                crate::int_arg(args, 2),
                crate::int_arg(args, 3),
            )
        },
    ),
    method(
        "formatHex",
        "(Ljava/lang/Appendable;[B)Ljava/lang/Appendable;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let out = ref_arg(args, 1);
            let bytes_ref = ref_arg(args, 2);
            if out.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("out")));
            }
            if bytes_ref.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("bytes")));
            }
            let length = vm.array_length(bytes_ref)? as i32;
            let text = hex_format_range(vm, this, bytes_ref, 0, length)?;
            let units = vm
                .string_utf16(text.as_ref())
                .map(<[u16]>::to_vec)
                .unwrap_or_default();
            hex_append_units(vm, out, &units)?;
            object(out)
        },
    ),
    method(
        "formatHex",
        "(Ljava/lang/Appendable;[BII)Ljava/lang/Appendable;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let out = ref_arg(args, 1);
            let bytes_ref = ref_arg(args, 2);
            if out.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("out")));
            }
            if bytes_ref.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("bytes")));
            }
            let text = hex_format_range(
                vm,
                this,
                bytes_ref,
                crate::int_arg(args, 3),
                crate::int_arg(args, 4),
            )?;
            let units = vm
                .string_utf16(text.as_ref())
                .map(<[u16]>::to_vec)
                .unwrap_or_default();
            hex_append_units(vm, out, &units)?;
            object(out)
        },
    ),
    method(
        "parseHex",
        "(Ljava/lang/CharSequence;)[B",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let string_ref = ref_arg(args, 1);
            if string_ref.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("string")));
            }
            let length = hex_sequence_units(vm, string_ref)?.len() as i32;
            hex_parse_range(vm, this, string_ref, 0, length)
        },
    ),
    method(
        "parseHex",
        "(Ljava/lang/CharSequence;II)[B",
        ACC_PUBLIC,
        |vm, _, args| {
            hex_parse_range(
                vm,
                receiver(args),
                ref_arg(args, 1),
                crate::int_arg(args, 2),
                crate::int_arg(args, 3),
            )
        },
    ),
    method("parseHex", "([CII)[B", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let array = ref_arg(args, 1);
        if array.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", Some("chars")));
        }
        let from = crate::int_arg(args, 2);
        let to = crate::int_arg(args, 3);
        let length = vm.array_length(array)? as i32;
        hex_check_from_to(vm, from, to, length)?;
        let mut units = Vec::with_capacity((to - from) as usize);
        for index in from..to {
            units.push(vm.array_get(array, index)?.as_int() as u16);
        }
        let (delimiter, prefix, suffix, _) = hex_fields(vm, this);
        let bytes = hex_parse(vm, &units, &delimiter, &prefix, &suffix)?;
        object(bytes_array(vm, &bytes)?)
    }),
    method("toLowHexDigit", "(I)C", ACC_PUBLIC, |vm, _, args| {
        let uppercase = read_int_field(vm, receiver(args), "upperCase") != 0;
        let value = crate::int_arg(args, 1);
        crate::int(i32::from(hex_digit_char((value & 0xf) as u32, uppercase)))
    }),
    method("toHighHexDigit", "(I)C", ACC_PUBLIC, |vm, _, args| {
        let uppercase = read_int_field(vm, receiver(args), "upperCase") != 0;
        let value = crate::int_arg(args, 1);
        crate::int(i32::from(hex_digit_char(
            ((value >> 4) & 0xf) as u32,
            uppercase,
        )))
    }),
    method(
        "toHexDigits",
        "(Ljava/lang/Appendable;B)Ljava/lang/Appendable;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let out = ref_arg(args, 1);
            if out.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let uppercase = read_int_field(vm, this, "upperCase") != 0;
            let value = crate::int_arg(args, 2) as u32;
            hex_append_char(vm, out, hex_digit_char(value >> 4, uppercase))?;
            hex_append_char(vm, out, hex_digit_char(value, uppercase))?;
            object(out)
        },
    ),
    method(
        "toHexDigits",
        "(B)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = (crate::int_arg(args, 1) as u8) as u64;
            object(hex_to_digits(vm, receiver(args), value, 2)?)
        },
    ),
    method(
        "toHexDigits",
        "(C)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = (crate::int_arg(args, 1) as u16) as u64;
            object(hex_to_digits(vm, receiver(args), value, 4)?)
        },
    ),
    method(
        "toHexDigits",
        "(S)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = (crate::int_arg(args, 1) as u16) as u64;
            object(hex_to_digits(vm, receiver(args), value, 4)?)
        },
    ),
    method(
        "toHexDigits",
        "(I)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = (crate::int_arg(args, 1) as u32) as u64;
            object(hex_to_digits(vm, receiver(args), value, 8)?)
        },
    ),
    method(
        "toHexDigits",
        "(J)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = crate::long_arg(args, 1) as u64;
            object(hex_to_digits(vm, receiver(args), value, 16)?)
        },
    ),
    method(
        "toHexDigits",
        "(JI)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = crate::long_arg(args, 1) as u64;
            let digits = crate::int_arg(args, 2);
            if !(0..=16).contains(&digits) {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some(&alloc::format!("number of digits: {digits}")),
                ));
            }
            object(hex_to_digits(vm, receiver(args), value, digits as usize)?)
        },
    ),
    method(
        "isHexDigit",
        "(I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| crate::boolean(hex_is_digit(crate::int_arg(args, 0))),
    ),
    method(
        "fromHexDigit",
        "(I)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let ch = crate::int_arg(args, 0);
            if !hex_is_digit(ch) {
                let shown = char::from_u32(ch as u32).unwrap_or('?');
                return Err(vm.throw_new(
                    "java/lang/NumberFormatException",
                    Some(&alloc::format!(
                        "not a hexadecimal digit: \"{shown}\" = {ch}"
                    )),
                ));
            }
            crate::int(hex_from_digit(vm, ch as u16)?)
        },
    ),
    method(
        "fromHexDigits",
        "(Ljava/lang/CharSequence;)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let string_ref = ref_arg(args, 0);
            if string_ref.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("string")));
            }
            let units = hex_sequence_units(vm, string_ref)?;
            let length = units.len() as i32;
            crate::int(hex_from_digits(vm, &units, 0, length, 8)? as i32)
        },
    ),
    method(
        "fromHexDigits",
        "(Ljava/lang/CharSequence;II)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let string_ref = ref_arg(args, 0);
            if string_ref.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("string")));
            }
            let units = hex_sequence_units(vm, string_ref)?;
            crate::int(hex_from_digits(
                vm,
                &units,
                crate::int_arg(args, 1),
                crate::int_arg(args, 2),
                8,
            )? as i32)
        },
    ),
    method(
        "fromHexDigitsToLong",
        "(Ljava/lang/CharSequence;)J",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let string_ref = ref_arg(args, 0);
            if string_ref.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("string")));
            }
            let units = hex_sequence_units(vm, string_ref)?;
            let length = units.len() as i32;
            Ok(Value::Long(hex_from_digits(vm, &units, 0, length, 16)?))
        },
    ),
    method(
        "fromHexDigitsToLong",
        "(Ljava/lang/CharSequence;II)J",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let string_ref = ref_arg(args, 0);
            if string_ref.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("string")));
            }
            let units = hex_sequence_units(vm, string_ref)?;
            Ok(Value::Long(hex_from_digits(
                vm,
                &units,
                crate::int_arg(args, 1),
                crate::int_arg(args, 2),
                16,
            )?))
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            if this == other {
                return crate::boolean(true);
            }
            if other.is_null() || vm.class_of(this) != vm.class_of(other) {
                return crate::boolean(false);
            }
            let (d1, p1, s1, u1) = hex_fields(vm, this);
            let (d2, p2, s2, u2) = hex_fields(vm, other);
            crate::boolean(u1 == u2 && d1 == d2 && p1 == p2 && s1 == s2)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let (delimiter, prefix, suffix, uppercase) = hex_fields(vm, receiver(args));
        let mut result = 1i32;
        result = result
            .wrapping_mul(31)
            .wrapping_add(hex_string_hash(&delimiter));
        result = result
            .wrapping_mul(31)
            .wrapping_add(hex_string_hash(&prefix));
        result = result
            .wrapping_mul(31)
            .wrapping_add(hex_string_hash(&suffix));
        result = result
            .wrapping_mul(31)
            .wrapping_add(if uppercase { 1231 } else { 1237 });
        crate::int(result)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (delimiter, prefix, suffix, uppercase) = hex_fields(vm, receiver(args));
            let mut out = Vec::new();
            hex_push_ascii(&mut out, "uppercase: ");
            hex_push_ascii(&mut out, if uppercase { "true" } else { "false" });
            hex_push_ascii(&mut out, ", delimiter: \"");
            out.extend(hex_escape_nl(&delimiter));
            hex_push_ascii(&mut out, "\", prefix: \"");
            out.extend(hex_escape_nl(&prefix));
            hex_push_ascii(&mut out, "\", suffix: \"");
            out.extend(hex_escape_nl(&suffix));
            hex_push_ascii(&mut out, "\"");
            object(vm.make_string_utf16(out)?)
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
        field("prefix", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("suffix", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("upperCase", "I", ACC_PRIVATE, None),
    ],
    &HEX_FORMAT_METHODS,
    None,
);
