//! Minimal `java.lang.reflect`: `Method`, `Field`, `Constructor`, and the
//! `Class` lookups.
//!
//! Members are views into the VM's class metadata: each holds its declaring
//! `Class` and the method/field-table slot.  Runtime-visible annotations are
//! materialised by [`crate::annotation`]; `setAccessible` is a no-op
//! (everything is accessible).

use alloc::boxed::Box;
use alloc::string::{String, ToString};

use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_classfile::{FieldType, MethodDescriptor};

use crate::invoke::class_field_descriptor;
use crate::lang::class_argument;
use crate::{
    boolean, class, double, field, float, int, long, method, object, receiver, ref_arg, string_arg,
    void,
};
use oxjvm_vm::{ClassId, ObjectRef, Value, Vm, VmError};

// -------------------------------------------------------------------------------------------
// Shared helpers
// -------------------------------------------------------------------------------------------

pub(crate) fn field_type_descriptor(field_type: &FieldType) -> String {
    match field_type {
        FieldType::Base(base) => String::from(base.as_char()),
        FieldType::Object(name) => alloc::format!("L{name};"),
        FieldType::Array(inner) => alloc::format!("[{}", field_type_descriptor(inner)),
    }
}

/// Resolve the `Class` object for a descriptor field type.
fn field_type_class(vm: &mut Vm<'_>, field_type: &FieldType) -> Result<ClassId, VmError> {
    match field_type {
        FieldType::Base(base) => vm.primitive_class(*base),
        FieldType::Object(name) => vm.resolve_class(name),
        FieldType::Array(_) => vm.resolve_class(&field_type_descriptor(field_type)),
    }
}

/// The declared type of a field table entry, as a `Class`.
fn field_declared_type(vm: &mut Vm<'_>, class: ClassId, slot: u32) -> Result<ClassId, VmError> {
    let descriptor = vm.classes.get(class).fields[slot as usize]
        .descriptor
        .clone();
    let field_type =
        FieldType::parse(&descriptor).map_err(|_| VmError::internal("bad field descriptor"))?;
    field_type_class(vm, &field_type)
}

/// The parameter classes of an executable descriptor.
fn executable_parameter_types(vm: &mut Vm<'_>, descriptor: &str) -> Result<Vec<ClassId>, VmError> {
    let parsed = MethodDescriptor::parse(descriptor)
        .map_err(|_| VmError::internal("bad executable descriptor"))?;
    let mut classes = Vec::new();
    for parameter in &parsed.parameters {
        classes.push(field_type_class(vm, parameter)?);
    }
    Ok(classes)
}

/// The return class of a method descriptor (`void` when there is none).
fn executable_return_type(vm: &mut Vm<'_>, descriptor: &str) -> Result<ClassId, VmError> {
    let parsed = MethodDescriptor::parse(descriptor)
        .map_err(|_| VmError::internal("bad method descriptor"))?;
    match parsed.returns {
        Some(returns) => field_type_class(vm, &returns),
        None => vm.primitive_class(oxjvm_classfile::BaseType::Void),
    }
}

/// A `Type[]` holding the given classes.  Raw classes stand in for generic types until generic
/// signatures are modelled.
fn type_array_of(vm: &mut Vm<'_>, classes: &[ClassId]) -> Result<Value, VmError> {
    let type_class = vm.resolve_class("java/lang/reflect/Type")?;
    let array = vm.allocate_object_array(type_class, classes.len())?;
    vm.protect(array);
    for (index, class) in classes.iter().enumerate() {
        let class_object = vm.class_object(*class)?;
        vm.array_set_ref(array, index, class_object)?;
    }
    object(array)
}

fn primitive_descriptor(descriptor: &str) -> bool {
    matches!(descriptor, "Z" | "B" | "C" | "S" | "I" | "J" | "F" | "D")
}

fn method_entry(vm: &Vm<'_>, class: ClassId, slot: u32) -> (String, String, u16) {
    let entry = &vm.classes.get(class).methods[slot as usize];
    (
        entry.name.clone(),
        entry.descriptor.clone(),
        entry.access_flags,
    )
}

fn field_entry(vm: &Vm<'_>, class: ClassId, slot: u32) -> (String, String, u16) {
    let entry = &vm.classes.get(class).fields[slot as usize];
    (
        entry.name.clone(),
        entry.descriptor.clone(),
        entry.access_flags,
    )
}

/// The storage slot (instance layout slot / `static_values` index) of a field
/// metadata entry.
fn field_storage_slot(vm: &Vm<'_>, class: ClassId, index: u32) -> u32 {
    u32::from(vm.classes.get(class).fields[index as usize].slot)
}

pub(crate) fn new_method(vm: &mut Vm<'_>, class: ClassId, slot: u32) -> Result<ObjectRef, VmError> {
    let method_class = vm.resolve_class("java/lang/reflect/Method")?;
    let instance = vm.new_instance(method_class)?;
    let class_object = vm.class_object(class)?;
    let (declaring, field) = vm.find_field(method_class, "clazz", "Ljava/lang/Class;")?;
    vm.set_instance_ref(instance, declaring, field, class_object);
    if let Some((declaring, field)) = vm.find_field_opt(method_class, "slot", "I") {
        vm.set_instance_int(instance, declaring, field, slot as i32);
    }
    Ok(instance)
}

fn new_field(vm: &mut Vm<'_>, class: ClassId, slot: u32) -> Result<ObjectRef, VmError> {
    let field_class = vm.resolve_class("java/lang/reflect/Field")?;
    let instance = vm.new_instance(field_class)?;
    let class_object = vm.class_object(class)?;
    let (declaring, field) = vm.find_field(field_class, "clazz", "Ljava/lang/Class;")?;
    vm.set_instance_ref(instance, declaring, field, class_object);
    if let Some((declaring, field)) = vm.find_field_opt(field_class, "slot", "I") {
        vm.set_instance_int(instance, declaring, field, slot as i32);
    }
    Ok(instance)
}

fn method_parts(vm: &Vm<'_>, this: ObjectRef) -> (ClassId, u32) {
    let class = class_object_target(vm, this);
    let slot = crate::lang::read_int_field(vm, this, "slot").max(0) as u32;
    (class, slot)
}

/// The class denoted by the `clazz` field of a Method/Field object.
pub(crate) fn class_object_target(vm: &Vm<'_>, this: ObjectRef) -> ClassId {
    let reference = vm
        .read_ref_field(this, vm.class_of(this), "clazz", "Ljava/lang/Class;")
        .unwrap_or(ObjectRef::NULL);
    match vm.heap.get(reference).map(|object| &object.data) {
        Some(oxjvm_vm::ObjectData::Class(class)) => *class,
        _ => ClassId::OBJECT,
    }
}

fn instance_value(vm: &Vm<'_>, object: ObjectRef, slot: u32) -> Option<Value> {
    match vm.heap.get(object).map(|object| &object.data) {
        Some(oxjvm_vm::ObjectData::Instance(fields)) => fields.get(slot as usize).cloned(),
        _ => None,
    }
}

fn set_instance_value(
    vm: &mut Vm<'_>,
    object: ObjectRef,
    slot: u32,
    value: Value,
) -> Result<(), VmError> {
    match vm.heap.get_mut(object).map(|object| &mut object.data) {
        Some(oxjvm_vm::ObjectData::Instance(fields)) if (slot as usize) < fields.len() => {
            fields[slot as usize] = value;
            Ok(())
        }
        _ => Err(vm.throw_new("java/lang/IllegalArgumentException", None)),
    }
}

// -------------------------------------------------------------------------------------------
// Method
// -------------------------------------------------------------------------------------------

const METHOD_METHODS: [oxjvm_vm::NativeMethodDef; 27] = [
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = method_parts(vm, receiver(args));
            let (name, _, _) = method_entry(vm, class, slot);
            object(vm.make_string(&name)?)
        },
    ),
    method(
        "getDeclaringClass",
        "()Ljava/lang/Class;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, _) = method_parts(vm, receiver(args));
            object(vm.class_object(class)?)
        },
    ),
    method("getModifiers", "()I", ACC_PUBLIC, |vm, _, args| {
        let (class, slot) = method_parts(vm, receiver(args));
        let (_, _, flags) = method_entry(vm, class, slot);
        int(i32::from(flags))
    }),
    method(
        "getParameterTypes",
        "()[Ljava/lang/Class;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = method_parts(vm, receiver(args));
            let (_, descriptor, _) = method_entry(vm, class, slot);
            let parsed = MethodDescriptor::parse(&descriptor)
                .map_err(|_| VmError::internal("bad method descriptor"))?;
            let mut classes = Vec::new();
            for parameter in &parsed.parameters {
                classes.push(field_type_class(vm, parameter)?);
            }
            let class_class = vm.resolve_class("java/lang/Class")?;
            let array = vm.allocate_object_array(class_class, classes.len())?;
            for (index, parameter) in classes.iter().enumerate() {
                let object = vm.class_object(*parameter)?;
                vm.array_set_ref(array, index, object)?;
            }
            object(array)
        },
    ),
    method(
        "getReturnType",
        "()Ljava/lang/Class;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = method_parts(vm, receiver(args));
            let (_, descriptor, _) = method_entry(vm, class, slot);
            let parsed = MethodDescriptor::parse(&descriptor)
                .map_err(|_| VmError::internal("bad method descriptor"))?;
            let Some(returns) = parsed.returns else {
                return object(
                    vm.class_object(vm.primitive_class(oxjvm_classfile::BaseType::Void)?)?,
                );
            };
            let class_id = field_type_class(vm, &returns)?;
            object(vm.class_object(class_id)?)
        },
    ),
    method(
        "getGenericReturnType",
        "()Ljava/lang/reflect/Type;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = method_parts(vm, receiver(args));
            generic_return_type(vm, class, slot)
        },
    ),
    method(
        "getGenericParameterTypes",
        "()[Ljava/lang/reflect/Type;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = method_parts(vm, receiver(args));
            generic_parameter_types(vm, class, slot)
        },
    ),
    method(
        "getGenericExceptionTypes",
        "()[Ljava/lang/reflect/Type;",
        ACC_PUBLIC,
        |vm, _, args| {
            let _ = method_parts(vm, receiver(args));
            type_array_of(vm, &[])
        },
    ),
    method("getParameterCount", "()I", ACC_PUBLIC, |vm, _, args| {
        let (class, slot) = method_parts(vm, receiver(args));
        let (_, descriptor, _) = method_entry(vm, class, slot);
        let parsed = MethodDescriptor::parse(&descriptor)
            .map_err(|_| VmError::internal("bad method descriptor"))?;
        int(parsed.parameters.len() as i32)
    }),
    method(
        "invoke",
        "(Ljava/lang/Object;[Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        reflect_invoke,
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null()
                || vm.class_name(vm.class_of(receiver(args))) != "java/lang/reflect/Method"
                || vm.class_of(receiver(args)) != vm.class_of(other)
            {
                return boolean(false);
            }
            boolean(method_parts(vm, receiver(args)) == method_parts(vm, other))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let (class, slot) = method_parts(vm, receiver(args));
        int((class.raw() as i32)
            .wrapping_mul(31)
            .wrapping_add(slot as i32))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = method_parts(vm, receiver(args));
            {
                let text = method_signature_text(vm, class, slot)?;
                object(vm.make_string(&text)?)
            }
        },
    ),
    method(
        "toGenericString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = method_parts(vm, receiver(args));
            {
                let text = method_signature_text(vm, class, slot)?;
                object(vm.make_string(&text)?)
            }
        },
    ),
    method(
        "getAnnotation",
        "(Ljava/lang/Class;)Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = method_parts(vm, receiver(args));
            let annotation = class_argument(vm, ref_arg(args, 1))?;
            object(crate::annotation::method_annotation(
                vm, class, slot, annotation,
            )?)
        },
    ),
    method(
        "getDeclaredAnnotation",
        "(Ljava/lang/Class;)Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = method_parts(vm, receiver(args));
            let annotation = class_argument(vm, ref_arg(args, 1))?;
            object(crate::annotation::method_annotation(
                vm, class, slot, annotation,
            )?)
        },
    ),
    method(
        "getDeclaredAnnotations",
        "()[Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = method_parts(vm, receiver(args));
            let annotations = crate::annotation::method_declared_annotations(vm, class, slot)?;
            object(crate::annotation::annotation_array(vm, &annotations)?)
        },
    ),
    method(
        "getAnnotations",
        "()[Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = method_parts(vm, receiver(args));
            let annotations = crate::annotation::method_declared_annotations(vm, class, slot)?;
            object(crate::annotation::annotation_array(vm, &annotations)?)
        },
    ),
    method(
        "getAnnotationsByType",
        "(Ljava/lang/Class;)[Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = method_parts(vm, receiver(args));
            let annotation = class_argument(vm, ref_arg(args, 1))?;
            object(crate::annotation::method_annotations_by_type(
                vm, class, slot, annotation,
            )?)
        },
    ),
    method(
        "isAnnotationPresent",
        "(Ljava/lang/Class;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = method_parts(vm, receiver(args));
            let annotation = class_argument(vm, ref_arg(args, 1))?;
            let instance = crate::annotation::method_annotation(vm, class, slot, annotation)?;
            boolean(!instance.is_null())
        },
    ),
    method(
        "getDefaultValue",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = method_parts(vm, receiver(args));
            object(crate::annotation::method_default_value(vm, class, slot)?)
        },
    ),
    method(
        "getParameterAnnotations",
        "()[[Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = method_parts(vm, receiver(args));
            object(crate::annotation::method_parameter_annotations(
                vm, class, slot,
            )?)
        },
    ),
    method(
        "getParameters",
        "()[Ljava/lang/reflect/Parameter;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (class, slot) = method_parts(vm, this);
            let (_, descriptor, _) = method_entry(vm, class, slot);
            executable_parameters(vm, this, &descriptor)
        },
    ),
    method("isDefault", "()Z", ACC_PUBLIC, |vm, _, args| {
        let (class, slot) = method_parts(vm, receiver(args));
        let flags = vm.classes.get(class).methods[slot as usize].access_flags;
        boolean(
            vm.classes.get(class).is_interface()
                && flags & (ACC_ABSTRACT | ACC_STATIC | ACC_PRIVATE) == 0,
        )
    }),
    method("isSynthetic", "()Z", ACC_PUBLIC, |vm, _, args| {
        let (class, slot) = method_parts(vm, receiver(args));
        let flags = vm.classes.get(class).methods[slot as usize].access_flags;
        boolean(flags & ACC_SYNTHETIC != 0)
    }),
    method("isBridge", "()Z", ACC_PUBLIC, |vm, _, args| {
        let (class, slot) = method_parts(vm, receiver(args));
        let flags = vm.classes.get(class).methods[slot as usize].access_flags;
        boolean(flags & ACC_BRIDGE != 0)
    }),
    method("isVarArgs", "()Z", ACC_PUBLIC, |vm, _, args| {
        let (class, slot) = method_parts(vm, receiver(args));
        let flags = vm.classes.get(class).methods[slot as usize].access_flags;
        boolean(flags & ACC_VARARGS != 0)
    }),
];

fn reflect_invoke(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let (class, slot) = method_parts(vm, receiver(args));
    let (_name, descriptor, flags) = method_entry(vm, class, slot);
    if flags & ACC_PRIVATE != 0 && !reflection_override(vm, receiver(args)) && !same_nest(vm, class)
    {
        return Err(vm.throw_new("java/lang/IllegalAccessException", None));
    }
    let parsed = MethodDescriptor::parse(&descriptor)
        .map_err(|_| VmError::internal("bad method descriptor"))?;
    let mut call_args = Vec::new();
    if flags & ACC_STATIC == 0 {
        let target = ref_arg(args, 1);
        if target.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        call_args.push(Value::Ref(target));
    }
    let arguments = ref_arg(args, 2);
    for (index, parameter) in parsed.parameters.iter().enumerate() {
        let value = vm.array_get(arguments, index as i32)?;
        call_args.push(crate::boxed::unbox_value(
            vm,
            value.as_ref(),
            &field_type_descriptor(parameter),
        ));
    }
    let result = match vm.invoke_method(class, slot, call_args) {
        Ok(value) => value,
        // `Method.invoke` wraps every exception the target throws.
        Err(VmError::Thrown(target)) => return Err(new_invocation_target(vm, target)),
        Err(other) => return Err(other),
    };
    let Some(returns) = parsed.returns else {
        return object(ObjectRef::NULL);
    };

    crate::boxed::box_value(vm, result, &field_type_descriptor(&returns))
}

/// Build a `java.lang.reflect.InvocationTargetException` around `target`.
fn new_invocation_target(vm: &mut Vm<'_>, target: ObjectRef) -> VmError {
    let result = (|| -> Result<ObjectRef, VmError> {
        let class = vm.resolve_class("java/lang/reflect/InvocationTargetException")?;
        let exception = vm.new_instance(class)?;
        let (declaring, method) = vm
            .find_method(class, "<init>", "(Ljava/lang/Throwable;)V")
            .ok_or_else(|| VmError::internal("missing InvocationTargetException constructor"))?;
        vm.invoke_method(
            declaring,
            method,
            alloc::vec![Value::Ref(exception), Value::Ref(target)],
        )?;
        Ok(exception)
    })();
    match result {
        Ok(exception) => VmError::Thrown(exception),
        Err(other) => other,
    }
}

pub(crate) const EXECUTABLE: oxjvm_vm::NativeClass = class(
    "java/lang/reflect/Executable",
    Some("java/lang/reflect/AccessibleObject"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &METHOD_METHODS,
    None,
);

pub(crate) const METHOD: oxjvm_vm::NativeClass = class(
    "java/lang/reflect/Method",
    Some("java/lang/reflect/Executable"),
    &[
        "java/lang/reflect/Member",
        "java/lang/reflect/AnnotatedElement",
    ],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("clazz", "Ljava/lang/Class;", ACC_PRIVATE, None),
        field("slot", "I", ACC_PRIVATE, None),
        field("override", "Z", ACC_PRIVATE, None),
    ],
    &METHOD_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Field
// -------------------------------------------------------------------------------------------

fn field_value(vm: &mut Vm<'_>, this: ObjectRef, target: ObjectRef) -> Result<Value, VmError> {
    let (class, index) = field_parts(vm, this);
    let storage = field_storage_slot(vm, class, index);
    let (_, _, flags) = field_entry(vm, class, index);
    if flags & ACC_STATIC != 0 {
        vm.ensure_initialized(class)?;
        return Ok(vm.classes.get(class).static_values[storage as usize]);
    }
    if target.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    instance_value(vm, target, storage)
        .ok_or_else(|| vm.throw_new("java/lang/IllegalArgumentException", None))
}

fn field_parts(vm: &Vm<'_>, this: ObjectRef) -> (ClassId, u32) {
    let class = class_object_target(vm, this);
    let slot = crate::lang::read_int_field(vm, this, "slot").max(0) as u32;
    (class, slot)
}

fn reflect_field_get(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let _ = field_parts(vm, this);
    let value = field_value(vm, this, ref_arg(args, 1))?;
    let (class, slot) = field_parts(vm, this);
    let (_, descriptor, _) = field_entry(vm, class, slot);
    if primitive_descriptor(&descriptor) {
        return crate::boxed::box_value(vm, value, &descriptor);
    }
    Ok(value)
}

/// Whether `setAccessible(true)` was requested for a reflected member.
fn reflection_override(vm: &Vm<'_>, this: ObjectRef) -> bool {
    let class = vm.class_of(this);
    vm.read_instance_int_named(this, class, "override", "Z")
        .unwrap_or(0)
        != 0
}

/// Whether the current Java caller and `declaring` belong to the same nest.
fn same_nest(vm: &Vm<'_>, declaring: ClassId) -> bool {
    vm.caller_class()
        .is_some_and(|caller| vm.nest_host(caller) == vm.nest_host(declaring))
}

/// Reject writes to final fields, mirroring the JDK's reflection rules.
fn check_field_writable(vm: &mut Vm<'_>, this: ObjectRef, flags: u16) -> Result<(), VmError> {
    if flags & ACC_FINAL == 0 {
        return Ok(());
    }
    if reflection_override(vm, this) && flags & ACC_STATIC == 0 {
        return Ok(());
    }
    Err(vm.throw_new("java/lang/IllegalAccessException", None))
}

fn set_accessible(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let flag = crate::int_arg(args, 1) != 0;
    let class = vm.class_of(this);
    if let Some((declaring, field)) = vm.find_field_opt(class, "override", "Z") {
        vm.set_instance_int(this, declaring, field, i32::from(flag));
    }
    void()
}

fn reflect_field_set(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let target = ref_arg(args, 1);
    let (class, index) = field_parts(vm, this);
    let storage = field_storage_slot(vm, class, index);
    let (_, descriptor, flags) = field_entry(vm, class, index);
    check_field_writable(vm, this, flags)?;
    let value = if primitive_descriptor(&descriptor) {
        crate::boxed::unbox_value(vm, ref_arg(args, 2), &descriptor)
    } else {
        Value::Ref(ref_arg(args, 2))
    };
    if flags & ACC_STATIC != 0 {
        vm.ensure_initialized(class)?;
        vm.classes.get_mut(class).static_values[storage as usize] = value;
    } else {
        set_instance_value(vm, target, storage, value)?;
    }
    void()
}

fn reflect_field_get_primitive(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    field_value(vm, receiver(args), ref_arg(args, 1))
}

fn reflect_field_set_primitive(
    vm: &mut Vm<'_>,
    args: &[Value],
    value: Value,
) -> Result<Value, VmError> {
    let this = receiver(args);
    let target = ref_arg(args, 1);
    let (class, index) = field_parts(vm, this);
    let storage = field_storage_slot(vm, class, index);
    let (_, _, flags) = field_entry(vm, class, index);
    check_field_writable(vm, this, flags)?;
    if flags & ACC_STATIC != 0 {
        vm.ensure_initialized(class)?;
        vm.classes.get_mut(class).static_values[storage as usize] = value;
    } else {
        set_instance_value(vm, target, storage, value)?;
    }
    void()
}

const FIELD_METHODS: [oxjvm_vm::NativeMethodDef; 36] = [
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = field_parts(vm, receiver(args));
            let (name, _, _) = field_entry(vm, class, slot);
            object(vm.make_string(&name)?)
        },
    ),
    method(
        "getDeclaringClass",
        "()Ljava/lang/Class;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, _) = field_parts(vm, receiver(args));
            object(vm.class_object(class)?)
        },
    ),
    method(
        "getType",
        "()Ljava/lang/Class;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = field_parts(vm, receiver(args));
            let class_id = field_declared_type(vm, class, slot)?;
            object(vm.class_object(class_id)?)
        },
    ),
    method(
        "getGenericType",
        "()Ljava/lang/reflect/Type;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = field_parts(vm, receiver(args));
            generic_field_type(vm, class, slot)
        },
    ),
    method(
        "toGenericString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = field_parts(vm, receiver(args));
            let (name, descriptor, flags) = field_entry(vm, class, slot);
            let field_type = FieldType::parse(&descriptor)
                .map_err(|_| VmError::internal("bad field descriptor"))?;
            let mask = ACC_PUBLIC
                | ACC_PROTECTED
                | ACC_PRIVATE
                | ACC_STATIC
                | ACC_FINAL
                | ACC_TRANSIENT
                | ACC_VOLATILE;
            let modifiers = modifiers_text(flags & mask);
            let mut text = String::new();
            if !modifiers.is_empty() {
                text.push_str(&modifiers);
                text.push(' ');
            }
            text.push_str(&binary_name_for_field_type(vm, &field_type)?);
            text.push(' ');
            text.push_str(&crate::binary_name(vm, class));
            text.push('.');
            text.push_str(&name);
            object(vm.make_string(&text)?)
        },
    ),
    method("getModifiers", "()I", ACC_PUBLIC, |vm, _, args| {
        let (class, slot) = field_parts(vm, receiver(args));
        let (_, _, flags) = field_entry(vm, class, slot);
        int(i32::from(flags))
    }),
    method("setAccessible", "(Z)V", ACC_PUBLIC, set_accessible),
    method(
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        reflect_field_get,
    ),
    method(
        "set",
        "(Ljava/lang/Object;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        reflect_field_set,
    ),
    method(
        "getInt",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC,
        |vm, _, args| reflect_field_get_primitive(vm, args),
    ),
    method(
        "setInt",
        "(Ljava/lang/Object;I)V",
        ACC_PUBLIC,
        |vm, _, args| reflect_field_set_primitive(vm, args, Value::Int(crate::int_arg(args, 2))),
    ),
    method(
        "getLong",
        "(Ljava/lang/Object;)J",
        ACC_PUBLIC,
        |vm, _, args| reflect_field_get_primitive(vm, args),
    ),
    method(
        "setLong",
        "(Ljava/lang/Object;J)V",
        ACC_PUBLIC,
        |vm, _, args| reflect_field_set_primitive(vm, args, Value::Long(crate::long_arg(args, 2))),
    ),
    method(
        "getBoolean",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| reflect_field_get_primitive(vm, args),
    ),
    method(
        "setBoolean",
        "(Ljava/lang/Object;Z)V",
        ACC_PUBLIC,
        |vm, _, args| {
            reflect_field_set_primitive(vm, args, Value::Int(i32::from(crate::bool_arg(args, 2))))
        },
    ),
    method(
        "getByte",
        "(Ljava/lang/Object;)B",
        ACC_PUBLIC,
        |vm, _, args| reflect_field_get_primitive(vm, args),
    ),
    method(
        "setByte",
        "(Ljava/lang/Object;B)V",
        ACC_PUBLIC,
        |vm, _, args| reflect_field_set_primitive(vm, args, Value::Int(crate::int_arg(args, 2))),
    ),
    method(
        "getShort",
        "(Ljava/lang/Object;)S",
        ACC_PUBLIC,
        |vm, _, args| reflect_field_get_primitive(vm, args),
    ),
    method(
        "setShort",
        "(Ljava/lang/Object;S)V",
        ACC_PUBLIC,
        |vm, _, args| reflect_field_set_primitive(vm, args, Value::Int(crate::int_arg(args, 2))),
    ),
    method(
        "getChar",
        "(Ljava/lang/Object;)C",
        ACC_PUBLIC,
        |vm, _, args| reflect_field_get_primitive(vm, args),
    ),
    method(
        "setChar",
        "(Ljava/lang/Object;C)V",
        ACC_PUBLIC,
        |vm, _, args| reflect_field_set_primitive(vm, args, Value::Int(crate::int_arg(args, 2))),
    ),
    method(
        "getFloat",
        "(Ljava/lang/Object;)F",
        ACC_PUBLIC,
        |vm, _, args| reflect_field_get_primitive(vm, args),
    ),
    method(
        "setFloat",
        "(Ljava/lang/Object;F)V",
        ACC_PUBLIC,
        |vm, _, args| {
            reflect_field_set_primitive(vm, args, Value::Float(crate::float_arg(args, 2)))
        },
    ),
    method(
        "getDouble",
        "(Ljava/lang/Object;)D",
        ACC_PUBLIC,
        |vm, _, args| reflect_field_get_primitive(vm, args),
    ),
    method(
        "setDouble",
        "(Ljava/lang/Object;D)V",
        ACC_PUBLIC,
        |vm, _, args| {
            reflect_field_set_primitive(vm, args, Value::Double(crate::double_arg(args, 2)))
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
            boolean(field_parts(vm, receiver(args)) == field_parts(vm, other))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let (class, slot) = field_parts(vm, receiver(args));
        int((class.raw() as i32)
            .wrapping_mul(31)
            .wrapping_add(slot as i32))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = field_parts(vm, receiver(args));
            let (name, _, _) = field_entry(vm, class, slot);
            let text = alloc::format!("{}.{name}", vm.class_name(class).replace('/', "."));
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "getAnnotation",
        "(Ljava/lang/Class;)Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = field_parts(vm, receiver(args));
            let annotation = class_argument(vm, ref_arg(args, 1))?;
            object(crate::annotation::field_annotation(
                vm, class, slot, annotation,
            )?)
        },
    ),
    method(
        "getDeclaredAnnotation",
        "(Ljava/lang/Class;)Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = field_parts(vm, receiver(args));
            let annotation = class_argument(vm, ref_arg(args, 1))?;
            object(crate::annotation::field_annotation(
                vm, class, slot, annotation,
            )?)
        },
    ),
    method(
        "getDeclaredAnnotations",
        "()[Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = field_parts(vm, receiver(args));
            let annotations = crate::annotation::field_declared_annotations(vm, class, slot)?;
            object(crate::annotation::annotation_array(vm, &annotations)?)
        },
    ),
    method(
        "getAnnotations",
        "()[Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = field_parts(vm, receiver(args));
            let annotations = crate::annotation::field_declared_annotations(vm, class, slot)?;
            object(crate::annotation::annotation_array(vm, &annotations)?)
        },
    ),
    method(
        "getAnnotationsByType",
        "(Ljava/lang/Class;)[Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = field_parts(vm, receiver(args));
            let annotation = class_argument(vm, ref_arg(args, 1))?;
            object(crate::annotation::field_annotations_by_type(
                vm, class, slot, annotation,
            )?)
        },
    ),
    method(
        "isAnnotationPresent",
        "(Ljava/lang/Class;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = field_parts(vm, receiver(args));
            let annotation = class_argument(vm, ref_arg(args, 1))?;
            let instance = crate::annotation::field_annotation(vm, class, slot, annotation)?;
            boolean(!instance.is_null())
        },
    ),
    method("isSynthetic", "()Z", ACC_PUBLIC, |vm, _, args| {
        let (class, slot) = field_parts(vm, receiver(args));
        let flags = vm.classes.get(class).fields[slot as usize].access_flags;
        boolean(flags & ACC_SYNTHETIC != 0)
    }),
    method("isEnumConstant", "()Z", ACC_PUBLIC, |vm, _, args| {
        let (class, slot) = field_parts(vm, receiver(args));
        let flags = vm.classes.get(class).fields[slot as usize].access_flags;
        boolean(flags & ACC_ENUM != 0)
    }),
];

pub(crate) const FIELD: oxjvm_vm::NativeClass = class(
    "java/lang/reflect/Field",
    Some("java/lang/reflect/AccessibleObject"),
    &[
        "java/lang/reflect/Member",
        "java/lang/reflect/AnnotatedElement",
    ],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("clazz", "Ljava/lang/Class;", ACC_PRIVATE, None),
        field("slot", "I", ACC_PRIVATE, None),
        field("override", "Z", ACC_PRIVATE, None),
    ],
    &FIELD_METHODS,
    None,
);

/// `java.lang.reflect.Member` (both Method and Field implement it).
pub(crate) const MEMBER: oxjvm_vm::NativeClass = class(
    "java/lang/reflect/Member",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[
        method(
            "getName",
            "()Ljava/lang/String;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "getModifiers",
            "()I",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "getDeclaringClass",
            "()Ljava/lang/Class;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
    ],
    None,
);

// -------------------------------------------------------------------------------------------
// Generic signatures
// -------------------------------------------------------------------------------------------

/// A type parsed from a generic `Signature` attribute.
#[derive(Debug, Clone)]
enum ParsedType {
    Primitive(oxjvm_classfile::BaseType),
    Class {
        name: String,
        arguments: Vec<ParsedType>,
    },
    Variable(String),
    Array(Box<ParsedType>),
    Wildcard {
        upper: Vec<ParsedType>,
        lower: Vec<ParsedType>,
    },
}

/// A recursive-descent parser for the JVMS 4.7.9.1 signature grammar, limited to the
/// structures reflection exposes.
struct SignatureParser<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> SignatureParser<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            bytes: text.as_bytes(),
            position: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    fn advance(&mut self) {
        self.position += 1;
    }

    fn expect(&mut self, byte: u8) -> Result<(), VmError> {
        if self.peek() == Some(byte) {
            self.advance();
            Ok(())
        } else {
            Err(VmError::internal("bad generic signature"))
        }
    }

    fn read_until(&mut self, terminator: u8) -> Result<String, VmError> {
        let mut text = String::new();
        while let Some(byte) = self.peek() {
            if byte == terminator {
                return Ok(text);
            }
            text.push(byte as char);
            self.advance();
        }
        Err(VmError::internal("bad generic signature"))
    }

    fn parse_field_type(&mut self) -> Result<ParsedType, VmError> {
        match self.peek() {
            Some(b'L') => {
                self.advance();
                self.parse_class()
            }
            Some(b'T') => {
                self.advance();
                let name = self.read_until(b';')?;
                self.expect(b';')?;
                Ok(ParsedType::Variable(name))
            }
            Some(b'[') => {
                self.advance();
                let component = self.parse_field_type()?;
                Ok(ParsedType::Array(Box::new(component)))
            }
            Some(byte) => {
                let base = match byte {
                    b'B' => oxjvm_classfile::BaseType::Byte,
                    b'C' => oxjvm_classfile::BaseType::Char,
                    b'D' => oxjvm_classfile::BaseType::Double,
                    b'F' => oxjvm_classfile::BaseType::Float,
                    b'I' => oxjvm_classfile::BaseType::Int,
                    b'J' => oxjvm_classfile::BaseType::Long,
                    b'S' => oxjvm_classfile::BaseType::Short,
                    b'Z' => oxjvm_classfile::BaseType::Boolean,
                    _ => return Err(VmError::internal("bad generic signature")),
                };
                self.advance();
                Ok(ParsedType::Primitive(base))
            }
            None => Err(VmError::internal("bad generic signature")),
        }
    }

    fn parse_class(&mut self) -> Result<ParsedType, VmError> {
        let mut name = String::new();
        let mut arguments = Vec::new();
        loop {
            while let Some(byte) = self.peek() {
                if byte == b'<' || byte == b';' || byte == b'.' {
                    break;
                }
                name.push(byte as char);
                self.advance();
            }
            if self.peek() == Some(b'<') {
                let parsed = self.parse_arguments()?;
                if arguments.is_empty() {
                    arguments = parsed;
                }
            }
            match self.peek() {
                Some(b'.') => {
                    self.advance();
                    name.push('$');
                }
                Some(b';') => {
                    self.advance();
                    return Ok(ParsedType::Class { name, arguments });
                }
                _ => return Err(VmError::internal("bad generic signature")),
            }
        }
    }

    fn parse_arguments(&mut self) -> Result<Vec<ParsedType>, VmError> {
        self.expect(b'<')?;
        let mut arguments = Vec::new();
        while self.peek() != Some(b'>') {
            arguments.push(self.parse_type_argument()?);
        }
        self.expect(b'>')?;
        Ok(arguments)
    }

    fn parse_type_argument(&mut self) -> Result<ParsedType, VmError> {
        match self.peek() {
            Some(b'*') => {
                self.advance();
                Ok(ParsedType::Wildcard {
                    upper: Vec::new(),
                    lower: Vec::new(),
                })
            }
            Some(b'+') => {
                self.advance();
                let bound = self.parse_field_type()?;
                Ok(ParsedType::Wildcard {
                    upper: vec![bound],
                    lower: Vec::new(),
                })
            }
            Some(b'-') => {
                self.advance();
                let bound = self.parse_field_type()?;
                Ok(ParsedType::Wildcard {
                    upper: Vec::new(),
                    lower: vec![bound],
                })
            }
            _ => self.parse_field_type(),
        }
    }

    /// Skip a balanced `<...>` block of formal type parameters.
    fn skip_balanced(&mut self) -> Result<(), VmError> {
        self.expect(b'<')?;
        let mut depth = 1;
        while depth > 0 {
            match self.peek() {
                Some(b'<') => {
                    depth += 1;
                    self.advance();
                }
                Some(b'>') => {
                    depth -= 1;
                    self.advance();
                }
                Some(_) => self.advance(),
                None => return Err(VmError::internal("bad generic signature")),
            }
        }
        Ok(())
    }
}

fn parse_field_signature(text: &str) -> Result<ParsedType, VmError> {
    let mut parser = SignatureParser::new(text);
    parser.parse_field_type()
}

fn parse_method_signature(text: &str) -> Result<(Vec<ParsedType>, ParsedType), VmError> {
    let mut parser = SignatureParser::new(text);
    if parser.peek() == Some(b'<') {
        parser.skip_balanced()?;
    }
    parser.expect(b'(')?;
    let mut parameters = Vec::new();
    while parser.peek() != Some(b')') {
        parameters.push(parser.parse_field_type()?);
    }
    parser.expect(b')')?;
    let returns = match parser.peek() {
        Some(b'V') => {
            parser.advance();
            ParsedType::Primitive(oxjvm_classfile::BaseType::Void)
        }
        _ => parser.parse_field_type()?,
    };
    Ok((parameters, returns))
}

/// The source-style display name of a parsed type.
fn parsed_type_name(parsed: &ParsedType) -> String {
    match parsed {
        ParsedType::Primitive(base) => base_type_name(*base).to_string(),
        ParsedType::Class { name, arguments } => {
            let dotted = name.replace('/', ".");
            if arguments.is_empty() {
                dotted
            } else {
                let inner: Vec<String> = arguments.iter().map(parsed_type_name).collect();
                alloc::format!("{dotted}<{}>", inner.join(", "))
            }
        }
        ParsedType::Variable(name) => name.clone(),
        ParsedType::Array(component) => alloc::format!("{}[]", parsed_type_name(component)),
        ParsedType::Wildcard { upper, lower } => {
            if let Some(bound) = lower.first() {
                alloc::format!("? super {}", parsed_type_name(bound))
            } else if let Some(bound) = upper.first() {
                alloc::format!("? extends {}", parsed_type_name(bound))
            } else {
                String::from("?")
            }
        }
    }
}

const PARAMETERIZED_TYPE_IMPL: &str =
    "sun/reflect/generics/reflectiveObjects/ParameterizedTypeImpl";
const TYPE_VARIABLE_IMPL: &str = "sun/reflect/generics/reflectiveObjects/TypeVariableImpl";
const GENERIC_ARRAY_TYPE_IMPL: &str = "sun/reflect/generics/reflectiveObjects/GenericArrayTypeImpl";
const WILDCARD_TYPE_IMPL: &str = "sun/reflect/generics/reflectiveObjects/WildcardTypeImpl";

fn impl_instance(vm: &mut Vm<'_>, name: &str) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class(name)?;
    vm.new_instance(class)
}

fn set_member_field(
    vm: &mut Vm<'_>,
    instance: ObjectRef,
    name: &str,
    descriptor: &str,
    value: ObjectRef,
) -> Result<(), VmError> {
    let class = vm.class_of(instance);
    let (declaring, field) = vm.find_field(class, name, descriptor)?;
    vm.set_instance_ref(instance, declaring, field, value);
    Ok(())
}

fn type_name_of_object(vm: &mut Vm<'_>, object: ObjectRef) -> Result<String, VmError> {
    let (class, method) =
        vm.resolve_virtual_method(object, "getTypeName", "()Ljava/lang/String;")?;
    let text = vm.invoke_method(class, method, vec![Value::Ref(object)])?;
    Ok(vm.string_or_empty(text.as_ref()))
}

/// A `Type` array with the given component type.
fn type_array(vm: &mut Vm<'_>, length: usize) -> Result<ObjectRef, VmError> {
    let type_class = vm.resolve_class("java/lang/reflect/Type")?;
    vm.allocate_object_array(type_class, length)
}

fn new_type_variable(vm: &mut Vm<'_>, name: &str) -> Result<ObjectRef, VmError> {
    let instance = impl_instance(vm, TYPE_VARIABLE_IMPL)?;
    vm.protect(instance);
    let text = vm.make_string(name)?;
    set_member_field(vm, instance, "name", "Ljava/lang/String;", text)?;
    set_member_field(vm, instance, "typeName", "Ljava/lang/String;", text)?;
    Ok(instance)
}

fn new_generic_array_type(vm: &mut Vm<'_>, component: ObjectRef) -> Result<ObjectRef, VmError> {
    let instance = impl_instance(vm, GENERIC_ARRAY_TYPE_IMPL)?;
    vm.protect(instance);
    let name = alloc::format!("{}[]", type_name_of_object(vm, component)?);
    let text = vm.make_string(&name)?;
    set_member_field(
        vm,
        instance,
        "componentType",
        "Ljava/lang/reflect/Type;",
        component,
    )?;
    set_member_field(vm, instance, "typeName", "Ljava/lang/String;", text)?;
    Ok(instance)
}

fn new_wildcard_type(
    vm: &mut Vm<'_>,
    upper: &[ObjectRef],
    lower: &[ObjectRef],
) -> Result<ObjectRef, VmError> {
    let instance = impl_instance(vm, WILDCARD_TYPE_IMPL)?;
    vm.protect(instance);
    let upper_array = type_array(vm, upper.len())?;
    vm.protect(upper_array);
    for (index, bound) in upper.iter().enumerate() {
        vm.array_set_ref(upper_array, index, *bound)?;
    }
    let lower_array = type_array(vm, lower.len())?;
    vm.protect(lower_array);
    for (index, bound) in lower.iter().enumerate() {
        vm.array_set_ref(lower_array, index, *bound)?;
    }
    let name = if let Some(bound) = lower.first() {
        alloc::format!("? super {}", type_name_of_object(vm, *bound)?)
    } else if let Some(bound) = upper.first() {
        alloc::format!("? extends {}", type_name_of_object(vm, *bound)?)
    } else {
        String::from("?")
    };
    let text = vm.make_string(&name)?;
    set_member_field(
        vm,
        instance,
        "upperBounds",
        "[Ljava/lang/reflect/Type;",
        upper_array,
    )?;
    set_member_field(
        vm,
        instance,
        "lowerBounds",
        "[Ljava/lang/reflect/Type;",
        lower_array,
    )?;
    set_member_field(vm, instance, "typeName", "Ljava/lang/String;", text)?;
    Ok(instance)
}

/// Materialise a parsed type as a reflective `Type` object.
fn type_object(vm: &mut Vm<'_>, parsed: &ParsedType) -> Result<ObjectRef, VmError> {
    match parsed {
        ParsedType::Primitive(base) => {
            let class = vm.primitive_class(*base)?;
            vm.class_object(class)
        }
        ParsedType::Class { name, arguments } => {
            let raw = vm.resolve_class(name)?;
            let raw_object = vm.class_object(raw)?;
            if arguments.is_empty() {
                return Ok(raw_object);
            }
            // A parameterized type: raw class plus its actual type arguments.
            let mut converted = Vec::new();
            for argument in arguments {
                converted.push(type_object(vm, argument)?);
            }
            let instance = impl_instance(vm, PARAMETERIZED_TYPE_IMPL)?;
            vm.protect(instance);
            let arguments = type_array(vm, converted.len())?;
            vm.protect(arguments);
            for (index, argument) in converted.iter().enumerate() {
                vm.array_set_ref(arguments, index, *argument)?;
            }
            let text = vm.make_string(&parsed_type_name(parsed))?;
            set_member_field(
                vm,
                instance,
                "rawType",
                "Ljava/lang/reflect/Type;",
                raw_object,
            )?;
            set_member_field(
                vm,
                instance,
                "actualTypeArguments",
                "[Ljava/lang/reflect/Type;",
                arguments,
            )?;
            set_member_field(
                vm,
                instance,
                "ownerType",
                "Ljava/lang/reflect/Type;",
                ObjectRef::NULL,
            )?;
            set_member_field(vm, instance, "typeName", "Ljava/lang/String;", text)?;
            Ok(instance)
        }
        ParsedType::Variable(name) => new_type_variable(vm, name),
        ParsedType::Array(component) => match component.as_ref() {
            ParsedType::Primitive(base) => {
                let descriptor = alloc::format!("[{}", primitive_descriptor_of(*base));
                let class = vm.resolve_class(&descriptor)?;
                vm.class_object(class)
            }
            ParsedType::Class { name, arguments } if arguments.is_empty() => {
                let descriptor = alloc::format!("[L{name};");
                let class = vm.resolve_class(&descriptor)?;
                vm.class_object(class)
            }
            other => {
                let component = type_object(vm, other)?;
                new_generic_array_type(vm, component)
            }
        },
        ParsedType::Wildcard { upper, lower } => {
            let mut converted_lower = Vec::new();
            for bound in lower {
                converted_lower.push(type_object(vm, bound)?);
            }
            let mut converted_upper = Vec::new();
            if upper.is_empty() && lower.is_empty() {
                let object_class = vm.resolve_class("java/lang/Object")?;
                converted_upper.push(vm.class_object(object_class)?);
            } else {
                for bound in upper {
                    converted_upper.push(type_object(vm, bound)?);
                }
            }
            new_wildcard_type(vm, &converted_upper, &converted_lower)
        }
    }
}

fn primitive_descriptor_of(base: oxjvm_classfile::BaseType) -> &'static str {
    match base {
        oxjvm_classfile::BaseType::Boolean => "Z",
        oxjvm_classfile::BaseType::Byte => "B",
        oxjvm_classfile::BaseType::Char => "C",
        oxjvm_classfile::BaseType::Short => "S",
        oxjvm_classfile::BaseType::Int => "I",
        oxjvm_classfile::BaseType::Long => "J",
        oxjvm_classfile::BaseType::Float => "F",
        oxjvm_classfile::BaseType::Double => "D",
        oxjvm_classfile::BaseType::Void => "V",
    }
}

/// The generic type of a field, from its `Signature` when present.
fn generic_field_type(vm: &mut Vm<'_>, class: ClassId, slot: u32) -> Result<Value, VmError> {
    let signature = vm.classes.get(class).fields[slot as usize]
        .signature
        .clone();
    let object = match signature {
        Some(text) => match parse_field_signature(&text) {
            Ok(parsed) => type_object(vm, &parsed)?,
            Err(_) => {
                let class_id = field_declared_type(vm, class, slot)?;
                vm.class_object(class_id)?
            }
        },
        None => {
            let class_id = field_declared_type(vm, class, slot)?;
            vm.class_object(class_id)?
        }
    };
    Ok(Value::Ref(object))
}

/// The generic return type of a method, from its `Signature` when present.
fn generic_return_type(vm: &mut Vm<'_>, class: ClassId, slot: u32) -> Result<Value, VmError> {
    let signature = vm.classes.get(class).methods[slot as usize]
        .signature
        .clone();
    let object = match signature {
        Some(text) => match parse_method_signature(&text) {
            Ok((_, returns)) => type_object(vm, &returns)?,
            Err(_) => {
                let descriptor = vm.classes.get(class).methods[slot as usize]
                    .descriptor
                    .clone();
                let class_id = executable_return_type(vm, &descriptor)?;
                vm.class_object(class_id)?
            }
        },
        None => {
            let descriptor = vm.classes.get(class).methods[slot as usize]
                .descriptor
                .clone();
            let class_id = executable_return_type(vm, &descriptor)?;
            vm.class_object(class_id)?
        }
    };
    Ok(Value::Ref(object))
}

/// The generic parameter types of a method or constructor, from its `Signature` when present.
fn generic_parameter_types(vm: &mut Vm<'_>, class: ClassId, slot: u32) -> Result<Value, VmError> {
    let signature = vm.classes.get(class).methods[slot as usize]
        .signature
        .clone();
    let parsed = signature.and_then(|text| parse_method_signature(&text).ok());
    let objects = match parsed {
        Some((parameters, _)) => {
            let mut objects = Vec::new();
            for parameter in &parameters {
                objects.push(type_object(vm, parameter)?);
            }
            objects
        }
        None => {
            let descriptor = vm.classes.get(class).methods[slot as usize]
                .descriptor
                .clone();
            let mut objects = Vec::new();
            for class_id in executable_parameter_types(vm, &descriptor)? {
                objects.push(vm.class_object(class_id)?);
            }
            objects
        }
    };
    let array = type_array(vm, objects.len())?;
    vm.protect(array);
    for (index, object) in objects.iter().enumerate() {
        vm.array_set_ref(array, index, *object)?;
    }
    Ok(Value::Ref(array))
}

const PARAMETERIZED_TYPE_IMPL_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "getRawType",
        "()Ljava/lang/reflect/Type;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let value = vm.read_ref_field(
                this,
                vm.class_of(this),
                "rawType",
                "Ljava/lang/reflect/Type;",
            );
            object(value.unwrap_or(ObjectRef::NULL))
        },
    ),
    method(
        "getActualTypeArguments",
        "()[Ljava/lang/reflect/Type;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let value = vm.read_ref_field(
                this,
                vm.class_of(this),
                "actualTypeArguments",
                "[Ljava/lang/reflect/Type;",
            );
            object(value.unwrap_or(ObjectRef::NULL))
        },
    ),
    method(
        "getOwnerType",
        "()Ljava/lang/reflect/Type;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let value = vm.read_ref_field(
                this,
                vm.class_of(this),
                "ownerType",
                "Ljava/lang/reflect/Type;",
            );
            object(value.unwrap_or(ObjectRef::NULL))
        },
    ),
    method(
        "getTypeName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = vm
                .read_string_field(this, vm.class_of(this), "typeName")
                .unwrap_or_default();
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = vm
                .read_string_field(this, vm.class_of(this), "typeName")
                .unwrap_or_default();
            object(vm.make_string(&text)?)
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
            let this = receiver(args);
            let class = vm.class_of(this);
            if vm.class_of(other) != class {
                return boolean(false);
            }
            let this_name = vm
                .read_string_field(this, class, "typeName")
                .unwrap_or_default();
            let other_name = vm
                .read_string_field(other, vm.class_of(other), "typeName")
                .unwrap_or_default();
            boolean(this_name == other_name)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let text = vm
            .read_string_field(this, vm.class_of(this), "typeName")
            .unwrap_or_default();
        int(text.chars().fold(0_i32, |hash, ch| {
            hash.wrapping_mul(31).wrapping_add(ch as i32)
        }))
    }),
];

pub(crate) const PARAMETERIZED_TYPE_IMPL_CLASS: oxjvm_vm::NativeClass = class(
    PARAMETERIZED_TYPE_IMPL,
    Some("java/lang/Object"),
    &["java/lang/reflect/ParameterizedType"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("rawType", "Ljava/lang/reflect/Type;", ACC_PRIVATE, None),
        field(
            "actualTypeArguments",
            "[Ljava/lang/reflect/Type;",
            ACC_PRIVATE,
            None,
        ),
        field("ownerType", "Ljava/lang/reflect/Type;", ACC_PRIVATE, None),
        field("typeName", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &PARAMETERIZED_TYPE_IMPL_METHODS,
    None,
);

const TYPE_VARIABLE_IMPL_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = vm
                .read_string_field(this, vm.class_of(this), "name")
                .unwrap_or_default();
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "getBounds",
        "()[Ljava/lang/reflect/Type;",
        ACC_PUBLIC,
        |vm, _, _| {
            let array = type_array(vm, 1)?;
            vm.protect(array);
            let object_class = vm.resolve_class("java/lang/Object")?;
            let bound = vm.class_object(object_class)?;
            vm.array_set_ref(array, 0, bound)?;
            object(array)
        },
    ),
    method(
        "getGenericDeclaration",
        "()Ljava/lang/reflect/GenericDeclaration;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getTypeName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = vm
                .read_string_field(this, vm.class_of(this), "typeName")
                .unwrap_or_default();
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = vm
                .read_string_field(this, vm.class_of(this), "typeName")
                .unwrap_or_default();
            object(vm.make_string(&text)?)
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
            let this = receiver(args);
            let class = vm.class_of(this);
            if vm.class_of(other) != class {
                return boolean(false);
            }
            let this_name = vm
                .read_string_field(this, class, "typeName")
                .unwrap_or_default();
            let other_name = vm
                .read_string_field(other, vm.class_of(other), "typeName")
                .unwrap_or_default();
            boolean(this_name == other_name)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let text = vm
            .read_string_field(this, vm.class_of(this), "typeName")
            .unwrap_or_default();
        int(text.chars().fold(0_i32, |hash, ch| {
            hash.wrapping_mul(31).wrapping_add(ch as i32)
        }))
    }),
];

pub(crate) const TYPE_VARIABLE_IMPL_CLASS: oxjvm_vm::NativeClass = class(
    TYPE_VARIABLE_IMPL,
    Some("java/lang/Object"),
    &["java/lang/reflect/TypeVariable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("typeName", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &TYPE_VARIABLE_IMPL_METHODS,
    None,
);

const GENERIC_ARRAY_TYPE_IMPL_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "getGenericComponentType",
        "()Ljava/lang/reflect/Type;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let value = vm.read_ref_field(
                this,
                vm.class_of(this),
                "componentType",
                "Ljava/lang/reflect/Type;",
            );
            object(value.unwrap_or(ObjectRef::NULL))
        },
    ),
    method(
        "getTypeName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = vm
                .read_string_field(this, vm.class_of(this), "typeName")
                .unwrap_or_default();
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = vm
                .read_string_field(this, vm.class_of(this), "typeName")
                .unwrap_or_default();
            object(vm.make_string(&text)?)
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
            let this = receiver(args);
            let class = vm.class_of(this);
            if vm.class_of(other) != class {
                return boolean(false);
            }
            let this_name = vm
                .read_string_field(this, class, "typeName")
                .unwrap_or_default();
            let other_name = vm
                .read_string_field(other, vm.class_of(other), "typeName")
                .unwrap_or_default();
            boolean(this_name == other_name)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let text = vm
            .read_string_field(this, vm.class_of(this), "typeName")
            .unwrap_or_default();
        int(text.chars().fold(0_i32, |hash, ch| {
            hash.wrapping_mul(31).wrapping_add(ch as i32)
        }))
    }),
];

pub(crate) const GENERIC_ARRAY_TYPE_IMPL_CLASS: oxjvm_vm::NativeClass = class(
    GENERIC_ARRAY_TYPE_IMPL,
    Some("java/lang/Object"),
    &["java/lang/reflect/GenericArrayType"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field(
            "componentType",
            "Ljava/lang/reflect/Type;",
            ACC_PRIVATE,
            None,
        ),
        field("typeName", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &GENERIC_ARRAY_TYPE_IMPL_METHODS,
    None,
);

const WILDCARD_TYPE_IMPL_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "getUpperBounds",
        "()[Ljava/lang/reflect/Type;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let value = vm.read_ref_field(
                this,
                vm.class_of(this),
                "upperBounds",
                "[Ljava/lang/reflect/Type;",
            );
            object(value.unwrap_or(ObjectRef::NULL))
        },
    ),
    method(
        "getLowerBounds",
        "()[Ljava/lang/reflect/Type;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let value = vm.read_ref_field(
                this,
                vm.class_of(this),
                "lowerBounds",
                "[Ljava/lang/reflect/Type;",
            );
            object(value.unwrap_or(ObjectRef::NULL))
        },
    ),
    method(
        "getTypeName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = vm
                .read_string_field(this, vm.class_of(this), "typeName")
                .unwrap_or_default();
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = vm
                .read_string_field(this, vm.class_of(this), "typeName")
                .unwrap_or_default();
            object(vm.make_string(&text)?)
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
            let this = receiver(args);
            let class = vm.class_of(this);
            if vm.class_of(other) != class {
                return boolean(false);
            }
            let this_name = vm
                .read_string_field(this, class, "typeName")
                .unwrap_or_default();
            let other_name = vm
                .read_string_field(other, vm.class_of(other), "typeName")
                .unwrap_or_default();
            boolean(this_name == other_name)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let text = vm
            .read_string_field(this, vm.class_of(this), "typeName")
            .unwrap_or_default();
        int(text.chars().fold(0_i32, |hash, ch| {
            hash.wrapping_mul(31).wrapping_add(ch as i32)
        }))
    }),
];

pub(crate) const WILDCARD_TYPE_IMPL_CLASS: oxjvm_vm::NativeClass = class(
    WILDCARD_TYPE_IMPL,
    Some("java/lang/Object"),
    &["java/lang/reflect/WildcardType"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field(
            "upperBounds",
            "[Ljava/lang/reflect/Type;",
            ACC_PRIVATE,
            None,
        ),
        field(
            "lowerBounds",
            "[Ljava/lang/reflect/Type;",
            ACC_PRIVATE,
            None,
        ),
        field("typeName", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &WILDCARD_TYPE_IMPL_METHODS,
    None,
);

/// `java.lang.reflect.InvocationHandler`, for proxy call sites.
pub(crate) const INVOCATION_HANDLER: oxjvm_vm::NativeClass = class(
    "java/lang/reflect/InvocationHandler",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[method(
        "invoke",
        "(Ljava/lang/Object;Ljava/lang/reflect/Method;[Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    )],
    None,
);

/// `java.lang.reflect.Proxy`: dynamic proxies are not generated, so every query answers
/// "not a proxy" and generation throws.
pub(crate) const PROXY: oxjvm_vm::NativeClass = class(
    "java/lang/reflect/Proxy",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[],
    &[
        method(
            "isProxyClass",
            "(Ljava/lang/Class;)Z",
            ACC_PUBLIC | ACC_STATIC,
            |_, _, _| boolean(false),
        ),
        method(
            "getInvocationHandler",
            "(Ljava/lang/Object;)Ljava/lang/reflect/InvocationHandler;",
            ACC_PUBLIC | ACC_STATIC,
            |vm, _, _| {
                Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("not a proxy instance"),
                ))
            },
        ),
        method(
            "getProxyClass",
            "(Ljava/lang/ClassLoader;[Ljava/lang/Class;)Ljava/lang/Class;",
            ACC_PUBLIC | ACC_STATIC,
            |vm, _, _| {
                Err(vm.throw_new(
                    "java/lang/UnsupportedOperationException",
                    Some("dynamic proxy classes are not supported"),
                ))
            },
        ),
        method(
            "newProxyInstance",
            "(Ljava/lang/ClassLoader;[Ljava/lang/Class;Ljava/lang/reflect/InvocationHandler;)Ljava/lang/Object;",
            ACC_PUBLIC | ACC_STATIC,
            |vm, _, _| {
                Err(vm.throw_new(
                    "java/lang/UnsupportedOperationException",
                    Some("dynamic proxy classes are not supported"),
                ))
            },
        ),
    ],
    None,
);

/// `java.lang.reflect.Type`: here raw `Class` objects stand in for generic types.
pub(crate) const TYPE: oxjvm_vm::NativeClass = class(
    "java/lang/reflect/Type",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

/// The remaining `java.lang.reflect` type interfaces, present so `instanceof` checks resolve.
pub(crate) const PARAMETERIZED_TYPE: oxjvm_vm::NativeClass = class(
    "java/lang/reflect/ParameterizedType",
    Some("java/lang/Object"),
    &["java/lang/reflect/Type"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[
        method(
            "getActualTypeArguments",
            "()[Ljava/lang/reflect/Type;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "getRawType",
            "()Ljava/lang/reflect/Type;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "getOwnerType",
            "()Ljava/lang/reflect/Type;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
    ],
    None,
);

pub(crate) const GENERIC_ARRAY_TYPE: oxjvm_vm::NativeClass = class(
    "java/lang/reflect/GenericArrayType",
    Some("java/lang/Object"),
    &["java/lang/reflect/Type"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[method(
        "getGenericComponentType",
        "()Ljava/lang/reflect/Type;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    )],
    None,
);

pub(crate) const TYPE_VARIABLE: oxjvm_vm::NativeClass = class(
    "java/lang/reflect/TypeVariable",
    Some("java/lang/Object"),
    &["java/lang/reflect/Type"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[
        method(
            "getName",
            "()Ljava/lang/String;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "getBounds",
            "()[Ljava/lang/reflect/Type;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "getGenericDeclaration",
            "()Ljava/lang/reflect/GenericDeclaration;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
    ],
    None,
);

pub(crate) const WILDCARD_TYPE: oxjvm_vm::NativeClass = class(
    "java/lang/reflect/WildcardType",
    Some("java/lang/Object"),
    &["java/lang/reflect/Type"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[
        method(
            "getUpperBounds",
            "()[Ljava/lang/reflect/Type;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "getLowerBounds",
            "()[Ljava/lang/reflect/Type;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
    ],
    None,
);

/// `java.lang.reflect.GenericDeclaration`, the super-interface of the reflective type owners.
pub(crate) const GENERIC_DECLARATION: oxjvm_vm::NativeClass = class(
    "java/lang/reflect/GenericDeclaration",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[method(
        "getTypeParameters",
        "()[Ljava/lang/reflect/TypeVariable;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    )],
    None,
);

/// `java.lang.reflect.AnnotatedElement`.
pub(crate) const ANNOTATED_ELEMENT: oxjvm_vm::NativeClass = class(
    "java/lang/reflect/AnnotatedElement",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[
        method(
            "getAnnotation",
            "(Ljava/lang/Class;)Ljava/lang/annotation/Annotation;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "getAnnotations",
            "()[Ljava/lang/annotation/Annotation;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "getDeclaredAnnotations",
            "()[Ljava/lang/annotation/Annotation;",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "isAnnotationPresent",
            "(Ljava/lang/Class;)Z",
            ACC_PUBLIC,
            crate::annotation::default_is_annotation_present,
        ),
        method(
            "getDeclaredAnnotation",
            "(Ljava/lang/Class;)Ljava/lang/annotation/Annotation;",
            ACC_PUBLIC,
            crate::annotation::default_get_declared_annotation,
        ),
        method(
            "getAnnotationsByType",
            "(Ljava/lang/Class;)[Ljava/lang/annotation/Annotation;",
            ACC_PUBLIC,
            crate::annotation::default_get_annotations_by_type,
        ),
        method(
            "getDeclaredAnnotationsByType",
            "(Ljava/lang/Class;)[Ljava/lang/annotation/Annotation;",
            ACC_PUBLIC,
            crate::annotation::default_get_declared_annotations_by_type,
        ),
    ],
    None,
);

/// `java.lang.reflect.Modifier`.
pub(crate) const MODIFIER: oxjvm_vm::NativeClass = class(
    "java/lang/reflect/Modifier",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field(
            "PUBLIC",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(0x0001)),
        ),
        field(
            "PRIVATE",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(0x0002)),
        ),
        field(
            "PROTECTED",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(0x0004)),
        ),
        field(
            "STATIC",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(0x0008)),
        ),
        field(
            "FINAL",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(0x0010)),
        ),
        field(
            "SYNCHRONIZED",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(0x0020)),
        ),
        field(
            "VOLATILE",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(0x0040)),
        ),
        field(
            "TRANSIENT",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(0x0080)),
        ),
        field(
            "NATIVE",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(0x0100)),
        ),
        field(
            "INTERFACE",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(0x0200)),
        ),
        field(
            "ABSTRACT",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(0x0400)),
        ),
        field(
            "STRICT",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(0x0800)),
        ),
        field(
            "SYNTHETIC",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(0x1000)),
        ),
        field(
            "ANNOTATION",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(0x2000)),
        ),
        field(
            "ENUM",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(0x4000)),
        ),
        field(
            "MANDATED",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(0x8000)),
        ),
    ],
    &[
        method("isPublic", "(I)Z", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
            boolean(crate::int_arg(args, 0) & 0x0001 != 0)
        }),
        method(
            "isPrivate",
            "(I)Z",
            ACC_PUBLIC | ACC_STATIC,
            |_, _, args| boolean(crate::int_arg(args, 0) & 0x0002 != 0),
        ),
        method(
            "isProtected",
            "(I)Z",
            ACC_PUBLIC | ACC_STATIC,
            |_, _, args| boolean(crate::int_arg(args, 0) & 0x0004 != 0),
        ),
        method("isStatic", "(I)Z", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
            boolean(crate::int_arg(args, 0) & 0x0008 != 0)
        }),
        method("isFinal", "(I)Z", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
            boolean(crate::int_arg(args, 0) & 0x0010 != 0)
        }),
        method(
            "isSynchronized",
            "(I)Z",
            ACC_PUBLIC | ACC_STATIC,
            |_, _, args| boolean(crate::int_arg(args, 0) & 0x0020 != 0),
        ),
        method(
            "isVolatile",
            "(I)Z",
            ACC_PUBLIC | ACC_STATIC,
            |_, _, args| boolean(crate::int_arg(args, 0) & 0x0040 != 0),
        ),
        method(
            "isTransient",
            "(I)Z",
            ACC_PUBLIC | ACC_STATIC,
            |_, _, args| boolean(crate::int_arg(args, 0) & 0x0080 != 0),
        ),
        method("isNative", "(I)Z", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
            boolean(crate::int_arg(args, 0) & 0x0100 != 0)
        }),
        method(
            "isInterface",
            "(I)Z",
            ACC_PUBLIC | ACC_STATIC,
            |_, _, args| boolean(crate::int_arg(args, 0) & 0x0200 != 0),
        ),
        method(
            "isAbstract",
            "(I)Z",
            ACC_PUBLIC | ACC_STATIC,
            |_, _, args| boolean(crate::int_arg(args, 0) & 0x0400 != 0),
        ),
        method("isStrict", "(I)Z", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
            boolean(crate::int_arg(args, 0) & 0x0800 != 0)
        }),
        method(
            "isSynthetic",
            "(I)Z",
            ACC_PUBLIC | ACC_STATIC,
            |_, _, args| boolean(crate::int_arg(args, 0) & 0x1000 != 0),
        ),
        method(
            "isAnnotation",
            "(I)Z",
            ACC_PUBLIC | ACC_STATIC,
            |_, _, args| boolean(crate::int_arg(args, 0) & 0x2000 != 0),
        ),
        method("isEnum", "(I)Z", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
            boolean(crate::int_arg(args, 0) & 0x4000 != 0)
        }),
        method(
            "isMandated",
            "(I)Z",
            ACC_PUBLIC | ACC_STATIC,
            |_, _, args| boolean(crate::int_arg(args, 0) & 0x8000 != 0),
        ),
        method(
            "toString",
            "(I)Ljava/lang/String;",
            ACC_PUBLIC | ACC_STATIC,
            |vm, _, args| {
                let text = modifiers_text(crate::int_arg(args, 0) as u16);
                object(vm.make_string(&text)?)
            },
        ),
        method(
            "classModifiers",
            "()I",
            ACC_PUBLIC | ACC_STATIC,
            |_, _, _| {
                int(0x0001 | 0x0002 | 0x0004 | 0x0010 | 0x0200 | 0x0400 | 0x1000 | 0x2000 | 0x4000)
            },
        ),
        method(
            "interfaceModifiers",
            "()I",
            ACC_PUBLIC | ACC_STATIC,
            |_, _, _| int(0x0001 | 0x0002 | 0x0004 | 0x0400 | 0x0200 | 0x1000 | 0x2000),
        ),
        method(
            "constructorModifiers",
            "()I",
            ACC_PUBLIC | ACC_STATIC,
            |_, _, _| int(0x0001 | 0x0002 | 0x0004),
        ),
        method(
            "methodModifiers",
            "()I",
            ACC_PUBLIC | ACC_STATIC,
            |_, _, _| {
                int(0x0001
                    | 0x0002
                    | 0x0004
                    | 0x0008
                    | 0x0010
                    | 0x0020
                    | 0x0100
                    | 0x0400
                    | 0x0800
                    | 0x1000)
            },
        ),
        method(
            "fieldModifiers",
            "()I",
            ACC_PUBLIC | ACC_STATIC,
            |_, _, _| int(0x0001 | 0x0002 | 0x0004 | 0x0008 | 0x0010 | 0x0040 | 0x0080 | 0x1000),
        ),
        method(
            "parameterModifiers",
            "()I",
            ACC_PUBLIC | ACC_STATIC,
            |_, _, _| int(0x0010 | 0x1000 | 0x8000),
        ),
    ],
    None,
);

// -------------------------------------------------------------------------------------------
// Constructor
// -------------------------------------------------------------------------------------------

fn new_constructor(vm: &mut Vm<'_>, class: ClassId, slot: u32) -> Result<ObjectRef, VmError> {
    let constructor_class = vm.resolve_class("java/lang/reflect/Constructor")?;
    let instance = vm.new_instance(constructor_class)?;
    let class_object = vm.class_object(class)?;
    let (declaring, field) = vm.find_field(constructor_class, "clazz", "Ljava/lang/Class;")?;
    vm.set_instance_ref(instance, declaring, field, class_object);
    if let Some((declaring, field)) = vm.find_field_opt(constructor_class, "slot", "I") {
        vm.set_instance_int(instance, declaring, field, slot as i32);
    }
    Ok(instance)
}

fn constructor_parts(vm: &Vm<'_>, this: ObjectRef) -> (ClassId, u32) {
    let class = class_object_target(vm, this);
    let slot = crate::lang::read_int_field(vm, this, "slot").max(0) as u32;
    (class, slot)
}

fn constructor_matches(
    vm: &mut Vm<'_>,
    class: ClassId,
    slot: u32,
    types: &[ClassId],
) -> Result<bool, VmError> {
    let (_, descriptor, _) = method_entry(vm, class, slot);
    let parsed = MethodDescriptor::parse(&descriptor)
        .map_err(|_| VmError::internal("bad constructor descriptor"))?;
    if parsed.parameters.len() != types.len() {
        return Ok(false);
    }
    for (parameter, expected) in parsed.parameters.iter().zip(types) {
        if field_type_class(vm, parameter)? != *expected {
            return Ok(false);
        }
    }
    Ok(true)
}

fn class_array_types(vm: &mut Vm<'_>, array: ObjectRef) -> Result<Vec<ClassId>, VmError> {
    if array.is_null() {
        return Ok(Vec::new());
    }
    let length = vm.array_length(array)?;
    let mut types = Vec::with_capacity(length);
    for index in 0..length {
        let value = vm.array_get(array, index as i32)?;
        types.push(class_argument(vm, value.as_ref())?);
    }
    Ok(types)
}

fn constructor_array(
    vm: &mut Vm<'_>,
    class: ClassId,
    public_only: bool,
) -> Result<ObjectRef, VmError> {
    let slots: Vec<u32> = declared_constructors(vm, class)
        .into_iter()
        .filter(|slot| {
            !public_only
                || vm.classes.get(class).methods[*slot as usize].access_flags & ACC_PUBLIC != 0
        })
        .collect();
    let constructor_class = vm.resolve_class("java/lang/reflect/Constructor")?;
    let array = vm.allocate_object_array(constructor_class, slots.len())?;
    vm.protect(array);
    for (index, slot) in slots.iter().enumerate() {
        let constructor = new_constructor(vm, class, *slot)?;
        vm.array_set_ref(array, index, constructor)?;
    }
    vm.unprotect(array);
    Ok(array)
}

/// `Class.getDeclaredConstructors()`.
pub(crate) fn class_reflect_constructors(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    object(constructor_array(vm, class, false)?)
}

/// `Class.getConstructors()`.
pub(crate) fn class_reflect_public_constructors(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    object(constructor_array(vm, class, true)?)
}

fn find_constructor(
    vm: &mut Vm<'_>,
    class: ClassId,
    types: &[ClassId],
    public_only: bool,
) -> Result<ObjectRef, VmError> {
    for slot in declared_constructors(vm, class) {
        if public_only
            && vm.classes.get(class).methods[slot as usize].access_flags & ACC_PUBLIC == 0
        {
            continue;
        }
        if constructor_matches(vm, class, slot, types)? {
            return new_constructor(vm, class, slot);
        }
    }
    Err(vm.throw_new("java/lang/NoSuchMethodException", Some("<init>")))
}

/// `Class.getDeclaredConstructor(Class[])`.
pub(crate) fn class_reflect_constructor(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    let types = class_array_types(vm, ref_arg(args, 1))?;
    object(find_constructor(vm, class, &types, false)?)
}

/// `Class.getConstructor(Class[])`.
pub(crate) fn class_reflect_public_constructor(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    let types = class_array_types(vm, ref_arg(args, 1))?;
    object(find_constructor(vm, class, &types, true)?)
}

fn declared_constructors(vm: &Vm<'_>, class: ClassId) -> Vec<u32> {
    vm.classes
        .get(class)
        .methods
        .iter()
        .enumerate()
        .filter(|(_, method)| method.name == "<init>")
        .map(|(index, _)| index as u32)
        .collect()
}

fn base_type_name(base: oxjvm_classfile::BaseType) -> &'static str {
    match base {
        oxjvm_classfile::BaseType::Boolean => "boolean",
        oxjvm_classfile::BaseType::Byte => "byte",
        oxjvm_classfile::BaseType::Char => "char",
        oxjvm_classfile::BaseType::Short => "short",
        oxjvm_classfile::BaseType::Int => "int",
        oxjvm_classfile::BaseType::Long => "long",
        oxjvm_classfile::BaseType::Float => "float",
        oxjvm_classfile::BaseType::Double => "double",
        oxjvm_classfile::BaseType::Void => "void",
    }
}

fn binary_name_for_field_type(_vm: &Vm<'_>, field_type: &FieldType) -> Result<String, VmError> {
    Ok(match field_type {
        FieldType::Base(base) => base_type_name(*base).to_string(),
        FieldType::Object(name) => name.replace('/', "."),
        FieldType::Array(_) => field_type_descriptor(field_type).replace('/', "."),
    })
}

/// The JDK-style signature text of a method (`Method.toString` and `toGenericString`).
fn method_signature_text(vm: &mut Vm<'_>, class: ClassId, slot: u32) -> Result<String, VmError> {
    let (name, descriptor, flags) = method_entry(vm, class, slot);
    let parsed = MethodDescriptor::parse(&descriptor)
        .map_err(|_| VmError::internal("bad method descriptor"))?;
    let mask = ACC_PUBLIC
        | ACC_PROTECTED
        | ACC_PRIVATE
        | ACC_ABSTRACT
        | ACC_STATIC
        | ACC_FINAL
        | ACC_SYNCHRONIZED
        | ACC_NATIVE
        | ACC_STRICT;
    let mut modifiers = modifiers_text(flags & mask);
    if vm.classes.get(class).is_interface()
        && flags & (ACC_ABSTRACT | ACC_STATIC | ACC_PRIVATE) == 0
    {
        if !modifiers.is_empty() {
            modifiers.push(' ');
        }
        modifiers.push_str("default");
    }
    let mut parameters = Vec::new();
    for parameter in &parsed.parameters {
        parameters.push(binary_name_for_field_type(vm, parameter)?);
    }
    let return_type = match &parsed.returns {
        Some(returns) => binary_name_for_field_type(vm, returns)?,
        None => String::from("void"),
    };
    let declaring = crate::binary_name(vm, class);
    let mut text = String::new();
    if !modifiers.is_empty() {
        text.push_str(&modifiers);
        text.push(' ');
    }
    text.push_str(&alloc::format!(
        "{return_type} {declaring}.{name}({})",
        parameters.join(",")
    ));
    Ok(text)
}

/// The JDK-style signature text of a constructor (`Constructor.toGenericString`).
fn constructor_signature_text(
    vm: &mut Vm<'_>,
    class: ClassId,
    slot: u32,
) -> Result<String, VmError> {
    let (_, descriptor, flags) = method_entry(vm, class, slot);
    let parsed = MethodDescriptor::parse(&descriptor)
        .map_err(|_| VmError::internal("bad constructor descriptor"))?;
    let mask = ACC_PUBLIC | ACC_PROTECTED | ACC_PRIVATE | ACC_FINAL;
    let modifiers = modifiers_text(flags & mask);
    let mut parameters = Vec::new();
    for parameter in &parsed.parameters {
        parameters.push(binary_name_for_field_type(vm, parameter)?);
    }
    let declaring = crate::binary_name(vm, class);
    let mut text = String::new();
    if !modifiers.is_empty() {
        text.push_str(&modifiers);
        text.push(' ');
    }
    text.push_str(&alloc::format!("{declaring}({})", parameters.join(",")));
    Ok(text)
}

fn modifiers_text(flags: u16) -> String {
    let mut parts = Vec::new();
    if flags & ACC_PUBLIC != 0 {
        parts.push("public");
    }
    if flags & ACC_PROTECTED != 0 {
        parts.push("protected");
    }
    if flags & ACC_PRIVATE != 0 {
        parts.push("private");
    }
    if flags & ACC_ABSTRACT != 0 {
        parts.push("abstract");
    }
    if flags & ACC_STATIC != 0 {
        parts.push("static");
    }
    if flags & ACC_FINAL != 0 {
        parts.push("final");
    }
    if flags & ACC_TRANSIENT != 0 {
        parts.push("transient");
    }
    if flags & ACC_VOLATILE != 0 {
        parts.push("volatile");
    }
    if flags & ACC_SYNCHRONIZED != 0 {
        parts.push("synchronized");
    }
    if flags & ACC_NATIVE != 0 {
        parts.push("native");
    }
    if flags & ACC_STRICT != 0 {
        parts.push("strictfp");
    }
    if flags & ACC_INTERFACE != 0 {
        parts.push("interface");
    }
    parts.join(" ")
}

fn constructor_new_instance(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let (class, slot) = constructor_parts(vm, receiver(args));
    let (is_interface, flags) = {
        let entry = vm.classes.get(class);
        (entry.is_interface(), entry.access_flags)
    };
    if is_interface || flags & ACC_ABSTRACT != 0 {
        let name = crate::binary_name(vm, class);
        return Err(vm.throw_new("java/lang/InstantiationException", Some(&name)));
    }
    vm.ensure_initialized(class)?;
    let (_, descriptor, _) = method_entry(vm, class, slot);
    let parsed = MethodDescriptor::parse(&descriptor)
        .map_err(|_| VmError::internal("bad constructor descriptor"))?;
    let arguments = ref_arg(args, 1);
    let mut call_args = Vec::with_capacity(parsed.parameters.len() + 1);
    for (index, parameter) in parsed.parameters.iter().enumerate() {
        let value = vm.array_get(arguments, index as i32)?;
        call_args.push(crate::boxed::unbox_value(
            vm,
            value.as_ref(),
            &field_type_descriptor(parameter),
        ));
    }
    let instance = vm.new_instance(class)?;
    vm.protect(instance);
    call_args.insert(0, Value::Ref(instance));
    vm.invoke_method(class, slot, call_args)?;
    vm.unprotect(instance);
    object(instance)
}

const CONSTRUCTOR_METHODS: [oxjvm_vm::NativeMethodDef; 24] = [
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, _) = constructor_parts(vm, receiver(args));
            let name = crate::binary_name(vm, class);
            object(vm.make_string(&name)?)
        },
    ),
    method(
        "getDeclaringClass",
        "()Ljava/lang/Class;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, _) = constructor_parts(vm, receiver(args));
            object(vm.class_object(class)?)
        },
    ),
    method("getModifiers", "()I", ACC_PUBLIC, |vm, _, args| {
        let (class, slot) = constructor_parts(vm, receiver(args));
        let (_, _, flags) = method_entry(vm, class, slot);
        int(i32::from(flags))
    }),
    method(
        "getParameterTypes",
        "()[Ljava/lang/Class;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = constructor_parts(vm, receiver(args));
            let (_, descriptor, _) = method_entry(vm, class, slot);
            let parsed = MethodDescriptor::parse(&descriptor)
                .map_err(|_| VmError::internal("bad constructor descriptor"))?;
            let mut classes = Vec::new();
            for parameter in &parsed.parameters {
                classes.push(field_type_class(vm, parameter)?);
            }
            let class_class = vm.resolve_class("java/lang/Class")?;
            let array = vm.allocate_object_array(class_class, classes.len())?;
            for (index, parameter) in classes.iter().enumerate() {
                let object = vm.class_object(*parameter)?;
                vm.array_set_ref(array, index, object)?;
            }
            object(array)
        },
    ),
    method("getParameterCount", "()I", ACC_PUBLIC, |vm, _, args| {
        let (class, slot) = constructor_parts(vm, receiver(args));
        let (_, descriptor, _) = method_entry(vm, class, slot);
        let parsed = MethodDescriptor::parse(&descriptor)
            .map_err(|_| VmError::internal("bad constructor descriptor"))?;
        int(parsed.parameters.len() as i32)
    }),
    method(
        "getGenericParameterTypes",
        "()[Ljava/lang/reflect/Type;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = constructor_parts(vm, receiver(args));
            generic_parameter_types(vm, class, slot)
        },
    ),
    method(
        "getGenericExceptionTypes",
        "()[Ljava/lang/reflect/Type;",
        ACC_PUBLIC,
        |vm, _, args| {
            let _ = constructor_parts(vm, receiver(args));
            type_array_of(vm, &[])
        },
    ),
    method(
        "toGenericString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = constructor_parts(vm, receiver(args));
            {
                let text = constructor_signature_text(vm, class, slot)?;
                object(vm.make_string(&text)?)
            }
        },
    ),
    method(
        "getExceptionTypes",
        "()[Ljava/lang/Class;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = constructor_parts(vm, receiver(args));
            let exceptions = vm.classes.get(class).methods[slot as usize]
                .exceptions
                .clone();
            let class_class = vm.resolve_class("java/lang/Class")?;
            let array = vm.allocate_object_array(class_class, exceptions.len())?;
            for (index, exception) in exceptions.iter().enumerate() {
                let object = vm.class_object(*exception)?;
                vm.array_set_ref(array, index, object)?;
            }
            object(array)
        },
    ),
    method(
        "newInstance",
        "([Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        constructor_new_instance,
    ),
    method("setAccessible", "(Z)V", ACC_PUBLIC, set_accessible),
    method(
        "getAnnotation",
        "(Ljava/lang/Class;)Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = constructor_parts(vm, receiver(args));
            let annotation = class_argument(vm, ref_arg(args, 1))?;
            object(crate::annotation::method_annotation(
                vm, class, slot, annotation,
            )?)
        },
    ),
    method(
        "getDeclaredAnnotation",
        "(Ljava/lang/Class;)Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = constructor_parts(vm, receiver(args));
            let annotation = class_argument(vm, ref_arg(args, 1))?;
            object(crate::annotation::method_annotation(
                vm, class, slot, annotation,
            )?)
        },
    ),
    method(
        "getDeclaredAnnotations",
        "()[Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = constructor_parts(vm, receiver(args));
            let annotations = crate::annotation::method_declared_annotations(vm, class, slot)?;
            object(crate::annotation::annotation_array(vm, &annotations)?)
        },
    ),
    method(
        "getAnnotations",
        "()[Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = constructor_parts(vm, receiver(args));
            let annotations = crate::annotation::method_declared_annotations(vm, class, slot)?;
            object(crate::annotation::annotation_array(vm, &annotations)?)
        },
    ),
    method(
        "getAnnotationsByType",
        "(Ljava/lang/Class;)[Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = constructor_parts(vm, receiver(args));
            let annotation = class_argument(vm, ref_arg(args, 1))?;
            object(crate::annotation::method_annotations_by_type(
                vm, class, slot, annotation,
            )?)
        },
    ),
    method(
        "isAnnotationPresent",
        "(Ljava/lang/Class;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = constructor_parts(vm, receiver(args));
            let annotation = class_argument(vm, ref_arg(args, 1))?;
            let instance = crate::annotation::method_annotation(vm, class, slot, annotation)?;
            boolean(!instance.is_null())
        },
    ),
    method(
        "getParameterAnnotations",
        "()[[Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = constructor_parts(vm, receiver(args));
            object(crate::annotation::constructor_parameter_annotations(
                vm, class, slot,
            )?)
        },
    ),
    method(
        "getParameters",
        "()[Ljava/lang/reflect/Parameter;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (class, slot) = constructor_parts(vm, this);
            let (_, descriptor, _) = method_entry(vm, class, slot);
            executable_parameters(vm, this, &descriptor)
        },
    ),
    method("isSynthetic", "()Z", ACC_PUBLIC, |vm, _, args| {
        let (class, slot) = constructor_parts(vm, receiver(args));
        let flags = vm.classes.get(class).methods[slot as usize].access_flags;
        boolean(flags & ACC_SYNTHETIC != 0)
    }),
    method("isVarArgs", "()Z", ACC_PUBLIC, |vm, _, args| {
        let (class, slot) = constructor_parts(vm, receiver(args));
        let flags = vm.classes.get(class).methods[slot as usize].access_flags;
        boolean(flags & ACC_VARARGS != 0)
    }),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() || vm.class_of(receiver(args)) != vm.class_of(other) {
                return boolean(false);
            }
            boolean(constructor_parts(vm, receiver(args)) == constructor_parts(vm, other))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let (class, slot) = constructor_parts(vm, receiver(args));
        int((class.raw() as i32)
            .wrapping_mul(31)
            .wrapping_add(slot as i32))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, slot) = constructor_parts(vm, receiver(args));
            let (_, descriptor, flags) = method_entry(vm, class, slot);
            let parsed = MethodDescriptor::parse(&descriptor)
                .map_err(|_| VmError::internal("bad constructor descriptor"))?;
            let mut parameters = Vec::new();
            for parameter in &parsed.parameters {
                parameters.push(binary_name_for_field_type(vm, parameter)?);
            }
            let modifiers = modifiers_text(flags & (ACC_PUBLIC | ACC_PROTECTED | ACC_PRIVATE));
            let name = crate::binary_name(vm, class);
            let text = alloc::format!("{modifiers} {name}({})", parameters.join(","));
            object(vm.make_string(text.trim())?)
        },
    ),
];

pub(crate) const CONSTRUCTOR: oxjvm_vm::NativeClass = class(
    "java/lang/reflect/Constructor",
    Some("java/lang/reflect/Executable"),
    &[
        "java/lang/reflect/Member",
        "java/lang/reflect/AnnotatedElement",
    ],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("clazz", "Ljava/lang/Class;", ACC_PRIVATE, None),
        field("slot", "I", ACC_PRIVATE, None),
        field("override", "Z", ACC_PRIVATE, None),
    ],
    &CONSTRUCTOR_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// AccessibleObject
// -------------------------------------------------------------------------------------------

const ACCESSIBLE_OBJECT_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("setAccessible", "(Z)V", ACC_PUBLIC, set_accessible),
    method(
        "trySetAccessible",
        "()Z",
        ACC_PUBLIC,
        |vm, context, args| {
            set_accessible(vm, context, &[args[0], Value::Int(1)])?;
            boolean(true)
        },
    ),
    method("isAccessible", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        let value = vm
            .read_instance_int_named(this, class, "override", "Z")
            .unwrap_or(0);
        boolean(value != 0)
    }),
    method(
        "canAccess",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(true),
    ),
    method(
        "setAccessible",
        "([Ljava/lang/reflect/AccessibleObject;Z)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let members = ref_arg(args, 0);
            let flag = crate::int_arg(args, 1) != 0;
            let length = vm.array_length(members)?;
            for index in 0..length {
                let member = vm.array_get(members, index as i32)?.as_ref();
                if member.is_null() {
                    return Err(vm.throw_new("java/lang/NullPointerException", None));
                }
                let class = vm.class_of(member);
                if let Some((declaring, field)) = vm.find_field_opt(class, "override", "Z") {
                    vm.set_instance_int(member, declaring, field, i32::from(flag));
                }
            }
            void()
        },
    ),
];

/// `java.lang.reflect.AccessibleObject`: the shared accessible-flag holder for members.
pub(crate) const ACCESSIBLE_OBJECT: oxjvm_vm::NativeClass = class(
    "java/lang/reflect/AccessibleObject",
    Some("java/lang/Object"),
    &["java/lang/reflect/AnnotatedElement"],
    ACC_PUBLIC,
    &[],
    &ACCESSIBLE_OBJECT_METHODS,
    None,
);

/// Build the `Parameter[]` for a method or constructor.
fn executable_parameters(
    vm: &mut Vm<'_>,
    executable: ObjectRef,
    descriptor: &str,
) -> Result<Value, VmError> {
    let parsed = MethodDescriptor::parse(descriptor)
        .map_err(|_| VmError::internal("bad executable descriptor"))?;
    let parameter_class = vm.resolve_class("java/lang/reflect/Parameter")?;
    let array = vm.allocate_object_array(parameter_class, parsed.parameters.len())?;
    vm.protect(array);
    for index in 0..parsed.parameters.len() {
        let parameter = vm.new_instance(parameter_class)?;
        let (declaring, field) =
            vm.find_field(parameter_class, "executable", "Ljava/lang/Object;")?;
        vm.set_instance_ref(parameter, declaring, field, executable);
        let (declaring, field) = vm.find_field(parameter_class, "slot", "I")?;
        vm.set_instance_int(parameter, declaring, field, index as i32);
        vm.array_set_ref(array, index, parameter)?;
    }
    vm.unprotect(array);
    object(array)
}

fn parameter_parts(vm: &Vm<'_>, this: ObjectRef) -> (ObjectRef, usize) {
    let executable = vm
        .read_ref_field(this, vm.class_of(this), "executable", "Ljava/lang/Object;")
        .unwrap_or(ObjectRef::NULL);
    let slot = crate::lang::read_int_field(vm, this, "slot").max(0) as usize;
    (executable, slot)
}

fn executable_descriptor(vm: &mut Vm<'_>, executable: ObjectRef) -> Result<String, VmError> {
    let method_class = vm.resolve_class("java/lang/reflect/Method")?;
    if vm.class_of(executable) == method_class {
        let (class, slot) = method_parts(vm, executable);
        Ok(method_entry(vm, class, slot).1)
    } else {
        let (class, slot) = constructor_parts(vm, executable);
        Ok(method_entry(vm, class, slot).1)
    }
}

fn parameter_annotations(vm: &mut Vm<'_>, this: ObjectRef) -> Result<ObjectRef, VmError> {
    let (executable, slot) = parameter_parts(vm, this);
    let method_class = vm.resolve_class("java/lang/reflect/Method")?;
    let is_method = !executable.is_null() && vm.class_of(executable) == method_class;
    let arrays = if executable.is_null() {
        ObjectRef::NULL
    } else if is_method {
        let (class, member_slot) = method_parts(vm, executable);
        crate::annotation::method_parameter_annotations(vm, class, member_slot)?
    } else {
        let (class, member_slot) = constructor_parts(vm, executable);
        crate::annotation::constructor_parameter_annotations(vm, class, member_slot)?
    };
    if !arrays.is_null() {
        let length = vm.array_length(arrays)?;
        if slot < length {
            return Ok(vm.array_get(arrays, slot as i32)?.as_ref());
        }
    }
    let component = vm.resolve_class("java/lang/annotation/Annotation")?;
    vm.allocate_object_array(component, 0)
}

fn parameter_get_annotation(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let requested = class_argument(vm, ref_arg(args, 1))?;
    let annotations = parameter_annotations(vm, receiver(args))?;
    let length = vm.array_length(annotations)?;
    for index in 0..length {
        let candidate = vm.array_get(annotations, index as i32)?.as_ref();
        if candidate.is_null() {
            continue;
        }
        if crate::annotation::annotation_type_of(vm, candidate)? == requested {
            return object(candidate);
        }
    }
    object(ObjectRef::NULL)
}

fn parameter_annotations_by_type(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    requested: ClassId,
) -> Result<ObjectRef, VmError> {
    let annotations = parameter_annotations(vm, this)?;
    let length = vm.array_length(annotations)?;
    let mut found = Vec::new();
    for index in 0..length {
        let candidate = vm.array_get(annotations, index as i32)?.as_ref();
        if candidate.is_null() {
            continue;
        }
        if crate::annotation::annotation_type_of(vm, candidate)? == requested {
            found.push(candidate);
        }
    }
    crate::annotation::typed_annotation_array(vm, requested, &found)
}

const PARAMETER_METHODS: [oxjvm_vm::NativeMethodDef; 17] = [
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (_, slot) = parameter_parts(vm, receiver(args));
            let text = alloc::format!("arg{slot}");
            object(vm.make_string(&text)?)
        },
    ),
    method("getModifiers", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method(
        "getDeclaringExecutable",
        "()Ljava/lang/reflect/Executable;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (executable, _) = parameter_parts(vm, receiver(args));
            object(executable)
        },
    ),
    method(
        "getParameterType",
        "()Ljava/lang/Class;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (executable, slot) = parameter_parts(vm, receiver(args));
            let descriptor = executable_descriptor(vm, executable)?;
            let parsed = MethodDescriptor::parse(&descriptor)
                .map_err(|_| VmError::internal("bad executable descriptor"))?;
            let parameter = parsed
                .parameters
                .get(slot)
                .ok_or_else(|| VmError::internal("parameter index out of range"))?;
            let descriptor = field_type_descriptor(parameter);
            let class = crate::annotation::class_for_descriptor(vm, &descriptor)?;
            let class_object = vm.class_object(class)?;
            object(class_object)
        },
    ),
    method(
        "getType",
        "()Ljava/lang/Class;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (executable, slot) = parameter_parts(vm, receiver(args));
            let descriptor = executable_descriptor(vm, executable)?;
            let parsed = MethodDescriptor::parse(&descriptor)
                .map_err(|_| VmError::internal("bad executable descriptor"))?;
            let parameter = parsed
                .parameters
                .get(slot)
                .ok_or_else(|| VmError::internal("parameter index out of range"))?;
            let descriptor = field_type_descriptor(parameter);
            let class = crate::annotation::class_for_descriptor(vm, &descriptor)?;
            let class_object = vm.class_object(class)?;
            object(class_object)
        },
    ),
    method(
        "getParameterizedType",
        "()Ljava/lang/reflect/Type;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (executable, slot) = parameter_parts(vm, receiver(args));
            let descriptor = executable_descriptor(vm, executable)?;
            let parsed = MethodDescriptor::parse(&descriptor)
                .map_err(|_| VmError::internal("bad executable descriptor"))?;
            let parameter = parsed
                .parameters
                .get(slot)
                .ok_or_else(|| VmError::internal("parameter index out of range"))?;
            let descriptor = field_type_descriptor(parameter);
            let class = crate::annotation::class_for_descriptor(vm, &descriptor)?;
            let class_object = vm.class_object(class)?;
            object(class_object)
        },
    ),
    method(
        "getAnnotation",
        "(Ljava/lang/Class;)Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        parameter_get_annotation,
    ),
    method(
        "getAnnotations",
        "()[Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| object(parameter_annotations(vm, receiver(args))?),
    ),
    method(
        "getDeclaredAnnotations",
        "()[Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| object(parameter_annotations(vm, receiver(args))?),
    ),
    method(
        "getAnnotationsByType",
        "(Ljava/lang/Class;)[Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let requested = class_argument(vm, ref_arg(args, 1))?;
            object(parameter_annotations_by_type(
                vm,
                receiver(args),
                requested,
            )?)
        },
    ),
    method(
        "getDeclaredAnnotationsByType",
        "(Ljava/lang/Class;)[Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let requested = class_argument(vm, ref_arg(args, 1))?;
            object(parameter_annotations_by_type(
                vm,
                receiver(args),
                requested,
            )?)
        },
    ),
    method(
        "isAnnotationPresent",
        "(Ljava/lang/Class;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let requested = class_argument(vm, ref_arg(args, 1))?;
            let annotations = parameter_annotations(vm, receiver(args))?;
            let length = vm.array_length(annotations)?;
            for index in 0..length {
                let candidate = vm.array_get(annotations, index as i32)?.as_ref();
                if !candidate.is_null()
                    && crate::annotation::annotation_type_of(vm, candidate)? == requested
                {
                    return boolean(true);
                }
            }
            boolean(false)
        },
    ),
    method("isNamePresent", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("isImplicit", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (executable, slot) = parameter_parts(vm, receiver(args));
            let descriptor = executable_descriptor(vm, executable)?;
            let parsed = MethodDescriptor::parse(&descriptor)
                .map_err(|_| VmError::internal("bad executable descriptor"))?;
            let parameter = parsed
                .parameters
                .get(slot)
                .ok_or_else(|| VmError::internal("parameter index out of range"))?;
            let descriptor = field_type_descriptor(parameter);
            let class = crate::annotation::class_for_descriptor(vm, &descriptor)?;
            let text = crate::lang::canonical_name(vm, class)
                .unwrap_or_else(|| crate::binary_name(vm, class));
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
            if other.is_null() || vm.class_of(this) != vm.class_of(other) {
                return boolean(false);
            }
            boolean(parameter_parts(vm, this) == parameter_parts(vm, other))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let (executable, slot) = parameter_parts(vm, receiver(args));
        int((executable.raw() as i32)
            .wrapping_mul(31)
            .wrapping_add(slot as i32))
    }),
];

/// `java.lang.reflect.Parameter`.
pub(crate) const PARAMETER: oxjvm_vm::NativeClass = class(
    "java/lang/reflect/Parameter",
    Some("java/lang/Object"),
    &["java/lang/reflect/AnnotatedElement"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("executable", "Ljava/lang/Object;", ACC_PRIVATE, None),
        field("slot", "I", ACC_PRIVATE, None),
    ],
    &PARAMETER_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Array
// -------------------------------------------------------------------------------------------

fn primitive_array_component(base: oxjvm_classfile::BaseType) -> oxjvm_vm::ArrayComponent {
    match base {
        oxjvm_classfile::BaseType::Boolean => oxjvm_vm::ArrayComponent::Boolean,
        oxjvm_classfile::BaseType::Byte => oxjvm_vm::ArrayComponent::Byte,
        oxjvm_classfile::BaseType::Char => oxjvm_vm::ArrayComponent::Char,
        oxjvm_classfile::BaseType::Short => oxjvm_vm::ArrayComponent::Short,
        oxjvm_classfile::BaseType::Int => oxjvm_vm::ArrayComponent::Int,
        oxjvm_classfile::BaseType::Long => oxjvm_vm::ArrayComponent::Long,
        oxjvm_classfile::BaseType::Float => oxjvm_vm::ArrayComponent::Float,
        oxjvm_classfile::BaseType::Double => oxjvm_vm::ArrayComponent::Double,
        oxjvm_classfile::BaseType::Void => oxjvm_vm::ArrayComponent::Reference,
    }
}

fn array_component_descriptor(vm: &Vm<'_>, array: ObjectRef) -> &'static str {
    match vm.classes.get(vm.class_of(array)).component {
        Some(oxjvm_vm::ArrayComponent::Boolean) => "Z",
        Some(oxjvm_vm::ArrayComponent::Byte) => "B",
        Some(oxjvm_vm::ArrayComponent::Char) => "C",
        Some(oxjvm_vm::ArrayComponent::Short) => "S",
        Some(oxjvm_vm::ArrayComponent::Int) => "I",
        Some(oxjvm_vm::ArrayComponent::Long) => "J",
        Some(oxjvm_vm::ArrayComponent::Float) => "F",
        Some(oxjvm_vm::ArrayComponent::Double) => "D",
        _ => "Ljava/lang/Object;",
    }
}

fn array_new_instance(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let component = class_argument(vm, ref_arg(args, 0))?;
    let length = crate::int_arg(args, 1);
    if length < 0 {
        return Err(vm.throw_new(
            "java/lang/NegativeArraySizeException",
            Some(&length.to_string()),
        ));
    }
    let kind = vm.classes.get(component).kind;
    let array = if kind == oxjvm_vm::ClassKind::Primitive {
        let base = vm.classes.get(component).primitive;
        let base = base.ok_or_else(|| VmError::internal("primitive class without base type"))?;
        vm.allocate_array_of(primitive_array_component(base), length as usize)?
    } else {
        vm.allocate_object_array(component, length as usize)?
    };
    object(array)
}

fn array_new_instance_dimensions(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let component = class_argument(vm, ref_arg(args, 0))?;
    let dimensions = ref_arg(args, 1);
    let count = vm.array_length(dimensions)?;
    let mut values = Vec::with_capacity(count);
    for index in 0..count {
        let value = vm.array_get(dimensions, index as i32)?.as_int();
        if value < 0 {
            return Err(vm.throw_new(
                "java/lang/NegativeArraySizeException",
                Some(&value.to_string()),
            ));
        }
        values.push(value as usize);
    }
    array_allocate_dimensions(vm, component, &values).map(Value::Ref)
}

fn array_allocate_dimensions(
    vm: &mut Vm<'_>,
    component: ClassId,
    dimensions: &[usize],
) -> Result<ObjectRef, VmError> {
    if dimensions.is_empty() {
        return Err(VmError::internal("Array.newInstance with no dimensions"));
    }
    if dimensions.len() == 1 {
        let kind = vm.classes.get(component).kind;
        return if kind == oxjvm_vm::ClassKind::Primitive {
            let base = vm.classes.get(component).primitive;
            let base =
                base.ok_or_else(|| VmError::internal("primitive class without base type"))?;
            vm.allocate_array_of(primitive_array_component(base), dimensions[0])
        } else {
            vm.allocate_object_array(component, dimensions[0])
        };
    }
    let inner_component = vm.array_class_for(component)?;
    let outer = vm.allocate_object_array(inner_component, dimensions[0])?;
    for index in 0..dimensions[0] {
        let inner = array_allocate_dimensions(vm, component, &dimensions[1..])?;
        vm.array_set_ref(outer, index, inner)?;
    }
    Ok(outer)
}

fn array_get_value(vm: &mut Vm<'_>, array: ObjectRef, index: i32) -> Result<Value, VmError> {
    let value = vm.array_get(array, index)?;
    if vm.classes.get(vm.class_of(array)).kind == oxjvm_vm::ClassKind::Array {
        let descriptor = array_component_descriptor(vm, array);
        if descriptor != "Ljava/lang/Object;" {
            return crate::boxed::box_value(vm, value, descriptor);
        }
    }
    Ok(value)
}

const ARRAY_METHODS: [oxjvm_vm::NativeMethodDef; 21] = [
    method(
        "newInstance",
        "(Ljava/lang/Class;I)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        array_new_instance,
    ),
    method(
        "newInstance",
        "(Ljava/lang/Class;[I)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        array_new_instance_dimensions,
    ),
    method(
        "getLength",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| int(vm.array_length(ref_arg(args, 0))? as i32),
    ),
    method(
        "get",
        "(Ljava/lang/Object;I)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| array_get_value(vm, ref_arg(args, 0), crate::int_arg(args, 1)),
    ),
    method(
        "getBoolean",
        "(Ljava/lang/Object;I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            boolean(
                vm.array_get(ref_arg(args, 0), crate::int_arg(args, 1))?
                    .as_int()
                    != 0,
            )
        },
    ),
    method(
        "getByte",
        "(Ljava/lang/Object;I)B",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            int(vm
                .array_get(ref_arg(args, 0), crate::int_arg(args, 1))?
                .as_int())
        },
    ),
    method(
        "getChar",
        "(Ljava/lang/Object;I)C",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            int(vm
                .array_get(ref_arg(args, 0), crate::int_arg(args, 1))?
                .as_int())
        },
    ),
    method(
        "getShort",
        "(Ljava/lang/Object;I)S",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            int(vm
                .array_get(ref_arg(args, 0), crate::int_arg(args, 1))?
                .as_int())
        },
    ),
    method(
        "getInt",
        "(Ljava/lang/Object;I)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            int(vm
                .array_get(ref_arg(args, 0), crate::int_arg(args, 1))?
                .as_int())
        },
    ),
    method(
        "getLong",
        "(Ljava/lang/Object;I)J",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            long(
                vm.array_get(ref_arg(args, 0), crate::int_arg(args, 1))?
                    .as_long(),
            )
        },
    ),
    method(
        "getFloat",
        "(Ljava/lang/Object;I)F",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            float(
                vm.array_get(ref_arg(args, 0), crate::int_arg(args, 1))?
                    .as_float(),
            )
        },
    ),
    method(
        "getDouble",
        "(Ljava/lang/Object;I)D",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            double(
                vm.array_get(ref_arg(args, 0), crate::int_arg(args, 1))?
                    .as_double(),
            )
        },
    ),
    method(
        "set",
        "(Ljava/lang/Object;ILjava/lang/Object;)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let array = ref_arg(args, 0);
            let index = crate::int_arg(args, 1);
            let descriptor = array_component_descriptor(vm, array);
            let value = if descriptor == "Ljava/lang/Object;" {
                Value::Ref(ref_arg(args, 2))
            } else {
                crate::boxed::unbox_value(vm, ref_arg(args, 2), descriptor)
            };
            vm.array_set(array, index, value, true)?;
            void()
        },
    ),
    method(
        "setBoolean",
        "(Ljava/lang/Object;IZ)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            vm.array_set(
                ref_arg(args, 0),
                crate::int_arg(args, 1),
                Value::Int(crate::int_arg(args, 2)),
                true,
            )?;
            void()
        },
    ),
    method(
        "setByte",
        "(Ljava/lang/Object;IB)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            vm.array_set(
                ref_arg(args, 0),
                crate::int_arg(args, 1),
                Value::Int(crate::int_arg(args, 2)),
                true,
            )?;
            void()
        },
    ),
    method(
        "setChar",
        "(Ljava/lang/Object;IC)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            vm.array_set(
                ref_arg(args, 0),
                crate::int_arg(args, 1),
                Value::Int(crate::int_arg(args, 2)),
                true,
            )?;
            void()
        },
    ),
    method(
        "setShort",
        "(Ljava/lang/Object;IS)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            vm.array_set(
                ref_arg(args, 0),
                crate::int_arg(args, 1),
                Value::Int(crate::int_arg(args, 2)),
                true,
            )?;
            void()
        },
    ),
    method(
        "setInt",
        "(Ljava/lang/Object;II)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            vm.array_set(
                ref_arg(args, 0),
                crate::int_arg(args, 1),
                Value::Int(crate::int_arg(args, 2)),
                true,
            )?;
            void()
        },
    ),
    method(
        "setLong",
        "(Ljava/lang/Object;IJ)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            vm.array_set(
                ref_arg(args, 0),
                crate::int_arg(args, 1),
                Value::Long(crate::long_arg(args, 2)),
                true,
            )?;
            void()
        },
    ),
    method(
        "setFloat",
        "(Ljava/lang/Object;IF)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            vm.array_set(
                ref_arg(args, 0),
                crate::int_arg(args, 1),
                Value::Float(crate::float_arg(args, 2)),
                true,
            )?;
            void()
        },
    ),
    method(
        "setDouble",
        "(Ljava/lang/Object;ID)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            vm.array_set(
                ref_arg(args, 0),
                crate::int_arg(args, 1),
                Value::Double(crate::double_arg(args, 2)),
                true,
            )?;
            void()
        },
    ),
];

/// `java.lang.reflect.Array`.
pub(crate) const REFLECT_ARRAY: oxjvm_vm::NativeClass = class(
    "java/lang/reflect/Array",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &ARRAY_METHODS,
    None,
);
// -------------------------------------------------------------------------------------------
// Class lookups
// -------------------------------------------------------------------------------------------

fn declared_methods(vm: &mut Vm<'_>, class: ClassId) -> Result<Vec<(ClassId, u32)>, VmError> {
    let mut found = Vec::new();
    let count = vm.classes.get(class).methods.len();
    for slot in 0..count {
        let name = vm.classes.get(class).methods[slot].name.clone();
        if name == "<init>" || name == "<clinit>" {
            continue;
        }
        found.push((class, slot as u32));
    }
    Ok(found)
}

/// Public methods of a class, its superclasses, and its superinterfaces.
///
/// Later occurrences of a signature (from superclasses or interfaces) are dropped so that the
/// most specific declaration wins, mirroring `Class.getMethods`.
fn class_methods(vm: &mut Vm<'_>, class: ClassId) -> Result<Vec<(ClassId, u32)>, VmError> {
    let mut found = Vec::new();
    let mut seen: Vec<(String, String)> = Vec::new();
    let mut interface_roots = Vec::new();
    let mut current = Some(class);
    while let Some(id) = current {
        for entry in declared_methods(vm, id)? {
            let (name, descriptor, flags) = method_entry(vm, entry.0, entry.1);
            if flags & ACC_PUBLIC == 0 {
                continue;
            }
            let key = (name, descriptor.clone());
            if !seen.contains(&key) {
                seen.push(key);
                found.push(entry);
            }
        }
        for interface in &vm.classes.get(id).interfaces {
            interface_roots.push(*interface);
        }
        current = vm.classes.get(id).super_class;
    }
    let mut stack: Vec<ClassId> = interface_roots.into_iter().rev().collect();
    let mut visited: Vec<ClassId> = Vec::new();
    while let Some(id) = stack.pop() {
        if visited.contains(&id) {
            continue;
        }
        visited.push(id);
        for entry in declared_methods(vm, id)? {
            let (name, descriptor, flags) = method_entry(vm, entry.0, entry.1);
            if flags & ACC_PUBLIC == 0 {
                continue;
            }
            let key = (name, descriptor.clone());
            if !seen.contains(&key) {
                seen.push(key);
                found.push(entry);
            }
        }
        for interface in vm.classes.get(id).interfaces.iter().rev() {
            stack.push(*interface);
        }
    }
    Ok(found)
}

/// Fields declared directly by `class` (`Class.getDeclaredFields`): inherited fields are not
/// included.
fn declared_fields(vm: &Vm<'_>, class: ClassId) -> Vec<(ClassId, u32)> {
    let count = vm.classes.get(class).fields.len();
    (0..count).map(|index| (class, index as u32)).collect()
}

/// Public fields of a class, its superclasses, and its superinterfaces (`Class.getFields`).
///
/// Later occurrences of a signature (from superclasses or interfaces) are dropped so that the
/// most specific declaration wins.
fn class_public_fields(vm: &Vm<'_>, class: ClassId) -> Vec<(ClassId, u32)> {
    let mut found = Vec::new();
    let mut seen: Vec<(String, String)> = Vec::new();
    let mut interface_roots = Vec::new();
    let mut current = Some(class);
    while let Some(id) = current {
        for (declaring, slot) in declared_fields(vm, id) {
            let (name, descriptor, flags) = field_entry(vm, declaring, slot);
            if flags & ACC_PUBLIC == 0 {
                continue;
            }
            let key = (name, descriptor);
            if !seen.contains(&key) {
                seen.push(key);
                found.push((declaring, slot));
            }
        }
        for interface in &vm.classes.get(id).interfaces {
            interface_roots.push(*interface);
        }
        current = vm.classes.get(id).super_class;
    }
    let mut stack: Vec<ClassId> = interface_roots.into_iter().rev().collect();
    let mut visited: Vec<ClassId> = Vec::new();
    while let Some(id) = stack.pop() {
        if visited.contains(&id) {
            continue;
        }
        visited.push(id);
        for (declaring, slot) in declared_fields(vm, id) {
            let (name, descriptor, flags) = field_entry(vm, declaring, slot);
            if flags & ACC_PUBLIC == 0 {
                continue;
            }
            let key = (name, descriptor);
            if !seen.contains(&key) {
                seen.push(key);
                found.push((declaring, slot));
            }
        }
        for interface in vm.classes.get(id).interfaces.iter().rev() {
            stack.push(*interface);
        }
    }
    found
}

/// `Class.getDeclaredMethods()` / `getMethods()`.
pub(crate) fn class_reflect_methods(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    let entries = declared_methods(vm, class)?;
    let method_class = vm.resolve_class("java/lang/reflect/Method")?;
    let array = vm.allocate_object_array(method_class, entries.len())?;
    for (index, (declaring, slot)) in entries.iter().enumerate() {
        let entry = new_method(vm, *declaring, *slot)?;
        vm.array_set_ref(array, index, entry)?;
    }
    object(array)
}

pub(crate) fn class_reflect_all_methods(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    let entries = class_methods(vm, class)?;
    let method_class = vm.resolve_class("java/lang/reflect/Method")?;
    let array = vm.allocate_object_array(method_class, entries.len())?;
    for (index, (declaring, slot)) in entries.iter().enumerate() {
        let entry = new_method(vm, *declaring, *slot)?;
        vm.array_set_ref(array, index, entry)?;
    }
    object(array)
}

pub(crate) fn class_reflect_fields(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    let entries = declared_fields(vm, class);
    let field_class = vm.resolve_class("java/lang/reflect/Field")?;
    let array = vm.allocate_object_array(field_class, entries.len())?;
    for (index, (declaring, slot)) in entries.iter().enumerate() {
        let entry = new_field(vm, *declaring, *slot)?;
        vm.array_set_ref(array, index, entry)?;
    }
    object(array)
}

/// `Class.getFields()`: public fields, including inherited ones.
pub(crate) fn class_reflect_all_fields(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    let entries = class_public_fields(vm, class);
    let field_class = vm.resolve_class("java/lang/reflect/Field")?;
    let array = vm.allocate_object_array(field_class, entries.len())?;
    for (index, (declaring, slot)) in entries.iter().enumerate() {
        let entry = new_field(vm, *declaring, *slot)?;
        vm.array_set_ref(array, index, entry)?;
    }
    object(array)
}

/// `Class.getDeclaredField(String)`.
pub(crate) fn class_reflect_field(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    let name = string_arg(vm, args[1])?;
    let entries = declared_fields(vm, class);
    for (declaring, slot) in entries {
        let (candidate, _, _) = field_entry(vm, declaring, slot);
        if candidate == name {
            let entry = new_field(vm, declaring, slot)?;
            return object(entry);
        }
    }
    Err(vm.throw_new("java/lang/NoSuchFieldException", Some(&name)))
}

/// `Class.getField(String)`: public fields, including inherited ones.
pub(crate) fn class_reflect_all_field(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    let name = string_arg(vm, args[1])?;
    let entries = class_public_fields(vm, class);
    for (declaring, slot) in entries {
        let (candidate, _, _) = field_entry(vm, declaring, slot);
        if candidate == name {
            let entry = new_field(vm, declaring, slot)?;
            return object(entry);
        }
    }
    Err(vm.throw_new("java/lang/NoSuchFieldException", Some(&name)))
}

/// Build the parameter part of a method descriptor from a `Class[]` argument.
fn parameter_descriptor(vm: &mut Vm<'_>, parameter_array: ObjectRef) -> Result<String, VmError> {
    let mut descriptor = String::from("(");
    let length = if parameter_array.is_null() {
        0
    } else {
        vm.array_length(parameter_array)?
    };
    for index in 0..length {
        let value = vm.array_get(parameter_array, index as i32)?;
        let parameter = class_argument(vm, value.as_ref())?;
        descriptor.push_str(&class_field_descriptor(vm, parameter));
    }
    descriptor.push(')');
    Ok(descriptor)
}

/// `Class.getDeclaredMethod(String, Class[])`.
pub(crate) fn class_reflect_declared_method(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    let name = string_arg(vm, args[1])?;
    let descriptor = parameter_descriptor(vm, ref_arg(args, 2))?;
    for (declaring, slot) in declared_methods(vm, class)? {
        let (candidate, candidate_descriptor, _) = method_entry(vm, declaring, slot);
        if candidate == name && candidate_descriptor.starts_with(&descriptor) {
            let entry = new_method(vm, declaring, slot)?;
            return object(entry);
        }
    }

    Err(vm.throw_new("java/lang/NoSuchMethodException", Some(&name)))
}

/// `Class.getMethod(String, Class[])`.
pub(crate) fn class_reflect_method(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    let name = string_arg(vm, args[1])?;
    let descriptor = parameter_descriptor(vm, ref_arg(args, 2))?;
    for (declaring, slot) in class_methods(vm, class)? {
        let (candidate, candidate_descriptor, _) = method_entry(vm, declaring, slot);
        if candidate == name && candidate_descriptor.starts_with(&descriptor) {
            let entry = new_method(vm, declaring, slot)?;
            return object(entry);
        }
    }

    Err(vm.throw_new("java/lang/NoSuchMethodException", Some(&name)))
}
// -------------------------------------------------------------------------------------------
// ProtectionDomain (stub)
// -------------------------------------------------------------------------------------------

const PERMISSION_COLLECTION_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "implies",
        "(Ljava/security/Permission;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(false),
    ),
    method(
        "add",
        "(Ljava/security/Permission;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "elements",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC,
        |vm, _, _| object(crate::util::new_enumeration(vm, &[])?),
    ),
    method("isReadOnly", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
];

/// `java.security.PermissionCollection`: an empty, read-only collection.
pub(crate) const PERMISSION_COLLECTION: oxjvm_vm::NativeClass = class(
    "java/security/PermissionCollection",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &PERMISSION_COLLECTION_METHODS,
    None,
);

const PERMISSIONS_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "implies",
        "(Ljava/security/Permission;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(false),
    ),
    method(
        "add",
        "(Ljava/security/Permission;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "elements",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC,
        |vm, _, _| object(crate::util::new_enumeration(vm, &[])?),
    ),
    method("isReadOnly", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
];

/// `java.security.Permissions`: the concrete empty permission collection.
pub(crate) const PERMISSIONS: oxjvm_vm::NativeClass = class(
    "java/security/Permissions",
    Some("java/security/PermissionCollection"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &PERMISSIONS_METHODS,
    None,
);

const PROTECTION_DOMAIN_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "getPermissions",
        "()Ljava/security/PermissionCollection;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/security/Permissions")?;
            object(vm.new_instance(class)?)
        },
    ),
    method(
        "getCodeSource",
        "()Ljava/security/CodeSource;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "implies",
        "(Ljava/security/Permission;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(false),
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("ProtectionDomain")?),
    ),
];

/// `java.security.ProtectionDomain`: a domain without code source or permissions.
pub(crate) const PROTECTION_DOMAIN: oxjvm_vm::NativeClass = class(
    "java/security/ProtectionDomain",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &PROTECTION_DOMAIN_METHODS,
    None,
);
