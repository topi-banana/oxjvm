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
fn reentrant_lock_acquire(vm: &mut Vm<'_>, lock: ObjectRef) {
    let current = vm.current_thread_object();
    loop {
        let held = crate::lang::read_int_field(vm, lock, "holdCount");
        if held == 0 || lock_owner(vm, lock) == current {
            crate::lang::write_int_field(vm, lock, "holdCount", held + 1);
            set_lock_owner(vm, lock, current);
            return;
        }
        vm.park_until_notified(lock, None);
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
    reentrant_lock_acquire(vm, this);
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
        vm.park_until_notified(this, timeout);
        return Ok(());
    }
    let held = crate::lang::read_int_field(vm, lock, "holdCount");
    crate::lang::write_int_field(vm, lock, "holdCount", 0);
    set_lock_owner(vm, lock, ObjectRef::NULL);
    if held > 0 {
        vm.monitor_notify(lock, true);
    }
    vm.park_until_notified(this, timeout);
    reentrant_lock_acquire(vm, lock);
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

fn new_noop_lock(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/concurrent/locks/ReentrantReadWriteLock$NoopLock")?;
    vm.new_instance(class)
}

const REENTRANT_READ_WRITE_LOCK_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        init_read_write_lock(vm, receiver(args), false)
    }),
    method("<init>", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        init_read_write_lock(vm, receiver(args), crate::bool_arg(args, 1))
    }),
    method(
        "readLock",
        "()Ljava/util/concurrent/locks/Lock;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let lock = vm
                .read_ref_field(this, class, "readLock", "Ljava/util/concurrent/locks/Lock;")
                .unwrap_or(ObjectRef::NULL);
            object(lock)
        },
    ),
    method(
        "writeLock",
        "()Ljava/util/concurrent/locks/Lock;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let lock = vm
                .read_ref_field(
                    this,
                    class,
                    "writeLock",
                    "Ljava/util/concurrent/locks/Lock;",
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
    method("getReadHoldCount", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method("getWriteHoldCount", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method("getReadLockCount", "()I", ACC_PUBLIC, |_, _, _| int(0)),
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
    let read = new_noop_lock(vm)?;
    let write = new_noop_lock(vm)?;
    let (declaring, field) =
        vm.find_field(class, "readLock", "Ljava/util/concurrent/locks/Lock;")?;
    vm.set_instance_ref(this, declaring, field, read);
    let (declaring, field) =
        vm.find_field(class, "writeLock", "Ljava/util/concurrent/locks/Lock;")?;
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
            "Ljava/util/concurrent/locks/Lock;",
            ACC_PRIVATE,
            None,
        ),
        field(
            "writeLock",
            "Ljava/util/concurrent/locks/Lock;",
            ACC_PRIVATE,
            None,
        ),
        field("fair", "I", ACC_PRIVATE, None),
    ],
    &REENTRANT_READ_WRITE_LOCK_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// CompletableFuture (completed futures only)
// -------------------------------------------------------------------------------------------

const COMPLETABLE_FUTURE_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method(
        "completedFuture",
        "(Ljava/lang/Object;)Ljava/util/concurrent/CompletableFuture;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let class = vm.resolve_class("java/util/concurrent/CompletableFuture")?;
            let instance = vm.new_instance(class)?;
            let (declaring, field) = vm.find_field(class, "result", "Ljava/lang/Object;")?;
            vm.set_instance_ref(instance, declaring, field, ref_arg(args, 0));
            object(instance)
        },
    ),
    method("join", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        future_result(vm, args)
    }),
    method("get", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        future_result(vm, args)
    }),
    method(
        "get",
        "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| future_result(vm, args),
    ),
    method("isDone", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
    method("isCancelled", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("isCompletedExceptionally", "()Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
    method("cancel", "(Z)Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method(
        "complete",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(true),
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("CompletableFuture")?),
    ),
];

fn future_result(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let this = receiver(args);
    let result = vm
        .read_ref_field(this, vm.class_of(this), "result", "Ljava/lang/Object;")
        .unwrap_or(ObjectRef::NULL);
    object(result)
}

/// `CompletableFuture`: only the already-completed form the interpreter needs.
pub(crate) const COMPLETABLE_FUTURE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/CompletableFuture",
    Some("java/lang/Object"),
    &["java/util/concurrent/Future"],
    ACC_PUBLIC,
    &[field("result", "Ljava/lang/Object;", ACC_PRIVATE, None)],
    &COMPLETABLE_FUTURE_METHODS,
    None,
);
