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
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            if crate::annotation::is_annotation_instance(vm, this) {
                return boolean(crate::annotation::annotation_equals(vm, this, other)?);
            }
            boolean(same(vm, this, other))
        },
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
    let reference = receiver(args);
    if crate::annotation::is_annotation_instance(vm, reference) {
        return int(crate::annotation::annotation_hash_code(vm, reference)?);
    }
    let hash = vm.heap.get(reference).map_or(0, |object| object.hash);
    int(hash)
}

fn object_to_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let reference = receiver(args);
    let text = if crate::annotation::is_annotation_instance(vm, reference) {
        crate::annotation::annotation_to_string(vm, reference)?
    } else {
        describe_object(vm, reference)
    };
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

const CLASS_METHODS: [oxjvm_vm::NativeMethodDef; 64] = [
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
        "getCanonicalName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        class_get_canonical_name,
    ),
    method(
        "getTypeName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        class_get_type_name,
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
        "getGenericInterfaces",
        "()[Ljava/lang/reflect/Type;",
        ACC_PUBLIC,
        class_get_generic_interfaces,
    ),
    method(
        "descriptorString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        class_descriptor_string,
    ),
    method(
        "getComponentType",
        "()Ljava/lang/Class;",
        ACC_PUBLIC,
        class_get_component_type,
    ),
    method("isInterface", "()Z", ACC_PUBLIC, |vm, _, args| {
        let class = class_argument(vm, receiver(args))?;
        boolean(vm.classes.get(class).is_interface())
    }),
    method("isArray", "()Z", ACC_PUBLIC, |vm, _, args| {
        let class = class_argument(vm, receiver(args))?;
        boolean(vm.classes.get(class).kind == oxjvm_vm::ClassKind::Array)
    }),
    method("isPrimitive", "()Z", ACC_PUBLIC, |vm, _, args| {
        let class = class_argument(vm, receiver(args))?;
        boolean(vm.classes.get(class).kind == oxjvm_vm::ClassKind::Primitive)
    }),
    method("isEnum", "()Z", ACC_PUBLIC, |vm, _, args| {
        let class = class_argument(vm, receiver(args))?;
        boolean(vm.classes.get(class).is_enum)
    }),
    method(
        "getEnumConstants",
        "()[Ljava/lang/Object;",
        ACC_PUBLIC,
        class_get_enum_constants,
    ),
    method("isSynthetic", "()Z", ACC_PUBLIC, |vm, _, args| {
        let class = class_argument(vm, receiver(args))?;
        boolean(vm.classes.get(class).access_flags & ACC_SYNTHETIC != 0)
    }),
    method("isMemberClass", "()Z", ACC_PUBLIC, |vm, _, args| {
        let class = class_argument(vm, receiver(args))?;
        boolean(vm.classes.get(class).declaring_class_name.is_some())
    }),
    method(
        "getClasses",
        "()[Ljava/lang/Class;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = class_argument(vm, receiver(args))?;
            let mut members: Vec<ClassId> = Vec::new();
            let mut current = Some(class);
            while let Some(candidate) = current {
                let name = vm.classes.get(candidate).name.clone();
                for entry in vm.classes.all() {
                    if entry.declaring_class_name.as_deref() == Some(&name)
                        && entry.access_flags & ACC_PUBLIC != 0
                    {
                        members.push(entry.id);
                    }
                }
                current = vm.classes.get(candidate).super_class;
            }
            let class_class = vm.resolve_class("java/lang/Class")?;
            let array = vm.allocate_object_array(class_class, members.len())?;
            for (index, member) in members.iter().enumerate() {
                let class_object = vm.class_object(*member)?;
                vm.array_set_ref(array, index, class_object)?;
            }
            object(array)
        },
    ),
    method(
        "getDeclaredClasses",
        "()[Ljava/lang/Class;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = class_argument(vm, receiver(args))?;
            let name = vm.classes.get(class).name.clone();
            let members: Vec<ClassId> = vm
                .classes
                .all()
                .iter()
                .filter(|candidate| candidate.declaring_class_name.as_deref() == Some(&name))
                .map(|candidate| candidate.id)
                .collect();
            let class_class = vm.resolve_class("java/lang/Class")?;
            let array = vm.allocate_object_array(class_class, members.len())?;
            for (index, member) in members.iter().enumerate() {
                let class_object = vm.class_object(*member)?;
                vm.array_set_ref(array, index, class_object)?;
            }
            object(array)
        },
    ),
    method("isLocalClass", "()Z", ACC_PUBLIC, |vm, _, args| {
        let class = class_argument(vm, receiver(args))?;
        let definition = vm.classes.get(class);
        boolean(
            definition.declaring_class_name.is_none() && definition.enclosing_class_name.is_some(),
        )
    }),
    method("isAnonymousClass", "()Z", ACC_PUBLIC, |vm, _, args| {
        let class = class_argument(vm, receiver(args))?;
        // Distinguishing anonymous from local classes needs `InnerClasses.inner_name_index`,
        // which the loader does not retain; anonymous classes never contain declarations that
        // discovery selects, so reporting `false` is safe.
        boolean(vm.classes.get(class).name.contains("$$Lambda$"))
    }),
    method("isAnnotation", "()Z", ACC_PUBLIC, |vm, _, args| {
        let class = class_argument(vm, receiver(args))?;
        boolean(vm.classes.get(class).access_flags & ACC_ANNOTATION != 0)
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
        "forName",
        "(Ljava/lang/String;ZLjava/lang/ClassLoader;)Ljava/lang/Class;",
        ACC_PUBLIC | ACC_STATIC,
        class_for_name_with_loader,
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
        |vm, _, _| object(system_class_loader(vm)?),
    ),
    method(
        "getResourceAsStream",
        "(Ljava/lang/String;)Ljava/io/InputStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let resource = string_arg(vm, args[1])?;
            let resource = resource.strip_prefix('/').unwrap_or(&resource);
            resource_as_stream(vm, resource)
        },
    ),
    method(
        "getResource",
        "(Ljava/lang/String;)Ljava/net/URL;",
        ACC_PUBLIC,
        class_get_resource,
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = class_argument(vm, receiver(args))?;
            if vm.classes.get(class).kind == oxjvm_vm::ClassKind::Primitive {
                let name = vm.classes.get(class).name.clone();
                return object(vm.make_string(&name)?);
            }
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
    method(
        "getModule",
        "()Ljava/lang/Module;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/lang/Module")?;
            object(vm.new_instance(class)?)
        },
    ),
    method(
        "getDeclaredMethods",
        "()[Ljava/lang/reflect/Method;",
        ACC_PUBLIC,
        crate::reflect::class_reflect_methods,
    ),
    method(
        "getMethods",
        "()[Ljava/lang/reflect/Method;",
        ACC_PUBLIC,
        crate::reflect::class_reflect_all_methods,
    ),
    method(
        "getDeclaredFields",
        "()[Ljava/lang/reflect/Field;",
        ACC_PUBLIC,
        crate::reflect::class_reflect_fields,
    ),
    method(
        "getFields",
        "()[Ljava/lang/reflect/Field;",
        ACC_PUBLIC,
        crate::reflect::class_reflect_all_fields,
    ),
    method(
        "getDeclaredField",
        "(Ljava/lang/String;)Ljava/lang/reflect/Field;",
        ACC_PUBLIC,
        crate::reflect::class_reflect_field,
    ),
    method(
        "getField",
        "(Ljava/lang/String;)Ljava/lang/reflect/Field;",
        ACC_PUBLIC,
        crate::reflect::class_reflect_all_field,
    ),
    method(
        "getDeclaredMethod",
        "(Ljava/lang/String;[Ljava/lang/Class;)Ljava/lang/reflect/Method;",
        ACC_PUBLIC,
        crate::reflect::class_reflect_declared_method,
    ),
    method(
        "getMethod",
        "(Ljava/lang/String;[Ljava/lang/Class;)Ljava/lang/reflect/Method;",
        ACC_PUBLIC,
        crate::reflect::class_reflect_method,
    ),
    method(
        "getAnnotation",
        "(Ljava/lang/Class;)Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = class_argument(vm, receiver(args))?;
            let annotation = class_argument(vm, ref_arg(args, 1))?;
            object(crate::annotation::class_annotation(vm, class, annotation)?)
        },
    ),
    method(
        "getDeclaredAnnotation",
        "(Ljava/lang/Class;)Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = class_argument(vm, receiver(args))?;
            let annotation = class_argument(vm, ref_arg(args, 1))?;
            object(crate::annotation::class_declared_annotation(
                vm, class, annotation,
            )?)
        },
    ),
    method(
        "getAnnotations",
        "()[Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = class_argument(vm, receiver(args))?;
            let annotations = crate::annotation::class_annotations(vm, class)?;
            object(crate::annotation::annotation_array(vm, &annotations)?)
        },
    ),
    method(
        "getDeclaredAnnotations",
        "()[Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = class_argument(vm, receiver(args))?;
            let annotations = crate::annotation::class_declared_annotations(vm, class)?;
            object(crate::annotation::annotation_array(vm, &annotations)?)
        },
    ),
    method(
        "getAnnotationsByType",
        "(Ljava/lang/Class;)[Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = class_argument(vm, receiver(args))?;
            let annotation = class_argument(vm, ref_arg(args, 1))?;
            object(crate::annotation::class_annotations_by_type(
                vm, class, annotation,
            )?)
        },
    ),
    method(
        "getDeclaredAnnotationsByType",
        "(Ljava/lang/Class;)[Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = class_argument(vm, receiver(args))?;
            let annotation = class_argument(vm, ref_arg(args, 1))?;
            object(crate::annotation::class_declared_annotations_by_type(
                vm, class, annotation,
            )?)
        },
    ),
    method(
        "isAnnotationPresent",
        "(Ljava/lang/Class;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = class_argument(vm, receiver(args))?;
            let annotation = class_argument(vm, ref_arg(args, 1))?;
            let instance = crate::annotation::class_annotation(vm, class, annotation)?;
            boolean(!instance.is_null())
        },
    ),
    method(
        "getDeclaredConstructors",
        "()[Ljava/lang/reflect/Constructor;",
        ACC_PUBLIC,
        crate::reflect::class_reflect_constructors,
    ),
    method(
        "getConstructors",
        "()[Ljava/lang/reflect/Constructor;",
        ACC_PUBLIC,
        crate::reflect::class_reflect_public_constructors,
    ),
    method(
        "getDeclaredConstructor",
        "([Ljava/lang/Class;)Ljava/lang/reflect/Constructor;",
        ACC_PUBLIC,
        crate::reflect::class_reflect_constructor,
    ),
    method(
        "getConstructor",
        "([Ljava/lang/Class;)Ljava/lang/reflect/Constructor;",
        ACC_PUBLIC,
        crate::reflect::class_reflect_public_constructor,
    ),
    method(
        "getPackage",
        "()Ljava/lang/Package;",
        ACC_PUBLIC,
        class_get_package,
    ),
    method(
        "getEnclosingClass",
        "()Ljava/lang/Class;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = class_argument(vm, receiver(args))?;
            let name = vm.classes.get(class).enclosing_class_name.clone();
            match name.and_then(|name| vm.resolve_class_lenient(&name)) {
                Some(enclosing) => {
                    let class_object = vm.class_object(enclosing)?;
                    object(class_object)
                }
                None => object(ObjectRef::NULL),
            }
        },
    ),
    method(
        "getDeclaringClass",
        "()Ljava/lang/Class;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = class_argument(vm, receiver(args))?;
            let name = vm.classes.get(class).declaring_class_name.clone();
            match name.and_then(|name| vm.resolve_class_lenient(&name)) {
                Some(declaring) => {
                    let class_object = vm.class_object(declaring)?;
                    object(class_object)
                }
                None => object(ObjectRef::NULL),
            }
        },
    ),
    method(
        "describeConstable",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = class_argument(vm, receiver(args))?;
            let class_object = vm.class_object(class)?;
            crate::optional::optional_new(vm, class_object)
        },
    ),
    method(
        "getTypeParameters",
        "()[Ljava/lang/reflect/TypeVariable;",
        ACC_PUBLIC,
        |vm, _, _| {
            let type_variable = vm.resolve_class("java/lang/reflect/TypeVariable")?;
            let array = vm.allocate_object_array(type_variable, 0)?;
            object(array)
        },
    ),
    method(
        "getProtectionDomain",
        "()Ljava/security/ProtectionDomain;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/security/ProtectionDomain")?;
            object(vm.new_instance(class)?)
        },
    ),
    method(
        "getGenericSuperclass",
        "()Ljava/lang/reflect/Type;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = class_argument(vm, receiver(args))?;
            match vm.classes.get(class).super_class {
                Some(super_class) => {
                    let class_object = vm.class_object(super_class)?;
                    object(class_object)
                }
                None => object(ObjectRef::NULL),
            }
        },
    ),
    method(
        "getAnnotatedSuperclass",
        "()Ljava/lang/reflect/AnnotatedType;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = class_argument(vm, receiver(args))?;
            match vm.classes.get(class).super_class {
                Some(super_class) => object(crate::reflect::new_annotated_type(vm, super_class)?),
                None => object(ObjectRef::NULL),
            }
        },
    ),
    method(
        "getRecordComponents",
        "()[Ljava/lang/reflect/RecordComponent;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/lang/reflect/RecordComponent")?;
            object(vm.allocate_object_array(class, 0)?)
        },
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

fn class_get_canonical_name(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    match canonical_name(vm, class) {
        Some(text) => object(vm.make_string(&text)?),
        None => object(ObjectRef::NULL),
    }
}

/// `Class.getTypeName`: the canonical form for arrays and primitives, the binary name otherwise.
fn class_get_type_name(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    let name = vm.class_name(class);
    let text = if name.starts_with('[') {
        canonical_name(vm, class).unwrap_or_else(|| name.to_string())
    } else {
        name.replace('/', ".")
    };
    object(vm.make_string(&text)?)
}

/// The canonical name of a class, or `None` for a lambda/anonymous/local class.
pub(crate) fn canonical_name(vm: &Vm<'_>, class: ClassId) -> Option<String> {
    let name = vm.class_name(class);
    if vm.classes.get(class).kind == oxjvm_vm::ClassKind::Primitive {
        return Some(name.to_string());
    }
    if name.starts_with('[') {
        let mut dimensions = 0;
        let mut rest = name;
        while let Some(next) = rest.strip_prefix('[') {
            dimensions += 1;
            rest = next;
        }
        let mut component = if let Some(object) = rest.strip_prefix('L') {
            object.strip_suffix(';').unwrap_or(object).replace('/', ".")
        } else {
            primitive_name(rest).to_string()
        };
        for _ in 0..dimensions {
            component.push_str("[]");
        }
        return Some(component);
    }
    if is_anonymous(name) {
        return None;
    }
    Some(name.replace(['/', '$'], "."))
}

/// Whether a binary name belongs to a lambda, anonymous, or local class, whose canonical
/// name is null.
fn is_anonymous(name: &str) -> bool {
    match name.rsplit('$').next() {
        Some(segment) if segment.len() < name.len() => {
            segment.as_bytes().first().is_some_and(u8::is_ascii_digit)
        }
        _ => false,
    }
}

/// The Java spelling of a primitive descriptor character (`I` → `int`).
fn primitive_name(descriptor: &str) -> &'static str {
    match descriptor {
        "Z" => "boolean",
        "B" => "byte",
        "C" => "char",
        "S" => "short",
        "I" => "int",
        "J" => "long",
        "F" => "float",
        "D" => "double",
        _ => "void",
    }
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

fn class_descriptor_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    let definition = vm.classes.get(class);
    let text = if definition.kind == oxjvm_vm::ClassKind::Primitive {
        match definition.name.as_str() {
            "boolean" => "Z",
            "byte" => "B",
            "char" => "C",
            "short" => "S",
            "int" => "I",
            "long" => "J",
            "float" => "F",
            "double" => "D",
            "void" => "V",
            other => other,
        }
        .to_string()
    } else if definition.name.starts_with('[') {
        definition.name.clone()
    } else {
        alloc::format!("L{};", definition.name)
    };
    object(vm.make_string(&text)?)
}

fn class_get_generic_interfaces(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    let interfaces = vm.classes.get(class).interfaces.clone();
    let type_class = vm.resolve_class("java/lang/reflect/Type")?;
    let array = vm.allocate_object_array(type_class, interfaces.len())?;
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
    class_for_name_with(vm, args, true)
}

/// `Class.forName(String, boolean, ClassLoader)`. The loader argument is accepted but ignored:
/// the VM has a single loader.
fn class_for_name_with_loader(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    class_for_name_with(vm, args, bool_arg(args, 1))
}

fn class_for_name_with(
    vm: &mut Vm<'_>,
    args: &[Value],
    initialize: bool,
) -> Result<Value, VmError> {
    let name = string_arg(vm, args[0])?;
    let internal = name.replace('.', "/");
    match vm.resolve_class(&internal) {
        Ok(class) => {
            if initialize {
                vm.ensure_initialized(class)?;
            }
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

/// `Class.getEnumConstants()`: a fresh copy of the enum's `$VALUES` array, or null.
fn class_get_enum_constants(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    if !vm.classes.get(class).is_enum {
        return object(ObjectRef::NULL);
    }
    vm.ensure_initialized(class)?;
    let descriptor = vm
        .classes
        .get(class)
        .fields
        .iter()
        .find(|field| field.name == "$VALUES")
        .map(|field| field.descriptor.clone());
    let Some(descriptor) = descriptor else {
        return object(ObjectRef::NULL);
    };
    let Some(values) = vm.read_static_ref_if_set(class, "$VALUES", &descriptor) else {
        return object(ObjectRef::NULL);
    };
    let clone = vm.clone_object(values)?;
    object(clone)
}

/// The internal package name of a class, following arrays to their component.
fn package_name_of(vm: &Vm<'_>, class: ClassId) -> String {
    let mut class = class;
    while vm.classes.get(class).kind == oxjvm_vm::ClassKind::Array {
        match vm.classes.get(class).component_class {
            Some(component) => class = component,
            None => break,
        }
    }
    let entry = vm.classes.get(class);
    if entry.kind == oxjvm_vm::ClassKind::Primitive {
        return String::new();
    }
    match entry.name.rfind('/') {
        Some(index) => entry.name[..index].to_string(),
        None => String::new(),
    }
}

fn class_get_package_name(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    let text = package_name_of(vm, class).replace('/', ".");
    object(vm.make_string(&text)?)
}

fn class_get_package(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = class_argument(vm, receiver(args))?;
    let name = package_name_of(vm, class);
    if name.is_empty() {
        return object(ObjectRef::NULL);
    }
    let package_class = vm.resolve_class("java/lang/Package")?;
    let instance = vm.new_instance(package_class)?;
    let text = name.replace('/', ".");
    let string = vm.make_string(&text)?;
    let (declaring, field) = vm.find_field(package_class, "packageName", "Ljava/lang/String;")?;
    vm.set_instance_ref(instance, declaring, field, string);
    object(instance)
}

/// The `package-info` class backing a `Package` object, when present.
fn package_info(vm: &mut Vm<'_>, package: ObjectRef) -> Option<ClassId> {
    let class = vm.class_of(package);
    let name = vm.read_ref_field(package, class, "packageName", "Ljava/lang/String;")?;
    let text = vm.string_value(name)?;
    let internal = if text.is_empty() {
        String::from("package-info")
    } else {
        alloc::format!("{}/package-info", text.replace('.', "/"))
    };
    vm.resolve_class_lenient(&internal)
}

fn package_get_name(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let package = receiver(args);
    let class = vm.class_of(package);
    let name = vm
        .read_ref_field(package, class, "packageName", "Ljava/lang/String;")
        .unwrap_or(ObjectRef::NULL);
    object(name)
}

fn package_declared_annotations(
    vm: &mut Vm<'_>,
    package: ObjectRef,
) -> Result<Vec<ObjectRef>, VmError> {
    match package_info(vm, package) {
        Some(info) => crate::annotation::class_declared_annotations(vm, info),
        None => Ok(Vec::new()),
    }
}

fn package_annotation(
    vm: &mut Vm<'_>,
    package: ObjectRef,
    annotation: ClassId,
) -> Result<ObjectRef, VmError> {
    match package_info(vm, package) {
        Some(info) => crate::annotation::class_declared_annotation(vm, info, annotation),
        None => Ok(ObjectRef::NULL),
    }
}

const PACKAGE_METHODS: [oxjvm_vm::NativeMethodDef; 20] = [
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        package_get_name,
    ),
    // Package metadata beyond the name is not carried by the class path, so the version and
    // vendor getters answer with `null`, as the JDK does for packages without a manifest.
    method(
        "getSpecificationTitle",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getSpecificationVersion",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getSpecificationVendor",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getImplementationTitle",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getImplementationVersion",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getImplementationVendor",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method("isSealed", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method(
        "isSealed",
        "(Ljava/net/URL;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            boolean(false)
        },
    ),
    method(
        "isCompatibleWith",
        "(Ljava/lang/String;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let desired = string_arg(vm, args[1])?;
            if desired.is_empty() {
                return Err(vm.throw_new("java/lang/NumberFormatException", None));
            }
            // Without a specification version no package can be compatible.
            boolean(false)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let package = receiver(args);
        let class = vm.class_of(package);
        let name = vm
            .read_ref_field(package, class, "packageName", "Ljava/lang/String;")
            .unwrap_or(ObjectRef::NULL);
        if name.is_null() {
            return int(0);
        }
        let (class, method) = vm.resolve_virtual_method(name, "hashCode", "()I")?;
        int(vm
            .invoke_method(class, method, alloc::vec![Value::Ref(name)])?
            .as_int())
    }),
    method(
        "getAnnotation",
        "(Ljava/lang/Class;)Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let annotation = class_argument(vm, ref_arg(args, 1))?;
            object(package_annotation(vm, receiver(args), annotation)?)
        },
    ),
    method(
        "getDeclaredAnnotation",
        "(Ljava/lang/Class;)Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let annotation = class_argument(vm, ref_arg(args, 1))?;
            object(package_annotation(vm, receiver(args), annotation)?)
        },
    ),
    method(
        "getAnnotations",
        "()[Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let annotations = package_declared_annotations(vm, receiver(args))?;
            object(crate::annotation::annotation_array(vm, &annotations)?)
        },
    ),
    method(
        "getDeclaredAnnotations",
        "()[Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let annotations = package_declared_annotations(vm, receiver(args))?;
            object(crate::annotation::annotation_array(vm, &annotations)?)
        },
    ),
    method(
        "getAnnotationsByType",
        "(Ljava/lang/Class;)[Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let annotation = class_argument(vm, ref_arg(args, 1))?;
            match package_info(vm, receiver(args)) {
                Some(info) => object(crate::annotation::class_declared_annotations_by_type(
                    vm, info, annotation,
                )?),
                None => object(crate::annotation::typed_annotation_array(
                    vm,
                    annotation,
                    &[],
                )?),
            }
        },
    ),
    method(
        "getDeclaredAnnotationsByType",
        "(Ljava/lang/Class;)[Ljava/lang/annotation/Annotation;",
        ACC_PUBLIC,
        |vm, _, args| {
            let annotation = class_argument(vm, ref_arg(args, 1))?;
            match package_info(vm, receiver(args)) {
                Some(info) => object(crate::annotation::class_declared_annotations_by_type(
                    vm, info, annotation,
                )?),
                None => object(crate::annotation::typed_annotation_array(
                    vm,
                    annotation,
                    &[],
                )?),
            }
        },
    ),
    method(
        "isAnnotationPresent",
        "(Ljava/lang/Class;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let annotation = class_argument(vm, ref_arg(args, 1))?;
            let instance = package_annotation(vm, receiver(args), annotation)?;
            boolean(!instance.is_null())
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let package = receiver(args);
            let class = vm.class_of(package);
            let name = vm
                .read_ref_field(package, class, "packageName", "Ljava/lang/String;")
                .unwrap_or(ObjectRef::NULL);
            let text = vm.string_or_empty(name);
            let text = alloc::format!("package {text}");
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "getPackage",
        "(Ljava/lang/String;)Ljava/lang/Package;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = string_arg(vm, args[0])?;
            if name.is_empty() {
                return object(ObjectRef::NULL);
            }
            let package_class = vm.resolve_class("java/lang/Package")?;
            let instance = vm.new_instance(package_class)?;
            let string = vm.make_string(&name)?;
            let (declaring, field) =
                vm.find_field(package_class, "packageName", "Ljava/lang/String;")?;
            vm.set_instance_ref(instance, declaring, field, string);
            object(instance)
        },
    ),
];

/// `java.lang.Package`: only names and package annotations are modelled.
pub(crate) const PACKAGE: oxjvm_vm::NativeClass = class(
    "java/lang/Package",
    Some("java/lang/Object"),
    &["java/lang/reflect/AnnotatedElement"],
    ACC_PUBLIC,
    &[field(
        "packageName",
        "Ljava/lang/String;",
        ACC_PRIVATE | ACC_FINAL,
        None,
    )],
    &PACKAGE_METHODS,
    None,
);

pub(crate) const CLASS: oxjvm_vm::NativeClass = class(
    "java/lang/Class",
    Some("java/lang/Object"),
    &[
        "java/io/Serializable",
        "java/lang/reflect/AnnotatedElement",
        "java/lang/reflect/Type",
    ],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &CLASS_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// String
// -------------------------------------------------------------------------------------------

const STRING_METHODS: [oxjvm_vm::NativeMethodDef; 55] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        string_init_string,
    ),
    method(
        "<init>",
        "(Ljava/lang/StringBuffer;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let buffer = ref_arg(args, 1);
            let (class, method) =
                vm.resolve_virtual_method(buffer, "toString", "()Ljava/lang/String;")?;
            let text = vm
                .invoke_method(class, method, vec![Value::Ref(buffer)])?
                .as_ref();
            let units = vm.string_utf16(text).unwrap_or_default();
            let this = receiver(args);
            write_string_payload(vm, this, units.to_vec())?;
            void()
        },
    ),
    method("<init>", "([C)V", ACC_PUBLIC, string_init_chars),
    method("<init>", "([CII)V", ACC_PUBLIC, string_init_chars_range),
    method("<init>", "([III)V", ACC_PUBLIC, string_init_code_points),
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
    method(
        "lines",
        "()Ljava/util/stream/Stream;",
        ACC_PUBLIC,
        string_lines,
    ),
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
        "compareTo",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC | ACC_BRIDGE | ACC_SYNTHETIC,
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
    method("lastIndexOf", "(II)I", ACC_PUBLIC, |vm, _, args| {
        let units = vm
            .string_utf16(receiver(args))
            .map(<[u16]>::to_vec)
            .unwrap_or_default();
        let ch = int_arg(args, 1) as u16;
        let from = (int_arg(args, 2) as isize).min(units.len() as isize);
        if from < 0 {
            return int(-1);
        }
        let found = units[..=from as usize]
            .iter()
            .rposition(|unit| *unit == ch)
            .map_or(-1, |index| index as i32);
        int(found)
    }),
    method(
        "lastIndexOf",
        "(Ljava/lang/String;)I",
        ACC_PUBLIC,
        |vm, _, args| string_last_index_of_string(vm, args, i32::MAX),
    ),
    method(
        "lastIndexOf",
        "(Ljava/lang/String;I)I",
        ACC_PUBLIC,
        |vm, _, args| string_last_index_of_string(vm, args, int_arg(args, 2)),
    ),
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
    method(
        "toUpperCase",
        "(Ljava/util/Locale;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm.string_or_empty(receiver(args));
            let language = locale_language(vm, ref_arg(args, 1))?;
            object(vm.make_string(&localized_case(&text, &language, true))?)
        },
    ),
    method(
        "toLowerCase",
        "(Ljava/util/Locale;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm.string_or_empty(receiver(args));
            let language = locale_language(vm, ref_arg(args, 1))?;
            object(vm.make_string(&localized_case(&text, &language, false))?)
        },
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
const STRING_METHODS_EXTRA: [oxjvm_vm::NativeMethodDef; 20] = [
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
        "join",
        "(Ljava/lang/CharSequence;Ljava/lang/Iterable;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        string_join_iterable,
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
    method(
        "replaceAll",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC,
        string_replace_all,
    ),
    method(
        "replaceFirst",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC,
        string_replace_first,
    ),
    method("<init>", "([BII)V", ACC_PUBLIC, string_init_bytes_range),
    method(
        "<init>",
        "([BLjava/nio/charset/Charset;)V",
        ACC_PUBLIC,
        string_init_bytes_charset_object,
    ),
    method(
        "<init>",
        "([BIILjava/nio/charset/Charset;)V",
        ACC_PUBLIC,
        string_init_bytes_range_charset_object,
    ),
    method(
        "getBytes",
        "(Ljava/lang/String;)[B",
        ACC_PUBLIC,
        string_get_bytes_charset,
    ),
    method(
        "getBytes",
        "(Ljava/nio/charset/Charset;)[B",
        ACC_PUBLIC,
        string_get_bytes_charset_object,
    ),
    method(
        "formatted",
        "([Ljava/lang/Object;)Ljava/lang/String;",
        ACC_PUBLIC,
        string_format,
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
    write_string_payload(vm, this, units.to_vec())?;
    void()
}

/// Replace a `String` object's payload in place (the only mutation of an immutable Java string,
/// used by the constructors before publication).
pub(crate) fn write_string_payload(
    vm: &mut Vm<'_>,
    string: ObjectRef,
    units: Vec<u16>,
) -> Result<(), VmError> {
    let Some(data) = vm.heap.get(string).map(|object| &object.data) else {
        return Err(VmError::internal("write_string_payload on a non-string"));
    };
    match data {
        oxjvm_vm::ObjectData::String(_) => {
            vm.replace_payload(string, oxjvm_vm::ObjectData::String(units))?;
            Ok(())
        }
        // `new String(...)` allocates a plain instance; the constructor converts it.
        oxjvm_vm::ObjectData::Instance(_) => {
            vm.replace_payload(string, oxjvm_vm::ObjectData::String(units))?;
            Ok(())
        }
        _ => Err(VmError::internal("write_string_payload on a non-string")),
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

fn string_init_code_points(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let reference = ref_arg(args, 1);
    if reference.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let codepoints: Vec<i32> = match vm.heap.get(reference).map(|object| &object.data) {
        Some(oxjvm_vm::ObjectData::Array(oxjvm_vm::ArrayData::Int(values))) => values.clone(),
        _ => return Err(vm.throw_new("java/lang/NullPointerException", None)),
    };
    let offset = int_arg(args, 2);
    let count = int_arg(args, 3);
    string_range_check(vm, offset, count, codepoints.len())?;
    let mut units: Vec<u16> = Vec::new();
    for &cp in &codepoints[offset as usize..(offset + count) as usize] {
        let Some(ch) = char::from_u32(cp as u32) else {
            return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
        };
        let mut buf = [0u16; 2];
        let encoded = ch.encode_utf16(&mut buf);
        units.extend_from_slice(encoded);
    }
    write_string_payload(vm, receiver(args), units)?;
    void()
}

/// `String(byte[], int, int)`: decode the slice as UTF-8.
fn string_init_bytes_range(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let bytes = array_bytes(vm, ref_arg(args, 1))?;
    let offset = int_arg(args, 2);
    let count = int_arg(args, 3);
    string_range_check(vm, offset, count, bytes.len())?;
    let slice = &bytes[offset as usize..(offset + count) as usize];
    let text = String::from_utf8_lossy(slice).into_owned();
    write_string_payload(vm, receiver(args), text.encode_utf16().collect())?;
    void()
}

/// `String.getBytes(String)`: encode as UTF-8 (the charset name is ignored).
/// `String.getBytes(String)`: encode with the named charset (UTF-8 when unknown).
fn string_get_bytes_charset(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let name = string_arg(vm, args[1])?;
    let text = vm.string_or_empty(receiver(args));
    let bytes = match crate::charset::canonical_name(&name) {
        Some(canonical) => crate::charset::encode(&text, canonical),
        None => text.into_bytes(),
    };
    object(byte_array_from(vm, &bytes)?)
}

/// `String.getBytes(Charset)`.
fn string_get_bytes_charset_object(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let name = crate::charset::name_of(vm, ref_arg(args, 1));
    let text = vm.string_or_empty(receiver(args));
    let bytes = match crate::charset::canonical_name(&name) {
        Some(canonical) => crate::charset::encode(&text, canonical),
        None => text.into_bytes(),
    };
    object(byte_array_from(vm, &bytes)?)
}

/// Allocate a guest `byte[]` with `bytes` as its contents.
fn byte_array_from(vm: &mut Vm<'_>, bytes: &[u8]) -> Result<ObjectRef, VmError> {
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, bytes.len())?;
    for (index, byte) in bytes.iter().enumerate() {
        vm.array_set(
            array,
            index as i32,
            Value::Int(i32::from(*byte as i8)),
            false,
        )?;
    }
    Ok(array)
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

fn string_lines(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = vm.string_or_empty(receiver(args));
    let chars: Vec<char> = text.chars().collect();
    let mut lines: Vec<ObjectRef> = Vec::new();
    let mut current = String::new();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            lines.push(vm.make_string(&current)?);
            current.clear();
            i += 1;
        } else if c == '\r' {
            lines.push(vm.make_string(&current)?);
            current.clear();
            if i + 1 < chars.len() && chars[i + 1] == '\n' {
                i += 2;
            } else {
                i += 1;
            }
        } else {
            current.push(c);
            i += 1;
        }
    }
    if !current.is_empty() {
        lines.push(vm.make_string(&current)?);
    }
    object(crate::stream::stream_from(vm, lines)?)
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

/// `String(byte[], String)`: decode with the named charset (UTF-8 when unknown).
fn string_init_bytes_charset(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let bytes = byte_array(vm, ref_arg(args, 1))?;
    let name = string_arg(vm, args[2])?;
    let units = match crate::charset::canonical_name(&name) {
        Some(canonical) => crate::charset::decode(&bytes, canonical),
        None => String::from_utf8_lossy(&bytes).encode_utf16().collect(),
    };
    write_string_payload(vm, receiver(args), units)?;
    void()
}

/// `String(byte[], Charset)`.
fn string_init_bytes_charset_object(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let bytes = byte_array(vm, ref_arg(args, 1))?;
    let name = crate::charset::name_of(vm, ref_arg(args, 2));
    let units = match crate::charset::canonical_name(&name) {
        Some(canonical) => crate::charset::decode(&bytes, canonical),
        None => String::from_utf8_lossy(&bytes).encode_utf16().collect(),
    };
    write_string_payload(vm, receiver(args), units)?;
    void()
}

/// `String(byte[], int, int, Charset)`.
fn string_init_bytes_range_charset_object(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let bytes = byte_array(vm, ref_arg(args, 1))?;
    let offset = int_arg(args, 2);
    let count = int_arg(args, 3);
    string_range_check(vm, offset, count, bytes.len())?;
    let slice = &bytes[offset as usize..(offset + count) as usize];
    let name = crate::charset::name_of(vm, ref_arg(args, 4));
    let units = match crate::charset::canonical_name(&name) {
        Some(canonical) => crate::charset::decode(slice, canonical),
        None => String::from_utf8_lossy(slice).encode_utf16().collect(),
    };
    write_string_payload(vm, receiver(args), units)?;
    void()
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
    let other = ref_arg(args, 1);
    if other.is_null() {
        return boolean(false);
    }
    let this = vm.string_or_empty(receiver(args));
    let other = vm.string_or_empty(other);
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
    let interned = vm.intern(&text)?;
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

/// The last occurrence of `needle` at or before `from` (JDK's `lastIndexOf(String, int)`).
fn string_last_index_of_string(
    vm: &mut Vm<'_>,
    args: &[Value],
    from: i32,
) -> Result<Value, VmError> {
    let haystack = vm
        .string_utf16(receiver(args))
        .map(<[u16]>::to_vec)
        .unwrap_or_default();
    let needle = vm
        .string_utf16(ref_arg(args, 1))
        .map(<[u16]>::to_vec)
        .ok_or_else(|| vm.throw_new("java/lang/NullPointerException", None))?;
    if needle.is_empty() {
        let index = (from as isize).clamp(0, haystack.len() as isize);
        return int(index as i32);
    }
    let limit = (from as isize).min(haystack.len() as isize - needle.len() as isize);
    if limit < 0 {
        return int(-1);
    }
    let last = limit as usize;
    for start in (0..=last).rev() {
        if haystack[start..].starts_with(&needle) {
            return int(start as i32);
        }
    }
    int(-1)
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

/// Locale-sensitive case conversion.  Turkish and Azeri are the only locales in
/// the test corpus whose dotted/dotless I rules differ from the default mapping.
fn localized_case(text: &str, language: &str, upper: bool) -> String {
    if language != "tr" && language != "az" {
        return if upper {
            text.to_uppercase()
        } else {
            text.to_lowercase()
        };
    }
    let mut out = String::new();
    for c in text.chars() {
        if upper {
            match c {
                'i' => out.push('İ'),
                'ı' => out.push('I'),
                _ => out.extend(c.to_uppercase()),
            }
        } else {
            match c {
                'I' => out.push('ı'),
                'İ' => out.push('i'),
                _ => out.extend(c.to_lowercase()),
            }
        }
    }
    out
}

fn locale_language(vm: &mut Vm<'_>, locale: ObjectRef) -> Result<String, VmError> {
    if locale.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    Ok(vm
        .read_string_field(locale, vm.class_of(locale), "language")
        .unwrap_or_default())
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

/// `String.join(CharSequence, Iterable<? extends CharSequence>)`.
fn string_join_iterable(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let delimiter = vm.string_or_empty(ref_arg(args, 0));
    let iterable = ref_arg(args, 1);
    let mut out = String::new();
    let (class, method) =
        vm.resolve_virtual_method(iterable, "iterator", "()Ljava/util/Iterator;")?;
    let iterator = vm
        .invoke_method(class, method, vec![Value::Ref(iterable)])?
        .as_ref();
    let mut first = true;
    loop {
        let (class, method) = vm.resolve_virtual_method(iterator, "hasNext", "()Z")?;
        if vm
            .invoke_method(class, method, vec![Value::Ref(iterator)])?
            .as_int()
            == 0
        {
            break;
        }
        let (class, method) =
            vm.resolve_virtual_method(iterator, "next", "()Ljava/lang/Object;")?;
        let element = vm
            .invoke_method(class, method, vec![Value::Ref(iterator)])?
            .as_ref();
        if !first {
            out.push_str(&delimiter);
        }
        first = false;
        if element.is_null() {
            out.push_str("null");
        } else {
            let (class, method) =
                vm.resolve_virtual_method(element, "toString", "()Ljava/lang/String;")?;
            let text = vm.invoke_method(class, method, vec![Value::Ref(element)])?;
            out.push_str(&vm.string_or_empty(text.as_ref()));
        }
    }
    object(vm.make_string(&out)?)
}

fn string_split(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    string_split_impl(vm, args, 0)
}

fn string_split_limit(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    string_split_impl(vm, args, int_arg(args, 2))
}

fn string_split_impl(vm: &mut Vm<'_>, args: &[Value], limit: i32) -> Result<Value, VmError> {
    let text = vm.string_or_empty(receiver(args));
    let separator = string_arg(vm, args[1])?;
    let parts: Vec<String> = if separator.is_empty() {
        text.chars().map(|ch| ch.to_string()).collect()
    } else {
        let units: Vec<u16> = text.encode_utf16().collect();
        let regex = crate::regex::compile_from_str(vm, &separator)?;
        crate::regex::split_spans(&regex, &units, limit)
            .iter()
            .map(|(start, end)| String::from_utf16_lossy(&units[*start..*end]))
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
    let units: Vec<u16> = text.encode_utf16().collect();
    let regex = crate::regex::compile_from_str(vm, &pattern)?;
    boolean(crate::regex::match_at(&regex, &units, true).is_some())
}

fn string_replace_all(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    string_replace_impl(vm, args, true)
}

fn string_replace_first(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    string_replace_impl(vm, args, false)
}

fn string_replace_impl(vm: &mut Vm<'_>, args: &[Value], all: bool) -> Result<Value, VmError> {
    let text = vm.string_or_empty(receiver(args));
    let pattern = string_arg(vm, args[1])?;
    let replacement = string_arg(vm, args[2])?;
    let units: Vec<u16> = text.encode_utf16().collect();
    let replacement: Vec<u16> = replacement.encode_utf16().collect();
    let regex = crate::regex::compile_from_str(vm, &pattern)?;
    let result = if all {
        crate::regex::replace_all(&regex, &units, &replacement)
    } else {
        crate::regex::replace_first(&regex, &units, &replacement)
    };
    object(vm.make_string_utf16(result)?)
}

/// A deliberately small regex engine for `String.matches`: literals and `.*`/`.`/`^`/`$`.
/// The bytes of a `byte[]`.
pub(crate) fn array_bytes(vm: &mut Vm<'_>, array: ObjectRef) -> Result<Vec<u8>, VmError> {
    if array.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let length = vm.array_length(array)?;
    let mut bytes = Vec::with_capacity(length);
    for index in 0..length {
        bytes.push(vm.array_get(array, index as i32)?.as_int() as u8);
    }
    Ok(bytes)
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
        &[field(
            "CASE_INSENSITIVE_ORDER",
            "Ljava/util/Comparator;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        )],
        &STRING_ALL_METHODS,
        Some(string_clinit),
    )
};

/// Both `String` method arrays flattened; built by a const fn to stay `'static`.
const STRING_ALL_METHODS: [oxjvm_vm::NativeMethodDef;
    STRING_METHODS.len() + STRING_METHODS_EXTRA.len()] = flatten_string_methods();

const fn flatten_string_methods()
-> [oxjvm_vm::NativeMethodDef; STRING_METHODS.len() + STRING_METHODS_EXTRA.len()] {
    let mut out = [method("<init>", "()V", ACC_PUBLIC, |_, _, _| void());
        STRING_METHODS.len() + STRING_METHODS_EXTRA.len()];
    let mut index = 0;
    while index < STRING_METHODS.len() {
        out[index] = STRING_METHODS[index];
        index += 1;
    }
    let mut extra = 0;
    while extra < STRING_METHODS_EXTRA.len() {
        out[STRING_METHODS.len() + extra] = STRING_METHODS_EXTRA[extra];
        extra += 1;
    }
    out
}

fn string_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let string = vm.resolve_class("java/lang/String")?;
    let comparator_class = vm.resolve_class("java/lang/OjvmCaseInsensitiveComparator")?;
    let comparator = vm.new_instance(comparator_class)?;
    vm.set_static_value(
        string,
        "CASE_INSENSITIVE_ORDER",
        "Ljava/util/Comparator;",
        Value::Ref(comparator),
    );
    Ok(())
}

const CASE_INSENSITIVE_COMPARATOR_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "compare",
    "(Ljava/lang/Object;Ljava/lang/Object;)I",
    ACC_PUBLIC,
    |vm, _, args| {
        let first = ref_arg(args, 1);
        let second = ref_arg(args, 2);
        if first.is_null() {
            return int(-1);
        }
        if second.is_null() {
            return int(1);
        }
        let (class, method) =
            vm.resolve_virtual_method(first, "compareToIgnoreCase", "(Ljava/lang/String;)I")?;
        vm.invoke_method(class, method, vec![Value::Ref(first), Value::Ref(second)])
    },
)];

/// `String.CASE_INSENSITIVE_ORDER`.
pub(crate) const CASE_INSENSITIVE_COMPARATOR: oxjvm_vm::NativeClass = class(
    "java/lang/OjvmCaseInsensitiveComparator",
    Some("java/lang/Object"),
    &["java/util/Comparator"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[],
    &CASE_INSENSITIVE_COMPARATOR_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// StringBuilder / StringBuffer
// -------------------------------------------------------------------------------------------

const BUILDER_METHODS: [oxjvm_vm::NativeMethodDef; 35] = [
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
        "(Ljava/lang/CharSequence;II)Ljava/lang/StringBuilder;",
        ACC_PUBLIC,
        builder_append_sequence_range,
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
    method(
        "append",
        "(C)Ljava/lang/StringBuilder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = char::from_u32(int_arg(args, 1) as u32 & 0xFFFF)
                .map_or(String::new(), String::from);
            builder_append_text(vm, receiver(args), &text)
        },
    ),
];

const BUFFER_METHODS: [oxjvm_vm::NativeMethodDef; 29] = [
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
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        builder_to_string,
    ),
    method("length", "()I", ACC_PUBLIC, builder_length),
    method("charAt", "(I)C", ACC_PUBLIC, builder_char_at),
    method("setLength", "(I)V", ACC_PUBLIC, builder_set_length),
    method("setCharAt", "(IC)V", ACC_PUBLIC, builder_set_char_at),
    method(
        "append",
        "(Ljava/lang/Object;)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        builder_append_object,
    ),
    method(
        "append",
        "(Ljava/lang/String;)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        builder_append_string,
    ),
    method(
        "append",
        "(Ljava/lang/CharSequence;)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        builder_append_sequence,
    ),
    method(
        "append",
        "([C)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        builder_append_chars,
    ),
    method(
        "append",
        "([CII)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        builder_append_chars_range,
    ),
    method(
        "append",
        "(Ljava/lang/CharSequence;II)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        builder_append_sequence_range,
    ),
    method(
        "append",
        "(Z)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        builder_append_bool,
    ),
    method(
        "append",
        "(C)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        builder_append_char,
    ),
    method(
        "append",
        "(I)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        builder_append_int,
    ),
    method(
        "append",
        "(J)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        builder_append_long,
    ),
    method(
        "append",
        "(F)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        builder_append_float,
    ),
    method(
        "append",
        "(D)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        builder_append_double,
    ),
    method(
        "append",
        "(Ljava/lang/StringBuffer;)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        builder_append_object,
    ),
    method(
        "indexOf",
        "(Ljava/lang/String;)I",
        ACC_PUBLIC,
        builder_index_of,
    ),
    method(
        "substring",
        "(II)Ljava/lang/String;",
        ACC_PUBLIC,
        builder_substring,
    ),
    method(
        "delete",
        "(II)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        builder_delete,
    ),
    method(
        "deleteCharAt",
        "(I)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        builder_delete_char_at,
    ),
    method(
        "insert",
        "(ILjava/lang/String;)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        builder_insert_string,
    ),
    method(
        "replace",
        "(IILjava/lang/String;)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        builder_replace,
    ),
    method(
        "reverse",
        "()Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        builder_reverse,
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
    if let Some((declaring, field)) = vm.find_field_opt(class, name, "I") {
        vm.set_instance_int(object, declaring, field, value);
    }
}

pub(crate) fn read_long_field(vm: &Vm<'_>, object: ObjectRef, name: &str) -> i64 {
    let class = vm.class_of(object);
    vm.read_instance_long_named(object, class, name, "J")
        .unwrap_or(0)
}

pub(crate) fn write_long_field(vm: &mut Vm<'_>, object: ObjectRef, name: &str, value: i64) {
    let class = vm.class_of(object);
    if let Some((declaring, field)) = vm.find_field_opt(class, name, "J") {
        vm.set_instance_long(object, declaring, field, value);
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
    let units = char_sequence_units(vm, reference)?;
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

fn builder_append_sequence_range(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let reference = ref_arg(args, 1);
    let start = int_arg(args, 2);
    let end = int_arg(args, 3);
    let units = if reference.is_null() {
        vec![b'n' as u16, b'u' as u16, b'l' as u16, b'l' as u16]
    } else {
        char_sequence_units(vm, reference)?
    };
    if start < 0 || end > units.len() as i32 || start > end {
        return Err(vm.throw_new(
            "java/lang/IndexOutOfBoundsException",
            Some(&alloc::format!(
                "start {start}, end {end}, length {}",
                units.len()
            )),
        ));
    }
    let slice = &units[start as usize..end as usize];
    let (mut current, count) = builder_state(vm, receiver(args));
    current.truncate(count);
    current.extend_from_slice(slice);
    builder_store(vm, receiver(args), &current, count + slice.len())?;
    object(receiver(args))
}

pub(crate) fn builder_append_text(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    text: &str,
) -> Result<Value, VmError> {
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

const SYSTEM_METHODS: [oxjvm_vm::NativeMethodDef; 26] = [
    method(
        "currentTimeMillis",
        "()J",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| long(vm.host_time_millis()),
    ),
    // The standard streams are never connected to a terminal here, so `console()` reports none.
    method(
        "console",
        "()Ljava/io/Console;",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| object(ObjectRef::NULL),
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
        |vm, _, args| system_set_property(vm, args),
    ),
    method(
        "clearProperty",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let key = string_arg(vm, args[0])?;
            match vm.clear_property(&key) {
                Some(text) => object(vm.make_string(&text)?),
                None => object(ObjectRef::NULL),
            }
        },
    ),
    method(
        "getProperties",
        "()Ljava/util/Properties;",
        ACC_PUBLIC | ACC_STATIC,
        system_get_properties,
    ),
    method(
        "setProperties",
        "(Ljava/util/Properties;)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            void()
        },
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
        |vm, _, args| {
            let system = system_class(vm)?;
            vm.set_static_value(system, "in", "Ljava/io/InputStream;", args[0]);
            void()
        },
    ),
    method(
        "loadLibrary",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| void(),
    ),
    method(
        "load",
        "(Ljava/lang/String;)V",
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
    method(
        "getenv",
        "()Ljava/util/Map;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let map_class = vm.resolve_class("java/util/HashMap")?;
            let map = vm.new_instance(map_class)?;
            let (declaring, ctor) = vm
                .find_method(map_class, "<init>", "()V")
                .ok_or_else(|| VmError::internal("HashMap() missing"))?;
            vm.invoke_method(declaring, ctor, vec![Value::Ref(map)])?;
            let (put_class, put) = vm
                .find_method(
                    map_class,
                    "put",
                    "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
                )
                .ok_or_else(|| VmError::internal("HashMap.put missing"))?;
            for (key, value) in vm.host.environment() {
                let key_ref = vm.make_string(&key)?;
                let value_ref = vm.make_string(&value)?;
                vm.invoke_method(
                    put_class,
                    put,
                    vec![Value::Ref(map), Value::Ref(key_ref), Value::Ref(value_ref)],
                )?;
            }
            object(map)
        },
    ),
    method(
        "getenv",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = string_arg(vm, args[0])?;
            for (key, value) in vm.host.environment() {
                if key == name {
                    return object(vm.make_string(&value)?);
                }
            }
            object(ObjectRef::NULL)
        },
    ),
    method(
        "getLogger",
        "(Ljava/lang/String;)Ljava/lang/System$Logger;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let class = vm.resolve_class("java/lang/System$OjvmLogger")?;
            let logger = vm.new_instance(class)?;
            let name = string_arg(vm, args[0])?;
            vm.set_string_field(logger, class, "name", &name)?;
            object(logger)
        },
    ),
    method(
        "mapLibraryName",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = string_arg(vm, args[0])?;
            let mapped = if cfg!(target_os = "windows") {
                alloc::format!("{name}.dll")
            } else if cfg!(target_os = "macos") {
                alloc::format!("lib{name}.dylib")
            } else {
                alloc::format!("lib{name}.so")
            };
            object(vm.make_string(&mapped)?)
        },
    ),
];

fn system_class(vm: &mut Vm<'_>) -> Result<ClassId, VmError> {
    vm.resolve_class("java/lang/System")
}

/// The VM's well-known default system properties.
pub(crate) const SYSTEM_PROPERTY_DEFAULTS: [(&str, &str); 27] = [
    ("java.version", "17"),
    ("java.specification.version", "17"),
    ("java.vm.name", "oxjvm"),
    ("java.vm.version", "0.1.0"),
    ("java.vm.vendor", "oxjvm"),
    ("java.vendor", "oxjvm"),
    ("java.vendor.url", "https://oxjvm.invalid/"),
    ("os.name", "Linux"),
    ("os.arch", "amd64"),
    ("os.version", "6.1.0"),
    ("file.separator", "/"),
    ("path.separator", ":"),
    ("line.separator", "\n"),
    ("file.encoding", "UTF-8"),
    ("sun.jnu.encoding", "UTF-8"),
    ("user.name", "oxjvm"),
    ("user.home", "/tmp"),
    ("user.country", "US"),
    ("user.language", "en"),
    ("user.timezone", "GMT"),
    ("jdk.debug", "release"),
    ("sun.arch.data.model", "64"),
    ("sun.cpu.endian", "little"),
    ("java.home", "/oxjvm"),
    ("java.io.tmpdir", "/tmp"),
    ("sun.java.command", ""),
    ("java.awt.headless", "true"),
];

/// The system properties a `-D` override must not change.
///
/// JEP 223 pins these to the values the platform reports, so the JDK's launcher drops a `-D`
/// that tries to replace one of them: `java -Djava.vm.name=Other` still reports the real VM
/// name.  The guest may still call `System.setProperty` on them, exactly as on the JDK.
pub(crate) const NON_WRITABLE_PROPERTIES: [&str; 24] = [
    "java.vendor.version",
    "java.vendor.url",
    "java.vendor",
    "java.vendor.url.bug",
    "java.vendor.url.bug.id",
    "java.vendor.url.bug.help",
    "java.vendor.url.security",
    "java.specification.version",
    "java.specification.vendor",
    "java.specification.name",
    "java.specification.title",
    "java.vm.version",
    "java.vm.name",
    "java.vm.vendor",
    "java.vm.specification.version",
    "java.vm.specification.vendor",
    "java.vm.specification.name",
    "java.runtime.version",
    "java.runtime.name",
    "java.runtime.vendor",
    "java.runtime.specification.version",
    "java.runtime.specification.vendor",
    "java.runtime.specification.name",
    "java.specification.maintenance.version",
];

/// Whether a `-D` override for this property has to be dropped.
#[must_use]
pub fn is_non_writable_property(key: &str) -> bool {
    NON_WRITABLE_PROPERTIES.contains(&key)
}

/// Looks up a system property: guest overrides first, then the host's own properties, then the
/// VM defaults.
///
/// The host comes before the defaults on purpose: a `-D` override on the command line has to win
/// over the built-in value, or a platform-specific property such as `java.io.tmpdir` would be
/// shadowed by the hard-coded one.
pub(crate) fn system_property(vm: &mut Vm<'_>, key: &str) -> Option<String> {
    if let Some(value) = vm.get_property(key) {
        return Some(value);
    }
    match key {
        // Virtual defaults that depend on runtime state rather than a constant.
        "java.class.path" => Some(String::new()),
        // `user.dir` is the process working directory, which only the host knows.  It used to be
        // pinned to "/" in the defaults above, which shadowed the host's real value (jtreg runs
        // each test in its own scratch directory, so `Paths.get("genSrc").toAbsolutePath()`
        // resolved to `/genSrc` instead of `<scratch>/genSrc`).
        "user.dir" => vm.host_property(key).or_else(|| Some(String::from("/"))),
        _ => {
            if let Some(value) = vm.host_property(key) {
                return Some(value);
            }
            if let Some((_, value)) = SYSTEM_PROPERTY_DEFAULTS
                .iter()
                .find(|(name, _)| *name == key)
            {
                return Some((*value).to_string());
            }
            None
        }
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

/// `System.setProperty(String, String)`: stores the value and returns the previous one.
fn system_set_property(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let key = string_arg(vm, args[0])?;
    let value = string_arg(vm, args[1])?;
    let previous = vm.get_property(&key);
    vm.set_property(&key, value);
    match previous {
        Some(text) => object(vm.make_string(&text)?),
        None => object(ObjectRef::NULL),
    }
}

/// `System.getProperties()`: a `Properties` snapshot of the defaults, host properties and guest
/// overrides, in that order.
fn system_get_properties(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    _args: &[Value],
) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/util/Properties")?;
    let instance = vm.new_instance(class)?;
    if let Some((declaring, ctor)) = vm.find_method(class, "<init>", "()V") {
        vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
    }
    let (put_class, put) = vm.resolve_virtual_method(
        instance,
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
    )?;
    let mut entries: Vec<(String, String)> = SYSTEM_PROPERTY_DEFAULTS
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect();
    entries.push(("java.class.path".to_string(), String::new()));
    entries.push(("java.io.tmpdir".to_string(), "/tmp".to_string()));
    let host_entries = vm.host.properties();
    entries.extend(host_entries);
    entries.extend(vm.property_overrides());
    if !entries.iter().any(|(key, _)| key == "user.dir") {
        let value = system_property(vm, "user.dir").unwrap_or_else(|| String::from("/"));
        entries.push(("user.dir".to_string(), value));
    }
    for (key, value) in entries {
        let key = vm.make_string(&key)?;
        let value = vm.make_string(&value)?;
        vm.invoke_method(
            put_class,
            put,
            vec![Value::Ref(instance), Value::Ref(key), Value::Ref(value)],
        )?;
    }
    object(instance)
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
    // System.in: an empty ByteArrayInputStream (tests replace it via setIn).
    let input_class = vm.resolve_class("java/io/ByteArrayInputStream")?;
    let input = vm.new_instance(input_class)?;
    let (declaring, ctor) = vm
        .find_method(input_class, "<init>", "([B)V")
        .ok_or_else(|| VmError::internal("ByteArrayInputStream(byte[]) constructor missing"))?;
    let empty = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, 0)?;
    vm.invoke_method(declaring, ctor, vec![Value::Ref(input), Value::Ref(empty)])?;
    vm.set_static_value(system, "in", "Ljava/io/InputStream;", Value::Ref(input));
    Ok(())
}

pub(crate) const SYSTEM: oxjvm_vm::NativeClass = class(
    "java/lang/System",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("in", "Ljava/io/InputStream;", ACC_PUBLIC | ACC_STATIC, None),
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

const RUNTIME_METHODS: [oxjvm_vm::NativeMethodDef; 17] = [
    method(
        "exec",
        "(Ljava/lang/String;)Ljava/lang/Process;",
        ACC_PUBLIC,
        |vm, _, args| crate::process::runtime_exec(vm, args),
    ),
    method(
        "exec",
        "([Ljava/lang/String;)Ljava/lang/Process;",
        ACC_PUBLIC,
        |vm, _, args| crate::process::runtime_exec_array(vm, args),
    ),
    method(
        "exec",
        "(Ljava/lang/String;[Ljava/lang/String;)Ljava/lang/Process;",
        ACC_PUBLIC,
        |vm, _, args| crate::process::runtime_exec(vm, args),
    ),
    method(
        "exec",
        "([Ljava/lang/String;[Ljava/lang/String;)Ljava/lang/Process;",
        ACC_PUBLIC,
        |vm, _, args| crate::process::runtime_exec_array_env(vm, args),
    ),
    method(
        "exec",
        "(Ljava/lang/String;[Ljava/lang/String;Ljava/io/File;)Ljava/lang/Process;",
        ACC_PUBLIC,
        |vm, _, args| crate::process::runtime_exec_dir(vm, args),
    ),
    method(
        "exec",
        "([Ljava/lang/String;[Ljava/lang/String;Ljava/io/File;)Ljava/lang/Process;",
        ACC_PUBLIC,
        |vm, _, args| crate::process::runtime_exec_array_dir(vm, args),
    ),
    method(
        "getRuntime",
        "()Ljava/lang/Runtime;",
        ACC_PUBLIC | ACC_STATIC,
        runtime_get_runtime,
    ),
    method("availableProcessors", "()I", ACC_PUBLIC, |_, _, _| int(1)),
    method("freeMemory", "()J", ACC_PUBLIC, |vm, _, _| {
        long(vm.heap_free_bytes() as i64)
    }),
    method("totalMemory", "()J", ACC_PUBLIC, |vm, _, _| {
        long(vm.heap_total_bytes() as i64)
    }),
    method("maxMemory", "()J", ACC_PUBLIC, |vm, _, _| {
        long(vm.heap_max_bytes().unwrap_or(i64::MAX as u64) as i64)
    }),
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
        "()Ljava/lang/Runtime$Version;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| runtime_version(vm),
    ),
];

/// `Runtime.version()`: the JDK 27 GA version, as a `Runtime$Version`.
fn runtime_version(vm: &mut Vm<'_>) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/lang/Runtime$Version")?;
    let instance = vm.new_instance(class)?;
    for (name, value) in [("feature", 27), ("interim", 0), ("update", 0), ("patch", 0)] {
        if let Some((declaring, field)) = vm.find_field_opt(class, name, "I") {
            vm.set_instance_int(instance, declaring, field, value);
        }
    }
    object(instance)
}

const RUNTIME_VERSION_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
    method("feature", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "feature"))
    }),
    method("major", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "feature"))
    }),
    method("interim", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "interim"))
    }),
    method("update", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "update"))
    }),
    method("patch", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "patch"))
    }),
    // A GA release has no pre-release or build identifiers.
    method("pre", "()Ljava/util/Optional;", ACC_PUBLIC, |vm, _, _| {
        empty_optional(vm)
    }),
    method("build", "()Ljava/util/Optional;", ACC_PUBLIC, |vm, _, _| {
        empty_optional(vm)
    }),
    method(
        "optional",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, _| empty_optional(vm),
    ),
    method(
        "version",
        "()Ljava/util/List;",
        ACC_PUBLIC,
        runtime_version_components,
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        runtime_version_to_string,
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
            if vm.class_of(receiver(args)) != vm.class_of(other) {
                return boolean(false);
            }
            boolean(runtime_version_compare(vm, receiver(args), other) == 0)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let mut hash = 1i32;
        for name in ["feature", "interim", "update", "patch"] {
            hash = hash
                .wrapping_mul(31)
                .wrapping_add(read_int_field(vm, this, name));
        }
        int(hash)
    }),
    method(
        "compareTo",
        "(Ljava/lang/Runtime$Version;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            int(runtime_version_compare(vm, receiver(args), other))
        },
    ),
    method(
        "parse",
        "(Ljava/lang/String;)Ljava/lang/Runtime$Version;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| runtime_version_parse(vm, args),
    ),
];

/// An empty `java.util.Optional` (the value field defaults to null).
fn empty_optional(vm: &mut Vm<'_>) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/util/Optional")?;
    let instance = vm.new_instance(class)?;
    object(instance)
}

/// `Runtime$Version.parse`: "27", "27.0.1", "27-ea+5" ...
fn runtime_version_parse(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let text = string_arg(vm, args[0])?;
    let (numbers, _) = match text.split_once('-') {
        Some((numbers, rest)) => (numbers, rest),
        None => match text.split_once('+') {
            Some((numbers, rest)) => (numbers, rest),
            None => (text.as_str(), ""),
        },
    };
    let mut parts = Vec::new();
    for part in numbers.split('.') {
        match part.parse::<i32>() {
            Ok(value) => parts.push(value),
            Err(_) => {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some(&alloc::format!("Invalid version string: {text}")),
                ));
            }
        }
    }
    if parts.is_empty() || parts.len() > 4 {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some(&alloc::format!("Invalid version string: {text}")),
        ));
    }
    let class = vm.resolve_class("java/lang/Runtime$Version")?;
    let instance = vm.new_instance(class)?;
    for (index, name) in ["feature", "interim", "update", "patch"].iter().enumerate() {
        let value = parts.get(index).copied().unwrap_or(0);
        if let Some((declaring, field)) = vm.find_field_opt(class, name, "I") {
            vm.set_instance_int(instance, declaring, field, value);
        }
    }
    object(instance)
}

fn runtime_version_compare(vm: &Vm<'_>, this: ObjectRef, other: ObjectRef) -> i32 {
    for name in ["feature", "interim", "update", "patch"] {
        let left = read_int_field(vm, this, name);
        let right = read_int_field(vm, other, name);
        if left != right {
            return if left < right { -1 } else { 1 };
        }
    }
    0
}

fn runtime_version_components(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let list_class = vm.resolve_class("java/util/ArrayList")?;
    let list = vm.new_instance(list_class)?;
    let (declaring, method) = vm
        .find_method(list_class, "<init>", "()V")
        .ok_or_else(|| VmError::internal("ArrayList() missing"))?;
    vm.invoke_method(declaring, method, vec![Value::Ref(list)])?;
    let (declaring, add) = vm
        .find_method(list_class, "add", "(Ljava/lang/Object;)Z")
        .ok_or_else(|| VmError::internal("ArrayList.add missing"))?;
    let this = receiver(args);
    for name in ["feature", "interim", "update", "patch"] {
        let value = read_int_field(vm, this, name);
        let boxed = crate::boxed::boxed_int(vm, value)?;
        vm.invoke_method(declaring, add, vec![Value::Ref(list), boxed])?;
    }
    object(list)
}

fn runtime_version_to_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let feature = read_int_field(vm, this, "feature");
    let mut text = alloc::format!("{feature}");
    let mut parts = Vec::new();
    parts.push(read_int_field(vm, this, "interim"));
    parts.push(read_int_field(vm, this, "update"));
    parts.push(read_int_field(vm, this, "patch"));
    while parts.last() == Some(&0) {
        parts.pop();
    }
    for part in parts {
        text.push('.');
        text.push_str(&alloc::format!("{part}"));
    }
    object(vm.make_string(&text)?)
}

pub(crate) const RUNTIME_VERSION: oxjvm_vm::NativeClass = class(
    "java/lang/Runtime$Version",
    Some("java/lang/Object"),
    &["java/lang/Comparable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("feature", "I", ACC_PRIVATE | ACC_FINAL, None),
        field("interim", "I", ACC_PRIVATE | ACC_FINAL, None),
        field("update", "I", ACC_PRIVATE | ACC_FINAL, None),
        field("patch", "I", ACC_PRIVATE | ACC_FINAL, None),
    ],
    &RUNTIME_VERSION_METHODS,
    None,
);

fn runtime_get_runtime(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    _args: &[Value],
) -> Result<Value, VmError> {
    let system = vm.resolve_class("java/lang/Runtime")?;
    match vm.read_static_ref_if_set(system, "currentRuntime", "Ljava/lang/Runtime;") {
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

const MODULE_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method("isNamed", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("getName", "()Ljava/lang/String;", ACC_PUBLIC, |_, _, _| {
        object(ObjectRef::NULL)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("unnamed module")?),
    ),
    method(
        "getClassLoader",
        "()Ljava/lang/ClassLoader;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
];

/// The unnamed module: every native/host class lives on the class path.
pub(crate) const MODULE: oxjvm_vm::NativeClass = class(
    "java/lang/Module",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &MODULE_METHODS,
    None,
);

const MODULE_LAYER_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "boot",
        "()Ljava/lang/ModuleLayer;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/lang/ModuleLayer")?;
            object(vm.new_instance(class)?)
        },
    ),
    method(
        "findLoader",
        "(Ljava/lang/String;)Ljava/lang/ClassLoader;",
        ACC_PUBLIC,
        |vm, _, _| object(system_class_loader(vm)?),
    ),
];

/// `java.lang.ModuleLayer`: the boot layer only needs `boot()`/`findLoader()`.
pub(crate) const MODULE_LAYER: oxjvm_vm::NativeClass = class(
    "java/lang/ModuleLayer",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &MODULE_LAYER_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// System.Logger
// -------------------------------------------------------------------------------------------

const SYSTEM_LOGGER_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name = vm
                .read_string_field(this, vm.class_of(this), "name")
                .unwrap_or_default();
            object(vm.make_string(&name)?)
        },
    ),
    method(
        "isLoggable",
        "(Ljava/lang/System$Logger$Level;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(true),
    ),
    method(
        "log",
        "(Ljava/lang/System$Logger$Level;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "log",
        "(Ljava/lang/System$Logger$Level;Ljava/util/function/Supplier;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "log",
        "(Ljava/lang/System$Logger$Level;Ljava/lang/String;Ljava/lang/Throwable;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "log",
        "(Ljava/lang/System$Logger$Level;Ljava/lang/String;[Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method("info", "(Ljava/lang/String;)V", ACC_PUBLIC, |_, _, _| {
        void()
    }),
    method(
        "info",
        "(Ljava/util/function/Supplier;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method("warn", "(Ljava/lang/String;)V", ACC_PUBLIC, |_, _, _| {
        void()
    }),
    method(
        "warn",
        "(Ljava/util/function/Supplier;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method("error", "(Ljava/lang/String;)V", ACC_PUBLIC, |_, _, _| {
        void()
    }),
    method(
        "error",
        "(Ljava/util/function/Supplier;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method("debug", "(Ljava/lang/String;)V", ACC_PUBLIC, |_, _, _| {
        void()
    }),
    method("trace", "(Ljava/lang/String;)V", ACC_PUBLIC, |_, _, _| {
        void()
    }),
];

/// `System.Logger`: the no-op logger interface.
pub(crate) const SYSTEM_LOGGER: oxjvm_vm::NativeClass = class(
    "java/lang/System$Logger",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &SYSTEM_LOGGER_METHODS,
    None,
);

/// The concrete logger returned by `System.getLogger`.
pub(crate) const SYSTEM_LOGGER_IMPL: oxjvm_vm::NativeClass = class(
    "java/lang/System$OjvmLogger",
    Some("java/lang/Object"),
    &["java/lang/System$Logger"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field("name", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &SYSTEM_LOGGER_METHODS,
    None,
);

const SYSTEM_LOGGER_LEVEL_METHODS: [oxjvm_vm::NativeMethodDef; 0] = [];

fn system_logger_level_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/lang/System$Logger$Level")?;
    for name in ["ALL", "TRACE", "DEBUG", "INFO", "WARNING", "ERROR", "OFF"] {
        let value = vm.new_instance(class)?;
        vm.set_static_value(
            class,
            name,
            "Ljava/lang/System$Logger$Level;",
            Value::Ref(value),
        );
    }
    Ok(())
}

/// `System.Logger.Level`: the constants are objects with no additional state.
pub(crate) const SYSTEM_LOGGER_LEVEL: oxjvm_vm::NativeClass = class(
    "java/lang/System$Logger$Level",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field(
            "ALL",
            "Ljava/lang/System$Logger$Level;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "TRACE",
            "Ljava/lang/System$Logger$Level;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "DEBUG",
            "Ljava/lang/System$Logger$Level;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "INFO",
            "Ljava/lang/System$Logger$Level;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "WARNING",
            "Ljava/lang/System$Logger$Level;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "ERROR",
            "Ljava/lang/System$Logger$Level;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "OFF",
            "Ljava/lang/System$Logger$Level;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &SYSTEM_LOGGER_LEVEL_METHODS,
    Some(system_logger_level_clinit),
);

// -------------------------------------------------------------------------------------------
// ProcessBuilder
// -------------------------------------------------------------------------------------------

/// Store a command (a `String[]` or `List`) as the builder's `List<String>` command.
fn process_builder_store_command(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    command: ObjectRef,
) -> Result<Value, VmError> {
    let list = if command.is_null() {
        crate::util::array_list_from(vm, &[])?
    } else if vm.class_name(vm.class_of(command)).starts_with('[') {
        let length = vm.array_length(command)?;
        let mut elements = Vec::new();
        for index in 0..length {
            elements.push(vm.array_get(command, index as i32)?.as_ref());
        }
        crate::util::array_list_from(vm, &elements)?
    } else {
        command
    };
    let (declaring, field) = vm.find_field(vm.class_of(this), "command", "Ljava/util/List;")?;
    vm.set_instance_ref(this, declaring, field, list);
    object(this)
}

const PROCESS_BUILDER_METHODS: [oxjvm_vm::NativeMethodDef; 15] = [
    method(
        "<init>",
        "([Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| process_builder_store_command(vm, receiver(args), ref_arg(args, 1)),
    ),
    method(
        "<init>",
        "(Ljava/util/List;)V",
        ACC_PUBLIC,
        |vm, _, args| process_builder_store_command(vm, receiver(args), ref_arg(args, 1)),
    ),
    method(
        "command",
        "()Ljava/util/List;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let command = vm
                .read_ref_field(this, vm.class_of(this), "command", "Ljava/util/List;")
                .unwrap_or(ObjectRef::NULL);
            object(command)
        },
    ),
    method(
        "command",
        "([Ljava/lang/String;)Ljava/lang/ProcessBuilder;",
        ACC_PUBLIC,
        |vm, _, args| process_builder_store_command(vm, receiver(args), ref_arg(args, 1)),
    ),
    method(
        "command",
        "(Ljava/util/List;)Ljava/lang/ProcessBuilder;",
        ACC_PUBLIC,
        |vm, _, args| process_builder_store_command(vm, receiver(args), ref_arg(args, 1)),
    ),
    method(
        "start",
        "()Ljava/lang/Process;",
        ACC_PUBLIC,
        |vm, _, args| crate::process::process_builder_start(vm, receiver(args)),
    ),
    method("redirectErrorStream", "()Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
    method(
        "redirectErrorStream",
        "(Z)Ljava/lang/ProcessBuilder;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method(
        "redirectInput",
        "(Ljava/lang/ProcessBuilder$Redirect;)Ljava/lang/ProcessBuilder;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method(
        "redirectOutput",
        "(Ljava/lang/ProcessBuilder$Redirect;)Ljava/lang/ProcessBuilder;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method(
        "redirectError",
        "(Ljava/lang/ProcessBuilder$Redirect;)Ljava/lang/ProcessBuilder;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method(
        "inheritIO",
        "()Ljava/lang/ProcessBuilder;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method("directory", "()Ljava/io/File;", ACC_PUBLIC, |_, _, _| {
        object(ObjectRef::NULL)
    }),
    method(
        "directory",
        "(Ljava/io/File;)Ljava/lang/ProcessBuilder;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method(
        "environment",
        "()Ljava/util/Map;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/HashMap")?;
            object(vm.new_instance(class)?)
        },
    ),
];

/// `ProcessBuilder$Redirect`: the constants plus the redirects a caller builds from a file.
pub(crate) const PROCESS_BUILDER_REDIRECT: oxjvm_vm::NativeClass = class(
    "java/lang/ProcessBuilder$Redirect",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field(
            "INHERIT",
            "Ljava/lang/ProcessBuilder$Redirect;",
            ACC_PUBLIC | ACC_STATIC,
            None,
        ),
        field(
            "PIPE",
            "Ljava/lang/ProcessBuilder$Redirect;",
            ACC_PUBLIC | ACC_STATIC,
            None,
        ),
        field(
            "DISCARD",
            "Ljava/lang/ProcessBuilder$Redirect;",
            ACC_PUBLIC | ACC_STATIC,
            None,
        ),
    ],
    &REDIRECT_METHODS,
    Some(redirect_clinit),
);

/// `ProcessBuilder$Redirect$Type`: the kind of a redirect.
pub(crate) const REDIRECT_TYPE: oxjvm_vm::NativeClass = class(
    "java/lang/ProcessBuilder$Redirect$Type",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("ordinal", "I", ACC_PRIVATE, None),
        field(
            "PIPE",
            "Ljava/lang/ProcessBuilder$Redirect$Type;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "INHERIT",
            "Ljava/lang/ProcessBuilder$Redirect$Type;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "DISCARD",
            "Ljava/lang/ProcessBuilder$Redirect$Type;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "READ",
            "Ljava/lang/ProcessBuilder$Redirect$Type;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "WRITE",
            "Ljava/lang/ProcessBuilder$Redirect$Type;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "APPEND",
            "Ljava/lang/ProcessBuilder$Redirect$Type;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &REDIRECT_TYPE_METHODS,
    Some(redirect_type_clinit),
);

/// The redirect a caller builds with `ProcessBuilder.Redirect.from` and friends.
pub(crate) const OJVM_REDIRECT: oxjvm_vm::NativeClass = class(
    "java/lang/OjvmRedirect",
    Some("java/lang/ProcessBuilder$Redirect"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field(
            "type",
            "Ljava/lang/ProcessBuilder$Redirect$Type;",
            ACC_PRIVATE,
            None,
        ),
        field("file", "Ljava/io/File;", ACC_PRIVATE, None),
    ],
    &[],
    None,
);

const REDIRECT_TYPE_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let class = vm.class_of(receiver(args));
        object(
            vm.make_string(
                &vm.read_string_field(receiver(args), class, "name")
                    .unwrap_or_default(),
            )?,
        )
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            // An enum prints its name.
            let class = vm.class_of(receiver(args));
            object(
                vm.make_string(
                    &vm.read_string_field(receiver(args), class, "name")
                        .unwrap_or_default(),
                )?,
            )
        },
    ),
    method("ordinal", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "ordinal"))
    }),
];

const REDIRECT_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "from",
        "(Ljava/io/File;)Ljava/lang/ProcessBuilder$Redirect;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| new_redirect(vm, "READ", ref_arg(args, 0)),
    ),
    method(
        "to",
        "(Ljava/io/File;)Ljava/lang/ProcessBuilder$Redirect;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| new_redirect(vm, "WRITE", ref_arg(args, 0)),
    ),
    method(
        "appendTo",
        "(Ljava/io/File;)Ljava/lang/ProcessBuilder$Redirect;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| new_redirect(vm, "APPEND", ref_arg(args, 0)),
    ),
    method(
        "type",
        "()Ljava/lang/ProcessBuilder$Redirect$Type;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = vm.class_of(receiver(args));
            object(
                vm.read_ref_field(
                    receiver(args),
                    class,
                    "type",
                    "Ljava/lang/ProcessBuilder$Redirect$Type;",
                )
                .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method("file", "()Ljava/io/File;", ACC_PUBLIC, |vm, _, args| {
        let class = vm.class_of(receiver(args));
        object(
            vm.read_ref_field(receiver(args), class, "file", "Ljava/io/File;")
                .unwrap_or(ObjectRef::NULL),
        )
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = vm.class_of(receiver(args));
            let kind = vm
                .read_ref_field(
                    receiver(args),
                    class,
                    "type",
                    "Ljava/lang/ProcessBuilder$Redirect$Type;",
                )
                .unwrap_or(ObjectRef::NULL);
            let name = if kind.is_null() {
                String::new()
            } else {
                vm.read_string_field(kind, vm.class_of(kind), "name")
                    .unwrap_or_default()
            };
            let file = vm
                .read_ref_field(receiver(args), class, "file", "Ljava/io/File;")
                .unwrap_or(ObjectRef::NULL);
            let path = if file.is_null() {
                String::new()
            } else {
                crate::file::file_path(vm, file)
            };
            let text = match name.as_str() {
                "READ" => alloc::format!("redirect to read from file \"{path}\""),
                "WRITE" => alloc::format!("redirect to write to file \"{path}\""),
                "APPEND" => alloc::format!("redirect to append to file \"{path}\""),
                other => other.to_string(),
            };
            object(vm.make_string(&text)?)
        },
    ),
];

/// A redirect of the given kind, pointing at a file.
fn new_redirect(vm: &mut Vm<'_>, kind: &str, file: ObjectRef) -> Result<Value, VmError> {
    if file.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let class = vm.resolve_class("java/lang/OjvmRedirect")?;
    let instance = vm.new_instance(class)?;
    let type_class = vm.resolve_class("java/lang/ProcessBuilder$Redirect$Type")?;
    vm.ensure_initialized(type_class)?;
    let kind_value = vm
        .read_static_ref_if_set(type_class, kind, "Ljava/lang/ProcessBuilder$Redirect$Type;")
        .unwrap_or(ObjectRef::NULL);
    let (declaring, field) =
        vm.find_field(class, "type", "Ljava/lang/ProcessBuilder$Redirect$Type;")?;
    vm.set_instance_ref(instance, declaring, field, kind_value);
    let (declaring, field) = vm.find_field(class, "file", "Ljava/io/File;")?;
    vm.set_instance_ref(instance, declaring, field, file);
    object(instance)
}

/// The type constants of a redirect.
fn redirect_type_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/lang/ProcessBuilder$Redirect$Type")?;
    for (ordinal, name) in ["PIPE", "INHERIT", "DISCARD", "READ", "WRITE", "APPEND"]
        .iter()
        .enumerate()
    {
        let instance = vm.new_instance(class)?;
        vm.set_string_field(instance, class, "name", name)?;
        write_int_field(vm, instance, "ordinal", ordinal as i32);
        vm.set_static_value(
            class,
            name,
            "Ljava/lang/ProcessBuilder$Redirect$Type;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

/// The three redirects the JDK defines as constants.
fn redirect_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/lang/OjvmRedirect")?;
    let type_class = vm.resolve_class("java/lang/ProcessBuilder$Redirect$Type")?;
    vm.ensure_initialized(type_class)?;
    for (name, kind) in [
        ("PIPE", "PIPE"),
        ("INHERIT", "INHERIT"),
        ("DISCARD", "WRITE"),
    ] {
        let instance = vm.new_instance(class)?;
        let kind_value = vm
            .read_static_ref_if_set(type_class, kind, "Ljava/lang/ProcessBuilder$Redirect$Type;")
            .unwrap_or(ObjectRef::NULL);
        let (declaring, field) =
            vm.find_field(class, "type", "Ljava/lang/ProcessBuilder$Redirect$Type;")?;
        vm.set_instance_ref(instance, declaring, field, kind_value);
        let redirect_class = vm.resolve_class("java/lang/ProcessBuilder$Redirect")?;
        vm.set_static_value(
            redirect_class,
            name,
            "Ljava/lang/ProcessBuilder$Redirect;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

/// `java.lang.ProcessBuilder`: external processes cannot be started (`start` throws).
pub(crate) const PROCESS_BUILDER: oxjvm_vm::NativeClass = class(
    "java/lang/ProcessBuilder",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[field("command", "Ljava/util/List;", ACC_PRIVATE, None)],
    &PROCESS_BUILDER_METHODS,
    None,
);

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

const THREAD_METHODS: [oxjvm_vm::NativeMethodDef; 47] = [
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
        "<init>",
        "(Ljava/lang/ThreadGroup;Ljava/lang/Runnable;)V",
        ACC_PUBLIC,
        thread_init,
    ),
    method(
        "<init>",
        "(Ljava/lang/ThreadGroup;Ljava/lang/Runnable;Ljava/lang/String;)V",
        ACC_PUBLIC,
        thread_init,
    ),
    method(
        "<init>",
        "(Ljava/lang/ThreadGroup;Ljava/lang/String;)V",
        ACC_PUBLIC,
        thread_init,
    ),
    method(
        "<init>",
        "(Ljava/lang/ThreadGroup;Ljava/lang/Runnable;Ljava/lang/String;J)V",
        ACC_PUBLIC,
        thread_init,
    ),
    method(
        "<init>",
        "(Ljava/lang/ThreadGroup;Ljava/lang/Runnable;Ljava/lang/String;JZ)V",
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
    method(
        "setContextClassLoader",
        "(Ljava/lang/ClassLoader;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let (declaring, field) =
                vm.find_field(class, "contextClassLoader", "Ljava/lang/ClassLoader;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method(
        "getContextClassLoader",
        "()Ljava/lang/ClassLoader;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let loader = vm
                .read_ref_field(this, class, "contextClassLoader", "Ljava/lang/ClassLoader;")
                .unwrap_or(ObjectRef::NULL);
            // The JVM initialises a thread's context class loader to the system class loader;
            // libraries such as TestNG call `getContextClassLoader().getResourceAsStream(...)`
            // unconditionally, so a null value would NPE.
            if loader.is_null() {
                object(system_class_loader(vm)?)
            } else {
                object(loader)
            }
        },
    ),
    method(
        "ofVirtual",
        "()Ljava/lang/Thread$Builder$OfVirtual;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| new_thread_builder(vm),
    ),
    method(
        "ofPlatform",
        "()Ljava/lang/Thread$Builder$OfPlatform;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| new_thread_builder(vm),
    ),
    method(
        "getStackTrace",
        "()[Ljava/lang/StackTraceElement;",
        ACC_PUBLIC,
        |vm, _, args| thread_stack_trace(vm, receiver(args)),
    ),
    method(
        "join",
        "(Ljava/time/Duration;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let duration = ref_arg(args, 1);
            let (class, method) = vm.resolve_virtual_method(duration, "toMillis", "()J")?;
            let millis = vm
                .invoke_method(class, method, vec![Value::Ref(duration)])?
                .as_long();
            vm.join_thread(receiver(args), Some(millis.max(0)));
            boolean(true)
        },
    ),
    method(
        "getThreadGroup",
        "()Ljava/lang/ThreadGroup;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, _| object(new_thread_group(vm, "main")?),
    ),
    method("isVirtual", "()Z", ACC_PUBLIC | ACC_FINAL, |_, _, _| {
        boolean(false)
    }),
    method("threadId", "()J", ACC_PUBLIC, |vm, _, args| {
        long(vm.thread_id(receiver(args)))
    }),
    method(
        "startVirtualThread",
        "(Ljava/lang/Runnable;)Ljava/lang/Thread;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let class = vm.resolve_class("java/lang/Thread")?;
            let thread = vm.new_instance(class)?;
            thread_init(
                vm,
                oxjvm_vm::NativeContext { class, method: 0 },
                &[Value::Ref(thread), Value::Ref(ref_arg(args, 0))],
            )?;
            vm.start_thread(thread)?;
            object(thread)
        },
    ),
    method(
        "getAllStackTraces",
        "()Ljava/util/Map;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let map = crate::util::hash_map_new(vm)?;
            let current = vm.current_thread_object();
            let trace = thread_stack_trace(vm, current)?.as_ref();
            crate::util::hash_map_insert(vm, map, current, trace)?;
            object(map)
        },
    ),
    method(
        "sleep",
        "(Ljava/time/Duration;)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let duration = ref_arg(args, 0);
            let (class, method) = vm.resolve_virtual_method(duration, "toMillis", "()J")?;
            let millis = vm
                .invoke_method(class, method, vec![Value::Ref(duration)])?
                .as_long();
            vm.sleep_current(millis.max(0));
            void()
        },
    ),
    method("onSpinWait", "()V", ACC_PUBLIC | ACC_STATIC, |_, _, _| {
        void()
    }),
    method(
        "getState",
        "()Ljava/lang/Thread$State;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/lang/Thread$State")?;
            vm.ensure_initialized(class)?;
            object(
                vm.read_static_ref_if_set(class, "RUNNABLE", "Ljava/lang/Thread$State;")
                    .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method(
        "enumerate",
        "([Ljava/lang/Thread;)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let array = ref_arg(args, 0);
            let length = vm.array_length(array)?;
            if length > 0 {
                vm.array_set_ref(array, 0, vm.current_thread_object())?;
                int(1)
            } else {
                int(0)
            }
        },
    ),
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

// -------------------------------------------------------------------------------------------
// Thread.Builder
// -------------------------------------------------------------------------------------------

fn new_thread_builder(vm: &mut Vm<'_>) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/lang/OjvmThreadBuilder")?;
    object(vm.new_instance(class)?)
}

fn thread_builder_name(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "name", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
    object(this)
}

/// `unstarted`/`start`: create the thread (named if a name was set).
fn thread_builder_make(vm: &mut Vm<'_>, args: &[Value], start: bool) -> Result<Value, VmError> {
    let this = receiver(args);
    let target = ref_arg(args, 1);
    let thread_class = vm.resolve_class("java/lang/Thread")?;
    let thread = vm.new_instance(thread_class)?;
    let (declaring, ctor) = vm
        .find_method(thread_class, "<init>", "(Ljava/lang/Runnable;)V")
        .ok_or_else(|| VmError::internal("Thread(Runnable) constructor missing"))?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![Value::Ref(thread), Value::Ref(target)],
    )?;
    if let Some(name) = vm.read_string_field(this, vm.class_of(this), "name") {
        if !name.is_empty() {
            let value = vm.make_string(&name)?;
            let (declaring, method) =
                vm.resolve_virtual_method(thread, "setName", "(Ljava/lang/String;)V")?;
            vm.invoke_method(
                declaring,
                method,
                vec![Value::Ref(thread), Value::Ref(value)],
            )?;
        }
    }
    if start {
        let (declaring, method) = vm.resolve_virtual_method(thread, "start", "()V")?;
        vm.invoke_method(declaring, method, vec![Value::Ref(thread)])?;
    }
    object(thread)
}

const THREAD_BUILDER_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "name",
        "(Ljava/lang/String;)Ljava/lang/Thread$Builder;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "name",
        "(Ljava/lang/String;J)Ljava/lang/Thread$Builder;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "unstarted",
        "(Ljava/lang/Runnable;)Ljava/lang/Thread;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "start",
        "(Ljava/lang/Runnable;)Ljava/lang/Thread;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "factory",
        "()Ljava/util/concurrent/ThreadFactory;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
];

pub(crate) const THREAD_BUILDER: oxjvm_vm::NativeClass = class(
    "java/lang/Thread$Builder",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &THREAD_BUILDER_METHODS,
    None,
);

const OF_VIRTUAL_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "name",
        "(Ljava/lang/String;)Ljava/lang/Thread$Builder$OfVirtual;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "name",
        "(Ljava/lang/String;J)Ljava/lang/Thread$Builder$OfVirtual;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
];

pub(crate) const THREAD_BUILDER_OF_VIRTUAL: oxjvm_vm::NativeClass = class(
    "java/lang/Thread$Builder$OfVirtual",
    Some("java/lang/Object"),
    &["java/lang/Thread$Builder"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &OF_VIRTUAL_METHODS,
    None,
);

const OF_PLATFORM_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "name",
        "(Ljava/lang/String;)Ljava/lang/Thread$Builder$OfPlatform;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "name",
        "(Ljava/lang/String;J)Ljava/lang/Thread$Builder$OfPlatform;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
];

pub(crate) const THREAD_BUILDER_OF_PLATFORM: oxjvm_vm::NativeClass = class(
    "java/lang/Thread$Builder$OfPlatform",
    Some("java/lang/Object"),
    &["java/lang/Thread$Builder"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &OF_PLATFORM_METHODS,
    None,
);

const OJVM_THREAD_BUILDER_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method(
        "name",
        "(Ljava/lang/String;)Ljava/lang/Thread$Builder;",
        ACC_PUBLIC,
        thread_builder_name,
    ),
    method(
        "name",
        "(Ljava/lang/String;)Ljava/lang/Thread$Builder$OfVirtual;",
        ACC_PUBLIC,
        thread_builder_name,
    ),
    method(
        "name",
        "(Ljava/lang/String;)Ljava/lang/Thread$Builder$OfPlatform;",
        ACC_PUBLIC,
        thread_builder_name,
    ),
    method(
        "name",
        "(Ljava/lang/String;J)Ljava/lang/Thread$Builder;",
        ACC_PUBLIC,
        thread_builder_name,
    ),
    method(
        "name",
        "(Ljava/lang/String;J)Ljava/lang/Thread$Builder$OfVirtual;",
        ACC_PUBLIC,
        thread_builder_name,
    ),
    method(
        "name",
        "(Ljava/lang/String;J)Ljava/lang/Thread$Builder$OfPlatform;",
        ACC_PUBLIC,
        thread_builder_name,
    ),
    method(
        "unstarted",
        "(Ljava/lang/Runnable;)Ljava/lang/Thread;",
        ACC_PUBLIC,
        |vm, _, args| thread_builder_make(vm, args, false),
    ),
    method(
        "start",
        "(Ljava/lang/Runnable;)Ljava/lang/Thread;",
        ACC_PUBLIC,
        |vm, _, args| thread_builder_make(vm, args, true),
    ),
];

pub(crate) const OJVM_THREAD_BUILDER: oxjvm_vm::NativeClass = class(
    "java/lang/OjvmThreadBuilder",
    Some("java/lang/Object"),
    &[
        "java/lang/Thread$Builder",
        "java/lang/Thread$Builder$OfVirtual",
        "java/lang/Thread$Builder$OfPlatform",
    ],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field("name", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &OJVM_THREAD_BUILDER_METHODS,
    None,
);

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
        field(
            "contextClassLoader",
            "Ljava/lang/ClassLoader;",
            ACC_PRIVATE,
            None,
        ),
        field("threadLocals", "Ljava/util/HashMap;", ACC_PRIVATE, None),
    ],
    &THREAD_METHODS,
    None,
);

fn thread_stack_trace(vm: &mut Vm<'_>, _thread: ObjectRef) -> Result<Value, VmError> {
    let frames = vm.capture_frames();
    let element = vm.resolve_class("java/lang/StackTraceElement")?;
    let array = vm.allocate_object_array(element, frames.len())?;
    let constructor = vm.find_method(
        element,
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;I)V",
    );
    for (index, frame) in frames.into_iter().enumerate() {
        let element_object = vm.new_instance(element)?;
        let file = match &frame.file_name {
            Some(name) => vm.make_string(name)?,
            None => ObjectRef::NULL,
        };
        let class_name = vm.make_string(&frame.class_name)?;
        let method_name = vm.make_string(&frame.method_name)?;
        if let Some((constructor_class, constructor_method)) = constructor {
            vm.invoke_method(
                constructor_class,
                constructor_method,
                vec![
                    Value::Ref(element_object),
                    Value::Ref(class_name),
                    Value::Ref(method_name),
                    Value::Ref(file),
                    Value::Int(i32::from(frame.line_number.unwrap_or(0))),
                ],
            )?;
        }
        vm.array_set_ref(array, index, element_object)?;
    }
    object(array)
}

// -------------------------------------------------------------------------------------------
// ThreadGroup
// -------------------------------------------------------------------------------------------

fn new_thread_group(vm: &mut Vm<'_>, name: &str) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/lang/ThreadGroup")?;
    let group = vm.new_instance(class)?;
    vm.set_string_field(group, class, "name", name)?;
    Ok(group)
}

const THREAD_GROUP_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        vm.set_string_field(this, vm.class_of(this), "name", "main")?;
        void()
    }),
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name = string_arg(vm, args[1])?;
            vm.set_string_field(this, vm.class_of(this), "name", &name)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/ThreadGroup;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name = string_arg(vm, args[2])?;
            vm.set_string_field(this, vm.class_of(this), "name", &name)?;
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "parent", "Ljava/lang/ThreadGroup;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method(
        "getName",
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
        "getParent",
        "()Ljava/lang/ThreadGroup;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(
                vm.read_ref_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "parent",
                    "Ljava/lang/ThreadGroup;",
                )
                .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method("activeCount", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method("activeGroupCount", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method(
        "enumerate",
        "([Ljava/lang/Thread;)I",
        ACC_PUBLIC,
        |_, _, _| int(0),
    ),
    method("list", "()V", ACC_PUBLIC, |_, _, _| void()),
];

pub(crate) const THREAD_GROUP: oxjvm_vm::NativeClass = class(
    "java/lang/ThreadGroup",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("parent", "Ljava/lang/ThreadGroup;", ACC_PRIVATE, None),
    ],
    &THREAD_GROUP_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Thread.State
// -------------------------------------------------------------------------------------------

const THREAD_STATE_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let name = vm
            .read_string_field(receiver(args), vm.class_of(receiver(args)), "name")
            .unwrap_or_default();
        object(vm.make_string(&name)?)
    }),
    method("ordinal", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "ordinal"))
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
        "()[Ljava/lang/Thread$State;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/lang/Thread$State")?;
            let names = [
                "NEW",
                "RUNNABLE",
                "BLOCKED",
                "WAITING",
                "TIMED_WAITING",
                "TERMINATED",
            ];
            let array = vm.allocate_object_array(class, names.len())?;
            for (index, name) in names.iter().enumerate() {
                if let Some(value) =
                    vm.read_static_ref_if_set(class, name, "Ljava/lang/Thread$State;")
                {
                    vm.array_set_ref(array, index, value)?;
                }
            }
            object(array)
        },
    ),
    method(
        "valueOf",
        "(Ljava/lang/String;)Ljava/lang/Thread$State;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = vm.string_or_empty(ref_arg(args, 0));
            let class = vm.resolve_class("java/lang/Thread$State")?;
            vm.ensure_initialized(class)?;
            if let Some(value) = vm.read_static_ref_if_set(class, &name, "Ljava/lang/Thread$State;")
            {
                return object(value);
            }
            Err(vm.throw_new("java/lang/IllegalArgumentException", Some(&name)))
        },
    ),
];

fn thread_state_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/lang/Thread$State")?;
    for (ordinal, name) in [
        "NEW",
        "RUNNABLE",
        "BLOCKED",
        "WAITING",
        "TIMED_WAITING",
        "TERMINATED",
    ]
    .iter()
    .enumerate()
    {
        let instance = vm.new_instance(class)?;
        vm.set_string_field(instance, class, "name", name)?;
        write_int_field(vm, instance, "ordinal", ordinal as i32);
        vm.set_static_value(
            class,
            name,
            "Ljava/lang/Thread$State;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

pub(crate) const THREAD_STATE: oxjvm_vm::NativeClass = class(
    "java/lang/Thread$State",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("ordinal", "I", ACC_PRIVATE, None),
        field(
            "NEW",
            "Ljava/lang/Thread$State;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "RUNNABLE",
            "Ljava/lang/Thread$State;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "BLOCKED",
            "Ljava/lang/Thread$State;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "WAITING",
            "Ljava/lang/Thread$State;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "TIMED_WAITING",
            "Ljava/lang/Thread$State;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "TERMINATED",
            "Ljava/lang/Thread$State;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &THREAD_STATE_METHODS,
    Some(thread_state_clinit),
);

// -------------------------------------------------------------------------------------------
// ThreadLocal
// -------------------------------------------------------------------------------------------

const THREAD_LOCAL_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("get", "()Ljava/lang/Object;", ACC_PUBLIC, thread_local_get),
    method("set", "(Ljava/lang/Object;)V", ACC_PUBLIC, thread_local_set),
    method("remove", "()V", ACC_PUBLIC, thread_local_remove),
    method(
        "initialValue",
        "()Ljava/lang/Object;",
        ACC_PROTECTED,
        thread_local_initial_value,
    ),
    method(
        "withInitial",
        "(Ljava/util/function/Supplier;)Ljava/lang/ThreadLocal;",
        ACC_PUBLIC | ACC_STATIC,
        thread_local_with_initial,
    ),
];

pub(crate) const THREAD_LOCAL: oxjvm_vm::NativeClass = class(
    "java/lang/ThreadLocal",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[field(
        "supplier",
        "Ljava/util/function/Supplier;",
        ACC_PRIVATE,
        None,
    )],
    &THREAD_LOCAL_METHODS,
    None,
);

/// `InheritableThreadLocal`: the same value storage as `ThreadLocal`.
pub(crate) const INHERITABLE_THREAD_LOCAL: oxjvm_vm::NativeClass = class(
    "java/lang/InheritableThreadLocal",
    Some("java/lang/ThreadLocal"),
    &[],
    ACC_PUBLIC,
    &[],
    &THREAD_LOCAL_METHODS,
    None,
);

/// The current thread's `ThreadLocal` map, created on demand.
fn thread_locals_map(vm: &mut Vm<'_>, create: bool) -> Result<ObjectRef, VmError> {
    let thread = vm.current_thread_object();
    let class = vm.class_of(thread);
    if let Some(map) = vm.read_ref_field(thread, class, "threadLocals", "Ljava/util/HashMap;") {
        if !map.is_null() {
            return Ok(map);
        }
    }
    if !create {
        return Ok(ObjectRef::NULL);
    }
    let map_class = vm.resolve_class("java/util/HashMap")?;
    let map = vm.new_instance(map_class)?;
    let (declaring, ctor) = vm
        .find_method(map_class, "<init>", "()V")
        .ok_or_else(|| VmError::internal("HashMap lacks its constructor"))?;
    vm.invoke_method(declaring, ctor, vec![Value::Ref(map)])?;
    let class = vm.class_of(thread);
    let (declaring, field) = vm.find_field(class, "threadLocals", "Ljava/util/HashMap;")?;
    vm.set_instance_ref(thread, declaring, field, map);
    Ok(map)
}

fn thread_local_get(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let map = thread_locals_map(vm, true)?;
    let (get_class, get) =
        vm.resolve_virtual_method(map, "get", "(Ljava/lang/Object;)Ljava/lang/Object;")?;
    let value = vm.invoke_method(get_class, get, vec![Value::Ref(map), Value::Ref(this)])?;
    if !value.as_ref().is_null() {
        return Ok(value);
    }
    let (init_class, init) =
        vm.resolve_virtual_method(this, "initialValue", "()Ljava/lang/Object;")?;
    let created = vm.invoke_method(init_class, init, vec![Value::Ref(this)])?;
    if !created.as_ref().is_null() {
        let (put_class, put) = vm.resolve_virtual_method(
            map,
            "put",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        )?;
        vm.invoke_method(
            put_class,
            put,
            vec![Value::Ref(map), Value::Ref(this), created],
        )?;
    }
    Ok(created)
}

fn thread_local_set(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let map = thread_locals_map(vm, true)?;
    let value = ref_arg(args, 1);
    let (put_class, put) = vm.resolve_virtual_method(
        map,
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
    )?;
    vm.invoke_method(
        put_class,
        put,
        vec![Value::Ref(map), Value::Ref(this), Value::Ref(value)],
    )?;
    void()
}

fn thread_local_remove(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let map = thread_locals_map(vm, false)?;
    if !map.is_null() {
        let (remove_class, remove) =
            vm.resolve_virtual_method(map, "remove", "(Ljava/lang/Object;)Ljava/lang/Object;")?;
        vm.invoke_method(
            remove_class,
            remove,
            vec![Value::Ref(map), Value::Ref(this)],
        )?;
    }
    void()
}

fn thread_local_initial_value(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let class = vm.class_of(this);
    let supplier = vm
        .read_ref_field(this, class, "supplier", "Ljava/util/function/Supplier;")
        .unwrap_or(ObjectRef::NULL);
    if supplier.is_null() {
        return object(ObjectRef::NULL);
    }
    let (class, method) = vm.resolve_virtual_method(supplier, "get", "()Ljava/lang/Object;")?;
    vm.invoke_method(class, method, vec![Value::Ref(supplier)])
}

fn thread_local_with_initial(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let supplier = ref_arg(args, 0);
    let class = vm.resolve_class("java/lang/ThreadLocal")?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "supplier", "Ljava/util/function/Supplier;")?;
    vm.set_instance_ref(instance, declaring, field, supplier);
    object(instance)
}

// -------------------------------------------------------------------------------------------
// ClassLoader
// -------------------------------------------------------------------------------------------

const CLASS_LOADER_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method(
        "getSystemClassLoader",
        "()Ljava/lang/ClassLoader;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(system_class_loader(vm)?),
    ),
    method(
        "getParent",
        "()Ljava/lang/ClassLoader;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            object(
                vm.read_ref_field_any(receiver(args), "parent")
                    .unwrap_or(ObjectRef::NULL),
            )
        },
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
        |vm, _, args| {
            let name = string_arg(vm, args[1])?;
            object(first_resource_url(vm, &name)?)
        },
    ),
    method(
        "getResources",
        "(Ljava/lang/String;)Ljava/util/Enumeration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = string_arg(vm, args[1])?;
            let urls = resource_urls(vm, &name)?;
            object(crate::util::new_enumeration(vm, &urls)?)
        },
    ),
    method(
        "getResourceAsStream",
        "(Ljava/lang/String;)Ljava/io/InputStream;",
        ACC_PUBLIC,
        class_loader_resource_as_stream,
    ),
    method(
        "<init>",
        "(Ljava/lang/ClassLoader;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if let Some((declaring, field)) =
                vm.find_field_opt(vm.class_of(this), "parent", "Ljava/lang/ClassLoader;")
            {
                vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            }
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/ClassLoader;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if let Some((declaring, field)) =
                vm.find_field_opt(vm.class_of(this), "parent", "Ljava/lang/ClassLoader;")
            {
                vm.set_instance_ref(this, declaring, field, ref_arg(args, 2));
            }
            void()
        },
    ),
    method(
        "defineClass",
        "(Ljava/lang/String;[BII)Ljava/lang/Class;",
        ACC_PROTECTED | ACC_FINAL,
        |vm, _, args| {
            let name = string_arg(vm, args[1])?.replace('.', "/");
            let bytes = array_bytes(vm, ref_arg(args, 2))?;
            let offset = int_arg(args, 3).max(0) as usize;
            let length = int_arg(args, 4).max(0) as usize;
            let end = (offset + length).min(bytes.len());
            let class = vm.define_class(&name, &bytes[offset..end])?;
            object(vm.class_object(class)?)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("sun.misc.Launcher$AppClassLoader")?),
    ),
];

/// The URL of the first class-path resource with this name, or null.
fn first_resource_url(vm: &mut Vm<'_>, resource: &str) -> Result<ObjectRef, VmError> {
    match vm.host.find_resources(resource).first() {
        Some(hit) => crate::net::new_url(vm, &hit.url),
        None => Ok(ObjectRef::NULL),
    }
}

/// Every class-path resource with this name, as `URL`s in class-path order.
fn resource_urls(vm: &mut Vm<'_>, resource: &str) -> Result<Vec<ObjectRef>, VmError> {
    let hits = vm.host.find_resources(resource);
    let mut urls = Vec::with_capacity(hits.len());
    for hit in hits {
        urls.push(crate::net::new_url(vm, &hit.url)?);
    }
    Ok(urls)
}

/// `Class.getResource`: a leading `/` names an absolute resource, anything else is
/// resolved against the class's package.
fn class_get_resource(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let name = string_arg(vm, args[1])?;
    let class = class_argument(vm, receiver(args))?;
    let resource = if let Some(absolute) = name.strip_prefix('/') {
        String::from(absolute)
    } else {
        let full = vm.class_name(class).to_string();
        match full.rfind('/') {
            Some(index) => alloc::format!("{}{}", &full[..index + 1], name),
            None => name.clone(),
        }
    };
    object(first_resource_url(vm, &resource)?)
}

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
    ACC_PUBLIC,
    &[
        field(
            "systemLoader",
            "Ljava/lang/ClassLoader;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
        field("parent", "Ljava/lang/ClassLoader;", ACC_PRIVATE, None),
    ],
    &CLASS_LOADER_METHODS,
    None,
);

/// The application class loader singleton (`Class.getClassLoader`).
pub(crate) fn system_class_loader(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/lang/ClassLoader")?;
    if let Some(existing) =
        vm.read_static_ref_if_set(class, "systemLoader", "Ljava/lang/ClassLoader;")
    {
        return Ok(existing);
    }
    let instance = vm.new_instance(class)?;
    vm.set_static_value(
        class,
        "systemLoader",
        "Ljava/lang/ClassLoader;",
        Value::Ref(instance),
    );
    Ok(instance)
}

fn class_loader_resource_as_stream(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let resource = string_arg(vm, args[1])?;
    resource_as_stream(vm, &resource)
}

/// Open a class-path resource as a `ByteArrayInputStream`, or return null.
pub(crate) fn resource_as_stream(vm: &mut Vm<'_>, resource: &str) -> Result<Value, VmError> {
    let Some(bytes) = vm.host.load_resource(resource) else {
        return object(ObjectRef::NULL);
    };
    let class = vm.resolve_class("java/io/ByteArrayInputStream")?;
    let instance = vm.new_instance(class)?;
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, bytes.len())?;
    for (index, byte) in bytes.iter().enumerate() {
        vm.array_set(
            array,
            index as i32,
            Value::Int(i32::from(*byte as i8)),
            false,
        )?;
    }
    let (declaring, ctor) = vm
        .find_method(class, "<init>", "([B)V")
        .ok_or_else(|| VmError::internal("ByteArrayInputStream lacks its constructor"))?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![Value::Ref(instance), Value::Ref(array)],
    )?;
    object(instance)
}

// -------------------------------------------------------------------------------------------
// Enum / Record
// -------------------------------------------------------------------------------------------

const ENUM_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
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
    // The `Comparable` bridge that the compiler would generate for `Enum<E>`.
    method(
        "compareTo",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC | ACC_FINAL | ACC_BRIDGE | ACC_SYNTHETIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            if other.is_null() || vm.class_of(this) != vm.class_of(other) {
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
    method(
        "valueOf",
        "(Ljava/lang/Class;Ljava/lang/String;)Ljava/lang/Enum;",
        ACC_PUBLIC | ACC_STATIC,
        enum_value_of,
    ),
];

/// `Enum.valueOf(Class, String)`: the static enum constant with that name.
fn enum_value_of(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class_object = ref_arg(args, 0);
    if class_object.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let name = string_arg(vm, args[1])?;
    let enum_class = class_argument(vm, class_object)?;
    if !vm.classes.get(enum_class).is_enum {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some(&alloc::format!(
                "{} is not an enum type",
                vm.class_name(enum_class)
            )),
        ));
    }
    vm.ensure_initialized(enum_class)?;
    let class_name = vm.class_name(enum_class).to_string();
    let descriptor = alloc::format!("L{class_name};");
    if let Some(value) = vm.read_static_ref_if_set(enum_class, &name, &descriptor) {
        return object(value);
    }
    Err(vm.throw_new(
        "java/lang/IllegalArgumentException",
        Some(&alloc::format!(
            "No enum constant {}",
            class_name.replace('/', ".")
        )),
    ))
}

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
    let receiver = match _args.first() {
        Some(Value::Ref(reference)) if !reference.is_null() => {
            let receiver_class = vm.class_name(vm.class_of(*reference)).to_string();
            alloc::format!(" on {receiver_class}")
        }
        _ => String::new(),
    };
    Err(vm.throw_new(
        "java/lang/AbstractMethodError",
        Some(&alloc::format!("{class}.{method}{receiver}")),
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

const CHAR_SEQUENCE_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
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
        char_sequence_chars,
    ),
    method(
        "codePoints",
        "()Ljava/util/stream/IntStream;",
        ACC_PUBLIC,
        char_sequence_code_points,
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

/// The UTF-16 units of any `CharSequence` (via `toString` for non-Strings).
fn char_sequence_units(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Vec<u16>, VmError> {
    if let Some(units) = vm.string_utf16(this) {
        return Ok(units.to_vec());
    }
    let (class, method) = vm.resolve_virtual_method(this, "toString", "()Ljava/lang/String;")?;
    let string = vm.invoke_method(class, method, vec![Value::Ref(this)])?;
    Ok(vm
        .string_utf16(string.as_ref())
        .map_or_else(Vec::new, |units| units.to_vec()))
}

fn char_sequence_chars(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let units = char_sequence_units(vm, receiver(args))?;
    let values: Vec<i32> = units.iter().map(|unit| i32::from(*unit)).collect();
    crate::intstream::int_stream_of(vm, values)
}

fn char_sequence_code_points(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let units = char_sequence_units(vm, receiver(args))?;
    let text = String::from_utf16_lossy(&units);
    let values: Vec<i32> = text.chars().map(|ch| ch as i32).collect();
    crate::intstream::int_stream_of(vm, values)
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
    &[
        method(
            "iterator",
            "()Ljava/util/Iterator;",
            ACC_PUBLIC | ACC_ABSTRACT,
            abstract_method,
        ),
        method(
            "forEach",
            "(Ljava/util/function/Consumer;)V",
            ACC_PUBLIC | ACC_ABSTRACT,
            abstract_method,
        ),
    ],
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

fn comparator_from_function(
    vm: &mut Vm<'_>,
    function: ObjectRef,
    comparator: ObjectRef,
) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/util/Comparator$FunctionComparator")?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "function", "Ljava/util/function/Function;")?;
    vm.set_instance_ref(instance, declaring, field, function);
    let (declaring, field) = vm.find_field(class, "comparator", "Ljava/util/Comparator;")?;
    vm.set_instance_ref(instance, declaring, field, comparator);
    object(instance)
}

fn comparator_natural(vm: &mut Vm<'_>) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/util/Collections$ReverseComparator")?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "comparator", "Ljava/util/Comparator;")?;
    vm.set_instance_ref(instance, declaring, field, ObjectRef::NULL);
    object(instance)
}

const COMPARATOR_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
    method(
        "compare",
        "(Ljava/lang/Object;Ljava/lang/Object;)I",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method,
    ),
    method(
        "reversed",
        "()Ljava/util/Comparator;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method,
    ),
    method(
        "thenComparing",
        "(Ljava/util/Comparator;)Ljava/util/Comparator;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method,
    ),
    method(
        "thenComparing",
        "(Ljava/util/function/Function;)Ljava/util/Comparator;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method,
    ),
    method(
        "comparing",
        "(Ljava/util/function/Function;)Ljava/util/Comparator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| comparator_from_function(vm, ref_arg(args, 0), ObjectRef::NULL),
    ),
    method(
        "comparing",
        "(Ljava/util/function/Function;Ljava/util/Comparator;)Ljava/util/Comparator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| comparator_from_function(vm, ref_arg(args, 0), ref_arg(args, 1)),
    ),
    method(
        "comparingInt",
        "(Ljava/util/function/ToIntFunction;)Ljava/util/Comparator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| comparator_from_function(vm, ref_arg(args, 0), ObjectRef::NULL),
    ),
    method(
        "comparingLong",
        "(Ljava/util/function/ToLongFunction;)Ljava/util/Comparator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| comparator_from_function(vm, ref_arg(args, 0), ObjectRef::NULL),
    ),
    method(
        "comparingDouble",
        "(Ljava/util/function/ToDoubleFunction;)Ljava/util/Comparator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| comparator_from_function(vm, ref_arg(args, 0), ObjectRef::NULL),
    ),
    method(
        "naturalOrder",
        "()Ljava/util/Comparator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| comparator_natural(vm),
    ),
    method(
        "reverseOrder",
        "()Ljava/util/Comparator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let natural = comparator_natural(vm)?.as_ref();
            let class = vm.resolve_class("java/util/Collections$ReverseComparator")?;
            let instance = vm.new_instance(class)?;
            let (declaring, field) =
                vm.find_field(class, "comparator", "Ljava/util/Comparator;")?;
            vm.set_instance_ref(instance, declaring, field, natural);
            object(instance)
        },
    ),
];

pub(crate) const COMPARATOR: oxjvm_vm::NativeClass = class(
    "java/util/Comparator",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &COMPARATOR_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// StackTraceElement
// -------------------------------------------------------------------------------------------

const STACK_TRACE_ELEMENT_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
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
    method("isNativeMethod", "()Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
    method(
        "getModuleName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getModuleVersion",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
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

const ANNOTATION_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "annotationType",
        "()Ljava/lang/Class;",
        ACC_PUBLIC,
        |vm, _, args| {
            let instance = receiver(args);
            let class = vm.class_of(instance);
            object(vm.class_object(class)?)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "hashCode",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
];

pub(crate) const ANNOTATION: oxjvm_vm::NativeClass = class(
    "java/lang/annotation/Annotation",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &ANNOTATION_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// SwitchBootstraps (shell)
// -------------------------------------------------------------------------------------------

pub(crate) const SWITCH_BOOTSTRAPS: oxjvm_vm::NativeClass = class(
    "java/lang/runtime/SwitchBootstraps",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &[],
    None,
);

// -------------------------------------------------------------------------------------------
// ValueLayout (shell)
// -------------------------------------------------------------------------------------------

pub(crate) const VALUE_LAYOUT: oxjvm_vm::NativeClass = class(
    "java/lang/foreign/ValueLayout",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE,
    &[],
    &[],
    None,
);

// -------------------------------------------------------------------------------------------
// RandomGenerator (shell)
// -------------------------------------------------------------------------------------------

const RANDOM_GENERATOR_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method("nextInt", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let (class, method) = vm.resolve_virtual_method(this, "nextInt", "()I")?;
        vm.invoke_method(class, method, vec![Value::Ref(this)])
    }),
    method("nextInt", "(I)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let (class, method) = vm.resolve_virtual_method(this, "nextInt", "(I)I")?;
        vm.invoke_method(class, method, vec![Value::Ref(this), args[1]])
    }),
    method("nextInt", "(II)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let (class, method) = vm.resolve_virtual_method(this, "nextInt", "(II)I")?;
        vm.invoke_method(class, method, vec![Value::Ref(this), args[1], args[2]])
    }),
    method("nextLong", "()J", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let (class, method) = vm.resolve_virtual_method(this, "nextLong", "()J")?;
        vm.invoke_method(class, method, vec![Value::Ref(this)])
    }),
    method("nextDouble", "()D", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let (class, method) = vm.resolve_virtual_method(this, "nextDouble", "()D")?;
        vm.invoke_method(class, method, vec![Value::Ref(this)])
    }),
    method("nextBoolean", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let (class, method) = vm.resolve_virtual_method(this, "nextBoolean", "()Z")?;
        vm.invoke_method(class, method, vec![Value::Ref(this)])
    }),
];

pub(crate) const RANDOM_GENERATOR: oxjvm_vm::NativeClass = class(
    "java/util/random/RandomGenerator",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE,
    &[],
    &RANDOM_GENERATOR_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// ZoneId (shell)
// -------------------------------------------------------------------------------------------

const ZONE_ID_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method(
        "of",
        "(Ljava/lang/String;)Ljava/time/ZoneId;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| zone_id_new(vm, args),
    ),
    method(
        "of",
        "(Ljava/lang/String;Ljava/util/Map;)Ljava/time/ZoneId;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| zone_id_new(vm, args),
    ),
    method(
        "systemDefault",
        "()Ljava/time/ZoneId;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let id = system_property(vm, "user.timezone")
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| String::from("UTC"));
            zone_id_with(vm, &id)
        },
    ),
    method(
        "getId",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = vm
                .read_string_field(this, vm.class_of(this), "id")
                .unwrap_or_default();
            object(vm.intern(&text)?)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = vm
                .read_string_field(this, vm.class_of(this), "id")
                .unwrap_or_default();
            object(vm.intern(&text)?)
        },
    ),
    method(
        "normalized",
        "()Ljava/time/ZoneId;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() || vm.class_of(other) != vm.class_of(receiver(args)) {
                return boolean(false);
            }
            let this = receiver(args);
            boolean(
                vm.read_string_field(this, vm.class_of(this), "id")
                    == vm.read_string_field(other, vm.class_of(other), "id"),
            )
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let text = vm
            .read_string_field(this, vm.class_of(this), "id")
            .unwrap_or_default();
        let mut hash = 0i32;
        for unit in text.encode_utf16() {
            hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
        }
        int(hash)
    }),
];

const ZONE_SHORT_IDS: [(&str, &str); 29] = [
    ("ACT", "Australia/Darwin"),
    ("AET", "Australia/Sydney"),
    ("AGT", "America/Argentina/Buenos_Aires"),
    ("ART", "Africa/Cairo"),
    ("AST", "America/Anchorage"),
    ("BET", "America/Sao_Paulo"),
    ("BST", "Asia/Dhaka"),
    ("CAT", "Africa/Harare"),
    ("CNT", "America/St_Johns"),
    ("CST", "America/Chicago"),
    ("CTT", "Asia/Shanghai"),
    ("EAT", "Africa/Addis_Ababa"),
    ("ECT", "Europe/Paris"),
    ("EST", "America/New_York"),
    ("HST", "Pacific/Honolulu"),
    ("IET", "America/Indiana/Indianapolis"),
    ("IST", "Asia/Kolkata"),
    ("JST", "Asia/Tokyo"),
    ("MIT", "Pacific/Apia"),
    ("MST", "America/Denver"),
    ("NET", "Asia/Yerevan"),
    ("NST", "Pacific/Auckland"),
    ("PLT", "Asia/Karachi"),
    ("PNT", "America/Phoenix"),
    ("PRT", "America/Puerto_Rico"),
    ("PST", "America/Los_Angeles"),
    ("SST", "Pacific/Guadalcanal"),
    ("VST", "Asia/Ho_Chi_Minh"),
    ("WET", "Europe/Lisbon"),
];

fn zone_id_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let zone_class = vm.resolve_class("java/time/ZoneId")?;
    let map_class = vm.resolve_class("java/util/HashMap")?;
    let map = vm.new_instance(map_class)?;
    let (declaring, ctor) = vm
        .find_method(map_class, "<init>", "()V")
        .ok_or_else(|| VmError::internal("HashMap lacks <init>"))?;
    vm.invoke_method(declaring, ctor, vec![Value::Ref(map)])?;
    for (key, value) in ZONE_SHORT_IDS {
        let key_ref = vm.intern(key)?;
        let value_ref = vm.intern(value)?;
        let (class, method) = vm.resolve_virtual_method(
            map,
            "put",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        )?;
        vm.invoke_method(
            class,
            method,
            vec![Value::Ref(map), Value::Ref(key_ref), Value::Ref(value_ref)],
        )?;
    }
    vm.set_static_value(zone_class, "SHORT_IDS", "Ljava/util/Map;", Value::Ref(map));
    Ok(())
}

fn zone_id_new(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let mut text = string_arg(vm, args[0])?;
    if args.len() > 1 {
        let aliases = ref_arg(args, 1);
        if !aliases.is_null() {
            let key = vm.intern(&text)?;
            let (class, method) = vm.resolve_virtual_method(
                aliases,
                "get",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
            )?;
            let value =
                vm.invoke_method(class, method, vec![Value::Ref(aliases), Value::Ref(key)])?;
            if !value.as_ref().is_null() {
                text = vm.string_or_empty(value.as_ref());
            }
        }
    }
    if text.is_empty() {
        return Err(vm.throw_new(
            "java/time/DateTimeException",
            Some("Zone ID must not be empty"),
        ));
    }
    zone_id_with(vm, &text)
}

fn zone_id_with(vm: &mut Vm<'_>, text: &str) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/time/ZoneId")?;
    let instance = vm.new_instance(class)?;
    vm.set_string_field(instance, class, "id", text)?;
    object(instance)
}

pub(crate) const ZONE_ID: oxjvm_vm::NativeClass = class(
    "java/time/ZoneId",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[
        field("id", "Ljava/lang/String;", ACC_PRIVATE, None),
        field(
            "SHORT_IDS",
            "Ljava/util/Map;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &ZONE_ID_METHODS,
    Some(zone_id_clinit),
);

// -------------------------------------------------------------------------------------------
// IO (JEP 512)
// -------------------------------------------------------------------------------------------

fn system_stream(vm: &mut Vm<'_>, name: &str, descriptor: &str) -> Result<ObjectRef, VmError> {
    let system = vm.resolve_class("java/lang/System")?;
    vm.ensure_initialized(system)?;
    Ok(vm
        .read_static_ref(system, name, descriptor)
        .unwrap_or(ObjectRef::NULL))
}

fn io_readln(vm: &mut Vm<'_>) -> Result<Value, VmError> {
    let input = system_stream(vm, "in", "Ljava/io/InputStream;")?;
    let mut bytes: Vec<u8> = Vec::new();
    loop {
        let (class, method) = vm.resolve_virtual_method(input, "read", "()I")?;
        let byte = vm
            .invoke_method(class, method, vec![Value::Ref(input)])?
            .as_int();
        if byte < 0 {
            if bytes.is_empty() {
                return object(ObjectRef::NULL);
            }
            break;
        }
        match byte as u8 {
            b'\n' => break,
            b'\r' => break,
            value => bytes.push(value),
        }
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    object(vm.make_string(&text)?)
}

const IO_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "println",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let out = system_stream(vm, "out", "Ljava/io/PrintStream;")?;
            let (class, method) =
                vm.resolve_virtual_method(out, "println", "(Ljava/lang/Object;)V")?;
            vm.invoke_method(
                class,
                method,
                vec![Value::Ref(out), Value::Ref(ref_arg(args, 0))],
            )?;
            void()
        },
    ),
    method("println", "()V", ACC_PUBLIC | ACC_STATIC, |vm, _, _| {
        let out = system_stream(vm, "out", "Ljava/io/PrintStream;")?;
        let (class, method) = vm.resolve_virtual_method(out, "println", "()V")?;
        vm.invoke_method(class, method, vec![Value::Ref(out)])?;
        void()
    }),
    method(
        "print",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let out = system_stream(vm, "out", "Ljava/io/PrintStream;")?;
            let (class, method) =
                vm.resolve_virtual_method(out, "print", "(Ljava/lang/Object;)V")?;
            vm.invoke_method(
                class,
                method,
                vec![Value::Ref(out), Value::Ref(ref_arg(args, 0))],
            )?;
            void()
        },
    ),
    method(
        "readln",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| io_readln(vm),
    ),
    method(
        "readln",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let out = system_stream(vm, "out", "Ljava/io/PrintStream;")?;
            let (class, method) =
                vm.resolve_virtual_method(out, "print", "(Ljava/lang/String;)V")?;
            vm.invoke_method(
                class,
                method,
                vec![Value::Ref(out), Value::Ref(ref_arg(args, 0))],
            )?;
            io_readln(vm)
        },
    ),
    method("<init>", "()V", ACC_PRIVATE, |_, _, _| void()),
];

pub(crate) const IO: oxjvm_vm::NativeClass = class(
    "java/lang/IO",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &IO_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// DataFlavor (shell)
// -------------------------------------------------------------------------------------------

pub(crate) const DATA_FLAVOR: oxjvm_vm::NativeClass = class(
    "java/awt/datatransfer/DataFlavor",
    Some("java/lang/Object"),
    &["java/io/Externalizable"],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
