//! Tier 2.4: `javax.tools` compiler tooling, `javax.lang.model.SourceVersion`, and
//! the `com.sun.tools.javac` entry points.  The compilation entry points delegate
//! to the host JDK's `javac`/`javadoc` binaries (the same way `ProcessBuilder`
//! delegates to the host), so this module removes the `NoClassDefFoundError`s
//! that blocked ~800 `langtools` tests and gives them a working compiler.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ArrayComponent, ObjectRef, Value, Vm, VmError};

use crate::lang::read_int_field;
use crate::{boolean, class, field, int, method, object, receiver, ref_arg, void};

const SOURCE_VERSIONS: [&str; 28] = [
    "RELEASE_0",
    "RELEASE_1",
    "RELEASE_2",
    "RELEASE_3",
    "RELEASE_4",
    "RELEASE_5",
    "RELEASE_6",
    "RELEASE_7",
    "RELEASE_8",
    "RELEASE_9",
    "RELEASE_10",
    "RELEASE_11",
    "RELEASE_12",
    "RELEASE_13",
    "RELEASE_14",
    "RELEASE_15",
    "RELEASE_16",
    "RELEASE_17",
    "RELEASE_18",
    "RELEASE_19",
    "RELEASE_20",
    "RELEASE_21",
    "RELEASE_22",
    "RELEASE_23",
    "RELEASE_24",
    "RELEASE_25",
    "RELEASE_26",
    "RELEASE_27",
];

// --------------------------------------------------------------------------
// javax.lang.model.SourceVersion
// --------------------------------------------------------------------------

pub(crate) const JAVA_FILE_MANAGER_LOCATION: oxjvm_vm::NativeClass = class(
    "javax/tools/JavaFileManager$Location",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &JAVA_FILE_MANAGER_LOCATION_METHODS,
    None,
);

const JAVA_FILE_MANAGER_LOCATION_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "isOutputLocation",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "iterator",
        "()Ljava/util/Iterator;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const SOURCE_VERSION: oxjvm_vm::NativeClass = class(
    "javax/lang/model/SourceVersion",
    Some("java/lang/Enum"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_ENUM,
    &[
        field(
            "$VALUES",
            "[Ljavax/lang/model/SourceVersion;",
            ACC_PRIVATE | ACC_STATIC | ACC_FINAL | ACC_SYNTHETIC,
            None,
        ),
        field(
            "RELEASE_0",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_1",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_2",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_3",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_4",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_5",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_6",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_7",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_8",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_9",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_10",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_11",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_12",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_13",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_14",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_15",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_16",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_17",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_18",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_19",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_20",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_21",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_22",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_23",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_24",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_25",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_26",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE_27",
            "Ljavax/lang/model/SourceVersion;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
    ],
    &SOURCE_VERSION_METHODS,
    Some(source_version_clinit),
);

const SOURCE_VERSION_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "latest",
        "()Ljavax/lang/model/SourceVersion;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _args| {
            let class = vm.resolve_class("javax/lang/model/SourceVersion")?;
            vm.ensure_initialized(class)?;
            let value = vm
                .read_static_ref(class, "$VALUES", "[Ljavax/lang/model/SourceVersion;")
                .ok_or_else(|| VmError::internal("no $VALUES"))?;
            let length = vm.array_length(value)?;
            object(vm.array_get(value, (length - 1) as i32)?.as_ref())
        },
    ),
    method(
        "latestSupported",
        "()Ljavax/lang/model/SourceVersion;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _args| {
            let class = vm.resolve_class("javax/lang/model/SourceVersion")?;
            vm.ensure_initialized(class)?;
            let value = vm
                .read_static_ref(class, "$VALUES", "[Ljavax/lang/model/SourceVersion;")
                .ok_or_else(|| VmError::internal("no $VALUES"))?;
            let length = vm.array_length(value)?;
            object(vm.array_get(value, (length - 1) as i32)?.as_ref())
        },
    ),
    method(
        "values",
        "()[Ljavax/lang/model/SourceVersion;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _args| {
            let class = vm.resolve_class("javax/lang/model/SourceVersion")?;
            vm.ensure_initialized(class)?;
            let value = vm
                .read_static_ref(class, "$VALUES", "[Ljavax/lang/model/SourceVersion;")
                .ok_or_else(|| VmError::internal("no $VALUES"))?;
            let clone = vm.clone_object(value)?;
            object(clone)
        },
    ),
    method(
        "valueOf",
        "(Ljava/lang/String;)Ljavax/lang/model/SourceVersion;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = vm.string_or_empty(ref_arg(args, 0));
            let class = vm.resolve_class("javax/lang/model/SourceVersion")?;
            vm.ensure_initialized(class)?;
            let values = vm
                .read_static_ref(class, "$VALUES", "[Ljavax/lang/model/SourceVersion;")
                .ok_or_else(|| VmError::internal("no $VALUES"))?;
            let length = vm.array_length(values)?;
            for index in 0..length {
                let element = vm.array_get(values, index as i32)?.as_ref();
                let elem_class = vm.class_of(element);
                let elem_name = vm
                    .read_string_field(element, elem_class, "name")
                    .unwrap_or_default();
                if elem_name == name {
                    return object(element);
                }
            }
            Err(vm.throw_new("java/lang/IllegalArgumentException", Some(&name)))
        },
    ),
];

fn source_version_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("javax/lang/model/SourceVersion")?;
    vm.ensure_initialized(class)?;
    let mut values = Vec::with_capacity(SOURCE_VERSIONS.len());
    for (ordinal, name) in SOURCE_VERSIONS.iter().enumerate() {
        let instance = vm.new_instance(class)?;
        let name_ref = vm.make_string(name)?;
        let (decl, ctor_id) = vm
            .find_method(class, "<init>", "(Ljava/lang/String;I)V")
            .ok_or_else(|| VmError::internal("SourceVersion lacks its constructor"))?;
        vm.invoke_method(
            decl,
            ctor_id,
            vec![
                Value::Ref(instance),
                Value::Ref(name_ref),
                Value::Int(ordinal as i32),
            ],
        )?;
        let field_name = *name;
        // register the static slot lazily via set_static_value; the field must exist in the class definition
        vm.set_static_value(
            class,
            field_name,
            "Ljavax/lang/model/SourceVersion;",
            Value::Ref(instance),
        );
        values.push(Value::Ref(instance));
    }
    // build $VALUES array
    let array = vm.allocate_array_of(ArrayComponent::Reference, values.len())?;
    for (index, value) in values.into_iter().enumerate() {
        vm.array_set(array, index as i32, value, false)?;
    }
    // register $VALUES static field (declared) by setting it
    vm.set_static_value(
        class,
        "$VALUES",
        "[Ljavax/lang/model/SourceVersion;",
        Value::Ref(array),
    );
    Ok(())
}

// --------------------------------------------------------------------------
// javax.tools.JavaFileObject$Kind
// --------------------------------------------------------------------------

pub(crate) const JAVA_FILE_OBJECT_KIND: oxjvm_vm::NativeClass = class(
    "javax/tools/JavaFileObject$Kind",
    Some("java/lang/Enum"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_ENUM,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE | ACC_FINAL, None),
        field("ordinal", "I", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "extension",
            "Ljava/lang/String;",
            ACC_PUBLIC | ACC_FINAL,
            None,
        ),
        field(
            "SOURCE",
            "Ljavax/tools/JavaFileObject$Kind;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "CLASS",
            "Ljavax/tools/JavaFileObject$Kind;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "HTML",
            "Ljavax/tools/JavaFileObject$Kind;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "OTHER",
            "Ljavax/tools/JavaFileObject$Kind;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "$VALUES",
            "[Ljavax/tools/JavaFileObject$Kind;",
            ACC_PRIVATE | ACC_STATIC | ACC_FINAL | ACC_SYNTHETIC,
            None,
        ),
    ],
    &JAVA_FILE_OBJECT_KIND_METHODS,
    Some(java_file_object_kind_clinit),
);

const JAVA_FILE_OBJECT_KIND_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
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
        "extension",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let ext = vm
                .read_string_field(this, class, "extension")
                .unwrap_or_default();
            object(vm.make_string(&ext)?)
        },
    ),
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
    method("isDeclaredType", "()Z", ACC_PUBLIC, |_vm, _, _args| {
        boolean(false)
    }),
];

fn java_file_object_kind_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("javax/tools/JavaFileObject$Kind")?;
    let values_order = [
        ("SOURCE", "SOURCE", ".java"),
        ("CLASS", "CLASS", ".class"),
        ("HTML", "HTML", ".html"),
        ("OTHER", "OTHER", ""),
    ];
    let mut built = Vec::with_capacity(values_order.len());
    for (ordinal, (name, _disp, ext)) in values_order.iter().enumerate() {
        let instance = vm.new_instance(class)?;
        let name_ref = vm.make_string(name)?;
        let (decl, ctor) = vm
            .find_method(class, "<init>", "(Ljava/lang/String;I)V")
            .ok_or_else(|| VmError::internal("JavaFileObject$Kind lacks its constructor"))?;
        vm.invoke_method(
            decl,
            ctor,
            vec![
                Value::Ref(instance),
                Value::Ref(name_ref),
                Value::Int(ordinal as i32),
            ],
        )?;
        let (d2, fid) = vm.find_field(class, "extension", "Ljava/lang/String;")?;
        let ext_ref = vm.make_string(ext)?;
        vm.set_instance_ref(instance, d2, fid, ext_ref);
        vm.set_static_value(
            class,
            name,
            "Ljavax/tools/JavaFileObject$Kind;",
            Value::Ref(instance),
        );
        built.push(Value::Ref(instance));
    }
    let array = vm.allocate_array_of(ArrayComponent::Reference, built.len())?;
    for (i, v) in built.into_iter().enumerate() {
        vm.array_set(array, i as i32, v, false)?;
    }
    vm.set_static_value(
        class,
        "$VALUES",
        "[Ljavax/tools/JavaFileObject$Kind;",
        Value::Ref(array),
    );
    Ok(())
}

// --------------------------------------------------------------------------
// javax.tools.StandardLocation
// --------------------------------------------------------------------------

pub(crate) const STANDARD_LOCATION: oxjvm_vm::NativeClass = class(
    "javax/tools/StandardLocation",
    Some("java/lang/Enum"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_ENUM,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE | ACC_FINAL, None),
        field("ordinal", "I", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "CLASS_OUTPUT",
            "Ljavax/tools/StandardLocation;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "SOURCE_OUTPUT",
            "Ljavax/tools/StandardLocation;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "CLASS_PATH",
            "Ljavax/tools/StandardLocation;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "SOURCE_PATH",
            "Ljavax/tools/StandardLocation;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "ANNOTATION_PROCESSOR_PATH",
            "Ljavax/tools/StandardLocation;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "ANNOTATION_PROCESSOR_MODULE_PATH",
            "Ljavax/tools/StandardLocation;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "MODULE_SOURCE_PATH",
            "Ljavax/tools/StandardLocation;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "MODULE_PATH",
            "Ljavax/tools/StandardLocation;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "PATCH_MODULE_PATH",
            "Ljavax/tools/StandardLocation;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "SYSTEM_MODULES",
            "Ljavax/tools/StandardLocation;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "UPGRADE_MODULE_PATH",
            "Ljavax/tools/StandardLocation;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "NATIVE_HEADER_OUTPUT",
            "Ljavax/tools/StandardLocation;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "DOCUMENTATION",
            "Ljavax/tools/StandardLocation;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "PLATFORM_CLASS_PATH",
            "Ljavax/tools/StandardLocation;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "OUTPUT_DIRECTORY",
            "Ljavax/tools/StandardLocation;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "GENERIC",
            "Ljavax/tools/StandardLocation;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "MODULE_EXPORTS",
            "Ljavax/tools/StandardLocation;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "RELEASE",
            "Ljavax/tools/StandardLocation;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "OTHER",
            "Ljavax/tools/StandardLocation;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "SNIPPET_PATH",
            "Ljavax/tools/StandardLocation;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "SOURCE_VERSION",
            "Ljavax/tools/StandardLocation;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "DOCLET_PATH",
            "Ljavax/tools/StandardLocation;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "$VALUES",
            "[Ljavax/tools/StandardLocation;",
            ACC_PRIVATE | ACC_STATIC | ACC_FINAL | ACC_SYNTHETIC,
            None,
        ),
    ],
    &STANDARD_LOCATION_METHODS,
    Some(standard_location_clinit),
);

const STANDARD_LOCATION_NAMES: [&str; 22] = [
    "CLASS_OUTPUT",
    "SOURCE_OUTPUT",
    "CLASS_PATH",
    "SOURCE_PATH",
    "ANNOTATION_PROCESSOR_PATH",
    "ANNOTATION_PROCESSOR_MODULE_PATH",
    "MODULE_SOURCE_PATH",
    "MODULE_PATH",
    "PATCH_MODULE_PATH",
    "SYSTEM_MODULES",
    "UPGRADE_MODULE_PATH",
    "NATIVE_HEADER_OUTPUT",
    "DOCUMENTATION",
    "PLATFORM_CLASS_PATH",
    "OUTPUT_DIRECTORY",
    "GENERIC",
    "MODULE_EXPORTS",
    "RELEASE",
    "OTHER",
    "SNIPPET_PATH",
    "SOURCE_VERSION",
    "DOCLET_PATH",
];

const STANDARD_LOCATION_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
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
        "getName",
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
];

fn standard_location_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("javax/tools/StandardLocation")?;
    let mut built = Vec::with_capacity(STANDARD_LOCATION_NAMES.len());
    for (ordinal, name) in STANDARD_LOCATION_NAMES.iter().enumerate() {
        let instance = vm.new_instance(class)?;
        let name_ref = vm.make_string(name)?;
        let (decl, ctor) = vm
            .find_method(class, "<init>", "(Ljava/lang/String;I)V")
            .ok_or_else(|| VmError::internal("StandardLocation lacks ctor"))?;
        vm.invoke_method(
            decl,
            ctor,
            vec![
                Value::Ref(instance),
                Value::Ref(name_ref),
                Value::Int(ordinal as i32),
            ],
        )?;
        vm.set_static_value(
            class,
            name,
            "Ljavax/tools/StandardLocation;",
            Value::Ref(instance),
        );
        built.push(Value::Ref(instance));
    }
    let array = vm.allocate_array_of(ArrayComponent::Reference, built.len())?;
    for (i, v) in built.into_iter().enumerate() {
        vm.array_set(array, i as i32, v, false)?;
    }
    vm.set_static_value(
        class,
        "$VALUES",
        "[Ljavax/tools/StandardLocation;",
        Value::Ref(array),
    );
    Ok(())
}

// --------------------------------------------------------------------------
// Diagnostic$Kind
// --------------------------------------------------------------------------

pub(crate) const DIAGNOSTIC_KIND: oxjvm_vm::NativeClass = class(
    "javax/tools/Diagnostic$Kind",
    Some("java/lang/Enum"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_ENUM,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE | ACC_FINAL, None),
        field("ordinal", "I", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "$VALUES",
            "[Ljavax/tools/Diagnostic$Kind;",
            ACC_PRIVATE | ACC_STATIC | ACC_FINAL | ACC_SYNTHETIC,
            None,
        ),
    ],
    &DIAGNOSTIC_KIND_METHODS,
    Some(diagnostic_kind_clinit),
);

const DIAGNOSTIC_KIND_NAMES: [&str; 5] = ["ERROR", "WARNING", "MANDATORY_WARNING", "NOTE", "OTHER"];

const DIAGNOSTIC_KIND_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
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
];

fn diagnostic_kind_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("javax/tools/Diagnostic$Kind")?;
    let mut built = Vec::with_capacity(DIAGNOSTIC_KIND_NAMES.len());
    for (ordinal, name) in DIAGNOSTIC_KIND_NAMES.iter().enumerate() {
        let instance = vm.new_instance(class)?;
        let name_ref = vm.make_string(name)?;
        let (decl, ctor) = vm
            .find_method(class, "<init>", "(Ljava/lang/String;I)V")
            .ok_or_else(|| VmError::internal("Diagnostic$Kind lacks ctor"))?;
        vm.invoke_method(
            decl,
            ctor,
            vec![
                Value::Ref(instance),
                Value::Ref(name_ref),
                Value::Int(ordinal as i32),
            ],
        )?;
        vm.set_static_value(
            class,
            name,
            "Ljavax/tools/Diagnostic$Kind;",
            Value::Ref(instance),
        );
        built.push(Value::Ref(instance));
    }
    let array = vm.allocate_array_of(ArrayComponent::Reference, built.len())?;
    for (i, v) in built.into_iter().enumerate() {
        vm.array_set(array, i as i32, v, false)?;
    }
    vm.set_static_value(
        class,
        "$VALUES",
        "[Ljavax/tools/Diagnostic$Kind;",
        Value::Ref(array),
    );
    Ok(())
}

// --------------------------------------------------------------------------
// SimpleJavaFileObject
// --------------------------------------------------------------------------

pub(crate) const SIMPLE_JAVA_FILE_OBJECT: oxjvm_vm::NativeClass = class(
    "javax/tools/SimpleJavaFileObject",
    Some("java/lang/Object"),
    &["javax/tools/JavaFileObject", "javax/tools/FileObject"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[
        field("uri", "Ljava/net/URI;", ACC_PROTECTED, None),
        field(
            "kind",
            "Ljavax/tools/JavaFileObject$Kind;",
            ACC_PROTECTED,
            None,
        ),
        field("content", "Ljava/lang/String;", ACC_PROTECTED, None),
    ],
    &SIMPLE_JAVA_FILE_OBJECT_METHODS,
    None,
);

const SIMPLE_JAVA_FILE_OBJECT_METHODS: [oxjvm_vm::NativeMethodDef; 15] = [
    method(
        "<init>",
        "(Ljava/net/URI;Ljavax/tools/JavaFileObject$Kind;)V",
        ACC_PROTECTED,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let uri = ref_arg(args, 1);
            let kind = ref_arg(args, 2);
            if !uri.is_null() {
                let (d, f) = vm.find_field(class, "uri", "Ljava/net/URI;")?;
                vm.set_instance_ref(this, d, f, uri);
            }
            if !kind.is_null() {
                let (d, f) = vm.find_field(class, "kind", "Ljavax/tools/JavaFileObject$Kind;")?;
                vm.set_instance_ref(this, d, f, kind);
            }
            void()
        },
    ),
    method(
        "getKind",
        "()Ljavax/tools/JavaFileObject$Kind;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            object(
                vm.read_ref_field(this, class, "kind", "Ljavax/tools/JavaFileObject$Kind;")
                    .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method("toUri", "()Ljava/net/URI;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        object(
            vm.read_ref_field(this, class, "uri", "Ljava/net/URI;")
                .unwrap_or(ObjectRef::NULL),
        )
    }),
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            // fall back: use uri.toString if uri field exists
            let uri_ref = vm
                .read_ref_field(this, class, "uri", "Ljava/net/URI;")
                .unwrap_or(ObjectRef::NULL);
            if !uri_ref.is_null() {
                let (uc, um) =
                    vm.resolve_virtual_method(uri_ref, "toString", "()Ljava/lang/String;")?;
                let v = vm.invoke_method(uc, um, vec![Value::Ref(uri_ref)])?;
                return Ok(v);
            }
            object(vm.make_string("")?)
        },
    ),
    method(
        "isNameCompatible",
        "(Ljava/lang/String;Ljavax/tools/JavaFileObject$Kind;)Z",
        ACC_PUBLIC,
        |_vm, _, _args| boolean(true),
    ),
    method("delete", "()Z", ACC_PUBLIC, |_vm, _, _args| boolean(false)),
    method("getLastModified", "()J", ACC_PUBLIC, |_vm, _, _args| {
        Ok(Value::Long(0))
    }),
    method(
        "getCharContent",
        "(Z)Ljava/lang/CharSequence;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let content = vm
                .read_ref_field(this, class, "content", "Ljava/lang/String;")
                .unwrap_or(ObjectRef::NULL);
            if !content.is_null() {
                return object(content);
            }
            // file-backed object: read contents from uri if present
            let uri_ref = vm
                .read_ref_field(this, class, "uri", "Ljava/net/URI;")
                .unwrap_or(ObjectRef::NULL);
            if uri_ref.is_null() {
                return object(vm.make_string("")?);
            }
            let uri_text = vm
                .read_string_field(uri_ref, vm.class_of(uri_ref), "string")
                .unwrap_or_default();
            // strip scheme
            let path = if let Some(idx) = uri_text.find(':') {
                &uri_text[idx + 1..]
            } else {
                uri_text.as_str()
            };
            let bytes = vm
                .host
                .filesystem()
                .and_then(|fs| fs.read(path))
                .unwrap_or_default();
            let text = String::from_utf8_lossy(&bytes).into_owned();
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "openInputStream",
        "()Ljava/io/InputStream;",
        ACC_PUBLIC,
        |vm, _, _args| Err(vm.throw_new("java/lang/UnsupportedOperationException", None)),
    ),
    method(
        "openOutputStream",
        "()Ljava/io/OutputStream;",
        ACC_PUBLIC,
        |vm, _, _args| Err(vm.throw_new("java/lang/UnsupportedOperationException", None)),
    ),
    method(
        "openReader",
        "(Z)Ljava/io/Reader;",
        ACC_PUBLIC,
        |vm, _, _args| Err(vm.throw_new("java/lang/UnsupportedOperationException", None)),
    ),
    method(
        "openWriter",
        "()Ljava/io/Writer;",
        ACC_PUBLIC,
        |vm, _, _args| Err(vm.throw_new("java/lang/UnsupportedOperationException", None)),
    ),
    method(
        "getNestingKind",
        "()Ljavax/lang/model/element/NestingKind;",
        ACC_PUBLIC,
        |_vm, _, _args| object(ObjectRef::NULL),
    ),
    method(
        "getAccessLevel",
        "()Ljavax/lang/model/element/Modifier;",
        ACC_PUBLIC,
        |_vm, _, _args| object(ObjectRef::NULL),
    ),
    method(
        "forSource",
        "(Ljava/net/URI;Ljava/lang/String;)Ljavax/tools/JavaFileObject;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let uri = ref_arg(args, 0);
            let text = ref_arg(args, 1);
            let sv = vm.resolve_class("javax/tools/SimpleJavaFileObject")?;
            let instance = vm.new_instance(sv)?;
            let (d, f) = vm.find_field(sv, "uri", "Ljava/net/URI;")?;
            vm.set_instance_ref(instance, d, f, uri);
            let kind_class = vm.resolve_class("javax/tools/JavaFileObject$Kind")?;
            vm.ensure_initialized(kind_class)?;
            let kind_ref = vm
                .read_static_ref(kind_class, "SOURCE", "Ljavax/tools/JavaFileObject$Kind;")
                .ok_or_else(|| VmError::internal("no SOURCE"))?;
            let (d2, f2) = vm.find_field(sv, "kind", "Ljavax/tools/JavaFileObject$Kind;")?;
            vm.set_instance_ref(instance, d2, f2, kind_ref);
            if !text.is_null() {
                let (d3, f3) = vm.find_field(sv, "content", "Ljava/lang/String;")?;
                vm.set_instance_ref(instance, d3, f3, text);
            }
            object(instance)
        },
    ),
];
// Note: SimpleJavaFileObject needs uri/kind fields populated by subclass constructors.
// The abstract JavaFileObject interface below is registered as a pure interface.

// --------------------------------------------------------------------------
// Pure interface shells (remove NoClassDefFoundError)
// --------------------------------------------------------------------------

const TOOL_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method(
        "run",
        "(Ljava/io/InputStream;Ljava/io/OutputStream;Ljava/io/OutputStream;[Ljava/lang/String;)I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getSourceVersions",
        "()Ljava/util/Set;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "name",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const TOOL: oxjvm_vm::NativeClass = class(
    "javax/tools/Tool",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &TOOL_METHODS,
    None,
);

const FILE_OBJECT_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method(
        "toUri",
        "()Ljava/net/URI;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "openInputStream",
        "()Ljava/io/InputStream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "openOutputStream",
        "()Ljava/io/OutputStream;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "openReader",
        "(Z)Ljava/io/Reader;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getCharContent",
        "(Z)Ljava/lang/CharSequence;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "openWriter",
        "()Ljava/io/Writer;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getLastModified",
        "()J",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "delete",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const FILE_OBJECT: oxjvm_vm::NativeClass = class(
    "javax/tools/FileObject",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &FILE_OBJECT_METHODS,
    None,
);

const JAVA_FILE_OBJECT_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "getKind",
        "()Ljavax/tools/JavaFileObject$Kind;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "isNameCompatible",
        "(Ljava/lang/String;Ljavax/tools/JavaFileObject$Kind;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getNestingKind",
        "()Ljavax/lang/model/element/NestingKind;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getAccessLevel",
        "()Ljavax/lang/model/element/Modifier;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const JAVA_FILE_OBJECT: oxjvm_vm::NativeClass = class(
    "javax/tools/JavaFileObject",
    Some("java/lang/Object"),
    &["javax/tools/FileObject"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &JAVA_FILE_OBJECT_METHODS,
    None,
);

pub(crate) const JAVA_FILE_MANAGER: oxjvm_vm::NativeClass = class(
    "javax/tools/JavaFileManager",
    Some("java/lang/Object"),
    &["javax/tools/OptionChecker", "java/io/Closeable"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

const STANDARD_JAVA_FILE_MANAGER_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "setLocation",
        "(Ljavax/tools/JavaFileManager$Location;Ljava/lang/Iterable;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getLocation",
        "(Ljavax/tools/JavaFileManager$Location;)Ljava/lang/Iterable;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getJavaFileObjects",
        "([Ljava/io/File;)Ljava/lang/Iterable;",
        ACC_PUBLIC | ACC_ABSTRACT | ACC_VARARGS,
        crate::lang::abstract_method_pub,
    ),
    method(
        "close",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const STANDARD_JAVA_FILE_MANAGER: oxjvm_vm::NativeClass = class(
    "javax/tools/StandardJavaFileManager",
    Some("java/lang/Object"),
    &["javax/tools/JavaFileManager"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &STANDARD_JAVA_FILE_MANAGER_METHODS,
    None,
);

pub(crate) const JAVA_COMPILER: oxjvm_vm::NativeClass = class(
    "javax/tools/JavaCompiler",
    Some("java/lang/Object"),
    &["javax/tools/Tool", "javax/tools/OptionChecker"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &JAVA_COMPILER_METHODS,
    None,
);

const JAVA_COMPILER_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "getStandardFileManager",
        "(Ljavax/tools/DiagnosticListener;Ljava/util/Locale;Ljava/nio/charset/Charset;)Ljavax/tools/StandardJavaFileManager;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getTask",
        "(Ljava/io/Writer;Ljavax/tools/JavaFileManager;Ljavax/tools/DiagnosticListener;Ljava/lang/Iterable;Ljava/lang/Iterable;Ljava/lang/Iterable;)Ljavax/tools/JavaCompiler$CompilationTask;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

const DOCUMENTATION_TOOL_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "getStandardFileManager",
        "(Ljavax/tools/DiagnosticListener;Ljava/util/Locale;Ljava/nio/charset/Charset;)Ljavax/tools/StandardJavaFileManager;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getTask",
        "(Ljava/io/Writer;Ljavax/tools/JavaFileManager;Ljavax/tools/DiagnosticListener;Ljava/lang/Class;Ljava/lang/Iterable;Ljava/lang/Iterable;)Ljavax/tools/DocumentationTool$DocumentationTask;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const DOCUMENTATION_TOOL: oxjvm_vm::NativeClass = class(
    "javax/tools/DocumentationTool",
    Some("java/lang/Object"),
    &["javax/tools/Tool", "javax/tools/OptionChecker"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &DOCUMENTATION_TOOL_METHODS,
    None,
);

pub(crate) const DIAGNOSTIC: oxjvm_vm::NativeClass = class(
    "javax/tools/Diagnostic",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const DIAGNOSTIC_LISTENER: oxjvm_vm::NativeClass = class(
    "javax/tools/DiagnosticListener",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

const OPTION_CHECKER_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "isSupportedOption",
    "(Ljava/lang/String;)I",
    ACC_PUBLIC | ACC_ABSTRACT,
    crate::lang::abstract_method_pub,
)];

pub(crate) const OPTION_CHECKER: oxjvm_vm::NativeClass = class(
    "javax/tools/OptionChecker",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &OPTION_CHECKER_METHODS,
    None,
);

pub(crate) const FORWARDING_JAVA_FILE_MANAGER: oxjvm_vm::NativeClass = class(
    "javax/tools/ForwardingJavaFileManager",
    Some("java/lang/Object"),
    &["javax/tools/JavaFileManager"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[field(
        "fileManager",
        "Ljavax/tools/JavaFileManager;",
        ACC_PROTECTED,
        None,
    )],
    &[],
    None,
);

pub(crate) const FORWARDING_JAVA_FILE_OBJECT: oxjvm_vm::NativeClass = class(
    "javax/tools/ForwardingJavaFileObject",
    Some("java/lang/Object"),
    &["javax/tools/JavaFileObject"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[field(
        "fileObject",
        "Ljavax/tools/JavaFileObject;",
        ACC_PROTECTED,
        None,
    )],
    &[],
    None,
);

pub(crate) const JAVA_COMPILER_COMPILATION_TASK: oxjvm_vm::NativeClass = class(
    "javax/tools/JavaCompiler$CompilationTask",
    Some("java/lang/Object"),
    &["java/util/concurrent/Callable"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &JAVA_COMPILER_COMPILATION_TASK_METHODS,
    None,
);

const JAVA_COMPILER_COMPILATION_TASK_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "call",
        "()Ljava/lang/Boolean;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "setProcessors",
        "(Ljava/lang/Iterable;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "addTaskListener",
        "(Lcom/sun/source/util/TaskListener;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "setTaskListener",
        "(Lcom/sun/source/util/TaskListener;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const TASK_LISTENER: oxjvm_vm::NativeClass = class(
    "com/sun/source/util/TaskListener",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const TASK_EVENT: oxjvm_vm::NativeClass = class(
    "com/sun/source/util/TaskEvent",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

const DOCUMENTATION_TOOL_DOCUMENTATION_TASK_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "call",
    "()Ljava/lang/Boolean;",
    ACC_PUBLIC | ACC_ABSTRACT,
    crate::lang::abstract_method_pub,
)];

pub(crate) const DOCUMENTATION_TOOL_DOCUMENTATION_TASK: oxjvm_vm::NativeClass = class(
    "javax/tools/DocumentationTool$DocumentationTask",
    Some("java/lang/Object"),
    &["java/util/concurrent/Callable"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &DOCUMENTATION_TOOL_DOCUMENTATION_TASK_METHODS,
    None,
);

pub(crate) const DIAGNOSTIC_COLLECTOR: oxjvm_vm::NativeClass = class(
    "javax/tools/DiagnosticCollector",
    Some("java/lang/Object"),
    &["javax/tools/DiagnosticListener"],
    ACC_PUBLIC,
    &[field("diagnostics", "Ljava/util/List;", ACC_PRIVATE, None)],
    &[],
    None,
);

// --------------------------------------------------------------------------
// ToolProvider
// --------------------------------------------------------------------------

pub(crate) const TOOL_PROVIDER: oxjvm_vm::NativeClass = class(
    "javax/tools/ToolProvider",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &TOOL_PROVIDER_METHODS,
    None,
);

const TOOL_PROVIDER_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "getSystemJavaCompiler",
        "()Ljavax/tools/JavaCompiler;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _args| {
            let class = vm.resolve_class("com/sun/tools/javac/api/JavacTool")?;
            // JavacTool.instance()
            let (c, m) = vm
                .find_method(class, "instance", "()Lcom/sun/tools/javac/api/JavacTool;")
                .ok_or_else(|| VmError::internal("no instance method"))?;
            vm.invoke_method(c, m, vec![])
        },
    ),
    method(
        "getSystemDocumentationTool",
        "()Ljavax/tools/DocumentationTool;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _args| {
            let class = vm.resolve_class("jdk/javadoc/internal/api/JavadocTool")?;
            let (c, m) = vm
                .find_method(
                    class,
                    "instance",
                    "()Ljdk/javadoc/internal/api/JavadocTool;",
                )
                .ok_or_else(|| VmError::internal("no instance method"))?;
            vm.invoke_method(c, m, vec![])
        },
    ),
];

// --------------------------------------------------------------------------
// com.sun.tools.javac.api.JavacTool
// --------------------------------------------------------------------------

pub(crate) const JAVAC_TOOL: oxjvm_vm::NativeClass = class(
    "com/sun/tools/javac/api/JavacTool",
    Some("java/lang/Object"),
    &["javax/tools/JavaCompiler", "javax/tools/OptionChecker"],
    ACC_PUBLIC,
    &[],
    &JAVAC_TOOL_METHODS,
    None,
);

fn javac_program(vm: &mut Vm<'_>) -> String {
    if let Some(jdk) = vm
        .host_property("test.boot.jdk")
        .or_else(|| vm.get_property("test.boot.jdk"))
    {
        if !jdk.is_empty() {
            return alloc::format!("{jdk}/bin/javac");
        }
    }
    if let Some(home) = vm.host_property("java.home") {
        if !home.is_empty() && home != "/oxjvm" {
            return alloc::format!("{home}/bin/javac");
        }
    }
    String::from("javac")
}

fn javadoc_program(vm: &mut Vm<'_>) -> String {
    if let Some(jdk) = vm
        .host_property("test.boot.jdk")
        .or_else(|| vm.get_property("test.boot.jdk"))
    {
        if !jdk.is_empty() {
            return alloc::format!("{jdk}/bin/javadoc");
        }
    }
    String::from("javadoc")
}

fn set_task_field(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: &str,
    descriptor: &str,
    value: ObjectRef,
) -> Result<(), VmError> {
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, name, descriptor)?;
    vm.set_instance_ref(this, declaring, field, value);
    Ok(())
}

const JAVAC_TOOL_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method(
        "instance",
        "()Lcom/sun/tools/javac/api/JavacTool;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _args| {
            // cache a singleton in a static field? just create a fresh one each call
            let class = vm.resolve_class("com/sun/tools/javac/api/JavacTool")?;
            let instance = vm.new_instance(class)?;
            object(instance)
        },
    ),
    method(
        "name",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _args| object(vm.make_string("javac")?),
    ),
    method(
        "getSourceVersions",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, _args| {
            // EnumSet.range(SourceVersion.RELEASE_3, latest)
            let sv = vm.resolve_class("javax/lang/model/SourceVersion")?;
            vm.ensure_initialized(sv)?;
            let values = vm
                .read_static_ref(sv, "$VALUES", "[Ljavax/lang/model/SourceVersion;")
                .ok_or_else(|| VmError::internal("no $VALUES"))?;
            let length = vm.array_length(values)?;
            // build a HashSet containing RELEASE_3..last
            let hash_set = vm.resolve_class("java/util/HashSet")?;
            let set_instance = vm.new_instance(hash_set)?;
            let (decl, ctor) = vm
                .find_method(hash_set, "<init>", "()V")
                .ok_or_else(|| VmError::internal("HashSet lacks ctor"))?;
            vm.invoke_method(decl, ctor, vec![Value::Ref(set_instance)])?;
            let from_index = 3usize; // RELEASE_3
            for index in from_index..length {
                let element = vm.array_get(values, index as i32)?.as_ref();
                let (c, m) =
                    vm.resolve_virtual_method(set_instance, "add", "(Ljava/lang/Object;)Z")?;
                let _ =
                    vm.invoke_method(c, m, vec![Value::Ref(set_instance), Value::Ref(element)])?;
            }
            object(set_instance)
        },
    ),
    method(
        "getSupportedOptions",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, _args| {
            let hash_set = vm.resolve_class("java/util/HashSet")?;
            let set_instance = vm.new_instance(hash_set)?;
            let (decl, ctor) = vm
                .find_method(hash_set, "<init>", "()V")
                .ok_or_else(|| VmError::internal("HashSet lacks ctor"))?;
            vm.invoke_method(decl, ctor, vec![Value::Ref(set_instance)])?;
            object(set_instance)
        },
    ),
    method(
        "isSupportedOption",
        "(Ljava/lang/String;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let option = vm.string_or_empty(ref_arg(args, 1));
            if option.starts_with('-') {
                int(0)
            } else {
                int(-1)
            }
        },
    ),
    method(
        "create",
        "()Lcom/sun/tools/javac/api/JavacTool;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("com/sun/tools/javac/api/JavacTool")?;
            object(vm.new_instance(class)?)
        },
    ),
    method(
        "getStandardFileManager",
        "(Ljavax/tools/DiagnosticListener;Ljava/util/Locale;Ljava/nio/charset/Charset;)Ljavax/tools/StandardJavaFileManager;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("com/sun/tools/javac/file/JavacFileManager")?;
            let instance = vm.new_instance(class)?;
            if let Some((declaring, ctor)) = vm.find_method(class, "<init>", "()V") {
                vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
            }
            object(instance)
        },
    ),
    method(
        "run",
        "(Ljava/io/InputStream;Ljava/io/OutputStream;Ljava/io/OutputStream;[Ljava/lang/String;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let args_array = ref_arg(args, 4);
            let mut cmd: Vec<String> = vec![javac_program(vm)];
            if !args_array.is_null() {
                let length = vm.array_length(args_array)?;
                for index in 0..length {
                    let element = vm.array_get(args_array, index as i32)?.as_ref();
                    cmd.push(vm.string_or_empty(element));
                }
            }
            match vm.host.run_process(&cmd[0], &cmd[1..], None) {
                Ok(result) => {
                    let out = ref_arg(args, 2);
                    let err = ref_arg(args, 3);
                    if !out.is_null() && !result.stdout.is_empty() {
                        if let Ok((c, m)) = vm.resolve_virtual_method(out, "write", "([B)V") {
                            let bytes = vm.allocate_array_of(
                                ArrayComponent::Byte,
                                result.stdout.len(),
                            )?;
                            for (index, byte) in result.stdout.iter().enumerate() {
                                vm.array_set(
                                    bytes,
                                    index as i32,
                                    Value::Int(i32::from(*byte)),
                                    false,
                                )?;
                            }
                            vm.invoke_method(c, m, vec![Value::Ref(out), Value::Ref(bytes)])?;
                        }
                    }
                    if !err.is_null() && !result.stderr.is_empty() {
                        if let Ok((c, m)) = vm.resolve_virtual_method(err, "write", "([B)V") {
                            let bytes = vm.allocate_array_of(
                                ArrayComponent::Byte,
                                result.stderr.len(),
                            )?;
                            for (index, byte) in result.stderr.iter().enumerate() {
                                vm.array_set(
                                    bytes,
                                    index as i32,
                                    Value::Int(i32::from(*byte)),
                                    false,
                                )?;
                            }
                            vm.invoke_method(c, m, vec![Value::Ref(err), Value::Ref(bytes)])?;
                        }
                    }
                    Ok(Value::Int(result.status))
                }
                Err(_) => Ok(Value::Int(1)),
            }
        },
    ),
    method(
        "getTask",
        "(Ljava/io/Writer;Ljavax/tools/JavaFileManager;Ljavax/tools/DiagnosticListener;Ljava/lang/Iterable;Ljava/lang/Iterable;Ljava/lang/Iterable;)Ljavax/tools/JavaCompiler$CompilationTask;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = vm.resolve_class("com/sun/tools/javac/api/JavacTaskImpl")?;
            let instance = vm.new_instance(class)?;
            if let Some((declaring, ctor)) = vm.find_method(class, "<init>", "()V") {
                vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
            }
            set_task_field(vm, instance, "writer", "Ljava/io/Writer;", ref_arg(args, 1))?;
            set_task_field(vm, instance, "fileManager", "Ljava/lang/Object;", ref_arg(args, 2))?;
            set_task_field(
                vm,
                instance,
                "diagnosticListener",
                "Ljava/lang/Object;",
                ref_arg(args, 3),
            )?;
            set_task_field(vm, instance, "options", "Ljava/lang/Object;", ref_arg(args, 4))?;
            set_task_field(vm, instance, "classes", "Ljava/lang/Object;", ref_arg(args, 5))?;
            set_task_field(
                vm,
                instance,
                "compilationUnits",
                "Ljava/lang/Object;",
                ref_arg(args, 6),
            )?;
            object(instance)
        },
    ),
];

// --------------------------------------------------------------------------
// com.sun.tools.javac.Main
// --------------------------------------------------------------------------

pub(crate) const JAVAC_MAIN: oxjvm_vm::NativeClass = class(
    "com/sun/tools/javac/Main",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &JAVAC_MAIN_METHODS,
    None,
);

const JAVAC_MAIN_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method(
        "compile",
        "([Ljava/lang/String;)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let args_array = ref_arg(args, 0);
            let mut cmd: Vec<String> = vec![javac_program(vm)];
            if !args_array.is_null() {
                let length = vm.array_length(args_array)?;
                for i in 0..length {
                    let element = vm.array_get(args_array, i as i32)?.as_ref();
                    cmd.push(vm.string_or_empty(element));
                }
            }
            let result = vm.host.run_process(&cmd[0], &cmd[1..], None);
            match result {
                Ok(r) => Ok(Value::Int(r.status)),
                Err(_) => Ok(Value::Int(1)),
            }
        },
    ),
    method(
        "compile",
        "([Ljava/lang/String;Ljava/io/PrintWriter;)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let args_array = ref_arg(args, 0);
            let mut cmd: Vec<String> = vec![javac_program(vm)];
            if !args_array.is_null() {
                let length = vm.array_length(args_array)?;
                for i in 0..length {
                    let element = vm.array_get(args_array, i as i32)?.as_ref();
                    cmd.push(vm.string_or_empty(element));
                }
            }
            let result = vm.host.run_process(&cmd[0], &cmd[1..], None);
            match result {
                Ok(r) => Ok(Value::Int(r.status)),
                Err(_) => Ok(Value::Int(1)),
            }
        },
    ),
    method(
        "main",
        "([Ljava/lang/String;)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let args_array = ref_arg(args, 0);
            let mut cmd: Vec<String> = vec![javac_program(vm)];
            if !args_array.is_null() {
                let length = vm.array_length(args_array)?;
                for i in 0..length {
                    let element = vm.array_get(args_array, i as i32)?.as_ref();
                    cmd.push(vm.string_or_empty(element));
                }
            }
            let _ = vm.host.run_process(&cmd[0], &cmd[1..], None);
            void()
        },
    ),
];

// --------------------------------------------------------------------------
// com.sun.tools.javac.util.Context
// --------------------------------------------------------------------------

pub(crate) const JAVAC_CONTEXT: oxjvm_vm::NativeClass = class(
    "com/sun/tools/javac/util/Context",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &[method("<init>", "()V", ACC_PUBLIC, |_, _, _| void())],
    None,
);

// --------------------------------------------------------------------------
// registration helper (called from lib.rs)
// --------------------------------------------------------------------------
// (registration happens in lib.rs's REGISTRY array)

// --------------------------------------------------------------------------
// jdk.javadoc.internal.api.JavadocTool (also serves as the DocumentationTool
// returned by ToolProvider so the metadata tests stop being blocked on NCDFE).
// --------------------------------------------------------------------------

pub(crate) const JAVADOC_TOOL: oxjvm_vm::NativeClass = class(
    "jdk/javadoc/internal/api/JavadocTool",
    Some("java/lang/Object"),
    &["javax/tools/DocumentationTool", "javax/tools/OptionChecker"],
    ACC_PUBLIC,
    &[],
    &JAVADOC_TOOL_METHODS,
    None,
);

const JAVADOC_TOOL_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "instance",
        "()Ljdk/javadoc/internal/api/JavadocTool;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _args| {
            let class = vm.resolve_class("jdk/javadoc/internal/api/JavadocTool")?;
            let instance = vm.new_instance(class)?;
            object(instance)
        },
    ),
    method(
        "name",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _args| object(vm.make_string("javadoc")?),
    ),
    method(
        "getSourceVersions",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, _args| {
            let sv = vm.resolve_class("javax/lang/model/SourceVersion")?;
            vm.ensure_initialized(sv)?;
            let values = vm
                .read_static_ref(sv, "$VALUES", "[Ljavax/lang/model/SourceVersion;")
                .ok_or_else(|| VmError::internal("no $VALUES"))?;
            let length = vm.array_length(values)?;
            let hash_set = vm.resolve_class("java/util/HashSet")?;
            let set_instance = vm.new_instance(hash_set)?;
            let (decl, ctor) = vm
                .find_method(hash_set, "<init>", "()V")
                .ok_or_else(|| VmError::internal("HashSet lacks ctor"))?;
            vm.invoke_method(decl, ctor, vec![Value::Ref(set_instance)])?;
            let from_index = 3usize;
            for index in from_index..length {
                let element = vm.array_get(values, index as i32)?.as_ref();
                let (c, m) =
                    vm.resolve_virtual_method(set_instance, "add", "(Ljava/lang/Object;)Z")?;
                let _ =
                    vm.invoke_method(c, m, vec![Value::Ref(set_instance), Value::Ref(element)])?;
            }
            object(set_instance)
        },
    ),
    method(
        "getSupportedOptions",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, _args| {
            let hash_set = vm.resolve_class("java/util/HashSet")?;
            let set_instance = vm.new_instance(hash_set)?;
            let (decl, ctor) = vm
                .find_method(hash_set, "<init>", "()V")
                .ok_or_else(|| VmError::internal("HashSet lacks ctor"))?;
            vm.invoke_method(decl, ctor, vec![Value::Ref(set_instance)])?;
            object(set_instance)
        },
    ),
    method(
        "isSupportedOption",
        "(Ljava/lang/String;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let option = vm.string_or_empty(ref_arg(args, 1));
            if option.starts_with('-') {
                int(0)
            } else {
                int(-1)
            }
        },
    ),
    method(
        "getStandardFileManager",
        "(Ljavax/tools/DiagnosticListener;Ljava/util/Locale;Ljava/nio/charset/Charset;)Ljavax/tools/StandardJavaFileManager;",
        ACC_PUBLIC,
        |vm, _, _args| {
            let class = vm.resolve_class("com/sun/tools/javac/file/JavacFileManager")?;
            let instance = vm.new_instance(class)?;
            // init
            let (c, m) = vm
                .find_method(class, "<init>", "()V")
                .ok_or_else(|| VmError::internal("no ctor"))?;
            vm.invoke_method(c, m, vec![Value::Ref(instance)])?;
            object(instance)
        },
    ),
    method(
        "getTask",
        "(Ljava/io/Writer;Ljavax/tools/JavaFileManager;Ljavax/tools/DiagnosticListener;Ljava/lang/Class;Ljava/lang/Iterable;Ljava/lang/Iterable;)Ljavax/tools/DocumentationTool$DocumentationTask;",
        ACC_PUBLIC,
        |vm, _, args| {
            let writer_ref = ref_arg(args, 1);
            let file_manager = ref_arg(args, 2);
            let diagnostic_listener = ref_arg(args, 3);
            let doclet = ref_arg(args, 4);
            let options_ref = ref_arg(args, 5);
            let files_ref = ref_arg(args, 6);
            if !options_ref.is_null() {
                let elems = crate::util::collection_elements(vm, options_ref)?;
                for elem in elems {
                    if elem.is_null() {
                        return Err(vm.throw_new("java/lang/NullPointerException", None));
                    }
                }
            }
            // validate source file kinds
            if !files_ref.is_null() {
                let elems = crate::util::collection_elements(vm, files_ref)?;
                for elem in elems {
                    if elem.is_null() {
                        return Err(vm.throw_new("java/lang/NullPointerException", None));
                    }
                    // reject non-source files
                    let source_kind = {
                        let kind_class = vm.resolve_class("javax/tools/JavaFileObject$Kind")?;
                        vm.ensure_initialized(kind_class)?;
                        vm.read_static_ref_if_set(
                            kind_class,
                            "SOURCE",
                            "Ljavax/tools/JavaFileObject$Kind;",
                        )
                    };
                    if let Some(source_kind_ref) = source_kind {
                        let elem_kind = if let Ok((c, m)) = vm.resolve_virtual_method(
                            elem,
                            "getKind",
                            "()Ljavax/tools/JavaFileObject$Kind;",
                        ) {
                            vm.invoke_method(c, m, vec![Value::Ref(elem)])
                                .ok()
                                .map(|v| v.as_ref())
                        } else {
                            None
                        };
                        if let Some(kind_ref) = elem_kind {
                            if kind_ref.raw() != source_kind_ref.raw() {
                                return Err(vm.throw_new(
                                    "java/lang/IllegalArgumentException",
                                    Some("bad file"),
                                ));
                            }
                        }
                    }
                }
            }
            let class = vm.resolve_class("jdk/javadoc/internal/api/JavadocTaskImpl")?;
            let instance = vm.new_instance(class)?;
            let (c, m) = vm
                .find_method(class, "<init>", "()V")
                .ok_or_else(|| VmError::internal("no ctor"))?;
            vm.invoke_method(c, m, vec![Value::Ref(instance)])?;
            // set fields
            // simpler sequential:
            if writer_ref.is_null() {
            } else {
                let (d, f) = vm.find_field(vm.class_of(instance), "writer", "Ljava/io/Writer;")?;
                vm.set_instance_ref(instance, d, f, writer_ref);
            }
            {
                let (d, f) =
                    vm.find_field(vm.class_of(instance), "fileManager", "Ljava/lang/Object;")?;
                vm.set_instance_ref(instance, d, f, file_manager);
            }
            {
                let (d, f) = vm.find_field(
                    vm.class_of(instance),
                    "diagnosticListener",
                    "Ljava/lang/Object;",
                )?;
                vm.set_instance_ref(instance, d, f, diagnostic_listener);
            }
            if !doclet.is_null() {
                let (d, f) = vm.find_field(vm.class_of(instance), "doclet", "Ljava/lang/Class;")?;
                vm.set_instance_ref(instance, d, f, doclet);
            }
            {
                let (d, f) =
                    vm.find_field(vm.class_of(instance), "options", "Ljava/lang/Object;")?;
                vm.set_instance_ref(instance, d, f, options_ref);
            }
            {
                let (d, f) = vm.find_field(vm.class_of(instance), "files", "Ljava/lang/Object;")?;
                vm.set_instance_ref(instance, d, f, files_ref);
            }
            object(instance)
        },
    ),
];

// --------------------------------------------------------------------------
// com.sun.tools.javac.file.JavacFileManager (a concrete StandardJavaFileManager)
// -------------------------------------------------------------------------- (a concrete StandardJavaFileManager)
// --------------------------------------------------------------------------

fn new_array_list(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/ArrayList")?;
    let list = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(class, "<init>", "()V")
        .ok_or_else(|| VmError::internal("ArrayList lacks <init>"))?;
    vm.invoke_method(declaring, ctor, vec![Value::Ref(list)])?;
    Ok(list)
}

fn list_add_value(vm: &mut Vm<'_>, list: ObjectRef, value: ObjectRef) -> Result<(), VmError> {
    let (class, method) = vm.resolve_virtual_method(list, "add", "(Ljava/lang/Object;)Z")?;
    vm.invoke_method(class, method, vec![Value::Ref(list), Value::Ref(value)])?;
    Ok(())
}

fn simple_java_file_object(vm: &mut Vm<'_>, file: ObjectRef) -> Result<ObjectRef, VmError> {
    let (tc, tm) = vm.resolve_virtual_method(file, "toURI", "()Ljava/net/URI;")?;
    let uri_ref = vm.invoke_method(tc, tm, vec![Value::Ref(file)])?.as_ref();
    let uri_text = vm
        .read_string_field(uri_ref, vm.class_of(uri_ref), "string")
        .unwrap_or_default();
    let kind_class = vm.resolve_class("javax/tools/JavaFileObject$Kind")?;
    vm.ensure_initialized(kind_class)?;
    let kind_name = if uri_text.ends_with(".class") {
        "CLASS"
    } else if uri_text.ends_with(".java") {
        "SOURCE"
    } else {
        "OTHER"
    };
    let kind_ref = vm
        .read_static_ref_if_set(kind_class, kind_name, "Ljavax/tools/JavaFileObject$Kind;")
        .ok_or_else(|| VmError::internal("no JavaFileObject kind"))?;
    let sfo_class = vm.resolve_class("javax/tools/SimpleJavaFileObject")?;
    let sfo = vm.new_instance(sfo_class)?;
    let (d, f) = vm.find_field(sfo_class, "uri", "Ljava/net/URI;")?;
    vm.set_instance_ref(sfo, d, f, uri_ref);
    let (d, f) = vm.find_field(sfo_class, "kind", "Ljavax/tools/JavaFileObject$Kind;")?;
    vm.set_instance_ref(sfo, d, f, kind_ref);
    Ok(sfo)
}

pub(crate) const JAVAC_FILE_MANAGER: oxjvm_vm::NativeClass = class(
    "com/sun/tools/javac/file/JavacFileManager",
    Some("java/lang/Object"),
    &["javax/tools/StandardJavaFileManager"],
    ACC_PUBLIC,
    &[field("output", "Ljava/io/File;", ACC_PRIVATE, None)],
    &JAVAC_FILE_MANAGER_METHODS,
    None,
);

const JAVAC_FILE_MANAGER_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "setLocation",
        "(Ljavax/tools/JavaFileManager$Location;Ljava/lang/Iterable;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let files_iter = ref_arg(args, 2);
            if !files_iter.is_null() {
                let elems = crate::util::collection_elements(vm, files_iter)?;
                if let Some(&first) = elems.first() {
                    if !first.is_null() {
                        let class = vm.class_of(this);
                        let (d, f) = vm.find_field(class, "output", "Ljava/io/File;")?;
                        vm.set_instance_ref(this, d, f, first);
                    }
                }
            }
            void()
        },
    ),
    method(
        "getLocation",
        "(Ljavax/tools/JavaFileManager$Location;)Ljava/lang/Iterable;",
        ACC_PUBLIC,
        |_vm, _, _args| object(ObjectRef::NULL),
    ),
    method(
        "getJavaFileObjects",
        "([Ljava/io/File;)Ljava/lang/Iterable;",
        ACC_PUBLIC | ACC_VARARGS,
        |vm, _, args| {
            let files_array = ref_arg(args, 1);
            let list_class = vm.resolve_class("java/util/ArrayList")?;
            let list = vm.new_instance(list_class)?;
            let (c, m) = vm
                .find_method(list_class, "<init>", "()V")
                .ok_or_else(|| VmError::internal("no ctor"))?;
            vm.invoke_method(c, m, vec![Value::Ref(list)])?;
            if !files_array.is_null() {
                let length = vm.array_length(files_array)?;
                let sfo_class = vm.resolve_class("javax/tools/SimpleJavaFileObject")?;
                let kind_class = vm.resolve_class("javax/tools/JavaFileObject$Kind")?;
                vm.ensure_initialized(kind_class)?;
                for i in 0..length {
                    let file_ref = vm.array_get(files_array, i as i32)?.as_ref();
                    if file_ref.is_null() {
                        continue;
                    }
                    // toURI
                    let (tc, tm) =
                        vm.resolve_virtual_method(file_ref, "toURI", "()Ljava/net/URI;")?;
                    let uri_ref = vm
                        .invoke_method(tc, tm, vec![Value::Ref(file_ref)])?
                        .as_ref();
                    // determine kind by extension
                    let uri_text = vm
                        .read_string_field(uri_ref, vm.class_of(uri_ref), "string")
                        .unwrap_or_default();
                    let kind_name = if uri_text.ends_with(".class") {
                        "CLASS"
                    } else if uri_text.ends_with(".java") {
                        "SOURCE"
                    } else {
                        "OTHER"
                    };
                    let kind_ref = vm
                        .read_static_ref_if_set(
                            kind_class,
                            kind_name,
                            "Ljavax/tools/JavaFileObject$Kind;",
                        )
                        .ok_or_else(|| VmError::internal("no kind"))?;
                    let sfo = vm.new_instance(sfo_class)?;
                    let (d, f) = vm.find_field(sfo_class, "uri", "Ljava/net/URI;")?;
                    vm.set_instance_ref(sfo, d, f, uri_ref);
                    let (d2, f2) =
                        vm.find_field(sfo_class, "kind", "Ljavax/tools/JavaFileObject$Kind;")?;
                    vm.set_instance_ref(sfo, d2, f2, kind_ref);
                    let (ac, am) =
                        vm.resolve_virtual_method(list, "add", "(Ljava/lang/Object;)Z")?;
                    let _ = vm.invoke_method(ac, am, vec![Value::Ref(list), Value::Ref(sfo)])?;
                }
            }
            object(list)
        },
    ),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("flush", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "preRegister",
        "(Lcom/sun/tools/javac/util/Context;)V",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| void(),
    ),
    method(
        "getJavaFileObjectsFromStrings",
        "(Ljava/lang/Iterable;)Ljava/lang/Iterable;",
        ACC_PUBLIC,
        |vm, _, args| {
            let strings = crate::util::collection_elements(vm, ref_arg(args, 1))?;
            let list = new_array_list(vm)?;
            for item in strings {
                let text = vm.string_or_empty(item);
                let file = crate::file::new_file(vm, &text)?.as_ref();
                let sfo = simple_java_file_object(vm, file)?;
                list_add_value(vm, list, sfo)?;
            }
            object(list)
        },
    ),
    method(
        "getJavaFileObjects",
        "([Ljava/lang/String;)Ljava/lang/Iterable;",
        ACC_PUBLIC | ACC_VARARGS,
        |vm, _, args| {
            let strings = ref_arg(args, 1);
            let list = new_array_list(vm)?;
            if !strings.is_null() {
                let length = vm.array_length(strings)?;
                for index in 0..length {
                    let text = vm
                        .array_get(strings, index as i32)
                        .map(|value| vm.string_or_empty(value.as_ref()))?;
                    let file = crate::file::new_file(vm, &text)?.as_ref();
                    let sfo = simple_java_file_object(vm, file)?;
                    list_add_value(vm, list, sfo)?;
                }
            }
            object(list)
        },
    ),
    method(
        "getJavaFileObjectsFromFiles",
        "(Ljava/lang/Iterable;)Ljava/lang/Iterable;",
        ACC_PUBLIC,
        |vm, _, args| {
            let files = crate::util::collection_elements(vm, ref_arg(args, 1))?;
            let list = new_array_list(vm)?;
            for file in files {
                let sfo = simple_java_file_object(vm, file)?;
                list_add_value(vm, list, sfo)?;
            }
            object(list)
        },
    ),
];

// --------------------------------------------------------------------------
// com.sun.tools.javac.api.JavacTaskImpl
// --------------------------------------------------------------------------

fn javac_task_call(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let this = receiver(args);
    let class = vm.class_of(this);
    let writer = vm
        .read_ref_field(this, class, "writer", "Ljava/io/Writer;")
        .unwrap_or(ObjectRef::NULL);
    let file_manager = vm
        .read_ref_field(this, class, "fileManager", "Ljava/lang/Object;")
        .unwrap_or(ObjectRef::NULL);
    let options_ref = vm
        .read_ref_field(this, class, "options", "Ljava/lang/Object;")
        .unwrap_or(ObjectRef::NULL);
    let units_ref = vm
        .read_ref_field(this, class, "compilationUnits", "Ljava/lang/Object;")
        .unwrap_or(ObjectRef::NULL);

    let mut cmd: Vec<String> = vec![javac_program(vm)];
    if !file_manager.is_null() {
        let fm_class = vm.class_of(file_manager);
        if let Some(output) = vm.read_ref_field(file_manager, fm_class, "output", "Ljava/io/File;") {
            if !output.is_null() {
                cmd.push("-d".to_string());
                cmd.push(crate::file::file_path(vm, output));
            }
        }
    }
    let options = if options_ref.is_null() {
        Vec::new()
    } else {
        crate::util::collection_elements(vm, options_ref).unwrap_or_default()
    };
    for option in options {
        if !option.is_null() {
            cmd.push(vm.string_or_empty(option));
        }
    }

    let mut sources: Vec<String> = Vec::new();
    let units = if units_ref.is_null() {
        Vec::new()
    } else {
        crate::util::collection_elements(vm, units_ref).unwrap_or_default()
    };
    if !units.is_empty() {
        let base = alloc::format!("/tmp/opencode/oxjvm-javac-{}", vm.host.nano_time());
        if let Some(fs) = vm.host.filesystem() {
            let _ = fs.create_dir_all(&base);
        }
        for (index, unit) in units.into_iter().enumerate() {
            if unit.is_null() {
                continue;
            }
            let content = match vm.resolve_virtual_method(
                unit,
                "getCharContent",
                "(Z)Ljava/lang/CharSequence;",
            ) {
                Ok((c, m)) => vm
                    .invoke_method(c, m, vec![Value::Ref(unit), Value::Int(1)])
                    .ok()
                    .map(|value| vm.string_or_empty(value.as_ref()))
                    .unwrap_or_default(),
                Err(_) => String::new(),
            };
            let name = match vm.resolve_virtual_method(unit, "getName", "()Ljava/lang/String;") {
                Ok((c, m)) => vm
                    .invoke_method(c, m, vec![Value::Ref(unit)])
                    .ok()
                    .map(|value| vm.string_or_empty(value.as_ref()))
                    .unwrap_or_default(),
                Err(_) => String::new(),
            };
            let basename = name.rsplit('/').next().unwrap_or("");
            let basename = if basename.ends_with(".java") {
                basename.to_string()
            } else {
                alloc::format!("Source{}.java", index)
            };
            let path = alloc::format!("{base}/{basename}");
            if let Some(fs) = vm.host.filesystem() {
                let _ = fs.write(&path, content.as_bytes(), false);
            }
            sources.push(path);
        }
    }
    cmd.extend(sources);

    let result = vm.host.run_process(&cmd[0], &cmd[1..], None);
    let (status, stdout, stderr) = match result {
        Ok(result) => (result.status, result.stdout, result.stderr),
        Err(_) => (1, Vec::new(), Vec::new()),
    };
    if !writer.is_null() {
        let text = alloc::format!(
            "{}{}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
        if !text.is_empty() {
            let message = vm.make_string(&text)?;
            let (wc, wm) =
                vm.resolve_virtual_method(writer, "write", "(Ljava/lang/String;)V")?;
            vm.invoke_method(wc, wm, vec![Value::Ref(writer), Value::Ref(message)])?;
        }
    }
    let bool_class = vm.resolve_class("java/lang/Boolean")?;
    vm.ensure_initialized(bool_class)?;
    let name = if status == 0 { "TRUE" } else { "FALSE" };
    let instance = vm
        .read_static_ref(bool_class, name, "Ljava/lang/Boolean;")
        .ok_or_else(|| VmError::internal("no Boolean constant"))?;
    Ok(Value::Ref(instance))
}

pub(crate) const JAVAC_TASK_IMPL: oxjvm_vm::NativeClass = class(
    "com/sun/tools/javac/api/JavacTaskImpl",
    Some("java/lang/Object"),
    &[
        "javax/tools/JavaCompiler$CompilationTask",
        "java/util/concurrent/Callable",
    ],
    ACC_PUBLIC,
    &[
        field("writer", "Ljava/io/Writer;", ACC_PRIVATE, None),
        field("fileManager", "Ljava/lang/Object;", ACC_PRIVATE, None),
        field(
            "diagnosticListener",
            "Ljava/lang/Object;",
            ACC_PRIVATE,
            None,
        ),
        field("options", "Ljava/lang/Object;", ACC_PRIVATE, None),
        field("classes", "Ljava/lang/Object;", ACC_PRIVATE, None),
        field("compilationUnits", "Ljava/lang/Object;", ACC_PRIVATE, None),
    ],
    &[
        method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
        method(
            "call",
            "()Ljava/lang/Boolean;",
            ACC_PUBLIC,
            |vm, _, args| javac_task_call(vm, args),
        ),
        method(
            "setProcessors",
            "(Ljava/lang/Iterable;)V",
            ACC_PUBLIC,
            |_, _, _| void(),
        ),
        method(
            "addTaskListener",
            "(Lcom/sun/source/util/TaskListener;)V",
            ACC_PUBLIC,
            |_, _, _| void(),
        ),
        method(
            "setTaskListener",
            "(Lcom/sun/source/util/TaskListener;)V",
            ACC_PUBLIC,
            |_, _, _| void(),
        ),
    ],
    None,
);

// --------------------------------------------------------------------------
// jdk.javadoc.internal.api.JavadocTaskImpl
// --------------------------------------------------------------------------

pub(crate) const JAVADOC_TASK_IMPL: oxjvm_vm::NativeClass = class(
    "jdk/javadoc/internal/api/JavadocTaskImpl",
    Some("java/lang/Object"),
    &[
        "javax/tools/DocumentationTool$DocumentationTask",
        "java/util/concurrent/Callable",
    ],
    ACC_PUBLIC,
    &[
        field("writer", "Ljava/io/Writer;", ACC_PRIVATE, None),
        field("fileManager", "Ljava/lang/Object;", ACC_PRIVATE, None),
        field(
            "diagnosticListener",
            "Ljava/lang/Object;",
            ACC_PRIVATE,
            None,
        ),
        field("doclet", "Ljava/lang/Class;", ACC_PRIVATE, None),
        field("options", "Ljava/lang/Object;", ACC_PRIVATE, None),
        field("files", "Ljava/lang/Object;", ACC_PRIVATE, None),
    ],
    &JAVADOC_TASK_IMPL_METHODS,
    None,
);

const JAVADOC_TASK_IMPL_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "call",
        "()Ljava/lang/Boolean;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            // extract fields
            let file_manager = vm
                .read_ref_field(this, class, "fileManager", "Ljava/lang/Object;")
                .unwrap_or(ObjectRef::NULL);
            let options_ref = vm
                .read_ref_field(this, class, "options", "Ljava/lang/Object;")
                .unwrap_or(ObjectRef::NULL);
            let files_ref = vm
                .read_ref_field(this, class, "files", "Ljava/lang/Object;")
                .unwrap_or(ObjectRef::NULL);

            // options list
            let options: Vec<ObjectRef> = if files_ref.is_null() {
                vec![]
            } else {
                crate::util::collection_elements(vm, options_ref).unwrap_or_default()
            };
            // output dir from file manager
            let mut output_dir: Option<String> = None;
            if !file_manager.is_null() {
                let fm_class = vm.class_of(file_manager);
                if let Some(out_ref) =
                    vm.read_ref_field(file_manager, fm_class, "output", "Ljava/io/File;")
                {
                    if !out_ref.is_null() {
                        output_dir = Some(crate::file::file_path(vm, out_ref));
                    }
                }
            }

            // write source files to a temp dir
            let src_base = "/tmp/opencode/oxjvm-javadoc-src";
            if let Some(fs) = vm.host.filesystem() {
                let _ = fs.create_dir_all(src_base);
            }
            // remove old children? best-effort not needed
            let mut source_paths: Vec<String> = Vec::new();
            if !files_ref.is_null() {
                let files = crate::util::collection_elements(vm, files_ref).unwrap_or_default();
                for (index, src) in files.into_iter().enumerate() {
                    if src.is_null() {
                        continue;
                    }
                    let content_seq = {
                        let (c, m) = match vm.resolve_virtual_method(
                            src,
                            "getCharContent",
                            "(Z)Ljava/lang/CharSequence;",
                        ) {
                            Ok(x) => x,
                            Err(_) => continue,
                        };
                        vm.invoke_method(c, m, vec![Value::Ref(src), Value::Int(0)])
                            .ok()
                    };
                    let text = content_seq
                        .map(|v| vm.string_or_empty(v.as_ref()))
                        .unwrap_or_default();
                    // Determine a Java source file name matching the public class name.
                    let name_str = vm
                        .resolve_virtual_method(src, "getName", "()Ljava/lang/String;")
                        .ok()
                        .and_then(|(c, m)| vm.invoke_method(c, m, vec![Value::Ref(src)]).ok())
                        .map(|v| vm.string_or_empty(v.as_ref()))
                        .unwrap_or_default();
                    let basename = name_str.rsplit('/').next().unwrap_or("");
                    let basename = if basename.is_empty() {
                        alloc::format!("Source{}.java", index)
                    } else {
                        basename.to_string()
                    };
                    let path = alloc::format!("{}/{}", src_base, basename);
                    if let Some(fs) = vm.host.filesystem() {
                        let _ = fs.write(&path, text.as_bytes(), false);
                    }
                    source_paths.push(path);
                }
            }

            // build command
            let mut cmd: Vec<String> = vec![javadoc_program(vm)];
            if let Some(dir) = &output_dir {
                cmd.push("-d".to_string());
                cmd.push(dir.clone());
            }
            for opt in &options {
                if !opt.is_null() {
                    cmd.push(vm.string_or_empty(*opt));
                }
            }
            for p in &source_paths {
                cmd.push(p.clone());
            }
            // drop nulls already handled
            let result = vm.host.run_process(&cmd[0], &cmd[1..], None);
            let status = match result {
                Ok(r) => r.status,
                Err(_) => 1,
            };
            // return Boolean
            let bool_class = vm.resolve_class("java/lang/Boolean")?;
            vm.ensure_initialized(bool_class)?;
            let name = if status == 0 { "TRUE" } else { "FALSE" };
            let instance = vm
                .read_static_ref(bool_class, name, "Ljava/lang/Boolean;")
                .ok_or_else(|| VmError::internal("no Boolean constant"))?;
            Ok(Value::Ref(instance))
        },
    ),
];

pub(crate) const DOCUMENTATION_TOOL_LOCATION: oxjvm_vm::NativeClass = class(
    "javax/tools/DocumentationTool$Location",
    Some("java/lang/Enum"),
    &["javax/tools/JavaFileManager$Location"],
    ACC_PUBLIC | ACC_FINAL | ACC_ENUM,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE | ACC_FINAL, None),
        field("ordinal", "I", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "DOCUMENTATION_OUTPUT",
            "Ljavax/tools/DocumentationTool$Location;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "DOCLET_PATH",
            "Ljavax/tools/DocumentationTool$Location;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "TAGLET_PATH",
            "Ljavax/tools/DocumentationTool$Location;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "SNIPPET_PATH",
            "Ljavax/tools/DocumentationTool$Location;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "$VALUES",
            "[Ljavax/tools/DocumentationTool$Location;",
            ACC_PRIVATE | ACC_STATIC | ACC_FINAL | ACC_SYNTHETIC,
            None,
        ),
    ],
    &DOCUMENTATION_TOOL_LOCATION_METHODS,
    Some(documentation_tool_location_clinit),
);

const DOCUMENTATION_TOOL_LOCATION_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
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
        "getName",
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
        "iterator",
        "()Ljava/util/Iterator;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let list_class = vm.resolve_class("java/util/ArrayList")?;
            let list = vm.new_instance(list_class)?;
            let (c, m) = vm
                .find_method(list_class, "<init>", "()V")
                .ok_or_else(|| VmError::internal("no ctor"))?;
            vm.invoke_method(c, m, vec![Value::Ref(list)])?;
            let (ac, am) = vm.resolve_virtual_method(list, "add", "(Ljava/lang/Object;)Z")?;
            let _ = vm.invoke_method(ac, am, vec![Value::Ref(list), Value::Ref(this)])?;
            let (ic, im) = vm.resolve_virtual_method(list, "iterator", "()Ljava/util/Iterator;")?;
            vm.invoke_method(ic, im, vec![Value::Ref(list)])
        },
    ),
    method("isOutputLocation", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        let name = vm
            .read_string_field(this, class, "name")
            .unwrap_or_default();
        boolean(name == "DOCUMENTATION_OUTPUT")
    }),
];

fn documentation_tool_location_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("javax/tools/DocumentationTool$Location")?;
    let names = [
        "DOCUMENTATION_OUTPUT",
        "DOCLET_PATH",
        "TAGLET_PATH",
        "SNIPPET_PATH",
    ];
    let mut built = Vec::new();
    for (ordinal, name) in names.iter().enumerate() {
        let instance = vm.new_instance(class)?;
        let name_ref = vm.make_string(name)?;
        let (decl, ctor) = vm
            .find_method(class, "<init>", "(Ljava/lang/String;I)V")
            .ok_or_else(|| VmError::internal("ctor missing"))?;
        vm.invoke_method(
            decl,
            ctor,
            vec![
                Value::Ref(instance),
                Value::Ref(name_ref),
                Value::Int(ordinal as i32),
            ],
        )?;
        vm.set_static_value(
            class,
            name,
            "Ljavax/tools/DocumentationTool$Location;",
            Value::Ref(instance),
        );
        built.push(Value::Ref(instance));
    }
    let array = vm.allocate_array_of(ArrayComponent::Reference, built.len())?;
    for (i, v) in built.into_iter().enumerate() {
        vm.array_set(array, i as i32, v, false)?;
    }
    vm.set_static_value(
        class,
        "$VALUES",
        "[Ljavax/tools/DocumentationTool$Location;",
        Value::Ref(array),
    );
    Ok(())
}
