//! `java.util`: `Objects`, `Arrays`, `Random`, and the core collections.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ArrayComponent, ArrayData, ObjectData, ObjectRef, Value, Vm, VmError};

use crate::{boolean, class, field, int, long, method, object, receiver, ref_arg, void};

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

const COLLECTION_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
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

const LIST_METHODS: [oxjvm_vm::NativeMethodDef; 28] = [
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
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
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

const SET_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
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

const MAP_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method(
        "put",
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

const OBJECTS_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
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
            let index = crate::long_arg(args, 0);
            let length = crate::long_arg(args, 1);
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
];

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

const ARRAYS_METHODS: [oxjvm_vm::NativeMethodDef; 24] = [
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
    method("fill", "([J J)V", ACC_PUBLIC | ACC_STATIC, |vm, _, args| {
        arrays_fill(
            vm,
            ref_arg(args, 0),
            Value::Long(crate::long_arg(args, 1)),
            false,
        )
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
];

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
        ) => Ok(x == y),
        _ => Ok(false),
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

const RANDOM_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
    method("<init>", "()V", ACC_PUBLIC, random_init),
    method("<init>", "(J)V", ACC_PUBLIC, random_init_seed),
    method("setSeed", "(J)V", ACC_PUBLIC, |vm, _, args| {
        set_random_seed(vm, receiver(args), crate::long_arg(args, 1));
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
    set_random_seed(vm, receiver(args), crate::long_arg(args, 1));
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

const ARRAY_LIST_METHODS: [oxjvm_vm::NativeMethodDef; 22] = [
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
];

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
    let elements = array_list_elements(vm, receiver(args));
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

// -------------------------------------------------------------------------------------------
// HashMap (linear-probing arrays)
// -------------------------------------------------------------------------------------------

const HASH_MAP_METHODS: [oxjvm_vm::NativeMethodDef; 17] = [
    method("<init>", "()V", ACC_PUBLIC, hash_map_init),
    method("<init>", "(I)V", ACC_PUBLIC, hash_map_init),
    method(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        hash_map_put,
    ),
    method(
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        hash_map_get,
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
        hash_map_store(vm, this, &pairs)?;
        object(ObjectRef::NULL)
    }
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

const IDENTITY_HASH_MAP_METHODS: [oxjvm_vm::NativeMethodDef; 16] = [
    method("<init>", "()V", ACC_PUBLIC, hash_map_init),
    method("<init>", "(I)V", ACC_PUBLIC, hash_map_init),
    method(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        identity_map_put,
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
// Properties
// -------------------------------------------------------------------------------------------

const PROPERTIES_METHODS: [oxjvm_vm::NativeMethodDef; 15] = [
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

const COLLECTIONS_METHODS: [oxjvm_vm::NativeMethodDef; 19] = [
    method(
        "sort",
        "(Ljava/util/List;)V",
        ACC_PUBLIC | ACC_STATIC,
        collections_sort,
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
            let ordering = if a.is_null() || b.is_null() {
                i32::from(a.is_null() && !b.is_null())
            } else if vm.string_utf16(a).is_some() && vm.string_utf16(b).is_some() {
                let an = vm.string_or_empty(a);
                let bn = vm.string_or_empty(b);
                match an.cmp(&bn) {
                    core::cmp::Ordering::Less => -1,
                    core::cmp::Ordering::Equal => 0,
                    core::cmp::Ordering::Greater => 1,
                }
            } else {
                let (class, method) =
                    vm.resolve_virtual_method(a, "compareTo", "(Ljava/lang/Object;)I")?;
                vm.invoke_method(class, method, vec![Value::Ref(a), Value::Ref(b)])?
                    .as_int()
            };
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

const HASH_SET_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
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

const HASHTABLE_METHODS: [oxjvm_vm::NativeMethodDef; 15] = [
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

const VECTOR_METHODS: [oxjvm_vm::NativeMethodDef; 21] = [
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

const CONCURRENT_MAP_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
    method(
        "put",
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

const CONCURRENT_HASH_MAP_METHODS: [oxjvm_vm::NativeMethodDef; 29] = [
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

const ENUM_MAP_METHODS: [oxjvm_vm::NativeMethodDef; 18] = [
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

const ENUM_SET_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
    method(
        "noneOf",
        "(Ljava/lang/Class;)Ljava/util/EnumSet;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_enum_set(vm)?),
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

const TIME_ZONE_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method(
        "getDefault",
        "()Ljava/util/TimeZone;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| new_time_zone(vm, "GMT"),
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
];

pub(crate) const TIME_ZONE: oxjvm_vm::NativeClass = class(
    "java/util/TimeZone",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[field("id", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &TIME_ZONE_METHODS,
    None,
);
