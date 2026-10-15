//! `java.util.stream.IntStream`: a compact, eager int stream.
//!
//! `CharSequence.chars()`/`codePoints()` return one of these, backed by an
//! `int[]` snapshot.

use alloc::vec;
use alloc::vec::Vec;
use oxjvm_classfile::flags::*;

use crate::lang::abstract_method_pub;
use crate::{boolean, class, field, int, long, method, object, receiver, ref_arg, void};
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

const INT_STREAM_CLASS: &str = "java/util/stream/OjvmIntStream";

fn int_elements(vm: &Vm<'_>, this: ObjectRef) -> Vec<i32> {
    match vm.read_ref_field(this, vm.class_of(this), "elements", "[I") {
        Some(array) => match vm.heap.get(array).map(|object| &object.data) {
            Some(oxjvm_vm::ObjectData::Array(oxjvm_vm::ArrayData::Int(elements))) => {
                elements.clone()
            }
            _ => Vec::new(),
        },
        None => Vec::new(),
    }
}

pub(crate) fn int_stream_from(vm: &mut Vm<'_>, values: Vec<i32>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class(INT_STREAM_CLASS)?;
    let instance = vm.new_instance(class)?;
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Int, values.len())?;
    for (index, value) in values.iter().enumerate() {
        vm.array_set(array, index as i32, Value::Int(*value), false)?;
    }
    let (declaring, field) = vm.find_field(class, "elements", "[I")?;
    vm.set_instance_ref(instance, declaring, field, array);
    Ok(instance)
}

/// `CharSequence.chars()` / `codePoints()` helper.
pub(crate) fn int_stream_of(vm: &mut Vm<'_>, values: Vec<i32>) -> Result<Value, VmError> {
    object(int_stream_from(vm, values)?)
}

fn call_int_unary(vm: &mut Vm<'_>, operator: ObjectRef, value: i32) -> Result<i32, VmError> {
    let (class, method) = vm.resolve_virtual_method(operator, "applyAsInt", "(I)I")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(operator), Value::Int(value)])?
        .as_int())
}

fn call_int_predicate(vm: &mut Vm<'_>, predicate: ObjectRef, value: i32) -> Result<bool, VmError> {
    let (class, method) = vm.resolve_virtual_method(predicate, "test", "(I)Z")?;
    Ok(vm
        .invoke_method(
            class,
            method,
            vec![Value::Ref(predicate), Value::Int(value)],
        )?
        .as_int()
        != 0)
}

fn call_int_consumer(vm: &mut Vm<'_>, consumer: ObjectRef, value: i32) -> Result<(), VmError> {
    let (class, method) = vm.resolve_virtual_method(consumer, "accept", "(I)V")?;
    vm.invoke_method(class, method, vec![Value::Ref(consumer), Value::Int(value)])?;
    Ok(())
}

const INT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 26] = [
    method(
        "of",
        "([I)Ljava/util/stream/IntStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let array = ref_arg(args, 0);
            let mut values = Vec::new();
            let length = vm.array_length(array)?;
            for index in 0..length {
                values.push(vm.array_get(array, index as i32)?.as_int());
            }
            int_stream_of(vm, values)
        },
    ),
    method(
        "range",
        "(II)Ljava/util/stream/IntStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let start = crate::int_arg(args, 0);
            let end = crate::int_arg(args, 1);
            let values: Vec<i32> = if start < end {
                (start..end).collect()
            } else {
                Vec::new()
            };
            int_stream_of(vm, values)
        },
    ),
    method(
        "rangeClosed",
        "(II)Ljava/util/stream/IntStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let start = crate::int_arg(args, 0);
            let end = crate::int_arg(args, 1);
            let values: Vec<i32> = if start <= end {
                (start..=end).collect()
            } else {
                Vec::new()
            };
            int_stream_of(vm, values)
        },
    ),
    method(
        "empty",
        "()Ljava/util/stream/IntStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| int_stream_of(vm, Vec::new()),
    ),
    method(
        "map",
        "(Ljava/util/function/IntUnaryOperator;)Ljava/util/stream/IntStream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "filter",
        "(Ljava/util/function/IntPredicate;)Ljava/util/stream/IntStream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "forEach",
        "(Ljava/util/function/IntConsumer;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "forEachOrdered",
        "(Ljava/util/function/IntConsumer;)V",
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
        "toArray",
        "()[I",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method("sum", "()I", ACC_PUBLIC | ACC_ABSTRACT, abstract_method_pub),
    method(
        "min",
        "()Ljava/util/OptionalInt;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "max",
        "()Ljava/util/OptionalInt;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "boxed",
        "()Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "mapToObj",
        "(Ljava/util/function/IntFunction;)Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "sorted",
        "()Ljava/util/stream/IntStream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "distinct",
        "()Ljava/util/stream/IntStream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "limit",
        "(J)Ljava/util/stream/IntStream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "skip",
        "(J)Ljava/util/stream/IntStream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "anyMatch",
        "(Ljava/util/function/IntPredicate;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "allMatch",
        "(Ljava/util/function/IntPredicate;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "noneMatch",
        "(Ljava/util/function/IntPredicate;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "parallel",
        "()Ljava/util/stream/IntStream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "sequential",
        "()Ljava/util/stream/IntStream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "unordered",
        "()Ljava/util/stream/IntStream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "iterator",
        "()Ljava/util/PrimitiveIterator$OfInt;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
];

pub(crate) const INT_STREAM: oxjvm_vm::NativeClass = class(
    "java/util/stream/IntStream",
    Some("java/lang/Object"),
    &["java/util/stream/BaseStream"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &INT_STREAM_METHODS,
    None,
);

const OJVM_INT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 20] = [
    method(
        "map",
        "(Ljava/util/function/IntUnaryOperator;)Ljava/util/stream/IntStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let operator = ref_arg(args, 1);
            let mut mapped = Vec::new();
            for value in int_elements(vm, receiver(args)) {
                mapped.push(call_int_unary(vm, operator, value)?);
            }
            object(int_stream_from(vm, mapped)?)
        },
    ),
    method(
        "filter",
        "(Ljava/util/function/IntPredicate;)Ljava/util/stream/IntStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let predicate = ref_arg(args, 1);
            let mut kept = Vec::new();
            for value in int_elements(vm, receiver(args)) {
                if call_int_predicate(vm, predicate, value)? {
                    kept.push(value);
                }
            }
            object(int_stream_from(vm, kept)?)
        },
    ),
    method(
        "forEach",
        "(Ljava/util/function/IntConsumer;)V",
        ACC_PUBLIC,
        |vm, _, args| int_stream_for_each(vm, args),
    ),
    method(
        "forEachOrdered",
        "(Ljava/util/function/IntConsumer;)V",
        ACC_PUBLIC,
        |vm, _, args| int_stream_for_each(vm, args),
    ),
    method("count", "()J", ACC_PUBLIC, |vm, _, args| {
        long(int_elements(vm, receiver(args)).len() as i64)
    }),
    method("toArray", "()[I", ACC_PUBLIC, |vm, _, args| {
        let values = int_elements(vm, receiver(args));
        let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Int, values.len())?;
        for (index, value) in values.iter().enumerate() {
            vm.array_set(array, index as i32, Value::Int(*value), false)?;
        }
        object(array)
    }),
    method("sum", "()I", ACC_PUBLIC, |vm, _, args| {
        int(int_elements(vm, receiver(args)).iter().copied().sum())
    }),
    method(
        "min",
        "()Ljava/util/OptionalInt;",
        ACC_PUBLIC,
        |vm, _, _| Err(crate::unsupported(vm, "IntStream.min")),
    ),
    method(
        "max",
        "()Ljava/util/OptionalInt;",
        ACC_PUBLIC,
        |vm, _, _| Err(crate::unsupported(vm, "IntStream.max")),
    ),
    method(
        "boxed",
        "()Ljava/util/stream/Stream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let mut elements = Vec::new();
            for value in int_elements(vm, receiver(args)) {
                elements.push(crate::boxed::boxed_int(vm, value)?.as_ref());
            }
            object(crate::stream::stream_from(vm, elements)?)
        },
    ),
    method(
        "mapToObj",
        "(Ljava/util/function/IntFunction;)Ljava/util/stream/Stream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let function = ref_arg(args, 1);
            let (class, method) =
                vm.resolve_virtual_method(function, "apply", "(I)Ljava/lang/Object;")?;
            let mut elements = Vec::new();
            for value in int_elements(vm, receiver(args)) {
                let mapped = vm
                    .invoke_method(class, method, vec![Value::Ref(function), Value::Int(value)])?
                    .as_ref();
                elements.push(mapped);
            }
            object(crate::stream::stream_from(vm, elements)?)
        },
    ),
    method(
        "sorted",
        "()Ljava/util/stream/IntStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let mut values = int_elements(vm, receiver(args));
            values.sort_unstable();
            object(int_stream_from(vm, values)?)
        },
    ),
    method(
        "distinct",
        "()Ljava/util/stream/IntStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let mut unique = Vec::new();
            for value in int_elements(vm, receiver(args)) {
                if !unique.contains(&value) {
                    unique.push(value);
                }
            }
            object(int_stream_from(vm, unique)?)
        },
    ),
    method(
        "limit",
        "(J)Ljava/util/stream/IntStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let values = int_elements(vm, receiver(args));
            let limit = crate::long_arg(args, 1).max(0) as usize;
            object(int_stream_from(
                vm,
                values[..limit.min(values.len())].to_vec(),
            )?)
        },
    ),
    method(
        "skip",
        "(J)Ljava/util/stream/IntStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let values = int_elements(vm, receiver(args));
            let skip = crate::long_arg(args, 1).max(0) as usize;
            object(int_stream_from(
                vm,
                values[skip.min(values.len())..].to_vec(),
            )?)
        },
    ),
    method(
        "anyMatch",
        "(Ljava/util/function/IntPredicate;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let predicate = ref_arg(args, 1);
            for value in int_elements(vm, receiver(args)) {
                if call_int_predicate(vm, predicate, value)? {
                    return boolean(true);
                }
            }
            boolean(false)
        },
    ),
    method(
        "allMatch",
        "(Ljava/util/function/IntPredicate;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let predicate = ref_arg(args, 1);
            for value in int_elements(vm, receiver(args)) {
                if !call_int_predicate(vm, predicate, value)? {
                    return boolean(false);
                }
            }
            boolean(true)
        },
    ),
    method(
        "noneMatch",
        "(Ljava/util/function/IntPredicate;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let predicate = ref_arg(args, 1);
            for value in int_elements(vm, receiver(args)) {
                if call_int_predicate(vm, predicate, value)? {
                    return boolean(false);
                }
            }
            boolean(true)
        },
    ),
    method(
        "parallel",
        "()Ljava/util/stream/IntStream;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method(
        "sequential",
        "()Ljava/util/stream/IntStream;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
];

fn int_stream_for_each(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let consumer = ref_arg(args, 1);
    for value in int_elements(vm, receiver(args)) {
        call_int_consumer(vm, consumer, value)?;
    }
    void()
}

pub(crate) const OJVM_INT_STREAM: oxjvm_vm::NativeClass = class(
    "java/util/stream/OjvmIntStream",
    Some("java/lang/Object"),
    &["java/util/stream/IntStream", "java/util/stream/BaseStream"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field("elements", "[I", ACC_PRIVATE, None)],
    &OJVM_INT_STREAM_METHODS,
    None,
);
