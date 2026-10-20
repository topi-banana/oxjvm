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
    if !callable.is_null() {
        let (declaring, method) =
            vm.resolve_virtual_method(callable, "call", "()Ljava/lang/Object;")?;
        let result = vm
            .invoke_method(declaring, method, vec![Value::Ref(callable)])?
            .as_ref();
        let (declaring, field) = vm.find_field(class, "result", "Ljava/lang/Object;")?;
        vm.set_instance_ref(task, declaring, field, result);
    } else {
        let runnable = vm
            .read_ref_field(task, class, "runnable", "Ljava/lang/Object;")
            .unwrap_or(ObjectRef::NULL);
        if !runnable.is_null() {
            let (declaring, method) = vm.resolve_virtual_method(runnable, "run", "()V")?;
            vm.invoke_method(declaring, method, vec![Value::Ref(runnable)])?;
        }
    }
    let (declaring, field) = vm.find_field(class, "state", "I")?;
    vm.set_instance_int(task, declaring, field, 1);
    Ok(())
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

const FUTURE_TASK_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
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
        let this = receiver(args);
        let class = vm.class_of(this);
        if vm
            .read_instance_int_named(this, class, "state", "I")
            .unwrap_or(0)
            == 0
        {
            future_task_run(vm, this)?;
        }
        object(
            vm.read_ref_field(this, class, "result", "Ljava/lang/Object;")
                .unwrap_or(ObjectRef::NULL),
        )
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
                future_task_run(vm, this)?;
            }
            object(
                vm.read_ref_field(this, class, "result", "Ljava/lang/Object;")
                    .unwrap_or(ObjectRef::NULL),
            )
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
        boolean(true)
    }),
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
            let (declaring, field) = vm.find_field(class, "result", "Ljava/lang/Object;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            let (declaring, field) = vm.find_field(class, "state", "I")?;
            vm.set_instance_int(this, declaring, field, 1);
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
            let (declaring, field) = vm.find_field(class, "exception", "Ljava/lang/Throwable;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            let (declaring, field) = vm.find_field(class, "state", "I")?;
            vm.set_instance_int(this, declaring, field, 1);
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

const EXECUTOR_SERVICE_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
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
        "invokeAny",
        "(Ljava/util/Collection;)Ljava/lang/Object;",
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

pub(crate) const SCHEDULED_EXECUTOR_SERVICE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/ScheduledExecutorService",
    Some("java/lang/Object"),
    &["java/util/concurrent/ExecutorService"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
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

const OJVM_EXECUTOR_METHODS: [oxjvm_vm::NativeMethodDef; 15] = [
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

const EXECUTORS_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method(
        "newSingleThreadExecutor",
        "()Ljava/util/concurrent/ExecutorService;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_executor(vm)?),
    ),
    method(
        "newFixedThreadPool",
        "(I)Ljava/util/concurrent/ExecutorService;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_executor(vm)?),
    ),
    method(
        "newCachedThreadPool",
        "()Ljava/util/concurrent/ExecutorService;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_executor(vm)?),
    ),
    method(
        "newScheduledThreadPool",
        "(I)Ljava/util/concurrent/ScheduledExecutorService;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_executor(vm)?),
    ),
    method(
        "newSingleThreadScheduledExecutor",
        "()Ljava/util/concurrent/ScheduledExecutorService;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_executor(vm)?),
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
            let class = vm.resolve_class("java/util/concurrent/OjvmCallable")?;
            let callable = vm.new_instance(class)?;
            let (declaring, field) = vm.find_field(class, "runnable", "Ljava/lang/Runnable;")?;
            vm.set_instance_ref(callable, declaring, field, ref_arg(args, 0));
            object(callable)
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
        let runnable = vm
            .read_ref_field(this, vm.class_of(this), "runnable", "Ljava/lang/Runnable;")
            .unwrap_or(ObjectRef::NULL);
        if !runnable.is_null() {
            let (class, method) = vm.resolve_virtual_method(runnable, "run", "()V")?;
            vm.invoke_method(class, method, vec![Value::Ref(runnable)])?;
        }
        object(ObjectRef::NULL)
    },
)];

pub(crate) const OJVM_CALLABLE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/OjvmCallable",
    Some("java/lang/Object"),
    &["java/util/concurrent/Callable"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field("runnable", "Ljava/lang/Runnable;", ACC_PRIVATE, None)],
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
