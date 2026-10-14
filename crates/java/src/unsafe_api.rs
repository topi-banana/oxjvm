//! `jdk.internal.misc.Unsafe` and `sun.misc.Unsafe` stubs.
//!
//! Offsets are defined by this VM: an object field at storage slot `n` has
//! offset `n * 8`, and an array element uses `arrayBaseOffset` 0 with
//! `arrayIndexScale` equal to the element width.  Primitive accessors then
//! translate an offset back to a slot or element index.

use oxjvm_classfile::flags::*;

use crate::{boolean, class, field, int, long, method, object, ref_arg, void};
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

fn class_argument_value(vm: &mut Vm<'_>, value: Value) -> Result<oxjvm_vm::ClassId, VmError> {
    crate::lang::class_argument(vm, value.as_ref())
}

/// `arrayIndexScale` for a component class.
fn component_scale(vm: &Vm<'_>, class: oxjvm_vm::ClassId) -> i32 {
    let name = vm.class_name(class);
    match name {
        "boolean" | "byte" => 1,
        "char" | "short" => 2,
        "int" | "float" => 4,
        "long" | "double" => 8,
        _ => 4,
    }
}

fn array_component(vm: &Vm<'_>, array: ObjectRef) -> Option<oxjvm_vm::ClassId> {
    vm.heap.get(array).and_then(|object| match &object.data {
        oxjvm_vm::ObjectData::Array(_) => Some(object.class),
        _ => None,
    })
}

/// The array element index for a byte offset (`base` is 0).
fn element_index(offset: i64, scale: i32) -> i32 {
    ((offset.max(0)) / i64::from(scale.max(1))) as i32
}

fn array_scale_of(vm: &Vm<'_>, array: ObjectRef) -> i32 {
    // `array_component` yields the array class; derive the element width from
    // the array's own descriptor.
    let name = match array_component(vm, array) {
        Some(class) => vm.class_name(class),
        None => return 4,
    };
    let descriptor = name.trim_start_matches('[');
    match descriptor {
        "Z" | "B" => 1,
        "C" | "S" => 2,
        "I" | "F" => 4,
        "J" | "D" => 8,
        _ => 4,
    }
}

fn unsafe_get(
    vm: &mut Vm<'_>,
    target: ObjectRef,
    offset: i64,
    descriptor: &str,
) -> Result<Value, VmError> {
    if target.is_null() {
        return Ok(match descriptor {
            "J" | "D" => Value::Long(0),
            _ => Value::Int(0),
        });
    }
    if array_component(vm, target).is_some() {
        let scale = array_scale_of(vm, target);
        return vm.array_get(target, element_index(offset, scale));
    }
    let slot = element_index(offset, 8);
    match vm.heap.get(target).map(|object| &object.data) {
        Some(oxjvm_vm::ObjectData::Instance(fields)) => {
            Ok(fields.get(slot as usize).copied().unwrap_or(Value::Int(0)))
        }
        _ => Ok(Value::Int(0)),
    }
}

fn unsafe_put(
    vm: &mut Vm<'_>,
    target: ObjectRef,
    offset: i64,
    value: Value,
) -> Result<Value, VmError> {
    if target.is_null() {
        return void();
    }
    if array_component(vm, target).is_some() {
        let scale = array_scale_of(vm, target);
        vm.array_set(target, element_index(offset, scale), value, false)?;
        return void();
    }
    let slot = element_index(offset, 8) as usize;
    if let Some(oxjvm_vm::ObjectData::Instance(fields)) =
        vm.heap.get_mut(target).map(|object| &mut object.data)
    {
        if slot < fields.len() {
            fields[slot] = value;
        }
    }
    void()
}

fn value_as_long(value: Value) -> i64 {
    match value {
        Value::Long(value) => value,
        Value::Int(value) => i64::from(value),
        Value::Float(value) => i64::from(value as i32),
        Value::Double(value) => value as i64,
        Value::Ref(reference) => i64::from(reference.raw()),
    }
}

// The tables are built once per call site through `class`, so the definitions
// are spelled out with a const-compatible length below.
macro_rules! unsafe_class {
    ($constant:ident, $name:literal, $get_unsafe:expr) => {
        pub(crate) const $constant: oxjvm_vm::NativeClass = {
            let definition = class(
                $name,
                Some("java/lang/Object"),
                &[],
                ACC_PUBLIC | ACC_FINAL,
                &[field(
                    "theUnsafe",
                    concat!("L", $name, ";"),
                    ACC_PRIVATE | ACC_STATIC | ACC_FINAL,
                    None,
                )],
                &UNSAFE_METHODS,
                Some($get_unsafe),
            );
            definition
        };
    };
}

fn unsafe_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("sun/misc/Unsafe")?;
    if vm
        .read_static_ref_if_set(class, "theUnsafe", "Lsun/misc/Unsafe;")
        .is_none()
    {
        let instance = vm.new_instance(class)?;
        vm.set_static_value(
            class,
            "theUnsafe",
            "Lsun/misc/Unsafe;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

fn jdk_unsafe_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("jdk/internal/misc/Unsafe")?;
    if vm
        .read_static_ref_if_set(class, "theUnsafe", "Ljdk/internal/misc/Unsafe;")
        .is_none()
    {
        let instance = vm.new_instance(class)?;
        vm.set_static_value(
            class,
            "theUnsafe",
            "Ljdk/internal/misc/Unsafe;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

unsafe_class!(SUN_MISC_UNSAFE, "sun/misc/Unsafe", unsafe_clinit);
unsafe_class!(
    JDK_INTERNAL_UNSAFE,
    "jdk/internal/misc/Unsafe",
    jdk_unsafe_clinit
);

/// The method table is shared by both `Unsafe` spellings, with the static
/// `getUnsafe` handled per class in `clinit`.
static UNSAFE_METHODS: [oxjvm_vm::NativeMethodDef; 46] = [
    method(
        "getUnsafe",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, ctx, _| {
            let name: &str = vm.class_name(ctx.class);
            let descriptor = if name.contains("sun") {
                "Lsun/misc/Unsafe;"
            } else {
                "Ljdk/internal/misc/Unsafe;"
            };
            match vm.read_static_ref_if_set(ctx.class, "theUnsafe", descriptor) {
                Some(existing) => object(existing),
                None => Err(vm.throw_new("java/lang/SecurityException", None)),
            }
        },
    ),
    method("addressSize", "()I", ACC_PUBLIC, |_, _, _| int(8)),
    method("pageSize", "()I", ACC_PUBLIC, |_, _, _| int(4096)),
    method(
        "arrayBaseOffset",
        "(Ljava/lang/Class;)I",
        ACC_PUBLIC,
        |_, _, _| int(0),
    ),
    method(
        "arrayIndexScale",
        "(Ljava/lang/Class;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = class_argument_value(vm, args[1])?;
            int(component_scale(vm, class))
        },
    ),
    method(
        "objectFieldOffset",
        "(Ljava/lang/reflect/Field;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let reference = ref_arg(args, 1);
            let slot = if reference.is_null() {
                0
            } else {
                crate::lang::read_int_field(vm, reference, "slot")
            };
            long(i64::from(slot) * 8)
        },
    ),
    method(
        "allocateInstance",
        "(Ljava/lang/Class;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = class_argument_value(vm, args[1])?;
            let instance = vm.new_instance(class)?;
            object(instance)
        },
    ),
    method("fullFence", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("loadFence", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("storeFence", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "throwException",
        "(Ljava/lang/Throwable;)V",
        ACC_PUBLIC,
        |_, _, args| Err(VmError::Thrown(ref_arg(args, 1))),
    ),
    method(
        "getObject",
        "(Ljava/lang/Object;J)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| unsafe_get(vm, ref_arg(args, 1), crate::long_arg(args, 2), "L"),
    ),
    method(
        "getReference",
        "(Ljava/lang/Object;J)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| unsafe_get(vm, ref_arg(args, 1), crate::long_arg(args, 2), "L"),
    ),
    method(
        "putObject",
        "(Ljava/lang/Object;JLjava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            unsafe_put(
                vm,
                ref_arg(args, 1),
                crate::long_arg(args, 2),
                Value::Ref(ref_arg(args, 3)),
            )
        },
    ),
    method(
        "putReference",
        "(Ljava/lang/Object;JLjava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            unsafe_put(
                vm,
                ref_arg(args, 1),
                crate::long_arg(args, 2),
                Value::Ref(ref_arg(args, 3)),
            )
        },
    ),
    method(
        "getInt",
        "(Ljava/lang/Object;J)I",
        ACC_PUBLIC,
        |vm, _, args| unsafe_get(vm, ref_arg(args, 1), crate::long_arg(args, 2), "I"),
    ),
    method(
        "putInt",
        "(Ljava/lang/Object;JI)V",
        ACC_PUBLIC,
        |vm, _, args| {
            unsafe_put(
                vm,
                ref_arg(args, 1),
                crate::long_arg(args, 2),
                Value::Int(crate::int_arg(args, 3)),
            )
        },
    ),
    method(
        "getLong",
        "(Ljava/lang/Object;J)J",
        ACC_PUBLIC,
        |vm, _, args| unsafe_get(vm, ref_arg(args, 1), crate::long_arg(args, 2), "J"),
    ),
    method(
        "putLong",
        "(Ljava/lang/Object;JJ)V",
        ACC_PUBLIC,
        |vm, _, args| {
            unsafe_put(
                vm,
                ref_arg(args, 1),
                crate::long_arg(args, 2),
                Value::Long(crate::long_arg(args, 3)),
            )
        },
    ),
    method(
        "getBoolean",
        "(Ljava/lang/Object;J)Z",
        ACC_PUBLIC,
        |vm, _, args| unsafe_get(vm, ref_arg(args, 1), crate::long_arg(args, 2), "Z"),
    ),
    method(
        "putBoolean",
        "(Ljava/lang/Object;JZ)V",
        ACC_PUBLIC,
        |vm, _, args| {
            unsafe_put(
                vm,
                ref_arg(args, 1),
                crate::long_arg(args, 2),
                Value::Int(i32::from(crate::bool_arg(args, 3))),
            )
        },
    ),
    method(
        "getByte",
        "(Ljava/lang/Object;J)B",
        ACC_PUBLIC,
        |vm, _, args| unsafe_get(vm, ref_arg(args, 1), crate::long_arg(args, 2), "B"),
    ),
    method(
        "putByte",
        "(Ljava/lang/Object;JB)V",
        ACC_PUBLIC,
        |vm, _, args| {
            unsafe_put(
                vm,
                ref_arg(args, 1),
                crate::long_arg(args, 2),
                Value::Int(crate::int_arg(args, 3)),
            )
        },
    ),
    method(
        "getShort",
        "(Ljava/lang/Object;J)S",
        ACC_PUBLIC,
        |vm, _, args| unsafe_get(vm, ref_arg(args, 1), crate::long_arg(args, 2), "S"),
    ),
    method(
        "putShort",
        "(Ljava/lang/Object;JS)V",
        ACC_PUBLIC,
        |vm, _, args| {
            unsafe_put(
                vm,
                ref_arg(args, 1),
                crate::long_arg(args, 2),
                Value::Int(crate::int_arg(args, 3)),
            )
        },
    ),
    method(
        "getChar",
        "(Ljava/lang/Object;J)C",
        ACC_PUBLIC,
        |vm, _, args| unsafe_get(vm, ref_arg(args, 1), crate::long_arg(args, 2), "C"),
    ),
    method(
        "putChar",
        "(Ljava/lang/Object;JC)V",
        ACC_PUBLIC,
        |vm, _, args| {
            unsafe_put(
                vm,
                ref_arg(args, 1),
                crate::long_arg(args, 2),
                Value::Int(crate::int_arg(args, 3)),
            )
        },
    ),
    method(
        "compareAndSetInt",
        "(Ljava/lang/Object;JII)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let target = ref_arg(args, 1);
            let offset = crate::long_arg(args, 2);
            let current = value_as_long(unsafe_get(vm, target, offset, "I")?);
            if current == i64::from(crate::int_arg(args, 3)) {
                unsafe_put(vm, target, offset, Value::Int(crate::int_arg(args, 4)))?;
                boolean(true)
            } else {
                boolean(false)
            }
        },
    ),
    method(
        "compareAndSetLong",
        "(Ljava/lang/Object;JJJ)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let target = ref_arg(args, 1);
            let offset = crate::long_arg(args, 2);
            let current = value_as_long(unsafe_get(vm, target, offset, "J")?);
            if current == crate::long_arg(args, 3) {
                unsafe_put(vm, target, offset, Value::Long(crate::long_arg(args, 4)))?;
                boolean(true)
            } else {
                boolean(false)
            }
        },
    ),
    method(
        "compareAndSetObject",
        "(Ljava/lang/Object;JLjava/lang/Object;Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let target = ref_arg(args, 1);
            let offset = crate::long_arg(args, 2);
            let current = unsafe_get(vm, target, offset, "L")?.as_ref();
            if current == ref_arg(args, 3) {
                unsafe_put(vm, target, offset, Value::Ref(ref_arg(args, 4)))?;
                boolean(true)
            } else {
                boolean(false)
            }
        },
    ),
    method(
        "getAndAddInt",
        "(Ljava/lang/Object;JI)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let target = ref_arg(args, 1);
            let offset = crate::long_arg(args, 2);
            let current = unsafe_get(vm, target, offset, "I")?.as_int();
            unsafe_put(
                vm,
                target,
                offset,
                Value::Int(current.wrapping_add(crate::int_arg(args, 3))),
            )?;
            int(current)
        },
    ),
    method(
        "getAndAddLong",
        "(Ljava/lang/Object;JJ)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let target = ref_arg(args, 1);
            let offset = crate::long_arg(args, 2);
            let current = unsafe_get(vm, target, offset, "J")?.as_long();
            unsafe_put(
                vm,
                target,
                offset,
                Value::Long(current.wrapping_add(crate::long_arg(args, 3))),
            )?;
            long(current)
        },
    ),
    method(
        "getAndSetInt",
        "(Ljava/lang/Object;JI)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let target = ref_arg(args, 1);
            let offset = crate::long_arg(args, 2);
            let current = unsafe_get(vm, target, offset, "I")?.as_int();
            unsafe_put(vm, target, offset, Value::Int(crate::int_arg(args, 3)))?;
            int(current)
        },
    ),
    method(
        "getAndSetObject",
        "(Ljava/lang/Object;JLjava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let target = ref_arg(args, 1);
            let offset = crate::long_arg(args, 2);
            let current = unsafe_get(vm, target, offset, "L")?;
            unsafe_put(vm, target, offset, Value::Ref(ref_arg(args, 3)))?;
            Ok(current)
        },
    ),
    // sun.misc.Unsafe spells compareAndSet as compareAndSwap; both spellings
    // exist on jdk.internal.misc.Unsafe.
    method(
        "compareAndSwapInt",
        "(Ljava/lang/Object;JII)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let target = ref_arg(args, 1);
            let offset = crate::long_arg(args, 2);
            let current = value_as_long(unsafe_get(vm, target, offset, "I")?);
            if current == i64::from(crate::int_arg(args, 3)) {
                unsafe_put(vm, target, offset, Value::Int(crate::int_arg(args, 4)))?;
                boolean(true)
            } else {
                boolean(false)
            }
        },
    ),
    method(
        "compareAndSwapLong",
        "(Ljava/lang/Object;JJJ)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let target = ref_arg(args, 1);
            let offset = crate::long_arg(args, 2);
            let current = value_as_long(unsafe_get(vm, target, offset, "J")?);
            if current == crate::long_arg(args, 3) {
                unsafe_put(vm, target, offset, Value::Long(crate::long_arg(args, 4)))?;
                boolean(true)
            } else {
                boolean(false)
            }
        },
    ),
    method(
        "compareAndSwapObject",
        "(Ljava/lang/Object;JLjava/lang/Object;Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let target = ref_arg(args, 1);
            let offset = crate::long_arg(args, 2);
            let current = unsafe_get(vm, target, offset, "L")?.as_ref();
            if current == ref_arg(args, 3) {
                unsafe_put(vm, target, offset, Value::Ref(ref_arg(args, 4)))?;
                boolean(true)
            } else {
                boolean(false)
            }
        },
    ),
    // The volatile/ordered spellings are ordinary accesses in this VM.
    method(
        "getIntVolatile",
        "(Ljava/lang/Object;J)I",
        ACC_PUBLIC,
        |vm, _, args| unsafe_get(vm, ref_arg(args, 1), crate::long_arg(args, 2), "I"),
    ),
    method(
        "putIntVolatile",
        "(Ljava/lang/Object;JI)V",
        ACC_PUBLIC,
        |vm, _, args| {
            unsafe_put(
                vm,
                ref_arg(args, 1),
                crate::long_arg(args, 2),
                Value::Int(crate::int_arg(args, 3)),
            )
        },
    ),
    method(
        "getLongVolatile",
        "(Ljava/lang/Object;J)J",
        ACC_PUBLIC,
        |vm, _, args| unsafe_get(vm, ref_arg(args, 1), crate::long_arg(args, 2), "J"),
    ),
    method(
        "putLongVolatile",
        "(Ljava/lang/Object;JJ)V",
        ACC_PUBLIC,
        |vm, _, args| {
            unsafe_put(
                vm,
                ref_arg(args, 1),
                crate::long_arg(args, 2),
                Value::Long(crate::long_arg(args, 3)),
            )
        },
    ),
    method(
        "getObjectVolatile",
        "(Ljava/lang/Object;J)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| unsafe_get(vm, ref_arg(args, 1), crate::long_arg(args, 2), "L"),
    ),
    method(
        "putObjectVolatile",
        "(Ljava/lang/Object;JLjava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            unsafe_put(
                vm,
                ref_arg(args, 1),
                crate::long_arg(args, 2),
                Value::Ref(ref_arg(args, 3)),
            )
        },
    ),
    method(
        "putOrderedInt",
        "(Ljava/lang/Object;JI)V",
        ACC_PUBLIC,
        |vm, _, args| {
            unsafe_put(
                vm,
                ref_arg(args, 1),
                crate::long_arg(args, 2),
                Value::Int(crate::int_arg(args, 3)),
            )
        },
    ),
    method(
        "putOrderedLong",
        "(Ljava/lang/Object;JJ)V",
        ACC_PUBLIC,
        |vm, _, args| {
            unsafe_put(
                vm,
                ref_arg(args, 1),
                crate::long_arg(args, 2),
                Value::Long(crate::long_arg(args, 3)),
            )
        },
    ),
    method(
        "putOrderedObject",
        "(Ljava/lang/Object;JLjava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            unsafe_put(
                vm,
                ref_arg(args, 1),
                crate::long_arg(args, 2),
                Value::Ref(ref_arg(args, 3)),
            )
        },
    ),
];
