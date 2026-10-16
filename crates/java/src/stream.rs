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

const STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 29] = [
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
        "takeWhile",
        "(Ljava/util/function/Predicate;)Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "dropWhile",
        "(Ljava/util/function/Predicate;)Ljava/util/stream/Stream;",
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
    method(
        "collect",
        "(Ljava/util/stream/Collector;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "collect",
        "(Ljava/util/function/Supplier;Ljava/util/function/BiConsumer;Ljava/util/function/BiConsumer;)Ljava/lang/Object;",
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

const OXJVM_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 31] = [
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
        "takeWhile",
        "(Ljava/util/function/Predicate;)Ljava/util/stream/Stream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let predicate = ref_arg(args, 1);
            let mut kept = Vec::new();
            for element in stream_get(vm, receiver(args)) {
                if call_test(vm, predicate, element)? {
                    kept.push(element);
                } else {
                    break;
                }
            }
            object(stream_from(vm, kept)?)
        },
    ),
    method(
        "dropWhile",
        "(Ljava/util/function/Predicate;)Ljava/util/stream/Stream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let predicate = ref_arg(args, 1);
            let mut dropping = true;
            let mut kept = Vec::new();
            for element in stream_get(vm, receiver(args)) {
                if dropping && call_test(vm, predicate, element)? {
                    continue;
                }
                dropping = false;
                kept.push(element);
            }
            object(stream_from(vm, kept)?)
        },
    ),
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
    method(
        "collect",
        "(Ljava/util/stream/Collector;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| stream_collect(vm, args),
    ),
    method(
        "collect",
        "(Ljava/util/function/Supplier;Ljava/util/function/BiConsumer;Ljava/util/function/BiConsumer;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let collector =
                make_custom_collector(vm, ref_arg(args, 1), ref_arg(args, 2), ObjectRef::NULL)?;
            let elements = stream_get(vm, receiver(args));
            collect_elements(vm, collector, elements)
        },
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

// -------------------------------------------------------------------------------------------
// `Collectors` / `Collector`: an eager `collect` over the stream snapshot.
//
// The JDK's `Collectors` factories return a `CollectorImpl`; oxjvm returns an
// `OjvmCollector` that carries a small kind tag plus the functions the factory
// was given.  `Stream.collect` reads the tag and performs the reduction
// directly.  A collector that is *not* an `OjvmCollector` (a hand-written
// `Collector`) goes through the generic supplier/accumulator/finisher path.
// -------------------------------------------------------------------------------------------

pub(crate) const COLLECTOR_CLASS: &str = "java/util/stream/OjvmCollector";

const COLLECT_TO_LIST: i32 = 0;
const COLLECT_TO_SET: i32 = 1;
const COLLECT_TO_COLLECTION: i32 = 2;
const COLLECT_JOINING: i32 = 3;
const COLLECT_COUNTING: i32 = 4;
const COLLECT_SUMMING_INT: i32 = 5;
const COLLECT_SUMMING_LONG: i32 = 6;
const COLLECT_SUMMING_DOUBLE: i32 = 7;
const COLLECT_TO_MAP: i32 = 8;
const COLLECT_UNMODIFIABLE_LIST: i32 = 9;
const COLLECT_UNMODIFIABLE_SET: i32 = 10;
const COLLECT_MAPPING: i32 = 11;
const COLLECT_AVERAGING_INT: i32 = 12;
const COLLECT_AVERAGING_LONG: i32 = 13;
const COLLECT_AVERAGING_DOUBLE: i32 = 14;
const COLLECT_TO_UNMODIFIABLE_MAP: i32 = 15;
const COLLECT_CUSTOM: i32 = 16;
const COLLECT_REDUCING: i32 = 17;

fn collector_new(vm: &mut Vm<'_>, kind: i32) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class(COLLECTOR_CLASS)?;
    let collector = vm.new_instance(class)?;
    crate::lang::write_int_field(vm, collector, "kind", kind);
    Ok(collector)
}

fn collector_set(vm: &mut Vm<'_>, collector: ObjectRef, name: &str, value: ObjectRef) {
    let class = vm.class_of(collector);
    if let Some((declaring, field)) = vm.find_field_opt(class, name, "Ljava/lang/Object;") {
        vm.set_instance_ref(collector, declaring, field, value);
    }
}

fn collector_get(vm: &Vm<'_>, collector: ObjectRef, name: &str) -> ObjectRef {
    vm.read_ref_field(
        collector,
        vm.class_of(collector),
        name,
        "Ljava/lang/Object;",
    )
    .unwrap_or(ObjectRef::NULL)
}

/// `collector.kind` when `collector` is an `OjvmCollector`, else `None`.
fn collector_kind(vm: &Vm<'_>, collector: ObjectRef) -> Option<i32> {
    if collector.is_null() || vm.class_name(vm.class_of(collector)) != COLLECTOR_CLASS {
        return None;
    }
    Some(crate::lang::read_int_field(vm, collector, "kind"))
}

fn call_apply1(
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

fn call_apply2(
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

fn call_apply_as_int(
    vm: &mut Vm<'_>,
    function: ObjectRef,
    argument: ObjectRef,
) -> Result<i32, VmError> {
    let (class, method) =
        vm.resolve_virtual_method(function, "applyAsInt", "(Ljava/lang/Object;)I")?;
    Ok(vm
        .invoke_method(
            class,
            method,
            vec![Value::Ref(function), Value::Ref(argument)],
        )?
        .as_int())
}

fn call_apply_as_long(
    vm: &mut Vm<'_>,
    function: ObjectRef,
    argument: ObjectRef,
) -> Result<i64, VmError> {
    let (class, method) =
        vm.resolve_virtual_method(function, "applyAsLong", "(Ljava/lang/Object;)J")?;
    Ok(vm
        .invoke_method(
            class,
            method,
            vec![Value::Ref(function), Value::Ref(argument)],
        )?
        .as_long())
}

fn call_apply_as_double(
    vm: &mut Vm<'_>,
    function: ObjectRef,
    argument: ObjectRef,
) -> Result<f64, VmError> {
    let (class, method) =
        vm.resolve_virtual_method(function, "applyAsDouble", "(Ljava/lang/Object;)D")?;
    let bits = vm
        .invoke_method(
            class,
            method,
            vec![Value::Ref(function), Value::Ref(argument)],
        )?
        .as_long();
    Ok(f64::from_bits(bits as u64))
}

/// `Object.toString()` of an element, resolving the actual class.
fn element_text(vm: &mut Vm<'_>, value: ObjectRef) -> Result<alloc::string::String, VmError> {
    if value.is_null() {
        return Ok(alloc::string::String::from("null"));
    }
    let (class, method) = vm.resolve_virtual_method(value, "toString", "()Ljava/lang/String;")?;
    let text = vm
        .invoke_method(class, method, vec![Value::Ref(value)])?
        .as_ref();
    Ok(vm
        .string_value(text)
        .unwrap_or_else(|| alloc::string::String::from("null")))
}

/// `supplier.get()` for a `java.util.function.Supplier`.
fn call_supplier(vm: &mut Vm<'_>, supplier: ObjectRef) -> Result<ObjectRef, VmError> {
    let (class, method) = vm.resolve_virtual_method(supplier, "get", "()Ljava/lang/Object;")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(supplier)])?
        .as_ref())
}

fn build_joining(
    vm: &mut Vm<'_>,
    elements: &[ObjectRef],
    delimiter: ObjectRef,
    prefix: ObjectRef,
    suffix: ObjectRef,
) -> Result<Value, VmError> {
    let delimiter = if delimiter.is_null() {
        alloc::string::String::new()
    } else {
        vm.string_value(delimiter).unwrap_or_default()
    };
    let prefix = if prefix.is_null() {
        alloc::string::String::new()
    } else {
        vm.string_value(prefix).unwrap_or_default()
    };
    let suffix = if suffix.is_null() {
        alloc::string::String::new()
    } else {
        vm.string_value(suffix).unwrap_or_default()
    };
    let mut out = prefix;
    for (index, element) in elements.iter().enumerate() {
        if index > 0 {
            out.push_str(&delimiter);
        }
        out.push_str(&element_text(vm, *element)?);
    }
    out.push_str(&suffix);
    object(vm.make_string(&out)?)
}

fn collect_to_map(
    vm: &mut Vm<'_>,
    collector: ObjectRef,
    elements: &[ObjectRef],
) -> Result<Value, VmError> {
    let key_mapper = collector_get(vm, collector, "f1");
    let value_mapper = collector_get(vm, collector, "f2");
    let merge = collector_get(vm, collector, "f3");
    let map_supplier = collector_get(vm, collector, "f4");
    let map = if map_supplier.is_null() {
        crate::util::hash_map_new(vm)?
    } else {
        call_supplier(vm, map_supplier)?
    };
    for element in elements {
        let key = call_apply1(vm, key_mapper, *element)?;
        let value = call_apply1(vm, value_mapper, *element)?;
        if key.is_null() || value.is_null() {
            return Err(vm.throw_new(
                "java/lang/NullPointerException",
                Some("element cannot be mapped to a null key"),
            ));
        }
        let existing = crate::util::hash_map_lookup(vm, map, key)?;
        let value = if !existing.is_null() && !merge.is_null() {
            call_apply2(vm, merge, existing, value)?
        } else {
            value
        };
        crate::util::hash_map_insert(vm, map, key, value)?;
    }
    object(map)
}

fn collect_elements(
    vm: &mut Vm<'_>,
    collector: ObjectRef,
    elements: Vec<ObjectRef>,
) -> Result<Value, VmError> {
    let Some(kind) = collector_kind(vm, collector) else {
        return generic_collect(vm, collector, elements);
    };
    match kind {
        COLLECT_TO_LIST | COLLECT_UNMODIFIABLE_LIST => {
            object(crate::util::array_list_from(vm, &elements)?)
        }
        COLLECT_TO_SET | COLLECT_UNMODIFIABLE_SET => {
            object(crate::util::hash_set_from(vm, &elements)?)
        }
        COLLECT_TO_COLLECTION => {
            let supplier = collector_get(vm, collector, "f4");
            let container = call_supplier(vm, supplier)?;
            let (class, method) =
                vm.resolve_virtual_method(container, "add", "(Ljava/lang/Object;)Z")?;
            for element in &elements {
                vm.invoke_method(
                    class,
                    method,
                    vec![Value::Ref(container), Value::Ref(*element)],
                )?;
            }
            object(container)
        }
        COLLECT_JOINING => build_joining(
            vm,
            &elements,
            collector_get(vm, collector, "s1"),
            collector_get(vm, collector, "s2"),
            collector_get(vm, collector, "s3"),
        ),
        COLLECT_COUNTING => crate::boxed::boxed_long(vm, elements.len() as i64),
        COLLECT_SUMMING_INT => {
            let mapper = collector_get(vm, collector, "f1");
            let mut sum = 0i32;
            for element in &elements {
                sum = sum.wrapping_add(call_apply_as_int(vm, mapper, *element)?);
            }
            crate::boxed::boxed_int(vm, sum)
        }
        COLLECT_SUMMING_LONG => {
            let mapper = collector_get(vm, collector, "f1");
            let mut sum = 0i64;
            for element in &elements {
                sum = sum.wrapping_add(call_apply_as_long(vm, mapper, *element)?);
            }
            crate::boxed::boxed_long(vm, sum)
        }
        COLLECT_SUMMING_DOUBLE => {
            let mapper = collector_get(vm, collector, "f1");
            let mut sum = 0.0f64;
            for element in &elements {
                sum += call_apply_as_double(vm, mapper, *element)?;
            }
            crate::boxed::boxed_double(vm, sum)
        }
        COLLECT_AVERAGING_INT => {
            let mapper = collector_get(vm, collector, "f1");
            let mut sum = 0i64;
            for element in &elements {
                sum += i64::from(call_apply_as_int(vm, mapper, *element)?);
            }
            let average = if elements.is_empty() {
                0.0
            } else {
                sum as f64 / elements.len() as f64
            };
            crate::boxed::boxed_double(vm, average)
        }
        COLLECT_AVERAGING_LONG => {
            let mapper = collector_get(vm, collector, "f1");
            let mut sum = 0i64;
            for element in &elements {
                sum = sum.wrapping_add(call_apply_as_long(vm, mapper, *element)?);
            }
            let average = if elements.is_empty() {
                0.0
            } else {
                sum as f64 / elements.len() as f64
            };
            crate::boxed::boxed_double(vm, average)
        }
        COLLECT_AVERAGING_DOUBLE => {
            let mapper = collector_get(vm, collector, "f1");
            let mut sum = 0.0f64;
            for element in &elements {
                sum += call_apply_as_double(vm, mapper, *element)?;
            }
            let average = if elements.is_empty() {
                0.0
            } else {
                sum / elements.len() as f64
            };
            crate::boxed::boxed_double(vm, average)
        }
        COLLECT_TO_MAP | COLLECT_TO_UNMODIFIABLE_MAP => collect_to_map(vm, collector, &elements),
        COLLECT_MAPPING => {
            let mapper = collector_get(vm, collector, "f1");
            let downstream = collector_get(vm, collector, "f3");
            let mut mapped = Vec::with_capacity(elements.len());
            for element in &elements {
                mapped.push(call_apply1(vm, mapper, *element)?);
            }
            collect_elements(vm, downstream, mapped)
        }
        COLLECT_REDUCING => {
            let identity = collector_get(vm, collector, "f4");
            let operator = collector_get(vm, collector, "f2");
            let mut result = identity;
            for element in &elements {
                result = call_apply2(vm, operator, result, *element)?;
            }
            object(result)
        }
        COLLECT_CUSTOM => generic_collect(vm, collector, elements),
        _ => generic_collect(vm, collector, elements),
    }
}

/// The generic `Collector` protocol: `supplier`, then `accumulator` per element, then `finisher`.
fn generic_collect(
    vm: &mut Vm<'_>,
    collector: ObjectRef,
    elements: Vec<ObjectRef>,
) -> Result<Value, VmError> {
    let kind = collector_kind(vm, collector);
    let (supplier, accumulator, finisher) = if kind.is_some() {
        (
            collector_get(vm, collector, "f4"),
            collector_get(vm, collector, "f1"),
            collector_get(vm, collector, "f2"),
        )
    } else {
        let (class, method) =
            vm.resolve_virtual_method(collector, "supplier", "()Ljava/util/function/Supplier;")?;
        let supplier = vm
            .invoke_method(class, method, vec![Value::Ref(collector)])?
            .as_ref();
        let (class, method) = vm.resolve_virtual_method(
            collector,
            "accumulator",
            "()Ljava/util/function/BiConsumer;",
        )?;
        let accumulator = vm
            .invoke_method(class, method, vec![Value::Ref(collector)])?
            .as_ref();
        let (class, method) =
            vm.resolve_virtual_method(collector, "finisher", "()Ljava/util/function/Function;")?;
        let finisher = vm
            .invoke_method(class, method, vec![Value::Ref(collector)])?
            .as_ref();
        (supplier, accumulator, finisher)
    };
    let container = call_supplier(vm, supplier)?;
    let (class, method) = vm.resolve_virtual_method(
        accumulator,
        "accept",
        "(Ljava/lang/Object;Ljava/lang/Object;)V",
    )?;
    for element in &elements {
        vm.invoke_method(
            class,
            method,
            vec![
                Value::Ref(accumulator),
                Value::Ref(container),
                Value::Ref(*element),
            ],
        )?;
    }
    if finisher.is_null() {
        return object(container);
    }
    Ok(Value::Ref(call_apply1(vm, finisher, container)?))
}

fn stream_collect(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let collector = ref_arg(args, 1);
    let elements = stream_get(vm, receiver(args));
    collect_elements(vm, collector, elements)
}

/// `Collector.of(...)` / `Collectors.toCollection`: keep the arbitrary functions for the generic path.
fn make_custom_collector(
    vm: &mut Vm<'_>,
    supplier: ObjectRef,
    accumulator: ObjectRef,
    finisher: ObjectRef,
) -> Result<ObjectRef, VmError> {
    let collector = collector_new(vm, COLLECT_CUSTOM)?;
    collector_set(vm, collector, "f4", supplier);
    collector_set(vm, collector, "f1", accumulator);
    collector_set(vm, collector, "f2", finisher);
    Ok(collector)
}

const COLLECTOR_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "of",
        "(Ljava/util/function/Supplier;Ljava/util/function/BiConsumer;Ljava/util/function/BinaryOperator;)Ljava/util/stream/Collector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let collector =
                make_custom_collector(vm, ref_arg(args, 0), ref_arg(args, 1), ObjectRef::NULL)?;
            object(collector)
        },
    ),
    method(
        "of",
        "(Ljava/util/function/Supplier;Ljava/util/function/BiConsumer;Ljava/util/function/BinaryOperator;Ljava/util/function/Function;[Ljava/util/stream/Collector$Characteristics;)Ljava/util/stream/Collector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let collector =
                make_custom_collector(vm, ref_arg(args, 0), ref_arg(args, 1), ref_arg(args, 3))?;
            object(collector)
        },
    ),
    method(
        "supplier",
        "()Ljava/util/function/Supplier;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "accumulator",
        "()Ljava/util/function/BiConsumer;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "finisher",
        "()Ljava/util/function/Function;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "characteristics",
        "()Ljava/util/Set;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
];

pub(crate) const COLLECTOR: oxjvm_vm::NativeClass = class(
    "java/util/stream/Collector",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &COLLECTOR_METHODS,
    None,
);

const COLLECTORS_METHODS: [oxjvm_vm::NativeMethodDef; 22] = [
    method(
        "toList",
        "()Ljava/util/stream/Collector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(collector_new(vm, COLLECT_TO_LIST)?),
    ),
    method(
        "toUnmodifiableList",
        "()Ljava/util/stream/Collector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(collector_new(vm, COLLECT_UNMODIFIABLE_LIST)?),
    ),
    method(
        "toSet",
        "()Ljava/util/stream/Collector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(collector_new(vm, COLLECT_TO_SET)?),
    ),
    method(
        "toUnmodifiableSet",
        "()Ljava/util/stream/Collector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(collector_new(vm, COLLECT_UNMODIFIABLE_SET)?),
    ),
    method(
        "toCollection",
        "(Ljava/util/function/Supplier;)Ljava/util/stream/Collector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let collector = collector_new(vm, COLLECT_TO_COLLECTION)?;
            collector_set(vm, collector, "f4", ref_arg(args, 0));
            object(collector)
        },
    ),
    method(
        "joining",
        "()Ljava/util/stream/Collector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(collector_new(vm, COLLECT_JOINING)?),
    ),
    method(
        "joining",
        "(Ljava/lang/CharSequence;)Ljava/util/stream/Collector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let collector = collector_new(vm, COLLECT_JOINING)?;
            collector_set(vm, collector, "s1", ref_arg(args, 0));
            object(collector)
        },
    ),
    method(
        "joining",
        "(Ljava/lang/CharSequence;Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Ljava/util/stream/Collector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let collector = collector_new(vm, COLLECT_JOINING)?;
            collector_set(vm, collector, "s1", ref_arg(args, 0));
            collector_set(vm, collector, "s2", ref_arg(args, 1));
            collector_set(vm, collector, "s3", ref_arg(args, 2));
            object(collector)
        },
    ),
    method(
        "counting",
        "()Ljava/util/stream/Collector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(collector_new(vm, COLLECT_COUNTING)?),
    ),
    method(
        "summingInt",
        "(Ljava/util/function/ToIntFunction;)Ljava/util/stream/Collector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let collector = collector_new(vm, COLLECT_SUMMING_INT)?;
            collector_set(vm, collector, "f1", ref_arg(args, 0));
            object(collector)
        },
    ),
    method(
        "summingLong",
        "(Ljava/util/function/ToLongFunction;)Ljava/util/stream/Collector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let collector = collector_new(vm, COLLECT_SUMMING_LONG)?;
            collector_set(vm, collector, "f1", ref_arg(args, 0));
            object(collector)
        },
    ),
    method(
        "summingDouble",
        "(Ljava/util/function/ToDoubleFunction;)Ljava/util/stream/Collector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let collector = collector_new(vm, COLLECT_SUMMING_DOUBLE)?;
            collector_set(vm, collector, "f1", ref_arg(args, 0));
            object(collector)
        },
    ),
    method(
        "averagingInt",
        "(Ljava/util/function/ToIntFunction;)Ljava/util/stream/Collector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let collector = collector_new(vm, COLLECT_AVERAGING_INT)?;
            collector_set(vm, collector, "f1", ref_arg(args, 0));
            object(collector)
        },
    ),
    method(
        "averagingLong",
        "(Ljava/util/function/ToLongFunction;)Ljava/util/stream/Collector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let collector = collector_new(vm, COLLECT_AVERAGING_LONG)?;
            collector_set(vm, collector, "f1", ref_arg(args, 0));
            object(collector)
        },
    ),
    method(
        "averagingDouble",
        "(Ljava/util/function/ToDoubleFunction;)Ljava/util/stream/Collector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let collector = collector_new(vm, COLLECT_AVERAGING_DOUBLE)?;
            collector_set(vm, collector, "f1", ref_arg(args, 0));
            object(collector)
        },
    ),
    method(
        "toMap",
        "(Ljava/util/function/Function;Ljava/util/function/Function;)Ljava/util/stream/Collector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let collector = collector_new(vm, COLLECT_TO_MAP)?;
            collector_set(vm, collector, "f1", ref_arg(args, 0));
            collector_set(vm, collector, "f2", ref_arg(args, 1));
            object(collector)
        },
    ),
    method(
        "toMap",
        "(Ljava/util/function/Function;Ljava/util/function/Function;Ljava/util/function/BinaryOperator;)Ljava/util/stream/Collector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let collector = collector_new(vm, COLLECT_TO_MAP)?;
            collector_set(vm, collector, "f1", ref_arg(args, 0));
            collector_set(vm, collector, "f2", ref_arg(args, 1));
            collector_set(vm, collector, "f3", ref_arg(args, 2));
            object(collector)
        },
    ),
    method(
        "toMap",
        "(Ljava/util/function/Function;Ljava/util/function/Function;Ljava/util/function/BinaryOperator;Ljava/util/function/Supplier;)Ljava/util/stream/Collector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let collector = collector_new(vm, COLLECT_TO_MAP)?;
            collector_set(vm, collector, "f1", ref_arg(args, 0));
            collector_set(vm, collector, "f2", ref_arg(args, 1));
            collector_set(vm, collector, "f3", ref_arg(args, 2));
            collector_set(vm, collector, "f4", ref_arg(args, 3));
            object(collector)
        },
    ),
    method(
        "toUnmodifiableMap",
        "(Ljava/util/function/Function;Ljava/util/function/Function;)Ljava/util/stream/Collector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let collector = collector_new(vm, COLLECT_TO_UNMODIFIABLE_MAP)?;
            collector_set(vm, collector, "f1", ref_arg(args, 0));
            collector_set(vm, collector, "f2", ref_arg(args, 1));
            object(collector)
        },
    ),
    method(
        "mapping",
        "(Ljava/util/function/Function;Ljava/util/stream/Collector;)Ljava/util/stream/Collector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let collector = collector_new(vm, COLLECT_MAPPING)?;
            collector_set(vm, collector, "f1", ref_arg(args, 0));
            collector_set(vm, collector, "f3", ref_arg(args, 1));
            object(collector)
        },
    ),
    method(
        "reducing",
        "(Ljava/lang/Object;Ljava/util/function/BinaryOperator;)Ljava/util/stream/Collector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let collector = collector_new(vm, COLLECT_REDUCING)?;
            collector_set(vm, collector, "f4", ref_arg(args, 0));
            collector_set(vm, collector, "f2", ref_arg(args, 1));
            object(collector)
        },
    ),
    // Placeholder so the count is stable while keeping the widely used factories above.
    method(
        "toUnmodifiableMap",
        "(Ljava/util/function/Function;Ljava/util/function/Function;Ljava/util/function/BinaryOperator;)Ljava/util/stream/Collector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let collector = collector_new(vm, COLLECT_TO_UNMODIFIABLE_MAP)?;
            collector_set(vm, collector, "f1", ref_arg(args, 0));
            collector_set(vm, collector, "f2", ref_arg(args, 1));
            collector_set(vm, collector, "f3", ref_arg(args, 2));
            object(collector)
        },
    ),
];

pub(crate) const COLLECTORS: oxjvm_vm::NativeClass = class(
    "java/util/stream/Collectors",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &COLLECTORS_METHODS,
    None,
);

const OJVM_COLLECTOR_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "characteristics",
    "()Ljava/util/Set;",
    ACC_PUBLIC,
    |vm, _, _args| {
        // The factories above all finish with the identity function.
        object(crate::util::hash_set_from(vm, &[])?)
    },
)];

pub(crate) const OJVM_COLLECTOR: oxjvm_vm::NativeClass = class(
    "java/util/stream/OjvmCollector",
    Some("java/lang/Object"),
    &["java/util/stream/Collector"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[
        field("kind", "I", ACC_PUBLIC, None),
        field("f1", "Ljava/lang/Object;", ACC_PUBLIC, None),
        field("f2", "Ljava/lang/Object;", ACC_PUBLIC, None),
        field("f3", "Ljava/lang/Object;", ACC_PUBLIC, None),
        field("f4", "Ljava/lang/Object;", ACC_PUBLIC, None),
        field("s1", "Ljava/lang/Object;", ACC_PUBLIC, None),
        field("s2", "Ljava/lang/Object;", ACC_PUBLIC, None),
        field("s3", "Ljava/lang/Object;", ACC_PUBLIC, None),
    ],
    &OJVM_COLLECTOR_METHODS,
    None,
);

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
