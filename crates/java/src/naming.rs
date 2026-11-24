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

/// `javax.naming.ldap.Rdn`: a relative distinguished name. The RFC 2253
/// escape/unescape value helpers are modeled here; the parsed type/value
/// entries and name comparison are added with the LDAP name subsystem.
pub(crate) const RDN: oxjvm_vm::NativeClass = class(
    "javax/naming/ldap/Rdn",
    Some("java/lang/Object"),
    &["java/io/Serializable", "java/lang/Comparable"],
    ACC_PUBLIC,
    &[],
    &RDN_METHODS,
    None,
);

const RDN_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "escapeValue",
        "(Ljava/lang/Object;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let val = ref_arg(args, 0);
            if val.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let class = vm.class_of(val);
            let name = vm.class_name(class);
            if name == "[B" {
                let bytes = crate::lang::array_bytes(vm, val)?;
                let text = rdn_escape_binary(&bytes);
                object(vm.make_string(&text)?)
            } else if name == "java/lang/String" {
                let units = vm.string_utf16(val).unwrap_or_default().to_vec();
                let escaped = rdn_escape_string(&units);
                object(vm.make_string_utf16(escaped)?)
            } else {
                Err(vm.throw_new(
                    "java/lang/ClassCastException",
                    Some(&alloc::format!(
                        "class {name} cannot be cast to class java.lang.String"
                    )),
                ))
            }
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
];

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
