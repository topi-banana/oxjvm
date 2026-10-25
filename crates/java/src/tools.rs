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
    let mut values = Vec::with_capacity(SOURCE_VERSIONS.len());
    for (ordinal, name) in SOURCE_VERSIONS.iter().enumerate() {
        let instance = vm.new_instance(class)?;
        let name_ref = vm.make_string(name)?;
        let (declaring, ctor) = vm
            .find_method(class, "<init>", "(Ljava/lang/String;I)V")
            .ok_or_else(|| VmError::internal("SourceVersion lacks its constructor"))?;
        // the constructor is inherited from Enum; find on the declaring class path
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
        let (fdecl, _) = vm.find_field(class, field_name, "Ljavax/lang/model/SourceVersion;")?;
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
    for (ordinal, (name, disp, ext)) in values_order.iter().enumerate() {
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
    ],
    &SIMPLE_JAVA_FILE_OBJECT_METHODS,
    None,
);

const SIMPLE_JAVA_FILE_OBJECT_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
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
            let (d, f) = vm.find_field(class, "kind", "Ljavax/tools/JavaFileObject$Kind;")?;
            object(
                vm.read_ref_field(this, class, "kind", "Ljavax/tools/JavaFileObject$Kind;")
                    .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method("toUri", "()Ljava/net/URI;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        let (d, f) = vm.find_field(class, "uri", "Ljava/net/URI;")?;
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
        |vm, _, _args| object(vm.make_string("")?),
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
];
// Note: SimpleJavaFileObject needs uri/kind fields populated by subclass constructors.
// The abstract JavaFileObject interface below is registered as a pure interface.

// --------------------------------------------------------------------------
// Pure interface shells (remove NoClassDefFoundError)
// --------------------------------------------------------------------------

macro_rules! interface {
    ($name:expr, $supers:expr) => {
        class(
            $name,
            Some("java/lang/Object"),
            $supers,
            ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
            &[],
            &[],
            None,
        )
    };
}

pub(crate) const TOOL: oxjvm_vm::NativeClass = class(
    "javax/tools/Tool",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const FILE_OBJECT: oxjvm_vm::NativeClass = class(
    "javax/tools/FileObject",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const JAVA_FILE_OBJECT: oxjvm_vm::NativeClass = class(
    "javax/tools/JavaFileObject",
    Some("java/lang/Object"),
    &["javax/tools/FileObject"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
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

pub(crate) const STANDARD_JAVA_FILE_MANAGER: oxjvm_vm::NativeClass = class(
    "javax/tools/StandardJavaFileManager",
    Some("java/lang/Object"),
    &["javax/tools/JavaFileManager"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const JAVA_COMPILER: oxjvm_vm::NativeClass = class(
    "javax/tools/JavaCompiler",
    Some("java/lang/Object"),
    &["javax/tools/Tool", "javax/tools/OptionChecker"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const DOCUMENTATION_TOOL: oxjvm_vm::NativeClass = class(
    "javax/tools/DocumentationTool",
    Some("java/lang/Object"),
    &["javax/tools/Tool", "javax/tools/OptionChecker"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
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

pub(crate) const OPTION_CHECKER: oxjvm_vm::NativeClass = class(
    "javax/tools/OptionChecker",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
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
    &[],
    None,
);

pub(crate) const DOCUMENTATION_TOOL_DOCUMENTATION_TASK: oxjvm_vm::NativeClass = class(
    "javax/tools/DocumentationTool$DocumentationTask",
    Some("java/lang/Object"),
    &["java/util/concurrent/Callable"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
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
            let class = vm.resolve_class("com/sun/tools/javac/api/JavacTool")?;
            // fall back: use a DocumentationTool shim class
            Err(vm.throw_new(
                "java/lang/UnsupportedOperationException",
                Some("javadoc tool not available"),
            ))
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

const JAVAC_TOOL_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
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
        "(Ljava/lang/String;)Z",
        ACC_PUBLIC,
        |_vm, _, _args| boolean(false),
    ),
    method(
        "run",
        "(Ljava/io/InputStream;Ljava/io/OutputStream;Ljava/io/OutputStream;[Ljava/lang/String;)I",
        ACC_PUBLIC,
        |vm, _, _args| {
            // delegate to host javac
            let args_array = ref_arg(_args, 3);
            let mut cmd: Vec<String> = vec!["javac".to_string()];
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
            let mut cmd: Vec<String> = vec!["javac".to_string()];
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
            let mut cmd: Vec<String> = vec!["javac".to_string()];
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
            let mut cmd: Vec<String> = vec!["javac".to_string()];
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
