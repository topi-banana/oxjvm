//! `java.nio.Buffer`, `ByteBuffer`, `CharBuffer` and `ByteOrder`.
//!
//! The buffers are backed by ordinary Java arrays so that `array()`/`wrap()`
//! have the same aliasing behaviour as the JDK.  A slice/duplicate shares the
//! backing array and carries an `offset` into it.

use alloc::string::String;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::{boolean, class, field, int, long, method, object, receiver, ref_arg};

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

fn buffer_position(vm: &Vm<'_>, this: ObjectRef) -> i32 {
    field_int(vm, this, "position")
}

fn buffer_limit(vm: &Vm<'_>, this: ObjectRef) -> i32 {
    field_int(vm, this, "limit")
}

fn buffer_capacity(vm: &Vm<'_>, this: ObjectRef) -> i32 {
    field_int(vm, this, "capacity")
}

fn buffer_mark(vm: &Vm<'_>, this: ObjectRef) -> i32 {
    field_int(vm, this, "mark")
}

fn set_position(vm: &mut Vm<'_>, this: ObjectRef, value: i32) {
    set_field_int(vm, this, "position", value);
}

fn set_limit(vm: &mut Vm<'_>, this: ObjectRef, value: i32) {
    set_field_int(vm, this, "limit", value);
}

fn set_mark(vm: &mut Vm<'_>, this: ObjectRef, value: i32) {
    set_field_int(vm, this, "mark", value);
}

fn check_position(vm: &mut Vm<'_>, this: ObjectRef, value: i32) -> Result<(), VmError> {
    if value < 0 || value > buffer_limit(vm, this) {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("newPosition out of range"),
        ));
    }
    Ok(())
}

fn check_limit(vm: &mut Vm<'_>, this: ObjectRef, value: i32) -> Result<(), VmError> {
    if value < 0 || value > buffer_capacity(vm, this) {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("newLimit out of range"),
        ));
    }
    if buffer_position(vm, this) > value {
        set_position(vm, this, value);
    }
    Ok(())
}

fn buffer_clear(vm: &mut Vm<'_>, this: ObjectRef) -> Value {
    set_position(vm, this, 0);
    let capacity = buffer_capacity(vm, this);
    set_limit(vm, this, capacity);
    set_mark(vm, this, -1);
    Value::Ref(this)
}

fn buffer_flip(vm: &mut Vm<'_>, this: ObjectRef) -> Value {
    let position = buffer_position(vm, this);
    set_limit(vm, this, position);
    set_position(vm, this, 0);
    set_mark(vm, this, -1);
    Value::Ref(this)
}

fn buffer_rewind(vm: &mut Vm<'_>, this: ObjectRef) -> Value {
    set_position(vm, this, 0);
    set_mark(vm, this, -1);
    Value::Ref(this)
}

fn buffer_reset(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Value, VmError> {
    let mark = buffer_mark(vm, this);
    if mark < 0 {
        return Err(vm.throw_new("java/nio/InvalidMarkException", None));
    }
    set_position(vm, this, mark);
    Ok(Value::Ref(this))
}

fn buffer_remaining(vm: &Vm<'_>, this: ObjectRef) -> i32 {
    buffer_limit(vm, this) - buffer_position(vm, this)
}

// -------------------------------------------------------------------------------------------
// ByteOrder
// -------------------------------------------------------------------------------------------

const BYTE_ORDER_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let little = field_int(vm, this, "little") != 0;
            object(vm.make_string(if little {
                "LITTLE_ENDIAN"
            } else {
                "BIG_ENDIAN"
            })?)
        },
    ),
    method(
        "nativeOrder",
        "()Ljava/nio/ByteOrder;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/nio/ByteOrder")?;
            let name = if cfg!(target_endian = "little") {
                "LITTLE_ENDIAN"
            } else {
                "BIG_ENDIAN"
            };
            object(
                vm.read_static_ref_if_set(class, name, "Ljava/nio/ByteOrder;")
                    .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
];

fn byte_order_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/nio/ByteOrder")?;
    for (name, little) in [("BIG_ENDIAN", 0), ("LITTLE_ENDIAN", 1)] {
        let instance = vm.new_instance(class)?;
        set_field_int(vm, instance, "little", little);
        vm.set_static_value(class, name, "Ljava/nio/ByteOrder;", Value::Ref(instance));
    }
    Ok(())
}

pub(crate) const BYTE_ORDER: oxjvm_vm::NativeClass = class(
    "java/nio/ByteOrder",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("little", "I", ACC_PRIVATE, None),
        field(
            "BIG_ENDIAN",
            "Ljava/nio/ByteOrder;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "LITTLE_ENDIAN",
            "Ljava/nio/ByteOrder;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &BYTE_ORDER_METHODS,
    Some(byte_order_clinit),
);

fn byte_order_little(vm: &Vm<'_>, order: ObjectRef) -> bool {
    if order.is_null() {
        return false;
    }
    field_int(vm, order, "little") != 0
}

fn default_byte_order(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/nio/ByteOrder")?;
    vm.ensure_initialized(class)?;
    Ok(vm
        .read_static_ref_if_set(class, "BIG_ENDIAN", "Ljava/nio/ByteOrder;")
        .unwrap_or(ObjectRef::NULL))
}

// -------------------------------------------------------------------------------------------
// Buffer
// -------------------------------------------------------------------------------------------

const BUFFER_METHODS: [oxjvm_vm::NativeMethodDef; 20] = [
    method("capacity", "()I", ACC_PUBLIC, |vm, _, args| {
        int(buffer_capacity(vm, receiver(args)))
    }),
    method("position", "()I", ACC_PUBLIC, |vm, _, args| {
        int(buffer_position(vm, receiver(args)))
    }),
    method(
        "position",
        "(I)Ljava/nio/Buffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            check_position(vm, this, crate::int_arg(args, 1))?;
            set_position(vm, this, crate::int_arg(args, 1));
            if buffer_mark(vm, this) > crate::int_arg(args, 1) {
                set_mark(vm, this, -1);
            }
            object(this)
        },
    ),
    method("limit", "()I", ACC_PUBLIC, |vm, _, args| {
        int(buffer_limit(vm, receiver(args)))
    }),
    method(
        "limit",
        "(I)Ljava/nio/Buffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            check_limit(vm, this, crate::int_arg(args, 1))?;
            set_limit(vm, this, crate::int_arg(args, 1));
            if buffer_mark(vm, this) > crate::int_arg(args, 1) {
                set_mark(vm, this, -1);
            }
            object(this)
        },
    ),
    method("mark", "()Ljava/nio/Buffer;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        set_mark(vm, this, buffer_position(vm, this));
        object(this)
    }),
    method("reset", "()Ljava/nio/Buffer;", ACC_PUBLIC, |vm, _, args| {
        buffer_reset(vm, receiver(args))
    }),
    method("clear", "()Ljava/nio/Buffer;", ACC_PUBLIC, |vm, _, args| {
        Ok(buffer_clear(vm, receiver(args)))
    }),
    method("flip", "()Ljava/nio/Buffer;", ACC_PUBLIC, |vm, _, args| {
        Ok(buffer_flip(vm, receiver(args)))
    }),
    method(
        "rewind",
        "()Ljava/nio/Buffer;",
        ACC_PUBLIC,
        |vm, _, args| Ok(buffer_rewind(vm, receiver(args))),
    ),
    method("remaining", "()I", ACC_PUBLIC, |vm, _, args| {
        int(buffer_remaining(vm, receiver(args)))
    }),
    method("hasRemaining", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(buffer_remaining(vm, receiver(args)) > 0)
    }),
    method("isReadOnly", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("isDirect", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("hasArray", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("array", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, _| {
        Err(vm.throw_new("java/lang/UnsupportedOperationException", None))
    }),
    method("arrayOffset", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method("slice", "()Ljava/nio/Buffer;", ACC_PUBLIC, |vm, _, _| {
        Err(vm.throw_new("java/lang/UnsupportedOperationException", None))
    }),
    method(
        "duplicate",
        "()Ljava/nio/Buffer;",
        ACC_PUBLIC,
        |vm, _, _| Err(vm.throw_new("java/lang/UnsupportedOperationException", None)),
    ),
    method("compact", "()Ljava/nio/Buffer;", ACC_PUBLIC, |vm, _, _| {
        Err(vm.throw_new("java/lang/UnsupportedOperationException", None))
    }),
];

pub(crate) const BUFFER: oxjvm_vm::NativeClass = class(
    "java/nio/Buffer",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[
        field("position", "I", ACC_PRIVATE, None),
        field("limit", "I", ACC_PRIVATE, None),
        field("capacity", "I", ACC_PRIVATE, None),
        field("mark", "I", ACC_PRIVATE, None),
    ],
    &BUFFER_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// ByteBuffer
// -------------------------------------------------------------------------------------------

fn byte_buffer_bytes(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "bytes", "[B")
        .unwrap_or(ObjectRef::NULL)
}

fn byte_buffer_offset(vm: &Vm<'_>, this: ObjectRef) -> i32 {
    field_int(vm, this, "offset")
}

fn byte_buffer_read_only(vm: &Vm<'_>, this: ObjectRef) -> bool {
    field_int(vm, this, "readOnly") != 0
}

fn byte_buffer_order(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "order", "Ljava/nio/ByteOrder;")
        .unwrap_or(ObjectRef::NULL)
}

fn byte_at(vm: &mut Vm<'_>, bytes: ObjectRef, index: usize) -> Result<u8, VmError> {
    Ok(vm.array_get(bytes, index as i32)?.as_int() as u8)
}

fn put_byte_at(vm: &mut Vm<'_>, bytes: ObjectRef, index: usize, value: u8) -> Result<(), VmError> {
    vm.array_set(
        bytes,
        index as i32,
        Value::Int(i32::from(value as i8)),
        true,
    )
}

fn new_byte_buffer(
    vm: &mut Vm<'_>,
    bytes: ObjectRef,
    offset: i32,
    capacity: i32,
    position: i32,
    limit: i32,
    order: ObjectRef,
    read_only: bool,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/nio/OjvmByteBuffer")?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "bytes", "[B")?;
    vm.set_instance_ref(instance, declaring, field, bytes);
    set_field_int(vm, instance, "offset", offset);
    set_field_int(vm, instance, "capacity", capacity);
    set_field_int(vm, instance, "position", position);
    set_field_int(vm, instance, "limit", limit);
    set_field_int(vm, instance, "mark", -1);
    let (declaring, field) = vm.find_field(class, "order", "Ljava/nio/ByteOrder;")?;
    vm.set_instance_ref(instance, declaring, field, order);
    set_field_int(vm, instance, "readOnly", i32::from(read_only));
    Ok(instance)
}

fn byte_buffer_allocate(vm: &mut Vm<'_>, length: i32) -> Result<Value, VmError> {
    if length < 0 {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("negative capacity"),
        ));
    }
    let bytes = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, length as usize)?;
    let order = default_byte_order(vm)?;
    let buffer = new_byte_buffer(vm, bytes, 0, length, 0, length, order, false)?;
    object(buffer)
}

fn byte_buffer_remaining(vm: &Vm<'_>, this: ObjectRef) -> usize {
    byte_buffer_offset(vm, this) as usize + buffer_position(vm, this) as usize
}

fn byte_buffer_absolute(vm: &Vm<'_>, this: ObjectRef, index: i32) -> usize {
    byte_buffer_offset(vm, this) as usize + index as usize
}

fn check_get(vm: &mut Vm<'_>, this: ObjectRef, count: usize) -> Result<(), VmError> {
    if byte_buffer_remaining(vm, this) + count
        > byte_buffer_offset(vm, this) as usize + buffer_limit(vm, this) as usize
    {
        return Err(vm.throw_new("java/nio/BufferUnderflowException", None));
    }
    Ok(())
}

fn check_put(vm: &mut Vm<'_>, this: ObjectRef, count: usize) -> Result<(), VmError> {
    if byte_buffer_read_only(vm, this) {
        return Err(vm.throw_new("java/nio/ReadOnlyBufferException", None));
    }
    if byte_buffer_remaining(vm, this) + count
        > byte_buffer_offset(vm, this) as usize + buffer_limit(vm, this) as usize
    {
        return Err(vm.throw_new("java/nio/BufferOverflowException", None));
    }
    Ok(())
}

fn byte_buffer_get(vm: &mut Vm<'_>, this: ObjectRef) -> Result<u8, VmError> {
    check_get(vm, this, 1)?;
    let index = byte_buffer_remaining(vm, this);
    let value = byte_at(vm, byte_buffer_bytes(vm, this), index)?;
    set_position(vm, this, buffer_position(vm, this) + 1);
    Ok(value)
}

fn byte_buffer_put(vm: &mut Vm<'_>, this: ObjectRef, value: u8) -> Result<(), VmError> {
    check_put(vm, this, 1)?;
    let index = byte_buffer_remaining(vm, this);
    put_byte_at(vm, byte_buffer_bytes(vm, this), index, value)?;
    set_position(vm, this, buffer_position(vm, this) + 1);
    Ok(())
}

fn byte_buffer_get_int(vm: &mut Vm<'_>, this: ObjectRef) -> Result<i32, VmError> {
    let bytes = byte_buffer_bytes(vm, this);
    let index = byte_buffer_remaining(vm, this);
    check_get(vm, this, 4)?;
    let little = byte_order_little(vm, byte_buffer_order(vm, this));
    let mut value = 0u32;
    for offset in 0..4 {
        let byte = u32::from(byte_at(vm, bytes, index + offset)?);
        value |= if little {
            byte << (8 * offset)
        } else {
            byte << (8 * (3 - offset))
        };
    }
    set_position(vm, this, buffer_position(vm, this) + 4);
    Ok(value as i32)
}

fn byte_buffer_put_int(vm: &mut Vm<'_>, this: ObjectRef, value: i32) -> Result<(), VmError> {
    let bytes = byte_buffer_bytes(vm, this);
    let index = byte_buffer_remaining(vm, this);
    check_put(vm, this, 4)?;
    let little = byte_order_little(vm, byte_buffer_order(vm, this));
    let raw = value as u32;
    for offset in 0..4 {
        let byte = if little {
            (raw >> (8 * offset)) as u8
        } else {
            (raw >> (8 * (3 - offset))) as u8
        };
        put_byte_at(vm, bytes, index + offset, byte)?;
    }
    set_position(vm, this, buffer_position(vm, this) + 4);
    Ok(())
}

fn byte_buffer_get_long(vm: &mut Vm<'_>, this: ObjectRef) -> Result<i64, VmError> {
    let bytes = byte_buffer_bytes(vm, this);
    let index = byte_buffer_remaining(vm, this);
    check_get(vm, this, 8)?;
    let little = byte_order_little(vm, byte_buffer_order(vm, this));
    let mut value = 0u64;
    for offset in 0..8 {
        let byte = u64::from(byte_at(vm, bytes, index + offset)?);
        value |= if little {
            byte << (8 * offset)
        } else {
            byte << (8 * (7 - offset))
        };
    }
    set_position(vm, this, buffer_position(vm, this) + 8);
    Ok(value as i64)
}

fn byte_buffer_put_long(vm: &mut Vm<'_>, this: ObjectRef, value: i64) -> Result<(), VmError> {
    let bytes = byte_buffer_bytes(vm, this);
    let index = byte_buffer_remaining(vm, this);
    check_put(vm, this, 8)?;
    let little = byte_order_little(vm, byte_buffer_order(vm, this));
    let raw = value as u64;
    for offset in 0..8 {
        let byte = if little {
            (raw >> (8 * offset)) as u8
        } else {
            (raw >> (8 * (7 - offset))) as u8
        };
        put_byte_at(vm, bytes, index + offset, byte)?;
    }
    set_position(vm, this, buffer_position(vm, this) + 8);
    Ok(())
}

fn byte_buffer_get_short(vm: &mut Vm<'_>, this: ObjectRef) -> Result<i16, VmError> {
    let bytes = byte_buffer_bytes(vm, this);
    let index = byte_buffer_remaining(vm, this);
    check_get(vm, this, 2)?;
    let little = byte_order_little(vm, byte_buffer_order(vm, this));
    let a = u16::from(byte_at(vm, bytes, index)?);
    let b = u16::from(byte_at(vm, bytes, index + 1)?);
    set_position(vm, this, buffer_position(vm, this) + 2);
    Ok(if little {
        (a | (b << 8)) as i16
    } else {
        ((a << 8) | b) as i16
    })
}

fn byte_buffer_put_short(vm: &mut Vm<'_>, this: ObjectRef, value: i16) -> Result<(), VmError> {
    let bytes = byte_buffer_bytes(vm, this);
    let index = byte_buffer_remaining(vm, this);
    check_put(vm, this, 2)?;
    let little = byte_order_little(vm, byte_buffer_order(vm, this));
    let raw = value as u16;
    let (a, b) = if little {
        (raw as u8, (raw >> 8) as u8)
    } else {
        ((raw >> 8) as u8, raw as u8)
    };
    put_byte_at(vm, bytes, index, a)?;
    put_byte_at(vm, bytes, index + 1, b)?;
    set_position(vm, this, buffer_position(vm, this) + 2);
    Ok(())
}

const BYTE_BUFFER_METHODS: [oxjvm_vm::NativeMethodDef; 49] = [
    method(
        "allocate",
        "(I)Ljava/nio/ByteBuffer;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| byte_buffer_allocate(vm, crate::int_arg(args, 0)),
    ),
    method(
        "allocateDirect",
        "(I)Ljava/nio/ByteBuffer;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| byte_buffer_allocate(vm, crate::int_arg(args, 0)),
    ),
    method(
        "wrap",
        "([B)Ljava/nio/ByteBuffer;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let bytes = ref_arg(args, 0);
            let length = vm.array_length(bytes)? as i32;
            let order = default_byte_order(vm)?;
            object(new_byte_buffer(
                vm, bytes, 0, length, 0, length, order, false,
            )?)
        },
    ),
    method(
        "wrap",
        "([BII)Ljava/nio/ByteBuffer;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let bytes = ref_arg(args, 0);
            let offset = crate::int_arg(args, 1);
            let length = crate::int_arg(args, 2);
            if offset < 0 || length < 0 {
                return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
            }
            let total = vm.array_length(bytes)? as i32;
            if offset + length > total {
                return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
            }
            let order = default_byte_order(vm)?;
            object(new_byte_buffer(
                vm, bytes, offset, length, 0, length, order, false,
            )?)
        },
    ),
    method("get", "()B", ACC_PUBLIC, |vm, _, args| {
        int(i32::from(byte_buffer_get(vm, receiver(args))? as i8))
    }),
    method("get", "(I)B", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let index = crate::int_arg(args, 1);
        if index < 0 || index >= buffer_limit(vm, this) {
            return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
        }
        let value = byte_at(
            vm,
            byte_buffer_bytes(vm, this),
            byte_buffer_absolute(vm, this, index),
        )?;
        int(i32::from(value as i8))
    }),
    method(
        "put",
        "(B)Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            byte_buffer_put(vm, this, crate::int_arg(args, 1) as u8)?;
            object(this)
        },
    ),
    method(
        "put",
        "(IB)Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if byte_buffer_read_only(vm, this) {
                return Err(vm.throw_new("java/nio/ReadOnlyBufferException", None));
            }
            let index = crate::int_arg(args, 1);
            if index < 0 || index >= buffer_limit(vm, this) {
                return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
            }
            put_byte_at(
                vm,
                byte_buffer_bytes(vm, this),
                byte_buffer_absolute(vm, this, index),
                crate::int_arg(args, 2) as u8,
            )?;
            object(this)
        },
    ),
    method(
        "get",
        "([B)Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let target = ref_arg(args, 1);
            let length = vm.array_length(target)?;
            check_get(vm, this, length)?;
            for index in 0..length {
                let value = byte_buffer_get(vm, this)?;
                vm.array_set(
                    target,
                    index as i32,
                    Value::Int(i32::from(value as i8)),
                    true,
                )?;
            }
            object(this)
        },
    ),
    method(
        "get",
        "([BII)Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let target = ref_arg(args, 1);
            let offset = crate::int_arg(args, 2);
            let length = crate::int_arg(args, 3);
            check_get(vm, this, length as usize)?;
            for index in 0..length {
                let value = byte_buffer_get(vm, this)?;
                vm.array_set(
                    target,
                    offset + index,
                    Value::Int(i32::from(value as i8)),
                    true,
                )?;
            }
            object(this)
        },
    ),
    method(
        "put",
        "([B)Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let source = ref_arg(args, 1);
            let length = vm.array_length(source)?;
            check_put(vm, this, length)?;
            for index in 0..length {
                let value = vm.array_get(source, index as i32)?.as_int() as u8;
                byte_buffer_put(vm, this, value)?;
            }
            object(this)
        },
    ),
    method(
        "put",
        "([BII)Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let source = ref_arg(args, 1);
            let offset = crate::int_arg(args, 2);
            let length = crate::int_arg(args, 3);
            check_put(vm, this, length as usize)?;
            for index in 0..length {
                let value = vm.array_get(source, offset + index)?.as_int() as u8;
                byte_buffer_put(vm, this, value)?;
            }
            object(this)
        },
    ),
    method(
        "put",
        "(Ljava/nio/ByteBuffer;)Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let source = ref_arg(args, 1);
            if source == this {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
            }
            let length = buffer_remaining(vm, source);
            if length < 0 {
                return Err(vm.throw_new("java/nio/BufferUnderflowException", None));
            }
            check_put(vm, this, length as usize)?;
            for _ in 0..length {
                let value = byte_buffer_get(vm, source)?;
                byte_buffer_put(vm, this, value)?;
            }
            object(this)
        },
    ),
    method("getInt", "()I", ACC_PUBLIC, |vm, _, args| {
        int(byte_buffer_get_int(vm, receiver(args))?)
    }),
    method("getInt", "(I)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let index = crate::int_arg(args, 1);
        if index < 0 || index + 4 > buffer_limit(vm, this) {
            return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
        }
        let bytes = byte_buffer_bytes(vm, this);
        let little = byte_order_little(vm, byte_buffer_order(vm, this));
        let mut value = 0u32;
        for offset in 0..4 {
            let byte = u32::from(byte_at(
                vm,
                bytes,
                byte_buffer_absolute(vm, this, index + offset),
            )?);
            value |= if little {
                byte << (8 * offset)
            } else {
                byte << (8 * (3 - offset))
            };
        }
        int(value as i32)
    }),
    method(
        "putInt",
        "(I)Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            byte_buffer_put_int(vm, this, crate::int_arg(args, 1))?;
            object(this)
        },
    ),
    method(
        "putInt",
        "(II)Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if byte_buffer_read_only(vm, this) {
                return Err(vm.throw_new("java/nio/ReadOnlyBufferException", None));
            }
            let index = crate::int_arg(args, 1);
            if index < 0 || index + 4 > buffer_limit(vm, this) {
                return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
            }
            let bytes = byte_buffer_bytes(vm, this);
            let little = byte_order_little(vm, byte_buffer_order(vm, this));
            let raw = crate::int_arg(args, 2) as u32;
            for offset in 0..4 {
                let byte = if little {
                    (raw >> (8 * offset)) as u8
                } else {
                    (raw >> (8 * (3 - offset))) as u8
                };
                put_byte_at(
                    vm,
                    bytes,
                    byte_buffer_absolute(vm, this, index + offset),
                    byte,
                )?;
            }
            object(this)
        },
    ),
    method("getLong", "()J", ACC_PUBLIC, |vm, _, args| {
        long(byte_buffer_get_long(vm, receiver(args))?)
    }),
    method(
        "putLong",
        "(J)Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            byte_buffer_put_long(vm, this, crate::long_arg(args, 1))?;
            object(this)
        },
    ),
    method("getShort", "()S", ACC_PUBLIC, |vm, _, args| {
        int(i32::from(byte_buffer_get_short(vm, receiver(args))?))
    }),
    method(
        "putShort",
        "(S)Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            byte_buffer_put_short(vm, this, crate::int_arg(args, 1) as i16)?;
            object(this)
        },
    ),
    method("getChar", "()C", ACC_PUBLIC, |vm, _, args| {
        int(i32::from(byte_buffer_get_short(vm, receiver(args))? as u16))
    }),
    method(
        "putChar",
        "(C)Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            byte_buffer_put_short(vm, this, crate::int_arg(args, 1) as i16)?;
            object(this)
        },
    ),
    method("getFloat", "()F", ACC_PUBLIC, |vm, _, args| {
        Ok(Value::Float(f32::from_bits(
            byte_buffer_get_int(vm, receiver(args))? as u32,
        )))
    }),
    method(
        "putFloat",
        "(F)Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            byte_buffer_put_int(vm, this, crate::float_arg(args, 1).to_bits() as i32)?;
            object(this)
        },
    ),
    method("getDouble", "()D", ACC_PUBLIC, |vm, _, args| {
        Ok(Value::Double(f64::from_bits(
            byte_buffer_get_long(vm, receiver(args))? as u64,
        )))
    }),
    method(
        "putDouble",
        "(D)Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            byte_buffer_put_long(vm, this, crate::double_arg(args, 1).to_bits() as i64)?;
            object(this)
        },
    ),
    method(
        "order",
        "()Ljava/nio/ByteOrder;",
        ACC_PUBLIC,
        |vm, _, args| object(byte_buffer_order(vm, receiver(args))),
    ),
    method(
        "order",
        "(Ljava/nio/ByteOrder;)Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "order", "Ljava/nio/ByteOrder;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            object(this)
        },
    ),
    method("isReadOnly", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(byte_buffer_read_only(vm, receiver(args)))
    }),
    method("hasArray", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(!byte_buffer_read_only(vm, receiver(args)))
    }),
    method("array", "()[B", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if byte_buffer_read_only(vm, this) {
            return Err(vm.throw_new("java/nio/ReadOnlyBufferException", None));
        }
        object(byte_buffer_bytes(vm, this))
    }),
    method("arrayOffset", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if byte_buffer_read_only(vm, this) {
            return Err(vm.throw_new("java/nio/ReadOnlyBufferException", None));
        }
        int(byte_buffer_offset(vm, this))
    }),
    method(
        "slice",
        "()Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let remaining = buffer_remaining(vm, this);
            let offset = byte_buffer_offset(vm, this) + buffer_position(vm, this);
            object(new_byte_buffer(
                vm,
                byte_buffer_bytes(vm, this),
                offset,
                remaining,
                0,
                remaining,
                byte_buffer_order(vm, this),
                byte_buffer_read_only(vm, this),
            )?)
        },
    ),
    method(
        "duplicate",
        "()Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            object(new_byte_buffer(
                vm,
                byte_buffer_bytes(vm, this),
                byte_buffer_offset(vm, this),
                buffer_capacity(vm, this),
                buffer_position(vm, this),
                buffer_limit(vm, this),
                byte_buffer_order(vm, this),
                byte_buffer_read_only(vm, this),
            )?)
        },
    ),
    method(
        "asReadOnlyBuffer",
        "()Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            object(new_byte_buffer(
                vm,
                byte_buffer_bytes(vm, this),
                byte_buffer_offset(vm, this),
                buffer_capacity(vm, this),
                buffer_position(vm, this),
                buffer_limit(vm, this),
                byte_buffer_order(vm, this),
                true,
            )?)
        },
    ),
    method(
        "asCharBuffer",
        "()Ljava/nio/CharBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            // The view reads the remaining bytes in pairs, most significant byte first.
            let this = receiver(args);
            let bytes = byte_buffer_bytes(vm, this);
            let offset = byte_buffer_offset(vm, this);
            let position = buffer_position(vm, this);
            let limit = buffer_limit(vm, this);
            let count = ((limit - position).max(0) / 2) as usize;
            let chars = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Char, count)?;
            for index in 0..count {
                let high = vm
                    .array_get(bytes, offset + position + index as i32 * 2)?
                    .as_int();
                let low = vm
                    .array_get(bytes, offset + position + index as i32 * 2 + 1)?
                    .as_int();
                let unit = ((high as u32 & 0xFF) << 8) | (low as u32 & 0xFF);
                vm.array_set(chars, index as i32, Value::Int(unit as i32), false)?;
            }
            object(new_char_buffer(
                vm,
                chars,
                0,
                count as i32,
                0,
                count as i32,
                false,
            )?)
        },
    ),
    method(
        "compact",
        "()Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if byte_buffer_read_only(vm, this) {
                return Err(vm.throw_new("java/nio/ReadOnlyBufferException", None));
            }
            let remaining = buffer_remaining(vm, this);
            let bytes = byte_buffer_bytes(vm, this);
            for index in 0..remaining {
                let value = byte_at(
                    vm,
                    bytes,
                    byte_buffer_absolute(vm, this, buffer_position(vm, this) + index),
                )?;
                put_byte_at(
                    vm,
                    bytes,
                    byte_buffer_offset(vm, this) as usize + index as usize,
                    value,
                )?;
            }
            set_position(vm, this, remaining);
            set_limit(vm, this, buffer_capacity(vm, this));
            set_mark(vm, this, -1);
            object(this)
        },
    ),
    method(
        "position",
        "(I)Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            check_position(vm, this, crate::int_arg(args, 1))?;
            set_position(vm, this, crate::int_arg(args, 1));
            object(this)
        },
    ),
    method(
        "limit",
        "(I)Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            check_limit(vm, this, crate::int_arg(args, 1))?;
            set_limit(vm, this, crate::int_arg(args, 1));
            object(this)
        },
    ),
    method(
        "mark",
        "()Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            set_mark(vm, this, buffer_position(vm, this));
            object(this)
        },
    ),
    method(
        "reset",
        "()Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| buffer_reset(vm, receiver(args)),
    ),
    method(
        "clear",
        "()Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| Ok(buffer_clear(vm, receiver(args))),
    ),
    method(
        "flip",
        "()Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| Ok(buffer_flip(vm, receiver(args))),
    ),
    method(
        "rewind",
        "()Ljava/nio/ByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| Ok(buffer_rewind(vm, receiver(args))),
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            if other.is_null() || vm.class_name(vm.class_of(other)) != "java/nio/ByteBuffer" {
                return boolean(false);
            }
            let length = buffer_remaining(vm, this);
            if length != buffer_remaining(vm, other) {
                return boolean(false);
            }
            let left = byte_buffer_bytes(vm, this);
            let right = byte_buffer_bytes(vm, other);
            for index in 0..length {
                let a = byte_at(
                    vm,
                    left,
                    byte_buffer_absolute(vm, this, buffer_position(vm, this) + index),
                )?;
                let b = byte_at(
                    vm,
                    right,
                    byte_buffer_absolute(vm, other, buffer_position(vm, other) + index),
                )?;
                if a != b {
                    return boolean(false);
                }
            }
            boolean(true)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let length = buffer_remaining(vm, this);
        let bytes = byte_buffer_bytes(vm, this);
        let mut hash = 1i32;
        for index in 0..length {
            let value = byte_at(
                vm,
                bytes,
                byte_buffer_absolute(vm, this, buffer_position(vm, this) + index),
            )?;
            hash = hash.wrapping_mul(31).wrapping_add(i32::from(value as i8));
        }
        int(hash)
    }),
    method(
        "compareTo",
        "(Ljava/nio/ByteBuffer;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let left_len = buffer_remaining(vm, this);
            let right_len = buffer_remaining(vm, other);
            let length = left_len.min(right_len);
            let left = byte_buffer_bytes(vm, this);
            let right = byte_buffer_bytes(vm, other);
            for index in 0..length {
                let a = byte_at(
                    vm,
                    left,
                    byte_buffer_absolute(vm, this, buffer_position(vm, this) + index),
                )?;
                let b = byte_at(
                    vm,
                    right,
                    byte_buffer_absolute(vm, other, buffer_position(vm, other) + index),
                )?;
                if a != b {
                    return int(i32::from(a) - i32::from(b));
                }
            }
            int(left_len - right_len)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = alloc::format!(
                "java.nio.ByteBuffer[pos={} lim={} cap={}]",
                buffer_position(vm, this),
                buffer_limit(vm, this),
                buffer_capacity(vm, this)
            );
            object(vm.make_string(&text)?)
        },
    ),
];

pub(crate) const BYTE_BUFFER: oxjvm_vm::NativeClass = class(
    "java/nio/ByteBuffer",
    Some("java/nio/Buffer"),
    &["java/lang/Comparable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[
        field("bytes", "[B", ACC_PRIVATE, None),
        field("offset", "I", ACC_PRIVATE, None),
        field("order", "Ljava/nio/ByteOrder;", ACC_PRIVATE, None),
        field("readOnly", "I", ACC_PRIVATE, None),
    ],
    &BYTE_BUFFER_METHODS,
    None,
);

// The concrete byte buffer returned by the factories.
pub(crate) const OJVM_BYTE_BUFFER: oxjvm_vm::NativeClass = class(
    "java/nio/OjvmByteBuffer",
    Some("java/nio/ByteBuffer"),
    &["java/lang/Comparable"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[],
    &[],
    None,
);

// -------------------------------------------------------------------------------------------
// CharBuffer (the subset used by CharsetEncoder/Decoder)
// -------------------------------------------------------------------------------------------

fn char_buffer_chars(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "chars", "[C")
        .unwrap_or(ObjectRef::NULL)
}

fn char_buffer_offset(vm: &Vm<'_>, this: ObjectRef) -> i32 {
    field_int(vm, this, "offset")
}

fn char_buffer_read_only(vm: &Vm<'_>, this: ObjectRef) -> bool {
    field_int(vm, this, "readOnly") != 0
}

fn new_char_buffer(
    vm: &mut Vm<'_>,
    chars: ObjectRef,
    offset: i32,
    capacity: i32,
    position: i32,
    limit: i32,
    read_only: bool,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/nio/OjvmCharBuffer")?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "chars", "[C")?;
    vm.set_instance_ref(instance, declaring, field, chars);
    set_field_int(vm, instance, "offset", offset);
    set_field_int(vm, instance, "capacity", capacity);
    set_field_int(vm, instance, "position", position);
    set_field_int(vm, instance, "limit", limit);
    set_field_int(vm, instance, "mark", -1);
    set_field_int(vm, instance, "readOnly", i32::from(read_only));
    Ok(instance)
}

fn char_buffer_allocate(vm: &mut Vm<'_>, length: i32) -> Result<Value, VmError> {
    if length < 0 {
        return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
    }
    let chars = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Char, length as usize)?;
    object(new_char_buffer(vm, chars, 0, length, 0, length, false)?)
}

fn char_at(vm: &mut Vm<'_>, chars: ObjectRef, index: usize) -> Result<u16, VmError> {
    Ok(vm.array_get(chars, index as i32)?.as_int() as u16)
}

fn put_char_at(vm: &mut Vm<'_>, chars: ObjectRef, index: usize, value: u16) -> Result<(), VmError> {
    vm.array_set(chars, index as i32, Value::Int(i32::from(value)), true)
}

fn char_buffer_get(vm: &mut Vm<'_>, this: ObjectRef) -> Result<u16, VmError> {
    if buffer_remaining(vm, this) <= 0 {
        return Err(vm.throw_new("java/nio/BufferUnderflowException", None));
    }
    let index = char_buffer_offset(vm, this) as usize + buffer_position(vm, this) as usize;
    let value = char_at(vm, char_buffer_chars(vm, this), index)?;
    set_position(vm, this, buffer_position(vm, this) + 1);
    Ok(value)
}

fn char_buffer_put(vm: &mut Vm<'_>, this: ObjectRef, value: u16) -> Result<(), VmError> {
    if char_buffer_read_only(vm, this) {
        return Err(vm.throw_new("java/nio/ReadOnlyBufferException", None));
    }
    if buffer_remaining(vm, this) <= 0 {
        return Err(vm.throw_new("java/nio/BufferOverflowException", None));
    }
    let index = char_buffer_offset(vm, this) as usize + buffer_position(vm, this) as usize;
    put_char_at(vm, char_buffer_chars(vm, this), index, value)?;
    set_position(vm, this, buffer_position(vm, this) + 1);
    Ok(())
}

pub(crate) fn char_buffer_text(vm: &mut Vm<'_>, this: ObjectRef) -> Result<String, VmError> {
    let length = buffer_remaining(vm, this);
    let chars = char_buffer_chars(vm, this);
    let mut units = Vec::with_capacity(length as usize);
    for index in 0..length {
        units.push(char_at(
            vm,
            chars,
            char_buffer_offset(vm, this) as usize
                + buffer_position(vm, this) as usize
                + index as usize,
        )?);
    }
    Ok(String::from_utf16_lossy(&units))
}

pub(crate) fn char_buffer_from_text(vm: &mut Vm<'_>, text: &str) -> Result<ObjectRef, VmError> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let chars = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Char, units.len())?;
    for (index, unit) in units.iter().enumerate() {
        put_char_at(vm, chars, index, *unit)?;
    }
    new_char_buffer(
        vm,
        chars,
        0,
        units.len() as i32,
        0,
        units.len() as i32,
        false,
    )
}

const CHAR_BUFFER_METHODS: [oxjvm_vm::NativeMethodDef; 22] = [
    method(
        "allocate",
        "(I)Ljava/nio/CharBuffer;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| char_buffer_allocate(vm, crate::int_arg(args, 0)),
    ),
    method(
        "wrap",
        "([C)Ljava/nio/CharBuffer;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let chars = ref_arg(args, 0);
            let length = vm.array_length(chars)? as i32;
            object(new_char_buffer(vm, chars, 0, length, 0, length, false)?)
        },
    ),
    method(
        "wrap",
        "([CII)Ljava/nio/CharBuffer;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let chars = ref_arg(args, 0);
            let offset = crate::int_arg(args, 1);
            let length = crate::int_arg(args, 2);
            let total = vm.array_length(chars)? as i32;
            if offset < 0 || length < 0 || offset + length > total {
                return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
            }
            object(new_char_buffer(
                vm, chars, offset, length, 0, length, false,
            )?)
        },
    ),
    method(
        "wrap",
        "(Ljava/lang/CharSequence;)Ljava/nio/CharBuffer;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 0));
            object(char_buffer_from_text(vm, &text)?)
        },
    ),
    method("get", "()C", ACC_PUBLIC, |vm, _, args| {
        int(i32::from(char_buffer_get(vm, receiver(args))?))
    }),
    method(
        "put",
        "(C)Ljava/nio/CharBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            char_buffer_put(vm, this, crate::int_arg(args, 1) as u16)?;
            object(this)
        },
    ),
    method("length", "()I", ACC_PUBLIC, |vm, _, args| {
        int(buffer_remaining(vm, receiver(args)))
    }),
    method("charAt", "(I)C", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let index = crate::int_arg(args, 1);
        if index < 0 || index >= buffer_remaining(vm, this) {
            return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
        }
        let value = char_at(
            vm,
            char_buffer_chars(vm, this),
            char_buffer_offset(vm, this) as usize
                + buffer_position(vm, this) as usize
                + index as usize,
        )?;
        int(i32::from(value))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = char_buffer_text(vm, receiver(args))?;
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "subSequence",
        "(II)Ljava/nio/CharBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let start = crate::int_arg(args, 1);
            let end = crate::int_arg(args, 2);
            if start < 0 || end < start || end > buffer_remaining(vm, this) {
                return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
            }
            let offset = char_buffer_offset(vm, this) + buffer_position(vm, this) + start;
            object(new_char_buffer(
                vm,
                char_buffer_chars(vm, this),
                offset,
                end - start,
                0,
                end - start,
                char_buffer_read_only(vm, this),
            )?)
        },
    ),
    method("position", "()I", ACC_PUBLIC, |vm, _, args| {
        int(buffer_position(vm, receiver(args)))
    }),
    method(
        "position",
        "(I)Ljava/nio/CharBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            check_position(vm, this, crate::int_arg(args, 1))?;
            set_position(vm, this, crate::int_arg(args, 1));
            object(this)
        },
    ),
    method("limit", "()I", ACC_PUBLIC, |vm, _, args| {
        int(buffer_limit(vm, receiver(args)))
    }),
    method(
        "limit",
        "(I)Ljava/nio/CharBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            check_limit(vm, this, crate::int_arg(args, 1))?;
            set_limit(vm, this, crate::int_arg(args, 1));
            object(this)
        },
    ),
    method("remaining", "()I", ACC_PUBLIC, |vm, _, args| {
        int(buffer_remaining(vm, receiver(args)))
    }),
    method("hasRemaining", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(buffer_remaining(vm, receiver(args)) > 0)
    }),
    method(
        "flip",
        "()Ljava/nio/CharBuffer;",
        ACC_PUBLIC,
        |vm, _, args| Ok(buffer_flip(vm, receiver(args))),
    ),
    method(
        "clear",
        "()Ljava/nio/CharBuffer;",
        ACC_PUBLIC,
        |vm, _, args| Ok(buffer_clear(vm, receiver(args))),
    ),
    method(
        "rewind",
        "()Ljava/nio/CharBuffer;",
        ACC_PUBLIC,
        |vm, _, args| Ok(buffer_rewind(vm, receiver(args))),
    ),
    method("isReadOnly", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(char_buffer_read_only(vm, receiver(args)))
    }),
    method("hasArray", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(!char_buffer_read_only(vm, receiver(args)))
    }),
    method("array", "()[C", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if char_buffer_read_only(vm, this) {
            return Err(vm.throw_new("java/nio/ReadOnlyBufferException", None));
        }
        object(char_buffer_chars(vm, this))
    }),
];

pub(crate) const CHAR_BUFFER: oxjvm_vm::NativeClass = class(
    "java/nio/CharBuffer",
    Some("java/nio/Buffer"),
    &[
        "java/lang/Comparable",
        "java/lang/Appendable",
        "java/lang/CharSequence",
    ],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[
        field("chars", "[C", ACC_PRIVATE, None),
        field("offset", "I", ACC_PRIVATE, None),
        field("readOnly", "I", ACC_PRIVATE, None),
    ],
    &CHAR_BUFFER_METHODS,
    None,
);

pub(crate) const OJVM_CHAR_BUFFER: oxjvm_vm::NativeClass = class(
    "java/nio/OjvmCharBuffer",
    Some("java/nio/CharBuffer"),
    &["java/lang/Comparable"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[],
    &[],
    None,
);

/// Remaining bytes of a `ByteBuffer`, without changing its position.
pub(crate) fn byte_buffer_remaining_bytes(
    vm: &mut Vm<'_>,
    this: ObjectRef,
) -> Result<Vec<u8>, VmError> {
    let length = buffer_remaining(vm, this);
    let bytes = byte_buffer_bytes(vm, this);
    let mut out = Vec::with_capacity(length as usize);
    for index in 0..length {
        out.push(byte_at(
            vm,
            bytes,
            byte_buffer_absolute(vm, this, buffer_position(vm, this) + index),
        )?);
    }
    Ok(out)
}

/// Build a fresh `ByteBuffer` over `bytes`.
pub(crate) fn byte_buffer_from_bytes(vm: &mut Vm<'_>, bytes: &[u8]) -> Result<ObjectRef, VmError> {
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, bytes.len())?;
    for (index, byte) in bytes.iter().enumerate() {
        put_byte_at(vm, array, index, *byte)?;
    }
    let order = default_byte_order(vm)?;
    new_byte_buffer(
        vm,
        array,
        0,
        bytes.len() as i32,
        0,
        bytes.len() as i32,
        order,
        false,
    )
}

/// Put as many bytes as fit, returning the number written.
pub(crate) fn byte_buffer_put_slice(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    bytes: &[u8],
) -> Result<usize, VmError> {
    let count = (buffer_remaining(vm, this) as usize).min(bytes.len());
    check_put(vm, this, count)?;
    for byte in &bytes[..count] {
        byte_buffer_put(vm, this, *byte)?;
    }
    Ok(count)
}
