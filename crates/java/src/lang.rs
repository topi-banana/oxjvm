//! `java.lang`: `Object`, `Class`, `String`, builders, `System`, `Thread`, and the core interfaces.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ClassId, ObjectRef, Value, Vm, VmError};

use crate::{
    bool_arg, boolean, class, double_arg, field, float_arg, int, int_arg, long, long_arg, method,
    object, receiver, ref_arg, string_arg, void,
};

// -------------------------------------------------------------------------------------------
// Object
// -------------------------------------------------------------------------------------------

const OBJECT_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "getClass",
        "()Ljava/lang/Class;",
        ACC_PUBLIC | ACC_NATIVE,
        object_get_class,
    ),
    method("hashCode", "()I", ACC_PUBLIC | ACC_NATIVE, object_hash_code),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| boolean(same(vm, receiver(args), ref_arg(args, 1))),
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        object_to_string,
    ),
    method(
        "clone",
        "()Ljava/lang/Object;",
        ACC_PROTECTED | ACC_NATIVE,
        object_clone,
    ),
    method("notify", "()V", ACC_PUBLIC | ACC_FINAL, object_notify),
    method(
        "notifyAll",
        "()V",
        ACC_PUBLIC | ACC_FINAL,
        object_notify_all,
    ),
    method("wait", "()V", ACC_PUBLIC | ACC_FINAL, |vm, _, args| {
        object_wait(vm, args, None)
    }),
    method("wait", "(J)V", ACC_PUBLIC | ACC_FINAL, object_wait_timeout),
    method(
        "wait",
        "(JI)V",
        ACC_PUBLIC | ACC_FINAL,
        object_wait_timeout_nanos,
    ),
    method("finalize", "()V", ACC_PROTECTED, |_, _, _| void()),
];

/// Identity comparison of two references.
fn same(_vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> bool {
    a == b
}

fn object_get_class(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = vm.class_of(receiver(args));
    let class_object = vm.class_object(class)?;
    object(class_object)
}

fn object_hash_code(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let hash = vm.heap.get(receiver(args)).map_or(0, |object| object.hash);
    int(hash)
}

fn object_to_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let reference = receiver(args);
    let text = describe_object(vm, reference);
    let string = vm.make_string(&text)?;
    object(string)
}

/// The default `Object.toString` form: `java.lang.Class@1a2b3c`.
pub(crate) fn describe_object(vm: &mut Vm<'_>, reference: ObjectRef) -> String {
    let name = crate::binary_name(vm, vm.class_of(reference));
    let hash = vm.heap.get(reference).map_or(0, |object| object.hash);
    alloc::format!("{name}@{:x}", hash as u32)
}

fn object_clone(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let reference = receiver(args);
    let cloneable = vm.resolve_class("java/lang/Cloneable")?;
    if !vm.is_instance(reference, cloneable) {
        return Err(vm.throw_new("java/lang/CloneNotSupportedException", None));
    }
    let clone = vm.clone_object(reference)?;
    object(clone)
}

fn check_monitor_owner(vm: &mut Vm<'_>, reference: ObjectRef) -> Result<(), VmError> {
    if reference.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let owner = vm.current_thread_object();
    if !vm.holds_monitor(reference, owner) {
        return Err(vm.throw_new(
            "java/lang/IllegalMonitorStateException",
            Some("current thread is not owner"),
        ));
    }
    Ok(())
}

fn object_notify(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let reference = receiver(args);
    check_monitor_owner(vm, reference)?;
    vm.monitor_notify(reference, false);
    void()
}

fn object_notify_all(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let reference = receiver(args);
    check_monitor_owner(vm, reference)?;
    vm.monitor_notify(reference, true);
    void()
}

fn object_wait(vm: &mut Vm<'_>, args: &[Value], timeout: Option<i64>) -> Result<Value, VmError> {
    let reference = receiver(args);
    check_monitor_owner(vm, reference)?;
    if vm.monitor_wait(reference, timeout) {
        void()
    } else {
        Err(vm.throw_new(
            "java/lang/IllegalMonitorStateException",
            Some("current thread is not owner"),
        ))
    }
}

fn object_wait_timeout(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let millis = long_arg(args, 1);
    if millis < 0 {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("timeout value is negative"),
        ));
    }
    object_wait(vm, args, if millis == 0 { None } else { Some(millis) })
}

fn object_wait_timeout_nanos(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let millis = long_arg(args, 1);
    let nanos = int_arg(args, 2);
    if millis < 0 {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("timeout value is negative"),
        ));
    }
    if !(0..=999_999).contains(&nanos) {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("nanosecond timeout value out of range"),
        ));
    }
    let total = if millis == 0 && nanos == 0 {
        None
    } else {
        Some(millis + i64::from(nanos > 0))
    };
    object_wait(vm, args, total)
}

pub(crate) const OBJECT: oxjvm_vm::NativeClass = class(
    "java/lang/Object",
    None,
    &[],
    ACC_PUBLIC,
    &[],
    &OBJECT_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Class
// -------------------------------------------------------------------------------------------

const CLASS_METHODS: [oxjvm_vm::NativeMethodDef; 22] = [
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        class_get_name,
    ),
    method(
        "getSimpleName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        class_get_simple_name,
    ),
    method(
        "getSuperclass",
        "()Ljava/lang/Class;",
        ACC_PUBLIC,
        class_get_superclass,
    ),
    method(
        "getInterfaces",
        "()[Ljava/lang/Class;",
        ACC_PUBLIC,
        class_get_interfaces,
    ),
    method(
        "getComponentType",
        "()Ljava/lang/Class;",
        ACC_PUBLIC,
        class_get_component_type,
    ),
    method("isInterface", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(vm.classes.get(vm.class_of(receiver(args))).is_interface())
    }),
    method("isArray", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(vm.classes.get(vm.class_of(receiver(args))).kind == oxjvm_vm::ClassKind::Array)
    }),
    method("isPrimitive", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(vm.classes.get(vm.class_of(receiver(args))).kind == oxjvm_vm::ClassKind::Primitive)
    }),
    method("isEnum", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(vm.classes.get(vm.class_of(receiver(args))).is_enum)
    }),
    method("isSynthetic", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(vm.classes.get(vm.class_of(receiver(args))).access_flags & ACC_SYNTHETIC != 0)
    }),
    method("isAnnotation", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(vm.classes.get(vm.class_of(receiver(args))).access_flags & ACC_ANNOTATION != 0)
    }),
    method(
        "isAssignableFrom",
        "(Ljava/lang/Class;)Z",
        ACC_PUBLIC,
        class_is_assignable_from,
    ),
    method(
        "isInstance",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = class_argument(vm, receiver(args))?;
            let reference = ref_arg(args, 1);
            boolean(!reference.is_null() && vm.is_instance(reference, this))
        },
    ),
    method(
        "cast",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        class_cast,
    ),
    method("getModifiers", "()I", ACC_PUBLIC, |vm, _, args| {
        let class = class_argument(vm, receiver(args))?;
        int(i32::from(vm.classes.get(class).access_flags))
    }),
    method("desiredAssertionStatus", "()Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
    method(
        "forName",
        "(Ljava/lang/String;)Ljava/lang/Class;",
        ACC_PUBLIC | ACC_STATIC,
        class_for_name,
    ),
    method(
        "newInstance",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        class_new_instance,
    ),
    method(
        "getClassLoader",
        "()Ljava/lang/ClassLoader;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = class_argument(vm, receiver(args))?;
            let kind = if vm.classes.get(class).is_interface() {
                "interface"
            } else {
                "class"
            };
            let text = alloc::format!("{kind} {}", crate::binary_name(vm, class));
            object(vm.make_string(&text)?)
        },
    ),
    method("isRecord", "()Z", ACC_PUBLIC, class_is_record),
    method(
        "getPackageName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        class_get_package_name,
    ),
];

/// The class denoted by a `java.lang.Class` receiver.
pub(crate) fn class_argument(vm: &mut Vm<'_>, reference: ObjectRef) -> Result<ClassId, VmError> {
    match vm.heap.get(reference).map(|object| &object.data) {
        Some(oxjvm_vm::ObjectData::Class(class)) => Ok(*class),
        _ => Err(vm.throw_new("java/lang/NullPointerException", None)),
    }
}

fn class_get_name(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    let name = vm.class_name(class).to_string();
    let text = if vm.classes.get(class).kind == oxjvm_vm::ClassKind::Primitive {
        name
    } else {
        name.replace('/', ".")
    };
    object(vm.make_string(&text)?)
}

fn class_get_simple_name(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    let name = vm.class_name(class);
    let simple = if let Some(element) = name.strip_prefix('[') {
        if let Some(object) = element.strip_prefix('L') {
            let object = object.strip_suffix(';').unwrap_or(object);
            alloc::format!("{object}[]")
        } else {
            alloc::format!("{element}[]")
        }
    } else {
        let base = name.rsplit('/').next().unwrap_or(name);
        base.rsplit('$').next().unwrap_or(base).to_string()
    };
    object(vm.make_string(&simple)?)
}

fn class_get_superclass(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    let definition = vm.classes.get(class);
    if definition.kind == oxjvm_vm::ClassKind::Primitive || definition.is_interface() {
        return object(ObjectRef::NULL);
    }
    match definition.super_class {
        Some(super_class) => {
            let class_object = vm.class_object(super_class)?;
            object(class_object)
        }
        None => object(ObjectRef::NULL),
    }
}

fn class_get_interfaces(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    let interfaces = vm.classes.get(class).interfaces.clone();
    let class_class = vm.resolve_class("java/lang/Class")?;
    let array = vm.allocate_object_array(class_class, interfaces.len())?;
    for (index, interface) in interfaces.into_iter().enumerate() {
        let class_object = vm.class_object(interface)?;
        vm.array_set_ref(array, index, class_object)?;
    }
    object(array)
}

fn class_get_component_type(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    let component = vm.classes.get(class).component.clone();
    match component {
        Some(oxjvm_vm::ArrayComponent::Reference) => {
            let component_class = vm.classes.get(class).component_class;
            match component_class {
                Some(component_class) => {
                    let class_object = vm.class_object(component_class)?;
                    object(class_object)
                }
                None => object(ObjectRef::NULL),
            }
        }
        Some(primitive) => {
            let base = match primitive {
                oxjvm_vm::ArrayComponent::Boolean => oxjvm_classfile::BaseType::Boolean,
                oxjvm_vm::ArrayComponent::Byte => oxjvm_classfile::BaseType::Byte,
                oxjvm_vm::ArrayComponent::Char => oxjvm_classfile::BaseType::Char,
                oxjvm_vm::ArrayComponent::Short => oxjvm_classfile::BaseType::Short,
                oxjvm_vm::ArrayComponent::Int => oxjvm_classfile::BaseType::Int,
                oxjvm_vm::ArrayComponent::Long => oxjvm_classfile::BaseType::Long,
                oxjvm_vm::ArrayComponent::Float => oxjvm_classfile::BaseType::Float,
                oxjvm_vm::ArrayComponent::Double => oxjvm_classfile::BaseType::Double,
                oxjvm_vm::ArrayComponent::Reference => oxjvm_classfile::BaseType::Void,
            };
            let class = vm.primitive_class(base)?;
            let class_object = vm.class_object(class)?;
            object(class_object)
        }
        None => object(ObjectRef::NULL),
    }
}

fn class_is_assignable_from(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let target = class_argument(vm, receiver(args))?;
    let other = class_argument(vm, ref_arg(args, 1))?;
    boolean(vm.class_is_subtype(other, target))
}

fn class_cast(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let target = class_argument(vm, receiver(args))?;
    let value = ref_arg(args, 1);
    if value.is_null() || vm.is_instance(value, target) {
        object(value)
    } else {
        let from = crate::binary_name(vm, vm.class_of(value));
        let to = crate::binary_name(vm, target);
        Err(vm.throw_new(
            "java/lang/ClassCastException",
            Some(&alloc::format!("Cannot cast {from} to {to}")),
        ))
    }
}

fn class_for_name(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let name = string_arg(vm, args[0])?;
    let internal = name.replace('.', "/");
    match vm.resolve_class(&internal) {
        Ok(class) => {
            vm.ensure_initialized(class)?;
            let class_object = vm.class_object(class)?;
            object(class_object)
        }
        Err(VmError::Thrown(exception)) => {
            let class_not_found = vm.resolve_class("java/lang/ClassNotFoundException")?;
            if vm.is_instance(exception, class_not_found) {
                Err(VmError::Thrown(exception))
            } else {
                Err(vm.throw_new("java/lang/ClassNotFoundException", Some(&name)))
            }
        }
        Err(other) => Err(other),
    }
}

fn class_new_instance(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    let (constructor_class, constructor) = vm
        .find_method(class, "<init>", "()V")
        .ok_or_else(|| vm.throw_new("java/lang/InstantiationException", None))?;
    let instance = vm.new_instance(class)?;
    vm.invoke_method(constructor_class, constructor, vec![Value::Ref(instance)])?;
    object(instance)
}

fn class_is_record(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    let record = match vm.resolve_class_lenient_pub("java/lang/Record") {
        Some(record) => record,
        None => return boolean(false),
    };
    boolean(vm.class_is_subtype(class, record))
}

fn class_get_package_name(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    let name = vm.class_name(class);
    let package = match name.rfind('/') {
        Some(index) => &name[..index],
        None => "",
    };
    let text = package.replace('/', ".");
    object(vm.make_string(&text)?)
}

pub(crate) const CLASS: oxjvm_vm::NativeClass = class(
    "java/lang/Class",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &CLASS_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// String
// -------------------------------------------------------------------------------------------

const STRING_METHODS: [oxjvm_vm::NativeMethodDef; 46] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        string_init_string,
    ),
    method("<init>", "([C)V", ACC_PUBLIC, string_init_chars),
    method("<init>", "([CII)V", ACC_PUBLIC, string_init_chars_range),
    method("<init>", "([B)V", ACC_PUBLIC, string_init_bytes),
    method(
        "<init>",
        "([BLjava/lang/String;)V",
        ACC_PUBLIC,
        string_init_bytes_charset,
    ),
    method("length", "()I", ACC_PUBLIC, string_length),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(
            vm.string_utf16(receiver(args))
                .is_some_and(<[u16]>::is_empty),
        )
    }),
    method("charAt", "()C", ACC_PUBLIC, string_char_at),
    method("charAt", "(I)C", ACC_PUBLIC, string_char_at_index),
    method("codePointAt", "(I)I", ACC_PUBLIC, string_code_point_at),
    method("hashCode", "()I", ACC_PUBLIC, string_hash_code),
    method("equals", "(Ljava/lang/Object;)Z", ACC_PUBLIC, string_equals),
    method(
        "equalsIgnoreCase",
        "(Ljava/lang/String;)Z",
        ACC_PUBLIC,
        string_equals_ignore_case,
    ),
    method(
        "compareTo",
        "(Ljava/lang/String;)I",
        ACC_PUBLIC,
        string_compare_to,
    ),
    method(
        "compareToIgnoreCase",
        "(Ljava/lang/String;)I",
        ACC_PUBLIC,
        string_compare_to_ignore_case,
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method(
        "intern",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_NATIVE,
        string_intern,
    ),
    method(
        "concat",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC,
        string_concat,
    ),
    method(
        "substring",
        "(I)Ljava/lang/String;",
        ACC_PUBLIC,
        string_substring,
    ),
    method(
        "substring",
        "(II)Ljava/lang/String;",
        ACC_PUBLIC,
        string_substring_range,
    ),
    method("indexOf", "(I)I", ACC_PUBLIC, string_index_of_char),
    method("indexOf", "(II)I", ACC_PUBLIC, string_index_of_char_from),
    method(
        "indexOf",
        "(Ljava/lang/String;)I",
        ACC_PUBLIC,
        string_index_of_string,
    ),
    method(
        "indexOf",
        "(Ljava/lang/String;I)I",
        ACC_PUBLIC,
        string_index_of_string_from,
    ),
    method("lastIndexOf", "(I)I", ACC_PUBLIC, string_last_index_of_char),
    method(
        "startsWith",
        "(Ljava/lang/String;)Z",
        ACC_PUBLIC,
        |vm, _c, args| string_starts_with_at(vm, args, 0),
    ),
    method(
        "startsWith",
        "(Ljava/lang/String;I)Z",
        ACC_PUBLIC,
        string_starts_with,
    ),
    method(
        "endsWith",
        "(Ljava/lang/String;)Z",
        ACC_PUBLIC,
        string_ends_with,
    ),
    method(
        "contains",
        "(Ljava/lang/CharSequence;)Z",
        ACC_PUBLIC,
        string_contains,
    ),
    method("toCharArray", "()[C", ACC_PUBLIC, string_to_char_array),
    method("getChars", "(II[CI)V", ACC_PUBLIC, string_get_chars),
    method("trim", "()Ljava/lang/String;", ACC_PUBLIC, string_trim),
    method("strip", "()Ljava/lang/String;", ACC_PUBLIC, string_strip),
    method(
        "stripLeading",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        string_strip_leading,
    ),
    method(
        "stripTrailing",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        string_strip_trailing,
    ),
    method("isBlank", "()Z", ACC_PUBLIC, string_is_blank),
    method("repeat", "(I)Ljava/lang/String;", ACC_PUBLIC, string_repeat),
    method(
        "replace",
        "(CC)Ljava/lang/String;",
        ACC_PUBLIC,
        string_replace_char,
    ),
    method(
        "replace",
        "(Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Ljava/lang/String;",
        ACC_PUBLIC,
        string_replace_sequence,
    ),
    method(
        "toUpperCase",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        string_to_upper,
    ),
    method(
        "toLowerCase",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        string_to_lower,
    ),
    method("getBytes", "()[B", ACC_PUBLIC, string_get_bytes),
    method(
        "valueOf",
        "(Ljava/lang/Object;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        string_value_of_object,
    ),
    method(
        "valueOf",
        "([C)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        string_value_of_chars,
    ),
    method(
        "valueOf",
        "([CII)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        string_value_of_chars_range,
    ),
];

// The remaining `String.valueOf` overloads and `format` are declared in a second array so the
// first stays readable; both are concatenated below.
const STRING_METHODS_EXTRA: [oxjvm_vm::NativeMethodDef; 11] = [
    method(
        "valueOf",
        "(Z)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        string_value_of_bool,
    ),
    method(
        "valueOf",
        "(C)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        string_value_of_char,
    ),
    method(
        "valueOf",
        "(I)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        string_value_of_int,
    ),
    method(
        "valueOf",
        "(J)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        string_value_of_long,
    ),
    method(
        "valueOf",
        "(F)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        string_value_of_float,
    ),
    method(
        "valueOf",
        "(D)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        string_value_of_double,
    ),
    method(
        "format",
        "(Ljava/lang/String;[Ljava/lang/Object;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        string_format,
    ),
    method(
        "join",
        "(Ljava/lang/CharSequence;[Ljava/lang/CharSequence;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        string_join,
    ),
    method(
        "split",
        "(Ljava/lang/String;)[Ljava/lang/String;",
        ACC_PUBLIC,
        string_split,
    ),
    method(
        "split",
        "(Ljava/lang/String;I)[Ljava/lang/String;",
        ACC_PUBLIC,
        string_split_limit,
    ),
    method(
        "matches",
        "(Ljava/lang/String;)Z",
        ACC_PUBLIC,
        string_matches,
    ),
];

fn string_init_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let other = ref_arg(args, 1);
    if other.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let units = vm
        .string_utf16(other)
        .map(<[u16]>::to_vec)
        .unwrap_or_default();
    write_string_payload(vm, this, units)?;
    void()
}

/// Replace a `String` object's payload in place (the only mutation of an immutable Java string,
/// used by the constructors before publication).
pub(crate) fn write_string_payload(
    vm: &mut Vm<'_>,
    string: ObjectRef,
    units: Vec<u16>,
) -> Result<(), VmError> {
    if let Some(oxjvm_vm::ObjectData::String(payload)) =
        vm.heap.get_mut(string).map(|object| &mut object.data)
    {
        *payload = units;
        Ok(())
    } else {
        Err(VmError::internal("write_string_payload on a non-string"))
    }
}

fn string_init_chars(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let units = char_array(vm, ref_arg(args, 1))?;
    write_string_payload(vm, receiver(args), units)?;
    void()
}

fn string_init_chars_range(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let units = char_array(vm, ref_arg(args, 1))?;
    let offset = int_arg(args, 2);
    let count = int_arg(args, 3);
    string_range_check(vm, offset, count, units.len())?;
    let slice = units[offset as usize..(offset + count) as usize].to_vec();
    write_string_payload(vm, receiver(args), slice)?;
    void()
}

fn string_range_check(
    vm: &mut Vm<'_>,
    offset: i32,
    count: i32,
    length: usize,
) -> Result<(), VmError> {
    if offset < 0 || count < 0 || (offset + count) as usize > length {
        return Err(vm.throw_new("java/lang/StringIndexOutOfBoundsException", None));
    }
    Ok(())
}

fn char_array(vm: &mut Vm<'_>, reference: ObjectRef) -> Result<Vec<u16>, VmError> {
    if reference.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    match vm.heap.get(reference).map(|object| &object.data) {
        Some(oxjvm_vm::ObjectData::Array(oxjvm_vm::ArrayData::Char(units))) => Ok(units.clone()),
        _ => Err(vm.throw_new("java/lang/NullPointerException", None)),
    }
}

fn string_init_bytes(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let bytes = byte_array(vm, ref_arg(args, 1))?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let units: Vec<u16> = text.encode_utf16().collect();
    write_string_payload(vm, receiver(args), units)?;
    void()
}

fn string_init_bytes_charset(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    string_init_bytes(vm, _c, args)
}

fn byte_array(vm: &mut Vm<'_>, reference: ObjectRef) -> Result<Vec<u8>, VmError> {
    if reference.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    match vm.heap.get(reference).map(|object| &object.data) {
        Some(oxjvm_vm::ObjectData::Array(oxjvm_vm::ArrayData::Byte(bytes))) => {
            Ok(bytes.iter().map(|byte| *byte as u8).collect())
        }
        _ => Err(vm.throw_new("java/lang/NullPointerException", None)),
    }
}

fn string_length(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let length = vm.string_utf16(receiver(args)).map_or(0, <[u16]>::len);
    int(length as i32)
}

fn string_char_at(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    string_char_at_index(vm, _c, args)
}

fn string_char_at_index(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let units = vm
        .string_utf16(receiver(args))
        .map(<[u16]>::to_vec)
        .unwrap_or_default();
    let index = int_arg(args, 1);
    if index < 0 || index as usize >= units.len() {
        return Err(vm.throw_new(
            "java/lang/StringIndexOutOfBoundsException",
            Some(&alloc::format!("String index out of range: {index}")),
        ));
    }
    int(i32::from(units[index as usize]))
}

fn string_code_point_at(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let units = vm
        .string_utf16(receiver(args))
        .map(<[u16]>::to_vec)
        .unwrap_or_default();
    let index = int_arg(args, 1);
    if index < 0 || index as usize >= units.len() {
        return Err(vm.throw_new(
            "java/lang/StringIndexOutOfBoundsException",
            Some(&alloc::format!("String index out of range: {index}")),
        ));
    }
    let first = u32::from(units[index as usize]);
    if (0xD800..0xDC00).contains(&first) && (index as usize + 1) < units.len() {
        let second = u32::from(units[index as usize + 1]);
        if (0xDC00..0xE000).contains(&second) {
            return int((0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00)) as i32);
        }
    }
    int(first as i32)
}

/// Java's `String.hashCode`.
pub(crate) fn java_string_hash(units: &[u16]) -> i32 {
    let mut hash = 0i32;
    for unit in units {
        hash = hash.wrapping_mul(31).wrapping_add(i32::from(*unit));
    }
    hash
}

fn string_hash_code(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let hash = vm.string_utf16(receiver(args)).map_or(0, java_string_hash);
    int(hash)
}

fn string_equals(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let other = ref_arg(args, 1);
    if this == other {
        return boolean(true);
    }
    let result = match (vm.string_utf16(this), vm.string_utf16(other)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    };
    boolean(result)
}

fn string_equals_ignore_case(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = vm.string_or_empty(receiver(args));
    let other = string_arg(vm, args[1])?;
    boolean(this.eq_ignore_ascii_case(&other))
}

fn string_compare_to(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = vm
        .string_utf16(receiver(args))
        .map(<[u16]>::to_vec)
        .unwrap_or_default();
    let other = vm
        .string_utf16(ref_arg(args, 1))
        .map(<[u16]>::to_vec)
        .ok_or_else(|| vm.throw_new("java/lang/NullPointerException", None))?;
    int(compare_units(&this, &other))
}

fn compare_units(a: &[u16], b: &[u16]) -> i32 {
    let count = a.len().min(b.len());
    for index in 0..count {
        let difference = i32::from(a[index]) - i32::from(b[index]);
        if difference != 0 {
            return difference;
        }
    }
    (a.len() as i32) - (b.len() as i32)
}

fn string_compare_to_ignore_case(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = vm.string_or_empty(receiver(args)).to_lowercase();
    let other = string_arg(vm, args[1])?.to_lowercase();
    int(compare_units(
        &this.encode_utf16().collect::<Vec<_>>(),
        &other.encode_utf16().collect::<Vec<_>>(),
    ))
}

fn string_intern(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = vm.string_or_empty(receiver(args));
    let interned = vm.intern(&text);
    object(interned)
}

fn string_concat(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let mut text = vm.string_or_empty(receiver(args));
    let other = string_arg(vm, args[1])?;
    text.push_str(&other);
    object(vm.make_string(&text)?)
}

fn string_substring(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let units = vm
        .string_utf16(receiver(args))
        .map(<[u16]>::to_vec)
        .unwrap_or_default();
    let begin = int_arg(args, 1);
    if begin < 0 || begin as usize > units.len() {
        return Err(vm.throw_new(
            "java/lang/StringIndexOutOfBoundsException",
            Some(&alloc::format!(
                "begin {begin}, end {}, length {}",
                units.len(),
                units.len()
            )),
        ));
    }
    let slice = units[begin as usize..].to_vec();
    object(vm.make_string_utf16(slice)?)
}

fn string_substring_range(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let units = vm
        .string_utf16(receiver(args))
        .map(<[u16]>::to_vec)
        .unwrap_or_default();
    let begin = int_arg(args, 1);
    let end = int_arg(args, 2);
    if begin < 0 || end > units.len() as i32 || begin > end {
        return Err(vm.throw_new(
            "java/lang/StringIndexOutOfBoundsException",
            Some(&alloc::format!(
                "begin {begin}, end {end}, length {}",
                units.len()
            )),
        ));
    }
    let slice = units[begin as usize..end as usize].to_vec();
    object(vm.make_string_utf16(slice)?)
}

fn string_index_of_char(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    string_index_of_char_from(vm, _c, args)
}

fn string_index_of_char_from(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let units = vm
        .string_utf16(receiver(args))
        .map(<[u16]>::to_vec)
        .unwrap_or_default();
    let ch = int_arg(args, 1) as u16;
    let from = if args.len() > 2 {
        int_arg(args, 2).max(0) as usize
    } else {
        0
    };
    let found = units
        .iter()
        .enumerate()
        .skip(from)
        .find(|(_, unit)| **unit == ch)
        .map_or(-1, |(index, _)| index as i32);
    int(found)
}

fn find_units(haystack: &[u16], needle: &[u16], from: usize) -> i32 {
    if needle.is_empty() {
        return from.min(haystack.len()) as i32;
    }
    if needle.len() > haystack.len() {
        return -1;
    }
    let last = haystack.len() - needle.len();
    let mut start = from;
    while start <= last {
        if &haystack[start..start + needle.len()] == needle {
            return start as i32;
        }
        start += 1;
    }
    -1
}

fn string_index_of_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let haystack = vm
        .string_utf16(receiver(args))
        .map(<[u16]>::to_vec)
        .unwrap_or_default();
    let needle = vm
        .string_utf16(ref_arg(args, 1))
        .map(<[u16]>::to_vec)
        .ok_or_else(|| vm.throw_new("java/lang/NullPointerException", None))?;
    int(find_units(&haystack, &needle, 0))
}

fn string_index_of_string_from(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let haystack = vm
        .string_utf16(receiver(args))
        .map(<[u16]>::to_vec)
        .unwrap_or_default();
    let needle = vm
        .string_utf16(ref_arg(args, 1))
        .map(<[u16]>::to_vec)
        .ok_or_else(|| vm.throw_new("java/lang/NullPointerException", None))?;
    let from = int_arg(args, 2).max(0) as usize;
    int(find_units(&haystack, &needle, from))
}

fn string_last_index_of_char(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let units = vm
        .string_utf16(receiver(args))
        .map(<[u16]>::to_vec)
        .unwrap_or_default();
    let ch = int_arg(args, 1) as u16;
    let found = units
        .iter()
        .rposition(|unit| *unit == ch)
        .map_or(-1, |index| index as i32);
    int(found)
}

fn string_starts_with(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    string_starts_with_at(vm, args, int_arg(args, 2))
}

fn string_starts_with_at(vm: &mut Vm<'_>, args: &[Value], offset: i32) -> Result<Value, VmError> {
    let units = vm
        .string_utf16(receiver(args))
        .map(<[u16]>::to_vec)
        .unwrap_or_default();
    let prefix = vm
        .string_utf16(ref_arg(args, 1))
        .map(<[u16]>::to_vec)
        .ok_or_else(|| vm.throw_new("java/lang/NullPointerException", None))?;
    if offset < 0 || (offset as usize) + prefix.len() > units.len() {
        return boolean(false);
    }
    boolean(units[offset as usize..offset as usize + prefix.len()] == prefix[..])
}

fn string_ends_with(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let units = vm
        .string_utf16(receiver(args))
        .map(<[u16]>::to_vec)
        .unwrap_or_default();
    let suffix = vm
        .string_utf16(ref_arg(args, 1))
        .map(<[u16]>::to_vec)
        .ok_or_else(|| vm.throw_new("java/lang/NullPointerException", None))?;
    boolean(units.len() >= suffix.len() && units[units.len() - suffix.len()..] == suffix[..])
}

fn string_contains(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let needle = ref_arg(args, 1);
    if needle.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let haystack = vm
        .string_utf16(receiver(args))
        .map(<[u16]>::to_vec)
        .unwrap_or_default();
    let needle = vm
        .string_utf16(needle)
        .map(<[u16]>::to_vec)
        .unwrap_or_default();
    boolean(find_units(&haystack, &needle, 0) >= 0)
}

fn string_to_char_array(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let units = vm
        .string_utf16(receiver(args))
        .map(<[u16]>::to_vec)
        .unwrap_or_default();
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Char, units.len())?;
    for (index, unit) in units.into_iter().enumerate() {
        vm.array_set(array, index as i32, Value::Int(i32::from(unit)), false)?;
    }
    object(array)
}

fn string_get_chars(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let units = vm
        .string_utf16(receiver(args))
        .map(<[u16]>::to_vec)
        .unwrap_or_default();
    let begin = int_arg(args, 1);
    let end = int_arg(args, 2);
    let destination = ref_arg(args, 3);
    let offset = int_arg(args, 4);
    if begin < 0 || end > units.len() as i32 || begin > end {
        return Err(vm.throw_new("java/lang/StringIndexOutOfBoundsException", None));
    }
    if destination.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    for (index, unit) in units[begin as usize..end as usize].iter().enumerate() {
        vm.array_set(
            destination,
            offset + index as i32,
            Value::Int(i32::from(*unit)),
            false,
        )?;
    }
    void()
}

fn string_trim(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let units = vm
        .string_utf16(receiver(args))
        .map(<[u16]>::to_vec)
        .unwrap_or_default();
    let start = units
        .iter()
        .position(|unit| *unit > 0x20)
        .unwrap_or(units.len());
    let end = units
        .iter()
        .rposition(|unit| *unit > 0x20)
        .map_or(start, |index| index + 1);
    object(vm.make_string_utf16(units[start..end].to_vec())?)
}

fn string_strip(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = vm.string_or_empty(receiver(args));
    let stripped: String = text.trim().to_string();
    object(vm.make_string(&stripped)?)
}

fn string_strip_leading(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = vm.string_or_empty(receiver(args));
    let stripped: String = text.trim_start().to_string();
    object(vm.make_string(&stripped)?)
}

fn string_strip_trailing(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = vm.string_or_empty(receiver(args));
    let stripped: String = text.trim_end().to_string();
    object(vm.make_string(&stripped)?)
}

fn string_is_blank(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = vm.string_or_empty(receiver(args));
    boolean(text.chars().all(char::is_whitespace))
}

fn string_repeat(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = vm.string_or_empty(receiver(args));
    let count = int_arg(args, 1);
    if count < 0 {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("count is negative"),
        ));
    }
    let mut out = String::new();
    for _ in 0..count {
        out.push_str(&text);
    }
    object(vm.make_string(&out)?)
}

fn string_replace_char(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let units = vm
        .string_utf16(receiver(args))
        .map(<[u16]>::to_vec)
        .unwrap_or_default();
    let old = int_arg(args, 1) as u16;
    let new = int_arg(args, 2) as u16;
    let replaced: Vec<u16> = units
        .into_iter()
        .map(|unit| if unit == old { new } else { unit })
        .collect();
    object(vm.make_string_utf16(replaced)?)
}

fn string_replace_sequence(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = vm.string_or_empty(receiver(args));
    let target = vm.string_or_empty(ref_arg(args, 1));
    let replacement = vm.string_or_empty(ref_arg(args, 2));
    let replaced = text.replace(&target, &replacement);
    object(vm.make_string(&replaced)?)
}

fn string_to_upper(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = vm.string_or_empty(receiver(args)).to_uppercase();
    object(vm.make_string(&text)?)
}

fn string_to_lower(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = vm.string_or_empty(receiver(args)).to_lowercase();
    object(vm.make_string(&text)?)
}

fn string_get_bytes(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = vm.string_or_empty(receiver(args));
    let bytes: Vec<u8> = text.into_bytes();
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, bytes.len())?;
    for (index, byte) in bytes.into_iter().enumerate() {
        vm.array_set(
            array,
            index as i32,
            Value::Int(i32::from(byte as i8)),
            false,
        )?;
    }
    object(array)
}

fn string_value_of_object(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let value = args[0];
    let text = vm.display_value(
        &oxjvm_classfile::FieldType::Object("java/lang/Object".into()),
        value,
    )?;
    object(vm.make_string(&text)?)
}

fn string_value_of_chars(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let units = char_array(vm, args[0].as_ref())?;
    object(vm.make_string_utf16(units)?)
}

fn string_value_of_chars_range(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let units = char_array(vm, args[0].as_ref())?;
    let offset = int_arg(args, 1);
    let count = int_arg(args, 2);
    string_range_check(vm, offset, count, units.len())?;
    let slice = units[offset as usize..(offset + count) as usize].to_vec();
    object(vm.make_string_utf16(slice)?)
}

fn string_value_of_bool(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    object(vm.make_string(if bool_arg(args, 0) { "true" } else { "false" })?)
}

fn string_value_of_char(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let unit = int_arg(args, 0) as u32 & 0xFFFF;
    let text = char::from_u32(unit).map_or_else(String::new, |ch| ch.to_string());
    object(vm.make_string(&text)?)
}

fn string_value_of_int(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = oxjvm_vm::format::int_to_string(int_arg(args, 0));
    object(vm.make_string(&text)?)
}

fn string_value_of_long(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = oxjvm_vm::format::long_to_string(long_arg(args, 0));
    object(vm.make_string(&text)?)
}

fn string_value_of_float(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = oxjvm_vm::format::float_to_string(float_arg(args, 0));
    object(vm.make_string(&text)?)
}

fn string_value_of_double(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = oxjvm_vm::format::double_to_string(double_arg(args, 0));
    object(vm.make_string(&text)?)
}

fn string_format(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let format = string_arg(vm, args[0])?;
    let mut arguments = Vec::new();
    if let Some(oxjvm_vm::ObjectData::Array(oxjvm_vm::ArrayData::Reference(elements))) =
        vm.heap.get(ref_arg(args, 1)).map(|object| &object.data)
    {
        arguments = elements.clone();
    }
    let text = format_string(vm, &format, &arguments)?;
    object(vm.make_string(&text)?)
}

/// A small `String.format` (no locale, `%s %d %f %x %b %c %n %%` plus width and `-`).
pub(crate) fn format_string(
    vm: &mut Vm<'_>,
    format: &str,
    arguments: &[ObjectRef],
) -> Result<String, VmError> {
    let mut out = String::new();
    let mut argument = 0usize;
    let mut chars = format.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            out.push(ch);
            continue;
        }
        let mut left_align = false;
        let mut width = 0usize;
        loop {
            match chars.peek() {
                Some('-') => {
                    left_align = true;
                    chars.next();
                }
                Some(digit) if digit.is_ascii_digit() => {
                    width = width * 10 + digit.to_digit(10).unwrap() as usize;
                    chars.next();
                }
                _ => break,
            }
        }
        let mut precision: Option<usize> = None;
        if chars.peek() == Some(&'.') {
            chars.next();
            let mut value = 0usize;
            while let Some(digit) = chars.peek().and_then(|digit| digit.to_digit(10)) {
                value = value * 10 + digit as usize;
                chars.next();
            }
            precision = Some(value);
        }
        let Some(spec) = chars.next() else { break };
        let text = match spec {
            '%' => "%".to_string(),
            'n' => "\n".to_string(),
            'b' => {
                let value = arguments.get(argument).copied().unwrap_or(ObjectRef::NULL);
                argument += 1;
                if value.is_null() {
                    "false".to_string()
                } else if let Some(text) = vm.string_value(value) {
                    if text.eq_ignore_ascii_case("true") {
                        "true"
                    } else {
                        "false"
                    }
                    .to_string()
                } else {
                    "true".to_string()
                }
            }
            'd' => {
                let value = arguments.get(argument).copied().unwrap_or(ObjectRef::NULL);
                argument += 1;
                let number = object_to_long(vm, value);
                oxjvm_vm::format::long_to_string(number)
            }
            'x' => {
                let value = arguments.get(argument).copied().unwrap_or(ObjectRef::NULL);
                argument += 1;
                let number = object_to_long(vm, value);
                oxjvm_vm::format::long_to_radix_string(number, 16)
            }
            'X' => {
                let value = arguments.get(argument).copied().unwrap_or(ObjectRef::NULL);
                argument += 1;
                let number = object_to_long(vm, value);
                oxjvm_vm::format::long_to_radix_string(number, 16).to_uppercase()
            }
            'c' => {
                let value = arguments.get(argument).copied().unwrap_or(ObjectRef::NULL);
                argument += 1;
                let unit = object_to_long(vm, value) as u32;
                char::from_u32(unit).map_or_else(String::new, |ch| ch.to_string())
            }
            's' | 'S' => {
                let value = arguments.get(argument).copied().unwrap_or(ObjectRef::NULL);
                argument += 1;
                vm.display_value(
                    &oxjvm_classfile::FieldType::Object("java/lang/Object".into()),
                    Value::Ref(value),
                )?
            }
            'f' => {
                let value = arguments.get(argument).copied().unwrap_or(ObjectRef::NULL);
                argument += 1;
                let number = object_to_double(vm, value);
                let digits = precision.unwrap_or(6);
                format_fixed(number, digits)
            }
            'e' | 'E' => {
                let value = arguments.get(argument).copied().unwrap_or(ObjectRef::NULL);
                argument += 1;
                let number = object_to_double(vm, value);
                let text = alloc::format!("{number:e}");
                if spec == 'E' {
                    text.to_uppercase()
                } else {
                    text
                }
            }
            other => alloc::format!("%{other}"),
        };
        if text.len() < width {
            let padding = width - text.len();
            if left_align {
                out.push_str(&text);
                for _ in 0..padding {
                    out.push(' ');
                }
            } else {
                for _ in 0..padding {
                    out.push(' ');
                }
                out.push_str(&text);
            }
        } else {
            out.push_str(&text);
        }
    }
    Ok(out)
}

fn format_fixed(value: f64, digits: usize) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    let text = alloc::format!("{value:.digits$}");
    text
}

fn object_to_long(vm: &mut Vm<'_>, value: ObjectRef) -> i64 {
    match vm.heap.get(value).map(|object| &object.data) {
        Some(oxjvm_vm::ObjectData::Instance(fields)) if !fields.is_empty() => match fields[0] {
            Value::Int(value) => i64::from(value),
            Value::Long(value) => value,
            _ => 0,
        },
        _ => 0,
    }
}

fn object_to_double(vm: &mut Vm<'_>, value: ObjectRef) -> f64 {
    match vm.heap.get(value).map(|object| &object.data) {
        Some(oxjvm_vm::ObjectData::Instance(fields)) if !fields.is_empty() => match fields[0] {
            Value::Int(value) => f64::from(value),
            Value::Long(value) => value as f64,
            Value::Float(value) => f64::from(value),
            Value::Double(value) => value,
            _ => 0.0,
        },
        _ => 0.0,
    }
}

fn string_join(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let delimiter = vm.string_or_empty(ref_arg(args, 0));
    let mut out = String::new();
    if let Some(oxjvm_vm::ObjectData::Array(oxjvm_vm::ArrayData::Reference(elements))) =
        vm.heap.get(ref_arg(args, 1)).map(|object| &object.data)
    {
        for (index, element) in elements.iter().enumerate() {
            if index > 0 {
                out.push_str(&delimiter);
            }
            if element.is_null() {
                out.push_str("null");
            } else {
                out.push_str(&vm.string_or_empty(*element));
            }
        }
    }
    object(vm.make_string(&out)?)
}

fn string_split(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = vm.string_or_empty(receiver(args));
    let separator = string_arg(vm, args[1])?;
    let parts: Vec<String> = if separator.is_empty() {
        text.chars().map(|ch| ch.to_string()).collect()
    } else {
        text.split(&separator as &str).map(str::to_string).collect()
    };
    let mut trimmed = parts;
    while trimmed.last().is_some_and(String::is_empty) {
        trimmed.pop();
    }
    string_array(vm, &trimmed)
}

fn string_split_limit(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = vm.string_or_empty(receiver(args));
    let separator = string_arg(vm, args[1])?;
    let limit = int_arg(args, 2);
    if limit <= 0 {
        return string_split(vm, _c, args);
    }
    let parts: Vec<String> = if separator.is_empty() {
        text.chars().map(|ch| ch.to_string()).collect()
    } else {
        text.splitn(limit as usize, &separator as &str)
            .map(str::to_string)
            .collect()
    };
    string_array(vm, &parts)
}

fn string_matches(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = vm.string_or_empty(receiver(args));
    let pattern = string_arg(vm, args[1])?;
    boolean(simple_regex_match(&pattern, &text))
}

/// A deliberately small regex engine for `String.matches`: literals and `.*`/`.`/`^`/`$`.
pub(crate) fn simple_regex_match(pattern: &str, text: &str) -> bool {
    if pattern == ".*" {
        return true;
    }
    if let Some(literal) = pattern.strip_prefix("^").and_then(|p| p.strip_suffix('$')) {
        return literal == text;
    }
    if let Some(prefix) = pattern.strip_suffix(".*") {
        return text.starts_with(prefix);
    }
    if let Some(suffix) = pattern.strip_prefix(".*") {
        return text.ends_with(suffix);
    }
    pattern == text
}

/// Build a `String[]` from Rust strings.
pub(crate) fn string_array(vm: &mut Vm<'_>, values: &[String]) -> Result<Value, VmError> {
    let string_class = vm.resolve_class("java/lang/String")?;
    let array = vm.allocate_object_array(string_class, values.len())?;
    for (index, value) in values.iter().enumerate() {
        let string = vm.make_string(value)?;
        vm.array_set_ref(array, index, string)?;
    }
    object(array)
}

pub(crate) const STRING: oxjvm_vm::NativeClass = {
    // The two method arrays are flattened here so the class exposes all overloads.
    class(
        "java/lang/String",
        Some("java/lang/Object"),
        &[
            "java/io/Serializable",
            "java/lang/Comparable",
            "java/lang/CharSequence",
        ],
        ACC_PUBLIC | ACC_FINAL,
        &[],
        &STRING_ALL_METHODS,
        None,
    )
};

/// Both `String` method arrays flattened; built by a const fn to stay `'static`.
const STRING_ALL_METHODS: [oxjvm_vm::NativeMethodDef; 57] = flatten_string_methods();

const fn flatten_string_methods() -> [oxjvm_vm::NativeMethodDef; 57] {
    let mut out = [method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()); 57];
    let mut index = 0;
    while index < 46 {
        out[index] = STRING_METHODS[index];
        index += 1;
    }
    let mut extra = 0;
    while extra < 11 {
        out[46 + extra] = STRING_METHODS_EXTRA[extra];
        extra += 1;
    }
    out
}

// -------------------------------------------------------------------------------------------
// StringBuilder / StringBuffer
// -------------------------------------------------------------------------------------------

const BUILDER_METHODS: [oxjvm_vm::NativeMethodDef; 33] = [
    method("<init>", "()V", ACC_PUBLIC, builder_init),
    method("<init>", "(I)V", ACC_PUBLIC, builder_init_capacity),
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        builder_init_string,
    ),
    method(
        "<init>",
        "(Ljava/lang/CharSequence;)V",
        ACC_PUBLIC,
        builder_init_sequence,
    ),
    method(
        "append",
        "(Ljava/lang/Object;)Ljava/lang/StringBuilder;",
        ACC_PUBLIC,
        builder_append_object,
    ),
    method(
        "append",
        "(Ljava/lang/String;)Ljava/lang/StringBuilder;",
        ACC_PUBLIC,
        builder_append_string,
    ),
    method(
        "append",
        "(Ljava/lang/CharSequence;)Ljava/lang/StringBuilder;",
        ACC_PUBLIC,
        builder_append_sequence,
    ),
    method(
        "append",
        "([C)Ljava/lang/StringBuilder;",
        ACC_PUBLIC,
        builder_append_chars,
    ),
    method(
        "append",
        "([CII)Ljava/lang/StringBuilder;",
        ACC_PUBLIC,
        builder_append_chars_range,
    ),
    method(
        "append",
        "(Z)Ljava/lang/StringBuilder;",
        ACC_PUBLIC,
        builder_append_bool,
    ),
    method(
        "append",
        "(C)Ljava/lang/StringBuilder;",
        ACC_PUBLIC,
        builder_append_char,
    ),
    method(
        "append",
        "(I)Ljava/lang/StringBuilder;",
        ACC_PUBLIC,
        builder_append_int,
    ),
    method(
        "append",
        "(J)Ljava/lang/StringBuilder;",
        ACC_PUBLIC,
        builder_append_long,
    ),
    method(
        "append",
        "(F)Ljava/lang/StringBuilder;",
        ACC_PUBLIC,
        builder_append_float,
    ),
    method(
        "append",
        "(D)Ljava/lang/StringBuilder;",
        ACC_PUBLIC,
        builder_append_double,
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        builder_to_string,
    ),
    method("length", "()I", ACC_PUBLIC, builder_length),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, c, args| {
        boolean(builder_length(vm, c, args)?.as_int() == 0)
    }),
    method("charAt", "(I)C", ACC_PUBLIC, builder_char_at),
    method("setLength", "(I)V", ACC_PUBLIC, builder_set_length),
    method("setCharAt", "(IC)V", ACC_PUBLIC, builder_set_char_at),
    method(
        "reverse",
        "()Ljava/lang/StringBuilder;",
        ACC_PUBLIC,
        builder_reverse,
    ),
    method(
        "insert",
        "(ILjava/lang/String;)Ljava/lang/StringBuilder;",
        ACC_PUBLIC,
        builder_insert_string,
    ),
    method(
        "delete",
        "(II)Ljava/lang/StringBuilder;",
        ACC_PUBLIC,
        builder_delete,
    ),
    method(
        "deleteCharAt",
        "(I)Ljava/lang/StringBuilder;",
        ACC_PUBLIC,
        builder_delete_char_at,
    ),
    method(
        "indexOf",
        "(Ljava/lang/String;)I",
        ACC_PUBLIC,
        builder_index_of,
    ),
    method("capacity", "()I", ACC_PUBLIC, builder_length),
    method("ensureCapacity", "(I)V", ACC_PUBLIC, |_, _, _| void()),
    method("trimToSize", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "append",
        "([CII)Ljava/lang/StringBuilder;",
        ACC_PUBLIC,
        builder_append_chars_range,
    ),
    method(
        "append",
        "(Ljava/lang/StringBuffer;)Ljava/lang/StringBuilder;",
        ACC_PUBLIC,
        builder_append_object,
    ),
    method(
        "replace",
        "(IILjava/lang/String;)Ljava/lang/StringBuilder;",
        ACC_PUBLIC,
        builder_replace,
    ),
    method(
        "substring",
        "(II)Ljava/lang/String;",
        ACC_PUBLIC,
        builder_substring,
    ),
];

const BUFFER_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method("<init>", "()V", ACC_PUBLIC, builder_init),
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        builder_init_string,
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        builder_to_string,
    ),
    method(
        "append",
        "(Ljava/lang/String;)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        builder_append_object,
    ),
];

fn builder_state(vm: &Vm<'_>, this: ObjectRef) -> (Vec<u16>, usize) {
    let class = vm.class_of(this);
    let count = read_int_field(vm, this, "count").max(0) as usize;
    let chars = match vm.read_ref_field(this, class, "value", "[C") {
        Some(array) => match vm.heap.get(array).map(|object| &object.data) {
            Some(oxjvm_vm::ObjectData::Array(oxjvm_vm::ArrayData::Char(units))) => units.clone(),
            _ => Vec::new(),
        },
        None => Vec::new(),
    };
    (chars, count)
}

pub(crate) fn read_int_field(vm: &Vm<'_>, object: ObjectRef, name: &str) -> i32 {
    let class = vm.class_of(object);
    vm.read_instance_int_named(object, class, name, "I")
        .unwrap_or(0)
}

pub(crate) fn write_int_field(vm: &mut Vm<'_>, object: ObjectRef, name: &str, value: i32) {
    let class = vm.class_of(object);
    if let Ok((declaring, field)) = vm.find_field(class, name, "I") {
        vm.set_instance_int(object, declaring, field, value);
    }
}

fn builder_store(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    units: &[u16],
    count: usize,
) -> Result<(), VmError> {
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "value", "[C")?;
    let array_slot = vm.classes.get(declaring).fields[field as usize].slot as usize;
    let need = units.len().max(count);
    let existing = vm.read_ref_field(this, declaring, "value", "[C");
    if existing.is_none() || vm.array_length(existing.expect("checked")).unwrap_or(0) < need {
        let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Char, need.max(16))?;
        vm.set_instance_ref(this, declaring, field, array);
    }
    let array = vm
        .read_ref_field(this, declaring, "value", "[C")
        .expect("just allocated");
    for (index, unit) in units.iter().take(count).enumerate() {
        vm.array_set(array, index as i32, Value::Int(i32::from(*unit)), false)?;
    }
    let _ = array_slot;
    write_int_field_named(vm, this, "count", count as i32, declaring, field)?;
    Ok(())
}

fn write_int_field_named(
    vm: &mut Vm<'_>,
    object: ObjectRef,
    name: &str,
    value: i32,
    _declaring: ClassId,
    _field: u32,
) -> Result<(), VmError> {
    let class = vm.class_of(object);
    let (declaring, field) = vm.find_field(class, name, "I")?;
    vm.set_instance_int(object, declaring, field, value);
    Ok(())
}

fn builder_init(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    builder_store(vm, receiver(args), &[], 0)?;
    void()
}

fn builder_init_capacity(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    builder_init(vm, _c, args)
}

fn builder_init_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = vm
        .string_utf16(ref_arg(args, 1))
        .map(<[u16]>::to_vec)
        .unwrap_or_default();
    builder_store(vm, receiver(args), &text, text.len())?;
    void()
}

fn builder_init_sequence(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let reference = ref_arg(args, 1);
    if reference.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let text = vm.string_or_empty(reference);
    let units: Vec<u16> = text.encode_utf16().collect();
    builder_store(vm, receiver(args), &units, units.len())?;
    void()
}

fn builder_append_object(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = vm.display_value(
        &oxjvm_classfile::FieldType::Object("java/lang/Object".into()),
        args[1],
    )?;
    builder_append_text(vm, receiver(args), &text)
}

fn builder_append_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = match vm.string_value(ref_arg(args, 1)) {
        Some(text) => text,
        None => "null".into(),
    };
    builder_append_text(vm, receiver(args), &text)
}

fn builder_append_sequence(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let reference = ref_arg(args, 1);
    let text = if reference.is_null() {
        "null".into()
    } else {
        vm.string_or_empty(reference)
    };
    builder_append_text(vm, receiver(args), &text)
}

fn builder_append_text(vm: &mut Vm<'_>, this: ObjectRef, text: &str) -> Result<Value, VmError> {
    let (mut units, count) = builder_state(vm, this);
    if units.len() < count + text.len() {
        units.resize(count + text.len(), 0);
    }
    let new_units: Vec<u16> = text.encode_utf16().collect();
    for (index, unit) in new_units.iter().enumerate() {
        units[count + index] = *unit;
    }
    builder_store(vm, this, &units, count + new_units.len())?;
    object(this)
}

fn builder_append_chars(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let units = char_array(vm, ref_arg(args, 1))?;
    let (mut current, count) = builder_state(vm, receiver(args));
    current.truncate(count);
    current.extend_from_slice(&units);
    builder_store(vm, receiver(args), &current, count + units.len())?;
    object(receiver(args))
}

fn builder_append_chars_range(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let units = char_array(vm, ref_arg(args, 1))?;
    let offset = int_arg(args, 2);
    let length = int_arg(args, 3);
    string_range_check(vm, offset, length, units.len())?;
    let slice = units[offset as usize..(offset + length) as usize].to_vec();
    builder_append_text(vm, receiver(args), &String::from_utf16_lossy(&slice))
}

fn builder_append_bool(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    builder_append_text(
        vm,
        receiver(args),
        if bool_arg(args, 1) { "true" } else { "false" },
    )
}

fn builder_append_char(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let unit = int_arg(args, 1) as u32 & 0xFFFF;
    let ch = char::from_u32(unit).unwrap_or('\u{FFFD}');
    let mut text = [0u8; 4];
    builder_append_text(vm, receiver(args), ch.encode_utf8(&mut text))
}

fn builder_append_int(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = oxjvm_vm::format::int_to_string(int_arg(args, 1));
    builder_append_text(vm, receiver(args), &text)
}

fn builder_append_long(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = oxjvm_vm::format::long_to_string(long_arg(args, 1));
    builder_append_text(vm, receiver(args), &text)
}

fn builder_append_float(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = oxjvm_vm::format::float_to_string(float_arg(args, 1));
    builder_append_text(vm, receiver(args), &text)
}

fn builder_append_double(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = oxjvm_vm::format::double_to_string(double_arg(args, 1));
    builder_append_text(vm, receiver(args), &text)
}

fn builder_to_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let (units, count) = builder_state(vm, receiver(args));
    object(vm.make_string_utf16(units[..count.min(units.len())].to_vec())?)
}

fn builder_length(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let (_, count) = builder_state(vm, receiver(args));
    int(count as i32)
}

fn builder_char_at(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let (units, count) = builder_state(vm, receiver(args));
    let index = int_arg(args, 1);
    if index < 0 || index as usize >= count || index as usize >= units.len() {
        return Err(vm.throw_new(
            "java/lang/StringIndexOutOfBoundsException",
            Some(&alloc::format!("index {index}, length {count}")),
        ));
    }
    int(i32::from(units[index as usize]))
}

fn builder_set_length(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let length = int_arg(args, 1);
    if length < 0 {
        return Err(vm.throw_new(
            "java/lang/StringIndexOutOfBoundsException",
            Some(&alloc::format!("String index out of range: {length}")),
        ));
    }
    let (mut units, count) = builder_state(vm, receiver(args));
    let length = length as usize;
    if units.len() < length {
        units.resize(length, 0);
    }
    builder_store(vm, receiver(args), &units, length.min(units.len()))
        .or_else(|_| builder_store(vm, receiver(args), &units, length))
        .map_err(|_: VmError| VmError::internal("setLength"))?;
    let _ = count;
    void()
}

fn builder_set_char_at(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let (mut units, count) = builder_state(vm, receiver(args));
    let index = int_arg(args, 1);
    if index < 0 || index as usize >= count {
        return Err(vm.throw_new(
            "java/lang/StringIndexOutOfBoundsException",
            Some(&alloc::format!("index {index}, length {count}")),
        ));
    }
    units[index as usize] = (int_arg(args, 2) as u32 & 0xFFFF) as u16;
    builder_store(vm, receiver(args), &units, count)?;
    void()
}

fn builder_reverse(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let (mut units, count) = builder_state(vm, receiver(args));
    units[..count].reverse();
    builder_store(vm, receiver(args), &units, count)?;
    object(receiver(args))
}

fn builder_insert_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let offset = int_arg(args, 1);
    let text = vm.string_or_empty(ref_arg(args, 2));
    let (units, count) = builder_state(vm, receiver(args));
    let offset = offset.max(0) as usize;
    if offset > count {
        return Err(vm.throw_new(
            "java/lang/StringIndexOutOfBoundsException",
            Some(&alloc::format!("offset {offset}, length {count}")),
        ));
    }
    let mut out: Vec<u16> = units[..count].to_vec();
    let inserted: Vec<u16> = text.encode_utf16().collect();
    let mut tail = out.split_off(offset);
    out.extend_from_slice(&inserted);
    out.append(&mut tail);
    builder_store(vm, receiver(args), &out, out.len())?;
    object(receiver(args))
}

fn builder_delete(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let (units, count) = builder_state(vm, receiver(args));
    let start = int_arg(args, 1);
    let end = int_arg(args, 2);
    if start < 0 || start > end || end > count as i32 {
        return Err(vm.throw_new("java/lang/StringIndexOutOfBoundsException", None));
    }
    let mut out: Vec<u16> = units[..count].to_vec();
    out.drain(start as usize..end as usize);
    builder_store(vm, receiver(args), &out, out.len())?;
    object(receiver(args))
}

fn builder_delete_char_at(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let index = int_arg(args, 1);
    let mut call = vec![args[0], Value::Int(index), Value::Int(index + 1)];
    let _ = &mut call;
    builder_delete(vm, _c, &call)
}

fn builder_index_of(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let (units, count) = builder_state(vm, receiver(args));
    let needle = vm
        .string_utf16(ref_arg(args, 1))
        .map(<[u16]>::to_vec)
        .unwrap_or_default();
    int(find_units(&units[..count.min(units.len())], &needle, 0))
}

fn builder_replace(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let (units, count) = builder_state(vm, receiver(args));
    let start = int_arg(args, 1).max(0) as usize;
    let end = int_arg(args, 2).max(0).min(count as i32) as usize;
    let text = vm.string_or_empty(ref_arg(args, 3));
    let mut out: Vec<u16> = units[..count].to_vec();
    out.splice(start..end.max(start).min(out.len()), text.encode_utf16());
    builder_store(vm, receiver(args), &out, out.len())?;
    object(receiver(args))
}

fn builder_substring(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let (units, count) = builder_state(vm, receiver(args));
    let start = int_arg(args, 1);
    let end = int_arg(args, 2);
    if start < 0 || end > count as i32 || start > end {
        return Err(vm.throw_new("java/lang/StringIndexOutOfBoundsException", None));
    }
    object(vm.make_string_utf16(units[start as usize..end as usize].to_vec())?)
}

pub(crate) const STRING_BUILDER: oxjvm_vm::NativeClass = class(
    "java/lang/StringBuilder",
    Some("java/lang/Object"),
    &[
        "java/io/Serializable",
        "java/lang/Comparable",
        "java/lang/CharSequence",
        "java/lang/Appendable",
    ],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("value", "[C", ACC_PRIVATE, None),
        field("count", "I", ACC_PRIVATE, None),
    ],
    &BUILDER_METHODS,
    None,
);

pub(crate) const STRING_BUFFER: oxjvm_vm::NativeClass = class(
    "java/lang/StringBuffer",
    Some("java/lang/Object"),
    &[
        "java/io/Serializable",
        "java/lang/Comparable",
        "java/lang/CharSequence",
        "java/lang/Appendable",
    ],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("value", "[C", ACC_PRIVATE, None),
        field("count", "I", ACC_PRIVATE, None),
    ],
    &BUFFER_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// System / Runtime
// -------------------------------------------------------------------------------------------

const SYSTEM_METHODS: [oxjvm_vm::NativeMethodDef; 17] = [
    method(
        "currentTimeMillis",
        "()J",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| long(vm.host_time_millis()),
    ),
    method("nanoTime", "()J", ACC_PUBLIC | ACC_STATIC, |vm, _, _| {
        long(vm.host_nano_time())
    }),
    method(
        "identityHashCode",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let reference = ref_arg(args, 0);
            let hash = vm.heap.get(reference).map_or(0, |object| object.hash);
            int(hash)
        },
    ),
    method(
        "arraycopy",
        "(Ljava/lang/Object;ILjava/lang/Object;II)V",
        ACC_PUBLIC | ACC_STATIC,
        system_arraycopy,
    ),
    method("exit", "(I)V", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        Err(Vm::exit_vm(int_arg(args, 0)))
    }),
    method("gc", "()V", ACC_PUBLIC | ACC_STATIC, |vm, _, _| {
        vm.gc();
        void()
    }),
    method(
        "getProperty",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        system_get_property,
    ),
    method(
        "getProperty",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        system_get_property_or_default,
    ),
    method(
        "setProperty",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "clearProperty",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getenv",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "lineSeparator",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(vm.make_string("\n")?),
    ),
    method(
        "setOut",
        "(Ljava/io/PrintStream;)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let stream = ref_arg(args, 0);
            vm.set_stdout_object(stream);
            let system = system_class(vm)?;
            vm.set_static_value(system, "out", "Ljava/io/PrintStream;", Value::Ref(stream));
            void()
        },
    ),
    method(
        "setErr",
        "(Ljava/io/PrintStream;)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let stream = ref_arg(args, 0);
            vm.set_stderr_object(stream);
            let system = system_class(vm)?;
            vm.set_static_value(system, "err", "Ljava/io/PrintStream;", Value::Ref(stream));
            void()
        },
    ),
    method(
        "setIn",
        "(Ljava/io/InputStream;)V",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| void(),
    ),
    method(
        "getSecurityManager",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "runFinalization",
        "()V",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| void(),
    ),
];

fn system_class(vm: &mut Vm<'_>) -> Result<ClassId, VmError> {
    vm.resolve_class("java/lang/System")
}

/// Looks up a system property: the VM's well-known defaults first, then the host.
pub(crate) fn system_property(vm: &mut Vm<'_>, key: &str) -> Option<String> {
    match key {
        "java.version" | "java.specification.version" => Some("17".to_string()),
        "java.vm.name" => Some("oxjvm".to_string()),
        "java.vm.version" => Some("0.1.0".to_string()),
        "os.name" => Some("oxjvm".to_string()),
        "os.arch" | "os.version" => Some("unknown".to_string()),
        "file.separator" => Some("/".to_string()),
        "path.separator" => Some(":".to_string()),
        "line.separator" => Some("\n".to_string()),
        "user.dir" => Some("/".to_string()),
        "java.class.path" => Some(String::new()),
        "java.io.tmpdir" => Some("/tmp".to_string()),
        _ => vm.host_property(key),
    }
}

fn system_get_property(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let key = string_arg(vm, args[0])?;
    match system_property(vm, &key) {
        Some(text) => object(vm.make_string(&text)?),
        None => object(ObjectRef::NULL),
    }
}

/// `System.getProperty(String, String)`: returns the supplied default when unset.
fn system_get_property_or_default(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let key = string_arg(vm, args[0])?;
    match system_property(vm, &key) {
        Some(text) => object(vm.make_string(&text)?),
        None => object(ref_arg(args, 1)),
    }
}

fn system_arraycopy(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let source = ref_arg(args, 0);
    let source_pos = int_arg(args, 1);
    let destination = ref_arg(args, 2);
    let destination_pos = int_arg(args, 3);
    let length = int_arg(args, 4);
    vm.array_copy(source, source_pos, destination, destination_pos, length)?;
    void()
}

/// Wiring for `System.out`, `System.err`, and `System.in`, run at class initialization.
fn system_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let print_stream = vm.resolve_class("java/io/PrintStream")?;
    let ctor = vm
        .find_method(print_stream, "<init>", "(I)V")
        .ok_or_else(|| VmError::internal("PrintStream(int) constructor missing"))?;
    for (kind, is_out) in [(1i32, true), (2i32, false)] {
        let stream = vm.new_instance(print_stream)?;
        vm.invoke_method(ctor.0, ctor.1, vec![Value::Ref(stream), Value::Int(kind)])?;
        if is_out {
            vm.set_stdout_object(stream);
        } else {
            vm.set_stderr_object(stream);
        }
    }
    let system = system_class(vm)?;
    vm.set_static_value(
        system,
        "out",
        "Ljava/io/PrintStream;",
        Value::Ref(vm.stdout_object()),
    );
    vm.set_static_value(
        system,
        "err",
        "Ljava/io/PrintStream;",
        Value::Ref(vm.stderr_object()),
    );
    Ok(())
}

pub(crate) const SYSTEM: oxjvm_vm::NativeClass = class(
    "java/lang/System",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field(
            "out",
            "Ljava/io/PrintStream;",
            ACC_PUBLIC | ACC_STATIC,
            None,
        ),
        field(
            "err",
            "Ljava/io/PrintStream;",
            ACC_PUBLIC | ACC_STATIC,
            None,
        ),
    ],
    &SYSTEM_METHODS,
    Some(system_clinit),
);

const RUNTIME_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
    method(
        "getRuntime",
        "()Ljava/lang/Runtime;",
        ACC_PUBLIC | ACC_STATIC,
        runtime_get_runtime,
    ),
    method("availableProcessors", "()I", ACC_PUBLIC, |_, _, _| int(1)),
    method("freeMemory", "()J", ACC_PUBLIC, |_, _, _| long(1 << 26)),
    method("totalMemory", "()J", ACC_PUBLIC, |_, _, _| long(1 << 26)),
    method("maxMemory", "()J", ACC_PUBLIC, |_, _, _| long(1 << 30)),
    method("gc", "()V", ACC_PUBLIC, |vm, _, _| {
        vm.gc();
        void()
    }),
    method("exit", "(I)V", ACC_PUBLIC, |_, _, args| {
        Err(Vm::exit_vm(int_arg(args, 0)))
    }),
    method("halt", "(I)V", ACC_PUBLIC, |_, _, args| {
        Err(Vm::exit_vm(int_arg(args, 0)))
    }),
    method(
        "addShutdownHook",
        "(Ljava/lang/Thread;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "removeShutdownHook",
        "(Ljava/lang/Thread;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(false),
    ),
    method(
        "version",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(vm.make_string("17")?),
    ),
];

fn runtime_get_runtime(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    _args: &[Value],
) -> Result<Value, VmError> {
    let system = vm.resolve_class("java/lang/Runtime")?;
    match vm.read_static_ref(system, "currentRuntime", "Ljava/lang/Runtime;") {
        Some(existing) => object(existing),
        None => {
            let instance = vm.new_instance(system)?;
            vm.set_static_value(
                system,
                "currentRuntime",
                "Ljava/lang/Runtime;",
                Value::Ref(instance),
            );
            object(instance)
        }
    }
}

pub(crate) const RUNTIME: oxjvm_vm::NativeClass = class(
    "java/lang/Runtime",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[field(
        "currentRuntime",
        "Ljava/lang/Runtime;",
        ACC_PRIVATE | ACC_STATIC | ACC_SYNTHETIC,
        None,
    )],
    &RUNTIME_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Thread
// -------------------------------------------------------------------------------------------

const THREAD_METHODS: [oxjvm_vm::NativeMethodDef; 27] = [
    method("<init>", "()V", ACC_PUBLIC, thread_init),
    method("<init>", "(Ljava/lang/Runnable;)V", ACC_PUBLIC, thread_init),
    method("<init>", "(Ljava/lang/String;)V", ACC_PUBLIC, thread_init),
    method(
        "<init>",
        "(Ljava/lang/Runnable;Ljava/lang/String;)V",
        ACC_PUBLIC,
        thread_init,
    ),
    method(
        "currentThread",
        "()Ljava/lang/Thread;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(vm.current_thread_object()),
    ),
    method("start", "()V", ACC_PUBLIC, thread_start),
    method("run", "()V", ACC_PUBLIC, thread_run),
    method("join", "()V", ACC_PUBLIC, |vm, _, args| {
        vm.join_thread(receiver(args), None);
        void()
    }),
    method("join", "(J)V", ACC_PUBLIC, |vm, _, args| {
        let millis = long_arg(args, 1);
        if millis < 0 {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("timeout value is negative"),
            ));
        }
        vm.join_thread(
            receiver(args),
            if millis == 0 { None } else { Some(millis) },
        );
        void()
    }),
    method("join", "(JI)V", ACC_PUBLIC, |vm, _, args| {
        let millis = long_arg(args, 1);
        let nanos = int_arg(args, 2);
        if millis < 0 {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("timeout value is negative"),
            ));
        }
        if !(0..=999_999).contains(&nanos) {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("nanosecond timeout value out of range"),
            ));
        }
        let total = if millis == 0 && nanos == 0 {
            None
        } else {
            Some(millis + i64::from(nanos > 0))
        };
        vm.join_thread(receiver(args), total);
        void()
    }),
    method("sleep", "(J)V", ACC_PUBLIC | ACC_STATIC, |vm, _, args| {
        let millis = long_arg(args, 0);
        if millis < 0 {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("timeout value is negative"),
            ));
        }
        vm.sleep_current(millis);
        void()
    }),
    method("sleep", "(JI)V", ACC_PUBLIC | ACC_STATIC, |vm, _, args| {
        let millis = long_arg(args, 0);
        let nanos = int_arg(args, 1);
        if millis < 0 {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("timeout value is negative"),
            ));
        }
        if !(0..=999_999).contains(&nanos) {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("nanosecond timeout value out of range"),
            ));
        }
        vm.sleep_current(millis + i64::from(nanos > 0));
        void()
    }),
    method("yield", "()V", ACC_PUBLIC | ACC_STATIC, |vm, _, _| {
        vm.yield_current();
        void()
    }),
    method("isAlive", "()Z", ACC_PUBLIC | ACC_FINAL, |vm, _, args| {
        boolean(vm.thread_is_alive(receiver(args)))
    }),
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let name = vm
                .thread_name_of(receiver(args))
                .unwrap_or_else(|| "Thread".into());
            object(vm.make_string(&name)?)
        },
    ),
    method(
        "setName",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let name = string_arg(vm, args[1])?;
            vm.set_thread_name(receiver(args), &name)?;
            void()
        },
    ),
    method(
        "getPriority",
        "()I",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| int(vm.thread_priority(receiver(args)).unwrap_or(5)),
    ),
    method(
        "setPriority",
        "(I)V",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let priority = int_arg(args, 1);
            if !(1..=10).contains(&priority) {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("priority out of range"),
                ));
            }
            vm.set_thread_priority(receiver(args), priority);
            void()
        },
    ),
    method("isDaemon", "()Z", ACC_PUBLIC | ACC_FINAL, |vm, _, args| {
        boolean(vm.thread_is_daemon(receiver(args)))
    }),
    method(
        "setDaemon",
        "(Z)V",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let daemon = bool_arg(args, 1);
            vm.set_thread_daemon(receiver(args), daemon)?;
            void()
        },
    ),
    method("interrupt", "()V", ACC_PUBLIC, |vm, _, args| {
        vm.interrupt_thread(receiver(args));
        void()
    }),
    method("isInterrupted", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(vm.thread_interrupted_flag(receiver(args)))
    }),
    method("interrupted", "()Z", ACC_PUBLIC | ACC_STATIC, |vm, _, _| {
        let current = vm.current_thread_object();
        let flag = vm.thread_interrupted_flag(current);
        vm.clear_thread_interrupted(current);
        boolean(flag)
    }),
    method("getId", "()J", ACC_PUBLIC, |vm, _, args| {
        long(vm.thread_id(receiver(args)))
    }),
    method(
        "holdsLock",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let object = ref_arg(args, 0);
            if object.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let current = vm.current_thread_object();
            boolean(vm.holds_monitor(object, current))
        },
    ),
    method("activeCount", "()I", ACC_PUBLIC | ACC_STATIC, |vm, _, _| {
        int(vm.live_thread_count() as i32)
    }),
    method("dumpStack", "()V", ACC_PUBLIC | ACC_STATIC, |_, _, _| {
        void()
    }),
];

fn thread_init(
    vm: &mut Vm<'_>,
    ctx: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let descriptor = vm.classes.get(ctx.class).methods[ctx.method as usize]
        .descriptor
        .clone();
    let mut target = ObjectRef::NULL;
    let mut name: Option<String> = None;
    for value in &args[1..] {
        if let Value::Ref(reference) = value {
            if !reference.is_null() {
                if vm.string_utf16(*reference).is_some() {
                    name = vm.string_value(*reference);
                } else {
                    let runnable = vm.resolve_class_lenient_pub("java/lang/Runnable");
                    if runnable.is_some_and(|runnable| vm.is_instance(*reference, runnable)) {
                        target = *reference;
                    }
                }
            }
        }
    }
    let thread = vm.class_of(this);
    let name = name.unwrap_or_else(|| alloc::format!("Thread-{}", vm.thread_id(this)));
    vm.set_string_field(this, thread, "name", &name)?;
    let (declaring, field) = vm.find_field(thread, "target", "Ljava/lang/Runnable;")?;
    vm.set_instance_ref(this, declaring, field, target);
    let (declaring, field) = vm.find_field(thread, "priority", "I")?;
    vm.set_instance_int(this, declaring, field, 5);
    let (declaring, field) = vm.find_field(thread, "daemon", "Z")?;
    vm.set_instance_int(this, declaring, field, 0);
    let _ = descriptor;
    void()
}

fn thread_start(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    vm.start_thread(receiver(args))?;
    void()
}

fn thread_run(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let class = vm.class_of(this);
    let target = vm.read_ref_field(this, class, "target", "Ljava/lang/Runnable;");
    if let Some(target) = target {
        if !target.is_null() {
            let (declaring, method) = vm.resolve_virtual_method(target, "run", "()V")?;
            vm.invoke_method(declaring, method, vec![Value::Ref(target)])?;
        }
    }
    void()
}

pub(crate) const THREAD: oxjvm_vm::NativeClass = class(
    "java/lang/Thread",
    Some("java/lang/Object"),
    &["java/lang/Runnable"],
    ACC_PUBLIC,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("priority", "I", ACC_PRIVATE, None),
        field("daemon", "Z", ACC_PRIVATE, None),
        field("target", "Ljava/lang/Runnable;", ACC_PRIVATE, None),
        field("started", "Z", ACC_PRIVATE, None),
    ],
    &THREAD_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// ClassLoader
// -------------------------------------------------------------------------------------------

const CLASS_LOADER_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "getSystemClassLoader",
        "()Ljava/lang/ClassLoader;",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getParent",
        "()Ljava/lang/ClassLoader;",
        ACC_PUBLIC | ACC_FINAL,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "loadClass",
        "(Ljava/lang/String;)Ljava/lang/Class;",
        ACC_PUBLIC,
        class_loader_load_class,
    ),
    method(
        "getResource",
        "(Ljava/lang/String;)Ljava/net/URL;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
];

fn class_loader_load_class(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let name = string_arg(vm, args[1])?;
    let internal = name.replace('.', "/");
    let class = vm
        .resolve_class(&internal)
        .map_err(|_| vm.throw_new("java/lang/ClassNotFoundException", Some(&name)))?;
    let class_object = vm.class_object(class)?;
    object(class_object)
}

pub(crate) const CLASS_LOADER: oxjvm_vm::NativeClass = class(
    "java/lang/ClassLoader",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &CLASS_LOADER_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Enum / Record
// -------------------------------------------------------------------------------------------

const ENUM_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method("<init>", "(Ljava/lang/String;I)V", ACC_PROTECTED, enum_init),
    method(
        "name",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let name = vm
                .read_string_field(this, class, "name")
                .unwrap_or_default();
            object(vm.make_string(&name)?)
        },
    ),
    method("ordinal", "()I", ACC_PUBLIC | ACC_FINAL, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "ordinal"))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let name = vm
                .read_string_field(this, class, "name")
                .unwrap_or_default();
            object(vm.make_string(&name)?)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_FINAL,
        |_vm, _, args| boolean(receiver(args) == ref_arg(args, 1)),
    ),
    method("hashCode", "()I", ACC_PUBLIC | ACC_FINAL, |vm, _, args| {
        int(vm.heap.get(receiver(args)).map_or(0, |object| object.hash))
    }),
    method(
        "compareTo",
        "(Ljava/lang/Enum;)I",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            if vm.class_of(this) != vm.class_of(other) {
                return Err(vm.throw_new("java/lang/ClassCastException", None));
            }
            int(read_int_field(vm, this, "ordinal") - read_int_field(vm, other, "ordinal"))
        },
    ),
    method(
        "getDeclaringClass",
        "()Ljava/lang/Class;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let class = vm.class_of(receiver(args));
            let class_object = vm.class_object(class)?;
            object(class_object)
        },
    ),
];

fn enum_init(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let class = vm.class_of(this);
    let name = ref_arg(args, 1);
    let ordinal = int_arg(args, 2);
    let (declaring, field) = vm.find_field(class, "name", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, name);
    write_int_field(vm, this, "ordinal", ordinal);
    void()
}

pub(crate) const ENUM: oxjvm_vm::NativeClass = class(
    "java/lang/Enum",
    Some("java/lang/Object"),
    &["java/io/Serializable", "java/lang/Comparable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE | ACC_FINAL, None),
        field("ordinal", "I", ACC_PRIVATE | ACC_FINAL, None),
    ],
    &ENUM_METHODS,
    None,
);

pub(crate) const RECORD: oxjvm_vm::NativeClass = class(
    "java/lang/Record",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &[method("<init>", "()V", ACC_PROTECTED, |_, _, _| void())],
    None,
);

// -------------------------------------------------------------------------------------------
// Interfaces and marker types
// -------------------------------------------------------------------------------------------

fn abstract_method(
    vm: &mut Vm<'_>,
    ctx: oxjvm_vm::NativeContext,
    _args: &[Value],
) -> Result<Value, VmError> {
    let class = vm.class_name(ctx.class).to_string();
    let method = vm.classes.get(ctx.class).methods[ctx.method as usize]
        .name
        .clone();
    Err(vm.throw_new(
        "java/lang/AbstractMethodError",
        Some(&alloc::format!("{class}.{method}")),
    ))
}

/// The abstract-method implementation, exposed for other native modules.
pub(crate) fn abstract_method_pub(
    vm: &mut Vm<'_>,
    ctx: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    abstract_method(vm, ctx, args)
}

/// Read one byte from a `ByteArrayInputStream`-shaped object (`buf`/`pos` fields), advancing the
/// position. Returns `None` at end of stream.
pub(crate) fn read_input_byte(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Option<i32>, VmError> {
    let class = vm.class_of(this);
    let buffer = vm
        .read_ref_field(this, class, "buf", "[B")
        .unwrap_or(ObjectRef::NULL);
    if buffer.is_null() {
        return Ok(None);
    }
    let position = read_int_field(vm, this, "pos");
    let length = vm.array_length(buffer).unwrap_or(0);
    if position < 0 || position as usize >= length {
        return Ok(None);
    }
    let byte = vm.array_get(buffer, position)?.as_int();
    write_int_field(vm, this, "pos", position + 1);
    Ok(Some(byte))
}

const CHAR_SEQUENCE_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method("length", "()I", ACC_PUBLIC | ACC_ABSTRACT, abstract_method),
    method("charAt", "(I)C", ACC_PUBLIC | ACC_ABSTRACT, abstract_method),
    method(
        "subSequence",
        "(II)Ljava/lang/CharSequence;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method,
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method,
    ),
    method("isEmpty", "()Z", ACC_PUBLIC, char_sequence_is_empty),
    method(
        "chars",
        "()Ljava/util/stream/IntStream;",
        ACC_PUBLIC,
        char_sequence_unsupported,
    ),
];

fn char_sequence_is_empty(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let (class, method) = vm.resolve_virtual_method(receiver(args), "length", "()I")?;
    let length = vm.invoke_method(class, method, args.to_vec())?;
    boolean(length.as_int() == 0)
}

fn char_sequence_unsupported(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    _args: &[Value],
) -> Result<Value, VmError> {
    Err(crate::unsupported(vm, "CharSequence.chars"))
}

pub(crate) const CHAR_SEQUENCE: oxjvm_vm::NativeClass = class(
    "java/lang/CharSequence",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &CHAR_SEQUENCE_METHODS,
    None,
);

const COMPARABLE_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "compareTo",
    "(Ljava/lang/Object;)I",
    ACC_PUBLIC | ACC_ABSTRACT,
    abstract_method,
)];

pub(crate) const COMPARABLE: oxjvm_vm::NativeClass = class(
    "java/lang/Comparable",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &COMPARABLE_METHODS,
    None,
);

pub(crate) const RUNNABLE: oxjvm_vm::NativeClass = class(
    "java/lang/Runnable",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[method(
        "run",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method,
    )],
    None,
);

pub(crate) const CLONEABLE: oxjvm_vm::NativeClass = class(
    "java/lang/Cloneable",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const SERIALIZABLE: oxjvm_vm::NativeClass = class(
    "java/io/Serializable",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const ITERABLE: oxjvm_vm::NativeClass = class(
    "java/lang/Iterable",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[method(
        "iterator",
        "()Ljava/util/Iterator;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method,
    )],
    None,
);

pub(crate) const AUTO_CLOSEABLE: oxjvm_vm::NativeClass = class(
    "java/lang/AutoCloseable",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[method(
        "close",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method,
    )],
    None,
);

pub(crate) const COMPARATOR: oxjvm_vm::NativeClass = class(
    "java/util/Comparator",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[method(
        "compare",
        "(Ljava/lang/Object;Ljava/lang/Object;)I",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method,
    )],
    None,
);

// -------------------------------------------------------------------------------------------
// StackTraceElement
// -------------------------------------------------------------------------------------------

const STACK_TRACE_ELEMENT_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;I)V",
        ACC_PUBLIC,
        stack_trace_element_init,
    ),
    method(
        "getClassName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = vm.class_of(receiver(args));
            let name = vm
                .read_string_field(receiver(args), class, "declaringClass")
                .unwrap_or_default();
            object(vm.make_string(&name)?)
        },
    ),
    method(
        "getMethodName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = vm.class_of(receiver(args));
            let name = vm
                .read_string_field(receiver(args), class, "methodName")
                .unwrap_or_default();
            object(vm.make_string(&name)?)
        },
    ),
    method(
        "getFileName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            match vm.read_ref_field(this, class, "fileName", "Ljava/lang/String;") {
                Some(reference) => object(reference),
                None => object(ObjectRef::NULL),
            }
        },
    ),
    method("getLineNumber", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "lineNumber"))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        stack_trace_element_to_string,
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            if this == other {
                return boolean(true);
            }
            if other.is_null() || vm.class_of(this) != vm.class_of(other) {
                return boolean(false);
            }
            let class = vm.class_of(this);
            let a = vm.read_string_field(this, class, "declaringClass");
            let b = vm.read_string_field(other, class, "declaringClass");
            let c = vm.read_string_field(this, class, "methodName");
            let d = vm.read_string_field(other, class, "methodName");
            let e = vm.read_string_field(this, class, "fileName");
            let f = vm.read_string_field(other, class, "fileName");
            boolean(
                a == b
                    && c == d
                    && e == f
                    && read_int_field(vm, this, "lineNumber")
                        == read_int_field(vm, other, "lineNumber"),
            )
        },
    ),
];

fn stack_trace_element_init(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "declaringClass", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
    let (declaring, field) = vm.find_field(class, "methodName", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, ref_arg(args, 2));
    let file_name = ref_arg(args, 3);
    if !file_name.is_null() {
        let (declaring, field) = vm.find_field(class, "fileName", "Ljava/lang/String;")?;
        vm.set_instance_ref(this, declaring, field, file_name);
    }
    write_int_field(vm, this, "lineNumber", int_arg(args, 4));
    void()
}

fn stack_trace_element_to_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let frames = oxjvm_vm::BacktraceFrame {
        class_name: vm
            .read_string_field(this, vm.class_of(this), "declaringClass")
            .unwrap_or_default(),
        method_name: vm
            .read_string_field(this, vm.class_of(this), "methodName")
            .unwrap_or_default(),
        file_name: vm.read_string_field(this, vm.class_of(this), "fileName"),
        line_number: Some(read_int_field(vm, this, "lineNumber") as u16),
    };
    let location = match (&frames.file_name, frames.line_number) {
        (Some(file), Some(line)) if line > 0 => alloc::format!("({file}:{line})"),
        (Some(file), _) => alloc::format!("({file})"),
        _ => "(Unknown Source)".into(),
    };
    let text = alloc::format!("{}.{}{}", frames.class_name, frames.method_name, location);
    object(vm.make_string(&text)?)
}

pub(crate) const STACK_TRACE_ELEMENT: oxjvm_vm::NativeClass = class(
    "java/lang/StackTraceElement",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("declaringClass", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("methodName", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("fileName", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("lineNumber", "I", ACC_PRIVATE, None),
    ],
    &STACK_TRACE_ELEMENT_METHODS,
    None,
);
