//! `java.util.concurrent.atomic`: `AtomicInteger`, `AtomicLong` and `AtomicBoolean`.
//!
//! These are plain objects with a single `value` field; the guest's own concurrency guarantees are
//! met because the interpreter executes one instruction at a time.

use alloc::string::{String, ToString};
use alloc::vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError, format};

use crate::boxed::{read_value_int, read_value_long, write_value_int, write_value_long};
use crate::{
    boolean, class, double, double_arg, field, float, int, long, method, object, receiver, ref_arg,
    void,
};

/// Apply an int operator to the current value, store the result, return (old, new).
fn atomic_int_apply(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    op: ObjectRef,
    name: &str,
    descriptor: &str,
    extra: Option<i32>,
) -> Result<(i32, i32), VmError> {
    if op.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let old = read_value_int(vm, this);
    let (class, method) = vm.resolve_virtual_method(op, name, descriptor)?;
    let mut args = vec![Value::Ref(op), Value::Int(old)];
    if let Some(value) = extra {
        args.push(Value::Int(value));
    }
    let new = vm.invoke_method(class, method, args)?.as_int();
    write_value_int(vm, this, new);
    Ok((old, new))
}

/// Apply a long operator to the current value, store the result, return (old, new).
fn atomic_long_apply(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    op: ObjectRef,
    name: &str,
    descriptor: &str,
    extra: Option<i64>,
) -> Result<(i64, i64), VmError> {
    if op.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let old = read_value_long(vm, this);
    let (class, method) = vm.resolve_virtual_method(op, name, descriptor)?;
    let mut args = vec![Value::Ref(op), Value::Long(old)];
    if let Some(value) = extra {
        args.push(Value::Long(value));
    }
    let new = vm.invoke_method(class, method, args)?.as_long();
    write_value_long(vm, this, new);
    Ok((old, new))
}

/// Apply a reference operator to the current value, store the result, return (old, new).
fn atomic_reference_apply(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    op: ObjectRef,
    extra: Option<ObjectRef>,
) -> Result<(ObjectRef, ObjectRef), VmError> {
    if op.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let old = atomic_reference_value(vm, this);
    let (name, descriptor) = if extra.is_some() {
        (
            "apply",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        )
    } else {
        ("apply", "(Ljava/lang/Object;)Ljava/lang/Object;")
    };
    let (class, method) = vm.resolve_virtual_method(op, name, descriptor)?;
    let mut args = vec![Value::Ref(op), Value::Ref(old)];
    if let Some(value) = extra {
        args.push(Value::Ref(value));
    }
    let new = vm.invoke_method(class, method, args)?.as_ref();
    atomic_reference_set(vm, this, new);
    Ok((old, new))
}

// -------------------------------------------------------------------------------------------
// AtomicInteger
// -------------------------------------------------------------------------------------------

const ATOMIC_INTEGER_METHODS: [oxjvm_vm::NativeMethodDef; 22] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        write_value_int(vm, receiver(args), 0);
        void()
    }),
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        write_value_int(vm, receiver(args), crate::int_arg(args, 1));
        void()
    }),
    method("get", "()I", ACC_PUBLIC | ACC_FINAL, |vm, _, args| {
        int(read_value_int(vm, receiver(args)))
    }),
    method("set", "(I)V", ACC_PUBLIC | ACC_FINAL, |vm, _, args| {
        write_value_int(vm, receiver(args), crate::int_arg(args, 1));
        void()
    }),
    method(
        "getAndSet",
        "(I)I",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let previous = read_value_int(vm, this);
            write_value_int(vm, this, crate::int_arg(args, 1));
            int(previous)
        },
    ),
    method(
        "compareAndSet",
        "(II)Z",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let expected = crate::int_arg(args, 1);
            if read_value_int(vm, this) == expected {
                write_value_int(vm, this, crate::int_arg(args, 2));
                boolean(true)
            } else {
                boolean(false)
            }
        },
    ),
    method(
        "weakCompareAndSet",
        "(II)Z",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let expected = crate::int_arg(args, 1);
            if read_value_int(vm, this) == expected {
                write_value_int(vm, this, crate::int_arg(args, 2));
                boolean(true)
            } else {
                boolean(false)
            }
        },
    ),
    method(
        "weakCompareAndSetPlain",
        "(II)Z",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let expected = crate::int_arg(args, 1);
            if read_value_int(vm, this) == expected {
                write_value_int(vm, this, crate::int_arg(args, 2));
                boolean(true)
            } else {
                boolean(false)
            }
        },
    ),
    method(
        "getAndIncrement",
        "()I",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let previous = read_value_int(vm, this);
            write_value_int(vm, this, previous.wrapping_add(1));
            int(previous)
        },
    ),
    method(
        "getAndDecrement",
        "()I",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let previous = read_value_int(vm, this);
            write_value_int(vm, this, previous.wrapping_sub(1));
            int(previous)
        },
    ),
    method(
        "getAndAdd",
        "(I)I",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let previous = read_value_int(vm, this);
            write_value_int(vm, this, previous.wrapping_add(crate::int_arg(args, 1)));
            int(previous)
        },
    ),
    method(
        "incrementAndGet",
        "()I",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let value = read_value_int(vm, this).wrapping_add(1);
            write_value_int(vm, this, value);
            int(value)
        },
    ),
    method(
        "decrementAndGet",
        "()I",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let value = read_value_int(vm, this).wrapping_sub(1);
            write_value_int(vm, this, value);
            int(value)
        },
    ),
    method(
        "addAndGet",
        "(I)I",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let value = read_value_int(vm, this).wrapping_add(crate::int_arg(args, 1));
            write_value_int(vm, this, value);
            int(value)
        },
    ),
    method("intValue", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_value_int(vm, receiver(args)))
    }),
    method("longValue", "()J", ACC_PUBLIC, |vm, _, args| {
        long(i64::from(read_value_int(vm, receiver(args))))
    }),
    method("floatValue", "()F", ACC_PUBLIC, |vm, _, args| {
        float(read_value_int(vm, receiver(args)) as f32)
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
    method(
        "getAndUpdate",
        "(Ljava/util/function/IntUnaryOperator;)I",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let (old, _) = atomic_int_apply(
                vm,
                receiver(args),
                ref_arg(args, 1),
                "applyAsInt",
                "(I)I",
                None,
            )?;
            int(old)
        },
    ),
    method(
        "updateAndGet",
        "(Ljava/util/function/IntUnaryOperator;)I",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let (_, new) = atomic_int_apply(
                vm,
                receiver(args),
                ref_arg(args, 1),
                "applyAsInt",
                "(I)I",
                None,
            )?;
            int(new)
        },
    ),
    method(
        "getAndAccumulate",
        "(ILjava/util/function/IntBinaryOperator;)I",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let (old, _) = atomic_int_apply(
                vm,
                receiver(args),
                ref_arg(args, 2),
                "applyAsInt",
                "(II)I",
                Some(crate::int_arg(args, 1)),
            )?;
            int(old)
        },
    ),
    method(
        "accumulateAndGet",
        "(ILjava/util/function/IntBinaryOperator;)I",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let (_, new) = atomic_int_apply(
                vm,
                receiver(args),
                ref_arg(args, 2),
                "applyAsInt",
                "(II)I",
                Some(crate::int_arg(args, 1)),
            )?;
            int(new)
        },
    ),
];

pub(crate) const ATOMIC_INTEGER: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/atomic/AtomicInteger",
    Some("java/lang/Number"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[field("value", "I", ACC_PRIVATE | ACC_VOLATILE, None)],
    &ATOMIC_INTEGER_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// AtomicLong
// -------------------------------------------------------------------------------------------

const ATOMIC_LONG_METHODS: [oxjvm_vm::NativeMethodDef; 21] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        write_value_long(vm, receiver(args), 0);
        void()
    }),
    method("<init>", "(J)V", ACC_PUBLIC, |vm, _, args| {
        write_value_long(vm, receiver(args), crate::long_arg(args, 1));
        void()
    }),
    method("get", "()J", ACC_PUBLIC | ACC_FINAL, |vm, _, args| {
        long(read_value_long(vm, receiver(args)))
    }),
    method("set", "(J)V", ACC_PUBLIC | ACC_FINAL, |vm, _, args| {
        write_value_long(vm, receiver(args), crate::long_arg(args, 1));
        void()
    }),
    method(
        "getAndSet",
        "(J)J",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let previous = read_value_long(vm, this);
            write_value_long(vm, this, crate::long_arg(args, 1));
            long(previous)
        },
    ),
    method(
        "compareAndSet",
        "(JJ)Z",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            if read_value_long(vm, this) == crate::long_arg(args, 1) {
                write_value_long(vm, this, crate::long_arg(args, 2));
                boolean(true)
            } else {
                boolean(false)
            }
        },
    ),
    method(
        "weakCompareAndSet",
        "(JJ)Z",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            if read_value_long(vm, this) == crate::long_arg(args, 1) {
                write_value_long(vm, this, crate::long_arg(args, 2));
                boolean(true)
            } else {
                boolean(false)
            }
        },
    ),
    method(
        "getAndIncrement",
        "()J",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let previous = read_value_long(vm, this);
            write_value_long(vm, this, previous.wrapping_add(1));
            long(previous)
        },
    ),
    method(
        "getAndDecrement",
        "()J",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let previous = read_value_long(vm, this);
            write_value_long(vm, this, previous.wrapping_sub(1));
            long(previous)
        },
    ),
    method(
        "getAndAdd",
        "(J)J",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let previous = read_value_long(vm, this);
            write_value_long(vm, this, previous.wrapping_add(crate::long_arg(args, 1)));
            long(previous)
        },
    ),
    method(
        "incrementAndGet",
        "()J",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let value = read_value_long(vm, this).wrapping_add(1);
            write_value_long(vm, this, value);
            long(value)
        },
    ),
    method(
        "decrementAndGet",
        "()J",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let value = read_value_long(vm, this).wrapping_sub(1);
            write_value_long(vm, this, value);
            long(value)
        },
    ),
    method(
        "addAndGet",
        "(J)J",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let value = read_value_long(vm, this).wrapping_add(crate::long_arg(args, 1));
            write_value_long(vm, this, value);
            long(value)
        },
    ),
    method("intValue", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_value_long(vm, receiver(args)) as i32)
    }),
    method("longValue", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_value_long(vm, receiver(args)))
    }),
    method("floatValue", "()F", ACC_PUBLIC, |vm, _, args| {
        float(read_value_long(vm, receiver(args)) as f32)
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
    method(
        "getAndUpdate",
        "(Ljava/util/function/LongUnaryOperator;)J",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let (old, _) = atomic_long_apply(
                vm,
                receiver(args),
                ref_arg(args, 1),
                "applyAsLong",
                "(J)J",
                None,
            )?;
            long(old)
        },
    ),
    method(
        "updateAndGet",
        "(Ljava/util/function/LongUnaryOperator;)J",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let (_, new) = atomic_long_apply(
                vm,
                receiver(args),
                ref_arg(args, 1),
                "applyAsLong",
                "(J)J",
                None,
            )?;
            long(new)
        },
    ),
    method(
        "getAndAccumulate",
        "(JLjava/util/function/LongBinaryOperator;)J",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let (old, _) = atomic_long_apply(
                vm,
                receiver(args),
                ref_arg(args, 2),
                "applyAsLong",
                "(JJ)J",
                Some(crate::long_arg(args, 1)),
            )?;
            long(old)
        },
    ),
    method(
        "accumulateAndGet",
        "(JLjava/util/function/LongBinaryOperator;)J",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let (_, new) = atomic_long_apply(
                vm,
                receiver(args),
                ref_arg(args, 2),
                "applyAsLong",
                "(JJ)J",
                Some(crate::long_arg(args, 1)),
            )?;
            long(new)
        },
    ),
];

pub(crate) const ATOMIC_LONG: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/atomic/AtomicLong",
    Some("java/lang/Number"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[field("value", "J", ACC_PRIVATE | ACC_VOLATILE, None)],
    &ATOMIC_LONG_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// AtomicBoolean
// -------------------------------------------------------------------------------------------

const ATOMIC_BOOLEAN_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        write_value_int(vm, receiver(args), 0);
        void()
    }),
    method("<init>", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        write_value_int(vm, receiver(args), i32::from(crate::bool_arg(args, 1)));
        void()
    }),
    method("get", "()Z", ACC_PUBLIC | ACC_FINAL, |vm, _, args| {
        boolean(read_value_int(vm, receiver(args)) != 0)
    }),
    method("set", "(Z)V", ACC_PUBLIC | ACC_FINAL, |vm, _, args| {
        write_value_int(vm, receiver(args), i32::from(crate::bool_arg(args, 1)));
        void()
    }),
    method(
        "getAndSet",
        "(Z)Z",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let previous = read_value_int(vm, this) != 0;
            write_value_int(vm, this, i32::from(crate::bool_arg(args, 1)));
            boolean(previous)
        },
    ),
    method(
        "compareAndSet",
        "(ZZ)Z",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            if (read_value_int(vm, this) != 0) == crate::bool_arg(args, 1) {
                write_value_int(vm, this, i32::from(crate::bool_arg(args, 2)));
                boolean(true)
            } else {
                boolean(false)
            }
        },
    ),
    method(
        "weakCompareAndSet",
        "(ZZ)Z",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            if (read_value_int(vm, this) != 0) == crate::bool_arg(args, 1) {
                write_value_int(vm, this, i32::from(crate::bool_arg(args, 2)));
                boolean(true)
            } else {
                boolean(false)
            }
        },
    ),
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
];

pub(crate) const ATOMIC_BOOLEAN: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/atomic/AtomicBoolean",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[field("value", "Z", ACC_PRIVATE | ACC_VOLATILE, None)],
    &ATOMIC_BOOLEAN_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// AtomicReference
// -------------------------------------------------------------------------------------------

fn atomic_reference_value(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "value", "Ljava/lang/Object;")
        .unwrap_or(ObjectRef::NULL)
}

fn atomic_reference_set(vm: &mut Vm<'_>, this: ObjectRef, value: ObjectRef) {
    if let Ok((declaring, field)) = vm.find_field(vm.class_of(this), "value", "Ljava/lang/Object;")
    {
        vm.set_instance_ref(this, declaring, field, value);
    }
}

const ATOMIC_REFERENCE_METHODS: [oxjvm_vm::NativeMethodDef; 13] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        atomic_reference_set(vm, receiver(args), ObjectRef::NULL);
        void()
    }),
    method(
        "<init>",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            atomic_reference_set(vm, receiver(args), ref_arg(args, 1));
            void()
        },
    ),
    method(
        "get",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| object(atomic_reference_value(vm, receiver(args))),
    ),
    method(
        "set",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            atomic_reference_set(vm, receiver(args), ref_arg(args, 1));
            void()
        },
    ),
    method(
        "getAndSet",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let previous = atomic_reference_value(vm, this);
            atomic_reference_set(vm, this, ref_arg(args, 1));
            object(previous)
        },
    ),
    method(
        "compareAndSet",
        "(Ljava/lang/Object;Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let expected = ref_arg(args, 1);
            let current = atomic_reference_value(vm, this);
            if current == expected {
                atomic_reference_set(vm, this, ref_arg(args, 2));
                boolean(true)
            } else {
                boolean(false)
            }
        },
    ),
    method(
        "weakCompareAndSet",
        "(Ljava/lang/Object;Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let expected = ref_arg(args, 1);
            let current = atomic_reference_value(vm, this);
            if current == expected {
                atomic_reference_set(vm, this, ref_arg(args, 2));
                boolean(true)
            } else {
                boolean(false)
            }
        },
    ),
    method(
        "lazySet",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            atomic_reference_set(vm, receiver(args), ref_arg(args, 1));
            void()
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = atomic_reference_value(vm, receiver(args));
            let text = vm.display_value(
                &oxjvm_classfile::FieldType::Object("java/lang/Object".into()),
                Value::Ref(value),
            )?;
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "getAndUpdate",
        "(Ljava/util/function/UnaryOperator;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let (old, _) = atomic_reference_apply(vm, receiver(args), ref_arg(args, 1), None)?;
            object(old)
        },
    ),
    method(
        "updateAndGet",
        "(Ljava/util/function/UnaryOperator;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let (_, new) = atomic_reference_apply(vm, receiver(args), ref_arg(args, 1), None)?;
            object(new)
        },
    ),
    method(
        "getAndAccumulate",
        "(Ljava/lang/Object;Ljava/util/function/BinaryOperator;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let (old, _) = atomic_reference_apply(
                vm,
                receiver(args),
                ref_arg(args, 2),
                Some(ref_arg(args, 1)),
            )?;
            object(old)
        },
    ),
    method(
        "accumulateAndGet",
        "(Ljava/lang/Object;Ljava/util/function/BinaryOperator;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let (_, new) = atomic_reference_apply(
                vm,
                receiver(args),
                ref_arg(args, 2),
                Some(ref_arg(args, 1)),
            )?;
            object(new)
        },
    ),
];

/// `AtomicReference`: a single volatile reference field.
pub(crate) const ATOMIC_REFERENCE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/atomic/AtomicReference",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[field(
        "value",
        "Ljava/lang/Object;",
        ACC_PRIVATE | ACC_VOLATILE,
        None,
    )],
    &ATOMIC_REFERENCE_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// LongAdder / LongAccumulator / DoubleAdder / DoubleAccumulator
// -------------------------------------------------------------------------------------------

const LONG_ADDER_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        write_value_long(vm, receiver(args), 0);
        void()
    }),
    method("add", "(J)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let sum = read_value_long(vm, this).wrapping_add(crate::long_arg(args, 1));
        write_value_long(vm, this, sum);
        void()
    }),
    method("increment", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let sum = read_value_long(vm, this).wrapping_add(1);
        write_value_long(vm, this, sum);
        void()
    }),
    method("decrement", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let sum = read_value_long(vm, this).wrapping_sub(1);
        write_value_long(vm, this, sum);
        void()
    }),
    method("sum", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_value_long(vm, receiver(args)))
    }),
    method("reset", "()V", ACC_PUBLIC, |vm, _, args| {
        write_value_long(vm, receiver(args), 0);
        void()
    }),
    method("sumThenReset", "()J", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let previous = read_value_long(vm, this);
        write_value_long(vm, this, 0);
        long(previous)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = read_value_long(vm, receiver(args)).to_string();
            object(vm.make_string(&text)?)
        },
    ),
    method("longValue", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_value_long(vm, receiver(args)))
    }),
    method("intValue", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_value_long(vm, receiver(args)) as i32)
    }),
    method("floatValue", "()F", ACC_PUBLIC, |vm, _, args| {
        float(read_value_long(vm, receiver(args)) as f32)
    }),
    method("doubleValue", "()D", ACC_PUBLIC, |vm, _, args| {
        double(read_value_long(vm, receiver(args)) as f64)
    }),
];

/// `LongAdder`: a sum counter modelled as a single `value` field; the interpreter executes
/// one instruction at a time, so the striped-cell layout is unnecessary.
pub(crate) const LONG_ADDER: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/atomic/LongAdder",
    Some("java/lang/Number"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[field("value", "J", ACC_PRIVATE, None)],
    &LONG_ADDER_METHODS,
    None,
);

const LONG_ACCUMULATOR_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method(
        "<init>",
        "(Ljava/util/function/LongBinaryOperator;J)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let function = ref_arg(args, 1);
            if function.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let identity = crate::long_arg(args, 2);
            write_value_long(vm, this, identity);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "identity", "J")?;
            vm.set_instance_long(this, declaring, field, identity);
            let (declaring, field) =
                vm.find_field(class, "function", "Ljava/util/function/LongBinaryOperator;")?;
            vm.set_instance_ref(this, declaring, field, function);
            void()
        },
    ),
    method("accumulate", "(J)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let function = vm
            .read_ref_field(
                this,
                vm.class_of(this),
                "function",
                "Ljava/util/function/LongBinaryOperator;",
            )
            .unwrap_or(ObjectRef::NULL);
        let (class, slot) = vm.resolve_virtual_method(function, "applyAsLong", "(JJ)J")?;
        let result = vm.invoke_method(
            class,
            slot,
            vec![
                Value::Ref(function),
                Value::Long(read_value_long(vm, this)),
                args[1],
            ],
        )?;
        write_value_long(vm, this, result.as_long());
        void()
    }),
    method("get", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_value_long(vm, receiver(args)))
    }),
    method("reset", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let identity = vm
            .read_instance_long_named(this, vm.class_of(this), "identity", "J")
            .unwrap_or(0);
        write_value_long(vm, this, identity);
        void()
    }),
    method("getThenReset", "()J", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let previous = read_value_long(vm, this);
        let identity = vm
            .read_instance_long_named(this, vm.class_of(this), "identity", "J")
            .unwrap_or(0);
        write_value_long(vm, this, identity);
        long(previous)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = read_value_long(vm, receiver(args)).to_string();
            object(vm.make_string(&text)?)
        },
    ),
    method("longValue", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_value_long(vm, receiver(args)))
    }),
    method("intValue", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_value_long(vm, receiver(args)) as i32)
    }),
    method("floatValue", "()F", ACC_PUBLIC, |vm, _, args| {
        float(read_value_long(vm, receiver(args)) as f32)
    }),
    method("doubleValue", "()D", ACC_PUBLIC, |vm, _, args| {
        double(read_value_long(vm, receiver(args)) as f64)
    }),
];

/// `LongAccumulator`: a value folded through a guest `LongBinaryOperator`.
pub(crate) const LONG_ACCUMULATOR: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/atomic/LongAccumulator",
    Some("java/lang/Number"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[
        field("value", "J", ACC_PRIVATE, None),
        field("identity", "J", ACC_PRIVATE, None),
        field(
            "function",
            "Ljava/util/function/LongBinaryOperator;",
            ACC_PRIVATE,
            None,
        ),
    ],
    &LONG_ACCUMULATOR_METHODS,
    None,
);

const DOUBLE_ADDER_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        write_value_long(vm, receiver(args), 0);
        void()
    }),
    method("add", "(D)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let sum = f64::from_bits(read_value_long(vm, this) as u64) + double_arg(args, 1);
        write_value_long(vm, this, sum.to_bits() as i64);
        void()
    }),
    method("sum", "()D", ACC_PUBLIC, |vm, _, args| {
        double(f64::from_bits(read_value_long(vm, receiver(args)) as u64))
    }),
    method("reset", "()V", ACC_PUBLIC, |vm, _, args| {
        write_value_long(vm, receiver(args), 0);
        void()
    }),
    method("sumThenReset", "()D", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let previous = f64::from_bits(read_value_long(vm, this) as u64);
        write_value_long(vm, this, 0);
        double(previous)
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
    method("longValue", "()J", ACC_PUBLIC, |vm, _, args| {
        long(f64::from_bits(read_value_long(vm, receiver(args)) as u64) as i64)
    }),
    method("intValue", "()I", ACC_PUBLIC, |vm, _, args| {
        int(f64::from_bits(read_value_long(vm, receiver(args)) as u64) as i32)
    }),
    method("floatValue", "()F", ACC_PUBLIC, |vm, _, args| {
        float(f64::from_bits(read_value_long(vm, receiver(args)) as u64) as f32)
    }),
    method("doubleValue", "()D", ACC_PUBLIC, |vm, _, args| {
        double(f64::from_bits(read_value_long(vm, receiver(args)) as u64))
    }),
];

/// `DoubleAdder`: a floating-point sum counter modelled as a single `value` field.
pub(crate) const DOUBLE_ADDER: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/atomic/DoubleAdder",
    Some("java/lang/Number"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[field("value", "D", ACC_PRIVATE, None)],
    &DOUBLE_ADDER_METHODS,
    None,
);

const DOUBLE_ACCUMULATOR_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method(
        "<init>",
        "(Ljava/util/function/DoubleBinaryOperator;D)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let function = ref_arg(args, 1);
            if function.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let identity = double_arg(args, 2);
            write_value_long(vm, this, identity.to_bits() as i64);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "identity", "D")?;
            vm.set_instance_long(this, declaring, field, identity.to_bits() as i64);
            let (declaring, field) = vm.find_field(
                class,
                "function",
                "Ljava/util/function/DoubleBinaryOperator;",
            )?;
            vm.set_instance_ref(this, declaring, field, function);
            void()
        },
    ),
    method("accumulate", "(D)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let function = vm
            .read_ref_field(
                this,
                vm.class_of(this),
                "function",
                "Ljava/util/function/DoubleBinaryOperator;",
            )
            .unwrap_or(ObjectRef::NULL);
        let (class, slot) = vm.resolve_virtual_method(function, "applyAsDouble", "(DD)D")?;
        let result = vm.invoke_method(
            class,
            slot,
            vec![
                Value::Ref(function),
                Value::Double(f64::from_bits(read_value_long(vm, this) as u64)),
                args[1],
            ],
        )?;
        write_value_long(vm, this, result.as_double().to_bits() as i64);
        void()
    }),
    method("get", "()D", ACC_PUBLIC, |vm, _, args| {
        double(f64::from_bits(read_value_long(vm, receiver(args)) as u64))
    }),
    method("reset", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let identity = vm
            .read_instance_long_named(this, vm.class_of(this), "identity", "D")
            .unwrap_or(0);
        write_value_long(vm, this, identity);
        void()
    }),
    method("getThenReset", "()D", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let previous = f64::from_bits(read_value_long(vm, this) as u64);
        let identity = vm
            .read_instance_long_named(this, vm.class_of(this), "identity", "D")
            .unwrap_or(0);
        write_value_long(vm, this, identity);
        double(previous)
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
    method("longValue", "()J", ACC_PUBLIC, |vm, _, args| {
        long(f64::from_bits(read_value_long(vm, receiver(args)) as u64) as i64)
    }),
    method("intValue", "()I", ACC_PUBLIC, |vm, _, args| {
        int(f64::from_bits(read_value_long(vm, receiver(args)) as u64) as i32)
    }),
    method("floatValue", "()F", ACC_PUBLIC, |vm, _, args| {
        float(f64::from_bits(read_value_long(vm, receiver(args)) as u64) as f32)
    }),
    method("doubleValue", "()D", ACC_PUBLIC, |vm, _, args| {
        double(f64::from_bits(read_value_long(vm, receiver(args)) as u64))
    }),
];

/// `DoubleAccumulator`: a value folded through a guest `DoubleBinaryOperator`.
pub(crate) const DOUBLE_ACCUMULATOR: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/atomic/DoubleAccumulator",
    Some("java/lang/Number"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[
        field("value", "D", ACC_PRIVATE, None),
        field("identity", "D", ACC_PRIVATE, None),
        field(
            "function",
            "Ljava/util/function/DoubleBinaryOperator;",
            ACC_PRIVATE,
            None,
        ),
    ],
    &DOUBLE_ACCUMULATOR_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Atomic arrays
// -------------------------------------------------------------------------------------------

fn atomic_int_array(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "array", "[I")
        .unwrap_or(ObjectRef::NULL)
}

fn atomic_long_array(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "array", "[J")
        .unwrap_or(ObjectRef::NULL)
}

fn atomic_ref_array(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "array", "[Ljava/lang/Object;")
        .unwrap_or(ObjectRef::NULL)
}

fn atomic_int_array_add(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    index: i32,
    delta: i32,
) -> Result<(i32, i32), VmError> {
    let array = atomic_int_array(vm, this);
    let old = vm.array_get(array, index)?.as_int();
    let new = old.wrapping_add(delta);
    vm.array_set(array, index, Value::Int(new), false)?;
    Ok((old, new))
}

fn atomic_long_array_add(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    index: i32,
    delta: i64,
) -> Result<(i64, i64), VmError> {
    let array = atomic_long_array(vm, this);
    let old = vm.array_get(array, index)?.as_long();
    let new = old.wrapping_add(delta);
    vm.array_set(array, index, Value::Long(new), false)?;
    Ok((old, new))
}

const ATOMIC_INTEGER_ARRAY_METHODS: [oxjvm_vm::NativeMethodDef; 16] = [
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let length = crate::int_arg(args, 1);
        if length < 0 {
            return Err(vm.throw_new("java/lang/NegativeArraySizeException", None));
        }
        let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Int, length as usize)?;
        let class = vm.class_of(this);
        let (declaring, field) = vm.find_field(class, "array", "[I")?;
        vm.set_instance_ref(this, declaring, field, array);
        void()
    }),
    method("<init>", "([I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let source = ref_arg(args, 1);
        if source.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        let length = vm.array_length(source)?;
        let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Int, length)?;
        for index in 0..length {
            let value = vm.array_get(source, index as i32)?;
            vm.array_set(array, index as i32, value, false)?;
        }
        let class = vm.class_of(this);
        let (declaring, field) = vm.find_field(class, "array", "[I")?;
        vm.set_instance_ref(this, declaring, field, array);
        void()
    }),
    method("length", "()I", ACC_PUBLIC, |vm, _, args| {
        int(vm.array_length(atomic_int_array(vm, receiver(args)))? as i32)
    }),
    method("get", "(I)I", ACC_PUBLIC, |vm, _, args| {
        int(vm
            .array_get(
                atomic_int_array(vm, receiver(args)),
                crate::int_arg(args, 1),
            )?
            .as_int())
    }),
    method("set", "(II)V", ACC_PUBLIC, |vm, _, args| {
        vm.array_set(
            atomic_int_array(vm, receiver(args)),
            crate::int_arg(args, 1),
            Value::Int(crate::int_arg(args, 2)),
            false,
        )?;
        void()
    }),
    method("lazySet", "(II)V", ACC_PUBLIC, |vm, _, args| {
        vm.array_set(
            atomic_int_array(vm, receiver(args)),
            crate::int_arg(args, 1),
            Value::Int(crate::int_arg(args, 2)),
            false,
        )?;
        void()
    }),
    method("getAndSet", "(II)I", ACC_PUBLIC, |vm, _, args| {
        let array = atomic_int_array(vm, receiver(args));
        let index = crate::int_arg(args, 1);
        let old = vm.array_get(array, index)?.as_int();
        vm.array_set(array, index, Value::Int(crate::int_arg(args, 2)), false)?;
        int(old)
    }),
    method("compareAndSet", "(III)Z", ACC_PUBLIC, |vm, _, args| {
        let array = atomic_int_array(vm, receiver(args));
        let index = crate::int_arg(args, 1);
        let expected = crate::int_arg(args, 2);
        let update = crate::int_arg(args, 3);
        if vm.array_get(array, index)?.as_int() == expected {
            vm.array_set(array, index, Value::Int(update), false)?;
            return boolean(true);
        }
        boolean(false)
    }),
    method("weakCompareAndSet", "(III)Z", ACC_PUBLIC, |vm, _, args| {
        let array = atomic_int_array(vm, receiver(args));
        let index = crate::int_arg(args, 1);
        let expected = crate::int_arg(args, 2);
        let update = crate::int_arg(args, 3);
        if vm.array_get(array, index)?.as_int() == expected {
            vm.array_set(array, index, Value::Int(update), false)?;
            return boolean(true);
        }
        boolean(false)
    }),
    method("getAndIncrement", "(I)I", ACC_PUBLIC, |vm, _, args| {
        let (old, _) = atomic_int_array_add(vm, receiver(args), crate::int_arg(args, 1), 1)?;
        int(old)
    }),
    method("getAndDecrement", "(I)I", ACC_PUBLIC, |vm, _, args| {
        let (old, _) = atomic_int_array_add(vm, receiver(args), crate::int_arg(args, 1), -1)?;
        int(old)
    }),
    method("incrementAndGet", "(I)I", ACC_PUBLIC, |vm, _, args| {
        let (_, new) = atomic_int_array_add(vm, receiver(args), crate::int_arg(args, 1), 1)?;
        int(new)
    }),
    method("decrementAndGet", "(I)I", ACC_PUBLIC, |vm, _, args| {
        let (_, new) = atomic_int_array_add(vm, receiver(args), crate::int_arg(args, 1), -1)?;
        int(new)
    }),
    method("getAndAdd", "(II)I", ACC_PUBLIC, |vm, _, args| {
        let (old, _) = atomic_int_array_add(
            vm,
            receiver(args),
            crate::int_arg(args, 1),
            crate::int_arg(args, 2),
        )?;
        int(old)
    }),
    method("addAndGet", "(II)I", ACC_PUBLIC, |vm, _, args| {
        let (_, new) = atomic_int_array_add(
            vm,
            receiver(args),
            crate::int_arg(args, 1),
            crate::int_arg(args, 2),
        )?;
        int(new)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let array = atomic_int_array(vm, receiver(args));
            let length = vm.array_length(array)?;
            let mut text = String::from("[");
            for index in 0..length {
                if index > 0 {
                    text.push_str(", ");
                }
                text.push_str(&alloc::format!(
                    "{}",
                    vm.array_get(array, index as i32)?.as_int()
                ));
            }
            text.push(']');
            object(vm.make_string(&text)?)
        },
    ),
];

/// `AtomicIntegerArray`: an int array with atomic element operations.
pub(crate) const ATOMIC_INTEGER_ARRAY: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/atomic/AtomicIntegerArray",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[field("array", "[I", ACC_PRIVATE | ACC_FINAL, None)],
    &ATOMIC_INTEGER_ARRAY_METHODS,
    None,
);

const ATOMIC_LONG_ARRAY_METHODS: [oxjvm_vm::NativeMethodDef; 16] = [
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let length = crate::int_arg(args, 1);
        if length < 0 {
            return Err(vm.throw_new("java/lang/NegativeArraySizeException", None));
        }
        let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Long, length as usize)?;
        let class = vm.class_of(this);
        let (declaring, field) = vm.find_field(class, "array", "[J")?;
        vm.set_instance_ref(this, declaring, field, array);
        void()
    }),
    method("<init>", "([J)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let source = ref_arg(args, 1);
        if source.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        let length = vm.array_length(source)?;
        let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Long, length)?;
        for index in 0..length {
            let value = vm.array_get(source, index as i32)?;
            vm.array_set(array, index as i32, value, false)?;
        }
        let class = vm.class_of(this);
        let (declaring, field) = vm.find_field(class, "array", "[J")?;
        vm.set_instance_ref(this, declaring, field, array);
        void()
    }),
    method("length", "()I", ACC_PUBLIC, |vm, _, args| {
        int(vm.array_length(atomic_long_array(vm, receiver(args)))? as i32)
    }),
    method("get", "(I)J", ACC_PUBLIC, |vm, _, args| {
        long(
            vm.array_get(
                atomic_long_array(vm, receiver(args)),
                crate::int_arg(args, 1),
            )?
            .as_long(),
        )
    }),
    method("set", "(IJ)V", ACC_PUBLIC, |vm, _, args| {
        vm.array_set(
            atomic_long_array(vm, receiver(args)),
            crate::int_arg(args, 1),
            Value::Long(crate::long_arg(args, 2)),
            false,
        )?;
        void()
    }),
    method("lazySet", "(IJ)V", ACC_PUBLIC, |vm, _, args| {
        vm.array_set(
            atomic_long_array(vm, receiver(args)),
            crate::int_arg(args, 1),
            Value::Long(crate::long_arg(args, 2)),
            false,
        )?;
        void()
    }),
    method("getAndSet", "(IJ)J", ACC_PUBLIC, |vm, _, args| {
        let array = atomic_long_array(vm, receiver(args));
        let index = crate::int_arg(args, 1);
        let old = vm.array_get(array, index)?.as_long();
        vm.array_set(array, index, Value::Long(crate::long_arg(args, 2)), false)?;
        long(old)
    }),
    method("compareAndSet", "(IJJ)Z", ACC_PUBLIC, |vm, _, args| {
        let array = atomic_long_array(vm, receiver(args));
        let index = crate::int_arg(args, 1);
        let expected = crate::long_arg(args, 2);
        let update = crate::long_arg(args, 3);
        if vm.array_get(array, index)?.as_long() == expected {
            vm.array_set(array, index, Value::Long(update), false)?;
            return boolean(true);
        }
        boolean(false)
    }),
    method("weakCompareAndSet", "(IJJ)Z", ACC_PUBLIC, |vm, _, args| {
        let array = atomic_long_array(vm, receiver(args));
        let index = crate::int_arg(args, 1);
        let expected = crate::long_arg(args, 2);
        let update = crate::long_arg(args, 3);
        if vm.array_get(array, index)?.as_long() == expected {
            vm.array_set(array, index, Value::Long(update), false)?;
            return boolean(true);
        }
        boolean(false)
    }),
    method("getAndIncrement", "(I)J", ACC_PUBLIC, |vm, _, args| {
        let (old, _) = atomic_long_array_add(vm, receiver(args), crate::int_arg(args, 1), 1)?;
        long(old)
    }),
    method("getAndDecrement", "(I)J", ACC_PUBLIC, |vm, _, args| {
        let (old, _) = atomic_long_array_add(vm, receiver(args), crate::int_arg(args, 1), -1)?;
        long(old)
    }),
    method("incrementAndGet", "(I)J", ACC_PUBLIC, |vm, _, args| {
        let (_, new) = atomic_long_array_add(vm, receiver(args), crate::int_arg(args, 1), 1)?;
        long(new)
    }),
    method("decrementAndGet", "(I)J", ACC_PUBLIC, |vm, _, args| {
        let (_, new) = atomic_long_array_add(vm, receiver(args), crate::int_arg(args, 1), -1)?;
        long(new)
    }),
    method("getAndAdd", "(IJ)J", ACC_PUBLIC, |vm, _, args| {
        let (old, _) = atomic_long_array_add(
            vm,
            receiver(args),
            crate::int_arg(args, 1),
            crate::long_arg(args, 2),
        )?;
        long(old)
    }),
    method("addAndGet", "(IJ)J", ACC_PUBLIC, |vm, _, args| {
        let (_, new) = atomic_long_array_add(
            vm,
            receiver(args),
            crate::int_arg(args, 1),
            crate::long_arg(args, 2),
        )?;
        long(new)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let array = atomic_long_array(vm, receiver(args));
            let length = vm.array_length(array)?;
            let mut text = String::from("[");
            for index in 0..length {
                if index > 0 {
                    text.push_str(", ");
                }
                text.push_str(&alloc::format!(
                    "{}",
                    vm.array_get(array, index as i32)?.as_long()
                ));
            }
            text.push(']');
            object(vm.make_string(&text)?)
        },
    ),
];

/// `AtomicLongArray`: a long array with atomic element operations.
pub(crate) const ATOMIC_LONG_ARRAY: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/atomic/AtomicLongArray",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[field("array", "[J", ACC_PRIVATE | ACC_FINAL, None)],
    &ATOMIC_LONG_ARRAY_METHODS,
    None,
);

const ATOMIC_REFERENCE_ARRAY_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let length = crate::int_arg(args, 1);
        if length < 0 {
            return Err(vm.throw_new("java/lang/NegativeArraySizeException", None));
        }
        let object_class = vm.resolve_class("java/lang/Object")?;
        let array = vm.allocate_object_array(object_class, length as usize)?;
        let class = vm.class_of(this);
        let (declaring, field) = vm.find_field(class, "array", "[Ljava/lang/Object;")?;
        vm.set_instance_ref(this, declaring, field, array);
        void()
    }),
    method(
        "<init>",
        "([Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let source = ref_arg(args, 1);
            if source.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let length = vm.array_length(source)?;
            let object_class = vm.resolve_class("java/lang/Object")?;
            let array = vm.allocate_object_array(object_class, length)?;
            for index in 0..length {
                let value = vm.array_get(source, index as i32)?.as_ref();
                vm.array_set_ref(array, index, value)?;
            }
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "array", "[Ljava/lang/Object;")?;
            vm.set_instance_ref(this, declaring, field, array);
            void()
        },
    ),
    method("length", "()I", ACC_PUBLIC, |vm, _, args| {
        int(vm.array_length(atomic_ref_array(vm, receiver(args)))? as i32)
    }),
    method("get", "(I)Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        object(
            vm.array_get(
                atomic_ref_array(vm, receiver(args)),
                crate::int_arg(args, 1),
            )?
            .as_ref(),
        )
    }),
    method(
        "set",
        "(ILjava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            vm.array_set_ref(
                atomic_ref_array(vm, receiver(args)),
                crate::int_arg(args, 1) as usize,
                ref_arg(args, 2),
            )?;
            void()
        },
    ),
    method(
        "lazySet",
        "(ILjava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            vm.array_set_ref(
                atomic_ref_array(vm, receiver(args)),
                crate::int_arg(args, 1) as usize,
                ref_arg(args, 2),
            )?;
            void()
        },
    ),
    method(
        "getAndSet",
        "(ILjava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let array = atomic_ref_array(vm, receiver(args));
            let index = crate::int_arg(args, 1);
            let old = vm.array_get(array, index)?.as_ref();
            vm.array_set_ref(array, index as usize, ref_arg(args, 2))?;
            object(old)
        },
    ),
    method(
        "compareAndSet",
        "(ILjava/lang/Object;Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let array = atomic_ref_array(vm, receiver(args));
            let index = crate::int_arg(args, 1);
            let expected = ref_arg(args, 2);
            let update = ref_arg(args, 3);
            if vm.array_get(array, index)?.as_ref() == expected {
                vm.array_set_ref(array, index as usize, update)?;
                return boolean(true);
            }
            boolean(false)
        },
    ),
    method(
        "weakCompareAndSet",
        "(ILjava/lang/Object;Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let array = atomic_ref_array(vm, receiver(args));
            let index = crate::int_arg(args, 1);
            let expected = ref_arg(args, 2);
            let update = ref_arg(args, 3);
            if vm.array_get(array, index)?.as_ref() == expected {
                vm.array_set_ref(array, index as usize, update)?;
                return boolean(true);
            }
            boolean(false)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let array = atomic_ref_array(vm, receiver(args));
            let length = vm.array_length(array)?;
            let mut text = String::from("[");
            for index in 0..length {
                if index > 0 {
                    text.push_str(", ");
                }
                let element = vm.array_get(array, index as i32)?.as_ref();
                if element.is_null() {
                    text.push_str("null");
                } else {
                    let (class, method) =
                        vm.resolve_virtual_method(element, "toString", "()Ljava/lang/String;")?;
                    let rendered = vm.invoke_method(class, method, vec![Value::Ref(element)])?;
                    text.push_str(&vm.string_or_empty(rendered.as_ref()));
                }
            }
            text.push(']');
            object(vm.make_string(&text)?)
        },
    ),
];

/// `AtomicReferenceArray`: an object array with atomic element operations.
pub(crate) const ATOMIC_REFERENCE_ARRAY: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/atomic/AtomicReferenceArray",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[field(
        "array",
        "[Ljava/lang/Object;",
        ACC_PRIVATE | ACC_FINAL,
        None,
    )],
    &ATOMIC_REFERENCE_ARRAY_METHODS,
    None,
);

fn field_updater_new(
    vm: &mut Vm<'_>,
    class_name: &str,
    name: ObjectRef,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class(class_name)?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "fieldName", "Ljava/lang/String;")?;
    vm.set_instance_ref(instance, declaring, field, name);
    Ok(instance)
}

fn updater_field_name(vm: &Vm<'_>, this: ObjectRef) -> String {
    vm.read_string_field_any(this, "fieldName")
        .unwrap_or_default()
}

const ATOMIC_INTEGER_FIELD_UPDATER_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method(
        "newUpdater",
        "(Ljava/lang/Class;Ljava/lang/String;)Ljava/util/concurrent/atomic/AtomicIntegerFieldUpdater;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            object(field_updater_new(
                vm,
                "java/util/concurrent/atomic/AtomicIntegerFieldUpdater",
                ref_arg(args, 1),
            )?)
        },
    ),
    method("get", "(Ljava/lang/Object;)I", ACC_PUBLIC, |vm, _, args| {
        let name = updater_field_name(vm, receiver(args));
        int(crate::lang::read_int_field(vm, ref_arg(args, 1), &name))
    }),
    method(
        "set",
        "(Ljava/lang/Object;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            crate::lang::write_int_field(vm, ref_arg(args, 1), &name, crate::int_arg(args, 2));
            void()
        },
    ),
    method(
        "lazySet",
        "(Ljava/lang/Object;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            crate::lang::write_int_field(vm, ref_arg(args, 1), &name, crate::int_arg(args, 2));
            void()
        },
    ),
    method(
        "getAndSet",
        "(Ljava/lang/Object;I)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            let target = ref_arg(args, 1);
            let old = crate::lang::read_int_field(vm, target, &name);
            crate::lang::write_int_field(vm, target, &name, crate::int_arg(args, 2));
            int(old)
        },
    ),
    method(
        "incrementAndGet",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            let target = ref_arg(args, 1);
            let new = crate::lang::read_int_field(vm, target, &name).wrapping_add(1);
            crate::lang::write_int_field(vm, target, &name, new);
            int(new)
        },
    ),
    method(
        "getAndIncrement",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            let target = ref_arg(args, 1);
            let old = crate::lang::read_int_field(vm, target, &name);
            crate::lang::write_int_field(vm, target, &name, old.wrapping_add(1));
            int(old)
        },
    ),
    method(
        "decrementAndGet",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            let target = ref_arg(args, 1);
            let new = crate::lang::read_int_field(vm, target, &name).wrapping_sub(1);
            crate::lang::write_int_field(vm, target, &name, new);
            int(new)
        },
    ),
    method(
        "getAndDecrement",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            let target = ref_arg(args, 1);
            let old = crate::lang::read_int_field(vm, target, &name);
            crate::lang::write_int_field(vm, target, &name, old.wrapping_sub(1));
            int(old)
        },
    ),
    method(
        "addAndGet",
        "(Ljava/lang/Object;I)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            let target = ref_arg(args, 1);
            let new = crate::lang::read_int_field(vm, target, &name)
                .wrapping_add(crate::int_arg(args, 2));
            crate::lang::write_int_field(vm, target, &name, new);
            int(new)
        },
    ),
    method(
        "getAndAdd",
        "(Ljava/lang/Object;I)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            let target = ref_arg(args, 1);
            let old = crate::lang::read_int_field(vm, target, &name);
            crate::lang::write_int_field(
                vm,
                target,
                &name,
                old.wrapping_add(crate::int_arg(args, 2)),
            );
            int(old)
        },
    ),
    method(
        "compareAndSet",
        "(Ljava/lang/Object;II)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            let target = ref_arg(args, 1);
            let expect = crate::int_arg(args, 2);
            let update = crate::int_arg(args, 3);
            if crate::lang::read_int_field(vm, target, &name) == expect {
                crate::lang::write_int_field(vm, target, &name, update);
                boolean(true)
            } else {
                boolean(false)
            }
        },
    ),
];

/// `AtomicIntegerFieldUpdater`: reflective atomic updates of a named volatile int field.
pub(crate) const ATOMIC_INTEGER_FIELD_UPDATER: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/atomic/AtomicIntegerFieldUpdater",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[field("fieldName", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &ATOMIC_INTEGER_FIELD_UPDATER_METHODS,
    None,
);

const ATOMIC_LONG_FIELD_UPDATER_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method(
        "newUpdater",
        "(Ljava/lang/Class;Ljava/lang/String;)Ljava/util/concurrent/atomic/AtomicLongFieldUpdater;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            object(field_updater_new(
                vm,
                "java/util/concurrent/atomic/AtomicLongFieldUpdater",
                ref_arg(args, 1),
            )?)
        },
    ),
    method("get", "(Ljava/lang/Object;)J", ACC_PUBLIC, |vm, _, args| {
        let name = updater_field_name(vm, receiver(args));
        long(crate::lang::read_long_field(vm, ref_arg(args, 1), &name))
    }),
    method(
        "set",
        "(Ljava/lang/Object;J)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            crate::lang::write_long_field(vm, ref_arg(args, 1), &name, crate::long_arg(args, 2));
            void()
        },
    ),
    method(
        "lazySet",
        "(Ljava/lang/Object;J)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            crate::lang::write_long_field(vm, ref_arg(args, 1), &name, crate::long_arg(args, 2));
            void()
        },
    ),
    method(
        "getAndSet",
        "(Ljava/lang/Object;J)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            let target = ref_arg(args, 1);
            let old = crate::lang::read_long_field(vm, target, &name);
            crate::lang::write_long_field(vm, target, &name, crate::long_arg(args, 2));
            long(old)
        },
    ),
    method(
        "incrementAndGet",
        "(Ljava/lang/Object;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            let target = ref_arg(args, 1);
            let new = crate::lang::read_long_field(vm, target, &name).wrapping_add(1);
            crate::lang::write_long_field(vm, target, &name, new);
            long(new)
        },
    ),
    method(
        "getAndIncrement",
        "(Ljava/lang/Object;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            let target = ref_arg(args, 1);
            let old = crate::lang::read_long_field(vm, target, &name);
            crate::lang::write_long_field(vm, target, &name, old.wrapping_add(1));
            long(old)
        },
    ),
    method(
        "decrementAndGet",
        "(Ljava/lang/Object;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            let target = ref_arg(args, 1);
            let new = crate::lang::read_long_field(vm, target, &name).wrapping_sub(1);
            crate::lang::write_long_field(vm, target, &name, new);
            long(new)
        },
    ),
    method(
        "getAndDecrement",
        "(Ljava/lang/Object;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            let target = ref_arg(args, 1);
            let old = crate::lang::read_long_field(vm, target, &name);
            crate::lang::write_long_field(vm, target, &name, old.wrapping_sub(1));
            long(old)
        },
    ),
    method(
        "addAndGet",
        "(Ljava/lang/Object;J)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            let target = ref_arg(args, 1);
            let new = crate::lang::read_long_field(vm, target, &name)
                .wrapping_add(crate::long_arg(args, 2));
            crate::lang::write_long_field(vm, target, &name, new);
            long(new)
        },
    ),
    method(
        "getAndAdd",
        "(Ljava/lang/Object;J)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            let target = ref_arg(args, 1);
            let old = crate::lang::read_long_field(vm, target, &name);
            crate::lang::write_long_field(
                vm,
                target,
                &name,
                old.wrapping_add(crate::long_arg(args, 2)),
            );
            long(old)
        },
    ),
    method(
        "compareAndSet",
        "(Ljava/lang/Object;JJ)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            let target = ref_arg(args, 1);
            let expect = crate::long_arg(args, 2);
            let update = crate::long_arg(args, 3);
            if crate::lang::read_long_field(vm, target, &name) == expect {
                crate::lang::write_long_field(vm, target, &name, update);
                boolean(true)
            } else {
                boolean(false)
            }
        },
    ),
];

/// `AtomicLongFieldUpdater`: reflective atomic updates of a named volatile long field.
pub(crate) const ATOMIC_LONG_FIELD_UPDATER: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/atomic/AtomicLongFieldUpdater",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[field("fieldName", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &ATOMIC_LONG_FIELD_UPDATER_METHODS,
    None,
);

const ATOMIC_REFERENCE_FIELD_UPDATER_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "newUpdater",
        "(Ljava/lang/Class;Ljava/lang/Class;Ljava/lang/String;)Ljava/util/concurrent/atomic/AtomicReferenceFieldUpdater;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            object(field_updater_new(
                vm,
                "java/util/concurrent/atomic/AtomicReferenceFieldUpdater",
                ref_arg(args, 2),
            )?)
        },
    ),
    method(
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            object(
                vm.read_ref_field_any(ref_arg(args, 1), &name)
                    .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method(
        "set",
        "(Ljava/lang/Object;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            vm.write_ref_field_any(ref_arg(args, 1), &name, ref_arg(args, 2));
            void()
        },
    ),
    method(
        "lazySet",
        "(Ljava/lang/Object;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            vm.write_ref_field_any(ref_arg(args, 1), &name, ref_arg(args, 2));
            void()
        },
    ),
    method(
        "getAndSet",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            let target = ref_arg(args, 1);
            let old = vm
                .read_ref_field_any(target, &name)
                .unwrap_or(ObjectRef::NULL);
            vm.write_ref_field_any(target, &name, ref_arg(args, 2));
            object(old)
        },
    ),
    method(
        "compareAndSet",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            let target = ref_arg(args, 1);
            let expect = ref_arg(args, 2);
            let update = ref_arg(args, 3);
            let current = vm
                .read_ref_field_any(target, &name)
                .unwrap_or(ObjectRef::NULL);
            if current == expect {
                vm.write_ref_field_any(target, &name, update);
                boolean(true)
            } else {
                boolean(false)
            }
        },
    ),
    method(
        "weakCompareAndSet",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = updater_field_name(vm, receiver(args));
            let target = ref_arg(args, 1);
            let expect = ref_arg(args, 2);
            let update = ref_arg(args, 3);
            let current = vm
                .read_ref_field_any(target, &name)
                .unwrap_or(ObjectRef::NULL);
            if current == expect {
                vm.write_ref_field_any(target, &name, update);
                boolean(true)
            } else {
                boolean(false)
            }
        },
    ),
];

/// `AtomicReferenceFieldUpdater`: reflective atomic updates of a named volatile reference field.
pub(crate) const ATOMIC_REFERENCE_FIELD_UPDATER: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/atomic/AtomicReferenceFieldUpdater",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[field("fieldName", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &ATOMIC_REFERENCE_FIELD_UPDATER_METHODS,
    None,
);
