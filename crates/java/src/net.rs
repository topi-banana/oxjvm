//! `java.net.InetAddress` / `Inet4Address` / `Inet6Address`: literal IPv4
//! and IPv6 addresses and `localhost` only (no real name resolution or
//! sockets).

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use oxjvm_classfile::flags::*;

use crate::{
    boolean, class, field, int, int_arg, long, method, object, receiver, ref_arg, string_arg, void,
};
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

/// Parse an IPv4 literal the way the JDK's `textToNumericFormatV4` does
/// (inet_aton style): one to four decimal parts, where the last part fills
/// the remaining bytes ("126.1" -> 126.0.0.1, "256" -> 0.0.1.0). Leading
/// zeros are decimal, a single trailing dot is tolerated, the total length
/// is capped at 15 characters, and hex digits, signs and empty parts are
/// rejected.
fn parse_ipv4(text: &str) -> Option<[u8; 4]> {
    let bytes = text.as_bytes();
    let mut len = bytes.len();
    if len == 0 || len > 15 {
        return None;
    }
    if bytes[len - 1] == b'.' {
        len -= 1;
        if len == 0 || bytes[len - 1] == b'.' {
            return None;
        }
    }
    let mut res = [0u8; 4];
    let mut tmp: u64 = 0;
    let mut curr_byte = 0usize;
    let mut new_octet = true;
    for &c in &bytes[..len] {
        if c == b'.' {
            if new_octet || curr_byte == 3 || tmp > 0xff {
                return None;
            }
            res[curr_byte] = tmp as u8;
            curr_byte += 1;
            tmp = 0;
            new_octet = true;
        } else {
            if !c.is_ascii_digit() {
                return None;
            }
            tmp = tmp * 10 + u64::from(c - b'0');
            if tmp > 0xffff_ffff {
                return None;
            }
            new_octet = false;
        }
    }
    if new_octet {
        return None;
    }
    match curr_byte {
        0 => {
            res[0] = (tmp >> 24) as u8;
            res[1] = (tmp >> 16) as u8;
            res[2] = (tmp >> 8) as u8;
            res[3] = tmp as u8;
        }
        1 => {
            if tmp > 0xff_ffff {
                return None;
            }
            res[1] = (tmp >> 16) as u8;
            res[2] = (tmp >> 8) as u8;
            res[3] = tmp as u8;
        }
        2 => {
            if tmp > 0xffff {
                return None;
            }
            res[2] = (tmp >> 8) as u8;
            res[3] = tmp as u8;
        }
        3 => {
            if tmp > 0xff {
                return None;
            }
            res[3] = tmp as u8;
        }
        _ => return None,
    }
    Some(res)
}

/// Parse the dotted-quad tail of an IPv6 literal: exactly four decimal
/// parts, each 0..=255. Leading zeros are allowed ("0001" -> 1) but signs,
/// hex digits and empty parts are not.
fn parse_embedded_v4(text: &str) -> Option<[u8; 4]> {
    let mut bytes = [0u8; 4];
    let mut parts = text.split('.');
    for slot in &mut bytes {
        let part = parts.next()?;
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let value: u32 = part.parse().ok()?;
        if value > 255 {
            return None;
        }
        *slot = value as u8;
    }
    if parts.next().is_some() {
        return None;
    }
    Some(bytes)
}

/// Parse an IPv6 literal the way the JDK's `textToNumericFormatV6` does:
/// up to eight 16-bit hex groups, at most one `::` compression that must
/// stand for at least one zero group, an optional trailing dotted-quad
/// IPv4 part (counting as two groups) and an optional numeric `%scope`
/// suffix. Named scopes need a real network interface, so they fail.
fn parse_ipv6(text: &str) -> Option<([u8; 16], Option<u32>)> {
    if !text.contains(':') {
        return None;
    }
    let (addr, scope) = match text.split_once('%') {
        Some((addr, scope)) => {
            if scope.is_empty() || !scope.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            (addr, Some(scope.parse::<u32>().ok()?))
        }
        None => (text, None),
    };
    if !addr.contains(':') {
        return None;
    }
    let (left, right, compressed) = match addr.split_once("::") {
        Some((left, right)) => {
            if right.contains("::") || right.starts_with(':') {
                return None;
            }
            (left, right, true)
        }
        None => (addr, "", false),
    };
    let mut left_groups: Vec<u16> = Vec::new();
    let mut right_groups: Vec<u16> = Vec::new();
    let mut tail: Option<[u8; 4]> = None;
    parse_v6_side(left, &mut left_groups, &mut tail, !compressed)?;
    parse_v6_side(right, &mut right_groups, &mut tail, compressed)?;
    let tail_groups = usize::from(tail.is_some()) * 2;
    let total = left_groups.len() + right_groups.len() + tail_groups;
    if compressed {
        if total >= 8 {
            return None;
        }
    } else if total != 8 {
        return None;
    }
    let mut bytes = [0u8; 16];
    let mut index = 0;
    for group in &left_groups {
        bytes[index..index + 2].copy_from_slice(&group.to_be_bytes());
        index += 2;
    }
    // The right-hand groups (and any IPv4 tail) sit at the end of the
    // address; `::` fills everything in between with zeros. The tail is
    // four bytes, not two.
    let mut index = 16 - (right_groups.len() * 2 + usize::from(tail.is_some()) * 4);
    for group in &right_groups {
        bytes[index..index + 2].copy_from_slice(&group.to_be_bytes());
        index += 2;
    }
    if let Some(v4) = tail {
        bytes[12..16].copy_from_slice(&v4);
    }
    Some((bytes, scope))
}

/// Parse one side of an IPv6 literal into 16-bit hex groups. A dotted-quad
/// IPv4 part is only accepted as the very last piece of the whole address.
fn parse_v6_side(
    side: &str,
    groups: &mut Vec<u16>,
    tail: &mut Option<[u8; 4]>,
    allow_tail: bool,
) -> Option<()> {
    if side.is_empty() {
        return Some(());
    }
    let parts: Vec<&str> = side.split(':').collect();
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            return None;
        }
        if part.contains('.') {
            if index != parts.len() - 1 || !allow_tail || tail.is_some() {
                return None;
            }
            *tail = Some(parse_embedded_v4(part)?);
        } else {
            if part.len() > 4 || !part.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            groups.push(u16::from_str_radix(part, 16).ok()?);
        }
    }
    Some(())
}

/// The JDK renders IPv6 addresses as eight lowercase hex groups without
/// `::` compression (`numericToTextFormat`).
fn v6_text(bytes: &[u8]) -> String {
    let mut text = String::new();
    for index in 0..8 {
        if index > 0 {
            text.push(':');
        }
        let group = u16::from_be_bytes([bytes[2 * index], bytes[2 * index + 1]]);
        text.push_str(&alloc::format!("{group:x}"));
    }
    text
}

/// IPv4-mapped IPv6 addresses (`::ffff:a.b.c.d`) are served as plain
/// `Inet4Address` instances by the JDK.
fn mapped_v4(bytes: &[u8; 16]) -> Option<[u8; 4]> {
    if bytes[..10].iter().all(|&b| b == 0) && bytes[10] == 0xff && bytes[11] == 0xff {
        Some([bytes[12], bytes[13], bytes[14], bytes[15]])
    } else {
        None
    }
}

fn dotted(bytes: &[u8]) -> String {
    alloc::format!("{}.{}.{}.{}", bytes[0], bytes[1], bytes[2], bytes[3])
}

fn bytes_array(vm: &mut Vm<'_>, bytes: &[u8]) -> Result<ObjectRef, VmError> {
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, bytes.len())?;
    for (index, byte) in bytes.iter().enumerate() {
        vm.array_set(array, index as i32, Value::Int(i32::from(*byte)), false)?;
    }
    Ok(array)
}

fn address_bytes(vm: &mut Vm<'_>, this: ObjectRef) -> Vec<u8> {
    let array = vm
        .read_ref_field(this, vm.class_of(this), "address", "[B")
        .unwrap_or(ObjectRef::NULL);
    if array.is_null() {
        return Vec::new();
    }
    let length = vm.array_length(array).unwrap_or(0);
    let mut bytes = Vec::with_capacity(length);
    for index in 0..length {
        if let Ok(Value::Int(value)) = vm.array_get(array, index as i32) {
            bytes.push(value as u8);
        }
    }
    bytes
}

fn new_address(
    vm: &mut Vm<'_>,
    class_name: &str,
    bytes: &[u8],
    host: Option<&str>,
) -> Result<Value, VmError> {
    let class = vm.resolve_class(class_name)?;
    let instance = vm.new_instance(class)?;
    let array = bytes_array(vm, bytes)?;
    let (declaring, field) = vm.find_field(class, "address", "[B")?;
    vm.set_instance_ref(instance, declaring, field, array);
    let text = if bytes.len() == 16 {
        v6_text(bytes)
    } else {
        dotted(bytes)
    };
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

/// Create an `Inet6Address`, recording a numeric scope id (if any) both in
/// the `scope_id` field and as a `%N` suffix on the numeric host address.
fn new_inet6(
    vm: &mut Vm<'_>,
    bytes: &[u8; 16],
    scope: Option<u32>,
    host: Option<&str>,
) -> Result<Value, VmError> {
    let value = new_address(vm, "java/net/Inet6Address", bytes, host)?;
    if let Some(scope) = scope {
        let this = value.as_ref();
        let class = vm.class_of(this);
        let text = alloc::format!("{}%{}", v6_text(bytes), scope);
        let (declaring, field) = vm.find_field(class, "hostAddress", "Ljava/lang/String;")?;
        let string = vm.make_string(&text)?;
        vm.set_instance_ref(this, declaring, field, string);
        crate::lang::write_int_field(vm, this, "scope_id", scope as i32);
    }
    Ok(value)
}

/// Serve a parsed IPv6 literal: IPv4-mapped addresses become `Inet4Address`
/// (unless scoped, which the JDK rejects); everything else is `Inet6Address`.
fn v6_address(
    vm: &mut Vm<'_>,
    bytes: [u8; 16],
    scope: Option<u32>,
) -> Option<Result<Value, VmError>> {
    if let Some(v4) = mapped_v4(&bytes) {
        if scope.is_none() {
            return Some(new_address(vm, "java/net/Inet4Address", &v4, None));
        }
        return None;
    }
    Some(new_inet6(vm, &bytes, scope, None))
}

/// Resolve a literal IPv4 or IPv6 address, `localhost` or the empty string.
/// The JDK does not trim whitespace, so a padded host is not a valid literal.
pub(crate) fn resolve_address(
    vm: &mut Vm<'_>,
    class_name: &str,
    text: &str,
) -> Result<Value, VmError> {
    if text.is_empty() {
        // The JDK answers an empty host with the cached loopback instance,
        // whose host name is "localhost".
        return inet_loopback(vm);
    }
    if text.starts_with('[') {
        // A bracketed literal is IPv6 only; there is no name-resolution
        // fallback for it.
        if let Some(inner) = text.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
            if let Some((bytes, scope)) = parse_ipv6(inner) {
                if let Some(result) = v6_address(vm, bytes, scope) {
                    return result;
                }
            }
        }
        return Err(vm.throw_new(
            "java/net/UnknownHostException",
            Some(&alloc::format!("{text}: Name or service not known")),
        ));
    }
    if text.eq_ignore_ascii_case("localhost") {
        return new_address(vm, class_name, &[127, 0, 0, 1], Some(text));
    }
    if let Some(bytes) = parse_ipv4(text) {
        return new_address(vm, class_name, &bytes, None);
    }
    if let Some((bytes, scope)) = parse_ipv6(text) {
        if let Some(result) = v6_address(vm, bytes, scope) {
            return result;
        }
    }
    Err(vm.throw_new(
        "java/net/UnknownHostException",
        Some(&alloc::format!("{text}: Name or service not known")),
    ))
}

/// The JDK caches a single loopback instance: `getLoopbackAddress` and a null
/// host lookup return that same reference on every call.
fn inet_loopback(vm: &mut Vm<'_>) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/net/InetAddress")?;
    vm.ensure_initialized(class)?;
    if let Some(existing) = vm.read_static_ref_if_set(class, "loopback", "Ljava/net/InetAddress;") {
        return object(existing);
    }
    let address = new_address(
        vm,
        "java/net/Inet4Address",
        &[127, 0, 0, 1],
        Some("localhost"),
    )?;
    vm.set_static_value(class, "loopback", "Ljava/net/InetAddress;", address);
    Ok(address)
}

fn address_from_args(
    vm: &mut Vm<'_>,
    class_name: &str,
    host: Option<ObjectRef>,
    array: ObjectRef,
) -> Result<Value, VmError> {
    let length = if array.is_null() {
        0
    } else {
        vm.array_length(array)?
    };
    if length != 4 && length != 16 {
        return Err(vm.throw_new(
            "java/net/UnknownHostException",
            Some("addr is of illegal length"),
        ));
    }
    let mut bytes = vec![0u8; length];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = vm.array_get(array, index as i32)?.as_int() as u8;
    }
    let host = host.and_then(|reference| vm.string_value(reference));
    if length == 16 {
        let mut sixteen = [0u8; 16];
        sixteen.copy_from_slice(&bytes);
        if let Some(v4) = mapped_v4(&sixteen) {
            return new_address(vm, "java/net/Inet4Address", &v4, host.as_deref());
        }
        return new_inet6(vm, &sixteen, None, host.as_deref());
    }
    new_address(vm, class_name, &bytes, host.as_deref())
}

fn address_predicate(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    predicate: fn(&[u8]) -> bool,
) -> Result<Value, VmError> {
    boolean(predicate(&address_bytes(vm, this)))
}

/// The JDK compares the raw address bytes only: an `Inet4Address` never
/// equals an `Inet6Address` (even IPv4-compatible ones), and the scope id
/// of an `Inet6Address` is ignored.
fn address_equals(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let other = ref_arg(args, 1);
    if other.is_null() {
        return boolean(false);
    }
    let this = address_bytes(vm, receiver(args));
    let other = address_bytes(vm, other);
    boolean(!this.is_empty() && this == other)
}

/// `Inet4Address.hashCode` is the big-endian integer of the four address
/// bytes; `Inet6Address.hashCode` sums each (signed) byte shifted by its
/// position within a 32-bit word, matching the JDK exactly.
fn address_hash(vm: &mut Vm<'_>, this: ObjectRef) -> i32 {
    let bytes = address_bytes(vm, this);
    if bytes.len() == 16 {
        let mut hash = 0i32;
        for (index, byte) in bytes.iter().enumerate() {
            let signed = i32::from(*byte as i8);
            hash = hash.wrapping_add(signed.wrapping_shl(24 - 8 * (index % 4) as u32));
        }
        hash
    } else if bytes.len() == 4 {
        i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    } else {
        0
    }
}

macro_rules! address_methods {
    ($constant:ident, $class_name:literal) => {
        const $constant: [oxjvm_vm::NativeMethodDef; 25] = [
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
                address_predicate(vm, receiver(args), |bytes| {
                    !bytes.is_empty() && bytes.iter().all(|&b| b == 0)
                })
            }),
            method("isLoopbackAddress", "()Z", ACC_PUBLIC, |vm, _, args| {
                address_predicate(vm, receiver(args), |bytes| match bytes.len() {
                    16 => bytes[..15].iter().all(|&b| b == 0) && bytes[15] == 1,
                    _ => bytes.len() == 4 && bytes[0] == 127,
                })
            }),
            method("isLinkLocalAddress", "()Z", ACC_PUBLIC, |vm, _, args| {
                address_predicate(vm, receiver(args), |bytes| match bytes.len() {
                    16 => bytes[0] == 0xfe && (bytes[1] & 0xc0) == 0x80,
                    _ => bytes.len() == 4 && bytes[0] == 169 && bytes[1] == 254,
                })
            }),
            method("isSiteLocalAddress", "()Z", ACC_PUBLIC, |vm, _, args| {
                address_predicate(vm, receiver(args), |bytes| match bytes.len() {
                    16 => bytes[0] == 0xfe && (bytes[1] & 0xc0) == 0xc0,
                    _ => {
                        bytes.len() == 4
                            && (bytes[0] == 10
                                || (bytes[0] == 192 && bytes[1] == 168)
                                || (bytes[0] == 172 && (bytes[1] & 0xf0) == 16))
                    }
                })
            }),
            method("isMulticastAddress", "()Z", ACC_PUBLIC, |vm, _, args| {
                address_predicate(vm, receiver(args), |bytes| match bytes.len() {
                    16 => bytes[0] == 0xff,
                    _ => bytes.len() == 4 && (224..=239).contains(&bytes[0]),
                })
            }),
            method("isMCGlobal", "()Z", ACC_PUBLIC, |vm, _, args| {
                address_predicate(vm, receiver(args), |bytes| {
                    bytes.len() == 16 && bytes[0] == 0xff && (bytes[1] & 0x0f) == 0x0e
                })
            }),
            method("isMCNodeLocal", "()Z", ACC_PUBLIC, |vm, _, args| {
                address_predicate(vm, receiver(args), |bytes| {
                    bytes.len() == 16 && bytes[0] == 0xff && (bytes[1] & 0x0f) == 0x01
                })
            }),
            method("isMCLinkLocal", "()Z", ACC_PUBLIC, |vm, _, args| {
                address_predicate(vm, receiver(args), |bytes| {
                    bytes.len() == 16 && bytes[0] == 0xff && (bytes[1] & 0x0f) == 0x02
                })
            }),
            method("isMCSiteLocal", "()Z", ACC_PUBLIC, |vm, _, args| {
                address_predicate(vm, receiver(args), |bytes| {
                    bytes.len() == 16 && bytes[0] == 0xff && (bytes[1] & 0x0f) == 0x05
                })
            }),
            method("isMCOrgLocal", "()Z", ACC_PUBLIC, |vm, _, args| {
                address_predicate(vm, receiver(args), |bytes| {
                    bytes.len() == 16 && bytes[0] == 0xff && (bytes[1] & 0x0f) == 0x08
                })
            }),
            method("isReachable", "(I)Z", ACC_PUBLIC, |vm, _, args| {
                let bytes = address_bytes(vm, receiver(args));
                boolean(match bytes.len() {
                    16 => bytes[..15].iter().all(|&b| b == 0) && bytes[15] == 1,
                    _ => bytes.len() == 4 && bytes[0] == 127,
                })
            }),
            method(
                "equals",
                "(Ljava/lang/Object;)Z",
                ACC_PUBLIC,
                |vm, _, args| address_equals(vm, args),
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
                    let address = vm
                        .read_string_field(this, vm.class_of(this), "hostAddress")
                        .unwrap_or_default();
                    let text = alloc::format!("{host}/{address}");
                    object(vm.make_string(&text)?)
                },
            ),
            method(
                "getByName",
                concat!("(Ljava/lang/String;)L", $class_name, ";"),
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    if ref_arg(args, 0).is_null() {
                        return inet_loopback(vm);
                    }
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
                        &[127, 0, 0, 1],
                        Some("localhost"),
                    )
                },
            ),
            method(
                "getLoopbackAddress",
                concat!("()L", $class_name, ";"),
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, _| inet_loopback(vm),
            ),
            method(
                "getAllByName",
                concat!("(Ljava/lang/String;)[L", $class_name, ";"),
                ACC_PUBLIC | ACC_STATIC,
                |vm, _, args| {
                    let address = if ref_arg(args, 0).is_null() {
                        inet_loopback(vm)?
                    } else {
                        let text = string_arg(vm, args[0])?;
                        resolve_address(vm, "java/net/Inet4Address", &text)?
                    };
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
        field(
            "loopback",
            "Ljava/net/InetAddress;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
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
                &[127, 0, 0, 1],
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
                &[127, 0, 0, 1],
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
    ACC_PUBLIC | ACC_FINAL | ACC_ENUM,
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

/// The protocol named at the start of `text`, following the JDK's syntax: a
/// letter then letters/digits/`+`/`-`/`.` up to a `:`. Anything else (a `/`,
/// `?`, `#`, or a digit first) means no protocol is present.
fn url_protocol_prefix(text: &str) -> Option<&str> {
    let mut chars = text.char_indices();
    let (_, first) = chars.next()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    for (index, ch) in chars {
        if ch == ':' {
            return Some(&text[..index]);
        }
        if !(ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.')) {
            return None;
        }
    }
    None
}

/// The protocols with a built-in stream handler; others raise
/// `MalformedURLException: unknown protocol`.
fn url_known_protocol(protocol: &str) -> bool {
    ["http", "https", "ftp", "file", "jar", "mailto", "jrt"]
        .iter()
        .any(|known| known.eq_ignore_ascii_case(protocol))
}

/// Validate an absolute URL spec and lowercase its protocol, as the JDK does.
fn url_absolute_spec(vm: &mut Vm<'_>, text: &str) -> Result<String, VmError> {
    let Some(protocol) = url_protocol_prefix(text) else {
        return Err(vm.throw_new(
            "java/net/MalformedURLException",
            Some(&alloc::format!("no protocol: {text}")),
        ));
    };
    if !url_known_protocol(protocol) {
        return Err(vm.throw_new(
            "java/net/MalformedURLException",
            Some(&alloc::format!("unknown protocol: {protocol}")),
        ));
    }
    let spec = alloc::format!(
        "{}{}",
        protocol.to_ascii_lowercase(),
        &text[protocol.len()..]
    );
    let spec = url_normalize_file_authority(&spec);
    url_validate_authority(vm, &spec)?;
    Ok(spec)
}

/// The JDK's file handler drops an empty authority: `file:///path` becomes
/// `file:/path` (likewise before a `?` query or `#` fragment).
fn url_normalize_file_authority(spec: &str) -> String {
    let Some(rest) = spec.strip_prefix("file://") else {
        return String::from(spec);
    };
    match rest.chars().next() {
        None | Some('/') | Some('?') | Some('#') => alloc::format!("file:{rest}"),
        _ => String::from(spec),
    }
}

struct UrlParts<'a> {
    protocol: &'a str,
    authority: Option<&'a str>,
    path: &'a str,
    query: Option<&'a str>,
    reference: Option<&'a str>,
}

/// Resolve a possibly relative URL against a base URL (RFC 3986 style, simplified).
/// Resolve `relative` against the base URL, following `URLStreamHandler.parseURL`:
/// an empty spec keeps the base, a fragment-only spec keeps the base path and
/// query, a query-only spec keeps the base directory, and a same-protocol spec
/// is parsed relative to the base.
fn resolve_url(vm: &mut Vm<'_>, base: ObjectRef, relative: &str) -> Result<String, VmError> {
    let original = url_text(vm, base);
    if relative.is_empty() {
        return Ok(original);
    }
    if relative.starts_with('#') {
        let stem = match original.find('#') {
            Some(index) => &original[..index],
            None => &original[..],
        };
        return Ok(alloc::format!("{stem}{relative}"));
    }
    let base_protocol = String::from(url_parts(&original).protocol);
    if let Some(protocol) = url_protocol_prefix(relative) {
        if protocol.eq_ignore_ascii_case(&base_protocol) {
            return resolve_url(vm, base, &relative[protocol.len() + 1..]);
        }
        if url_known_protocol(protocol) {
            return Ok(alloc::format!(
                "{}{}",
                protocol.to_ascii_lowercase(),
                &relative[protocol.len()..]
            ));
        }
        return Err(vm.throw_new(
            "java/net/MalformedURLException",
            Some(&alloc::format!("unknown protocol: {protocol}")),
        ));
    }
    // A jar base keeps its inner URL and `!/` separator intact: the relative
    // spec resolves against the entry path that follows the separator.
    if base_protocol == "jar" {
        if let Some(index) = original.find("!/") {
            let prefix = &original[..index + 1];
            let entry = &original[index + 2..];
            let entry_path = match entry.find(['?', '#']) {
                Some(end) => &entry[..end],
                None => entry,
            };
            // The entry path always sits under a leading `/` after the `!/`.
            let directory = if entry_path.is_empty() {
                String::from("/")
            } else {
                match entry_path.rfind('/') {
                    Some(end) => alloc::format!("/{}/", &entry_path[..end]),
                    None => String::from("/"),
                }
            };
            let (rel_path, suffix) = match relative.find(['?', '#']) {
                Some(end) => (&relative[..end], &relative[end..]),
                None => (relative, ""),
            };
            let merged = if rel_path.starts_with('/') {
                url_normalize_rel_path(rel_path)
            } else {
                url_normalize_rel_path(&alloc::format!("{directory}{rel_path}"))
            };
            return Ok(alloc::format!("{prefix}{merged}{suffix}"));
        }
    }
    // A parts-constructed base keeps its host and file apart; merge against a
    // slash-prefixed file so the host is not swallowed into the path.
    let merge_base = match vm.read_string_field(base, vm.class_of(base), "partsHost") {
        Some(host) if !host.is_empty() => {
            let port = crate::lang::read_int_field(vm, base, "partsPort");
            let file = vm
                .read_string_field(base, vm.class_of(base), "partsFile")
                .unwrap_or_default();
            let file = if file.starts_with('/') {
                file
            } else {
                alloc::format!("/{file}")
            };
            url_from_parts(&base_protocol, &host, port, &file)
        }
        _ => original.clone(),
    };
    let parts = url_parts(&merge_base);
    if relative.starts_with("//") {
        return Ok(alloc::format!("{}:{relative}", parts.protocol));
    }
    if relative.starts_with('/') {
        return Ok(match parts.authority {
            Some(authority) => alloc::format!("{}://{authority}{relative}", parts.protocol),
            None => alloc::format!("{}:{relative}", parts.protocol),
        });
    }
    let query_only = relative.starts_with('?');
    let directory = match parts.path.rfind('/') {
        Some(index) => &parts.path[..=index],
        None if query_only || parts.authority.is_some() => "/",
        None => "",
    };
    let (rel_path, suffix) = match relative.find(['?', '#']) {
        Some(index) => (&relative[..index], &relative[index..]),
        None => (relative, ""),
    };
    let merged = url_normalize_rel_path(&alloc::format!("{directory}{rel_path}"));
    Ok(match parts.authority {
        Some(authority) => alloc::format!("{}://{authority}{merged}{suffix}", parts.protocol),
        None => alloc::format!("{}:{merged}{suffix}", parts.protocol),
    })
}

/// Normalize a merged relative path exactly as `URLStreamHandler.parseURL` does
/// for relative paths: drop embedded `/./` and `/../` (keeping leading `/../`),
/// then a trailing `/..` or `/.` — unlike RFC 3986, a trailing slash survives.
fn url_normalize_rel_path(path: &str) -> String {
    let mut path = String::from(path);
    // RFC 3986 remove_dot_segments drops a leading "./" segment.
    while let Some(stripped) = path.strip_prefix("./") {
        path = String::from(stripped);
    }
    while let Some(index) = path.find("/./") {
        path = alloc::format!("{}{}", &path[..index], &path[index + 2..]);
    }
    let mut start = 0;
    while let Some(found) = path[start..].find("/../") {
        let index = start + found;
        if index > 0 {
            if let Some(limit) = path[..index].rfind('/') {
                if path[limit..].find("/../") != Some(0) {
                    path = alloc::format!("{}{}", &path[..limit], &path[index + 3..]);
                    start = 0;
                    continue;
                }
            }
        }
        start = index + 3;
    }
    while path.ends_with("/..") {
        let Some(index) = path.find("/..") else { break };
        if index == 0 {
            break;
        }
        match path[..index].rfind('/') {
            Some(limit) => path = alloc::format!("{}/", &path[..limit]),
            None => break,
        }
    }
    if path.ends_with("/.") {
        path.pop();
    }
    path
}

/// Store a parts-constructed URL: the spec keeps the JDK's exact external form
/// (a file without a leading `/` is not given one), while the host/port/file
/// fields keep the components apart for getters and relative resolution.
fn url_set_parts(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    protocol: &str,
    host: &str,
    port: i32,
    file: &str,
) -> Result<(), VmError> {
    let protocol = protocol.to_ascii_lowercase();
    if port < -1 {
        return Err(vm.throw_new(
            "java/net/MalformedURLException",
            Some(&alloc::format!("Port out of range: {port}")),
        ));
    }
    // A bare IPv6 literal host is stored bracketed, like the JDK does.
    let bracketed;
    let host = if host.contains(':') && !host.starts_with('[') {
        bracketed = alloc::format!("[{host}]");
        &bracketed
    } else {
        host
    };
    let class = vm.class_of(this);
    let spec = url_from_parts(&protocol, host, port, file);
    vm.set_string_field(this, class, "spec", &spec)?;
    let host_ref = vm.make_string(host)?;
    let (declaring, field) = vm.find_field(class, "partsHost", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, host_ref);
    crate::lang::write_int_field(vm, this, "partsPort", port);
    let file_ref = vm.make_string(file)?;
    let (declaring, field) = vm.find_field(class, "partsFile", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, file_ref);
    Ok(())
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

/// The JDK's ftp handler percent-decodes the authority when opening a
/// connection; a `%` not followed by two hex digits is a malformed escape
/// pair and raises MalformedURLException. Other protocols do not perform
/// this check at openConnection time.
fn url_check_ftp_authority(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    let text = url_text(vm, this);
    let parts = url_parts(&text);
    if !parts.protocol.eq_ignore_ascii_case("ftp") {
        return Ok(());
    }
    let Some(authority) = parts.authority else {
        return Ok(());
    };
    let bytes = authority.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let valid = index + 2 < bytes.len()
                && bytes[index + 1].is_ascii_hexdigit()
                && bytes[index + 2].is_ascii_hexdigit();
            if !valid {
                return Err(vm.throw_new(
                    "java/net/MalformedURLException",
                    Some("Malformed escape pair: %"),
                ));
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    Ok(())
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

/// Validate a numeric port the way the JDK's URL parser does: it must parse as
/// an int and not be below -1 (which stands for the protocol default).
fn url_validate_port(vm: &mut Vm<'_>, text: &str) -> Result<(), VmError> {
    match text.parse::<i32>() {
        Ok(port) if port >= -1 => Ok(()),
        Ok(port) => Err(vm.throw_new(
            "java/net/MalformedURLException",
            Some(&alloc::format!("Port out of range: {port}")),
        )),
        Err(_) => Err(vm.throw_new(
            "java/net/MalformedURLException",
            Some(&alloc::format!("Invalid port: {text}")),
        )),
    }
}

/// Validate the authority of an absolute URL spec the way `URLStreamHandler.parseURL`
/// does: a bracketed host must be an IPv6 literal, anything after the closing bracket
/// must be a `:port` suffix, and a numeric port must not be below -1.
fn url_validate_authority(vm: &mut Vm<'_>, spec: &str) -> Result<(), VmError> {
    let Some(authority) = url_parts(spec).authority else {
        return Ok(());
    };
    let host = url_without_userinfo(authority);
    if let Some(rest) = host.strip_prefix('[') {
        let Some(close) = rest.find(']') else {
            return Err(vm.throw_new(
                "java/net/MalformedURLException",
                Some(&alloc::format!("Invalid authority field: {authority}")),
            ));
        };
        let inner = &rest[..close];
        if !inner.contains(':')
            || !inner
                .chars()
                .all(|ch| ch.is_ascii_hexdigit() || matches!(ch, ':' | '.'))
        {
            return Err(vm.throw_new(
                "java/net/MalformedURLException",
                Some(&alloc::format!("Invalid host: {host}")),
            ));
        }
        let after = &rest[close + 1..];
        if !after.is_empty() {
            let Some(port_text) = after.strip_prefix(':') else {
                return Err(vm.throw_new(
                    "java/net/MalformedURLException",
                    Some(&alloc::format!("Invalid authority field: {authority}")),
                ));
            };
            url_validate_port(vm, port_text)?;
        }
    } else if let Some(index) = host.rfind(':') {
        let port_text = &host[index + 1..];
        if !port_text.is_empty() {
            url_validate_port(vm, port_text)?;
        }
    }
    Ok(())
}

/// Build a URL spec from protocol/host/port/file parts, matching the JDK's
/// `URL(protocol, host, port, file)` constructors (an empty host drops the `//`).
fn url_from_parts(protocol: &str, host: &str, port: i32, file: &str) -> String {
    if host.is_empty() {
        return alloc::format!("{protocol}:{file}");
    }
    if port >= 0 {
        alloc::format!("{protocol}://{host}:{port}{file}")
    } else {
        alloc::format!("{protocol}://{host}{file}")
    }
}

const URL_METHODS: [oxjvm_vm::NativeMethodDef; 23] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = string_arg(vm, args[1])?;
            let text = url_absolute_spec(vm, &text)?;
            let this = receiver(args);
            vm.set_string_field(this, vm.class_of(this), "spec", &text)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/net/URL;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let base = ref_arg(args, 1);
            let relative = string_arg(vm, args[2])?;
            let text = if base.is_null() {
                url_absolute_spec(vm, &relative)?
            } else {
                resolve_url(vm, base, &relative)?
            };
            let this = receiver(args);
            vm.set_string_field(this, vm.class_of(this), "spec", &text)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let protocol = string_arg(vm, args[1])?;
            let host_null = ref_arg(args, 2).is_null();
            let host = if host_null {
                String::new()
            } else {
                string_arg(vm, args[2])?
            };
            let file = string_arg(vm, args[3])?;
            let this = receiver(args);
            url_set_parts(vm, this, &protocol, &host, -1, &file)?;
            crate::lang::write_int_field(vm, this, "partsHostNull", i32::from(host_null));
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;ILjava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let protocol = string_arg(vm, args[1])?;
            let host_null = ref_arg(args, 2).is_null();
            let host = if host_null {
                String::new()
            } else {
                string_arg(vm, args[2])?
            };
            // A null host drops the authority, so the port is not retained.
            let port = if host_null { -1 } else { int_arg(args, 3) };
            let file = string_arg(vm, args[4])?;
            let this = receiver(args);
            url_set_parts(vm, this, &protocol, &host, port, &file)?;
            crate::lang::write_int_field(vm, this, "partsHostNull", i32::from(host_null));
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
            let this = receiver(args);
            if let Some(host) = vm.read_string_field(this, vm.class_of(this), "partsHost") {
                if host.is_empty() {
                    return object(ObjectRef::NULL);
                }
                let port = crate::lang::read_int_field(vm, this, "partsPort");
                let authority = if port >= 0 {
                    alloc::format!("{host}:{port}")
                } else {
                    host
                };
                return object(vm.make_string(&authority)?);
            }
            let text = url_text(vm, this);
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
            let this = receiver(args);
            // A URL built with an explicit null host keeps a null host, unlike
            // a URL parsed from a string with no authority (empty host).
            if crate::lang::read_int_field(vm, this, "partsHostNull") == 1 {
                return object(ObjectRef::NULL);
            }
            if let Some(host) = vm.read_string_field(this, vm.class_of(this), "partsHost") {
                return object(vm.make_string(&host)?);
            }
            let text = url_text(vm, this);
            // The JDK returns an empty host (not null) when there is no authority.
            let host = match url_parts(&text).authority {
                Some(authority) => String::from(url_host(authority)),
                None => String::new(),
            };
            object(vm.make_string(&host)?)
        },
    ),
    method("getPort", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if vm
            .read_string_field(this, vm.class_of(this), "partsHost")
            .is_some()
        {
            return int(crate::lang::read_int_field(vm, this, "partsPort"));
        }
        let text = url_text(vm, this);
        match url_parts(&text).authority {
            Some(authority) => int(url_port(authority)),
            None => int(-1),
        }
    }),
    method("getDefaultPort", "()I", ACC_PUBLIC, |vm, _, args| {
        let text = url_text(vm, receiver(args));
        let protocol = text.split_once(':').map_or("", |(protocol, _)| protocol);
        int(match protocol {
            "http" => 80,
            "https" => 443,
            "ftp" => 21,
            _ => -1,
        })
    }),
    method(
        "getPath",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if let Some(file) = vm.read_string_field(this, vm.class_of(this), "partsFile") {
                let path = file.split_once('?').map_or(file.as_str(), |(path, _)| path);
                return object(vm.make_string(path)?);
            }
            let text = url_text(vm, this);
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
            let this = receiver(args);
            if let Some(file) = vm.read_string_field(this, vm.class_of(this), "partsFile") {
                return object(vm.make_string(&file)?);
            }
            let text = url_text(vm, this);
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
    method(
        "sameFile",
        "(Ljava/net/URL;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() {
                return boolean(false);
            }
            let left_text = url_text(vm, receiver(args));
            let right_text = url_text(vm, other);
            let left = url_parts(&left_text);
            let right = url_parts(&right_text);
            boolean(
                left.protocol == right.protocol
                    && left.authority == right.authority
                    && left.path == right.path,
            )
        },
    ),
    method(
        "openConnection",
        "()Ljava/net/URLConnection;",
        ACC_PUBLIC,
        |vm, _, args| {
            url_check_ftp_authority(vm, receiver(args))?;
            let class = vm.resolve_class("java/net/URLConnection")?;
            let connection = vm.new_instance(class)?;
            let (declaring, field) = vm.find_field(class, "url", "Ljava/net/URL;")?;
            vm.set_instance_ref(connection, declaring, field, receiver(args));
            object(connection)
        },
    ),
    method(
        "openConnection",
        "(Ljava/net/Proxy;)Ljava/net/URLConnection;",
        ACC_PUBLIC,
        |vm, _, args| {
            url_check_ftp_authority(vm, receiver(args))?;
            let class = vm.resolve_class("java/net/URLConnection")?;
            let connection = vm.new_instance(class)?;
            let (declaring, field) = vm.find_field(class, "url", "Ljava/net/URL;")?;
            vm.set_instance_ref(connection, declaring, field, receiver(args));
            object(connection)
        },
    ),
    method(
        "openStream",
        "()Ljava/io/InputStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = url_text(vm, receiver(args));
            let parts = url_parts(&text);
            if parts.protocol != "file" {
                return Err(vm.throw_new(
                    "java/io/IOException",
                    Some(&alloc::format!(
                        "protocol not supported: {}",
                        parts.protocol
                    )),
                ));
            }
            let class = vm.resolve_class("java/io/FileInputStream")?;
            let stream = vm.new_instance(class)?;
            let (declaring, ctor) = vm
                .find_method(class, "<init>", "(Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("FileInputStream(String) missing"))?;
            let path = vm.make_string(parts.path)?;
            vm.invoke_method(declaring, ctor, vec![Value::Ref(stream), Value::Ref(path)])?;
            object(stream)
        },
    ),
];

pub(crate) const URL: oxjvm_vm::NativeClass = class(
    "java/net/URL",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("spec", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("partsHost", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("partsHostNull", "I", ACC_PRIVATE, None),
        field("partsPort", "I", ACC_PRIVATE, None),
        field("partsFile", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
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

const NETWORK_INTERFACE_METHODS: [oxjvm_vm::NativeMethodDef; 19] = [
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
        "getByIndex",
        "(I)Ljava/net/NetworkInterface;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if int_arg(args, 0) < 0 {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
            }
            // No host interface is ever found, so any valid index yields null.
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

/// Decode a URL-encoded string, reporting the JDK's exact error messages;
/// malformed UTF-8 byte sequences become replacement characters, not errors.
fn url_decode(text: &str) -> Result<String, String> {
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
                    return Err(String::from(
                        "URLDecoder: Incomplete trailing escape (%) pattern",
                    ));
                }
                let high = char::from(bytes[index + 1]).to_digit(16);
                let low = char::from(bytes[index + 2]).to_digit(16);
                let (Some(high), Some(low)) = (high, low) else {
                    let bad = if high.is_none() {
                        bytes[index + 1]
                    } else {
                        bytes[index + 2]
                    };
                    return Err(alloc::format!(
                        "URLDecoder: Illegal hex characters in escape (%) pattern - not a hexadecimal digit: \"{}\" = {}",
                        char::from(bad),
                        u32::from(bad)
                    ));
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
    Ok(String::from_utf8_lossy(&out).into_owned())
}

const URL_ENCODER_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method(
        "encode",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = string_arg(vm, args[0])
                .map_err(|_| vm.throw_new("java/lang/NullPointerException", None))?;
            object(vm.make_string(&url_encode(&text))?)
        },
    ),
    method(
        "encode",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = string_arg(vm, args[0])
                .map_err(|_| vm.throw_new("java/lang/NullPointerException", None))?;
            let enc_ref = args[1].as_ref();
            if enc_ref.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("charsetName")));
            }
            let enc_name = vm.string_or_empty(enc_ref);
            // The JDK resolves the charset name; an illegal or unknown name is
            // reported as an UnsupportedEncodingException.
            if crate::charset::for_name(vm, &enc_name)?.is_none() {
                return Err(vm.throw_new("java/io/UnsupportedEncodingException", Some(&enc_name)));
            }
            let text = if text.is_empty() { String::new() } else { text };
            object(vm.make_string(&url_encode(&text))?)
        },
    ),
    method(
        "encode",
        "(Ljava/lang/String;Ljava/nio/charset/Charset;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = string_arg(vm, args[0])
                .map_err(|_| vm.throw_new("java/lang/NullPointerException", None))?;
            if args[1].as_ref().is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("charset")));
            }
            object(vm.make_string(&url_encode(&text))?)
        },
    ),
];

pub(crate) const URL_ENCODER: oxjvm_vm::NativeClass = class(
    "java/net/URLEncoder",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &URL_ENCODER_METHODS,
    None,
);

const URL_DECODER_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method(
        "decode",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = string_arg(vm, args[0])
                .map_err(|_| vm.throw_new("java/lang/NullPointerException", None))?;
            match url_decode(&text) {
                Ok(decoded) => object(vm.make_string(&decoded)?),
                Err(message) => {
                    Err(vm.throw_new("java/lang/IllegalArgumentException", Some(&message)))
                }
            }
        },
    ),
    method(
        "decode",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = string_arg(vm, args[0])
                .map_err(|_| vm.throw_new("java/lang/NullPointerException", None))?;
            let enc_ref = args[1].as_ref();
            if enc_ref.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("enc")));
            }
            let enc_name = vm.string_or_empty(enc_ref);
            if enc_name.is_empty() {
                return Err(vm.throw_new(
                    "java/io/UnsupportedEncodingException",
                    Some("URLDecoder: empty string enc parameter"),
                ));
            }
            // The JDK resolves the charset name; an illegal or unknown name is
            // reported as an UnsupportedEncodingException.
            if crate::charset::for_name(vm, &enc_name)?.is_none() {
                return Err(vm.throw_new("java/io/UnsupportedEncodingException", Some(&enc_name)));
            }
            match url_decode(&text) {
                Ok(decoded) => object(vm.make_string(&decoded)?),
                Err(message) => {
                    Err(vm.throw_new("java/lang/IllegalArgumentException", Some(&message)))
                }
            }
        },
    ),
    method(
        "decode",
        "(Ljava/lang/String;Ljava/nio/charset/Charset;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = string_arg(vm, args[0])
                .map_err(|_| vm.throw_new("java/lang/NullPointerException", None))?;
            if args[1].as_ref().is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("Charset")));
            }
            match url_decode(&text) {
                Ok(decoded) => object(vm.make_string(&decoded)?),
                Err(message) => {
                    Err(vm.throw_new("java/lang/IllegalArgumentException", Some(&message)))
                }
            }
        },
    ),
];

pub(crate) const URL_DECODER: oxjvm_vm::NativeClass = class(
    "java/net/URLDecoder",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &URL_DECODER_METHODS,
    None,
);

const URL_CONNECTION_METHODS: [oxjvm_vm::NativeMethodDef; 19] = [
    method(
        "getInputStream",
        "()Ljava/io/InputStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let url = vm
                .read_ref_field(this, vm.class_of(this), "url", "Ljava/net/URL;")
                .unwrap_or(ObjectRef::NULL);
            let (class, method) =
                vm.resolve_virtual_method(url, "openStream", "()Ljava/io/InputStream;")?;
            vm.invoke_method(class, method, vec![Value::Ref(url)])
        },
    ),
    method("getContentLength", "()I", ACC_PUBLIC, |vm, _, args| {
        int(url_connection_length(vm, receiver(args)) as i32)
    }),
    method("getContentLengthLong", "()J", ACC_PUBLIC, |vm, _, args| {
        long(url_connection_length(vm, receiver(args)))
    }),
    method("getLastModified", "()J", ACC_PUBLIC, |vm, _, args| {
        let path = url_connection_path(vm, receiver(args));
        long(
            vm.host
                .filesystem()
                .and_then(|fs| fs.metadata(&path))
                .map_or(0, |metadata| metadata.modified_millis),
        )
    }),
    method(
        "getContentType",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getHeaderFields",
        "()Ljava/util/Map;",
        ACC_PUBLIC,
        |vm, _, _| object(crate::util::hash_map_new(vm)?),
    ),
    method("getURL", "()Ljava/net/URL;", ACC_PUBLIC, |vm, _, args| {
        object(
            vm.read_ref_field(
                receiver(args),
                vm.class_of(receiver(args)),
                "url",
                "Ljava/net/URL;",
            )
            .unwrap_or(ObjectRef::NULL),
        )
    }),
    method("connect", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "guessContentTypeFromStream",
        "(Ljava/io/InputStream;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let stream = ref_arg(args, 0);
            if stream.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let (class, method) = vm.resolve_virtual_method(stream, "markSupported", "()Z")?;
            if vm
                .invoke_method(class, method, vec![Value::Ref(stream)])?
                .as_int()
                == 0
            {
                return object(ObjectRef::NULL);
            }
            let (class, method) = vm.resolve_virtual_method(stream, "mark", "(I)V")?;
            vm.invoke_method(class, method, vec![Value::Ref(stream), Value::Int(16)])?;
            let (class, method) = vm.resolve_virtual_method(stream, "read", "()I")?;
            let mut bytes = [-1i32; 16];
            for slot in bytes.iter_mut() {
                *slot = vm
                    .invoke_method(class, method, vec![Value::Ref(stream)])?
                    .as_int();
            }
            let (class, method) = vm.resolve_virtual_method(stream, "reset", "()V")?;
            vm.invoke_method(class, method, vec![Value::Ref(stream)])?;
            match guess_content_type_magic(&bytes) {
                Some(name) => object(vm.make_string(name)?),
                None => object(ObjectRef::NULL),
            }
        },
    ),
    method(
        "guessContentTypeFromName",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return object(ObjectRef::NULL);
            }
            let map = url_connection_file_name_map(vm)?;
            let (class, method) = vm.resolve_virtual_method(
                map,
                "getContentTypeFor",
                "(Ljava/lang/String;)Ljava/lang/String;",
            )?;
            vm.invoke_method(class, method, vec![Value::Ref(map), Value::Ref(name)])
        },
    ),
    method(
        "getFileNameMap",
        "()Ljava/net/FileNameMap;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(url_connection_file_name_map(vm)?),
    ),
    method(
        "setDefaultUseCaches",
        "(Ljava/lang/String;Z)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let protocol = ref_arg(args, 0);
            if protocol.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let map = url_connection_default_use_caches(vm)?;
            let value = url_connection_box_boolean(vm, crate::bool_arg(args, 1))?;
            crate::util::hash_map_insert(vm, map, protocol, value)?;
            void()
        },
    ),
    method(
        "getDefaultUseCaches",
        "(Ljava/lang/String;)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let protocol = ref_arg(args, 0);
            if protocol.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let map = url_connection_default_use_caches(vm)?;
            let value = crate::util::hash_map_lookup(vm, map, protocol)?;
            if value.is_null() {
                return boolean(true);
            }
            let (class, method) = vm.resolve_virtual_method(value, "booleanValue", "()Z")?;
            let result = vm.invoke_method(class, method, vec![Value::Ref(value)])?;
            boolean(result.as_int() != 0)
        },
    ),
    method(
        "setDefaultRequestProperty",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| void(),
    ),
    method(
        "getDefaultRequestProperty",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "setRequestProperty",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let key = ref_arg(args, 1);
            if key.is_null() {
                return Err(
                    vm.throw_new("java/lang/NullPointerException", Some("key cannot be null"))
                );
            }
            let this = receiver(args);
            let (keys, values) = url_connection_request_lists(vm, this)?;
            let key_text = vm.string_or_empty(key);
            let key_elements = crate::util::array_list_elements(vm, keys);
            let value_elements = crate::util::array_list_elements(vm, values);
            let mut kept_keys = Vec::new();
            let mut kept_values = Vec::new();
            for (index, element) in key_elements.iter().enumerate() {
                if vm.string_or_empty(*element) != key_text {
                    kept_keys.push(*element);
                    kept_values.push(value_elements[index]);
                }
            }
            kept_keys.push(key);
            kept_values.push(ref_arg(args, 2));
            crate::util::array_list_store(vm, keys, &kept_keys)?;
            crate::util::array_list_store(vm, values, &kept_values)?;
            void()
        },
    ),
    method(
        "addRequestProperty",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let key = ref_arg(args, 1);
            if key.is_null() {
                return Err(
                    vm.throw_new("java/lang/NullPointerException", Some("key cannot be null"))
                );
            }
            let this = receiver(args);
            let (keys, values) = url_connection_request_lists(vm, this)?;
            let mut key_elements = crate::util::array_list_elements(vm, keys);
            let mut value_elements = crate::util::array_list_elements(vm, values);
            key_elements.push(key);
            value_elements.push(ref_arg(args, 2));
            crate::util::array_list_store(vm, keys, &key_elements)?;
            crate::util::array_list_store(vm, values, &value_elements)?;
            void()
        },
    ),
    method(
        "getRequestProperty",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            // FileURLConnection ignores request properties.
            if url_connection_protocol(vm, this)?.eq_ignore_ascii_case("file") {
                return object(ObjectRef::NULL);
            }
            let class = vm.class_of(this);
            let keys = vm
                .read_ref_field(this, class, "requestKeys", "Ljava/util/ArrayList;")
                .unwrap_or(ObjectRef::NULL);
            let values = vm
                .read_ref_field(this, class, "requestValues", "Ljava/util/ArrayList;")
                .unwrap_or(ObjectRef::NULL);
            if keys.is_null() || values.is_null() {
                return object(ObjectRef::NULL);
            }
            let key_text = vm.string_or_empty(ref_arg(args, 1));
            let key_elements = crate::util::array_list_elements(vm, keys);
            let value_elements = crate::util::array_list_elements(vm, values);
            for index in (0..key_elements.len()).rev() {
                if vm.string_or_empty(key_elements[index]) == key_text {
                    return object(value_elements[index]);
                }
            }
            object(ObjectRef::NULL)
        },
    ),
    method(
        "getRequestProperties",
        "()Ljava/util/Map;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            // FileURLConnection ignores request properties.
            if url_connection_protocol(vm, this)?.eq_ignore_ascii_case("file") {
                return object(crate::util::new_hash_map(vm)?);
            }
            let class = vm.class_of(this);
            let keys = vm
                .read_ref_field(this, class, "requestKeys", "Ljava/util/ArrayList;")
                .unwrap_or(ObjectRef::NULL);
            let values = vm
                .read_ref_field(this, class, "requestValues", "Ljava/util/ArrayList;")
                .unwrap_or(ObjectRef::NULL);
            let map_class = vm.resolve_class("java/util/LinkedHashMap")?;
            let map = vm.new_instance(map_class)?;
            if keys.is_null() || values.is_null() {
                return object(map);
            }
            let key_elements = crate::util::array_list_elements(vm, keys);
            let value_elements = crate::util::array_list_elements(vm, values);
            let mut seen: Vec<String> = Vec::new();
            for element in &key_elements {
                let key_text = vm.string_or_empty(*element);
                if seen.contains(&key_text) {
                    continue;
                }
                seen.push(key_text.clone());
                let list = crate::util::new_array_list(vm)?;
                let mut group = Vec::new();
                for (other, other_element) in key_elements.iter().enumerate() {
                    if vm.string_or_empty(*other_element) == key_text {
                        group.push(value_elements[other]);
                    }
                }
                crate::util::array_list_store(vm, list, &group)?;
                let (declaring, method) = vm.resolve_virtual_method(
                    map,
                    "put",
                    "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
                )?;
                vm.invoke_method(
                    declaring,
                    method,
                    vec![Value::Ref(map), Value::Ref(*element), Value::Ref(list)],
                )?;
            }
            object(map)
        },
    ),
];

/// The lazily-created per-protocol `defaultUseCaches` map on `URLConnection`.
fn url_connection_default_use_caches(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/net/URLConnection")?;
    if let Some(map) = vm.read_static_ref_if_set(class, "defaultUseCaches", "Ljava/util/HashMap;") {
        return Ok(map);
    }
    let map = crate::util::hash_map_new(vm)?;
    vm.set_static_value(
        class,
        "defaultUseCaches",
        "Ljava/util/HashMap;",
        Value::Ref(map),
    );
    Ok(map)
}

/// The lazily-created `FileNameMap` singleton handed out by `getFileNameMap`.
fn url_connection_file_name_map(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/net/URLConnection")?;
    if let Some(map) = vm.read_static_ref_if_set(class, "fileNameMap", "Ljava/net/FileNameMap;") {
        return Ok(map);
    }
    let impl_class = vm.resolve_class("java/net/OjvmFileNameMap")?;
    let instance = vm.new_instance(impl_class)?;
    vm.set_static_value(
        class,
        "fileNameMap",
        "Ljava/net/FileNameMap;",
        Value::Ref(instance),
    );
    Ok(instance)
}

/// Box a boolean through `Boolean.valueOf` for the `defaultUseCaches` map.
fn url_connection_box_boolean(vm: &mut Vm<'_>, value: bool) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/lang/Boolean")?;
    let (declaring, method) = vm
        .find_method(class, "valueOf", "(Z)Ljava/lang/Boolean;")
        .ok_or_else(|| vm.throw_new("java/lang/NoSuchMethodError", Some("Boolean.valueOf")))?;
    Ok(vm
        .invoke_method(declaring, method, vec![Value::Int(i32::from(value))])?
        .as_ref())
}

/// Magic-byte sniffing behind `URLConnection.guessContentTypeFromStream`.
fn guess_content_type_magic(c: &[i32]) -> Option<&'static str> {
    let starts = |prefix: &[i32]| c.len() >= prefix.len() && c[..prefix.len()] == *prefix;
    let starts_ci = |prefix: &[i32]| {
        c.len() >= prefix.len()
            && c[..prefix.len()]
                .iter()
                .zip(prefix.iter())
                .all(|(&byte, &want)| {
                    let lower = if (0x41..=0x5a).contains(&byte) {
                        byte + 0x20
                    } else {
                        byte
                    };
                    lower == want
                })
    };
    if starts(&[0x47, 0x49, 0x46, 0x38]) {
        return Some("image/gif");
    }
    if starts(&[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]) {
        return Some("image/png");
    }
    if c.len() >= 4
        && c[0] == 0xff
        && c[1] == 0xd8
        && c[2] == 0xff
        && (c[3] == 0xe0 || c[3] == 0xee)
    {
        return Some("image/jpeg");
    }
    if starts(&[0x49, 0x49, 0x2a, 0x00]) || starts(&[0x4d, 0x4d, 0x00, 0x2a]) {
        return Some("image/tiff");
    }
    if starts(&[0x3c, 0x3f, 0x78, 0x6d, 0x6c]) {
        return Some("application/xml");
    }
    if starts(&[0xca, 0xfe, 0xba, 0xbe]) {
        return Some("application/java-vm");
    }
    if starts(&[0x2e, 0x73, 0x6e, 0x64]) {
        return Some("audio/basic");
    }
    if starts(&[0x52, 0x49, 0x46, 0x46]) {
        return Some("audio/x-wav");
    }
    if starts_ci(&[0x3c, 0x68, 0x74, 0x6d, 0x6c])
        || starts_ci(&[0x3c, 0x21, 0x64, 0x6f, 0x63, 0x74, 0x79, 0x70, 0x65])
    {
        return Some("text/html");
    }
    None
}

fn url_connection_path(vm: &Vm<'_>, connection: ObjectRef) -> String {
    let url = vm
        .read_ref_field(connection, vm.class_of(connection), "url", "Ljava/net/URL;")
        .unwrap_or(ObjectRef::NULL);
    url_text(vm, url)
        .split_once(':')
        .map_or_else(String::new, |(_, path)| String::from(path))
}

fn url_connection_length(vm: &mut Vm<'_>, connection: ObjectRef) -> i64 {
    let path = url_connection_path(vm, connection);
    vm.host
        .filesystem()
        .and_then(|fs| fs.metadata(&path))
        .map_or(-1, |metadata| metadata.len as i64)
}

/// `java.net.URLConnection`: file-backed metadata and streams for `file:` URLs.
/// The protocol of a URLConnection's URL, or an empty string when unset.
fn url_connection_protocol(vm: &mut Vm<'_>, this: ObjectRef) -> Result<String, VmError> {
    let url = vm
        .read_ref_field(this, vm.class_of(this), "url", "Ljava/net/URL;")
        .unwrap_or(ObjectRef::NULL);
    if url.is_null() {
        return Ok(String::new());
    }
    let (class, method) = vm.resolve_virtual_method(url, "getProtocol", "()Ljava/lang/String;")?;
    let value = vm.invoke_method(class, method, vec![Value::Ref(url)])?;
    Ok(vm.string_or_empty(value.as_ref()))
}

/// The parallel key/value lists holding a URLConnection's request properties.
fn url_connection_request_lists(
    vm: &mut Vm<'_>,
    this: ObjectRef,
) -> Result<(ObjectRef, ObjectRef), VmError> {
    let class = vm.class_of(this);
    let keys = vm
        .read_ref_field(this, class, "requestKeys", "Ljava/util/ArrayList;")
        .unwrap_or(ObjectRef::NULL);
    let values = vm
        .read_ref_field(this, class, "requestValues", "Ljava/util/ArrayList;")
        .unwrap_or(ObjectRef::NULL);
    if !keys.is_null() && !values.is_null() {
        return Ok((keys, values));
    }
    let keys = crate::util::new_array_list(vm)?;
    let values = crate::util::new_array_list(vm)?;
    let (declaring, field) = vm.find_field(class, "requestKeys", "Ljava/util/ArrayList;")?;
    vm.set_instance_ref(this, declaring, field, keys);
    let (declaring, field) = vm.find_field(class, "requestValues", "Ljava/util/ArrayList;")?;
    vm.set_instance_ref(this, declaring, field, values);
    Ok((keys, values))
}

pub(crate) const URL_CONNECTION: oxjvm_vm::NativeClass = class(
    "java/net/URLConnection",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[
        field("url", "Ljava/net/URL;", ACC_PRIVATE, None),
        field("requestKeys", "Ljava/util/ArrayList;", ACC_PRIVATE, None),
        field("requestValues", "Ljava/util/ArrayList;", ACC_PRIVATE, None),
        field(
            "defaultUseCaches",
            "Ljava/util/HashMap;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
        field(
            "fileNameMap",
            "Ljava/net/FileNameMap;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
    ],
    &URL_CONNECTION_METHODS,
    None,
);

const FILE_NAME_MAP_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "getContentTypeFor",
    "(Ljava/lang/String;)Ljava/lang/String;",
    ACC_PUBLIC | ACC_ABSTRACT,
    crate::lang::abstract_method_pub,
)];

/// `java.net.FileNameMap`: maps file names to MIME content types.
pub(crate) const FILE_NAME_MAP: oxjvm_vm::NativeClass = class(
    "java/net/FileNameMap",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &FILE_NAME_MAP_METHODS,
    None,
);

const OJVM_FILE_NAME_MAP_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "getContentTypeFor",
    "(Ljava/lang/String;)Ljava/lang/String;",
    ACC_PUBLIC,
    |vm, _, args| {
        let name = vm.string_or_empty(ref_arg(args, 1));
        match content_type_for(&name) {
            Some(kind) => object(vm.make_string(kind)?),
            None => object(ObjectRef::NULL),
        }
    },
)];

/// The default `FileNameMap` implementation backed by the JDK's
/// `content-types.properties` extension table.
pub(crate) const OJVM_FILE_NAME_MAP: oxjvm_vm::NativeClass = class(
    "java/net/OjvmFileNameMap",
    Some("java/lang/Object"),
    &["java/net/FileNameMap"],
    ACC_PUBLIC,
    &[],
    &OJVM_FILE_NAME_MAP_METHODS,
    None,
);

/// Extension-to-content-type lookup matching the JDK's default table
/// (case-insensitive, last `.` suffix; unknown extensions map to null).
fn content_type_for(name: &str) -> Option<&'static str> {
    let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "html" | "htm" => "text/html",
        "txt" | "text" | "java" | "c" | "h" | "pl" | "py" => "text/plain",
        "css" => "text/css",
        "js" => "text/javascript",
        "xml" => "application/xml",
        "json" => "application/json",
        "pdf" => "application/pdf",
        "gif" => "image/gif",
        "jpg" | "jpeg" | "jpe" => "image/jpeg",
        "png" => "image/png",
        "tif" | "tiff" => "image/tiff",
        "bmp" => "image/bmp",
        "zip" => "application/zip",
        "jar" => "application/java-archive",
        "gz" => "application/gzip",
        "tar" => "application/x-tar",
        "wav" => "audio/x-wav",
        "au" | "snd" => "audio/basic",
        "mp3" => "audio/mpeg",
        "mpg" => "video/mpeg",
        "avi" => "application/x-troff-msvideo",
        "ps" | "eps" => "application/postscript",
        "dvi" => "application/x-dvi",
        "rtf" => "application/rtf",
        "doc" => "application/msword",
        "xls" => "application/vnd.ms-excel",
        "ppt" => "application/vnd.ms-powerpoint",
        "bin" | "exe" | "o" => "application/octet-stream",
        "sh" => "application/x-shar",
        _ => return None,
    })
}

// -------------------------------------------------------------------------------------------
// Tier 2.1 shells: networking types not yet fully implemented
// -------------------------------------------------------------------------------------------

fn server_socket_bool(vm: &Vm<'_>, this: ObjectRef, name: &str) -> bool {
    vm.read_instance_int_named(this, vm.class_of(this), name, "Z")
        .unwrap_or(0)
        != 0
}

fn server_socket_set_bool(vm: &mut Vm<'_>, this: ObjectRef, name: &str, value: bool) {
    if let Some((declaring, field)) = vm.find_field_opt(vm.class_of(this), name, "Z") {
        vm.set_instance_int(this, declaring, field, i32::from(value));
    }
}

fn server_socket_int(vm: &Vm<'_>, this: ObjectRef, name: &str) -> i32 {
    vm.read_instance_int_named(this, vm.class_of(this), name, "I")
        .unwrap_or(0)
}

fn server_socket_set_int(vm: &mut Vm<'_>, this: ObjectRef, name: &str, value: i32) {
    if let Some((declaring, field)) = vm.find_field_opt(vm.class_of(this), name, "I") {
        vm.set_instance_int(this, declaring, field, value);
    }
}

fn server_socket_address(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field_any(this, "localAddr")
        .unwrap_or(ObjectRef::NULL)
}

fn new_inet_socket_address(
    vm: &mut Vm<'_>,
    address: ObjectRef,
    port: i32,
) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/net/InetSocketAddress")?;
    let instance = vm.new_instance(class)?;
    init_inet_socket_address(vm, instance, address, port)?;
    object(instance)
}

fn server_socket_bind(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    endpoint: ObjectRef,
    backlog: i32,
) -> Result<(), VmError> {
    if server_socket_bool(vm, this, "bound") {
        return Err(vm.throw_new("java/net/SocketException", Some("Already bound")));
    }
    if server_socket_bool(vm, this, "closed") {
        return Err(vm.throw_new("java/net/SocketException", Some("Socket is closed!")));
    }
    if !endpoint.is_null() {
        let class = vm.resolve_class("java/net/InetSocketAddress")?;
        if !vm.is_instance(endpoint, class) {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("Unsupported address type"),
            ));
        }
        let port = inet_socket_address_port(vm, endpoint);
        let address = inet_socket_address_address(vm, endpoint);
        server_socket_set_int(vm, this, "localPort", port);
        let (declaring, field) =
            vm.find_field(vm.class_of(this), "localAddr", "Ljava/net/InetAddress;")?;
        vm.set_instance_ref(this, declaring, field, address);
    } else {
        server_socket_set_int(vm, this, "localPort", 0);
    }
    if server_socket_int(vm, this, "localPort") == 0 {
        let ephemeral = 20000 + (this.raw() as i32 % 20000);
        server_socket_set_int(vm, this, "localPort", ephemeral);
    }
    server_socket_set_int(vm, this, "backlog", backlog);
    server_socket_set_bool(vm, this, "bound", true);
    Ok(())
}

fn server_socket_init(vm: &mut Vm<'_>, this: ObjectRef, port: i32) -> Result<(), VmError> {
    server_socket_set_int(vm, this, "localPort", -1);
    server_socket_set_int(vm, this, "soTimeout", 0);
    server_socket_set_int(vm, this, "receiveBufferSize", 65536);
    let wildcard = wildcard_address(vm)?;
    let (declaring, field) =
        vm.find_field(vm.class_of(this), "localAddr", "Ljava/net/InetAddress;")?;
    vm.set_instance_ref(this, declaring, field, wildcard);
    if port >= 0 {
        validate_port(vm, port)?;
        server_socket_set_int(vm, this, "localPort", port);
        if port == 0 {
            let ephemeral = 20000 + (this.raw() as i32 % 20000);
            server_socket_set_int(vm, this, "localPort", ephemeral);
        }
        server_socket_set_bool(vm, this, "bound", true);
    }
    Ok(())
}

const SERVER_SOCKET_METHODS: [oxjvm_vm::NativeMethodDef; 27] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        server_socket_init(vm, receiver(args), -1)?;
        void()
    }),
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        server_socket_init(vm, receiver(args), int_arg(args, 1))?;
        void()
    }),
    method("<init>", "(II)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        server_socket_init(vm, this, int_arg(args, 1))?;
        server_socket_set_int(vm, this, "backlog", int_arg(args, 2));
        void()
    }),
    method(
        "<init>",
        "(IILjava/net/InetAddress;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            server_socket_init(vm, this, int_arg(args, 1))?;
            server_socket_set_int(vm, this, "backlog", int_arg(args, 2));
            let address = ref_arg(args, 3);
            if !address.is_null() {
                let (declaring, field) =
                    vm.find_field(vm.class_of(this), "localAddr", "Ljava/net/InetAddress;")?;
                vm.set_instance_ref(this, declaring, field, address);
            }
            void()
        },
    ),
    method(
        "bind",
        "(Ljava/net/SocketAddress;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            server_socket_bind(vm, receiver(args), ref_arg(args, 1), 50)?;
            void()
        },
    ),
    method(
        "bind",
        "(Ljava/net/SocketAddress;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            server_socket_bind(vm, receiver(args), ref_arg(args, 1), int_arg(args, 2))?;
            void()
        },
    ),
    method("getLocalPort", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if server_socket_bool(vm, this, "bound") {
            int(server_socket_int(vm, this, "localPort"))
        } else {
            int(-1)
        }
    }),
    method(
        "getInetAddress",
        "()Ljava/net/InetAddress;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if server_socket_bool(vm, this, "bound") {
                object(server_socket_address(vm, this))
            } else {
                object(ObjectRef::NULL)
            }
        },
    ),
    method(
        "getLocalSocketAddress",
        "()Ljava/net/SocketAddress;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if !server_socket_bool(vm, this, "bound") || server_socket_bool(vm, this, "closed") {
                return object(ObjectRef::NULL);
            }
            let address = server_socket_address(vm, this);
            new_inet_socket_address(vm, address, server_socket_int(vm, this, "localPort"))
        },
    ),
    method("isBound", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(server_socket_bool(vm, receiver(args), "bound"))
    }),
    method("isClosed", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(server_socket_bool(vm, receiver(args), "closed"))
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        server_socket_set_bool(vm, this, "closed", true);
        void()
    }),
    method("setReuseAddress", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        server_socket_set_bool(vm, receiver(args), "reuseAddress", crate::bool_arg(args, 1));
        void()
    }),
    method("getReuseAddress", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(server_socket_bool(vm, receiver(args), "reuseAddress"))
    }),
    method("setSoTimeout", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let timeout = int_arg(args, 1);
        if timeout < 0 {
            return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("timeout < 0")));
        }
        server_socket_set_int(vm, receiver(args), "soTimeout", timeout);
        void()
    }),
    method("getSoTimeout", "()I", ACC_PUBLIC, |vm, _, args| {
        int(server_socket_int(vm, receiver(args), "soTimeout"))
    }),
    method("setReceiveBufferSize", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let size = int_arg(args, 1);
        if size <= 0 {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("negative receive size"),
            ));
        }
        server_socket_set_int(vm, receiver(args), "receiveBufferSize", size);
        void()
    }),
    method("getReceiveBufferSize", "()I", ACC_PUBLIC, |vm, _, args| {
        int(server_socket_int(vm, receiver(args), "receiveBufferSize"))
    }),
    method("accept", "()Ljava/net/Socket;", ACC_PUBLIC, |vm, _, _| {
        Err(vm.throw_new(
            "java/net/SocketException",
            Some("accept is not supported by oxjvm"),
        ))
    }),
    method(
        "getChannel",
        "()Ljava/nio/channels/ServerSocketChannel;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "setPerformancePreferences",
        "(III)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let address = server_socket_address(vm, this);
            let address_text = if address.is_null() {
                String::from("0.0.0.0")
            } else {
                vm.read_string_field_any(address, "hostAddress")
                    .unwrap_or_else(|| dotted(&address_bytes(vm, address)))
            };
            let text = alloc::format!(
                "ServerSocket[addr={address_text},localport={}]",
                server_socket_int(vm, this, "localPort")
            );
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "setOption",
        "(Ljava/net/SocketOption;Ljava/lang/Object;)Ljava/net/ServerSocket;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method(
        "getOption",
        "(Ljava/net/SocketOption;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "supportedOptions",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/HashSet")?;
            let instance = vm.new_instance(class)?;
            if let Some((declaring, ctor)) = vm.find_method(class, "<init>", "()V") {
                vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
            }
            object(instance)
        },
    ),
    method("setSoReuseAddress", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        server_socket_set_bool(vm, receiver(args), "reuseAddress", crate::bool_arg(args, 1));
        void()
    }),
    method("getSoReuseAddress", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(server_socket_bool(vm, receiver(args), "reuseAddress"))
    }),
];

pub(crate) const SERVER_SOCKET: oxjvm_vm::NativeClass = class(
    "java/net/ServerSocket",
    Some("java/lang/Object"),
    &["java/io/Closeable"],
    ACC_PUBLIC,
    &[
        field("bound", "Z", ACC_PRIVATE, None),
        field("closed", "Z", ACC_PRIVATE, None),
        field("reuseAddress", "Z", ACC_PRIVATE, None),
        field("localPort", "I", ACC_PRIVATE, None),
        field("backlog", "I", ACC_PRIVATE, None),
        field("soTimeout", "I", ACC_PRIVATE, None),
        field("receiveBufferSize", "I", ACC_PRIVATE, None),
        field("localAddr", "Ljava/net/InetAddress;", ACC_PRIVATE, None),
    ],
    &SERVER_SOCKET_METHODS,
    None,
);

const SOCKET_METHODS: [oxjvm_vm::NativeMethodDef; 15] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "<init>",
        "(Ljava/net/InetAddress;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            crate::lang::write_int_field(vm, this, "port", int_arg(args, 2));
            crate::lang::write_int_field(vm, this, "connected", 1);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "address", "Ljava/net/InetAddress;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            crate::lang::write_int_field(vm, this, "port", int_arg(args, 2));
            crate::lang::write_int_field(vm, this, "connected", 1);
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/net/SocketAddress;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            crate::lang::write_int_field(vm, receiver(args), "connected", 1);
            void()
        },
    ),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        crate::lang::write_int_field(vm, receiver(args), "closed", 1);
        void()
    }),
    method(
        "connect",
        "(Ljava/net/SocketAddress;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            crate::lang::write_int_field(vm, receiver(args), "connected", 1);
            void()
        },
    ),
    method(
        "connect",
        "(Ljava/net/SocketAddress;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            crate::lang::write_int_field(vm, receiver(args), "connected", 1);
            void()
        },
    ),
    method("isClosed", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "closed") != 0)
    }),
    method("isConnected", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "connected") != 0)
    }),
    method("isBound", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "connected") != 0)
    }),
    method("getPort", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "port"))
    }),
    method("getLocalPort", "()I", ACC_PUBLIC, |_, _, _| int(-1)),
    method(
        "getInetAddress",
        "()Ljava/net/InetAddress;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field_any(vm, receiver(args), "address")),
    ),
    method("getSoTimeout", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method("setSoTimeout", "(I)V", ACC_PUBLIC, |_, _, _| void()),
];

fn ref_field_any(vm: &Vm<'_>, this: ObjectRef, name: &str) -> ObjectRef {
    vm.read_ref_field_any(this, name).unwrap_or(ObjectRef::NULL)
}

pub(crate) const SOCKET: oxjvm_vm::NativeClass = class(
    "java/net/Socket",
    Some("java/lang/Object"),
    &["java/io/Closeable"],
    ACC_PUBLIC,
    &[
        field("port", "I", ACC_PRIVATE, None),
        field("closed", "I", ACC_PRIVATE, None),
        field("connected", "I", ACC_PRIVATE, None),
        field("address", "Ljava/net/InetAddress;", ACC_PRIVATE, None),
    ],
    &SOCKET_METHODS,
    None,
);

pub(crate) const SOCKET_IMPL: oxjvm_vm::NativeClass = class(
    "java/net/SocketImpl",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const SOCKET_ADDRESS: oxjvm_vm::NativeClass = class(
    "java/net/SocketAddress",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

fn inet_socket_address_port(vm: &Vm<'_>, this: ObjectRef) -> i32 {
    crate::lang::read_int_field(vm, this, "port")
}

fn inet_socket_address_address(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field_any(this, "addr")
        .unwrap_or(ObjectRef::NULL)
}

fn validate_port(vm: &mut Vm<'_>, port: i32) -> Result<(), VmError> {
    if !(0..=65535).contains(&port) {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("port out of range:"),
        ));
    }
    Ok(())
}

fn init_inet_socket_address(
    vm: &mut Vm<'_>,
    instance: ObjectRef,
    address: ObjectRef,
    port: i32,
) -> Result<(), VmError> {
    validate_port(vm, port)?;
    let class = vm.class_of(instance);
    let (declaring, field) = vm.find_field(class, "addr", "Ljava/net/InetAddress;")?;
    vm.set_instance_ref(instance, declaring, field, address);
    let (declaring, field) = vm.find_field(class, "port", "I")?;
    vm.set_instance_int(instance, declaring, field, port);
    Ok(())
}

fn wildcard_address(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    Ok(new_address(vm, "java/net/InetAddress", &[0, 0, 0, 0], None)?.as_ref())
}

fn inet_socket_address_host(vm: &Vm<'_>, this: ObjectRef) -> String {
    if let Some(host) = vm.read_string_field_any(this, "hostname") {
        return host;
    }
    let address = inet_socket_address_address(vm, this);
    if address.is_null() {
        return String::from("0.0.0.0");
    }
    vm.read_string_field_any(address, "hostName")
        .or_else(|| vm.read_string_field_any(address, "hostAddress"))
        .unwrap_or_else(|| String::from("0.0.0.0"))
}

const INET_SOCKET_ADDRESS_METHODS: [oxjvm_vm::NativeMethodDef; 13] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let wildcard = wildcard_address(vm)?;
        init_inet_socket_address(vm, receiver(args), wildcard, 0)?;
        void()
    }),
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let port = int_arg(args, 1);
        let wildcard = wildcard_address(vm)?;
        init_inet_socket_address(vm, receiver(args), wildcard, port)?;
        void()
    }),
    method(
        "<init>",
        "(Ljava/net/InetAddress;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let port = int_arg(args, 2);
            let mut address = ref_arg(args, 1);
            if address.is_null() {
                address = wildcard_address(vm)?;
            }
            init_inet_socket_address(vm, receiver(args), address, port)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = string_arg(vm, args[1])?;
            let port = int_arg(args, 2);
            validate_port(vm, port)?;
            let address = resolve_address(vm, "java/net/InetAddress", &text)?.as_ref();
            init_inet_socket_address(vm, receiver(args), address, port)?;
            void()
        },
    ),
    method("getPort", "()I", ACC_PUBLIC, |vm, _, args| {
        int(inet_socket_address_port(vm, receiver(args)))
    }),
    method(
        "getAddress",
        "()Ljava/net/InetAddress;",
        ACC_PUBLIC,
        |vm, _, args| object(inet_socket_address_address(vm, receiver(args))),
    ),
    method(
        "getHostName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let address = inet_socket_address_address(vm, this);
            let host = if address.is_null() {
                inet_socket_address_host(vm, this)
            } else {
                let (class, method) =
                    vm.resolve_virtual_method(address, "getHostName", "()Ljava/lang/String;")?;
                let value = vm.invoke_method(class, method, vec![Value::Ref(address)])?;
                vm.string_value(value.as_ref())
                    .unwrap_or_else(|| inet_socket_address_host(vm, this))
            };
            object(vm.make_string(&host)?)
        },
    ),
    method(
        "getHostString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let host = inet_socket_address_host(vm, receiver(args));
            object(vm.make_string(&host)?)
        },
    ),
    method("isUnresolved", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(inet_socket_address_address(vm, receiver(args)).is_null())
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let address = inet_socket_address_address(vm, this);
            let port = inet_socket_address_port(vm, this);
            let text = if address.is_null() {
                alloc::format!("{}:{port}", inet_socket_address_host(vm, this))
            } else {
                let (class, method) =
                    vm.resolve_virtual_method(address, "toString", "()Ljava/lang/String;")?;
                let value = vm.invoke_method(class, method, vec![Value::Ref(address)])?;
                let address_text = vm
                    .string_value(value.as_ref())
                    .unwrap_or_else(|| inet_socket_address_host(vm, this));
                alloc::format!("{address_text}:{port}")
            };
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            if other.is_null() || vm.class_name(vm.class_of(other)) != "java/net/InetSocketAddress"
            {
                return boolean(false);
            }
            if inet_socket_address_port(vm, this) != inet_socket_address_port(vm, other) {
                return boolean(false);
            }
            let left = inet_socket_address_address(vm, this);
            let right = inet_socket_address_address(vm, other);
            if left.is_null() || right.is_null() {
                return boolean(left.is_null() && right.is_null());
            }
            let (class, method) =
                vm.resolve_virtual_method(left, "equals", "(Ljava/lang/Object;)Z")?;
            let result =
                vm.invoke_method(class, method, vec![Value::Ref(left), Value::Ref(right)])?;
            boolean(result.as_int() != 0)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let address = inet_socket_address_address(vm, this);
        let address_hash = if address.is_null() {
            0
        } else {
            let (class, method) = vm.resolve_virtual_method(address, "hashCode", "()I")?;
            vm.invoke_method(class, method, vec![Value::Ref(address)])?
                .as_int()
        };
        int(address_hash.wrapping_add(inet_socket_address_port(vm, this)))
    }),
    method(
        "createUnresolved",
        "(Ljava/lang/String;I)Ljava/net/InetSocketAddress;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let host = string_arg(vm, args[0])?;
            let port = int_arg(args, 1);
            validate_port(vm, port)?;
            let class = vm.resolve_class("java/net/InetSocketAddress")?;
            let instance = vm.new_instance(class)?;
            let (declaring, field) = vm.find_field(class, "port", "I")?;
            vm.set_instance_int(instance, declaring, field, port);
            let (declaring, field) = vm.find_field(class, "hostname", "Ljava/lang/String;")?;
            let value = vm.make_string(&host)?;
            vm.set_instance_ref(instance, declaring, field, value);
            object(instance)
        },
    ),
];

pub(crate) const INET_SOCKET_ADDRESS: oxjvm_vm::NativeClass = class(
    "java/net/InetSocketAddress",
    Some("java/net/SocketAddress"),
    &[],
    ACC_PUBLIC,
    &[
        field("addr", "Ljava/net/InetAddress;", ACC_PRIVATE, None),
        field("port", "I", ACC_PRIVATE, None),
        field("hostname", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &INET_SOCKET_ADDRESS_METHODS,
    None,
);

pub(crate) const SOCKET_OPTION: oxjvm_vm::NativeClass = class(
    "java/net/SocketOption",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE,
    &[],
    &[],
    None,
);

const DATAGRAM_SOCKET_METHODS: [oxjvm_vm::NativeMethodDef; 18] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        crate::lang::write_int_field(vm, this, "port", int_arg(args, 1));
        crate::lang::write_int_field(vm, this, "bound", 1);
        void()
    }),
    method(
        "<init>",
        "(ILjava/net/InetAddress;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            crate::lang::write_int_field(vm, this, "port", int_arg(args, 1));
            crate::lang::write_int_field(vm, this, "bound", 1);
            let class = vm.class_of(this);
            let (declaring, field) =
                vm.find_field(class, "localAddress", "Ljava/net/InetAddress;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 2));
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/net/SocketAddress;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            crate::lang::write_int_field(vm, this, "bound", 1);
            let address = ref_arg(args, 1);
            if !address.is_null() {
                let (class, method) = vm.resolve_virtual_method(address, "getPort", "()I")?;
                let port = vm
                    .invoke_method(class, method, vec![Value::Ref(address)])?
                    .as_int();
                crate::lang::write_int_field(vm, this, "port", port);
            }
            void()
        },
    ),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        crate::lang::write_int_field(vm, receiver(args), "closed", 1);
        void()
    }),
    method(
        "bind",
        "(Ljava/net/SocketAddress;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            crate::lang::write_int_field(vm, receiver(args), "bound", 1);
            void()
        },
    ),
    method(
        "connect",
        "(Ljava/net/SocketAddress;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "connect",
        "(Ljava/net/InetAddress;I)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method("disconnect", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("isClosed", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "closed") != 0)
    }),
    method("isBound", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "bound") != 0)
    }),
    method("isConnected", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("getLocalPort", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "port"))
    }),
    method("getPort", "()I", ACC_PUBLIC, |_, _, _| int(-1)),
    method("getSoTimeout", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method("setSoTimeout", "(I)V", ACC_PUBLIC, |_, _, _| void()),
    method("getReuseAddress", "()Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
    method("setReuseAddress", "(Z)V", ACC_PUBLIC, |_, _, _| void()),
];

pub(crate) const DATAGRAM_SOCKET: oxjvm_vm::NativeClass = class(
    "java/net/DatagramSocket",
    Some("java/lang/Object"),
    &["java/io/Closeable"],
    ACC_PUBLIC,
    &[
        field("port", "I", ACC_PRIVATE, None),
        field("closed", "I", ACC_PRIVATE, None),
        field("bound", "I", ACC_PRIVATE, None),
        field("localAddress", "Ljava/net/InetAddress;", ACC_PRIVATE, None),
    ],
    &DATAGRAM_SOCKET_METHODS,
    None,
);

/// `DatagramPacket.setData`: validate the buffer range (the JDK reports an
/// `IllegalArgumentException` here, not an `IndexOutOfBoundsException`) and store it.
fn datagram_packet_set_data(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    buf: ObjectRef,
    offset: i32,
    length: i32,
) -> Result<(), VmError> {
    if buf.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let total = vm.array_length(buf)? as i32;
    if offset < 0 || length < 0 || i64::from(offset) + i64::from(length) > i64::from(total) {
        return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
    }
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "buf", "[B")?;
    vm.set_instance_ref(this, declaring, field, buf);
    crate::lang::write_int_field(vm, this, "length", length);
    crate::lang::write_int_field(vm, this, "offset", offset);
    Ok(())
}

fn datagram_packet_set_endpoint(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    address: ObjectRef,
    port: i32,
) -> Result<(), VmError> {
    validate_port(vm, port)?;
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "address", "Ljava/net/InetAddress;")?;
    vm.set_instance_ref(this, declaring, field, address);
    crate::lang::write_int_field(vm, this, "port", port);
    Ok(())
}

const DATAGRAM_PACKET_METHODS: [oxjvm_vm::NativeMethodDef; 17] = [
    method("<init>", "([BI)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        datagram_packet_set_data(vm, this, ref_arg(args, 1), 0, int_arg(args, 2))?;
        void()
    }),
    method("<init>", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        datagram_packet_set_data(
            vm,
            this,
            ref_arg(args, 1),
            int_arg(args, 2),
            int_arg(args, 3),
        )?;
        void()
    }),
    method(
        "<init>",
        "([BILjava/net/InetAddress;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            datagram_packet_set_data(vm, this, ref_arg(args, 1), 0, int_arg(args, 2))?;
            datagram_packet_set_endpoint(vm, this, ref_arg(args, 3), int_arg(args, 4))?;
            void()
        },
    ),
    method(
        "<init>",
        "([BIILjava/net/InetAddress;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            datagram_packet_set_data(
                vm,
                this,
                ref_arg(args, 1),
                int_arg(args, 2),
                int_arg(args, 3),
            )?;
            datagram_packet_set_endpoint(vm, this, ref_arg(args, 4), int_arg(args, 5))?;
            void()
        },
    ),
    method(
        "<init>",
        "([BILjava/net/SocketAddress;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            datagram_packet_set_data(vm, this, ref_arg(args, 1), 0, int_arg(args, 2))?;
            datagram_packet_set_socket_address(vm, this, ref_arg(args, 3))?;
            void()
        },
    ),
    method(
        "<init>",
        "([BIILjava/net/SocketAddress;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            datagram_packet_set_data(
                vm,
                this,
                ref_arg(args, 1),
                int_arg(args, 2),
                int_arg(args, 3),
            )?;
            datagram_packet_set_socket_address(vm, this, ref_arg(args, 4))?;
            void()
        },
    ),
    method("getLength", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "length"))
    }),
    method("getOffset", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "offset"))
    }),
    method("getData", "()[B", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        object(
            vm.read_ref_field(this, vm.class_of(this), "buf", "[B")
                .unwrap_or(ObjectRef::NULL),
        )
    }),
    method(
        "getAddress",
        "()Ljava/net/InetAddress;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            object(
                vm.read_ref_field(this, vm.class_of(this), "address", "Ljava/net/InetAddress;")
                    .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method("getPort", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "port"))
    }),
    method("setLength", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let length = int_arg(args, 1);
        let total = vm.array_length(
            vm.read_ref_field(this, vm.class_of(this), "buf", "[B")
                .unwrap_or(ObjectRef::NULL),
        )? as i32;
        let offset = crate::lang::read_int_field(vm, this, "offset");
        if length < 0 || i64::from(offset) + i64::from(length) > i64::from(total) {
            return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
        }
        crate::lang::write_int_field(vm, this, "length", length);
        void()
    }),
    method("setData", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        datagram_packet_set_data(
            vm,
            this,
            ref_arg(args, 1),
            int_arg(args, 2),
            int_arg(args, 3),
        )?;
        void()
    }),
    method("setData", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let buf = ref_arg(args, 1);
        if buf.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", Some("null packet buffer")));
        }
        let length = vm.array_length(buf)? as i32;
        datagram_packet_set_data(vm, this, buf, 0, length)?;
        void()
    }),
    method(
        "setAddress",
        "(Ljava/net/InetAddress;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "address", "Ljava/net/InetAddress;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method("setPort", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let port = int_arg(args, 1);
        validate_port(vm, port)?;
        crate::lang::write_int_field(vm, receiver(args), "port", port);
        void()
    }),
    method(
        "getSocketAddress",
        "()Ljava/net/SocketAddress;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let address = vm
                .read_ref_field(this, vm.class_of(this), "address", "Ljava/net/InetAddress;")
                .unwrap_or(ObjectRef::NULL);
            // The JDK always builds `new InetSocketAddress(getAddress(), getPort())`;
            // a null address becomes the wildcard address there.
            let address = if address.is_null() {
                wildcard_address(vm)?
            } else {
                address
            };
            let port = crate::lang::read_int_field(vm, this, "port");
            let class = vm.resolve_class("java/net/InetSocketAddress")?;
            let instance = vm.new_instance(class)?;
            init_inet_socket_address(vm, instance, address, port)?;
            object(instance)
        },
    ),
];

fn datagram_packet_set_socket_address(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    address: ObjectRef,
) -> Result<(), VmError> {
    if address.is_null() || vm.class_name(vm.class_of(address)) != "java/net/InetSocketAddress" {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("unsupported address type"),
        ));
    }
    let inet = vm
        .read_ref_field(
            address,
            vm.class_of(address),
            "addr",
            "Ljava/net/InetAddress;",
        )
        .unwrap_or(ObjectRef::NULL);
    if inet.is_null() {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("unresolved address"),
        ));
    }
    let port = crate::lang::read_int_field(vm, address, "port");
    datagram_packet_set_endpoint(vm, this, inet, port)
}

pub(crate) const DATAGRAM_PACKET: oxjvm_vm::NativeClass = class(
    "java/net/DatagramPacket",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("buf", "[B", ACC_PRIVATE, None),
        field("length", "I", ACC_PRIVATE, None),
        field("offset", "I", ACC_PRIVATE, None),
        field("address", "Ljava/net/InetAddress;", ACC_PRIVATE, None),
        field("port", "I", ACC_PRIVATE, None),
    ],
    &DATAGRAM_PACKET_METHODS,
    None,
);

pub(crate) const MULTICAST_SOCKET: oxjvm_vm::NativeClass = class(
    "java/net/MulticastSocket",
    Some("java/net/DatagramSocket"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

fn init_url_class_loader(
    vm: &mut Vm<'_>,
    instance: ObjectRef,
    urls: ObjectRef,
    parent: ObjectRef,
) -> Result<(), VmError> {
    let class = vm.class_of(instance);
    let urls = if urls.is_null() {
        let url_class = vm.resolve_class("java/net/URL")?;
        vm.allocate_object_array(url_class, 0)?
    } else {
        urls
    };
    if let Some((declaring, field)) = vm.find_field_opt(class, "urls", "[Ljava/net/URL;") {
        vm.set_instance_ref(instance, declaring, field, urls);
    }
    if let Some((declaring, field)) = vm.find_field_opt(class, "parent", "Ljava/lang/ClassLoader;")
    {
        vm.set_instance_ref(instance, declaring, field, parent);
    }
    Ok(())
}

fn new_url_class_loader_value(
    vm: &mut Vm<'_>,
    urls: ObjectRef,
    parent: ObjectRef,
) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/net/URLClassLoader")?;
    let instance = vm.new_instance(class)?;
    init_url_class_loader(vm, instance, urls, parent)?;
    object(instance)
}

const URL_CLASS_LOADER_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method("<init>", "([Ljava/net/URL;)V", ACC_PUBLIC, |vm, _, args| {
        let parent = crate::lang::system_class_loader(vm)?;
        init_url_class_loader(vm, receiver(args), ref_arg(args, 1), parent)?;
        void()
    }),
    method(
        "<init>",
        "([Ljava/net/URL;Ljava/lang/ClassLoader;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            init_url_class_loader(vm, receiver(args), ref_arg(args, 1), ref_arg(args, 2))?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;[Ljava/net/URL;Ljava/lang/ClassLoader;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            init_url_class_loader(vm, receiver(args), ref_arg(args, 2), ref_arg(args, 3))?;
            void()
        },
    ),
    method("getURLs", "()[Ljava/net/URL;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let urls = vm
            .read_ref_field(this, vm.class_of(this), "urls", "[Ljava/net/URL;")
            .unwrap_or(ObjectRef::NULL);
        object(urls)
    }),
    method(
        "newInstance",
        "([Ljava/net/URL;)Ljava/net/URLClassLoader;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let parent = crate::lang::system_class_loader(vm)?;
            new_url_class_loader_value(vm, ref_arg(args, 0), parent)
        },
    ),
    method(
        "newInstance",
        "([Ljava/net/URL;Ljava/lang/ClassLoader;)Ljava/net/URLClassLoader;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| new_url_class_loader_value(vm, ref_arg(args, 0), ref_arg(args, 1)),
    ),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "addURL",
        "(Ljava/net/URL;)V",
        ACC_PROTECTED,
        |vm, _, args| {
            let this = receiver(args);
            let url = ref_arg(args, 1);
            let class = vm.class_of(this);
            let urls = vm
                .read_ref_field(this, class, "urls", "[Ljava/net/URL;")
                .unwrap_or(ObjectRef::NULL);
            let length = if urls.is_null() {
                0
            } else {
                vm.array_length(urls)?
            };
            let url_class = vm.resolve_class("java/net/URL")?;
            let array = vm.allocate_object_array(url_class, length + 1)?;
            for index in 0..length {
                let element = vm.array_get(urls, index as i32)?.as_ref();
                vm.array_set_ref(array, index, element)?;
            }
            vm.array_set_ref(array, length, url)?;
            let (declaring, field) = vm.find_field(class, "urls", "[Ljava/net/URL;")?;
            vm.set_instance_ref(this, declaring, field, array);
            void()
        },
    ),
];

pub(crate) const URL_CLASS_LOADER: oxjvm_vm::NativeClass = class(
    "java/net/URLClassLoader",
    Some("java/lang/ClassLoader"),
    &[],
    ACC_PUBLIC,
    &[field("urls", "[Ljava/net/URL;", ACC_PRIVATE, None)],
    &URL_CLASS_LOADER_METHODS,
    None,
);

pub(crate) const AUTHENTICATOR: oxjvm_vm::NativeClass = class(
    "java/net/Authenticator",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[field(
        "theDefault",
        "Ljava/net/Authenticator;",
        ACC_PRIVATE | ACC_STATIC,
        None,
    )],
    &AUTHENTICATOR_METHODS,
    None,
);

const AUTHENTICATOR_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "setDefault",
        "(Ljava/net/Authenticator;)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let class = vm.resolve_class("java/net/Authenticator")?;
            vm.set_static_value(
                class,
                "theDefault",
                "Ljava/net/Authenticator;",
                Value::Ref(ref_arg(args, 0)),
            );
            void()
        },
    ),
    method(
        "getDefault",
        "()Ljava/net/Authenticator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/net/Authenticator")?;
            object(
                vm.read_static_ref(class, "theDefault", "Ljava/net/Authenticator;")
                    .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
];

const PROXY_SELECTOR_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "getDefault",
        "()Ljava/net/ProxySelector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let security = vm.resolve_class("java/net/ProxySelector")?;
            vm.ensure_initialized(security)?;
            object(
                vm.read_static_ref(security, "defaultSelector", "Ljava/net/ProxySelector;")
                    .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method(
        "setDefault",
        "(Ljava/net/ProxySelector;)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let security = vm.resolve_class("java/net/ProxySelector")?;
            vm.set_static_value(
                security,
                "defaultSelector",
                "Ljava/net/ProxySelector;",
                Value::Ref(ref_arg(args, 0)),
            );
            void()
        },
    ),
    method(
        "select",
        "(Ljava/net/URI;)Ljava/util/List;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "of",
        "(Ljava/net/InetSocketAddress;)Ljava/net/ProxySelector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let address = ref_arg(args, 0);
            let class = vm.resolve_class("java/net/OjvmProxySelector")?;
            let instance = vm.new_instance(class)?;
            let (declaring, field) = vm.find_field(class, "address", "Ljava/net/SocketAddress;")?;
            vm.set_instance_ref(instance, declaring, field, address);
            object(instance)
        },
    ),
    method(
        "connectFailed",
        "(Ljava/net/URI;Ljava/net/SocketAddress;Ljava/io/IOException;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const PROXY_SELECTOR: oxjvm_vm::NativeClass = class(
    "java/net/ProxySelector",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[field(
        "defaultSelector",
        "Ljava/net/ProxySelector;",
        ACC_PRIVATE | ACC_STATIC,
        None,
    )],
    &PROXY_SELECTOR_METHODS,
    None,
);

const OJVM_PROXY_SELECTOR_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "select",
        "(Ljava/net/URI;)Ljava/util/List;",
        ACC_PUBLIC,
        |vm, _, args| {
            let uri = ref_arg(args, 1);
            if uri.is_null() {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("URI can't be null"),
                ));
            }
            let (class, method) =
                vm.resolve_virtual_method(uri, "getScheme", "()Ljava/lang/String;")?;
            let scheme = vm.invoke_method(class, method, vec![Value::Ref(uri)])?;
            let scheme = vm.string_or_empty(scheme.as_ref());
            if scheme.is_empty() {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("protocol can't be null"),
                ));
            }
            let this = receiver(args);
            let address = vm
                .read_ref_field(
                    this,
                    vm.class_of(this),
                    "address",
                    "Ljava/net/SocketAddress;",
                )
                .unwrap_or(ObjectRef::NULL);
            let proxy_class = vm.resolve_class("java/net/Proxy")?;
            vm.ensure_initialized(proxy_class)?;
            let use_proxy = !address.is_null()
                && (scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"));
            let proxy = if use_proxy {
                let types = vm.resolve_class("java/net/Proxy$Type")?;
                vm.ensure_initialized(types)?;
                let http = vm
                    .read_static_ref(types, "HTTP", "Ljava/net/Proxy$Type;")
                    .ok_or_else(|| VmError::internal("Proxy.Type.HTTP missing"))?;
                let instance = vm.new_instance(proxy_class)?;
                let (declaring, ctor) = vm
                    .find_method(
                        proxy_class,
                        "<init>",
                        "(Ljava/net/Proxy$Type;Ljava/net/SocketAddress;)V",
                    )
                    .ok_or_else(|| {
                        VmError::internal("Proxy(Type,SocketAddress) constructor missing")
                    })?;
                vm.invoke_method(
                    declaring,
                    ctor,
                    vec![Value::Ref(instance), Value::Ref(http), Value::Ref(address)],
                )?;
                instance
            } else {
                vm.read_static_ref(proxy_class, "NO_PROXY", "Ljava/net/Proxy;")
                    .ok_or_else(|| VmError::internal("Proxy.NO_PROXY missing"))?
            };
            object(crate::util::array_list_from(vm, &[proxy])?)
        },
    ),
    method(
        "connectFailed",
        "(Ljava/net/URI;Ljava/net/SocketAddress;Ljava/io/IOException;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            for index in 1..=3 {
                if ref_arg(args, index).is_null() {
                    return Err(vm.throw_new(
                        "java/lang/IllegalArgumentException",
                        Some("Arguments can't be null."),
                    ));
                }
            }
            void()
        },
    ),
];

pub(crate) const OJVM_PROXY_SELECTOR: oxjvm_vm::NativeClass = class(
    "java/net/OjvmProxySelector",
    Some("java/net/ProxySelector"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field(
        "address",
        "Ljava/net/SocketAddress;",
        ACC_PRIVATE,
        None,
    )],
    &OJVM_PROXY_SELECTOR_METHODS,
    None,
);

pub(crate) const COOKIE_HANDLER: oxjvm_vm::NativeClass = class(
    "java/net/CookieHandler",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const HTTP_URL_CONNECTION: oxjvm_vm::NativeClass = class(
    "java/net/HttpURLConnection",
    Some("java/net/URLConnection"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

/// Split a SocketPermission name into host and inclusive port range. A
/// bracketed IPv6 host keeps its brackets; otherwise the last colon separates
/// the port. An absent port means the whole range.
fn socket_permission_parts(name: &str) -> (String, i32, i32) {
    let (host, port) = if let Some(rest) = name.strip_prefix('[') {
        match rest.find(']') {
            Some(index) => (String::from(&name[..index + 2]), &rest[index + 1..]),
            None => (String::from(name), ""),
        }
    } else {
        match name.rfind(':') {
            Some(index) => (String::from(&name[..index]), &name[index..]),
            None => (String::from(name), ""),
        }
    };
    let port = port.strip_prefix(':').unwrap_or("");
    let (low, high) = if port.is_empty() {
        (0, 65535)
    } else if let Some((from, to)) = port.split_once('-') {
        (
            if from.is_empty() {
                0
            } else {
                from.parse().unwrap_or(0)
            },
            if to.is_empty() {
                65535
            } else {
                to.parse().unwrap_or(65535)
            },
        )
    } else {
        let value = port.parse().unwrap_or(0);
        (value, value)
    };
    (host, low, high)
}

/// The address bytes of an IP literal (brackets stripped, IPv4-mapped IPv6
/// folded to IPv4), or `None` for a hostname.
fn socket_ip_bytes(text: &str) -> Option<Vec<u8>> {
    let text = if text.starts_with('[') && text.ends_with(']') {
        &text[1..text.len() - 1]
    } else {
        text
    };
    if let Some(bytes) = parse_ipv4(text) {
        return Some(bytes.to_vec());
    }
    if let Some((bytes, _)) = parse_ipv6(text) {
        if let Some(v4) = mapped_v4(&bytes) {
            return Some(v4.to_vec());
        }
        return Some(bytes.to_vec());
    }
    None
}

fn socket_host_matches(pattern: &str, host: &str) -> bool {
    match (socket_ip_bytes(pattern), socket_ip_bytes(host)) {
        (Some(a), Some(b)) => a == b,
        (None, None) => {
            let pattern = pattern.to_ascii_lowercase();
            let host = host.to_ascii_lowercase();
            if pattern == "*" {
                return true;
            }
            if let Some(suffix) = pattern.strip_prefix("*.") {
                // "*.example.com" covers subdomains only, not the apex itself.
                return host.ends_with(&alloc::format!(".{suffix}"));
            }
            pattern == host
        }
        // Without name resolution an IP literal never matches a hostname.
        _ => false,
    }
}

fn socket_actions_mask(actions: &str) -> u32 {
    let mut mask = 0;
    for action in actions.split(',') {
        match action.trim() {
            "accept" => mask |= 1,
            "connect" => mask |= 2,
            "listen" => mask |= 4,
            "resolve" => mask |= 8,
            _ => {}
        }
    }
    // accept/connect/listen implicitly grant resolve, like the JDK.
    if mask & 0b111 != 0 {
        mask |= 8;
    }
    mask
}

fn socket_permission_implies(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let other = ref_arg(args, 1);
    if other.is_null() || vm.class_name(vm.class_of(other)) != "java/net/SocketPermission" {
        return boolean(false);
    }
    let this = receiver(args);
    let this_name = vm.read_string_field_any(this, "name").unwrap_or_default();
    let other_name = vm.read_string_field_any(other, "name").unwrap_or_default();
    let this_actions = vm
        .read_string_field_any(this, "actions")
        .unwrap_or_default();
    let other_actions = vm
        .read_string_field_any(other, "actions")
        .unwrap_or_default();
    let this_mask = socket_actions_mask(&this_actions);
    let other_mask = socket_actions_mask(&other_actions);
    if (this_mask & other_mask) != other_mask {
        return boolean(false);
    }
    let (this_host, this_low, this_high) = socket_permission_parts(&this_name);
    let (other_host, other_low, other_high) = socket_permission_parts(&other_name);
    if !socket_host_matches(&this_host, &other_host) {
        return boolean(false);
    }
    // A resolve-only permission is granted for any port.
    if (other_mask & !8) == 0 {
        return boolean(true);
    }
    boolean(other_low >= this_low && other_high <= this_high)
}

/// Split a SocketPermission name into host and raw port text (bracket-aware).
fn socket_permission_split(name: &str) -> (String, String) {
    if let Some(rest) = name.strip_prefix('[') {
        if let Some(index) = rest.find(']') {
            let port = rest[index + 1..].strip_prefix(':').unwrap_or("");
            return (String::from(&name[..index + 2]), String::from(port));
        }
    }
    match name.rfind(':') {
        Some(index) => (
            String::from(&name[..index]),
            String::from(&name[index + 1..]),
        ),
        None => (String::from(name), String::new()),
    }
}

/// Numeric port range for equality: an absent port is (-1, -1).
fn socket_port_numeric(port_text: &str) -> (i32, i32) {
    if port_text.is_empty() {
        return (-1, -1);
    }
    if let Some((from, to)) = port_text.split_once('-') {
        (
            if from.is_empty() {
                0
            } else {
                from.parse().unwrap_or(0)
            },
            if to.is_empty() {
                65535
            } else {
                to.parse().unwrap_or(65535)
            },
        )
    } else {
        let value = port_text.parse().unwrap_or(0);
        (value, value)
    }
}

/// Validate the action list and render it in the JDK's canonical order
/// (connect,listen,accept,resolve); accept/connect/listen imply resolve.
fn socket_actions_canonical(vm: &mut Vm<'_>, actions: &str) -> Result<String, VmError> {
    let mut mask = 0u32;
    for token in actions.split(',') {
        match token.trim().to_ascii_lowercase().as_str() {
            "accept" => mask |= 1,
            "connect" => mask |= 2,
            "listen" => mask |= 4,
            "resolve" => mask |= 8,
            _ => {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some(&alloc::format!("invalid action: {actions}")),
                ));
            }
        }
    }
    if mask & 0b111 != 0 {
        mask |= 8;
    }
    let mut parts: Vec<&str> = Vec::new();
    if mask & 2 != 0 {
        parts.push("connect");
    }
    if mask & 4 != 0 {
        parts.push("listen");
    }
    if mask & 1 != 0 {
        parts.push("accept");
    }
    if mask & 8 != 0 {
        parts.push("resolve");
    }
    Ok(parts.join(","))
}

/// True for a 1-4 digit hexadecimal IPv6 group.
fn socket_hex_group(text: &str) -> bool {
    !text.is_empty() && text.len() <= 4 && text.chars().all(|c| c.is_ascii_hexdigit())
}

/// Recognize a full (non-compressed) eight-group IPv6 literal written without
/// brackets, optionally followed by ":port" or carrying a "%scope" on the last
/// group, and return the bracketed form the JDK stores. Compressed (`::`)
/// forms are not recognized here; they fall through to the ambiguous-hostport
/// rejection, exactly like the JDK.
fn socket_normalize_v6(name: &str) -> Option<String> {
    if name.starts_with('[') || name.is_empty() {
        return None;
    }
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() != 8 && parts.len() != 9 {
        return None;
    }
    let mut groups: Vec<String> = Vec::new();
    for (index, part) in parts[..8].iter().enumerate() {
        let group = if index == 7 {
            match part.split_once('%') {
                Some((hex, scope)) => {
                    if !socket_hex_group(hex)
                        || scope.is_empty()
                        || !scope.chars().all(|c| c.is_ascii_digit())
                    {
                        return None;
                    }
                    alloc::format!("{hex}%{scope}")
                }
                None => {
                    if !socket_hex_group(part) {
                        return None;
                    }
                    String::from(*part)
                }
            }
        } else {
            if !socket_hex_group(part) {
                return None;
            }
            String::from(*part)
        };
        groups.push(group);
    }
    let host = alloc::format!("[{}]", groups.join(":"));
    if parts.len() == 9 {
        Some(alloc::format!("{host}:{}", parts[8]))
    } else {
        Some(host)
    }
}

/// Validate a SocketPermission host/port name and normalize it the way the JDK
/// constructor does: an empty host becomes "localhost", and a full unbracketed
/// IPv6 literal is bracketed.
fn socket_permission_validate(vm: &mut Vm<'_>, name: &str) -> Result<String, VmError> {
    let normalized = if let Some(rest) = name.strip_prefix('[') {
        match rest.find(']') {
            Some(index) => {
                let after = &rest[index + 1..];
                if !after.is_empty() && !after.starts_with(':') {
                    return Err(vm.throw_new(
                        "java/lang/IllegalArgumentException",
                        Some("invalid hostport part"),
                    ));
                }
            }
            None => {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("invalid hostport part"),
                ));
            }
        }
        String::from(name)
    } else if let Some(v6) = socket_normalize_v6(name) {
        v6
    } else {
        if let Some(index) = name.rfind(':') {
            if name[..index].contains(':') {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("Ambiguous hostport part"),
                ));
            }
        }
        if name.is_empty() {
            String::from("localhost")
        } else {
            String::from(name)
        }
    };
    let (_, port_text) = socket_permission_split(&normalized);
    if !port_text.is_empty() {
        let ok = if let Some((from, to)) = port_text.split_once('-') {
            let low = if from.is_empty() {
                Some(0)
            } else {
                from.parse::<i32>().ok()
            };
            let high = if to.is_empty() {
                Some(65535)
            } else {
                to.parse::<i32>().ok()
            };
            match (low, high) {
                (Some(l), Some(h)) if l <= h => true,
                (Some(_), Some(_)) => {
                    return Err(vm.throw_new(
                        "java/lang/IllegalArgumentException",
                        Some(&alloc::format!("invalid port range: {port_text}")),
                    ));
                }
                _ => false,
            }
        } else {
            port_text.parse::<i32>().is_ok()
        };
        if !ok {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some(&alloc::format!("invalid port: {port_text}")),
            ));
        }
    }
    Ok(normalized)
}

/// SocketPermission.equals: same action mask, same host (IP literals by
/// address bytes, hostnames case-insensitively) and same port. A resolve-only
/// wildcard ignores the port; IP ports compare numerically, hostname ports as
/// written.
fn socket_permission_equals(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    other: ObjectRef,
) -> Result<Value, VmError> {
    let this_name = vm.read_string_field_any(this, "name").unwrap_or_default();
    let other_name = vm.read_string_field_any(other, "name").unwrap_or_default();
    let this_mask = socket_actions_mask(
        &vm.read_string_field_any(this, "actions")
            .unwrap_or_default(),
    );
    let other_mask = socket_actions_mask(
        &vm.read_string_field_any(other, "actions")
            .unwrap_or_default(),
    );
    if this_mask != other_mask {
        return boolean(false);
    }
    let (this_host, this_port) = socket_permission_split(&this_name);
    let (other_host, other_port) = socket_permission_split(&other_name);
    match (socket_ip_bytes(&this_host), socket_ip_bytes(&other_host)) {
        (Some(a), Some(b)) => {
            if a != b {
                return boolean(false);
            }
            let (al, ah) = socket_port_numeric(&this_port);
            let (bl, bh) = socket_port_numeric(&other_port);
            boolean(al == bl && ah == bh)
        }
        (None, None) => {
            if !this_host.eq_ignore_ascii_case(&other_host) {
                return boolean(false);
            }
            if this_mask == 8 && this_host.starts_with('*') {
                return boolean(true);
            }
            boolean(this_port == other_port)
        }
        _ => boolean(false),
    }
}

pub(crate) const SOCKET_PERMISSION: oxjvm_vm::NativeClass = class(
    "java/net/SocketPermission",
    Some("java/security/Permission"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("actions", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &[
        method(
            "<init>",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            ACC_PUBLIC,
            |vm, _, args| {
                let this = receiver(args);
                let class = vm.class_of(this);
                let name = string_arg(vm, args[1])?;
                let actions = string_arg(vm, args[2])?;
                let canonical = socket_actions_canonical(vm, &actions)?;
                let name = socket_permission_validate(vm, &name)?;
                vm.set_string_field(this, class, "name", &name)?;
                vm.set_string_field(this, class, "actions", &canonical)?;
                void()
            },
        ),
        method(
            "getName",
            "()Ljava/lang/String;",
            ACC_PUBLIC,
            |vm, _, args| {
                let text = vm
                    .read_string_field_any(receiver(args), "name")
                    .unwrap_or_default();
                object(vm.make_string(&text)?)
            },
        ),
        method(
            "getActions",
            "()Ljava/lang/String;",
            ACC_PUBLIC,
            |vm, _, args| {
                let text = vm
                    .read_string_field_any(receiver(args), "actions")
                    .unwrap_or_default();
                object(vm.make_string(&text)?)
            },
        ),
        method(
            "implies",
            "(Ljava/security/Permission;)Z",
            ACC_PUBLIC,
            |vm, _, args| socket_permission_implies(vm, args),
        ),
        method(
            "equals",
            "(Ljava/lang/Object;)Z",
            ACC_PUBLIC,
            |vm, _, args| {
                let other = ref_arg(args, 1);
                if other.is_null()
                    || vm.class_name(vm.class_of(other)) != "java/net/SocketPermission"
                {
                    return boolean(false);
                }
                socket_permission_equals(vm, receiver(args), other)
            },
        ),
        method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
            let name = vm
                .read_string_field_any(receiver(args), "name")
                .unwrap_or_default();
            let mut hash = 0i32;
            for unit in name.encode_utf16() {
                hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
            }
            int(hash)
        }),
        method(
            "toString",
            "()Ljava/lang/String;",
            ACC_PUBLIC,
            |vm, _, args| {
                let this = receiver(args);
                let name = vm.read_string_field_any(this, "name").unwrap_or_default();
                let actions = vm
                    .read_string_field_any(this, "actions")
                    .unwrap_or_default();
                let text =
                    alloc::format!("(\"java.net.SocketPermission\" \"{name}\" \"{actions}\")");
                object(vm.make_string(&text)?)
            },
        ),
        method(
            "newPermissionCollection",
            "()Ljava/security/PermissionCollection;",
            ACC_PUBLIC,
            |vm, _, _| {
                let class = vm.resolve_class("java/net/SocketPermissionCollection")?;
                let instance = vm.new_instance(class)?;
                let (declaring, ctor) = vm
                    .find_method(class, "<init>", "()V")
                    .ok_or_else(|| VmError::internal("SocketPermissionCollection() missing"))?;
                vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
                object(instance)
            },
        ),
    ],
    None,
);

/// The ArrayList backing a SocketPermissionCollection.
fn socket_collection_list(vm: &mut Vm<'_>, this: ObjectRef) -> Result<ObjectRef, VmError> {
    let class = vm.class_of(this);
    Ok(vm
        .read_ref_field(this, class, "perms", "Ljava/util/ArrayList;")
        .unwrap_or(ObjectRef::NULL))
}

/// SocketPermissionCollection.implies: union the action masks of every stored
/// permission whose host matches and whose port range covers the query (a
/// resolve-only query ignores ports), then check the union grants the query.
fn socket_collection_implies(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let other = ref_arg(args, 1);
    if other.is_null() || vm.class_name(vm.class_of(other)) != "java/net/SocketPermission" {
        return boolean(false);
    }
    let list = socket_collection_list(vm, receiver(args))?;
    if list.is_null() {
        return boolean(false);
    }
    let perms = crate::util::array_list_elements(vm, list);
    let other_name = vm.read_string_field_any(other, "name").unwrap_or_default();
    let other_actions = vm
        .read_string_field_any(other, "actions")
        .unwrap_or_default();
    let other_mask = socket_actions_mask(&other_actions);
    let (other_host, other_low, other_high) = socket_permission_parts(&other_name);
    let mut union = 0u32;
    for perm in perms {
        if perm.is_null() || vm.class_name(vm.class_of(perm)) != "java/net/SocketPermission" {
            continue;
        }
        let name = vm.read_string_field_any(perm, "name").unwrap_or_default();
        let actions = vm
            .read_string_field_any(perm, "actions")
            .unwrap_or_default();
        let mask = socket_actions_mask(&actions);
        let (host, low, high) = socket_permission_parts(&name);
        if !socket_host_matches(&host, &other_host) {
            continue;
        }
        if (other_mask & !8) != 0 && (other_low < low || other_high > high) {
            continue;
        }
        union |= mask;
    }
    boolean((union & other_mask) == other_mask)
}

/// Render an action mask in the JDK's canonical order.
fn socket_actions_render(mask: u32) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if mask & 2 != 0 {
        parts.push("connect");
    }
    if mask & 4 != 0 {
        parts.push("listen");
    }
    if mask & 1 != 0 {
        parts.push("accept");
    }
    if mask & 8 != 0 {
        parts.push("resolve");
    }
    parts.join(",")
}

/// Construct a SocketPermission natively (used when merging collection entries).
fn new_socket_permission(vm: &mut Vm<'_>, name: &str, actions: &str) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/net/SocketPermission")?;
    let instance = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(class, "<init>", "(Ljava/lang/String;Ljava/lang/String;)V")
        .ok_or_else(|| VmError::internal("SocketPermission() missing"))?;
    let name_val = vm.make_string(name)?;
    let actions_val = vm.make_string(actions)?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![
            Value::Ref(instance),
            Value::Ref(name_val),
            Value::Ref(actions_val),
        ],
    )?;
    Ok(instance)
}

/// SocketPermissionCollection.add: permissions with the same name are merged
/// (their action masks unioned) rather than duplicated, like the JDK.
fn socket_collection_add(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let perm = ref_arg(args, 1);
    if perm.is_null() || vm.class_name(vm.class_of(perm)) != "java/net/SocketPermission" {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("invalid permission type"),
        ));
    }
    crate::security::ensure_collection_writable(vm, receiver(args))?;
    let list = socket_collection_list(vm, receiver(args))?;
    let name = vm.read_string_field_any(perm, "name").unwrap_or_default();
    let actions = vm
        .read_string_field_any(perm, "actions")
        .unwrap_or_default();
    let mask = socket_actions_mask(&actions);
    let perms = crate::util::array_list_elements(vm, list);
    let mut existing = None;
    for (index, stored) in perms.iter().enumerate() {
        if !stored.is_null()
            && vm
                .read_string_field_any(*stored, "name")
                .unwrap_or_default()
                == name
        {
            existing = Some(index);
            break;
        }
    }
    match existing {
        Some(index) => {
            let old_actions = vm
                .read_string_field_any(perms[index], "actions")
                .unwrap_or_default();
            let merged = socket_actions_mask(&old_actions) | mask;
            let merged_perm = new_socket_permission(vm, &name, &socket_actions_render(merged))?;
            let (declaring, set) =
                vm.resolve_virtual_method(list, "set", "(ILjava/lang/Object;)Ljava/lang/Object;")?;
            vm.invoke_method(
                declaring,
                set,
                vec![
                    Value::Ref(list),
                    int(index as i32)?,
                    Value::Ref(merged_perm),
                ],
            )?;
        }
        None => {
            let (declaring, add) =
                vm.resolve_virtual_method(list, "add", "(Ljava/lang/Object;)Z")?;
            vm.invoke_method(declaring, add, vec![Value::Ref(list), Value::Ref(perm)])?;
        }
    }
    void()
}

pub(crate) const SOCKET_PERMISSION_COLLECTION: oxjvm_vm::NativeClass = class(
    "java/net/SocketPermissionCollection",
    Some("java/security/PermissionCollection"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[field("perms", "Ljava/util/ArrayList;", ACC_PRIVATE, None)],
    &[
        method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let list = vm.resolve_class("java/util/ArrayList")?;
            let instance = vm.new_instance(list)?;
            let (declaring, ctor) = vm
                .find_method(list, "<init>", "()V")
                .ok_or_else(|| VmError::internal("ArrayList() missing"))?;
            vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
            let (declaring, field) = vm.find_field(class, "perms", "Ljava/util/ArrayList;")?;
            vm.set_instance_ref(this, declaring, field, instance);
            void()
        }),
        method(
            "add",
            "(Ljava/security/Permission;)V",
            ACC_PUBLIC,
            |vm, _, args| socket_collection_add(vm, args),
        ),
        method(
            "implies",
            "(Ljava/security/Permission;)Z",
            ACC_PUBLIC,
            |vm, _, args| socket_collection_implies(vm, args),
        ),
        method(
            "elements",
            "()Ljava/util/Enumeration;",
            ACC_PUBLIC,
            |vm, _, args| {
                let list = socket_collection_list(vm, receiver(args))?;
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

pub(crate) const URL_PERMISSION: oxjvm_vm::NativeClass = class(
    "java/net/URLPermission",
    Some("java/security/Permission"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &[],
    None,
);

fn proxy_type_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/net/Proxy$Type")?;
    for (ordinal, name) in ["DIRECT", "HTTP", "SOCKS"].iter().enumerate() {
        let instance = vm.new_instance(class)?;
        vm.set_string_field(instance, class, "name", name)?;
        if let Some((declaring, field)) = vm.find_field_opt(class, "ordinal", "I") {
            vm.set_instance_int(instance, declaring, field, ordinal as i32);
        }
        vm.set_static_value(class, name, "Ljava/net/Proxy$Type;", Value::Ref(instance));
    }
    Ok(())
}

pub(crate) const PROXY_TYPE: oxjvm_vm::NativeClass = class(
    "java/net/Proxy$Type",
    Some("java/lang/Enum"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_ENUM,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE | ACC_FINAL, None),
        field("ordinal", "I", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "DIRECT",
            "Ljava/net/Proxy$Type;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "HTTP",
            "Ljava/net/Proxy$Type;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "SOCKS",
            "Ljava/net/Proxy$Type;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
    ],
    &[
        method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
            let this = receiver(args);
            let text = vm
                .read_string_field(this, vm.class_of(this), "name")
                .unwrap_or_default();
            object(vm.make_string(&text)?)
        }),
        method(
            "toString",
            "()Ljava/lang/String;",
            ACC_PUBLIC,
            |vm, _, args| {
                let this = receiver(args);
                let text = vm
                    .read_string_field(this, vm.class_of(this), "name")
                    .unwrap_or_default();
                object(vm.make_string(&text)?)
            },
        ),
        method("ordinal", "()I", ACC_PUBLIC, |vm, _, args| {
            int(crate::lang::read_int_field(vm, receiver(args), "ordinal"))
        }),
        method(
            "values",
            "()[Ljava/net/Proxy$Type;",
            ACC_PUBLIC | ACC_STATIC,
            |vm, _, _| {
                let class = vm.resolve_class("java/net/Proxy$Type")?;
                vm.ensure_initialized(class)?;
                let array = vm.allocate_object_array(class, 3)?;
                for (index, name) in ["DIRECT", "HTTP", "SOCKS"].iter().enumerate() {
                    if let Some(value) =
                        vm.read_static_ref_if_set(class, name, "Ljava/net/Proxy$Type;")
                    {
                        vm.array_set_ref(array, index, value)?;
                    }
                }
                object(array)
            },
        ),
        method(
            "valueOf",
            "(Ljava/lang/String;)Ljava/net/Proxy$Type;",
            ACC_PUBLIC | ACC_STATIC,
            |vm, _, args| {
                let name = vm.string_or_empty(ref_arg(args, 0));
                let class = vm.resolve_class("java/net/Proxy$Type")?;
                vm.ensure_initialized(class)?;
                match vm.read_static_ref_if_set(class, &name, "Ljava/net/Proxy$Type;") {
                    Some(value) => object(value),
                    None => Err(vm.throw_new("java/lang/IllegalArgumentException", Some(&name))),
                }
            },
        ),
    ],
    Some(proxy_type_clinit),
);

fn proxy_init(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let proxy_type = ref_arg(args, 1);
    let address = ref_arg(args, 2);
    if proxy_type.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "type", "Ljava/net/Proxy$Type;")?;
    vm.set_instance_ref(this, declaring, field, proxy_type);
    let type_name = vm
        .read_string_field(proxy_type, vm.class_of(proxy_type), "name")
        .unwrap_or_default();
    let address_ref = if type_name == "DIRECT" || address.is_null() {
        ObjectRef::NULL
    } else if let Some(text) = vm.string_value(address) {
        if text.is_empty() {
            ObjectRef::NULL
        } else {
            // A host:port pair.
            let (host, port) = text.rsplit_once(':').unwrap_or((text.as_str(), "0"));
            let address_class = vm.resolve_class("java/net/InetSocketAddress")?;
            let instance = vm.new_instance(address_class)?;
            let (d, ctor) = vm
                .find_method(address_class, "<init>", "(Ljava/lang/String;I)V")
                .ok_or_else(|| VmError::internal("InetSocketAddress(String,int) missing"))?;
            let host = vm.make_string(host)?;
            vm.invoke_method(
                d,
                ctor,
                vec![
                    Value::Ref(instance),
                    Value::Ref(host),
                    Value::Int(port.parse().unwrap_or(0)),
                ],
            )?;
            instance
        }
    } else {
        address
    };
    let (declaring, field) = vm.find_field(class, "address", "Ljava/net/SocketAddress;")?;
    vm.set_instance_ref(this, declaring, field, address_ref);
    void()
}

fn proxy_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/net/Proxy")?;
    let types = vm.resolve_class("java/net/Proxy$Type")?;
    vm.ensure_initialized(types)?;
    let direct = vm
        .read_static_ref(types, "DIRECT", "Ljava/net/Proxy$Type;")
        .ok_or_else(|| VmError::internal("Proxy.Type.DIRECT missing"))?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "type", "Ljava/net/Proxy$Type;")?;
    vm.set_instance_ref(instance, declaring, field, direct);
    vm.set_static_value(class, "NO_PROXY", "Ljava/net/Proxy;", Value::Ref(instance));
    Ok(())
}

pub(crate) const PROXY: oxjvm_vm::NativeClass = class(
    "java/net/Proxy",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("type", "Ljava/net/Proxy$Type;", ACC_PRIVATE, None),
        field("address", "Ljava/net/SocketAddress;", ACC_PRIVATE, None),
        field(
            "NO_PROXY",
            "Ljava/net/Proxy;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &[
        method("<init>", "()V", ACC_PRIVATE, |_, _, _| void()),
        method(
            "<init>",
            "(Ljava/net/Proxy$Type;Ljava/net/SocketAddress;)V",
            ACC_PUBLIC,
            proxy_init,
        ),
        method(
            "<init>",
            "(Ljava/net/Proxy$Type;Ljava/lang/String;)V",
            ACC_PUBLIC,
            proxy_init,
        ),
        method(
            "type",
            "()Ljava/net/Proxy$Type;",
            ACC_PUBLIC,
            |vm, _, args| {
                let this = receiver(args);
                let class = vm.class_of(this);
                object(
                    vm.read_ref_field(this, class, "type", "Ljava/net/Proxy$Type;")
                        .unwrap_or(ObjectRef::NULL),
                )
            },
        ),
        method(
            "address",
            "()Ljava/net/SocketAddress;",
            ACC_PUBLIC,
            |vm, _, args| {
                let this = receiver(args);
                let class = vm.class_of(this);
                object(
                    vm.read_ref_field(this, class, "address", "Ljava/net/SocketAddress;")
                        .unwrap_or(ObjectRef::NULL),
                )
            },
        ),
        method(
            "toString",
            "()Ljava/lang/String;",
            ACC_PUBLIC,
            |vm, _, args| {
                let this = receiver(args);
                let class = vm.class_of(this);
                let proxy_type = vm
                    .read_ref_field(this, class, "type", "Ljava/net/Proxy$Type;")
                    .unwrap_or(ObjectRef::NULL);
                let name = vm
                    .read_string_field(proxy_type, vm.class_of(proxy_type), "name")
                    .unwrap_or_default();
                let address = vm
                    .read_ref_field(this, class, "address", "Ljava/net/SocketAddress;")
                    .unwrap_or(ObjectRef::NULL);
                if address.is_null() {
                    object(vm.make_string(&name)?)
                } else {
                    let (c, m) =
                        vm.resolve_virtual_method(address, "toString", "()Ljava/lang/String;")?;
                    let text = vm.invoke_method(c, m, vec![Value::Ref(address)])?;
                    let address_text = vm.string_or_empty(text.as_ref());
                    object(vm.make_string(&alloc::format!("{name} @ {address_text}"))?)
                }
            },
        ),
        method(
            "equals",
            "(Ljava/lang/Object;)Z",
            ACC_PUBLIC,
            |vm, _, args| {
                let other = ref_arg(args, 1);
                if other.is_null() || vm.class_of(other) != vm.class_of(receiver(args)) {
                    return boolean(false);
                }
                let this = receiver(args);
                let class = vm.class_of(this);
                let this_type = vm
                    .read_ref_field(this, class, "type", "Ljava/net/Proxy$Type;")
                    .unwrap_or(ObjectRef::NULL);
                let other_type = vm
                    .read_ref_field(other, vm.class_of(other), "type", "Ljava/net/Proxy$Type;")
                    .unwrap_or(ObjectRef::NULL);
                let this_address = vm
                    .read_ref_field(this, class, "address", "Ljava/net/SocketAddress;")
                    .unwrap_or(ObjectRef::NULL);
                let other_address = vm
                    .read_ref_field(
                        other,
                        vm.class_of(other),
                        "address",
                        "Ljava/net/SocketAddress;",
                    )
                    .unwrap_or(ObjectRef::NULL);
                boolean(this_type == other_type && this_address == other_address)
            },
        ),
        method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let address = vm
                .read_ref_field(this, class, "address", "Ljava/net/SocketAddress;")
                .unwrap_or(ObjectRef::NULL);
            int(vm.heap.get(address).map_or(0, |object| object.hash))
        }),
    ],
    Some(proxy_clinit),
);

const INET6_ADDRESS_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("getScopeId", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "scope_id"))
    }),
    method(
        "isIPv4CompatibleAddress",
        "()Z",
        ACC_PUBLIC,
        |vm, _, args| {
            address_predicate(vm, receiver(args), |bytes| {
                bytes.len() == 16 && bytes[..12].iter().all(|&b| b == 0)
            })
        },
    ),
    method(
        "getScopedInterface",
        "()Ljava/net/NetworkInterface;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getByAddress",
        "(Ljava/lang/String;[BI)Ljava/net/Inet6Address;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let scope = int_arg(args, 2);
            if scope < 0 {
                return Err(vm.throw_new("java/net/UnknownHostException", Some("scope_id < 0")));
            }
            inet6_by_address(vm, ref_arg(args, 0), ref_arg(args, 1), Some(scope as u32))
        },
    ),
    method(
        "getByAddress",
        "(Ljava/lang/String;[BLjava/net/NetworkInterface;)Ljava/net/Inet6Address;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if !ref_arg(args, 2).is_null() {
                // No host network interfaces are exposed, so a scoped
                // interface cannot be resolved.
                return Err(vm.throw_new(
                    "java/net/UnknownHostException",
                    Some("no such network interface"),
                ));
            }
            inet6_by_address(vm, ref_arg(args, 0), ref_arg(args, 1), None)
        },
    ),
];

/// `Inet6Address.getByAddress` requires exactly 16 bytes and, unlike
/// `InetAddress.getByAddress`, keeps IPv4-mapped addresses as `Inet6Address`.
fn inet6_by_address(
    vm: &mut Vm<'_>,
    host: ObjectRef,
    array: ObjectRef,
    scope: Option<u32>,
) -> Result<Value, VmError> {
    let length = if array.is_null() {
        0
    } else {
        vm.array_length(array)?
    };
    if length != 16 {
        return Err(vm.throw_new(
            "java/net/UnknownHostException",
            Some("addr is of illegal length"),
        ));
    }
    let mut bytes = [0u8; 16];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = vm.array_get(array, index as i32)?.as_int() as u8;
    }
    let host = if host.is_null() {
        None
    } else {
        vm.string_value(host)
    };
    new_inet6(vm, &bytes, scope, host.as_deref())
}

pub(crate) const INET6_ADDRESS: oxjvm_vm::NativeClass = class(
    "java/net/Inet6Address",
    Some("java/net/InetAddress"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[field("scope_id", "I", ACC_PRIVATE, None)],
    &INET6_ADDRESS_METHODS,
    None,
);
