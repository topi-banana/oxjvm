//! `java.lang.Throwable` and the exception hierarchy.

use alloc::vec;
use alloc::vec::Vec;

use alloc::string::String;
use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::lang::read_int_field;
use crate::{class, field, method, object, receiver, ref_arg, void};

// -------------------------------------------------------------------------------------------
// Throwable
// -------------------------------------------------------------------------------------------

const THROWABLE_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
    method("<init>", "()V", ACC_PUBLIC, exception_ctor),
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        exception_ctor,
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/Throwable;)V",
        ACC_PUBLIC,
        exception_ctor,
    ),
    method(
        "<init>",
        "(Ljava/lang/Throwable;)V",
        ACC_PUBLIC,
        exception_ctor,
    ),
    method(
        "getMessage",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        throwable_get_message,
    ),
    method(
        "getLocalizedMessage",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        throwable_get_message,
    ),
    method(
        "getCause",
        "()Ljava/lang/Throwable;",
        ACC_PUBLIC,
        throwable_get_cause,
    ),
    method(
        "initCause",
        "(Ljava/lang/Throwable;)Ljava/lang/Throwable;",
        ACC_PUBLIC,
        throwable_init_cause,
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        throwable_to_string,
    ),
    method(
        "printStackTrace",
        "()V",
        ACC_PUBLIC,
        throwable_print_stack_trace,
    ),
    method(
        "printStackTrace",
        "(Ljava/io/PrintStream;)V",
        ACC_PUBLIC,
        throwable_print_stack_trace_stream,
    ),
    method(
        "fillInStackTrace",
        "()Ljava/lang/Throwable;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            vm.capture_backtrace(this);
            object(this)
        },
    ),
    method(
        "getStackTrace",
        "()[Ljava/lang/StackTraceElement;",
        ACC_PUBLIC,
        throwable_get_stack_trace,
    ),
    method(
        "setStackTrace",
        "([Ljava/lang/StackTraceElement;)V",
        ACC_PUBLIC,
        throwable_set_stack_trace,
    ),
];

/// The generic exception constructor: message/cause bookkeeping in `Throwable`, delegation to the
/// superclass constructor otherwise.
pub(crate) fn exception_ctor(
    vm: &mut Vm<'_>,
    context: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let class = context.class;
    let descriptor = vm.classes.get(class).methods[context.method as usize]
        .descriptor
        .clone();
    let throwable = vm.resolve_class("java/lang/Throwable")?;
    if class == throwable {
        let detail = match descriptor.as_str() {
            "(Ljava/lang/String;)V" | "(Ljava/lang/String;Ljava/lang/Throwable;)V" => {
                ref_arg(args, 1)
            }
            _ => ObjectRef::NULL,
        };
        let cause = match descriptor.as_str() {
            "(Ljava/lang/String;Ljava/lang/Throwable;)V" | "(Ljava/lang/Throwable;)V" => {
                ref_arg(args, args.len() - 1)
            }
            _ => ObjectRef::NULL,
        };
        let (declaring, field) = vm.find_field(class, "detailMessage", "Ljava/lang/String;")?;
        vm.set_instance_ref(this, declaring, field, detail);
        let (declaring, field) = vm.find_field(class, "cause", "Ljava/lang/Throwable;")?;
        let cause = if cause == this {
            ObjectRef::NULL
        } else {
            cause
        };
        vm.set_instance_ref(this, declaring, field, cause);
        vm.capture_backtrace(this);
        return void();
    }
    let super_class = vm
        .classes
        .get(class)
        .super_class
        .ok_or_else(|| VmError::internal("exception without a superclass"))?;
    let (declaring, method) = vm
        .find_method(super_class, "<init>", &descriptor)
        .ok_or_else(|| vm.throw_new("java/lang/NoSuchMethodError", Some("<init>")))?;
    vm.invoke_method(declaring, method, args.to_vec())?;
    void()
}

fn throwable_get_message(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "detailMessage", "Ljava/lang/String;")?;
    let slot = vm.classes.get(declaring).fields[field as usize].slot as usize;
    let value = match vm.heap.get(this).map(|object| &object.data) {
        Some(oxjvm_vm::ObjectData::Instance(fields)) => fields[slot],
        _ => Value::Ref(ObjectRef::NULL),
    };
    object(value.as_ref())
}

fn throwable_get_cause(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let class = vm.class_of(this);
    let cause = vm
        .read_ref_field(this, class, "cause", "Ljava/lang/Throwable;")
        .unwrap_or(ObjectRef::NULL);
    if cause == this {
        object(ObjectRef::NULL)
    } else {
        object(cause)
    }
}

fn throwable_init_cause(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let cause = ref_arg(args, 1);
    if cause == this {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("Self-causation not permitted"),
        ));
    }
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "cause", "Ljava/lang/Throwable;")?;
    vm.set_instance_ref(this, declaring, field, cause);
    object(this)
}

fn throwable_to_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = vm.throwable_to_string(receiver(args));
    object(vm.make_string(&text)?)
}

fn throwable_print_stack_trace(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    vm.print_throwable(oxjvm_platform::Stream::Stderr, receiver(args));
    void()
}

fn throwable_print_stack_trace_stream(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let stream = ref_arg(args, 1);
    let target = if stream == vm.stderr_object() {
        oxjvm_platform::Stream::Stderr
    } else {
        oxjvm_platform::Stream::Stdout
    };
    vm.print_throwable(target, receiver(args));
    void()
}

fn throwable_get_stack_trace(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let frames = vm.backtrace(this);
    let element = vm.resolve_class("java/lang/StackTraceElement")?;
    let array = vm.allocate_object_array(element, frames.len())?;
    let constructor = vm.find_method(
        element,
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;I)V",
    );
    for (index, frame) in frames.into_iter().enumerate() {
        let element_object = vm.new_instance(element)?;
        let file = match &frame.file_name {
            Some(name) => vm.make_string(name)?,
            None => ObjectRef::NULL,
        };
        let class_name = vm.make_string(&frame.class_name2())?;
        let method_name = vm.make_string(&frame.method_name)?;
        if let Some((constructor_class, constructor_method)) = constructor {
            vm.invoke_method(
                constructor_class,
                constructor_method,
                vec![
                    Value::Ref(element_object),
                    Value::Ref(class_name),
                    Value::Ref(method_name),
                    Value::Ref(file),
                    Value::Int(i32::from(frame.line_number.unwrap_or(0))),
                ],
            )?;
        }
        vm.array_set_ref(array, index, element_object)?;
    }
    object(array)
}

fn throwable_set_stack_trace(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let array = ref_arg(args, 1);
    let mut frames = Vec::new();
    if let Some(oxjvm_vm::ObjectData::Array(oxjvm_vm::ArrayData::Reference(elements))) =
        vm.heap.get(array).map(|object| &object.data)
    {
        for element in elements {
            let class_name = vm
                .read_string_field_any(*element, "declaringClass")
                .unwrap_or_default();
            let method_name = vm
                .read_string_field_any(*element, "methodName")
                .unwrap_or_default();
            let file_name = vm.read_string_field_any(*element, "fileName");
            let line = read_int_field(vm, *element, "lineNumber");
            frames.push(oxjvm_vm::BacktraceFrame {
                class_name,
                method_name,
                file_name,
                line_number: Some(line as u16),
            });
        }
    }
    vm.set_backtrace(this, frames);
    void()
}

/// Helper so `BacktraceFrame` can be converted without exposing its fields.
trait FrameClass {
    fn class_name2(&self) -> String;
}

impl FrameClass for oxjvm_vm::BacktraceFrame {
    fn class_name2(&self) -> String {
        self.class_name.clone()
    }
}

pub(crate) const THROWABLE: oxjvm_vm::NativeClass = class(
    "java/lang/Throwable",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[
        field("detailMessage", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("cause", "Ljava/lang/Throwable;", ACC_PRIVATE, None),
    ],
    &THROWABLE_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// The exception hierarchy
// -------------------------------------------------------------------------------------------

const EXCEPTION_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method("<init>", "()V", ACC_PUBLIC, exception_ctor),
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        exception_ctor,
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/Throwable;)V",
        ACC_PUBLIC,
        exception_ctor,
    ),
    method(
        "<init>",
        "(Ljava/lang/Throwable;)V",
        ACC_PUBLIC,
        exception_ctor,
    ),
];

macro_rules! exception {
    ($constant:ident, $name:literal, $super:literal) => {
        pub(crate) const $constant: oxjvm_vm::NativeClass = class(
            $name,
            Some($super),
            &[],
            ACC_PUBLIC,
            &[],
            &EXCEPTION_METHODS,
            None,
        );
    };
}

exception!(ERROR, "java/lang/Error", "java/lang/Throwable");
exception!(
    VIRTUAL_MACHINE_ERROR,
    "java/lang/VirtualMachineError",
    "java/lang/Error"
);
exception!(
    OUT_OF_MEMORY_ERROR,
    "java/lang/OutOfMemoryError",
    "java/lang/VirtualMachineError"
);
exception!(
    STACK_OVERFLOW_ERROR,
    "java/lang/StackOverflowError",
    "java/lang/VirtualMachineError"
);
exception!(
    INTERNAL_ERROR,
    "java/lang/InternalError",
    "java/lang/VirtualMachineError"
);
exception!(
    ASSERTION_ERROR,
    "java/lang/AssertionError",
    "java/lang/Error"
);
exception!(LINKAGE_ERROR, "java/lang/LinkageError", "java/lang/Error");
exception!(
    BOOTSTRAP_METHOD_ERROR,
    "java/lang/BootstrapMethodError",
    "java/lang/LinkageError"
);
exception!(
    CLASS_CIRCULARITY_ERROR,
    "java/lang/ClassCircularityError",
    "java/lang/LinkageError"
);
exception!(
    EXCEPTION_IN_INITIALIZER_ERROR,
    "java/lang/ExceptionInInitializerError",
    "java/lang/LinkageError"
);
exception!(
    INCOMPATIBLE_CLASS_CHANGE_ERROR,
    "java/lang/IncompatibleClassChangeError",
    "java/lang/LinkageError"
);
exception!(
    ABSTRACT_METHOD_ERROR,
    "java/lang/AbstractMethodError",
    "java/lang/IncompatibleClassChangeError"
);
exception!(
    ILLEGAL_ACCESS_ERROR,
    "java/lang/IllegalAccessError",
    "java/lang/IncompatibleClassChangeError"
);
exception!(
    INSTANTIATION_ERROR,
    "java/lang/InstantiationError",
    "java/lang/IncompatibleClassChangeError"
);
exception!(
    NO_SUCH_FIELD_ERROR,
    "java/lang/NoSuchFieldError",
    "java/lang/IncompatibleClassChangeError"
);
exception!(
    NO_SUCH_METHOD_ERROR,
    "java/lang/NoSuchMethodError",
    "java/lang/IncompatibleClassChangeError"
);
exception!(
    NO_CLASS_DEF_FOUND_ERROR,
    "java/lang/NoClassDefFoundError",
    "java/lang/LinkageError"
);
exception!(
    UNSATISFIED_LINK_ERROR,
    "java/lang/UnsatisfiedLinkError",
    "java/lang/LinkageError"
);
exception!(
    VERIFY_ERROR,
    "java/lang/VerifyError",
    "java/lang/LinkageError"
);
exception!(EXCEPTION, "java/lang/Exception", "java/lang/Throwable");
exception!(
    REFLECTIVE_OPERATION_EXCEPTION,
    "java/lang/ReflectiveOperationException",
    "java/lang/Exception"
);
exception!(
    RUNTIME_EXCEPTION,
    "java/lang/RuntimeException",
    "java/lang/Exception"
);
exception!(
    ARITHMETIC_EXCEPTION,
    "java/lang/ArithmeticException",
    "java/lang/RuntimeException"
);
exception!(
    ARRAY_STORE_EXCEPTION,
    "java/lang/ArrayStoreException",
    "java/lang/RuntimeException"
);
exception!(
    CLASS_CAST_EXCEPTION,
    "java/lang/ClassCastException",
    "java/lang/RuntimeException"
);
exception!(
    ILLEGAL_ARGUMENT_EXCEPTION,
    "java/lang/IllegalArgumentException",
    "java/lang/RuntimeException"
);
exception!(
    ILLEGAL_MONITOR_STATE_EXCEPTION,
    "java/lang/IllegalMonitorStateException",
    "java/lang/RuntimeException"
);
exception!(
    ILLEGAL_STATE_EXCEPTION,
    "java/lang/IllegalStateException",
    "java/lang/RuntimeException"
);
exception!(
    INDEX_OUT_OF_BOUNDS_EXCEPTION,
    "java/lang/IndexOutOfBoundsException",
    "java/lang/RuntimeException"
);
exception!(
    ARRAY_INDEX_OUT_OF_BOUNDS_EXCEPTION,
    "java/lang/ArrayIndexOutOfBoundsException",
    "java/lang/IndexOutOfBoundsException"
);
exception!(
    STRING_INDEX_OUT_OF_BOUNDS_EXCEPTION,
    "java/lang/StringIndexOutOfBoundsException",
    "java/lang/IndexOutOfBoundsException"
);
exception!(
    NEGATIVE_ARRAY_SIZE_EXCEPTION,
    "java/lang/NegativeArraySizeException",
    "java/lang/RuntimeException"
);
exception!(
    NULL_POINTER_EXCEPTION,
    "java/lang/NullPointerException",
    "java/lang/RuntimeException"
);
exception!(
    UNSUPPORTED_OPERATION_EXCEPTION,
    "java/lang/UnsupportedOperationException",
    "java/lang/RuntimeException"
);
exception!(
    NUMBER_FORMAT_EXCEPTION,
    "java/lang/NumberFormatException",
    "java/lang/IllegalArgumentException"
);
exception!(
    CLASS_NOT_FOUND_EXCEPTION,
    "java/lang/ClassNotFoundException",
    "java/lang/ReflectiveOperationException"
);
exception!(
    CLONE_NOT_SUPPORTED_EXCEPTION,
    "java/lang/CloneNotSupportedException",
    "java/lang/Exception"
);
exception!(
    INTERRUPTED_EXCEPTION,
    "java/lang/InterruptedException",
    "java/lang/Exception"
);
