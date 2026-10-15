//! `java.lang.annotation`: the enum-shaped `ElementType`/`RetentionPolicy`
//! and the annotation interfaces (`Retention`, `Target`, `Deprecated`, ...).
//!
//! oxjvm does not materialise annotation instances yet, so the interfaces only
//! need to resolve and expose the methods tests call through reflection.

use oxjvm_classfile::flags::*;

use crate::lang::abstract_method_pub;
use crate::{class, field, method, object, receiver, ref_arg};
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

const ELEMENT_TYPES: [&str; 11] = [
    "TYPE",
    "FIELD",
    "METHOD",
    "PARAMETER",
    "CONSTRUCTOR",
    "LOCAL_VARIABLE",
    "ANNOTATION_TYPE",
    "PACKAGE",
    "TYPE_PARAMETER",
    "TYPE_USE",
    "MODULE",
];

const RETENTION_POLICIES: [&str; 3] = ["SOURCE", "CLASS", "RUNTIME"];

fn annotation_constants(vm: &Vm<'_>, class: oxjvm_vm::ClassId) -> &'static [&'static str] {
    match vm.class_name(class) {
        "java/lang/annotation/ElementType" => &ELEMENT_TYPES,
        "java/lang/annotation/RetentionPolicy" => &RETENTION_POLICIES,
        _ => &[],
    }
}

fn annotation_enum_clinit(
    vm: &mut Vm<'_>,
    class_name: &str,
    constants: &[&str],
) -> Result<(), VmError> {
    let class = vm.resolve_class(class_name)?;
    for (index, name) in constants.iter().enumerate() {
        let instance = vm.new_instance(class)?;
        if let Some((declaring, field)) = vm.find_field_opt(class, "ordinal", "I") {
            vm.set_instance_int(instance, declaring, field, index as i32);
        }
        let descriptor = alloc::format!("L{class_name};");
        vm.set_static_value(class, name, &descriptor, Value::Ref(instance));
    }
    Ok(())
}

fn element_type_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    annotation_enum_clinit(vm, "java/lang/annotation/ElementType", &ELEMENT_TYPES)
}

fn retention_policy_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    annotation_enum_clinit(
        vm,
        "java/lang/annotation/RetentionPolicy",
        &RETENTION_POLICIES,
    )
}

fn annotation_ordinal(vm: &Vm<'_>, instance: ObjectRef) -> i32 {
    crate::lang::read_int_field(vm, instance, "ordinal")
}

fn annotation_name_of(
    vm: &Vm<'_>,
    ctx: oxjvm_vm::NativeContext,
    ordinal: i32,
) -> alloc::string::String {
    let constants = annotation_constants(vm, ctx.class);
    let name = constants
        .get(ordinal.clamp(0, constants.len().saturating_sub(1) as i32) as usize)
        .copied()
        .unwrap_or_default();
    alloc::string::String::from(name)
}

macro_rules! annotation_enum_methods {
    ($constant:ident, $class_name:literal) => {
        const $constant: [oxjvm_vm::NativeMethodDef; 5] = [
            method("ordinal", "()I", ACC_PUBLIC, |vm, _, args| {
                Ok(Value::Int(annotation_ordinal(vm, receiver(args))))
            }),
            method(
                "name",
                "()Ljava/lang/String;",
                ACC_PUBLIC,
                |vm, ctx, args| {
                    let name = annotation_name_of(vm, ctx, annotation_ordinal(vm, receiver(args)));
                    object(vm.make_string(&name)?)
                },
            ),
            method(
                "toString",
                "()Ljava/lang/String;",
                ACC_PUBLIC,
                |vm, ctx, args| {
                    let name = annotation_name_of(vm, ctx, annotation_ordinal(vm, receiver(args)));
                    object(vm.make_string(&name)?)
                },
            ),
            method(
                "values",
                concat!("()[L", $class_name, ";"),
                ACC_PUBLIC | ACC_STATIC,
                |vm, ctx, _| {
                    let class = ctx.class;
                    let constants = annotation_constants(vm, class);
                    let array = vm.allocate_object_array(class, constants.len())?;
                    let descriptor = concat!("L", $class_name, ";");
                    for (index, name) in constants.iter().enumerate() {
                        if let Some(value) = vm.read_static_ref_if_set(class, name, descriptor) {
                            vm.array_set_ref(array, index, value)?;
                        }
                    }
                    object(array)
                },
            ),
            method(
                "valueOf",
                concat!("(Ljava/lang/String;)L", $class_name, ";"),
                ACC_PUBLIC | ACC_STATIC,
                |vm, ctx, args| {
                    let name = vm.string_or_empty(ref_arg(args, 0));
                    let class = ctx.class;
                    let descriptor = concat!("L", $class_name, ";");
                    if annotation_constants(vm, class).contains(&name.as_str()) {
                        if let Some(value) = vm.read_static_ref_if_set(class, &name, descriptor) {
                            return object(value);
                        }
                    }
                    Err(vm.throw_new(
                        "java/lang/IllegalArgumentException",
                        Some(&alloc::format!("No enum constant {name}")),
                    ))
                },
            ),
        ];
    };
}

annotation_enum_methods!(ELEMENT_TYPE_METHODS, "java/lang/annotation/ElementType");
annotation_enum_methods!(
    RETENTION_POLICY_METHODS,
    "java/lang/annotation/RetentionPolicy"
);

macro_rules! annotation_enum_class {
    ($constant:ident, $name:literal, $methods:ident, $clinit:expr, [$($field:literal),* $(,)?]) => {
        pub(crate) const $constant: oxjvm_vm::NativeClass = class(
            $name,
            Some("java/lang/Object"),
            &[],
            ACC_PUBLIC | ACC_FINAL,
            &[
                field("ordinal", "I", ACC_PRIVATE | ACC_FINAL, None),
                $(field($field, concat!("L", $name, ";"), ACC_PUBLIC | ACC_STATIC | ACC_FINAL, None),)*
            ],
            &$methods,
            Some($clinit),
        );
    };
}

annotation_enum_class!(
    ELEMENT_TYPE,
    "java/lang/annotation/ElementType",
    ELEMENT_TYPE_METHODS,
    element_type_clinit,
    [
        "TYPE",
        "FIELD",
        "METHOD",
        "PARAMETER",
        "CONSTRUCTOR",
        "LOCAL_VARIABLE",
        "ANNOTATION_TYPE",
        "PACKAGE",
        "TYPE_PARAMETER",
        "TYPE_USE",
        "MODULE",
    ]
);

annotation_enum_class!(
    RETENTION_POLICY,
    "java/lang/annotation/RetentionPolicy",
    RETENTION_POLICY_METHODS,
    retention_policy_clinit,
    ["SOURCE", "CLASS", "RUNTIME"]
);

// -------------------------------------------------------------------------------------------
// Annotation interfaces
// -------------------------------------------------------------------------------------------

macro_rules! annotation_interface {
    ($constant:ident, $name:literal, $methods:expr) => {
        pub(crate) const $constant: oxjvm_vm::NativeClass = class(
            $name,
            Some("java/lang/Object"),
            &[],
            ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
            &[],
            $methods,
            None,
        );
    };
}

const RETENTION_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "value",
    "()Ljava/lang/annotation/RetentionPolicy;",
    ACC_PUBLIC | ACC_ABSTRACT,
    abstract_method_pub,
)];

annotation_interface!(
    RETENTION,
    "java/lang/annotation/Retention",
    &RETENTION_METHODS
);

const TARGET_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "value",
    "()[Ljava/lang/annotation/ElementType;",
    ACC_PUBLIC | ACC_ABSTRACT,
    abstract_method_pub,
)];

annotation_interface!(TARGET, "java/lang/annotation/Target", &TARGET_METHODS);

const DEPRECATED_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "since",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "forRemoval",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
];

annotation_interface!(DEPRECATED, "java/lang/Deprecated", &DEPRECATED_METHODS);

const REPEATABLE_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "value",
    "()Ljava/lang/Class;",
    ACC_PUBLIC | ACC_ABSTRACT,
    abstract_method_pub,
)];

annotation_interface!(
    REPEATABLE,
    "java/lang/annotation/Repeatable",
    &REPEATABLE_METHODS
);

annotation_interface!(DOCUMENTED, "java/lang/annotation/Documented", &[]);
annotation_interface!(INHERITED, "java/lang/annotation/Inherited", &[]);
