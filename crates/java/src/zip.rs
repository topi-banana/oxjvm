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

const ZIP_ENTRY_METHODS: [oxjvm_vm::NativeMethodDef; 21] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = string_arg(vm, args[1])?;
            let this = receiver(args);
            write_string_field(vm, this, "name", &name);
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
    method("setTime", "(J)V", ACC_PUBLIC, |vm, _, args| {
        write_long_field(vm, receiver(args), "time", crate::long_arg(args, 1));
        void()
    }),
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

const ZIP_OUTPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
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
        "putNextEntry",
        "(Ljava/util/zip/ZipEntry;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let entry = ref_arg(args, 1);
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
        zip_finish(vm, receiver(args))
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        zip_finish(vm, this)?;
        if let Some(target) = read_ref_field(vm, this, "target", "Ljava/io/OutputStream;") {
            if !target.is_null() {
                let (class, close) = vm.resolve_virtual_method(target, "close", "()V")?;
                vm.invoke_method(class, close, vec![Value::Ref(target)])?;
            }
        }
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
    let mut end = Vec::new();
    end.extend_from_slice(&0x0605_4B50u32.to_le_bytes());
    end.extend_from_slice(&0u16.to_le_bytes());
    end.extend_from_slice(&0u16.to_le_bytes());
    end.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    end.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    end.extend_from_slice(&(central.len() as u32).to_le_bytes());
    end.extend_from_slice(&(cd_offset as u32).to_le_bytes());
    end.extend_from_slice(&0u16.to_le_bytes());
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

const CRC32_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
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
        if offset < 0 || count < 0 || (offset + count) as usize > bytes.len() {
            return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
        }
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

pub(crate) const CRC32: oxjvm_vm::NativeClass = class(
    "java/util/zip/CRC32",
    Some("java/lang/Object"),
    &["java/util/zip/Checksum"],
    ACC_PUBLIC,
    &[field("value", "I", ACC_PRIVATE, None)],
    &CRC32_METHODS,
    None,
);

const CRC32C_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
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
        if offset < 0 || count < 0 || (offset + count) as usize > bytes.len() {
            return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
        }
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

const ADLER32_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
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
        if offset < 0 || count < 0 || (offset + count) as usize > bytes.len() {
            return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
        }
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

const ZIP_FILE_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
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
    ],
    &ZIP_FILE_METHODS,
    None,
);
