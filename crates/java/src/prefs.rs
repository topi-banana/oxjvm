//! `java.util.prefs`: an in-memory preferences hierarchy.
//!
//! The JDK backs preferences with a platform-specific store; here the default
//! factory hands out an in-memory tree, which is enough for the API contract
//! (node traversal, typed accessors, change listeners and XML import/export).

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use oxjvm_classfile::flags::*;

use crate::{
    bool_arg, boolean, class, field, int, int_arg, long, long_arg, method, object, receiver,
    ref_arg, void,
};
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

const MAX_KEY_LENGTH: usize = 80;
const MAX_VALUE_LENGTH: usize = 8 * 1024;
const MAX_NAME_LENGTH: usize = 80;

// -------------------------------------------------------------------------------------------
// Helpers
// -------------------------------------------------------------------------------------------

fn prefs_set_ref(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: &str,
    desc: &str,
    value: ObjectRef,
) -> Result<(), VmError> {
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, name, desc)?;
    vm.set_instance_ref(this, declaring, field, value);
    Ok(())
}

fn prefs_get_ref(vm: &Vm<'_>, this: ObjectRef, name: &str, desc: &str) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), name, desc)
        .unwrap_or(ObjectRef::NULL)
}

fn prefs_set_string(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: &str,
    value: &str,
) -> Result<(), VmError> {
    let text = vm.make_string(value)?;
    prefs_set_ref(vm, this, name, "Ljava/lang/String;", text)
}

fn prefs_invoke(
    vm: &mut Vm<'_>,
    obj: ObjectRef,
    name: &str,
    descriptor: &str,
    extra: Vec<Value>,
) -> Result<Value, VmError> {
    let (class, method) = vm.resolve_virtual_method(obj, name, descriptor)?;
    let mut args = vec![Value::Ref(obj)];
    args.extend(extra);
    vm.invoke_method(class, method, args)
}

fn prefs_check_not_removed(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    if crate::lang::read_int_field(vm, this, "removed") != 0 {
        return Err(vm.throw_new(
            "java/lang/IllegalStateException",
            Some("Node has been removed."),
        ));
    }
    Ok(())
}

fn prefs_validate_key(vm: &mut Vm<'_>, key: &str) -> Result<(), VmError> {
    if key.contains('\u{0}') {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("Key contains code point U+0000"),
        ));
    }
    Ok(())
}

/// The static user/system root, created on first use.
fn prefs_root(vm: &mut Vm<'_>, user: bool) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/prefs/Preferences")?;
    vm.ensure_initialized(class)?;
    let name = if user { "userRoot" } else { "systemRoot" };
    if let Some(existing) = vm.read_static_ref_if_set(class, name, "Ljava/util/prefs/Preferences;")
    {
        if !existing.is_null() {
            return Ok(existing);
        }
    }
    let root = prefs_new_node(vm, ObjectRef::NULL, "")?;
    vm.set_static_value(
        class,
        name,
        "Ljava/util/prefs/Preferences;",
        Value::Ref(root),
    );
    Ok(root)
}

/// Construct an `InMemoryPreferences` node and run its constructor (which
/// chains to `AbstractPreferences` for parent/name validation and caches).
fn prefs_new_node(vm: &mut Vm<'_>, parent: ObjectRef, name: &str) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/prefs/InMemoryPreferences")?;
    let instance = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(
            class,
            "<init>",
            "(Ljava/util/prefs/AbstractPreferences;Ljava/lang/String;)V",
        )
        .ok_or_else(|| VmError::internal("InMemoryPreferences lacks its constructor"))?;
    let name_text = vm.make_string(name)?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![
            Value::Ref(instance),
            Value::Ref(parent),
            Value::Ref(name_text),
        ],
    )?;
    Ok(instance)
}

// -------------------------------------------------------------------------------------------
// Exceptions
// -------------------------------------------------------------------------------------------

const PREFS_EXCEPTION_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "<init>",
        "()V",
        ACC_PUBLIC,
        crate::throwable::exception_ctor,
    ),
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        crate::throwable::exception_ctor,
    ),
];

pub(crate) const BACKING_STORE_EXCEPTION: oxjvm_vm::NativeClass = class(
    "java/util/prefs/BackingStoreException",
    Some("java/lang/Exception"),
    &[],
    ACC_PUBLIC,
    &[],
    &PREFS_EXCEPTION_METHODS,
    None,
);

pub(crate) const INVALID_PREFERENCES_FORMAT_EXCEPTION: oxjvm_vm::NativeClass = class(
    "java/util/prefs/InvalidPreferencesFormatException",
    Some("java/lang/Exception"),
    &[],
    ACC_PUBLIC,
    &[],
    &PREFS_EXCEPTION_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Events and listeners
// -------------------------------------------------------------------------------------------

pub(crate) const NODE_CHANGE_LISTENER: oxjvm_vm::NativeClass = class(
    "java/util/prefs/NodeChangeListener",
    Some("java/lang/Object"),
    &["java/util/EventListener"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &NODE_CHANGE_LISTENER_METHODS,
    None,
);

const NODE_CHANGE_LISTENER_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "childAdded",
        "(Ljava/util/prefs/NodeChangeEvent;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "childRemoved",
        "(Ljava/util/prefs/NodeChangeEvent;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const PREFERENCE_CHANGE_LISTENER: oxjvm_vm::NativeClass = class(
    "java/util/prefs/PreferenceChangeListener",
    Some("java/lang/Object"),
    &["java/util/EventListener"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &PREFERENCE_CHANGE_LISTENER_METHODS,
    None,
);

const PREFERENCE_CHANGE_LISTENER_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "preferenceChange",
    "(Ljava/util/prefs/PreferenceChangeEvent;)V",
    ACC_PUBLIC | ACC_ABSTRACT,
    crate::lang::abstract_method_pub,
)];

pub(crate) const PREFERENCES_FACTORY: oxjvm_vm::NativeClass = class(
    "java/util/prefs/PreferencesFactory",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &PREFERENCES_FACTORY_METHODS,
    None,
);

const PREFERENCES_FACTORY_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "userRoot",
        "()Ljava/util/prefs/Preferences;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "systemRoot",
        "()Ljava/util/prefs/Preferences;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

fn prefs_event_init(vm: &mut Vm<'_>, this: ObjectRef, source: ObjectRef) -> Result<(), VmError> {
    let event_class = vm.resolve_class("java/util/EventObject")?;
    let (declaring, ctor) = vm
        .find_method(event_class, "<init>", "(Ljava/lang/Object;)V")
        .ok_or_else(|| VmError::internal("EventObject lacks its constructor"))?;
    vm.invoke_method(declaring, ctor, vec![Value::Ref(this), Value::Ref(source)])?;
    Ok(())
}

pub(crate) const NODE_CHANGE_EVENT: oxjvm_vm::NativeClass = class(
    "java/util/prefs/NodeChangeEvent",
    Some("java/util/EventObject"),
    &[],
    ACC_PUBLIC,
    &[field(
        "child",
        "Ljava/util/prefs/Preferences;",
        ACC_PRIVATE,
        None,
    )],
    &NODE_CHANGE_EVENT_METHODS,
    None,
);

const NODE_CHANGE_EVENT_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method(
        "<init>",
        "(Ljava/util/prefs/Preferences;Ljava/util/prefs/Preferences;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            prefs_event_init(vm, this, ref_arg(args, 1))?;
            prefs_set_ref(
                vm,
                this,
                "child",
                "Ljava/util/prefs/Preferences;",
                ref_arg(args, 2),
            )?;
            void()
        },
    ),
    method(
        "getChild",
        "()Ljava/util/prefs/Preferences;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(prefs_get_ref(
                vm,
                receiver(args),
                "child",
                "Ljava/util/prefs/Preferences;",
            ))
        },
    ),
    method(
        "getParent",
        "()Ljava/util/prefs/Preferences;",
        ACC_PUBLIC,
        |vm, _, args| {
            let source = prefs_get_ref(vm, receiver(args), "source", "Ljava/lang/Object;");
            object(source)
        },
    ),
];

pub(crate) const PREFERENCE_CHANGE_EVENT: oxjvm_vm::NativeClass = class(
    "java/util/prefs/PreferenceChangeEvent",
    Some("java/util/EventObject"),
    &[],
    ACC_PUBLIC,
    &[
        field("key", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("newValue", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &PREFERENCE_CHANGE_EVENT_METHODS,
    None,
);

const PREFERENCE_CHANGE_EVENT_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "<init>",
        "(Ljava/util/prefs/Preferences;Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            prefs_event_init(vm, this, ref_arg(args, 1))?;
            prefs_set_ref(vm, this, "key", "Ljava/lang/String;", ref_arg(args, 2))?;
            prefs_set_ref(vm, this, "newValue", "Ljava/lang/String;", ref_arg(args, 3))?;
            void()
        },
    ),
    method(
        "getNode",
        "()Ljava/util/prefs/Preferences;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(prefs_get_ref(
                vm,
                receiver(args),
                "source",
                "Ljava/lang/Object;",
            ))
        },
    ),
    method(
        "getKey",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(prefs_get_ref(
                vm,
                receiver(args),
                "key",
                "Ljava/lang/String;",
            ))
        },
    ),
    method(
        "getNewValue",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(prefs_get_ref(
                vm,
                receiver(args),
                "newValue",
                "Ljava/lang/String;",
            ))
        },
    ),
];
// -------------------------------------------------------------------------------------------
// java.util.prefs.Preferences
// -------------------------------------------------------------------------------------------

fn prefs_node_for_package(
    vm: &mut Vm<'_>,
    class_arg: ObjectRef,
    user: bool,
) -> Result<Value, VmError> {
    let root = prefs_root(vm, user)?;
    if class_arg.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let name = prefs_invoke(vm, class_arg, "getName", "()Ljava/lang/String;", vec![])?;
    let name = vm.string_value(name.as_ref()).unwrap_or_default();
    let package = match name.rfind('.') {
        Some(i) => String::from(&name[..i]).replace('.', "/"),
        None => String::new(),
    };
    let path = vm.make_string(&package)?;
    prefs_invoke(
        vm,
        root,
        "node",
        "(Ljava/lang/String;)Ljava/util/prefs/Preferences;",
        vec![Value::Ref(path)],
    )
}

const PREFERENCES_METHODS: [oxjvm_vm::NativeMethodDef; 38] = [
    method(
        "userRoot",
        "()Ljava/util/prefs/Preferences;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(prefs_root(vm, true)?),
    ),
    method(
        "systemRoot",
        "()Ljava/util/prefs/Preferences;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(prefs_root(vm, false)?),
    ),
    method(
        "userNodeForPackage",
        "(Ljava/lang/Class;)Ljava/util/prefs/Preferences;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| prefs_node_for_package(vm, ref_arg(args, 0), true),
    ),
    method(
        "systemNodeForPackage",
        "(Ljava/lang/Class;)Ljava/util/prefs/Preferences;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| prefs_node_for_package(vm, ref_arg(args, 0), false),
    ),
    method(
        "importPreferences",
        "(Ljava/io/InputStream;)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| prefs_import_xml(vm, ref_arg(args, 0)),
    ),
    method(
        "put",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "get",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "remove",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "clear",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "putInt",
        "(Ljava/lang/String;I)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getInt",
        "(Ljava/lang/String;I)I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "putLong",
        "(Ljava/lang/String;J)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getLong",
        "(Ljava/lang/String;J)J",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "putBoolean",
        "(Ljava/lang/String;Z)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getBoolean",
        "(Ljava/lang/String;Z)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "putFloat",
        "(Ljava/lang/String;F)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getFloat",
        "(Ljava/lang/String;F)F",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "putDouble",
        "(Ljava/lang/String;D)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getDouble",
        "(Ljava/lang/String;D)D",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "putByteArray",
        "(Ljava/lang/String;[B)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getByteArray",
        "(Ljava/lang/String;[B)[B",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "keys",
        "()[Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "childrenNames",
        "()[Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "parent",
        "()Ljava/util/prefs/Preferences;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "node",
        "(Ljava/lang/String;)Ljava/util/prefs/Preferences;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "nodeExists",
        "(Ljava/lang/String;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "removeNode",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "name",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "absolutePath",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "isUserNode",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "flush",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "sync",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "addNodeChangeListener",
        "(Ljava/util/prefs/NodeChangeListener;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "removeNodeChangeListener",
        "(Ljava/util/prefs/NodeChangeListener;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "addPreferenceChangeListener",
        "(Ljava/util/prefs/PreferenceChangeListener;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "removePreferenceChangeListener",
        "(Ljava/util/prefs/PreferenceChangeListener;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "exportNode",
        "(Ljava/io/OutputStream;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "exportSubtree",
        "(Ljava/io/OutputStream;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const PREFERENCES: oxjvm_vm::NativeClass = class(
    "java/util/prefs/Preferences",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[
        field(
            "userRoot",
            "Ljava/util/prefs/Preferences;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
        field(
            "systemRoot",
            "Ljava/util/prefs/Preferences;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
        field(
            "MAX_KEY_LENGTH",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MAX_VALUE_LENGTH",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MAX_NAME_LENGTH",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &PREFERENCES_METHODS,
    Some(prefs_clinit),
);

fn prefs_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/util/prefs/Preferences")?;
    vm.set_static_value(
        class,
        "MAX_KEY_LENGTH",
        "I",
        Value::Int(MAX_KEY_LENGTH as i32),
    );
    vm.set_static_value(
        class,
        "MAX_VALUE_LENGTH",
        "I",
        Value::Int(MAX_VALUE_LENGTH as i32),
    );
    vm.set_static_value(
        class,
        "MAX_NAME_LENGTH",
        "I",
        Value::Int(MAX_NAME_LENGTH as i32),
    );
    Ok(())
}

// -------------------------------------------------------------------------------------------
// java.util.prefs.AbstractPreferences
// -------------------------------------------------------------------------------------------

fn prefs_collection_elements(vm: &mut Vm<'_>, coll: ObjectRef) -> Result<Vec<ObjectRef>, VmError> {
    let it = prefs_invoke(vm, coll, "iterator", "()Ljava/util/Iterator;", vec![])?;
    let it = it.as_ref();
    let mut out = Vec::new();
    loop {
        let has_next = prefs_invoke(vm, it, "hasNext", "()Z", vec![])?;
        if !has_next.as_boolean() {
            break;
        }
        let next = prefs_invoke(vm, it, "next", "()Ljava/lang/Object;", vec![])?;
        out.push(next.as_ref());
    }
    Ok(out)
}

fn prefs_kid_cache(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    prefs_get_ref(vm, this, "kidCache", "Ljava/util/HashMap;")
}

fn prefs_child(vm: &mut Vm<'_>, parent: ObjectRef, name: &str) -> Result<ObjectRef, VmError> {
    if name.len() > MAX_NAME_LENGTH {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some(&alloc::format!("Node name {name} too long")),
        ));
    }
    let cache = prefs_kid_cache(vm, parent);
    let key = vm.make_string(name)?;
    let cached = prefs_invoke(
        vm,
        cache,
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        vec![Value::Ref(key)],
    )?;
    if !cached.as_ref().is_null() {
        return Ok(cached.as_ref());
    }
    let child = prefs_invoke(
        vm,
        parent,
        "childSpi",
        "(Ljava/lang/String;)Ljava/util/prefs/AbstractPreferences;",
        vec![Value::Ref(key)],
    )?;
    let child = child.as_ref();
    if crate::lang::read_int_field(vm, child, "newNode") != 0 {
        prefs_enqueue_node_event(vm, parent, child, true)?;
    }
    prefs_invoke(
        vm,
        cache,
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        vec![Value::Ref(key), Value::Ref(child)],
    )?;
    Ok(child)
}

fn prefs_node(vm: &mut Vm<'_>, this: ObjectRef, path: &str) -> Result<ObjectRef, VmError> {
    prefs_check_not_removed(vm, this)?;
    if path.is_empty() {
        return Ok(this);
    }
    let root = prefs_get_ref(vm, this, "root", "Ljava/util/prefs/AbstractPreferences;");
    if path == "/" {
        return Ok(root);
    }
    let (start, rest) = if let Some(stripped) = path.strip_prefix('/') {
        (root, stripped)
    } else {
        (this, path)
    };
    let mut current = start;
    for token in rest.split('/') {
        if token.is_empty() {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("Invalid path (consecutive or trailing slashes)"),
            ));
        }
        current = prefs_child(vm, current, token)?;
    }
    Ok(current)
}

fn prefs_node_exists(vm: &mut Vm<'_>, this: ObjectRef, path: &str) -> Result<bool, VmError> {
    if path.is_empty() {
        return Ok(crate::lang::read_int_field(vm, this, "removed") == 0);
    }
    prefs_check_not_removed(vm, this)?;
    if path == "/" {
        return Ok(true);
    }
    let root = prefs_get_ref(vm, this, "root", "Ljava/util/prefs/AbstractPreferences;");
    let (start, rest) = if let Some(stripped) = path.strip_prefix('/') {
        (root, stripped)
    } else {
        (this, path)
    };
    let mut current = start;
    for token in rest.split('/') {
        if token.is_empty() {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("Invalid path (consecutive or trailing slashes)"),
            ));
        }
        let cache = prefs_kid_cache(vm, current);
        let key = vm.make_string(token)?;
        let cached = prefs_invoke(
            vm,
            cache,
            "get",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            vec![Value::Ref(key)],
        )?;
        let child = if !cached.as_ref().is_null() {
            cached.as_ref()
        } else {
            let got = prefs_invoke(
                vm,
                current,
                "getChild",
                "(Ljava/lang/String;)Ljava/util/prefs/AbstractPreferences;",
                vec![Value::Ref(key)],
            )?;
            got.as_ref()
        };
        if child.is_null() {
            return Ok(false);
        }
        current = child;
    }
    Ok(true)
}

fn prefs_make_node_event(
    vm: &mut Vm<'_>,
    parent: ObjectRef,
    child: ObjectRef,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/prefs/NodeChangeEvent")?;
    let event = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(
            class,
            "<init>",
            "(Ljava/util/prefs/Preferences;Ljava/util/prefs/Preferences;)V",
        )
        .ok_or_else(|| VmError::internal("NodeChangeEvent ctor"))?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![Value::Ref(event), Value::Ref(parent), Value::Ref(child)],
    )?;
    Ok(event)
}

fn prefs_enqueue_node_event(
    vm: &mut Vm<'_>,
    parent: ObjectRef,
    child: ObjectRef,
    added: bool,
) -> Result<(), VmError> {
    let listeners = prefs_get_ref(vm, parent, "nodeListeners", "Ljava/util/List;");
    let size = prefs_invoke(vm, listeners, "size", "()I", vec![])?.as_int();
    if size == 0 {
        return Ok(());
    }
    let event = prefs_make_node_event(vm, parent, child)?;
    let method = if added { "childAdded" } else { "childRemoved" };
    for i in 0..size {
        let listener = prefs_invoke(
            vm,
            listeners,
            "get",
            "(I)Ljava/lang/Object;",
            vec![Value::Int(i)],
        )?
        .as_ref();
        prefs_invoke(
            vm,
            listener,
            method,
            "(Ljava/util/prefs/NodeChangeEvent;)V",
            vec![Value::Ref(event)],
        )?;
    }
    Ok(())
}

fn prefs_enqueue_pref_event(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    key: ObjectRef,
    new_value: ObjectRef,
) -> Result<(), VmError> {
    let listeners = prefs_get_ref(vm, this, "prefListeners", "Ljava/util/List;");
    let size = prefs_invoke(vm, listeners, "size", "()I", vec![])?.as_int();
    if size == 0 {
        return Ok(());
    }
    let class = vm.resolve_class("java/util/prefs/PreferenceChangeEvent")?;
    let event = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(
            class,
            "<init>",
            "(Ljava/util/prefs/Preferences;Ljava/lang/String;Ljava/lang/String;)V",
        )
        .ok_or_else(|| VmError::internal("PreferenceChangeEvent ctor"))?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![
            Value::Ref(event),
            Value::Ref(this),
            Value::Ref(key),
            Value::Ref(new_value),
        ],
    )?;
    for i in 0..size {
        let listener = prefs_invoke(
            vm,
            listeners,
            "get",
            "(I)Ljava/lang/Object;",
            vec![Value::Int(i)],
        )?
        .as_ref();
        prefs_invoke(
            vm,
            listener,
            "preferenceChange",
            "(Ljava/util/prefs/PreferenceChangeEvent;)V",
            vec![Value::Ref(event)],
        )?;
    }
    Ok(())
}

fn prefs_remove_node2(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    prefs_check_not_removed(vm, this)?;
    let cache = prefs_kid_cache(vm, this);
    let children = {
        let values = prefs_invoke(vm, cache, "values", "()Ljava/util/Collection;", vec![])?;
        prefs_collection_elements(vm, values.as_ref())?
    };
    for child in children {
        prefs_remove_node2(vm, child)?;
    }
    prefs_invoke(vm, cache, "clear", "()V", vec![])?;
    prefs_invoke(vm, this, "removeNodeSpi", "()V", vec![])?;
    crate::lang::write_int_field(vm, this, "removed", 1);
    let parent = prefs_get_ref(vm, this, "parent", "Ljava/util/prefs/AbstractPreferences;");
    if !parent.is_null() {
        prefs_enqueue_node_event(vm, parent, this, false)?;
    }
    Ok(())
}

fn prefs_flush2(vm: &mut Vm<'_>, this: ObjectRef, sync: bool) -> Result<(), VmError> {
    let spi = if sync { "syncSpi" } else { "flushSpi" };
    prefs_invoke(vm, this, spi, "()V", vec![])?;
    if crate::lang::read_int_field(vm, this, "removed") != 0 {
        return Ok(());
    }
    let cache = prefs_kid_cache(vm, this);
    let children = {
        let values = prefs_invoke(vm, cache, "values", "()Ljava/util/Collection;", vec![])?;
        prefs_collection_elements(vm, values.as_ref())?
    };
    for child in children {
        prefs_flush2(vm, child, sync)?;
    }
    Ok(())
}

fn prefs_sorted_string_array(vm: &mut Vm<'_>, mut names: Vec<String>) -> Result<Value, VmError> {
    names.sort();
    names.dedup();
    let string_class = vm.resolve_class("java/lang/String")?;
    let array = vm.allocate_object_array(string_class, names.len())?;
    for (i, name) in names.iter().enumerate() {
        let text = vm.make_string(name)?;
        vm.array_set_ref(array, i, text)?;
    }
    object(array)
}

fn prefs_put_typed(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    key: ObjectRef,
    text: &str,
) -> Result<Value, VmError> {
    let value = vm.make_string(text)?;
    prefs_invoke(
        vm,
        this,
        "put",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        vec![Value::Ref(key), Value::Ref(value)],
    )?;
    void()
}

fn prefs_get_string(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    key: ObjectRef,
) -> Result<Option<String>, VmError> {
    let result = prefs_invoke(
        vm,
        this,
        "get",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
        vec![Value::Ref(key), Value::Ref(ObjectRef::NULL)],
    )?;
    Ok(vm.string_value(result.as_ref()))
}

fn prefs_abstract_ctor(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    parent: ObjectRef,
    name: &str,
) -> Result<(), VmError> {
    if parent.is_null() {
        if !name.is_empty() {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("Root name must be \"\""),
            ));
        }
        prefs_set_string(vm, this, "absolutePath", "/")?;
        prefs_set_ref(
            vm,
            this,
            "root",
            "Ljava/util/prefs/AbstractPreferences;",
            this,
        )?;
    } else {
        if name.contains('/') {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("Name contains '/'"),
            ));
        }
        if name.is_empty() {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("Illegal name: empty string"),
            ));
        }
        let root = prefs_get_ref(vm, parent, "root", "Ljava/util/prefs/AbstractPreferences;");
        prefs_set_ref(
            vm,
            this,
            "root",
            "Ljava/util/prefs/AbstractPreferences;",
            root,
        )?;
        let parent_path = vm
            .read_string_field_any(parent, "absolutePath")
            .unwrap_or_default();
        let path = if parent == root {
            alloc::format!("/{name}")
        } else {
            alloc::format!("{parent_path}/{name}")
        };
        prefs_set_string(vm, this, "absolutePath", &path)?;
    }
    prefs_set_string(vm, this, "name", name)?;
    prefs_set_ref(
        vm,
        this,
        "parent",
        "Ljava/util/prefs/AbstractPreferences;",
        parent,
    )?;
    let kid_cache = crate::util::new_hash_map(vm)?;
    prefs_set_ref(vm, this, "kidCache", "Ljava/util/HashMap;", kid_cache)?;
    let node_listeners = crate::util::new_array_list(vm)?;
    prefs_set_ref(
        vm,
        this,
        "nodeListeners",
        "Ljava/util/List;",
        node_listeners,
    )?;
    let pref_listeners = crate::util::new_array_list(vm)?;
    prefs_set_ref(
        vm,
        this,
        "prefListeners",
        "Ljava/util/List;",
        pref_listeners,
    )?;
    Ok(())
}

fn prefs_listener_add(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    field: &str,
    listener: ObjectRef,
) -> Result<Value, VmError> {
    if listener.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let list = prefs_get_ref(vm, this, field, "Ljava/util/List;");
    let contains = prefs_invoke(
        vm,
        list,
        "contains",
        "(Ljava/lang/Object;)Z",
        vec![Value::Ref(listener)],
    )?;
    if contains.as_boolean() {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("Listener already registered"),
        ));
    }
    prefs_invoke(
        vm,
        list,
        "add",
        "(Ljava/lang/Object;)Z",
        vec![Value::Ref(listener)],
    )?;
    void()
}

fn prefs_listener_remove(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    field: &str,
    listener: ObjectRef,
) -> Result<Value, VmError> {
    if listener.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let list = prefs_get_ref(vm, this, field, "Ljava/util/List;");
    let removed = prefs_invoke(
        vm,
        list,
        "remove",
        "(Ljava/lang/Object;)Z",
        vec![Value::Ref(listener)],
    )?;
    if !removed.as_boolean() {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("Listener not registered"),
        ));
    }
    void()
}

const ABSTRACT_PREFERENCES_METHODS: [oxjvm_vm::NativeMethodDef; 46] = [
    method(
        "<init>",
        "(Ljava/util/prefs/AbstractPreferences;Ljava/lang/String;)V",
        ACC_PROTECTED,
        |vm, _, args| {
            let name = vm.string_value(ref_arg(args, 2)).unwrap_or_default();
            prefs_abstract_ctor(vm, receiver(args), ref_arg(args, 1), &name)?;
            void()
        },
    ),
    method(
        "put",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let key = vm.string_value(ref_arg(args, 1));
            let value = vm.string_value(ref_arg(args, 2));
            let (key, value) = match (key, value) {
                (Some(k), Some(v)) => (k, v),
                _ => return Err(vm.throw_new("java/lang/NullPointerException", None)),
            };
            prefs_check_not_removed(vm, this)?;
            prefs_validate_key(vm, &key)?;
            prefs_validate_key(vm, &value)?;
            if key.len() > MAX_KEY_LENGTH || value.len() > MAX_VALUE_LENGTH {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("Key or value too long"),
                ));
            }
            let old = prefs_get_string(vm, this, ref_arg(args, 1))?;
            prefs_invoke(
                vm,
                this,
                "putSpi",
                "(Ljava/lang/String;Ljava/lang/String;)V",
                vec![Value::Ref(ref_arg(args, 1)), Value::Ref(ref_arg(args, 2))],
            )?;
            if old.as_deref() != Some(value.as_str()) {
                prefs_enqueue_pref_event(vm, this, ref_arg(args, 1), ref_arg(args, 2))?;
            }
            void()
        },
    ),
    method(
        "get",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let key = match vm.string_value(ref_arg(args, 1)) {
                Some(k) => k,
                None => return Err(vm.throw_new("java/lang/NullPointerException", None)),
            };
            prefs_check_not_removed(vm, this)?;
            prefs_validate_key(vm, &key)?;
            let result = prefs_invoke(
                vm,
                this,
                "getSpi",
                "(Ljava/lang/String;)Ljava/lang/String;",
                vec![Value::Ref(ref_arg(args, 1))],
            )?;
            if result.as_ref().is_null() {
                return object(ref_arg(args, 2));
            }
            object(result.as_ref())
        },
    ),
    method(
        "remove",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let key = match vm.string_value(ref_arg(args, 1)) {
                Some(k) => k,
                None => return Err(vm.throw_new("java/lang/NullPointerException", None)),
            };
            prefs_check_not_removed(vm, this)?;
            prefs_validate_key(vm, &key)?;
            let old = prefs_get_string(vm, this, ref_arg(args, 1))?;
            prefs_invoke(
                vm,
                this,
                "removeSpi",
                "(Ljava/lang/String;)V",
                vec![Value::Ref(ref_arg(args, 1))],
            )?;
            if old.is_some() {
                prefs_enqueue_pref_event(vm, this, ref_arg(args, 1), ObjectRef::NULL)?;
            }
            void()
        },
    ),
    method("clear", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        prefs_check_not_removed(vm, this)?;
        let keys = prefs_invoke(vm, this, "keysSpi", "()[Ljava/lang/String;", vec![])?;
        let keys = keys.as_ref();
        let len = vm.array_length(keys)?;
        for i in 0..len {
            let key = vm.array_get(keys, i as i32)?.as_ref();
            prefs_invoke(
                vm,
                this,
                "remove",
                "(Ljava/lang/String;)V",
                vec![Value::Ref(key)],
            )?;
        }
        void()
    }),
    method(
        "putInt",
        "(Ljava/lang/String;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = alloc::format!("{}", int_arg(args, 2));
            prefs_put_typed(vm, receiver(args), ref_arg(args, 1), &text)
        },
    ),
    method(
        "getInt",
        "(Ljava/lang/String;I)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = prefs_get_string(vm, receiver(args), ref_arg(args, 1))?;
            let parsed = value
                .and_then(|v| v.trim().parse::<i32>().ok())
                .unwrap_or(int_arg(args, 2));
            int(parsed)
        },
    ),
    method(
        "putLong",
        "(Ljava/lang/String;J)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = alloc::format!("{}", long_arg(args, 2));
            prefs_put_typed(vm, receiver(args), ref_arg(args, 1), &text)
        },
    ),
    method(
        "getLong",
        "(Ljava/lang/String;J)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = prefs_get_string(vm, receiver(args), ref_arg(args, 1))?;
            let parsed = value
                .and_then(|v| v.trim().parse::<i64>().ok())
                .unwrap_or(long_arg(args, 2));
            long(parsed)
        },
    ),
    method(
        "putBoolean",
        "(Ljava/lang/String;Z)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = if bool_arg(args, 2) { "true" } else { "false" };
            prefs_put_typed(vm, receiver(args), ref_arg(args, 1), text)
        },
    ),
    method(
        "getBoolean",
        "(Ljava/lang/String;Z)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = prefs_get_string(vm, receiver(args), ref_arg(args, 1))?;
            let parsed = match value.as_deref().map(str::trim) {
                Some("true") => true,
                Some("false") => false,
                _ => bool_arg(args, 2),
            };
            boolean(parsed)
        },
    ),
    method(
        "putFloat",
        "(Ljava/lang/String;F)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let bits = int_arg(args, 2);
            let text = alloc::format!("{}", f32::from_bits(bits as u32));
            prefs_put_typed(vm, receiver(args), ref_arg(args, 1), &text)
        },
    ),
    method(
        "getFloat",
        "(Ljava/lang/String;F)F",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = prefs_get_string(vm, receiver(args), ref_arg(args, 1))?;
            let parsed = value
                .and_then(|v| v.trim().parse::<f32>().ok())
                .unwrap_or(f32::from_bits(int_arg(args, 2) as u32));
            int(parsed.to_bits() as i32)
        },
    ),
    method(
        "putDouble",
        "(Ljava/lang/String;D)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let bits = long_arg(args, 2);
            let text = alloc::format!("{}", f64::from_bits(bits as u64));
            prefs_put_typed(vm, receiver(args), ref_arg(args, 1), &text)
        },
    ),
    method(
        "getDouble",
        "(Ljava/lang/String;D)D",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = prefs_get_string(vm, receiver(args), ref_arg(args, 1))?;
            let parsed = value
                .and_then(|v| v.trim().parse::<f64>().ok())
                .unwrap_or(f64::from_bits(long_arg(args, 2) as u64));
            long(parsed.to_bits() as i64)
        },
    ),
    method(
        "putByteArray",
        "(Ljava/lang/String;[B)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let bytes = crate::lang::array_bytes(vm, ref_arg(args, 2))?;
            let encoded = prefs_base64_encode(&bytes);
            prefs_put_typed(vm, receiver(args), ref_arg(args, 1), &encoded)
        },
    ),
    method(
        "getByteArray",
        "(Ljava/lang/String;[B)[B",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = prefs_get_string(vm, receiver(args), ref_arg(args, 1))?;
            if let Some(text) = value {
                if let Some(bytes) = prefs_base64_decode(&text) {
                    let array = crate::lang::byte_array_from(vm, &bytes)?;
                    return object(array);
                }
            }
            object(ref_arg(args, 2))
        },
    ),
    method(
        "keys",
        "()[Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            prefs_check_not_removed(vm, this)?;
            prefs_invoke(vm, this, "keysSpi", "()[Ljava/lang/String;", vec![])
        },
    ),
    method(
        "childrenNames",
        "()[Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            prefs_check_not_removed(vm, this)?;
            let spi = prefs_invoke(
                vm,
                this,
                "childrenNamesSpi",
                "()[Ljava/lang/String;",
                vec![],
            )?;
            let spi = spi.as_ref();
            let spi_len = vm.array_length(spi)?;
            let mut names = Vec::new();
            for i in 0..spi_len {
                let element = vm.array_get(spi, i as i32)?;
                if let Some(name) = vm.string_value(element.as_ref()) {
                    names.push(name);
                }
            }
            let cache = prefs_kid_cache(vm, this);
            let key_set = prefs_invoke(vm, cache, "keySet", "()Ljava/util/Set;", vec![])?;
            for key in prefs_collection_elements(vm, key_set.as_ref())? {
                if let Some(name) = vm.string_value(key) {
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
            prefs_sorted_string_array(vm, names)
        },
    ),
    method(
        "parent",
        "()Ljava/util/prefs/Preferences;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            prefs_check_not_removed(vm, this)?;
            object(prefs_get_ref(
                vm,
                this,
                "parent",
                "Ljava/util/prefs/AbstractPreferences;",
            ))
        },
    ),
    method(
        "node",
        "(Ljava/lang/String;)Ljava/util/prefs/Preferences;",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = match vm.string_value(ref_arg(args, 1)) {
                Some(p) => p,
                None => return Err(vm.throw_new("java/lang/NullPointerException", None)),
            };
            object(prefs_node(vm, receiver(args), &path)?)
        },
    ),
    method(
        "nodeExists",
        "(Ljava/lang/String;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = match vm.string_value(ref_arg(args, 1)) {
                Some(p) => p,
                None => return Err(vm.throw_new("java/lang/NullPointerException", None)),
            };
            boolean(prefs_node_exists(vm, receiver(args), &path)?)
        },
    ),
    method("removeNode", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let root = prefs_get_ref(vm, this, "root", "Ljava/util/prefs/AbstractPreferences;");
        if this == root {
            return Err(vm.throw_new(
                "java/lang/UnsupportedOperationException",
                Some("Can't remove the root!"),
            ));
        }
        prefs_remove_node2(vm, this)?;
        let parent = prefs_get_ref(vm, this, "parent", "Ljava/util/prefs/AbstractPreferences;");
        let name = prefs_get_ref(vm, this, "name", "Ljava/lang/String;");
        let cache = prefs_kid_cache(vm, parent);
        prefs_invoke(
            vm,
            cache,
            "remove",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            vec![Value::Ref(name)],
        )?;
        void()
    }),
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        object(prefs_get_ref(
            vm,
            receiver(args),
            "name",
            "Ljava/lang/String;",
        ))
    }),
    method(
        "absolutePath",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(prefs_get_ref(
                vm,
                receiver(args),
                "absolutePath",
                "Ljava/lang/String;",
            ))
        },
    ),
    method("isUserNode", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let root = prefs_get_ref(vm, this, "root", "Ljava/util/prefs/AbstractPreferences;");
        let user_root = prefs_root(vm, true)?;
        boolean(root == user_root)
    }),
    method("flush", "()V", ACC_PUBLIC, |vm, _, args| {
        prefs_flush2(vm, receiver(args), false)?;
        void()
    }),
    method("sync", "()V", ACC_PUBLIC, |vm, _, args| {
        prefs_flush2(vm, receiver(args), true)?;
        void()
    }),
    method(
        "addNodeChangeListener",
        "(Ljava/util/prefs/NodeChangeListener;)V",
        ACC_PUBLIC,
        |vm, _, args| prefs_listener_add(vm, receiver(args), "nodeListeners", ref_arg(args, 1)),
    ),
    method(
        "removeNodeChangeListener",
        "(Ljava/util/prefs/NodeChangeListener;)V",
        ACC_PUBLIC,
        |vm, _, args| prefs_listener_remove(vm, receiver(args), "nodeListeners", ref_arg(args, 1)),
    ),
    method(
        "addPreferenceChangeListener",
        "(Ljava/util/prefs/PreferenceChangeListener;)V",
        ACC_PUBLIC,
        |vm, _, args| prefs_listener_add(vm, receiver(args), "prefListeners", ref_arg(args, 1)),
    ),
    method(
        "removePreferenceChangeListener",
        "(Ljava/util/prefs/PreferenceChangeListener;)V",
        ACC_PUBLIC,
        |vm, _, args| prefs_listener_remove(vm, receiver(args), "prefListeners", ref_arg(args, 1)),
    ),
    method(
        "exportNode",
        "(Ljava/io/OutputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            prefs_export(vm, receiver(args), ref_arg(args, 1), false)?;
            void()
        },
    ),
    method(
        "exportSubtree",
        "(Ljava/io/OutputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            prefs_export(vm, receiver(args), ref_arg(args, 1), true)?;
            void()
        },
    ),
    method(
        "putSpi",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PROTECTED | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getSpi",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PROTECTED | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "removeSpi",
        "(Ljava/lang/String;)V",
        ACC_PROTECTED | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "childSpi",
        "(Ljava/lang/String;)Ljava/util/prefs/AbstractPreferences;",
        ACC_PROTECTED | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "keysSpi",
        "()[Ljava/lang/String;",
        ACC_PROTECTED | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "childrenNamesSpi",
        "()[Ljava/lang/String;",
        ACC_PROTECTED | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "removeNodeSpi",
        "()V",
        ACC_PROTECTED | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "flushSpi",
        "()V",
        ACC_PROTECTED | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "syncSpi",
        "()V",
        ACC_PROTECTED | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getChild",
        "(Ljava/lang/String;)Ljava/util/prefs/AbstractPreferences;",
        ACC_PROTECTED,
        |vm, _, args| {
            let cache = prefs_kid_cache(vm, receiver(args));
            prefs_invoke(
                vm,
                cache,
                "get",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                vec![Value::Ref(ref_arg(args, 1))],
            )
        },
    ),
    method("isRemoved", "()Z", ACC_PROTECTED, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "removed") != 0)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let path = vm
                .read_string_field_any(this, "absolutePath")
                .unwrap_or_default();
            let root = prefs_get_ref(vm, this, "root", "Ljava/util/prefs/AbstractPreferences;");
            let user_root = prefs_root(vm, true)?;
            let kind = if root == user_root { "User" } else { "System" };
            let text = alloc::format!("{kind} Preference Node: {path}");
            let s = vm.make_string(&text)?;
            object(s)
        },
    ),
];

pub(crate) const ABSTRACT_PREFERENCES: oxjvm_vm::NativeClass = class(
    "java/util/prefs/AbstractPreferences",
    Some("java/util/prefs/Preferences"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[
        field(
            "parent",
            "Ljava/util/prefs/AbstractPreferences;",
            ACC_PROTECTED,
            None,
        ),
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("absolutePath", "Ljava/lang/String;", ACC_PRIVATE, None),
        field(
            "root",
            "Ljava/util/prefs/AbstractPreferences;",
            ACC_PRIVATE,
            None,
        ),
        field("removed", "I", ACC_PRIVATE, None),
        field("newNode", "I", ACC_PRIVATE, None),
        field("kidCache", "Ljava/util/HashMap;", ACC_PRIVATE, None),
        field("nodeListeners", "Ljava/util/List;", ACC_PRIVATE, None),
        field("prefListeners", "Ljava/util/List;", ACC_PRIVATE, None),
    ],
    &ABSTRACT_PREFERENCES_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Base64 (byte-array preferences)
// -------------------------------------------------------------------------------------------

const BASE64_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn prefs_base64_encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(BASE64_ALPHABET[(triple >> 18) as usize & 0x3F] as char);
        out.push(BASE64_ALPHABET[(triple >> 12) as usize & 0x3F] as char);
        if chunk.len() > 1 {
            out.push(BASE64_ALPHABET[(triple >> 6) as usize & 0x3F] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(BASE64_ALPHABET[triple as usize & 0x3F] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn prefs_base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for ch in text.chars() {
        if ch == '=' || ch == '\n' || ch == '\r' {
            continue;
        }
        let value = BASE64_ALPHABET.iter().position(|&c| c as char == ch)? as u32;
        acc = (acc << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

// -------------------------------------------------------------------------------------------
// XML export / import
// -------------------------------------------------------------------------------------------

fn prefs_xml_escape(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

fn prefs_xml_unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn prefs_read_stream(vm: &mut Vm<'_>, stream: ObjectRef) -> Result<Vec<u8>, VmError> {
    let mut bytes = Vec::new();
    if stream.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let (class, method) = vm.resolve_virtual_method(stream, "read", "()I")?;
    loop {
        let value = vm
            .invoke_method(class, method, vec![Value::Ref(stream)])?
            .as_int();
        if value < 0 {
            break;
        }
        bytes.push((value & 0xFF) as u8);
    }
    Ok(bytes)
}

fn prefs_write_stream(vm: &mut Vm<'_>, stream: ObjectRef, bytes: &[u8]) -> Result<(), VmError> {
    if stream.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let (class, method) = vm.resolve_virtual_method(stream, "write", "([B)V")?;
    let array = crate::lang::byte_array_from(vm, bytes)?;
    vm.invoke_method(class, method, vec![Value::Ref(stream), Value::Ref(array)])?;
    Ok(())
}

/// Emit one node's `<map>` (and, for a subtree, its child `<node>` elements)
/// using the JDK's two-space indentation.
fn prefs_export_node_xml(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    indent: usize,
    subtree: bool,
    out: &mut String,
) -> Result<(), VmError> {
    let pad = " ".repeat(indent);
    let keys = prefs_invoke(vm, this, "keys", "()[Ljava/lang/String;", vec![])?;
    let keys = keys.as_ref();
    let len = vm.array_length(keys)?;
    if len == 0 {
        out.push_str(&alloc::format!("{pad}<map/>\n"));
    } else {
        out.push_str(&alloc::format!("{pad}<map>\n"));
        for i in 0..len {
            let key = vm.array_get(keys, i as i32)?.as_ref();
            let key_text = vm.string_value(key).unwrap_or_default();
            let value = prefs_invoke(
                vm,
                this,
                "get",
                "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                vec![Value::Ref(key), Value::Ref(ObjectRef::NULL)],
            )?;
            let value_text = vm.string_value(value.as_ref()).unwrap_or_default();
            out.push_str(&alloc::format!(
                "{pad}  <entry key=\"{}\" value=\"{}\"/>\n",
                prefs_xml_escape(&key_text),
                prefs_xml_escape(&value_text)
            ));
        }
        out.push_str(&alloc::format!("{pad}</map>\n"));
    }
    if subtree {
        let names = prefs_invoke(vm, this, "childrenNames", "()[Ljava/lang/String;", vec![])?;
        let names = names.as_ref();
        let count = vm.array_length(names)?;
        for i in 0..count {
            let name = vm.array_get(names, i as i32)?.as_ref();
            let name_text = vm.string_value(name).unwrap_or_default();
            let child = prefs_invoke(
                vm,
                this,
                "node",
                "(Ljava/lang/String;)Ljava/util/prefs/Preferences;",
                vec![Value::Ref(name)],
            )?;
            out.push_str(&alloc::format!(
                "{pad}<node name=\"{}\">\n",
                prefs_xml_escape(&name_text)
            ));
            prefs_export_node_xml(vm, child.as_ref(), indent + 2, true, out)?;
            out.push_str(&alloc::format!("{pad}</node>\n"));
        }
    }
    Ok(())
}

fn prefs_export(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    stream: ObjectRef,
    subtree: bool,
) -> Result<(), VmError> {
    prefs_check_not_removed(vm, this)?;
    let root = prefs_get_ref(vm, this, "root", "Ljava/util/prefs/AbstractPreferences;");
    let user_root = prefs_root(vm, true)?;
    let kind = if root == user_root { "user" } else { "system" };
    // Bottom-up list of nodes from `this` to (excluding) the root.
    let mut ancestors: Vec<String> = Vec::new();
    let mut kid = this;
    loop {
        let dad = prefs_get_ref(vm, kid, "parent", "Ljava/util/prefs/AbstractPreferences;");
        if dad.is_null() {
            break;
        }
        ancestors.push(vm.read_string_field_any(kid, "name").unwrap_or_default());
        kid = dad;
    }
    ancestors.reverse();
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n");
    out.push_str("<!DOCTYPE preferences SYSTEM \"http://java.sun.com/dtd/preferences.dtd\">\n");
    out.push_str("<preferences EXTERNAL_XML_VERSION=\"1.0\">\n");
    out.push_str(&alloc::format!("  <root type=\"{kind}\">\n"));
    let mut indent = 4;
    for name in &ancestors {
        let pad = " ".repeat(indent);
        out.push_str(&alloc::format!("{pad}<map/>\n"));
        out.push_str(&alloc::format!(
            "{pad}<node name=\"{}\">\n",
            prefs_xml_escape(name)
        ));
        indent += 2;
    }
    prefs_export_node_xml(vm, this, indent, subtree, &mut out)?;
    for _ in &ancestors {
        indent -= 2;
        let pad = " ".repeat(indent);
        out.push_str(&alloc::format!("{pad}</node>\n"));
    }
    out.push_str("  </root>\n");
    out.push_str("</preferences>\n");
    prefs_write_stream(vm, stream, out.as_bytes())
}

/// A minimal XML element tree, enough for the preferences document format.
struct PrefsXmlElement {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<PrefsXmlElement>,
}

impl PrefsXmlElement {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// Skip whitespace, comments, processing instructions and the DOCTYPE, then
/// parse a single element (with its children) starting at `pos`.
fn prefs_parse_element(text: &str, pos: &mut usize) -> Option<PrefsXmlElement> {
    let bytes = text.as_bytes();
    loop {
        while *pos < bytes.len() && bytes[*pos].is_ascii_whitespace() {
            *pos += 1;
        }
        if *pos >= bytes.len() || bytes[*pos] != b'<' {
            return None;
        }
        if text[*pos..].starts_with("<!--") {
            let end = text[*pos..].find("-->")? + *pos + 3;
            *pos = end;
            continue;
        }
        if text[*pos..].starts_with("<?") {
            let end = text[*pos..].find("?>")? + *pos + 2;
            *pos = end;
            continue;
        }
        if text[*pos..].starts_with("<!") {
            let end = text[*pos..].find('>')? + *pos + 1;
            *pos = end;
            continue;
        }
        break;
    }
    // Opening tag.
    let tag_end = text[*pos..].find('>')? + *pos;
    let tag = &text[*pos + 1..tag_end];
    *pos = tag_end + 1;
    let self_closing = tag.ends_with('/');
    let tag = if self_closing {
        &tag[..tag.len() - 1]
    } else {
        tag
    };
    let mut parts = tag.splitn(2, char::is_whitespace);
    let name = String::from(parts.next()?.trim());
    let mut attrs = Vec::new();
    if let Some(attr_text) = parts.next() {
        let mut rest = attr_text.trim();
        while !rest.is_empty() {
            let eq = rest.find('=')?;
            let key = String::from(rest[..eq].trim());
            rest = rest[eq + 1..].trim_start();
            let quote = rest.chars().next()?;
            if quote != '"' && quote != '\'' {
                return None;
            }
            rest = &rest[1..];
            let end = rest.find(quote)?;
            let value = prefs_xml_unescape(&rest[..end]);
            attrs.push((key, value));
            rest = rest[end + 1..].trim_start();
        }
    }
    let mut children = Vec::new();
    if !self_closing {
        let close = alloc::format!("</{name}>");
        loop {
            // Skip text content and whitespace between child elements.
            while *pos < bytes.len() && bytes[*pos] != b'<' {
                *pos += 1;
            }
            if *pos >= bytes.len() {
                return None;
            }
            if text[*pos..].starts_with(&close) {
                *pos += close.len();
                break;
            }
            if text[*pos..].starts_with("<!--") {
                let end = text[*pos..].find("-->")? + *pos + 3;
                *pos = end;
                continue;
            }
            let child = prefs_parse_element(text, pos)?;
            children.push(child);
        }
    }
    Some(PrefsXmlElement {
        name,
        attrs,
        children,
    })
}

fn prefs_import_subtree(
    vm: &mut Vm<'_>,
    node: ObjectRef,
    xml: &PrefsXmlElement,
) -> Result<(), VmError> {
    if crate::lang::read_int_field(vm, node, "removed") != 0 {
        return Ok(());
    }
    // First child is the <map>; the rest are <node> elements.
    let mut kids = Vec::new();
    for child in &xml.children {
        if child.name == "map" {
            for entry in &child.children {
                if entry.name != "entry" {
                    continue;
                }
                let Some(key) = entry.attr("key") else {
                    continue;
                };
                let Some(value) = entry.attr("value") else {
                    continue;
                };
                let key_text = vm.make_string(key)?;
                let value_text = vm.make_string(value)?;
                prefs_invoke(
                    vm,
                    node,
                    "put",
                    "(Ljava/lang/String;Ljava/lang/String;)V",
                    vec![Value::Ref(key_text), Value::Ref(value_text)],
                )?;
            }
        } else if child.name == "node" {
            let Some(name) = child.attr("name") else {
                continue;
            };
            let name_text = vm.make_string(name)?;
            let kid = prefs_invoke(
                vm,
                node,
                "node",
                "(Ljava/lang/String;)Ljava/util/prefs/Preferences;",
                vec![Value::Ref(name_text)],
            )?;
            kids.push((kid.as_ref(), child));
        }
    }
    for (kid, child_xml) in kids {
        prefs_import_subtree(vm, kid, child_xml)?;
    }
    Ok(())
}

fn prefs_import_xml(vm: &mut Vm<'_>, stream: ObjectRef) -> Result<Value, VmError> {
    let bytes = prefs_read_stream(vm, stream)?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let mut pos = 0;
    let Some(root) = prefs_parse_element(&text, &mut pos) else {
        return Err(vm.throw_new(
            "java/util/prefs/InvalidPreferencesFormatException",
            Some("Invalid preferences XML"),
        ));
    };
    if root.name != "preferences" {
        return Err(vm.throw_new(
            "java/util/prefs/InvalidPreferencesFormatException",
            Some("Invalid preferences XML"),
        ));
    }
    let Some(xml_root) = root.children.iter().find(|c| c.name == "root") else {
        return Err(vm.throw_new(
            "java/util/prefs/InvalidPreferencesFormatException",
            Some("Invalid preferences XML"),
        ));
    };
    let user = xml_root.attr("type") == Some("user");
    let prefs_root = prefs_root(vm, user)?;
    prefs_import_subtree(vm, prefs_root, xml_root)?;
    void()
}

// -------------------------------------------------------------------------------------------
// java.util.prefs.InMemoryPreferences (default factory backing store)
// -------------------------------------------------------------------------------------------

fn prefs_map(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    prefs_get_ref(vm, this, "map", "Ljava/util/HashMap;")
}

fn prefs_map_call(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: &str,
    descriptor: &str,
    extra: Vec<Value>,
) -> Result<Value, VmError> {
    prefs_invoke(vm, prefs_map(vm, this), name, descriptor, extra)
}

const IN_MEMORY_PREFERENCES_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
    method(
        "<init>",
        "(Ljava/util/prefs/AbstractPreferences;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name = vm.string_value(ref_arg(args, 2)).unwrap_or_default();
            prefs_abstract_ctor(vm, this, ref_arg(args, 1), &name)?;
            let hash_map_class = vm.resolve_class("java/util/HashMap")?;
            let map = vm.new_instance(hash_map_class)?;
            let (declaring, ctor) = vm
                .find_method(hash_map_class, "<init>", "()V")
                .ok_or_else(|| VmError::internal("HashMap ctor"))?;
            vm.invoke_method(declaring, ctor, vec![Value::Ref(map)])?;
            prefs_set_ref(vm, this, "map", "Ljava/util/HashMap;", map)?;
            void()
        },
    ),
    method(
        "putSpi",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PROTECTED,
        |vm, _, args| {
            prefs_map_call(
                vm,
                receiver(args),
                "put",
                "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
                vec![Value::Ref(ref_arg(args, 1)), Value::Ref(ref_arg(args, 2))],
            )?;
            void()
        },
    ),
    method(
        "getSpi",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PROTECTED,
        |vm, _, args| {
            prefs_map_call(
                vm,
                receiver(args),
                "get",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                vec![Value::Ref(ref_arg(args, 1))],
            )
        },
    ),
    method(
        "removeSpi",
        "(Ljava/lang/String;)V",
        ACC_PROTECTED,
        |vm, _, args| {
            prefs_map_call(
                vm,
                receiver(args),
                "remove",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                vec![Value::Ref(ref_arg(args, 1))],
            )?;
            void()
        },
    ),
    method(
        "childSpi",
        "(Ljava/lang/String;)Ljava/util/prefs/AbstractPreferences;",
        ACC_PROTECTED,
        |vm, _, args| {
            let this = receiver(args);
            let name = vm.string_value(ref_arg(args, 1)).unwrap_or_default();
            let child = prefs_new_node(vm, this, &name)?;
            crate::lang::write_int_field(vm, child, "newNode", 1);
            object(child)
        },
    ),
    method(
        "keysSpi",
        "()[Ljava/lang/String;",
        ACC_PROTECTED,
        |vm, _, args| {
            let this = receiver(args);
            let key_set = prefs_map_call(vm, this, "keySet", "()Ljava/util/Set;", vec![])?;
            let keys = prefs_collection_elements(vm, key_set.as_ref())?;
            let mut names = Vec::new();
            for key in keys {
                if let Some(name) = vm.string_value(key) {
                    names.push(name);
                }
            }
            prefs_sorted_string_array(vm, names)
        },
    ),
    method(
        "childrenNamesSpi",
        "()[Ljava/lang/String;",
        ACC_PROTECTED,
        |vm, _, _| prefs_sorted_string_array(vm, Vec::new()),
    ),
    method("removeNodeSpi", "()V", ACC_PROTECTED, |vm, _, args| {
        prefs_map_call(vm, receiver(args), "clear", "()V", vec![])?;
        void()
    }),
    method("flushSpi", "()V", ACC_PROTECTED, |_, _, _| void()),
    method("syncSpi", "()V", ACC_PROTECTED, |_, _, _| void()),
    method("isUserNode", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let root = prefs_get_ref(vm, this, "root", "Ljava/util/prefs/AbstractPreferences;");
        let user_root = prefs_root(vm, true)?;
        boolean(root == user_root)
    }),
];

pub(crate) const IN_MEMORY_PREFERENCES: oxjvm_vm::NativeClass = class(
    "java/util/prefs/InMemoryPreferences",
    Some("java/util/prefs/AbstractPreferences"),
    &[],
    ACC_PUBLIC,
    &[field("map", "Ljava/util/HashMap;", ACC_PRIVATE, None)],
    &IN_MEMORY_PREFERENCES_METHODS,
    None,
);
