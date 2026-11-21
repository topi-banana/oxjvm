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

const BOOLEAN_METHODS: [oxjvm_vm::NativeMethodDef; 17] = [
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

const CHARACTER_METHODS: [oxjvm_vm::NativeMethodDef; 62] = [
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
        if !(2..=36).contains(&radix) {
            return int(-1);
        }
        let value = char::from_u32(ch)
            .and_then(|c| c.to_digit(radix as u32))
            .map_or(-1, |d| d as i32);
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
            boolean(char::from_u32(int_arg(args, 0) as u32).is_some_and(char::is_whitespace))
        },
    ),
    method("digit", "(II)I", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        let code_point = int_arg(args, 0) as u32;
        let radix = int_arg(args, 1);
        if !(2..=36).contains(&radix) {
            return int(-1);
        }
        let value = char::from_u32(code_point)
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
                char::from_u32(unit).is_some_and(|ch| ch == '_' || ch == '$' || ch.is_alphabetic()),
            )
        },
    ),
    method(
        "isJavaIdentifierPart",
        "(C)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let unit = int_arg(args, 0) as u32 & 0xFFFF;
            boolean(char::from_u32(unit).is_some_and(|ch| {
                ch == '_'
                    || ch == '$'
                    || ch.is_alphanumeric()
                    || ch == '\u{200c}'
                    || ch == '\u{200d}'
            }))
        },
    ),
    method(
        "isUnicodeIdentifierStart",
        "(C)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let unit = int_arg(args, 0) as u32 & 0xFFFF;
            boolean(char::from_u32(unit).is_some_and(|ch| {
                ch.is_alphabetic()
                    || ch == '$'
                    || ch == '_'
                    || ('\u{00a2}'..='\u{00a5}').contains(&ch)
            }))
        },
    ),
    method(
        "isUnicodeIdentifierPart",
        "(C)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let unit = int_arg(args, 0) as u32 & 0xFFFF;
            boolean(char::from_u32(unit).is_some_and(|ch| {
                ch.is_alphanumeric()
                    || ch == '$'
                    || ch == '_'
                    || ch == '\u{200c}'
                    || ch == '\u{200d}'
            }))
        },
    ),
    method(
        "isUnicodeIdentifierStart",
        "(I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let code_point = int_arg(args, 0) as u32;
            boolean(char::from_u32(code_point).is_some_and(|ch| {
                ch.is_alphabetic()
                    || ch == '$'
                    || ch == '_'
                    || ('\u{00a2}'..='\u{00a5}').contains(&ch)
            }))
        },
    ),
    method(
        "isUnicodeIdentifierPart",
        "(I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let code_point = int_arg(args, 0) as u32;
            boolean(char::from_u32(code_point).is_some_and(|ch| {
                ch.is_alphanumeric()
                    || ch == '$'
                    || ch == '_'
                    || ch == '\u{200c}'
                    || ch == '\u{200d}'
            }))
        },
    ),
    method(
        "isDefined",
        "(C)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let unit = int_arg(args, 0) as u32 & 0xFFFF;
            boolean(char::from_u32(unit).is_some())
        },
    ),
    method(
        "isWhitespace",
        "(I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| {
            let code_point = int_arg(args, 0) as u32;
            boolean(char::from_u32(code_point).is_some_and(|ch| ch.is_whitespace()))
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
const UNICODE_BLOCKS: [(&str, u32, u32); 15] = [
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

const UNICODE_BLOCK_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "of",
    "(I)Ljava/lang/Character$UnicodeBlock;",
    ACC_PUBLIC | ACC_STATIC,
    |vm, _, args| {
        let code_point = int_arg(args, 0) as u32;
        let Some((name, _, _)) = UNICODE_BLOCKS
            .iter()
            .find(|(_, start, end)| (*start..=*end).contains(&code_point))
        else {
            return object(ObjectRef::NULL);
        };
        let name = *name;
        let class = vm.resolve_class("java/lang/Character$UnicodeBlock")?;
        let block =
            match vm.read_static_ref_if_set(class, name, "Ljava/lang/Character$UnicodeBlock;") {
                Some(block) => block,
                None => {
                    unicode_block_clinit(vm)?;
                    vm.read_static_ref_if_set(class, name, "Ljava/lang/Character$UnicodeBlock;")
                        .unwrap_or(ObjectRef::NULL)
                }
            };
        object(block)
    },
)];

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
            f64::NAN
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
        return sign | if mantissa != 0 { 0x7e00 } else { 0x7c00 };
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
