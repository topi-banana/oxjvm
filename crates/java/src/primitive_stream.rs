//! `DoubleStream` and `LongStream`: eager primitive streams over value arrays.

use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::{boolean, class, field, long, method, object, receiver, ref_arg, void};

fn double_elements(vm: &Vm<'_>, this: ObjectRef) -> Vec<f64> {
    let class = vm.class_of(this);
    match vm.read_ref_field(this, class, "elements", "[D") {
        Some(array) => match vm.heap.get(array).map(|object| &object.data) {
            Some(oxjvm_vm::ObjectData::Array(oxjvm_vm::ArrayData::Double(values))) => {
                values.clone()
            }
            _ => Vec::new(),
        },
        None => Vec::new(),
    }
}

fn long_elements(vm: &Vm<'_>, this: ObjectRef) -> Vec<i64> {
    let class = vm.class_of(this);
    match vm.read_ref_field(this, class, "elements", "[J") {
        Some(array) => match vm.heap.get(array).map(|object| &object.data) {
            Some(oxjvm_vm::ObjectData::Array(oxjvm_vm::ArrayData::Long(values))) => values.clone(),
            _ => Vec::new(),
        },
        None => Vec::new(),
    }
}

pub(crate) fn double_stream_from(vm: &mut Vm<'_>, values: Vec<f64>) -> Result<ObjectRef, VmError> {
    new_double_stream(vm, values)
}

fn new_double_stream(vm: &mut Vm<'_>, values: Vec<f64>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/stream/OjvmDoubleStream")?;
    let instance = vm.new_instance(class)?;
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Double, values.len())?;
    for (index, value) in values.iter().enumerate() {
        vm.array_set(array, index as i32, Value::Double(*value), false)?;
    }
    let (declaring, field) = vm.find_field(class, "elements", "[D")?;
    vm.set_instance_ref(instance, declaring, field, array);
    Ok(instance)
}

pub(crate) fn long_stream_from(vm: &mut Vm<'_>, values: Vec<i64>) -> Result<ObjectRef, VmError> {
    new_long_stream(vm, values)
}

fn new_long_stream(vm: &mut Vm<'_>, values: Vec<i64>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/stream/OjvmLongStream")?;
    let instance = vm.new_instance(class)?;
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Long, values.len())?;
    for (index, value) in values.iter().enumerate() {
        vm.array_set(array, index as i32, Value::Long(*value), false)?;
    }
    let (declaring, field) = vm.find_field(class, "elements", "[J")?;
    vm.set_instance_ref(instance, declaring, field, array);
    Ok(instance)
}

fn call_double_unary(vm: &mut Vm<'_>, operator: ObjectRef, value: f64) -> Result<f64, VmError> {
    let (class, method) = vm.resolve_virtual_method(operator, "applyAsDouble", "(D)D")?;
    Ok(vm
        .invoke_method(
            class,
            method,
            vec![Value::Ref(operator), Value::Double(value)],
        )?
        .as_double())
}

fn call_double_predicate(
    vm: &mut Vm<'_>,
    predicate: ObjectRef,
    value: f64,
) -> Result<bool, VmError> {
    let (class, method) = vm.resolve_virtual_method(predicate, "test", "(D)Z")?;
    Ok(vm
        .invoke_method(
            class,
            method,
            vec![Value::Ref(predicate), Value::Double(value)],
        )?
        .as_int()
        != 0)
}

fn call_double_consumer(vm: &mut Vm<'_>, consumer: ObjectRef, value: f64) -> Result<(), VmError> {
    let (class, method) = vm.resolve_virtual_method(consumer, "accept", "(D)V")?;
    vm.invoke_method(
        class,
        method,
        vec![Value::Ref(consumer), Value::Double(value)],
    )?;
    Ok(())
}

fn call_double_binary(
    vm: &mut Vm<'_>,
    operator: ObjectRef,
    left: f64,
    right: f64,
) -> Result<f64, VmError> {
    let (class, method) = vm.resolve_virtual_method(operator, "applyAsDouble", "(DD)D")?;
    Ok(vm
        .invoke_method(
            class,
            method,
            vec![
                Value::Ref(operator),
                Value::Double(left),
                Value::Double(right),
            ],
        )?
        .as_double())
}

fn call_long_unary(vm: &mut Vm<'_>, operator: ObjectRef, value: i64) -> Result<i64, VmError> {
    let (class, method) = vm.resolve_virtual_method(operator, "applyAsLong", "(J)J")?;
    Ok(vm
        .invoke_method(
            class,
            method,
            vec![Value::Ref(operator), Value::Long(value)],
        )?
        .as_long())
}

fn call_long_predicate(vm: &mut Vm<'_>, predicate: ObjectRef, value: i64) -> Result<bool, VmError> {
    let (class, method) = vm.resolve_virtual_method(predicate, "test", "(J)Z")?;
    Ok(vm
        .invoke_method(
            class,
            method,
            vec![Value::Ref(predicate), Value::Long(value)],
        )?
        .as_int()
        != 0)
}

fn call_long_consumer(vm: &mut Vm<'_>, consumer: ObjectRef, value: i64) -> Result<(), VmError> {
    let (class, method) = vm.resolve_virtual_method(consumer, "accept", "(J)V")?;
    vm.invoke_method(
        class,
        method,
        vec![Value::Ref(consumer), Value::Long(value)],
    )?;
    Ok(())
}

fn call_long_binary(
    vm: &mut Vm<'_>,
    operator: ObjectRef,
    left: i64,
    right: i64,
) -> Result<i64, VmError> {
    let (class, method) = vm.resolve_virtual_method(operator, "applyAsLong", "(JJ)J")?;
    Ok(vm
        .invoke_method(
            class,
            method,
            vec![Value::Ref(operator), Value::Long(left), Value::Long(right)],
        )?
        .as_long())
}

fn optional_double(vm: &mut Vm<'_>, value: Option<f64>) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/util/OptionalDouble")?;
    let instance = vm.new_instance(class)?;
    match value {
        Some(value) => {
            crate::lang::write_int_field(vm, instance, "present", 1);
            let (declaring, field) = vm.find_field(class, "value", "D")?;
            vm.set_instance_long(instance, declaring, field, value.to_bits() as i64);
        }
        None => crate::lang::write_int_field(vm, instance, "present", 0),
    }
    object(instance)
}

fn optional_long(vm: &mut Vm<'_>, value: Option<i64>) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/util/OptionalLong")?;
    let instance = vm.new_instance(class)?;
    match value {
        Some(value) => {
            crate::lang::write_int_field(vm, instance, "present", 1);
            let (declaring, field) = vm.find_field(class, "value", "J")?;
            vm.set_instance_long(instance, declaring, field, value);
        }
        None => crate::lang::write_int_field(vm, instance, "present", 0),
    }
    object(instance)
}

const DOUBLE_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 36] = [
    method(
        "of",
        "([D)Ljava/util/stream/DoubleStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let array = ref_arg(args, 0);
            let length = vm.array_length(array)?;
            let mut values = Vec::with_capacity(length);
            for index in 0..length {
                values.push(vm.array_get(array, index as i32)?.as_double());
            }
            object(new_double_stream(vm, values)?)
        },
    ),
    method(
        "empty",
        "()Ljava/util/stream/DoubleStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_double_stream(vm, Vec::new())?),
    ),
    method(
        "iterate",
        "(DLjava/util/function/DoubleUnaryOperator;)Ljava/util/stream/DoubleStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let mut current = crate::double_arg(args, 0);
            let operator = ref_arg(args, 1);
            let mut values = Vec::new();
            for _ in 0..1024 {
                values.push(current);
                current = call_double_unary(vm, operator, current)?;
            }
            object(new_double_stream(vm, values)?)
        },
    ),
    method(
        "generate",
        "(Ljava/util/function/Supplier;)Ljava/util/stream/DoubleStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let supplier = ref_arg(args, 0);
            let (class, get) =
                vm.resolve_virtual_method(supplier, "get", "()Ljava/lang/Object;")?;
            let mut values = Vec::new();
            for _ in 0..1024 {
                let value = vm.invoke_method(class, get, vec![Value::Ref(supplier)])?;
                values.push(f64::from_bits(
                    crate::boxed::unbox_value(vm, value.as_ref(), "D").as_long() as u64,
                ));
            }
            object(new_double_stream(vm, values)?)
        },
    ),
    method(
        "concat",
        "(Ljava/util/stream/DoubleStream;Ljava/util/stream/DoubleStream;)Ljava/util/stream/DoubleStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let mut values = double_elements(vm, ref_arg(args, 0));
            values.extend(double_elements(vm, ref_arg(args, 1)));
            object(new_double_stream(vm, values)?)
        },
    ),
    method(
        "map",
        "(Ljava/util/function/DoubleUnaryOperator;)Ljava/util/stream/DoubleStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let operator = ref_arg(args, 1);
            let mut mapped = Vec::new();
            for value in double_elements(vm, receiver(args)) {
                mapped.push(call_double_unary(vm, operator, value)?);
            }
            object(new_double_stream(vm, mapped)?)
        },
    ),
    method(
        "filter",
        "(Ljava/util/function/DoublePredicate;)Ljava/util/stream/DoubleStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let predicate = ref_arg(args, 1);
            let mut kept = Vec::new();
            for value in double_elements(vm, receiver(args)) {
                if call_double_predicate(vm, predicate, value)? {
                    kept.push(value);
                }
            }
            object(new_double_stream(vm, kept)?)
        },
    ),
    method(
        "mapToObj",
        "(Ljava/util/function/DoubleFunction;)Ljava/util/stream/Stream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let function = ref_arg(args, 1);
            let mut values = Vec::new();
            for value in double_elements(vm, receiver(args)) {
                let (class, method) =
                    vm.resolve_virtual_method(function, "apply", "(D)Ljava/lang/Object;")?;
                values.push(
                    vm.invoke_method(
                        class,
                        method,
                        vec![Value::Ref(function), Value::Double(value)],
                    )?
                    .as_ref(),
                );
            }
            object(crate::stream::stream_from(vm, values)?)
        },
    ),
    method(
        "mapToInt",
        "(Ljava/util/function/DoubleToIntFunction;)Ljava/util/stream/IntStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let function = ref_arg(args, 1);
            let mut values = Vec::new();
            for value in double_elements(vm, receiver(args)) {
                let (class, method) = vm.resolve_virtual_method(function, "applyAsInt", "(D)I")?;
                values.push(
                    vm.invoke_method(
                        class,
                        method,
                        vec![Value::Ref(function), Value::Double(value)],
                    )?
                    .as_int(),
                );
            }
            crate::intstream::int_stream_of(vm, values)
        },
    ),
    method(
        "mapToLong",
        "(Ljava/util/function/DoubleToLongFunction;)Ljava/util/stream/LongStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let function = ref_arg(args, 1);
            let mut values = Vec::new();
            for value in double_elements(vm, receiver(args)) {
                let (class, method) = vm.resolve_virtual_method(function, "applyAsLong", "(D)J")?;
                values.push(
                    vm.invoke_method(
                        class,
                        method,
                        vec![Value::Ref(function), Value::Double(value)],
                    )?
                    .as_long(),
                );
            }
            object(new_long_stream(vm, values)?)
        },
    ),
    method(
        "forEach",
        "(Ljava/util/function/DoubleConsumer;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let consumer = ref_arg(args, 1);
            for value in double_elements(vm, receiver(args)) {
                call_double_consumer(vm, consumer, value)?;
            }
            void()
        },
    ),
    method(
        "forEachOrdered",
        "(Ljava/util/function/DoubleConsumer;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let consumer = ref_arg(args, 1);
            for value in double_elements(vm, receiver(args)) {
                call_double_consumer(vm, consumer, value)?;
            }
            void()
        },
    ),
    method("count", "()J", ACC_PUBLIC, |vm, _, args| {
        long(double_elements(vm, receiver(args)).len() as i64)
    }),
    method("toArray", "()[D", ACC_PUBLIC, |vm, _, args| {
        let values = double_elements(vm, receiver(args));
        let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Double, values.len())?;
        for (index, value) in values.iter().enumerate() {
            vm.array_set(array, index as i32, Value::Double(*value), false)?;
        }
        object(array)
    }),
    method("sum", "()D", ACC_PUBLIC, |vm, _, args| {
        Ok(Value::Double(
            double_elements(vm, receiver(args)).iter().sum(),
        ))
    }),
    method(
        "average",
        "()Ljava/util/OptionalDouble;",
        ACC_PUBLIC,
        |vm, _, args| {
            let values = double_elements(vm, receiver(args));
            optional_double(
                vm,
                if values.is_empty() {
                    None
                } else {
                    Some(values.iter().sum::<f64>() / values.len() as f64)
                },
            )
        },
    ),
    method(
        "min",
        "()Ljava/util/OptionalDouble;",
        ACC_PUBLIC,
        |vm, _, args| {
            let values = double_elements(vm, receiver(args));
            optional_double(
                vm,
                values
                    .iter()
                    .copied()
                    .reduce(|a, b| if b < a { b } else { a }),
            )
        },
    ),
    method(
        "max",
        "()Ljava/util/OptionalDouble;",
        ACC_PUBLIC,
        |vm, _, args| {
            let values = double_elements(vm, receiver(args));
            optional_double(
                vm,
                values
                    .iter()
                    .copied()
                    .reduce(|a, b| if b > a { b } else { a }),
            )
        },
    ),
    method(
        "boxed",
        "()Ljava/util/stream/Stream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let mut boxed = Vec::new();
            for value in double_elements(vm, receiver(args)) {
                boxed.push(
                    crate::boxed::box_value(vm, Value::Long(value.to_bits() as i64), "D")?.as_ref(),
                );
            }
            object(crate::stream::stream_from(vm, boxed)?)
        },
    ),
    method(
        "sorted",
        "()Ljava/util/stream/DoubleStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let mut values = double_elements(vm, receiver(args));
            values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
            object(new_double_stream(vm, values)?)
        },
    ),
    method(
        "distinct",
        "()Ljava/util/stream/DoubleStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let mut values = Vec::new();
            for value in double_elements(vm, receiver(args)) {
                if !values.iter().any(|existing| existing == &value) {
                    values.push(value);
                }
            }
            object(new_double_stream(vm, values)?)
        },
    ),
    method(
        "limit",
        "(J)Ljava/util/stream/DoubleStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let limit = crate::long_arg(args, 1).max(0) as usize;
            let values: Vec<f64> = double_elements(vm, receiver(args))
                .into_iter()
                .take(limit)
                .collect();
            object(new_double_stream(vm, values)?)
        },
    ),
    method(
        "skip",
        "(J)Ljava/util/stream/DoubleStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let skip = crate::long_arg(args, 1).max(0) as usize;
            let values: Vec<f64> = double_elements(vm, receiver(args))
                .into_iter()
                .skip(skip)
                .collect();
            object(new_double_stream(vm, values)?)
        },
    ),
    method(
        "anyMatch",
        "(Ljava/util/function/DoublePredicate;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let predicate = ref_arg(args, 1);
            for value in double_elements(vm, receiver(args)) {
                if call_double_predicate(vm, predicate, value)? {
                    return boolean(true);
                }
            }
            boolean(false)
        },
    ),
    method(
        "allMatch",
        "(Ljava/util/function/DoublePredicate;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let predicate = ref_arg(args, 1);
            for value in double_elements(vm, receiver(args)) {
                if !call_double_predicate(vm, predicate, value)? {
                    return boolean(false);
                }
            }
            boolean(true)
        },
    ),
    method(
        "noneMatch",
        "(Ljava/util/function/DoublePredicate;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let predicate = ref_arg(args, 1);
            for value in double_elements(vm, receiver(args)) {
                if call_double_predicate(vm, predicate, value)? {
                    return boolean(false);
                }
            }
            boolean(true)
        },
    ),
    method(
        "findFirst",
        "()Ljava/util/OptionalDouble;",
        ACC_PUBLIC,
        |vm, _, args| optional_double(vm, double_elements(vm, receiver(args)).first().copied()),
    ),
    method(
        "findAny",
        "()Ljava/util/OptionalDouble;",
        ACC_PUBLIC,
        |vm, _, args| optional_double(vm, double_elements(vm, receiver(args)).first().copied()),
    ),
    method(
        "reduce",
        "(DLjava/util/function/DoubleBinaryOperator;)D",
        ACC_PUBLIC,
        |vm, _, args| {
            let mut accumulator = crate::double_arg(args, 1);
            let operator = ref_arg(args, 2);
            for value in double_elements(vm, receiver(args)) {
                accumulator = call_double_binary(vm, operator, accumulator, value)?;
            }
            Ok(Value::Double(accumulator))
        },
    ),
    method(
        "reduce",
        "(Ljava/util/function/DoubleBinaryOperator;)Ljava/util/OptionalDouble;",
        ACC_PUBLIC,
        |vm, _, args| {
            let operator = ref_arg(args, 1);
            let mut iter = double_elements(vm, receiver(args)).into_iter();
            let Some(mut accumulator) = iter.next() else {
                return optional_double(vm, None);
            };
            for value in iter {
                accumulator = call_double_binary(vm, operator, accumulator, value)?;
            }
            optional_double(vm, Some(accumulator))
        },
    ),
    method("isParallel", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method(
        "parallel",
        "()Ljava/util/stream/DoubleStream;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method(
        "sequential",
        "()Ljava/util/stream/DoubleStream;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method(
        "unordered",
        "()Ljava/util/stream/DoubleStream;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method(
        "onClose",
        "(Ljava/lang/Runnable;)Ljava/util/stream/BaseStream;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
];

pub(crate) const DOUBLE_STREAM: oxjvm_vm::NativeClass = class(
    "java/util/stream/DoubleStream",
    Some("java/lang/Object"),
    &["java/util/stream/BaseStream"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &DOUBLE_STREAM_METHODS,
    None,
);

pub(crate) const OJVM_DOUBLE_STREAM: oxjvm_vm::NativeClass = class(
    "java/util/stream/OjvmDoubleStream",
    Some("java/lang/Object"),
    &[
        "java/util/stream/DoubleStream",
        "java/util/stream/BaseStream",
    ],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field("elements", "[D", ACC_PRIVATE, None)],
    &DOUBLE_STREAM_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// LongStream
// -------------------------------------------------------------------------------------------

const LONG_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 35] = [
    method(
        "of",
        "([J)Ljava/util/stream/LongStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let array = ref_arg(args, 0);
            let length = vm.array_length(array)?;
            let mut values = Vec::with_capacity(length);
            for index in 0..length {
                values.push(vm.array_get(array, index as i32)?.as_long());
            }
            object(new_long_stream(vm, values)?)
        },
    ),
    method(
        "range",
        "(JJ)Ljava/util/stream/LongStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let start = crate::long_arg(args, 0);
            let end = crate::long_arg(args, 1);
            let values: Vec<i64> = if start < end {
                (start..end).collect()
            } else {
                Vec::new()
            };
            object(new_long_stream(vm, values)?)
        },
    ),
    method(
        "rangeClosed",
        "(JJ)Ljava/util/stream/LongStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let start = crate::long_arg(args, 0);
            let end = crate::long_arg(args, 1);
            let values: Vec<i64> = if start <= end {
                (start..=end).collect()
            } else {
                Vec::new()
            };
            object(new_long_stream(vm, values)?)
        },
    ),
    method(
        "empty",
        "()Ljava/util/stream/LongStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_long_stream(vm, Vec::new())?),
    ),
    method(
        "iterate",
        "(JLjava/util/function/LongUnaryOperator;)Ljava/util/stream/LongStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let mut current = crate::long_arg(args, 0);
            let operator = ref_arg(args, 1);
            let mut values = Vec::new();
            for _ in 0..1024 {
                values.push(current);
                current = call_long_unary(vm, operator, current)?;
            }
            object(new_long_stream(vm, values)?)
        },
    ),
    method(
        "generate",
        "(Ljava/util/function/Supplier;)Ljava/util/stream/LongStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let supplier = ref_arg(args, 0);
            let (class, get) =
                vm.resolve_virtual_method(supplier, "get", "()Ljava/lang/Object;")?;
            let mut values = Vec::new();
            for _ in 0..1024 {
                let value = vm.invoke_method(class, get, vec![Value::Ref(supplier)])?;
                values.push(crate::boxed::unbox_value(vm, value.as_ref(), "J").as_long());
            }
            object(new_long_stream(vm, values)?)
        },
    ),
    method(
        "concat",
        "(Ljava/util/stream/LongStream;Ljava/util/stream/LongStream;)Ljava/util/stream/LongStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let mut values = long_elements(vm, ref_arg(args, 0));
            values.extend(long_elements(vm, ref_arg(args, 1)));
            object(new_long_stream(vm, values)?)
        },
    ),
    method(
        "map",
        "(Ljava/util/function/LongUnaryOperator;)Ljava/util/stream/LongStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let operator = ref_arg(args, 1);
            let mut mapped = Vec::new();
            for value in long_elements(vm, receiver(args)) {
                mapped.push(call_long_unary(vm, operator, value)?);
            }
            object(new_long_stream(vm, mapped)?)
        },
    ),
    method(
        "filter",
        "(Ljava/util/function/LongPredicate;)Ljava/util/stream/LongStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let predicate = ref_arg(args, 1);
            let mut kept = Vec::new();
            for value in long_elements(vm, receiver(args)) {
                if call_long_predicate(vm, predicate, value)? {
                    kept.push(value);
                }
            }
            object(new_long_stream(vm, kept)?)
        },
    ),
    method(
        "mapToObj",
        "(Ljava/util/function/LongFunction;)Ljava/util/stream/Stream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let function = ref_arg(args, 1);
            let mut values = Vec::new();
            for value in long_elements(vm, receiver(args)) {
                let (class, method) =
                    vm.resolve_virtual_method(function, "apply", "(J)Ljava/lang/Object;")?;
                values.push(
                    vm.invoke_method(
                        class,
                        method,
                        vec![Value::Ref(function), Value::Long(value)],
                    )?
                    .as_ref(),
                );
            }
            object(crate::stream::stream_from(vm, values)?)
        },
    ),
    method(
        "mapToInt",
        "(Ljava/util/function/LongToIntFunction;)Ljava/util/stream/IntStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let function = ref_arg(args, 1);
            let mut values = Vec::new();
            for value in long_elements(vm, receiver(args)) {
                let (class, method) = vm.resolve_virtual_method(function, "applyAsInt", "(J)I")?;
                values.push(
                    vm.invoke_method(
                        class,
                        method,
                        vec![Value::Ref(function), Value::Long(value)],
                    )?
                    .as_int(),
                );
            }
            crate::intstream::int_stream_of(vm, values)
        },
    ),
    method(
        "forEach",
        "(Ljava/util/function/LongConsumer;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let consumer = ref_arg(args, 1);
            for value in long_elements(vm, receiver(args)) {
                call_long_consumer(vm, consumer, value)?;
            }
            void()
        },
    ),
    method("count", "()J", ACC_PUBLIC, |vm, _, args| {
        long(long_elements(vm, receiver(args)).len() as i64)
    }),
    method("toArray", "()[J", ACC_PUBLIC, |vm, _, args| {
        let values = long_elements(vm, receiver(args));
        let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Long, values.len())?;
        for (index, value) in values.iter().enumerate() {
            vm.array_set(array, index as i32, Value::Long(*value), false)?;
        }
        object(array)
    }),
    method("sum", "()J", ACC_PUBLIC, |vm, _, args| {
        long(long_elements(vm, receiver(args)).iter().sum())
    }),
    method(
        "average",
        "()Ljava/util/OptionalDouble;",
        ACC_PUBLIC,
        |vm, _, args| {
            let values = long_elements(vm, receiver(args));
            optional_double(
                vm,
                if values.is_empty() {
                    None
                } else {
                    Some(values.iter().sum::<i64>() as f64 / values.len() as f64)
                },
            )
        },
    ),
    method(
        "min",
        "()Ljava/util/OptionalLong;",
        ACC_PUBLIC,
        |vm, _, args| optional_long(vm, long_elements(vm, receiver(args)).into_iter().min()),
    ),
    method(
        "max",
        "()Ljava/util/OptionalLong;",
        ACC_PUBLIC,
        |vm, _, args| optional_long(vm, long_elements(vm, receiver(args)).into_iter().max()),
    ),
    method(
        "boxed",
        "()Ljava/util/stream/Stream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let mut boxed = Vec::new();
            for value in long_elements(vm, receiver(args)) {
                boxed.push(crate::boxed::box_value(vm, Value::Long(value), "J")?.as_ref());
            }
            object(crate::stream::stream_from(vm, boxed)?)
        },
    ),
    method(
        "sorted",
        "()Ljava/util/stream/LongStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let mut values = long_elements(vm, receiver(args));
            values.sort_unstable();
            object(new_long_stream(vm, values)?)
        },
    ),
    method(
        "distinct",
        "()Ljava/util/stream/LongStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let mut values = Vec::new();
            for value in long_elements(vm, receiver(args)) {
                if !values.contains(&value) {
                    values.push(value);
                }
            }
            object(new_long_stream(vm, values)?)
        },
    ),
    method(
        "limit",
        "(J)Ljava/util/stream/LongStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let limit = crate::long_arg(args, 1).max(0) as usize;
            let values: Vec<i64> = long_elements(vm, receiver(args))
                .into_iter()
                .take(limit)
                .collect();
            object(new_long_stream(vm, values)?)
        },
    ),
    method(
        "skip",
        "(J)Ljava/util/stream/LongStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let skip = crate::long_arg(args, 1).max(0) as usize;
            let values: Vec<i64> = long_elements(vm, receiver(args))
                .into_iter()
                .skip(skip)
                .collect();
            object(new_long_stream(vm, values)?)
        },
    ),
    method(
        "anyMatch",
        "(Ljava/util/function/LongPredicate;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let predicate = ref_arg(args, 1);
            for value in long_elements(vm, receiver(args)) {
                if call_long_predicate(vm, predicate, value)? {
                    return boolean(true);
                }
            }
            boolean(false)
        },
    ),
    method(
        "allMatch",
        "(Ljava/util/function/LongPredicate;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let predicate = ref_arg(args, 1);
            for value in long_elements(vm, receiver(args)) {
                if !call_long_predicate(vm, predicate, value)? {
                    return boolean(false);
                }
            }
            boolean(true)
        },
    ),
    method(
        "noneMatch",
        "(Ljava/util/function/LongPredicate;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let predicate = ref_arg(args, 1);
            for value in long_elements(vm, receiver(args)) {
                if call_long_predicate(vm, predicate, value)? {
                    return boolean(false);
                }
            }
            boolean(true)
        },
    ),
    method(
        "findFirst",
        "()Ljava/util/OptionalLong;",
        ACC_PUBLIC,
        |vm, _, args| optional_long(vm, long_elements(vm, receiver(args)).first().copied()),
    ),
    method(
        "findAny",
        "()Ljava/util/OptionalLong;",
        ACC_PUBLIC,
        |vm, _, args| optional_long(vm, long_elements(vm, receiver(args)).first().copied()),
    ),
    method(
        "reduce",
        "(JLjava/util/function/LongBinaryOperator;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let mut accumulator = crate::long_arg(args, 1);
            let operator = ref_arg(args, 2);
            for value in long_elements(vm, receiver(args)) {
                accumulator = call_long_binary(vm, operator, accumulator, value)?;
            }
            long(accumulator)
        },
    ),
    method("isParallel", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method(
        "parallel",
        "()Ljava/util/stream/LongStream;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method(
        "sequential",
        "()Ljava/util/stream/LongStream;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method(
        "unordered",
        "()Ljava/util/stream/LongStream;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method(
        "onClose",
        "(Ljava/lang/Runnable;)Ljava/util/stream/BaseStream;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
];

pub(crate) const LONG_STREAM: oxjvm_vm::NativeClass = class(
    "java/util/stream/LongStream",
    Some("java/lang/Object"),
    &["java/util/stream/BaseStream"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &LONG_STREAM_METHODS,
    None,
);

pub(crate) const OJVM_LONG_STREAM: oxjvm_vm::NativeClass = class(
    "java/util/stream/OjvmLongStream",
    Some("java/lang/Object"),
    &["java/util/stream/LongStream", "java/util/stream/BaseStream"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field("elements", "[J", ACC_PRIVATE, None)],
    &LONG_STREAM_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// OptionalDouble / OptionalLong
// -------------------------------------------------------------------------------------------

const OPTIONAL_DOUBLE_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method(
        "empty",
        "()Ljava/util/OptionalDouble;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| optional_double(vm, None),
    ),
    method(
        "of",
        "(D)Ljava/util/OptionalDouble;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| optional_double(vm, Some(crate::double_arg(args, 0))),
    ),
    method("isPresent", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "present") != 0)
    }),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "present") == 0)
    }),
    method("getAsDouble", "()D", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if crate::lang::read_int_field(vm, this, "present") == 0 {
            return Err(vm.throw_new("java/util/NoSuchElementException", None));
        }
        let value = vm.read_instance_long_named(this, vm.class_of(this), "value", "D");
        Ok(Value::Double(f64::from_bits(value.unwrap_or(0) as u64)))
    }),
    method("orElse", "(D)D", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if crate::lang::read_int_field(vm, this, "present") == 0 {
            return Ok(Value::Double(crate::double_arg(args, 1)));
        }
        let value = vm.read_instance_long_named(this, vm.class_of(this), "value", "D");
        Ok(Value::Double(f64::from_bits(value.unwrap_or(0) as u64)))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = if crate::lang::read_int_field(vm, this, "present") == 0 {
                alloc::string::String::from("OptionalDouble.empty")
            } else {
                let value = vm
                    .read_instance_long_named(this, vm.class_of(this), "value", "D")
                    .unwrap_or(0);
                alloc::format!("OptionalDouble[{}]", f64::from_bits(value as u64))
            };
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "stream",
        "()Ljava/util/stream/DoubleStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let values = if crate::lang::read_int_field(vm, this, "present") == 0 {
                Vec::new()
            } else {
                let value = vm
                    .read_instance_long_named(this, vm.class_of(this), "value", "D")
                    .unwrap_or(0);
                vec![f64::from_bits(value as u64)]
            };
            object(new_double_stream(vm, values)?)
        },
    ),
];

pub(crate) const OPTIONAL_DOUBLE: oxjvm_vm::NativeClass = class(
    "java/util/OptionalDouble",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("present", "I", ACC_PRIVATE, None),
        field("value", "D", ACC_PRIVATE, None),
    ],
    &OPTIONAL_DOUBLE_METHODS,
    None,
);

const OPTIONAL_LONG_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "empty",
        "()Ljava/util/OptionalLong;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| optional_long(vm, None),
    ),
    method(
        "of",
        "(J)Ljava/util/OptionalLong;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| optional_long(vm, Some(crate::long_arg(args, 0))),
    ),
    method("isPresent", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "present") != 0)
    }),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "present") == 0)
    }),
    method("getAsLong", "()J", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if crate::lang::read_int_field(vm, this, "present") == 0 {
            return Err(vm.throw_new("java/util/NoSuchElementException", None));
        }
        long(
            vm.read_instance_long_named(this, vm.class_of(this), "value", "J")
                .unwrap_or(0),
        )
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = if crate::lang::read_int_field(vm, this, "present") == 0 {
                alloc::string::String::from("OptionalLong.empty")
            } else {
                let value = vm
                    .read_instance_long_named(this, vm.class_of(this), "value", "J")
                    .unwrap_or(0);
                alloc::format!("OptionalLong[{value}]")
            };
            object(vm.make_string(&text)?)
        },
    ),
];

pub(crate) const OPTIONAL_LONG: oxjvm_vm::NativeClass = class(
    "java/util/OptionalLong",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("present", "I", ACC_PRIVATE, None),
        field("value", "J", ACC_PRIVATE, None),
    ],
    &OPTIONAL_LONG_METHODS,
    None,
);
