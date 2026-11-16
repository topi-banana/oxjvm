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
    let (class, method) = vm.resolve_virtual_method(list, "add", "(Ljava/lang/Object;)Z")?;
    vm.invoke_method(class, method, vec![Value::Ref(list), Value::Ref(value)])?;
    Ok(())
}

fn new_byte_array(vm: &mut Vm<'_>, bytes: &[u8]) -> Result<ObjectRef, VmError> {
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, bytes.len())?;
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
        let value = vm
            .invoke_method(class, read, vec![Value::Ref(stream)])?
            .as_int();
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

/// The security properties the JDK ships in `conf/security/java.security`.  Tests read
/// them through `Security.getProperty`; a missing value surfaces as an NPE in the test
/// library (e.g. `SecurityUtils.removeFromDisabledAlgs`).
const SECURITY_PROPERTY_DEFAULTS: [(&str, &str); 9] = [
    (
        "jdk.tls.disabledAlgorithms",
        "SSLv3, TLSv1, TLSv1.1, DTLSv1.0, RC4, DES, MD5withRSA, DH keySize < 1024, \
         EC keySize < 224, 3DES_EDE_CBC, anon, NULL, ECDH, TLS_RSA_*, \
         rsa_pkcs1_sha1 usage HandshakeSignature, ecdsa_sha1 usage HandshakeSignature, \
         dsa_sha1 usage HandshakeSignature",
    ),
    (
        "jdk.certpath.disabledAlgorithms",
        "MD2, MD5, SHA1 jdkCA & usage TLSServer, RSA keySize < 1024, DSA keySize < 1024, \
         EC keySize < 224, SHA1 usage SignedJAR & denyAfter 2019-01-01",
    ),
    (
        "jdk.jar.disabledAlgorithms",
        "MD2, MD5, RSA keySize < 1024, DSA keySize < 1024, SHA1 denyAfter 2019-01-01",
    ),
    (
        "jdk.tls.legacyAlgorithms",
        "NULL, anon, RC4, DES, 3DES_EDE_CBC",
    ),
    ("ssl.KeyManagerFactory.algorithm", "SunX509"),
    ("ssl.TrustManagerFactory.algorithm", "PKIX"),
    ("securerandom.source", "file:/dev/random"),
    (
        "securerandom.strongAlgorithms",
        "NativePRNGBlocking:SUN,DRBG:SUN",
    ),
    (
        "jdk.xml.dsig.secureValidationPolicy",
        "disallowAlg http://www.w3.org/TR/1999/REC-xslt-19991116, \
         disallowAlg http://www.w3.org/2001/04/xmldsig-more#rsa-md5, \
         disallowAlg http://www.w3.org/2001/04/xmldsig-more#hmac-md5, \
         disallowAlg http://www.w3.org/2001/04/xmldsig-more#md5, \
         disallowAlg http://www.w3.org/2000/09/xmldsig#sha1, \
         disallowAlg http://www.w3.org/2000/09/xmldsig#dsa-sha1, \
         disallowAlg http://www.w3.org/2000/09/xmldsig#rsa-sha1, \
         disallowAlg http://www.w3.org/2007/05/xmldsig-more#sha1-rsa-MGF1, \
         disallowAlg http://www.w3.org/2001/04/xmldsig-more#ecdsa-sha1, \
         disallowAlg http://www.w3.org/TR/1999/REC-xpath-19991116, \
         maxTransforms 5, maxReferences 30, \
         disallowReferenceUriSchemes file http https, \
         minKeySize RSA 1024, minKeySize DSA 1024, minKeySize EC 224, \
         noDuplicateIds, noRetrievalMethodLoops",
    ),
];

fn security_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let security = vm.resolve_class("java/security/Security")?;
    let properties = vm.resolve_class("java/util/Properties")?;
    let props = vm.new_instance(properties)?;
    let (declaring, ctor) = vm
        .find_method(properties, "<init>", "()V")
        .ok_or_else(|| VmError::internal("Properties lacks <init>"))?;
    vm.invoke_method(declaring, ctor, vec![Value::Ref(props)])?;
    for (key, value) in SECURITY_PROPERTY_DEFAULTS {
        let key = vm.make_string(key)?;
        let value = vm.make_string(value)?;
        property_call(
            vm,
            props,
            "setProperty",
            "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/Object;",
            vec![Value::Ref(props), Value::Ref(key), Value::Ref(value)],
        )?;
    }
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
/// The standard JDK security provider names oxjvm answers to.  oxjvm backs them all with the
/// same built-in implementations, but tests select a provider by name (`SunJCE`, `SunRsaSign`,
/// `SunEC`, ...) and expect `Security.getProvider` / `getInstance(..., provider)` to resolve it.
pub(crate) fn known_provider(name: &str) -> bool {
    const NAMES: [&str; 12] = [
        "SUN",
        "SunRsaSign",
        "SunEC",
        "SunJSSE",
        "SunJCE",
        "SunJGSS",
        "SunSASL",
        "XMLDSig",
        "SunPCSC",
        "JdkLDAP",
        "JdkSASL",
        "SunPKCS11",
    ];
    NAMES.iter().any(|n| n.eq_ignore_ascii_case(name)) || name.eq_ignore_ascii_case("SUNW")
}

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
            if known_provider(&text) {
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
            let names = ["SUN", "SunRsaSign", "SunEC", "SunJSSE", "SunJCE"];
            let array = vm.allocate_object_array(provider_class, names.len())?;
            for (index, name) in names.iter().enumerate() {
                let provider = new_sun_provider(vm, name)?;
                vm.array_set_ref(array, index, provider)?;
            }
            object(array)
        },
    ),
    method(
        "getProviders",
        "(Ljava/lang/String;)[Ljava/security/Provider;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let provider_class = vm.resolve_class("java/security/Provider")?;
            let names = ["SUN", "SunRsaSign", "SunEC", "SunJSSE", "SunJCE"];
            let array = vm.allocate_object_array(provider_class, names.len())?;
            for (index, name) in names.iter().enumerate() {
                let provider = new_sun_provider(vm, name)?;
                vm.array_set_ref(array, index, provider)?;
            }
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
        field(
            "props",
            "Ljava/util/Properties;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
        field(
            "extra",
            "Ljava/security/Provider;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
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
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "name", "Ljava/lang/String;")),
    ),
    method(
        "getVersionStr",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "versionStr",
                "Ljava/lang/String;",
            ))
        },
    ),
    method(
        "getInfo",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "info", "Ljava/lang/String;")),
    ),
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
    method(
        "getServices",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, _| object(new_hash_set(vm)?),
    ),
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
    method(
        "getFormat",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "format",
                "Ljava/lang/String;",
            ))
        },
    ),
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

pub(crate) fn new_public_key(vm: &mut Vm<'_>, algorithm: &str) -> Result<ObjectRef, VmError> {
    new_key(vm, "java/security/OjvmPublicKey", algorithm, "X.509", &[])
}

pub(crate) fn new_private_key(vm: &mut Vm<'_>, algorithm: &str) -> Result<ObjectRef, VmError> {
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
        field(
            "privateKey",
            "Ljava/security/PrivateKey;",
            ACC_PRIVATE,
            None,
        ),
    ],
    &KEY_PAIR_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// java.security.SecureRandom
// -------------------------------------------------------------------------------------------

fn secure_random_next_bytes(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    array: ObjectRef,
) -> Result<(), VmError> {
    if array.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let length = vm.array_length(array)?;
    for index in 0..length {
        let (class, next) = vm.resolve_virtual_method(this, "nextInt", "()I")?;
        let value = vm
            .invoke_method(class, next, vec![Value::Ref(this)])?
            .as_int();
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
    method(
        "getAlgorithm",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("NativePRNG")?),
    ),
    method(
        "getProvider",
        "()Ljava/security/Provider;",
        ACC_PUBLIC,
        |vm, _, _| object(new_sun_provider(vm, "SUN")?),
    ),
    method("setSeed", "(J)V", ACC_PUBLIC, |_, _, _| void()),
    method("setSeed", "([B)V", ACC_PUBLIC, |_, _, _| void()),
    method("nextBytes", "([B)V", ACC_PUBLIC, |vm, _, args| {
        secure_random_next_bytes(vm, receiver(args), ref_arg(args, 1))?;
        void()
    }),
    method("generateSeed", "(I)[B", ACC_PUBLIC, |vm, _, args| {
        let length = int_arg(args, 1).max(0) as usize;
        let bytes = vec![0u8; length];
        object(new_byte_array(vm, &bytes)?)
    }),
    method(
        "getSeed",
        "(I)[B",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let length = int_arg(args, 0).max(0) as usize;
            let bytes = vec![0u8; length];
            object(new_byte_array(vm, &bytes)?)
        },
    ),
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
    set_ref_field(vm, store, "provider", "Ljava/security/Provider;", provider)?;
    Ok(store)
}

const KEY_STORE_METHODS: [oxjvm_vm::NativeMethodDef; 27] = [
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
            if !known_provider(&provider) {
                return Err(vm.throw_new("java/security/NoSuchProviderException", Some(&provider)));
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
        "(Ljava/io/File;[C)Ljava/security/KeyStore;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let store = key_store_instance(vm, "PKCS12")?;
            object(store)
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
    method(
        "getType",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "type", "Ljava/lang/String;")),
    ),
    method(
        "getProvider",
        "()Ljava/security/Provider;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "provider",
                "Ljava/security/Provider;",
            ))
        },
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
    method(
        "containsAlias",
        "(Ljava/lang/String;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(false),
    ),
    method(
        "isKeyEntry",
        "(Ljava/lang/String;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(false),
    ),
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
    method(
        "deleteEntry",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
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
            set_string_field(
                vm,
                receiver(args),
                "algorithm",
                &vm.string_or_empty(ref_arg(args, 1)),
            )?;
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
            if !known_provider(&provider) {
                return Err(vm.throw_new("java/security/NoSuchProviderException", Some(&provider)));
            }
            object(key_pair_generator_instance(
                vm,
                &vm.string_or_empty(ref_arg(args, 0)),
            )?)
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
            object(key_pair_generator_instance(
                vm,
                &vm.string_or_empty(ref_arg(args, 0)),
            )?)
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
    method(
        "getProvider",
        "()Ljava/security/Provider;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "provider",
                "Ljava/security/Provider;",
            ))
        },
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
                vec![Value::Ref(pair), Value::Ref(public), Value::Ref(private)],
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
                vec![Value::Ref(pair), Value::Ref(public), Value::Ref(private)],
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
            if !known_provider(&provider) {
                return Err(vm.throw_new("java/security/NoSuchProviderException", Some(&provider)));
            }
            object(certificate_factory_instance(
                vm,
                &vm.string_or_empty(ref_arg(args, 0)),
            )?)
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
            object(certificate_factory_instance(
                vm,
                &vm.string_or_empty(ref_arg(args, 0)),
            )?)
        },
    ),
    method(
        "getType",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "type", "Ljava/lang/String;")),
    ),
    method(
        "getProvider",
        "()Ljava/security/Provider;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "provider",
                "Ljava/security/Provider;",
            ))
        },
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
            boolean(
                ref_field(vm, this, "encoded", "[B").raw()
                    == ref_field(vm, other, "encoded", "[B").raw(),
            )
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
    method("getEncoded", "()[B", ACC_PUBLIC, |vm, _, args| {
        object(ref_field(vm, receiver(args), "encoded", "[B"))
    }),
    method(
        "verify",
        "(Ljava/security/PublicKey;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
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
    method("checkValidity", "()V", ACC_PUBLIC, |_, _, _| void()),
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
            vm.invoke_method(declaring, ctor, vec![Value::Ref(instance), Value::Long(0)])
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
    method("getTBSCertificate", "()[B", ACC_PUBLIC, |vm, _, _| {
        object(new_byte_array(vm, &[])?)
    }),
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
    method("getSigAlgParams", "()[B", ACC_PUBLIC, |_, _, _| {
        object(ObjectRef::NULL)
    }),
    method("getIssuerUniqueID", "()[Z", ACC_PUBLIC, |_, _, _| {
        object(ObjectRef::NULL)
    }),
    method("getSubjectUniqueID", "()[Z", ACC_PUBLIC, |_, _, _| {
        object(ObjectRef::NULL)
    }),
    method("getKeyUsage", "()[Z", ACC_PUBLIC, |_, _, _| {
        object(ObjectRef::NULL)
    }),
    method("getBasicConstraints", "()I", ACC_PUBLIC, |_, _, _| int(-1)),
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
    method(
        "getLocation",
        "()Ljava/net/URL;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "location", "Ljava/net/URL;")),
    ),
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
    method(
        "implies",
        "(Ljava/security/CodeSource;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(true),
    ),
    method("hashCode", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| via_code_source_to_string(vm, args),
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
            set_ref_field(
                vm,
                this,
                "codeSource",
                "Ljava/security/CodeSource;",
                ref_arg(args, 1),
            )?;
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
        field(
            "codeSource",
            "Ljava/security/CodeSource;",
            ACC_PRIVATE,
            None,
        ),
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
    let (class, method) = vm.resolve_virtual_method(action, "run", "()Ljava/lang/Object;")?;
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
    &[field(
        "exception",
        "Ljava/lang/Exception;",
        ACC_PRIVATE,
        None,
    )],
    &[
        method(
            "<init>",
            "(Ljava/lang/Exception;)V",
            ACC_PUBLIC,
            |vm, _, args| {
                let this = receiver(args);
                set_ref_field(
                    vm,
                    this,
                    "exception",
                    "Ljava/lang/Exception;",
                    ref_arg(args, 1),
                )?;
                let exception_ctor =
                    vm.find_method(vm.class_of(this), "<init>", "(Ljava/lang/Throwable;)V");
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
                return Err(
                    vm.throw_new("java/lang/NullPointerException", Some("name can't be null"))
                );
            }
            set_ref_field(vm, receiver(args), "name", "Ljava/lang/String;", name)?;
            void()
        },
    ),
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "name", "Ljava/lang/String;")),
    ),
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
            vm.invoke_method(declaring, ctor, vec![Value::Ref(this), Value::Ref(name)])?;
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
    set_ref_field(vm, cipher, "provider", "Ljava/security/Provider;", provider)?;
    Ok(cipher)
}

const CIPHER_METHODS: [oxjvm_vm::NativeMethodDef; 24] = [
    method(
        "getMaxAllowedKeyLength",
        "(Ljava/lang/String;)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            // oxjvm ships the unlimited-strength policy: every transformation allows the
            // largest key size the algorithm can express.
            int(i32::MAX)
        },
    ),
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
            if !known_provider(&provider) {
                return Err(vm.throw_new("java/security/NoSuchProviderException", Some(&provider)));
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
    method(
        "getAlgorithm",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "transformation",
                "Ljava/lang/String;",
            ))
        },
    ),
    method(
        "getProvider",
        "()Ljava/security/Provider;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "provider",
                "Ljava/security/Provider;",
            ))
        },
    ),
    method("getBlockSize", "()I", ACC_PUBLIC, |_, _, _| int(16)),
    method("getOutputSize", "(I)I", ACC_PUBLIC, |_, _, args| {
        int(int_arg(args, 1).max(0))
    }),
    method("getIV", "()[B", ACC_PUBLIC, |_, _, _| {
        object(ObjectRef::NULL)
    }),
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
    method("update", "([B)[B", ACC_PUBLIC, |_, _, args| {
        object(ref_arg(args, 1))
    }),
    method("update", "([BII)[B", ACC_PUBLIC, |vm, _, args| {
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
    }),
    method("doFinal", "()[B", ACC_PUBLIC, |vm, _, _| {
        object(new_byte_array(vm, &[])?)
    }),
    method("doFinal", "([B)[B", ACC_PUBLIC, |_, _, args| {
        object(ref_arg(args, 1))
    }),
    method("doFinal", "([BII)[B", ACC_PUBLIC, |vm, _, args| {
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
    }),
    method("doFinal", "([BI)I", ACC_PUBLIC, |vm, _, args| {
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
    }),
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
                return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("Empty key")));
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
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "algorithm",
                "Ljava/lang/String;",
            ))
        },
    ),
    method(
        "getFormat",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("RAW")?),
    ),
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
        method("<init>", "([B)V", ACC_PUBLIC, |vm, _, args| {
            let iv = ref_arg(args, 1);
            if iv.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            set_ref_field(vm, receiver(args), "iv", "[B", iv)?;
            void()
        }),
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
        method("<init>", "(I[B)V", ACC_PUBLIC, |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "tLen", "I")?;
            vm.set_instance_int(this, declaring, field, int_arg(args, 1));
            set_ref_field(vm, this, "src", "[B", ref_arg(args, 2))?;
            void()
        }),
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
            if !known_provider(&provider) {
                return Err(vm.throw_new("java/security/NoSuchProviderException", Some(&provider)));
            }
            object(ssl_context_instance(
                vm,
                &vm.string_or_empty(ref_arg(args, 0)),
            )?)
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
            object(ssl_context_instance(
                vm,
                &vm.string_or_empty(ref_arg(args, 0)),
            )?)
        },
    ),
    method(
        "getProtocol",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "protocol",
                "Ljava/lang/String;",
            ))
        },
    ),
    method(
        "getProvider",
        "()Ljava/security/Provider;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "provider",
                "Ljava/security/Provider;",
            ))
        },
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
        method("<init>", "([B)V", ACC_PUBLIC, |vm, _, args| {
            let encoded = ref_arg(args, 1);
            if encoded.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            set_ref_field(vm, receiver(args), "encoded", "[B", encoded)?;
            void()
        }),
        method("getEncoded", "()[B", ACC_PUBLIC, |vm, _, args| {
            object(ref_field(vm, receiver(args), "encoded", "[B"))
        }),
        method(
            "getFormat",
            "()Ljava/lang/String;",
            ACC_PUBLIC,
            |vm, _, _| object(vm.make_string("X.509")?),
        ),
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
        method("<init>", "([B)V", ACC_PUBLIC, |vm, _, args| {
            let encoded = ref_arg(args, 1);
            if encoded.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            set_ref_field(vm, receiver(args), "encoded", "[B", encoded)?;
            void()
        }),
        method("getEncoded", "()[B", ACC_PUBLIC, |vm, _, args| {
            object(ref_field(vm, receiver(args), "encoded", "[B"))
        }),
        method(
            "getFormat",
            "()Ljava/lang/String;",
            ACC_PUBLIC,
            |vm, _, _| object(vm.make_string("PKCS#8")?),
        ),
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

// -------------------------------------------------------------------------------------------
// java.security.MessageDigest (real SHA-256 / SHA-1 / MD5)
// -------------------------------------------------------------------------------------------

fn sha256(data: &[u8]) -> Vec<u8> {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    let mut out = Vec::with_capacity(32);
    for value in h {
        out.extend_from_slice(&value.to_be_bytes());
    }
    out
}

fn sha1(data: &[u8]) -> Vec<u8> {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut out = Vec::with_capacity(20);
    for value in h {
        out.extend_from_slice(&value.to_be_bytes());
    }
    out
}

fn md5(data: &[u8]) -> Vec<u8> {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
        0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
        0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
        0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
        0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
        0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
        0xeb86d391,
    ];
    let mut h: [u32; 4] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476];
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_le_bytes());
    for chunk in msg.chunks(64) {
        let mut m = [0u32; 16];
        for i in 0..16 {
            m[i] = u32::from_le_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        let [mut a, mut b, mut c, mut d] = h;
        for i in 0..64 {
            let (f, g) = match i {
                0..=15 => ((b & c) | ((!b) & d), i),
                16..=31 => ((d & b) | ((!d) & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | (!d)), (7 * i) % 16),
            };
            let temp = d;
            d = c;
            c = b;
            let sum = a.wrapping_add(f).wrapping_add(K[i]).wrapping_add(m[g]);
            b = b.wrapping_add(sum.rotate_left(S[i]));
            a = temp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
    }
    let mut out = Vec::with_capacity(16);
    for value in h {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

/// A digest algorithm: canonical name, hash function and output length in bytes.
type DigestAlgorithm = (&'static str, fn(&[u8]) -> Vec<u8>, i32);

/// Normalize an algorithm name to the canonical form and return its digest function and length.
fn digest_algorithm(name: &str) -> Option<DigestAlgorithm> {
    let upper = name.to_ascii_uppercase().replace('-', "");
    match upper.as_str() {
        "SHA256" => Some(("SHA-256", sha256, 32)),
        "SHA1" | "SHA" => Some(("SHA-1", sha1, 20)),
        "MD5" => Some(("MD5", md5, 16)),
        _ => None,
    }
}

fn message_digest_data(vm: &mut Vm<'_>, this: ObjectRef) -> Vec<u8> {
    let class = vm.class_of(this);
    let buf = vm
        .read_ref_field(this, class, "buf", "[B")
        .unwrap_or(ObjectRef::NULL);
    let count = crate::lang::read_int_field(vm, this, "count").max(0) as usize;
    let mut bytes = Vec::with_capacity(count);
    for index in 0..count {
        if let Ok(value) = vm.array_get(buf, index as i32) {
            bytes.push(value.as_int() as u8);
        }
    }
    bytes
}

fn message_digest_append(vm: &mut Vm<'_>, this: ObjectRef, bytes: &[u8]) -> Result<(), VmError> {
    let existing = message_digest_data(vm, this);
    let mut all = existing;
    all.extend_from_slice(bytes);
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, all.len())?;
    for (index, byte) in all.iter().enumerate() {
        vm.array_set(
            array,
            index as i32,
            Value::Int(i32::from(*byte as i8)),
            false,
        )?;
    }
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "buf", "[B")?;
    vm.set_instance_ref(this, declaring, field, array);
    crate::lang::write_int_field(vm, this, "count", all.len() as i32);
    Ok(())
}

fn message_digest_compute(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Vec<u8>, VmError> {
    let algorithm = string_field(vm, this, "algorithm");
    let data = message_digest_data(vm, this);
    let (_, func, _) = digest_algorithm(&algorithm).ok_or_else(|| {
        vm.throw_new("java/lang/IllegalStateException", Some("unknown algorithm"))
    })?;
    Ok(func(&data))
}

fn message_digest_instance(vm: &mut Vm<'_>, algorithm: &str) -> Result<ObjectRef, VmError> {
    let (canonical, _, _) = digest_algorithm(algorithm).ok_or_else(|| {
        vm.throw_new(
            "java/security/NoSuchAlgorithmException",
            Some(&alloc::format!("{algorithm} MessageDigest not available")),
        )
    })?;
    let class = vm.resolve_class("java/security/MessageDigest")?;
    let instance = vm.new_instance(class)?;
    set_string_field(vm, instance, "algorithm", canonical)?;
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, 0)?;
    let (declaring, field) = vm.find_field(class, "buf", "[B")?;
    vm.set_instance_ref(instance, declaring, field, array);
    crate::lang::write_int_field(vm, instance, "count", 0);
    Ok(instance)
}

const MESSAGE_DIGEST_METHODS: [oxjvm_vm::NativeMethodDef; 16] = [
    method(
        "getInstance",
        "(Ljava/lang/String;)Ljava/security/MessageDigest;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(message_digest_instance(vm, &vm.string_or_empty(name))?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/security/MessageDigest;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let provider = vm.string_or_empty(ref_arg(args, 1));
            if !known_provider(&provider) {
                return Err(vm.throw_new("java/security/NoSuchProviderException", Some(&provider)));
            }
            object(message_digest_instance(
                vm,
                &vm.string_or_empty(ref_arg(args, 0)),
            )?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/security/Provider;)Ljava/security/MessageDigest;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(message_digest_instance(
                vm,
                &vm.string_or_empty(ref_arg(args, 0)),
            )?)
        },
    ),
    method(
        "getAlgorithm",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(vm.make_string(&string_field(vm, receiver(args), "algorithm"))?),
    ),
    method("getDigestLength", "()I", ACC_PUBLIC, |vm, _, args| {
        let algorithm = string_field(vm, receiver(args), "algorithm");
        let (_, _, len) = digest_algorithm(&algorithm).unwrap_or(("", sha256, 0));
        int(len)
    }),
    method("update", "(B)V", ACC_PUBLIC, |vm, _, args| {
        message_digest_append(vm, receiver(args), &[(int_arg(args, 1) & 0xFF) as u8])?;
        void()
    }),
    method("update", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let array = ref_arg(args, 1);
        if array.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        let length = vm.array_length(array)? as i32;
        let mut bytes = Vec::with_capacity(length as usize);
        for index in 0..length {
            bytes.push(vm.array_get(array, index)?.as_int() as u8);
        }
        message_digest_append(vm, receiver(args), &bytes)?;
        void()
    }),
    method("update", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        let array = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        if array.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        let capacity = vm.array_length(array)? as i32;
        if offset < 0 || length < 0 || offset + length > capacity {
            return Err(vm.throw_new("java/lang/ArrayIndexOutOfBoundsException", None));
        }
        let mut bytes = Vec::with_capacity(length as usize);
        for index in 0..length {
            bytes.push(vm.array_get(array, offset + index)?.as_int() as u8);
        }
        message_digest_append(vm, receiver(args), &bytes)?;
        void()
    }),
    method("digest", "()[B", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let digest = message_digest_compute(vm, this)?;
        let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, digest.len())?;
        for (index, byte) in digest.iter().enumerate() {
            vm.array_set(
                array,
                index as i32,
                Value::Int(i32::from(*byte as i8)),
                false,
            )?;
        }
        crate::lang::write_int_field(vm, this, "count", 0);
        object(array)
    }),
    method("digest", "([B)[B", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let array = ref_arg(args, 1);
        if !array.is_null() {
            let length = vm.array_length(array)? as i32;
            let mut bytes = Vec::with_capacity(length as usize);
            for index in 0..length {
                bytes.push(vm.array_get(array, index)?.as_int() as u8);
            }
            message_digest_append(vm, this, &bytes)?;
        }
        let digest = message_digest_compute(vm, this)?;
        let out = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, digest.len())?;
        for (index, byte) in digest.iter().enumerate() {
            vm.array_set(out, index as i32, Value::Int(i32::from(*byte as i8)), false)?;
        }
        crate::lang::write_int_field(vm, this, "count", 0);
        object(out)
    }),
    method("reset", "()V", ACC_PUBLIC, |vm, _, args| {
        crate::lang::write_int_field(vm, receiver(args), "count", 0);
        void()
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(vm.make_string(&string_field(vm, receiver(args), "algorithm"))?),
    ),
    method(
        "isEqual",
        "([B[B)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let a = ref_arg(args, 0);
            let b = ref_arg(args, 1);
            let la = vm.array_length(a)? as i32;
            let lb = vm.array_length(b)? as i32;
            if la != lb {
                return boolean(false);
            }
            for index in 0..la {
                if vm.array_get(a, index)?.as_int() != vm.array_get(b, index)?.as_int() {
                    return boolean(false);
                }
            }
            boolean(true)
        },
    ),
    method(
        "clone",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |_vm, _, args| object(receiver(args)),
    ),
    method(
        "getProvider",
        "()Ljava/security/Provider;",
        ACC_PUBLIC,
        |vm, _, _| object(new_sun_provider(vm, "SUN")?),
    ),
    method("digest", "([BII)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let buf = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let digest = message_digest_compute(vm, this)?;
        let len = digest.len() as i32;
        if vm.array_length(buf)? as i32 - offset < len {
            return Err(vm.throw_new("java/security/DigestException", Some("buffer too small")));
        }
        for (index, byte) in digest.iter().enumerate() {
            vm.array_set(
                buf,
                offset + index as i32,
                Value::Int(i32::from(*byte as i8)),
                false,
            )?;
        }
        crate::lang::write_int_field(vm, this, "count", 0);
        int(len)
    }),
];

pub(crate) const MESSAGE_DIGEST: oxjvm_vm::NativeClass = class(
    "java/security/MessageDigest",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[
        field("algorithm", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("buf", "[B", ACC_PRIVATE, None),
        field("count", "I", ACC_PRIVATE, None),
    ],
    &MESSAGE_DIGEST_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Shared crypto helpers
// -------------------------------------------------------------------------------------------

/// Copy the contents of a Java `byte[]` into a Rust vector (null yields empty).
fn read_byte_array(vm: &mut Vm<'_>, array: ObjectRef) -> Result<Vec<u8>, VmError> {
    if array.is_null() {
        return Ok(Vec::new());
    }
    let length = vm.array_length(array)?;
    let mut bytes = Vec::with_capacity(length);
    for index in 0..length {
        bytes.push(vm.array_get(array, index as i32)?.as_int() as u8);
    }
    Ok(bytes)
}

/// Generate `len` cryptographically-styled random bytes through a fresh SecureRandom.
fn random_bytes(vm: &mut Vm<'_>, len: usize) -> Result<Vec<u8>, VmError> {
    let class = vm.resolve_class("java/security/SecureRandom")?;
    let random = vm.new_instance(class)?;
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, len)?;
    let (declaring, method) = vm.resolve_virtual_method(random, "nextBytes", "([B)V")?;
    vm.invoke_method(
        declaring,
        method,
        vec![Value::Ref(random), Value::Ref(array)],
    )?;
    read_byte_array(vm, array)
}

/// Build a `javax.crypto.spec.SecretKeySpec` from raw key material.
fn new_secret_key_spec(
    vm: &mut Vm<'_>,
    bytes: &[u8],
    algorithm: &str,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("javax/crypto/spec/SecretKeySpec")?;
    let spec = vm.new_instance(class)?;
    let key = new_byte_array(vm, bytes)?;
    set_ref_field(vm, spec, "key", "[B", key)?;
    set_string_field(vm, spec, "algorithm", algorithm)?;
    Ok(spec)
}

/// Read a Key's raw encoding through its `getEncoded()` method.
fn key_encoded(vm: &mut Vm<'_>, key: ObjectRef) -> Result<Vec<u8>, VmError> {
    if key.is_null() {
        return Ok(Vec::new());
    }
    let (class, method) = vm.resolve_virtual_method(key, "getEncoded", "()[B")?;
    let array = vm
        .invoke_method(class, method, vec![Value::Ref(key)])?
        .as_ref();
    read_byte_array(vm, array)
}

/// Read a Key's algorithm name through its `getAlgorithm()` method.
fn key_algorithm(vm: &mut Vm<'_>, key: ObjectRef) -> Result<String, VmError> {
    if key.is_null() {
        return Ok(String::new());
    }
    let (class, method) = vm.resolve_virtual_method(key, "getAlgorithm", "()Ljava/lang/String;")?;
    let text = vm
        .invoke_method(class, method, vec![Value::Ref(key)])?
        .as_ref();
    Ok(vm.string_or_empty(text))
}

/// Accumulate bytes into a `buf`/`count` pair (shared by Mac and Signature).
fn crypto_append(vm: &mut Vm<'_>, this: ObjectRef, bytes: &[u8]) -> Result<(), VmError> {
    let class = vm.class_of(this);
    let buf = vm
        .read_ref_field(this, class, "buf", "[B")
        .unwrap_or(ObjectRef::NULL);
    let count = crate::lang::read_int_field(vm, this, "count").max(0) as usize;
    let mut all = read_byte_array(vm, buf)?;
    all.truncate(count);
    all.extend_from_slice(bytes);
    let array = new_byte_array(vm, &all)?;
    let (declaring, field) = vm.find_field(class, "buf", "[B")?;
    vm.set_instance_ref(this, declaring, field, array);
    crate::lang::write_int_field(vm, this, "count", all.len() as i32);
    Ok(())
}

/// Read the accumulated bytes of a `buf`/`count` pair.
fn crypto_data(vm: &mut Vm<'_>, this: ObjectRef) -> Vec<u8> {
    let class = vm.class_of(this);
    let buf = vm
        .read_ref_field(this, class, "buf", "[B")
        .unwrap_or(ObjectRef::NULL);
    let count = crate::lang::read_int_field(vm, this, "count").max(0) as usize;
    let mut bytes = read_byte_array(vm, buf).unwrap_or_default();
    bytes.truncate(count);
    bytes
}

// -------------------------------------------------------------------------------------------
// javax.crypto.KeyGenerator
// -------------------------------------------------------------------------------------------

/// Default key size (in bits) for the common symmetric algorithms.
fn default_key_size(algorithm: &str) -> i32 {
    let upper = algorithm.to_ascii_uppercase();
    if let Some(hash) = upper.strip_prefix("HMAC") {
        return match hash {
            "SHA1" | "SHA" => 160,
            "SHA224" => 224,
            "SHA256" => 256,
            "SHA384" => 384,
            "SHA512" => 512,
            "MD5" => 128,
            _ => 128,
        };
    }
    match upper.as_str() {
        "DES" => 56,
        "DESEDE" | "TRIPLEDES" => 168,
        "CHACHA20" => 256,
        _ => 128,
    }
}

fn key_generator_instance(vm: &mut Vm<'_>, algorithm: &str) -> Result<ObjectRef, VmError> {
    if algorithm.is_empty() {
        return Err(vm.throw_new(
            "java/security/NoSuchAlgorithmException",
            Some("algorithm must not be empty"),
        ));
    }
    let class = vm.resolve_class("javax/crypto/KeyGenerator")?;
    let generator = vm.new_instance(class)?;
    set_string_field(vm, generator, "algorithm", algorithm)?;
    let provider = new_sun_provider(vm, "SunJCE")?;
    set_ref_field(
        vm,
        generator,
        "provider",
        "Ljava/security/Provider;",
        provider,
    )?;
    let (declaring, field) = vm.find_field(class, "keySize", "I")?;
    vm.set_instance_int(generator, declaring, field, default_key_size(algorithm));
    Ok(generator)
}

const KEY_GENERATOR_METHODS: [oxjvm_vm::NativeMethodDef; 13] = [
    method(
        "getInstance",
        "(Ljava/lang/String;)Ljavax/crypto/KeyGenerator;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(key_generator_instance(vm, &vm.string_or_empty(name))?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/lang/String;)Ljavax/crypto/KeyGenerator;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let provider = vm.string_or_empty(ref_arg(args, 1));
            if !known_provider(&provider) {
                return Err(vm.throw_new("java/security/NoSuchProviderException", Some(&provider)));
            }
            object(key_generator_instance(
                vm,
                &vm.string_or_empty(ref_arg(args, 0)),
            )?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/security/Provider;)Ljavax/crypto/KeyGenerator;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(key_generator_instance(
                vm,
                &vm.string_or_empty(ref_arg(args, 0)),
            )?)
        },
    ),
    method(
        "getAlgorithm",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| object(vm.make_string(&string_field(vm, receiver(args), "algorithm"))?),
    ),
    method(
        "getProvider",
        "()Ljava/security/Provider;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "provider",
                "Ljava/security/Provider;",
            ))
        },
    ),
    method("init", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let size = int_arg(args, 1);
        let class = vm.class_of(this);
        let (declaring, field) = vm.find_field(class, "keySize", "I")?;
        vm.set_instance_int(this, declaring, field, size);
        void()
    }),
    method(
        "init",
        "(ILjava/security/SecureRandom;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let size = int_arg(args, 1);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "keySize", "I")?;
            vm.set_instance_int(this, declaring, field, size);
            void()
        },
    ),
    method(
        "init",
        "(Ljava/security/SecureRandom;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "init",
        "(Ljava/security/spec/AlgorithmParameterSpec;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "init",
        "(Ljava/security/spec/AlgorithmParameterSpec;Ljava/security/SecureRandom;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "generateKey",
        "()Ljavax/crypto/SecretKey;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let algorithm = string_field(vm, this, "algorithm");
            let class = vm.class_of(this);
            let key_size = vm
                .read_instance_int_named(this, class, "keySize", "I")
                .unwrap_or_else(|| default_key_size(&algorithm));
            let byte_len = ((key_size + 7) / 8).max(1) as usize;
            let bytes = random_bytes(vm, byte_len)?;
            object(new_secret_key_spec(vm, &bytes, &algorithm)?)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = alloc::format!(
                "KeyGenerator[{}]",
                string_field(vm, receiver(args), "algorithm")
            );
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "clone",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |_vm, _, args| object(receiver(args)),
    ),
];

pub(crate) const KEY_GENERATOR: oxjvm_vm::NativeClass = class(
    "javax/crypto/KeyGenerator",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("algorithm", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("keySize", "I", ACC_PRIVATE, None),
        field("provider", "Ljava/security/Provider;", ACC_PRIVATE, None),
    ],
    &KEY_GENERATOR_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// javax.crypto.Mac (real HMAC for HmacSHA1 / HmacSHA256 / HmacMD5)
// -------------------------------------------------------------------------------------------

/// An HMAC hash function paired with its block size in bytes.
type HmacHash = (fn(&[u8]) -> Vec<u8>, usize);

/// Map an HMAC algorithm name to its underlying hash function and block size.
fn hmac_hash(name: &str) -> Option<HmacHash> {
    let upper = name.to_ascii_uppercase().replace('-', "");
    let hash = upper.strip_prefix("HMAC")?;
    match hash {
        "SHA1" | "SHA" => Some((sha1, 64)),
        "SHA256" => Some((sha256, 64)),
        "MD5" => Some((md5, 64)),
        _ => None,
    }
}

/// Compute HMAC per RFC 2104: H((K' ^ opad) || H((K' ^ ipad) || message)).
fn hmac(func: fn(&[u8]) -> Vec<u8>, block: usize, key: &[u8], message: &[u8]) -> Vec<u8> {
    let mut key_block = vec![0u8; block];
    if key.len() > block {
        let hashed = func(key);
        key_block[..hashed.len()].copy_from_slice(&hashed);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Vec::with_capacity(block + message.len());
    for byte in &key_block {
        inner.push(byte ^ 0x36);
    }
    inner.extend_from_slice(message);
    let inner_hash = func(&inner);
    let mut outer = Vec::with_capacity(block + inner_hash.len());
    for byte in &key_block {
        outer.push(byte ^ 0x5C);
    }
    outer.extend_from_slice(&inner_hash);
    func(&outer)
}

fn mac_instance(vm: &mut Vm<'_>, algorithm: &str) -> Result<ObjectRef, VmError> {
    if hmac_hash(algorithm).is_none() {
        return Err(vm.throw_new(
            "java/security/NoSuchAlgorithmException",
            Some(&alloc::format!("{algorithm} Mac not available")),
        ));
    }
    let class = vm.resolve_class("javax/crypto/Mac")?;
    let mac = vm.new_instance(class)?;
    set_string_field(vm, mac, "algorithm", algorithm)?;
    let provider = new_sun_provider(vm, "SunJCE")?;
    set_ref_field(vm, mac, "provider", "Ljava/security/Provider;", provider)?;
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, 0)?;
    let (declaring, field) = vm.find_field(class, "buf", "[B")?;
    vm.set_instance_ref(mac, declaring, field, array);
    crate::lang::write_int_field(vm, mac, "count", 0);
    Ok(mac)
}

fn mac_compute(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Vec<u8>, VmError> {
    let algorithm = string_field(vm, this, "algorithm");
    let (func, block) = hmac_hash(&algorithm).ok_or_else(|| {
        vm.throw_new("java/lang/IllegalStateException", Some("unknown algorithm"))
    })?;
    let class = vm.class_of(this);
    let key_array = vm
        .read_ref_field(this, class, "key", "[B")
        .unwrap_or(ObjectRef::NULL);
    let key = read_byte_array(vm, key_array)?;
    let message = crypto_data(vm, this);
    Ok(hmac(func, block, &key, &message))
}

const MAC_METHODS: [oxjvm_vm::NativeMethodDef; 17] = [
    method(
        "getInstance",
        "(Ljava/lang/String;)Ljavax/crypto/Mac;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(mac_instance(vm, &vm.string_or_empty(name))?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/lang/String;)Ljavax/crypto/Mac;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let provider = vm.string_or_empty(ref_arg(args, 1));
            if !known_provider(&provider) {
                return Err(vm.throw_new("java/security/NoSuchProviderException", Some(&provider)));
            }
            object(mac_instance(vm, &vm.string_or_empty(ref_arg(args, 0)))?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/security/Provider;)Ljavax/crypto/Mac;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(mac_instance(vm, &vm.string_or_empty(ref_arg(args, 0)))?)
        },
    ),
    method(
        "getAlgorithm",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| object(vm.make_string(&string_field(vm, receiver(args), "algorithm"))?),
    ),
    method(
        "getProvider",
        "()Ljava/security/Provider;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "provider",
                "Ljava/security/Provider;",
            ))
        },
    ),
    method(
        "getMacLength",
        "()I",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let algorithm = string_field(vm, receiver(args), "algorithm");
            let len = hmac_hash(&algorithm)
                .map(|(func, _)| func(&[]).len())
                .unwrap_or(0);
            int(len as i32)
        },
    ),
    method(
        "init",
        "(Ljava/security/Key;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let key = ref_arg(args, 1);
            if key.is_null() {
                return Err(vm.throw_new("java/security/InvalidKeyException", Some("Null key")));
            }
            let bytes = key_encoded(vm, key)?;
            let array = new_byte_array(vm, &bytes)?;
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "key", "[B")?;
            vm.set_instance_ref(this, declaring, field, array);
            crate::lang::write_int_field(vm, this, "count", 0);
            void()
        },
    ),
    method(
        "init",
        "(Ljava/security/Key;Ljava/security/spec/AlgorithmParameterSpec;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let key = ref_arg(args, 1);
            if key.is_null() {
                return Err(vm.throw_new("java/security/InvalidKeyException", Some("Null key")));
            }
            let bytes = key_encoded(vm, key)?;
            let array = new_byte_array(vm, &bytes)?;
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "key", "[B")?;
            vm.set_instance_ref(this, declaring, field, array);
            crate::lang::write_int_field(vm, this, "count", 0);
            void()
        },
    ),
    method("update", "(B)V", ACC_PUBLIC, |vm, _, args| {
        crypto_append(vm, receiver(args), &[(int_arg(args, 1) & 0xFF) as u8])?;
        void()
    }),
    method("update", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = read_byte_array(vm, ref_arg(args, 1))?;
        crypto_append(vm, receiver(args), &bytes)?;
        void()
    }),
    method("update", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        let array = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        if array.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        let capacity = vm.array_length(array)? as i32;
        if offset < 0 || length < 0 || offset + length > capacity {
            return Err(vm.throw_new("java/lang/ArrayIndexOutOfBoundsException", None));
        }
        let mut bytes = Vec::with_capacity(length as usize);
        for index in 0..length {
            bytes.push(vm.array_get(array, offset + index)?.as_int() as u8);
        }
        crypto_append(vm, receiver(args), &bytes)?;
        void()
    }),
    method("doFinal", "()[B", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let mac = mac_compute(vm, this)?;
        crate::lang::write_int_field(vm, this, "count", 0);
        object(new_byte_array(vm, &mac)?)
    }),
    method("doFinal", "([B)[B", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let input = read_byte_array(vm, ref_arg(args, 1))?;
        crypto_append(vm, this, &input)?;
        let mac = mac_compute(vm, this)?;
        crate::lang::write_int_field(vm, this, "count", 0);
        object(new_byte_array(vm, &mac)?)
    }),
    method("doFinal", "([BI)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let buf = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let mac = mac_compute(vm, this)?;
        if vm.array_length(buf)? as i32 - offset < mac.len() as i32 {
            return Err(vm.throw_new(
                "javax/crypto/ShortBufferException",
                Some("buffer too small"),
            ));
        }
        for (index, byte) in mac.iter().enumerate() {
            vm.array_set(
                buf,
                offset + index as i32,
                Value::Int(i32::from(*byte as i8)),
                false,
            )?;
        }
        crate::lang::write_int_field(vm, this, "count", 0);
        int(mac.len() as i32)
    }),
    method("reset", "()V", ACC_PUBLIC, |vm, _, args| {
        crate::lang::write_int_field(vm, receiver(args), "count", 0);
        void()
    }),
    method(
        "clone",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |_vm, _, args| object(receiver(args)),
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = alloc::format!("Mac[{}]", string_field(vm, receiver(args), "algorithm"));
            object(vm.make_string(&text)?)
        },
    ),
];

pub(crate) const MAC: oxjvm_vm::NativeClass = class(
    "javax/crypto/Mac",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("algorithm", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("provider", "Ljava/security/Provider;", ACC_PRIVATE, None),
        field("key", "[B", ACC_PRIVATE, None),
        field("buf", "[B", ACC_PRIVATE, None),
        field("count", "I", ACC_PRIVATE, None),
    ],
    &MAC_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// java.security.Signature (hash-based sign/verify round-trip)
// -------------------------------------------------------------------------------------------

/// A signature's digest: canonical name and hash function.
type SignatureDigest = (&'static str, fn(&[u8]) -> Vec<u8>);

/// Extract the digest portion of a "SHA256withRSA"-style algorithm name.
fn signature_digest(name: &str) -> Option<SignatureDigest> {
    let upper = name.to_ascii_uppercase();
    let digest = upper.split("WITH").next()?.trim();
    match digest {
        "SHA256" => Some(("SHA-256", sha256)),
        "SHA1" | "SHA" => Some(("SHA-1", sha1)),
        "MD5" => Some(("MD5", md5)),
        "NONE" => Some(("NONE", sha256)),
        _ => None,
    }
}

fn signature_instance(vm: &mut Vm<'_>, algorithm: &str) -> Result<ObjectRef, VmError> {
    if signature_digest(algorithm).is_none() {
        return Err(vm.throw_new(
            "java/security/NoSuchAlgorithmException",
            Some(&alloc::format!("{algorithm} Signature not available")),
        ));
    }
    let class = vm.resolve_class("java/security/Signature")?;
    let signature = vm.new_instance(class)?;
    set_string_field(vm, signature, "algorithm", algorithm)?;
    let provider = new_sun_provider(vm, "SUN")?;
    set_ref_field(
        vm,
        signature,
        "provider",
        "Ljava/security/Provider;",
        provider,
    )?;
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, 0)?;
    let (declaring, field) = vm.find_field(class, "buf", "[B")?;
    vm.set_instance_ref(signature, declaring, field, array);
    crate::lang::write_int_field(vm, signature, "count", 0);
    crate::lang::write_int_field(vm, signature, "state", 0);
    Ok(signature)
}

fn signature_hash(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Vec<u8>, VmError> {
    let algorithm = string_field(vm, this, "algorithm");
    let (_, func) = signature_digest(&algorithm).ok_or_else(|| {
        vm.throw_new("java/lang/IllegalStateException", Some("unknown algorithm"))
    })?;
    let data = crypto_data(vm, this);
    Ok(func(&data))
}

const SIGNATURE_METHODS: [oxjvm_vm::NativeMethodDef; 19] = [
    method(
        "getInstance",
        "(Ljava/lang/String;)Ljava/security/Signature;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(signature_instance(vm, &vm.string_or_empty(name))?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/security/Signature;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let provider = vm.string_or_empty(ref_arg(args, 1));
            if !known_provider(&provider) {
                return Err(vm.throw_new("java/security/NoSuchProviderException", Some(&provider)));
            }
            object(signature_instance(
                vm,
                &vm.string_or_empty(ref_arg(args, 0)),
            )?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/security/Provider;)Ljava/security/Signature;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(signature_instance(
                vm,
                &vm.string_or_empty(ref_arg(args, 0)),
            )?)
        },
    ),
    method(
        "getAlgorithm",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(vm.make_string(&string_field(vm, receiver(args), "algorithm"))?),
    ),
    method(
        "getProvider",
        "()Ljava/security/Provider;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "provider",
                "Ljava/security/Provider;",
            ))
        },
    ),
    method(
        "initSign",
        "(Ljava/security/PrivateKey;)V",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/security/InvalidKeyException", Some("Null key")));
            }
            crate::lang::write_int_field(vm, this, "state", 2);
            crate::lang::write_int_field(vm, this, "count", 0);
            void()
        },
    ),
    method(
        "initSign",
        "(Ljava/security/PrivateKey;Ljava/security/SecureRandom;)V",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/security/InvalidKeyException", Some("Null key")));
            }
            crate::lang::write_int_field(vm, this, "state", 2);
            crate::lang::write_int_field(vm, this, "count", 0);
            void()
        },
    ),
    method(
        "initVerify",
        "(Ljava/security/PublicKey;)V",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/security/InvalidKeyException", Some("Null key")));
            }
            crate::lang::write_int_field(vm, this, "state", 3);
            crate::lang::write_int_field(vm, this, "count", 0);
            void()
        },
    ),
    method(
        "initVerify",
        "(Ljava/security/cert/Certificate;)V",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new(
                    "java/security/InvalidKeyException",
                    Some("Null certificate"),
                ));
            }
            crate::lang::write_int_field(vm, this, "state", 3);
            crate::lang::write_int_field(vm, this, "count", 0);
            void()
        },
    ),
    method("update", "(B)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if crate::lang::read_int_field(vm, this, "state") == 0 {
            return Err(vm.throw_new("java/security/SignatureException", Some("not initialized")));
        }
        crypto_append(vm, this, &[(int_arg(args, 1) & 0xFF) as u8])?;
        void()
    }),
    method("update", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if crate::lang::read_int_field(vm, this, "state") == 0 {
            return Err(vm.throw_new("java/security/SignatureException", Some("not initialized")));
        }
        let bytes = read_byte_array(vm, ref_arg(args, 1))?;
        crypto_append(vm, this, &bytes)?;
        void()
    }),
    method("update", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if crate::lang::read_int_field(vm, this, "state") == 0 {
            return Err(vm.throw_new("java/security/SignatureException", Some("not initialized")));
        }
        let array = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        let capacity = vm.array_length(array)? as i32;
        if offset < 0 || length < 0 || offset + length > capacity {
            return Err(vm.throw_new("java/lang/ArrayIndexOutOfBoundsException", None));
        }
        let mut bytes = Vec::with_capacity(length as usize);
        for index in 0..length {
            bytes.push(vm.array_get(array, offset + index)?.as_int() as u8);
        }
        crypto_append(vm, this, &bytes)?;
        void()
    }),
    method("sign", "()[B", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if crate::lang::read_int_field(vm, this, "state") != 2 {
            return Err(vm.throw_new(
                "java/security/SignatureException",
                Some("not initialized for signing"),
            ));
        }
        let hash = signature_hash(vm, this)?;
        crate::lang::write_int_field(vm, this, "count", 0);
        object(new_byte_array(vm, &hash)?)
    }),
    method("sign", "([BII)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if crate::lang::read_int_field(vm, this, "state") != 2 {
            return Err(vm.throw_new(
                "java/security/SignatureException",
                Some("not initialized for signing"),
            ));
        }
        let buf = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let hash = signature_hash(vm, this)?;
        if vm.array_length(buf)? as i32 - offset < hash.len() as i32 {
            return Err(vm.throw_new("java/security/SignatureException", Some("buffer too small")));
        }
        for (index, byte) in hash.iter().enumerate() {
            vm.array_set(
                buf,
                offset + index as i32,
                Value::Int(i32::from(*byte as i8)),
                false,
            )?;
        }
        crate::lang::write_int_field(vm, this, "count", 0);
        int(hash.len() as i32)
    }),
    method("verify", "([B)Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if crate::lang::read_int_field(vm, this, "state") != 3 {
            return Err(vm.throw_new(
                "java/security/SignatureException",
                Some("not initialized for verification"),
            ));
        }
        let expected = read_byte_array(vm, ref_arg(args, 1))?;
        let hash = signature_hash(vm, this)?;
        crate::lang::write_int_field(vm, this, "count", 0);
        boolean(expected == hash)
    }),
    method("verify", "([BII)Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if crate::lang::read_int_field(vm, this, "state") != 3 {
            return Err(vm.throw_new(
                "java/security/SignatureException",
                Some("not initialized for verification"),
            ));
        }
        let array = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        let capacity = vm.array_length(array)? as i32;
        if offset < 0 || length < 0 || offset + length > capacity {
            return Err(vm.throw_new("java/lang/ArrayIndexOutOfBoundsException", None));
        }
        let mut expected = Vec::with_capacity(length as usize);
        for index in 0..length {
            expected.push(vm.array_get(array, offset + index)?.as_int() as u8);
        }
        let hash = signature_hash(vm, this)?;
        crate::lang::write_int_field(vm, this, "count", 0);
        boolean(expected == hash)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = alloc::format!(
                "Signature[{}]",
                string_field(vm, receiver(args), "algorithm")
            );
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "clone",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |_vm, _, args| object(receiver(args)),
    ),
    method(
        "setParameter",
        "(Ljava/lang/String;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
];

pub(crate) const SIGNATURE: oxjvm_vm::NativeClass = class(
    "java/security/Signature",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("algorithm", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("provider", "Ljava/security/Provider;", ACC_PRIVATE, None),
        field("buf", "[B", ACC_PRIVATE, None),
        field("count", "I", ACC_PRIVATE, None),
        field("state", "I", ACC_PRIVATE, None),
    ],
    &SIGNATURE_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// javax.crypto.SecretKeyFactory
// -------------------------------------------------------------------------------------------

fn secret_key_factory_instance(vm: &mut Vm<'_>, algorithm: &str) -> Result<ObjectRef, VmError> {
    if algorithm.is_empty() {
        return Err(vm.throw_new(
            "java/security/NoSuchAlgorithmException",
            Some("algorithm must not be empty"),
        ));
    }
    let class = vm.resolve_class("javax/crypto/SecretKeyFactory")?;
    let factory = vm.new_instance(class)?;
    set_string_field(vm, factory, "algorithm", algorithm)?;
    let provider = new_sun_provider(vm, "SunJCE")?;
    set_ref_field(
        vm,
        factory,
        "provider",
        "Ljava/security/Provider;",
        provider,
    )?;
    Ok(factory)
}

const SECRET_KEY_FACTORY_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method(
        "getInstance",
        "(Ljava/lang/String;)Ljavax/crypto/SecretKeyFactory;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(secret_key_factory_instance(vm, &vm.string_or_empty(name))?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/lang/String;)Ljavax/crypto/SecretKeyFactory;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let provider = vm.string_or_empty(ref_arg(args, 1));
            if !known_provider(&provider) {
                return Err(vm.throw_new("java/security/NoSuchProviderException", Some(&provider)));
            }
            object(secret_key_factory_instance(
                vm,
                &vm.string_or_empty(ref_arg(args, 0)),
            )?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/security/Provider;)Ljavax/crypto/SecretKeyFactory;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(secret_key_factory_instance(
                vm,
                &vm.string_or_empty(ref_arg(args, 0)),
            )?)
        },
    ),
    method(
        "getAlgorithm",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| object(vm.make_string(&string_field(vm, receiver(args), "algorithm"))?),
    ),
    method(
        "getProvider",
        "()Ljava/security/Provider;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "provider",
                "Ljava/security/Provider;",
            ))
        },
    ),
    method(
        "generateSecret",
        "(Ljava/security/spec/KeySpec;)Ljavax/crypto/SecretKey;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let spec = ref_arg(args, 1);
            if spec.is_null() {
                return Err(vm.throw_new(
                    "java/security/spec/InvalidKeySpecException",
                    Some("Null spec"),
                ));
            }
            let algorithm = string_field(vm, this, "algorithm");
            let bytes = key_encoded(vm, spec)?;
            if bytes.is_empty() {
                return Err(vm.throw_new(
                    "java/security/spec/InvalidKeySpecException",
                    Some("Empty key"),
                ));
            }
            object(new_secret_key_spec(vm, &bytes, &algorithm)?)
        },
    ),
    method(
        "getKeySpec",
        "(Ljavax/crypto/SecretKey;Ljava/lang/Class;)Ljava/security/spec/KeySpec;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let key = ref_arg(args, 1);
            if key.is_null() {
                return Err(vm.throw_new(
                    "java/security/spec/InvalidKeySpecException",
                    Some("Null key"),
                ));
            }
            let algorithm = key_algorithm(vm, key)?;
            let bytes = key_encoded(vm, key)?;
            object(new_secret_key_spec(vm, &bytes, &algorithm)?)
        },
    ),
    method(
        "translateKey",
        "(Ljavax/crypto/SecretKey;)Ljavax/crypto/SecretKey;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let key = ref_arg(args, 1);
            if key.is_null() {
                return Err(vm.throw_new("java/security/InvalidKeyException", Some("Null key")));
            }
            let algorithm = key_algorithm(vm, key)?;
            let bytes = key_encoded(vm, key)?;
            object(new_secret_key_spec(vm, &bytes, &algorithm)?)
        },
    ),
];

pub(crate) const SECRET_KEY_FACTORY: oxjvm_vm::NativeClass = class(
    "javax/crypto/SecretKeyFactory",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("algorithm", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("provider", "Ljava/security/Provider;", ACC_PRIVATE, None),
    ],
    &SECRET_KEY_FACTORY_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// java.security.KeyFactory
// -------------------------------------------------------------------------------------------

fn key_factory_instance(vm: &mut Vm<'_>, algorithm: &str) -> Result<ObjectRef, VmError> {
    if algorithm.is_empty() {
        return Err(vm.throw_new(
            "java/security/NoSuchAlgorithmException",
            Some("algorithm must not be empty"),
        ));
    }
    let class = vm.resolve_class("java/security/KeyFactory")?;
    let factory = vm.new_instance(class)?;
    set_string_field(vm, factory, "algorithm", algorithm)?;
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

const KEY_FACTORY_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method(
        "getInstance",
        "(Ljava/lang/String;)Ljava/security/KeyFactory;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(key_factory_instance(vm, &vm.string_or_empty(name))?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/security/KeyFactory;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let provider = vm.string_or_empty(ref_arg(args, 1));
            if !known_provider(&provider) {
                return Err(vm.throw_new("java/security/NoSuchProviderException", Some(&provider)));
            }
            object(key_factory_instance(
                vm,
                &vm.string_or_empty(ref_arg(args, 0)),
            )?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/security/Provider;)Ljava/security/KeyFactory;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(key_factory_instance(
                vm,
                &vm.string_or_empty(ref_arg(args, 0)),
            )?)
        },
    ),
    method(
        "getAlgorithm",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(vm.make_string(&string_field(vm, receiver(args), "algorithm"))?),
    ),
    method(
        "getProvider",
        "()Ljava/security/Provider;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "provider",
                "Ljava/security/Provider;",
            ))
        },
    ),
    method(
        "generatePublic",
        "(Ljava/security/spec/KeySpec;)Ljava/security/PublicKey;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new(
                    "java/security/spec/InvalidKeySpecException",
                    Some("Null spec"),
                ));
            }
            let algorithm = string_field(vm, this, "algorithm");
            object(new_public_key(vm, &algorithm)?)
        },
    ),
    method(
        "generatePrivate",
        "(Ljava/security/spec/KeySpec;)Ljava/security/PrivateKey;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new(
                    "java/security/spec/InvalidKeySpecException",
                    Some("Null spec"),
                ));
            }
            let algorithm = string_field(vm, this, "algorithm");
            object(new_private_key(vm, &algorithm)?)
        },
    ),
    method(
        "getKeySpec",
        "(Ljava/security/Key;Ljava/lang/Class;)Ljava/security/spec/KeySpec;",
        ACC_PUBLIC,
        |vm, _, args| {
            let key = ref_arg(args, 1);
            if key.is_null() {
                return Err(vm.throw_new(
                    "java/security/spec/InvalidKeySpecException",
                    Some("Null key"),
                ));
            }
            let bytes = key_encoded(vm, key)?;
            let encoded = new_byte_array(vm, &bytes)?;
            let spec_class = vm.resolve_class("java/security/spec/X509EncodedKeySpec")?;
            let spec = vm.new_instance(spec_class)?;
            set_ref_field(vm, spec, "encoded", "[B", encoded)?;
            object(spec)
        },
    ),
];

pub(crate) const KEY_FACTORY: oxjvm_vm::NativeClass = class(
    "java/security/KeyFactory",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("algorithm", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("provider", "Ljava/security/Provider;", ACC_PRIVATE, None),
    ],
    &KEY_FACTORY_METHODS,
    None,
);
