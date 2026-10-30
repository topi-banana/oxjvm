//! `java.net.http`: client/request/response builders and a curl-backed `send`.
//!
//! oxjvm has no sockets, so the synchronous client delegates an actual request to the host's
//! `curl` when one is available.  The builder objects themselves are fully modelled, which is
//! what most API-contract tests exercise.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::{boolean, class, field, int, long, method, object, receiver, ref_arg, void};

fn new_instance(vm: &mut Vm<'_>, class_name: &str) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class(class_name)?;
    let instance = vm.new_instance(class)?;
    if let Some((declaring, ctor)) = vm.find_method(class, "<init>", "()V") {
        vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
    }
    Ok(instance)
}

fn ref_field(vm: &Vm<'_>, this: ObjectRef, name: &str, descriptor: &str) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), name, descriptor)
        .unwrap_or(ObjectRef::NULL)
}

fn set_ref(
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

fn empty_optional(vm: &mut Vm<'_>) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/util/Optional")?;
    let (declaring, method) = vm
        .find_method(class, "empty", "()Ljava/util/Optional;")
        .ok_or_else(|| VmError::internal("Optional.empty"))?;
    vm.invoke_method(declaring, method, vec![])
}

fn optional_of(vm: &mut Vm<'_>, value: ObjectRef) -> Result<Value, VmError> {
    if value.is_null() {
        return empty_optional(vm);
    }
    let class = vm.resolve_class("java/util/Optional")?;
    let (declaring, method) = vm
        .find_method(
            class,
            "ofNullable",
            "(Ljava/lang/Object;)Ljava/util/Optional;",
        )
        .ok_or_else(|| VmError::internal("Optional.ofNullable"))?;
    vm.invoke_method(declaring, method, vec![Value::Ref(value)])
}

fn new_hash_map(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    new_instance(vm, "java/util/HashMap")
}

fn new_array_list(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    new_instance(vm, "java/util/ArrayList")
}

fn map_call(
    vm: &mut Vm<'_>,
    map: ObjectRef,
    name: &str,
    descriptor: &str,
    args: Vec<Value>,
) -> Result<Value, VmError> {
    let (class, method) = vm.resolve_virtual_method(map, name, descriptor)?;
    vm.invoke_method(class, method, args)
}

fn string_value(vm: &Vm<'_>, reference: ObjectRef) -> String {
    vm.string_value(reference).unwrap_or_default()
}

fn enum_constant_clinit(vm: &mut Vm<'_>, class_name: &str, names: &[&str]) -> Result<(), VmError> {
    let class = vm.resolve_class(class_name)?;
    let descriptor = alloc::format!("L{class_name};");
    for (ordinal, name) in names.iter().enumerate() {
        let instance = vm.new_instance(class)?;
        if let Some((declaring, field)) = vm.find_field_opt(class, "ordinal", "I") {
            vm.set_instance_int(instance, declaring, field, ordinal as i32);
        }
        if let Some((declaring, field)) = vm.find_field_opt(class, "name", "Ljava/lang/String;") {
            let text = vm.make_string(name)?;
            vm.set_instance_ref(instance, declaring, field, text);
        }
        vm.set_static_value(class, name, &descriptor, Value::Ref(instance));
    }
    Ok(())
}

fn enum_strings_clinit(vm: &mut Vm<'_>, class_name: &str, names: &[&str]) -> Result<(), VmError> {
    let class = vm.resolve_class(class_name)?;
    let descriptor = alloc::format!("L{class_name};");
    for (ordinal, name) in names.iter().enumerate() {
        let instance = vm.new_instance(class)?;
        let name_ref = vm.make_string(name)?;
        let (declaring, ctor) = vm
            .find_method(class, "<init>", "(Ljava/lang/String;I)V")
            .ok_or_else(|| VmError::internal("enum ctor"))?;
        vm.invoke_method(
            declaring,
            ctor,
            vec![
                Value::Ref(instance),
                Value::Ref(name_ref),
                Value::Int(ordinal as i32),
            ],
        )?;
        vm.set_static_value(class, name, &descriptor, Value::Ref(instance));
    }
    Ok(())
}

// -------------------------------------------------------------------------------------------
// HttpClient enums
// -------------------------------------------------------------------------------------------

const ENUM_STRING_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = vm
                .read_string_field_any(receiver(args), "name")
                .unwrap_or_default();
            object(vm.make_string(&name)?)
        },
    ),
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let name = vm
            .read_string_field_any(receiver(args), "name")
            .unwrap_or_default();
        object(vm.make_string(&name)?)
    }),
];

pub(crate) const HTTP_CLIENT_VERSION: oxjvm_vm::NativeClass = class(
    "java/net/http/HttpClient$Version",
    Some("java/lang/Enum"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_ENUM,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "HTTP_1_1",
            "Ljava/net/http/HttpClient$Version;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "HTTP_2",
            "Ljava/net/http/HttpClient$Version;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "HTTP_3",
            "Ljava/net/http/HttpClient$Version;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
    ],
    &ENUM_STRING_METHODS,
    Some(|vm| {
        enum_strings_clinit(
            vm,
            "java/net/http/HttpClient$Version",
            &["HTTP_1_1", "HTTP_2", "HTTP_3"],
        )
    }),
);

fn redirect_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    enum_constant_clinit(
        vm,
        "java/net/http/HttpClient$Redirect",
        &["NEVER", "ALWAYS", "NORMAL"],
    )
}

pub(crate) const HTTP_CLIENT_REDIRECT: oxjvm_vm::NativeClass = class(
    "java/net/http/HttpClient$Redirect",
    Some("java/lang/Enum"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_ENUM,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE | ACC_FINAL, None),
        field("ordinal", "I", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "NEVER",
            "Ljava/net/http/HttpClient$Redirect;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "ALWAYS",
            "Ljava/net/http/HttpClient$Redirect;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "NORMAL",
            "Ljava/net/http/HttpClient$Redirect;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
    ],
    &ENUM_STRING_METHODS,
    Some(redirect_clinit),
);

fn redirect_constant(vm: &mut Vm<'_>, name: &str) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/net/http/HttpClient$Redirect")?;
    vm.ensure_initialized(class)?;
    Ok(vm
        .read_static_ref(class, name, "Ljava/net/http/HttpClient$Redirect;")
        .unwrap_or(ObjectRef::NULL))
}

fn version_constant(vm: &mut Vm<'_>, name: &str) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/net/http/HttpClient$Version")?;
    vm.ensure_initialized(class)?;
    Ok(vm
        .read_static_ref(class, name, "Ljava/net/http/HttpClient$Version;")
        .unwrap_or(ObjectRef::NULL))
}

// -------------------------------------------------------------------------------------------
// HttpHeaders
// -------------------------------------------------------------------------------------------

const HTTP_HEADERS_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method("<init>", "(Ljava/util/Map;)V", ACC_PUBLIC, |vm, _, args| {
        set_ref(
            vm,
            receiver(args),
            "map",
            "Ljava/util/Map;",
            ref_arg(args, 1),
        )?;
        void()
    }),
    method(
        "of",
        "(Ljava/util/Map;Ljava/util/function/BiPredicate;)Ljava/net/http/HttpHeaders;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let headers = new_instance(vm, "java/net/http/HttpHeaders")?;
            set_ref(vm, headers, "map", "Ljava/util/Map;", ref_arg(args, 0))?;
            object(headers)
        },
    ),
    method(
        "firstValue",
        "(Ljava/lang/String;)Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = ref_field(vm, receiver(args), "map", "Ljava/util/Map;");
            let name = ref_arg(args, 1);
            let list = map_call(
                vm,
                map,
                "get",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                vec![Value::Ref(map), Value::Ref(name)],
            )?
            .as_ref();
            if list.is_null() {
                return empty_optional(vm);
            }
            let (cc, cm) = vm.resolve_virtual_method(list, "isEmpty", "()Z")?;
            if vm.invoke_method(cc, cm, vec![Value::Ref(list)])?.as_int() != 0 {
                return empty_optional(vm);
            }
            let (gc, gm) = vm.resolve_virtual_method(list, "get", "(I)Ljava/lang/Object;")?;
            let value = vm.invoke_method(gc, gm, vec![Value::Ref(list), Value::Int(0)])?;
            optional_of(vm, value.as_ref())
        },
    ),
    method(
        "allValues",
        "(Ljava/lang/String;)Ljava/util/List;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = ref_field(vm, receiver(args), "map", "Ljava/util/Map;");
            let name = ref_arg(args, 1);
            let list = map_call(
                vm,
                map,
                "get",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                vec![Value::Ref(map), Value::Ref(name)],
            )?
            .as_ref();
            if list.is_null() {
                object(new_array_list(vm)?)
            } else {
                object(list)
            }
        },
    ),
    method("map", "()Ljava/util/Map;", ACC_PUBLIC, |vm, _, args| {
        object(ref_field(vm, receiver(args), "map", "Ljava/util/Map;"))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let map = ref_field(vm, receiver(args), "map", "Ljava/util/Map;");
            let (class, method) =
                vm.resolve_virtual_method(map, "toString", "()Ljava/lang/String;")?;
            vm.invoke_method(class, method, vec![Value::Ref(map)])
        },
    ),
    method("equals", "(Ljava/lang/Object;)Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
    method("hashCode", "()I", ACC_PUBLIC, |_, _, _| int(0)),
];

pub(crate) const HTTP_HEADERS: oxjvm_vm::NativeClass = class(
    "java/net/http/HttpHeaders",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[field("map", "Ljava/util/Map;", ACC_PRIVATE, None)],
    &HTTP_HEADERS_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// HttpRequest and its builder
// -------------------------------------------------------------------------------------------

const HTTP_REQUEST_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method(
        "newBuilder",
        "(Ljava/net/URI;)Ljava/net/http/HttpRequest$Builder;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let class = vm.resolve_class("java/net/http/OjvmHttpRequestBuilder")?;
            let builder = vm.new_instance(class)?;
            if let Some((declaring, ctor)) = vm.find_method(class, "<init>", "()V") {
                vm.invoke_method(declaring, ctor, vec![Value::Ref(builder)])?;
            }
            set_ref(vm, builder, "uri", "Ljava/net/URI;", ref_arg(args, 0))?;
            object(builder)
        },
    ),
    method(
        "newBuilder",
        "()Ljava/net/http/HttpRequest$Builder;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_instance(vm, "java/net/http/OjvmHttpRequestBuilder")?),
    ),
    method(
        "newBuilder",
        "(Ljava/net/http/HttpRequest;Ljava/util/function/BiPredicate;)Ljava/net/http/HttpRequest$Builder;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _args| object(new_instance(vm, "java/net/http/OjvmHttpRequestBuilder")?),
    ),
    abstract_method("bodyPublisher", "()Ljava/util/Optional;"),
    abstract_method("method", "()Ljava/lang/String;"),
    abstract_method("timeout", "()Ljava/util/Optional;"),
    abstract_method("expectContinue", "()Z"),
    abstract_method("uri", "()Ljava/net/URI;"),
    abstract_method("version", "()Ljava/util/Optional;"),
    abstract_method("headers", "()Ljava/net/http/HttpHeaders;"),
];

pub(crate) const HTTP_REQUEST: oxjvm_vm::NativeClass = class(
    "java/net/http/HttpRequest",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &HTTP_REQUEST_METHODS,
    None,
);

const fn abstract_method(
    name: &'static str,
    descriptor: &'static str,
) -> oxjvm_vm::NativeMethodDef {
    method(
        name,
        descriptor,
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    )
}

const OJVM_HTTP_REQUEST_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "bodyPublisher",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, args| {
            let body = ref_field(
                vm,
                receiver(args),
                "bodyPublisher",
                "Ljava/net/http/HttpRequest$BodyPublisher;",
            );
            optional_of(vm, body)
        },
    ),
    method(
        "method",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "method",
                "Ljava/lang/String;",
            ))
        },
    ),
    method(
        "timeout",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, args| {
            let timeout = ref_field(vm, receiver(args), "timeout", "Ljava/time/Duration;");
            optional_of(vm, timeout)
        },
    ),
    method("expectContinue", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        boolean(
            vm.read_instance_int_named(this, class, "expectContinue", "Z")
                .unwrap_or(0)
                != 0,
        )
    }),
    method("uri", "()Ljava/net/URI;", ACC_PUBLIC, |vm, _, args| {
        object(ref_field(vm, receiver(args), "uri", "Ljava/net/URI;"))
    }),
    method(
        "version",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, args| {
            let version = ref_field(
                vm,
                receiver(args),
                "version",
                "Ljava/net/http/HttpClient$Version;",
            );
            optional_of(vm, version)
        },
    ),
    method(
        "headers",
        "()Ljava/net/http/HttpHeaders;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "headers",
                "Ljava/net/http/HttpHeaders;",
            ))
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let method = string_value(vm, ref_field(vm, this, "method", "Ljava/lang/String;"));
            let uri = string_value(vm, ref_field(vm, this, "uri", "Ljava/net/URI;"));
            object(vm.make_string(&alloc::format!("{method} {uri}"))?)
        },
    ),
];

pub(crate) const OJVM_HTTP_REQUEST: oxjvm_vm::NativeClass = class(
    "java/net/http/OjvmHttpRequest",
    Some("java/net/http/HttpRequest"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[
        field("uri", "Ljava/net/URI;", ACC_PRIVATE, None),
        field("method", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("headers", "Ljava/net/http/HttpHeaders;", ACC_PRIVATE, None),
        field("timeout", "Ljava/time/Duration;", ACC_PRIVATE, None),
        field(
            "version",
            "Ljava/net/http/HttpClient$Version;",
            ACC_PRIVATE,
            None,
        ),
        field(
            "bodyPublisher",
            "Ljava/net/http/HttpRequest$BodyPublisher;",
            ACC_PRIVATE,
            None,
        ),
        field("expectContinue", "Z", ACC_PRIVATE, None),
    ],
    &OJVM_HTTP_REQUEST_METHODS,
    None,
);

fn request_builder_set_method(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    method_name: &str,
    body: ObjectRef,
) -> Result<(), VmError> {
    let name = vm.make_string(method_name)?;
    set_ref(vm, this, "method", "Ljava/lang/String;", name)?;
    set_ref(
        vm,
        this,
        "bodyPublisher",
        "Ljava/net/http/HttpRequest$BodyPublisher;",
        body,
    )?;
    Ok(())
}

const HTTP_REQUEST_BUILDER_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
    abstract_method("uri", "(Ljava/net/URI;)Ljava/net/http/HttpRequest$Builder;"),
    abstract_method("expectContinue", "(Z)Ljava/net/http/HttpRequest$Builder;"),
    abstract_method(
        "version",
        "(Ljava/net/http/HttpClient$Version;)Ljava/net/http/HttpRequest$Builder;",
    ),
    abstract_method(
        "header",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/net/http/HttpRequest$Builder;",
    ),
    abstract_method(
        "headers",
        "([Ljava/lang/String;)Ljava/net/http/HttpRequest$Builder;",
    ),
    abstract_method(
        "timeout",
        "(Ljava/time/Duration;)Ljava/net/http/HttpRequest$Builder;",
    ),
    abstract_method(
        "setHeader",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/net/http/HttpRequest$Builder;",
    ),
    abstract_method("GET", "()Ljava/net/http/HttpRequest$Builder;"),
    abstract_method(
        "POST",
        "(Ljava/net/http/HttpRequest$BodyPublisher;)Ljava/net/http/HttpRequest$Builder;",
    ),
    abstract_method(
        "method",
        "(Ljava/lang/String;Ljava/net/http/HttpRequest$BodyPublisher;)Ljava/net/http/HttpRequest$Builder;",
    ),
    abstract_method("build", "()Ljava/net/http/HttpRequest;"),
];

pub(crate) const HTTP_REQUEST_BUILDER: oxjvm_vm::NativeClass = class(
    "java/net/http/HttpRequest$Builder",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &HTTP_REQUEST_BUILDER_METHODS,
    None,
);

fn new_empty_headers(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let map = new_hash_map(vm)?;
    let headers = new_instance(vm, "java/net/http/HttpHeaders")?;
    set_ref(vm, headers, "map", "Ljava/util/Map;", map)?;
    Ok(headers)
}

fn header_add(
    vm: &mut Vm<'_>,
    headers: ObjectRef,
    name: ObjectRef,
    value: ObjectRef,
) -> Result<(), VmError> {
    let map = ref_field(vm, headers, "map", "Ljava/util/Map;");
    let existing = map_call(
        vm,
        map,
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        vec![Value::Ref(map), Value::Ref(name)],
    )?
    .as_ref();
    let list = if existing.is_null() {
        let list = new_array_list(vm)?;
        map_call(
            vm,
            map,
            "put",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            vec![Value::Ref(map), Value::Ref(name), Value::Ref(list)],
        )?;
        list
    } else {
        existing
    };
    let (class, method) = vm.resolve_virtual_method(list, "add", "(Ljava/lang/Object;)Z")?;
    vm.invoke_method(class, method, vec![Value::Ref(list), Value::Ref(value)])?;
    Ok(())
}

const OJVM_HTTP_REQUEST_BUILDER_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let headers = new_empty_headers(vm)?;
        set_ref(vm, this, "headers", "Ljava/net/http/HttpHeaders;", headers)?;
        let get = vm.make_string("GET")?;
        set_ref(vm, this, "method", "Ljava/lang/String;", get)?;
        let never = redirect_constant(vm, "NEVER")?;
        set_ref(
            vm,
            this,
            "redirect",
            "Ljava/net/http/HttpClient$Redirect;",
            never,
        )?;
        void()
    }),
    method(
        "uri",
        "(Ljava/net/URI;)Ljava/net/http/HttpRequest$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            set_ref(vm, this, "uri", "Ljava/net/URI;", ref_arg(args, 1))?;
            object(this)
        },
    ),
    method(
        "expectContinue",
        "(Z)Ljava/net/http/HttpRequest$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "expectContinue", "Z")?;
            vm.set_instance_int(this, declaring, field, crate::bool_arg(args, 1) as i32);
            object(this)
        },
    ),
    method(
        "version",
        "(Ljava/net/http/HttpClient$Version;)Ljava/net/http/HttpRequest$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            set_ref(
                vm,
                this,
                "version",
                "Ljava/net/http/HttpClient$Version;",
                ref_arg(args, 1),
            )?;
            object(this)
        },
    ),
    method(
        "header",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/net/http/HttpRequest$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let headers = ref_field(vm, this, "headers", "Ljava/net/http/HttpHeaders;");
            header_add(vm, headers, ref_arg(args, 1), ref_arg(args, 2))?;
            object(this)
        },
    ),
    method(
        "headers",
        "([Ljava/lang/String;)Ljava/net/http/HttpRequest$Builder;",
        ACC_PUBLIC | ACC_VARARGS,
        |vm, _, args| {
            let this = receiver(args);
            let headers = ref_field(vm, this, "headers", "Ljava/net/http/HttpHeaders;");
            let values = ref_arg(args, 1);
            if !values.is_null() {
                let length = vm.array_length(values)?;
                let mut index = 0;
                while index + 1 < length {
                    let name = vm.array_get(values, index as i32)?.as_ref();
                    let value = vm.array_get(values, (index + 1) as i32)?.as_ref();
                    header_add(vm, headers, name, value)?;
                    index += 2;
                }
            }
            object(this)
        },
    ),
    method(
        "timeout",
        "(Ljava/time/Duration;)Ljava/net/http/HttpRequest$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            set_ref(
                vm,
                this,
                "timeout",
                "Ljava/time/Duration;",
                ref_arg(args, 1),
            )?;
            object(this)
        },
    ),
    method(
        "setHeader",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/net/http/HttpRequest$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let headers = ref_field(vm, this, "headers", "Ljava/net/http/HttpHeaders;");
            header_add(vm, headers, ref_arg(args, 1), ref_arg(args, 2))?;
            object(this)
        },
    ),
    method(
        "GET",
        "()Ljava/net/http/HttpRequest$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            request_builder_set_method(vm, receiver(args), "GET", ObjectRef::NULL)?;
            object(receiver(args))
        },
    ),
    method(
        "POST",
        "(Ljava/net/http/HttpRequest$BodyPublisher;)Ljava/net/http/HttpRequest$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            request_builder_set_method(vm, receiver(args), "POST", ref_arg(args, 1))?;
            object(receiver(args))
        },
    ),
    method(
        "method",
        "(Ljava/lang/String;Ljava/net/http/HttpRequest$BodyPublisher;)Ljava/net/http/HttpRequest$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name = ref_arg(args, 1);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let method_name = vm.string_or_empty(name);
            request_builder_set_method(vm, this, &method_name, ref_arg(args, 2))?;
            object(this)
        },
    ),
    method(
        "build",
        "()Ljava/net/http/HttpRequest;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let request = new_instance(vm, "java/net/http/OjvmHttpRequest")?;
            for (name, descriptor) in [
                ("uri", "Ljava/net/URI;"),
                ("method", "Ljava/lang/String;"),
                ("headers", "Ljava/net/http/HttpHeaders;"),
                ("timeout", "Ljava/time/Duration;"),
                ("version", "Ljava/net/http/HttpClient$Version;"),
                ("bodyPublisher", "Ljava/net/http/HttpRequest$BodyPublisher;"),
            ] {
                let value = ref_field(vm, this, name, descriptor);
                if !value.is_null() {
                    set_ref(vm, request, name, descriptor, value)?;
                }
            }
            let expect = vm
                .read_instance_int_named(this, vm.class_of(this), "expectContinue", "Z")
                .unwrap_or(0);
            let class = vm.class_of(request);
            let (declaring, field) = vm.find_field(class, "expectContinue", "Z")?;
            vm.set_instance_int(request, declaring, field, expect);
            let uri = ref_field(vm, request, "uri", "Ljava/net/URI;");
            if uri.is_null() {
                return Err(vm.throw_new("java/lang/IllegalStateException", Some("URI not set")));
            }
            object(request)
        },
    ),
];

pub(crate) const OJVM_HTTP_REQUEST_BUILDER: oxjvm_vm::NativeClass = class(
    "java/net/http/OjvmHttpRequestBuilder",
    Some("java/lang/Object"),
    &["java/net/http/HttpRequest$Builder"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[
        field("uri", "Ljava/net/URI;", ACC_PRIVATE, None),
        field("method", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("headers", "Ljava/net/http/HttpHeaders;", ACC_PRIVATE, None),
        field("timeout", "Ljava/time/Duration;", ACC_PRIVATE, None),
        field(
            "version",
            "Ljava/net/http/HttpClient$Version;",
            ACC_PRIVATE,
            None,
        ),
        field(
            "bodyPublisher",
            "Ljava/net/http/HttpRequest$BodyPublisher;",
            ACC_PRIVATE,
            None,
        ),
        field("expectContinue", "Z", ACC_PRIVATE, None),
        field(
            "redirect",
            "Ljava/net/http/HttpClient$Redirect;",
            ACC_PRIVATE,
            None,
        ),
    ],
    &OJVM_HTTP_REQUEST_BUILDER_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Body publishers / handlers
// -------------------------------------------------------------------------------------------

pub(crate) const BODY_PUBLISHER: oxjvm_vm::NativeClass = class(
    "java/net/http/HttpRequest$BodyPublisher",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[
        abstract_method("contentLength", "()J"),
        abstract_method("subscribe", "(Ljava/util/concurrent/Flow$Subscriber;)V"),
    ],
    None,
);

const OJVM_BODY_PUBLISHER_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("contentLength", "()J", ACC_PUBLIC, |vm, _, args| {
        let data = ref_field(vm, receiver(args), "data", "[B");
        if data.is_null() {
            long(0)
        } else {
            long(vm.array_length(data)? as i64)
        }
    }),
    method(
        "subscribe",
        "(Ljava/util/concurrent/Flow$Subscriber;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
];

pub(crate) const OJVM_BODY_PUBLISHER: oxjvm_vm::NativeClass = class(
    "java/net/http/OjvmBodyPublisher",
    Some("java/lang/Object"),
    &["java/net/http/HttpRequest$BodyPublisher"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field("data", "[B", ACC_PRIVATE, None)],
    &OJVM_BODY_PUBLISHER_METHODS,
    None,
);

fn new_byte_array(vm: &mut Vm<'_>, bytes: &[u8]) -> Result<ObjectRef, VmError> {
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, bytes.len())?;
    for (index, byte) in bytes.iter().enumerate() {
        vm.array_set(array, index as i32, Value::Int(i32::from(*byte)), false)?;
    }
    Ok(array)
}

fn publisher_from_bytes(vm: &mut Vm<'_>, bytes: &[u8]) -> Result<Value, VmError> {
    let publisher = new_instance(vm, "java/net/http/OjvmBodyPublisher")?;
    let data = new_byte_array(vm, bytes)?;
    set_ref(vm, publisher, "data", "[B", data)?;
    object(publisher)
}

const BODY_PUBLISHERS_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "noBody",
        "()Ljava/net/http/HttpRequest$BodyPublisher;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| publisher_from_bytes(vm, &[]),
    ),
    method(
        "ofString",
        "(Ljava/lang/String;)Ljava/net/http/HttpRequest$BodyPublisher;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 0));
            publisher_from_bytes(vm, text.as_bytes())
        },
    ),
    method(
        "ofByteArray",
        "([B)Ljava/net/http/HttpRequest$BodyPublisher;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let array = ref_arg(args, 0);
            if array.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let length = vm.array_length(array)?;
            let mut bytes = Vec::with_capacity(length);
            for index in 0..length {
                bytes.push(vm.array_get(array, index as i32)?.as_int() as u8);
            }
            publisher_from_bytes(vm, &bytes)
        },
    ),
    method(
        "ofInputStream",
        "(Ljava/util/function/Supplier;)Ljava/net/http/HttpRequest$BodyPublisher;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| publisher_from_bytes(vm, &[]),
    ),
    method(
        "ofFile",
        "(Ljava/nio/file/Path;)Ljava/net/http/HttpRequest$BodyPublisher;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| publisher_from_bytes(vm, &[]),
    ),
    method(
        "fromPublisher",
        "(Ljava/util/concurrent/Flow$Publisher;)Ljava/net/http/HttpRequest$BodyPublisher;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| publisher_from_bytes(vm, &[]),
    ),
];

pub(crate) const BODY_PUBLISHERS: oxjvm_vm::NativeClass = class(
    "java/net/http/HttpRequest$BodyPublishers",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &BODY_PUBLISHERS_METHODS,
    None,
);

pub(crate) const BODY_HANDLER: oxjvm_vm::NativeClass = class(
    "java/net/http/HttpResponse$BodyHandler",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[abstract_method(
        "apply",
        "(Ljava/net/http/HttpResponse$ResponseInfo;)Ljava/net/http/HttpResponse$BodySubscriber;",
    )],
    None,
);

pub(crate) const BODY_SUBSCRIBER: oxjvm_vm::NativeClass = class(
    "java/net/http/HttpResponse$BodySubscriber",
    Some("java/lang/Object"),
    &["java/util/concurrent/Flow$Subscriber"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const RESPONSE_INFO: oxjvm_vm::NativeClass = class(
    "java/net/http/HttpResponse$ResponseInfo",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

const OJVM_BODY_HANDLER_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            set_ref(
                vm,
                receiver(args),
                "kind",
                "Ljava/lang/String;",
                ref_arg(args, 1),
            )?;
            void()
        },
    ),
    method(
        "apply",
        "(Ljava/net/http/HttpResponse$ResponseInfo;)Ljava/net/http/HttpResponse$BodySubscriber;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
];

pub(crate) const OJVM_BODY_HANDLER: oxjvm_vm::NativeClass = class(
    "java/net/http/OjvmBodyHandler",
    Some("java/lang/Object"),
    &["java/net/http/HttpResponse$BodyHandler"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field("kind", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &OJVM_BODY_HANDLER_METHODS,
    None,
);

fn body_handler(vm: &mut Vm<'_>, kind: &str) -> Result<Value, VmError> {
    let handler = new_instance(vm, "java/net/http/OjvmBodyHandler")?;
    let text = vm.make_string(kind)?;
    set_ref(vm, handler, "kind", "Ljava/lang/String;", text)?;
    object(handler)
}

const BODY_HANDLERS_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method(
        "ofString",
        "()Ljava/net/http/HttpResponse$BodyHandler;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| body_handler(vm, "string"),
    ),
    method(
        "ofString",
        "(Ljava/nio/charset/Charset;)Ljava/net/http/HttpResponse$BodyHandler;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| body_handler(vm, "string"),
    ),
    method(
        "ofByteArray",
        "()Ljava/net/http/HttpResponse$BodyHandler;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| body_handler(vm, "bytes"),
    ),
    method(
        "discarding",
        "()Ljava/net/http/HttpResponse$BodyHandler;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| body_handler(vm, "discard"),
    ),
    method(
        "ofInputStream",
        "()Ljava/net/http/HttpResponse$BodyHandler;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| body_handler(vm, "stream"),
    ),
    method(
        "ofLines",
        "()Ljava/net/http/HttpResponse$BodyHandler;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| body_handler(vm, "lines"),
    ),
    method(
        "ofFile",
        "(Ljava/nio/file/Path;)Ljava/net/http/HttpResponse$BodyHandler;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| body_handler(vm, "file"),
    ),
    method(
        "ofFile",
        "(Ljava/nio/file/Path;Ljava/nio/file/OpenOption;)Ljava/net/http/HttpResponse$BodyHandler;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| body_handler(vm, "file"),
    ),
    method(
        "ofByteArrayConsumer",
        "(Ljava/util/function/Consumer;)Ljava/net/http/HttpResponse$BodyHandler;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| body_handler(vm, "bytes"),
    ),
];

pub(crate) const BODY_HANDLERS: oxjvm_vm::NativeClass = class(
    "java/net/http/HttpResponse$BodyHandlers",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &BODY_HANDLERS_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// HttpResponse
// -------------------------------------------------------------------------------------------

const HTTP_RESPONSE_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    abstract_method("statusCode", "()I"),
    abstract_method("request", "()Ljava/net/http/HttpRequest;"),
    abstract_method("previousResponse", "()Ljava/util/Optional;"),
    abstract_method("headers", "()Ljava/net/http/HttpHeaders;"),
    abstract_method("body", "()Ljava/lang/Object;"),
    abstract_method("sslSession", "()Ljava/util/Optional;"),
    abstract_method("uri", "()Ljava/net/URI;"),
    abstract_method("version", "()Ljava/net/http/HttpClient$Version;"),
];

pub(crate) const HTTP_RESPONSE: oxjvm_vm::NativeClass = class(
    "java/net/http/HttpResponse",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &HTTP_RESPONSE_METHODS,
    None,
);

const OJVM_HTTP_RESPONSE_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("statusCode", "()I", ACC_PUBLIC, |vm, _, args| {
        int(vm
            .read_instance_int_named(receiver(args), vm.class_of(receiver(args)), "status", "I")
            .unwrap_or(0))
    }),
    method(
        "request",
        "()Ljava/net/http/HttpRequest;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "request",
                "Ljava/net/http/HttpRequest;",
            ))
        },
    ),
    method(
        "previousResponse",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, _| empty_optional(vm),
    ),
    method(
        "headers",
        "()Ljava/net/http/HttpHeaders;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "headers",
                "Ljava/net/http/HttpHeaders;",
            ))
        },
    ),
    method("body", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        object(ref_field(vm, receiver(args), "body", "Ljava/lang/Object;"))
    }),
    method(
        "sslSession",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, _| empty_optional(vm),
    ),
    method("uri", "()Ljava/net/URI;", ACC_PUBLIC, |vm, _, args| {
        object(ref_field(vm, receiver(args), "uri", "Ljava/net/URI;"))
    }),
    method(
        "version",
        "()Ljava/net/http/HttpClient$Version;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "version",
                "Ljava/net/http/HttpClient$Version;",
            ))
        },
    ),
    method(
        "trailers",
        "()Ljava/net/http/HttpHeaders;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "headers",
                "Ljava/net/http/HttpHeaders;",
            ))
        },
    ),
];

pub(crate) const OJVM_HTTP_RESPONSE: oxjvm_vm::NativeClass = class(
    "java/net/http/OjvmHttpResponse",
    Some("java/lang/Object"),
    &["java/net/http/HttpResponse"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[
        field("status", "I", ACC_PRIVATE, None),
        field("body", "Ljava/lang/Object;", ACC_PRIVATE, None),
        field("headers", "Ljava/net/http/HttpHeaders;", ACC_PRIVATE, None),
        field("request", "Ljava/net/http/HttpRequest;", ACC_PRIVATE, None),
        field("uri", "Ljava/net/URI;", ACC_PRIVATE, None),
        field(
            "version",
            "Ljava/net/http/HttpClient$Version;",
            ACC_PRIVATE,
            None,
        ),
    ],
    &OJVM_HTTP_RESPONSE_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// HttpClient
// -------------------------------------------------------------------------------------------

const HTTP_CLIENT_METHODS: [oxjvm_vm::NativeMethodDef; 13] = [
    method(
        "newHttpClient",
        "()Ljava/net/http/HttpClient;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_instance(vm, "java/net/http/OjvmHttpClient")?),
    ),
    method(
        "newBuilder",
        "()Ljava/net/http/HttpClient$Builder;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_instance(vm, "java/net/http/OjvmHttpClientBuilder")?),
    ),
    abstract_method("cookieHandler", "()Ljava/util/Optional;"),
    abstract_method("connectTimeout", "()Ljava/util/Optional;"),
    abstract_method("followRedirects", "()Ljava/net/http/HttpClient$Redirect;"),
    abstract_method("proxy", "()Ljava/util/Optional;"),
    abstract_method("sslContext", "()Ljavax/net/ssl/SSLContext;"),
    abstract_method("sslParameters", "()Ljavax/net/ssl/SSLParameters;"),
    abstract_method("authenticator", "()Ljava/util/Optional;"),
    abstract_method("version", "()Ljava/net/http/HttpClient$Version;"),
    abstract_method("executor", "()Ljava/util/Optional;"),
    abstract_method(
        "send",
        "(Ljava/net/http/HttpRequest;Ljava/net/http/HttpResponse$BodyHandler;)Ljava/net/http/HttpResponse;",
    ),
    abstract_method(
        "sendAsync",
        "(Ljava/net/http/HttpRequest;Ljava/net/http/HttpResponse$BodyHandler;)Ljava/util/concurrent/CompletableFuture;",
    ),
];

pub(crate) const HTTP_CLIENT: oxjvm_vm::NativeClass = class(
    "java/net/http/HttpClient",
    Some("java/lang/Object"),
    &["java/lang/AutoCloseable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &HTTP_CLIENT_METHODS,
    None,
);

fn send_request(
    vm: &mut Vm<'_>,
    client: ObjectRef,
    request: ObjectRef,
    handler: ObjectRef,
) -> Result<ObjectRef, VmError> {
    if request.is_null() || handler.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let uri = ref_field(vm, request, "uri", "Ljava/net/URI;");
    let uri_text = {
        let (class, method) = vm.resolve_virtual_method(uri, "toString", "()Ljava/lang/String;")?;
        let value = vm.invoke_method(class, method, vec![Value::Ref(uri)])?;
        vm.string_or_empty(value.as_ref())
    };
    let kind = {
        let handler_class = vm.class_of(handler);
        vm.read_string_field(handler, handler_class, "kind")
            .unwrap_or_default()
    };
    let body_path = alloc::format!("/tmp/opencode/oxjvm-http-{}", vm.host.nano_time());
    let cmd = [
        String::from("curl"),
        String::from("-sS"),
        String::from("-L"),
        String::from("-o"),
        body_path.clone(),
        String::from("-w"),
        String::from("%{http_code}"),
        uri_text.clone(),
    ];
    let (status, body_bytes) = match vm.host.run_process(&cmd[0], &cmd[1..], None) {
        Ok(result) => {
            let code = String::from_utf8_lossy(&result.stdout)
                .trim()
                .parse::<i32>()
                .unwrap_or(-1);
            let bytes = vm
                .host
                .filesystem()
                .and_then(|fs| fs.read(&body_path))
                .unwrap_or_default();
            if code <= 0 {
                let message = String::from_utf8_lossy(&result.stderr).to_string();
                return Err(vm.throw_new(
                    "java/io/IOException",
                    Some(if message.is_empty() {
                        "request failed"
                    } else {
                        &message
                    }),
                ));
            }
            (code, bytes)
        }
        Err(error) => {
            return Err(vm.throw_new(
                "java/io/IOException",
                Some(&alloc::format!("request failed: {error}")),
            ));
        }
    };
    let _ = client;

    let body = match kind.as_str() {
        "bytes" => Value::Ref(new_byte_array(vm, &body_bytes)?),
        "discard" | "file" | "stream" => Value::Ref(ObjectRef::NULL),
        _ => {
            let text = String::from_utf8_lossy(&body_bytes).to_string();
            Value::Ref(vm.make_string(&text)?)
        }
    };
    let response = new_instance(vm, "java/net/http/OjvmHttpResponse")?;
    let class = vm.class_of(response);
    let (declaring, field) = vm.find_field(class, "status", "I")?;
    vm.set_instance_int(response, declaring, field, status);
    set_ref(vm, response, "body", "Ljava/lang/Object;", body.as_ref())?;
    set_ref(
        vm,
        response,
        "request",
        "Ljava/net/http/HttpRequest;",
        request,
    )?;
    set_ref(vm, response, "uri", "Ljava/net/URI;", uri)?;
    let headers = new_empty_headers(vm)?;
    set_ref(
        vm,
        response,
        "headers",
        "Ljava/net/http/HttpHeaders;",
        headers,
    )?;
    let version = version_constant(vm, "HTTP_1_1")?;
    set_ref(
        vm,
        response,
        "version",
        "Ljava/net/http/HttpClient$Version;",
        version,
    )?;
    Ok(response)
}

fn client_field_ref(vm: &Vm<'_>, this: ObjectRef, name: &str, descriptor: &str) -> ObjectRef {
    ref_field(vm, this, name, descriptor)
}

const OJVM_HTTP_CLIENT_METHODS: [oxjvm_vm::NativeMethodDef; 18] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "cookieHandler",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = client_field_ref(
                vm,
                receiver(args),
                "cookieHandler",
                "Ljava/net/CookieHandler;",
            );
            optional_of(vm, value)
        },
    ),
    method(
        "connectTimeout",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value =
                client_field_ref(vm, receiver(args), "connectTimeout", "Ljava/time/Duration;");
            optional_of(vm, value)
        },
    ),
    method(
        "followRedirects",
        "()Ljava/net/http/HttpClient$Redirect;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = client_field_ref(
                vm,
                receiver(args),
                "followRedirects",
                "Ljava/net/http/HttpClient$Redirect;",
            );
            if value.is_null() {
                object(redirect_constant(vm, "NEVER")?)
            } else {
                object(value)
            }
        },
    ),
    method(
        "proxy",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = client_field_ref(vm, receiver(args), "proxy", "Ljava/net/ProxySelector;");
            optional_of(vm, value)
        },
    ),
    method(
        "sslContext",
        "()Ljavax/net/ssl/SSLContext;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = client_field_ref(
                vm,
                receiver(args),
                "sslContext",
                "Ljavax/net/ssl/SSLContext;",
            );
            if !value.is_null() {
                return object(value);
            }
            let class = vm.resolve_class("javax/net/ssl/SSLContext")?;
            let (declaring, method) = vm
                .find_method(class, "getDefault", "()Ljavax/net/ssl/SSLContext;")
                .ok_or_else(|| VmError::internal("SSLContext.getDefault"))?;
            vm.invoke_method(declaring, method, vec![])
        },
    ),
    method(
        "sslParameters",
        "()Ljavax/net/ssl/SSLParameters;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = client_field_ref(
                vm,
                receiver(args),
                "sslParameters",
                "Ljavax/net/ssl/SSLParameters;",
            );
            if value.is_null() {
                object(new_instance(vm, "javax/net/ssl/SSLParameters")?)
            } else {
                object(value)
            }
        },
    ),
    method(
        "authenticator",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = client_field_ref(
                vm,
                receiver(args),
                "authenticator",
                "Ljava/net/Authenticator;",
            );
            optional_of(vm, value)
        },
    ),
    method(
        "version",
        "()Ljava/net/http/HttpClient$Version;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = client_field_ref(
                vm,
                receiver(args),
                "version",
                "Ljava/net/http/HttpClient$Version;",
            );
            if value.is_null() {
                object(version_constant(vm, "HTTP_1_1")?)
            } else {
                object(value)
            }
        },
    ),
    method(
        "executor",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = client_field_ref(
                vm,
                receiver(args),
                "executor",
                "Ljava/util/concurrent/Executor;",
            );
            optional_of(vm, value)
        },
    ),
    method(
        "send",
        "(Ljava/net/http/HttpRequest;Ljava/net/http/HttpResponse$BodyHandler;)Ljava/net/http/HttpResponse;",
        ACC_PUBLIC,
        |vm, _, args| {
            let response = send_request(vm, receiver(args), ref_arg(args, 1), ref_arg(args, 2))?;
            object(response)
        },
    ),
    method(
        "sendAsync",
        "(Ljava/net/http/HttpRequest;Ljava/net/http/HttpResponse$BodyHandler;)Ljava/util/concurrent/CompletableFuture;",
        ACC_PUBLIC,
        |vm, _, args| {
            let response = send_request(vm, receiver(args), ref_arg(args, 1), ref_arg(args, 2))?;
            let future_class = vm.resolve_class("java/util/concurrent/CompletableFuture")?;
            let (declaring, method) = vm
                .find_method(
                    future_class,
                    "completedFuture",
                    "(Ljava/lang/Object;)Ljava/util/concurrent/CompletableFuture;",
                )
                .ok_or_else(|| VmError::internal("completedFuture"))?;
            vm.invoke_method(declaring, method, vec![Value::Ref(response)])
        },
    ),
    method(
        "newBuilder",
        "()Ljava/net/http/HttpClient$Builder;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("shutdown", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("shutdownNow", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "awaitTermination",
        "(Ljava/time/Duration;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(true),
    ),
    method("isTerminated", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
];

pub(crate) const OJVM_HTTP_CLIENT: oxjvm_vm::NativeClass = class(
    "java/net/http/OjvmHttpClient",
    Some("java/net/http/HttpClient"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[
        field("connectTimeout", "Ljava/time/Duration;", ACC_PRIVATE, None),
        field(
            "followRedirects",
            "Ljava/net/http/HttpClient$Redirect;",
            ACC_PRIVATE,
            None,
        ),
        field("proxy", "Ljava/net/ProxySelector;", ACC_PRIVATE, None),
        field(
            "sslContext",
            "Ljavax/net/ssl/SSLContext;",
            ACC_PRIVATE,
            None,
        ),
        field(
            "sslParameters",
            "Ljavax/net/ssl/SSLParameters;",
            ACC_PRIVATE,
            None,
        ),
        field(
            "authenticator",
            "Ljava/net/Authenticator;",
            ACC_PRIVATE,
            None,
        ),
        field(
            "cookieHandler",
            "Ljava/net/CookieHandler;",
            ACC_PRIVATE,
            None,
        ),
        field(
            "version",
            "Ljava/net/http/HttpClient$Version;",
            ACC_PRIVATE,
            None,
        ),
        field(
            "executor",
            "Ljava/util/concurrent/Executor;",
            ACC_PRIVATE,
            None,
        ),
    ],
    &OJVM_HTTP_CLIENT_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// HttpClient$Builder
// -------------------------------------------------------------------------------------------

const HTTP_CLIENT_BUILDER_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
    abstract_method(
        "cookieHandler",
        "(Ljava/net/CookieHandler;)Ljava/net/http/HttpClient$Builder;",
    ),
    abstract_method(
        "connectTimeout",
        "(Ljava/time/Duration;)Ljava/net/http/HttpClient$Builder;",
    ),
    abstract_method(
        "followRedirects",
        "(Ljava/net/http/HttpClient$Redirect;)Ljava/net/http/HttpClient$Builder;",
    ),
    abstract_method(
        "proxy",
        "(Ljava/net/ProxySelector;)Ljava/net/http/HttpClient$Builder;",
    ),
    abstract_method(
        "sslContext",
        "(Ljavax/net/ssl/SSLContext;)Ljava/net/http/HttpClient$Builder;",
    ),
    abstract_method(
        "sslParameters",
        "(Ljavax/net/ssl/SSLParameters;)Ljava/net/http/HttpClient$Builder;",
    ),
    abstract_method(
        "authenticator",
        "(Ljava/net/Authenticator;)Ljava/net/http/HttpClient$Builder;",
    ),
    abstract_method(
        "version",
        "(Ljava/net/http/HttpClient$Version;)Ljava/net/http/HttpClient$Builder;",
    ),
    abstract_method(
        "executor",
        "(Ljava/util/concurrent/Executor;)Ljava/net/http/HttpClient$Builder;",
    ),
    abstract_method("build", "()Ljava/net/http/HttpClient;"),
    abstract_method(
        "localAddress",
        "(Ljava/net/InetAddress;)Ljava/net/http/HttpClient$Builder;",
    ),
    abstract_method("priority", "(I)Ljava/net/http/HttpClient$Builder;"),
    abstract_method(
        "proxy",
        "(Ljava/net/ProxySelector;)Ljava/net/http/HttpClient$Builder;",
    ),
    abstract_method("build", "()Ljava/net/http/HttpClient;"),
];

pub(crate) const HTTP_CLIENT_BUILDER: oxjvm_vm::NativeClass = class(
    "java/net/http/HttpClient$Builder",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &HTTP_CLIENT_BUILDER_METHODS,
    None,
);

fn builder_set(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: &str,
    descriptor: &str,
    value: ObjectRef,
) -> Result<Value, VmError> {
    set_ref(vm, this, name, descriptor, value)?;
    object(this)
}

const OJVM_HTTP_CLIENT_BUILDER_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "cookieHandler",
        "(Ljava/net/CookieHandler;)Ljava/net/http/HttpClient$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            builder_set(
                vm,
                receiver(args),
                "cookieHandler",
                "Ljava/net/CookieHandler;",
                ref_arg(args, 1),
            )
        },
    ),
    method(
        "connectTimeout",
        "(Ljava/time/Duration;)Ljava/net/http/HttpClient$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            builder_set(
                vm,
                receiver(args),
                "connectTimeout",
                "Ljava/time/Duration;",
                ref_arg(args, 1),
            )
        },
    ),
    method(
        "followRedirects",
        "(Ljava/net/http/HttpClient$Redirect;)Ljava/net/http/HttpClient$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            builder_set(
                vm,
                receiver(args),
                "followRedirects",
                "Ljava/net/http/HttpClient$Redirect;",
                ref_arg(args, 1),
            )
        },
    ),
    method(
        "proxy",
        "(Ljava/net/ProxySelector;)Ljava/net/http/HttpClient$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            builder_set(
                vm,
                receiver(args),
                "proxy",
                "Ljava/net/ProxySelector;",
                ref_arg(args, 1),
            )
        },
    ),
    method(
        "sslContext",
        "(Ljavax/net/ssl/SSLContext;)Ljava/net/http/HttpClient$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            builder_set(
                vm,
                receiver(args),
                "sslContext",
                "Ljavax/net/ssl/SSLContext;",
                ref_arg(args, 1),
            )
        },
    ),
    method(
        "authenticator",
        "(Ljava/net/Authenticator;)Ljava/net/http/HttpClient$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            builder_set(
                vm,
                receiver(args),
                "authenticator",
                "Ljava/net/Authenticator;",
                ref_arg(args, 1),
            )
        },
    ),
    method(
        "version",
        "(Ljava/net/http/HttpClient$Version;)Ljava/net/http/HttpClient$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            builder_set(
                vm,
                receiver(args),
                "version",
                "Ljava/net/http/HttpClient$Version;",
                ref_arg(args, 1),
            )
        },
    ),
    method(
        "executor",
        "(Ljava/util/concurrent/Executor;)Ljava/net/http/HttpClient$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            builder_set(
                vm,
                receiver(args),
                "executor",
                "Ljava/util/concurrent/Executor;",
                ref_arg(args, 1),
            )
        },
    ),
    method(
        "build",
        "()Ljava/net/http/HttpClient;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let client = new_instance(vm, "java/net/http/OjvmHttpClient")?;
            for (name, descriptor) in [
                ("connectTimeout", "Ljava/time/Duration;"),
                ("followRedirects", "Ljava/net/http/HttpClient$Redirect;"),
                ("proxy", "Ljava/net/ProxySelector;"),
                ("sslContext", "Ljavax/net/ssl/SSLContext;"),
                ("authenticator", "Ljava/net/Authenticator;"),
                ("cookieHandler", "Ljava/net/CookieHandler;"),
                ("version", "Ljava/net/http/HttpClient$Version;"),
                ("executor", "Ljava/util/concurrent/Executor;"),
            ] {
                let value = ref_field(vm, this, name, descriptor);
                if !value.is_null() {
                    set_ref(vm, client, name, descriptor, value)?;
                }
            }
            object(client)
        },
    ),
];

pub(crate) const OJVM_HTTP_CLIENT_BUILDER: oxjvm_vm::NativeClass = class(
    "java/net/http/OjvmHttpClientBuilder",
    Some("java/lang/Object"),
    &["java/net/http/HttpClient$Builder"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[
        field("connectTimeout", "Ljava/time/Duration;", ACC_PRIVATE, None),
        field(
            "followRedirects",
            "Ljava/net/http/HttpClient$Redirect;",
            ACC_PRIVATE,
            None,
        ),
        field("proxy", "Ljava/net/ProxySelector;", ACC_PRIVATE, None),
        field(
            "sslContext",
            "Ljavax/net/ssl/SSLContext;",
            ACC_PRIVATE,
            None,
        ),
        field(
            "authenticator",
            "Ljava/net/Authenticator;",
            ACC_PRIVATE,
            None,
        ),
        field(
            "cookieHandler",
            "Ljava/net/CookieHandler;",
            ACC_PRIVATE,
            None,
        ),
        field(
            "version",
            "Ljava/net/http/HttpClient$Version;",
            ACC_PRIVATE,
            None,
        ),
        field(
            "executor",
            "Ljava/util/concurrent/Executor;",
            ACC_PRIVATE,
            None,
        ),
    ],
    &OJVM_HTTP_CLIENT_BUILDER_METHODS,
    None,
);

pub(crate) const HTTP_TIMEOUT_EXCEPTION: oxjvm_vm::NativeClass = class(
    "java/net/http/HttpTimeoutException",
    Some("java/io/IOException"),
    &[],
    ACC_PUBLIC,
    &[],
    &[method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let message = ref_arg(args, 1);
            let exception = vm.resolve_class("java/io/IOException")?;
            let (declaring, ctor) = vm
                .find_method(exception, "<init>", "(Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("IOException ctor"))?;
            vm.invoke_method(
                declaring,
                ctor,
                vec![Value::Ref(receiver(args)), Value::Ref(message)],
            )?;
            void()
        },
    )],
    None,
);

pub(crate) const HTTP_CONNECT_TIMEOUT_EXCEPTION: oxjvm_vm::NativeClass = class(
    "java/net/http/HttpConnectTimeoutException",
    Some("java/net/http/HttpTimeoutException"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
// -------------------------------------------------------------------------------------------
// java.util.concurrent.Flow (the reactive-streams interfaces used by body publishers)
// -------------------------------------------------------------------------------------------

pub(crate) const FLOW: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/Flow",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &[],
    None,
);

pub(crate) const FLOW_PUBLISHER: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/Flow$Publisher",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[abstract_method(
        "subscribe",
        "(Ljava/util/concurrent/Flow$Subscriber;)V",
    )],
    None,
);

pub(crate) const FLOW_SUBSCRIBER: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/Flow$Subscriber",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const FLOW_SUBSCRIPTION: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/Flow$Subscription",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const FLOW_PROCESSOR: oxjvm_vm::NativeClass = class(
    "java/util/concurrent/Flow$Processor",
    Some("java/lang/Object"),
    &[
        "java/util/concurrent/Flow$Publisher",
        "java/util/concurrent/Flow$Subscriber",
    ],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);
