//! The boxed primitive classes and `Number`.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError, format};

use crate::{
    bool_arg, boolean, class, double, field, float, int, int_arg, long, long_arg, method,
    method_throws, object, receiver, ref_arg, string_arg, void,
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

/// `Integer.compress`/`Long.compress`: pack the mask-selected bits downward.
fn compress_bits(value: u64, mask: u64) -> u64 {
    let mut out = 0u64;
    let mut bit = 0u32;
    for index in 0..64 {
        if mask & (1u64 << index) != 0 {
            if value & (1u64 << index) != 0 {
                out |= 1u64 << bit;
            }
            bit += 1;
        }
    }
    out
}

/// `Integer.expand`/`Long.expand`: spread the low bits into the mask positions.
fn expand_bits(value: u64, mask: u64) -> u64 {
    let mut out = 0u64;
    let mut bit = 0u32;
    for index in 0..64 {
        if mask & (1u64 << index) != 0 {
            if value & (1u64 << bit) != 0 {
                out |= 1u64 << index;
            }
            bit += 1;
        }
    }
    out
}

/// Java `Character.isSpaceChar`: only Unicode Zs/Zl/Zp categories (not tab/newline).
fn is_space_char_code(code_point: u32) -> bool {
    matches!(
        code_point,
        0x20 | 0xA0 | 0x1680 | 0x2000..=0x200A | 0x2028 | 0x2029 | 0x202F | 0x205F | 0x3000
    )
}

const INTEGER_METHODS: [oxjvm_vm::NativeMethodDef; 52] = [
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
            int(compare_ints(a, b))
        },
    ),
    method(
        "compareTo",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC | ACC_BRIDGE | ACC_SYNTHETIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            if other.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            if vm.class_of(this) != vm.class_of(other) {
                return Err(vm.throw_new("java/lang/ClassCastException", None));
            }
            let a = read_value_int(vm, this);
            let b = read_value_int(vm, other);
            int(compare_ints(a, b))
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
    method_throws(
        "parseInt",
        "(Ljava/lang/String;)I",
        ACC_PUBLIC | ACC_STATIC,
        parse_int_10,
        &["java/lang/NumberFormatException"],
    ),
    method_throws(
        "parseInt",
        "(Ljava/lang/String;I)I",
        ACC_PUBLIC | ACC_STATIC,
        parse_int_radix,
        &["java/lang/NumberFormatException"],
    ),
    method_throws(
        "parseInt",
        "(Ljava/lang/CharSequence;III)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = charsequence_text(vm, ref_arg(args, 0))?;
            let begin = int_arg(args, 1);
            let end = int_arg(args, 2);
            let radix = int_arg(args, 3);
            let sub = charsequence_sub(vm, &text, begin, end)?;
            match format::parse_int_radix(&sub, radix as u32) {
                Some(value) => int(value),
                None => Err(vm.throw_new(
                    "java/lang/NumberFormatException",
                    Some(&alloc::format!("For input string: \"{sub}\"")),
                )),
            }
        },
        &["java/lang/NumberFormatException"],
    ),
    method(
        "parseUnsignedInt",
        "(Ljava/lang/String;)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = parse_string_arg(vm, args[0])?;
            match format::parse_uint_radix(&text, 10) {
                Some(value) => int(value as i32),
                None => Err(vm.throw_new(
                    "java/lang/NumberFormatException",
                    Some(&alloc::format!("For input string: \"{text}\"")),
                )),
            }
        },
    ),
    method(
        "parseUnsignedInt",
        "(Ljava/lang/String;I)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = parse_string_arg(vm, args[0])?;
            let radix = int_arg(args, 1);
            match format::parse_uint_radix(&text, radix as u32) {
                Some(value) => int(value as i32),
                None => Err(vm.throw_new(
                    "java/lang/NumberFormatException",
                    Some(&alloc::format!("For input string: \"{text}\"")),
                )),
            }
        },
    ),
    method(
        "parseUnsignedInt",
        "(Ljava/lang/CharSequence;III)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = charsequence_text(vm, ref_arg(args, 0))?;
            let begin = int_arg(args, 1);
            let end = int_arg(args, 2);
            let radix = int_arg(args, 3);
            let sub = charsequence_sub(vm, &text, begin, end)?;
            match format::parse_uint_radix(&sub, radix as u32) {
                Some(value) => int(value as i32),
                None => Err(vm.throw_new(
                    "java/lang/NumberFormatException",
                    Some(&alloc::format!("For input string: \"{sub}\"")),
                )),
            }
        },
    ),
    method(
        "toUnsignedLong",
        "(I)J",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| long(i64::from(int_arg(args, 0) as u32)),
    ),
    method(
        "toUnsignedString",
        "(I)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = alloc::format!("{}", int_arg(args, 0) as u32);
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toUnsignedString",
        "(II)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = unsigned_radix_string(int_arg(args, 0) as u32 as u64, int_arg(args, 1));
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "valueOf",
        "(I)Ljava/lang/Integer;",
        ACC_PUBLIC | ACC_STATIC,
        boxed_int_value_of,
    ),
    method(
        "decode",
        "(Ljava/lang/String;)Ljava/lang/Integer;",
        ACC_PUBLIC | ACC_STATIC,
        boxed_integer_decode,
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
    method("signum", "(I)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        int(int_arg(args, 0).signum())
    }),
    method(
        "reverseBytes",
        "(I)I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| int(int_arg(args, 0).swap_bytes()),
    ),
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
            let text = format::ulong_to_radix_string(int_arg(args, 0) as u32 as u64, 16);
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toBinaryString",
        "(I)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = format::ulong_to_radix_string(int_arg(args, 0) as u32 as u64, 2);
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toOctalString",
        "(I)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = format::ulong_to_radix_string(int_arg(args, 0) as u32 as u64, 8);
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "getInteger",
        "(Ljava/lang/String;)Ljava/lang/Integer;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| integer_get_integer(vm, args, Value::Ref(ObjectRef::NULL)),
    ),
    method(
        "getInteger",
        "(Ljava/lang/String;I)Ljava/lang/Integer;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let default = boxed_int(vm, int_arg(args, 1))?;
            integer_get_integer(vm, args, default)
        },
    ),
    method("max", "(II)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        int(int_arg(args, 0).max(int_arg(args, 1)))
    }),
    method("min", "(II)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        int(int_arg(args, 0).min(int_arg(args, 1)))
    }),
    method("sum", "(II)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        int(int_arg(args, 0).wrapping_add(int_arg(args, 1)))
    }),
    method("bitCount", "(I)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        int(int_arg(args, 0).count_ones() as i32)
    }),
    method("reverse", "(I)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        int(int_arg(args, 0).reverse_bits())
    }),
    method(
        "rotateLeft",
        "(II)I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| int(int_arg(args, 0).rotate_left(int_arg(args, 1) as u32)),
    ),
    method(
        "rotateRight",
        "(II)I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| int(int_arg(args, 0).rotate_right(int_arg(args, 1) as u32)),
    ),
    method(
        "numberOfLeadingZeros",
        "(I)I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| int(int_arg(args, 0).leading_zeros() as i32),
    ),
    method(
        "numberOfTrailingZeros",
        "(I)I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| int(int_arg(args, 0).trailing_zeros() as i32),
    ),
    method(
        "highestOneBit",
        "(I)I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let value = int_arg(args, 0) as u32;
            if value == 0 {
                return int(0);
            }
            int((1u32 << (31 - value.leading_zeros())) as i32)
        },
    ),
    method(
        "lowestOneBit",
        "(I)I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let value = int_arg(args, 0);
            int(value & value.wrapping_neg())
        },
    ),
    method(
        "divideUnsigned",
        "(II)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let divisor = int_arg(args, 1) as u32;
            if divisor == 0 {
                return Err(vm.throw_new("java/lang/ArithmeticException", Some("/ by zero")));
            }
            int(((int_arg(args, 0) as u32) / divisor) as i32)
        },
    ),
    method(
        "remainderUnsigned",
        "(II)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let divisor = int_arg(args, 1) as u32;
            if divisor == 0 {
                return Err(vm.throw_new("java/lang/ArithmeticException", Some("/ by zero")));
            }
            int(((int_arg(args, 0) as u32) % divisor) as i32)
        },
    ),
    method(
        "compress",
        "(II)I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let value = int_arg(args, 0) as u32;
            let mask = int_arg(args, 1) as u32;
            int(compress_bits(u64::from(value), u64::from(mask)) as i32)
        },
    ),
    method("expand", "(II)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        let value = int_arg(args, 0) as u32;
        let mask = int_arg(args, 1) as u32;
        int(expand_bits(u64::from(value), u64::from(mask)) as i32)
    }),
];

/// `Integer.getInteger`: the system property parsed as an int, or the default.
fn integer_get_integer(vm: &mut Vm<'_>, args: &[Value], default: Value) -> Result<Value, VmError> {
    // The JDK catches NullPointerException from System.getProperty and returns
    // the default, so a null name yields the default rather than throwing.
    if args[0].as_ref().is_null() {
        return Ok(default);
    }
    let key = string_arg(vm, args[0])?;
    if let Some(text) = crate::lang::system_property(vm, &key) {
        if let Some(value) = format::parse_int(&text) {
            return boxed_int(vm, value);
        }
    }
    Ok(default)
}

/// Box `value` for a field/method descriptor (`I`/`J`/`Z`/`C`/`S`/`B`/`F`/`D`;
/// reference descriptors pass the value through).
pub(crate) fn box_value(vm: &mut Vm<'_>, value: Value, descriptor: &str) -> Result<Value, VmError> {
    let class_name = match descriptor {
        "Z" => "java/lang/Boolean",
        "B" => "java/lang/Byte",
        "C" => "java/lang/Character",
        "S" => "java/lang/Short",
        "I" => "java/lang/Integer",
        "J" => "java/lang/Long",
        "F" => "java/lang/Float",
        "D" => "java/lang/Double",
        _ => return Ok(value),
    };
    if descriptor == "Z" {
        let class = vm.resolve_class(class_name)?;
        vm.ensure_initialized(class)?;
        let name = if value.as_int() != 0 { "TRUE" } else { "FALSE" };
        if let Some(existing) = vm.read_static_ref_if_set(class, name, "Ljava/lang/Boolean;") {
            return object(existing);
        }
        let instance = vm.new_instance(class)?;
        write_value_int(vm, instance, value.as_int());
        vm.set_static_value(class, name, "Ljava/lang/Boolean;", Value::Ref(instance));
        return object(instance);
    }
    // Float and Double are stored as their raw bits and are never cached. The
    // value may arrive either as a typed Float/Double (reflection) or already
    // as its bit pattern in an Int/Long (var handles, streams, beans).
    if descriptor == "F" {
        let bits = match value {
            Value::Float(f) => f.to_bits() as i32,
            other => other.as_int(),
        };
        let class = vm.resolve_class(class_name)?;
        let instance = vm.new_instance(class)?;
        write_value_int(vm, instance, bits);
        return object(instance);
    }
    if descriptor == "D" {
        let bits = match value {
            Value::Double(d) => d.to_bits() as i64,
            other => other.as_long(),
        };
        let class = vm.resolve_class(class_name)?;
        let instance = vm.new_instance(class)?;
        write_value_long(vm, instance, bits);
        return object(instance);
    }
    let tag = descriptor.as_bytes()[0];
    let key = match descriptor {
        "J" | "D" => value.as_long(),
        _ => i64::from(value.as_int()),
    };
    if cacheable(tag, key) {
        if let Some(cached) = vm.cached_boxed(tag, key) {
            return object(cached);
        }
    }
    let class = vm.resolve_class(class_name)?;
    let instance = vm.new_instance(class)?;
    match descriptor {
        "J" | "D" => write_value_long(vm, instance, value.as_long()),
        _ => write_value_int(vm, instance, value.as_int()),
    }
    if cacheable(tag, key) {
        vm.cache_boxed(tag, key, instance);
    }
    object(instance)
}

/// Whether the wrapper class for `tag` caches `key` (the JDK's identity guarantees).
fn cacheable(tag: u8, key: i64) -> bool {
    match tag {
        b'B' => (-128..=127).contains(&key),
        b'C' => (0..=127).contains(&key),
        b'S' | b'I' | b'J' => (-128..=127).contains(&key),
        _ => false,
    }
}

/// Box an `int`-sized primitive, reusing the cached instance where the JDK does.
fn box_integer_like(
    vm: &mut Vm<'_>,
    class_name: &str,
    tag: u8,
    value: i32,
) -> Result<Value, VmError> {
    let key = i64::from(value);
    if cacheable(tag, key) {
        if let Some(cached) = vm.cached_boxed(tag, key) {
            return object(cached);
        }
    }
    let class = vm.resolve_class(class_name)?;
    let instance = vm.new_instance(class)?;
    write_value_int(vm, instance, value);
    if cacheable(tag, key) {
        vm.cache_boxed(tag, key, instance);
    }
    object(instance)
}

/// Unbox `reference` per `descriptor` (reference descriptors pass through).
pub(crate) fn unbox_value(vm: &Vm<'_>, reference: ObjectRef, descriptor: &str) -> Value {
    match descriptor {
        "J" | "D" => Value::Long(read_value_long(vm, reference)),
        "Z" | "B" | "C" | "S" | "I" | "F" => Value::Int(read_value_int(vm, reference)),
        _ => Value::Ref(reference),
    }
}

/// Unbox a boxed primitive for a reflective call (`Method.invoke`, `Constructor.newInstance`,
/// `Field.set`, `Array.set`), applying JLS 5.1.2 widening primitive conversion.
///
/// A boxed value may be widened to a larger primitive type (e.g. `Integer` -> `long`), but
/// never narrowed. Reference parameters pass through unchanged (including `null`). A `null`
/// or non-box argument for a primitive parameter, or a type mismatch, throws
/// `IllegalArgumentException`, matching the reference implementation.
pub(crate) fn unbox_call_argument(
    vm: &mut Vm<'_>,
    reference: ObjectRef,
    descriptor: &str,
) -> Result<Value, VmError> {
    if !matches!(descriptor, "Z" | "B" | "C" | "S" | "I" | "J" | "F" | "D") {
        return Ok(Value::Ref(reference));
    }
    if reference.is_null() {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("argument type mismatch"),
        ));
    }
    let name = vm.class_name(vm.class_of(reference));
    let (source, int_bits, long_bits) = match name {
        "java/lang/Boolean" => ('Z', read_value_int(vm, reference), 0i64),
        "java/lang/Byte" => ('B', read_value_int(vm, reference), 0),
        "java/lang/Character" => ('C', read_value_int(vm, reference), 0),
        "java/lang/Short" => ('S', read_value_int(vm, reference), 0),
        "java/lang/Integer" => ('I', read_value_int(vm, reference), 0),
        "java/lang/Long" => ('J', 0, read_value_long(vm, reference)),
        "java/lang/Float" => ('F', read_value_int(vm, reference), 0),
        "java/lang/Double" => ('D', 0, read_value_long(vm, reference)),
        _ => {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("argument type mismatch"),
            ));
        }
    };
    let widened = match descriptor {
        "Z" => (source == 'Z').then_some(Value::Int(int_bits)),
        "B" => (source == 'B').then_some(Value::Int(int_bits)),
        "C" => (source == 'C').then_some(Value::Int(int_bits)),
        "S" => matches!(source, 'B' | 'S').then_some(Value::Int(int_bits)),
        "I" => matches!(source, 'B' | 'C' | 'S' | 'I').then_some(Value::Int(int_bits)),
        "J" => match source {
            'B' | 'C' | 'S' | 'I' => Some(Value::Long(i64::from(int_bits))),
            'J' => Some(Value::Long(long_bits)),
            _ => None,
        },
        "F" => match source {
            'B' | 'C' | 'S' | 'I' => Some(Value::Float(int_bits as f32)),
            'J' => Some(Value::Float(long_bits as f32)),
            'F' => Some(Value::Float(f32::from_bits(int_bits as u32))),
            _ => None,
        },
        "D" => match source {
            'B' | 'C' | 'S' | 'I' => Some(Value::Double(f64::from(int_bits))),
            'J' => Some(Value::Double(long_bits as f64)),
            'F' => Some(Value::Double(f64::from(f32::from_bits(int_bits as u32)))),
            'D' => Some(Value::Double(f64::from_bits(long_bits as u64))),
            _ => None,
        },
        _ => None,
    };
    widened.ok_or_else(|| {
        vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("argument type mismatch"),
        )
    })
}

pub(crate) fn boxed_int(vm: &mut Vm<'_>, value: i32) -> Result<Value, VmError> {
    box_integer_like(vm, "java/lang/Integer", b'I', value)
}

fn boxed_int_init(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    write_value_int(vm, receiver(args), int_arg(args, 1));
    void()
}

fn compare_ints(a: i32, b: i32) -> i32 {
    if a < b {
        -1
    } else if a > b {
        1
    } else {
        0
    }
}

fn compare_longs(a: i64, b: i64) -> i32 {
    if a < b {
        -1
    } else if a > b {
        1
    } else {
        0
    }
}

/// JDK `Float.compare`: numeric order first, then the canonical bit pattern as a tie-break so
/// that NaN equals NaN and outranks every value while -0.0 sorts below +0.0.
fn compare_floats(a: f32, b: f32) -> i32 {
    if a < b {
        return -1;
    }
    if a > b {
        return 1;
    }
    let ab = if a.is_nan() {
        0x7FC0_0000u32 as i32
    } else {
        a.to_bits() as i32
    };
    let bb = if b.is_nan() {
        0x7FC0_0000u32 as i32
    } else {
        b.to_bits() as i32
    };
    ab.cmp(&bb) as i32
}

/// JDK `Double.compare`: see `compare_floats`.
pub(crate) fn compare_doubles(a: f64, b: f64) -> i32 {
    if a < b {
        return -1;
    }
    if a > b {
        return 1;
    }
    let ab = if a.is_nan() {
        0x7FF8_0000_0000_0000u64 as i64
    } else {
        a.to_bits() as i64
    };
    let bb = if b.is_nan() {
        0x7FF8_0000_0000_0000u64 as i64
    } else {
        b.to_bits() as i64
    };
    ab.cmp(&bb) as i32
}

/// The `value` field of a boxed primitive, whose descriptor varies by class
/// (`I`/`Z`/`C`/`S`/`B`/`F`).
pub(crate) fn read_value_int(vm: &Vm<'_>, object: ObjectRef) -> i32 {
    let class = vm.class_of(object);
    for descriptor in ["I", "Z", "C", "S", "B", "F"] {
        if let Some(value) = vm.read_instance_int_named(object, class, "value", descriptor) {
            return value;
        }
    }
    0
}

pub(crate) fn write_value_int(vm: &mut Vm<'_>, object: ObjectRef, value: i32) {
    let class = vm.class_of(object);
    for descriptor in ["I", "Z", "C", "S", "B", "F"] {
        if let Some((declaring, field)) = vm.find_field_opt(class, "value", descriptor) {
            vm.set_instance_int(object, declaring, field, value);
            return;
        }
    }
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

/// The text of a `CharSequence` argument via its `toString()`.
fn charsequence_text(vm: &mut Vm<'_>, reference: ObjectRef) -> Result<String, VmError> {
    if reference.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let (class, method) =
        vm.resolve_virtual_method(reference, "toString", "()Ljava/lang/String;")?;
    let result = vm.invoke_method(class, method, vec![Value::Ref(reference)])?;
    Ok(vm.string_value(result.as_ref()).unwrap_or_default())
}

/// The `[begin, end)` subsequence of a `CharSequence` text, validating bounds.
fn charsequence_sub(vm: &mut Vm<'_>, text: &str, begin: i32, end: i32) -> Result<String, VmError> {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len() as i32;
    if begin < 0 || end > len || begin > end {
        return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
    }
    Ok(chars[begin as usize..end as usize].iter().collect())
}

/// The text of a `String` argument for numeric parsing: a null string is a
/// `NumberFormatException` ("Cannot parse null string"), as the JDK requires.
fn parse_string_arg(vm: &mut Vm<'_>, value: Value) -> Result<String, VmError> {
    if value.as_ref().is_null() {
        return Err(vm.throw_new(
            "java/lang/NumberFormatException",
            Some("Cannot parse null string"),
        ));
    }
    string_arg(vm, value)
}

fn parse_int_10(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = parse_string_arg(vm, args[0])?;
    match format::parse_int(&text) {
        Some(value) => int(value),
        None => Err(vm.throw_new(
            "java/lang/NumberFormatException",
            Some(&alloc::format!("For input string: \"{text}\"")),
        )),
    }
}

/// The shared numeric part of `Integer.decode`/`Long.decode`: sign, radix prefix and digits.
fn decode_integer(text: &str) -> Option<i128> {
    let (negative, rest) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (radix, digits) = if let Some(rest) = rest
        .strip_prefix("0x")
        .or_else(|| rest.strip_prefix("0X"))
        .or_else(|| rest.strip_prefix('#'))
    {
        (16u32, rest)
    } else if let Some(rest) = rest.strip_prefix('0') {
        if rest.is_empty() {
            (10u32, "0")
        } else {
            (8u32, rest)
        }
    } else {
        (10u32, rest)
    };
    // A sign is only permitted at the very start (before any radix prefix);
    // `from_str_radix` would otherwise accept `0x+10`/`0x-10`.
    if digits.starts_with('+') || digits.starts_with('-') {
        return None;
    }
    let magnitude = u128::from_str_radix(digits, radix).ok()?;
    Some(if negative {
        -(magnitude as i128)
    } else {
        magnitude as i128
    })
}

fn boxed_integer_decode(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = string_arg(vm, args[0])?;
    match decode_integer(&text).and_then(|value| i32::try_from(value).ok()) {
        Some(value) => boxed_int(vm, value),
        None => Err(vm.throw_new(
            "java/lang/NumberFormatException",
            Some(&alloc::format!("For input string: \"{text}\"")),
        )),
    }
}

fn boxed_long_decode(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = string_arg(vm, args[0])?;
    match decode_integer(&text).and_then(|value| i64::try_from(value).ok()) {
        Some(value) => boxed_long(vm, value),
        None => Err(vm.throw_new(
            "java/lang/NumberFormatException",
            Some(&alloc::format!("For input string: \"{text}\"")),
        )),
    }
}

fn boxed_short_decode(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = string_arg(vm, args[0])?;
    match decode_integer(&text).and_then(|value| i16::try_from(value).ok()) {
        Some(value) => box_integer_like(vm, "java/lang/Short", b'S', i32::from(value)),
        None => Err(vm.throw_new(
            "java/lang/NumberFormatException",
            Some(&alloc::format!("For input string: \"{text}\"")),
        )),
    }
}

fn boxed_byte_decode(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = string_arg(vm, args[0])?;
    match decode_integer(&text).and_then(|value| i8::try_from(value).ok()) {
        Some(value) => box_integer_like(vm, "java/lang/Byte", b'B', i32::from(value)),
        None => Err(vm.throw_new(
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
    let text = parse_string_arg(vm, args[0])?;
    let radix = int_arg(args, 1);
    match format::parse_int_radix(&text, radix as u32) {
        Some(value) => int(value),
        None => Err(vm.throw_new(
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
    boxed_int(vm, int_arg(args, 0))
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

fn short_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    set_type_field(vm, "java/lang/Short", oxjvm_classfile::BaseType::Short)
}

fn byte_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    set_type_field(vm, "java/lang/Byte", oxjvm_classfile::BaseType::Byte)
}

fn boolean_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    set_type_field(vm, "java/lang/Boolean", oxjvm_classfile::BaseType::Boolean)?;
    let class = vm.resolve_class("java/lang/Boolean")?;
    for (name, value) in [("TRUE", true), ("FALSE", false)] {
        let instance = vm.new_instance(class)?;
        write_value_int(vm, instance, i32::from(value));
        vm.set_static_value(class, name, "Ljava/lang/Boolean;", Value::Ref(instance));
    }
    Ok(())
}

fn character_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    set_type_field(vm, "java/lang/Character", oxjvm_classfile::BaseType::Char)
}

fn float_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    set_type_field(vm, "java/lang/Float", oxjvm_classfile::BaseType::Float)
}

fn double_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    set_type_field(vm, "java/lang/Double", oxjvm_classfile::BaseType::Double)
}

fn void_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    set_type_field(vm, "java/lang/Void", oxjvm_classfile::BaseType::Void)
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

/// The unsigned radix representation of a 64-bit value, the way `Long.toUnsignedString`
/// and `Integer.toUnsignedString` write it.
fn unsigned_radix_string(mut value: u64, radix: i32) -> String {
    let radix = if (2..=36).contains(&radix) {
        radix as u64
    } else {
        10
    };
    if radix == 10 {
        return alloc::format!("{value}");
    }
    if value == 0 {
        return String::from("0");
    }
    let mut digits = Vec::new();
    while value != 0 {
        let digit = (value % radix) as u32;
        digits.push(char::from_digit(digit, radix as u32).unwrap_or('0'));
        value /= radix;
    }
    digits.iter().rev().collect()
}

const LONG_METHODS: [oxjvm_vm::NativeMethodDef; 51] = [
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
        "toString",
        "(JI)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let radix = int_arg(args, 1).clamp(2, 36) as u32;
            let text = format::long_to_radix_string(long_arg(args, 0), radix);
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "parseLong",
        "(Ljava/lang/String;)J",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| long(parse_long_arg(vm, args[0])?),
    ),
    method(
        "parseLong",
        "(Ljava/lang/CharSequence;III)J",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = charsequence_text(vm, ref_arg(args, 0))?;
            let begin = int_arg(args, 1);
            let end = int_arg(args, 2);
            let radix = int_arg(args, 3);
            let sub = charsequence_sub(vm, &text, begin, end)?;
            match format::parse_long_radix(&sub, radix as u32) {
                Some(value) => long(value),
                None => Err(vm.throw_new(
                    "java/lang/NumberFormatException",
                    Some(&alloc::format!("For input string: \"{sub}\"")),
                )),
            }
        },
    ),
    method(
        "parseUnsignedLong",
        "(Ljava/lang/String;)J",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = parse_string_arg(vm, args[0])?;
            match format::parse_ulong_radix(&text, 10) {
                Some(value) => long(value as i64),
                None => Err(vm.throw_new(
                    "java/lang/NumberFormatException",
                    Some(&alloc::format!("For input string: \"{text}\"")),
                )),
            }
        },
    ),
    method(
        "parseUnsignedLong",
        "(Ljava/lang/String;I)J",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = parse_string_arg(vm, args[0])?;
            let radix = int_arg(args, 1);
            match format::parse_ulong_radix(&text, radix as u32) {
                Some(value) => long(value as i64),
                None => Err(vm.throw_new(
                    "java/lang/NumberFormatException",
                    Some(&alloc::format!("For input string: \"{text}\"")),
                )),
            }
        },
    ),
    method(
        "parseUnsignedLong",
        "(Ljava/lang/CharSequence;III)J",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = charsequence_text(vm, ref_arg(args, 0))?;
            let begin = int_arg(args, 1);
            let end = int_arg(args, 2);
            let radix = int_arg(args, 3);
            let sub = charsequence_sub(vm, &text, begin, end)?;
            match format::parse_ulong_radix(&sub, radix as u32) {
                Some(value) => long(value as i64),
                None => Err(vm.throw_new(
                    "java/lang/NumberFormatException",
                    Some(&alloc::format!("For input string: \"{sub}\"")),
                )),
            }
        },
    ),
    method(
        "compareUnsigned",
        "(JJ)I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| int((long_arg(args, 0) as u64).cmp(&(long_arg(args, 1) as u64)) as i32),
    ),
    method(
        "toOctalString",
        "(J)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = format::ulong_to_radix_string(long_arg(args, 0) as u64, 8);
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "decode",
        "(Ljava/lang/String;)Ljava/lang/Long;",
        ACC_PUBLIC | ACC_STATIC,
        boxed_long_decode,
    ),
    method(
        "valueOf",
        "(J)Ljava/lang/Long;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| boxed_long(vm, long_arg(args, 0)),
    ),
    method(
        "valueOf",
        "(Ljava/lang/String;)Ljava/lang/Long;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let value = parse_long_arg(vm, args[0])?;
            boxed_long(vm, value)
        },
    ),
    method("compare", "(JJ)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        int(long_arg(args, 0).cmp(&long_arg(args, 1)) as i32)
    }),
    method(
        "divideUnsigned",
        "(JJ)J",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let dividend = long_arg(args, 0) as u64;
            let divisor = long_arg(args, 1) as u64;
            if divisor == 0 {
                return Err(vm.throw_new("java/lang/ArithmeticException", Some("/ by zero")));
            }
            long((dividend / divisor) as i64)
        },
    ),
    method(
        "remainderUnsigned",
        "(JJ)J",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let dividend = long_arg(args, 0) as u64;
            let divisor = long_arg(args, 1) as u64;
            if divisor == 0 {
                return Err(vm.throw_new("java/lang/ArithmeticException", Some("/ by zero")));
            }
            long((dividend % divisor) as i64)
        },
    ),
    method(
        "toHexString",
        "(J)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = format::ulong_to_radix_string(long_arg(args, 0) as u64, 16);
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toBinaryString",
        "(J)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = format::ulong_to_radix_string(long_arg(args, 0) as u64, 2);
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toUnsignedString",
        "(J)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = alloc::format!("{}", long_arg(args, 0) as u64);
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toUnsignedString",
        "(JI)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = unsigned_radix_string(long_arg(args, 0) as u64, int_arg(args, 1));
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
    method(
        "getLong",
        "(Ljava/lang/String;)Ljava/lang/Long;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| long_get_long(vm, args, Value::Ref(ObjectRef::NULL)),
    ),
    method(
        "getLong",
        "(Ljava/lang/String;J)Ljava/lang/Long;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let default = boxed_long(vm, long_arg(args, 1))?;
            long_get_long(vm, args, default)
        },
    ),
    method(
        "getLong",
        "(Ljava/lang/String;Ljava/lang/Long;)Ljava/lang/Long;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| long_get_long(vm, args, Value::Ref(ref_arg(args, 1))),
    ),
    method(
        "compareTo",
        "(Ljava/lang/Long;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            int(compare_longs(
                read_value_long(vm, receiver(args)),
                read_value_long(vm, ref_arg(args, 1)),
            ))
        },
    ),
    method(
        "compareTo",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC | ACC_BRIDGE | ACC_SYNTHETIC,
        |vm, _, args| {
            let a = read_value_long(vm, receiver(args));
            let b = read_value_long(vm, ref_arg(args, 1));
            int(compare_longs(a, b))
        },
    ),
    method("sum", "(JJ)J", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        long(long_arg(args, 0).wrapping_add(long_arg(args, 1)))
    }),
    method("max", "(JJ)J", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        long(long_arg(args, 0).max(long_arg(args, 1)))
    }),
    method("min", "(JJ)J", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        long(long_arg(args, 0).min(long_arg(args, 1)))
    }),
    method(
        "numberOfLeadingZeros",
        "(J)I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| int(long_arg(args, 0).leading_zeros() as i32),
    ),
    method(
        "numberOfTrailingZeros",
        "(J)I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| int(long_arg(args, 0).trailing_zeros() as i32),
    ),
    method(
        "highestOneBit",
        "(J)J",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let value = long_arg(args, 0) as u64;
            if value == 0 {
                return long(0);
            }
            long((1u64 << (63 - value.leading_zeros())) as i64)
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
    method("bitCount", "(J)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        int(long_arg(args, 0).count_ones() as i32)
    }),
    method("reverse", "(J)J", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        long(long_arg(args, 0).reverse_bits())
    }),
    method(
        "rotateLeft",
        "(JI)J",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| long(long_arg(args, 0).rotate_left(int_arg(args, 1) as u32)),
    ),
    method(
        "rotateRight",
        "(JI)J",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| long(long_arg(args, 0).rotate_right(int_arg(args, 1) as u32)),
    ),
    method(
        "reverseBytes",
        "(J)J",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| long(long_arg(args, 0).swap_bytes()),
    ),
    method("signum", "(J)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        int(long_arg(args, 0).signum() as i32)
    }),
    method("hashCode", "(J)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        let value = long_arg(args, 0);
        int((value ^ (value >> 32)) as i32)
    }),
    method(
        "compress",
        "(JJ)J",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let value = long_arg(args, 0) as u64;
            let mask = long_arg(args, 1) as u64;
            long(compress_bits(value, mask) as i64)
        },
    ),
    method("expand", "(JJ)J", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        let value = long_arg(args, 0) as u64;
        let mask = long_arg(args, 1) as u64;
        long(expand_bits(value, mask) as i64)
    }),
    method(
        "parseLong",
        "(Ljava/lang/String;I)J",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = parse_string_arg(vm, args[0])?;
            let radix = int_arg(args, 1);
            match format::parse_long_radix(&text, radix as u32) {
                Some(value) => long(value),
                None => Err(vm.throw_new(
                    "java/lang/NumberFormatException",
                    Some(&alloc::format!("For input string: \"{text}\"")),
                )),
            }
        },
    ),
];

/// `Long.getLong`: the system property parsed as a long, or the default when unset/unparseable.
fn long_get_long(vm: &mut Vm<'_>, args: &[Value], default: Value) -> Result<Value, VmError> {
    // The JDK catches NullPointerException from System.getProperty and returns
    // the default, so a null name yields the default rather than throwing.
    if args[0].as_ref().is_null() {
        return Ok(default);
    }
    let key = string_arg(vm, args[0])?;
    if let Some(text) = crate::lang::system_property(vm, &key) {
        if let Some(value) = format::parse_long(&text) {
            return boxed_long(vm, value);
        }
    }
    Ok(default)
}

/// The `value` field of a boxed primitive, whose descriptor is `J` (Long) or `D` (Double).
pub(crate) fn read_value_long(vm: &Vm<'_>, object: ObjectRef) -> i64 {
    let class = vm.class_of(object);
    let Some(oxjvm_vm::ObjectData::Instance(fields)) = vm.heap.get(object).map(|o| &o.data) else {
        return 0;
    };
    for descriptor in ["J", "D"] {
        let entry = vm.classes.get(class);
        if let Some(field) = entry
            .fields
            .iter()
            .find(|field| field.name == "value" && field.descriptor == descriptor)
        {
            return fields[field.slot as usize].as_long();
        }
    }
    0
}

pub(crate) fn write_value_long(vm: &mut Vm<'_>, object: ObjectRef, value: i64) {
    let class = vm.class_of(object);
    for descriptor in ["J", "D"] {
        if let Some((declaring, field)) = vm.find_field_opt(class, "value", descriptor) {
            let slot = vm.classes.get(declaring).fields[field as usize].slot as usize;
            if let Some(oxjvm_vm::ObjectData::Instance(fields)) =
                vm.heap.get_mut(object).map(|object| &mut object.data)
            {
                fields[slot] = Value::Long(value);
            }
            return;
        }
    }
}

/// Parses a `String` long argument like the JDK: a null string is a
/// `NumberFormatException` ("Cannot parse null string"), not an NPE.
fn parse_long_arg(vm: &mut Vm<'_>, value: Value) -> Result<i64, VmError> {
    let Some(text) = vm.string_value(value.as_ref()) else {
        return Err(vm.throw_new(
            "java/lang/NumberFormatException",
            Some("Cannot parse null string"),
        ));
    };
    match format::parse_long(&text) {
        Some(parsed) => Ok(parsed),
        None => Err(vm.throw_new(
            "java/lang/NumberFormatException",
            Some(&alloc::format!("For input string: \"{text}\"")),
        )),
    }
}

/// Boxes a `long`, reusing the cached instance for the `-128..=127` range.
pub(crate) fn boxed_long(vm: &mut Vm<'_>, value: i64) -> Result<Value, VmError> {
    if cacheable(b'J', value) {
        if let Some(cached) = vm.cached_boxed(b'J', value) {
            return object(cached);
        }
    }
    let class = vm.resolve_class("java/lang/Long")?;
    let instance = vm.new_instance(class)?;
    write_value_long(vm, instance, value);
    if cacheable(b'J', value) {
        vm.cache_boxed(b'J', value, instance);
    }
    object(instance)
}

/// Boxes a `double` into a fresh `java/lang/Double` (its `value` field is stored as raw bits).
pub(crate) fn boxed_double(vm: &mut Vm<'_>, value: f64) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/lang/Double")?;
    let instance = vm.new_instance(class)?;
    write_value_long(vm, instance, value.to_bits() as i64);
    object(instance)
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

const SHORT_METHODS: [oxjvm_vm::NativeMethodDef; 18] = [
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
        "toUnsignedLong",
        "(S)J",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| long(i64::from(int_arg(args, 0) as i16 as u16)),
    ),
    method(
        "toUnsignedInt",
        "(S)I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| int(i32::from(int_arg(args, 0) as i16 as u16)),
    ),
    method("compare", "(SS)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        int(int_arg(args, 0) as i16 as i32 - int_arg(args, 1) as i16 as i32)
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
        "toString",
        "(S)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = format::int_to_string(int_arg(args, 0) as i16 as i32);
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
                Some(value) if (-32768..=32767).contains(&value) => int(value),
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
        |vm, _, args| box_integer_like(vm, "java/lang/Short", b'S', int_arg(args, 0)),
    ),
    method(
        "decode",
        "(Ljava/lang/String;)Ljava/lang/Short;",
        ACC_PUBLIC | ACC_STATIC,
        boxed_short_decode,
    ),
    method(
        "compareTo",
        "(Ljava/lang/Short;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            int(read_value_int(vm, receiver(args)) as i16 as i32
                - read_value_int(vm, ref_arg(args, 1)) as i16 as i32)
        },
    ),
    method(
        "compareTo",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC | ACC_BRIDGE | ACC_SYNTHETIC,
        |vm, _, args| {
            let a = read_value_int(vm, receiver(args));
            let b = read_value_int(vm, ref_arg(args, 1));
            int(compare_ints(a, b))
        },
    ),
    method(
        "reverseBytes",
        "(S)S",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| int(i32::from((int_arg(args, 0) as i16).swap_bytes())),
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_value_int(vm, receiver(args)))
    }),
    method("hashCode", "(S)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        int(i32::from(int_arg(args, 0) as i16))
    }),
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
    Some(short_clinit),
);

const BYTE_METHODS: [oxjvm_vm::NativeMethodDef; 16] = [
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
        "toUnsignedLong",
        "(B)J",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| long(i64::from(int_arg(args, 0) as i8 as u8)),
    ),
    method(
        "toUnsignedInt",
        "(B)I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| int(i32::from(int_arg(args, 0) as i8 as u8)),
    ),
    method("compare", "(BB)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        int(int_arg(args, 0) as i8 as i32 - int_arg(args, 1) as i8 as i32)
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
        "toString",
        "(B)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = format::int_to_string(int_arg(args, 0) as i8 as i32);
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
        |vm, _, args| box_integer_like(vm, "java/lang/Byte", b'B', int_arg(args, 0)),
    ),
    method(
        "decode",
        "(Ljava/lang/String;)Ljava/lang/Byte;",
        ACC_PUBLIC | ACC_STATIC,
        boxed_byte_decode,
    ),
    method(
        "compareTo",
        "(Ljava/lang/Byte;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            int(read_value_int(vm, receiver(args)) as i8 as i32
                - read_value_int(vm, ref_arg(args, 1)) as i8 as i32)
        },
    ),
    method(
        "compareTo",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC | ACC_BRIDGE | ACC_SYNTHETIC,
        |vm, _, args| {
            let a = read_value_int(vm, receiver(args));
            let b = read_value_int(vm, ref_arg(args, 1));
            int(compare_ints(a, b))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_value_int(vm, receiver(args)))
    }),
    method("hashCode", "(B)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        int(i32::from(int_arg(args, 0) as i8))
    }),
    method(
        "parseByte",
        "(Ljava/lang/String;)B",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = string_arg(vm, args[0])?;
            match format::parse_int(&text) {
                Some(value) if (-128..=127).contains(&value) => int(value),
                _ => Err(vm.throw_new(
                    "java/lang/NumberFormatException",
                    Some(&alloc::format!("For input string: \"{text}\"")),
                )),
            }
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
    Some(byte_clinit),
);

const BOOLEAN_METHODS: [oxjvm_vm::NativeMethodDef; 18] = [
    method("<init>", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        write_value_int(vm, receiver(args), i32::from(bool_arg(args, 1)));
        void()
    }),
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = parse_boolean(vm.string_value(ref_arg(args, 1)).as_deref());
            write_value_int(vm, receiver(args), i32::from(value));
            void()
        },
    ),
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
        "logicalAnd",
        "(ZZ)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| boolean(bool_arg(args, 0) && bool_arg(args, 1)),
    ),
    method(
        "logicalOr",
        "(ZZ)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| boolean(bool_arg(args, 0) || bool_arg(args, 1)),
    ),
    method(
        "logicalXor",
        "(ZZ)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| boolean(bool_arg(args, 0) ^ bool_arg(args, 1)),
    ),
    method("compare", "(ZZ)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        int(i32::from(bool_arg(args, 0)) - i32::from(bool_arg(args, 1)))
    }),
    method("hashCode", "(Z)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        int(if bool_arg(args, 0) { 1231 } else { 1237 })
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
        |vm, _, args| boolean(parse_boolean(vm.string_value(ref_arg(args, 0)).as_deref())),
    ),
    method(
        "getBoolean",
        "(Ljava/lang/String;)Z",
        ACC_PUBLIC | ACC_STATIC,
        boolean_get_boolean,
    ),
    method(
        "valueOf",
        "(Z)Ljava/lang/Boolean;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let class = vm.resolve_class("java/lang/Boolean")?;
            let name = if bool_arg(args, 0) { "TRUE" } else { "FALSE" };
            if let Some(existing) = vm.read_static_ref_if_set(class, name, "Ljava/lang/Boolean;") {
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
    method(
        "valueOf",
        "(Ljava/lang/String;)Ljava/lang/Boolean;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let value = parse_boolean(vm.string_value(ref_arg(args, 0)).as_deref());
            let class = vm.resolve_class("java/lang/Boolean")?;
            let name = if value { "TRUE" } else { "FALSE" };
            if let Some(existing) = vm.read_static_ref_if_set(class, name, "Ljava/lang/Boolean;") {
                return object(existing);
            }
            let instance = vm.new_instance(class)?;
            write_value_int(vm, instance, i32::from(value));
            vm.set_static_value(class, name, "Ljava/lang/Boolean;", Value::Ref(instance));
            object(instance)
        },
    ),
    method(
        "compareTo",
        "(Ljava/lang/Boolean;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            int(compare_ints(
                read_value_int(vm, receiver(args)),
                read_value_int(vm, ref_arg(args, 1)),
            ))
        },
    ),
    method(
        "compareTo",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC | ACC_BRIDGE | ACC_SYNTHETIC,
        |vm, _, args| {
            let a = read_value_int(vm, receiver(args));
            let b = read_value_int(vm, ref_arg(args, 1));
            int(compare_ints(a, b))
        },
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
    Some(boolean_clinit),
);

/// `Boolean.parseBoolean`: true only for `"true"` in any case; a null string is false.
fn parse_boolean(text: Option<&str>) -> bool {
    text.is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

/// `Boolean.getBoolean`: parses `System.getProperty(name)`, so a null key throws.
fn boolean_get_boolean(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    // The JDK catches NullPointerException from System.getProperty and returns
    // false, so a null name yields false rather than throwing.
    if args[0].as_ref().is_null() {
        return boolean(false);
    }
    let key = string_arg(vm, args[0])?;
    boolean(parse_boolean(
        crate::lang::system_property(vm, &key).as_deref(),
    ))
}

/// The Unicode general category of a code point (best effort beyond ASCII/Latin-1).
fn char_general_category(unit: u32) -> i32 {
    match unit {
        0x00..=0x1F | 0x7F..=0x9F => 15,
        0x20 => 12,
        0x30..=0x39 => 9,
        0x41..=0x5A => 1,
        0x61..=0x7A => 2,
        0x28 | 0x5B | 0x7B => 21,
        0x29 | 0x5D | 0x7D => 22,
        0x2D => 20,
        0x5F => 23,
        0x2B | 0x3C | 0x3D | 0x3E | 0x7C | 0x7E => 28,
        0x24 => 26,
        0x5E | 0x60 => 27,
        0x21 | 0x22 | 0x23 | 0x25 | 0x26 | 0x27 | 0x2A | 0x2C | 0x2E | 0x2F | 0x3A | 0x3B
        | 0x3F | 0x40 | 0x5C => 30,
        _ => {
            let Some(ch) = char::from_u32(unit) else {
                return if (0xD800..=0xDFFF).contains(&unit) {
                    19
                } else {
                    0
                };
            };
            if ch.is_control() {
                return 15;
            }
            if ch == '\u{2028}' {
                return 13;
            }
            if ch == '\u{2029}' {
                return 14;
            }
            if ch.is_whitespace() {
                return 12;
            }
            if ch.is_alphabetic() {
                if ch.is_uppercase() {
                    return 1;
                }
                if ch.is_lowercase() {
                    return 2;
                }
                if char_is_titlecase(unit) {
                    return 3;
                }
                return 5;
            }
            if ch.is_numeric() {
                let decimal = ch.to_digit(10);
                if decimal.is_some() {
                    return 9;
                }
                if (0x2160..=0x217F).contains(&unit) {
                    return 10;
                }
                return 11;
            }
            match unit {
                0xAB | 0x2039 | 0x2018 | 0x201A | 0x201C | 0x201E => 24,
                0xBB | 0x203A | 0x2019 | 0x201D => 25,
                0x2010..=0x2015 => 20,
                0x203E | 0xFF3F => 23,
                0xA2 | 0xA3 | 0xA5 | 0x20A0..=0x20BF => 26,
                0xB1 | 0xD7 | 0xF7 | 0x2200..=0x22FF => 28,
                0xA9 | 0xAE | 0xB0 | 0x2100..=0x214F => 29,
                0xA1 | 0xBF | 0x2020..=0x2027 | 0x2030..=0x2038 | 0x2041..=0x2043 => 30,
                _ => 0,
            }
        }
    }
}

/// Whether a code point is a titlecase letter (Unicode `Lt`).
fn char_is_titlecase(unit: u32) -> bool {
    matches!(
        unit,
        0x01C5 | 0x01C8 | 0x01CB | 0x01F2 | 0x1FBC | 0x1FCC | 0x1FFC
    )
}

/// Whether a code point has the Bidi_Mirrored property (common mirrored glyphs).
fn char_is_mirrored(unit: u32) -> bool {
    matches!(
        unit,
        0x28 | 0x29
            | 0x5B
            | 0x5D
            | 0x7B
            | 0x7D
            | 0x3C
            | 0x3E
            | 0xAB
            | 0xBB
            | 0x2039
            | 0x203A
            | 0x2045
            | 0x2046
            | 0x207D
            | 0x207E
            | 0x208D
            | 0x208E
            | 0x2264
            | 0x2265
            | 0x2266
            | 0x2267
            | 0x226A
            | 0x226B
            | 0x2282
            | 0x2283
            | 0x2286
            | 0x2287
            | 0x2208
            | 0x220B
            | 0x2209
            | 0x220C
            | 0x2308
            | 0x2309
            | 0x230A
            | 0x230B
            | 0x27E6
            | 0x27E7
            | 0x27E8
            | 0x27E9
            | 0x27EA
            | 0x27EB
            | 0x2983
            | 0x2984
            | 0x2985
            | 0x2986
            | 0x2987
            | 0x2988
            | 0x2989
            | 0x298A
            | 0x29FC
            | 0x29FD
    )
}

/// The numeric value of a code point, or -1 when it has none.
fn char_numeric_value(unit: u32) -> i32 {
    let Some(ch) = char::from_u32(unit) else {
        return -1;
    };
    if let Some(digit) = ch.to_digit(10) {
        return digit as i32;
    }
    match unit {
        0x41..=0x5A => (unit - 0x41 + 10) as i32,
        0x61..=0x7A => (unit - 0x61 + 10) as i32,
        0xFF21..=0xFF3A => (unit - 0xFF21 + 10) as i32,
        0xFF41..=0xFF5A => (unit - 0xFF41 + 10) as i32,
        0xB9 => 1,
        0xB2 => 2,
        0xB3 => 3,
        0x2070 => 0,
        0x2074..=0x2079 => (unit - 0x2074 + 4) as i32,
        0x2080..=0x2089 => (unit - 0x2080) as i32,
        0x24EA => 0,
        0x2460..=0x2468 => (unit - 0x2460 + 1) as i32,
        0x2469..=0x2473 => (unit - 0x2469 + 10) as i32,
        0x2160..=0x216B => [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12][(unit - 0x2160) as usize],
        0x2170..=0x217B => [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12][(unit - 0x2170) as usize],
        0x216C | 0x217C => 50,
        0x216D | 0x217D => 100,
        0x216E | 0x217E => 500,
        0x216F | 0x217F => 1000,
        _ => -1,
    }
}

/// Simple (single-character) uppercase mapping used by `Character.toUpperCase`:
/// full mappings that expand to several characters (ß → SS, ﬁ → FI) have no
/// simple form, so the character is returned unchanged.
fn char_simple_upper(c: char) -> char {
    // Greek letters with ypogegrammeni: the full uppercase expands to capital + Ι,
    // but the simple mapping is the precomposed capital with prosgegrammeni.
    match c {
        '\u{1F80}'..='\u{1F87}' | '\u{1F90}'..='\u{1F97}' | '\u{1FA0}'..='\u{1FA7}' => {
            return char::from_u32(c as u32 + 8).unwrap_or(c);
        }
        '\u{1FB3}' => return '\u{1FBC}',
        '\u{1FC3}' => return '\u{1FCC}',
        '\u{1FF3}' => return '\u{1FFC}',
        _ => {}
    }
    let mut upper = c.to_uppercase();
    let first = upper.next().unwrap_or(c);
    if upper.next().is_some() { c } else { first }
}

/// Simple lowercase mapping; U+0130 is the one character whose full lowercase
/// expands (i + combining dot) but whose simple mapping is a single character.
fn char_simple_lower(c: char) -> char {
    if c == '\u{0130}' {
        return 'i';
    }
    let mut lower = c.to_lowercase();
    let first = lower.next().unwrap_or(c);
    if lower.next().is_some() { c } else { first }
}

/// Titlecase differs from uppercase only for the digraph characters; everything
/// else uses the simple uppercase mapping.
fn char_to_titlecase(c: char) -> char {
    match c {
        '\u{01C4}' | '\u{01C6}' => '\u{01C5}',
        '\u{01C7}' | '\u{01C9}' => '\u{01C8}',
        '\u{01CA}' | '\u{01CC}' => '\u{01CB}',
        '\u{01F1}' | '\u{01F3}' => '\u{01F2}',
        '\u{01C5}' | '\u{01C8}' | '\u{01CB}' | '\u{01F2}' => c,
        _ => char_simple_upper(c),
    }
}

/// Directionality of every code point, as contiguous (inclusive end, value) runs.
/// Generated from the reference JDK so undefined code points report -1.
static DIRECTIONALITY_TABLE: [(u32, i8); 2328] = [
    (0x8, 9),
    (0x9, 11),
    (0xA, 10),
    (0xB, 11),
    (0xC, 12),
    (0xD, 10),
    (0x1B, 9),
    (0x1E, 10),
    (0x1F, 11),
    (0x20, 12),
    (0x22, 13),
    (0x25, 5),
    (0x2A, 13),
    (0x2B, 4),
    (0x2C, 7),
    (0x2D, 4),
    (0x2F, 7),
    (0x39, 3),
    (0x3A, 7),
    (0x40, 13),
    (0x5A, 0),
    (0x60, 13),
    (0x7A, 0),
    (0x7E, 13),
    (0x84, 9),
    (0x85, 10),
    (0x9F, 9),
    (0xA0, 7),
    (0xA1, 13),
    (0xA5, 5),
    (0xA9, 13),
    (0xAA, 0),
    (0xAC, 13),
    (0xAD, 9),
    (0xAF, 13),
    (0xB1, 5),
    (0xB3, 3),
    (0xB4, 13),
    (0xB5, 0),
    (0xB8, 13),
    (0xB9, 3),
    (0xBA, 0),
    (0xBF, 13),
    (0xD6, 0),
    (0xD7, 13),
    (0xF6, 0),
    (0xF7, 13),
    (0x2B8, 0),
    (0x2BA, 13),
    (0x2C1, 0),
    (0x2CF, 13),
    (0x2D1, 0),
    (0x2DF, 13),
    (0x2E4, 0),
    (0x2ED, 13),
    (0x2EE, 0),
    (0x2FF, 13),
    (0x36F, 8),
    (0x373, 0),
    (0x375, 13),
    (0x377, 0),
    (0x379, -1),
    (0x37D, 0),
    (0x37E, 13),
    (0x37F, 0),
    (0x383, -1),
    (0x385, 13),
    (0x386, 0),
    (0x387, 13),
    (0x38A, 0),
    (0x38B, -1),
    (0x38C, 0),
    (0x38D, -1),
    (0x3A1, 0),
    (0x3A2, -1),
    (0x3F5, 0),
    (0x3F6, 13),
    (0x482, 0),
    (0x489, 8),
    (0x52F, 0),
    (0x530, -1),
    (0x556, 0),
    (0x558, -1),
    (0x589, 0),
    (0x58A, 13),
    (0x58C, -1),
    (0x58E, 13),
    (0x58F, 5),
    (0x590, -1),
    (0x5BD, 8),
    (0x5BE, 1),
    (0x5BF, 8),
    (0x5C0, 1),
    (0x5C2, 8),
    (0x5C3, 1),
    (0x5C5, 8),
    (0x5C6, 1),
    (0x5C7, 8),
    (0x5CF, -1),
    (0x5EA, 1),
    (0x5EE, -1),
    (0x5F4, 1),
    (0x5FF, -1),
    (0x605, 6),
    (0x607, 13),
    (0x608, 2),
    (0x60A, 5),
    (0x60B, 2),
    (0x60C, 7),
    (0x60D, 2),
    (0x60F, 13),
    (0x61A, 8),
    (0x64A, 2),
    (0x65F, 8),
    (0x669, 6),
    (0x66A, 5),
    (0x66C, 6),
    (0x66F, 2),
    (0x670, 8),
    (0x6D5, 2),
    (0x6DC, 8),
    (0x6DD, 6),
    (0x6DE, 13),
    (0x6E4, 8),
    (0x6E6, 2),
    (0x6E8, 8),
    (0x6E9, 13),
    (0x6ED, 8),
    (0x6EF, 2),
    (0x6F9, 3),
    (0x70D, 2),
    (0x70E, -1),
    (0x710, 2),
    (0x711, 8),
    (0x72F, 2),
    (0x74A, 8),
    (0x74C, -1),
    (0x7A5, 2),
    (0x7B0, 8),
    (0x7B1, 2),
    (0x7BF, -1),
    (0x7EA, 1),
    (0x7F3, 8),
    (0x7F5, 1),
    (0x7F9, 13),
    (0x7FA, 1),
    (0x7FC, -1),
    (0x7FD, 8),
    (0x815, 1),
    (0x819, 8),
    (0x81A, 1),
    (0x823, 8),
    (0x824, 1),
    (0x827, 8),
    (0x828, 1),
    (0x82D, 8),
    (0x82F, -1),
    (0x83E, 1),
    (0x83F, -1),
    (0x858, 1),
    (0x85B, 8),
    (0x85D, -1),
    (0x85E, 1),
    (0x85F, -1),
    (0x86A, 2),
    (0x86F, -1),
    (0x88F, 2),
    (0x891, 6),
    (0x896, -1),
    (0x89F, 8),
    (0x8C9, 2),
    (0x8E1, 8),
    (0x8E2, 6),
    (0x902, 8),
    (0x939, 0),
    (0x93A, 8),
    (0x93B, 0),
    (0x93C, 8),
    (0x940, 0),
    (0x948, 8),
    (0x94C, 0),
    (0x94D, 8),
    (0x950, 0),
    (0x957, 8),
    (0x961, 0),
    (0x963, 8),
    (0x980, 0),
    (0x981, 8),
    (0x983, 0),
    (0x984, -1),
    (0x98C, 0),
    (0x98E, -1),
    (0x990, 0),
    (0x992, -1),
    (0x9A8, 0),
    (0x9A9, -1),
    (0x9B0, 0),
    (0x9B1, -1),
    (0x9B2, 0),
    (0x9B5, -1),
    (0x9B9, 0),
    (0x9BB, -1),
    (0x9BC, 8),
    (0x9C0, 0),
    (0x9C4, 8),
    (0x9C6, -1),
    (0x9C8, 0),
    (0x9CA, -1),
    (0x9CC, 0),
    (0x9CD, 8),
    (0x9CE, 0),
    (0x9D6, -1),
    (0x9D7, 0),
    (0x9DB, -1),
    (0x9DD, 0),
    (0x9DE, -1),
    (0x9E1, 0),
    (0x9E3, 8),
    (0x9E5, -1),
    (0x9F1, 0),
    (0x9F3, 5),
    (0x9FA, 0),
    (0x9FB, 5),
    (0x9FD, 0),
    (0x9FE, 8),
    (0xA00, -1),
    (0xA02, 8),
    (0xA03, 0),
    (0xA04, -1),
    (0xA0A, 0),
    (0xA0E, -1),
    (0xA10, 0),
    (0xA12, -1),
    (0xA28, 0),
    (0xA29, -1),
    (0xA30, 0),
    (0xA31, -1),
    (0xA33, 0),
    (0xA34, -1),
    (0xA36, 0),
    (0xA37, -1),
    (0xA39, 0),
    (0xA3B, -1),
    (0xA3C, 8),
    (0xA3D, -1),
    (0xA40, 0),
    (0xA42, 8),
    (0xA46, -1),
    (0xA48, 8),
    (0xA4A, -1),
    (0xA4D, 8),
    (0xA50, -1),
    (0xA51, 8),
    (0xA58, -1),
    (0xA5C, 0),
    (0xA5D, -1),
    (0xA5E, 0),
    (0xA65, -1),
    (0xA6F, 0),
    (0xA71, 8),
    (0xA74, 0),
    (0xA75, 8),
    (0xA76, 0),
    (0xA80, -1),
    (0xA82, 8),
    (0xA83, 0),
    (0xA84, -1),
    (0xA8D, 0),
    (0xA8E, -1),
    (0xA91, 0),
    (0xA92, -1),
    (0xAA8, 0),
    (0xAA9, -1),
    (0xAB0, 0),
    (0xAB1, -1),
    (0xAB3, 0),
    (0xAB4, -1),
    (0xAB9, 0),
    (0xABB, -1),
    (0xABC, 8),
    (0xAC0, 0),
    (0xAC5, 8),
    (0xAC6, -1),
    (0xAC8, 8),
    (0xAC9, 0),
    (0xACA, -1),
    (0xACC, 0),
    (0xACD, 8),
    (0xACF, -1),
    (0xAD0, 0),
    (0xADF, -1),
    (0xAE1, 0),
    (0xAE3, 8),
    (0xAE5, -1),
    (0xAF0, 0),
    (0xAF1, 5),
    (0xAF8, -1),
    (0xAF9, 0),
    (0xAFF, 8),
    (0xB00, -1),
    (0xB01, 8),
    (0xB03, 0),
    (0xB04, -1),
    (0xB0C, 0),
    (0xB0E, -1),
    (0xB10, 0),
    (0xB12, -1),
    (0xB28, 0),
    (0xB29, -1),
    (0xB30, 0),
    (0xB31, -1),
    (0xB33, 0),
    (0xB34, -1),
    (0xB39, 0),
    (0xB3B, -1),
    (0xB3C, 8),
    (0xB3E, 0),
    (0xB3F, 8),
    (0xB40, 0),
    (0xB44, 8),
    (0xB46, -1),
    (0xB48, 0),
    (0xB4A, -1),
    (0xB4C, 0),
    (0xB4D, 8),
    (0xB54, -1),
    (0xB56, 8),
    (0xB57, 0),
    (0xB5B, -1),
    (0xB5D, 0),
    (0xB5E, -1),
    (0xB61, 0),
    (0xB63, 8),
    (0xB65, -1),
    (0xB77, 0),
    (0xB81, -1),
    (0xB82, 8),
    (0xB83, 0),
    (0xB84, -1),
    (0xB8A, 0),
    (0xB8D, -1),
    (0xB90, 0),
    (0xB91, -1),
    (0xB95, 0),
    (0xB98, -1),
    (0xB9A, 0),
    (0xB9B, -1),
    (0xB9C, 0),
    (0xB9D, -1),
    (0xB9F, 0),
    (0xBA2, -1),
    (0xBA4, 0),
    (0xBA7, -1),
    (0xBAA, 0),
    (0xBAD, -1),
    (0xBB9, 0),
    (0xBBD, -1),
    (0xBBF, 0),
    (0xBC0, 8),
    (0xBC2, 0),
    (0xBC5, -1),
    (0xBC8, 0),
    (0xBC9, -1),
    (0xBCC, 0),
    (0xBCD, 8),
    (0xBCF, -1),
    (0xBD0, 0),
    (0xBD6, -1),
    (0xBD7, 0),
    (0xBE5, -1),
    (0xBF2, 0),
    (0xBF8, 13),
    (0xBF9, 5),
    (0xBFA, 13),
    (0xBFF, -1),
    (0xC00, 8),
    (0xC03, 0),
    (0xC04, 8),
    (0xC0C, 0),
    (0xC0D, -1),
    (0xC10, 0),
    (0xC11, -1),
    (0xC28, 0),
    (0xC29, -1),
    (0xC39, 0),
    (0xC3B, -1),
    (0xC3C, 8),
    (0xC3D, 0),
    (0xC40, 8),
    (0xC44, 0),
    (0xC45, -1),
    (0xC48, 8),
    (0xC49, -1),
    (0xC4D, 8),
    (0xC54, -1),
    (0xC56, 8),
    (0xC57, -1),
    (0xC5A, 0),
    (0xC5B, -1),
    (0xC5D, 0),
    (0xC5F, -1),
    (0xC61, 0),
    (0xC63, 8),
    (0xC65, -1),
    (0xC6F, 0),
    (0xC76, -1),
    (0xC77, 0),
    (0xC7E, 13),
    (0xC80, 0),
    (0xC81, 8),
    (0xC8C, 0),
    (0xC8D, -1),
    (0xC90, 0),
    (0xC91, -1),
    (0xCA8, 0),
    (0xCA9, -1),
    (0xCB3, 0),
    (0xCB4, -1),
    (0xCB9, 0),
    (0xCBB, -1),
    (0xCBC, 8),
    (0xCC4, 0),
    (0xCC5, -1),
    (0xCC8, 0),
    (0xCC9, -1),
    (0xCCB, 0),
    (0xCCD, 8),
    (0xCD4, -1),
    (0xCD6, 0),
    (0xCDB, -1),
    (0xCDE, 0),
    (0xCDF, -1),
    (0xCE1, 0),
    (0xCE3, 8),
    (0xCE5, -1),
    (0xCEF, 0),
    (0xCF0, -1),
    (0xCF3, 0),
    (0xCFF, -1),
    (0xD01, 8),
    (0xD0C, 0),
    (0xD0D, -1),
    (0xD10, 0),
    (0xD11, -1),
    (0xD3A, 0),
    (0xD3C, 8),
    (0xD40, 0),
    (0xD44, 8),
    (0xD45, -1),
    (0xD48, 0),
    (0xD49, -1),
    (0xD4C, 0),
    (0xD4D, 8),
    (0xD4F, 0),
    (0xD53, -1),
    (0xD61, 0),
    (0xD63, 8),
    (0xD65, -1),
    (0xD7F, 0),
    (0xD80, -1),
    (0xD81, 8),
    (0xD83, 0),
    (0xD84, -1),
    (0xD96, 0),
    (0xD99, -1),
    (0xDB1, 0),
    (0xDB2, -1),
    (0xDBB, 0),
    (0xDBC, -1),
    (0xDBD, 0),
    (0xDBF, -1),
    (0xDC6, 0),
    (0xDC9, -1),
    (0xDCA, 8),
    (0xDCE, -1),
    (0xDD1, 0),
    (0xDD4, 8),
    (0xDD5, -1),
    (0xDD6, 8),
    (0xDD7, -1),
    (0xDDF, 0),
    (0xDE5, -1),
    (0xDEF, 0),
    (0xDF1, -1),
    (0xDF4, 0),
    (0xE00, -1),
    (0xE30, 0),
    (0xE31, 8),
    (0xE33, 0),
    (0xE3A, 8),
    (0xE3E, -1),
    (0xE3F, 5),
    (0xE46, 0),
    (0xE4E, 8),
    (0xE5B, 0),
    (0xE80, -1),
    (0xE82, 0),
    (0xE83, -1),
    (0xE84, 0),
    (0xE85, -1),
    (0xE8A, 0),
    (0xE8B, -1),
    (0xEA3, 0),
    (0xEA4, -1),
    (0xEA5, 0),
    (0xEA6, -1),
    (0xEB0, 0),
    (0xEB1, 8),
    (0xEB3, 0),
    (0xEBC, 8),
    (0xEBD, 0),
    (0xEBF, -1),
    (0xEC4, 0),
    (0xEC5, -1),
    (0xEC6, 0),
    (0xEC7, -1),
    (0xECE, 8),
    (0xECF, -1),
    (0xED9, 0),
    (0xEDB, -1),
    (0xEDF, 0),
    (0xEFF, -1),
    (0xF17, 0),
    (0xF19, 8),
    (0xF34, 0),
    (0xF35, 8),
    (0xF36, 0),
    (0xF37, 8),
    (0xF38, 0),
    (0xF39, 8),
    (0xF3D, 13),
    (0xF47, 0),
    (0xF48, -1),
    (0xF6C, 0),
    (0xF70, -1),
    (0xF7E, 8),
    (0xF7F, 0),
    (0xF84, 8),
    (0xF85, 0),
    (0xF87, 8),
    (0xF8C, 0),
    (0xF97, 8),
    (0xF98, -1),
    (0xFBC, 8),
    (0xFBD, -1),
    (0xFC5, 0),
    (0xFC6, 8),
    (0xFCC, 0),
    (0xFCD, -1),
    (0xFDA, 0),
    (0xFFF, -1),
    (0x102C, 0),
    (0x1030, 8),
    (0x1031, 0),
    (0x1037, 8),
    (0x1038, 0),
    (0x103A, 8),
    (0x103C, 0),
    (0x103E, 8),
    (0x1057, 0),
    (0x1059, 8),
    (0x105D, 0),
    (0x1060, 8),
    (0x1070, 0),
    (0x1074, 8),
    (0x1081, 0),
    (0x1082, 8),
    (0x1084, 0),
    (0x1086, 8),
    (0x108C, 0),
    (0x108D, 8),
    (0x109C, 0),
    (0x109D, 8),
    (0x10C5, 0),
    (0x10C6, -1),
    (0x10C7, 0),
    (0x10CC, -1),
    (0x10CD, 0),
    (0x10CF, -1),
    (0x1248, 0),
    (0x1249, -1),
    (0x124D, 0),
    (0x124F, -1),
    (0x1256, 0),
    (0x1257, -1),
    (0x1258, 0),
    (0x1259, -1),
    (0x125D, 0),
    (0x125F, -1),
    (0x1288, 0),
    (0x1289, -1),
    (0x128D, 0),
    (0x128F, -1),
    (0x12B0, 0),
    (0x12B1, -1),
    (0x12B5, 0),
    (0x12B7, -1),
    (0x12BE, 0),
    (0x12BF, -1),
    (0x12C0, 0),
    (0x12C1, -1),
    (0x12C5, 0),
    (0x12C7, -1),
    (0x12D6, 0),
    (0x12D7, -1),
    (0x1310, 0),
    (0x1311, -1),
    (0x1315, 0),
    (0x1317, -1),
    (0x135A, 0),
    (0x135C, -1),
    (0x135F, 8),
    (0x137C, 0),
    (0x137F, -1),
    (0x138F, 0),
    (0x1399, 13),
    (0x139F, -1),
    (0x13F5, 0),
    (0x13F7, -1),
    (0x13FD, 0),
    (0x13FF, -1),
    (0x1400, 13),
    (0x167F, 0),
    (0x1680, 12),
    (0x169A, 0),
    (0x169C, 13),
    (0x169F, -1),
    (0x16F8, 0),
    (0x16FF, -1),
    (0x1711, 0),
    (0x1714, 8),
    (0x1715, 0),
    (0x171E, -1),
    (0x1731, 0),
    (0x1733, 8),
    (0x1736, 0),
    (0x173F, -1),
    (0x1751, 0),
    (0x1753, 8),
    (0x175F, -1),
    (0x176C, 0),
    (0x176D, -1),
    (0x1770, 0),
    (0x1771, -1),
    (0x1773, 8),
    (0x177F, -1),
    (0x17B3, 0),
    (0x17B5, 8),
    (0x17B6, 0),
    (0x17BD, 8),
    (0x17C5, 0),
    (0x17C6, 8),
    (0x17C8, 0),
    (0x17D3, 8),
    (0x17DA, 0),
    (0x17DB, 5),
    (0x17DC, 0),
    (0x17DD, 8),
    (0x17DF, -1),
    (0x17E9, 0),
    (0x17EF, -1),
    (0x17F9, 13),
    (0x17FF, -1),
    (0x180A, 13),
    (0x180D, 8),
    (0x180E, 9),
    (0x180F, 8),
    (0x1819, 0),
    (0x181F, -1),
    (0x1878, 0),
    (0x187F, -1),
    (0x1884, 0),
    (0x1886, 8),
    (0x18A8, 0),
    (0x18A9, 8),
    (0x18AA, 0),
    (0x18AF, -1),
    (0x18F5, 0),
    (0x18FF, -1),
    (0x191E, 0),
    (0x191F, -1),
    (0x1922, 8),
    (0x1926, 0),
    (0x1928, 8),
    (0x192B, 0),
    (0x192F, -1),
    (0x1931, 0),
    (0x1932, 8),
    (0x1938, 0),
    (0x193B, 8),
    (0x193F, -1),
    (0x1940, 13),
    (0x1943, -1),
    (0x1945, 13),
    (0x196D, 0),
    (0x196F, -1),
    (0x1974, 0),
    (0x197F, -1),
    (0x19AB, 0),
    (0x19AF, -1),
    (0x19C9, 0),
    (0x19CF, -1),
    (0x19DA, 0),
    (0x19DD, -1),
    (0x19FF, 13),
    (0x1A16, 0),
    (0x1A18, 8),
    (0x1A1A, 0),
    (0x1A1B, 8),
    (0x1A1D, -1),
    (0x1A55, 0),
    (0x1A56, 8),
    (0x1A57, 0),
    (0x1A5E, 8),
    (0x1A5F, -1),
    (0x1A60, 8),
    (0x1A61, 0),
    (0x1A62, 8),
    (0x1A64, 0),
    (0x1A6C, 8),
    (0x1A72, 0),
    (0x1A7C, 8),
    (0x1A7E, -1),
    (0x1A7F, 8),
    (0x1A89, 0),
    (0x1A8F, -1),
    (0x1A99, 0),
    (0x1A9F, -1),
    (0x1AAD, 0),
    (0x1AAF, -1),
    (0x1ADD, 8),
    (0x1ADF, -1),
    (0x1AEB, 8),
    (0x1AFF, -1),
    (0x1B03, 8),
    (0x1B33, 0),
    (0x1B34, 8),
    (0x1B35, 0),
    (0x1B3A, 8),
    (0x1B3B, 0),
    (0x1B3C, 8),
    (0x1B41, 0),
    (0x1B42, 8),
    (0x1B4C, 0),
    (0x1B4D, -1),
    (0x1B6A, 0),
    (0x1B73, 8),
    (0x1B7F, 0),
    (0x1B81, 8),
    (0x1BA1, 0),
    (0x1BA5, 8),
    (0x1BA7, 0),
    (0x1BA9, 8),
    (0x1BAA, 0),
    (0x1BAD, 8),
    (0x1BE5, 0),
    (0x1BE6, 8),
    (0x1BE7, 0),
    (0x1BE9, 8),
    (0x1BEC, 0),
    (0x1BED, 8),
    (0x1BEE, 0),
    (0x1BF1, 8),
    (0x1BF3, 0),
    (0x1BFB, -1),
    (0x1C2B, 0),
    (0x1C33, 8),
    (0x1C35, 0),
    (0x1C37, 8),
    (0x1C3A, -1),
    (0x1C49, 0),
    (0x1C4C, -1),
    (0x1C8A, 0),
    (0x1C8F, -1),
    (0x1CBA, 0),
    (0x1CBC, -1),
    (0x1CC7, 0),
    (0x1CCF, -1),
    (0x1CD2, 8),
    (0x1CD3, 0),
    (0x1CE0, 8),
    (0x1CE1, 0),
    (0x1CE8, 8),
    (0x1CEC, 0),
    (0x1CED, 8),
    (0x1CF3, 0),
    (0x1CF4, 8),
    (0x1CF7, 0),
    (0x1CF9, 8),
    (0x1CFA, 0),
    (0x1CFF, -1),
    (0x1DBF, 0),
    (0x1DFF, 8),
    (0x1F15, 0),
    (0x1F17, -1),
    (0x1F1D, 0),
    (0x1F1F, -1),
    (0x1F45, 0),
    (0x1F47, -1),
    (0x1F4D, 0),
    (0x1F4F, -1),
    (0x1F57, 0),
    (0x1F58, -1),
    (0x1F59, 0),
    (0x1F5A, -1),
    (0x1F5B, 0),
    (0x1F5C, -1),
    (0x1F5D, 0),
    (0x1F5E, -1),
    (0x1F7D, 0),
    (0x1F7F, -1),
    (0x1FB4, 0),
    (0x1FB5, -1),
    (0x1FBC, 0),
    (0x1FBD, 13),
    (0x1FBE, 0),
    (0x1FC1, 13),
    (0x1FC4, 0),
    (0x1FC5, -1),
    (0x1FCC, 0),
    (0x1FCF, 13),
    (0x1FD3, 0),
    (0x1FD5, -1),
    (0x1FDB, 0),
    (0x1FDC, -1),
    (0x1FDF, 13),
    (0x1FEC, 0),
    (0x1FEF, 13),
    (0x1FF1, -1),
    (0x1FF4, 0),
    (0x1FF5, -1),
    (0x1FFC, 0),
    (0x1FFE, 13),
    (0x1FFF, -1),
    (0x200A, 12),
    (0x200D, 9),
    (0x200E, 0),
    (0x200F, 1),
    (0x2027, 13),
    (0x2028, 12),
    (0x2029, 10),
    (0x202A, 14),
    (0x202B, 16),
    (0x202C, 18),
    (0x202D, 15),
    (0x202E, 17),
    (0x202F, 7),
    (0x2034, 5),
    (0x2043, 13),
    (0x2044, 7),
    (0x205E, 13),
    (0x205F, 12),
    (0x2064, 9),
    (0x2065, -1),
    (0x2066, 19),
    (0x2067, 20),
    (0x2068, 21),
    (0x2069, 22),
    (0x206F, 9),
    (0x2070, 3),
    (0x2071, 0),
    (0x2073, -1),
    (0x2079, 3),
    (0x207B, 4),
    (0x207E, 13),
    (0x207F, 0),
    (0x2089, 3),
    (0x208B, 4),
    (0x208E, 13),
    (0x208F, -1),
    (0x209C, 0),
    (0x209F, -1),
    (0x20C1, 5),
    (0x20CF, -1),
    (0x20F0, 8),
    (0x20FF, -1),
    (0x2101, 13),
    (0x2102, 0),
    (0x2106, 13),
    (0x2107, 0),
    (0x2109, 13),
    (0x2113, 0),
    (0x2114, 13),
    (0x2115, 0),
    (0x2118, 13),
    (0x211D, 0),
    (0x2123, 13),
    (0x2124, 0),
    (0x2125, 13),
    (0x2126, 0),
    (0x2127, 13),
    (0x2128, 0),
    (0x2129, 13),
    (0x212D, 0),
    (0x212E, 5),
    (0x2139, 0),
    (0x213B, 13),
    (0x213F, 0),
    (0x2144, 13),
    (0x2149, 0),
    (0x214D, 13),
    (0x214F, 0),
    (0x215F, 13),
    (0x2188, 0),
    (0x218B, 13),
    (0x218F, -1),
    (0x2211, 13),
    (0x2212, 4),
    (0x2213, 5),
    (0x2335, 13),
    (0x237A, 0),
    (0x2394, 13),
    (0x2395, 0),
    (0x2429, 13),
    (0x243F, -1),
    (0x244A, 13),
    (0x245F, -1),
    (0x2487, 13),
    (0x249B, 3),
    (0x24E9, 0),
    (0x26AB, 13),
    (0x26AC, 0),
    (0x27FF, 13),
    (0x28FF, 0),
    (0x2B73, 13),
    (0x2B75, -1),
    (0x2BFF, 13),
    (0x2CE4, 0),
    (0x2CEA, 13),
    (0x2CEE, 0),
    (0x2CF1, 8),
    (0x2CF3, 0),
    (0x2CF8, -1),
    (0x2CFF, 13),
    (0x2D25, 0),
    (0x2D26, -1),
    (0x2D27, 0),
    (0x2D2C, -1),
    (0x2D2D, 0),
    (0x2D2F, -1),
    (0x2D67, 0),
    (0x2D6E, -1),
    (0x2D70, 0),
    (0x2D7E, -1),
    (0x2D7F, 8),
    (0x2D96, 0),
    (0x2D9F, -1),
    (0x2DA6, 0),
    (0x2DA7, -1),
    (0x2DAE, 0),
    (0x2DAF, -1),
    (0x2DB6, 0),
    (0x2DB7, -1),
    (0x2DBE, 0),
    (0x2DBF, -1),
    (0x2DC6, 0),
    (0x2DC7, -1),
    (0x2DCE, 0),
    (0x2DCF, -1),
    (0x2DD6, 0),
    (0x2DD7, -1),
    (0x2DDE, 0),
    (0x2DDF, -1),
    (0x2DFF, 8),
    (0x2E5D, 13),
    (0x2E7F, -1),
    (0x2E99, 13),
    (0x2E9A, -1),
    (0x2EF3, 13),
    (0x2EFF, -1),
    (0x2FD5, 13),
    (0x2FEF, -1),
    (0x2FFF, 13),
    (0x3000, 12),
    (0x3004, 13),
    (0x3007, 0),
    (0x3020, 13),
    (0x3029, 0),
    (0x302D, 8),
    (0x302F, 0),
    (0x3030, 13),
    (0x3035, 0),
    (0x3037, 13),
    (0x303C, 0),
    (0x303F, 13),
    (0x3040, -1),
    (0x3096, 0),
    (0x3098, -1),
    (0x309A, 8),
    (0x309C, 13),
    (0x309F, 0),
    (0x30A0, 13),
    (0x30FA, 0),
    (0x30FB, 13),
    (0x30FF, 0),
    (0x3104, -1),
    (0x312F, 0),
    (0x3130, -1),
    (0x318E, 0),
    (0x318F, -1),
    (0x31BF, 0),
    (0x31E5, 13),
    (0x31EE, -1),
    (0x31EF, 13),
    (0x321C, 0),
    (0x321E, 13),
    (0x321F, -1),
    (0x324F, 0),
    (0x325F, 13),
    (0x327B, 0),
    (0x327E, 13),
    (0x32B0, 0),
    (0x32BF, 13),
    (0x32CB, 0),
    (0x32CF, 13),
    (0x3376, 0),
    (0x337A, 13),
    (0x33DD, 0),
    (0x33DF, 13),
    (0x33FE, 0),
    (0x33FF, 13),
    (0x4DBF, 0),
    (0x4DFF, 13),
    (0xA48C, 0),
    (0xA48F, -1),
    (0xA4C6, 13),
    (0xA4CF, -1),
    (0xA60C, 0),
    (0xA60F, 13),
    (0xA62B, 0),
    (0xA63F, -1),
    (0xA66E, 0),
    (0xA672, 8),
    (0xA673, 13),
    (0xA67D, 8),
    (0xA67F, 13),
    (0xA69D, 0),
    (0xA69F, 8),
    (0xA6EF, 0),
    (0xA6F1, 8),
    (0xA6F7, 0),
    (0xA6FF, -1),
    (0xA721, 13),
    (0xA787, 0),
    (0xA788, 13),
    (0xA7DC, 0),
    (0xA7F0, -1),
    (0xA801, 0),
    (0xA802, 8),
    (0xA805, 0),
    (0xA806, 8),
    (0xA80A, 0),
    (0xA80B, 8),
    (0xA824, 0),
    (0xA826, 8),
    (0xA827, 0),
    (0xA82B, 13),
    (0xA82C, 8),
    (0xA82F, -1),
    (0xA837, 0),
    (0xA839, 5),
    (0xA83F, -1),
    (0xA873, 0),
    (0xA877, 13),
    (0xA87F, -1),
    (0xA8C3, 0),
    (0xA8C5, 8),
    (0xA8CD, -1),
    (0xA8D9, 0),
    (0xA8DF, -1),
    (0xA8F1, 8),
    (0xA8FE, 0),
    (0xA8FF, 8),
    (0xA925, 0),
    (0xA92D, 8),
    (0xA946, 0),
    (0xA951, 8),
    (0xA953, 0),
    (0xA95E, -1),
    (0xA97C, 0),
    (0xA97F, -1),
    (0xA982, 8),
    (0xA9B2, 0),
    (0xA9B3, 8),
    (0xA9B5, 0),
    (0xA9B9, 8),
    (0xA9BB, 0),
    (0xA9BD, 8),
    (0xA9CD, 0),
    (0xA9CE, -1),
    (0xA9D9, 0),
    (0xA9DD, -1),
    (0xA9E4, 0),
    (0xA9E5, 8),
    (0xA9FE, 0),
    (0xA9FF, -1),
    (0xAA28, 0),
    (0xAA2E, 8),
    (0xAA30, 0),
    (0xAA32, 8),
    (0xAA34, 0),
    (0xAA36, 8),
    (0xAA3F, -1),
    (0xAA42, 0),
    (0xAA43, 8),
    (0xAA4B, 0),
    (0xAA4C, 8),
    (0xAA4D, 0),
    (0xAA4F, -1),
    (0xAA59, 0),
    (0xAA5B, -1),
    (0xAA7B, 0),
    (0xAA7C, 8),
    (0xAAAF, 0),
    (0xAAB0, 8),
    (0xAAB1, 0),
    (0xAAB4, 8),
    (0xAAB6, 0),
    (0xAAB8, 8),
    (0xAABD, 0),
    (0xAABF, 8),
    (0xAAC0, 0),
    (0xAAC1, 8),
    (0xAAC2, 0),
    (0xAADA, -1),
    (0xAAEB, 0),
    (0xAAED, 8),
    (0xAAF5, 0),
    (0xAAF6, 8),
    (0xAB00, -1),
    (0xAB06, 0),
    (0xAB08, -1),
    (0xAB0E, 0),
    (0xAB10, -1),
    (0xAB16, 0),
    (0xAB1F, -1),
    (0xAB26, 0),
    (0xAB27, -1),
    (0xAB2E, 0),
    (0xAB2F, -1),
    (0xAB69, 0),
    (0xAB6B, 13),
    (0xAB6F, -1),
    (0xABE4, 0),
    (0xABE5, 8),
    (0xABE7, 0),
    (0xABE8, 8),
    (0xABEC, 0),
    (0xABED, 8),
    (0xABEF, -1),
    (0xABF9, 0),
    (0xABFF, -1),
    (0xD7A3, 0),
    (0xD7AF, -1),
    (0xD7C6, 0),
    (0xD7CA, -1),
    (0xD7FB, 0),
    (0xD7FF, -1),
    (0xFA6D, 0),
    (0xFA6F, -1),
    (0xFAD9, 0),
    (0xFAFF, -1),
    (0xFB06, 0),
    (0xFB12, -1),
    (0xFB17, 0),
    (0xFB1C, -1),
    (0xFB1D, 1),
    (0xFB1E, 8),
    (0xFB28, 1),
    (0xFB29, 4),
    (0xFB36, 1),
    (0xFB37, -1),
    (0xFB3C, 1),
    (0xFB3D, -1),
    (0xFB3E, 1),
    (0xFB3F, -1),
    (0xFB41, 1),
    (0xFB42, -1),
    (0xFB44, 1),
    (0xFB45, -1),
    (0xFB4F, 1),
    (0xFBC2, 2),
    (0xFBD2, 13),
    (0xFD3D, 2),
    (0xFD4F, 13),
    (0xFD8F, 2),
    (0xFD91, 13),
    (0xFDC7, 2),
    (0xFDCF, 13),
    (0xFDEF, -1),
    (0xFDFC, 2),
    (0xFDFF, 13),
    (0xFE0F, 8),
    (0xFE19, 13),
    (0xFE1F, -1),
    (0xFE2F, 8),
    (0xFE4F, 13),
    (0xFE50, 7),
    (0xFE51, 13),
    (0xFE52, 7),
    (0xFE53, -1),
    (0xFE54, 13),
    (0xFE55, 7),
    (0xFE5E, 13),
    (0xFE5F, 5),
    (0xFE61, 13),
    (0xFE63, 4),
    (0xFE66, 13),
    (0xFE67, -1),
    (0xFE68, 13),
    (0xFE6A, 5),
    (0xFE6B, 13),
    (0xFE6F, -1),
    (0xFE74, 2),
    (0xFE75, -1),
    (0xFEFC, 2),
    (0xFEFE, -1),
    (0xFEFF, 9),
    (0xFF00, -1),
    (0xFF02, 13),
    (0xFF05, 5),
    (0xFF0A, 13),
    (0xFF0B, 4),
    (0xFF0C, 7),
    (0xFF0D, 4),
    (0xFF0F, 7),
    (0xFF19, 3),
    (0xFF1A, 7),
    (0xFF20, 13),
    (0xFF3A, 0),
    (0xFF40, 13),
    (0xFF5A, 0),
    (0xFF65, 13),
    (0xFFBE, 0),
    (0xFFC1, -1),
    (0xFFC7, 0),
    (0xFFC9, -1),
    (0xFFCF, 0),
    (0xFFD1, -1),
    (0xFFD7, 0),
    (0xFFD9, -1),
    (0xFFDC, 0),
    (0xFFDF, -1),
    (0xFFE1, 5),
    (0xFFE4, 13),
    (0xFFE6, 5),
    (0xFFE7, -1),
    (0xFFEE, 13),
    (0xFFF8, -1),
    (0xFFFD, 13),
    (0xFFFF, -1),
    (0x1000B, 0),
    (0x1000C, -1),
    (0x10026, 0),
    (0x10027, -1),
    (0x1003A, 0),
    (0x1003B, -1),
    (0x1003D, 0),
    (0x1003E, -1),
    (0x1004D, 0),
    (0x1004F, -1),
    (0x1005D, 0),
    (0x1007F, -1),
    (0x100FA, 0),
    (0x100FF, -1),
    (0x10100, 0),
    (0x10101, 13),
    (0x10102, 0),
    (0x10106, -1),
    (0x10133, 0),
    (0x10136, -1),
    (0x1013F, 0),
    (0x1018C, 13),
    (0x1018E, 0),
    (0x1018F, -1),
    (0x1019C, 13),
    (0x1019F, -1),
    (0x101A0, 13),
    (0x101CF, -1),
    (0x101FC, 0),
    (0x101FD, 8),
    (0x1027F, -1),
    (0x1029C, 0),
    (0x1029F, -1),
    (0x102D0, 0),
    (0x102DF, -1),
    (0x102E0, 8),
    (0x102FB, 3),
    (0x102FF, -1),
    (0x10323, 0),
    (0x1032C, -1),
    (0x1034A, 0),
    (0x1034F, -1),
    (0x10375, 0),
    (0x1037A, 8),
    (0x1037F, -1),
    (0x1039D, 0),
    (0x1039E, -1),
    (0x103C3, 0),
    (0x103C7, -1),
    (0x103D5, 0),
    (0x103FF, -1),
    (0x1049D, 0),
    (0x1049F, -1),
    (0x104A9, 0),
    (0x104AF, -1),
    (0x104D3, 0),
    (0x104D7, -1),
    (0x104FB, 0),
    (0x104FF, -1),
    (0x10527, 0),
    (0x1052F, -1),
    (0x10563, 0),
    (0x1056E, -1),
    (0x1057A, 0),
    (0x1057B, -1),
    (0x1058A, 0),
    (0x1058B, -1),
    (0x10592, 0),
    (0x10593, -1),
    (0x10595, 0),
    (0x10596, -1),
    (0x105A1, 0),
    (0x105A2, -1),
    (0x105B1, 0),
    (0x105B2, -1),
    (0x105B9, 0),
    (0x105BA, -1),
    (0x105BC, 0),
    (0x105BF, -1),
    (0x105F3, 0),
    (0x105FF, -1),
    (0x10736, 0),
    (0x1073F, -1),
    (0x10755, 0),
    (0x1075F, -1),
    (0x10767, 0),
    (0x1077F, -1),
    (0x10785, 0),
    (0x10786, -1),
    (0x107B0, 0),
    (0x107B1, -1),
    (0x107BA, 0),
    (0x107FF, -1),
    (0x10805, 1),
    (0x10807, -1),
    (0x10808, 1),
    (0x10809, -1),
    (0x10835, 1),
    (0x10836, -1),
    (0x10838, 1),
    (0x1083B, -1),
    (0x1083C, 1),
    (0x1083E, -1),
    (0x10855, 1),
    (0x10856, -1),
    (0x1089E, 1),
    (0x108A6, -1),
    (0x108AF, 1),
    (0x108DF, -1),
    (0x108F2, 1),
    (0x108F3, -1),
    (0x108F5, 1),
    (0x108FA, -1),
    (0x1091B, 1),
    (0x1091E, -1),
    (0x1091F, 13),
    (0x10939, 1),
    (0x1093E, -1),
    (0x10959, 1),
    (0x1097F, -1),
    (0x109B7, 1),
    (0x109BB, -1),
    (0x109CF, 1),
    (0x109D1, -1),
    (0x10A00, 1),
    (0x10A03, 8),
    (0x10A04, -1),
    (0x10A06, 8),
    (0x10A0B, -1),
    (0x10A0F, 8),
    (0x10A13, 1),
    (0x10A14, -1),
    (0x10A17, 1),
    (0x10A18, -1),
    (0x10A35, 1),
    (0x10A37, -1),
    (0x10A3A, 8),
    (0x10A3E, -1),
    (0x10A3F, 8),
    (0x10A48, 1),
    (0x10A4F, -1),
    (0x10A58, 1),
    (0x10A5F, -1),
    (0x10A9F, 1),
    (0x10ABF, -1),
    (0x10AE4, 1),
    (0x10AE6, 8),
    (0x10AEA, -1),
    (0x10AF6, 1),
    (0x10AFF, -1),
    (0x10B35, 1),
    (0x10B38, -1),
    (0x10B3F, 13),
    (0x10B55, 1),
    (0x10B57, -1),
    (0x10B72, 1),
    (0x10B77, -1),
    (0x10B91, 1),
    (0x10B98, -1),
    (0x10B9C, 1),
    (0x10BA8, -1),
    (0x10BAF, 1),
    (0x10BFF, -1),
    (0x10C48, 1),
    (0x10C7F, -1),
    (0x10CB2, 1),
    (0x10CBF, -1),
    (0x10CF2, 1),
    (0x10CF9, -1),
    (0x10CFF, 1),
    (0x10D23, 2),
    (0x10D27, 8),
    (0x10D2F, -1),
    (0x10D39, 6),
    (0x10D3F, -1),
    (0x10D49, 6),
    (0x10D65, 1),
    (0x10D68, -1),
    (0x10D6D, 8),
    (0x10D6E, 13),
    (0x10D85, 1),
    (0x10D8D, -1),
    (0x10D8F, 1),
    (0x10E5F, -1),
    (0x10E7E, 6),
    (0x10E7F, -1),
    (0x10EA9, 1),
    (0x10EAA, -1),
    (0x10EAC, 8),
    (0x10EAD, 1),
    (0x10EAF, -1),
    (0x10EB1, 1),
    (0x10EC1, -1),
    (0x10EC7, 2),
    (0x10ECF, -1),
    (0x10ED8, 13),
    (0x10EF9, -1),
    (0x10EFF, 8),
    (0x10F27, 1),
    (0x10F2F, -1),
    (0x10F45, 2),
    (0x10F50, 8),
    (0x10F59, 2),
    (0x10F6F, -1),
    (0x10F81, 1),
    (0x10F85, 8),
    (0x10F89, 1),
    (0x10FAF, -1),
    (0x10FCB, 1),
    (0x10FDF, -1),
    (0x10FF6, 1),
    (0x10FFF, -1),
    (0x11000, 0),
    (0x11001, 8),
    (0x11037, 0),
    (0x11046, 8),
    (0x1104D, 0),
    (0x11051, -1),
    (0x11065, 13),
    (0x1106F, 0),
    (0x11070, 8),
    (0x11072, 0),
    (0x11074, 8),
    (0x11075, 0),
    (0x1107E, -1),
    (0x11081, 8),
    (0x110B2, 0),
    (0x110B6, 8),
    (0x110B8, 0),
    (0x110BA, 8),
    (0x110C1, 0),
    (0x110C2, 8),
    (0x110CC, -1),
    (0x110CD, 0),
    (0x110CF, -1),
    (0x110E8, 0),
    (0x110EF, -1),
    (0x110F9, 0),
    (0x110FF, -1),
    (0x11102, 8),
    (0x11126, 0),
    (0x1112B, 8),
    (0x1112C, 0),
    (0x11134, 8),
    (0x11135, -1),
    (0x11147, 0),
    (0x1114F, -1),
    (0x11172, 0),
    (0x11173, 8),
    (0x11176, 0),
    (0x1117F, -1),
    (0x11181, 8),
    (0x111B5, 0),
    (0x111BE, 8),
    (0x111C8, 0),
    (0x111CC, 8),
    (0x111CE, 0),
    (0x111CF, 8),
    (0x111DF, 0),
    (0x111E0, -1),
    (0x111F4, 0),
    (0x111FF, -1),
    (0x11211, 0),
    (0x11212, -1),
    (0x1122E, 0),
    (0x11231, 8),
    (0x11233, 0),
    (0x11234, 8),
    (0x11235, 0),
    (0x11237, 8),
    (0x1123D, 0),
    (0x1123E, 8),
    (0x11240, 0),
    (0x11241, 8),
    (0x1127F, -1),
    (0x11286, 0),
    (0x11287, -1),
    (0x11288, 0),
    (0x11289, -1),
    (0x1128D, 0),
    (0x1128E, -1),
    (0x1129D, 0),
    (0x1129E, -1),
    (0x112A9, 0),
    (0x112AF, -1),
    (0x112DE, 0),
    (0x112DF, 8),
    (0x112E2, 0),
    (0x112EA, 8),
    (0x112EF, -1),
    (0x112F9, 0),
    (0x112FF, -1),
    (0x11301, 8),
    (0x11303, 0),
    (0x11304, -1),
    (0x1130C, 0),
    (0x1130E, -1),
    (0x11310, 0),
    (0x11312, -1),
    (0x11328, 0),
    (0x11329, -1),
    (0x11330, 0),
    (0x11331, -1),
    (0x11333, 0),
    (0x11334, -1),
    (0x11339, 0),
    (0x1133A, -1),
    (0x1133C, 8),
    (0x1133F, 0),
    (0x11340, 8),
    (0x11344, 0),
    (0x11346, -1),
    (0x11348, 0),
    (0x1134A, -1),
    (0x1134D, 0),
    (0x1134F, -1),
    (0x11350, 0),
    (0x11356, -1),
    (0x11357, 0),
    (0x1135C, -1),
    (0x11363, 0),
    (0x11365, -1),
    (0x1136C, 8),
    (0x1136F, -1),
    (0x11374, 8),
    (0x1137F, -1),
    (0x11389, 0),
    (0x1138A, -1),
    (0x1138B, 0),
    (0x1138D, -1),
    (0x1138E, 0),
    (0x1138F, -1),
    (0x113B5, 0),
    (0x113B6, -1),
    (0x113BA, 0),
    (0x113C0, 8),
    (0x113C1, -1),
    (0x113C2, 0),
    (0x113C4, -1),
    (0x113C5, 0),
    (0x113C6, -1),
    (0x113CA, 0),
    (0x113CB, -1),
    (0x113CD, 0),
    (0x113CE, 8),
    (0x113CF, 0),
    (0x113D0, 8),
    (0x113D1, 0),
    (0x113D2, 8),
    (0x113D5, 0),
    (0x113D6, -1),
    (0x113D8, 0),
    (0x113E0, -1),
    (0x113E2, 8),
    (0x113FF, -1),
    (0x11437, 0),
    (0x1143F, 8),
    (0x11441, 0),
    (0x11444, 8),
    (0x11445, 0),
    (0x11446, 8),
    (0x1145B, 0),
    (0x1145C, -1),
    (0x1145D, 0),
    (0x1145E, 8),
    (0x11461, 0),
    (0x1147F, -1),
    (0x114B2, 0),
    (0x114B8, 8),
    (0x114B9, 0),
    (0x114BA, 8),
    (0x114BE, 0),
    (0x114C0, 8),
    (0x114C1, 0),
    (0x114C3, 8),
    (0x114C7, 0),
    (0x114CF, -1),
    (0x114D9, 0),
    (0x1157F, -1),
    (0x115B1, 0),
    (0x115B5, 8),
    (0x115B7, -1),
    (0x115BB, 0),
    (0x115BD, 8),
    (0x115BE, 0),
    (0x115C0, 8),
    (0x115DB, 0),
    (0x115DD, 8),
    (0x115FF, -1),
    (0x11632, 0),
    (0x1163A, 8),
    (0x1163C, 0),
    (0x1163D, 8),
    (0x1163E, 0),
    (0x11640, 8),
    (0x11644, 0),
    (0x1164F, -1),
    (0x11659, 0),
    (0x1165F, -1),
    (0x1166C, 13),
    (0x1167F, -1),
    (0x116AA, 0),
    (0x116AB, 8),
    (0x116AC, 0),
    (0x116AD, 8),
    (0x116AF, 0),
    (0x116B5, 8),
    (0x116B6, 0),
    (0x116B7, 8),
    (0x116B9, 0),
    (0x116BF, -1),
    (0x116C9, 0),
    (0x116CF, -1),
    (0x116E3, 0),
    (0x116FF, -1),
    (0x1171A, 0),
    (0x1171C, -1),
    (0x1171D, 8),
    (0x1171E, 0),
    (0x1171F, 8),
    (0x11721, 0),
    (0x11725, 8),
    (0x11726, 0),
    (0x1172B, 8),
    (0x1172F, -1),
    (0x11746, 0),
    (0x117FF, -1),
    (0x1182E, 0),
    (0x11837, 8),
    (0x11838, 0),
    (0x1183A, 8),
    (0x1183B, 0),
    (0x1189F, -1),
    (0x118F2, 0),
    (0x118FE, -1),
    (0x11906, 0),
    (0x11908, -1),
    (0x11909, 0),
    (0x1190B, -1),
    (0x11913, 0),
    (0x11914, -1),
    (0x11916, 0),
    (0x11917, -1),
    (0x11935, 0),
    (0x11936, -1),
    (0x11938, 0),
    (0x1193A, -1),
    (0x1193C, 8),
    (0x1193D, 0),
    (0x1193E, 8),
    (0x11942, 0),
    (0x11943, 8),
    (0x11946, 0),
    (0x1194F, -1),
    (0x11959, 0),
    (0x1199F, -1),
    (0x119A7, 0),
    (0x119A9, -1),
    (0x119D3, 0),
    (0x119D7, 8),
    (0x119D9, -1),
    (0x119DB, 8),
    (0x119DF, 0),
    (0x119E0, 8),
    (0x119E4, 0),
    (0x119FF, -1),
    (0x11A00, 0),
    (0x11A06, 8),
    (0x11A08, 0),
    (0x11A0A, 8),
    (0x11A32, 0),
    (0x11A38, 8),
    (0x11A3A, 0),
    (0x11A3E, 8),
    (0x11A46, 0),
    (0x11A47, 8),
    (0x11A4F, -1),
    (0x11A50, 0),
    (0x11A56, 8),
    (0x11A58, 0),
    (0x11A5B, 8),
    (0x11A89, 0),
    (0x11A96, 8),
    (0x11A97, 0),
    (0x11A99, 8),
    (0x11AA2, 0),
    (0x11AAF, -1),
    (0x11AF8, 0),
    (0x11AFF, -1),
    (0x11B09, 0),
    (0x11B5F, -1),
    (0x11B60, 8),
    (0x11B61, 0),
    (0x11B64, 8),
    (0x11B65, 0),
    (0x11B66, 8),
    (0x11B67, 0),
    (0x11BBF, -1),
    (0x11BE1, 0),
    (0x11BEF, -1),
    (0x11BF9, 0),
    (0x11BFF, -1),
    (0x11C08, 0),
    (0x11C09, -1),
    (0x11C2F, 0),
    (0x11C36, 8),
    (0x11C37, -1),
    (0x11C3D, 8),
    (0x11C45, 0),
    (0x11C4F, -1),
    (0x11C6C, 0),
    (0x11C6F, -1),
    (0x11C8F, 0),
    (0x11C91, -1),
    (0x11CA7, 8),
    (0x11CA8, -1),
    (0x11CA9, 0),
    (0x11CB0, 8),
    (0x11CB1, 0),
    (0x11CB3, 8),
    (0x11CB4, 0),
    (0x11CB6, 8),
    (0x11CFF, -1),
    (0x11D06, 0),
    (0x11D07, -1),
    (0x11D09, 0),
    (0x11D0A, -1),
    (0x11D30, 0),
    (0x11D36, 8),
    (0x11D39, -1),
    (0x11D3A, 8),
    (0x11D3B, -1),
    (0x11D3D, 8),
    (0x11D3E, -1),
    (0x11D45, 8),
    (0x11D46, 0),
    (0x11D47, 8),
    (0x11D4F, -1),
    (0x11D59, 0),
    (0x11D5F, -1),
    (0x11D65, 0),
    (0x11D66, -1),
    (0x11D68, 0),
    (0x11D69, -1),
    (0x11D8E, 0),
    (0x11D8F, -1),
    (0x11D91, 8),
    (0x11D92, -1),
    (0x11D94, 0),
    (0x11D95, 8),
    (0x11D96, 0),
    (0x11D97, 8),
    (0x11D98, 0),
    (0x11D9F, -1),
    (0x11DA9, 0),
    (0x11DAF, -1),
    (0x11DDB, 0),
    (0x11DDF, -1),
    (0x11DE9, 0),
    (0x11EDF, -1),
    (0x11EF2, 0),
    (0x11EF4, 8),
    (0x11EF8, 0),
    (0x11EFF, -1),
    (0x11F01, 8),
    (0x11F10, 0),
    (0x11F11, -1),
    (0x11F35, 0),
    (0x11F3A, 8),
    (0x11F3D, -1),
    (0x11F3F, 0),
    (0x11F40, 8),
    (0x11F41, 0),
    (0x11F42, 8),
    (0x11F59, 0),
    (0x11F5A, 8),
    (0x11FAF, -1),
    (0x11FB0, 0),
    (0x11FBF, -1),
    (0x11FD4, 0),
    (0x11FDC, 13),
    (0x11FE0, 5),
    (0x11FF1, 13),
    (0x11FFE, -1),
    (0x12399, 0),
    (0x123FF, -1),
    (0x1246E, 0),
    (0x1246F, -1),
    (0x12474, 0),
    (0x1247F, -1),
    (0x12543, 0),
    (0x12F8F, -1),
    (0x12FF2, 0),
    (0x12FFF, -1),
    (0x1343F, 0),
    (0x13440, 8),
    (0x13446, 0),
    (0x13455, 8),
    (0x1345F, -1),
    (0x143FA, 0),
    (0x143FF, -1),
    (0x14646, 0),
    (0x160FF, -1),
    (0x1611D, 0),
    (0x16129, 8),
    (0x1612C, 0),
    (0x1612F, 8),
    (0x16139, 0),
    (0x167FF, -1),
    (0x16A38, 0),
    (0x16A3F, -1),
    (0x16A5E, 0),
    (0x16A5F, -1),
    (0x16A69, 0),
    (0x16A6D, -1),
    (0x16ABE, 0),
    (0x16ABF, -1),
    (0x16AC9, 0),
    (0x16ACF, -1),
    (0x16AED, 0),
    (0x16AEF, -1),
    (0x16AF4, 8),
    (0x16AF5, 0),
    (0x16AFF, -1),
    (0x16B2F, 0),
    (0x16B36, 8),
    (0x16B45, 0),
    (0x16B4F, -1),
    (0x16B59, 0),
    (0x16B5A, -1),
    (0x16B61, 0),
    (0x16B62, -1),
    (0x16B77, 0),
    (0x16B7C, -1),
    (0x16B8F, 0),
    (0x16D3F, -1),
    (0x16D79, 0),
    (0x16E3F, -1),
    (0x16E9A, 0),
    (0x16E9F, -1),
    (0x16EB8, 0),
    (0x16EBA, -1),
    (0x16ED3, 0),
    (0x16EFF, -1),
    (0x16F4A, 0),
    (0x16F4E, -1),
    (0x16F4F, 8),
    (0x16F87, 0),
    (0x16F8E, -1),
    (0x16F92, 8),
    (0x16F9F, 0),
    (0x16FDF, -1),
    (0x16FE1, 0),
    (0x16FE2, 13),
    (0x16FE3, 0),
    (0x16FE4, 8),
    (0x16FEF, -1),
    (0x16FF6, 0),
    (0x16FFF, -1),
    (0x18CD5, 0),
    (0x18CFE, -1),
    (0x18D1E, 0),
    (0x18D7F, -1),
    (0x18DF2, 0),
    (0x1AFEF, -1),
    (0x1AFF3, 0),
    (0x1AFF4, -1),
    (0x1AFFB, 0),
    (0x1AFFC, -1),
    (0x1AFFE, 0),
    (0x1AFFF, -1),
    (0x1B122, 0),
    (0x1B131, -1),
    (0x1B132, 0),
    (0x1B14F, -1),
    (0x1B152, 0),
    (0x1B154, -1),
    (0x1B155, 0),
    (0x1B163, -1),
    (0x1B167, 0),
    (0x1B16F, -1),
    (0x1B2FB, 0),
    (0x1BBFF, -1),
    (0x1BC6A, 0),
    (0x1BC6F, -1),
    (0x1BC7C, 0),
    (0x1BC7F, -1),
    (0x1BC88, 0),
    (0x1BC8F, -1),
    (0x1BC99, 0),
    (0x1BC9B, -1),
    (0x1BC9C, 0),
    (0x1BC9E, 8),
    (0x1BC9F, 0),
    (0x1BCA3, 9),
    (0x1CBFF, -1),
    (0x1CCD5, 13),
    (0x1CCEF, 0),
    (0x1CCF9, 3),
    (0x1CCFC, 13),
    (0x1CCFF, -1),
    (0x1CEB3, 13),
    (0x1CEB9, -1),
    (0x1CED0, 13),
    (0x1CEDF, -1),
    (0x1CEF0, 13),
    (0x1CEFF, -1),
    (0x1CF2D, 8),
    (0x1CF2F, -1),
    (0x1CF46, 8),
    (0x1CF4F, -1),
    (0x1CFC3, 0),
    (0x1CFFF, -1),
    (0x1D0F5, 0),
    (0x1D0FF, -1),
    (0x1D126, 0),
    (0x1D128, -1),
    (0x1D166, 0),
    (0x1D169, 8),
    (0x1D172, 0),
    (0x1D17A, 9),
    (0x1D182, 8),
    (0x1D184, 0),
    (0x1D18B, 8),
    (0x1D1A9, 0),
    (0x1D1AD, 8),
    (0x1D1E8, 0),
    (0x1D1EA, 13),
    (0x1D1FF, -1),
    (0x1D241, 13),
    (0x1D244, 8),
    (0x1D245, 13),
    (0x1D2BF, -1),
    (0x1D2D3, 0),
    (0x1D2DF, -1),
    (0x1D2F3, 0),
    (0x1D2FF, -1),
    (0x1D356, 13),
    (0x1D35F, -1),
    (0x1D378, 0),
    (0x1D3FF, -1),
    (0x1D454, 0),
    (0x1D455, -1),
    (0x1D49C, 0),
    (0x1D49D, -1),
    (0x1D49F, 0),
    (0x1D4A1, -1),
    (0x1D4A2, 0),
    (0x1D4A4, -1),
    (0x1D4A6, 0),
    (0x1D4A8, -1),
    (0x1D4AC, 0),
    (0x1D4AD, -1),
    (0x1D4B9, 0),
    (0x1D4BA, -1),
    (0x1D4BB, 0),
    (0x1D4BC, -1),
    (0x1D4C3, 0),
    (0x1D4C4, -1),
    (0x1D505, 0),
    (0x1D506, -1),
    (0x1D50A, 0),
    (0x1D50C, -1),
    (0x1D514, 0),
    (0x1D515, -1),
    (0x1D51C, 0),
    (0x1D51D, -1),
    (0x1D539, 0),
    (0x1D53A, -1),
    (0x1D53E, 0),
    (0x1D53F, -1),
    (0x1D544, 0),
    (0x1D545, -1),
    (0x1D546, 0),
    (0x1D549, -1),
    (0x1D550, 0),
    (0x1D551, -1),
    (0x1D6A5, 0),
    (0x1D6A7, -1),
    (0x1D6C0, 0),
    (0x1D6C1, 13),
    (0x1D6DA, 0),
    (0x1D6DB, 13),
    (0x1D6FA, 0),
    (0x1D6FB, 13),
    (0x1D714, 0),
    (0x1D715, 13),
    (0x1D734, 0),
    (0x1D735, 13),
    (0x1D74E, 0),
    (0x1D74F, 13),
    (0x1D76E, 0),
    (0x1D76F, 13),
    (0x1D788, 0),
    (0x1D789, 13),
    (0x1D7A8, 0),
    (0x1D7A9, 13),
    (0x1D7C2, 0),
    (0x1D7C3, 13),
    (0x1D7CB, 0),
    (0x1D7CD, -1),
    (0x1D7FF, 3),
    (0x1D9FF, 0),
    (0x1DA36, 8),
    (0x1DA3A, 0),
    (0x1DA6C, 8),
    (0x1DA74, 0),
    (0x1DA75, 8),
    (0x1DA83, 0),
    (0x1DA84, 8),
    (0x1DA8B, 0),
    (0x1DA9A, -1),
    (0x1DA9F, 8),
    (0x1DAA0, -1),
    (0x1DAAF, 8),
    (0x1DEFF, -1),
    (0x1DF1E, 0),
    (0x1DF24, -1),
    (0x1DF2A, 0),
    (0x1DFFF, -1),
    (0x1E006, 8),
    (0x1E007, -1),
    (0x1E018, 8),
    (0x1E01A, -1),
    (0x1E021, 8),
    (0x1E022, -1),
    (0x1E024, 8),
    (0x1E025, -1),
    (0x1E02A, 8),
    (0x1E02F, -1),
    (0x1E06D, 0),
    (0x1E08E, -1),
    (0x1E08F, 8),
    (0x1E0FF, -1),
    (0x1E12C, 0),
    (0x1E12F, -1),
    (0x1E136, 8),
    (0x1E13D, 0),
    (0x1E13F, -1),
    (0x1E149, 0),
    (0x1E14D, -1),
    (0x1E14F, 0),
    (0x1E28F, -1),
    (0x1E2AD, 0),
    (0x1E2AE, 8),
    (0x1E2BF, -1),
    (0x1E2EB, 0),
    (0x1E2EF, 8),
    (0x1E2F9, 0),
    (0x1E2FE, -1),
    (0x1E2FF, 5),
    (0x1E4CF, -1),
    (0x1E4EB, 0),
    (0x1E4EF, 8),
    (0x1E4F9, 0),
    (0x1E5CF, -1),
    (0x1E5ED, 0),
    (0x1E5EF, 8),
    (0x1E5FA, 0),
    (0x1E5FE, -1),
    (0x1E5FF, 0),
    (0x1E6BF, -1),
    (0x1E6DE, 0),
    (0x1E6DF, -1),
    (0x1E6E2, 0),
    (0x1E6E3, 8),
    (0x1E6E5, 0),
    (0x1E6E6, 8),
    (0x1E6ED, 0),
    (0x1E6EF, 8),
    (0x1E6F4, 0),
    (0x1E6F5, 8),
    (0x1E6FD, -1),
    (0x1E6FF, 0),
    (0x1E7DF, -1),
    (0x1E7E6, 0),
    (0x1E7E7, -1),
    (0x1E7EB, 0),
    (0x1E7EC, -1),
    (0x1E7EE, 0),
    (0x1E7EF, -1),
    (0x1E7FE, 0),
    (0x1E7FF, -1),
    (0x1E8C4, 1),
    (0x1E8C6, -1),
    (0x1E8CF, 1),
    (0x1E8D6, 8),
    (0x1E8FF, -1),
    (0x1E943, 1),
    (0x1E94A, 8),
    (0x1E94B, 1),
    (0x1E94F, -1),
    (0x1E959, 1),
    (0x1E95D, -1),
    (0x1E95F, 1),
    (0x1EC70, -1),
    (0x1ECB4, 2),
    (0x1ED00, -1),
    (0x1ED3D, 2),
    (0x1EDFF, -1),
    (0x1EE03, 2),
    (0x1EE04, -1),
    (0x1EE1F, 2),
    (0x1EE20, -1),
    (0x1EE22, 2),
    (0x1EE23, -1),
    (0x1EE24, 2),
    (0x1EE26, -1),
    (0x1EE27, 2),
    (0x1EE28, -1),
    (0x1EE32, 2),
    (0x1EE33, -1),
    (0x1EE37, 2),
    (0x1EE38, -1),
    (0x1EE39, 2),
    (0x1EE3A, -1),
    (0x1EE3B, 2),
    (0x1EE41, -1),
    (0x1EE42, 2),
    (0x1EE46, -1),
    (0x1EE47, 2),
    (0x1EE48, -1),
    (0x1EE49, 2),
    (0x1EE4A, -1),
    (0x1EE4B, 2),
    (0x1EE4C, -1),
    (0x1EE4F, 2),
    (0x1EE50, -1),
    (0x1EE52, 2),
    (0x1EE53, -1),
    (0x1EE54, 2),
    (0x1EE56, -1),
    (0x1EE57, 2),
    (0x1EE58, -1),
    (0x1EE59, 2),
    (0x1EE5A, -1),
    (0x1EE5B, 2),
    (0x1EE5C, -1),
    (0x1EE5D, 2),
    (0x1EE5E, -1),
    (0x1EE5F, 2),
    (0x1EE60, -1),
    (0x1EE62, 2),
    (0x1EE63, -1),
    (0x1EE64, 2),
    (0x1EE66, -1),
    (0x1EE6A, 2),
    (0x1EE6B, -1),
    (0x1EE72, 2),
    (0x1EE73, -1),
    (0x1EE77, 2),
    (0x1EE78, -1),
    (0x1EE7C, 2),
    (0x1EE7D, -1),
    (0x1EE7E, 2),
    (0x1EE7F, -1),
    (0x1EE89, 2),
    (0x1EE8A, -1),
    (0x1EE9B, 2),
    (0x1EEA0, -1),
    (0x1EEA3, 2),
    (0x1EEA4, -1),
    (0x1EEA9, 2),
    (0x1EEAA, -1),
    (0x1EEBB, 2),
    (0x1EEEF, -1),
    (0x1EEF1, 13),
    (0x1EFFF, -1),
    (0x1F02B, 13),
    (0x1F02F, -1),
    (0x1F093, 13),
    (0x1F09F, -1),
    (0x1F0AE, 13),
    (0x1F0B0, -1),
    (0x1F0BF, 13),
    (0x1F0C0, -1),
    (0x1F0CF, 13),
    (0x1F0D0, -1),
    (0x1F0F5, 13),
    (0x1F0FF, -1),
    (0x1F10A, 3),
    (0x1F10F, 13),
    (0x1F12E, 0),
    (0x1F12F, 13),
    (0x1F169, 0),
    (0x1F16F, 13),
    (0x1F1AC, 0),
    (0x1F1AD, 13),
    (0x1F1E5, -1),
    (0x1F202, 0),
    (0x1F20F, -1),
    (0x1F23B, 0),
    (0x1F23F, -1),
    (0x1F248, 0),
    (0x1F24F, -1),
    (0x1F251, 0),
    (0x1F25F, -1),
    (0x1F265, 13),
    (0x1F2FF, -1),
    (0x1F6D8, 13),
    (0x1F6DB, -1),
    (0x1F6EC, 13),
    (0x1F6EF, -1),
    (0x1F6FC, 13),
    (0x1F6FF, -1),
    (0x1F7D9, 13),
    (0x1F7DF, -1),
    (0x1F7EB, 13),
    (0x1F7EF, -1),
    (0x1F7F0, 13),
    (0x1F7FF, -1),
    (0x1F80B, 13),
    (0x1F80F, -1),
    (0x1F847, 13),
    (0x1F84F, -1),
    (0x1F859, 13),
    (0x1F85F, -1),
    (0x1F887, 13),
    (0x1F88F, -1),
    (0x1F8AD, 13),
    (0x1F8AF, -1),
    (0x1F8BB, 13),
    (0x1F8BF, -1),
    (0x1F8C1, 13),
    (0x1F8CF, -1),
    (0x1F8D8, 13),
    (0x1F8FF, -1),
    (0x1FA57, 13),
    (0x1FA5F, -1),
    (0x1FA6D, 13),
    (0x1FA6F, -1),
    (0x1FA7C, 13),
    (0x1FA7F, -1),
    (0x1FA8A, 13),
    (0x1FA8D, -1),
    (0x1FAC6, 13),
    (0x1FAC7, -1),
    (0x1FAC8, 13),
    (0x1FACC, -1),
    (0x1FADC, 13),
    (0x1FADE, -1),
    (0x1FAEA, 13),
    (0x1FAEE, -1),
    (0x1FAF8, 13),
    (0x1FAFF, -1),
    (0x1FB92, 13),
    (0x1FB93, -1),
    (0x1FBEF, 13),
    (0x1FBF9, 3),
    (0x1FBFA, 13),
    (0x1FFFF, -1),
    (0x2A6DF, 0),
    (0x2A6FF, -1),
    (0x2B81D, 0),
    (0x2B81F, -1),
    (0x2CEAD, 0),
    (0x2CEAF, -1),
    (0x2EBE0, 0),
    (0x2EBEF, -1),
    (0x2EE5D, 0),
    (0x2F7FF, -1),
    (0x2FA1D, 0),
    (0x2FFFF, -1),
    (0x3134A, 0),
    (0x3134F, -1),
    (0x33479, 0),
    (0xE0000, -1),
    (0xE0001, 9),
    (0xE001F, -1),
    (0xE007F, 9),
    (0xE00FF, -1),
    (0xE01EF, 8),
    (0xEFFFF, -1),
    (0xFFFFD, 0),
    (0xFFFFF, -1),
    (0x10FFFD, 0),
    (0x10FFFF, -1),
];

/// Code point ranges that are defined in Unicode.
const DEFINED_RANGES: [(u32, u32); 735] = [
    (0x0, 0x377),
    (0x37A, 0x37F),
    (0x384, 0x38A),
    (0x38C, 0x38C),
    (0x38E, 0x3A1),
    (0x3A3, 0x52F),
    (0x531, 0x556),
    (0x559, 0x58A),
    (0x58D, 0x58F),
    (0x591, 0x5C7),
    (0x5D0, 0x5EA),
    (0x5EF, 0x5F4),
    (0x600, 0x70D),
    (0x70F, 0x74A),
    (0x74D, 0x7B1),
    (0x7C0, 0x7FA),
    (0x7FD, 0x82D),
    (0x830, 0x83E),
    (0x840, 0x85B),
    (0x85E, 0x85E),
    (0x860, 0x86A),
    (0x870, 0x891),
    (0x897, 0x983),
    (0x985, 0x98C),
    (0x98F, 0x990),
    (0x993, 0x9A8),
    (0x9AA, 0x9B0),
    (0x9B2, 0x9B2),
    (0x9B6, 0x9B9),
    (0x9BC, 0x9C4),
    (0x9C7, 0x9C8),
    (0x9CB, 0x9CE),
    (0x9D7, 0x9D7),
    (0x9DC, 0x9DD),
    (0x9DF, 0x9E3),
    (0x9E6, 0x9FE),
    (0xA01, 0xA03),
    (0xA05, 0xA0A),
    (0xA0F, 0xA10),
    (0xA13, 0xA28),
    (0xA2A, 0xA30),
    (0xA32, 0xA33),
    (0xA35, 0xA36),
    (0xA38, 0xA39),
    (0xA3C, 0xA3C),
    (0xA3E, 0xA42),
    (0xA47, 0xA48),
    (0xA4B, 0xA4D),
    (0xA51, 0xA51),
    (0xA59, 0xA5C),
    (0xA5E, 0xA5E),
    (0xA66, 0xA76),
    (0xA81, 0xA83),
    (0xA85, 0xA8D),
    (0xA8F, 0xA91),
    (0xA93, 0xAA8),
    (0xAAA, 0xAB0),
    (0xAB2, 0xAB3),
    (0xAB5, 0xAB9),
    (0xABC, 0xAC5),
    (0xAC7, 0xAC9),
    (0xACB, 0xACD),
    (0xAD0, 0xAD0),
    (0xAE0, 0xAE3),
    (0xAE6, 0xAF1),
    (0xAF9, 0xAFF),
    (0xB01, 0xB03),
    (0xB05, 0xB0C),
    (0xB0F, 0xB10),
    (0xB13, 0xB28),
    (0xB2A, 0xB30),
    (0xB32, 0xB33),
    (0xB35, 0xB39),
    (0xB3C, 0xB44),
    (0xB47, 0xB48),
    (0xB4B, 0xB4D),
    (0xB55, 0xB57),
    (0xB5C, 0xB5D),
    (0xB5F, 0xB63),
    (0xB66, 0xB77),
    (0xB82, 0xB83),
    (0xB85, 0xB8A),
    (0xB8E, 0xB90),
    (0xB92, 0xB95),
    (0xB99, 0xB9A),
    (0xB9C, 0xB9C),
    (0xB9E, 0xB9F),
    (0xBA3, 0xBA4),
    (0xBA8, 0xBAA),
    (0xBAE, 0xBB9),
    (0xBBE, 0xBC2),
    (0xBC6, 0xBC8),
    (0xBCA, 0xBCD),
    (0xBD0, 0xBD0),
    (0xBD7, 0xBD7),
    (0xBE6, 0xBFA),
    (0xC00, 0xC0C),
    (0xC0E, 0xC10),
    (0xC12, 0xC28),
    (0xC2A, 0xC39),
    (0xC3C, 0xC44),
    (0xC46, 0xC48),
    (0xC4A, 0xC4D),
    (0xC55, 0xC56),
    (0xC58, 0xC5A),
    (0xC5C, 0xC5D),
    (0xC60, 0xC63),
    (0xC66, 0xC6F),
    (0xC77, 0xC8C),
    (0xC8E, 0xC90),
    (0xC92, 0xCA8),
    (0xCAA, 0xCB3),
    (0xCB5, 0xCB9),
    (0xCBC, 0xCC4),
    (0xCC6, 0xCC8),
    (0xCCA, 0xCCD),
    (0xCD5, 0xCD6),
    (0xCDC, 0xCDE),
    (0xCE0, 0xCE3),
    (0xCE6, 0xCEF),
    (0xCF1, 0xCF3),
    (0xD00, 0xD0C),
    (0xD0E, 0xD10),
    (0xD12, 0xD44),
    (0xD46, 0xD48),
    (0xD4A, 0xD4F),
    (0xD54, 0xD63),
    (0xD66, 0xD7F),
    (0xD81, 0xD83),
    (0xD85, 0xD96),
    (0xD9A, 0xDB1),
    (0xDB3, 0xDBB),
    (0xDBD, 0xDBD),
    (0xDC0, 0xDC6),
    (0xDCA, 0xDCA),
    (0xDCF, 0xDD4),
    (0xDD6, 0xDD6),
    (0xDD8, 0xDDF),
    (0xDE6, 0xDEF),
    (0xDF2, 0xDF4),
    (0xE01, 0xE3A),
    (0xE3F, 0xE5B),
    (0xE81, 0xE82),
    (0xE84, 0xE84),
    (0xE86, 0xE8A),
    (0xE8C, 0xEA3),
    (0xEA5, 0xEA5),
    (0xEA7, 0xEBD),
    (0xEC0, 0xEC4),
    (0xEC6, 0xEC6),
    (0xEC8, 0xECE),
    (0xED0, 0xED9),
    (0xEDC, 0xEDF),
    (0xF00, 0xF47),
    (0xF49, 0xF6C),
    (0xF71, 0xF97),
    (0xF99, 0xFBC),
    (0xFBE, 0xFCC),
    (0xFCE, 0xFDA),
    (0x1000, 0x10C5),
    (0x10C7, 0x10C7),
    (0x10CD, 0x10CD),
    (0x10D0, 0x1248),
    (0x124A, 0x124D),
    (0x1250, 0x1256),
    (0x1258, 0x1258),
    (0x125A, 0x125D),
    (0x1260, 0x1288),
    (0x128A, 0x128D),
    (0x1290, 0x12B0),
    (0x12B2, 0x12B5),
    (0x12B8, 0x12BE),
    (0x12C0, 0x12C0),
    (0x12C2, 0x12C5),
    (0x12C8, 0x12D6),
    (0x12D8, 0x1310),
    (0x1312, 0x1315),
    (0x1318, 0x135A),
    (0x135D, 0x137C),
    (0x1380, 0x1399),
    (0x13A0, 0x13F5),
    (0x13F8, 0x13FD),
    (0x1400, 0x169C),
    (0x16A0, 0x16F8),
    (0x1700, 0x1715),
    (0x171F, 0x1736),
    (0x1740, 0x1753),
    (0x1760, 0x176C),
    (0x176E, 0x1770),
    (0x1772, 0x1773),
    (0x1780, 0x17DD),
    (0x17E0, 0x17E9),
    (0x17F0, 0x17F9),
    (0x1800, 0x1819),
    (0x1820, 0x1878),
    (0x1880, 0x18AA),
    (0x18B0, 0x18F5),
    (0x1900, 0x191E),
    (0x1920, 0x192B),
    (0x1930, 0x193B),
    (0x1940, 0x1940),
    (0x1944, 0x196D),
    (0x1970, 0x1974),
    (0x1980, 0x19AB),
    (0x19B0, 0x19C9),
    (0x19D0, 0x19DA),
    (0x19DE, 0x1A1B),
    (0x1A1E, 0x1A5E),
    (0x1A60, 0x1A7C),
    (0x1A7F, 0x1A89),
    (0x1A90, 0x1A99),
    (0x1AA0, 0x1AAD),
    (0x1AB0, 0x1ADD),
    (0x1AE0, 0x1AEB),
    (0x1B00, 0x1B4C),
    (0x1B4E, 0x1BF3),
    (0x1BFC, 0x1C37),
    (0x1C3B, 0x1C49),
    (0x1C4D, 0x1C8A),
    (0x1C90, 0x1CBA),
    (0x1CBD, 0x1CC7),
    (0x1CD0, 0x1CFA),
    (0x1D00, 0x1F15),
    (0x1F18, 0x1F1D),
    (0x1F20, 0x1F45),
    (0x1F48, 0x1F4D),
    (0x1F50, 0x1F57),
    (0x1F59, 0x1F59),
    (0x1F5B, 0x1F5B),
    (0x1F5D, 0x1F5D),
    (0x1F5F, 0x1F7D),
    (0x1F80, 0x1FB4),
    (0x1FB6, 0x1FC4),
    (0x1FC6, 0x1FD3),
    (0x1FD6, 0x1FDB),
    (0x1FDD, 0x1FEF),
    (0x1FF2, 0x1FF4),
    (0x1FF6, 0x1FFE),
    (0x2000, 0x2064),
    (0x2066, 0x2071),
    (0x2074, 0x208E),
    (0x2090, 0x209C),
    (0x20A0, 0x20C1),
    (0x20D0, 0x20F0),
    (0x2100, 0x218B),
    (0x2190, 0x2429),
    (0x2440, 0x244A),
    (0x2460, 0x2B73),
    (0x2B76, 0x2CF3),
    (0x2CF9, 0x2D25),
    (0x2D27, 0x2D27),
    (0x2D2D, 0x2D2D),
    (0x2D30, 0x2D67),
    (0x2D6F, 0x2D70),
    (0x2D7F, 0x2D96),
    (0x2DA0, 0x2DA6),
    (0x2DA8, 0x2DAE),
    (0x2DB0, 0x2DB6),
    (0x2DB8, 0x2DBE),
    (0x2DC0, 0x2DC6),
    (0x2DC8, 0x2DCE),
    (0x2DD0, 0x2DD6),
    (0x2DD8, 0x2DDE),
    (0x2DE0, 0x2E5D),
    (0x2E80, 0x2E99),
    (0x2E9B, 0x2EF3),
    (0x2F00, 0x2FD5),
    (0x2FF0, 0x303F),
    (0x3041, 0x3096),
    (0x3099, 0x30FF),
    (0x3105, 0x312F),
    (0x3131, 0x318E),
    (0x3190, 0x31E5),
    (0x31EF, 0x321E),
    (0x3220, 0xA48C),
    (0xA490, 0xA4C6),
    (0xA4D0, 0xA62B),
    (0xA640, 0xA6F7),
    (0xA700, 0xA7DC),
    (0xA7F1, 0xA82C),
    (0xA830, 0xA839),
    (0xA840, 0xA877),
    (0xA880, 0xA8C5),
    (0xA8CE, 0xA8D9),
    (0xA8E0, 0xA953),
    (0xA95F, 0xA97C),
    (0xA980, 0xA9CD),
    (0xA9CF, 0xA9D9),
    (0xA9DE, 0xA9FE),
    (0xAA00, 0xAA36),
    (0xAA40, 0xAA4D),
    (0xAA50, 0xAA59),
    (0xAA5C, 0xAAC2),
    (0xAADB, 0xAAF6),
    (0xAB01, 0xAB06),
    (0xAB09, 0xAB0E),
    (0xAB11, 0xAB16),
    (0xAB20, 0xAB26),
    (0xAB28, 0xAB2E),
    (0xAB30, 0xAB6B),
    (0xAB70, 0xABED),
    (0xABF0, 0xABF9),
    (0xAC00, 0xD7A3),
    (0xD7B0, 0xD7C6),
    (0xD7CB, 0xD7FB),
    (0xD800, 0xFA6D),
    (0xFA70, 0xFAD9),
    (0xFB00, 0xFB06),
    (0xFB13, 0xFB17),
    (0xFB1D, 0xFB36),
    (0xFB38, 0xFB3C),
    (0xFB3E, 0xFB3E),
    (0xFB40, 0xFB41),
    (0xFB43, 0xFB44),
    (0xFB46, 0xFDCF),
    (0xFDF0, 0xFE19),
    (0xFE20, 0xFE52),
    (0xFE54, 0xFE66),
    (0xFE68, 0xFE6B),
    (0xFE70, 0xFE74),
    (0xFE76, 0xFEFC),
    (0xFEFF, 0xFEFF),
    (0xFF01, 0xFFBE),
    (0xFFC2, 0xFFC7),
    (0xFFCA, 0xFFCF),
    (0xFFD2, 0xFFD7),
    (0xFFDA, 0xFFDC),
    (0xFFE0, 0xFFE6),
    (0xFFE8, 0xFFEE),
    (0xFFF9, 0xFFFD),
    (0x10000, 0x1000B),
    (0x1000D, 0x10026),
    (0x10028, 0x1003A),
    (0x1003C, 0x1003D),
    (0x1003F, 0x1004D),
    (0x10050, 0x1005D),
    (0x10080, 0x100FA),
    (0x10100, 0x10102),
    (0x10107, 0x10133),
    (0x10137, 0x1018E),
    (0x10190, 0x1019C),
    (0x101A0, 0x101A0),
    (0x101D0, 0x101FD),
    (0x10280, 0x1029C),
    (0x102A0, 0x102D0),
    (0x102E0, 0x102FB),
    (0x10300, 0x10323),
    (0x1032D, 0x1034A),
    (0x10350, 0x1037A),
    (0x10380, 0x1039D),
    (0x1039F, 0x103C3),
    (0x103C8, 0x103D5),
    (0x10400, 0x1049D),
    (0x104A0, 0x104A9),
    (0x104B0, 0x104D3),
    (0x104D8, 0x104FB),
    (0x10500, 0x10527),
    (0x10530, 0x10563),
    (0x1056F, 0x1057A),
    (0x1057C, 0x1058A),
    (0x1058C, 0x10592),
    (0x10594, 0x10595),
    (0x10597, 0x105A1),
    (0x105A3, 0x105B1),
    (0x105B3, 0x105B9),
    (0x105BB, 0x105BC),
    (0x105C0, 0x105F3),
    (0x10600, 0x10736),
    (0x10740, 0x10755),
    (0x10760, 0x10767),
    (0x10780, 0x10785),
    (0x10787, 0x107B0),
    (0x107B2, 0x107BA),
    (0x10800, 0x10805),
    (0x10808, 0x10808),
    (0x1080A, 0x10835),
    (0x10837, 0x10838),
    (0x1083C, 0x1083C),
    (0x1083F, 0x10855),
    (0x10857, 0x1089E),
    (0x108A7, 0x108AF),
    (0x108E0, 0x108F2),
    (0x108F4, 0x108F5),
    (0x108FB, 0x1091B),
    (0x1091F, 0x10939),
    (0x1093F, 0x10959),
    (0x10980, 0x109B7),
    (0x109BC, 0x109CF),
    (0x109D2, 0x10A03),
    (0x10A05, 0x10A06),
    (0x10A0C, 0x10A13),
    (0x10A15, 0x10A17),
    (0x10A19, 0x10A35),
    (0x10A38, 0x10A3A),
    (0x10A3F, 0x10A48),
    (0x10A50, 0x10A58),
    (0x10A60, 0x10A9F),
    (0x10AC0, 0x10AE6),
    (0x10AEB, 0x10AF6),
    (0x10B00, 0x10B35),
    (0x10B39, 0x10B55),
    (0x10B58, 0x10B72),
    (0x10B78, 0x10B91),
    (0x10B99, 0x10B9C),
    (0x10BA9, 0x10BAF),
    (0x10C00, 0x10C48),
    (0x10C80, 0x10CB2),
    (0x10CC0, 0x10CF2),
    (0x10CFA, 0x10D27),
    (0x10D30, 0x10D39),
    (0x10D40, 0x10D65),
    (0x10D69, 0x10D85),
    (0x10D8E, 0x10D8F),
    (0x10E60, 0x10E7E),
    (0x10E80, 0x10EA9),
    (0x10EAB, 0x10EAD),
    (0x10EB0, 0x10EB1),
    (0x10EC2, 0x10EC7),
    (0x10ED0, 0x10ED8),
    (0x10EFA, 0x10F27),
    (0x10F30, 0x10F59),
    (0x10F70, 0x10F89),
    (0x10FB0, 0x10FCB),
    (0x10FE0, 0x10FF6),
    (0x11000, 0x1104D),
    (0x11052, 0x11075),
    (0x1107F, 0x110C2),
    (0x110CD, 0x110CD),
    (0x110D0, 0x110E8),
    (0x110F0, 0x110F9),
    (0x11100, 0x11134),
    (0x11136, 0x11147),
    (0x11150, 0x11176),
    (0x11180, 0x111DF),
    (0x111E1, 0x111F4),
    (0x11200, 0x11211),
    (0x11213, 0x11241),
    (0x11280, 0x11286),
    (0x11288, 0x11288),
    (0x1128A, 0x1128D),
    (0x1128F, 0x1129D),
    (0x1129F, 0x112A9),
    (0x112B0, 0x112EA),
    (0x112F0, 0x112F9),
    (0x11300, 0x11303),
    (0x11305, 0x1130C),
    (0x1130F, 0x11310),
    (0x11313, 0x11328),
    (0x1132A, 0x11330),
    (0x11332, 0x11333),
    (0x11335, 0x11339),
    (0x1133B, 0x11344),
    (0x11347, 0x11348),
    (0x1134B, 0x1134D),
    (0x11350, 0x11350),
    (0x11357, 0x11357),
    (0x1135D, 0x11363),
    (0x11366, 0x1136C),
    (0x11370, 0x11374),
    (0x11380, 0x11389),
    (0x1138B, 0x1138B),
    (0x1138E, 0x1138E),
    (0x11390, 0x113B5),
    (0x113B7, 0x113C0),
    (0x113C2, 0x113C2),
    (0x113C5, 0x113C5),
    (0x113C7, 0x113CA),
    (0x113CC, 0x113D5),
    (0x113D7, 0x113D8),
    (0x113E1, 0x113E2),
    (0x11400, 0x1145B),
    (0x1145D, 0x11461),
    (0x11480, 0x114C7),
    (0x114D0, 0x114D9),
    (0x11580, 0x115B5),
    (0x115B8, 0x115DD),
    (0x11600, 0x11644),
    (0x11650, 0x11659),
    (0x11660, 0x1166C),
    (0x11680, 0x116B9),
    (0x116C0, 0x116C9),
    (0x116D0, 0x116E3),
    (0x11700, 0x1171A),
    (0x1171D, 0x1172B),
    (0x11730, 0x11746),
    (0x11800, 0x1183B),
    (0x118A0, 0x118F2),
    (0x118FF, 0x11906),
    (0x11909, 0x11909),
    (0x1190C, 0x11913),
    (0x11915, 0x11916),
    (0x11918, 0x11935),
    (0x11937, 0x11938),
    (0x1193B, 0x11946),
    (0x11950, 0x11959),
    (0x119A0, 0x119A7),
    (0x119AA, 0x119D7),
    (0x119DA, 0x119E4),
    (0x11A00, 0x11A47),
    (0x11A50, 0x11AA2),
    (0x11AB0, 0x11AF8),
    (0x11B00, 0x11B09),
    (0x11B60, 0x11B67),
    (0x11BC0, 0x11BE1),
    (0x11BF0, 0x11BF9),
    (0x11C00, 0x11C08),
    (0x11C0A, 0x11C36),
    (0x11C38, 0x11C45),
    (0x11C50, 0x11C6C),
    (0x11C70, 0x11C8F),
    (0x11C92, 0x11CA7),
    (0x11CA9, 0x11CB6),
    (0x11D00, 0x11D06),
    (0x11D08, 0x11D09),
    (0x11D0B, 0x11D36),
    (0x11D3A, 0x11D3A),
    (0x11D3C, 0x11D3D),
    (0x11D3F, 0x11D47),
    (0x11D50, 0x11D59),
    (0x11D60, 0x11D65),
    (0x11D67, 0x11D68),
    (0x11D6A, 0x11D8E),
    (0x11D90, 0x11D91),
    (0x11D93, 0x11D98),
    (0x11DA0, 0x11DA9),
    (0x11DB0, 0x11DDB),
    (0x11DE0, 0x11DE9),
    (0x11EE0, 0x11EF8),
    (0x11F00, 0x11F10),
    (0x11F12, 0x11F3A),
    (0x11F3E, 0x11F5A),
    (0x11FB0, 0x11FB0),
    (0x11FC0, 0x11FF1),
    (0x11FFF, 0x12399),
    (0x12400, 0x1246E),
    (0x12470, 0x12474),
    (0x12480, 0x12543),
    (0x12F90, 0x12FF2),
    (0x13000, 0x13455),
    (0x13460, 0x143FA),
    (0x14400, 0x14646),
    (0x16100, 0x16139),
    (0x16800, 0x16A38),
    (0x16A40, 0x16A5E),
    (0x16A60, 0x16A69),
    (0x16A6E, 0x16ABE),
    (0x16AC0, 0x16AC9),
    (0x16AD0, 0x16AED),
    (0x16AF0, 0x16AF5),
    (0x16B00, 0x16B45),
    (0x16B50, 0x16B59),
    (0x16B5B, 0x16B61),
    (0x16B63, 0x16B77),
    (0x16B7D, 0x16B8F),
    (0x16D40, 0x16D79),
    (0x16E40, 0x16E9A),
    (0x16EA0, 0x16EB8),
    (0x16EBB, 0x16ED3),
    (0x16F00, 0x16F4A),
    (0x16F4F, 0x16F87),
    (0x16F8F, 0x16F9F),
    (0x16FE0, 0x16FE4),
    (0x16FF0, 0x16FF6),
    (0x17000, 0x18CD5),
    (0x18CFF, 0x18D1E),
    (0x18D80, 0x18DF2),
    (0x1AFF0, 0x1AFF3),
    (0x1AFF5, 0x1AFFB),
    (0x1AFFD, 0x1AFFE),
    (0x1B000, 0x1B122),
    (0x1B132, 0x1B132),
    (0x1B150, 0x1B152),
    (0x1B155, 0x1B155),
    (0x1B164, 0x1B167),
    (0x1B170, 0x1B2FB),
    (0x1BC00, 0x1BC6A),
    (0x1BC70, 0x1BC7C),
    (0x1BC80, 0x1BC88),
    (0x1BC90, 0x1BC99),
    (0x1BC9C, 0x1BCA3),
    (0x1CC00, 0x1CCFC),
    (0x1CD00, 0x1CEB3),
    (0x1CEBA, 0x1CED0),
    (0x1CEE0, 0x1CEF0),
    (0x1CF00, 0x1CF2D),
    (0x1CF30, 0x1CF46),
    (0x1CF50, 0x1CFC3),
    (0x1D000, 0x1D0F5),
    (0x1D100, 0x1D126),
    (0x1D129, 0x1D1EA),
    (0x1D200, 0x1D245),
    (0x1D2C0, 0x1D2D3),
    (0x1D2E0, 0x1D2F3),
    (0x1D300, 0x1D356),
    (0x1D360, 0x1D378),
    (0x1D400, 0x1D454),
    (0x1D456, 0x1D49C),
    (0x1D49E, 0x1D49F),
    (0x1D4A2, 0x1D4A2),
    (0x1D4A5, 0x1D4A6),
    (0x1D4A9, 0x1D4AC),
    (0x1D4AE, 0x1D4B9),
    (0x1D4BB, 0x1D4BB),
    (0x1D4BD, 0x1D4C3),
    (0x1D4C5, 0x1D505),
    (0x1D507, 0x1D50A),
    (0x1D50D, 0x1D514),
    (0x1D516, 0x1D51C),
    (0x1D51E, 0x1D539),
    (0x1D53B, 0x1D53E),
    (0x1D540, 0x1D544),
    (0x1D546, 0x1D546),
    (0x1D54A, 0x1D550),
    (0x1D552, 0x1D6A5),
    (0x1D6A8, 0x1D7CB),
    (0x1D7CE, 0x1DA8B),
    (0x1DA9B, 0x1DA9F),
    (0x1DAA1, 0x1DAAF),
    (0x1DF00, 0x1DF1E),
    (0x1DF25, 0x1DF2A),
    (0x1E000, 0x1E006),
    (0x1E008, 0x1E018),
    (0x1E01B, 0x1E021),
    (0x1E023, 0x1E024),
    (0x1E026, 0x1E02A),
    (0x1E030, 0x1E06D),
    (0x1E08F, 0x1E08F),
    (0x1E100, 0x1E12C),
    (0x1E130, 0x1E13D),
    (0x1E140, 0x1E149),
    (0x1E14E, 0x1E14F),
    (0x1E290, 0x1E2AE),
    (0x1E2C0, 0x1E2F9),
    (0x1E2FF, 0x1E2FF),
    (0x1E4D0, 0x1E4F9),
    (0x1E5D0, 0x1E5FA),
    (0x1E5FF, 0x1E5FF),
    (0x1E6C0, 0x1E6DE),
    (0x1E6E0, 0x1E6F5),
    (0x1E6FE, 0x1E6FF),
    (0x1E7E0, 0x1E7E6),
    (0x1E7E8, 0x1E7EB),
    (0x1E7ED, 0x1E7EE),
    (0x1E7F0, 0x1E7FE),
    (0x1E800, 0x1E8C4),
    (0x1E8C7, 0x1E8D6),
    (0x1E900, 0x1E94B),
    (0x1E950, 0x1E959),
    (0x1E95E, 0x1E95F),
    (0x1EC71, 0x1ECB4),
    (0x1ED01, 0x1ED3D),
    (0x1EE00, 0x1EE03),
    (0x1EE05, 0x1EE1F),
    (0x1EE21, 0x1EE22),
    (0x1EE24, 0x1EE24),
    (0x1EE27, 0x1EE27),
    (0x1EE29, 0x1EE32),
    (0x1EE34, 0x1EE37),
    (0x1EE39, 0x1EE39),
    (0x1EE3B, 0x1EE3B),
    (0x1EE42, 0x1EE42),
    (0x1EE47, 0x1EE47),
    (0x1EE49, 0x1EE49),
    (0x1EE4B, 0x1EE4B),
    (0x1EE4D, 0x1EE4F),
    (0x1EE51, 0x1EE52),
    (0x1EE54, 0x1EE54),
    (0x1EE57, 0x1EE57),
    (0x1EE59, 0x1EE59),
    (0x1EE5B, 0x1EE5B),
    (0x1EE5D, 0x1EE5D),
    (0x1EE5F, 0x1EE5F),
    (0x1EE61, 0x1EE62),
    (0x1EE64, 0x1EE64),
    (0x1EE67, 0x1EE6A),
    (0x1EE6C, 0x1EE72),
    (0x1EE74, 0x1EE77),
    (0x1EE79, 0x1EE7C),
    (0x1EE7E, 0x1EE7E),
    (0x1EE80, 0x1EE89),
    (0x1EE8B, 0x1EE9B),
    (0x1EEA1, 0x1EEA3),
    (0x1EEA5, 0x1EEA9),
    (0x1EEAB, 0x1EEBB),
    (0x1EEF0, 0x1EEF1),
    (0x1F000, 0x1F02B),
    (0x1F030, 0x1F093),
    (0x1F0A0, 0x1F0AE),
    (0x1F0B1, 0x1F0BF),
    (0x1F0C1, 0x1F0CF),
    (0x1F0D1, 0x1F0F5),
    (0x1F100, 0x1F1AD),
    (0x1F1E6, 0x1F202),
    (0x1F210, 0x1F23B),
    (0x1F240, 0x1F248),
    (0x1F250, 0x1F251),
    (0x1F260, 0x1F265),
    (0x1F300, 0x1F6D8),
    (0x1F6DC, 0x1F6EC),
    (0x1F6F0, 0x1F6FC),
    (0x1F700, 0x1F7D9),
    (0x1F7E0, 0x1F7EB),
    (0x1F7F0, 0x1F7F0),
    (0x1F800, 0x1F80B),
    (0x1F810, 0x1F847),
    (0x1F850, 0x1F859),
    (0x1F860, 0x1F887),
    (0x1F890, 0x1F8AD),
    (0x1F8B0, 0x1F8BB),
    (0x1F8C0, 0x1F8C1),
    (0x1F8D0, 0x1F8D8),
    (0x1F900, 0x1FA57),
    (0x1FA60, 0x1FA6D),
    (0x1FA70, 0x1FA7C),
    (0x1FA80, 0x1FA8A),
    (0x1FA8E, 0x1FAC6),
    (0x1FAC8, 0x1FAC8),
    (0x1FACD, 0x1FADC),
    (0x1FADF, 0x1FAEA),
    (0x1FAEF, 0x1FAF8),
    (0x1FB00, 0x1FB92),
    (0x1FB94, 0x1FBFA),
    (0x20000, 0x2A6DF),
    (0x2A700, 0x2B81D),
    (0x2B820, 0x2CEAD),
    (0x2CEB0, 0x2EBE0),
    (0x2EBF0, 0x2EE5D),
    (0x2F800, 0x2FA1D),
    (0x30000, 0x3134A),
    (0x31350, 0x33479),
    (0xE0001, 0xE0001),
    (0xE0020, 0xE007F),
    (0xE0100, 0xE01EF),
    (0xF0000, 0xFFFFD),
    (0x100000, 0x10FFFD),
];

/// Code point ranges that are ignorable in identifiers.
const IGNORABLE_RANGES: [(u32, u32); 24] = [
    (0x0, 0x8),
    (0xE, 0x1B),
    (0x7F, 0x9F),
    (0xAD, 0xAD),
    (0x600, 0x605),
    (0x61C, 0x61C),
    (0x6DD, 0x6DD),
    (0x70F, 0x70F),
    (0x890, 0x891),
    (0x8E2, 0x8E2),
    (0x180E, 0x180E),
    (0x200B, 0x200F),
    (0x202A, 0x202E),
    (0x2060, 0x2064),
    (0x2066, 0x206F),
    (0xFEFF, 0xFEFF),
    (0xFFF9, 0xFFFB),
    (0x110BD, 0x110BD),
    (0x110CD, 0x110CD),
    (0x13430, 0x1343F),
    (0x1BCA0, 0x1BCA3),
    (0x1D173, 0x1D17A),
    (0xE0001, 0xE0001),
    (0xE0020, 0xE007F),
];

/// The directionality of a code point, or -1 when it is undefined.
fn char_directionality(cp: u32) -> i8 {
    let idx = DIRECTIONALITY_TABLE.partition_point(|&(end, _)| end < cp);
    DIRECTIONALITY_TABLE
        .get(idx)
        .map_or(-1, |&(_, value)| value)
}

/// Whether a code point is defined in Unicode.
fn char_is_defined(cp: u32) -> bool {
    let idx = DEFINED_RANGES.partition_point(|&(start, _)| start <= cp);
    idx > 0 && DEFINED_RANGES[idx - 1].1 >= cp
}

/// Whether a code point is ignorable in a Java identifier.
fn char_is_identifier_ignorable(cp: u32) -> bool {
    let idx = IGNORABLE_RANGES.partition_point(|&(start, _)| start <= cp);
    idx > 0 && IGNORABLE_RANGES[idx - 1].1 >= cp
}

const CHARACTER_METHODS: [oxjvm_vm::NativeMethodDef; 79] = [
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
            let unit = read_value_int(vm, receiver(args)) as u16;
            object(vm.make_string_utf16(alloc::vec![unit])?)
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
            boolean(crate::lang::java_is_whitespace(
                (int_arg(args, 0) & 0xFFFF) as u16,
            ))
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
        "isUpperCase",
        "(I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            boolean(char::from_u32(int_arg(args, 0) as u32).is_some_and(char::is_uppercase))
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
        "isLowerCase",
        "(I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            boolean(char::from_u32(int_arg(args, 0) as u32).is_some_and(char::is_lowercase))
        },
    ),
    method(
        "toUpperCase",
        "(C)C",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let ch = int_arg(args, 0) as u32 & 0xFFFF;
            int(char::from_u32(ch).map_or(ch as i32, |c| char_simple_upper(c) as i32))
        },
    ),
    method(
        "toLowerCase",
        "(C)C",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let ch = int_arg(args, 0) as u32 & 0xFFFF;
            int(char::from_u32(ch).map_or(ch as i32, |c| char_simple_lower(c) as i32))
        },
    ),
    method(
        "toTitleCase",
        "(C)C",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let ch = int_arg(args, 0) as u32 & 0xFFFF;
            int(char::from_u32(ch).map_or(ch as i32, |c| char_to_titlecase(c) as i32))
        },
    ),
    method(
        "toUpperCase",
        "(I)I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let cp = int_arg(args, 0) as u32;
            int(char::from_u32(cp).map_or(cp as i32, |c| char_simple_upper(c) as i32))
        },
    ),
    method(
        "toLowerCase",
        "(I)I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let cp = int_arg(args, 0) as u32;
            int(char::from_u32(cp).map_or(cp as i32, |c| char_simple_lower(c) as i32))
        },
    ),
    method(
        "toTitleCase",
        "(I)I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let cp = int_arg(args, 0) as u32;
            int(char::from_u32(cp).map_or(cp as i32, |c| char_to_titlecase(c) as i32))
        },
    ),
    method("digit", "(CI)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        let ch = int_arg(args, 0) as u32 & 0xFFFF;
        let radix = int_arg(args, 1);
        if !(2..=36).contains(&radix) {
            return int(-1);
        }
        let value = char::from_u32(ch).map_or(-1, |c| crate::lang::java_digit(c, radix as u32));
        int(value)
    }),
    method("isDigit", "(I)Z", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        boolean(char::from_u32(int_arg(args, 0) as u32).is_some_and(char::is_numeric))
    }),
    method("isLetter", "(I)Z", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        boolean(char::from_u32(int_arg(args, 0) as u32).is_some_and(char::is_alphabetic))
    }),
    method(
        "isLetterOrDigit",
        "(I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            boolean(char::from_u32(int_arg(args, 0) as u32).is_some_and(char::is_alphanumeric))
        },
    ),
    method(
        "isWhitespace",
        "(I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let code_point = int_arg(args, 0) as u32;
            boolean(code_point <= 0xFFFF && crate::lang::java_is_whitespace(code_point as u16))
        },
    ),
    method("digit", "(II)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        let code_point = int_arg(args, 0) as u32;
        let radix = int_arg(args, 1);
        if !(2..=36).contains(&radix) {
            return int(-1);
        }
        let value =
            char::from_u32(code_point).map_or(-1, |c| crate::lang::java_digit(c, radix as u32));
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
        |vm, _, args| box_integer_like(vm, "java/lang/Character", b'C', int_arg(args, 0)),
    ),
    method("hashCode", "(C)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        int(int_arg(args, 0))
    }),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_value_int(vm, receiver(args)))
    }),
    method("compare", "(CC)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        int(int_arg(args, 0) - int_arg(args, 1))
    }),
    method(
        "reverseBytes",
        "(C)C",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| int(i32::from((int_arg(args, 0) as u16).swap_bytes())),
    ),
    method(
        "toString",
        "(C)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let unit = int_arg(args, 0) as u16;
            // A char is always a single UTF-16 unit, even a lone surrogate, so
            // build the string from the raw unit rather than a scalar value.
            object(vm.make_string_utf16(alloc::vec![unit])?)
        },
    ),
    method(
        "compareTo",
        "(Ljava/lang/Character;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            int(read_value_int(vm, receiver(args)) as u16 as i32
                - read_value_int(vm, ref_arg(args, 1)) as u16 as i32)
        },
    ),
    method(
        "compareTo",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC | ACC_BRIDGE | ACC_SYNTHETIC,
        |vm, _, args| {
            let a = read_value_int(vm, receiver(args));
            let b = read_value_int(vm, ref_arg(args, 1));
            int(compare_ints(a, b))
        },
    ),
    method(
        "charCount",
        "(I)I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| int(if int_arg(args, 0) >= 0x1_0000 { 2 } else { 1 }),
    ),
    method(
        "codePointAt",
        "(Ljava/lang/CharSequence;I)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let sequence = ref_arg(args, 0);
            let index = int_arg(args, 1);
            let (char_at_class, char_at) = vm.resolve_virtual_method(sequence, "charAt", "(I)C")?;
            let high = vm
                .invoke_method(
                    char_at_class,
                    char_at,
                    vec![Value::Ref(sequence), Value::Int(index)],
                )?
                .as_int() as u32
                & 0xFFFF;
            if (0xD800..0xDC00).contains(&high) {
                let (length_class, length) =
                    vm.resolve_virtual_method(sequence, "length", "()I")?;
                let length = vm
                    .invoke_method(length_class, length, vec![Value::Ref(sequence)])?
                    .as_int();
                if index + 1 < length {
                    let low = vm
                        .invoke_method(
                            char_at_class,
                            char_at,
                            vec![Value::Ref(sequence), Value::Int(index + 1)],
                        )?
                        .as_int() as u32
                        & 0xFFFF;
                    if (0xDC00..0xE000).contains(&low) {
                        return int((0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00)) as i32);
                    }
                }
            }
            int(high as i32)
        },
    ),
    method(
        "codePointAt",
        "([CI)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let array = ref_arg(args, 0);
            let index = int_arg(args, 1);
            let length = vm.array_length(array)? as i32;
            if index < 0 || index >= length {
                return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
            }
            let high = vm.array_get(array, index)?.as_int() as u32 & 0xFFFF;
            if (0xD800..0xDC00).contains(&high) && index + 1 < length {
                let low = vm.array_get(array, index + 1)?.as_int() as u32 & 0xFFFF;
                if (0xDC00..0xE000).contains(&low) {
                    return int((0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00)) as i32);
                }
            }
            int(high as i32)
        },
    ),
    method(
        "isSurrogate",
        "(C)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let unit = int_arg(args, 0) as u32 & 0xFFFF;
            boolean((0xD800..0xE000).contains(&unit))
        },
    ),
    method(
        "isHighSurrogate",
        "(C)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let unit = int_arg(args, 0) as u32 & 0xFFFF;
            boolean((0xD800..0xDC00).contains(&unit))
        },
    ),
    method(
        "isLowSurrogate",
        "(C)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let unit = int_arg(args, 0) as u32 & 0xFFFF;
            boolean((0xDC00..0xE000).contains(&unit))
        },
    ),
    method(
        "toCodePoint",
        "(CC)I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let high = int_arg(args, 0) as u32 & 0xFFFF;
            let low = int_arg(args, 1) as u32 & 0xFFFF;
            int((0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00)) as i32)
        },
    ),
    method(
        "highSurrogate",
        "(I)C",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let code_point = int_arg(args, 0) as u32;
            int((0xD800 + ((code_point - 0x10000) >> 10)) as i32)
        },
    ),
    method(
        "lowSurrogate",
        "(I)C",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let code_point = int_arg(args, 0) as u32;
            int((0xDC00 + (code_point & 0x3FF)) as i32)
        },
    ),
    method(
        "charCount",
        "(I)I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let code_point = int_arg(args, 0) as u32;
            int(if code_point >= 0x10000 { 2 } else { 1 })
        },
    ),
    method(
        "isBmpCodePoint",
        "(I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let code_point = int_arg(args, 0) as u32;
            boolean(code_point <= 0xFFFF)
        },
    ),
    method(
        "isSupplementaryCodePoint",
        "(I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let code_point = int_arg(args, 0) as u32;
            boolean((0x10000..=0x10FFFF).contains(&code_point))
        },
    ),
    method(
        "isValidCodePoint",
        "(I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let code_point = int_arg(args, 0) as u32;
            boolean(code_point <= 0x10FFFF)
        },
    ),
    method("isDigit", "(C)Z", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        let unit = int_arg(args, 0) as u32 & 0xFFFF;
        boolean(char::from_u32(unit).is_some_and(|ch| ch.is_ascii_digit()))
    }),
    method("isLetter", "(C)Z", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        let unit = int_arg(args, 0) as u32 & 0xFFFF;
        boolean(char::from_u32(unit).is_some_and(|ch| ch.is_alphabetic()))
    }),
    method(
        "isSpaceChar",
        "(C)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let unit = int_arg(args, 0) as u32 & 0xFFFF;
            boolean(is_space_char_code(unit))
        },
    ),
    method(
        "isAlphabetic",
        "(I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let code_point = int_arg(args, 0) as u32;
            boolean(char::from_u32(code_point).is_some_and(|ch| ch.is_alphabetic()))
        },
    ),
    method(
        "isJavaIdentifierStart",
        "(C)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let unit = int_arg(args, 0) as u32 & 0xFFFF;
            boolean(
                char_is_defined(unit)
                    && char::from_u32(unit)
                        .is_some_and(|ch| ch == '_' || ch == '$' || ch.is_alphabetic()),
            )
        },
    ),
    method(
        "isJavaIdentifierStart",
        "(I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let cp = int_arg(args, 0) as u32;
            boolean(
                char_is_defined(cp)
                    && char::from_u32(cp)
                        .is_some_and(|ch| ch == '_' || ch == '$' || ch.is_alphabetic()),
            )
        },
    ),
    method(
        "isJavaIdentifierPart",
        "(C)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let unit = int_arg(args, 0) as u32 & 0xFFFF;
            boolean(
                char_is_defined(unit)
                    && char::from_u32(unit).is_some_and(|ch| {
                        ch == '_'
                            || ch == '$'
                            || ch.is_alphanumeric()
                            || ch == '\u{200c}'
                            || ch == '\u{200d}'
                    }),
            )
        },
    ),
    method(
        "isJavaIdentifierPart",
        "(I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let cp = int_arg(args, 0) as u32;
            boolean(
                char_is_defined(cp)
                    && char::from_u32(cp).is_some_and(|ch| {
                        ch == '_'
                            || ch == '$'
                            || ch.is_alphanumeric()
                            || ch == '\u{200c}'
                            || ch == '\u{200d}'
                    }),
            )
        },
    ),
    method(
        "isUnicodeIdentifierStart",
        "(C)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let unit = int_arg(args, 0) as u32 & 0xFFFF;
            boolean(
                char_is_defined(unit)
                    && char::from_u32(unit).is_some_and(|ch| {
                        ch.is_alphabetic()
                            || ch == '$'
                            || ch == '_'
                            || ('\u{00a2}'..='\u{00a5}').contains(&ch)
                    }),
            )
        },
    ),
    method(
        "isUnicodeIdentifierPart",
        "(C)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let unit = int_arg(args, 0) as u32 & 0xFFFF;
            boolean(
                char_is_defined(unit)
                    && char::from_u32(unit).is_some_and(|ch| {
                        ch.is_alphanumeric()
                            || ch == '$'
                            || ch == '_'
                            || ch == '\u{200c}'
                            || ch == '\u{200d}'
                    }),
            )
        },
    ),
    method(
        "isUnicodeIdentifierStart",
        "(I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let code_point = int_arg(args, 0) as u32;
            boolean(
                char_is_defined(code_point)
                    && char::from_u32(code_point).is_some_and(|ch| {
                        ch.is_alphabetic()
                            || ch == '$'
                            || ch == '_'
                            || ('\u{00a2}'..='\u{00a5}').contains(&ch)
                    }),
            )
        },
    ),
    method(
        "isUnicodeIdentifierPart",
        "(I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let code_point = int_arg(args, 0) as u32;
            boolean(
                char_is_defined(code_point)
                    && char::from_u32(code_point).is_some_and(|ch| {
                        ch.is_alphanumeric()
                            || ch == '$'
                            || ch == '_'
                            || ch == '\u{200c}'
                            || ch == '\u{200d}'
                    }),
            )
        },
    ),
    method(
        "isDefined",
        "(C)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let unit = int_arg(args, 0) as u32 & 0xFFFF;
            boolean(char_is_defined(unit))
        },
    ),
    method(
        "isDefined",
        "(I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| boolean(char_is_defined(int_arg(args, 0) as u32)),
    ),
    method(
        "getDirectionality",
        "(C)B",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let unit = int_arg(args, 0) as u32 & 0xFFFF;
            Ok(Value::Int(i32::from(char_directionality(unit))))
        },
    ),
    method(
        "getDirectionality",
        "(I)B",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            Ok(Value::Int(i32::from(char_directionality(
                int_arg(args, 0) as u32
            ))))
        },
    ),
    method(
        "isIdentifierIgnorable",
        "(C)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let unit = int_arg(args, 0) as u32 & 0xFFFF;
            boolean(char_is_identifier_ignorable(unit))
        },
    ),
    method(
        "isIdentifierIgnorable",
        "(I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| boolean(char_is_identifier_ignorable(int_arg(args, 0) as u32)),
    ),
    method(
        "isWhitespace",
        "(I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let code_point = int_arg(args, 0) as u32;
            boolean(code_point <= 0xFFFF && crate::lang::java_is_whitespace(code_point as u16))
        },
    ),
    method(
        "isSpaceChar",
        "(I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let code_point = int_arg(args, 0) as u32;
            boolean(is_space_char_code(code_point))
        },
    ),
    method(
        "isISOControl",
        "(C)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let unit = int_arg(args, 0) as u32 & 0xFFFF;
            boolean((0x00..=0x1F).contains(&unit) || (0x7F..=0x9F).contains(&unit))
        },
    ),
    method(
        "isISOControl",
        "(I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let code_point = int_arg(args, 0) as u32;
            boolean((0x00..=0x1F).contains(&code_point) || (0x7F..=0x9F).contains(&code_point))
        },
    ),
    method(
        "toChars",
        "(I)[C",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let code_point = int_arg(args, 0) as u32;
            if code_point > 0x10FFFF {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("Not a valid Unicode code point"),
                ));
            }
            let units: Vec<u16> = if code_point <= 0xFFFF {
                vec![code_point as u16]
            } else {
                let value = code_point - 0x10000;
                vec![
                    0xD800 | ((value >> 10) as u16),
                    0xDC00 | ((value & 0x3FF) as u16),
                ]
            };
            let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Char, units.len())?;
            for (index, unit) in units.iter().enumerate() {
                vm.array_set(array, index as i32, Value::Int(i32::from(*unit)), false)?;
            }
            object(array)
        },
    ),
    method(
        "toChars",
        "(I[CI)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let code_point = int_arg(args, 0) as u32;
            if code_point > 0x10FFFF {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("Not a valid Unicode code point"),
                ));
            }
            let dst = ref_arg(args, 1);
            if dst.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let dst_index = int_arg(args, 2);
            let units: Vec<u16> = if code_point <= 0xFFFF {
                vec![code_point as u16]
            } else {
                let value = code_point - 0x10000;
                vec![
                    0xD800 | ((value >> 10) as u16),
                    0xDC00 | ((value & 0x3FF) as u16),
                ]
            };
            let length = vm.array_length(dst)?;
            if dst_index < 0 || dst_index as usize + units.len() > length {
                return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
            }
            for (offset, unit) in units.iter().enumerate() {
                vm.array_set(
                    dst,
                    dst_index + offset as i32,
                    Value::Int(i32::from(*unit)),
                    false,
                )?;
            }
            int(units.len() as i32)
        },
    ),
    method(
        "toString",
        "(I)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let code_point = int_arg(args, 0) as u32;
            if code_point > 0x10FFFF {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("Not a valid Unicode code point"),
                ));
            }
            let units: Vec<u16> = if code_point <= 0xFFFF {
                vec![code_point as u16]
            } else {
                let value = code_point - 0x10000;
                vec![
                    0xD800 | ((value >> 10) as u16),
                    0xDC00 | ((value & 0x3FF) as u16),
                ]
            };
            object(vm.make_string_utf16(units)?)
        },
    ),
    method("getType", "(C)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        int(char_general_category(int_arg(args, 0) as u32 & 0xFFFF))
    }),
    method("getType", "(I)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        int(char_general_category(int_arg(args, 0) as u32))
    }),
    method(
        "isTitleCase",
        "(C)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| boolean(char_is_titlecase(int_arg(args, 0) as u32 & 0xFFFF)),
    ),
    method(
        "isTitleCase",
        "(I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| boolean(char_is_titlecase(int_arg(args, 0) as u32)),
    ),
    method(
        "isMirrored",
        "(C)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| boolean(char_is_mirrored(int_arg(args, 0) as u32 & 0xFFFF)),
    ),
    method(
        "isMirrored",
        "(I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| boolean(char_is_mirrored(int_arg(args, 0) as u32)),
    ),
    method(
        "getNumericValue",
        "(C)I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| int(char_numeric_value(int_arg(args, 0) as u32 & 0xFFFF)),
    ),
    method(
        "getNumericValue",
        "(I)I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| int(char_numeric_value(int_arg(args, 0) as u32)),
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
            "DIRECTIONALITY_UNDEFINED",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(-1)),
        ),
        field(
            "DIRECTIONALITY_LEFT_TO_RIGHT",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(0)),
        ),
        field(
            "DIRECTIONALITY_RIGHT_TO_LEFT",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(1)),
        ),
        field(
            "DIRECTIONALITY_EUROPEAN_NUMBER",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(3)),
        ),
        field(
            "DIRECTIONALITY_EUROPEAN_NUMBER_SEPARATOR",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(4)),
        ),
        field(
            "DIRECTIONALITY_EUROPEAN_NUMBER_TERMINATOR",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(5)),
        ),
        field(
            "DIRECTIONALITY_ARABIC_NUMBER",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(6)),
        ),
        field(
            "DIRECTIONALITY_COMMON_NUMBER_SEPARATOR",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(7)),
        ),
        field(
            "DIRECTIONALITY_NONSPACING_MARK",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(8)),
        ),
        field(
            "DIRECTIONALITY_BOUNDARY_NEUTRAL",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(9)),
        ),
        field(
            "DIRECTIONALITY_PARAGRAPH_SEPARATOR",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(10)),
        ),
        field(
            "DIRECTIONALITY_SEGMENT_SEPARATOR",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(11)),
        ),
        field(
            "DIRECTIONALITY_WHITESPACE",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(12)),
        ),
        field(
            "DIRECTIONALITY_OTHER_NEUTRALS",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(13)),
        ),
        field(
            "DIRECTIONALITY_LEFT_TO_RIGHT_EMBEDDING",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(14)),
        ),
        field(
            "DIRECTIONALITY_LEFT_TO_RIGHT_OVERRIDE",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(15)),
        ),
        field(
            "DIRECTIONALITY_RIGHT_TO_LEFT_ARABIC",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(2)),
        ),
        field(
            "DIRECTIONALITY_RIGHT_TO_LEFT_EMBEDDING",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(16)),
        ),
        field(
            "DIRECTIONALITY_RIGHT_TO_LEFT_OVERRIDE",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(17)),
        ),
        field(
            "DIRECTIONALITY_POP_DIRECTIONAL_FORMAT",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(18)),
        ),
        field(
            "DIRECTIONALITY_FIRST_STRONG_ISOLATE",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(21)),
        ),
        field(
            "DIRECTIONALITY_LEFT_TO_RIGHT_ISOLATE",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(19)),
        ),
        field(
            "DIRECTIONALITY_RIGHT_TO_LEFT_ISOLATE",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(20)),
        ),
        field(
            "DIRECTIONALITY_POP_DIRECTIONAL_ISOLATE",
            "B",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(22)),
        ),
        field(
            "TYPE",
            "Ljava/lang/Class;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &CHARACTER_METHODS,
    Some(character_clinit),
);

// -------------------------------------------------------------------------------------------
// Character.UnicodeBlock
// -------------------------------------------------------------------------------------------

/// The `UnicodeBlock` constants picocli compares against, with their code point ranges.
const UNICODE_BLOCKS: [(&str, u32, u32); 16] = [
    ("HIRAGANA", 0x3040, 0x309F),
    ("KATAKANA", 0x30A0, 0x30FF),
    ("KATAKANA_PHONETIC_EXTENSIONS", 0x31F0, 0x31FF),
    ("HANGUL_COMPATIBILITY_JAMO", 0x3130, 0x318F),
    ("HANGUL_JAMO", 0x1100, 0x11FF),
    ("HANGUL_SYLLABLES", 0xAC00, 0xD7AF),
    ("CJK_UNIFIED_IDEOGRAPHS", 0x4E00, 0x9FFF),
    ("CJK_UNIFIED_IDEOGRAPHS_EXTENSION_A", 0x3400, 0x4DBF),
    ("CJK_UNIFIED_IDEOGRAPHS_EXTENSION_B", 0x20000, 0x2A6DF),
    ("CJK_COMPATIBILITY_FORMS", 0xFE30, 0xFE4F),
    ("CJK_COMPATIBILITY_IDEOGRAPHS", 0xF900, 0xFAFF),
    ("CJK_RADICALS_SUPPLEMENT", 0x2E80, 0x2EFF),
    ("CJK_SYMBOLS_AND_PUNCTUATION", 0x3000, 0x303F),
    ("ENCLOSED_CJK_LETTERS_AND_MONTHS", 0x3200, 0x32FF),
    ("HALFWIDTH_AND_FULLWIDTH_FORMS", 0xFF00, 0xFFEF),
    ("SPECIALS", 0xFFF0, 0xFFFF),
];

/// Create the block constants once; every field must be non-null before `of` is compared with
/// them, otherwise unrelated code points would compare equal to an unset field.
fn unicode_block_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/lang/Character$UnicodeBlock")?;
    for (name, _, _) in UNICODE_BLOCKS {
        if vm
            .read_static_ref_if_set(class, name, "Ljava/lang/Character$UnicodeBlock;")
            .is_some()
        {
            continue;
        }
        let instance = vm.new_instance(class)?;
        vm.set_static_value(
            class,
            name,
            "Ljava/lang/Character$UnicodeBlock;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

fn unicode_block_of(vm: &mut Vm<'_>, code_point: u32) -> Result<Value, VmError> {
    if code_point > 0x10FFFF {
        return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
    }
    let Some((name, _, _)) = UNICODE_BLOCKS
        .iter()
        .find(|(_, start, end)| (*start..=*end).contains(&code_point))
    else {
        return object(ObjectRef::NULL);
    };
    let name = *name;
    let class = vm.resolve_class("java/lang/Character$UnicodeBlock")?;
    let block = match vm.read_static_ref_if_set(class, name, "Ljava/lang/Character$UnicodeBlock;") {
        Some(block) => block,
        None => {
            unicode_block_clinit(vm)?;
            vm.read_static_ref_if_set(class, name, "Ljava/lang/Character$UnicodeBlock;")
                .unwrap_or(ObjectRef::NULL)
        }
    };
    object(block)
}

const UNICODE_BLOCK_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "of",
        "(C)Ljava/lang/Character$UnicodeBlock;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| unicode_block_of(vm, int_arg(args, 0) as u32 & 0xFFFF),
    ),
    method(
        "of",
        "(I)Ljava/lang/Character$UnicodeBlock;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| unicode_block_of(vm, int_arg(args, 0) as u32),
    ),
];

/// `Character.UnicodeBlock`: the constants are identity compared, so `of` must return them.
pub(crate) const UNICODE_BLOCK: oxjvm_vm::NativeClass = class(
    "java/lang/Character$UnicodeBlock",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field(
            "HIRAGANA",
            "Ljava/lang/Character$UnicodeBlock;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "KATAKANA",
            "Ljava/lang/Character$UnicodeBlock;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "KATAKANA_PHONETIC_EXTENSIONS",
            "Ljava/lang/Character$UnicodeBlock;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "HANGUL_COMPATIBILITY_JAMO",
            "Ljava/lang/Character$UnicodeBlock;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "HANGUL_JAMO",
            "Ljava/lang/Character$UnicodeBlock;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "HANGUL_SYLLABLES",
            "Ljava/lang/Character$UnicodeBlock;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "CJK_UNIFIED_IDEOGRAPHS",
            "Ljava/lang/Character$UnicodeBlock;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "CJK_UNIFIED_IDEOGRAPHS_EXTENSION_A",
            "Ljava/lang/Character$UnicodeBlock;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "CJK_UNIFIED_IDEOGRAPHS_EXTENSION_B",
            "Ljava/lang/Character$UnicodeBlock;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "CJK_COMPATIBILITY_FORMS",
            "Ljava/lang/Character$UnicodeBlock;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "CJK_COMPATIBILITY_IDEOGRAPHS",
            "Ljava/lang/Character$UnicodeBlock;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "CJK_RADICALS_SUPPLEMENT",
            "Ljava/lang/Character$UnicodeBlock;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "CJK_SYMBOLS_AND_PUNCTUATION",
            "Ljava/lang/Character$UnicodeBlock;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "ENCLOSED_CJK_LETTERS_AND_MONTHS",
            "Ljava/lang/Character$UnicodeBlock;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "HALFWIDTH_AND_FULLWIDTH_FORMS",
            "Ljava/lang/Character$UnicodeBlock;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "SPECIALS",
            "Ljava/lang/Character$UnicodeBlock;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &UNICODE_BLOCK_METHODS,
    Some(unicode_block_clinit),
);

/// `Double.toHexString`: the JDK's hexadecimal form, `0x1.<mantissa>p<exponent>`.
///
/// The exponent is the binary exponent of the value, and the mantissa keeps the trailing zeros
/// the JDK drops.  A zero, an infinity and a NaN have their own spellings.
fn double_to_hex_string(value: f64) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-Infinity".into()
        } else {
            "Infinity".into()
        };
    }
    let sign = if value.is_sign_negative() { "-" } else { "" };
    let magnitude = value.abs();
    if magnitude == 0.0 {
        return alloc::format!("{sign}0x0.0p0");
    }
    let bits = magnitude.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32 - 1023;
    // The implicit leading one and the 52 fraction bits are the significand.
    let significand = (bits & 0x000f_ffff_ffff_ffff) | 0x0010_0000_0000_0000;
    let digits = alloc::format!("{significand:013x}");
    let fraction = digits[1..].trim_end_matches('0');
    if fraction.is_empty() {
        alloc::format!("{sign}0x1.0p{exponent}")
    } else {
        alloc::format!("{sign}0x1.{fraction}p{exponent}")
    }
}

/// `Float.toHexString`: the single-precision form of [`double_to_hex_string`].
fn float_to_hex_string(value: f32) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-Infinity".into()
        } else {
            "Infinity".into()
        };
    }
    let sign = if value.is_sign_negative() { "-" } else { "" };
    let magnitude = value.abs();
    if magnitude == 0.0 {
        return alloc::format!("{sign}0x0.0p0");
    }
    let bits = magnitude.to_bits();
    let exponent = ((bits >> 23) & 0xff) as i32 - 127;
    let significand = (bits & 0x007f_ffff) | 0x0080_0000;
    let digits = alloc::format!("{significand:06x}");
    let fraction = digits[1..].trim_end_matches('0');
    if fraction.is_empty() {
        alloc::format!("{sign}0x1.0p{exponent}")
    } else {
        alloc::format!("{sign}0x1.{fraction}p{exponent}")
    }
}

/// JDK `Math.max`/`min` semantics: NaN propagates and +0.0 outranks -0.0 for max (vice versa
/// for min), which Rust's `f32::max`/`min` do not guarantee.
fn jdk_float_max(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        return f32::NAN;
    }
    if a == 0.0 && b == 0.0 {
        return f32::from_bits(a.to_bits() & b.to_bits());
    }
    if a > b { a } else { b }
}

fn jdk_float_min(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        return f32::NAN;
    }
    if a == 0.0 && b == 0.0 {
        return f32::from_bits(a.to_bits() | b.to_bits());
    }
    if a < b { a } else { b }
}

fn jdk_double_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == 0.0 && b == 0.0 {
        return f64::from_bits(a.to_bits() & b.to_bits());
    }
    if a > b { a } else { b }
}

fn jdk_double_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == 0.0 && b == 0.0 {
        return f64::from_bits(a.to_bits() | b.to_bits());
    }
    if a < b { a } else { b }
}

const FLOAT_METHODS: [oxjvm_vm::NativeMethodDef; 28] = [
    method("<init>", "(F)V", ACC_PUBLIC, |vm, _, args| {
        write_value_int(
            vm,
            receiver(args),
            crate::float_arg(args, 1).to_bits() as i32,
        );
        void()
    }),
    method(
        "toHexString",
        "(F)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = float_to_hex_string(crate::float_arg(args, 0));
            object(vm.make_string(&text)?)
        },
    ),
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
                Some(value) => float(value),
                None => Err(vm.throw_new(
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
        "floatToRawIntBits",
        "(F)I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| int(crate::float_arg(args, 0).to_bits() as i32),
    ),
    method("compare", "(FF)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        int(compare_floats(
            crate::float_arg(args, 0),
            crate::float_arg(args, 1),
        ))
    }),
    method("isFinite", "(F)Z", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        boolean(crate::float_arg(args, 0).is_finite())
    }),
    method(
        "isInfinite",
        "(F)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| boolean(crate::float_arg(args, 0).is_infinite()),
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
    method(
        "valueOf",
        "(F)Ljava/lang/Float;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let class = vm.resolve_class("java/lang/Float")?;
            let instance = vm.new_instance(class)?;
            write_value_int(vm, instance, crate::float_arg(args, 0).to_bits() as i32);
            object(instance)
        },
    ),
    method(
        "compareTo",
        "(Ljava/lang/Float;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let a = f32::from_bits(read_value_int(vm, receiver(args)) as u32);
            let b = f32::from_bits(read_value_int(vm, ref_arg(args, 1)) as u32);
            int(compare_floats(a, b))
        },
    ),
    method(
        "compareTo",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC | ACC_BRIDGE | ACC_SYNTHETIC,
        |vm, _, args| {
            let a = f32::from_bits(read_value_int(vm, receiver(args)) as u32);
            let b = f32::from_bits(read_value_int(vm, ref_arg(args, 1)) as u32);
            int(compare_floats(a, b))
        },
    ),
    method(
        "floatToFloat16",
        "(F)S",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| int(i32::from(float_to_float16(crate::float_arg(args, 0)) as i16)),
    ),
    method(
        "float16ToFloat",
        "(S)F",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| float(float16_to_float(int_arg(args, 0) as u16)),
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let bits = read_value_int(vm, receiver(args)) as u32;
        int(if f32::from_bits(bits).is_nan() {
            0x7FC0_0000u32 as i32
        } else {
            bits as i32
        })
    }),
    method("hashCode", "(F)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        let value = crate::float_arg(args, 0);
        int(if value.is_nan() {
            0x7FC0_0000u32 as i32
        } else {
            value.to_bits() as i32
        })
    }),
    method("sum", "(FF)F", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        float(crate::float_arg(args, 0) + crate::float_arg(args, 1))
    }),
    method("max", "(FF)F", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        float(jdk_float_max(
            crate::float_arg(args, 0),
            crate::float_arg(args, 1),
        ))
    }),
    method("min", "(FF)F", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        float(jdk_float_min(
            crate::float_arg(args, 0),
            crate::float_arg(args, 1),
        ))
    }),
    method(
        "valueOf",
        "(Ljava/lang/String;)Ljava/lang/Float;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = string_arg(vm, args[0])?;
            match format::parse_float(&text) {
                Some(value) => {
                    let class = vm.resolve_class("java/lang/Float")?;
                    let instance = vm.new_instance(class)?;
                    write_value_int(vm, instance, value.to_bits() as i32);
                    object(instance)
                }
                None => Err(vm.throw_new(
                    "java/lang/NumberFormatException",
                    Some(&alloc::format!("For input string: \"{text}\"")),
                )),
            }
        },
    ),
];

fn float16_to_float(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exponent = ((bits >> 10) & 0x1f) as i32;
    let mantissa = f64::from(bits & 0x3ff);
    let value = if exponent == 0 {
        mantissa / 1024.0 * pow2(-14)
    } else if exponent == 0x1f {
        if mantissa == 0.0 {
            f64::INFINITY
        } else {
            // Preserve the NaN significand (and sign) through the widening
            // conversion the way the JDK does, so a binary16 NaN round-trips.
            let nan_bits =
                (u32::from(bits & 0x8000) << 16) | 0x7f80_0000 | (u32::from(bits & 0x3ff) << 13);
            return f32::from_bits(nan_bits);
        }
    } else {
        (1.0 + mantissa / 1024.0) * pow2(exponent - 15)
    };
    (sign * value) as f32
}

fn pow2(exponent: i32) -> f64 {
    let mut value = 1.0;
    if exponent >= 0 {
        for _ in 0..exponent {
            value *= 2.0;
        }
    } else {
        for _ in 0..(-exponent) {
            value /= 2.0;
        }
    }
    value
}

fn float_to_float16(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let mut exponent = ((bits >> 23) & 0xff) as i32;
    let mantissa = bits & 0x7f_ffff;
    if exponent == 0xff {
        // Preserve the NaN significand through the narrowing conversion: keep
        // the top ten mantissa bits, forcing the quiet bit when they would all
        // shift out so a NaN never collapses into infinity.
        if mantissa != 0 {
            let mut significand = mantissa >> 13;
            if significand == 0 {
                significand = 0x200;
            }
            return sign | 0x7c00 | significand as u16;
        }
        return sign | 0x7c00;
    }
    if exponent == 0 {
        return sign;
    }
    exponent = exponent - 127 + 15;
    if exponent >= 0x1f {
        return sign | 0x7c00;
    }
    if exponent <= 0 {
        if exponent < -10 {
            return sign;
        }
        let mantissa = mantissa | 0x80_0000;
        let shift = (14 - exponent) as u32;
        let mut half = mantissa >> shift;
        let remainder = mantissa & ((1 << shift) - 1);
        let halfway = 1 << (shift - 1);
        if remainder > halfway || (remainder == halfway && (half & 1) != 0) {
            half += 1;
        }
        return sign | half as u16;
    }
    let mut half = ((exponent as u32) << 10) | (mantissa >> 13);
    let remainder = mantissa & 0x1fff;
    if remainder > 0x1000 || (remainder == 0x1000 && (half & 1) != 0) {
        half += 1;
    }
    if half >= 0x7c00 {
        return sign | 0x7c00;
    }
    sign | half as u16
}

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
    Some(float_clinit),
);

const DOUBLE_METHODS: [oxjvm_vm::NativeMethodDef; 26] = [
    method("<init>", "(D)V", ACC_PUBLIC, |vm, _, args| {
        write_value_long(
            vm,
            receiver(args),
            crate::double_arg(args, 1).to_bits() as i64,
        );
        void()
    }),
    method(
        "toHexString",
        "(D)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = double_to_hex_string(crate::double_arg(args, 0));
            object(vm.make_string(&text)?)
        },
    ),
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
                Some(value) => double(value),
                None => Err(vm.throw_new(
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
    method(
        "compareTo",
        "(Ljava/lang/Double;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let a = f64::from_bits(read_value_long(vm, receiver(args)) as u64);
            let b = f64::from_bits(read_value_long(vm, ref_arg(args, 1)) as u64);
            int(compare_doubles(a, b))
        },
    ),
    method(
        "compareTo",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC | ACC_BRIDGE | ACC_SYNTHETIC,
        |vm, _, args| {
            let a = f64::from_bits(read_value_long(vm, receiver(args)) as u64);
            let b = f64::from_bits(read_value_long(vm, ref_arg(args, 1)) as u64);
            int(compare_doubles(a, b))
        },
    ),
    method(
        "valueOf",
        "(D)Ljava/lang/Double;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let class = vm.resolve_class("java/lang/Double")?;
            let instance = vm.new_instance(class)?;
            write_value_long(vm, instance, crate::double_arg(args, 0).to_bits() as i64);
            object(instance)
        },
    ),
    method(
        "valueOf",
        "(Ljava/lang/String;)Ljava/lang/Double;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = string_arg(vm, args[0])?;
            match format::parse_double(&text) {
                Some(value) => {
                    let class = vm.resolve_class("java/lang/Double")?;
                    let instance = vm.new_instance(class)?;
                    write_value_long(vm, instance, value.to_bits() as i64);
                    object(instance)
                }
                None => Err(vm.throw_new(
                    "java/lang/NumberFormatException",
                    Some(&alloc::format!("For input string: \"{text}\"")),
                )),
            }
        },
    ),
    method("isNaN", "(D)Z", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        boolean(crate::double_arg(args, 0).is_nan())
    }),
    method("isFinite", "(D)Z", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        boolean(crate::double_arg(args, 0).is_finite())
    }),
    method(
        "isInfinite",
        "(D)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| boolean(crate::double_arg(args, 0).is_infinite()),
    ),
    method("compare", "(DD)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        int(compare_doubles(
            crate::double_arg(args, 0),
            crate::double_arg(args, 1),
        ))
    }),
    method("sum", "(DD)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        Ok(Value::Double(
            crate::double_arg(args, 0) + crate::double_arg(args, 1),
        ))
    }),
    method("max", "(DD)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        Ok(Value::Double(jdk_double_max(
            crate::double_arg(args, 0),
            crate::double_arg(args, 1),
        )))
    }),
    method("min", "(DD)D", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        Ok(Value::Double(jdk_double_min(
            crate::double_arg(args, 0),
            crate::double_arg(args, 1),
        )))
    }),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let bits = read_value_long(vm, receiver(args)) as u64;
        let bits = if f64::from_bits(bits).is_nan() {
            0x7FF8_0000_0000_0000u64
        } else {
            bits
        };
        int((bits ^ (bits >> 32)) as i32)
    }),
    method("hashCode", "(D)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        let value = crate::double_arg(args, 0);
        let bits = if value.is_nan() {
            0x7FF8_0000_0000_0000u64
        } else {
            value.to_bits()
        };
        int((bits ^ (bits >> 32)) as i32)
    }),
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
    Some(double_clinit),
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
    Some(void_clinit),
);
