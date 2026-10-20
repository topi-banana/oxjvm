//! `java.lang.ref`: a non-collecting reference implementation.
//!
//! References never clear themselves, but `get`/`clear`/queue bookkeeping is
//! enough for tests that only exercise the API and serialization metadata.

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::{boolean, class, field, method, object, receiver, ref_arg, void};

const REFERENCE_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
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
    method("isEnqueued", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("enqueue", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
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
    ],
    &REFERENCE_METHODS,
    None,
);

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
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "remove",
        "()Ljava/lang/ref/Reference;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "remove",
        "(J)Ljava/lang/ref/Reference;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
];

pub(crate) const REFERENCE_QUEUE: oxjvm_vm::NativeClass = class(
    "java/lang/ref/ReferenceQueue",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &REFERENCE_QUEUE_METHODS,
    None,
);
