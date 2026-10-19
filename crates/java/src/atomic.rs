//! `java.util.concurrent.atomic`: `AtomicInteger`, `AtomicLong` and `AtomicBoolean`.
//!
//! These are plain objects with a single `value` field; the guest's own concurrency guarantees are
//! met because the interpreter executes one instruction at a time.

use oxjvm_classfile::flags::*;

use crate::boxed::{read_value_int, read_value_long, write_value_int, write_value_long};
use crate::{boolean, class, field, int, long, method, object, receiver, void};

// -------------------------------------------------------------------------------------------
// AtomicInteger
// -------------------------------------------------------------------------------------------

const ATOMIC_INTEGER_METHODS: [oxjvm_vm::NativeMethodDef; 18] = [
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
        crate::float(read_value_int(vm, receiver(args)) as f32)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = oxjvm_vm::format::int_to_string(read_value_int(vm, receiver(args)));
            object(vm.make_string(&text)?)
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

const ATOMIC_LONG_METHODS: [oxjvm_vm::NativeMethodDef; 17] = [
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
        crate::float(read_value_long(vm, receiver(args)) as f32)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = oxjvm_vm::format::long_to_string(read_value_long(vm, receiver(args)));
            object(vm.make_string(&text)?)
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

fn atomic_reference_value(vm: &oxjvm_vm::Vm<'_>, this: oxjvm_vm::ObjectRef) -> oxjvm_vm::ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "value", "Ljava/lang/Object;")
        .unwrap_or(oxjvm_vm::ObjectRef::NULL)
}

fn atomic_reference_set(
    vm: &mut oxjvm_vm::Vm<'_>,
    this: oxjvm_vm::ObjectRef,
    value: oxjvm_vm::ObjectRef,
) {
    if let Ok((declaring, field)) = vm.find_field(vm.class_of(this), "value", "Ljava/lang/Object;")
    {
        vm.set_instance_ref(this, declaring, field, value);
    }
}

const ATOMIC_REFERENCE_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        atomic_reference_set(vm, receiver(args), oxjvm_vm::ObjectRef::NULL);
        void()
    }),
    method(
        "<init>",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            atomic_reference_set(vm, receiver(args), crate::ref_arg(args, 1));
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
            atomic_reference_set(vm, receiver(args), crate::ref_arg(args, 1));
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
            atomic_reference_set(vm, this, crate::ref_arg(args, 1));
            object(previous)
        },
    ),
    method(
        "compareAndSet",
        "(Ljava/lang/Object;Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let expected = crate::ref_arg(args, 1);
            let current = atomic_reference_value(vm, this);
            if current == expected {
                atomic_reference_set(vm, this, crate::ref_arg(args, 2));
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
            let expected = crate::ref_arg(args, 1);
            let current = atomic_reference_value(vm, this);
            if current == expected {
                atomic_reference_set(vm, this, crate::ref_arg(args, 2));
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
            atomic_reference_set(vm, receiver(args), crate::ref_arg(args, 1));
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
                oxjvm_vm::Value::Ref(value),
            )?;
            object(vm.make_string(&text)?)
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
