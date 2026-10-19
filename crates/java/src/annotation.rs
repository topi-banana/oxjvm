//! `java.lang.annotation`: the enum-shaped `ElementType`/`RetentionPolicy`
//! and the annotation interfaces (`Retention`, `Target`, `Deprecated`, ...).
//!
//! Runtime-visible annotations are materialised on demand: an instance of the
//! annotation interface is created and its element methods are backed by a
//! hidden instance field holding the boxed element values.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use oxjvm_classfile::descriptor::MethodDescriptor;
use oxjvm_classfile::flags::*;
use oxjvm_classfile::{Annotation, BaseType, ConstantPool, CpInfo, ElementValue, FieldType};

use crate::lang::abstract_method_pub;
use crate::{class, field, method, object, receiver, ref_arg, void};
use oxjvm_vm::{ArrayComponent, ClassId, Field, ObjectRef, Value, Vm, VmError};

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

fn annotation_constants(vm: &Vm<'_>, class: ClassId) -> &'static [&'static str] {
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

fn annotation_name_of(vm: &Vm<'_>, ctx: oxjvm_vm::NativeContext, ordinal: i32) -> String {
    let constants = annotation_constants(vm, ctx.class);
    let name = constants
        .get(ordinal.clamp(0, constants.len().saturating_sub(1) as i32) as usize)
        .copied()
        .unwrap_or_default();
    String::from(name)
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
            &["java/lang/annotation/Annotation"],
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

fn incomplete_annotation_ctor(
    vm: &mut Vm<'_>,
    context: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let annotation_type = ref_arg(args, 1);
    let element_name = ref_arg(args, 2);
    if annotation_type.is_null() || element_name.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let (declaring, field) = vm.find_field(context.class, "annotationType", "Ljava/lang/Class;")?;
    vm.set_instance_ref(this, declaring, field, annotation_type);
    let (declaring, field) = vm.find_field(context.class, "elementName", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, element_name);
    let annotation_class = crate::lang::class_argument(vm, annotation_type)?;
    let type_name = crate::binary_name(vm, annotation_class);
    let element = vm.string_or_empty(element_name);
    let message = alloc::format!("Annotation missing element member: {element} in {type_name}");
    let message = vm.make_string(&message)?;
    let (declaring, field) = vm.find_field(context.class, "detailMessage", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, message);
    vm.capture_backtrace(this);
    void()
}

/// `java.lang.annotation.IncompleteAnnotationException`.
pub(crate) const INCOMPLETE_ANNOTATION_EXCEPTION: oxjvm_vm::NativeClass = class(
    "java/lang/annotation/IncompleteAnnotationException",
    Some("java/lang/RuntimeException"),
    &[],
    ACC_PUBLIC,
    &[
        field(
            "annotationType",
            "Ljava/lang/Class;",
            ACC_PRIVATE | ACC_FINAL,
            None,
        ),
        field(
            "elementName",
            "Ljava/lang/String;",
            ACC_PRIVATE | ACC_FINAL,
            None,
        ),
    ],
    &[
        method(
            "<init>",
            "(Ljava/lang/Class;Ljava/lang/String;)V",
            ACC_PUBLIC,
            incomplete_annotation_ctor,
        ),
        method(
            "annotationType",
            "()Ljava/lang/Class;",
            ACC_PUBLIC,
            |vm, _, args| {
                let this = receiver(args);
                let class = vm.class_of(this);
                let value = vm
                    .read_ref_field(this, class, "annotationType", "Ljava/lang/Class;")
                    .unwrap_or(ObjectRef::NULL);
                object(value)
            },
        ),
        method(
            "elementName",
            "()Ljava/lang/String;",
            ACC_PUBLIC,
            |vm, _, args| {
                let this = receiver(args);
                let class = vm.class_of(this);
                let value = vm
                    .read_ref_field(this, class, "elementName", "Ljava/lang/String;")
                    .unwrap_or(ObjectRef::NULL);
                object(value)
            },
        ),
    ],
    None,
);

// -------------------------------------------------------------------------------------------
// Annotation materialisation
// -------------------------------------------------------------------------------------------

/// Hidden instance field holding the boxed element values of a materialised annotation.
const VALUES_FIELD: &str = "$oxjvm$annotationValues";

/// Descriptor of [`VALUES_FIELD`].
const VALUES_DESCRIPTOR: &str = "[Ljava/lang/Object;";

/// One declared element method of an annotation interface.
struct ElementMethod {
    /// Index in the annotation class's method table.
    index: usize,
    /// Element name.
    name: String,
    /// Return type descriptor.
    descriptor: String,
    /// `AnnotationDefault`, when declared.
    default: Option<ElementValue>,
}

/// The element methods of an annotation interface, in class-file order.
fn element_methods(vm: &Vm<'_>, class: ClassId) -> Vec<ElementMethod> {
    vm.classes
        .get(class)
        .methods
        .iter()
        .enumerate()
        .filter(|(_, method)| {
            !method.is_static() && method.name != "<init>" && method.name != "<clinit>"
        })
        .map(|(index, method)| ElementMethod {
            index,
            name: method.name.clone(),
            descriptor: method
                .descriptor
                .strip_prefix("()")
                .unwrap_or(&method.descriptor)
                .to_string(),
            default: method.annotation_default.clone(),
        })
        .collect()
}

/// The internal name of an annotation from its class-file entry.
fn annotation_type_name(pool: &ConstantPool, annotation: &Annotation) -> Option<String> {
    let descriptor = pool.utf8(annotation.type_index).ok()?;
    let name = descriptor.strip_prefix('L')?.strip_suffix(';')?;
    Some(name.to_string())
}

/// The internal name of an object descriptor.
fn object_descriptor_name(descriptor: &str) -> Option<String> {
    let name = descriptor.strip_prefix('L')?.strip_suffix(';')?;
    Some(name.to_string())
}

/// Resolve the annotation interface of a class-file annotation.
fn annotation_class_of(
    vm: &mut Vm<'_>,
    owner: ClassId,
    annotation: &Annotation,
) -> Option<ClassId> {
    let name = annotation_type_name(&vm.classes.get(owner).constant_pool, annotation)?;
    vm.resolve_class_lenient(&name)
}

/// Whether an annotation type is itself annotated with `@Inherited`.
fn annotation_is_inherited(vm: &Vm<'_>, annotation_class: ClassId) -> bool {
    let class = vm.classes.get(annotation_class);
    class.annotations.iter().any(|annotation| {
        annotation_type_name(&class.constant_pool, annotation).as_deref()
            == Some("java/lang/annotation/Inherited")
    })
}

/// Add the hidden values field to an annotation interface, if needed.
fn ensure_values_field(vm: &mut Vm<'_>, class: ClassId) {
    if vm
        .find_field_opt(class, VALUES_FIELD, VALUES_DESCRIPTOR)
        .is_some()
    {
        return;
    }
    let class_ref = vm.classes.get_mut(class);
    let slot = class_ref.instance_slots;
    class_ref.instance_slots += 1;
    class_ref
        .instance_defaults
        .push(Value::Ref(ObjectRef::NULL));
    class_ref.fields.push(Field {
        name: VALUES_FIELD.into(),
        descriptor: VALUES_DESCRIPTOR.into(),
        access_flags: ACC_PRIVATE | ACC_SYNTHETIC | ACC_FINAL,
        owner: class,
        is_static: false,
        slot,
        constant: None,
        signature: None,
        annotations: Vec::new(),
    });
}

/// Back every element method with [`annotation_element_native`].
///
/// Native annotation interfaces (such as `java.lang.annotation.Repeatable`) register their
/// elements with the abstract stub, which [`Method::is_abstract`] does not report as abstract
/// because it has a native body; overwrite it unconditionally.
fn install_element_natives(vm: &mut Vm<'_>, class: ClassId, elements: &[ElementMethod]) {
    for element in elements {
        let method = &mut vm.classes.get_mut(class).methods[element.index];
        method.native = Some(annotation_element_native);
    }
}

/// The index of an element method among the annotation's elements.
fn element_position(vm: &Vm<'_>, class: ClassId, method: u32) -> usize {
    let mut position = 0;
    for (index, entry) in vm.classes.get(class).methods.iter().enumerate() {
        if entry.is_static() || entry.name == "<init>" || entry.name == "<clinit>" {
            continue;
        }
        if index as u32 == method {
            return position;
        }
        position += 1;
    }
    0
}

/// Element implementation: read the boxed value from the hidden array.
fn annotation_element_native(
    vm: &mut Vm<'_>,
    context: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let return_type = vm
        .classes
        .get(context.class)
        .methods
        .get(context.method as usize)
        .map(|method| {
            method
                .descriptor
                .strip_prefix("()")
                .unwrap_or("")
                .to_string()
        })
        .unwrap_or_default();
    let position = element_position(vm, context.class, context.method);
    let class = vm.class_of(this);
    let Some(values) = vm.read_ref_field(this, class, VALUES_FIELD, VALUES_DESCRIPTOR) else {
        return Ok(Value::Ref(ObjectRef::NULL));
    };
    let boxed = vm.array_get(values, position as i32)?;
    if let Some(error) = annotation_value_error(vm, boxed)? {
        return Err(error);
    }
    Ok(unbox_annotation_value(vm, boxed, &return_type))
}

/// Unbox a stored element value back into an operand-stack value.
fn unbox_annotation_value(vm: &Vm<'_>, value: Value, descriptor: &str) -> Value {
    match descriptor {
        "Z" | "B" | "C" | "S" | "I" => match value {
            Value::Ref(reference) if !reference.is_null() => {
                Value::Int(crate::boxed::read_value_int(vm, reference))
            }
            Value::Ref(_) => Value::Int(0),
            other => other,
        },
        "J" => match value {
            Value::Ref(reference) if !reference.is_null() => {
                Value::Long(crate::boxed::read_value_long(vm, reference))
            }
            Value::Ref(_) => Value::Long(0),
            Value::Int(value) => Value::Long(value as i64),
            other => other,
        },
        "F" => match value {
            Value::Ref(reference) if !reference.is_null() => Value::Float(f32::from_bits(
                crate::boxed::read_value_int(vm, reference) as u32,
            )),
            Value::Ref(_) => Value::Float(0.0),
            Value::Int(value) => Value::Float(value as f32),
            other => other,
        },
        "D" => match value {
            Value::Ref(reference) if !reference.is_null() => Value::Double(f64::from_bits(
                crate::boxed::read_value_long(vm, reference) as u64,
            )),
            Value::Ref(_) => Value::Double(0.0),
            Value::Int(value) => Value::Double(value as f64),
            other => other,
        },
        _ => value,
    }
}

/// Box an element value for storage in the hidden `Object[]`.
fn box_annotation_value(vm: &mut Vm<'_>, value: Value, descriptor: &str) -> Result<Value, VmError> {
    match descriptor {
        "F" => {
            let class = vm.resolve_class("java/lang/Float")?;
            let instance = vm.new_instance(class)?;
            crate::boxed::write_value_int(vm, instance, value.as_float().to_bits() as i32);
            Ok(Value::Ref(instance))
        }
        "D" => crate::boxed::boxed_double(vm, value.as_double()),
        _ => crate::boxed::box_value(vm, value, descriptor),
    }
}

/// The array component for a primitive descriptor.
fn array_component(descriptor: &str) -> Option<ArrayComponent> {
    Some(match descriptor {
        "Z" => ArrayComponent::Boolean,
        "B" => ArrayComponent::Byte,
        "C" => ArrayComponent::Char,
        "S" => ArrayComponent::Short,
        "I" => ArrayComponent::Int,
        "J" => ArrayComponent::Long,
        "F" => ArrayComponent::Float,
        "D" => ArrayComponent::Double,
        _ => return None,
    })
}

/// Resolve the class named by a field descriptor (`I`, `Ljava/lang/String;`, `[I`).
pub(crate) fn class_for_descriptor(vm: &mut Vm<'_>, descriptor: &str) -> Result<ClassId, VmError> {
    match FieldType::parse(descriptor) {
        Ok(FieldType::Base(base)) => vm.primitive_class(base),
        Ok(FieldType::Object(name)) => vm.resolve_class(&name),
        Ok(FieldType::Array(_)) => vm.resolve_class(descriptor),
        Err(_) if descriptor == "V" => vm.primitive_class(BaseType::Void),
        Err(_) => Err(vm.throw_new("java/lang/TypeNotPresentException", Some(descriptor))),
    }
}

/// Materialise one annotation, returning null when its type cannot be resolved.
fn materialize(
    vm: &mut Vm<'_>,
    owner: ClassId,
    annotation: &Annotation,
) -> Result<ObjectRef, VmError> {
    let Some(annotation_class) = annotation_class_of(vm, owner, annotation) else {
        return Ok(ObjectRef::NULL);
    };
    let elements = element_methods(vm, annotation_class);
    ensure_values_field(vm, annotation_class);
    install_element_natives(vm, annotation_class, &elements);

    let object_class = vm.resolve_class("java/lang/Object")?;
    let values = vm.allocate_object_array(object_class, elements.len())?;
    vm.protect(values);
    for (position, element) in elements.iter().enumerate() {
        let explicit = annotation.elements.iter().find(|pair| {
            vm.classes
                .get(owner)
                .constant_pool
                .utf8(pair.name_index)
                .is_ok_and(|name| name == element.name)
        });
        let converted = if let Some(pair) = explicit {
            convert_value(vm, owner, &element.descriptor, &pair.value, true)?
        } else if let Some(default) = &element.default {
            convert_value(vm, annotation_class, &element.descriptor, default, true)?
        } else {
            Value::default_for(
                element
                    .descriptor
                    .as_bytes()
                    .first()
                    .copied()
                    .unwrap_or(b'L'),
            )
        };
        let boxed = box_annotation_value(vm, converted, &element.descriptor)?;
        vm.array_set_ref(values, position, boxed.as_ref())?;
    }
    let instance = vm.new_instance(annotation_class)?;
    let (declaring, field) = vm
        .find_field_opt(annotation_class, VALUES_FIELD, VALUES_DESCRIPTOR)
        .ok_or_else(|| VmError::internal("annotation values field missing"))?;
    vm.set_instance_ref(instance, declaring, field, values);
    vm.unprotect(values);
    Ok(instance)
}

/// The exception for an annotation enum constant that is missing from its enum.
fn enum_constant_not_present(vm: &mut Vm<'_>, enum_class: ClassId, constant: &str) -> VmError {
    let result = (|| -> Result<ObjectRef, VmError> {
        let exception_class = vm.resolve_class("java/lang/EnumConstantNotPresentException")?;
        let instance = vm.new_instance(exception_class)?;
        let class_object = vm.class_object(enum_class)?;
        let name = vm.make_string(constant)?;
        let (declaring, constructor) = vm
            .find_method(
                exception_class,
                "<init>",
                "(Ljava/lang/Class;Ljava/lang/String;)V",
            )
            .ok_or_else(|| {
                VmError::internal("missing EnumConstantNotPresentException constructor")
            })?;
        vm.invoke_method(
            declaring,
            constructor,
            alloc::vec![
                Value::Ref(instance),
                Value::Ref(class_object),
                Value::Ref(name)
            ],
        )?;
        Ok(instance)
    })();
    match result {
        Ok(exception) => VmError::Thrown(exception),
        Err(error) => error,
    }
}

/// The internal placeholder for an annotation value that fails lazily on access.
const VALUE_ERROR_CLASS: &str = "oxjvm/AnnotationValueError";

fn annotation_type_mismatch_ctor(
    vm: &mut Vm<'_>,
    context: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let element = ref_arg(args, 1);
    let found_type = ref_arg(args, 2);
    let (declaring, field) =
        vm.find_field(context.class, "element", "Ljava/lang/reflect/Method;")?;
    vm.set_instance_ref(this, declaring, field, element);
    let (declaring, field) = vm.find_field(context.class, "foundType", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, found_type);
    let found = vm.string_or_empty(found_type);
    let message = alloc::format!("Type '{found}' is not a valid type for element ");
    // The element's string form is appended only when it is available.
    let message = if element.is_null() {
        message
    } else {
        let (class, method) =
            vm.resolve_virtual_method(element, "toString", "()Ljava/lang/String;")?;
        let text = vm
            .invoke_method(class, method, alloc::vec![Value::Ref(element)])?
            .as_ref();
        alloc::format!("{message}{}", vm.string_or_empty(text))
    };
    let message = vm.make_string(&message)?;
    let (declaring, field) = vm.find_field(context.class, "detailMessage", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, message);
    vm.capture_backtrace(this);
    void()
}

/// `java.lang.annotation.AnnotationTypeMismatchException`.
pub(crate) const ANNOTATION_TYPE_MISMATCH_EXCEPTION: oxjvm_vm::NativeClass = class(
    "java/lang/annotation/AnnotationTypeMismatchException",
    Some("java/lang/RuntimeException"),
    &[],
    ACC_PUBLIC,
    &[
        field(
            "element",
            "Ljava/lang/reflect/Method;",
            ACC_PRIVATE | ACC_FINAL,
            None,
        ),
        field(
            "foundType",
            "Ljava/lang/String;",
            ACC_PRIVATE | ACC_FINAL,
            None,
        ),
    ],
    &[
        method(
            "<init>",
            "(Ljava/lang/reflect/Method;Ljava/lang/String;)V",
            ACC_PUBLIC,
            annotation_type_mismatch_ctor,
        ),
        method(
            "element",
            "()Ljava/lang/reflect/Method;",
            ACC_PUBLIC,
            |vm, _, args| {
                let this = receiver(args);
                let class = vm.class_of(this);
                let value = vm
                    .read_ref_field(this, class, "element", "Ljava/lang/reflect/Method;")
                    .unwrap_or(ObjectRef::NULL);
                object(value)
            },
        ),
        method(
            "foundType",
            "()Ljava/lang/String;",
            ACC_PUBLIC,
            |vm, _, args| {
                let this = receiver(args);
                let class = vm.class_of(this);
                let value = vm
                    .read_ref_field(this, class, "foundType", "Ljava/lang/String;")
                    .unwrap_or(ObjectRef::NULL);
                object(value)
            },
        ),
    ],
    None,
);

/// `oxjvm.AnnotationValueError`: stores a deferred annotation-value exception.
pub(crate) const ANNOTATION_VALUE_ERROR: oxjvm_vm::NativeClass = class(
    VALUE_ERROR_CLASS,
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("kind", "I", ACC_PRIVATE, None),
        field("typeName", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("enumType", "Ljava/lang/Class;", ACC_PRIVATE, None),
        field("constantName", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &[],
    None,
);

fn type_error_proxy(vm: &mut Vm<'_>, name: &str) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class(VALUE_ERROR_CLASS)?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "kind", "I")?;
    vm.set_instance_int(instance, declaring, field, 0);
    let (declaring, field) = vm.find_field(class, "typeName", "Ljava/lang/String;")?;
    let name = vm.make_string(name)?;
    vm.set_instance_ref(instance, declaring, field, name);
    Ok(instance)
}

fn enum_error_proxy(
    vm: &mut Vm<'_>,
    enum_class: ClassId,
    constant: &str,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class(VALUE_ERROR_CLASS)?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "kind", "I")?;
    vm.set_instance_int(instance, declaring, field, 1);
    let (declaring, field) = vm.find_field(class, "enumType", "Ljava/lang/Class;")?;
    let enum_object = vm.class_object(enum_class)?;
    vm.set_instance_ref(instance, declaring, field, enum_object);
    let (declaring, field) = vm.find_field(class, "constantName", "Ljava/lang/String;")?;
    let constant = vm.make_string(constant)?;
    vm.set_instance_ref(instance, declaring, field, constant);
    Ok(instance)
}

fn is_annotation_value_error(vm: &mut Vm<'_>, value: Value) -> Result<bool, VmError> {
    let Value::Ref(reference) = value else {
        return Ok(false);
    };
    if reference.is_null() {
        return Ok(false);
    }
    let class = vm.resolve_class(VALUE_ERROR_CLASS)?;
    Ok(vm.class_of(reference) == class)
}

/// How `Annotation.toString` renders a deferred error proxy.
fn annotation_value_error_text(vm: &mut Vm<'_>, value: Value) -> Result<Option<String>, VmError> {
    if !is_annotation_value_error(vm, value)? {
        return Ok(None);
    }
    let Value::Ref(reference) = value else {
        return Ok(None);
    };
    let class = vm.resolve_class(VALUE_ERROR_CLASS)?;
    let kind = crate::lang::read_int_field(vm, reference, "kind");
    if kind == 1 {
        let constant = vm
            .read_ref_field(reference, class, "constantName", "Ljava/lang/String;")
            .unwrap_or(ObjectRef::NULL);
        let text = vm.string_or_empty(constant);
        return Ok(Some(alloc::format!(
            "{text} /* Warning: constant not present! */"
        )));
    }
    let type_name = vm
        .read_ref_field(reference, class, "typeName", "Ljava/lang/String;")
        .unwrap_or(ObjectRef::NULL);
    let text = vm.string_or_empty(type_name).replace('/', ".");
    Ok(Some(alloc::format!(
        "{text}.class /* Warning: type not present! */"
    )))
}

fn annotation_value_error(vm: &mut Vm<'_>, value: Value) -> Result<Option<VmError>, VmError> {
    if !is_annotation_value_error(vm, value)? {
        return Ok(None);
    }
    let Value::Ref(reference) = value else {
        return Ok(None);
    };
    let class = vm.resolve_class(VALUE_ERROR_CLASS)?;
    let kind = crate::lang::read_int_field(vm, reference, "kind");
    if kind == 1 {
        let enum_type = vm
            .read_ref_field(reference, class, "enumType", "Ljava/lang/Class;")
            .unwrap_or(ObjectRef::NULL);
        let constant = vm
            .read_ref_field(reference, class, "constantName", "Ljava/lang/String;")
            .unwrap_or(ObjectRef::NULL);
        let enum_class = crate::lang::class_argument(vm, enum_type)?;
        let text = vm.string_or_empty(constant);
        return Ok(Some(enum_constant_not_present(vm, enum_class, &text)));
    }
    let type_name = vm
        .read_ref_field(reference, class, "typeName", "Ljava/lang/String;")
        .unwrap_or(ObjectRef::NULL);
    let text = vm.string_or_empty(type_name);
    Ok(Some(type_not_present(vm, &text)))
}

/// The exception for an unresolvable type referenced by an annotation value.
fn type_not_present(vm: &mut Vm<'_>, name: &str) -> VmError {
    vm.throw_new(
        "java/lang/TypeNotPresentException",
        Some(&name.replace('/', ".")),
    )
}

/// Materialise a nested annotation value, which must have a resolvable type.
fn materialize_required(
    vm: &mut Vm<'_>,
    owner: ClassId,
    annotation: &Annotation,
) -> Result<ObjectRef, VmError> {
    let name =
        annotation_type_name(&vm.classes.get(owner).constant_pool, annotation).unwrap_or_default();
    if annotation_class_of(vm, owner, annotation).is_none() {
        return Err(type_not_present(vm, &name));
    }
    materialize(vm, owner, annotation)
}

/// Convert one class-file element value into a VM value.
fn convert_value(
    vm: &mut Vm<'_>,
    owner: ClassId,
    descriptor: &str,
    value: &ElementValue,
    lazy: bool,
) -> Result<Value, VmError> {
    match value {
        ElementValue::Constant { tag, index } => {
            let (tag, index) = (*tag, *index);
            match tag {
                b's' => {
                    let text = string_constant(vm, owner, index);
                    Ok(Value::Ref(vm.make_string(&text)?))
                }
                b'B' | b'C' | b'I' | b'S' | b'Z' => {
                    let value = int_constant(vm, owner, index);
                    match descriptor {
                        "J" => Ok(Value::Long(value as i64)),
                        "F" => Ok(Value::Float(value as f32)),
                        "D" => Ok(Value::Double(value as f64)),
                        _ => Ok(Value::Int(value)),
                    }
                }
                b'J' => Ok(Value::Long(long_constant(vm, owner, index))),
                b'F' => Ok(Value::Float(float_constant(vm, owner, index))),
                b'D' => Ok(Value::Double(double_constant(vm, owner, index))),
                _ => Err(VmError::internal("bad annotation constant tag")),
            }
        }
        ElementValue::Enum {
            type_name_index,
            const_name_index,
        } => {
            let (type_descriptor, constant_name) = {
                let pool = &vm.classes.get(owner).constant_pool;
                (
                    pool.utf8(*type_name_index).unwrap_or_default().to_string(),
                    pool.utf8(*const_name_index).unwrap_or_default().to_string(),
                )
            };
            let Some(class_name) = object_descriptor_name(&type_descriptor) else {
                return Err(VmError::internal("bad enum descriptor"));
            };
            let Some(class) = vm.resolve_class_lenient(&class_name) else {
                return Err(type_not_present(vm, &class_name));
            };
            vm.ensure_initialized(class)?;
            match vm.read_static_ref(class, &constant_name, &type_descriptor) {
                Some(reference) if !reference.is_null() => Ok(Value::Ref(reference)),
                _ if lazy => Ok(Value::Ref(enum_error_proxy(vm, class, &constant_name)?)),
                _ => Err(enum_constant_not_present(vm, class, &constant_name)),
            }
        }
        ElementValue::Class(index) => {
            let descriptor = {
                let pool = &vm.classes.get(owner).constant_pool;
                pool.utf8(*index).unwrap_or_default().to_string()
            };
            match class_for_descriptor(vm, &descriptor) {
                Ok(class) => Ok(Value::Ref(vm.class_object(class)?)),
                Err(_) if lazy => {
                    let name = object_descriptor_name(&descriptor).unwrap_or(descriptor);
                    Ok(Value::Ref(type_error_proxy(vm, &name)?))
                }
                Err(_) => {
                    let name = object_descriptor_name(&descriptor).unwrap_or(descriptor);
                    Err(type_not_present(vm, &name))
                }
            }
        }
        ElementValue::Annotation(annotation) => {
            Ok(Value::Ref(materialize_required(vm, owner, annotation)?))
        }
        ElementValue::Array(values) => convert_array(vm, owner, descriptor, values, lazy),
    }
}

/// Convert the elements of an annotation array value.
fn convert_array(
    vm: &mut Vm<'_>,
    owner: ClassId,
    descriptor: &str,
    values: &[ElementValue],
    lazy: bool,
) -> Result<Value, VmError> {
    let component = descriptor.strip_prefix('[').unwrap_or(descriptor);
    if component.starts_with('L') || component.starts_with('[') {
        let component_class = class_for_descriptor(vm, component)?;
        let array = vm.allocate_object_array(component_class, values.len())?;
        vm.protect(array);
        let mut error = None;
        for (index, value) in values.iter().enumerate() {
            let converted = convert_value(vm, owner, component, value, lazy)?;
            if lazy && is_annotation_value_error(vm, converted)? {
                if error.is_none() {
                    error = Some(converted);
                }
                continue;
            }
            let boxed = box_annotation_value(vm, converted, component)?;
            vm.array_set_ref(array, index, boxed.as_ref())?;
        }
        vm.unprotect(array);
        if let Some(error) = error {
            return Ok(error);
        }
        Ok(Value::Ref(array))
    } else if let Some(component_type) = array_component(component) {
        let array = vm.allocate_array_of(component_type, values.len())?;
        for (index, value) in values.iter().enumerate() {
            let converted = convert_value(vm, owner, component, value, lazy)?;
            vm.array_set(array, index as i32, converted, false)?;
        }
        Ok(Value::Ref(array))
    } else {
        Err(VmError::internal("bad annotation array component"))
    }
}

fn string_constant(vm: &Vm<'_>, owner: ClassId, index: u16) -> String {
    vm.classes
        .get(owner)
        .constant_pool
        .utf8(index)
        .unwrap_or_default()
        .to_string()
}

fn int_constant(vm: &Vm<'_>, owner: ClassId, index: u16) -> i32 {
    match vm.classes.get(owner).constant_pool.get(index) {
        Ok(CpInfo::Integer(value)) => *value,
        _ => 0,
    }
}

fn long_constant(vm: &Vm<'_>, owner: ClassId, index: u16) -> i64 {
    match vm.classes.get(owner).constant_pool.get(index) {
        Ok(CpInfo::Long(value)) => *value,
        _ => 0,
    }
}

fn float_constant(vm: &Vm<'_>, owner: ClassId, index: u16) -> f32 {
    vm.classes
        .get(owner)
        .constant_pool
        .get(index)
        .ok()
        .and_then(CpInfo::as_f32)
        .unwrap_or(0.0)
}

fn double_constant(vm: &Vm<'_>, owner: ClassId, index: u16) -> f64 {
    vm.classes
        .get(owner)
        .constant_pool
        .get(index)
        .ok()
        .and_then(CpInfo::as_f64)
        .unwrap_or(0.0)
}

/// Materialise every resolvable annotation of an attributed structure.
fn materialize_all(
    vm: &mut Vm<'_>,
    owner: ClassId,
    annotations: &[Annotation],
) -> Result<Vec<ObjectRef>, VmError> {
    let mut result = Vec::with_capacity(annotations.len());
    for annotation in annotations {
        let instance = materialize(vm, owner, annotation)?;
        if !instance.is_null() {
            result.push(instance);
        }
    }
    Ok(result)
}

/// Find an annotation by type among raw annotations.
fn find_raw<'a>(
    vm: &Vm<'_>,
    owner: ClassId,
    annotations: &'a [Annotation],
    annotation_class: ClassId,
) -> Option<&'a Annotation> {
    let name = &vm.classes.get(annotation_class).name;
    let pool = &vm.classes.get(owner).constant_pool;
    annotations
        .iter()
        .find(|annotation| annotation_type_name(pool, annotation).as_deref() == Some(name.as_str()))
}

/// Build a `java.lang.annotation.Annotation[]`.
pub(crate) fn annotation_array(
    vm: &mut Vm<'_>,
    instances: &[ObjectRef],
) -> Result<ObjectRef, VmError> {
    let component = vm.resolve_class("java/lang/annotation/Annotation")?;
    let array = vm.allocate_object_array(component, instances.len())?;
    for (index, instance) in instances.iter().enumerate() {
        vm.array_set_ref(array, index, *instance)?;
    }
    Ok(array)
}

/// `Class.getDeclaredAnnotations()`.
pub(crate) fn class_declared_annotations(
    vm: &mut Vm<'_>,
    class: ClassId,
) -> Result<Vec<ObjectRef>, VmError> {
    let annotations = vm.classes.get(class).annotations.clone();
    materialize_all(vm, class, &annotations)
}

/// `Class.getDeclaredAnnotation(Class)`.
pub(crate) fn class_declared_annotation(
    vm: &mut Vm<'_>,
    class: ClassId,
    annotation_class: ClassId,
) -> Result<ObjectRef, VmError> {
    let annotations = vm.classes.get(class).annotations.clone();
    match find_raw(vm, class, &annotations, annotation_class) {
        Some(annotation) => materialize(vm, class, annotation),
        None => Ok(ObjectRef::NULL),
    }
}

/// `Class.getAnnotation(Class)`, including `@Inherited` annotations.
pub(crate) fn class_annotation(
    vm: &mut Vm<'_>,
    class: ClassId,
    annotation_class: ClassId,
) -> Result<ObjectRef, VmError> {
    let instance = class_declared_annotation(vm, class, annotation_class)?;
    if !instance.is_null() {
        return Ok(instance);
    }
    if !annotation_is_inherited(vm, annotation_class) {
        return Ok(ObjectRef::NULL);
    }
    let mut current = vm.classes.get(class).super_class;
    while let Some(candidate) = current {
        let instance = class_declared_annotation(vm, candidate, annotation_class)?;
        if !instance.is_null() {
            return Ok(instance);
        }
        current = vm.classes.get(candidate).super_class;
    }
    Ok(ObjectRef::NULL)
}

/// `Class.getAnnotations()`, including `@Inherited` annotations.
pub(crate) fn class_annotations(
    vm: &mut Vm<'_>,
    class: ClassId,
) -> Result<Vec<ObjectRef>, VmError> {
    let mut result = class_declared_annotations(vm, class)?;
    let mut seen: Vec<String> = result
        .iter()
        .map(|instance| vm.classes.get(vm.class_of(*instance)).name.clone())
        .collect();
    let mut current = vm.classes.get(class).super_class;
    while let Some(candidate) = current {
        let annotations: Vec<(String, Annotation)> = {
            let class_ref = vm.classes.get(candidate);
            class_ref
                .annotations
                .iter()
                .filter_map(|annotation| {
                    let name = annotation_type_name(&class_ref.constant_pool, annotation)?;
                    Some((name, annotation.clone()))
                })
                .collect()
        };
        for (name, annotation) in annotations {
            if seen.contains(&name) {
                continue;
            }
            let Some(annotation_class) = vm.resolve_class_lenient(&name) else {
                continue;
            };
            if !annotation_is_inherited(vm, annotation_class) {
                continue;
            }
            let instance = materialize(vm, candidate, &annotation)?;
            if !instance.is_null() {
                seen.push(name);
                result.push(instance);
            }
        }
        current = vm.classes.get(candidate).super_class;
    }
    Ok(result)
}

/// An array of the annotation type itself, used by the `*ByType` methods.
pub(crate) fn typed_annotation_array(
    vm: &mut Vm<'_>,
    annotation_class: ClassId,
    instances: &[ObjectRef],
) -> Result<ObjectRef, VmError> {
    let array = vm.allocate_object_array(annotation_class, instances.len())?;
    for (index, instance) in instances.iter().enumerate() {
        vm.array_set_ref(array, index, *instance)?;
    }
    Ok(array)
}

/// Arrange directly present and container-expanded annotations the way the JDK's
/// `getDirectlyAndIndirectlyPresent` does.
fn order_direct_and_indirect(
    direct: Option<ObjectRef>,
    indirect: Vec<ObjectRef>,
    container_before_containee: bool,
) -> Vec<ObjectRef> {
    let mut result = Vec::with_capacity(indirect.len() + 1);
    if direct.is_none() || container_before_containee {
        result.extend(indirect);
        if let Some(direct) = direct {
            result.push(direct);
        }
    } else {
        if let Some(direct) = direct {
            result.push(direct);
        }
        result.extend(indirect);
    }
    result
}

/// Expand the directly and indirectly (container) present annotations of one level.
fn declared_annotations_by_type(
    vm: &mut Vm<'_>,
    owner: ClassId,
    annotations: &[Annotation],
    annotation_class: ClassId,
) -> Result<Vec<ObjectRef>, VmError> {
    let container = repeatable_container(vm, annotation_class);
    let mut direct = None;
    let mut container_instance = None;
    let mut container_before_containee = false;
    for annotation in annotations {
        let Some(raw_type) = annotation_class_of(vm, owner, annotation) else {
            continue;
        };
        if raw_type == annotation_class {
            if direct.is_none() {
                let instance = materialize(vm, owner, annotation)?;
                if !instance.is_null() {
                    direct = Some(instance);
                }
            }
        } else if Some(raw_type) == container && container_instance.is_none() {
            let instance = materialize(vm, owner, annotation)?;
            if !instance.is_null() {
                container_instance = Some(instance);
            }
        }
    }
    if let Some(container) = container {
        for annotation in annotations {
            let Some(raw_type) = annotation_class_of(vm, owner, annotation) else {
                continue;
            };
            if raw_type == container {
                container_before_containee = true;
                break;
            }
            if raw_type == annotation_class {
                break;
            }
        }
    }
    let mut indirect = Vec::new();
    if let Some(instance) = container_instance {
        let expanded = container_array(vm, annotation_class, instance)?;
        let length = vm.array_length(expanded)?;
        for index in 0..length {
            indirect.push(vm.array_get(expanded, index as i32)?.as_ref());
        }
    }
    Ok(order_direct_and_indirect(
        direct,
        indirect,
        container_before_containee,
    ))
}

fn declared_annotations_by_type_array(
    vm: &mut Vm<'_>,
    owner: ClassId,
    annotations: &[Annotation],
    annotation_class: ClassId,
) -> Result<ObjectRef, VmError> {
    let instances = declared_annotations_by_type(vm, owner, annotations, annotation_class)?;
    typed_annotation_array(vm, annotation_class, &instances)
}

/// `Class.getDeclaredAnnotationsByType(Class)`.
pub(crate) fn class_declared_annotations_by_type(
    vm: &mut Vm<'_>,
    class: ClassId,
    annotation_class: ClassId,
) -> Result<ObjectRef, VmError> {
    let annotations = vm.classes.get(class).annotations.clone();
    declared_annotations_by_type_array(vm, class, &annotations, annotation_class)
}

/// `Class.getAnnotationsByType(Class)`.
pub(crate) fn class_annotations_by_type(
    vm: &mut Vm<'_>,
    class: ClassId,
    annotation_class: ClassId,
) -> Result<ObjectRef, VmError> {
    let mut current = Some(class);
    while let Some(level) = current {
        let annotations = vm.classes.get(level).annotations.clone();
        let instances = declared_annotations_by_type(vm, level, &annotations, annotation_class)?;
        if !instances.is_empty() {
            return typed_annotation_array(vm, annotation_class, &instances);
        }
        current = vm.classes.get(level).super_class;
        if current.is_some() && !annotation_is_inherited(vm, annotation_class) {
            break;
        }
    }
    typed_annotation_array(vm, annotation_class, &[])
}

/// `Method.getDeclaredAnnotations()`.
pub(crate) fn method_declared_annotations(
    vm: &mut Vm<'_>,
    owner: ClassId,
    slot: u32,
) -> Result<Vec<ObjectRef>, VmError> {
    let annotations = vm
        .classes
        .get(owner)
        .methods
        .get(slot as usize)
        .map(|method| method.annotations.clone())
        .unwrap_or_default();
    materialize_all(vm, owner, &annotations)
}

/// `Method.getAnnotation(Class)`.
pub(crate) fn method_annotation(
    vm: &mut Vm<'_>,
    owner: ClassId,
    slot: u32,
    annotation_class: ClassId,
) -> Result<ObjectRef, VmError> {
    let annotations = vm
        .classes
        .get(owner)
        .methods
        .get(slot as usize)
        .map(|method| method.annotations.clone())
        .unwrap_or_default();
    match find_raw(vm, owner, &annotations, annotation_class) {
        Some(annotation) => materialize(vm, owner, annotation),
        None => Ok(ObjectRef::NULL),
    }
}

/// `Method.getAnnotationsByType(Class)`.
pub(crate) fn method_annotations_by_type(
    vm: &mut Vm<'_>,
    owner: ClassId,
    slot: u32,
    annotation_class: ClassId,
) -> Result<ObjectRef, VmError> {
    let annotations = vm
        .classes
        .get(owner)
        .methods
        .get(slot as usize)
        .map(|method| method.annotations.clone())
        .unwrap_or_default();
    declared_annotations_by_type_array(vm, owner, &annotations, annotation_class)
}

/// `Method.getParameterAnnotations()`.
pub(crate) fn method_parameter_annotations(
    vm: &mut Vm<'_>,
    owner: ClassId,
    slot: u32,
) -> Result<ObjectRef, VmError> {
    let parameters = vm
        .classes
        .get(owner)
        .methods
        .get(slot as usize)
        .map(|method| method.parameter_annotations.clone())
        .unwrap_or_default();
    let annotation_component = vm.resolve_class("java/lang/annotation/Annotation")?;
    let array_component = vm.resolve_class("[Ljava/lang/annotation/Annotation;")?;
    let outer = vm.allocate_object_array(array_component, parameters.len())?;
    vm.protect(outer);
    for (index, annotations) in parameters.iter().enumerate() {
        let inner = vm.allocate_object_array(annotation_component, annotations.len())?;
        vm.protect(inner);
        for (position, annotation) in annotations.iter().enumerate() {
            let instance = materialize(vm, owner, annotation)?;
            if !instance.is_null() {
                vm.array_set_ref(inner, position, instance)?;
            }
        }
        vm.unprotect(inner);
        vm.array_set_ref(outer, index, inner)?;
    }
    vm.unprotect(outer);
    Ok(outer)
}

/// `Constructor.getParameterAnnotations()`, padded for implicit leading parameters.
pub(crate) fn constructor_parameter_annotations(
    vm: &mut Vm<'_>,
    owner: ClassId,
    slot: u32,
) -> Result<ObjectRef, VmError> {
    let result = method_parameter_annotations(vm, owner, slot)?;
    let (descriptor, is_member, is_static, is_enum) = {
        let entry = vm.classes.get(owner);
        (
            entry
                .methods
                .get(slot as usize)
                .map(|method| method.descriptor.clone())
                .unwrap_or_default(),
            entry.declaring_class_name.is_some(),
            entry.access_flags & ACC_STATIC != 0,
            entry.is_enum,
        )
    };
    let Ok(parsed) = MethodDescriptor::parse(&descriptor) else {
        return Ok(result);
    };
    let num_parameters = parsed.parameters.len();
    let result_length = vm.array_length(result)?;
    let shift = if result_length == num_parameters {
        0
    } else if is_enum
        && result_length + 2 == num_parameters
        && matches!(
            parsed.parameters.first(),
            Some(FieldType::Object(name)) if name == "java/lang/String"
        )
        && matches!(
            parsed.parameters.get(1),
            Some(FieldType::Base(BaseType::Int))
        )
    {
        2
    } else if is_member && !is_static && result_length + 1 == num_parameters {
        1
    } else {
        return Ok(result);
    };
    if shift == 0 {
        return Ok(result);
    }
    let component = vm.resolve_class("[Ljava/lang/annotation/Annotation;")?;
    let outer = vm.allocate_object_array(component, num_parameters)?;
    vm.protect(outer);
    let annotation_component = vm.resolve_class("java/lang/annotation/Annotation")?;
    let mut index = 0;
    while index < shift {
        let empty = vm.allocate_object_array(annotation_component, 0)?;
        vm.array_set_ref(outer, index, empty)?;
        index += 1;
    }
    index = 0;
    while index < result_length {
        let inner = vm.array_get(result, index as i32)?.as_ref();
        vm.array_set_ref(outer, index + shift, inner)?;
        index += 1;
    }
    vm.unprotect(outer);
    Ok(outer)
}

/// The runtime class of a materialised annotation instance.
pub(crate) fn annotation_type_of(vm: &mut Vm<'_>, instance: ObjectRef) -> Result<ClassId, VmError> {
    let (class, method) =
        vm.resolve_virtual_method(instance, "annotationType", "()Ljava/lang/Class;")?;
    let result = vm.invoke_method(class, method, alloc::vec![Value::Ref(instance)])?;
    crate::lang::class_argument(vm, result.as_ref())
}

/// `Method.getDefaultValue()`.
pub(crate) fn method_default_value(
    vm: &mut Vm<'_>,
    owner: ClassId,
    slot: u32,
) -> Result<ObjectRef, VmError> {
    let (default, descriptor) = vm
        .classes
        .get(owner)
        .methods
        .get(slot as usize)
        .map(|method| (method.annotation_default.clone(), method.descriptor.clone()))
        .unwrap_or((None, String::new()));
    let Some(default) = default else {
        return Ok(ObjectRef::NULL);
    };
    let return_type = descriptor.strip_prefix("()").unwrap_or(&descriptor);
    let value = convert_value(vm, owner, return_type, &default, false)?;
    let boxed = box_annotation_value(vm, value, return_type)?;
    Ok(boxed.as_ref())
}

/// `Field.getDeclaredAnnotations()`.
pub(crate) fn field_declared_annotations(
    vm: &mut Vm<'_>,
    owner: ClassId,
    slot: u32,
) -> Result<Vec<ObjectRef>, VmError> {
    let annotations = vm
        .classes
        .get(owner)
        .fields
        .get(slot as usize)
        .map(|field| field.annotations.clone())
        .unwrap_or_default();
    materialize_all(vm, owner, &annotations)
}

/// `Field.getAnnotation(Class)`.
pub(crate) fn field_annotation(
    vm: &mut Vm<'_>,
    owner: ClassId,
    slot: u32,
    annotation_class: ClassId,
) -> Result<ObjectRef, VmError> {
    let annotations = vm
        .classes
        .get(owner)
        .fields
        .get(slot as usize)
        .map(|field| field.annotations.clone())
        .unwrap_or_default();
    match find_raw(vm, owner, &annotations, annotation_class) {
        Some(annotation) => materialize(vm, owner, annotation),
        None => Ok(ObjectRef::NULL),
    }
}

/// `Field.getAnnotationsByType(Class)`.
pub(crate) fn field_annotations_by_type(
    vm: &mut Vm<'_>,
    owner: ClassId,
    slot: u32,
    annotation_class: ClassId,
) -> Result<ObjectRef, VmError> {
    let annotations = vm
        .classes
        .get(owner)
        .fields
        .get(slot as usize)
        .map(|field| field.annotations.clone())
        .unwrap_or_default();
    declared_annotations_by_type_array(vm, owner, &annotations, annotation_class)
}

/// The container annotation of a repeatable annotation type.
fn repeatable_container(vm: &mut Vm<'_>, annotation_class: ClassId) -> Option<ClassId> {
    let container_name = {
        let class = vm.classes.get(annotation_class);
        let repeatable = class.annotations.iter().find(|annotation| {
            annotation_type_name(&class.constant_pool, annotation).as_deref()
                == Some("java/lang/annotation/Repeatable")
        })?;
        let value = repeatable
            .elements
            .iter()
            .find(|pair| class.constant_pool.utf8(pair.name_index).ok() == Some("value"))?;
        let ElementValue::Class(index) = value.value else {
            return None;
        };
        let descriptor = class.constant_pool.utf8(index).ok()?;
        object_descriptor_name(descriptor)?
    };
    vm.resolve_class_lenient(&container_name)
}

/// The named element of a materialised annotation instance.
fn annotation_named_value(
    vm: &mut Vm<'_>,
    instance: ObjectRef,
    name: &str,
) -> Result<Option<Value>, VmError> {
    let class = vm.class_of(instance);
    let Some(values) = vm.read_ref_field(instance, class, VALUES_FIELD, VALUES_DESCRIPTOR) else {
        return Ok(None);
    };
    let position = element_methods(vm, class)
        .iter()
        .position(|element| element.name == name);
    match position {
        Some(position) => Ok(Some(vm.array_get(values, position as i32)?)),
        None => Ok(None),
    }
}

/// Whether an object is a materialised annotation instance.
pub(crate) fn is_annotation_instance(vm: &Vm<'_>, instance: ObjectRef) -> bool {
    !instance.is_null()
        && vm
            .read_ref_field(
                instance,
                vm.class_of(instance),
                VALUES_FIELD,
                VALUES_DESCRIPTOR,
            )
            .is_some()
}

/// Collect the annotations returned by a virtual `AnnotatedElement` method.
fn collect_annotations(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    method_name: &str,
) -> Result<Vec<ObjectRef>, VmError> {
    let (class, method) =
        vm.resolve_virtual_method(this, method_name, "()[Ljava/lang/annotation/Annotation;")?;
    let result = vm.invoke_method(class, method, alloc::vec![Value::Ref(this)])?;
    let array = result.as_ref();
    if array.is_null() {
        return Ok(Vec::new());
    }
    let length = vm.array_length(array)?;
    let mut annotations = Vec::with_capacity(length);
    for index in 0..length {
        annotations.push(vm.array_get(array, index as i32)?.as_ref());
    }
    Ok(annotations)
}

/// The direct-or-container scan shared by the `*ByType` defaults.
fn scan_by_type(
    vm: &mut Vm<'_>,
    annotations: &[ObjectRef],
    annotation_class: ClassId,
) -> Result<ObjectRef, VmError> {
    let direct = annotations
        .iter()
        .find(|annotation| vm.class_of(**annotation) == annotation_class)
        .copied();
    let container = repeatable_container(vm, annotation_class);
    let mut container_instance = None;
    let mut container_before_containee = false;
    if let Some(container_class) = container {
        container_instance = annotations
            .iter()
            .find(|annotation| vm.class_of(**annotation) == container_class)
            .copied();
        for annotation in annotations {
            let class = vm.class_of(*annotation);
            if class == container_class {
                container_before_containee = true;
                break;
            }
            if class == annotation_class {
                break;
            }
        }
    }
    let mut indirect = Vec::new();
    if let Some(instance) = container_instance {
        let expanded = container_array(vm, annotation_class, instance)?;
        let length = vm.array_length(expanded)?;
        for index in 0..length {
            indirect.push(vm.array_get(expanded, index as i32)?.as_ref());
        }
    }
    let instances = order_direct_and_indirect(direct, indirect, container_before_containee);
    typed_annotation_array(vm, annotation_class, &instances)
}

/// The default `AnnotatedElement.getDeclaredAnnotation`.
pub(crate) fn default_get_declared_annotation(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let annotation_class = crate::lang::class_argument(vm, ref_arg(args, 1))?;
    let annotations = collect_annotations(vm, this, "getDeclaredAnnotations")?;
    let found = annotations
        .iter()
        .find(|annotation| vm.class_of(**annotation) == annotation_class)
        .copied();
    object(found.unwrap_or(ObjectRef::NULL))
}

/// The default `AnnotatedElement.getDeclaredAnnotationsByType`.
pub(crate) fn default_get_declared_annotations_by_type(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let annotation_class = crate::lang::class_argument(vm, ref_arg(args, 1))?;
    let annotations = collect_annotations(vm, this, "getDeclaredAnnotations")?;
    object(scan_by_type(vm, &annotations, annotation_class)?)
}

/// The default `AnnotatedElement.getAnnotationsByType`.
pub(crate) fn default_get_annotations_by_type(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let annotation_class = crate::lang::class_argument(vm, ref_arg(args, 1))?;
    let (class, method) = vm.resolve_virtual_method(
        this,
        "getDeclaredAnnotationsByType",
        "(Ljava/lang/Class;)[Ljava/lang/annotation/Annotation;",
    )?;
    let declared = vm
        .invoke_method(class, method, alloc::vec![Value::Ref(this), args[1]])?
        .as_ref();
    let length = if declared.is_null() {
        0
    } else {
        vm.array_length(declared)?
    };
    if length > 0 {
        return object(declared);
    }
    // Inheritance applies to class elements carrying an `@Inherited` annotation type.
    let class_class = vm.resolve_class("java/lang/Class")?;
    if vm.class_of(this) == class_class && annotation_is_inherited(vm, annotation_class) {
        let (class, method) =
            vm.resolve_virtual_method(this, "getSuperclass", "()Ljava/lang/Class;")?;
        let superclass = vm
            .invoke_method(class, method, alloc::vec![Value::Ref(this)])?
            .as_ref();
        if !superclass.is_null() {
            let (class, method) = vm.resolve_virtual_method(
                superclass,
                "getAnnotationsByType",
                "(Ljava/lang/Class;)[Ljava/lang/annotation/Annotation;",
            )?;
            return vm.invoke_method(class, method, alloc::vec![Value::Ref(superclass), args[1]]);
        }
    }
    object(typed_annotation_array(vm, annotation_class, &[])?)
}

/// The default `AnnotatedElement.isAnnotationPresent`.
pub(crate) fn default_is_annotation_present(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    crate::lang::class_argument(vm, ref_arg(args, 1))?;
    let (class, method) = vm.resolve_virtual_method(
        this,
        "getAnnotation",
        "(Ljava/lang/Class;)Ljava/lang/annotation/Annotation;",
    )?;
    let annotation = vm.invoke_method(class, method, alloc::vec![Value::Ref(this), args[1]])?;
    Ok(Value::Int(i32::from(!annotation.as_ref().is_null())))
}

/// Java `String.hashCode` over UTF-16 code units.
fn java_string_hash(text: &str) -> i32 {
    let mut hash = 0i32;
    for unit in text.encode_utf16() {
        hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
    }
    hash
}

/// Whether two stored element values are equal per the annotation contract.
fn annotation_value_equals(
    vm: &mut Vm<'_>,
    descriptor: &str,
    a: Value,
    b: Value,
) -> Result<bool, VmError> {
    match descriptor.as_bytes().first().copied() {
        Some(b'[') => {
            let component = &descriptor[1..];
            let array_a = a.as_ref();
            let array_b = b.as_ref();
            if array_a.is_null() || array_b.is_null() {
                return Ok(array_a.is_null() && array_b.is_null());
            }
            let length_a = vm.array_length(array_a)?;
            let length_b = vm.array_length(array_b)?;
            if length_a != length_b {
                return Ok(false);
            }
            for index in 0..length_a {
                let value_a = vm.array_get(array_a, index as i32)?;
                let value_b = vm.array_get(array_b, index as i32)?;
                if !annotation_value_equals(vm, component, value_a, value_b)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Some(b'Z') | Some(b'B') | Some(b'C') | Some(b'S') | Some(b'I') => {
            let value_a = unbox_annotation_value(vm, a, descriptor);
            let value_b = unbox_annotation_value(vm, b, descriptor);
            Ok(value_a.as_int() == value_b.as_int())
        }
        Some(b'J') => {
            let value_a = unbox_annotation_value(vm, a, descriptor);
            let value_b = unbox_annotation_value(vm, b, descriptor);
            Ok(value_a.as_long() == value_b.as_long())
        }
        Some(b'F') => {
            let value_a = unbox_annotation_value(vm, a, descriptor);
            let value_b = unbox_annotation_value(vm, b, descriptor);
            Ok(value_a.as_float().to_bits() == value_b.as_float().to_bits())
        }
        Some(b'D') => {
            let value_a = unbox_annotation_value(vm, a, descriptor);
            let value_b = unbox_annotation_value(vm, b, descriptor);
            Ok(value_a.as_double().to_bits() == value_b.as_double().to_bits())
        }
        Some(b'L') => {
            let reference_a = a.as_ref();
            let reference_b = b.as_ref();
            if reference_a.is_null() || reference_b.is_null() {
                return Ok(reference_a.is_null() && reference_b.is_null());
            }
            if descriptor == "Ljava/lang/String;" {
                return Ok(vm.string_value(reference_a) == vm.string_value(reference_b));
            }
            if is_annotation_instance(vm, reference_a) || is_annotation_instance(vm, reference_b) {
                return annotation_equals(vm, reference_a, reference_b);
            }
            Ok(reference_a == reference_b)
        }
        _ => Ok(false),
    }
}

/// The concise identity of a heap object, for hashing annotation members.
fn object_identity(vm: &Vm<'_>, reference: ObjectRef) -> i32 {
    vm.heap.get(reference).map_or(0, |object| object.hash)
}

/// The hash of one stored element value.
fn annotation_value_hash(vm: &mut Vm<'_>, descriptor: &str, value: Value) -> Result<i32, VmError> {
    match descriptor.as_bytes().first().copied() {
        Some(b'[') => {
            let component = &descriptor[1..];
            let array = value.as_ref();
            if array.is_null() {
                return Ok(0);
            }
            let length = vm.array_length(array)?;
            let mut hash = 1i32;
            for index in 0..length {
                let element = vm.array_get(array, index as i32)?;
                hash = hash
                    .wrapping_mul(31)
                    .wrapping_add(annotation_value_hash(vm, component, element)?);
            }
            Ok(hash)
        }
        Some(b'Z') => {
            let value = unbox_annotation_value(vm, value, descriptor);
            Ok(if value.as_int() != 0 { 1231 } else { 1237 })
        }
        Some(b'B') | Some(b'C') | Some(b'S') | Some(b'I') => {
            let value = unbox_annotation_value(vm, value, descriptor);
            Ok(value.as_int())
        }
        Some(b'J') => {
            let value = unbox_annotation_value(vm, value, descriptor);
            let long = value.as_long();
            Ok((long ^ (long >> 32)) as i32)
        }
        Some(b'F') => {
            let value = unbox_annotation_value(vm, value, descriptor);
            Ok(value.as_float().to_bits() as i32)
        }
        Some(b'D') => {
            let value = unbox_annotation_value(vm, value, descriptor);
            let bits = value.as_double().to_bits();
            Ok((bits ^ (bits >> 32)) as i32)
        }
        Some(b'L') => {
            let reference = value.as_ref();
            if reference.is_null() {
                return Ok(0);
            }
            if descriptor == "Ljava/lang/String;" {
                return Ok(vm
                    .string_value(reference)
                    .map_or(0, |text| java_string_hash(&text)));
            }
            if is_annotation_instance(vm, reference) {
                return annotation_hash_code(vm, reference);
            }
            Ok(object_identity(vm, reference))
        }
        _ => Ok(0),
    }
}

/// The value-based `equals` of two annotation instances.
pub(crate) fn annotation_equals(
    vm: &mut Vm<'_>,
    a: ObjectRef,
    b: ObjectRef,
) -> Result<bool, VmError> {
    if a == b {
        return Ok(true);
    }
    if a.is_null() || b.is_null() {
        return Ok(false);
    }
    let class = vm.class_of(a);
    if class != vm.class_of(b) || !is_annotation_instance(vm, b) {
        return Ok(false);
    }
    let elements = element_methods(vm, class);
    let values_a = vm
        .read_ref_field(a, class, VALUES_FIELD, VALUES_DESCRIPTOR)
        .unwrap_or(ObjectRef::NULL);
    let values_b = vm
        .read_ref_field(b, class, VALUES_FIELD, VALUES_DESCRIPTOR)
        .unwrap_or(ObjectRef::NULL);
    for (position, element) in elements.iter().enumerate() {
        let value_a = vm.array_get(values_a, position as i32)?;
        let value_b = vm.array_get(values_b, position as i32)?;
        if !annotation_value_equals(vm, &element.descriptor, value_a, value_b)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The `hashCode` of an annotation instance.
pub(crate) fn annotation_hash_code(vm: &mut Vm<'_>, instance: ObjectRef) -> Result<i32, VmError> {
    let class = vm.class_of(instance);
    let elements = element_methods(vm, class);
    let values = vm
        .read_ref_field(instance, class, VALUES_FIELD, VALUES_DESCRIPTOR)
        .unwrap_or(ObjectRef::NULL);
    let mut result = 0i32;
    for (position, element) in elements.iter().enumerate() {
        let value = vm.array_get(values, position as i32)?;
        let name_hash = java_string_hash(&element.name);
        let value_hash = annotation_value_hash(vm, &element.descriptor, value)?;
        result = result.wrapping_add(127i32.wrapping_mul(name_hash) ^ value_hash);
    }
    Ok(result)
}

/// The `toString` of an annotation instance, in the JDK's format.
pub(crate) fn annotation_to_string(
    vm: &mut Vm<'_>,
    instance: ObjectRef,
) -> Result<String, VmError> {
    let class = vm.class_of(instance);
    let type_name = crate::lang::canonical_name(vm, class)
        .unwrap_or_else(|| String::from("<no canonical name>"));
    let elements = element_methods(vm, class);
    let values = vm
        .read_ref_field(instance, class, VALUES_FIELD, VALUES_DESCRIPTOR)
        .unwrap_or(ObjectRef::NULL);
    let mut parts = Vec::with_capacity(elements.len());
    for (position, element) in elements.iter().enumerate() {
        let value = vm.array_get(values, position as i32)?;
        let text = annotation_value_text(vm, &element.descriptor, value)?;
        if elements.len() == 1 && element.name == "value" {
            parts.push(text);
        } else {
            parts.push(alloc::format!("{}={text}", element.name));
        }
    }
    Ok(alloc::format!("@{type_name}({})", parts.join(", ")))
}

/// Escape one character as the JDK's annotation source formatting does.
fn quote_char(character: char, char_context: bool) -> String {
    match character {
        '\u{8}' => "\\b".into(),
        '\u{c}' => "\\f".into(),
        '\n' => "\\n".into(),
        '\r' => "\\r".into(),
        '\t' => "\\t".into(),
        '\'' if char_context => "\\'".into(),
        '\"' if !char_context => "\\\"".into(),
        '\\' => "\\\\".into(),
        other if (' '..='~').contains(&other) => other.to_string(),
        other => alloc::format!("\\u{:04x}", other as u32),
    }
}

fn quoted_string(text: &str) -> String {
    let mut result = String::from("\"");
    for character in text.chars() {
        result.push_str(&quote_char(character, false));
    }
    result.push('"');
    result
}

fn quoted_char(character: char) -> String {
    alloc::format!("'{}'", quote_char(character, true))
}

fn float_source(value: f32) -> String {
    if value.is_finite() {
        alloc::format!("{}f", oxjvm_vm::format::float_to_string(value))
    } else if value.is_infinite() {
        if value < 0.0 {
            "-1.0f/0.0f".into()
        } else {
            "1.0f/0.0f".into()
        }
    } else {
        "0.0f/0.0f".into()
    }
}

fn double_source(value: f64) -> String {
    if value.is_finite() {
        oxjvm_vm::format::double_to_string(value)
    } else if value.is_infinite() {
        if value < 0.0 {
            "-1.0/0.0".into()
        } else {
            "1.0/0.0".into()
        }
    } else {
        "0.0/0.0".into()
    }
}

/// The JDK's source-like rendering of one annotation value.
fn annotation_value_text(
    vm: &mut Vm<'_>,
    descriptor: &str,
    value: Value,
) -> Result<String, VmError> {
    if let Some(text) = annotation_value_error_text(vm, value)? {
        return Ok(text);
    }
    match descriptor.as_bytes().first().copied() {
        Some(b'[') => {
            let component = &descriptor[1..];
            let array = value.as_ref();
            if array.is_null() {
                return Ok("null".into());
            }
            let length = vm.array_length(array)?;
            let mut parts = Vec::with_capacity(length);
            for index in 0..length {
                let element = vm.array_get(array, index as i32)?;
                parts.push(annotation_value_text(vm, component, element)?);
            }
            Ok(alloc::format!("{{{}}}", parts.join(", ")))
        }
        Some(b'Z') => {
            let value = unbox_annotation_value(vm, value, descriptor);
            Ok(if value.as_int() != 0 { "true" } else { "false" }.into())
        }
        Some(b'B') => {
            let value = unbox_annotation_value(vm, value, descriptor);
            Ok(alloc::format!("(byte)0x{:02x}", value.as_int() as u8))
        }
        Some(b'S') | Some(b'I') => {
            let value = unbox_annotation_value(vm, value, descriptor);
            Ok(alloc::format!("{}", value.as_int()))
        }
        Some(b'C') => {
            let value = unbox_annotation_value(vm, value, descriptor);
            let character = char::from_u32(value.as_int() as u32).unwrap_or('\u{FFFD}');
            Ok(quoted_char(character))
        }
        Some(b'J') => {
            let value = unbox_annotation_value(vm, value, descriptor);
            Ok(alloc::format!("{}L", value.as_long()))
        }
        Some(b'F') => {
            let value = unbox_annotation_value(vm, value, descriptor);
            Ok(float_source(value.as_float()))
        }
        Some(b'D') => {
            let value = unbox_annotation_value(vm, value, descriptor);
            Ok(double_source(value.as_double()))
        }
        Some(b'L') => {
            let reference = value.as_ref();
            if reference.is_null() {
                return Ok("null".into());
            }
            if descriptor == "Ljava/lang/String;" {
                return Ok(quoted_string(
                    &vm.string_value(reference).unwrap_or_default(),
                ));
            }
            if is_annotation_instance(vm, reference) {
                return annotation_to_string(vm, reference);
            }
            let class_class = vm.resolve_class("java/lang/Class")?;
            if vm.class_of(reference) == class_class {
                let target = crate::lang::class_argument(vm, reference)?;
                let name = crate::lang::canonical_name(vm, target)
                    .unwrap_or_else(|| String::from("<no canonical name>"));
                return Ok(alloc::format!("{name}.class"));
            }
            let enum_class = vm.resolve_class("java/lang/Enum")?;
            let (method_name, method_descriptor) =
                if vm.class_is_subtype(vm.class_of(reference), enum_class) {
                    ("name", "()Ljava/lang/String;")
                } else {
                    ("toString", "()Ljava/lang/String;")
                };
            let (class, method) =
                vm.resolve_virtual_method(reference, method_name, method_descriptor)?;
            let result = vm.invoke_method(class, method, alloc::vec![Value::Ref(reference)])?;
            let text = result.as_ref();
            if text.is_null() {
                return Ok("null".into());
            }
            Ok(vm.string_value(text).unwrap_or_default())
        }
        _ => Ok("null".into()),
    }
}

/// The container's `value` array, typed as `annotation_class[]`.
fn container_array(
    vm: &mut Vm<'_>,
    annotation_class: ClassId,
    instance: ObjectRef,
) -> Result<ObjectRef, VmError> {
    if instance.is_null() {
        return typed_annotation_array(vm, annotation_class, &[]);
    }
    match annotation_named_value(vm, instance, "value")? {
        Some(Value::Ref(array)) if !array.is_null() => Ok(array),
        _ => typed_annotation_array(vm, annotation_class, &[]),
    }
}
