//! Tier 2.2 shells: `java.security.*` and `java.security.*` collaborators.

use crate::class;
use oxjvm_classfile::flags::*;

pub(crate) const SECURITY: oxjvm_vm::NativeClass = class(
    "java/security/Security",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &[],
    None,
);

pub(crate) const PROVIDER: oxjvm_vm::NativeClass = class(
    "java/security/Provider",
    Some("java/util/Properties"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const KEY: oxjvm_vm::NativeClass = class(
    "java/security/Key",
    Some("java/io/Serializable"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE,
    &[],
    &[],
    None,
);

// Note: Key is an interface; represent as a marker-ish class.

pub(crate) const PUBLIC_KEY: oxjvm_vm::NativeClass = class(
    "java/security/PublicKey",
    Some("java/security/Key"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE,
    &[],
    &[],
    None,
);

pub(crate) const PRIVATE_KEY: oxjvm_vm::NativeClass = class(
    "java/security/PrivateKey",
    Some("java/security/Key"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE,
    &[],
    &[],
    None,
);

pub(crate) const KEY_PAIR: oxjvm_vm::NativeClass = class(
    "java/security/KeyPair",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &[],
    None,
);

pub(crate) const KEY_STORE: oxjvm_vm::NativeClass = class(
    "java/security/KeyStore",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

pub(crate) const SECURE_RANDOM: oxjvm_vm::NativeClass = class(
    "java/security/SecureRandom",
    Some("java/util/Random"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

pub(crate) const KEY_PAIR_GENERATOR: oxjvm_vm::NativeClass = class(
    "java/security/KeyPairGenerator",
    Some("java/security/KeyPairGeneratorSpi"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const KEY_PAIR_GENERATOR_SPI: oxjvm_vm::NativeClass = class(
    "java/security/KeyPairGeneratorSpi",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const CERTIFICATE_FACTORY: oxjvm_vm::NativeClass = class(
    "java/security/cert/CertificateFactory",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

// certificate base classes
pub(crate) const CERTIFICATE: oxjvm_vm::NativeClass = class(
    "java/security/cert/Certificate",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &[],
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

pub(crate) const X509_CERTIFICATE: oxjvm_vm::NativeClass = class(
    "java/security/cert/X509Certificate",
    Some("java/security/cert/Certificate"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const ACCESS_CONTROLLER: oxjvm_vm::NativeClass = class(
    "java/security/AccessController",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &[],
    None,
);

pub(crate) const PERMISSION: oxjvm_vm::NativeClass = class(
    "java/security/Permission",
    Some("java/lang/Object"),
    &["java/security/Guard", "java/io/Serializable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const BASIC_PERMISSION: oxjvm_vm::NativeClass = class(
    "java/security/BasicPermission",
    Some("java/security/Permission"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

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

pub(crate) const CODE_SOURCE: oxjvm_vm::NativeClass = class(
    "java/security/CodeSource",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

pub(crate) const PROTECTION_DOMAIN: oxjvm_vm::NativeClass = class(
    "java/security/ProtectionDomain",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
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
