//! `java.lang.invoke`: method handles, method types, lookups, and the bootstrap-method stubs.
//!
//! `LambdaMetafactory`, `StringConcatFactory`, and `ObjectMethods` are never *called*: the runtime
//! intercepts `invokedynamic` at resolution time. The classes exist so method references to their
//! bootstrap methods resolve, and their implementations raise `BootstrapMethodError` if called
//! directly.

use alloc::string::String;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{MethodHandleValue, ObjectRef, Value, Vm, VmError};

use crate::{class, field, method, object, receiver, ref_arg, string_arg};

// -------------------------------------------------------------------------------------------
// MethodType
// -------------------------------------------------------------------------------------------

const METHOD_TYPE_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method(
        "descriptorString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let descriptor = vm.method_type_of(receiver(args)).unwrap_or_default();
            object(vm.make_string(&descriptor)?)
        },
    ),
    method(
        "toMethodDescriptorString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let descriptor = vm.method_type_of(receiver(args)).unwrap_or_default();
            object(vm.make_string(&descriptor)?)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let descriptor = vm.method_type_of(receiver(args)).unwrap_or_default();
            object(vm.make_string(&descriptor)?)
        },
    ),
];

pub(crate) const METHOD_TYPE: oxjvm_vm::NativeClass = class(
    "java/lang/invoke/MethodType",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &METHOD_TYPE_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// MethodHandle
// -------------------------------------------------------------------------------------------

const METHOD_HANDLE_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let handle = vm.method_handle_of(receiver(args));
            let text = match handle {
                Some(MethodHandleValue::Static { .. }) => "MethodHandle(static)",
                Some(MethodHandleValue::Virtual { .. }) => "MethodHandle(virtual)",
                Some(MethodHandleValue::Special { .. }) => "MethodHandle(special)",
                Some(MethodHandleValue::New { .. }) => "MethodHandle(constructor)",
                Some(MethodHandleValue::Getter { .. })
                | Some(MethodHandleValue::StaticGetter { .. }) => "MethodHandle(getter)",
                Some(MethodHandleValue::Setter { .. })
                | Some(MethodHandleValue::StaticSetter { .. }) => "MethodHandle(setter)",
                Some(MethodHandleValue::Bound { .. }) => "MethodHandle(bound)",
                Some(MethodHandleValue::Identity { .. }) => "MethodHandle(identity)",
                None => "MethodHandle",
            };
            object(vm.make_string(text)?)
        },
    ),
    method(
        "asType",
        "(Ljava/lang/invoke/MethodType;)Ljava/lang/invoke/MethodHandle;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
];

pub(crate) const METHOD_HANDLE: oxjvm_vm::NativeClass = class(
    "java/lang/invoke/MethodHandle",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &METHOD_HANDLE_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// MethodHandles + Lookup
// -------------------------------------------------------------------------------------------

const METHOD_HANDLES_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method(
        "lookup",
        "()Ljava/lang/invoke/MethodHandles$Lookup;",
        ACC_PUBLIC | ACC_STATIC,
        lookup_singleton,
    ),
    method(
        "publicLookup",
        "()Ljava/lang/invoke/MethodHandles$Lookup;",
        ACC_PUBLIC | ACC_STATIC,
        lookup_singleton,
    ),
    method(
        "identity",
        "(Ljava/lang/Class;)Ljava/lang/invoke/MethodHandle;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let class = class_argument(vm, ref_arg(args, 0))?;
            let handle = vm.make_method_handle(MethodHandleValue::Identity { class })?;
            object(handle)
        },
    ),
];

fn lookup_singleton(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    _args: &[Value],
) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/lang/invoke/MethodHandles$Lookup")?;
    let existing = vm.read_static_ref(
        class,
        "IMPL_LOOKUP",
        "Ljava/lang/invoke/MethodHandles$Lookup;",
    );
    if let Some(existing) = existing {
        return object(existing);
    }
    let lookup = vm.new_instance(class)?;
    vm.set_static_value(
        class,
        "IMPL_LOOKUP",
        "Ljava/lang/invoke/MethodHandles$Lookup;",
        Value::Ref(lookup),
    );
    object(lookup)
}

pub(crate) const METHOD_HANDLES: oxjvm_vm::NativeClass = class(
    "java/lang/invoke/MethodHandles",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &METHOD_HANDLES_METHODS,
    None,
);

const LOOKUP_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "findStatic",
        "(Ljava/lang/Class;Ljava/lang/String;Ljava/lang/invoke/MethodType;)Ljava/lang/invoke/MethodHandle;",
        ACC_PUBLIC,
        lookup_find_static,
    ),
    method(
        "findVirtual",
        "(Ljava/lang/Class;Ljava/lang/String;Ljava/lang/invoke/MethodType;)Ljava/lang/invoke/MethodHandle;",
        ACC_PUBLIC,
        lookup_find_virtual,
    ),
    method(
        "findSpecial",
        "(Ljava/lang/Class;Ljava/lang/String;Ljava/lang/invoke/MethodType;Ljava/lang/Class;)Ljava/lang/invoke/MethodHandle;",
        ACC_PUBLIC,
        lookup_find_special,
    ),
    method(
        "findConstructor",
        "(Ljava/lang/Class;Ljava/lang/invoke/MethodType;)Ljava/lang/invoke/MethodHandle;",
        ACC_PUBLIC,
        lookup_find_constructor,
    ),
    method(
        "findGetter",
        "(Ljava/lang/Class;Ljava/lang/String;Ljava/lang/Class;)Ljava/lang/invoke/MethodHandle;",
        ACC_PUBLIC,
        lookup_find_getter,
    ),
    method(
        "findSetter",
        "(Ljava/lang/Class;Ljava/lang/String;Ljava/lang/Class;)Ljava/lang/invoke/MethodHandle;",
        ACC_PUBLIC,
        lookup_find_setter,
    ),
];

/// The class denoted by a `Class` argument.
pub(crate) fn class_argument(
    vm: &mut Vm<'_>,
    reference: ObjectRef,
) -> Result<oxjvm_vm::ClassId, VmError> {
    match vm.heap.get(reference).map(|object| &object.data) {
        Some(oxjvm_vm::ObjectData::Class(class)) => Ok(*class),
        _ => Err(vm.throw_new("java/lang/NullPointerException", None)),
    }
}

fn method_type_argument(vm: &mut Vm<'_>, reference: ObjectRef) -> Result<String, VmError> {
    vm.method_type_of(reference)
        .ok_or_else(|| vm.throw_new("java/lang/NullPointerException", None))
}

fn lookup_find_static(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, ref_arg(args, 1))?;
    let name = string_arg(vm, args[2])?;
    let descriptor = method_type_argument(vm, ref_arg(args, 3))?;
    let handle = vm.find_method_handle_target(class, &name, &descriptor, true, false, false)?;
    let value = vm.make_method_handle(handle)?;
    object(value)
}

fn lookup_find_virtual(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, ref_arg(args, 1))?;
    let name = string_arg(vm, args[2])?;
    let descriptor = method_type_argument(vm, ref_arg(args, 3))?;
    let handle = vm.find_method_handle_target(class, &name, &descriptor, false, true, false)?;
    let value = vm.make_method_handle(handle)?;
    object(value)
}

fn lookup_find_special(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, ref_arg(args, 1))?;
    let name = string_arg(vm, args[2])?;
    let descriptor = method_type_argument(vm, ref_arg(args, 3))?;
    let handle = vm.find_method_handle_target(class, &name, &descriptor, false, false, false)?;
    let value = vm.make_method_handle(handle)?;
    object(value)
}

fn lookup_find_constructor(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, ref_arg(args, 1))?;
    let descriptor = method_type_argument(vm, ref_arg(args, 2))?;
    let handle = vm.find_method_handle_target(class, "<init>", &descriptor, false, false, true)?;
    let value = vm.make_method_handle(handle)?;
    object(value)
}

fn class_field_descriptor(vm: &mut Vm<'_>, class: oxjvm_vm::ClassId) -> String {
    if vm.classes.get(class).kind == oxjvm_vm::ClassKind::Primitive {
        match vm.classes.get(class).primitive {
            Some(oxjvm_classfile::BaseType::Boolean) => "Z".into(),
            Some(oxjvm_classfile::BaseType::Byte) => "B".into(),
            Some(oxjvm_classfile::BaseType::Char) => "C".into(),
            Some(oxjvm_classfile::BaseType::Short) => "S".into(),
            Some(oxjvm_classfile::BaseType::Int) => "I".into(),
            Some(oxjvm_classfile::BaseType::Long) => "J".into(),
            Some(oxjvm_classfile::BaseType::Float) => "F".into(),
            Some(oxjvm_classfile::BaseType::Double) => "D".into(),
            _ => "V".into(),
        }
    } else {
        let name = vm.class_name(class);
        if name.starts_with('[') {
            name.into()
        } else {
            alloc::format!("L{name};")
        }
    }
}

fn lookup_find_getter(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, ref_arg(args, 1))?;
    let name = string_arg(vm, args[2])?;
    let field_class = class_argument(vm, ref_arg(args, 3))?;
    let descriptor = class_field_descriptor(vm, field_class);
    let (declaring, field) = vm.find_field(class, &name, &descriptor)?;
    let is_static = vm.classes.get(declaring).fields[field as usize].is_static;
    let handle = if is_static {
        MethodHandleValue::StaticGetter {
            class: declaring,
            field,
        }
    } else {
        MethodHandleValue::Getter {
            class: declaring,
            field,
        }
    };
    object(vm.make_method_handle(handle)?)
}

fn lookup_find_setter(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, ref_arg(args, 1))?;
    let name = string_arg(vm, args[2])?;
    let field_class = class_argument(vm, ref_arg(args, 3))?;
    let descriptor = class_field_descriptor(vm, field_class);
    let (declaring, field) = vm.find_field(class, &name, &descriptor)?;
    let is_static = vm.classes.get(declaring).fields[field as usize].is_static;
    let handle = if is_static {
        MethodHandleValue::StaticSetter {
            class: declaring,
            field,
        }
    } else {
        MethodHandleValue::Setter {
            class: declaring,
            field,
        }
    };
    object(vm.make_method_handle(handle)?)
}

pub(crate) const LOOKUP: oxjvm_vm::NativeClass = class(
    "java/lang/invoke/MethodHandles$Lookup",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[field(
        "IMPL_LOOKUP",
        "Ljava/lang/invoke/MethodHandles$Lookup;",
        ACC_PRIVATE | ACC_STATIC | ACC_SYNTHETIC,
        None,
    )],
    &LOOKUP_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Bootstrap-method stubs
// -------------------------------------------------------------------------------------------

fn bootstrap_stub(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    _args: &[Value],
) -> Result<Value, VmError> {
    Err(vm.throw_new(
        "java/lang/BootstrapMethodError",
        Some("bootstrap methods are resolved by the runtime, not called directly"),
    ))
}

const LAMBDA_METAFACTORY_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "metafactory",
        "(Ljava/lang/invoke/MethodHandles$Lookup;Ljava/lang/String;Ljava/lang/invoke/MethodType;Ljava/lang/invoke/MethodType;Ljava/lang/invoke/MethodHandle;Ljava/lang/invoke/MethodType;)Ljava/lang/invoke/CallSite;",
        ACC_PUBLIC | ACC_STATIC,
        bootstrap_stub,
    ),
    method(
        "altMetafactory",
        "(Ljava/lang/invoke/MethodHandles$Lookup;Ljava/lang/String;Ljava/lang/invoke/MethodType;[Ljava/lang/Object;)Ljava/lang/invoke/CallSite;",
        ACC_PUBLIC | ACC_STATIC,
        bootstrap_stub,
    ),
];

pub(crate) const LAMBDA_METAFACTORY: oxjvm_vm::NativeClass = class(
    "java/lang/invoke/LambdaMetafactory",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &LAMBDA_METAFACTORY_METHODS,
    None,
);

const STRING_CONCAT_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "makeConcat",
        "(Ljava/lang/invoke/MethodHandles$Lookup;Ljava/lang/String;Ljava/lang/invoke/MethodType;)Ljava/lang/invoke/CallSite;",
        ACC_PUBLIC | ACC_STATIC,
        bootstrap_stub,
    ),
    method(
        "makeConcatWithConstants",
        "(Ljava/lang/invoke/MethodHandles$Lookup;Ljava/lang/String;Ljava/lang/invoke/MethodType;Ljava/lang/String;[Ljava/lang/Object;)Ljava/lang/invoke/CallSite;",
        ACC_PUBLIC | ACC_STATIC,
        bootstrap_stub,
    ),
];

pub(crate) const STRING_CONCAT_FACTORY: oxjvm_vm::NativeClass = class(
    "java/lang/invoke/StringConcatFactory",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &STRING_CONCAT_METHODS,
    None,
);

const OBJECT_METHODS_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "bootstrap",
    "(Ljava/lang/invoke/MethodHandles$Lookup;Ljava/lang/String;Ljava/lang/invoke/TypeDescriptor;Ljava/lang/Class;Ljava/lang/String;[Ljava/lang/invoke/MethodHandle;)Ljava/lang/Object;",
    ACC_PUBLIC | ACC_STATIC,
    bootstrap_stub,
)];

pub(crate) const OBJECT_METHODS: oxjvm_vm::NativeClass = class(
    "java/lang/runtime/ObjectMethods",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &OBJECT_METHODS_METHODS,
    None,
);

pub(crate) const TYPE_DESCRIPTOR: oxjvm_vm::NativeClass = class(
    "java/lang/invoke/TypeDescriptor",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

// -------------------------------------------------------------------------------------------
// Functional interfaces
// -------------------------------------------------------------------------------------------

macro_rules! functional_interface {
    ($constant:ident, $name:literal $(, $interfaces:literal)*) => {
        pub(crate) const $constant: oxjvm_vm::NativeClass = class(
            $name,
            Some("java/lang/Object"),
            &[$($interfaces),*],
            ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
            &[],
            &FUNCTIONAL_METHODS,
            None,
        );
    };
}

// Each interface carries the same shape; the dispatcher selects by descriptor, so a shared array
// with one abstract method of each common descriptor is sufficient and lets a single class serve
// every functional interface users link against.
const FUNCTIONAL_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method(
        "apply",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "apply",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "accept",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "accept",
        "(Ljava/lang/Object;Ljava/lang/Object;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "get",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "test",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "test",
        "(Ljava/lang/Object;Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "run",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "call",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

functional_interface!(FUNCTION, "java/util/function/Function");
functional_interface!(BIFUNCTION, "java/util/function/BiFunction");
functional_interface!(CONSUMER, "java/util/function/Consumer");
functional_interface!(BICONSUMER, "java/util/function/BiConsumer");
functional_interface!(SUPPLIER, "java/util/function/Supplier");
functional_interface!(PREDICATE, "java/util/function/Predicate");
functional_interface!(BIPREDICATE, "java/util/function/BiPredicate");
functional_interface!(
    UNARY_OPERATOR,
    "java/util/function/UnaryOperator",
    "java/util/function/Function"
);
functional_interface!(
    BINARY_OPERATOR,
    "java/util/function/BinaryOperator",
    "java/util/function/BiFunction"
);
functional_interface!(CALLABLE, "java/util/concurrent/Callable");

pub(crate) const APPENDABLE: oxjvm_vm::NativeClass = class(
    "java/lang/Appendable",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[
        method(
            "append",
            "(Ljava/lang/CharSequence;)Ljava/lang/Appendable;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "append",
            "(Ljava/lang/CharSequence;II)Ljava/lang/Appendable;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "append",
            "(C)Ljava/lang/Appendable;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
    ],
    None,
);
