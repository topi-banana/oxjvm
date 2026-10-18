//! `java.util.ServiceLoader`: providers declared in `META-INF/services/<service>`.
//!
//! Loading is eager: `load` reads every `META-INF/services` resource named after the service
//! class and instantiates each provider, so `iterator`, `stream`, and `spliterator` all share
//! the same snapshot. `reload` rebuilds it.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::lang::abstract_method_pub;
use crate::{class, field, method, object, receiver, ref_arg, void};

const PROVIDER_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "type",
        "()Ljava/lang/Class;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "get",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
];

/// `java.util.ServiceLoader$Provider`: the interface `stream()` elements implement.
pub(crate) const SERVICE_LOADER_PROVIDER: oxjvm_vm::NativeClass = class(
    "java/util/ServiceLoader$Provider",
    Some("java/lang/Object"),
    &["java/util/function/Supplier"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &PROVIDER_METHODS,
    None,
);

const OJVM_PROVIDER_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method("type", "()Ljava/lang/Class;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        object(
            vm.read_ref_field(this, class, "type", "Ljava/lang/Class;")
                .unwrap_or(ObjectRef::NULL),
        )
    }),
    method("get", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        object(
            vm.read_ref_field(this, class, "value", "Ljava/lang/Object;")
                .unwrap_or(ObjectRef::NULL),
        )
    }),
];

/// The concrete provider `stream()` hands out: a class, an instance, and the
/// `ServiceLoader$Provider` interface.
pub(crate) const OJVM_PROVIDER: oxjvm_vm::NativeClass = class(
    "java/util/OjvmProvider",
    Some("java/lang/Object"),
    &["java/util/ServiceLoader$Provider"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[
        field("type", "Ljava/lang/Class;", ACC_PRIVATE, None),
        field("value", "Ljava/lang/Object;", ACC_PRIVATE, None),
    ],
    &OJVM_PROVIDER_METHODS,
    None,
);

fn new_provider(
    vm: &mut Vm<'_>,
    class_object: ObjectRef,
    value: ObjectRef,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/OjvmProvider")?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "type", "Ljava/lang/Class;")?;
    vm.set_instance_ref(instance, declaring, field, class_object);
    let (declaring, field) = vm.find_field(class, "value", "Ljava/lang/Object;")?;
    vm.set_instance_ref(instance, declaring, field, value);
    Ok(instance)
}

const SERVICE_LOADER_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method(
        "load",
        "(Ljava/lang/Class;)Ljava/util/ServiceLoader;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| service_loader_load(vm, ref_arg(args, 0)),
    ),
    method(
        "load",
        "(Ljava/lang/Class;Ljava/lang/ClassLoader;)Ljava/util/ServiceLoader;",
        ACC_PUBLIC | ACC_STATIC,
        // oxjvm has a single class path, so the loader argument carries no extra scope.
        |vm, _, args| service_loader_load(vm, ref_arg(args, 0)),
    ),
    method(
        "loadInstalled",
        "(Ljava/lang/Class;)Ljava/util/ServiceLoader;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| service_loader_load(vm, ref_arg(args, 0)),
    ),
    method(
        "iterator",
        "()Ljava/util/Iterator;",
        ACC_PUBLIC,
        service_loader_iterator,
    ),
    method(
        "spliterator",
        "()Ljava/util/Spliterator;",
        ACC_PUBLIC,
        service_loader_spliterator,
    ),
    method(
        "stream",
        "()Ljava/util/stream/Stream;",
        ACC_PUBLIC,
        service_loader_stream,
    ),
    method(
        "forEach",
        "(Ljava/util/function/Consumer;)V",
        ACC_PUBLIC,
        service_loader_for_each,
    ),
    method(
        "findFirst",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        service_loader_find_first,
    ),
    method("reload", "()V", ACC_PUBLIC, |vm, _, args| {
        reload(vm, receiver(args))?;
        void()
    }),
];

pub(crate) const SERVICE_LOADER: oxjvm_vm::NativeClass = class(
    "java/util/ServiceLoader",
    Some("java/lang/Object"),
    &["java/lang/Iterable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("service", "Ljava/lang/Class;", ACC_PRIVATE, None),
        field("instances", "Ljava/util/ArrayList;", ACC_PRIVATE, None),
        field("providers", "Ljava/util/ArrayList;", ACC_PRIVATE, None),
    ],
    &SERVICE_LOADER_METHODS,
    None,
);

/// `ServiceLoader.load(service)`: build the loader and snapshot its providers.
fn service_loader_load(vm: &mut Vm<'_>, service: ObjectRef) -> Result<Value, VmError> {
    if service.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let class = vm.resolve_class("java/util/ServiceLoader")?;
    let loader = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "service", "Ljava/lang/Class;")?;
    vm.set_instance_ref(loader, declaring, field, service);
    reload(vm, loader)?;
    object(loader)
}

/// Read the loader's service class name and rebuild both provider lists.
fn reload(vm: &mut Vm<'_>, loader: ObjectRef) -> Result<(), VmError> {
    let loader_class = vm.class_of(loader);
    let service = vm
        .read_ref_field(loader, loader_class, "service", "Ljava/lang/Class;")
        .unwrap_or(ObjectRef::NULL);
    let service_class = crate::lang::class_argument(vm, service)?;
    let binary = vm.class_name(service_class).replace('/', ".");
    let mut instances = Vec::new();
    let mut providers = Vec::new();
    for name in provider_names(vm, &binary) {
        let internal = name.replace('.', "/");
        let provider_class = match vm.resolve_class(&internal) {
            Ok(class) => class,
            Err(_) => {
                let message = alloc::format!("{name}: provider class not found");
                return Err(vm.throw_new("java/util/ServiceConfigurationError", Some(&message)));
            }
        };
        vm.ensure_initialized(provider_class)?;
        let instance = vm.new_instance(provider_class)?;
        let Some((declaring, ctor)) = vm.find_method(provider_class, "<init>", "()V") else {
            let message = alloc::format!("{name}: no no-argument constructor");
            return Err(vm.throw_new("java/util/ServiceConfigurationError", Some(&message)));
        };
        vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
        if !vm.is_instance(instance, service_class) {
            let message = alloc::format!("{name}: not a subtype of {binary}");
            return Err(vm.throw_new("java/util/ServiceConfigurationError", Some(&message)));
        }
        let class_object = vm.class_object(provider_class)?;
        providers.push(new_provider(vm, class_object, instance)?);
        instances.push(instance);
    }
    let instances = crate::util::array_list_from(vm, &instances)?;
    let providers = crate::util::array_list_from(vm, &providers)?;
    let (declaring, field) = vm.find_field(loader_class, "instances", "Ljava/util/ArrayList;")?;
    vm.set_instance_ref(loader, declaring, field, instances);
    let (declaring, field) = vm.find_field(loader_class, "providers", "Ljava/util/ArrayList;")?;
    vm.set_instance_ref(loader, declaring, field, providers);
    Ok(())
}

/// The provider class names listed in every `META-INF/services/<service>` resource.
fn provider_names(vm: &mut Vm<'_>, service: &str) -> Vec<String> {
    let resource = alloc::format!("META-INF/services/{service}");
    let mut names = Vec::new();
    for hit in vm.host.find_resources(&resource) {
        let text = String::from_utf8_lossy(&hit.bytes);
        for line in text.lines() {
            let content = match line.find('#') {
                Some(index) => &line[..index],
                None => line,
            };
            let content = content.trim();
            if !content.is_empty() {
                names.push(String::from(content));
            }
        }
    }
    names
}

/// One of the two lists the loader snapshots into a private field.
fn loader_list(vm: &mut Vm<'_>, loader: ObjectRef, name: &str) -> ObjectRef {
    let class = vm.class_of(loader);
    vm.read_ref_field(loader, class, name, "Ljava/util/ArrayList;")
        .unwrap_or(ObjectRef::NULL)
}

fn service_loader_iterator(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let list = loader_list(vm, receiver(args), "instances");
    let (class, method) = vm.resolve_virtual_method(list, "iterator", "()Ljava/util/Iterator;")?;
    vm.invoke_method(class, method, vec![Value::Ref(list)])
}

fn service_loader_spliterator(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    // The JDK inherits `Iterable.spliterator`, which traverses the providers' instances.
    let list = loader_list(vm, receiver(args), "instances");
    let elements = crate::util::collection_elements(vm, list)?;
    object(crate::stream::spliterator_from(vm, &elements)?)
}

fn service_loader_stream(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let list = loader_list(vm, receiver(args), "providers");
    let elements = crate::util::collection_elements(vm, list)?;
    object(crate::stream::stream_from(vm, elements)?)
}

fn service_loader_for_each(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let list = loader_list(vm, receiver(args), "instances");
    let elements = crate::util::collection_elements(vm, list)?;
    let consumer = ref_arg(args, 1);
    for element in elements {
        let (class, method) =
            vm.resolve_virtual_method(consumer, "accept", "(Ljava/lang/Object;)V")?;
        vm.invoke_method(
            class,
            method,
            vec![Value::Ref(consumer), Value::Ref(element)],
        )?;
    }
    void()
}

fn service_loader_find_first(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let list = loader_list(vm, receiver(args), "instances");
    let elements = crate::util::collection_elements(vm, list)?;
    match elements.first() {
        Some(first) => crate::optional::optional_new(vm, *first),
        None => crate::optional::optional_empty_value(vm),
    }
}
