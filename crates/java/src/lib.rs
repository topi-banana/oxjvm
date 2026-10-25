#![no_std]
#![deny(unsafe_code)]
//! The native `java.*` class library for oxjvm.
//!
//! Every class here is a real runtime class: the loader registers it under its internal name, and
//! the verifier, resolver, and dispatcher treat it exactly like a class read from a `.class` file.
//! The difference is that method bodies are Rust functions receiving the VM, the invocation
//! context, and the arguments (the receiver first for instance methods), and returning a [`Value`].
//!
//! [`natives`] is the complete registry; [`oxjvm_vm::Vm::new`] must be given it (or a superset) so
//! that `java/lang/Object` exists before anything is loaded.

extern crate alloc;

#[cfg(test)]
extern crate std;

pub(crate) mod annotation;
pub(crate) mod atomic;
pub(crate) mod beans;
pub(crate) mod boxed;
pub(crate) mod buffer;
pub(crate) mod charset;
pub(crate) mod concurrent;
pub(crate) mod encoding;
pub(crate) mod executor;
pub(crate) mod file;
pub(crate) mod format;
pub(crate) mod intstream;
pub(crate) mod invoke;
pub(crate) mod io;
pub(crate) mod jar;
pub(crate) mod lang;
pub(crate) mod locale;
pub(crate) mod logging;
pub(crate) mod math;
pub(crate) mod net;
pub(crate) mod nio;
pub(crate) mod optional;
pub(crate) mod primitive_stream;
pub(crate) mod process;
pub(crate) mod process_handle;
pub(crate) mod reference;
pub(crate) mod reflect;
pub(crate) mod regex;
pub(crate) mod serialization;
pub(crate) mod service_loader;
pub(crate) mod stack;
pub(crate) mod stream;
pub(crate) mod text;
pub(crate) mod throwable;
pub(crate) mod time;
pub(crate) mod tree;
pub(crate) mod unsafe_api;
pub(crate) mod uri;
pub(crate) mod util;
pub(crate) mod uuid;
pub(crate) mod zip;

use alloc::string::String;

use oxjvm_vm::{
    NativeClass, NativeClinit, NativeConstant, NativeFieldDef, NativeFn, NativeMethodDef,
    ObjectRef, Value, Vm, VmError,
};

/// Build a [`NativeMethodDef`].
#[must_use]
pub(crate) const fn method(
    name: &'static str,
    descriptor: &'static str,
    access_flags: u16,
    native: NativeFn,
) -> NativeMethodDef {
    NativeMethodDef {
        name,
        descriptor,
        access_flags,
        native,
    }
}

/// Build a [`NativeFieldDef`].
#[must_use]
pub(crate) const fn field(
    name: &'static str,
    descriptor: &'static str,
    access_flags: u16,
    constant: Option<NativeConstant>,
) -> NativeFieldDef {
    NativeFieldDef {
        name,
        descriptor,
        access_flags,
        constant,
    }
}

/// Build a [`NativeClass`].
#[must_use]
pub(crate) const fn class(
    name: &'static str,
    super_name: Option<&'static str>,
    interfaces: &'static [&'static str],
    access_flags: u16,
    fields: &'static [NativeFieldDef],
    methods: &'static [NativeMethodDef],
    clinit: Option<NativeClinit>,
) -> NativeClass {
    NativeClass {
        name,
        super_name,
        interfaces,
        access_flags,
        fields,
        methods,
        clinit,
    }
}

/// The complete native registry, loaded on demand by the runtime.
#[must_use]
pub fn natives() -> &'static [NativeClass] {
    &REGISTRY
}

/// Whether a `-D` override for this system property has to be dropped.
///
/// JEP 223 pins the platform properties (the VM name and vendor, the specification name, …) to
/// the values the platform reports, so a launcher that lets `-D` replace one of them lies about
/// which VM it is.  A host that parses `-D` options asks the runtime here before applying one.
#[must_use]
pub fn is_non_writable_property(key: &str) -> bool {
    lang::is_non_writable_property(key)
}

/// The registry itself, ordered so that `java/lang/Object` is first.
static REGISTRY: [NativeClass; 538] = [
    lang::OBJECT,
    lang::CLASS,
    lang::PACKAGE,
    lang::STRING,
    lang::STRING_BUILDER,
    lang::STRING_BUFFER,
    lang::SYSTEM,
    lang::SYSTEM_LOGGER,
    lang::SYSTEM_LOGGER_IMPL,
    lang::SYSTEM_LOGGER_LEVEL,
    lang::RUNTIME,
    lang::MODULE,
    lang::MODULE_LAYER,
    lang::PROCESS_BUILDER,
    lang::PROCESS_BUILDER_REDIRECT,
    format::FORMAT,
    format::FIELD_POSITION,
    format::NUMBER_FORMAT,
    format::OJVM_NUMBER_FORMAT,
    format::DATE_FORMAT,
    format::OJVM_DATE_FORMAT,
    format::CHOICE_FORMAT,
    format::MESSAGE_FORMAT,
    format::PARSE_EXCEPTION,
    format::PARSE_POSITION,
    format::NUMBER_FORMAT_STYLE,
    format::DECIMAL_FORMAT,
    format::DECIMAL_FORMAT_SYMBOLS,
    lang::REDIRECT_TYPE,
    lang::OJVM_REDIRECT,
    process::PROCESS,
    process::OJVM_PROCESS,
    process_handle::PROCESS_HANDLE,
    process_handle::OJVM_PROCESS_HANDLE,
    process_handle::PROCESS_HANDLE_INFO,
    process_handle::OJVM_PROCESS_HANDLE_INFO,
    lang::RUNTIME_VERSION,
    lang::SWITCH_BOOTSTRAPS,
    lang::VALUE_LAYOUT,
    lang::THREAD,
    lang::THREAD_GROUP,
    lang::THREAD_STATE,
    lang::THREAD_LOCAL,
    lang::INHERITABLE_THREAD_LOCAL,
    lang::CLASS_LOADER,
    lang::ENUM,
    lang::RECORD,
    lang::CHAR_SEQUENCE,
    lang::COMPARABLE,
    lang::RUNNABLE,
    lang::CLONEABLE,
    lang::SERIALIZABLE,
    lang::ITERABLE,
    lang::AUTO_CLOSEABLE,
    lang::COMPARATOR,
    lang::STACK_TRACE_ELEMENT,
    throwable::THROWABLE,
    throwable::ERROR,
    throwable::VIRTUAL_MACHINE_ERROR,
    throwable::OUT_OF_MEMORY_ERROR,
    throwable::STACK_OVERFLOW_ERROR,
    throwable::INTERNAL_ERROR,
    throwable::ASSERTION_ERROR,
    throwable::LINKAGE_ERROR,
    throwable::BOOTSTRAP_METHOD_ERROR,
    throwable::CLASS_CIRCULARITY_ERROR,
    throwable::EXCEPTION_IN_INITIALIZER_ERROR,
    throwable::INCOMPATIBLE_CLASS_CHANGE_ERROR,
    throwable::ABSTRACT_METHOD_ERROR,
    throwable::ILLEGAL_ACCESS_ERROR,
    throwable::INSTANTIATION_ERROR,
    throwable::NO_SUCH_FIELD_ERROR,
    throwable::NO_SUCH_METHOD_ERROR,
    throwable::NO_CLASS_DEF_FOUND_ERROR,
    throwable::UNSATISFIED_LINK_ERROR,
    throwable::VERIFY_ERROR,
    throwable::EXCEPTION,
    throwable::REFLECTIVE_OPERATION_EXCEPTION,
    throwable::INSTANTIATION_EXCEPTION,
    throwable::INVOCATION_TARGET_EXCEPTION,
    throwable::RUNTIME_EXCEPTION,
    throwable::CANCELLATION_EXCEPTION,
    throwable::MISSING_RESOURCE_EXCEPTION,
    throwable::TYPE_NOT_PRESENT_EXCEPTION,
    throwable::ENUM_CONSTANT_NOT_PRESENT_EXCEPTION,
    throwable::ARITHMETIC_EXCEPTION,
    throwable::ARRAY_STORE_EXCEPTION,
    throwable::CLASS_CAST_EXCEPTION,
    throwable::ILLEGAL_ARGUMENT_EXCEPTION,
    throwable::SECURITY_EXCEPTION,
    throwable::ILLEGAL_ACCESS_EXCEPTION,
    throwable::REFLECTIVE_OPERATION_EXCEPTION,
    throwable::NO_SUCH_FIELD_EXCEPTION,
    throwable::NO_SUCH_METHOD_EXCEPTION,
    throwable::FILE_SYSTEM_EXCEPTION,
    throwable::NO_SUCH_FILE_EXCEPTION,
    throwable::ACCESS_DENIED_EXCEPTION,
    throwable::NOT_DIRECTORY_EXCEPTION,
    throwable::DIRECTORY_NOT_EMPTY_EXCEPTION,
    throwable::GENERAL_SECURITY_EXCEPTION,
    throwable::NO_SUCH_ALGORITHM_EXCEPTION,
    throwable::NO_SUCH_PROVIDER_EXCEPTION,
    throwable::KEY_EXCEPTION,
    throwable::INVALID_KEY_EXCEPTION,
    throwable::UNRECOVERABLE_KEY_EXCEPTION,
    throwable::MALFORMED_URL_EXCEPTION,
    throwable::KEY_STORE_EXCEPTION,
    throwable::KEY_MANAGEMENT_EXCEPTION,
    throwable::INVALID_ALGORITHM_PARAMETER_EXCEPTION,
    throwable::SIGNATURE_EXCEPTION,
    throwable::CERTIFICATE_EXCEPTION,
    throwable::CERTIFICATE_EXPIRED_EXCEPTION,
    throwable::CERTIFICATE_NOT_YET_VALID_EXCEPTION,
    throwable::ZIP_EXCEPTION,
    throwable::UNKNOWN_HOST_EXCEPTION,
    throwable::DATA_FORMAT_EXCEPTION,
    throwable::ZIP_CONSTANTS,
    throwable::MATCH_EXCEPTION,
    throwable::FILE_NOT_FOUND_EXCEPTION,
    throwable::UNSUPPORTED_CHARSET_EXCEPTION,
    throwable::ILLEGAL_MONITOR_STATE_EXCEPTION,
    throwable::ILLEGAL_STATE_EXCEPTION,
    throwable::INDEX_OUT_OF_BOUNDS_EXCEPTION,
    throwable::ARRAY_INDEX_OUT_OF_BOUNDS_EXCEPTION,
    throwable::STRING_INDEX_OUT_OF_BOUNDS_EXCEPTION,
    throwable::NEGATIVE_ARRAY_SIZE_EXCEPTION,
    throwable::NULL_POINTER_EXCEPTION,
    throwable::UNSUPPORTED_OPERATION_EXCEPTION,
    throwable::REJECTED_EXECUTION_EXCEPTION,
    throwable::TIMEOUT_EXCEPTION,
    throwable::EXECUTION_EXCEPTION,
    throwable::FILE_SYSTEM_NOT_FOUND_EXCEPTION,
    throwable::PROVIDER_NOT_FOUND_EXCEPTION,
    throwable::FILE_SYSTEM_ALREADY_EXISTS_EXCEPTION,
    throwable::MALFORMED_INPUT_EXCEPTION,
    throwable::UNMAPPABLE_CHARACTER_EXCEPTION,
    throwable::CODER_MALFUNCTION_ERROR,
    throwable::BUFFER_OVERFLOW_EXCEPTION,
    throwable::BUFFER_UNDERFLOW_EXCEPTION,
    throwable::INVALID_MARK_EXCEPTION,
    throwable::READ_ONLY_BUFFER_EXCEPTION,
    throwable::NUMBER_FORMAT_EXCEPTION,
    throwable::CLASS_NOT_FOUND_EXCEPTION,
    throwable::CLONE_NOT_SUPPORTED_EXCEPTION,
    throwable::INTERRUPTED_EXCEPTION,
    buffer::BUFFER,
    buffer::BYTE_BUFFER,
    buffer::OJVM_BYTE_BUFFER,
    buffer::BYTE_ORDER,
    buffer::CHAR_BUFFER,
    buffer::OJVM_CHAR_BUFFER,
    math::MATH,
    math::STRICT_MATH,
    math::BIG_INTEGER,
    math::BIG_DECIMAL,
    boxed::NUMBER,
    boxed::INTEGER,
    boxed::LONG,
    boxed::SHORT,
    boxed::BYTE,
    boxed::BOOLEAN,
    boxed::CHARACTER,
    boxed::UNICODE_BLOCK,
    boxed::FLOAT,
    boxed::DOUBLE,
    boxed::VOID,
    io::IO_EXCEPTION,
    io::FILE_NOT_FOUND_EXCEPTION,
    io::UNSUPPORTED_ENCODING_EXCEPTION,
    io::EOF_EXCEPTION,
    io::CLOSEABLE,
    io::FLUSHABLE,
    io::OUTPUT_STREAM,
    io::FILTER_OUTPUT_STREAM,
    io::PRINT_STREAM,
    io::INPUT_STREAM,
    io::BYTE_ARRAY_INPUT_STREAM,
    io::BYTE_ARRAY_OUTPUT_STREAM,
    io::READER,
    io::WRITER,
    io::PRINT_WRITER,
    io::FILE_WRITER,
    io::FILE_READER,
    io::INPUT_STREAM_READER,
    io::BUFFERED_READER,
    io::BUFFERED_WRITER,
    io::OBJECT_STREAM_CLASS,
    serialization::OBJECT_OUTPUT_STREAM,
    serialization::OBJECT_INPUT_STREAM,
    serialization::PUT_FIELD,
    serialization::OBJECT_INPUT_VALIDATION,
    serialization::OBJECT_INPUT_FILTER,
    serialization::OBJECT_INPUT_FILTER_STATUS,
    io::OBJECT_STREAM_FIELD,
    io::STRING_READER,
    io::OUTPUT_STREAM_WRITER,
    io::STRING_WRITER,
    util::NO_SUCH_ELEMENT_EXCEPTION,
    util::EMPTY_STACK_EXCEPTION,
    throwable::SERVICE_CONFIGURATION_ERROR,
    util::ITERATOR,
    util::COLLECTION,
    util::LIST,
    util::SET,
    util::ABSTRACT_SET,
    util::MAP,
    util::ABSTRACT_MAP,
    util::OBJECTS,
    locale::LOCALE,
    optional::OPTIONAL,
    util::ARRAYS,
    util::RANDOM_ACCESS,
    util::RANDOM,
    util::ARRAY_LIST,
    util::ARRAY_LIST_ITERATOR,
    util::QUEUE,
    util::DEQUE,
    util::ARRAY_DEQUE,
    util::CONCURRENT_LINKED_DEQUE,
    util::ARRAY_LIST_LIST_ITERATOR,
    util::LIST_ITERATOR,
    util::LINKED_LIST,
    util::COPY_ON_WRITE_ARRAY_LIST,
    util::CONCURRENT_LINKED_QUEUE,
    util::PROPERTIES,
    uuid::UUID,
    util::HASH_MAP,
    util::IDENTITY_HASH_MAP,
    util::ENUMERATION,
    util::OJVM_ENUMERATION,
    util::STRING_TOKENIZER,
    util::HASH_SET,
    tree::TREE_SET,
    util::LINKED_HASH_SET,
    util::BIT_SET,
    util::HASHTABLE,
    util::VECTOR,
    util::STACK,
    util::MAP_ENTRY,
    util::OJVM_ENTRY,
    util::CONCURRENT_MAP,
    util::CONCURRENT_HASH_MAP,
    util::CONCURRENT_HASH_MAP_KEY_SET,
    regex::PATTERN_PREDICATE,
    net::INET_ADDRESS,
    net::INET4_ADDRESS,
    net::URL,
    net::URL_CONNECTION,
    net::STANDARD_PROTOCOL_FAMILY,
    net::NETWORK_INTERFACE,
    net::URL_ENCODER,
    net::URL_DECODER,
    beans::FEATURE_DESCRIPTOR,
    beans::EXCEPTION_LISTENER,
    annotation::ELEMENT_TYPE,
    annotation::RETENTION_POLICY,
    annotation::RETENTION,
    annotation::TARGET,
    annotation::DEPRECATED,
    annotation::REPEATABLE,
    annotation::DOCUMENTED,
    annotation::INHERITED,
    annotation::INCOMPLETE_ANNOTATION_EXCEPTION,
    annotation::ANNOTATION_TYPE_MISMATCH_EXCEPTION,
    annotation::ANNOTATION_VALUE_ERROR,
    encoding::BASE64,
    encoding::BASE64_ENCODER,
    encoding::BASE64_DECODER,
    encoding::HEX_FORMAT,
    lang::THREAD_BUILDER,
    lang::THREAD_BUILDER_OF_VIRTUAL,
    lang::THREAD_BUILDER_OF_PLATFORM,
    lang::OJVM_THREAD_BUILDER,
    lang::CASE_INSENSITIVE_COMPARATOR,
    util::ENUM_MAP,
    util::LINKED_HASH_MAP,
    util::ENUM_SET,
    util::TIME_ZONE,
    util::SCANNER,
    util::DATE,
    util::CURRENCY,
    file::FILE_FILTER,
    invoke::THREAD_FACTORY,
    invoke::VAR_HANDLE,
    invoke::ACCESS_MODE,
    invoke::DOUBLE_CONSUMER,
    invoke::DOUBLE_PREDICATE,
    invoke::DOUBLE_UNARY_OPERATOR,
    invoke::DOUBLE_FUNCTION,
    invoke::DOUBLE_BINARY_OPERATOR,
    invoke::DOUBLE_TO_INT_FUNCTION,
    invoke::DOUBLE_TO_LONG_FUNCTION,
    invoke::LONG_CONSUMER,
    invoke::LONG_PREDICATE,
    invoke::LONG_UNARY_OPERATOR,
    invoke::LONG_FUNCTION,
    invoke::LONG_BINARY_OPERATOR,
    invoke::LONG_TO_INT_FUNCTION,
    invoke::LONG_TO_DOUBLE_FUNCTION,
    invoke::INT_TO_LONG_FUNCTION,
    invoke::INT_TO_DOUBLE_FUNCTION,
    invoke::BOOLEAN_SUPPLIER,
    invoke::INT_SUPPLIER,
    invoke::LONG_SUPPLIER,
    invoke::DOUBLE_SUPPLIER,
    invoke::OJVM_IDENTITY,
    invoke::OJVM_FUNCTION,
    invoke::PREDICATE_COMBINER,
    lang::ANNOTATION,
    stack::STACK_WALKER,
    stack::STACK_WALKER_OPTION,
    stack::STACK_FRAME,
    stack::OJVM_STACK_FRAME,
    text::BREAK_ITERATOR,
    text::SIMPLE_DATE_FORMAT,
    text::CHARACTER_ITERATOR,
    text::STRING_CHARACTER_ITERATOR,
    time::DURATION,
    time::DATE_TIME_FORMATTER,
    time::ZONED_DATE_TIME,
    time::INSTANT,
    util::SORTED_MAP,
    util::NAVIGABLE_MAP,
    tree::TREE_MAP,
    tree::OXJVM_ENTRY_ORDER,
    tree::OXJVM_REVERSED_ORDER,
    intstream::INT_STREAM,
    primitive_stream::DOUBLE_STREAM,
    primitive_stream::OJVM_DOUBLE_STREAM,
    primitive_stream::LONG_STREAM,
    primitive_stream::OJVM_LONG_STREAM,
    primitive_stream::OPTIONAL_DOUBLE,
    primitive_stream::OPTIONAL_LONG,
    intstream::OJVM_INT_STREAM,
    stream::BASE_STREAM,
    stream::STREAM,
    stream::SPLITERATOR,
    stream::OXJVM_STREAM,
    stream::OJVM_SPLITERATOR,
    stream::STREAM_SUPPORT,
    stream::STREAM_ITERATOR,
    stream::COLLECTOR,
    stream::COLLECTORS,
    stream::OJVM_COLLECTOR,
    logging::LEVEL,
    logging::LOG_RECORD,
    logging::RESOURCE_BUNDLE,
    logging::LIST_RESOURCE_BUNDLE,
    logging::FILTER,
    logging::FORMATTER,
    logging::SIMPLE_FORMATTER,
    logging::ERROR_MANAGER,
    logging::HANDLER,
    logging::STREAM_HANDLER,
    logging::CONSOLE_HANDLER,
    logging::LOGGER,
    logging::LOG_MANAGER,
    logging::LOGGING_MXBEAN,
    logging::LOGGING_MXBEAN_IMPL,
    util::COLLECTIONS,
    util::REVERSE_COMPARATOR,
    util::FUNCTION_COMPARATOR,
    util::CHAINED_COMPARATOR,
    service_loader::SERVICE_LOADER,
    service_loader::SERVICE_LOADER_PROVIDER,
    service_loader::OJVM_PROVIDER,
    invoke::METHOD_TYPE,
    invoke::METHOD_HANDLE,
    invoke::METHOD_HANDLES,
    invoke::LOOKUP,
    invoke::LAMBDA_METAFACTORY,
    invoke::STRING_CONCAT_FACTORY,
    invoke::OBJECT_METHODS,
    invoke::TYPE_DESCRIPTOR,
    invoke::FUNCTION,
    invoke::BIFUNCTION,
    invoke::CONSUMER,
    invoke::BICONSUMER,
    invoke::SUPPLIER,
    invoke::PREDICATE,
    invoke::BIPREDICATE,
    invoke::UNARY_OPERATOR,
    invoke::BINARY_OPERATOR,
    invoke::CALLABLE,
    invoke::INT_CONSUMER,
    invoke::INT_PREDICATE,
    invoke::INT_UNARY_OPERATOR,
    invoke::INT_FUNCTION,
    invoke::TO_INT_FUNCTION,
    invoke::TO_LONG_FUNCTION,
    invoke::TO_DOUBLE_FUNCTION,
    invoke::APPENDABLE,
    regex::PATTERN,
    regex::MATCHER,
    regex::MATCH_RESULT,
    regex::PATTERN_SYNTAX_EXCEPTION,
    file::FILE,
    file::FILE_INPUT_STREAM,
    file::FILE_OUTPUT_STREAM,
    zip::ZIP_ENTRY,
    jar::ATTRIBUTES,
    jar::ATTRIBUTES_NAME,
    jar::MANIFEST,
    jar::JAR_ENTRY,
    jar::JAR_OUTPUT_STREAM,
    zip::ZIP_OUTPUT_STREAM,
    zip::CRC32,
    zip::CHECKSUM,
    nio::PATH,
    nio::OXJVM_PATH,
    nio::PATHS,
    nio::FILES,
    nio::FILE_SYSTEM,
    nio::OJVM_FILE_SYSTEM,
    nio::FILE_SYSTEMS,
    nio::FILE_STORE,
    nio::OJVM_FILE_STORE,
    nio::FILE_SYSTEM_PROVIDER,
    nio::OJVM_FILE_SYSTEM_PROVIDER,
    nio::OJVM_DIRECTORY_STREAM,
    nio::OJVM_PATH_ITERATOR,
    nio::FILE_CHANNEL,
    nio::OJVM_FILE_CHANNEL,
    nio::BYTE_CHANNEL,
    nio::READABLE_BYTE_CHANNEL,
    nio::WRITABLE_BYTE_CHANNEL,
    nio::CHANNEL,
    nio::COPY_OPTION,
    nio::OPEN_OPTION,
    nio::LINK_OPTION,
    nio::STANDARD_COPY_OPTION,
    nio::FILE_TIME,
    nio::BASIC_FILE_ATTRIBUTES,
    nio::OJVM_BASIC_FILE_ATTRIBUTES,
    nio::FILE_VISIT_OPTION,
    nio::DIRECTORY_STREAM,
    nio::DIRECTORY_STREAM_FILTER,
    nio::FILENAME_FILTER,
    nio::STANDARD_OPEN_OPTION,
    reflect::MEMBER,
    reflect::ANNOTATED_ELEMENT,
    reflect::TYPE,
    reflect::PARAMETERIZED_TYPE,
    reflect::GENERIC_ARRAY_TYPE,
    reflect::TYPE_VARIABLE,
    reflect::WILDCARD_TYPE,
    reflect::GENERIC_DECLARATION,
    reflect::PARAMETERIZED_TYPE_IMPL_CLASS,
    reflect::TYPE_VARIABLE_IMPL_CLASS,
    reflect::GENERIC_ARRAY_TYPE_IMPL_CLASS,
    reflect::WILDCARD_TYPE_IMPL_CLASS,
    reflect::INVOCATION_HANDLER,
    reflect::PROXY,
    reflect::MODIFIER,
    reflect::ACCESSIBLE_OBJECT,
    reflect::EXECUTABLE,
    reflect::METHOD,
    reflect::FIELD,
    reflect::CONSTRUCTOR,
    reflect::PARAMETER,
    reflect::REFLECT_ARRAY,
    reflect::PERMISSION_COLLECTION,
    reflect::PERMISSIONS,
    reflect::PROTECTION_DOMAIN,
    reflect::ANNOTATED_TYPE,
    reflect::OJVM_ANNOTATED_TYPE,
    reflect::RECORD_COMPONENT,
    reflect::ACCESS_FLAG,
    reflect::ACCESS_FLAG_LOCATION,
    reflect::REFLECT_PERMISSION,
    reference::REFERENCE,
    reference::WEAK_REFERENCE,
    reference::SOFT_REFERENCE,
    reference::PHANTOM_REFERENCE,
    reference::REFERENCE_QUEUE,
    beans::PROPERTY_DESCRIPTOR,
    beans::PROPERTY_EDITOR,
    beans::PROPERTY_EDITOR_SUPPORT,
    beans::OJVM_PROPERTY_EDITOR,
    beans::PROPERTY_EDITOR_MANAGER,
    beans::BEAN_INFO,
    beans::OJVM_BEAN_INFO,
    beans::INTROSPECTOR,
    beans::PROPERTY_CHANGE_EVENT,
    beans::PROPERTY_CHANGE_LISTENER,
    beans::VETOABLE_CHANGE_LISTENER,
    beans::PROPERTY_CHANGE_SUPPORT,
    beans::VETOABLE_CHANGE_SUPPORT,
    beans::PROPERTY_VETO_EXCEPTION,
    beans::BEAN_DESCRIPTOR,
    beans::METHOD_DESCRIPTOR,
    beans::PARAMETER_DESCRIPTOR,
    beans::EVENT_SET_DESCRIPTOR,
    beans::SIMPLE_BEAN_INFO,
    beans::BEANS,
    beans::INTROSPECTION_EXCEPTION,
    uri::URI,
    unsafe_api::SUN_MISC_UNSAFE,
    unsafe_api::JDK_INTERNAL_UNSAFE,
    charset::CHARSET,
    charset::CODER_RESULT,
    charset::CHARSET_ENCODER,
    charset::OJVM_CHARSET_ENCODER,
    charset::CHARSET_DECODER,
    charset::OJVM_CHARSET_DECODER,
    charset::STANDARD_CHARSETS,
    nio::FILE_ATTRIBUTE,
    atomic::ATOMIC_INTEGER,
    atomic::ATOMIC_LONG,
    atomic::ATOMIC_BOOLEAN,
    atomic::ATOMIC_REFERENCE,
    concurrent::TIME_UNIT,
    executor::FUTURE,
    executor::RUNNABLE_FUTURE,
    executor::SCHEDULED_FUTURE,
    executor::FUTURE_TASK,
    executor::EXECUTOR,
    executor::EXECUTOR_SERVICE,
    executor::SCHEDULED_EXECUTOR_SERVICE,
    executor::OJVM_EXECUTOR_SERVICE,
    executor::EXECUTORS,
    executor::OJVM_CALLABLE,
    executor::OJVM_THREAD_FACTORY,
    executor::THREAD_LOCAL_RANDOM,
    concurrent::COUNT_DOWN_LATCH,
    concurrent::CYCLIC_BARRIER,
    concurrent::CONDITION,
    concurrent::CONDITION_OBJECT,
    concurrent::READ_WRITE_LOCK,
    concurrent::NOOP_LOCK,
    concurrent::FUTURE,
    concurrent::REENTRANT_READ_WRITE_LOCK,
    concurrent::COMPLETABLE_FUTURE,
    concurrent::REENTRANT_LOCK,
    concurrent::LOCK_SUPPORT,
    concurrent::LOCK,
    util::SORTED_SET,
    util::NAVIGABLE_SET,
];

/// A read-only view of a string argument.
///
/// # Errors
///
/// Throws `NullPointerException` for a null argument, like the JDK.
pub(crate) fn string_arg(vm: &mut Vm<'_>, value: Value) -> Result<String, VmError> {
    let reference = value.as_ref();
    if reference.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    vm.string_value(reference)
        .ok_or_else(|| vm.throw_new("java/lang/NullPointerException", None))
}

/// The receiver of an instance call.
pub(crate) fn receiver(args: &[Value]) -> ObjectRef {
    args.first()
        .copied()
        .unwrap_or(Value::Ref(ObjectRef::NULL))
        .as_ref()
}

/// An `int` argument.
pub(crate) fn int_arg(args: &[Value], index: usize) -> i32 {
    args[index].as_int()
}

/// A `long` argument.
pub(crate) fn long_arg(args: &[Value], index: usize) -> i64 {
    args[index].as_long()
}

/// A `float` argument.
pub(crate) fn float_arg(args: &[Value], index: usize) -> f32 {
    args[index].as_float()
}

/// A `double` argument.
pub(crate) fn double_arg(args: &[Value], index: usize) -> f64 {
    args[index].as_double()
}

/// A reference argument.
pub(crate) fn ref_arg(args: &[Value], index: usize) -> ObjectRef {
    args[index].as_ref()
}

/// A `boolean` argument.
pub(crate) fn bool_arg(args: &[Value], index: usize) -> bool {
    args[index].as_int() != 0
}

/// `void` return.
pub(crate) fn void() -> Result<Value, VmError> {
    Ok(Value::Int(0))
}

/// An `int` return.
pub(crate) fn int(value: i32) -> Result<Value, VmError> {
    Ok(Value::Int(value))
}

/// A `long` return.
pub(crate) fn long(value: i64) -> Result<Value, VmError> {
    Ok(Value::Long(value))
}

/// A `float` return.
pub(crate) fn float(value: f32) -> Result<Value, VmError> {
    Ok(Value::Float(value))
}

/// A `double` return.
pub(crate) fn double(value: f64) -> Result<Value, VmError> {
    Ok(Value::Double(value))
}

/// A `boolean` return.
pub(crate) fn boolean(value: bool) -> Result<Value, VmError> {
    Ok(Value::Int(i32::from(value)))
}

/// A reference return.
pub(crate) fn object(reference: ObjectRef) -> Result<Value, VmError> {
    Ok(Value::Ref(reference))
}

/// Throw `UnsupportedOperationException` with a method name.
pub(crate) fn unsupported(vm: &mut Vm<'_>, what: &str) -> VmError {
    vm.throw_new(
        "java/lang/UnsupportedOperationException",
        Some(&alloc::format!("oxjvm does not implement {what}")),
    )
}

/// The dotted binary name of a class.
pub(crate) fn binary_name(vm: &Vm<'_>, class: oxjvm_vm::ClassId) -> String {
    vm.class_name(class).replace('/', ".")
}
