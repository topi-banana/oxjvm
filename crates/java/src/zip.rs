//! `java.util.zip`: stored-entry `ZipOutputStream`/`ZipEntry` and `CRC32`.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ArrayComponent, ObjectRef, Value, Vm, VmError};

use crate::{class, field, int, long, method, object, receiver, ref_arg, string_arg, void};

/// The IEEE CRC-32 of a byte slice.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

const ZIP_ENTRY_METHODS: [oxjvm_vm::NativeMethodDef; 25] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = string_arg(vm, args[1])?;
            let this = receiver(args);
            write_string_field(vm, this, "name", &name);
            write_long_field(vm, this, "time", -1);
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/util/zip/ZipEntry;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            if let Some(name) = read_string_field(vm, other, "name") {
                write_string_field(vm, this, "name", &name);
            }
            for name in ["time", "size", "compressedSize", "crc", "offset"] {
                let value = read_long_field(vm, other, name);
                write_long_field(vm, this, name, value);
            }
            let method = read_int_field(vm, other, "method");
            write_int_field_named(vm, this, "method", method);
            if let Some(extra) = read_ref_field(vm, other, "extra", "[B") {
                write_ref_field(vm, this, "extra", "[B", extra);
            }
            if let Some(comment) = read_string_field(vm, other, "comment") {
                write_string_field(vm, this, "comment", &comment);
            }
            void()
        },
    ),
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| match read_string_field(vm, receiver(args), "name") {
            Some(name) => object(vm.make_string(&name)?),
            None => object(ObjectRef::NULL),
        },
    ),
    method(
        "setName",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = string_arg(vm, args[1])?;
            write_string_field(vm, receiver(args), "name", &name);
            void()
        },
    ),
    method("getTime", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "time"))
    }),
    method(
        "setTimeLocal",
        "(Ljava/time/LocalDateTime;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let time = ref_arg(args, 1);
            if time.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let zone = crate::time::system_default_zone(vm)?;
            let offset = i64::from(crate::time::zone_offset_seconds(vm, zone));
            let epoch_day = read_long_field(vm, time, "epochDay");
            let nano_of_day = read_long_field(vm, time, "nanoOfDay");
            let epoch_second = epoch_day * 86_400 + nano_of_day / 1_000_000_000 - offset;
            let millis = epoch_second * 1000 + nano_of_day % 1_000_000_000 / 1_000_000;
            write_long_field(vm, this, "time", millis);
            void()
        },
    ),
    method(
        "getTimeLocal",
        "()Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let time = read_long_field(vm, this, "time");
            if time == -1 {
                // The JDK decodes the unset DOS time (-1) and fails on month 15.
                return Err(vm.throw_new(
                    "java/time/DateTimeException",
                    Some("Invalid value for MonthOfYear (valid values 1 - 12): 15"),
                ));
            }
            let zone = crate::time::system_default_zone(vm)?;
            object(crate::time::local_date_time_of_epoch_millis(
                vm, time, zone,
            )?)
        },
    ),
    method("setTime", "(J)V", ACC_PUBLIC, |vm, _, args| {
        write_long_field(vm, receiver(args), "time", crate::long_arg(args, 1));
        void()
    }),
    method(
        "getLastModifiedTime",
        "()Ljava/nio/file/attribute/FileTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let time = read_long_field(vm, receiver(args), "time");
            if time == -1 {
                return object(ObjectRef::NULL);
            }
            let class = vm.resolve_class("java/nio/file/attribute/FileTime")?;
            let (declaring, method) = vm
                .find_method(class, "fromMillis", "(J)Ljava/nio/file/attribute/FileTime;")
                .ok_or_else(|| VmError::internal("FileTime.fromMillis missing"))?;
            vm.invoke_method(declaring, method, vec![Value::Long(time)])
        },
    ),
    method(
        "setLastModifiedTime",
        "(Ljava/nio/file/attribute/FileTime;)Ljava/util/zip/ZipEntry;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let file_time = ref_arg(args, 1);
            if file_time.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let (class, method) = vm.resolve_virtual_method(file_time, "toMillis", "()J")?;
            let millis = vm
                .invoke_method(class, method, vec![Value::Ref(file_time)])?
                .as_long();
            write_long_field(vm, this, "time", millis);
            object(this)
        },
    ),
    method("getSize", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "size"))
    }),
    method("setSize", "(J)V", ACC_PUBLIC, |vm, _, args| {
        write_long_field(vm, receiver(args), "size", crate::long_arg(args, 1));
        void()
    }),
    method("getCompressedSize", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "compressedSize"))
    }),
    method("setCompressedSize", "(J)V", ACC_PUBLIC, |vm, _, args| {
        write_long_field(
            vm,
            receiver(args),
            "compressedSize",
            crate::long_arg(args, 1),
        );
        void()
    }),
    method("getCrc", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "crc"))
    }),
    method("setCrc", "(J)V", ACC_PUBLIC, |vm, _, args| {
        write_long_field(vm, receiver(args), "crc", crate::long_arg(args, 1));
        void()
    }),
    method("getMethod", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "method"))
    }),
    method("setMethod", "(I)V", ACC_PUBLIC, |vm, _, args| {
        write_int_field_named(vm, receiver(args), "method", crate::int_arg(args, 1));
        void()
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| match read_string_field(vm, receiver(args), "name") {
            Some(name) => object(vm.make_string(&name)?),
            None => object(ObjectRef::NULL),
        },
    ),
    method("setExtra", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let extra = ref_arg(args, 1);
        if !extra.is_null() {
            let length = vm.array_length(extra)?;
            if length > 0xFFFF {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("invalid extra field length"),
                ));
            }
        }
        write_ref_field(vm, this, "extra", "[B", extra);
        void()
    }),
    method("getExtra", "()[B", ACC_PUBLIC, |vm, _, args| {
        object(read_ref_field(vm, receiver(args), "extra", "[B").unwrap_or(ObjectRef::NULL))
    }),
    method(
        "setComment",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let comment = ref_arg(args, 1);
            if !comment.is_null() {
                let units = vm.string_utf16(comment).unwrap_or_default();
                let encoded = crate::io::modified_utf8(units).len();
                if encoded > 0xFFFF {
                    return Err(vm.throw_new(
                        "java/lang/IllegalArgumentException",
                        Some("invalid entry comment length"),
                    ));
                }
            }
            write_ref_field(vm, this, "comment", "Ljava/lang/String;", comment);
            void()
        },
    ),
    method(
        "getComment",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(
                read_ref_field(vm, receiver(args), "comment", "Ljava/lang/String;")
                    .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method("isDirectory", "()Z", ACC_PUBLIC, |vm, _, args| {
        let is_dir =
            read_string_field(vm, receiver(args), "name").is_some_and(|name| name.ends_with('/'));
        crate::boolean(is_dir)
    }),
    method(
        "clone",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.resolve_class("java/util/zip/ZipEntry")?;
            let copy = vm.new_instance(class)?;
            if let Some(name) = read_string_field(vm, this, "name") {
                write_string_field(vm, copy, "name", &name);
            }
            for name in ["time", "size", "compressedSize", "crc", "offset"] {
                let value = read_long_field(vm, this, name);
                write_long_field(vm, copy, name, value);
            }
            let method = read_int_field(vm, this, "method");
            write_int_field_named(vm, copy, "method", method);
            if let Some(extra) = read_ref_field(vm, this, "extra", "[B") {
                write_ref_field(vm, copy, "extra", "[B", extra);
            }
            if let Some(comment) = read_string_field(vm, this, "comment") {
                write_string_field(vm, copy, "comment", &comment);
            }
            object(copy)
        },
    ),
];

pub(crate) const ZIP_ENTRY: oxjvm_vm::NativeClass = class(
    "java/util/zip/ZipEntry",
    Some("java/lang/Object"),
    &["java/lang/Cloneable"],
    ACC_PUBLIC,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("time", "J", ACC_PRIVATE, None),
        field("size", "J", ACC_PRIVATE, None),
        field("compressedSize", "J", ACC_PRIVATE, None),
        field("crc", "J", ACC_PRIVATE, None),
        field("method", "I", ACC_PRIVATE, None),
        field("offset", "J", ACC_PRIVATE, None),
        field("extra", "[B", ACC_PRIVATE, None),
        field("comment", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &ZIP_ENTRY_METHODS,
    None,
);

const ZIP_OUTPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
    method(
        "<init>",
        "(Ljava/io/OutputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let target = ref_arg(args, 1);
            if target.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            write_ref_field(vm, this, "target", "Ljava/io/OutputStream;", target);
            let buffer = new_instance(vm, "java/io/ByteArrayOutputStream")?;
            write_ref_field(
                vm,
                this,
                "buffer",
                "Ljava/io/ByteArrayOutputStream;",
                buffer,
            );
            let entries = new_array_list(vm)?;
            write_ref_field(vm, this, "entries", "Ljava/util/ArrayList;", entries);
            write_long_field(vm, this, "offset", 0);
            write_long_field(vm, this, "entryOffset", 0);
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/OutputStream;Ljava/nio/charset/Charset;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let target = ref_arg(args, 1);
            let charset = ref_arg(args, 2);
            if target.is_null() || charset.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let class = vm.class_of(this);
            let (declaring, ctor) = vm
                .find_method(class, "<init>", "(Ljava/io/OutputStream;)V")
                .ok_or_else(|| VmError::internal("ZipOutputStream.<init> missing"))?;
            vm.invoke_method(declaring, ctor, vec![Value::Ref(this), Value::Ref(target)])?;
            write_ref_field(vm, this, "charset", "Ljava/nio/charset/Charset;", charset);
            void()
        },
    ),
    method(
        "setComment",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let comment = ref_arg(args, 1);
            if !comment.is_null() && vm.string_or_empty(comment).into_bytes().len() > 0xFFFF {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("invalid comment length"),
                ));
            }
            write_ref_field(vm, this, "comment", "Ljava/lang/String;", comment);
            void()
        },
    ),
    method(
        "putNextEntry",
        "(Ljava/util/zip/ZipEntry;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let entry = ref_arg(args, 1);
            if read_int_field(vm, this, "closed") != 0 {
                return Err(vm.throw_new("java/io/IOException", Some("Stream closed")));
            }
            if entry.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let buffer = new_instance(vm, "java/io/ByteArrayOutputStream")?;
            write_ref_field(
                vm,
                this,
                "buffer",
                "Ljava/io/ByteArrayOutputStream;",
                buffer,
            );
            let offset = read_long_field(vm, this, "offset");
            write_long_field(vm, this, "entryOffset", offset);
            write_ref_field(vm, this, "entry", "Ljava/util/zip/ZipEntry;", entry);
            void()
        },
    ),
    method("write", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let byte = crate::int_arg(args, 1) as u8;
        zip_buffer_write(vm, receiver(args), &[byte])
    }),
    method("write", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
        zip_buffer_write(vm, receiver(args), &bytes)
    }),
    method("write", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
        let offset = crate::int_arg(args, 2);
        let count = crate::int_arg(args, 3);
        if offset < 0 || count < 0 || (offset + count) as usize > bytes.len() {
            return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
        }
        let slice = bytes[offset as usize..(offset + count) as usize].to_vec();
        zip_buffer_write(vm, receiver(args), &slice)
    }),
    method("closeEntry", "()V", ACC_PUBLIC, |vm, _, args| {
        zip_close_entry(vm, receiver(args))
    }),
    method("finish", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if read_int_field(vm, this, "closed") != 0 {
            return Err(vm.throw_new("java/io/IOException", Some("Stream closed")));
        }
        zip_finish(vm, this)
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if read_int_field(vm, this, "closed") != 0 {
            return void();
        }
        zip_finish(vm, this)?;
        if let Some(target) = read_ref_field(vm, this, "target", "Ljava/io/OutputStream;") {
            if !target.is_null() {
                let (class, close) = vm.resolve_virtual_method(target, "close", "()V")?;
                vm.invoke_method(class, close, vec![Value::Ref(target)])?;
            }
        }
        write_int_field_named(vm, this, "closed", 1);
        void()
    }),
    method("flush", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if let Some(target) = read_ref_field(vm, this, "target", "Ljava/io/OutputStream;") {
            if !target.is_null() {
                let (class, flush) = vm.resolve_virtual_method(target, "flush", "()V")?;
                vm.invoke_method(class, flush, vec![Value::Ref(target)])?;
            }
        }
        void()
    }),
];

pub(crate) const ZIP_OUTPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/util/zip/ZipOutputStream",
    Some("java/io/FilterOutputStream"),
    &[],
    ACC_PUBLIC,
    &[
        field("target", "Ljava/io/OutputStream;", ACC_PRIVATE, None),
        field(
            "buffer",
            "Ljava/io/ByteArrayOutputStream;",
            ACC_PRIVATE,
            None,
        ),
        field("entry", "Ljava/util/zip/ZipEntry;", ACC_PRIVATE, None),
        field("entries", "Ljava/util/ArrayList;", ACC_PRIVATE, None),
        field("offset", "J", ACC_PRIVATE, None),
        field("entryOffset", "J", ACC_PRIVATE, None),
        field("charset", "Ljava/nio/charset/Charset;", ACC_PRIVATE, None),
        field("comment", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("closed", "I", ACC_PRIVATE, None),
    ],
    &ZIP_OUTPUT_STREAM_METHODS,
    None,
);

fn new_instance(vm: &mut Vm<'_>, class_name: &str) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class(class_name)?;
    let instance = vm.new_instance(class)?;
    if let Some((declaring, ctor)) = vm.find_method(class, "<init>", "()V") {
        vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
    }
    Ok(instance)
}

fn new_array_list(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    new_instance(vm, "java/util/ArrayList")
}

fn zip_buffer_write(vm: &mut Vm<'_>, this: ObjectRef, bytes: &[u8]) -> Result<Value, VmError> {
    if read_int_field(vm, this, "closed") != 0 {
        return Err(vm.throw_new("java/io/IOException", Some("Stream closed")));
    }
    let Some(buffer) = read_ref_field(vm, this, "buffer", "Ljava/io/ByteArrayOutputStream;") else {
        return Err(vm.throw_new("java/io/IOException", Some("No current entry")));
    };
    if buffer.is_null() {
        return Err(vm.throw_new("java/io/IOException", Some("No current entry")));
    }
    let array = vm.allocate_array_of(ArrayComponent::Byte, bytes.len())?;
    for (index, byte) in bytes.iter().enumerate() {
        vm.array_set(
            array,
            index as i32,
            Value::Int(i32::from(*byte as i8)),
            false,
        )?;
    }
    let (class, method) = vm.resolve_virtual_method(buffer, "write", "([BII)V")?;
    vm.invoke_method(
        class,
        method,
        vec![
            Value::Ref(buffer),
            Value::Ref(array),
            Value::Int(0),
            Value::Int(bytes.len() as i32),
        ],
    )?;
    void()
}

fn zip_close_entry(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Value, VmError> {
    if read_int_field(vm, this, "closed") != 0 {
        return Err(vm.throw_new("java/io/IOException", Some("Stream closed")));
    }
    let Some(entry) = read_ref_field(vm, this, "entry", "Ljava/util/zip/ZipEntry;") else {
        return void();
    };
    if entry.is_null() {
        return void();
    }
    let buffer = read_ref_field(vm, this, "buffer", "Ljava/io/ByteArrayOutputStream;")
        .unwrap_or(ObjectRef::NULL);
    let data = byte_array_output_bytes(vm, buffer)?;
    let crc = crc32(&data);
    let name = read_string_field(vm, entry, "name").unwrap_or_default();
    let offset = read_long_field(vm, this, "entryOffset");
    let name_bytes = name.clone().into_bytes();
    let mut header = Vec::with_capacity(30 + name_bytes.len() + data.len());
    header.extend_from_slice(&0x0403_4B50u32.to_le_bytes());
    header.extend_from_slice(&20u16.to_le_bytes());
    header.extend_from_slice(&0x0800u16.to_le_bytes());
    header.extend_from_slice(&0u16.to_le_bytes());
    header.extend_from_slice(&0u16.to_le_bytes());
    header.extend_from_slice(&0u16.to_le_bytes());
    header.extend_from_slice(&crc.to_le_bytes());
    header.extend_from_slice(&(data.len() as u32).to_le_bytes());
    header.extend_from_slice(&(data.len() as u32).to_le_bytes());
    header.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
    header.extend_from_slice(&0u16.to_le_bytes());
    header.extend_from_slice(&name_bytes);
    header.extend_from_slice(&data);
    zip_write_target(vm, this, &header)?;
    write_long_field(vm, entry, "crc", i64::from(crc));
    write_long_field(vm, entry, "size", data.len() as i64);
    write_long_field(vm, entry, "compressedSize", data.len() as i64);
    write_long_field(vm, entry, "offset", offset);
    let entries =
        read_ref_field(vm, this, "entries", "Ljava/util/ArrayList;").unwrap_or(ObjectRef::NULL);
    if !entries.is_null() {
        let (class, add) = vm.resolve_virtual_method(entries, "add", "(Ljava/lang/Object;)Z")?;
        vm.invoke_method(class, add, vec![Value::Ref(entries), Value::Ref(entry)])?;
    }
    write_long_field(vm, this, "offset", offset + header.len() as i64);
    write_long_field(vm, this, "entryOffset", offset + header.len() as i64);
    write_ref_field(
        vm,
        this,
        "entry",
        "Ljava/util/zip/ZipEntry;",
        ObjectRef::NULL,
    );
    void()
}

fn zip_finish(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Value, VmError> {
    if read_long_field(vm, this, "offset") < 0 {
        return void();
    }
    if let Some(entry) = read_ref_field(vm, this, "entry", "Ljava/util/zip/ZipEntry;") {
        if !entry.is_null() {
            zip_close_entry(vm, this)?;
        }
    }
    let entries =
        read_ref_field(vm, this, "entries", "Ljava/util/ArrayList;").unwrap_or(ObjectRef::NULL);
    let entries = if entries.is_null() {
        Vec::new()
    } else {
        crate::util::array_list_elements(vm, entries)
    };
    let cd_offset = read_long_field(vm, this, "offset");
    let mut central = Vec::new();
    for entry in &entries {
        let name = read_string_field(vm, *entry, "name").unwrap_or_default();
        let name_bytes = name.clone().into_bytes();
        let crc = read_long_field(vm, *entry, "crc") as u32;
        let size = read_long_field(vm, *entry, "size") as u32;
        let offset = read_long_field(vm, *entry, "offset") as u32;
        central.extend_from_slice(&0x0201_4B50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&0x0800u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(&name_bytes);
    }
    let comment_bytes = read_string_field(vm, this, "comment")
        .unwrap_or_default()
        .into_bytes();
    let mut end = Vec::new();
    end.extend_from_slice(&0x0605_4B50u32.to_le_bytes());
    end.extend_from_slice(&0u16.to_le_bytes());
    end.extend_from_slice(&0u16.to_le_bytes());
    end.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    end.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    end.extend_from_slice(&(central.len() as u32).to_le_bytes());
    end.extend_from_slice(&(cd_offset as u32).to_le_bytes());
    end.extend_from_slice(&(comment_bytes.len() as u16).to_le_bytes());
    end.extend_from_slice(&comment_bytes);
    let mut tail = central;
    tail.extend_from_slice(&end);
    zip_write_target(vm, this, &tail)?;
    write_long_field(vm, this, "offset", -1);
    void()
}

fn zip_write_target(vm: &mut Vm<'_>, this: ObjectRef, bytes: &[u8]) -> Result<(), VmError> {
    let Some(target) = read_ref_field(vm, this, "target", "Ljava/io/OutputStream;") else {
        return Err(vm.throw_new("java/io/IOException", Some("No target stream")));
    };
    if target.is_null() {
        return Err(vm.throw_new("java/io/IOException", Some("No target stream")));
    }
    let array = vm.allocate_array_of(ArrayComponent::Byte, bytes.len())?;
    for (index, byte) in bytes.iter().enumerate() {
        vm.array_set(
            array,
            index as i32,
            Value::Int(i32::from(*byte as i8)),
            false,
        )?;
    }
    let (class, method) = vm.resolve_virtual_method(target, "write", "([BII)V")?;
    vm.invoke_method(
        class,
        method,
        vec![
            Value::Ref(target),
            Value::Ref(array),
            Value::Int(0),
            Value::Int(bytes.len() as i32),
        ],
    )?;
    Ok(())
}

fn byte_array_output_bytes(vm: &mut Vm<'_>, buffer: ObjectRef) -> Result<Vec<u8>, VmError> {
    if buffer.is_null() {
        return Ok(Vec::new());
    }
    let class = vm.class_of(buffer);
    let array = vm
        .read_ref_field(buffer, class, "buf", "[B")
        .unwrap_or(ObjectRef::NULL);
    let count = crate::lang::read_int_field(vm, buffer, "count").max(0) as usize;
    if array.is_null() {
        return Ok(Vec::new());
    }
    let mut bytes = Vec::with_capacity(count);
    for index in 0..count {
        bytes.push(vm.array_get(array, index as i32)?.as_int() as u8);
    }
    Ok(bytes)
}

// -------------------------------------------------------------------------------------------
// CRC32
// -------------------------------------------------------------------------------------------

const CRC32_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "update",
        "(Ljava/nio/ByteBuffer;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let bytes = checksum_buffer_bytes(vm, ref_arg(args, 1))?;
            crc32_update(vm, receiver(args), &bytes);
            void()
        },
    ),
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        write_int_field_named(vm, receiver(args), "value", 0);
        void()
    }),
    method("update", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let byte = crate::int_arg(args, 1) as u8;
        crc32_update(vm, receiver(args), &[byte]);
        void()
    }),
    method("update", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
        crc32_update(vm, receiver(args), &bytes);
        void()
    }),
    method("update", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
        let offset = crate::int_arg(args, 2);
        let count = crate::int_arg(args, 3);
        check_byte_range(vm, bytes.len(), offset, count)?;
        crc32_update(
            vm,
            receiver(args),
            &bytes[offset as usize..(offset + count) as usize],
        );
        void()
    }),
    method("getValue", "()J", ACC_PUBLIC, |vm, _, args| {
        long(i64::from(read_int_field(vm, receiver(args), "value") as u32))
    }),
    method("reset", "()V", ACC_PUBLIC, |vm, _, args| {
        write_int_field_named(vm, receiver(args), "value", 0);
        void()
    }),
];

fn checked_input_ctor(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    input: ObjectRef,
    checksum: ObjectRef,
) -> Result<(), VmError> {
    let (declaring, field) = vm.find_field(vm.class_of(this), "in", "Ljava/io/InputStream;")?;
    vm.set_instance_ref(this, declaring, field, input);
    let (declaring, field) =
        vm.find_field(vm.class_of(this), "checksum", "Ljava/util/zip/Checksum;")?;
    vm.set_instance_ref(this, declaring, field, checksum);
    Ok(())
}

fn checked_input_stream(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    read_ref_field(vm, this, "in", "Ljava/io/InputStream;").unwrap_or(ObjectRef::NULL)
}

fn checked_input_checksum(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    read_ref_field(vm, this, "checksum", "Ljava/util/zip/Checksum;").unwrap_or(ObjectRef::NULL)
}

const CHECKED_INPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method(
        "<init>",
        "(Ljava/io/InputStream;Ljava/util/zip/Checksum;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            checked_input_ctor(vm, receiver(args), ref_arg(args, 1), ref_arg(args, 2))?;
            void()
        },
    ),
    method("read", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let input = checked_input_stream(vm, this);
        let (class, method) = vm.resolve_virtual_method(input, "read", "()I")?;
        let byte = vm
            .invoke_method(class, method, vec![Value::Ref(input)])?
            .as_int();
        if byte >= 0 {
            let checksum = checked_input_checksum(vm, this);
            if !checksum.is_null() {
                let (class, method) = vm.resolve_virtual_method(checksum, "update", "(I)V")?;
                vm.invoke_method(class, method, vec![Value::Ref(checksum), Value::Int(byte)])?;
            }
        }
        int(byte)
    }),
    method("read", "([BII)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let input = checked_input_stream(vm, this);
        let buffer = ref_arg(args, 1);
        let offset = crate::int_arg(args, 2);
        let length = crate::int_arg(args, 3);
        let (class, method) = vm.resolve_virtual_method(input, "read", "([BII)I")?;
        let count = vm
            .invoke_method(
                class,
                method,
                vec![
                    Value::Ref(input),
                    Value::Ref(buffer),
                    Value::Int(offset),
                    Value::Int(length),
                ],
            )?
            .as_int();
        if count > 0 {
            let checksum = checked_input_checksum(vm, this);
            if !checksum.is_null() {
                let (class, method) = vm.resolve_virtual_method(checksum, "update", "([BII)V")?;
                vm.invoke_method(
                    class,
                    method,
                    vec![
                        Value::Ref(checksum),
                        Value::Ref(buffer),
                        Value::Int(offset),
                        Value::Int(count),
                    ],
                )?;
            }
        }
        int(count)
    }),
    method(
        "getChecksum",
        "()Ljava/util/zip/Checksum;",
        ACC_PUBLIC,
        |vm, _, args| object(checked_input_checksum(vm, receiver(args))),
    ),
    method("available", "()I", ACC_PUBLIC, |vm, _, args| {
        let input = checked_input_stream(vm, receiver(args));
        let (class, method) = vm.resolve_virtual_method(input, "available", "()I")?;
        vm.invoke_method(class, method, vec![Value::Ref(input)])
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let input = checked_input_stream(vm, receiver(args));
        let (class, method) = vm.resolve_virtual_method(input, "close", "()V")?;
        vm.invoke_method(class, method, vec![Value::Ref(input)])?;
        void()
    }),
    method("skip", "(J)J", ACC_PUBLIC, |vm, _, args| {
        let input = checked_input_stream(vm, receiver(args));
        let count = crate::long_arg(args, 1);
        let (class, method) = vm.resolve_virtual_method(input, "skip", "(J)J")?;
        vm.invoke_method(class, method, vec![Value::Ref(input), Value::Long(count)])
    }),
    method("mark", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let input = checked_input_stream(vm, receiver(args));
        let limit = crate::int_arg(args, 1);
        let (class, method) = vm.resolve_virtual_method(input, "mark", "(I)V")?;
        vm.invoke_method(class, method, vec![Value::Ref(input), Value::Int(limit)])?;
        void()
    }),
    method("reset", "()V", ACC_PUBLIC, |vm, _, args| {
        let input = checked_input_stream(vm, receiver(args));
        let (class, method) = vm.resolve_virtual_method(input, "reset", "()V")?;
        vm.invoke_method(class, method, vec![Value::Ref(input)])?;
        void()
    }),
    method("markSupported", "()Z", ACC_PUBLIC, |vm, _, args| {
        let input = checked_input_stream(vm, receiver(args));
        let (class, method) = vm.resolve_virtual_method(input, "markSupported", "()Z")?;
        vm.invoke_method(class, method, vec![Value::Ref(input)])
    }),
];

/// `java.util.zip.CheckedInputStream`: an InputStream that updates a Checksum as it reads.
pub(crate) const CHECKED_INPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/util/zip/CheckedInputStream",
    Some("java/io/InputStream"),
    &[],
    ACC_PUBLIC,
    &[
        field("in", "Ljava/io/InputStream;", ACC_PRIVATE, None),
        field("checksum", "Ljava/util/zip/Checksum;", ACC_PRIVATE, None),
    ],
    &CHECKED_INPUT_STREAM_METHODS,
    None,
);

pub(crate) const CRC32: oxjvm_vm::NativeClass = class(
    "java/util/zip/CRC32",
    Some("java/lang/Object"),
    &["java/util/zip/Checksum"],
    ACC_PUBLIC,
    &[field("value", "I", ACC_PRIVATE, None)],
    &CRC32_METHODS,
    None,
);

const CRC32C_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "update",
        "(Ljava/nio/ByteBuffer;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let bytes = checksum_buffer_bytes(vm, ref_arg(args, 1))?;
            crc32c_update(vm, receiver(args), &bytes);
            void()
        },
    ),
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        write_int_field_named(vm, receiver(args), "value", 0);
        void()
    }),
    method("update", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let byte = crate::int_arg(args, 1) as u8;
        crc32c_update(vm, receiver(args), &[byte]);
        void()
    }),
    method("update", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
        crc32c_update(vm, receiver(args), &bytes);
        void()
    }),
    method("update", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
        let offset = crate::int_arg(args, 2);
        let count = crate::int_arg(args, 3);
        check_byte_range(vm, bytes.len(), offset, count)?;
        crc32c_update(
            vm,
            receiver(args),
            &bytes[offset as usize..(offset + count) as usize],
        );
        void()
    }),
    method("getValue", "()J", ACC_PUBLIC, |vm, _, args| {
        long(i64::from(read_int_field(vm, receiver(args), "value") as u32))
    }),
    method("reset", "()V", ACC_PUBLIC, |vm, _, args| {
        write_int_field_named(vm, receiver(args), "value", 0);
        void()
    }),
];

pub(crate) const CRC32C: oxjvm_vm::NativeClass = class(
    "java/util/zip/CRC32C",
    Some("java/lang/Object"),
    &["java/util/zip/Checksum"],
    ACC_PUBLIC,
    &[field("value", "I", ACC_PRIVATE, None)],
    &CRC32C_METHODS,
    None,
);

const ADLER32_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "update",
        "(Ljava/nio/ByteBuffer;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let bytes = checksum_buffer_bytes(vm, ref_arg(args, 1))?;
            adler32_update(vm, receiver(args), &bytes);
            void()
        },
    ),
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        write_int_field_named(vm, receiver(args), "value", 1);
        void()
    }),
    method("update", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let byte = crate::int_arg(args, 1) as u8;
        adler32_update(vm, receiver(args), &[byte]);
        void()
    }),
    method("update", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
        adler32_update(vm, receiver(args), &bytes);
        void()
    }),
    method("update", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
        let offset = crate::int_arg(args, 2);
        let count = crate::int_arg(args, 3);
        check_byte_range(vm, bytes.len(), offset, count)?;
        adler32_update(
            vm,
            receiver(args),
            &bytes[offset as usize..(offset + count) as usize],
        );
        void()
    }),
    method("getValue", "()J", ACC_PUBLIC, |vm, _, args| {
        long(i64::from(read_int_field(vm, receiver(args), "value") as u32))
    }),
    method("reset", "()V", ACC_PUBLIC, |vm, _, args| {
        write_int_field_named(vm, receiver(args), "value", 1);
        void()
    }),
];

pub(crate) const ADLER32: oxjvm_vm::NativeClass = class(
    "java/util/zip/Adler32",
    Some("java/lang/Object"),
    &["java/util/zip/Checksum"],
    ACC_PUBLIC,
    &[field("value", "I", ACC_PRIVATE, None)],
    &ADLER32_METHODS,
    None,
);

const CHECKSUM_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "update",
        "(I)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "update",
        "([B)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "update",
        "([BII)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getValue",
        "()J",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "reset",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const CHECKSUM: oxjvm_vm::NativeClass = class(
    "java/util/zip/Checksum",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &CHECKSUM_METHODS,
    None,
);

fn crc32_update(vm: &mut Vm<'_>, this: ObjectRef, bytes: &[u8]) {
    let previous = read_int_field(vm, this, "value") as u32;
    let mut crc = !previous;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    write_int_field_named(vm, this, "value", !crc as i32);
}

/// CRC-32C (Castagnoli), the reflected polynomial `0x82F63B78`.
fn crc32c_update(vm: &mut Vm<'_>, this: ObjectRef, bytes: &[u8]) {
    let previous = read_int_field(vm, this, "value") as u32;
    let mut crc = !previous;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0x82F6_3B78 & mask);
        }
    }
    write_int_field_named(vm, this, "value", !crc as i32);
}

/// Adler-32 (RFC 1950): two running sums modulo 65521, packed `b<<16 | a`.
fn adler32_update(vm: &mut Vm<'_>, this: ObjectRef, bytes: &[u8]) {
    let previous = read_int_field(vm, this, "value") as u32;
    let mut a = previous & 0xFFFF;
    let mut b = (previous >> 16) & 0xFFFF;
    for byte in bytes {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    write_int_field_named(vm, this, "value", ((b << 16) | a) as i32);
}

// -------------------------------------------------------------------------------------------
// Field helpers shared by the zip classes
// -------------------------------------------------------------------------------------------

fn read_int_field(vm: &Vm<'_>, this: ObjectRef, name: &str) -> i32 {
    let class = vm.class_of(this);
    vm.read_instance_int_named(this, class, name, "I")
        .unwrap_or(0)
}

fn write_int_field_named(vm: &mut Vm<'_>, this: ObjectRef, name: &str, value: i32) {
    let class = vm.class_of(this);
    if let Some((declaring, field)) = vm.find_field_opt(class, name, "I") {
        vm.set_instance_int(this, declaring, field, value);
    }
}

fn read_long_field(vm: &Vm<'_>, this: ObjectRef, name: &str) -> i64 {
    let class = vm.class_of(this);
    if let Some((declaring, field)) = vm.find_field_opt(class, name, "J") {
        let slot = vm.classes.get(declaring).fields[field as usize].slot as usize;
        if let Some(oxjvm_vm::ObjectData::Instance(fields)) = vm.heap.get(this).map(|o| &o.data) {
            return fields.get(slot).map(|value| value.as_long()).unwrap_or(0);
        }
    }
    0
}

fn write_long_field(vm: &mut Vm<'_>, this: ObjectRef, name: &str, value: i64) {
    let class = vm.class_of(this);
    if let Some((declaring, field)) = vm.find_field_opt(class, name, "J") {
        let slot = vm.classes.get(declaring).fields[field as usize].slot as usize;
        if let Some(oxjvm_vm::ObjectData::Instance(fields)) =
            vm.heap.get_mut(this).map(|o| &mut o.data)
        {
            fields[slot] = Value::Long(value);
        }
    }
}

fn read_string_field(vm: &Vm<'_>, this: ObjectRef, name: &str) -> Option<String> {
    let class = vm.class_of(this);
    vm.read_string_field(this, class, name)
}

fn write_string_field(vm: &mut Vm<'_>, this: ObjectRef, name: &str, value: &str) {
    let class = vm.class_of(this);
    if let Some((declaring, field)) = vm.find_field_opt(class, name, "Ljava/lang/String;") {
        if let Ok(text) = vm.make_string(value) {
            vm.set_instance_ref(this, declaring, field, text);
        }
    }
}

fn read_ref_field(vm: &Vm<'_>, this: ObjectRef, name: &str, descriptor: &str) -> Option<ObjectRef> {
    let class = vm.class_of(this);
    vm.read_ref_field(this, class, name, descriptor)
}

fn write_ref_field(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: &str,
    descriptor: &str,
    value: ObjectRef,
) {
    let class = vm.class_of(this);
    if let Some((declaring, field)) = vm.find_field_opt(class, name, descriptor) {
        vm.set_instance_ref(this, declaring, field, value);
    }
}

// -------------------------------------------------------------------------------------------
// java.util.zip.ZipFile
// -------------------------------------------------------------------------------------------

/// Allocate a Java `byte[]` holding `bytes`.
fn zip_byte_array(vm: &mut Vm<'_>, bytes: &[u8]) -> Result<ObjectRef, VmError> {
    let array = vm.allocate_array_of(ArrayComponent::Byte, bytes.len())?;
    for (index, byte) in bytes.iter().enumerate() {
        vm.array_set(
            array,
            index as i32,
            Value::Int(i32::from(*byte as i8)),
            false,
        )?;
    }
    Ok(array)
}

/// Build a `ZipEntry` mirroring a platform archive member.
fn zip_entry_from(
    vm: &mut Vm<'_>,
    entry: &oxjvm_platform::zip::ZipEntry,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/zip/ZipEntry")?;
    let instance = vm.new_instance(class)?;
    write_string_field(vm, instance, "name", &entry.name);
    write_long_field(vm, instance, "size", i64::from(entry.uncompressed_size));
    write_long_field(
        vm,
        instance,
        "compressedSize",
        i64::from(entry.compressed_size),
    );
    write_long_field(vm, instance, "crc", i64::from(entry.crc32));
    write_int_field_named(vm, instance, "method", i32::from(entry.compression));
    Ok(instance)
}

/// Read a zip file from the host filesystem and populate a `ZipFile`'s name/entries/bytes.
pub(crate) fn zip_file_init(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    path: &str,
) -> Result<Value, VmError> {
    let bytes = vm
        .host
        .filesystem()
        .and_then(|fs| fs.read(path))
        .ok_or_else(|| {
            vm.throw_new(
                "java/io/FileNotFoundException",
                Some(&alloc::format!("{path} (No such file or directory)")),
            )
        })?;
    let archive = oxjvm_platform::zip::ZipArchive::new(&bytes).map_err(|_| {
        vm.throw_new(
            "java/util/zip/ZipException",
            Some("zip file is empty or malformed"),
        )
    })?;
    let mut entries = Vec::with_capacity(archive.entries().len());
    for entry in archive.entries() {
        entries.push(zip_entry_from(vm, entry)?);
    }
    let list = crate::util::array_list_from(vm, &entries)?;
    let class = vm.class_of(this);
    let name = vm.make_string(path)?;
    let (declaring, field) = vm.find_field(class, "name", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, name);
    let (declaring, field) = vm.find_field(class, "entries", "Ljava/util/List;")?;
    vm.set_instance_ref(this, declaring, field, list);
    let raw = zip_byte_array(vm, &bytes)?;
    let (declaring, field) = vm.find_field(class, "bytes", "[B")?;
    vm.set_instance_ref(this, declaring, field, raw);
    let comment = if archive.comment().is_empty() {
        ObjectRef::NULL
    } else {
        vm.make_string(archive.comment())?
    };
    let (declaring, field) = vm.find_field(class, "comment", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, comment);
    void()
}

fn zip_file_entries(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "entries", "Ljava/util/List;")
        .unwrap_or(ObjectRef::NULL)
}

fn zip_file_bytes(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Vec<u8>, VmError> {
    let raw = vm
        .read_ref_field(this, vm.class_of(this), "bytes", "[B")
        .unwrap_or(ObjectRef::NULL);
    crate::lang::array_bytes(vm, raw)
}

/// The uncompressed bytes of a named member, re-parsed from the stored archive bytes.
pub(crate) fn zip_file_read(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: &str,
) -> Result<Vec<u8>, VmError> {
    let bytes = zip_file_bytes(vm, this)?;
    let archive = oxjvm_platform::zip::ZipArchive::new(&bytes)
        .map_err(|_| vm.throw_new("java/util/zip/ZipException", Some("malformed zip")))?;
    archive
        .read(name)
        .map_err(|_| vm.throw_new("java/util/zip/ZipException", Some("invalid entry")))?
        .ok_or_else(|| vm.throw_new("java/util/zip/ZipException", Some("no such entry")))
}

pub(crate) fn zip_file_input_stream(vm: &mut Vm<'_>, data: &[u8]) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/io/ByteArrayInputStream")?;
    let instance = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(class, "<init>", "([B)V")
        .ok_or_else(|| VmError::internal("ByteArrayInputStream(byte[]) missing"))?;
    let array = zip_byte_array(vm, data)?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![Value::Ref(instance), Value::Ref(array)],
    )?;
    object(instance)
}

const ZIP_FILE_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
    method("<init>", "(Ljava/io/File;)V", ACC_PUBLIC, |vm, _, args| {
        let path = crate::file::file_path(vm, ref_arg(args, 1));
        zip_file_init(vm, receiver(args), &path)
    }),
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = string_arg(vm, args[1])?;
            zip_file_init(vm, receiver(args), &path)
        },
    ),
    method("<init>", "(Ljava/io/File;I)V", ACC_PUBLIC, |vm, _, args| {
        let path = crate::file::file_path(vm, ref_arg(args, 1));
        zip_file_init(vm, receiver(args), &path)
    }),
    method(
        "<init>",
        "(Ljava/lang/String;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = string_arg(vm, args[1])?;
            zip_file_init(vm, receiver(args), &path)
        },
    ),
    method(
        "getEntry",
        "(Ljava/lang/String;)Ljava/util/zip/ZipEntry;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name = vm.string_or_empty(ref_arg(args, 1));
            let list = zip_file_entries(vm, this);
            for entry in crate::util::array_list_elements(vm, list) {
                if read_string_field(vm, entry, "name").as_deref() == Some(name.as_str()) {
                    return object(entry);
                }
            }
            object(ObjectRef::NULL)
        },
    ),
    method(
        "getInputStream",
        "(Ljava/util/zip/ZipEntry;)Ljava/io/InputStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let entry = ref_arg(args, 1);
            if entry.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let name = read_string_field(vm, entry, "name").unwrap_or_default();
            let data = zip_file_read(vm, this, &name)?;
            zip_file_input_stream(vm, &data)
        },
    ),
    method(
        "entries",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let list = zip_file_entries(vm, this);
            let elements = crate::util::array_list_elements(vm, list);
            object(crate::util::new_enumeration(vm, &elements)?)
        },
    ),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        let list = zip_file_entries(vm, receiver(args));
        int(crate::util::array_list_elements(vm, list).len() as i32)
    }),
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            object(
                vm.read_ref_field(this, vm.class_of(this), "name", "Ljava/lang/String;")
                    .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "getComment",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(
                vm.read_ref_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "comment",
                    "Ljava/lang/String;",
                )
                .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
];

/// `java.util.zip.ZipFile`: a random-access zip archive read from the host filesystem.
pub(crate) const ZIP_FILE: oxjvm_vm::NativeClass = class(
    "java/util/zip/ZipFile",
    Some("java/lang/Object"),
    &["java/io/Closeable"],
    ACC_PUBLIC,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("entries", "Ljava/util/List;", ACC_PRIVATE, None),
        field("bytes", "[B", ACC_PRIVATE, None),
        field("comment", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &ZIP_FILE_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Deflater / Inflater: DEFLATE compression built on the platform's raw inflate.
//
// The compressor emits valid stored (uncompressed) DEFLATE blocks, which any
// inflater accepts; the decompressor uses the dependency-free platform inflate.
// Both buffer their input and produce output eagerly, which covers the
// setInput/deflate/inflate round-trips the tests exercise.
// -------------------------------------------------------------------------------------------

fn read_buf_field(vm: &mut Vm<'_>, this: ObjectRef, name: &str) -> Vec<u8> {
    match read_ref_field(vm, this, name, "[B") {
        Some(array) if !array.is_null() => crate::lang::array_bytes(vm, array).unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn write_buf_field(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: &str,
    bytes: &[u8],
) -> Result<(), VmError> {
    let array = crate::lang::byte_array_from(vm, bytes)?;
    write_ref_field(vm, this, name, "[B", array);
    Ok(())
}

fn append_input(vm: &mut Vm<'_>, this: ObjectRef, bytes: &[u8]) -> Result<(), VmError> {
    let mut buf = read_buf_field(vm, this, "buf");
    buf.extend_from_slice(bytes);
    let total = read_long_field(vm, this, "totalIn") + bytes.len() as i64;
    write_long_field(vm, this, "totalIn", total);
    write_buf_field(vm, this, "buf", &buf)
}

/// The JDK's zip bounds check: overflow-safe, and it throws
/// `ArrayIndexOutOfBoundsException` rather than the generic index exception.
fn check_byte_range(vm: &mut Vm<'_>, len: usize, off: i32, count: i32) -> Result<(), VmError> {
    if off < 0 || count < 0 || i64::from(off) + i64::from(count) > len as i64 {
        return Err(vm.throw_new("java/lang/ArrayIndexOutOfBoundsException", None));
    }
    Ok(())
}

/// Consume a `ByteBuffer`'s remaining bytes for a checksum update, advancing
/// position to limit as the JDK's `Checksum.update(ByteBuffer)` default does.
fn checksum_buffer_bytes(vm: &mut Vm<'_>, buffer: ObjectRef) -> Result<Vec<u8>, VmError> {
    if buffer.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let bytes = crate::buffer::byte_buffer_remaining_bytes(vm, buffer)?;
    let class = vm.class_of(buffer);
    if let Some((declaring, field)) = vm.find_field_opt(class, "limit", "I") {
        let limit = vm.read_instance_int(buffer, declaring, field).unwrap_or(0);
        if let Some((position_class, position_field)) = vm.find_field_opt(class, "position", "I") {
            vm.set_instance_int(buffer, position_class, position_field, limit);
        }
    }
    Ok(bytes)
}

fn copy_pending(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    out: ObjectRef,
    off: i32,
    len: i32,
) -> Result<i32, VmError> {
    let pending = read_buf_field(vm, this, "pending");
    let pos = read_int_field(vm, this, "pendingPos").max(0) as usize;
    if pos >= pending.len() {
        return Ok(0);
    }
    let n = (pending.len() - pos).min(len.max(0) as usize);
    for i in 0..n {
        vm.array_set(
            out,
            off + i as i32,
            Value::Int(i32::from(pending[pos + i] as i8)),
            false,
        )?;
    }
    write_int_field_named(vm, this, "pendingPos", (pos + n) as i32);
    let total = read_long_field(vm, this, "totalOut") + n as i64;
    write_long_field(vm, this, "totalOut", total);
    Ok(n as i32)
}

fn pending_exhausted(vm: &mut Vm<'_>, this: ObjectRef) -> bool {
    let pending = read_buf_field(vm, this, "pending");
    read_int_field(vm, this, "pendingPos").max(0) as usize >= pending.len()
}

/// Encode `data` as stored (BTYPE=00) DEFLATE blocks (RFC 1951). When
/// `final_block` is set the last block carries BFINAL=1; an empty `data` with
/// `final_block` emits the terminating empty stored block.
fn stored_blocks(data: &[u8], final_block: bool) -> Vec<u8> {
    let mut out = Vec::new();
    if data.is_empty() {
        if final_block {
            out.push(0x01);
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0xFFFFu16.to_le_bytes());
        }
        return out;
    }
    let mut chunks = data.chunks(65535).peekable();
    while let Some(chunk) = chunks.next() {
        let is_last = final_block && chunks.peek().is_none();
        out.push(if is_last { 0x01 } else { 0x00 });
        let len = chunk.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(chunk);
    }
    out
}

/// Continue an Adler-32 (RFC 1950) running checksum `prev` over `data`.
fn adler32_step(prev: u32, data: &[u8]) -> u32 {
    let mut a = prev & 0xFFFF;
    let mut b = (prev >> 16) & 0xFFFF;
    for byte in data {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn inflater_do_inflate(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    out: ObjectRef,
    off: i32,
    len: i32,
) -> Result<Value, VmError> {
    check_byte_range(vm, vm.array_length(out)?, off, len)?;
    let n = copy_pending(vm, this, out, off, len)?;
    if n > 0 {
        return int(n);
    }
    if read_int_field(vm, this, "finished") != 0 {
        return int(0);
    }
    let buf = read_buf_field(vm, this, "buf");
    let nowrap = read_int_field(vm, this, "nowrap") != 0;
    let raw: &[u8] = if nowrap || buf.len() < 2 {
        &buf
    } else {
        &buf[2..]
    };
    match oxjvm_platform::inflate::inflate(raw, 1 << 28) {
        Ok(decoded) => {
            write_buf_field(vm, this, "pending", &decoded)?;
            write_int_field_named(vm, this, "pendingPos", 0);
            write_int_field_named(vm, this, "finished", 1);
            write_int_field_named(vm, this, "needInput", 0);
            let n = copy_pending(vm, this, out, off, len)?;
            int(n)
        }
        Err(oxjvm_platform::inflate::InflateError::Truncated) => {
            write_int_field_named(vm, this, "needInput", 1);
            int(0)
        }
        Err(_) => Err(vm.throw_new(
            "java/util/zip/DataFormatException",
            Some("invalid deflate stream"),
        )),
    }
}

const INFLATER_METHODS: [oxjvm_vm::NativeMethodDef; 16] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        write_int_field_named(vm, receiver(args), "nowrap", 0);
        void()
    }),
    method("<init>", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        write_int_field_named(vm, receiver(args), "nowrap", crate::int_arg(args, 1));
        void()
    }),
    method("setInput", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
        append_input(vm, receiver(args), &bytes)?;
        write_int_field_named(vm, receiver(args), "needInput", 0);
        void()
    }),
    method("setInput", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
        let off = crate::int_arg(args, 2);
        let len = crate::int_arg(args, 3);
        if off < 0 || len < 0 || (off + len) as usize > bytes.len() {
            return Err(vm.throw_new("java/lang/ArrayIndexOutOfBoundsException", None));
        }
        append_input(
            vm,
            receiver(args),
            &bytes[off as usize..(off + len) as usize],
        )?;
        write_int_field_named(vm, receiver(args), "needInput", 0);
        void()
    }),
    method("inflate", "([B)I", ACC_PUBLIC, |vm, _, args| {
        let out = ref_arg(args, 1);
        let len = vm.array_length(out)? as i32;
        inflater_do_inflate(vm, receiver(args), out, 0, len)
    }),
    method("inflate", "([BII)I", ACC_PUBLIC, |vm, _, args| {
        inflater_do_inflate(
            vm,
            receiver(args),
            ref_arg(args, 1),
            crate::int_arg(args, 2),
            crate::int_arg(args, 3),
        )
    }),
    method("needsInput", "()Z", ACC_PUBLIC, |vm, _, args| {
        crate::boolean(
            read_int_field(vm, receiver(args), "needInput") != 0
                && read_int_field(vm, receiver(args), "finished") == 0,
        )
    }),
    method("needsDictionary", "()Z", ACC_PUBLIC, |_, _, _| {
        crate::boolean(false)
    }),
    method("finished", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        crate::boolean(read_int_field(vm, this, "finished") != 0 && pending_exhausted(vm, this))
    }),
    method("getRemaining", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if read_int_field(vm, this, "finished") != 0 {
            return int(0);
        }
        int(read_buf_field(vm, this, "buf").len() as i32)
    }),
    method("getTotalIn", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_long_field(vm, receiver(args), "totalIn") as i32)
    }),
    method("getTotalOut", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_long_field(vm, receiver(args), "totalOut") as i32)
    }),
    method("getBytesRead", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "totalIn"))
    }),
    method("getBytesWritten", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "totalOut"))
    }),
    method("reset", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        write_buf_field(vm, this, "buf", &[])?;
        write_buf_field(vm, this, "pending", &[])?;
        write_int_field_named(vm, this, "pendingPos", 0);
        write_int_field_named(vm, this, "finished", 0);
        write_int_field_named(vm, this, "needInput", 0);
        write_long_field(vm, this, "totalIn", 0);
        write_long_field(vm, this, "totalOut", 0);
        void()
    }),
    method("end", "()V", ACC_PUBLIC, |_, _, _| void()),
];

pub(crate) const INFLATER: oxjvm_vm::NativeClass = class(
    "java/util/zip/Inflater",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("buf", "[B", ACC_PRIVATE, None),
        field("pending", "[B", ACC_PRIVATE, None),
        field("pendingPos", "I", ACC_PRIVATE, None),
        field("nowrap", "I", ACC_PRIVATE, None),
        field("finished", "I", ACC_PRIVATE, None),
        field("needInput", "I", ACC_PRIVATE, None),
        field("totalIn", "J", ACC_PRIVATE, None),
        field("totalOut", "J", ACC_PRIVATE, None),
    ],
    &INFLATER_METHODS,
    None,
);

fn deflater_do_deflate(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    out: ObjectRef,
    off: i32,
    len: i32,
) -> Result<Value, VmError> {
    check_byte_range(vm, vm.array_length(out)?, off, len)?;
    let n = copy_pending(vm, this, out, off, len)?;
    if n > 0 {
        return int(n);
    }
    if read_int_field(vm, this, "finished") != 0 {
        return int(0);
    }
    let nowrap = read_int_field(vm, this, "nowrap") != 0;
    let mut pending = Vec::new();
    // Emit the zlib header (RFC 1950) once, before the first block.
    if !nowrap && read_int_field(vm, this, "headerSent") == 0 {
        pending.extend_from_slice(&[0x78, 0x9C]);
        write_int_field_named(vm, this, "headerSent", 1);
    }
    let buf = read_buf_field(vm, this, "buf");
    let finishing = read_int_field(vm, this, "finish") != 0;
    if !buf.is_empty() {
        // Stream the buffered input as stored blocks; when finishing, mark the
        // last block final and append the trailer so one call can complete.
        pending.extend_from_slice(&stored_blocks(&buf, finishing));
        let adler = adler32_step(read_long_field(vm, this, "adler") as u32, &buf);
        write_long_field(vm, this, "adler", i64::from(adler));
        write_buf_field(vm, this, "buf", &[])?;
        if finishing {
            if !nowrap {
                pending.extend_from_slice(&adler.to_be_bytes());
            }
            write_int_field_named(vm, this, "finished", 1);
        }
    } else if finishing {
        // Terminate with a final empty stored block, then the Adler-32 trailer.
        pending.extend_from_slice(&stored_blocks(&[], true));
        if !nowrap {
            let adler = read_long_field(vm, this, "adler") as u32;
            pending.extend_from_slice(&adler.to_be_bytes());
        }
        write_int_field_named(vm, this, "finished", 1);
    }
    if pending.is_empty() {
        return int(0);
    }
    write_buf_field(vm, this, "pending", &pending)?;
    write_int_field_named(vm, this, "pendingPos", 0);
    let n = copy_pending(vm, this, out, off, len)?;
    int(n)
}

const DEFLATER_METHODS: [oxjvm_vm::NativeMethodDef; 20] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        write_int_field_named(vm, this, "level", -1);
        write_long_field(vm, this, "adler", 1);
        void()
    }),
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        write_int_field_named(vm, this, "level", crate::int_arg(args, 1));
        write_long_field(vm, this, "adler", 1);
        void()
    }),
    method("<init>", "(IZ)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        write_int_field_named(vm, this, "level", crate::int_arg(args, 1));
        write_int_field_named(vm, this, "nowrap", crate::int_arg(args, 2));
        write_long_field(vm, this, "adler", 1);
        void()
    }),
    method("setInput", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
        append_input(vm, receiver(args), &bytes)?;
        void()
    }),
    method("setInput", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
        let off = crate::int_arg(args, 2);
        let len = crate::int_arg(args, 3);
        if off < 0 || len < 0 || (off + len) as usize > bytes.len() {
            return Err(vm.throw_new("java/lang/ArrayIndexOutOfBoundsException", None));
        }
        append_input(
            vm,
            receiver(args),
            &bytes[off as usize..(off + len) as usize],
        )?;
        void()
    }),
    method("deflate", "([B)I", ACC_PUBLIC, |vm, _, args| {
        let out = ref_arg(args, 1);
        let len = vm.array_length(out)? as i32;
        deflater_do_deflate(vm, receiver(args), out, 0, len)
    }),
    method("deflate", "([BII)I", ACC_PUBLIC, |vm, _, args| {
        deflater_do_deflate(
            vm,
            receiver(args),
            ref_arg(args, 1),
            crate::int_arg(args, 2),
            crate::int_arg(args, 3),
        )
    }),
    method("finish", "()V", ACC_PUBLIC, |vm, _, args| {
        write_int_field_named(vm, receiver(args), "finish", 1);
        void()
    }),
    method("setDictionary", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
        write_buf_field(vm, receiver(args), "dict", &bytes)?;
        void()
    }),
    method("setDictionary", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
        let off = crate::int_arg(args, 2);
        let len = crate::int_arg(args, 3);
        check_byte_range(vm, bytes.len(), off, len)?;
        write_buf_field(
            vm,
            receiver(args),
            "dict",
            &bytes[off as usize..(off + len) as usize],
        )?;
        void()
    }),
    method("finished", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        crate::boolean(read_int_field(vm, this, "finished") != 0 && pending_exhausted(vm, this))
    }),
    method("needsInput", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        crate::boolean(
            read_int_field(vm, this, "finish") == 0 && read_buf_field(vm, this, "buf").is_empty(),
        )
    }),
    method("getTotalIn", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_long_field(vm, receiver(args), "totalIn") as i32)
    }),
    method("getTotalOut", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_long_field(vm, receiver(args), "totalOut") as i32)
    }),
    method("getBytesRead", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "totalIn"))
    }),
    method("getBytesWritten", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "totalOut"))
    }),
    method("setLevel", "(I)V", ACC_PUBLIC, |vm, _, args| {
        write_int_field_named(vm, receiver(args), "level", crate::int_arg(args, 1));
        void()
    }),
    method("setStrategy", "(I)V", ACC_PUBLIC, |_, _, _| void()),
    method("reset", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        write_buf_field(vm, this, "buf", &[])?;
        write_buf_field(vm, this, "pending", &[])?;
        write_int_field_named(vm, this, "pendingPos", 0);
        write_int_field_named(vm, this, "finish", 0);
        write_int_field_named(vm, this, "finished", 0);
        write_int_field_named(vm, this, "headerSent", 0);
        write_long_field(vm, this, "adler", 1);
        write_long_field(vm, this, "totalIn", 0);
        write_long_field(vm, this, "totalOut", 0);
        void()
    }),
    method("end", "()V", ACC_PUBLIC, |_, _, _| void()),
];

pub(crate) const DEFLATER: oxjvm_vm::NativeClass = class(
    "java/util/zip/Deflater",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("buf", "[B", ACC_PRIVATE, None),
        field("pending", "[B", ACC_PRIVATE, None),
        field("pendingPos", "I", ACC_PRIVATE, None),
        field("nowrap", "I", ACC_PRIVATE, None),
        field("finish", "I", ACC_PRIVATE, None),
        field("finished", "I", ACC_PRIVATE, None),
        field("level", "I", ACC_PRIVATE, None),
        field("headerSent", "I", ACC_PRIVATE, None),
        field("adler", "J", ACC_PRIVATE, None),
        field("totalIn", "J", ACC_PRIVATE, None),
        field("totalOut", "J", ACC_PRIVATE, None),
        field("dict", "[B", ACC_PRIVATE, None),
    ],
    &DEFLATER_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// GZIP / Deflater stream wrappers built on Deflater/Inflater.
//
// These use an eager model: the input side reads the whole underlying stream and
// inflates it once, the output side buffers writes and deflates on finish/close.
// This covers the ByteArray-based round-trip and header tests.
// -------------------------------------------------------------------------------------------

fn filter_in(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    read_ref_field(vm, this, "in", "Ljava/io/InputStream;").unwrap_or(ObjectRef::NULL)
}

fn filter_out(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    read_ref_field(vm, this, "out", "Ljava/io/OutputStream;").unwrap_or(ObjectRef::NULL)
}

fn read_in_array(
    vm: &mut Vm<'_>,
    input: ObjectRef,
    arr: ObjectRef,
    off: i32,
    len: i32,
) -> Result<i32, VmError> {
    let (class, method) = vm.resolve_virtual_method(input, "read", "([BII)I")?;
    Ok(vm
        .invoke_method(
            class,
            method,
            vec![
                Value::Ref(input),
                Value::Ref(arr),
                Value::Int(off),
                Value::Int(len),
            ],
        )?
        .as_int())
}

fn write_out_bytes(vm: &mut Vm<'_>, out: ObjectRef, bytes: &[u8]) -> Result<(), VmError> {
    let arr = crate::lang::byte_array_from(vm, bytes)?;
    let (class, method) = vm.resolve_virtual_method(out, "write", "([B)V")?;
    vm.invoke_method(class, method, vec![Value::Ref(out), Value::Ref(arr)])?;
    Ok(())
}

fn read_all_input(vm: &mut Vm<'_>, input: ObjectRef) -> Result<Vec<u8>, VmError> {
    let mut out = Vec::new();
    let chunk = vm.allocate_array_of(ArrayComponent::Byte, 4096)?;
    loop {
        let n = read_in_array(vm, input, chunk, 0, 4096)?;
        if n <= 0 {
            break;
        }
        for i in 0..n {
            out.push(vm.array_get(chunk, i)?.as_int() as u8);
        }
    }
    Ok(out)
}

fn set_native_input(vm: &mut Vm<'_>, target: ObjectRef, data: &[u8]) -> Result<(), VmError> {
    let arr = crate::lang::byte_array_from(vm, data)?;
    let (class, method) = vm.resolve_virtual_method(target, "setInput", "([B)V")?;
    vm.invoke_method(class, method, vec![Value::Ref(target), Value::Ref(arr)])?;
    Ok(())
}

fn le32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// Return the index where DEFLATE data begins in a GZIP member (RFC 1952).
fn gzip_header_end(comp: &[u8]) -> Result<usize, VmError> {
    if comp.len() < 10 || comp[0] != 0x1f || comp[1] != 0x8b || comp[2] != 8 {
        return Err(VmError::internal("bad gzip header"));
    }
    let flg = comp[3];
    let mut pos = 10usize;
    if flg & 0x04 != 0 {
        if pos + 2 > comp.len() {
            return Err(VmError::internal("truncated gzip extra"));
        }
        let xlen = u16::from_le_bytes([comp[pos], comp[pos + 1]]) as usize;
        pos += 2 + xlen;
    }
    if flg & 0x08 != 0 {
        while pos < comp.len() && comp[pos] != 0 {
            pos += 1;
        }
        pos += 1;
    }
    if flg & 0x10 != 0 {
        while pos < comp.len() && comp[pos] != 0 {
            pos += 1;
        }
        pos += 1;
    }
    if flg & 0x02 != 0 {
        pos += 2;
    }
    if pos > comp.len() {
        return Err(VmError::internal("truncated gzip header"));
    }
    Ok(pos)
}

/// Read one unsigned byte from `input` via its single-byte `read()`, validating
/// the result range exactly as `GZIPInputStream.readUByte` does: EOF raises
/// `EOFException`, and a value outside `-1..255` raises `IOException`.
fn read_ubyte(vm: &mut Vm<'_>, input: ObjectRef) -> Result<u8, VmError> {
    let (class, method) = vm.resolve_virtual_method(input, "read", "()I")?;
    let byte = vm
        .invoke_method(class, method, vec![Value::Ref(input)])?
        .as_int();
    if byte == -1 {
        return Err(vm.throw_new("java/io/EOFException", None));
    }
    if !(-1..=255).contains(&byte) {
        let name = vm.class_name(vm.class_of(input)).replace('/', ".");
        let message = alloc::format!("{name}.read() returned value out of range -1..255: {byte}");
        return Err(vm.throw_new("java/io/IOException", Some(&message)));
    }
    Ok(byte as u8)
}

/// Read a GZIP member header eagerly (as the JDK constructor does), returning
/// the header bytes so the decompression path can prepend them to the rest of
/// the stream.
fn read_gzip_header_eager(vm: &mut Vm<'_>, input: ObjectRef) -> Result<Vec<u8>, VmError> {
    let mut hdr = Vec::new();
    for _ in 0..10 {
        hdr.push(read_ubyte(vm, input)?);
    }
    if hdr[0] != 0x1f || hdr[1] != 0x8b || hdr[2] != 8 {
        return Err(vm.throw_new("java/util/zip/ZipException", Some("Not in GZIP format")));
    }
    let flg = hdr[3];
    if flg & 0x04 != 0 {
        let lo = read_ubyte(vm, input)?;
        let hi = read_ubyte(vm, input)?;
        hdr.push(lo);
        hdr.push(hi);
        let xlen = u16::from_le_bytes([lo, hi]) as usize;
        for _ in 0..xlen {
            hdr.push(read_ubyte(vm, input)?);
        }
    }
    for mask in [0x08, 0x10] {
        if flg & mask != 0 {
            loop {
                let byte = read_ubyte(vm, input)?;
                hdr.push(byte);
                if byte == 0 {
                    break;
                }
            }
        }
    }
    if flg & 0x02 != 0 {
        hdr.push(read_ubyte(vm, input)?);
        hdr.push(read_ubyte(vm, input)?);
    }
    Ok(hdr)
}

fn ensure_decompressed(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    if read_int_field(vm, this, "filled") != 0 {
        return Ok(());
    }
    let input = filter_in(vm, this);
    let mut comp = read_all_input(vm, input)?;
    let inf =
        read_ref_field(vm, this, "inf", "Ljava/util/zip/Inflater;").unwrap_or(ObjectRef::NULL);
    let gzip = read_int_field(vm, this, "gzip") != 0;
    if gzip {
        let header = read_buf_field(vm, this, "gzhdr");
        if !header.is_empty() {
            let mut full = header;
            full.extend_from_slice(&comp);
            comp = full;
        }
    }
    let data: Vec<u8>;
    let mut trailer_crc = 0u32;
    let mut trailer_size = 0u32;
    if gzip {
        let hdr = gzip_header_end(&comp)
            .map_err(|_| vm.throw_new("java/util/zip/ZipException", Some("Not in GZIP format")))?;
        if comp.len() < hdr + 8 {
            return Err(vm.throw_new("java/util/zip/ZipException", Some("Corrupt GZIP trailer")));
        }
        let end = comp.len() - 8;
        data = comp[hdr..end].to_vec();
        trailer_crc = le32(&comp[end..]);
        trailer_size = le32(&comp[end + 4..]);
    } else {
        data = comp;
    }
    set_native_input(vm, inf, &data)?;
    let mut dec = Vec::new();
    let chunk = vm.allocate_array_of(ArrayComponent::Byte, 4096)?;
    loop {
        let n = inflater_do_inflate(vm, inf, chunk, 0, 4096)?.as_int();
        if n <= 0 {
            break;
        }
        for i in 0..n {
            dec.push(vm.array_get(chunk, i)?.as_int() as u8);
        }
    }
    if gzip && (crc32(&dec) != trailer_crc || dec.len() as u32 != trailer_size) {
        return Err(vm.throw_new("java/util/zip/ZipException", Some("Corrupt GZIP trailer")));
    }
    write_buf_field(vm, this, "dbuf", &dec)?;
    write_int_field_named(vm, this, "dpos", 0);
    write_int_field_named(vm, this, "filled", 1);
    Ok(())
}

fn serve_dbuf(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    out: ObjectRef,
    off: i32,
    len: i32,
) -> Result<Value, VmError> {
    let dbuf = read_buf_field(vm, this, "dbuf");
    let dpos = read_int_field(vm, this, "dpos").max(0) as usize;
    if dpos >= dbuf.len() {
        return int(-1);
    }
    if len == 0 {
        return int(0);
    }
    let n = (len.max(0) as usize).min(dbuf.len() - dpos);
    for i in 0..n {
        vm.array_set(
            out,
            off + i as i32,
            Value::Int(i32::from(dbuf[dpos + i] as i8)),
            false,
        )?;
    }
    write_int_field_named(vm, this, "dpos", (dpos + n) as i32);
    int(n as i32)
}

fn inflater_stream_read(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    out: ObjectRef,
    off: i32,
    len: i32,
) -> Result<Value, VmError> {
    if read_int_field(vm, this, "closed") != 0 {
        return Err(vm.throw_new("java/io/IOException", Some("Stream closed")));
    }
    ensure_decompressed(vm, this)?;
    serve_dbuf(vm, this, out, off, len)
}

fn inflater_stream_ctor(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    input: ObjectRef,
    inf: ObjectRef,
    gzip: bool,
) -> Result<(), VmError> {
    if input.is_null() || inf.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let (declaring, field) = vm.find_field(vm.class_of(this), "in", "Ljava/io/InputStream;")?;
    vm.set_instance_ref(this, declaring, field, input);
    write_ref_field(vm, this, "inf", "Ljava/util/zip/Inflater;", inf);
    write_int_field_named(vm, this, "gzip", i32::from(gzip));
    Ok(())
}

const INFLATER_INPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method(
        "<init>",
        "(Ljava/io/InputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let inf = new_instance(vm, "java/util/zip/Inflater")?;
            inflater_stream_ctor(vm, this, ref_arg(args, 1), inf, false)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/InputStream;Ljava/util/zip/Inflater;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            inflater_stream_ctor(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                false,
            )?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/InputStream;Ljava/util/zip/Inflater;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            if crate::int_arg(args, 3) <= 0 {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("size <= 0")));
            }
            inflater_stream_ctor(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                false,
            )?;
            void()
        },
    ),
    method("read", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let one = vm.allocate_array_of(ArrayComponent::Byte, 1)?;
        let n = inflater_stream_read(vm, this, one, 0, 1)?.as_int();
        if n <= 0 {
            return int(-1);
        }
        int(vm.array_get(one, 0)?.as_int() & 0xFF)
    }),
    method("read", "([B)I", ACC_PUBLIC, |vm, _, args| {
        let out = ref_arg(args, 1);
        let len = vm.array_length(out)? as i32;
        inflater_stream_read(vm, receiver(args), out, 0, len)
    }),
    method("read", "([BII)I", ACC_PUBLIC, |vm, _, args| {
        inflater_stream_read(
            vm,
            receiver(args),
            ref_arg(args, 1),
            crate::int_arg(args, 2),
            crate::int_arg(args, 3),
        )
    }),
    method("available", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        ensure_decompressed(vm, this)?;
        let dbuf = read_buf_field(vm, this, "dbuf");
        let dpos = read_int_field(vm, this, "dpos").max(0) as usize;
        int(i32::from(dpos < dbuf.len()))
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        write_int_field_named(vm, this, "closed", 1);
        let input = filter_in(vm, this);
        if !input.is_null() {
            let (class, method) = vm.resolve_virtual_method(input, "close", "()V")?;
            vm.invoke_method(class, method, vec![Value::Ref(input)])?;
        }
        void()
    }),
    method("skip", "(J)J", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let mut remaining = crate::long_arg(args, 1);
        if remaining < 0 {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("negative skip length"),
            ));
        }
        let chunk = vm.allocate_array_of(ArrayComponent::Byte, 4096)?;
        let mut skipped = 0i64;
        while remaining > 0 {
            let want = remaining.min(4096) as i32;
            let n = inflater_stream_read(vm, this, chunk, 0, want)?.as_int();
            if n <= 0 {
                break;
            }
            skipped += i64::from(n);
            remaining -= i64::from(n);
        }
        long(skipped)
    }),
    method("markSupported", "()Z", ACC_PUBLIC, |_, _, _| {
        crate::boolean(false)
    }),
    method("mark", "(I)V", ACC_PUBLIC, |_, _, _| void()),
    method("reset", "()V", ACC_PUBLIC, |vm, _, _| {
        Err(vm.throw_new("java/io/IOException", Some("mark not supported")))
    }),
];

pub(crate) const INFLATER_INPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/util/zip/InflaterInputStream",
    Some("java/io/FilterInputStream"),
    &[],
    ACC_PUBLIC,
    &[
        field("inf", "Ljava/util/zip/Inflater;", ACC_PROTECTED, None),
        field("dbuf", "[B", ACC_PRIVATE, None),
        field("dpos", "I", ACC_PRIVATE, None),
        field("filled", "I", ACC_PRIVATE, None),
        field("gzip", "I", ACC_PRIVATE, None),
        field("closed", "I", ACC_PRIVATE, None),
    ],
    &INFLATER_INPUT_STREAM_METHODS,
    None,
);

const GZIP_INPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "<init>",
        "(Ljava/io/InputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let inf = new_instance(vm, "java/util/zip/Inflater")?;
            let (class, method) = vm.resolve_virtual_method(inf, "<init>", "(Z)V")?;
            vm.invoke_method(class, method, vec![Value::Ref(inf), Value::Int(1)])?;
            inflater_stream_ctor(vm, this, ref_arg(args, 1), inf, true)?;
            let header = read_gzip_header_eager(vm, ref_arg(args, 1))?;
            write_buf_field(vm, this, "gzhdr", &header)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/InputStream;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if crate::int_arg(args, 2) <= 0 {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("size <= 0")));
            }
            let inf = new_instance(vm, "java/util/zip/Inflater")?;
            let (class, method) = vm.resolve_virtual_method(inf, "<init>", "(Z)V")?;
            vm.invoke_method(class, method, vec![Value::Ref(inf), Value::Int(1)])?;
            inflater_stream_ctor(vm, this, ref_arg(args, 1), inf, true)?;
            let header = read_gzip_header_eager(vm, ref_arg(args, 1))?;
            write_buf_field(vm, this, "gzhdr", &header)?;
            void()
        },
    ),
];

pub(crate) const GZIP_INPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/util/zip/GZIPInputStream",
    Some("java/util/zip/InflaterInputStream"),
    &[],
    ACC_PUBLIC,
    &[field("gzhdr", "[B", ACC_PRIVATE, None)],
    &GZIP_INPUT_STREAM_METHODS,
    None,
);

const ZIP_INPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "<init>",
        "(Ljava/io/InputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let inf = new_instance(vm, "java/util/zip/Inflater")?;
            let (class, method) = vm.resolve_virtual_method(inf, "<init>", "(Z)V")?;
            vm.invoke_method(class, method, vec![Value::Ref(inf), Value::Int(1)])?;
            inflater_stream_ctor(vm, this, ref_arg(args, 1), inf, false)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/InputStream;Ljava/nio/charset/Charset;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let inf = new_instance(vm, "java/util/zip/Inflater")?;
            let (class, method) = vm.resolve_virtual_method(inf, "<init>", "(Z)V")?;
            vm.invoke_method(class, method, vec![Value::Ref(inf), Value::Int(1)])?;
            inflater_stream_ctor(vm, this, ref_arg(args, 1), inf, false)?;
            void()
        },
    ),
];

pub(crate) const ZIP_INPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/util/zip/ZipInputStream",
    Some("java/util/zip/InflaterInputStream"),
    &[],
    ACC_PUBLIC,
    &[],
    &ZIP_INPUT_STREAM_METHODS,
    None,
);

fn append_ibuf(vm: &mut Vm<'_>, this: ObjectRef, bytes: &[u8]) -> Result<(), VmError> {
    let mut buf = read_buf_field(vm, this, "ibuf");
    buf.extend_from_slice(bytes);
    write_buf_field(vm, this, "ibuf", &buf)
}

fn deflater_output_finish(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    if read_int_field(vm, this, "done") != 0 {
        return Ok(());
    }
    let def =
        read_ref_field(vm, this, "def", "Ljava/util/zip/Deflater;").unwrap_or(ObjectRef::NULL);
    let ibuf = read_buf_field(vm, this, "ibuf");
    let out = filter_out(vm, this);
    let gzip = read_int_field(vm, this, "gzip") != 0;
    set_native_input(vm, def, &ibuf)?;
    let (class, method) = vm.resolve_virtual_method(def, "finish", "()V")?;
    vm.invoke_method(class, method, vec![Value::Ref(def)])?;
    let chunk = vm.allocate_array_of(ArrayComponent::Byte, 4096)?;
    loop {
        let n = deflater_do_deflate(vm, def, chunk, 0, 4096)?.as_int();
        if n <= 0 {
            break;
        }
        let mut slice = Vec::with_capacity(n as usize);
        for i in 0..n {
            slice.push(vm.array_get(chunk, i)?.as_int() as u8);
        }
        write_out_bytes(vm, out, &slice)?;
    }
    if gzip {
        let mut trailer = crc32(&ibuf).to_le_bytes().to_vec();
        trailer.extend_from_slice(&(ibuf.len() as u32).to_le_bytes());
        write_out_bytes(vm, out, &trailer)?;
    }
    write_int_field_named(vm, this, "done", 1);
    Ok(())
}

fn deflater_output_ctor(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    out: ObjectRef,
    def: ObjectRef,
    gzip: bool,
) -> Result<(), VmError> {
    if out.is_null() || def.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let (declaring, field) = vm.find_field(vm.class_of(this), "out", "Ljava/io/OutputStream;")?;
    vm.set_instance_ref(this, declaring, field, out);
    write_ref_field(vm, this, "def", "Ljava/util/zip/Deflater;", def);
    write_int_field_named(vm, this, "gzip", i32::from(gzip));
    if gzip {
        write_out_bytes(vm, out, &[0x1f, 0x8b, 0x08, 0x00, 0, 0, 0, 0, 0x00, 0xff])?;
    }
    Ok(())
}

const DEFLATER_OUTPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method(
        "<init>",
        "(Ljava/io/OutputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let def = new_instance(vm, "java/util/zip/Deflater")?;
            deflater_output_ctor(vm, this, ref_arg(args, 1), def, false)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/OutputStream;Z)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let def = new_instance(vm, "java/util/zip/Deflater")?;
            deflater_output_ctor(vm, this, ref_arg(args, 1), def, false)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/OutputStream;Ljava/util/zip/Deflater;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            deflater_output_ctor(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                false,
            )?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/OutputStream;Ljava/util/zip/Deflater;Z)V",
        ACC_PUBLIC,
        |vm, _, args| {
            deflater_output_ctor(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                false,
            )?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/OutputStream;Ljava/util/zip/Deflater;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            if crate::int_arg(args, 3) <= 0 {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("size <= 0")));
            }
            deflater_output_ctor(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                false,
            )?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/OutputStream;Ljava/util/zip/Deflater;IZ)V",
        ACC_PUBLIC,
        |vm, _, args| {
            if crate::int_arg(args, 3) <= 0 {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("size <= 0")));
            }
            deflater_output_ctor(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                false,
            )?;
            void()
        },
    ),
    method("write", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let byte = crate::int_arg(args, 1) as u8;
        append_ibuf(vm, receiver(args), &[byte])?;
        void()
    }),
    method("write", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
        append_ibuf(vm, receiver(args), &bytes)?;
        void()
    }),
    method("write", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
        let off = crate::int_arg(args, 2);
        let len = crate::int_arg(args, 3);
        if off < 0 || len < 0 || (off + len) as usize > bytes.len() {
            return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
        }
        append_ibuf(
            vm,
            receiver(args),
            &bytes[off as usize..(off + len) as usize],
        )?;
        void()
    }),
    method("finish", "()V", ACC_PUBLIC, |vm, _, args| {
        deflater_output_finish(vm, receiver(args))?;
        void()
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        deflater_output_finish(vm, this)?;
        let out = filter_out(vm, this);
        if !out.is_null() {
            let (class, method) = vm.resolve_virtual_method(out, "close", "()V")?;
            vm.invoke_method(class, method, vec![Value::Ref(out)])?;
        }
        void()
    }),
    method("flush", "()V", ACC_PUBLIC, |vm, _, args| {
        let out = filter_out(vm, receiver(args));
        if !out.is_null() {
            let (class, method) = vm.resolve_virtual_method(out, "flush", "()V")?;
            vm.invoke_method(class, method, vec![Value::Ref(out)])?;
        }
        void()
    }),
];

pub(crate) const DEFLATER_OUTPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/util/zip/DeflaterOutputStream",
    Some("java/io/FilterOutputStream"),
    &[],
    ACC_PUBLIC,
    &[
        field("def", "Ljava/util/zip/Deflater;", ACC_PROTECTED, None),
        field("ibuf", "[B", ACC_PRIVATE, None),
        field("done", "I", ACC_PRIVATE, None),
        field("gzip", "I", ACC_PRIVATE, None),
    ],
    &DEFLATER_OUTPUT_STREAM_METHODS,
    None,
);

fn gzip_output_ctor(vm: &mut Vm<'_>, this: ObjectRef, out: ObjectRef) -> Result<(), VmError> {
    let def = new_instance(vm, "java/util/zip/Deflater")?;
    let (class, method) = vm.resolve_virtual_method(def, "<init>", "(IZ)V")?;
    vm.invoke_method(
        class,
        method,
        vec![Value::Ref(def), Value::Int(-1), Value::Int(1)],
    )?;
    deflater_output_ctor(vm, this, out, def, true)
}

const GZIP_OUTPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "<init>",
        "(Ljava/io/OutputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            gzip_output_ctor(vm, receiver(args), ref_arg(args, 1))?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/OutputStream;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            if crate::int_arg(args, 2) <= 0 {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("size <= 0")));
            }
            gzip_output_ctor(vm, receiver(args), ref_arg(args, 1))?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/OutputStream;Z)V",
        ACC_PUBLIC,
        |vm, _, args| {
            gzip_output_ctor(vm, receiver(args), ref_arg(args, 1))?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/OutputStream;IZ)V",
        ACC_PUBLIC,
        |vm, _, args| {
            if crate::int_arg(args, 2) <= 0 {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("size <= 0")));
            }
            gzip_output_ctor(vm, receiver(args), ref_arg(args, 1))?;
            void()
        },
    ),
];

pub(crate) const GZIP_OUTPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/util/zip/GZIPOutputStream",
    Some("java/util/zip/DeflaterOutputStream"),
    &[],
    ACC_PUBLIC,
    &[],
    &GZIP_OUTPUT_STREAM_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// DeflaterInputStream / InflaterOutputStream: the remaining zip stream wrappers.
// -------------------------------------------------------------------------------------------

fn deflater_input_ensure_open(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    if read_int_field(vm, this, "closed") != 0 {
        return Err(vm.throw_new("java/io/IOException", Some("Stream closed")));
    }
    Ok(())
}

fn deflater_input_fill(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    if read_int_field(vm, this, "filled") != 0 {
        return Ok(());
    }
    let input = filter_in(vm, this);
    let raw = read_all_input(vm, input)?;
    let def =
        read_ref_field(vm, this, "def", "Ljava/util/zip/Deflater;").unwrap_or(ObjectRef::NULL);
    set_native_input(vm, def, &raw)?;
    let (class, method) = vm.resolve_virtual_method(def, "finish", "()V")?;
    vm.invoke_method(class, method, vec![Value::Ref(def)])?;
    let mut comp = Vec::new();
    let chunk = vm.allocate_array_of(ArrayComponent::Byte, 4096)?;
    loop {
        let n = deflater_do_deflate(vm, def, chunk, 0, 4096)?.as_int();
        if n <= 0 {
            break;
        }
        for i in 0..n {
            comp.push(vm.array_get(chunk, i)?.as_int() as u8);
        }
    }
    write_buf_field(vm, this, "dbuf", &comp)?;
    write_int_field_named(vm, this, "dpos", 0);
    write_int_field_named(vm, this, "filled", 1);
    Ok(())
}

fn deflater_input_ctor(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    input: ObjectRef,
    def: ObjectRef,
) -> Result<(), VmError> {
    if input.is_null() || def.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let (declaring, field) = vm.find_field(vm.class_of(this), "in", "Ljava/io/InputStream;")?;
    vm.set_instance_ref(this, declaring, field, input);
    write_ref_field(vm, this, "def", "Ljava/util/zip/Deflater;", def);
    Ok(())
}

const DEFLATER_INPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method(
        "<init>",
        "(Ljava/io/InputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let def = new_instance(vm, "java/util/zip/Deflater")?;
            deflater_input_ctor(vm, this, ref_arg(args, 1), def)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/InputStream;Ljava/util/zip/Deflater;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            deflater_input_ctor(vm, receiver(args), ref_arg(args, 1), ref_arg(args, 2))?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/InputStream;Ljava/util/zip/Deflater;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            if crate::int_arg(args, 3) <= 0 {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("size <= 0")));
            }
            deflater_input_ctor(vm, receiver(args), ref_arg(args, 1), ref_arg(args, 2))?;
            void()
        },
    ),
    method("read", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        deflater_input_ensure_open(vm, this)?;
        deflater_input_fill(vm, this)?;
        let one = vm.allocate_array_of(ArrayComponent::Byte, 1)?;
        let n = serve_dbuf(vm, this, one, 0, 1)?.as_int();
        if n <= 0 {
            write_int_field_named(vm, this, "eof", 1);
            return int(-1);
        }
        int(vm.array_get(one, 0)?.as_int() & 0xFF)
    }),
    method("read", "([B)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        deflater_input_ensure_open(vm, this)?;
        let out = ref_arg(args, 1);
        let bytes = crate::lang::array_bytes(vm, out)?;
        if bytes.is_empty() {
            return int(0);
        }
        deflater_input_fill(vm, this)?;
        let n = serve_dbuf(vm, this, out, 0, bytes.len() as i32)?.as_int();
        if n <= 0 {
            write_int_field_named(vm, this, "eof", 1);
            return int(-1);
        }
        int(n)
    }),
    method("read", "([BII)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        deflater_input_ensure_open(vm, this)?;
        let out = ref_arg(args, 1);
        let bytes = crate::lang::array_bytes(vm, out)?;
        let off = crate::int_arg(args, 2);
        let len = crate::int_arg(args, 3);
        if off < 0 || len < 0 || (off + len) as usize > bytes.len() {
            return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
        }
        if len == 0 {
            return int(0);
        }
        deflater_input_fill(vm, this)?;
        let n = serve_dbuf(vm, this, out, off, len)?.as_int();
        if n <= 0 {
            write_int_field_named(vm, this, "eof", 1);
            return int(-1);
        }
        int(n)
    }),
    method("available", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        deflater_input_ensure_open(vm, this)?;
        int(i32::from(read_int_field(vm, this, "eof") == 0))
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if read_int_field(vm, this, "closed") != 0 {
            return void();
        }
        write_int_field_named(vm, this, "closed", 1);
        let input = filter_in(vm, this);
        if !input.is_null() {
            let (class, method) = vm.resolve_virtual_method(input, "close", "()V")?;
            vm.invoke_method(class, method, vec![Value::Ref(input)])?;
        }
        void()
    }),
    method("skip", "(J)J", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        deflater_input_ensure_open(vm, this)?;
        let mut remaining = crate::long_arg(args, 1);
        if remaining < 0 {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("skip value is negative"),
            ));
        }
        deflater_input_fill(vm, this)?;
        let chunk = vm.allocate_array_of(ArrayComponent::Byte, 4096)?;
        let mut skipped = 0i64;
        while remaining > 0 {
            let want = remaining.min(4096) as i32;
            let n = serve_dbuf(vm, this, chunk, 0, want)?.as_int();
            if n <= 0 {
                write_int_field_named(vm, this, "eof", 1);
                break;
            }
            skipped += i64::from(n);
            remaining -= i64::from(n);
        }
        long(skipped)
    }),
    method("markSupported", "()Z", ACC_PUBLIC, |_, _, _| {
        crate::boolean(false)
    }),
    method("mark", "(I)V", ACC_PUBLIC, |_, _, _| void()),
    method("reset", "()V", ACC_PUBLIC, |vm, _, _| {
        Err(vm.throw_new("java/io/IOException", Some("mark not supported")))
    }),
];

pub(crate) const DEFLATER_INPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/util/zip/DeflaterInputStream",
    Some("java/io/FilterInputStream"),
    &[],
    ACC_PUBLIC,
    &[
        field("def", "Ljava/util/zip/Deflater;", ACC_PROTECTED, None),
        field("dbuf", "[B", ACC_PRIVATE, None),
        field("dpos", "I", ACC_PRIVATE, None),
        field("filled", "I", ACC_PRIVATE, None),
        field("closed", "I", ACC_PRIVATE, None),
        field("eof", "I", ACC_PRIVATE, None),
    ],
    &DEFLATER_INPUT_STREAM_METHODS,
    None,
);

fn inflater_output_ensure_open(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    if read_int_field(vm, this, "closed") != 0 {
        return Err(vm.throw_new("java/io/IOException", Some("Stream closed")));
    }
    Ok(())
}

fn inflater_output_finish(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    if read_int_field(vm, this, "done") != 0 {
        return Ok(());
    }
    let inf =
        read_ref_field(vm, this, "inf", "Ljava/util/zip/Inflater;").unwrap_or(ObjectRef::NULL);
    let ibuf = read_buf_field(vm, this, "ibuf");
    let out = filter_out(vm, this);
    set_native_input(vm, inf, &ibuf)?;
    let chunk = vm.allocate_array_of(ArrayComponent::Byte, 4096)?;
    loop {
        let n = inflater_do_inflate(vm, inf, chunk, 0, 4096)?.as_int();
        if n <= 0 {
            break;
        }
        let mut slice = Vec::with_capacity(n as usize);
        for i in 0..n {
            slice.push(vm.array_get(chunk, i)?.as_int() as u8);
        }
        write_out_bytes(vm, out, &slice)?;
    }
    write_int_field_named(vm, this, "done", 1);
    Ok(())
}

fn inflater_output_ctor(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    out: ObjectRef,
    inf: ObjectRef,
) -> Result<(), VmError> {
    if out.is_null() || inf.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let (declaring, field) = vm.find_field(vm.class_of(this), "out", "Ljava/io/OutputStream;")?;
    vm.set_instance_ref(this, declaring, field, out);
    write_ref_field(vm, this, "inf", "Ljava/util/zip/Inflater;", inf);
    Ok(())
}

const INFLATER_OUTPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method(
        "<init>",
        "(Ljava/io/OutputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let inf = new_instance(vm, "java/util/zip/Inflater")?;
            inflater_output_ctor(vm, this, ref_arg(args, 1), inf)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/OutputStream;Ljava/util/zip/Inflater;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            inflater_output_ctor(vm, receiver(args), ref_arg(args, 1), ref_arg(args, 2))?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/OutputStream;Ljava/util/zip/Inflater;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            if crate::int_arg(args, 3) <= 0 {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("size <= 0")));
            }
            inflater_output_ctor(vm, receiver(args), ref_arg(args, 1), ref_arg(args, 2))?;
            void()
        },
    ),
    method("write", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        inflater_output_ensure_open(vm, this)?;
        let byte = crate::int_arg(args, 1) as u8;
        append_ibuf(vm, this, &[byte])?;
        void()
    }),
    method("write", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        inflater_output_ensure_open(vm, this)?;
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
        append_ibuf(vm, this, &bytes)?;
        void()
    }),
    method("write", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        inflater_output_ensure_open(vm, this)?;
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
        let off = crate::int_arg(args, 2);
        let len = crate::int_arg(args, 3);
        if off < 0 || len < 0 || (off + len) as usize > bytes.len() {
            return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
        }
        append_ibuf(vm, this, &bytes[off as usize..(off + len) as usize])?;
        void()
    }),
    method("finish", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        inflater_output_ensure_open(vm, this)?;
        inflater_output_finish(vm, this)?;
        void()
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if read_int_field(vm, this, "closed") != 0 {
            return void();
        }
        inflater_output_finish(vm, this)?;
        let out = filter_out(vm, this);
        if !out.is_null() {
            let (class, method) = vm.resolve_virtual_method(out, "close", "()V")?;
            vm.invoke_method(class, method, vec![Value::Ref(out)])?;
        }
        write_int_field_named(vm, this, "closed", 1);
        void()
    }),
    method("flush", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        inflater_output_ensure_open(vm, this)?;
        let out = filter_out(vm, this);
        if !out.is_null() {
            let (class, method) = vm.resolve_virtual_method(out, "flush", "()V")?;
            vm.invoke_method(class, method, vec![Value::Ref(out)])?;
        }
        void()
    }),
];

pub(crate) const INFLATER_OUTPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/util/zip/InflaterOutputStream",
    Some("java/io/FilterOutputStream"),
    &[],
    ACC_PUBLIC,
    &[
        field("inf", "Ljava/util/zip/Inflater;", ACC_PROTECTED, None),
        field("ibuf", "[B", ACC_PRIVATE, None),
        field("done", "I", ACC_PRIVATE, None),
        field("closed", "I", ACC_PRIVATE, None),
    ],
    &INFLATER_OUTPUT_STREAM_METHODS,
    None,
);
