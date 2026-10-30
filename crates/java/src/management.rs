//! Tier 2.3: `java.lang.management.*`, `com.sun.management.*` and `javax.management`.
//!
//! The MXBeans report the state oxjvm actually has (thread count, heap usage, loaded classes);
//! values that have no meaning in a cooperative single-host-thread VM (CPU times, GC pauses)
//! report `0` or `-1` exactly as a JVM does when a counter is unsupported.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::{boolean, class, field, int, long, method, object, receiver, ref_arg, void};

// -------------------------------------------------------------------------------------------
// Helpers
// -------------------------------------------------------------------------------------------

fn new_instance(vm: &mut Vm<'_>, class_name: &str) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class(class_name)?;
    let instance = vm.new_instance(class)?;
    if let Some((declaring, ctor)) = vm.find_method(class, "<init>", "()V") {
        vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
    }
    Ok(instance)
}

fn new_hash_map(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/HashMap")?;
    let map = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(class, "<init>", "()V")
        .ok_or_else(|| VmError::internal("HashMap lacks <init>"))?;
    vm.invoke_method(declaring, ctor, vec![Value::Ref(map)])?;
    Ok(map)
}

fn new_array_list(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/ArrayList")?;
    let list = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(class, "<init>", "()V")
        .ok_or_else(|| VmError::internal("ArrayList lacks <init>"))?;
    vm.invoke_method(declaring, ctor, vec![Value::Ref(list)])?;
    Ok(list)
}

fn new_hash_set(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/HashSet")?;
    let set = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(class, "<init>", "()V")
        .ok_or_else(|| VmError::internal("HashSet lacks <init>"))?;
    vm.invoke_method(declaring, ctor, vec![Value::Ref(set)])?;
    Ok(set)
}

fn list_add(vm: &mut Vm<'_>, list: ObjectRef, value: ObjectRef) -> Result<(), VmError> {
    let (class, method) = vm.resolve_virtual_method(list, "add", "(Ljava/lang/Object;)Z")?;
    vm.invoke_method(class, method, vec![Value::Ref(list), Value::Ref(value)])?;
    Ok(())
}

fn set_add(vm: &mut Vm<'_>, set: ObjectRef, value: ObjectRef) -> Result<(), VmError> {
    let (class, method) = vm.resolve_virtual_method(set, "add", "(Ljava/lang/Object;)Z")?;
    vm.invoke_method(class, method, vec![Value::Ref(set), Value::Ref(value)])?;
    Ok(())
}

fn map_put(
    vm: &mut Vm<'_>,
    map: ObjectRef,
    key: ObjectRef,
    value: ObjectRef,
) -> Result<(), VmError> {
    let (class, method) = vm.resolve_virtual_method(
        map,
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
    )?;
    vm.invoke_method(
        class,
        method,
        vec![Value::Ref(map), Value::Ref(key), Value::Ref(value)],
    )?;
    Ok(())
}

fn map_get(vm: &mut Vm<'_>, map: ObjectRef, key: ObjectRef) -> Result<ObjectRef, VmError> {
    let (class, method) =
        vm.resolve_virtual_method(map, "get", "(Ljava/lang/Object;)Ljava/lang/Object;")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(map), Value::Ref(key)])?
        .as_ref())
}

fn map_remove(vm: &mut Vm<'_>, map: ObjectRef, key: ObjectRef) -> Result<(), VmError> {
    let (class, method) =
        vm.resolve_virtual_method(map, "remove", "(Ljava/lang/Object;)Ljava/lang/Object;")?;
    vm.invoke_method(class, method, vec![Value::Ref(map), Value::Ref(key)])?;
    Ok(())
}

fn map_size(vm: &mut Vm<'_>, map: ObjectRef) -> Result<i32, VmError> {
    let (class, method) = vm.resolve_virtual_method(map, "size", "()I")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(map)])?
        .as_int())
}

fn map_contains_key(vm: &mut Vm<'_>, map: ObjectRef, key: ObjectRef) -> Result<bool, VmError> {
    let (class, method) = vm.resolve_virtual_method(map, "containsKey", "(Ljava/lang/Object;)Z")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(map), Value::Ref(key)])?
        .as_int()
        != 0)
}

fn new_string_array(vm: &mut Vm<'_>, values: &[&str]) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/lang/String")?;
    let array = vm.allocate_object_array(class, values.len())?;
    for (index, value) in values.iter().enumerate() {
        let string = vm.make_string(value)?;
        vm.array_set_ref(array, index, string)?;
    }
    Ok(array)
}

fn new_long_array(vm: &mut Vm<'_>, values: &[i64]) -> Result<ObjectRef, VmError> {
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Long, values.len())?;
    for (index, value) in values.iter().enumerate() {
        vm.array_set(array, index as i32, Value::Long(*value), false)?;
    }
    Ok(array)
}

fn new_usage(
    vm: &mut Vm<'_>,
    init: i64,
    used: i64,
    committed: i64,
    max: i64,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/lang/management/MemoryUsage")?;
    let usage = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(class, "<init>", "(JJJJ)V")
        .ok_or_else(|| VmError::internal("MemoryUsage lacks <init>"))?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![
            Value::Ref(usage),
            Value::Long(init),
            Value::Long(used),
            Value::Long(committed),
            Value::Long(max),
        ],
    )?;
    Ok(usage)
}

fn system_property(vm: &mut Vm<'_>, name: &str) -> Result<ObjectRef, VmError> {
    let system = vm.resolve_class("java/lang/System")?;
    vm.ensure_initialized(system)?;
    let (class, method) = vm
        .find_method(
            system,
            "getProperty",
            "(Ljava/lang/String;)Ljava/lang/String;",
        )
        .ok_or_else(|| VmError::internal("System.getProperty"))?;
    let key = vm.make_string(name)?;
    let value = vm.invoke_method(class, method, vec![Value::Ref(key)])?;
    Ok(value.as_ref())
}

fn system_property_str(vm: &mut Vm<'_>, name: &str) -> String {
    system_property(vm, name)
        .ok()
        .map(|value| vm.string_or_empty(value))
        .unwrap_or_default()
}

fn bean_singleton(
    vm: &mut Vm<'_>,
    field_name: &str,
    descriptor: &str,
    impl_class: &str,
) -> Result<ObjectRef, VmError> {
    let factory = vm.resolve_class("java/lang/management/ManagementFactory")?;
    vm.ensure_initialized(factory)?;
    if let Some(bean) = vm.read_static_ref_if_set(factory, field_name, descriptor) {
        return Ok(bean);
    }
    let bean = new_instance(vm, impl_class)?;
    vm.set_static_value(factory, field_name, descriptor, Value::Ref(bean));
    Ok(bean)
}

fn long_field(vm: &Vm<'_>, this: ObjectRef, name: &str) -> i64 {
    let class = vm.class_of(this);
    vm.read_instance_long_named(this, class, name, "J")
        .unwrap_or(0)
}

fn set_long_field(vm: &mut Vm<'_>, this: ObjectRef, name: &str, value: i64) -> Result<(), VmError> {
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, name, "J")?;
    vm.set_instance_long(this, declaring, field, value);
    Ok(())
}

fn ref_field(vm: &Vm<'_>, this: ObjectRef, name: &str, descriptor: &str) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), name, descriptor)
        .unwrap_or(ObjectRef::NULL)
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

// -------------------------------------------------------------------------------------------
// java.lang.management.ManagementFactory
// -------------------------------------------------------------------------------------------

fn management_factory_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let factory = vm.resolve_class("java/lang/management/ManagementFactory")?;
    let now = vm.host_time_millis();
    vm.set_static_value(factory, "startTime", "J", Value::Long(now));
    let server = new_instance(vm, "javax/management/OjvmMBeanServer")?;
    vm.set_static_value(
        factory,
        "server",
        "Ljavax/management/MBeanServer;",
        Value::Ref(server),
    );
    Ok(())
}

fn platform_bean_for_class(vm: &mut Vm<'_>, class: ObjectRef) -> Result<ObjectRef, VmError> {
    if class.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    // Class.getName() for the requested interface.
    let (name_class, name_method) =
        vm.resolve_virtual_method(class, "getName", "()Ljava/lang/String;")?;
    let name = vm.invoke_method(name_class, name_method, vec![Value::Ref(class)])?;
    let name = vm.string_or_empty(name.as_ref());
    match name.as_str() {
        "java.lang.management.ThreadMXBean" | "com.sun.management.ThreadMXBean" => bean_singleton(
            vm,
            "threadBean",
            "Ljava/lang/management/ThreadMXBean;",
            "java/lang/management/OjvmThreadMXBean",
        ),
        "java.lang.management.MemoryMXBean" => bean_singleton(
            vm,
            "memoryBean",
            "Ljava/lang/management/MemoryMXBean;",
            "java/lang/management/OjvmMemoryMXBean",
        ),
        "java.lang.management.RuntimeMXBean" => bean_singleton(
            vm,
            "runtimeBean",
            "Ljava/lang/management/RuntimeMXBean;",
            "java/lang/management/OjvmRuntimeMXBean",
        ),
        "java.lang.management.OperatingSystemMXBean"
        | "com.sun.management.OperatingSystemMXBean" => bean_singleton(
            vm,
            "osBean",
            "Ljava/lang/management/OperatingSystemMXBean;",
            "java/lang/management/OjvmOperatingSystemMXBean",
        ),
        "java.lang.management.ClassLoadingMXBean" => bean_singleton(
            vm,
            "classLoadingBean",
            "Ljava/lang/management/ClassLoadingMXBean;",
            "java/lang/management/OjvmClassLoadingMXBean",
        ),
        "java.lang.management.CompilationMXBean" => bean_singleton(
            vm,
            "compilationBean",
            "Ljava/lang/management/CompilationMXBean;",
            "java/lang/management/OjvmCompilationMXBean",
        ),
        "com.sun.management.HotSpotDiagnosticMXBean" => bean_singleton(
            vm,
            "hotspotBean",
            "Lcom/sun/management/HotSpotDiagnosticMXBean;",
            "java/lang/management/OjvmHotSpotDiagnosticMXBean",
        ),
        "java.lang.management.GarbageCollectorMXBean"
        | "java.lang.management.MemoryPoolMXBean"
        | "java.lang.management.MemoryManagerMXBean" => Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("not a singleton MXBean"),
        )),
        _ => Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("not a platform MXBean interface"),
        )),
    }
}

fn register_platform_bean(
    vm: &mut Vm<'_>,
    server: ObjectRef,
    bean: ObjectRef,
    name: &str,
) -> Result<(), VmError> {
    let object_name = new_object_name(vm, name)?.as_ref();
    let (class, method) =
        vm.resolve_virtual_method(server, "isRegistered", "(Ljavax/management/ObjectName;)Z")?;
    if vm
        .invoke_method(
            class,
            method,
            vec![Value::Ref(server), Value::Ref(object_name)],
        )?
        .as_int()
        != 0
    {
        return Ok(());
    }
    if let Ok((class, method)) = vm.resolve_virtual_method(
        server,
        "registerMBean",
        "(Ljava/lang/Object;Ljavax/management/ObjectName;)Ljavax/management/ObjectInstance;",
    ) {
        let _ = vm.invoke_method(
            class,
            method,
            vec![
                Value::Ref(server),
                Value::Ref(bean),
                Value::Ref(object_name),
            ],
        );
    }
    Ok(())
}

fn platform_beans_for_class(vm: &mut Vm<'_>, class: ObjectRef) -> Result<ObjectRef, VmError> {
    if class.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let list = new_array_list(vm)?;
    let (nc, nm) = vm.resolve_virtual_method(class, "getName", "()Ljava/lang/String;")?;
    let value = vm.invoke_method(nc, nm, vec![Value::Ref(class)])?;
    let name = vm.string_or_empty(value.as_ref());
    match name.as_str() {
        "java.lang.management.GarbageCollectorMXBean" => {
            let bean = bean_singleton(
                vm,
                "garbageCollectorBean",
                "Ljava/lang/management/GarbageCollectorMXBean;",
                "java/lang/management/OjvmGarbageCollectorMXBean",
            )?;
            list_add(vm, list, bean)?;
        }
        "java.lang.management.MemoryPoolMXBean" => {
            let bean = bean_singleton(
                vm,
                "memoryPoolBean",
                "Ljava/lang/management/MemoryPoolMXBean;",
                "java/lang/management/OjvmMemoryPoolMXBean",
            )?;
            list_add(vm, list, bean)?;
        }
        "java.lang.management.MemoryManagerMXBean" => {
            let bean = bean_singleton(
                vm,
                "memoryManagerBean",
                "Ljava/lang/management/MemoryManagerMXBean;",
                "java/lang/management/OjvmMemoryManagerMXBean",
            )?;
            list_add(vm, list, bean)?;
        }
        _ => {
            let bean = platform_bean_for_class(vm, class)?;
            if !bean.is_null() {
                list_add(vm, list, bean)?;
            }
        }
    }
    Ok(list)
}

const MANAGEMENT_FACTORY_METHODS: [oxjvm_vm::NativeMethodDef; 17] = [
    method(
        "getClassLoadingMXBean",
        "()Ljava/lang/management/ClassLoadingMXBean;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            object(bean_singleton(
                vm,
                "classLoadingBean",
                "Ljava/lang/management/ClassLoadingMXBean;",
                "java/lang/management/OjvmClassLoadingMXBean",
            )?)
        },
    ),
    method(
        "getMemoryMXBean",
        "()Ljava/lang/management/MemoryMXBean;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            object(bean_singleton(
                vm,
                "memoryBean",
                "Ljava/lang/management/MemoryMXBean;",
                "java/lang/management/OjvmMemoryMXBean",
            )?)
        },
    ),
    method(
        "getThreadMXBean",
        "()Ljava/lang/management/ThreadMXBean;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            object(bean_singleton(
                vm,
                "threadBean",
                "Ljava/lang/management/ThreadMXBean;",
                "java/lang/management/OjvmThreadMXBean",
            )?)
        },
    ),
    method(
        "getRuntimeMXBean",
        "()Ljava/lang/management/RuntimeMXBean;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            object(bean_singleton(
                vm,
                "runtimeBean",
                "Ljava/lang/management/RuntimeMXBean;",
                "java/lang/management/OjvmRuntimeMXBean",
            )?)
        },
    ),
    method(
        "getCompilationMXBean",
        "()Ljava/lang/management/CompilationMXBean;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            object(bean_singleton(
                vm,
                "compilationBean",
                "Ljava/lang/management/CompilationMXBean;",
                "java/lang/management/OjvmCompilationMXBean",
            )?)
        },
    ),
    method(
        "getOperatingSystemMXBean",
        "()Ljava/lang/management/OperatingSystemMXBean;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            object(bean_singleton(
                vm,
                "osBean",
                "Ljava/lang/management/OperatingSystemMXBean;",
                "java/lang/management/OjvmOperatingSystemMXBean",
            )?)
        },
    ),
    method(
        "getMemoryPoolMXBeans",
        "()Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let list = new_array_list(vm)?;
            let bean = bean_singleton(
                vm,
                "memoryPoolBean",
                "Ljava/lang/management/MemoryPoolMXBean;",
                "java/lang/management/OjvmMemoryPoolMXBean",
            )?;
            list_add(vm, list, bean)?;
            object(list)
        },
    ),
    method(
        "getMemoryManagerMXBeans",
        "()Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let list = new_array_list(vm)?;
            let bean = bean_singleton(
                vm,
                "memoryManagerBean",
                "Ljava/lang/management/MemoryManagerMXBean;",
                "java/lang/management/OjvmMemoryManagerMXBean",
            )?;
            list_add(vm, list, bean)?;
            object(list)
        },
    ),
    method(
        "getGarbageCollectorMXBeans",
        "()Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let list = new_array_list(vm)?;
            let bean = bean_singleton(
                vm,
                "garbageCollectorBean",
                "Ljava/lang/management/GarbageCollectorMXBean;",
                "java/lang/management/OjvmGarbageCollectorMXBean",
            )?;
            list_add(vm, list, bean)?;
            object(list)
        },
    ),
    method(
        "getPlatformMBeanServer",
        "()Ljavax/management/MBeanServer;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let factory = vm.resolve_class("java/lang/management/ManagementFactory")?;
            vm.ensure_initialized(factory)?;
            let server = vm
                .read_static_ref(factory, "server", "Ljavax/management/MBeanServer;")
                .unwrap_or(ObjectRef::NULL);
            let beans = [
                (
                    "threadBean",
                    "Ljava/lang/management/ThreadMXBean;",
                    "java/lang/management/OjvmThreadMXBean",
                    "java.lang:type=Threading",
                ),
                (
                    "memoryBean",
                    "Ljava/lang/management/MemoryMXBean;",
                    "java/lang/management/OjvmMemoryMXBean",
                    "java.lang:type=Memory",
                ),
                (
                    "runtimeBean",
                    "Ljava/lang/management/RuntimeMXBean;",
                    "java/lang/management/OjvmRuntimeMXBean",
                    "java.lang:type=Runtime",
                ),
                (
                    "osBean",
                    "Ljava/lang/management/OperatingSystemMXBean;",
                    "java/lang/management/OjvmOperatingSystemMXBean",
                    "java.lang:type=OperatingSystem",
                ),
                (
                    "classLoadingBean",
                    "Ljava/lang/management/ClassLoadingMXBean;",
                    "java/lang/management/OjvmClassLoadingMXBean",
                    "java.lang:type=ClassLoading",
                ),
                (
                    "compilationBean",
                    "Ljava/lang/management/CompilationMXBean;",
                    "java/lang/management/OjvmCompilationMXBean",
                    "java.lang:type=Compilation",
                ),
                (
                    "garbageCollectorBean",
                    "Ljava/lang/management/GarbageCollectorMXBean;",
                    "java/lang/management/OjvmGarbageCollectorMXBean",
                    "java.lang:type=GarbageCollector,name=G1 Young Generation",
                ),
                (
                    "memoryManagerBean",
                    "Ljava/lang/management/MemoryManagerMXBean;",
                    "java/lang/management/OjvmMemoryManagerMXBean",
                    "java.lang:type=MemoryManager,name=G1 Young Generation Manager",
                ),
                (
                    "memoryPoolBean",
                    "Ljava/lang/management/MemoryPoolMXBean;",
                    "java/lang/management/OjvmMemoryPoolMXBean",
                    "java.lang:type=MemoryPool,name=G1 Eden Space",
                ),
            ];
            for (field_name, descriptor, impl_class, object_name) in beans {
                let bean = bean_singleton(vm, field_name, descriptor, impl_class)?;
                register_platform_bean(vm, server, bean, object_name)?;
            }
            object(server)
        },
    ),
    method(
        "getPlatformMXBean",
        "(Ljava/lang/Class;)Ljava/lang/management/PlatformManagedObject;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(platform_bean_for_class(vm, ref_arg(args, 0))?),
    ),
    method(
        "getPlatformMXBeans",
        "(Ljava/lang/Class;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(platform_beans_for_class(vm, ref_arg(args, 0))?),
    ),
    method(
        "getPlatformManagementInterfaces",
        "()Ljava/util/Set;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_hash_set(vm)?),
    ),
    method(
        "newPlatformMXBeanProxy",
        "(Ljavax/management/MBeanServerConnection;Ljava/lang/String;Ljava/lang/Class;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            Err(vm.throw_new(
                "java/lang/UnsupportedOperationException",
                Some("MXBean proxies are not implemented"),
            ))
        },
    ),
    method(
        "getPlatformMXBean",
        "(Ljavax/management/MBeanServerConnection;Ljava/lang/Class;)Ljava/lang/management/PlatformManagedObject;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(platform_bean_for_class(vm, ref_arg(args, 1))?),
    ),
    method(
        "getPlatformMXBeans",
        "(Ljavax/management/MBeanServerConnection;Ljava/lang/Class;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(platform_beans_for_class(vm, ref_arg(args, 1))?),
    ),
    method(
        "getClassLoadingMXBean",
        "()Ljava/lang/management/ClassLoadingMXBean;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            object(bean_singleton(
                vm,
                "classLoadingBean",
                "Ljava/lang/management/ClassLoadingMXBean;",
                "java/lang/management/OjvmClassLoadingMXBean",
            )?)
        },
    ),
];

pub(crate) const MANAGEMENT_FACTORY: oxjvm_vm::NativeClass = class(
    "java/lang/management/ManagementFactory",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("startTime", "J", ACC_PRIVATE | ACC_STATIC, None),
        field(
            "server",
            "Ljavax/management/MBeanServer;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
        field(
            "threadBean",
            "Ljava/lang/management/ThreadMXBean;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
        field(
            "memoryBean",
            "Ljava/lang/management/MemoryMXBean;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
        field(
            "runtimeBean",
            "Ljava/lang/management/RuntimeMXBean;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
        field(
            "osBean",
            "Ljava/lang/management/OperatingSystemMXBean;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
        field(
            "classLoadingBean",
            "Ljava/lang/management/ClassLoadingMXBean;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
        field(
            "compilationBean",
            "Ljava/lang/management/CompilationMXBean;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
        field(
            "garbageCollectorBean",
            "Ljava/lang/management/GarbageCollectorMXBean;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
        field(
            "memoryManagerBean",
            "Ljava/lang/management/MemoryManagerMXBean;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
        field(
            "memoryPoolBean",
            "Ljava/lang/management/MemoryPoolMXBean;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
        field(
            "hotspotBean",
            "Lcom/sun/management/HotSpotDiagnosticMXBean;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
    ],
    &MANAGEMENT_FACTORY_METHODS,
    Some(management_factory_clinit),
);

const fn mxbean(name: &'static str) -> oxjvm_vm::NativeClass {
    class(
        name,
        Some("java/lang/Object"),
        &[],
        ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
        &[],
        &[],
        None,
    )
}

pub(crate) const MX_BEAN: oxjvm_vm::NativeClass = class(
    "java/lang/management/MXBean",
    Some("java/lang/Object"),
    &["java/lang/annotation/Annotation"],
    ACC_PUBLIC | ACC_ANNOTATION | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const BUFFER_POOL_MX_BEAN: oxjvm_vm::NativeClass =
    mxbean("java/lang/management/BufferPoolMXBean");
pub(crate) const PLATFORM_LOGGING_MX_BEAN: oxjvm_vm::NativeClass =
    mxbean("java/lang/management/PlatformLoggingMXBean");
pub(crate) const PLATFORM_MANAGED_OBJECT: oxjvm_vm::NativeClass = class(
    "java/lang/management/PlatformManagedObject",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[abstract_method(
        "getObjectName",
        "()Ljavax/management/ObjectName;",
    )],
    None,
);

fn new_object_name(vm: &mut Vm<'_>, text: &str) -> Result<Value, VmError> {
    let class = vm.resolve_class("javax/management/ObjectName")?;
    let (declaring, method) = vm
        .find_method(class, "<init>", "(Ljava/lang/String;)V")
        .ok_or_else(|| VmError::internal("ObjectName ctor"))?;
    let instance = vm.new_instance(class)?;
    let text = vm.make_string(text)?;
    vm.invoke_method(
        declaring,
        method,
        vec![Value::Ref(instance), Value::Ref(text)],
    )
    .map(|_| Value::Ref(instance))
}

pub(crate) const VM_OPTION: oxjvm_vm::NativeClass = class(
    "com/sun/management/VMOption",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("value", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("writeable", "Z", ACC_PRIVATE, None),
        field("origin", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &VM_OPTION_METHODS,
    None,
);

const VM_OPTION_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;ZLjava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            set_ref_field(vm, this, "name", "Ljava/lang/String;", ref_arg(args, 1))?;
            set_ref_field(vm, this, "value", "Ljava/lang/String;", ref_arg(args, 2))?;
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "writeable", "Z")?;
            vm.set_instance_int(this, declaring, field, crate::bool_arg(args, 3) as i32);
            set_ref_field(vm, this, "origin", "Ljava/lang/String;", ref_arg(args, 4))?;
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
        "getValue",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "value", "Ljava/lang/String;")),
    ),
    method("isWriteable", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        let (declaring, field) = vm.find_field(class, "writeable", "Z")?;
        boolean(vm.read_instance_int(this, declaring, field).unwrap_or(0) != 0)
    }),
    method(
        "getOrigin",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "origin",
                "Ljava/lang/String;",
            ))
        },
    ),
];

pub(crate) const THREAD_INFO: oxjvm_vm::NativeClass = class(
    "java/lang/management/ThreadInfo",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[
        field("threadId", "J", ACC_PRIVATE, None),
        field("threadName", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("threadState", "Ljava/lang/Thread$State;", ACC_PRIVATE, None),
    ],
    &THREAD_INFO_METHODS,
    None,
);

const THREAD_INFO_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "<init>",
        "(JLjava/lang/String;Ljava/lang/Thread$State;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            set_long_field(vm, this, "threadId", args[1].as_long())?;
            set_ref_field(
                vm,
                this,
                "threadName",
                "Ljava/lang/String;",
                ref_arg(args, 2),
            )?;
            set_ref_field(
                vm,
                this,
                "threadState",
                "Ljava/lang/Thread$State;",
                ref_arg(args, 3),
            )?;
            void()
        },
    ),
    method("getThreadId", "()J", ACC_PUBLIC, |vm, _, args| {
        long(long_field(vm, receiver(args), "threadId"))
    }),
    method(
        "getThreadName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "threadName",
                "Ljava/lang/String;",
            ))
        },
    ),
    method(
        "getThreadState",
        "()Ljava/lang/Thread$State;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "threadState",
                "Ljava/lang/Thread$State;",
            ))
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = alloc::format!(
                "ThreadInfo[{}]",
                vm.string_or_empty(ref_field(
                    vm,
                    receiver(args),
                    "threadName",
                    "Ljava/lang/String;"
                ))
            );
            object(vm.make_string(&text)?)
        },
    ),
]; // -------------------------------------------------------------------------------------------
// java.lang.management.MemoryUsage
// -------------------------------------------------------------------------------------------

const MEMORY_USAGE_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method("<init>", "(JJJJ)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        set_long_field(vm, this, "init", args[1].as_long())?;
        set_long_field(vm, this, "used", args[2].as_long())?;
        set_long_field(vm, this, "committed", args[3].as_long())?;
        set_long_field(vm, this, "max", args[4].as_long())?;
        void()
    }),
    method("getInit", "()J", ACC_PUBLIC, |vm, _, args| {
        long(long_field(vm, receiver(args), "init"))
    }),
    method("getUsed", "()J", ACC_PUBLIC, |vm, _, args| {
        long(long_field(vm, receiver(args), "used"))
    }),
    method("getCommitted", "()J", ACC_PUBLIC, |vm, _, args| {
        long(long_field(vm, receiver(args), "committed"))
    }),
    method("getMax", "()J", ACC_PUBLIC, |vm, _, args| {
        long(long_field(vm, receiver(args), "max"))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = alloc::format!(
                "init = {}\nused = {}\ncommitted = {}\nmax = {}",
                long_field(vm, this, "init"),
                long_field(vm, this, "used"),
                long_field(vm, this, "committed"),
                long_field(vm, this, "max")
            );
            object(vm.make_string(&text)?)
        },
    ),
];

pub(crate) const MEMORY_USAGE: oxjvm_vm::NativeClass = class(
    "java/lang/management/MemoryUsage",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("init", "J", ACC_PRIVATE, None),
        field("used", "J", ACC_PRIVATE, None),
        field("committed", "J", ACC_PRIVATE, None),
        field("max", "J", ACC_PRIVATE, None),
    ],
    &MEMORY_USAGE_METHODS,
    None,
);

fn memory_type_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/lang/management/MemoryType")?;
    for (ordinal, name) in ["HEAP", "NON_HEAP"].iter().enumerate() {
        let instance = vm.new_instance(class)?;
        if let Some((declaring, field)) = vm.find_field_opt(class, "ordinal", "I") {
            vm.set_instance_int(instance, declaring, field, ordinal as i32);
        }
        if let Some((declaring, field)) = vm.find_field_opt(class, "name", "Ljava/lang/String;") {
            let text = vm.make_string(name)?;
            vm.set_instance_ref(instance, declaring, field, text);
        }
        let descriptor = "Ljava/lang/management/MemoryType;";
        vm.set_static_value(class, name, descriptor, Value::Ref(instance));
    }
    Ok(())
}

const MEMORY_TYPE_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method(
        "values",
        "()[Ljava/lang/management/MemoryType;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/lang/management/MemoryType")?;
            vm.ensure_initialized(class)?;
            let array = vm.allocate_object_array(class, 2)?;
            for (index, name) in ["HEAP", "NON_HEAP"].iter().enumerate() {
                let value = vm
                    .read_static_ref(class, name, "Ljava/lang/management/MemoryType;")
                    .unwrap_or(ObjectRef::NULL);
                vm.array_set_ref(array, index, value)?;
            }
            object(array)
        },
    ),
    method(
        "valueOf",
        "(Ljava/lang/String;)Ljava/lang/management/MemoryType;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = ref_arg(args, 0);
            if name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let text = vm.string_or_empty(name);
            let class = vm.resolve_class("java/lang/management/MemoryType")?;
            vm.ensure_initialized(class)?;
            let value = vm
                .read_static_ref(class, &text, "Ljava/lang/management/MemoryType;")
                .ok_or_else(|| {
                    vm.throw_new(
                        "java/lang/IllegalArgumentException",
                        Some("No enum constant"),
                    )
                })?;
            object(value)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "name", "Ljava/lang/String;")),
    ),
];

pub(crate) const MEMORY_TYPE: oxjvm_vm::NativeClass = class(
    "java/lang/management/MemoryType",
    Some("java/lang/Enum"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_ENUM,
    &[
        field(
            "HEAP",
            "Ljava/lang/management/MemoryType;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "NON_HEAP",
            "Ljava/lang/management/MemoryType;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
    ],
    &MEMORY_TYPE_METHODS,
    Some(memory_type_clinit),
);

pub(crate) const MEMORY_NOTIFICATION_INFO: oxjvm_vm::NativeClass = class(
    "java/lang/management/MemoryNotificationInfo",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

// CompositeData / CompositeType (javax.management.openmbean)
pub(crate) const COMPOSITE_DATA: oxjvm_vm::NativeClass = class(
    "javax/management/openmbean/CompositeData",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);
pub(crate) const COMPOSITE_TYPE: oxjvm_vm::NativeClass = class(
    "javax/management/openmbean/CompositeType",
    Some("javax/management/openmbean/OpenType"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
pub(crate) const OPEN_TYPE: oxjvm_vm::NativeClass = class(
    "javax/management/openmbean/OpenType",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &[],
    None,
);
pub(crate) const SIMPLE_TYPE: oxjvm_vm::NativeClass = class(
    "javax/management/openmbean/SimpleType",
    Some("javax/management/openmbean/OpenType"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &SIMPLE_TYPE_FIELDS,
    &SIMPLE_TYPE_METHODS,
    Some(simple_type_clinit),
);

static SIMPLE_TYPE_FIELDS: [oxjvm_vm::NativeFieldDef; 15] = [
    field("className", "Ljava/lang/String;", ACC_PRIVATE, None),
    field(
        "VOID",
        "Ljavax/management/openmbean/SimpleType;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "BOOLEAN",
        "Ljavax/management/openmbean/SimpleType;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "CHARACTER",
        "Ljavax/management/openmbean/SimpleType;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "BYTE",
        "Ljavax/management/openmbean/SimpleType;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "SHORT",
        "Ljavax/management/openmbean/SimpleType;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "INTEGER",
        "Ljavax/management/openmbean/SimpleType;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "LONG",
        "Ljavax/management/openmbean/SimpleType;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "FLOAT",
        "Ljavax/management/openmbean/SimpleType;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "DOUBLE",
        "Ljavax/management/openmbean/SimpleType;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "STRING",
        "Ljavax/management/openmbean/SimpleType;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "BIGDECIMAL",
        "Ljavax/management/openmbean/SimpleType;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "BIGINTEGER",
        "Ljavax/management/openmbean/SimpleType;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "DATE",
        "Ljavax/management/openmbean/SimpleType;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "OBJECTNAME",
        "Ljavax/management/openmbean/SimpleType;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
];

const SIMPLE_TYPE_NAMES: [(&str, &str); 14] = [
    ("VOID", "java.lang.Void"),
    ("BOOLEAN", "java.lang.Boolean"),
    ("CHARACTER", "java.lang.Character"),
    ("BYTE", "java.lang.Byte"),
    ("SHORT", "java.lang.Short"),
    ("INTEGER", "java.lang.Integer"),
    ("LONG", "java.lang.Long"),
    ("FLOAT", "java.lang.Float"),
    ("DOUBLE", "java.lang.Double"),
    ("STRING", "java.lang.String"),
    ("BIGDECIMAL", "java.math.BigDecimal"),
    ("BIGINTEGER", "java.math.BigInteger"),
    ("DATE", "java.util.Date"),
    ("OBJECTNAME", "javax.management.ObjectName"),
];

const SIMPLE_TYPE_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "getClassName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let text = vm
                .read_string_field(this, class, "className")
                .unwrap_or_default();
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let text = vm
                .read_string_field(this, class, "className")
                .unwrap_or_default();
            object(vm.make_string(&text)?)
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
            let first = vm
                .read_string_field(receiver(args), vm.class_of(receiver(args)), "className")
                .unwrap_or_default();
            let second = vm
                .read_string_field(other, vm.class_of(other), "className")
                .unwrap_or_default();
            boolean(first == second)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let text = vm
            .read_string_field(receiver(args), vm.class_of(receiver(args)), "className")
            .unwrap_or_default();
        let mut hash = 0i32;
        for unit in text.encode_utf16() {
            hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
        }
        int(hash)
    }),
    method("isValue", "(Ljava/lang/Object;)Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
];

fn simple_type_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("javax/management/openmbean/SimpleType")?;
    for (name, class_name) in SIMPLE_TYPE_NAMES {
        let instance = vm.new_instance(class)?;
        vm.set_string_field(instance, class, "className", class_name)?;
        vm.set_static_value(
            class,
            name,
            "Ljavax/management/openmbean/SimpleType;",
            Value::Ref(instance),
        );
    }
    Ok(())
}
pub(crate) const TABULAR_TYPE: oxjvm_vm::NativeClass = class(
    "javax/management/openmbean/TabularType",
    Some("javax/management/openmbean/CompositeType"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
pub(crate) const TABULAR_DATA: oxjvm_vm::NativeClass = class(
    "javax/management/openmbean/TabularData",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);
pub(crate) const TABULAR_DATA_SUPPORT: oxjvm_vm::NativeClass = class(
    "javax/management/openmbean/TabularDataSupport",
    Some("java/lang/Object"),
    &["javax/management/openmbean/TabularData", "java/util/Map"],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

// -------------------------------------------------------------------------------------------
// Bean interfaces: declare the methods so invokeinterface resolves
// -------------------------------------------------------------------------------------------

const THREAD_MX_BEAN_METHODS: [oxjvm_vm::NativeMethodDef; 24] = [
    abstract_method("getThreadCount", "()I"),
    abstract_method("getPeakThreadCount", "()I"),
    abstract_method("getTotalStartedThreadCount", "()J"),
    abstract_method("getDaemonThreadCount", "()I"),
    abstract_method("getAllThreadIds", "()[J"),
    abstract_method("getThreadInfo", "(J)Ljava/lang/management/ThreadInfo;"),
    abstract_method("getThreadInfo", "(JI)Ljava/lang/management/ThreadInfo;"),
    abstract_method("getThreadInfo", "([JZZ)[Ljava/lang/management/ThreadInfo;"),
    abstract_method("dumpAllThreads", "(ZZ)[Ljava/lang/management/ThreadInfo;"),
    abstract_method("findDeadlockedThreads", "()[J"),
    abstract_method("findMonitorDeadlockedThreads", "()[J"),
    abstract_method("isThreadCpuTimeSupported", "()Z"),
    abstract_method("isCurrentThreadCpuTimeSupported", "()Z"),
    abstract_method("isThreadCpuTimeEnabled", "()Z"),
    abstract_method("setThreadCpuTimeEnabled", "(Z)V"),
    abstract_method("getCurrentThreadCpuTime", "()J"),
    abstract_method("getCurrentThreadUserTime", "()J"),
    abstract_method("getThreadCpuTime", "(J)J"),
    abstract_method("getThreadUserTime", "(J)J"),
    abstract_method("isThreadContentionMonitoringSupported", "()Z"),
    abstract_method("isThreadContentionMonitoringEnabled", "()Z"),
    abstract_method("setThreadContentionMonitoringEnabled", "(Z)V"),
    abstract_method("isObjectMonitorUsageSupported", "()Z"),
    abstract_method("isSynchronizerUsageSupported", "()Z"),
];

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

const fn redefined_thread_mx_bean() -> oxjvm_vm::NativeClass {
    class(
        "java/lang/management/ThreadMXBean",
        Some("java/lang/Object"),
        &["java/lang/management/PlatformManagedObject"],
        ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
        &[],
        &THREAD_MX_BEAN_METHODS,
        None,
    )
}

pub(crate) const THREAD_MX_BEAN_IMPL: oxjvm_vm::NativeClass = redefined_thread_mx_bean();

const THREAD_MX_BEAN_IMPL_METHODS: [oxjvm_vm::NativeMethodDef; 30] = [
    method(
        "getObjectName",
        "()Ljavax/management/ObjectName;",
        ACC_PUBLIC,
        |vm, _, _| new_object_name(vm, "java.lang:type=Threading"),
    ),
    method("getThreadCount", "()I", ACC_PUBLIC, |vm, _, _| {
        int(vm.live_thread_count() as i32)
    }),
    method("getPeakThreadCount", "()I", ACC_PUBLIC, |vm, _, _| {
        int(vm.live_thread_count() as i32)
    }),
    method(
        "getTotalStartedThreadCount",
        "()J",
        ACC_PUBLIC,
        |vm, _, _| long(vm.live_thread_count() as i64),
    ),
    method("getDaemonThreadCount", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method("getAllThreadIds", "()[J", ACC_PUBLIC, |vm, _, _| {
        let count = vm.live_thread_count();
        let ids: Vec<i64> = (1..=count as i64).collect();
        object(new_long_array(vm, &ids)?)
    }),
    method(
        "getThreadInfo",
        "(J)Ljava/lang/management/ThreadInfo;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getThreadInfo",
        "(JI)Ljava/lang/management/ThreadInfo;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getThreadInfo",
        "([JZZ)[Ljava/lang/management/ThreadInfo;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/lang/management/ThreadInfo")?;
            object(vm.allocate_object_array(class, 0)?)
        },
    ),
    method(
        "dumpAllThreads",
        "(ZZ)[Ljava/lang/management/ThreadInfo;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/lang/management/ThreadInfo")?;
            object(vm.allocate_object_array(class, 0)?)
        },
    ),
    method("findDeadlockedThreads", "()[J", ACC_PUBLIC, |_, _, _| {
        object(ObjectRef::NULL)
    }),
    method(
        "findMonitorDeadlockedThreads",
        "()[J",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method("isThreadCpuTimeSupported", "()Z", ACC_PUBLIC, |_, _, _| {
        boolean(true)
    }),
    method(
        "isCurrentThreadCpuTimeSupported",
        "()Z",
        ACC_PUBLIC,
        |_, _, _| boolean(true),
    ),
    method("isThreadCpuTimeEnabled", "()Z", ACC_PUBLIC, |_, _, _| {
        boolean(true)
    }),
    method("setThreadCpuTimeEnabled", "(Z)V", ACC_PUBLIC, |_, _, _| {
        void()
    }),
    method("getCurrentThreadCpuTime", "()J", ACC_PUBLIC, |_, _, _| {
        long(0)
    }),
    method("getCurrentThreadUserTime", "()J", ACC_PUBLIC, |_, _, _| {
        long(0)
    }),
    method("getThreadCpuTime", "(J)J", ACC_PUBLIC, |_, _, _| long(0)),
    method("getThreadUserTime", "(J)J", ACC_PUBLIC, |_, _, _| long(0)),
    method(
        "isThreadContentionMonitoringSupported",
        "()Z",
        ACC_PUBLIC,
        |_, _, _| boolean(true),
    ),
    method(
        "isThreadContentionMonitoringEnabled",
        "()Z",
        ACC_PUBLIC,
        |_, _, _| boolean(false),
    ),
    method(
        "setThreadContentionMonitoringEnabled",
        "(Z)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "isObjectMonitorUsageSupported",
        "()Z",
        ACC_PUBLIC,
        |_, _, _| boolean(false),
    ),
    method(
        "isSynchronizerUsageSupported",
        "()Z",
        ACC_PUBLIC,
        |_, _, _| boolean(false),
    ),
    method("getThreadAllocatedBytes", "(J)J", ACC_PUBLIC, |_, _, _| {
        long(-1)
    }),
    method(
        "getThreadAllocatedBytes",
        "([J)[J",
        ACC_PUBLIC,
        |vm, _, args| {
            let ids = ref_arg(args, 1);
            if ids.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let length = vm.array_length(ids)?;
            let values: Vec<i64> = vec![-1; length];
            object(new_long_array(vm, &values)?)
        },
    ),
    method(
        "isThreadAllocatedMemorySupported",
        "()Z",
        ACC_PUBLIC,
        |_, _, _| boolean(false),
    ),
    method(
        "isThreadAllocatedMemoryEnabled",
        "()Z",
        ACC_PUBLIC,
        |_, _, _| boolean(false),
    ),
    method(
        "setThreadAllocatedMemoryEnabled",
        "(Z)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
];

pub(crate) const OJVM_THREAD_MX_BEAN: oxjvm_vm::NativeClass = class(
    "java/lang/management/OjvmThreadMXBean",
    Some("java/lang/Object"),
    &[
        "java/lang/management/ThreadMXBean",
        "com/sun/management/ThreadMXBean",
    ],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &THREAD_MX_BEAN_IMPL_METHODS,
    None,
);

const MEMORY_MX_BEAN_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    abstract_method("getHeapMemoryUsage", "()Ljava/lang/management/MemoryUsage;"),
    abstract_method(
        "getNonHeapMemoryUsage",
        "()Ljava/lang/management/MemoryUsage;",
    ),
    abstract_method("getObjectPendingFinalizationCount", "()I"),
    abstract_method("isVerbose", "()Z"),
    abstract_method("setVerbose", "(Z)V"),
    abstract_method("gc", "()V"),
];

pub(crate) const MEMORY_MX_BEAN_DEF: oxjvm_vm::NativeClass = class(
    "java/lang/management/MemoryMXBean",
    Some("java/lang/Object"),
    &["java/lang/management/PlatformManagedObject"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &MEMORY_MX_BEAN_METHODS,
    None,
);

const OJVM_MEMORY_MX_BEAN_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "getObjectName",
        "()Ljavax/management/ObjectName;",
        ACC_PUBLIC,
        |vm, _, _| new_object_name(vm, "java.lang:type=Memory"),
    ),
    method(
        "getHeapMemoryUsage",
        "()Ljava/lang/management/MemoryUsage;",
        ACC_PUBLIC,
        |vm, _, _| {
            let used = vm.heap_used_bytes() as i64;
            let committed = vm.heap_total_bytes() as i64;
            let max = vm.heap_max_bytes().map_or(-1, |value| value as i64);
            object(new_usage(vm, 0, used, committed, max)?)
        },
    ),
    method(
        "getNonHeapMemoryUsage",
        "()Ljava/lang/management/MemoryUsage;",
        ACC_PUBLIC,
        |vm, _, _| object(new_usage(vm, 0, 0, 0, -1)?),
    ),
    method(
        "getObjectPendingFinalizationCount",
        "()I",
        ACC_PUBLIC,
        |_, _, _| int(0),
    ),
    method("isVerbose", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("setVerbose", "(Z)V", ACC_PUBLIC, |_, _, _| void()),
    method("gc", "()V", ACC_PUBLIC, |vm, _, _| {
        vm.gc();
        void()
    }),
];

pub(crate) const OJVM_MEMORY_MX_BEAN: oxjvm_vm::NativeClass = class(
    "java/lang/management/OjvmMemoryMXBean",
    Some("java/lang/Object"),
    &["java/lang/management/MemoryMXBean"],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &OJVM_MEMORY_MX_BEAN_METHODS,
    None,
);

const RUNTIME_MX_BEAN_METHODS: [oxjvm_vm::NativeMethodDef; 15] = [
    abstract_method("getName", "()Ljava/lang/String;"),
    abstract_method("getVmName", "()Ljava/lang/String;"),
    abstract_method("getVmVendor", "()Ljava/lang/String;"),
    abstract_method("getVmVersion", "()Ljava/lang/String;"),
    abstract_method("getSpecName", "()Ljava/lang/String;"),
    abstract_method("getSpecVendor", "()Ljava/lang/String;"),
    abstract_method("getSpecVersion", "()Ljava/lang/String;"),
    abstract_method("getManagementSpecVersion", "()Ljava/lang/String;"),
    abstract_method("getClassPath", "()Ljava/lang/String;"),
    abstract_method("getLibraryPath", "()Ljava/lang/String;"),
    abstract_method("getBootClassPath", "()Ljava/lang/String;"),
    abstract_method("getInputArguments", "()Ljava/util/List;"),
    abstract_method("getUptime", "()J"),
    abstract_method("getStartTime", "()J"),
    abstract_method("getSystemProperties", "()Ljava/util/Properties;"),
];

pub(crate) const RUNTIME_MX_BEAN_DEF: oxjvm_vm::NativeClass = class(
    "java/lang/management/RuntimeMXBean",
    Some("java/lang/Object"),
    &["java/lang/management/PlatformManagedObject"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &RUNTIME_MX_BEAN_METHODS,
    None,
);

fn runtime_uptime(vm: &mut Vm<'_>) -> i64 {
    let factory = vm.resolve_class("java/lang/management/ManagementFactory");
    let start = factory
        .ok()
        .and_then(|class| {
            vm.ensure_initialized(class).ok()?;
            match vm.classes.get(class).static_values.first() {
                Some(Value::Long(value)) => Some(*value),
                _ => None,
            }
        })
        .unwrap_or(0);
    (vm.host_time_millis() - start).max(0)
}

const OJVM_RUNTIME_MX_BEAN_METHODS: [oxjvm_vm::NativeMethodDef; 18] = [
    method(
        "getObjectName",
        "()Ljavax/management/ObjectName;",
        ACC_PUBLIC,
        |vm, _, _| new_object_name(vm, "java.lang:type=Runtime"),
    ),
    method("getName", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, _| {
        object(vm.make_string("1@localhost")?)
    }),
    method(
        "getVmName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("oxjvm")?),
    ),
    method(
        "getVmVendor",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("oxjvm")?),
    ),
    method(
        "getVmVersion",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("27")?),
    ),
    method(
        "getSpecName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("Java Virtual Machine Specification")?),
    ),
    method(
        "getSpecVendor",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("Oracle Corporation")?),
    ),
    method(
        "getSpecVersion",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("27")?),
    ),
    method(
        "getManagementSpecVersion",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("1.4")?),
    ),
    method(
        "getClassPath",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| {
            let text = system_property_str(vm, "java.class.path");
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "getLibraryPath",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| {
            let text = system_property_str(vm, "java.library.path");
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "getBootClassPath",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("")?),
    ),
    method(
        "getInputArguments",
        "()Ljava/util/List;",
        ACC_PUBLIC,
        |vm, _, _| object(new_array_list(vm)?),
    ),
    method("getPid", "()J", ACC_PUBLIC, |_, _, _| long(1)),
    method("getProcessId", "()J", ACC_PUBLIC, |_, _, _| long(1)),
    method("getUptime", "()J", ACC_PUBLIC, |vm, _, _| {
        long(runtime_uptime(vm))
    }),
    method("getStartTime", "()J", ACC_PUBLIC, |vm, _, _| {
        let start = runtime_uptime(vm);
        long(vm.host_time_millis() - start)
    }),
    method(
        "getSystemProperties",
        "()Ljava/util/Properties;",
        ACC_PUBLIC,
        |vm, _, _| {
            let system = vm.resolve_class("java/lang/System")?;
            vm.ensure_initialized(system)?;
            object(
                vm.read_static_ref_if_set(system, "props", "Ljava/util/Properties;")
                    .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
];

pub(crate) const OJVM_RUNTIME_MX_BEAN: oxjvm_vm::NativeClass = class(
    "java/lang/management/OjvmRuntimeMXBean",
    Some("java/lang/Object"),
    &["com/sun/management/RuntimeMXBean"],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &OJVM_RUNTIME_MX_BEAN_METHODS,
    None,
);

const OS_MX_BEAN_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    abstract_method("getName", "()Ljava/lang/String;"),
    abstract_method("getArch", "()Ljava/lang/String;"),
    abstract_method("getVersion", "()Ljava/lang/String;"),
    abstract_method("getAvailableProcessors", "()I"),
    abstract_method("getSystemLoadAverage", "()D"),
];

pub(crate) const OS_MX_BEAN_DEF: oxjvm_vm::NativeClass = class(
    "java/lang/management/OperatingSystemMXBean",
    Some("java/lang/Object"),
    &["java/lang/management/PlatformManagedObject"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &OS_MX_BEAN_METHODS,
    None,
);

const COM_SUN_OS_MX_BEAN_METHODS: [oxjvm_vm::NativeMethodDef; 13] = [
    abstract_method("getCommittedVirtualMemorySize", "()J"),
    abstract_method("getTotalSwapSpaceSize", "()J"),
    abstract_method("getFreeSwapSpaceSize", "()J"),
    abstract_method("getProcessCpuTime", "()J"),
    abstract_method("getFreeMemorySize", "()J"),
    abstract_method("getTotalMemorySize", "()J"),
    abstract_method("getFreePhysicalMemorySize", "()J"),
    abstract_method("getTotalPhysicalMemorySize", "()J"),
    abstract_method("getSystemCpuLoad", "()D"),
    abstract_method("getProcessCpuLoad", "()D"),
    abstract_method("getCpuLoad", "()D"),
    abstract_method("getProcessCpuTime", "()J"),
    abstract_method("isProcessCpuTimeSupported", "()Z"),
];

pub(crate) const COM_SUN_OS_MX_BEAN_DEF: oxjvm_vm::NativeClass = class(
    "com/sun/management/OperatingSystemMXBean",
    Some("java/lang/Object"),
    &["java/lang/management/OperatingSystemMXBean"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &COM_SUN_OS_MX_BEAN_METHODS,
    None,
);

const OJVM_OS_MX_BEAN_METHODS: [oxjvm_vm::NativeMethodDef; 18] = [
    method(
        "getObjectName",
        "()Ljavax/management/ObjectName;",
        ACC_PUBLIC,
        |vm, _, _| new_object_name(vm, "java.lang:type=OperatingSystem"),
    ),
    method("getName", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, _| {
        let text = system_property_str(vm, "os.name");
        object(vm.make_string(&text)?)
    }),
    method("getArch", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, _| {
        let text = system_property_str(vm, "os.arch");
        object(vm.make_string(&text)?)
    }),
    method(
        "getVersion",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| {
            let text = system_property_str(vm, "os.version");
            object(vm.make_string(&text)?)
        },
    ),
    method("getAvailableProcessors", "()I", ACC_PUBLIC, |_, _, _| {
        int(1)
    }),
    method("getSystemLoadAverage", "()D", ACC_PUBLIC, |_, _, _| {
        crate::double(-1.0)
    }),
    method(
        "getCommittedVirtualMemorySize",
        "()J",
        ACC_PUBLIC,
        |_, _, _| long(0),
    ),
    method("getTotalSwapSpaceSize", "()J", ACC_PUBLIC, |_, _, _| {
        long(0)
    }),
    method("getFreeSwapSpaceSize", "()J", ACC_PUBLIC, |_, _, _| long(0)),
    method("getProcessCpuTime", "()J", ACC_PUBLIC, |_, _, _| long(0)),
    method("getFreeMemorySize", "()J", ACC_PUBLIC, |_, _, _| {
        long(1 << 31)
    }),
    method("getTotalMemorySize", "()J", ACC_PUBLIC, |_, _, _| {
        long(1 << 33)
    }),
    method("getFreePhysicalMemorySize", "()J", ACC_PUBLIC, |_, _, _| {
        long(1 << 31)
    }),
    method(
        "getTotalPhysicalMemorySize",
        "()J",
        ACC_PUBLIC,
        |_, _, _| long(1 << 33),
    ),
    method("getSystemCpuLoad", "()D", ACC_PUBLIC, |_, _, _| {
        crate::double(-1.0)
    }),
    method("getProcessCpuLoad", "()D", ACC_PUBLIC, |_, _, _| {
        crate::double(-1.0)
    }),
    method("getCpuLoad", "()D", ACC_PUBLIC, |_, _, _| {
        crate::double(-1.0)
    }),
    method("isProcessCpuTimeSupported", "()Z", ACC_PUBLIC, |_, _, _| {
        boolean(true)
    }),
];

pub(crate) const OJVM_OS_MX_BEAN: oxjvm_vm::NativeClass = class(
    "java/lang/management/OjvmOperatingSystemMXBean",
    Some("java/lang/Object"),
    &["com/sun/management/OperatingSystemMXBean"],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &OJVM_OS_MX_BEAN_METHODS,
    None,
);

const CLASS_LOADING_MX_BEAN_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    abstract_method("getTotalLoadedClassCount", "()J"),
    abstract_method("getLoadedClassCount", "()I"),
    abstract_method("getUnloadedClassCount", "()J"),
    abstract_method("isVerbose", "()Z"),
    abstract_method("setVerbose", "(Z)V"),
];

pub(crate) const CLASS_LOADING_MX_BEAN_DEF: oxjvm_vm::NativeClass = class(
    "java/lang/management/ClassLoadingMXBean",
    Some("java/lang/Object"),
    &["java/lang/management/PlatformManagedObject"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &CLASS_LOADING_MX_BEAN_METHODS,
    None,
);

const OJVM_CLASS_LOADING_MX_BEAN_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "getObjectName",
        "()Ljavax/management/ObjectName;",
        ACC_PUBLIC,
        |vm, _, _| new_object_name(vm, "java.lang:type=ClassLoading"),
    ),
    method("getTotalLoadedClassCount", "()J", ACC_PUBLIC, |vm, _, _| {
        long(vm.classes.len() as i64)
    }),
    method("getLoadedClassCount", "()I", ACC_PUBLIC, |vm, _, _| {
        int(vm.classes.len() as i32)
    }),
    method("getUnloadedClassCount", "()J", ACC_PUBLIC, |_, _, _| {
        long(0)
    }),
    method("isVerbose", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("setVerbose", "(Z)V", ACC_PUBLIC, |_, _, _| void()),
];

pub(crate) const OJVM_CLASS_LOADING_MX_BEAN: oxjvm_vm::NativeClass = class(
    "java/lang/management/OjvmClassLoadingMXBean",
    Some("java/lang/Object"),
    &["java/lang/management/ClassLoadingMXBean"],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &OJVM_CLASS_LOADING_MX_BEAN_METHODS,
    None,
);

const COMPILATION_MX_BEAN_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    abstract_method("getName", "()Ljava/lang/String;"),
    abstract_method("isCompilationTimeMonitoringSupported", "()Z"),
    abstract_method("getTotalCompilationTime", "()J"),
];

pub(crate) const COMPILATION_MX_BEAN_DEF: oxjvm_vm::NativeClass = class(
    "java/lang/management/CompilationMXBean",
    Some("java/lang/Object"),
    &["java/lang/management/PlatformManagedObject"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &COMPILATION_MX_BEAN_METHODS,
    None,
);

const OJVM_COMPILATION_MX_BEAN_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "getObjectName",
        "()Ljavax/management/ObjectName;",
        ACC_PUBLIC,
        |vm, _, _| new_object_name(vm, "java.lang:type=Compilation"),
    ),
    method("getName", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, _| {
        object(vm.make_string("oxjvm")?)
    }),
    method(
        "isCompilationTimeMonitoringSupported",
        "()Z",
        ACC_PUBLIC,
        |_, _, _| boolean(false),
    ),
    method("getTotalCompilationTime", "()J", ACC_PUBLIC, |_, _, _| {
        long(0)
    }),
];

pub(crate) const OJVM_COMPILATION_MX_BEAN: oxjvm_vm::NativeClass = class(
    "java/lang/management/OjvmCompilationMXBean",
    Some("java/lang/Object"),
    &["java/lang/management/CompilationMXBean"],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &OJVM_COMPILATION_MX_BEAN_METHODS,
    None,
);

const GC_MX_BEAN_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    abstract_method("getName", "()Ljava/lang/String;"),
    abstract_method("isValid", "()Z"),
    abstract_method("getMemoryPoolNames", "()[Ljava/lang/String;"),
    abstract_method("getCollectionCount", "()J"),
    abstract_method("getCollectionTime", "()J"),
    abstract_method("getLastGcInfo", "()Lcom/sun/management/GcInfo;"),
];

pub(crate) const GC_MX_BEAN_DEF: oxjvm_vm::NativeClass = class(
    "java/lang/management/GarbageCollectorMXBean",
    Some("java/lang/Object"),
    &["java/lang/management/MemoryManagerMXBean"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &GC_MX_BEAN_METHODS,
    None,
);

const OJVM_GC_MX_BEAN_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "getObjectName",
        "()Ljavax/management/ObjectName;",
        ACC_PUBLIC,
        |vm, _, _| {
            new_object_name(
                vm,
                "java.lang:type=GarbageCollector,name=G1 Young Generation",
            )
        },
    ),
    method("getName", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, _| {
        object(vm.make_string("G1 Young Generation")?)
    }),
    method("isValid", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
    method(
        "getMemoryPoolNames",
        "()[Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(new_string_array(vm, &["G1 Eden Space"])?),
    ),
    method("getCollectionCount", "()J", ACC_PUBLIC, |_, _, _| long(0)),
    method("getCollectionTime", "()J", ACC_PUBLIC, |_, _, _| long(0)),
    method(
        "getLastGcInfo",
        "()Lcom/sun/management/GcInfo;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
];

pub(crate) const OJVM_GC_MX_BEAN: oxjvm_vm::NativeClass = class(
    "java/lang/management/OjvmGarbageCollectorMXBean",
    Some("java/lang/Object"),
    &["com/sun/management/GarbageCollectorMXBean"],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &OJVM_GC_MX_BEAN_METHODS,
    None,
);

pub(crate) const GC_INFO: oxjvm_vm::NativeClass = class(
    "com/sun/management/GcInfo",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

const MEMORY_MANAGER_MX_BEAN_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    abstract_method("getName", "()Ljava/lang/String;"),
    abstract_method("isValid", "()Z"),
    abstract_method("getMemoryPoolNames", "()[Ljava/lang/String;"),
];

pub(crate) const MEMORY_MANAGER_MX_BEAN_DEF: oxjvm_vm::NativeClass = class(
    "java/lang/management/MemoryManagerMXBean",
    Some("java/lang/Object"),
    &["java/lang/management/PlatformManagedObject"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &MEMORY_MANAGER_MX_BEAN_METHODS,
    None,
);

const OJVM_MEMORY_MANAGER_MX_BEAN_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "getObjectName",
        "()Ljavax/management/ObjectName;",
        ACC_PUBLIC,
        |vm, _, _| {
            new_object_name(
                vm,
                "java.lang:type=MemoryManager,name=G1 Young Generation Manager",
            )
        },
    ),
    method("getName", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, _| {
        object(vm.make_string("G1 Young Generation Manager")?)
    }),
    method("isValid", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
    method(
        "getMemoryPoolNames",
        "()[Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(new_string_array(vm, &["G1 Eden Space"])?),
    ),
];

pub(crate) const OJVM_MEMORY_MANAGER_MX_BEAN: oxjvm_vm::NativeClass = class(
    "java/lang/management/OjvmMemoryManagerMXBean",
    Some("java/lang/Object"),
    &["java/lang/management/MemoryManagerMXBean"],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &OJVM_MEMORY_MANAGER_MX_BEAN_METHODS,
    None,
);

const MEMORY_POOL_MX_BEAN_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
    abstract_method("getName", "()Ljava/lang/String;"),
    abstract_method("getType", "()Ljava/lang/management/MemoryType;"),
    abstract_method("getUsage", "()Ljava/lang/management/MemoryUsage;"),
    abstract_method("getPeakUsage", "()Ljava/lang/management/MemoryUsage;"),
    abstract_method("resetPeakUsage", "()V"),
    abstract_method("getCollectionUsage", "()Ljava/lang/management/MemoryUsage;"),
    abstract_method("isValid", "()Z"),
    abstract_method("getMemoryManagerNames", "()[Ljava/lang/String;"),
    abstract_method("isUsageThresholdSupported", "()Z"),
    abstract_method("getUsageThreshold", "()J"),
    abstract_method("setUsageThreshold", "(J)V"),
    abstract_method("isUsageThresholdExceeded", "()Z"),
    abstract_method("isCollectionUsageThresholdSupported", "()Z"),
    abstract_method("getCollectionUsageThreshold", "()J"),
];

pub(crate) const MEMORY_POOL_MX_BEAN_DEF: oxjvm_vm::NativeClass = class(
    "java/lang/management/MemoryPoolMXBean",
    Some("java/lang/Object"),
    &["java/lang/management/PlatformManagedObject"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &MEMORY_POOL_MX_BEAN_METHODS,
    None,
);

fn memory_type_value(vm: &mut Vm<'_>, name: &str) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/lang/management/MemoryType")?;
    vm.ensure_initialized(class)?;
    Ok(vm
        .read_static_ref(class, name, "Ljava/lang/management/MemoryType;")
        .unwrap_or(ObjectRef::NULL))
}

const OJVM_MEMORY_POOL_MX_BEAN_METHODS: [oxjvm_vm::NativeMethodDef; 16] = [
    method(
        "getObjectName",
        "()Ljavax/management/ObjectName;",
        ACC_PUBLIC,
        |vm, _, _| new_object_name(vm, "java.lang:type=MemoryPool,name=G1 Eden Space"),
    ),
    method("getName", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, _| {
        object(vm.make_string("G1 Eden Space")?)
    }),
    method(
        "getType",
        "()Ljava/lang/management/MemoryType;",
        ACC_PUBLIC,
        |vm, _, _| object(memory_type_value(vm, "HEAP")?),
    ),
    method(
        "getUsage",
        "()Ljava/lang/management/MemoryUsage;",
        ACC_PUBLIC,
        |vm, _, _| {
            let used = vm.heap_used_bytes() as i64;
            let committed = vm.heap_total_bytes() as i64;
            let max = vm.heap_max_bytes().map_or(-1, |value| value as i64);
            object(new_usage(vm, 0, used, committed, max)?)
        },
    ),
    method(
        "getPeakUsage",
        "()Ljava/lang/management/MemoryUsage;",
        ACC_PUBLIC,
        |vm, _, _| {
            let used = vm.heap_used_bytes() as i64;
            let committed = vm.heap_total_bytes() as i64;
            let max = vm.heap_max_bytes().map_or(-1, |value| value as i64);
            object(new_usage(vm, 0, used, committed, max)?)
        },
    ),
    method("resetPeakUsage", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "getCollectionUsage",
        "()Ljava/lang/management/MemoryUsage;",
        ACC_PUBLIC,
        |vm, _, _| object(new_usage(vm, 0, 0, 0, -1)?),
    ),
    method("isValid", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
    method(
        "getMemoryManagerNames",
        "()[Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(new_string_array(vm, &["G1 Young Generation Manager"])?),
    ),
    method("isUsageThresholdSupported", "()Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
    method("getUsageThreshold", "()J", ACC_PUBLIC, |_, _, _| long(0)),
    method("setUsageThreshold", "(J)V", ACC_PUBLIC, |_, _, _| void()),
    method("isUsageThresholdExceeded", "()Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
    method(
        "isCollectionUsageThresholdSupported",
        "()Z",
        ACC_PUBLIC,
        |_, _, _| boolean(false),
    ),
    method(
        "getCollectionUsageThreshold",
        "()J",
        ACC_PUBLIC,
        |_, _, _| long(0),
    ),
    method(
        "setCollectionUsageThreshold",
        "(J)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
];

pub(crate) const OJVM_MEMORY_POOL_MX_BEAN: oxjvm_vm::NativeClass = class(
    "java/lang/management/OjvmMemoryPoolMXBean",
    Some("java/lang/Object"),
    &["java/lang/management/MemoryPoolMXBean"],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &OJVM_MEMORY_POOL_MX_BEAN_METHODS,
    None,
);

const HOTSPOT_MX_BEAN_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    abstract_method("getDiagnosticOptions", "()Ljava/util/List;"),
    abstract_method(
        "getVMOption",
        "(Ljava/lang/String;)Lcom/sun/management/VMOption;",
    ),
    abstract_method("setVMOption", "(Ljava/lang/String;Ljava/lang/String;)V"),
    abstract_method("dumpHeap", "(Ljava/lang/String;Z)V"),
];

pub(crate) const HOTSPOT_MX_BEAN_DEF: oxjvm_vm::NativeClass = class(
    "com/sun/management/HotSpotDiagnosticMXBean",
    Some("java/lang/Object"),
    &["java/lang/management/PlatformManagedObject"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &HOTSPOT_MX_BEAN_METHODS,
    None,
);

fn new_vm_option(vm: &mut Vm<'_>, name: &str, value: &str) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("com/sun/management/VMOption")?;
    let option = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(
            class,
            "<init>",
            "(Ljava/lang/String;Ljava/lang/String;ZLjava/lang/String;)V",
        )
        .ok_or_else(|| VmError::internal("VMOption lacks <init>"))?;
    let name = vm.make_string(name)?;
    let value = vm.make_string(value)?;
    let origin = vm.make_string("default")?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![
            Value::Ref(option),
            Value::Ref(name),
            Value::Ref(value),
            Value::Int(0),
            Value::Ref(origin),
        ],
    )?;
    Ok(option)
}

const OJVM_HOTSPOT_MX_BEAN_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "getObjectName",
        "()Ljavax/management/ObjectName;",
        ACC_PUBLIC,
        |vm, _, _| new_object_name(vm, "com.sun.management:type=HotSpotDiagnostic"),
    ),
    method(
        "getDiagnosticOptions",
        "()Ljava/util/List;",
        ACC_PUBLIC,
        |vm, _, _| {
            let list = new_array_list(vm)?;
            let option = new_vm_option(vm, "HeapDumpOnOutOfMemoryError", "false")?;
            list_add(vm, list, option)?;
            object(list)
        },
    ),
    method(
        "getVMOption",
        "(Ljava/lang/String;)Lcom/sun/management/VMOption;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = vm.string_or_empty(ref_arg(args, 1));
            object(new_vm_option(vm, &name, "")?)
        },
    ),
    method(
        "setVMOption",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "dumpHeap",
        "(Ljava/lang/String;Z)V",
        ACC_PUBLIC,
        |vm, _, _| {
            Err(vm.throw_new(
                "java/io/IOException",
                Some("heap dumping is not implemented"),
            ))
        },
    ),
];

pub(crate) const OJVM_HOTSPOT_MX_BEAN: oxjvm_vm::NativeClass = class(
    "java/lang/management/OjvmHotSpotDiagnosticMXBean",
    Some("java/lang/Object"),
    &["com/sun/management/HotSpotDiagnosticMXBean"],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &OJVM_HOTSPOT_MX_BEAN_METHODS,
    None,
);

const COM_SUN_RUNTIME_MX_BEAN_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    abstract_method("getPid", "()J"),
    abstract_method("getProcessId", "()J"),
];

pub(crate) const COM_SUN_RUNTIME_MX_BEAN_DEF: oxjvm_vm::NativeClass = class(
    "com/sun/management/RuntimeMXBean",
    Some("java/lang/Object"),
    &["java/lang/management/RuntimeMXBean"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &COM_SUN_RUNTIME_MX_BEAN_METHODS,
    None,
);

const COM_SUN_THREAD_MX_BEAN_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    abstract_method("getThreadAllocatedBytes", "(J)J"),
    abstract_method("getThreadAllocatedBytes", "([J)[J"),
    abstract_method("isThreadAllocatedMemorySupported", "()Z"),
    abstract_method("isThreadAllocatedMemoryEnabled", "()Z"),
    abstract_method("setThreadAllocatedMemoryEnabled", "(Z)V"),
];

pub(crate) const COM_SUN_THREAD_MX_BEAN_DEF: oxjvm_vm::NativeClass = class(
    "com/sun/management/ThreadMXBean",
    Some("java/lang/Object"),
    &["java/lang/management/ThreadMXBean"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &COM_SUN_THREAD_MX_BEAN_METHODS,
    None,
);

const COM_SUN_GC_MX_BEAN_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [abstract_method(
    "getLastGcInfo",
    "()Lcom/sun/management/GcInfo;",
)];

pub(crate) const COM_SUN_GC_MX_BEAN_DEF: oxjvm_vm::NativeClass = class(
    "com/sun/management/GarbageCollectorMXBean",
    Some("java/lang/Object"),
    &["java/lang/management/GarbageCollectorMXBean"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &COM_SUN_GC_MX_BEAN_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// javax.management: ObjectName / ObjectInstance / Attribute / MBeanInfo
// -------------------------------------------------------------------------------------------

const OBJECT_NAME_METHODS: [oxjvm_vm::NativeMethodDef; 13] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = ref_arg(args, 1);
            if text.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let canonical = vm.string_or_empty(text);
            let (domain, keys) = match canonical.split_once(':') {
                Some((domain, keys)) => (domain.to_string(), keys.to_string()),
                None => {
                    return Err(vm.throw_new(
                        "javax/management/MalformedObjectNameException",
                        Some("missing domain"),
                    ));
                }
            };
            if domain.is_empty() {
                return Err(vm.throw_new(
                    "javax/management/MalformedObjectNameException",
                    Some("empty domain"),
                ));
            }
            let this = receiver(args);
            set_string_field_value(vm, this, "canonicalName", &canonical)?;
            set_string_field_value(vm, this, "domain", &domain)?;
            set_string_field_value(vm, this, "keyList", &keys)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let domain = vm.string_arg_value(args[1])?;
            let key = vm.string_arg_value(args[2])?;
            let value = vm.string_arg_value(args[3])?;
            let keys = alloc::format!("{key}={value}");
            let canonical = alloc::format!("{domain}:{keys}");
            let this = receiver(args);
            set_string_field_value(vm, this, "canonicalName", &canonical)?;
            set_string_field_value(vm, this, "domain", &domain)?;
            set_string_field_value(vm, this, "keyList", &keys)?;
            void()
        },
    ),
    method(
        "getCanonicalName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "canonicalName",
                "Ljava/lang/String;",
            ))
        },
    ),
    method(
        "getDomain",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "domain",
                "Ljava/lang/String;",
            ))
        },
    ),
    method(
        "getKeyPropertyListString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "keyList",
                "Ljava/lang/String;",
            ))
        },
    ),
    method(
        "getKeyProperty",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let key = vm.string_or_empty(ref_arg(args, 1));
            let keys = vm
                .read_string_field_any(receiver(args), "keyList")
                .unwrap_or_default();
            for pair in keys.split(',') {
                if let Some((name, value)) = pair.split_once('=') {
                    if name.trim() == key {
                        return object(vm.make_string(value.trim())?);
                    }
                }
            }
            object(ObjectRef::NULL)
        },
    ),
    method("isPattern", "()Z", ACC_PUBLIC, |vm, _, args| {
        let text = vm
            .read_string_field_any(receiver(args), "canonicalName")
            .unwrap_or_default();
        boolean(text.contains('*') || text.contains('?'))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "canonicalName",
                "Ljava/lang/String;",
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
            boolean(
                vm.read_string_field_any(this, "canonicalName")
                    == vm.read_string_field_any(other, "canonicalName"),
            )
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let text = vm
            .read_string_field_any(receiver(args), "canonicalName")
            .unwrap_or_default();
        let mut hash: i32 = 0;
        for unit in text.encode_utf16() {
            hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
        }
        int(hash)
    }),
    method(
        "getInstance",
        "(Ljava/lang/String;)Ljavax/management/ObjectName;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = ref_arg(args, 0);
            if text.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let canonical = vm.string_or_empty(text);
            let (domain, keys) = match canonical.split_once(':') {
                Some((domain, keys)) => (domain.to_string(), keys.to_string()),
                None => {
                    return Err(vm.throw_new(
                        "javax/management/MalformedObjectNameException",
                        Some("missing domain"),
                    ));
                }
            };
            let class = vm.resolve_class("javax/management/ObjectName")?;
            let instance = vm.new_instance(class)?;
            set_string_field_value(vm, instance, "canonicalName", &canonical)?;
            set_string_field_value(vm, instance, "domain", &domain)?;
            set_string_field_value(vm, instance, "keyList", &keys)?;
            object(instance)
        },
    ),
    method(
        "quote",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 0));
            let quoted = alloc::format!("\"{}\"", text.replace('"', "\\\""));
            object(vm.make_string(&quoted)?)
        },
    ),
    method(
        "unquote",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 0));
            let unquoted = text.trim_matches('"').replace("\\\"", "\"");
            object(vm.make_string(&unquoted)?)
        },
    ),
];

fn set_string_field_value(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: &str,
    value: &str,
) -> Result<(), VmError> {
    let class = vm.class_of(this);
    vm.set_string_field(this, class, name, value)
}

trait StringArgValue {
    fn string_arg_value(&self, value: Value) -> Result<String, VmError>;
}

impl StringArgValue for Vm<'_> {
    fn string_arg_value(&self, value: Value) -> Result<String, VmError> {
        self.string_value(value.as_ref())
            .ok_or_else(|| VmError::internal("string argument expected"))
    }
}

pub(crate) const OBJECT_NAME: oxjvm_vm::NativeClass = class(
    "javax/management/ObjectName",
    Some("java/lang/Object"),
    &["javax/management/QueryExp", "java/io/Serializable"],
    ACC_PUBLIC,
    &[
        field("canonicalName", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("domain", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("keyList", "Ljava/lang/String;", ACC_PRIVATE, None),
        field(
            "WILDCARD",
            "Ljavax/management/ObjectName;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &OBJECT_NAME_METHODS,
    Some(|vm| {
        let class = vm.resolve_class("javax/management/ObjectName")?;
        let instance = vm.new_instance(class)?;
        vm.set_string_field(instance, class, "canonicalName", "*:*")?;
        vm.set_string_field(instance, class, "domain", "*")?;
        vm.set_string_field(instance, class, "keyList", "*")?;
        vm.set_static_value(
            class,
            "WILDCARD",
            "Ljavax/management/ObjectName;",
            Value::Ref(instance),
        );
        Ok(())
    }),
);

const OBJECT_INSTANCE_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "<init>",
        "(Ljavax/management/ObjectName;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            set_ref_field(
                vm,
                this,
                "name",
                "Ljavax/management/ObjectName;",
                ref_arg(args, 1),
            )?;
            set_ref_field(
                vm,
                this,
                "className",
                "Ljava/lang/String;",
                ref_arg(args, 2),
            )?;
            void()
        },
    ),
    method(
        "getObjectName",
        "()Ljavax/management/ObjectName;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "name",
                "Ljavax/management/ObjectName;",
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
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name = ref_field(vm, this, "name", "Ljavax/management/ObjectName;");
            let name_text = if name.is_null() {
                String::new()
            } else {
                vm.read_string_field_any(name, "canonicalName")
                    .unwrap_or_default()
            };
            object(vm.make_string(&name_text)?)
        },
    ),
];

pub(crate) const OBJECT_INSTANCE: oxjvm_vm::NativeClass = class(
    "javax/management/ObjectInstance",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[
        field("name", "Ljavax/management/ObjectName;", ACC_PRIVATE, None),
        field("className", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &OBJECT_INSTANCE_METHODS,
    None,
);

const ATTRIBUTE_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            set_ref_field(vm, this, "name", "Ljava/lang/String;", ref_arg(args, 1))?;
            set_ref_field(vm, this, "value", "Ljava/lang/Object;", ref_arg(args, 2))?;
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
        "getValue",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "value", "Ljava/lang/Object;")),
    ),
];

pub(crate) const ATTRIBUTE: oxjvm_vm::NativeClass = class(
    "javax/management/Attribute",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("value", "Ljava/lang/Object;", ACC_PRIVATE, None),
    ],
    &ATTRIBUTE_METHODS,
    None,
);

const ATTRIBUTE_LIST_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "add",
        "(Ljavax/management/Attribute;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let list = receiver(args);
            let (class, method) =
                vm.resolve_virtual_method(list, "add", "(Ljava/lang/Object;)Z")?;
            vm.invoke_method(
                class,
                method,
                vec![Value::Ref(list), Value::Ref(ref_arg(args, 1))],
            )?;
            void()
        },
    ),
];

pub(crate) const ATTRIBUTE_LIST: oxjvm_vm::NativeClass = class(
    "javax/management/AttributeList",
    Some("java/util/ArrayList"),
    &[],
    ACC_PUBLIC,
    &[],
    &ATTRIBUTE_LIST_METHODS,
    None,
);

const MBEAN_INFO_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;[Ljavax/management/MBeanAttributeInfo;[Ljavax/management/MBeanOperationInfo;[Ljavax/management/MBeanConstructorInfo;[Ljavax/management/MBeanNotificationInfo;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            set_ref_field(
                vm,
                this,
                "className",
                "Ljava/lang/String;",
                ref_arg(args, 1),
            )?;
            set_ref_field(
                vm,
                this,
                "description",
                "Ljava/lang/String;",
                ref_arg(args, 2),
            )?;
            void()
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
        "getDescription",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "description",
                "Ljava/lang/String;",
            ))
        },
    ),
    method(
        "getAttributes",
        "()[Ljavax/management/MBeanAttributeInfo;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getOperations",
        "()[Ljavax/management/MBeanOperationInfo;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
];

pub(crate) const MBEAN_INFO: oxjvm_vm::NativeClass = class(
    "javax/management/MBeanInfo",
    Some("java/lang/Object"),
    &["java/io/Serializable", "javax/management/DescriptorRead"],
    ACC_PUBLIC,
    &[
        field("className", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("description", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &MBEAN_INFO_METHODS,
    None,
);

pub(crate) const MBEAN_ATTRIBUTE_INFO: oxjvm_vm::NativeClass = class(
    "javax/management/MBeanAttributeInfo",
    Some("javax/management/MBeanFeatureInfo"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
pub(crate) const MBEAN_FEATURE_INFO: oxjvm_vm::NativeClass = class(
    "javax/management/MBeanFeatureInfo",
    Some("java/lang/Object"),
    &["java/io/Serializable", "javax/management/DescriptorRead"],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
pub(crate) const MBEAN_OPERATION_INFO: oxjvm_vm::NativeClass = class(
    "javax/management/MBeanOperationInfo",
    Some("javax/management/MBeanFeatureInfo"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
pub(crate) const MBEAN_PARAMETER_INFO: oxjvm_vm::NativeClass = class(
    "javax/management/MBeanParameterInfo",
    Some("javax/management/MBeanFeatureInfo"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

pub(crate) const MBEAN_CONSTRUCTOR_INFO: oxjvm_vm::NativeClass = class(
    "javax/management/MBeanConstructorInfo",
    Some("javax/management/MBeanFeatureInfo"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

pub(crate) const MBEAN_NOTIFICATION_INFO: oxjvm_vm::NativeClass = class(
    "javax/management/MBeanNotificationInfo",
    Some("javax/management/MBeanFeatureInfo"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);

pub(crate) const NOTIFICATION_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/Object;J)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            set_ref_field(vm, this, "type", "Ljava/lang/String;", ref_arg(args, 1))?;
            set_ref_field(vm, this, "source", "Ljava/lang/Object;", ref_arg(args, 2))?;
            set_long_field(vm, this, "timeStamp", args[3].as_long())?;
            void()
        },
    ),
    method(
        "getType",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(ref_field(vm, receiver(args), "type", "Ljava/lang/String;")),
    ),
    method(
        "getMessage",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method("getTimeStamp", "()J", ACC_PUBLIC, |vm, _, args| {
        long(long_field(vm, receiver(args), "timeStamp"))
    }),
    method(
        "getUserData",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
];

pub(crate) const NOTIFICATION: oxjvm_vm::NativeClass = class(
    "javax/management/Notification",
    Some("java/util/EventObject"),
    &[],
    ACC_PUBLIC,
    &[
        field("type", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("timeStamp", "J", ACC_PRIVATE, None),
    ],
    &NOTIFICATION_METHODS,
    None,
);

pub(crate) const NOTIFICATION_BROADCASTER_SUPPORT: oxjvm_vm::NativeClass = class(
    "javax/management/NotificationBroadcasterSupport",
    Some("java/lang/Object"),
    &["javax/management/NotificationEmitter"],
    ACC_PUBLIC,
    &[],
    &[
        method(
            "addNotificationListener",
            "(Ljavax/management/NotificationListener;Ljavax/management/NotificationFilter;Ljava/lang/Object;)V",
            ACC_PUBLIC,
            |_, _, _| void(),
        ),
        method(
            "removeNotificationListener",
            "(Ljavax/management/NotificationListener;)V",
            ACC_PUBLIC,
            |_, _, _| void(),
        ),
        method(
            "sendNotification",
            "(Ljavax/management/Notification;)V",
            ACC_PUBLIC,
            |_, _, _| void(),
        ),
    ],
    None,
);

// -------------------------------------------------------------------------------------------
// javax.management.MBeanServer
// -------------------------------------------------------------------------------------------

const MBEAN_SERVER_METHODS: [oxjvm_vm::NativeMethodDef; 21] = [
    abstract_method(
        "createMBean",
        "(Ljava/lang/String;Ljavax/management/ObjectName;)Ljavax/management/ObjectInstance;",
    ),
    abstract_method(
        "registerMBean",
        "(Ljava/lang/Object;Ljavax/management/ObjectName;)Ljavax/management/ObjectInstance;",
    ),
    abstract_method("unregisterMBean", "(Ljavax/management/ObjectName;)V"),
    abstract_method(
        "getObjectInstance",
        "(Ljavax/management/ObjectName;)Ljavax/management/ObjectInstance;",
    ),
    abstract_method(
        "queryMBeans",
        "(Ljavax/management/ObjectName;Ljavax/management/QueryExp;)Ljava/util/Set;",
    ),
    abstract_method(
        "queryNames",
        "(Ljavax/management/ObjectName;Ljavax/management/QueryExp;)Ljava/util/Set;",
    ),
    abstract_method("isRegistered", "(Ljavax/management/ObjectName;)Z"),
    abstract_method("getMBeanCount", "()Ljava/lang/Integer;"),
    abstract_method(
        "getMBeanInfo",
        "(Ljavax/management/ObjectName;)Ljavax/management/MBeanInfo;",
    ),
    abstract_method(
        "getAttribute",
        "(Ljavax/management/ObjectName;Ljava/lang/String;)Ljava/lang/Object;",
    ),
    abstract_method(
        "setAttribute",
        "(Ljavax/management/ObjectName;Ljavax/management/Attribute;)V",
    ),
    abstract_method(
        "getAttributes",
        "(Ljavax/management/ObjectName;[Ljava/lang/String;)Ljavax/management/AttributeList;",
    ),
    abstract_method(
        "setAttributes",
        "(Ljavax/management/ObjectName;Ljavax/management/AttributeList;)Ljavax/management/AttributeList;",
    ),
    abstract_method(
        "invoke",
        "(Ljavax/management/ObjectName;Ljava/lang/String;[Ljava/lang/Object;[Ljava/lang/String;)Ljava/lang/Object;",
    ),
    abstract_method("getDefaultDomain", "()Ljava/lang/String;"),
    abstract_method("getDomains", "()[Ljava/lang/String;"),
    abstract_method(
        "addNotificationListener",
        "(Ljavax/management/ObjectName;Ljavax/management/NotificationListener;Ljavax/management/NotificationFilter;Ljava/lang/Object;)V",
    ),
    abstract_method(
        "removeNotificationListener",
        "(Ljavax/management/ObjectName;Ljavax/management/NotificationListener;)V",
    ),
    abstract_method(
        "getClassLoaderFor",
        "(Ljavax/management/ObjectName;)Ljava/lang/ClassLoader;",
    ),
    abstract_method("instantiate", "(Ljava/lang/String;)Ljava/lang/Object;"),
    abstract_method(
        "getClassLoaderRepository",
        "()Ljavax/management/loading/ClassLoaderRepository;",
    ),
];

pub(crate) const MBEAN_SERVER: oxjvm_vm::NativeClass = class(
    "javax/management/MBeanServer",
    Some("java/lang/Object"),
    &[
        "javax/management/MBeanServerConnection",
        "javax/management/MBeanServerRegistrationUtility",
    ],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &MBEAN_SERVER_METHODS,
    None,
);

pub(crate) const MBEAN_SERVER_CONNECTION: oxjvm_vm::NativeClass = class(
    "javax/management/MBeanServerConnection",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &MBEAN_SERVER_METHODS,
    None,
);

pub(crate) const MBEAN_SERVER_REGISTRATION_UTILITY: oxjvm_vm::NativeClass = class(
    "javax/management/MBeanServerRegistrationUtility",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

fn object_name_text(vm: &Vm<'_>, name: ObjectRef) -> String {
    if name.is_null() {
        return String::new();
    }
    vm.read_string_field_any(name, "canonicalName")
        .unwrap_or_default()
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" || pattern == "?" {
        return true;
    }
    if !pattern.contains('*') && !pattern.contains('?') {
        return pattern == text;
    }
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut star_text = 0usize;
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            star_text = t;
            p += 1;
        } else if let Some(star_pos) = star {
            p = star_pos + 1;
            star_text += 1;
            t = star_text;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

fn object_name_matches(pattern: &str, actual: &str) -> bool {
    let (pattern_domain, pattern_keys) = match pattern.split_once(':') {
        Some(parts) => parts,
        None => return pattern == actual,
    };
    let (actual_domain, actual_keys) = match actual.split_once(':') {
        Some(parts) => parts,
        None => return false,
    };
    if pattern_domain != "*" && pattern_domain != actual_domain {
        return false;
    }
    for pair in pattern_keys.split(',') {
        let pair = pair.trim();
        if pair.is_empty() || pair == "*" {
            continue;
        }
        let (key, value) = match pair.split_once('=') {
            Some(parts) => parts,
            None => return false,
        };
        let mut found = false;
        for actual_pair in actual_keys.split(',') {
            if let Some((actual_key, actual_value)) = actual_pair.trim().split_once('=') {
                if actual_key == key && wildcard_match(value, actual_value) {
                    found = true;
                    break;
                }
            }
        }
        if !found {
            return false;
        }
    }
    true
}

const OJVM_MBEAN_SERVER_METHODS: [oxjvm_vm::NativeMethodDef; 21] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let entries = new_hash_map(vm)?;
        set_ref_field(vm, this, "entries", "Ljava/util/HashMap;", entries)?;
        void()
    }),
    method(
        "registerMBean",
        "(Ljava/lang/Object;Ljavax/management/ObjectName;)Ljavax/management/ObjectInstance;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let target = ref_arg(args, 1);
            let name = ref_arg(args, 2);
            if target.is_null() || name.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let entries = ref_field(vm, this, "entries", "Ljava/util/HashMap;");
            map_put(vm, entries, name, target)?;
            let (get_class, get_class_method) =
                vm.resolve_virtual_method(target, "getClass", "()Ljava/lang/Class;")?;
            let class_ref =
                vm.invoke_method(get_class, get_class_method, vec![Value::Ref(target)])?;
            let (nclass, nmethod) =
                vm.resolve_virtual_method(class_ref.as_ref(), "getName", "()Ljava/lang/String;")?;
            let text = vm.invoke_method(nclass, nmethod, vec![Value::Ref(class_ref.as_ref())])?;
            let instance_class = vm.resolve_class("javax/management/ObjectInstance")?;
            let instance = vm.new_instance(instance_class)?;
            let (declaring, ctor) = vm
                .find_method(
                    instance_class,
                    "<init>",
                    "(Ljavax/management/ObjectName;Ljava/lang/String;)V",
                )
                .ok_or_else(|| VmError::internal("ObjectInstance ctor"))?;
            vm.invoke_method(
                declaring,
                ctor,
                vec![Value::Ref(instance), Value::Ref(name), text],
            )?;
            object(instance)
        },
    ),
    method(
        "unregisterMBean",
        "(Ljavax/management/ObjectName;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let entries = ref_field(vm, this, "entries", "Ljava/util/HashMap;");
            map_remove(vm, entries, ref_arg(args, 1))?;
            void()
        },
    ),
    method(
        "getObjectInstance",
        "(Ljavax/management/ObjectName;)Ljavax/management/ObjectInstance;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name = ref_arg(args, 1);
            let entries = ref_field(vm, this, "entries", "Ljava/util/HashMap;");
            let target = map_get(vm, entries, name)?;
            if target.is_null() {
                return object(ObjectRef::NULL);
            }
            let (nclass, nmethod) =
                vm.resolve_virtual_method(target, "getClass", "()Ljava/lang/Class;")?;
            let class_ref = vm.invoke_method(nclass, nmethod, vec![Value::Ref(target)])?;
            let (cc, cm) =
                vm.resolve_virtual_method(class_ref.as_ref(), "getName", "()Ljava/lang/String;")?;
            let text = vm.invoke_method(cc, cm, vec![Value::Ref(class_ref.as_ref())])?;
            let instance_class = vm.resolve_class("javax/management/ObjectInstance")?;
            let instance = vm.new_instance(instance_class)?;
            let (declaring, ctor) = vm
                .find_method(
                    instance_class,
                    "<init>",
                    "(Ljavax/management/ObjectName;Ljava/lang/String;)V",
                )
                .ok_or_else(|| VmError::internal("ObjectInstance ctor"))?;
            vm.invoke_method(
                declaring,
                ctor,
                vec![Value::Ref(instance), Value::Ref(name), text],
            )?;
            object(instance)
        },
    ),
    method(
        "isRegistered",
        "(Ljavax/management/ObjectName;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let entries = ref_field(vm, this, "entries", "Ljava/util/HashMap;");
            boolean(map_contains_key(vm, entries, ref_arg(args, 1))?)
        },
    ),
    method(
        "getMBeanCount",
        "()Ljava/lang/Integer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let entries = ref_field(vm, this, "entries", "Ljava/util/HashMap;");
            let count = map_size(vm, entries)?;
            crate::boxed::boxed_int(vm, count)
        },
    ),
    method(
        "queryNames",
        "(Ljavax/management/ObjectName;Ljavax/management/QueryExp;)Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let pattern_ref = ref_arg(args, 1);
            let entries = ref_field(vm, this, "entries", "Ljava/util/HashMap;");
            let set = new_hash_set(vm)?;
            if pattern_ref.is_null() {
                let (class, method) =
                    vm.resolve_virtual_method(entries, "keySet", "()Ljava/util/Set;")?;
                return vm.invoke_method(class, method, vec![Value::Ref(entries)]);
            }
            let pattern = object_name_text(vm, pattern_ref);
            let (class, method) =
                vm.resolve_virtual_method(entries, "keySet", "()Ljava/util/Set;")?;
            let keys = vm
                .invoke_method(class, method, vec![Value::Ref(entries)])?
                .as_ref();
            let (iclass, it) =
                vm.resolve_virtual_method(keys, "iterator", "()Ljava/util/Iterator;")?;
            let iterator = vm
                .invoke_method(iclass, it, vec![Value::Ref(keys)])?
                .as_ref();
            loop {
                let (hc, hm) = vm.resolve_virtual_method(iterator, "hasNext", "()Z")?;
                if vm
                    .invoke_method(hc, hm, vec![Value::Ref(iterator)])?
                    .as_int()
                    == 0
                {
                    break;
                }
                let (nc, nm) =
                    vm.resolve_virtual_method(iterator, "next", "()Ljava/lang/Object;")?;
                let name = vm
                    .invoke_method(nc, nm, vec![Value::Ref(iterator)])?
                    .as_ref();
                if object_name_matches(&pattern, &object_name_text(vm, name)) {
                    set_add(vm, set, name)?;
                }
            }
            object(set)
        },
    ),
    method(
        "queryMBeans",
        "(Ljavax/management/ObjectName;Ljavax/management/QueryExp;)Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let entries = ref_field(vm, this, "entries", "Ljava/util/HashMap;");
            let (class, method) =
                vm.resolve_virtual_method(entries, "keySet", "()Ljava/util/Set;")?;
            let keys = vm
                .invoke_method(class, method, vec![Value::Ref(entries)])?
                .as_ref();
            let set = new_hash_set(vm)?;
            let (iclass, it) =
                vm.resolve_virtual_method(keys, "iterator", "()Ljava/util/Iterator;")?;
            let iterator = vm
                .invoke_method(iclass, it, vec![Value::Ref(keys)])?
                .as_ref();
            loop {
                let (hc, hm) = vm.resolve_virtual_method(iterator, "hasNext", "()Z")?;
                if vm
                    .invoke_method(hc, hm, vec![Value::Ref(iterator)])?
                    .as_int()
                    == 0
                {
                    break;
                }
                let (nc, nm) =
                    vm.resolve_virtual_method(iterator, "next", "()Ljava/lang/Object;")?;
                let name = vm
                    .invoke_method(nc, nm, vec![Value::Ref(iterator)])?
                    .as_ref();
                let target = map_get(vm, entries, name)?;
                let (cc, cm) =
                    vm.resolve_virtual_method(target, "getClass", "()Ljava/lang/Class;")?;
                let class_ref = vm.invoke_method(cc, cm, vec![Value::Ref(target)])?;
                let (gc, gm) = vm.resolve_virtual_method(
                    class_ref.as_ref(),
                    "getName",
                    "()Ljava/lang/String;",
                )?;
                let text = vm.invoke_method(gc, gm, vec![Value::Ref(class_ref.as_ref())])?;
                let instance_class = vm.resolve_class("javax/management/ObjectInstance")?;
                let instance = vm.new_instance(instance_class)?;
                let (declaring, ctor) = vm
                    .find_method(
                        instance_class,
                        "<init>",
                        "(Ljavax/management/ObjectName;Ljava/lang/String;)V",
                    )
                    .ok_or_else(|| VmError::internal("ObjectInstance ctor"))?;
                vm.invoke_method(
                    declaring,
                    ctor,
                    vec![Value::Ref(instance), Value::Ref(name), text],
                )?;
                set_add(vm, set, instance)?;
            }
            object(set)
        },
    ),
    method(
        "getMBeanInfo",
        "(Ljavax/management/ObjectName;)Ljavax/management/MBeanInfo;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("javax/management/MBeanInfo")?;
            let instance = vm.new_instance(class)?;
            let (declaring, ctor) = vm
                .find_method(
                    class,
                    "<init>",
                    "(Ljava/lang/String;Ljava/lang/String;[Ljavax/management/MBeanAttributeInfo;[Ljavax/management/MBeanOperationInfo;[Ljavax/management/MBeanConstructorInfo;[Ljavax/management/MBeanNotificationInfo;)V",
                )
                .ok_or_else(|| VmError::internal("MBeanInfo ctor"))?;
            let name = vm.make_string("java.lang.Object")?;
            let description = vm.make_string("")?;
            vm.invoke_method(
                declaring,
                ctor,
                vec![
                    Value::Ref(instance),
                    Value::Ref(name),
                    Value::Ref(description),
                    Value::Ref(ObjectRef::NULL),
                    Value::Ref(ObjectRef::NULL),
                    Value::Ref(ObjectRef::NULL),
                    Value::Ref(ObjectRef::NULL),
                ],
            )?;
            object(instance)
        },
    ),
    method(
        "getAttribute",
        "(Ljavax/management/ObjectName;Ljava/lang/String;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "setAttribute",
        "(Ljavax/management/ObjectName;Ljavax/management/Attribute;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "getAttributes",
        "(Ljavax/management/ObjectName;[Ljava/lang/String;)Ljavax/management/AttributeList;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("javax/management/AttributeList")?;
            object(vm.new_instance(class)?)
        },
    ),
    method(
        "setAttributes",
        "(Ljavax/management/ObjectName;Ljavax/management/AttributeList;)Ljavax/management/AttributeList;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("javax/management/AttributeList")?;
            object(vm.new_instance(class)?)
        },
    ),
    method(
        "invoke",
        "(Ljavax/management/ObjectName;Ljava/lang/String;[Ljava/lang/Object;[Ljava/lang/String;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getDefaultDomain",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("DefaultDomain")?),
    ),
    method(
        "getDomains",
        "()[Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(new_string_array(vm, &["DefaultDomain"])?),
    ),
    method(
        "addNotificationListener",
        "(Ljavax/management/ObjectName;Ljavax/management/NotificationListener;Ljavax/management/NotificationFilter;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "removeNotificationListener",
        "(Ljavax/management/ObjectName;Ljavax/management/NotificationListener;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "getClassLoaderFor",
        "(Ljavax/management/ObjectName;)Ljava/lang/ClassLoader;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "instantiate",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getClassLoaderRepository",
        "()Ljavax/management/loading/ClassLoaderRepository;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
];

pub(crate) const OJVM_MBEAN_SERVER: oxjvm_vm::NativeClass = class(
    "javax/management/OjvmMBeanServer",
    Some("java/lang/Object"),
    &["javax/management/MBeanServer"],
    ACC_PUBLIC | ACC_FINAL,
    &[field("entries", "Ljava/util/HashMap;", ACC_PRIVATE, None)],
    &OJVM_MBEAN_SERVER_METHODS,
    None,
);

const MBEAN_SERVER_FACTORY_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "createMBeanServer",
        "()Ljavax/management/MBeanServer;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_instance(vm, "javax/management/OjvmMBeanServer")?),
    ),
    method(
        "createMBeanServer",
        "(Ljava/lang/String;)Ljavax/management/MBeanServer;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_instance(vm, "javax/management/OjvmMBeanServer")?),
    ),
    method(
        "newMBeanServer",
        "()Ljavax/management/MBeanServer;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_instance(vm, "javax/management/OjvmMBeanServer")?),
    ),
    method(
        "newMBeanServer",
        "(Ljava/lang/String;)Ljavax/management/MBeanServer;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_instance(vm, "javax/management/OjvmMBeanServer")?),
    ),
    method(
        "releaseMBeanServer",
        "(Ljavax/management/MBeanServer;)V",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| void(),
    ),
    method(
        "findMBeanServer",
        "(Ljava/lang/String;)Ljava/util/ArrayList;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_array_list(vm)?),
    ),
    method(
        "getClassLoaderRepository",
        "(Ljavax/management/MBeanServer;)Ljavax/management/loading/ClassLoaderRepository;",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
];

pub(crate) const MBEAN_SERVER_FACTORY: oxjvm_vm::NativeClass = class(
    "javax/management/MBeanServerFactory",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &MBEAN_SERVER_FACTORY_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Remaining javax.management shells
// -------------------------------------------------------------------------------------------

pub(crate) const STANDARD_MBEAN: oxjvm_vm::NativeClass = class(
    "javax/management/StandardMBean",
    Some("java/lang/Object"),
    &[
        "javax/management/DynamicMBean",
        "javax/management/MBeanRegistration",
    ],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
pub(crate) const DYNAMIC_MBEAN: oxjvm_vm::NativeClass = class(
    "javax/management/DynamicMBean",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);
pub(crate) const MBEAN_REGISTRATION: oxjvm_vm::NativeClass = class(
    "javax/management/MBeanRegistration",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);
pub(crate) const INTROSPECTION_EXCEPTION: oxjvm_vm::NativeClass = class(
    "javax/management/IntrospectionException",
    Some("java/lang/Exception"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
pub(crate) const MALFORMED_OBJECT_NAME_EXCEPTION: oxjvm_vm::NativeClass = class(
    "javax/management/MalformedObjectNameException",
    Some("java/lang/Exception"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
pub(crate) const INSTANCE_NOT_FOUND_EXCEPTION: oxjvm_vm::NativeClass = class(
    "javax/management/InstanceNotFoundException",
    Some("javax/management/OperationsException"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
pub(crate) const OPERATIONS_EXCEPTION: oxjvm_vm::NativeClass = class(
    "javax/management/OperationsException",
    Some("javax/management/JMException"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
pub(crate) const JM_EXCEPTION: oxjvm_vm::NativeClass = class(
    "javax/management/JMException",
    Some("java/lang/Exception"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
pub(crate) const REFLECTION_EXCEPTION: oxjvm_vm::NativeClass = class(
    "javax/management/ReflectionException",
    Some("javax/management/JMException"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
pub(crate) const MBEAN_EXCEPTION: oxjvm_vm::NativeClass = class(
    "javax/management/MBeanException",
    Some("javax/management/JMException"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
pub(crate) const NOT_COMPREHENSIBLE_EXCEPTION: oxjvm_vm::NativeClass = class(
    "javax/management/NotCompliantMBeanException",
    Some("javax/management/OperationsException"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
pub(crate) const MONITOR: oxjvm_vm::NativeClass = class(
    "javax/management/monitor/Monitor",
    Some("javax/management/NotificationBroadcasterSupport"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &[],
    None,
);
pub(crate) const NOTIFICATION_BROADCASTER: oxjvm_vm::NativeClass = class(
    "javax/management/NotificationBroadcaster",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);
pub(crate) const NOTIFICATION_EMITTER: oxjvm_vm::NativeClass = class(
    "javax/management/NotificationEmitter",
    Some("javax/management/NotificationBroadcaster"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);
pub(crate) const NOTIFICATION_LISTENER: oxjvm_vm::NativeClass = class(
    "javax/management/NotificationListener",
    Some("java/lang/Object"),
    &["java/util/EventListener"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);
pub(crate) const NOTIFICATION_FILTER: oxjvm_vm::NativeClass = class(
    "javax/management/NotificationFilter",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);
pub(crate) const QUERY_EXP: oxjvm_vm::NativeClass = class(
    "javax/management/QueryExp",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);
pub(crate) const DESCRIPTOR: oxjvm_vm::NativeClass = class(
    "javax/management/Descriptor",
    Some("java/lang/Object"),
    &["java/io/Serializable", "java/lang/Cloneable"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);
pub(crate) const DESCRIPTOR_READ: oxjvm_vm::NativeClass = class(
    "javax/management/DescriptorRead",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);
pub(crate) const DESCRIPTOR_SUPPORT: oxjvm_vm::NativeClass = class(
    "javax/management/DescriptorSupport",
    Some("java/lang/Object"),
    &["javax/management/Descriptor"],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
pub(crate) const IMMUTABLE_DESCRIPTOR: oxjvm_vm::NativeClass = class(
    "javax/management/ImmutableDescriptor",
    Some("java/lang/Object"),
    &["javax/management/Descriptor"],
    ACC_PUBLIC | ACC_FINAL,
    &[field(
        "EMPTY_DESCRIPTOR",
        "Ljavax/management/ImmutableDescriptor;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    )],
    &[],
    Some(|vm| {
        let class = vm.resolve_class("javax/management/ImmutableDescriptor")?;
        let instance = vm.new_instance(class)?;
        vm.set_static_value(
            class,
            "EMPTY_DESCRIPTOR",
            "Ljavax/management/ImmutableDescriptor;",
            Value::Ref(instance),
        );
        Ok(())
    }),
);
pub(crate) const MXBEAN_TYPE: oxjvm_vm::NativeClass = class(
    "javax/management/MXBean",
    Some("java/lang/annotation/Annotation"),
    &[],
    ACC_PUBLIC | ACC_ANNOTATION | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);
