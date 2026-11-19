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

/// The authority part (`//...` up to the path), without the leading slashes.
fn uri_authority(text: &str) -> Option<&str> {
    let rest = text.find(':').map(|index| &text[index + 1..])?;
    let rest = rest.strip_prefix("//")?;
    Some(rest.split(['/', '?', '#']).next().unwrap_or(rest))
}

/// The part after `marker` and before `end` (or the string end).
fn uri_part(text: &str, marker: char, end: char) -> Option<String> {
    let start = text.find(marker)? + marker.len_utf8();
    let rest = &text[start..];
    let value = if end == '\0' {
        rest
    } else {
        rest.split(end).next().unwrap_or(rest)
    };
    Some(String::from(value))
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

/// The components of a URI reference, split loosely on the raw text.
struct UriParts<'a> {
    scheme: Option<&'a str>,
    authority: Option<&'a str>,
    path: &'a str,
    query: Option<&'a str>,
    fragment: Option<&'a str>,
}

fn parse_uri(text: &str) -> UriParts<'_> {
    let (scheme, rest) = match text.split_once(':') {
        Some((scheme, rest))
            if scheme
                .chars()
                .next()
                .is_some_and(|first| first.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')) =>
        {
            (Some(scheme), rest)
        }
        _ => (None, text),
    };
    let (rest, fragment) = match rest.split_once('#') {
        Some((rest, fragment)) => (rest, Some(fragment)),
        None => (rest, None),
    };
    let (rest, query) = match rest.split_once('?') {
        Some((rest, query)) => (rest, Some(query)),
        None => (rest, None),
    };
    let (authority, path) = match rest.strip_prefix("//") {
        Some(after) => match after.find('/') {
            Some(index) => (Some(&after[..index]), &after[index..]),
            None => (Some(after), ""),
        },
        None => (None, rest),
    };
    UriParts {
        scheme,
        authority,
        path,
        query,
        fragment,
    }
}

/// Normalize dot segments in a URI path the way `java.net.URI` does: remove
/// "." segments and ".." segments together with their predecessor, collapse
/// redundant slashes, and keep leading ".." segments and any trailing slash.
pub(crate) fn normalize_uri_path(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    let absolute = path.starts_with('/');
    let trailing = path.ends_with('/');
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if segments.last().is_some_and(|prev| *prev != "..") {
                    segments.pop();
                } else {
                    segments.push("..");
                }
            }
            _ => segments.push(segment),
        }
    }
    // Prevent scheme-name confusion in a relative path.
    if !absolute && segments.first().is_some_and(|first| first.contains(':')) {
        segments.insert(0, ".");
    }
    let mut out = String::new();
    if absolute {
        out.push('/');
    }
    out.push_str(&segments.join("/"));
    if trailing && !out.ends_with('/') {
        out.push('/');
    }
    out
}

/// Merge a relative child path into the base path directory (RFC 2396 5.2).
fn resolve_uri_path(base: &str, child: &str, base_absolute: bool) -> String {
    let directory = match base.rfind('/') {
        Some(index) => &base[..=index],
        None => "",
    };
    let merged = if child.is_empty() {
        String::from(directory)
    } else if base.contains('/') || !base_absolute {
        alloc::format!("{directory}{child}")
    } else {
        alloc::format!("/{child}")
    };
    normalize_uri_path(&merged)
}

/// Resolve `child` against `base`, following `java.net.URI.resolve`.
fn resolve_uri(base: &str, child: &str) -> String {
    let base_parts = parse_uri(base);
    let child_parts = parse_uri(child);
    let opaque = |parts: &UriParts<'_>| {
        parts.scheme.is_some() && parts.authority.is_none() && !parts.path.starts_with('/')
    };
    if opaque(&child_parts) || opaque(&base_parts) {
        return String::from(child);
    }
    // A lone fragment replaces the base fragment and keeps everything else.
    if child_parts.scheme.is_none()
        && child_parts.authority.is_none()
        && child_parts.path.is_empty()
        && child_parts.query.is_none()
        && child_parts.fragment.is_some()
    {
        if base_parts.fragment == child_parts.fragment {
            return String::from(base);
        }
        let head = base.split('#').next().unwrap_or(base);
        return alloc::format!("{head}#{}", child_parts.fragment.unwrap_or_default());
    }
    if child_parts.scheme.is_some() {
        return String::from(child);
    }
    let (authority, path) = match child_parts.authority {
        Some(authority) => (Some(authority), String::from(child_parts.path)),
        None => {
            let path = if child_parts.path.starts_with('/') {
                String::from(child_parts.path)
            } else {
                resolve_uri_path(
                    base_parts.path,
                    child_parts.path,
                    base_parts.scheme.is_some(),
                )
            };
            (base_parts.authority, path)
        }
    };
    let mut out = String::new();
    if let Some(scheme) = base_parts.scheme {
        out.push_str(scheme);
        out.push(':');
    }
    if let Some(authority) = authority {
        out.push_str("//");
        out.push_str(authority);
    }
    out.push_str(&path);
    if let Some(query) = child_parts.query {
        out.push('?');
        out.push_str(query);
    }
    if let Some(fragment) = child_parts.fragment {
        out.push('#');
        out.push_str(fragment);
    }
    out
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

fn uri_init_parts(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let scheme = string_arg(vm, args[1])?;
    let user_info = string_arg(vm, args[2])?;
    let host = string_arg(vm, args[3])?;
    let port = args[4].as_int();
    let path = string_arg(vm, args[5])?;
    let query = string_arg(vm, args[6])?;
    let fragment = string_arg(vm, args[7])?;
    let mut text = String::new();
    if !scheme.is_empty() {
        text.push_str(&scheme);
        text.push(':');
    }
    if !host.is_empty() {
        text.push_str("//");
        if !user_info.is_empty() {
            text.push_str(&user_info);
            text.push('@');
        }
        text.push_str(&host);
        if port != -1 {
            text.push_str(&alloc::format!(":{port}"));
        }
    }
    text.push_str(&path);
    if !query.is_empty() {
        text.push('?');
        text.push_str(&query);
    }
    if !fragment.is_empty() {
        text.push('#');
        text.push_str(&fragment);
    }
    let this = receiver(args);
    vm.set_string_field(this, vm.class_of(this), "string", &text)?;
    void()
}

const URI_METHODS: [oxjvm_vm::NativeMethodDef; 27] = [
    method("<init>", "(Ljava/lang/String;)V", ACC_PUBLIC, uri_init),
    method(
        "compareTo",
        "(Ljava/net/URI;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = uri_text(vm, receiver(args));
            let other = uri_text(vm, ref_arg(args, 1));
            int(this.cmp(&other) as i32)
        },
    ),
    method(
        "compareTo",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC | ACC_BRIDGE | ACC_SYNTHETIC,
        |vm, _, args| {
            let this = uri_text(vm, receiver(args));
            let other = uri_text(vm, ref_arg(args, 1));
            int(this.cmp(&other) as i32)
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;ILjava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        uri_init_parts,
    ),
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
        "getHost",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = uri_text(vm, receiver(args));
            match uri_authority(&text)
                .map(|authority| authority.rsplit('@').next().unwrap_or(authority))
            {
                Some(host) => {
                    let host = host.split(':').next().unwrap_or(host);
                    object(vm.intern(host)?)
                }
                None => object(ObjectRef::NULL),
            }
        },
    ),
    method("getPort", "()I", ACC_PUBLIC, |vm, _, args| {
        let text = uri_text(vm, receiver(args));
        let Some(authority) = uri_authority(&text) else {
            return int(-1);
        };
        let host = authority.rsplit('@').next().unwrap_or(authority);
        match host.rsplit_once(':') {
            Some((_, port)) if port.bytes().all(|byte| byte.is_ascii_digit()) => {
                int(port.parse().unwrap_or(-1))
            }
            _ => int(-1),
        }
    }),
    method(
        "getUserInfo",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = uri_text(vm, receiver(args));
            match uri_authority(&text).and_then(|authority| authority.split_once('@')) {
                Some((user, _)) => object(vm.intern(user)?),
                None => object(ObjectRef::NULL),
            }
        },
    ),
    method(
        "getAuthority",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = uri_text(vm, receiver(args));
            match uri_authority(&text) {
                Some(authority) => object(vm.intern(authority)?),
                None => object(ObjectRef::NULL),
            }
        },
    ),
    method(
        "getQuery",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = uri_text(vm, receiver(args));
            match uri_part(&text, '?', '#') {
                Some(query) => object(vm.intern(&percent_decode(&query))?),
                None => object(ObjectRef::NULL),
            }
        },
    ),
    method(
        "getFragment",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = uri_text(vm, receiver(args));
            match uri_part(&text, '#', '\0') {
                Some(fragment) => object(vm.intern(&percent_decode(&fragment))?),
                None => object(ObjectRef::NULL),
            }
        },
    ),
    method(
        "getSchemeSpecificPart",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = uri_text(vm, receiver(args));
            let rest = match text.find(':') {
                Some(index) => &text[index + 1..],
                None => text.as_str(),
            };
            let rest = rest.split('#').next().unwrap_or(rest);
            object(vm.intern(&percent_decode(rest))?)
        },
    ),
    method(
        "getRawSchemeSpecificPart",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = uri_text(vm, receiver(args));
            let rest = match text.find(':') {
                Some(index) => &text[index + 1..],
                None => text.as_str(),
            };
            let rest = rest.split('#').next().unwrap_or(rest);
            object(vm.intern(rest)?)
        },
    ),
    method("normalize", "()Ljava/net/URI;", ACC_PUBLIC, |_, _, args| {
        object(receiver(args))
    }),
    method("isOpaque", "()Z", ACC_PUBLIC, |vm, _, args| {
        let text = uri_text(vm, receiver(args));
        boolean(uri_authority(&text).is_none())
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
        "resolve",
        "(Ljava/net/URI;)Ljava/net/URI;",
        ACC_PUBLIC,
        |vm, _, args| {
            let base = uri_text(vm, receiver(args));
            let child = uri_text(vm, ref_arg(args, 1));
            let text = resolve_uri(&base, &child);
            new_uri(vm, &text)
        },
    ),
    method(
        "resolve",
        "(Ljava/lang/String;)Ljava/net/URI;",
        ACC_PUBLIC,
        |vm, _, args| {
            let base = uri_text(vm, receiver(args));
            let child = string_arg(vm, args[1])?;
            let text = resolve_uri(&base, &child);
            new_uri(vm, &text)
        },
    ),
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
