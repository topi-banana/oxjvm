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

const MEMORY_USAGE_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
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
        "from",
        "(Ljavax/management/openmbean/CompositeData;)Ljava/lang/management/MemoryUsage;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let cd = ref_arg(args, 0);
            if cd.is_null() {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("null CompositeData"),
                ));
            }
            memory_usage_validate(vm, cd)?;
            let init = composite_data_long(vm, cd, "init")?;
            let used = composite_data_long(vm, cd, "used")?;
            let committed = composite_data_long(vm, cd, "committed")?;
            let max = composite_data_long(vm, cd, "max")?;
            let class = vm.resolve_class("java/lang/management/MemoryUsage")?;
            let instance = vm.new_instance(class)?;
            let (declaring, ctor) = vm
                .find_method(class, "<init>", "(JJJJ)V")
                .ok_or_else(|| VmError::internal("MemoryUsage lacks <init>"))?;
            vm.invoke_method(
                declaring,
                ctor,
                vec![
                    Value::Ref(instance),
                    Value::Long(init),
                    Value::Long(used),
                    Value::Long(committed),
                    Value::Long(max),
                ],
            )?;
            object(instance)
        },
    ),
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

/// Validate that a CompositeData carries the four MemoryUsage items, each a `SimpleType.LONG`.
fn memory_usage_validate(vm: &mut Vm<'_>, cd: ObjectRef) -> Result<(), VmError> {
    let iae = |vm: &mut Vm<'_>| {
        vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("invalid MemoryUsage CompositeData"),
        )
    };
    let (ct_class, ct_method) = vm.resolve_virtual_method(
        cd,
        "getCompositeType",
        "()Ljavax/management/openmbean/CompositeType;",
    )?;
    let composite_type = vm
        .invoke_method(ct_class, ct_method, vec![Value::Ref(cd)])?
        .as_ref();
    if composite_type.is_null() {
        return Err(iae(vm));
    }
    let simple_class = vm.resolve_class("javax/management/openmbean/SimpleType")?;
    let long_type = vm
        .read_static_ref(
            simple_class,
            "LONG",
            "Ljavax/management/openmbean/SimpleType;",
        )
        .unwrap_or(ObjectRef::NULL);
    for name in ["init", "used", "committed", "max"] {
        let key = vm.make_string(name)?;
        let (get_class, get_method) = vm.resolve_virtual_method(
            composite_type,
            "getType",
            "(Ljava/lang/String;)Ljavax/management/openmbean/OpenType;",
        )?;
        let item_type = match vm.invoke_method(
            get_class,
            get_method,
            vec![Value::Ref(composite_type), Value::Ref(key)],
        ) {
            Ok(value) => value.as_ref(),
            Err(_) => return Err(iae(vm)),
        };
        if !open_equals(vm, item_type, long_type)? {
            return Err(iae(vm));
        }
    }
    Ok(())
}

/// Read a named item of a CompositeData and unbox it to a `long` (the item is a `Long`).
fn composite_data_long(vm: &mut Vm<'_>, cd: ObjectRef, name: &str) -> Result<i64, VmError> {
    let key = vm.make_string(name)?;
    let (get_class, get_method) =
        vm.resolve_virtual_method(cd, "get", "(Ljava/lang/String;)Ljava/lang/Object;")?;
    let value = vm
        .invoke_method(get_class, get_method, vec![Value::Ref(cd), Value::Ref(key)])?
        .as_ref();
    if value.is_null() {
        return Ok(-1);
    }
    let (lv_class, lv_method) = vm.resolve_virtual_method(value, "longValue", "()J")?;
    Ok(vm
        .invoke_method(lv_class, lv_method, vec![Value::Ref(value)])?
        .as_long())
}

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
const COMPOSITE_DATA_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "getCompositeType",
        "()Ljavax/management/openmbean/CompositeType;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "get",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getAll",
        "([Ljava/lang/String;)[Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "containsKey",
        "(Ljava/lang/String;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "containsValue",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "values",
        "()Ljava/util/Collection;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const COMPOSITE_DATA: oxjvm_vm::NativeClass = class(
    "javax/management/openmbean/CompositeData",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &COMPOSITE_DATA_METHODS,
    None,
);
/// Read the elements of an object array as references.
fn array_refs(vm: &mut Vm<'_>, array: ObjectRef) -> Result<Vec<ObjectRef>, VmError> {
    let length = vm.array_length(array)?;
    let mut out = Vec::with_capacity(length);
    for index in 0..length {
        out.push(vm.array_get(array, index as i32)?.as_ref());
    }
    Ok(out)
}

/// Java `String.hashCode()`.
fn java_string_hash(text: &str) -> i32 {
    let mut hash = 0i32;
    for unit in text.encode_utf16() {
        hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
    }
    hash
}

/// Deep equality for open-data values: arrays are compared element by element, other objects
/// through their `equals` method.
fn open_equals(vm: &mut Vm<'_>, left: ObjectRef, right: ObjectRef) -> Result<bool, VmError> {
    if left.is_null() || right.is_null() {
        return Ok(left.is_null() && right.is_null());
    }
    let left_len = vm.array_length(left).ok();
    let right_len = vm.array_length(right).ok();
    match (left_len, right_len) {
        (Some(n), Some(m)) => {
            if n != m {
                return Ok(false);
            }
            for index in 0..n {
                let a = vm.array_get(left, index as i32)?;
                let b = vm.array_get(right, index as i32)?;
                if !value_equals(vm, a, b)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (None, None) => {
            let (class, method) =
                vm.resolve_virtual_method(left, "equals", "(Ljava/lang/Object;)Z")?;
            Ok(vm
                .invoke_method(class, method, vec![Value::Ref(left), Value::Ref(right)])?
                .as_int()
                != 0)
        }
        _ => Ok(false),
    }
}

/// Equality of two array elements (or other slot values).
fn value_equals(vm: &mut Vm<'_>, a: Value, b: Value) -> Result<bool, VmError> {
    match (a, b) {
        (Value::Ref(x), Value::Ref(y)) => open_equals(vm, x, y),
        (Value::Int(x), Value::Int(y)) => Ok(x == y),
        (Value::Long(x), Value::Long(y)) => Ok(x == y),
        (Value::Float(x), Value::Float(y)) => Ok(x.to_bits() == y.to_bits()),
        (Value::Double(x), Value::Double(y)) => Ok(x.to_bits() == y.to_bits()),
        _ => Ok(false),
    }
}

/// Deep hash for open-data values: arrays hash their elements, other objects use `hashCode`.
fn open_hash(vm: &mut Vm<'_>, value: ObjectRef) -> Result<i32, VmError> {
    if value.is_null() {
        return Ok(0);
    }
    if let Ok(length) = vm.array_length(value) {
        let mut hash = 1i32;
        for index in 0..length {
            let element = vm.array_get(value, index as i32)?;
            hash = hash.wrapping_mul(31).wrapping_add(value_hash(vm, element)?);
        }
        return Ok(hash);
    }
    let (class, method) = vm.resolve_virtual_method(value, "hashCode", "()I")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(value)])?
        .as_int())
}

/// Hash of an array element (or other slot value).
fn value_hash(vm: &mut Vm<'_>, value: Value) -> Result<i32, VmError> {
    match value {
        Value::Ref(reference) => open_hash(vm, reference),
        Value::Int(i) => Ok(i),
        Value::Long(l) => Ok((l ^ (l >> 32)) as i32),
        Value::Float(f) => Ok(f.to_bits() as i32),
        Value::Double(d) => {
            let bits = d.to_bits();
            Ok((bits ^ (bits >> 32)) as i32)
        }
    }
}

fn composite_type_init(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    type_name: ObjectRef,
    description: ObjectRef,
    item_names: ObjectRef,
    item_descriptions: ObjectRef,
    item_types: ObjectRef,
) -> Result<(), VmError> {
    let iae =
        |vm: &mut Vm<'_>, msg: &str| vm.throw_new("java/lang/IllegalArgumentException", Some(msg));
    if type_name.is_null() || description.is_null() {
        return Err(iae(vm, "typeName or description is null"));
    }
    let type_name_text = vm.string_or_empty(type_name);
    let description_text = vm.string_or_empty(description);
    if type_name_text.is_empty() || description_text.is_empty() {
        return Err(iae(vm, "typeName or description is empty"));
    }
    if item_names.is_null() || item_descriptions.is_null() || item_types.is_null() {
        return Err(iae(vm, "item names/descriptions/types is null"));
    }
    let names = array_refs(vm, item_names)?;
    let descriptions = array_refs(vm, item_descriptions)?;
    let types = array_refs(vm, item_types)?;
    if names.is_empty() {
        return Err(iae(vm, "no items"));
    }
    if names.len() != descriptions.len() || names.len() != types.len() {
        return Err(iae(vm, "item arrays have different lengths"));
    }
    let names_list = new_array_list(vm)?;
    let types_map = new_hash_map(vm)?;
    let descriptions_map = new_hash_map(vm)?;
    for index in 0..names.len() {
        let name = names[index];
        let desc = descriptions[index];
        let open_type = types[index];
        if name.is_null() || desc.is_null() || open_type.is_null() {
            return Err(iae(vm, "null item name/description/type"));
        }
        let name_text = vm.string_or_empty(name);
        if name_text.is_empty() {
            return Err(iae(vm, "empty item name"));
        }
        if map_contains_key(vm, types_map, name)? {
            return Err(iae(vm, "duplicate item name"));
        }
        list_add(vm, names_list, name)?;
        map_put(vm, types_map, name, open_type)?;
        map_put(vm, descriptions_map, name, desc)?;
    }
    let class = vm.class_of(this);
    vm.set_string_field(this, class, "typeName", &type_name_text)?;
    vm.set_string_field(this, class, "description", &description_text)?;
    set_ref_field(vm, this, "names", "Ljava/util/List;", names_list)?;
    set_ref_field(vm, this, "types", "Ljava/util/Map;", types_map)?;
    set_ref_field(
        vm,
        this,
        "descriptions",
        "Ljava/util/Map;",
        descriptions_map,
    )?;
    Ok(())
}

/// The item names of a CompositeType, in declaration order.
fn composite_type_names(vm: &mut Vm<'_>, this: ObjectRef) -> Vec<ObjectRef> {
    let names = ref_field(vm, this, "names", "Ljava/util/List;");
    let mut out = Vec::new();
    if names.is_null() {
        return out;
    }
    if let Ok(size) = map_size(vm, names) {
        for index in 0..size {
            if let Ok((class, method)) =
                vm.resolve_virtual_method(names, "get", "(I)Ljava/lang/Object;")
            {
                if let Ok(value) =
                    vm.invoke_method(class, method, vec![Value::Ref(names), Value::Int(index)])
                {
                    out.push(value.as_ref());
                }
            }
        }
    }
    out
}

/// Whether every item of `super_type` is present in `sub_type` with an equal open type
/// (the open-data "is a value of" relation between two composite types).
fn composite_type_covers(
    vm: &mut Vm<'_>,
    super_type: ObjectRef,
    sub_type: ObjectRef,
) -> Result<bool, VmError> {
    if sub_type.is_null() {
        return Ok(false);
    }
    let types = ref_field(vm, super_type, "types", "Ljava/util/Map;");
    let other_types = ref_field(vm, sub_type, "types", "Ljava/util/Map;");
    for name in composite_type_names(vm, super_type) {
        if !map_contains_key(vm, other_types, name)? {
            return Ok(false);
        }
        let mine = map_get(vm, types, name)?;
        let theirs = map_get(vm, other_types, name)?;
        if !open_equals(vm, mine, theirs)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn composite_type_is_value(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    value: ObjectRef,
) -> Result<bool, VmError> {
    let composite_data = vm.resolve_class("javax/management/openmbean/CompositeData")?;
    if value.is_null() || !vm.is_instance(value, composite_data) {
        return Ok(false);
    }
    let (cd_class, cd_method) = vm.resolve_virtual_method(
        value,
        "getCompositeType",
        "()Ljavax/management/openmbean/CompositeType;",
    )?;
    let other_type = vm
        .invoke_method(cd_class, cd_method, vec![Value::Ref(value)])?
        .as_ref();
    composite_type_covers(vm, this, other_type)
}

fn composite_type_equals(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    other: ObjectRef,
) -> Result<bool, VmError> {
    let composite_type = vm.resolve_class("javax/management/openmbean/CompositeType")?;
    if other.is_null() || !vm.is_instance(other, composite_type) {
        return Ok(false);
    }
    let a = vm
        .read_string_field_any(this, "typeName")
        .unwrap_or_default();
    let b = vm
        .read_string_field_any(other, "typeName")
        .unwrap_or_default();
    if a != b {
        return Ok(false);
    }
    let a = vm
        .read_string_field_any(this, "description")
        .unwrap_or_default();
    let b = vm
        .read_string_field_any(other, "description")
        .unwrap_or_default();
    if a != b {
        return Ok(false);
    }
    let types = ref_field(vm, this, "types", "Ljava/util/Map;");
    let other_types = ref_field(vm, other, "types", "Ljava/util/Map;");
    if map_size(vm, types)? != map_size(vm, other_types)? {
        return Ok(false);
    }
    for name in composite_type_names(vm, this) {
        if !map_contains_key(vm, other_types, name)? {
            return Ok(false);
        }
        let mine_type = map_get(vm, types, name)?;
        let their_type = map_get(vm, other_types, name)?;
        if !open_equals(vm, mine_type, their_type)? {
            return Ok(false);
        }
    }
    Ok(true)
}

const COMPOSITE_TYPE_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;[Ljava/lang/String;[Ljavax/management/openmbean/OpenType;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            composite_type_init(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                ref_arg(args, 3),
                ref_arg(args, 4),
                ref_arg(args, 5),
            )?;
            void()
        },
    ),
    method(
        "getClassName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("javax.management.openmbean.CompositeData")?),
    ),
    method(
        "getTypeName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm
                .read_string_field_any(receiver(args), "typeName")
                .unwrap_or_default();
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "getDescription",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm
                .read_string_field_any(receiver(args), "description")
                .unwrap_or_default();
            object(vm.make_string(&text)?)
        },
    ),
    method("keySet", "()Ljava/util/Set;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let set = new_hash_set(vm)?;
        for name in composite_type_names(vm, this) {
            set_add(vm, set, name)?;
        }
        object(set)
    }),
    method(
        "getType",
        "(Ljava/lang/String;)Ljavax/management/openmbean/OpenType;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let key = ref_arg(args, 1);
            let types = ref_field(vm, this, "types", "Ljava/util/Map;");
            if key.is_null() || !map_contains_key(vm, types, key)? {
                return Err(vm.throw_new(
                    "javax/management/openmbean/OpenDataException",
                    Some("no such item"),
                ));
            }
            object(map_get(vm, types, key)?)
        },
    ),
    method(
        "getDescription",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let key = ref_arg(args, 1);
            let descriptions = ref_field(vm, this, "descriptions", "Ljava/util/Map;");
            if key.is_null() || !map_contains_key(vm, descriptions, key)? {
                return Err(vm.throw_new(
                    "javax/management/openmbean/OpenDataException",
                    Some("no such item"),
                ));
            }
            object(map_get(vm, descriptions, key)?)
        },
    ),
    method(
        "containsKey",
        "(Ljava/lang/String;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let key = ref_arg(args, 1);
            if key.is_null() {
                return boolean(false);
            }
            let types = ref_field(vm, receiver(args), "types", "Ljava/util/Map;");
            boolean(map_contains_key(vm, types, key)?)
        },
    ),
    method(
        "isValue",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            boolean(composite_type_is_value(
                vm,
                receiver(args),
                ref_arg(args, 1),
            )?)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| boolean(composite_type_equals(vm, receiver(args), ref_arg(args, 1))?),
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let mut hash = java_string_hash(
            &vm.read_string_field_any(this, "typeName")
                .unwrap_or_default(),
        );
        hash = hash.wrapping_add(java_string_hash(
            &vm.read_string_field_any(this, "description")
                .unwrap_or_default(),
        ));
        let types = ref_field(vm, this, "types", "Ljava/util/Map;");
        for name in composite_type_names(vm, this) {
            let name_text = vm.string_or_empty(name);
            hash = hash.wrapping_add(java_string_hash(&name_text));
            let type_value = map_get(vm, types, name)?;
            hash = hash.wrapping_add(open_hash(vm, type_value)?);
        }
        int(hash)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = alloc::format!(
                "{} [{}]",
                vm.read_string_field_any(this, "typeName")
                    .unwrap_or_default(),
                vm.read_string_field_any(this, "description")
                    .unwrap_or_default()
            );
            object(vm.make_string(&text)?)
        },
    ),
];

pub(crate) const COMPOSITE_TYPE: oxjvm_vm::NativeClass = class(
    "javax/management/openmbean/CompositeType",
    Some("javax/management/openmbean/OpenType"),
    &[],
    ACC_PUBLIC,
    &[
        field("typeName", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("description", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("names", "Ljava/util/List;", ACC_PRIVATE, None),
        field("types", "Ljava/util/Map;", ACC_PRIVATE, None),
        field("descriptions", "Ljava/util/Map;", ACC_PRIVATE, None),
    ],
    &COMPOSITE_TYPE_METHODS,
    None,
);
const OPEN_TYPE_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "getClassName",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getTypeName",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getDescription",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "isValue",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "hashCode",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

fn composite_data_support_init(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    composite_type: ObjectRef,
    item_names: ObjectRef,
    items_array: ObjectRef,
) -> Result<(), VmError> {
    let iae =
        |vm: &mut Vm<'_>, msg: &str| vm.throw_new("java/lang/IllegalArgumentException", Some(msg));
    if composite_type.is_null() {
        return Err(iae(vm, "compositeType is null"));
    }
    if item_names.is_null() || items_array.is_null() {
        return Err(iae(vm, "itemNames or items is null"));
    }
    let names = array_refs(vm, item_names)?;
    let values = array_refs(vm, items_array)?;
    if names.len() != values.len() {
        return Err(iae(vm, "itemNames and items have different lengths"));
    }
    let type_names = composite_type_names(vm, composite_type);
    if names.len() != type_names.len() {
        return Err(iae(vm, "itemNames does not match the composite type"));
    }
    let items = new_hash_map(vm)?;
    for index in 0..names.len() {
        let name = names[index];
        let value = values[index];
        if name.is_null() {
            return Err(iae(vm, "null item name"));
        }
        let name_text = vm.string_or_empty(name);
        if name_text.is_empty() {
            return Err(iae(vm, "empty item name"));
        }
        // The name must be an item of the composite type, and the value must be valid for it.
        let (get_type_class, get_type_method) = vm.resolve_virtual_method(
            composite_type,
            "getType",
            "(Ljava/lang/String;)Ljavax/management/openmbean/OpenType;",
        )?;
        let open_type = vm
            .invoke_method(
                get_type_class,
                get_type_method,
                vec![Value::Ref(composite_type), Value::Ref(name)],
            )
            .map_err(|_| iae(vm, "item name not in composite type"))?
            .as_ref();
        let (is_value_class, is_value_method) =
            vm.resolve_virtual_method(open_type, "isValue", "(Ljava/lang/Object;)Z")?;
        let valid = vm
            .invoke_method(
                is_value_class,
                is_value_method,
                vec![Value::Ref(open_type), Value::Ref(value)],
            )?
            .as_int()
            != 0;
        if !valid {
            return Err(vm.throw_new(
                "javax/management/openmbean/OpenDataException",
                Some("invalid value for item"),
            ));
        }
        if map_contains_key(vm, items, name)? {
            return Err(iae(vm, "duplicate item name"));
        }
        map_put(vm, items, name, value)?;
    }
    set_ref_field(
        vm,
        this,
        "type",
        "Ljavax/management/openmbean/CompositeType;",
        composite_type,
    )?;
    set_ref_field(vm, this, "items", "Ljava/util/Map;", items)?;
    Ok(())
}

fn composite_data_support_get(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    key: ObjectRef,
) -> Result<ObjectRef, VmError> {
    let items = ref_field(vm, this, "items", "Ljava/util/Map;");
    if key.is_null() {
        return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("null key")));
    }
    let key_text = vm.string_or_empty(key);
    if key_text.is_empty() || !map_contains_key(vm, items, key)? {
        return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("no such item")));
    }
    map_get(vm, items, key)
}

fn composite_data_support_equals(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    other: ObjectRef,
) -> Result<bool, VmError> {
    let composite_data = vm.resolve_class("javax/management/openmbean/CompositeData")?;
    if other.is_null() || !vm.is_instance(other, composite_data) {
        return Ok(false);
    }
    let type_ref = ref_field(
        vm,
        this,
        "type",
        "Ljavax/management/openmbean/CompositeType;",
    );
    let (cd_class, cd_method) = vm.resolve_virtual_method(
        other,
        "getCompositeType",
        "()Ljavax/management/openmbean/CompositeType;",
    )?;
    let other_type = vm
        .invoke_method(cd_class, cd_method, vec![Value::Ref(other)])?
        .as_ref();
    if !open_equals(vm, type_ref, other_type)? {
        return Ok(false);
    }
    let items = ref_field(vm, this, "items", "Ljava/util/Map;");
    for name in composite_type_names(vm, type_ref) {
        let mine = map_get(vm, items, name)?;
        let (get_class, get_method) =
            vm.resolve_virtual_method(other, "get", "(Ljava/lang/String;)Ljava/lang/Object;")?;
        let theirs = vm
            .invoke_method(
                get_class,
                get_method,
                vec![Value::Ref(other), Value::Ref(name)],
            )?
            .as_ref();
        if !open_equals(vm, mine, theirs)? {
            return Ok(false);
        }
    }
    Ok(true)
}

const COMPOSITE_DATA_SUPPORT_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
    method(
        "<init>",
        "(Ljavax/management/openmbean/CompositeType;[Ljava/lang/String;[Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            composite_data_support_init(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                ref_arg(args, 3),
            )?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljavax/management/openmbean/CompositeType;Ljava/util/Map;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let composite_type = ref_arg(args, 1);
            let map = ref_arg(args, 2);
            if map.is_null() {
                return Err(
                    vm.throw_new("java/lang/IllegalArgumentException", Some("null items map"))
                );
            }
            let (keyset_class, keyset_method) =
                vm.resolve_virtual_method(map, "keySet", "()Ljava/util/Set;")?;
            let keyset = vm
                .invoke_method(keyset_class, keyset_method, vec![Value::Ref(map)])?
                .as_ref();
            let keys = collection_refs(vm, keyset)?;
            let object_class = vm.resolve_class("java/lang/Object")?;
            let string_class = vm.resolve_class("java/lang/String")?;
            let names_array = vm.allocate_object_array(string_class, keys.len())?;
            let values_array = vm.allocate_object_array(object_class, keys.len())?;
            for (index, key) in keys.iter().enumerate() {
                let (get_class, get_method) = vm.resolve_virtual_method(
                    map,
                    "get",
                    "(Ljava/lang/Object;)Ljava/lang/Object;",
                )?;
                let value = vm
                    .invoke_method(
                        get_class,
                        get_method,
                        vec![Value::Ref(map), Value::Ref(*key)],
                    )?
                    .as_ref();
                vm.array_set_ref(names_array, index, *key)?;
                vm.array_set_ref(values_array, index, value)?;
            }
            composite_data_support_init(vm, this, composite_type, names_array, values_array)?;
            void()
        },
    ),
    method(
        "getCompositeType",
        "()Ljavax/management/openmbean/CompositeType;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "type",
                "Ljavax/management/openmbean/CompositeType;",
            ))
        },
    ),
    method(
        "get",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(composite_data_support_get(
                vm,
                receiver(args),
                ref_arg(args, 1),
            )?)
        },
    ),
    method(
        "getAll",
        "([Ljava/lang/String;)[Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let keys = ref_arg(args, 1);
            if keys.is_null() {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("null keys")));
            }
            let key_refs = array_refs(vm, keys)?;
            let object_class = vm.resolve_class("java/lang/Object")?;
            let result = vm.allocate_object_array(object_class, key_refs.len())?;
            for (index, key) in key_refs.iter().enumerate() {
                let value = composite_data_support_get(vm, this, *key)?;
                vm.array_set_ref(result, index, value)?;
            }
            object(result)
        },
    ),
    method(
        "containsKey",
        "(Ljava/lang/String;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let key = ref_arg(args, 1);
            if key.is_null() {
                return boolean(false);
            }
            let key_text = vm.string_or_empty(key);
            if key_text.is_empty() {
                return boolean(false);
            }
            let items = ref_field(vm, receiver(args), "items", "Ljava/util/Map;");
            boolean(map_contains_key(vm, items, key)?)
        },
    ),
    method(
        "containsValue",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let value = ref_arg(args, 1);
            let type_ref = ref_field(
                vm,
                this,
                "type",
                "Ljavax/management/openmbean/CompositeType;",
            );
            let items = ref_field(vm, this, "items", "Ljava/util/Map;");
            for name in composite_type_names(vm, type_ref) {
                let item_value = map_get(vm, items, name)?;
                if open_equals(vm, item_value, value)? {
                    return boolean(true);
                }
            }
            boolean(false)
        },
    ),
    method(
        "values",
        "()Ljava/util/Collection;",
        ACC_PUBLIC,
        |vm, _, args| {
            let items = ref_field(vm, receiver(args), "items", "Ljava/util/Map;");
            let (class, method) =
                vm.resolve_virtual_method(items, "values", "()Ljava/util/Collection;")?;
            vm.invoke_method(class, method, vec![Value::Ref(items)])
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            boolean(composite_data_support_equals(
                vm,
                receiver(args),
                ref_arg(args, 1),
            )?)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let type_ref = ref_field(
            vm,
            this,
            "type",
            "Ljavax/management/openmbean/CompositeType;",
        );
        let mut hash = open_hash(vm, type_ref)?;
        let items = ref_field(vm, this, "items", "Ljava/util/Map;");
        for name in composite_type_names(vm, type_ref) {
            let item_value = map_get(vm, items, name)?;
            hash = hash.wrapping_add(open_hash(vm, item_value)?);
        }
        int(hash)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let type_ref = ref_field(
                vm,
                this,
                "type",
                "Ljavax/management/openmbean/CompositeType;",
            );
            let text = alloc::format!(
                "{} items",
                vm.read_string_field_any(type_ref, "typeName")
                    .unwrap_or_default()
            );
            object(vm.make_string(&text)?)
        },
    ),
];

pub(crate) const COMPOSITE_DATA_SUPPORT: oxjvm_vm::NativeClass = class(
    "javax/management/openmbean/CompositeDataSupport",
    Some("java/lang/Object"),
    &[
        "javax/management/openmbean/CompositeData",
        "java/io/Serializable",
    ],
    ACC_PUBLIC,
    &[
        field(
            "type",
            "Ljavax/management/openmbean/CompositeType;",
            ACC_PRIVATE,
            None,
        ),
        field("items", "Ljava/util/Map;", ACC_PRIVATE, None),
    ],
    &COMPOSITE_DATA_SUPPORT_METHODS,
    None,
);

pub(crate) const OPEN_DATA_EXCEPTION: oxjvm_vm::NativeClass = class(
    "javax/management/openmbean/OpenDataException",
    Some("java/lang/Exception"),
    &[],
    ACC_PUBLIC,
    &[],
    &[
        method(
            "<init>",
            "()V",
            ACC_PUBLIC,
            crate::throwable::exception_ctor,
        ),
        method(
            "<init>",
            "(Ljava/lang/String;)V",
            ACC_PUBLIC,
            crate::throwable::exception_ctor,
        ),
    ],
    None,
);

/// Whether `value` is an array whose elements match an `ArrayType` of the given dimension.
fn array_value_matches(
    vm: &mut Vm<'_>,
    value: ObjectRef,
    dimension: i32,
    element_type: ObjectRef,
    primitive: bool,
) -> Result<bool, VmError> {
    if value.is_null() {
        return Ok(false);
    }
    let Ok(length) = vm.array_length(value) else {
        return Ok(false);
    };
    for index in 0..length {
        let element = vm.array_get(value, index as i32)?;
        if dimension > 1 {
            if !array_value_matches(vm, element.as_ref(), dimension - 1, element_type, primitive)? {
                return Ok(false);
            }
            continue;
        }
        if primitive {
            // A primitive array element must be a primitive slot, not a reference.
            if matches!(element, Value::Ref(_)) {
                return Ok(false);
            }
            continue;
        }
        let element_ref = element.as_ref();
        if element_ref.is_null() {
            continue; // null is a valid value for any non-primitive open type
        }
        let (class, method) =
            vm.resolve_virtual_method(element_type, "isValue", "(Ljava/lang/Object;)Z")?;
        let valid = vm
            .invoke_method(
                class,
                method,
                vec![Value::Ref(element_type), Value::Ref(element_ref)],
            )?
            .as_int()
            != 0;
        if !valid {
            return Ok(false);
        }
    }
    Ok(true)
}

fn array_type_init(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    dimension: i32,
    element_type: ObjectRef,
) -> Result<(), VmError> {
    if dimension < 1 {
        return Err(vm.throw_new(
            "javax/management/openmbean/OpenDataException",
            Some("dimension must be >= 1"),
        ));
    }
    if element_type.is_null() {
        return Err(vm.throw_new(
            "javax/management/openmbean/OpenDataException",
            Some("null element type"),
        ));
    }
    let (class, method) =
        vm.resolve_virtual_method(element_type, "getClassName", "()Ljava/lang/String;")?;
    let element_class_name = vm
        .invoke_method(class, method, vec![Value::Ref(element_type)])?
        .as_ref();
    let element_class_name = vm.string_or_empty(element_class_name);
    let class_name = alloc::format!("{}L{};", "[".repeat(dimension as usize), element_class_name);
    crate::lang::write_int_field(vm, this, "dimension", dimension);
    crate::lang::write_int_field(vm, this, "primitive", 0);
    set_ref_field(
        vm,
        this,
        "elementType",
        "Ljavax/management/openmbean/OpenType;",
        element_type,
    )?;
    let class = vm.class_of(this);
    vm.set_string_field(this, class, "className", &class_name)?;
    Ok(())
}

fn array_type_equals(vm: &mut Vm<'_>, this: ObjectRef, other: ObjectRef) -> Result<bool, VmError> {
    let array_type = vm.resolve_class("javax/management/openmbean/ArrayType")?;
    if other.is_null() || !vm.is_instance(other, array_type) {
        return Ok(false);
    }
    if crate::lang::read_int_field(vm, this, "dimension")
        != crate::lang::read_int_field(vm, other, "dimension")
    {
        return Ok(false);
    }
    if crate::lang::read_int_field(vm, this, "primitive")
        != crate::lang::read_int_field(vm, other, "primitive")
    {
        return Ok(false);
    }
    let a = ref_field(
        vm,
        this,
        "elementType",
        "Ljavax/management/openmbean/OpenType;",
    );
    let b = ref_field(
        vm,
        other,
        "elementType",
        "Ljavax/management/openmbean/OpenType;",
    );
    open_equals(vm, a, b)
}

const ARRAY_TYPE_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method(
        "<init>",
        "(ILjavax/management/openmbean/OpenType;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            array_type_init(
                vm,
                receiver(args),
                crate::int_arg(args, 1),
                ref_arg(args, 2),
            )?;
            void()
        },
    ),
    method(
        "getArrayType",
        "(Ljavax/management/openmbean/OpenType;)Ljavax/management/openmbean/ArrayType;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let element_type = ref_arg(args, 0);
            let class = vm.resolve_class("javax/management/openmbean/ArrayType")?;
            let instance = vm.new_instance(class)?;
            array_type_init(vm, instance, 1, element_type)?;
            object(instance)
        },
    ),
    method(
        "getPrimitiveArrayType",
        "(Ljava/lang/Class;)Ljavax/management/openmbean/ArrayType;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let clazz = ref_arg(args, 0);
            if clazz.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let (name_class, name_method) =
                vm.resolve_virtual_method(clazz, "getName", "()Ljava/lang/String;")?;
            let name = vm
                .invoke_method(name_class, name_method, vec![Value::Ref(clazz)])?
                .as_ref();
            let name = vm.string_or_empty(name);
            let dimension = name.chars().take_while(|c| *c == '[').count() as i32;
            let component = name.trim_start_matches('[');
            let simple = match component {
                "I" => "INTEGER",
                "J" => "LONG",
                "F" => "FLOAT",
                "D" => "DOUBLE",
                "Z" => "BOOLEAN",
                "B" => "BYTE",
                "C" => "CHARACTER",
                "S" => "SHORT",
                _ => {
                    return Err(vm.throw_new(
                        "java/lang/IllegalArgumentException",
                        Some("not a primitive array class"),
                    ));
                }
            };
            let simple_class = vm.resolve_class("javax/management/openmbean/SimpleType")?;
            let element_type = vm
                .read_static_ref(
                    simple_class,
                    simple,
                    "Ljavax/management/openmbean/SimpleType;",
                )
                .unwrap_or(ObjectRef::NULL);
            let class = vm.resolve_class("javax/management/openmbean/ArrayType")?;
            let instance = vm.new_instance(class)?;
            crate::lang::write_int_field(vm, instance, "dimension", dimension);
            crate::lang::write_int_field(vm, instance, "primitive", 1);
            set_ref_field(
                vm,
                instance,
                "elementType",
                "Ljavax/management/openmbean/OpenType;",
                element_type,
            )?;
            let instance_class = vm.class_of(instance);
            vm.set_string_field(instance, instance_class, "className", &name)?;
            object(instance)
        },
    ),
    method(
        "getElementOpenType",
        "()Ljavax/management/openmbean/OpenType;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "elementType",
                "Ljavax/management/openmbean/OpenType;",
            ))
        },
    ),
    method("getDimension", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "dimension"))
    }),
    method("isPrimitive", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "primitive") != 0)
    }),
    method(
        "getClassName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm
                .read_string_field_any(receiver(args), "className")
                .unwrap_or_default();
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "getTypeName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm
                .read_string_field_any(receiver(args), "className")
                .unwrap_or_default();
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "getDescription",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm
                .read_string_field_any(receiver(args), "className")
                .unwrap_or_default();
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "isValue",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let dimension = crate::lang::read_int_field(vm, this, "dimension");
            let primitive = crate::lang::read_int_field(vm, this, "primitive") != 0;
            let element_type = ref_field(
                vm,
                this,
                "elementType",
                "Ljavax/management/openmbean/OpenType;",
            );
            boolean(array_value_matches(
                vm,
                ref_arg(args, 1),
                dimension,
                element_type,
                primitive,
            )?)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| boolean(array_type_equals(vm, receiver(args), ref_arg(args, 1))?),
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let mut hash = crate::lang::read_int_field(vm, this, "dimension");
        let element_type = ref_field(
            vm,
            this,
            "elementType",
            "Ljavax/management/openmbean/OpenType;",
        );
        hash = hash
            .wrapping_mul(31)
            .wrapping_add(open_hash(vm, element_type)?);
        int(hash)
    }),
];

pub(crate) const ARRAY_TYPE: oxjvm_vm::NativeClass = class(
    "javax/management/openmbean/ArrayType",
    Some("javax/management/openmbean/OpenType"),
    &[],
    ACC_PUBLIC,
    &[
        field("dimension", "I", ACC_PRIVATE, None),
        field("primitive", "I", ACC_PRIVATE, None),
        field(
            "elementType",
            "Ljavax/management/openmbean/OpenType;",
            ACC_PRIVATE,
            None,
        ),
        field("className", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &ARRAY_TYPE_METHODS,
    None,
);

pub(crate) const OPEN_TYPE: oxjvm_vm::NativeClass = class(
    "javax/management/openmbean/OpenType",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &OPEN_TYPE_METHODS,
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

const SIMPLE_TYPE_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
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
    method(
        "getTypeName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = vm
                .read_string_field(this, vm.class_of(this), "className")
                .unwrap_or_default();
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "getDescription",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = vm
                .read_string_field(this, vm.class_of(this), "className")
                .unwrap_or_default();
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "isValue",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let value = ref_arg(args, 1);
            let class_name = vm
                .read_string_field(this, vm.class_of(this), "className")
                .unwrap_or_default();
            boolean(simple_type_is_value(vm, &class_name, value)?)
        },
    ),
];

/// Whether `value` is a valid instance of the simple type named `class_name`.
fn simple_type_is_value(
    vm: &mut Vm<'_>,
    class_name: &str,
    value: ObjectRef,
) -> Result<bool, VmError> {
    if class_name == "java.lang.Void" {
        return Ok(value.is_null());
    }
    if value.is_null() {
        return Ok(false);
    }
    let internal = class_name.replace('.', "/");
    let Ok(class) = vm.resolve_class(&internal) else {
        return Ok(false);
    };
    Ok(vm.is_instance(value, class))
}

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
fn tabular_type_init(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    type_name: ObjectRef,
    description: ObjectRef,
    row_type: ObjectRef,
    index_names: ObjectRef,
) -> Result<(), VmError> {
    let iae =
        |vm: &mut Vm<'_>, msg: &str| vm.throw_new("java/lang/IllegalArgumentException", Some(msg));
    if type_name.is_null() || description.is_null() {
        return Err(iae(vm, "typeName or description is null"));
    }
    let type_name_text = vm.string_or_empty(type_name);
    let description_text = vm.string_or_empty(description);
    if type_name_text.is_empty() || description_text.is_empty() {
        return Err(iae(vm, "typeName or description is empty"));
    }
    if row_type.is_null() {
        return Err(iae(vm, "rowType is null"));
    }
    if index_names.is_null() {
        return Err(iae(vm, "indexNames is null"));
    }
    let names = array_refs(vm, index_names)?;
    if names.is_empty() {
        return Err(iae(vm, "no index names"));
    }
    let index_list = new_array_list(vm)?;
    for name in &names {
        if name.is_null() {
            return Err(iae(vm, "null index name"));
        }
        let name_text = vm.string_or_empty(*name);
        if name_text.is_empty() {
            return Err(iae(vm, "empty index name"));
        }
        // Each index name must be a valid item of the row type.
        let (contains_class, contains_method) =
            vm.resolve_virtual_method(row_type, "containsKey", "(Ljava/lang/String;)Z")?;
        let present = vm
            .invoke_method(
                contains_class,
                contains_method,
                vec![Value::Ref(row_type), Value::Ref(*name)],
            )?
            .as_int()
            != 0;
        if !present {
            return Err(iae(vm, "index name is not an item of the row type"));
        }
        list_add(vm, index_list, *name)?;
    }
    let class = vm.class_of(this);
    vm.set_string_field(this, class, "typeName", &type_name_text)?;
    vm.set_string_field(this, class, "description", &description_text)?;
    set_ref_field(
        vm,
        this,
        "rowType",
        "Ljavax/management/openmbean/CompositeType;",
        row_type,
    )?;
    set_ref_field(vm, this, "indexNames", "Ljava/util/List;", index_list)?;
    Ok(())
}

fn tabular_type_equals(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    other: ObjectRef,
) -> Result<bool, VmError> {
    let tabular_type = vm.resolve_class("javax/management/openmbean/TabularType")?;
    if other.is_null() || !vm.is_instance(other, tabular_type) {
        return Ok(false);
    }
    let a = vm
        .read_string_field_any(this, "typeName")
        .unwrap_or_default();
    let b = vm
        .read_string_field_any(other, "typeName")
        .unwrap_or_default();
    if a != b {
        return Ok(false);
    }
    let a = vm
        .read_string_field_any(this, "description")
        .unwrap_or_default();
    let b = vm
        .read_string_field_any(other, "description")
        .unwrap_or_default();
    if a != b {
        return Ok(false);
    }
    let row_a = ref_field(
        vm,
        this,
        "rowType",
        "Ljavax/management/openmbean/CompositeType;",
    );
    let row_b = ref_field(
        vm,
        other,
        "rowType",
        "Ljavax/management/openmbean/CompositeType;",
    );
    if !open_equals(vm, row_a, row_b)? {
        return Ok(false);
    }
    let index_a = ref_field(vm, this, "indexNames", "Ljava/util/List;");
    let index_b = ref_field(vm, other, "indexNames", "Ljava/util/List;");
    open_equals(vm, index_a, index_b)
}

const TABULAR_TYPE_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;Ljavax/management/openmbean/CompositeType;[Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            tabular_type_init(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                ref_arg(args, 3),
                ref_arg(args, 4),
            )?;
            void()
        },
    ),
    method(
        "getClassName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("javax.management.openmbean.TabularData")?),
    ),
    method(
        "getTypeName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm
                .read_string_field_any(receiver(args), "typeName")
                .unwrap_or_default();
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "getDescription",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm
                .read_string_field_any(receiver(args), "description")
                .unwrap_or_default();
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "getRowType",
        "()Ljavax/management/openmbean/CompositeType;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "rowType",
                "Ljavax/management/openmbean/CompositeType;",
            ))
        },
    ),
    method(
        "getIndexNames",
        "()Ljava/util/List;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "indexNames",
                "Ljava/util/List;",
            ))
        },
    ),
    method(
        "isValue",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let value = ref_arg(args, 1);
            let tabular_data = vm.resolve_class("javax/management/openmbean/TabularData")?;
            if value.is_null() || !vm.is_instance(value, tabular_data) {
                return boolean(false);
            }
            let (td_class, td_method) = vm.resolve_virtual_method(
                value,
                "getTabularType",
                "()Ljavax/management/openmbean/TabularType;",
            )?;
            let other_type = vm
                .invoke_method(td_class, td_method, vec![Value::Ref(value)])?
                .as_ref();
            if other_type.is_null() {
                return boolean(false);
            }
            let a = vm
                .read_string_field_any(this, "typeName")
                .unwrap_or_default();
            let b = vm
                .read_string_field_any(other_type, "typeName")
                .unwrap_or_default();
            if a != b {
                return boolean(false);
            }
            let a = vm
                .read_string_field_any(this, "description")
                .unwrap_or_default();
            let b = vm
                .read_string_field_any(other_type, "description")
                .unwrap_or_default();
            if a != b {
                return boolean(false);
            }
            let index_a = ref_field(vm, this, "indexNames", "Ljava/util/List;");
            let index_b = ref_field(vm, other_type, "indexNames", "Ljava/util/List;");
            if !open_equals(vm, index_a, index_b)? {
                return boolean(false);
            }
            let row_a = ref_field(
                vm,
                this,
                "rowType",
                "Ljavax/management/openmbean/CompositeType;",
            );
            let row_b = ref_field(
                vm,
                other_type,
                "rowType",
                "Ljavax/management/openmbean/CompositeType;",
            );
            boolean(composite_type_covers(vm, row_a, row_b)?)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| boolean(tabular_type_equals(vm, receiver(args), ref_arg(args, 1))?),
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let mut hash = java_string_hash(
            &vm.read_string_field_any(this, "typeName")
                .unwrap_or_default(),
        );
        hash = hash.wrapping_add(java_string_hash(
            &vm.read_string_field_any(this, "description")
                .unwrap_or_default(),
        ));
        let row = ref_field(
            vm,
            this,
            "rowType",
            "Ljavax/management/openmbean/CompositeType;",
        );
        hash = hash.wrapping_add(open_hash(vm, row)?);
        let index = ref_field(vm, this, "indexNames", "Ljava/util/List;");
        hash = hash.wrapping_add(open_hash(vm, index)?);
        int(hash)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = alloc::format!(
                "{} [{}]",
                vm.read_string_field_any(this, "typeName")
                    .unwrap_or_default(),
                vm.read_string_field_any(this, "description")
                    .unwrap_or_default()
            );
            object(vm.make_string(&text)?)
        },
    ),
];

pub(crate) const TABULAR_TYPE: oxjvm_vm::NativeClass = class(
    "javax/management/openmbean/TabularType",
    Some("javax/management/openmbean/OpenType"),
    &[],
    ACC_PUBLIC,
    &[
        field("typeName", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("description", "Ljava/lang/String;", ACC_PRIVATE, None),
        field(
            "rowType",
            "Ljavax/management/openmbean/CompositeType;",
            ACC_PRIVATE,
            None,
        ),
        field("indexNames", "Ljava/util/List;", ACC_PRIVATE, None),
    ],
    &TABULAR_TYPE_METHODS,
    None,
);
/// The elements of a `List`, in order.
fn list_refs(vm: &mut Vm<'_>, list: ObjectRef) -> Result<Vec<ObjectRef>, VmError> {
    let (size_class, size_method) = vm.resolve_virtual_method(list, "size", "()I")?;
    let size = vm
        .invoke_method(size_class, size_method, vec![Value::Ref(list)])?
        .as_int();
    let mut out = Vec::with_capacity(size.max(0) as usize);
    for index in 0..size {
        let (class, method) = vm.resolve_virtual_method(list, "get", "(I)Ljava/lang/Object;")?;
        out.push(
            vm.invoke_method(class, method, vec![Value::Ref(list), Value::Int(index)])?
                .as_ref(),
        );
    }
    Ok(out)
}

/// The elements of a `Collection`, via its iterator.
fn collection_refs(vm: &mut Vm<'_>, collection: ObjectRef) -> Result<Vec<ObjectRef>, VmError> {
    let (iter_class, iter_method) =
        vm.resolve_virtual_method(collection, "iterator", "()Ljava/util/Iterator;")?;
    let iterator = vm
        .invoke_method(iter_class, iter_method, vec![Value::Ref(collection)])?
        .as_ref();
    let mut out = Vec::new();
    loop {
        let (has_class, has_method) = vm.resolve_virtual_method(iterator, "hasNext", "()Z")?;
        let has_next = vm
            .invoke_method(has_class, has_method, vec![Value::Ref(iterator)])?
            .as_int()
            != 0;
        if !has_next {
            break;
        }
        let (next_class, next_method) =
            vm.resolve_virtual_method(iterator, "next", "()Ljava/lang/Object;")?;
        out.push(
            vm.invoke_method(next_class, next_method, vec![Value::Ref(iterator)])?
                .as_ref(),
        );
    }
    Ok(out)
}

fn tabular_data_support_init(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    tabular_type: ObjectRef,
) -> Result<(), VmError> {
    if tabular_type.is_null() {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("null tabularType"),
        ));
    }
    let rows = new_hash_map(vm)?;
    set_ref_field(
        vm,
        this,
        "tabularType",
        "Ljavax/management/openmbean/TabularType;",
        tabular_type,
    )?;
    set_ref_field(vm, this, "rows", "Ljava/util/Map;", rows)?;
    Ok(())
}

/// The index (a List of the index-item values) of a row.
fn tabular_calculate_index(
    vm: &mut Vm<'_>,
    tabular_type: ObjectRef,
    row: ObjectRef,
) -> Result<ObjectRef, VmError> {
    let (index_class, index_method) =
        vm.resolve_virtual_method(tabular_type, "getIndexNames", "()Ljava/util/List;")?;
    let index_names = vm
        .invoke_method(index_class, index_method, vec![Value::Ref(tabular_type)])?
        .as_ref();
    let index = new_array_list(vm)?;
    for name in list_refs(vm, index_names)? {
        let (get_class, get_method) =
            vm.resolve_virtual_method(row, "get", "(Ljava/lang/String;)Ljava/lang/Object;")?;
        let value = vm
            .invoke_method(
                get_class,
                get_method,
                vec![Value::Ref(row), Value::Ref(name)],
            )?
            .as_ref();
        list_add(vm, index, value)?;
    }
    Ok(index)
}

fn tabular_data_support_put(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    row: ObjectRef,
) -> Result<ObjectRef, VmError> {
    if row.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let tabular_type = ref_field(
        vm,
        this,
        "tabularType",
        "Ljavax/management/openmbean/TabularType;",
    );
    let (row_class, row_method) = vm.resolve_virtual_method(
        tabular_type,
        "getRowType",
        "()Ljavax/management/openmbean/CompositeType;",
    )?;
    let row_type = vm
        .invoke_method(row_class, row_method, vec![Value::Ref(tabular_type)])?
        .as_ref();
    let (is_value_class, is_value_method) =
        vm.resolve_virtual_method(row_type, "isValue", "(Ljava/lang/Object;)Z")?;
    let valid = vm
        .invoke_method(
            is_value_class,
            is_value_method,
            vec![Value::Ref(row_type), Value::Ref(row)],
        )?
        .as_int()
        != 0;
    if !valid {
        return Err(vm.throw_new(
            "javax/management/openmbean/OpenDataException",
            Some("row is not a value of the row type"),
        ));
    }
    let index = tabular_calculate_index(vm, tabular_type, row)?;
    let rows = ref_field(vm, this, "rows", "Ljava/util/Map;");
    if map_contains_key(vm, rows, index)? {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("duplicate index"),
        ));
    }
    map_put(vm, rows, index, row)?;
    Ok(ObjectRef::NULL)
}

const TABULAR_DATA_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "getTabularType",
        "()Ljavax/management/openmbean/TabularType;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "calculateIndex",
        "(Ljavax/management/openmbean/CompositeData;)Ljava/util/List;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "put",
        "(Ljavax/management/openmbean/CompositeData;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "putAll",
        "([Ljavax/management/openmbean/CompositeData;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const TABULAR_DATA: oxjvm_vm::NativeClass = class(
    "javax/management/openmbean/TabularData",
    Some("java/lang/Object"),
    &["java/util/Map"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &TABULAR_DATA_METHODS,
    None,
);

const TABULAR_DATA_SUPPORT_METHODS: [oxjvm_vm::NativeMethodDef; 18] = [
    method(
        "<init>",
        "(Ljavax/management/openmbean/TabularType;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            tabular_data_support_init(vm, receiver(args), ref_arg(args, 1))?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljavax/management/openmbean/TabularType;Ljava/util/Map;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            tabular_data_support_init(vm, this, ref_arg(args, 1))?;
            let map = ref_arg(args, 2);
            if !map.is_null() {
                let (values_class, values_method) =
                    vm.resolve_virtual_method(map, "values", "()Ljava/util/Collection;")?;
                let values = vm
                    .invoke_method(values_class, values_method, vec![Value::Ref(map)])?
                    .as_ref();
                for row in collection_refs(vm, values)? {
                    tabular_data_support_put(vm, this, row)?;
                }
            }
            void()
        },
    ),
    method(
        "getTabularType",
        "()Ljavax/management/openmbean/TabularType;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "tabularType",
                "Ljavax/management/openmbean/TabularType;",
            ))
        },
    ),
    method(
        "calculateIndex",
        "(Ljavax/management/openmbean/CompositeData;)Ljava/util/List;",
        ACC_PUBLIC,
        |vm, _, args| {
            let tabular_type = ref_field(
                vm,
                receiver(args),
                "tabularType",
                "Ljavax/management/openmbean/TabularType;",
            );
            object(tabular_calculate_index(vm, tabular_type, ref_arg(args, 1))?)
        },
    ),
    method(
        "put",
        "(Ljavax/management/openmbean/CompositeData;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            tabular_data_support_put(vm, receiver(args), ref_arg(args, 1))?;
            void()
        },
    ),
    method(
        "putAll",
        "([Ljavax/management/openmbean/CompositeData;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let rows = ref_arg(args, 1);
            if rows.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            for row in array_refs(vm, rows)? {
                tabular_data_support_put(vm, this, row)?;
            }
            void()
        },
    ),
    method(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(tabular_data_support_put(
                vm,
                receiver(args),
                ref_arg(args, 2),
            )?)
        },
    ),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        let rows = ref_field(vm, receiver(args), "rows", "Ljava/util/Map;");
        int(map_size(vm, rows)?)
    }),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        let rows = ref_field(vm, receiver(args), "rows", "Ljava/util/Map;");
        boolean(map_size(vm, rows)? == 0)
    }),
    method(
        "containsKey",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let rows = ref_field(vm, receiver(args), "rows", "Ljava/util/Map;");
            boolean(map_contains_key(vm, rows, ref_arg(args, 1))?)
        },
    ),
    method(
        "containsValue",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let rows = ref_field(vm, receiver(args), "rows", "Ljava/util/Map;");
            let (class, method) =
                vm.resolve_virtual_method(rows, "containsValue", "(Ljava/lang/Object;)Z")?;
            vm.invoke_method(class, method, vec![Value::Ref(rows), args[1]])
        },
    ),
    method(
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let rows = ref_field(vm, receiver(args), "rows", "Ljava/util/Map;");
            object(map_get(vm, rows, ref_arg(args, 1))?)
        },
    ),
    method(
        "remove",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let rows = ref_field(vm, receiver(args), "rows", "Ljava/util/Map;");
            let key = ref_arg(args, 1);
            let previous = map_get(vm, rows, key)?;
            map_remove(vm, rows, key)?;
            object(previous)
        },
    ),
    method("putAll", "(Ljava/util/Map;)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let map = ref_arg(args, 1);
        if !map.is_null() {
            let (values_class, values_method) =
                vm.resolve_virtual_method(map, "values", "()Ljava/util/Collection;")?;
            let values = vm
                .invoke_method(values_class, values_method, vec![Value::Ref(map)])?
                .as_ref();
            for row in collection_refs(vm, values)? {
                tabular_data_support_put(vm, this, row)?;
            }
        }
        void()
    }),
    method("clear", "()V", ACC_PUBLIC, |vm, _, args| {
        let rows = ref_field(vm, receiver(args), "rows", "Ljava/util/Map;");
        let (class, method) = vm.resolve_virtual_method(rows, "clear", "()V")?;
        vm.invoke_method(class, method, vec![Value::Ref(rows)])?;
        void()
    }),
    method("keySet", "()Ljava/util/Set;", ACC_PUBLIC, |vm, _, args| {
        let rows = ref_field(vm, receiver(args), "rows", "Ljava/util/Map;");
        let (class, method) = vm.resolve_virtual_method(rows, "keySet", "()Ljava/util/Set;")?;
        vm.invoke_method(class, method, vec![Value::Ref(rows)])
    }),
    method(
        "values",
        "()Ljava/util/Collection;",
        ACC_PUBLIC,
        |vm, _, args| {
            let rows = ref_field(vm, receiver(args), "rows", "Ljava/util/Map;");
            let (class, method) =
                vm.resolve_virtual_method(rows, "values", "()Ljava/util/Collection;")?;
            vm.invoke_method(class, method, vec![Value::Ref(rows)])
        },
    ),
    method(
        "entrySet",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, args| {
            let rows = ref_field(vm, receiver(args), "rows", "Ljava/util/Map;");
            let (class, method) =
                vm.resolve_virtual_method(rows, "entrySet", "()Ljava/util/Set;")?;
            vm.invoke_method(class, method, vec![Value::Ref(rows)])
        },
    ),
];

pub(crate) const TABULAR_DATA_SUPPORT: oxjvm_vm::NativeClass = class(
    "javax/management/openmbean/TabularDataSupport",
    Some("java/lang/Object"),
    &[
        "javax/management/openmbean/TabularData",
        "java/util/Map",
        "java/io/Serializable",
    ],
    ACC_PUBLIC,
    &[
        field(
            "tabularType",
            "Ljavax/management/openmbean/TabularType;",
            ACC_PRIVATE,
            None,
        ),
        field("rows", "Ljava/util/Map;", ACC_PRIVATE, None),
    ],
    &TABULAR_DATA_SUPPORT_METHODS,
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

/// `MBeanServer.createMBean`: instantiate the named class and register it under the given name.
fn mbean_server_create(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let this = receiver(args);
    let class_name = crate::string_arg(vm, args[1])?;
    let name = ref_arg(args, 2);
    if name.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let internal = class_name.replace('.', "/");
    let class = vm.resolve_class(&internal)?;
    let target = vm.new_instance(class)?;
    let (declaring, method) = vm.resolve_virtual_method(
        this,
        "registerMBean",
        "(Ljava/lang/Object;Ljavax/management/ObjectName;)Ljavax/management/ObjectInstance;",
    )?;
    vm.invoke_method(
        declaring,
        method,
        vec![Value::Ref(this), Value::Ref(target), Value::Ref(name)],
    )
}

const OJVM_MBEAN_SERVER_METHODS: [oxjvm_vm::NativeMethodDef; 23] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let entries = new_hash_map(vm)?;
        set_ref_field(vm, this, "entries", "Ljava/util/HashMap;", entries)?;
        void()
    }),
    method(
        "createMBean",
        "(Ljava/lang/String;Ljavax/management/ObjectName;)Ljavax/management/ObjectInstance;",
        ACC_PUBLIC,
        |vm, _, args| mbean_server_create(vm, args),
    ),
    method(
        "createMBean",
        "(Ljava/lang/String;Ljavax/management/ObjectName;[Ljava/lang/Object;[Ljava/lang/String;)Ljavax/management/ObjectInstance;",
        ACC_PUBLIC,
        |vm, _, args| mbean_server_create(vm, args),
    ),
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
// -------------------------------------------------------------------------------------------
// Descriptor storage shared by DescriptorSupport and ImmutableDescriptor. Fields live in an
// insertion-ordered LinkedHashMap keyed by the original-case name; lookups, equals and hashCode
// are case-insensitive, matching the javax.management contract.

fn descriptor_map(vm: &mut Vm<'_>, this: ObjectRef) -> Result<ObjectRef, VmError> {
    let class = vm.class_of(this);
    if let Some(map) = vm.read_ref_field(this, class, "fields", "Ljava/util/LinkedHashMap;") {
        if !map.is_null() {
            return Ok(map);
        }
    }
    let map = crate::util::linked_hash_map_new(vm)?;
    let (declaring, field) = vm.find_field(class, "fields", "Ljava/util/LinkedHashMap;")?;
    vm.set_instance_ref(this, declaring, field, map);
    Ok(map)
}

fn descriptor_pairs(
    vm: &mut Vm<'_>,
    this: ObjectRef,
) -> Result<Vec<(ObjectRef, ObjectRef)>, VmError> {
    let map = descriptor_map(vm, this)?;
    Ok(crate::util::hash_map_pairs(vm, map))
}

fn descriptor_put(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: ObjectRef,
    value: ObjectRef,
) -> Result<(), VmError> {
    let map = descriptor_map(vm, this)?;
    let want = vm.string_or_empty(name).to_lowercase();
    let mut key = name;
    for (existing, _) in crate::util::hash_map_pairs(vm, map) {
        if vm.string_or_empty(existing).to_lowercase() == want {
            key = existing;
            break;
        }
    }
    crate::util::hash_map_insert(vm, map, key, value)?;
    Ok(())
}

fn descriptor_get(vm: &mut Vm<'_>, this: ObjectRef, name: &str) -> Result<ObjectRef, VmError> {
    let want = name.to_lowercase();
    for (key, value) in descriptor_pairs(vm, this)? {
        if vm.string_or_empty(key).to_lowercase() == want {
            return Ok(value);
        }
    }
    Ok(ObjectRef::NULL)
}

fn descriptor_contains(vm: &mut Vm<'_>, this: ObjectRef, name: &str) -> Result<bool, VmError> {
    let want = name.to_lowercase();
    for (key, _) in descriptor_pairs(vm, this)? {
        if vm.string_or_empty(key).to_lowercase() == want {
            return Ok(true);
        }
    }
    Ok(false)
}

fn descriptor_remove(vm: &mut Vm<'_>, this: ObjectRef, name: &str) -> Result<(), VmError> {
    let want = name.to_lowercase();
    let pairs = descriptor_pairs(vm, this)?;
    let fresh = crate::util::linked_hash_map_new(vm)?;
    for (key, value) in pairs {
        if vm.string_or_empty(key).to_lowercase() != want {
            crate::util::hash_map_insert(vm, fresh, key, value)?;
        }
    }
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "fields", "Ljava/util/LinkedHashMap;")?;
    vm.set_instance_ref(this, declaring, field, fresh);
    Ok(())
}

/// Java `String.hashCode` over the UTF-16 code units of `text`.
fn descriptor_hash_code(vm: &mut Vm<'_>, this: ObjectRef) -> Result<i32, VmError> {
    let mut hash = 0i32;
    for (key, value) in descriptor_pairs(vm, this)? {
        let name_hash = java_string_hash(&vm.string_or_empty(key).to_lowercase());
        let value_hash = crate::util::objects_deep_hash_code(vm, value)?;
        hash = hash.wrapping_add(name_hash ^ value_hash);
    }
    Ok(hash)
}

fn descriptor_equals(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<bool, VmError> {
    if a == b {
        return Ok(true);
    }
    if b.is_null() {
        return Ok(false);
    }
    let descriptor_class = vm.resolve_class("javax/management/Descriptor")?;
    if !vm.is_instance(b, descriptor_class) {
        return Ok(false);
    }
    let pairs_a = descriptor_pairs(vm, a)?;
    let pairs_b = descriptor_pairs(vm, b)?;
    if pairs_a.len() != pairs_b.len() {
        return Ok(false);
    }
    for (key, value) in pairs_a {
        let name = vm.string_or_empty(key);
        if !descriptor_contains(vm, b, &name)? {
            return Ok(false);
        }
        let other = descriptor_get(vm, b, &name)?;
        if !crate::util::objects_deep_equals(vm, value, other)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Shallow-copy an array value (defensive copy for immutable descriptors); non-arrays pass through.
fn descriptor_clone_array(vm: &mut Vm<'_>, array: ObjectRef) -> Result<ObjectRef, VmError> {
    if array.is_null() {
        return Ok(array);
    }
    let is_array = matches!(
        vm.heap.get(array).map(|object| &object.data),
        Some(oxjvm_vm::ObjectData::Array(_))
    );
    if !is_array {
        return Ok(array);
    }
    let class = vm.class_of(array);
    let len = vm.array_length(array)?;
    let copy = vm.allocate_array(class, len)?;
    for index in 0..len {
        let value = vm.array_get(array, index as i32)?;
        vm.array_set(copy, index as i32, value, false)?;
    }
    Ok(copy)
}

fn descriptor_field_names(vm: &mut Vm<'_>, this: ObjectRef) -> Result<ObjectRef, VmError> {
    let pairs = descriptor_pairs(vm, this)?;
    let string_class = vm.resolve_class("java/lang/String")?;
    let array = vm.allocate_object_array(string_class, pairs.len())?;
    for (index, (key, _)) in pairs.iter().enumerate() {
        vm.array_set_ref(array, index, *key)?;
    }
    Ok(array)
}

fn descriptor_field_values(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    names: ObjectRef,
    clone: bool,
) -> Result<ObjectRef, VmError> {
    let wanted: Vec<String> = if names.is_null() {
        descriptor_pairs(vm, this)?
            .into_iter()
            .map(|(key, _)| vm.string_or_empty(key))
            .collect()
    } else {
        let len = vm.array_length(names)?;
        let mut wanted_names = Vec::with_capacity(len);
        for index in 0..len {
            let name = vm.array_get(names, index as i32)?.as_ref();
            wanted_names.push(vm.string_or_empty(name));
        }
        wanted_names
    };
    let object_class = vm.resolve_class("java/lang/Object")?;
    let array = vm.allocate_object_array(object_class, wanted.len())?;
    for (index, name) in wanted.iter().enumerate() {
        let mut value = descriptor_get(vm, this, name)?;
        if clone {
            value = descriptor_clone_array(vm, value)?;
        }
        vm.array_set_ref(array, index, value)?;
    }
    Ok(array)
}

fn descriptor_to_string(vm: &mut Vm<'_>, this: ObjectRef) -> Result<String, VmError> {
    let mut out = String::from("{");
    for (index, (key, value)) in descriptor_pairs(vm, this)?.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push_str(&vm.string_or_empty(*key));
        out.push('=');
        if value.is_null() {
            out.push_str("null");
        } else {
            let (class, method) =
                vm.resolve_virtual_method(*value, "toString", "()Ljava/lang/String;")?;
            let text = vm.invoke_method(class, method, vec![Value::Ref(*value)])?;
            out.push_str(&vm.string_or_empty(text.as_ref()));
        }
    }
    out.push('}');
    Ok(out)
}

fn descriptor_init_from_arrays(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    names: ObjectRef,
    values: ObjectRef,
    clone: bool,
) -> Result<(), VmError> {
    if names.is_null() || values.is_null() {
        return Ok(());
    }
    let name_len = vm.array_length(names)?;
    let value_len = vm.array_length(values)?;
    if name_len != value_len {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("field names and values must have the same length"),
        ));
    }
    for index in 0..name_len {
        let name = vm.array_get(names, index as i32)?.as_ref();
        if name.is_null() {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("null field name"),
            ));
        }
        let mut value = vm.array_get(values, index as i32)?.as_ref();
        if clone {
            value = descriptor_clone_array(vm, value)?;
        }
        descriptor_put(vm, this, name, value)?;
    }
    Ok(())
}

fn descriptor_get_field_value(
    vm: &mut Vm<'_>,
    args: &[Value],
    clone: bool,
) -> Result<Value, VmError> {
    let this = receiver(args);
    let name = vm.string_or_empty(ref_arg(args, 1));
    let mut value = descriptor_get(vm, this, &name)?;
    if clone {
        value = descriptor_clone_array(vm, value)?;
    }
    object(value)
}

fn descriptor_get_field_values(
    vm: &mut Vm<'_>,
    args: &[Value],
    clone: bool,
) -> Result<Value, VmError> {
    object(descriptor_field_values(
        vm,
        receiver(args),
        ref_arg(args, 1),
        clone,
    )?)
}

/// Build an exception instance (without throwing) to use as a cause.
fn descriptor_make_cause(
    vm: &mut Vm<'_>,
    class_name: &str,
    message: &str,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class(class_name)?;
    let instance = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(class, "<init>", "(Ljava/lang/String;)V")
        .ok_or_else(|| VmError::internal("exception(String) missing"))?;
    let message = vm.make_string(message)?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![Value::Ref(instance), Value::Ref(message)],
    )?;
    Ok(instance)
}

/// Immutable descriptors reject mutation with a `RuntimeOperationsException`. Bad field names
/// (null/empty) wrap an `IllegalArgumentException`; otherwise the cause is `UnsupportedOperationException`.
fn immutable_descriptor_reject(vm: &mut Vm<'_>, names: ObjectRef) -> Result<Value, VmError> {
    let mut bad_name = false;
    if !names.is_null() {
        let len = vm.array_length(names)?;
        for index in 0..len {
            let name = vm.array_get(names, index as i32)?.as_ref();
            if name.is_null() || vm.string_or_empty(name).is_empty() {
                bad_name = true;
                break;
            }
        }
    }
    let (cause_class, message) = if bad_name {
        (
            "java/lang/IllegalArgumentException",
            "field name must not be null or empty",
        )
    } else {
        (
            "java/lang/UnsupportedOperationException",
            "this descriptor is immutable",
        )
    };
    let cause = descriptor_make_cause(vm, cause_class, message)?;
    Err(vm.throw_new_with_cause("javax/management/RuntimeOperationsException", cause))
}

fn immutable_descriptor_reject_name(vm: &mut Vm<'_>, name: ObjectRef) -> Result<Value, VmError> {
    let bad = name.is_null() || vm.string_or_empty(name).is_empty();
    let (cause_class, message) = if bad {
        (
            "java/lang/IllegalArgumentException",
            "field name must not be null or empty",
        )
    } else {
        (
            "java/lang/UnsupportedOperationException",
            "this descriptor is immutable",
        )
    };
    let cause = descriptor_make_cause(vm, cause_class, message)?;
    Err(vm.throw_new_with_cause("javax/management/RuntimeOperationsException", cause))
}

const DESCRIPTOR_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method(
        "getFieldValue",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getFieldNames",
        "()[Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getFieldValues",
        "([Ljava/lang/String;)[Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "setField",
        "(Ljava/lang/String;Ljava/lang/Object;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "setFields",
        "([Ljava/lang/String;[Ljava/lang/Object;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "removeField",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "isValid",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "isEmpty",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "clone",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "hashCode",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const DESCRIPTOR: oxjvm_vm::NativeClass = class(
    "javax/management/Descriptor",
    Some("java/lang/Object"),
    &["java/io/Serializable", "java/lang/Cloneable"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &DESCRIPTOR_METHODS,
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
const DESCRIPTOR_SUPPORT_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "<init>",
        "([Ljava/lang/String;[Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            descriptor_init_from_arrays(vm, this, ref_arg(args, 1), ref_arg(args, 2), false)?;
            void()
        },
    ),
    method(
        "getFieldValue",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| descriptor_get_field_value(vm, args, false),
    ),
    method(
        "getFieldNames",
        "()[Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(descriptor_field_names(vm, receiver(args))?),
    ),
    method(
        "getFieldValues",
        "([Ljava/lang/String;)[Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| descriptor_get_field_values(vm, args, false),
    ),
    method(
        "setField",
        "(Ljava/lang/String;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let name = ref_arg(args, 1);
            if name.is_null() {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("null field name"),
                ));
            }
            descriptor_put(vm, this, name, ref_arg(args, 2))?;
            void()
        },
    ),
    method(
        "setFields",
        "([Ljava/lang/String;[Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            descriptor_init_from_arrays(vm, this, ref_arg(args, 1), ref_arg(args, 2), false)?;
            void()
        },
    ),
    method(
        "removeField",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = vm.string_or_empty(ref_arg(args, 1));
            descriptor_remove(vm, receiver(args), &name)?;
            void()
        },
    ),
    method("isValid", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(descriptor_pairs(vm, receiver(args))?.is_empty())
    }),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| boolean(descriptor_equals(vm, receiver(args), ref_arg(args, 1))?),
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        int(descriptor_hash_code(vm, receiver(args))?)
    }),
    method(
        "clone",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let copy = vm.new_instance(class)?;
            for (key, value) in descriptor_pairs(vm, this)? {
                let value = descriptor_clone_array(vm, value)?;
                descriptor_put(vm, copy, key, value)?;
            }
            object(copy)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = descriptor_to_string(vm, receiver(args))?;
            object(vm.make_string(&text)?)
        },
    ),
];

pub(crate) const DESCRIPTOR_SUPPORT: oxjvm_vm::NativeClass = class(
    "javax/management/DescriptorSupport",
    Some("java/lang/Object"),
    &["javax/management/Descriptor"],
    ACC_PUBLIC,
    &[field(
        "fields",
        "Ljava/util/LinkedHashMap;",
        ACC_PRIVATE,
        None,
    )],
    &DESCRIPTOR_SUPPORT_METHODS,
    None,
);
const IMMUTABLE_DESCRIPTOR_METHODS: [oxjvm_vm::NativeMethodDef; 17] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "<init>",
        "([Ljava/lang/String;[Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            descriptor_init_from_arrays(vm, this, ref_arg(args, 1), ref_arg(args, 2), true)?;
            void()
        },
    ),
    method(
        "<init>",
        "([Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let fields = ref_arg(args, 1);
            if !fields.is_null() {
                let len = vm.array_length(fields)?;
                for index in 0..len {
                    let field = vm.array_get(fields, index as i32)?.as_ref();
                    let text = vm.string_or_empty(field);
                    let (name, value) = match text.split_once('=') {
                        Some((name, value)) => (name, value),
                        None => (text.as_str(), ""),
                    };
                    let name = vm.make_string(name)?;
                    let value = vm.make_string(value)?;
                    descriptor_put(vm, this, name, value)?;
                }
            }
            void()
        },
    ),
    method("<init>", "(Ljava/util/Map;)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let map = ref_arg(args, 1);
        if !map.is_null() {
            for (key, value) in crate::util::abstract_map_entries(vm, map)? {
                let value = descriptor_clone_array(vm, value)?;
                descriptor_put(vm, this, key, value)?;
            }
        }
        void()
    }),
    method(
        "union",
        "([Ljavax/management/Descriptor;)Ljavax/management/ImmutableDescriptor;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let descriptors = ref_arg(args, 0);
            let immutable_class = vm.resolve_class("javax/management/ImmutableDescriptor")?;
            let empty = vm
                .read_static_ref_if_set(
                    immutable_class,
                    "EMPTY_DESCRIPTOR",
                    "Ljavax/management/ImmutableDescriptor;",
                )
                .unwrap_or(ObjectRef::NULL);
            let mut inputs: Vec<ObjectRef> = Vec::new();
            let mut all_pairs: Vec<(ObjectRef, ObjectRef)> = Vec::new();
            let mut any_non_empty = false;
            if !descriptors.is_null() {
                let len = vm.array_length(descriptors)?;
                for index in 0..len {
                    let descriptor = vm.array_get(descriptors, index as i32)?.as_ref();
                    if descriptor.is_null() {
                        continue;
                    }
                    inputs.push(descriptor);
                    let pairs = descriptor_pairs(vm, descriptor)?;
                    if !pairs.is_empty() {
                        any_non_empty = true;
                    }
                    all_pairs.extend(pairs);
                }
            }
            if !any_non_empty {
                return object(empty);
            }
            let result = vm.new_instance(immutable_class)?;
            for (key, value) in &all_pairs {
                let value = descriptor_clone_array(vm, *value)?;
                descriptor_put(vm, result, *key, value)?;
            }
            for descriptor in &inputs {
                if vm.class_of(*descriptor) == immutable_class
                    && descriptor_equals(vm, result, *descriptor)?
                {
                    return object(*descriptor);
                }
            }
            object(result)
        },
    ),
    method(
        "getFieldValue",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| descriptor_get_field_value(vm, args, true),
    ),
    method(
        "getFieldNames",
        "()[Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(descriptor_field_names(vm, receiver(args))?),
    ),
    method(
        "getFieldValues",
        "([Ljava/lang/String;)[Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| descriptor_get_field_values(vm, args, true),
    ),
    method(
        "setField",
        "(Ljava/lang/String;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| immutable_descriptor_reject_name(vm, ref_arg(args, 1)),
    ),
    method(
        "setFields",
        "([Ljava/lang/String;[Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| immutable_descriptor_reject(vm, ref_arg(args, 1)),
    ),
    method(
        "removeField",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| immutable_descriptor_reject_name(vm, ref_arg(args, 1)),
    ),
    method("isValid", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(descriptor_pairs(vm, receiver(args))?.is_empty())
    }),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| boolean(descriptor_equals(vm, receiver(args), ref_arg(args, 1))?),
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        int(descriptor_hash_code(vm, receiver(args))?)
    }),
    method("clone", "()Ljava/lang/Object;", ACC_PUBLIC, |_, _, args| {
        object(receiver(args))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = descriptor_to_string(vm, receiver(args))?;
            object(vm.make_string(&text)?)
        },
    ),
];

pub(crate) const IMMUTABLE_DESCRIPTOR: oxjvm_vm::NativeClass = class(
    "javax/management/ImmutableDescriptor",
    Some("java/lang/Object"),
    &["javax/management/Descriptor"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field(
            "EMPTY_DESCRIPTOR",
            "Ljavax/management/ImmutableDescriptor;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field("fields", "Ljava/util/LinkedHashMap;", ACC_PRIVATE, None),
    ],
    &IMMUTABLE_DESCRIPTOR_METHODS,
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
pub(crate) const MODELMBEAN_DESCRIPTOR_SUPPORT: oxjvm_vm::NativeClass = class(
    "javax/management/modelmbean/DescriptorSupport",
    Some("java/lang/Object"),
    &["javax/management/Descriptor"],
    ACC_PUBLIC,
    &[field(
        "fields",
        "Ljava/util/LinkedHashMap;",
        ACC_PRIVATE,
        None,
    )],
    &DESCRIPTOR_SUPPORT_METHODS,
    None,
);

pub(crate) const RUNTIME_OPERATIONS_EXCEPTION: oxjvm_vm::NativeClass = class(
    "javax/management/RuntimeOperationsException",
    Some("java/lang/RuntimeException"),
    &[],
    ACC_PUBLIC,
    &[],
    &[
        method(
            "<init>",
            "()V",
            ACC_PUBLIC,
            crate::throwable::exception_ctor,
        ),
        method(
            "<init>",
            "(Ljava/lang/String;)V",
            ACC_PUBLIC,
            crate::throwable::exception_ctor,
        ),
        method(
            "<init>",
            "(Ljava/lang/Throwable;)V",
            ACC_PUBLIC,
            crate::throwable::exception_ctor,
        ),
    ],
    None,
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
