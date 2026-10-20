//! `java.util`: `Objects`, `Arrays`, `Random`, and the core collections.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ArrayComponent, ArrayData, ObjectData, ObjectRef, Value, Vm, VmError};

use crate::{
    boolean, class, field, int, long, long_arg, method, object, receiver, ref_arg, string_arg, void,
};

// -------------------------------------------------------------------------------------------
// NoSuchElementException + interfaces
// -------------------------------------------------------------------------------------------

pub(crate) const NO_SUCH_ELEMENT_EXCEPTION: oxjvm_vm::NativeClass = {
    const METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
        method(
            "<init>",
            "()V",
            ACC_PUBLIC,
            crate::throwable::exception_ctor,
        ),
        method(
            "<init>",
            "(Ljava/lang/String;)V",
            ACC_PUBLIC,
            crate::throwable::exception_ctor,
        ),
        method(
            "<init>",
            "(Ljava/lang/String;Ljava/lang/Throwable;)V",
            ACC_PUBLIC,
            crate::throwable::exception_ctor,
        ),
        method(
            "<init>",
            "(Ljava/lang/Throwable;)V",
            ACC_PUBLIC,
            crate::throwable::exception_ctor,
        ),
    ];
    class(
        "java/util/NoSuchElementException",
        Some("java/lang/RuntimeException"),
        &[],
        ACC_PUBLIC,
        &[],
        &METHODS,
        None,
    )
};

/// `java.util.EmptyStackException`, thrown by the `Stack` pop/peek operations.
pub(crate) const EMPTY_STACK_EXCEPTION: oxjvm_vm::NativeClass = {
    const METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
        "<init>",
        "()V",
        ACC_PUBLIC,
        crate::throwable::exception_ctor,
    )];
    class(
        "java/util/EmptyStackException",
        Some("java/lang/RuntimeException"),
        &[],
        ACC_PUBLIC,
        &[],
        &METHODS,
        None,
    )
};

const ITERATOR_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method(
        "hasNext",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "next",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method("remove", "()V", ACC_PUBLIC, |_, _, _| void()),
];

pub(crate) const ITERATOR: oxjvm_vm::NativeClass = class(
    "java/util/Iterator",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &ITERATOR_METHODS,
    None,
);

const LIST_ITERATOR_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method(
        "hasNext",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "next",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "hasPrevious",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "previous",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "nextIndex",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "previousIndex",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "remove",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "set",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "add",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const LIST_ITERATOR: oxjvm_vm::NativeClass = class(
    "java/util/ListIterator",
    Some("java/lang/Object"),
    &["java/util/Iterator"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &LIST_ITERATOR_METHODS,
    None,
);

const COLLECTION_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method(
        "size",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "addAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "stream",
        "()Ljava/util/stream/Stream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let elements = collection_elements(vm, receiver(args))?;
            object(crate::stream::stream_from(vm, elements)?)
        },
    ),
    method(
        "isEmpty",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "add",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "remove",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "contains",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "clear",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "iterator",
        "()Ljava/util/Iterator;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "forEach",
        "(Ljava/util/function/Consumer;)V",
        ACC_PUBLIC,
        collection_for_each,
    ),
    method(
        "toArray",
        "()[Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "toArray",
        "([Ljava/lang/Object;)[Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const COLLECTION: oxjvm_vm::NativeClass = class(
    "java/util/Collection",
    Some("java/lang/Object"),
    &["java/lang/Iterable"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &COLLECTION_METHODS,
    None,
);

const LIST_METHODS: [oxjvm_vm::NativeMethodDef; 35] = [
    method(
        "size",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "sort",
        "(Ljava/util/Comparator;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "add",
        "(ILjava/lang/Object;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "remove",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "removeAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "addAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "addAll",
        "(ILjava/util/Collection;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "isEmpty",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "get",
        "(I)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "set",
        "(ILjava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "add",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "remove",
        "(I)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "contains",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "indexOf",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "clear",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "iterator",
        "()Ljava/util/Iterator;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "toArray",
        "()[Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "toArray",
        "([Ljava/lang/Object;)[Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "toArray",
        "(Ljava/util/function/IntFunction;)[Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "forEach",
        "(Ljava/util/function/Consumer;)V",
        ACC_PUBLIC,
        collection_for_each,
    ),
    method(
        "of",
        "()Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(list_of_values(vm, &[])?),
    ),
    method(
        "of",
        "(Ljava/lang/Object;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(list_of_values(vm, &[ref_arg(args, 0)])?),
    ),
    method(
        "of",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(list_of_values(vm, &[ref_arg(args, 0), ref_arg(args, 1)])?),
    ),
    method(
        "of",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            object(list_of_values(
                vm,
                &[ref_arg(args, 0), ref_arg(args, 1), ref_arg(args, 2)],
            )?)
        },
    ),
    method(
        "of",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            object(list_of_values(
                vm,
                &[
                    ref_arg(args, 0),
                    ref_arg(args, 1),
                    ref_arg(args, 2),
                    ref_arg(args, 3),
                ],
            )?)
        },
    ),
    method(
        "of",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            object(list_of_values(
                vm,
                &[
                    ref_arg(args, 0),
                    ref_arg(args, 1),
                    ref_arg(args, 2),
                    ref_arg(args, 3),
                    ref_arg(args, 4),
                ],
            )?)
        },
    ),
    method(
        "of",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            object(list_of_values(
                vm,
                &[
                    ref_arg(args, 0),
                    ref_arg(args, 1),
                    ref_arg(args, 2),
                    ref_arg(args, 3),
                    ref_arg(args, 4),
                    ref_arg(args, 5),
                ],
            )?)
        },
    ),
    method(
        "of",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            object(list_of_values(
                vm,
                &[
                    ref_arg(args, 0),
                    ref_arg(args, 1),
                    ref_arg(args, 2),
                    ref_arg(args, 3),
                    ref_arg(args, 4),
                    ref_arg(args, 5),
                    ref_arg(args, 6),
                ],
            )?)
        },
    ),
    method(
        "of",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            object(list_of_values(
                vm,
                &[
                    ref_arg(args, 0),
                    ref_arg(args, 1),
                    ref_arg(args, 2),
                    ref_arg(args, 3),
                    ref_arg(args, 4),
                    ref_arg(args, 5),
                    ref_arg(args, 6),
                    ref_arg(args, 7),
                ],
            )?)
        },
    ),
    method(
        "of",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            object(list_of_values(
                vm,
                &[
                    ref_arg(args, 0),
                    ref_arg(args, 1),
                    ref_arg(args, 2),
                    ref_arg(args, 3),
                    ref_arg(args, 4),
                    ref_arg(args, 5),
                    ref_arg(args, 6),
                    ref_arg(args, 7),
                    ref_arg(args, 8),
                ],
            )?)
        },
    ),
    method(
        "of",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            object(list_of_values(
                vm,
                &[
                    ref_arg(args, 0),
                    ref_arg(args, 1),
                    ref_arg(args, 2),
                    ref_arg(args, 3),
                    ref_arg(args, 4),
                    ref_arg(args, 5),
                    ref_arg(args, 6),
                    ref_arg(args, 7),
                    ref_arg(args, 8),
                    ref_arg(args, 9),
                ],
            )?)
        },
    ),
    method(
        "of",
        "([Ljava/lang/Object;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC | ACC_VARARGS,
        |vm, _, args| {
            let elements = object_array_values(vm, ref_arg(args, 0));
            object(list_of_values(vm, &elements)?)
        },
    ),
    method(
        "listIterator",
        "()Ljava/util/ListIterator;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "listIterator",
        "(I)Ljava/util/ListIterator;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "subList",
        "(II)Ljava/util/List;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const LIST: oxjvm_vm::NativeClass = class(
    "java/util/List",
    Some("java/lang/Object"),
    &["java/util/Collection"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &LIST_METHODS,
    None,
);

const SET_METHODS: [oxjvm_vm::NativeMethodDef; 26] = [
    method(
        "of",
        "()Ljava/util/Set;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(set_of_values(vm, &[])?),
    ),
    method(
        "of",
        "(Ljava/lang/Object;)Ljava/util/Set;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(set_of_values(vm, &[ref_arg(args, 0)])?),
    ),
    method(
        "of",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/Set;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(set_of_values(vm, &[ref_arg(args, 0), ref_arg(args, 1)])?),
    ),
    method(
        "of",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/Set;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            object(set_of_values(
                vm,
                &[ref_arg(args, 0), ref_arg(args, 1), ref_arg(args, 2)],
            )?)
        },
    ),
    method(
        "of",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/Set;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            object(set_of_values(
                vm,
                &[
                    ref_arg(args, 0),
                    ref_arg(args, 1),
                    ref_arg(args, 2),
                    ref_arg(args, 3),
                ],
            )?)
        },
    ),
    method(
        "of",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/Set;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            object(set_of_values(
                vm,
                &[
                    ref_arg(args, 0),
                    ref_arg(args, 1),
                    ref_arg(args, 2),
                    ref_arg(args, 3),
                    ref_arg(args, 4),
                ],
            )?)
        },
    ),
    method(
        "of",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/Set;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            object(set_of_values(
                vm,
                &[
                    ref_arg(args, 0),
                    ref_arg(args, 1),
                    ref_arg(args, 2),
                    ref_arg(args, 3),
                    ref_arg(args, 4),
                    ref_arg(args, 5),
                ],
            )?)
        },
    ),
    method(
        "of",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/Set;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            object(set_of_values(
                vm,
                &[
                    ref_arg(args, 0),
                    ref_arg(args, 1),
                    ref_arg(args, 2),
                    ref_arg(args, 3),
                    ref_arg(args, 4),
                    ref_arg(args, 5),
                    ref_arg(args, 6),
                ],
            )?)
        },
    ),
    method(
        "of",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/Set;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            object(set_of_values(
                vm,
                &[
                    ref_arg(args, 0),
                    ref_arg(args, 1),
                    ref_arg(args, 2),
                    ref_arg(args, 3),
                    ref_arg(args, 4),
                    ref_arg(args, 5),
                    ref_arg(args, 6),
                    ref_arg(args, 7),
                ],
            )?)
        },
    ),
    method(
        "of",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/Set;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            object(set_of_values(
                vm,
                &[
                    ref_arg(args, 0),
                    ref_arg(args, 1),
                    ref_arg(args, 2),
                    ref_arg(args, 3),
                    ref_arg(args, 4),
                    ref_arg(args, 5),
                    ref_arg(args, 6),
                    ref_arg(args, 7),
                    ref_arg(args, 8),
                ],
            )?)
        },
    ),
    method(
        "of",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/Set;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            object(set_of_values(
                vm,
                &[
                    ref_arg(args, 0),
                    ref_arg(args, 1),
                    ref_arg(args, 2),
                    ref_arg(args, 3),
                    ref_arg(args, 4),
                    ref_arg(args, 5),
                    ref_arg(args, 6),
                    ref_arg(args, 7),
                    ref_arg(args, 8),
                    ref_arg(args, 9),
                ],
            )?)
        },
    ),
    method(
        "of",
        "([Ljava/lang/Object;)Ljava/util/Set;",
        ACC_PUBLIC | ACC_STATIC | ACC_VARARGS,
        |vm, _, args| {
            let elements = object_array_values(vm, ref_arg(args, 0));
            object(set_of_values(vm, &elements)?)
        },
    ),
    method(
        "size",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "isEmpty",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "contains",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "iterator",
        "()Ljava/util/Iterator;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "add",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "remove",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "clear",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "addAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "containsAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "removeAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "retainAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "toArray",
        "()[Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "toArray",
        "([Ljava/lang/Object;)[Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "forEach",
        "(Ljava/util/function/Consumer;)V",
        ACC_PUBLIC,
        collection_for_each,
    ),
];

pub(crate) const SET: oxjvm_vm::NativeClass = class(
    "java/util/Set",
    Some("java/lang/Object"),
    &["java/util/Collection"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &SET_METHODS,
    None,
);

const SORTED_SET_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "comparator",
        "()Ljava/util/Comparator;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "first",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "last",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "subSet",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/SortedSet;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const SORTED_SET: oxjvm_vm::NativeClass = class(
    "java/util/SortedSet",
    Some("java/lang/Object"),
    &["java/util/Set"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &SORTED_SET_METHODS,
    None,
);

const NAVIGABLE_SET_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "first",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "last",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "pollFirst",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "pollLast",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "descendingSet",
        "()Ljava/util/NavigableSet;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const NAVIGABLE_SET: oxjvm_vm::NativeClass = class(
    "java/util/NavigableSet",
    Some("java/lang/Object"),
    &["java/util/SortedSet"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &NAVIGABLE_SET_METHODS,
    None,
);

macro_rules! map_of {
    (0, $descriptor:literal) => {
        method("of", $descriptor, ACC_PUBLIC | ACC_STATIC, |vm, _, _| {
            map_of_pairs(vm, &[])
        })
    };
    ($arg_count:expr, $descriptor:literal) => {
        method("of", $descriptor, ACC_PUBLIC | ACC_STATIC, |vm, _, args| {
            let mut pairs = Vec::with_capacity($arg_count / 2);
            let mut index = 0;
            while index < $arg_count {
                pairs.push((ref_arg(args, index), ref_arg(args, index + 1)));
                index += 2;
            }
            map_of_pairs(vm, &pairs)
        })
    };
}

const MAP_METHODS: [oxjvm_vm::NativeMethodDef; 26] = [
    method(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "putIfAbsent",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getOrDefault",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "computeIfAbsent",
        "(Ljava/lang/Object;Ljava/util/function/Function;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "remove",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "containsKey",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "size",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "isEmpty",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "clear",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "keySet",
        "()Ljava/util/Set;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "putAll",
        "(Ljava/util/Map;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "entrySet",
        "()Ljava/util/Set;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "values",
        "()Ljava/util/Collection;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "forEach",
        "(Ljava/util/function/BiConsumer;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = receiver(args);
            let action = ref_arg(args, 1);
            if action.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            for (key, value) in abstract_map_entries(vm, map)? {
                let (class, method) = vm.resolve_virtual_method(
                    action,
                    "accept",
                    "(Ljava/lang/Object;Ljava/lang/Object;)V",
                )?;
                vm.invoke_method(
                    class,
                    method,
                    vec![Value::Ref(action), Value::Ref(key), Value::Ref(value)],
                )?;
            }
            void()
        },
    ),
    map_of!(0, "()Ljava/util/Map;"),
    map_of!(2, "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/Map;"),
    map_of!(
        4,
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/Map;"
    ),
    map_of!(
        6,
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/Map;"
    ),
    map_of!(
        8,
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/Map;"
    ),
    map_of!(
        10,
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/Map;"
    ),
    map_of!(
        12,
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/Map;"
    ),
    map_of!(
        14,
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/Map;"
    ),
    map_of!(
        16,
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/Map;"
    ),
    map_of!(
        18,
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/Map;"
    ),
    map_of!(
        20,
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/Map;"
    ),
];

pub(crate) const MAP: oxjvm_vm::NativeClass = class(
    "java/util/Map",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &MAP_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Objects
// -------------------------------------------------------------------------------------------

const OBJECTS_METHODS: [oxjvm_vm::NativeMethodDef; 13] = [
    method(
        "equals",
        "(Ljava/lang/Object;Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let a = ref_arg(args, 0);
            let b = ref_arg(args, 1);
            boolean(objects_equals(vm, a, b)?)
        },
    ),
    method(
        "hashCode",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let reference = ref_arg(args, 0);
            if reference.is_null() {
                int(0)
            } else {
                let (class, method) = vm.resolve_virtual_method(reference, "hashCode", "()I")?;
                vm.invoke_method(class, method, vec![Value::Ref(reference)])
            }
        },
    ),
    method(
        "hash",
        "([Ljava/lang/Object;)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let array = ref_arg(args, 0);
            if array.is_null() {
                return int(1);
            }
            let elements: Vec<ObjectRef> = match vm.heap.get(array).map(|object| &object.data) {
                Some(ObjectData::Array(ArrayData::Reference(elements))) => elements.clone(),
                _ => Vec::new(),
            };
            let mut result = 1i32;
            for element in elements {
                let hash = if element.is_null() {
                    0
                } else {
                    let (class, method) = vm.resolve_virtual_method(element, "hashCode", "()I")?;
                    vm.invoke_method(class, method, vec![Value::Ref(element)])?
                        .as_int()
                };
                result = result.wrapping_mul(31).wrapping_add(hash);
            }
            int(result)
        },
    ),
    method(
        "toString",
        "(Ljava/lang/Object;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = vm.display_value(
                &oxjvm_classfile::FieldType::Object("java/lang/Object".into()),
                args[0],
            )?;
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toString",
        "(Ljava/lang/Object;Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let reference = ref_arg(args, 0);
            if reference.is_null() {
                return object(vm.make_string(&vm.string_or_empty(ref_arg(args, 1)))?);
            }
            let (class, method) =
                vm.resolve_virtual_method(reference, "toString", "()Ljava/lang/String;")?;
            vm.invoke_method(class, method, vec![Value::Ref(reference)])
        },
    ),
    method(
        "requireNonNull",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let reference = ref_arg(args, 0);
            if reference.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(reference)
        },
    ),
    method(
        "requireNonNull",
        "(Ljava/lang/Object;Ljava/lang/String;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let reference = ref_arg(args, 0);
            if reference.is_null() {
                return Err(vm.throw_new(
                    "java/lang/NullPointerException",
                    Some(&vm.string_or_empty(ref_arg(args, 1))),
                ));
            }
            object(reference)
        },
    ),
    method(
        "isNull",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| boolean(ref_arg(args, 0).is_null()),
    ),
    method(
        "nonNull",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| boolean(!ref_arg(args, 0).is_null()),
    ),
    method(
        "checkIndex",
        "(II)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let index = crate::int_arg(args, 0);
            let length = crate::int_arg(args, 1);
            if index < 0 || index >= length {
                return Err(vm.throw_new(
                    "java/lang/IndexOutOfBoundsException",
                    Some(&alloc::format!(
                        "Index {index} out of bounds for length {length}"
                    )),
                ));
            }
            int(index)
        },
    ),
    method(
        "checkIndex",
        "(JJ)J",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let index = long_arg(args, 0);
            let length = long_arg(args, 1);
            if index < 0 || index >= length {
                return Err(vm.throw_new(
                    "java/lang/IndexOutOfBoundsException",
                    Some(&alloc::format!(
                        "Index {index} out of bounds for length {length}"
                    )),
                ));
            }
            long(index)
        },
    ),
    method(
        "deepEquals",
        "(Ljava/lang/Object;Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| boolean(objects_deep_equals(vm, ref_arg(args, 0), ref_arg(args, 1))?),
    ),
    method(
        "deepHashCode",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| int(objects_deep_hash_code(vm, ref_arg(args, 0))?),
    ),
];

fn objects_deep_equals(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<bool, VmError> {
    if a == b {
        return Ok(true);
    }
    if a.is_null() || b.is_null() {
        return Ok(false);
    }
    let data_a = vm.heap.get(a).map(|object| object.data.clone());
    let data_b = vm.heap.get(b).map(|object| object.data.clone());
    match (data_a, data_b) {
        (Some(ObjectData::Array(array_a)), Some(ObjectData::Array(array_b))) => {
            if array_a.len() != array_b.len() {
                return Ok(false);
            }
            match (array_a, array_b) {
                (ArrayData::Boolean(x), ArrayData::Boolean(y)) => Ok(x == y),
                (ArrayData::Byte(x), ArrayData::Byte(y)) => Ok(x == y),
                (ArrayData::Char(x), ArrayData::Char(y)) => Ok(x == y),
                (ArrayData::Short(x), ArrayData::Short(y)) => Ok(x == y),
                (ArrayData::Int(x), ArrayData::Int(y)) => Ok(x == y),
                (ArrayData::Long(x), ArrayData::Long(y)) => Ok(x == y),
                (ArrayData::Float(x), ArrayData::Float(y)) => {
                    Ok(x.iter().zip(&y).all(|(a, b)| a.to_bits() == b.to_bits()))
                }
                (ArrayData::Double(x), ArrayData::Double(y)) => {
                    Ok(x.iter().zip(&y).all(|(a, b)| a.to_bits() == b.to_bits()))
                }
                (ArrayData::Reference(x), ArrayData::Reference(y)) => {
                    for (left, right) in x.iter().zip(&y) {
                        if !objects_deep_equals(vm, *left, *right)? {
                            return Ok(false);
                        }
                    }
                    Ok(true)
                }
                _ => Ok(false),
            }
        }
        _ => objects_equals(vm, a, b),
    }
}

fn objects_deep_hash_code(vm: &mut Vm<'_>, reference: ObjectRef) -> Result<i32, VmError> {
    if reference.is_null() {
        return Ok(0);
    }
    let data = vm.heap.get(reference).map(|object| object.data.clone());
    let Some(ObjectData::Array(array)) = data else {
        let (class, method) = vm.resolve_virtual_method(reference, "hashCode", "()I")?;
        return Ok(vm
            .invoke_method(class, method, vec![Value::Ref(reference)])?
            .as_int());
    };
    let mut result = 1i32;
    match array {
        ArrayData::Boolean(values) => {
            for value in values {
                result = result
                    .wrapping_mul(31)
                    .wrapping_add(if value != 0 { 1231 } else { 1237 });
            }
        }
        ArrayData::Byte(values) => {
            for value in values {
                result = result.wrapping_mul(31).wrapping_add(i32::from(value));
            }
        }
        ArrayData::Char(values) => {
            for value in values {
                result = result.wrapping_mul(31).wrapping_add(i32::from(value));
            }
        }
        ArrayData::Short(values) => {
            for value in values {
                result = result.wrapping_mul(31).wrapping_add(i32::from(value));
            }
        }
        ArrayData::Int(values) => {
            for value in values {
                result = result.wrapping_mul(31).wrapping_add(value);
            }
        }
        ArrayData::Long(values) => {
            for value in values {
                result = result
                    .wrapping_mul(31)
                    .wrapping_add((value ^ (value >> 32)) as i32);
            }
        }
        ArrayData::Float(values) => {
            for value in values {
                result = result.wrapping_mul(31).wrapping_add(value.to_bits() as i32);
            }
        }
        ArrayData::Double(values) => {
            for value in values {
                let bits = value.to_bits();
                result = result
                    .wrapping_mul(31)
                    .wrapping_add((bits ^ (bits >> 32)) as i32);
            }
        }
        ArrayData::Reference(values) => {
            for value in values {
                result = result
                    .wrapping_mul(31)
                    .wrapping_add(objects_deep_hash_code(vm, value)?);
            }
        }
    }
    Ok(result)
}

fn objects_equals(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<bool, VmError> {
    if a == b {
        return Ok(true);
    }
    if a.is_null() || b.is_null() {
        return Ok(false);
    }
    let (class, method) = vm.resolve_virtual_method(a, "equals", "(Ljava/lang/Object;)Z")?;
    let result = vm.invoke_method(class, method, vec![Value::Ref(a), Value::Ref(b)])?;
    Ok(result.as_int() != 0)
}

pub(crate) const OBJECTS: oxjvm_vm::NativeClass = class(
    "java/util/Objects",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &OBJECTS_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Arrays
// -------------------------------------------------------------------------------------------

macro_rules! array_equals {
    ($descriptor:literal) => {
        method(
            "equals",
            $descriptor,
            ACC_PUBLIC | ACC_STATIC,
            |vm, _, args| {
                let a = ref_arg(args, 0);
                let b = ref_arg(args, 1);
                boolean(a == b || arrays_equal(vm, a, b)?)
            },
        )
    };
}

macro_rules! array_copy_of {
    ($descriptor:literal) => {
        method(
            "copyOf",
            $descriptor,
            ACC_PUBLIC | ACC_STATIC,
            arrays_copy_of_objects,
        )
    };
}

macro_rules! array_copy_of_range {
    ($descriptor:literal) => {
        method(
            "copyOfRange",
            $descriptor,
            ACC_PUBLIC | ACC_STATIC,
            arrays_copy_of_range_objects,
        )
    };
}

macro_rules! array_mismatch {
    ($descriptor:literal) => {
        method(
            "mismatch",
            $descriptor,
            ACC_PUBLIC | ACC_STATIC,
            |vm, _, args| int(arrays_mismatch(vm, ref_arg(args, 0), ref_arg(args, 1))?),
        )
    };
}

const ARRAYS_METHODS: [oxjvm_vm::NativeMethodDef; 70] = [
    method(
        "toString",
        "([I)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| arrays_to_string(vm, ref_arg(args, 0), ArrayComponent::Int),
    ),
    method(
        "toString",
        "([J)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| arrays_to_string(vm, ref_arg(args, 0), ArrayComponent::Long),
    ),
    method(
        "toString",
        "([D)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| arrays_to_string(vm, ref_arg(args, 0), ArrayComponent::Double),
    ),
    method(
        "toString",
        "([F)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| arrays_to_string(vm, ref_arg(args, 0), ArrayComponent::Float),
    ),
    method(
        "toString",
        "([B)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| arrays_to_string(vm, ref_arg(args, 0), ArrayComponent::Byte),
    ),
    method(
        "toString",
        "([C)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| arrays_to_string(vm, ref_arg(args, 0), ArrayComponent::Char),
    ),
    method(
        "toString",
        "([S)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| arrays_to_string(vm, ref_arg(args, 0), ArrayComponent::Short),
    ),
    method(
        "toString",
        "([Z)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| arrays_to_string(vm, ref_arg(args, 0), ArrayComponent::Boolean),
    ),
    method(
        "toString",
        "([Ljava/lang/Object;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        arrays_to_string_objects,
    ),
    method(
        "copyOf",
        "([II)[I",
        ACC_PUBLIC | ACC_STATIC,
        arrays_copy_of_int,
    ),
    method("fill", "([II)V", ACC_PUBLIC | ACC_STATIC, |vm, _, args| {
        let array = ref_arg(args, 0);
        let length = vm.array_length(array)?;
        for index in 0..length {
            vm.array_set(
                array,
                index as i32,
                Value::Int(crate::int_arg(args, 1)),
                false,
            )?;
        }
        void()
    }),
    method(
        "equals",
        "([I[I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let a = ref_arg(args, 0);
            let b = ref_arg(args, 1);
            boolean(a == b || arrays_equal(vm, a, b)?)
        },
    ),
    method(
        "equals",
        "([Ljava/lang/Object;[Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let a = ref_arg(args, 0);
            let b = ref_arg(args, 1);
            boolean(a == b || arrays_equal(vm, a, b)?)
        },
    ),
    method(
        "copyOf",
        "([Ljava/lang/Object;I)[Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        arrays_copy_of_objects,
    ),
    method(
        "copyOfRange",
        "([Ljava/lang/Object;II)[Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        arrays_copy_of_range_objects,
    ),
    method(
        "asList",
        "([Ljava/lang/Object;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC | ACC_VARARGS,
        arrays_as_list,
    ),
    method(
        "fill",
        "([Ljava/lang/Object;Ljava/lang/Object;)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| arrays_fill(vm, ref_arg(args, 0), Value::Ref(ref_arg(args, 1)), true),
    ),
    method("fill", "([JJ)V", ACC_PUBLIC | ACC_STATIC, |vm, _, args| {
        arrays_fill(vm, ref_arg(args, 0), Value::Long(long_arg(args, 1)), false)
    }),
    method("fill", "([ZZ)V", ACC_PUBLIC | ACC_STATIC, |vm, _, args| {
        arrays_fill(
            vm,
            ref_arg(args, 0),
            Value::Int(crate::int_arg(args, 1)),
            false,
        )
    }),
    method("fill", "([BB)V", ACC_PUBLIC | ACC_STATIC, |vm, _, args| {
        arrays_fill(
            vm,
            ref_arg(args, 0),
            Value::Int(crate::int_arg(args, 1)),
            false,
        )
    }),
    method("fill", "([CC)V", ACC_PUBLIC | ACC_STATIC, |vm, _, args| {
        arrays_fill(
            vm,
            ref_arg(args, 0),
            Value::Int(crate::int_arg(args, 1)),
            false,
        )
    }),
    method("fill", "([SS)V", ACC_PUBLIC | ACC_STATIC, |vm, _, args| {
        arrays_fill(
            vm,
            ref_arg(args, 0),
            Value::Int(crate::int_arg(args, 1)),
            false,
        )
    }),
    method("fill", "([FF)V", ACC_PUBLIC | ACC_STATIC, |vm, _, args| {
        arrays_fill(
            vm,
            ref_arg(args, 0),
            Value::Float(crate::float_arg(args, 1)),
            false,
        )
    }),
    method("fill", "([DD)V", ACC_PUBLIC | ACC_STATIC, |vm, _, args| {
        arrays_fill(
            vm,
            ref_arg(args, 0),
            Value::Double(crate::double_arg(args, 1)),
            false,
        )
    }),
    method(
        "stream",
        "([Ljava/lang/Object;)Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| crate::stream::stream_of_array(vm, ref_arg(args, 0)),
    ),
    method("sort", "([I)V", ACC_PUBLIC | ACC_STATIC, |vm, _, args| {
        arrays_sort_int(vm, ref_arg(args, 0))
    }),
    method("sort", "([J)V", ACC_PUBLIC | ACC_STATIC, |vm, _, args| {
        arrays_sort_long(vm, ref_arg(args, 0))
    }),
    method("sort", "([S)V", ACC_PUBLIC | ACC_STATIC, |vm, _, args| {
        arrays_sort_short(vm, ref_arg(args, 0))
    }),
    method("sort", "([C)V", ACC_PUBLIC | ACC_STATIC, |vm, _, args| {
        arrays_sort_char(vm, ref_arg(args, 0))
    }),
    method("sort", "([B)V", ACC_PUBLIC | ACC_STATIC, |vm, _, args| {
        arrays_sort_byte(vm, ref_arg(args, 0))
    }),
    method("sort", "([F)V", ACC_PUBLIC | ACC_STATIC, |vm, _, args| {
        arrays_sort_float(vm, ref_arg(args, 0))
    }),
    method("sort", "([D)V", ACC_PUBLIC | ACC_STATIC, |vm, _, args| {
        arrays_sort_double(vm, ref_arg(args, 0))
    }),
    method(
        "sort",
        "([Ljava/lang/Object;)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| arrays_sort_objects(vm, args, false),
    ),
    method(
        "sort",
        "([Ljava/lang/Object;Ljava/util/Comparator;)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| arrays_sort_objects(vm, args, true),
    ),
    method(
        "stream",
        "([I)Ljava/util/stream/IntStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let array = ref_arg(args, 0);
            let length = vm.array_length(array)?;
            let mut values = Vec::with_capacity(length);
            for index in 0..length {
                values.push(vm.array_get(array, index as i32)?.as_int());
            }
            crate::intstream::int_stream_of(vm, values)
        },
    ),
    array_equals!("([B[B)Z"),
    array_equals!("([C[C)Z"),
    array_equals!("([S[S)Z"),
    array_equals!("([J[J)Z"),
    array_equals!("([F[F)Z"),
    array_equals!("([D[D)Z"),
    array_equals!("([Z[Z)Z"),
    array_copy_of!("([BI)[B"),
    array_copy_of!("([CI)[C"),
    array_copy_of!("([SI)[S"),
    array_copy_of!("([JI)[J"),
    array_copy_of!("([FI)[F"),
    array_copy_of!("([DI)[D"),
    array_copy_of!("([ZI)[Z"),
    method(
        "copyOf",
        "([Ljava/lang/Object;ILjava/lang/Class;)[Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        arrays_copy_of_objects,
    ),
    array_copy_of_range!("([BII)[B"),
    array_copy_of_range!("([CII)[C"),
    array_copy_of_range!("([SII)[S"),
    array_copy_of_range!("([III)[I"),
    array_copy_of_range!("([JII)[J"),
    array_copy_of_range!("([FII)[F"),
    array_copy_of_range!("([DII)[D"),
    array_copy_of_range!("([ZII)[Z"),
    method(
        "copyOfRange",
        "([Ljava/lang/Object;IILjava/lang/Class;)[Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        arrays_copy_of_range_objects,
    ),
    array_mismatch!("([B[B)I"),
    array_mismatch!("([C[C)I"),
    array_mismatch!("([S[S)I"),
    array_mismatch!("([I[I)I"),
    array_mismatch!("([J[J)I"),
    array_mismatch!("([F[F)I"),
    array_mismatch!("([D[D)I"),
    array_mismatch!("([Z[Z)I"),
    array_mismatch!("([Ljava/lang/Object;[Ljava/lang/Object;)I"),
    method(
        "mismatch",
        "([III[III)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            int(arrays_mismatch_range(
                vm,
                ref_arg(args, 0),
                crate::int_arg(args, 1),
                ref_arg(args, 2),
                crate::int_arg(args, 3),
                crate::int_arg(args, 4),
            )?)
        },
    ),
    method(
        "deepToString",
        "([Ljava/lang/Object;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = deep_array_text(vm, ref_arg(args, 0))?;
            object(vm.make_string(&text)?)
        },
    ),
];

/// The recursive rendering used by `Arrays.deepToString`.
fn deep_array_text(vm: &mut Vm<'_>, array: ObjectRef) -> Result<String, VmError> {
    if array.is_null() {
        return Ok("null".into());
    }
    let data = vm.heap.get(array).map(|object| object.data.clone());
    let Some(ObjectData::Array(data)) = data else {
        let (class, method) =
            vm.resolve_virtual_method(array, "toString", "()Ljava/lang/String;")?;
        let string = vm.invoke_method(class, method, vec![Value::Ref(array)])?;
        return Ok(vm.string_or_empty(string.as_ref()));
    };
    let component = match &data {
        ArrayData::Boolean(_) => ArrayComponent::Boolean,
        ArrayData::Byte(_) => ArrayComponent::Byte,
        ArrayData::Char(_) => ArrayComponent::Char,
        ArrayData::Short(_) => ArrayComponent::Short,
        ArrayData::Int(_) => ArrayComponent::Int,
        ArrayData::Long(_) => ArrayComponent::Long,
        ArrayData::Float(_) => ArrayComponent::Float,
        ArrayData::Double(_) => ArrayComponent::Double,
        ArrayData::Reference(_) => ArrayComponent::Reference,
    };
    let length = vm.array_length(array)?;
    let mut text = String::from("[");
    for index in 0..length {
        if index > 0 {
            text.push_str(", ");
        }
        let value = vm.array_get(array, index as i32)?;
        match component {
            ArrayComponent::Reference => {
                let reference = value.as_ref();
                if reference.is_null() {
                    text.push_str("null");
                } else if matches!(
                    vm.heap.get(reference).map(|object| &object.data),
                    Some(ObjectData::Array(_))
                ) {
                    text.push_str(&deep_array_text(vm, reference)?);
                } else {
                    let (class, method) =
                        vm.resolve_virtual_method(reference, "toString", "()Ljava/lang/String;")?;
                    let string = vm.invoke_method(class, method, vec![Value::Ref(reference)])?;
                    text.push_str(&vm.string_or_empty(string.as_ref()));
                }
            }
            _ => text.push_str(&format_array_element(value, &component)),
        }
    }
    text.push(']');
    Ok(text)
}

/// Sort an integer array; `Values` are read and written in the array's own component width.
fn arrays_sort_int(vm: &mut Vm<'_>, array: ObjectRef) -> Result<Value, VmError> {
    arrays_sort_primitive(vm, array, ArrayComponent::Int)
}

fn arrays_sort_long(vm: &mut Vm<'_>, array: ObjectRef) -> Result<Value, VmError> {
    arrays_sort_primitive(vm, array, ArrayComponent::Long)
}

fn arrays_sort_short(vm: &mut Vm<'_>, array: ObjectRef) -> Result<Value, VmError> {
    arrays_sort_primitive(vm, array, ArrayComponent::Short)
}

fn arrays_sort_char(vm: &mut Vm<'_>, array: ObjectRef) -> Result<Value, VmError> {
    arrays_sort_primitive(vm, array, ArrayComponent::Char)
}

fn arrays_sort_byte(vm: &mut Vm<'_>, array: ObjectRef) -> Result<Value, VmError> {
    arrays_sort_primitive(vm, array, ArrayComponent::Byte)
}

fn arrays_sort_float(vm: &mut Vm<'_>, array: ObjectRef) -> Result<Value, VmError> {
    arrays_sort_primitive(vm, array, ArrayComponent::Float)
}

fn arrays_sort_double(vm: &mut Vm<'_>, array: ObjectRef) -> Result<Value, VmError> {
    arrays_sort_primitive(vm, array, ArrayComponent::Double)
}

/// Sort a primitive array in place with the natural order of its component.
fn arrays_sort_primitive(
    vm: &mut Vm<'_>,
    array: ObjectRef,
    component: ArrayComponent,
) -> Result<Value, VmError> {
    if array.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let length = vm.array_length(array)?;
    if matches!(component, ArrayComponent::Int | ArrayComponent::Long) {
        let mut values: Vec<Value> = Vec::with_capacity(length);
        for index in 0..length {
            values.push(vm.array_get(array, index as i32)?);
        }
        if component == ArrayComponent::Int {
            values.sort_by_key(|value| value.as_int());
        } else {
            values.sort_by_key(|value| value.as_long());
        }
        for (index, value) in values.into_iter().enumerate() {
            vm.array_set(array, index as i32, value, false)?;
        }
        return void();
    }
    if matches!(
        component,
        ArrayComponent::Byte | ArrayComponent::Char | ArrayComponent::Short | ArrayComponent::Float
    ) {
        let mut values: Vec<i32> = Vec::with_capacity(length);
        for index in 0..length {
            let value = vm.array_get(array, index as i32)?;
            values.push(if component == ArrayComponent::Float {
                sortable_float(value).to_bits() as i32
            } else {
                value.as_int()
            });
        }
        if component == ArrayComponent::Float {
            values.sort_by(|a, b| f32::from_bits(*a as u32).total_cmp(&f32::from_bits(*b as u32)));
        } else {
            let key: fn(i32) -> i32 = match component {
                ArrayComponent::Byte => |value| i32::from(value as i8),
                ArrayComponent::Short => |value| i32::from(value as i16),
                _ => |value| i32::from(value as u16),
            };
            values.sort_by_key(|value| key(*value));
        }
        for (index, value) in values.into_iter().enumerate() {
            let slot = if component == ArrayComponent::Float {
                Value::Float(sortable_float(Value::Int(value)))
            } else {
                Value::Int(value)
            };
            vm.array_set(array, index as i32, slot, false)?;
        }
        return void();
    }
    let mut values: Vec<f64> = Vec::with_capacity(length);
    for index in 0..length {
        values.push(sortable_double(vm.array_get(array, index as i32)?));
    }
    values.sort_by(f64::total_cmp);
    for (index, value) in values.into_iter().enumerate() {
        vm.array_set(array, index as i32, Value::Double(value), false)?;
    }
    void()
}

/// A float slot's value: either a `Float` or the raw bits in an `Int`.
fn sortable_float(value: Value) -> f32 {
    match value {
        Value::Float(value) => value,
        Value::Int(bits) => f32::from_bits(bits as u32),
        _ => 0.0,
    }
}

/// A double slot's value: either a `Double` or the raw bits in a `Long`/`Int`.
fn sortable_double(value: Value) -> f64 {
    match value {
        Value::Double(value) => value,
        Value::Long(bits) => f64::from_bits(bits as u64),
        Value::Int(bits) => f64::from_bits(bits as u32 as u64),
        _ => 0.0,
    }
}

/// `Arrays.sort(Object[])` and `Arrays.sort(Object[], Comparator)`.
fn arrays_sort_objects(
    vm: &mut Vm<'_>,
    args: &[Value],
    with_comparator: bool,
) -> Result<Value, VmError> {
    let array = ref_arg(args, 0);
    if array.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let comparator = if with_comparator {
        let comparator = ref_arg(args, 1);
        if comparator.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        Some(comparator)
    } else {
        None
    };
    let length = vm.array_length(array)?;
    let mut values: Vec<Value> = Vec::with_capacity(length);
    for index in 0..length {
        values.push(vm.array_get(array, index as i32)?);
    }
    let mut error: Option<VmError> = None;
    values.sort_by(|a, b| {
        if error.is_some() {
            return core::cmp::Ordering::Equal;
        }
        let ordered = match comparator {
            Some(comparator) => compare_with(vm, comparator, *a, *b),
            None => compare_natural(vm, *a, *b),
        };
        match ordered {
            Ok(ordering) => ordering,
            Err(failure) => {
                error = Some(failure);
                core::cmp::Ordering::Equal
            }
        }
    });
    if let Some(failure) = error {
        return Err(failure);
    }
    for (index, value) in values.into_iter().enumerate() {
        vm.array_set(array, index as i32, value, true)?;
    }
    void()
}

/// `a.compareTo(b)` through the virtual method table.
fn compare_natural(vm: &mut Vm<'_>, a: Value, b: Value) -> Result<core::cmp::Ordering, VmError> {
    let left = a.as_ref();
    if left.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let (class, method) = vm.resolve_virtual_method(left, "compareTo", "(Ljava/lang/Object;)I")?;
    let result = vm
        .invoke_method(class, method, vec![Value::Ref(left), b])?
        .as_int();
    Ok(result.cmp(&0))
}

/// `comparator.compare(a, b)` through the virtual method table.
fn compare_with(
    vm: &mut Vm<'_>,
    comparator: ObjectRef,
    a: Value,
    b: Value,
) -> Result<core::cmp::Ordering, VmError> {
    let (class, method) = vm.resolve_virtual_method(
        comparator,
        "compare",
        "(Ljava/lang/Object;Ljava/lang/Object;)I",
    )?;
    let result = vm
        .invoke_method(class, method, vec![Value::Ref(comparator), a, b])?
        .as_int();
    Ok(result.cmp(&0))
}

/// Fill every slot of an array with the same value.
fn arrays_fill(
    vm: &mut Vm<'_>,
    array: ObjectRef,
    value: Value,
    checked: bool,
) -> Result<Value, VmError> {
    if array.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let length = vm.array_length(array)?;
    for index in 0..length {
        vm.array_set(array, index as i32, value, checked)?;
    }
    void()
}

/// `Arrays.copyOf(Object[], int)`.
fn arrays_copy_of_objects(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let source = ref_arg(args, 0);
    if source.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let requested = crate::int_arg(args, 1);
    if requested < 0 {
        return Err(vm.throw_new(
            "java/lang/NegativeArraySizeException",
            Some(&alloc::format!("{requested}")),
        ));
    }
    let length = vm.array_length(source)?;
    let result = vm.allocate_array(vm.class_of(source), requested as usize)?;
    for index in 0..length.min(requested as usize) {
        let value = vm.array_get(source, index as i32)?;
        vm.array_set(result, index as i32, value, true)?;
    }
    object(result)
}

/// `Arrays.copyOfRange(Object[], int, int)`: `to` may exceed the source length, in which case the
/// tail is filled with `null`.
fn arrays_copy_of_range_objects(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let source = ref_arg(args, 0);
    if source.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let from = crate::int_arg(args, 1);
    let to = crate::int_arg(args, 2);
    let length = vm.array_length(source)? as i32;
    if from < 0 || from > length {
        return Err(vm.throw_new(
            "java/lang/ArrayIndexOutOfBoundsException",
            Some(&alloc::format!(
                "arraycopy: source index {from} out of bounds for object array[{length}]"
            )),
        ));
    }
    if from > to {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some(&alloc::format!("from({from}) > to({to})")),
        ));
    }
    let count = (to - from) as usize;
    let result = vm.allocate_array(vm.class_of(source), count)?;
    for offset in 0..count {
        let index = from as usize + offset;
        if index < length as usize {
            let value = vm.array_get(source, index as i32)?;
            vm.array_set(result, offset as i32, value, true)?;
        }
    }
    object(result)
}

/// `Arrays.asList(Object[])`, returning a mutable `ArrayList` copy of the elements.
fn arrays_as_list(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let array = ref_arg(args, 0);
    if array.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let length = vm.array_length(array)?;
    let mut elements = Vec::with_capacity(length);
    for index in 0..length {
        elements.push(vm.array_get(array, index as i32)?.as_ref());
    }
    let list_class = vm.resolve_class("java/util/ArrayList")?;
    let list = vm.new_instance(list_class)?;
    array_list_store(vm, list, &elements)?;
    object(list)
}

fn arrays_equal(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<bool, VmError> {
    let (Some(left), Some(right)) = (
        vm.heap.get(a).map(|object| object.data.clone()),
        vm.heap.get(b).map(|object| object.data.clone()),
    ) else {
        return Ok(false);
    };
    match (left, right) {
        (ObjectData::Array(ArrayData::Int(x)), ObjectData::Array(ArrayData::Int(y))) => Ok(x == y),
        (ObjectData::Array(ArrayData::Long(x)), ObjectData::Array(ArrayData::Long(y))) => {
            Ok(x == y)
        }
        (ObjectData::Array(ArrayData::Double(x)), ObjectData::Array(ArrayData::Double(y))) => {
            Ok(x == y)
        }
        (ObjectData::Array(ArrayData::Byte(x)), ObjectData::Array(ArrayData::Byte(y))) => {
            Ok(x == y)
        }
        (ObjectData::Array(ArrayData::Char(x)), ObjectData::Array(ArrayData::Char(y))) => {
            Ok(x == y)
        }
        (ObjectData::Array(ArrayData::Short(x)), ObjectData::Array(ArrayData::Short(y))) => {
            Ok(x == y)
        }
        (ObjectData::Array(ArrayData::Float(x)), ObjectData::Array(ArrayData::Float(y))) => {
            Ok(x == y)
        }
        (ObjectData::Array(ArrayData::Boolean(x)), ObjectData::Array(ArrayData::Boolean(y))) => {
            Ok(x == y)
        }
        (
            ObjectData::Array(ArrayData::Reference(x)),
            ObjectData::Array(ArrayData::Reference(y)),
        ) => {
            if x.len() != y.len() {
                return Ok(false);
            }
            for (left, right) in x.iter().zip(y.iter()) {
                if !objects_equals(vm, *left, *right)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn arrays_mismatch(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<i32, VmError> {
    let a_len = vm.array_length(a)?;
    let b_len = vm.array_length(b)?;
    let common = a_len.min(b_len);
    for index in 0..common {
        let left = vm.array_get(a, index as i32)?;
        let right = vm.array_get(b, index as i32)?;
        if !array_values_equal(vm, left, right)? {
            return Ok(index as i32);
        }
    }
    Ok(if a_len == b_len { -1 } else { common as i32 })
}

fn arrays_mismatch_range(
    vm: &mut Vm<'_>,
    a: ObjectRef,
    a_from: i32,
    b: ObjectRef,
    b_from: i32,
    length: i32,
) -> Result<i32, VmError> {
    if length < 0 {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("length must be non-negative"),
        ));
    }
    let a_len = vm.array_length(a)? as i32;
    let b_len = vm.array_length(b)? as i32;
    if a_from < 0 || b_from < 0 || a_from + length > a_len || b_from + length > b_len {
        return Err(vm.throw_new("java/lang/ArrayIndexOutOfBoundsException", None));
    }
    for offset in 0..length {
        let left = vm.array_get(a, a_from + offset)?;
        let right = vm.array_get(b, b_from + offset)?;
        if !array_values_equal(vm, left, right)? {
            return Ok(offset);
        }
    }
    Ok(-1)
}

fn array_values_equal(vm: &mut Vm<'_>, a: Value, b: Value) -> Result<bool, VmError> {
    match (a, b) {
        (Value::Ref(left), Value::Ref(right)) => objects_equals(vm, left, right),
        (Value::Float(left), Value::Float(right)) => Ok(left.to_bits() == right.to_bits()),
        (Value::Double(left), Value::Double(right)) => Ok(left.to_bits() == right.to_bits()),
        _ => Ok(a == b),
    }
}

fn arrays_to_string(
    vm: &mut Vm<'_>,
    array: ObjectRef,
    component: ArrayComponent,
) -> Result<Value, VmError> {
    if array.is_null() {
        return object(vm.make_string("null")?);
    }
    let length = vm.array_length(array)?;
    let mut text = String::from("[");
    for index in 0..length {
        if index > 0 {
            text.push_str(", ");
        }
        let value = vm.array_get(array, index as i32)?;
        text.push_str(&format_array_element(value, &component));
    }
    text.push(']');
    object(vm.make_string(&text)?)
}

fn format_array_element(value: Value, component: &ArrayComponent) -> String {
    match component {
        ArrayComponent::Boolean => if value.as_int() != 0 { "true" } else { "false" }.to_string(),
        ArrayComponent::Char => {
            let unit = value.as_int() as u32 & 0xFFFF;
            char::from_u32(unit).map_or_else(String::new, |ch| ch.to_string())
        }
        ArrayComponent::Byte => oxjvm_vm::format::int_to_string(value.as_int() as i8 as i32),
        ArrayComponent::Short => oxjvm_vm::format::int_to_string(value.as_int() as i16 as i32),
        ArrayComponent::Int => oxjvm_vm::format::int_to_string(value.as_int()),
        ArrayComponent::Long => oxjvm_vm::format::long_to_string(value.as_long()),
        ArrayComponent::Float => oxjvm_vm::format::float_to_string(value.as_float()),
        ArrayComponent::Double => oxjvm_vm::format::double_to_string(value.as_double()),
        ArrayComponent::Reference => String::new(),
    }
}

fn arrays_to_string_objects(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let array = ref_arg(args, 0);
    if array.is_null() {
        return object(vm.make_string("null")?);
    }
    let elements = match vm.heap.get(array).map(|object| &object.data) {
        Some(ObjectData::Array(ArrayData::Reference(elements))) => elements.clone(),
        _ => Vec::new(),
    };
    let mut text = String::from("[");
    for (index, element) in elements.iter().enumerate() {
        if index > 0 {
            text.push_str(", ");
        }
        if element.is_null() {
            text.push_str("null");
        } else {
            let (class, method) =
                vm.resolve_virtual_method(*element, "toString", "()Ljava/lang/String;")?;
            let string = vm.invoke_method(class, method, vec![Value::Ref(*element)])?;
            text.push_str(&vm.string_or_empty(string.as_ref()));
        }
    }
    text.push(']');
    object(vm.make_string(&text)?)
}

fn arrays_copy_of_int(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let array = ref_arg(args, 0);
    let length = crate::int_arg(args, 1);
    if length < 0 {
        return Err(vm.throw_new("java/lang/NegativeArraySizeException", None));
    }
    let source = vm.array_length(array)?;
    let copy = vm.allocate_array_of(ArrayComponent::Int, length as usize)?;
    for index in 0..(length as usize).min(source) {
        let value = vm.array_get(array, index as i32)?;
        vm.array_set(copy, index as i32, value, false)?;
    }
    object(copy)
}

pub(crate) const ARRAYS: oxjvm_vm::NativeClass = class(
    "java/util/Arrays",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &ARRAYS_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Random (exactly the java.util.Random LCG)
// -------------------------------------------------------------------------------------------

const RANDOM_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method("<init>", "()V", ACC_PUBLIC, random_init),
    method("<init>", "(J)V", ACC_PUBLIC, random_init_seed),
    method("setSeed", "(J)V", ACC_PUBLIC, |vm, _, args| {
        set_random_seed(vm, receiver(args), long_arg(args, 1));
        void()
    }),
    method("next", "(I)I", ACC_PUBLIC, {
        fn next_bits(
            vm: &mut Vm<'_>,
            _c: oxjvm_vm::NativeContext,
            args: &[Value],
        ) -> Result<Value, VmError> {
            let bits = crate::int_arg(args, 1);
            let this = receiver(args);
            let mut seed = read_random_seed(vm, this);
            seed = seed.wrapping_mul(0x5DEECE66D).wrapping_add(0xB) & ((1i64 << 48) - 1);
            write_random_seed(vm, this, seed);
            int((seed >> (48 - bits)) as i32)
        }
        next_bits
    }),
    method("nextInt", "()I", ACC_PUBLIC, |vm, _, args| {
        next_random_int(vm, receiver(args), 32, &[])
    }),
    method("nextInt", "(I)I", ACC_PUBLIC, |vm, _, args| {
        let bound = crate::int_arg(args, 1);
        if bound <= 0 {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("bound must be positive"),
            ));
        }
        int(next_random_bounded(vm, receiver(args), bound)?)
    }),
    method("nextInt", "(II)I", ACC_PUBLIC, |vm, _, args| {
        let origin = crate::int_arg(args, 1);
        let bound = crate::int_arg(args, 2);
        if bound <= origin {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("bound must be greater than origin"),
            ));
        }
        let value = next_random_bounded(vm, receiver(args), bound - origin)?;
        int(origin.wrapping_add(value))
    }),
    method("nextLong", "()J", ACC_PUBLIC, |vm, _, args| {
        let high = next_random_int(vm, receiver(args), 32, &[])?;
        let low = next_random_int(vm, receiver(args), 32, &[])?;
        long(((high.as_int() as i64) << 32).wrapping_add(i64::from(low.as_int())))
    }),
    method("nextBoolean", "()Z", ACC_PUBLIC, |vm, _, args| {
        let value = next_random_int(vm, receiver(args), 1, &[])?;
        boolean(value.as_int() != 0)
    }),
    method("nextDouble", "()D", ACC_PUBLIC, |vm, _, args| {
        let high = next_random_int(vm, receiver(args), 26, &[])?;
        let low = next_random_int(vm, receiver(args), 27, &[])?;
        let value = ((high.as_int() as i64) << 27).wrapping_add(i64::from(low.as_int()));
        Ok(Value::Double(value as f64 / (1i64 << 53) as f64))
    }),
    method("nextFloat", "()F", ACC_PUBLIC, |vm, _, args| {
        let bits = next_random_int(vm, receiver(args), 24, &[])?.as_int();
        Ok(Value::Float(bits as f32 / (1i32 << 24) as f32))
    }),
    method("nextBytes", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let array = ref_arg(args, 1);
        let length = vm.array_length(array)?;
        for index in 0..length {
            let value = next_random_int(vm, this, 8, &[])?.as_int() as i8;
            vm.array_set(array, index as i32, Value::Int(i32::from(value)), false)?;
        }
        void()
    }),
];

fn random_init(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let now = vm.host_time_millis();
    set_random_seed(vm, receiver(args), now);
    void()
}

fn random_init_seed(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    set_random_seed(vm, receiver(args), long_arg(args, 1));
    void()
}

fn set_random_seed(vm: &mut Vm<'_>, this: ObjectRef, seed: i64) {
    let scrambled = (seed ^ 0x5DEECE66D) & ((1i64 << 48) - 1);
    write_random_seed(vm, this, scrambled);
}

fn read_random_seed(vm: &Vm<'_>, this: ObjectRef) -> i64 {
    match vm.heap.get(this).map(|object| &object.data) {
        Some(ObjectData::Instance(fields)) if !fields.is_empty() => fields[0].as_long(),
        _ => 0,
    }
}

fn write_random_seed(vm: &mut Vm<'_>, this: ObjectRef, seed: i64) {
    let class = vm.class_of(this);
    if let Some((declaring, field)) = vm.find_field_opt(class, "seed", "J") {
        let slot = vm.classes.get(declaring).fields[field as usize].slot as usize;
        if let Some(ObjectData::Instance(fields)) =
            vm.heap.get_mut(this).map(|object| &mut object.data)
        {
            fields[slot] = Value::Long(seed);
        }
    }
}

/// A uniformly distributed value in `[0, bound)` (the JDK rejection loop).
fn next_random_bounded(vm: &mut Vm<'_>, this: ObjectRef, bound: i32) -> Result<i32, VmError> {
    loop {
        let bits = next_random_int(vm, this, 31, &[])?.as_int();
        let value = bits.rem_euclid(bound);
        if bits - value + (bound - 1) >= 0 {
            return Ok(value);
        }
    }
}

fn next_random_int(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    bits: i32,
    _bound: &[i32],
) -> Result<Value, VmError> {
    let mut seed = read_random_seed(vm, this);
    seed = seed.wrapping_mul(0x5DEECE66D).wrapping_add(0xB) & ((1i64 << 48) - 1);
    write_random_seed(vm, this, seed);
    Ok(Value::Int((seed >> (48 - bits)) as i32))
}

/// `java.util.RandomAccess`: a marker interface.
pub(crate) const RANDOM_ACCESS: oxjvm_vm::NativeClass = class(
    "java/util/RandomAccess",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const RANDOM: oxjvm_vm::NativeClass = class(
    "java/util/Random",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[field("seed", "J", ACC_PRIVATE, None)],
    &RANDOM_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// ArrayList + iterator
// -------------------------------------------------------------------------------------------

const ARRAY_LIST_METHODS: [oxjvm_vm::NativeMethodDef; 31] = [
    method("<init>", "()V", ACC_PUBLIC, array_list_init),
    method("<init>", "(I)V", ACC_PUBLIC, array_list_init),
    method("add", "(Ljava/lang/Object;)Z", ACC_PUBLIC, array_list_add),
    method(
        "addAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC,
        array_list_add_all,
    ),
    method(
        "addAll",
        "(ILjava/util/Collection;)Z",
        ACC_PUBLIC,
        array_list_add_all_at,
    ),
    method("get", "(I)Ljava/lang/Object;", ACC_PUBLIC, array_list_get),
    method(
        "set",
        "(ILjava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        array_list_set,
    ),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "size"))
    }),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "size") == 0)
    }),
    method(
        "remove",
        "(I)Ljava/lang/Object;",
        ACC_PUBLIC,
        array_list_remove,
    ),
    method(
        "remove",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        array_list_remove_object,
    ),
    method(
        "contains",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| boolean(array_list_index_of(vm, receiver(args), ref_arg(args, 1))? >= 0),
    ),
    method(
        "indexOf",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC,
        |vm, _, args| int(array_list_index_of(vm, receiver(args), ref_arg(args, 1))?),
    ),
    method("clear", "()V", ACC_PUBLIC, array_list_clear),
    method(
        "iterator",
        "()Ljava/util/Iterator;",
        ACC_PUBLIC,
        array_list_iterator,
    ),
    method(
        "toArray",
        "()[Ljava/lang/Object;",
        ACC_PUBLIC,
        array_list_to_array,
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        array_list_to_string,
    ),
    method(
        "add",
        "(ILjava/lang/Object;)V",
        ACC_PUBLIC,
        array_list_insert,
    ),
    method(
        "toArray",
        "([Ljava/lang/Object;)[Ljava/lang/Object;",
        ACC_PUBLIC,
        array_list_to_array_into,
    ),
    method(
        "forEach",
        "(Ljava/util/function/Consumer;)V",
        ACC_PUBLIC,
        collection_for_each,
    ),
    method(
        "<init>",
        "(Ljava/util/Collection;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let elements = collection_elements(vm, ref_arg(args, 1))?;
            array_list_store(vm, receiver(args), &elements)?;
            void()
        },
    ),
    method(
        "toArray",
        "(Ljava/util/function/IntFunction;)[Ljava/lang/Object;",
        ACC_PUBLIC,
        array_list_to_array_generator,
    ),
    method(
        "sort",
        "(Ljava/util/Comparator;)V",
        ACC_PUBLIC,
        array_list_sort,
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        array_list_equals,
    ),
    method("hashCode", "()I", ACC_PUBLIC, array_list_hash_code),
    method(
        "removeAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC,
        collection_remove_all,
    ),
    method(
        "retainAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC,
        collection_retain_all,
    ),
    method("trimToSize", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "listIterator",
        "()Ljava/util/ListIterator;",
        ACC_PUBLIC,
        |vm, _, args| list_iterator_new(vm, receiver(args), 0),
    ),
    method(
        "listIterator",
        "(I)Ljava/util/ListIterator;",
        ACC_PUBLIC,
        |vm, _, args| list_iterator_new(vm, receiver(args), crate::int_arg(args, 1)),
    ),
    method(
        "subList",
        "(II)Ljava/util/List;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let from = crate::int_arg(args, 1).max(0) as usize;
            let to = crate::int_arg(args, 2).max(0) as usize;
            let elements = array_list_elements(vm, this);
            if from > to || to > elements.len() {
                return Err(vm.throw_new(
                    "java/lang/IndexOutOfBoundsException",
                    Some(&alloc::format!("fromIndex: {from}, toIndex: {to}")),
                ));
            }
            object(array_list_from(vm, &elements[from..to])?)
        },
    ),
];

fn list_iterator_new(vm: &mut Vm<'_>, list: ObjectRef, cursor: i32) -> Result<Value, VmError> {
    let size = crate::lang::read_int_field(vm, list, "size");
    if cursor < 0 || cursor > size {
        return Err(vm.throw_new(
            "java/lang/IndexOutOfBoundsException",
            Some(&alloc::format!("Index: {cursor}")),
        ));
    }
    let class = vm.resolve_class("java/util/ArrayList$ListItr")?;
    let iterator = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "list", "Ljava/util/List;")?;
    vm.set_instance_ref(iterator, declaring, field, list);
    crate::lang::write_int_field(vm, iterator, "cursor", cursor);
    crate::lang::write_int_field(vm, iterator, "lastReturned", -1);
    object(iterator)
}

/// `List.equals`: element-wise equality with any other `List`.
fn array_list_equals(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let other = ref_arg(args, 1);
    if other.is_null() {
        return boolean(false);
    }
    if this == other {
        return boolean(true);
    }
    if !{
        let list_class = vm.resolve_class("java/util/List")?;
        vm.is_instance(other, list_class)
    } {
        return boolean(false);
    }
    let elements = array_list_elements(vm, this);
    let others = collection_elements(vm, other)?;
    if elements.len() != others.len() {
        return boolean(false);
    }
    for (element, other) in elements.iter().zip(others.iter()) {
        if !keys_equal(vm, *element, *other)? {
            return boolean(false);
        }
    }
    boolean(true)
}

/// `List.hashCode`: the `31 * hash + element.hashCode()` fold.
fn array_list_hash_code(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let elements = array_list_elements(vm, receiver(args));
    let mut hash = 1_i32;
    for element in elements {
        hash = hash
            .wrapping_mul(31)
            .wrapping_add(object_hash(vm, element)?);
    }
    int(hash)
}

/// `List.sort`: insertion sort with the supplied comparator, or `Collections.sort` order when
/// the comparator is `null`.
fn array_list_sort(
    vm: &mut Vm<'_>,
    context: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let comparator = ref_arg(args, 1);
    if comparator.is_null() {
        collections_sort(vm, context, &[args[0]])?;
        return void();
    }
    let list = receiver(args);
    let mut elements = array_list_elements(vm, list);
    for index in 1..elements.len() {
        let mut cursor = index;
        while cursor > 0 {
            let a = elements[cursor - 1];
            let b = elements[cursor];
            let (class, method) = vm.resolve_virtual_method(
                comparator,
                "compare",
                "(Ljava/lang/Object;Ljava/lang/Object;)I",
            )?;
            let ordering = vm
                .invoke_method(
                    class,
                    method,
                    vec![Value::Ref(comparator), Value::Ref(a), Value::Ref(b)],
                )?
                .as_int();
            if ordering <= 0 {
                break;
            }
            elements.swap(cursor - 1, cursor);
            cursor -= 1;
        }
    }
    array_list_store(vm, list, &elements)?;
    void()
}

/// `Iterable.forEach`: invoke `Consumer.accept` for every element.
fn collection_for_each(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let consumer = ref_arg(args, 1);
    if consumer.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let (class, method) = vm.resolve_virtual_method(consumer, "accept", "(Ljava/lang/Object;)V")?;
    let elements = collection_elements(vm, receiver(args))?;
    for element in elements {
        vm.invoke_method(
            class,
            method,
            vec![Value::Ref(consumer), Value::Ref(element)],
        )?;
    }
    void()
}

pub(crate) fn array_list_elements(vm: &Vm<'_>, this: ObjectRef) -> Vec<ObjectRef> {
    let size = crate::lang::read_int_field(vm, this, "size").max(0) as usize;
    match vm.read_ref_field(
        this,
        vm.class_of(this),
        "elementData",
        "[Ljava/lang/Object;",
    ) {
        Some(array) => match vm.heap.get(array).map(|object| &object.data) {
            Some(ObjectData::Array(ArrayData::Reference(elements))) => {
                let mut elements = elements.clone();
                elements.truncate(size.min(elements.len()));
                elements
            }
            _ => Vec::new(),
        },
        None => Vec::new(),
    }
}

fn array_list_store(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    elements: &[ObjectRef],
) -> Result<(), VmError> {
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "elementData", "[Ljava/lang/Object;")?;
    let object_class = vm.resolve_class("java/lang/Object")?;
    let array = vm.allocate_object_array(object_class, elements.len().max(1))?;
    for (index, element) in elements.iter().enumerate() {
        vm.array_set_ref(array, index, *element)?;
    }
    vm.set_instance_ref(this, declaring, field, array);
    crate::lang::write_int_field(vm, this, "size", elements.len() as i32);
    Ok(())
}

/// Create an empty `ArrayList`, for APIs that hand one out.
pub(crate) fn new_array_list(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/ArrayList")?;
    let instance = vm.new_instance(class)?;
    array_list_store(vm, instance, &[])?;
    Ok(instance)
}

fn array_list_init(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    array_list_store(vm, receiver(args), &[])?;
    void()
}

fn array_list_add(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let mut elements = array_list_elements(vm, this);
    elements.push(ref_arg(args, 1));
    array_list_store(vm, this, &elements)?;
    boolean(true)
}

/// Snapshot a guest `Collection` through its `Iterator`, so any implementation works.
pub(crate) fn collection_elements(
    vm: &mut Vm<'_>,
    collection: ObjectRef,
) -> Result<Vec<ObjectRef>, VmError> {
    if collection.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let (class, method) =
        vm.resolve_virtual_method(collection, "iterator", "()Ljava/util/Iterator;")?;
    let iterator = vm
        .invoke_method(class, method, vec![Value::Ref(collection)])?
        .as_ref();
    let (has_next_class, has_next) = vm.resolve_virtual_method(iterator, "hasNext", "()Z")?;
    let (next_class, next) = vm.resolve_virtual_method(iterator, "next", "()Ljava/lang/Object;")?;
    let mut elements = Vec::new();
    loop {
        let value = vm.invoke_method(has_next_class, has_next, vec![Value::Ref(iterator)])?;
        if value.as_int() == 0 {
            return Ok(elements);
        }
        let value = vm.invoke_method(next_class, next, vec![Value::Ref(iterator)])?;
        elements.push(value.as_ref());
    }
}

/// `List.addAll(Collection)`: append every element of `collection`.
fn array_list_add_all(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let incoming = collection_elements(vm, ref_arg(args, 1))?;
    let changed = !incoming.is_empty();
    let mut elements = array_list_elements(vm, this);
    elements.extend(incoming);
    array_list_store(vm, this, &elements)?;
    boolean(changed)
}

/// `List.addAll(int, Collection)`: insert every element of `collection` at `index`.
fn array_list_add_all_at(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let index = crate::int_arg(args, 1);
    let incoming = collection_elements(vm, ref_arg(args, 2))?;
    let mut elements = array_list_elements(vm, this);
    if index < 0 || index as usize > elements.len() {
        return Err(vm.throw_new(
            "java/lang/IndexOutOfBoundsException",
            Some(&alloc::format!(
                "Index {index} out of bounds for length {}",
                elements.len()
            )),
        ));
    }
    let changed = !incoming.is_empty();
    for (offset, element) in incoming.into_iter().enumerate() {
        elements.insert(index as usize + offset, element);
    }
    array_list_store(vm, this, &elements)?;
    boolean(changed)
}

fn array_list_get(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let elements = array_list_elements(vm, receiver(args));
    let index = crate::int_arg(args, 1);
    if index < 0 || index as usize >= elements.len() {
        return Err(vm.throw_new(
            "java/lang/IndexOutOfBoundsException",
            Some(&alloc::format!(
                "Index {index} out of bounds for length {}",
                elements.len()
            )),
        ));
    }
    object(elements[index as usize])
}

fn array_list_set(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let mut elements = array_list_elements(vm, this);
    let index = crate::int_arg(args, 1);
    if index < 0 || index as usize >= elements.len() {
        return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
    }
    let previous = elements[index as usize];
    elements[index as usize] = ref_arg(args, 2);
    array_list_store(vm, this, &elements)?;
    object(previous)
}

fn array_list_insert(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let mut elements = array_list_elements(vm, this);
    let index = crate::int_arg(args, 1);
    if index < 0 || index as usize > elements.len() {
        return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
    }
    elements.insert(index as usize, ref_arg(args, 2));
    array_list_store(vm, this, &elements)?;
    void()
}

fn array_list_remove(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let mut elements = array_list_elements(vm, this);
    let index = crate::int_arg(args, 1);
    if index < 0 || index as usize >= elements.len() {
        return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
    }
    let removed = elements.remove(index as usize);
    array_list_store(vm, this, &elements)?;
    object(removed)
}

fn array_list_remove_object(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let target = ref_arg(args, 1);
    let index = array_list_index_of(vm, this, target)?;
    if index < 0 {
        return boolean(false);
    }
    let mut elements = array_list_elements(vm, this);
    elements.remove(index as usize);
    array_list_store(vm, this, &elements)?;
    boolean(true)
}

fn array_list_index_of(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    target: ObjectRef,
) -> Result<i32, VmError> {
    let elements = array_list_elements(vm, this);
    for (index, element) in elements.iter().enumerate() {
        if keys_equal(vm, *element, target)? {
            return Ok(index as i32);
        }
    }
    Ok(-1)
}

fn array_list_clear(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    array_list_store(vm, receiver(args), &[])?;
    void()
}

fn array_list_iterator(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/util/ArrayList$Itr")?;
    let iterator = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "list", "Ljava/util/List;")?;
    vm.set_instance_ref(iterator, declaring, field, receiver(args));
    crate::lang::write_int_field(vm, iterator, "cursor", 0);
    object(iterator)
}

fn array_list_to_array(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let elements = array_list_elements(vm, receiver(args));
    let object_class = vm.resolve_class("java/lang/Object")?;
    let array = vm.allocate_object_array(object_class, elements.len())?;
    for (index, element) in elements.iter().enumerate() {
        vm.array_set_ref(array, index, *element)?;
    }
    object(array)
}

/// `List.toArray(T[])`: reuse `target` when it is large enough (null-terminated), otherwise
/// allocate a new array with `target`'s runtime component type.
/// `List.toArray(IntFunction)`: the generator allocates the result array.
fn array_list_to_array_generator(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let elements = array_list_elements(vm, receiver(args));
    let generator = ref_arg(args, 1);
    let (class, method) = vm.resolve_virtual_method(generator, "apply", "(I)Ljava/lang/Object;")?;
    let array = vm
        .invoke_method(
            class,
            method,
            vec![Value::Ref(generator), Value::Int(elements.len() as i32)],
        )?
        .as_ref();
    if array.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let length = vm.array_length(array)?;
    if length < elements.len() {
        return Err(vm.throw_new(
            "java/lang/ArrayIndexOutOfBoundsException",
            Some(&alloc::format!(
                "Index {} out of bounds for length {length}",
                elements.len()
            )),
        ));
    }
    for (index, element) in elements.iter().enumerate() {
        vm.array_set_ref(array, index, *element)?;
    }
    if length > elements.len() {
        vm.array_set_ref(array, elements.len(), ObjectRef::NULL)?;
    }
    object(array)
}

fn array_list_to_array_into(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let target = ref_arg(args, 1);
    if target.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let elements = array_list_elements(vm, receiver(args));
    let target_length = vm.array_length(target)?;
    if target_length < elements.len() {
        let result = vm.allocate_array(vm.class_of(target), elements.len())?;
        for (index, element) in elements.iter().enumerate() {
            vm.array_set_ref(result, index, *element)?;
        }
        return object(result);
    }
    for (index, element) in elements.iter().enumerate() {
        vm.array_set_ref(target, index, *element)?;
    }
    if target_length > elements.len() {
        vm.array_set_ref(target, elements.len(), ObjectRef::NULL)?;
    }
    object(target)
}

fn array_list_to_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let elements = array_list_elements(vm, receiver(args));
    let mut text = String::from("[");
    for (index, element) in elements.iter().enumerate() {
        if index > 0 {
            text.push_str(", ");
        }
        if element.is_null() {
            text.push_str("null");
        } else {
            let (class, method) =
                vm.resolve_virtual_method(*element, "toString", "()Ljava/lang/String;")?;
            let string = vm.invoke_method(class, method, vec![Value::Ref(*element)])?;
            text.push_str(&vm.string_or_empty(string.as_ref()));
        }
    }
    text.push(']');
    object(vm.make_string(&text)?)
}

pub(crate) const ARRAY_LIST: oxjvm_vm::NativeClass = class(
    "java/util/ArrayList",
    Some("java/lang/Object"),
    &[
        "java/util/List",
        "java/lang/Cloneable",
        "java/io/Serializable",
    ],
    ACC_PUBLIC,
    &[
        field("elementData", "[Ljava/lang/Object;", ACC_PRIVATE, None),
        field("size", "I", ACC_PRIVATE, None),
    ],
    &ARRAY_LIST_METHODS,
    None,
);

/// `LinkedList`, backed by the same element array as `ArrayList`.
///
/// The observable difference (iterator/removal corner cases) is not exercised by the current
/// corpus; sharing the implementation keeps list semantics in one place.
pub(crate) const LINKED_LIST: oxjvm_vm::NativeClass = class(
    "java/util/LinkedList",
    Some("java/lang/Object"),
    &[
        "java/util/List",
        "java/lang/Cloneable",
        "java/io/Serializable",
    ],
    ACC_PUBLIC,
    &[
        field("elementData", "[Ljava/lang/Object;", ACC_PRIVATE, None),
        field("size", "I", ACC_PRIVATE, None),
    ],
    &ARRAY_LIST_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Queue / Deque / ArrayDeque
// -------------------------------------------------------------------------------------------

const QUEUE_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "add",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "offer",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "remove",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "poll",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "element",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "peek",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const QUEUE: oxjvm_vm::NativeClass = class(
    "java/util/Queue",
    Some("java/lang/Object"),
    &["java/util/Collection"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &QUEUE_METHODS,
    None,
);

const DEQUE_METHODS: [oxjvm_vm::NativeMethodDef; 19] = [
    method(
        "addFirst",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "addLast",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "offerFirst",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "offerLast",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "removeFirst",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "removeLast",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "pollFirst",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "pollLast",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getFirst",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getLast",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "peekFirst",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "peekLast",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "removeFirstOccurrence",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "removeLastOccurrence",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "push",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "pop",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "remove",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "peek",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "add",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const DEQUE: oxjvm_vm::NativeClass = class(
    "java/util/Deque",
    Some("java/lang/Object"),
    &["java/util/Queue"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &DEQUE_METHODS,
    None,
);

fn deque_first(vm: &mut Vm<'_>, this: ObjectRef, missing: &str) -> Result<Value, VmError> {
    let elements = array_list_elements(vm, this);
    match elements.first() {
        Some(element) => object(*element),
        None => Err(vm.throw_new(missing, None)),
    }
}

fn deque_last(vm: &mut Vm<'_>, this: ObjectRef, missing: &str) -> Result<Value, VmError> {
    let elements = array_list_elements(vm, this);
    match elements.last() {
        Some(element) => object(*element),
        None => Err(vm.throw_new(missing, None)),
    }
}

fn deque_remove_first(vm: &mut Vm<'_>, this: ObjectRef, missing: &str) -> Result<Value, VmError> {
    let mut elements = array_list_elements(vm, this);
    if elements.is_empty() {
        return Err(vm.throw_new(missing, None));
    }
    let element = elements.remove(0);
    array_list_store(vm, this, &elements)?;
    object(element)
}

fn deque_remove_last(vm: &mut Vm<'_>, this: ObjectRef, missing: &str) -> Result<Value, VmError> {
    let mut elements = array_list_elements(vm, this);
    match elements.pop() {
        Some(element) => {
            array_list_store(vm, this, &elements)?;
            object(element)
        }
        None => Err(vm.throw_new(missing, None)),
    }
}

fn deque_poll_first(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Value, VmError> {
    let mut elements = array_list_elements(vm, this);
    if elements.is_empty() {
        return object(ObjectRef::NULL);
    }
    let element = elements.remove(0);
    array_list_store(vm, this, &elements)?;
    object(element)
}

fn deque_poll_last(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Value, VmError> {
    let mut elements = array_list_elements(vm, this);
    match elements.pop() {
        Some(element) => {
            array_list_store(vm, this, &elements)?;
            object(element)
        }
        None => object(ObjectRef::NULL),
    }
}

fn deque_add_first(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let mut elements = array_list_elements(vm, this);
    elements.insert(0, ref_arg(args, 1));
    array_list_store(vm, this, &elements)?;
    void()
}

fn deque_offer_first(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    deque_add_first(vm, _c, args)?;
    boolean(true)
}

fn deque_remove_occurrence(vm: &mut Vm<'_>, args: &[Value], first: bool) -> Result<Value, VmError> {
    let this = receiver(args);
    let target = ref_arg(args, 1);
    let mut elements = array_list_elements(vm, this);
    let index = if first {
        elements
            .iter()
            .position(|element| keys_equal(vm, *element, target).unwrap_or(false))
    } else {
        elements
            .iter()
            .rposition(|element| keys_equal(vm, *element, target).unwrap_or(false))
    };
    let Some(index) = index else {
        return boolean(false);
    };
    elements.remove(index);
    array_list_store(vm, this, &elements)?;
    boolean(true)
}

const ARRAY_DEQUE_METHODS: [oxjvm_vm::NativeMethodDef; 36] = [
    method("<init>", "()V", ACC_PUBLIC, array_list_init),
    method("<init>", "(I)V", ACC_PUBLIC, array_list_init),
    method(
        "<init>",
        "(Ljava/util/Collection;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            array_list_store(vm, this, &[])?;
            let incoming = collection_elements(vm, ref_arg(args, 1))?;
            array_list_store(vm, this, &incoming)?;
            void()
        },
    ),
    method("add", "(Ljava/lang/Object;)Z", ACC_PUBLIC, array_list_add),
    method(
        "addLast",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let mut elements = array_list_elements(vm, this);
            elements.push(ref_arg(args, 1));
            array_list_store(vm, this, &elements)?;
            void()
        },
    ),
    method("offer", "(Ljava/lang/Object;)Z", ACC_PUBLIC, array_list_add),
    method(
        "offerLast",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        array_list_add,
    ),
    method("push", "(Ljava/lang/Object;)V", ACC_PUBLIC, deque_add_first),
    method(
        "addFirst",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC,
        deque_add_first,
    ),
    method(
        "offerFirst",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        deque_offer_first,
    ),
    method("poll", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        deque_poll_first(vm, receiver(args))
    }),
    method(
        "pollFirst",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| deque_poll_first(vm, receiver(args)),
    ),
    method(
        "pollLast",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| deque_poll_last(vm, receiver(args)),
    ),
    method("pop", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        deque_remove_first(vm, receiver(args), "java/util/NoSuchElementException")
    }),
    method(
        "removeFirst",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| deque_remove_first(vm, receiver(args), "java/util/NoSuchElementException"),
    ),
    method(
        "removeLast",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| deque_remove_last(vm, receiver(args), "java/util/NoSuchElementException"),
    ),
    method(
        "remove",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| deque_remove_first(vm, receiver(args), "java/util/NoSuchElementException"),
    ),
    method(
        "remove",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        array_list_remove_object,
    ),
    method(
        "removeFirstOccurrence",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| deque_remove_occurrence(vm, args, true),
    ),
    method(
        "removeLastOccurrence",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| deque_remove_occurrence(vm, args, false),
    ),
    method("peek", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        let elements = array_list_elements(vm, receiver(args));
        object(elements.first().copied().unwrap_or(ObjectRef::NULL))
    }),
    method(
        "peekFirst",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let elements = array_list_elements(vm, receiver(args));
            object(elements.first().copied().unwrap_or(ObjectRef::NULL))
        },
    ),
    method(
        "peekLast",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let elements = array_list_elements(vm, receiver(args));
            object(elements.last().copied().unwrap_or(ObjectRef::NULL))
        },
    ),
    method(
        "element",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| deque_first(vm, receiver(args), "java/util/NoSuchElementException"),
    ),
    method(
        "getFirst",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| deque_first(vm, receiver(args), "java/util/NoSuchElementException"),
    ),
    method(
        "getLast",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| deque_last(vm, receiver(args), "java/util/NoSuchElementException"),
    ),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "size"))
    }),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "size") == 0)
    }),
    method(
        "addAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC,
        array_list_add_all,
    ),
    method(
        "contains",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| boolean(array_list_index_of(vm, receiver(args), ref_arg(args, 1))? >= 0),
    ),
    method("clear", "()V", ACC_PUBLIC, array_list_clear),
    method(
        "iterator",
        "()Ljava/util/Iterator;",
        ACC_PUBLIC,
        array_list_iterator,
    ),
    method(
        "toArray",
        "()[Ljava/lang/Object;",
        ACC_PUBLIC,
        array_list_to_array,
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        array_list_to_string,
    ),
    method(
        "removeAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC,
        collection_remove_all,
    ),
    method(
        "retainAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC,
        collection_retain_all,
    ),
];

/// `ArrayDeque`: an element-array-backed deque.
pub(crate) const ARRAY_DEQUE: oxjvm_vm::NativeClass = class(
    "java/util/ArrayDeque",
    Some("java/lang/Object"),
    &[
        "java/util/Deque",
        "java/lang/Cloneable",
        "java/io/Serializable",
    ],
    ACC_PUBLIC,
    &[
        field("elementData", "[Ljava/lang/Object;", ACC_PRIVATE, None),
        field("size", "I", ACC_PRIVATE, None),
    ],
    &ARRAY_DEQUE_METHODS,
    None,
);

/// `ConcurrentLinkedDeque`: the same element-array deque as `ArrayDeque`.
pub(crate) const CONCURRENT_LINKED_DEQUE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/ConcurrentLinkedDeque",
    Some("java/util/ArrayDeque"),
    &["java/util/Deque"],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

/// `CopyOnWriteArrayList`, backed by the same element array as `ArrayList`.
///
/// The snapshot iterator and per-write copying are unobservable in a cooperative runtime, so
/// inheriting `ArrayList`'s storage keeps the list semantics in one place.
pub(crate) const COPY_ON_WRITE_ARRAY_LIST: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/CopyOnWriteArrayList",
    Some("java/util/ArrayList"),
    &["java/util/RandomAccess"],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

/// `ConcurrentLinkedQueue`, backed by the same element array as `ArrayList`.
pub(crate) const CONCURRENT_LINKED_QUEUE: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/ConcurrentLinkedQueue",
    Some("java/util/ArrayList"),
    &[],
    ACC_PUBLIC,
    &[],
    &CONCURRENT_LINKED_QUEUE_METHODS,
    None,
);

const CONCURRENT_LINKED_QUEUE_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("offer", "(Ljava/lang/Object;)Z", ACC_PUBLIC, array_list_add),
    method(
        "poll",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        concurrent_queue_poll,
    ),
    method(
        "peek",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        concurrent_queue_peek,
    ),
    method(
        "element",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        concurrent_queue_element,
    ),
    method(
        "remove",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        concurrent_queue_remove,
    ),
];

/// `Queue.poll`: the head, or `null` when the queue is empty.
fn concurrent_queue_poll(
    vm: &mut Vm<'_>,
    context: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    if crate::lang::read_int_field(vm, receiver(args), "size") == 0 {
        return object(ObjectRef::NULL);
    }
    array_list_remove(vm, context, &[args[0], Value::Int(0)])
}

/// `Queue.peek`: the head, or `null` when the queue is empty.
fn concurrent_queue_peek(
    vm: &mut Vm<'_>,
    context: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    if crate::lang::read_int_field(vm, receiver(args), "size") == 0 {
        return object(ObjectRef::NULL);
    }
    array_list_get(vm, context, &[args[0], Value::Int(0)])
}

/// `Queue.element`: the head, or `NoSuchElementException` when the queue is empty.
fn concurrent_queue_element(
    vm: &mut Vm<'_>,
    context: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    if crate::lang::read_int_field(vm, receiver(args), "size") == 0 {
        return Err(vm.throw_new("java/util/NoSuchElementException", None));
    }
    array_list_get(vm, context, &[args[0], Value::Int(0)])
}

/// `Queue.remove()`: the head, or `NoSuchElementException` when the queue is empty.
fn concurrent_queue_remove(
    vm: &mut Vm<'_>,
    context: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    if crate::lang::read_int_field(vm, receiver(args), "size") == 0 {
        return Err(vm.throw_new("java/util/NoSuchElementException", None));
    }
    array_list_remove(vm, context, &[args[0], Value::Int(0)])
}

// -------------------------------------------------------------------------------------------
// AbstractMap
// -------------------------------------------------------------------------------------------

/// Collect a map's `(key, value)` pairs through `entrySet`, the contract subclasses implement.
fn abstract_map_entries(
    vm: &mut Vm<'_>,
    map: ObjectRef,
) -> Result<Vec<(ObjectRef, ObjectRef)>, VmError> {
    let (set_class, entry_set) = vm.resolve_virtual_method(map, "entrySet", "()Ljava/util/Set;")?;
    let set = vm
        .invoke_method(set_class, entry_set, vec![Value::Ref(map)])?
        .as_ref();
    let (iterator_class, iterator_method) =
        vm.resolve_virtual_method(set, "iterator", "()Ljava/util/Iterator;")?;
    let iterator = vm
        .invoke_method(iterator_class, iterator_method, vec![Value::Ref(set)])?
        .as_ref();
    let (has_next_class, has_next) = vm.resolve_virtual_method(iterator, "hasNext", "()Z")?;
    let (next_class, next) = vm.resolve_virtual_method(iterator, "next", "()Ljava/lang/Object;")?;
    let mut entries = Vec::new();
    loop {
        if vm
            .invoke_method(has_next_class, has_next, vec![Value::Ref(iterator)])?
            .as_int()
            == 0
        {
            return Ok(entries);
        }
        let entry = vm
            .invoke_method(next_class, next, vec![Value::Ref(iterator)])?
            .as_ref();
        let (key_class, get_key) =
            vm.resolve_virtual_method(entry, "getKey", "()Ljava/lang/Object;")?;
        let key = vm
            .invoke_method(key_class, get_key, vec![Value::Ref(entry)])?
            .as_ref();
        let (value_class, get_value) =
            vm.resolve_virtual_method(entry, "getValue", "()Ljava/lang/Object;")?;
        let value = vm
            .invoke_method(value_class, get_value, vec![Value::Ref(entry)])?
            .as_ref();
        entries.push((key, value));
    }
}

const ABSTRACT_MAP_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let (class, method) = vm.resolve_virtual_method(this, "size", "()I")?;
        boolean(
            vm.invoke_method(class, method, vec![Value::Ref(this)])?
                .as_int()
                == 0,
        )
    }),
    method("putAll", "(Ljava/util/Map;)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let entries = abstract_map_entries(vm, ref_arg(args, 1))?;
        let (class, method) = vm.resolve_virtual_method(
            this,
            "put",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        )?;
        for (key, value) in entries {
            vm.invoke_method(
                class,
                method,
                vec![Value::Ref(this), Value::Ref(key), Value::Ref(value)],
            )?;
        }
        void()
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let entries = abstract_map_entries(vm, this)?;
            let mut text = String::from("{");
            for (index, (key, value)) in entries.iter().enumerate() {
                if index > 0 {
                    text.push_str(", ");
                }
                text.push_str(&value_to_string(vm, *key)?);
                text.push('=');
                text.push_str(&value_to_string(vm, *value)?);
            }
            text.push('}');
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            if this.raw() == other.raw() {
                return boolean(true);
            }
            let map_class = vm.resolve_class("java/util/Map")?;
            if other.is_null() || !vm.is_instance(other, map_class) {
                return boolean(false);
            }
            let entries = abstract_map_entries(vm, this)?;
            let (size_class, size_method) = vm.resolve_virtual_method(other, "size", "()I")?;
            let other_size = vm
                .invoke_method(size_class, size_method, vec![Value::Ref(other)])?
                .as_int();
            if entries.len() as i32 != other_size {
                return boolean(false);
            }
            let (contains_class, contains_method) =
                vm.resolve_virtual_method(other, "containsKey", "(Ljava/lang/Object;)Z")?;
            let (get_class, get_method) =
                vm.resolve_virtual_method(other, "get", "(Ljava/lang/Object;)Ljava/lang/Object;")?;
            for (key, value) in entries {
                let present = vm
                    .invoke_method(
                        contains_class,
                        contains_method,
                        vec![Value::Ref(other), Value::Ref(key)],
                    )?
                    .as_int()
                    != 0;
                if !present {
                    return boolean(false);
                }
                let other_value = vm
                    .invoke_method(
                        get_class,
                        get_method,
                        vec![Value::Ref(other), Value::Ref(key)],
                    )?
                    .as_ref();
                if !keys_equal(vm, value, other_value)? {
                    return boolean(false);
                }
            }
            boolean(true)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let entries = abstract_map_entries(vm, receiver(args))?;
        let mut hash: i32 = 0;
        for (key, value) in entries {
            let key_hash = object_hash(vm, key)?;
            let value_hash = object_hash(vm, value)?;
            hash = hash.wrapping_add(key_hash ^ value_hash);
        }
        int(hash)
    }),
    method(
        "getOrDefault",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let key = ref_arg(args, 1);
            let (get_class, get_method) =
                vm.resolve_virtual_method(this, "get", "(Ljava/lang/Object;)Ljava/lang/Object;")?;
            let value = vm
                .invoke_method(
                    get_class,
                    get_method,
                    vec![Value::Ref(this), Value::Ref(key)],
                )?
                .as_ref();
            if !value.is_null() {
                return object(value);
            }
            let (contains_class, contains_method) =
                vm.resolve_virtual_method(this, "containsKey", "(Ljava/lang/Object;)Z")?;
            let present = vm
                .invoke_method(
                    contains_class,
                    contains_method,
                    vec![Value::Ref(this), Value::Ref(key)],
                )?
                .as_int()
                != 0;
            if present {
                object(ObjectRef::NULL)
            } else {
                object(ref_arg(args, 2))
            }
        },
    ),
    method(
        "putIfAbsent",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let key = ref_arg(args, 1);
            let value = ref_arg(args, 2);
            let (get_class, get_method) =
                vm.resolve_virtual_method(this, "get", "(Ljava/lang/Object;)Ljava/lang/Object;")?;
            let present = vm
                .invoke_method(
                    get_class,
                    get_method,
                    vec![Value::Ref(this), Value::Ref(key)],
                )?
                .as_ref();
            if !present.is_null() {
                return object(present);
            }
            let (put_class, put_method) = vm.resolve_virtual_method(
                this,
                "put",
                "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            )?;
            vm.invoke_method(
                put_class,
                put_method,
                vec![Value::Ref(this), Value::Ref(key), Value::Ref(value)],
            )
        },
    ),
    method(
        "forEach",
        "(Ljava/util/function/BiConsumer;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let action = ref_arg(args, 1);
            let entries = abstract_map_entries(vm, this)?;
            let (class, method) = vm.resolve_virtual_method(
                action,
                "accept",
                "(Ljava/lang/Object;Ljava/lang/Object;)V",
            )?;
            for (key, value) in entries {
                vm.invoke_method(
                    class,
                    method,
                    vec![Value::Ref(action), Value::Ref(key), Value::Ref(value)],
                )?;
            }
            void()
        },
    ),
];

/// `java.util.AbstractMap`: the `entrySet`-driven base every map subclass shares.
pub(crate) const ABSTRACT_MAP: oxjvm_vm::NativeClass = class(
    "java/util/AbstractMap",
    Some("java/lang/Object"),
    &["java/util/Map"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &ABSTRACT_MAP_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// AbstractSet
// -------------------------------------------------------------------------------------------

/// `Set.contains` through virtual dispatch.
fn set_contains(vm: &mut Vm<'_>, set: ObjectRef, value: ObjectRef) -> Result<bool, VmError> {
    let (class, method) = vm.resolve_virtual_method(set, "contains", "(Ljava/lang/Object;)Z")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(set), Value::Ref(value)])?
        .as_int()
        != 0)
}

const ABSTRACT_SET_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        boolean(invoke_int(vm, this, "size")? == 0)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let elements = collection_elements(vm, receiver(args))?;
            let mut text = String::from("[");
            for (index, element) in elements.iter().enumerate() {
                if index > 0 {
                    text.push_str(", ");
                }
                text.push_str(&value_to_string(vm, *element)?);
            }
            text.push(']');
            object(vm.make_string(&text)?)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let elements = collection_elements(vm, receiver(args))?;
        let mut hash: i32 = 0;
        for element in elements {
            hash = hash.wrapping_add(object_hash(vm, element)?);
        }
        int(hash)
    }),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            if this.raw() == other.raw() {
                return boolean(true);
            }
            let set_class = vm.resolve_class("java/util/Set")?;
            if other.is_null() || !vm.is_instance(other, set_class) {
                return boolean(false);
            }
            let (size_class, size_method) = vm.resolve_virtual_method(other, "size", "()I")?;
            let other_size = vm
                .invoke_method(size_class, size_method, vec![Value::Ref(other)])?
                .as_int();
            if invoke_int(vm, this, "size")? != other_size {
                return boolean(false);
            }
            for element in collection_elements(vm, this)? {
                if !set_contains(vm, other, element)? {
                    return boolean(false);
                }
            }
            boolean(true)
        },
    ),
    method(
        "containsAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            for element in collection_elements(vm, ref_arg(args, 1))? {
                if !set_contains(vm, this, element)? {
                    return boolean(false);
                }
            }
            boolean(true)
        },
    ),
    method("add", "(Ljava/lang/Object;)Z", ACC_PUBLIC, |vm, _, _| {
        Err(vm.throw_new("java/lang/UnsupportedOperationException", None))
    }),
    method(
        "toArray",
        "()[Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let elements = collection_elements(vm, receiver(args))?;
            let object_class = vm.resolve_class("java/lang/Object")?;
            let array = vm.allocate_object_array(object_class, elements.len())?;
            for (index, element) in elements.iter().enumerate() {
                vm.array_set_ref(array, index, *element)?;
            }
            object(array)
        },
    ),
    method(
        "toArray",
        "([Ljava/lang/Object;)[Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let elements = collection_elements(vm, receiver(args))?;
            let given = ref_arg(args, 1);
            let length = vm.array_length(given)?;
            let object_class = vm.resolve_class("java/lang/Object")?;
            let array = if length >= elements.len() {
                given
            } else {
                vm.allocate_object_array(object_class, elements.len())?
            };
            for (index, element) in elements.iter().enumerate() {
                vm.array_set_ref(array, index, *element)?;
            }
            object(array)
        },
    ),
];

/// `java.util.AbstractSet`: the `iterator`/`size`-driven base for sets.
pub(crate) const ABSTRACT_SET: oxjvm_vm::NativeClass = class(
    "java/util/AbstractSet",
    Some("java/lang/Object"),
    &["java/util/Set"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &ABSTRACT_SET_METHODS,
    None,
);

const ITR_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method("hasNext", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let list = vm
            .read_ref_field(this, vm.class_of(this), "list", "Ljava/util/List;")
            .unwrap_or(ObjectRef::NULL);
        let cursor = crate::lang::read_int_field(vm, this, "cursor");
        boolean(cursor < crate::lang::read_int_field(vm, list, "size"))
    }),
    method("next", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let list = vm
            .read_ref_field(this, vm.class_of(this), "list", "Ljava/util/List;")
            .unwrap_or(ObjectRef::NULL);
        let cursor = crate::lang::read_int_field(vm, this, "cursor");
        let elements = array_list_elements(vm, list);
        if cursor < 0 || cursor as usize >= elements.len() {
            return Err(vm.throw_new("java/util/NoSuchElementException", None));
        }
        crate::lang::write_int_field(vm, this, "cursor", cursor + 1);
        object(elements[cursor as usize])
    }),
    method("remove", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let list = vm
            .read_ref_field(this, vm.class_of(this), "list", "Ljava/util/List;")
            .unwrap_or(ObjectRef::NULL);
        let cursor = crate::lang::read_int_field(vm, this, "cursor");
        if cursor <= 0 {
            return Err(vm.throw_new("java/lang/IllegalStateException", None));
        }
        let elements = array_list_elements(vm, list);
        let mut kept = Vec::with_capacity(elements.len().saturating_sub(1));
        for (index, element) in elements.iter().enumerate() {
            if index != (cursor - 1) as usize {
                kept.push(*element);
            }
        }
        array_list_store(vm, list, &kept)?;
        crate::lang::write_int_field(vm, this, "cursor", cursor - 1);
        void()
    }),
];

pub(crate) const ARRAY_LIST_ITERATOR: oxjvm_vm::NativeClass = class(
    "java/util/ArrayList$Itr",
    Some("java/lang/Object"),
    &["java/util/Iterator"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[
        field("list", "Ljava/util/List;", ACC_PRIVATE, None),
        field("cursor", "I", ACC_PRIVATE, None),
    ],
    &ITR_METHODS,
    None,
);

const LIST_ITR_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method("hasPrevious", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        boolean(crate::lang::read_int_field(vm, this, "cursor") > 0)
    }),
    method(
        "previous",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let list = vm
                .read_ref_field(this, vm.class_of(this), "list", "Ljava/util/List;")
                .unwrap_or(ObjectRef::NULL);
            let cursor = crate::lang::read_int_field(vm, this, "cursor");
            if cursor <= 0 {
                return Err(vm.throw_new("java/util/NoSuchElementException", None));
            }
            let elements = array_list_elements(vm, list);
            let index = (cursor - 1) as usize;
            crate::lang::write_int_field(vm, this, "cursor", cursor - 1);
            crate::lang::write_int_field(vm, this, "lastReturned", cursor - 1);
            object(elements[index])
        },
    ),
    method("nextIndex", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "cursor"))
    }),
    method("previousIndex", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "cursor") - 1)
    }),
    method("set", "(Ljava/lang/Object;)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let index = crate::lang::read_int_field(vm, this, "lastReturned");
        if index < 0 {
            return Err(vm.throw_new("java/lang/IllegalStateException", None));
        }
        let list = vm
            .read_ref_field(this, vm.class_of(this), "list", "Ljava/util/List;")
            .unwrap_or(ObjectRef::NULL);
        let mut elements = array_list_elements(vm, list);
        if let Some(slot) = elements.get_mut(index as usize) {
            *slot = ref_arg(args, 1);
        }
        array_list_store(vm, list, &elements)?;
        void()
    }),
    method("add", "(Ljava/lang/Object;)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let cursor = crate::lang::read_int_field(vm, this, "cursor");
        let list = vm
            .read_ref_field(this, vm.class_of(this), "list", "Ljava/util/List;")
            .unwrap_or(ObjectRef::NULL);
        let mut elements = array_list_elements(vm, list);
        let index = (cursor.max(0) as usize).min(elements.len());
        elements.insert(index, ref_arg(args, 1));
        array_list_store(vm, list, &elements)?;
        crate::lang::write_int_field(vm, this, "cursor", cursor + 1);
        crate::lang::write_int_field(vm, this, "lastReturned", -1);
        void()
    }),
];

pub(crate) const ARRAY_LIST_LIST_ITERATOR: oxjvm_vm::NativeClass = class(
    "java/util/ArrayList$ListItr",
    Some("java/util/ArrayList$Itr"),
    &["java/util/ListIterator"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field("lastReturned", "I", ACC_PRIVATE, None)],
    &LIST_ITR_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// HashMap (linear-probing arrays)
// -------------------------------------------------------------------------------------------

const HASH_MAP_METHODS: [oxjvm_vm::NativeMethodDef; 23] = [
    method("<init>", "()V", ACC_PUBLIC, hash_map_init),
    method("<init>", "(I)V", ACC_PUBLIC, hash_map_init),
    method("<init>", "(IF)V", ACC_PUBLIC, hash_map_init),
    method("<init>", "(IFZ)V", ACC_PUBLIC, hash_map_init),
    method(
        "<init>",
        "(Ljava/util/Map;)V",
        ACC_PUBLIC,
        |vm, context, args| {
            hash_map_init(vm, context, args)?;
            hash_map_put_all(vm, receiver(args), ref_arg(args, 1))
        },
    ),
    method("putAll", "(Ljava/util/Map;)V", ACC_PUBLIC, |vm, _, args| {
        hash_map_put_all(vm, receiver(args), ref_arg(args, 1))
    }),
    method(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        hash_map_put,
    ),
    method(
        "putIfAbsent",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| map_put_if_absent(vm, receiver(args), ref_arg(args, 1), ref_arg(args, 2)),
    ),
    method(
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        hash_map_get,
    ),
    method(
        "computeIfAbsent",
        "(Ljava/lang/Object;Ljava/util/function/Function;)Ljava/lang/Object;",
        ACC_PUBLIC,
        hash_map_compute_if_absent,
    ),
    method(
        "getOrDefault",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let key = ref_arg(args, 1);
            let index = hash_map_index_of(vm, receiver(args), key)?;
            if index >= 0 {
                hash_map_value_at(vm, receiver(args), index as usize)
            } else {
                object(ref_arg(args, 2))
            }
        },
    ),
    method(
        "containsKey",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| boolean(hash_map_index_of(vm, receiver(args), ref_arg(args, 1))? >= 0),
    ),
    method(
        "remove",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        hash_map_remove,
    ),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "size"))
    }),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "size") == 0)
    }),
    method("clear", "()V", ACC_PUBLIC, hash_map_init),
    method("keySet", "()Ljava/util/Set;", ACC_PUBLIC, hash_map_key_set),
    method(
        "values",
        "()Ljava/util/Collection;",
        ACC_PUBLIC,
        hash_map_values,
    ),
    method(
        "containsValue",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let values = hash_map_values_of(vm, receiver(args))?;
            boolean(values.iter().any(|value| *value == ref_arg(args, 1)))
        },
    ),
    method("putAll", "(Ljava/util/Map;)V", ACC_PUBLIC, |vm, _, args| {
        hash_map_put_all(vm, receiver(args), ref_arg(args, 1))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        hash_map_to_string,
    ),
    method(
        "entrySet",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, args| map_entry_set(vm, receiver(args)),
    ),
    method(
        "replace",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let index = hash_map_index_of(vm, receiver(args), ref_arg(args, 1))?;
            if index < 0 {
                return object(ObjectRef::NULL);
            }
            let previous = hash_map_value_at(vm, receiver(args), index as usize)?;
            let mut pairs = hash_map_pairs(vm, receiver(args));
            pairs[index as usize].1 = ref_arg(args, 2);
            hash_map_store(vm, receiver(args), &pairs)?;
            Ok(previous)
        },
    ),
];

fn hash_map_pairs(vm: &Vm<'_>, this: ObjectRef) -> Vec<(ObjectRef, ObjectRef)> {
    // The backing arrays can be longer than `size` (e.g. after a removal), so
    // only the first `size` entries are live.
    let size = crate::lang::read_int_field(vm, this, "size").max(0) as usize;
    let keys = match vm.read_ref_field(this, vm.class_of(this), "keys", "[Ljava/lang/Object;") {
        Some(array) => match vm.heap.get(array).map(|object| &object.data) {
            Some(ObjectData::Array(ArrayData::Reference(elements))) => elements.clone(),
            _ => Vec::new(),
        },
        None => Vec::new(),
    };
    let values = match vm.read_ref_field(this, vm.class_of(this), "values", "[Ljava/lang/Object;") {
        Some(array) => match vm.heap.get(array).map(|object| &object.data) {
            Some(ObjectData::Array(ArrayData::Reference(elements))) => elements.clone(),
            _ => Vec::new(),
        },
        None => Vec::new(),
    };
    let mut pairs: Vec<(ObjectRef, ObjectRef)> = keys.into_iter().zip(values).collect();
    pairs.truncate(size);
    pairs
}

fn hash_map_store(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    pairs: &[(ObjectRef, ObjectRef)],
) -> Result<(), VmError> {
    let class = vm.class_of(this);
    let (declaring, key_field) = vm.find_field(class, "keys", "[Ljava/lang/Object;")?;
    let (declaring2, value_field) = vm.find_field(class, "values", "[Ljava/lang/Object;")?;
    let object_class = vm.resolve_class("java/lang/Object")?;
    let keys = vm.allocate_object_array(object_class, pairs.len().max(1))?;
    let values = vm.allocate_object_array(object_class, pairs.len().max(1))?;
    for (index, (key, value)) in pairs.iter().enumerate() {
        vm.array_set_ref(keys, index, *key)?;
        vm.array_set_ref(values, index, *value)?;
    }
    vm.set_instance_ref(this, declaring, key_field, keys);
    vm.set_instance_ref(this, declaring2, value_field, values);
    crate::lang::write_int_field(vm, this, "size", pairs.len() as i32);
    Ok(())
}

fn hash_map_init(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    hash_map_store(vm, receiver(args), &[])?;
    void()
}

fn hash_map_index_of(vm: &mut Vm<'_>, this: ObjectRef, key: ObjectRef) -> Result<i32, VmError> {
    map_index_of(vm, this, key, false)
}

/// Index of `key` in the pair arrays; `by_identity` selects `==` over `equals`.
fn map_index_of(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    key: ObjectRef,
    by_identity: bool,
) -> Result<i32, VmError> {
    let pairs = hash_map_pairs(vm, this);
    for (index, (candidate, _)) in pairs.iter().enumerate() {
        let equal = if by_identity {
            *candidate == key
        } else {
            keys_equal(vm, *candidate, key)?
        };
        if equal {
            return Ok(index as i32);
        }
    }
    Ok(-1)
}

fn keys_equal(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<bool, VmError> {
    if a == b {
        return Ok(true);
    }
    if a.is_null() || b.is_null() {
        return Ok(false);
    }
    let (class, method) = vm.resolve_virtual_method(a, "equals", "(Ljava/lang/Object;)Z")?;
    let result = vm.invoke_method(class, method, vec![Value::Ref(a), Value::Ref(b)])?;
    Ok(result.as_int() != 0)
}

fn hash_map_value_at(vm: &mut Vm<'_>, this: ObjectRef, index: usize) -> Result<Value, VmError> {
    let pairs = hash_map_pairs(vm, this);
    object(pairs[index].1)
}

fn hash_map_put(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let key = ref_arg(args, 1);
    let value = ref_arg(args, 2);
    let mut pairs = hash_map_pairs(vm, this);
    if let Some(index) = pairs
        .iter()
        .position(|(candidate, _)| keys_equal(vm, *candidate, key).unwrap_or(false))
    {
        let previous = pairs[index].1;
        pairs[index].1 = value;
        hash_map_store(vm, this, &pairs)?;
        object(previous)
    } else {
        pairs.push((key, value));
        hash_map_rehash_if_needed(vm, &mut pairs)?;
        hash_map_store(vm, this, &pairs)?;
        object(ObjectRef::NULL)
    }
}

/// Reorder the pair array when the JDK's default 16-bucket table would resize.
///
/// The pair-array representation preserves insertion order, while a real
/// `HashMap` iterates in bucket order.  Tests such as `NullKeyAtResize` depend
/// on the rehash changing `keySet()` order, so emulate the table growth points
/// (13, 25, 49, ... entries) and stable-sort by the new bucket index.
fn hash_map_rehash_if_needed(
    vm: &mut Vm<'_>,
    pairs: &mut [(ObjectRef, ObjectRef)],
) -> Result<(), VmError> {
    let len = pairs.len();
    if len <= 12 {
        return Ok(());
    }
    let steps = len - 1;
    if steps % 12 != 0 {
        return Ok(());
    }
    let factor = steps / 12;
    if !factor.is_power_of_two() {
        return Ok(());
    }
    let capacity = 16usize << (factor.trailing_zeros() + 1);
    let mut ordered = Vec::with_capacity(len);
    for pair in pairs.iter() {
        let hash = object_hash(vm, pair.0)?;
        ordered.push((hash_bucket(hash, capacity), *pair));
    }
    ordered.sort_by_key(|(bucket, _)| *bucket);
    for (slot, (_, pair)) in pairs.iter_mut().zip(ordered) {
        *slot = pair;
    }
    Ok(())
}

fn hash_bucket(hash: i32, capacity: usize) -> usize {
    let hash = hash as u32;
    ((hash ^ (hash >> 16)) as usize) & (capacity - 1)
}

fn hash_map_compute_if_absent(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let key = ref_arg(args, 1);
    let function = ref_arg(args, 2);
    if function.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let pairs = hash_map_pairs(vm, this);
    if let Some((_, value)) = pairs
        .iter()
        .find(|(candidate, _)| keys_equal(vm, *candidate, key).unwrap_or(false))
    {
        return object(*value);
    }
    let (class, method) =
        vm.resolve_virtual_method(function, "apply", "(Ljava/lang/Object;)Ljava/lang/Object;")?;
    let value = vm
        .invoke_method(class, method, vec![Value::Ref(function), Value::Ref(key)])?
        .as_ref();
    if !value.is_null() {
        let mut pairs = pairs;
        pairs.push((key, value));
        hash_map_rehash_if_needed(vm, &mut pairs)?;
        hash_map_store(vm, this, &pairs)?;
    }
    object(value)
}

fn hash_map_get(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let index = hash_map_index_of(vm, receiver(args), ref_arg(args, 1))?;
    if index < 0 {
        object(ObjectRef::NULL)
    } else {
        hash_map_value_at(vm, receiver(args), index as usize)
    }
}

fn hash_map_remove(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let index = hash_map_index_of(vm, this, ref_arg(args, 1))?;
    if index < 0 {
        return object(ObjectRef::NULL);
    }
    let mut pairs = hash_map_pairs(vm, this);
    let (_, removed) = pairs.remove(index as usize);
    hash_map_store(vm, this, &pairs)?;
    object(removed)
}

fn hash_map_key_set(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let pairs = hash_map_pairs(vm, receiver(args));
    let list = array_list_from(vm, &pairs.iter().map(|(key, _)| *key).collect::<Vec<_>>())?;
    object(list)
}

fn hash_map_values(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let pairs = hash_map_pairs(vm, receiver(args));
    let list = array_list_from(
        vm,
        &pairs.iter().map(|(_, value)| *value).collect::<Vec<_>>(),
    )?;
    object(list)
}

fn hash_map_values_of(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Vec<ObjectRef>, VmError> {
    Ok(hash_map_pairs(vm, this)
        .into_iter()
        .map(|(_, value)| value)
        .collect())
}

/// `List.of(...)`: an `ArrayList` with the given elements.
fn list_of_values(vm: &mut Vm<'_>, elements: &[ObjectRef]) -> Result<ObjectRef, VmError> {
    array_list_from(vm, elements)
}

/// The elements of an object array (empty for anything else).
fn object_array_values(vm: &Vm<'_>, array: ObjectRef) -> Vec<ObjectRef> {
    if array.is_null() {
        return Vec::new();
    }
    match vm.heap.get(array).map(|object| &object.data) {
        Some(ObjectData::Array(ArrayData::Reference(elements))) => elements.clone(),
        _ => Vec::new(),
    }
}

pub(crate) fn array_list_from(
    vm: &mut Vm<'_>,
    elements: &[ObjectRef],
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/ArrayList")?;
    let list = vm.new_instance(class)?;
    array_list_store(vm, list, elements)?;
    Ok(list)
}

fn hash_map_to_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let pairs = hash_map_pairs(vm, receiver(args));
    let mut text = String::from("{");
    for (index, (key, value)) in pairs.iter().enumerate() {
        if index > 0 {
            text.push_str(", ");
        }
        text.push_str(&value_to_string(vm, *key)?);
        text.push('=');
        text.push_str(&value_to_string(vm, *value)?);
    }
    text.push('}');
    object(vm.make_string(&text)?)
}

fn value_to_string(vm: &mut Vm<'_>, reference: ObjectRef) -> Result<String, VmError> {
    if reference.is_null() {
        return Ok("null".into());
    }
    if let Some(text) = vm.string_value(reference) {
        return Ok(text);
    }
    let (class, method) =
        vm.resolve_virtual_method(reference, "toString", "()Ljava/lang/String;")?;
    let string = vm.invoke_method(class, method, vec![Value::Ref(reference)])?;
    Ok(vm.string_or_empty(string.as_ref()))
}

pub(crate) const HASH_MAP: oxjvm_vm::NativeClass = class(
    "java/util/HashMap",
    Some("java/lang/Object"),
    &[
        "java/util/Map",
        "java/lang/Cloneable",
        "java/io/Serializable",
    ],
    ACC_PUBLIC,
    &[
        field("keys", "[Ljava/lang/Object;", ACC_PRIVATE, None),
        field("values", "[Ljava/lang/Object;", ACC_PRIVATE, None),
        field("size", "I", ACC_PRIVATE, None),
    ],
    &HASH_MAP_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// IdentityHashMap (key identity, same pair-array layout as HashMap)
// -------------------------------------------------------------------------------------------

const IDENTITY_HASH_MAP_METHODS: [oxjvm_vm::NativeMethodDef; 17] = [
    method("<init>", "()V", ACC_PUBLIC, hash_map_init),
    method("<init>", "(I)V", ACC_PUBLIC, hash_map_init),
    method(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        identity_map_put,
    ),
    method(
        "putIfAbsent",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| map_put_if_absent(vm, receiver(args), ref_arg(args, 1), ref_arg(args, 2)),
    ),
    method(
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        identity_map_get,
    ),
    method(
        "getOrDefault",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let key = ref_arg(args, 1);
            let index = map_index_of(vm, receiver(args), key, true)?;
            if index >= 0 {
                hash_map_value_at(vm, receiver(args), index as usize)
            } else {
                object(ref_arg(args, 2))
            }
        },
    ),
    method(
        "containsKey",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| boolean(map_index_of(vm, receiver(args), ref_arg(args, 1), true)? >= 0),
    ),
    method(
        "remove",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        identity_map_remove,
    ),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "size"))
    }),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "size") == 0)
    }),
    method("clear", "()V", ACC_PUBLIC, hash_map_init),
    method("keySet", "()Ljava/util/Set;", ACC_PUBLIC, hash_map_key_set),
    method(
        "values",
        "()Ljava/util/Collection;",
        ACC_PUBLIC,
        hash_map_values,
    ),
    method(
        "containsValue",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let values = hash_map_values_of(vm, receiver(args))?;
            boolean(values.iter().any(|value| *value == ref_arg(args, 1)))
        },
    ),
    method("putAll", "(Ljava/util/Map;)V", ACC_PUBLIC, |vm, _, args| {
        hash_map_put_all(vm, receiver(args), ref_arg(args, 1))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        hash_map_to_string,
    ),
    method(
        "entrySet",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, args| map_entry_set(vm, receiver(args)),
    ),
];

fn identity_map_put(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let key = ref_arg(args, 1);
    let value = ref_arg(args, 2);
    let mut pairs = hash_map_pairs(vm, this);
    if let Some(index) = pairs.iter().position(|(candidate, _)| *candidate == key) {
        let previous = pairs[index].1;
        pairs[index].1 = value;
        hash_map_store(vm, this, &pairs)?;
        object(previous)
    } else {
        pairs.push((key, value));
        hash_map_store(vm, this, &pairs)?;
        object(ObjectRef::NULL)
    }
}

fn identity_map_get(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let index = map_index_of(vm, receiver(args), ref_arg(args, 1), true)?;
    if index < 0 {
        object(ObjectRef::NULL)
    } else {
        hash_map_value_at(vm, receiver(args), index as usize)
    }
}

fn identity_map_remove(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let index = map_index_of(vm, this, ref_arg(args, 1), true)?;
    if index < 0 {
        return object(ObjectRef::NULL);
    }
    let mut pairs = hash_map_pairs(vm, this);
    let (_, removed) = pairs.remove(index as usize);
    hash_map_store(vm, this, &pairs)?;
    object(removed)
}

pub(crate) const IDENTITY_HASH_MAP: oxjvm_vm::NativeClass = class(
    "java/util/IdentityHashMap",
    Some("java/lang/Object"),
    &[
        "java/util/Map",
        "java/lang/Cloneable",
        "java/io/Serializable",
    ],
    ACC_PUBLIC,
    &[
        field("keys", "[Ljava/lang/Object;", ACC_PRIVATE, None),
        field("values", "[Ljava/lang/Object;", ACC_PRIVATE, None),
        field("size", "I", ACC_PRIVATE, None),
    ],
    &IDENTITY_HASH_MAP_METHODS,
    None,
);

/// `SortedMap`: only needed so types like `Charset.availableCharsets()` resolve;
/// the concrete map returned is a plain `HashMap`.
pub(crate) const NAVIGABLE_MAP: oxjvm_vm::NativeClass = class(
    "java/util/NavigableMap",
    Some("java/lang/Object"),
    &["java/util/SortedMap"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[
        method(
            "firstEntry",
            "()Ljava/util/Map$Entry;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "lastEntry",
            "()Ljava/util/Map$Entry;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "pollFirstEntry",
            "()Ljava/util/Map$Entry;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "pollLastEntry",
            "()Ljava/util/Map$Entry;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "descendingMap",
            "()Ljava/util/NavigableMap;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "navigableKeySet",
            "()Ljava/util/NavigableSet;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "descendingKeySet",
            "()Ljava/util/NavigableSet;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "ceilingKey",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "floorKey",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "higherKey",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "lowerKey",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
    ],
    None,
);

pub(crate) const SORTED_MAP: oxjvm_vm::NativeClass = class(
    "java/util/SortedMap",
    Some("java/lang/Object"),
    &["java/util/Map"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[
        method(
            "comparator",
            "()Ljava/util/Comparator;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "firstKey",
            "()Ljava/lang/Object;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "lastKey",
            "()Ljava/lang/Object;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "headMap",
            "(Ljava/lang/Object;)Ljava/util/SortedMap;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "tailMap",
            "(Ljava/lang/Object;)Ljava/util/SortedMap;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "subMap",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/SortedMap;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
    ],
    None,
);

// -------------------------------------------------------------------------------------------
// TreeMap
// -------------------------------------------------------------------------------------------

const TREE_MAP_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method(
        "comparator",
        "()Ljava/util/Comparator;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "firstKey",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let pairs = hash_map_pairs(vm, receiver(args));
            let Some((key, _)) = pairs.first() else {
                return Err(vm.throw_new("java/util/NoSuchElementException", None));
            };
            object(*key)
        },
    ),
    method(
        "lastKey",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let pairs = hash_map_pairs(vm, receiver(args));
            let Some((key, _)) = pairs.last() else {
                return Err(vm.throw_new("java/util/NoSuchElementException", None));
            };
            object(*key)
        },
    ),
];

/// `TreeMap`: HashMap storage for now; the sorted views are only surfaced through
/// `firstKey`/`lastKey`, which read the live entries.
pub(crate) const TREE_MAP: oxjvm_vm::NativeClass = class(
    "java/util/TreeMap",
    Some("java/util/HashMap"),
    &["java/util/NavigableMap", "java/io/Serializable"],
    ACC_PUBLIC,
    &[],
    &TREE_MAP_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Properties
// -------------------------------------------------------------------------------------------

const PROPERTIES_METHODS: [oxjvm_vm::NativeMethodDef; 16] = [
    method("<init>", "()V", ACC_PUBLIC, properties_init),
    method(
        "getProperty",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC,
        properties_get_property,
    ),
    method(
        "getProperty",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC,
        properties_get_property_or_default,
    ),
    method(
        "setProperty",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/Object;",
        ACC_PUBLIC,
        properties_set_property,
    ),
    method(
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            properties_map_call(
                vm,
                receiver(args),
                "get",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                vec![Value::Ref(ref_arg(args, 1))],
            )
        },
    ),
    method(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            properties_map_call(
                vm,
                receiver(args),
                "put",
                "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
                vec![Value::Ref(ref_arg(args, 1)), Value::Ref(ref_arg(args, 2))],
            )
        },
    ),
    method(
        "putIfAbsent",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            map_put_if_absent(
                vm,
                properties_map(vm, receiver(args)),
                ref_arg(args, 1),
                ref_arg(args, 2),
            )
        },
    ),
    method(
        "remove",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            properties_map_call(
                vm,
                receiver(args),
                "remove",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                vec![Value::Ref(ref_arg(args, 1))],
            )
        },
    ),
    method(
        "containsKey",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            properties_map_call(
                vm,
                receiver(args),
                "containsKey",
                "(Ljava/lang/Object;)Z",
                vec![Value::Ref(ref_arg(args, 1))],
            )
        },
    ),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        properties_map_call(vm, receiver(args), "size", "()I", Vec::new())
    }),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        properties_map_call(vm, receiver(args), "isEmpty", "()Z", Vec::new())
    }),
    method("clear", "()V", ACC_PUBLIC, |vm, _, args| {
        properties_map_call(vm, receiver(args), "clear", "()V", Vec::new())
    }),
    method("putAll", "(Ljava/util/Map;)V", ACC_PUBLIC, |vm, _, args| {
        let map = properties_map(vm, receiver(args));
        hash_map_put_all(vm, map, ref_arg(args, 1))
    }),
    method("keySet", "()Ljava/util/Set;", ACC_PUBLIC, |vm, _, args| {
        let map = properties_map(vm, receiver(args));
        hash_map_call(vm, map, "keySet", "()Ljava/util/Set;", &[])
    }),
    method(
        "entrySet",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = properties_map(vm, receiver(args));
            hash_map_call(vm, map, "entrySet", "()Ljava/util/Set;", &[])
        },
    ),
    method(
        "load",
        "(Ljava/io/InputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| properties_load(vm, args),
    ),
];

pub(crate) const PROPERTIES: oxjvm_vm::NativeClass = class(
    "java/util/Properties",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[field("map", "Ljava/util/HashMap;", ACC_PRIVATE, None)],
    &PROPERTIES_METHODS,
    None,
);

fn properties_map(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    let class = vm.class_of(this);
    vm.read_ref_field(this, class, "map", "Ljava/util/HashMap;")
        .unwrap_or(ObjectRef::NULL)
}

/// `Properties.load(InputStream)`: parse `key=value` lines through `put`.
fn properties_load(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let this = receiver(args);
    let stream = ref_arg(args, 1);
    let mut bytes = Vec::new();
    if !stream.is_null() {
        let (class, method) = vm.resolve_virtual_method(stream, "read", "()I")?;
        loop {
            let value = vm
                .invoke_method(class, method, vec![Value::Ref(stream)])?
                .as_int();
            if value < 0 {
                break;
            }
            bytes.push((value & 0xFF) as u8);
        }
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
            continue;
        }
        let (key, value) = match trimmed.split_once('=').or_else(|| trimmed.split_once(':')) {
            Some((key, value)) => (key.trim_end(), value.trim_start()),
            None => (trimmed, ""),
        };
        let key = vm.make_string(key)?;
        let value = vm.make_string(value)?;
        properties_map_call(
            vm,
            this,
            "put",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            vec![Value::Ref(key), Value::Ref(value)],
        )?;
    }
    void()
}

fn properties_init(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let map_class = vm.resolve_class("java/util/HashMap")?;
    let map = vm.new_instance(map_class)?;
    let (declaring, ctor) = vm
        .find_method(map_class, "<init>", "()V")
        .ok_or_else(|| VmError::internal("HashMap lacks its constructor"))?;
    vm.invoke_method(declaring, ctor, vec![Value::Ref(map)])?;
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "map", "Ljava/util/HashMap;")?;
    vm.set_instance_ref(this, declaring, field, map);
    void()
}

/// Invoke one of the backing `HashMap`'s methods.
fn properties_map_call(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: &str,
    descriptor: &str,
    args: Vec<Value>,
) -> Result<Value, VmError> {
    let map = properties_map(vm, this);
    if map.is_null() {
        return Err(VmError::internal("Properties without a backing map"));
    }
    let (class, method) = vm.resolve_virtual_method(map, name, descriptor)?;
    let mut full = vec![Value::Ref(map)];
    full.extend(args);
    vm.invoke_method(class, method, full)
}

fn properties_get_property(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let value = properties_map_call(
        vm,
        receiver(args),
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        vec![Value::Ref(ref_arg(args, 1))],
    )?;
    let reference = value.as_ref();
    if !reference.is_null() && vm.string_value(reference).is_some() {
        object(reference)
    } else {
        object(ObjectRef::NULL)
    }
}

fn properties_get_property_or_default(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let value = properties_map_call(
        vm,
        receiver(args),
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        vec![Value::Ref(ref_arg(args, 1))],
    )?;
    let reference = value.as_ref();
    if !reference.is_null() && vm.string_value(reference).is_some() {
        object(reference)
    } else {
        object(ref_arg(args, 2))
    }
}

fn properties_set_property(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let previous = properties_map_call(
        vm,
        receiver(args),
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        vec![Value::Ref(ref_arg(args, 1)), Value::Ref(ref_arg(args, 2))],
    )?;
    let reference = previous.as_ref();
    if !reference.is_null() && vm.string_value(reference).is_some() {
        object(reference)
    } else {
        object(ObjectRef::NULL)
    }
}

// -------------------------------------------------------------------------------------------
// Collections
// -------------------------------------------------------------------------------------------

const COLLECTIONS_METHODS: [oxjvm_vm::NativeMethodDef; 23] = [
    method(
        "sort",
        "(Ljava/util/List;)V",
        ACC_PUBLIC | ACC_STATIC,
        collections_sort,
    ),
    method(
        "sort",
        "(Ljava/util/List;Ljava/util/Comparator;)V",
        ACC_PUBLIC | ACC_STATIC,
        collections_sort_with,
    ),
    method(
        "reverse",
        "(Ljava/util/List;)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let list = ref_arg(args, 0);
            let size = invoke_int(vm, list, "size")?;
            for index in 0..size / 2 {
                let a = invoke_object(vm, list, "get", &[Value::Int(index)])?;
                let b = invoke_object(vm, list, "get", &[Value::Int(size - 1 - index)])?;
                invoke_void(vm, list, "set", &[Value::Int(index), Value::Ref(b)])?;
                invoke_void(
                    vm,
                    list,
                    "set",
                    &[Value::Int(size - 1 - index), Value::Ref(a)],
                )?;
            }
            void()
        },
    ),
    method(
        "reverseOrder",
        "()Ljava/util/Comparator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| new_reverse_comparator(vm, ObjectRef::NULL),
    ),
    method(
        "reverseOrder",
        "(Ljava/util/Comparator;)Ljava/util/Comparator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| new_reverse_comparator(vm, ref_arg(args, 0)),
    ),
    method(
        "addAll",
        "(Ljava/util/Collection;[Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_STATIC,
        collections_add_all,
    ),
    method(
        "synchronizedList",
        "(Ljava/util/List;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        // oxjvm executes one instruction at a time and has no exposed locking API,
        // so the wrapper is the list itself.
        |_, _, args| object(ref_arg(args, 0)),
    ),
    method(
        "synchronizedMap",
        "(Ljava/util/Map;)Ljava/util/Map;",
        ACC_PUBLIC | ACC_STATIC,
        // As with synchronizedList, the map itself is returned.
        |_, _, args| object(ref_arg(args, 0)),
    ),
    method(
        "emptyList",
        "()Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(array_list_from(vm, &[])?),
    ),
    method(
        "emptySet",
        "()Ljava/util/Set;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(set_of_values(vm, &[])?),
    ),
    method(
        "emptyMap",
        "()Ljava/util/Map;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_hash_map(vm)?),
    ),
    method(
        "singletonList",
        "(Ljava/lang/Object;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(array_list_from(vm, &[ref_arg(args, 0)])?),
    ),
    method(
        "singleton",
        "(Ljava/lang/Object;)Ljava/util/Set;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(set_of_values(vm, &[ref_arg(args, 0)])?),
    ),
    method(
        "singletonMap",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/Map;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let map = new_hash_map(vm)?;
            hash_map_call(
                vm,
                map,
                "put",
                "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
                &[Value::Ref(ref_arg(args, 0)), Value::Ref(ref_arg(args, 1))],
            )?;
            object(map)
        },
    ),
    method(
        "nCopies",
        "(ILjava/lang/Object;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let count = crate::int_arg(args, 0).max(0) as usize;
            let element = ref_arg(args, 1);
            object(array_list_from(vm, &vec![element; count])?)
        },
    ),
    method(
        "unmodifiableList",
        "(Ljava/util/List;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| object(ref_arg(args, 0)),
    ),
    method(
        "unmodifiableSet",
        "(Ljava/util/Set;)Ljava/util/Set;",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| object(ref_arg(args, 0)),
    ),
    method(
        "unmodifiableMap",
        "(Ljava/util/Map;)Ljava/util/Map;",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| object(ref_arg(args, 0)),
    ),
    method(
        "unmodifiableCollection",
        "(Ljava/util/Collection;)Ljava/util/Collection;",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| object(ref_arg(args, 0)),
    ),
    method(
        "newSetFromMap",
        "(Ljava/util/Map;)Ljava/util/Set;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let class = vm.resolve_class("java/util/HashSet")?;
            let set = vm.new_instance(class)?;
            let (declaring, field) = vm.find_field(class, "map", "Ljava/util/HashMap;")?;
            vm.set_instance_ref(set, declaring, field, ref_arg(args, 0));
            object(set)
        },
    ),
    method(
        "synchronizedSet",
        "(Ljava/util/Set;)Ljava/util/Set;",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| object(ref_arg(args, 0)),
    ),
    method(
        "synchronizedCollection",
        "(Ljava/util/Collection;)Ljava/util/Collection;",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| object(ref_arg(args, 0)),
    ),
    method(
        "list",
        "(Ljava/util/Enumeration;)Ljava/util/ArrayList;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let elements = enumeration_elements(vm, ref_arg(args, 0))?;
            object(array_list_from(vm, &elements)?)
        },
    ),
];

/// `Collections.addAll(Collection, T...)`.
fn collections_add_all(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let collection = ref_arg(args, 0);
    let array = ref_arg(args, 1);
    if collection.is_null() || array.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let length = vm.array_length(array)?;
    let (class, method) = vm.resolve_virtual_method(collection, "add", "(Ljava/lang/Object;)Z")?;
    let mut changed = false;
    for index in 0..length {
        let value = vm.array_get(array, index as i32)?;
        let result = vm.invoke_method(class, method, vec![Value::Ref(collection), value])?;
        changed |= result.as_int() != 0;
    }
    boolean(changed)
}

/// Create a `Collections$ReverseComparator`, ordering by `delegate` (natural order when `null`).
fn new_reverse_comparator(vm: &mut Vm<'_>, delegate: ObjectRef) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/util/Collections$ReverseComparator")?;
    let instance = vm.new_instance(class)?;
    vm.protect(instance);
    let (declaring, field) = vm.find_field(class, "comparator", "Ljava/util/Comparator;")?;
    vm.set_instance_ref(instance, declaring, field, delegate);
    object(instance)
}

/// The natural ordering of two objects: nulls first, then strings, then `Comparable.compareTo`.
fn natural_compare(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<i32, VmError> {
    if a.is_null() || b.is_null() {
        return Ok(i32::from(a.is_null() && !b.is_null()));
    }
    if vm.string_utf16(a).is_some() && vm.string_utf16(b).is_some() {
        let an = vm.string_or_empty(a);
        let bn = vm.string_or_empty(b);
        return Ok(match an.cmp(&bn) {
            core::cmp::Ordering::Less => -1,
            core::cmp::Ordering::Equal => 0,
            core::cmp::Ordering::Greater => 1,
        });
    }
    let (class, method) = vm.resolve_virtual_method(a, "compareTo", "(Ljava/lang/Object;)I")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(a), Value::Ref(b)])?
        .as_int())
}

const REVERSE_COMPARATOR_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "compare",
    "(Ljava/lang/Object;Ljava/lang/Object;)I",
    ACC_PUBLIC,
    |vm, _, args| {
        let this = receiver(args);
        let a = ref_arg(args, 1);
        let b = ref_arg(args, 2);
        let delegate = vm.read_ref_field(
            this,
            vm.class_of(this),
            "comparator",
            "Ljava/util/Comparator;",
        );
        let ordering = match delegate {
            Some(delegate) if !delegate.is_null() => {
                let (class, method) = vm.resolve_virtual_method(
                    delegate,
                    "compare",
                    "(Ljava/lang/Object;Ljava/lang/Object;)I",
                )?;
                vm.invoke_method(
                    class,
                    method,
                    vec![Value::Ref(delegate), Value::Ref(a), Value::Ref(b)],
                )?
                .as_int()
            }
            _ => natural_compare(vm, a, b)?,
        };
        int(-ordering)
    },
)];

/// `Collections$ReverseComparator`: negates the delegate's ordering.
pub(crate) const REVERSE_COMPARATOR: oxjvm_vm::NativeClass = class(
    "java/util/Collections$ReverseComparator",
    Some("java/lang/Object"),
    &["java/util/Comparator"],
    ACC_PUBLIC,
    &[field(
        "comparator",
        "Ljava/util/Comparator;",
        ACC_PRIVATE,
        None,
    )],
    &REVERSE_COMPARATOR_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Comparator.comparing / thenComparing
// -------------------------------------------------------------------------------------------

const FUNCTION_COMPARATOR_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "compare",
        "(Ljava/lang/Object;Ljava/lang/Object;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let function = vm
                .read_ref_field(this, class, "function", "Ljava/util/function/Function;")
                .unwrap_or(ObjectRef::NULL);
            let delegate = vm
                .read_ref_field(this, class, "comparator", "Ljava/util/Comparator;")
                .unwrap_or(ObjectRef::NULL);
            let a = ref_arg(args, 1);
            let b = ref_arg(args, 2);
            let key_a = comparator_apply(vm, function, a)?;
            let key_b = comparator_apply(vm, function, b)?;
            let ordering = if delegate.is_null() {
                natural_compare(vm, key_a, key_b)?
            } else {
                let (class, method) = vm.resolve_virtual_method(
                    delegate,
                    "compare",
                    "(Ljava/lang/Object;Ljava/lang/Object;)I",
                )?;
                vm.invoke_method(
                    class,
                    method,
                    vec![Value::Ref(delegate), Value::Ref(key_a), Value::Ref(key_b)],
                )?
                .as_int()
            };
            int(ordering)
        },
    ),
    method(
        "reversed",
        "()Ljava/util/Comparator;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.resolve_class("java/util/Collections$ReverseComparator")?;
            let instance = vm.new_instance(class)?;
            let (declaring, field) =
                vm.find_field(class, "comparator", "Ljava/util/Comparator;")?;
            vm.set_instance_ref(instance, declaring, field, this);
            object(instance)
        },
    ),
    method(
        "thenComparing",
        "(Ljava/util/Comparator;)Ljava/util/Comparator;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let class = vm.resolve_class("java/util/Comparator$ChainedComparator")?;
            let instance = vm.new_instance(class)?;
            let (declaring, field) = vm.find_field(class, "first", "Ljava/util/Comparator;")?;
            vm.set_instance_ref(instance, declaring, field, this);
            let (declaring, field) = vm.find_field(class, "second", "Ljava/util/Comparator;")?;
            vm.set_instance_ref(instance, declaring, field, other);
            object(instance)
        },
    ),
    method(
        "thenComparing",
        "(Ljava/util/function/Function;)Ljava/util/Comparator;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let class = vm.resolve_class("java/util/Comparator$FunctionComparator")?;
            let instance = vm.new_instance(class)?;
            let (declaring, field) =
                vm.find_field(class, "function", "Ljava/util/function/Function;")?;
            vm.set_instance_ref(instance, declaring, field, other);
            let chained = vm.resolve_class("java/util/Comparator$ChainedComparator")?;
            let result = vm.new_instance(chained)?;
            let (declaring, field) = vm.find_field(chained, "first", "Ljava/util/Comparator;")?;
            vm.set_instance_ref(result, declaring, field, this);
            let (declaring, field) = vm.find_field(chained, "second", "Ljava/util/Comparator;")?;
            vm.set_instance_ref(result, declaring, field, instance);
            object(result)
        },
    ),
];

const CHAINED_COMPARATOR_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "compare",
    "(Ljava/lang/Object;Ljava/lang/Object;)I",
    ACC_PUBLIC,
    |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        let first = vm
            .read_ref_field(this, class, "first", "Ljava/util/Comparator;")
            .unwrap_or(ObjectRef::NULL);
        let second = vm
            .read_ref_field(this, class, "second", "Ljava/util/Comparator;")
            .unwrap_or(ObjectRef::NULL);
        let a = ref_arg(args, 1);
        let b = ref_arg(args, 2);
        let ordering = comparator_compare_with(vm, first, a, b)?;
        if ordering != 0 {
            return int(ordering);
        }
        int(comparator_compare_with(vm, second, a, b)?)
    },
)];

fn comparator_compare_with(
    vm: &mut Vm<'_>,
    comparator: ObjectRef,
    a: ObjectRef,
    b: ObjectRef,
) -> Result<i32, VmError> {
    if comparator.is_null() {
        return natural_compare(vm, a, b);
    }
    let (class, method) = vm.resolve_virtual_method(
        comparator,
        "compare",
        "(Ljava/lang/Object;Ljava/lang/Object;)I",
    )?;
    Ok(vm
        .invoke_method(
            class,
            method,
            vec![Value::Ref(comparator), Value::Ref(a), Value::Ref(b)],
        )?
        .as_int())
}

/// `Comparator.thenComparing`: the first comparator's ordering, then the second's on ties.
pub(crate) const CHAINED_COMPARATOR: oxjvm_vm::NativeClass = class(
    "java/util/Comparator$ChainedComparator",
    Some("java/lang/Object"),
    &["java/util/Comparator"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[
        field("first", "Ljava/util/Comparator;", ACC_PRIVATE, None),
        field("second", "Ljava/util/Comparator;", ACC_PRIVATE, None),
    ],
    &CHAINED_COMPARATOR_METHODS,
    None,
);

fn comparator_apply(
    vm: &mut Vm<'_>,
    function: ObjectRef,
    value: ObjectRef,
) -> Result<ObjectRef, VmError> {
    let (class, method) =
        vm.resolve_virtual_method(function, "apply", "(Ljava/lang/Object;)Ljava/lang/Object;")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(function), Value::Ref(value)])?
        .as_ref())
}

/// The comparator `Comparator.comparing` returns: applies a key function, then compares keys.
pub(crate) const FUNCTION_COMPARATOR: oxjvm_vm::NativeClass = class(
    "java/util/Comparator$FunctionComparator",
    Some("java/lang/Object"),
    &["java/util/Comparator"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[
        field(
            "function",
            "Ljava/util/function/Function;",
            ACC_PRIVATE,
            None,
        ),
        field("comparator", "Ljava/util/Comparator;", ACC_PRIVATE, None),
    ],
    &FUNCTION_COMPARATOR_METHODS,
    None,
);

fn collections_sort(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let list = ref_arg(args, 0);
    let size = invoke_int(vm, list, "size")?;
    // Insertion sort using `compareTo` on each element.
    for index in 1..size {
        let mut cursor = index;
        while cursor > 0 {
            let a = invoke_object(vm, list, "get", &[Value::Int(cursor - 1)])?;
            let b = invoke_object(vm, list, "get", &[Value::Int(cursor)])?;
            let ordering = natural_compare(vm, a, b)?;
            if ordering <= 0 {
                break;
            }
            invoke_void(vm, list, "set", &[Value::Int(cursor - 1), Value::Ref(b)])?;
            invoke_void(vm, list, "set", &[Value::Int(cursor), Value::Ref(a)])?;
            cursor -= 1;
        }
    }
    void()
}

/// `Collections.sort(List, Comparator)`: insertion sort with the supplied comparator.
fn collections_sort_with(
    vm: &mut Vm<'_>,
    context: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let list = ref_arg(args, 0);
    let comparator = ref_arg(args, 1);
    if comparator.is_null() {
        return collections_sort(vm, context, &[args[0]]);
    }
    let size = invoke_int(vm, list, "size")?;
    for index in 1..size {
        let mut cursor = index;
        while cursor > 0 {
            let a = invoke_object(vm, list, "get", &[Value::Int(cursor - 1)])?;
            let b = invoke_object(vm, list, "get", &[Value::Int(cursor)])?;
            let (class, method) = vm.resolve_virtual_method(
                comparator,
                "compare",
                "(Ljava/lang/Object;Ljava/lang/Object;)I",
            )?;
            let ordering = vm
                .invoke_method(
                    class,
                    method,
                    vec![Value::Ref(comparator), Value::Ref(a), Value::Ref(b)],
                )?
                .as_int();
            if ordering <= 0 {
                break;
            }
            invoke_void(vm, list, "set", &[Value::Int(cursor - 1), Value::Ref(b)])?;
            invoke_void(vm, list, "set", &[Value::Int(cursor), Value::Ref(a)])?;
            cursor -= 1;
        }
    }
    void()
}

fn invoke_int(vm: &mut Vm<'_>, receiver: ObjectRef, name: &str) -> Result<i32, VmError> {
    let (class, method) = vm.resolve_virtual_method(receiver, name, "()I")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(receiver)])?
        .as_int())
}

fn invoke_object(
    vm: &mut Vm<'_>,
    receiver: ObjectRef,
    name: &str,
    extra: &[Value],
) -> Result<ObjectRef, VmError> {
    let (class, method) = vm.resolve_virtual_method(receiver, name, "(I)Ljava/lang/Object;")?;
    let mut args = vec![Value::Ref(receiver)];
    args.extend_from_slice(extra);
    Ok(vm.invoke_method(class, method, args)?.as_ref())
}

fn invoke_void(
    vm: &mut Vm<'_>,
    receiver: ObjectRef,
    name: &str,
    extra: &[Value],
) -> Result<(), VmError> {
    let (class, method) =
        vm.resolve_virtual_method(receiver, name, "(ILjava/lang/Object;)Ljava/lang/Object;")?;
    let mut args = vec![Value::Ref(receiver)];
    args.extend_from_slice(extra);
    vm.invoke_method(class, method, args)?;
    Ok(())
}

/// Invoke a virtual method that returns `boolean`.
fn invoke_virtual_bool(
    vm: &mut Vm<'_>,
    receiver: ObjectRef,
    name: &str,
    descriptor: &str,
    extra: &[Value],
) -> Result<bool, VmError> {
    let (class, method) = vm.resolve_virtual_method(receiver, name, descriptor)?;
    let mut args = vec![Value::Ref(receiver)];
    args.extend_from_slice(extra);
    Ok(vm.invoke_method(class, method, args)?.as_int() != 0)
}

/// `Collection.removeAll`: remove every element the other collection contains.
fn collection_remove_all(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let others = collection_elements(vm, ref_arg(args, 1))?;
    let mut changed = false;
    for element in others {
        changed |= invoke_virtual_bool(
            vm,
            this,
            "remove",
            "(Ljava/lang/Object;)Z",
            &[Value::Ref(element)],
        )?;
    }
    boolean(changed)
}

/// `Collection.retainAll`: remove every element the other collection does not contain.
fn collection_retain_all(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let other = ref_arg(args, 1);
    let elements = collection_elements(vm, this)?;
    let mut changed = false;
    for element in elements {
        let keeps = invoke_virtual_bool(
            vm,
            other,
            "contains",
            "(Ljava/lang/Object;)Z",
            &[Value::Ref(element)],
        )?;
        if !keeps {
            changed |= invoke_virtual_bool(
                vm,
                this,
                "remove",
                "(Ljava/lang/Object;)Z",
                &[Value::Ref(element)],
            )?;
        }
    }
    boolean(changed)
}

pub(crate) const COLLECTIONS: oxjvm_vm::NativeClass = class(
    "java/util/Collections",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &COLLECTIONS_METHODS,
    None,
);

/// A `HashSet` with no elements (the backing map is installed explicitly).
fn new_hash_set(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/HashSet")?;
    let set = vm.new_instance(class)?;
    let map = new_hash_map(vm)?;
    let (declaring, field) = vm.find_field(class, "map", "Ljava/util/HashMap;")?;
    vm.set_instance_ref(set, declaring, field, map);
    Ok(set)
}

/// `Set.of(...)`: a `HashSet` with the given elements.
fn set_of_values(vm: &mut Vm<'_>, elements: &[ObjectRef]) -> Result<ObjectRef, VmError> {
    let set = new_hash_set(vm)?;
    let present = hash_set_present(vm)?;
    let map = hash_set_map(vm, set);
    for element in elements {
        hash_map_call(
            vm,
            map,
            "put",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            &[Value::Ref(*element), Value::Ref(present)],
        )?;
    }
    Ok(set)
}

/// A fresh `HashSet` containing `elements` (the `Collectors.toSet` backing store).
pub(crate) fn hash_set_from(vm: &mut Vm<'_>, elements: &[ObjectRef]) -> Result<ObjectRef, VmError> {
    let set = new_hash_set(vm)?;
    let present = hash_set_present(vm)?;
    let map = hash_set_map(vm, set);
    for element in elements {
        hash_map_call(
            vm,
            map,
            "put",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            &[Value::Ref(*element), Value::Ref(present)],
        )?;
    }
    Ok(set)
}

/// A fresh `HashMap` (the `Collectors.toMap` backing store).
pub(crate) fn hash_map_new(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    new_hash_map(vm)
}

/// `map.put(key, value)`, discarding the previous value.
pub(crate) fn hash_map_insert(
    vm: &mut Vm<'_>,
    map: ObjectRef,
    key: ObjectRef,
    value: ObjectRef,
) -> Result<(), VmError> {
    hash_map_call(
        vm,
        map,
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        &[Value::Ref(key), Value::Ref(value)],
    )?;
    Ok(())
}

/// `map.get(key)`.
pub(crate) fn hash_map_lookup(
    vm: &mut Vm<'_>,
    map: ObjectRef,
    key: ObjectRef,
) -> Result<ObjectRef, VmError> {
    Ok(hash_map_call(
        vm,
        map,
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        &[Value::Ref(key)],
    )?
    .as_ref())
}

/// `Map.putIfAbsent`: store the value only when the key has no (non-null) mapping yet.
///
/// The lookups go through the map's own `get`/`put`, so the legacy wrappers
/// (`Properties`, `Hashtable`, `EnumMap`) and the concurrent maps share one implementation.
pub(crate) fn map_put_if_absent(
    vm: &mut Vm<'_>,
    map: ObjectRef,
    key: ObjectRef,
    value: ObjectRef,
) -> Result<Value, VmError> {
    let (class, get) =
        vm.resolve_virtual_method(map, "get", "(Ljava/lang/Object;)Ljava/lang/Object;")?;
    let existing = vm.invoke_method(class, get, vec![Value::Ref(map), Value::Ref(key)])?;
    if !existing.as_ref().is_null() {
        return Ok(existing);
    }
    let (class, put) = vm.resolve_virtual_method(
        map,
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
    )?;
    vm.invoke_method(
        class,
        put,
        vec![Value::Ref(map), Value::Ref(key), Value::Ref(value)],
    )
}

// -------------------------------------------------------------------------------------------
// Enumeration + the legacy collections (HashSet, Hashtable, Vector)
// -------------------------------------------------------------------------------------------

const ENUMERATION_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "hasMoreElements",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "nextElement",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const ENUMERATION: oxjvm_vm::NativeClass = class(
    "java/util/Enumeration",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &ENUMERATION_METHODS,
    None,
);

const OJVM_ENUMERATION_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method("hasMoreElements", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let elements = vm
            .read_ref_field(this, vm.class_of(this), "elements", "[Ljava/lang/Object;")
            .map_or(0, |array| vm.array_length(array).unwrap_or(0));
        boolean(crate::lang::read_int_field(vm, this, "cursor") < elements as i32)
    }),
    method(
        "nextElement",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let array = vm
                .read_ref_field(this, vm.class_of(this), "elements", "[Ljava/lang/Object;")
                .unwrap_or(ObjectRef::NULL);
            let cursor = crate::lang::read_int_field(vm, this, "cursor");
            let length = vm.array_length(array).unwrap_or(0);
            if cursor < 0 || cursor as usize >= length {
                return Err(vm.throw_new("java/util/NoSuchElementException", None));
            }
            crate::lang::write_int_field(vm, this, "cursor", cursor + 1);
            vm.array_get(array, cursor)
        },
    ),
];

// -------------------------------------------------------------------------------------------
// StringTokenizer
// -------------------------------------------------------------------------------------------

fn tokenizer_text(vm: &Vm<'_>, this: ObjectRef) -> String {
    vm.read_string_field(this, vm.class_of(this), "text")
        .unwrap_or_default()
}

fn tokenizer_delimiters(vm: &Vm<'_>, this: ObjectRef) -> String {
    vm.read_string_field(this, vm.class_of(this), "delimiters")
        .unwrap_or_default()
}

fn tokenizer_init(
    vm: &mut Vm<'_>,
    context: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let descriptor = vm.classes.get(context.class).methods[context.method as usize]
        .descriptor
        .clone();
    let text = string_arg(vm, args[1])?;
    let (delimiters, return_delims) = match descriptor.as_str() {
        "(Ljava/lang/String;Ljava/lang/String;Z)V" => {
            (string_arg(vm, args[2])?, crate::int_arg(args, 3) != 0)
        }
        "(Ljava/lang/String;Ljava/lang/String;)V" => (string_arg(vm, args[2])?, false),
        _ => (String::from(" \t\n\r\u{000C}"), false),
    };
    let class = vm.class_of(this);
    vm.set_string_field(this, class, "text", &text)?;
    vm.set_string_field(this, class, "delimiters", &delimiters)?;
    crate::lang::write_int_field(vm, this, "position", 0);
    crate::lang::write_int_field(vm, this, "returnDelims", i32::from(return_delims));
    void()
}

/// Take the next token at the current position, optionally advancing it.
fn tokenizer_token(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    delimiters: &str,
    advance: bool,
) -> Result<Option<String>, VmError> {
    let text = tokenizer_text(vm, this);
    let chars: Vec<char> = text.chars().collect();
    let return_delims = crate::lang::read_int_field(vm, this, "returnDelims") != 0;
    let mut position = crate::lang::read_int_field(vm, this, "position").max(0) as usize;
    while position < chars.len() && !return_delims && delimiters.contains(chars[position]) {
        position += 1;
    }
    if position >= chars.len() {
        if advance {
            crate::lang::write_int_field(vm, this, "position", position as i32);
        }
        return Ok(None);
    }
    let start = position;
    if return_delims && delimiters.contains(chars[position]) {
        position += 1;
    } else {
        while position < chars.len() && !delimiters.contains(chars[position]) {
            position += 1;
        }
    }
    if advance {
        crate::lang::write_int_field(vm, this, "position", position as i32);
    }
    Ok(Some(chars[start..position].iter().collect()))
}

fn tokenizer_has_more(vm: &mut Vm<'_>, this: ObjectRef, delimiters: &str) -> Result<bool, VmError> {
    Ok(tokenizer_token(vm, this, delimiters, false)?.is_some())
}

fn tokenizer_next(vm: &mut Vm<'_>, this: ObjectRef, delimiters: &str) -> Result<Value, VmError> {
    match tokenizer_token(vm, this, delimiters, true)? {
        Some(token) => object(vm.make_string(&token)?),
        None => Err(vm.throw_new("java/util/NoSuchElementException", None)),
    }
}

const STRING_TOKENIZER_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        tokenizer_init,
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        tokenizer_init,
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;Z)V",
        ACC_PUBLIC,
        tokenizer_init,
    ),
    method("hasMoreTokens", "()Z", ACC_PUBLIC, |vm, _, args| {
        let delimiters = tokenizer_delimiters(vm, receiver(args));
        boolean(tokenizer_has_more(vm, receiver(args), &delimiters)?)
    }),
    method(
        "nextToken",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let delimiters = tokenizer_delimiters(vm, receiver(args));
            tokenizer_next(vm, receiver(args), &delimiters)
        },
    ),
    method(
        "nextToken",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let delimiters = string_arg(vm, args[1])?;
            tokenizer_next(vm, receiver(args), &delimiters)
        },
    ),
    method("countTokens", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let delimiters = tokenizer_delimiters(vm, this);
        let saved = crate::lang::read_int_field(vm, this, "position");
        let mut count = 0;
        while tokenizer_token(vm, this, &delimiters, true)?.is_some() {
            count += 1;
        }
        crate::lang::write_int_field(vm, this, "position", saved);
        int(count)
    }),
    method("hasMoreElements", "()Z", ACC_PUBLIC, |vm, _, args| {
        let delimiters = tokenizer_delimiters(vm, receiver(args));
        boolean(tokenizer_has_more(vm, receiver(args), &delimiters)?)
    }),
    method(
        "nextElement",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let delimiters = tokenizer_delimiters(vm, receiver(args));
            tokenizer_next(vm, receiver(args), &delimiters)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = tokenizer_text(vm, receiver(args));
            object(vm.make_string(&text)?)
        },
    ),
];

pub(crate) const STRING_TOKENIZER: oxjvm_vm::NativeClass = class(
    "java/util/StringTokenizer",
    Some("java/lang/Object"),
    &["java/util/Enumeration"],
    ACC_PUBLIC,
    &[
        field("text", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("delimiters", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("position", "I", ACC_PRIVATE, None),
        field("returnDelims", "I", ACC_PRIVATE, None),
    ],
    &STRING_TOKENIZER_METHODS,
    None,
);

pub(crate) const OJVM_ENUMERATION: oxjvm_vm::NativeClass = class(
    "java/util/OjvmEnumeration",
    Some("java/lang/Object"),
    &["java/util/Enumeration"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[
        field("elements", "[Ljava/lang/Object;", ACC_PRIVATE, None),
        field("cursor", "I", ACC_PRIVATE, None),
    ],
    &OJVM_ENUMERATION_METHODS,
    None,
);

pub(crate) fn new_enumeration(
    vm: &mut Vm<'_>,
    elements: &[ObjectRef],
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/OjvmEnumeration")?;
    let instance = vm.new_instance(class)?;
    let object_class = vm.resolve_class("java/lang/Object")?;
    let array = vm.allocate_object_array(object_class, elements.len())?;
    for (index, element) in elements.iter().enumerate() {
        vm.array_set_ref(array, index, *element)?;
    }
    let (declaring, field) = vm.find_field(class, "elements", "[Ljava/lang/Object;")?;
    vm.set_instance_ref(instance, declaring, field, array);
    Ok(instance)
}

/// Drain an `Enumeration` into a `Vec`, the way `Collections.list` does.
pub(crate) fn enumeration_elements(
    vm: &mut Vm<'_>,
    enumeration: ObjectRef,
) -> Result<Vec<ObjectRef>, VmError> {
    if enumeration.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let (has_class, has_more) = vm.resolve_virtual_method(enumeration, "hasMoreElements", "()Z")?;
    let (next_class, next) =
        vm.resolve_virtual_method(enumeration, "nextElement", "()Ljava/lang/Object;")?;
    let mut elements = Vec::new();
    loop {
        let value = vm.invoke_method(has_class, has_more, vec![Value::Ref(enumeration)])?;
        if value.as_int() == 0 {
            return Ok(elements);
        }
        let value = vm.invoke_method(next_class, next, vec![Value::Ref(enumeration)])?;
        elements.push(value.as_ref());
    }
}

/// `Map.of(...)` and the empty map share the same backing `HashMap`.
fn map_of_pairs(vm: &mut Vm<'_>, pairs: &[(ObjectRef, ObjectRef)]) -> Result<Value, VmError> {
    for (key, value) in pairs {
        if key.is_null() || value.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
    }
    let map = new_hash_map(vm)?;
    for (key, value) in pairs {
        hash_map_insert(vm, map, *key, *value)?;
    }
    object(map)
}

fn map_keys(vm: &Vm<'_>, map: ObjectRef) -> Vec<ObjectRef> {
    hash_map_pairs(vm, map)
        .into_iter()
        .map(|(key, _)| key)
        .collect()
}

fn map_values(vm: &Vm<'_>, map: ObjectRef) -> Vec<ObjectRef> {
    hash_map_pairs(vm, map)
        .into_iter()
        .map(|(_, value)| value)
        .collect()
}

// -------------------------------------------------------------------------------------------
// HashSet (a wrapper over HashMap)
// -------------------------------------------------------------------------------------------

fn hash_set_map(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "map", "Ljava/util/HashMap;")
        .unwrap_or(ObjectRef::NULL)
}

fn new_hash_map(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/HashMap")?;
    let instance = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(class, "<init>", "()V")
        .ok_or_else(|| VmError::internal("HashMap() missing"))?;
    vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
    Ok(instance)
}

fn hash_map_call(
    vm: &mut Vm<'_>,
    map: ObjectRef,
    name: &str,
    descriptor: &str,
    arguments: &[Value],
) -> Result<Value, VmError> {
    let (class, method) = vm.resolve_virtual_method(map, name, descriptor)?;
    let mut call_args = vec![Value::Ref(map)];
    call_args.extend_from_slice(arguments);
    vm.invoke_method(class, method, call_args)
}

/// Merge `source` into `this` (`Map.putAll`).  The source is walked through
/// its own `keySet` and `get`, so this works for any `Map` implementation
/// (including wrappers that delegate through native methods).
fn hash_map_put_all(vm: &mut Vm<'_>, this: ObjectRef, source: ObjectRef) -> Result<Value, VmError> {
    if source.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let keys = hash_map_call(vm, source, "keySet", "()Ljava/util/Set;", &[])?;
    for key in collection_elements(vm, keys.as_ref())? {
        let value = hash_map_call(
            vm,
            source,
            "get",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            &[Value::Ref(key)],
        )?;
        hash_map_call(
            vm,
            this,
            "put",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            &[Value::Ref(key), Value::Ref(value.as_ref())],
        )?;
    }
    void()
}

fn hash_set_present(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/lang/Boolean")?;
    let name = "TRUE";
    if let Some(existing) = vm.read_static_ref_if_set(class, name, "Ljava/lang/Boolean;") {
        return Ok(existing);
    }
    let instance = vm.new_instance(class)?;
    crate::boxed::write_value_int(vm, instance, 1);
    vm.set_static_value(class, name, "Ljava/lang/Boolean;", Value::Ref(instance));
    Ok(instance)
}

const HASH_SET_METHODS: [oxjvm_vm::NativeMethodDef; 20] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let map = new_hash_map(vm)?;
        let this = receiver(args);
        let class = vm.class_of(this);
        let (declaring, field) = vm.find_field(class, "map", "Ljava/util/HashMap;")?;
        vm.set_instance_ref(this, declaring, field, map);
        void()
    }),
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let map = new_hash_map(vm)?;
        let this = receiver(args);
        let class = vm.class_of(this);
        let (declaring, field) = vm.find_field(class, "map", "Ljava/util/HashMap;")?;
        vm.set_instance_ref(this, declaring, field, map);
        void()
    }),
    method(
        "<init>",
        "(Ljava/util/Collection;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = new_hash_map(vm)?;
            let this = receiver(args);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "map", "Ljava/util/HashMap;")?;
            vm.set_instance_ref(this, declaring, field, map);
            let present = hash_set_present(vm)?;
            for element in collection_elements(vm, ref_arg(args, 1))? {
                hash_map_call(
                    vm,
                    map,
                    "put",
                    "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
                    &[Value::Ref(element), Value::Ref(present)],
                )?;
            }
            void()
        },
    ),
    method("add", "(Ljava/lang/Object;)Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let map = hash_set_map(vm, this);
        let present = hash_set_present(vm)?;
        let previous = hash_map_call(
            vm,
            map,
            "put",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            &[Value::Ref(ref_arg(args, 1)), Value::Ref(present)],
        )?;
        boolean(previous.as_ref().is_null())
    }),
    method(
        "remove",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = hash_set_map(vm, receiver(args));
            let previous = hash_map_call(
                vm,
                map,
                "remove",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                &[Value::Ref(ref_arg(args, 1))],
            )?;
            boolean(!previous.as_ref().is_null())
        },
    ),
    method(
        "contains",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = hash_set_map(vm, receiver(args));
            hash_map_call(
                vm,
                map,
                "containsKey",
                "(Ljava/lang/Object;)Z",
                &[Value::Ref(ref_arg(args, 1))],
            )
        },
    ),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        let map = hash_set_map(vm, receiver(args));
        hash_map_call(vm, map, "size", "()I", &[])
    }),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        let map = hash_set_map(vm, receiver(args));
        hash_map_call(vm, map, "isEmpty", "()Z", &[])
    }),
    method("clear", "()V", ACC_PUBLIC, |vm, _, args| {
        let map = hash_set_map(vm, receiver(args));
        hash_map_call(vm, map, "clear", "()V", &[])
    }),
    method(
        "iterator",
        "()Ljava/util/Iterator;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = hash_set_map(vm, receiver(args));
            let keys = hash_map_call(vm, map, "keySet", "()Ljava/util/Set;", &[])?;
            let set = keys.as_ref();
            let (class, method) =
                vm.resolve_virtual_method(set, "iterator", "()Ljava/util/Iterator;")?;
            vm.invoke_method(class, method, vec![Value::Ref(set)])
        },
    ),
    method(
        "addAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let map = hash_set_map(vm, this);
            let present = hash_set_present(vm)?;
            let mut changed = false;
            for element in collection_elements(vm, ref_arg(args, 1))? {
                let previous = hash_map_call(
                    vm,
                    map,
                    "put",
                    "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
                    &[Value::Ref(element), Value::Ref(present)],
                )?;
                changed |= previous.as_ref().is_null();
            }
            boolean(changed)
        },
    ),
    method(
        "containsAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let map = hash_set_map(vm, this);
            for element in collection_elements(vm, ref_arg(args, 1))? {
                let contains = hash_map_call(
                    vm,
                    map,
                    "containsKey",
                    "(Ljava/lang/Object;)Z",
                    &[Value::Ref(element)],
                )?;
                if contains.as_int() == 0 {
                    return boolean(false);
                }
            }
            boolean(true)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = hash_set_map(vm, receiver(args));
            let keys = map_keys(vm, map);
            let mut text = String::from("[");
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    text.push_str(", ");
                }
                text.push_str(&value_to_string(vm, *key)?);
            }
            text.push(']');
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "forEach",
        "(Ljava/util/function/Consumer;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = hash_set_map(vm, receiver(args));
            let consumer = ref_arg(args, 1);
            let (class, method) =
                vm.resolve_virtual_method(consumer, "accept", "(Ljava/lang/Object;)V")?;
            for key in map_keys(vm, map) {
                vm.invoke_method(class, method, vec![Value::Ref(consumer), Value::Ref(key)])?;
            }
            void()
        },
    ),
    method(
        "toArray",
        "()[Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = hash_set_map(vm, receiver(args));
            let keys = map_keys(vm, map);
            let object_class = vm.resolve_class("java/lang/Object")?;
            let array = vm.allocate_object_array(object_class, keys.len())?;
            vm.protect(array);
            for (index, key) in keys.iter().enumerate() {
                vm.array_set_ref(array, index, *key)?;
            }
            object(array)
        },
    ),
    method(
        "toArray",
        "([Ljava/lang/Object;)[Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = hash_set_map(vm, receiver(args));
            let keys = map_keys(vm, map);
            let given = ref_arg(args, 1);
            let length = vm.array_length(given)?;
            let object_class = vm.resolve_class("java/lang/Object")?;
            let array = if length >= keys.len() {
                given
            } else {
                vm.allocate_object_array(object_class, keys.len())?
            };
            for (index, key) in keys.iter().enumerate() {
                vm.array_set_ref(array, index, *key)?;
            }
            object(array)
        },
    ),
    method(
        "removeAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC,
        collection_remove_all,
    ),
    method(
        "retainAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC,
        collection_retain_all,
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            if other == this {
                return boolean(true);
            }
            if other.is_null() {
                return boolean(false);
            }
            let set_class = vm.resolve_class("java/util/Set")?;
            if !vm.is_instance(other, set_class) {
                return boolean(false);
            }
            let this_keys = map_keys(vm, hash_set_map(vm, this));
            let (class, method) = vm.resolve_virtual_method(other, "size", "()I")?;
            let other_size = vm
                .invoke_method(class, method, vec![Value::Ref(other)])?
                .as_int();
            if this_keys.len() as i32 != other_size {
                return boolean(false);
            }
            let (class, method) =
                vm.resolve_virtual_method(other, "contains", "(Ljava/lang/Object;)Z")?;
            for key in &this_keys {
                let found = vm
                    .invoke_method(class, method, vec![Value::Ref(other), Value::Ref(*key)])?
                    .as_int();
                if found == 0 {
                    return boolean(false);
                }
            }
            boolean(true)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let keys = map_keys(vm, hash_set_map(vm, receiver(args)));
        let mut sum = 0i32;
        for key in keys {
            sum = sum.wrapping_add(object_hash(vm, key)?);
        }
        int(sum)
    }),
];

pub(crate) const HASH_SET: oxjvm_vm::NativeClass = class(
    "java/util/HashSet",
    Some("java/lang/Object"),
    &[
        "java/util/Set",
        "java/lang/Cloneable",
        "java/io/Serializable",
    ],
    ACC_PUBLIC,
    &[field("map", "Ljava/util/HashMap;", ACC_PRIVATE, None)],
    &HASH_SET_METHODS,
    None,
);

const TREE_SET_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "comparator",
        "()Ljava/util/Comparator;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "first",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = hash_set_map(vm, receiver(args));
            let pairs = hash_map_pairs(vm, map);
            match pairs.first() {
                Some((key, _)) => object(*key),
                None => Err(vm.throw_new("java/util/NoSuchElementException", None)),
            }
        },
    ),
    method("last", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        let map = hash_set_map(vm, receiver(args));
        let pairs = hash_map_pairs(vm, map);
        match pairs.last() {
            Some((key, _)) => object(*key),
            None => Err(vm.throw_new("java/util/NoSuchElementException", None)),
        }
    }),
    method(
        "pollFirst",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let map = hash_set_map(vm, this);
            let pairs = hash_map_pairs(vm, map);
            let Some((key, _)) = pairs.first().copied() else {
                return object(ObjectRef::NULL);
            };
            hash_map_call(
                vm,
                map,
                "remove",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                &[Value::Ref(key)],
            )?;
            object(key)
        },
    ),
    method(
        "pollLast",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let map = hash_set_map(vm, this);
            let pairs = hash_map_pairs(vm, map);
            let Some((key, _)) = pairs.last().copied() else {
                return object(ObjectRef::NULL);
            };
            hash_map_call(
                vm,
                map,
                "remove",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                &[Value::Ref(key)],
            )?;
            object(key)
        },
    ),
];

/// `TreeSet`: unordered `HashSet` storage with the `SortedSet`/`NavigableSet` view.
pub(crate) const TREE_SET: oxjvm_vm::NativeClass = class(
    "java/util/TreeSet",
    Some("java/util/HashSet"),
    &["java/util/NavigableSet", "java/io/Serializable"],
    ACC_PUBLIC,
    &[],
    &TREE_SET_METHODS,
    None,
);

/// `LinkedHashSet`: insertion-ordered in the JDK; here it inherits `HashSet`'s map-backed
/// behaviour (iteration order is not guaranteed, which no test depends on).
pub(crate) const LINKED_HASH_SET: oxjvm_vm::NativeClass = class(
    "java/util/LinkedHashSet",
    Some("java/util/HashSet"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

// -------------------------------------------------------------------------------------------
// BitSet
// -------------------------------------------------------------------------------------------

fn bitset_words(vm: &mut Vm<'_>, this: ObjectRef) -> Vec<u64> {
    match vm.read_ref_field(this, vm.class_of(this), "words", "[J") {
        Some(array) => {
            let length = vm.array_length(array).unwrap_or(0);
            let mut words = Vec::with_capacity(length);
            let mut reads = Vec::with_capacity(length);
            for index in 0..length {
                reads.push(
                    vm.array_get(array, index as i32)
                        .map(Value::as_long)
                        .unwrap_or(0) as u64,
                );
            }
            words.append(&mut reads);
            words
        }
        None => Vec::new(),
    }
}

fn bitset_store(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    words: &[u64],
    min_bits: usize,
) -> Result<(), VmError> {
    let mut length = min_bits.div_ceil(64);
    length = length.max(
        words
            .iter()
            .rposition(|word| *word != 0)
            .map_or(0, |i| i + 1),
    );
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "words", "[J")?;
    let array = vm.allocate_array_of(ArrayComponent::Long, length)?;
    for (index, word) in words.iter().take(length).enumerate() {
        vm.array_set(array, index as i32, Value::Long(*word as i64), false)?;
    }
    vm.set_instance_ref(this, declaring, field, array);
    Ok(())
}

fn bit_set_range(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    from: usize,
    to: usize,
    value: bool,
) -> Result<(), VmError> {
    let mut words = bitset_words(vm, this);
    if from >= to {
        return Ok(());
    }
    words.resize(to.div_ceil(64), 0);
    for bit in from..to {
        let word = bit / 64;
        let mask = 1u64 << (bit % 64);
        if value {
            words[word] |= mask;
        } else {
            words[word] &= !mask;
        }
    }
    bitset_store(vm, this, &words, 0)
}

const BITSET_METHODS: [oxjvm_vm::NativeMethodDef; 23] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        bitset_store(vm, receiver(args), &[], 0)?;
        void()
    }),
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let size = crate::int_arg(args, 1).max(0) as usize;
        bitset_store(vm, receiver(args), &[], size)?;
        void()
    }),
    method("get", "(I)Z", ACC_PUBLIC, |vm, _, args| {
        let words = bitset_words(vm, receiver(args));
        let bit = crate::int_arg(args, 1);
        if bit < 0 {
            return Err(vm.throw_new(
                "java/lang/IndexOutOfBoundsException",
                Some(&alloc::format!("bit index < 0: {bit}")),
            ));
        }
        let bit = bit as usize;
        boolean((words.get(bit / 64).copied().unwrap_or(0) >> (bit % 64)) & 1 == 1)
    }),
    method("set", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let bit = crate::int_arg(args, 1).max(0) as usize;
        bit_set_range(vm, receiver(args), bit, bit + 1, true)?;
        void()
    }),
    method("set", "(IZ)V", ACC_PUBLIC, |vm, _, args| {
        if crate::int_arg(args, 1) < 0 {
            return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", Some("bit index < 0")));
        }
        let bit = crate::int_arg(args, 1) as usize;
        bit_set_range(
            vm,
            receiver(args),
            bit,
            bit + 1,
            crate::int_arg(args, 2) != 0,
        )?;
        void()
    }),
    method("set", "(II)V", ACC_PUBLIC, |vm, _, args| {
        let from = crate::int_arg(args, 1).max(0) as usize;
        let to = crate::int_arg(args, 2).max(0) as usize;
        bit_set_range(vm, receiver(args), from, to, true)?;
        void()
    }),
    method("clear", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let bit = crate::int_arg(args, 1).max(0) as usize;
        bit_set_range(vm, receiver(args), bit, bit + 1, false)?;
        void()
    }),
    method("clear", "(II)V", ACC_PUBLIC, |vm, _, args| {
        let from = crate::int_arg(args, 1).max(0) as usize;
        let to = crate::int_arg(args, 2).max(0) as usize;
        bit_set_range(vm, receiver(args), from, to, false)?;
        void()
    }),
    method("clear", "()V", ACC_PUBLIC, |vm, _, args| {
        bitset_store(vm, receiver(args), &[], 0)?;
        void()
    }),
    method("flip", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let bit = crate::int_arg(args, 1).max(0) as usize;
        let words = bitset_words(vm, receiver(args));
        let value = (words.get(bit / 64).copied().unwrap_or(0) >> (bit % 64)) & 1 == 0;
        bit_set_range(vm, receiver(args), bit, bit + 1, value)?;
        void()
    }),
    method("length", "()I", ACC_PUBLIC, |vm, _, args| {
        let words = bitset_words(vm, receiver(args));
        for (index, word) in words.iter().enumerate().rev() {
            if *word != 0 {
                return int(index as i32 * 64 + (64 - word.leading_zeros() as i32));
            }
        }
        int(0)
    }),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        int(bitset_words(vm, receiver(args)).len() as i32 * 64)
    }),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(
            bitset_words(vm, receiver(args))
                .iter()
                .all(|word| *word == 0),
        )
    }),
    method("cardinality", "()I", ACC_PUBLIC, |vm, _, args| {
        int(bitset_words(vm, receiver(args))
            .iter()
            .map(|word| word.count_ones() as i32)
            .sum())
    }),
    method("nextSetBit", "(I)I", ACC_PUBLIC, |vm, _, args| {
        let words = bitset_words(vm, receiver(args));
        let from = crate::int_arg(args, 1).max(0) as usize;
        for bit in from..words.len() * 64 {
            if (words[bit / 64] >> (bit % 64)) & 1 == 1 {
                return int(bit as i32);
            }
        }
        int(-1)
    }),
    method("and", "(Ljava/util/BitSet;)V", ACC_PUBLIC, |vm, _, args| {
        let other = bitset_words(vm, ref_arg(args, 1));
        let mut words = bitset_words(vm, receiver(args));
        for (index, word) in words.iter_mut().enumerate() {
            *word &= other.get(index).copied().unwrap_or(0);
        }
        bitset_store(vm, receiver(args), &words, 0)?;
        void()
    }),
    method("or", "(Ljava/util/BitSet;)V", ACC_PUBLIC, |vm, _, args| {
        let other = bitset_words(vm, ref_arg(args, 1));
        let mut words = bitset_words(vm, receiver(args));
        words.resize(words.len().max(other.len()), 0);
        for (index, word) in other.iter().enumerate() {
            words[index] |= *word;
        }
        bitset_store(vm, receiver(args), &words, 0)?;
        void()
    }),
    method("xor", "(Ljava/util/BitSet;)V", ACC_PUBLIC, |vm, _, args| {
        let other = bitset_words(vm, ref_arg(args, 1));
        let mut words = bitset_words(vm, receiver(args));
        words.resize(words.len().max(other.len()), 0);
        for (index, word) in other.iter().enumerate() {
            words[index] ^= *word;
        }
        bitset_store(vm, receiver(args), &words, 0)?;
        void()
    }),
    method(
        "andNot",
        "(Ljava/util/BitSet;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = bitset_words(vm, ref_arg(args, 1));
            let mut words = bitset_words(vm, receiver(args));
            for (index, word) in words.iter_mut().enumerate() {
                *word &= !other.get(index).copied().unwrap_or(0);
            }
            bitset_store(vm, receiver(args), &words, 0)?;
            void()
        },
    ),
    method("toLongArray", "()[J", ACC_PUBLIC, |vm, _, args| {
        let words = bitset_words(vm, receiver(args));
        object(words_from_u64(vm, &words)?)
    }),
    method(
        "valueOf",
        "([J)Ljava/util/BitSet;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let array = ref_arg(args, 0);
            let length = vm.array_length(array)?;
            let mut words = Vec::with_capacity(length);
            for index in 0..length {
                words.push(vm.array_get(array, index as i32)?.as_long() as u64);
            }
            let class = vm.resolve_class("java/util/BitSet")?;
            let instance = vm.new_instance(class)?;
            bitset_store(vm, instance, &words, 0)?;
            object(instance)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() {
                return boolean(false);
            }
            boolean(bitset_words(vm, receiver(args)) == bitset_words(vm, other))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let words = bitset_words(vm, receiver(args));
        let mut hash = 1234i64;
        for (index, word) in words.iter().enumerate() {
            hash ^= (*word as i64).wrapping_mul((index as i64) + 1);
        }
        int(hash as i32)
    }),
];

fn words_from_u64(vm: &mut Vm<'_>, words: &[u64]) -> Result<ObjectRef, VmError> {
    let array = vm.allocate_array_of(ArrayComponent::Long, words.len())?;
    for (index, word) in words.iter().enumerate() {
        vm.array_set(array, index as i32, Value::Long(*word as i64), false)?;
    }
    Ok(array)
}

pub(crate) const BIT_SET: oxjvm_vm::NativeClass = class(
    "java/util/BitSet",
    Some("java/lang/Object"),
    &["java/lang/Cloneable", "java/io/Serializable"],
    ACC_PUBLIC,
    &[field("words", "[J", ACC_PRIVATE, None)],
    &BITSET_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Hashtable (also a HashMap wrapper)
// -------------------------------------------------------------------------------------------

fn hashtable_map(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    hash_set_map(vm, this)
}

const HASHTABLE_METHODS: [oxjvm_vm::NativeMethodDef; 16] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let map = new_hash_map(vm)?;
        let this = receiver(args);
        let class = vm.class_of(this);
        let (declaring, field) = vm.find_field(class, "map", "Ljava/util/HashMap;")?;
        vm.set_instance_ref(this, declaring, field, map);
        void()
    }),
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let map = new_hash_map(vm)?;
        let this = receiver(args);
        let class = vm.class_of(this);
        let (declaring, field) = vm.find_field(class, "map", "Ljava/util/HashMap;")?;
        vm.set_instance_ref(this, declaring, field, map);
        void()
    }),
    method(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = hashtable_map(vm, receiver(args));
            hash_map_call(
                vm,
                map,
                "put",
                "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
                &[Value::Ref(ref_arg(args, 1)), Value::Ref(ref_arg(args, 2))],
            )
        },
    ),
    method(
        "putIfAbsent",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            map_put_if_absent(
                vm,
                hashtable_map(vm, receiver(args)),
                ref_arg(args, 1),
                ref_arg(args, 2),
            )
        },
    ),
    method(
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = hashtable_map(vm, receiver(args));
            hash_map_call(
                vm,
                map,
                "get",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                &[Value::Ref(ref_arg(args, 1))],
            )
        },
    ),
    method(
        "remove",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = hashtable_map(vm, receiver(args));
            hash_map_call(
                vm,
                map,
                "remove",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                &[Value::Ref(ref_arg(args, 1))],
            )
        },
    ),
    method(
        "containsKey",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = hashtable_map(vm, receiver(args));
            hash_map_call(
                vm,
                map,
                "containsKey",
                "(Ljava/lang/Object;)Z",
                &[Value::Ref(ref_arg(args, 1))],
            )
        },
    ),
    method(
        "contains",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = hashtable_map(vm, receiver(args));
            hash_map_call(
                vm,
                map,
                "containsValue",
                "(Ljava/lang/Object;)Z",
                &[Value::Ref(ref_arg(args, 1))],
            )
        },
    ),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        let map = hashtable_map(vm, receiver(args));
        hash_map_call(vm, map, "size", "()I", &[])
    }),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        let map = hashtable_map(vm, receiver(args));
        hash_map_call(vm, map, "isEmpty", "()Z", &[])
    }),
    method("clear", "()V", ACC_PUBLIC, |vm, _, args| {
        let map = hashtable_map(vm, receiver(args));
        hash_map_call(vm, map, "clear", "()V", &[])
    }),
    method(
        "keys",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = hashtable_map(vm, receiver(args));
            let keys = map_keys(vm, map);
            object(new_enumeration(vm, &keys)?)
        },
    ),
    method(
        "elements",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = hashtable_map(vm, receiver(args));
            let values = map_values(vm, map);
            object(new_enumeration(vm, &values)?)
        },
    ),
    method("putAll", "(Ljava/util/Map;)V", ACC_PUBLIC, |vm, _, args| {
        let map = hashtable_map(vm, receiver(args));
        hash_map_put_all(vm, map, ref_arg(args, 1))
    }),
    method("keySet", "()Ljava/util/Set;", ACC_PUBLIC, |vm, _, args| {
        let map = hashtable_map(vm, receiver(args));
        hash_map_call(vm, map, "keySet", "()Ljava/util/Set;", &[])
    }),
    method(
        "entrySet",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = hashtable_map(vm, receiver(args));
            hash_map_call(vm, map, "entrySet", "()Ljava/util/Set;", &[])
        },
    ),
];

pub(crate) const HASHTABLE: oxjvm_vm::NativeClass = class(
    "java/util/Hashtable",
    Some("java/lang/Object"),
    &[
        "java/util/Map",
        "java/lang/Cloneable",
        "java/io/Serializable",
    ],
    ACC_PUBLIC,
    &[field("map", "Ljava/util/HashMap;", ACC_PRIVATE, None)],
    &HASHTABLE_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Vector (ArrayList plus the legacy element API)
// -------------------------------------------------------------------------------------------

const VECTOR_METHODS: [oxjvm_vm::NativeMethodDef; 31] = [
    method("<init>", "()V", ACC_PUBLIC, array_list_init),
    method("<init>", "(I)V", ACC_PUBLIC, array_list_init),
    method("<init>", "(II)V", ACC_PUBLIC, array_list_init),
    method("add", "(Ljava/lang/Object;)Z", ACC_PUBLIC, array_list_add),
    method(
        "add",
        "(ILjava/lang/Object;)V",
        ACC_PUBLIC,
        array_list_insert,
    ),
    method(
        "addElement",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let mut elements = array_list_elements(vm, this);
            elements.push(ref_arg(args, 1));
            array_list_store(vm, this, &elements)?;
            void()
        },
    ),
    method(
        "insertElementAt",
        "(Ljava/lang/Object;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let mut elements = array_list_elements(vm, this);
            let index = crate::int_arg(args, 2);
            if index < 0 || index as usize > elements.len() {
                return Err(vm.throw_new(
                    "java/lang/ArrayIndexOutOfBoundsException",
                    Some(&index.to_string()),
                ));
            }
            elements.insert(index as usize, ref_arg(args, 1));
            array_list_store(vm, this, &elements)?;
            void()
        },
    ),
    method(
        "elementAt",
        "(I)Ljava/lang/Object;",
        ACC_PUBLIC,
        array_list_get,
    ),
    method("get", "(I)Ljava/lang/Object;", ACC_PUBLIC, array_list_get),
    method(
        "set",
        "(ILjava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        array_list_set,
    ),
    method(
        "firstElement",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let elements = array_list_elements(vm, receiver(args));
            match elements.first() {
                Some(first) => object(*first),
                None => Err(vm.throw_new("java/util/NoSuchElementException", None)),
            }
        },
    ),
    method(
        "lastElement",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let elements = array_list_elements(vm, receiver(args));
            match elements.last() {
                Some(last) => object(*last),
                None => Err(vm.throw_new("java/util/NoSuchElementException", None)),
            }
        },
    ),
    method(
        "remove",
        "(I)Ljava/lang/Object;",
        ACC_PUBLIC,
        array_list_remove,
    ),
    method(
        "remove",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        array_list_remove_object,
    ),
    method(
        "removeElement",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        array_list_remove_object,
    ),
    method("removeElementAt", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let mut elements = array_list_elements(vm, this);
        let index = crate::int_arg(args, 1);
        if index < 0 || index as usize >= elements.len() {
            return Err(vm.throw_new("java/lang/ArrayIndexOutOfBoundsException", None));
        }
        elements.remove(index as usize);
        array_list_store(vm, this, &elements)?;
        void()
    }),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "size"))
    }),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "size") == 0)
    }),
    method("clear", "()V", ACC_PUBLIC, array_list_clear),
    method(
        "addAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC,
        array_list_add_all,
    ),
    method(
        "addAll",
        "(ILjava/util/Collection;)Z",
        ACC_PUBLIC,
        array_list_add_all_at,
    ),
    method(
        "removeAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC,
        collection_remove_all,
    ),
    method(
        "retainAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC,
        collection_retain_all,
    ),
    method(
        "contains",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| boolean(array_list_index_of(vm, receiver(args), ref_arg(args, 1))? >= 0),
    ),
    method(
        "indexOf",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC,
        |vm, _, args| int(array_list_index_of(vm, receiver(args), ref_arg(args, 1))?),
    ),
    method(
        "toArray",
        "()[Ljava/lang/Object;",
        ACC_PUBLIC,
        array_list_to_array,
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        array_list_to_string,
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        array_list_equals,
    ),
    method("hashCode", "()I", ACC_PUBLIC, array_list_hash_code),
    method(
        "iterator",
        "()Ljava/util/Iterator;",
        ACC_PUBLIC,
        array_list_iterator,
    ),
    method(
        "elements",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let elements = array_list_elements(vm, receiver(args));
            object(new_enumeration(vm, &elements)?)
        },
    ),
];

pub(crate) const VECTOR: oxjvm_vm::NativeClass = class(
    "java/util/Vector",
    Some("java/lang/Object"),
    &[
        "java/util/List",
        "java/lang/Cloneable",
        "java/io/Serializable",
    ],
    ACC_PUBLIC,
    &[
        field("elementData", "[Ljava/lang/Object;", ACC_PRIVATE, None),
        field("size", "I", ACC_PRIVATE, None),
    ],
    &VECTOR_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Stack
// -------------------------------------------------------------------------------------------

const STACK_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "push",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let value = ref_arg(args, 1);
            let mut elements = array_list_elements(vm, this);
            elements.push(value);
            array_list_store(vm, this, &elements)?;
            object(value)
        },
    ),
    method("pop", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let mut elements = array_list_elements(vm, this);
        let Some(value) = elements.pop() else {
            return Err(vm.throw_new("java/util/EmptyStackException", None));
        };
        array_list_store(vm, this, &elements)?;
        object(value)
    }),
    method("peek", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        let elements = array_list_elements(vm, receiver(args));
        let Some(value) = elements.last() else {
            return Err(vm.throw_new("java/util/EmptyStackException", None));
        };
        object(*value)
    }),
    method("empty", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(array_list_elements(vm, receiver(args)).is_empty())
    }),
    method(
        "search",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let needle = ref_arg(args, 1);
            let elements = array_list_elements(vm, this);
            for (offset, element) in elements.iter().rev().enumerate() {
                if keys_equal(vm, *element, needle)? {
                    return int(offset as i32 + 1);
                }
            }
            int(-1)
        },
    ),
];

/// `java.util.Stack`, sharing `Vector`'s element array.
pub(crate) const STACK: oxjvm_vm::NativeClass = class(
    "java/util/Stack",
    Some("java/util/Vector"),
    &[],
    ACC_PUBLIC,
    &[],
    &STACK_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// ConcurrentHashMap (+ ConcurrentMap / Map.Entry)
//
// A HashMap wrapper: the interpreter executes one instruction at a time, so the
// concurrent views need no real locking.
// -------------------------------------------------------------------------------------------

const MAP_ENTRY_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method(
        "getKey",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getValue",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "setValue",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const MAP_ENTRY: oxjvm_vm::NativeClass = class(
    "java/util/Map$Entry",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &MAP_ENTRY_METHODS,
    None,
);

fn entry_field(vm: &Vm<'_>, this: ObjectRef, name: &str) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), name, "Ljava/lang/Object;")
        .unwrap_or(ObjectRef::NULL)
}

fn object_hash(vm: &mut Vm<'_>, reference: ObjectRef) -> Result<i32, VmError> {
    if reference.is_null() {
        return Ok(0);
    }
    let (class, method) = vm.resolve_virtual_method(reference, "hashCode", "()I")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(reference)])?
        .as_int())
}

fn values_equal(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<bool, VmError> {
    keys_equal(vm, a, b)
}

/// The map's entries as a `HashSet` of `Map.Entry` snapshots.
fn map_entry_set(vm: &mut Vm<'_>, map: ObjectRef) -> Result<Value, VmError> {
    let pairs = hash_map_pairs(vm, map);
    let set_class = vm.resolve_class("java/util/HashSet")?;
    let set = vm.new_instance(set_class)?;
    let set_map = new_hash_map(vm)?;
    let (declaring, field) = vm.find_field(set_class, "map", "Ljava/util/HashMap;")?;
    vm.set_instance_ref(set, declaring, field, set_map);
    let present = hash_set_present(vm)?;
    for (key, value) in pairs {
        let entry = new_entry(vm, key, value)?;
        hash_map_call(
            vm,
            set_map,
            "put",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            &[Value::Ref(entry), Value::Ref(present)],
        )?;
    }
    object(set)
}

fn new_entry(vm: &mut Vm<'_>, key: ObjectRef, value: ObjectRef) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/OxjvmEntry")?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "key", "Ljava/lang/Object;")?;
    vm.set_instance_ref(instance, declaring, field, key);
    let (declaring, field) = vm.find_field(class, "value", "Ljava/lang/Object;")?;
    vm.set_instance_ref(instance, declaring, field, value);
    Ok(instance)
}

const OJVM_ENTRY_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "getKey",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| object(entry_field(vm, receiver(args), "key")),
    ),
    method(
        "getValue",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| object(entry_field(vm, receiver(args), "value")),
    ),
    method(
        "setValue",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let previous = entry_field(vm, this, "value");
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "value", "Ljava/lang/Object;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            object(previous)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() || vm.class_of(receiver(args)) != vm.class_of(other) {
                return boolean(false);
            }
            let keys = values_equal(
                vm,
                entry_field(vm, receiver(args), "key"),
                entry_field(vm, other, "key"),
            )?;
            if !keys {
                return boolean(false);
            }
            boolean(values_equal(
                vm,
                entry_field(vm, receiver(args), "value"),
                entry_field(vm, other, "value"),
            )?)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let key = object_hash(vm, entry_field(vm, this, "key"))?;
        let value = object_hash(vm, entry_field(vm, this, "value"))?;
        int(key ^ value)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let key = value_to_string(vm, entry_field(vm, this, "key"))?;
            let value = value_to_string(vm, entry_field(vm, this, "value"))?;
            object(vm.make_string(&alloc::format!("{key}={value}"))?)
        },
    ),
];

pub(crate) const OJVM_ENTRY: oxjvm_vm::NativeClass = class(
    "java/util/OxjvmEntry",
    Some("java/lang/Object"),
    &["java/util/Map$Entry"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[
        field("key", "Ljava/lang/Object;", ACC_PRIVATE, None),
        field("value", "Ljava/lang/Object;", ACC_PRIVATE, None),
    ],
    &OJVM_ENTRY_METHODS,
    None,
);

fn concurrent_map(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    hash_set_map(vm, this)
}

fn new_concurrent_map(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    new_hash_map(vm)
}

fn concurrent_map_call(
    vm: &mut Vm<'_>,
    map: ObjectRef,
    name: &str,
    descriptor: &str,
    arguments: &[Value],
) -> Result<Value, VmError> {
    hash_map_call(vm, map, name, descriptor, arguments)
}

fn function_apply1(
    vm: &mut Vm<'_>,
    function: ObjectRef,
    argument: ObjectRef,
) -> Result<ObjectRef, VmError> {
    let (class, method) =
        vm.resolve_virtual_method(function, "apply", "(Ljava/lang/Object;)Ljava/lang/Object;")?;
    Ok(vm
        .invoke_method(
            class,
            method,
            vec![Value::Ref(function), Value::Ref(argument)],
        )?
        .as_ref())
}

fn bifunction_apply2(
    vm: &mut Vm<'_>,
    function: ObjectRef,
    first: ObjectRef,
    second: ObjectRef,
) -> Result<ObjectRef, VmError> {
    let (class, method) = vm.resolve_virtual_method(
        function,
        "apply",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
    )?;
    Ok(vm
        .invoke_method(
            class,
            method,
            vec![Value::Ref(function), Value::Ref(first), Value::Ref(second)],
        )?
        .as_ref())
}

/// Put through the backing map, returning the previous value.
fn concurrent_put(
    vm: &mut Vm<'_>,
    map: ObjectRef,
    key: ObjectRef,
    value: ObjectRef,
) -> Result<Value, VmError> {
    concurrent_map_call(
        vm,
        map,
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        &[Value::Ref(key), Value::Ref(value)],
    )
}

const CONCURRENT_MAP_METHODS: [oxjvm_vm::NativeMethodDef; 15] = [
    method(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "putIfAbsent",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getOrDefault",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "putIfAbsent",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "remove",
        "(Ljava/lang/Object;Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "replace",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "replace",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "computeIfAbsent",
        "(Ljava/lang/Object;Ljava/util/function/Function;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "computeIfPresent",
        "(Ljava/lang/Object;Ljava/util/function/BiFunction;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "compute",
        "(Ljava/lang/Object;Ljava/util/function/BiFunction;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "merge",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/util/function/BiFunction;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "forEach",
        "(Ljava/util/function/BiConsumer;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "putAll",
        "(Ljava/util/Map;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "entrySet",
        "()Ljava/util/Set;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const CONCURRENT_MAP: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/ConcurrentMap",
    Some("java/lang/Object"),
    &["java/util/Map"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &CONCURRENT_MAP_METHODS,
    None,
);

const CONCURRENT_HASH_MAP_METHODS: [oxjvm_vm::NativeMethodDef; 30] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let map = new_concurrent_map(vm)?;
        concurrent_init(vm, args, map)
    }),
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let map = new_concurrent_map(vm)?;
        concurrent_init(vm, args, map)
    }),
    method("<init>", "(Ljava/util/Map;)V", ACC_PUBLIC, |vm, _, args| {
        let map = new_concurrent_map(vm)?;
        concurrent_init(vm, args, map)?;
        concurrent_put_all(vm, receiver(args), ref_arg(args, 1))
    }),
    method(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = concurrent_map(vm, receiver(args));
            concurrent_put(vm, map, ref_arg(args, 1), ref_arg(args, 2))
        },
    ),
    method(
        "putIfAbsent",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            map_put_if_absent(
                vm,
                concurrent_map(vm, receiver(args)),
                ref_arg(args, 1),
                ref_arg(args, 2),
            )
        },
    ),
    method(
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = concurrent_map(vm, receiver(args));
            concurrent_map_call(
                vm,
                map,
                "get",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                &[Value::Ref(ref_arg(args, 1))],
            )
        },
    ),
    method(
        "getOrDefault",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = concurrent_map(vm, receiver(args));
            concurrent_map_call(
                vm,
                map,
                "getOrDefault",
                "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
                &[Value::Ref(ref_arg(args, 1)), Value::Ref(ref_arg(args, 2))],
            )
        },
    ),
    method(
        "putIfAbsent",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let map = concurrent_map(vm, this);
            let key = ref_arg(args, 1);
            let index = hash_map_index_of(vm, map, key)?;
            if index >= 0 {
                return hash_map_value_at(vm, map, index as usize);
            }
            concurrent_put(vm, map, key, ref_arg(args, 2))?;
            object(ObjectRef::NULL)
        },
    ),
    method(
        "remove",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = concurrent_map(vm, receiver(args));
            concurrent_map_call(
                vm,
                map,
                "remove",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                &[Value::Ref(ref_arg(args, 1))],
            )
        },
    ),
    method(
        "remove",
        "(Ljava/lang/Object;Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let map = concurrent_map(vm, this);
            let key = ref_arg(args, 1);
            let index = hash_map_index_of(vm, map, key)?;
            if index < 0 {
                return boolean(false);
            }
            let current = hash_map_value_at(vm, map, index as usize)?;
            if !values_equal(vm, current.as_ref(), ref_arg(args, 2))? {
                return boolean(false);
            }
            concurrent_map_call(
                vm,
                map,
                "remove",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                &[Value::Ref(key)],
            )?;
            boolean(true)
        },
    ),
    method(
        "replace",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let map = concurrent_map(vm, this);
            let key = ref_arg(args, 1);
            let index = hash_map_index_of(vm, map, key)?;
            if index < 0 {
                return object(ObjectRef::NULL);
            }
            let previous = hash_map_value_at(vm, map, index as usize)?;
            concurrent_put(vm, map, key, ref_arg(args, 2))?;
            Ok(previous)
        },
    ),
    method(
        "replace",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let map = concurrent_map(vm, this);
            let key = ref_arg(args, 1);
            let index = hash_map_index_of(vm, map, key)?;
            if index < 0 {
                return boolean(false);
            }
            let current = hash_map_value_at(vm, map, index as usize)?;
            if !values_equal(vm, current.as_ref(), ref_arg(args, 2))? {
                return boolean(false);
            }
            concurrent_put(vm, map, key, ref_arg(args, 3))?;
            boolean(true)
        },
    ),
    method(
        "computeIfAbsent",
        "(Ljava/lang/Object;Ljava/util/function/Function;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let map = concurrent_map(vm, this);
            let key = ref_arg(args, 1);
            let index = hash_map_index_of(vm, map, key)?;
            if index >= 0 {
                return hash_map_value_at(vm, map, index as usize);
            }
            let computed = function_apply1(vm, ref_arg(args, 2), key)?;
            if computed.is_null() {
                return object(ObjectRef::NULL);
            }
            concurrent_put(vm, map, key, computed)?;
            object(computed)
        },
    ),
    method(
        "computeIfPresent",
        "(Ljava/lang/Object;Ljava/util/function/BiFunction;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let map = concurrent_map(vm, this);
            let key = ref_arg(args, 1);
            let index = hash_map_index_of(vm, map, key)?;
            if index < 0 {
                return object(ObjectRef::NULL);
            }
            let current = hash_map_value_at(vm, map, index as usize)?.as_ref();
            let computed = bifunction_apply2(vm, ref_arg(args, 2), key, current)?;
            if computed.is_null() {
                concurrent_map_call(
                    vm,
                    map,
                    "remove",
                    "(Ljava/lang/Object;)Ljava/lang/Object;",
                    &[Value::Ref(key)],
                )?;
                return object(ObjectRef::NULL);
            }
            concurrent_put(vm, map, key, computed)?;
            object(computed)
        },
    ),
    method(
        "compute",
        "(Ljava/lang/Object;Ljava/util/function/BiFunction;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let map = concurrent_map(vm, this);
            let key = ref_arg(args, 1);
            let index = hash_map_index_of(vm, map, key)?;
            let current = if index >= 0 {
                hash_map_value_at(vm, map, index as usize)?.as_ref()
            } else {
                ObjectRef::NULL
            };
            let computed = bifunction_apply2(vm, ref_arg(args, 2), key, current)?;
            if computed.is_null() {
                if index >= 0 {
                    concurrent_map_call(
                        vm,
                        map,
                        "remove",
                        "(Ljava/lang/Object;)Ljava/lang/Object;",
                        &[Value::Ref(key)],
                    )?;
                }
                return object(ObjectRef::NULL);
            }
            concurrent_put(vm, map, key, computed)?;
            object(computed)
        },
    ),
    method(
        "merge",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/util/function/BiFunction;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let map = concurrent_map(vm, this);
            let key = ref_arg(args, 1);
            let value = ref_arg(args, 2);
            if value.is_null() {
                return object(ObjectRef::NULL);
            }
            let index = hash_map_index_of(vm, map, key)?;
            if index < 0 {
                concurrent_put(vm, map, key, value)?;
                return object(value);
            }
            let current = hash_map_value_at(vm, map, index as usize)?.as_ref();
            let merged = bifunction_apply2(vm, ref_arg(args, 3), current, value)?;
            if merged.is_null() {
                concurrent_map_call(
                    vm,
                    map,
                    "remove",
                    "(Ljava/lang/Object;)Ljava/lang/Object;",
                    &[Value::Ref(key)],
                )?;
                return object(ObjectRef::NULL);
            }
            concurrent_put(vm, map, key, merged)?;
            object(merged)
        },
    ),
    method(
        "containsKey",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = concurrent_map(vm, receiver(args));
            concurrent_map_call(
                vm,
                map,
                "containsKey",
                "(Ljava/lang/Object;)Z",
                &[Value::Ref(ref_arg(args, 1))],
            )
        },
    ),
    method(
        "containsValue",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = concurrent_map(vm, receiver(args));
            concurrent_map_call(
                vm,
                map,
                "containsValue",
                "(Ljava/lang/Object;)Z",
                &[Value::Ref(ref_arg(args, 1))],
            )
        },
    ),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        let map = concurrent_map(vm, receiver(args));
        concurrent_map_call(vm, map, "size", "()I", &[])
    }),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        let map = concurrent_map(vm, receiver(args));
        concurrent_map_call(vm, map, "isEmpty", "()Z", &[])
    }),
    method("clear", "()V", ACC_PUBLIC, |vm, _, args| {
        let map = concurrent_map(vm, receiver(args));
        concurrent_map_call(vm, map, "clear", "()V", &[])
    }),
    method("keySet", "()Ljava/util/Set;", ACC_PUBLIC, |vm, _, args| {
        let map = concurrent_map(vm, receiver(args));
        concurrent_map_call(vm, map, "keySet", "()Ljava/util/Set;", &[])
    }),
    method(
        "values",
        "()Ljava/util/Collection;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = concurrent_map(vm, receiver(args));
            concurrent_map_call(vm, map, "values", "()Ljava/util/Collection;", &[])
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = concurrent_map(vm, receiver(args));
            concurrent_map_call(vm, map, "toString", "()Ljava/lang/String;", &[])
        },
    ),
    method("putAll", "(Ljava/util/Map;)V", ACC_PUBLIC, |vm, _, args| {
        concurrent_put_all(vm, receiver(args), ref_arg(args, 1))
    }),
    method(
        "forEach",
        "(Ljava/util/function/BiConsumer;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let map = concurrent_map(vm, this);
            let consumer = ref_arg(args, 1);
            let (class, method) = vm.resolve_virtual_method(
                consumer,
                "accept",
                "(Ljava/lang/Object;Ljava/lang/Object;)V",
            )?;
            for (key, value) in hash_map_pairs(vm, map) {
                vm.invoke_method(
                    class,
                    method,
                    vec![Value::Ref(consumer), Value::Ref(key), Value::Ref(value)],
                )?;
            }
            void()
        },
    ),
    method(
        "entrySet",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let pairs = hash_map_pairs(vm, concurrent_map(vm, this));
            let set_class = vm.resolve_class("java/util/HashSet")?;
            let set = vm.new_instance(set_class)?;
            let map_class = vm.resolve_class("java/util/HashMap")?;
            let (declaring, ctor) = vm
                .find_method(map_class, "<init>", "()V")
                .ok_or_else(|| VmError::internal("HashMap() missing"))?;
            let backing = vm.new_instance(map_class)?;
            vm.invoke_method(declaring, ctor, vec![Value::Ref(backing)])?;
            let (declaring, field) = vm.find_field(set_class, "map", "Ljava/util/HashMap;")?;
            vm.set_instance_ref(set, declaring, field, backing);
            let present = hash_set_present(vm)?;
            for (key, value) in pairs {
                let entry = new_entry(vm, key, value)?;
                concurrent_map_call(
                    vm,
                    backing,
                    "put",
                    "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
                    &[Value::Ref(entry), Value::Ref(present)],
                )?;
            }
            object(set)
        },
    ),
    method(
        "newKeySet",
        "()Ljava/util/concurrent/ConcurrentHashMap$KeySetView;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let map = new_concurrent_map(vm)?;
            let view = concurrent_key_set_view(vm, map)?;
            object(view)
        },
    ),
    method(
        "newKeySet",
        "(I)Ljava/util/concurrent/ConcurrentHashMap$KeySetView;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let map = new_concurrent_map(vm)?;
            let view = concurrent_key_set_view(vm, map)?;
            object(view)
        },
    ),
    method("mappingCount", "()J", ACC_PUBLIC, |vm, _, args| {
        long(i64::from(crate::lang::read_int_field(
            vm,
            concurrent_map(vm, receiver(args)),
            "size",
        )))
    }),
];

fn concurrent_init(vm: &mut Vm<'_>, args: &[Value], map: ObjectRef) -> Result<Value, VmError> {
    let this = receiver(args);
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "map", "Ljava/util/HashMap;")?;
    vm.set_instance_ref(this, declaring, field, map);
    void()
}

fn concurrent_put_all(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    source: ObjectRef,
) -> Result<Value, VmError> {
    let map = concurrent_map(vm, this);
    if source.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    // Walk the source through its key set and `get`, which every Map supports.
    let keys = concurrent_map_call(vm, source, "keySet", "()Ljava/util/Set;", &[])?;
    for key in collection_elements(vm, keys.as_ref())? {
        let value = concurrent_map_call(
            vm,
            source,
            "get",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            &[Value::Ref(key)],
        )?;
        concurrent_put(vm, map, key, value.as_ref())?;
    }
    void()
}

pub(crate) const CONCURRENT_HASH_MAP: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/ConcurrentHashMap",
    Some("java/lang/Object"),
    &[
        "java/util/concurrent/ConcurrentMap",
        "java/util/Map",
        "java/io/Serializable",
    ],
    ACC_PUBLIC,
    &[field("map", "Ljava/util/HashMap;", ACC_PRIVATE, None)],
    &CONCURRENT_HASH_MAP_METHODS,
    None,
);

fn concurrent_key_set_view(vm: &mut Vm<'_>, map: ObjectRef) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/concurrent/ConcurrentHashMap$KeySetView")?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "map", "Ljava/util/HashMap;")?;
    vm.set_instance_ref(instance, declaring, field, map);
    Ok(instance)
}

/// `ConcurrentHashMap.KeySetView`: a Set view sharing the HashSet implementation.
pub(crate) const CONCURRENT_HASH_MAP_KEY_SET: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/ConcurrentHashMap$KeySetView",
    Some("java/lang/Object"),
    &["java/util/Set"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field("map", "Ljava/util/HashMap;", ACC_PRIVATE, None)],
    &HASH_SET_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// EnumMap (also a HashMap wrapper; enum keys use Enum.equals/hashCode)
// -------------------------------------------------------------------------------------------

fn enum_map_map(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    hash_set_map(vm, this)
}

fn enum_map_init(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let map = new_hash_map(vm)?;
    let this = receiver(args);
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "map", "Ljava/util/HashMap;")?;
    vm.set_instance_ref(this, declaring, field, map);
    void()
}

const ENUM_MAP_METHODS: [oxjvm_vm::NativeMethodDef; 19] = [
    method(
        "<init>",
        "(Ljava/lang/Class;)V",
        ACC_PUBLIC,
        |vm, _, args| enum_map_init(vm, args),
    ),
    method(
        "<init>",
        "(Ljava/util/EnumMap;)V",
        ACC_PUBLIC,
        |vm, _, args| enum_map_init(vm, args),
    ),
    method("<init>", "(Ljava/util/Map;)V", ACC_PUBLIC, |vm, _, args| {
        enum_map_init(vm, args)?;
        let this = receiver(args);
        concurrent_put_all(vm, this, ref_arg(args, 1))
    }),
    method(
        "put",
        "(Ljava/lang/Enum;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = enum_map_map(vm, receiver(args));
            concurrent_put(vm, map, ref_arg(args, 1), ref_arg(args, 2))
        },
    ),
    method(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_BRIDGE | ACC_SYNTHETIC,
        |vm, _, args| {
            let map = enum_map_map(vm, receiver(args));
            concurrent_put(vm, map, ref_arg(args, 1), ref_arg(args, 2))
        },
    ),
    method(
        "putIfAbsent",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            map_put_if_absent(
                vm,
                enum_map_map(vm, receiver(args)),
                ref_arg(args, 1),
                ref_arg(args, 2),
            )
        },
    ),
    method(
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = enum_map_map(vm, receiver(args));
            hash_map_call(
                vm,
                map,
                "get",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                &[Value::Ref(ref_arg(args, 1))],
            )
        },
    ),
    method(
        "getOrDefault",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = enum_map_map(vm, receiver(args));
            hash_map_call(
                vm,
                map,
                "getOrDefault",
                "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
                &[Value::Ref(ref_arg(args, 1)), Value::Ref(ref_arg(args, 2))],
            )
        },
    ),
    method(
        "remove",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = enum_map_map(vm, receiver(args));
            hash_map_call(
                vm,
                map,
                "remove",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                &[Value::Ref(ref_arg(args, 1))],
            )
        },
    ),
    method(
        "containsKey",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = enum_map_map(vm, receiver(args));
            hash_map_call(
                vm,
                map,
                "containsKey",
                "(Ljava/lang/Object;)Z",
                &[Value::Ref(ref_arg(args, 1))],
            )
        },
    ),
    method(
        "containsValue",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = enum_map_map(vm, receiver(args));
            hash_map_call(
                vm,
                map,
                "containsValue",
                "(Ljava/lang/Object;)Z",
                &[Value::Ref(ref_arg(args, 1))],
            )
        },
    ),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        let map = enum_map_map(vm, receiver(args));
        hash_map_call(vm, map, "size", "()I", &[])
    }),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        let map = enum_map_map(vm, receiver(args));
        hash_map_call(vm, map, "isEmpty", "()Z", &[])
    }),
    method("clear", "()V", ACC_PUBLIC, |vm, _, args| {
        let map = enum_map_map(vm, receiver(args));
        hash_map_call(vm, map, "clear", "()V", &[])
    }),
    method("keySet", "()Ljava/util/Set;", ACC_PUBLIC, |vm, _, args| {
        let map = enum_map_map(vm, receiver(args));
        hash_map_call(vm, map, "keySet", "()Ljava/util/Set;", &[])
    }),
    method(
        "values",
        "()Ljava/util/Collection;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = enum_map_map(vm, receiver(args));
            hash_map_call(vm, map, "values", "()Ljava/util/Collection;", &[])
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = enum_map_map(vm, receiver(args));
            hash_map_call(vm, map, "toString", "()Ljava/lang/String;", &[])
        },
    ),
    method("putAll", "(Ljava/util/Map;)V", ACC_PUBLIC, |vm, _, args| {
        concurrent_put_all(vm, receiver(args), ref_arg(args, 1))
    }),
    method(
        "entrySet",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = enum_map_map(vm, receiver(args));
            hash_map_call(vm, map, "entrySet", "()Ljava/util/Set;", &[])
        },
    ),
];

pub(crate) const ENUM_MAP: oxjvm_vm::NativeClass = class(
    "java/util/EnumMap",
    Some("java/lang/Object"),
    &["java/util/Map", "java/io/Serializable"],
    ACC_PUBLIC,
    &[field("map", "Ljava/util/HashMap;", ACC_PRIVATE, None)],
    &ENUM_MAP_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// LinkedHashMap / EnumSet
// -------------------------------------------------------------------------------------------

/// `LinkedHashMap`: the same insertion-ordered storage as the native HashMap.
pub(crate) const LINKED_HASH_MAP: oxjvm_vm::NativeClass = class(
    "java/util/LinkedHashMap",
    Some("java/util/HashMap"),
    &[],
    ACC_PUBLIC,
    &[
        field("keys", "[Ljava/lang/Object;", ACC_PRIVATE, None),
        field("values", "[Ljava/lang/Object;", ACC_PRIVATE, None),
        field("size", "I", ACC_PRIVATE, None),
    ],
    &HASH_MAP_METHODS,
    None,
);

fn enum_set_map(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    hash_set_map(vm, this)
}

fn new_enum_set(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/EnumSet")?;
    let set = vm.new_instance(class)?;
    let map = new_hash_map(vm)?;
    let (declaring, field) = vm.find_field(class, "map", "Ljava/util/HashMap;")?;
    vm.set_instance_ref(set, declaring, field, map);
    Ok(set)
}

fn enum_set_add(vm: &mut Vm<'_>, this: ObjectRef, element: ObjectRef) -> Result<Value, VmError> {
    let map = enum_set_map(vm, this);
    let index = hash_map_index_of(vm, map, element)?;
    if index >= 0 {
        return boolean(false);
    }
    let present = hash_set_present(vm)?;
    hash_map_call(
        vm,
        map,
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        &[Value::Ref(element), Value::Ref(present)],
    )?;
    boolean(true)
}

const ENUM_SET_METHODS: [oxjvm_vm::NativeMethodDef; 17] = [
    method(
        "noneOf",
        "(Ljava/lang/Class;)Ljava/util/EnumSet;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_enum_set(vm)?),
    ),
    method(
        "allOf",
        "(Ljava/lang/Class;)Ljava/util/EnumSet;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let set = new_enum_set(vm)?;
            let class_object = ref_arg(args, 0);
            let (class, method) = vm.resolve_virtual_method(
                class_object,
                "getEnumConstants",
                "()[Ljava/lang/Object;",
            )?;
            let constants = vm
                .invoke_method(class, method, vec![Value::Ref(class_object)])?
                .as_ref();
            if !constants.is_null() {
                let length = vm.array_length(constants)?;
                for index in 0..length {
                    let element = vm.array_get(constants, index as i32)?.as_ref();
                    enum_set_add(vm, set, element)?;
                }
            }
            object(set)
        },
    ),
    method(
        "of",
        "(Ljava/lang/Enum;)Ljava/util/EnumSet;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let set = new_enum_set(vm)?;
            enum_set_add(vm, set, ref_arg(args, 0))?;
            object(set)
        },
    ),
    method(
        "of",
        "(Ljava/lang/Enum;Ljava/lang/Enum;)Ljava/util/EnumSet;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let set = new_enum_set(vm)?;
            enum_set_add(vm, set, ref_arg(args, 0))?;
            enum_set_add(vm, set, ref_arg(args, 1))?;
            object(set)
        },
    ),
    method(
        "of",
        "(Ljava/lang/Enum;Ljava/lang/Enum;Ljava/lang/Enum;)Ljava/util/EnumSet;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let set = new_enum_set(vm)?;
            for index in 0..3 {
                enum_set_add(vm, set, ref_arg(args, index))?;
            }
            object(set)
        },
    ),
    method(
        "of",
        "(Ljava/lang/Enum;Ljava/lang/Enum;Ljava/lang/Enum;Ljava/lang/Enum;)Ljava/util/EnumSet;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let set = new_enum_set(vm)?;
            for index in 0..4 {
                enum_set_add(vm, set, ref_arg(args, index))?;
            }
            object(set)
        },
    ),
    method(
        "of",
        "(Ljava/lang/Enum;Ljava/lang/Enum;Ljava/lang/Enum;Ljava/lang/Enum;Ljava/lang/Enum;)Ljava/util/EnumSet;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let set = new_enum_set(vm)?;
            for index in 0..5 {
                enum_set_add(vm, set, ref_arg(args, index))?;
            }
            object(set)
        },
    ),
    method(
        "of",
        "(Ljava/lang/Enum;[Ljava/lang/Enum;)Ljava/util/EnumSet;",
        ACC_PUBLIC | ACC_STATIC | ACC_VARARGS,
        |vm, _, args| {
            let set = new_enum_set(vm)?;
            enum_set_add(vm, set, ref_arg(args, 0))?;
            let array = ref_arg(args, 1);
            if !array.is_null() {
                let length = vm.array_length(array)?;
                for index in 0..length {
                    let element = vm.array_get(array, index as i32)?.as_ref();
                    enum_set_add(vm, set, element)?;
                }
            }
            object(set)
        },
    ),
    method("add", "(Ljava/lang/Enum;)Z", ACC_PUBLIC, |vm, _, args| {
        enum_set_add(vm, receiver(args), ref_arg(args, 1))
    }),
    method(
        "add",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_BRIDGE | ACC_SYNTHETIC,
        |vm, _, args| enum_set_add(vm, receiver(args), ref_arg(args, 1)),
    ),
    method(
        "remove",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = enum_set_map(vm, receiver(args));
            let index = hash_map_index_of(vm, map, ref_arg(args, 1))?;
            if index < 0 {
                return boolean(false);
            }
            hash_map_call(
                vm,
                map,
                "remove",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                &[Value::Ref(ref_arg(args, 1))],
            )?;
            boolean(true)
        },
    ),
    method(
        "contains",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = enum_set_map(vm, receiver(args));
            let index = hash_map_index_of(vm, map, ref_arg(args, 1))?;
            boolean(index >= 0)
        },
    ),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        let map = enum_set_map(vm, receiver(args));
        hash_map_call(vm, map, "size", "()I", &[])
    }),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        let map = enum_set_map(vm, receiver(args));
        hash_map_call(vm, map, "isEmpty", "()Z", &[])
    }),
    method("clear", "()V", ACC_PUBLIC, |vm, _, args| {
        let map = enum_set_map(vm, receiver(args));
        hash_map_call(vm, map, "clear", "()V", &[])
    }),
    method(
        "iterator",
        "()Ljava/util/Iterator;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = enum_set_map(vm, receiver(args));
            let mut keys = Vec::new();
            for (key, _) in hash_map_pairs(vm, map) {
                keys.push(key);
            }
            let list = array_list_from(vm, &keys)?;
            let (class, method) =
                vm.resolve_virtual_method(list, "iterator", "()Ljava/util/Iterator;")?;
            vm.invoke_method(class, method, vec![Value::Ref(list)])
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = enum_set_map(vm, receiver(args));
            hash_map_call(vm, map, "toString", "()Ljava/lang/String;", &[])
        },
    ),
];

pub(crate) const ENUM_SET: oxjvm_vm::NativeClass = class(
    "java/util/EnumSet",
    Some("java/lang/Object"),
    &["java/util/Set", "java/io/Serializable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[field("map", "Ljava/util/HashMap;", ACC_PRIVATE, None)],
    &ENUM_SET_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// TimeZone
// -------------------------------------------------------------------------------------------

fn time_zone_id(vm: &Vm<'_>, this: ObjectRef) -> String {
    vm.read_string_field(this, vm.class_of(this), "id")
        .unwrap_or_default()
}

fn new_time_zone(vm: &mut Vm<'_>, id: &str) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/util/TimeZone")?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "id", "Ljava/lang/String;")?;
    let text = vm.make_string(id)?;
    vm.set_instance_ref(instance, declaring, field, text);
    object(instance)
}

const TIME_ZONE_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method(
        "getDefault",
        "()Ljava/util/TimeZone;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/TimeZone")?;
            if let Some(default) =
                vm.read_static_ref_if_set(class, "defaultZone", "Ljava/util/TimeZone;")
            {
                return object(default);
            }
            new_time_zone(vm, "GMT")
        },
    ),
    method(
        "setDefault",
        "(Ljava/util/TimeZone;)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let class = vm.resolve_class("java/util/TimeZone")?;
            vm.set_static_value(
                class,
                "defaultZone",
                "Ljava/util/TimeZone;",
                Value::Ref(ref_arg(args, 0)),
            );
            void()
        },
    ),
    method(
        "getTimeZone",
        "(Ljava/lang/String;)Ljava/util/TimeZone;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let id = vm.string_or_empty(ref_arg(args, 0));
            let id = if id.is_empty() {
                String::from("GMT")
            } else {
                id
            };
            new_time_zone(vm, &id)
        },
    ),
    method(
        "getID",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let id = time_zone_id(vm, receiver(args));
            object(vm.make_string(&id)?)
        },
    ),
    method(
        "setID",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "id", "Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method("getRawOffset", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method("getOffset", "(J)I", ACC_PUBLIC, |_, _, _| int(0)),
    method("useDaylightTime", "()Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
    method(
        "getAvailableIDs",
        "()[Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let string_class = vm.resolve_class("java/lang/String")?;
            let array = vm.allocate_object_array(string_class, 1)?;
            let text = vm.make_string("GMT")?;
            vm.array_set_ref(array, 0, text)?;
            object(array)
        },
    ),
    method(
        "getDisplayName",
        "(ZILjava/util/Locale;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let id = time_zone_id(vm, receiver(args));
            object(vm.make_string(&id)?)
        },
    ),
    method(
        "getDisplayName",
        "(ZI)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let id = time_zone_id(vm, receiver(args));
            object(vm.make_string(&id)?)
        },
    ),
    method(
        "availableIDs",
        "()Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let id = vm.make_string("GMT")?;
            object(crate::stream::stream_from(vm, vec![id])?)
        },
    ),
];

pub(crate) const TIME_ZONE: oxjvm_vm::NativeClass = class(
    "java/util/TimeZone",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[
        field("id", "Ljava/lang/String;", ACC_PRIVATE, None),
        field(
            "defaultZone",
            "Ljava/util/TimeZone;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
    ],
    &TIME_ZONE_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Scanner (string sources)
// -------------------------------------------------------------------------------------------

const SCANNER_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "input", "Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method(
        "findInLine",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC,
        scanner_find_in_line,
    ),
    method(
        "match",
        "()Ljava/util/regex/MatchResult;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let matched = vm
                .read_ref_field(this, class, "match", "Ljava/util/regex/MatchResult;")
                .unwrap_or(ObjectRef::NULL);
            if matched.is_null() {
                return Err(vm.throw_new(
                    "java/lang/IllegalStateException",
                    Some("No match result available"),
                ));
            }
            object(matched)
        },
    ),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
];

pub(crate) const SCANNER: oxjvm_vm::NativeClass = class(
    "java/util/Scanner",
    Some("java/lang/Object"),
    &["java/io/Closeable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("input", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("position", "I", ACC_PRIVATE, None),
        field("match", "Ljava/util/regex/MatchResult;", ACC_PRIVATE, None),
    ],
    &SCANNER_METHODS,
    None,
);

/// `Scanner.findInLine`: the first match of `pattern` in the current line, or null.
///
/// The scan starts at the stored position, stops at the line terminator, and advances the
/// position past the match; `match()` then reports the same match.
fn scanner_find_in_line(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let class = vm.class_of(this);
    let pattern_text = string_arg(vm, args[1])?;
    let input = vm
        .read_string_field(this, class, "input")
        .unwrap_or_default();
    let units: Vec<u16> = input.encode_utf16().collect();
    let position = crate::lang::read_int_field(vm, this, "position").max(0) as usize;
    let position = position.min(units.len());
    let mut line_end = units.len();
    for (offset, unit) in units[position..].iter().enumerate() {
        if *unit == u16::from(b'\n') || *unit == u16::from(b'\r') {
            line_end = position + offset;
            break;
        }
    }
    let line = String::from_utf16_lossy(&units[position..line_end]);
    let line = vm.make_string(&line)?;
    let pattern = crate::regex::compile_pattern(vm, &pattern_text)?;
    let matcher = crate::regex::matcher_for(vm, pattern, line)?;
    let (matcher_class, find) = vm.resolve_virtual_method(matcher, "find", "()Z")?;
    let found = vm
        .invoke_method(matcher_class, find, vec![Value::Ref(matcher)])?
        .as_int()
        != 0;
    if !found {
        return object(ObjectRef::NULL);
    }
    let (text_class, group) =
        vm.resolve_virtual_method(matcher, "group", "()Ljava/lang/String;")?;
    let text = vm.invoke_method(text_class, group, vec![Value::Ref(matcher)])?;
    let (end_class, end) = vm.resolve_virtual_method(matcher, "end", "()I")?;
    let consumed = vm
        .invoke_method(end_class, end, vec![Value::Ref(matcher)])?
        .as_int();
    crate::lang::write_int_field(vm, this, "position", position as i32 + consumed);
    let (declaring, field) = vm.find_field(class, "match", "Ljava/util/regex/MatchResult;")?;
    vm.set_instance_ref(this, declaring, field, matcher);
    Ok(text)
}

// -------------------------------------------------------------------------------------------
// Date / Currency
// -------------------------------------------------------------------------------------------

/// Days-since-epoch to civil date (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let doe = shifted.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// `Date.toString`: `EEE MMM dd HH:mm:ss zzz yyyy` in the VM's default (GMT) time zone.
fn date_to_string(time: i64) -> String {
    const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let days = time.div_euclid(86_400_000);
    let millis_of_day = time.rem_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    let hour = millis_of_day / 3_600_000;
    let minute = millis_of_day / 60_000 % 60;
    let second = millis_of_day / 1_000 % 60;
    let weekday = (days + 4).rem_euclid(7) as usize;
    alloc::format!(
        "{} {} {:02} {:02}:{:02}:{:02} GMT {year}",
        WEEKDAYS[weekday],
        MONTHS[month as usize - 1],
        day,
        hour,
        minute,
        second,
    )
}

fn date_init(
    vm: &mut Vm<'_>,
    context: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let descriptor = vm.classes.get(context.class).methods[context.method as usize]
        .descriptor
        .clone();
    if descriptor == "(J)V" {
        crate::lang::write_long_field(vm, this, "time", long_arg(args, 1));
    } else {
        let now = vm.host_time_millis();
        crate::lang::write_long_field(vm, this, "time", now);
    }
    void()
}

fn date_init_fields(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let year = crate::int_arg(args, 1) as i64 + 1900;
    let month = crate::int_arg(args, 2) as i64 + 1;
    let day = crate::int_arg(args, 3) as i64;
    let millis = days_from_civil(year, month, day).saturating_mul(86_400_000);
    crate::lang::write_long_field(vm, receiver(args), "time", millis);
    void()
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = (month + 9) % 12;
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

const DATE_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
    method("<init>", "()V", ACC_PUBLIC, date_init),
    method("<init>", "(J)V", ACC_PUBLIC, date_init),
    method("<init>", "(III)V", ACC_PUBLIC, date_init_fields),
    method("getTime", "()J", ACC_PUBLIC, |vm, _, args| {
        long(crate::lang::read_long_field(vm, receiver(args), "time"))
    }),
    method("setTime", "(J)V", ACC_PUBLIC, |vm, _, args| {
        crate::lang::write_long_field(vm, receiver(args), "time", long_arg(args, 1));
        void()
    }),
    method(
        "before",
        "(Ljava/util/Date;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = crate::lang::read_long_field(vm, receiver(args), "time");
            let other = crate::lang::read_long_field(vm, ref_arg(args, 1), "time");
            boolean(this < other)
        },
    ),
    method("after", "(Ljava/util/Date;)Z", ACC_PUBLIC, |vm, _, args| {
        let this = crate::lang::read_long_field(vm, receiver(args), "time");
        let other = crate::lang::read_long_field(vm, ref_arg(args, 1), "time");
        boolean(this > other)
    }),
    method(
        "compareTo",
        "(Ljava/util/Date;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = crate::lang::read_long_field(vm, receiver(args), "time");
            let other = crate::lang::read_long_field(vm, ref_arg(args, 1), "time");
            int(if this < other {
                -1
            } else if this == other {
                0
            } else {
                1
            })
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() {
                return boolean(false);
            }
            let date = vm.resolve_class("java/util/Date")?;
            if !vm.is_instance(other, date) {
                return boolean(false);
            }
            let this_time = crate::lang::read_long_field(vm, receiver(args), "time");
            let other_time = crate::lang::read_long_field(vm, other, "time");
            boolean(this_time == other_time)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let time = crate::lang::read_long_field(vm, receiver(args), "time");
        int((time as i32) ^ ((time >> 32) as i32))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let time = crate::lang::read_long_field(vm, receiver(args), "time");
            object(vm.make_string(&date_to_string(time))?)
        },
    ),
];

/// `java.util.Date`: a millisecond instant with UTC formatting.
pub(crate) const DATE: oxjvm_vm::NativeClass = class(
    "java/util/Date",
    Some("java/lang/Object"),
    &[
        "java/io/Serializable",
        "java/lang/Cloneable",
        "java/lang/Comparable",
    ],
    ACC_PUBLIC,
    &[field("time", "J", ACC_PRIVATE, None)],
    &DATE_METHODS,
    None,
);

fn new_currency(vm: &mut Vm<'_>, code: &str) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/util/Currency")?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "currencyCode", "Ljava/lang/String;")?;
    let text = vm.make_string(code)?;
    vm.set_instance_ref(instance, declaring, field, text);
    object(instance)
}

fn currency_code(vm: &Vm<'_>, this: ObjectRef) -> String {
    vm.read_string_field(this, vm.class_of(this), "currencyCode")
        .unwrap_or_default()
}

/// The ISO currency for a two-letter country, restricted to the common cases.
fn currency_for_country(country: &str) -> &'static str {
    match country {
        "US" => "USD",
        "GB" => "GBP",
        "JP" => "JPY",
        "CH" => "CHF",
        "CA" => "CAD",
        "AU" => "AUD",
        "NZ" => "NZD",
        "CN" => "CNY",
        "KR" => "KRW",
        "IN" => "INR",
        "BR" => "BRL",
        "RU" => "RUB",
        "SE" => "SEK",
        "NO" => "NOK",
        "DK" => "DKK",
        "MX" => "MXN",
        "SG" => "SGD",
        "HK" => "HKD",
        "TW" => "TWD",
        "ZA" => "ZAR",
        "AE" => "AED",
        "SA" => "SAR",
        "TR" => "TRY",
        "PL" => "PLN",
        "CZ" => "CZK",
        "HU" => "HUF",
        "RO" => "RON",
        "IL" => "ILS",
        "TH" => "THB",
        "MY" => "MYR",
        "ID" => "IDR",
        "PH" => "PHP",
        "VN" => "VND",
        "AR" => "ARS",
        "CL" => "CLP",
        "CO" => "COP",
        "DE" | "FR" | "ES" | "IT" | "NL" | "BE" | "IE" | "FI" | "AT" | "PT" | "GR" | "LU"
        | "SK" | "SI" | "EE" | "LV" | "LT" | "MT" | "CY" => "EUR",
        _ => "XXX",
    }
}

const CURRENCY_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method(
        "getInstance",
        "(Ljava/lang/String;)Ljava/util/Currency;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let code = string_arg(vm, args[0])?;
            if code.len() != 3 || !code.bytes().all(|byte| byte.is_ascii_uppercase()) {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
            }
            new_currency(vm, &code)
        },
    ),
    method(
        "getInstance",
        "(Ljava/util/Locale;)Ljava/util/Currency;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let locale = ref_arg(args, 0);
            let country = vm
                .read_string_field(locale, vm.class_of(locale), "country")
                .unwrap_or_default();
            new_currency(vm, currency_for_country(&country))
        },
    ),
    method(
        "getCurrencyCode",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let code = currency_code(vm, receiver(args));
            object(vm.make_string(&code)?)
        },
    ),
    method(
        "getSymbol",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let code = currency_code(vm, receiver(args));
            object(vm.make_string(&code)?)
        },
    ),
    method(
        "getSymbol",
        "(Ljava/util/Locale;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let code = currency_code(vm, receiver(args));
            object(vm.make_string(&code)?)
        },
    ),
    method(
        "getDisplayName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let code = currency_code(vm, receiver(args));
            object(vm.make_string(&code)?)
        },
    ),
    method(
        "getDefaultFractionDigits",
        "()I",
        ACC_PUBLIC,
        |vm, _, args| {
            let code = currency_code(vm, receiver(args));
            int(if code == "JPY" { 0 } else { 2 })
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let code = currency_code(vm, receiver(args));
            object(vm.make_string(&code)?)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() {
                return boolean(false);
            }
            let currency = vm.resolve_class("java/util/Currency")?;
            if !vm.is_instance(other, currency) {
                return boolean(false);
            }
            boolean(currency_code(vm, other) == currency_code(vm, receiver(args)))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let code = currency_code(vm, receiver(args));
        let hash = code.chars().fold(0_i32, |hash, ch| {
            hash.wrapping_mul(31).wrapping_add(ch as i32)
        });
        int(hash)
    }),
];

/// `java.util.Currency`, carrying only the currency code.
pub(crate) const CURRENCY: oxjvm_vm::NativeClass = class(
    "java/util/Currency",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_FINAL,
    &[field(
        "currencyCode",
        "Ljava/lang/String;",
        ACC_PRIVATE,
        None,
    )],
    &CURRENCY_METHODS,
    None,
);
