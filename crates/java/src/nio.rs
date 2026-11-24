//! A minimal `java.nio.file` over the host [`FileSystem`] boundary: `Paths`, `Path`, `Files` and
//! the option/attribute types the tests link against.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_platform::FileKind;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::module::{enum_class, enum_constant, field_string};
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

const PATH_METHODS: [oxjvm_vm::NativeMethodDef; 31] = [
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
        "compareTo",
        "(Ljava/nio/file/Path;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = path_string(vm, receiver(args));
            let other = path_string(vm, ref_arg(args, 1));
            int(this.cmp(&other) as i32)
        },
    ),
    method(
        "compareTo",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC | ACC_BRIDGE | ACC_SYNTHETIC,
        |vm, _, args| {
            let this = path_string(vm, receiver(args));
            let other = path_string(vm, ref_arg(args, 1));
            int(this.cmp(&other) as i32)
        },
    ),
    method(
        "getFileName",
        "()Ljava/nio/file/Path;",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = path_string(vm, receiver(args));
            if path.is_empty() {
                return new_path(vm, "");
            }
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
        if path.is_empty() {
            return int(1);
        }
        int(path_segments(&path).len() as i32)
    }),
    method(
        "getName",
        "(I)Ljava/nio/file/Path;",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = path_string(vm, receiver(args));
            if path.is_empty() && args[1].as_int() == 0 {
                return new_path(vm, "");
            }
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
    method(
        "getFileSystem",
        "()Ljava/nio/file/FileSystem;",
        ACC_PUBLIC,
        |vm, _, _| object(new_file_system(vm)?),
    ),
    method(
        "resolve",
        "(Ljava/lang/String;[Ljava/lang/String;)Ljava/nio/file/Path;",
        ACC_PUBLIC,
        |vm, _, args| {
            let base = path_string(vm, receiver(args));
            let first = string_arg(vm, args[1])?;
            let mut path = join_path(&base, &first);
            if let Some(oxjvm_vm::ObjectData::Array(oxjvm_vm::ArrayData::Reference(elements))) = vm
                .heap
                .get(ref_arg(args, 2))
                .map(|object| object.data.clone())
            {
                for element in elements {
                    if !element.is_null() {
                        path = join_path(&path, &vm.string_or_empty(element));
                    }
                }
            }
            new_path(vm, &path)
        },
    ),
    method(
        "endsWith",
        "(Ljava/lang/String;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = path_string(vm, receiver(args));
            let suffix = string_arg(vm, args[1])?;
            boolean(
                path.trim_end_matches('/')
                    .ends_with(suffix.trim_end_matches('/')),
            )
        },
    ),
    method(
        "subpath",
        "(II)Ljava/nio/file/Path;",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = path_string(vm, receiver(args));
            let begin = crate::int_arg(args, 1);
            let end = crate::int_arg(args, 2);
            // The JDK treats a path as a sequence of name elements and hands out a range of
            // them; a range that is out of bounds is an IndexOutOfBoundsException.
            let elements: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
            if begin < 0 || end > elements.len() as i32 || begin > end {
                return Err(vm.throw_new(
                    "java/lang/IndexOutOfBoundsException",
                    Some(&alloc::format!(
                        "beginIndex={begin}, endIndex={end}, path length={}",
                        elements.len()
                    )),
                ));
            }
            let slice = elements[begin as usize..end as usize].join("/");
            new_path(vm, &slice)
        },
    ),
    method(
        "resolveSibling",
        "(Ljava/lang/String;)Ljava/nio/file/Path;",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = string_arg(vm, args[1])?;
            if other.starts_with('/') {
                return new_path(vm, &other);
            }
            // A sibling is resolved against the parent of this path.
            let path = path_string(vm, receiver(args));
            let parent = match path.rfind('/') {
                Some(index) => &path[..index],
                None => "",
            };
            new_path(vm, &join_path(parent, &other))
        },
    ),
    method(
        "toRealPath",
        "([Ljava/nio/file/LinkOption;)Ljava/nio/file/Path;",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = path_string(vm, receiver(args));
            let real = vm
                .host
                .filesystem()
                .and_then(|fs| fs.canonicalize(&path))
                .unwrap_or_else(|| crate::file::absolute_of_path(vm, &path));
            new_path(vm, &real)
        },
    ),
    method(
        "startsWith",
        "(Ljava/lang/String;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = path_string(vm, receiver(args));
            let other = vm.string_or_empty(ref_arg(args, 1));
            boolean(path.starts_with(&other))
        },
    ),
    method(
        "endsWith",
        "(Ljava/lang/String;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = path_string(vm, receiver(args));
            let other = vm.string_or_empty(ref_arg(args, 1));
            boolean(path.ends_with(&other))
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

const PATH_INTERFACE_METHODS: [oxjvm_vm::NativeMethodDef; 28] = [
    method(
        "of",
        "(Ljava/lang/String;[Ljava/lang/String;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| path_of(vm, args),
    ),
    method(
        "of",
        "(Ljava/net/URI;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let uri = ref_arg(args, 0);
            let (class, method) =
                vm.resolve_virtual_method(uri, "toString", "()Ljava/lang/String;")?;
            let text = vm.invoke_method(class, method, vec![Value::Ref(uri)])?;
            let text = vm.string_or_empty(text.as_ref());
            let path = text
                .strip_prefix("file:")
                .unwrap_or(&text)
                .trim_start_matches("//")
                .to_string();
            let path = path.trim_end_matches('/');
            let path = if path.is_empty() { "/" } else { path };
            new_path(vm, path)
        },
    ),
    method(
        "compareTo",
        "(Ljava/nio/file/Path;)I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
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
        "resolveSibling",
        "(Ljava/lang/String;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "subpath",
        "(II)Ljava/nio/file/Path;",
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
        "startsWith",
        "(Ljava/lang/String;)Z",
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
    method(
        "getFileSystem",
        "()Ljava/nio/file/FileSystem;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "resolve",
        "(Ljava/lang/String;[Ljava/lang/String;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "endsWith",
        "(Ljava/lang/String;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "toRealPath",
        "([Ljava/nio/file/LinkOption;)Ljava/nio/file/Path;",
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

const PATHS_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "get",
        "(Ljava/lang/String;[Ljava/lang/String;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| path_of(vm, args),
    ),
    method(
        "get",
        "(Ljava/net/URI;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let uri = ref_arg(args, 0);
            let (class, method) =
                vm.resolve_virtual_method(uri, "toString", "()Ljava/lang/String;")?;
            let text = vm.invoke_method(class, method, vec![Value::Ref(uri)])?;
            let text = vm.string_or_empty(text.as_ref());
            let path = text
                .strip_prefix("file:")
                .unwrap_or(&text)
                .trim_start_matches("//")
                .to_string();
            let path = path.trim_end_matches('/');
            let path = if path.is_empty() { "/" } else { path };
            new_path(vm, path)
        },
    ),
];

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

fn invalid_path_ctor(
    vm: &mut Vm<'_>,
    context: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let input = string_arg(vm, args[1])?;
    let reason = string_arg(vm, args[2])?;
    let index = if vm.classes.get(context.class).methods[context.method as usize].descriptor
        == "(Ljava/lang/String;Ljava/lang/String;I)V"
    {
        crate::int_arg(args, 3)
    } else {
        -1
    };
    for (name, value) in [("input", input.as_str()), ("reason", reason.as_str())] {
        let text = vm.make_string(value)?;
        let (declaring, field) = vm.find_field(context.class, name, "Ljava/lang/String;")?;
        vm.set_instance_ref(this, declaring, field, text);
    }
    let (declaring, field) = vm.find_field(context.class, "index", "I")?;
    vm.set_instance_int(this, declaring, field, index);
    let message = vm.make_string(&alloc::format!("{reason}: {input}"))?;
    let (declaring, field) = vm.find_field(context.class, "detailMessage", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, message);
    vm.capture_backtrace(this);
    void()
}

/// `java.nio.file.InvalidPathException`.
pub(crate) const INVALID_PATH_EXCEPTION: oxjvm_vm::NativeClass = class(
    "java/nio/file/InvalidPathException",
    Some("java/lang/IllegalArgumentException"),
    &[],
    ACC_PUBLIC,
    &[
        field("input", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("reason", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("index", "I", ACC_PRIVATE, None),
    ],
    &[
        method(
            "<init>",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            ACC_PUBLIC,
            invalid_path_ctor,
        ),
        method(
            "<init>",
            "(Ljava/lang/String;Ljava/lang/String;I)V",
            ACC_PUBLIC,
            invalid_path_ctor,
        ),
        method(
            "getInput",
            "()Ljava/lang/String;",
            ACC_PUBLIC,
            |vm, _, args| {
                let this = receiver(args);
                object(
                    vm.read_ref_field(this, vm.class_of(this), "input", "Ljava/lang/String;")
                        .unwrap_or(ObjectRef::NULL),
                )
            },
        ),
        method(
            "getReason",
            "()Ljava/lang/String;",
            ACC_PUBLIC,
            |vm, _, args| {
                let this = receiver(args);
                object(
                    vm.read_ref_field(this, vm.class_of(this), "reason", "Ljava/lang/String;")
                        .unwrap_or(ObjectRef::NULL),
                )
            },
        ),
        method("getIndex", "()I", ACC_PUBLIC, |vm, _, args| {
            int(crate::lang::read_int_field(vm, receiver(args), "index"))
        }),
    ],
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

// -------------------------------------------------------------------------------------------
// POSIX file permissions
// -------------------------------------------------------------------------------------------

/// The nine POSIX permissions, in `rwxrwxrwx` order, with their mode bits.
const POSIX_PERMISSIONS: [(&str, u32); 9] = [
    ("OWNER_READ", 0o400),
    ("OWNER_WRITE", 0o200),
    ("OWNER_EXECUTE", 0o100),
    ("GROUP_READ", 0o040),
    ("GROUP_WRITE", 0o020),
    ("GROUP_EXECUTE", 0o010),
    ("OTHERS_READ", 0o004),
    ("OTHERS_WRITE", 0o002),
    ("OTHERS_EXECUTE", 0o001),
];

fn posix_permission(vm: &mut Vm<'_>, name: &str) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/nio/file/attribute/PosixFilePermission")?;
    vm.ensure_initialized(class)?;
    Ok(vm
        .read_static_ref_if_set(class, name, "Ljava/nio/file/attribute/PosixFilePermission;")
        .unwrap_or(ObjectRef::NULL))
}

fn posix_file_permission_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/nio/file/attribute/PosixFilePermission")?;
    for (ordinal, (name, _)) in POSIX_PERMISSIONS.iter().enumerate() {
        let instance = vm.new_instance(class)?;
        let (declaring, field) = vm.find_field(class, "name", "Ljava/lang/String;")?;
        let text = vm.make_string(name)?;
        vm.set_instance_ref(instance, declaring, field, text);
        crate::lang::write_int_field(vm, instance, "ordinal", ordinal as i32);
        vm.set_static_value(
            class,
            name,
            "Ljava/nio/file/attribute/PosixFilePermission;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

const POSIX_FILE_PERMISSION_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let name = vm
            .read_ref_field(this, vm.class_of(this), "name", "Ljava/lang/String;")
            .unwrap_or(ObjectRef::NULL);
        object(name)
    }),
    method("ordinal", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "ordinal"))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name = vm
                .read_ref_field(this, vm.class_of(this), "name", "Ljava/lang/String;")
                .unwrap_or(ObjectRef::NULL);
            object(name)
        },
    ),
    method(
        "values",
        "()[Ljava/nio/file/attribute/PosixFilePermission;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/nio/file/attribute/PosixFilePermission")?;
            vm.ensure_initialized(class)?;
            let array = vm.allocate_object_array(class, POSIX_PERMISSIONS.len())?;
            for (index, (name, _)) in POSIX_PERMISSIONS.iter().enumerate() {
                let permission = posix_permission(vm, name)?;
                vm.array_set_ref(array, index, permission)?;
            }
            object(array)
        },
    ),
    method(
        "valueOf",
        "(Ljava/lang/String;)Ljava/nio/file/attribute/PosixFilePermission;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = vm.string_or_empty(ref_arg(args, 0));
            for (candidate, _) in POSIX_PERMISSIONS.iter() {
                if name == *candidate {
                    return object(posix_permission(vm, candidate)?);
                }
            }
            Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some(&alloc::format!(
                    "No enum constant java.nio.file.attribute.PosixFilePermission.{name}"
                )),
            ))
        },
    ),
];

// -------------------------------------------------------------------------------------------
// java.nio.file.attribute.AclEntry / AclEntry$Builder / AclEntryType / AclEntryFlag /
// AclEntryPermission / UserPrincipal
// -------------------------------------------------------------------------------------------

enum_class!(
    ACL_ENTRY_TYPE,
    ACL_ENTRY_TYPE_METHODS,
    acl_entry_type_clinit,
    "java/nio/file/attribute/AclEntryType",
    ["ALLOW", "DENY", "AUDIT", "ALARM"]
);

enum_class!(
    ACL_ENTRY_FLAG,
    ACL_ENTRY_FLAG_METHODS,
    acl_entry_flag_clinit,
    "java/nio/file/attribute/AclEntryFlag",
    [
        "FILE_INHERIT",
        "DIRECTORY_INHERIT",
        "NO_PROPAGATE_INHERIT",
        "INHERIT_ONLY"
    ]
);

enum_class!(
    ACL_ENTRY_PERMISSION,
    ACL_ENTRY_PERMISSION_METHODS,
    acl_entry_permission_clinit,
    "java/nio/file/attribute/AclEntryPermission",
    [
        "READ_DATA",
        "WRITE_DATA",
        "APPEND_DATA",
        "READ_NAMED_ATTRS",
        "WRITE_NAMED_ATTRS",
        "EXECUTE",
        "DELETE_CHILD",
        "READ_ATTRIBUTES",
        "WRITE_ATTRIBUTES",
        "DELETE",
        "READ_ACL",
        "WRITE_ACL",
        "WRITE_OWNER",
        "SYNCHRONIZE"
    ]
);

pub(crate) const USER_PRINCIPAL: oxjvm_vm::NativeClass = class(
    "java/nio/file/attribute/UserPrincipal",
    Some("java/lang/Object"),
    &["java/security/Principal"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

fn acl_field(vm: &Vm<'_>, object_ref: ObjectRef, name: &str, descriptor: &str) -> ObjectRef {
    vm.read_ref_field(object_ref, vm.class_of(object_ref), name, descriptor)
        .unwrap_or(ObjectRef::NULL)
}

fn acl_set_field(
    vm: &mut Vm<'_>,
    object_ref: ObjectRef,
    name: &str,
    descriptor: &str,
    value: ObjectRef,
) -> Result<(), VmError> {
    let (declaring, field) = vm.find_field(vm.class_of(object_ref), name, descriptor)?;
    vm.set_instance_ref(object_ref, declaring, field, value);
    Ok(())
}

fn acl_empty_set(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/Collections")?;
    let (declaring, method) = vm
        .find_method(class, "emptySet", "()Ljava/util/Set;")
        .ok_or_else(|| VmError::internal("Collections.emptySet missing"))?;
    Ok(vm.invoke_method(declaring, method, vec![])?.as_ref())
}

/// Copy a permission/flag set the way `AclEntry.Builder` does: an empty set
/// becomes the shared empty set, anything else is copied into an `EnumSet`
/// and every element is checked against the expected enum type.
fn acl_copy_enum_set(
    vm: &mut Vm<'_>,
    set: ObjectRef,
    enum_name: &str,
) -> Result<ObjectRef, VmError> {
    if set.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let (class, method) = vm.resolve_virtual_method(set, "isEmpty", "()Z")?;
    let empty = vm
        .invoke_method(class, method, vec![Value::Ref(set)])?
        .as_int()
        != 0;
    if empty {
        return acl_empty_set(vm);
    }
    let enum_set_class = vm.resolve_class("java/util/EnumSet")?;
    let (declaring, copy_of) = vm
        .find_method(
            enum_set_class,
            "copyOf",
            "(Ljava/util/Collection;)Ljava/util/EnumSet;",
        )
        .ok_or_else(|| VmError::internal("EnumSet.copyOf missing"))?;
    let copy = vm
        .invoke_method(declaring, copy_of, vec![Value::Ref(set)])?
        .as_ref();
    let enum_class = vm.resolve_class(enum_name)?;
    for element in crate::util::collection_elements(vm, copy)? {
        if element.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        if !vm.is_instance(element, enum_class) {
            return Err(vm.throw_new("java/lang/ClassCastException", None));
        }
    }
    Ok(copy)
}

/// Build an `EnumSet` from a varargs array, rejecting null elements.
fn acl_enum_set_from_array(
    vm: &mut Vm<'_>,
    array: ObjectRef,
    enum_name: &str,
) -> Result<ObjectRef, VmError> {
    if array.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let enum_class = vm.resolve_class(enum_name)?;
    let mirror = vm.class_object(enum_class)?;
    let enum_set_class = vm.resolve_class("java/util/EnumSet")?;
    let (declaring, none_of) = vm
        .find_method(
            enum_set_class,
            "noneOf",
            "(Ljava/lang/Class;)Ljava/util/EnumSet;",
        )
        .ok_or_else(|| VmError::internal("EnumSet.noneOf missing"))?;
    let set = vm
        .invoke_method(declaring, none_of, vec![Value::Ref(mirror)])?
        .as_ref();
    let (add_class, add) = vm.resolve_virtual_method(set, "add", "(Ljava/lang/Object;)Z")?;
    let length = vm.array_length(array)?;
    for index in 0..length {
        let element = vm.array_get(array, index as i32)?.as_ref();
        if element.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        vm.invoke_method(add_class, add, vec![Value::Ref(set), Value::Ref(element)])?;
    }
    Ok(set)
}

fn acl_builder_new(
    vm: &mut Vm<'_>,
    entry_type: ObjectRef,
    who: ObjectRef,
    perms: ObjectRef,
    flags: ObjectRef,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/nio/file/attribute/AclEntry$Builder")?;
    let builder = vm.new_instance(class)?;
    acl_set_field(
        vm,
        builder,
        "type",
        "Ljava/nio/file/attribute/AclEntryType;",
        entry_type,
    )?;
    acl_set_field(
        vm,
        builder,
        "who",
        "Ljava/nio/file/attribute/UserPrincipal;",
        who,
    )?;
    acl_set_field(vm, builder, "perms", "Ljava/util/Set;", perms)?;
    acl_set_field(vm, builder, "flags", "Ljava/util/Set;", flags)?;
    Ok(builder)
}

const ACL_ENTRY_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method(
        "newBuilder",
        "()Ljava/nio/file/attribute/AclEntry$Builder;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let empty = acl_empty_set(vm)?;
            object(acl_builder_new(
                vm,
                ObjectRef::NULL,
                ObjectRef::NULL,
                empty,
                empty,
            )?)
        },
    ),
    method(
        "newBuilder",
        "(Ljava/nio/file/attribute/AclEntry;)Ljava/nio/file/attribute/AclEntry$Builder;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let entry = ref_arg(args, 0);
            if entry.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let entry_type = acl_field(vm, entry, "type", "Ljava/nio/file/attribute/AclEntryType;");
            let who = acl_field(vm, entry, "who", "Ljava/nio/file/attribute/UserPrincipal;");
            let perms = acl_field(vm, entry, "perms", "Ljava/util/Set;");
            let flags = acl_field(vm, entry, "flags", "Ljava/util/Set;");
            object(acl_builder_new(vm, entry_type, who, perms, flags)?)
        },
    ),
    method(
        "type",
        "()Ljava/nio/file/attribute/AclEntryType;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(acl_field(
                vm,
                receiver(args),
                "type",
                "Ljava/nio/file/attribute/AclEntryType;",
            ))
        },
    ),
    method(
        "principal",
        "()Ljava/nio/file/attribute/UserPrincipal;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(acl_field(
                vm,
                receiver(args),
                "who",
                "Ljava/nio/file/attribute/UserPrincipal;",
            ))
        },
    ),
    method(
        "permissions",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, args| object(acl_field(vm, receiver(args), "perms", "Ljava/util/Set;")),
    ),
    method("flags", "()Ljava/util/Set;", ACC_PUBLIC, |vm, _, args| {
        object(acl_field(vm, receiver(args), "flags", "Ljava/util/Set;"))
    }),
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
            let acl_class = vm.resolve_class("java/nio/file/attribute/AclEntry")?;
            if other.is_null() || !vm.is_instance(other, acl_class) {
                return boolean(false);
            }
            let this_type = acl_field(vm, this, "type", "Ljava/nio/file/attribute/AclEntryType;");
            let other_type = acl_field(vm, other, "type", "Ljava/nio/file/attribute/AclEntryType;");
            if this_type != other_type {
                return boolean(false);
            }
            for (name, descriptor) in [
                ("who", "Ljava/nio/file/attribute/UserPrincipal;"),
                ("perms", "Ljava/util/Set;"),
                ("flags", "Ljava/util/Set;"),
            ] {
                let left = acl_field(vm, this, name, descriptor);
                let right = acl_field(vm, other, name, descriptor);
                let (class, method) =
                    vm.resolve_virtual_method(left, "equals", "(Ljava/lang/Object;)Z")?;
                let equal = vm
                    .invoke_method(class, method, vec![Value::Ref(left), Value::Ref(right)])?
                    .as_int()
                    != 0;
                if !equal {
                    return boolean(false);
                }
            }
            boolean(true)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let object_class = vm.resolve_class("java/lang/Object")?;
        let array = vm.allocate_object_array(object_class, 4)?;
        vm.array_set_ref(
            array,
            0,
            acl_field(vm, this, "type", "Ljava/nio/file/attribute/AclEntryType;"),
        )?;
        vm.array_set_ref(
            array,
            1,
            acl_field(vm, this, "who", "Ljava/nio/file/attribute/UserPrincipal;"),
        )?;
        vm.array_set_ref(array, 2, acl_field(vm, this, "perms", "Ljava/util/Set;"))?;
        vm.array_set_ref(array, 3, acl_field(vm, this, "flags", "Ljava/util/Set;"))?;
        let objects_class = vm.resolve_class("java/util/Objects")?;
        let (declaring, hash) = vm
            .find_method(objects_class, "hash", "([Ljava/lang/Object;)I")
            .ok_or_else(|| VmError::internal("Objects.hash missing"))?;
        let value = vm
            .invoke_method(declaring, hash, vec![Value::Ref(array)])?
            .as_int();
        int(value)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let who = acl_field(vm, this, "who", "Ljava/nio/file/attribute/UserPrincipal;");
            let (class, method) =
                vm.resolve_virtual_method(who, "getName", "()Ljava/lang/String;")?;
            let name = vm
                .invoke_method(class, method, vec![Value::Ref(who)])?
                .as_ref();
            let mut text = vm.string_or_empty(name);
            text.push(':');
            let perms = acl_field(vm, this, "perms", "Ljava/util/Set;");
            text.push_str(&acl_enum_names(vm, perms)?);
            text.push(':');
            let flags = acl_field(vm, this, "flags", "Ljava/util/Set;");
            let (flag_class, is_empty) = vm.resolve_virtual_method(flags, "isEmpty", "()Z")?;
            let flags_empty = vm
                .invoke_method(flag_class, is_empty, vec![Value::Ref(flags)])?
                .as_int()
                != 0;
            if !flags_empty {
                text.push_str(&acl_enum_names(vm, flags)?);
                text.push(':');
            }
            let entry_type = acl_field(vm, this, "type", "Ljava/nio/file/attribute/AclEntryType;");
            let (type_class, type_name) =
                vm.resolve_virtual_method(entry_type, "name", "()Ljava/lang/String;")?;
            let name = vm
                .invoke_method(type_class, type_name, vec![Value::Ref(entry_type)])?
                .as_ref();
            text.push_str(&vm.string_or_empty(name));
            object(vm.make_string(&text)?)
        },
    ),
];

/// Join the `name()` of every enum constant in a set with `/`.
fn acl_enum_names(vm: &mut Vm<'_>, set: ObjectRef) -> Result<String, VmError> {
    let mut names = Vec::new();
    for element in crate::util::collection_elements(vm, set)? {
        let (class, method) = vm.resolve_virtual_method(element, "name", "()Ljava/lang/String;")?;
        let name = vm
            .invoke_method(class, method, vec![Value::Ref(element)])?
            .as_ref();
        names.push(vm.string_or_empty(name));
    }
    Ok(names.join("/"))
}

pub(crate) const ACL_ENTRY: oxjvm_vm::NativeClass = class(
    "java/nio/file/attribute/AclEntry",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field(
            "type",
            "Ljava/nio/file/attribute/AclEntryType;",
            ACC_PRIVATE,
            None,
        ),
        field(
            "who",
            "Ljava/nio/file/attribute/UserPrincipal;",
            ACC_PRIVATE,
            None,
        ),
        field("perms", "Ljava/util/Set;", ACC_PRIVATE, None),
        field("flags", "Ljava/util/Set;", ACC_PRIVATE, None),
    ],
    &ACL_ENTRY_METHODS,
    None,
);

const ACL_ENTRY_BUILDER_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "setType",
        "(Ljava/nio/file/attribute/AclEntryType;)Ljava/nio/file/attribute/AclEntry$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let entry_type = ref_arg(args, 1);
            if entry_type.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            acl_set_field(
                vm,
                this,
                "type",
                "Ljava/nio/file/attribute/AclEntryType;",
                entry_type,
            )?;
            object(this)
        },
    ),
    method(
        "setPrincipal",
        "(Ljava/nio/file/attribute/UserPrincipal;)Ljava/nio/file/attribute/AclEntry$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let who = ref_arg(args, 1);
            if who.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            acl_set_field(
                vm,
                this,
                "who",
                "Ljava/nio/file/attribute/UserPrincipal;",
                who,
            )?;
            object(this)
        },
    ),
    method(
        "setPermissions",
        "(Ljava/util/Set;)Ljava/nio/file/attribute/AclEntry$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let perms = acl_copy_enum_set(
                vm,
                ref_arg(args, 1),
                "java/nio/file/attribute/AclEntryPermission",
            )?;
            acl_set_field(vm, this, "perms", "Ljava/util/Set;", perms)?;
            object(this)
        },
    ),
    method(
        "setPermissions",
        "([Ljava/nio/file/attribute/AclEntryPermission;)Ljava/nio/file/attribute/AclEntry$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let perms = acl_enum_set_from_array(
                vm,
                ref_arg(args, 1),
                "java/nio/file/attribute/AclEntryPermission",
            )?;
            acl_set_field(vm, this, "perms", "Ljava/util/Set;", perms)?;
            object(this)
        },
    ),
    method(
        "setFlags",
        "(Ljava/util/Set;)Ljava/nio/file/attribute/AclEntry$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let flags =
                acl_copy_enum_set(vm, ref_arg(args, 1), "java/nio/file/attribute/AclEntryFlag")?;
            acl_set_field(vm, this, "flags", "Ljava/util/Set;", flags)?;
            object(this)
        },
    ),
    method(
        "setFlags",
        "([Ljava/nio/file/attribute/AclEntryFlag;)Ljava/nio/file/attribute/AclEntry$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let flags = acl_enum_set_from_array(
                vm,
                ref_arg(args, 1),
                "java/nio/file/attribute/AclEntryFlag",
            )?;
            acl_set_field(vm, this, "flags", "Ljava/util/Set;", flags)?;
            object(this)
        },
    ),
    method(
        "build",
        "()Ljava/nio/file/attribute/AclEntry;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let entry_type = acl_field(vm, this, "type", "Ljava/nio/file/attribute/AclEntryType;");
            if entry_type.is_null() {
                return Err(vm.throw_new(
                    "java/lang/IllegalStateException",
                    Some("Missing type component"),
                ));
            }
            let who = acl_field(vm, this, "who", "Ljava/nio/file/attribute/UserPrincipal;");
            if who.is_null() {
                return Err(vm.throw_new(
                    "java/lang/IllegalStateException",
                    Some("Missing who component"),
                ));
            }
            let perms = acl_field(vm, this, "perms", "Ljava/util/Set;");
            let flags = acl_field(vm, this, "flags", "Ljava/util/Set;");
            let class = vm.resolve_class("java/nio/file/attribute/AclEntry")?;
            let entry = vm.new_instance(class)?;
            acl_set_field(
                vm,
                entry,
                "type",
                "Ljava/nio/file/attribute/AclEntryType;",
                entry_type,
            )?;
            acl_set_field(
                vm,
                entry,
                "who",
                "Ljava/nio/file/attribute/UserPrincipal;",
                who,
            )?;
            acl_set_field(vm, entry, "perms", "Ljava/util/Set;", perms)?;
            acl_set_field(vm, entry, "flags", "Ljava/util/Set;", flags)?;
            object(entry)
        },
    ),
];

pub(crate) const ACL_ENTRY_BUILDER: oxjvm_vm::NativeClass = class(
    "java/nio/file/attribute/AclEntry$Builder",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field(
            "type",
            "Ljava/nio/file/attribute/AclEntryType;",
            ACC_PRIVATE,
            None,
        ),
        field(
            "who",
            "Ljava/nio/file/attribute/UserPrincipal;",
            ACC_PRIVATE,
            None,
        ),
        field("perms", "Ljava/util/Set;", ACC_PRIVATE, None),
        field("flags", "Ljava/util/Set;", ACC_PRIVATE, None),
    ],
    &ACL_ENTRY_BUILDER_METHODS,
    None,
);

pub(crate) const POSIX_FILE_PERMISSION: oxjvm_vm::NativeClass = class(
    "java/nio/file/attribute/PosixFilePermission",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_ENUM,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("ordinal", "I", ACC_PRIVATE, None),
        field(
            "OWNER_READ",
            "Ljava/nio/file/attribute/PosixFilePermission;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "OWNER_WRITE",
            "Ljava/nio/file/attribute/PosixFilePermission;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "OWNER_EXECUTE",
            "Ljava/nio/file/attribute/PosixFilePermission;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "GROUP_READ",
            "Ljava/nio/file/attribute/PosixFilePermission;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "GROUP_WRITE",
            "Ljava/nio/file/attribute/PosixFilePermission;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "GROUP_EXECUTE",
            "Ljava/nio/file/attribute/PosixFilePermission;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "OTHERS_READ",
            "Ljava/nio/file/attribute/PosixFilePermission;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "OTHERS_WRITE",
            "Ljava/nio/file/attribute/PosixFilePermission;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "OTHERS_EXECUTE",
            "Ljava/nio/file/attribute/PosixFilePermission;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &POSIX_FILE_PERMISSION_METHODS,
    Some(posix_file_permission_clinit),
);

/// Whether `set` contains the named permission singleton.
fn posix_set_contains(vm: &mut Vm<'_>, set: ObjectRef, name: &str) -> Result<bool, VmError> {
    let permission = posix_permission(vm, name)?;
    let (class, method) = vm.resolve_virtual_method(set, "contains", "(Ljava/lang/Object;)Z")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(set), Value::Ref(permission)])?
        .as_int()
        != 0)
}

const POSIX_FILE_PERMISSIONS_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "fromString",
        "(Ljava/lang/String;)Ljava/util/Set;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = string_arg(vm, args[0])?;
            let chars: Vec<char> = text.chars().collect();
            if chars.len() != 9 {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("Invalid permissions string"),
                ));
            }
            let mut elements = Vec::new();
            for (index, (name, _)) in POSIX_PERMISSIONS.iter().enumerate() {
                let expected = "rwxrwxrwx".chars().nth(index).unwrap();
                if chars[index] == expected {
                    elements.push(posix_permission(vm, name)?);
                } else if chars[index] != '-' {
                    return Err(vm.throw_new(
                        "java/lang/IllegalArgumentException",
                        Some("Invalid permissions string"),
                    ));
                }
            }
            object(crate::util::hash_set_from(vm, &elements)?)
        },
    ),
    method(
        "toString",
        "(Ljava/util/Set;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let set = ref_arg(args, 0);
            let mut text = String::new();
            for (index, (name, _)) in POSIX_PERMISSIONS.iter().enumerate() {
                let expected = "rwxrwxrwx".chars().nth(index).unwrap();
                if posix_set_contains(vm, set, name)? {
                    text.push(expected);
                } else {
                    text.push('-');
                }
            }
            object(vm.make_string(&text)?)
        },
    ),
];

pub(crate) const POSIX_FILE_PERMISSIONS: oxjvm_vm::NativeClass = class(
    "java/nio/file/attribute/PosixFilePermissions",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &POSIX_FILE_PERMISSIONS_METHODS,
    None,
);

const FILES_METHODS: [oxjvm_vm::NativeMethodDef; 66] = [
    method(
        "getPosixFilePermissions",
        "(Ljava/nio/file/Path;[Ljava/nio/file/LinkOption;)Ljava/util/Set;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let Some(metadata) = fs_metadata(vm, &path) else {
                return Err(vm.throw_new("java/nio/file/NoSuchFileException", Some(&path)));
            };
            let mut elements = Vec::new();
            for (name, bit) in POSIX_PERMISSIONS.iter() {
                if metadata.mode & bit != 0 {
                    elements.push(posix_permission(vm, name)?);
                }
            }
            object(crate::util::hash_set_from(vm, &elements)?)
        },
    ),
    method(
        "setPosixFilePermissions",
        "(Ljava/nio/file/Path;Ljava/util/Set;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path_ref = ref_arg(args, 0);
            let path = path_string(vm, path_ref);
            let set = ref_arg(args, 1);
            let mut mode = 0u32;
            for (name, bit) in POSIX_PERMISSIONS.iter() {
                if posix_set_contains(vm, set, name)? {
                    mode |= bit;
                }
            }
            let ok = vm
                .host
                .filesystem()
                .map(|fs| fs.set_mode(&path, mode))
                .unwrap_or(false);
            if !ok {
                return Err(vm.throw_new("java/lang/UnsupportedOperationException", None));
            }
            object(path_ref)
        },
    ),
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
        "copy",
        "(Ljava/nio/file/Path;Ljava/io/OutputStream;)J",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| files_copy_to_stream(vm, args),
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
        "writeString",
        "(Ljava/nio/file/Path;Ljava/lang/CharSequence;Ljava/nio/charset/Charset;[Ljava/nio/file/OpenOption;)Ljava/nio/file/Path;",
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
            let prefix = vm.string_or_empty(ref_arg(args, 1));
            let suffix = if ref_arg(args, 2).is_null() {
                String::from(".tmp")
            } else {
                vm.string_or_empty(ref_arg(args, 2))
            };
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
            let prefix = vm.string_or_empty(ref_arg(args, 0));
            let suffix = if ref_arg(args, 1).is_null() {
                String::from(".tmp")
            } else {
                vm.string_or_empty(ref_arg(args, 1))
            };
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
        "(Ljava/nio/file/Path;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            // Without a charset the JDK reads the file as UTF-8.
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
        "newBufferedReader",
        "(Ljava/nio/file/Path;)Ljava/io/BufferedReader;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| files_new_buffered_reader(vm, args),
    ),
    method(
        "mismatch",
        "(Ljava/nio/file/Path;Ljava/nio/file/Path;)J",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let first = path_string(vm, ref_arg(args, 0));
            let second = path_string(vm, ref_arg(args, 1));
            let left = vm
                .host
                .filesystem()
                .and_then(|fs| fs.read(&first))
                .unwrap_or_default();
            let right = vm
                .host
                .filesystem()
                .and_then(|fs| fs.read(&second))
                .unwrap_or_default();
            let mut offset = 0usize;
            while offset < left.len() && offset < right.len() && left[offset] == right[offset] {
                offset += 1;
            }
            if offset < left.len() || offset < right.len() {
                long(offset as i64)
            } else {
                long(-1)
            }
        },
    ),
    method(
        "isSameFile",
        "(Ljava/nio/file/Path;Ljava/nio/file/Path;)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let first = path_string(vm, ref_arg(args, 0));
            let second = path_string(vm, ref_arg(args, 1));
            boolean(first == second)
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
    method(
        "createSymbolicLink",
        "(Ljava/nio/file/Path;Ljava/nio/file/Path;[Ljava/nio/file/attribute/FileAttribute;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let link = path_string(vm, ref_arg(args, 0));
            let target = path_string(vm, ref_arg(args, 1));
            if !vm
                .host
                .filesystem()
                .map(|fs| fs.symlink(&link, &target))
                .unwrap_or(false)
            {
                return Err(vm.throw_new("java/io/IOException", Some(&link)));
            }
            object(ref_arg(args, 0))
        },
    ),
    method(
        "readSymbolicLink",
        "(Ljava/nio/file/Path;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let link = path_string(vm, ref_arg(args, 0));
            let Some(target) = vm.host.filesystem().and_then(|fs| fs.read_link(&link)) else {
                return Err(vm.throw_new("java/io/IOException", Some(&link)));
            };
            new_path(vm, &target)
        },
    ),
    method(
        "createLink",
        "(Ljava/nio/file/Path;Ljava/nio/file/Path;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let link = path_string(vm, ref_arg(args, 0));
            let target = path_string(vm, ref_arg(args, 1));
            if !vm
                .host
                .filesystem()
                .map(|fs| fs.hard_link(&link, &target))
                .unwrap_or(false)
            {
                return Err(vm.throw_new("java/io/IOException", Some(&link)));
            }
            object(ref_arg(args, 0))
        },
    ),
    method(
        "isSymbolicLink",
        "(Ljava/nio/file/Path;)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            boolean(
                vm.host
                    .filesystem()
                    .and_then(|fs| fs.read_link(&path))
                    .is_some(),
            )
        },
    ),
    method(
        "isWritable",
        "(Ljava/nio/file/Path;)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            boolean(fs_metadata(vm, &path).is_some_and(|metadata| !metadata.readonly))
        },
    ),
    method(
        "isReadable",
        "(Ljava/nio/file/Path;)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            boolean(fs_metadata(vm, &path).is_some())
        },
    ),
    method(
        "isExecutable",
        "(Ljava/nio/file/Path;)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            boolean(
                fs_metadata(vm, &path)
                    .is_some_and(|metadata| metadata.kind == FileKind::File && !metadata.readonly),
            )
        },
    ),
    method(
        "size",
        "(Ljava/nio/file/Path;)J",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let Some(metadata) = fs_metadata(vm, &path) else {
                return Err(vm.throw_new("java/io/IOException", Some(&path)));
            };
            long(metadata.len as i64)
        },
    ),
    method(
        "probeContentType",
        "(Ljava/nio/file/Path;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let content = match path.rsplit('.').next().unwrap_or("") {
                "txt" | "java" | "properties" => "text/plain",
                "html" | "htm" => "text/html",
                "xml" => "application/xml",
                "json" => "application/json",
                "class" => "application/java-vm",
                "jar" | "zip" => "application/zip",
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "pdf" => "application/pdf",
                _ => return object(ObjectRef::NULL),
            };
            object(vm.make_string(content)?)
        },
    ),
    method(
        "getFileStore",
        "(Ljava/nio/file/Path;)Ljava/nio/file/FileStore;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_file_store(vm)?),
    ),
    method(
        "walkFileTree",
        "(Ljava/nio/file/Path;Ljava/nio/file/FileVisitor;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let start = ref_arg(args, 0);
            let path = path_string(vm, start);
            let visitor = ref_arg(args, 1);
            walk_file_tree(vm, &path, start, visitor, 0, usize::MAX)?;
            object(start)
        },
    ),
    method(
        "walkFileTree",
        "(Ljava/nio/file/Path;Ljava/nio/file/FileVisitor;[Ljava/nio/file/FileVisitOption;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let start = ref_arg(args, 0);
            let path = path_string(vm, start);
            let visitor = ref_arg(args, 1);
            walk_file_tree(vm, &path, start, visitor, 0, usize::MAX)?;
            object(start)
        },
    ),
    method(
        "walkFileTree",
        "(Ljava/nio/file/Path;Ljava/util/Set;ILjava/nio/file/FileVisitor;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let start = ref_arg(args, 0);
            if start.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let path = path_string(vm, start);
            let options = ref_arg(args, 1);
            if options.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            for option in crate::util::collection_elements(vm, options)? {
                if option.is_null() {
                    return Err(vm.throw_new("java/lang/NullPointerException", None));
                }
            }
            let visitor = ref_arg(args, 3);
            if visitor.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let max_depth = args[2].as_int();
            if max_depth < 0 {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("maxDepth must not be negative"),
                ));
            }
            walk_file_tree(vm, &path, start, visitor, 0, max_depth as usize)?;
            object(start)
        },
    ),
    method(
        "newDirectoryStream",
        "(Ljava/nio/file/Path;)Ljava/nio/file/DirectoryStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            new_directory_stream(vm, &path, None)
        },
    ),
    method(
        "newDirectoryStream",
        "(Ljava/nio/file/Path;Ljava/lang/String;)Ljava/nio/file/DirectoryStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let glob = string_arg(vm, args[1])?;
            new_directory_stream(vm, &path, Some(&glob))
        },
    ),
    method(
        "newDirectoryStream",
        "(Ljava/nio/file/Path;Ljava/nio/file/DirectoryStream$Filter;)Ljava/nio/file/DirectoryStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let filter = ref_arg(args, 1);
            new_directory_stream_filtered(vm, &path, filter)
        },
    ),
    method(
        "newBufferedWriter",
        "(Ljava/nio/file/Path;Ljava/nio/charset/Charset;[Ljava/nio/file/OpenOption;)Ljava/io/BufferedWriter;",
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
        "write",
        "(Ljava/nio/file/Path;Ljava/lang/Iterable;Ljava/nio/charset/Charset;[Ljava/nio/file/OpenOption;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let mut text = String::new();
            for element in crate::util::collection_elements(vm, ref_arg(args, 1))? {
                text.push_str(&text_of(vm, element)?);
                text.push('\n');
            }
            let charset = crate::charset::name_of(vm, ref_arg(args, 2));
            let bytes = crate::charset::encode(&text, &charset);
            let path = path_string(vm, ref_arg(args, 0));
            if !fs_write(vm, &path, &bytes, false) {
                return Err(vm.throw_new("java/io/IOException", Some(&path)));
            }
            object(ref_arg(args, 0))
        },
    ),
    method(
        "getLastModifiedTime",
        "(Ljava/nio/file/Path;[Ljava/nio/file/LinkOption;)Ljava/nio/file/attribute/FileTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let Some(metadata) = fs_metadata(vm, &path) else {
                return Err(vm.throw_new("java/io/IOException", Some(&path)));
            };
            file_time_of(vm, metadata.modified_millis)
        },
    ),
    method(
        "setLastModifiedTime",
        "(Ljava/nio/file/Path;Ljava/nio/file/attribute/FileTime;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let time = ref_arg(args, 1);
            let millis = vm
                .read_instance_long_named(time, vm.class_of(time), "millis", "J")
                .unwrap_or(0);
            if !vm
                .host
                .filesystem()
                .map(|fs| fs.set_modified(&path, millis))
                .unwrap_or(false)
            {
                return Err(vm.throw_new("java/io/IOException", Some(&path)));
            }
            object(ref_arg(args, 0))
        },
    ),
    method(
        "isHidden",
        "(Ljava/nio/file/Path;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let name = path.rsplit('/').next().unwrap_or("");
            boolean(name.starts_with('.'))
        },
    ),
    method(
        "newBufferedReader",
        "(Ljava/nio/file/Path;Ljava/nio/charset/Charset;)Ljava/io/BufferedReader;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            files_new_buffered_reader(vm, args)
        },
    ),
    method(
        "lines",
        "(Ljava/nio/file/Path;Ljava/nio/charset/Charset;)Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let path = path_string(vm, ref_arg(args, 0));
            let text = read_text_file(vm, &path)?;
            let mut lines = Vec::new();
            for line in text.lines() {
                lines.push(vm.make_string(line)?);
            }
            object(crate::stream::stream_from(vm, lines)?)
        },
    ),
    method(
        "getAttribute",
        "(Ljava/nio/file/Path;Ljava/lang/String;[Ljava/nio/file/LinkOption;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let attribute = string_arg(vm, args[1])?;
            let Some(metadata) = fs_metadata(vm, &path) else {
                return Err(vm.throw_new("java/io/IOException", Some(&path)));
            };
            let (view, name) = match attribute.split_once(':') {
                Some((view, name)) => (view, name),
                None => ("basic", attribute.as_str()),
            };
            if view != "basic" {
                return Err(
                    vm.throw_new("java/lang/UnsupportedOperationException", Some(&attribute))
                );
            }
            match name {
                "size" => crate::boxed::box_value(vm, Value::Long(metadata.len as i64), "J"),
                "isDirectory" => crate::boxed::box_value(
                    vm,
                    Value::Int(i32::from(metadata.kind == FileKind::Directory)),
                    "Z",
                ),
                "isRegularFile" => crate::boxed::box_value(
                    vm,
                    Value::Int(i32::from(metadata.kind == FileKind::File)),
                    "Z",
                ),
                "isSymbolicLink" => crate::boxed::box_value(vm, Value::Int(0), "Z"),
                "isOther" => crate::boxed::box_value(
                    vm,
                    Value::Int(i32::from(metadata.kind == FileKind::Other)),
                    "Z",
                ),
                "lastModifiedTime" | "creationTime" | "lastAccessTime" => {
                    file_time_of(vm, metadata.modified_millis)
                }
                _ => Err(vm.throw_new("java/lang/UnsupportedOperationException", Some(&attribute))),
            }
        },
    ),
    method(
        "readAttributes",
        "(Ljava/nio/file/Path;Ljava/lang/String;[Ljava/nio/file/LinkOption;)Ljava/util/Map;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let attribute = string_arg(vm, args[1])?;
            let Some(metadata) = fs_metadata(vm, &path) else {
                return Err(vm.throw_new("java/io/IOException", Some(&path)));
            };
            let (view, names) = match attribute.split_once(':') {
                Some((view, names)) => (view, names),
                None => ("basic", attribute.as_str()),
            };
            let keys: Vec<&str> = if names == "*" {
                match view {
                    "posix" | "unix" => vec![
                        "lastModifiedTime",
                        "lastAccessTime",
                        "creationTime",
                        "size",
                        "isDirectory",
                        "isRegularFile",
                        "isSymbolicLink",
                        "isOther",
                        "fileKey",
                        "permissions",
                        "group",
                        "owner",
                    ],
                    "dos" => vec![
                        "lastModifiedTime",
                        "lastAccessTime",
                        "creationTime",
                        "size",
                        "isDirectory",
                        "isRegularFile",
                        "isSymbolicLink",
                        "isOther",
                        "fileKey",
                        "readonly",
                        "hidden",
                        "system",
                        "archive",
                    ],
                    "owner" => vec!["owner"],
                    _ => vec![
                        "lastModifiedTime",
                        "lastAccessTime",
                        "creationTime",
                        "size",
                        "isDirectory",
                        "isRegularFile",
                        "isSymbolicLink",
                        "isOther",
                        "fileKey",
                    ],
                }
            } else {
                names.split(',').collect()
            };
            let map = crate::util::linked_hash_map_new(vm)?;
            for name in keys {
                let key = vm.make_string(name)?;
                let value = match name {
                    "permissions" => crate::util::hash_set_from(vm, &[])?,
                    "owner" | "group" | "fileKey" => ObjectRef::NULL,
                    "readonly" | "hidden" | "system" | "archive" => {
                        crate::boxed::box_value(vm, Value::Int(0), "Z")?.as_ref()
                    }
                    _ => basic_attribute_value(vm, &metadata, name)?.as_ref(),
                };
                crate::util::hash_map_insert(vm, map, key, value)?;
            }
            object(map)
        },
    ),
    method(
        "setAttribute",
        "(Ljava/nio/file/Path;Ljava/lang/String;Ljava/lang/Object;[Ljava/nio/file/LinkOption;)Ljava/nio/file/Path;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            let attribute = string_arg(vm, args[1])?;
            let value = ref_arg(args, 2);
            let (view, name) = match attribute.split_once(':') {
                Some((view, name)) => (view, name),
                None => ("basic", attribute.as_str()),
            };
            if view == "basic"
                && matches!(name, "lastModifiedTime" | "lastAccessTime" | "creationTime")
            {
                let millis = vm
                    .read_instance_long_named(value, vm.class_of(value), "millis", "J")
                    .unwrap_or(0);
                if !vm
                    .host
                    .filesystem()
                    .map(|fs| fs.set_modified(&path, millis))
                    .unwrap_or(false)
                {
                    return Err(vm.throw_new("java/io/IOException", Some(&path)));
                }
                return object(ref_arg(args, 0));
            }
            Err(vm.throw_new("java/lang/UnsupportedOperationException", Some(&attribute)))
        },
    ),
];

/// Boxed value of a single `basic` view attribute for `metadata`.
fn basic_attribute_value(
    vm: &mut Vm<'_>,
    metadata: &oxjvm_platform::FileMetadata,
    name: &str,
) -> Result<Value, VmError> {
    match name {
        "size" => crate::boxed::box_value(vm, Value::Long(metadata.len as i64), "J"),
        "isDirectory" => crate::boxed::box_value(
            vm,
            Value::Int(i32::from(metadata.kind == FileKind::Directory)),
            "Z",
        ),
        "isRegularFile" => crate::boxed::box_value(
            vm,
            Value::Int(i32::from(metadata.kind == FileKind::File)),
            "Z",
        ),
        "isSymbolicLink" => crate::boxed::box_value(vm, Value::Int(0), "Z"),
        "isOther" => crate::boxed::box_value(
            vm,
            Value::Int(i32::from(metadata.kind == FileKind::Other)),
            "Z",
        ),
        "lastModifiedTime" | "creationTime" | "lastAccessTime" => {
            file_time_of(vm, metadata.modified_millis)
        }
        "fileKey" => Ok(Value::Ref(ObjectRef::NULL)),
        _ => Err(vm.throw_new("java/lang/UnsupportedOperationException", Some(name))),
    }
}

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

/// `Files.newBufferedReader`: a reader over the file, decoded as UTF-8.
fn files_new_buffered_reader(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let path = path_string(vm, ref_arg(args, 0));
    let text = read_text_file(vm, &path)?;
    let reader_class = vm.resolve_class("java/io/StringReader")?;
    let reader = vm.new_instance(reader_class)?;
    let (declaring, ctor) = vm
        .find_method(reader_class, "<init>", "(Ljava/lang/String;)V")
        .ok_or_else(|| VmError::internal("StringReader lacks its constructor"))?;
    let content = vm.make_string(&text)?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![Value::Ref(reader), Value::Ref(content)],
    )?;
    let buffered_class = vm.resolve_class("java/io/BufferedReader")?;
    let buffered = vm.new_instance(buffered_class)?;
    let (declaring, ctor) = vm
        .find_method(buffered_class, "<init>", "(Ljava/io/Reader;)V")
        .ok_or_else(|| VmError::internal("BufferedReader lacks its constructor"))?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![Value::Ref(buffered), Value::Ref(reader)],
    )?;
    object(buffered)
}

/// `Files.copy(Path, OutputStream)`: the file goes to the stream, byte for byte.
///
/// The JDK reads the file in chunks and writes each of them; the number of bytes it copied is
/// the number of bytes the file holds.
fn files_copy_to_stream(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let path = path_string(vm, ref_arg(args, 0));
    let stream = ref_arg(args, 1);
    if stream.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let Some(bytes) = vm.host.filesystem().and_then(|fs| fs.read(&path)) else {
        return Err(vm.throw_new("java/io/IOException", Some(&path)));
    };
    let count = write_to_stream(vm, stream, &bytes)?;
    long(count as i64)
}

/// Write bytes to an output stream, the way the JDK's `OutputStream.write(byte[])` does.
fn write_to_stream(vm: &mut Vm<'_>, stream: ObjectRef, bytes: &[u8]) -> Result<usize, VmError> {
    let array = bytes_array(vm, bytes)?;
    let (class, write) = vm.resolve_virtual_method(stream, "write", "([B)V")?;
    vm.invoke_method(class, write, vec![Value::Ref(stream), array])?;
    Ok(bytes.len())
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

/// Depth-first `Files.walkFileTree`.
///
/// Returns 0 to continue, 1 to terminate the whole walk, or 3 to skip the parent directory's
/// remaining siblings (`FileVisitResult.SKIP_SIBLINGS`).
fn walk_file_tree(
    vm: &mut Vm<'_>,
    path: &str,
    path_ref: ObjectRef,
    visitor: ObjectRef,
    depth: usize,
    max_depth: usize,
) -> Result<i32, VmError> {
    if path_ref.is_null() || visitor.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let attributes = make_basic_attributes(vm, path)?;
    let is_directory = fs_is_dir(vm, path);
    if is_directory && depth >= max_depth {
        return Ok(0);
    }
    if !is_directory {
        let result = visit_call(
            vm,
            visitor,
            "visitFile",
            "(Ljava/lang/Object;Ljava/nio/file/attribute/BasicFileAttributes;)Ljava/nio/file/FileVisitResult;",
            &[Value::Ref(path_ref), Value::Ref(attributes)],
        )?;
        return Ok(if visit_code(vm, result) == 1 { 1 } else { 0 });
    }
    let pre = visit_call(
        vm,
        visitor,
        "preVisitDirectory",
        "(Ljava/lang/Object;Ljava/nio/file/attribute/BasicFileAttributes;)Ljava/nio/file/FileVisitResult;",
        &[Value::Ref(path_ref), Value::Ref(attributes)],
    )?;
    let code = visit_code(vm, pre);
    if code == 1 {
        return Ok(1);
    }
    if code >= 2 {
        return Ok(if code == 3 { 3 } else { 0 });
    }
    {
        if depth < max_depth {
            let mut entries = fs_list(vm, path).unwrap_or_default();
            entries.sort();
            for entry in entries {
                let child = join_path(path, &entry);
                let child_ref = new_path(vm, &child)?.as_ref();
                let child_code =
                    walk_file_tree(vm, &child, child_ref, visitor, depth + 1, max_depth)?;
                if child_code == 1 {
                    return Ok(1);
                }
                if child_code == 3 {
                    break;
                }
            }
        }
        let post = visit_call(
            vm,
            visitor,
            "postVisitDirectory",
            "(Ljava/lang/Object;Ljava/io/IOException;)Ljava/nio/file/FileVisitResult;",
            &[Value::Ref(path_ref), Value::Ref(ObjectRef::NULL)],
        )?;
        let post_code = visit_code(vm, post);
        if post_code == 1 {
            return Ok(1);
        }
        if post_code == 3 {
            return Ok(3);
        }
    }
    Ok(0)
}

fn visit_call(
    vm: &mut Vm<'_>,
    visitor: ObjectRef,
    name: &str,
    descriptor: &str,
    args: &[Value],
) -> Result<Value, VmError> {
    let (class, method) = vm.resolve_virtual_method(visitor, name, descriptor)?;
    let mut call = alloc::vec![Value::Ref(visitor)];
    call.extend_from_slice(args);
    vm.invoke_method(class, method, call)
}

/// The ordinal of a `FileVisitResult` (CONTINUE=0, TERMINATE=1, SKIP_SUBTREE=2, SKIP_SIBLINGS=3).
fn visit_code(vm: &Vm<'_>, result: Value) -> i32 {
    crate::lang::read_int_field(vm, result.as_ref(), "ordinal")
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

const FILE_TIME_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
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
        "toInstant",
        "()Ljava/time/Instant;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let millis = vm
                .read_instance_long_named(this, class, "millis", "J")
                .unwrap_or(0);
            let instant = vm.resolve_class("java/time/Instant")?;
            let (declaring, method) = vm
                .find_method(instant, "ofEpochMilli", "(J)Ljava/time/Instant;")
                .ok_or_else(|| VmError::internal("Instant.ofEpochMilli"))?;
            vm.invoke_method(declaring, method, vec![Value::Long(millis)])
        },
    ),
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
// FileVisitor / SimpleFileVisitor / FileVisitResult
// -------------------------------------------------------------------------------------------

fn file_visit_result_constant(
    vm: &mut Vm<'_>,
    name: &str,
    ordinal: i32,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/nio/file/FileVisitResult")?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "ordinal", "I")?;
    vm.set_instance_int(instance, declaring, field, ordinal);
    let (declaring, field) = vm.find_field(class, "name", "Ljava/lang/String;")?;
    let text = vm.make_string(name)?;
    vm.set_instance_ref(instance, declaring, field, text);
    Ok(instance)
}

const FILE_VISIT_RESULT_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("ordinal", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "ordinal"))
    }),
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let name = vm.read_string_field_any(this, "name").unwrap_or_default();
        object(vm.make_string(&name)?)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name = vm.read_string_field_any(this, "name").unwrap_or_default();
            object(vm.make_string(&name)?)
        },
    ),
    method(
        "values",
        "()[Ljava/nio/file/FileVisitResult;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/nio/file/FileVisitResult")?;
            vm.ensure_initialized(class)?;
            let array = vm.allocate_object_array(class, 4)?;
            for (index, name) in ["CONTINUE", "TERMINATE", "SKIP_SUBTREE", "SKIP_SIBLINGS"]
                .iter()
                .enumerate()
            {
                if let Some(value) =
                    vm.read_static_ref_if_set(class, name, "Ljava/nio/file/FileVisitResult;")
                {
                    vm.array_set_ref(array, index, value)?;
                }
            }
            object(array)
        },
    ),
    method(
        "valueOf",
        "(Ljava/lang/String;)Ljava/nio/file/FileVisitResult;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = vm.string_or_empty(ref_arg(args, 0));
            let class = vm.resolve_class("java/nio/file/FileVisitResult")?;
            vm.ensure_initialized(class)?;
            if ["CONTINUE", "TERMINATE", "SKIP_SUBTREE", "SKIP_SIBLINGS"].contains(&name.as_str()) {
                if let Some(value) =
                    vm.read_static_ref_if_set(class, &name, "Ljava/nio/file/FileVisitResult;")
                {
                    return object(value);
                }
            }
            Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some(&alloc::format!(
                    "No enum constant java.nio.file.FileVisitResult.{name}"
                )),
            ))
        },
    ),
];

fn file_visit_result_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/nio/file/FileVisitResult")?;
    for (ordinal, name) in ["CONTINUE", "TERMINATE", "SKIP_SUBTREE", "SKIP_SIBLINGS"]
        .iter()
        .enumerate()
    {
        let value = file_visit_result_constant(vm, name, ordinal as i32)?;
        vm.set_static_value(
            class,
            name,
            "Ljava/nio/file/FileVisitResult;",
            Value::Ref(value),
        );
    }
    Ok(())
}

pub(crate) const FILE_VISIT_RESULT: oxjvm_vm::NativeClass = class(
    "java/nio/file/FileVisitResult",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_ENUM,
    &[
        field(
            "CONTINUE",
            "Ljava/nio/file/FileVisitResult;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "TERMINATE",
            "Ljava/nio/file/FileVisitResult;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "SKIP_SUBTREE",
            "Ljava/nio/file/FileVisitResult;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "SKIP_SIBLINGS",
            "Ljava/nio/file/FileVisitResult;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field("ordinal", "I", ACC_PRIVATE, None),
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &FILE_VISIT_RESULT_METHODS,
    Some(file_visit_result_clinit),
);

const FILE_VISITOR_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "preVisitDirectory",
        "(Ljava/lang/Object;Ljava/nio/file/attribute/BasicFileAttributes;)Ljava/nio/file/FileVisitResult;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "visitFile",
        "(Ljava/lang/Object;Ljava/nio/file/attribute/BasicFileAttributes;)Ljava/nio/file/FileVisitResult;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "visitFileFailed",
        "(Ljava/lang/Object;Ljava/io/IOException;)Ljava/nio/file/FileVisitResult;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "postVisitDirectory",
        "(Ljava/lang/Object;Ljava/io/IOException;)Ljava/nio/file/FileVisitResult;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const FILE_VISITOR: oxjvm_vm::NativeClass = class(
    "java/nio/file/FileVisitor",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &FILE_VISITOR_METHODS,
    None,
);

fn file_visit_result_continue(vm: &mut Vm<'_>) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/nio/file/FileVisitResult")?;
    vm.ensure_initialized(class)?;
    match vm.read_static_ref_if_set(class, "CONTINUE", "Ljava/nio/file/FileVisitResult;") {
        Some(value) => object(value),
        None => Err(vm.throw_new(
            "java/lang/IllegalStateException",
            Some("FileVisitResult.CONTINUE is not initialized"),
        )),
    }
}

const SIMPLE_FILE_VISITOR_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "preVisitDirectory",
        "(Ljava/lang/Object;Ljava/nio/file/attribute/BasicFileAttributes;)Ljava/nio/file/FileVisitResult;",
        ACC_PUBLIC,
        |vm, _, args| simple_file_visitor_continue(vm, args),
    ),
    method(
        "visitFile",
        "(Ljava/lang/Object;Ljava/nio/file/attribute/BasicFileAttributes;)Ljava/nio/file/FileVisitResult;",
        ACC_PUBLIC,
        |vm, _, args| simple_file_visitor_continue(vm, args),
    ),
    method(
        "visitFileFailed",
        "(Ljava/lang/Object;Ljava/io/IOException;)Ljava/nio/file/FileVisitResult;",
        ACC_PUBLIC,
        |vm, _, args| {
            if args[1].as_ref().is_null() || args[2].as_ref().is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            Err(VmError::Thrown(ref_arg(args, 2)))
        },
    ),
    method(
        "postVisitDirectory",
        "(Ljava/lang/Object;Ljava/io/IOException;)Ljava/nio/file/FileVisitResult;",
        ACC_PUBLIC,
        |vm, _, args| {
            if args[1].as_ref().is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let error = ref_arg(args, 2);
            if error.is_null() {
                return file_visit_result_continue(vm);
            }
            Err(VmError::Thrown(error))
        },
    ),
];

/// `SimpleFileVisitor`'s default: reject null arguments, then continue.
fn simple_file_visitor_continue(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    if args[1].as_ref().is_null() || args[2].as_ref().is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    file_visit_result_continue(vm)
}

pub(crate) const SIMPLE_FILE_VISITOR: oxjvm_vm::NativeClass = class(
    "java/nio/file/SimpleFileVisitor",
    Some("java/lang/Object"),
    &["java/nio/file/FileVisitor"],
    ACC_PUBLIC,
    &[],
    &SIMPLE_FILE_VISITOR_METHODS,
    None,
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
// FileSystem / FileSystems / FileStore / FileSystemProvider
// -------------------------------------------------------------------------------------------

fn new_file_system(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/nio/file/OjvmFileSystem")?;
    vm.new_instance(class)
}

fn new_file_store(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/nio/file/OjvmFileStore")?;
    vm.new_instance(class)
}

fn new_file_system_provider(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/nio/file/OjvmFileSystemProvider")?;
    vm.new_instance(class)
}

const FILE_SYSTEM_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
    method(
        "getPath",
        "(Ljava/lang/String;[Ljava/lang/String;)Ljava/nio/file/Path;",
        ACC_PUBLIC,
        |vm, _, args| path_of(vm, args),
    ),
    method(
        "getPathMatcher",
        "(Ljava/lang/String;)Ljava/nio/file/PathMatcher;",
        ACC_PUBLIC,
        |vm, _, args| {
            let spec = string_arg(vm, args[1])?;
            let Some(colon) = spec.find(':') else {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("Expected 'syntax:pattern'"),
                ));
            };
            let syntax = &spec[..colon];
            let pattern = &spec[colon + 1..];
            if syntax.is_empty() {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
            }
            let regex = if syntax.eq_ignore_ascii_case("glob") {
                glob_to_regex(vm, pattern)?
            } else if syntax.eq_ignore_ascii_case("regex") {
                String::from(pattern)
            } else {
                return Err(vm.throw_new(
                    "java/lang/UnsupportedOperationException",
                    Some(&alloc::format!("Syntax '{syntax}' not recognized")),
                ));
            };
            // Compile eagerly so a bad pattern fails here, as the JDK does.
            let _ = crate::regex::compile_from_str(vm, &regex)?;
            let class = vm.resolve_class("java/nio/file/OjvmPathMatcher")?;
            let matcher = vm.new_instance(class)?;
            vm.set_string_field(matcher, class, "regex", &regex)?;
            object(matcher)
        },
    ),
    method(
        "getSeparator",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("/")?),
    ),
    method(
        "getRootDirectories",
        "()Ljava/lang/Iterable;",
        ACC_PUBLIC,
        |vm, _, _| {
            let root = new_path(vm, "/")?.as_ref();
            object(crate::util::array_list_from(vm, &[root])?)
        },
    ),
    method(
        "getFileStores",
        "()Ljava/lang/Iterable;",
        ACC_PUBLIC,
        |vm, _, _| {
            let store = new_file_store(vm)?;
            object(crate::util::array_list_from(vm, &[store])?)
        },
    ),
    method("isOpen", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
    method("isReadOnly", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "provider",
        "()Ljava/nio/file/spi/FileSystemProvider;",
        ACC_PUBLIC,
        |vm, _, _| object(new_file_system_provider(vm)?),
    ),
    method(
        "newWatchService",
        "()Ljava/nio/file/WatchService;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/nio/file/OjvmWatchService")?;
            object(vm.new_instance(class)?)
        },
    ),
    method(
        "supportedFileAttributeViews",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, _| {
            let mut names = Vec::new();
            for view in ["basic", "dos", "owner", "posix", "unix", "user"] {
                names.push(vm.make_string(view)?);
            }
            object(crate::util::hash_set_from(vm, &names)?)
        },
    ),
];

pub(crate) const FILE_SYSTEM: oxjvm_vm::NativeClass = class(
    "java/nio/file/FileSystem",
    Some("java/lang/Object"),
    &["java/io/Closeable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &FILE_SYSTEM_METHODS,
    None,
);

const PATH_MATCHER_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "matches",
    "(Ljava/nio/file/Path;)Z",
    ACC_PUBLIC | ACC_ABSTRACT,
    crate::lang::abstract_method_pub,
)];

pub(crate) const PATH_MATCHER: oxjvm_vm::NativeClass = class(
    "java/nio/file/PathMatcher",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT | ACC_INTERFACE,
    &[],
    &PATH_MATCHER_METHODS,
    None,
);

const OJVM_PATH_MATCHER_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "matches",
    "(Ljava/nio/file/Path;)Z",
    ACC_PUBLIC,
    |vm, _, args| {
        let this = receiver(args);
        let path = ref_arg(args, 1);
        let regex = vm
            .read_string_field(this, vm.class_of(this), "regex")
            .unwrap_or_default();
        let (class, method) =
            vm.resolve_virtual_method(path, "toString", "()Ljava/lang/String;")?;
        let value = vm.invoke_method(class, method, vec![Value::Ref(path)])?;
        let text = vm.string_value(value.as_ref()).unwrap_or_default();
        let compiled = crate::regex::compile_from_str(vm, &regex)?;
        let units: Vec<u16> = text.encode_utf16().collect();
        boolean(crate::regex::match_at(&compiled, &units, true).is_some())
    },
)];

pub(crate) const OJVM_PATH_MATCHER: oxjvm_vm::NativeClass = class(
    "java/nio/file/OjvmPathMatcher",
    Some("java/lang/Object"),
    &["java/nio/file/PathMatcher"],
    ACC_PUBLIC | ACC_FINAL,
    &[field("regex", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &OJVM_PATH_MATCHER_METHODS,
    None,
);

pub(crate) const OJVM_FILE_SYSTEM: oxjvm_vm::NativeClass = class(
    "java/nio/file/OjvmFileSystem",
    Some("java/nio/file/FileSystem"),
    &["java/io/Closeable"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[],
    &[],
    None,
);

const WATCH_SERVICE_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "close",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "poll",
        "()Ljava/nio/file/WatchKey;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "take",
        "()Ljava/nio/file/WatchKey;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "poll",
        "(JLjava/util/concurrent/TimeUnit;)Ljava/nio/file/WatchKey;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const WATCH_SERVICE: oxjvm_vm::NativeClass = class(
    "java/nio/file/WatchService",
    Some("java/lang/Object"),
    &["java/io/Closeable"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &WATCH_SERVICE_METHODS,
    None,
);

const OJVM_WATCH_SERVICE_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        crate::lang::write_int_field(vm, receiver(args), "closed", 1);
        void()
    }),
    method(
        "poll",
        "()Ljava/nio/file/WatchKey;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "take",
        "()Ljava/nio/file/WatchKey;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "poll",
        "(JLjava/util/concurrent/TimeUnit;)Ljava/nio/file/WatchKey;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
];

pub(crate) const OJVM_WATCH_SERVICE: oxjvm_vm::NativeClass = class(
    "java/nio/file/OjvmWatchService",
    Some("java/lang/Object"),
    &["java/nio/file/WatchService"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field("closed", "I", ACC_PRIVATE, None)],
    &OJVM_WATCH_SERVICE_METHODS,
    None,
);

const FILE_SYSTEMS_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "getDefault",
        "()Ljava/nio/file/FileSystem;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_file_system(vm)?),
    ),
    method(
        "getFileSystem",
        "(Ljava/net/URI;)Ljava/nio/file/FileSystem;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let uri = ref_arg(args, 0);
            let (class, method) =
                vm.resolve_virtual_method(uri, "toString", "()Ljava/lang/String;")?;
            let text = vm.invoke_method(class, method, vec![Value::Ref(uri)])?;
            let text = vm.string_or_empty(text.as_ref());
            if text.starts_with("file:") {
                object(new_file_system(vm)?)
            } else {
                Err(vm.throw_new("java/nio/file/FileSystemNotFoundException", Some(&text)))
            }
        },
    ),
    method(
        "newFileSystem",
        "(Ljava/nio/file/Path;)Ljava/nio/file/FileSystem;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_file_system(vm)?),
    ),
    method(
        "newFileSystem",
        "(Ljava/nio/file/Path;Ljava/util/Map;)Ljava/nio/file/FileSystem;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_file_system(vm)?),
    ),
    method(
        "newFileSystem",
        "(Ljava/nio/file/Path;Ljava/util/Map;Ljava/lang/ClassLoader;)Ljava/nio/file/FileSystem;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_file_system(vm)?),
    ),
    method(
        "newFileSystem",
        "(Ljava/net/URI;Ljava/util/Map;)Ljava/nio/file/FileSystem;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let uri = ref_arg(args, 0);
            let (class, method) =
                vm.resolve_virtual_method(uri, "toString", "()Ljava/lang/String;")?;
            let value = vm.invoke_method(class, method, vec![Value::Ref(uri)])?;
            let text = vm.string_or_empty(value.as_ref());
            if text.starts_with("file:") {
                object(new_file_system(vm)?)
            } else {
                Err(vm.throw_new(
                    "java/nio/file/ProviderNotFoundException",
                    Some(&alloc::format!("provider for {text} not found")),
                ))
            }
        },
    ),
    method(
        "newFileSystem",
        "(Ljava/net/URI;Ljava/util/Map;Ljava/lang/ClassLoader;)Ljava/nio/file/FileSystem;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let uri = ref_arg(args, 0);
            let (class, method) =
                vm.resolve_virtual_method(uri, "toString", "()Ljava/lang/String;")?;
            let value = vm.invoke_method(class, method, vec![Value::Ref(uri)])?;
            let text = vm.string_or_empty(value.as_ref());
            if text.starts_with("file:") {
                object(new_file_system(vm)?)
            } else {
                Err(vm.throw_new(
                    "java/nio/file/ProviderNotFoundException",
                    Some(&alloc::format!("provider for {text} not found")),
                ))
            }
        },
    ),
];

pub(crate) const FILE_SYSTEMS: oxjvm_vm::NativeClass = class(
    "java/nio/file/FileSystems",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &FILE_SYSTEMS_METHODS,
    None,
);

const FILE_STORE_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, _| {
        object(vm.make_string("oxjvm")?)
    }),
    method("type", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, _| {
        object(vm.make_string("basic")?)
    }),
    method("isReadOnly", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("getTotalSpace", "()J", ACC_PUBLIC, |_, _, _| long(1 << 40)),
    method("getUsableSpace", "()J", ACC_PUBLIC, |_, _, _| long(1 << 30)),
    method("getUnallocatedSpace", "()J", ACC_PUBLIC, |_, _, _| {
        long(1 << 30)
    }),
    method("getBlockSize", "()J", ACC_PUBLIC, |_, _, _| long(4096)),
    method(
        "supportsFileAttributeView",
        "(Ljava/lang/String;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = vm.string_or_empty(ref_arg(args, 1));
            boolean(matches!(name.as_str(), "basic" | "posix" | "owner" | "dos"))
        },
    ),
    method(
        "supportsFileAttributeView",
        "(Ljava/lang/Class;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(true),
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("oxjvm")?),
    ),
];

pub(crate) const FILE_STORE: oxjvm_vm::NativeClass = class(
    "java/nio/file/FileStore",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &FILE_STORE_METHODS,
    None,
);

pub(crate) const OJVM_FILE_STORE: oxjvm_vm::NativeClass = class(
    "java/nio/file/OjvmFileStore",
    Some("java/nio/file/FileStore"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[],
    &[],
    None,
);

const FILE_SYSTEM_PROVIDER_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "getScheme",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("file")?),
    ),
    method(
        "getFileSystem",
        "(Ljava/net/URI;)Ljava/nio/file/FileSystem;",
        ACC_PUBLIC,
        |vm, _, _| object(new_file_system(vm)?),
    ),
    method(
        "newFileSystem",
        "(Ljava/nio/file/Path;Ljava/util/Map;)Ljava/nio/file/FileSystem;",
        ACC_PUBLIC,
        |vm, _, _| object(new_file_system(vm)?),
    ),
    method(
        "newFileSystem",
        "(Ljava/net/URI;Ljava/util/Map;)Ljava/nio/file/FileSystem;",
        ACC_PUBLIC,
        |vm, _, _| object(new_file_system(vm)?),
    ),
    method(
        "getPath",
        "(Ljava/net/URI;)Ljava/nio/file/Path;",
        ACC_PUBLIC,
        |vm, _, args| {
            let uri = ref_arg(args, 1);
            let (class, method) =
                vm.resolve_virtual_method(uri, "toString", "()Ljava/lang/String;")?;
            let text = vm.invoke_method(class, method, vec![Value::Ref(uri)])?;
            let text = vm.string_or_empty(text.as_ref());
            let path = text
                .strip_prefix("file:")
                .unwrap_or(&text)
                .trim_start_matches("//")
                .to_string();
            new_path(vm, &path)
        },
    ),
];

pub(crate) const FILE_SYSTEM_PROVIDER: oxjvm_vm::NativeClass = class(
    "java/nio/file/spi/FileSystemProvider",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &FILE_SYSTEM_PROVIDER_METHODS,
    None,
);

pub(crate) const OJVM_FILE_SYSTEM_PROVIDER: oxjvm_vm::NativeClass = class(
    "java/nio/file/OjvmFileSystemProvider",
    Some("java/nio/file/spi/FileSystemProvider"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[],
    &[],
    None,
);

// -------------------------------------------------------------------------------------------
// DirectoryStream implementation
// -------------------------------------------------------------------------------------------

/// Translate a glob pattern to a regex, following the JDK's `sun.nio.fs.Globs`
/// (Unix flavour). Bad patterns raise `PatternSyntaxException`.
fn glob_to_regex(vm: &mut Vm<'_>, glob: &str) -> Result<String, VmError> {
    let chars: Vec<char> = glob.chars().collect();
    let bad = |vm: &mut Vm<'_>, message: &str, index: usize| -> VmError {
        vm.throw_new(
            "java/util/regex/PatternSyntaxException",
            Some(&alloc::format!("{message} near index {index}\n{glob}")),
        )
    };
    let regex_meta = |c: char| ".^$+{[]|()".contains(c);
    let glob_meta = |c: char| "\\*?[{".contains(c);
    let next = |i: usize| -> char { chars.get(i).copied().unwrap_or('\0') };
    let mut regex = String::from("^");
    let mut in_group = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        match c {
            '\\' => {
                if i == chars.len() {
                    return Err(bad(vm, "No character to escape", i - 1));
                }
                let n = chars[i];
                i += 1;
                if glob_meta(n) || regex_meta(n) {
                    regex.push('\\');
                }
                regex.push(n);
            }
            '[' => {
                // Build a flat character class. The JDK wraps the class in an
                // intersection `[[^/]&&[...]]` to exclude the name separator; this
                // regex engine has no intersection, so fold the separator into the
                // class directly (positive classes never contain '/', negated ones
                // gain it).
                let mut body = String::new();
                let mut negated = false;
                if next(i) == '^' {
                    body.push_str("\\^");
                    i += 1;
                } else {
                    if next(i) == '!' {
                        negated = true;
                        i += 1;
                    }
                    if next(i) == '-' {
                        body.push('-');
                        i += 1;
                    }
                }
                let mut has_range_start = false;
                let mut last = '\0';
                let mut closed = false;
                while i < chars.len() {
                    let c = chars[i];
                    i += 1;
                    if c == ']' {
                        closed = true;
                        break;
                    }
                    if c == '/' {
                        return Err(bad(vm, "Explicit 'name separator' in class", i - 1));
                    }
                    if c == '\\' || c == '[' || (c == '&' && next(i) == '&') {
                        body.push('\\');
                    }
                    body.push(c);
                    if c == '-' {
                        if !has_range_start {
                            return Err(bad(vm, "Invalid range", i - 1));
                        }
                        let n = next(i);
                        i += 1;
                        if n == ']' {
                            closed = true;
                            break;
                        }
                        if n == '\0' {
                            break;
                        }
                        if n < last {
                            return Err(bad(vm, "Invalid range", i - 3));
                        }
                        body.push(n);
                        has_range_start = false;
                    } else {
                        has_range_start = true;
                        last = c;
                    }
                }
                if !closed {
                    return Err(bad(vm, "Missing ']'", i - 1));
                }
                if negated {
                    regex.push_str("[^/");
                } else {
                    regex.push('[');
                }
                regex.push_str(&body);
                regex.push(']');
            }
            '{' => {
                if in_group {
                    return Err(bad(vm, "Cannot nest groups", i - 1));
                }
                regex.push_str("(?:(?:");
                in_group = true;
            }
            '}' => {
                if in_group {
                    regex.push_str("))");
                    in_group = false;
                } else {
                    regex.push('}');
                }
            }
            ',' => {
                if in_group {
                    regex.push_str(")|(?:");
                } else {
                    regex.push(',');
                }
            }
            '*' => {
                if next(i) == '*' {
                    regex.push_str(".*");
                    i += 1;
                } else {
                    regex.push_str("[^/]*");
                }
            }
            '?' => regex.push_str("[^/]"),
            _ => {
                if regex_meta(c) {
                    regex.push('\\');
                }
                regex.push(c);
            }
        }
    }
    if in_group {
        return Err(bad(vm, "Missing '}'", i - 1));
    }
    regex.push('$');
    Ok(regex)
}

fn glob_matches(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    let mut pi = 0;
    let mut ni = 0;
    let mut star = None;
    let mut mark = 0;
    while ni < name.len() {
        if pi < pattern.len() && (pattern[pi] == '?' || pattern[pi] == name[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < pattern.len() && pattern[pi] == '*' {
            star = Some(pi);
            mark = ni;
            pi += 1;
        } else if let Some(index) = star {
            pi = index + 1;
            mark += 1;
            ni = mark;
        } else {
            return false;
        }
    }
    while pi < pattern.len() && pattern[pi] == '*' {
        pi += 1;
    }
    pi == pattern.len()
}

fn new_directory_stream(vm: &mut Vm<'_>, path: &str, glob: Option<&str>) -> Result<Value, VmError> {
    let names = fs_list(vm, path).unwrap_or_default();
    let mut elements = Vec::new();
    for name in names {
        if glob.is_none_or(|pattern| glob_matches(pattern, &name)) {
            elements.push(new_path(vm, &join_path(path, &name))?.as_ref());
        }
    }
    let class = vm.resolve_class("java/nio/file/OjvmDirectoryStream")?;
    let stream = vm.new_instance(class)?;
    let object_class = vm.resolve_class("java/lang/Object")?;
    let array = vm.allocate_object_array(object_class, elements.len())?;
    for (index, element) in elements.iter().enumerate() {
        vm.array_set_ref(array, index, *element)?;
    }
    let (declaring, field) = vm.find_field(class, "elements", "[Ljava/lang/Object;")?;
    vm.set_instance_ref(stream, declaring, field, array);
    crate::lang::write_int_field(vm, stream, "cursor", 0);
    object(stream)
}

fn new_directory_stream_filtered(
    vm: &mut Vm<'_>,
    path: &str,
    filter: ObjectRef,
) -> Result<Value, VmError> {
    let names = fs_list(vm, path).unwrap_or_default();
    let mut elements = Vec::new();
    for name in names {
        let entry = new_path(vm, &join_path(path, &name))?;
        if filter.is_null() {
            continue;
        }
        let (declaring, method) =
            vm.resolve_virtual_method(filter, "accept", "(Ljava/nio/file/Path;)Z")?;
        let accepted = vm
            .invoke_method(
                declaring,
                method,
                vec![Value::Ref(filter), Value::Ref(entry.as_ref())],
            )?
            .as_int()
            != 0;
        if accepted {
            elements.push(entry.as_ref());
        }
    }
    let class = vm.resolve_class("java/nio/file/OjvmDirectoryStream")?;
    let stream = vm.new_instance(class)?;
    let object_class = vm.resolve_class("java/lang/Object")?;
    let array = vm.allocate_object_array(object_class, elements.len())?;
    for (index, element) in elements.iter().enumerate() {
        vm.array_set_ref(array, index, *element)?;
    }
    let (declaring, field) = vm.find_field(class, "elements", "[Ljava/lang/Object;")?;
    vm.set_instance_ref(stream, declaring, field, array);
    crate::lang::write_int_field(vm, stream, "cursor", 0);
    object(stream)
}

const OJVM_DIRECTORY_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "iterator",
        "()Ljava/util/Iterator;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let elements = vm
                .read_ref_field(this, vm.class_of(this), "elements", "[Ljava/lang/Object;")
                .unwrap_or(ObjectRef::NULL);
            let class = vm.resolve_class("java/nio/file/OjvmPathIterator")?;
            let iterator = vm.new_instance(class)?;
            let (declaring, field) = vm.find_field(class, "elements", "[Ljava/lang/Object;")?;
            vm.set_instance_ref(iterator, declaring, field, elements);
            crate::lang::write_int_field(vm, iterator, "cursor", 0);
            object(iterator)
        },
    ),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
];

pub(crate) const OJVM_DIRECTORY_STREAM: oxjvm_vm::NativeClass = class(
    "java/nio/file/OjvmDirectoryStream",
    Some("java/lang/Object"),
    &["java/nio/file/DirectoryStream", "java/io/Closeable"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field("elements", "[Ljava/lang/Object;", ACC_PRIVATE, None)],
    &OJVM_DIRECTORY_STREAM_METHODS,
    None,
);

const OJVM_PATH_ITERATOR_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method("hasNext", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let elements = vm
            .read_ref_field(this, vm.class_of(this), "elements", "[Ljava/lang/Object;")
            .unwrap_or(ObjectRef::NULL);
        let length = vm.array_length(elements).unwrap_or(0);
        boolean(crate::lang::read_int_field(vm, this, "cursor") < length as i32)
    }),
    method("next", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let elements = vm
            .read_ref_field(this, vm.class_of(this), "elements", "[Ljava/lang/Object;")
            .unwrap_or(ObjectRef::NULL);
        let cursor = crate::lang::read_int_field(vm, this, "cursor");
        let length = vm.array_length(elements).unwrap_or(0);
        if cursor < 0 || cursor >= length as i32 {
            return Err(vm.throw_new("java/util/NoSuchElementException", None));
        }
        crate::lang::write_int_field(vm, this, "cursor", cursor + 1);
        vm.array_get(elements, cursor)
    }),
    method("remove", "()V", ACC_PUBLIC, |_, _, _| {
        Err(VmError::internal("Iterator.remove is unsupported"))
    }),
];

pub(crate) const OJVM_PATH_ITERATOR: oxjvm_vm::NativeClass = class(
    "java/nio/file/OjvmPathIterator",
    Some("java/lang/Object"),
    &["java/util/Iterator"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[
        field("elements", "[Ljava/lang/Object;", ACC_PRIVATE, None),
        field("cursor", "I", ACC_PRIVATE, None),
    ],
    &OJVM_PATH_ITERATOR_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// FileChannel
// -------------------------------------------------------------------------------------------

pub(crate) fn new_file_channel(
    vm: &mut Vm<'_>,
    path: &str,
    position: i64,
    writable: bool,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/nio/channels/OjvmFileChannel")?;
    let channel = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "path", "Ljava/lang/String;")?;
    let text = vm.make_string(path)?;
    vm.set_instance_ref(channel, declaring, field, text);
    let (declaring, field) = vm.find_field(class, "position", "J")?;
    vm.set_instance_long(channel, declaring, field, position);
    crate::lang::write_int_field(vm, channel, "writable", i32::from(writable));
    crate::lang::write_int_field(vm, channel, "readable", 1);
    crate::lang::write_int_field(vm, channel, "open", 1);
    Ok(channel)
}

fn file_channel_read(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    target: ObjectRef,
    absolute: Option<i64>,
) -> Result<i32, VmError> {
    let path = vm
        .read_string_field(this, vm.class_of(this), "path")
        .unwrap_or_default();
    let Some(bytes) = vm.host.filesystem().and_then(|fs| fs.read(&path)) else {
        return Err(vm.throw_new("java/io/IOException", Some(&path)));
    };
    let position = absolute.unwrap_or_else(|| {
        vm.read_instance_long_named(this, vm.class_of(this), "position", "J")
            .unwrap_or(0)
    });
    if position >= bytes.len() as i64 {
        return Ok(-1);
    }
    let start = position as usize;
    let available = &bytes[start..];
    let count = crate::buffer::byte_buffer_put_slice(vm, target, available)?;
    if absolute.is_none() {
        let (declaring, field) = vm.find_field(vm.class_of(this), "position", "J")?;
        vm.set_instance_long(this, declaring, field, position + count as i64);
    }
    Ok(count as i32)
}

fn file_channel_write(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    source: ObjectRef,
    absolute: Option<i64>,
) -> Result<i32, VmError> {
    let path = vm
        .read_string_field(this, vm.class_of(this), "path")
        .unwrap_or_default();
    let bytes = crate::buffer::byte_buffer_remaining_bytes(vm, source)?;
    let mut existing = vm
        .host
        .filesystem()
        .and_then(|fs| fs.read(&path))
        .unwrap_or_default();
    let position = absolute
        .unwrap_or_else(|| {
            vm.read_instance_long_named(this, vm.class_of(this), "position", "J")
                .unwrap_or(0)
        })
        .max(0) as usize;
    if existing.len() < position + bytes.len() {
        existing.resize(position + bytes.len(), 0);
    }
    existing[position..position + bytes.len()].copy_from_slice(&bytes);
    if !vm
        .host
        .filesystem()
        .map(|fs| fs.write(&path, &existing, false))
        .unwrap_or(false)
    {
        return Err(vm.throw_new("java/io/IOException", Some(&path)));
    }
    let (declaring, field) = vm.find_field(vm.class_of(this), "position", "J")?;
    vm.set_instance_long(this, declaring, field, (position + bytes.len()) as i64);
    Ok(bytes.len() as i32)
}

const FILE_CHANNEL_METHODS: [oxjvm_vm::NativeMethodDef; 16] = [
    method(
        "open",
        "(Ljava/nio/file/Path;[Ljava/nio/file/OpenOption;)Ljava/nio/channels/FileChannel;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            object(new_file_channel(vm, &path, 0, true)?)
        },
    ),
    method(
        "open",
        "(Ljava/nio/file/Path;Ljava/util/Set;[Ljava/nio/file/attribute/FileAttribute;)Ljava/nio/channels/FileChannel;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let path = path_string(vm, ref_arg(args, 0));
            object(new_file_channel(vm, &path, 0, true)?)
        },
    ),
    method(
        "read",
        "(Ljava/nio/ByteBuffer;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            int(file_channel_read(
                vm,
                receiver(args),
                ref_arg(args, 1),
                None,
            )?)
        },
    ),
    method(
        "read",
        "(Ljava/nio/ByteBuffer;J)I",
        ACC_PUBLIC,
        |vm, _, args| {
            int(file_channel_read(
                vm,
                receiver(args),
                ref_arg(args, 1),
                Some(crate::long_arg(args, 2)),
            )?)
        },
    ),
    method(
        "write",
        "(Ljava/nio/ByteBuffer;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            int(file_channel_write(
                vm,
                receiver(args),
                ref_arg(args, 1),
                None,
            )?)
        },
    ),
    method(
        "write",
        "(Ljava/nio/ByteBuffer;J)I",
        ACC_PUBLIC,
        |vm, _, args| {
            int(file_channel_write(
                vm,
                receiver(args),
                ref_arg(args, 1),
                Some(crate::long_arg(args, 2)),
            )?)
        },
    ),
    method("position", "()J", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        long(
            vm.read_instance_long_named(this, vm.class_of(this), "position", "J")
                .unwrap_or(0),
        )
    }),
    method(
        "position",
        "(J)Ljava/nio/channels/FileChannel;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let new_position = crate::long_arg(args, 1);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "position", "J")?;
            vm.set_instance_long(this, declaring, field, new_position);
            let owner = vm
                .read_ref_field(this, class, "owner", "Ljava/lang/Object;")
                .unwrap_or(ObjectRef::NULL);
            if !owner.is_null() {
                propagate_channel_position(vm, owner, new_position);
            }
            object(this)
        },
    ),
    method("size", "()J", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let path = vm
            .read_string_field(this, vm.class_of(this), "path")
            .unwrap_or_default();
        long(fs_metadata(vm, &path).map_or(0, |metadata| metadata.len as i64))
    }),
    method(
        "truncate",
        "(J)Ljava/nio/channels/FileChannel;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let path = vm
                .read_string_field(this, vm.class_of(this), "path")
                .unwrap_or_default();
            let size = crate::long_arg(args, 1).max(0) as usize;
            let mut bytes = vm
                .host
                .filesystem()
                .and_then(|fs| fs.read(&path))
                .unwrap_or_default();
            bytes.truncate(size);
            if !vm
                .host
                .filesystem()
                .map(|fs| fs.write(&path, &bytes, false))
                .unwrap_or(false)
            {
                return Err(vm.throw_new("java/io/IOException", Some(&path)));
            }
            object(this)
        },
    ),
    method(
        "transferTo",
        "(JJLjava/nio/channels/WritableByteChannel;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let position = crate::long_arg(args, 1);
            let count = crate::long_arg(args, 2);
            let target = ref_arg(args, 3);
            if position < 0 || count < 0 {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
            }
            let path = vm
                .read_string_field(this, vm.class_of(this), "path")
                .unwrap_or_default();
            let bytes = vm
                .host
                .filesystem()
                .and_then(|fs| fs.read(&path))
                .unwrap_or_default();
            let start = (position as usize).min(bytes.len());
            let end = (position.saturating_add(count) as usize).min(bytes.len());
            let buffer = crate::buffer::byte_buffer_from_bytes(vm, &bytes[start..end])?;
            let (class, method) =
                vm.resolve_virtual_method(target, "write", "(Ljava/nio/ByteBuffer;)I")?;
            let written = vm
                .invoke_method(class, method, vec![Value::Ref(target), Value::Ref(buffer)])?
                .as_int();
            long(i64::from(written.max(0)))
        },
    ),
    method(
        "transferFrom",
        "(Ljava/nio/channels/ReadableByteChannel;JJ)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let source = ref_arg(args, 1);
            let position = crate::long_arg(args, 2);
            let count = crate::long_arg(args, 3);
            if position < 0 || count < 0 {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
            }
            let capacity = count.min(i64::from(i32::MAX)) as usize;
            let buffer = crate::buffer::byte_buffer_from_bytes(vm, &vec![0u8; capacity])?;
            let (class, method) =
                vm.resolve_virtual_method(source, "read", "(Ljava/nio/ByteBuffer;)I")?;
            let read = vm
                .invoke_method(class, method, vec![Value::Ref(source), Value::Ref(buffer)])?
                .as_int();
            if read <= 0 {
                return long(0);
            }
            let (class, method) =
                vm.resolve_virtual_method(buffer, "flip", "()Ljava/nio/Buffer;")?;
            vm.invoke_method(class, method, vec![Value::Ref(buffer)])?;
            let bytes = crate::buffer::byte_buffer_consume_remaining(vm, buffer)?;
            let path = vm
                .read_string_field(this, vm.class_of(this), "path")
                .unwrap_or_default();
            let mut existing = vm
                .host
                .filesystem()
                .and_then(|fs| fs.read(&path))
                .unwrap_or_default();
            let start = position as usize;
            if existing.len() < start + bytes.len() {
                existing.resize(start + bytes.len(), 0);
            }
            existing[start..start + bytes.len()].copy_from_slice(&bytes);
            if !vm
                .host
                .filesystem()
                .map(|fs| fs.write(&path, &existing, false))
                .unwrap_or(false)
            {
                return Err(vm.throw_new("java/io/IOException", Some(&path)));
            }
            long(bytes.len() as i64)
        },
    ),
    method(
        "map",
        "(Ljava/nio/channels/FileChannel$MapMode;JJ)Ljava/nio/MappedByteBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let position = crate::long_arg(args, 2);
            let size = crate::long_arg(args, 3);
            if position < 0 {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("Negative position"),
                ));
            }
            if size < 0 {
                return Err(
                    vm.throw_new("java/lang/IllegalArgumentException", Some("Negative size"))
                );
            }
            if size > i64::from(i32::MAX) {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("Size exceeds Integer.MAX_VALUE"),
                ));
            }
            let mode = ref_arg(args, 1);
            let mode_name = vm
                .read_string_field(mode, vm.class_of(mode), "name")
                .unwrap_or_default();
            let read_only = mode_name == "READ_ONLY";
            let writable = crate::lang::read_int_field(vm, this, "writable") != 0;
            let readable = crate::lang::read_int_field(vm, this, "readable") != 0;
            if read_only && !readable {
                return Err(vm.throw_new("java/nio/channels/NonReadableChannelException", None));
            }
            if !read_only && !writable {
                return Err(vm.throw_new("java/nio/channels/NonWritableChannelException", None));
            }
            let path = vm
                .read_string_field(this, vm.class_of(this), "path")
                .unwrap_or_default();
            let bytes = vm
                .host
                .filesystem()
                .and_then(|fs| fs.read(&path))
                .unwrap_or_default();
            // The mapped region is always `size` bytes; bytes past the current
            // end of file read as zero.  A writable mapping extends the file to
            // cover the region, just like the JDK.
            let start = position as usize;
            let length = size as usize;
            let mut region = vec![0u8; length];
            if start < bytes.len() {
                let available = (bytes.len() - start).min(length);
                region[..available].copy_from_slice(&bytes[start..start + available]);
            }
            if !read_only {
                let needed = start.saturating_add(length);
                if bytes.len() < needed {
                    let mut extended = bytes;
                    extended.resize(needed, 0);
                    let _ = vm
                        .host
                        .filesystem()
                        .map(|fs| fs.write(&path, &extended, false));
                }
            }
            object(crate::buffer::mapped_byte_buffer_from_bytes(
                vm, &region, read_only,
            )?)
        },
    ),
    method("force", "(Z)V", ACC_PUBLIC, |_, _, _| void()),
    method("isOpen", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "open") != 0)
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        crate::lang::write_int_field(vm, this, "open", 0);
        void()
    }),
];

pub(crate) const FILE_CHANNEL: oxjvm_vm::NativeClass = class(
    "java/nio/channels/FileChannel",
    Some("java/lang/Object"),
    &["java/nio/channels/ByteChannel"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &FILE_CHANNEL_METHODS,
    None,
);

pub(crate) const OJVM_FILE_CHANNEL: oxjvm_vm::NativeClass = class(
    "java/nio/channels/OjvmFileChannel",
    Some("java/nio/channels/FileChannel"),
    &["java/nio/channels/ByteChannel"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[
        field("path", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("position", "J", ACC_PRIVATE, None),
        field("writable", "I", ACC_PRIVATE, None),
        field("readable", "I", ACC_PRIVATE, None),
        field("open", "I", ACC_PRIVATE, None),
        field("owner", "Ljava/lang/Object;", ACC_PRIVATE, None),
    ],
    &[],
    None,
);

const MAP_MODE_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "toString",
    "()Ljava/lang/String;",
    ACC_PUBLIC,
    |vm, _, args| {
        let name = vm
            .read_string_field(receiver(args), vm.class_of(receiver(args)), "name")
            .unwrap_or_default();
        object(vm.make_string(&name)?)
    },
)];

fn map_mode_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/nio/channels/FileChannel$MapMode")?;
    for name in ["READ_ONLY", "READ_WRITE", "PRIVATE"] {
        let instance = vm.new_instance(class)?;
        vm.set_string_field(instance, class, "name", name)?;
        vm.set_static_value(
            class,
            name,
            "Ljava/nio/channels/FileChannel$MapMode;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

pub(crate) const MAP_MODE: oxjvm_vm::NativeClass = class(
    "java/nio/channels/FileChannel$MapMode",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field(
            "READ_ONLY",
            "Ljava/nio/channels/FileChannel$MapMode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "READ_WRITE",
            "Ljava/nio/channels/FileChannel$MapMode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "PRIVATE",
            "Ljava/nio/channels/FileChannel$MapMode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &MAP_MODE_METHODS,
    Some(map_mode_clinit),
);

/// Link a stream-derived channel back to its owning stream so that seeking the
/// channel (`FileChannel.position(long)`) moves the stream's read/write cursor,
/// matching the JDK's shared file-descriptor position.
pub(crate) fn set_file_channel_owner(
    vm: &mut Vm<'_>,
    channel: ObjectRef,
    owner: ObjectRef,
) -> Result<(), VmError> {
    let class = vm.class_of(channel);
    let (declaring, field) = vm.find_field(class, "owner", "Ljava/lang/Object;")?;
    vm.set_instance_ref(channel, declaring, field, owner);
    Ok(())
}

/// Mirror a channel seek onto the owning stream's `position` field (int or long).
fn propagate_channel_position(vm: &mut Vm<'_>, owner: ObjectRef, position: i64) {
    let class = vm.class_of(owner);
    if let Some((declaring, field)) = vm.find_field_opt(class, "position", "I") {
        vm.set_instance_int(owner, declaring, field, position as i32);
    } else if let Some((declaring, field)) = vm.find_field_opt(class, "position", "J") {
        vm.set_instance_long(owner, declaring, field, position);
    }
}

pub(crate) const BYTE_CHANNEL: oxjvm_vm::NativeClass = class(
    "java/nio/channels/ByteChannel",
    Some("java/lang/Object"),
    &[
        "java/nio/channels/ReadableByteChannel",
        "java/nio/channels/WritableByteChannel",
    ],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const READABLE_BYTE_CHANNEL: oxjvm_vm::NativeClass = class(
    "java/nio/channels/ReadableByteChannel",
    Some("java/lang/Object"),
    &["java/nio/channels/Channel"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[method(
        "read",
        "(Ljava/nio/ByteBuffer;)I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    )],
    None,
);

pub(crate) const WRITABLE_BYTE_CHANNEL: oxjvm_vm::NativeClass = class(
    "java/nio/channels/WritableByteChannel",
    Some("java/lang/Object"),
    &["java/nio/channels/Channel"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[method(
        "write",
        "(Ljava/nio/ByteBuffer;)I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    )],
    None,
);

pub(crate) const CHANNEL: oxjvm_vm::NativeClass = class(
    "java/nio/channels/Channel",
    Some("java/lang/Object"),
    &["java/io/Closeable"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[
        method(
            "isOpen",
            "()Z",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
        method(
            "close",
            "()V",
            ACC_PUBLIC | ACC_ABSTRACT,
            crate::lang::abstract_method_pub,
        ),
    ],
    None,
);
