//! `javax.naming` reference addresses: `RefAddr`, `StringRefAddr`,
//! `BinaryRefAddr` and `Reference`. These are the plain data carriers used by
//! JNDI `Reference` objects; no directory/service-provider machinery.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use oxjvm_classfile::flags::*;

use crate::{boolean, class, field, int, int_arg, method, object, receiver, ref_arg, void};
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
