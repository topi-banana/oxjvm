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

const PATH_METHODS: [oxjvm_vm::NativeMethodDef; 21] = [
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
        "resolveSibling",
        "(Ljava/lang/String;)Ljava/nio/file/Path;",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = path_string(vm, receiver(args));
            let other = string_arg(vm, args[1])?;
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
    method("toUri", "()Ljava/net/URI;", ACC_PUBLIC, |vm, _, args| {
        let path = path_string(vm, receiver(args));
        let text = if path.starts_with('/') {
            alloc::format!("file:{path}")
        } else {
            alloc::format!("file:/{path}")
        };
        crate::uri::new_uri(vm, &text)
    }),
    method(
        "relativize",
        "(Ljava/nio/file/Path;)Ljava/nio/file/Path;",
        ACC_PUBLIC,
        |vm, _, args| {
            let base = path_string(vm, receiver(args));
            let other = path_string(vm, ref_arg(args, 1));
            let base_parts: Vec<&str> = base.split('/').filter(|p| !p.is_empty()).collect();
            let other_parts: Vec<&str> = other.split('/').filter(|p| !p.is_empty()).collect();
            let mut common = 0;
            while common < base_parts.len()
                && common < other_parts.len()
                && base_parts[common] == other_parts[common]
            {
                common += 1;
            }
            if common == 0 && base_parts.first() != other_parts.first() {
                // Not relativizable: the JDK returns the other path unchanged.
                return new_path(vm, &other);
            }
            let mut result = vec![".."; base_parts.len() - common];
            result.extend_from_slice(&other_parts[common..]);
            new_path(vm, &result.join("/"))
        },
    ),
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

const PATH_INTERFACE_METHODS: [oxjvm_vm::NativeMethodDef; 19] = [
    method(
        "of",
        "(Ljava/lang/String;[Ljava/lang/String;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| path_of(vm, args),
    ),
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
    method(
        "toUri",
        "()Ljava/net/URI;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "relativize",
        "(Ljava/nio/file/Path;)Ljava/nio/file/Path;",
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
    |vm, _, args| path_of(vm, args),
)];

/// `Paths.get(first, more...)` / `Path.of(first, more...)`.
fn path_of(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
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
}

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

pub(crate) fn new_path(vm: &mut Vm<'_>, path: &str) -> Result<Value, VmError> {
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

const FILES_METHODS: [oxjvm_vm::NativeMethodDef; 32] = [
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
    method(
        "createTempDirectory",
        "(Ljava/lang/String;[Ljava/nio/file/attribute/FileAttribute;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let prefix = string_arg(vm, args[0])?;
            let dir =
                crate::lang::system_property(vm, "java.io.tmpdir").unwrap_or_else(|| "/tmp".into());
            let seed = vm.host.random_seed();
            let path = alloc::format!("{dir}/{prefix}{seed:016x}");
            let created = vm
                .host
                .filesystem()
                .map(|fs| fs.create_dir(&path))
                .unwrap_or(false);
            if !created {
                return Err(vm.throw_new("java/io/IOException", Some(&path)));
            }
            new_path(vm, &path)
        },
    ),
    method(
        "list",
        "(Ljava/nio/file/Path;)Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let names = vm
                .host
                .filesystem()
                .and_then(|fs| fs.list(&path))
                .unwrap_or_default();
            let mut paths = Vec::new();
            for name in names {
                paths.push(new_path(vm, &join_path(&path, &name))?.as_ref());
            }
            object(crate::stream::stream_from(vm, paths)?)
        },
    ),
    method(
        "notExists",
        "(Ljava/nio/file/Path;[Ljava/nio/file/LinkOption;)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            boolean(fs_metadata(vm, &path).is_none())
        },
    ),
    method(
        "write",
        "(Ljava/nio/file/Path;Ljava/lang/Iterable;[Ljava/nio/file/OpenOption;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let mut text = String::new();
            for element in crate::util::collection_elements(vm, ref_arg(args, 1))? {
                text.push_str(&text_of(vm, element)?);
                text.push('\n');
            }
            files_write_text(vm, ref_arg(args, 0), &text)
        },
    ),
    method(
        "writeString",
        "(Ljava/nio/file/Path;Ljava/lang/CharSequence;[Ljava/nio/file/OpenOption;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 1));
            files_write_text(vm, ref_arg(args, 0), &text)
        },
    ),
    method(
        "createTempDirectory",
        "(Ljava/nio/file/Path;Ljava/lang/String;[Ljava/nio/file/attribute/FileAttribute;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let dir = path_string(vm, ref_arg(args, 0));
            let prefix = string_arg(vm, args[1])?;
            let seed = vm.host.random_seed();
            let path = alloc::format!("{dir}/{prefix}{seed:016x}");
            if !fs_create_dir(vm, &path) {
                return Err(vm.throw_new("java/io/IOException", Some(&path)));
            }
            new_path(vm, &path)
        },
    ),
    method(
        "createTempFile",
        "(Ljava/nio/file/Path;Ljava/lang/String;Ljava/lang/String;[Ljava/nio/file/attribute/FileAttribute;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let dir = path_string(vm, ref_arg(args, 0));
            let prefix = string_arg(vm, args[1])?;
            let suffix = vm.string_or_empty(ref_arg(args, 2));
            let seed = vm.host.random_seed();
            let path = alloc::format!("{dir}/{prefix}{seed:016x}{suffix}");
            if !fs_write(vm, &path, &[], false) {
                return Err(vm.throw_new("java/io/IOException", Some(&path)));
            }
            new_path(vm, &path)
        },
    ),
    method(
        "readAttributes",
        "(Ljava/nio/file/Path;Ljava/lang/Class;[Ljava/nio/file/LinkOption;)Ljava/nio/file/attribute/BasicFileAttributes;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let Some(metadata) = fs_metadata(vm, &path) else {
                return Err(vm.throw_new("java/io/IOException", Some(&path)));
            };
            let class = vm.resolve_class("java/nio/file/attribute/OjvmBasicFileAttributes")?;
            let instance = vm.new_instance(class)?;
            let kind = match metadata.kind {
                FileKind::Directory => 1,
                FileKind::File => 0,
                _ => 2,
            };
            if let Some((declaring, field)) = vm.find_field_opt(class, "kind", "I") {
                vm.set_instance_int(instance, declaring, field, kind);
            }
            if let Some((declaring, field)) = vm.find_field_opt(class, "size", "J") {
                vm.set_instance_long(instance, declaring, field, metadata.len as i64);
            }
            for name in ["created", "accessed", "modified"] {
                if let Some((declaring, field)) = vm.find_field_opt(class, name, "J") {
                    vm.set_instance_long(instance, declaring, field, metadata.modified_millis);
                }
            }
            object(instance)
        },
    ),
    method(
        "createTempFile",
        "(Ljava/lang/String;Ljava/lang/String;[Ljava/nio/file/attribute/FileAttribute;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let prefix = string_arg(vm, args[0])?;
            let suffix = vm.string_or_empty(ref_arg(args, 1));
            let dir = crate::lang::system_property(vm, "java.io.tmpdir")
                .unwrap_or_else(|| String::from("/tmp"));
            let seed = vm.host.random_seed();
            let path = alloc::format!("{dir}/{prefix}{seed:016x}{suffix}");
            if !fs_write(vm, &path, &[], false) {
                return Err(vm.throw_new("java/io/IOException", Some(&path)));
            }
            new_path(vm, &path)
        },
    ),
    method(
        "walk",
        "(Ljava/nio/file/Path;[Ljava/nio/file/FileVisitOption;)Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let mut paths = Vec::new();
            walk_paths(vm, &path, 0, usize::MAX, &mut paths)?;
            paths_to_stream(vm, &paths)
        },
    ),
    method(
        "walk",
        "(Ljava/nio/file/Path;I[Ljava/nio/file/FileVisitOption;)Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let max_depth = crate::int_arg(args, 1).max(0) as usize;
            let mut paths = Vec::new();
            walk_paths(vm, &path, 0, max_depth, &mut paths)?;
            paths_to_stream(vm, &paths)
        },
    ),
    method(
        "find",
        "(Ljava/nio/file/Path;ILjava/util/function/BiPredicate;[Ljava/nio/file/FileVisitOption;)Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let max_depth = crate::int_arg(args, 1).max(0) as usize;
            let predicate = ref_arg(args, 2);
            let mut paths = Vec::new();
            walk_paths(vm, &path, 0, max_depth, &mut paths)?;
            let mut kept = Vec::new();
            for candidate in &paths {
                let attributes = make_basic_attributes(vm, candidate)?;
                let instance = new_path(vm, candidate)?.as_ref();
                let (class, method) = vm.resolve_virtual_method(
                    predicate,
                    "test",
                    "(Ljava/lang/Object;Ljava/lang/Object;)Z",
                )?;
                let matched = vm
                    .invoke_method(
                        class,
                        method,
                        vec![
                            Value::Ref(predicate),
                            Value::Ref(instance),
                            Value::Ref(attributes),
                        ],
                    )?
                    .as_int();
                if matched != 0 {
                    kept.push(instance);
                }
            }
            object(crate::stream::stream_from(vm, kept)?)
        },
    ),
    method(
        "newBufferedWriter",
        "(Ljava/nio/file/Path;[Ljava/nio/file/OpenOption;)Ljava/io/BufferedWriter;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let writer_class = vm.resolve_class("java/io/FileWriter")?;
            let writer = vm.new_instance(writer_class)?;
            let (declaring, ctor) = vm
                .find_method(writer_class, "<init>", "(Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("FileWriter(String) missing"))?;
            let path_text = vm.make_string(&path)?;
            vm.invoke_method(
                declaring,
                ctor,
                vec![Value::Ref(writer), Value::Ref(path_text)],
            )?;
            let buffered_class = vm.resolve_class("java/io/BufferedWriter")?;
            let buffered = vm.new_instance(buffered_class)?;
            let (declaring, ctor) = vm
                .find_method(buffered_class, "<init>", "(Ljava/io/Writer;)V")
                .ok_or_else(|| VmError::internal("BufferedWriter(Writer) missing"))?;
            vm.invoke_method(
                declaring,
                ctor,
                vec![Value::Ref(buffered), Value::Ref(writer)],
            )?;
            object(buffered)
        },
    ),
    method(
        "readString",
        "(Ljava/nio/file/Path;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let text = read_text_file(vm, &path)?;
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "readString",
        "(Ljava/nio/file/Path;Ljava/nio/charset/Charset;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let text = read_text_file(vm, &path)?;
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "readAllLines",
        "(Ljava/nio/file/Path;Ljava/nio/charset/Charset;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let text = read_text_file(vm, &path)?;
            let mut lines = Vec::new();
            for line in text.lines() {
                lines.push(vm.make_string(line)?);
            }
            object(crate::util::array_list_from(vm, &lines)?)
        },
    ),
    method(
        "lines",
        "(Ljava/nio/file/Path;)Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let text = read_text_file(vm, &path)?;
            let mut lines = Vec::new();
            for line in text.lines() {
                lines.push(vm.make_string(line)?);
            }
            object(crate::stream::stream_from(vm, lines)?)
        },
    ),
];

fn read_text_file(vm: &mut Vm<'_>, path: &str) -> Result<String, VmError> {
    let Some(bytes) = vm.host.filesystem().and_then(|fs| fs.read(path)) else {
        return Err(vm.throw_new("java/io/IOException", Some(path)));
    };
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

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

/// A CharSequence as text (via `toString` for non-Strings).
fn text_of(vm: &mut Vm<'_>, reference: ObjectRef) -> Result<String, VmError> {
    if let Some(text) = vm.string_value(reference) {
        return Ok(text);
    }
    let (class, method) =
        vm.resolve_virtual_method(reference, "toString", "()Ljava/lang/String;")?;
    let text = vm.invoke_method(class, method, vec![Value::Ref(reference)])?;
    Ok(vm.string_or_empty(text.as_ref()))
}

/// Write `text` (UTF-8) to a `Path`, returning the path.
fn files_write_text(vm: &mut Vm<'_>, path_ref: ObjectRef, text: &str) -> Result<Value, VmError> {
    let path = path_string(vm, path_ref);
    if !fs_write(vm, &path, text.as_bytes(), false) {
        return Err(vm.throw_new("java/io/IOException", Some(&path)));
    }
    object(path_ref)
}

fn fs_is_dir(vm: &mut Vm<'_>, path: &str) -> bool {
    fs_metadata(vm, path).is_some_and(|metadata| metadata.kind == FileKind::Directory)
}

fn fs_list(vm: &mut Vm<'_>, path: &str) -> Option<Vec<String>> {
    vm.host.filesystem()?.list(path)
}

/// Build an `OjvmBasicFileAttributes` for `path`, as `Files.readAttributes`/`Files.find` need.
fn make_basic_attributes(vm: &mut Vm<'_>, path: &str) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/nio/file/attribute/OjvmBasicFileAttributes")?;
    let instance = vm.new_instance(class)?;
    let metadata = fs_metadata(vm, path);
    let (kind, len, modified) = match metadata {
        Some(metadata) => (
            match metadata.kind {
                FileKind::Directory => 1,
                FileKind::File => 0,
                _ => 2,
            },
            metadata.len as i64,
            metadata.modified_millis,
        ),
        None => (2, 0, 0),
    };
    if let Some((declaring, field)) = vm.find_field_opt(class, "kind", "I") {
        vm.set_instance_int(instance, declaring, field, kind);
    }
    if let Some((declaring, field)) = vm.find_field_opt(class, "size", "J") {
        vm.set_instance_long(instance, declaring, field, len);
    }
    for name in ["created", "accessed", "modified"] {
        if let Some((declaring, field)) = vm.find_field_opt(class, name, "J") {
            vm.set_instance_long(instance, declaring, field, modified);
        }
    }
    Ok(instance)
}

/// Depth-first walk of `path` (a file or a directory), appending every visited path.
fn walk_paths(
    vm: &mut Vm<'_>,
    path: &str,
    depth: usize,
    max_depth: usize,
    out: &mut Vec<String>,
) -> Result<(), VmError> {
    out.push(String::from(path));
    if fs_is_dir(vm, path) {
        if max_depth != usize::MAX && depth >= max_depth {
            return Ok(());
        }
        if let Some(entries) = fs_list(vm, path) {
            let mut sorted = entries;
            sorted.sort();
            for entry in sorted {
                let child = join_path(path, &entry);
                walk_paths(vm, &child, depth + 1, max_depth, out)?;
            }
        }
    }
    Ok(())
}

fn paths_to_stream(vm: &mut Vm<'_>, paths: &[String]) -> Result<Value, VmError> {
    let mut objects = Vec::with_capacity(paths.len());
    for path in paths {
        objects.push(new_path(vm, path)?.as_ref());
    }
    object(crate::stream::stream_from(vm, objects)?)
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

// -------------------------------------------------------------------------------------------
// FileTime / BasicFileAttributes / FileVisitOption
// -------------------------------------------------------------------------------------------

const FILE_TIME_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method("<init>", "(J)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if let Some((declaring, field)) = vm.find_field_opt(vm.class_of(this), "millis", "J") {
            vm.set_instance_long(this, declaring, field, crate::long_arg(args, 1));
        }
        void()
    }),
    method(
        "fromMillis",
        "(J)Ljava/nio/file/attribute/FileTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let class = vm.resolve_class("java/nio/file/attribute/FileTime")?;
            let instance = vm.new_instance(class)?;
            let (declaring, field) = vm.find_field(class, "millis", "J")?;
            vm.set_instance_long(instance, declaring, field, crate::long_arg(args, 0));
            object(instance)
        },
    ),
    method("toMillis", "()J", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        long(
            vm.read_instance_long_named(this, class, "millis", "J")
                .unwrap_or(0),
        )
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let millis = vm
                .read_instance_long_named(this, class, "millis", "J")
                .unwrap_or(0);
            object(vm.make_string(&alloc::format!("{millis}"))?)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() || vm.class_of(receiver(args)) != vm.class_of(other) {
                return boolean(false);
            }
            let this_millis = vm
                .read_instance_long_named(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "millis",
                    "J",
                )
                .unwrap_or(0);
            let other_millis = vm
                .read_instance_long_named(other, vm.class_of(other), "millis", "J")
                .unwrap_or(0);
            boolean(this_millis == other_millis)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let millis = vm
            .read_instance_long_named(this, vm.class_of(this), "millis", "J")
            .unwrap_or(0);
        int((millis ^ (millis >> 32)) as i32)
    }),
];

pub(crate) const FILE_TIME: oxjvm_vm::NativeClass = class(
    "java/nio/file/attribute/FileTime",
    Some("java/lang/Object"),
    &["java/lang/Comparable"],
    ACC_PUBLIC | ACC_FINAL,
    &[field("millis", "J", ACC_PRIVATE, None)],
    &FILE_TIME_METHODS,
    None,
);

const BASIC_FILE_ATTRIBUTES_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method(
        "isDirectory",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "isRegularFile",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "isSymbolicLink",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "isOther",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "size",
        "()J",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "creationTime",
        "()Ljava/nio/file/attribute/FileTime;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "lastAccessTime",
        "()Ljava/nio/file/attribute/FileTime;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "lastModifiedTime",
        "()Ljava/nio/file/attribute/FileTime;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "fileKey",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const BASIC_FILE_ATTRIBUTES: oxjvm_vm::NativeClass = class(
    "java/nio/file/attribute/BasicFileAttributes",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &BASIC_FILE_ATTRIBUTES_METHODS,
    None,
);

fn attribute_kind(vm: &Vm<'_>, this: ObjectRef) -> i32 {
    crate::lang::read_int_field(vm, this, "kind")
}

fn file_time_of(vm: &mut Vm<'_>, millis: i64) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/nio/file/attribute/FileTime")?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "millis", "J")?;
    vm.set_instance_long(instance, declaring, field, millis);
    object(instance)
}

const OJVM_BASIC_FILE_ATTRIBUTES_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method("isDirectory", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(attribute_kind(vm, receiver(args)) == 1)
    }),
    method("isRegularFile", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(attribute_kind(vm, receiver(args)) == 0)
    }),
    method("isSymbolicLink", "()Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
    method("isOther", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(attribute_kind(vm, receiver(args)) == 2)
    }),
    method("size", "()J", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        long(
            vm.read_instance_long_named(this, vm.class_of(this), "size", "J")
                .unwrap_or(0),
        )
    }),
    method(
        "creationTime",
        "()Ljava/nio/file/attribute/FileTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let created = vm
                .read_instance_long_named(this, vm.class_of(this), "created", "J")
                .unwrap_or(0);
            file_time_of(vm, created)
        },
    ),
    method(
        "lastAccessTime",
        "()Ljava/nio/file/attribute/FileTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let accessed = vm
                .read_instance_long_named(this, vm.class_of(this), "accessed", "J")
                .unwrap_or(0);
            file_time_of(vm, accessed)
        },
    ),
    method(
        "lastModifiedTime",
        "()Ljava/nio/file/attribute/FileTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let modified = vm
                .read_instance_long_named(this, vm.class_of(this), "modified", "J")
                .unwrap_or(0);
            file_time_of(vm, modified)
        },
    ),
    method("fileKey", "()Ljava/lang/Object;", ACC_PUBLIC, |_, _, _| {
        object(ObjectRef::NULL)
    }),
];

pub(crate) const OJVM_BASIC_FILE_ATTRIBUTES: oxjvm_vm::NativeClass = class(
    "java/nio/file/attribute/OjvmBasicFileAttributes",
    Some("java/lang/Object"),
    &["java/nio/file/attribute/BasicFileAttributes"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[
        field("kind", "I", ACC_PRIVATE, None),
        field("size", "J", ACC_PRIVATE, None),
        field("created", "J", ACC_PRIVATE, None),
        field("accessed", "J", ACC_PRIVATE, None),
        field("modified", "J", ACC_PRIVATE, None),
    ],
    &OJVM_BASIC_FILE_ATTRIBUTES_METHODS,
    None,
);

// FileVisitOption (enum-shaped)
const FILE_VISIT_OPTION_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("ordinal", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, _| {
        object(vm.make_string("FOLLOW_LINKS")?)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("FOLLOW_LINKS")?),
    ),
    method(
        "values",
        "()[Ljava/nio/file/FileVisitOption;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/nio/file/FileVisitOption")?;
            let array = vm.allocate_object_array(class, 1)?;
            if let Some(option) =
                vm.read_static_ref_if_set(class, "FOLLOW_LINKS", "Ljava/nio/file/FileVisitOption;")
            {
                vm.array_set_ref(array, 0, option)?;
            }
            object(array)
        },
    ),
    method(
        "valueOf",
        "(Ljava/lang/String;)Ljava/nio/file/FileVisitOption;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = vm.string_or_empty(ref_arg(args, 0));
            let class = vm.resolve_class("java/nio/file/FileVisitOption")?;
            if name == "FOLLOW_LINKS" {
                if let Some(option) = vm.read_static_ref_if_set(
                    class,
                    "FOLLOW_LINKS",
                    "Ljava/nio/file/FileVisitOption;",
                ) {
                    return object(option);
                }
            }
            Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some(&alloc::format!(
                    "No enum constant java.nio.file.FileVisitOption.{name}"
                )),
            ))
        },
    ),
];

fn file_visit_option_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/nio/file/FileVisitOption")?;
    let instance = vm.new_instance(class)?;
    vm.set_static_value(
        class,
        "FOLLOW_LINKS",
        "Ljava/nio/file/FileVisitOption;",
        Value::Ref(instance),
    );
    Ok(())
}

pub(crate) const FILE_VISIT_OPTION: oxjvm_vm::NativeClass = class(
    "java/nio/file/FileVisitOption",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[field(
        "FOLLOW_LINKS",
        "Ljava/nio/file/FileVisitOption;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    )],
    &FILE_VISIT_OPTION_METHODS,
    Some(file_visit_option_clinit),
);

// -------------------------------------------------------------------------------------------
// DirectoryStream / DirectoryStream$Filter / FilenameFilter
// -------------------------------------------------------------------------------------------

pub(crate) const DIRECTORY_STREAM: oxjvm_vm::NativeClass = class(
    "java/nio/file/DirectoryStream",
    Some("java/lang/Object"),
    &["java/io/Closeable", "java/lang/Iterable"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[method(
        "iterator",
        "()Ljava/util/Iterator;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    )],
    None,
);

pub(crate) const DIRECTORY_STREAM_FILTER: oxjvm_vm::NativeClass = class(
    "java/nio/file/DirectoryStream$Filter",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[method(
        "accept",
        "(Ljava/nio/file/Path;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    )],
    None,
);

pub(crate) const FILENAME_FILTER: oxjvm_vm::NativeClass = class(
    "java/io/FilenameFilter",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[method(
        "accept",
        "(Ljava/io/File;Ljava/lang/String;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    )],
    None,
);

// -------------------------------------------------------------------------------------------
// ByteOrder
// -------------------------------------------------------------------------------------------

fn byte_order_instance(
    vm: &mut Vm<'_>,
    class: oxjvm_vm::ClassId,
    name: &str,
) -> Result<Value, VmError> {
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "name", "Ljava/lang/String;")?;
    let text = vm.make_string(name)?;
    vm.set_instance_ref(instance, declaring, field, text);
    Ok(Value::Ref(instance))
}

fn byte_order_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/nio/ByteOrder")?;
    for name in ["BIG_ENDIAN", "LITTLE_ENDIAN"] {
        let value = byte_order_instance(vm, class, name)?;
        vm.set_static_value(class, name, "Ljava/nio/ByteOrder;", value);
    }
    Ok(())
}

const BYTE_ORDER_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "toString",
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
        "nativeOrder",
        "()Ljava/nio/ByteOrder;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/nio/ByteOrder")?;
            vm.ensure_initialized(class)?;
            let value = vm
                .read_static_ref_if_set(class, "LITTLE_ENDIAN", "Ljava/nio/ByteOrder;")
                .unwrap_or(ObjectRef::NULL);
            object(value)
        },
    ),
];

/// `java.nio.ByteOrder`: one of the two singleton constants.
pub(crate) const BYTE_ORDER: oxjvm_vm::NativeClass = class(
    "java/nio/ByteOrder",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "BIG_ENDIAN",
            "Ljava/nio/ByteOrder;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "LITTLE_ENDIAN",
            "Ljava/nio/ByteOrder;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &BYTE_ORDER_METHODS,
    Some(byte_order_clinit),
);
