//! Tier 2.2: `java.security.*`, `javax.crypto.*` and `javax.net.ssl.*`.
//!
//! The cryptographic primitives themselves are not implemented; the classes here provide the
//! factory APIs, key/certificate containers and provider metadata so that programs which only
//! inspect them (and the many tests that merely construct a factory) run.  Operations that would
//! need real cryptography return their input unchanged or an empty result.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::{boolean, class, double, field, int, int_arg, method, object, receiver, ref_arg, void};

// -------------------------------------------------------------------------------------------
// Small helpers
// -------------------------------------------------------------------------------------------

fn new_hash_set(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let hash_set = vm.resolve_class("java/util/HashSet")?;
    let set = vm.new_instance(hash_set)?;
    let (declaring, ctor) = vm
        .find_method(hash_set, "<init>", "()V")
        .ok_or_else(|| VmError::internal("HashSet lacks <init>"))?;
    vm.invoke_method(declaring, ctor, vec![Value::Ref(set)])?;
    Ok(set)
}

fn new_array_list(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let list_class = vm.resolve_class("java/util/ArrayList")?;
    let list = vm.new_instance(list_class)?;
    let (declaring, ctor) = vm
        .find_method(list_class, "<init>", "()V")
        .ok_or_else(|| VmError::internal("ArrayList lacks <init>"))?;
    vm.invoke_method(declaring, ctor, vec![Value::Ref(list)])?;
    Ok(list)
}

fn list_add(vm: &mut Vm<'_>, list: ObjectRef, value: ObjectRef) -> Result<(), VmError> {
    let (class, method) =
        vm.resolve_virtual_method(list, "add", "(Ljava/lang/Object;)Z")?;
    vm.invoke_method(class, method, vec![Value::Ref(list), Value::Ref(value)])?;
    Ok(())
}

fn new_byte_array(vm: &mut Vm<'_>, bytes: &[u8]) -> Result<ObjectRef, VmError> {
    let byte_class = vm.primitive_class(oxjvm_classfile::BaseType::Byte)?;
    let array = vm.allocate_object_array(byte_class, bytes.len())?;
    for (index, byte) in bytes.iter().enumerate() {
        vm.array_set(array, index as i32, Value::Int(i32::from(*byte)), false)?;
    }
    Ok(array)
}

fn read_all(vm: &mut Vm<'_>, stream: ObjectRef) -> Result<Vec<u8>, VmError> {
    let mut bytes = Vec::new();
    if stream.is_null() {
        return Ok(bytes);
    }
    let (class, read) = vm.resolve_virtual_method(stream, "read", "()I")?;
    loop {
        let value = vm.invoke_method(class, read, vec![Value::Ref(stream)])?.as_int();
        if value < 0 {
            return Ok(bytes);
        }
        bytes.push(value as u8);
    }
}

fn empty_enumeration(vm: &mut Vm<'_>) -> Result<Value, VmError> {
    object(crate::util::new_enumeration(vm, &[])?)
}

fn string_field(vm: &Vm<'_>, this: ObjectRef, name: &str) -> String {
    vm.read_ref_field(this, vm.class_of(this), name, "Ljava/lang/String;")
        .and_then(|value| vm.string_value(value))
        .unwrap_or_default()
}

fn set_string_field(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: &str,
    value: &str,
) -> Result<(), VmError> {
    let text = vm.make_string(value)?;
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, name, "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, text);
    Ok(())
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

fn ref_field(vm: &Vm<'_>, this: ObjectRef, name: &str, descriptor: &str) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), name, descriptor)
        .unwrap_or(ObjectRef::NULL)
}

// -------------------------------------------------------------------------------------------
// java.security.Security
// -------------------------------------------------------------------------------------------

fn security_properties(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let security = vm.resolve_class("java/security/Security")?;
    vm.ensure_initialized(security)?;
    Ok(vm
        .read_static_ref(security, "props", "Ljava/util/Properties;")
        .unwrap_or(ObjectRef::NULL))
}

fn security_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let security = vm.resolve_class("java/security/Security")?;
    let properties = vm.resolve_class("java/util/Properties")?;
    let props = vm.new_instance(properties)?;
    let (declaring, ctor) = vm
        .find_method(properties, "<init>", "()V")
        .ok_or_else(|| VmError::internal("Properties lacks <init>"))?;
    vm.invoke_method(declaring, ctor, vec![Value::Ref(props)])?;
    vm.set_static_value(
        security,
        "props",
        "Ljava/util/Properties;",
        Value::Ref(props),
    );
    Ok(())
}

fn property_call(
    vm: &mut Vm<'_>,
    props: ObjectRef,
    name: &str,
    descriptor: &str,
    args: Vec<Value>,
) -> Result<Value, VmError> {
    let (declaring, method) = vm.resolve_virtual_method(props, name, descriptor)?;
    vm.invoke_method(declaring, method, args)
}

/// The single provider oxjvm advertises.
pub(crate) fn new_sun_provider(vm: &mut Vm<'_>, name: &str) -> Result<ObjectRef, VmError> {
    let provider_class = vm.resolve_class("java/security/Provider")?;
    let provider = vm.new_instance(provider_class)?;
    let (declaring, ctor) = vm
        .find_method(
            provider_class,
            "<init>",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
        )
        .ok_or_else(|| VmError::internal("Provider lacks <init>"))?;
    let provider_name = vm.make_string(name)?;
    let version = vm.make_string("27")?;
    let info = vm.make_string("oxjvm security provider")?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![
            Value::Ref(provider),
            Value::Ref(provider_name),
            Value::Ref(version),
            Value::Ref(info),
        ],
    )?;
    Ok(provider)
}

const SECURITY_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method(
        "getProperty",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let props = security_properties(vm)?;
            let name = ref_arg(args, 0);
            property_call(
                vm,
                props,
                "getProperty",
                "(Ljava/lang/String;)Ljava/lang/String;",
                vec![Value::Ref(props), Value::Ref(name)],
            )
        },
    ),
    method(
        "getProperty",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let props = security_properties(vm)?;
            property_call(
                vm,
                props,
                "getProperty",
                "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                vec![
                    Value::Ref(props),
                    Value::Ref(ref_arg(args, 0)),
                    Value::Ref(ref_arg(args, 1)),
                ],
            )
        },
    ),
    method(
        "setProperty",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let props = security_properties(vm)?;
            property_call(
                vm,
                props,
                "setProperty",
                "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/Object;",
                vec![
                    Value::Ref(props),
                    Value::Ref(ref_arg(args, 0)),
                    Value::Ref(ref_arg(args, 1)),
                ],
            )?;
            void()
        },
    ),
    method(
        "getProvider",
        "(Ljava/lang/String;)Ljava/security/Provider;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return object(ObjectRef::NULL);
            }
            let text = vm.string_or_empty(name);
            if text.is_empty() {
                return object(ObjectRef::NULL);
            }
            // oxjvm presents a single SUN provider; any other name is unknown unless it was
            // explicitly installed through insertProviderAt.
            let security = vm.resolve_class("java/security/Security")?;
            vm.ensure_initialized(security)?;
            if let Some(extra) = vm.read_static_ref(security, "extra", "Ljava/security/Provider;") {
                if !extra.is_null() && string_field(vm, extra, "name") == text {
                    return object(extra);
                }
            }
            if text.eq_ignore_ascii_case("SUN") || text.eq_ignore_ascii_case("SUNW") {
                object(new_sun_provider(vm, &text)?)
            } else {
                object(ObjectRef::NULL)
            }
        },
    ),
    method(
        "getProviders",
        "()[Ljava/security/Provider;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let provider_class = vm.resolve_class("java/security/Provider")?;
            let array = vm.allocate_object_array(provider_class, 1)?;
            let provider = new_sun_provider(vm, "SUN")?;
            vm.array_set_ref(array, 0, provider)?;
            object(array)
        },
    ),
    method(
        "getProviders",
        "(Ljava/lang/String;)[Ljava/security/Provider;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let provider_class = vm.resolve_class("java/security/Provider")?;
            let array = vm.allocate_object_array(provider_class, 1)?;
            let provider = new_sun_provider(vm, "SUN")?;
            vm.array_set_ref(array, 0, provider)?;
            object(array)
        },
    ),
    method(
        "getProviders",
        "(Ljava/util/Map;)[Ljava/security/Provider;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let provider_class = vm.resolve_class("java/security/Provider")?;
            object(vm.allocate_object_array(provider_class, 0)?)
        },
    ),
    method(
        "insertProviderAt",
        "(Ljava/security/Provider;I)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let provider = ref_arg(args, 0);
            if provider.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let security = vm.resolve_class("java/security/Security")?;
            vm.ensure_initialized(security)?;
            vm.set_static_value(
                security,
                "extra",
                "Ljava/security/Provider;",
                Value::Ref(provider),
            );
            int(1)
        },
    ),
    method(
        "addProvider",
        "(Ljava/security/Provider;)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let provider = ref_arg(args, 0);
            if provider.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let security = vm.resolve_class("java/security/Security")?;
            vm.ensure_initialized(security)?;
            vm.set_static_value(
                security,
                "extra",
                "Ljava/security/Provider;",
                Value::Ref(provider),
            );
            int(2)
        },
    ),
    method(
        "removeProvider",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _args| {
            let security = vm.resolve_class("java/security/Security")?;
            vm.ensure_initialized(security)?;
            vm.set_static_value(
                security,
                "extra",
                "Ljava/security/Provider;",
                Value::Ref(ObjectRef::NULL),
            );
            void()
        },
    ),
    method(
        "getAlgorithms",
        "(Ljava/lang/String;)Ljava/util/Set;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_hash_set(vm)?),
    ),
    method(
        "getAlgorithmProperty",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
];

pub(crate) const SECURITY: oxjvm_vm::NativeClass = class(
    "java/security/Security",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("props", "Ljava/util/Properties;", ACC_PRIVATE | ACC_STATIC, None),
        field("extra", "Ljava/security/Provider;", ACC_PRIVATE | ACC_STATIC, None),
    ],
    &SECURITY_METHODS,
    Some(security_clinit),
);

// -------------------------------------------------------------------------------------------
// java.security.Provider
// -------------------------------------------------------------------------------------------

fn provider_init(
    vm: &mut Vm<'_>,
    context: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let properties = vm.resolve_class("java/util/Properties")?;
    let (declaring, ctor) = vm
        .find_method(properties, "<init>", "()V")
        .ok_or_else(|| VmError::internal("Properties lacks <init>"))?;
    vm.invoke_method(declaring, ctor, vec![Value::Ref(this)])?;
    let descriptor = vm.classes.get(context.class).methods[context.method as usize]
        .descriptor
        .clone();
    let name = ref_arg(args, 1);
    let info = ref_arg(args, 3);
    set_ref_field(vm, this, "name", "Ljava/lang/String;", name)?;
    set_ref_field(vm, this, "info", "Ljava/lang/String;", info)?;
    if descriptor == "(Ljava/lang/String;DLjava/lang/String;)V" {
        let version = args[2].as_double();
        let text = alloc::format!("{version}");
        set_string_field(vm, this, "versionStr", &text)?;
        let _ = version;
    } else {
        let version = ref_arg(args, 2);
        set_ref_field(vm, this, "versionStr", "Ljava/lang/String;", version)?;
        let text = vm.string_or_empty(version);
        let _ = text;
    }
    void()
}

const PROVIDER_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PROTECTED,
        provider_init,
    ),
    method(
        "<init>",
        "(Ljava/lang/String;DLjava/lang/String;)V",
        ACC_PROTECTED,
        provider_init,
    ),
    method("getName", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        object(ref_field(vm, receiver(args), "name", "Ljava/lang/String;"))
    }),
    method("getVersionStr", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        object(ref_field(
            vm,
            receiver(args),
            "versionStr",
            "Ljava/lang/String;",
        ))
    }),
    method("getInfo", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        object(ref_field(vm, receiver(args), "info", "Ljava/lang/String;"))
    }),
    method("getVersion", "()D", ACC_PUBLIC, |vm, _, args| {
        let text = string_field(vm, receiver(args), "versionStr");
        double(text.parse::<f64>().unwrap_or(1.0))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = alloc::format!(
                "{} version {}",
                string_field(vm, this, "name"),
                string_field(vm, this, "versionStr")
            );
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "getService",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/security/Provider$Service;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method("getServices", "()Ljava/util/Set;", ACC_PUBLIC, |vm, _, _| {
        object(new_hash_set(vm)?)
    }),
    method("isConfigured", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
];

pub(crate) const PROVIDER: oxjvm_vm::NativeClass = class(
    "java/security/Provider",
    Some("java/util/Properties"),
    &[],
    ACC_PUBLIC,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("versionStr", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("info", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &PROVIDER_METHODS,
    None,
);

pub(crate) const PROVIDER_SERVICE: oxjvm_vm::NativeClass = class(
    "java/security/Provider$Service",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

// -------------------------------------------------------------------------------------------
// java.security.Key / PublicKey / PrivateKey (+ AsymmetricKey)
// -------------------------------------------------------------------------------------------

const KEY_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method(
        "getAlgorithm",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getFormat",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getEncoded",
        "()[B",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const KEY: oxjvm_vm::NativeClass = class(
    "java/security/Key",
    Some("java/io/Serializable"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &KEY_METHODS,
    None,
);

pub(crate) const ASYMMETRIC_KEY: oxjvm_vm::NativeClass = class(
    "java/security/AsymmetricKey",
    Some("java/security/Key"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const PUBLIC_KEY: oxjvm_vm::NativeClass = class(
    "java/security/PublicKey",
    Some("java/security/AsymmetricKey"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const PRIVATE_KEY: oxjvm_vm::NativeClass = class(
    "java/security/PrivateKey",
    Some("java/security/AsymmetricKey"),
    &["javax/security/auth/Destroyable"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

const OJVM_KEY_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;[B)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            set_ref_field(
                vm,
                this,
                "algorithm",
                "Ljava/lang/String;",
                ref_arg(args, 1),
            )?;
            set_ref_field(vm, this, "format", "Ljava/lang/String;", ref_arg(args, 2))?;
            set_ref_field(vm, this, "encoded", "[B", ref_arg(args, 3))?;
            void()
        },
    ),
    method(
        "getAlgorithm",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "algorithm",
                "Ljava/lang/String;",
            ))
        },
    ),
    method("getFormat", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        object(ref_field(vm, receiver(args), "format", "Ljava/lang/String;"))
    }),
    method("getEncoded", "()[B", ACC_PUBLIC, |vm, _, args| {
        object(ref_field(vm, receiver(args), "encoded", "[B"))
    }),
];

const OJVM_KEY_FIELDS: [oxjvm_vm::NativeFieldDef; 3] = [
    field("algorithm", "Ljava/lang/String;", ACC_PRIVATE, None),
    field("format", "Ljava/lang/String;", ACC_PRIVATE, None),
    field("encoded", "[B", ACC_PRIVATE, None),
];

pub(crate) const OJVM_PUBLIC_KEY: oxjvm_vm::NativeClass = class(
    "java/security/OjvmPublicKey",
    Some("java/lang/Object"),
    &["java/security/PublicKey"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &OJVM_KEY_FIELDS,
    &OJVM_KEY_METHODS,
    None,
);

pub(crate) const OJVM_PRIVATE_KEY: oxjvm_vm::NativeClass = class(
    "java/security/OjvmPrivateKey",
    Some("java/lang/Object"),
    &["java/security/PrivateKey"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &OJVM_KEY_FIELDS,
    &OJVM_KEY_METHODS,
    None,
);

fn new_key(
    vm: &mut Vm<'_>,
    class_name: &str,
    algorithm: &str,
    format: &str,
    encoded: &[u8],
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class(class_name)?;
    let key = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(class, "<init>", "(Ljava/lang/String;Ljava/lang/String;[B)V")
        .ok_or_else(|| VmError::internal("key lacks <init>"))?;
    let algorithm = vm.make_string(algorithm)?;
    let format = vm.make_string(format)?;
    let encoded = new_byte_array(vm, encoded)?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![
            Value::Ref(key),
            Value::Ref(algorithm),
            Value::Ref(format),
            Value::Ref(encoded),
        ],
    )?;
    Ok(key)
}

pub(crate) fn new_public_key(
    vm: &mut Vm<'_>,
    algorithm: &str,
) -> Result<ObjectRef, VmError> {
    new_key(vm, "java/security/OjvmPublicKey", algorithm, "X.509", &[])
}

pub(crate) fn new_private_key(
    vm: &mut Vm<'_>,
    algorithm: &str,
) -> Result<ObjectRef, VmError> {
    new_key(vm, "java/security/OjvmPrivateKey", algorithm, "PKCS#8", &[])
}

// -------------------------------------------------------------------------------------------
// java.security.KeyPair
// -------------------------------------------------------------------------------------------

const KEY_PAIR_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method(
        "<init>",
        "(Ljava/security/PublicKey;Ljava/security/PrivateKey;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            set_ref_field(
                vm,
                this,
                "publicKey",
                "Ljava/security/PublicKey;",
                ref_arg(args, 1),
            )?;
            set_ref_field(
                vm,
                this,
                "privateKey",
                "Ljava/security/PrivateKey;",
                ref_arg(args, 2),
            )?;
            void()
        },
    ),
    method(
        "getPublic",
        "()Ljava/security/PublicKey;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "publicKey",
                "Ljava/security/PublicKey;",
            ))
        },
    ),
    method(
        "getPrivate",
        "()Ljava/security/PrivateKey;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "privateKey",
                "Ljava/security/PrivateKey;",
            ))
        },
    ),
];

pub(crate) const KEY_PAIR: oxjvm_vm::NativeClass = class(
    "java/security/KeyPair",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("publicKey", "Ljava/security/PublicKey;", ACC_PRIVATE, None),
        field("privateKey", "Ljava/security/PrivateKey;", ACC_PRIVATE, None),
    ],
    &KEY_PAIR_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// java.security.SecureRandom
// -------------------------------------------------------------------------------------------

fn secure_random_next_bytes(vm: &mut Vm<'_>, this: ObjectRef, array: ObjectRef) -> Result<(), VmError> {
    if array.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let length = vm.array_length(array)?;
    for index in 0..length {
        let (class, next) = vm.resolve_virtual_method(this, "nextInt", "()I")?;
        let value = vm.invoke_method(class, next, vec![Value::Ref(this)])?.as_int();
        vm.array_set(array, index as i32, Value::Int(value & 0xFF), false)?;
    }
    Ok(())
}

const SECURE_RANDOM_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("<init>", "([B)V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "getInstance",
        "(Ljava/lang/String;)Ljava/security/SecureRandom;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let class = vm.resolve_class("java/security/SecureRandom")?;
            object(vm.new_instance(class)?)
        },
    ),
    method(
        "getInstanceStrong",
        "()Ljava/security/SecureRandom;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/security/SecureRandom")?;
            object(vm.new_instance(class)?)
        },
    ),
    method("getAlgorithm", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, _| {
        object(vm.make_string("NativePRNG")?)
    }),
    method("getProvider", "()Ljava/security/Provider;", ACC_PUBLIC, |vm, _, _| {
        object(new_sun_provider(vm, "SUN")?)
    }),
    method("setSeed", "(J)V", ACC_PUBLIC, |_, _, _| void()),
    method("setSeed", "([B)V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "nextBytes",
        "([B)V",
        ACC_PUBLIC,
        |vm, _, args| {
            secure_random_next_bytes(vm, receiver(args), ref_arg(args, 1))?;
            void()
        },
    ),
    method("generateSeed", "(I)[B", ACC_PUBLIC, |vm, _, args| {
        let length = int_arg(args, 1).max(0) as usize;
        let bytes = vec![0u8; length];
        object(new_byte_array(vm, &bytes)?)
    }),
    method("getSeed", "(I)[B", ACC_PUBLIC | ACC_STATIC, |vm, _, args| {
        let length = int_arg(args, 0).max(0) as usize;
        let bytes = vec![0u8; length];
        object(new_byte_array(vm, &bytes)?)
    }),
];

pub(crate) const SECURE_RANDOM: oxjvm_vm::NativeClass = class(
    "java/security/SecureRandom",
    Some("java/util/Random"),
    &[],
    ACC_PUBLIC,
    &[],
    &SECURE_RANDOM_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// java.security.KeyStore
// -------------------------------------------------------------------------------------------

fn key_store_instance(vm: &mut Vm<'_>, store_type: &str) -> Result<ObjectRef, VmError> {
    const SUPPORTED: [&str; 6] = ["jks", "pkcs12", "jceks", "pkcs11", "dks", "caseexactjks"];
    if !SUPPORTED.contains(&store_type.to_ascii_lowercase().as_str()) {
        return Err(vm.throw_new(
            "java/security/KeyStoreException",
            Some(&alloc::format!("{store_type} not found")),
        ));
    }
    let class = vm.resolve_class("java/security/KeyStore")?;
    let store = vm.new_instance(class)?;
    set_string_field(vm, store, "type", store_type)?;
    let provider = new_sun_provider(vm, "SUN")?;
    set_ref_field(
        vm,
        store,
        "provider",
        "Ljava/security/Provider;",
        provider,
    )?;
    Ok(store)
}

const KEY_STORE_METHODS: [oxjvm_vm::NativeMethodDef; 26] = [
    method(
        "getInstance",
        "(Ljava/lang/String;)Ljava/security/KeyStore;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let text = vm.string_or_empty(name);
            if text.is_empty() {
                return Err(vm.throw_new(
                    "java/security/KeyStoreException",
                    Some("type must not be empty"),
                ));
            }
            object(key_store_instance(vm, &text)?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/security/KeyStore;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            let provider = ref_arg(args, 1);
            if name.is_null() || provider.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let text = vm.string_or_empty(name);
            let provider = vm.string_or_empty(provider);
            if !provider.eq_ignore_ascii_case("SUN")
                && !provider.eq_ignore_ascii_case("SUNW")
            {
                return Err(vm.throw_new(
                    "java/security/NoSuchProviderException",
                    Some(&provider),
                ));
            }
            object(key_store_instance(vm, &text)?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/security/Provider;)Ljava/security/KeyStore;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(key_store_instance(vm, &vm.string_or_empty(name))?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/io/File;Ljava/lang/StringBuilder;)Ljava/security/KeyStore;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(key_store_instance(vm, "PKCS12")?)
        },
    ),
    method(
        "getDefaultType",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(vm.make_string("pkcs12")?),
    ),
    method("getType", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        object(ref_field(vm, receiver(args), "type", "Ljava/lang/String;"))
    }),
    method(
        "getProvider",
        "()Ljava/security/Provider;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "provider", "Ljava/security/Provider;")),
    ),
    method(
        "load",
        "(Ljava/io/InputStream;[C)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let stream = ref_arg(args, 1);
            if !stream.is_null() {
                let bytes = read_all(vm, stream)?;
                let data = new_byte_array(vm, &bytes)?;
                set_ref_field(vm, this, "data", "[B", data)?;
            }
            set_ref_field(vm, this, "password", "[C", ref_arg(args, 2))?;
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "loaded", "Z")?;
            vm.set_instance_int(this, declaring, field, 1);
            void()
        },
    ),
    method(
        "load",
        "(Ljava/security/KeyStore$LoadStoreParameter;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "loaded", "Z")?;
            vm.set_instance_int(this, declaring, field, 1);
            void()
        },
    ),
    method(
        "store",
        "(Ljava/io/OutputStream;[C)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let out = ref_arg(args, 1);
            let data = ref_field(vm, this, "data", "[B");
            if !out.is_null() {
                if !data.is_null() {
                    let (class, write) = vm.resolve_virtual_method(out, "write", "([B)V")?;
                    vm.invoke_method(class, write, vec![Value::Ref(out), Value::Ref(data)])?;
                } else {
                    let (class, flush) = vm.resolve_virtual_method(out, "flush", "()V")?;
                    vm.invoke_method(class, flush, vec![Value::Ref(out)])?;
                }
            }
            set_ref_field(vm, this, "password", "[C", ref_arg(args, 2))?;
            void()
        },
    ),
    method(
        "store",
        "(Ljava/security/KeyStore$LoadStoreParameter;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method("size", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method(
        "aliases",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC,
        |vm, _, _| empty_enumeration(vm),
    ),
    method("containsAlias", "(Ljava/lang/String;)Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
    method("isKeyEntry", "(Ljava/lang/String;)Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
    method(
        "isCertificateEntry",
        "(Ljava/lang/String;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(false),
    ),
    method(
        "getKey",
        "(Ljava/lang/String;[C)Ljava/security/Key;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getCertificate",
        "(Ljava/lang/String;)Ljava/security/cert/Certificate;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getCertificateChain",
        "(Ljava/lang/String;)[Ljava/security/cert/Certificate;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getCreationDate",
        "(Ljava/lang/String;)Ljava/util/Date;",
        ACC_PUBLIC,
        |vm, _, _| {
            let date_class = vm.resolve_class("java/util/Date")?;
            object(vm.new_instance(date_class)?)
        },
    ),
    method(
        "getCertificateAlias",
        "(Ljava/security/cert/Certificate;)Ljava/lang/String;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "setCertificateEntry",
        "(Ljava/lang/String;Ljava/security/cert/Certificate;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "setKeyEntry",
        "(Ljava/lang/String;Ljava/security/Key;[C[Ljava/security/cert/Certificate;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "setKeyEntry",
        "(Ljava/lang/String;[B[Ljava/security/cert/Certificate;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method("deleteEntry", "(Ljava/lang/String;)V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "getEntry",
        "(Ljava/lang/String;Ljava/security/KeyStore$ProtectionParameter;)Ljava/security/KeyStore$Entry;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
];

pub(crate) const KEY_STORE: oxjvm_vm::NativeClass = class(
    "java/security/KeyStore",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("type", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("provider", "Ljava/security/Provider;", ACC_PRIVATE, None),
        field("data", "[B", ACC_PRIVATE, None),
        field("password", "[C", ACC_PRIVATE, None),
        field("loaded", "Z", ACC_PRIVATE, None),
    ],
    &KEY_STORE_METHODS,
    None,
);

pub(crate) const KEY_STORE_LOAD_STORE_PARAMETER: oxjvm_vm::NativeClass = class(
    "java/security/KeyStore$LoadStoreParameter",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const KEY_STORE_PROTECTION_PARAMETER: oxjvm_vm::NativeClass = class(
    "java/security/KeyStore$ProtectionParameter",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const KEY_STORE_ENTRY: oxjvm_vm::NativeClass = class(
    "java/security/KeyStore$Entry",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const KEY_STORE_PRIVATE_KEY_ENTRY: oxjvm_vm::NativeClass = class(
    "java/security/KeyStore$PrivateKeyEntry",
    Some("java/lang/Object"),
    &["java/security/KeyStore$Entry"],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &[],
    None,
);

pub(crate) const KEY_STORE_TRUSTED_CERTIFICATE_ENTRY: oxjvm_vm::NativeClass = class(
    "java/security/KeyStore$TrustedCertificateEntry",
    Some("java/lang/Object"),
    &["java/security/KeyStore$Entry"],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &[],
    None,
);

pub(crate) const KEY_STORE_SECRET_KEY_ENTRY: oxjvm_vm::NativeClass = class(
    "java/security/KeyStore$SecretKeyEntry",
    Some("java/lang/Object"),
    &["java/security/KeyStore$Entry"],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &[],
    None,
);

pub(crate) const KEY_STORE_PASSWORD_PROTECTION: oxjvm_vm::NativeClass = class(
    "java/security/KeyStore$PasswordProtection",
    Some("java/lang/Object"),
    &["java/security/KeyStore$ProtectionParameter"],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

// -------------------------------------------------------------------------------------------
// java.security.KeyPairGenerator
// -------------------------------------------------------------------------------------------

fn key_pair_generator_instance(vm: &mut Vm<'_>, algorithm: &str) -> Result<ObjectRef, VmError> {
    if algorithm.is_empty() {
        return Err(vm.throw_new(
            "java/security/NoSuchAlgorithmException",
            Some("algorithm must not be empty"),
        ));
    }
    let class = vm.resolve_class("java/security/KeyPairGenerator")?;
    let generator = vm.new_instance(class)?;
    set_string_field(vm, generator, "algorithm", algorithm)?;
    let provider = new_sun_provider(vm, "SUN")?;
    set_ref_field(
        vm,
        generator,
        "provider",
        "Ljava/security/Provider;",
        provider,
    )?;
    let (declaring, field) = vm.find_field(class, "keySize", "I")?;
    vm.set_instance_int(generator, declaring, field, 2048);
    Ok(generator)
}

const KEY_PAIR_GENERATOR_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PROTECTED,
        |vm, _, args| {
            set_string_field(vm, receiver(args), "algorithm", &vm.string_or_empty(ref_arg(args, 1)))?;
            void()
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;)Ljava/security/KeyPairGenerator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(key_pair_generator_instance(vm, &vm.string_or_empty(name))?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/security/KeyPairGenerator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let provider = vm.string_or_empty(ref_arg(args, 1));
            if !provider.eq_ignore_ascii_case("SUN") && !provider.eq_ignore_ascii_case("SUNW") {
                return Err(vm.throw_new(
                    "java/security/NoSuchProviderException",
                    Some(&provider),
                ));
            }
            object(key_pair_generator_instance(vm, &vm.string_or_empty(ref_arg(args, 0)))?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/security/Provider;)Ljava/security/KeyPairGenerator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(key_pair_generator_instance(vm, &vm.string_or_empty(ref_arg(args, 0)))?)
        },
    ),
    method(
        "getAlgorithm",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "algorithm", "Ljava/lang/String;")),
    ),
    method(
        "getProvider",
        "()Ljava/security/Provider;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "provider", "Ljava/security/Provider;")),
    ),
    method("initialize", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        let (declaring, field) = vm.find_field(class, "keySize", "I")?;
        vm.set_instance_int(this, declaring, field, int_arg(args, 1));
        void()
    }),
    method(
        "initialize",
        "(ILjava/security/SecureRandom;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "keySize", "I")?;
            vm.set_instance_int(this, declaring, field, int_arg(args, 1));
            void()
        },
    ),
    method(
        "initialize",
        "(Ljava/security/spec/AlgorithmParameterSpec;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "initialize",
        "(Ljava/security/spec/AlgorithmParameterSpec;Ljava/security/SecureRandom;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "genKeyPair",
        "()Ljava/security/KeyPair;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let algorithm = string_field(vm, this, "algorithm");
            let public = new_public_key(vm, &algorithm)?;
            let private = new_private_key(vm, &algorithm)?;
            let pair_class = vm.resolve_class("java/security/KeyPair")?;
            let pair = vm.new_instance(pair_class)?;
            let (declaring, ctor) = vm
                .find_method(
                    pair_class,
                    "<init>",
                    "(Ljava/security/PublicKey;Ljava/security/PrivateKey;)V",
                )
                .ok_or_else(|| VmError::internal("KeyPair lacks <init>"))?;
            vm.invoke_method(
                declaring,
                ctor,
                vec![
                    Value::Ref(pair),
                    Value::Ref(public),
                    Value::Ref(private),
                ],
            )?;
            object(pair)
        },
    ),
    method(
        "generateKeyPair",
        "()Ljava/security/KeyPair;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let algorithm = string_field(vm, this, "algorithm");
            let public = new_public_key(vm, &algorithm)?;
            let private = new_private_key(vm, &algorithm)?;
            let pair_class = vm.resolve_class("java/security/KeyPair")?;
            let pair = vm.new_instance(pair_class)?;
            let (declaring, ctor) = vm
                .find_method(
                    pair_class,
                    "<init>",
                    "(Ljava/security/PublicKey;Ljava/security/PrivateKey;)V",
                )
                .ok_or_else(|| VmError::internal("KeyPair lacks <init>"))?;
            vm.invoke_method(
                declaring,
                ctor,
                vec![
                    Value::Ref(pair),
                    Value::Ref(public),
                    Value::Ref(private),
                ],
            )?;
            object(pair)
        },
    ),
    method("getKeySize", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        let (declaring, field) = vm.find_field(class, "keySize", "I")?;
        int(vm.read_instance_int(this, declaring, field).unwrap_or(0))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = alloc::format!(
                "KeyPairGenerator[{}]",
                string_field(vm, receiver(args), "algorithm")
            );
            object(vm.make_string(&text)?)
        },
    ),
];

pub(crate) const KEY_PAIR_GENERATOR: oxjvm_vm::NativeClass = class(
    "java/security/KeyPairGenerator",
    Some("java/security/KeyPairGeneratorSpi"),
    &[],
    ACC_PUBLIC,
    &[
        field("algorithm", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("provider", "Ljava/security/Provider;", ACC_PRIVATE, None),
        field("keySize", "I", ACC_PRIVATE, None),
    ],
    &KEY_PAIR_GENERATOR_METHODS,
    None,
);

pub(crate) const KEY_PAIR_GENERATOR_SPI: oxjvm_vm::NativeClass = class(
    "java/security/KeyPairGeneratorSpi",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &[method("<init>", "()V", ACC_PUBLIC, |_, _, _| void())],
    None,
);

// -------------------------------------------------------------------------------------------
// java.security.cert.CertificateFactory + Certificate / X509Certificate
// -------------------------------------------------------------------------------------------

fn certificate_factory_instance(vm: &mut Vm<'_>, cert_type: &str) -> Result<ObjectRef, VmError> {
    if cert_type.is_empty() {
        return Err(vm.throw_new(
            "java/security/cert/CertificateException",
            Some("type must not be empty"),
        ));
    }
    let class = vm.resolve_class("java/security/cert/CertificateFactory")?;
    let factory = vm.new_instance(class)?;
    set_string_field(vm, factory, "type", cert_type)?;
    let provider = new_sun_provider(vm, "SUN")?;
    set_ref_field(
        vm,
        factory,
        "provider",
        "Ljava/security/Provider;",
        provider,
    )?;
    Ok(factory)
}

fn new_certificate(vm: &mut Vm<'_>, cert_type: &str, encoded: &[u8]) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/security/cert/X509Certificate")?;
    let certificate = vm.new_instance(class)?;
    set_string_field(vm, certificate, "type", cert_type)?;
    let bytes = new_byte_array(vm, encoded)?;
    set_ref_field(vm, certificate, "encoded", "[B", bytes)?;
    Ok(certificate)
}

const CERTIFICATE_FACTORY_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method(
        "getInstance",
        "(Ljava/lang/String;)Ljava/security/cert/CertificateFactory;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(certificate_factory_instance(vm, &vm.string_or_empty(name))?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/security/cert/CertificateFactory;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let provider = vm.string_or_empty(ref_arg(args, 1));
            if !provider.eq_ignore_ascii_case("SUN") && !provider.eq_ignore_ascii_case("SUNW") {
                return Err(vm.throw_new(
                    "java/security/NoSuchProviderException",
                    Some(&provider),
                ));
            }
            object(certificate_factory_instance(vm, &vm.string_or_empty(ref_arg(args, 0)))?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/security/Provider;)Ljava/security/cert/CertificateFactory;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(certificate_factory_instance(vm, &vm.string_or_empty(ref_arg(args, 0)))?)
        },
    ),
    method("getType", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        object(ref_field(vm, receiver(args), "type", "Ljava/lang/String;"))
    }),
    method(
        "getProvider",
        "()Ljava/security/Provider;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "provider", "Ljava/security/Provider;")),
    ),
    method(
        "generateCertificate",
        "(Ljava/io/InputStream;)Ljava/security/cert/Certificate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let stream = ref_arg(args, 1);
            if stream.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let bytes = read_all(vm, stream)?;
            let cert_type = string_field(vm, this, "type");
            object(new_certificate(vm, &cert_type, &bytes)?)
        },
    ),
    method(
        "generateCertificates",
        "(Ljava/io/InputStream;)Ljava/util/Collection;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let stream = ref_arg(args, 1);
            let list = new_array_list(vm)?;
            if !stream.is_null() {
                let bytes = read_all(vm, stream)?;
                if !bytes.is_empty() {
                    let cert_type = string_field(vm, this, "type");
                    let certificate = new_certificate(vm, &cert_type, &bytes)?;
                    list_add(vm, list, certificate)?;
                }
            }
            object(list)
        },
    ),
    method(
        "getCertPathEncodings",
        "()Ljava/util/Iterator;",
        ACC_PUBLIC,
        |vm, _, _| {
            let list = new_array_list(vm)?;
            let (declaring, method) =
                vm.resolve_virtual_method(list, "iterator", "()Ljava/util/Iterator;")?;
            vm.invoke_method(declaring, method, vec![Value::Ref(list)])
        },
    ),
    method(
        "generateCertPath",
        "(Ljava/io/InputStream;)Ljava/security/cert/CertPath;",
        ACC_PUBLIC,
        |_, _, _| Err(VmError::internal("CertPath is not implemented")),
    ),
];

pub(crate) const CERTIFICATE_FACTORY: oxjvm_vm::NativeClass = class(
    "java/security/cert/CertificateFactory",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("type", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("provider", "Ljava/security/Provider;", ACC_PRIVATE, None),
    ],
    &CERTIFICATE_FACTORY_METHODS,
    None,
);

const CERTIFICATE_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method(
        "getType",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| object(ref_field(vm, receiver(args), "type", "Ljava/lang/String;")),
    ),
    method(
        "getEncoded",
        "()[B",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "verify",
        "(Ljava/security/PublicKey;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "verify",
        "(Ljava/security/PublicKey;Ljava/lang/String;)V",
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
        "getPublicKey",
        "()Ljava/security/PublicKey;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            if other.is_null() || vm.class_of(other) != vm.class_of(this) {
                return boolean(false);
            }
            boolean(ref_field(vm, this, "encoded", "[B").raw() == ref_field(vm, other, "encoded", "[B").raw())
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method("getVersion", "()I", ACC_PUBLIC, |_, _, _| int(3)),
];

pub(crate) const CERTIFICATE: oxjvm_vm::NativeClass = class(
    "java/security/cert/Certificate",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[
        field("type", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("encoded", "[B", ACC_PRIVATE, None),
    ],
    &CERTIFICATE_METHODS,
    None,
);

const X509_CERTIFICATE_METHODS: [oxjvm_vm::NativeMethodDef; 25] = [
    method(
        "getEncoded",
        "()[B",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "encoded", "[B")),
    ),
    method("verify", "(Ljava/security/PublicKey;)V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "verify",
        "(Ljava/security/PublicKey;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = alloc::format!(
                "X.509 certificate ({})",
                string_field(vm, receiver(args), "type")
            );
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "getPublicKey",
        "()Ljava/security/PublicKey;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "checkValidity",
        "()V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "checkValidity",
        "(Ljava/util/Date;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method("getVersion", "()I", ACC_PUBLIC, |_, _, _| int(3)),
    method(
        "getSerialNumber",
        "()Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, _| {
            let big = vm.resolve_class("java/math/BigInteger")?;
            let instance = vm.new_instance(big)?;
            let (declaring, ctor) = vm
                .find_method(big, "<init>", "(J)V")
                .ok_or_else(|| VmError::internal("BigInteger lacks <init>"))?;
            vm.invoke_method(
                declaring,
                ctor,
                vec![Value::Ref(instance), Value::Long(0)],
            )
        },
    ),
    method(
        "getIssuerDN",
        "()Ljava/security/Principal;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getSubjectDN",
        "()Ljava/security/Principal;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getIssuerX500Principal",
        "()Ljavax/security/auth/x500/X500Principal;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getSubjectX500Principal",
        "()Ljavax/security/auth/x500/X500Principal;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getNotBefore",
        "()Ljava/util/Date;",
        ACC_PUBLIC,
        |vm, _, _| {
            let date = vm.resolve_class("java/util/Date")?;
            object(vm.new_instance(date)?)
        },
    ),
    method(
        "getNotAfter",
        "()Ljava/util/Date;",
        ACC_PUBLIC,
        |vm, _, _| {
            let date = vm.resolve_class("java/util/Date")?;
            object(vm.new_instance(date)?)
        },
    ),
    method(
        "getTBSCertificate",
        "()[B",
        ACC_PUBLIC,
        |vm, _, _| object(new_byte_array(vm, &[])?),
    ),
    method("getSignature", "()[B", ACC_PUBLIC, |vm, _, _| {
        object(new_byte_array(vm, &[])?)
    }),
    method(
        "getSigAlgName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("SHA256withRSA")?),
    ),
    method(
        "getSigAlgOID",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("1.2.840.113549.1.1.11")?),
    ),
    method(
        "getSigAlgParams",
        "()[B",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getIssuerUniqueID",
        "()[Z",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getSubjectUniqueID",
        "()[Z",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getKeyUsage",
        "()[Z",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getBasicConstraints",
        "()I",
        ACC_PUBLIC,
        |_, _, _| int(-1),
    ),
    method(
        "getExtendedKeyUsage",
        "()Ljava/util/List;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
];

pub(crate) const X509_CERTIFICATE: oxjvm_vm::NativeClass = class(
    "java/security/cert/X509Certificate",
    Some("java/security/cert/Certificate"),
    &[],
    ACC_PUBLIC,
    &[],
    &X509_CERTIFICATE_METHODS,
    None,
);

pub(crate) const CERTIFICATE_ENCODING_EXCEPTION: oxjvm_vm::NativeClass = class(
    "java/security/cert/CertificateEncodingException",
    Some("java/security/cert/CertificateException"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

pub(crate) const X509_CRL: oxjvm_vm::NativeClass = class(
    "java/security/cert/X509CRL",
    Some("java/security/cert/CRL"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const CRL: oxjvm_vm::NativeClass = class(
    "java/security/cert/CRL",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

// -------------------------------------------------------------------------------------------
// java.security.CodeSource / ProtectionDomain
// -------------------------------------------------------------------------------------------

const CODE_SOURCE_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method(
        "<init>",
        "(Ljava/net/URL;[Ljava/security/cert/Certificate;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            set_ref_field(vm, this, "location", "Ljava/net/URL;", ref_arg(args, 1))?;
            set_ref_field(
                vm,
                this,
                "certificates",
                "[Ljava/security/cert/Certificate;",
                ref_arg(args, 2),
            )?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/net/URL;[Ljava/security/CodeSigner;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            set_ref_field(vm, this, "location", "Ljava/net/URL;", ref_arg(args, 1))?;
            set_ref_field(
                vm,
                this,
                "codeSigners",
                "[Ljava/security/CodeSigner;",
                ref_arg(args, 2),
            )?;
            void()
        },
    ),
    method("getLocation", "()Ljava/net/URL;", ACC_PUBLIC, |vm, _, args| {
        object(ref_field(vm, receiver(args), "location", "Ljava/net/URL;"))
    }),
    method(
        "getCertificates",
        "()[Ljava/security/cert/Certificate;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "certificates",
                "[Ljava/security/cert/Certificate;",
            ))
        },
    ),
    method(
        "getCodeSigners",
        "()[Ljava/security/CodeSigner;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "codeSigners",
                "[Ljava/security/CodeSigner;",
            ))
        },
    ),
    method("implies", "(Ljava/security/CodeSource;)Z", ACC_PUBLIC, |_, _, _| {
        boolean(true)
    }),
    method("hashCode", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            via_code_source_to_string(vm, args)
        },
    ),
];

fn via_code_source_to_string(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let this = receiver(args);
    let location = ref_field(vm, this, "location", "Ljava/net/URL;");
    let text = if location.is_null() {
        String::from("<no location>")
    } else {
        let (class, method) =
            vm.resolve_virtual_method(location, "toString", "()Ljava/lang/String;")?;
        let value = vm.invoke_method(class, method, vec![Value::Ref(location)])?;
        vm.string_value(value.as_ref()).unwrap_or_default()
    };
    object(vm.make_string(&text)?)
}

pub(crate) const CODE_SOURCE: oxjvm_vm::NativeClass = class(
    "java/security/CodeSource",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[
        field("location", "Ljava/net/URL;", ACC_PRIVATE, None),
        field(
            "certificates",
            "[Ljava/security/cert/Certificate;",
            ACC_PRIVATE,
            None,
        ),
        field(
            "codeSigners",
            "[Ljava/security/CodeSigner;",
            ACC_PRIVATE,
            None,
        ),
    ],
    &CODE_SOURCE_METHODS,
    None,
);

pub(crate) const CODE_SIGNER: oxjvm_vm::NativeClass = class(
    "java/security/CodeSigner",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

const PROTECTION_DOMAIN_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "<init>",
        "(Ljava/security/CodeSource;Ljava/security/PermissionCollection;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            set_ref_field(vm, this, "codeSource", "Ljava/security/CodeSource;", ref_arg(args, 1))?;
            set_ref_field(
                vm,
                this,
                "permissions",
                "Ljava/security/PermissionCollection;",
                ref_arg(args, 2),
            )?;
            void()
        },
    ),
    method(
        "getCodeSource",
        "()Ljava/security/CodeSource;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "codeSource",
                "Ljava/security/CodeSource;",
            ))
        },
    ),
    method(
        "getPermissions",
        "()Ljava/security/PermissionCollection;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "permissions",
                "Ljava/security/PermissionCollection;",
            ))
        },
    ),
    method(
        "getClassLoader",
        "()Ljava/lang/ClassLoader;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "implies",
        "(Ljava/security/Permission;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(true),
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("ProtectionDomain")?),
    ),
];

pub(crate) const PROTECTION_DOMAIN: oxjvm_vm::NativeClass = class(
    "java/security/ProtectionDomain",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("codeSource", "Ljava/security/CodeSource;", ACC_PRIVATE, None),
        field(
            "permissions",
            "Ljava/security/PermissionCollection;",
            ACC_PRIVATE,
            None,
        ),
    ],
    &PROTECTION_DOMAIN_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// java.security.AccessController + privileged actions
// -------------------------------------------------------------------------------------------

pub(crate) const PRIVILEGED_ACTION: oxjvm_vm::NativeClass = class(
    "java/security/PrivilegedAction",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[method(
        "run",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    )],
    None,
);

pub(crate) const PRIVILEGED_EXCEPTION_ACTION: oxjvm_vm::NativeClass = class(
    "java/security/PrivilegedExceptionAction",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[method(
        "run",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    )],
    None,
);

fn run_privileged_action(
    vm: &mut Vm<'_>,
    action: ObjectRef,
    exception_action: bool,
) -> Result<Value, VmError> {
    if action.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let (class, method) =
        vm.resolve_virtual_method(action, "run", "()Ljava/lang/Object;")?;
    let _ = exception_action;
    vm.invoke_method(class, method, vec![Value::Ref(action)])
}

const ACCESS_CONTROLLER_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "doPrivileged",
        "(Ljava/security/PrivilegedAction;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| run_privileged_action(vm, ref_arg(args, 0), false),
    ),
    method(
        "doPrivileged",
        "(Ljava/security/PrivilegedAction;Ljava/security/AccessControlContext;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| run_privileged_action(vm, ref_arg(args, 0), false),
    ),
    method(
        "doPrivilegedWithCombiner",
        "(Ljava/security/PrivilegedAction;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| run_privileged_action(vm, ref_arg(args, 0), false),
    ),
    method(
        "doPrivileged",
        "(Ljava/security/PrivilegedExceptionAction;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| run_privileged_action(vm, ref_arg(args, 0), true),
    ),
    method(
        "doPrivilegedWithCombiner",
        "(Ljava/security/PrivilegedExceptionAction;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| run_privileged_action(vm, ref_arg(args, 0), true),
    ),
    method(
        "checkPermission",
        "(Ljava/security/Permission;)V",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| void(),
    ),
];

pub(crate) const ACCESS_CONTROLLER: oxjvm_vm::NativeClass = class(
    "java/security/AccessController",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &ACCESS_CONTROLLER_METHODS,
    None,
);

pub(crate) const ACCESS_CONTROL_CONTEXT: oxjvm_vm::NativeClass = class(
    "java/security/AccessControlContext",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &[],
    None,
);

pub(crate) const PRIVILEGED_ACTION_EXCEPTION: oxjvm_vm::NativeClass = class(
    "java/security/PrivilegedActionException",
    Some("java/lang/Exception"),
    &[],
    ACC_PUBLIC,
    &[field("exception", "Ljava/lang/Exception;", ACC_PRIVATE, None)],
    &[
        method(
            "<init>",
            "(Ljava/lang/Exception;)V",
            ACC_PUBLIC,
            |vm, _, args| {
                let this = receiver(args);
                set_ref_field(vm, this, "exception", "Ljava/lang/Exception;", ref_arg(args, 1))?;
                let exception_ctor = vm
                    .find_method(vm.class_of(this), "<init>", "(Ljava/lang/Throwable;)V");
                if let Some((declaring, ctor)) = exception_ctor {
                    vm.invoke_method(declaring, ctor, vec![Value::Ref(this)])?;
                }
                void()
            },
        ),
        method(
            "getException",
            "()Ljava/lang/Exception;",
            ACC_PUBLIC,
            |vm, _, args| {
                object(ref_field(
                    vm,
                    receiver(args),
                    "exception",
                    "Ljava/lang/Exception;",
                ))
            },
        ),
    ],
    None,
);

// -------------------------------------------------------------------------------------------
// java.security.Permission family (shells with names/actions)
// -------------------------------------------------------------------------------------------

pub(crate) const PERMISSION: oxjvm_vm::NativeClass = class(
    "java/security/Permission",
    Some("java/lang/Object"),
    &["java/security/Guard", "java/io/Serializable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[field("name", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &PERMISSION_METHODS,
    None,
);

const PERMISSION_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = ref_arg(args, 1);
            if name.is_null() {
                return Err(vm.throw_new(
                    "java/lang/NullPointerException",
                    Some("name can't be null"),
                ));
            }
            set_ref_field(vm, receiver(args), "name", "Ljava/lang/String;", name)?;
            void()
        },
    ),
    method("getName", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        object(ref_field(vm, receiver(args), "name", "Ljava/lang/String;"))
    }),
    method(
        "getActions",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "implies",
        "(Ljava/security/Permission;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "newPermissionCollection",
        "()Ljava/security/PermissionCollection;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = alloc::format!(
                "({} {})",
                vm.class_name(vm.class_of(receiver(args))),
                string_field(vm, receiver(args), "name")
            );
            object(vm.make_string(&text)?)
        },
    ),
];

pub(crate) const BASIC_PERMISSION: oxjvm_vm::NativeClass = class(
    "java/security/BasicPermission",
    Some("java/security/Permission"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[field("actions", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &BASIC_PERMISSION_METHODS,
    None,
);

const BASIC_PERMISSION_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = ref_arg(args, 1);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let parent = vm.resolve_class("java/security/Permission")?;
            let (declaring, ctor) = vm
                .find_method(parent, "<init>", "(Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("Permission lacks <init>"))?;
            vm.invoke_method(declaring, ctor, args.to_vec())?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name = ref_arg(args, 1);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let parent = vm.resolve_class("java/security/Permission")?;
            let (declaring, ctor) = vm
                .find_method(parent, "<init>", "(Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("Permission lacks <init>"))?;
            vm.invoke_method(
                declaring,
                ctor,
                vec![Value::Ref(this), Value::Ref(name)],
            )?;
            set_ref_field(vm, this, "actions", "Ljava/lang/String;", ref_arg(args, 2))?;
            void()
        },
    ),
    method(
        "getActions",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let actions = ref_field(vm, receiver(args), "actions", "Ljava/lang/String;");
            if actions.is_null() {
                object(vm.make_string("")?)
            } else {
                object(actions)
            }
        },
    ),
    method(
        "implies",
        "(Ljava/security/Permission;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            if other.is_null() {
                return boolean(false);
            }
            if vm.class_of(other) != vm.class_of(this) {
                return boolean(false);
            }
            boolean(string_field(vm, this, "name") == string_field(vm, other, "name"))
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            if other.is_null() || vm.class_of(other) != vm.class_of(this) {
                return boolean(false);
            }
            boolean(string_field(vm, this, "name") == string_field(vm, other, "name"))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |_, _, _| int(0)),
];

pub(crate) const PERMISSION_COLLECTION: oxjvm_vm::NativeClass = class(
    "java/security/PermissionCollection",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const PERMISSIONS: oxjvm_vm::NativeClass = class(
    "java/security/Permissions",
    Some("java/security/PermissionCollection"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &[],
    None,
);

pub(crate) const POLICY: oxjvm_vm::NativeClass = class(
    "java/security/Policy",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const SECURE_CLASS_LOADER: oxjvm_vm::NativeClass = class(
    "java/security/SecureClassLoader",
    Some("java/lang/ClassLoader"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

pub(crate) const GUARD: oxjvm_vm::NativeClass = class(
    "java/security/Guard",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

// -------------------------------------------------------------------------------------------
// javax.crypto.Cipher (+ SecretKey / spec classes)
// -------------------------------------------------------------------------------------------

fn cipher_instance(vm: &mut Vm<'_>, transformation: &str) -> Result<ObjectRef, VmError> {
    if transformation.is_empty() {
        return Err(vm.throw_new(
            "java/security/NoSuchAlgorithmException",
            Some("transformation must not be empty"),
        ));
    }
    let class = vm.resolve_class("javax/crypto/Cipher")?;
    let cipher = vm.new_instance(class)?;
    set_string_field(vm, cipher, "transformation", transformation)?;
    let provider = new_sun_provider(vm, "SUN")?;
    set_ref_field(
        vm,
        cipher,
        "provider",
        "Ljava/security/Provider;",
        provider,
    )?;
    Ok(cipher)
}

const CIPHER_METHODS: [oxjvm_vm::NativeMethodDef; 23] = [
    method(
        "getInstance",
        "(Ljava/lang/String;)Ljavax/crypto/Cipher;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(cipher_instance(vm, &vm.string_or_empty(name))?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/lang/String;)Ljavax/crypto/Cipher;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let provider = vm.string_or_empty(ref_arg(args, 1));
            if !provider.eq_ignore_ascii_case("SUN") && !provider.eq_ignore_ascii_case("SUNW") {
                return Err(vm.throw_new(
                    "java/security/NoSuchProviderException",
                    Some(&provider),
                ));
            }
            object(cipher_instance(vm, &vm.string_or_empty(ref_arg(args, 0)))?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/security/Provider;)Ljavax/crypto/Cipher;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(cipher_instance(vm, &vm.string_or_empty(ref_arg(args, 0)))?)
        },
    ),
    method("getAlgorithm", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        object(ref_field(
            vm,
            receiver(args),
            "transformation",
            "Ljava/lang/String;",
        ))
    }),
    method(
        "getProvider",
        "()Ljava/security/Provider;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "provider", "Ljava/security/Provider;")),
    ),
    method("getBlockSize", "()I", ACC_PUBLIC, |_, _, _| int(16)),
    method("getOutputSize", "(I)I", ACC_PUBLIC, |_, _, args| {
        int(int_arg(args, 1).max(0))
    }),
    method("getIV", "()[B", ACC_PUBLIC, |_, _, _| object(ObjectRef::NULL)),
    method(
        "getParameters",
        "()Ljava/security/AlgorithmParameters;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getExemptionMechanism",
        "()Ljavax/crypto/ExemptionMechanism;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "init",
        "(ILjava/security/Key;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "mode", "I")?;
            vm.set_instance_int(this, declaring, field, int_arg(args, 1));
            set_ref_field(vm, this, "key", "Ljava/security/Key;", ref_arg(args, 2))?;
            void()
        },
    ),
    method(
        "init",
        "(ILjava/security/Key;Ljava/security/SecureRandom;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "mode", "I")?;
            vm.set_instance_int(this, declaring, field, int_arg(args, 1));
            set_ref_field(vm, this, "key", "Ljava/security/Key;", ref_arg(args, 2))?;
            void()
        },
    ),
    method(
        "init",
        "(ILjava/security/Key;Ljava/security/spec/AlgorithmParameterSpec;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "mode", "I")?;
            vm.set_instance_int(this, declaring, field, int_arg(args, 1));
            set_ref_field(vm, this, "key", "Ljava/security/Key;", ref_arg(args, 2))?;
            void()
        },
    ),
    method(
        "init",
        "(ILjava/security/Key;Ljava/security/spec/AlgorithmParameterSpec;Ljava/security/SecureRandom;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "mode", "I")?;
            vm.set_instance_int(this, declaring, field, int_arg(args, 1));
            set_ref_field(vm, this, "key", "Ljava/security/Key;", ref_arg(args, 2))?;
            void()
        },
    ),
    method(
        "update",
        "([B)[B",
        ACC_PUBLIC,
        |_, _, args| object(ref_arg(args, 1)),
    ),
    method(
        "update",
        "([BII)[B",
        ACC_PUBLIC,
        |vm, _, args| {
            let source = ref_arg(args, 1);
            if source.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let offset = int_arg(args, 2).max(0) as usize;
            let length = int_arg(args, 3).max(0) as usize;
            let mut bytes = Vec::with_capacity(length);
            for index in 0..length {
                bytes.push(vm.array_get(source, (offset + index) as i32)?.as_int() as u8);
            }
            object(new_byte_array(vm, &bytes)?)
        },
    ),
    method(
        "doFinal",
        "()[B",
        ACC_PUBLIC,
        |vm, _, _| object(new_byte_array(vm, &[])?),
    ),
    method(
        "doFinal",
        "([B)[B",
        ACC_PUBLIC,
        |_, _, args| object(ref_arg(args, 1)),
    ),
    method(
        "doFinal",
        "([BII)[B",
        ACC_PUBLIC,
        |vm, _, args| {
            let source = ref_arg(args, 1);
            if source.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let offset = int_arg(args, 2).max(0) as usize;
            let length = int_arg(args, 3).max(0) as usize;
            let mut bytes = Vec::with_capacity(length);
            for index in 0..length {
                bytes.push(vm.array_get(source, (offset + index) as i32)?.as_int() as u8);
            }
            object(new_byte_array(vm, &bytes)?)
        },
    ),
    method(
        "doFinal",
        "([BI)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let source = ref_arg(args, 1);
            if source.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let offset = int_arg(args, 2).max(0) as usize;
            let length = vm.array_length(source)?.saturating_sub(offset);
            for index in 0..length {
                let byte = vm.array_get(source, (offset + index) as i32)?.as_int();
                vm.array_set(source, (offset + index) as i32, Value::Int(byte), false)?;
            }
            int(length as i32)
        },
    ),
    method(
        "wrap",
        "(Ljava/security/Key;)[B",
        ACC_PUBLIC,
        |vm, _, args| {
            let key = ref_arg(args, 1);
            if key.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let (class, method) = vm.resolve_virtual_method(key, "getEncoded", "()[B")?;
            vm.invoke_method(class, method, vec![Value::Ref(key)])
        },
    ),
    method(
        "unwrap",
        "([BLjava/lang/String;I)Ljava/security/Key;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = vm.string_or_empty(ref_arg(args, 2));
            object(new_private_key(vm, &name)?)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = alloc::format!(
                "Cipher[{}]",
                string_field(vm, receiver(args), "transformation")
            );
            object(vm.make_string(&text)?)
        },
    ),
];

pub(crate) const CIPHER: oxjvm_vm::NativeClass = class(
    "javax/crypto/Cipher",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("transformation", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("provider", "Ljava/security/Provider;", ACC_PRIVATE, None),
        field("mode", "I", ACC_PRIVATE, None),
        field("key", "Ljava/security/Key;", ACC_PRIVATE, None),
    ],
    &CIPHER_METHODS,
    None,
);

pub(crate) const SECRET_KEY: oxjvm_vm::NativeClass = class(
    "javax/crypto/SecretKey",
    Some("java/security/Key"),
    &["javax/security/auth/Destroyable"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const SECRET_KEY_SPEC: oxjvm_vm::NativeClass = class(
    "javax/crypto/spec/SecretKeySpec",
    Some("java/lang/Object"),
    &["javax/crypto/SecretKey", "java/security/spec/KeySpec"],
    ACC_PUBLIC,
    &[
        field("algorithm", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("key", "[B", ACC_PRIVATE, None),
    ],
    &SECRET_KEY_SPEC_METHODS,
    None,
);

const SECRET_KEY_SPEC_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "<init>",
        "([BLjava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let key = ref_arg(args, 1);
            if key.is_null() || ref_arg(args, 2).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            if vm.array_length(key)? == 0 {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("Empty key"),
                ));
            }
            set_ref_field(vm, this, "key", "[B", key)?;
            set_ref_field(
                vm,
                this,
                "algorithm",
                "Ljava/lang/String;",
                ref_arg(args, 2),
            )?;
            void()
        },
    ),
    method(
        "<init>",
        "([BIILjava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let source = ref_arg(args, 1);
            if source.is_null() || ref_arg(args, 4).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let offset = int_arg(args, 2).max(0) as usize;
            let length = int_arg(args, 3).max(0) as usize;
            let mut bytes = Vec::with_capacity(length);
            for index in 0..length {
                bytes.push(vm.array_get(source, (offset + index) as i32)?.as_int() as u8);
            }
            let key = new_byte_array(vm, &bytes)?;
            set_ref_field(vm, this, "key", "[B", key)?;
            set_ref_field(
                vm,
                this,
                "algorithm",
                "Ljava/lang/String;",
                ref_arg(args, 4),
            )?;
            void()
        },
    ),
    method(
        "getAlgorithm",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "algorithm", "Ljava/lang/String;")),
    ),
    method("getFormat", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, _| {
        object(vm.make_string("RAW")?)
    }),
    method("getEncoded", "()[B", ACC_PUBLIC, |vm, _, args| {
        object(ref_field(vm, receiver(args), "key", "[B"))
    }),
    method("equals", "(Ljava/lang/Object;)Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
    method("hashCode", "()I", ACC_PUBLIC, |_, _, _| int(0)),
];

pub(crate) const IV_PARAMETER_SPEC: oxjvm_vm::NativeClass = class(
    "javax/crypto/spec/IvParameterSpec",
    Some("java/lang/Object"),
    &["java/security/spec/AlgorithmParameterSpec"],
    ACC_PUBLIC,
    &[field("iv", "[B", ACC_PRIVATE, None)],
    &[
        method(
            "<init>",
            "([B)V",
            ACC_PUBLIC,
            |vm, _, args| {
                let iv = ref_arg(args, 1);
                if iv.is_null() {
                    return Err(vm.throw_new("java/lang/NullPointerException", None));
                }
                set_ref_field(vm, receiver(args), "iv", "[B", iv)?;
                void()
            },
        ),
        method("getIV", "()[B", ACC_PUBLIC, |vm, _, args| {
            object(ref_field(vm, receiver(args), "iv", "[B"))
        }),
    ],
    None,
);

pub(crate) const GCMPARAMETER_SPEC: oxjvm_vm::NativeClass = class(
    "javax/crypto/spec/GCMParameterSpec",
    Some("java/lang/Object"),
    &["java/security/spec/AlgorithmParameterSpec"],
    ACC_PUBLIC,
    &[
        field("tLen", "I", ACC_PRIVATE, None),
        field("src", "[B", ACC_PRIVATE, None),
    ],
    &[
        method(
            "<init>",
            "(I[B)V",
            ACC_PUBLIC,
            |vm, _, args| {
                let this = receiver(args);
                let class = vm.class_of(this);
                let (declaring, field) = vm.find_field(class, "tLen", "I")?;
                vm.set_instance_int(this, declaring, field, int_arg(args, 1));
                set_ref_field(vm, this, "src", "[B", ref_arg(args, 2))?;
                void()
            },
        ),
        method("getTLen", "()I", ACC_PUBLIC, |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "tLen", "I")?;
            int(vm.read_instance_int(this, declaring, field).unwrap_or(0))
        }),
        method("getIV", "()[B", ACC_PUBLIC, |vm, _, args| {
            object(ref_field(vm, receiver(args), "src", "[B"))
        }),
    ],
    None,
);

pub(crate) const KEY_SPEC: oxjvm_vm::NativeClass = class(
    "java/security/spec/KeySpec",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const ALGORITHM_PARAMETER_SPEC: oxjvm_vm::NativeClass = class(
    "java/security/spec/AlgorithmParameterSpec",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

// -------------------------------------------------------------------------------------------
// javax.net.ssl.SSLContext (+ its collaborators)
// -------------------------------------------------------------------------------------------

fn ssl_context_instance(vm: &mut Vm<'_>, protocol: &str) -> Result<ObjectRef, VmError> {
    if protocol.is_empty() {
        return Err(vm.throw_new(
            "java/security/NoSuchAlgorithmException",
            Some("protocol must not be empty"),
        ));
    }
    let class = vm.resolve_class("javax/net/ssl/SSLContext")?;
    let context = vm.new_instance(class)?;
    set_string_field(vm, context, "protocol", protocol)?;
    let provider = new_sun_provider(vm, "SUN")?;
    set_ref_field(
        vm,
        context,
        "provider",
        "Ljava/security/Provider;",
        provider,
    )?;
    Ok(context)
}

const SSL_CONTEXT_METHODS: [oxjvm_vm::NativeMethodDef; 13] = [
    method(
        "getDefault",
        "()Ljavax/net/ssl/SSLContext;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(ssl_context_instance(vm, "TLS")?),
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;)Ljavax/net/ssl/SSLContext;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(ssl_context_instance(vm, &vm.string_or_empty(name))?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/lang/String;)Ljavax/net/ssl/SSLContext;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let provider = vm.string_or_empty(ref_arg(args, 1));
            if !provider.eq_ignore_ascii_case("SUN") && !provider.eq_ignore_ascii_case("SUNW") {
                return Err(vm.throw_new(
                    "java/security/NoSuchProviderException",
                    Some(&provider),
                ));
            }
            object(ssl_context_instance(vm, &vm.string_or_empty(ref_arg(args, 0)))?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/security/Provider;)Ljavax/net/ssl/SSLContext;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(ssl_context_instance(vm, &vm.string_or_empty(ref_arg(args, 0)))?)
        },
    ),
    method("getProtocol", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        object(ref_field(vm, receiver(args), "protocol", "Ljava/lang/String;"))
    }),
    method(
        "getProvider",
        "()Ljava/security/Provider;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "provider", "Ljava/security/Provider;")),
    ),
    method(
        "init",
        "([Ljavax/net/ssl/KeyManager;[Ljavax/net/ssl/TrustManager;Ljava/security/SecureRandom;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "getSocketFactory",
        "()Ljavax/net/ssl/SSLSocketFactory;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("javax/net/ssl/SSLSocketFactory")?;
            object(vm.new_instance(class)?)
        },
    ),
    method(
        "getServerSocketFactory",
        "()Ljavax/net/ssl/SSLServerSocketFactory;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("javax/net/ssl/SSLServerSocketFactory")?;
            object(vm.new_instance(class)?)
        },
    ),
    method(
        "createSSLEngine",
        "()Ljavax/net/ssl/SSLEngine;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("javax/net/ssl/SSLEngine")?;
            object(vm.new_instance(class)?)
        },
    ),
    method(
        "createSSLEngine",
        "(Ljava/lang/String;I)Ljavax/net/ssl/SSLEngine;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("javax/net/ssl/SSLEngine")?;
            object(vm.new_instance(class)?)
        },
    ),
    method(
        "getDefaultSSLParameters",
        "()Ljavax/net/ssl/SSLParameters;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("javax/net/ssl/SSLParameters")?;
            object(vm.new_instance(class)?)
        },
    ),
    method(
        "getSupportedSSLParameters",
        "()Ljavax/net/ssl/SSLParameters;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("javax/net/ssl/SSLParameters")?;
            object(vm.new_instance(class)?)
        },
    ),
];

pub(crate) const SSL_CONTEXT: oxjvm_vm::NativeClass = class(
    "javax/net/ssl/SSLContext",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("protocol", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("provider", "Ljava/security/Provider;", ACC_PRIVATE, None),
    ],
    &SSL_CONTEXT_METHODS,
    None,
);

const SSL_SOCKET_FACTORY_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "getDefaultCipherSuites",
        "()[Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getSupportedCipherSuites",
        "()[Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "createSocket",
        "(Ljava/net/Socket;Ljava/lang/String;IZ)Ljava/net/Socket;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getDefault",
        "()Ljavax/net/SocketFactory;",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| Err(VmError::internal("no default SSL socket factory")),
    ),
];

pub(crate) const SSL_SOCKET_FACTORY: oxjvm_vm::NativeClass = class(
    "javax/net/ssl/SSLSocketFactory",
    Some("javax/net/SocketFactory"),
    &[],
    ACC_PUBLIC,
    &[],
    &SSL_SOCKET_FACTORY_METHODS,
    None,
);

pub(crate) const SSL_SERVER_SOCKET_FACTORY: oxjvm_vm::NativeClass = class(
    "javax/net/ssl/SSLServerSocketFactory",
    Some("javax/net/ServerSocketFactory"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

pub(crate) const SSLE_ENGINE: oxjvm_vm::NativeClass = class(
    "javax/net/ssl/SSLEngine",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const SSL_PARAMETERS: oxjvm_vm::NativeClass = class(
    "javax/net/ssl/SSLParameters",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

pub(crate) const KEY_MANAGER: oxjvm_vm::NativeClass = class(
    "javax/net/ssl/KeyManager",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const TRUST_MANAGER: oxjvm_vm::NativeClass = class(
    "javax/net/ssl/TrustManager",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

// -------------------------------------------------------------------------------------------
// java.security.spec leaves
// -------------------------------------------------------------------------------------------

pub(crate) const X509_ENCODED_KEY_SPEC: oxjvm_vm::NativeClass = class(
    "java/security/spec/X509EncodedKeySpec",
    Some("java/lang/Object"),
    &["java/security/spec/KeySpec"],
    ACC_PUBLIC,
    &[field("encoded", "[B", ACC_PRIVATE, None)],
    &[
        method(
            "<init>",
            "([B)V",
            ACC_PUBLIC,
            |vm, _, args| {
                let encoded = ref_arg(args, 1);
                if encoded.is_null() {
                    return Err(vm.throw_new("java/lang/NullPointerException", None));
                }
                set_ref_field(vm, receiver(args), "encoded", "[B", encoded)?;
                void()
            },
        ),
        method("getEncoded", "()[B", ACC_PUBLIC, |vm, _, args| {
            object(ref_field(vm, receiver(args), "encoded", "[B"))
        }),
        method("getFormat", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, _| {
            object(vm.make_string("X.509")?)
        }),
    ],
    None,
);

pub(crate) const PKCS8_ENCODED_KEY_SPEC: oxjvm_vm::NativeClass = class(
    "java/security/spec/PKCS8EncodedKeySpec",
    Some("java/lang/Object"),
    &["java/security/spec/KeySpec"],
    ACC_PUBLIC,
    &[field("encoded", "[B", ACC_PRIVATE, None)],
    &[
        method(
            "<init>",
            "([B)V",
            ACC_PUBLIC,
            |vm, _, args| {
                let encoded = ref_arg(args, 1);
                if encoded.is_null() {
                    return Err(vm.throw_new("java/lang/NullPointerException", None));
                }
                set_ref_field(vm, receiver(args), "encoded", "[B", encoded)?;
                void()
            },
        ),
        method("getEncoded", "()[B", ACC_PUBLIC, |vm, _, args| {
            object(ref_field(vm, receiver(args), "encoded", "[B"))
        }),
        method("getFormat", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, _| {
            object(vm.make_string("PKCS#8")?)
        }),
    ],
    None,
);

pub(crate) const DESTROYABLE: oxjvm_vm::NativeClass = class(
    "javax/security/auth/Destroyable",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const DESTROY_FAILED_EXCEPTION: oxjvm_vm::NativeClass = class(
    "javax/security/auth/DestroyFailedException",
    Some("java/lang/Exception"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);