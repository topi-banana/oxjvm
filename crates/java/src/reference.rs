//! `java.lang.ref`: a non-collecting reference implementation.
//!
//! References never clear themselves, but `get`/`clear`/queue bookkeeping is
//! enough for tests that only exercise the API and serialization metadata.

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::{boolean, class, field, method, object, receiver, ref_arg, void};

const REFERENCE_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "clone",
        "()Ljava/lang/Object;",
        ACC_PROTECTED,
        |vm, _, _| {
            // References are not cloneable; a subclass that wants cloning must
            // override clone itself (JDK Reference.clone always throws).
            Err(vm.throw_new("java/lang/CloneNotSupportedException", None))
        },
    ),
    method("get", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        object(
            vm.read_ref_field(
                receiver(args),
                vm.class_of(receiver(args)),
                "referent",
                "Ljava/lang/Object;",
            )
            .unwrap_or(ObjectRef::NULL),
        )
    }),
    method("clear", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if let Some((declaring, field)) =
            vm.find_field_opt(vm.class_of(this), "referent", "Ljava/lang/Object;")
        {
            vm.set_instance_ref(this, declaring, field, ObjectRef::NULL);
        }
        void()
    }),
    method(
        "refersTo",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let referent = vm
                .read_ref_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "referent",
                    "Ljava/lang/Object;",
                )
                .unwrap_or(ObjectRef::NULL);
            boolean(referent == ref_arg(args, 1))
        },
    ),
    method("isEnqueued", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "enqueued") != 0)
    }),
    method("enqueue", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let queue = vm
            .read_ref_field(
                this,
                vm.class_of(this),
                "queue",
                "Ljava/lang/ref/ReferenceQueue;",
            )
            .unwrap_or(ObjectRef::NULL);
        if queue.is_null() || crate::lang::read_int_field(vm, this, "enqueued") != 0 {
            return boolean(false);
        }
        reference_queue_push(vm, queue, this)?;
        boolean(true)
    }),
    method(
        "reachabilityFence",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| void(),
    ),
];

pub(crate) const REFERENCE: oxjvm_vm::NativeClass = class(
    "java/lang/ref/Reference",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[
        field("referent", "Ljava/lang/Object;", ACC_PRIVATE, None),
        field("queue", "Ljava/lang/ref/ReferenceQueue;", ACC_PRIVATE, None),
        field("enqueued", "I", ACC_PRIVATE, None),
        field("next", "Ljava/lang/ref/Reference;", ACC_PRIVATE, None),
    ],
    &REFERENCE_METHODS,
    None,
);

/// Link a reference onto the head of its queue and mark it enqueued.
fn reference_queue_push(vm: &mut Vm<'_>, queue: ObjectRef, this: ObjectRef) -> Result<(), VmError> {
    let class = vm.class_of(this);
    let head = vm
        .read_ref_field(
            queue,
            vm.class_of(queue),
            "head",
            "Ljava/lang/ref/Reference;",
        )
        .unwrap_or(ObjectRef::NULL);
    if let Some((declaring, field)) = vm.find_field_opt(class, "next", "Ljava/lang/ref/Reference;")
    {
        vm.set_instance_ref(this, declaring, field, head);
    }
    if let Some((declaring, field)) =
        vm.find_field_opt(vm.class_of(queue), "head", "Ljava/lang/ref/Reference;")
    {
        vm.set_instance_ref(queue, declaring, field, this);
    }
    if let Some((declaring, field)) = vm.find_field_opt(class, "enqueued", "I") {
        vm.set_instance_int(this, declaring, field, 1);
    }
    Ok(())
}

/// Pop the head of a queue, clearing its enqueued state; null when empty.
fn reference_queue_pop(vm: &mut Vm<'_>, queue: ObjectRef) -> Result<ObjectRef, VmError> {
    let head = vm
        .read_ref_field(
            queue,
            vm.class_of(queue),
            "head",
            "Ljava/lang/ref/Reference;",
        )
        .unwrap_or(ObjectRef::NULL);
    if head.is_null() {
        return Ok(ObjectRef::NULL);
    }
    let next = vm
        .read_ref_field(head, vm.class_of(head), "next", "Ljava/lang/ref/Reference;")
        .unwrap_or(ObjectRef::NULL);
    if let Some((declaring, field)) =
        vm.find_field_opt(vm.class_of(queue), "head", "Ljava/lang/ref/Reference;")
    {
        vm.set_instance_ref(queue, declaring, field, next);
    }
    let class = vm.class_of(head);
    if let Some((declaring, field)) = vm.find_field_opt(class, "next", "Ljava/lang/ref/Reference;")
    {
        vm.set_instance_ref(head, declaring, field, ObjectRef::NULL);
    }
    if let Some((declaring, field)) = vm.find_field_opt(class, "enqueued", "I") {
        vm.set_instance_int(head, declaring, field, 0);
    }
    Ok(head)
}

const REFERENCE_CONSTRUCTOR_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "<init>",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| reference_init(vm, receiver(args), ref_arg(args, 1), ObjectRef::NULL),
    ),
    method(
        "<init>",
        "(Ljava/lang/Object;Ljava/lang/ref/ReferenceQueue;)V",
        ACC_PUBLIC,
        |vm, _, args| reference_init(vm, receiver(args), ref_arg(args, 1), ref_arg(args, 2)),
    ),
];

fn reference_init(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    referent: ObjectRef,
    queue: ObjectRef,
) -> Result<Value, VmError> {
    let class = vm.class_of(this);
    if let Some((declaring, field)) = vm.find_field_opt(class, "referent", "Ljava/lang/Object;") {
        vm.set_instance_ref(this, declaring, field, referent);
    }
    if let Some((declaring, field)) =
        vm.find_field_opt(class, "queue", "Ljava/lang/ref/ReferenceQueue;")
    {
        vm.set_instance_ref(this, declaring, field, queue);
    }
    void()
}

pub(crate) const WEAK_REFERENCE: oxjvm_vm::NativeClass = class(
    "java/lang/ref/WeakReference",
    Some("java/lang/ref/Reference"),
    &[],
    ACC_PUBLIC,
    &[],
    &REFERENCE_CONSTRUCTOR_METHODS,
    None,
);

pub(crate) const SOFT_REFERENCE: oxjvm_vm::NativeClass = class(
    "java/lang/ref/SoftReference",
    Some("java/lang/ref/Reference"),
    &[],
    ACC_PUBLIC,
    &[],
    &REFERENCE_CONSTRUCTOR_METHODS,
    None,
);

pub(crate) const PHANTOM_REFERENCE: oxjvm_vm::NativeClass = class(
    "java/lang/ref/PhantomReference",
    Some("java/lang/ref/Reference"),
    &[],
    ACC_PUBLIC,
    &[],
    &REFERENCE_CONSTRUCTOR_METHODS,
    None,
);

const REFERENCE_QUEUE_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "poll",
        "()Ljava/lang/ref/Reference;",
        ACC_PUBLIC,
        |vm, _, args| object(reference_queue_pop(vm, receiver(args))?),
    ),
    method(
        "remove",
        "()Ljava/lang/ref/Reference;",
        ACC_PUBLIC,
        |vm, _, args| object(reference_queue_pop(vm, receiver(args))?),
    ),
    method(
        "remove",
        "(J)Ljava/lang/ref/Reference;",
        ACC_PUBLIC,
        |vm, _, args| object(reference_queue_pop(vm, receiver(args))?),
    ),
];

pub(crate) const REFERENCE_QUEUE: oxjvm_vm::NativeClass = class(
    "java/lang/ref/ReferenceQueue",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[field(
        "head",
        "Ljava/lang/ref/Reference;",
        ACC_PRIVATE,
        None,
    )],
    &REFERENCE_QUEUE_METHODS,
    None,
);
