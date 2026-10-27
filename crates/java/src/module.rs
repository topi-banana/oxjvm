//! `java.lang.module.ModuleDescriptor`: descriptors, builders and the nested
//! `Requires`/`Exports`/`Opens`/`Provides`/`Modifier`/`Version` types.
//!
//! The module path itself is not modelled; this is the data model that tests
//! and tools build and inspect.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::{boolean, class, field, int, method, object, receiver, ref_arg};

// -------------------------------------------------------------------------------------------
// Small collection helpers
// -------------------------------------------------------------------------------------------

fn new_hash_set(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/HashSet")?;
    let instance = vm.new_instance(class)?;
    if let Some((declaring, ctor)) = vm.find_method(class, "<init>", "()V") {
        vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
    }
    Ok(instance)
}

fn new_array_list(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/ArrayList")?;
    let instance = vm.new_instance(class)?;
    if let Some((declaring, ctor)) = vm.find_method(class, "<init>", "()V") {
        vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
    }
    Ok(instance)
}

fn collection_add(vm: &mut Vm<'_>, collection: ObjectRef, value: ObjectRef) -> Result<(), VmError> {
    let (class, method) = vm.resolve_virtual_method(collection, "add", "(Ljava/lang/Object;)Z")?;
    vm.invoke_method(
        class,
        method,
        vec![Value::Ref(collection), Value::Ref(value)],
    )?;
    Ok(())
}

fn collection_refs(vm: &mut Vm<'_>, collection: ObjectRef) -> Result<Vec<ObjectRef>, VmError> {
    if collection.is_null() {
        return Ok(Vec::new());
    }
    let mut values = Vec::new();
    let (class, method) =
        vm.resolve_virtual_method(collection, "iterator", "()Ljava/util/Iterator;")?;
    let iterator = vm
        .invoke_method(class, method, vec![Value::Ref(collection)])?
        .as_ref();
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
        values.push(
            vm.invoke_method(class, method, vec![Value::Ref(iterator)])?
                .as_ref(),
        );
    }
    Ok(values)
}

fn string_values(vm: &mut Vm<'_>, collection: ObjectRef) -> Result<Vec<String>, VmError> {
    let mut values = Vec::new();
    for reference in collection_refs(vm, collection)? {
        values.push(vm.string_value(reference).unwrap_or_default());
    }
    Ok(values)
}

fn set_text(values: &[String]) -> String {
    alloc::format!("[{}]", values.join(", "))
}

fn object_text(vm: &mut Vm<'_>, object_ref: ObjectRef) -> Result<String, VmError> {
    if object_ref.is_null() {
        return Ok(String::new());
    }
    let (class, method) =
        vm.resolve_virtual_method(object_ref, "toString", "()Ljava/lang/String;")?;
    let value = vm.invoke_method(class, method, vec![Value::Ref(object_ref)])?;
    Ok(vm.string_value(value.as_ref()).unwrap_or_default())
}

fn reference_texts(vm: &mut Vm<'_>, collection: ObjectRef) -> Result<Vec<String>, VmError> {
    let mut values = Vec::new();
    for reference in collection_refs(vm, collection)? {
        values.push(object_text(vm, reference)?);
    }
    Ok(values)
}

fn field_set(vm: &mut Vm<'_>, object_ref: ObjectRef, name: &str) -> Result<ObjectRef, VmError> {
    let value = vm
        .read_ref_field(object_ref, vm.class_of(object_ref), name, "Ljava/util/Set;")
        .unwrap_or(ObjectRef::NULL);
    if value.is_null() {
        new_hash_set(vm)
    } else {
        Ok(value)
    }
}

fn field_string(vm: &Vm<'_>, object_ref: ObjectRef, name: &str) -> String {
    vm.read_string_field_any(object_ref, name)
        .unwrap_or_default()
}

fn set_field_ref(vm: &mut Vm<'_>, object_ref: ObjectRef, name: &str, value: ObjectRef) {
    if let Some((declaring, field)) =
        vm.find_field_opt(vm.class_of(object_ref), name, "Ljava/util/Set;")
    {
        vm.set_instance_ref(object_ref, declaring, field, value);
    }
}

fn set_field_string(vm: &mut Vm<'_>, object_ref: ObjectRef, name: &str, value: ObjectRef) {
    if let Some((declaring, field)) =
        vm.find_field_opt(vm.class_of(object_ref), name, "Ljava/lang/String;")
    {
        vm.set_instance_ref(object_ref, declaring, field, value);
    }
}

fn set_field_bool(vm: &mut Vm<'_>, object_ref: ObjectRef, name: &str, value: bool) {
    if let Some((declaring, field)) = vm.find_field_opt(vm.class_of(object_ref), name, "Z") {
        vm.set_instance_int(object_ref, declaring, field, i32::from(value));
    }
}

fn read_bool(vm: &Vm<'_>, object_ref: ObjectRef, name: &str) -> bool {
    vm.read_instance_int_named(object_ref, vm.class_of(object_ref), name, "Z")
        .unwrap_or(0)
        != 0
}

fn optional_string(vm: &mut Vm<'_>, value: Option<&str>) -> Result<Value, VmError> {
    match value {
        Some(text) => {
            let string = vm.make_string(text)?;
            crate::optional::optional_new(vm, string)
        }
        None => crate::optional::optional_empty_value(vm),
    }
}

// -------------------------------------------------------------------------------------------
// ModuleDescriptor.Requires
// -------------------------------------------------------------------------------------------

const REQUIRES_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let name = field_string(vm, receiver(args), "name");
        object(vm.make_string(&name)?)
    }),
    method(
        "modifiers",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let modifiers = field_set(vm, this, "modifiers")?;
            object(modifiers)
        },
    ),
    method(
        "compiledVersion",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let version = field_string(vm, this, "compiledVersion");
            if version.is_empty() {
                return crate::optional::optional_empty_value(vm);
            }
            let class = vm.resolve_class("java/lang/module/ModuleDescriptor$Version")?;
            let instance = vm.new_instance(class)?;
            let text = vm.make_string(&version)?;
            set_field_string(vm, instance, "text", text);
            crate::optional::optional_new(vm, instance)
        },
    ),
    method(
        "rawCompiledVersion",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, args| {
            let version = field_string(vm, receiver(args), "rawCompiledVersion");
            optional_string(
                vm,
                if version.is_empty() {
                    None
                } else {
                    Some(&version)
                },
            )
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name = field_string(vm, this, "name");
            let modifiers = field_set(vm, this, "modifiers")?;
            let mut parts = Vec::new();
            for modifier in collection_refs(vm, modifiers)? {
                parts.push(field_string(vm, modifier, "name").to_ascii_lowercase());
            }
            let version = field_string(vm, this, "compiledVersion");
            let text = if parts.is_empty() {
                if version.is_empty() {
                    name
                } else {
                    alloc::format!("{name} (compiled with {version})")
                }
            } else if version.is_empty() {
                alloc::format!("{} {name}", parts.join(" "))
            } else {
                alloc::format!("{} {name} (compiled with {version})", parts.join(" "))
            };
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
            if other.is_null() || vm.class_of(other) != vm.class_of(this) {
                return boolean(false);
            }
            let left = object_text(vm, this)?;
            let right = object_text(vm, other)?;
            boolean(left == right)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        int(string_hash(&object_text(vm, receiver(args))?))
    }),
];

pub(crate) const REQUIRES: oxjvm_vm::NativeClass = class(
    "java/lang/module/ModuleDescriptor$Requires",
    Some("java/lang/Object"),
    &["java/lang/Comparable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("modifiers", "Ljava/util/Set;", ACC_PRIVATE, None),
        field("compiledVersion", "Ljava/lang/String;", ACC_PRIVATE, None),
        field(
            "rawCompiledVersion",
            "Ljava/lang/String;",
            ACC_PRIVATE,
            None,
        ),
    ],
    &REQUIRES_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// ModuleDescriptor.Exports / Opens
// -------------------------------------------------------------------------------------------

macro_rules! qualified_methods {
    ($constant:ident, $class_name:literal) => {
        const $constant: [oxjvm_vm::NativeMethodDef; 6] = [
            method(
                "source",
                "()Ljava/lang/String;",
                ACC_PUBLIC,
                |vm, _, args| {
                    let source = field_string(vm, receiver(args), "source");
                    object(vm.make_string(&source)?)
                },
            ),
            method("targets", "()Ljava/util/Set;", ACC_PUBLIC, |vm, _, args| {
                let this = receiver(args);
                let targets = field_set(vm, this, "targets")?;
                object(targets)
            }),
            method("isQualified", "()Z", ACC_PUBLIC, |vm, _, args| {
                let this = receiver(args);
                let targets = field_set(vm, this, "targets")?;
                boolean(!collection_refs(vm, targets)?.is_empty())
            }),
            method(
                "toString",
                "()Ljava/lang/String;",
                ACC_PUBLIC,
                |vm, _, args| {
                    let this = receiver(args);
                    let source = field_string(vm, this, "source");
                    let targets = field_set(vm, this, "targets")?;
                    let values = string_values(vm, targets)?;
                    let text = if values.is_empty() {
                        source
                    } else {
                        alloc::format!("{source} to {}", set_text(&values))
                    };
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
                    if other.is_null() || vm.class_of(other) != vm.class_of(this) {
                        return boolean(false);
                    }
                    let left = object_text(vm, this)?;
                    let right = object_text(vm, other)?;
                    boolean(left == right)
                },
            ),
            method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
                int(string_hash(&object_text(vm, receiver(args))?))
            }),
        ];
    };
}

qualified_methods!(EXPORTS_METHODS, "java/lang/module/ModuleDescriptor$Exports");

pub(crate) const EXPORTS: oxjvm_vm::NativeClass = class(
    "java/lang/module/ModuleDescriptor$Exports",
    Some("java/lang/Object"),
    &["java/lang/Comparable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("source", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("targets", "Ljava/util/Set;", ACC_PRIVATE, None),
    ],
    &EXPORTS_METHODS,
    None,
);

qualified_methods!(OPENS_METHODS, "java/lang/module/ModuleDescriptor$Opens");

pub(crate) const OPENS: oxjvm_vm::NativeClass = class(
    "java/lang/module/ModuleDescriptor$Opens",
    Some("java/lang/Object"),
    &["java/lang/Comparable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("source", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("targets", "Ljava/util/Set;", ACC_PRIVATE, None),
    ],
    &OPENS_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// ModuleDescriptor.Provides
// -------------------------------------------------------------------------------------------

const PROVIDES_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "service",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let service = field_string(vm, receiver(args), "service");
            object(vm.make_string(&service)?)
        },
    ),
    method(
        "providers",
        "()Ljava/util/List;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let providers = vm
                .read_ref_field(this, vm.class_of(this), "providers", "Ljava/util/List;")
                .unwrap_or(ObjectRef::NULL);
            if providers.is_null() {
                return object(new_array_list(vm)?);
            }
            object(providers)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let service = field_string(vm, this, "service");
            let providers = vm
                .read_ref_field(this, vm.class_of(this), "providers", "Ljava/util/List;")
                .unwrap_or(ObjectRef::NULL);
            let values = string_values(vm, providers)?;
            object(vm.make_string(&alloc::format!("{service} with {}", set_text(&values)))?)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            if other.is_null() || vm.class_of(other) != vm.class_of(this) {
                return boolean(false);
            }
            let left = object_text(vm, this)?;
            let right = object_text(vm, other)?;
            boolean(left == right)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        int(string_hash(&object_text(vm, receiver(args))?))
    }),
];

pub(crate) const PROVIDES: oxjvm_vm::NativeClass = class(
    "java/lang/module/ModuleDescriptor$Provides",
    Some("java/lang/Object"),
    &["java/lang/Comparable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("service", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("providers", "Ljava/util/List;", ACC_PRIVATE, None),
    ],
    &PROVIDES_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// ModuleDescriptor.Modifier / Requires.Modifier
// -------------------------------------------------------------------------------------------

fn string_hash(text: &str) -> i32 {
    let mut hash = 0i32;
    for unit in text.encode_utf16() {
        hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
    }
    hash
}

fn enum_constant(
    vm: &mut Vm<'_>,
    class_name: &str,
    name: &str,
    ordinal: i32,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class(class_name)?;
    let instance = vm.new_instance(class)?;
    if let Some((declaring, field)) = vm.find_field_opt(class, "ordinal", "I") {
        vm.set_instance_int(instance, declaring, field, ordinal);
    }
    let value = vm.make_string(name)?;
    set_field_string(vm, instance, "name", value);
    Ok(instance)
}

macro_rules! enum_class {
    ($constant:ident, $methods:ident, $clinit:ident, $class_name:literal, [$($name:literal),* $(,)?]) => {
        const $methods: [oxjvm_vm::NativeMethodDef; 5] = [
            method("ordinal", "()I", ACC_PUBLIC, |vm, _, args| {
                int(crate::lang::read_int_field(vm, receiver(args), "ordinal"))
            }),
            method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
                let name = field_string(vm, receiver(args), "name");
                object(vm.make_string(&name)?)
            }),
            method(
                "toString",
                "()Ljava/lang/String;",
                ACC_PUBLIC,
                |vm, _, args| {
                    let name = field_string(vm, receiver(args), "name");
                    object(vm.make_string(&name)?)
                },
            ),
            method(
                "values",
                concat!("()[L", $class_name, ";"),
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, _| {
                    let class = vm.resolve_class($class_name)?;
                    vm.ensure_initialized(class)?;
                    let names = [$(($name)),*];
                    let array = vm.allocate_object_array(class, names.len())?;
                    for (index, name) in names.iter().enumerate() {
                        if let Some(value) = vm.read_static_ref_if_set(
                            class,
                            name,
                            concat!("L", $class_name, ";"),
                        ) {
                            vm.array_set_ref(array, index, value)?;
                        }
                    }
                    object(array)
                },
            ),
            method(
                "valueOf",
                concat!(
                    "(Ljava/lang/String;)L", $class_name, ";"
                ),
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    let name = vm.string_or_empty(ref_arg(args, 0));
                    let class = vm.resolve_class($class_name)?;
                    vm.ensure_initialized(class)?;
                    let names = [$(($name)),*];
                    if names.contains(&name.as_str()) {
                        if let Some(value) = vm.read_static_ref_if_set(
                            class,
                            &name,
                            concat!("L", $class_name, ";"),
                        ) {
                            return object(value);
                        }
                    }
                    Err(vm.throw_new(
                        "java/lang/IllegalArgumentException",
                        Some(&alloc::format!(
                            "No enum constant {}.{name}",
                            $class_name.replace('/', ".")
                        )),
                    ))
                },
            ),
        ];

        fn $clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
            let class = vm.resolve_class($class_name)?;
            let names = [$(($name)),*];
            for (ordinal, name) in names.iter().enumerate() {
                let value = enum_constant(vm, $class_name, name, ordinal as i32)?;
                vm.set_static_value(
                    class,
                    name,
                    concat!("L", $class_name, ";"),
                    Value::Ref(value),
                );
            }
            Ok(())
        }

        pub(crate) const $constant: oxjvm_vm::NativeClass = class(
            $class_name,
            Some("java/lang/Object"),
            &[],
            ACC_PUBLIC | ACC_FINAL | ACC_ENUM,
            &[
                $(
                    field(
                        $name,
                        concat!("L", $class_name, ";"),
                        ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
                        None,
                    ),
                )*
                field("ordinal", "I", ACC_PRIVATE, None),
                field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
            ],
            &$methods,
            Some($clinit),
        );
    };
}

enum_class!(
    REQUIRES_MODIFIER,
    REQUIRES_MODIFIER_METHODS,
    requires_modifier_clinit,
    "java/lang/module/ModuleDescriptor$Requires$Modifier",
    ["TRANSITIVE", "STATIC", "SYNTHETIC", "MANDATED"]
);

enum_class!(
    DESCRIPTOR_MODIFIER,
    DESCRIPTOR_MODIFIER_METHODS,
    descriptor_modifier_clinit,
    "java/lang/module/ModuleDescriptor$Modifier",
    ["OPEN", "AUTOMATIC", "SYNTHETIC", "MANDATED"]
);

fn modifier_set(vm: &mut Vm<'_>, class_name: &str, names: &[&str]) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class(class_name)?;
    vm.ensure_initialized(class)?;
    let set = new_hash_set(vm)?;
    for name in names {
        if let Some(value) =
            vm.read_static_ref_if_set(class, name, &alloc::format!("L{class_name};"))
        {
            collection_add(vm, set, value)?;
        }
    }
    Ok(set)
}

// -------------------------------------------------------------------------------------------
// ModuleDescriptor.Version
// -------------------------------------------------------------------------------------------

const VERSION_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "parse",
        "(Ljava/lang/String;)Ljava/lang/module/ModuleDescriptor$Version;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = crate::string_arg(vm, args[0])?;
            if text.is_empty() {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("empty version string"),
                ));
            }
            let class = vm.resolve_class("java/lang/module/ModuleDescriptor$Version")?;
            let instance = vm.new_instance(class)?;
            let value = vm.make_string(&text)?;
            set_field_string(vm, instance, "text", value);
            object(instance)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = field_string(vm, receiver(args), "text");
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
            if other.is_null() || vm.class_of(other) != vm.class_of(this) {
                return boolean(false);
            }
            boolean(field_string(vm, this, "text") == field_string(vm, other, "text"))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        int(string_hash(&field_string(vm, receiver(args), "text")))
    }),
    method(
        "compareTo",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let left = field_string(vm, receiver(args), "text");
            let right = field_string(vm, ref_arg(args, 1), "text");
            int(match left.cmp(&right) {
                core::cmp::Ordering::Less => -1,
                core::cmp::Ordering::Equal => 0,
                core::cmp::Ordering::Greater => 1,
            })
        },
    ),
];

pub(crate) const VERSION: oxjvm_vm::NativeClass = class(
    "java/lang/module/ModuleDescriptor$Version",
    Some("java/lang/Object"),
    &["java/lang/Comparable"],
    ACC_PUBLIC | ACC_FINAL,
    &[field("text", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &VERSION_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Builders for the nested value types
// -------------------------------------------------------------------------------------------

fn new_requires(vm: &mut Vm<'_>, name: &str, modifiers: ObjectRef) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/lang/module/ModuleDescriptor$Requires")?;
    let instance = vm.new_instance(class)?;
    let name_value = vm.make_string(name)?;
    set_field_string(vm, instance, "name", name_value);
    set_field_ref(vm, instance, "modifiers", modifiers);
    Ok(instance)
}

fn new_qualified(
    vm: &mut Vm<'_>,
    class_name: &str,
    source: &str,
    targets: ObjectRef,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class(class_name)?;
    let instance = vm.new_instance(class)?;
    let source_value = vm.make_string(source)?;
    set_field_string(vm, instance, "source", source_value);
    set_field_ref(vm, instance, "targets", targets);
    Ok(instance)
}

fn new_provides(
    vm: &mut Vm<'_>,
    service: &str,
    providers: ObjectRef,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/lang/module/ModuleDescriptor$Provides")?;
    let instance = vm.new_instance(class)?;
    let service_value = vm.make_string(service)?;
    set_field_string(vm, instance, "service", service_value);
    if let Some((declaring, field)) = vm.find_field_opt(class, "providers", "Ljava/util/List;") {
        vm.set_instance_ref(instance, declaring, field, providers);
    }
    Ok(instance)
}

// -------------------------------------------------------------------------------------------
// ModuleDescriptor.Builder
// -------------------------------------------------------------------------------------------
fn builder_add_requires(
    vm: &mut Vm<'_>,
    builder: ObjectRef,
    requirement: ObjectRef,
    name: &str,
) -> Result<(), VmError> {
    let requires = field_set(vm, builder, "requires")?;
    for existing in collection_refs(vm, requires)? {
        if field_string(vm, existing, "name") == name {
            let (class, method) =
                vm.resolve_virtual_method(requires, "remove", "(Ljava/lang/Object;)Z")?;
            vm.invoke_method(
                class,
                method,
                vec![Value::Ref(requires), Value::Ref(existing)],
            )?;
            break;
        }
    }
    collection_add(vm, requires, requirement)
}

const BUILDER_METHODS: [oxjvm_vm::NativeMethodDef; 17] = [
    method(
        "build",
        "()Ljava/lang/module/ModuleDescriptor;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let descriptor_class = vm.resolve_class("java/lang/module/ModuleDescriptor")?;
            let descriptor = vm.new_instance(descriptor_class)?;
            let name = field_string(vm, this, "name");
            let name_value = vm.make_string(&name)?;
            set_field_string(vm, descriptor, "name", name_value);
            set_field_bool(vm, descriptor, "open", read_bool(vm, this, "open"));
            set_field_bool(
                vm,
                descriptor,
                "automatic",
                read_bool(vm, this, "automatic"),
            );
            for field_name in [
                "requires", "exports", "opens", "uses", "provides", "packages",
            ] {
                let value = field_set(vm, this, field_name)?;
                set_field_ref(vm, descriptor, field_name, value);
            }
            for field_name in ["version", "mainClass"] {
                let text = field_string(vm, this, field_name);
                if !text.is_empty() {
                    let text_value = vm.make_string(&text)?;
                    set_field_string(vm, descriptor, field_name, text_value);
                }
            }
            object(descriptor)
        },
    ),
    method(
        "name",
        "(Ljava/lang/String;)Ljava/lang/module/ModuleDescriptor$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name = crate::string_arg(vm, args[1])?;
            let name_value = vm.make_string(&name)?;
            set_field_string(vm, this, "name", name_value);
            object(this)
        },
    ),
    method(
        "requires",
        "(Ljava/lang/String;)Ljava/lang/module/ModuleDescriptor$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name = crate::string_arg(vm, args[1])?;
            let modifiers = new_hash_set(vm)?;
            let requirement = new_requires(vm, &name, modifiers)?;
            builder_add_requires(vm, this, requirement, &name)?;
            object(this)
        },
    ),
    method(
        "requires",
        "(Ljava/util/Set;Ljava/lang/String;)Ljava/lang/module/ModuleDescriptor$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name = crate::string_arg(vm, args[2])?;
            let modifiers = ref_arg(args, 1);
            let requirement = new_requires(vm, &name, modifiers)?;
            builder_add_requires(vm, this, requirement, &name)?;
            object(this)
        },
    ),
    method(
        "requires",
        "(Ljava/lang/module/ModuleDescriptor$Requires;)Ljava/lang/module/ModuleDescriptor$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let requirement = ref_arg(args, 1);
            let name = field_string(vm, requirement, "name");
            builder_add_requires(vm, this, requirement, &name)?;
            object(this)
        },
    ),
    method(
        "exports",
        "(Ljava/lang/String;)Ljava/lang/module/ModuleDescriptor$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let source = crate::string_arg(vm, args[1])?;
            let targets = new_hash_set(vm)?;
            let export = new_qualified(
                vm,
                "java/lang/module/ModuleDescriptor$Exports",
                &source,
                targets,
            )?;
            let exports = field_set(vm, this, "exports")?;
            collection_add(vm, exports, export)?;
            object(this)
        },
    ),
    method(
        "exports",
        "(Ljava/lang/String;Ljava/util/Set;)Ljava/lang/module/ModuleDescriptor$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let source = crate::string_arg(vm, args[1])?;
            let export = new_qualified(
                vm,
                "java/lang/module/ModuleDescriptor$Exports",
                &source,
                ref_arg(args, 2),
            )?;
            let exports = field_set(vm, this, "exports")?;
            collection_add(vm, exports, export)?;
            object(this)
        },
    ),
    method(
        "exports",
        "(Ljava/lang/module/ModuleDescriptor$Exports;)Ljava/lang/module/ModuleDescriptor$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let exports = field_set(vm, this, "exports")?;
            collection_add(vm, exports, ref_arg(args, 1))?;
            object(this)
        },
    ),
    method(
        "opens",
        "(Ljava/lang/String;)Ljava/lang/module/ModuleDescriptor$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let source = crate::string_arg(vm, args[1])?;
            let targets = new_hash_set(vm)?;
            let open = new_qualified(
                vm,
                "java/lang/module/ModuleDescriptor$Opens",
                &source,
                targets,
            )?;
            let opens = field_set(vm, this, "opens")?;
            collection_add(vm, opens, open)?;
            object(this)
        },
    ),
    method(
        "opens",
        "(Ljava/lang/String;Ljava/util/Set;)Ljava/lang/module/ModuleDescriptor$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let source = crate::string_arg(vm, args[1])?;
            let open = new_qualified(
                vm,
                "java/lang/module/ModuleDescriptor$Opens",
                &source,
                ref_arg(args, 2),
            )?;
            let opens = field_set(vm, this, "opens")?;
            collection_add(vm, opens, open)?;
            object(this)
        },
    ),
    method(
        "opens",
        "(Ljava/lang/module/ModuleDescriptor$Opens;)Ljava/lang/module/ModuleDescriptor$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let opens = field_set(vm, this, "opens")?;
            collection_add(vm, opens, ref_arg(args, 1))?;
            object(this)
        },
    ),
    method(
        "uses",
        "(Ljava/lang/String;)Ljava/lang/module/ModuleDescriptor$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name = crate::string_arg(vm, args[1])?;
            let uses = field_set(vm, this, "uses")?;
            let value = vm.make_string(&name)?;
            collection_add(vm, uses, value)?;
            object(this)
        },
    ),
    method(
        "provides",
        "(Ljava/lang/String;Ljava/util/List;)Ljava/lang/module/ModuleDescriptor$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let service = crate::string_arg(vm, args[1])?;
            let provides = new_provides(vm, &service, ref_arg(args, 2))?;
            let all = field_set(vm, this, "provides")?;
            collection_add(vm, all, provides)?;
            object(this)
        },
    ),
    method(
        "provides",
        "(Ljava/lang/module/ModuleDescriptor$Provides;)Ljava/lang/module/ModuleDescriptor$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let provides = field_set(vm, this, "provides")?;
            collection_add(vm, provides, ref_arg(args, 1))?;
            object(this)
        },
    ),
    method(
        "packages",
        "(Ljava/util/Set;)Ljava/lang/module/ModuleDescriptor$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let packages = ref_arg(args, 1);
            set_field_ref(vm, this, "packages", packages);
            object(this)
        },
    ),
    method(
        "version",
        "(Ljava/lang/String;)Ljava/lang/module/ModuleDescriptor$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let version = crate::string_arg(vm, args[1])?;
            let version_value = vm.make_string(&version)?;
            set_field_string(vm, this, "version", version_value);
            object(this)
        },
    ),
    method(
        "mainClass",
        "(Ljava/lang/String;)Ljava/lang/module/ModuleDescriptor$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name = crate::string_arg(vm, args[1])?;
            let name_value = vm.make_string(&name)?;
            set_field_string(vm, this, "mainClass", name_value);
            object(this)
        },
    ),
];

pub(crate) const BUILDER: oxjvm_vm::NativeClass = class(
    "java/lang/module/ModuleDescriptor$Builder",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("open", "Z", ACC_PRIVATE, None),
        field("automatic", "Z", ACC_PRIVATE, None),
        field("requires", "Ljava/util/Set;", ACC_PRIVATE, None),
        field("exports", "Ljava/util/Set;", ACC_PRIVATE, None),
        field("opens", "Ljava/util/Set;", ACC_PRIVATE, None),
        field("uses", "Ljava/util/Set;", ACC_PRIVATE, None),
        field("provides", "Ljava/util/Set;", ACC_PRIVATE, None),
        field("packages", "Ljava/util/Set;", ACC_PRIVATE, None),
        field("version", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("mainClass", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &BUILDER_METHODS,
    None,
);

fn new_builder(vm: &mut Vm<'_>, name: &str, open: bool, automatic: bool) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/lang/module/ModuleDescriptor$Builder")?;
    let instance = vm.new_instance(class)?;
    let name_value = vm.make_string(name)?;
    set_field_string(vm, instance, "name", name_value);
    set_field_bool(vm, instance, "open", open);
    set_field_bool(vm, instance, "automatic", automatic);
    for field_name in [
        "requires", "exports", "opens", "uses", "provides", "packages",
    ] {
        let set = new_hash_set(vm)?;
        set_field_ref(vm, instance, field_name, set);
    }
    // `java.base` is an implicit `mandated` requirement of every module.
    let modifiers = modifier_set(
        vm,
        "java/lang/module/ModuleDescriptor$Requires$Modifier",
        &["MANDATED"],
    )?;
    let requirement = new_requires(vm, "java.base", modifiers)?;
    let requires = field_set(vm, instance, "requires")?;
    collection_add(vm, requires, requirement)?;
    object(instance)
}

// -------------------------------------------------------------------------------------------
// ModuleDescriptor
// -------------------------------------------------------------------------------------------

fn descriptor_requires_names(
    vm: &mut Vm<'_>,
    descriptor: ObjectRef,
) -> Result<Vec<String>, VmError> {
    let requires = field_set(vm, descriptor, "requires")?;
    let mut names = Vec::new();
    for requirement in collection_refs(vm, requires)? {
        names.push(field_string(vm, requirement, "name"));
    }
    Ok(names)
}

fn descriptor_text(vm: &mut Vm<'_>, descriptor: ObjectRef) -> Result<String, VmError> {
    let name = field_string(vm, descriptor, "name");
    let version = field_string(vm, descriptor, "version");
    let mut text = String::new();
    if read_bool(vm, descriptor, "open") {
        text.push_str("open ");
    }
    text.push_str("module { name: ");
    text.push_str(&name);
    if !version.is_empty() {
        text.push('@');
        text.push_str(&version);
    }
    let requires = descriptor_requires_names(vm, descriptor)?;
    text.push_str(", ");
    text.push_str(&set_text(&requires));

    let uses = field_set(vm, descriptor, "uses")?;
    let uses = string_values(vm, uses)?;
    if !uses.is_empty() {
        text.push_str(", uses: ");
        text.push_str(&set_text(&uses));
    }
    let exports = field_set(vm, descriptor, "exports")?;
    let exports = reference_texts(vm, exports)?;
    if !exports.is_empty() {
        text.push_str(", exports: ");
        text.push_str(&set_text(&exports));
    }
    let opens = field_set(vm, descriptor, "opens")?;
    let opens = reference_texts(vm, opens)?;
    if !opens.is_empty() {
        text.push_str(", opens: ");
        text.push_str(&set_text(&opens));
    }
    let provides = field_set(vm, descriptor, "provides")?;
    let provides = reference_texts(vm, provides)?;
    if !provides.is_empty() {
        text.push_str(", provides: ");
        text.push_str(&set_text(&provides));
    }
    text.push_str(" }");
    Ok(text)
}

const DESCRIPTOR_METHODS: [oxjvm_vm::NativeMethodDef; 23] = [
    method(
        "newModule",
        "(Ljava/lang/String;)Ljava/lang/module/ModuleDescriptor$Builder;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = crate::string_arg(vm, args[0])?;
            new_builder(vm, &name, false, false)
        },
    ),
    method(
        "newOpenModule",
        "(Ljava/lang/String;)Ljava/lang/module/ModuleDescriptor$Builder;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = crate::string_arg(vm, args[0])?;
            new_builder(vm, &name, true, false)
        },
    ),
    method(
        "newAutomaticModule",
        "(Ljava/lang/String;)Ljava/lang/module/ModuleDescriptor$Builder;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = crate::string_arg(vm, args[0])?;
            new_builder(vm, &name, false, true)
        },
    ),
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let name = field_string(vm, receiver(args), "name");
        object(vm.make_string(&name)?)
    }),
    method("isOpen", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(read_bool(vm, receiver(args), "open"))
    }),
    method("isAutomatic", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(read_bool(vm, receiver(args), "automatic"))
    }),
    method("isSynthetic", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("isMandated", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method(
        "modifiers",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let mut names: Vec<&str> = Vec::new();
            if read_bool(vm, this, "open") {
                names.push("OPEN");
            }
            if read_bool(vm, this, "automatic") {
                names.push("AUTOMATIC");
            }
            let set = modifier_set(vm, "java/lang/module/ModuleDescriptor$Modifier", &names)?;
            object(set)
        },
    ),
    method(
        "requires",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let requires = field_set(vm, this, "requires")?;
            object(requires)
        },
    ),
    method("exports", "()Ljava/util/Set;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let exports = field_set(vm, this, "exports")?;
        object(exports)
    }),
    method("opens", "()Ljava/util/Set;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let opens = field_set(vm, this, "opens")?;
        object(opens)
    }),
    method("uses", "()Ljava/util/Set;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let uses = field_set(vm, this, "uses")?;
        object(uses)
    }),
    method(
        "provides",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let provides = field_set(vm, this, "provides")?;
            object(provides)
        },
    ),
    method(
        "packages",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let packages = field_set(vm, this, "packages")?;
            object(packages)
        },
    ),
    method(
        "version",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let version = field_string(vm, this, "version");
            if version.is_empty() {
                return crate::optional::optional_empty_value(vm);
            }
            let class = vm.resolve_class("java/lang/module/ModuleDescriptor$Version")?;
            let instance = vm.new_instance(class)?;
            let text = vm.make_string(&version)?;
            set_field_string(vm, instance, "text", text);
            crate::optional::optional_new(vm, instance)
        },
    ),
    method(
        "rawVersion",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, args| {
            let version = field_string(vm, receiver(args), "version");
            optional_string(
                vm,
                if version.is_empty() {
                    None
                } else {
                    Some(&version)
                },
            )
        },
    ),
    method(
        "mainClass",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, args| {
            let main = field_string(vm, receiver(args), "mainClass");
            optional_string(vm, if main.is_empty() { None } else { Some(&main) })
        },
    ),
    method(
        "toNameAndVersion",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name = field_string(vm, this, "name");
            let version = field_string(vm, this, "version");
            let text = if version.is_empty() {
                name
            } else {
                alloc::format!("{name}@{version}")
            };
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = descriptor_text(vm, receiver(args))?;
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
            if other.is_null() || vm.class_of(other) != vm.class_of(this) {
                return boolean(false);
            }
            let left = descriptor_text(vm, this)?;
            let right = descriptor_text(vm, other)?;
            boolean(left == right)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        int(string_hash(&descriptor_text(vm, receiver(args))?))
    }),
    method(
        "compareTo",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let left = field_string(vm, this, "name");
            let right = field_string(vm, other, "name");
            let ordering = left.cmp(&right);
            int(match ordering {
                core::cmp::Ordering::Less => -1,
                core::cmp::Ordering::Equal => {
                    let lv = field_string(vm, this, "version");
                    let rv = field_string(vm, other, "version");
                    match lv.cmp(&rv) {
                        core::cmp::Ordering::Less => -1,
                        core::cmp::Ordering::Equal => 0,
                        core::cmp::Ordering::Greater => 1,
                    }
                }
                core::cmp::Ordering::Greater => 1,
            })
        },
    ),
];

pub(crate) const MODULE_DESCRIPTOR: oxjvm_vm::NativeClass = class(
    "java/lang/module/ModuleDescriptor",
    Some("java/lang/Object"),
    &["java/lang/Comparable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("open", "Z", ACC_PRIVATE, None),
        field("automatic", "Z", ACC_PRIVATE, None),
        field("requires", "Ljava/util/Set;", ACC_PRIVATE, None),
        field("exports", "Ljava/util/Set;", ACC_PRIVATE, None),
        field("opens", "Ljava/util/Set;", ACC_PRIVATE, None),
        field("uses", "Ljava/util/Set;", ACC_PRIVATE, None),
        field("provides", "Ljava/util/Set;", ACC_PRIVATE, None),
        field("packages", "Ljava/util/Set;", ACC_PRIVATE, None),
        field("version", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("mainClass", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &DESCRIPTOR_METHODS,
    None,
);
