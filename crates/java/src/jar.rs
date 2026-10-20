//! `java.util.jar`: manifest parsing/writing and a jar output stream.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::{boolean, class, field, int, method, object, receiver, ref_arg, string_arg, void};

fn new_attributes(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/jar/Attributes")?;
    let attributes = vm.new_instance(class)?;
    let map = crate::util::hash_map_new(vm)?;
    let (declaring, field) = vm.find_field(class, "map", "Ljava/util/HashMap;")?;
    vm.set_instance_ref(attributes, declaring, field, map);
    Ok(attributes)
}

fn attributes_map(vm: &Vm<'_>, attributes: ObjectRef) -> ObjectRef {
    vm.read_ref_field(
        attributes,
        vm.class_of(attributes),
        "map",
        "Ljava/util/HashMap;",
    )
    .unwrap_or(ObjectRef::NULL)
}

fn attributes_put(
    vm: &mut Vm<'_>,
    attributes: ObjectRef,
    key: ObjectRef,
    value: ObjectRef,
) -> Result<(), VmError> {
    let map = attributes_map(vm, attributes);
    crate::util::hash_map_insert(vm, map, key, value)
}

fn attribute_key(vm: &mut Vm<'_>, key: ObjectRef) -> Result<ObjectRef, VmError> {
    let Some(text) = vm.string_value(key) else {
        return Ok(key);
    };
    let class = vm.resolve_class("java/util/jar/Attributes$Name")?;
    let name = vm.new_instance(class)?;
    vm.set_string_field(name, class, "name", &text)?;
    Ok(name)
}

fn attributes_text(vm: &Vm<'_>, attributes: ObjectRef) -> Vec<(String, String)> {
    let map = attributes_map(vm, attributes);
    let mut pairs = Vec::new();
    for (key, value) in crate::util::hash_map_pairs(vm, map) {
        pairs.push((vm.string_or_empty(key), vm.string_or_empty(value)));
    }
    pairs
}

const ATTRIBUTES_METHODS: [oxjvm_vm::NativeMethodDef; 19] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let attributes = receiver(args);
        let map = crate::util::hash_map_new(vm)?;
        let (declaring, field) =
            vm.find_field(vm.class_of(attributes), "map", "Ljava/util/HashMap;")?;
        vm.set_instance_ref(attributes, declaring, field, map);
        void()
    }),
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let attributes = receiver(args);
        let map = crate::util::hash_map_new(vm)?;
        let (declaring, field) =
            vm.find_field(vm.class_of(attributes), "map", "Ljava/util/HashMap;")?;
        vm.set_instance_ref(attributes, declaring, field, map);
        void()
    }),
    method(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = attributes_map(vm, receiver(args));
            let key = attribute_key(vm, ref_arg(args, 1))?;
            let (class, method) = vm.resolve_virtual_method(
                map,
                "put",
                "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            )?;
            vm.invoke_method(
                class,
                method,
                vec![
                    Value::Ref(map),
                    Value::Ref(key),
                    Value::Ref(ref_arg(args, 2)),
                ],
            )
        },
    ),
    method(
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = attributes_map(vm, receiver(args));
            let key = attribute_key(vm, ref_arg(args, 1))?;
            let (class, method) =
                vm.resolve_virtual_method(map, "get", "(Ljava/lang/Object;)Ljava/lang/Object;")?;
            vm.invoke_method(class, method, vec![Value::Ref(map), Value::Ref(key)])
        },
    ),
    method(
        "getValue",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let key = string_arg(vm, args[1])?;
            let key_text = vm.make_string(&key)?;
            let name = attribute_key(vm, key_text)?;
            let map = attributes_map(vm, receiver(args));
            let (class, method) =
                vm.resolve_virtual_method(map, "get", "(Ljava/lang/Object;)Ljava/lang/Object;")?;
            let value = vm.invoke_method(class, method, vec![Value::Ref(map), Value::Ref(name)])?;
            if value.as_ref().is_null() {
                object(ObjectRef::NULL)
            } else {
                let (class, to_string) =
                    vm.resolve_virtual_method(value.as_ref(), "toString", "()Ljava/lang/String;")?;
                vm.invoke_method(class, to_string, vec![value])
            }
        },
    ),
    method(
        "getValue",
        "(Ljava/util/jar/Attributes$Name;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = ref_arg(args, 1);
            let (class, method) =
                vm.resolve_virtual_method(name, "toString", "()Ljava/lang/String;")?;
            let text = vm.invoke_method(class, method, vec![Value::Ref(name)])?;
            let key = vm.string_or_empty(text.as_ref());
            let map = attributes_map(vm, receiver(args));
            let (class, method) =
                vm.resolve_virtual_method(map, "get", "(Ljava/lang/Object;)Ljava/lang/Object;")?;
            let value = vm.invoke_method(
                class,
                method,
                vec![Value::Ref(map), Value::Ref(text.as_ref())],
            )?;
            let _ = key;
            if value.as_ref().is_null() {
                object(ObjectRef::NULL)
            } else {
                let (class, to_string) =
                    vm.resolve_virtual_method(value.as_ref(), "toString", "()Ljava/lang/String;")?;
                vm.invoke_method(class, to_string, vec![value])
            }
        },
    ),
    method(
        "putValue",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let key = string_arg(vm, args[1])?;
            let value = string_arg(vm, args[2])?;
            let key_object = vm.make_string(&key)?;
            let key_object = attribute_key(vm, key_object)?;
            let value_object = vm.make_string(&value)?;
            let map = attributes_map(vm, receiver(args));
            let (class, method) = vm.resolve_virtual_method(
                map,
                "put",
                "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            )?;
            let previous = vm.invoke_method(
                class,
                method,
                vec![
                    Value::Ref(map),
                    Value::Ref(key_object),
                    Value::Ref(value_object),
                ],
            )?;
            if previous.as_ref().is_null() {
                object(ObjectRef::NULL)
            } else {
                let (class, to_string) = vm.resolve_virtual_method(
                    previous.as_ref(),
                    "toString",
                    "()Ljava/lang/String;",
                )?;
                vm.invoke_method(class, to_string, vec![previous])
            }
        },
    ),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        let map = attributes_map(vm, receiver(args));
        let (class, method) = vm.resolve_virtual_method(map, "size", "()I")?;
        vm.invoke_method(class, method, vec![Value::Ref(map)])
    }),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        let map = attributes_map(vm, receiver(args));
        let (class, method) = vm.resolve_virtual_method(map, "isEmpty", "()Z")?;
        vm.invoke_method(class, method, vec![Value::Ref(map)])
    }),
    method(
        "containsKey",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = attributes_map(vm, receiver(args));
            let key = attribute_key(vm, ref_arg(args, 1))?;
            let (class, method) =
                vm.resolve_virtual_method(map, "containsKey", "(Ljava/lang/Object;)Z")?;
            vm.invoke_method(class, method, vec![Value::Ref(map), Value::Ref(key)])
        },
    ),
    method(
        "containsValue",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = attributes_map(vm, receiver(args));
            let (class, method) =
                vm.resolve_virtual_method(map, "containsValue", "(Ljava/lang/Object;)Z")?;
            vm.invoke_method(
                class,
                method,
                vec![Value::Ref(map), Value::Ref(ref_arg(args, 1))],
            )
        },
    ),
    method(
        "remove",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = attributes_map(vm, receiver(args));
            let key = attribute_key(vm, ref_arg(args, 1))?;
            let (class, method) =
                vm.resolve_virtual_method(map, "remove", "(Ljava/lang/Object;)Ljava/lang/Object;")?;
            vm.invoke_method(class, method, vec![Value::Ref(map), Value::Ref(key)])
        },
    ),
    method("clear", "()V", ACC_PUBLIC, |vm, _, args| {
        let map = attributes_map(vm, receiver(args));
        let (class, method) = vm.resolve_virtual_method(map, "clear", "()V")?;
        vm.invoke_method(class, method, vec![Value::Ref(map)])?;
        void()
    }),
    method("keySet", "()Ljava/util/Set;", ACC_PUBLIC, |vm, _, args| {
        let map = attributes_map(vm, receiver(args));
        let (class, method) = vm.resolve_virtual_method(map, "keySet", "()Ljava/util/Set;")?;
        vm.invoke_method(class, method, vec![Value::Ref(map)])
    }),
    method(
        "entrySet",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = attributes_map(vm, receiver(args));
            let (class, method) =
                vm.resolve_virtual_method(map, "entrySet", "()Ljava/util/Set;")?;
            vm.invoke_method(class, method, vec![Value::Ref(map)])
        },
    ),
    method(
        "values",
        "()Ljava/util/Collection;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = attributes_map(vm, receiver(args));
            let (class, method) =
                vm.resolve_virtual_method(map, "values", "()Ljava/util/Collection;")?;
            vm.invoke_method(class, method, vec![Value::Ref(map)])
        },
    ),
    method(
        "clone",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let copy = new_attributes(vm)?;
            for (key, value) in attributes_text(vm, receiver(args)) {
                let key = vm.make_string(&key)?;
                let value = vm.make_string(&value)?;
                attributes_put(vm, copy, key, value)?;
            }
            object(copy)
        },
    ),
    method("equals", "(Ljava/lang/Object;)Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
    method("hashCode", "()I", ACC_PUBLIC, |_, _, _| int(0)),
];

pub(crate) const ATTRIBUTES: oxjvm_vm::NativeClass = class(
    "java/util/jar/Attributes",
    Some("java/lang/Object"),
    &["java/lang/Cloneable"],
    ACC_PUBLIC,
    &[field("map", "Ljava/util/HashMap;", ACC_PRIVATE, None)],
    &ATTRIBUTES_METHODS,
    None,
);

const ATTRIBUTES_NAME_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = string_arg(vm, args[1])?;
            let this = receiver(args);
            let class = vm.class_of(this);
            vm.set_string_field(this, class, "name", &name)?;
            void()
        },
    ),
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
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() {
                return boolean(false);
            }
            let left = vm
                .read_string_field(receiver(args), vm.class_of(receiver(args)), "name")
                .unwrap_or_default();
            let right = vm
                .read_string_field(other, vm.class_of(other), "name")
                .unwrap_or_default();
            boolean(left == right)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let name = vm
            .read_string_field(receiver(args), vm.class_of(receiver(args)), "name")
            .unwrap_or_default();
        let mut hash = 0i32;
        for unit in name.encode_utf16() {
            hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
        }
        int(hash)
    }),
];

pub(crate) const ATTRIBUTES_NAME: oxjvm_vm::NativeClass = class(
    "java/util/jar/Attributes$Name",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field(
            "MANIFEST_VERSION",
            "Ljava/util/jar/Attributes$Name;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MAIN_CLASS",
            "Ljava/util/jar/Attributes$Name;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "IMPLEMENTATION_TITLE",
            "Ljava/util/jar/Attributes$Name;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "IMPLEMENTATION_VERSION",
            "Ljava/util/jar/Attributes$Name;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "IMPLEMENTATION_VENDOR",
            "Ljava/util/jar/Attributes$Name;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "SPECIFICATION_TITLE",
            "Ljava/util/jar/Attributes$Name;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "SPECIFICATION_VERSION",
            "Ljava/util/jar/Attributes$Name;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "SPECIFICATION_VENDOR",
            "Ljava/util/jar/Attributes$Name;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "SEALED",
            "Ljava/util/jar/Attributes$Name;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &ATTRIBUTES_NAME_METHODS,
    Some(attributes_name_clinit),
);

fn manifest_main(vm: &Vm<'_>, manifest: ObjectRef) -> ObjectRef {
    vm.read_ref_field(
        manifest,
        vm.class_of(manifest),
        "main",
        "Ljava/util/jar/Attributes;",
    )
    .unwrap_or(ObjectRef::NULL)
}

fn manifest_entries(vm: &Vm<'_>, manifest: ObjectRef) -> ObjectRef {
    vm.read_ref_field(
        manifest,
        vm.class_of(manifest),
        "entries",
        "Ljava/util/HashMap;",
    )
    .unwrap_or(ObjectRef::NULL)
}

fn manifest_text(vm: &mut Vm<'_>, manifest: ObjectRef) -> Result<String, VmError> {
    let mut text = String::new();
    text.push_str("Manifest-Version: 1.0\r\n\r\n");
    for (key, value) in attributes_text(vm, manifest_main(vm, manifest)) {
        if key != "Manifest-Version" {
            text.push_str(&alloc::format!("{key}: {value}\r\n"));
        }
    }
    text.push_str("\r\n");
    for (name, attributes) in crate::util::hash_map_pairs(vm, manifest_entries(vm, manifest)) {
        text.push_str(&alloc::format!("Name: {}\r\n", vm.string_or_empty(name)));
        for (key, value) in attributes_text(vm, attributes) {
            text.push_str(&alloc::format!("{key}: {value}\r\n"));
        }
        text.push_str("\r\n");
    }
    Ok(text)
}

const MANIFEST_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        manifest_init(vm, receiver(args), false)
    }),
    method(
        "<init>",
        "(Ljava/io/InputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            manifest_init(vm, receiver(args), true)?;
            manifest_read(vm, receiver(args), ref_arg(args, 1))
        },
    ),
    method(
        "<init>",
        "(Ljava/util/jar/Manifest;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            manifest_init(vm, receiver(args), false)?;
            let source = ref_arg(args, 1);
            for (key, value) in attributes_text(vm, manifest_main(vm, source)) {
                let key = vm.make_string(&key)?;
                let value = vm.make_string(&value)?;
                attributes_put(vm, manifest_main(vm, receiver(args)), key, value)?;
            }
            void()
        },
    ),
    method(
        "getMainAttributes",
        "()Ljava/util/jar/Attributes;",
        ACC_PUBLIC,
        |vm, _, args| object(manifest_main(vm, receiver(args))),
    ),
    method(
        "getEntries",
        "()Ljava/util/Map;",
        ACC_PUBLIC,
        |vm, _, args| object(manifest_entries(vm, receiver(args))),
    ),
    method(
        "getAttributes",
        "(Ljava/lang/String;)Ljava/util/jar/Attributes;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = ref_arg(args, 1);
            let map = manifest_entries(vm, receiver(args));
            let (class, method) =
                vm.resolve_virtual_method(map, "get", "(Ljava/lang/Object;)Ljava/lang/Object;")?;
            vm.invoke_method(class, method, vec![Value::Ref(map), Value::Ref(name)])
        },
    ),
    method("clear", "()V", ACC_PUBLIC, |vm, _, args| {
        let main = manifest_main(vm, receiver(args));
        for (key, _) in attributes_text(vm, main) {
            let key = vm.make_string(&key)?;
            let (class, method) = vm.resolve_virtual_method(
                map_of(vm, main),
                "remove",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
            )?;
            vm.invoke_method(
                class,
                method,
                vec![Value::Ref(map_of(vm, main)), Value::Ref(key)],
            )?;
        }
        let entries = manifest_entries(vm, receiver(args));
        let (class, method) = vm.resolve_virtual_method(entries, "clear", "()V")?;
        vm.invoke_method(class, method, vec![Value::Ref(entries)])?;
        void()
    }),
    method(
        "write",
        "(Ljava/io/OutputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = manifest_text(vm, receiver(args))?;
            let out = ref_arg(args, 1);
            let bytes = text.as_bytes();
            let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, bytes.len())?;
            for (index, byte) in bytes.iter().enumerate() {
                vm.array_set(
                    array,
                    index as i32,
                    Value::Int(i32::from(*byte as i8)),
                    false,
                )?;
            }
            let (class, method) = vm.resolve_virtual_method(out, "write", "([B)V")?;
            vm.invoke_method(class, method, vec![Value::Ref(out), Value::Ref(array)])?;
            void()
        },
    ),
    method(
        "read",
        "(Ljava/io/InputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| manifest_read(vm, receiver(args), ref_arg(args, 1)),
    ),
    method("equals", "(Ljava/lang/Object;)Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
];

fn map_of(vm: &Vm<'_>, attributes: ObjectRef) -> ObjectRef {
    attributes_map(vm, attributes)
}

fn manifest_init(vm: &mut Vm<'_>, manifest: ObjectRef, _read: bool) -> Result<Value, VmError> {
    let main = new_attributes(vm)?;
    let entries = crate::util::hash_map_new(vm)?;
    let class = vm.class_of(manifest);
    let (declaring, field) = vm.find_field(class, "main", "Ljava/util/jar/Attributes;")?;
    vm.set_instance_ref(manifest, declaring, field, main);
    let (declaring, field) = vm.find_field(class, "entries", "Ljava/util/HashMap;")?;
    vm.set_instance_ref(manifest, declaring, field, entries);
    void()
}

fn read_stream_text(vm: &mut Vm<'_>, input: ObjectRef) -> Result<String, VmError> {
    let mut bytes = Vec::new();
    loop {
        let (class, method) = vm.resolve_virtual_method(input, "read", "()I")?;
        let value = vm
            .invoke_method(class, method, vec![Value::Ref(input)])?
            .as_int();
        if value < 0 {
            break;
        }
        bytes.push(value as u8);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn manifest_read(vm: &mut Vm<'_>, manifest: ObjectRef, input: ObjectRef) -> Result<Value, VmError> {
    let text = read_stream_text(vm, input)?;
    let main = manifest_main(vm, manifest);
    let entries = manifest_entries(vm, manifest);
    let mut current = main;
    let mut section_name: Option<String> = None;
    for line in text.lines() {
        if line.trim().is_empty() {
            if let Some(name) = section_name.take() {
                let key = vm.make_string(&name)?;
                let attributes = new_attributes(vm)?;
                crate::util::hash_map_insert(vm, entries, key, attributes)?;
                current = attributes;
            }
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim_start();
        if key == "Name" {
            section_name = Some(value.to_string());
        } else {
            let key = vm.make_string(key)?;
            let value = vm.make_string(value)?;
            attributes_put(vm, current, key, value)?;
        }
    }
    void()
}

fn attributes_name_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/util/jar/Attributes$Name")?;
    for (field, value) in [
        ("MANIFEST_VERSION", "Manifest-Version"),
        ("MAIN_CLASS", "Main-Class"),
        ("IMPLEMENTATION_TITLE", "Implementation-Title"),
        ("IMPLEMENTATION_VERSION", "Implementation-Version"),
        ("IMPLEMENTATION_VENDOR", "Implementation-Vendor"),
        ("SPECIFICATION_TITLE", "Specification-Title"),
        ("SPECIFICATION_VERSION", "Specification-Version"),
        ("SPECIFICATION_VENDOR", "Specification-Vendor"),
        ("SEALED", "Sealed"),
    ] {
        let instance = vm.new_instance(class)?;
        vm.set_string_field(instance, class, "name", value)?;
        vm.set_static_value(
            class,
            field,
            "Ljava/util/jar/Attributes$Name;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

pub(crate) const MANIFEST: oxjvm_vm::NativeClass = class(
    "java/util/jar/Manifest",
    Some("java/lang/Object"),
    &["java/lang/Cloneable"],
    ACC_PUBLIC,
    &[
        field("main", "Ljava/util/jar/Attributes;", ACC_PRIVATE, None),
        field("entries", "Ljava/util/HashMap;", ACC_PRIVATE, None),
    ],
    &MANIFEST_METHODS,
    None,
);

const JAR_ENTRY_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let name = string_arg(vm, args[1])?;
            vm.set_string_field(this, class, "name", &name)?;
            let attributes = new_attributes(vm)?;
            let (declaring, field) =
                vm.find_field(class, "attributes", "Ljava/util/jar/Attributes;")?;
            vm.set_instance_ref(this, declaring, field, attributes);
            void()
        },
    ),
    method(
        "getAttributes",
        "()Ljava/util/jar/Attributes;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(
                vm.read_ref_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "attributes",
                    "Ljava/util/jar/Attributes;",
                )
                .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method(
        "getCertificates",
        "()[Ljava/security/cert/Certificate;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/security/cert/Certificate")?;
            object(vm.allocate_object_array(class, 0)?)
        },
    ),
    method(
        "getCodeSigners",
        "()[Ljava/security/CodeSigner;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
];

pub(crate) const JAR_ENTRY: oxjvm_vm::NativeClass = class(
    "java/util/jar/JarEntry",
    Some("java/util/zip/ZipEntry"),
    &[],
    ACC_PUBLIC,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field(
            "attributes",
            "Ljava/util/jar/Attributes;",
            ACC_PRIVATE,
            None,
        ),
    ],
    &JAR_ENTRY_METHODS,
    None,
);

fn jar_write_manifest(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    let manifest = vm
        .read_ref_field(
            this,
            vm.class_of(this),
            "manifest",
            "Ljava/util/jar/Manifest;",
        )
        .unwrap_or(ObjectRef::NULL);
    if manifest.is_null() || crate::lang::read_int_field(vm, this, "manifestWritten") != 0 {
        return Ok(());
    }
    let zip_class = vm.resolve_class("java/util/zip/ZipOutputStream")?;
    let entry_class = vm.resolve_class("java/util/zip/ZipEntry")?;
    let entry = vm.new_instance(entry_class)?;
    let (declaring, ctor) = vm
        .find_method(entry_class, "<init>", "(Ljava/lang/String;)V")
        .ok_or_else(|| VmError::internal("ZipEntry ctor missing"))?;
    let name = vm.make_string("META-INF/MANIFEST.MF")?;
    vm.invoke_method(declaring, ctor, vec![Value::Ref(entry), Value::Ref(name)])?;
    let (declaring, put) = vm
        .find_method(zip_class, "putNextEntry", "(Ljava/util/zip/ZipEntry;)V")
        .ok_or_else(|| VmError::internal("ZipOutputStream.putNextEntry missing"))?;
    vm.invoke_method(declaring, put, vec![Value::Ref(this), Value::Ref(entry)])?;
    let text = manifest_text(vm, manifest)?;
    let bytes = text.as_bytes();
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, bytes.len())?;
    for (index, byte) in bytes.iter().enumerate() {
        vm.array_set(
            array,
            index as i32,
            Value::Int(i32::from(*byte as i8)),
            false,
        )?;
    }
    let (declaring, write) = vm
        .find_method(zip_class, "write", "([B)V")
        .ok_or_else(|| VmError::internal("ZipOutputStream.write missing"))?;
    vm.invoke_method(declaring, write, vec![Value::Ref(this), Value::Ref(array)])?;
    let (declaring, close) = vm
        .find_method(zip_class, "closeEntry", "()V")
        .ok_or_else(|| VmError::internal("ZipOutputStream.closeEntry missing"))?;
    vm.invoke_method(declaring, close, vec![Value::Ref(this)])?;
    crate::lang::write_int_field(vm, this, "manifestWritten", 1);
    Ok(())
}

const JAR_OUTPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "<init>",
        "(Ljava/io/OutputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| jar_output_init(vm, receiver(args), ref_arg(args, 1), ObjectRef::NULL),
    ),
    method(
        "<init>",
        "(Ljava/io/OutputStream;Ljava/util/jar/Manifest;)V",
        ACC_PUBLIC,
        |vm, _, args| jar_output_init(vm, receiver(args), ref_arg(args, 1), ref_arg(args, 2)),
    ),
    method(
        "putNextEntry",
        "(Ljava/util/zip/ZipEntry;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            jar_write_manifest(vm, receiver(args))?;
            let zip_class = vm.resolve_class("java/util/zip/ZipOutputStream")?;
            let (declaring, method) = vm
                .find_method(zip_class, "putNextEntry", "(Ljava/util/zip/ZipEntry;)V")
                .ok_or_else(|| VmError::internal("ZipOutputStream.putNextEntry missing"))?;
            vm.invoke_method(
                declaring,
                method,
                vec![Value::Ref(receiver(args)), Value::Ref(ref_arg(args, 1))],
            )?;
            void()
        },
    ),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        jar_write_manifest(vm, receiver(args))?;
        let zip_class = vm.resolve_class("java/util/zip/ZipOutputStream")?;
        let (declaring, method) = vm
            .find_method(zip_class, "close", "()V")
            .ok_or_else(|| VmError::internal("ZipOutputStream.close missing"))?;
        vm.invoke_method(declaring, method, vec![Value::Ref(receiver(args))])?;
        void()
    }),
];

fn jar_output_init(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    out: ObjectRef,
    manifest: ObjectRef,
) -> Result<Value, VmError> {
    let zip_class = vm.resolve_class("java/util/zip/ZipOutputStream")?;
    let (declaring, ctor) = vm
        .find_method(zip_class, "<init>", "(Ljava/io/OutputStream;)V")
        .ok_or_else(|| VmError::internal("ZipOutputStream(OutputStream) missing"))?;
    vm.invoke_method(declaring, ctor, vec![Value::Ref(this), Value::Ref(out)])?;
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "manifest", "Ljava/util/jar/Manifest;")?;
    vm.set_instance_ref(this, declaring, field, manifest);
    crate::lang::write_int_field(vm, this, "manifestWritten", 0);
    void()
}

pub(crate) const JAR_OUTPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/util/jar/JarOutputStream",
    Some("java/util/zip/ZipOutputStream"),
    &[],
    ACC_PUBLIC,
    &[
        field("manifest", "Ljava/util/jar/Manifest;", ACC_PRIVATE, None),
        field("manifestWritten", "I", ACC_PRIVATE, None),
    ],
    &JAR_OUTPUT_STREAM_METHODS,
    None,
);
