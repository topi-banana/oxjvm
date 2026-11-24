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

fn opt_string_field(vm: &Vm<'_>, this: ObjectRef, name: &str) -> Option<String> {
    let reference = vm.read_ref_field(this, vm.class_of(this), name, "Ljava/lang/String;")?;
    if reference.is_null() {
        None
    } else {
        vm.string_value(reference)
    }
}

fn list_set(
    vm: &mut Vm<'_>,
    list: ObjectRef,
    index: usize,
    value: ObjectRef,
) -> Result<(), VmError> {
    let (class, method) =
        vm.resolve_virtual_method(list, "set", "(ILjava/lang/Object;)Ljava/lang/Object;")?;
    vm.invoke_method(
        class,
        method,
        vec![
            Value::Ref(list),
            Value::Int(index as i32),
            Value::Ref(value),
        ],
    )?;
    Ok(())
}

/// PermissionCollection.add implementations call this after the type check:
/// a collection marked readonly rejects further additions.
pub(crate) fn ensure_collection_writable(
    vm: &mut Vm<'_>,
    collection: ObjectRef,
) -> Result<(), VmError> {
    if crate::lang::read_int_field(vm, collection, "readOnly") != 0 {
        return Err(vm.throw_new(
            "java/lang/SecurityException",
            Some("attempt to add a Permission to a readonly PermissionCollection"),
        ));
    }
    Ok(())
}

/// Render a permission for an exception message the way string concatenation
/// would: null becomes "null", anything else uses its virtual toString().
fn permission_display(vm: &mut Vm<'_>, permission: ObjectRef) -> Result<String, VmError> {
    if permission.is_null() {
        return Ok(String::from("null"));
    }
    let (declaring, to_string) =
        vm.resolve_virtual_method(permission, "toString", "()Ljava/lang/String;")?;
    let value = vm.invoke_method(declaring, to_string, vec![Value::Ref(permission)])?;
    Ok(match value {
        Value::Ref(reference) if !reference.is_null() => {
            vm.string_value(reference).unwrap_or_default()
        }
        _ => String::from("null"),
    })
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
    // Installed providers are singletons in the JDK: cache them by (case-folded)
    // name so Security.getProvider(name) and getInstance(...).getProvider() hand
    // back the very same object.
    let security = vm.resolve_class("java/security/Security")?;
    vm.ensure_initialized(security)?;
    let mut cache = vm
        .read_static_ref(security, "providerCache", "Ljava/util/HashMap;")
        .unwrap_or(ObjectRef::NULL);
    if cache.is_null() {
        cache = crate::util::new_hash_map(vm)?;
        vm.set_static_value(
            security,
            "providerCache",
            "Ljava/util/HashMap;",
            Value::Ref(cache),
        );
    }
    let key = vm.make_string(&name.to_lowercase())?;
    let existing = crate::util::hash_map_lookup(vm, cache, key)?;
    if !existing.is_null() {
        return Ok(existing);
    }
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
    crate::util::hash_map_insert(vm, cache, key, provider)?;
    Ok(provider)
}

const SECURITY_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method(
        "getProperty",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let props = security_properties(vm)?;
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
            let name = ref_arg(args, 0);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let props = security_properties(vm)?;
            property_call(
                vm,
                props,
                "getProperty",
                "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                vec![
                    Value::Ref(props),
                    Value::Ref(name),
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
            let name = ref_arg(args, 0);
            let value = ref_arg(args, 1);
            if name.is_null() || value.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
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
        |vm, _, args| {
            if ref_arg(args, 0).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
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
        |vm, _, args| {
            if ref_arg(args, 0).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
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
        |vm, _, args| {
            if ref_arg(args, 0).is_null() {
                return void();
            }
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
        field(
            "providerCache",
            "Ljava/util/HashMap;",
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
        let mut text = alloc::format!("{version}");
        if !text.contains('.') && !text.contains('E') {
            text.push_str(".0");
        }
        set_string_field(vm, this, "versionStr", &text)?;
    } else {
        let version = ref_arg(args, 2);
        set_ref_field(vm, this, "versionStr", "Ljava/lang/String;", version)?;
    }
    provider_put_id(vm, this)?;
    let services = crate::util::linked_hash_map_new(vm)?;
    set_ref_field(vm, this, "services", "Ljava/util/LinkedHashMap;", services)?;
    crate::lang::write_int_field(vm, this, "dirty", 1);
    void()
}

/// Register the immutable `Provider.id` attributes in the property table.
fn provider_put_id(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    let table = vm.resolve_class("java/util/Hashtable")?;
    let (declaring, put) = vm
        .find_method(
            table,
            "put",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        )
        .ok_or_else(|| VmError::internal("Hashtable.put missing"))?;
    let class_name = vm.class_name(vm.class_of(this)).replace('/', ".");
    let entries = [
        ("Provider.id name", string_field(vm, this, "name")),
        ("Provider.id version", string_field(vm, this, "versionStr")),
        ("Provider.id info", string_field(vm, this, "info")),
        ("Provider.id className", class_name),
    ];
    for (key, value) in entries {
        let key = vm.make_string(key)?;
        let value = vm.make_string(&value)?;
        vm.invoke_method(
            declaring,
            put,
            vec![Value::Ref(this), Value::Ref(key), Value::Ref(value)],
        )?;
    }
    Ok(())
}

/// `Provider` ignores put/remove updates of keys starting with `Provider.`.
fn provider_key_blocked(vm: &Vm<'_>, key: ObjectRef) -> bool {
    !key.is_null()
        && vm.class_name(vm.class_of(key)) == "java/lang/String"
        && vm.string_or_empty(key).starts_with("Provider.")
}

const PROVIDER_METHODS: [oxjvm_vm::NativeMethodDef; 13] = [
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
        |vm, _, args| {
            let this = receiver(args);
            let service_type = vm.string_or_empty(ref_arg(args, 1));
            let algorithm = vm.string_or_empty(ref_arg(args, 2));
            let services = provider_ensure_parsed(vm, this)?;
            let map_key = vm.make_string(&provider_service_key(
                &provider_engine_name(&service_type),
                &algorithm,
            ))?;
            let service = crate::util::hash_map_lookup(vm, services, map_key)?;
            if service.is_null() || opt_string_field(vm, service, "className").is_none() {
                return object(ObjectRef::NULL);
            }
            object(service)
        },
    ),
    method(
        "getServices",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let services = provider_ensure_parsed(vm, this)?;
            let set = new_hash_set(vm)?;
            let pairs = crate::util::hash_map_pairs(vm, services);
            let mut seen: Vec<ObjectRef> = Vec::new();
            for (_, service) in pairs {
                if service.is_null()
                    || opt_string_field(vm, service, "className").is_none()
                    || seen.contains(&service)
                {
                    continue;
                }
                seen.push(service);
                let (class, add) =
                    vm.resolve_virtual_method(set, "add", "(Ljava/lang/Object;)Z")?;
                vm.invoke_method(class, add, vec![Value::Ref(set), Value::Ref(service)])?;
            }
            object(set)
        },
    ),
    method("isConfigured", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
    method(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let key = ref_arg(args, 1);
            if provider_key_blocked(vm, key) {
                return object(ObjectRef::NULL);
            }
            let table = vm.resolve_class("java/util/Hashtable")?;
            let (declaring, put) = vm
                .find_method(
                    table,
                    "put",
                    "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
                )
                .ok_or_else(|| VmError::internal("Hashtable.put missing"))?;
            let result = vm.invoke_method(
                declaring,
                put,
                vec![
                    Value::Ref(this),
                    Value::Ref(key),
                    Value::Ref(ref_arg(args, 2)),
                ],
            )?;
            crate::lang::write_int_field(vm, this, "dirty", 1);
            Ok(result)
        },
    ),
    method(
        "remove",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let key = ref_arg(args, 1);
            if provider_key_blocked(vm, key) {
                return object(ObjectRef::NULL);
            }
            let table = vm.resolve_class("java/util/Hashtable")?;
            let (declaring, remove) = vm
                .find_method(table, "remove", "(Ljava/lang/Object;)Ljava/lang/Object;")
                .ok_or_else(|| VmError::internal("Hashtable.remove missing"))?;
            let result =
                vm.invoke_method(declaring, remove, vec![Value::Ref(this), Value::Ref(key)])?;
            crate::lang::write_int_field(vm, this, "dirty", 1);
            Ok(result)
        },
    ),
    method("clear", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let table = vm.resolve_class("java/util/Hashtable")?;
        let (declaring, clear) = vm
            .find_method(table, "clear", "()V")
            .ok_or_else(|| VmError::internal("Hashtable.clear missing"))?;
        vm.invoke_method(declaring, clear, vec![Value::Ref(this)])?;
        provider_put_id(vm, this)?;
        crate::lang::write_int_field(vm, this, "dirty", 1);
        void()
    }),
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
        field("services", "Ljava/util/LinkedHashMap;", ACC_PRIVATE, None),
        field("dirty", "I", ACC_PRIVATE, None),
    ],
    &PROVIDER_METHODS,
    None,
);

/// Canonical engine name for a (possibly mis-cased) service type, matching
/// the JDK's `getEngineName`; unknown types pass through unchanged.
fn provider_engine_name(name: &str) -> String {
    const ENGINES: &[&str] = &[
        "AlgorithmParameterGenerator",
        "AlgorithmParameters",
        "KeyFactory",
        "KeyPairGenerator",
        "KeyStore",
        "MessageDigest",
        "SecureRandom",
        "Signature",
        "CertificateFactory",
        "CertPathBuilder",
        "CertPathValidator",
        "CertStore",
        "Cipher",
        "ExemptionMechanism",
        "Mac",
        "KeyAgreement",
        "KeyGenerator",
        "SecretKeyFactory",
        "KEM",
        "KDF",
        "KeyManagerFactory",
        "SSLContext",
        "TrustManagerFactory",
        "GssApiMechanism",
        "SaslClientFactory",
        "SaslServerFactory",
        "Policy",
        "Configuration",
    ];
    let lower = name.to_lowercase();
    for engine in ENGINES {
        if engine.to_lowercase() == lower {
            return String::from(*engine);
        }
    }
    String::from(name)
}

/// The upper-cased `TYPE.ALGORITHM` key used by the service map. The JDK
/// upper-cases with `Locale.ENGLISH`, so the folding must stay locale-free.
fn provider_service_key(service_type: &str, algorithm: &str) -> String {
    alloc::format!("{}.{}", service_type, algorithm.to_uppercase())
}

fn svc_new(
    vm: &mut Vm<'_>,
    provider: ObjectRef,
    service_type: &str,
    algorithm: &str,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/security/Provider$Service")?;
    let service = vm.new_instance(class)?;
    set_ref_field(
        vm,
        service,
        "provider",
        "Ljava/security/Provider;",
        provider,
    )?;
    set_string_field(vm, service, "type", service_type)?;
    set_string_field(vm, service, "algorithm", algorithm)?;
    let string_class = vm.resolve_class("java/lang/String")?;
    let empty = vm.allocate_object_array(string_class, 0)?;
    set_ref_field(vm, service, "aliases", "[Ljava/lang/String;", empty)?;
    let string_array_class = vm.resolve_class("[Ljava/lang/String;")?;
    let empty_pairs = vm.allocate_object_array(string_array_class, 0)?;
    set_ref_field(
        vm,
        service,
        "attributes",
        "[[Ljava/lang/String;",
        empty_pairs,
    )?;
    Ok(service)
}

fn svc_aliases(vm: &mut Vm<'_>, service: ObjectRef) -> Result<Vec<String>, VmError> {
    let array = ref_field(vm, service, "aliases", "[Ljava/lang/String;");
    let mut aliases = Vec::new();
    if array.is_null() {
        return Ok(aliases);
    }
    let length = vm.array_length(array)?;
    for index in 0..length {
        let item = vm.array_get(array, index as i32)?.as_ref();
        aliases.push(vm.string_value(item).unwrap_or_default());
    }
    Ok(aliases)
}

fn svc_store_aliases(
    vm: &mut Vm<'_>,
    service: ObjectRef,
    aliases: &[String],
) -> Result<(), VmError> {
    let string_class = vm.resolve_class("java/lang/String")?;
    let array = vm.allocate_object_array(string_class, aliases.len())?;
    for (index, alias) in aliases.iter().enumerate() {
        let text = vm.make_string(alias)?;
        vm.array_set_ref(array, index, text)?;
    }
    set_ref_field(vm, service, "aliases", "[Ljava/lang/String;", array)?;
    Ok(())
}

fn svc_add_alias(vm: &mut Vm<'_>, service: ObjectRef, alias: &str) -> Result<(), VmError> {
    let mut aliases = svc_aliases(vm, service)?;
    aliases.push(String::from(alias));
    svc_store_aliases(vm, service, &aliases)
}

fn svc_remove_alias(vm: &mut Vm<'_>, service: ObjectRef, alias: &str) -> Result<(), VmError> {
    let mut aliases = svc_aliases(vm, service)?;
    if let Some(position) = aliases.iter().position(|item| item == alias) {
        aliases.remove(position);
        svc_store_aliases(vm, service, &aliases)?;
    }
    Ok(())
}

fn svc_attributes(vm: &mut Vm<'_>, service: ObjectRef) -> Result<Vec<(String, String)>, VmError> {
    let array = ref_field(vm, service, "attributes", "[[Ljava/lang/String;");
    let mut attributes = Vec::new();
    if array.is_null() {
        return Ok(attributes);
    }
    let length = vm.array_length(array)?;
    for index in 0..length {
        let pair = vm.array_get(array, index as i32)?.as_ref();
        let name_ref = vm.array_get(pair, 0)?.as_ref();
        let value_ref = vm.array_get(pair, 1)?.as_ref();
        let name = vm.string_value(name_ref).unwrap_or_default();
        let value = vm.string_value(value_ref).unwrap_or_default();
        attributes.push((name, value));
    }
    Ok(attributes)
}

fn svc_add_attribute(
    vm: &mut Vm<'_>,
    service: ObjectRef,
    name: &str,
    value: &str,
) -> Result<(), VmError> {
    let mut attributes = svc_attributes(vm, service)?;
    // Attribute names are case-insensitive (the JDK's UString map).
    let lower = name.to_lowercase();
    if let Some(entry) = attributes
        .iter_mut()
        .find(|(existing, _)| existing.to_lowercase() == lower)
    {
        entry.1 = String::from(value);
    } else {
        attributes.push((String::from(name), String::from(value)));
    }
    let string_class = vm.resolve_class("java/lang/String")?;
    let string_array_class = vm.resolve_class("[Ljava/lang/String;")?;
    let array = vm.allocate_object_array(string_array_class, attributes.len())?;
    for (index, (attr_name, attr_value)) in attributes.iter().enumerate() {
        let pair = vm.allocate_object_array(string_class, 2)?;
        let name_ref = vm.make_string(attr_name)?;
        let value_ref = vm.make_string(attr_value)?;
        vm.array_set_ref(pair, 0, name_ref)?;
        vm.array_set_ref(pair, 1, value_ref)?;
        vm.array_set_ref(array, index, pair)?;
    }
    set_ref_field(vm, service, "attributes", "[[Ljava/lang/String;", array)?;
    Ok(())
}

/// Re-parse the property table into the service map when it changed, walking
/// the entries in table order exactly like the JDK's incremental parseLegacy.
fn provider_ensure_parsed(vm: &mut Vm<'_>, this: ObjectRef) -> Result<ObjectRef, VmError> {
    let services = ref_field(vm, this, "services", "Ljava/util/LinkedHashMap;");
    if !services.is_null() && crate::lang::read_int_field(vm, this, "dirty") == 0 {
        return Ok(services);
    }
    let services = if services.is_null() {
        crate::util::linked_hash_map_new(vm)?
    } else {
        let (class, clear) = vm.resolve_virtual_method(services, "clear", "()V")?;
        vm.invoke_method(class, clear, vec![Value::Ref(services)])?;
        services
    };
    set_ref_field(vm, this, "services", "Ljava/util/LinkedHashMap;", services)?;
    let table = vm
        .read_ref_field(this, vm.class_of(this), "map", "Ljava/util/HashMap;")
        .unwrap_or(ObjectRef::NULL);
    let pairs = if table.is_null() {
        Vec::new()
    } else {
        crate::util::hash_map_pairs(vm, table)
    };
    for (key_ref, value_ref) in pairs {
        if key_ref.is_null()
            || value_ref.is_null()
            || vm.class_name(vm.class_of(key_ref)) != "java/lang/String"
            || vm.class_name(vm.class_of(value_ref)) != "java/lang/String"
        {
            continue;
        }
        let key = vm.string_or_empty(key_ref);
        let value = vm.string_or_empty(value_ref);
        if key.starts_with("Provider.") {
            continue;
        }
        let get_or_create =
            |vm: &mut Vm<'_>, service_type: &str, algorithm: &str| -> Result<ObjectRef, VmError> {
                let map_key = vm.make_string(&provider_service_key(service_type, algorithm))?;
                let existing = crate::util::hash_map_lookup(vm, services, map_key)?;
                if !existing.is_null() {
                    return Ok(existing);
                }
                let service = svc_new(vm, this, service_type, algorithm)?;
                let map_key = vm.make_string(&provider_service_key(service_type, algorithm))?;
                crate::util::hash_map_insert(vm, services, map_key, service)?;
                Ok(service)
            };
        if key.to_lowercase().starts_with("alg.alias.") {
            let rest = &key[10..];
            let Some(dot) = rest.find('.') else { continue };
            if dot < 1 {
                continue;
            }
            let service_type = provider_engine_name(&rest[..dot]);
            let alias_alg = &rest[dot + 1..];
            let std_service = get_or_create(vm, &service_type, &value)?;
            let alias_key = vm.make_string(&provider_service_key(&service_type, alias_alg))?;
            let previous = crate::util::hash_map_lookup(vm, services, alias_key)?;
            if !previous.is_null() && previous != std_service {
                svc_remove_alias(vm, previous, alias_alg)?;
            }
            svc_add_alias(vm, std_service, alias_alg)?;
            let alias_key = vm.make_string(&provider_service_key(&service_type, alias_alg))?;
            crate::util::hash_map_insert(vm, services, alias_key, std_service)?;
        } else {
            let Some(dot) = key.find('.') else { continue };
            if dot < 1 {
                continue;
            }
            let service_type = provider_engine_name(&key[..dot]);
            let rest = &key[dot + 1..];
            if let Some(space) = rest.find(' ') {
                let algorithm = &rest[..space];
                let attr_name = rest[space + 1..].trim_start_matches(' ');
                let service = get_or_create(vm, &service_type, algorithm)?;
                svc_add_attribute(vm, service, attr_name, &value)?;
            } else {
                let service = get_or_create(vm, &service_type, rest)?;
                set_string_field(vm, service, "className", &value)?;
            }
        }
    }
    crate::lang::write_int_field(vm, this, "dirty", 0);
    Ok(services)
}

const PROVIDER_SERVICE_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method(
        "getType",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "type", "Ljava/lang/String;")),
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
        "getClassName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "className",
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
        "getAttribute",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name_ref = ref_arg(args, 1);
            if name_ref.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let name = vm.string_or_empty(name_ref).to_lowercase();
            let attributes = svc_attributes(vm, receiver(args))?;
            for (attr_name, value) in attributes {
                if attr_name.to_lowercase() == name {
                    return object(vm.make_string(&value)?);
                }
            }
            object(ObjectRef::NULL)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let provider = ref_field(vm, this, "provider", "Ljava/security/Provider;");
            let provider_name = string_field(vm, provider, "name");
            let service_type = string_field(vm, this, "type");
            let algorithm = string_field(vm, this, "algorithm");
            let class_name =
                opt_string_field(vm, this, "className").unwrap_or_else(|| String::from("null"));
            let mut text =
                alloc::format!("{provider_name}: {service_type}.{algorithm} -> {class_name}");
            let aliases = svc_aliases(vm, this)?;
            if !aliases.is_empty() {
                text.push_str(&alloc::format!("\r\n  aliases: [{}]", aliases.join(", ")));
            }
            let attributes = svc_attributes(vm, this)?;
            if !attributes.is_empty() {
                let rendered: Vec<String> = attributes
                    .iter()
                    .map(|(name, value)| alloc::format!("{name}={value}"))
                    .collect();
                text.push_str(&alloc::format!(
                    "\r\n  attributes: {{{}}}",
                    rendered.join(", ")
                ));
            }
            text.push_str("\r\n");
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "newInstance",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class_name = string_field(vm, receiver(args), "className");
            Err(vm.throw_new(
                "java/security/NoSuchAlgorithmException",
                Some(&alloc::format!(
                    "Error constructing implementation (class: {class_name})"
                )),
            ))
        },
    ),
    method(
        "supportsParameter",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(true),
    ),
];

pub(crate) const PROVIDER_SERVICE: oxjvm_vm::NativeClass = class(
    "java/security/Provider$Service",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("provider", "Ljava/security/Provider;", ACC_PRIVATE, None),
        field("type", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("algorithm", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("className", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("aliases", "[Ljava/lang/String;", ACC_PRIVATE, None),
        field("attributes", "[[Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &PROVIDER_SERVICE_METHODS,
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

// -------------------------------------------------------------------------------------------
// java.security.Principal / Certificate (deprecated) / Identity / IdentityScope
// -------------------------------------------------------------------------------------------

pub(crate) const PRINCIPAL: oxjvm_vm::NativeClass = class(
    "java/security/Principal",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &PRINCIPAL_METHODS,
    None,
);

const PRINCIPAL_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "getName",
    "()Ljava/lang/String;",
    ACC_PUBLIC | ACC_ABSTRACT,
    crate::lang::abstract_method_pub,
)];

pub(crate) const DOMAIN_COMBINER: oxjvm_vm::NativeClass = class(
    "java/security/DomainCombiner",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &DOMAIN_COMBINER_METHODS,
    None,
);

const DOMAIN_COMBINER_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "combine",
    "([Ljava/security/ProtectionDomain;[Ljava/security/ProtectionDomain;)[Ljava/security/ProtectionDomain;",
    ACC_PUBLIC | ACC_ABSTRACT,
    crate::lang::abstract_method_pub,
)];

pub(crate) const SECURITY_CERTIFICATE: oxjvm_vm::NativeClass = class(
    "java/security/Certificate",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &SECURITY_CERTIFICATE_METHODS,
    None,
);

const SECURITY_CERTIFICATE_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "getGuarantor",
        "()Ljava/security/Principal;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getPrincipal",
        "()Ljava/security/Principal;",
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
        "encode",
        "(Ljava/io/OutputStream;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "decode",
        "(Ljava/io/InputStream;)V",
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
        "toString",
        "(Z)Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

/// Identity.keyEquals: formats must match ignoring case (or both be null)
/// and the encoded key bytes must be identical.
fn identity_key_equals(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<bool, VmError> {
    if a.is_null() || b.is_null() {
        return Ok(false);
    }
    let (declaring, get_format) =
        vm.resolve_virtual_method(a, "getFormat", "()Ljava/lang/String;")?;
    let format_a = vm.invoke_method(declaring, get_format, vec![Value::Ref(a)])?;
    let format_b = vm.invoke_method(declaring, get_format, vec![Value::Ref(b)])?;
    let text_a = format_a.as_ref();
    let text_b = format_b.as_ref();
    if text_a.is_null() != text_b.is_null() {
        return Ok(false);
    }
    if !text_a.is_null() {
        let sa = vm.string_value(text_a).unwrap_or_default();
        let sb = vm.string_value(text_b).unwrap_or_default();
        if !sa.eq_ignore_ascii_case(&sb) {
            return Ok(false);
        }
    }
    let (declaring, get_encoded) = vm.resolve_virtual_method(a, "getEncoded", "()[B")?;
    let encoded_a = vm.invoke_method(declaring, get_encoded, vec![Value::Ref(a)])?;
    let encoded_b = vm.invoke_method(declaring, get_encoded, vec![Value::Ref(b)])?;
    let bytes_a = encoded_a.as_ref();
    let bytes_b = encoded_b.as_ref();
    if bytes_a.is_null() || bytes_b.is_null() {
        return Ok(bytes_a.is_null() && bytes_b.is_null());
    }
    let len_a = vm.array_length(bytes_a)?;
    let len_b = vm.array_length(bytes_b)?;
    if len_a != len_b {
        return Ok(false);
    }
    for index in 0..len_a {
        if vm.array_get(bytes_a, index as i32)?.as_int()
            != vm.array_get(bytes_b, index as i32)?.as_int()
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn identity_full_name(vm: &Vm<'_>, this: ObjectRef) -> String {
    let name = string_field(vm, this, "name");
    let scope = ref_field(vm, this, "scope", "Ljava/security/IdentityScope;");
    if scope.is_null() {
        name
    } else {
        alloc::format!("{name}.{}", string_field(vm, scope, "name"))
    }
}

pub(crate) const IDENTITY: oxjvm_vm::NativeClass = class(
    "java/security/Identity",
    Some("java/lang/Object"),
    &["java/security/Principal", "java/io/Serializable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("scope", "Ljava/security/IdentityScope;", ACC_PRIVATE, None),
        field("publicKey", "Ljava/security/PublicKey;", ACC_PRIVATE, None),
        field("info", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("certs", "Ljava/util/ArrayList;", ACC_PRIVATE, None),
    ],
    &IDENTITY_METHODS,
    None,
);

const IDENTITY_METHODS: [oxjvm_vm::NativeMethodDef; 17] = [
    method("<init>", "()V", ACC_PROTECTED, |vm, _, args| {
        let class = vm.resolve_class("java/security/Identity")?;
        let (declaring, ctor) = vm
            .find_method(class, "<init>", "(Ljava/lang/String;)V")
            .ok_or_else(|| VmError::internal("Identity lacks <init>"))?;
        let name = vm.make_string("restoring...")?;
        vm.invoke_method(declaring, ctor, vec![args[0], Value::Ref(name)])?;
        void()
    }),
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            set_ref_field(
                vm,
                receiver(args),
                "name",
                "Ljava/lang/String;",
                ref_arg(args, 1),
            )?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/security/IdentityScope;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.resolve_class("java/security/Identity")?;
            let (declaring, ctor) = vm
                .find_method(class, "<init>", "(Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("Identity lacks <init>"))?;
            vm.invoke_method(declaring, ctor, vec![args[0], args[1]])?;
            let scope = ref_arg(args, 2);
            if !scope.is_null() {
                let (declaring, add) =
                    vm.resolve_virtual_method(scope, "addIdentity", "(Ljava/security/Identity;)V")?;
                vm.invoke_method(declaring, add, vec![Value::Ref(scope), Value::Ref(this)])?;
            }
            set_ref_field(vm, this, "scope", "Ljava/security/IdentityScope;", scope)?;
            void()
        },
    ),
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| object(ref_field(vm, receiver(args), "name", "Ljava/lang/String;")),
    ),
    method(
        "getScope",
        "()Ljava/security/IdentityScope;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "scope",
                "Ljava/security/IdentityScope;",
            ))
        },
    ),
    method(
        "getPublicKey",
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
        "setPublicKey",
        "(Ljava/security/PublicKey;)V",
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
            let list = new_array_list(vm)?;
            set_ref_field(vm, this, "certs", "Ljava/util/ArrayList;", list)?;
            void()
        },
    ),
    method(
        "setInfo",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            set_ref_field(
                vm,
                receiver(args),
                "info",
                "Ljava/lang/String;",
                ref_arg(args, 1),
            )?;
            void()
        },
    ),
    method(
        "getInfo",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "info", "Ljava/lang/String;")),
    ),
    method(
        "addCertificate",
        "(Ljava/security/Certificate;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let certificate = ref_arg(args, 1);
            let mut list = ref_field(vm, this, "certs", "Ljava/util/ArrayList;");
            if list.is_null() {
                list = new_array_list(vm)?;
                set_ref_field(vm, this, "certs", "Ljava/util/ArrayList;", list)?;
            }
            let (declaring, get_public_key) = vm.resolve_virtual_method(
                certificate,
                "getPublicKey",
                "()Ljava/security/PublicKey;",
            )?;
            let cert_key =
                vm.invoke_method(declaring, get_public_key, vec![Value::Ref(certificate)])?;
            let public_key = ref_field(vm, this, "publicKey", "Ljava/security/PublicKey;");
            if !public_key.is_null() {
                if !identity_key_equals(vm, public_key, cert_key.as_ref())? {
                    return Err(vm.throw_new(
                        "java/security/KeyManagementException",
                        Some("public key different from cert public key"),
                    ));
                }
            } else {
                set_ref_field(
                    vm,
                    this,
                    "publicKey",
                    "Ljava/security/PublicKey;",
                    cert_key.as_ref(),
                )?;
            }
            list_add(vm, list, certificate)?;
            void()
        },
    ),
    method(
        "removeCertificate",
        "(Ljava/security/Certificate;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let list = ref_field(vm, receiver(args), "certs", "Ljava/util/ArrayList;");
            if !list.is_null() {
                let (declaring, remove) =
                    vm.resolve_virtual_method(list, "remove", "(Ljava/lang/Object;)Z")?;
                vm.invoke_method(declaring, remove, vec![Value::Ref(list), args[1]])?;
            }
            void()
        },
    ),
    method(
        "certificates",
        "()[Ljava/security/Certificate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let list = ref_field(vm, receiver(args), "certs", "Ljava/util/ArrayList;");
            let elements = if list.is_null() {
                Vec::new()
            } else {
                crate::util::array_list_elements(vm, list)
            };
            let cert_class = vm.resolve_class("java/security/Certificate")?;
            let array = vm.allocate_object_array(cert_class, elements.len())?;
            for (index, element) in elements.iter().enumerate() {
                vm.array_set_ref(array, index, *element)?;
            }
            object(array)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            if this == other {
                return boolean(true);
            }
            let identity = vm.resolve_class("java/security/Identity")?;
            if other.is_null() || !vm.is_instance(other, identity) {
                return boolean(false);
            }
            if identity_full_name(vm, this) == identity_full_name(vm, other) {
                return boolean(true);
            }
            let (declaring, identity_equals) =
                vm.resolve_virtual_method(this, "identityEquals", "(Ljava/security/Identity;)Z")?;
            let value = vm.invoke_method(
                declaring,
                identity_equals,
                vec![Value::Ref(this), Value::Ref(other)],
            )?;
            boolean(value.as_int() != 0)
        },
    ),
    method(
        "identityEquals",
        "(Ljava/security/Identity;)Z",
        ACC_PROTECTED,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let this_name = string_field(vm, this, "name");
            let other_name = string_field(vm, other, "name");
            if !this_name.eq_ignore_ascii_case(&other_name) {
                return boolean(false);
            }
            let this_key = ref_field(vm, this, "publicKey", "Ljava/security/PublicKey;");
            let other_key = ref_field(vm, other, "publicKey", "Ljava/security/PublicKey;");
            if this_key.is_null() != other_key.is_null() {
                return boolean(false);
            }
            if !this_key.is_null() {
                return boolean(ref_equals(vm, this_key, other_key)?);
            }
            boolean(true)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let name = string_field(vm, receiver(args), "name");
        let units: Vec<u16> = name.encode_utf16().collect();
        int(crate::lang::java_string_hash(&units))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name = string_field(vm, this, "name");
            let scope = ref_field(vm, this, "scope", "Ljava/security/IdentityScope;");
            let text = if scope.is_null() {
                name
            } else {
                alloc::format!("{name}[{}]", string_field(vm, scope, "name"))
            };
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toString",
        "(Z)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, to_string) =
                vm.resolve_virtual_method(this, "toString", "()Ljava/lang/String;")?;
            let value = vm.invoke_method(declaring, to_string, vec![Value::Ref(this)])?;
            let mut out = vm.string_value(value.as_ref()).unwrap_or_default();
            if args[1].as_int() != 0 {
                let public_key = ref_field(vm, this, "publicKey", "Ljava/security/PublicKey;");
                out += "\n";
                out += if public_key.is_null() {
                    "\tno public key"
                } else {
                    "\tpublic key initialized"
                };
                out += "\n";
                let list = ref_field(vm, this, "certs", "Ljava/util/ArrayList;");
                if list.is_null() {
                    out += "\tno certificates";
                } else {
                    out += "\tcertificates: \n";
                    for (index, cert) in crate::util::array_list_elements(vm, list)
                        .iter()
                        .enumerate()
                    {
                        let (declaring, get_principal) = vm.resolve_virtual_method(
                            *cert,
                            "getPrincipal",
                            "()Ljava/security/Principal;",
                        )?;
                        let principal =
                            vm.invoke_method(declaring, get_principal, vec![Value::Ref(*cert)])?;
                        let (declaring, get_guarantor) = vm.resolve_virtual_method(
                            *cert,
                            "getGuarantor",
                            "()Ljava/security/Principal;",
                        )?;
                        let guarantor =
                            vm.invoke_method(declaring, get_guarantor, vec![Value::Ref(*cert)])?;
                        out += &alloc::format!(
                            "\tcertificate {}\tfor  : {}\n\t\t\tfrom : {}\n",
                            index + 1,
                            permission_display(vm, principal.as_ref())?,
                            permission_display(vm, guarantor.as_ref())?
                        );
                    }
                }
                match opt_string_field(vm, this, "info") {
                    Some(info) => out += &alloc::format!("\n\t{info}"),
                    None => out += "\n\tno additional information available.",
                }
            }
            object(vm.make_string(&out)?)
        },
    ),
];

pub(crate) const IDENTITY_SCOPE: oxjvm_vm::NativeClass = class(
    "java/security/IdentityScope",
    Some("java/security/Identity"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &IDENTITY_SCOPE_METHODS,
    None,
);

const IDENTITY_SCOPE_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method("<init>", "()V", ACC_PROTECTED, |vm, _, args| {
        let class = vm.resolve_class("java/security/IdentityScope")?;
        let (declaring, ctor) = vm
            .find_method(class, "<init>", "(Ljava/lang/String;)V")
            .ok_or_else(|| VmError::internal("IdentityScope lacks <init>"))?;
        let name = vm.make_string("restoring...")?;
        vm.invoke_method(declaring, ctor, vec![args[0], Value::Ref(name)])?;
        void()
    }),
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let parent = vm.resolve_class("java/security/Identity")?;
            let (declaring, ctor) = vm
                .find_method(parent, "<init>", "(Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("Identity lacks <init>"))?;
            vm.invoke_method(declaring, ctor, vec![args[0], args[1]])?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/security/IdentityScope;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let parent = vm.resolve_class("java/security/Identity")?;
            let (declaring, ctor) = vm
                .find_method(
                    parent,
                    "<init>",
                    "(Ljava/lang/String;Ljava/security/IdentityScope;)V",
                )
                .ok_or_else(|| VmError::internal("Identity lacks <init>"))?;
            vm.invoke_method(declaring, ctor, args.to_vec())?;
            void()
        },
    ),
    method(
        "getSystemScope",
        "()Ljava/security/IdentityScope;",
        ACC_PUBLIC | ACC_STATIC,
        // No system.scope security property is set, so the JDK leaves the
        // system scope null.
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "size",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getIdentity",
        "(Ljava/lang/String;)Ljava/security/Identity;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getIdentity",
        "(Ljava/security/PublicKey;)Ljava/security/Identity;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getIdentity",
        "(Ljava/security/Principal;)Ljava/security/Identity;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let principal = ref_arg(args, 1);
            let (declaring, get_name) =
                vm.resolve_virtual_method(principal, "getName", "()Ljava/lang/String;")?;
            let name = vm.invoke_method(declaring, get_name, vec![Value::Ref(principal)])?;
            let (declaring, get_identity) = vm.resolve_virtual_method(
                this,
                "getIdentity",
                "(Ljava/lang/String;)Ljava/security/Identity;",
            )?;
            let value = vm.invoke_method(declaring, get_identity, vec![Value::Ref(this), name])?;
            object(value.as_ref())
        },
    ),
    method(
        "addIdentity",
        "(Ljava/security/Identity;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "removeIdentity",
        "(Ljava/security/Identity;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "identities",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let identity = vm.resolve_class("java/security/Identity")?;
            let (declaring, to_string) = vm
                .find_method(identity, "toString", "()Ljava/lang/String;")
                .ok_or_else(|| VmError::internal("Identity lacks toString"))?;
            let value = vm.invoke_method(declaring, to_string, vec![Value::Ref(this)])?;
            let base = vm.string_value(value.as_ref()).unwrap_or_default();
            let (declaring, size) = vm.resolve_virtual_method(this, "size", "()I")?;
            let count = vm.invoke_method(declaring, size, vec![Value::Ref(this)])?;
            object(vm.make_string(&alloc::format!("{base}[{}]", count.as_int()))?)
        },
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

/// Clone an object array preserving its component type, then (per the JDK's
/// PrivateKeyEntry) re-type it to X509Certificate[] when the end-entity cert is
/// an X509Certificate but the incoming array is not already X509Certificate[].
fn pke_clone_chain(vm: &mut Vm<'_>, chain: ObjectRef) -> Result<ObjectRef, VmError> {
    let length = vm.array_length(chain)?;
    let array_class_name = String::from(vm.class_name(vm.class_of(chain)));
    let component_name = array_class_name
        .strip_prefix("[L")
        .and_then(|s| s.strip_suffix(';'))
        .unwrap_or("java/lang/Object");
    let component = vm.resolve_class(component_name)?;
    let mut copy = vm.allocate_object_array(component, length)?;
    for index in 0..length {
        let value = vm.array_get(chain, index as i32)?;
        vm.array_set_ref(copy, index, value.as_ref())?;
    }
    if length > 0 {
        let first = vm.array_get(copy, 0)?.as_ref();
        let x509 = vm.resolve_class("java/security/cert/X509Certificate")?;
        if vm.is_instance(first, x509)
            && array_class_name != "[Ljava/security/cert/X509Certificate;"
        {
            let retyped = vm.allocate_object_array(x509, length)?;
            for index in 0..length {
                let value = vm.array_get(copy, index as i32)?;
                vm.array_set_ref(retyped, index, value.as_ref())?;
            }
            copy = retyped;
        }
    }
    Ok(copy)
}

fn pke_init(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    private_key: ObjectRef,
    chain: ObjectRef,
) -> Result<(), VmError> {
    if private_key.is_null() || chain.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", Some("invalid null input")));
    }
    let length = vm.array_length(chain)?;
    if length == 0 {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("invalid zero-length input chain"),
        ));
    }
    let cloned = pke_clone_chain(vm, chain)?;
    // All certificates must share the same type.
    let first = vm.array_get(cloned, 0)?.as_ref();
    let (tc, get_type) = vm.resolve_virtual_method(first, "getType", "()Ljava/lang/String;")?;
    let cert_type = vm
        .invoke_method(tc, get_type, vec![Value::Ref(first)])?
        .as_ref();
    let cert_type_text = vm.string_or_empty(cert_type);
    for index in 1..length {
        let cert = vm.array_get(cloned, index as i32)?.as_ref();
        let (c, m) = vm.resolve_virtual_method(cert, "getType", "()Ljava/lang/String;")?;
        let t = vm.invoke_method(c, m, vec![Value::Ref(cert)])?.as_ref();
        if vm.string_or_empty(t) != cert_type_text {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("chain does not contain certificates of the same type"),
            ));
        }
    }
    // The private key algorithm must match the end-entity public key algorithm.
    let (kc, get_alg) =
        vm.resolve_virtual_method(private_key, "getAlgorithm", "()Ljava/lang/String;")?;
    let key_alg = vm
        .invoke_method(kc, get_alg, vec![Value::Ref(private_key)])?
        .as_ref();
    let (pc, get_pub) =
        vm.resolve_virtual_method(first, "getPublicKey", "()Ljava/security/PublicKey;")?;
    let pub_key = vm
        .invoke_method(pc, get_pub, vec![Value::Ref(first)])?
        .as_ref();
    let (qc, pub_alg) =
        vm.resolve_virtual_method(pub_key, "getAlgorithm", "()Ljava/lang/String;")?;
    let pub_alg_ref = vm
        .invoke_method(qc, pub_alg, vec![Value::Ref(pub_key)])?
        .as_ref();
    if vm.string_or_empty(key_alg) != vm.string_or_empty(pub_alg_ref) {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some(
                "private key algorithm does not match algorithm of public key in end entity certificate (at index 0)",
            ),
        ));
    }
    set_ref_field(
        vm,
        this,
        "privKey",
        "Ljava/security/PrivateKey;",
        private_key,
    )?;
    set_ref_field(
        vm,
        this,
        "chain",
        "[Ljava/security/cert/Certificate;",
        cloned,
    )?;
    let empty = new_hash_set(vm)?;
    set_ref_field(vm, this, "attributes", "Ljava/util/Set;", empty)?;
    Ok(())
}

const KEY_STORE_PRIVATE_KEY_ENTRY_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "<init>",
        "(Ljava/security/PrivateKey;[Ljava/security/cert/Certificate;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            pke_init(vm, receiver(args), ref_arg(args, 1), ref_arg(args, 2))?;
            void()
        },
    ),
    method(
        "getPrivateKey",
        "()Ljava/security/PrivateKey;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "privKey",
                "Ljava/security/PrivateKey;",
            ))
        },
    ),
    method(
        "getCertificateChain",
        "()[Ljava/security/cert/Certificate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let chain = ref_field(
                vm,
                receiver(args),
                "chain",
                "[Ljava/security/cert/Certificate;",
            );
            object(pke_clone_chain(vm, chain)?)
        },
    ),
    method(
        "getCertificate",
        "()Ljava/security/cert/Certificate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let chain = ref_field(
                vm,
                receiver(args),
                "chain",
                "[Ljava/security/cert/Certificate;",
            );
            object(vm.array_get(chain, 0)?.as_ref())
        },
    ),
];

pub(crate) const KEY_STORE_PRIVATE_KEY_ENTRY: oxjvm_vm::NativeClass = class(
    "java/security/KeyStore$PrivateKeyEntry",
    Some("java/lang/Object"),
    &["java/security/KeyStore$Entry"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("privKey", "Ljava/security/PrivateKey;", ACC_PRIVATE, None),
        field(
            "chain",
            "[Ljava/security/cert/Certificate;",
            ACC_PRIVATE,
            None,
        ),
        field("attributes", "Ljava/util/Set;", ACC_PRIVATE, None),
    ],
    &KEY_STORE_PRIVATE_KEY_ENTRY_METHODS,
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
    &[field("password", "[C", ACC_PRIVATE, None)],
    &[
        method("<init>", "([C)V", ACC_PUBLIC, |vm, _, args| {
            let password = ref_arg(args, 1);
            if password.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            set_ref_field(vm, receiver(args), "password", "[C", password)?;
            void()
        }),
        method("getPassword", "()[C", ACC_PUBLIC, |vm, _, args| {
            object(ref_field(vm, receiver(args), "password", "[C"))
        }),
        method("destroy", "()V", ACC_PUBLIC, |_, _, _| void()),
    ],
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
    let folded = cert_type.to_uppercase();
    if folded != "X.509" && folded != "X509" {
        return Err(vm.throw_new(
            "java/security/cert/CertificateException",
            Some(&alloc::format!("{cert_type} not found")),
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
    // An X.509 certificate is a DER SEQUENCE; data that does not begin with the
    // SEQUENCE tag is not a parseable certificate (CertificateParsingException).
    if encoded.first() != Some(&0x30) {
        return Err(vm.throw_new(
            "java/security/cert/CertificateParsingException",
            Some("Unable to parse certificate data"),
        ));
    }
    let class = vm.resolve_class("java/security/cert/X509Certificate")?;
    let certificate = vm.new_instance(class)?;
    set_string_field(vm, certificate, "type", cert_type)?;
    let bytes = new_byte_array(vm, encoded)?;
    set_ref_field(vm, certificate, "encoded", "[B", bytes)?;
    Ok(certificate)
}

const CERTIFICATE_FACTORY_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
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
        |vm, _, _| {
            let certs = new_array_list(vm)?;
            object(new_cert_path(vm, certs)?)
        },
    ),
    method(
        "generateCertPath",
        "(Ljava/util/List;)Ljava/security/cert/CertPath;",
        ACC_PUBLIC,
        |vm, _, args| {
            let certs = ref_arg(args, 1);
            if certs.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(new_cert_path(vm, certs)?)
        },
    ),
];

/// Build an `OjvmCertPath` of type X.509 holding the given certificate list.
fn new_cert_path(vm: &mut Vm<'_>, certs: ObjectRef) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/security/cert/OjvmCertPath")?;
    let path = vm.new_instance(class)?;
    set_string_field(vm, path, "type", "X.509")?;
    set_ref_field(vm, path, "certs", "Ljava/util/List;", certs)?;
    Ok(path)
}

pub(crate) const CERT_PATH: oxjvm_vm::NativeClass = class(
    "java/security/cert/CertPath",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[
        field("type", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("certs", "Ljava/util/List;", ACC_PRIVATE, None),
    ],
    &[
        method(
            "getType",
            "()Ljava/lang/String;",
            ACC_PUBLIC,
            |vm, _, args| object(vm.make_string(&string_field(vm, receiver(args), "type"))?),
        ),
        method(
            "getCertificates",
            "()Ljava/util/List;",
            ACC_PUBLIC,
            |vm, _, args| object(ref_field(vm, receiver(args), "certs", "Ljava/util/List;")),
        ),
        method("getEncoded", "()[B", ACC_PUBLIC, |vm, _, _| {
            object(new_byte_array(vm, &[])?)
        }),
    ],
    None,
);

pub(crate) const OJVM_CERT_PATH: oxjvm_vm::NativeClass = class(
    "java/security/cert/OjvmCertPath",
    Some("java/security/cert/CertPath"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[],
    &[],
    None,
);

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

const CERTIFICATE_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PROTECTED,
        |vm, _, args| {
            set_ref_field(
                vm,
                receiver(args),
                "type",
                "Ljava/lang/String;",
                ref_arg(args, 1),
            )?;
            void()
        },
    ),
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
        "verify",
        "(Ljava/security/PublicKey;Ljava/security/Provider;)V",
        ACC_PUBLIC,
        |vm, _, _| Err(vm.throw_new("java/lang/UnsupportedOperationException", None)),
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

const X509_CERTIFICATE_METHODS: [oxjvm_vm::NativeMethodDef; 26] = [
    method("<init>", "()V", ACC_PROTECTED, |vm, _, args| {
        set_string_field(vm, receiver(args), "type", "X.509")?;
        void()
    }),
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
            let certs = ref_arg(args, 2);
            if !certs.is_null() {
                let copy = clone_cert_array(vm, certs)?;
                set_ref_field(
                    vm,
                    this,
                    "certificates",
                    "[Ljava/security/cert/Certificate;",
                    copy,
                )?;
            }
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
            let signers = ref_arg(args, 2);
            if !signers.is_null() {
                let copy = clone_signer_array(vm, signers)?;
                set_ref_field(vm, this, "codeSigners", "[Ljava/security/CodeSigner;", copy)?;
            }
            void()
        },
    ),
    method(
        "getLocation",
        "()Ljava/net/URL;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| object(ref_field(vm, receiver(args), "location", "Ljava/net/URL;")),
    ),
    method(
        "getCertificates",
        "()[Ljava/security/cert/Certificate;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let certs = ref_field(
                vm,
                this,
                "certificates",
                "[Ljava/security/cert/Certificate;",
            );
            if !certs.is_null() {
                return object(clone_cert_array(vm, certs)?);
            }
            let signers = ref_field(vm, this, "codeSigners", "[Ljava/security/CodeSigner;");
            if !signers.is_null() {
                // Flattening signer cert paths needs CertPath support; with no
                // chains available the JDK conversion yields an empty array.
                let cert_class = vm.resolve_class("java/security/cert/Certificate")?;
                let empty = vm.allocate_object_array(cert_class, 0)?;
                set_ref_field(
                    vm,
                    this,
                    "certificates",
                    "[Ljava/security/cert/Certificate;",
                    empty,
                )?;
                return object(clone_cert_array(vm, empty)?);
            }
            object(ObjectRef::NULL)
        },
    ),
    method(
        "getCodeSigners",
        "()[Ljava/security/CodeSigner;",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let this = receiver(args);
            let signers = ref_field(vm, this, "codeSigners", "[Ljava/security/CodeSigner;");
            if !signers.is_null() {
                return object(clone_signer_array(vm, signers)?);
            }
            let certs = ref_field(
                vm,
                this,
                "certificates",
                "[Ljava/security/cert/Certificate;",
            );
            if !certs.is_null() {
                // Only X.509 certificates convert to code signers, and building
                // their cert paths is unsupported, so the conversion yields an
                // empty (non-null) signer array exactly as for non-X.509 input.
                let signer_class = vm.resolve_class("java/security/CodeSigner")?;
                let empty = vm.allocate_object_array(signer_class, 0)?;
                set_ref_field(
                    vm,
                    this,
                    "codeSigners",
                    "[Ljava/security/CodeSigner;",
                    empty,
                )?;
                return object(clone_signer_array(vm, empty)?);
            }
            object(ObjectRef::NULL)
        },
    ),
    method(
        "implies",
        "(Ljava/security/CodeSource;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let that = ref_arg(args, 1);
            if that.is_null() {
                return boolean(false);
            }
            if !code_source_match_certs(vm, this, that)? {
                return boolean(false);
            }
            boolean(code_source_match_location(vm, this, that)?)
        },
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

/// `CodeSource.matchCerts` (subset match): a source with no certs and no signers
/// matches any key; otherwise each signer/cert on this side must equal some on
/// the other side, and both sides must carry the same kind of credential.
fn code_source_match_certs(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    that: ObjectRef,
) -> Result<bool, VmError> {
    let this_certs = ref_field(
        vm,
        this,
        "certificates",
        "[Ljava/security/cert/Certificate;",
    );
    let this_signers = ref_field(vm, this, "codeSigners", "[Ljava/security/CodeSigner;");
    let that_certs = ref_field(
        vm,
        that,
        "certificates",
        "[Ljava/security/cert/Certificate;",
    );
    let that_signers = ref_field(vm, that, "codeSigners", "[Ljava/security/CodeSigner;");
    if this_certs.is_null() && this_signers.is_null() {
        return Ok(true);
    }
    if !this_signers.is_null() && !that_signers.is_null() {
        return code_source_array_subset(vm, this_signers, that_signers);
    }
    if !this_certs.is_null() && !that_certs.is_null() {
        return code_source_array_subset(vm, this_certs, that_certs);
    }
    Ok(false)
}

fn code_source_array_subset(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<bool, VmError> {
    let len_a = vm.array_length(a)?;
    let len_b = vm.array_length(b)?;
    for i in 0..len_a {
        let ea = vm.array_get(a, i as i32)?.as_ref();
        let mut found = false;
        for j in 0..len_b {
            let eb = vm.array_get(b, j as i32)?.as_ref();
            if ref_equals(vm, ea, eb)? {
                found = true;
                break;
            }
        }
        if !found {
            return Ok(false);
        }
    }
    Ok(true)
}

/// `CodeSource.matchLocation`: protocol (case-insensitive), port (default-port
/// aware), file (recursive `/-`, flat `/*`, or exact), reference, and host.
fn code_source_match_location(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    that: ObjectRef,
) -> Result<bool, VmError> {
    let this_loc = ref_field(vm, this, "location", "Ljava/net/URL;");
    if this_loc.is_null() {
        return Ok(true);
    }
    let that_loc = ref_field(vm, that, "location", "Ljava/net/URL;");
    if that_loc.is_null() {
        return Ok(false);
    }
    if ref_equals(vm, this_loc, that_loc)? {
        return Ok(true);
    }
    let this_protocol = url_call_string(vm, this_loc, "getProtocol")?.unwrap_or_default();
    let that_protocol = url_call_string(vm, that_loc, "getProtocol")?.unwrap_or_default();
    if !this_protocol.eq_ignore_ascii_case(&that_protocol) {
        return Ok(false);
    }
    let this_port = url_call_int(vm, this_loc, "getPort")?;
    if this_port != -1 {
        let that_port = url_call_int(vm, that_loc, "getPort")?;
        let port = if that_port != -1 {
            that_port
        } else {
            url_call_int(vm, that_loc, "getDefaultPort")?
        };
        if this_port != port {
            return Ok(false);
        }
    }
    let this_file = url_call_string(vm, this_loc, "getFile")?.unwrap_or_default();
    let that_file = url_call_string(vm, that_loc, "getFile")?.unwrap_or_default();
    if let Some(prefix) = this_file.strip_suffix("/-") {
        let this_path = alloc::format!("{prefix}/");
        if !that_file.starts_with(&this_path) {
            return Ok(false);
        }
    } else if let Some(prefix) = this_file.strip_suffix("/*") {
        let this_path = alloc::format!("{prefix}/");
        let Some(last) = that_file.rfind('/') else {
            return Ok(false);
        };
        if that_file[..last + 1] != this_path {
            return Ok(false);
        }
    } else if that_file != this_file && that_file != alloc::format!("{this_file}/") {
        return Ok(false);
    }
    if let Some(this_ref) = url_call_string(vm, this_loc, "getRef")? {
        let that_ref = url_call_string(vm, that_loc, "getRef")?;
        if that_ref.as_deref() != Some(this_ref.as_str()) {
            return Ok(false);
        }
    }
    if let Some(this_host) = url_call_string(vm, this_loc, "getHost")? {
        let that_host = url_call_string(vm, that_loc, "getHost")?;
        let this_local = this_host.is_empty() || this_host == "localhost";
        let that_local = that_host
            .as_deref()
            .is_some_and(|host| host.is_empty() || host == "localhost");
        if this_local && that_local {
            // ok
        } else {
            let Some(that_host) = that_host else {
                return Ok(false);
            };
            if this_host != that_host && !code_source_host_implies(vm, &this_host, &that_host)? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

/// Host implication for `CodeSource`: a wildcard `*.suffix` covers any host (or
/// deeper wildcard) ending in `.suffix`; a non-wildcard never implies a wildcard;
/// otherwise resolve both names and compare addresses.
fn code_source_host_implies(
    vm: &mut Vm<'_>,
    this_host: &str,
    that_host: &str,
) -> Result<bool, VmError> {
    let this_lower = this_host.to_ascii_lowercase();
    let that_lower = that_host.to_ascii_lowercase();
    if let Some(suffix) = this_lower.strip_prefix("*.") {
        return Ok(that_lower.ends_with(&alloc::format!(".{suffix}")));
    }
    if that_lower.starts_with("*.") {
        return Ok(false);
    }
    if this_lower == that_lower {
        return Ok(true);
    }
    let this_ip = resolve_host_ip(vm, this_host)?;
    let that_ip = resolve_host_ip(vm, that_host)?;
    Ok(match (this_ip, that_ip) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    })
}

/// Resolve a host to its address string, or `None` when the name is unknown.
fn resolve_host_ip(vm: &mut Vm<'_>, host: &str) -> Result<Option<String>, VmError> {
    let address = match crate::net::resolve_address(vm, "java/net/Inet4Address", host) {
        Ok(value) => value.as_ref(),
        Err(VmError::Thrown(exc)) => {
            let unknown = vm.resolve_class("java/net/UnknownHostException")?;
            if vm.is_instance(exc, unknown) {
                return Ok(None);
            }
            return Err(VmError::Thrown(exc));
        }
        Err(error) => return Err(error),
    };
    if address.is_null() {
        return Ok(None);
    }
    let (class, method) =
        vm.resolve_virtual_method(address, "getHostAddress", "()Ljava/lang/String;")?;
    let value = vm.invoke_method(class, method, vec![Value::Ref(address)])?;
    Ok(vm.string_value(value.as_ref()))
}

fn url_call_string(vm: &mut Vm<'_>, url: ObjectRef, name: &str) -> Result<Option<String>, VmError> {
    let (class, method) = vm.resolve_virtual_method(url, name, "()Ljava/lang/String;")?;
    let value = vm.invoke_method(class, method, vec![Value::Ref(url)])?;
    let reference = value.as_ref();
    if reference.is_null() {
        return Ok(None);
    }
    Ok(vm.string_value(reference))
}

fn url_call_int(vm: &mut Vm<'_>, url: ObjectRef, name: &str) -> Result<i32, VmError> {
    let (class, method) = vm.resolve_virtual_method(url, name, "()I")?;
    let value = vm.invoke_method(class, method, vec![Value::Ref(url)])?;
    Ok(value.as_int())
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
        |vm, _, args| {
            let this = receiver(args);
            let permission = ref_arg(args, 1);
            let perms = ref_field(
                vm,
                this,
                "permissions",
                "Ljava/security/PermissionCollection;",
            );
            // A domain with no permissions implies nothing (the JDK consults the
            // policy here, which this VM does not install).
            if perms.is_null() {
                return boolean(false);
            }
            let (class, method) =
                vm.resolve_virtual_method(perms, "implies", "(Ljava/security/Permission;)Z")?;
            let value = vm.invoke_method(
                class,
                method,
                vec![Value::Ref(perms), Value::Ref(permission)],
            )?;
            boolean(value.as_boolean())
        },
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
    let runtime = vm.resolve_class("java/lang/RuntimeException")?;
    let error = vm.resolve_class("java/lang/Error")?;
    match vm.invoke_method(class, method, vec![Value::Ref(action)]) {
        Ok(value) => Ok(value),
        Err(VmError::Thrown(exc)) if exception_action => {
            // A PrivilegedExceptionAction wraps checked exceptions in a
            // PrivilegedActionException; runtime exceptions and errors propagate.
            if vm.is_instance(exc, runtime) || vm.is_instance(exc, error) {
                return Err(VmError::Thrown(exc));
            }
            let pae_class = vm.resolve_class("java/security/PrivilegedActionException")?;
            let pae = vm.new_instance(pae_class)?;
            let (declaring, ctor) = vm
                .find_method(pae_class, "<init>", "(Ljava/lang/Exception;)V")
                .ok_or_else(|| {
                    VmError::internal("PrivilegedActionException lacks its constructor")
                })?;
            vm.invoke_method(declaring, ctor, vec![Value::Ref(pae), Value::Ref(exc)])?;
            Err(VmError::Thrown(pae))
        }
        Err(error) => Err(error),
    }
}

const ACCESS_CONTROLLER_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "getContext",
        "()Ljava/security/AccessControlContext;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/security/AccessControlContext")?;
            let acc = vm.new_instance(class)?;
            object(acc)
        },
    ),
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
    &[
        field(
            "context",
            "[Ljava/security/ProtectionDomain;",
            ACC_PRIVATE,
            None,
        ),
        field(
            "combiner",
            "Ljava/security/DomainCombiner;",
            ACC_PRIVATE,
            None,
        ),
    ],
    &ACCESS_CONTROL_CONTEXT_METHODS,
    None,
);

const ACCESS_CONTROL_CONTEXT_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "<init>",
        "([Ljava/security/ProtectionDomain;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let context = ref_arg(args, 1);
            if context.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            // The JDK keeps the non-null, deduplicated protection domains; an
            // all-null or empty array collapses to a null context.
            let len = vm.array_length(context)?;
            let mut kept: Vec<ObjectRef> = Vec::new();
            for index in 0..len {
                let domain = vm.array_get(context, index as i32)?.as_ref();
                if domain.is_null() {
                    continue;
                }
                let mut duplicate = false;
                for &existing in &kept {
                    if ref_equals(vm, existing, domain)? {
                        duplicate = true;
                        break;
                    }
                }
                if !duplicate {
                    kept.push(domain);
                }
            }
            if !kept.is_empty() {
                let domain_class = vm.resolve_class("java/security/ProtectionDomain")?;
                let array = vm.allocate_object_array(domain_class, kept.len())?;
                for (index, &domain) in kept.iter().enumerate() {
                    vm.array_set_ref(array, index, domain)?;
                }
                set_ref_field(
                    vm,
                    this,
                    "context",
                    "[Ljava/security/ProtectionDomain;",
                    array,
                )?;
            }
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/security/AccessControlContext;Ljava/security/DomainCombiner;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let acc = ref_arg(args, 1);
            if acc.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let context = ref_field(vm, acc, "context", "[Ljava/security/ProtectionDomain;");
            set_ref_field(
                vm,
                this,
                "context",
                "[Ljava/security/ProtectionDomain;",
                context,
            )?;
            set_ref_field(
                vm,
                this,
                "combiner",
                "Ljava/security/DomainCombiner;",
                ref_arg(args, 2),
            )?;
            void()
        },
    ),
    method(
        "getDomainCombiner",
        "()Ljava/security/DomainCombiner;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "combiner",
                "Ljava/security/DomainCombiner;",
            ))
        },
    ),
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
            let acc_class = vm.resolve_class("java/security/AccessControlContext")?;
            if other.is_null() || !vm.is_instance(other, acc_class) {
                return boolean(false);
            }
            let this_context = ref_field(vm, this, "context", "[Ljava/security/ProtectionDomain;");
            let that_context = ref_field(vm, other, "context", "[Ljava/security/ProtectionDomain;");
            if !acc_equal_pds(vm, this_context, that_context)? {
                return boolean(false);
            }
            let this_combiner = ref_field(vm, this, "combiner", "Ljava/security/DomainCombiner;");
            let that_combiner = ref_field(vm, other, "combiner", "Ljava/security/DomainCombiner;");
            if this_combiner.is_null() {
                return boolean(that_combiner.is_null());
            }
            boolean(ref_equals(vm, this_combiner, that_combiner)?)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let context = ref_field(vm, this, "context", "[Ljava/security/ProtectionDomain;");
        let mut hash = 0i32;
        if !context.is_null() {
            let len = vm.array_length(context)?;
            for index in 0..len {
                let domain = vm.array_get(context, index as i32)?.as_ref();
                if !domain.is_null() {
                    let (declaring, method) =
                        vm.resolve_virtual_method(domain, "hashCode", "()I")?;
                    let value = vm.invoke_method(declaring, method, vec![Value::Ref(domain)])?;
                    hash = hash.wrapping_add(value.as_int());
                }
            }
        }
        int(hash)
    }),
];

/// `AccessControlContext.equalPDs`: two null contexts match; otherwise each
/// protection domain on either side must equal some domain on the other side.
fn acc_equal_pds(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<bool, VmError> {
    if a.is_null() {
        return Ok(b.is_null());
    }
    if b.is_null() {
        return Ok(false);
    }
    Ok(acc_contains_all_pds(vm, a, b)? && acc_contains_all_pds(vm, b, a)?)
}

fn acc_contains_all_pds(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<bool, VmError> {
    let len_a = vm.array_length(a)?;
    let len_b = vm.array_length(b)?;
    for i in 0..len_a {
        let pa = vm.array_get(a, i as i32)?.as_ref();
        let mut found = false;
        for j in 0..len_b {
            let pb = vm.array_get(b, j as i32)?.as_ref();
            if ref_equals(vm, pa, pb)? {
                found = true;
                break;
            }
        }
        if !found {
            return Ok(false);
        }
    }
    Ok(true)
}

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
                let exception = ref_arg(args, 1);
                set_ref_field(vm, this, "exception", "Ljava/lang/Exception;", exception)?;
                // The JDK calls super(null, exception): no message, cause set.
                let exception_ctor = vm.find_method(
                    vm.class_of(this),
                    "<init>",
                    "(Ljava/lang/String;Ljava/lang/Throwable;)V",
                );
                if let Some((declaring, ctor)) = exception_ctor {
                    vm.invoke_method(
                        declaring,
                        ctor,
                        vec![
                            Value::Ref(this),
                            Value::Ref(ObjectRef::NULL),
                            Value::Ref(exception),
                        ],
                    )?;
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
        method(
            "toString",
            "()Ljava/lang/String;",
            ACC_PUBLIC,
            |vm, _, args| {
                let this = receiver(args);
                let name = vm.class_name(vm.class_of(this)).replace('/', ".");
                let cause = ref_field(vm, this, "exception", "Ljava/lang/Exception;");
                let text = if cause.is_null() {
                    name
                } else {
                    let (class, method) =
                        vm.resolve_virtual_method(cause, "toString", "()Ljava/lang/String;")?;
                    let value = vm.invoke_method(class, method, vec![Value::Ref(cause)])?;
                    alloc::format!("{name}: {}", vm.string_or_empty(value.as_ref()))
                };
                object(vm.make_string(&text)?)
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
            // The JDK constructor stores the name without a null check; each
            // concrete subclass validates its own arguments.
            set_ref_field(
                vm,
                receiver(args),
                "name",
                "Ljava/lang/String;",
                ref_arg(args, 1),
            )?;
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
            let this = receiver(args);
            let class_name = crate::binary_name(vm, vm.class_of(this));
            let name = string_field(vm, this, "name");
            let (declaring, get_actions) =
                vm.resolve_virtual_method(this, "getActions", "()Ljava/lang/String;")?;
            let actions_value = vm.invoke_method(declaring, get_actions, vec![Value::Ref(this)])?;
            let actions = match actions_value {
                Value::Ref(reference) if !reference.is_null() => {
                    vm.string_value(reference).unwrap_or_default()
                }
                _ => String::new(),
            };
            let text = if actions.is_empty() {
                alloc::format!("(\"{class_name}\" \"{name}\")")
            } else {
                alloc::format!("(\"{class_name}\" \"{name}\" \"{actions}\")")
            };
            object(vm.make_string(&text)?)
        },
    ),
];

// -------------------------------------------------------------------------------------------
// java.security.AllPermission
// -------------------------------------------------------------------------------------------

pub(crate) const ALL_PERMISSION: oxjvm_vm::NativeClass = class(
    "java/security/AllPermission",
    Some("java/security/Permission"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &ALL_PERMISSION_METHODS,
    None,
);

const ALL_PERMISSION_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let parent = vm.resolve_class("java/security/Permission")?;
        let (declaring, ctor) = vm
            .find_method(parent, "<init>", "(Ljava/lang/String;)V")
            .ok_or_else(|| VmError::internal("Permission lacks <init>"))?;
        let name = vm.make_string("<all permissions>")?;
        vm.invoke_method(declaring, ctor, vec![args[0], Value::Ref(name)])?;
        void()
    }),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            // The JDK ignores both arguments and delegates to AllPermission().
            let class = vm.resolve_class("java/security/AllPermission")?;
            let (declaring, ctor) = vm
                .find_method(class, "<init>", "()V")
                .ok_or_else(|| VmError::internal("AllPermission lacks <init>"))?;
            vm.invoke_method(declaring, ctor, vec![args[0]])?;
            void()
        },
    ),
    method(
        "implies",
        "(Ljava/security/Permission;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(true),
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            let all = vm.resolve_class("java/security/AllPermission")?;
            boolean(!other.is_null() && vm.is_instance(other, all))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |_, _, _| int(1)),
    method(
        "getActions",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("<all actions>")?),
    ),
    method(
        "newPermissionCollection",
        "()Ljava/security/PermissionCollection;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/security/AllPermissionCollection")?;
            let instance = vm.new_instance(class)?;
            let (declaring, ctor) = vm
                .find_method(class, "<init>", "()V")
                .ok_or_else(|| VmError::internal("AllPermissionCollection lacks <init>"))?;
            vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
            object(instance)
        },
    ),
];

pub(crate) const ALL_PERMISSION_COLLECTION: oxjvm_vm::NativeClass = class(
    "java/security/AllPermissionCollection",
    Some("java/security/PermissionCollection"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[field("allAllowed", "I", ACC_PRIVATE, None)],
    &ALL_PERMISSION_COLLECTION_METHODS,
    None,
);

const ALL_PERMISSION_COLLECTION_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "add",
        "(Ljava/security/Permission;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let permission = ref_arg(args, 1);
            let all = vm.resolve_class("java/security/AllPermission")?;
            if permission.is_null() || !vm.is_instance(permission, all) {
                let display = permission_display(vm, permission)?;
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some(&alloc::format!("invalid permission: {display}")),
                ));
            }
            ensure_collection_writable(vm, this)?;
            crate::lang::write_int_field(vm, this, "allAllowed", 1);
            void()
        },
    ),
    method(
        "implies",
        "(Ljava/security/Permission;)Z",
        ACC_PUBLIC,
        |vm, _, args| boolean(crate::lang::read_int_field(vm, receiver(args), "allAllowed") != 0),
    ),
    method(
        "elements",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC,
        |vm, _, args| {
            if crate::lang::read_int_field(vm, receiver(args), "allAllowed") == 0 {
                return object(crate::util::new_enumeration(vm, &[])?);
            }
            let class = vm.resolve_class("java/security/AllPermission")?;
            let instance = vm.new_instance(class)?;
            let (declaring, ctor) = vm
                .find_method(class, "<init>", "()V")
                .ok_or_else(|| VmError::internal("AllPermission lacks <init>"))?;
            vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
            object(crate::util::new_enumeration(vm, &[instance])?)
        },
    ),
];

// -------------------------------------------------------------------------------------------
// java.util.PropertyPermissionCollection
// -------------------------------------------------------------------------------------------

fn property_collection_mask(vm: &Vm<'_>, list: ObjectRef, name: &str) -> Option<i32> {
    for existing in crate::util::array_list_elements(vm, list) {
        if string_field(vm, existing, "name") == name {
            return Some(crate::lang::read_int_field(vm, existing, "mask"));
        }
    }
    None
}

/// The package-private PropertyPermission(String, int) constructor used when
/// the collection merges two masks for the same property name.
fn property_permission_with_mask(
    vm: &mut Vm<'_>,
    name: &str,
    mask: i32,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/PropertyPermission")?;
    let instance = vm.new_instance(class)?;
    let basic = vm.resolve_class("java/security/BasicPermission")?;
    let (declaring, ctor) = vm
        .find_method(basic, "<init>", "(Ljava/lang/String;)V")
        .ok_or_else(|| VmError::internal("BasicPermission lacks <init>"))?;
    let name_value = vm.make_string(name)?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![Value::Ref(instance), Value::Ref(name_value)],
    )?;
    crate::lang::write_int_field(vm, instance, "mask", mask);
    Ok(instance)
}

pub(crate) const PROPERTY_PERMISSION_COLLECTION: oxjvm_vm::NativeClass = class(
    "java/util/PropertyPermissionCollection",
    Some("java/security/PermissionCollection"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("perms", "Ljava/util/ArrayList;", ACC_PRIVATE, None),
        field("allAllowed", "I", ACC_PRIVATE, None),
    ],
    &PROPERTY_PERMISSION_COLLECTION_METHODS,
    None,
);

const PROPERTY_PERMISSION_COLLECTION_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let list = new_array_list(vm)?;
        set_ref_field(vm, this, "perms", "Ljava/util/ArrayList;", list)?;
        void()
    }),
    method(
        "add",
        "(Ljava/security/Permission;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let permission = ref_arg(args, 1);
            let property = vm.resolve_class("java/util/PropertyPermission")?;
            if permission.is_null() || !vm.is_instance(permission, property) {
                let display = permission_display(vm, permission)?;
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some(&alloc::format!("invalid permission: {display}")),
                ));
            }
            ensure_collection_writable(vm, this)?;
            let list = ref_field(vm, this, "perms", "Ljava/util/ArrayList;");
            let name = string_field(vm, permission, "name");
            let elements = crate::util::array_list_elements(vm, list);
            let mut found = false;
            for (index, existing) in elements.iter().enumerate() {
                if string_field(vm, *existing, "name") != name {
                    continue;
                }
                found = true;
                let old_mask = crate::lang::read_int_field(vm, *existing, "mask");
                let new_mask = crate::lang::read_int_field(vm, permission, "mask");
                if old_mask != new_mask {
                    let effective = old_mask | new_mask;
                    if effective == new_mask {
                        list_set(vm, list, index, permission)?;
                    } else if effective != old_mask {
                        let merged = property_permission_with_mask(vm, &name, effective)?;
                        list_set(vm, list, index, merged)?;
                    }
                }
                break;
            }
            if !found {
                list_add(vm, list, permission)?;
            }
            if name == "*" {
                crate::lang::write_int_field(vm, this, "allAllowed", 1);
            }
            void()
        },
    ),
    method(
        "implies",
        "(Ljava/security/Permission;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let permission = ref_arg(args, 1);
            let property = vm.resolve_class("java/util/PropertyPermission")?;
            if permission.is_null() || !vm.is_instance(permission, property) {
                return boolean(false);
            }
            let list = ref_field(vm, this, "perms", "Ljava/util/ArrayList;");
            let desired = crate::lang::read_int_field(vm, permission, "mask");
            let mut effective = 0;
            if crate::lang::read_int_field(vm, this, "allAllowed") != 0 {
                if let Some(mask) = property_collection_mask(vm, list, "*") {
                    effective |= mask;
                    if (effective & desired) == desired {
                        return boolean(true);
                    }
                }
            }
            let name = string_field(vm, permission, "name");
            if let Some(mask) = property_collection_mask(vm, list, &name) {
                effective |= mask;
                if (effective & desired) == desired {
                    return boolean(true);
                }
            }
            // Walk up the dotted name looking for a.b.* style entries.
            let chars: Vec<char> = name.chars().collect();
            let mut offset = chars.len() as isize - 1;
            loop {
                let mut last = -1isize;
                let mut index = offset;
                while index >= 0 {
                    if chars[index as usize] == '.' {
                        last = index;
                        break;
                    }
                    index -= 1;
                }
                if last == -1 {
                    break;
                }
                let prefix: String = chars[..(last + 1) as usize].iter().collect();
                let wildcard = alloc::format!("{prefix}*");
                if let Some(mask) = property_collection_mask(vm, list, &wildcard) {
                    effective |= mask;
                    if (effective & desired) == desired {
                        return boolean(true);
                    }
                }
                offset = last - 1;
            }
            boolean(false)
        },
    ),
    method(
        "elements",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let list = ref_field(vm, receiver(args), "perms", "Ljava/util/ArrayList;");
            let perms = if list.is_null() {
                Vec::new()
            } else {
                crate::util::array_list_elements(vm, list)
            };
            object(crate::util::new_enumeration(vm, &perms)?)
        },
    ),
];

// -------------------------------------------------------------------------------------------
// java.io.FilePermission
// -------------------------------------------------------------------------------------------

/// Parse FilePermission actions into a mask (EXECUTE=1, WRITE=2, READ=4,
/// DELETE=8, READLINK=16) using the JDK's backwards comma scan. Unlike
/// PropertyPermission, a leading comma is rejected here.
fn file_actions_mask(vm: &mut Vm<'_>, actions: ObjectRef) -> Result<i32, VmError> {
    if actions.is_null() {
        return Ok(0);
    }
    let text = vm.string_value(actions).unwrap_or_default();
    let invalid = alloc::format!("invalid permission: {text}");
    let chars: Vec<char> = text.chars().collect();
    let is_ws = |c: char| c == ' ' || c == '\r' || c == '\n' || c == '\u{000C}' || c == '\t';
    let mut i = chars.len() as isize - 1;
    if i < 0 {
        return Ok(0);
    }
    let mut mask = 0i32;
    while i != -1 {
        while i != -1 && is_ws(chars[i as usize]) {
            i -= 1;
        }
        let u = i as usize;
        let (matchlen, bit): (isize, i32) = if i >= 3
            && chars[u - 3].eq_ignore_ascii_case(&'r')
            && chars[u - 2].eq_ignore_ascii_case(&'e')
            && chars[u - 1].eq_ignore_ascii_case(&'a')
            && chars[u].eq_ignore_ascii_case(&'d')
        {
            (4, 4)
        } else if i >= 4
            && chars[u - 4].eq_ignore_ascii_case(&'w')
            && chars[u - 3].eq_ignore_ascii_case(&'r')
            && chars[u - 2].eq_ignore_ascii_case(&'i')
            && chars[u - 1].eq_ignore_ascii_case(&'t')
            && chars[u].eq_ignore_ascii_case(&'e')
        {
            (5, 2)
        } else if i >= 6
            && chars[u - 6].eq_ignore_ascii_case(&'e')
            && chars[u - 5].eq_ignore_ascii_case(&'x')
            && chars[u - 4].eq_ignore_ascii_case(&'e')
            && chars[u - 3].eq_ignore_ascii_case(&'c')
            && chars[u - 2].eq_ignore_ascii_case(&'u')
            && chars[u - 1].eq_ignore_ascii_case(&'t')
            && chars[u].eq_ignore_ascii_case(&'e')
        {
            (7, 1)
        } else if i >= 5
            && chars[u - 5].eq_ignore_ascii_case(&'d')
            && chars[u - 4].eq_ignore_ascii_case(&'e')
            && chars[u - 3].eq_ignore_ascii_case(&'l')
            && chars[u - 2].eq_ignore_ascii_case(&'e')
            && chars[u - 1].eq_ignore_ascii_case(&'t')
            && chars[u].eq_ignore_ascii_case(&'e')
        {
            (6, 8)
        } else if i >= 7
            && chars[u - 7].eq_ignore_ascii_case(&'r')
            && chars[u - 6].eq_ignore_ascii_case(&'e')
            && chars[u - 5].eq_ignore_ascii_case(&'a')
            && chars[u - 4].eq_ignore_ascii_case(&'d')
            && chars[u - 3].eq_ignore_ascii_case(&'l')
            && chars[u - 2].eq_ignore_ascii_case(&'i')
            && chars[u - 1].eq_ignore_ascii_case(&'n')
            && chars[u].eq_ignore_ascii_case(&'k')
        {
            (8, 16)
        } else {
            return Err(vm.throw_new("java/lang/IllegalArgumentException", Some(&invalid)));
        };
        mask |= bit;
        let mut seen_comma = false;
        while i >= matchlen && !seen_comma {
            let c = chars[(i - matchlen) as usize];
            if is_ws(c) {
                // keep scanning towards the comma
            } else if c == ',' && i > matchlen {
                seen_comma = true;
            } else {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", Some(&invalid)));
            }
            i -= 1;
        }
        i -= matchlen;
    }
    Ok(mask)
}

/// Normalize a path the way `new File(name).getPath()` followed by
/// `Path.normalize()` does on Unix: collapse duplicate separators, drop
/// trailing separators (except the root), then resolve "." and ".."
/// lexically. Returns None for paths the file system rejects (NUL bytes).
fn normalize_file_path(name: &str) -> Option<(bool, Vec<String>)> {
    if name.contains('\0') {
        return None;
    }
    let mut collapsed = String::new();
    let mut prev_slash = false;
    for c in name.chars() {
        if c == '/' {
            if prev_slash {
                continue;
            }
            prev_slash = true;
        } else {
            prev_slash = false;
        }
        collapsed.push(c);
    }
    while collapsed.len() > 1 && collapsed.ends_with('/') {
        collapsed.pop();
    }
    let absolute = collapsed.starts_with('/');
    let mut elements: Vec<String> = Vec::new();
    for part in collapsed.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            if let Some(last) = elements.last() {
                if last != ".." {
                    elements.pop();
                    continue;
                }
            }
            if absolute {
                continue;
            }
            elements.push(String::from(".."));
        } else {
            elements.push(String::from(part));
        }
    }
    Some((absolute, elements))
}

fn render_file_path(absolute: bool, elements: &[String]) -> String {
    if absolute {
        if elements.is_empty() {
            String::from("/")
        } else {
            alloc::format!("/{}", elements.join("/"))
        }
    } else {
        elements.join("/")
    }
}

fn file_path_parts(npath: &str) -> (bool, Vec<&str>) {
    if let Some(rest) = npath.strip_prefix('/') {
        (
            true,
            if rest.is_empty() {
                Vec::new()
            } else {
                rest.split('/').collect()
            },
        )
    } else {
        (
            false,
            if npath.is_empty() {
                Vec::new()
            } else {
                npath.split('/').collect()
            },
        )
    }
}

/// Depth between an outer normalized path and an inner one, mirroring
/// FilePermission.containsPath: -1 when there is no containment relation.
fn file_contains_path(p1: &str, p2: &str) -> i32 {
    let (abs1, e1) = file_path_parts(p1);
    let (abs2, e2) = file_path_parts(p2);
    if abs1 != abs2 {
        return -1;
    }
    let empty1 = !abs1 && e1.is_empty();
    let empty2 = !abs2 && e2.is_empty();
    if empty1 {
        if empty2 {
            return 0;
        }
        if e2[0] == ".." {
            return -1;
        }
        return e2.len() as i32;
    }
    if empty2 {
        if e1.last() == Some(&"..") {
            return e1.len() as i32;
        }
        return -1;
    }
    let c1 = e1.len();
    let c2 = e2.len();
    let n = c1.min(c2);
    let mut i = 0;
    while i < n && e1[i] == e2[i] {
        i += 1;
    }
    if i < c1 && e1[c1 - 1] != ".." {
        return -1;
    }
    if i < c2 && e2[i] == ".." {
        return -1;
    }
    (c1 - i + c2 - i) as i32
}

fn file_flag(vm: &Vm<'_>, permission: ObjectRef, name: &str) -> bool {
    crate::lang::read_int_field(vm, permission, name) != 0
}

/// FilePermission.impliesIgnoreMask: path containment ignoring the action
/// mask, including the wildcard-level and invalid-permission rules.
fn file_implies_ignore_mask(vm: &Vm<'_>, this: ObjectRef, that: ObjectRef) -> bool {
    if this == that {
        return true;
    }
    if file_flag(vm, this, "allFiles") {
        return true;
    }
    if file_flag(vm, this, "invalid") || file_flag(vm, that, "invalid") {
        return false;
    }
    if file_flag(vm, that, "allFiles") {
        return false;
    }
    let this_recursive = file_flag(vm, this, "recursive");
    let that_recursive = file_flag(vm, that, "recursive");
    let this_directory = file_flag(vm, this, "directory");
    let that_directory = file_flag(vm, that, "directory");
    if (this_recursive && that_recursive) != that_recursive
        || (this_directory && that_directory) != that_directory
    {
        return false;
    }
    let this_npath = string_field(vm, this, "npath");
    let that_npath = string_field(vm, that, "npath");
    if this_npath == that_npath && this_directory == that_directory {
        return true;
    }
    let diff = file_contains_path(&this_npath, &that_npath);
    if diff >= 1 && this_recursive {
        return true;
    }
    if diff == 1 && this_directory && !that_directory {
        return true;
    }
    false
}

/// Clone a FilePermission with a different action mask, as the collection's
/// merge does via the private withNewActions constructor.
fn file_permission_with_mask(
    vm: &mut Vm<'_>,
    input: ObjectRef,
    mask: i32,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/io/FilePermission")?;
    let instance = vm.new_instance(class)?;
    let name = ref_field(vm, input, "name", "Ljava/lang/String;");
    set_ref_field(vm, instance, "name", "Ljava/lang/String;", name)?;
    for flag in ["allFiles", "directory", "recursive", "invalid"] {
        crate::lang::write_int_field(
            vm,
            instance,
            flag,
            crate::lang::read_int_field(vm, input, flag),
        );
    }
    let npath = string_field(vm, input, "npath");
    set_string_field(vm, instance, "npath", &npath)?;
    crate::lang::write_int_field(vm, instance, "mask", mask);
    Ok(instance)
}

pub(crate) const FILE_PERMISSION: oxjvm_vm::NativeClass = class(
    "java/io/FilePermission",
    Some("java/security/Permission"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("mask", "I", ACC_PRIVATE, None),
        field("allFiles", "I", ACC_PRIVATE, None),
        field("directory", "I", ACC_PRIVATE, None),
        field("recursive", "I", ACC_PRIVATE, None),
        field("invalid", "I", ACC_PRIVATE, None),
        field("npath", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &FILE_PERMISSION_METHODS,
    None,
);

const FILE_PERMISSION_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let parent = vm.resolve_class("java/security/Permission")?;
            let (declaring, ctor) = vm
                .find_method(parent, "<init>", "(Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("Permission lacks <init>"))?;
            vm.invoke_method(declaring, ctor, vec![args[0], args[1]])?;
            let mask = file_actions_mask(vm, ref_arg(args, 2))?;
            if mask == 0 || (mask & 31) != mask {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("invalid actions mask"),
                ));
            }
            let name_ref = ref_arg(args, 1);
            if name_ref.is_null() {
                return Err(
                    vm.throw_new("java/lang/NullPointerException", Some("name can't be null"))
                );
            }
            let name = vm.string_value(name_ref).unwrap_or_default();
            crate::lang::write_int_field(vm, this, "mask", mask);
            if name == "<<ALL FILES>>" {
                crate::lang::write_int_field(vm, this, "allFiles", 1);
                set_string_field(vm, this, "npath", "")?;
                return void();
            }
            let remember_star = name.ends_with('*');
            let work = if remember_star {
                alloc::format!("{}-", &name[..name.len() - 1])
            } else {
                name.clone()
            };
            match normalize_file_path(&work) {
                None => {
                    crate::lang::write_int_field(vm, this, "invalid", 1);
                    set_string_field(vm, this, "npath", "-u-s-e-l-e-s-s-")?;
                }
                Some((absolute, mut elements)) => {
                    if elements.last().is_some_and(|last| last == "-") {
                        crate::lang::write_int_field(vm, this, "directory", 1);
                        crate::lang::write_int_field(
                            vm,
                            this,
                            "recursive",
                            i32::from(!remember_star),
                        );
                        elements.pop();
                    }
                    set_string_field(vm, this, "npath", &render_file_path(absolute, &elements))?;
                }
            }
            void()
        },
    ),
    method(
        "getActions",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let mask = crate::lang::read_int_field(vm, receiver(args), "mask");
            let mut parts: Vec<&str> = Vec::new();
            if mask & 4 != 0 {
                parts.push("read");
            }
            if mask & 2 != 0 {
                parts.push("write");
            }
            if mask & 1 != 0 {
                parts.push("execute");
            }
            if mask & 8 != 0 {
                parts.push("delete");
            }
            if mask & 16 != 0 {
                parts.push("readlink");
            }
            object(vm.make_string(&parts.join(","))?)
        },
    ),
    method(
        "implies",
        "(Ljava/security/Permission;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let file_permission = vm.resolve_class("java/io/FilePermission")?;
            if other.is_null() || !vm.is_instance(other, file_permission) {
                return boolean(false);
            }
            let this_mask = crate::lang::read_int_field(vm, this, "mask");
            let other_mask = crate::lang::read_int_field(vm, other, "mask");
            if (this_mask & other_mask) != other_mask {
                return boolean(false);
            }
            boolean(file_implies_ignore_mask(vm, this, other))
        },
    ),
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
            let file_permission = vm.resolve_class("java/io/FilePermission")?;
            if other.is_null() || !vm.is_instance(other, file_permission) {
                return boolean(false);
            }
            if file_flag(vm, this, "invalid") || file_flag(vm, other, "invalid") {
                return boolean(false);
            }
            boolean(
                crate::lang::read_int_field(vm, this, "mask")
                    == crate::lang::read_int_field(vm, other, "mask")
                    && file_flag(vm, this, "allFiles") == file_flag(vm, other, "allFiles")
                    && string_field(vm, this, "npath") == string_field(vm, other, "npath")
                    && file_flag(vm, this, "directory") == file_flag(vm, other, "directory")
                    && file_flag(vm, this, "recursive") == file_flag(vm, other, "recursive"),
            )
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let bool_hash = |flag: bool| if flag { 1231 } else { 1237 };
        let npath = string_field(vm, this, "npath");
        let units: Vec<u16> = npath.encode_utf16().collect();
        let elements = [
            crate::lang::read_int_field(vm, this, "mask"),
            bool_hash(file_flag(vm, this, "allFiles")),
            bool_hash(file_flag(vm, this, "directory")),
            bool_hash(file_flag(vm, this, "recursive")),
            crate::lang::java_string_hash(&units),
            0, // npath2 is only set during deserialization
            bool_hash(file_flag(vm, this, "invalid")),
        ];
        let mut result = 1i32;
        for element in elements {
            result = result.wrapping_mul(31).wrapping_add(element);
        }
        int(result)
    }),
    method(
        "newPermissionCollection",
        "()Ljava/security/PermissionCollection;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/io/FilePermissionCollection")?;
            let instance = vm.new_instance(class)?;
            let (declaring, ctor) = vm
                .find_method(class, "<init>", "()V")
                .ok_or_else(|| VmError::internal("FilePermissionCollection lacks <init>"))?;
            vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
            object(instance)
        },
    ),
];

pub(crate) const FILE_PERMISSION_COLLECTION: oxjvm_vm::NativeClass = class(
    "java/io/FilePermissionCollection",
    Some("java/security/PermissionCollection"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[field("perms", "Ljava/util/ArrayList;", ACC_PRIVATE, None)],
    &FILE_PERMISSION_COLLECTION_METHODS,
    None,
);

const FILE_PERMISSION_COLLECTION_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let list = new_array_list(vm)?;
        set_ref_field(vm, this, "perms", "Ljava/util/ArrayList;", list)?;
        void()
    }),
    method(
        "add",
        "(Ljava/security/Permission;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let permission = ref_arg(args, 1);
            let file_permission = vm.resolve_class("java/io/FilePermission")?;
            if permission.is_null() || !vm.is_instance(permission, file_permission) {
                let display = permission_display(vm, permission)?;
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some(&alloc::format!("invalid permission: {display}")),
                ));
            }
            ensure_collection_writable(vm, this)?;
            let list = ref_field(vm, this, "perms", "Ljava/util/ArrayList;");
            let name = string_field(vm, permission, "name");
            let elements = crate::util::array_list_elements(vm, list);
            let mut found = false;
            for (index, existing) in elements.iter().enumerate() {
                if string_field(vm, *existing, "name") != name {
                    continue;
                }
                found = true;
                let old_mask = crate::lang::read_int_field(vm, *existing, "mask");
                let new_mask = crate::lang::read_int_field(vm, permission, "mask");
                if old_mask != new_mask {
                    let effective = old_mask | new_mask;
                    if effective == new_mask {
                        list_set(vm, list, index, permission)?;
                    } else if effective != old_mask {
                        let merged = file_permission_with_mask(vm, permission, effective)?;
                        list_set(vm, list, index, merged)?;
                    }
                }
                break;
            }
            if !found {
                list_add(vm, list, permission)?;
            }
            void()
        },
    ),
    method(
        "implies",
        "(Ljava/security/Permission;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let permission = ref_arg(args, 1);
            let file_permission = vm.resolve_class("java/io/FilePermission")?;
            if permission.is_null() || !vm.is_instance(permission, file_permission) {
                return boolean(false);
            }
            let desired = crate::lang::read_int_field(vm, permission, "mask");
            let mut effective = 0;
            let mut needed = desired;
            let list = ref_field(vm, this, "perms", "Ljava/util/ArrayList;");
            for existing in crate::util::array_list_elements(vm, list) {
                let mask = crate::lang::read_int_field(vm, existing, "mask");
                if (needed & mask) != 0 && file_implies_ignore_mask(vm, existing, permission) {
                    effective |= mask;
                    if (effective & desired) == desired {
                        return boolean(true);
                    }
                    needed = desired & !effective;
                }
            }
            boolean(false)
        },
    ),
    method(
        "elements",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let list = ref_field(vm, receiver(args), "perms", "Ljava/util/ArrayList;");
            let perms = if list.is_null() {
                Vec::new()
            } else {
                crate::util::array_list_elements(vm, list)
            };
            object(crate::util::new_enumeration(vm, &perms)?)
        },
    ),
];

// -------------------------------------------------------------------------------------------
// java.security.UnresolvedPermission
// -------------------------------------------------------------------------------------------

fn clone_cert_array(vm: &mut Vm<'_>, certs: ObjectRef) -> Result<ObjectRef, VmError> {
    let length = vm.array_length(certs)?;
    let cert_class = vm.resolve_class("java/security/cert/Certificate")?;
    let copy = vm.allocate_object_array(cert_class, length)?;
    for index in 0..length {
        let value = vm.array_get(certs, index as i32)?;
        vm.array_set_ref(copy, index, value.as_ref())?;
    }
    Ok(copy)
}

fn clone_signer_array(vm: &mut Vm<'_>, signers: ObjectRef) -> Result<ObjectRef, VmError> {
    let length = vm.array_length(signers)?;
    let signer_class = vm.resolve_class("java/security/CodeSigner")?;
    let copy = vm.allocate_object_array(signer_class, length)?;
    for index in 0..length {
        let value = vm.array_get(signers, index as i32)?;
        vm.array_set_ref(copy, index, value.as_ref())?;
    }
    Ok(copy)
}

fn ref_equals(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<bool, VmError> {
    if a == b {
        return Ok(true);
    }
    if a.is_null() || b.is_null() {
        return Ok(false);
    }
    let (declaring, equals) = vm.resolve_virtual_method(a, "equals", "(Ljava/lang/Object;)Z")?;
    let value = vm.invoke_method(declaring, equals, vec![Value::Ref(a), Value::Ref(b)])?;
    Ok(value.as_boolean())
}

/// Certificate arrays compare as sets: every cert on each side must equal
/// some cert on the other side.
fn cert_arrays_equal(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<bool, VmError> {
    if a.is_null() && b.is_null() {
        return Ok(true);
    }
    if a.is_null() || b.is_null() {
        return Ok(false);
    }
    let len_a = vm.array_length(a)?;
    let len_b = vm.array_length(b)?;
    if len_a != len_b {
        return Ok(false);
    }
    for i in 0..len_a {
        let ca = vm.array_get(a, i as i32)?.as_ref();
        let mut found = false;
        for j in 0..len_b {
            let cb = vm.array_get(b, j as i32)?.as_ref();
            if ref_equals(vm, ca, cb)? {
                found = true;
                break;
            }
        }
        if !found {
            return Ok(false);
        }
    }
    for j in 0..len_b {
        let cb = vm.array_get(b, j as i32)?.as_ref();
        let mut found = false;
        for i in 0..len_a {
            let ca = vm.array_get(a, i as i32)?.as_ref();
            if ref_equals(vm, cb, ca)? {
                found = true;
                break;
            }
        }
        if !found {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(crate) const UNRESOLVED_PERMISSION: oxjvm_vm::NativeClass = class(
    "java/security/UnresolvedPermission",
    Some("java/security/Permission"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("type", "Ljava/lang/String;", ACC_PRIVATE, None),
        // The JDK field is called "name" too, but it must not shadow the
        // Permission.name field that holds the type; getName() inherits it.
        field("uname", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("actions", "Ljava/lang/String;", ACC_PRIVATE, None),
        field(
            "certs",
            "[Ljava/security/cert/Certificate;",
            ACC_PRIVATE,
            None,
        ),
    ],
    &UNRESOLVED_PERMISSION_METHODS,
    None,
);

const UNRESOLVED_PERMISSION_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;[Ljava/security/cert/Certificate;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let parent = vm.resolve_class("java/security/Permission")?;
            let (declaring, ctor) = vm
                .find_method(parent, "<init>", "(Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("Permission lacks <init>"))?;
            vm.invoke_method(declaring, ctor, vec![args[0], args[1]])?;
            let type_ref = ref_arg(args, 1);
            if type_ref.is_null() {
                return Err(
                    vm.throw_new("java/lang/NullPointerException", Some("type can't be null"))
                );
            }
            set_ref_field(vm, this, "type", "Ljava/lang/String;", type_ref)?;
            set_ref_field(vm, this, "uname", "Ljava/lang/String;", ref_arg(args, 2))?;
            set_ref_field(vm, this, "actions", "Ljava/lang/String;", ref_arg(args, 3))?;
            let certs = ref_arg(args, 4);
            if !certs.is_null() {
                let copy = clone_cert_array(vm, certs)?;
                set_ref_field(vm, this, "certs", "[Ljava/security/cert/Certificate;", copy)?;
            }
            void()
        },
    ),
    method(
        "getActions",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("")?),
    ),
    method(
        "getUnresolvedType",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "type", "Ljava/lang/String;")),
    ),
    method(
        "getUnresolvedName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "uname", "Ljava/lang/String;")),
    ),
    method(
        "getUnresolvedActions",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "actions",
                "Ljava/lang/String;",
            ))
        },
    ),
    method(
        "getUnresolvedCerts",
        "()[Ljava/security/cert/Certificate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let certs = ref_field(
                vm,
                receiver(args),
                "certs",
                "[Ljava/security/cert/Certificate;",
            );
            if certs.is_null() {
                return object(ObjectRef::NULL);
            }
            object(clone_cert_array(vm, certs)?)
        },
    ),
    method(
        "implies",
        "(Ljava/security/Permission;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(false),
    ),
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
            let unresolved = vm.resolve_class("java/security/UnresolvedPermission")?;
            if other.is_null() || !vm.is_instance(other, unresolved) {
                return boolean(false);
            }
            if string_field(vm, this, "type") != string_field(vm, other, "type") {
                return boolean(false);
            }
            if opt_string_field(vm, this, "uname") != opt_string_field(vm, other, "uname") {
                return boolean(false);
            }
            if opt_string_field(vm, this, "actions") != opt_string_field(vm, other, "actions") {
                return boolean(false);
            }
            let these_certs = ref_field(vm, this, "certs", "[Ljava/security/cert/Certificate;");
            let other_certs = ref_field(vm, other, "certs", "[Ljava/security/cert/Certificate;");
            boolean(cert_arrays_equal(vm, these_certs, other_certs)?)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let hash_text = |text: &str| {
            let units: Vec<u16> = text.encode_utf16().collect();
            crate::lang::java_string_hash(&units)
        };
        let type_hash = hash_text(&string_field(vm, this, "type"));
        let name_hash = opt_string_field(vm, this, "uname")
            .map(|text| hash_text(&text))
            .unwrap_or(0);
        let actions_hash = opt_string_field(vm, this, "actions")
            .map(|text| hash_text(&text))
            .unwrap_or(0);
        let mut result = 1i32;
        for element in [type_hash, name_hash, actions_hash] {
            result = result.wrapping_mul(31).wrapping_add(element);
        }
        int(result)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let type_text = string_field(vm, this, "type");
            let name_text =
                opt_string_field(vm, this, "uname").unwrap_or_else(|| String::from("null"));
            let actions_text =
                opt_string_field(vm, this, "actions").unwrap_or_else(|| String::from("null"));
            object(vm.make_string(&alloc::format!(
                "(unresolved {type_text} {name_text} {actions_text})"
            ))?)
        },
    ),
    method(
        "newPermissionCollection",
        "()Ljava/security/PermissionCollection;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/security/UnresolvedPermissionCollection")?;
            let instance = vm.new_instance(class)?;
            let (declaring, ctor) = vm
                .find_method(class, "<init>", "()V")
                .ok_or_else(|| VmError::internal("UnresolvedPermissionCollection lacks <init>"))?;
            vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
            object(instance)
        },
    ),
];

pub(crate) const UNRESOLVED_PERMISSION_COLLECTION: oxjvm_vm::NativeClass = class(
    "java/security/UnresolvedPermissionCollection",
    Some("java/security/PermissionCollection"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[field("perms", "Ljava/util/ArrayList;", ACC_PRIVATE, None)],
    &UNRESOLVED_PERMISSION_COLLECTION_METHODS,
    None,
);

const UNRESOLVED_PERMISSION_COLLECTION_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let list = new_array_list(vm)?;
        set_ref_field(vm, this, "perms", "Ljava/util/ArrayList;", list)?;
        void()
    }),
    method(
        "add",
        "(Ljava/security/Permission;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let permission = ref_arg(args, 1);
            let unresolved = vm.resolve_class("java/security/UnresolvedPermission")?;
            if permission.is_null() || !vm.is_instance(permission, unresolved) {
                let display = permission_display(vm, permission)?;
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some(&alloc::format!("invalid permission: {display}")),
                ));
            }
            // The JDK's UnresolvedPermissionCollection.add has no readonly
            // check, unlike the other collections.
            let list = ref_field(vm, this, "perms", "Ljava/util/ArrayList;");
            list_add(vm, list, permission)?;
            void()
        },
    ),
    method(
        "implies",
        "(Ljava/security/Permission;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(false),
    ),
    method(
        "elements",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let list = ref_field(vm, receiver(args), "perms", "Ljava/util/ArrayList;");
            let perms = if list.is_null() {
                Vec::new()
            } else {
                crate::util::array_list_elements(vm, list)
            };
            object(crate::util::new_enumeration(vm, &perms)?)
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

pub(crate) const SECURITY_PERMISSION: oxjvm_vm::NativeClass = class(
    "java/security/SecurityPermission",
    Some("java/security/BasicPermission"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &SECURITY_PERMISSION_METHODS,
    None,
);

const SECURITY_PERMISSION_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let parent = vm.resolve_class("java/security/BasicPermission")?;
            let (declaring, ctor) = vm
                .find_method(parent, "<init>", "(Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("BasicPermission lacks <init>"))?;
            vm.invoke_method(declaring, ctor, args.to_vec())?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let parent = vm.resolve_class("java/security/BasicPermission")?;
            let (declaring, ctor) = vm
                .find_method(parent, "<init>", "(Ljava/lang/String;Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("BasicPermission lacks <init>"))?;
            vm.invoke_method(declaring, ctor, args.to_vec())?;
            void()
        },
    ),
];

pub(crate) const RUNTIME_PERMISSION: oxjvm_vm::NativeClass = class(
    "java/lang/RuntimePermission",
    Some("java/security/BasicPermission"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &RUNTIME_PERMISSION_METHODS,
    None,
);

const RUNTIME_PERMISSION_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let parent = vm.resolve_class("java/security/BasicPermission")?;
            let (declaring, ctor) = vm
                .find_method(parent, "<init>", "(Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("BasicPermission lacks <init>"))?;
            vm.invoke_method(declaring, ctor, args.to_vec())?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let parent = vm.resolve_class("java/security/BasicPermission")?;
            let (declaring, ctor) = vm
                .find_method(parent, "<init>", "(Ljava/lang/String;Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("BasicPermission lacks <init>"))?;
            vm.invoke_method(declaring, ctor, args.to_vec())?;
            void()
        },
    ),
];

pub(crate) const NET_PERMISSION: oxjvm_vm::NativeClass = class(
    "java/net/NetPermission",
    Some("java/security/BasicPermission"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &NET_PERMISSION_METHODS,
    None,
);

const NET_PERMISSION_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let parent = vm.resolve_class("java/security/BasicPermission")?;
            let (declaring, ctor) = vm
                .find_method(parent, "<init>", "(Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("BasicPermission lacks <init>"))?;
            vm.invoke_method(declaring, ctor, args.to_vec())?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let parent = vm.resolve_class("java/security/BasicPermission")?;
            let (declaring, ctor) = vm
                .find_method(parent, "<init>", "(Ljava/lang/String;Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("BasicPermission lacks <init>"))?;
            vm.invoke_method(declaring, ctor, args.to_vec())?;
            void()
        },
    ),
];

/// Parse PropertyPermission actions into a READ(1)/WRITE(2) mask, mirroring the
/// JDK's backwards scan: words are matched from the end, separated by commas
/// with optional surrounding whitespace. null/empty yield mask 0 (rejected by
/// the constructor); anything else invalid throws IllegalArgumentException.
fn property_actions_mask(vm: &mut Vm<'_>, actions: ObjectRef) -> Result<i32, VmError> {
    if actions.is_null() {
        return Ok(0);
    }
    let text = vm.string_value(actions).unwrap_or_default();
    let invalid = alloc::format!("invalid permission: {text}");
    let chars: Vec<char> = text.chars().collect();
    let is_ws = |c: char| c == ' ' || c == '\r' || c == '\n' || c == '\u{000C}' || c == '\t';
    let mut i = chars.len() as isize - 1;
    if i < 0 {
        return Ok(0);
    }
    let mut mask = 0i32;
    while i != -1 {
        while i != -1 && is_ws(chars[i as usize]) {
            i -= 1;
        }
        let u = i as usize;
        let matchlen: isize;
        if i >= 3
            && chars[u - 3].eq_ignore_ascii_case(&'r')
            && chars[u - 2].eq_ignore_ascii_case(&'e')
            && chars[u - 1].eq_ignore_ascii_case(&'a')
            && chars[u].eq_ignore_ascii_case(&'d')
        {
            matchlen = 4;
            mask |= 1;
        } else if i >= 4
            && chars[u - 4].eq_ignore_ascii_case(&'w')
            && chars[u - 3].eq_ignore_ascii_case(&'r')
            && chars[u - 2].eq_ignore_ascii_case(&'i')
            && chars[u - 1].eq_ignore_ascii_case(&'t')
            && chars[u].eq_ignore_ascii_case(&'e')
        {
            matchlen = 5;
            mask |= 2;
        } else {
            return Err(vm.throw_new("java/lang/IllegalArgumentException", Some(&invalid)));
        }
        let mut seen_comma = false;
        while i >= matchlen && !seen_comma {
            let c = chars[(i - matchlen) as usize];
            if c == ',' {
                seen_comma = true;
            } else if !is_ws(c) {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", Some(&invalid)));
            }
            i -= 1;
        }
        i -= matchlen;
    }
    Ok(mask)
}

pub(crate) const PROPERTY_PERMISSION: oxjvm_vm::NativeClass = class(
    "java/util/PropertyPermission",
    Some("java/security/BasicPermission"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[field("mask", "I", ACC_PRIVATE, None)],
    &PROPERTY_PERMISSION_METHODS,
    None,
);

const PROPERTY_PERMISSION_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let parent = vm.resolve_class("java/security/BasicPermission")?;
            let (declaring, ctor) = vm
                .find_method(parent, "<init>", "(Ljava/lang/String;Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("BasicPermission lacks <init>"))?;
            vm.invoke_method(declaring, ctor, args.to_vec())?;
            let mask = property_actions_mask(vm, ref_arg(args, 2))?;
            if mask == 0 {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("invalid actions mask"),
                ));
            }
            crate::lang::write_int_field(vm, this, "mask", mask);
            void()
        },
    ),
    method(
        "getActions",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let mask = crate::lang::read_int_field(vm, receiver(args), "mask");
            let text = match mask & 3 {
                1 => "read",
                2 => "write",
                3 => "read,write",
                _ => "",
            };
            object(vm.make_string(text)?)
        },
    ),
    method(
        "implies",
        "(Ljava/security/Permission;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let prop = vm.resolve_class("java/util/PropertyPermission")?;
            if other.is_null() || !vm.is_instance(other, prop) {
                return boolean(false);
            }
            let this_mask = crate::lang::read_int_field(vm, this, "mask");
            let other_mask = crate::lang::read_int_field(vm, other, "mask");
            if (this_mask & other_mask) != other_mask {
                return boolean(false);
            }
            boolean(basic_name_implies(
                &string_field(vm, this, "name"),
                &string_field(vm, other, "name"),
            ))
        },
    ),
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
            let prop = vm.resolve_class("java/util/PropertyPermission")?;
            if other.is_null() || !vm.is_instance(other, prop) {
                return boolean(false);
            }
            let this_mask = crate::lang::read_int_field(vm, this, "mask");
            let other_mask = crate::lang::read_int_field(vm, other, "mask");
            boolean(
                this_mask == other_mask
                    && string_field(vm, this, "name") == string_field(vm, other, "name"),
            )
        },
    ),
    method(
        "newPermissionCollection",
        "()Ljava/security/PermissionCollection;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/PropertyPermissionCollection")?;
            let instance = vm.new_instance(class)?;
            let (declaring, ctor) = vm
                .find_method(class, "<init>", "()V")
                .ok_or_else(|| VmError::internal("PropertyPermissionCollection lacks <init>"))?;
            vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
            object(instance)
        },
    ),
];

pub(crate) const SSL_PERMISSION: oxjvm_vm::NativeClass = class(
    "javax/net/ssl/SSLPermission",
    Some("java/security/BasicPermission"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &SSL_PERMISSION_METHODS,
    None,
);

const SSL_PERMISSION_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let parent = vm.resolve_class("java/security/BasicPermission")?;
            let (declaring, ctor) = vm
                .find_method(parent, "<init>", "(Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("BasicPermission lacks <init>"))?;
            vm.invoke_method(declaring, ctor, args.to_vec())?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let parent = vm.resolve_class("java/security/BasicPermission")?;
            let (declaring, ctor) = vm
                .find_method(parent, "<init>", "(Ljava/lang/String;Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("BasicPermission lacks <init>"))?;
            vm.invoke_method(declaring, ctor, args.to_vec())?;
            void()
        },
    ),
];

pub(crate) const SQL_PERMISSION: oxjvm_vm::NativeClass = class(
    "java/sql/SQLPermission",
    Some("java/security/BasicPermission"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &SQL_PERMISSION_METHODS,
    None,
);

const SQL_PERMISSION_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let parent = vm.resolve_class("java/security/BasicPermission")?;
            let (declaring, ctor) = vm
                .find_method(parent, "<init>", "(Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("BasicPermission lacks <init>"))?;
            vm.invoke_method(declaring, ctor, args.to_vec())?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let parent = vm.resolve_class("java/security/BasicPermission")?;
            let (declaring, ctor) = vm
                .find_method(parent, "<init>", "(Ljava/lang/String;Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("BasicPermission lacks <init>"))?;
            vm.invoke_method(declaring, ctor, args.to_vec())?;
            void()
        },
    ),
];

/// Compute the wildcard flag and match path for a BasicPermission name,
/// mirroring the JDK's private init(): a trailing "*" (alone or after ".")
/// makes a wildcard whose path drops the "*"; the legacy bare "exitVM"
/// behaves like "exitVM.*".
fn basic_permission_path(name: &str) -> (bool, String) {
    if name == "*" {
        (true, String::new())
    } else if let Some(prefix) = name.strip_suffix(".*") {
        (true, alloc::format!("{prefix}."))
    } else if name == "exitVM" {
        (true, String::from("exitVM."))
    } else {
        (false, String::from(name))
    }
}

/// The key a BasicPermissionCollection stores a permission under: the name
/// itself, except the legacy bare "exitVM" canonicalizes to "exitVM.*".
fn basic_permission_canonical(name: &str) -> String {
    if name == "exitVM" {
        String::from("exitVM.*")
    } else {
        String::from(name)
    }
}

/// BasicPermission name implication (the caller checks class identity).
fn basic_name_implies(this_name: &str, other_name: &str) -> bool {
    let (this_wild, this_path) = basic_permission_path(this_name);
    let (other_wild, other_path) = basic_permission_path(other_name);
    if this_wild {
        if other_wild {
            other_path.starts_with(&this_path)
        } else {
            other_path.chars().count() > this_path.chars().count()
                && other_path.starts_with(&this_path)
        }
    } else if other_wild {
        false
    } else {
        this_path == other_path
    }
}

const BASIC_PERMISSION_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
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
            if vm.string_utf16(name).is_none_or(|units| units.is_empty()) {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("name can't be empty"),
                ));
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
                return Err(
                    vm.throw_new("java/lang/NullPointerException", Some("name can't be null"))
                );
            }
            if vm.string_utf16(name).is_none_or(|units| units.is_empty()) {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("name can't be empty"),
                ));
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
            boolean(basic_name_implies(
                &string_field(vm, this, "name"),
                &string_field(vm, other, "name"),
            ))
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
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let name = ref_field(vm, receiver(args), "name", "Ljava/lang/String;");
        int(vm
            .string_utf16(name)
            .map_or(0, crate::lang::java_string_hash))
    }),
    method(
        "newPermissionCollection",
        "()Ljava/security/PermissionCollection;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.resolve_class("java/security/BasicPermissionCollection")?;
            let instance = vm.new_instance(class)?;
            let (declaring, ctor) = vm
                .find_method(class, "<init>", "()V")
                .ok_or_else(|| VmError::internal("BasicPermissionCollection() missing"))?;
            vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
            let perm_class = String::from(vm.class_name(vm.class_of(this)));
            let class_name = vm.make_string(&perm_class)?;
            set_ref_field(
                vm,
                instance,
                "permClassName",
                "Ljava/lang/String;",
                class_name,
            )?;
            object(instance)
        },
    ),
];

/// The ArrayList backing a BasicPermissionCollection.
fn basic_collection_list(vm: &mut Vm<'_>, this: ObjectRef) -> Result<ObjectRef, VmError> {
    let class = vm.class_of(this);
    Ok(vm
        .read_ref_field(this, class, "perms", "Ljava/util/ArrayList;")
        .unwrap_or(ObjectRef::NULL))
}

/// If a stored permission canonicalizes to `key`, invoke its implies on
/// `other` and return the result; otherwise return None.
fn basic_collection_lookup_implies(
    vm: &mut Vm<'_>,
    perms: &[ObjectRef],
    key: &str,
    other: ObjectRef,
) -> Result<Option<bool>, VmError> {
    for perm in perms {
        if perm.is_null() {
            continue;
        }
        let name = vm.read_string_field_any(*perm, "name").unwrap_or_default();
        if basic_permission_canonical(&name) == key {
            let (declaring, implies) =
                vm.resolve_virtual_method(*perm, "implies", "(Ljava/security/Permission;)Z")?;
            let result = vm.invoke_method(
                declaring,
                implies,
                vec![Value::Ref(*perm), Value::Ref(other)],
            )?;
            return Ok(Some(result.as_boolean()));
        }
    }
    Ok(None)
}

/// BasicPermissionCollection.add: only BasicPermissions of a single class are
/// accepted; entries are keyed by canonical name so a later grant for the same
/// key replaces the earlier one, and a "*" name marks the collection all-allowed.
fn basic_collection_add(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let perm = ref_arg(args, 1);
    let basic = vm.resolve_class("java/security/BasicPermission")?;
    if perm.is_null() || !vm.is_instance(perm, basic) {
        let display = permission_display(vm, perm)?;
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some(&alloc::format!("invalid permission: {display}")),
        ));
    }
    ensure_collection_writable(vm, receiver(args))?;
    let this = receiver(args);
    let perm_class = String::from(vm.class_name(vm.class_of(perm)));
    let stored_class = vm
        .read_string_field_any(this, "permClassName")
        .unwrap_or_default();
    if !stored_class.is_empty() && stored_class != perm_class {
        let display = permission_display(vm, perm)?;
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some(&alloc::format!("invalid permission: {display}")),
        ));
    }
    if stored_class.is_empty() {
        let class_name = vm.make_string(&perm_class)?;
        set_ref_field(vm, this, "permClassName", "Ljava/lang/String;", class_name)?;
    }
    let name = vm.read_string_field_any(perm, "name").unwrap_or_default();
    let canonical = basic_permission_canonical(&name);
    let list = basic_collection_list(vm, this)?;
    let perms = crate::util::array_list_elements(vm, list);
    let mut existing = None;
    for (index, stored) in perms.iter().enumerate() {
        if stored.is_null() {
            continue;
        }
        let stored_name = vm
            .read_string_field_any(*stored, "name")
            .unwrap_or_default();
        if basic_permission_canonical(&stored_name) == canonical {
            existing = Some(index);
            break;
        }
    }
    match existing {
        Some(index) => {
            let (declaring, set) =
                vm.resolve_virtual_method(list, "set", "(ILjava/lang/Object;)Ljava/lang/Object;")?;
            vm.invoke_method(
                declaring,
                set,
                vec![Value::Ref(list), int(index as i32)?, Value::Ref(perm)],
            )?;
        }
        None => {
            list_add(vm, list, perm)?;
        }
    }
    if canonical == "*" {
        crate::lang::write_int_field(vm, this, "allAllowed", 1);
    }
    void()
}

/// BasicPermissionCollection.implies: same-class only; "*" grants everything;
/// otherwise a direct canonical-name hit, then walking up the dotted path
/// looking for "a.b.*" style wildcard grants.
fn basic_collection_implies(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let other = ref_arg(args, 1);
    let basic = vm.resolve_class("java/security/BasicPermission")?;
    if other.is_null() || !vm.is_instance(other, basic) {
        return boolean(false);
    }
    let this = receiver(args);
    let stored_class = vm
        .read_string_field_any(this, "permClassName")
        .unwrap_or_default();
    let other_class = String::from(vm.class_name(vm.class_of(other)));
    if stored_class != other_class {
        return boolean(false);
    }
    if crate::lang::read_int_field(vm, this, "allAllowed") != 0 {
        return boolean(true);
    }
    let other_name = vm.read_string_field_any(other, "name").unwrap_or_default();
    let canonical = basic_permission_canonical(&other_name);
    let list = basic_collection_list(vm, this)?;
    let perms = crate::util::array_list_elements(vm, list);
    if let Some(result) = basic_collection_lookup_implies(vm, &perms, &canonical, other)? {
        return boolean(result);
    }
    let mut offset = canonical.len();
    while let Some(last) = canonical[..offset].rfind('.') {
        let key = alloc::format!("{}*", &canonical[..last + 1]);
        if let Some(result) = basic_collection_lookup_implies(vm, &perms, &key, other)? {
            return boolean(result);
        }
        offset = last;
    }
    boolean(false)
}

pub(crate) const BASIC_PERMISSION_COLLECTION: oxjvm_vm::NativeClass = class(
    "java/security/BasicPermissionCollection",
    Some("java/security/PermissionCollection"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("perms", "Ljava/util/ArrayList;", ACC_PRIVATE, None),
        field("permClassName", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("allAllowed", "I", ACC_PRIVATE, None),
    ],
    &[
        method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let instance = new_array_list(vm)?;
            let (declaring, field) = vm.find_field(class, "perms", "Ljava/util/ArrayList;")?;
            vm.set_instance_ref(this, declaring, field, instance);
            void()
        }),
        method(
            "add",
            "(Ljava/security/Permission;)V",
            ACC_PUBLIC,
            |vm, _, args| basic_collection_add(vm, args),
        ),
        method(
            "implies",
            "(Ljava/security/Permission;)Z",
            ACC_PUBLIC,
            |vm, _, args| basic_collection_implies(vm, args),
        ),
        method(
            "elements",
            "()Ljava/util/Enumeration;",
            ACC_PUBLIC,
            |vm, _, args| {
                let list = basic_collection_list(vm, receiver(args))?;
                let perms = if list.is_null() {
                    Vec::new()
                } else {
                    crate::util::array_list_elements(vm, list)
                };
                object(crate::util::new_enumeration(vm, &perms)?)
            },
        ),
    ],
    None,
);

pub(crate) const PERMISSION_COLLECTION: oxjvm_vm::NativeClass = class(
    "java/security/PermissionCollection",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[field("readOnly", "I", ACC_PRIVATE, None)],
    &PERMISSION_COLLECTION_METHODS,
    None,
);

const PERMISSION_COLLECTION_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "implies",
        "(Ljava/security/Permission;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(false),
    ),
    method(
        "add",
        "(Ljava/security/Permission;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "elements",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC,
        |vm, _, _| object(crate::util::new_enumeration(vm, &[])?),
    ),
    method("isReadOnly", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "readOnly") != 0)
    }),
    method("setReadOnly", "()V", ACC_PUBLIC, |vm, _, args| {
        crate::lang::write_int_field(vm, receiver(args), "readOnly", 1);
        void()
    }),
];

/// Find the homogeneous collection stored for a permission class name.
fn permissions_collection_for(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    class_name: &str,
) -> Result<ObjectRef, VmError> {
    let names = ref_field(vm, this, "classNames", "Ljava/util/ArrayList;");
    let collections = ref_field(vm, this, "perms", "Ljava/util/ArrayList;");
    let name_elements = crate::util::array_list_elements(vm, names);
    let collection_elements = crate::util::array_list_elements(vm, collections);
    for (index, name) in name_elements.iter().enumerate() {
        if vm.string_value(*name).unwrap_or_default() == class_name {
            return Ok(collection_elements[index]);
        }
    }
    Ok(ObjectRef::NULL)
}

pub(crate) const PERMISSIONS: oxjvm_vm::NativeClass = class(
    "java/security/Permissions",
    Some("java/security/PermissionCollection"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("perms", "Ljava/util/ArrayList;", ACC_PRIVATE, None),
        field("classNames", "Ljava/util/ArrayList;", ACC_PRIVATE, None),
        field("allPermission", "I", ACC_PRIVATE, None),
    ],
    &PERMISSIONS_METHODS,
    None,
);

const PERMISSIONS_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let perms = new_array_list(vm)?;
        set_ref_field(vm, this, "perms", "Ljava/util/ArrayList;", perms)?;
        let names = new_array_list(vm)?;
        set_ref_field(vm, this, "classNames", "Ljava/util/ArrayList;", names)?;
        void()
    }),
    method(
        "add",
        "(Ljava/security/Permission;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let permission = ref_arg(args, 1);
            if crate::lang::read_int_field(vm, this, "readOnly") != 0 {
                return Err(vm.throw_new(
                    "java/lang/SecurityException",
                    Some("attempt to add a Permission to a readonly Permissions object"),
                ));
            }
            if permission.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let class_name = String::from(vm.class_name(vm.class_of(permission)));
            let mut collection = permissions_collection_for(vm, this, &class_name)?;
            if collection.is_null() {
                let (declaring, new_collection) = vm.resolve_virtual_method(
                    permission,
                    "newPermissionCollection",
                    "()Ljava/security/PermissionCollection;",
                )?;
                let value =
                    vm.invoke_method(declaring, new_collection, vec![Value::Ref(permission)])?;
                collection = value.as_ref();
                if collection.is_null() {
                    let hash = vm.resolve_class("java/security/PermissionsHash")?;
                    collection = vm.new_instance(hash)?;
                    let (declaring, ctor) = vm
                        .find_method(hash, "<init>", "()V")
                        .ok_or_else(|| VmError::internal("PermissionsHash lacks <init>"))?;
                    vm.invoke_method(declaring, ctor, vec![Value::Ref(collection)])?;
                }
                let names = ref_field(vm, this, "classNames", "Ljava/util/ArrayList;");
                let perms = ref_field(vm, this, "perms", "Ljava/util/ArrayList;");
                let name_value = vm.make_string(&class_name)?;
                list_add(vm, names, name_value)?;
                list_add(vm, perms, collection)?;
            }
            let (declaring, add) =
                vm.resolve_virtual_method(collection, "add", "(Ljava/security/Permission;)V")?;
            vm.invoke_method(
                declaring,
                add,
                vec![Value::Ref(collection), Value::Ref(permission)],
            )?;
            let all = vm.resolve_class("java/security/AllPermission")?;
            if vm.is_instance(permission, all) {
                crate::lang::write_int_field(vm, this, "allPermission", 1);
            }
            void()
        },
    ),
    method(
        "implies",
        "(Ljava/security/Permission;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if crate::lang::read_int_field(vm, this, "allPermission") != 0 {
                return boolean(true);
            }
            let permission = ref_arg(args, 1);
            if permission.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let class_name = String::from(vm.class_name(vm.class_of(permission)));
            let collection = permissions_collection_for(vm, this, &class_name)?;
            if collection.is_null() {
                return boolean(false);
            }
            let (declaring, implies) =
                vm.resolve_virtual_method(collection, "implies", "(Ljava/security/Permission;)Z")?;
            let value = vm.invoke_method(
                declaring,
                implies,
                vec![Value::Ref(collection), Value::Ref(permission)],
            )?;
            boolean(value.as_int() != 0)
        },
    ),
    method(
        "elements",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let perms = ref_field(vm, receiver(args), "perms", "Ljava/util/ArrayList;");
            let mut all = Vec::new();
            for collection in crate::util::array_list_elements(vm, perms) {
                let (declaring, elements) =
                    vm.resolve_virtual_method(collection, "elements", "()Ljava/util/Enumeration;")?;
                let value = vm.invoke_method(declaring, elements, vec![Value::Ref(collection)])?;
                all.extend(crate::util::enumeration_elements(vm, value.as_ref())?);
            }
            object(crate::util::new_enumeration(vm, &all)?)
        },
    ),
];

/// Heterogeneous fallback collection used by Permissions when a permission
/// class does not provide its own newPermissionCollection().
pub(crate) const PERMISSIONS_HASH: oxjvm_vm::NativeClass = class(
    "java/security/PermissionsHash",
    Some("java/security/PermissionCollection"),
    &[],
    ACC_FINAL,
    &[field("perms", "Ljava/util/ArrayList;", ACC_PRIVATE, None)],
    &PERMISSIONS_HASH_METHODS,
    None,
);

const PERMISSIONS_HASH_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let list = new_array_list(vm)?;
        set_ref_field(vm, this, "perms", "Ljava/util/ArrayList;", list)?;
        void()
    }),
    method(
        "add",
        "(Ljava/security/Permission;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let permission = ref_arg(args, 1);
            let list = ref_field(vm, this, "perms", "Ljava/util/ArrayList;");
            let elements = crate::util::array_list_elements(vm, list);
            for (index, existing) in elements.iter().enumerate() {
                if ref_equals(vm, *existing, permission)? {
                    list_set(vm, list, index, permission)?;
                    return void();
                }
            }
            list_add(vm, list, permission)?;
            void()
        },
    ),
    method(
        "implies",
        "(Ljava/security/Permission;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let list = ref_field(vm, receiver(args), "perms", "Ljava/util/ArrayList;");
            let permission = ref_arg(args, 1);
            for existing in crate::util::array_list_elements(vm, list) {
                let (declaring, implies) = vm.resolve_virtual_method(
                    existing,
                    "implies",
                    "(Ljava/security/Permission;)Z",
                )?;
                let value = vm.invoke_method(
                    declaring,
                    implies,
                    vec![Value::Ref(existing), Value::Ref(permission)],
                )?;
                if value.as_int() != 0 {
                    return boolean(true);
                }
            }
            boolean(false)
        },
    ),
    method(
        "elements",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let list = ref_field(vm, receiver(args), "perms", "Ljava/util/ArrayList;");
            let perms = if list.is_null() {
                Vec::new()
            } else {
                crate::util::array_list_elements(vm, list)
            };
            object(crate::util::new_enumeration(vm, &perms)?)
        },
    ),
];

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

/// Shared `Cipher.init`: record the mode and key, reset any previous IV, and capture the IV
/// from an `IvParameterSpec`/`GCMParameterSpec` when one is supplied.
fn cipher_init(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    mode: i32,
    key: ObjectRef,
    spec: ObjectRef,
) -> Result<(), VmError> {
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "mode", "I")?;
    vm.set_instance_int(this, declaring, field, mode);
    set_ref_field(vm, this, "key", "Ljava/security/Key;", key)?;
    set_ref_field(vm, this, "iv", "[B", ObjectRef::NULL)?;
    if !spec.is_null() {
        if let Ok((declaring, method)) = vm.resolve_virtual_method(spec, "getIV", "()[B") {
            let iv = vm
                .invoke_method(declaring, method, vec![Value::Ref(spec)])?
                .as_ref();
            set_ref_field(vm, this, "iv", "[B", iv)?;
        }
    }
    Ok(())
}

/// Copy `inputLen` bytes from `input[inputOffset..]` into `output[outputOffset..]` and return
/// the count. The Cipher shell passes data through unchanged, so the in-place update/doFinal
/// overloads simply move the bytes into the caller's output buffer.
fn cipher_copy_to_output(
    vm: &mut Vm<'_>,
    args: &[Value],
    output_offset: i32,
) -> Result<Value, VmError> {
    let source = ref_arg(args, 1);
    let output = ref_arg(args, 4);
    if source.is_null() || output.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let input_offset = int_arg(args, 2);
    let length = int_arg(args, 3);
    let capacity = vm.array_length(source)? as i32;
    if input_offset < 0 || length < 0 || input_offset + length > capacity {
        return Err(vm.throw_new("java/lang/ArrayIndexOutOfBoundsException", None));
    }
    let out_capacity = vm.array_length(output)? as i32;
    if output_offset < 0 || output_offset + length > out_capacity {
        return Err(vm.throw_new(
            "javax/crypto/ShortBufferException",
            Some("output buffer too small"),
        ));
    }
    for index in 0..length {
        let byte = vm.array_get(source, input_offset + index)?;
        vm.array_set(output, output_offset + index, byte, false)?;
    }
    int(length)
}

const CIPHER_METHODS: [oxjvm_vm::NativeMethodDef; 30] = [
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
    method("getIV", "()[B", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let iv = ref_field(vm, this, "iv", "[B");
        if !iv.is_null() {
            return object(iv);
        }
        // ECB mode has no IV; every other mode gets a fresh random 16-byte IV.
        let transformation = string_field(vm, this, "transformation").to_ascii_uppercase();
        let mode = transformation.split('/').nth(1).unwrap_or("ECB");
        if mode == "ECB" {
            return object(ObjectRef::NULL);
        }
        let bytes = random_bytes(vm, 16)?;
        let array = new_byte_array(vm, &bytes)?;
        set_ref_field(vm, this, "iv", "[B", array)?;
        object(array)
    }),
    method(
        "getParameters",
        "()Ljava/security/AlgorithmParameters;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let transformation = string_field(vm, this, "transformation");
            let algorithm = String::from(transformation.split('/').next().unwrap_or(""));
            let params = algorithm_parameters_instance(vm, &algorithm)?;
            let iv = ref_field(vm, this, "iv", "[B");
            if !iv.is_null() {
                let spec = new_iv_parameter_spec(vm, iv)?;
                set_ref_field(
                    vm,
                    params,
                    "spec",
                    "Ljava/security/spec/AlgorithmParameterSpec;",
                    spec,
                )?;
            }
            object(params)
        },
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
            cipher_init(
                vm,
                receiver(args),
                int_arg(args, 1),
                ref_arg(args, 2),
                ObjectRef::NULL,
            )?;
            void()
        },
    ),
    method(
        "init",
        "(ILjava/security/Key;Ljava/security/SecureRandom;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            cipher_init(
                vm,
                receiver(args),
                int_arg(args, 1),
                ref_arg(args, 2),
                ObjectRef::NULL,
            )?;
            void()
        },
    ),
    method(
        "init",
        "(ILjava/security/Key;Ljava/security/spec/AlgorithmParameterSpec;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            cipher_init(
                vm,
                receiver(args),
                int_arg(args, 1),
                ref_arg(args, 2),
                ref_arg(args, 3),
            )?;
            void()
        },
    ),
    method(
        "init",
        "(ILjava/security/Key;Ljava/security/spec/AlgorithmParameterSpec;Ljava/security/SecureRandom;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            cipher_init(
                vm,
                receiver(args),
                int_arg(args, 1),
                ref_arg(args, 2),
                ref_arg(args, 3),
            )?;
            void()
        },
    ),
    method(
        "init",
        "(ILjava/security/Key;Ljava/security/AlgorithmParameters;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let params = ref_arg(args, 3);
            let spec = if params.is_null() {
                ObjectRef::NULL
            } else {
                vm.read_ref_field_any(params, "spec")
                    .unwrap_or(ObjectRef::NULL)
            };
            cipher_init(vm, receiver(args), int_arg(args, 1), ref_arg(args, 2), spec)?;
            void()
        },
    ),
    method(
        "init",
        "(ILjava/security/Key;Ljava/security/AlgorithmParameters;Ljava/security/SecureRandom;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let params = ref_arg(args, 3);
            let spec = if params.is_null() {
                ObjectRef::NULL
            } else {
                vm.read_ref_field_any(params, "spec")
                    .unwrap_or(ObjectRef::NULL)
            };
            cipher_init(vm, receiver(args), int_arg(args, 1), ref_arg(args, 2), spec)?;
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
    method("update", "([BII[B)I", ACC_PUBLIC, |vm, _, args| {
        cipher_copy_to_output(vm, args, 0)
    }),
    method("update", "([BII[BI)I", ACC_PUBLIC, |vm, _, args| {
        cipher_copy_to_output(vm, args, int_arg(args, 5))
    }),
    method("doFinal", "([BII[B)I", ACC_PUBLIC, |vm, _, args| {
        cipher_copy_to_output(vm, args, 0)
    }),
    method("doFinal", "([BII[BI)I", ACC_PUBLIC, |vm, _, args| {
        cipher_copy_to_output(vm, args, int_arg(args, 5))
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
        field("iv", "[B", ACC_PRIVATE, None),
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
            let len = vm.array_length(iv)?;
            let copy = byte_slice(vm, iv, 0, len)?;
            set_ref_field(vm, receiver(args), "iv", "[B", copy)?;
            void()
        }),
        method("<init>", "([BII)V", ACC_PUBLIC, |vm, _, args| {
            let this = receiver(args);
            let iv = ref_arg(args, 1);
            let offset = int_arg(args, 2);
            let len = int_arg(args, 3);
            if iv.is_null() {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("IV missing")));
            }
            if offset < 0 {
                return Err(vm.throw_new(
                    "java/lang/ArrayIndexOutOfBoundsException",
                    Some("offset is negative"),
                ));
            }
            if len < 0 {
                return Err(vm.throw_new(
                    "java/lang/ArrayIndexOutOfBoundsException",
                    Some("len is negative"),
                ));
            }
            let total = vm.array_length(iv)? as i32;
            if total - offset < len {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("IV buffer too short for given offset/length combination"),
                ));
            }
            let copy = byte_slice(vm, iv, offset as usize, len as usize)?;
            set_ref_field(vm, this, "iv", "[B", copy)?;
            void()
        }),
        method("getIV", "()[B", ACC_PUBLIC, |vm, _, args| {
            let iv = ref_field(vm, receiver(args), "iv", "[B");
            let len = vm.array_length(iv)?;
            object(byte_slice(vm, iv, 0, len)?)
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
            let t_len = int_arg(args, 1);
            let src = ref_arg(args, 2);
            if src.is_null() {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("src array is null"),
                ));
            }
            let len = vm.array_length(src)? as i32;
            gcm_init(vm, this, t_len, src, 0, len)?;
            void()
        }),
        method("<init>", "(I[BII)V", ACC_PUBLIC, |vm, _, args| {
            let this = receiver(args);
            gcm_init(
                vm,
                this,
                int_arg(args, 1),
                ref_arg(args, 2),
                int_arg(args, 3),
                int_arg(args, 4),
            )?;
            void()
        }),
        method("getTLen", "()I", ACC_PUBLIC, |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "tLen", "I")?;
            int(vm.read_instance_int(this, declaring, field).unwrap_or(0))
        }),
        method("getIV", "()[B", ACC_PUBLIC, |vm, _, args| {
            let src = ref_field(vm, receiver(args), "src", "[B");
            let len = vm.array_length(src)?;
            object(byte_slice(vm, src, 0, len)?)
        }),
    ],
    None,
);

// -------------------------------------------------------------------------------------------
// java.security.AlgorithmParameters
// -------------------------------------------------------------------------------------------

fn algorithm_parameters_instance(vm: &mut Vm<'_>, algorithm: &str) -> Result<ObjectRef, VmError> {
    if algorithm.is_empty() {
        return Err(vm.throw_new(
            "java/security/NoSuchAlgorithmException",
            Some("algorithm must not be empty"),
        ));
    }
    let class = vm.resolve_class("java/security/AlgorithmParameters")?;
    let params = vm.new_instance(class)?;
    set_string_field(vm, params, "algorithm", algorithm)?;
    let provider = new_sun_provider(vm, "SunJCE")?;
    set_ref_field(vm, params, "provider", "Ljava/security/Provider;", provider)?;
    Ok(params)
}

/// Build an `IvParameterSpec` wrapping the given IV array.
fn new_iv_parameter_spec(vm: &mut Vm<'_>, iv: ObjectRef) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("javax/crypto/spec/IvParameterSpec")?;
    let spec = vm.new_instance(class)?;
    set_ref_field(vm, spec, "iv", "[B", iv)?;
    Ok(spec)
}

const ALGORITHM_PARAMETERS_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method(
        "getInstance",
        "(Ljava/lang/String;)Ljava/security/AlgorithmParameters;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(algorithm_parameters_instance(
                vm,
                &vm.string_or_empty(name),
            )?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/security/AlgorithmParameters;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let provider = vm.string_or_empty(ref_arg(args, 1));
            if !known_provider(&provider) {
                return Err(vm.throw_new("java/security/NoSuchProviderException", Some(&provider)));
            }
            object(algorithm_parameters_instance(
                vm,
                &vm.string_or_empty(ref_arg(args, 0)),
            )?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/security/Provider;)Ljava/security/AlgorithmParameters;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(algorithm_parameters_instance(
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
        "init",
        "(Ljava/security/spec/AlgorithmParameterSpec;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let spec = ref_arg(args, 1);
            if spec.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            set_ref_field(
                vm,
                receiver(args),
                "spec",
                "Ljava/security/spec/AlgorithmParameterSpec;",
                spec,
            )?;
            void()
        },
    ),
    method("init", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let encoded = ref_arg(args, 1);
        if encoded.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        let spec = new_iv_parameter_spec(vm, encoded)?;
        set_ref_field(
            vm,
            receiver(args),
            "spec",
            "Ljava/security/spec/AlgorithmParameterSpec;",
            spec,
        )?;
        void()
    }),
    method(
        "init",
        "([BLjava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let encoded = ref_arg(args, 1);
            if encoded.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let spec = new_iv_parameter_spec(vm, encoded)?;
            set_ref_field(
                vm,
                receiver(args),
                "spec",
                "Ljava/security/spec/AlgorithmParameterSpec;",
                spec,
            )?;
            void()
        },
    ),
    method(
        "getParameterSpec",
        "(Ljava/lang/Class;)Ljava/security/spec/AlgorithmParameterSpec;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let requested = crate::lang::class_argument(vm, ref_arg(args, 1))?;
            let name = String::from(vm.class_name(requested));
            let spec = ref_field(
                vm,
                this,
                "spec",
                "Ljava/security/spec/AlgorithmParameterSpec;",
            );
            if spec.is_null() {
                return Err(vm.throw_new(
                    "java/security/spec/InvalidParameterSpecException",
                    Some("Parameters not initialized"),
                ));
            }
            let spec_name = String::from(vm.class_name(vm.class_of(spec)));
            if name == spec_name {
                return object(spec);
            }
            // An IV-bearing spec can always be viewed as a plain IvParameterSpec.
            if name == "javax/crypto/spec/IvParameterSpec" {
                if let Ok((class, method)) = vm.resolve_virtual_method(spec, "getIV", "()[B") {
                    let iv = vm
                        .invoke_method(class, method, vec![Value::Ref(spec)])?
                        .as_ref();
                    return object(new_iv_parameter_spec(vm, iv)?);
                }
            }
            Err(vm.throw_new(
                "java/security/spec/InvalidParameterSpecException",
                Some(&name),
            ))
        },
    ),
    method("getEncoded", "()[B", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let spec = ref_field(
            vm,
            this,
            "spec",
            "Ljava/security/spec/AlgorithmParameterSpec;",
        );
        if spec.is_null() {
            return object(new_byte_array(vm, &[])?);
        }
        let (class, method) = vm.resolve_virtual_method(spec, "getIV", "()[B")?;
        let iv = vm
            .invoke_method(class, method, vec![Value::Ref(spec)])?
            .as_ref();
        object(iv)
    }),
    method(
        "getEncoded",
        "(Ljava/lang/String;)[B",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let spec = ref_field(
                vm,
                this,
                "spec",
                "Ljava/security/spec/AlgorithmParameterSpec;",
            );
            if spec.is_null() {
                return object(new_byte_array(vm, &[])?);
            }
            let (class, method) = vm.resolve_virtual_method(spec, "getIV", "()[B")?;
            let iv = vm
                .invoke_method(class, method, vec![Value::Ref(spec)])?
                .as_ref();
            object(iv)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = alloc::format!(
                "AlgorithmParameters[{}]",
                string_field(vm, receiver(args), "algorithm")
            );
            object(vm.make_string(&text)?)
        },
    ),
];

pub(crate) const ALGORITHM_PARAMETERS: oxjvm_vm::NativeClass = class(
    "java/security/AlgorithmParameters",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("algorithm", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("provider", "Ljava/security/Provider;", ACC_PRIVATE, None),
        field(
            "spec",
            "Ljava/security/spec/AlgorithmParameterSpec;",
            ACC_PRIVATE,
            None,
        ),
    ],
    &ALGORITHM_PARAMETERS_METHODS,
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
    let provider = new_sun_provider(vm, "SunJSSE")?;
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

const SSL_ENGINE_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method("setUseClientMode", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        let (declaring, field) = vm.find_field(class, "clientMode", "I")?;
        vm.set_instance_int(this, declaring, field, int_arg(args, 1));
        void()
    }),
    method("getUseClientMode", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "clientMode") != 0)
    }),
    method("closeInbound", "()V", ACC_PUBLIC, |vm, _, args| {
        crate::lang::write_int_field(vm, receiver(args), "inboundDone", 1);
        void()
    }),
    method("closeOutbound", "()V", ACC_PUBLIC, |vm, _, args| {
        crate::lang::write_int_field(vm, receiver(args), "outboundDone", 1);
        void()
    }),
    method("isInboundDone", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "inboundDone") != 0)
    }),
    method("isOutboundDone", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "outboundDone") != 0)
    }),
];

pub(crate) const SSLE_ENGINE: oxjvm_vm::NativeClass = class(
    "javax/net/ssl/SSLEngine",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[
        field("clientMode", "I", ACC_PRIVATE, None),
        field("inboundDone", "I", ACC_PRIVATE, None),
        field("outboundDone", "I", ACC_PRIVATE, None),
    ],
    &SSL_ENGINE_METHODS,
    None,
);

/// Clone a `String[]` of named groups, rejecting null/blank/duplicate
/// elements with IllegalArgumentException as the JDK does.
fn ssl_named_groups_clone(vm: &mut Vm<'_>, array: ObjectRef) -> Result<ObjectRef, VmError> {
    let len = vm.array_length(array)?;
    let string_class = vm.resolve_class("java/lang/String")?;
    let copy = vm.allocate_object_array(string_class, len)?;
    let mut seen: Vec<String> = Vec::new();
    for i in 0..len {
        let element = vm.array_get(array, i as i32)?.as_ref();
        if element.is_null() {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("Named group must not be null or empty"),
            ));
        }
        let text = vm.string_or_empty(element);
        if text.is_empty() {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("Named group must not be null or empty"),
            ));
        }
        if seen.contains(&text) {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("Duplicate named group"),
            ));
        }
        seen.push(text);
        vm.array_set_ref(copy, i, element)?;
    }
    Ok(copy)
}

const SSL_PARAMETERS_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "<init>",
        "([Ljava/lang/String;[Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "cipherSuites", "[Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            let (declaring, field) = vm.find_field(class, "protocols", "[Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 2));
            void()
        },
    ),
    method(
        "getCipherSuites",
        "()[Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "cipherSuites",
                "[Ljava/lang/String;",
            ))
        },
    ),
    method(
        "setCipherSuites",
        "([Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "cipherSuites", "[Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method(
        "getProtocols",
        "()[Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "protocols",
                "[Ljava/lang/String;",
            ))
        },
    ),
    method(
        "setProtocols",
        "([Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "protocols", "[Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method("getUseCipherSuitesOrder", "()Z", ACC_PUBLIC, |_, _, _| {
        boolean(true)
    }),
    method("setUseCipherSuitesOrder", "(Z)V", ACC_PUBLIC, |_, _, _| {
        void()
    }),
    method("getEnableRetransmissions", "()Z", ACC_PUBLIC, |_, _, _| {
        boolean(true)
    }),
    method(
        "getNamedGroups",
        "()[Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let groups = ref_field(vm, receiver(args), "namedGroups", "[Ljava/lang/String;");
            if groups.is_null() {
                return object(ObjectRef::NULL);
            }
            object(ssl_named_groups_clone(vm, groups)?)
        },
    ),
    method(
        "setNamedGroups",
        "([Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let groups = ref_arg(args, 1);
            let stored = if groups.is_null() {
                ObjectRef::NULL
            } else {
                ssl_named_groups_clone(vm, groups)?
            };
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "namedGroups", "[Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, stored);
            void()
        },
    ),
];

pub(crate) const SSL_PARAMETERS: oxjvm_vm::NativeClass = class(
    "javax/net/ssl/SSLParameters",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("cipherSuites", "[Ljava/lang/String;", ACC_PRIVATE, None),
        field("protocols", "[Ljava/lang/String;", ACC_PRIVATE, None),
        field("namedGroups", "[Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &SSL_PARAMETERS_METHODS,
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

pub(crate) fn md5(data: &[u8]) -> Vec<u8> {
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

const MESSAGE_DIGEST_METHODS: [oxjvm_vm::NativeMethodDef; 17] = [
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
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("No input buffer given"),
            ));
        }
        let capacity = vm.array_length(array)? as i32;
        if offset < 0 || length < 0 || offset > capacity - length {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("Input buffer too short"),
            ));
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
        let length = int_arg(args, 3);
        if buf.is_null() {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("No output buffer given"),
            ));
        }
        let capacity = vm.array_length(buf)? as i32;
        if offset < 0 || length < 0 || offset > capacity - length {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("Output buffer too small for specified offset and length"),
            ));
        }
        let digest = message_digest_compute(vm, this)?;
        let len = digest.len() as i32;
        if length < len {
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
    method(
        "update",
        "(Ljava/nio/ByteBuffer;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let buffer = ref_arg(args, 1);
            if buffer.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let bytes = crate::buffer::byte_buffer_remaining_bytes(vm, buffer)?;
            message_digest_append(vm, receiver(args), &bytes)?;
            let class = vm.class_of(buffer);
            if let Some((declaring, field)) = vm.find_field_opt(class, "limit", "I") {
                let limit = vm.read_instance_int(buffer, declaring, field).unwrap_or(0);
                if let Some((position_class, position_field)) =
                    vm.find_field_opt(class, "position", "I")
                {
                    vm.set_instance_int(buffer, position_class, position_field, limit);
                }
            }
            void()
        },
    ),
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

/// Resolve a Signature implementation advertised by a custom installed provider
/// through its `Signature.<algorithm>` property, mirroring the JDK's SPI lookup.
fn signature_custom_instance(
    vm: &mut Vm<'_>,
    algorithm: &str,
) -> Result<Option<ObjectRef>, VmError> {
    let security = vm.resolve_class("java/security/Security")?;
    vm.ensure_initialized(security)?;
    let extra = vm
        .read_static_ref(security, "extra", "Ljava/security/Provider;")
        .unwrap_or(ObjectRef::NULL);
    if extra.is_null() {
        return Ok(None);
    }
    let key = vm.make_string(&alloc::format!("Signature.{algorithm}"))?;
    let value = property_call(
        vm,
        extra,
        "getProperty",
        "(Ljava/lang/String;)Ljava/lang/String;",
        vec![Value::Ref(extra), Value::Ref(key)],
    )?
    .as_ref();
    if value.is_null() {
        return Ok(None);
    }
    let impl_name = vm.string_or_empty(value).replace('.', "/");
    let class = vm.resolve_class(&impl_name)?;
    let instance = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(class, "<init>", "()V")
        .ok_or_else(|| VmError::internal("Signature impl lacks <init>"))?;
    vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
    set_string_field(vm, instance, "algorithm", algorithm)?;
    set_ref_field(vm, instance, "provider", "Ljava/security/Provider;", extra)?;
    Ok(Some(instance))
}

fn signature_instance(vm: &mut Vm<'_>, algorithm: &str) -> Result<ObjectRef, VmError> {
    if signature_digest(algorithm).is_none() {
        if let Some(custom) = signature_custom_instance(vm, algorithm)? {
            return Ok(custom);
        }
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

const SIGNATURE_METHODS: [oxjvm_vm::NativeMethodDef; 20] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PROTECTED,
        |vm, _, args| {
            let this = receiver(args);
            let name = ref_arg(args, 1);
            if name.is_null() {
                set_ref_field(vm, this, "algorithm", "Ljava/lang/String;", ObjectRef::NULL)?;
            } else {
                let text = vm.string_or_empty(name);
                set_string_field(vm, this, "algorithm", &text)?;
            }
            void()
        },
    ),
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
            let des_class = vm.resolve_class("javax/crypto/spec/DESKeySpec")?;
            let desede_class = vm.resolve_class("javax/crypto/spec/DESedeKeySpec")?;
            let bytes = if vm.is_instance(spec, des_class) || vm.is_instance(spec, desede_class) {
                let (kc, km) = vm.resolve_virtual_method(spec, "getKey", "()[B")?;
                let array = vm.invoke_method(kc, km, vec![Value::Ref(spec)])?.as_ref();
                let mut key_bytes = read_byte_array(vm, array)?;
                if algorithm.eq_ignore_ascii_case("DES") || algorithm.eq_ignore_ascii_case("DESede")
                {
                    // DES key generation forces odd parity on every byte.
                    for byte in key_bytes.iter_mut() {
                        if (*byte as u32).count_ones() & 1 == 0 {
                            *byte ^= 1;
                        }
                    }
                }
                key_bytes
            } else {
                key_encoded(vm, spec)?
            };
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

// -------------------------------------------------------------------------------------------
// javax.security.auth.PrivateCredentialPermission
// -------------------------------------------------------------------------------------------

/// `StringTokenizer(name, " ", true)`: runs of non-space characters plus each
/// individual space as its own token.
fn pcp_tokenize(name: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in name.chars() {
        if ch == ' ' {
            if !current.is_empty() {
                tokens.push(core::mem::take(&mut current));
            }
            tokens.push(String::from(" "));
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Parse the permission name into the credential class and principal pairs,
/// mirroring the JDK's `init` (including its tokenizer-driven error cases).
fn pcp_init(vm: &mut Vm<'_>, this: ObjectRef, name: Option<&str>) -> Result<(), VmError> {
    let name = match name {
        Some(text) if !text.trim().is_empty() => text,
        _ => {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("invalid empty name"),
            ));
        }
    };
    let syntax_error = |vm: &mut Vm<'_>, detail: &str| {
        vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some(&alloc::format!(
                "permission name \"{name}\" syntax invalid: {detail}"
            )),
        )
    };
    let tokens = pcp_tokenize(name);
    let credential_class = tokens.first().cloned().unwrap_or_default();
    let mut index = 1usize;
    if index >= tokens.len() {
        return Err(syntax_error(
            vm,
            "Credential Class not followed by a Principal Class and Name",
        ));
    }
    let mut pairs: Vec<(String, String)> = Vec::new();
    while index < tokens.len() {
        index += 1; // skip the delimiter token
        let Some(principal_class) = tokens.get(index).cloned() else {
            return Err(vm.throw_new("java/util/NoSuchElementException", None));
        };
        index += 1;
        if index >= tokens.len() {
            return Err(syntax_error(
                vm,
                "Principal Class not followed by a Principal Name",
            ));
        }
        index += 1; // skip the delimiter token
        let Some(mut principal_name) = tokens.get(index).cloned() else {
            return Err(vm.throw_new("java/util/NoSuchElementException", None));
        };
        index += 1;
        if !principal_name.starts_with('"') {
            return Err(syntax_error(
                vm,
                "Principal Name must be surrounded by quotes",
            ));
        }
        if !principal_name.ends_with('"') {
            while index < tokens.len() {
                principal_name.push_str(&tokens[index]);
                index += 1;
                if principal_name.ends_with('"') {
                    break;
                }
            }
            if !principal_name.ends_with('"') {
                return Err(syntax_error(vm, "Principal Name missing end quote"));
            }
        }
        if principal_name.len() < 2 {
            return Err(vm.throw_new("java/lang/StringIndexOutOfBoundsException", None));
        }
        let inner = String::from(&principal_name[1..principal_name.len() - 1]);
        if principal_class == "*" && inner != "*" {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some(
                    "Principal Class can not be a wildcard value if Principal Name is not a wildcard value",
                ),
            ));
        }
        pairs.push((principal_class, inner));
    }
    set_string_field(vm, this, "credentialClass", &credential_class)?;
    let string_class = vm.resolve_class("java/lang/String")?;
    let string_array_class = vm.resolve_class("[Ljava/lang/String;")?;
    let owners = vm.allocate_object_array(string_array_class, pairs.len())?;
    for (slot, (principal_class, principal_name)) in pairs.iter().enumerate() {
        let pair = vm.allocate_object_array(string_class, 2)?;
        let class_ref = vm.make_string(principal_class)?;
        let name_ref = vm.make_string(principal_name)?;
        vm.array_set_ref(pair, 0, class_ref)?;
        vm.array_set_ref(pair, 1, name_ref)?;
        vm.array_set_ref(owners, slot, pair)?;
    }
    set_ref_field(vm, this, "owners", "[[Ljava/lang/String;", owners)?;
    Ok(())
}

/// Read the stored principal pairs back out of the `String[][]` field.
fn pcp_owner_pairs(vm: &mut Vm<'_>, obj: ObjectRef) -> Result<Vec<(String, String)>, VmError> {
    let owners = ref_field(vm, obj, "owners", "[[Ljava/lang/String;");
    let mut pairs = Vec::new();
    if owners.is_null() {
        return Ok(pairs);
    }
    let length = vm.array_length(owners)?;
    for index in 0..length {
        let pair = vm.array_get(owners, index as i32)?.as_ref();
        let class_ref = vm.array_get(pair, 0)?.as_ref();
        let name_ref = vm.array_get(pair, 1)?.as_ref();
        let principal_class = vm.string_value(class_ref).unwrap_or_default();
        let principal_name = vm.string_value(name_ref).unwrap_or_default();
        pairs.push((principal_class, principal_name));
    }
    Ok(pairs)
}

fn pcp_implies_credential_class(this_c: &str, that_c: &str) -> bool {
    if this_c == "*" {
        return true;
    }
    // The JDK strips the trailing "*." (two characters) before prefix matching.
    if this_c.ends_with('*') && this_c.len() >= 2 {
        return that_c.starts_with(&this_c[..this_c.len() - 2]);
    }
    this_c == that_c
}

fn pcp_implies_principal_set(this_p: &[(String, String)], that_p: &[(String, String)]) -> bool {
    if that_p.is_empty() {
        return true;
    }
    if this_p.is_empty() {
        return false;
    }
    this_p.iter().all(|(this_class, this_name)| {
        that_p.iter().any(|(that_class, that_name)| {
            (this_class == "*" || this_class == that_class)
                && (this_name == "*" || this_name == that_name)
        })
    })
}

/// Full implies check between two PrivateCredentialPermission instances.
fn pcp_implies(vm: &mut Vm<'_>, this: ObjectRef, other: ObjectRef) -> Result<bool, VmError> {
    let this_class = string_field(vm, this, "credentialClass");
    let that_class = string_field(vm, other, "credentialClass");
    if !pcp_implies_credential_class(&this_class, &that_class) {
        return Ok(false);
    }
    let this_principals = pcp_owner_pairs(vm, this)?;
    let that_principals = pcp_owner_pairs(vm, other)?;
    Ok(pcp_implies_principal_set(
        &this_principals,
        &that_principals,
    ))
}

const PRIVATE_CREDENTIAL_PERMISSION_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name_ref = ref_arg(args, 1);
            let actions = vm.string_or_empty(ref_arg(args, 2));
            let parent = vm.resolve_class("java/security/Permission")?;
            let (declaring, ctor) = vm
                .find_method(parent, "<init>", "(Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("Permission lacks <init>"))?;
            vm.invoke_method(
                declaring,
                ctor,
                vec![Value::Ref(this), Value::Ref(name_ref)],
            )?;
            if !actions.eq_ignore_ascii_case("read") {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("actions can only be read."),
                ));
            }
            let name = if name_ref.is_null() {
                None
            } else {
                vm.string_value(name_ref)
            };
            pcp_init(vm, this, name.as_deref())?;
            void()
        },
    ),
    method(
        "getCredentialClass",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "credentialClass",
                "Ljava/lang/String;",
            ))
        },
    ),
    method(
        "getPrincipals",
        "()[[Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let pairs = pcp_owner_pairs(vm, receiver(args))?;
            let string_class = vm.resolve_class("java/lang/String")?;
            let string_array_class = vm.resolve_class("[Ljava/lang/String;")?;
            let result = vm.allocate_object_array(string_array_class, pairs.len())?;
            for (slot, (principal_class, principal_name)) in pairs.iter().enumerate() {
                let pair = vm.allocate_object_array(string_class, 2)?;
                let class_ref = vm.make_string(principal_class)?;
                let name_ref = vm.make_string(principal_name)?;
                vm.array_set_ref(pair, 0, class_ref)?;
                vm.array_set_ref(pair, 1, name_ref)?;
                vm.array_set_ref(result, slot, pair)?;
            }
            object(result)
        },
    ),
    method(
        "implies",
        "(Ljava/security/Permission;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let pcp = vm.resolve_class("javax/security/auth/PrivateCredentialPermission")?;
            if other.is_null() || !vm.is_instance(other, pcp) {
                return boolean(false);
            }
            boolean(pcp_implies(vm, this, other)?)
        },
    ),
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
            let pcp = vm.resolve_class("javax/security/auth/PrivateCredentialPermission")?;
            if other.is_null() || !vm.is_instance(other, pcp) {
                return boolean(false);
            }
            let forward = pcp_implies(vm, this, other)?;
            let backward = pcp_implies(vm, other, this)?;
            boolean(forward && backward)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let credential = ref_field(vm, receiver(args), "credentialClass", "Ljava/lang/String;");
        let hash = vm
            .string_utf16(credential)
            .map(crate::lang::java_string_hash)
            .unwrap_or(0);
        int(hash)
    }),
    method(
        "getActions",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("read")?),
    ),
    method(
        "newPermissionCollection",
        "()Ljava/security/PermissionCollection;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
];

pub(crate) const PRIVATE_CREDENTIAL_PERMISSION: oxjvm_vm::NativeClass = class(
    "javax/security/auth/PrivateCredentialPermission",
    Some("java/security/Permission"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("credentialClass", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("owners", "[[Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &PRIVATE_CREDENTIAL_PERMISSION_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// java.security.cert CertStore / CertPathBuilder / CertPathValidator getInstance contracts
// -------------------------------------------------------------------------------------------

pub(crate) const CERT_STORE_PARAMETERS: oxjvm_vm::NativeClass = class(
    "java/security/cert/CertStoreParameters",
    Some("java/lang/Object"),
    &["java/lang/Cloneable"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &CERT_STORE_PARAMETERS_METHODS,
    None,
);

const CERT_STORE_PARAMETERS_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "clone",
    "()Ljava/lang/Object;",
    ACC_PUBLIC | ACC_ABSTRACT,
    crate::lang::abstract_method_pub,
)];

const COLLECTION_CERT_STORE_PARAMETERS_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "<init>",
        "(Ljava/util/Collection;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let collection = ref_arg(args, 1);
            if collection.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            set_ref_field(vm, this, "coll", "Ljava/util/Collection;", collection)?;
            void()
        },
    ),
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let empty = new_hash_set(vm)?;
        set_ref_field(vm, this, "coll", "Ljava/util/Collection;", empty)?;
        void()
    }),
    method(
        "getCollection",
        "()Ljava/util/Collection;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "coll",
                "Ljava/util/Collection;",
            ))
        },
    ),
    method(
        "clone",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let copy = vm.new_instance(class)?;
            let coll = ref_field(vm, this, "coll", "Ljava/util/Collection;");
            set_ref_field(vm, copy, "coll", "Ljava/util/Collection;", coll)?;
            object(copy)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let coll = ref_field(vm, receiver(args), "coll", "Ljava/util/Collection;");
            let (class, to_string) =
                vm.resolve_virtual_method(coll, "toString", "()Ljava/lang/String;")?;
            let text = vm.invoke_method(class, to_string, vec![Value::Ref(coll)])?;
            let rendered = vm.string_or_empty(text.as_ref());
            object(vm.make_string(&alloc::format!(
                "CollectionCertStoreParameters: [\n  collection: {rendered}\n]"
            ))?)
        },
    ),
];

pub(crate) const COLLECTION_CERT_STORE_PARAMETERS: oxjvm_vm::NativeClass = class(
    "java/security/cert/CollectionCertStoreParameters",
    Some("java/lang/Object"),
    &["java/security/cert/CertStoreParameters"],
    ACC_PUBLIC,
    &[field("coll", "Ljava/util/Collection;", ACC_PRIVATE, None)],
    &COLLECTION_CERT_STORE_PARAMETERS_METHODS,
    None,
);

fn cert_store_instance(
    vm: &mut Vm<'_>,
    cert_type: &str,
    params: ObjectRef,
) -> Result<ObjectRef, VmError> {
    let folded = cert_type.to_uppercase();
    let collection_class = vm.resolve_class("java/security/cert/CollectionCertStoreParameters")?;
    if folded == "COLLECTION" {
        if params.is_null() || !vm.is_instance(params, collection_class) {
            return Err(vm.throw_new(
                "java/security/InvalidAlgorithmParameterException",
                Some("params must be CollectionCertStoreParameters"),
            ));
        }
    } else if folded == "LDAP" {
        // The LDAP store requires LDAPCertStoreParameters, which this runtime
        // does not provide, so any argument is rejected like the JDK SPI does.
        return Err(vm.throw_new(
            "java/security/InvalidAlgorithmParameterException",
            Some("params must be LDAPCertStoreParameters"),
        ));
    } else {
        return Err(vm.throw_new("java/security/NoSuchAlgorithmException", Some(cert_type)));
    }
    let class = vm.resolve_class("java/security/cert/CertStore")?;
    let store = vm.new_instance(class)?;
    set_string_field(vm, store, "type", cert_type)?;
    set_ref_field(
        vm,
        store,
        "params",
        "Ljava/security/cert/CertStoreParameters;",
        params,
    )?;
    Ok(store)
}

const CERT_STORE_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/security/cert/CertStoreParameters;)Ljava/security/cert/CertStore;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("null type name")));
            }
            let cert_type = vm.string_or_empty(name);
            object(cert_store_instance(vm, &cert_type, ref_arg(args, 1))?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/security/cert/CertStoreParameters;Ljava/lang/String;)Ljava/security/cert/CertStore;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("null type name")));
            }
            let provider_arg = ref_arg(args, 2);
            if provider_arg.is_null() {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("null provider name"),
                ));
            }
            let provider = vm.string_or_empty(provider_arg);
            if provider.is_empty() {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("empty provider name"),
                ));
            }
            if !known_provider(&provider) {
                return Err(vm.throw_new("java/security/NoSuchProviderException", Some(&provider)));
            }
            let cert_type = vm.string_or_empty(name);
            object(cert_store_instance(vm, &cert_type, ref_arg(args, 1))?)
        },
    ),
    method(
        "getType",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "type", "Ljava/lang/String;")),
    ),
    method(
        "getCertStoreParameters",
        "()Ljava/security/cert/CertStoreParameters;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "params",
                "Ljava/security/cert/CertStoreParameters;",
            ))
        },
    ),
];

pub(crate) const CERT_STORE: oxjvm_vm::NativeClass = class(
    "java/security/cert/CertStore",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("type", "Ljava/lang/String;", ACC_PRIVATE, None),
        field(
            "params",
            "Ljava/security/cert/CertStoreParameters;",
            ACC_PRIVATE,
            None,
        ),
    ],
    &CERT_STORE_METHODS,
    None,
);

fn cert_path_instance(
    vm: &mut Vm<'_>,
    class_name: &str,
    algorithm: &str,
) -> Result<ObjectRef, VmError> {
    if algorithm.to_uppercase() != "PKIX" {
        return Err(vm.throw_new("java/security/NoSuchAlgorithmException", Some(algorithm)));
    }
    let class = vm.resolve_class(class_name)?;
    let instance = vm.new_instance(class)?;
    set_string_field(vm, instance, "algorithm", algorithm)?;
    Ok(instance)
}

const CERT_PATH_BUILDER_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "getInstance",
        "(Ljava/lang/String;)Ljava/security/cert/CertPathBuilder;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return Err(vm.throw_new(
                    "java/lang/NullPointerException",
                    Some("null algorithm name"),
                ));
            }
            let algorithm = vm.string_or_empty(name);
            object(cert_path_instance(
                vm,
                "java/security/cert/CertPathBuilder",
                &algorithm,
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
];

pub(crate) const CERT_PATH_BUILDER: oxjvm_vm::NativeClass = class(
    "java/security/cert/CertPathBuilder",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[field("algorithm", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &CERT_PATH_BUILDER_METHODS,
    None,
);

const CERT_PATH_VALIDATOR_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "getInstance",
        "(Ljava/lang/String;)Ljava/security/cert/CertPathValidator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return Err(vm.throw_new(
                    "java/lang/NullPointerException",
                    Some("null algorithm name"),
                ));
            }
            let algorithm = vm.string_or_empty(name);
            object(cert_path_instance(
                vm,
                "java/security/cert/CertPathValidator",
                &algorithm,
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
];

pub(crate) const CERT_PATH_VALIDATOR: oxjvm_vm::NativeClass = class(
    "java/security/cert/CertPathValidator",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[field("algorithm", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &CERT_PATH_VALIDATOR_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// java.security.cert TrustAnchor / PKIXParameters trust-anchor validation
// -------------------------------------------------------------------------------------------

const TRUST_ANCHOR_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/security/PublicKey;[B)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let ca_name = ref_arg(args, 1);
            let pub_key = ref_arg(args, 2);
            let name_constraints = ref_arg(args, 3);
            if pub_key.is_null() {
                return Err(vm.throw_new(
                    "java/lang/NullPointerException",
                    Some("the pubKey parameter must be non-null"),
                ));
            }
            if ca_name.is_null() {
                return Err(vm.throw_new(
                    "java/lang/NullPointerException",
                    Some("the caName parameter must be non-null"),
                ));
            }
            let name = vm.string_or_empty(ca_name);
            if name.is_empty() {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("the caName parameter must be a non-empty String"),
                ));
            }
            set_string_field(vm, this, "caName", &name)?;
            set_ref_field(vm, this, "pubKey", "Ljava/security/PublicKey;", pub_key)?;
            set_ref_field(vm, this, "nameConstraints", "[B", name_constraints)?;
            void()
        },
    ),
    method(
        "getCAName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "caName",
                "Ljava/lang/String;",
            ))
        },
    ),
    method(
        "getCAPublicKey",
        "()Ljava/security/PublicKey;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "pubKey",
                "Ljava/security/PublicKey;",
            ))
        },
    ),
    method("getNameConstraints", "()[B", ACC_PUBLIC, |vm, _, args| {
        object(ref_field(vm, receiver(args), "nameConstraints", "[B"))
    }),
    method(
        "getTrustedCert",
        "()Ljava/security/cert/X509Certificate;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
];

pub(crate) const TRUST_ANCHOR: oxjvm_vm::NativeClass = class(
    "java/security/cert/TrustAnchor",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("caName", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("pubKey", "Ljava/security/PublicKey;", ACC_PRIVATE, None),
        field("nameConstraints", "[B", ACC_PRIVATE, None),
    ],
    &TRUST_ANCHOR_METHODS,
    None,
);

fn pkix_set_trust_anchors(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    anchors: ObjectRef,
) -> Result<(), VmError> {
    if anchors.is_null() {
        return Err(vm.throw_new(
            "java/lang/NullPointerException",
            Some("the trustAnchors parameters must be non-null"),
        ));
    }
    let (class, is_empty) = vm.resolve_virtual_method(anchors, "isEmpty", "()Z")?;
    let empty = vm
        .invoke_method(class, is_empty, vec![Value::Ref(anchors)])?
        .as_int()
        != 0;
    if empty {
        return Err(vm.throw_new(
            "java/security/InvalidAlgorithmParameterException",
            Some("the trustAnchors parameter must be non-empty"),
        ));
    }
    let anchor_class = vm.resolve_class("java/security/cert/TrustAnchor")?;
    let (class, to_array) =
        vm.resolve_virtual_method(anchors, "toArray", "()[Ljava/lang/Object;")?;
    let array = vm
        .invoke_method(class, to_array, vec![Value::Ref(anchors)])?
        .as_ref();
    let length = vm.array_length(array)?;
    for index in 0..length {
        let element = vm.array_get(array, index as i32)?.as_ref();
        if element.is_null() || !vm.is_instance(element, anchor_class) {
            return Err(vm.throw_new(
                "java/lang/ClassCastException",
                Some("all elements of set must be of type java.security.cert.TrustAnchor"),
            ));
        }
    }
    set_ref_field(vm, this, "trustAnchors", "Ljava/util/Set;", anchors)?;
    Ok(())
}

const PKIX_PARAMETERS_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method("<init>", "(Ljava/util/Set;)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        pkix_set_trust_anchors(vm, this, ref_arg(args, 1))?;
        void()
    }),
    method(
        "setTrustAnchors",
        "(Ljava/util/Set;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            pkix_set_trust_anchors(vm, receiver(args), ref_arg(args, 1))?;
            void()
        },
    ),
    method(
        "getTrustAnchors",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "trustAnchors",
                "Ljava/util/Set;",
            ))
        },
    ),
];

pub(crate) const PKIX_PARAMETERS: oxjvm_vm::NativeClass = class(
    "java/security/cert/PKIXParameters",
    Some("java/lang/Object"),
    &["java/security/cert/CertPathParameters"],
    ACC_PUBLIC,
    &[field("trustAnchors", "Ljava/util/Set;", ACC_PRIVATE, None)],
    &PKIX_PARAMETERS_METHODS,
    None,
);

pub(crate) const CERT_PATH_PARAMETERS: oxjvm_vm::NativeClass = class(
    "java/security/cert/CertPathParameters",
    Some("java/lang/Object"),
    &["java/lang/Cloneable"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &CERT_PATH_PARAMETERS_METHODS,
    None,
);

const CERT_PATH_PARAMETERS_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "clone",
    "()Ljava/lang/Object;",
    ACC_PUBLIC | ACC_ABSTRACT,
    crate::lang::abstract_method_pub,
)];

// -------------------------------------------------------------------------------------------
// javax.net.ssl KeyManagerFactory / TrustManagerFactory getInstance contracts
// -------------------------------------------------------------------------------------------

fn ssl_factory_instance(
    vm: &mut Vm<'_>,
    class_name: &str,
    algorithm: &str,
) -> Result<ObjectRef, VmError> {
    if algorithm.is_empty() {
        return Err(vm.throw_new(
            "java/security/NoSuchAlgorithmException",
            Some("algorithm must not be empty"),
        ));
    }
    let class = vm.resolve_class(class_name)?;
    let factory = vm.new_instance(class)?;
    set_string_field(vm, factory, "algorithm", algorithm)?;
    let provider = new_sun_provider(vm, "SunJSSE")?;
    set_ref_field(
        vm,
        factory,
        "provider",
        "Ljava/security/Provider;",
        provider,
    )?;
    Ok(factory)
}

const KEY_MANAGER_FACTORY_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "getInstance",
        "(Ljava/lang/String;)Ljavax/net/ssl/KeyManagerFactory;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let algorithm = vm.string_or_empty(name);
            object(ssl_factory_instance(
                vm,
                "javax/net/ssl/KeyManagerFactory",
                &algorithm,
            )?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/lang/String;)Ljavax/net/ssl/KeyManagerFactory;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let provider = vm.string_or_empty(ref_arg(args, 1));
            if !known_provider(&provider) {
                return Err(vm.throw_new("java/security/NoSuchProviderException", Some(&provider)));
            }
            let algorithm = vm.string_or_empty(ref_arg(args, 0));
            object(ssl_factory_instance(
                vm,
                "javax/net/ssl/KeyManagerFactory",
                &algorithm,
            )?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/security/Provider;)Ljavax/net/ssl/KeyManagerFactory;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let algorithm = vm.string_or_empty(ref_arg(args, 0));
            object(ssl_factory_instance(
                vm,
                "javax/net/ssl/KeyManagerFactory",
                &algorithm,
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
];

pub(crate) const KEY_MANAGER_FACTORY: oxjvm_vm::NativeClass = class(
    "javax/net/ssl/KeyManagerFactory",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("algorithm", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("provider", "Ljava/security/Provider;", ACC_PRIVATE, None),
    ],
    &KEY_MANAGER_FACTORY_METHODS,
    None,
);

const TRUST_MANAGER_FACTORY_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "getInstance",
        "(Ljava/lang/String;)Ljavax/net/ssl/TrustManagerFactory;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let algorithm = vm.string_or_empty(name);
            object(ssl_factory_instance(
                vm,
                "javax/net/ssl/TrustManagerFactory",
                &algorithm,
            )?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/lang/String;)Ljavax/net/ssl/TrustManagerFactory;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let provider = vm.string_or_empty(ref_arg(args, 1));
            if !known_provider(&provider) {
                return Err(vm.throw_new("java/security/NoSuchProviderException", Some(&provider)));
            }
            let algorithm = vm.string_or_empty(ref_arg(args, 0));
            object(ssl_factory_instance(
                vm,
                "javax/net/ssl/TrustManagerFactory",
                &algorithm,
            )?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/security/Provider;)Ljavax/net/ssl/TrustManagerFactory;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() || ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let algorithm = vm.string_or_empty(ref_arg(args, 0));
            object(ssl_factory_instance(
                vm,
                "javax/net/ssl/TrustManagerFactory",
                &algorithm,
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
];

pub(crate) const TRUST_MANAGER_FACTORY: oxjvm_vm::NativeClass = class(
    "javax/net/ssl/TrustManagerFactory",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("algorithm", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("provider", "Ljava/security/Provider;", ACC_PRIVATE, None),
    ],
    &TRUST_MANAGER_FACTORY_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// javax.security.auth.login exception hierarchy
// -------------------------------------------------------------------------------------------

pub(crate) const LOGIN_EXCEPTION: oxjvm_vm::NativeClass = class(
    "javax/security/auth/login/LoginException",
    Some("java/security/GeneralSecurityException"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

pub(crate) const ACCOUNT_EXCEPTION: oxjvm_vm::NativeClass = class(
    "javax/security/auth/login/AccountException",
    Some("javax/security/auth/login/LoginException"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

pub(crate) const ACCOUNT_EXPIRED_EXCEPTION: oxjvm_vm::NativeClass = class(
    "javax/security/auth/login/AccountExpiredException",
    Some("javax/security/auth/login/AccountException"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

pub(crate) const ACCOUNT_LOCKED_EXCEPTION: oxjvm_vm::NativeClass = class(
    "javax/security/auth/login/AccountLockedException",
    Some("javax/security/auth/login/AccountException"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

pub(crate) const ACCOUNT_NOT_FOUND_EXCEPTION: oxjvm_vm::NativeClass = class(
    "javax/security/auth/login/AccountNotFoundException",
    Some("javax/security/auth/login/AccountException"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

pub(crate) const CREDENTIAL_EXCEPTION: oxjvm_vm::NativeClass = class(
    "javax/security/auth/login/CredentialException",
    Some("javax/security/auth/login/LoginException"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

pub(crate) const CREDENTIAL_EXPIRED_EXCEPTION: oxjvm_vm::NativeClass = class(
    "javax/security/auth/login/CredentialExpiredException",
    Some("javax/security/auth/login/CredentialException"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

pub(crate) const CREDENTIAL_NOT_FOUND_EXCEPTION: oxjvm_vm::NativeClass = class(
    "javax/security/auth/login/CredentialNotFoundException",
    Some("javax/security/auth/login/CredentialException"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

// -------------------------------------------------------------------------------------------
// javax.security.auth.kerberos.DelegationPermission
// -------------------------------------------------------------------------------------------

/// `StringTokenizer(target, "\"", true)`: runs of non-quote characters plus
/// each individual quote as its own token (empty runs are skipped).
fn dp_tokenize(target: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in target.chars() {
        if ch == '"' {
            if !current.is_empty() {
                tokens.push(core::mem::take(&mut current));
            }
            tokens.push(String::from("\""));
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn dp_init(vm: &mut Vm<'_>, this: ObjectRef, target: &str) -> Result<(), VmError> {
    let tokens = dp_tokenize(target);
    let mut illegal = |reason: &str| -> VmError {
        vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some(&alloc::format!("Illegal input [{target}]: {reason}")),
        )
    };
    let get = |index: usize| -> Option<&String> { tokens.get(index) };
    if get(0).map(String::as_str) != Some("\"") {
        return Err(illegal("improperly quoted"));
    }
    let subordinate = match get(1) {
        Some(text) => text,
        None => return Err(illegal("not enough input")),
    };
    if subordinate == "\"" {
        return Err(illegal("bad subordinate name"));
    }
    if get(2).is_none() {
        return Err(illegal("not enough input"));
    }
    match get(3) {
        Some(sep) if sep.trim().is_empty() => {}
        Some(_) => return Err(illegal("improperly separated")),
        None => return Err(illegal("not enough input")),
    }
    if get(4).is_none() {
        return Err(illegal("not enough input"));
    }
    let service = match get(5) {
        Some(text) => text,
        None => return Err(illegal("not enough input")),
    };
    if service == "\"" {
        return Err(illegal("bad service name"));
    }
    if get(6).is_none() {
        return Err(illegal("not enough input"));
    }
    if tokens.len() > 7 {
        return Err(illegal("extra input"));
    }
    set_string_field(vm, this, "subordinate", subordinate)?;
    set_string_field(vm, this, "service", service)?;
    Ok(())
}

fn dp_hash(vm: &Vm<'_>, this: ObjectRef) -> i32 {
    let subordinate = ref_field(vm, this, "subordinate", "Ljava/lang/String;");
    let service = ref_field(vm, this, "service", "Ljava/lang/String;");
    let sub_hash = vm
        .string_utf16(subordinate)
        .map(crate::lang::java_string_hash)
        .unwrap_or(0);
    let svc_hash = vm
        .string_utf16(service)
        .map(crate::lang::java_string_hash)
        .unwrap_or(0);
    // Objects.hash(subordinate, service)
    31i32
        .wrapping_mul(31i32.wrapping_add(sub_hash))
        .wrapping_add(svc_hash)
}

const DELEGATION_PERMISSION_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name_ref = ref_arg(args, 1);
            let parent = vm.resolve_class("java/security/Permission")?;
            let (declaring, ctor) = vm
                .find_method(parent, "<init>", "(Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("Permission lacks <init>"))?;
            vm.invoke_method(
                declaring,
                ctor,
                vec![Value::Ref(this), Value::Ref(name_ref)],
            )?;
            if name_ref.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let name = vm.string_or_empty(name_ref);
            dp_init(vm, this, &name)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name_ref = ref_arg(args, 1);
            let parent = vm.resolve_class("java/security/Permission")?;
            let (declaring, ctor) = vm
                .find_method(parent, "<init>", "(Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("Permission lacks <init>"))?;
            vm.invoke_method(
                declaring,
                ctor,
                vec![Value::Ref(this), Value::Ref(name_ref)],
            )?;
            if name_ref.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let name = vm.string_or_empty(name_ref);
            dp_init(vm, this, &name)?;
            void()
        },
    ),
    method(
        "implies",
        "(Ljava/security/Permission;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let class = vm.resolve_class("javax/security/auth/kerberos/DelegationPermission")?;
            let (declaring, equals) = vm
                .find_method(class, "equals", "(Ljava/lang/Object;)Z")
                .ok_or_else(|| VmError::internal("DelegationPermission lacks equals"))?;
            let result =
                vm.invoke_method(declaring, equals, vec![Value::Ref(this), Value::Ref(other)])?;
            Ok(result)
        },
    ),
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
            let class = vm.resolve_class("javax/security/auth/kerberos/DelegationPermission")?;
            if other.is_null() || !vm.is_instance(other, class) {
                return boolean(false);
            }
            let same_sub =
                string_field(vm, this, "subordinate") == string_field(vm, other, "subordinate");
            let same_svc = string_field(vm, this, "service") == string_field(vm, other, "service");
            boolean(same_sub && same_svc)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        int(dp_hash(vm, receiver(args)))
    }),
    method(
        "getActions",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("")?),
    ),
    method(
        "newPermissionCollection",
        "()Ljava/security/PermissionCollection;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class =
                vm.resolve_class("javax/security/auth/kerberos/KrbDelegationPermissionCollection")?;
            let instance = vm.new_instance(class)?;
            let (declaring, ctor) = vm.find_method(class, "<init>", "()V").ok_or_else(|| {
                VmError::internal("KrbDelegationPermissionCollection lacks <init>")
            })?;
            vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
            object(instance)
        },
    ),
];

pub(crate) const DELEGATION_PERMISSION: oxjvm_vm::NativeClass = class(
    "javax/security/auth/kerberos/DelegationPermission",
    Some("java/security/Permission"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("subordinate", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("service", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &DELEGATION_PERMISSION_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// javax.security.auth.kerberos DelegationPermission collection
// -------------------------------------------------------------------------------------------

fn dp_collection_contains(
    vm: &mut Vm<'_>,
    list: ObjectRef,
    permission: ObjectRef,
) -> Result<bool, VmError> {
    for existing in crate::util::array_list_elements(vm, list) {
        let (class, equals) =
            vm.resolve_virtual_method(existing, "equals", "(Ljava/lang/Object;)Z")?;
        let same = vm
            .invoke_method(
                class,
                equals,
                vec![Value::Ref(existing), Value::Ref(permission)],
            )?
            .as_int()
            != 0;
        if same {
            return Ok(true);
        }
    }
    Ok(false)
}

const DELEGATION_PERMISSION_COLLECTION_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let perms = new_array_list(vm)?;
        set_ref_field(vm, this, "perms", "Ljava/util/ArrayList;", perms)?;
        void()
    }),
    method(
        "add",
        "(Ljava/security/Permission;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let permission = ref_arg(args, 1);
            let dp = vm.resolve_class("javax/security/auth/kerberos/DelegationPermission")?;
            if permission.is_null() || !vm.is_instance(permission, dp) {
                let display = permission_display(vm, permission)?;
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some(&alloc::format!("invalid permission: {display}")),
                ));
            }
            ensure_collection_writable(vm, this)?;
            let perms = ref_field(vm, this, "perms", "Ljava/util/ArrayList;");
            if !dp_collection_contains(vm, perms, permission)? {
                list_add(vm, perms, permission)?;
            }
            void()
        },
    ),
    method(
        "implies",
        "(Ljava/security/Permission;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let permission = ref_arg(args, 1);
            let dp = vm.resolve_class("javax/security/auth/kerberos/DelegationPermission")?;
            if permission.is_null() || !vm.is_instance(permission, dp) {
                return boolean(false);
            }
            let perms = ref_field(vm, this, "perms", "Ljava/util/ArrayList;");
            boolean(dp_collection_contains(vm, perms, permission)?)
        },
    ),
    method(
        "elements",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let perms = ref_field(vm, receiver(args), "perms", "Ljava/util/ArrayList;");
            let elements = crate::util::array_list_elements(vm, perms);
            object(crate::util::new_enumeration(vm, &elements)?)
        },
    ),
];

pub(crate) const DELEGATION_PERMISSION_COLLECTION: oxjvm_vm::NativeClass = class(
    "javax/security/auth/kerberos/KrbDelegationPermissionCollection",
    Some("java/security/PermissionCollection"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[field("perms", "Ljava/util/ArrayList;", ACC_PRIVATE, None)],
    &DELEGATION_PERMISSION_COLLECTION_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// javax.security.auth.kerberos.ServicePermission (+ collection)
// -------------------------------------------------------------------------------------------

const SP_INITIATE: i32 = 0x1;
const SP_ACCEPT: i32 = 0x2;

fn sp_get_mask(vm: &mut Vm<'_>, action: &str) -> Result<i32, VmError> {
    let trimmed = action.trim();
    if trimmed == "-" || trimmed.is_empty() {
        return Ok(0);
    }
    let mut mask = 0;
    for part in trimmed.split(',') {
        let token = part.trim();
        if token.is_empty() {
            continue;
        }
        if token.eq_ignore_ascii_case("initiate") {
            mask |= SP_INITIATE;
        } else if token.eq_ignore_ascii_case("accept") {
            mask |= SP_ACCEPT;
        } else {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some(&alloc::format!("invalid permission: {action}")),
            ));
        }
    }
    Ok(mask)
}

fn sp_actions(mask: i32) -> &'static str {
    match mask & (SP_INITIATE | SP_ACCEPT) {
        SP_INITIATE => "initiate",
        SP_ACCEPT => "accept",
        _ => "initiate,accept",
    }
}

fn sp_implies_ignore_mask(vm: &Vm<'_>, this: ObjectRef, other: ObjectRef) -> bool {
    let this_name = string_field(vm, this, "name");
    let other_name = string_field(vm, other, "name");
    this_name == "*"
        || this_name == other_name
        || (other_name.starts_with('@') && this_name.ends_with(&other_name))
}

fn sp_with_mask(vm: &mut Vm<'_>, name: &str, mask: i32) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("javax/security/auth/kerberos/ServicePermission")?;
    let instance = vm.new_instance(class)?;
    let parent = vm.resolve_class("java/security/Permission")?;
    let (declaring, ctor) = vm
        .find_method(parent, "<init>", "(Ljava/lang/String;)V")
        .ok_or_else(|| VmError::internal("Permission lacks <init>"))?;
    let name_value = vm.make_string(name)?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![Value::Ref(instance), Value::Ref(name_value)],
    )?;
    crate::lang::write_int_field(vm, instance, "mask", mask);
    Ok(instance)
}

const SERVICE_PERMISSION_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name_ref = ref_arg(args, 1);
            let action_ref = ref_arg(args, 2);
            let parent = vm.resolve_class("java/security/Permission")?;
            let (declaring, ctor) = vm
                .find_method(parent, "<init>", "(Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("Permission lacks <init>"))?;
            vm.invoke_method(
                declaring,
                ctor,
                vec![Value::Ref(this), Value::Ref(name_ref)],
            )?;
            if name_ref.is_null() {
                return Err(vm.throw_new(
                    "java/lang/NullPointerException",
                    Some("service principal can't be null"),
                ));
            }
            let action = vm.string_or_empty(action_ref);
            let mask = sp_get_mask(vm, &action)?;
            crate::lang::write_int_field(vm, this, "mask", mask);
            void()
        },
    ),
    method(
        "implies",
        "(Ljava/security/Permission;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let class = vm.resolve_class("javax/security/auth/kerberos/ServicePermission")?;
            if other.is_null() || !vm.is_instance(other, class) {
                return boolean(false);
            }
            let this_mask = crate::lang::read_int_field(vm, this, "mask");
            let other_mask = crate::lang::read_int_field(vm, other, "mask");
            let covers = (this_mask & other_mask) == other_mask;
            boolean(covers && sp_implies_ignore_mask(vm, this, other))
        },
    ),
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
            let class = vm.resolve_class("javax/security/auth/kerberos/ServicePermission")?;
            if other.is_null() || !vm.is_instance(other, class) {
                return boolean(false);
            }
            let same_mask = crate::lang::read_int_field(vm, this, "mask")
                == crate::lang::read_int_field(vm, other, "mask");
            let same_name = string_field(vm, this, "name") == string_field(vm, other, "name");
            boolean(same_mask && same_name)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let name = string_field(vm, this, "name");
        let name_hash = crate::lang::java_string_hash(&name.encode_utf16().collect::<Vec<u16>>());
        let mask = crate::lang::read_int_field(vm, this, "mask");
        int(name_hash ^ mask)
    }),
    method(
        "getActions",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let mask = crate::lang::read_int_field(vm, receiver(args), "mask");
            object(vm.make_string(sp_actions(mask))?)
        },
    ),
    method("getMask", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "mask"))
    }),
    method(
        "newPermissionCollection",
        "()Ljava/security/PermissionCollection;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class =
                vm.resolve_class("javax/security/auth/kerberos/KrbServicePermissionCollection")?;
            let instance = vm.new_instance(class)?;
            let (declaring, ctor) = vm
                .find_method(class, "<init>", "()V")
                .ok_or_else(|| VmError::internal("KrbServicePermissionCollection lacks <init>"))?;
            vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
            object(instance)
        },
    ),
];

pub(crate) const SERVICE_PERMISSION: oxjvm_vm::NativeClass = class(
    "javax/security/auth/kerberos/ServicePermission",
    Some("java/security/Permission"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[field("mask", "I", ACC_PRIVATE, None)],
    &SERVICE_PERMISSION_METHODS,
    None,
);

fn sp_collection_find(vm: &Vm<'_>, list: ObjectRef, name: &str) -> Option<ObjectRef> {
    crate::util::array_list_elements(vm, list)
        .into_iter()
        .find(|&existing| string_field(vm, existing, "name") == name)
}

const SERVICE_PERMISSION_COLLECTION_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let perms = new_array_list(vm)?;
        set_ref_field(vm, this, "perms", "Ljava/util/ArrayList;", perms)?;
        void()
    }),
    method(
        "add",
        "(Ljava/security/Permission;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let permission = ref_arg(args, 1);
            let sp = vm.resolve_class("javax/security/auth/kerberos/ServicePermission")?;
            if permission.is_null() || !vm.is_instance(permission, sp) {
                let display = permission_display(vm, permission)?;
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some(&alloc::format!("invalid permission: {display}")),
                ));
            }
            ensure_collection_writable(vm, this)?;
            let perms = ref_field(vm, this, "perms", "Ljava/util/ArrayList;");
            let name = string_field(vm, permission, "name");
            let new_mask = crate::lang::read_int_field(vm, permission, "mask");
            if let Some(existing) = sp_collection_find(vm, perms, &name) {
                let old_mask = crate::lang::read_int_field(vm, existing, "mask");
                if old_mask != new_mask {
                    let effective = old_mask | new_mask;
                    if effective == new_mask {
                        // Replace existing with the new permission.
                        let elements = crate::util::array_list_elements(vm, perms);
                        let fresh = new_array_list(vm)?;
                        for element in elements {
                            if element == existing {
                                list_add(vm, fresh, permission)?;
                            } else {
                                list_add(vm, fresh, element)?;
                            }
                        }
                        set_ref_field(vm, this, "perms", "Ljava/util/ArrayList;", fresh)?;
                    } else if effective != old_mask {
                        let merged = sp_with_mask(vm, &name, effective)?;
                        let elements = crate::util::array_list_elements(vm, perms);
                        let fresh = new_array_list(vm)?;
                        for element in elements {
                            if element == existing {
                                list_add(vm, fresh, merged)?;
                            } else {
                                list_add(vm, fresh, element)?;
                            }
                        }
                        set_ref_field(vm, this, "perms", "Ljava/util/ArrayList;", fresh)?;
                    }
                }
            } else {
                list_add(vm, perms, permission)?;
            }
            void()
        },
    ),
    method(
        "implies",
        "(Ljava/security/Permission;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let permission = ref_arg(args, 1);
            let sp = vm.resolve_class("javax/security/auth/kerberos/ServicePermission")?;
            if permission.is_null() || !vm.is_instance(permission, sp) {
                return boolean(false);
            }
            let perms = ref_field(vm, this, "perms", "Ljava/util/ArrayList;");
            let desired = crate::lang::read_int_field(vm, permission, "mask");
            if desired == 0 {
                for existing in crate::util::array_list_elements(vm, perms) {
                    if sp_implies_ignore_mask(vm, existing, permission) {
                        return boolean(true);
                    }
                }
                return boolean(false);
            }
            if let Some(star) = sp_collection_find(vm, perms, "*") {
                let mask = crate::lang::read_int_field(vm, star, "mask");
                if (mask & desired) == desired {
                    return boolean(true);
                }
            }
            let name = string_field(vm, permission, "name");
            if let Some(existing) = sp_collection_find(vm, perms, &name) {
                let mask = crate::lang::read_int_field(vm, existing, "mask");
                return boolean((mask & desired) == desired);
            }
            boolean(false)
        },
    ),
    method(
        "elements",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let perms = ref_field(vm, receiver(args), "perms", "Ljava/util/ArrayList;");
            let elements = crate::util::array_list_elements(vm, perms);
            object(crate::util::new_enumeration(vm, &elements)?)
        },
    ),
];

pub(crate) const SERVICE_PERMISSION_COLLECTION: oxjvm_vm::NativeClass = class(
    "javax/security/auth/kerberos/KrbServicePermissionCollection",
    Some("java/security/PermissionCollection"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[field("perms", "Ljava/util/ArrayList;", ACC_PRIVATE, None)],
    &SERVICE_PERMISSION_COLLECTION_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// javax.crypto.spec parameter helpers and RC5 / RC2 / DES / DESede key specs
// -------------------------------------------------------------------------------------------

/// Copy `len` bytes starting at `offset` out of a byte array into a fresh array.
fn byte_slice(
    vm: &mut Vm<'_>,
    array: ObjectRef,
    offset: usize,
    len: usize,
) -> Result<ObjectRef, VmError> {
    let bytes = read_byte_array(vm, array)?;
    new_byte_array(vm, &bytes[offset..offset + len])
}

/// `Arrays.hashCode(byte[])` over raw bytes (a null array contributes zero).
fn byte_array_hash_bytes(bytes: Option<&[u8]>) -> i32 {
    let mut result = 1i32;
    if let Some(bytes) = bytes {
        for byte in bytes {
            result = result.wrapping_mul(31).wrapping_add(i32::from(*byte));
        }
    }
    result
}

/// Shared GCMParameterSpec init: validate tLen and the buffer window, then copy.
fn gcm_init(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    t_len: i32,
    src: ObjectRef,
    offset: i32,
    len: i32,
) -> Result<(), VmError> {
    if t_len < 0 {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("Length argument is negative"),
        ));
    }
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "tLen", "I")?;
    vm.set_instance_int(this, declaring, field, t_len);
    let total = if src.is_null() {
        0
    } else {
        vm.array_length(src)? as i32
    };
    if src.is_null() || len < 0 || offset < 0 || len > total - offset {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("Invalid buffer arguments"),
        ));
    }
    let copy = byte_slice(vm, src, offset as usize, len as usize)?;
    set_ref_field(vm, this, "src", "[B", copy)?;
    Ok(())
}

const RC5_PARAMETER_SPEC_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method("<init>", "(III)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        crate::lang::write_int_field(vm, this, "version", int_arg(args, 1));
        crate::lang::write_int_field(vm, this, "rounds", int_arg(args, 2));
        crate::lang::write_int_field(vm, this, "wordSize", int_arg(args, 3));
        void()
    }),
    method("<init>", "(III[B)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let word_size = int_arg(args, 3);
        crate::lang::write_int_field(vm, this, "version", int_arg(args, 1));
        crate::lang::write_int_field(vm, this, "rounds", int_arg(args, 2));
        crate::lang::write_int_field(vm, this, "wordSize", word_size);
        let iv = ref_arg(args, 4);
        let block_size = (word_size / 8) * 2;
        rc5_rc2_iv(vm, this, iv, 0, block_size)?;
        void()
    }),
    method("<init>", "(III[BI)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let word_size = int_arg(args, 3);
        crate::lang::write_int_field(vm, this, "version", int_arg(args, 1));
        crate::lang::write_int_field(vm, this, "rounds", int_arg(args, 2));
        crate::lang::write_int_field(vm, this, "wordSize", word_size);
        let block_size = (word_size / 8) * 2;
        rc5_rc2_iv(vm, this, ref_arg(args, 4), int_arg(args, 5), block_size)?;
        void()
    }),
    method("getVersion", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "version"))
    }),
    method("getRounds", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "rounds"))
    }),
    method("getWordSize", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "wordSize"))
    }),
    method("getIV", "()[B", ACC_PUBLIC, |vm, _, args| {
        let iv = ref_field(vm, receiver(args), "iv", "[B");
        if iv.is_null() {
            return object(ObjectRef::NULL);
        }
        let len = vm.array_length(iv)?;
        object(byte_slice(vm, iv, 0, len)?)
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
            let class = vm.resolve_class("javax/crypto/spec/RC5ParameterSpec")?;
            if other.is_null() || !vm.is_instance(other, class) {
                return boolean(false);
            }
            let same_params = crate::lang::read_int_field(vm, this, "version")
                == crate::lang::read_int_field(vm, other, "version")
                && crate::lang::read_int_field(vm, this, "rounds")
                    == crate::lang::read_int_field(vm, other, "rounds")
                && crate::lang::read_int_field(vm, this, "wordSize")
                    == crate::lang::read_int_field(vm, other, "wordSize");
            let this_iv = ref_field(vm, this, "iv", "[B");
            let other_iv = ref_field(vm, other, "iv", "[B");
            let same_iv = read_byte_array(vm, this_iv)? == read_byte_array(vm, other_iv)?;
            boolean(same_params && same_iv)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let iv = ref_field(vm, this, "iv", "[B");
        let iv_bytes = if iv.is_null() {
            None
        } else {
            Some(read_byte_array(vm, iv)?)
        };
        let hash = byte_array_hash_bytes(iv_bytes.as_deref())
            .wrapping_add(crate::lang::read_int_field(vm, this, "version"))
            .wrapping_add(crate::lang::read_int_field(vm, this, "rounds"))
            .wrapping_add(crate::lang::read_int_field(vm, this, "wordSize"));
        int(hash)
    }),
];

/// Validate and copy the IV window shared by RC5/RC2 (blockSize bytes required).
fn rc5_rc2_iv(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    iv: ObjectRef,
    offset: i32,
    block_size: i32,
) -> Result<(), VmError> {
    if iv.is_null() {
        return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("IV missing")));
    }
    if offset < 0 {
        return Err(vm.throw_new(
            "java/lang/ArrayIndexOutOfBoundsException",
            Some("offset is negative"),
        ));
    }
    let total = vm.array_length(iv)? as i32;
    if total - offset < block_size {
        return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("IV too short")));
    }
    let copy = byte_slice(vm, iv, offset as usize, block_size as usize)?;
    set_ref_field(vm, this, "iv", "[B", copy)?;
    Ok(())
}

pub(crate) const RC5_PARAMETER_SPEC: oxjvm_vm::NativeClass = class(
    "javax/crypto/spec/RC5ParameterSpec",
    Some("java/lang/Object"),
    &["java/security/spec/AlgorithmParameterSpec"],
    ACC_PUBLIC,
    &[
        field("version", "I", ACC_PRIVATE, None),
        field("rounds", "I", ACC_PRIVATE, None),
        field("wordSize", "I", ACC_PRIVATE, None),
        field("iv", "[B", ACC_PRIVATE, None),
    ],
    &RC5_PARAMETER_SPEC_METHODS,
    None,
);

const RC2_PARAMETER_SPEC_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        crate::lang::write_int_field(vm, receiver(args), "effectiveKeyBits", int_arg(args, 1));
        void()
    }),
    method("<init>", "(I[B)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        crate::lang::write_int_field(vm, this, "effectiveKeyBits", int_arg(args, 1));
        rc5_rc2_iv(vm, this, ref_arg(args, 2), 0, 8)?;
        void()
    }),
    method("<init>", "(I[BI)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        crate::lang::write_int_field(vm, this, "effectiveKeyBits", int_arg(args, 1));
        rc5_rc2_iv(vm, this, ref_arg(args, 2), int_arg(args, 3), 8)?;
        void()
    }),
    method("getEffectiveKeyBits", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(
            vm,
            receiver(args),
            "effectiveKeyBits",
        ))
    }),
    method("getIV", "()[B", ACC_PUBLIC, |vm, _, args| {
        let iv = ref_field(vm, receiver(args), "iv", "[B");
        if iv.is_null() {
            return object(ObjectRef::NULL);
        }
        let len = vm.array_length(iv)?;
        object(byte_slice(vm, iv, 0, len)?)
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
            let class = vm.resolve_class("javax/crypto/spec/RC2ParameterSpec")?;
            if other.is_null() || !vm.is_instance(other, class) {
                return boolean(false);
            }
            let same_bits = crate::lang::read_int_field(vm, this, "effectiveKeyBits")
                == crate::lang::read_int_field(vm, other, "effectiveKeyBits");
            let this_iv = ref_field(vm, this, "iv", "[B");
            let other_iv = ref_field(vm, other, "iv", "[B");
            let same_iv = read_byte_array(vm, this_iv)? == read_byte_array(vm, other_iv)?;
            boolean(same_bits && same_iv)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let iv = ref_field(vm, this, "iv", "[B");
        let iv_bytes = if iv.is_null() {
            None
        } else {
            Some(read_byte_array(vm, iv)?)
        };
        let hash = byte_array_hash_bytes(iv_bytes.as_deref())
            .wrapping_add(crate::lang::read_int_field(vm, this, "effectiveKeyBits"));
        int(hash)
    }),
];

pub(crate) const RC2_PARAMETER_SPEC: oxjvm_vm::NativeClass = class(
    "javax/crypto/spec/RC2ParameterSpec",
    Some("java/lang/Object"),
    &["java/security/spec/AlgorithmParameterSpec"],
    ACC_PUBLIC,
    &[
        field("effectiveKeyBits", "I", ACC_PRIVATE, None),
        field("iv", "[B", ACC_PRIVATE, None),
    ],
    &RC2_PARAMETER_SPEC_METHODS,
    None,
);

// DES weak / semi-weak key table (16 entries of 8 bytes), mirroring the JDK.
const DES_WEAK_KEYS: [[u8; 8]; 16] = [
    [0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01],
    [0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE],
    [0x1F, 0x1F, 0x1F, 0x1F, 0x0E, 0x0E, 0x0E, 0x0E],
    [0xE0, 0xE0, 0xE0, 0xE0, 0xF1, 0xF1, 0xF1, 0xF1],
    [0x01, 0xFE, 0x01, 0xFE, 0x01, 0xFE, 0x01, 0xFE],
    [0x1F, 0xE0, 0x1F, 0xE0, 0x0E, 0xF1, 0x0E, 0xF1],
    [0x01, 0xE0, 0x01, 0xE0, 0x01, 0xF1, 0x01, 0xF1],
    [0x1F, 0xFE, 0x1F, 0xFE, 0x0E, 0xFE, 0x0E, 0xFE],
    [0x01, 0x1F, 0x01, 0x1F, 0x01, 0x0E, 0x01, 0x0E],
    [0xE0, 0xFE, 0xE0, 0xFE, 0xF1, 0xFE, 0xF1, 0xFE],
    [0xFE, 0x01, 0xFE, 0x01, 0xFE, 0x01, 0xFE, 0x01],
    [0xE0, 0x1F, 0xE0, 0x1F, 0xF1, 0x0E, 0xF1, 0x0E],
    [0xE0, 0x01, 0xE0, 0x01, 0xF1, 0x01, 0xF1, 0x01],
    [0xFE, 0x1F, 0xFE, 0x1F, 0xFE, 0x0E, 0xFE, 0x0E],
    [0x1F, 0x01, 0x1F, 0x01, 0x0E, 0x01, 0x0E, 0x01],
    [0xFE, 0xE0, 0xFE, 0xE0, 0xFE, 0xF1, 0xFE, 0xF1],
];

/// Shared DES/DESede key-window validation: null key, negative offset, short key.
fn des_key_window(
    vm: &mut Vm<'_>,
    key: ObjectRef,
    offset: i32,
    key_len: i32,
) -> Result<Vec<u8>, VmError> {
    if key.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", Some("null key")));
    }
    if offset < 0 {
        return Err(vm.throw_new(
            "java/lang/ArrayIndexOutOfBoundsException",
            Some("offset is negative"),
        ));
    }
    let bytes = read_byte_array(vm, key)?;
    if (bytes.len() as i32) - offset < key_len {
        return Err(vm.throw_new("java/security/InvalidKeyException", Some("Wrong key size")));
    }
    Ok(bytes)
}

/// Static isParityAdjusted/isWeak validation uses InvalidKeyException for null.
fn des_static_window(
    vm: &mut Vm<'_>,
    key: ObjectRef,
    offset: i32,
    key_len: i32,
) -> Result<Vec<u8>, VmError> {
    if key.is_null() {
        return Err(vm.throw_new("java/security/InvalidKeyException", Some("null key")));
    }
    if offset < 0 {
        return Err(vm.throw_new(
            "java/lang/ArrayIndexOutOfBoundsException",
            Some("offset is negative"),
        ));
    }
    let bytes = read_byte_array(vm, key)?;
    if (bytes.len() as i32) - offset < key_len {
        return Err(vm.throw_new("java/security/InvalidKeyException", Some("Wrong key size")));
    }
    Ok(bytes)
}

fn des_parity_adjusted(bytes: &[u8], offset: usize) -> bool {
    for i in 0..8 {
        let k = (bytes[offset + i] as u32).count_ones();
        if k & 1 == 0 {
            return false;
        }
    }
    true
}

const DES_KEY_SPEC_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("<init>", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let bytes = des_key_window(vm, ref_arg(args, 1), 0, 8)?;
        let copy = new_byte_array(vm, &bytes[0..8])?;
        set_ref_field(vm, this, "key", "[B", copy)?;
        void()
    }),
    method("<init>", "([BI)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let offset = int_arg(args, 2);
        let bytes = des_key_window(vm, ref_arg(args, 1), offset, 8)?;
        let copy = new_byte_array(vm, &bytes[offset as usize..offset as usize + 8])?;
        set_ref_field(vm, this, "key", "[B", copy)?;
        void()
    }),
    method("getKey", "()[B", ACC_PUBLIC, |vm, _, args| {
        let key = ref_field(vm, receiver(args), "key", "[B");
        let len = vm.array_length(key)?;
        object(byte_slice(vm, key, 0, len)?)
    }),
    method(
        "isParityAdjusted",
        "([BI)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let bytes = des_static_window(vm, ref_arg(args, 0), int_arg(args, 1), 8)?;
            boolean(des_parity_adjusted(&bytes, int_arg(args, 1) as usize))
        },
    ),
    method(
        "isWeak",
        "([BI)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let offset = int_arg(args, 1);
            let bytes = des_static_window(vm, ref_arg(args, 0), offset, 8)?;
            let window = &bytes[offset as usize..offset as usize + 8];
            boolean(DES_WEAK_KEYS.iter().any(|weak| weak == window))
        },
    ),
];

pub(crate) const DES_KEY_SPEC: oxjvm_vm::NativeClass = class(
    "javax/crypto/spec/DESKeySpec",
    Some("java/lang/Object"),
    &["java/security/spec/KeySpec"],
    ACC_PUBLIC,
    &[field("key", "[B", ACC_PRIVATE, None)],
    &DES_KEY_SPEC_METHODS,
    None,
);

const DES_EDE_KEY_SPEC_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method("<init>", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let bytes = des_key_window(vm, ref_arg(args, 1), 0, 24)?;
        let copy = new_byte_array(vm, &bytes[0..24])?;
        set_ref_field(vm, this, "key", "[B", copy)?;
        void()
    }),
    method("<init>", "([BI)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let offset = int_arg(args, 2);
        let bytes = des_key_window(vm, ref_arg(args, 1), offset, 24)?;
        let copy = new_byte_array(vm, &bytes[offset as usize..offset as usize + 24])?;
        set_ref_field(vm, this, "key", "[B", copy)?;
        void()
    }),
    method("getKey", "()[B", ACC_PUBLIC, |vm, _, args| {
        let key = ref_field(vm, receiver(args), "key", "[B");
        let len = vm.array_length(key)?;
        object(byte_slice(vm, key, 0, len)?)
    }),
    method(
        "isParityAdjusted",
        "([BI)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let offset = int_arg(args, 1);
            let bytes = des_static_window(vm, ref_arg(args, 0), offset, 24)?;
            let o = offset as usize;
            boolean(
                des_parity_adjusted(&bytes, o)
                    && des_parity_adjusted(&bytes, o + 8)
                    && des_parity_adjusted(&bytes, o + 16),
            )
        },
    ),
];

pub(crate) const DES_EDE_KEY_SPEC: oxjvm_vm::NativeClass = class(
    "javax/crypto/spec/DESedeKeySpec",
    Some("java/lang/Object"),
    &["java/security/spec/KeySpec"],
    ACC_PUBLIC,
    &[field("key", "[B", ACC_PRIVATE, None)],
    &DES_EDE_KEY_SPEC_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// java.security.URIParameter and javax.security.auth.login.Configuration
// -------------------------------------------------------------------------------------------

pub(crate) const POLICY_PARAMETERS: oxjvm_vm::NativeClass = class(
    "java/security/Policy$Parameters",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const CONFIGURATION_PARAMETERS: oxjvm_vm::NativeClass = class(
    "javax/security/auth/login/Configuration$Parameters",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const URI_PARAMETER: oxjvm_vm::NativeClass = class(
    "java/security/URIParameter",
    Some("java/lang/Object"),
    &[
        "java/security/Policy$Parameters",
        "javax/security/auth/login/Configuration$Parameters",
    ],
    ACC_PUBLIC,
    &[field("uri", "Ljava/net/URI;", ACC_PRIVATE, None)],
    &[
        method("<init>", "(Ljava/net/URI;)V", ACC_PUBLIC, |vm, _, args| {
            let uri = ref_arg(args, 1);
            if uri.is_null() {
                return Err(
                    vm.throw_new("java/lang/NullPointerException", Some("invalid null URI"))
                );
            }
            set_ref_field(vm, receiver(args), "uri", "Ljava/net/URI;", uri)?;
            void()
        }),
        method("getURI", "()Ljava/net/URI;", ACC_PUBLIC, |vm, _, args| {
            object(ref_field(vm, receiver(args), "uri", "Ljava/net/URI;"))
        }),
    ],
    None,
);

fn configuration_instance(vm: &mut Vm<'_>, service_type: &str) -> Result<ObjectRef, VmError> {
    if !service_type.eq_ignore_ascii_case("JavaLoginConfig") {
        return Err(vm.throw_new(
            "java/security/NoSuchAlgorithmException",
            Some(&alloc::format!(
                "{service_type} Configuration not available"
            )),
        ));
    }
    let class = vm.resolve_class("javax/security/auth/login/Configuration")?;
    let config = vm.new_instance(class)?;
    set_string_field(vm, config, "type", service_type)?;
    Ok(config)
}

const CONFIGURATION_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "getInstance",
        "(Ljava/lang/String;Ljavax/security/auth/login/Configuration$Parameters;)Ljavax/security/auth/login/Configuration;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("null type name")));
            }
            object(configuration_instance(vm, &vm.string_or_empty(name))?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljavax/security/auth/login/Configuration$Parameters;Ljava/security/Provider;)Ljavax/security/auth/login/Configuration;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("null type name")));
            }
            if ref_arg(args, 2).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(configuration_instance(vm, &vm.string_or_empty(name))?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/String;Ljava/security/Provider;)Ljavax/security/auth/login/Configuration;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("null type name")));
            }
            object(configuration_instance(vm, &vm.string_or_empty(name))?)
        },
    ),
    method(
        "getType",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "type", "Ljava/lang/String;")),
    ),
];

pub(crate) const CONFIGURATION: oxjvm_vm::NativeClass = class(
    "javax/security/auth/login/Configuration",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[field("type", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &CONFIGURATION_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// javax.security.auth.callback callbacks
// -------------------------------------------------------------------------------------------

fn cb_read_chars(vm: &mut Vm<'_>, array: ObjectRef) -> Result<Option<Vec<u16>>, VmError> {
    if array.is_null() {
        return Ok(None);
    }
    let len = vm.array_length(array)?;
    let mut units = Vec::with_capacity(len);
    for index in 0..len {
        units.push(vm.array_get(array, index as i32)?.as_int() as u16);
    }
    Ok(Some(units))
}

fn cb_new_chars(vm: &mut Vm<'_>, units: &[u16]) -> Result<ObjectRef, VmError> {
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Char, units.len())?;
    for (index, unit) in units.iter().enumerate() {
        vm.array_set(array, index as i32, Value::Int(i32::from(*unit)), false)?;
    }
    Ok(array)
}

fn cb_prompt(vm: &mut Vm<'_>, prompt: ObjectRef) -> Result<String, VmError> {
    if prompt.is_null() {
        return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
    }
    let text = vm.string_or_empty(prompt);
    if text.is_empty() {
        return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
    }
    Ok(text)
}

pub(crate) const CALLBACK: oxjvm_vm::NativeClass = class(
    "javax/security/auth/callback/Callback",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const CALLBACK_HANDLER: oxjvm_vm::NativeClass = class(
    "javax/security/auth/callback/CallbackHandler",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[method(
        "handle",
        "([Ljavax/security/auth/callback/Callback;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    )],
    None,
);

const PASSWORD_CALLBACK_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "<init>",
        "(Ljava/lang/String;Z)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let prompt = cb_prompt(vm, ref_arg(args, 1))?;
            set_string_field(vm, this, "prompt", &prompt)?;
            crate::lang::write_int_field(vm, this, "echoOn", args[2].as_int());
            void()
        },
    ),
    method(
        "getPrompt",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "prompt",
                "Ljava/lang/String;",
            ))
        },
    ),
    method("isEchoOn", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "echoOn") != 0)
    }),
    method("setPassword", "([C)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let units = cb_read_chars(vm, ref_arg(args, 1))?;
        let copy = match units {
            Some(units) => cb_new_chars(vm, &units)?,
            None => ObjectRef::NULL,
        };
        set_ref_field(vm, this, "inputPassword", "[C", copy)?;
        void()
    }),
    method("getPassword", "()[C", ACC_PUBLIC, |vm, _, args| {
        let password = ref_field(vm, receiver(args), "inputPassword", "[C");
        let units = cb_read_chars(vm, password)?;
        match units {
            Some(units) => object(cb_new_chars(vm, &units)?),
            None => object(ObjectRef::NULL),
        }
    }),
    method("clearPassword", "()V", ACC_PUBLIC, |vm, _, args| {
        set_ref_field(vm, receiver(args), "inputPassword", "[C", ObjectRef::NULL)?;
        void()
    }),
];

pub(crate) const PASSWORD_CALLBACK: oxjvm_vm::NativeClass = class(
    "javax/security/auth/callback/PasswordCallback",
    Some("java/lang/Object"),
    &["javax/security/auth/callback/Callback"],
    ACC_PUBLIC,
    &[
        field("prompt", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("echoOn", "Z", ACC_PRIVATE, None),
        field("inputPassword", "[C", ACC_PRIVATE, None),
    ],
    &PASSWORD_CALLBACK_METHODS,
    None,
);

const NAME_CALLBACK_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let prompt = cb_prompt(vm, ref_arg(args, 1))?;
            set_string_field(vm, receiver(args), "prompt", &prompt)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let prompt = cb_prompt(vm, ref_arg(args, 1))?;
            set_string_field(vm, this, "prompt", &prompt)?;
            let default = ref_arg(args, 2);
            if !default.is_null() {
                let text = vm.string_or_empty(default);
                if text.is_empty() {
                    return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
                }
                set_string_field(vm, this, "defaultName", &text)?;
            }
            void()
        },
    ),
    method(
        "getPrompt",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "prompt",
                "Ljava/lang/String;",
            ))
        },
    ),
    method(
        "getDefaultName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "defaultName",
                "Ljava/lang/String;",
            ))
        },
    ),
    method(
        "setName",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            set_ref_field(
                vm,
                receiver(args),
                "name",
                "Ljava/lang/String;",
                ref_arg(args, 1),
            )?;
            void()
        },
    ),
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "name", "Ljava/lang/String;")),
    ),
];

pub(crate) const NAME_CALLBACK: oxjvm_vm::NativeClass = class(
    "javax/security/auth/callback/NameCallback",
    Some("java/lang/Object"),
    &["javax/security/auth/callback/Callback"],
    ACC_PUBLIC,
    &[
        field("prompt", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("defaultName", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &NAME_CALLBACK_METHODS,
    None,
);

const TEXT_INPUT_CALLBACK_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let prompt = cb_prompt(vm, ref_arg(args, 1))?;
            set_string_field(vm, receiver(args), "prompt", &prompt)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let prompt = cb_prompt(vm, ref_arg(args, 1))?;
            set_string_field(vm, this, "prompt", &prompt)?;
            let default = ref_arg(args, 2);
            if !default.is_null() {
                let text = vm.string_or_empty(default);
                if text.is_empty() {
                    return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
                }
                set_string_field(vm, this, "defaultText", &text)?;
            }
            void()
        },
    ),
    method(
        "getPrompt",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "prompt",
                "Ljava/lang/String;",
            ))
        },
    ),
    method(
        "getDefaultText",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "defaultText",
                "Ljava/lang/String;",
            ))
        },
    ),
    method(
        "setText",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            set_ref_field(
                vm,
                receiver(args),
                "text",
                "Ljava/lang/String;",
                ref_arg(args, 1),
            )?;
            void()
        },
    ),
    method(
        "getText",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "text", "Ljava/lang/String;")),
    ),
];

pub(crate) const TEXT_INPUT_CALLBACK: oxjvm_vm::NativeClass = class(
    "javax/security/auth/callback/TextInputCallback",
    Some("java/lang/Object"),
    &["javax/security/auth/callback/Callback"],
    ACC_PUBLIC,
    &[
        field("prompt", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("defaultText", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("text", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &TEXT_INPUT_CALLBACK_METHODS,
    None,
);

const TEXT_OUTPUT_CALLBACK_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method(
        "<init>",
        "(ILjava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let message_type = args[1].as_int();
            if !(0..=2).contains(&message_type) {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
            }
            let message = ref_arg(args, 2);
            if message.is_null() {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
            }
            crate::lang::write_int_field(vm, this, "messageType", message_type);
            set_ref_field(vm, this, "message", "Ljava/lang/String;", message)?;
            void()
        },
    ),
    method("getMessageType", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(
            vm,
            receiver(args),
            "messageType",
        ))
    }),
    method(
        "getMessage",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "message",
                "Ljava/lang/String;",
            ))
        },
    ),
];

pub(crate) const TEXT_OUTPUT_CALLBACK: oxjvm_vm::NativeClass = class(
    "javax/security/auth/callback/TextOutputCallback",
    Some("java/lang/Object"),
    &["javax/security/auth/callback/Callback"],
    ACC_PUBLIC,
    &[
        field("messageType", "I", ACC_PRIVATE, None),
        field("message", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &TEXT_OUTPUT_CALLBACK_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// javax.security.auth.Subject
// -------------------------------------------------------------------------------------------

/// Copy a collection into a fresh HashSet, rejecting null collections and null
/// elements with NullPointerException (the JDK's collectionNullClean).
fn subject_null_clean_set(vm: &mut Vm<'_>, collection: ObjectRef) -> Result<ObjectRef, VmError> {
    let elements = crate::util::collection_elements(vm, collection)?;
    let set = new_hash_set(vm)?;
    let (declaring, add) = vm.resolve_virtual_method(set, "add", "(Ljava/lang/Object;)Z")?;
    for element in elements {
        if element.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        vm.invoke_method(declaring, add, vec![Value::Ref(set), Value::Ref(element)])?;
    }
    Ok(set)
}

/// Bind `subject` as the current Subject, returning the previous binding so it
/// can be restored (the JDK uses a ScopedValue; oxjvm tracks a single binding).
fn subject_push(vm: &mut Vm<'_>, subject: ObjectRef) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("javax/security/auth/Subject")?;
    let old = vm
        .read_static_ref(class, "currentSubject", "Ljavax/security/auth/Subject;")
        .unwrap_or(ObjectRef::NULL);
    vm.set_static_value(
        class,
        "currentSubject",
        "Ljavax/security/auth/Subject;",
        Value::Ref(subject),
    );
    Ok(old)
}

fn subject_pop(vm: &mut Vm<'_>, old: ObjectRef) -> Result<(), VmError> {
    let class = vm.resolve_class("javax/security/auth/Subject")?;
    vm.set_static_value(
        class,
        "currentSubject",
        "Ljavax/security/auth/Subject;",
        Value::Ref(old),
    );
    Ok(())
}

/// `Subject.callAs`: run a Callable with the subject bound, wrapping any thrown
/// exception in a CompletionException.
fn subject_call_as(
    vm: &mut Vm<'_>,
    subject: ObjectRef,
    callable: ObjectRef,
) -> Result<Value, VmError> {
    if callable.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let old = subject_push(vm, subject)?;
    let (class, method) = vm.resolve_virtual_method(callable, "call", "()Ljava/lang/Object;")?;
    let result = vm.invoke_method(class, method, vec![Value::Ref(callable)]);
    subject_pop(vm, old)?;
    match result {
        Ok(value) => Ok(value),
        Err(VmError::Thrown(exc)) => {
            let ce_class = vm.resolve_class("java/util/concurrent/CompletionException")?;
            let ce = vm.new_instance(ce_class)?;
            let (declaring, ctor) = vm
                .find_method(ce_class, "<init>", "(Ljava/lang/Throwable;)V")
                .ok_or_else(|| VmError::internal("CompletionException lacks <init>"))?;
            vm.invoke_method(declaring, ctor, vec![Value::Ref(ce), Value::Ref(exc)])?;
            Err(VmError::Thrown(ce))
        }
        Err(error) => Err(error),
    }
}

const SUBJECT_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let principals = new_hash_set(vm)?;
        let pub_creds = new_hash_set(vm)?;
        let priv_creds = new_hash_set(vm)?;
        set_ref_field(vm, this, "principals", "Ljava/util/Set;", principals)?;
        set_ref_field(vm, this, "pubCredentials", "Ljava/util/Set;", pub_creds)?;
        set_ref_field(vm, this, "privCredentials", "Ljava/util/Set;", priv_creds)?;
        void()
    }),
    method(
        "<init>",
        "(ZLjava/util/Set;Ljava/util/Set;Ljava/util/Set;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let principals = subject_null_clean_set(vm, ref_arg(args, 2))?;
            let pub_creds = subject_null_clean_set(vm, ref_arg(args, 3))?;
            let priv_creds = subject_null_clean_set(vm, ref_arg(args, 4))?;
            set_ref_field(vm, this, "principals", "Ljava/util/Set;", principals)?;
            set_ref_field(vm, this, "pubCredentials", "Ljava/util/Set;", pub_creds)?;
            set_ref_field(vm, this, "privCredentials", "Ljava/util/Set;", priv_creds)?;
            crate::lang::write_int_field(vm, this, "readOnly", args[1].as_int());
            void()
        },
    ),
    method(
        "getPrincipals",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "principals",
                "Ljava/util/Set;",
            ))
        },
    ),
    method(
        "getPrincipals",
        "(Ljava/lang/Class;)Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let principals = ref_field(vm, this, "principals", "Ljava/util/Set;");
            let requested = ref_arg(args, 1);
            if requested.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let principal_class = vm.resolve_class("java/security/Principal")?;
            let object_class = vm.resolve_class("java/lang/Object")?;
            let principal_mirror = vm.class_object(principal_class)?;
            let object_mirror = vm.class_object(object_class)?;
            // For the broad Principal/Object views hand back the live set so adds
            // are reflected; narrower class views are filtered snapshots.
            if requested == principal_mirror || requested == object_mirror {
                return object(principals);
            }
            let filtered = new_hash_set(vm)?;
            let (declaring, add) =
                vm.resolve_virtual_method(filtered, "add", "(Ljava/lang/Object;)Z")?;
            let (ic, is_instance) =
                vm.resolve_virtual_method(requested, "isInstance", "(Ljava/lang/Object;)Z")?;
            for element in crate::util::collection_elements(vm, principals)? {
                let matches = vm
                    .invoke_method(
                        ic,
                        is_instance,
                        vec![Value::Ref(requested), Value::Ref(element)],
                    )?
                    .as_int()
                    != 0;
                if matches {
                    vm.invoke_method(
                        declaring,
                        add,
                        vec![Value::Ref(filtered), Value::Ref(element)],
                    )?;
                }
            }
            object(filtered)
        },
    ),
    method(
        "getPublicCredentials",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "pubCredentials",
                "Ljava/util/Set;",
            ))
        },
    ),
    method(
        "getPrivateCredentials",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "privCredentials",
                "Ljava/util/Set;",
            ))
        },
    ),
    method("setReadOnly", "()V", ACC_PUBLIC, |vm, _, args| {
        crate::lang::write_int_field(vm, receiver(args), "readOnly", 1);
        void()
    }),
    method("isReadOnly", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "readOnly") != 0)
    }),
    method(
        "current",
        "()Ljavax/security/auth/Subject;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("javax/security/auth/Subject")?;
            let current = vm
                .read_static_ref(class, "currentSubject", "Ljavax/security/auth/Subject;")
                .unwrap_or(ObjectRef::NULL);
            object(current)
        },
    ),
    method(
        "callAs",
        "(Ljavax/security/auth/Subject;Ljava/util/concurrent/Callable;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| subject_call_as(vm, ref_arg(args, 0), ref_arg(args, 1)),
    ),
    method(
        "doAs",
        "(Ljavax/security/auth/Subject;Ljava/security/PrivilegedAction;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let old = subject_push(vm, ref_arg(args, 0))?;
            let result = run_privileged_action(vm, ref_arg(args, 1), false);
            subject_pop(vm, old)?;
            result
        },
    ),
    method(
        "doAs",
        "(Ljavax/security/auth/Subject;Ljava/security/PrivilegedExceptionAction;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let old = subject_push(vm, ref_arg(args, 0))?;
            let result = run_privileged_action(vm, ref_arg(args, 1), true);
            subject_pop(vm, old)?;
            result
        },
    ),
    method(
        "doAsPrivileged",
        "(Ljavax/security/auth/Subject;Ljava/security/PrivilegedAction;Ljava/security/AccessControlContext;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let old = subject_push(vm, ref_arg(args, 0))?;
            let result = run_privileged_action(vm, ref_arg(args, 1), false);
            subject_pop(vm, old)?;
            result
        },
    ),
    method(
        "doAsPrivileged",
        "(Ljavax/security/auth/Subject;Ljava/security/PrivilegedExceptionAction;Ljava/security/AccessControlContext;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let old = subject_push(vm, ref_arg(args, 0))?;
            let result = run_privileged_action(vm, ref_arg(args, 1), true);
            subject_pop(vm, old)?;
            result
        },
    ),
];

pub(crate) const SUBJECT: oxjvm_vm::NativeClass = class(
    "javax/security/auth/Subject",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("principals", "Ljava/util/Set;", ACC_PRIVATE, None),
        field("pubCredentials", "Ljava/util/Set;", ACC_PRIVATE, None),
        field("privCredentials", "Ljava/util/Set;", ACC_PRIVATE, None),
        field("readOnly", "Z", ACC_PRIVATE, None),
        field(
            "currentSubject",
            "Ljavax/security/auth/Subject;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
    ],
    &SUBJECT_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// java.security.PKCS12Attribute
// -------------------------------------------------------------------------------------------

/// Parse a dotted-decimal OID into its components.
fn pkcs12_oid_components(name: &str) -> Option<Vec<u64>> {
    let mut components = Vec::new();
    for part in name.split('.') {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        components.push(part.parse::<u64>().ok()?);
    }
    if components.len() < 2 {
        return None;
    }
    Some(components)
}

/// Append one OID subidentifier in base-128 (most significant group first).
fn pkcs12_push_subidentifier(out: &mut Vec<u8>, value: u64) {
    let mut groups = Vec::new();
    let mut v = value;
    loop {
        groups.push((v & 0x7f) as u8);
        v >>= 7;
        if v == 0 {
            break;
        }
    }
    for i in (0..groups.len()).rev() {
        let mut byte = groups[i];
        if i != 0 {
            byte |= 0x80;
        }
        out.push(byte);
    }
}

fn pkcs12_encode_oid(components: &[u64]) -> Vec<u8> {
    let mut out = Vec::new();
    pkcs12_push_subidentifier(&mut out, 40 * components[0] + components[1]);
    for &c in &components[2..] {
        pkcs12_push_subidentifier(&mut out, c);
    }
    out
}

fn pkcs12_der_length(out: &mut Vec<u8>, len: usize) {
    if len < 0x80 {
        out.push(len as u8);
    } else {
        let mut bytes = Vec::new();
        let mut v = len;
        while v > 0 {
            bytes.push((v & 0xff) as u8);
            v >>= 8;
        }
        out.push(0x80 | bytes.len() as u8);
        for i in (0..bytes.len()).rev() {
            out.push(bytes[i]);
        }
    }
}

fn pkcs12_der_tlv(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(tag);
    pkcs12_der_length(&mut out, content.len());
    out.extend_from_slice(content);
    out
}

fn pkcs12_is_colon_hex(value: &str) -> bool {
    let parts: Vec<&str> = value.split(':').collect();
    parts.len() >= 2
        && parts
            .iter()
            .all(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn pkcs12_hex_pairs_to_bytes(value: &str) -> Vec<u8> {
    let mut bytes: Vec<u8> = value
        .split(':')
        .filter_map(|p| u8::from_str_radix(p, 16).ok())
        .collect();
    // BigInteger.toByteArray strips leading zero bytes.
    while bytes.len() > 1 && bytes[0] == 0 {
        bytes.remove(0);
    }
    bytes
}

/// DER-encode `SEQUENCE { OID, SET { value... } }` as the JDK does.
fn pkcs12_encode(components: &[u64], values: &[String]) -> Vec<u8> {
    let oid = pkcs12_der_tlv(0x06, &pkcs12_encode_oid(components));
    let mut set_content = Vec::new();
    for value in values {
        if pkcs12_is_colon_hex(value) {
            set_content.extend_from_slice(&pkcs12_der_tlv(0x04, &pkcs12_hex_pairs_to_bytes(value)));
        } else {
            set_content.extend_from_slice(&pkcs12_der_tlv(0x0c, value.as_bytes()));
        }
    }
    let set = pkcs12_der_tlv(0x31, &set_content);
    let mut seq_content = Vec::new();
    seq_content.extend_from_slice(&oid);
    seq_content.extend_from_slice(&set);
    pkcs12_der_tlv(0x30, &seq_content)
}

fn pkcs12_read_tlv(bytes: &[u8], pos: usize) -> Option<(u8, Vec<u8>, usize)> {
    if pos >= bytes.len() {
        return None;
    }
    let tag = bytes[pos];
    let mut p = pos + 1;
    if p >= bytes.len() {
        return None;
    }
    let len_byte = bytes[p];
    p += 1;
    let len = if len_byte < 0x80 {
        len_byte as usize
    } else {
        let n = (len_byte & 0x7f) as usize;
        if n == 0 || p + n > bytes.len() {
            return None;
        }
        let mut l = 0usize;
        for _ in 0..n {
            l = (l << 8) | bytes[p] as usize;
            p += 1;
        }
        l
    };
    if p + len > bytes.len() {
        return None;
    }
    Some((tag, bytes[p..p + len].to_vec(), p + len))
}

fn pkcs12_decode_oid(content: &[u8]) -> Option<String> {
    let mut subids = Vec::new();
    let mut value = 0u64;
    for &b in content {
        value = (value << 7) | (b & 0x7f) as u64;
        if b & 0x80 == 0 {
            subids.push(value);
            value = 0;
        }
    }
    if subids.is_empty() {
        return None;
    }
    let first = subids[0];
    let (c0, c1) = if first < 40 {
        (0, first)
    } else if first < 80 {
        (1, first - 40)
    } else {
        (2, first - 80)
    };
    let mut parts = vec![alloc::format!("{c0}"), alloc::format!("{c1}")];
    for &s in &subids[1..] {
        parts.push(alloc::format!("{s}"));
    }
    Some(parts.join("."))
}

fn pkcs12_bytes_to_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| alloc::format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn pkcs12_parse(bytes: &[u8]) -> Option<(String, String)> {
    let (tag, seq, _) = pkcs12_read_tlv(bytes, 0)?;
    if tag != 0x30 {
        return None;
    }
    let (oid_tag, oid_content, next) = pkcs12_read_tlv(&seq, 0)?;
    if oid_tag != 0x06 {
        return None;
    }
    let name = pkcs12_decode_oid(&oid_content)?;
    let (set_tag, set_content, _) = pkcs12_read_tlv(&seq, next)?;
    if set_tag != 0x31 {
        return None;
    }
    let mut values = Vec::new();
    let mut p = 0;
    while p < set_content.len() {
        let (vtag, vcontent, np) = pkcs12_read_tlv(&set_content, p)?;
        if vtag == 0x0c {
            values.push(String::from_utf8(vcontent).ok()?);
        } else if vtag == 0x04 {
            values.push(pkcs12_bytes_to_hex(&vcontent));
        } else {
            return None;
        }
        p = np;
    }
    let value = if values.len() == 1 {
        values[0].clone()
    } else {
        alloc::format!("[{}]", values.join(", "))
    };
    Some((name, value))
}

fn pkcs12_arrays_hash(bytes: &[u8]) -> i32 {
    let mut result = 1i32;
    for &b in bytes {
        result = result.wrapping_mul(31).wrapping_add((b as i8) as i32);
    }
    result
}

fn pkcs12_bytes_to_array(vm: &mut Vm<'_>, bytes: &[u8]) -> Result<ObjectRef, VmError> {
    let arr = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, bytes.len())?;
    for (i, &b) in bytes.iter().enumerate() {
        vm.array_set(arr, i as i32, Value::Int(b as i32), false)?;
    }
    Ok(arr)
}

fn pkcs12_store(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: &str,
    value: &str,
    encoded: &[u8],
) -> Result<(), VmError> {
    let class = vm.class_of(this);
    let (d, f) = vm.find_field(class, "name", "Ljava/lang/String;")?;
    let name_str = vm.make_string(name)?;
    vm.set_instance_ref(this, d, f, name_str);
    let (d, f) = vm.find_field(class, "value", "Ljava/lang/String;")?;
    let value_str = vm.make_string(value)?;
    vm.set_instance_ref(this, d, f, value_str);
    let (d, f) = vm.find_field(class, "encoded", "[B")?;
    let arr = pkcs12_bytes_to_array(vm, encoded)?;
    vm.set_instance_ref(this, d, f, arr);
    Ok(())
}

fn pkcs12_encoded_bytes(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Vec<u8>, VmError> {
    let encoded = vm
        .read_ref_field(this, vm.class_of(this), "encoded", "[B")
        .unwrap_or(ObjectRef::NULL);
    crate::lang::array_bytes(vm, encoded)
}

const PKCS12_ATTRIBUTE_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name_ref = ref_arg(args, 1);
            let value_ref = ref_arg(args, 2);
            if name_ref.is_null() || value_ref.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let name = vm.string_or_empty(name_ref);
            let value = vm.string_or_empty(value_ref);
            let components = match pkcs12_oid_components(&name) {
                Some(c) => c,
                None => {
                    return Err(vm.throw_new(
                        "java/lang/IllegalArgumentException",
                        Some("Incorrect format: name"),
                    ));
                }
            };
            let values: Vec<String> =
                if value.len() > 1 && value.starts_with('[') && value.ends_with(']') {
                    value[1..value.len() - 1]
                        .split(", ")
                        .map(String::from)
                        .collect()
                } else {
                    vec![value.clone()]
                };
            let encoded = pkcs12_encode(&components, &values);
            pkcs12_store(vm, this, &name, &value, &encoded)?;
            void()
        },
    ),
    method("<init>", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let encoded_ref = ref_arg(args, 1);
        if encoded_ref.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        let bytes = crate::lang::array_bytes(vm, encoded_ref)?;
        let (name, value) = match pkcs12_parse(&bytes) {
            Some(nv) => nv,
            None => {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("Incorrect format: encoded"),
                ));
            }
        };
        pkcs12_store(vm, this, &name, &value, &bytes)?;
        void()
    }),
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            object(
                vm.read_ref_field(this, vm.class_of(this), "name", "Ljava/lang/String;")
                    .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method(
        "getValue",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            object(
                vm.read_ref_field(this, vm.class_of(this), "value", "Ljava/lang/String;")
                    .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method("getEncoded", "()[B", ACC_PUBLIC, |vm, _, args| {
        let bytes = pkcs12_encoded_bytes(vm, receiver(args))?;
        object(pkcs12_bytes_to_array(vm, &bytes)?)
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
            let pkcs12 = vm.resolve_class("java/security/PKCS12Attribute")?;
            if other.is_null() || !vm.is_instance(other, pkcs12) {
                return boolean(false);
            }
            let a = pkcs12_encoded_bytes(vm, this)?;
            let b = pkcs12_encoded_bytes(vm, other)?;
            boolean(a == b)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let bytes = pkcs12_encoded_bytes(vm, receiver(args))?;
        int(pkcs12_arrays_hash(&bytes))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let name = vm
                .read_ref_field(this, class, "name", "Ljava/lang/String;")
                .unwrap_or(ObjectRef::NULL);
            let value = vm
                .read_ref_field(this, class, "value", "Ljava/lang/String;")
                .unwrap_or(ObjectRef::NULL);
            let text = alloc::format!("{}={}", vm.string_or_empty(name), vm.string_or_empty(value));
            object(vm.make_string(&text)?)
        },
    ),
];

pub(crate) const PKCS12_ATTRIBUTE: oxjvm_vm::NativeClass = class(
    "java/security/PKCS12Attribute",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("value", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("encoded", "[B", ACC_PRIVATE, None),
    ],
    &PKCS12_ATTRIBUTE_METHODS,
    None,
);
