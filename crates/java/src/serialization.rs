//! A compact in-process object stream.
//!
//! Objects are kept in a per-VM handle table; the wire format is the handle
//! number followed by primitive values.  This is sufficient for the common
//! `ByteArrayOutputStream` round-trip pattern while keeping references and
//! cycles intact.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::{
    boolean, class, field, int, long, method, object, receiver, ref_arg, string_arg, void,
};

fn object_map(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/io/ObjectOutputStream")?;
    if let Some(map) = vm.read_static_ref_if_set(class, "objects", "Ljava/util/HashMap;") {
        return Ok(map);
    }
    let map = crate::util::hash_map_new(vm)?;
    vm.set_static_value(class, "objects", "Ljava/util/HashMap;", Value::Ref(map));
    Ok(map)
}

fn map_lookup(vm: &mut Vm<'_>, id: i32) -> Result<ObjectRef, VmError> {
    let map = object_map(vm)?;
    let key = crate::boxed::box_value(vm, Value::Int(id), "I")?;
    let (class, method) =
        vm.resolve_virtual_method(map, "get", "(Ljava/lang/Object;)Ljava/lang/Object;")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(map), key])?
        .as_ref())
}

fn map_insert(vm: &mut Vm<'_>, id: i32, value: ObjectRef) -> Result<(), VmError> {
    let map = object_map(vm)?;
    let key = crate::boxed::box_value(vm, Value::Int(id), "I")?;
    crate::util::hash_map_insert(vm, map, key.as_ref(), value)
}

fn write_to_output(vm: &mut Vm<'_>, out: ObjectRef, bytes: &[u8]) -> Result<(), VmError> {
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, bytes.len())?;
    for (index, byte) in bytes.iter().enumerate() {
        vm.array_set(
            array,
            index as i32,
            Value::Int(i32::from(*byte as i8)),
            false,
        )?;
    }
    let (class, method) = vm.resolve_virtual_method(out, "write", "([B)V")?;
    vm.invoke_method(class, method, vec![Value::Ref(out), Value::Ref(array)])?;
    Ok(())
}

fn read_byte(vm: &mut Vm<'_>, input: ObjectRef) -> Result<u8, VmError> {
    let (class, method) = vm.resolve_virtual_method(input, "read", "()I")?;
    let value = vm
        .invoke_method(class, method, vec![Value::Ref(input)])?
        .as_int();
    if value < 0 {
        return Err(vm.throw_new("java/io/EOFException", None));
    }
    Ok(value as u8)
}

fn write_int_value(vm: &mut Vm<'_>, out: ObjectRef, value: i32) -> Result<(), VmError> {
    write_to_output(vm, out, &value.to_be_bytes())
}

fn read_int_value(vm: &mut Vm<'_>, input: ObjectRef) -> Result<i32, VmError> {
    let mut bytes = [0u8; 4];
    for byte in &mut bytes {
        *byte = read_byte(vm, input)?;
    }
    Ok(i32::from_be_bytes(bytes))
}

fn write_long_value(vm: &mut Vm<'_>, out: ObjectRef, value: i64) -> Result<(), VmError> {
    write_to_output(vm, out, &value.to_be_bytes())
}

fn read_long_value(vm: &mut Vm<'_>, input: ObjectRef) -> Result<i64, VmError> {
    let mut bytes = [0u8; 8];
    for byte in &mut bytes {
        *byte = read_byte(vm, input)?;
    }
    Ok(i64::from_be_bytes(bytes))
}

fn write_short_value(vm: &mut Vm<'_>, out: ObjectRef, value: i16) -> Result<(), VmError> {
    write_to_output(vm, out, &value.to_be_bytes())
}

fn read_short_value(vm: &mut Vm<'_>, input: ObjectRef) -> Result<i16, VmError> {
    let mut bytes = [0u8; 2];
    for byte in &mut bytes {
        *byte = read_byte(vm, input)?;
    }
    Ok(i16::from_be_bytes(bytes))
}

fn output_of(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "out", "Ljava/io/OutputStream;")
        .unwrap_or(ObjectRef::NULL)
}

fn input_of(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "in", "Ljava/io/InputStream;")
        .unwrap_or(ObjectRef::NULL)
}

const OBJECT_OUTPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 21] = [
    method(
        "<init>",
        "(Ljava/io/OutputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "out", "Ljava/io/OutputStream;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/OutputStream;Z)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "out", "Ljava/io/OutputStream;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method(
        "writeObject",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let counter = crate::lang::read_int_field(vm, this, "counter");
            let id = if counter == 0 {
                let class = vm.resolve_class("java/io/ObjectOutputStream")?;
                crate::lang::write_int_field(vm, this, "counter", 1);
                let _ = class;
                1
            } else {
                crate::lang::write_int_field(vm, this, "counter", counter + 1);
                counter + 1
            };
            let object = ref_arg(args, 1);
            map_insert(vm, id, object)?;
            let out = output_of(vm, this);
            write_int_value(vm, out, id)?;
            let _ = class;
            void()
        },
    ),
    method(
        "writeUnshared",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let counter = crate::lang::read_int_field(vm, this, "counter") + 1;
            crate::lang::write_int_field(vm, this, "counter", counter);
            map_insert(vm, counter, ref_arg(args, 1))?;
            let out = output_of(vm, this);
            write_int_value(vm, out, counter)?;
            void()
        },
    ),
    method("defaultWriteObject", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "putFields",
        "()Ljava/io/ObjectOutputStream$PutField;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/io/ObjectOutputStream$PutField")?;
            object(vm.new_instance(class)?)
        },
    ),
    method("writeFields", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("reset", "()V", ACC_PUBLIC, |vm, _, args| {
        crate::lang::write_int_field(vm, receiver(args), "counter", 0);
        void()
    }),
    method("useProtocolVersion", "(I)V", ACC_PUBLIC, |_, _, _| void()),
    method("write", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let out = output_of(vm, receiver(args));
        write_to_output(vm, out, &[crate::int_arg(args, 1) as u8])?;
        void()
    }),
    method("write", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
        let out = output_of(vm, receiver(args));
        write_to_output(vm, out, &bytes)?;
        void()
    }),
    method("write", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
        let offset = crate::int_arg(args, 2).max(0) as usize;
        let count = crate::int_arg(args, 3).max(0) as usize;
        let end = (offset + count).min(bytes.len());
        let out = output_of(vm, receiver(args));
        write_to_output(vm, out, &bytes[offset..end])?;
        void()
    }),
    method("writeBoolean", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        let out = output_of(vm, receiver(args));
        write_to_output(vm, out, &[u8::from(crate::int_arg(args, 1) != 0)])?;
        void()
    }),
    method("writeByte", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let out = output_of(vm, receiver(args));
        write_to_output(vm, out, &[crate::int_arg(args, 1) as u8])?;
        void()
    }),
    method("writeShort", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let out = output_of(vm, receiver(args));
        write_short_value(vm, out, crate::int_arg(args, 1) as i16)?;
        void()
    }),
    method("writeChar", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let out = output_of(vm, receiver(args));
        write_short_value(vm, out, crate::int_arg(args, 1) as i16)?;
        void()
    }),
    method("writeInt", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let out = output_of(vm, receiver(args));
        write_int_value(vm, out, crate::int_arg(args, 1))?;
        void()
    }),
    method("writeLong", "(J)V", ACC_PUBLIC, |vm, _, args| {
        let out = output_of(vm, receiver(args));
        write_long_value(vm, out, crate::long_arg(args, 1))?;
        void()
    }),
    method("writeFloat", "(F)V", ACC_PUBLIC, |vm, _, args| {
        let out = output_of(vm, receiver(args));
        write_int_value(vm, out, crate::float_arg(args, 1).to_bits() as i32)?;
        void()
    }),
    method("writeDouble", "(D)V", ACC_PUBLIC, |vm, _, args| {
        let out = output_of(vm, receiver(args));
        write_long_value(vm, out, crate::double_arg(args, 1).to_bits() as i64)?;
        void()
    }),
    method(
        "writeUTF",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = string_arg(vm, args[1])?;
            let bytes = text.as_bytes();
            let out = output_of(vm, receiver(args));
            write_short_value(vm, out, bytes.len().min(i16::MAX as usize) as i16)?;
            write_to_output(vm, out, &bytes[..bytes.len().min(i16::MAX as usize)])?;
            void()
        },
    ),
];

pub(crate) const OBJECT_OUTPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/io/ObjectOutputStream",
    Some("java/io/OutputStream"),
    &[],
    ACC_PUBLIC,
    &[
        field("out", "Ljava/io/OutputStream;", ACC_PRIVATE, None),
        field("counter", "I", ACC_PRIVATE, None),
        field(
            "objects",
            "Ljava/util/HashMap;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
    ],
    &OBJECT_OUTPUT_STREAM_METHODS,
    None,
);

const OBJECT_INPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 18] = [
    method(
        "<init>",
        "(Ljava/io/InputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "in", "Ljava/io/InputStream;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method(
        "readObject",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let input = input_of(vm, receiver(args));
            let id = read_int_value(vm, input)?;
            object(map_lookup(vm, id)?)
        },
    ),
    method(
        "readUnshared",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let input = input_of(vm, receiver(args));
            let id = read_int_value(vm, input)?;
            object(map_lookup(vm, id)?)
        },
    ),
    method("defaultReadObject", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("read", "()I", ACC_PUBLIC, |vm, _, args| {
        let input = input_of(vm, receiver(args));
        int(i32::from(read_byte(vm, input)?))
    }),
    method("read", "([B)I", ACC_PUBLIC, |vm, _, args| {
        let target = ref_arg(args, 1);
        let length = vm.array_length(target)?;
        object_input_read(vm, receiver(args), target, 0, length as i32)
    }),
    method("read", "([BII)I", ACC_PUBLIC, |vm, _, args| {
        let target = ref_arg(args, 1);
        let offset = crate::int_arg(args, 2);
        let count = crate::int_arg(args, 3);
        object_input_read(vm, receiver(args), target, offset, count)
    }),
    method("readBoolean", "()Z", ACC_PUBLIC, |vm, _, args| {
        let input = input_of(vm, receiver(args));
        boolean(read_byte(vm, input)? != 0)
    }),
    method("readByte", "()B", ACC_PUBLIC, |vm, _, args| {
        let input = input_of(vm, receiver(args));
        int(i32::from(read_byte(vm, input)? as i8))
    }),
    method("readUnsignedByte", "()I", ACC_PUBLIC, |vm, _, args| {
        let input = input_of(vm, receiver(args));
        int(i32::from(read_byte(vm, input)?))
    }),
    method("readShort", "()S", ACC_PUBLIC, |vm, _, args| {
        let input = input_of(vm, receiver(args));
        int(i32::from(read_short_value(vm, input)?))
    }),
    method("readChar", "()C", ACC_PUBLIC, |vm, _, args| {
        let input = input_of(vm, receiver(args));
        int(i32::from(read_short_value(vm, input)? as u16))
    }),
    method("readInt", "()I", ACC_PUBLIC, |vm, _, args| {
        let input = input_of(vm, receiver(args));
        int(read_int_value(vm, input)?)
    }),
    method("readLong", "()J", ACC_PUBLIC, |vm, _, args| {
        let input = input_of(vm, receiver(args));
        long(read_long_value(vm, input)?)
    }),
    method("readFloat", "()F", ACC_PUBLIC, |vm, _, args| {
        let input = input_of(vm, receiver(args));
        Ok(Value::Float(f32::from_bits(
            read_int_value(vm, input)? as u32
        )))
    }),
    method("readDouble", "()D", ACC_PUBLIC, |vm, _, args| {
        let input = input_of(vm, receiver(args));
        Ok(Value::Double(f64::from_bits(
            read_long_value(vm, input)? as u64
        )))
    }),
    method(
        "readUTF",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let input = input_of(vm, receiver(args));
            let length = read_short_value(vm, input)? as usize;
            let mut bytes = Vec::with_capacity(length);
            for _ in 0..length {
                bytes.push(read_byte(vm, input)?);
            }
            object(vm.make_string(&String::from_utf8_lossy(&bytes))?)
        },
    ),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let input = input_of(vm, this);
        if !input.is_null() {
            let (class, method) = vm.resolve_virtual_method(input, "close", "()V")?;
            vm.invoke_method(class, method, vec![Value::Ref(input)])?;
        }
        void()
    }),
];

pub(crate) const OBJECT_INPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/io/ObjectInputStream",
    Some("java/io/InputStream"),
    &[],
    ACC_PUBLIC,
    &[field("in", "Ljava/io/InputStream;", ACC_PRIVATE, None)],
    &OBJECT_INPUT_STREAM_METHODS,
    None,
);

fn object_input_read(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    target: ObjectRef,
    offset: i32,
    count: i32,
) -> Result<Value, VmError> {
    let input = input_of(vm, this);
    let mut read = 0;
    for index in 0..count {
        let byte = match read_byte(vm, input) {
            Ok(byte) => byte,
            Err(_) => break,
        };
        vm.array_set(
            target,
            offset + index,
            Value::Int(i32::from(byte as i8)),
            true,
        )?;
        read += 1;
    }
    int(read)
}

const PUT_FIELD_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method(
        "put",
        "(Ljava/lang/String;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method("put", "(Ljava/lang/String;Z)V", ACC_PUBLIC, |_, _, _| {
        void()
    }),
    method("put", "(Ljava/lang/String;B)V", ACC_PUBLIC, |_, _, _| {
        void()
    }),
    method("put", "(Ljava/lang/String;C)V", ACC_PUBLIC, |_, _, _| {
        void()
    }),
    method("put", "(Ljava/lang/String;S)V", ACC_PUBLIC, |_, _, _| {
        void()
    }),
    method("put", "(Ljava/lang/String;I)V", ACC_PUBLIC, |_, _, _| {
        void()
    }),
    method("put", "(Ljava/lang/String;J)V", ACC_PUBLIC, |_, _, _| {
        void()
    }),
    method("put", "(Ljava/lang/String;F)V", ACC_PUBLIC, |_, _, _| {
        void()
    }),
    method("put", "(Ljava/lang/String;D)V", ACC_PUBLIC, |_, _, _| {
        void()
    }),
    method(
        "write",
        "(Ljava/io/ObjectOutput;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
];

pub(crate) const PUT_FIELD: oxjvm_vm::NativeClass = class(
    "java/io/ObjectOutputStream$PutField",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &PUT_FIELD_METHODS,
    None,
);

pub(crate) const OBJECT_INPUT_VALIDATION: oxjvm_vm::NativeClass = class(
    "java/io/ObjectInputValidation",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[method(
        "validateObject",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    )],
    None,
);

pub(crate) const OBJECT_INPUT_FILTER: oxjvm_vm::NativeClass = class(
    "java/io/ObjectInputFilter",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const OBJECT_INPUT_FILTER_STATUS: oxjvm_vm::NativeClass = class(
    "java/io/ObjectInputFilter$Status",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field(
            "UNDECIDED",
            "Ljava/io/ObjectInputFilter$Status;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "ALLOWED",
            "Ljava/io/ObjectInputFilter$Status;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "REJECTED",
            "Ljava/io/ObjectInputFilter$Status;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &[],
    Some(object_input_filter_status_clinit),
);

fn object_input_filter_status_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/io/ObjectInputFilter$Status")?;
    for name in ["UNDECIDED", "ALLOWED", "REJECTED"] {
        let instance = vm.new_instance(class)?;
        vm.set_static_value(
            class,
            name,
            "Ljava/io/ObjectInputFilter$Status;",
            Value::Ref(instance),
        );
    }
    Ok(())
}
