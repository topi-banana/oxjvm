//! `javax.naming` reference addresses and initial-context plumbing:
//! `RefAddr`, `StringRefAddr`, `BinaryRefAddr`, `Reference`, `NamingException`
//! and `InitialContext`. These are the plain data carriers and environment
//! handling used by JNDI; no directory/service-provider machinery.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use oxjvm_classfile::flags::*;

use crate::{
    bool_arg, boolean, class, field, int, int_arg, method, object, receiver, ref_arg, void,
};
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

// -------------------------------------------------------------------------------------------
// javax.naming.RefAddr
// -------------------------------------------------------------------------------------------

/// `javax.naming.RefAddr`: the abstract address carried by a `Reference`.
pub(crate) const REF_ADDR: oxjvm_vm::NativeClass = class(
    "javax/naming/RefAddr",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[field("addrType", "Ljava/lang/String;", ACC_PROTECTED, None)],
    &REF_ADDR_METHODS,
    None,
);

const REF_ADDR_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PROTECTED,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "addrType", "Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method(
        "getType",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let addr_type = vm
                .read_ref_field(this, vm.class_of(this), "addrType", "Ljava/lang/String;")
                .unwrap_or(ObjectRef::NULL);
            object(addr_type)
        },
    ),
    method(
        "getContent",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| ref_addr_equals(vm, receiver(args), ref_arg(args, 1)),
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        ref_addr_hash(vm, receiver(args))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| ref_addr_to_string(vm, receiver(args)),
    ),
];

/// Invoke `getContent()` on an address (dispatches to the concrete subclass).
fn ref_addr_content(vm: &mut Vm<'_>, this: ObjectRef) -> Result<ObjectRef, VmError> {
    let (class, method) = vm.resolve_virtual_method(this, "getContent", "()Ljava/lang/Object;")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(this)])?
        .as_ref())
}

/// The hash code of an object via its virtual `hashCode()`.
fn object_hash(vm: &mut Vm<'_>, reference: ObjectRef) -> Result<i32, VmError> {
    let (class, method) = vm.resolve_virtual_method(reference, "hashCode", "()I")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(reference)])?
        .as_int())
}

/// Read the `addrType` of an address as text (empty when null).
fn ref_addr_type(vm: &Vm<'_>, this: ObjectRef) -> String {
    vm.read_string_field(this, vm.class_of(this), "addrType")
        .unwrap_or_default()
}

fn ref_addr_equals(vm: &mut Vm<'_>, this: ObjectRef, obj: ObjectRef) -> Result<Value, VmError> {
    let ref_addr = vm.resolve_class("javax/naming/RefAddr")?;
    if obj.is_null() || !vm.is_instance(obj, ref_addr) {
        return boolean(false);
    }
    if ref_addr_type(vm, this) != ref_addr_type(vm, obj) {
        return boolean(false);
    }
    let this_content = ref_addr_content(vm, this)?;
    let that_content = ref_addr_content(vm, obj)?;
    if this_content == that_content {
        return boolean(true);
    }
    if this_content.is_null() {
        return boolean(false);
    }
    let (class, method) =
        vm.resolve_virtual_method(this_content, "equals", "(Ljava/lang/Object;)Z")?;
    let result = vm.invoke_method(
        class,
        method,
        vec![Value::Ref(this_content), Value::Ref(that_content)],
    )?;
    boolean(result.as_int() != 0)
}

fn ref_addr_hash(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Value, VmError> {
    let addr_type = vm
        .read_ref_field(this, vm.class_of(this), "addrType", "Ljava/lang/String;")
        .unwrap_or(ObjectRef::NULL);
    let type_hash = object_hash(vm, addr_type)?;
    let content = ref_addr_content(vm, this)?;
    let hash = if content.is_null() {
        type_hash
    } else {
        type_hash.wrapping_add(object_hash(vm, content)?)
    };
    int(hash)
}

fn ref_addr_to_string(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Value, VmError> {
    let addr_type = ref_addr_type(vm, this);
    let content = ref_addr_content(vm, this)?;
    let content_text = if content.is_null() {
        String::from("null")
    } else {
        let (class, method) =
            vm.resolve_virtual_method(content, "toString", "()Ljava/lang/String;")?;
        let result = vm.invoke_method(class, method, vec![Value::Ref(content)])?;
        vm.string_value(result.as_ref()).unwrap_or_default()
    };
    let text = alloc::format!("Type: {addr_type}\nContent: {content_text}\n");
    object(vm.make_string(&text)?)
}

// -------------------------------------------------------------------------------------------
// javax.naming.StringRefAddr
// -------------------------------------------------------------------------------------------

/// `javax.naming.StringRefAddr`: an address whose contents are a string.
pub(crate) const STRING_REF_ADDR: oxjvm_vm::NativeClass = class(
    "javax/naming/StringRefAddr",
    Some("javax/naming/RefAddr"),
    &[],
    ACC_PUBLIC,
    &[field("contents", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &STRING_REF_ADDR_METHODS,
    None,
);

const STRING_REF_ADDR_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "addrType", "Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            let (declaring, field) = vm.find_field(class, "contents", "Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 2));
            void()
        },
    ),
    method(
        "getContent",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let contents = vm
                .read_ref_field(this, vm.class_of(this), "contents", "Ljava/lang/String;")
                .unwrap_or(ObjectRef::NULL);
            object(contents)
        },
    ),
];

// -------------------------------------------------------------------------------------------
// javax.naming.BinaryRefAddr
// -------------------------------------------------------------------------------------------

/// `javax.naming.BinaryRefAddr`: an address whose contents are a byte array.
pub(crate) const BINARY_REF_ADDR: oxjvm_vm::NativeClass = class(
    "javax/naming/BinaryRefAddr",
    Some("javax/naming/RefAddr"),
    &[],
    ACC_PUBLIC,
    &[field("buf", "[B", ACC_PRIVATE, None)],
    &BINARY_REF_ADDR_METHODS,
    None,
);

const BINARY_REF_ADDR_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "<init>",
        "(Ljava/lang/String;[B)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let src = crate::lang::array_bytes(vm, ref_arg(args, 2))?;
            binary_ref_addr_store(vm, this, ref_arg(args, 1), &src)
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;[BII)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let src = crate::lang::array_bytes(vm, ref_arg(args, 2))?;
            let offset = int_arg(args, 3).max(0) as usize;
            let count = int_arg(args, 4).max(0) as usize;
            let end = (offset + count).min(src.len());
            let slice = if offset <= src.len() {
                &src[offset..end]
            } else {
                &[]
            };
            binary_ref_addr_store(vm, this, ref_arg(args, 1), slice)
        },
    ),
    method(
        "getContent",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let buf = vm
                .read_ref_field(this, vm.class_of(this), "buf", "[B")
                .unwrap_or(ObjectRef::NULL);
            object(buf)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| binary_ref_addr_equals(vm, receiver(args), ref_arg(args, 1)),
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let addr_type = vm
            .read_ref_field(this, vm.class_of(this), "addrType", "Ljava/lang/String;")
            .unwrap_or(ObjectRef::NULL);
        let mut hash = object_hash(vm, addr_type)?;
        let buf = vm
            .read_ref_field(this, vm.class_of(this), "buf", "[B")
            .unwrap_or(ObjectRef::NULL);
        for byte in crate::lang::array_bytes(vm, buf)? {
            hash = hash.wrapping_add(i32::from(byte as i8));
        }
        int(hash)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let addr_type = ref_addr_type(vm, this);
            let buf = vm
                .read_ref_field(this, vm.class_of(this), "buf", "[B")
                .unwrap_or(ObjectRef::NULL);
            let bytes = crate::lang::array_bytes(vm, buf)?;
            let mut text = alloc::format!("Address Type: {addr_type}\nAddressContents: ");
            for &byte in bytes.iter().take(32) {
                // Integer.toHexString(byte) sign-extends to int first.
                let hex = alloc::format!("{:x}", i32::from(byte as i8));
                text.push_str(&hex);
                text.push(' ');
            }
            if bytes.len() >= 32 {
                text.push_str(" ...\n");
            }
            object(vm.make_string(&text)?)
        },
    ),
];

fn binary_ref_addr_store(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    addr_type: ObjectRef,
    bytes: &[u8],
) -> Result<Value, VmError> {
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "addrType", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, addr_type);
    let buf = crate::lang::byte_array_from(vm, bytes)?;
    let (declaring, field) = vm.find_field(class, "buf", "[B")?;
    vm.set_instance_ref(this, declaring, field, buf);
    void()
}

fn binary_ref_addr_equals(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    obj: ObjectRef,
) -> Result<Value, VmError> {
    let binary = vm.resolve_class("javax/naming/BinaryRefAddr")?;
    if obj.is_null() || !vm.is_instance(obj, binary) {
        return boolean(false);
    }
    if ref_addr_type(vm, this) != ref_addr_type(vm, obj) {
        return boolean(false);
    }
    let this_buf = vm
        .read_ref_field(this, vm.class_of(this), "buf", "[B")
        .unwrap_or(ObjectRef::NULL);
    let that_buf = vm
        .read_ref_field(obj, vm.class_of(obj), "buf", "[B")
        .unwrap_or(ObjectRef::NULL);
    let this_bytes = crate::lang::array_bytes(vm, this_buf)?;
    let that_bytes = crate::lang::array_bytes(vm, that_buf)?;
    boolean(this_bytes == that_bytes)
}

// -------------------------------------------------------------------------------------------
// javax.naming.Reference
// -------------------------------------------------------------------------------------------

/// `javax.naming.Reference`: a class name plus an ordered list of addresses.
pub(crate) const REFERENCE: oxjvm_vm::NativeClass = class(
    "javax/naming/Reference",
    Some("java/lang/Object"),
    &["java/lang/Cloneable", "java/io/Serializable"],
    ACC_PUBLIC,
    &[
        field("className", "Ljava/lang/String;", ACC_PROTECTED, None),
        field("addrs", "Ljava/util/Vector;", ACC_PROTECTED, None),
        field("classFactory", "Ljava/lang/String;", ACC_PROTECTED, None),
        field(
            "classFactoryLocation",
            "Ljava/lang/String;",
            ACC_PROTECTED,
            None,
        ),
    ],
    &REFERENCE_METHODS,
    None,
);

const REFERENCE_METHODS: [oxjvm_vm::NativeMethodDef; 18] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            reference_init(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ObjectRef::NULL,
                ObjectRef::NULL,
                ObjectRef::NULL,
            )
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljavax/naming/RefAddr;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            reference_init(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                ObjectRef::NULL,
                ObjectRef::NULL,
            )
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            reference_init(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ObjectRef::NULL,
                ref_arg(args, 2),
                ref_arg(args, 3),
            )
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljavax/naming/RefAddr;Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            reference_init(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                ref_arg(args, 3),
                ref_arg(args, 4),
            )
        },
    ),
    method(
        "getClassName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| reference_string_field(vm, receiver(args), "className"),
    ),
    method(
        "getFactoryClassName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| reference_string_field(vm, receiver(args), "classFactory"),
    ),
    method(
        "getFactoryClassLocation",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| reference_string_field(vm, receiver(args), "classFactoryLocation"),
    ),
    method(
        "get",
        "(Ljava/lang/String;)Ljavax/naming/RefAddr;",
        ACC_PUBLIC,
        |vm, _, args| reference_get_by_type(vm, receiver(args), ref_arg(args, 1)),
    ),
    method(
        "get",
        "(I)Ljavax/naming/RefAddr;",
        ACC_PUBLIC,
        |vm, _, args| {
            reference_vector_call(
                vm,
                receiver(args),
                "elementAt",
                "(I)Ljava/lang/Object;",
                &[Value::Int(int_arg(args, 1))],
            )
        },
    ),
    method(
        "getAll",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC,
        |vm, _, args| {
            reference_vector_call(
                vm,
                receiver(args),
                "elements",
                "()Ljava/util/Enumeration;",
                &[],
            )
        },
    ),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        reference_vector_call(vm, receiver(args), "size", "()I", &[])
    }),
    method(
        "add",
        "(Ljavax/naming/RefAddr;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let addr = ref_arg(args, 1);
            reference_vector_call(
                vm,
                receiver(args),
                "addElement",
                "(Ljava/lang/Object;)V",
                &[Value::Ref(addr)],
            )?;
            void()
        },
    ),
    method(
        "add",
        "(ILjavax/naming/RefAddr;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let posn = int_arg(args, 1);
            let addr = ref_arg(args, 2);
            reference_vector_call(
                vm,
                receiver(args),
                "insertElementAt",
                "(Ljava/lang/Object;I)V",
                &[Value::Ref(addr), Value::Int(posn)],
            )?;
            void()
        },
    ),
    method(
        "remove",
        "(I)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let posn = int_arg(args, 1);
            let removed = reference_vector_call(
                vm,
                receiver(args),
                "elementAt",
                "(I)Ljava/lang/Object;",
                &[Value::Int(posn)],
            )?;
            reference_vector_call(
                vm,
                receiver(args),
                "removeElementAt",
                "(I)V",
                &[Value::Int(posn)],
            )?;
            object(removed.as_ref())
        },
    ),
    method("clear", "()V", ACC_PUBLIC, |vm, _, args| {
        reference_vector_call(vm, receiver(args), "clear", "()V", &[])?;
        void()
    }),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| reference_equals(vm, receiver(args), ref_arg(args, 1)),
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        reference_hash(vm, receiver(args))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| reference_to_string(vm, receiver(args)),
    ),
];

/// Create an empty `java.util.Vector`.
fn new_vector(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/Vector")?;
    let instance = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(class, "<init>", "()V")
        .ok_or_else(|| VmError::internal("Vector lacks its no-arg constructor"))?;
    vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
    Ok(instance)
}

#[allow(clippy::too_many_arguments)]
fn reference_init(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    class_name: ObjectRef,
    addr: ObjectRef,
    factory: ObjectRef,
    factory_location: ObjectRef,
) -> Result<Value, VmError> {
    let class = vm.class_of(this);
    let addrs = new_vector(vm)?;
    let (declaring, field) = vm.find_field(class, "className", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, class_name);
    let (declaring, field) = vm.find_field(class, "addrs", "Ljava/util/Vector;")?;
    vm.set_instance_ref(this, declaring, field, addrs);
    let (declaring, field) = vm.find_field(class, "classFactory", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, factory);
    let (declaring, field) = vm.find_field(class, "classFactoryLocation", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, factory_location);
    if !addr.is_null() {
        let (vector_class, method) =
            vm.resolve_virtual_method(addrs, "addElement", "(Ljava/lang/Object;)V")?;
        vm.invoke_method(
            vector_class,
            method,
            vec![Value::Ref(addrs), Value::Ref(addr)],
        )?;
    }
    void()
}

fn reference_string_field(vm: &mut Vm<'_>, this: ObjectRef, name: &str) -> Result<Value, VmError> {
    let reference = vm
        .read_ref_field(this, vm.class_of(this), name, "Ljava/lang/String;")
        .unwrap_or(ObjectRef::NULL);
    object(reference)
}

/// The address vector stored on a reference.
fn reference_addrs(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "addrs", "Ljava/util/Vector;")
        .unwrap_or(ObjectRef::NULL)
}

/// Invoke a method on the reference's address vector, returning the raw value.
fn reference_vector_call(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: &str,
    descriptor: &str,
    extra: &[Value],
) -> Result<Value, VmError> {
    let addrs = reference_addrs(vm, this);
    let (class, method) = vm.resolve_virtual_method(addrs, name, descriptor)?;
    let mut args = Vec::with_capacity(extra.len() + 1);
    args.push(Value::Ref(addrs));
    args.extend_from_slice(extra);
    vm.invoke_method(class, method, args)
}

fn reference_get_by_type(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    addr_type: ObjectRef,
) -> Result<Value, VmError> {
    let wanted = vm.string_value(addr_type).unwrap_or_default();
    let size = reference_vector_call(vm, this, "size", "()I", &[])?.as_int();
    for index in 0..size {
        let addr = reference_vector_call(
            vm,
            this,
            "elementAt",
            "(I)Ljava/lang/Object;",
            &[Value::Int(index)],
        )?
        .as_ref();
        let addr_type_ref = vm
            .read_ref_field(addr, vm.class_of(addr), "addrType", "Ljava/lang/String;")
            .unwrap_or(ObjectRef::NULL);
        if vm.string_value(addr_type_ref).unwrap_or_default() == wanted {
            return object(addr);
        }
    }
    object(ObjectRef::NULL)
}

fn reference_equals(vm: &mut Vm<'_>, this: ObjectRef, obj: ObjectRef) -> Result<Value, VmError> {
    let reference = vm.resolve_class("javax/naming/Reference")?;
    if obj.is_null() || !vm.is_instance(obj, reference) {
        return boolean(false);
    }
    let this_name = vm
        .read_string_field(this, vm.class_of(this), "className")
        .unwrap_or_default();
    let that_name = vm
        .read_string_field(obj, vm.class_of(obj), "className")
        .unwrap_or_default();
    if this_name != that_name {
        return boolean(false);
    }
    let this_size = reference_vector_call(vm, this, "size", "()I", &[])?.as_int();
    let that_size = reference_vector_call(vm, obj, "size", "()I", &[])?.as_int();
    if this_size != that_size {
        return boolean(false);
    }
    for index in 0..this_size {
        let this_addr = reference_vector_call(
            vm,
            this,
            "elementAt",
            "(I)Ljava/lang/Object;",
            &[Value::Int(index)],
        )?
        .as_ref();
        let that_addr = reference_vector_call(
            vm,
            obj,
            "elementAt",
            "(I)Ljava/lang/Object;",
            &[Value::Int(index)],
        )?
        .as_ref();
        let (class, method) =
            vm.resolve_virtual_method(this_addr, "equals", "(Ljava/lang/Object;)Z")?;
        let equal = vm.invoke_method(
            class,
            method,
            vec![Value::Ref(this_addr), Value::Ref(that_addr)],
        )?;
        if equal.as_int() == 0 {
            return boolean(false);
        }
    }
    boolean(true)
}

fn reference_hash(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Value, VmError> {
    let class_name = vm
        .read_ref_field(this, vm.class_of(this), "className", "Ljava/lang/String;")
        .unwrap_or(ObjectRef::NULL);
    let mut hash = object_hash(vm, class_name)?;
    let size = reference_vector_call(vm, this, "size", "()I", &[])?.as_int();
    for index in 0..size {
        let addr = reference_vector_call(
            vm,
            this,
            "elementAt",
            "(I)Ljava/lang/Object;",
            &[Value::Int(index)],
        )?
        .as_ref();
        hash = hash.wrapping_add(object_hash(vm, addr)?);
    }
    int(hash)
}

fn reference_to_string(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Value, VmError> {
    let class_name = vm
        .read_string_field(this, vm.class_of(this), "className")
        .unwrap_or_default();
    let mut text = alloc::format!("Reference Class Name: {class_name}\n");
    let size = reference_vector_call(vm, this, "size", "()I", &[])?.as_int();
    for index in 0..size {
        let addr = reference_vector_call(
            vm,
            this,
            "elementAt",
            "(I)Ljava/lang/Object;",
            &[Value::Int(index)],
        )?
        .as_ref();
        let (class, method) =
            vm.resolve_virtual_method(addr, "toString", "()Ljava/lang/String;")?;
        let result = vm.invoke_method(class, method, vec![Value::Ref(addr)])?;
        text.push_str(&vm.string_value(result.as_ref()).unwrap_or_default());
    }
    object(vm.make_string(&text)?)
}

// -------------------------------------------------------------------------------------------
// javax.naming.Context
// -------------------------------------------------------------------------------------------

/// `javax.naming.Context`: the naming-operation interface. Only the type is
/// modeled; the directory operations themselves need a service provider.
pub(crate) const CONTEXT: oxjvm_vm::NativeClass = class(
    "javax/naming/Context",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

// -------------------------------------------------------------------------------------------
// javax.naming.NamingException
// -------------------------------------------------------------------------------------------

/// `javax.naming.NamingException`: the root of the JNDI exception hierarchy.
pub(crate) const NAMING_EXCEPTION: oxjvm_vm::NativeClass = class(
    "javax/naming/NamingException",
    Some("java/lang/Exception"),
    &[],
    ACC_PUBLIC,
    &[
        field("resolvedName", "Ljavax/naming/Name;", ACC_PROTECTED, None),
        field("resolvedObj", "Ljava/lang/Object;", ACC_PROTECTED, None),
        field("remainingName", "Ljavax/naming/Name;", ACC_PROTECTED, None),
        field(
            "rootException",
            "Ljava/lang/Throwable;",
            ACC_PROTECTED,
            None,
        ),
    ],
    &NAMING_EXCEPTION_METHODS,
    None,
);

const NAMING_EXCEPTION_METHODS: [oxjvm_vm::NativeMethodDef; 15] = [
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
    method(
        "getExplanation",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (class, method) =
                vm.resolve_virtual_method(this, "getMessage", "()Ljava/lang/String;")?;
            let result = vm.invoke_method(class, method, vec![Value::Ref(this)])?;
            object(result.as_ref())
        },
    ),
    method(
        "getResolvedObj",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            object(naming_exception_field(
                vm,
                this,
                "resolvedObj",
                "Ljava/lang/Object;",
            ))
        },
    ),
    method(
        "setResolvedObj",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            set_naming_exception_field(
                vm,
                this,
                "resolvedObj",
                "Ljava/lang/Object;",
                ref_arg(args, 1),
            )
        },
    ),
    method(
        "getResolvedName",
        "()Ljavax/naming/Name;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            object(naming_exception_field(
                vm,
                this,
                "resolvedName",
                "Ljavax/naming/Name;",
            ))
        },
    ),
    method(
        "setResolvedName",
        "(Ljavax/naming/Name;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            set_naming_exception_field(
                vm,
                this,
                "resolvedName",
                "Ljavax/naming/Name;",
                ref_arg(args, 1),
            )
        },
    ),
    method(
        "getRemainingName",
        "()Ljavax/naming/Name;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            object(naming_exception_field(
                vm,
                this,
                "remainingName",
                "Ljavax/naming/Name;",
            ))
        },
    ),
    method(
        "setRemainingName",
        "(Ljavax/naming/Name;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            set_naming_exception_field(
                vm,
                this,
                "remainingName",
                "Ljavax/naming/Name;",
                ref_arg(args, 1),
            )
        },
    ),
    method(
        "getRootCause",
        "()Ljava/lang/Throwable;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            object(naming_exception_field(
                vm,
                this,
                "rootException",
                "Ljava/lang/Throwable;",
            ))
        },
    ),
    method(
        "setRootCause",
        "(Ljava/lang/Throwable;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let cause = ref_arg(args, 1);
            if cause != this {
                set_naming_exception_field(
                    vm,
                    this,
                    "rootException",
                    "Ljava/lang/Throwable;",
                    cause,
                )?;
            }
            void()
        },
    ),
    method(
        "getCause",
        "()Ljava/lang/Throwable;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            object(naming_exception_field(
                vm,
                this,
                "rootException",
                "Ljava/lang/Throwable;",
            ))
        },
    ),
    method(
        "initCause",
        "(Ljava/lang/Throwable;)Ljava/lang/Throwable;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let cause = ref_arg(args, 1);
            let throwable = vm.resolve_class("java/lang/Throwable")?;
            let (declaring, method) = vm
                .find_method(
                    throwable,
                    "initCause",
                    "(Ljava/lang/Throwable;)Ljava/lang/Throwable;",
                )
                .ok_or_else(|| VmError::internal("Throwable lacks initCause"))?;
            vm.invoke_method(declaring, method, vec![Value::Ref(this), Value::Ref(cause)])?;
            if cause != this {
                set_naming_exception_field(
                    vm,
                    this,
                    "rootException",
                    "Ljava/lang/Throwable;",
                    cause,
                )?;
            }
            object(this)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| naming_exception_to_string(vm, receiver(args)),
    ),
    method(
        "toString",
        "(Z)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let detail = bool_arg(args, 1);
            let resolved = naming_exception_field(vm, this, "resolvedObj", "Ljava/lang/Object;");
            if !detail || resolved.is_null() {
                return naming_exception_to_string(vm, this);
            }
            let base = naming_exception_to_string(vm, this)?;
            let mut text = vm.string_value(base.as_ref()).unwrap_or_default();
            let (class, method) =
                vm.resolve_virtual_method(resolved, "toString", "()Ljava/lang/String;")?;
            let result = vm.invoke_method(class, method, vec![Value::Ref(resolved)])?;
            text.push_str("; resolved object ");
            text.push_str(&vm.string_value(result.as_ref()).unwrap_or_default());
            object(vm.make_string(&text)?)
        },
    ),
];

fn naming_exception_field(vm: &Vm<'_>, this: ObjectRef, name: &str, descriptor: &str) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), name, descriptor)
        .unwrap_or(ObjectRef::NULL)
}

fn set_naming_exception_field(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: &str,
    descriptor: &str,
    value: ObjectRef,
) -> Result<Value, VmError> {
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, name, descriptor)?;
    vm.set_instance_ref(this, declaring, field, value);
    void()
}

/// `NamingException.toString()`: `Throwable.toString()` plus the root exception
/// and remaining name, when present.
fn naming_exception_to_string(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Value, VmError> {
    let throwable = vm.resolve_class("java/lang/Throwable")?;
    let (declaring, method) = vm
        .find_method(throwable, "toString", "()Ljava/lang/String;")
        .ok_or_else(|| VmError::internal("Throwable lacks toString"))?;
    let base = vm.invoke_method(declaring, method, vec![Value::Ref(this)])?;
    let mut text = vm.string_value(base.as_ref()).unwrap_or_default();
    let root = naming_exception_field(vm, this, "rootException", "Ljava/lang/Throwable;");
    if !root.is_null() {
        text.push_str(" [Root exception is ");
        text.push_str(&object_text(vm, root)?);
        text.push(']');
    }
    let remaining = naming_exception_field(vm, this, "remainingName", "Ljavax/naming/Name;");
    if !remaining.is_null() {
        text.push_str("; remaining name '");
        text.push_str(&object_text(vm, remaining)?);
        text.push('\'');
    }
    object(vm.make_string(&text)?)
}

/// The virtual `toString()` of an object as text.
fn object_text(vm: &mut Vm<'_>, reference: ObjectRef) -> Result<String, VmError> {
    let (class, method) =
        vm.resolve_virtual_method(reference, "toString", "()Ljava/lang/String;")?;
    let result = vm.invoke_method(class, method, vec![Value::Ref(reference)])?;
    Ok(vm.string_value(result.as_ref()).unwrap_or_default())
}

// -------------------------------------------------------------------------------------------
// javax.naming.InitialContext
// -------------------------------------------------------------------------------------------

/// `javax.naming.InitialContext`: the entry point that holds the environment.
/// Without a service provider the default context cannot be created, but the
/// environment cloning/initialization contract is fully modeled.
pub(crate) const INITIAL_CONTEXT: oxjvm_vm::NativeClass = class(
    "javax/naming/InitialContext",
    Some("java/lang/Object"),
    &["javax/naming/Context"],
    ACC_PUBLIC,
    &[
        field("myProps", "Ljava/util/Hashtable;", ACC_PROTECTED, None),
        field(
            "defaultInitCtx",
            "Ljavax/naming/Context;",
            ACC_PROTECTED,
            None,
        ),
        field("gotDefault", "Z", ACC_PROTECTED, None),
    ],
    &INITIAL_CONTEXT_METHODS,
    None,
);

const INITIAL_CONTEXT_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        initial_context_init(vm, receiver(args), ObjectRef::NULL)
    }),
    method(
        "<init>",
        "(Ljava/util/Hashtable;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let env = ref_arg(args, 1);
            // The constructor clones the environment before handing it to init().
            let env = if env.is_null() {
                env
            } else {
                let (class, method) =
                    vm.resolve_virtual_method(env, "clone", "()Ljava/lang/Object;")?;
                vm.invoke_method(class, method, vec![Value::Ref(env)])?
                    .as_ref()
            };
            initial_context_init(vm, this, env)
        },
    ),
    method("<init>", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        if !bool_arg(args, 1) {
            initial_context_init(vm, receiver(args), ObjectRef::NULL)?;
        }
        void()
    }),
    method(
        "init",
        "(Ljava/util/Hashtable;)V",
        ACC_PROTECTED,
        |vm, _, args| initial_context_init(vm, receiver(args), ref_arg(args, 1)),
    ),
    method(
        "getDefaultInitCtx",
        "()Ljavax/naming/Context;",
        ACC_PROTECTED,
        |vm, _, _| {
            Err(vm.throw_new(
                "javax/naming/NamingException",
                Some("Cannot instantiate initial context: no service provider is available"),
            ))
        },
    ),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        let (declaring, field) = vm.find_field(class, "myProps", "Ljava/util/Hashtable;")?;
        vm.set_instance_ref(this, declaring, field, ObjectRef::NULL);
        let (declaring, field) =
            vm.find_field(class, "defaultInitCtx", "Ljavax/naming/Context;")?;
        vm.set_instance_ref(this, declaring, field, ObjectRef::NULL);
        let (declaring, field) = vm.find_field(class, "gotDefault", "Z")?;
        vm.set_instance_int(this, declaring, field, 0);
        void()
    }),
];

/// Create an empty `java.util.Hashtable` with the given initial capacity.
fn new_hashtable(vm: &mut Vm<'_>, capacity: i32) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/Hashtable")?;
    let instance = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(class, "<init>", "(I)V")
        .ok_or_else(|| VmError::internal("Hashtable lacks its capacity constructor"))?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![Value::Ref(instance), Value::Int(capacity)],
    )?;
    Ok(instance)
}

/// `InitialContext.init()`: store the environment as-is (a null environment
/// becomes a fresh table), then try the factory when one is configured.
fn initial_context_init(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    env: ObjectRef,
) -> Result<Value, VmError> {
    let class = vm.class_of(this);
    let props = if env.is_null() {
        new_hashtable(vm, 11)?
    } else {
        env
    };
    let (declaring, field) = vm.find_field(class, "myProps", "Ljava/util/Hashtable;")?;
    vm.set_instance_ref(this, declaring, field, props);
    // ResourceManager.getInitialEnvironment() merges JNDI system properties and
    // application resource files into the table in place; neither source exists
    // in this VM, so a non-null environment passes through unchanged.
    let key = vm.make_string("java.naming.factory.initial")?;
    let (method_class, method) =
        vm.resolve_virtual_method(props, "get", "(Ljava/lang/Object;)Ljava/lang/Object;")?;
    let factory = vm.invoke_method(
        method_class,
        method,
        vec![Value::Ref(props), Value::Ref(key)],
    )?;
    if !factory.as_ref().is_null() {
        let factory_name = vm.string_value(factory.as_ref()).unwrap_or_default();
        return Err(vm.throw_new(
            "javax/naming/NamingException",
            Some(&alloc::format!("Cannot instantiate class: {factory_name}")),
        ));
    }
    void()
}

// -------------------------------------------------------------------------------------------
// javax.naming.ldap.Rdn
// -------------------------------------------------------------------------------------------

/// `javax.naming.ldap.Rdn`: a relative distinguished name, holding an ordered
/// list of type/value entries plus the RFC 2253 escape/unescape value helpers.
pub(crate) const RDN: oxjvm_vm::NativeClass = class(
    "javax/naming/ldap/Rdn",
    Some("java/lang/Object"),
    &["java/io/Serializable", "java/lang/Comparable"],
    ACC_PUBLIC,
    &[field("entries", "Ljava/util/ArrayList;", ACC_PRIVATE, None)],
    &RDN_METHODS,
    None,
);

const RDN_METHODS: [oxjvm_vm::NativeMethodDef; 17] = [
    method("<init>", "()V", 0, |vm, _, args| {
        let this = receiver(args);
        let entries = crate::util::new_array_list(vm)?;
        ldap_set_ref(vm, this, "entries", "Ljava/util/ArrayList;", entries)?;
        void()
    }),
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = ref_arg(args, 1);
            if text.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let entries = crate::util::new_array_list(vm)?;
            ldap_set_ref(vm, this, "entries", "Ljava/util/ArrayList;", entries)?;
            ldap_parse_rdn_into(vm, text, this)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljavax/naming/ldap/Rdn;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let source = ref_arg(args, 1);
            if source.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let elements = crate::util::array_list_elements(vm, rdn_entries(vm, source));
            let entries = crate::util::array_list_from(vm, &elements)?;
            ldap_set_ref(vm, this, "entries", "Ljava/util/ArrayList;", entries)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let attr_type = ref_arg(args, 1);
            let value = ref_arg(args, 2);
            if value.is_null() {
                return Err(vm.throw_new(
                    "java/lang/NullPointerException",
                    Some("Cannot set value to null"),
                ));
            }
            if attr_type.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let type_text = vm.string_value(attr_type).unwrap_or_default();
            let value_class = vm.class_name(vm.class_of(value));
            let empty_value = (value_class == "[B" && vm.array_length(value)? == 0)
                || (value_class == "java/lang/String"
                    && vm.string_value(value).unwrap_or_default().is_empty());
            if type_text.is_empty() || empty_value {
                let value_text = object_text(vm, value)?;
                return Err(vm.throw_new(
                    "javax/naming/InvalidNameException",
                    Some(&alloc::format!(
                        "type or value cannot be empty, type:{type_text} value:{value_text}"
                    )),
                ));
            }
            let entries = crate::util::new_array_list(vm)?;
            ldap_set_ref(vm, this, "entries", "Ljava/util/ArrayList;", entries)?;
            rdn_put(vm, this, attr_type, value)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljavax/naming/directory/Attributes;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let attr_set = ref_arg(args, 1);
            if attr_set.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let size = ldap_call(vm, attr_set, "size", "()I", vec![])?.as_int();
            if size == 0 {
                return Err(vm.throw_new(
                    "javax/naming/InvalidNameException",
                    Some("Attributes cannot be empty"),
                ));
            }
            let entries = crate::util::new_array_list(vm)?;
            ldap_set_ref(vm, this, "entries", "Ljava/util/ArrayList;", entries)?;
            let all = ldap_call(
                vm,
                attr_set,
                "getAll",
                "()Ljavax/naming/NamingEnumeration;",
                vec![],
            )?
            .as_ref();
            loop {
                let more = match ldap_call(vm, all, "hasMore", "()Z", vec![]) {
                    Ok(value) => value.as_boolean(),
                    Err(error) => return Err(wrap_naming_error(vm, error)?),
                };
                if !more {
                    break;
                }
                let attr = match ldap_call(vm, all, "next", "()Ljava/lang/Object;", vec![]) {
                    Ok(value) => value.as_ref(),
                    Err(error) => return Err(wrap_naming_error(vm, error)?),
                };
                let attr_type =
                    ldap_call(vm, attr, "getID", "()Ljava/lang/String;", vec![])?.as_ref();
                let value = ldap_call(vm, attr, "get", "()Ljava/lang/Object;", vec![])?.as_ref();
                let entry = rdn_entry_new(vm, attr_type, value)?;
                let mut elements = crate::util::array_list_elements(vm, entries);
                elements.push(entry);
                crate::util::array_list_store(vm, entries, &elements)?;
            }
            rdn_sort(vm, this)?;
            void()
        },
    ),
    method(
        "escapeValue",
        "(Ljava/lang/Object;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let units = rdn_escape_value_units(vm, ref_arg(args, 0))?;
            object(vm.make_string_utf16(units)?)
        },
    ),
    method(
        "unescapeValue",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let val = ref_arg(args, 0);
            if val.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let units = vm.string_utf16(val).unwrap_or_default().to_vec();
            let original = vm.string_value(val).unwrap_or_default();
            rdn_unescape(vm, &units, &original)
        },
    ),
    method(
        "put",
        "(Ljava/lang/String;Ljava/lang/Object;)Ljavax/naming/ldap/Rdn;",
        0,
        |vm, _, args| {
            let this = receiver(args);
            rdn_put(vm, this, ref_arg(args, 1), ref_arg(args, 2))?;
            object(this)
        },
    ),
    method("sort", "()V", 0, |vm, _, args| {
        rdn_sort(vm, receiver(args))?;
        void()
    }),
    method(
        "getValue",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let entry = rdn_first_entry(vm, this)?;
            ldap_call(vm, entry, "getValue", "()Ljava/lang/Object;", vec![])
        },
    ),
    method(
        "getType",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let entry = rdn_first_entry(vm, this)?;
            ldap_call(vm, entry, "getType", "()Ljava/lang/String;", vec![])
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = rdn_text(vm, receiver(args))?;
            object(vm.make_string_utf16(text)?)
        },
    ),
    method(
        "compareTo",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let result = rdn_compare(vm, receiver(args), ref_arg(args, 1))?;
            int(result)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let result = rdn_equals(vm, receiver(args), ref_arg(args, 1))?;
            boolean(result)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let result = rdn_hash(vm, receiver(args))?;
        int(result)
    }),
    method(
        "toAttributes",
        "()Ljavax/naming/directory/Attributes;",
        ACC_PUBLIC,
        |vm, _, args| {
            let attrs = rdn_to_attributes(vm, receiver(args))?;
            object(attrs)
        },
    ),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let elements = crate::util::array_list_elements(vm, rdn_entries(vm, this));
        int(elements.len() as i32)
    }),
];
// -------------------------------------------------------------------------------------------
// LDAP name shared helpers
// -------------------------------------------------------------------------------------------

fn ldap_read_ref(vm: &Vm<'_>, this: ObjectRef, name: &str, descriptor: &str) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), name, descriptor)
        .unwrap_or(ObjectRef::NULL)
}

fn ldap_set_ref(
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

/// Virtual call with the receiver prepended to `extra` arguments.
fn ldap_call(
    vm: &mut Vm<'_>,
    target: ObjectRef,
    name: &str,
    descriptor: &str,
    extra: Vec<Value>,
) -> Result<Value, VmError> {
    let (class, method) = vm.resolve_virtual_method(target, name, descriptor)?;
    let mut args = vec![Value::Ref(target)];
    args.extend(extra);
    vm.invoke_method(class, method, args)
}

/// Non-virtual call of a method declared on the receiver's own class.
fn ldap_self_call(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: &str,
    descriptor: &str,
    extra: Vec<Value>,
) -> Result<Value, VmError> {
    let class = vm.class_of(this);
    let (declaring, method) = vm
        .find_method(class, name, descriptor)
        .ok_or_else(|| VmError::internal(alloc::format!("missing {name}{descriptor}")))?;
    let mut args = vec![Value::Ref(this)];
    args.extend(extra);
    vm.invoke_method(declaring, method, args)
}

fn ldap_string_units(vm: &Vm<'_>, text: ObjectRef) -> Vec<u16> {
    vm.string_utf16(text).unwrap_or_default().to_vec()
}

fn ldap_locale_english(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/Locale")?;
    vm.ensure_initialized(class)?;
    vm.read_static_ref_if_set(class, "ENGLISH", "Ljava/util/Locale;")
        .ok_or_else(|| VmError::internal("Locale.ENGLISH is missing"))
}

fn ldap_to_upper_english(vm: &mut Vm<'_>, text: ObjectRef) -> Result<ObjectRef, VmError> {
    let english = ldap_locale_english(vm)?;
    Ok(ldap_call(
        vm,
        text,
        "toUpperCase",
        "(Ljava/util/Locale;)Ljava/lang/String;",
        vec![Value::Ref(english)],
    )?
    .as_ref())
}

fn ldap_to_lower_english(vm: &mut Vm<'_>, text: ObjectRef) -> Result<ObjectRef, VmError> {
    let english = ldap_locale_english(vm)?;
    Ok(ldap_call(
        vm,
        text,
        "toLowerCase",
        "(Ljava/util/Locale;)Ljava/lang/String;",
        vec![Value::Ref(english)],
    )?
    .as_ref())
}

/// JDK code catching `NamingException` treats it as inequality / end of data.
fn swallow_naming(vm: &mut Vm<'_>, error: VmError) -> Result<bool, VmError> {
    if let VmError::Thrown(exception) = error {
        let class = vm.resolve_class("javax/naming/NamingException")?;
        if vm.is_instance(exception, class) {
            vm.discard_backtrace(exception);
            return Ok(false);
        }
    }
    Err(error)
}

/// `Rdn(Attributes)` wraps a `NamingException` from the attribute set into an
/// `InvalidNameException` with the original as its cause.
fn wrap_naming_error(vm: &mut Vm<'_>, error: VmError) -> Result<VmError, VmError> {
    let VmError::Thrown(cause) = error else {
        return Ok(error);
    };
    let naming = vm.resolve_class("javax/naming/NamingException")?;
    if !vm.is_instance(cause, naming) {
        return Ok(VmError::Thrown(cause));
    }
    let message = ldap_call(vm, cause, "getMessage", "()Ljava/lang/String;", vec![])?.as_ref();
    let text = vm.string_value(message);
    let wrapped = vm.throw_new("javax/naming/InvalidNameException", text.as_deref());
    let VmError::Thrown(invalid) = wrapped else {
        return Ok(wrapped);
    };
    ldap_call(
        vm,
        invalid,
        "initCause",
        "(Ljava/lang/Throwable;)Ljava/lang/Throwable;",
        vec![Value::Ref(cause)],
    )?;
    Ok(VmError::Thrown(invalid))
}

// -------------------------------------------------------------------------------------------
// RFC 2253 value escaping
// -------------------------------------------------------------------------------------------

/// RFC 2253 "whitespace": space and carriage return only.
fn rdn_is_whitespace(unit: u16) -> bool {
    unit == u16::from(b' ') || unit == u16::from(b'\r')
}

fn rdn_char(unit: u16) -> char {
    char::from_u32(u32::from(unit)).unwrap_or('\u{FFFD}')
}

fn rdn_digit(unit: u16, radix: u32) -> i32 {
    rdn_char(unit).to_digit(radix).map_or(-1, |d| d as i32)
}

/// Escape a string value: backslash-prefix leading/trailing whitespace and
/// any of `,=+<>#;"\`.
fn rdn_escape_string(units: &[u16]) -> Vec<u16> {
    const ESCAPEES: &str = ",=+<>#;\"\\";
    let mut lead = 0;
    while lead < units.len() && rdn_is_whitespace(units[lead]) {
        lead += 1;
    }
    let mut trail = units.len() as i32 - 1;
    while trail >= 0 && rdn_is_whitespace(units[trail as usize]) {
        trail -= 1;
    }
    let mut out = Vec::with_capacity(units.len());
    for (index, &unit) in units.iter().enumerate() {
        if index < lead || index as i32 > trail || ESCAPEES.contains(rdn_char(unit)) {
            out.push(u16::from(b'\\'));
        }
        out.push(unit);
    }
    out
}

/// Escape a binary value as `#` followed by lowercase hex pairs.
fn rdn_escape_binary(bytes: &[u8]) -> String {
    let mut text = String::from("#");
    for &byte in bytes {
        text.push(char::from_digit(u32::from(0xF & (byte >> 4)), 16).unwrap_or('0'));
        text.push(char::from_digit(u32::from(0xF & byte), 16).unwrap_or('0'));
    }
    text
}

/// `Rdn.escapeValue`: String values are escaped, byte arrays become hex.
fn rdn_escape_value_units(vm: &mut Vm<'_>, val: ObjectRef) -> Result<Vec<u16>, VmError> {
    if val.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let class = vm.class_of(val);
    let name = vm.class_name(class);
    if name == "[B" {
        let bytes = crate::lang::array_bytes(vm, val)?;
        Ok(rdn_escape_binary(&bytes).encode_utf16().collect())
    } else if name == "java/lang/String" {
        Ok(rdn_escape_string(vm.string_utf16(val).unwrap_or_default()))
    } else {
        Err(vm.throw_new(
            "java/lang/ClassCastException",
            Some(&alloc::format!(
                "class {name} cannot be cast to class java.lang.String"
            )),
        ))
    }
}

/// Decode a `#CEB1DF80`-style hex-pair value into bytes.
fn rdn_decode_hex_pairs(
    vm: &mut Vm<'_>,
    units: &[u16],
    mut beg: usize,
    end: usize,
    original: &str,
) -> Result<Vec<u8>, VmError> {
    let mut bytes = Vec::new();
    while beg + 1 < end {
        let hi = rdn_digit(units[beg], 16);
        let lo = rdn_digit(units[beg + 1], 16);
        if hi < 0 || lo < 0 {
            break;
        }
        bytes.push(((hi << 4) + lo) as u8);
        beg += 2;
    }
    if beg != end {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some(&alloc::format!("Illegal attribute value: {original}")),
        ));
    }
    Ok(bytes)
}

/// Collect the longest prefix of `\XY` hex-encoded UTF-8 octets.
fn rdn_utf8_octets(units: &[u16], mut beg: usize, end: usize) -> Vec<u8> {
    let mut octets = Vec::new();
    while beg + 2 < end {
        if units[beg] != u16::from(b'\\') {
            break;
        }
        beg += 1;
        let hi = rdn_digit(units[beg], 16);
        beg += 1;
        let lo = rdn_digit(units[beg], 16);
        beg += 1;
        if hi < 0 || lo < 0 {
            break;
        }
        octets.push(((hi << 4) + lo) as u8);
    }
    octets
}

/// Unescape an RFC 2253 attribute value, returning a String or a byte array.
fn rdn_unescape(vm: &mut Vm<'_>, units: &[u16], original: &str) -> Result<Value, VmError> {
    let mut beg = 0;
    let mut end = units.len();
    while beg < end && rdn_is_whitespace(units[beg]) {
        beg += 1;
    }
    while beg < end && rdn_is_whitespace(units[end - 1]) {
        end -= 1;
    }
    // Retain a trailing whitespace char that was preceded by a backslash.
    if end != units.len() && beg < end && units[end - 1] == u16::from(b'\\') {
        end += 1;
    }
    if beg >= end {
        return object(vm.make_string("")?);
    }
    if units[beg] == u16::from(b'#') {
        let bytes = rdn_decode_hex_pairs(vm, units, beg + 1, end, original)?;
        return object(crate::lang::byte_array_from(vm, &bytes)?);
    }
    if units[beg] == u16::from(b'"') && units[end - 1] == u16::from(b'"') {
        beg += 1;
        end -= 1;
    }
    let mut builder: Vec<u16> = Vec::new();
    let mut esc: i32 = -1;
    let mut index = beg;
    while index < end {
        if units[index] == u16::from(b'\\') && index + 1 < end {
            if !rdn_char(units[index + 1]).is_alphanumeric() {
                index += 1;
                builder.push(units[index]);
                esc = index as i32;
                index += 1;
            } else {
                let octets = rdn_utf8_octets(units, index, end);
                if octets.is_empty() {
                    return Err(vm.throw_new(
                        "java/lang/IllegalArgumentException",
                        Some(&alloc::format!(
                            "Not a valid attribute string value:{original},improper usage of backslash"
                        )),
                    ));
                }
                let decoded = String::from_utf8_lossy(&octets);
                builder.extend(decoded.encode_utf16());
                index += octets.len() * 3;
            }
        } else {
            builder.push(units[index]);
            index += 1;
        }
    }
    if let Some(&last) = builder.last() {
        if rdn_is_whitespace(last) && esc != end as i32 - 1 {
            builder.pop();
        }
    }
    object(vm.make_string_utf16(builder)?)
}
// -------------------------------------------------------------------------------------------
// javax.naming.ldap.Rdn$RdnEntry
// -------------------------------------------------------------------------------------------

/// One type/value pair of an `Rdn`, with a lazily cached canonical value
/// representation used for case-insensitive comparison.
pub(crate) const RDN_ENTRY: oxjvm_vm::NativeClass = class(
    "javax/naming/ldap/Rdn$RdnEntry",
    Some("java/lang/Object"),
    &["java/lang/Comparable"],
    ACC_STATIC,
    &[
        field("type", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("value", "Ljava/lang/Object;", ACC_PRIVATE, None),
        field("comparable", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &RDN_ENTRY_METHODS,
    None,
);

const RDN_ENTRY_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method("<init>", "()V", 0, |_, _, _| void()),
    method("getType", "()Ljava/lang/String;", 0, |vm, _, args| {
        let this = receiver(args);
        object(ldap_read_ref(vm, this, "type", "Ljava/lang/String;"))
    }),
    method("getValue", "()Ljava/lang/Object;", 0, |vm, _, args| {
        let this = receiver(args);
        object(ldap_read_ref(vm, this, "value", "Ljava/lang/Object;"))
    }),
    method(
        "compareTo",
        "(Ljavax/naming/ldap/Rdn$RdnEntry;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let result = rdn_entry_compare(vm, receiver(args), ref_arg(args, 1))?;
            int(result)
        },
    ),
    method(
        "compareTo",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let entry_class = vm.resolve_class("javax/naming/ldap/Rdn$RdnEntry")?;
            if !vm.is_instance(other, entry_class) {
                return Err(vm.throw_new(
                    "java/lang/ClassCastException",
                    Some("The obj is not a RdnEntry"),
                ));
            }
            let result = rdn_entry_compare(vm, this, other)?;
            int(result)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let result = rdn_entry_equals(vm, receiver(args), ref_arg(args, 1))?;
            boolean(result)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let result = rdn_entry_hash(vm, receiver(args))?;
        int(result)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let attr_type = ldap_read_ref(vm, this, "type", "Ljava/lang/String;");
            let value = ldap_read_ref(vm, this, "value", "Ljava/lang/Object;");
            let mut units = ldap_string_units(vm, attr_type);
            units.push(u16::from(b'='));
            units.extend(rdn_escape_value_units(vm, value)?);
            object(vm.make_string_utf16(units)?)
        },
    ),
];

// -------------------------------------------------------------------------------------------
// Rdn instance helpers
// -------------------------------------------------------------------------------------------

fn rdn_entries(vm: &Vm<'_>, rdn: ObjectRef) -> ObjectRef {
    ldap_read_ref(vm, rdn, "entries", "Ljava/util/ArrayList;")
}

fn rdn_entry_new(
    vm: &mut Vm<'_>,
    attr_type: ObjectRef,
    value: ObjectRef,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("javax/naming/ldap/Rdn$RdnEntry")?;
    let entry = vm.new_instance(class)?;
    ldap_set_ref(vm, entry, "type", "Ljava/lang/String;", attr_type)?;
    ldap_set_ref(vm, entry, "value", "Ljava/lang/Object;", value)?;
    Ok(entry)
}

/// `Rdn.put`: append a type/value entry, cloning byte-array values.
fn rdn_put(
    vm: &mut Vm<'_>,
    rdn: ObjectRef,
    attr_type: ObjectRef,
    value: ObjectRef,
) -> Result<ObjectRef, VmError> {
    let stored = if vm.class_name(vm.class_of(value)) == "[B" {
        let bytes = crate::lang::array_bytes(vm, value)?;
        crate::lang::byte_array_from(vm, &bytes)?
    } else {
        value
    };
    let entry = rdn_entry_new(vm, attr_type, stored)?;
    let entries = rdn_entries(vm, rdn);
    let mut elements = crate::util::array_list_elements(vm, entries);
    elements.push(entry);
    crate::util::array_list_store(vm, entries, &elements)?;
    Ok(rdn)
}

/// `Rdn.sort`: stable ordering of entries by type then canonical value.
fn rdn_sort(vm: &mut Vm<'_>, rdn: ObjectRef) -> Result<(), VmError> {
    let entries = rdn_entries(vm, rdn);
    let mut elements = crate::util::array_list_elements(vm, entries);
    if elements.len() > 1 {
        for index in 1..elements.len() {
            let mut position = index;
            while position > 0 {
                let diff = rdn_entry_compare(vm, elements[position - 1], elements[position])?;
                if diff <= 0 {
                    break;
                }
                elements.swap(position - 1, position);
                position -= 1;
            }
        }
        crate::util::array_list_store(vm, entries, &elements)?;
    }
    Ok(())
}

fn rdn_first_entry(vm: &mut Vm<'_>, rdn: ObjectRef) -> Result<ObjectRef, VmError> {
    let elements = crate::util::array_list_elements(vm, rdn_entries(vm, rdn));
    elements.first().copied().ok_or_else(|| {
        vm.throw_new(
            "java/lang/IndexOutOfBoundsException",
            Some("Index 0 out of bounds for length 0"),
        )
    })
}

/// The canonical comparison form of an entry value: hex for byte arrays,
/// upper-cased (English locale) text for strings. Cached on the entry.
fn rdn_entry_comparable(vm: &mut Vm<'_>, entry: ObjectRef) -> Result<ObjectRef, VmError> {
    let cached = ldap_read_ref(vm, entry, "comparable", "Ljava/lang/String;");
    if !cached.is_null() {
        return Ok(cached);
    }
    let value = ldap_read_ref(vm, entry, "value", "Ljava/lang/Object;");
    if value.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let text = if vm.class_name(vm.class_of(value)) == "[B" {
        let bytes = crate::lang::array_bytes(vm, value)?;
        vm.make_string(&rdn_escape_binary(&bytes))?
    } else {
        ldap_to_upper_english(vm, value)?
    };
    ldap_set_ref(vm, entry, "comparable", "Ljava/lang/String;", text)?;
    Ok(text)
}

fn rdn_entry_compare(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<i32, VmError> {
    let type_a = ldap_read_ref(vm, a, "type", "Ljava/lang/String;");
    let type_b = ldap_read_ref(vm, b, "type", "Ljava/lang/String;");
    let diff = ldap_call(
        vm,
        type_a,
        "compareToIgnoreCase",
        "(Ljava/lang/String;)I",
        vec![Value::Ref(type_b)],
    )?
    .as_int();
    if diff != 0 {
        return Ok(diff);
    }
    let value_a = ldap_read_ref(vm, a, "value", "Ljava/lang/Object;");
    let value_b = ldap_read_ref(vm, b, "value", "Ljava/lang/Object;");
    // Arrays use identity equality (Object.equals); strings compare by value.
    let shortcut = value_a == value_b
        || (!vm.class_name(vm.class_of(value_a)).starts_with('[')
            && !vm.class_name(vm.class_of(value_b)).starts_with('[')
            && ldap_call(
                vm,
                value_a,
                "equals",
                "(Ljava/lang/Object;)Z",
                vec![Value::Ref(value_b)],
            )?
            .as_boolean());
    if shortcut {
        return Ok(0);
    }
    let comparable_a = rdn_entry_comparable(vm, a)?;
    let comparable_b = rdn_entry_comparable(vm, b)?;
    Ok(ldap_call(
        vm,
        comparable_a,
        "compareTo",
        "(Ljava/lang/String;)I",
        vec![Value::Ref(comparable_b)],
    )?
    .as_int())
}

fn rdn_entry_equals(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<bool, VmError> {
    if a == b {
        return Ok(true);
    }
    let entry_class = vm.resolve_class("javax/naming/ldap/Rdn$RdnEntry")?;
    if !vm.is_instance(b, entry_class) {
        return Ok(false);
    }
    let type_a = ldap_read_ref(vm, a, "type", "Ljava/lang/String;");
    let type_b = ldap_read_ref(vm, b, "type", "Ljava/lang/String;");
    if !ldap_call(
        vm,
        type_a,
        "equalsIgnoreCase",
        "(Ljava/lang/String;)Z",
        vec![Value::Ref(type_b)],
    )?
    .as_boolean()
    {
        return Ok(false);
    }
    let comparable_a = rdn_entry_comparable(vm, a)?;
    let comparable_b = rdn_entry_comparable(vm, b)?;
    Ok(ldap_call(
        vm,
        comparable_a,
        "equals",
        "(Ljava/lang/Object;)Z",
        vec![Value::Ref(comparable_b)],
    )?
    .as_boolean())
}

fn rdn_entry_hash(vm: &mut Vm<'_>, entry: ObjectRef) -> Result<i32, VmError> {
    let attr_type = ldap_read_ref(vm, entry, "type", "Ljava/lang/String;");
    let upper = ldap_to_upper_english(vm, attr_type)?;
    let type_hash = ldap_call(vm, upper, "hashCode", "()I", vec![])?.as_int();
    let comparable = rdn_entry_comparable(vm, entry)?;
    let value_hash = ldap_call(vm, comparable, "hashCode", "()I", vec![])?.as_int();
    Ok(type_hash.wrapping_add(value_hash))
}

fn rdn_text(vm: &mut Vm<'_>, rdn: ObjectRef) -> Result<Vec<u16>, VmError> {
    let elements = crate::util::array_list_elements(vm, rdn_entries(vm, rdn));
    let mut units: Vec<u16> = Vec::new();
    for (index, entry) in elements.iter().enumerate() {
        if index > 0 {
            units.push(u16::from(b'+'));
        }
        let text = ldap_call(vm, *entry, "toString", "()Ljava/lang/String;", vec![])?.as_ref();
        units.extend(ldap_string_units(vm, text));
    }
    Ok(units)
}

fn rdn_compare(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<i32, VmError> {
    let rdn_class = vm.resolve_class("javax/naming/ldap/Rdn")?;
    if !vm.is_instance(b, rdn_class) {
        return Err(vm.throw_new("java/lang/ClassCastException", Some("The obj is not a Rdn")));
    }
    if a == b {
        return Ok(0);
    }
    let entries_a = crate::util::array_list_elements(vm, rdn_entries(vm, a));
    let entries_b = crate::util::array_list_elements(vm, rdn_entries(vm, b));
    let min_size = entries_a.len().min(entries_b.len());
    for index in 0..min_size {
        let diff = rdn_entry_compare(vm, entries_a[index], entries_b[index])?;
        if diff != 0 {
            return Ok(diff);
        }
    }
    Ok((entries_a.len() - entries_b.len()) as i32)
}

fn rdn_equals(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<bool, VmError> {
    if a == b {
        return Ok(true);
    }
    let rdn_class = vm.resolve_class("javax/naming/ldap/Rdn")?;
    if !vm.is_instance(b, rdn_class) {
        return Ok(false);
    }
    let entries_a = crate::util::array_list_elements(vm, rdn_entries(vm, a));
    let entries_b = crate::util::array_list_elements(vm, rdn_entries(vm, b));
    if entries_a.len() != entries_b.len() {
        return Ok(false);
    }
    for index in 0..entries_a.len() {
        if !rdn_entry_equals(vm, entries_a[index], entries_b[index])? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn rdn_hash(vm: &mut Vm<'_>, rdn: ObjectRef) -> Result<i32, VmError> {
    let elements = crate::util::array_list_elements(vm, rdn_entries(vm, rdn));
    let mut hash: i32 = 0;
    for entry in elements {
        hash = hash.wrapping_add(rdn_entry_hash(vm, entry)?);
    }
    Ok(hash)
}

/// `Rdn.toAttributes`: a case-ignoring attribute set; repeated types merge
/// their values into one multi-valued attribute.
fn rdn_to_attributes(vm: &mut Vm<'_>, rdn: ObjectRef) -> Result<ObjectRef, VmError> {
    let attrs = basic_attributes_new(vm, true)?;
    let elements = crate::util::array_list_elements(vm, rdn_entries(vm, rdn));
    for entry in elements {
        let attr_type = ldap_call(vm, entry, "getType", "()Ljava/lang/String;", vec![])?.as_ref();
        let value = ldap_call(vm, entry, "getValue", "()Ljava/lang/Object;", vec![])?.as_ref();
        let previous = ldap_call(
            vm,
            attrs,
            "put",
            "(Ljava/lang/String;Ljava/lang/Object;)Ljavax/naming/directory/Attribute;",
            vec![Value::Ref(attr_type), Value::Ref(value)],
        )?
        .as_ref();
        if !previous.is_null() {
            ldap_call(
                vm,
                previous,
                "add",
                "(Ljava/lang/Object;)Z",
                vec![Value::Ref(value)],
            )?;
            ldap_call(
                vm,
                attrs,
                "put",
                "(Ljavax/naming/directory/Attribute;)Ljavax/naming/directory/Attribute;",
                vec![Value::Ref(previous)],
            )?;
        }
    }
    Ok(attrs)
}
// -------------------------------------------------------------------------------------------
// RFC 2253 parser (javax.naming.ldap.Rfc2253Parser)
// -------------------------------------------------------------------------------------------

struct LdapParser {
    chars: Vec<u16>,
    cur: usize,
    name: String,
}

impl LdapParser {
    fn new(vm: &Vm<'_>, text: ObjectRef) -> Self {
        Self {
            chars: vm.string_utf16(text).unwrap_or_default().to_vec(),
            cur: 0,
            name: vm.string_value(text).unwrap_or_default(),
        }
    }

    fn invalid(&self, vm: &mut Vm<'_>, prefix: &str) -> VmError {
        vm.throw_new(
            "javax/naming/InvalidNameException",
            Some(&alloc::format!("{prefix}{}", self.name)),
        )
    }

    fn consume_whitespace(&mut self) {
        while self.cur < self.chars.len() && rdn_is_whitespace(self.chars[self.cur]) {
            self.cur += 1;
        }
    }

    /// Attribute types: letters/digits plus '.', '-' and internal spaces.
    fn parse_attr_type(&mut self, vm: &mut Vm<'_>) -> Result<Vec<u16>, VmError> {
        let beg = self.cur;
        while self.cur < self.chars.len() {
            let unit = self.chars[self.cur];
            if rdn_char(unit).is_alphanumeric()
                || unit == u16::from(b'.')
                || unit == u16::from(b'-')
                || unit == u16::from(b' ')
            {
                self.cur += 1;
            } else {
                break;
            }
        }
        // Back out any trailing spaces.
        while self.cur > beg && self.chars[self.cur - 1] == u16::from(b' ') {
            self.cur -= 1;
        }
        if beg == self.cur {
            return Err(self.invalid(vm, "Invalid name: "));
        }
        Ok(self.chars[beg..self.cur].to_vec())
    }

    fn parse_attr_value(&mut self, vm: &mut Vm<'_>) -> Result<Vec<u16>, VmError> {
        let len = self.chars.len();
        if self.cur < len && self.chars[self.cur] == u16::from(b'#') {
            let beg = self.cur;
            self.cur += 1;
            while self.cur < len && rdn_char(self.chars[self.cur]).is_alphanumeric() {
                self.cur += 1;
            }
            return Ok(self.chars[beg..self.cur].to_vec());
        }
        if self.cur < len && self.chars[self.cur] == u16::from(b'"') {
            let beg = self.cur;
            self.cur += 1;
            while self.cur < len && self.chars[self.cur] != u16::from(b'"') {
                if self.chars[self.cur] == u16::from(b'\\') {
                    self.cur += 1;
                }
                self.cur += 1;
            }
            if self.cur >= len {
                return Err(self.invalid(vm, "Invalid name: "));
            }
            self.cur += 1;
            return Ok(self.chars[beg..self.cur].to_vec());
        }
        let beg = self.cur;
        let mut esc: i32 = -1;
        while self.cur < len
            && self.chars[self.cur] != u16::from(b',')
            && self.chars[self.cur] != u16::from(b';')
            && self.chars[self.cur] != u16::from(b'+')
        {
            if self.chars[self.cur] == u16::from(b'\\') {
                self.cur += 1;
                esc = self.cur as i32;
            }
            self.cur += 1;
        }
        if self.cur > len {
            // A trailing backslash with nothing after it.
            return Err(self.invalid(vm, "Invalid name: "));
        }
        // Trim off unescaped trailing whitespace.
        let mut end = self.cur;
        while end > beg {
            let unit = self.chars[end - 1];
            if !rdn_is_whitespace(unit) || esc == (end - 1) as i32 {
                break;
            }
            end -= 1;
        }
        Ok(self.chars[beg..end].to_vec())
    }
}

/// Parse one RDN into `rdn`; the whole string must be consumed.
fn ldap_parse_rdn_into(vm: &mut Vm<'_>, text: ObjectRef, rdn: ObjectRef) -> Result<(), VmError> {
    let mut parser = LdapParser::new(vm, text);
    ldap_parse_do(&mut parser, vm, rdn)?;
    if parser.cur < parser.chars.len() {
        return Err(parser.invalid(vm, "Invalid RDN: "));
    }
    Ok(())
}

/// Parse a full DN into a fresh `ArrayList` of `Rdn`s (rightmost first).
fn ldap_parse_dn(vm: &mut Vm<'_>, text: ObjectRef) -> Result<ObjectRef, VmError> {
    let mut parser = LdapParser::new(vm, text);
    let mut rdns: Vec<ObjectRef> = Vec::new();
    if parser.chars.is_empty() {
        return crate::util::array_list_from(vm, &rdns);
    }
    let rdn = rdn_new_empty(vm)?;
    ldap_parse_do(&mut parser, vm, rdn)?;
    rdns.push(rdn);
    while parser.cur < parser.chars.len() {
        let unit = parser.chars[parser.cur];
        if unit == u16::from(b',') || unit == u16::from(b';') {
            parser.cur += 1;
            let next = rdn_new_empty(vm)?;
            ldap_parse_do(&mut parser, vm, next)?;
            rdns.insert(0, next);
        } else {
            return Err(parser.invalid(vm, "Invalid name: "));
        }
    }
    crate::util::array_list_from(vm, &rdns)
}

fn ldap_parse_do(parser: &mut LdapParser, vm: &mut Vm<'_>, rdn: ObjectRef) -> Result<(), VmError> {
    loop {
        parser.consume_whitespace();
        let type_units = parser.parse_attr_type(vm)?;
        parser.consume_whitespace();
        if parser.cur >= parser.chars.len() || parser.chars[parser.cur] != u16::from(b'=') {
            return Err(parser.invalid(vm, "Invalid name: "));
        }
        parser.cur += 1;
        parser.consume_whitespace();
        let value_units = parser.parse_attr_value(vm)?;
        parser.consume_whitespace();
        let attr_type = vm.make_string_utf16(type_units)?;
        let value_text = String::from_utf16_lossy(&value_units);
        let value = rdn_unescape(vm, &value_units, &value_text)?;
        rdn_put(vm, rdn, attr_type, value.as_ref())?;
        if parser.cur >= parser.chars.len() || parser.chars[parser.cur] != u16::from(b'+') {
            break;
        }
        parser.cur += 1;
    }
    rdn_sort(vm, rdn)
}

fn rdn_new_empty(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("javax/naming/ldap/Rdn")?;
    let rdn = vm.new_instance(class)?;
    let entries = crate::util::new_array_list(vm)?;
    ldap_set_ref(vm, rdn, "entries", "Ljava/util/ArrayList;", entries)?;
    Ok(rdn)
}

// -------------------------------------------------------------------------------------------
// javax.naming.InvalidNameException and the Name / enumeration interfaces
// -------------------------------------------------------------------------------------------

/// `javax.naming.InvalidNameException`: a name violates the naming syntax.
pub(crate) const INVALID_NAME_EXCEPTION: oxjvm_vm::NativeClass = class(
    "javax/naming/InvalidNameException",
    Some("javax/naming/NamingException"),
    &[],
    ACC_PUBLIC,
    &[],
    &INVALID_NAME_EXCEPTION_METHODS,
    None,
);

const INVALID_NAME_EXCEPTION_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
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

/// `javax.naming.Name`: the generic name interface implemented by `LdapName`.
pub(crate) const NAME: oxjvm_vm::NativeClass = class(
    "javax/naming/Name",
    Some("java/lang/Object"),
    &[
        "java/lang/Cloneable",
        "java/io/Serializable",
        "java/lang/Comparable",
    ],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &NAME_METHODS,
    None,
);

const NAME_METHODS: [oxjvm_vm::NativeMethodDef; 15] = [
    method(
        "clone",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "compareTo",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "size",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "isEmpty",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getAll",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "get",
        "(I)Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getPrefix",
        "(I)Ljavax/naming/Name;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getSuffix",
        "(I)Ljavax/naming/Name;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "startsWith",
        "(Ljavax/naming/Name;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "endsWith",
        "(Ljavax/naming/Name;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "addAll",
        "(Ljavax/naming/Name;)Ljavax/naming/Name;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "addAll",
        "(ILjavax/naming/Name;)Ljavax/naming/Name;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "add",
        "(Ljava/lang/String;)Ljavax/naming/Name;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "add",
        "(ILjava/lang/String;)Ljavax/naming/Name;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "remove",
        "(I)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

/// `javax.naming.NamingEnumeration`: an enumeration that may throw
/// `NamingException` and be closed.
pub(crate) const NAMING_ENUMERATION: oxjvm_vm::NativeClass = class(
    "javax/naming/NamingEnumeration",
    Some("java/lang/Object"),
    &["java/util/Enumeration"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &NAMING_ENUMERATION_METHODS,
    None,
);

const NAMING_ENUMERATION_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method(
        "hasMore",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "next",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "close",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

/// `javax.naming.directory.Attribute`: one named, possibly multi-valued
/// directory attribute.
pub(crate) const ATTRIBUTE: oxjvm_vm::NativeClass = class(
    "javax/naming/directory/Attribute",
    Some("java/lang/Object"),
    &["java/lang/Cloneable", "java/io/Serializable"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &ATTRIBUTE_METHODS,
    None,
);

const ATTRIBUTE_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
    method(
        "clone",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getAll",
        "()Ljavax/naming/NamingEnumeration;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "get",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "size",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getID",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "contains",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "add",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "remove",
        "(Ljava/lang/Object;)Z",
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
        "isOrdered",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "get",
        "(I)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "remove",
        "(I)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "add",
        "(ILjava/lang/Object;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "set",
        "(ILjava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

/// `javax.naming.directory.Attributes`: a set of directory attributes.
pub(crate) const ATTRIBUTES: oxjvm_vm::NativeClass = class(
    "javax/naming/directory/Attributes",
    Some("java/lang/Object"),
    &["java/lang/Cloneable", "java/io/Serializable"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &ATTRIBUTES_METHODS,
    None,
);

const ATTRIBUTES_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method(
        "clone",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "isCaseIgnored",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "size",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "get",
        "(Ljava/lang/String;)Ljavax/naming/directory/Attribute;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getAll",
        "()Ljavax/naming/NamingEnumeration;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getIDs",
        "()Ljavax/naming/NamingEnumeration;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "put",
        "(Ljava/lang/String;Ljava/lang/Object;)Ljavax/naming/directory/Attribute;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "put",
        "(Ljavax/naming/directory/Attribute;)Ljavax/naming/directory/Attribute;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "remove",
        "(Ljava/lang/String;)Ljavax/naming/directory/Attribute;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];
// -------------------------------------------------------------------------------------------
// javax.naming.ldap.LdapName
// -------------------------------------------------------------------------------------------

/// `javax.naming.ldap.LdapName`: an RFC 2253 distinguished name, stored as a
/// list of `Rdn`s (index 0 is the rightmost) plus a cached unparsed form.
pub(crate) const LDAP_NAME: oxjvm_vm::NativeClass = class(
    "javax/naming/ldap/LdapName",
    Some("java/lang/Object"),
    &["javax/naming/Name"],
    ACC_PUBLIC,
    &[
        field("rdns", "Ljava/util/ArrayList;", ACC_PRIVATE, None),
        field("unparsed", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &LDAP_NAME_METHODS,
    None,
);

const LDAP_NAME_METHODS: [oxjvm_vm::NativeMethodDef; 28] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = ref_arg(args, 1);
            if text.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let rdns = ldap_parse_dn(vm, text)?;
            ldap_set_ref(vm, this, "rdns", "Ljava/util/ArrayList;", rdns)?;
            ldap_set_ref(vm, this, "unparsed", "Ljava/lang/String;", text)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/util/List;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let list = ref_arg(args, 1);
            if list.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let rdn_class = vm.resolve_class("javax/naming/ldap/Rdn")?;
            let size = ldap_call(vm, list, "size", "()I", vec![])?.as_int();
            let mut elements: Vec<ObjectRef> = Vec::new();
            for index in 0..size {
                let element = ldap_call(
                    vm,
                    list,
                    "get",
                    "(I)Ljava/lang/Object;",
                    vec![Value::Int(index)],
                )?
                .as_ref();
                if !vm.is_instance(element, rdn_class) {
                    let element_text = object_text(vm, element)?;
                    return Err(vm.throw_new(
                        "java/lang/IllegalArgumentException",
                        Some(&alloc::format!(
                            "Entry:{element_text}  not a valid type;list entries must be of type Rdn"
                        )),
                    ));
                }
                elements.push(element);
            }
            let rdns = crate::util::array_list_from(vm, &elements)?;
            ldap_set_ref(vm, this, "rdns", "Ljava/util/ArrayList;", rdns)?;
            ldap_set_ref(vm, this, "unparsed", "Ljava/lang/String;", ObjectRef::NULL)?;
            void()
        },
    ),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        let elements = ldap_rdns(vm, receiver(args));
        int(elements.len() as i32)
    }),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        let elements = ldap_rdns(vm, receiver(args));
        boolean(elements.is_empty())
    }),
    method(
        "getAll",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let enumeration =
                ldap_name_enum_new(vm, ldap_read_ref(vm, this, "rdns", "Ljava/util/ArrayList;"))?;
            object(enumeration)
        },
    ),
    method("get", "(I)Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let list = ldap_read_ref(vm, this, "rdns", "Ljava/util/ArrayList;");
        let rdn = ldap_call(
            vm,
            list,
            "get",
            "(I)Ljava/lang/Object;",
            vec![Value::Int(int_arg(args, 1))],
        )?
        .as_ref();
        ldap_call(vm, rdn, "toString", "()Ljava/lang/String;", vec![])
    }),
    method(
        "getRdn",
        "(I)Ljavax/naming/ldap/Rdn;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let list = ldap_read_ref(vm, this, "rdns", "Ljava/util/ArrayList;");
            ldap_call(
                vm,
                list,
                "get",
                "(I)Ljava/lang/Object;",
                vec![Value::Int(int_arg(args, 1))],
            )
        },
    ),
    method(
        "getPrefix",
        "(I)Ljavax/naming/Name;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let posn = int_arg(args, 1);
            let elements = ldap_rdns(vm, this);
            let size = elements.len() as i32;
            if posn < 0 || posn > size {
                return Err(vm.throw_new(
                    "java/lang/IndexOutOfBoundsException",
                    Some(&alloc::format!("Posn: {posn}, Size: {size}")),
                ));
            }
            let name = ldap_name_from(vm, ObjectRef::NULL, &elements[..posn as usize])?;
            object(name)
        },
    ),
    method(
        "getSuffix",
        "(I)Ljavax/naming/Name;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let posn = int_arg(args, 1);
            let elements = ldap_rdns(vm, this);
            let size = elements.len() as i32;
            if posn < 0 || posn > size {
                return Err(vm.throw_new(
                    "java/lang/IndexOutOfBoundsException",
                    Some(&alloc::format!("Posn: {posn}, Size: {size}")),
                ));
            }
            let name = ldap_name_from(vm, ObjectRef::NULL, &elements[posn as usize..])?;
            object(name)
        },
    ),
    method(
        "startsWith",
        "(Ljavax/naming/Name;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let result = ldap_starts_ends(vm, receiver(args), ref_arg(args, 1), true)?;
            boolean(result)
        },
    ),
    method(
        "startsWith",
        "(Ljava/util/List;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let result = ldap_starts_ends_list(vm, receiver(args), ref_arg(args, 1), true)?;
            boolean(result)
        },
    ),
    method(
        "endsWith",
        "(Ljavax/naming/Name;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let result = ldap_starts_ends(vm, receiver(args), ref_arg(args, 1), false)?;
            boolean(result)
        },
    ),
    method(
        "endsWith",
        "(Ljava/util/List;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let result = ldap_starts_ends_list(vm, receiver(args), ref_arg(args, 1), false)?;
            boolean(result)
        },
    ),
    method(
        "addAll",
        "(Ljavax/naming/Name;)Ljavax/naming/Name;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let size = ldap_rdns(vm, this).len() as i32;
            ldap_call(
                vm,
                this,
                "addAll",
                "(ILjavax/naming/Name;)Ljavax/naming/Name;",
                vec![Value::Int(size), Value::Ref(ref_arg(args, 1))],
            )
        },
    ),
    method(
        "addAll",
        "(Ljava/util/List;)Ljavax/naming/Name;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let size = ldap_rdns(vm, this).len() as i32;
            ldap_call(
                vm,
                this,
                "addAll",
                "(ILjava/util/List;)Ljavax/naming/Name;",
                vec![Value::Int(size), Value::Ref(ref_arg(args, 1))],
            )
        },
    ),
    method(
        "addAll",
        "(ILjavax/naming/Name;)Ljavax/naming/Name;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let mut posn = int_arg(args, 1);
            let suffix = ref_arg(args, 2);
            if suffix.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            ldap_set_ref(vm, this, "unparsed", "Ljava/lang/String;", ObjectRef::NULL)?;
            let list = ldap_read_ref(vm, this, "rdns", "Ljava/util/ArrayList;");
            let ldap_class = vm.resolve_class("javax/naming/ldap/LdapName")?;
            if vm.is_instance(suffix, ldap_class) {
                let other = ldap_read_ref(vm, suffix, "rdns", "Ljava/util/ArrayList;");
                ldap_call(
                    vm,
                    list,
                    "addAll",
                    "(ILjava/util/Collection;)Z",
                    vec![Value::Int(posn), Value::Ref(other)],
                )?;
            } else {
                let comps =
                    ldap_call(vm, suffix, "getAll", "()Ljava/util/Enumeration;", vec![])?.as_ref();
                while ldap_call(vm, comps, "hasMoreElements", "()Z", vec![])?.as_boolean() {
                    let comp = ldap_call(vm, comps, "nextElement", "()Ljava/lang/Object;", vec![])?
                        .as_ref();
                    let rdn = rdn_new_empty(vm)?;
                    ldap_parse_rdn_into(vm, comp, rdn)?;
                    ldap_call(
                        vm,
                        list,
                        "add",
                        "(ILjava/lang/Object;)V",
                        vec![Value::Int(posn), Value::Ref(rdn)],
                    )?;
                    posn += 1;
                }
            }
            object(this)
        },
    ),
    method(
        "addAll",
        "(ILjava/util/List;)Ljavax/naming/Name;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let posn = int_arg(args, 1);
            let suffix = ref_arg(args, 2);
            if suffix.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            ldap_set_ref(vm, this, "unparsed", "Ljava/lang/String;", ObjectRef::NULL)?;
            let list = ldap_read_ref(vm, this, "rdns", "Ljava/util/ArrayList;");
            let rdn_class = vm.resolve_class("javax/naming/ldap/Rdn")?;
            let size = ldap_call(vm, suffix, "size", "()I", vec![])?.as_int();
            for index in 0..size {
                let element = ldap_call(
                    vm,
                    suffix,
                    "get",
                    "(I)Ljava/lang/Object;",
                    vec![Value::Int(index)],
                )?
                .as_ref();
                if !vm.is_instance(element, rdn_class) {
                    let element_text = object_text(vm, element)?;
                    return Err(vm.throw_new(
                        "java/lang/IllegalArgumentException",
                        Some(&alloc::format!(
                            "Entry:{element_text}  not a valid type;suffix list entries must be of type Rdn"
                        )),
                    ));
                }
                ldap_call(
                    vm,
                    list,
                    "add",
                    "(ILjava/lang/Object;)V",
                    vec![Value::Int(index + posn), Value::Ref(element)],
                )?;
            }
            object(this)
        },
    ),
    method(
        "add",
        "(Ljava/lang/String;)Ljavax/naming/Name;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let size = ldap_rdns(vm, this).len() as i32;
            ldap_call(
                vm,
                this,
                "add",
                "(ILjava/lang/String;)Ljavax/naming/Name;",
                vec![Value::Int(size), Value::Ref(ref_arg(args, 1))],
            )
        },
    ),
    method(
        "add",
        "(Ljavax/naming/ldap/Rdn;)Ljavax/naming/Name;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let size = ldap_rdns(vm, this).len() as i32;
            ldap_call(
                vm,
                this,
                "add",
                "(ILjavax/naming/ldap/Rdn;)Ljavax/naming/Name;",
                vec![Value::Int(size), Value::Ref(ref_arg(args, 1))],
            )
        },
    ),
    method(
        "add",
        "(ILjava/lang/String;)Ljavax/naming/Name;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let posn = int_arg(args, 1);
            let comp = ref_arg(args, 2);
            let rdn = rdn_new_empty(vm)?;
            ldap_parse_rdn_into(vm, comp, rdn)?;
            let list = ldap_read_ref(vm, this, "rdns", "Ljava/util/ArrayList;");
            ldap_call(
                vm,
                list,
                "add",
                "(ILjava/lang/Object;)V",
                vec![Value::Int(posn), Value::Ref(rdn)],
            )?;
            ldap_set_ref(vm, this, "unparsed", "Ljava/lang/String;", ObjectRef::NULL)?;
            object(this)
        },
    ),
    method(
        "add",
        "(ILjavax/naming/ldap/Rdn;)Ljavax/naming/Name;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let posn = int_arg(args, 1);
            let comp = ref_arg(args, 2);
            if comp.is_null() {
                return Err(vm.throw_new(
                    "java/lang/NullPointerException",
                    Some("Cannot set comp to null"),
                ));
            }
            let list = ldap_read_ref(vm, this, "rdns", "Ljava/util/ArrayList;");
            ldap_call(
                vm,
                list,
                "add",
                "(ILjava/lang/Object;)V",
                vec![Value::Int(posn), Value::Ref(comp)],
            )?;
            ldap_set_ref(vm, this, "unparsed", "Ljava/lang/String;", ObjectRef::NULL)?;
            object(this)
        },
    ),
    method(
        "remove",
        "(I)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            ldap_set_ref(vm, this, "unparsed", "Ljava/lang/String;", ObjectRef::NULL)?;
            let list = ldap_read_ref(vm, this, "rdns", "Ljava/util/ArrayList;");
            let rdn = ldap_call(
                vm,
                list,
                "remove",
                "(I)Ljava/lang/Object;",
                vec![Value::Int(int_arg(args, 1))],
            )?
            .as_ref();
            ldap_call(vm, rdn, "toString", "()Ljava/lang/String;", vec![])
        },
    ),
    method(
        "getRdns",
        "()Ljava/util/List;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let list = ldap_read_ref(vm, this, "rdns", "Ljava/util/ArrayList;");
            let collections = vm.resolve_class("java/util/Collections")?;
            let (declaring, method) = vm
                .find_method(
                    collections,
                    "unmodifiableList",
                    "(Ljava/util/List;)Ljava/util/List;",
                )
                .ok_or_else(|| VmError::internal("Collections.unmodifiableList missing"))?;
            vm.invoke_method(declaring, method, vec![Value::Ref(list)])
        },
    ),
    method(
        "clone",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let elements = ldap_rdns(vm, this);
            let unparsed = ldap_read_ref(vm, this, "unparsed", "Ljava/lang/String;");
            let copy = ldap_name_from(vm, unparsed, &elements)?;
            object(copy)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let unparsed = ldap_read_ref(vm, this, "unparsed", "Ljava/lang/String;");
            if !unparsed.is_null() {
                return object(unparsed);
            }
            let elements = ldap_rdns(vm, this);
            let mut units: Vec<u16> = Vec::new();
            if !elements.is_empty() {
                let last = elements.len() - 1;
                let text = ldap_call(
                    vm,
                    elements[last],
                    "toString",
                    "()Ljava/lang/String;",
                    vec![],
                )?
                .as_ref();
                units.extend(ldap_string_units(vm, text));
                for index in (0..last).rev() {
                    units.push(u16::from(b','));
                    let text = ldap_call(
                        vm,
                        elements[index],
                        "toString",
                        "()Ljava/lang/String;",
                        vec![],
                    )?
                    .as_ref();
                    units.extend(ldap_string_units(vm, text));
                }
            }
            let text = vm.make_string_utf16(units)?;
            ldap_set_ref(vm, this, "unparsed", "Ljava/lang/String;", text)?;
            object(text)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let result = ldap_name_equals(vm, receiver(args), ref_arg(args, 1))?;
            boolean(result)
        },
    ),
    method(
        "compareTo",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let result = ldap_name_compare(vm, receiver(args), ref_arg(args, 1))?;
            int(result)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let elements = ldap_rdns(vm, receiver(args));
        let mut hash: i32 = 0;
        for rdn in elements {
            hash = hash.wrapping_add(rdn_hash(vm, rdn)?);
        }
        int(hash)
    }),
];
// -------------------------------------------------------------------------------------------
// LdapName helpers
// -------------------------------------------------------------------------------------------

fn ldap_rdns(vm: &Vm<'_>, name: ObjectRef) -> Vec<ObjectRef> {
    let list = ldap_read_ref(vm, name, "rdns", "Ljava/util/ArrayList;");
    crate::util::array_list_elements(vm, list)
}

fn ldap_name_from(
    vm: &mut Vm<'_>,
    unparsed: ObjectRef,
    elements: &[ObjectRef],
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("javax/naming/ldap/LdapName")?;
    let name = vm.new_instance(class)?;
    let rdns = crate::util::array_list_from(vm, elements)?;
    ldap_set_ref(vm, name, "rdns", "Ljava/util/ArrayList;", rdns)?;
    ldap_set_ref(vm, name, "unparsed", "Ljava/lang/String;", unparsed)?;
    Ok(name)
}

/// Shared `startsWith`/`endsWith(Name)` body.
fn ldap_starts_ends(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    other: ObjectRef,
    prefix: bool,
) -> Result<bool, VmError> {
    if other.is_null() {
        return Ok(false);
    }
    let mine = ldap_rdns(vm, this);
    let len2 = ldap_call(vm, other, "size", "()I", vec![])?.as_int();
    if len2 < 0 || mine.len() < len2 as usize {
        return Ok(false);
    }
    let beg = if prefix {
        0
    } else {
        mine.len() - len2 as usize
    };
    ldap_name_matches(vm, &mine, beg, beg + len2 as usize, other)
}

/// Shared `startsWith`/`endsWith(List<Rdn>)` body.
fn ldap_starts_ends_list(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    list: ObjectRef,
    prefix: bool,
) -> Result<bool, VmError> {
    if list.is_null() {
        return Ok(false);
    }
    let mine = ldap_rdns(vm, this);
    let size = ldap_call(vm, list, "size", "()I", vec![])?.as_int();
    if size < 0 || mine.len() < size as usize {
        return Ok(false);
    }
    let mut theirs: Vec<ObjectRef> = Vec::new();
    for index in 0..size {
        theirs.push(
            ldap_call(
                vm,
                list,
                "get",
                "(I)Ljava/lang/Object;",
                vec![Value::Int(index)],
            )?
            .as_ref(),
        );
    }
    let beg = if prefix {
        0
    } else {
        mine.len() - size as usize
    };
    for index in 0..theirs.len() {
        if !rdn_equals(vm, mine[beg + index], theirs[index])? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Compare components `[beg,end)` against the name `other`; non-LdapName
/// components are parsed as RDN strings, and a parse failure means no match.
fn ldap_name_matches(
    vm: &mut Vm<'_>,
    mine: &[ObjectRef],
    beg: usize,
    end: usize,
    other: ObjectRef,
) -> Result<bool, VmError> {
    let ldap_class = vm.resolve_class("javax/naming/ldap/LdapName")?;
    if vm.is_instance(other, ldap_class) {
        let theirs = ldap_rdns(vm, other);
        for index in beg..end {
            if !rdn_equals(vm, mine[index], theirs[index - beg])? {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    let invalid_class = vm.resolve_class("javax/naming/InvalidNameException")?;
    for (offset, &mine_rdn) in mine[beg..end].iter().enumerate() {
        let comp = ldap_call(
            vm,
            other,
            "get",
            "(I)Ljava/lang/String;",
            vec![Value::Int(offset as i32)],
        )?
        .as_ref();
        let rdn = rdn_new_empty(vm)?;
        match ldap_parse_rdn_into(vm, comp, rdn) {
            Ok(()) => {}
            Err(VmError::Thrown(exception)) => {
                if vm.is_instance(exception, invalid_class) {
                    vm.discard_backtrace(exception);
                    return Ok(false);
                }
                return Err(VmError::Thrown(exception));
            }
            Err(other_error) => return Err(other_error),
        }
        if !rdn_equals(vm, rdn, mine_rdn)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn ldap_name_equals(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<bool, VmError> {
    if a == b {
        return Ok(true);
    }
    let ldap_class = vm.resolve_class("javax/naming/ldap/LdapName")?;
    if !vm.is_instance(b, ldap_class) {
        return Ok(false);
    }
    let entries_a = ldap_rdns(vm, a);
    let entries_b = ldap_rdns(vm, b);
    if entries_a.len() != entries_b.len() {
        return Ok(false);
    }
    let unparsed_a = ldap_read_ref(vm, a, "unparsed", "Ljava/lang/String;");
    let unparsed_b = ldap_read_ref(vm, b, "unparsed", "Ljava/lang/String;");
    if !unparsed_a.is_null()
        && !unparsed_b.is_null()
        && ldap_call(
            vm,
            unparsed_a,
            "equalsIgnoreCase",
            "(Ljava/lang/String;)Z",
            vec![Value::Ref(unparsed_b)],
        )?
        .as_boolean()
    {
        return Ok(true);
    }
    for index in 0..entries_a.len() {
        if !rdn_equals(vm, entries_a[index], entries_b[index])? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn ldap_name_compare(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<i32, VmError> {
    let ldap_class = vm.resolve_class("javax/naming/ldap/LdapName")?;
    if !vm.is_instance(b, ldap_class) {
        return Err(vm.throw_new(
            "java/lang/ClassCastException",
            Some("The obj is not a LdapName"),
        ));
    }
    if a == b {
        return Ok(0);
    }
    let unparsed_a = ldap_read_ref(vm, a, "unparsed", "Ljava/lang/String;");
    let unparsed_b = ldap_read_ref(vm, b, "unparsed", "Ljava/lang/String;");
    if !unparsed_a.is_null()
        && !unparsed_b.is_null()
        && ldap_call(
            vm,
            unparsed_a,
            "equalsIgnoreCase",
            "(Ljava/lang/String;)Z",
            vec![Value::Ref(unparsed_b)],
        )?
        .as_boolean()
    {
        return Ok(0);
    }
    let entries_a = ldap_rdns(vm, a);
    let entries_b = ldap_rdns(vm, b);
    let min_size = entries_a.len().min(entries_b.len());
    for index in 0..min_size {
        let diff = rdn_compare(vm, entries_a[index], entries_b[index])?;
        if diff != 0 {
            return Ok(diff);
        }
    }
    Ok((entries_a.len() - entries_b.len()) as i32)
}

// -------------------------------------------------------------------------------------------
// javax.naming.directory.BasicAttribute
// -------------------------------------------------------------------------------------------

/// `javax.naming.directory.BasicAttribute`: an attribute id plus an ordered
/// or unordered vector of values.
pub(crate) const BASIC_ATTRIBUTE: oxjvm_vm::NativeClass = class(
    "javax/naming/directory/BasicAttribute",
    Some("java/lang/Object"),
    &["javax/naming/directory/Attribute"],
    ACC_PUBLIC,
    &[
        field("attrID", "Ljava/lang/String;", ACC_PROTECTED, None),
        field("values", "Ljava/util/Vector;", ACC_PROTECTED, None),
        field("ordered", "Z", ACC_PROTECTED, None),
    ],
    &BASIC_ATTRIBUTE_METHODS,
    None,
);

const BASIC_ATTRIBUTE_METHODS: [oxjvm_vm::NativeMethodDef; 21] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            ldap_self_call(
                vm,
                this,
                "<init>",
                "(Ljava/lang/String;Z)V",
                vec![Value::Ref(ref_arg(args, 1)), Value::Int(0)],
            )?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            ldap_self_call(
                vm,
                this,
                "<init>",
                "(Ljava/lang/String;Ljava/lang/Object;Z)V",
                vec![
                    Value::Ref(ref_arg(args, 1)),
                    Value::Ref(ref_arg(args, 2)),
                    Value::Int(0),
                ],
            )?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Z)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            ldap_set_ref(vm, this, "attrID", "Ljava/lang/String;", ref_arg(args, 1))?;
            let values = ldap_new_vector(vm)?;
            ldap_set_ref(vm, this, "values", "Ljava/util/Vector;", values)?;
            crate::lang::write_int_field(vm, this, "ordered", int_arg(args, 2));
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/Object;Z)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            ldap_self_call(
                vm,
                this,
                "<init>",
                "(Ljava/lang/String;Z)V",
                vec![Value::Ref(ref_arg(args, 1)), Value::Int(int_arg(args, 3))],
            )?;
            let values = ldap_read_ref(vm, this, "values", "Ljava/util/Vector;");
            ldap_call(
                vm,
                values,
                "addElement",
                "(Ljava/lang/Object;)V",
                vec![Value::Ref(ref_arg(args, 2))],
            )?;
            void()
        },
    ),
    method(
        "clone",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let attr_id = ldap_read_ref(vm, this, "attrID", "Ljava/lang/String;");
            let ordered = crate::lang::read_int_field(vm, this, "ordered");
            let class = vm.class_of(this);
            let copy = vm.new_instance(class)?;
            ldap_self_call(
                vm,
                copy,
                "<init>",
                "(Ljava/lang/String;Z)V",
                vec![Value::Ref(attr_id), Value::Int(ordered)],
            )?;
            let values = ldap_read_ref(vm, this, "values", "Ljava/util/Vector;");
            let copy_values = ldap_read_ref(vm, copy, "values", "Ljava/util/Vector;");
            let size = ldap_call(vm, values, "size", "()I", vec![])?.as_int();
            for index in 0..size {
                let value = ldap_call(
                    vm,
                    values,
                    "elementAt",
                    "(I)Ljava/lang/Object;",
                    vec![Value::Int(index)],
                )?
                .as_ref();
                ldap_call(
                    vm,
                    copy_values,
                    "addElement",
                    "(Ljava/lang/Object;)V",
                    vec![Value::Ref(value)],
                )?;
            }
            object(copy)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            match basic_attribute_equals(vm, this, other) {
                Ok(result) => boolean(result),
                Err(error) => boolean(swallow_naming(vm, error)?),
            }
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let result = basic_attribute_hash(vm, receiver(args))?;
        int(result)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let attr_id = ldap_read_ref(vm, this, "attrID", "Ljava/lang/String;");
            let mut text = vm.string_value(attr_id).unwrap_or_default();
            text.push_str(": ");
            let values = ldap_read_ref(vm, this, "values", "Ljava/util/Vector;");
            let size = ldap_call(vm, values, "size", "()I", vec![])?.as_int();
            if size == 0 {
                text.push_str("No values");
            } else {
                for index in 0..size {
                    if index > 0 {
                        text.push_str(", ");
                    }
                    let value = ldap_call(
                        vm,
                        values,
                        "elementAt",
                        "(I)Ljava/lang/Object;",
                        vec![Value::Int(index)],
                    )?
                    .as_ref();
                    if value.is_null() {
                        text.push_str("null");
                    } else {
                        text.push_str(&object_text(vm, value)?);
                    }
                }
            }
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "getAll",
        "()Ljavax/naming/NamingEnumeration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let values = ldap_read_ref(vm, this, "values", "Ljava/util/Vector;");
            let elements =
                ldap_call(vm, values, "elements", "()Ljava/util/Enumeration;", vec![])?.as_ref();
            let enumeration = ldap_enum_new(
                vm,
                "javax/naming/directory/BasicAttribute$ValuesEnumImpl",
                "list",
                elements,
            )?;
            object(enumeration)
        },
    ),
    method("get", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let values = ldap_read_ref(vm, this, "values", "Ljava/util/Vector;");
        let size = ldap_call(vm, values, "size", "()I", vec![])?.as_int();
        if size == 0 {
            let attr_id = ldap_read_ref(vm, this, "attrID", "Ljava/lang/String;");
            return Err(vm.throw_new(
                "java/util/NoSuchElementException",
                Some(&alloc::format!(
                    "Attribute {} has no value",
                    vm.string_value(attr_id).unwrap_or_default()
                )),
            ));
        }
        ldap_call(
            vm,
            values,
            "elementAt",
            "(I)Ljava/lang/Object;",
            vec![Value::Int(0)],
        )
    }),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let values = ldap_read_ref(vm, this, "values", "Ljava/util/Vector;");
        ldap_call(vm, values, "size", "()I", vec![])
    }),
    method(
        "getID",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            object(ldap_read_ref(vm, this, "attrID", "Ljava/lang/String;"))
        },
    ),
    method(
        "contains",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let index = basic_attribute_find(vm, this, ref_arg(args, 1))?;
            boolean(index >= 0)
        },
    ),
    method("add", "(Ljava/lang/Object;)Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let value = ref_arg(args, 1);
        let ordered = crate::lang::read_int_field(vm, this, "ordered") != 0;
        if ordered || basic_attribute_find(vm, this, value)? < 0 {
            let values = ldap_read_ref(vm, this, "values", "Ljava/util/Vector;");
            ldap_call(
                vm,
                values,
                "addElement",
                "(Ljava/lang/Object;)V",
                vec![Value::Ref(value)],
            )?;
            boolean(true)
        } else {
            boolean(false)
        }
    }),
    method(
        "remove",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let index = basic_attribute_find(vm, this, ref_arg(args, 1))?;
            if index >= 0 {
                let values = ldap_read_ref(vm, this, "values", "Ljava/util/Vector;");
                ldap_call(
                    vm,
                    values,
                    "removeElementAt",
                    "(I)V",
                    vec![Value::Int(index)],
                )?;
                boolean(true)
            } else {
                boolean(false)
            }
        },
    ),
    method("clear", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let values = ldap_read_ref(vm, this, "values", "Ljava/util/Vector;");
        ldap_call(vm, values, "clear", "()V", vec![])?;
        void()
    }),
    method("isOrdered", "()Z", ACC_PUBLIC, |vm, _, args| {
        let ordered = crate::lang::read_int_field(vm, receiver(args), "ordered") != 0;
        boolean(ordered)
    }),
    method("get", "(I)Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let values = ldap_read_ref(vm, this, "values", "Ljava/util/Vector;");
        ldap_call(
            vm,
            values,
            "elementAt",
            "(I)Ljava/lang/Object;",
            vec![Value::Int(int_arg(args, 1))],
        )
    }),
    method(
        "remove",
        "(I)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let index = int_arg(args, 1);
            let values = ldap_read_ref(vm, this, "values", "Ljava/util/Vector;");
            let answer = ldap_call(
                vm,
                values,
                "elementAt",
                "(I)Ljava/lang/Object;",
                vec![Value::Int(index)],
            )?;
            ldap_call(
                vm,
                values,
                "removeElementAt",
                "(I)V",
                vec![Value::Int(index)],
            )?;
            object(answer.as_ref())
        },
    ),
    method(
        "add",
        "(ILjava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let index = int_arg(args, 1);
            let value = ref_arg(args, 2);
            let ordered = crate::lang::read_int_field(vm, this, "ordered") != 0;
            if !ordered && basic_attribute_find(vm, this, value)? >= 0 {
                return Err(vm.throw_new(
                    "java/lang/IllegalStateException",
                    Some("Cannot add duplicate to unordered attribute"),
                ));
            }
            let values = ldap_read_ref(vm, this, "values", "Ljava/util/Vector;");
            ldap_call(
                vm,
                values,
                "insertElementAt",
                "(Ljava/lang/Object;I)V",
                vec![Value::Ref(value), Value::Int(index)],
            )?;
            void()
        },
    ),
    method(
        "set",
        "(ILjava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let index = int_arg(args, 1);
            let value = ref_arg(args, 2);
            let ordered = crate::lang::read_int_field(vm, this, "ordered") != 0;
            if !ordered && basic_attribute_find(vm, this, value)? >= 0 {
                return Err(vm.throw_new(
                    "java/lang/IllegalStateException",
                    Some("Cannot add duplicate to unordered attribute"),
                ));
            }
            let values = ldap_read_ref(vm, this, "values", "Ljava/util/Vector;");
            let answer = ldap_call(
                vm,
                values,
                "elementAt",
                "(I)Ljava/lang/Object;",
                vec![Value::Int(index)],
            )?;
            ldap_call(
                vm,
                values,
                "setElementAt",
                "(Ljava/lang/Object;I)V",
                vec![Value::Ref(value), Value::Int(index)],
            )?;
            object(answer.as_ref())
        },
    ),
];
// -------------------------------------------------------------------------------------------
// BasicAttribute helpers
// -------------------------------------------------------------------------------------------

fn ldap_new_vector(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/Vector")?;
    let instance = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(class, "<init>", "()V")
        .ok_or_else(|| VmError::internal("Vector() missing"))?;
    vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
    Ok(instance)
}

/// `BasicAttribute.find`: locate a value, comparing arrays element-wise.
fn basic_attribute_find(
    vm: &mut Vm<'_>,
    attr: ObjectRef,
    target: ObjectRef,
) -> Result<i32, VmError> {
    let values = ldap_read_ref(vm, attr, "values", "Ljava/util/Vector;");
    let size = ldap_call(vm, values, "size", "()I", vec![])?.as_int();
    if target.is_null() {
        for index in 0..size {
            let element = ldap_call(
                vm,
                values,
                "elementAt",
                "(I)Ljava/lang/Object;",
                vec![Value::Int(index)],
            )?
            .as_ref();
            if element.is_null() {
                return Ok(index);
            }
        }
        return Ok(-1);
    }
    let target_class = vm.class_of(target);
    if vm.class_name(target_class).starts_with('[') {
        for index in 0..size {
            let element = ldap_call(
                vm,
                values,
                "elementAt",
                "(I)Ljava/lang/Object;",
                vec![Value::Int(index)],
            )?
            .as_ref();
            if !element.is_null()
                && vm.class_of(element) == target_class
                && ldap_array_equals(vm, target, element)?
            {
                return Ok(index);
            }
        }
        return Ok(-1);
    }
    Ok(ldap_call(
        vm,
        values,
        "indexOf",
        "(Ljava/lang/Object;I)I",
        vec![Value::Ref(target), Value::Int(0)],
    )?
    .as_int())
}

fn ldap_array_equals(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<bool, VmError> {
    let len = vm.array_length(a)?;
    if len != vm.array_length(b)? {
        return Ok(false);
    }
    if vm.class_name(vm.class_of(a)) == "[B" {
        return Ok(crate::lang::array_bytes(vm, a)? == crate::lang::array_bytes(vm, b)?);
    }
    for index in 0..len {
        let value_a = vm.array_get(a, index as i32)?;
        let value_b = vm.array_get(b, index as i32)?;
        let equal = match (value_a, value_b) {
            (Value::Ref(x), Value::Ref(y)) => ldap_ref_equals(vm, x, y)?,
            (Value::Ref(_), _) | (_, Value::Ref(_)) => false,
            _ => value_a == value_b,
        };
        if !equal {
            return Ok(false);
        }
    }
    Ok(true)
}

/// `BasicAttribute.valueEquals`: identity, then arrays element-wise, then
/// `Object.equals`.
fn ldap_ref_equals(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<bool, VmError> {
    if a == b {
        return Ok(true);
    }
    if a.is_null() || b.is_null() {
        return Ok(false);
    }
    if vm.class_name(vm.class_of(a)).starts_with('[')
        && vm.class_name(vm.class_of(b)).starts_with('[')
    {
        return ldap_array_equals(vm, a, b);
    }
    Ok(ldap_call(
        vm,
        a,
        "equals",
        "(Ljava/lang/Object;)Z",
        vec![Value::Ref(b)],
    )?
    .as_boolean())
}

fn basic_attribute_equals(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    other: ObjectRef,
) -> Result<bool, VmError> {
    let attribute_class = vm.resolve_class("javax/naming/directory/Attribute")?;
    if !vm.is_instance(other, attribute_class) {
        return Ok(false);
    }
    let ordered = crate::lang::read_int_field(vm, this, "ordered") != 0;
    let their_ordered = ldap_call(vm, other, "isOrdered", "()Z", vec![])?.as_boolean();
    if ordered != their_ordered {
        return Ok(false);
    }
    let attr_id = ldap_read_ref(vm, this, "attrID", "Ljava/lang/String;");
    let their_id = ldap_call(vm, other, "getID", "()Ljava/lang/String;", vec![])?.as_ref();
    if !ldap_call(
        vm,
        attr_id,
        "equals",
        "(Ljava/lang/Object;)Z",
        vec![Value::Ref(their_id)],
    )?
    .as_boolean()
    {
        return Ok(false);
    }
    let values = ldap_read_ref(vm, this, "values", "Ljava/util/Vector;");
    let len = ldap_call(vm, values, "size", "()I", vec![])?.as_int();
    let their_len = ldap_call(vm, other, "size", "()I", vec![])?.as_int();
    if len != their_len {
        return Ok(false);
    }
    if ordered {
        for index in 0..len {
            let mine = ldap_call(
                vm,
                values,
                "elementAt",
                "(I)Ljava/lang/Object;",
                vec![Value::Int(index)],
            )?
            .as_ref();
            let theirs = ldap_call(
                vm,
                other,
                "get",
                "(I)Ljava/lang/Object;",
                vec![Value::Int(index)],
            )?
            .as_ref();
            if !ldap_ref_equals(vm, mine, theirs)? {
                return Ok(false);
            }
        }
    } else {
        let theirs = ldap_call(
            vm,
            other,
            "getAll",
            "()Ljavax/naming/NamingEnumeration;",
            vec![],
        )?
        .as_ref();
        while ldap_call(vm, theirs, "hasMoreElements", "()Z", vec![])?.as_boolean() {
            let value =
                ldap_call(vm, theirs, "nextElement", "()Ljava/lang/Object;", vec![])?.as_ref();
            if basic_attribute_find(vm, this, value)? < 0 {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

/// Hash of one array element as boxed by `Array.get`.
fn ldap_value_hash(vm: &mut Vm<'_>, value: Value) -> Result<i32, VmError> {
    match value {
        Value::Ref(reference) if !reference.is_null() => {
            Ok(ldap_call(vm, reference, "hashCode", "()I", vec![])?.as_int())
        }
        Value::Ref(_) => Ok(0),
        Value::Int(number) => Ok(number),
        Value::Long(number) => {
            let bits = number as u64;
            Ok((bits ^ (bits >> 32)) as i32)
        }
        Value::Float(number) => Ok(number.to_bits() as i32),
        Value::Double(number) => {
            let bits = number.to_bits();
            Ok((bits ^ (bits >> 32)) as i32)
        }
    }
}

fn basic_attribute_hash(vm: &mut Vm<'_>, attr: ObjectRef) -> Result<i32, VmError> {
    let attr_id = ldap_read_ref(vm, attr, "attrID", "Ljava/lang/String;");
    let mut hash = ldap_call(vm, attr_id, "hashCode", "()I", vec![])?.as_int();
    let values = ldap_read_ref(vm, attr, "values", "Ljava/util/Vector;");
    let size = ldap_call(vm, values, "size", "()I", vec![])?.as_int();
    for index in 0..size {
        let value = ldap_call(
            vm,
            values,
            "elementAt",
            "(I)Ljava/lang/Object;",
            vec![Value::Int(index)],
        )?
        .as_ref();
        if value.is_null() {
            continue;
        }
        let class_name = vm.class_name(vm.class_of(value));
        if class_name == "[B" {
            for byte in crate::lang::array_bytes(vm, value)? {
                hash = hash.wrapping_add(i32::from(byte as i8));
            }
        } else if class_name.starts_with('[') {
            let len = vm.array_length(value)?;
            for element in 0..len {
                let element_value = vm.array_get(value, element as i32)?;
                hash = hash.wrapping_add(ldap_value_hash(vm, element_value)?);
            }
        } else {
            hash = hash.wrapping_add(ldap_call(vm, value, "hashCode", "()I", vec![])?.as_int());
        }
    }
    Ok(hash)
}

// -------------------------------------------------------------------------------------------
// javax.naming.directory.BasicAttributes
// -------------------------------------------------------------------------------------------

/// `javax.naming.directory.BasicAttributes`: a hashtable of attributes,
/// optionally ignoring the case of attribute ids.
pub(crate) const BASIC_ATTRIBUTES: oxjvm_vm::NativeClass = class(
    "javax/naming/directory/BasicAttributes",
    Some("java/lang/Object"),
    &["javax/naming/directory/Attributes"],
    ACC_PUBLIC,
    &[
        field("ignoreCase", "Z", ACC_PRIVATE, None),
        field("attrs", "Ljava/util/Hashtable;", 0, None),
    ],
    &BASIC_ATTRIBUTES_METHODS,
    None,
);

const BASIC_ATTRIBUTES_METHODS: [oxjvm_vm::NativeMethodDef; 16] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let attrs = new_hashtable(vm, 11)?;
        ldap_set_ref(vm, this, "attrs", "Ljava/util/Hashtable;", attrs)?;
        crate::lang::write_int_field(vm, this, "ignoreCase", 0);
        void()
    }),
    method("<init>", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let attrs = new_hashtable(vm, 11)?;
        ldap_set_ref(vm, this, "attrs", "Ljava/util/Hashtable;", attrs)?;
        crate::lang::write_int_field(vm, this, "ignoreCase", int_arg(args, 1));
        void()
    }),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            ldap_self_call(vm, this, "<init>", "()V", vec![])?;
            basic_attributes_put_value(vm, this, ref_arg(args, 1), ref_arg(args, 2))?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/Object;Z)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            ldap_self_call(
                vm,
                this,
                "<init>",
                "(Z)V",
                vec![Value::Int(int_arg(args, 3))],
            )?;
            basic_attributes_put_value(vm, this, ref_arg(args, 1), ref_arg(args, 2))?;
            void()
        },
    ),
    method(
        "clone",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let copy = vm.clone_object(this)?;
            let attrs = ldap_read_ref(vm, this, "attrs", "Ljava/util/Hashtable;");
            let attrs_copy =
                ldap_call(vm, attrs, "clone", "()Ljava/lang/Object;", vec![])?.as_ref();
            ldap_set_ref(vm, copy, "attrs", "Ljava/util/Hashtable;", attrs_copy)?;
            object(copy)
        },
    ),
    method("isCaseIgnored", "()Z", ACC_PUBLIC, |vm, _, args| {
        let ignored = crate::lang::read_int_field(vm, receiver(args), "ignoreCase") != 0;
        boolean(ignored)
    }),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let attrs = ldap_read_ref(vm, this, "attrs", "Ljava/util/Hashtable;");
        ldap_call(vm, attrs, "size", "()I", vec![])
    }),
    method(
        "get",
        "(Ljava/lang/String;)Ljavax/naming/directory/Attribute;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let key = basic_attributes_key(vm, this, ref_arg(args, 1))?;
            let attrs = ldap_read_ref(vm, this, "attrs", "Ljava/util/Hashtable;");
            ldap_call(
                vm,
                attrs,
                "get",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                vec![Value::Ref(key)],
            )
        },
    ),
    method(
        "getAll",
        "()Ljavax/naming/NamingEnumeration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let attrs = ldap_read_ref(vm, this, "attrs", "Ljava/util/Hashtable;");
            let elements =
                ldap_call(vm, attrs, "elements", "()Ljava/util/Enumeration;", vec![])?.as_ref();
            let enumeration = ldap_enum_new(
                vm,
                "javax/naming/directory/BasicAttributes$AttrEnumImpl",
                "elements",
                elements,
            )?;
            object(enumeration)
        },
    ),
    method(
        "getIDs",
        "()Ljavax/naming/NamingEnumeration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let attrs = ldap_read_ref(vm, this, "attrs", "Ljava/util/Hashtable;");
            let elements =
                ldap_call(vm, attrs, "elements", "()Ljava/util/Enumeration;", vec![])?.as_ref();
            let enumeration = ldap_enum_new(
                vm,
                "javax/naming/directory/BasicAttributes$IDEnumImpl",
                "elements",
                elements,
            )?;
            object(enumeration)
        },
    ),
    method(
        "put",
        "(Ljava/lang/String;Ljava/lang/Object;)Ljavax/naming/directory/Attribute;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            basic_attributes_put_value(vm, this, ref_arg(args, 1), ref_arg(args, 2))
        },
    ),
    method(
        "put",
        "(Ljavax/naming/directory/Attribute;)Ljavax/naming/directory/Attribute;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let attr = ref_arg(args, 1);
            let id = ldap_call(vm, attr, "getID", "()Ljava/lang/String;", vec![])?.as_ref();
            let key = basic_attributes_key(vm, this, id)?;
            let attrs = ldap_read_ref(vm, this, "attrs", "Ljava/util/Hashtable;");
            ldap_call(
                vm,
                attrs,
                "put",
                "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
                vec![Value::Ref(key), Value::Ref(attr)],
            )
        },
    ),
    method(
        "remove",
        "(Ljava/lang/String;)Ljavax/naming/directory/Attribute;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let key = basic_attributes_key(vm, this, ref_arg(args, 1))?;
            let attrs = ldap_read_ref(vm, this, "attrs", "Ljava/util/Hashtable;");
            ldap_call(
                vm,
                attrs,
                "remove",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                vec![Value::Ref(key)],
            )
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let attrs = ldap_read_ref(vm, this, "attrs", "Ljava/util/Hashtable;");
            let size = ldap_call(vm, attrs, "size", "()I", vec![])?.as_int();
            if size == 0 {
                return object(vm.make_string("No attributes")?);
            }
            ldap_call(vm, attrs, "toString", "()Ljava/lang/String;", vec![])
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            match basic_attributes_equals(vm, this, other) {
                Ok(result) => boolean(result),
                Err(error) => boolean(swallow_naming(vm, error)?),
            }
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let mut hash = crate::lang::read_int_field(vm, this, "ignoreCase");
        let attrs = ldap_read_ref(vm, this, "attrs", "Ljava/util/Hashtable;");
        let elements =
            ldap_call(vm, attrs, "elements", "()Ljava/util/Enumeration;", vec![])?.as_ref();
        while ldap_call(vm, elements, "hasMoreElements", "()Z", vec![])?.as_boolean() {
            let attr =
                ldap_call(vm, elements, "nextElement", "()Ljava/lang/Object;", vec![])?.as_ref();
            hash = hash.wrapping_add(ldap_call(vm, attr, "hashCode", "()I", vec![])?.as_int());
        }
        int(hash)
    }),
];

fn basic_attributes_new(vm: &mut Vm<'_>, ignore_case: bool) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("javax/naming/directory/BasicAttributes")?;
    let attrs = vm.new_instance(class)?;
    let table = new_hashtable(vm, 11)?;
    ldap_set_ref(vm, attrs, "attrs", "Ljava/util/Hashtable;", table)?;
    crate::lang::write_int_field(vm, attrs, "ignoreCase", i32::from(ignore_case));
    Ok(attrs)
}

/// Lower-case the id when the set ignores case (English locale).
fn basic_attributes_key(
    vm: &mut Vm<'_>,
    attrs: ObjectRef,
    id: ObjectRef,
) -> Result<ObjectRef, VmError> {
    if crate::lang::read_int_field(vm, attrs, "ignoreCase") != 0 {
        ldap_to_lower_english(vm, id)
    } else {
        Ok(id)
    }
}

fn basic_attributes_put_value(
    vm: &mut Vm<'_>,
    attrs: ObjectRef,
    id: ObjectRef,
    value: ObjectRef,
) -> Result<Value, VmError> {
    let class = vm.resolve_class("javax/naming/directory/BasicAttribute")?;
    let attr = vm.new_instance(class)?;
    ldap_self_call(
        vm,
        attr,
        "<init>",
        "(Ljava/lang/String;Ljava/lang/Object;)V",
        vec![Value::Ref(id), Value::Ref(value)],
    )?;
    ldap_self_call(
        vm,
        attrs,
        "put",
        "(Ljavax/naming/directory/Attribute;)Ljavax/naming/directory/Attribute;",
        vec![Value::Ref(attr)],
    )
}

fn basic_attributes_equals(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    other: ObjectRef,
) -> Result<bool, VmError> {
    let attributes_class = vm.resolve_class("javax/naming/directory/Attributes")?;
    if !vm.is_instance(other, attributes_class) {
        return Ok(false);
    }
    let ignore_case = crate::lang::read_int_field(vm, this, "ignoreCase") != 0;
    let their_ignore = ldap_call(vm, other, "isCaseIgnored", "()Z", vec![])?.as_boolean();
    if ignore_case != their_ignore {
        return Ok(false);
    }
    let attrs = ldap_read_ref(vm, this, "attrs", "Ljava/util/Hashtable;");
    let size = ldap_call(vm, attrs, "size", "()I", vec![])?.as_int();
    let their_size = ldap_call(vm, other, "size", "()I", vec![])?.as_int();
    if size != their_size {
        return Ok(false);
    }
    let theirs = ldap_call(
        vm,
        other,
        "getAll",
        "()Ljavax/naming/NamingEnumeration;",
        vec![],
    )?
    .as_ref();
    while ldap_call(vm, theirs, "hasMore", "()Z", vec![])?.as_boolean() {
        let their = ldap_call(vm, theirs, "next", "()Ljava/lang/Object;", vec![])?.as_ref();
        let their_id = ldap_call(vm, their, "getID", "()Ljava/lang/String;", vec![])?.as_ref();
        let mine = ldap_self_call(
            vm,
            this,
            "get",
            "(Ljava/lang/String;)Ljavax/naming/directory/Attribute;",
            vec![Value::Ref(their_id)],
        )?
        .as_ref();
        if !ldap_call(
            vm,
            their,
            "equals",
            "(Ljava/lang/Object;)Z",
            vec![Value::Ref(mine)],
        )?
        .as_boolean()
        {
            return Ok(false);
        }
    }
    Ok(true)
}
// -------------------------------------------------------------------------------------------
// Naming enumerations
// -------------------------------------------------------------------------------------------

/// The anonymous `Enumeration<String>` returned by `LdapName.getAll`: walks
/// the RDN list, stringifying each component.
pub(crate) const LDAP_NAME_ENUM: oxjvm_vm::NativeClass = class(
    "javax/naming/ldap/LdapName$1",
    Some("java/lang/Object"),
    &["java/util/Enumeration"],
    ACC_SYNTHETIC,
    &[
        field("list", "Ljava/util/List;", ACC_PRIVATE, None),
        field("index", "I", ACC_PRIVATE, None),
    ],
    &LDAP_NAME_ENUM_METHODS,
    None,
);

const LDAP_NAME_ENUM_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method("<init>", "()V", 0, |_, _, _| void()),
    method("hasMoreElements", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let list = ldap_read_ref(vm, this, "list", "Ljava/util/List;");
        let index = crate::lang::read_int_field(vm, this, "index");
        let size = ldap_call(vm, list, "size", "()I", vec![])?.as_int();
        boolean(index < size)
    }),
    method(
        "nextElement",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let list = ldap_read_ref(vm, this, "list", "Ljava/util/List;");
            let index = crate::lang::read_int_field(vm, this, "index");
            let size = ldap_call(vm, list, "size", "()I", vec![])?.as_int();
            if index >= size {
                return Err(vm.throw_new("java/util/NoSuchElementException", None));
            }
            let rdn = ldap_call(
                vm,
                list,
                "get",
                "(I)Ljava/lang/Object;",
                vec![Value::Int(index)],
            )?
            .as_ref();
            crate::lang::write_int_field(vm, this, "index", index + 1);
            ldap_call(vm, rdn, "toString", "()Ljava/lang/String;", vec![])
        },
    ),
];

fn ldap_name_enum_new(vm: &mut Vm<'_>, list: ObjectRef) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("javax/naming/ldap/LdapName$1")?;
    let enumeration = vm.new_instance(class)?;
    ldap_set_ref(vm, enumeration, "list", "Ljava/util/List;", list)?;
    crate::lang::write_int_field(vm, enumeration, "index", 0);
    Ok(enumeration)
}

/// `BasicAttributes$AttrEnumImpl`: enumerates the attribute values.
pub(crate) const ATTR_ENUM_IMPL: oxjvm_vm::NativeClass = class(
    "javax/naming/directory/BasicAttributes$AttrEnumImpl",
    Some("java/lang/Object"),
    &["javax/naming/NamingEnumeration"],
    ACC_SYNTHETIC,
    &[field("elements", "Ljava/util/Enumeration;", 0, None)],
    &ATTR_ENUM_IMPL_METHODS,
    None,
);

const ATTR_ENUM_IMPL_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method("<init>", "()V", 0, |_, _, _| void()),
    method("hasMoreElements", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let elements = ldap_enum_elements(vm, this, "elements")?;
        ldap_call(vm, elements, "hasMoreElements", "()Z", vec![])
    }),
    method(
        "nextElement",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let elements = ldap_enum_elements(vm, this, "elements")?;
            ldap_call(vm, elements, "nextElement", "()Ljava/lang/Object;", vec![])
        },
    ),
    method("hasMore", "()Z", ACC_PUBLIC, |vm, _, args| {
        ldap_self_call(vm, receiver(args), "hasMoreElements", "()Z", vec![])
    }),
    method("next", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        ldap_self_call(
            vm,
            receiver(args),
            "nextElement",
            "()Ljava/lang/Object;",
            vec![],
        )
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        ldap_set_ref(
            vm,
            this,
            "elements",
            "Ljava/util/Enumeration;",
            ObjectRef::NULL,
        )?;
        void()
    }),
];

/// `BasicAttributes$IDEnumImpl`: enumerates attribute ids in original case.
pub(crate) const ID_ENUM_IMPL: oxjvm_vm::NativeClass = class(
    "javax/naming/directory/BasicAttributes$IDEnumImpl",
    Some("java/lang/Object"),
    &["javax/naming/NamingEnumeration"],
    ACC_SYNTHETIC,
    &[field("elements", "Ljava/util/Enumeration;", 0, None)],
    &ID_ENUM_IMPL_METHODS,
    None,
);

const ID_ENUM_IMPL_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method("<init>", "()V", 0, |_, _, _| void()),
    method("hasMoreElements", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let elements = ldap_enum_elements(vm, this, "elements")?;
        ldap_call(vm, elements, "hasMoreElements", "()Z", vec![])
    }),
    method(
        "nextElement",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let elements = ldap_enum_elements(vm, this, "elements")?;
            let attr =
                ldap_call(vm, elements, "nextElement", "()Ljava/lang/Object;", vec![])?.as_ref();
            ldap_call(vm, attr, "getID", "()Ljava/lang/String;", vec![])
        },
    ),
    method("hasMore", "()Z", ACC_PUBLIC, |vm, _, args| {
        ldap_self_call(vm, receiver(args), "hasMoreElements", "()Z", vec![])
    }),
    method("next", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        ldap_self_call(
            vm,
            receiver(args),
            "nextElement",
            "()Ljava/lang/Object;",
            vec![],
        )
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        ldap_set_ref(
            vm,
            this,
            "elements",
            "Ljava/util/Enumeration;",
            ObjectRef::NULL,
        )?;
        void()
    }),
];

/// `BasicAttribute$ValuesEnumImpl`: enumerates one attribute's values.
pub(crate) const VALUES_ENUM_IMPL: oxjvm_vm::NativeClass = class(
    "javax/naming/directory/BasicAttribute$ValuesEnumImpl",
    Some("java/lang/Object"),
    &["javax/naming/NamingEnumeration"],
    ACC_SYNTHETIC,
    &[field("list", "Ljava/util/Enumeration;", 0, None)],
    &VALUES_ENUM_IMPL_METHODS,
    None,
);

const VALUES_ENUM_IMPL_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method("<init>", "()V", 0, |_, _, _| void()),
    method("hasMoreElements", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let elements = ldap_enum_elements(vm, this, "list")?;
        ldap_call(vm, elements, "hasMoreElements", "()Z", vec![])
    }),
    method(
        "nextElement",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let elements = ldap_enum_elements(vm, this, "list")?;
            ldap_call(vm, elements, "nextElement", "()Ljava/lang/Object;", vec![])
        },
    ),
    method("hasMore", "()Z", ACC_PUBLIC, |vm, _, args| {
        ldap_self_call(vm, receiver(args), "hasMoreElements", "()Z", vec![])
    }),
    method("next", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        ldap_self_call(
            vm,
            receiver(args),
            "nextElement",
            "()Ljava/lang/Object;",
            vec![],
        )
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        ldap_set_ref(vm, this, "list", "Ljava/util/Enumeration;", ObjectRef::NULL)?;
        void()
    }),
];

fn ldap_enum_elements(
    vm: &mut Vm<'_>,
    enumeration: ObjectRef,
    field_name: &str,
) -> Result<ObjectRef, VmError> {
    let elements = ldap_read_ref(vm, enumeration, field_name, "Ljava/util/Enumeration;");
    if elements.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    Ok(elements)
}

fn ldap_enum_new(
    vm: &mut Vm<'_>,
    class_name: &str,
    field_name: &str,
    elements: ObjectRef,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class(class_name)?;
    let enumeration = vm.new_instance(class)?;
    ldap_set_ref(
        vm,
        enumeration,
        field_name,
        "Ljava/util/Enumeration;",
        elements,
    )?;
    Ok(enumeration)
}
