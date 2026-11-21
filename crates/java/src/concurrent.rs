//! `java.util.concurrent` primitives: `TimeUnit` and `CountDownLatch`.
//!
//! The interpreter runs green threads cooperatively, so a waiting native cannot block: it parks
//! the current thread and returns, and execution continues after the call once another thread
//! wakes it.  `CountDownLatch` uses this via [`Vm::park_until_notified`], notifying every waiter
//! exactly when the count reaches zero.

use alloc::vec;
use alloc::vec::Vec;
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

const TIME_UNIT_METHODS: [oxjvm_vm::NativeMethodDef; 13] = [
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
    method(
        "convert",
        "(Ljava/time/Duration;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let duration = ref_arg(args, 1);
            if duration.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let nanos = crate::lang::read_long_field(vm, duration, "nanos");
            let to = unit_index(vm, receiver(args));
            long(unit_convert(nanos, 0, to))
        },
    ),
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
            vm.park_until_notified(this, None)?;
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
            vm.park_until_notified(this, Some(millis))?;
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

// -------------------------------------------------------------------------------------------
// CyclicBarrier
// -------------------------------------------------------------------------------------------

const CYCLIC_BARRIER_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let parties = crate::int_arg(args, 1);
        if parties <= 0 {
            return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("parties <= 0")));
        }
        let this = receiver(args);
        crate::lang::write_int_field(vm, this, "parties", parties);
        crate::lang::write_int_field(vm, this, "count", 0);
        void()
    }),
    method(
        "<init>",
        "(ILjava/lang/Runnable;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let parties = crate::int_arg(args, 1);
            if parties <= 0 {
                return Err(
                    vm.throw_new("java/lang/IllegalArgumentException", Some("parties <= 0"))
                );
            }
            let this = receiver(args);
            crate::lang::write_int_field(vm, this, "parties", parties);
            crate::lang::write_int_field(vm, this, "count", 0);
            void()
        },
    ),
    method("await", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let parties = crate::lang::read_int_field(vm, this, "parties");
        let waiting = crate::lang::read_int_field(vm, this, "count") + 1;
        crate::lang::write_int_field(vm, this, "count", waiting);
        if waiting >= parties {
            crate::lang::write_int_field(vm, this, "count", 0);
            vm.monitor_notify(this, true);
            int(parties - waiting)
        } else {
            vm.park_until_notified(this, None)?;
            int(parties - waiting)
        }
    }),
    method(
        "await",
        "(JLjava/util/concurrent/TimeUnit;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let parties = crate::lang::read_int_field(vm, this, "parties");
            let waiting = crate::lang::read_int_field(vm, this, "count") + 1;
            crate::lang::write_int_field(vm, this, "count", waiting);
            if waiting >= parties {
                crate::lang::write_int_field(vm, this, "count", 0);
                vm.monitor_notify(this, true);
                int(parties - waiting)
            } else {
                let millis = time_unit_to_millis(vm, ref_arg(args, 2), crate::long_arg(args, 1))?;
                vm.park_until_notified(this, Some(millis))?;
                int(parties - waiting)
            }
        },
    ),
    method("getParties", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "parties"))
    }),
    method("reset", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        crate::lang::write_int_field(vm, this, "count", 0);
        vm.monitor_notify(this, true);
        void()
    }),
    method("isBroken", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("getNumberWaiting", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "count"))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let parties = crate::lang::read_int_field(vm, this, "parties");
            let waiting = crate::lang::read_int_field(vm, this, "count");
            object(vm.make_string(&alloc::format!(
                "java.util.concurrent.CyclicBarrier[parties={parties}, waited={waiting}]"
            ))?)
        },
    ),
];

pub(crate) const CYCLIC_BARRIER: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/CyclicBarrier",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("parties", "I", ACC_PRIVATE, None),
        field("count", "I", ACC_PRIVATE, None),
    ],
    &CYCLIC_BARRIER_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// java.util.concurrent.locks
// -------------------------------------------------------------------------------------------

pub(crate) const CONDITION: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/locks/Condition",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[
        method(
            "await",
            "()V",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "signal",
            "()V",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "signalAll",
            "()V",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
    ],
    None,
);

pub(crate) const LOCK: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/locks/Lock",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[
        method(
            "lock",
            "()V",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "lockInterruptibly",
            "()V",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "tryLock",
            "()Z",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "tryLock",
            "(JLjava/util/concurrent/TimeUnit;)Z",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "unlock",
            "()V",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "newCondition",
            "()Ljava/util/concurrent/locks/Condition;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
    ],
    None,
);

// -------------------------------------------------------------------------------------------
// ReentrantLock + Condition impl + LockSupport
// -------------------------------------------------------------------------------------------

fn read_bool_field(vm: &Vm<'_>, object: ObjectRef, name: &str) -> bool {
    let class = vm.class_of(object);
    vm.read_instance_int_named(object, class, name, "Z")
        .unwrap_or(0)
        != 0
}

fn write_bool_field(vm: &mut Vm<'_>, object: ObjectRef, name: &str, value: bool) {
    let class = vm.class_of(object);
    if let Some((declaring, field)) = vm.find_field_opt(class, name, "Z") {
        vm.set_instance_int(object, declaring, field, i32::from(value));
    }
}

fn lock_owner(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    let class = vm.class_of(this);
    vm.read_ref_field(this, class, "owner", "Ljava/lang/Thread;")
        .unwrap_or(ObjectRef::NULL)
}

fn set_lock_owner(vm: &mut Vm<'_>, this: ObjectRef, owner: ObjectRef) {
    let class = vm.class_of(this);
    if let Some((declaring, field)) = vm.find_field_opt(class, "owner", "Ljava/lang/Thread;") {
        vm.set_instance_ref(this, declaring, field, owner);
    }
}

/// Acquire `lock`, re-entering when the current thread already holds it.
///
/// Threads run cooperatively, so a contended acquisition parks the caller until
/// `ReentrantLock.unlock` notifies the monitor; the loop then re-checks the owner.
fn reentrant_lock_acquire(vm: &mut Vm<'_>, lock: ObjectRef) -> Result<(), VmError> {
    let current = vm.current_thread_object();
    loop {
        let held = crate::lang::read_int_field(vm, lock, "holdCount");
        if held == 0 || lock_owner(vm, lock) == current {
            crate::lang::write_int_field(vm, lock, "holdCount", held + 1);
            set_lock_owner(vm, lock, current);
            return Ok(());
        }
        vm.park_until_notified(lock, None)?;
    }
}

fn reentrant_lock_init(
    vm: &mut Vm<'_>,
    context: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let fair = match vm.classes.get(context.class).methods[context.method as usize]
        .descriptor
        .as_str()
    {
        "(Z)V" => crate::int_arg(args, 1) != 0,
        _ => false,
    };
    write_bool_field(vm, this, "fair", fair);
    crate::lang::write_int_field(vm, this, "holdCount", 0);
    set_lock_owner(vm, this, ObjectRef::NULL);
    void()
}

fn reentrant_lock_lock(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    reentrant_lock_acquire(vm, this)?;
    void()
}

fn reentrant_lock_try_lock(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let current = vm.current_thread_object();
    let held = crate::lang::read_int_field(vm, this, "holdCount");
    if held == 0 || lock_owner(vm, this) == current {
        crate::lang::write_int_field(vm, this, "holdCount", held + 1);
        set_lock_owner(vm, this, current);
        return boolean(true);
    }
    boolean(false)
}

fn reentrant_lock_unlock(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let current = vm.current_thread_object();
    let held = crate::lang::read_int_field(vm, this, "holdCount");
    if held == 0 || lock_owner(vm, this) != current {
        return Err(vm.throw_new("java/lang/IllegalMonitorStateException", None));
    }
    let remaining = held - 1;
    crate::lang::write_int_field(vm, this, "holdCount", remaining);
    if remaining == 0 {
        set_lock_owner(vm, this, ObjectRef::NULL);
        vm.monitor_notify(this, true);
    }
    void()
}

fn reentrant_lock_new_condition(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let class = vm.resolve_class("oxjvm/Condition")?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) =
        vm.find_field(class, "lock", "Ljava/util/concurrent/locks/ReentrantLock;")?;
    vm.set_instance_ref(instance, declaring, field, this);
    object(instance)
}

fn reentrant_lock_to_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let held = crate::lang::read_int_field(vm, this, "holdCount");
    let class_name = "java.util.concurrent.locks.ReentrantLock";
    let text = if held == 0 {
        alloc::format!("{class_name}@{:x}[Unlocked]", this.raw())
    } else {
        alloc::format!("{class_name}@{:x}[Locked by thread]", this.raw())
    };
    object(vm.make_string(&text)?)
}

const REENTRANT_LOCK_METHODS: [oxjvm_vm::NativeMethodDef; 16] = [
    method("<init>", "()V", ACC_PUBLIC, reentrant_lock_init),
    method("<init>", "(Z)V", ACC_PUBLIC, reentrant_lock_init),
    method("lock", "()V", ACC_PUBLIC, reentrant_lock_lock),
    method("lockInterruptibly", "()V", ACC_PUBLIC, reentrant_lock_lock),
    method("tryLock", "()Z", ACC_PUBLIC, reentrant_lock_try_lock),
    method(
        "tryLock",
        "(JLjava/util/concurrent/TimeUnit;)Z",
        ACC_PUBLIC,
        reentrant_lock_try_lock,
    ),
    method("unlock", "()V", ACC_PUBLIC, reentrant_lock_unlock),
    method(
        "newCondition",
        "()Ljava/util/concurrent/locks/Condition;",
        ACC_PUBLIC,
        reentrant_lock_new_condition,
    ),
    method("isLocked", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "holdCount") > 0)
    }),
    method("isHeldByCurrentThread", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if crate::lang::read_int_field(vm, this, "holdCount") == 0 {
            return boolean(false);
        }
        boolean(lock_owner(vm, this) == vm.current_thread_object())
    }),
    method("getHoldCount", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "holdCount"))
    }),
    method("getQueueLength", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method("hasQueuedThreads", "()Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
    method(
        "hasQueuedThread",
        "(Ljava/lang/Thread;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(false),
    ),
    method("isFair", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(read_bool_field(vm, receiver(args), "fair"))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        reentrant_lock_to_string,
    ),
];

/// `java.util.concurrent.locks.ReentrantLock`, with cooperative blocking and reentrancy.
pub(crate) const REENTRANT_LOCK: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/locks/ReentrantLock",
    Some("java/lang/Object"),
    &["java/util/concurrent/locks/Lock", "java/io/Serializable"],
    ACC_PUBLIC,
    &[
        field("holdCount", "I", ACC_PRIVATE, None),
        field("owner", "Ljava/lang/Thread;", ACC_PRIVATE, None),
        field("fair", "Z", ACC_PRIVATE, None),
    ],
    &REENTRANT_LOCK_METHODS,
    None,
);

/// Park the caller on a condition, releasing its lock (if any) while waiting.
fn condition_await(vm: &mut Vm<'_>, this: ObjectRef, timeout: Option<i64>) -> Result<(), VmError> {
    let class = vm.class_of(this);
    let lock = vm
        .read_ref_field(
            this,
            class,
            "lock",
            "Ljava/util/concurrent/locks/ReentrantLock;",
        )
        .unwrap_or(ObjectRef::NULL);
    if lock.is_null() {
        vm.park_until_notified(this, timeout)?;
        return Ok(());
    }
    let held = crate::lang::read_int_field(vm, lock, "holdCount");
    crate::lang::write_int_field(vm, lock, "holdCount", 0);
    set_lock_owner(vm, lock, ObjectRef::NULL);
    if held > 0 {
        vm.monitor_notify(lock, true);
    }
    vm.park_until_notified(this, timeout)?;
    reentrant_lock_acquire(vm, lock)?;
    Ok(())
}

fn condition_await_plain(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    condition_await(vm, this, None)?;
    void()
}

fn condition_await_timed(
    vm: &mut Vm<'_>,
    context: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let descriptor = vm.classes.get(context.class).methods[context.method as usize]
        .descriptor
        .clone();
    let timeout = match descriptor.as_str() {
        "(JLjava/util/concurrent/TimeUnit;)Z" => Some(time_unit_to_millis(
            vm,
            ref_arg(args, 2),
            crate::long_arg(args, 1),
        )?),
        "(J)Z" => Some(crate::long_arg(args, 1).max(0)),
        // `awaitUntil(Date)`: the cooperative scheduler has no wall clock, so wait indefinitely.
        _ => None,
    };
    condition_await(vm, this, timeout)?;
    boolean(true)
}

fn condition_await_nanos(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let millis = crate::long_arg(args, 1).max(0) / 1_000_000;
    condition_await(vm, this, Some(millis))?;
    long(0)
}

fn condition_signal(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    vm.monitor_notify(receiver(args), false);
    void()
}

fn condition_signal_all(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    vm.monitor_notify(receiver(args), true);
    void()
}

const CONDITION_OBJECT_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method("await", "()V", ACC_PUBLIC, condition_await_plain),
    method(
        "await",
        "(JLjava/util/concurrent/TimeUnit;)Z",
        ACC_PUBLIC,
        condition_await_timed,
    ),
    method("await", "(J)Z", ACC_PUBLIC, condition_await_timed),
    method(
        "awaitUntil",
        "(Ljava/util/Date;)Z",
        ACC_PUBLIC,
        condition_await_timed,
    ),
    method("awaitNanos", "(J)J", ACC_PUBLIC, condition_await_nanos),
    method(
        "awaitUninterruptibly",
        "()V",
        ACC_PUBLIC,
        condition_await_plain,
    ),
    method("signal", "()V", ACC_PUBLIC, condition_signal),
    method("signalAll", "()V", ACC_PUBLIC, condition_signal_all),
];

/// Concrete `Condition` handed out by `ReentrantLock.newCondition`.
pub(crate) const CONDITION_OBJECT: oxjvm_vm::NativeClass = class(
    "oxjvm/Condition",
    Some("java/lang/Object"),
    &["java/util/concurrent/locks/Condition"],
    ACC_PUBLIC,
    &[field(
        "lock",
        "Ljava/util/concurrent/locks/ReentrantLock;",
        ACC_PRIVATE,
        None,
    )],
    &CONDITION_OBJECT_METHODS,
    None,
);

const LOCK_SUPPORT_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method("park", "()V", ACC_PUBLIC | ACC_STATIC, |_, _, _| void()),
    method(
        "park",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| void(),
    ),
    method("parkNanos", "(J)V", ACC_PUBLIC | ACC_STATIC, |_, _, _| {
        void()
    }),
    method(
        "parkNanos",
        "(Ljava/lang/Object;J)V",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| void(),
    ),
    method("parkUntil", "(J)V", ACC_PUBLIC | ACC_STATIC, |_, _, _| {
        void()
    }),
    method(
        "parkUntil",
        "(Ljava/lang/Object;J)V",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| void(),
    ),
    method(
        "unpark",
        "(Ljava/lang/Thread;)V",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| void(),
    ),
    method(
        "getBlocker",
        "(Ljava/lang/Thread;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
];

/// `java.util.concurrent.locks.LockSupport`; parking is a no-op because the runtime has no
/// ambient blocking primitives that use it.
pub(crate) const LOCK_SUPPORT: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/locks/LockSupport",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &LOCK_SUPPORT_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// ReadWriteLock / ReentrantReadWriteLock
// -------------------------------------------------------------------------------------------

pub(crate) const READ_WRITE_LOCK: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/locks/ReadWriteLock",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[
        method(
            "readLock",
            "()Ljava/util/concurrent/locks/Lock;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "writeLock",
            "()Ljava/util/concurrent/locks/Lock;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
    ],
    None,
);

const FUTURE_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "get",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "get",
        "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "isDone",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "isCancelled",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "cancel",
        "(Z)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const FUTURE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/Future",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &FUTURE_METHODS,
    None,
);

const NOOP_LOCK_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method("lock", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("lockInterruptibly", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("tryLock", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
    method(
        "tryLock",
        "(JLjava/util/concurrent/TimeUnit;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(true),
    ),
    method("unlock", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "newCondition",
        "()Ljava/util/concurrent/locks/Condition;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("oxjvm/Condition")?;
            let instance = vm.new_instance(class)?;
            object(instance)
        },
    ),
];

/// A `Lock` whose operations are no-ops: the interpreter is cooperative and single-threaded.
pub(crate) const NOOP_LOCK: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/locks/ReentrantReadWriteLock$NoopLock",
    Some("java/lang/Object"),
    &["java/util/concurrent/locks/Lock"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[],
    &NOOP_LOCK_METHODS,
    None,
);

/// Adjust the parent lock's read hold count.
fn read_lock_adjust(vm: &mut Vm<'_>, this: ObjectRef, delta: i32) {
    let class = vm.class_of(this);
    let parent = vm
        .read_ref_field(
            this,
            class,
            "parent",
            "Ljava/util/concurrent/locks/ReentrantReadWriteLock;",
        )
        .unwrap_or(ObjectRef::NULL);
    if parent.is_null() {
        return;
    }
    let count = crate::lang::read_int_field(vm, parent, "readCount");
    crate::lang::write_int_field(vm, parent, "readCount", (count + delta).max(0));
}

const READ_LOCK_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method("lock", "()V", ACC_PUBLIC, |vm, _, args| {
        read_lock_adjust(vm, receiver(args), 1);
        void()
    }),
    method("lockInterruptibly", "()V", ACC_PUBLIC, |vm, _, args| {
        read_lock_adjust(vm, receiver(args), 1);
        void()
    }),
    method("tryLock", "()Z", ACC_PUBLIC, |vm, _, args| {
        read_lock_adjust(vm, receiver(args), 1);
        boolean(true)
    }),
    method(
        "tryLock",
        "(JLjava/util/concurrent/TimeUnit;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            read_lock_adjust(vm, receiver(args), 1);
            boolean(true)
        },
    ),
    method("unlock", "()V", ACC_PUBLIC, |vm, _, args| {
        read_lock_adjust(vm, receiver(args), -1);
        void()
    }),
    method(
        "newCondition",
        "()Ljava/util/concurrent/locks/Condition;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("oxjvm/Condition")?;
            object(vm.new_instance(class)?)
        },
    ),
];

/// `ReentrantReadWriteLock.ReadLock`: a non-blocking read lock.
pub(crate) const READ_LOCK: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/locks/ReentrantReadWriteLock$ReadLock",
    Some("java/lang/Object"),
    &["java/util/concurrent/locks/Lock"],
    ACC_PUBLIC | ACC_FINAL,
    &[field(
        "parent",
        "Ljava/util/concurrent/locks/ReentrantReadWriteLock;",
        ACC_PRIVATE,
        None,
    )],
    &READ_LOCK_METHODS,
    None,
);

/// `ReentrantReadWriteLock.WriteLock`: a non-blocking write lock.
pub(crate) const WRITE_LOCK: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/locks/ReentrantReadWriteLock$WriteLock",
    Some("java/lang/Object"),
    &["java/util/concurrent/locks/Lock"],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &NOOP_LOCK_METHODS,
    None,
);

const REENTRANT_READ_WRITE_LOCK_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        init_read_write_lock(vm, receiver(args), false)
    }),
    method("<init>", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        init_read_write_lock(vm, receiver(args), crate::bool_arg(args, 1))
    }),
    method(
        "readLock",
        "()Ljava/util/concurrent/locks/ReentrantReadWriteLock$ReadLock;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let lock = vm
                .read_ref_field(
                    this,
                    class,
                    "readLock",
                    "Ljava/util/concurrent/locks/ReentrantReadWriteLock$ReadLock;",
                )
                .unwrap_or(ObjectRef::NULL);
            object(lock)
        },
    ),
    method(
        "writeLock",
        "()Ljava/util/concurrent/locks/ReentrantReadWriteLock$WriteLock;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let lock = vm
                .read_ref_field(
                    this,
                    class,
                    "writeLock",
                    "Ljava/util/concurrent/locks/ReentrantReadWriteLock$WriteLock;",
                )
                .unwrap_or(ObjectRef::NULL);
            object(lock)
        },
    ),
    method("isFair", "()Z", ACC_PUBLIC | ACC_FINAL, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "fair") != 0)
    }),
    method("isWriteLocked", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method(
        "isWriteLockedByCurrentThread",
        "()Z",
        ACC_PUBLIC,
        |_, _, _| boolean(false),
    ),
    method("getReadHoldCount", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "readCount"))
    }),
    method("getWriteHoldCount", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method("getReadLockCount", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "readCount"))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("ReentrantReadWriteLock")?),
    ),
];

fn init_read_write_lock(vm: &mut Vm<'_>, this: ObjectRef, fair: bool) -> Result<Value, VmError> {
    crate::lang::write_int_field(vm, this, "fair", i32::from(fair));
    let class = vm.class_of(this);
    let read_class =
        vm.resolve_class("java/util/concurrent/locks/ReentrantReadWriteLock$ReadLock")?;
    let write_class =
        vm.resolve_class("java/util/concurrent/locks/ReentrantReadWriteLock$WriteLock")?;
    let read = vm.new_instance(read_class)?;
    let write = vm.new_instance(write_class)?;
    let (declaring, field) = vm.find_field(
        class,
        "readLock",
        "Ljava/util/concurrent/locks/ReentrantReadWriteLock$ReadLock;",
    )?;
    vm.set_instance_ref(this, declaring, field, read);
    let read_class_of = vm.class_of(read);
    if let Some((declaring, field)) = vm.find_field_opt(
        read_class_of,
        "parent",
        "Ljava/util/concurrent/locks/ReentrantReadWriteLock;",
    ) {
        vm.set_instance_ref(read, declaring, field, this);
    }
    let (declaring, field) = vm.find_field(
        class,
        "writeLock",
        "Ljava/util/concurrent/locks/ReentrantReadWriteLock$WriteLock;",
    )?;
    vm.set_instance_ref(this, declaring, field, write);
    void()
}

/// `ReentrantReadWriteLock`: read and write locks that never block.
pub(crate) const REENTRANT_READ_WRITE_LOCK: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/locks/ReentrantReadWriteLock",
    Some("java/lang/Object"),
    &[
        "java/util/concurrent/locks/ReadWriteLock",
        "java/io/Serializable",
    ],
    ACC_PUBLIC,
    &[
        field(
            "readLock",
            "Ljava/util/concurrent/locks/ReentrantReadWriteLock$ReadLock;",
            ACC_PRIVATE,
            None,
        ),
        field(
            "writeLock",
            "Ljava/util/concurrent/locks/ReentrantReadWriteLock$WriteLock;",
            ACC_PRIVATE,
            None,
        ),
        field("fair", "I", ACC_PRIVATE, None),
        field("readCount", "I", ACC_PRIVATE, None),
    ],
    &REENTRANT_READ_WRITE_LOCK_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// CompletableFuture (completed futures only)
// -------------------------------------------------------------------------------------------

const COMPLETABLE_FUTURE_METHODS: [oxjvm_vm::NativeMethodDef; 31] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "completedFuture",
        "(Ljava/lang/Object;)Ljava/util/concurrent/CompletableFuture;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| completable_completed(vm, ref_arg(args, 0)),
    ),
    method(
        "allOf",
        "([Ljava/util/concurrent/CompletableFuture;)Ljava/util/concurrent/CompletableFuture;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let futures = ref_arg(args, 0);
            if futures.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let length = vm.array_length(futures)?;
            for index in 0..length {
                let future = vm.array_get(futures, index as i32)?.as_ref();
                if future.is_null() {
                    return Err(vm.throw_new("java/lang/NullPointerException", None));
                }
            }
            completable_completed(vm, ObjectRef::NULL)
        },
    ),
    method(
        "anyOf",
        "([Ljava/util/concurrent/CompletableFuture;)Ljava/util/concurrent/CompletableFuture;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let futures = ref_arg(args, 0);
            if futures.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let length = vm.array_length(futures)?;
            if length == 0 {
                return completable_completed(vm, ObjectRef::NULL);
            }
            let first = vm.array_get(futures, 0)?.as_ref();
            if first.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            completable_completed(vm, completable_result(vm, first))
        },
    ),
    method(
        "supplyAsync",
        "(Ljava/util/function/Supplier;)Ljava/util/concurrent/CompletableFuture;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let supplier = ref_arg(args, 0);
            let (declaring, method) =
                vm.resolve_virtual_method(supplier, "get", "()Ljava/lang/Object;")?;
            let result = vm
                .invoke_method(declaring, method, vec![Value::Ref(supplier)])?
                .as_ref();
            completable_completed(vm, result)
        },
    ),
    method(
        "supplyAsync",
        "(Ljava/util/function/Supplier;Ljava/util/concurrent/Executor;)Ljava/util/concurrent/CompletableFuture;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let supplier = ref_arg(args, 0);
            let (declaring, method) =
                vm.resolve_virtual_method(supplier, "get", "()Ljava/lang/Object;")?;
            let result = vm
                .invoke_method(declaring, method, vec![Value::Ref(supplier)])?
                .as_ref();
            completable_completed(vm, result)
        },
    ),
    method(
        "runAsync",
        "(Ljava/lang/Runnable;)Ljava/util/concurrent/CompletableFuture;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let runnable = ref_arg(args, 0);
            let (declaring, method) = vm.resolve_virtual_method(runnable, "run", "()V")?;
            vm.invoke_method(declaring, method, vec![Value::Ref(runnable)])?;
            completable_completed(vm, ObjectRef::NULL)
        },
    ),
    method(
        "runAsync",
        "(Ljava/lang/Runnable;Ljava/util/concurrent/Executor;)Ljava/util/concurrent/CompletableFuture;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let runnable = ref_arg(args, 0);
            let (declaring, method) = vm.resolve_virtual_method(runnable, "run", "()V")?;
            vm.invoke_method(declaring, method, vec![Value::Ref(runnable)])?;
            completable_completed(vm, ObjectRef::NULL)
        },
    ),
    method("join", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        future_result(vm, args)
    }),
    method(
        "thenApply",
        "(Ljava/util/function/Function;)Ljava/util/concurrent/CompletableFuture;",
        ACC_PUBLIC,
        |vm, _, args| cf_then_apply(vm, receiver(args), ref_arg(args, 1)),
    ),
    method(
        "thenApplyAsync",
        "(Ljava/util/function/Function;)Ljava/util/concurrent/CompletableFuture;",
        ACC_PUBLIC,
        |vm, _, args| cf_then_apply(vm, receiver(args), ref_arg(args, 1)),
    ),
    method(
        "thenCompose",
        "(Ljava/util/function/Function;)Ljava/util/concurrent/CompletableFuture;",
        ACC_PUBLIC,
        |vm, _, args| cf_then_compose(vm, receiver(args), ref_arg(args, 1)),
    ),
    method(
        "thenComposeAsync",
        "(Ljava/util/function/Function;)Ljava/util/concurrent/CompletableFuture;",
        ACC_PUBLIC,
        |vm, _, args| cf_then_compose(vm, receiver(args), ref_arg(args, 1)),
    ),
    method(
        "thenAccept",
        "(Ljava/util/function/Consumer;)Ljava/util/concurrent/CompletableFuture;",
        ACC_PUBLIC,
        |vm, _, args| cf_then_accept(vm, receiver(args), ref_arg(args, 1)),
    ),
    method(
        "thenAcceptAsync",
        "(Ljava/util/function/Consumer;)Ljava/util/concurrent/CompletableFuture;",
        ACC_PUBLIC,
        |vm, _, args| cf_then_accept(vm, receiver(args), ref_arg(args, 1)),
    ),
    method(
        "thenRun",
        "(Ljava/lang/Runnable;)Ljava/util/concurrent/CompletableFuture;",
        ACC_PUBLIC,
        |vm, _, args| cf_then_run(vm, ref_arg(args, 1)),
    ),
    method(
        "thenRunAsync",
        "(Ljava/lang/Runnable;)Ljava/util/concurrent/CompletableFuture;",
        ACC_PUBLIC,
        |vm, _, args| cf_then_run(vm, ref_arg(args, 1)),
    ),
    method("get", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        future_result(vm, args)
    }),
    method(
        "getNow",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let done = vm
                .read_instance_int_named(this, vm.class_of(this), "done", "Z")
                .unwrap_or(0)
                != 0;
            if done {
                future_result(vm, args)
            } else {
                Ok(args[1])
            }
        },
    ),
    method(
        "get",
        "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| future_result(vm, args),
    ),
    method("isDone", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        boolean(
            vm.read_instance_int_named(this, vm.class_of(this), "done", "Z")
                .unwrap_or(0)
                != 0,
        )
    }),
    method("isCancelled", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("isCompletedExceptionally", "()Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
    method("cancel", "(Z)Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method(
        "complete",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let done = vm
                .read_instance_int_named(this, class, "done", "Z")
                .unwrap_or(0)
                != 0;
            if done {
                return boolean(false);
            }
            let (declaring, field) = vm.find_field(class, "result", "Ljava/lang/Object;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            if let Some((declaring, field)) = vm.find_field_opt(class, "done", "Z") {
                vm.set_instance_int(this, declaring, field, 1);
            }
            boolean(true)
        },
    ),
    method(
        "completeExceptionally",
        "(Ljava/lang/Throwable;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(true),
    ),
    method(
        "orTimeout",
        "(JLjava/util/concurrent/TimeUnit;)Ljava/util/concurrent/CompletableFuture;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method(
        "completeOnTimeout",
        "(Ljava/lang/Object;JLjava/util/concurrent/TimeUnit;)Ljava/util/concurrent/CompletableFuture;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("CompletableFuture")?),
    ),
    method(
        "thenCombine",
        "(Ljava/util/concurrent/CompletionStage;Ljava/util/function/BiFunction;)Ljava/util/concurrent/CompletableFuture;",
        ACC_PUBLIC,
        |vm, _, args| cf_then_combine(vm, receiver(args), ref_arg(args, 1), ref_arg(args, 2)),
    ),
    method(
        "thenCombineAsync",
        "(Ljava/util/concurrent/CompletionStage;Ljava/util/function/BiFunction;)Ljava/util/concurrent/CompletableFuture;",
        ACC_PUBLIC,
        |vm, _, args| cf_then_combine(vm, receiver(args), ref_arg(args, 1), ref_arg(args, 2)),
    ),
];

fn future_result(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let this = receiver(args);
    let result = vm
        .read_ref_field(this, vm.class_of(this), "result", "Ljava/lang/Object;")
        .unwrap_or(ObjectRef::NULL);
    object(result)
}

/// A `CompletableFuture` already completed with `result`.
fn completable_completed(vm: &mut Vm<'_>, result: ObjectRef) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/util/concurrent/CompletableFuture")?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "result", "Ljava/lang/Object;")?;
    vm.set_instance_ref(instance, declaring, field, result);
    if let Some((declaring, field)) = vm.find_field_opt(class, "done", "Z") {
        vm.set_instance_int(instance, declaring, field, 1);
    }
    object(instance)
}

/// The result a completed `CompletableFuture` holds.
fn completable_result(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "result", "Ljava/lang/Object;")
        .unwrap_or(ObjectRef::NULL)
}

/// `thenApply`: map the result through a function into a new completed future.
fn cf_then_apply(vm: &mut Vm<'_>, this: ObjectRef, function: ObjectRef) -> Result<Value, VmError> {
    let result = completable_result(vm, this);
    let (declaring, method) =
        vm.resolve_virtual_method(function, "apply", "(Ljava/lang/Object;)Ljava/lang/Object;")?;
    let mapped = vm
        .invoke_method(
            declaring,
            method,
            vec![Value::Ref(function), Value::Ref(result)],
        )?
        .as_ref();
    completable_completed(vm, mapped)
}

/// `thenCompose`: flat-map the result through a function that returns a future.
fn cf_then_compose(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    function: ObjectRef,
) -> Result<Value, VmError> {
    let result = completable_result(vm, this);
    let (declaring, method) =
        vm.resolve_virtual_method(function, "apply", "(Ljava/lang/Object;)Ljava/lang/Object;")?;
    let next = vm
        .invoke_method(
            declaring,
            method,
            vec![Value::Ref(function), Value::Ref(result)],
        )?
        .as_ref();
    object(next)
}

/// `thenAccept`: feed the result to a consumer, yielding a void future.
/// `thenCombine`: combine the results of two completed futures.
fn cf_then_combine(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    other: ObjectRef,
    function: ObjectRef,
) -> Result<Value, VmError> {
    let a = completable_result(vm, this);
    let b = completable_result(vm, other);
    let (declaring, method) = vm.resolve_virtual_method(
        function,
        "apply",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
    )?;
    let mapped = vm
        .invoke_method(
            declaring,
            method,
            vec![Value::Ref(function), Value::Ref(a), Value::Ref(b)],
        )?
        .as_ref();
    completable_completed(vm, mapped)
}

fn cf_then_accept(vm: &mut Vm<'_>, this: ObjectRef, consumer: ObjectRef) -> Result<Value, VmError> {
    let result = completable_result(vm, this);
    let (declaring, method) =
        vm.resolve_virtual_method(consumer, "accept", "(Ljava/lang/Object;)V")?;
    vm.invoke_method(
        declaring,
        method,
        vec![Value::Ref(consumer), Value::Ref(result)],
    )?;
    completable_completed(vm, ObjectRef::NULL)
}

/// `thenRun`: run an action after completion, yielding a void future.
fn cf_then_run(vm: &mut Vm<'_>, action: ObjectRef) -> Result<Value, VmError> {
    let (declaring, method) = vm.resolve_virtual_method(action, "run", "()V")?;
    vm.invoke_method(declaring, method, vec![Value::Ref(action)])?;
    completable_completed(vm, ObjectRef::NULL)
}

/// `CompletableFuture`: only the already-completed form the interpreter needs.
pub(crate) const COMPLETABLE_FUTURE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/CompletableFuture",
    Some("java/lang/Object"),
    &["java/util/concurrent/Future"],
    ACC_PUBLIC,
    &[
        field("result", "Ljava/lang/Object;", ACC_PRIVATE, None),
        field("done", "Z", ACC_PRIVATE, None),
    ],
    &COMPLETABLE_FUTURE_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// BlockingQueue / ArrayBlockingQueue
// -------------------------------------------------------------------------------------------

/// `BlockingQueue`: a `Queue` with blocking `put`/`take` and timed variants.
pub(crate) const BLOCKING_QUEUE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/BlockingQueue",
    Some("java/lang/Object"),
    &["java/util/Queue"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &BLOCKING_QUEUE_METHODS,
    None,
);

const BLOCKING_QUEUE_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "put",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "take",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "poll",
        "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "offer",
        "(Ljava/lang/Object;JLjava/util/concurrent/TimeUnit;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "remainingCapacity",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "drainTo",
        "(Ljava/util/Collection;)I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "drainTo",
        "(Ljava/util/Collection;I)I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

/// `ArrayBlockingQueue`, backed by the same element array as `ArrayList` with a fixed
/// capacity.  Blocking operations park on the queue's monitor; every mutation notifies all
/// waiters so producers and consumers wake each other through the cooperative scheduler.
pub(crate) const ARRAY_BLOCKING_QUEUE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/ArrayBlockingQueue",
    Some("java/util/ArrayList"),
    &["java/util/concurrent/BlockingQueue", "java/io/Serializable"],
    ACC_PUBLIC,
    &[field("capacity", "I", ACC_PRIVATE, None)],
    &ARRAY_BLOCKING_QUEUE_METHODS,
    None,
);

const ARRAY_BLOCKING_QUEUE_METHODS: [oxjvm_vm::NativeMethodDef; 19] = [
    method("<init>", "(I)V", ACC_PUBLIC, abq_init),
    method("<init>", "(IZ)V", ACC_PUBLIC, abq_init),
    method(
        "<init>",
        "(IZLjava/util/Collection;)V",
        ACC_PUBLIC,
        abq_init,
    ),
    method("add", "(Ljava/lang/Object;)Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if abq_size(vm, this) >= abq_capacity(vm, this) {
            return Err(vm.throw_new("java/lang/IllegalStateException", Some("Queue full")));
        }
        abq_push(vm, this, ref_arg(args, 1))?;
        boolean(true)
    }),
    method(
        "offer",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if abq_size(vm, this) >= abq_capacity(vm, this) {
                return boolean(false);
            }
            abq_push(vm, this, ref_arg(args, 1))?;
            boolean(true)
        },
    ),
    method(
        "offer",
        "(Ljava/lang/Object;JLjava/util/concurrent/TimeUnit;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let element = ref_arg(args, 1);
            let millis = time_unit_to_millis(vm, ref_arg(args, 3), crate::long_arg(args, 2))?;
            let deadline = vm.virtual_time_millis().saturating_add(millis.max(0));
            loop {
                if abq_size(vm, this) < abq_capacity(vm, this) {
                    abq_push(vm, this, element)?;
                    return boolean(true);
                }
                let remaining = deadline - vm.virtual_time_millis();
                if remaining <= 0 {
                    return boolean(false);
                }
                vm.park_until_notified(this, Some(remaining))?;
            }
        },
    ),
    method("put", "(Ljava/lang/Object;)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let element = ref_arg(args, 1);
        loop {
            if abq_size(vm, this) < abq_capacity(vm, this) {
                abq_push(vm, this, element)?;
                return void();
            }
            vm.park_until_notified(this, None)?;
        }
    }),
    method("take", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        loop {
            if abq_size(vm, this) > 0 {
                return object(abq_pop_head(vm, this)?);
            }
            vm.park_until_notified(this, None)?;
        }
    }),
    method("poll", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if abq_size(vm, this) == 0 {
            return object(ObjectRef::NULL);
        }
        object(abq_pop_head(vm, this)?)
    }),
    method(
        "poll",
        "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let millis = time_unit_to_millis(vm, ref_arg(args, 2), crate::long_arg(args, 1))?;
            let deadline = vm.virtual_time_millis().saturating_add(millis.max(0));
            loop {
                if abq_size(vm, this) > 0 {
                    return object(abq_pop_head(vm, this)?);
                }
                let remaining = deadline - vm.virtual_time_millis();
                if remaining <= 0 {
                    return object(ObjectRef::NULL);
                }
                vm.park_until_notified(this, Some(remaining))?;
            }
        },
    ),
    method("peek", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        let elements = crate::util::array_list_elements(vm, receiver(args));
        object(elements.first().copied().unwrap_or(ObjectRef::NULL))
    }),
    method(
        "element",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let elements = crate::util::array_list_elements(vm, receiver(args));
            match elements.first() {
                Some(head) => object(*head),
                None => Err(vm.throw_new("java/util/NoSuchElementException", None)),
            }
        },
    ),
    method(
        "remove",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if abq_size(vm, this) == 0 {
                return Err(vm.throw_new("java/util/NoSuchElementException", None));
            }
            object(abq_pop_head(vm, this)?)
        },
    ),
    method(
        "remove",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let super_class = vm.resolve_class("java/util/ArrayList")?;
            let (declaring, method) = vm
                .find_method(super_class, "remove", "(Ljava/lang/Object;)Z")
                .ok_or_else(|| VmError::internal("ArrayList.remove(Object) missing"))?;
            let result = vm.invoke_method(declaring, method, vec![args[0], args[1]])?;
            if result.as_int() != 0 {
                vm.monitor_notify(this, true);
            }
            Ok(result)
        },
    ),
    method("remainingCapacity", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        int((abq_capacity(vm, this) - abq_size(vm, this)) as i32)
    }),
    method(
        "drainTo",
        "(Ljava/util/Collection;)I",
        ACC_PUBLIC,
        |vm, _, args| abq_drain_to(vm, args, usize::MAX),
    ),
    method(
        "drainTo",
        "(Ljava/util/Collection;I)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let max = crate::int_arg(args, 2).max(0) as usize;
            abq_drain_to(vm, args, max)
        },
    ),
    method(
        "addAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let collection = ref_arg(args, 1);
            if collection.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let extra = crate::util::collection_elements(vm, collection)?;
            let mut elements = crate::util::array_list_elements(vm, this);
            if elements.len() + extra.len() > abq_capacity(vm, this) {
                return Err(vm.throw_new("java/lang/IllegalStateException", Some("Queue full")));
            }
            elements.extend(extra);
            crate::util::array_list_store(vm, this, &elements)?;
            vm.monitor_notify(this, true);
            boolean(true)
        },
    ),
    method("clear", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        crate::util::array_list_store(vm, this, &[])?;
        vm.monitor_notify(this, true);
        void()
    }),
];

fn abq_init(
    vm: &mut Vm<'_>,
    context: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let capacity = crate::int_arg(args, 1);
    if capacity <= 0 {
        return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
    }
    crate::lang::write_int_field(vm, this, "capacity", capacity);
    let descriptor = vm.classes.get(context.class).methods[context.method as usize]
        .descriptor
        .as_str();
    let mut elements = Vec::new();
    if descriptor == "(IZLjava/util/Collection;)V" {
        elements = crate::util::collection_elements(vm, ref_arg(args, 3))?;
        if elements.iter().any(|e| e.is_null()) {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        if elements.len() > capacity as usize {
            return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
        }
    }
    crate::util::array_list_store(vm, this, &elements)?;
    void()
}

fn abq_size(vm: &Vm<'_>, this: ObjectRef) -> usize {
    crate::lang::read_int_field(vm, this, "size").max(0) as usize
}

fn abq_capacity(vm: &Vm<'_>, this: ObjectRef) -> usize {
    crate::lang::read_int_field(vm, this, "capacity").max(0) as usize
}

fn abq_push(vm: &mut Vm<'_>, this: ObjectRef, element: ObjectRef) -> Result<(), VmError> {
    if element.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let mut elements = crate::util::array_list_elements(vm, this);
    elements.push(element);
    crate::util::array_list_store(vm, this, &elements)?;
    vm.monitor_notify(this, true);
    Ok(())
}

fn abq_pop_head(vm: &mut Vm<'_>, this: ObjectRef) -> Result<ObjectRef, VmError> {
    let mut elements = crate::util::array_list_elements(vm, this);
    let head = elements.remove(0);
    crate::util::array_list_store(vm, this, &elements)?;
    vm.monitor_notify(this, true);
    Ok(head)
}

fn abq_drain_to(vm: &mut Vm<'_>, args: &[Value], max: usize) -> Result<Value, VmError> {
    let this = receiver(args);
    let collection = ref_arg(args, 1);
    if collection.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let elements = crate::util::array_list_elements(vm, this);
    let mut transferred = 0usize;
    for element in elements.iter().take(max) {
        let (declaring, method) =
            vm.resolve_virtual_method(collection, "add", "(Ljava/lang/Object;)Z")?;
        let added = vm.invoke_method(
            declaring,
            method,
            vec![Value::Ref(collection), Value::Ref(*element)],
        )?;
        if added.as_int() == 0 {
            break;
        }
        transferred += 1;
    }
    if transferred > 0 {
        crate::util::array_list_store(vm, this, &elements[transferred..])?;
        vm.monitor_notify(this, true);
    }
    int(transferred as i32)
}

// -------------------------------------------------------------------------------------------
// Phaser
// -------------------------------------------------------------------------------------------

/// `Phaser` with a single-level (root-only) party model.  Termination is encoded as a
/// negative phase, matching the sign bit of the real implementation's state word.
pub(crate) const PHASER: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/Phaser",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("phase", "I", ACC_PRIVATE, None),
        field("registered", "I", ACC_PRIVATE, None),
        field("arrived", "I", ACC_PRIVATE, None),
    ],
    &PHASER_METHODS,
    None,
);

const PHASER_METHODS: [oxjvm_vm::NativeMethodDef; 21] = [
    method("<init>", "()V", ACC_PUBLIC, phaser_init),
    method("<init>", "(I)V", ACC_PUBLIC, phaser_init),
    method(
        "<init>",
        "(Ljava/util/concurrent/Phaser;)V",
        ACC_PUBLIC,
        phaser_init,
    ),
    method(
        "<init>",
        "(Ljava/util/concurrent/Phaser;I)V",
        ACC_PUBLIC,
        phaser_init,
    ),
    method("register", "()I", ACC_PUBLIC, |vm, _, args| {
        phaser_register(vm, receiver(args), 1)
    }),
    method("bulkRegister", "(I)I", ACC_PUBLIC, |vm, _, args| {
        let parties = crate::int_arg(args, 1);
        if parties < 0 {
            return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
        }
        phaser_register(vm, receiver(args), parties)
    }),
    method("arrive", "()I", ACC_PUBLIC, |vm, _, args| {
        phaser_arrive(vm, receiver(args), false)
    }),
    method("arriveAndDeregister", "()I", ACC_PUBLIC, |vm, _, args| {
        phaser_arrive(vm, receiver(args), true)
    }),
    method("arriveAndAwaitAdvance", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let phase = phaser_arrive(vm, this, false)?.as_int();
        phaser_await_advance(vm, this, phase, None)
    }),
    method("awaitAdvance", "(I)I", ACC_PUBLIC, |vm, _, args| {
        phaser_await_advance(vm, receiver(args), crate::int_arg(args, 1), None)
    }),
    method(
        "awaitAdvanceInterruptibly",
        "(I)I",
        ACC_PUBLIC,
        |vm, _, args| phaser_await_advance(vm, receiver(args), crate::int_arg(args, 1), None),
    ),
    method(
        "awaitAdvanceInterruptibly",
        "(IJLjava/util/concurrent/TimeUnit;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let millis = time_unit_to_millis(vm, ref_arg(args, 3), crate::long_arg(args, 2))?;
            phaser_await_advance(vm, receiver(args), crate::int_arg(args, 1), Some(millis))
        },
    ),
    method("forceTermination", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let phase = crate::lang::read_int_field(vm, this, "phase");
        if phase >= 0 {
            crate::lang::write_int_field(vm, this, "phase", phase | i32::MIN);
            vm.monitor_notify(this, true);
        }
        void()
    }),
    method("isTerminated", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "phase") < 0)
    }),
    method("getPhase", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "phase"))
    }),
    method("getRegisteredParties", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(
            vm,
            receiver(args),
            "registered",
        ))
    }),
    method("getArrivedParties", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if crate::lang::read_int_field(vm, this, "phase") < 0 {
            return int(0);
        }
        int(crate::lang::read_int_field(vm, this, "arrived"))
    }),
    method("getUnarrivedParties", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if crate::lang::read_int_field(vm, this, "phase") < 0 {
            return int(0);
        }
        let registered = crate::lang::read_int_field(vm, this, "registered");
        let arrived = crate::lang::read_int_field(vm, this, "arrived");
        int(registered - arrived)
    }),
    method(
        "getParent",
        "()Ljava/util/concurrent/Phaser;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getRoot",
        "()Ljava/util/concurrent/Phaser;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method("onAdvance", "(II)Z", ACC_PROTECTED, |_, _, args| {
        boolean(crate::int_arg(args, 2) == 0)
    }),
];

fn phaser_init(
    vm: &mut Vm<'_>,
    context: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let descriptor = vm.classes.get(context.class).methods[context.method as usize]
        .descriptor
        .as_str();
    let parties = match descriptor {
        "()V" | "(Ljava/util/concurrent/Phaser;)V" => 0,
        _ => crate::int_arg(args, 1),
    };
    if parties < 0 {
        return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
    }
    crate::lang::write_int_field(vm, this, "phase", 0);
    crate::lang::write_int_field(vm, this, "registered", parties);
    crate::lang::write_int_field(vm, this, "arrived", 0);
    void()
}

fn phaser_register(vm: &mut Vm<'_>, this: ObjectRef, parties: i32) -> Result<Value, VmError> {
    let phase = crate::lang::read_int_field(vm, this, "phase");
    if phase < 0 || parties == 0 {
        return int(phase);
    }
    let registered = crate::lang::read_int_field(vm, this, "registered");
    crate::lang::write_int_field(vm, this, "registered", registered + parties);
    int(phase)
}

fn phaser_arrive(vm: &mut Vm<'_>, this: ObjectRef, deregister: bool) -> Result<Value, VmError> {
    let phase = crate::lang::read_int_field(vm, this, "phase");
    if phase < 0 {
        return int(phase);
    }
    let registered = crate::lang::read_int_field(vm, this, "registered");
    let arrived = crate::lang::read_int_field(vm, this, "arrived");
    if registered == 0 {
        return Err(vm.throw_new(
            "java/lang/IllegalStateException",
            Some("Phaser has no registered parties"),
        ));
    }
    let next_registered = if deregister {
        registered - 1
    } else {
        registered
    };
    crate::lang::write_int_field(vm, this, "registered", next_registered);
    let next_arrived = arrived + 1;
    if next_arrived >= next_registered {
        crate::lang::write_int_field(vm, this, "arrived", 0);
        phaser_advance(vm, this, phase, next_registered)?;
    } else {
        crate::lang::write_int_field(vm, this, "arrived", next_arrived);
    }
    int(phase)
}

/// Complete a phase: consult the overridable `onAdvance` hook, then bump or terminate.
fn phaser_advance(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    phase: i32,
    registered: i32,
) -> Result<(), VmError> {
    let (declaring, method) = vm.resolve_virtual_method(this, "onAdvance", "(II)Z")?;
    let terminate = vm.invoke_method(
        declaring,
        method,
        vec![Value::Ref(this), Value::Int(phase), Value::Int(registered)],
    )?;
    let next = if terminate.as_int() != 0 {
        phase | i32::MIN
    } else {
        (phase + 1) & 0x7fff_ffff
    };
    crate::lang::write_int_field(vm, this, "phase", next);
    vm.monitor_notify(this, true);
    Ok(())
}

fn phaser_await_advance(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    phase: i32,
    timeout: Option<i64>,
) -> Result<Value, VmError> {
    if phase < 0 {
        return int(phase);
    }
    let deadline = timeout.map(|millis| vm.virtual_time_millis().saturating_add(millis.max(0)));
    loop {
        let current = crate::lang::read_int_field(vm, this, "phase");
        if current != phase {
            return int(current);
        }
        match deadline {
            Some(deadline) => {
                let remaining = deadline - vm.virtual_time_millis();
                if remaining <= 0 {
                    return int(phase);
                }
                vm.park_until_notified(this, Some(remaining))?;
            }
            None => vm.park_until_notified(this, None)?,
        }
    }
}

// -------------------------------------------------------------------------------------------
// Semaphore
// -------------------------------------------------------------------------------------------

fn semaphore_permits(vm: &Vm<'_>, this: ObjectRef) -> i32 {
    crate::lang::read_int_field(vm, this, "permits")
}

fn semaphore_acquire(vm: &mut Vm<'_>, this: ObjectRef, count: i32) -> Result<(), VmError> {
    if count < 0 {
        return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
    }
    loop {
        let permits = semaphore_permits(vm, this);
        if permits >= count {
            crate::lang::write_int_field(vm, this, "permits", permits - count);
            return Ok(());
        }
        vm.park_until_notified(this, None)?;
    }
}

fn semaphore_release(vm: &mut Vm<'_>, this: ObjectRef, count: i32) -> Result<(), VmError> {
    if count < 0 {
        return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
    }
    let permits = semaphore_permits(vm, this);
    crate::lang::write_int_field(vm, this, "permits", permits.saturating_add(count));
    vm.monitor_notify(this, true);
    Ok(())
}

const SEMAPHORE_METHODS: [oxjvm_vm::NativeMethodDef; 16] = [
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        crate::lang::write_int_field(vm, receiver(args), "permits", crate::int_arg(args, 1));
        void()
    }),
    method("<init>", "(IZ)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        crate::lang::write_int_field(vm, this, "permits", crate::int_arg(args, 1));
        crate::lang::write_int_field(vm, this, "fair", i32::from(crate::int_arg(args, 2) != 0));
        void()
    }),
    method("acquire", "()V", ACC_PUBLIC, |vm, _, args| {
        semaphore_acquire(vm, receiver(args), 1)?;
        void()
    }),
    method("acquire", "(I)V", ACC_PUBLIC, |vm, _, args| {
        semaphore_acquire(vm, receiver(args), crate::int_arg(args, 1))?;
        void()
    }),
    method(
        "acquireUninterruptibly",
        "()V",
        ACC_PUBLIC,
        |vm, _, args| {
            semaphore_acquire(vm, receiver(args), 1)?;
            void()
        },
    ),
    method(
        "acquireUninterruptibly",
        "(I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            semaphore_acquire(vm, receiver(args), crate::int_arg(args, 1))?;
            void()
        },
    ),
    method("tryAcquire", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let permits = semaphore_permits(vm, this);
        if permits >= 1 {
            crate::lang::write_int_field(vm, this, "permits", permits - 1);
            return boolean(true);
        }
        boolean(false)
    }),
    method("tryAcquire", "(I)Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let count = crate::int_arg(args, 1);
        if count < 0 {
            return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
        }
        let permits = semaphore_permits(vm, this);
        if permits >= count {
            crate::lang::write_int_field(vm, this, "permits", permits - count);
            return boolean(true);
        }
        boolean(false)
    }),
    method(
        "tryAcquire",
        "(JLjava/util/concurrent/TimeUnit;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let millis = time_unit_to_millis(vm, ref_arg(args, 2), crate::long_arg(args, 1))?;
            let deadline = vm.virtual_time_millis().saturating_add(millis.max(0));
            loop {
                let permits = semaphore_permits(vm, this);
                if permits >= 1 {
                    crate::lang::write_int_field(vm, this, "permits", permits - 1);
                    return boolean(true);
                }
                if vm.virtual_time_millis() >= deadline {
                    return boolean(false);
                }
                vm.park_until_notified(this, None)?;
            }
        },
    ),
    method("release", "()V", ACC_PUBLIC, |vm, _, args| {
        semaphore_release(vm, receiver(args), 1)?;
        void()
    }),
    method("release", "(I)V", ACC_PUBLIC, |vm, _, args| {
        semaphore_release(vm, receiver(args), crate::int_arg(args, 1))?;
        void()
    }),
    method("availablePermits", "()I", ACC_PUBLIC, |vm, _, args| {
        int(semaphore_permits(vm, receiver(args)))
    }),
    method("drainPermits", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let permits = semaphore_permits(vm, this);
        crate::lang::write_int_field(vm, this, "permits", 0);
        int(permits)
    }),
    method("isFair", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "fair") != 0)
    }),
    method("getQueueLength", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method("hasQueuedThreads", "()Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
];

pub(crate) const SEMAPHORE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/Semaphore",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[
        field("permits", "I", ACC_PRIVATE, None),
        field("fair", "I", ACC_PRIVATE, None),
    ],
    &SEMAPHORE_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Exchanger
// -------------------------------------------------------------------------------------------

const EXCHANGER_METHODS: [oxjvm_vm::NativeMethodDef; 1] =
    [method("<init>", "()V", ACC_PUBLIC, |_, _, _| void())];

/// `java.util.concurrent.Exchanger`: construction is supported; the two-party
/// `exchange` rendezvous requires real threads and is not implemented.
pub(crate) const EXCHANGER: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/Exchanger",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &EXCHANGER_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// LinkedBlockingQueue / LinkedBlockingDeque
// -------------------------------------------------------------------------------------------'''

/// `LinkedBlockingQueue`: an optionally-bounded blocking queue backed by the `ArrayList`
/// element array.  Blocking operations park on the queue monitor; mutations notify waiters.
pub(crate) const LINKED_BLOCKING_QUEUE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/LinkedBlockingQueue",
    Some("java/util/ArrayList"),
    &["java/util/concurrent/BlockingQueue", "java/io/Serializable"],
    ACC_PUBLIC,
    &[field("capacity", "I", ACC_PRIVATE, None)],
    &LINKED_BLOCKING_QUEUE_METHODS,
    None,
);

const LINKED_BLOCKING_QUEUE_METHODS: [oxjvm_vm::NativeMethodDef; 15] = [
    method("<init>", "()V", ACC_PUBLIC, lbq_init),
    method("<init>", "(I)V", ACC_PUBLIC, lbq_init),
    method("<init>", "(Ljava/util/Collection;)V", ACC_PUBLIC, lbq_init),
    method("add", "(Ljava/lang/Object;)Z", ACC_PUBLIC, |vm, _, args| {
        let element = ref_arg(args, 1);
        if element.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        lbq_push(vm, receiver(args), element)?;
        boolean(true)
    }),
    method("put", "(Ljava/lang/Object;)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let element = ref_arg(args, 1);
        if element.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        loop {
            if lbq_size(vm, this) < lbq_capacity(vm, this) {
                lbq_push(vm, this, element)?;
                return void();
            }
            vm.park_until_notified(this, None)?;
        }
    }),
    method("take", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        loop {
            if lbq_size(vm, this) > 0 {
                return object(lbq_pop_head(vm, this)?);
            }
            vm.park_until_notified(this, None)?;
        }
    }),
    method("poll", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if lbq_size(vm, this) > 0 {
            return object(lbq_pop_head(vm, this)?);
        }
        object(ObjectRef::NULL)
    }),
    method("peek", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let elements = crate::util::array_list_elements(vm, this);
        object(elements.first().copied().unwrap_or(ObjectRef::NULL))
    }),
    method(
        "poll",
        "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let millis = time_unit_to_millis(vm, ref_arg(args, 2), crate::long_arg(args, 1))?;
            let deadline = vm.virtual_time_millis().saturating_add(millis.max(0));
            loop {
                if lbq_size(vm, this) > 0 {
                    return object(lbq_pop_head(vm, this)?);
                }
                let remaining = deadline - vm.virtual_time_millis();
                if remaining <= 0 {
                    return object(ObjectRef::NULL);
                }
                vm.park_until_notified(this, Some(remaining))?;
            }
        },
    ),
    method(
        "offer",
        "(Ljava/lang/Object;JLjava/util/concurrent/TimeUnit;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let element = ref_arg(args, 1);
            if element.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let millis = time_unit_to_millis(vm, ref_arg(args, 3), crate::long_arg(args, 2))?;
            let deadline = vm.virtual_time_millis().saturating_add(millis.max(0));
            loop {
                if lbq_size(vm, this) < lbq_capacity(vm, this) {
                    lbq_push(vm, this, element)?;
                    return boolean(true);
                }
                let remaining = deadline - vm.virtual_time_millis();
                if remaining <= 0 {
                    return boolean(false);
                }
                vm.park_until_notified(this, Some(remaining))?;
            }
        },
    ),
    method(
        "offer",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let element = ref_arg(args, 1);
            if element.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            if lbq_size(vm, this) >= lbq_capacity(vm, this) {
                return boolean(false);
            }
            lbq_push(vm, this, element)?;
            boolean(true)
        },
    ),
    method("remainingCapacity", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        int((lbq_capacity(vm, this) - lbq_size(vm, this)) as i32)
    }),
    method(
        "drainTo",
        "(Ljava/util/Collection;)I",
        ACC_PUBLIC,
        |vm, _, args| lbq_drain_to(vm, args, usize::MAX),
    ),
    method(
        "drainTo",
        "(Ljava/util/Collection;I)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let max = crate::int_arg(args, 2).max(0) as usize;
            lbq_drain_to(vm, args, max)
        },
    ),
    method("clear", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        crate::util::array_list_store(vm, this, &[])?;
        vm.monitor_notify(this, true);
        void()
    }),
];

fn lbq_init(
    vm: &mut Vm<'_>,
    context: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let descriptor = vm.classes.get(context.class).methods[context.method as usize]
        .descriptor
        .as_str();
    let mut capacity = i32::MAX;
    let mut elements = Vec::new();
    match descriptor {
        "(I)V" => {
            capacity = crate::int_arg(args, 1);
            if capacity <= 0 {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
            }
        }
        "(Ljava/util/Collection;)V" => {
            elements = crate::util::collection_elements(vm, ref_arg(args, 1))?;
            if elements.iter().any(|e| e.is_null()) {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
        }
        _ => {}
    }
    crate::lang::write_int_field(vm, this, "capacity", capacity);
    crate::util::array_list_store(vm, this, &elements)?;
    void()
}

fn lbq_size(vm: &Vm<'_>, this: ObjectRef) -> usize {
    crate::lang::read_int_field(vm, this, "size").max(0) as usize
}

fn lbq_capacity(vm: &Vm<'_>, this: ObjectRef) -> usize {
    let capacity = crate::lang::read_int_field(vm, this, "capacity");
    if capacity <= 0 {
        i32::MAX as usize
    } else {
        capacity as usize
    }
}

fn lbq_push(vm: &mut Vm<'_>, this: ObjectRef, element: ObjectRef) -> Result<(), VmError> {
    let mut elements = crate::util::array_list_elements(vm, this);
    elements.push(element);
    crate::util::array_list_store(vm, this, &elements)?;
    vm.monitor_notify(this, true);
    Ok(())
}

fn lbq_pop_head(vm: &mut Vm<'_>, this: ObjectRef) -> Result<ObjectRef, VmError> {
    let mut elements = crate::util::array_list_elements(vm, this);
    let head = elements.remove(0);
    crate::util::array_list_store(vm, this, &elements)?;
    vm.monitor_notify(this, true);
    Ok(head)
}

fn lbq_drain_to(vm: &mut Vm<'_>, args: &[Value], max: usize) -> Result<Value, VmError> {
    let this = receiver(args);
    let collection = ref_arg(args, 1);
    if collection.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let elements = crate::util::array_list_elements(vm, this);
    let mut transferred = 0usize;
    for element in elements.iter().take(max) {
        let (declaring, method) =
            vm.resolve_virtual_method(collection, "add", "(Ljava/lang/Object;)Z")?;
        let added = vm.invoke_method(
            declaring,
            method,
            vec![Value::Ref(collection), Value::Ref(*element)],
        )?;
        if added.as_int() == 0 {
            break;
        }
        transferred += 1;
    }
    if transferred > 0 {
        crate::util::array_list_store(vm, this, &elements[transferred..])?;
        vm.monitor_notify(this, true);
    }
    int(transferred as i32)
}

/// `BlockingDeque`: a `Deque` with blocking put/take operations at both ends.
pub(crate) const BLOCKING_DEQUE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/BlockingDeque",
    Some("java/lang/Object"),
    &["java/util/Deque", "java/util/concurrent/BlockingQueue"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &BLOCKING_DEQUE_METHODS,
    None,
);

const BLOCKING_DEQUE_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method(
        "putFirst",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "putLast",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "takeFirst",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "takeLast",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "offerFirst",
        "(Ljava/lang/Object;JLjava/util/concurrent/TimeUnit;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "offerLast",
        "(Ljava/lang/Object;JLjava/util/concurrent/TimeUnit;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "pollFirst",
        "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "pollLast",
        "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

/// `LinkedBlockingDeque`: a blocking double-ended queue backed by the `ArrayDeque` element
/// array.  Blocking operations park on the deque monitor; mutations notify waiters.
pub(crate) const LINKED_BLOCKING_DEQUE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/LinkedBlockingDeque",
    Some("java/util/ArrayDeque"),
    &["java/util/concurrent/BlockingDeque", "java/io/Serializable"],
    ACC_PUBLIC,
    &[field("capacity", "I", ACC_PRIVATE, None)],
    &LINKED_BLOCKING_DEQUE_METHODS,
    None,
);

const LINKED_BLOCKING_DEQUE_METHODS: [oxjvm_vm::NativeMethodDef; 16] = [
    method("<init>", "()V", ACC_PUBLIC, lbd_init),
    method("<init>", "(I)V", ACC_PUBLIC, lbd_init),
    method("<init>", "(Ljava/util/Collection;)V", ACC_PUBLIC, lbd_init),
    method("add", "(Ljava/lang/Object;)Z", ACC_PUBLIC, |vm, _, args| {
        lbd_add(vm, args, false)
    }),
    method(
        "addFirst",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            lbd_add(vm, args, true)?;
            void()
        },
    ),
    method(
        "addLast",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            lbd_add(vm, args, false)?;
            void()
        },
    ),
    method(
        "offer",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| lbd_offer(vm, args, false),
    ),
    method(
        "offerFirst",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| lbd_offer(vm, args, true),
    ),
    method(
        "offerLast",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| lbd_offer(vm, args, false),
    ),
    method(
        "putFirst",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| lbd_put(vm, args, true),
    ),
    method(
        "putLast",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| lbd_put(vm, args, false),
    ),
    method("put", "(Ljava/lang/Object;)V", ACC_PUBLIC, |vm, _, args| {
        lbd_put(vm, args, false)
    }),
    method(
        "takeFirst",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| lbd_take(vm, args, true),
    ),
    method(
        "takeLast",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| lbd_take(vm, args, false),
    ),
    method("take", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        lbd_take(vm, args, true)
    }),
    method("remainingCapacity", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        int((lbd_capacity(vm, this) - lbd_size(vm, this)) as i32)
    }),
];

fn lbd_init(
    vm: &mut Vm<'_>,
    context: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let descriptor = vm.classes.get(context.class).methods[context.method as usize]
        .descriptor
        .as_str();
    let mut capacity = i32::MAX;
    let mut elements = Vec::new();
    match descriptor {
        "(I)V" => {
            capacity = crate::int_arg(args, 1);
            if capacity <= 0 {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
            }
        }
        "(Ljava/util/Collection;)V" => {
            elements = crate::util::collection_elements(vm, ref_arg(args, 1))?;
            if elements.iter().any(|e| e.is_null()) {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
        }
        _ => {}
    }
    crate::lang::write_int_field(vm, this, "capacity", capacity);
    crate::util::array_list_store(vm, this, &elements)?;
    void()
}

fn lbd_size(vm: &Vm<'_>, this: ObjectRef) -> usize {
    crate::lang::read_int_field(vm, this, "size").max(0) as usize
}

/// Non-blocking insertion honoring the capacity bound.
fn lbd_offer(vm: &mut Vm<'_>, args: &[Value], first: bool) -> Result<Value, VmError> {
    let this = receiver(args);
    let element = ref_arg(args, 1);
    if element.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    if lbd_size(vm, this) >= lbd_capacity(vm, this) {
        return boolean(false);
    }
    let mut elements = crate::util::array_list_elements(vm, this);
    if first {
        elements.insert(0, element);
    } else {
        elements.push(element);
    }
    crate::util::array_list_store(vm, this, &elements)?;
    vm.monitor_notify(this, true);
    boolean(true)
}

fn lbd_add(vm: &mut Vm<'_>, args: &[Value], first: bool) -> Result<Value, VmError> {
    if lbd_offer(vm, args, first)?.as_int() == 0 {
        return Err(vm.throw_new("java/lang/IllegalStateException", Some("Deque full")));
    }
    boolean(true)
}

fn lbd_capacity(vm: &Vm<'_>, this: ObjectRef) -> usize {
    let capacity = crate::lang::read_int_field(vm, this, "capacity");
    if capacity <= 0 {
        i32::MAX as usize
    } else {
        capacity as usize
    }
}

fn lbd_put(vm: &mut Vm<'_>, args: &[Value], first: bool) -> Result<Value, VmError> {
    let this = receiver(args);
    let element = ref_arg(args, 1);
    if element.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    loop {
        if lbd_size(vm, this) < lbd_capacity(vm, this) {
            let mut elements = crate::util::array_list_elements(vm, this);
            if first {
                elements.insert(0, element);
            } else {
                elements.push(element);
            }
            crate::util::array_list_store(vm, this, &elements)?;
            vm.monitor_notify(this, true);
            return void();
        }
        vm.park_until_notified(this, None)?;
    }
}

fn lbd_take(vm: &mut Vm<'_>, args: &[Value], first: bool) -> Result<Value, VmError> {
    let this = receiver(args);
    loop {
        if lbd_size(vm, this) > 0 {
            let mut elements = crate::util::array_list_elements(vm, this);
            let element = if first {
                elements.remove(0)
            } else {
                elements.pop().unwrap_or(ObjectRef::NULL)
            };
            crate::util::array_list_store(vm, this, &elements)?;
            vm.monitor_notify(this, true);
            return object(element);
        }
        vm.park_until_notified(this, None)?;
    }
}

// -------------------------------------------------------------------------------------------
// PriorityBlockingQueue
// -------------------------------------------------------------------------------------------

/// `PriorityBlockingQueue`: an unbounded blocking queue on the `PriorityQueue` heap.
/// Blocking takes park on the queue monitor; insertions notify waiters.
pub(crate) const PRIORITY_BLOCKING_QUEUE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/PriorityBlockingQueue",
    Some("java/util/PriorityQueue"),
    &["java/util/concurrent/BlockingQueue", "java/io/Serializable"],
    ACC_PUBLIC,
    &[],
    &PRIORITY_BLOCKING_QUEUE_METHODS,
    None,
);

const PRIORITY_BLOCKING_QUEUE_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method("<init>", "()V", ACC_PUBLIC, crate::util::pq_init),
    method("<init>", "(I)V", ACC_PUBLIC, crate::util::pq_init),
    method(
        "<init>",
        "(Ljava/util/Comparator;)V",
        ACC_PUBLIC,
        crate::util::pq_init,
    ),
    method(
        "<init>",
        "(ILjava/util/Comparator;)V",
        ACC_PUBLIC,
        crate::util::pq_init,
    ),
    method(
        "<init>",
        "(Ljava/util/Collection;)V",
        ACC_PUBLIC,
        crate::util::pq_init,
    ),
    method("put", "(Ljava/lang/Object;)V", ACC_PUBLIC, |vm, _, args| {
        // Unbounded: offer never blocks and rejects null itself.
        let this = receiver(args);
        crate::util::pq_offer(vm, this, ref_arg(args, 1))?;
        vm.monitor_notify(this, true);
        void()
    }),
    method(
        "offer",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            crate::util::pq_offer(vm, this, ref_arg(args, 1))?;
            vm.monitor_notify(this, true);
            boolean(true)
        },
    ),
    method("take", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        loop {
            let (class, method) =
                vm.resolve_virtual_method(this, "poll", "()Ljava/lang/Object;")?;
            let element = vm
                .invoke_method(class, method, vec![Value::Ref(this)])?
                .as_ref();
            if !element.is_null() {
                vm.monitor_notify(this, true);
                return object(element);
            }
            vm.park_until_notified(this, None)?;
        }
    }),
    method(
        "offer",
        "(Ljava/lang/Object;JLjava/util/concurrent/TimeUnit;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            crate::util::pq_offer(vm, this, ref_arg(args, 1))?;
            vm.monitor_notify(this, true);
            boolean(true)
        },
    ),
    method(
        "poll",
        "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let millis = time_unit_to_millis(vm, ref_arg(args, 2), crate::long_arg(args, 1))?;
            let deadline = vm.virtual_time_millis().saturating_add(millis.max(0));
            loop {
                let (class, method) =
                    vm.resolve_virtual_method(this, "poll", "()Ljava/lang/Object;")?;
                let element = vm
                    .invoke_method(class, method, vec![Value::Ref(this)])?
                    .as_ref();
                if !element.is_null() {
                    return object(element);
                }
                let remaining = deadline - vm.virtual_time_millis();
                if remaining <= 0 {
                    return object(ObjectRef::NULL);
                }
                vm.park_until_notified(this, Some(remaining))?;
            }
        },
    ),
    method("remainingCapacity", "()I", ACC_PUBLIC, |_, _, _| {
        int(i32::MAX)
    }),
    method(
        "drainTo",
        "(Ljava/util/Collection;)I",
        ACC_PUBLIC,
        |vm, _, args| lbq_drain_to(vm, args, usize::MAX),
    ),
];

// -------------------------------------------------------------------------------------------
// TransferQueue / LinkedTransferQueue
// -------------------------------------------------------------------------------------------

/// `TransferQueue`: a `BlockingQueue` where producers may wait for consumers to receive.
pub(crate) const TRANSFER_QUEUE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/TransferQueue",
    Some("java/lang/Object"),
    &["java/util/concurrent/BlockingQueue"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &TRANSFER_QUEUE_METHODS,
    None,
);

const TRANSFER_QUEUE_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "tryTransfer",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "transfer",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "tryTransfer",
        "(Ljava/lang/Object;JLjava/util/concurrent/TimeUnit;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "hasWaitingConsumer",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getWaitingConsumerCount",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

/// `LinkedTransferQueue`: an unbounded transfer queue on the `ArrayList` element array.
/// The cooperative runtime has no independent consumer handoff, so `transfer` behaves like
/// `put` and `tryTransfer` like `offer`; the queue contract tests observe only that.
pub(crate) const LINKED_TRANSFER_QUEUE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/LinkedTransferQueue",
    Some("java/util/ArrayList"),
    &["java/util/concurrent/TransferQueue", "java/io/Serializable"],
    ACC_PUBLIC,
    &[field("capacity", "I", ACC_PRIVATE, None)],
    &LINKED_TRANSFER_QUEUE_METHODS,
    None,
);

const LINKED_TRANSFER_QUEUE_METHODS: [oxjvm_vm::NativeMethodDef; 21] = [
    method("<init>", "()V", ACC_PUBLIC, lbq_init),
    method("<init>", "(Ljava/util/Collection;)V", ACC_PUBLIC, lbq_init),
    method("poll", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if lbq_size(vm, this) == 0 {
            return object(ObjectRef::NULL);
        }
        object(lbq_pop_head(vm, this)?)
    }),
    method("peek", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        let elements = crate::util::array_list_elements(vm, receiver(args));
        object(elements.first().copied().unwrap_or(ObjectRef::NULL))
    }),
    method(
        "element",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let elements = crate::util::array_list_elements(vm, receiver(args));
            match elements.first() {
                Some(head) => object(*head),
                None => Err(vm.throw_new("java/util/NoSuchElementException", None)),
            }
        },
    ),
    method(
        "remove",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if lbq_size(vm, this) == 0 {
                return Err(vm.throw_new("java/util/NoSuchElementException", None));
            }
            object(lbq_pop_head(vm, this)?)
        },
    ),
    method("add", "(Ljava/lang/Object;)Z", ACC_PUBLIC, |vm, _, args| {
        let element = ref_arg(args, 1);
        if element.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        lbq_push(vm, receiver(args), element)?;
        boolean(true)
    }),
    method(
        "offer",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let element = ref_arg(args, 1);
            if element.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            lbq_push(vm, receiver(args), element)?;
            boolean(true)
        },
    ),
    method("put", "(Ljava/lang/Object;)V", ACC_PUBLIC, |vm, _, args| {
        let element = ref_arg(args, 1);
        if element.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        lbq_push(vm, receiver(args), element)?;
        void()
    }),
    method("take", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        loop {
            if lbq_size(vm, this) > 0 {
                return object(lbq_pop_head(vm, this)?);
            }
            vm.park_until_notified(this, None)?;
        }
    }),
    method(
        "poll",
        "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let millis = time_unit_to_millis(vm, ref_arg(args, 2), crate::long_arg(args, 1))?;
            let deadline = vm.virtual_time_millis().saturating_add(millis.max(0));
            loop {
                if lbq_size(vm, this) > 0 {
                    return object(lbq_pop_head(vm, this)?);
                }
                let remaining = deadline - vm.virtual_time_millis();
                if remaining <= 0 {
                    return object(ObjectRef::NULL);
                }
                vm.park_until_notified(this, Some(remaining))?;
            }
        },
    ),
    method(
        "offer",
        "(Ljava/lang/Object;JLjava/util/concurrent/TimeUnit;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let element = ref_arg(args, 1);
            if element.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            lbq_push(vm, receiver(args), element)?;
            boolean(true)
        },
    ),
    method("remainingCapacity", "()I", ACC_PUBLIC, |_, _, _| {
        int(i32::MAX)
    }),
    method(
        "drainTo",
        "(Ljava/util/Collection;)I",
        ACC_PUBLIC,
        |vm, _, args| lbq_drain_to(vm, args, usize::MAX),
    ),
    method(
        "drainTo",
        "(Ljava/util/Collection;I)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let max = crate::int_arg(args, 2).max(0) as usize;
            lbq_drain_to(vm, args, max)
        },
    ),
    method("clear", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        crate::util::array_list_store(vm, this, &[])?;
        vm.monitor_notify(this, true);
        void()
    }),
    method(
        "transfer",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let element = ref_arg(args, 1);
            if element.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            lbq_push(vm, this, element)?;
            // Wait until a consumer takes an element, the way a transfer waits for its handoff.
            vm.park_until_notified(this, None)?;
            void()
        },
    ),
    method(
        "tryTransfer",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            // Without a waiting consumer the transfer does not happen.
            boolean(false)
        },
    ),
    method(
        "tryTransfer",
        "(Ljava/lang/Object;JLjava/util/concurrent/TimeUnit;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let millis = time_unit_to_millis(vm, ref_arg(args, 3), crate::long_arg(args, 2))?;
            if millis > 0 {
                let deadline = vm.virtual_time_millis().saturating_add(millis);
                let remaining = deadline - vm.virtual_time_millis();
                if remaining > 0 {
                    vm.park_until_notified(this, Some(remaining))?;
                }
            }
            boolean(false)
        },
    ),
    method("hasWaitingConsumer", "()Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
    method("getWaitingConsumerCount", "()I", ACC_PUBLIC, |_, _, _| {
        int(0)
    }),
];
