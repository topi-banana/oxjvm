//! `java.net.URI`: a minimal URI holding its textual form.
//!
//! Enough for `File.toURI` and the usual `toString`/`getScheme`/`getPath`/
//! `isAbsolute` checks; no parsing beyond the scheme delimiter.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use oxjvm_classfile::flags::*;

use crate::{boolean, class, field, int, method, object, receiver, ref_arg, string_arg, void};
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

fn uri_text(vm: &Vm<'_>, this: ObjectRef) -> String {
    vm.read_string_field(this, vm.class_of(this), "string")
        .unwrap_or_default()
}

fn uri_scheme(text: &str) -> Option<String> {
    let index = text.find(':')?;
    Some(text[..index].into())
}

/// Percent-decode a URI path (`%20` becomes a space); malformed escapes stay as they are.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = core::str::from_utf8(&bytes[index + 1..index + 3])
                .ok()
                .and_then(|hex| u8::from_str_radix(hex, 16).ok());
            if let Some(byte) = hex {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn uri_init(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = string_arg(vm, args[1])?;
    let this = receiver(args);
    vm.set_string_field(this, vm.class_of(this), "string", &text)?;
    void()
}

const URI_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method("<init>", "(Ljava/lang/String;)V", ACC_PUBLIC, uri_init),
    method(
        "create",
        "(Ljava/lang/String;)Ljava/net/URI;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = string_arg(vm, args[0])?;
            let class = vm.resolve_class("java/net/URI")?;
            let instance = vm.new_instance(class)?;
            let (declaring, ctor) = vm
                .find_method(class, "<init>", "(Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("URI lacks its constructor"))?;
            let text = vm.make_string(&text)?;
            vm.invoke_method(
                declaring,
                ctor,
                vec![Value::Ref(instance), Value::Ref(text)],
            )?;
            object(instance)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = uri_text(vm, receiver(args));
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "getScheme",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = uri_text(vm, receiver(args));
            match uri_scheme(&text) {
                Some(scheme) => object(vm.make_string(&scheme)?),
                None => object(ObjectRef::NULL),
            }
        },
    ),
    method(
        "getPath",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = uri_text(vm, receiver(args));
            let path = match text.find(':') {
                Some(index) => &text[index + 1..],
                None => text.as_str(),
            };
            object(vm.make_string(&percent_decode(path))?)
        },
    ),
    method(
        "getRawPath",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = uri_text(vm, receiver(args));
            match text.find(':') {
                Some(index) if !text[index + 1..].is_empty() => {
                    object(vm.make_string(&text[index + 1..])?)
                }
                _ => object(ObjectRef::NULL),
            }
        },
    ),
    method("isAbsolute", "()Z", ACC_PUBLIC, |vm, _, args| {
        let text = uri_text(vm, receiver(args));
        boolean(uri_scheme(&text).is_some())
    }),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() || vm.class_of(receiver(args)) != vm.class_of(other) {
                return boolean(false);
            }
            boolean(uri_text(vm, receiver(args)) == uri_text(vm, other))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let text = uri_text(vm, receiver(args));
        let mut hash = 0i32;
        for unit in text.encode_utf16() {
            hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
        }
        int(hash)
    }),
    method(
        "toASCIIString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = uri_text(vm, receiver(args));
            object(vm.make_string(&text)?)
        },
    ),
    method("toURL", "()Ljava/net/URL;", ACC_PUBLIC, |vm, _, args| {
        let text = uri_text(vm, receiver(args));
        if uri_scheme(&text).is_none() {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("URI is not absolute"),
            ));
        }
        object(crate::net::new_url(vm, &text)?)
    }),
    method(
        "relativize",
        "(Ljava/net/URI;)Ljava/net/URI;",
        ACC_PUBLIC,
        |vm, _, args| {
            let base = uri_text(vm, receiver(args));
            let other = uri_text(vm, ref_arg(args, 1));
            let suffix = if !base.is_empty() && other.starts_with(&base) {
                String::from(&other[base.len()..])
            } else {
                other.clone()
            };
            let class = vm.resolve_class("java/net/URI")?;
            let instance = vm.new_instance(class)?;
            let text = vm.make_string(&suffix)?;
            let (declaring, field) = vm.find_field(class, "string", "Ljava/lang/String;")?;
            vm.set_instance_ref(instance, declaring, field, text);
            object(instance)
        },
    ),
];

pub(crate) const URI: oxjvm_vm::NativeClass = class(
    "java/net/URI",
    Some("java/lang/Object"),
    &["java/io/Serializable", "java/lang/Comparable"],
    ACC_PUBLIC | ACC_FINAL,
    &[field("string", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &URI_METHODS,
    None,
);

/// Create a `java.net.URI` for `text` (`File.toURI` uses this).
pub(crate) fn new_uri(vm: &mut Vm<'_>, text: &str) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/net/URI")?;
    let instance = vm.new_instance(class)?;
    let string = vm.make_string(text)?;
    let (declaring, ctor) = vm
        .find_method(class, "<init>", "(Ljava/lang/String;)V")
        .ok_or_else(|| VmError::internal("URI lacks its constructor"))?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![Value::Ref(instance), Value::Ref(string)],
    )?;
    object(instance)
}
