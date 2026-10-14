//! Minimal `java.lang.reflect`: `Method` and `Field`, plus the `Class` lookups.
//!
//! Methods/fields are views into the VM's class metadata: a `Method` holds its
//! declaring `Class` and the method-table slot, a `Field` the field slot.  No
//! annotation metadata is modelled (`getAnnotationsByType` returns an empty
//! array), and `setAccessible` is a no-op (everything is accessible).

use alloc::string::String;

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

const METHOD_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
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
            let (name, _, _) = method_entry(vm, class, slot);
            let text = alloc::format!("{}.{name}", vm.class_name(class).replace('/', "."));
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "getAnnotationsByType",
        "(Ljava/lang/Class;)[Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let annotation = class_argument(vm, ref_arg(args, 1))?;
            let array = vm.allocate_object_array(annotation, 0)?;
            object(array)
        },
    ),
];

fn reflect_invoke(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let (class, slot) = method_parts(vm, receiver(args));
    let (_name, descriptor, flags) = method_entry(vm, class, slot);
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
    let result = vm.invoke_method(class, slot, call_args)?;
    let Some(returns) = parsed.returns else {
        return object(ObjectRef::NULL);
    };

    crate::boxed::box_value(vm, result, &field_type_descriptor(&returns))
}

pub(crate) const METHOD: oxjvm_vm::NativeClass = class(
    "java/lang/reflect/Method",
    Some("java/lang/Object"),
    &["java/lang/reflect/Member"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("clazz", "Ljava/lang/Class;", ACC_PRIVATE, None),
        field("slot", "I", ACC_PRIVATE, None),
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
    let value = if primitive_descriptor(&descriptor) {
        crate::boxed::unbox_value(vm, ref_arg(args, 2), &descriptor)
    } else {
        Value::Ref(ref_arg(args, 2))
    };
    if flags & ACC_STATIC != 0 {
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
    if flags & ACC_STATIC != 0 {
        vm.classes.get_mut(class).static_values[storage as usize] = value;
    } else {
        set_instance_value(vm, target, storage, value)?;
    }
    void()
}

const FIELD_METHODS: [oxjvm_vm::NativeMethodDef; 19] = [
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
    method("setAccessible", "(Z)V", ACC_PUBLIC, |_, _, _| void()),
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
        "getAnnotationsByType",
        "(Ljava/lang/Class;)[Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let annotation = class_argument(vm, ref_arg(args, 1))?;
            let array = vm.allocate_object_array(annotation, 0)?;
            object(array)
        },
    ),
];

pub(crate) const FIELD: oxjvm_vm::NativeClass = class(
    "java/lang/reflect/Field",
    Some("java/lang/Object"),
    &["java/lang/reflect/Member"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("clazz", "Ljava/lang/Class;", ACC_PRIVATE, None),
        field("slot", "I", ACC_PRIVATE, None),
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

fn class_methods(vm: &mut Vm<'_>, class: ClassId) -> Result<Vec<(ClassId, u32)>, VmError> {
    let mut found = Vec::new();
    let mut current = Some(class);
    while let Some(id) = current {
        for entry in declared_methods(vm, id)? {
            found.push(entry);
        }
        current = vm.classes.get(id).super_class;
    }
    Ok(found)
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

/// `Class.getDeclaredMethod(String, Class[])` / `getMethod(...)`.
pub(crate) fn class_reflect_method(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    let name = string_arg(vm, args[1])?;
    let parameter_array = ref_arg(args, 2);
    let mut descriptor = String::from("(");
    let length = vm.array_length(parameter_array)?;
    for index in 0..length {
        let value = vm.array_get(parameter_array, index as i32)?;
        let parameter = class_argument(vm, value.as_ref())?;
        descriptor.push_str(&class_field_descriptor(vm, parameter));
    }
    descriptor.push(')');
    let entries = class_methods(vm, class)?;
    for (declaring, slot) in entries {
        let (candidate, candidate_descriptor, _) = method_entry(vm, declaring, slot);
        if candidate == name && candidate_descriptor.starts_with(&descriptor) {
            let entry = new_method(vm, declaring, slot)?;
            return object(entry);
        }
    }

    Err(vm.throw_new("java/lang/NoSuchMethodException", Some(&name)))
}
