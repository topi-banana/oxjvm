//! Minimal `java.lang.reflect`: `Method`, `Field`, `Constructor`, and the
//! `Class` lookups.
//!
//! Members are views into the VM's class metadata: each holds its declaring
//! `Class` and the method/field-table slot.  Runtime-visible annotations are
//! materialised by [`crate::annotation`]; `setAccessible` is a no-op
//! (everything is accessible).

use alloc::string::{String, ToString};

use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_classfile::{FieldType, MethodDescriptor};

use crate::invoke::class_field_descriptor;
use crate::lang::class_argument;
use crate::{boolean, class, field, int, method, object, receiver, ref_arg, string_arg, void};
use oxjvm_vm::{ClassId, ObjectRef, Value, Vm, VmError};

// -------------------------------------------------------------------------------------------
// Shared helpers
// -------------------------------------------------------------------------------------------

fn field_type_descriptor(field_type: &FieldType) -> String {
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

fn new_method(vm: &mut Vm<'_>, class: ClassId, slot: u32) -> Result<ObjectRef, VmError> {
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
fn class_object_target(vm: &Vm<'_>, this: ObjectRef) -> ClassId {
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

const METHOD_METHODS: [oxjvm_vm::NativeMethodDef; 20] = [
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
            object(vm.make_string(&text)?)
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
];

fn reflect_invoke(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let (class, slot) = method_parts(vm, receiver(args));
    let (_name, descriptor, flags) = method_entry(vm, class, slot);
    if flags & ACC_PRIVATE != 0 && !reflection_override(vm, receiver(args)) {
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

pub(crate) const METHOD: oxjvm_vm::NativeClass = class(
    "java/lang/reflect/Method",
    Some("java/lang/Object"),
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

const FIELD_METHODS: [oxjvm_vm::NativeMethodDef; 32] = [
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
            let (_, descriptor, _) = field_entry(vm, class, slot);
            let field_type = FieldType::parse(&descriptor)
                .map_err(|_| VmError::internal("bad field descriptor"))?;
            let class_id = field_type_class(vm, &field_type)?;
            object(vm.class_object(class_id)?)
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
];

pub(crate) const FIELD: oxjvm_vm::NativeClass = class(
    "java/lang/reflect/Field",
    Some("java/lang/Object"),
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

const CONSTRUCTOR_METHODS: [oxjvm_vm::NativeMethodDef; 19] = [
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
    Some("java/lang/Object"),
    &[
        "java/lang/reflect/Member",
        "java/lang/reflect/AnnotatedElement",
    ],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("clazz", "Ljava/lang/Class;", ACC_PRIVATE, None),
        field("slot", "I", ACC_PRIVATE, None),
    ],
    &CONSTRUCTOR_METHODS,
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

const PARAMETER_METHODS: [oxjvm_vm::NativeMethodDef; 15] = [
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
            let key = (name, signature_of(&descriptor));
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
            let key = (name, signature_of(&descriptor));
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

/// The name-and-parameters key used to deduplicate overridden methods.
fn signature_of(descriptor: &str) -> String {
    match descriptor.find(')') {
        Some(index) => descriptor[..=index].to_string(),
        None => descriptor.to_string(),
    }
}

fn class_fields(vm: &mut Vm<'_>, class: ClassId) -> Result<Vec<(ClassId, u32)>, VmError> {
    let mut found = Vec::new();
    let mut current = Some(class);
    while let Some(id) = current {
        let count = vm.classes.get(id).fields.len();
        for index in 0..count {
            found.push((id, index as u32));
        }
        current = vm.classes.get(id).super_class;
    }
    Ok(found)
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
    let entries = class_fields(vm, class)?;
    let field_class = vm.resolve_class("java/lang/reflect/Field")?;
    let array = vm.allocate_object_array(field_class, entries.len())?;
    for (index, (declaring, slot)) in entries.iter().enumerate() {
        let entry = new_field(vm, *declaring, *slot)?;
        vm.array_set_ref(array, index, entry)?;
    }
    object(array)
}

/// `Class.getDeclaredField(String)` / `getField(String)`.
pub(crate) fn class_reflect_field(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    let name = string_arg(vm, args[1])?;
    let entries = class_fields(vm, class)?;
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
