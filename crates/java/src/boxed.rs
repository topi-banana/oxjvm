//! The boxed primitive classes and `Number`.

use alloc::string::{String, ToString};

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError, format};

use crate::{
    bool_arg, boolean, class, double, field, float, int, int_arg, long, long_arg, method, object,
    receiver, ref_arg, string_arg, void,
};

// -------------------------------------------------------------------------------------------
// Number
// -------------------------------------------------------------------------------------------

const NUMBER_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "intValue",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "longValue",
        "()J",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "floatValue",
        "()F",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "doubleValue",
        "()D",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method("byteValue", "()B", ACC_PUBLIC, number_byte_value),
    method("shortValue", "()S", ACC_PUBLIC, number_short_value),
];

fn number_byte_value(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let (class, method) = vm.resolve_virtual_method(receiver(args), "intValue", "()I")?;
    let value = vm.invoke_method(class, method, args.to_vec())?;
    int(value.as_int() as i8 as i32)
}

fn number_short_value(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let (class, method) = vm.resolve_virtual_method(receiver(args), "intValue", "()I")?;
    let value = vm.invoke_method(class, method, args.to_vec())?;
    int(value.as_int() as i16 as i32)
}

pub(crate) const NUMBER: oxjvm_vm::NativeClass = class(
    "java/lang/Number",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &NUMBER_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Integer
// -------------------------------------------------------------------------------------------

const INTEGER_METHODS: [oxjvm_vm::NativeMethodDef; 24] = [
    method("<init>", "(I)V", ACC_PUBLIC, boxed_int_init),
    method("intValue", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_value_int(vm, receiver(args)))
    }),
    method("longValue", "()J", ACC_PUBLIC, |vm, _, args| {
        long(i64::from(read_value_int(vm, receiver(args))))
    }),
    method("floatValue", "()F", ACC_PUBLIC, |vm, _, args| {
        float(read_value_int(vm, receiver(args)) as f32)
    }),
    method("doubleValue", "()D", ACC_PUBLIC, |vm, _, args| {
        double(f64::from(read_value_int(vm, receiver(args))))
    }),
    method("byteValue", "()B", ACC_PUBLIC, |vm, _, args| {
        int(read_value_int(vm, receiver(args)) as i8 as i32)
    }),
    method("shortValue", "()S", ACC_PUBLIC, |vm, _, args| {
        int(read_value_int(vm, receiver(args)) as i16 as i32)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = format::int_to_string(read_value_int(vm, receiver(args)));
            object(vm.make_string(&text)?)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_value_int(vm, receiver(args)))
    }),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        boxed_int_equals,
    ),
    method(
        "compareTo",
        "(Ljava/lang/Integer;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let a = read_value_int(vm, receiver(args));
            let b = read_value_int(vm, ref_arg(args, 1));
            int(a - b)
        },
    ),
    method(
        "toString",
        "(I)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = format::int_to_string(int_arg(args, 0));
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toString",
        "(II)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text =
                format::int_to_radix_string(int_arg(args, 0), int_arg(args, 1).clamp(2, 36) as u32);
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "parseInt",
        "(Ljava/lang/String;)I",
        ACC_PUBLIC | ACC_STATIC,
        parse_int_10,
    ),
    method(
        "parseInt",
        "(Ljava/lang/String;I)I",
        ACC_PUBLIC | ACC_STATIC,
        parse_int_radix,
    ),
    method(
        "valueOf",
        "(I)Ljava/lang/Integer;",
        ACC_PUBLIC | ACC_STATIC,
        boxed_int_value_of,
    ),
    method(
        "valueOf",
        "(Ljava/lang/String;)Ljava/lang/Integer;",
        ACC_PUBLIC | ACC_STATIC,
        boxed_int_value_of_string,
    ),
    method(
        "valueOf",
        "(Ljava/lang/String;I)Ljava/lang/Integer;",
        ACC_PUBLIC | ACC_STATIC,
        boxed_int_value_of_string_radix,
    ),
    method("compare", "(II)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        int(int_arg(args, 0).cmp(&int_arg(args, 1)) as i32)
    }),
    method(
        "compareUnsigned",
        "(II)I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| int((int_arg(args, 0) as u32).cmp(&(int_arg(args, 1) as u32)) as i32),
    ),
    method("hashCode", "(I)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        int(int_arg(args, 0))
    }),
    method(
        "toHexString",
        "(I)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = format::int_to_radix_string(int_arg(args, 0), 16);
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toBinaryString",
        "(I)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = format::int_to_radix_string(int_arg(args, 0), 2);
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toOctalString",
        "(I)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = format::int_to_radix_string(int_arg(args, 0), 8);
            object(vm.make_string(&text)?)
        },
    ),
];

fn boxed_int_init(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    write_value_int(vm, receiver(args), int_arg(args, 1));
    void()
}

pub(crate) fn read_value_int(vm: &Vm<'_>, object: ObjectRef) -> i32 {
    crate::lang::read_int_field(vm, object, "value")
}

pub(crate) fn write_value_int(vm: &mut Vm<'_>, object: ObjectRef, value: i32) {
    crate::lang::write_int_field(vm, object, "value", value);
}

fn boxed_int_equals(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let other = ref_arg(args, 1);
    if other.is_null() {
        return boolean(false);
    }
    let class = vm.class_of(receiver(args));
    if vm.class_of(other) != class {
        return boolean(false);
    }
    boolean(read_value_int(vm, receiver(args)) == read_value_int(vm, other))
}

fn parse_int_10(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = string_arg(vm, args[0])?;
    match format::parse_int(&text) {
        Ok(value) => int(value),
        Err(()) => Err(vm.throw_new(
            "java/lang/NumberFormatException",
            Some(&alloc::format!("For input string: \"{text}\"")),
        )),
    }
}

fn parse_int_radix(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = string_arg(vm, args[0])?;
    let radix = int_arg(args, 1);
    match format::parse_int_radix(&text, radix as u32) {
        Ok(value) => int(value),
        Err(()) => Err(vm.throw_new(
            "java/lang/NumberFormatException",
            Some(&alloc::format!("For input string: \"{text}\"")),
        )),
    }
}

fn boxed_int_value_of(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/lang/Integer")?;
    let instance = vm.new_instance(class)?;
    write_value_int(vm, instance, int_arg(args, 0));
    object(instance)
}

fn boxed_int_value_of_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let value = parse_int_10(vm, _c, args)?;
    boxed_int_value_of(vm, _c, &[value])
}

fn boxed_int_value_of_string_radix(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let value = parse_int_radix(vm, _c, args)?;
    boxed_int_value_of(vm, _c, &[value])
}

fn integer_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    set_type_field(vm, "java/lang/Integer", oxjvm_classfile::BaseType::Int)
}

pub(crate) fn set_type_field(
    vm: &mut Vm<'_>,
    class_name: &str,
    base: oxjvm_classfile::BaseType,
) -> Result<(), VmError> {
    let class = vm.resolve_class(class_name)?;
    let primitive = vm.primitive_class(base)?;
    let class_object = vm.class_object(primitive)?;
    vm.set_static_value(class, "TYPE", "Ljava/lang/Class;", Value::Ref(class_object));
    Ok(())
}

pub(crate) const INTEGER: oxjvm_vm::NativeClass = class(
    "java/lang/Integer",
    Some("java/lang/Number"),
    &["java/lang/Comparable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("value", "I", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "MIN_VALUE",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(i32::MIN)),
        ),
        field(
            "MAX_VALUE",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(i32::MAX)),
        ),
        field(
            "SIZE",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(32)),
        ),
        field(
            "BYTES",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(4)),
        ),
        field(
            "TYPE",
            "Ljava/lang/Class;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &INTEGER_METHODS,
    Some(integer_clinit),
);

// -------------------------------------------------------------------------------------------
// Long
// -------------------------------------------------------------------------------------------

const LONG_METHODS: [oxjvm_vm::NativeMethodDef; 16] = [
    method("<init>", "(J)V", ACC_PUBLIC, |vm, _, args| {
        write_value_long(vm, receiver(args), long_arg(args, 1));
        void()
    }),
    method("intValue", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_value_long(vm, receiver(args)) as i32)
    }),
    method("longValue", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_value_long(vm, receiver(args)))
    }),
    method("floatValue", "()F", ACC_PUBLIC, |vm, _, args| {
        float(read_value_long(vm, receiver(args)) as f32)
    }),
    method("doubleValue", "()D", ACC_PUBLIC, |vm, _, args| {
        double(read_value_long(vm, receiver(args)) as f64)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = format::long_to_string(read_value_long(vm, receiver(args)));
            object(vm.make_string(&text)?)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        int(
            (read_value_long(vm, receiver(args)) ^ (read_value_long(vm, receiver(args)) >> 32))
                as i32,
        )
    }),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() || vm.class_of(other) != vm.class_of(receiver(args)) {
                return boolean(false);
            }
            boolean(read_value_long(vm, receiver(args)) == read_value_long(vm, other))
        },
    ),
    method(
        "toString",
        "(J)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = format::long_to_string(long_arg(args, 0));
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "parseLong",
        "(Ljava/lang/String;)J",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = string_arg(vm, args[0])?;
            match format::parse_long(&text) {
                Ok(value) => long(value),
                Err(()) => Err(vm.throw_new(
                    "java/lang/NumberFormatException",
                    Some(&alloc::format!("For input string: \"{text}\"")),
                )),
            }
        },
    ),
    method(
        "valueOf",
        "(J)Ljava/lang/Long;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let class = vm.resolve_class("java/lang/Long")?;
            let instance = vm.new_instance(class)?;
            write_value_long(vm, instance, long_arg(args, 0));
            object(instance)
        },
    ),
    method("compare", "(JJ)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        int(long_arg(args, 0).cmp(&long_arg(args, 1)) as i32)
    }),
    method(
        "toHexString",
        "(J)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = format::long_to_radix_string(long_arg(args, 0), 16);
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toBinaryString",
        "(J)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = format::long_to_radix_string(long_arg(args, 0), 2);
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "highestOneBit",
        "(J)J",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let value = long_arg(args, 0);
            long(if value == 0 {
                0
            } else {
                1i64 << (63 - value.leading_zeros())
            })
        },
    ),
    method(
        "lowestOneBit",
        "(J)J",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let value = long_arg(args, 0);
            long(value & value.wrapping_neg())
        },
    ),
];

pub(crate) fn read_value_long(vm: &Vm<'_>, object: ObjectRef) -> i64 {
    match vm.heap.get(object).map(|object| &object.data) {
        Some(oxjvm_vm::ObjectData::Instance(fields)) if !fields.is_empty() => fields[0].as_long(),
        _ => 0,
    }
}

pub(crate) fn write_value_long(vm: &mut Vm<'_>, object: ObjectRef, value: i64) {
    if let Some(class) = vm.heap.get(object).map(|object| object.class) {
        if let Ok((declaring, field)) = vm.find_field(class, "value", "J") {
            let slot = vm.classes.get(declaring).fields[field as usize].slot as usize;
            if let Some(oxjvm_vm::ObjectData::Instance(fields)) =
                vm.heap.get_mut(object).map(|object| &mut object.data)
            {
                fields[slot] = Value::Long(value);
            }
        }
    }
}

fn long_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    set_type_field(vm, "java/lang/Long", oxjvm_classfile::BaseType::Long)
}

pub(crate) const LONG: oxjvm_vm::NativeClass = class(
    "java/lang/Long",
    Some("java/lang/Number"),
    &["java/lang/Comparable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("value", "J", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "MIN_VALUE",
            "J",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Long(i64::MIN)),
        ),
        field(
            "MAX_VALUE",
            "J",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Long(i64::MAX)),
        ),
        field(
            "SIZE",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(64)),
        ),
        field(
            "BYTES",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(8)),
        ),
        field(
            "TYPE",
            "Ljava/lang/Class;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &LONG_METHODS,
    Some(long_clinit),
);

// -------------------------------------------------------------------------------------------
// Short / Byte / Boolean / Character / Float / Double / Void
// -------------------------------------------------------------------------------------------

const SHORT_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method("<init>", "(S)V", ACC_PUBLIC, |vm, _, args| {
        write_value_int(vm, receiver(args), int_arg(args, 1));
        void()
    }),
    method("shortValue", "()S", ACC_PUBLIC, |vm, _, args| {
        int(read_value_int(vm, receiver(args)) as i16 as i32)
    }),
    method("intValue", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_value_int(vm, receiver(args)) as i16 as i32)
    }),
    method("longValue", "()J", ACC_PUBLIC, |vm, _, args| {
        long(i64::from(read_value_int(vm, receiver(args)) as i16))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = format::int_to_string(read_value_int(vm, receiver(args)) as i16 as i32);
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        boxed_int_equals,
    ),
    method(
        "parseShort",
        "(Ljava/lang/String;)S",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = string_arg(vm, args[0])?;
            match format::parse_int(&text) {
                Ok(value) if (-32768..=32767).contains(&value) => int(value),
                _ => Err(vm.throw_new(
                    "java/lang/NumberFormatException",
                    Some(&alloc::format!("For input string: \"{text}\"")),
                )),
            }
        },
    ),
    method(
        "valueOf",
        "(S)Ljava/lang/Short;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let class = vm.resolve_class("java/lang/Short")?;
            let instance = vm.new_instance(class)?;
            write_value_int(vm, instance, int_arg(args, 0));
            object(instance)
        },
    ),
];

pub(crate) const SHORT: oxjvm_vm::NativeClass = class(
    "java/lang/Short",
    Some("java/lang/Number"),
    &["java/lang/Comparable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("value", "S", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "MIN_VALUE",
            "S",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(-32768)),
        ),
        field(
            "MAX_VALUE",
            "S",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(32767)),
        ),
        field(
            "TYPE",
            "Ljava/lang/Class;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &SHORT_METHODS,
    None,
);

const BYTE_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method("<init>", "(B)V", ACC_PUBLIC, |vm, _, args| {
        write_value_int(vm, receiver(args), int_arg(args, 1));
        void()
    }),
    method("byteValue", "()B", ACC_PUBLIC, |vm, _, args| {
        int(read_value_int(vm, receiver(args)) as i8 as i32)
    }),
    method("intValue", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_value_int(vm, receiver(args)) as i8 as i32)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = format::int_to_string(read_value_int(vm, receiver(args)) as i8 as i32);
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        boxed_int_equals,
    ),
    method(
        "valueOf",
        "(B)Ljava/lang/Byte;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let class = vm.resolve_class("java/lang/Byte")?;
            let instance = vm.new_instance(class)?;
            write_value_int(vm, instance, int_arg(args, 0));
            object(instance)
        },
    ),
];

pub(crate) const BYTE: oxjvm_vm::NativeClass = class(
    "java/lang/Byte",
    Some("java/lang/Number"),
    &["java/lang/Comparable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("value", "B", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "MIN_VALUE",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(-128)),
        ),
        field(
            "MAX_VALUE",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(127)),
        ),
        field(
            "TYPE",
            "Ljava/lang/Class;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &BYTE_METHODS,
    None,
);

const BOOLEAN_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method("<init>", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        write_value_int(vm, receiver(args), i32::from(bool_arg(args, 1)));
        void()
    }),
    method("booleanValue", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(read_value_int(vm, receiver(args)) != 0)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(vm.make_string(if read_value_int(vm, receiver(args)) != 0 {
                "true"
            } else {
                "false"
            })?)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        int(if read_value_int(vm, receiver(args)) != 0 {
            1231
        } else {
            1237
        })
    }),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        boxed_int_equals,
    ),
    method(
        "parseBoolean",
        "(Ljava/lang/String;)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let _value = args[0].as_ref();
            boolean(false)
        },
    ),
    method(
        "valueOf",
        "(Z)Ljava/lang/Boolean;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let class = vm.resolve_class("java/lang/Boolean")?;
            let name = if bool_arg(args, 0) { "TRUE" } else { "FALSE" };
            if let Some(existing) = vm.read_static_ref(class, name, "Ljava/lang/Boolean;") {
                return object(existing);
            }
            let instance = vm.new_instance(class)?;
            write_value_int(vm, instance, i32::from(bool_arg(args, 0)));
            vm.set_static_value(class, name, "Ljava/lang/Boolean;", Value::Ref(instance));
            object(instance)
        },
    ),
    method(
        "toString",
        "(Z)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(vm.make_string(if bool_arg(args, 0) { "true" } else { "false" })?),
    ),
];

pub(crate) const BOOLEAN: oxjvm_vm::NativeClass = class(
    "java/lang/Boolean",
    Some("java/lang/Object"),
    &["java/io/Serializable", "java/lang/Comparable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("value", "Z", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "TRUE",
            "Ljava/lang/Boolean;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "FALSE",
            "Ljava/lang/Boolean;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "TYPE",
            "Ljava/lang/Class;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &BOOLEAN_METHODS,
    None,
);

const CHARACTER_METHODS: [oxjvm_vm::NativeMethodDef; 17] = [
    method("<init>", "(C)V", ACC_PUBLIC, |vm, _, args| {
        write_value_int(vm, receiver(args), int_arg(args, 1));
        void()
    }),
    method("charValue", "()C", ACC_PUBLIC, |vm, _, args| {
        int(read_value_int(vm, receiver(args)) & 0xFFFF)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let unit = read_value_int(vm, receiver(args)) as u32 & 0xFFFF;
            let text = char::from_u32(unit).map_or_else(String::new, |ch| ch.to_string());
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        boxed_int_equals,
    ),
    method("isDigit", "(C)Z", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        boolean(
            char::from_u32(int_arg(args, 0) as u32 & 0xFFFF).is_some_and(|ch| ch.is_ascii_digit()),
        )
    }),
    method("isLetter", "(C)Z", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        boolean(char::from_u32(int_arg(args, 0) as u32 & 0xFFFF).is_some_and(char::is_alphabetic))
    }),
    method(
        "isLetterOrDigit",
        "(C)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            boolean(
                char::from_u32(int_arg(args, 0) as u32 & 0xFFFF).is_some_and(char::is_alphanumeric),
            )
        },
    ),
    method(
        "isWhitespace",
        "(C)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            boolean(
                char::from_u32(int_arg(args, 0) as u32 & 0xFFFF)
                    .is_some_and(|ch| ch.is_whitespace() || ch == '\u{00A0}' && false),
            )
        },
    ),
    method(
        "isUpperCase",
        "(C)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            boolean(
                char::from_u32(int_arg(args, 0) as u32 & 0xFFFF).is_some_and(char::is_uppercase),
            )
        },
    ),
    method(
        "isLowerCase",
        "(C)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            boolean(
                char::from_u32(int_arg(args, 0) as u32 & 0xFFFF).is_some_and(char::is_lowercase),
            )
        },
    ),
    method(
        "toUpperCase",
        "(C)C",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let ch = char::from_u32(int_arg(args, 0) as u32 & 0xFFFF).unwrap_or('\u{FFFD}');
            let upper: String = ch.to_uppercase().collect();
            int(upper.chars().next().map_or(0, |c| c as i32))
        },
    ),
    method(
        "toLowerCase",
        "(C)C",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let ch = char::from_u32(int_arg(args, 0) as u32 & 0xFFFF).unwrap_or('\u{FFFD}');
            let lower: String = ch.to_lowercase().collect();
            int(lower.chars().next().map_or(0, |c| c as i32))
        },
    ),
    method("digit", "(CI)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        let ch = int_arg(args, 0) as u32 & 0xFFFF;
        let radix = int_arg(args, 1);
        let value = char::from_u32(ch)
            .and_then(|c| c.to_digit(radix as u32))
            .map_or(-1, |d| d as i32);
        int(value)
    }),
    method(
        "forDigit",
        "(II)C",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let digit = int_arg(args, 0);
            let radix = int_arg(args, 1);
            int(char::from_digit(digit as u32, radix.clamp(2, 36) as u32).map_or(0, |ch| ch as i32))
        },
    ),
    method(
        "valueOf",
        "(C)Ljava/lang/Character;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let class = vm.resolve_class("java/lang/Character")?;
            let instance = vm.new_instance(class)?;
            write_value_int(vm, instance, int_arg(args, 0));
            object(instance)
        },
    ),
    method("hashCode", "(C)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        int(int_arg(args, 0))
    }),
    method(
        "toString",
        "(C)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let unit = int_arg(args, 0) as u32 & 0xFFFF;
            let text = char::from_u32(unit).map_or_else(String::new, |ch| ch.to_string());
            object(vm.make_string(&text)?)
        },
    ),
];

pub(crate) const CHARACTER: oxjvm_vm::NativeClass = class(
    "java/lang/Character",
    Some("java/lang/Object"),
    &["java/io/Serializable", "java/lang/Comparable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("value", "C", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "MIN_VALUE",
            "C",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(0)),
        ),
        field(
            "MAX_VALUE",
            "C",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(65535)),
        ),
        field(
            "TYPE",
            "Ljava/lang/Class;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &CHARACTER_METHODS,
    None,
);

const FLOAT_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method("<init>", "(F)V", ACC_PUBLIC, |vm, _, args| {
        write_value_int(
            vm,
            receiver(args),
            crate::float_arg(args, 1).to_bits() as i32,
        );
        void()
    }),
    method("floatValue", "()F", ACC_PUBLIC, |vm, _, args| {
        float(f32::from_bits(read_value_int(vm, receiver(args)) as u32))
    }),
    method("doubleValue", "()D", ACC_PUBLIC, |vm, _, args| {
        double(f64::from(f32::from_bits(
            read_value_int(vm, receiver(args)) as u32,
        )))
    }),
    method("intValue", "()I", ACC_PUBLIC, |vm, _, args| {
        int(f32::from_bits(read_value_int(vm, receiver(args)) as u32) as i32)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text =
                format::float_to_string(f32::from_bits(read_value_int(vm, receiver(args)) as u32));
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() || vm.class_of(other) != vm.class_of(receiver(args)) {
                return boolean(false);
            }
            let a = f32::from_bits(read_value_int(vm, receiver(args)) as u32);
            let b = f32::from_bits(read_value_int(vm, other) as u32);
            boolean(a.to_bits() == b.to_bits())
        },
    ),
    method("isNaN", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(f32::from_bits(read_value_int(vm, receiver(args)) as u32).is_nan())
    }),
    method(
        "toString",
        "(F)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = format::float_to_string(crate::float_arg(args, 0));
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "parseFloat",
        "(Ljava/lang/String;)F",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = string_arg(vm, args[0])?;
            match format::parse_float(&text) {
                Ok(value) => float(value),
                Err(()) => Err(vm.throw_new(
                    "java/lang/NumberFormatException",
                    Some(&alloc::format!("For input string: \"{text}\"")),
                )),
            }
        },
    ),
    method(
        "floatToIntBits",
        "(F)I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let value = crate::float_arg(args, 0);
            int(if value.is_nan() {
                0x7FC0_0000u32 as i32
            } else {
                value.to_bits() as i32
            })
        },
    ),
    method(
        "intBitsToFloat",
        "(I)F",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| float(f32::from_bits(int_arg(args, 0) as u32)),
    ),
    method("isNaN", "(F)Z", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        boolean(crate::float_arg(args, 0).is_nan())
    }),
];

pub(crate) const FLOAT: oxjvm_vm::NativeClass = class(
    "java/lang/Float",
    Some("java/lang/Number"),
    &["java/lang/Comparable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("value", "F", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "NaN",
            "F",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Float(f32::NAN)),
        ),
        field(
            "POSITIVE_INFINITY",
            "F",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Float(f32::INFINITY)),
        ),
        field(
            "NEGATIVE_INFINITY",
            "F",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Float(f32::NEG_INFINITY)),
        ),
        field(
            "MAX_VALUE",
            "F",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Float(f32::MAX)),
        ),
        field(
            "MIN_VALUE",
            "F",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Float(f32::MIN_POSITIVE)),
        ),
        field(
            "TYPE",
            "Ljava/lang/Class;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &FLOAT_METHODS,
    None,
);

const DOUBLE_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method("<init>", "(D)V", ACC_PUBLIC, |vm, _, args| {
        write_value_long(
            vm,
            receiver(args),
            crate::double_arg(args, 1).to_bits() as i64,
        );
        void()
    }),
    method("doubleValue", "()D", ACC_PUBLIC, |vm, _, args| {
        double(f64::from_bits(read_value_long(vm, receiver(args)) as u64))
    }),
    method("floatValue", "()F", ACC_PUBLIC, |vm, _, args| {
        float(f64::from_bits(read_value_long(vm, receiver(args)) as u64) as f32)
    }),
    method("intValue", "()I", ACC_PUBLIC, |vm, _, args| {
        int(f64::from_bits(read_value_long(vm, receiver(args)) as u64) as i32)
    }),
    method("longValue", "()J", ACC_PUBLIC, |vm, _, args| {
        long(f64::from_bits(read_value_long(vm, receiver(args)) as u64) as i64)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = format::double_to_string(f64::from_bits(
                read_value_long(vm, receiver(args)) as u64,
            ));
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() || vm.class_of(other) != vm.class_of(receiver(args)) {
                return boolean(false);
            }
            boolean(
                (read_value_long(vm, receiver(args)) as u64) == (read_value_long(vm, other) as u64),
            )
        },
    ),
    method(
        "toString",
        "(D)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = format::double_to_string(crate::double_arg(args, 0));
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "parseDouble",
        "(Ljava/lang/String;)D",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = string_arg(vm, args[0])?;
            match format::parse_double(&text) {
                Ok(value) => double(value),
                Err(()) => Err(vm.throw_new(
                    "java/lang/NumberFormatException",
                    Some(&alloc::format!("For input string: \"{text}\"")),
                )),
            }
        },
    ),
    method(
        "doubleToLongBits",
        "(D)J",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let value = crate::double_arg(args, 0);
            long(if value.is_nan() {
                0x7FF8_0000_0000_0000u64 as i64
            } else {
                value.to_bits() as i64
            })
        },
    ),
    method(
        "doubleToRawLongBits",
        "(D)J",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| long(crate::double_arg(args, 0).to_bits() as i64),
    ),
    method(
        "longBitsToDouble",
        "(J)D",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| double(f64::from_bits(long_arg(args, 0) as u64)),
    ),
];

pub(crate) const DOUBLE: oxjvm_vm::NativeClass = class(
    "java/lang/Double",
    Some("java/lang/Number"),
    &["java/lang/Comparable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("value", "D", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "NaN",
            "D",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Double(f64::NAN)),
        ),
        field(
            "POSITIVE_INFINITY",
            "D",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Double(f64::INFINITY)),
        ),
        field(
            "NEGATIVE_INFINITY",
            "D",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Double(f64::NEG_INFINITY)),
        ),
        field(
            "MAX_VALUE",
            "D",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Double(f64::MAX)),
        ),
        field(
            "MIN_VALUE",
            "D",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Double(f64::MIN_POSITIVE)),
        ),
        field(
            "TYPE",
            "Ljava/lang/Class;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &DOUBLE_METHODS,
    None,
);

pub(crate) const VOID: oxjvm_vm::NativeClass = class(
    "java/lang/Void",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[field(
        "TYPE",
        "Ljava/lang/Class;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    )],
    &[],
    None,
);
