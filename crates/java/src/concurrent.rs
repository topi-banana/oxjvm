//! `java.util.concurrent` primitives: `TimeUnit` and `CountDownLatch`.
//!
//! The interpreter runs green threads cooperatively, so a waiting native cannot block: it parks
//! the current thread and returns, and execution continues after the call once another thread
//! wakes it.  `CountDownLatch` uses this via [`Vm::park_until_notified`], notifying every waiter
//! exactly when the count reaches zero.

use alloc::vec;
use oxjvm_classfile::flags::*;

use crate::{boolean, class, field, int, long, method, object, receiver, ref_arg, void};
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

// -------------------------------------------------------------------------------------------
// TimeUnit
// -------------------------------------------------------------------------------------------

/// Nanoseconds per unit, indexed like the `TimeUnit` constants.
const UNIT_NANOS: [i64; 7] = [
    1,
    1_000,
    1_000_000,
    1_000_000_000,
    60 * 1_000_000_000,
    3_600 * 1_000_000_000,
    86_400 * 1_000_000_000,
];

const UNIT_NAMES: [&str; 7] = [
    "NANOSECONDS",
    "MICROSECONDS",
    "MILLISECONDS",
    "SECONDS",
    "MINUTES",
    "HOURS",
    "DAYS",
];

fn unit_index(vm: &Vm<'_>, unit: ObjectRef) -> i64 {
    i64::from(crate::lang::read_int_field(vm, unit, "index"))
}

fn unit_convert(value: i64, from: i64, to: i64) -> i64 {
    let from = from.clamp(0, 6) as usize;
    let to = to.clamp(0, 6) as usize;
    let nanos = i128::from(value) * i128::from(UNIT_NANOS[from]);
    let result = nanos / i128::from(UNIT_NANOS[to]);
    i64::try_from(result).unwrap_or(if result < 0 { i64::MIN } else { i64::MAX })
}

fn unit_to(vm: &mut Vm<'_>, args: &[Value], to: usize) -> Result<Value, VmError> {
    let this = receiver(args);
    let index = unit_index(vm, this);
    long(unit_convert(crate::long_arg(args, 1), index, to as i64))
}

fn time_unit_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/util/concurrent/TimeUnit")?;
    for (index, name) in UNIT_NAMES.iter().enumerate() {
        let instance = vm.new_instance(class)?;
        if let Some((declaring, field)) = vm.find_field_opt(class, "index", "I") {
            vm.set_instance_int(instance, declaring, field, index as i32);
        }
        vm.set_static_value(
            class,
            name,
            "Ljava/util/concurrent/TimeUnit;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

const TIME_UNIT_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method("toNanos", "(J)J", ACC_PUBLIC, |vm, _, args| {
        unit_to(vm, args, 0)
    }),
    method("toMicros", "(J)J", ACC_PUBLIC, |vm, _, args| {
        unit_to(vm, args, 1)
    }),
    method("toMillis", "(J)J", ACC_PUBLIC, |vm, _, args| {
        unit_to(vm, args, 2)
    }),
    method("toSeconds", "(J)J", ACC_PUBLIC, |vm, _, args| {
        unit_to(vm, args, 3)
    }),
    method(
        "convert",
        "(JLjava/util/concurrent/TimeUnit;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            // `convert(duration, sourceUnit)`: from the argument's unit to this unit.
            let from = unit_index(vm, ref_arg(args, 2));
            let to = unit_index(vm, receiver(args));
            long(unit_convert(crate::long_arg(args, 1), from, to))
        },
    ),
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let index = unit_index(vm, receiver(args)).clamp(0, 6) as usize;
        object(vm.make_string(UNIT_NAMES[index])?)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let index = unit_index(vm, receiver(args)).clamp(0, 6) as usize;
            object(vm.make_string(UNIT_NAMES[index])?)
        },
    ),
    method("sleep", "(J)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let index = unit_index(vm, this);
        vm.sleep_current(unit_convert(crate::long_arg(args, 1), index, 2));
        void()
    }),
    method(
        "values",
        "()[Ljava/util/concurrent/TimeUnit;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/concurrent/TimeUnit")?;
            let array = vm.allocate_object_array(class, UNIT_NAMES.len())?;
            for (index, name) in UNIT_NAMES.iter().enumerate() {
                if let Some(unit) =
                    vm.read_static_ref_if_set(class, name, "Ljava/util/concurrent/TimeUnit;")
                {
                    vm.array_set_ref(array, index, unit)?;
                }
            }
            object(array)
        },
    ),
    method(
        "valueOf",
        "(Ljava/lang/String;)Ljava/util/concurrent/TimeUnit;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = vm.string_or_empty(ref_arg(args, 0));
            let class = vm.resolve_class("java/util/concurrent/TimeUnit")?;
            for candidate in UNIT_NAMES {
                if candidate == name {
                    if let Some(unit) = vm.read_static_ref_if_set(
                        class,
                        candidate,
                        "Ljava/util/concurrent/TimeUnit;",
                    ) {
                        return object(unit);
                    }
                }
            }
            Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some(&alloc::format!(
                    "No enum constant java.util.concurrent.TimeUnit.{name}"
                )),
            ))
        },
    ),
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if let Some((declaring, field)) = vm.find_field_opt(vm.class_of(this), "index", "I") {
            let value = crate::int_arg(args, 1);
            vm.set_instance_int(this, declaring, field, value);
        }
        void()
    }),
    method("ordinal", "()I", ACC_PUBLIC, |vm, _, args| {
        int(unit_index(vm, receiver(args)) as i32)
    }),
];

pub(crate) const TIME_UNIT: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/TimeUnit",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("index", "I", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "NANOSECONDS",
            "Ljava/util/concurrent/TimeUnit;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MICROSECONDS",
            "Ljava/util/concurrent/TimeUnit;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MILLISECONDS",
            "Ljava/util/concurrent/TimeUnit;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "SECONDS",
            "Ljava/util/concurrent/TimeUnit;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MINUTES",
            "Ljava/util/concurrent/TimeUnit;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "HOURS",
            "Ljava/util/concurrent/TimeUnit;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "DAYS",
            "Ljava/util/concurrent/TimeUnit;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &TIME_UNIT_METHODS,
    Some(time_unit_clinit),
);

/// Read a static `TimeUnit` constant (used by `CountDownLatch.await`).
fn time_unit_constant(vm: &mut Vm<'_>, name: &str) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/concurrent/TimeUnit")?;
    vm.read_static_ref_if_set(class, name, "Ljava/util/concurrent/TimeUnit;")
        .ok_or_else(|| VmError::internal("TimeUnit constants not initialized"))
}

/// Convert `value` in `unit` to milliseconds through `TimeUnit.convert`.
fn time_unit_to_millis(vm: &mut Vm<'_>, unit: ObjectRef, value: i64) -> Result<i64, VmError> {
    let milliseconds = time_unit_constant(vm, "MILLISECONDS")?;
    let (class, method) =
        vm.resolve_virtual_method(unit, "convert", "(JLjava/util/concurrent/TimeUnit;)J")?;
    let result = vm.invoke_method(
        class,
        method,
        vec![
            Value::Ref(unit),
            Value::Long(value),
            Value::Ref(milliseconds),
        ],
    )?;
    Ok(result.as_long())
}

// -------------------------------------------------------------------------------------------
// CountDownLatch
// -------------------------------------------------------------------------------------------

const COUNT_DOWN_LATCH_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let count = crate::int_arg(args, 1);
        if count < 0 {
            return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("count < 0")));
        }
        crate::lang::write_int_field(vm, receiver(args), "count", count);
        void()
    }),
    method("await", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if crate::lang::read_int_field(vm, this, "count") > 0 {
            vm.park_until_notified(this, None);
        }
        void()
    }),
    method(
        "await",
        "(JLjava/util/concurrent/TimeUnit;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let count = crate::lang::read_int_field(vm, this, "count");
            if count <= 0 {
                return boolean(true);
            }
            let millis = time_unit_to_millis(vm, ref_arg(args, 2), crate::long_arg(args, 1))?;
            vm.park_until_notified(this, Some(millis));
            // The thread continues here once notified (or once the deadline passed; the
            // cooperative scheduler does not distinguish the two for native waiters).
            boolean(true)
        },
    ),
    method("countDown", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let count = crate::lang::read_int_field(vm, this, "count");
        if count > 0 {
            crate::lang::write_int_field(vm, this, "count", count - 1);
            if count == 1 {
                vm.monitor_notify(this, true);
            }
        }
        void()
    }),
    method("getCount", "()J", ACC_PUBLIC, |vm, _, args| {
        long(i64::from(crate::lang::read_int_field(
            vm,
            receiver(args),
            "count",
        )))
    }),
];

pub(crate) const COUNT_DOWN_LATCH: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/CountDownLatch",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[field("count", "I", ACC_PRIVATE | ACC_VOLATILE, None)],
    &COUNT_DOWN_LATCH_METHODS,
    None,
);
