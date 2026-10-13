//! `java.util.stream`: a compact, eager `Stream` over object arrays.
//!
//! Every intermediate operation snapshots the elements into a new
//! `OjvmStream`, so pipelines are simple and allocation-bounded.  Primitive
//! streams live in [`crate::intstream`].

use alloc::vec;
use alloc::vec::Vec;
use oxjvm_classfile::flags::*;

use crate::lang::abstract_method_pub;
use crate::{boolean, class, field, long, method, object, receiver, ref_arg, void};
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

pub(crate) const STREAM_CLASS: &str = "java/util/stream/OxjvmStream";

/// Snapshot the backing array of an `OjvmStream`.
fn stream_elements(vm: &Vm<'_>, this: ObjectRef) -> Vec<ObjectRef> {
    match vm.read_ref_field(this, vm.class_of(this), "elements", "[Ljava/lang/Object;") {
        Some(array) => match vm.heap.get(array).map(|object| &object.data) {
            Some(oxjvm_vm::ObjectData::Array(oxjvm_vm::ArrayData::Reference(elements))) => {
                elements.clone()
            }
            _ => Vec::new(),
        },
        None => Vec::new(),
    }
}

/// Build a new `OjvmStream` over `elements`.
pub(crate) fn stream_from(vm: &mut Vm<'_>, elements: Vec<ObjectRef>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class(STREAM_CLASS)?;
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

/// `Arrays.stream(Object[])`.
pub(crate) fn stream_of_array(vm: &mut Vm<'_>, array: ObjectRef) -> Result<Value, VmError> {
    if array.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let elements = match vm.heap.get(array).map(|object| &object.data) {
        Some(oxjvm_vm::ObjectData::Array(oxjvm_vm::ArrayData::Reference(elements))) => {
            elements.clone()
        }
        _ => Vec::new(),
    };
    object(stream_from(vm, elements)?)
}

fn call_apply(
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

fn call_test(vm: &mut Vm<'_>, predicate: ObjectRef, argument: ObjectRef) -> Result<bool, VmError> {
    let (class, method) = vm.resolve_virtual_method(predicate, "test", "(Ljava/lang/Object;)Z")?;
    Ok(vm
        .invoke_method(
            class,
            method,
            vec![Value::Ref(predicate), Value::Ref(argument)],
        )?
        .as_int()
        != 0)
}

fn call_accept(vm: &mut Vm<'_>, consumer: ObjectRef, argument: ObjectRef) -> Result<(), VmError> {
    let (class, method) = vm.resolve_virtual_method(consumer, "accept", "(Ljava/lang/Object;)V")?;
    vm.invoke_method(
        class,
        method,
        vec![Value::Ref(consumer), Value::Ref(argument)],
    )?;
    Ok(())
}

fn call_compare(
    vm: &mut Vm<'_>,
    comparator: ObjectRef,
    left: ObjectRef,
    right: ObjectRef,
) -> Result<i32, VmError> {
    let (class, method) = vm.resolve_virtual_method(
        comparator,
        "compare",
        "(Ljava/lang/Object;Ljava/lang/Object;)I",
    )?;
    Ok(vm
        .invoke_method(
            class,
            method,
            vec![Value::Ref(comparator), Value::Ref(left), Value::Ref(right)],
        )?
        .as_int())
}

fn elements_equal(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<bool, VmError> {
    if a == b {
        return Ok(true);
    }
    if a.is_null() || b.is_null() {
        return Ok(false);
    }
    let (class, method) = vm.resolve_virtual_method(a, "equals", "(Ljava/lang/Object;)Z")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(a), Value::Ref(b)])?
        .as_int()
        != 0)
}

/// Insertion sort, so a failing `compare` can propagate.
fn sort_elements(
    vm: &mut Vm<'_>,
    mut elements: Vec<ObjectRef>,
    comparator: Option<ObjectRef>,
) -> Result<Vec<ObjectRef>, VmError> {
    for i in 1..elements.len() {
        let mut j = i;
        while j > 0 {
            let greater = match comparator {
                Some(comparator) => call_compare(vm, comparator, elements[j - 1], elements[j])? > 0,
                None => {
                    let (class, method) = vm.resolve_virtual_method(
                        elements[j - 1],
                        "compareTo",
                        "(Ljava/lang/Object;)I",
                    )?;
                    vm.invoke_method(
                        class,
                        method,
                        vec![Value::Ref(elements[j - 1]), Value::Ref(elements[j])],
                    )?
                    .as_int()
                        > 0
                }
            };
            if !greater {
                break;
            }
            elements.swap(j - 1, j);
            j -= 1;
        }
    }
    Ok(elements)
}

fn stream_get(vm: &mut Vm<'_>, this: ObjectRef) -> Vec<ObjectRef> {
    stream_elements(vm, this)
}

const STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 25] = [
    method(
        "of",
        "([Ljava/lang/Object;)Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let array = ref_arg(args, 0);
            if array.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            stream_of_array(vm, array)
        },
    ),
    method(
        "of",
        "(Ljava/lang/Object;)Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(stream_from(vm, vec![ref_arg(args, 0)])?),
    ),
    method(
        "empty",
        "()Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(stream_from(vm, Vec::new())?),
    ),
    method(
        "concat",
        "(Ljava/util/stream/Stream;Ljava/util/stream/Stream;)Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let mut left = stream_elements(vm, ref_arg(args, 0));
            left.extend(stream_elements(vm, ref_arg(args, 1)));
            object(stream_from(vm, left)?)
        },
    ),
    method(
        "filter",
        "(Ljava/util/function/Predicate;)Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "map",
        "(Ljava/util/function/Function;)Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "distinct",
        "()Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "sorted",
        "()Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "sorted",
        "(Ljava/util/Comparator;)Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "forEach",
        "(Ljava/util/function/Consumer;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "forEachOrdered",
        "(Ljava/util/function/Consumer;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "toArray",
        "()[Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "toArray",
        "(Ljava/util/function/IntFunction;)[Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "count",
        "()J",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "anyMatch",
        "(Ljava/util/function/Predicate;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "allMatch",
        "(Ljava/util/function/Predicate;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "noneMatch",
        "(Ljava/util/function/Predicate;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "findFirst",
        "()Ljava/util/Optional;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "findAny",
        "()Ljava/util/Optional;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "limit",
        "(J)Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "skip",
        "(J)Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "toList",
        "()Ljava/util/List;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "parallel",
        "()Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "sequential",
        "()Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "unordered",
        "()Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
];

const BASE_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "iterator",
        "()Ljava/util/Iterator;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "isParallel",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "sequential",
        "()Ljava/util/stream/BaseStream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "parallel",
        "()Ljava/util/stream/BaseStream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "unordered",
        "()Ljava/util/stream/BaseStream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "close",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
];

pub(crate) const BASE_STREAM: oxjvm_vm::NativeClass = class(
    "java/util/stream/BaseStream",
    Some("java/lang/Object"),
    &["java/lang/AutoCloseable"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &BASE_STREAM_METHODS,
    None,
);

pub(crate) const STREAM: oxjvm_vm::NativeClass = class(
    "java/util/stream/Stream",
    Some("java/lang/Object"),
    &["java/util/stream/BaseStream"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &STREAM_METHODS,
    None,
);

const OXJVM_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 27] = [
    method(
        "filter",
        "(Ljava/util/function/Predicate;)Ljava/util/stream/Stream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let predicate = ref_arg(args, 1);
            let mut kept = Vec::new();
            for element in stream_get(vm, receiver(args)) {
                if call_test(vm, predicate, element)? {
                    kept.push(element);
                }
            }
            object(stream_from(vm, kept)?)
        },
    ),
    method(
        "map",
        "(Ljava/util/function/Function;)Ljava/util/stream/Stream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let function = ref_arg(args, 1);
            let mut mapped = Vec::new();
            for element in stream_get(vm, receiver(args)) {
                mapped.push(call_apply(vm, function, element)?);
            }
            object(stream_from(vm, mapped)?)
        },
    ),
    method(
        "distinct",
        "()Ljava/util/stream/Stream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let mut unique: Vec<ObjectRef> = Vec::new();
            for element in stream_get(vm, receiver(args)) {
                let mut seen = false;
                for candidate in &unique {
                    if elements_equal(vm, *candidate, element)? {
                        seen = true;
                        break;
                    }
                }
                if !seen {
                    unique.push(element);
                }
            }
            object(stream_from(vm, unique)?)
        },
    ),
    method(
        "sorted",
        "()Ljava/util/stream/Stream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let elements = stream_get(vm, receiver(args));
            let sorted = sort_elements(vm, elements, None)?;
            object(stream_from(vm, sorted)?)
        },
    ),
    method(
        "sorted",
        "(Ljava/util/Comparator;)Ljava/util/stream/Stream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let elements = stream_get(vm, receiver(args));
            let sorted = sort_elements(vm, elements, Some(ref_arg(args, 1)))?;
            object(stream_from(vm, sorted)?)
        },
    ),
    method(
        "forEach",
        "(Ljava/util/function/Consumer;)V",
        ACC_PUBLIC,
        |vm, _, args| stream_for_each(vm, args),
    ),
    method(
        "forEachOrdered",
        "(Ljava/util/function/Consumer;)V",
        ACC_PUBLIC,
        |vm, _, args| stream_for_each(vm, args),
    ),
    method(
        "toArray",
        "()[Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let elements = stream_get(vm, receiver(args));
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
        "(Ljava/util/function/IntFunction;)[Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let generator = ref_arg(args, 1);
            let elements = stream_get(vm, receiver(args));
            let (class, method) =
                vm.resolve_virtual_method(generator, "apply", "(I)Ljava/lang/Object;")?;
            let array = vm
                .invoke_method(
                    class,
                    method,
                    vec![Value::Ref(generator), Value::Int(elements.len() as i32)],
                )?
                .as_ref();
            for (index, element) in elements.iter().enumerate() {
                vm.array_set_ref(array, index, *element)?;
            }
            object(array)
        },
    ),
    method("count", "()J", ACC_PUBLIC, |vm, _, args| {
        long(stream_get(vm, receiver(args)).len() as i64)
    }),
    method(
        "anyMatch",
        "(Ljava/util/function/Predicate;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let predicate = ref_arg(args, 1);
            for element in stream_get(vm, receiver(args)) {
                if call_test(vm, predicate, element)? {
                    return boolean(true);
                }
            }
            boolean(false)
        },
    ),
    method(
        "allMatch",
        "(Ljava/util/function/Predicate;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let predicate = ref_arg(args, 1);
            for element in stream_get(vm, receiver(args)) {
                if !call_test(vm, predicate, element)? {
                    return boolean(false);
                }
            }
            boolean(true)
        },
    ),
    method(
        "noneMatch",
        "(Ljava/util/function/Predicate;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let predicate = ref_arg(args, 1);
            for element in stream_get(vm, receiver(args)) {
                if call_test(vm, predicate, element)? {
                    return boolean(false);
                }
            }
            boolean(true)
        },
    ),
    method(
        "findFirst",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, args| stream_find(vm, args),
    ),
    method(
        "findAny",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, args| stream_find(vm, args),
    ),
    method(
        "limit",
        "(J)Ljava/util/stream/Stream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let elements = stream_get(vm, receiver(args));
            let limit = crate::long_arg(args, 1).max(0) as usize;
            object(stream_from(
                vm,
                elements[..limit.min(elements.len())].to_vec(),
            )?)
        },
    ),
    method(
        "skip",
        "(J)Ljava/util/stream/Stream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let elements = stream_get(vm, receiver(args));
            let skip = crate::long_arg(args, 1).max(0) as usize;
            object(stream_from(
                vm,
                elements[skip.min(elements.len())..].to_vec(),
            )?)
        },
    ),
    method("toList", "()Ljava/util/List;", ACC_PUBLIC, |vm, _, args| {
        let elements = stream_get(vm, receiver(args));
        object(crate::util::array_list_from(vm, &elements)?)
    }),
    method(
        "iterator",
        "()Ljava/util/Iterator;",
        ACC_PUBLIC,
        |vm, _, args| {
            let elements = stream_get(vm, receiver(args));
            let class = vm.resolve_class("java/util/stream/OxjvmStream$Itr")?;
            let iterator = vm.new_instance(class)?;
            let object_class = vm.resolve_class("java/lang/Object")?;
            let array = vm.allocate_object_array(object_class, elements.len())?;
            for (index, element) in elements.iter().enumerate() {
                vm.array_set_ref(array, index, *element)?;
            }
            let (declaring, field) = vm.find_field(class, "elements", "[Ljava/lang/Object;")?;
            vm.set_instance_ref(iterator, declaring, field, array);
            object(iterator)
        },
    ),
    method("isParallel", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "parallel") != 0)
    }),
    method(
        "parallel",
        "()Ljava/util/stream/BaseStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            crate::lang::write_int_field(vm, receiver(args), "parallel", 1);
            object(receiver(args))
        },
    ),
    method(
        "sequential",
        "()Ljava/util/stream/BaseStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            crate::lang::write_int_field(vm, receiver(args), "parallel", 0);
            object(receiver(args))
        },
    ),
    method(
        "unordered",
        "()Ljava/util/stream/BaseStream;",
        ACC_PUBLIC,
        |_vm, _, args| object(receiver(args)),
    ),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
    // Bridge: `Stream` declares the covariant Stream-returning overloads, so the
    // concrete class exposes them with `Stream` return types too.
    method(
        "parallel",
        "()Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_BRIDGE | ACC_SYNTHETIC,
        |vm, _, args| {
            crate::lang::write_int_field(vm, receiver(args), "parallel", 1);
            object(receiver(args))
        },
    ),
    method(
        "sequential",
        "()Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_BRIDGE | ACC_SYNTHETIC,
        |vm, _, args| {
            crate::lang::write_int_field(vm, receiver(args), "parallel", 0);
            object(receiver(args))
        },
    ),
    method(
        "unordered",
        "()Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_BRIDGE | ACC_SYNTHETIC,
        |_vm, _, args| object(receiver(args)),
    ),
];

fn stream_for_each(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let consumer = ref_arg(args, 1);
    for element in stream_get(vm, receiver(args)) {
        call_accept(vm, consumer, element)?;
    }
    void()
}

fn stream_find(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let elements = stream_get(vm, receiver(args));
    match elements.first() {
        Some(element) => crate::optional::optional_new(vm, *element),
        None => crate::optional::optional_empty_value(vm),
    }
}

pub(crate) const OXJVM_STREAM: oxjvm_vm::NativeClass = class(
    "java/util/stream/OxjvmStream",
    Some("java/lang/Object"),
    &["java/util/stream/Stream", "java/util/stream/BaseStream"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[
        field("elements", "[Ljava/lang/Object;", ACC_PRIVATE, None),
        field("parallel", "I", ACC_PRIVATE, None),
    ],
    &OXJVM_STREAM_METHODS,
    None,
);

const STREAM_ITERATOR_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method("hasNext", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let elements = vm
            .read_ref_field(this, vm.class_of(this), "elements", "[Ljava/lang/Object;")
            .map_or(0, |array| vm.array_length(array).unwrap_or(0));
        boolean(crate::lang::read_int_field(vm, this, "cursor") < elements as i32)
    }),
    method("next", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
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
    }),
    method("remove", "()V", ACC_PUBLIC, abstract_method_pub),
];

pub(crate) const STREAM_ITERATOR: oxjvm_vm::NativeClass = class(
    "java/util/stream/OxjvmStream$Itr",
    Some("java/lang/Object"),
    &["java/util/Iterator"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[
        field("elements", "[Ljava/lang/Object;", ACC_PRIVATE, None),
        field("cursor", "I", ACC_PRIVATE, None),
    ],
    &STREAM_ITERATOR_METHODS,
    None,
);
