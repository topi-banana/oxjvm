//! Tier 2.3 shells: `java.lang.management.*` and collaborators.

use oxjvm_classfile::flags::*;
use crate::class;

pub(crate) const MANAGEMENT_FACTORY: oxjvm_vm::NativeClass = class(
    "java/lang/management/ManagementFactory",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &[],
    None,
);

const fn mxbean(name: &'static str) -> oxjvm_vm::NativeClass {
    class(name, Some("java/lang/Object"), &[], ACC_PUBLIC | ACC_INTERFACE, &[], &[], None)
}

pub(crate) const MX_BEAN: oxjvm_vm::NativeClass = class(
    "java/lang/management/MXBean",
    Some("java/lang/annotation/Annotation"),
    &[],
    ACC_PUBLIC | ACC_ANNOTATION | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const MEMORY_MX_BEAN: oxjvm_vm::NativeClass = mxbean("java/lang/management/MemoryMXBean");
pub(crate) const THREAD_MX_BEAN: oxjvm_vm::NativeClass = mxbean("java/lang/management/ThreadMXBean");
pub(crate) const RUNTIME_MX_BEAN: oxjvm_vm::NativeClass = mxbean("java/lang/management/RuntimeMXBean");
pub(crate) const OPERATING_SYSTEM_MX_BEAN: oxjvm_vm::NativeClass =
    mxbean("java/lang/management/OperatingSystemMXBean");
pub(crate) const CLASS_LOADING_MX_BEAN: oxjvm_vm::NativeClass =
    mxbean("java/lang/management/ClassLoadingMXBean");
pub(crate) const COMPILATION_MX_BEAN: oxjvm_vm::NativeClass =
    mxbean("java/lang/management/CompilationMXBean");
pub(crate) const GARBAGE_COLLECTOR_MX_BEAN: oxjvm_vm::NativeClass =
    mxbean("java/lang/management/GarbageCollectorMXBean");
pub(crate) const MEMORY_MANAGER_MX_BEAN: oxjvm_vm::NativeClass =
    mxbean("java/lang/management/MemoryManagerMXBean");
pub(crate) const MEMORY_POOL_MX_BEAN: oxjvm_vm::NativeClass = mxbean("java/lang/management/MemoryPoolMXBean");
pub(crate) const BUFFER_POOL_MX_BEAN: oxjvm_vm::NativeClass = mxbean("java/lang/management/BufferPoolMXBean");
pub(crate) const PLATFORM_LOGGING_MX_BEAN: oxjvm_vm::NativeClass =
    mxbean("java/lang/management/PlatformLoggingMXBean");
pub(crate) const PLATFORM_MANAGED_OBJECT: oxjvm_vm::NativeClass = mxbean("java/lang/management/PlatformManagedObject");

// memory support classes
pub(crate) const MEMORY_USAGE: oxjvm_vm::NativeClass = class(
    "java/lang/management/MemoryUsage",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
pub(crate) const MEMORY_TYPE: oxjvm_vm::NativeClass = class(
    "java/lang/management/MemoryType",
    Some("java/lang/Enum"),
    &[],
    ACC_PUBLIC | ACC_ENUM,
    &[],
    &[],
    None,
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
    ACC_PUBLIC | ACC_INTERFACE,
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
    &[],
    &[],
    None,
);
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
    ACC_PUBLIC | ACC_INTERFACE,
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

// MBean core
pub(crate) const MBEAN_SERVER: oxjvm_vm::NativeClass = class(
    "javax/management/MBeanServer",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE,
    &[],
    &[],
    None,
);
pub(crate) const MBEAN_SERVER_FACTORY: oxjvm_vm::NativeClass = class(
    "javax/management/MBeanServerFactory",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
pub(crate) const OBJECT_NAME: oxjvm_vm::NativeClass = class(
    "javax/management/ObjectName",
    Some("java/lang/Object"),
    &["javax/management/QueryExp", "java/io/Serializable"],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
pub(crate) const MBEAN_INFO: oxjvm_vm::NativeClass = class(
    "javax/management/MBeanInfo",
    Some("java/lang/Object"),
    &["java/io/Serializable", "javax/management/DescriptorRead"],
    ACC_PUBLIC,
    &[],
    &[],
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
pub(crate) const OBJECT_INSTANCE: oxjvm_vm::NativeClass = class(
    "javax/management/ObjectInstance",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
pub(crate) const ATTRIBUTE: oxjvm_vm::NativeClass = class(
    "javax/management/Attribute",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
pub(crate) const ATTRIBUTE_LIST: oxjvm_vm::NativeClass = class(
    "javax/management/AttributeList",
    Some("java/util/ArrayList"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
pub(crate) const STANDARD_MBEAN: oxjvm_vm::NativeClass = class(
    "javax/management/StandardMBean",
    Some("java/lang/Object"),
    &["javax/management/DynamicMBean", "javax/management/MBeanRegistration"],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
pub(crate) const DYNAMIC_MBEAN: oxjvm_vm::NativeClass = class(
    "javax/management/DynamicMBean",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE,
    &[],
    &[],
    None,
);
pub(crate) const MBEAN_REGISTRATION: oxjvm_vm::NativeClass = class(
    "javax/management/MBeanRegistration",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE,
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
    ACC_PUBLIC | ACC_INTERFACE,
    &[],
    &[],
    None,
);
pub(crate) const NOTIFICATION_EMITTER: oxjvm_vm::NativeClass = class(
    "javax/management/NotificationEmitter",
    Some("javax/management/NotificationBroadcaster"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE,
    &[],
    &[],
    None,
);
pub(crate) const NOTIFICATION: oxjvm_vm::NativeClass = class(
    "javax/management/Notification",
    Some("java/util/EventObject"),
    &[],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
pub(crate) const NOTIFICATION_BROADCASTER_SUPPORT: oxjvm_vm::NativeClass = class(
    "javax/management/NotificationBroadcasterSupport",
    Some("java/lang/Object"),
    &["javax/management/NotificationEmitter"],
    ACC_PUBLIC,
    &[],
    &[],
    None,
);
pub(crate) const DESCRIPTOR: oxjvm_vm::NativeClass = class(
    "javax/management/Descriptor",
    Some("java/io/Serializable"),
    &["java/lang/Cloneable"],
    ACC_PUBLIC | ACC_INTERFACE,
    &[],
    &[],
    None,
);
pub(crate) const DESCRIPTOR_READ: oxjvm_vm::NativeClass = class(
    "javax/management/DescriptorRead",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE,
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
    &[],
    &[],
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