//! A pragmatic subset of the JDK 24+ `java.lang.classfile` API and
//! `java.lang.constant.ClassDesc`.
//!
//! `ClassFile.of().parse(bytes)` returns a real model (the class name, version, flags and methods
//! come from oxjvm's own class-file reader); transformations are not supported and raise
//! `UnsupportedOperationException`.  The reading half is what the jtreg utilities use to inspect
//! generated classes.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::{boolean, class, field, int, method, object, receiver, ref_arg, void};

fn new_instance(vm: &mut Vm<'_>, class_name: &str) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class(class_name)?;
    let instance = vm.new_instance(class)?;
    if let Some((declaring, ctor)) = vm.find_method(class, "<init>", "()V") {
        vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
    }
    Ok(instance)
}

fn new_array_list(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    new_instance(vm, "java/util/ArrayList")
}

fn list_add(vm: &mut Vm<'_>, list: ObjectRef, value: ObjectRef) -> Result<(), VmError> {
    let (class, method) = vm.resolve_virtual_method(list, "add", "(Ljava/lang/Object;)Z")?;
    vm.invoke_method(class, method, vec![Value::Ref(list), Value::Ref(value)])?;
    Ok(())
}

fn empty_optional(vm: &mut Vm<'_>) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/util/Optional")?;
    let (declaring, method) = vm
        .find_method(class, "empty", "()Ljava/util/Optional;")
        .ok_or_else(|| VmError::internal("Optional.empty"))?;
    vm.invoke_method(declaring, method, vec![])
}

fn optional_of(vm: &mut Vm<'_>, value: ObjectRef) -> Result<Value, VmError> {
    if value.is_null() {
        return empty_optional(vm);
    }
    let class = vm.resolve_class("java/util/Optional")?;
    let (declaring, method) = vm
        .find_method(
            class,
            "ofNullable",
            "(Ljava/lang/Object;)Ljava/util/Optional;",
        )
        .ok_or_else(|| VmError::internal("Optional.ofNullable"))?;
    vm.invoke_method(declaring, method, vec![Value::Ref(value)])
}

fn string_field(vm: &Vm<'_>, this: ObjectRef, name: &str) -> String {
    vm.read_string_field_any(this, name).unwrap_or_default()
}

fn set_string_field(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: &str,
    value: &str,
) -> Result<(), VmError> {
    let class = vm.class_of(this);
    vm.set_string_field(this, class, name, value)
}

fn int_field(vm: &Vm<'_>, this: ObjectRef, name: &str) -> i32 {
    vm.read_instance_int_named(this, vm.class_of(this), name, "I")
        .unwrap_or(0)
}

fn set_int_field(vm: &mut Vm<'_>, this: ObjectRef, name: &str, value: i32) -> Result<(), VmError> {
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, name, "I")?;
    vm.set_instance_int(this, declaring, field, value);
    Ok(())
}

fn new_byte_array(vm: &mut Vm<'_>, bytes: &[u8]) -> Result<ObjectRef, VmError> {
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, bytes.len())?;
    for (index, byte) in bytes.iter().enumerate() {
        vm.array_set(array, index as i32, Value::Int(i32::from(*byte)), false)?;
    }
    Ok(array)
}

fn read_byte_array(vm: &mut Vm<'_>, array: ObjectRef) -> Result<Vec<u8>, VmError> {
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

const fn abstract_method(
    name: &'static str,
    descriptor: &'static str,
) -> oxjvm_vm::NativeMethodDef {
    method(
        name,
        descriptor,
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    )
}

// -------------------------------------------------------------------------------------------
// java.lang.constant.ClassDesc
// -------------------------------------------------------------------------------------------

fn descriptor_to_name(descriptor: &str) -> String {
    let mut dimensions = 0;
    let mut rest = descriptor;
    while let Some(stripped) = rest.strip_prefix('[') {
        dimensions += 1;
        rest = stripped;
    }
    let base = if let Some(internal) = rest.strip_prefix('L').and_then(|s| s.strip_suffix(';')) {
        internal.replace('/', ".")
    } else {
        match rest {
            "B" => "byte".to_string(),
            "C" => "char".to_string(),
            "D" => "double".to_string(),
            "F" => "float".to_string(),
            "I" => "int".to_string(),
            "J" => "long".to_string(),
            "S" => "short".to_string(),
            "Z" => "boolean".to_string(),
            "V" => "void".to_string(),
            other => other.to_string(),
        }
    };
    let mut name = base;
    for _ in 0..dimensions {
        name.push_str("[]");
    }
    name
}

fn name_to_descriptor(name: &str) -> String {
    let mut dimensions = 0;
    let mut base = name;
    while let Some(stripped) = base.strip_suffix("[]") {
        dimensions += 1;
        base = stripped;
    }
    let descriptor = match base {
        "byte" => "B".to_string(),
        "char" => "C".to_string(),
        "double" => "D".to_string(),
        "float" => "F".to_string(),
        "int" => "I".to_string(),
        "long" => "J".to_string(),
        "short" => "S".to_string(),
        "boolean" => "Z".to_string(),
        "void" => "V".to_string(),
        other => alloc::format!("L{};", other.replace('.', "/")),
    };
    let mut result = descriptor;
    for _ in 0..dimensions {
        result.insert(0, '[');
    }
    result
}

fn new_class_desc(vm: &mut Vm<'_>, name: &str) -> Result<ObjectRef, VmError> {
    let desc = new_instance(vm, "java/lang/constant/OjvmClassDesc")?;
    let descriptor = name_to_descriptor(name);
    set_string_field(vm, desc, "name", name)?;
    set_string_field(vm, desc, "descriptor", &descriptor)?;
    Ok(desc)
}

const CLASS_DESC_METHODS: [oxjvm_vm::NativeMethodDef; 17] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "of",
        "(Ljava/lang/String;)Ljava/lang/constant/ClassDesc;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = vm.string_or_empty(ref_arg(args, 0));
            object(new_class_desc(vm, &name)?)
        },
    ),
    method(
        "of",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/constant/ClassDesc;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let package = vm.string_or_empty(ref_arg(args, 0));
            let simple = vm.string_or_empty(ref_arg(args, 1));
            let name = if package.is_empty() {
                simple
            } else {
                alloc::format!("{package}.{simple}")
            };
            object(new_class_desc(vm, &name)?)
        },
    ),
    method(
        "ofDescriptor",
        "(Ljava/lang/String;)Ljava/lang/constant/ClassDesc;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let descriptor = vm.string_or_empty(ref_arg(args, 0));
            object(new_class_desc(vm, &descriptor_to_name(&descriptor))?)
        },
    ),
    method(
        "ofInternalName",
        "(Ljava/lang/String;)Ljava/lang/constant/ClassDesc;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let internal = vm.string_or_empty(ref_arg(args, 0));
            object(new_class_desc(vm, &internal.replace('/', "."))?)
        },
    ),
    method(
        "displayName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = string_field(vm, receiver(args), "name");
            object(vm.make_string(&name)?)
        },
    ),
    method(
        "descriptorString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let descriptor = string_field(vm, receiver(args), "descriptor");
            object(vm.make_string(&descriptor)?)
        },
    ),
    method(
        "packageName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = string_field(vm, receiver(args), "name");
            let package = name.rsplit_once('.').map(|(p, _)| p.to_string()).unwrap_or_default();
            object(vm.make_string(&package)?)
        },
    ),
    method(
        "arrayType",
        "()Ljava/lang/constant/ClassDesc;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name = string_field(vm, this, "name");
            object(new_class_desc(vm, &alloc::format!("{name}[]"))?)
        },
    ),
    method(
        "arrayType",
        "(I)Ljava/lang/constant/ClassDesc;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let mut name = string_field(vm, this, "name");
            for _ in 0..crate::int_arg(args, 1).max(0) {
                name.push_str("[]");
            }
            object(new_class_desc(vm, &name)?)
        },
    ),
    method(
        "componentType",
        "()Ljava/lang/constant/ClassDesc;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name = string_field(vm, this, "name");
            let component = name.strip_suffix("[]").unwrap_or(&name).to_string();
            object(new_class_desc(vm, &component)?)
        },
    ),
    method("isArray", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(string_field(vm, receiver(args), "descriptor").starts_with('['))
    }),
    method("isPrimitive", "()Z", ACC_PUBLIC, |vm, _, args| {
        let descriptor = string_field(vm, receiver(args), "descriptor");
        boolean(descriptor.len() == 1)
    }),
    method(
        "nested",
        "(Ljava/lang/String;)Ljava/lang/constant/ClassDesc;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name = string_field(vm, this, "name");
            let nested = vm.string_or_empty(ref_arg(args, 1));
            object(new_class_desc(vm, &alloc::format!("{name}${nested}"))?)
        },
    ),
    method("toString", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let name = string_field(vm, receiver(args), "name");
        object(vm.make_string(&name)?)
    }),
    method("equals", "(Ljava/lang/Object;)Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let other = ref_arg(args, 1);
        if other.is_null() || vm.class_of(other) != vm.class_of(this) {
            return boolean(false);
        }
        boolean(
            string_field(vm, this, "descriptor") == string_field(vm, other, "descriptor"),
        )
    }),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let descriptor = string_field(vm, receiver(args), "descriptor");
        let mut hash: i32 = 0;
        for unit in descriptor.encode_utf16() {
            hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
        }
        int(hash)
    }),
];

pub(crate) const OJVM_CLASS_DESC: oxjvm_vm::NativeClass = class(
    "java/lang/constant/OjvmClassDesc",
    Some("java/lang/Object"),
    &["java/lang/constant/ClassDesc"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("descriptor", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &CLASS_DESC_METHODS,
    None,
);

const CLASS_DESC_INTERFACE_METHODS: [oxjvm_vm::NativeMethodDef; 15] = [
    abstract_method("arrayType", "()Ljava/lang/constant/ClassDesc;"),
    abstract_method(
        "arrayType",
        "(I)Ljava/lang/constant/ClassDesc;",
    ),
    abstract_method(
        "nested",
        "(Ljava/lang/String;)Ljava/lang/constant/ClassDesc;",
    ),
    abstract_method("isArray", "()Z"),
    abstract_method("isPrimitive", "()Z"),
    abstract_method("componentType", "()Ljava/lang/constant/ClassDesc;"),
    abstract_method("packageName", "()Ljava/lang/String;"),
    abstract_method("displayName", "()Ljava/lang/String;"),
    abstract_method("descriptorString", "()Ljava/lang/String;"),
    abstract_method("equals", "(Ljava/lang/Object;)Z"),
    abstract_method("hashCode", "()I"),
    abstract_method("toString", "()Ljava/lang/String;"),
    method(
        "of",
        "(Ljava/lang/String;)Ljava/lang/constant/ClassDesc;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = vm.string_or_empty(ref_arg(args, 0));
            object(new_class_desc(vm, &name)?)
        },
    ),
    method(
        "ofDescriptor",
        "(Ljava/lang/String;)Ljava/lang/constant/ClassDesc;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let descriptor = vm.string_or_empty(ref_arg(args, 0));
            object(new_class_desc(vm, &descriptor_to_name(&descriptor))?)
        },
    ),
    method(
        "ofInternalName",
        "(Ljava/lang/String;)Ljava/lang/constant/ClassDesc;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let internal = vm.string_or_empty(ref_arg(args, 0));
            object(new_class_desc(vm, &internal.replace('/', "."))?)
        },
    ),
];

pub(crate) const CLASS_DESC: oxjvm_vm::NativeClass = class(
    "java/lang/constant/ClassDesc",
    Some("java/lang/Object"),
    &["java/lang/constant/ConstantDesc"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &CLASS_DESC_INTERFACE_METHODS,
    None,
);

pub(crate) const CONSTANT_DESC: oxjvm_vm::NativeClass = class(
    "java/lang/constant/ConstantDesc",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

// -------------------------------------------------------------------------------------------
// java.lang.classfile.AccessFlags / constant-pool entries
// -------------------------------------------------------------------------------------------

const ACCESS_FLAGS_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("flagsMask", "()I", ACC_PUBLIC, |vm, _, args| {
        int(int_field(vm, receiver(args), "flags"))
    }),
    method(
        "has",
        "(Ljava/lang/reflect/AccessFlag;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let flag = ref_arg(args, 1);
            if flag.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let (class, method) = vm.resolve_virtual_method(flag, "mask", "()I")?;
            let mask = vm.invoke_method(class, method, vec![Value::Ref(flag)])?.as_int();
            boolean(int_field(vm, this, "flags") & mask != 0)
        },
    ),
    method("flags", "()Ljava/util/Set;", ACC_PUBLIC, |vm, _, _| {
        object(new_instance(vm, "java/util/HashSet")?)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(vm.make_string(&alloc::format!("flags=0x{:x}", int_field(vm, receiver(args), "flags")))?),
    ),
];

pub(crate) const OJVM_ACCESS_FLAGS: oxjvm_vm::NativeClass = class(
    "java/lang/classfile/OjvmAccessFlags",
    Some("java/lang/Object"),
    &["java/lang/classfile/AccessFlags"],
    ACC_PUBLIC,
    &[field("flags", "I", ACC_PRIVATE, None)],
    &ACCESS_FLAGS_METHODS,
    None,
);

const UTF8_ENTRY_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "stringValue",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = string_field(vm, receiver(args), "value");
            object(vm.make_string(&value)?)
        },
    ),
    method("equalsString", "(Ljava/lang/String;)Z", ACC_PUBLIC, |vm, _, args| {
        boolean(string_field(vm, receiver(args), "value") == vm.string_or_empty(ref_arg(args, 1)))
    }),
    method("isFieldType", "(Ljava/lang/constant/ClassDesc;)Z", ACC_PUBLIC, |_, _, _| {
        boolean(true)
    }),
    method("isMethodType", "(Ljava/lang/constant/MethodTypeDesc;)Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
];

pub(crate) const OJVM_UTF8_ENTRY: oxjvm_vm::NativeClass = class(
    "java/lang/classfile/constantpool/OjvmUtf8Entry",
    Some("java/lang/Object"),
    &["java/lang/classfile/constantpool/Utf8Entry"],
    ACC_PUBLIC,
    &[field("value", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &UTF8_ENTRY_METHODS,
    None,
);

const CLASS_ENTRY_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "name",
        "()Ljava/lang/classfile/constantpool/Utf8Entry;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = string_field(vm, receiver(args), "internalName");
            let entry = new_instance(vm, "java/lang/classfile/constantpool/OjvmUtf8Entry")?;
            set_string_field(vm, entry, "value", &value)?;
            object(entry)
        },
    ),
    method(
        "asInternalName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = string_field(vm, receiver(args), "internalName");
            object(vm.make_string(&value)?)
        },
    ),
    method(
        "asSymbol",
        "()Ljava/lang/constant/ClassDesc;",
        ACC_PUBLIC,
        |vm, _, args| {
            let internal = string_field(vm, receiver(args), "internalName");
            object(new_class_desc(vm, &internal.replace('/', "."))?)
        },
    ),
    method(
        "matches",
        "(Ljava/lang/constant/ClassDesc;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let symbol = ref_arg(args, 1);
            if symbol.is_null() {
                return boolean(false);
            }
            let this = receiver(args);
            let descriptor = vm
                .read_string_field_any(symbol, "descriptor")
                .unwrap_or_default();
            boolean(
                descriptor == alloc::format!("L{};", string_field(vm, this, "internalName")),
            )
        },
    ),
];

pub(crate) const OJVM_CLASS_ENTRY: oxjvm_vm::NativeClass = class(
    "java/lang/classfile/constantpool/OjvmClassEntry",
    Some("java/lang/Object"),
    &["java/lang/classfile/constantpool/ClassEntry"],
    ACC_PUBLIC,
    &[field("internalName", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &CLASS_ENTRY_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// java.lang.classfile.ClassFile / ClassModel / MethodModel
// -------------------------------------------------------------------------------------------

fn new_class_entry(vm: &mut Vm<'_>, internal: &str) -> Result<ObjectRef, VmError> {
    let entry = new_instance(vm, "java/lang/classfile/constantpool/OjvmClassEntry")?;
    set_string_field(vm, entry, "internalName", internal)?;
    Ok(entry)
}

fn new_method_model(
    vm: &mut Vm<'_>,
    name: &str,
    descriptor: &str,
    flags: i32,
) -> Result<ObjectRef, VmError> {
    let model = new_instance(vm, "java/lang/classfile/OjvmMethodModel")?;
    let name_entry = new_instance(vm, "java/lang/classfile/constantpool/OjvmUtf8Entry")?;
    set_string_field(vm, name_entry, "value", name)?;
    let type_entry = new_instance(vm, "java/lang/classfile/constantpool/OjvmUtf8Entry")?;
    set_string_field(vm, type_entry, "value", descriptor)?;
    let class = vm.class_of(model);
    let (declaring, field) = vm.find_field(class, "nameEntry", "Ljava/lang/classfile/constantpool/Utf8Entry;")?;
    vm.set_instance_ref(model, declaring, field, name_entry);
    let (declaring, field) = vm.find_field(class, "typeEntry", "Ljava/lang/classfile/constantpool/Utf8Entry;")?;
    vm.set_instance_ref(model, declaring, field, type_entry);
    set_int_field(vm, model, "flags", flags)?;
    Ok(model)
}

fn parse_class_model(vm: &mut Vm<'_>, bytes: &[u8]) -> Result<ObjectRef, VmError> {
    let parsed = oxjvm_classfile::ClassFile::read(bytes)
        .map_err(|error| vm.throw_new("java/lang/IllegalArgumentException", Some(&alloc::format!("{error:?}"))))?;
    let internal = parsed
        .constant_pool
        .class_name(parsed.this_class)
        .unwrap_or("java/lang/Object")
        .to_string();
    let model = new_instance(vm, "java/lang/classfile/OjvmClassModel")?;
    set_string_field(vm, model, "internalName", &internal)?;
    set_int_field(vm, model, "majorVersion", i32::from(parsed.major_version))?;
    set_int_field(vm, model, "minorVersion", i32::from(parsed.minor_version))?;
    let flags = new_instance(vm, "java/lang/classfile/OjvmAccessFlags")?;
    set_int_field(vm, flags, "flags", i32::from(parsed.access_flags))?;
    set_ref_field(vm, model, "flags", "Ljava/lang/classfile/AccessFlags;", flags)?;
    let methods = new_array_list(vm)?;
    for method in &parsed.methods {
        let name = parsed
            .constant_pool
            .utf8(method.name_index)
            .unwrap_or("<unknown>");
        let descriptor = parsed
            .constant_pool
            .utf8(method.descriptor_index)
            .unwrap_or("()V");
        let model = new_method_model(vm, name, descriptor, i32::from(method.access_flags))?;
        list_add(vm, methods, model)?;
    }
    set_ref_field(vm, model, "methods", "Ljava/util/List;", methods)?;
    let bytes_array = new_byte_array(vm, bytes)?;
    set_ref_field(vm, model, "bytes", "[B", bytes_array)?;
    Ok(model)
}

fn set_ref_field(
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

fn minimal_class_bytes(internal_name: &str) -> Vec<u8> {
    // magic, version 61.0, constant pool with this/super, empty everything else
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&[0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x3D]);
    bytes.extend_from_slice(&[0x00, 0x05]);
    // #1 Class -> #2
    bytes.extend_from_slice(&[0x07, 0x00, 0x02]);
    // #2 Utf8 internal name
    bytes.push(0x01);
    bytes.extend_from_slice(&(internal_name.len() as u16).to_be_bytes());
    bytes.extend_from_slice(internal_name.as_bytes());
    // #3 Class -> #4
    bytes.extend_from_slice(&[0x07, 0x00, 0x04]);
    // #4 Utf8 java/lang/Object
    bytes.push(0x01);
    bytes.extend_from_slice(&18u16.to_be_bytes());
    bytes.extend_from_slice(b"java/lang/Object");
    // access = public | super, this=1, super=3
    bytes.extend_from_slice(&[0x00, 0x21, 0x00, 0x01, 0x00, 0x03]);
    bytes.extend_from_slice(&[0x00, 0x00]); // interfaces
    bytes.extend_from_slice(&[0x00, 0x00]); // fields
    bytes.extend_from_slice(&[0x00, 0x00]); // methods
    bytes.extend_from_slice(&[0x00, 0x00]); // attributes
    bytes
}

const CLASS_FILE_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "of",
        "()Ljava/lang/classfile/ClassFile;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_instance(vm, "java/lang/classfile/OjvmClassFile")?),
    ),
    method(
        "of",
        "([Ljava/lang/classfile/ClassFile$Option;)Ljava/lang/classfile/ClassFile;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_instance(vm, "java/lang/classfile/OjvmClassFile")?),
    ),
    method(
        "withOptions",
        "([Ljava/lang/classfile/ClassFile$Option;)Ljava/lang/classfile/ClassFile;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method(
        "parse",
        "([B)Ljava/lang/classfile/ClassModel;",
        ACC_PUBLIC,
        |vm, _, args| {
            let bytes = read_byte_array(vm, ref_arg(args, 1))?;
            object(parse_class_model(vm, &bytes)?)
        },
    ),
    method(
        "build",
        "(Ljava/lang/classfile/constantpool/ClassEntry;Ljava/lang/classfile/constantpool/ConstantPoolBuilder;Ljava/util/function/Consumer;)[B",
        ACC_PUBLIC,
        |vm, _, args| {
            let entry = ref_arg(args, 1);
            if entry.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let internal = string_field(vm, entry, "internalName");
            let bytes = minimal_class_bytes(&internal);
            object(new_byte_array(vm, &bytes)?)
        },
    ),
    method(
        "transformClass",
        "(Ljava/lang/classfile/ClassModel;Ljava/lang/classfile/constantpool/ClassEntry;Ljava/lang/classfile/ClassTransform;)[B",
        ACC_PUBLIC,
        |vm, _, _| {
            Err(vm.throw_new(
                "java/lang/UnsupportedOperationException",
                Some("class transformation is not implemented"),
            ))
        },
    ),
    method(
        "verify",
        "([B)Ljava/util/List;",
        ACC_PUBLIC,
        |vm, _, _| object(new_array_list(vm)?),
    ),
    method(
        "latestMajorVersion",
        "()I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| int(71),
    ),
];

pub(crate) const OJVM_CLASS_FILE: oxjvm_vm::NativeClass = class(
    "java/lang/classfile/OjvmClassFile",
    Some("java/lang/Object"),
    &["java/lang/classfile/ClassFile"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[],
    &CLASS_FILE_METHODS,
    None,
);

const CLASS_FILE_INTERFACE_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    abstract_method(
        "withOptions",
        "([Ljava/lang/classfile/ClassFile$Option;)Ljava/lang/classfile/ClassFile;",
    ),
    abstract_method(
        "parse",
        "([B)Ljava/lang/classfile/ClassModel;",
    ),
    abstract_method(
        "build",
        "(Ljava/lang/classfile/constantpool/ClassEntry;Ljava/lang/classfile/constantpool/ConstantPoolBuilder;Ljava/util/function/Consumer;)[B",
    ),
    abstract_method(
        "transformClass",
        "(Ljava/lang/classfile/ClassModel;Ljava/lang/classfile/constantpool/ClassEntry;Ljava/lang/classfile/ClassTransform;)[B",
    ),
    abstract_method("verify", "([B)Ljava/util/List;"),
    method(
        "of",
        "()Ljava/lang/classfile/ClassFile;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_instance(vm, "java/lang/classfile/OjvmClassFile")?),
    ),
    method(
        "of",
        "([Ljava/lang/classfile/ClassFile$Option;)Ljava/lang/classfile/ClassFile;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_instance(vm, "java/lang/classfile/OjvmClassFile")?),
    ),
    method(
        "latestMajorVersion",
        "()I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| int(71),
    ),
    method(
        "latestMinorVersion",
        "()I",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| int(0),
    ),
];

pub(crate) const CLASS_FILE_INTERFACE: oxjvm_vm::NativeClass = class(
    "java/lang/classfile/ClassFile",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &CLASS_FILE_INTERFACE_METHODS,
    None,
);

pub(crate) const CLASS_FILE_OPTION: oxjvm_vm::NativeClass = class(
    "java/lang/classfile/ClassFile$Option",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const CONSTANT_POOL_BUILDER: oxjvm_vm::NativeClass = class(
    "java/lang/classfile/constantpool/ConstantPoolBuilder",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

const CLASS_MODEL_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "thisClass",
        "()Ljava/lang/classfile/constantpool/ClassEntry;",
        ACC_PUBLIC,
        |vm, _, args| object(new_class_entry(vm, &string_field(vm, receiver(args), "internalName"))?),
    ),
    method(
        "superclass",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, _| {
            let entry = new_class_entry(vm, "java/lang/Object")?;
            optional_of(vm, entry)
        },
    ),
    method("majorVersion", "()I", ACC_PUBLIC, |vm, _, args| {
        int(int_field(vm, receiver(args), "majorVersion"))
    }),
    method("minorVersion", "()I", ACC_PUBLIC, |vm, _, args| {
        int(int_field(vm, receiver(args), "minorVersion"))
    }),
    method(
        "flags",
        "()Ljava/lang/classfile/AccessFlags;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(vm.read_ref_field_any(receiver(args), "flags").unwrap_or(ObjectRef::NULL))
        },
    ),
    method(
        "methods",
        "()Ljava/util/List;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(vm.read_ref_field_any(receiver(args), "methods").unwrap_or(ObjectRef::NULL))
        },
    ),
    method("fields", "()Ljava/util/List;", ACC_PUBLIC, |vm, _, _| {
        object(new_array_list(vm)?)
    }),
    method("interfaces", "()Ljava/util/List;", ACC_PUBLIC, |vm, _, _| {
        object(new_array_list(vm)?)
    }),
    method("isModuleInfo", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method(
        "thisClassInternalName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let internal = string_field(vm, receiver(args), "internalName");
            object(vm.make_string(&internal)?)
        },
    ),
    method("toString", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let text = alloc::format!(
            "ClassModel[{}]",
            string_field(vm, receiver(args), "internalName")
        );
        object(vm.make_string(&text)?)
    }),
];

const CLASS_MODEL_INTERFACE_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    abstract_method(
        "thisClass",
        "()Ljava/lang/classfile/constantpool/ClassEntry;",
    ),
    abstract_method("superclass", "()Ljava/util/Optional;"),
    abstract_method("majorVersion", "()I"),
    abstract_method("minorVersion", "()I"),
    abstract_method("flags", "()Ljava/lang/classfile/AccessFlags;"),
    abstract_method("methods", "()Ljava/util/List;"),
    abstract_method("fields", "()Ljava/util/List;"),
    abstract_method("interfaces", "()Ljava/util/List;"),
    abstract_method("isModuleInfo", "()Z"),
];

pub(crate) const CLASS_MODEL_INTERFACE: oxjvm_vm::NativeClass = class(
    "java/lang/classfile/ClassModel",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &CLASS_MODEL_INTERFACE_METHODS,
    None,
);

const METHOD_MODEL_INTERFACE_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    abstract_method(
        "methodName",
        "()Ljava/lang/classfile/constantpool/Utf8Entry;",
    ),
    abstract_method(
        "methodType",
        "()Ljava/lang/classfile/constantpool/Utf8Entry;",
    ),
    abstract_method("flags", "()Ljava/lang/classfile/AccessFlags;"),
    abstract_method("parent", "()Ljava/util/Optional;"),
    abstract_method("code", "()Ljava/util/Optional;"),
    abstract_method("toString", "()Ljava/lang/String;"),
];

pub(crate) const METHOD_MODEL_INTERFACE: oxjvm_vm::NativeClass = class(
    "java/lang/classfile/MethodModel",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &METHOD_MODEL_INTERFACE_METHODS,
    None,
);

const ACCESS_FLAGS_INTERFACE_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    abstract_method("flagsMask", "()I"),
    abstract_method("flags", "()Ljava/util/Set;"),
    abstract_method("has", "(Ljava/lang/reflect/AccessFlag;)Z"),
];

pub(crate) const ACCESS_FLAGS_INTERFACE: oxjvm_vm::NativeClass = class(
    "java/lang/classfile/AccessFlags",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &ACCESS_FLAGS_INTERFACE_METHODS,
    None,
);

const UTF8_ENTRY_INTERFACE_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    abstract_method("stringValue", "()Ljava/lang/String;"),
    abstract_method("equalsString", "(Ljava/lang/String;)Z"),
];

pub(crate) const UTF8_ENTRY_INTERFACE: oxjvm_vm::NativeClass = class(
    "java/lang/classfile/constantpool/Utf8Entry",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &UTF8_ENTRY_INTERFACE_METHODS,
    None,
);

const CLASS_ENTRY_INTERFACE_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    abstract_method(
        "name",
        "()Ljava/lang/classfile/constantpool/Utf8Entry;",
    ),
    abstract_method("asInternalName", "()Ljava/lang/String;"),
    abstract_method(
        "asSymbol",
        "()Ljava/lang/constant/ClassDesc;",
    ),
    abstract_method(
        "matches",
        "(Ljava/lang/constant/ClassDesc;)Z",
    ),
];

pub(crate) const CLASS_ENTRY_INTERFACE: oxjvm_vm::NativeClass = class(
    "java/lang/classfile/constantpool/ClassEntry",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &CLASS_ENTRY_INTERFACE_METHODS,
    None,
);

pub(crate) const OJVM_CLASS_MODEL: oxjvm_vm::NativeClass = class(
    "java/lang/classfile/OjvmClassModel",
    Some("java/lang/Object"),
    &["java/lang/classfile/ClassModel"],
    ACC_PUBLIC,
    &[
        field("internalName", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("majorVersion", "I", ACC_PRIVATE, None),
        field("minorVersion", "I", ACC_PRIVATE, None),
        field("flags", "Ljava/lang/classfile/AccessFlags;", ACC_PRIVATE, None),
        field("methods", "Ljava/util/List;", ACC_PRIVATE, None),
        field("bytes", "[B", ACC_PRIVATE, None),
    ],
    &CLASS_MODEL_METHODS,
    None,
);

const METHOD_MODEL_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "methodName",
        "()Ljava/lang/classfile/constantpool/Utf8Entry;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(
                vm.read_ref_field_any(receiver(args), "nameEntry")
                    .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method(
        "methodType",
        "()Ljava/lang/classfile/constantpool/Utf8Entry;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(
                vm.read_ref_field_any(receiver(args), "typeEntry")
                    .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method(
        "flags",
        "()Ljava/lang/classfile/AccessFlags;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let flags = new_instance(vm, "java/lang/classfile/OjvmAccessFlags")?;
            set_int_field(vm, flags, "flags", int_field(vm, this, "flags"))?;
            object(flags)
        },
    ),
    method(
        "parent",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, _| empty_optional(vm),
    ),
    method(
        "code",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, _| empty_optional(vm),
    ),
    method("toString", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let name = vm.read_ref_field_any(this, "nameEntry").unwrap_or(ObjectRef::NULL);
        let name = vm.read_string_field_any(name, "value").unwrap_or_default();
        object(vm.make_string(&name)?)
    }),
    method("methodNameString", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let entry = vm
            .read_ref_field_any(receiver(args), "nameEntry")
            .unwrap_or(ObjectRef::NULL);
        let name = vm.read_string_field_any(entry, "value").unwrap_or_default();
        object(vm.make_string(&name)?)
    }),
    method("methodTypeString", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let entry = vm
            .read_ref_field_any(receiver(args), "typeEntry")
            .unwrap_or(ObjectRef::NULL);
        let descriptor = vm.read_string_field_any(entry, "value").unwrap_or_default();
        object(vm.make_string(&descriptor)?)
    }),
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        set_int_field(vm, receiver(args), "flags", crate::int_arg(args, 1))?;
        void()
    }),
];

pub(crate) const OJVM_METHOD_MODEL: oxjvm_vm::NativeClass = class(
    "java/lang/classfile/OjvmMethodModel",
    Some("java/lang/Object"),
    &["java/lang/classfile/MethodModel"],
    ACC_PUBLIC,
    &[
        field("nameEntry", "Ljava/lang/classfile/constantpool/Utf8Entry;", ACC_PRIVATE, None),
        field("typeEntry", "Ljava/lang/classfile/constantpool/Utf8Entry;", ACC_PRIVATE, None),
        field("flags", "I", ACC_PRIVATE, None),
    ],
    &METHOD_MODEL_METHODS,
    None,
);

pub(crate) const CLASS_TRANSFORM: oxjvm_vm::NativeClass = class(
    "java/lang/classfile/ClassTransform",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const METHOD_TYPE_DESC: oxjvm_vm::NativeClass = class(
    "java/lang/constant/MethodTypeDesc",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);