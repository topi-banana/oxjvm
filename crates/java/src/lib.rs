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
pub(crate) mod charset;
pub(crate) mod concurrent;
pub(crate) mod encoding;
pub(crate) mod file;
pub(crate) mod intstream;
pub(crate) mod invoke;
pub(crate) mod io;
pub(crate) mod lang;
pub(crate) mod locale;
pub(crate) mod logging;
pub(crate) mod math;
pub(crate) mod net;
pub(crate) mod nio;
pub(crate) mod optional;
pub(crate) mod reflect;
pub(crate) mod regex;
pub(crate) mod service_loader;
pub(crate) mod stack;
pub(crate) mod stream;
pub(crate) mod throwable;
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

/// The registry itself, ordered so that `java/lang/Object` is first.
static REGISTRY: [NativeClass; 304] = [
    lang::OBJECT,
    lang::CLASS,
    lang::PACKAGE,
    lang::STRING,
    lang::STRING_BUILDER,
    lang::STRING_BUFFER,
    lang::SYSTEM,
    lang::RUNTIME,
    lang::MODULE,
    lang::RUNTIME_VERSION,
    lang::THREAD,
    lang::THREAD_LOCAL,
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
    throwable::INVOCATION_TARGET_EXCEPTION,
    throwable::RUNTIME_EXCEPTION,
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
    throwable::NUMBER_FORMAT_EXCEPTION,
    throwable::CLASS_NOT_FOUND_EXCEPTION,
    throwable::CLONE_NOT_SUPPORTED_EXCEPTION,
    throwable::INTERRUPTED_EXCEPTION,
    math::MATH,
    math::STRICT_MATH,
    boxed::NUMBER,
    boxed::INTEGER,
    boxed::LONG,
    boxed::SHORT,
    boxed::BYTE,
    boxed::BOOLEAN,
    boxed::CHARACTER,
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
    io::BUFFERED_READER,
    io::BUFFERED_WRITER,
    io::OUTPUT_STREAM_WRITER,
    io::STRING_WRITER,
    util::NO_SUCH_ELEMENT_EXCEPTION,
    throwable::SERVICE_CONFIGURATION_ERROR,
    util::ITERATOR,
    util::COLLECTION,
    util::LIST,
    util::SET,
    util::MAP,
    util::OBJECTS,
    locale::LOCALE,
    optional::OPTIONAL,
    util::ARRAYS,
    util::RANDOM_ACCESS,
    util::RANDOM,
    util::ARRAY_LIST,
    util::ARRAY_LIST_ITERATOR,
    util::LINKED_LIST,
    util::PROPERTIES,
    uuid::UUID,
    util::HASH_MAP,
    util::IDENTITY_HASH_MAP,
    util::ENUMERATION,
    util::OJVM_ENUMERATION,
    util::HASH_SET,
    util::LINKED_HASH_SET,
    util::BIT_SET,
    util::HASHTABLE,
    util::VECTOR,
    util::MAP_ENTRY,
    util::OJVM_ENTRY,
    util::CONCURRENT_MAP,
    util::CONCURRENT_HASH_MAP,
    util::CONCURRENT_HASH_MAP_KEY_SET,
    regex::PATTERN_PREDICATE,
    net::INET_ADDRESS,
    net::INET4_ADDRESS,
    net::URL,
    net::STANDARD_PROTOCOL_FAMILY,
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
    file::FILE_FILTER,
    invoke::THREAD_FACTORY,
    lang::ANNOTATION,
    stack::STACK_WALKER,
    stack::STACK_WALKER_OPTION,
    stack::STACK_FRAME,
    stack::OJVM_STACK_FRAME,
    util::SORTED_MAP,
    intstream::INT_STREAM,
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
    logging::FILTER,
    logging::FORMATTER,
    logging::SIMPLE_FORMATTER,
    logging::ERROR_MANAGER,
    logging::HANDLER,
    logging::STREAM_HANDLER,
    logging::CONSOLE_HANDLER,
    logging::LOGGER,
    logging::LOG_MANAGER,
    util::COLLECTIONS,
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
    zip::ZIP_OUTPUT_STREAM,
    zip::CRC32,
    zip::CHECKSUM,
    nio::PATH,
    nio::OXJVM_PATH,
    nio::PATHS,
    nio::FILES,
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
    reflect::MODIFIER,
    reflect::METHOD,
    reflect::FIELD,
    reflect::CONSTRUCTOR,
    reflect::PARAMETER,
    uri::URI,
    unsafe_api::SUN_MISC_UNSAFE,
    unsafe_api::JDK_INTERNAL_UNSAFE,
    charset::CHARSET,
    charset::STANDARD_CHARSETS,
    nio::FILE_ATTRIBUTE,
    atomic::ATOMIC_INTEGER,
    atomic::ATOMIC_LONG,
    atomic::ATOMIC_BOOLEAN,
    concurrent::TIME_UNIT,
    concurrent::COUNT_DOWN_LATCH,
    concurrent::CONDITION,
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
