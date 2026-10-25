//! A compact `java.beans` implementation: introspection over getter/setter
//! methods, property editors, and the change-listener support classes.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::MethodDescriptor;
use oxjvm_classfile::flags::*;
use oxjvm_vm::{ClassId, ObjectRef, Value, Vm, VmError};

use crate::lang::class_argument;
use crate::{boolean, class, field, int, method, object, receiver, ref_arg, string_arg, void};

pub(crate) const EXCEPTION_LISTENER: oxjvm_vm::NativeClass = class(
    "java/beans/ExceptionListener",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[method(
        "exceptionThrown",
        "(Ljava/lang/Exception;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    )],
    None,
);

fn capitalize(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn method_param_count(descriptor: &str) -> Option<usize> {
    MethodDescriptor::parse(descriptor)
        .ok()
        .map(|parsed| parsed.parameters.len())
}

fn find_method_with_params(
    vm: &Vm<'_>,
    class: ClassId,
    name: &str,
    params: usize,
) -> Option<(ClassId, u32)> {
    let entry = vm.classes.get(class);
    for (index, candidate) in entry.methods.iter().enumerate() {
        if candidate.name == name && method_param_count(&candidate.descriptor) == Some(params) {
            return Some((class, index as u32));
        }
    }
    if let Some(super_class) = entry.super_class {
        if let Some(found) = find_method_with_params(vm, super_class, name, params) {
            return Some(found);
        }
    }
    for interface in &entry.interfaces {
        if let Some(found) = find_method_with_params(vm, *interface, name, params) {
            return Some(found);
        }
    }
    None
}

fn method_return_descriptor(vm: &Vm<'_>, class: ClassId, slot: u32) -> Option<String> {
    let descriptor = &vm.classes.get(class).methods[slot as usize].descriptor;
    MethodDescriptor::parse(descriptor).ok().and_then(|parsed| {
        parsed
            .returns
            .as_ref()
            .map(crate::reflect::field_type_descriptor)
    })
}

fn method_parameter_descriptor(vm: &Vm<'_>, class: ClassId, slot: u32) -> Option<String> {
    let descriptor = &vm.classes.get(class).methods[slot as usize].descriptor;
    MethodDescriptor::parse(descriptor).ok().and_then(|parsed| {
        parsed
            .parameters
            .first()
            .map(crate::reflect::field_type_descriptor)
    })
}

fn setup_property_descriptor(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: &str,
    bean_class: ClassId,
    read: Option<(ClassId, u32)>,
    write: Option<(ClassId, u32)>,
) -> Result<(), VmError> {
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "name", "Ljava/lang/String;")?;
    let name_text = vm.make_string(name)?;
    vm.set_instance_ref(this, declaring, field, name_text);
    let (declaring, field) = vm.find_field(class, "beanClass", "Ljava/lang/Class;")?;
    let class_object = vm.class_object(bean_class)?;
    vm.set_instance_ref(this, declaring, field, class_object);
    let display = capitalize(name);
    let (declaring, field) = vm.find_field(class, "displayName", "Ljava/lang/String;")?;
    let display_text = vm.make_string(&display)?;
    vm.set_instance_ref(this, declaring, field, display_text);
    if let Some((declaring_class, slot)) = read {
        let method = crate::reflect::new_method(vm, declaring_class, slot)?;
        let (declaring, field) =
            vm.find_field(class, "readMethod", "Ljava/lang/reflect/Method;")?;
        vm.set_instance_ref(this, declaring, field, method);
    }
    if let Some((declaring_class, slot)) = write {
        let method = crate::reflect::new_method(vm, declaring_class, slot)?;
        let (declaring, field) =
            vm.find_field(class, "writeMethod", "Ljava/lang/reflect/Method;")?;
        vm.set_instance_ref(this, declaring, field, method);
    }
    let type_descriptor = read
        .and_then(|(declaring_class, slot)| method_return_descriptor(vm, declaring_class, slot))
        .or_else(|| {
            write.and_then(|(declaring_class, slot)| {
                method_parameter_descriptor(vm, declaring_class, slot)
            })
        })
        .unwrap_or_else(|| String::from("Ljava/lang/Object;"));
    let type_class = crate::annotation::class_for_descriptor(vm, &type_descriptor)?;
    let type_object = vm.class_object(type_class)?;
    let (declaring, field) = vm.find_field(class, "propertyType", "Ljava/lang/Class;")?;
    vm.set_instance_ref(this, declaring, field, type_object);
    Ok(())
}

const PROPERTY_DESCRIPTOR_METHODS: [oxjvm_vm::NativeMethodDef; 27] = [
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/Class;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = string_arg(vm, args[1])?;
            let bean_class = class_argument(vm, ref_arg(args, 2))?;
            let getter_name = alloc::format!("get{}", capitalize(&name));
            let boolean_name = alloc::format!("is{}", capitalize(&name));
            let setter_name = alloc::format!("set{}", capitalize(&name));
            let read = find_method_with_params(vm, bean_class, &getter_name, 0)
                .or_else(|| find_method_with_params(vm, bean_class, &boolean_name, 0));
            let write = find_method_with_params(vm, bean_class, &setter_name, 1);
            setup_property_descriptor(vm, receiver(args), &name, bean_class, read, write)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/Class;Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = string_arg(vm, args[1])?;
            let bean_class = class_argument(vm, ref_arg(args, 2))?;
            let getter = string_arg(vm, args[3]).unwrap_or_default();
            let setter = string_arg(vm, args[4]).unwrap_or_default();
            let read = if getter.is_empty() {
                None
            } else {
                find_method_with_params(vm, bean_class, &getter, 0)
            };
            let write = if setter.is_empty() {
                None
            } else {
                find_method_with_params(vm, bean_class, &setter, 1)
            };
            setup_property_descriptor(vm, receiver(args), &name, bean_class, read, write)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/Class;Ljava/lang/reflect/Method;Ljava/lang/reflect/Method;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = string_arg(vm, args[1])?;
            let bean_class = class_argument(vm, ref_arg(args, 2))?;
            let read = method_parts(vm, ref_arg(args, 3));
            let write = method_parts(vm, ref_arg(args, 4));
            setup_property_descriptor(vm, receiver(args), &name, bean_class, read, write)?;
            let class = vm.class_of(receiver(args));
            if let Some((declaring, field)) =
                vm.find_field_opt(class, "readMethod", "Ljava/lang/reflect/Method;")
            {
                vm.set_instance_ref(receiver(args), declaring, field, ref_arg(args, 3));
            }
            if let Some((declaring, field)) =
                vm.find_field_opt(class, "writeMethod", "Ljava/lang/reflect/Method;")
            {
                vm.set_instance_ref(receiver(args), declaring, field, ref_arg(args, 4));
            }
            void()
        },
    ),
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| property_string(vm, receiver(args), "name"),
    ),
    method(
        "getDisplayName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| property_string(vm, receiver(args), "displayName"),
    ),
    method(
        "getShortDescription",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| property_string(vm, receiver(args), "shortDescription"),
    ),
    method(
        "getPropertyType",
        "()Ljava/lang/Class;",
        ACC_PUBLIC,
        |vm, _, args| property_ref(vm, receiver(args), "propertyType", "Ljava/lang/Class;"),
    ),
    method(
        "getReadMethod",
        "()Ljava/lang/reflect/Method;",
        ACC_PUBLIC,
        |vm, _, args| {
            property_ref(
                vm,
                receiver(args),
                "readMethod",
                "Ljava/lang/reflect/Method;",
            )
        },
    ),
    method(
        "getWriteMethod",
        "()Ljava/lang/reflect/Method;",
        ACC_PUBLIC,
        |vm, _, args| {
            property_ref(
                vm,
                receiver(args),
                "writeMethod",
                "Ljava/lang/reflect/Method;",
            )
        },
    ),
    method(
        "setReadMethod",
        "(Ljava/lang/reflect/Method;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            set_property_ref(
                vm,
                receiver(args),
                "readMethod",
                "Ljava/lang/reflect/Method;",
                ref_arg(args, 1),
            );
            void()
        },
    ),
    method(
        "setWriteMethod",
        "(Ljava/lang/reflect/Method;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            set_property_ref(
                vm,
                receiver(args),
                "writeMethod",
                "Ljava/lang/reflect/Method;",
                ref_arg(args, 1),
            );
            void()
        },
    ),
    method("isBound", "()Z", ACC_PUBLIC, |vm, _, args| {
        property_flag(vm, receiver(args), 0)
    }),
    method("isConstrained", "()Z", ACC_PUBLIC, |vm, _, args| {
        property_flag(vm, receiver(args), 1)
    }),
    method("isExpert", "()Z", ACC_PUBLIC, |vm, _, args| {
        property_flag(vm, receiver(args), 2)
    }),
    method("isHidden", "()Z", ACC_PUBLIC, |vm, _, args| {
        property_flag(vm, receiver(args), 3)
    }),
    method("isPreferred", "()Z", ACC_PUBLIC, |vm, _, args| {
        property_flag(vm, receiver(args), 4)
    }),
    method("setBound", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        set_property_flag(vm, receiver(args), 0, crate::int_arg(args, 1) != 0);
        void()
    }),
    method("setConstrained", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        set_property_flag(vm, receiver(args), 1, crate::int_arg(args, 1) != 0);
        void()
    }),
    method("setExpert", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        set_property_flag(vm, receiver(args), 2, crate::int_arg(args, 1) != 0);
        void()
    }),
    method("setHidden", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        set_property_flag(vm, receiver(args), 3, crate::int_arg(args, 1) != 0);
        void()
    }),
    method("setPreferred", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        set_property_flag(vm, receiver(args), 4, crate::int_arg(args, 1) != 0);
        void()
    }),
    method(
        "setDisplayName",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            set_property_ref(
                vm,
                receiver(args),
                "displayName",
                "Ljava/lang/String;",
                ref_arg(args, 1),
            );
            void()
        },
    ),
    method(
        "setShortDescription",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            set_property_ref(
                vm,
                receiver(args),
                "shortDescription",
                "Ljava/lang/String;",
                ref_arg(args, 1),
            );
            void()
        },
    ),
    method(
        "getValue",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = property_value_map(vm, receiver(args))?;
            let key = ref_arg(args, 1);
            let (class, method) =
                vm.resolve_virtual_method(map, "get", "(Ljava/lang/Object;)Ljava/lang/Object;")?;
            vm.invoke_method(class, method, vec![Value::Ref(map), Value::Ref(key)])
        },
    ),
    method(
        "setValue",
        "(Ljava/lang/String;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = property_value_map(vm, receiver(args))?;
            let key = ref_arg(args, 1);
            let value = ref_arg(args, 2);
            let (class, method) = vm.resolve_virtual_method(
                map,
                "put",
                "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            )?;
            vm.invoke_method(
                class,
                method,
                vec![Value::Ref(map), Value::Ref(key), Value::Ref(value)],
            )?;
            void()
        },
    ),
    method(
        "createPropertyEditor",
        "(Ljava/lang/Object;)Ljava/beans/PropertyEditor;",
        ACC_PUBLIC,
        |vm, _, args| {
            let type_object =
                property_ref(vm, receiver(args), "propertyType", "Ljava/lang/Class;")?.as_ref();
            property_editor_for(vm, type_object)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = property_string(vm, receiver(args), "name")?;
            let text = vm.string_or_empty(name.as_ref());
            object(vm.make_string(&text)?)
        },
    ),
];

fn method_parts(vm: &Vm<'_>, method: ObjectRef) -> Option<(ClassId, u32)> {
    if method.is_null() {
        return None;
    }
    let class_object =
        vm.read_ref_field(method, vm.class_of(method), "clazz", "Ljava/lang/Class;")?;
    let class = crate::reflect::class_object_target(vm, class_object);
    let slot = crate::lang::read_int_field(vm, method, "slot").max(0) as u32;
    Some((class, slot))
}

fn property_string(vm: &mut Vm<'_>, this: ObjectRef, name: &str) -> Result<Value, VmError> {
    let value = vm
        .read_string_field(this, vm.class_of(this), name)
        .unwrap_or_default();
    object(vm.make_string(&value)?)
}

fn property_ref(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: &str,
    descriptor: &str,
) -> Result<Value, VmError> {
    object(
        vm.read_ref_field(this, vm.class_of(this), name, descriptor)
            .unwrap_or(ObjectRef::NULL),
    )
}

fn set_property_ref(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: &str,
    descriptor: &str,
    value: ObjectRef,
) {
    if let Some((declaring, field)) = vm.find_field_opt(vm.class_of(this), name, descriptor) {
        vm.set_instance_ref(this, declaring, field, value);
    }
}

fn property_flag(vm: &Vm<'_>, this: ObjectRef, bit: i32) -> Result<Value, VmError> {
    boolean((crate::lang::read_int_field(vm, this, "flags") & (1 << bit)) != 0)
}

fn set_property_flag(vm: &mut Vm<'_>, this: ObjectRef, bit: i32, value: bool) {
    let mut flags = crate::lang::read_int_field(vm, this, "flags");
    if value {
        flags |= 1 << bit;
    } else {
        flags &= !(1 << bit);
    }
    crate::lang::write_int_field(vm, this, "flags", flags);
}

fn property_value_map(vm: &mut Vm<'_>, this: ObjectRef) -> Result<ObjectRef, VmError> {
    if let Some(map) = vm.read_ref_field(this, vm.class_of(this), "value", "Ljava/util/HashMap;") {
        if !map.is_null() {
            return Ok(map);
        }
    }
    let map = crate::util::hash_map_new(vm)?;
    let (declaring, field) = vm.find_field(vm.class_of(this), "value", "Ljava/util/HashMap;")?;
    vm.set_instance_ref(this, declaring, field, map);
    Ok(map)
}

pub(crate) const PROPERTY_DESCRIPTOR: oxjvm_vm::NativeClass = class(
    "java/beans/PropertyDescriptor",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("beanClass", "Ljava/lang/Class;", ACC_PRIVATE, None),
        field(
            "readMethod",
            "Ljava/lang/reflect/Method;",
            ACC_PRIVATE,
            None,
        ),
        field(
            "writeMethod",
            "Ljava/lang/reflect/Method;",
            ACC_PRIVATE,
            None,
        ),
        field("propertyType", "Ljava/lang/Class;", ACC_PRIVATE, None),
        field("displayName", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("shortDescription", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("flags", "I", ACC_PRIVATE, None),
        field("value", "Ljava/util/HashMap;", ACC_PRIVATE, None),
    ],
    &PROPERTY_DESCRIPTOR_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// PropertyEditor / PropertyEditorSupport / PropertyEditorManager
// -------------------------------------------------------------------------------------------

const PROPERTY_EDITOR_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method(
        "setValue",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getValue",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "setAsText",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getAsText",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getTags",
        "()[Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "isPaintable",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getCustomEditor",
        "()Ljava/awt/Component;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "supportsCustomEditor",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "addPropertyChangeListener",
        "(Ljava/beans/PropertyChangeListener;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "removePropertyChangeListener",
        "(Ljava/beans/PropertyChangeListener;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const PROPERTY_EDITOR: oxjvm_vm::NativeClass = class(
    "java/beans/PropertyEditor",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &PROPERTY_EDITOR_METHODS,
    None,
);

const PROPERTY_EDITOR_SUPPORT_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "setValue",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            if let Some((declaring, field)) =
                vm.find_field_opt(vm.class_of(receiver(args)), "value", "Ljava/lang/Object;")
            {
                vm.set_instance_ref(receiver(args), declaring, field, ref_arg(args, 1));
            }
            void()
        },
    ),
    method(
        "getValue",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(
                vm.read_ref_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "value",
                    "Ljava/lang/Object;",
                )
                .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method(
        "setAsText",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = ref_arg(args, 1);
            if let Some((declaring, field)) =
                vm.find_field_opt(vm.class_of(receiver(args)), "value", "Ljava/lang/Object;")
            {
                vm.set_instance_ref(receiver(args), declaring, field, text);
            }
            void()
        },
    ),
    method(
        "getAsText",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = vm
                .read_ref_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "value",
                    "Ljava/lang/Object;",
                )
                .unwrap_or(ObjectRef::NULL);
            if value.is_null() {
                return object(vm.make_string("")?);
            }
            let (class, method) =
                vm.resolve_virtual_method(value, "toString", "()Ljava/lang/String;")?;
            vm.invoke_method(class, method, vec![Value::Ref(value)])
        },
    ),
    method("getTags", "()[Ljava/lang/String;", ACC_PUBLIC, |_, _, _| {
        object(ObjectRef::NULL)
    }),
    method("isPaintable", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method(
        "getCustomEditor",
        "()Ljava/awt/Component;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method("supportsCustomEditor", "()Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
    method(
        "addPropertyChangeListener",
        "(Ljava/beans/PropertyChangeListener;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "removePropertyChangeListener",
        "(Ljava/beans/PropertyChangeListener;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
];

pub(crate) const PROPERTY_EDITOR_SUPPORT: oxjvm_vm::NativeClass = class(
    "java/beans/PropertyEditorSupport",
    Some("java/lang/Object"),
    &["java/beans/PropertyEditor"],
    ACC_PUBLIC,
    &[field("value", "Ljava/lang/Object;", ACC_PRIVATE, None)],
    &PROPERTY_EDITOR_SUPPORT_METHODS,
    None,
);

fn property_editor_for(vm: &mut Vm<'_>, type_object: ObjectRef) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/beans/OjvmPropertyEditor")?;
    let editor = vm.new_instance(class)?;
    if let Some((declaring, field)) = vm.find_field_opt(class, "type", "Ljava/lang/Class;") {
        vm.set_instance_ref(editor, declaring, field, type_object);
    }
    object(editor)
}

const OJVM_PROPERTY_EDITOR_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "setAsText",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 1));
            let type_object = vm
                .read_ref_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "type",
                    "Ljava/lang/Class;",
                )
                .unwrap_or(ObjectRef::NULL);
            let type_name = if type_object.is_null() {
                String::new()
            } else {
                let class = class_argument(vm, type_object)?;
                vm.classes.get(class).name.clone()
            };
            let value = if type_name == "java/lang/Integer" {
                match text.parse::<i32>() {
                    Ok(value) => crate::boxed::box_value(vm, Value::Int(value), "I")?.as_ref(),
                    Err(_) => ObjectRef::NULL,
                }
            } else if type_name == "java/lang/Long" {
                match text.parse::<i64>() {
                    Ok(value) => crate::boxed::box_value(vm, Value::Long(value), "J")?.as_ref(),
                    Err(_) => ObjectRef::NULL,
                }
            } else if type_name == "java/lang/Double" {
                match text.parse::<f64>() {
                    Ok(value) => {
                        crate::boxed::box_value(vm, Value::Long(value.to_bits() as i64), "D")?
                            .as_ref()
                    }
                    Err(_) => ObjectRef::NULL,
                }
            } else if type_name == "java/lang/Boolean" {
                crate::boxed::box_value(vm, Value::Int(i32::from(text == "true")), "Z")?.as_ref()
            } else {
                ref_arg(args, 1)
            };
            if let Some((declaring, field)) =
                vm.find_field_opt(vm.class_of(receiver(args)), "value", "Ljava/lang/Object;")
            {
                vm.set_instance_ref(receiver(args), declaring, field, value);
            }
            void()
        },
    ),
    method(
        "getAsText",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = vm
                .read_ref_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "value",
                    "Ljava/lang/Object;",
                )
                .unwrap_or(ObjectRef::NULL);
            if value.is_null() {
                return object(vm.make_string("")?);
            }
            let (class, method) =
                vm.resolve_virtual_method(value, "toString", "()Ljava/lang/String;")?;
            vm.invoke_method(class, method, vec![Value::Ref(value)])
        },
    ),
];

pub(crate) const OJVM_PROPERTY_EDITOR: oxjvm_vm::NativeClass = class(
    "java/beans/OjvmPropertyEditor",
    Some("java/beans/PropertyEditorSupport"),
    &["java/beans/PropertyEditor"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field("type", "Ljava/lang/Class;", ACC_PRIVATE, None)],
    &OJVM_PROPERTY_EDITOR_METHODS,
    None,
);

const PROPERTY_EDITOR_MANAGER_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "findEditor",
        "(Ljava/lang/Class;)Ljava/beans/PropertyEditor;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| property_editor_for(vm, ref_arg(args, 0)),
    ),
    method(
        "registerEditor",
        "(Ljava/lang/Class;Ljava/lang/Class;)V",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| void(),
    ),
    method(
        "getEditorSearchPath",
        "()[Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/lang/String")?;
            object(vm.allocate_object_array(class, 0)?)
        },
    ),
    method(
        "setEditorSearchPath",
        "([Ljava/lang/String;)V",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| void(),
    ),
    method(
        "findEditor",
        "(Ljava/lang/Class;Ljava/lang/ClassLoader;)Ljava/beans/PropertyEditor;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| property_editor_for(vm, ref_arg(args, 0)),
    ),
    method(
        "setOwner",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| void(),
    ),
];

pub(crate) const PROPERTY_EDITOR_MANAGER: oxjvm_vm::NativeClass = class(
    "java/beans/PropertyEditorManager",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &PROPERTY_EDITOR_MANAGER_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Introspector / BeanInfo
// -------------------------------------------------------------------------------------------

const BEAN_INFO_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method(
        "getBeanDescriptor",
        "()Ljava/beans/BeanDescriptor;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getEventSetDescriptors",
        "()[Ljava/beans/EventSetDescriptor;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getDefaultEventIndex",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getPropertyDescriptors",
        "()[Ljava/beans/PropertyDescriptor;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getDefaultPropertyIndex",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getMethodDescriptors",
        "()[Ljava/beans/MethodDescriptor;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getAdditionalBeanInfo",
        "()[Ljava/beans/BeanInfo;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getIcon",
        "(I)Ljava/awt/Image;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const BEAN_INFO: oxjvm_vm::NativeClass = class(
    "java/beans/BeanInfo",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &BEAN_INFO_METHODS,
    None,
);

fn bean_property_descriptors(vm: &mut Vm<'_>, bean_class: ClassId) -> Result<ObjectRef, VmError> {
    let descriptor_class = vm.resolve_class("java/beans/PropertyDescriptor")?;
    let mut properties: Vec<String> = Vec::new();
    let mut class = Some(bean_class);
    while let Some(current) = class {
        for candidate in &vm.classes.get(current).methods {
            let name = candidate.name.clone();
            let property = if let Some(rest) = name.strip_prefix("get") {
                if method_param_count(&candidate.descriptor) == Some(0) && !rest.is_empty() {
                    Some(rest.to_string())
                } else {
                    None
                }
            } else if let Some(rest) = name.strip_prefix("is") {
                if method_param_count(&candidate.descriptor) == Some(0) && !rest.is_empty() {
                    Some(rest.to_string())
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(property) = property {
                let property = property[..1].to_lowercase() + &property[1..];
                if !properties.contains(&property) {
                    properties.push(property);
                }
            }
        }
        class = vm.classes.get(current).super_class;
    }
    let array = vm.allocate_object_array(descriptor_class, properties.len())?;
    for (index, property) in properties.iter().enumerate() {
        let descriptor = vm.new_instance(descriptor_class)?;
        let (declaring, ctor) = vm
            .find_method(
                descriptor_class,
                "<init>",
                "(Ljava/lang/String;Ljava/lang/Class;)V",
            )
            .ok_or_else(|| VmError::internal("PropertyDescriptor ctor missing"))?;
        let name = vm.make_string(property)?;
        let class_object = vm.class_object(bean_class)?;
        vm.invoke_method(
            declaring,
            ctor,
            vec![
                Value::Ref(descriptor),
                Value::Ref(name),
                Value::Ref(class_object),
            ],
        )?;
        vm.array_set_ref(array, index, descriptor)?;
    }
    Ok(array)
}

const OJVM_BEAN_INFO_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method(
        "getBeanDescriptor",
        "()Ljava/beans/BeanDescriptor;",
        ACC_PUBLIC,
        |vm, _, args| {
            let bean_class = bean_info_class(vm, receiver(args))?;
            let descriptor = bean_descriptor(vm, bean_class)?;
            object(descriptor)
        },
    ),
    method(
        "getEventSetDescriptors",
        "()[Ljava/beans/EventSetDescriptor;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/beans/EventSetDescriptor")?;
            object(vm.allocate_object_array(class, 0)?)
        },
    ),
    method("getDefaultEventIndex", "()I", ACC_PUBLIC, |_, _, _| int(-1)),
    method(
        "getPropertyDescriptors",
        "()[Ljava/beans/PropertyDescriptor;",
        ACC_PUBLIC,
        |vm, _, args| {
            let bean_class = bean_info_class(vm, receiver(args))?;
            object(bean_property_descriptors(vm, bean_class)?)
        },
    ),
    method("getDefaultPropertyIndex", "()I", ACC_PUBLIC, |_, _, _| {
        int(-1)
    }),
    method(
        "getMethodDescriptors",
        "()[Ljava/beans/MethodDescriptor;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/beans/MethodDescriptor")?;
            object(vm.allocate_object_array(class, 0)?)
        },
    ),
    method(
        "getAdditionalBeanInfo",
        "()[Ljava/beans/BeanInfo;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/beans/BeanInfo")?;
            object(vm.allocate_object_array(class, 0)?)
        },
    ),
    method("getIcon", "(I)Ljava/awt/Image;", ACC_PUBLIC, |_, _, _| {
        object(ObjectRef::NULL)
    }),
];

fn bean_info_class(vm: &mut Vm<'_>, info: ObjectRef) -> Result<ClassId, VmError> {
    let class_object = vm
        .read_ref_field(info, vm.class_of(info), "beanClass", "Ljava/lang/Class;")
        .unwrap_or(ObjectRef::NULL);
    class_argument(vm, class_object)
}

fn bean_descriptor(vm: &mut Vm<'_>, bean_class: ClassId) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/beans/BeanDescriptor")?;
    let descriptor = vm.new_instance(class)?;
    let class_object = vm.class_object(bean_class)?;
    if let Some((declaring, field)) = vm.find_field_opt(class, "beanClass", "Ljava/lang/Class;") {
        vm.set_instance_ref(descriptor, declaring, field, class_object);
    }
    Ok(descriptor)
}

pub(crate) const OJVM_BEAN_INFO: oxjvm_vm::NativeClass = class(
    "java/beans/OjvmBeanInfo",
    Some("java/lang/Object"),
    &["java/beans/BeanInfo"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field("beanClass", "Ljava/lang/Class;", ACC_PRIVATE, None)],
    &OJVM_BEAN_INFO_METHODS,
    None,
);

const INTROSPECTOR_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "getBeanInfo",
        "(Ljava/lang/Class;)Ljava/beans/BeanInfo;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| introspector_bean_info(vm, ref_arg(args, 0)),
    ),
    method(
        "getBeanInfo",
        "(Ljava/lang/Class;I)Ljava/beans/BeanInfo;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| introspector_bean_info(vm, ref_arg(args, 0)),
    ),
    method(
        "getBeanInfo",
        "(Ljava/lang/Class;Ljava/lang/Class;)Ljava/beans/BeanInfo;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| introspector_bean_info(vm, ref_arg(args, 0)),
    ),
    method("flushCaches", "()V", ACC_PUBLIC | ACC_STATIC, |_, _, _| {
        void()
    }),
    method(
        "getBeanInfo",
        "(Ljava/lang/Class;Ljava/lang/Class;I)Ljava/beans/BeanInfo;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| introspector_bean_info(vm, ref_arg(args, 0)),
    ),
];

fn introspector_bean_info(vm: &mut Vm<'_>, class_object: ObjectRef) -> Result<Value, VmError> {
    let bean_class = class_argument(vm, class_object)?;
    let class = vm.resolve_class("java/beans/OjvmBeanInfo")?;
    let info = vm.new_instance(class)?;
    let class_object = vm.class_object(bean_class)?;
    if let Some((declaring, field)) = vm.find_field_opt(class, "beanClass", "Ljava/lang/Class;") {
        vm.set_instance_ref(info, declaring, field, class_object);
    }
    object(info)
}

pub(crate) const INTROSPECTOR: oxjvm_vm::NativeClass = class(
    "java/beans/Introspector",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &INTROSPECTOR_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Change support
// -------------------------------------------------------------------------------------------

const PROPERTY_CHANGE_EVENT_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method(
        "<init>",
        "(Ljava/lang/Object;Ljava/lang/String;Ljava/lang/Object;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = vm.class_of(receiver(args));
            for (name, descriptor, value) in [
                ("source", "Ljava/lang/Object;", ref_arg(args, 1)),
                ("propertyName", "Ljava/lang/String;", ref_arg(args, 2)),
                ("oldValue", "Ljava/lang/Object;", ref_arg(args, 3)),
                ("newValue", "Ljava/lang/Object;", ref_arg(args, 4)),
            ] {
                if let Some((declaring, field)) = vm.find_field_opt(class, name, descriptor) {
                    vm.set_instance_ref(receiver(args), declaring, field, value);
                }
            }
            void()
        },
    ),
    method(
        "getSource",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(
                vm.read_ref_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "source",
                    "Ljava/lang/Object;",
                )
                .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method(
        "getPropertyName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(
                vm.read_ref_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "propertyName",
                    "Ljava/lang/String;",
                )
                .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method(
        "getOldValue",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(
                vm.read_ref_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "oldValue",
                    "Ljava/lang/Object;",
                )
                .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method(
        "getNewValue",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(
                vm.read_ref_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "newValue",
                    "Ljava/lang/Object;",
                )
                .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = vm
                .read_string_field(receiver(args), vm.class_of(receiver(args)), "propertyName")
                .unwrap_or_default();
            object(vm.make_string(&alloc::format!("{name}: {} -> {}", "old", "new"))?)
        },
    ),
    method(
        "getPropertyChangeEvent",
        "()Ljava/beans/PropertyChangeEvent;",
        ACC_PUBLIC,
        |_vm, _, args| object(receiver(args)),
    ),
    method(
        "getPropagationId",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "setPropagationId",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
];

pub(crate) const PROPERTY_CHANGE_EVENT: oxjvm_vm::NativeClass = class(
    "java/beans/PropertyChangeEvent",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("source", "Ljava/lang/Object;", ACC_PRIVATE, None),
        field("propertyName", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("oldValue", "Ljava/lang/Object;", ACC_PRIVATE, None),
        field("newValue", "Ljava/lang/Object;", ACC_PRIVATE, None),
    ],
    &PROPERTY_CHANGE_EVENT_METHODS,
    None,
);

pub(crate) const PROPERTY_CHANGE_LISTENER: oxjvm_vm::NativeClass = class(
    "java/beans/PropertyChangeListener",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[method(
        "propertyChange",
        "(Ljava/beans/PropertyChangeEvent;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    )],
    None,
);

pub(crate) const VETOABLE_CHANGE_LISTENER: oxjvm_vm::NativeClass = class(
    "java/beans/VetoableChangeListener",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[method(
        "vetoableChange",
        "(Ljava/beans/PropertyChangeEvent;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    )],
    None,
);

const PROPERTY_CHANGE_SUPPORT_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method(
        "<init>",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = vm.class_of(receiver(args));
            if let Some((declaring, field)) =
                vm.find_field_opt(class, "source", "Ljava/lang/Object;")
            {
                vm.set_instance_ref(receiver(args), declaring, field, ref_arg(args, 1));
            }
            let list = crate::util::array_list_from(vm, &[])?;
            let (declaring, field) = vm.find_field(class, "listeners", "Ljava/util/ArrayList;")?;
            vm.set_instance_ref(receiver(args), declaring, field, list);
            void()
        },
    ),
    method(
        "addPropertyChangeListener",
        "(Ljava/beans/PropertyChangeListener;)V",
        ACC_PUBLIC,
        |vm, _, args| change_add(vm, receiver(args), ref_arg(args, 1), "listeners"),
    ),
    method(
        "addPropertyChangeListener",
        "(Ljava/lang/String;Ljava/beans/PropertyChangeListener;)V",
        ACC_PUBLIC,
        |vm, _, args| change_add(vm, receiver(args), ref_arg(args, 2), "listeners"),
    ),
    method(
        "removePropertyChangeListener",
        "(Ljava/beans/PropertyChangeListener;)V",
        ACC_PUBLIC,
        |vm, _, args| change_remove(vm, receiver(args), ref_arg(args, 1), "listeners"),
    ),
    method(
        "getPropertyChangeListeners",
        "()[Ljava/beans/PropertyChangeListener;",
        ACC_PUBLIC,
        |vm, _, args| {
            change_listeners(
                vm,
                receiver(args),
                "listeners",
                "java/beans/PropertyChangeListener",
            )
        },
    ),
    method(
        "firePropertyChange",
        "(Ljava/beans/PropertyChangeEvent;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            change_fire(
                vm,
                receiver(args),
                ref_arg(args, 1),
                "listeners",
                "propertyChange",
            )
        },
    ),
    method(
        "firePropertyChange",
        "(Ljava/lang/String;Ljava/lang/Object;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let event = make_change_event(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                ref_arg(args, 3),
            )?;
            change_fire(vm, receiver(args), event, "listeners", "propertyChange")
        },
    ),
    method(
        "firePropertyChange",
        "(Ljava/lang/String;ZZ)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let old = vm.make_string(if crate::int_arg(args, 2) != 0 {
                "true"
            } else {
                "false"
            })?;
            let new = vm.make_string(if crate::int_arg(args, 3) != 0 {
                "true"
            } else {
                "false"
            })?;
            let event = make_change_event(vm, receiver(args), ref_arg(args, 1), old, new)?;
            change_fire(vm, receiver(args), event, "listeners", "propertyChange")
        },
    ),
    method(
        "firePropertyChange",
        "(Ljava/lang/String;II)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let old = crate::boxed::box_value(vm, Value::Int(crate::int_arg(args, 2)), "I")?;
            let new = crate::boxed::box_value(vm, Value::Int(crate::int_arg(args, 3)), "I")?;
            let event = make_change_event(
                vm,
                receiver(args),
                ref_arg(args, 1),
                old.as_ref(),
                new.as_ref(),
            )?;
            change_fire(vm, receiver(args), event, "listeners", "propertyChange")
        },
    ),
];

fn change_add(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    listener: ObjectRef,
    field_name: &str,
) -> Result<Value, VmError> {
    let list = vm
        .read_ref_field(this, vm.class_of(this), field_name, "Ljava/util/ArrayList;")
        .unwrap_or(ObjectRef::NULL);
    if !list.is_null() {
        let (class, method) = vm.resolve_virtual_method(list, "add", "(Ljava/lang/Object;)Z")?;
        vm.invoke_method(class, method, vec![Value::Ref(list), Value::Ref(listener)])?;
    }
    void()
}

fn change_remove(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    listener: ObjectRef,
    field_name: &str,
) -> Result<Value, VmError> {
    let list = vm
        .read_ref_field(this, vm.class_of(this), field_name, "Ljava/util/ArrayList;")
        .unwrap_or(ObjectRef::NULL);
    if !list.is_null() {
        let (class, method) = vm.resolve_virtual_method(list, "remove", "(Ljava/lang/Object;)Z")?;
        vm.invoke_method(class, method, vec![Value::Ref(list), Value::Ref(listener)])?;
    }
    void()
}

fn change_listeners(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    field_name: &str,
    component: &str,
) -> Result<Value, VmError> {
    let list = vm
        .read_ref_field(this, vm.class_of(this), field_name, "Ljava/util/ArrayList;")
        .unwrap_or(ObjectRef::NULL);
    let class = vm.resolve_class(component)?;
    let elements = if list.is_null() {
        Vec::new()
    } else {
        crate::util::array_list_elements(vm, list)
    };
    let array = vm.allocate_object_array(class, elements.len())?;
    for (index, element) in elements.iter().enumerate() {
        vm.array_set_ref(array, index, *element)?;
    }
    object(array)
}

fn change_fire(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    event: ObjectRef,
    field_name: &str,
    method_name: &str,
) -> Result<Value, VmError> {
    let list = vm
        .read_ref_field(this, vm.class_of(this), field_name, "Ljava/util/ArrayList;")
        .unwrap_or(ObjectRef::NULL);
    if !list.is_null() {
        for listener in crate::util::array_list_elements(vm, list) {
            let (class, method) = vm.resolve_virtual_method(
                listener,
                method_name,
                "(Ljava/beans/PropertyChangeEvent;)V",
            )?;
            vm.invoke_method(class, method, vec![Value::Ref(listener), Value::Ref(event)])?;
        }
    }
    void()
}

fn make_change_event(
    vm: &mut Vm<'_>,
    source: ObjectRef,
    name: ObjectRef,
    old: ObjectRef,
    new: ObjectRef,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/beans/PropertyChangeEvent")?;
    let event = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(
            class,
            "<init>",
            "(Ljava/lang/Object;Ljava/lang/String;Ljava/lang/Object;Ljava/lang/Object;)V",
        )
        .ok_or_else(|| VmError::internal("PropertyChangeEvent ctor missing"))?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![
            Value::Ref(event),
            Value::Ref(source),
            Value::Ref(name),
            Value::Ref(old),
            Value::Ref(new),
        ],
    )?;
    Ok(event)
}

pub(crate) const PROPERTY_CHANGE_SUPPORT: oxjvm_vm::NativeClass = class(
    "java/beans/PropertyChangeSupport",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[
        field("source", "Ljava/lang/Object;", ACC_PRIVATE, None),
        field("listeners", "Ljava/util/ArrayList;", ACC_PRIVATE, None),
    ],
    &PROPERTY_CHANGE_SUPPORT_METHODS,
    None,
);

const VETOABLE_CHANGE_SUPPORT_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "<init>",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = vm.class_of(receiver(args));
            if let Some((declaring, field)) =
                vm.find_field_opt(class, "source", "Ljava/lang/Object;")
            {
                vm.set_instance_ref(receiver(args), declaring, field, ref_arg(args, 1));
            }
            let list = crate::util::array_list_from(vm, &[])?;
            let (declaring, field) = vm.find_field(class, "listeners", "Ljava/util/ArrayList;")?;
            vm.set_instance_ref(receiver(args), declaring, field, list);
            void()
        },
    ),
    method(
        "addVetoableChangeListener",
        "(Ljava/beans/VetoableChangeListener;)V",
        ACC_PUBLIC,
        |vm, _, args| change_add(vm, receiver(args), ref_arg(args, 1), "listeners"),
    ),
    method(
        "addVetoableChangeListener",
        "(Ljava/lang/String;Ljava/beans/VetoableChangeListener;)V",
        ACC_PUBLIC,
        |vm, _, args| change_add(vm, receiver(args), ref_arg(args, 2), "listeners"),
    ),
    method(
        "removeVetoableChangeListener",
        "(Ljava/beans/VetoableChangeListener;)V",
        ACC_PUBLIC,
        |vm, _, args| change_remove(vm, receiver(args), ref_arg(args, 1), "listeners"),
    ),
    method(
        "fireVetoableChange",
        "(Ljava/beans/PropertyChangeEvent;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            change_fire(
                vm,
                receiver(args),
                ref_arg(args, 1),
                "listeners",
                "vetoableChange",
            )
        },
    ),
    method(
        "fireVetoableChange",
        "(Ljava/lang/String;Ljava/lang/Object;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let event = make_change_event(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                ref_arg(args, 3),
            )?;
            change_fire(vm, receiver(args), event, "listeners", "vetoableChange")
        },
    ),
    method(
        "getVetoableChangeListeners",
        "()[Ljava/beans/VetoableChangeListener;",
        ACC_PUBLIC,
        |vm, _, args| {
            change_listeners(
                vm,
                receiver(args),
                "listeners",
                "java/beans/VetoableChangeListener",
            )
        },
    ),
];

pub(crate) const VETOABLE_CHANGE_SUPPORT: oxjvm_vm::NativeClass = class(
    "java/beans/VetoableChangeSupport",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[
        field("source", "Ljava/lang/Object;", ACC_PRIVATE, None),
        field("listeners", "Ljava/util/ArrayList;", ACC_PRIVATE, None),
    ],
    &VETOABLE_CHANGE_SUPPORT_METHODS,
    None,
);

pub(crate) const PROPERTY_VETO_EXCEPTION: oxjvm_vm::NativeClass = class(
    "java/beans/PropertyVetoException",
    Some("java/lang/Exception"),
    &[],
    ACC_PUBLIC,
    &[field("message", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &[
        method(
            "<init>",
            "(Ljava/lang/String;Ljava/beans/PropertyChangeEvent;)V",
            ACC_PUBLIC,
            |vm, _, args| {
                let class = vm.class_of(receiver(args));
                if let Some((declaring, field)) =
                    vm.find_field_opt(class, "message", "Ljava/lang/String;")
                {
                    vm.set_instance_ref(receiver(args), declaring, field, ref_arg(args, 1));
                }
                vm.capture_backtrace(receiver(args));
                void()
            },
        ),
        method(
            "getPropertyChangeEvent",
            "()Ljava/beans/PropertyChangeEvent;",
            ACC_PUBLIC,
            |_, _, _| object(ObjectRef::NULL),
        ),
    ],
    None,
);

// -------------------------------------------------------------------------------------------
// BeanDescriptor / MethodDescriptor / EventSetDescriptor / SimpleBeanInfo / Beans
// -------------------------------------------------------------------------------------------

const BEAN_DESCRIPTOR_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "<init>",
        "(Ljava/lang/Class;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = vm.class_of(receiver(args));
            if let Some((declaring, field)) =
                vm.find_field_opt(class, "beanClass", "Ljava/lang/Class;")
            {
                vm.set_instance_ref(receiver(args), declaring, field, ref_arg(args, 1));
            }
            void()
        },
    ),
    method(
        "getBeanClass",
        "()Ljava/lang/Class;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(
                vm.read_ref_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "beanClass",
                    "Ljava/lang/Class;",
                )
                .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method(
        "getCustomizerClass",
        "()Ljava/lang/Class;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class_object = vm
                .read_ref_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "beanClass",
                    "Ljava/lang/Class;",
                )
                .unwrap_or(ObjectRef::NULL);
            if class_object.is_null() {
                return object(vm.make_string("")?);
            }
            let class = class_argument(vm, class_object)?;
            let name = vm.classes.get(class).name.clone();
            object(vm.make_string(&name)?)
        },
    ),
    method(
        "getDisplayName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class_object = vm
                .read_ref_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "beanClass",
                    "Ljava/lang/Class;",
                )
                .unwrap_or(ObjectRef::NULL);
            if class_object.is_null() {
                return object(vm.make_string("")?);
            }
            let class = class_argument(vm, class_object)?;
            let name = vm.classes.get(class).name.clone();
            object(vm.make_string(&name)?)
        },
    ),
    method(
        "getShortDescription",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("")?),
    ),
    method(
        "getValue",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
];

pub(crate) const BEAN_DESCRIPTOR: oxjvm_vm::NativeClass = class(
    "java/beans/BeanDescriptor",
    Some("java/lang/Object"),
    &["java/beans/FeatureDescriptor"],
    ACC_PUBLIC,
    &[field("beanClass", "Ljava/lang/Class;", ACC_PRIVATE, None)],
    &BEAN_DESCRIPTOR_METHODS,
    None,
);

const METHOD_DESCRIPTOR_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "<init>",
        "(Ljava/lang/reflect/Method;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = vm.class_of(receiver(args));
            if let Some((declaring, field)) =
                vm.find_field_opt(class, "method", "Ljava/lang/reflect/Method;")
            {
                vm.set_instance_ref(receiver(args), declaring, field, ref_arg(args, 1));
            }
            void()
        },
    ),
    method(
        "getMethod",
        "()Ljava/lang/reflect/Method;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(
                vm.read_ref_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "method",
                    "Ljava/lang/reflect/Method;",
                )
                .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method("getName", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, _| {
        object(vm.make_string("")?)
    }),
    method(
        "getParameterDescriptors",
        "()[Ljava/beans/ParameterDescriptor;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/beans/ParameterDescriptor")?;
            object(vm.allocate_object_array(class, 0)?)
        },
    ),
    method(
        "getDisplayName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("")?),
    ),
];

pub(crate) const METHOD_DESCRIPTOR: oxjvm_vm::NativeClass = class(
    "java/beans/MethodDescriptor",
    Some("java/lang/Object"),
    &["java/beans/FeatureDescriptor"],
    ACC_PUBLIC,
    &[field(
        "method",
        "Ljava/lang/reflect/Method;",
        ACC_PRIVATE,
        None,
    )],
    &METHOD_DESCRIPTOR_METHODS,
    None,
);

pub(crate) const PARAMETER_DESCRIPTOR: oxjvm_vm::NativeClass = class(
    "java/beans/ParameterDescriptor",
    Some("java/lang/Object"),
    &["java/beans/FeatureDescriptor"],
    ACC_PUBLIC,
    &[],
    &[method("<init>", "()V", ACC_PUBLIC, |_, _, _| void())],
    None,
);

pub(crate) const EVENT_SET_DESCRIPTOR: oxjvm_vm::NativeClass = class(
    "java/beans/EventSetDescriptor",
    Some("java/lang/Object"),
    &["java/beans/FeatureDescriptor"],
    ACC_PUBLIC,
    &[],
    &[method("<init>", "()V", ACC_PUBLIC, |_, _, _| void())],
    None,
);

const SIMPLE_BEAN_INFO_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "getBeanDescriptor",
        "()Ljava/beans/BeanDescriptor;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getEventSetDescriptors",
        "()[Ljava/beans/EventSetDescriptor;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method("getDefaultEventIndex", "()I", ACC_PUBLIC, |_, _, _| int(-1)),
    method(
        "getPropertyDescriptors",
        "()[Ljava/beans/PropertyDescriptor;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method("getDefaultPropertyIndex", "()I", ACC_PUBLIC, |_, _, _| {
        int(-1)
    }),
    method(
        "getMethodDescriptors",
        "()[Ljava/beans/MethodDescriptor;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getAdditionalBeanInfo",
        "()[Ljava/beans/BeanInfo;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
];

pub(crate) const SIMPLE_BEAN_INFO: oxjvm_vm::NativeClass = class(
    "java/beans/SimpleBeanInfo",
    Some("java/lang/Object"),
    &["java/beans/BeanInfo"],
    ACC_PUBLIC,
    &[],
    &SIMPLE_BEAN_INFO_METHODS,
    None,
);

const BEANS_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "isInstanceOf",
        "(Ljava/lang/Object;Ljava/lang/Class;)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let class = class_argument(vm, ref_arg(args, 1))?;
            boolean(vm.is_instance(ref_arg(args, 0), class))
        },
    ),
    method(
        "getInstanceOf",
        "(Ljava/lang/Object;Ljava/lang/Class;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let class = class_argument(vm, ref_arg(args, 1))?;
            if vm.is_instance(ref_arg(args, 0), class) {
                object(ref_arg(args, 0))
            } else {
                object(ObjectRef::NULL)
            }
        },
    ),
    method(
        "instantiate",
        "(Ljava/lang/Class;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let class = class_argument(vm, ref_arg(args, 0))?;
            let instance = vm.new_instance(class)?;
            if let Some((declaring, ctor)) = vm.find_method(class, "<init>", "()V") {
                vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
            }
            object(instance)
        },
    ),
    method("isDesignTime", "()Z", ACC_PUBLIC | ACC_STATIC, |_, _, _| {
        boolean(false)
    }),
    method(
        "setDesignTime",
        "(Z)V",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| void(),
    ),
    method(
        "setGuiAvailable",
        "(Z)V",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| void(),
    ),
];

pub(crate) const BEANS: oxjvm_vm::NativeClass = class(
    "java/beans/Beans",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &BEANS_METHODS,
    None,
);

pub(crate) const INTROSPECTION_EXCEPTION: oxjvm_vm::NativeClass = class(
    "java/beans/IntrospectionException",
    Some("java/lang/Exception"),
    &[],
    ACC_PUBLIC,
    &[],
    &[method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    )],
    None,
);

const FEATURE_DESCRIPTOR_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
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
        "getDisplayName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = vm
                .read_string_field(receiver(args), vm.class_of(receiver(args)), "displayName")
                .unwrap_or_default();
            object(vm.make_string(&name)?)
        },
    ),
    method(
        "getShortDescription",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = vm
                .read_string_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "shortDescription",
                )
                .unwrap_or_default();
            object(vm.make_string(&name)?)
        },
    ),
    method(
        "setName",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            if let Some((declaring, field)) =
                vm.find_field_opt(vm.class_of(receiver(args)), "name", "Ljava/lang/String;")
            {
                vm.set_instance_ref(receiver(args), declaring, field, ref_arg(args, 1));
            }
            void()
        },
    ),
    method(
        "setDisplayName",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            if let Some((declaring, field)) = vm.find_field_opt(
                vm.class_of(receiver(args)),
                "displayName",
                "Ljava/lang/String;",
            ) {
                vm.set_instance_ref(receiver(args), declaring, field, ref_arg(args, 1));
            }
            void()
        },
    ),
    method(
        "setShortDescription",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            if let Some((declaring, field)) = vm.find_field_opt(
                vm.class_of(receiver(args)),
                "shortDescription",
                "Ljava/lang/String;",
            ) {
                vm.set_instance_ref(receiver(args), declaring, field, ref_arg(args, 1));
            }
            void()
        },
    ),
    method("isExpert", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("isHidden", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("isPreferred", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
];

pub(crate) const FEATURE_DESCRIPTOR: oxjvm_vm::NativeClass = class(
    "java/beans/FeatureDescriptor",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("displayName", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("shortDescription", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &FEATURE_DESCRIPTOR_METHODS,
    None,
);
