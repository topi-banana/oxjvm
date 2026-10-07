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

pub(crate) mod boxed;
pub(crate) mod invoke;
pub(crate) mod io;
pub(crate) mod lang;
pub(crate) mod math;
pub(crate) mod throwable;
pub(crate) mod util;

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
static REGISTRY: [NativeClass; 118] = [
    lang::OBJECT,
    lang::CLASS,
    lang::STRING,
    lang::STRING_BUILDER,
    lang::STRING_BUFFER,
    lang::SYSTEM,
    lang::RUNTIME,
    lang::THREAD,
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
    throwable::RUNTIME_EXCEPTION,
    throwable::ARITHMETIC_EXCEPTION,
    throwable::ARRAY_STORE_EXCEPTION,
    throwable::CLASS_CAST_EXCEPTION,
    throwable::ILLEGAL_ARGUMENT_EXCEPTION,
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
    util::NO_SUCH_ELEMENT_EXCEPTION,
    util::ITERATOR,
    util::COLLECTION,
    util::LIST,
    util::SET,
    util::MAP,
    util::OBJECTS,
    util::ARRAYS,
    util::RANDOM,
    util::ARRAY_LIST,
    util::ARRAY_LIST_ITERATOR,
    util::HASH_MAP,
    util::COLLECTIONS,
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
    invoke::APPENDABLE,
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
