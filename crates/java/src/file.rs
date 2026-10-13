//! `java.io.File` and the file-backed streams, over the host [`FileSystem`] boundary.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_platform::{FileKind, FileMetadata};
use oxjvm_vm::{ArrayComponent, ObjectRef, Value, Vm, VmError};

use crate::{
    boolean, class, field, int, long, long_arg, method, object, receiver, ref_arg, string_arg, void,
};

// -------------------------------------------------------------------------------------------
// java.io.File
// -------------------------------------------------------------------------------------------

const FILE_METHODS: [oxjvm_vm::NativeMethodDef; 33] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = string_arg(vm, args[1])?;
            file_init_path(vm, receiver(args), path)
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let parent = string_arg(vm, args[1])?;
            let child = string_arg(vm, args[2])?;
            file_init_path(vm, receiver(args), join_paths(&parent, &child))
        },
    ),
    method(
        "<init>",
        "(Ljava/io/File;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let parent = file_path(vm, ref_arg(args, 1));
            let child = string_arg(vm, args[2])?;
            file_init_path(vm, receiver(args), join_paths(&parent, &child))
        },
    ),
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = file_path(vm, receiver(args));
            object(vm.make_string(&name_of(&path))?)
        },
    ),
    method(
        "getPath",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = file_path(vm, receiver(args));
            object(vm.make_string(&path)?)
        },
    ),
    method(
        "getParent",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = file_path(vm, receiver(args));
            match parent_of_path(&path) {
                Some(parent) => object(vm.make_string(&parent)?),
                None => object(ObjectRef::NULL),
            }
        },
    ),
    method(
        "getParentFile",
        "()Ljava/io/File;",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = file_path(vm, receiver(args));
            match parent_of_path(&path) {
                Some(parent) => new_file(vm, &parent),
                None => object(ObjectRef::NULL),
            }
        },
    ),
    method(
        "getAbsolutePath",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = file_path(vm, receiver(args));
            let absolute = absolute_of_path(vm, &path);
            object(vm.make_string(&absolute)?)
        },
    ),
    method(
        "getAbsoluteFile",
        "()Ljava/io/File;",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = file_path(vm, receiver(args));
            let absolute = absolute_of_path(vm, &path);
            new_file(vm, &absolute)
        },
    ),
    method(
        "getCanonicalPath",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = file_path(vm, receiver(args));
            let canonical = canonical_of(vm, &path);
            object(vm.make_string(&canonical)?)
        },
    ),
    method(
        "getCanonicalFile",
        "()Ljava/io/File;",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = file_path(vm, receiver(args));
            let canonical = canonical_of(vm, &path);
            new_file(vm, &canonical)
        },
    ),
    method("exists", "()Z", ACC_PUBLIC, |vm, _, args| {
        let path = file_path(vm, receiver(args));
        boolean(fs_metadata(vm, &path).is_some())
    }),
    method("isFile", "()Z", ACC_PUBLIC, |vm, _, args| {
        let path = file_path(vm, receiver(args));
        boolean(fs_metadata(vm, &path).is_some_and(|metadata| metadata.kind == FileKind::File))
    }),
    method("isDirectory", "()Z", ACC_PUBLIC, |vm, _, args| {
        let path = file_path(vm, receiver(args));
        boolean(fs_metadata(vm, &path).is_some_and(|metadata| metadata.kind == FileKind::Directory))
    }),
    method("isHidden", "()Z", ACC_PUBLIC, |vm, _, args| {
        let path = file_path(vm, receiver(args));
        boolean(name_of(&path).starts_with('.'))
    }),
    method("length", "()J", ACC_PUBLIC, |vm, _, args| {
        let path = file_path(vm, receiver(args));
        long(
            fs_metadata(vm, &path)
                .map(|metadata| metadata.len as i64)
                .unwrap_or(0),
        )
    }),
    method("lastModified", "()J", ACC_PUBLIC, |vm, _, args| {
        let path = file_path(vm, receiver(args));
        long(
            fs_metadata(vm, &path)
                .map(|metadata| metadata.modified_millis)
                .unwrap_or(0),
        )
    }),
    method("setLastModified", "(J)Z", ACC_PUBLIC, |vm, _, args| {
        let path = file_path(vm, receiver(args));
        let millis = long_arg(args, 1);
        let ok = fs_set_modified(vm, &path, millis);
        boolean(ok)
    }),
    method("canRead", "()Z", ACC_PUBLIC, |vm, _, args| {
        let path = file_path(vm, receiver(args));
        boolean(fs_metadata(vm, &path).is_some())
    }),
    method("canWrite", "()Z", ACC_PUBLIC, |vm, _, args| {
        let path = file_path(vm, receiver(args));
        boolean(fs_metadata(vm, &path).is_some_and(|metadata| !metadata.readonly))
    }),
    method("canExecute", "()Z", ACC_PUBLIC, |vm, _, args| {
        let path = file_path(vm, receiver(args));
        boolean(fs_metadata(vm, &path).is_some())
    }),
    method("createNewFile", "()Z", ACC_PUBLIC, |vm, _, args| {
        let path = file_path(vm, receiver(args));
        if fs_metadata(vm, &path).is_some() {
            return boolean(false);
        }
        if fs_write(vm, &path, &[], false) {
            boolean(true)
        } else {
            Err(vm.throw_new(
                "java/io/IOException",
                Some(&alloc::format!("Cannot create {path}")),
            ))
        }
    }),
    method("mkdir", "()Z", ACC_PUBLIC, |vm, _, args| {
        let path = file_path(vm, receiver(args));
        let created = vm
            .host
            .filesystem()
            .map(|fs| fs.create_dir(&path))
            .unwrap_or(false);
        boolean(created)
    }),
    method("mkdirs", "()Z", ACC_PUBLIC, |vm, _, args| {
        let path = file_path(vm, receiver(args));
        boolean(fs_create_dir_all(vm, &path))
    }),
    method("delete", "()Z", ACC_PUBLIC, |vm, _, args| {
        let path = file_path(vm, receiver(args));
        let Some(metadata) = fs_metadata(vm, &path) else {
            return boolean(false);
        };
        let removed = match metadata.kind {
            FileKind::Directory => fs_remove_dir(vm, &path),
            _ => fs_remove_file(vm, &path),
        };
        boolean(removed)
    }),
    method(
        "renameTo",
        "(Ljava/io/File;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let from = file_path(vm, receiver(args));
            let to = file_path(vm, ref_arg(args, 1));
            boolean(fs_rename(vm, &from, &to))
        },
    ),
    method(
        "list",
        "()[Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = file_path(vm, receiver(args));
            match fs_list(vm, &path) {
                Some(names) => crate::lang::string_array(vm, &names),
                None => object(ObjectRef::NULL),
            }
        },
    ),
    method(
        "listFiles",
        "()[Ljava/io/File;",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = file_path(vm, receiver(args));
            let Some(names) = fs_list(vm, &path) else {
                return object(ObjectRef::NULL);
            };
            let file_class = vm.resolve_class("java/io/File")?;
            let array = vm.allocate_object_array(file_class, names.len())?;
            for (index, name) in names.iter().enumerate() {
                let full = join_paths(&path, name);
                let value = new_file(vm, &full)?;
                vm.array_set_ref(array, index, value.as_ref())?;
            }
            object(array)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = file_path(vm, receiver(args));
            object(vm.make_string(&path)?)
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
            boolean(file_path(vm, receiver(args)) == file_path(vm, other))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let path = file_path(vm, receiver(args));
        let mut hash = 0i32;
        for unit in path.encode_utf16() {
            hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
        }
        int(hash)
    }),
    method(
        "compareTo",
        "(Ljava/io/File;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let a = file_path(vm, receiver(args));
            let b = file_path(vm, ref_arg(args, 1));
            int(match a.cmp(&b) {
                core::cmp::Ordering::Less => -1,
                core::cmp::Ordering::Equal => 0,
                core::cmp::Ordering::Greater => 1,
            })
        },
    ),
    method("isAbsolute", "()Z", ACC_PUBLIC, |vm, _, args| {
        let path = file_path(vm, receiver(args));
        boolean(path.starts_with('/'))
    }),
];

pub(crate) const FILE: oxjvm_vm::NativeClass = class(
    "java/io/File",
    Some("java/lang/Object"),
    &["java/io/Serializable", "java/lang/Comparable"],
    ACC_PUBLIC,
    &[
        field("path", "Ljava/lang/String;", ACC_PRIVATE, None),
        field(
            "separator",
            "Ljava/lang/String;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "separatorChar",
            "C",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "pathSeparator",
            "Ljava/lang/String;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "pathSeparatorChar",
            "C",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &FILE_METHODS,
    Some(file_clinit),
);

fn file_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/io/File")?;
    let separator =
        crate::lang::system_property(vm, "file.separator").unwrap_or_else(|| "/".into());
    let separator = vm.make_string(&separator)?;
    vm.set_static_value(
        class,
        "separator",
        "Ljava/lang/String;",
        Value::Ref(separator),
    );
    let path_separator =
        crate::lang::system_property(vm, "path.separator").unwrap_or_else(|| ":".into());
    let path_separator = vm.make_string(&path_separator)?;
    vm.set_static_value(
        class,
        "pathSeparator",
        "Ljava/lang/String;",
        Value::Ref(path_separator),
    );
    vm.set_static_value(class, "separatorChar", "C", Value::Int('/' as i32));
    vm.set_static_value(class, "pathSeparatorChar", "C", Value::Int(':' as i32));
    Ok(())
}

pub(crate) fn file_path(vm: &Vm<'_>, this: ObjectRef) -> String {
    let class = vm.class_of(this);
    vm.read_string_field(this, class, "path")
        .unwrap_or_default()
}

fn file_init_path(vm: &mut Vm<'_>, this: ObjectRef, path: String) -> Result<Value, VmError> {
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "path", "Ljava/lang/String;")?;
    let text = vm.make_string(&path)?;
    vm.set_instance_ref(this, declaring, field, text);
    void()
}

fn new_file(vm: &mut Vm<'_>, path: &str) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/io/File")?;
    let instance = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(class, "<init>", "(Ljava/lang/String;)V")
        .ok_or_else(|| VmError::internal("File lacks its constructor"))?;
    let text = vm.make_string(path)?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![Value::Ref(instance), Value::Ref(text)],
    )?;
    object(instance)
}

fn join_paths(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        return child.to_string();
    }
    if child.is_empty() {
        return parent.to_string();
    }
    if parent.ends_with('/') {
        alloc::format!("{parent}{child}")
    } else {
        alloc::format!("{parent}/{child}")
    }
}

pub(crate) fn parent_of_path(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.rfind('/') {
        Some(0) => Some("/".to_string()),
        Some(index) => Some(trimmed[..index].to_string()),
        None => None,
    }
}

fn name_of(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(index) => trimmed[index + 1..].to_string(),
        None => trimmed.to_string(),
    }
}

pub(crate) fn absolute_of_path(vm: &mut Vm<'_>, path: &str) -> String {
    if path.starts_with('/') {
        return path.to_string();
    }
    let cwd = crate::lang::system_property(vm, "user.dir").unwrap_or_else(|| "/".into());
    let cwd = cwd.trim_end_matches('/');
    if cwd.is_empty() {
        alloc::format!("/{path}")
    } else {
        alloc::format!("{cwd}/{path}")
    }
}

fn canonical_of(vm: &mut Vm<'_>, path: &str) -> String {
    let absolute = absolute_of_path(vm, path);
    vm.host
        .filesystem()
        .and_then(|fs| fs.canonicalize(&absolute))
        .unwrap_or(absolute)
}

fn fs_metadata(vm: &mut Vm<'_>, path: &str) -> Option<FileMetadata> {
    vm.host.filesystem()?.metadata(path)
}

fn fs_list(vm: &mut Vm<'_>, path: &str) -> Option<Vec<String>> {
    vm.host.filesystem()?.list(path)
}

fn fs_write(vm: &mut Vm<'_>, path: &str, bytes: &[u8], append: bool) -> bool {
    vm.host
        .filesystem()
        .map(|fs| fs.write(path, bytes, append))
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

fn fs_set_modified(vm: &mut Vm<'_>, path: &str, millis: i64) -> bool {
    vm.host
        .filesystem()
        .map(|fs| fs.set_modified(path, millis))
        .unwrap_or(false)
}

// -------------------------------------------------------------------------------------------
// java.io.FileInputStream
// -------------------------------------------------------------------------------------------

const FILE_INPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = string_arg(vm, args[1])?;
            file_input_init(vm, receiver(args), &path)
        },
    ),
    method("<init>", "(Ljava/io/File;)V", ACC_PUBLIC, |vm, _, args| {
        let path = file_path(vm, ref_arg(args, 1));
        file_input_init(vm, receiver(args), &path)
    }),
    method("read", "()I", ACC_PUBLIC, file_input_read),
    method("read", "([B)I", ACC_PUBLIC, |vm, _, args| {
        let target = ref_arg(args, 1);
        let length = vm.array_length(target).unwrap_or(0) as i32;
        file_input_read_into(vm, receiver(args), target, 0, length)
    }),
    method("read", "([BII)I", ACC_PUBLIC, |vm, _, args| {
        let target = ref_arg(args, 1);
        let offset = crate::int_arg(args, 2);
        let count = crate::int_arg(args, 3);
        file_input_read_into(vm, receiver(args), target, offset, count)
    }),
    method("available", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let (length, position) = file_input_state(vm, this)?;
        int((length as i32 - position as i32).max(0))
    }),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("skip", "(J)J", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let (length, position) = file_input_state(vm, this)?;
        let requested = long_arg(args, 1).max(0) as usize;
        let next = (position + requested).min(length);
        write_file_position(vm, this, next);
        long((next - position) as i64)
    }),
];

pub(crate) const FILE_INPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/io/FileInputStream",
    Some("java/io/InputStream"),
    &[],
    ACC_PUBLIC,
    &[
        field("path", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("data", "[B", ACC_PRIVATE, None),
        field("position", "I", ACC_PRIVATE, None),
    ],
    &FILE_INPUT_STREAM_METHODS,
    None,
);

fn file_input_init(vm: &mut Vm<'_>, this: ObjectRef, path: &str) -> Result<Value, VmError> {
    let Some(bytes) = vm.host.filesystem().and_then(|fs| fs.read(path)) else {
        return Err(vm.throw_new("java/io/FileNotFoundException", Some(path)));
    };
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "path", "Ljava/lang/String;")?;
    let text = vm.make_string(path)?;
    vm.set_instance_ref(this, declaring, field, text);
    let data = vm.allocate_array_of(ArrayComponent::Byte, bytes.len())?;
    for (index, byte) in bytes.iter().enumerate() {
        vm.array_set(
            data,
            index as i32,
            Value::Int(i32::from(*byte as i8)),
            false,
        )?;
    }
    let (declaring, field) = vm.find_field(class, "data", "[B")?;
    vm.set_instance_ref(this, declaring, field, data);
    crate::lang::write_int_field(vm, this, "position", 0);
    void()
}

fn file_input_state(vm: &Vm<'_>, this: ObjectRef) -> Result<(usize, usize), VmError> {
    let class = vm.class_of(this);
    let data = vm
        .read_ref_field(this, class, "data", "[B")
        .unwrap_or(ObjectRef::NULL);
    let length = vm.array_length(data).unwrap_or(0);
    let position = crate::lang::read_int_field(vm, this, "position").max(0) as usize;
    Ok((length, position))
}

fn write_file_position(vm: &mut Vm<'_>, this: ObjectRef, position: usize) {
    let class = vm.class_of(this);
    if let Ok((declaring, field)) = vm.find_field(class, "position", "I") {
        vm.set_instance_int(this, declaring, field, position as i32);
    }
}

fn file_input_read(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let (length, position) = file_input_state(vm, this)?;
    if position >= length {
        return int(-1);
    }
    let class = vm.class_of(this);
    let data = vm
        .read_ref_field(this, class, "data", "[B")
        .unwrap_or(ObjectRef::NULL);
    let byte = vm.array_get(data, position as i32)?.as_int();
    write_file_position(vm, this, position + 1);
    int(byte & 0xFF)
}

fn file_input_read_into(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    target: ObjectRef,
    offset: i32,
    count: i32,
) -> Result<Value, VmError> {
    if target.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    if offset < 0 || count < 0 {
        return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
    }
    let target_length = vm.array_length(target)? as i32;
    if offset + count > target_length {
        return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
    }
    let (length, position) = file_input_state(vm, this)?;
    if position >= length {
        return int(-1);
    }
    let class = vm.class_of(this);
    let data = vm
        .read_ref_field(this, class, "data", "[B")
        .unwrap_or(ObjectRef::NULL);
    let available = (length - position).min(count as usize) as i32;
    for index in 0..available {
        let byte = vm
            .array_get(data, (position + index as usize) as i32)?
            .as_int();
        vm.array_set(target, offset + index, Value::Int(byte), false)?;
    }
    write_file_position(vm, this, position + available as usize);
    int(available)
}

// -------------------------------------------------------------------------------------------
// java.io.FileOutputStream
// -------------------------------------------------------------------------------------------

const FILE_OUTPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = string_arg(vm, args[1])?;
            file_output_init(vm, receiver(args), &path, false)
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Z)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = string_arg(vm, args[1])?;
            let append = crate::bool_arg(args, 2);
            file_output_init(vm, receiver(args), &path, append)
        },
    ),
    method("<init>", "(Ljava/io/File;)V", ACC_PUBLIC, |vm, _, args| {
        let path = file_path(vm, ref_arg(args, 1));
        file_output_init(vm, receiver(args), &path, false)
    }),
    method("<init>", "(Ljava/io/File;Z)V", ACC_PUBLIC, |vm, _, args| {
        let path = file_path(vm, ref_arg(args, 1));
        let append = crate::bool_arg(args, 2);
        file_output_init(vm, receiver(args), &path, append)
    }),
    method("write", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let byte = crate::int_arg(args, 1) as u8;
        file_output_write(vm, receiver(args), &[byte])
    }),
    method("write", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
        file_output_write(vm, receiver(args), &bytes)
    }),
    method("write", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
        let offset = crate::int_arg(args, 2);
        let count = crate::int_arg(args, 3);
        if offset < 0 || count < 0 || (offset + count) as usize > bytes.len() {
            return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
        }
        let slice = bytes[offset as usize..(offset + count) as usize].to_vec();
        file_output_write(vm, receiver(args), &slice)
    }),
    method("flush", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
];

pub(crate) const FILE_OUTPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/io/FileOutputStream",
    Some("java/io/OutputStream"),
    &[],
    ACC_PUBLIC,
    &[
        field("path", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("append", "Z", ACC_PRIVATE, None),
        field("opened", "Z", ACC_PRIVATE, None),
    ],
    &FILE_OUTPUT_STREAM_METHODS,
    None,
);

fn file_output_init(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    path: &str,
    append: bool,
) -> Result<Value, VmError> {
    if fs_metadata(vm, path).is_some_and(|metadata| metadata.kind == FileKind::Directory) {
        return Err(vm.throw_new(
            "java/io/FileNotFoundException",
            Some(&alloc::format!("{path} (Is a directory)")),
        ));
    }
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "path", "Ljava/lang/String;")?;
    let text = vm.make_string(path)?;
    vm.set_instance_ref(this, declaring, field, text);
    write_z_field(vm, this, "append", append);
    write_z_field(vm, this, "opened", false);
    void()
}

fn write_z_field(vm: &mut Vm<'_>, this: ObjectRef, name: &str, value: bool) {
    let class = vm.class_of(this);
    if let Ok((declaring, field)) = vm.find_field(class, name, "Z") {
        vm.set_instance_int(this, declaring, field, i32::from(value));
    }
}

fn read_z_field(vm: &Vm<'_>, this: ObjectRef, name: &str) -> bool {
    let class = vm.class_of(this);
    vm.read_instance_int_named(this, class, name, "Z")
        .unwrap_or(0)
        != 0
}

fn file_output_write(vm: &mut Vm<'_>, this: ObjectRef, bytes: &[u8]) -> Result<Value, VmError> {
    let path = file_path(vm, this);
    let append = read_z_field(vm, this, "append") || read_z_field(vm, this, "opened");
    if !fs_write(vm, &path, bytes, append) {
        return Err(vm.throw_new(
            "java/io/IOException",
            Some(&alloc::format!("Cannot write to {path}")),
        ));
    }
    write_z_field(vm, this, "opened", true);
    void()
}
