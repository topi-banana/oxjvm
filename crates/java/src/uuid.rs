//! `java.util.UUID`.

use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectData, ObjectRef, Value, Vm, VmError};

use crate::{boolean, class, field, int, long, method, object, receiver, string_arg, void};

const UUID_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
    method("<init>", "(JJ)V", ACC_PUBLIC, |vm, _, args| {
        write_bits(
            vm,
            receiver(args),
            crate::long_arg(args, 1),
            crate::long_arg(args, 2),
        );
        void()
    }),
    method(
        "randomUUID",
        "()Ljava/util/UUID;",
        ACC_PUBLIC | ACC_STATIC,
        uuid_random,
    ),
    method(
        "fromString",
        "(Ljava/lang/String;)Ljava/util/UUID;",
        ACC_PUBLIC | ACC_STATIC,
        uuid_from_string,
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        uuid_to_string,
    ),
    method(
        "getMostSignificantBits",
        "()J",
        ACC_PUBLIC,
        |vm, _, args| long(uuid_bits(vm, receiver(args)).0),
    ),
    method(
        "getLeastSignificantBits",
        "()J",
        ACC_PUBLIC,
        |vm, _, args| long(uuid_bits(vm, receiver(args)).1),
    ),
    method("version", "()I", ACC_PUBLIC, |vm, _, args| {
        let (most, _) = uuid_bits(vm, receiver(args));
        int(((most >> 12) & 0xF) as i32)
    }),
    method("variant", "()I", ACC_PUBLIC, |vm, _, args| {
        let (_, least) = uuid_bits(vm, receiver(args));
        let bits = least as u64;
        let variant = if bits & (1 << 63) == 0 {
            0
        } else if bits & (1 << 62) == 0 {
            2
        } else if bits & (1 << 61) == 0 {
            6
        } else {
            7
        };
        int(variant)
    }),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = crate::ref_arg(args, 1);
            if other.is_null() || vm.class_of(other) != vm.class_of(receiver(args)) {
                return boolean(false);
            }
            boolean(uuid_bits(vm, receiver(args)) == uuid_bits(vm, other))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let (most, least) = uuid_bits(vm, receiver(args));
        int(((most >> 32) ^ most ^ (least >> 32) ^ least) as i32)
    }),
    method(
        "compareTo",
        "(Ljava/util/UUID;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = crate::ref_arg(args, 1);
            let (a_most, a_least) = uuid_bits(vm, receiver(args));
            let (b_most, b_least) = uuid_bits(vm, other);
            let ordering = a_most.cmp(&b_most).then(a_least.cmp(&b_least));
            int(match ordering {
                core::cmp::Ordering::Less => -1,
                core::cmp::Ordering::Equal => 0,
                core::cmp::Ordering::Greater => 1,
            })
        },
    ),
];

pub(crate) const UUID: oxjvm_vm::NativeClass = class(
    "java/util/UUID",
    Some("java/lang/Object"),
    &["java/io/Serializable", "java/lang/Comparable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("mostSigBits", "J", ACC_PRIVATE | ACC_FINAL, None),
        field("leastSigBits", "J", ACC_PRIVATE | ACC_FINAL, None),
    ],
    &UUID_METHODS,
    None,
);

fn uuid_bits(vm: &Vm<'_>, this: ObjectRef) -> (i64, i64) {
    let class = vm.class_of(this);
    let entry = vm.classes.get(class);
    let mut most = 0i64;
    let mut least = 0i64;
    if let Some(ObjectData::Instance(fields)) = vm.heap.get(this).map(|object| &object.data) {
        for field in entry.fields.iter() {
            let value = fields
                .get(field.slot as usize)
                .map(|value| value.as_long())
                .unwrap_or(0);
            match field.name.as_str() {
                "mostSigBits" => most = value,
                "leastSigBits" => least = value,
                _ => {}
            }
        }
    }
    (most, least)
}

fn write_bits(vm: &mut Vm<'_>, this: ObjectRef, most: i64, least: i64) {
    let class = vm.class_of(this);
    for (name, value) in [("mostSigBits", most), ("leastSigBits", least)] {
        if let Some((declaring, field)) = vm.find_field_opt(class, name, "J") {
            let slot = vm.classes.get(declaring).fields[field as usize].slot as usize;
            if let Some(ObjectData::Instance(fields)) =
                vm.heap.get_mut(this).map(|object| &mut object.data)
            {
                fields[slot] = Value::Long(value);
            }
        }
    }
}

fn uuid_new(vm: &mut Vm<'_>, most: i64, least: i64) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/util/UUID")?;
    let instance = vm.new_instance(class)?;
    write_bits(vm, instance, most, least);
    object(instance)
}

fn uuid_random(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    _args: &[Value],
) -> Result<Value, VmError> {
    let first = vm.random_u64();
    let second = vm.random_u64();
    let most = ((first & 0xFFFF_FFFF_FFFF_0FFF) | 0x0000_0000_0000_4000) as i64;
    let least = ((second & 0x3FFF_FFFF_FFFF_FFFF) | 0x8000_0000_0000_0000) as i64;
    uuid_new(vm, most, least)
}

fn uuid_from_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = string_arg(vm, args[0])?;
    let Some((most, least)) = parse_uuid(&text) else {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some(&alloc::format!("Invalid UUID string: {text}")),
        ));
    };
    uuid_new(vm, most, least)
}

fn parse_uuid(text: &str) -> Option<(i64, i64)> {
    let mut digits: Vec<u8> = Vec::with_capacity(32);
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let ch = bytes[index];
        if ch == b'-' {
            index += 1;
            continue;
        }
        if !ch.is_ascii_hexdigit() {
            return None;
        }
        digits.push(ch);
        index += 1;
    }
    if digits.len() != 32 {
        return None;
    }
    // Reject misplaced hyphens in the canonical 8-4-4-4-12 form; anything else without hyphens
    // (32 hex digits) is accepted, like the JDK's fromString for canonical inputs.
    if text.contains('-') {
        let mut expected = 0usize;
        for (position, ch) in text.char_indices() {
            match position {
                8 | 13 | 18 | 23 => {
                    if ch != '-' {
                        return None;
                    }
                }
                _ => {
                    expected += 1;
                }
            }
        }
        if expected != 32 {
            return None;
        }
    }
    let mut most = 0i64;
    let mut least = 0i64;
    for (index, digit) in digits.iter().enumerate() {
        let value = (digit.to_ascii_lowercase() as char).to_digit(16)? as i64;
        if index < 16 {
            most = (most << 4) | value;
        } else {
            least = (least << 4) | value;
        }
    }
    Some((most, least))
}

fn uuid_to_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let (most, least) = uuid_bits(vm, receiver(args));
    let most = most as u64;
    let least = least as u64;
    let text = alloc::format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        (most >> 32) as u32,
        (most >> 16) as u16,
        (most & 0xFFFF) as u16,
        (least >> 48) as u16,
        least & 0xFFFF_FFFF_FFFF
    );
    object(vm.make_string(&text)?)
}
