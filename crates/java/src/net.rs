//! `java.net.InetAddress` / `Inet4Address`: literal IPv4 addresses and
//! `localhost` only (no real name resolution or sockets).

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use oxjvm_classfile::flags::*;

use crate::{boolean, class, field, int, method, object, receiver, ref_arg, string_arg, void};
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

fn parse_ipv4(text: &str) -> Option<[u8; 4]> {
    let parts: Vec<&str> = text.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut bytes = [0u8; 4];
    for (index, part) in parts.iter().enumerate() {
        let value: u32 = part.parse().ok()?;
        if value > 255 {
            return None;
        }
        bytes[index] = value as u8;
    }
    Some(bytes)
}

fn dotted(bytes: &[u8; 4]) -> String {
    alloc::format!("{}.{}.{}.{}", bytes[0], bytes[1], bytes[2], bytes[3])
}

fn bytes_array(vm: &mut Vm<'_>, bytes: &[u8]) -> Result<ObjectRef, VmError> {
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, bytes.len())?;
    for (index, byte) in bytes.iter().enumerate() {
        vm.array_set(array, index as i32, Value::Int(i32::from(*byte)), false)?;
    }
    Ok(array)
}

fn address_bytes(vm: &mut Vm<'_>, this: ObjectRef) -> [u8; 4] {
    let array = vm
        .read_ref_field(this, vm.class_of(this), "address", "[B")
        .unwrap_or(ObjectRef::NULL);
    let mut bytes = [0u8; 4];
    for (index, slot) in bytes.iter_mut().enumerate() {
        if let Ok(Value::Int(value)) = vm.array_get(array, index as i32) {
            *slot = value as u8;
        }
    }
    bytes
}

fn new_address(
    vm: &mut Vm<'_>,
    class_name: &str,
    bytes: [u8; 4],
    host: Option<&str>,
) -> Result<Value, VmError> {
    let class = vm.resolve_class(class_name)?;
    let instance = vm.new_instance(class)?;
    let array = bytes_array(vm, &bytes)?;
    let (declaring, field) = vm.find_field(class, "address", "[B")?;
    vm.set_instance_ref(instance, declaring, field, array);
    let text = dotted(&bytes);
    let (declaring, field) = vm.find_field(class, "hostAddress", "Ljava/lang/String;")?;
    let value = vm.make_string(&text)?;
    vm.set_instance_ref(instance, declaring, field, value);
    if let Some(host) = host {
        let (declaring, field) = vm.find_field(class, "hostName", "Ljava/lang/String;")?;
        let value = vm.make_string(host)?;
        vm.set_instance_ref(instance, declaring, field, value);
    }
    object(instance)
}

/// Resolve a literal IPv4 address, `localhost` or the empty string.
fn resolve_address(vm: &mut Vm<'_>, class_name: &str, text: &str) -> Result<Value, VmError> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("localhost") {
        return new_address(vm, class_name, [127, 0, 0, 1], Some(text));
    }
    if let Some(bytes) = parse_ipv4(trimmed) {
        return new_address(vm, class_name, bytes, None);
    }
    Err(vm.throw_new(
        "java/net/UnknownHostException",
        Some(&alloc::format!("{text}: Name or service not known")),
    ))
}

fn address_from_args(
    vm: &mut Vm<'_>,
    class_name: &str,
    host: Option<ObjectRef>,
    array: ObjectRef,
) -> Result<Value, VmError> {
    let length = vm.array_length(array)?;
    if length != 4 {
        return Err(vm.throw_new(
            "java/net/UnknownHostException",
            Some("Only IPv4 literals are supported"),
        ));
    }
    let mut bytes = [0u8; 4];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = vm.array_get(array, index as i32)?.as_int() as u8;
    }
    let host = host.and_then(|reference| vm.string_value(reference));
    new_address(vm, class_name, bytes, host.as_deref())
}

fn address_predicate(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    predicate: fn(&[u8; 4]) -> bool,
) -> Result<Value, VmError> {
    boolean(predicate(&address_bytes(vm, this)))
}

fn address_equals(vm: &mut Vm<'_>, args: &[Value], class_name: &str) -> Result<Value, VmError> {
    let other = ref_arg(args, 1);
    if other.is_null() {
        return boolean(false);
    }
    if vm.class_name(vm.class_of(other)) != class_name {
        // Allow Inet4Address/InetAddress comparisons via the address bytes.
        if !vm.class_name(vm.class_of(other)).ends_with("InetAddress")
            && !vm.class_name(vm.class_of(other)).ends_with("Inet4Address")
        {
            return boolean(false);
        }
    }
    boolean(address_bytes(vm, receiver(args)) == address_bytes(vm, other))
}

fn address_hash(vm: &mut Vm<'_>, this: ObjectRef) -> i32 {
    let bytes = address_bytes(vm, this);
    let mut hash = 0i32;
    for byte in bytes {
        hash = hash.wrapping_mul(31).wrapping_add(i32::from(byte));
    }
    hash
}

macro_rules! address_methods {
    ($constant:ident, $class_name:literal) => {
        const $constant: [oxjvm_vm::NativeMethodDef; 24] = [
            method("getAddress", "()[B", ACC_PUBLIC, |vm, _, args| {
                let bytes = address_bytes(vm, receiver(args));
                object(bytes_array(vm, &bytes)?)
            }),
            method(
                "getHostAddress",
                "()Ljava/lang/String;",
                ACC_PUBLIC,
                |vm, _, args| {
                    let text = vm
                        .read_string_field(
                            receiver(args),
                            vm.class_of(receiver(args)),
                            "hostAddress",
                        )
                        .unwrap_or_default();
                    object(vm.make_string(&text)?)
                },
            ),
            method(
                "getHostName",
                "()Ljava/lang/String;",
                ACC_PUBLIC,
                |vm, _, args| {
                    let this = receiver(args);
                    let name = vm
                        .read_string_field(this, vm.class_of(this), "hostName")
                        .unwrap_or_else(|| {
                            vm.read_string_field(this, vm.class_of(this), "hostAddress")
                                .unwrap_or_default()
                        });
                    object(vm.make_string(&name)?)
                },
            ),
            method(
                "getCanonicalHostName",
                "()Ljava/lang/String;",
                ACC_PUBLIC,
                |vm, _, args| {
                    let this = receiver(args);
                    let name = vm
                        .read_string_field(this, vm.class_of(this), "hostAddress")
                        .unwrap_or_default();
                    object(vm.make_string(&name)?)
                },
            ),
            method(
                "getHostString",
                "()Ljava/lang/String;",
                ACC_PUBLIC,
                |vm, _, args| {
                    let this = receiver(args);
                    let name = vm
                        .read_string_field(this, vm.class_of(this), "hostName")
                        .unwrap_or_else(|| {
                            vm.read_string_field(this, vm.class_of(this), "hostAddress")
                                .unwrap_or_default()
                        });
                    object(vm.make_string(&name)?)
                },
            ),
            method("isAnyLocalAddress", "()Z", ACC_PUBLIC, |vm, _, args| {
                address_predicate(vm, receiver(args), |bytes| *bytes == [0, 0, 0, 0])
            }),
            method("isLoopbackAddress", "()Z", ACC_PUBLIC, |vm, _, args| {
                address_predicate(vm, receiver(args), |bytes| bytes[0] == 127)
            }),
            method("isLinkLocalAddress", "()Z", ACC_PUBLIC, |vm, _, args| {
                address_predicate(vm, receiver(args), |bytes| {
                    bytes[0] == 169 && bytes[1] == 254
                })
            }),
            method("isSiteLocalAddress", "()Z", ACC_PUBLIC, |vm, _, args| {
                address_predicate(vm, receiver(args), |bytes| {
                    bytes[0] == 10 || (bytes[0] == 192 && bytes[1] == 168)
                })
            }),
            method("isMulticastAddress", "()Z", ACC_PUBLIC, |vm, _, args| {
                address_predicate(vm, receiver(args), |bytes| (224..=239).contains(&bytes[0]))
            }),
            method("isMCGlobal", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
            method("isMCLinkLocal", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
            method("isMCSiteLocal", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
            method("isMCOrgLocal", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
            method("isReachable", "(I)Z", ACC_PUBLIC, |vm, _, args| {
                boolean(address_bytes(vm, receiver(args))[0] == 127)
            }),
            method(
                "equals",
                "(Ljava/lang/Object;)Z",
                ACC_PUBLIC,
                |vm, _, args| address_equals(vm, args, $class_name),
            ),
            method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
                int(address_hash(vm, receiver(args)))
            }),
            method(
                "toString",
                "()Ljava/lang/String;",
                ACC_PUBLIC,
                |vm, _, args| {
                    let this = receiver(args);
                    let host = vm
                        .read_string_field(this, vm.class_of(this), "hostName")
                        .unwrap_or_default();
                    let address = dotted(&address_bytes(vm, this));
                    let text = if host.is_empty() {
                        address
                    } else {
                        alloc::format!("{host}/{address}")
                    };
                    object(vm.make_string(&text)?)
                },
            ),
            method(
                "getByName",
                concat!("(Ljava/lang/String;)L", $class_name, ";"),
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    let text = string_arg(vm, args[0])?;
                    resolve_address(vm, "java/net/Inet4Address", &text)
                },
            ),
            method(
                "getLocalHost",
                concat!("()L", $class_name, ";"),
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, _| {
                    new_address(
                        vm,
                        "java/net/Inet4Address",
                        [127, 0, 0, 1],
                        Some("localhost"),
                    )
                },
            ),
            method(
                "getLoopbackAddress",
                concat!("()L", $class_name, ";"),
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, _| {
                    new_address(
                        vm,
                        "java/net/Inet4Address",
                        [127, 0, 0, 1],
                        Some("localhost"),
                    )
                },
            ),
            method(
                "getAllByName",
                concat!("(Ljava/lang/String;)[L", $class_name, ";"),
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    let text = string_arg(vm, args[0])?;
                    let address = resolve_address(vm, "java/net/Inet4Address", &text)?;
                    let class = vm.resolve_class($class_name)?;
                    let array = vm.allocate_object_array(class, 1)?;
                    vm.array_set_ref(array, 0, address.as_ref())?;
                    object(array)
                },
            ),
            method(
                "getByAddress",
                concat!("(Ljava/lang/String;[B)L", $class_name, ";"),
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    address_from_args(
                        vm,
                        "java/net/Inet4Address",
                        Some(ref_arg(args, 0)),
                        ref_arg(args, 1),
                    )
                },
            ),
            method(
                "getByAddress",
                concat!("([B)L", $class_name, ";"),
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    address_from_args(vm, "java/net/Inet4Address", None, ref_arg(args, 0))
                },
            ),
        ];
    };
}

address_methods!(INET_ADDRESS_METHODS, "java/net/InetAddress");

pub(crate) const INET_ADDRESS: oxjvm_vm::NativeClass = class(
    "java/net/InetAddress",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[
        field("address", "[B", ACC_PRIVATE, None),
        field("hostName", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("hostAddress", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &INET_ADDRESS_METHODS,
    None,
);

const INET4_ADDRESS_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "getByName",
        "(Ljava/lang/String;)Ljava/net/Inet4Address;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = string_arg(vm, args[0])?;
            resolve_address(vm, "java/net/Inet4Address", &text)
        },
    ),
    method(
        "getAllByName",
        "(Ljava/lang/String;)[Ljava/net/InetAddress;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = string_arg(vm, args[0])?;
            let address = resolve_address(vm, "java/net/Inet4Address", &text)?;
            let class = vm.resolve_class("java/net/InetAddress")?;
            let array = vm.allocate_object_array(class, 1)?;
            vm.array_set_ref(array, 0, address.as_ref())?;
            object(array)
        },
    ),
    method(
        "getByAddress",
        "(Ljava/lang/String;[B)Ljava/net/Inet4Address;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            address_from_args(
                vm,
                "java/net/Inet4Address",
                Some(ref_arg(args, 0)),
                ref_arg(args, 1),
            )
        },
    ),
    method(
        "getByAddress",
        "([B)Ljava/net/Inet4Address;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| address_from_args(vm, "java/net/Inet4Address", None, ref_arg(args, 0)),
    ),
    method(
        "getLocalHost",
        "()Ljava/net/Inet4Address;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            new_address(
                vm,
                "java/net/Inet4Address",
                [127, 0, 0, 1],
                Some("localhost"),
            )
        },
    ),
    method(
        "getLoopbackAddress",
        "()Ljava/net/Inet4Address;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            new_address(
                vm,
                "java/net/Inet4Address",
                [127, 0, 0, 1],
                Some("localhost"),
            )
        },
    ),
];

pub(crate) const INET4_ADDRESS: oxjvm_vm::NativeClass = class(
    "java/net/Inet4Address",
    Some("java/net/InetAddress"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &INET4_ADDRESS_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// StandardProtocolFamily (enum-shaped)
// -------------------------------------------------------------------------------------------

const STANDARD_PROTOCOL_FAMILIES: [&str; 3] = ["INET", "INET6", "UNIX"];

fn standard_protocol_family_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/net/StandardProtocolFamily")?;
    for (index, name) in STANDARD_PROTOCOL_FAMILIES.iter().enumerate() {
        let instance = vm.new_instance(class)?;
        if let Some((declaring, field)) = vm.find_field_opt(class, "ordinal", "I") {
            vm.set_instance_int(instance, declaring, field, index as i32);
        }
        if let Some((declaring, field)) = vm.find_field_opt(class, "name", "Ljava/lang/String;") {
            let text = vm.make_string(name)?;
            vm.set_instance_ref(instance, declaring, field, text);
        }
        vm.set_static_value(
            class,
            name,
            "Ljava/net/StandardProtocolFamily;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

const STANDARD_PROTOCOL_FAMILY_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("ordinal", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "ordinal"))
    }),
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        object(
            vm.read_ref_field(
                receiver(args),
                vm.class_of(receiver(args)),
                "name",
                "Ljava/lang/String;",
            )
            .unwrap_or(ObjectRef::NULL),
        )
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(
                vm.read_ref_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "name",
                    "Ljava/lang/String;",
                )
                .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method(
        "values",
        "()[Ljava/net/StandardProtocolFamily;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/net/StandardProtocolFamily")?;
            let array = vm.allocate_object_array(class, STANDARD_PROTOCOL_FAMILIES.len())?;
            for (index, name) in STANDARD_PROTOCOL_FAMILIES.iter().enumerate() {
                if let Some(value) =
                    vm.read_static_ref_if_set(class, name, "Ljava/net/StandardProtocolFamily;")
                {
                    vm.array_set_ref(array, index, value)?;
                }
            }
            object(array)
        },
    ),
    method(
        "valueOf",
        "(Ljava/lang/String;)Ljava/net/StandardProtocolFamily;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = string_arg(vm, args[0])?;
            let class = vm.resolve_class("java/net/StandardProtocolFamily")?;
            if STANDARD_PROTOCOL_FAMILIES.contains(&name.as_str()) {
                if let Some(value) =
                    vm.read_static_ref_if_set(class, &name, "Ljava/net/StandardProtocolFamily;")
                {
                    return object(value);
                }
            }
            Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some(&alloc::format!(
                    "No enum constant java.net.StandardProtocolFamily.{name}"
                )),
            ))
        },
    ),
];

pub(crate) const STANDARD_PROTOCOL_FAMILY: oxjvm_vm::NativeClass = class(
    "java/net/StandardProtocolFamily",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("ordinal", "I", ACC_PRIVATE | ACC_FINAL, None),
        field("name", "Ljava/lang/String;", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "INET",
            "Ljava/net/StandardProtocolFamily;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "INET6",
            "Ljava/net/StandardProtocolFamily;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "UNIX",
            "Ljava/net/StandardProtocolFamily;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &STANDARD_PROTOCOL_FAMILY_METHODS,
    Some(standard_protocol_family_clinit),
);

// -------------------------------------------------------------------------------------------
// URL: a minimal URL holding its textual form (no protocol handlers or streams).
// -------------------------------------------------------------------------------------------

fn url_text(vm: &Vm<'_>, this: ObjectRef) -> String {
    vm.read_string_field(this, vm.class_of(this), "spec")
        .unwrap_or_default()
}

/// A protocol is present when a non-empty segment before `:` is free of `/`.
fn url_has_protocol(text: &str) -> bool {
    match text.find(':') {
        Some(index) => index > 0 && !text[..index].contains('/'),
        None => false,
    }
}

struct UrlParts<'a> {
    protocol: &'a str,
    authority: Option<&'a str>,
    path: &'a str,
    query: Option<&'a str>,
    reference: Option<&'a str>,
}

fn url_parts(text: &str) -> UrlParts<'_> {
    let (protocol, rest) = match text.find(':') {
        Some(index) => (&text[..index], &text[index + 1..]),
        None => ("", text),
    };
    let (authority, rest) = match rest.strip_prefix("//") {
        Some(after) => {
            let end = after.find(['/', '?', '#']).unwrap_or(after.len());
            (Some(&after[..end]), &after[end..])
        }
        None => (None, rest),
    };
    let (rest, reference) = match rest.find('#') {
        Some(index) => (&rest[..index], Some(&rest[index + 1..])),
        None => (rest, None),
    };
    let (path, query) = match rest.find('?') {
        Some(index) => (&rest[..index], Some(&rest[index + 1..])),
        None => (rest, None),
    };
    UrlParts {
        protocol,
        authority,
        path,
        query,
        reference,
    }
}

fn url_without_userinfo(authority: &str) -> &str {
    match authority.rfind('@') {
        Some(index) => &authority[index + 1..],
        None => authority,
    }
}

fn url_host(authority: &str) -> &str {
    let host = url_without_userinfo(authority);
    if let Some(rest) = host.strip_prefix('[') {
        return match rest.find(']') {
            Some(index) => &host[..index + 2],
            None => host,
        };
    }
    match host.rfind(':') {
        Some(index) => &host[..index],
        None => host,
    }
}

fn url_port(authority: &str) -> i32 {
    let host = url_without_userinfo(authority);
    let port = if let Some(rest) = host.strip_prefix('[') {
        match rest.find(']') {
            Some(index) => &rest[index + 1..],
            None => "",
        }
    } else {
        match host.rfind(':') {
            Some(index) => &host[index..],
            None => "",
        }
    };
    match port.strip_prefix(':') {
        Some(text) if !text.is_empty() => text.parse().unwrap_or(-1),
        _ => -1,
    }
}

const URL_METHODS: [oxjvm_vm::NativeMethodDef; 16] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = string_arg(vm, args[1])?;
            if !url_has_protocol(&text) {
                return Err(vm.throw_new(
                    "java/net/MalformedURLException",
                    Some(&alloc::format!("no protocol: {text}")),
                ));
            }
            let this = receiver(args);
            vm.set_string_field(this, vm.class_of(this), "spec", &text)?;
            void()
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = url_text(vm, receiver(args));
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toExternalForm",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = url_text(vm, receiver(args));
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "getProtocol",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = url_text(vm, receiver(args));
            object(vm.make_string(url_parts(&text).protocol)?)
        },
    ),
    method(
        "getAuthority",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = url_text(vm, receiver(args));
            match url_parts(&text).authority {
                Some(authority) => object(vm.make_string(authority)?),
                None => object(ObjectRef::NULL),
            }
        },
    ),
    method(
        "getUserInfo",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = url_text(vm, receiver(args));
            let userinfo =
                url_parts(&text)
                    .authority
                    .and_then(|authority| match authority.rfind('@') {
                        Some(index) if index > 0 => Some(&authority[..index]),
                        _ => None,
                    });
            match userinfo {
                Some(userinfo) => object(vm.make_string(userinfo)?),
                None => object(ObjectRef::NULL),
            }
        },
    ),
    method(
        "getHost",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = url_text(vm, receiver(args));
            // The JDK returns an empty host (not null) when there is no authority.
            let host = match url_parts(&text).authority {
                Some(authority) => String::from(url_host(authority)),
                None => String::new(),
            };
            object(vm.make_string(&host)?)
        },
    ),
    method("getPort", "()I", ACC_PUBLIC, |vm, _, args| {
        let text = url_text(vm, receiver(args));
        match url_parts(&text).authority {
            Some(authority) => int(url_port(authority)),
            None => int(-1),
        }
    }),
    method("getDefaultPort", "()I", ACC_PUBLIC, |_, _, _| int(-1)),
    method(
        "getPath",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = url_text(vm, receiver(args));
            object(vm.make_string(url_parts(&text).path)?)
        },
    ),
    method(
        "getQuery",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = url_text(vm, receiver(args));
            match url_parts(&text).query {
                Some(query) => object(vm.make_string(query)?),
                None => object(ObjectRef::NULL),
            }
        },
    ),
    method(
        "getRef",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = url_text(vm, receiver(args));
            match url_parts(&text).reference {
                Some(reference) => object(vm.make_string(reference)?),
                None => object(ObjectRef::NULL),
            }
        },
    ),
    method(
        "getFile",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = url_text(vm, receiver(args));
            let parts = url_parts(&text);
            let file = match parts.query {
                Some(query) => alloc::format!("{}?{query}", parts.path),
                None => String::from(parts.path),
            };
            object(vm.make_string(&file)?)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() || vm.class_of(receiver(args)) != vm.class_of(other) {
                return boolean(false);
            }
            boolean(url_text(vm, receiver(args)) == url_text(vm, other))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let text = url_text(vm, receiver(args));
        let mut hash = 0i32;
        for unit in text.encode_utf16() {
            hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
        }
        int(hash)
    }),
    method("toURI", "()Ljava/net/URI;", ACC_PUBLIC, |vm, _, args| {
        let text = url_text(vm, receiver(args));
        crate::uri::new_uri(vm, &text)
    }),
];

pub(crate) const URL: oxjvm_vm::NativeClass = class(
    "java/net/URL",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_FINAL,
    &[field("spec", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &URL_METHODS,
    None,
);

/// Create a `java.net.URL` for `text` (`URI.toURL` uses this).
pub(crate) fn new_url(vm: &mut Vm<'_>, text: &str) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/net/URL")?;
    let instance = vm.new_instance(class)?;
    let string = vm.make_string(text)?;
    let (declaring, ctor) = vm
        .find_method(class, "<init>", "(Ljava/lang/String;)V")
        .ok_or_else(|| VmError::internal("URL lacks its constructor"))?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![Value::Ref(instance), Value::Ref(string)],
    )?;
    Ok(instance)
}

// -------------------------------------------------------------------------------------------
// NetworkInterface
// -------------------------------------------------------------------------------------------

const NETWORK_INTERFACE_METHODS: [oxjvm_vm::NativeMethodDef; 18] = [
    method(
        "getByName",
        "(Ljava/lang/String;)Ljava/net/NetworkInterface;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            // Validate the argument like the JDK, but no interface is ever found.
            string_arg(vm, args[0])?;
            object(ObjectRef::NULL)
        },
    ),
    method(
        "getByInetAddress",
        "(Ljava/net/InetAddress;)Ljava/net/NetworkInterface;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(ObjectRef::NULL)
        },
    ),
    method(
        "getNetworkInterfaces",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(crate::util::new_enumeration(vm, &[])?),
    ),
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = vm
                .read_string_field(receiver(args), vm.class_of(receiver(args)), "name")
                .unwrap_or_default();
            object(vm.make_string(&name)?)
        },
    ),
    method(
        "getDisplayName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = vm
                .read_string_field(receiver(args), vm.class_of(receiver(args)), "name")
                .unwrap_or_default();
            object(vm.make_string(&name)?)
        },
    ),
    method("getIndex", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method("getMTU", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method("getHardwareAddress", "()[B", ACC_PUBLIC, |_, _, _| {
        object(ObjectRef::NULL)
    }),
    method("isUp", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("isLoopback", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("isPointToPoint", "()Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
    method("isVirtual", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("supportsMulticast", "()Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
    method(
        "getInetAddresses",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC,
        |vm, _, _| object(crate::util::new_enumeration(vm, &[])?),
    ),
    method(
        "getInterfaceAddresses",
        "()Ljava/util/List;",
        ACC_PUBLIC,
        |vm, _, _| object(crate::util::new_array_list(vm)?),
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = vm
                .read_string_field(receiver(args), vm.class_of(receiver(args)), "name")
                .unwrap_or_default();
            object(vm.make_string(&name)?)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        let name = vm
            .read_string_field(this, class, "name")
            .unwrap_or_default();
        let hash = name.chars().fold(0_i32, |hash, ch| {
            hash.wrapping_mul(31).wrapping_add(ch as i32)
        });
        int(hash)
    }),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() {
                return boolean(false);
            }
            let class = vm.resolve_class("java/net/NetworkInterface")?;
            if !vm.is_instance(other, class) {
                return boolean(false);
            }
            let this_name = vm
                .read_string_field(receiver(args), vm.class_of(receiver(args)), "name")
                .unwrap_or_default();
            let other_name = vm
                .read_string_field(other, vm.class_of(other), "name")
                .unwrap_or_default();
            boolean(this_name == other_name)
        },
    ),
];

/// `java.net.NetworkInterface` with no host interfaces; enough for reflective registration.
pub(crate) const NETWORK_INTERFACE: oxjvm_vm::NativeClass = class(
    "java/net/NetworkInterface",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[field("name", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &NETWORK_INTERFACE_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// URLEncoder / URLDecoder
// -------------------------------------------------------------------------------------------

/// `application/x-www-form-urlencoded` encoding: unreserved ASCII stays as-is, spaces become
/// `+`, and every other character is percent-encoded from its UTF-8 bytes.
fn url_encode(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '*' | '_' => out.push(ch),
            ' ' => out.push('+'),
            _ => {
                let mut buffer = [0u8; 4];
                for byte in ch.encode_utf8(&mut buffer).as_bytes() {
                    out.push('%');
                    out.push(nibble_hex(byte >> 4));
                    out.push(nibble_hex(byte & 0x0F));
                }
            }
        }
    }
    out
}

fn nibble_hex(value: u8) -> char {
    char::from_digit(u32::from(value), 16)
        .unwrap_or('0')
        .to_ascii_uppercase()
}

fn url_decode(vm: &mut Vm<'_>, text: &str) -> Result<Option<String>, VmError> {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' => {
                if index + 2 >= bytes.len() {
                    return Ok(None);
                }
                let Some(high) = char::from(bytes[index + 1]).to_digit(16) else {
                    return Ok(None);
                };
                let Some(low) = char::from(bytes[index + 2]).to_digit(16) else {
                    return Ok(None);
                };
                out.push(((high << 4) | low) as u8);
                index += 3;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    match core::str::from_utf8(&out) {
        Ok(decoded) => Ok(Some(String::from(decoded))),
        Err(_) => {
            let _ = vm;
            Ok(None)
        }
    }
}

const URL_ENCODER_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "encode",
    "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
    ACC_PUBLIC | ACC_STATIC,
    |vm, _, args| {
        let text = string_arg(vm, args[0])
            .map_err(|_| vm.throw_new("java/lang/NullPointerException", Some("charsetName")))?;
        let text = if text.is_empty() { String::new() } else { text };
        object(vm.make_string(&url_encode(&text))?)
    },
)];

pub(crate) const URL_ENCODER: oxjvm_vm::NativeClass = class(
    "java/net/URLEncoder",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &URL_ENCODER_METHODS,
    None,
);

const URL_DECODER_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "decode",
    "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
    ACC_PUBLIC | ACC_STATIC,
    |vm, _, args| {
        let text = string_arg(vm, args[0])
            .map_err(|_| vm.throw_new("java/lang/NullPointerException", None))?;
        match url_decode(vm, &text)? {
            Some(decoded) => object(vm.make_string(&decoded)?),
            None => Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("URLDecoder: Illegal hex characters in escape (%) pattern"),
            )),
        }
    },
)];

pub(crate) const URL_DECODER: oxjvm_vm::NativeClass = class(
    "java/net/URLDecoder",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &URL_DECODER_METHODS,
    None,
);
