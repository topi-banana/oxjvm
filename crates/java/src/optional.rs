//! `java.util.Optional`.

use alloc::vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::{boolean, class, field, method, object, receiver, ref_arg, void};

const OPTIONAL_METHODS: [oxjvm_vm::NativeMethodDef; 15] = [
    method(
        "<init>",
        "(Ljava/lang/Object;)V",
        ACC_PRIVATE,
        |vm, _, args| optional_init(vm, args, ref_arg(args, 1)),
    ),
    method(
        "empty",
        "()Ljava/util/Optional;",
        ACC_PUBLIC | ACC_STATIC,
        optional_empty,
    ),
    method(
        "of",
        "(Ljava/lang/Object;)Ljava/util/Optional;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let value = ref_arg(args, 0);
            if value.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            optional_new(vm, value)
        },
    ),
    method(
        "ofNullable",
        "(Ljava/lang/Object;)Ljava/util/Optional;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| optional_new(vm, ref_arg(args, 0)),
    ),
    method("get", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        let value = optional_value(vm, receiver(args));
        if value.is_null() {
            return Err(vm.throw_new("java/util/NoSuchElementException", Some("No value present")));
        }
        object(value)
    }),
    method("isPresent", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(!optional_value(vm, receiver(args)).is_null())
    }),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(optional_value(vm, receiver(args)).is_null())
    }),
    method(
        "orElse",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = optional_value(vm, receiver(args));
            if value.is_null() {
                object(ref_arg(args, 1))
            } else {
                object(value)
            }
        },
    ),
    method(
        "orElseGet",
        "(Ljava/util/function/Supplier;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = optional_value(vm, receiver(args));
            if !value.is_null() {
                return object(value);
            }
            let supplier = ref_arg(args, 1);
            let (class, method) =
                vm.resolve_virtual_method(supplier, "get", "()Ljava/lang/Object;")?;
            vm.invoke_method(class, method, vec![Value::Ref(supplier)])
        },
    ),
    method(
        "orElseThrow",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = optional_value(vm, receiver(args));
            if value.is_null() {
                Err(vm.throw_new("java/util/NoSuchElementException", Some("No value present")))
            } else {
                object(value)
            }
        },
    ),
    method(
        "orElseThrow",
        "(Ljava/util/function/Supplier;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = optional_value(vm, receiver(args));
            if !value.is_null() {
                return object(value);
            }
            let supplier = ref_arg(args, 1);
            if supplier.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let (class, method) =
                vm.resolve_virtual_method(supplier, "get", "()Ljava/lang/Object;")?;
            let thrown = vm
                .invoke_method(class, method, vec![Value::Ref(supplier)])?
                .as_ref();
            Err(VmError::Thrown(thrown))
        },
    ),
    method(
        "ifPresent",
        "(Ljava/util/function/Consumer;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = optional_value(vm, receiver(args));
            if value.is_null() {
                return void();
            }
            let consumer = ref_arg(args, 1);
            if consumer.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let (class, method) =
                vm.resolve_virtual_method(consumer, "accept", "(Ljava/lang/Object;)V")?;
            vm.invoke_method(class, method, vec![Value::Ref(consumer), Value::Ref(value)])?;
            void()
        },
    ),
    method(
        "map",
        "(Ljava/util/function/Function;)Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = optional_value(vm, receiver(args));
            if value.is_null() {
                return optional_new(vm, ObjectRef::NULL);
            }
            let function = ref_arg(args, 1);
            let (class, method) = vm.resolve_virtual_method(
                function,
                "apply",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
            )?;
            let mapped =
                vm.invoke_method(class, method, vec![Value::Ref(function), Value::Ref(value)])?;
            optional_new(vm, mapped.as_ref())
        },
    ),
    method(
        "filter",
        "(Ljava/util/function/Predicate;)Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let value = optional_value(vm, this);
            if value.is_null() {
                return object(this);
            }
            let predicate = ref_arg(args, 1);
            let (class, method) =
                vm.resolve_virtual_method(predicate, "test", "(Ljava/lang/Object;)Z")?;
            let keep = vm
                .invoke_method(
                    class,
                    method,
                    vec![Value::Ref(predicate), Value::Ref(value)],
                )?
                .as_int()
                != 0;
            if keep {
                object(this)
            } else {
                optional_new(vm, ObjectRef::NULL)
            }
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = optional_value(vm, receiver(args));
            if value.is_null() {
                return object(vm.make_string("Optional.empty")?);
            }
            let (class, method) =
                vm.resolve_virtual_method(value, "toString", "()Ljava/lang/String;")?;
            let text = vm.invoke_method(class, method, vec![Value::Ref(value)])?;
            let text = vm.string_or_empty(text.as_ref());
            object(vm.make_string(&alloc::format!("Optional[{text}]"))?)
        },
    ),
];

pub(crate) const OPTIONAL: oxjvm_vm::NativeClass = class(
    "java/util/Optional",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("value", "Ljava/lang/Object;", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "EMPTY",
            "Ljava/util/Optional;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &OPTIONAL_METHODS,
    None,
);

fn optional_init(vm: &mut Vm<'_>, args: &[Value], value: ObjectRef) -> Result<Value, VmError> {
    let this = receiver(args);
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "value", "Ljava/lang/Object;")?;
    vm.set_instance_ref(this, declaring, field, value);
    void()
}

fn optional_value(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    let class = vm.class_of(this);
    vm.read_ref_field(this, class, "value", "Ljava/lang/Object;")
        .unwrap_or(ObjectRef::NULL)
}

pub(crate) fn optional_new(vm: &mut Vm<'_>, value: ObjectRef) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/util/Optional")?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "value", "Ljava/lang/Object;")?;
    vm.set_instance_ref(instance, declaring, field, value);
    object(instance)
}

/// The shared empty `Optional` (without the native-call context of `Optional.empty`).
pub(crate) fn optional_empty_value(vm: &mut Vm<'_>) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/util/Optional")?;
    if let Some(empty) = vm.read_static_ref_if_set(class, "EMPTY", "Ljava/util/Optional;") {
        return object(empty);
    }
    let instance = optional_new(vm, ObjectRef::NULL)?;
    let empty = instance.as_ref();
    vm.set_static_value(class, "EMPTY", "Ljava/util/Optional;", Value::Ref(empty));
    Ok(instance)
}

fn optional_empty(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    _args: &[Value],
) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/util/Optional")?;
    if let Some(empty) = vm.read_static_ref_if_set(class, "EMPTY", "Ljava/util/Optional;") {
        return object(empty);
    }
    let instance = optional_new(vm, ObjectRef::NULL)?;
    let empty = instance.as_ref();
    vm.set_static_value(class, "EMPTY", "Ljava/util/Optional;", Value::Ref(empty));
    Ok(instance)
}
