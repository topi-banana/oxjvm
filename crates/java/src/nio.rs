//! A minimal `java.nio.file` over the host [`FileSystem`] boundary: `Paths`, `Path`, `Files` and
//! the option/attribute types the tests link against.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_platform::FileKind;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::{
    boolean, class, field, int, long, method, object, receiver, ref_arg, string_arg, void,
};

// -------------------------------------------------------------------------------------------
// Internal path helpers
// -------------------------------------------------------------------------------------------

pub(crate) fn normalize_path(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if !parts.is_empty() && parts.last() != Some(&"..") {
                    parts.pop();
                } else if !absolute {
                    parts.push("..");
                }
            }
            other => parts.push(other),
        }
    }
    let joined = parts.join("/");
    if absolute {
        alloc::format!("/{joined}")
    } else if joined.is_empty() {
        ".".to_string()
    } else {
        joined
    }
}

fn path_segments(path: &str) -> Vec<&str> {
    path.trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect()
}

fn join_path(base: &str, child: &str) -> String {
    if child.starts_with('/') {
        return child.to_string();
    }
    if base.is_empty() || base == "." {
        return child.to_string();
    }
    if base.ends_with('/') {
        alloc::format!("{base}{child}")
    } else {
        alloc::format!("{base}/{child}")
    }
}

// -------------------------------------------------------------------------------------------
// Path (concrete implementation) + Paths
// -------------------------------------------------------------------------------------------

const PATH_METHODS: [oxjvm_vm::NativeMethodDef; 18] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = string_arg(vm, args[1])?;
            let this = receiver(args);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "path", "Ljava/lang/String;")?;
            let value = vm.make_string(&text)?;
            vm.set_instance_ref(this, declaring, field, value);
            void()
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(path_text(vm, receiver(args))),
    ),
    method(
        "getFileName",
        "()Ljava/nio/file/Path;",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = path_string(vm, receiver(args));
            let segments = path_segments(&path);
            match segments.last() {
                Some(name) => new_path(vm, name),
                None => object(ObjectRef::NULL),
            }
        },
    ),
    method(
        "getParent",
        "()Ljava/nio/file/Path;",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = path_string(vm, receiver(args));
            let Some(parent) = crate::file::parent_of_path(&path) else {
                return object(ObjectRef::NULL);
            };
            new_path(vm, &parent)
        },
    ),
    method(
        "getRoot",
        "()Ljava/nio/file/Path;",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = path_string(vm, receiver(args));
            if path.starts_with('/') {
                new_path(vm, "/")
            } else {
                object(ObjectRef::NULL)
            }
        },
    ),
    method("getNameCount", "()I", ACC_PUBLIC, |vm, _, args| {
        let path = path_string(vm, receiver(args));
        int(path_segments(&path).len() as i32)
    }),
    method(
        "getName",
        "(I)Ljava/nio/file/Path;",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = path_string(vm, receiver(args));
            let segments = path_segments(&path);
            let index = crate::int_arg(args, 1);
            if index < 0 || index as usize >= segments.len() {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("name index")));
            }
            new_path(vm, segments[index as usize])
        },
    ),
    method(
        "resolve",
        "(Ljava/nio/file/Path;)Ljava/nio/file/Path;",
        ACC_PUBLIC,
        |vm, _, args| {
            let base = path_string(vm, receiver(args));
            let other = path_string(vm, ref_arg(args, 1));
            new_path(vm, &join_path(&base, &other))
        },
    ),
    method(
        "resolve",
        "(Ljava/lang/String;)Ljava/nio/file/Path;",
        ACC_PUBLIC,
        |vm, _, args| {
            let base = path_string(vm, receiver(args));
            let other = string_arg(vm, args[1])?;
            new_path(vm, &join_path(&base, &other))
        },
    ),
    method(
        "resolveSibling",
        "(Ljava/nio/file/Path;)Ljava/nio/file/Path;",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = path_string(vm, receiver(args));
            let other = path_string(vm, ref_arg(args, 1));
            let base = crate::file::parent_of_path(&path).unwrap_or_default();
            new_path(vm, &join_path(&base, &other))
        },
    ),
    method(
        "toAbsolutePath",
        "()Ljava/nio/file/Path;",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = path_string(vm, receiver(args));
            let absolute = crate::file::absolute_of_path(vm, &path);
            new_path(vm, &absolute)
        },
    ),
    method(
        "normalize",
        "()Ljava/nio/file/Path;",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = path_string(vm, receiver(args));
            new_path(vm, &normalize_path(&path))
        },
    ),
    method("toFile", "()Ljava/io/File;", ACC_PUBLIC, |vm, _, args| {
        let path = path_string(vm, receiver(args));
        let file_class = vm.resolve_class("java/io/File")?;
        let instance = vm.new_instance(file_class)?;
        let (declaring, ctor) = vm
            .find_method(file_class, "<init>", "(Ljava/lang/String;)V")
            .ok_or_else(|| VmError::internal("File lacks its constructor"))?;
        let text = vm.make_string(&path)?;
        vm.invoke_method(
            declaring,
            ctor,
            vec![Value::Ref(instance), Value::Ref(text)],
        )?;
        object(instance)
    }),
    method("isAbsolute", "()Z", ACC_PUBLIC, |vm, _, args| {
        let path = path_string(vm, receiver(args));
        boolean(path.starts_with('/'))
    }),
    method(
        "startsWith",
        "(Ljava/nio/file/Path;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = path_string(vm, receiver(args));
            let other = path_string(vm, ref_arg(args, 1));
            boolean(path.starts_with(&other))
        },
    ),
    method(
        "endsWith",
        "(Ljava/nio/file/Path;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = path_string(vm, receiver(args));
            let other = path_string(vm, ref_arg(args, 1));
            boolean(path.ends_with(&other))
        },
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
            boolean(path_string(vm, receiver(args)) == path_string(vm, other))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let path = path_string(vm, receiver(args));
        let mut hash = 0i32;
        for unit in path.encode_utf16() {
            hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
        }
        int(hash)
    }),
];

pub(crate) const OXJVM_PATH: oxjvm_vm::NativeClass = class(
    "java/nio/file/OxjvmPath",
    Some("java/lang/Object"),
    &[
        "java/nio/file/Path",
        "java/lang/Comparable",
        "java/lang/Iterable",
    ],
    ACC_PUBLIC,
    &[field("path", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &PATH_METHODS,
    None,
);

const PATH_INTERFACE_METHODS: [oxjvm_vm::NativeMethodDef; 16] = [
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getFileName",
        "()Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getParent",
        "()Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getRoot",
        "()Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getNameCount",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getName",
        "(I)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "resolve",
        "(Ljava/nio/file/Path;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "resolve",
        "(Ljava/lang/String;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "resolveSibling",
        "(Ljava/nio/file/Path;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "toAbsolutePath",
        "()Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "normalize",
        "()Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "toFile",
        "()Ljava/io/File;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "isAbsolute",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "startsWith",
        "(Ljava/nio/file/Path;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "endsWith",
        "(Ljava/nio/file/Path;)Z",
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

pub(crate) const PATH: oxjvm_vm::NativeClass = class(
    "java/nio/file/Path",
    Some("java/lang/Object"),
    &["java/lang/Comparable", "java/lang/Iterable"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &PATH_INTERFACE_METHODS,
    None,
);

const PATHS_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "get",
    "(Ljava/lang/String;[Ljava/lang/String;)Ljava/nio/file/Path;",
    ACC_PUBLIC | ACC_STATIC,
    |vm, _, args| {
        let first = string_arg(vm, args[0])?;
        let mut path = first;
        if let Some(oxjvm_vm::ObjectData::Array(oxjvm_vm::ArrayData::Reference(elements))) = vm
            .heap
            .get(ref_arg(args, 1))
            .map(|object| object.data.clone())
        {
            for element in elements {
                if !element.is_null() {
                    let part = vm.string_or_empty(element);
                    path = join_path(&path, &part);
                }
            }
        }
        new_path(vm, &path)
    },
)];

pub(crate) const PATHS: oxjvm_vm::NativeClass = class(
    "java/nio/file/Paths",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &PATHS_METHODS,
    None,
);

pub(crate) fn path_string(vm: &Vm<'_>, path: ObjectRef) -> String {
    if path.is_null() {
        return String::new();
    }
    if let Some(text) = vm.string_utf16(path) {
        return String::from_utf16_lossy(text);
    }
    let class = vm.class_of(path);
    vm.read_string_field(path, class, "path")
        .unwrap_or_default()
}

fn path_text(vm: &mut Vm<'_>, path: ObjectRef) -> ObjectRef {
    let text = path_string(vm, path);
    vm.make_string(&text).unwrap_or(ObjectRef::NULL)
}

fn new_path(vm: &mut Vm<'_>, path: &str) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/nio/file/OxjvmPath")?;
    let instance = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(class, "<init>", "(Ljava/lang/String;)V")
        .ok_or_else(|| VmError::internal("OxjvmPath lacks its constructor"))?;
    let text = vm.make_string(path)?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![Value::Ref(instance), Value::Ref(text)],
    )?;
    object(instance)
}

// -------------------------------------------------------------------------------------------
// Files
// -------------------------------------------------------------------------------------------

const FILES_METHODS: [oxjvm_vm::NativeMethodDef; 15] = [
    method(
        "createDirectories",
        "(Ljava/nio/file/Path;[Ljava/nio/file/attribute/FileAttribute;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            if !fs_create_dir_all(vm, &path) && !fs_is_dir(vm, &path) {
                return Err(vm.throw_new("java/io/IOException", Some(&path)));
            }
            object(ref_arg(args, 0))
        },
    ),
    method(
        "createDirectory",
        "(Ljava/nio/file/Path;[Ljava/nio/file/attribute/FileAttribute;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            if !fs_create_dir(vm, &path) {
                return Err(vm.throw_new("java/io/IOException", Some(&path)));
            }
            object(ref_arg(args, 0))
        },
    ),
    method(
        "createFile",
        "(Ljava/nio/file/Path;[Ljava/nio/file/attribute/FileAttribute;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            if !fs_write(vm, &path, &[], true) {
                return Err(vm.throw_new("java/io/IOException", Some(&path)));
            }
            object(ref_arg(args, 0))
        },
    ),
    method(
        "copy",
        "(Ljava/io/InputStream;Ljava/nio/file/Path;[Ljava/nio/file/CopyOption;)J",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| files_copy_stream(vm, args),
    ),
    method(
        "copy",
        "(Ljava/nio/file/Path;Ljava/nio/file/Path;[Ljava/nio/file/CopyOption;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let from = path_string(vm, ref_arg(args, 0));
            let to = path_string(vm, ref_arg(args, 1));
            let Some(bytes) = vm.host.filesystem().and_then(|fs| fs.read(&from)) else {
                return Err(vm.throw_new("java/io/IOException", Some(&from)));
            };
            if !fs_write(vm, &to, &bytes, false) {
                return Err(vm.throw_new("java/io/IOException", Some(&to)));
            }
            object(ref_arg(args, 1))
        },
    ),
    method(
        "move",
        "(Ljava/nio/file/Path;Ljava/nio/file/Path;[Ljava/nio/file/CopyOption;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let from = path_string(vm, ref_arg(args, 0));
            let to = path_string(vm, ref_arg(args, 1));
            if !fs_rename(vm, &from, &to) {
                return Err(vm.throw_new("java/io/IOException", Some(&from)));
            }
            object(ref_arg(args, 1))
        },
    ),
    method(
        "write",
        "(Ljava/nio/file/Path;[B[Ljava/nio/file/OpenOption;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
            if !fs_write(vm, &path, &bytes, false) {
                return Err(vm.throw_new("java/io/IOException", Some(&path)));
            }
            object(ref_arg(args, 0))
        },
    ),
    method(
        "readAllBytes",
        "(Ljava/nio/file/Path;)[B",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let Some(bytes) = vm.host.filesystem().and_then(|fs| fs.read(&path)) else {
                return Err(vm.throw_new("java/io/IOException", Some(&path)));
            };
            bytes_array(vm, &bytes)
        },
    ),
    method(
        "exists",
        "(Ljava/nio/file/Path;[Ljava/nio/file/LinkOption;)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            boolean(fs_metadata(vm, &path).is_some())
        },
    ),
    method(
        "isDirectory",
        "(Ljava/nio/file/Path;[Ljava/nio/file/LinkOption;)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            boolean(
                fs_metadata(vm, &path).is_some_and(|metadata| metadata.kind == FileKind::Directory),
            )
        },
    ),
    method(
        "isRegularFile",
        "(Ljava/nio/file/Path;[Ljava/nio/file/LinkOption;)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            boolean(fs_metadata(vm, &path).is_some_and(|metadata| metadata.kind == FileKind::File))
        },
    ),
    method(
        "delete",
        "(Ljava/nio/file/Path;)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let removed = fs_metadata(vm, &path)
                .map(|metadata| match metadata.kind {
                    FileKind::Directory => fs_remove_dir(vm, &path),
                    _ => fs_remove_file(vm, &path),
                })
                .unwrap_or(false);
            if !removed {
                return Err(vm.throw_new("java/io/IOException", Some(&path)));
            }
            void()
        },
    ),
    method(
        "deleteIfExists",
        "(Ljava/nio/file/Path;)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let Some(metadata) = fs_metadata(vm, &path) else {
                return boolean(false);
            };
            boolean(match metadata.kind {
                FileKind::Directory => fs_remove_dir(vm, &path),
                _ => fs_remove_file(vm, &path),
            })
        },
    ),
    method(
        "newInputStream",
        "(Ljava/nio/file/Path;[Ljava/nio/file/OpenOption;)Ljava/io/InputStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let class = vm.resolve_class("java/io/FileInputStream")?;
            let instance = vm.new_instance(class)?;
            let (declaring, ctor) = vm
                .find_method(class, "<init>", "(Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("FileInputStream lacks its constructor"))?;
            let text = vm.make_string(&path)?;
            vm.invoke_method(
                declaring,
                ctor,
                vec![Value::Ref(instance), Value::Ref(text)],
            )?;
            object(instance)
        },
    ),
    method(
        "newOutputStream",
        "(Ljava/nio/file/Path;[Ljava/nio/file/OpenOption;)Ljava/io/OutputStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let class = vm.resolve_class("java/io/FileOutputStream")?;
            let instance = vm.new_instance(class)?;
            let (declaring, ctor) = vm
                .find_method(class, "<init>", "(Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("FileOutputStream lacks its constructor"))?;
            let text = vm.make_string(&path)?;
            vm.invoke_method(
                declaring,
                ctor,
                vec![Value::Ref(instance), Value::Ref(text)],
            )?;
            object(instance)
        },
    ),
];

pub(crate) const FILES: oxjvm_vm::NativeClass = class(
    "java/nio/file/Files",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &FILES_METHODS,
    None,
);

fn files_copy_stream(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let input = ref_arg(args, 0);
    let path = path_string(vm, ref_arg(args, 1));
    if input.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let mut bytes = Vec::new();
    let buffer = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, 8192)?;
    loop {
        let (class, read) = vm.resolve_virtual_method(input, "read", "([B)I")?;
        let count = vm
            .invoke_method(class, read, vec![Value::Ref(input), Value::Ref(buffer)])?
            .as_int();
        if count <= 0 {
            break;
        }
        for index in 0..count {
            bytes.push(vm.array_get(buffer, index)?.as_int() as u8);
        }
    }
    if !fs_write(vm, &path, &bytes, false) {
        return Err(vm.throw_new("java/io/IOException", Some(&path)));
    }
    long(bytes.len() as i64)
}

fn bytes_array(vm: &mut Vm<'_>, bytes: &[u8]) -> Result<Value, VmError> {
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, bytes.len())?;
    for (index, byte) in bytes.iter().enumerate() {
        vm.array_set(
            array,
            index as i32,
            Value::Int(i32::from(*byte as i8)),
            false,
        )?;
    }
    object(array)
}

fn fs_metadata(vm: &mut Vm<'_>, path: &str) -> Option<oxjvm_platform::FileMetadata> {
    vm.host.filesystem()?.metadata(path)
}

fn fs_is_dir(vm: &mut Vm<'_>, path: &str) -> bool {
    fs_metadata(vm, path).is_some_and(|metadata| metadata.kind == FileKind::Directory)
}

fn fs_write(vm: &mut Vm<'_>, path: &str, bytes: &[u8], append: bool) -> bool {
    vm.host
        .filesystem()
        .map(|fs| fs.write(path, bytes, append))
        .unwrap_or(false)
}

fn fs_create_dir(vm: &mut Vm<'_>, path: &str) -> bool {
    vm.host
        .filesystem()
        .map(|fs| fs.create_dir(path))
        .unwrap_or(false)
}

fn fs_create_dir_all(vm: &mut Vm<'_>, path: &str) -> bool {
    vm.host
        .filesystem()
        .map(|fs| fs.create_dir_all(path))
        .unwrap_or(false)
}

fn fs_remove_file(vm: &mut Vm<'_>, path: &str) -> bool {
    vm.host
        .filesystem()
        .map(|fs| fs.remove_file(path))
        .unwrap_or(false)
}

fn fs_remove_dir(vm: &mut Vm<'_>, path: &str) -> bool {
    vm.host
        .filesystem()
        .map(|fs| fs.remove_dir(path))
        .unwrap_or(false)
}

fn fs_rename(vm: &mut Vm<'_>, from: &str, to: &str) -> bool {
    vm.host
        .filesystem()
        .map(|fs| fs.rename(from, to))
        .unwrap_or(false)
}

// -------------------------------------------------------------------------------------------
// Options and attributes
// -------------------------------------------------------------------------------------------

pub(crate) const COPY_OPTION: oxjvm_vm::NativeClass = class(
    "java/nio/file/CopyOption",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const OPEN_OPTION: oxjvm_vm::NativeClass = class(
    "java/nio/file/OpenOption",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const LINK_OPTION: oxjvm_vm::NativeClass = class(
    "java/nio/file/LinkOption",
    Some("java/lang/Object"),
    &["java/nio/file/OpenOption"],
    ACC_PUBLIC | ACC_FINAL,
    &[field(
        "NOFOLLOW_LINKS",
        "Ljava/nio/file/LinkOption;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    )],
    &[method("<init>", "()V", ACC_PRIVATE, |_, _, _| void())],
    Some(link_option_clinit),
);

fn link_option_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/nio/file/LinkOption")?;
    let instance = vm.new_instance(class)?;
    vm.set_static_value(
        class,
        "NOFOLLOW_LINKS",
        "Ljava/nio/file/LinkOption;",
        Value::Ref(instance),
    );
    Ok(())
}

pub(crate) const STANDARD_COPY_OPTION: oxjvm_vm::NativeClass = class(
    "java/nio/file/StandardCopyOption",
    Some("java/lang/Object"),
    &["java/nio/file/CopyOption"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field(
            "REPLACE_EXISTING",
            "Ljava/nio/file/StandardCopyOption;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "COPY_ATTRIBUTES",
            "Ljava/nio/file/StandardCopyOption;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "ATOMIC_MOVE",
            "Ljava/nio/file/StandardCopyOption;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &[method("<init>", "()V", ACC_PRIVATE, |_, _, _| void())],
    Some(standard_copy_option_clinit),
);

fn standard_copy_option_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/nio/file/StandardCopyOption")?;
    for name in ["REPLACE_EXISTING", "COPY_ATTRIBUTES", "ATOMIC_MOVE"] {
        let instance = vm.new_instance(class)?;
        vm.set_static_value(
            class,
            name,
            "Ljava/nio/file/StandardCopyOption;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

pub(crate) const STANDARD_OPEN_OPTION: oxjvm_vm::NativeClass = class(
    "java/nio/file/StandardOpenOption",
    Some("java/lang/Object"),
    &["java/nio/file/OpenOption"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field(
            "READ",
            "Ljava/nio/file/StandardOpenOption;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "WRITE",
            "Ljava/nio/file/StandardOpenOption;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "APPEND",
            "Ljava/nio/file/StandardOpenOption;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "TRUNCATE_EXISTING",
            "Ljava/nio/file/StandardOpenOption;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "CREATE",
            "Ljava/nio/file/StandardOpenOption;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "CREATE_NEW",
            "Ljava/nio/file/StandardOpenOption;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "DELETE_ON_CLOSE",
            "Ljava/nio/file/StandardOpenOption;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "SPARSE",
            "Ljava/nio/file/StandardOpenOption;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "SYNC",
            "Ljava/nio/file/StandardOpenOption;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "DSYNC",
            "Ljava/nio/file/StandardOpenOption;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &[method("<init>", "()V", ACC_PRIVATE, |_, _, _| void())],
    Some(standard_open_option_clinit),
);

fn standard_open_option_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/nio/file/StandardOpenOption")?;
    for name in [
        "READ",
        "WRITE",
        "APPEND",
        "TRUNCATE_EXISTING",
        "CREATE",
        "CREATE_NEW",
        "DELETE_ON_CLOSE",
        "SPARSE",
        "SYNC",
        "DSYNC",
    ] {
        let instance = vm.new_instance(class)?;
        vm.set_static_value(
            class,
            name,
            "Ljava/nio/file/StandardOpenOption;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

const FILE_ATTRIBUTE_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "name",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "value",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const FILE_ATTRIBUTE: oxjvm_vm::NativeClass = class(
    "java/nio/file/attribute/FileAttribute",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &FILE_ATTRIBUTE_METHODS,
    None,
);
