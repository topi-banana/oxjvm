//! `java.util.concurrent.Executors` and a compact, eager executor/future model.
//!
//! The VM's threads are available, but most library tests only need a working
//! `ExecutorService`/`Future` pair.  Tasks therefore run on submission; that
//! preserves the observable completion semantics without a work queue.

use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::{boolean, class, field, int, long, method, object, receiver, ref_arg, void};

fn future_task_new(
    vm: &mut Vm<'_>,
    callable: ObjectRef,
    runnable: ObjectRef,
    result: ObjectRef,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/concurrent/FutureTask")?;
    let task = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "callable", "Ljava/lang/Object;")?;
    vm.set_instance_ref(task, declaring, field, callable);
    let (declaring, field) = vm.find_field(class, "runnable", "Ljava/lang/Object;")?;
    vm.set_instance_ref(task, declaring, field, runnable);
    let (declaring, field) = vm.find_field(class, "result", "Ljava/lang/Object;")?;
    vm.set_instance_ref(task, declaring, field, result);
    let (declaring, field) = vm.find_field(class, "state", "I")?;
    vm.set_instance_int(task, declaring, field, 0);
    Ok(task)
}

fn future_task_run(vm: &mut Vm<'_>, task: ObjectRef) -> Result<(), VmError> {
    let class = vm.class_of(task);
    if vm
        .read_instance_int_named(task, class, "state", "I")
        .unwrap_or(0)
        != 0
    {
        return Ok(());
    }
    let callable = vm
        .read_ref_field(task, class, "callable", "Ljava/lang/Object;")
        .unwrap_or(ObjectRef::NULL);
    let outcome = if !callable.is_null() {
        let (declaring, method) =
            vm.resolve_virtual_method(callable, "call", "()Ljava/lang/Object;")?;
        vm.invoke_method(declaring, method, vec![Value::Ref(callable)])
    } else {
        let runnable = vm
            .read_ref_field(task, class, "runnable", "Ljava/lang/Object;")
            .unwrap_or(ObjectRef::NULL);
        if runnable.is_null() {
            return Ok(());
        }
        let (declaring, method) = vm.resolve_virtual_method(runnable, "run", "()V")?;
        vm.invoke_method(declaring, method, vec![Value::Ref(runnable)])
    };
    match outcome {
        Ok(value) => {
            let result = if callable.is_null() {
                vm.read_ref_field(task, class, "result", "Ljava/lang/Object;")
                    .unwrap_or(ObjectRef::NULL)
            } else {
                value.as_ref()
            };
            let (declaring, method) =
                vm.resolve_virtual_method(task, "set", "(Ljava/lang/Object;)V")?;
            vm.invoke_method(
                declaring,
                method,
                vec![Value::Ref(task), Value::Ref(result)],
            )?;
        }
        Err(VmError::Thrown(exception)) => {
            let (declaring, method) =
                vm.resolve_virtual_method(task, "setException", "(Ljava/lang/Throwable;)V")?;
            vm.invoke_method(
                declaring,
                method,
                vec![Value::Ref(task), Value::Ref(exception)],
            )?;
        }
        Err(error) => return Err(error),
    }
    Ok(())
}

/// Invoke the protected `done()` hook virtually so subclass overrides run and
/// any exception they throw propagates to the caller, as in the JDK.
fn future_task_done(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    let (class, method) = vm.resolve_virtual_method(this, "done", "()V")?;
    vm.invoke_method(class, method, vec![Value::Ref(this)])?;
    Ok(())
}

/// The shared body of `FutureTask.get()` and `get(timeout, unit)`: run a pending task, then
/// report its outcome. A cancelled task throws `CancellationException`; an exceptionally
/// completed task throws `ExecutionException` wrapping the cause; otherwise the result is
/// returned. The timeout is ignored because tasks complete eagerly, but the exception contract
/// must hold even when a getter races the setter (observed once the scheduler preempted threads).
fn future_task_get(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Value, VmError> {
    let class = vm.class_of(this);
    if vm
        .read_instance_int_named(this, class, "state", "I")
        .unwrap_or(0)
        == 0
    {
        future_task_run(vm, this)?;
    }
    let state = vm
        .read_instance_int_named(this, class, "state", "I")
        .unwrap_or(0);
    if state == 2 {
        return Err(vm.throw_new("java/util/concurrent/CancellationException", None));
    }
    let exception = vm
        .read_ref_field(this, class, "exception", "Ljava/lang/Throwable;")
        .unwrap_or(ObjectRef::NULL);
    if !exception.is_null() {
        return Err(vm.throw_new_with_cause("java/util/concurrent/ExecutionException", exception));
    }
    object(
        vm.read_ref_field(this, class, "result", "Ljava/lang/Object;")
            .unwrap_or(ObjectRef::NULL),
    )
}

fn new_executor(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/concurrent/OjvmExecutorService")?;
    vm.new_instance(class)
}

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
        "cancel",
        "(Z)Z",
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
        "isDone",
        "()Z",
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

pub(crate) const RUNNABLE_FUTURE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/RunnableFuture",
    Some("java/lang/Object"),
    &["java/util/concurrent/Future", "java/lang/Runnable"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const SCHEDULED_FUTURE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/ScheduledFuture",
    Some("java/lang/Object"),
    &["java/util/concurrent/Future", "java/lang/Comparable"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

const FUTURE_TASK_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
    method(
        "<init>",
        "(Ljava/util/concurrent/Callable;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let task = future_task_new(vm, ref_arg(args, 1), ObjectRef::NULL, ObjectRef::NULL)?;
            copy_future_task(vm, receiver(args), task)
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/Runnable;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let task = future_task_new(vm, ObjectRef::NULL, ref_arg(args, 1), ObjectRef::NULL)?;
            copy_future_task(vm, receiver(args), task)
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/Runnable;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let task = future_task_new(vm, ObjectRef::NULL, ref_arg(args, 1), ref_arg(args, 2))?;
            copy_future_task(vm, receiver(args), task)
        },
    ),
    method("run", "()V", ACC_PUBLIC, |vm, _, args| {
        future_task_run(vm, receiver(args))?;
        void()
    }),
    method("get", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        future_task_get(vm, receiver(args))
    }),
    method(
        "get",
        "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            if vm
                .read_instance_int_named(this, class, "state", "I")
                .unwrap_or(0)
                == 0
            {
                return Err(vm.throw_new("java/util/concurrent/TimeoutException", None));
            }
            future_task_get(vm, this)
        },
    ),
    method("isDone", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(
            vm.read_instance_int_named(receiver(args), vm.class_of(receiver(args)), "state", "I")
                .unwrap_or(0)
                != 0,
        )
    }),
    method("isCancelled", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(
            vm.read_instance_int_named(receiver(args), vm.class_of(receiver(args)), "state", "I")
                .unwrap_or(0)
                == 2,
        )
    }),
    method("cancel", "(Z)Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        let state = vm
            .read_instance_int_named(this, class, "state", "I")
            .unwrap_or(0);
        if state != 0 {
            return boolean(false);
        }
        let (declaring, field) = vm.find_field(class, "state", "I")?;
        vm.set_instance_int(this, declaring, field, 2);
        future_task_done(vm, this)?;
        boolean(true)
    }),
    method("runAndReset", "()Z", ACC_PROTECTED, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        if vm
            .read_instance_int_named(this, class, "state", "I")
            .unwrap_or(0)
            != 0
        {
            return boolean(false);
        }
        let callable = vm
            .read_ref_field(this, class, "callable", "Ljava/lang/Object;")
            .unwrap_or(ObjectRef::NULL);
        let outcome = if !callable.is_null() {
            let (declaring, method) =
                vm.resolve_virtual_method(callable, "call", "()Ljava/lang/Object;")?;
            vm.invoke_method(declaring, method, vec![Value::Ref(callable)])
        } else {
            let runnable = vm
                .read_ref_field(this, class, "runnable", "Ljava/lang/Object;")
                .unwrap_or(ObjectRef::NULL);
            if runnable.is_null() {
                return boolean(false);
            }
            let (declaring, method) = vm.resolve_virtual_method(runnable, "run", "()V")?;
            vm.invoke_method(declaring, method, vec![Value::Ref(runnable)])
        };
        match outcome {
            Ok(_) => boolean(true),
            Err(VmError::Thrown(exception)) => {
                let (declaring, method) =
                    vm.resolve_virtual_method(this, "setException", "(Ljava/lang/Throwable;)V")?;
                vm.invoke_method(
                    declaring,
                    method,
                    vec![Value::Ref(this), Value::Ref(exception)],
                )?;
                boolean(false)
            }
            Err(error) => Err(error),
        }
    }),
    method("done", "()V", ACC_PROTECTED, |_, _, _| void()),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let state = vm
                .read_instance_int_named(this, vm.class_of(this), "state", "I")
                .unwrap_or(0);
            object(vm.make_string(if state == 1 { "completed" } else { "pending" })?)
        },
    ),
    method(
        "set",
        "(Ljava/lang/Object;)V",
        ACC_PROTECTED,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            if vm
                .read_instance_int_named(this, class, "state", "I")
                .unwrap_or(0)
                != 0
            {
                return void();
            }
            let (declaring, field) = vm.find_field(class, "result", "Ljava/lang/Object;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            let (declaring, field) = vm.find_field(class, "state", "I")?;
            vm.set_instance_int(this, declaring, field, 1);
            future_task_done(vm, this)?;
            void()
        },
    ),
    method(
        "setException",
        "(Ljava/lang/Throwable;)V",
        ACC_PROTECTED,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            if vm
                .read_instance_int_named(this, class, "state", "I")
                .unwrap_or(0)
                != 0
            {
                return void();
            }
            let (declaring, field) = vm.find_field(class, "exception", "Ljava/lang/Throwable;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            let (declaring, field) = vm.find_field(class, "state", "I")?;
            vm.set_instance_int(this, declaring, field, 1);
            future_task_done(vm, this)?;
            void()
        },
    ),
];

fn copy_future_task(
    vm: &mut Vm<'_>,
    target: ObjectRef,
    source: ObjectRef,
) -> Result<Value, VmError> {
    let class = vm.class_of(source);
    for (name, descriptor) in [
        ("callable", "Ljava/lang/Object;"),
        ("runnable", "Ljava/lang/Object;"),
        ("result", "Ljava/lang/Object;"),
    ] {
        if let Some(value) = vm.read_ref_field(source, class, name, descriptor) {
            let (declaring, field) = vm.find_field(vm.class_of(target), name, descriptor)?;
            vm.set_instance_ref(target, declaring, field, value);
        }
    }
    if let Some(state) = vm.read_instance_int_named(source, class, "state", "I") {
        let (declaring, field) = vm.find_field(vm.class_of(target), "state", "I")?;
        vm.set_instance_int(target, declaring, field, state);
    }
    void()
}

pub(crate) const FUTURE_TASK: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/FutureTask",
    Some("java/lang/Object"),
    &[
        "java/util/concurrent/RunnableFuture",
        "java/util/concurrent/Future",
        "java/lang/Runnable",
    ],
    ACC_PUBLIC,
    &[
        field("callable", "Ljava/lang/Object;", ACC_PRIVATE, None),
        field("runnable", "Ljava/lang/Object;", ACC_PRIVATE, None),
        field("result", "Ljava/lang/Object;", ACC_PRIVATE, None),
        field("exception", "Ljava/lang/Throwable;", ACC_PRIVATE, None),
        field("state", "I", ACC_PRIVATE, None),
    ],
    &FUTURE_TASK_METHODS,
    None,
);

const EXECUTOR_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "execute",
    "(Ljava/lang/Runnable;)V",
    ACC_PUBLIC | ACC_ABSTRACT,
    crate::lang::abstract_method_pub,
)];

pub(crate) const EXECUTOR: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/Executor",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &EXECUTOR_METHODS,
    None,
);

const EXECUTOR_SERVICE_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method(
        "shutdown",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "shutdownNow",
        "()Ljava/util/List;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "isShutdown",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "isTerminated",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "awaitTermination",
        "(JLjava/util/concurrent/TimeUnit;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "submit",
        "(Ljava/util/concurrent/Callable;)Ljava/util/concurrent/Future;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "submit",
        "(Ljava/lang/Runnable;)Ljava/util/concurrent/Future;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "submit",
        "(Ljava/lang/Runnable;Ljava/lang/Object;)Ljava/util/concurrent/Future;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "invokeAll",
        "(Ljava/util/Collection;)Ljava/util/List;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "invokeAll",
        "(Ljava/util/Collection;JLjava/util/concurrent/TimeUnit;)Ljava/util/List;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "invokeAny",
        "(Ljava/util/Collection;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "invokeAny",
        "(Ljava/util/Collection;JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const EXECUTOR_SERVICE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/ExecutorService",
    Some("java/lang/Object"),
    &["java/util/concurrent/Executor", "java/lang/AutoCloseable"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &EXECUTOR_SERVICE_METHODS,
    None,
);

const SCHEDULED_EXECUTOR_SERVICE_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "schedule",
        "(Ljava/lang/Runnable;JLjava/util/concurrent/TimeUnit;)Ljava/util/concurrent/ScheduledFuture;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "schedule",
        "(Ljava/util/concurrent/Callable;JLjava/util/concurrent/TimeUnit;)Ljava/util/concurrent/ScheduledFuture;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "scheduleAtFixedRate",
        "(Ljava/lang/Runnable;JJLjava/util/concurrent/TimeUnit;)Ljava/util/concurrent/ScheduledFuture;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "scheduleWithFixedDelay",
        "(Ljava/lang/Runnable;JJLjava/util/concurrent/TimeUnit;)Ljava/util/concurrent/ScheduledFuture;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const SCHEDULED_EXECUTOR_SERVICE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/ScheduledExecutorService",
    Some("java/lang/Object"),
    &["java/util/concurrent/ExecutorService"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &SCHEDULED_EXECUTOR_SERVICE_METHODS,
    None,
);

fn executor_execute(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    runnable: ObjectRef,
) -> Result<Value, VmError> {
    if crate::lang::read_int_field(vm, this, "shutdown") != 0 {
        return Err(vm.throw_new("java/util/concurrent/RejectedExecutionException", None));
    }
    let task = future_task_new(vm, ObjectRef::NULL, runnable, ObjectRef::NULL)?;
    future_task_run(vm, task)?;
    object(task)
}

const OJVM_EXECUTOR_METHODS: [oxjvm_vm::NativeMethodDef; 18] = [
    method(
        "execute",
        "(Ljava/lang/Runnable;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            executor_execute(vm, receiver(args), ref_arg(args, 1))?;
            void()
        },
    ),
    method(
        "submit",
        "(Ljava/util/concurrent/Callable;)Ljava/util/concurrent/Future;",
        ACC_PUBLIC,
        |vm, _, args| {
            let task = future_task_new(vm, ref_arg(args, 1), ObjectRef::NULL, ObjectRef::NULL)?;
            future_task_run(vm, task)?;
            object(task)
        },
    ),
    method(
        "submit",
        "(Ljava/lang/Runnable;)Ljava/util/concurrent/Future;",
        ACC_PUBLIC,
        |vm, _, args| {
            let task = future_task_new(vm, ObjectRef::NULL, ref_arg(args, 1), ObjectRef::NULL)?;
            future_task_run(vm, task)?;
            object(task)
        },
    ),
    method(
        "submit",
        "(Ljava/lang/Runnable;Ljava/lang/Object;)Ljava/util/concurrent/Future;",
        ACC_PUBLIC,
        |vm, _, args| {
            let task = future_task_new(vm, ObjectRef::NULL, ref_arg(args, 1), ref_arg(args, 2))?;
            future_task_run(vm, task)?;
            object(task)
        },
    ),
    method("shutdown", "()V", ACC_PUBLIC, |vm, _, args| {
        crate::lang::write_int_field(vm, receiver(args), "shutdown", 1);
        void()
    }),
    method(
        "shutdownNow",
        "()Ljava/util/List;",
        ACC_PUBLIC,
        |vm, _, _| object(crate::util::array_list_from(vm, &[])?),
    ),
    method("isShutdown", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "shutdown") != 0)
    }),
    method("isTerminated", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "shutdown") != 0)
    }),
    method(
        "awaitTermination",
        "(JLjava/util/concurrent/TimeUnit;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(true),
    ),
    method(
        "invokeAll",
        "(Ljava/util/Collection;)Ljava/util/List;",
        ACC_PUBLIC,
        |vm, _, args| {
            let mut futures = Vec::new();
            for element in crate::util::collection_elements(vm, ref_arg(args, 1))? {
                let task = future_task_new(vm, element, ObjectRef::NULL, ObjectRef::NULL)?;
                future_task_run(vm, task)?;
                futures.push(task);
            }
            object(crate::util::array_list_from(vm, &futures)?)
        },
    ),
    method(
        "invokeAll",
        "(Ljava/util/Collection;JLjava/util/concurrent/TimeUnit;)Ljava/util/List;",
        ACC_PUBLIC,
        |vm, _, args| {
            let mut futures = Vec::new();
            for element in crate::util::collection_elements(vm, ref_arg(args, 1))? {
                let task = future_task_new(vm, element, ObjectRef::NULL, ObjectRef::NULL)?;
                future_task_run(vm, task)?;
                futures.push(task);
            }
            object(crate::util::array_list_from(vm, &futures)?)
        },
    ),
    method(
        "invokeAny",
        "(Ljava/util/Collection;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let elements = crate::util::collection_elements(vm, ref_arg(args, 1))?;
            let Some(callable) = elements.first().copied() else {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
            };
            let (declaring, method) =
                vm.resolve_virtual_method(callable, "call", "()Ljava/lang/Object;")?;
            vm.invoke_method(declaring, method, vec![Value::Ref(callable)])
        },
    ),
    method(
        "invokeAny",
        "(Ljava/util/Collection;JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let elements = crate::util::collection_elements(vm, ref_arg(args, 1))?;
            let Some(callable) = elements.first().copied() else {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
            };
            let (declaring, method) =
                vm.resolve_virtual_method(callable, "call", "()Ljava/lang/Object;")?;
            vm.invoke_method(declaring, method, vec![Value::Ref(callable)])
        },
    ),
    method(
        "schedule",
        "(Ljava/util/concurrent/Callable;JLjava/util/concurrent/TimeUnit;)Ljava/util/concurrent/ScheduledFuture;",
        ACC_PUBLIC,
        |vm, _, args| {
            let task = future_task_new(vm, ref_arg(args, 1), ObjectRef::NULL, ObjectRef::NULL)?;
            future_task_run(vm, task)?;
            object(task)
        },
    ),
    method(
        "schedule",
        "(Ljava/lang/Runnable;JLjava/util/concurrent/TimeUnit;)Ljava/util/concurrent/ScheduledFuture;",
        ACC_PUBLIC,
        |vm, _, args| {
            let task = future_task_new(vm, ObjectRef::NULL, ref_arg(args, 1), ObjectRef::NULL)?;
            future_task_run(vm, task)?;
            object(task)
        },
    ),
    method(
        "scheduleAtFixedRate",
        "(Ljava/lang/Runnable;JJLjava/util/concurrent/TimeUnit;)Ljava/util/concurrent/ScheduledFuture;",
        ACC_PUBLIC,
        |vm, _, args| {
            let task = future_task_new(vm, ObjectRef::NULL, ref_arg(args, 1), ObjectRef::NULL)?;
            future_task_run(vm, task)?;
            object(task)
        },
    ),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        crate::lang::write_int_field(vm, receiver(args), "shutdown", 1);
        void()
    }),
    method(
        "scheduleWithFixedDelay",
        "(Ljava/lang/Runnable;JJLjava/util/concurrent/TimeUnit;)Ljava/util/concurrent/ScheduledFuture;",
        ACC_PUBLIC,
        |vm, _, args| {
            let task = future_task_new(vm, ObjectRef::NULL, ref_arg(args, 1), ObjectRef::NULL)?;
            future_task_run(vm, task)?;
            object(task)
        },
    ),
];

pub(crate) const OJVM_EXECUTOR_SERVICE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/OjvmExecutorService",
    Some("java/lang/Object"),
    &[
        "java/util/concurrent/ExecutorService",
        "java/util/concurrent/ScheduledExecutorService",
        "java/lang/AutoCloseable",
    ],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field("shutdown", "I", ACC_PRIVATE, None)],
    &OJVM_EXECUTOR_METHODS,
    None,
);

/// `AbstractExecutorService`: eager single-threaded execution of submitted tasks.
pub(crate) const ABSTRACT_EXECUTOR_SERVICE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/AbstractExecutorService",
    Some("java/lang/Object"),
    &["java/util/concurrent/ExecutorService"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[field("shutdown", "I", ACC_PRIVATE, None)],
    &OJVM_EXECUTOR_METHODS,
    None,
);

fn thread_pool_executor_init(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    core: i32,
    max: i32,
    keep_alive: i64,
    queue: ObjectRef,
) -> Result<Value, VmError> {
    if core < 0 || max <= 0 || max < core || keep_alive < 0 {
        return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
    }
    if queue.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    crate::lang::write_int_field(vm, this, "corePoolSize", core);
    crate::lang::write_int_field(vm, this, "maximumPoolSize", max);
    crate::lang::write_long_field(vm, this, "keepAliveTime", keep_alive);
    let class = vm.class_of(this);
    let (declaring, field) =
        vm.find_field(class, "queue", "Ljava/util/concurrent/BlockingQueue;")?;
    vm.set_instance_ref(this, declaring, field, queue);
    void()
}

const THREAD_POOL_EXECUTOR_METHODS: [oxjvm_vm::NativeMethodDef; 21] = [
    method(
        "<init>",
        "(IIJLjava/util/concurrent/TimeUnit;Ljava/util/concurrent/BlockingQueue;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            thread_pool_executor_init(
                vm,
                receiver(args),
                crate::int_arg(args, 1),
                crate::int_arg(args, 2),
                crate::long_arg(args, 3),
                ref_arg(args, 5),
            )
        },
    ),
    method(
        "<init>",
        "(IIJLjava/util/concurrent/TimeUnit;Ljava/util/concurrent/BlockingQueue;Ljava/util/concurrent/ThreadFactory;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            thread_pool_executor_init(
                vm,
                receiver(args),
                crate::int_arg(args, 1),
                crate::int_arg(args, 2),
                crate::long_arg(args, 3),
                ref_arg(args, 5),
            )
        },
    ),
    method(
        "<init>",
        "(IIJLjava/util/concurrent/TimeUnit;Ljava/util/concurrent/BlockingQueue;Ljava/util/concurrent/RejectedExecutionHandler;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            thread_pool_executor_init(
                vm,
                receiver(args),
                crate::int_arg(args, 1),
                crate::int_arg(args, 2),
                crate::long_arg(args, 3),
                ref_arg(args, 5),
            )
        },
    ),
    method(
        "<init>",
        "(IIJLjava/util/concurrent/TimeUnit;Ljava/util/concurrent/BlockingQueue;Ljava/util/concurrent/ThreadFactory;Ljava/util/concurrent/RejectedExecutionHandler;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            thread_pool_executor_init(
                vm,
                receiver(args),
                crate::int_arg(args, 1),
                crate::int_arg(args, 2),
                crate::long_arg(args, 3),
                ref_arg(args, 5),
            )
        },
    ),
    method("getCorePoolSize", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(
            vm,
            receiver(args),
            "corePoolSize",
        ))
    }),
    method("getMaximumPoolSize", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(
            vm,
            receiver(args),
            "maximumPoolSize",
        ))
    }),
    method("getPoolSize", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(
            vm,
            receiver(args),
            "corePoolSize",
        ))
    }),
    method("getActiveCount", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method("getLargestPoolSize", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(
            vm,
            receiver(args),
            "corePoolSize",
        ))
    }),
    method("getTaskCount", "()J", ACC_PUBLIC, |_, _, _| long(0)),
    method("getCompletedTaskCount", "()J", ACC_PUBLIC, |_, _, _| {
        long(0)
    }),
    method(
        "getQueue",
        "()Ljava/util/concurrent/BlockingQueue;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            object(
                vm.read_ref_field(this, class, "queue", "Ljava/util/concurrent/BlockingQueue;")
                    .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method(
        "getKeepAliveTime",
        "(Ljava/util/concurrent/TimeUnit;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            long(crate::lang::read_long_field(
                vm,
                receiver(args),
                "keepAliveTime",
            ))
        },
    ),
    method("setCorePoolSize", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let core = crate::int_arg(args, 1);
        if core < 0 {
            return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
        }
        crate::lang::write_int_field(vm, receiver(args), "corePoolSize", core);
        void()
    }),
    method("setMaximumPoolSize", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let max = crate::int_arg(args, 1);
        let this = receiver(args);
        if max <= 0 || max < crate::lang::read_int_field(vm, this, "corePoolSize") {
            return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
        }
        crate::lang::write_int_field(vm, this, "maximumPoolSize", max);
        void()
    }),
    method(
        "setKeepAliveTime",
        "(JLjava/util/concurrent/TimeUnit;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            crate::lang::write_long_field(
                vm,
                receiver(args),
                "keepAliveTime",
                crate::long_arg(args, 1),
            );
            void()
        },
    ),
    method(
        "allowCoreThreadTimeOut",
        "(Z)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = crate::int_arg(args, 1) != 0;
            let this = receiver(args);
            if value && crate::lang::read_long_field(vm, this, "keepAliveTime") <= 0 {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
            }
            crate::lang::write_int_field(vm, this, "allowCoreThreadTimeOut", i32::from(value));
            void()
        },
    ),
    method(
        "allowsCoreThreadTimeOut",
        "()Z",
        ACC_PUBLIC,
        |vm, _, args| {
            boolean(crate::lang::read_int_field(vm, receiver(args), "allowCoreThreadTimeOut") != 0)
        },
    ),
    method("prestartCoreThread", "()Z", ACC_PUBLIC, |_, _, _| {
        boolean(true)
    }),
    method(
        "prestartAllCoreThreads",
        "()I",
        ACC_PUBLIC,
        |vm, _, args| {
            int(crate::lang::read_int_field(
                vm,
                receiver(args),
                "corePoolSize",
            ))
        },
    ),
    method("purge", "()V", ACC_PUBLIC, |_, _, _| void()),
];

/// `ThreadPoolExecutor`: eager execution with pool-size bookkeeping.
pub(crate) const THREAD_POOL_EXECUTOR: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/ThreadPoolExecutor",
    Some("java/util/concurrent/AbstractExecutorService"),
    &[],
    ACC_PUBLIC,
    &[
        field("corePoolSize", "I", ACC_PRIVATE, None),
        field("maximumPoolSize", "I", ACC_PRIVATE, None),
        field("keepAliveTime", "J", ACC_PRIVATE, None),
        field("allowCoreThreadTimeOut", "I", ACC_PRIVATE, None),
        field(
            "queue",
            "Ljava/util/concurrent/BlockingQueue;",
            ACC_PRIVATE,
            None,
        ),
    ],
    &THREAD_POOL_EXECUTOR_METHODS,
    None,
);

/// Create a `ForkJoinTask` carrying a callable or runnable, run eagerly like `FutureTask`.
fn fork_join_task_new(
    vm: &mut Vm<'_>,
    callable: ObjectRef,
    runnable: ObjectRef,
    result: ObjectRef,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/concurrent/ForkJoinTask")?;
    let task = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "callable", "Ljava/lang/Object;")?;
    vm.set_instance_ref(task, declaring, field, callable);
    let (declaring, field) = vm.find_field(class, "runnable", "Ljava/lang/Object;")?;
    vm.set_instance_ref(task, declaring, field, runnable);
    let (declaring, field) = vm.find_field(class, "result", "Ljava/lang/Object;")?;
    vm.set_instance_ref(task, declaring, field, result);
    let (declaring, field) = vm.find_field(class, "state", "I")?;
    vm.set_instance_int(task, declaring, field, 0);
    Ok(task)
}

const FORK_JOIN_TASK_METHODS: [oxjvm_vm::NativeMethodDef; 13] = [
    method(
        "fork",
        "()Ljava/util/concurrent/ForkJoinTask;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            future_task_run(vm, this)?;
            object(this)
        },
    ),
    method("join", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        future_task_get(vm, receiver(args))
    }),
    method("get", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        future_task_get(vm, receiver(args))
    }),
    method(
        "get",
        "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| future_task_get(vm, receiver(args)),
    ),
    method("quietlyJoin", "()V", ACC_PUBLIC, |vm, _, args| {
        future_task_get(vm, receiver(args))?;
        void()
    }),
    method(
        "invoke",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| future_task_get(vm, receiver(args)),
    ),
    method("isDone", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "state") != 0)
    }),
    method("isCancelled", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("isCompletedAbnormally", "()Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
    method("cancel", "(Z)Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method(
        "adapt",
        "(Ljava/lang/Runnable;)Ljava/util/concurrent/ForkJoinTask;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            object(fork_join_task_new(
                vm,
                ObjectRef::NULL,
                ref_arg(args, 0),
                ObjectRef::NULL,
            )?)
        },
    ),
    method(
        "adapt",
        "(Ljava/util/concurrent/Callable;)Ljava/util/concurrent/ForkJoinTask;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            object(fork_join_task_new(
                vm,
                ref_arg(args, 0),
                ObjectRef::NULL,
                ObjectRef::NULL,
            )?)
        },
    ),
    method(
        "adapt",
        "(Ljava/lang/Runnable;Ljava/lang/Object;)Ljava/util/concurrent/ForkJoinTask;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            object(fork_join_task_new(
                vm,
                ObjectRef::NULL,
                ref_arg(args, 0),
                ref_arg(args, 1),
            )?)
        },
    ),
];

/// `java.util.concurrent.ForkJoinTask`: the eager future the fork/join pool hands out. It
/// shares `FutureTask`'s field layout so the shared run/get helpers apply.
pub(crate) const FORK_JOIN_TASK: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/ForkJoinTask",
    Some("java/lang/Object"),
    &["java/util/concurrent/Future", "java/io/Serializable"],
    ACC_PUBLIC,
    &[
        field("callable", "Ljava/lang/Object;", ACC_PRIVATE, None),
        field("runnable", "Ljava/lang/Object;", ACC_PRIVATE, None),
        field("result", "Ljava/lang/Object;", ACC_PRIVATE, None),
        field("state", "I", ACC_PRIVATE, None),
    ],
    &FORK_JOIN_TASK_METHODS,
    None,
);

const FORK_JOIN_POOL_METHODS: [oxjvm_vm::NativeMethodDef; 23] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        crate::lang::write_int_field(vm, receiver(args), "parallelism", 1);
        void()
    }),
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let parallelism = crate::int_arg(args, 1).max(1);
        crate::lang::write_int_field(vm, receiver(args), "parallelism", parallelism);
        void()
    }),
    method(
        "commonPool",
        "()Ljava/util/concurrent/ForkJoinPool;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/concurrent/ForkJoinPool")?;
            if let Some(existing) =
                vm.read_static_ref_if_set(class, "common", "Ljava/util/concurrent/ForkJoinPool;")
            {
                if !existing.is_null() {
                    return object(existing);
                }
            }
            let instance = vm.new_instance(class)?;
            crate::lang::write_int_field(vm, instance, "parallelism", 1);
            vm.set_static_value(
                class,
                "common",
                "Ljava/util/concurrent/ForkJoinPool;",
                Value::Ref(instance),
            );
            object(instance)
        },
    ),
    method(
        "getCommonPoolParallelism",
        "()I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| int(1),
    ),
    method("getParallelism", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "parallelism").max(1))
    }),
    method("getPoolSize", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method("getActiveThreadCount", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method("getRunningThreadCount", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method("getQueuedTaskCount", "()J", ACC_PUBLIC, |_, _, _| long(0)),
    method("getDelayedTaskCount", "()J", ACC_PUBLIC, |_, _, _| long(0)),
    method("getStealCount", "()J", ACC_PUBLIC, |_, _, _| long(0)),
    method("isQuiescent", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
    method(
        "execute",
        "(Ljava/lang/Runnable;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            executor_execute(vm, receiver(args), ref_arg(args, 1))?;
            void()
        },
    ),
    method(
        "submit",
        "(Ljava/util/concurrent/Callable;)Ljava/util/concurrent/ForkJoinTask;",
        ACC_PUBLIC,
        |vm, _, args| {
            let task = fork_join_task_new(vm, ref_arg(args, 1), ObjectRef::NULL, ObjectRef::NULL)?;
            future_task_run(vm, task)?;
            object(task)
        },
    ),
    method(
        "submit",
        "(Ljava/lang/Runnable;)Ljava/util/concurrent/ForkJoinTask;",
        ACC_PUBLIC,
        |vm, _, args| {
            let task = fork_join_task_new(vm, ObjectRef::NULL, ref_arg(args, 1), ObjectRef::NULL)?;
            future_task_run(vm, task)?;
            object(task)
        },
    ),
    method(
        "submit",
        "(Ljava/lang/Runnable;Ljava/lang/Object;)Ljava/util/concurrent/ForkJoinTask;",
        ACC_PUBLIC,
        |vm, _, args| {
            let task = fork_join_task_new(vm, ObjectRef::NULL, ref_arg(args, 1), ref_arg(args, 2))?;
            future_task_run(vm, task)?;
            object(task)
        },
    ),
    method(
        "invoke",
        "(Ljava/util/concurrent/ForkJoinTask;)Ljava/util/concurrent/ForkJoinTask;",
        ACC_PUBLIC,
        |vm, _, args| {
            let task = ref_arg(args, 1);
            future_task_run(vm, task)?;
            object(task)
        },
    ),
    method("shutdown", "()V", ACC_PUBLIC, |vm, _, args| {
        crate::lang::write_int_field(vm, receiver(args), "shutdown", 1);
        void()
    }),
    method(
        "shutdownNow",
        "()Ljava/util/List;",
        ACC_PUBLIC,
        |vm, _, args| {
            crate::lang::write_int_field(vm, receiver(args), "shutdown", 1);
            object(crate::util::new_array_list(vm)?)
        },
    ),
    method("isShutdown", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "shutdown") != 0)
    }),
    method("isTerminated", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "shutdown") != 0)
    }),
    method(
        "awaitTermination",
        "(JLjava/util/concurrent/TimeUnit;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(true),
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("java.util.concurrent.ForkJoinPool")?),
    ),
];

/// `java.util.concurrent.ForkJoinPool`: backed by the eager executor model, so tasks run on
/// submission. `commonPool()` hands out a shared singleton, which is what `CompletableFuture`
/// and most `ExecutorService` callers need.
pub(crate) const FORK_JOIN_POOL: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/ForkJoinPool",
    Some("java/lang/Object"),
    &["java/util/concurrent/ExecutorService"],
    ACC_PUBLIC,
    &[
        field("parallelism", "I", ACC_PRIVATE, None),
        field("shutdown", "I", ACC_PRIVATE, None),
        field(
            "common",
            "Ljava/util/concurrent/ForkJoinPool;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
    ],
    &FORK_JOIN_POOL_METHODS,
    None,
);

const EXECUTORS_METHODS: [oxjvm_vm::NativeMethodDef; 21] = [
    method(
        "newSingleThreadExecutor",
        "()Ljava/util/concurrent/ExecutorService;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_executor(vm)?),
    ),
    method(
        "newSingleThreadExecutor",
        "(Ljava/util/concurrent/ThreadFactory;)Ljava/util/concurrent/ExecutorService;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(new_executor(vm)?)
        },
    ),
    method(
        "newFixedThreadPool",
        "(I)Ljava/util/concurrent/ExecutorService;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if crate::int_arg(args, 0) <= 0 {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
            }
            object(new_executor(vm)?)
        },
    ),
    method(
        "newFixedThreadPool",
        "(ILjava/util/concurrent/ThreadFactory;)Ljava/util/concurrent/ExecutorService;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            if crate::int_arg(args, 0) <= 0 {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
            }
            object(new_executor(vm)?)
        },
    ),
    method(
        "newCachedThreadPool",
        "()Ljava/util/concurrent/ExecutorService;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_executor(vm)?),
    ),
    method(
        "newCachedThreadPool",
        "(Ljava/util/concurrent/ThreadFactory;)Ljava/util/concurrent/ExecutorService;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(new_executor(vm)?)
        },
    ),
    method(
        "newScheduledThreadPool",
        "(I)Ljava/util/concurrent/ScheduledExecutorService;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if crate::int_arg(args, 0) < 0 {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
            }
            object(new_executor(vm)?)
        },
    ),
    method(
        "newScheduledThreadPool",
        "(ILjava/util/concurrent/ThreadFactory;)Ljava/util/concurrent/ScheduledExecutorService;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            if crate::int_arg(args, 0) < 0 {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
            }
            object(new_executor(vm)?)
        },
    ),
    method(
        "newSingleThreadScheduledExecutor",
        "()Ljava/util/concurrent/ScheduledExecutorService;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_executor(vm)?),
    ),
    method(
        "newSingleThreadScheduledExecutor",
        "(Ljava/util/concurrent/ThreadFactory;)Ljava/util/concurrent/ScheduledExecutorService;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(new_executor(vm)?)
        },
    ),
    method(
        "newWorkStealingPool",
        "()Ljava/util/concurrent/ExecutorService;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_executor(vm)?),
    ),
    method(
        "newVirtualThreadPerTaskExecutor",
        "()Ljava/util/concurrent/ExecutorService;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_executor(vm)?),
    ),
    method(
        "newThreadPerTaskExecutor",
        "(Ljava/util/concurrent/ThreadFactory;)Ljava/util/concurrent/ExecutorService;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_executor(vm)?),
    ),
    method(
        "callable",
        "(Ljava/lang/Runnable;)Ljava/util/concurrent/Callable;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let class = vm.resolve_class("java/util/concurrent/OjvmCallable")?;
            let callable = vm.new_instance(class)?;
            let (declaring, field) = vm.find_field(class, "runnable", "Ljava/lang/Runnable;")?;
            vm.set_instance_ref(callable, declaring, field, ref_arg(args, 0));
            object(callable)
        },
    ),
    method(
        "callable",
        "(Ljava/lang/Runnable;Ljava/lang/Object;)Ljava/util/concurrent/Callable;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let class = vm.resolve_class("java/util/concurrent/OjvmCallable")?;
            let callable = vm.new_instance(class)?;
            let (declaring, field) = vm.find_field(class, "runnable", "Ljava/lang/Runnable;")?;
            vm.set_instance_ref(callable, declaring, field, ref_arg(args, 0));
            let (declaring, field) = vm.find_field(class, "result", "Ljava/lang/Object;")?;
            vm.set_instance_ref(callable, declaring, field, ref_arg(args, 1));
            object(callable)
        },
    ),
    method(
        "callable",
        "(Ljava/security/PrivilegedAction;)Ljava/util/concurrent/Callable;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let class = vm.resolve_class("java/util/concurrent/OjvmCallable")?;
            let callable = vm.new_instance(class)?;
            let (declaring, field) = vm.find_field(class, "action", "Ljava/lang/Object;")?;
            vm.set_instance_ref(callable, declaring, field, ref_arg(args, 0));
            object(callable)
        },
    ),
    method(
        "callable",
        "(Ljava/security/PrivilegedExceptionAction;)Ljava/util/concurrent/Callable;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let class = vm.resolve_class("java/util/concurrent/OjvmCallable")?;
            let callable = vm.new_instance(class)?;
            let (declaring, field) = vm.find_field(class, "action", "Ljava/lang/Object;")?;
            vm.set_instance_ref(callable, declaring, field, ref_arg(args, 0));
            object(callable)
        },
    ),
    method(
        "privilegedCallable",
        "(Ljava/util/concurrent/Callable;)Ljava/util/concurrent/Callable;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let class = vm.resolve_class("java/util/concurrent/OjvmCallable")?;
            let callable = vm.new_instance(class)?;
            let (declaring, field) =
                vm.find_field(class, "delegate", "Ljava/util/concurrent/Callable;")?;
            vm.set_instance_ref(callable, declaring, field, ref_arg(args, 0));
            object(callable)
        },
    ),
    method(
        "unconfigurableExecutorService",
        "(Ljava/util/concurrent/ExecutorService;)Ljava/util/concurrent/ExecutorService;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(ref_arg(args, 0))
        },
    ),
    method(
        "unconfigurableScheduledExecutorService",
        "(Ljava/util/concurrent/ScheduledExecutorService;)Ljava/util/concurrent/ScheduledExecutorService;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(ref_arg(args, 0))
        },
    ),
    method(
        "defaultThreadFactory",
        "()Ljava/util/concurrent/ThreadFactory;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/concurrent/OjvmThreadFactory")?;
            object(vm.new_instance(class)?)
        },
    ),
];

pub(crate) const EXECUTORS: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/Executors",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &EXECUTORS_METHODS,
    None,
);

const OJVM_CALLABLE_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "call",
    "()Ljava/lang/Object;",
    ACC_PUBLIC,
    |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        let delegate = vm
            .read_ref_field(this, class, "delegate", "Ljava/util/concurrent/Callable;")
            .unwrap_or(ObjectRef::NULL);
        if !delegate.is_null() {
            let (declaring, method) =
                vm.resolve_virtual_method(delegate, "call", "()Ljava/lang/Object;")?;
            return vm.invoke_method(declaring, method, vec![Value::Ref(delegate)]);
        }
        let action = vm
            .read_ref_field(this, class, "action", "Ljava/lang/Object;")
            .unwrap_or(ObjectRef::NULL);
        if !action.is_null() {
            let (declaring, method) =
                vm.resolve_virtual_method(action, "run", "()Ljava/lang/Object;")?;
            return vm.invoke_method(declaring, method, vec![Value::Ref(action)]);
        }
        let runnable = vm
            .read_ref_field(this, class, "runnable", "Ljava/lang/Runnable;")
            .unwrap_or(ObjectRef::NULL);
        if !runnable.is_null() {
            let (class, method) = vm.resolve_virtual_method(runnable, "run", "()V")?;
            vm.invoke_method(class, method, vec![Value::Ref(runnable)])?;
        }
        object(
            vm.read_ref_field(this, class, "result", "Ljava/lang/Object;")
                .unwrap_or(ObjectRef::NULL),
        )
    },
)];

pub(crate) const OJVM_CALLABLE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/OjvmCallable",
    Some("java/lang/Object"),
    &["java/util/concurrent/Callable"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[
        field("runnable", "Ljava/lang/Runnable;", ACC_PRIVATE, None),
        field("result", "Ljava/lang/Object;", ACC_PRIVATE, None),
        field("action", "Ljava/lang/Object;", ACC_PRIVATE, None),
        field(
            "delegate",
            "Ljava/util/concurrent/Callable;",
            ACC_PRIVATE,
            None,
        ),
    ],
    &OJVM_CALLABLE_METHODS,
    None,
);

const OJVM_THREAD_FACTORY_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "newThread",
    "(Ljava/lang/Runnable;)Ljava/lang/Thread;",
    ACC_PUBLIC,
    |vm, _, args| {
        let class = vm.resolve_class("java/lang/Thread")?;
        let thread = vm.new_instance(class)?;
        let (declaring, field) = vm.find_field(class, "target", "Ljava/lang/Runnable;")?;
        vm.set_instance_ref(thread, declaring, field, ref_arg(args, 1));
        object(thread)
    },
)];

pub(crate) const OJVM_THREAD_FACTORY: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/OjvmThreadFactory",
    Some("java/lang/Object"),
    &["java/util/concurrent/ThreadFactory"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[],
    &OJVM_THREAD_FACTORY_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// ThreadLocalRandom
// -------------------------------------------------------------------------------------------

fn random_next(vm: &mut Vm<'_>) -> u64 {
    vm.host.random_seed()
}

const THREAD_LOCAL_RANDOM_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method(
        "current",
        "()Ljava/util/concurrent/ThreadLocalRandom;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/concurrent/ThreadLocalRandom")?;
            object(vm.new_instance(class)?)
        },
    ),
    method("nextInt", "()I", ACC_PUBLIC, |vm, _, _| {
        int(random_next(vm) as i32)
    }),
    method("nextInt", "(I)I", ACC_PUBLIC, |vm, _, args| {
        let bound = crate::int_arg(args, 1);
        if bound <= 0 {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("bound must be positive"),
            ));
        }
        int((random_next(vm) % bound as u64) as i32)
    }),
    method("nextInt", "(II)I", ACC_PUBLIC, |vm, _, args| {
        let origin = crate::int_arg(args, 1);
        let bound = crate::int_arg(args, 2);
        if origin >= bound {
            return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
        }
        int(origin + (random_next(vm) % (bound - origin) as u64) as i32)
    }),
    method("nextLong", "()J", ACC_PUBLIC, |vm, _, _| {
        long(random_next(vm) as i64)
    }),
    method("nextDouble", "()D", ACC_PUBLIC, |vm, _, _| {
        Ok(Value::Double((random_next(vm) as f64) / (u64::MAX as f64)))
    }),
    method("nextFloat", "()F", ACC_PUBLIC, |vm, _, _| {
        Ok(Value::Float(
            (random_next(vm) % 1_000_000) as f32 / 1_000_000.0,
        ))
    }),
    method("nextBoolean", "()Z", ACC_PUBLIC, |vm, _, _| {
        boolean(random_next(vm) & 1 == 1)
    }),
    method("setSeed", "(J)V", ACC_PUBLIC, |_, _, _| void()),
];

pub(crate) const THREAD_LOCAL_RANDOM: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/ThreadLocalRandom",
    Some("java/util/Random"),
    &[],
    ACC_PUBLIC,
    &[],
    &THREAD_LOCAL_RANDOM_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// SplittableRandom (SplitMix64)
// -------------------------------------------------------------------------------------------

const GOLDEN_GAMMA: i64 = 0x9e37_79b9_7f4a_7c15u64 as i64;

/// Stafford variant 13 of the 64-bit mix function (`SplittableRandom.mix64`).
fn splitmix64_mix64(z: i64) -> i64 {
    let mut z = z as u64;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    (z ^ (z >> 31)) as i64
}

/// Stafford variant 4 mix, high 32 bits (`SplittableRandom.mix32`).
fn splitmix64_mix32(z: i64) -> i32 {
    let mut z = z as u64;
    z = (z ^ (z >> 33)).wrapping_mul(0x62a9_d9ed_7997_05f5);
    (((z ^ (z >> 28)).wrapping_mul(0xcb24_d0a5_c88c_35b3)) >> 32) as i32
}

/// The gamma for a split generator (`SplittableRandom.mixGamma`).
fn splitmix64_mix_gamma(z: i64) -> i64 {
    let mut z = z as u64;
    z = (z ^ (z >> 33)).wrapping_mul(0xff51_afd7_ed55_8ccd);
    z = (z ^ (z >> 33)).wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    z = (z ^ (z >> 33)) | 1;
    let n = (z ^ (z >> 1)).count_ones();
    if n < 24 {
        (z ^ 0xaaaa_aaaa_aaaa_aaaa) as i64
    } else {
        z as i64
    }
}

/// `nextSeed`: advance the seed by the gamma and return it.
fn splittable_next_seed(vm: &mut Vm<'_>, this: ObjectRef) -> i64 {
    let seed = crate::lang::read_long_field(vm, this, "seed")
        .wrapping_add(crate::lang::read_long_field(vm, this, "gamma"));
    crate::lang::write_long_field(vm, this, "seed", seed);
    seed
}

fn splittable_next_long(vm: &mut Vm<'_>, this: ObjectRef) -> i64 {
    splitmix64_mix64(splittable_next_seed(vm, this))
}

fn splittable_next_int(vm: &mut Vm<'_>, this: ObjectRef) -> i32 {
    splitmix64_mix32(splittable_next_seed(vm, this))
}

/// Uniform int in `[0, bound)` with the JDK's rejection sampling.
fn splittable_int_bounded(vm: &mut Vm<'_>, this: ObjectRef, bound: i32) -> Result<i32, VmError> {
    if bound <= 0 {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("bound must be positive"),
        ));
    }
    let mut r = splittable_next_int(vm, this);
    let m = bound - 1;
    if bound & m == 0 {
        r &= m;
    } else {
        let mut u = (r as u32 >> 1) as i32;
        loop {
            r = u % bound;
            if u.wrapping_add(m).wrapping_sub(r) >= 0 {
                break;
            }
            u = (splittable_next_int(vm, this) as u32 >> 1) as i32;
        }
    }
    Ok(r)
}

/// Uniform long in `[0, bound)` with the JDK's rejection sampling.
fn splittable_long_bounded(vm: &mut Vm<'_>, this: ObjectRef, bound: i64) -> Result<i64, VmError> {
    if bound <= 0 {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("bound must be positive"),
        ));
    }
    let mut r = splittable_next_long(vm, this);
    let m = bound - 1;
    if bound & m == 0 {
        r &= m;
    } else {
        let mut u = (r as u64 >> 1) as i64;
        loop {
            r = u % bound;
            if u.wrapping_add(m).wrapping_sub(r) >= 0 {
                break;
            }
            u = (splittable_next_long(vm, this) as u64 >> 1) as i64;
        }
    }
    Ok(r)
}

fn splittable_next_double(vm: &mut Vm<'_>, this: ObjectRef) -> f64 {
    ((splittable_next_long(vm, this) as u64) >> 11) as f64 * (1.0 / ((1u64 << 53) as f64))
}

fn splittable_random_new(vm: &mut Vm<'_>, seed: i64, gamma: i64) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/SplittableRandom")?;
    let instance = vm.new_instance(class)?;
    crate::lang::write_long_field(vm, instance, "seed", seed);
    crate::lang::write_long_field(vm, instance, "gamma", gamma);
    Ok(instance)
}

const SPLITTABLE_RANDOM_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        crate::lang::write_long_field(
            vm,
            this,
            "seed",
            splitmix64_mix64(0x0123_4567_89ab_cdefu64 as i64),
        );
        crate::lang::write_long_field(vm, this, "gamma", GOLDEN_GAMMA);
        void()
    }),
    method("<init>", "(J)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        crate::lang::write_long_field(vm, this, "seed", crate::long_arg(args, 1));
        crate::lang::write_long_field(vm, this, "gamma", GOLDEN_GAMMA);
        void()
    }),
    method("nextInt", "()I", ACC_PUBLIC, |vm, _, args| {
        int(splittable_next_int(vm, receiver(args)))
    }),
    method("nextInt", "(I)I", ACC_PUBLIC, |vm, _, args| {
        int(splittable_int_bounded(
            vm,
            receiver(args),
            crate::int_arg(args, 1),
        )?)
    }),
    method("nextInt", "(II)I", ACC_PUBLIC, |vm, _, args| {
        let origin = crate::int_arg(args, 1);
        let bound = crate::int_arg(args, 2);
        if origin >= bound {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("origin must be less than bound"),
            ));
        }
        let n = bound.wrapping_sub(origin);
        let value = if n > 0 {
            origin.wrapping_add(splittable_int_bounded(vm, receiver(args), n)?)
        } else {
            loop {
                let r = splittable_next_int(vm, receiver(args));
                if r >= origin && r < bound {
                    break r;
                }
            }
        };
        int(value)
    }),
    method("nextLong", "()J", ACC_PUBLIC, |vm, _, args| {
        long(splittable_next_long(vm, receiver(args)))
    }),
    method("nextLong", "(J)J", ACC_PUBLIC, |vm, _, args| {
        long(splittable_long_bounded(
            vm,
            receiver(args),
            crate::long_arg(args, 1),
        )?)
    }),
    method("nextLong", "(JJ)J", ACC_PUBLIC, |vm, _, args| {
        let origin = crate::long_arg(args, 1);
        let bound = crate::long_arg(args, 2);
        if origin >= bound {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("origin must be less than bound"),
            ));
        }
        let n = bound.wrapping_sub(origin);
        let value = if n > 0 {
            origin.wrapping_add(splittable_long_bounded(vm, receiver(args), n)?)
        } else {
            loop {
                let r = splittable_next_long(vm, receiver(args));
                if r >= origin && r < bound {
                    break r;
                }
            }
        };
        long(value)
    }),
    method("nextDouble", "()D", ACC_PUBLIC, |vm, _, args| {
        crate::double(splittable_next_double(vm, receiver(args)))
    }),
    method("nextDouble", "(DD)D", ACC_PUBLIC, |vm, _, args| {
        let origin = crate::double_arg(args, 1);
        let bound = crate::double_arg(args, 2);
        if !matches!(origin.partial_cmp(&bound), Some(core::cmp::Ordering::Less)) {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("origin must be less than bound"),
            ));
        }
        let n = bound - origin;
        let value = if n.is_finite() && n > 0.0 {
            let mut r = origin + splittable_next_double(vm, receiver(args)) * n;
            if r >= bound {
                r = f64::from_bits(bound.to_bits() - 1);
            }
            r
        } else {
            loop {
                let r = origin + splittable_next_double(vm, receiver(args)) * n;
                if r >= origin && r < bound {
                    break r;
                }
            }
        };
        crate::double(value)
    }),
    method("nextBoolean", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(splittable_next_int(vm, receiver(args)) < 0)
    }),
    method(
        "split",
        "()Ljava/util/SplittableRandom;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let seed = splittable_next_long(vm, this);
            let gamma = splitmix64_mix_gamma(splittable_next_seed(vm, this));
            object(splittable_random_new(vm, seed, gamma)?)
        },
    ),
];

pub(crate) const SPLITTABLE_RANDOM: oxjvm_vm::NativeClass = class(
    "java/util/SplittableRandom",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("seed", "J", ACC_PRIVATE, None),
        field("gamma", "J", ACC_PRIVATE, None),
    ],
    &SPLITTABLE_RANDOM_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// CompletionService / ExecutorCompletionService
// -------------------------------------------------------------------------------------------

const COMPLETION_SERVICE_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "submit",
        "(Ljava/util/concurrent/Callable;)Ljava/util/concurrent/Future;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "submit",
        "(Ljava/lang/Runnable;Ljava/lang/Object;)Ljava/util/concurrent/Future;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "take",
        "()Ljava/util/concurrent/Future;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "poll",
        "()Ljava/util/concurrent/Future;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "poll",
        "(JLjava/util/concurrent/TimeUnit;)Ljava/util/concurrent/Future;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const COMPLETION_SERVICE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/CompletionService",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &COMPLETION_SERVICE_METHODS,
    None,
);

/// The completion queue held by an `ExecutorCompletionService` (an `ArrayList`).
fn completion_queue(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "queue", "Ljava/util/List;")
        .unwrap_or(ObjectRef::NULL)
}

/// Append a completed future to the queue.
fn completion_enqueue(vm: &mut Vm<'_>, this: ObjectRef, future: ObjectRef) -> Result<(), VmError> {
    let queue = completion_queue(vm, this);
    let (class, method) = vm.resolve_virtual_method(queue, "add", "(Ljava/lang/Object;)Z")?;
    vm.invoke_method(class, method, vec![Value::Ref(queue), Value::Ref(future)])?;
    Ok(())
}

/// Remove and return the head of the queue, or null when it is empty.
fn completion_dequeue(vm: &mut Vm<'_>, this: ObjectRef) -> Result<ObjectRef, VmError> {
    let queue = completion_queue(vm, this);
    let (size_class, size) = vm.resolve_virtual_method(queue, "size", "()I")?;
    let count = vm
        .invoke_method(size_class, size, vec![Value::Ref(queue)])?
        .as_int();
    if count == 0 {
        return Ok(ObjectRef::NULL);
    }
    let (class, method) = vm.resolve_virtual_method(queue, "remove", "(I)Ljava/lang/Object;")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(queue), Value::Int(0)])?
        .as_ref())
}

/// Submit a task, run it eagerly, and queue the completed future.
fn completion_submit(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    callable: ObjectRef,
    runnable: ObjectRef,
    result: ObjectRef,
) -> Result<Value, VmError> {
    let task = future_task_new(vm, callable, runnable, result)?;
    future_task_run(vm, task)?;
    completion_enqueue(vm, this, task)?;
    object(task)
}

const EXECUTOR_COMPLETION_SERVICE_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "<init>",
        "(Ljava/util/concurrent/Executor;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let queue = crate::util::array_list_from(vm, &[])?;
            let (declaring, field) =
                vm.find_field(class, "executor", "Ljava/util/concurrent/Executor;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            let (declaring, field) = vm.find_field(class, "queue", "Ljava/util/List;")?;
            vm.set_instance_ref(this, declaring, field, queue);
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/util/concurrent/Executor;Ljava/util/concurrent/BlockingQueue;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let queue = crate::util::array_list_from(vm, &[])?;
            let (declaring, field) =
                vm.find_field(class, "executor", "Ljava/util/concurrent/Executor;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            let (declaring, field) = vm.find_field(class, "queue", "Ljava/util/List;")?;
            vm.set_instance_ref(this, declaring, field, queue);
            void()
        },
    ),
    method(
        "submit",
        "(Ljava/util/concurrent/Callable;)Ljava/util/concurrent/Future;",
        ACC_PUBLIC,
        |vm, _, args| {
            completion_submit(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ObjectRef::NULL,
                ObjectRef::NULL,
            )
        },
    ),
    method(
        "submit",
        "(Ljava/lang/Runnable;Ljava/lang/Object;)Ljava/util/concurrent/Future;",
        ACC_PUBLIC,
        |vm, _, args| {
            completion_submit(
                vm,
                receiver(args),
                ObjectRef::NULL,
                ref_arg(args, 1),
                ref_arg(args, 2),
            )
        },
    ),
    method(
        "take",
        "()Ljava/util/concurrent/Future;",
        ACC_PUBLIC,
        |vm, _, args| {
            let future = completion_dequeue(vm, receiver(args))?;
            if future.is_null() {
                return Err(vm.throw_new("java/util/NoSuchElementException", None));
            }
            object(future)
        },
    ),
    method(
        "poll",
        "()Ljava/util/concurrent/Future;",
        ACC_PUBLIC,
        |vm, _, args| object(completion_dequeue(vm, receiver(args))?),
    ),
    method(
        "poll",
        "(JLjava/util/concurrent/TimeUnit;)Ljava/util/concurrent/Future;",
        ACC_PUBLIC,
        |vm, _, args| object(completion_dequeue(vm, receiver(args))?),
    ),
];

pub(crate) const EXECUTOR_COMPLETION_SERVICE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/ExecutorCompletionService",
    Some("java/lang/Object"),
    &["java/util/concurrent/CompletionService"],
    ACC_PUBLIC,
    &[
        field(
            "executor",
            "Ljava/util/concurrent/Executor;",
            ACC_PRIVATE,
            None,
        ),
        field("queue", "Ljava/util/List;", ACC_PRIVATE, None),
    ],
    &EXECUTOR_COMPLETION_SERVICE_METHODS,
    None,
);

/// `RejectedExecutionHandler`: the hook invoked when a task cannot be accepted.
pub(crate) const REJECTED_EXECUTION_HANDLER: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/RejectedExecutionHandler",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &REJECTED_EXECUTION_HANDLER_METHODS,
    None,
);

const REJECTED_EXECUTION_HANDLER_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "rejectedExecution",
    "(Ljava/lang/Runnable;Ljava/util/concurrent/ThreadPoolExecutor;)V",
    ACC_PUBLIC | ACC_ABSTRACT,
    crate::lang::abstract_method_pub,
)];

fn scheduled_tpe_init(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    core: i32,
    queue: ObjectRef,
) -> Result<Value, VmError> {
    if core < 0 {
        return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
    }
    thread_pool_executor_init(vm, this, core, i32::MAX, 0, queue)
}

fn scheduled_tpe_queue(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/concurrent/LinkedBlockingQueue")?;
    let queue = vm.new_instance(class)?;
    let (declaring, method) = vm.resolve_virtual_method(queue, "<init>", "()V")?;
    vm.invoke_method(declaring, method, vec![Value::Ref(queue)])?;
    Ok(queue)
}

pub(crate) const SCHEDULED_THREAD_POOL_EXECUTOR: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/ScheduledThreadPoolExecutor",
    Some("java/util/concurrent/ThreadPoolExecutor"),
    &["java/util/concurrent/ScheduledExecutorService"],
    ACC_PUBLIC,
    &[],
    &SCHEDULED_THREAD_POOL_EXECUTOR_METHODS,
    None,
);

const SCHEDULED_THREAD_POOL_EXECUTOR_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let queue = scheduled_tpe_queue(vm)?;
        scheduled_tpe_init(vm, receiver(args), crate::int_arg(args, 1), queue)
    }),
    method(
        "<init>",
        "(ILjava/util/concurrent/ThreadFactory;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            if ref_arg(args, 2).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let queue = scheduled_tpe_queue(vm)?;
            scheduled_tpe_init(vm, receiver(args), crate::int_arg(args, 1), queue)
        },
    ),
    method(
        "<init>",
        "(ILjava/util/concurrent/RejectedExecutionHandler;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            if ref_arg(args, 2).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let queue = scheduled_tpe_queue(vm)?;
            scheduled_tpe_init(vm, receiver(args), crate::int_arg(args, 1), queue)
        },
    ),
    method(
        "<init>",
        "(ILjava/util/concurrent/ThreadFactory;Ljava/util/concurrent/RejectedExecutionHandler;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            if ref_arg(args, 2).is_null() || ref_arg(args, 3).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let queue = scheduled_tpe_queue(vm)?;
            scheduled_tpe_init(vm, receiver(args), crate::int_arg(args, 1), queue)
        },
    ),
];
