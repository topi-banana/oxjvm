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

const HEX_FORMAT_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
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
            let uppercase = read_int_field(vm, receiver(args), "upperCase") != 0;
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
            write_int_field(vm, this, "upperCase", 1);
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
    method(
        "toHexDigits",
        "(I)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let uppercase = read_int_field(vm, receiver(args), "upperCase") != 0;
            let value = crate::int_arg(args, 1) as u32;
            let text = if uppercase {
                alloc::format!("{:08X}", value)
            } else {
                alloc::format!("{:08x}", value)
            };
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toHexDigits",
        "(J)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let uppercase = read_int_field(vm, receiver(args), "upperCase") != 0;
            let value = crate::long_arg(args, 1) as u64;
            let text = if uppercase {
                alloc::format!("{:016X}", value)
            } else {
                alloc::format!("{:016x}", value)
            };
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toHexDigits",
        "(C)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let uppercase = read_int_field(vm, receiver(args), "upperCase") != 0;
            let value = crate::int_arg(args, 1) as u32 & 0xFFFF;
            let text = if uppercase {
                alloc::format!("{:04X}", value)
            } else {
                alloc::format!("{:04x}", value)
            };
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "fromHexDigits",
        "(Ljava/lang/CharSequence;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 1));
            let value = u32::from_str_radix(text.trim(), 16)
                .map_err(|_| vm.throw_new("java/lang/NumberFormatException", Some(&text)))?;
            crate::int(value as i32)
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
