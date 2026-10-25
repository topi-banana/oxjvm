//! `java.lang.invoke`: method handles, method types, lookups, and the bootstrap-method stubs.
//!
//! `LambdaMetafactory`, `StringConcatFactory`, and `ObjectMethods` are never *called*: the runtime
//! intercepts `invokedynamic` at resolution time. The classes exist so method references to their
//! bootstrap methods resolve, and their implementations raise `BootstrapMethodError` if called
//! directly.

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{MethodHandleValue, ObjectRef, Value, Vm, VmError};

use crate::{
    boolean, class, field, int, long, method, object, receiver, ref_arg, string_arg, void,
};

// -------------------------------------------------------------------------------------------
// MethodType
// -------------------------------------------------------------------------------------------

const METHOD_TYPE_METHODS: [oxjvm_vm::NativeMethodDef; 17] = [
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
    method("parameterCount", "()I", ACC_PUBLIC, |vm, _, args| {
        let descriptor = vm.method_type_of(receiver(args)).unwrap_or_default();
        Ok(Value::Int(descriptor_types(&descriptor).len() as i32))
    }),
    method(
        "parameterType",
        "(I)Ljava/lang/Class;",
        ACC_PUBLIC,
        |vm, _, args| {
            let descriptor = vm.method_type_of(receiver(args)).unwrap_or_default();
            let types = descriptor_types(&descriptor);
            let index = crate::int_arg(args, 1);
            if index < 0 || index as usize >= types.len() {
                return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
            }
            class_object_for_descriptor(vm, &types[index as usize])
        },
    ),
    method(
        "returnType",
        "()Ljava/lang/Class;",
        ACC_PUBLIC,
        |vm, _, args| {
            let descriptor = vm.method_type_of(receiver(args)).unwrap_or_default();
            let ret = descriptor
                .split_once(')')
                .map_or("V", |(_, rest)| rest)
                .to_string();
            class_object_for_descriptor(vm, &ret)
        },
    ),
    method(
        "methodType",
        "(Ljava/lang/Class;)Ljava/lang/invoke/MethodType;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let return_type = class_argument(vm, ref_arg(args, 0))?;
            method_type_from(vm, return_type, &[])
        },
    ),
    method(
        "methodType",
        "(Ljava/lang/Class;Ljava/lang/Class;)Ljava/lang/invoke/MethodType;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let return_type = class_argument(vm, ref_arg(args, 0))?;
            let parameter = class_argument(vm, ref_arg(args, 1))?;
            method_type_from(vm, return_type, &[parameter])
        },
    ),
    method(
        "methodType",
        "(Ljava/lang/Class;[Ljava/lang/Class;)Ljava/lang/invoke/MethodType;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let return_type = class_argument(vm, ref_arg(args, 0))?;
            let array = ref_arg(args, 1);
            let mut parameters = Vec::new();
            let length = vm.array_length(array)?;
            for index in 0..length {
                let value = vm.array_get(array, index as i32)?;
                parameters.push(class_argument(vm, value.as_ref())?);
            }
            method_type_from(vm, return_type, &parameters)
        },
    ),
    method(
        "methodType",
        "(Ljava/lang/Class;Ljava/util/List;)Ljava/lang/invoke/MethodType;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let return_type = class_argument(vm, ref_arg(args, 0))?;
            let list = ref_arg(args, 1);
            let mut parameters = Vec::new();
            for element in crate::util::collection_elements(vm, list)? {
                parameters.push(class_argument(vm, element)?);
            }
            method_type_from(vm, return_type, &parameters)
        },
    ),
    method(
        "methodType",
        "(Ljava/lang/Class;Ljava/lang/invoke/MethodType;)Ljava/lang/invoke/MethodType;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let return_type = class_argument(vm, ref_arg(args, 0))?;
            let parameter_type = ref_arg(args, 1);
            let descriptor = vm.method_type_of(parameter_type).unwrap_or_default();
            let parameters = descriptor_params(&descriptor);
            let mut result = String::from("(");
            result.push_str(&parameters);
            result.push(')');
            result.push_str(&class_field_descriptor(vm, return_type));
            object(vm.make_method_type(&result)?)
        },
    ),
    method(
        "methodType",
        "(Ljava/lang/Class;Ljava/lang/Class;[Ljava/lang/Class;)Ljava/lang/invoke/MethodType;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let return_type = class_argument(vm, ref_arg(args, 0))?;
            let first = class_argument(vm, ref_arg(args, 1))?;
            let array = ref_arg(args, 2);
            let mut parameters = Vec::new();
            parameters.push(first);
            let length = vm.array_length(array)?;
            for index in 0..length {
                let value = vm.array_get(array, index as i32)?;
                parameters.push(class_argument(vm, value.as_ref())?);
            }
            method_type_from(vm, return_type, &parameters)
        },
    ),
    method(
        "parameterList",
        "()Ljava/util/List;",
        ACC_PUBLIC,
        |vm, _, args| {
            let descriptor = vm.method_type_of(receiver(args)).unwrap_or_default();
            let types = descriptor_types(&descriptor);
            let mut classes = Vec::new();
            for descriptor in types {
                let class = class_object_for_descriptor(vm, &descriptor)?.as_ref();
                classes.push(class);
            }
            object(crate::util::array_list_from(vm, &classes)?)
        },
    ),
    method(
        "genericMethodType",
        "(I)Ljava/lang/invoke/MethodType;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let count = crate::int_arg(args, 0).max(0) as usize;
            let mut descriptor = String::from("(");
            for _ in 0..count {
                descriptor.push_str("Ljava/lang/Object;");
            }
            descriptor.push_str(")Ljava/lang/Object;");
            object(vm.make_method_type(&descriptor)?)
        },
    ),
    method(
        "changeReturnType",
        "(Ljava/lang/Class;)Ljava/lang/invoke/MethodType;",
        ACC_PUBLIC,
        |vm, _, args| {
            let descriptor = vm.method_type_of(receiver(args)).unwrap_or_default();
            let params = descriptor_params(&descriptor);
            let class = class_argument(vm, ref_arg(args, 1))?;
            let result = alloc::format!("{params}{}", class_field_descriptor(vm, class));
            object(vm.make_method_type(&result)?)
        },
    ),
    method(
        "appendParameterTypes",
        "(Ljava/util/List;)Ljava/lang/invoke/MethodType;",
        ACC_PUBLIC,
        |vm, _, args| {
            let descriptor = vm.method_type_of(receiver(args)).unwrap_or_default();
            let mut params = descriptor_params(&descriptor);
            for element in crate::util::collection_elements(vm, ref_arg(args, 1))? {
                let class = class_argument(vm, element)?;
                params.push_str(&class_field_descriptor(vm, class));
            }
            let result = alloc::format!(
                "{params}{}",
                descriptor.split_once(')').map_or("V", |(_, r)| r)
            );
            object(vm.make_method_type(&result)?)
        },
    ),
    method(
        "describeConstable",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, args| crate::optional::optional_new(vm, receiver(args)),
    ),
];

/// The parameter part (including parentheses) of a method descriptor.
/// The parameter descriptors of a method descriptor.
fn descriptor_types(descriptor: &str) -> Vec<String> {
    let chars: Vec<char> = descriptor.chars().collect();
    let mut types = Vec::new();
    if chars.first() != Some(&'(') {
        return types;
    }
    let mut index = 1;
    while index < chars.len() && chars[index] != ')' {
        let start = index;
        while index < chars.len() && chars[index] == '[' {
            index += 1;
        }
        if index < chars.len() && chars[index] == 'L' {
            while index < chars.len() && chars[index] != ';' {
                index += 1;
            }
        }
        index += 1;
        types.push(chars[start..index.min(chars.len())].iter().collect());
    }
    types
}

/// The `Class` object for a field descriptor.
fn class_object_for_descriptor(vm: &mut Vm<'_>, descriptor: &str) -> Result<Value, VmError> {
    let base = match descriptor {
        "Z" => Some(oxjvm_classfile::BaseType::Boolean),
        "B" => Some(oxjvm_classfile::BaseType::Byte),
        "C" => Some(oxjvm_classfile::BaseType::Char),
        "S" => Some(oxjvm_classfile::BaseType::Short),
        "I" => Some(oxjvm_classfile::BaseType::Int),
        "J" => Some(oxjvm_classfile::BaseType::Long),
        "F" => Some(oxjvm_classfile::BaseType::Float),
        "D" => Some(oxjvm_classfile::BaseType::Double),
        _ => None,
    };
    if let Some(base) = base {
        let class = vm.primitive_class(base)?;
        return object(vm.class_object(class)?);
    }
    if let Some(name) = descriptor
        .strip_prefix('L')
        .and_then(|rest| rest.strip_suffix(';'))
    {
        let class = vm.resolve_class(name)?;
        return object(vm.class_object(class)?);
    }
    let class = vm.resolve_class("java/lang/Object")?;
    object(vm.class_object(class)?)
}

fn descriptor_params(descriptor: &str) -> String {
    match descriptor.find(')') {
        Some(index) => descriptor[..=index].to_string(),
        None => "()".to_string(),
    }
}

/// Build a `MethodType` from a return class and parameter classes.
fn method_type_from(
    vm: &mut Vm<'_>,
    return_type: oxjvm_vm::ClassId,
    parameters: &[oxjvm_vm::ClassId],
) -> Result<Value, VmError> {
    let mut descriptor = String::from("(");
    for parameter in parameters {
        descriptor.push_str(&class_field_descriptor(vm, *parameter));
    }
    descriptor.push(')');
    descriptor.push_str(&class_field_descriptor(vm, return_type));
    object(vm.make_method_type(&descriptor)?)
}

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

const METHOD_HANDLE_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
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
    method(
        "type",
        "()Ljava/lang/invoke/MethodType;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_method_type("(Ljava/lang/Object;)Ljava/lang/Object;")?),
    ),
    method(
        "bindTo",
        "(Ljava/lang/Object;)Ljava/lang/invoke/MethodHandle;",
        ACC_PUBLIC,
        |vm, _, args| {
            let target = vm
                .method_handle_of(receiver(args))
                .ok_or_else(|| VmError::internal("not a handle"))?;
            let bound = MethodHandleValue::Bound {
                receiver: ref_arg(args, 1),
                target: alloc::boxed::Box::new(target),
            };
            object(vm.make_method_handle(bound)?)
        },
    ),
    method(
        "invokeWithArguments",
        "([Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_VARARGS,
        |vm, _, args| {
            let handle = vm
                .method_handle_of(receiver(args))
                .ok_or_else(|| VmError::internal("not a handle"))?;
            let array = ref_arg(args, 1);
            let length = vm.array_length(array)?;
            let mut values = Vec::new();
            for index in 0..length {
                values.push(vm.array_get(array, index as i32)?);
            }
            vm.invoke_handle(&handle, &values)
        },
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

const METHOD_HANDLES_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
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
    method(
        "privateLookupIn",
        "(Ljava/lang/Class;Ljava/lang/invoke/MethodHandles$Lookup;)Ljava/lang/invoke/MethodHandles$Lookup;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| lookup_with_class(vm, ref_arg(args, 0)),
    ),
    method(
        "constant",
        "(Ljava/lang/Class;Ljava/lang/Object;)Ljava/lang/invoke/MethodHandle;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let class = class_argument(vm, ref_arg(args, 0))?;
            object(vm.make_method_handle(MethodHandleValue::Identity { class })?)
        },
    ),
    method(
        "arrayElementVarHandle",
        "(Ljava/lang/Class;)Ljava/lang/invoke/VarHandle;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| new_var_handle(vm, ref_arg(args, 0), ObjectRef::NULL, ObjectRef::NULL, 1),
    ),
    method(
        "byteArrayViewVarHandle",
        "(Ljava/lang/Class;Ljava/nio/ByteOrder;)Ljava/lang/invoke/VarHandle;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| new_var_handle(vm, ref_arg(args, 0), ObjectRef::NULL, ObjectRef::NULL, 2),
    ),
    method(
        "arrayLength",
        "(Ljava/lang/Class;)Ljava/lang/invoke/MethodHandle;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _args| {
            let array_class = vm.resolve_class("java/lang/reflect/Array")?;
            let handle = vm.find_method_handle_target(
                array_class,
                "getLength",
                "(Ljava/lang/Object;)I",
                true,
                false,
                false,
            )?;
            object(vm.make_method_handle(handle)?)
        },
    ),
    method(
        "tryFinally",
        "(Ljava/lang/invoke/MethodHandle;Ljava/lang/invoke/MethodHandle;)Ljava/lang/invoke/MethodHandle;",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| object(ref_arg(args, 0)),
    ),
    method(
        "spreadInvoker",
        "(Ljava/lang/invoke/MethodType;I)Ljava/lang/invoke/MethodHandle;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            object(vm.make_method_handle(MethodHandleValue::Identity {
                class: oxjvm_vm::ClassId::OBJECT,
            })?)
        },
    ),
    method(
        "foldArguments",
        "(Ljava/lang/invoke/MethodHandle;Ljava/lang/invoke/MethodHandle;)Ljava/lang/invoke/MethodHandle;",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| object(ref_arg(args, 0)),
    ),
    method(
        "filterArguments",
        "(Ljava/lang/invoke/MethodHandle;I[Ljava/lang/invoke/MethodHandle;)Ljava/lang/invoke/MethodHandle;",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| object(ref_arg(args, 0)),
    ),
    method(
        "explicitCastArguments",
        "(Ljava/lang/invoke/MethodHandle;Ljava/lang/invoke/MethodType;)Ljava/lang/invoke/MethodHandle;",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| object(ref_arg(args, 0)),
    ),
    method(
        "dropArguments",
        "(Ljava/lang/invoke/MethodHandle;I[Ljava/lang/Class;)Ljava/lang/invoke/MethodHandle;",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| object(ref_arg(args, 0)),
    ),
];

fn lookup_singleton(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    _args: &[Value],
) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/lang/invoke/MethodHandles$Lookup")?;
    let existing = vm.read_static_ref_if_set(
        class,
        "IMPL_LOOKUP",
        "Ljava/lang/invoke/MethodHandles$Lookup;",
    );
    if let Some(existing) = existing {
        return object(existing);
    }
    let lookup = vm.new_instance(class)?;
    if let (Ok(object_class), Some((declaring, field))) = (
        vm.class_object(oxjvm_vm::ClassId::OBJECT),
        vm.find_field_opt(class, "lookupClass", "Ljava/lang/Class;"),
    ) {
        vm.set_instance_ref(lookup, declaring, field, object_class);
    }
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

const LOOKUP_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
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
    method(
        "lookupClass",
        "()Ljava/lang/Class;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(
                vm.read_ref_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "lookupClass",
                    "Ljava/lang/Class;",
                )
                .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method(
        "in",
        "(Ljava/lang/Class;)Ljava/lang/invoke/MethodHandles$Lookup;",
        ACC_PUBLIC,
        |vm, _, args| lookup_with_class(vm, ref_arg(args, 1)),
    ),
    method(
        "dropLookupMode",
        "(I)Ljava/lang/invoke/MethodHandles$Lookup;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method(
        "findStaticGetter",
        "(Ljava/lang/Class;Ljava/lang/String;Ljava/lang/Class;)Ljava/lang/invoke/MethodHandle;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = class_argument(vm, ref_arg(args, 1))?;
            let name = string_arg(vm, args[2])?;
            let field_type = class_argument(vm, ref_arg(args, 3))?;
            let descriptor = class_field_descriptor(vm, field_type);
            let (declaring, field) = vm.find_field(class, &name, &descriptor)?;
            object(vm.make_method_handle(MethodHandleValue::StaticGetter {
                class: declaring,
                field,
            })?)
        },
    ),
    method(
        "unreflect",
        "(Ljava/lang/reflect/Method;)Ljava/lang/invoke/MethodHandle;",
        ACC_PUBLIC,
        |vm, _, args| {
            let method = ref_arg(args, 1);
            let class_object = vm
                .read_ref_field(method, vm.class_of(method), "clazz", "Ljava/lang/Class;")
                .unwrap_or(ObjectRef::NULL);
            let class = class_argument(vm, class_object)?;
            let slot = crate::lang::read_int_field(vm, method, "slot").max(0) as u32;
            let definition = &vm.classes.get(class).methods[slot as usize];
            let value = if definition.is_static() {
                MethodHandleValue::Static {
                    class,
                    method: slot,
                }
            } else {
                MethodHandleValue::Virtual {
                    class,
                    method: slot,
                }
            };
            object(vm.make_method_handle(value)?)
        },
    ),
    method(
        "findVarHandle",
        "(Ljava/lang/Class;Ljava/lang/String;Ljava/lang/Class;)Ljava/lang/invoke/VarHandle;",
        ACC_PUBLIC,
        |vm, _, args| new_var_handle(vm, ref_arg(args, 1), ref_arg(args, 2), ref_arg(args, 3), 0),
    ),
    method(
        "findStaticVarHandle",
        "(Ljava/lang/Class;Ljava/lang/String;Ljava/lang/Class;)Ljava/lang/invoke/VarHandle;",
        ACC_PUBLIC,
        |vm, _, args| new_var_handle(vm, ref_arg(args, 1), ref_arg(args, 2), ref_arg(args, 3), 3),
    ),
    method(
        "revealDirect",
        "(Ljava/lang/invoke/MethodHandle;)Ljava/lang/invoke/MethodHandleInfo;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
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

pub(crate) fn class_field_descriptor(vm: &mut Vm<'_>, class: oxjvm_vm::ClassId) -> String {
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
    &[
        field(
            "IMPL_LOOKUP",
            "Ljava/lang/invoke/MethodHandles$Lookup;",
            ACC_PRIVATE | ACC_STATIC | ACC_SYNTHETIC,
            None,
        ),
        field("lookupClass", "Ljava/lang/Class;", ACC_PRIVATE, None),
    ],
    &LOOKUP_METHODS,
    None,
);

fn lookup_with_class(vm: &mut Vm<'_>, class_object: ObjectRef) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/lang/invoke/MethodHandles$Lookup")?;
    let lookup = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "lookupClass", "Ljava/lang/Class;")?;
    vm.set_instance_ref(lookup, declaring, field, class_object);
    object(lookup)
}

fn new_var_handle(
    vm: &mut Vm<'_>,
    class_object: ObjectRef,
    name_object: ObjectRef,
    field_type: ObjectRef,
    kind: i32,
) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/lang/invoke/VarHandle")?;
    let handle = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "targetClass", "Ljava/lang/Class;")?;
    vm.set_instance_ref(handle, declaring, field, class_object);
    let name = vm.string_or_empty(name_object);
    vm.set_string_field(handle, class, "name", &name)?;
    let (declaring, field) = vm.find_field(class, "kind", "I")?;
    vm.set_instance_int(handle, declaring, field, kind);
    if !field_type.is_null() {
        let type_class = class_argument(vm, field_type)?;
        let descriptor = class_field_descriptor(vm, type_class);
        vm.set_string_field(handle, class, "descriptor", &descriptor)?;
        let type_object = vm.class_object(type_class)?;
        let (declaring, field) = vm.find_field(class, "fieldType", "Ljava/lang/Class;")?;
        vm.set_instance_ref(handle, declaring, field, type_object);
    }
    object(handle)
}

fn var_handle_parts(
    vm: &mut Vm<'_>,
    this: ObjectRef,
) -> Result<(oxjvm_vm::ClassId, u32, String, String), VmError> {
    let class_object = vm
        .read_ref_field(this, vm.class_of(this), "targetClass", "Ljava/lang/Class;")
        .unwrap_or(ObjectRef::NULL);
    let class = class_argument(vm, class_object)?;
    let name = vm
        .read_string_field(this, vm.class_of(this), "name")
        .unwrap_or_default();
    let descriptor = vm
        .read_string_field(this, vm.class_of(this), "descriptor")
        .unwrap_or_else(|| "Ljava/lang/Object;".into());
    let (declaring, field) = vm.find_field(class, &name, &descriptor)?;
    Ok((declaring, field, name, descriptor))
}

fn var_handle_get(vm: &mut Vm<'_>, this: ObjectRef, target: ObjectRef) -> Result<Value, VmError> {
    let (declaring, field, name, descriptor) = var_handle_parts(vm, this)?;
    match descriptor.as_str() {
        d if d.starts_with('L') || d.starts_with('[') => Ok(Value::Ref(
            vm.read_ref_field(target, declaring, &name, d)
                .unwrap_or(ObjectRef::NULL),
        )),
        "J" => crate::boxed::box_value(
            vm,
            Value::Long(
                vm.read_instance_long_named(target, declaring, &name, &descriptor)
                    .unwrap_or(0),
            ),
            "J",
        ),
        "D" => crate::boxed::box_value(
            vm,
            Value::Long(
                vm.read_instance_long_named(target, declaring, &name, &descriptor)
                    .unwrap_or(0),
            ),
            "D",
        ),
        "F" => crate::boxed::box_value(
            vm,
            Value::Int(vm.read_instance_int(target, declaring, field).unwrap_or(0)),
            "F",
        ),
        _ => crate::boxed::box_value(
            vm,
            Value::Int(vm.read_instance_int(target, declaring, field).unwrap_or(0)),
            &descriptor,
        ),
    }
}

fn var_handle_set(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    target: ObjectRef,
    value: Value,
) -> Result<(), VmError> {
    let (declaring, field, _name, descriptor) = var_handle_parts(vm, this)?;
    let value = match value {
        Value::Ref(reference) if !(descriptor.starts_with('L') || descriptor.starts_with('[')) => {
            crate::boxed::unbox_value(vm, reference, &descriptor)
        }
        other => other,
    };
    match descriptor.as_str() {
        d if d.starts_with('L') || d.starts_with('[') => {
            vm.set_instance_ref(target, declaring, field, value.as_ref());
        }
        "J" | "D" => vm.set_instance_long(target, declaring, field, value.as_long()),
        _ => vm.set_instance_int(target, declaring, field, value.as_int()),
    }
    Ok(())
}

const VAR_HANDLE_METHODS: [oxjvm_vm::NativeMethodDef; 32] = [
    method(
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| var_handle_get(vm, receiver(args), ref_arg(args, 1)),
    ),
    method(
        "getVolatile",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| var_handle_get(vm, receiver(args), ref_arg(args, 1)),
    ),
    method(
        "getAcquire",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| var_handle_get(vm, receiver(args), ref_arg(args, 1)),
    ),
    method(
        "getOpaque",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| var_handle_get(vm, receiver(args), ref_arg(args, 1)),
    ),
    method(
        "set",
        "(Ljava/lang/Object;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            var_handle_set(
                vm,
                receiver(args),
                ref_arg(args, 1),
                Value::Ref(ref_arg(args, 2)),
            )?;
            void()
        },
    ),
    method(
        "setVolatile",
        "(Ljava/lang/Object;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            var_handle_set(
                vm,
                receiver(args),
                ref_arg(args, 1),
                Value::Ref(ref_arg(args, 2)),
            )?;
            void()
        },
    ),
    method(
        "setRelease",
        "(Ljava/lang/Object;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            var_handle_set(
                vm,
                receiver(args),
                ref_arg(args, 1),
                Value::Ref(ref_arg(args, 2)),
            )?;
            void()
        },
    ),
    method(
        "setOpaque",
        "(Ljava/lang/Object;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            var_handle_set(
                vm,
                receiver(args),
                ref_arg(args, 1),
                Value::Ref(ref_arg(args, 2)),
            )?;
            void()
        },
    ),
    method(
        "getAndSet",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let old = var_handle_get(vm, receiver(args), ref_arg(args, 1))?;
            var_handle_set(
                vm,
                receiver(args),
                ref_arg(args, 1),
                Value::Ref(ref_arg(args, 2)),
            )?;
            object(old.as_ref())
        },
    ),
    method(
        "compareAndSet",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let old = var_handle_get(vm, receiver(args), ref_arg(args, 1))?;
            if old == Value::Ref(ref_arg(args, 3)) {
                var_handle_set(
                    vm,
                    receiver(args),
                    ref_arg(args, 1),
                    Value::Ref(ref_arg(args, 2)),
                )?;
                boolean(true)
            } else {
                boolean(false)
            }
        },
    ),
    method(
        "weakCompareAndSet",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let old = var_handle_get(vm, receiver(args), ref_arg(args, 1))?;
            if old == Value::Ref(ref_arg(args, 3)) {
                var_handle_set(
                    vm,
                    receiver(args),
                    ref_arg(args, 1),
                    Value::Ref(ref_arg(args, 2)),
                )?;
                boolean(true)
            } else {
                boolean(false)
            }
        },
    ),
    method(
        "compareAndExchange",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let old = var_handle_get(vm, receiver(args), ref_arg(args, 1))?;
            if old == Value::Ref(ref_arg(args, 3)) {
                var_handle_set(
                    vm,
                    receiver(args),
                    ref_arg(args, 1),
                    Value::Ref(ref_arg(args, 2)),
                )?;
            }
            object(old.as_ref())
        },
    ),
    method(
        "getAndAdd",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let old = var_handle_get(vm, receiver(args), ref_arg(args, 1))?;
            if let Value::Int(old) = old {
                let addition = crate::boxed::unbox_value(vm, ref_arg(args, 2), "I").as_int();
                var_handle_set(
                    vm,
                    receiver(args),
                    ref_arg(args, 1),
                    Value::Int(old.wrapping_add(addition)),
                )?;
                crate::boxed::box_value(vm, Value::Int(old), "I")
            } else {
                object(old.as_ref())
            }
        },
    ),
    method("get", "(Ljava/lang/Object;)I", ACC_PUBLIC, |vm, _, args| {
        let value = var_handle_get(vm, receiver(args), ref_arg(args, 1))?;
        int(crate::boxed::unbox_value(vm, value.as_ref(), "I").as_int())
    }),
    method("get", "(Ljava/lang/Object;)J", ACC_PUBLIC, |vm, _, args| {
        let value = var_handle_get(vm, receiver(args), ref_arg(args, 1))?;
        long(crate::boxed::unbox_value(vm, value.as_ref(), "J").as_long())
    }),
    method("get", "(Ljava/lang/Object;)Z", ACC_PUBLIC, |vm, _, args| {
        let value = var_handle_get(vm, receiver(args), ref_arg(args, 1))?;
        boolean(crate::boxed::unbox_value(vm, value.as_ref(), "Z").as_int() != 0)
    }),
    method("get", "(Ljava/lang/Object;)F", ACC_PUBLIC, |vm, _, args| {
        let value = var_handle_get(vm, receiver(args), ref_arg(args, 1))?;
        Ok(Value::Float(f32::from_bits(
            crate::boxed::unbox_value(vm, value.as_ref(), "F").as_int() as u32,
        )))
    }),
    method("get", "(Ljava/lang/Object;)D", ACC_PUBLIC, |vm, _, args| {
        let value = var_handle_get(vm, receiver(args), ref_arg(args, 1))?;
        Ok(Value::Double(f64::from_bits(
            crate::boxed::unbox_value(vm, value.as_ref(), "D").as_long() as u64,
        )))
    }),
    method(
        "set",
        "(Ljava/lang/Object;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            var_handle_set(
                vm,
                receiver(args),
                ref_arg(args, 1),
                Value::Int(crate::int_arg(args, 2)),
            )?;
            void()
        },
    ),
    method(
        "set",
        "(Ljava/lang/Object;J)V",
        ACC_PUBLIC,
        |vm, _, args| {
            var_handle_set(
                vm,
                receiver(args),
                ref_arg(args, 1),
                Value::Long(crate::long_arg(args, 2)),
            )?;
            void()
        },
    ),
    method(
        "set",
        "(Ljava/lang/Object;Z)V",
        ACC_PUBLIC,
        |vm, _, args| {
            var_handle_set(
                vm,
                receiver(args),
                ref_arg(args, 1),
                Value::Int(crate::int_arg(args, 2)),
            )?;
            void()
        },
    ),
    method(
        "set",
        "(Ljava/lang/Object;B)V",
        ACC_PUBLIC,
        |vm, _, args| {
            var_handle_set(
                vm,
                receiver(args),
                ref_arg(args, 1),
                Value::Int(crate::int_arg(args, 2)),
            )?;
            void()
        },
    ),
    method(
        "set",
        "(Ljava/lang/Object;C)V",
        ACC_PUBLIC,
        |vm, _, args| {
            var_handle_set(
                vm,
                receiver(args),
                ref_arg(args, 1),
                Value::Int(crate::int_arg(args, 2)),
            )?;
            void()
        },
    ),
    method(
        "set",
        "(Ljava/lang/Object;S)V",
        ACC_PUBLIC,
        |vm, _, args| {
            var_handle_set(
                vm,
                receiver(args),
                ref_arg(args, 1),
                Value::Int(crate::int_arg(args, 2)),
            )?;
            void()
        },
    ),
    method(
        "set",
        "(Ljava/lang/Object;F)V",
        ACC_PUBLIC,
        |vm, _, args| {
            var_handle_set(
                vm,
                receiver(args),
                ref_arg(args, 1),
                Value::Float(crate::float_arg(args, 2)),
            )?;
            void()
        },
    ),
    method(
        "set",
        "(Ljava/lang/Object;D)V",
        ACC_PUBLIC,
        |vm, _, args| {
            var_handle_set(
                vm,
                receiver(args),
                ref_arg(args, 1),
                Value::Double(crate::double_arg(args, 2)),
            )?;
            void()
        },
    ),
    method(
        "compareAndSet",
        "(Ljava/lang/Object;II)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let old = var_handle_get(vm, receiver(args), ref_arg(args, 1))?;
            let expected = Value::Int(crate::int_arg(args, 2));
            if old == expected {
                var_handle_set(
                    vm,
                    receiver(args),
                    ref_arg(args, 1),
                    Value::Int(crate::int_arg(args, 3)),
                )?;
                boolean(true)
            } else {
                boolean(false)
            }
        },
    ),
    method(
        "isAccessModeSupported",
        "(Ljava/lang/invoke/VarHandle$AccessMode;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(true),
    ),
    method("hasInvokeExactBehavior", "()Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
    method(
        "varType",
        "()Ljava/lang/Class;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(
                vm.read_ref_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "fieldType",
                    "Ljava/lang/Class;",
                )
                .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method(
        "coordinateTypes",
        "()Ljava/util/List;",
        ACC_PUBLIC,
        |vm, _, _args| {
            let object_class = vm.class_object(oxjvm_vm::ClassId::OBJECT)?;
            object(crate::util::array_list_from(vm, &[object_class])?)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("VarHandle")?),
    ),
];

pub(crate) const VAR_HANDLE: oxjvm_vm::NativeClass = class(
    "java/lang/invoke/VarHandle",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[
        field("targetClass", "Ljava/lang/Class;", ACC_PRIVATE, None),
        field("fieldType", "Ljava/lang/Class;", ACC_PRIVATE, None),
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("descriptor", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("kind", "I", ACC_PRIVATE, None),
    ],
    &VAR_HANDLE_METHODS,
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
const FUNCTIONAL_METHODS: [oxjvm_vm::NativeMethodDef; 43] = [
    method(
        "apply",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "apply",
        "(I)Ljava/lang/Object;",
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
        "accept",
        "(I)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "test",
        "(I)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "applyAsInt",
        "(I)I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "accept",
        "(D)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "test",
        "(D)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "applyAsDouble",
        "(D)D",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "apply",
        "(D)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "applyAsDouble",
        "(Ljava/lang/Object;)D",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "accept",
        "(J)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "test",
        "(J)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "applyAsLong",
        "(J)J",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "apply",
        "(J)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "applyAsLong",
        "(Ljava/lang/Object;)J",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "applyAsDouble",
        "(DD)D",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "applyAsLong",
        "(JJ)J",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "accept",
        "(Ljava/lang/Object;D)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "accept",
        "(Ljava/lang/Object;J)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "applyAsInt",
        "(J)I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "applyAsDouble",
        "(J)D",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "applyAsLong",
        "(I)J",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "applyAsDouble",
        "(I)D",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getAsBoolean",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getAsInt",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getAsLong",
        "()J",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getAsDouble",
        "()D",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "newThread",
        "(Ljava/lang/Runnable;)Ljava/lang/Thread;",
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
    method(
        "identity",
        "()Ljava/util/function/Function;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/function/OxjvmIdentity")?;
            let instance = vm.new_instance(class)?;
            object(instance)
        },
    ),
    method(
        "and",
        "(Ljava/util/function/Predicate;)Ljava/util/function/Predicate;",
        ACC_PUBLIC,
        |vm, _, args| predicate_combine(vm, 0, receiver(args), ref_arg(args, 1)),
    ),
    method(
        "or",
        "(Ljava/util/function/Predicate;)Ljava/util/function/Predicate;",
        ACC_PUBLIC,
        |vm, _, args| predicate_combine(vm, 1, receiver(args), ref_arg(args, 1)),
    ),
    method(
        "negate",
        "()Ljava/util/function/Predicate;",
        ACC_PUBLIC,
        |vm, _, args| predicate_combine(vm, 2, receiver(args), ObjectRef::NULL),
    ),
    method(
        "isEqual",
        "(Ljava/lang/Object;)Ljava/util/function/Predicate;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| predicate_combine(vm, 3, ref_arg(args, 0), ObjectRef::NULL),
    ),
    method(
        "andThen",
        "(Ljava/util/function/Function;)Ljava/util/function/Function;",
        ACC_PUBLIC,
        |vm, _, args| function_and_then(vm, receiver(args), ref_arg(args, 1), false),
    ),
    method(
        "compose",
        "(Ljava/util/function/Function;)Ljava/util/function/Function;",
        ACC_PUBLIC,
        |vm, _, args| function_and_then(vm, receiver(args), ref_arg(args, 1), true),
    ),
];

fn predicate_combine(
    vm: &mut Vm<'_>,
    op: i32,
    left: ObjectRef,
    right: ObjectRef,
) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/util/function/OxjvmPredicate")?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "op", "I")?;
    vm.set_instance_int(instance, declaring, field, op);
    let (declaring, field) = vm.find_field(class, "left", "Ljava/util/function/Predicate;")?;
    vm.set_instance_ref(instance, declaring, field, left);
    let (declaring, field) = vm.find_field(class, "right", "Ljava/util/function/Predicate;")?;
    vm.set_instance_ref(instance, declaring, field, right);
    object(instance)
}

fn predicate_test(
    vm: &mut Vm<'_>,
    predicate: ObjectRef,
    value: ObjectRef,
) -> Result<bool, VmError> {
    let (class, method) = vm.resolve_virtual_method(predicate, "test", "(Ljava/lang/Object;)Z")?;
    Ok(vm
        .invoke_method(
            class,
            method,
            alloc::vec![Value::Ref(predicate), Value::Ref(value)],
        )?
        .as_int()
        != 0)
}

const PREDICATE_COMBINER_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "test",
    "(Ljava/lang/Object;)Z",
    ACC_PUBLIC,
    |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        let op = crate::lang::read_int_field(vm, this, "op");
        let left = vm
            .read_ref_field(this, class, "left", "Ljava/util/function/Predicate;")
            .unwrap_or(ObjectRef::NULL);
        let right = vm
            .read_ref_field(this, class, "right", "Ljava/util/function/Predicate;")
            .unwrap_or(ObjectRef::NULL);
        let value = ref_arg(args, 1);
        let result = match op {
            0 => predicate_test(vm, left, value)? && predicate_test(vm, right, value)?,
            1 => predicate_test(vm, left, value)? || predicate_test(vm, right, value)?,
            3 => {
                if left.is_null() {
                    value.is_null()
                } else {
                    let (cls, m) =
                        vm.resolve_virtual_method(left, "equals", "(Ljava/lang/Object;)Z")?;
                    vm.invoke_method(cls, m, alloc::vec![Value::Ref(left), Value::Ref(value)])?
                        .as_int()
                        != 0
                }
            }
            _ => !predicate_test(vm, left, value)?,
        };
        boolean(result)
    },
)];

/// A composite `Predicate` from `and`, `or`, and `negate`.
pub(crate) const PREDICATE_COMBINER: oxjvm_vm::NativeClass = class(
    "java/util/function/OxjvmPredicate",
    Some("java/lang/Object"),
    &["java/util/function/Predicate"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[
        field("op", "I", ACC_PRIVATE, None),
        field("left", "Ljava/util/function/Predicate;", ACC_PRIVATE, None),
        field("right", "Ljava/util/function/Predicate;", ACC_PRIVATE, None),
    ],
    &PREDICATE_COMBINER_METHODS,
    None,
);

const IDENTITY_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "apply",
    "(Ljava/lang/Object;)Ljava/lang/Object;",
    ACC_PUBLIC,
    |_, _, args| object(ref_arg(args, 1)),
)];

/// The receiver `Function.identity()` returns: stateless, returning its argument unchanged.
pub(crate) const OJVM_IDENTITY: oxjvm_vm::NativeClass = class(
    "java/util/function/OxjvmIdentity",
    Some("java/lang/Object"),
    &["java/util/function/Function"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[],
    &IDENTITY_METHODS,
    None,
);

fn function_and_then(
    vm: &mut Vm<'_>,
    receiver: ObjectRef,
    argument: ObjectRef,
    compose: bool,
) -> Result<Value, VmError> {
    if argument.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let class = vm.resolve_class("java/util/function/OxjvmFunction")?;
    let instance = vm.new_instance(class)?;
    let (first, second) = if compose {
        (argument, receiver)
    } else {
        (receiver, argument)
    };
    for (name, value) in [("first", first), ("second", second)] {
        let (declaring, field) = vm.find_field(class, name, "Ljava/util/function/Function;")?;
        vm.set_instance_ref(instance, declaring, field, value);
    }
    object(instance)
}

const OJVM_FUNCTION_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "apply",
    "(Ljava/lang/Object;)Ljava/lang/Object;",
    ACC_PUBLIC,
    |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        let first = vm
            .read_ref_field(this, class, "first", "Ljava/util/function/Function;")
            .unwrap_or(ObjectRef::NULL);
        let second = vm
            .read_ref_field(this, class, "second", "Ljava/util/function/Function;")
            .unwrap_or(ObjectRef::NULL);
        let (first_class, first_method) =
            vm.resolve_virtual_method(first, "apply", "(Ljava/lang/Object;)Ljava/lang/Object;")?;
        let intermediate = vm.invoke_method(
            first_class,
            first_method,
            alloc::vec![Value::Ref(first), Value::Ref(ref_arg(args, 1))],
        )?;
        let (second_class, second_method) =
            vm.resolve_virtual_method(second, "apply", "(Ljava/lang/Object;)Ljava/lang/Object;")?;
        vm.invoke_method(
            second_class,
            second_method,
            alloc::vec![Value::Ref(second), intermediate],
        )
    },
)];

/// A composed `Function` returned by `Function.andThen`/`compose`.
pub(crate) const OJVM_FUNCTION: oxjvm_vm::NativeClass = class(
    "java/util/function/OxjvmFunction",
    Some("java/lang/Object"),
    &["java/util/function/Function"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[
        field("first", "Ljava/util/function/Function;", ACC_PRIVATE, None),
        field("second", "Ljava/util/function/Function;", ACC_PRIVATE, None),
    ],
    &OJVM_FUNCTION_METHODS,
    None,
);

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
functional_interface!(INT_CONSUMER, "java/util/function/IntConsumer");
functional_interface!(INT_PREDICATE, "java/util/function/IntPredicate");
functional_interface!(INT_UNARY_OPERATOR, "java/util/function/IntUnaryOperator");
functional_interface!(CALLABLE, "java/util/concurrent/Callable");
functional_interface!(INT_FUNCTION, "java/util/function/IntFunction");
functional_interface!(TO_INT_FUNCTION, "java/util/function/ToIntFunction");
functional_interface!(TO_LONG_FUNCTION, "java/util/function/ToLongFunction");
functional_interface!(TO_DOUBLE_FUNCTION, "java/util/function/ToDoubleFunction");
functional_interface!(THREAD_FACTORY, "java/util/concurrent/ThreadFactory");
functional_interface!(DOUBLE_CONSUMER, "java/util/function/DoubleConsumer");
functional_interface!(DOUBLE_PREDICATE, "java/util/function/DoublePredicate");
functional_interface!(
    DOUBLE_UNARY_OPERATOR,
    "java/util/function/DoubleUnaryOperator"
);
functional_interface!(DOUBLE_FUNCTION, "java/util/function/DoubleFunction");
functional_interface!(
    DOUBLE_BINARY_OPERATOR,
    "java/util/function/DoubleBinaryOperator"
);
functional_interface!(
    DOUBLE_TO_INT_FUNCTION,
    "java/util/function/DoubleToIntFunction"
);
functional_interface!(
    DOUBLE_TO_LONG_FUNCTION,
    "java/util/function/DoubleToLongFunction"
);
functional_interface!(LONG_CONSUMER, "java/util/function/LongConsumer");
functional_interface!(LONG_PREDICATE, "java/util/function/LongPredicate");
functional_interface!(LONG_UNARY_OPERATOR, "java/util/function/LongUnaryOperator");
functional_interface!(LONG_FUNCTION, "java/util/function/LongFunction");
functional_interface!(
    LONG_BINARY_OPERATOR,
    "java/util/function/LongBinaryOperator"
);
functional_interface!(LONG_TO_INT_FUNCTION, "java/util/function/LongToIntFunction");
functional_interface!(
    LONG_TO_DOUBLE_FUNCTION,
    "java/util/function/LongToDoubleFunction"
);
functional_interface!(INT_TO_LONG_FUNCTION, "java/util/function/IntToLongFunction");
functional_interface!(
    INT_TO_DOUBLE_FUNCTION,
    "java/util/function/IntToDoubleFunction"
);
functional_interface!(BOOLEAN_SUPPLIER, "java/util/function/BooleanSupplier");
functional_interface!(INT_SUPPLIER, "java/util/function/IntSupplier");
functional_interface!(LONG_SUPPLIER, "java/util/function/LongSupplier");
functional_interface!(DOUBLE_SUPPLIER, "java/util/function/DoubleSupplier");

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

// -------------------------------------------------------------------------------------------
// VarHandle.AccessMode
// -------------------------------------------------------------------------------------------

const ACCESS_MODE_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let name = vm
            .read_string_field(receiver(args), vm.class_of(receiver(args)), "name")
            .unwrap_or_default();
        object(vm.make_string(&name)?)
    }),
    method(
        "methodName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = vm
                .read_string_field(receiver(args), vm.class_of(receiver(args)), "name")
                .unwrap_or_default();
            // JDK lower-cases the first character: GET -> get, GET_VOLATILE -> getVolatile, etc.
            let mut chars = name.chars();
            let mut result = String::new();
            if let Some(first) = chars.next() {
                result.push(first.to_ascii_lowercase());
                result.push_str(chars.as_str());
            }
            object(vm.make_string(&result)?)
        },
    ),
    method("ordinal", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "ordinal"))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = vm
                .read_string_field(receiver(args), vm.class_of(receiver(args)), "name")
                .unwrap_or_default();
            object(vm.make_string(&name)?)
        },
    ),
    method(
        "values",
        "()[Ljava/lang/invoke/VarHandle$AccessMode;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/lang/invoke/VarHandle$AccessMode")?;
            let names = [
                "GET",
                "SET",
                "GET_VOLATILE",
                "SET_VOLATILE",
                "GET_ACQUIRE",
                "SET_RELEASE",
                "COMPARE_AND_SET",
                "COMPARE_AND_EXCHANGE",
                "GET_AND_SET",
            ];
            let array = vm.allocate_object_array(class, names.len())?;
            for (index, name) in names.iter().enumerate() {
                if let Some(value) = vm.read_static_ref_if_set(
                    class,
                    name,
                    "Ljava/lang/invoke/VarHandle$AccessMode;",
                ) {
                    vm.array_set_ref(array, index, value)?;
                }
            }
            object(array)
        },
    ),
    method(
        "valueOf",
        "(Ljava/lang/String;)Ljava/lang/invoke/VarHandle$AccessMode;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = vm.string_or_empty(ref_arg(args, 0));
            let class = vm.resolve_class("java/lang/invoke/VarHandle$AccessMode")?;
            vm.ensure_initialized(class)?;
            if let Some(value) =
                vm.read_static_ref_if_set(class, &name, "Ljava/lang/invoke/VarHandle$AccessMode;")
            {
                return object(value);
            }
            Err(vm.throw_new("java/lang/IllegalArgumentException", Some(&name)))
        },
    ),
];

fn access_mode_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/lang/invoke/VarHandle$AccessMode")?;
    for (ordinal, name) in [
        "GET",
        "SET",
        "GET_VOLATILE",
        "SET_VOLATILE",
        "GET_ACQUIRE",
        "SET_RELEASE",
        "COMPARE_AND_SET",
        "COMPARE_AND_EXCHANGE",
        "GET_AND_SET",
    ]
    .iter()
    .enumerate()
    {
        let instance = vm.new_instance(class)?;
        vm.set_string_field(instance, class, "name", name)?;
        crate::lang::write_int_field(vm, instance, "ordinal", ordinal as i32);
        vm.set_static_value(
            class,
            name,
            "Ljava/lang/invoke/VarHandle$AccessMode;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

pub(crate) const ACCESS_MODE: oxjvm_vm::NativeClass = class(
    "java/lang/invoke/VarHandle$AccessMode",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("ordinal", "I", ACC_PRIVATE, None),
        field(
            "GET",
            "Ljava/lang/invoke/VarHandle$AccessMode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "SET",
            "Ljava/lang/invoke/VarHandle$AccessMode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "GET_VOLATILE",
            "Ljava/lang/invoke/VarHandle$AccessMode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "SET_VOLATILE",
            "Ljava/lang/invoke/VarHandle$AccessMode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "GET_ACQUIRE",
            "Ljava/lang/invoke/VarHandle$AccessMode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "SET_RELEASE",
            "Ljava/lang/invoke/VarHandle$AccessMode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "COMPARE_AND_SET",
            "Ljava/lang/invoke/VarHandle$AccessMode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "COMPARE_AND_EXCHANGE",
            "Ljava/lang/invoke/VarHandle$AccessMode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "GET_AND_SET",
            "Ljava/lang/invoke/VarHandle$AccessMode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &ACCESS_MODE_METHODS,
    Some(access_mode_clinit),
);
