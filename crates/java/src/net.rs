//! `java.net.InetAddress` / `Inet4Address`: literal IPv4 addresses and
//! `localhost` only (no real name resolution or sockets).

use alloc::string::String;
use alloc::vec::Vec;
use oxjvm_classfile::flags::*;

use crate::{boolean, class, field, int, method, object, receiver, ref_arg};
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
                    let text = crate::string_arg(vm, args[0])?;
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
                    let text = crate::string_arg(vm, args[0])?;
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
            let text = crate::string_arg(vm, args[0])?;
            resolve_address(vm, "java/net/Inet4Address", &text)
        },
    ),
    method(
        "getAllByName",
        "(Ljava/lang/String;)[Ljava/net/InetAddress;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = crate::string_arg(vm, args[0])?;
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
