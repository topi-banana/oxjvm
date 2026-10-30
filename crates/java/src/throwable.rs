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

const THROWABLE_METHODS: [oxjvm_vm::NativeMethodDef; 18] = [
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
        "(Ljava/lang/String;Ljava/lang/Throwable;ZZ)V",
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
        "printStackTrace",
        "(Ljava/io/PrintWriter;)V",
        ACC_PUBLIC,
        throwable_print_stack_trace_writer,
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
    method(
        "addSuppressed",
        "(Ljava/lang/Throwable;)V",
        ACC_PUBLIC,
        throwable_add_suppressed,
    ),
    method(
        "getSuppressed",
        "()[Ljava/lang/Throwable;",
        ACC_PUBLIC,
        throwable_get_suppressed,
    ),
];

fn throwable_add_suppressed(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let suppressed = ref_arg(args, 1);
    if suppressed.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    if suppressed == this {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("Self-suppression not permitted"),
        ));
    }
    let class = vm.class_of(this);
    let existing = vm
        .read_ref_field(this, class, "suppressed", "[Ljava/lang/Throwable;")
        .unwrap_or(ObjectRef::NULL);
    let throwable = vm.resolve_class("java/lang/Throwable")?;
    let length = if existing.is_null() {
        0
    } else {
        vm.array_length(existing)?
    };
    let array = vm.allocate_object_array(throwable, length + 1)?;
    for index in 0..length {
        let element = vm.array_get(existing, index as i32)?.as_ref();
        vm.array_set_ref(array, index, element)?;
    }
    vm.array_set_ref(array, length, suppressed)?;
    let (declaring, field) = vm.find_field(class, "suppressed", "[Ljava/lang/Throwable;")?;
    vm.set_instance_ref(this, declaring, field, array);
    void()
}

fn throwable_get_suppressed(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let class = vm.class_of(this);
    let array = vm
        .read_ref_field(this, class, "suppressed", "[Ljava/lang/Throwable;")
        .unwrap_or(ObjectRef::NULL);
    if array.is_null() {
        let throwable = vm.resolve_class("java/lang/Throwable")?;
        let empty = vm.allocate_object_array(throwable, 0)?;
        return object(empty);
    }
    object(array)
}

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
            "(Ljava/lang/String;)V"
            | "(Ljava/lang/String;Ljava/lang/Throwable;)V"
            | "(Ljava/lang/String;Ljava/lang/Throwable;ZZ)V" => ref_arg(args, 1),
            _ => ObjectRef::NULL,
        };
        let cause = match descriptor.as_str() {
            "(Ljava/lang/String;Ljava/lang/Throwable;)V"
            | "(Ljava/lang/String;Ljava/lang/Throwable;ZZ)V" => ref_arg(args, 2),
            "(Ljava/lang/Throwable;)V" => ref_arg(args, 1),
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

/// `printStackTrace(PrintWriter)`: the writer receives the same text as `System.err` would.
fn throwable_print_stack_trace_writer(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let writer = ref_arg(args, 1);
    if !writer.is_null() {
        let text = vm.throwable_text(this);
        let string = vm.make_string(&text)?;
        let (class, method) =
            vm.resolve_virtual_method(writer, "print", "(Ljava/lang/String;)V")?;
        vm.invoke_method(class, method, vec![Value::Ref(writer), Value::Ref(string)])?;
    }
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
        field("suppressed", "[Ljava/lang/Throwable;", ACC_PRIVATE, None),
    ],
    &THROWABLE_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// The exception hierarchy
// -------------------------------------------------------------------------------------------

const EXCEPTION_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
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
        "(Ljava/lang/String;Ljava/lang/Throwable;ZZ)V",
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
const ASSERTION_ERROR_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
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
        "(Ljava/lang/String;Ljava/lang/Throwable;ZZ)V",
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
        "<init>",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC,
        assertion_error_object_ctor,
    ),
];

/// `AssertionError(Object)`: a `Throwable` argument becomes the cause, and other
/// values are rendered by `String.valueOf`.
fn assertion_error_object_ctor(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let detail = ref_arg(args, 1);
    let throwable = vm.resolve_class("java/lang/Throwable")?;
    let cause = if !detail.is_null() && vm.is_instance(detail, throwable) {
        detail
    } else {
        ObjectRef::NULL
    };
    let message = if detail.is_null() {
        ObjectRef::NULL
    } else if vm.string_value(detail).is_some() {
        detail
    } else {
        let string_class = vm.resolve_class("java/lang/String")?;
        let (_, value_of) = vm
            .find_method(
                string_class,
                "valueOf",
                "(Ljava/lang/Object;)Ljava/lang/String;",
            )
            .ok_or_else(|| VmError::internal("String.valueOf(Object) missing"))?;
        vm.invoke_method(string_class, value_of, vec![Value::Ref(detail)])?
            .as_ref()
    };
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "detailMessage", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, message);
    let (declaring, field) = vm.find_field(class, "cause", "Ljava/lang/Throwable;")?;
    vm.set_instance_ref(this, declaring, field, cause);
    vm.capture_backtrace(this);
    void()
}

pub(crate) const ASSERTION_ERROR: oxjvm_vm::NativeClass = class(
    "java/lang/AssertionError",
    Some("java/lang/Error"),
    &[],
    ACC_PUBLIC,
    &[],
    &ASSERTION_ERROR_METHODS,
    None,
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
    INSTANTIATION_EXCEPTION,
    "java/lang/InstantiationException",
    "java/lang/ReflectiveOperationException"
);

const INVOCATION_TARGET_EXCEPTION_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("<init>", "()V", ACC_PUBLIC, invocation_target_ctor),
    method(
        "<init>",
        "(Ljava/lang/Throwable;)V",
        ACC_PUBLIC,
        invocation_target_ctor,
    ),
    method(
        "<init>",
        "(Ljava/lang/Throwable;Ljava/lang/String;)V",
        ACC_PUBLIC,
        invocation_target_ctor,
    ),
    method(
        "getTargetException",
        "()Ljava/lang/Throwable;",
        ACC_PUBLIC,
        invocation_target_target,
    ),
    method(
        "getCause",
        "()Ljava/lang/Throwable;",
        ACC_PUBLIC,
        invocation_target_target,
    ),
];

fn invocation_target_target(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let class = vm.class_of(this);
    let target = vm
        .read_ref_field(this, class, "target", "Ljava/lang/Throwable;")
        .unwrap_or(ObjectRef::NULL);
    object(target)
}

fn invocation_target_ctor(
    vm: &mut Vm<'_>,
    context: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let class = context.class;
    let descriptor = vm.classes.get(class).methods[context.method as usize]
        .descriptor
        .clone();
    let (target, message) = match descriptor.as_str() {
        "()V" => (ObjectRef::NULL, ObjectRef::NULL),
        "(Ljava/lang/Throwable;)V" => {
            let cause = ref_arg(args, 1);
            let text = if cause.is_null() {
                ObjectRef::NULL
            } else {
                let (method_class, method) =
                    vm.resolve_virtual_method(cause, "toString", "()Ljava/lang/String;")?;
                vm.invoke_method(method_class, method, vec![Value::Ref(cause)])?
                    .as_ref()
            };
            (cause, text)
        }
        _ => (ref_arg(args, 1), ref_arg(args, 2)),
    };
    let (declaring, field) = vm.find_field(class, "target", "Ljava/lang/Throwable;")?;
    vm.set_instance_ref(this, declaring, field, target);
    let super_class = vm
        .classes
        .get(class)
        .super_class
        .ok_or_else(|| VmError::internal("exception without a superclass"))?;
    let (declaring, method) = vm
        .find_method(super_class, "<init>", "(Ljava/lang/String;)V")
        .ok_or_else(|| vm.throw_new("java/lang/NoSuchMethodError", Some("<init>")))?;
    vm.invoke_method(
        declaring,
        method,
        vec![Value::Ref(this), Value::Ref(message)],
    )?;
    void()
}

/// `java.lang.reflect.InvocationTargetException`.
pub(crate) const INVOCATION_TARGET_EXCEPTION: oxjvm_vm::NativeClass = class(
    "java/lang/reflect/InvocationTargetException",
    Some("java/lang/ReflectiveOperationException"),
    &[],
    ACC_PUBLIC,
    &[field("target", "Ljava/lang/Throwable;", ACC_PRIVATE, None)],
    &INVOCATION_TARGET_EXCEPTION_METHODS,
    None,
);
exception!(
    RUNTIME_EXCEPTION,
    "java/lang/RuntimeException",
    "java/lang/Exception"
);
exception!(
    CANCELLATION_EXCEPTION,
    "java/util/concurrent/CancellationException",
    "java/lang/RuntimeException"
);
exception!(
    MISSING_RESOURCE_EXCEPTION,
    "java/util/MissingResourceException",
    "java/lang/RuntimeException"
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
    SECURITY_EXCEPTION,
    "java/lang/SecurityException",
    "java/lang/RuntimeException"
);
exception!(
    MATCH_EXCEPTION,
    "java/lang/MatchException",
    "java/lang/RuntimeException"
);
exception!(
    FILE_NOT_FOUND_EXCEPTION,
    "java/io/FileNotFoundException",
    "java/io/IOException"
);

exception!(
    GENERAL_SECURITY_EXCEPTION,
    "java/security/GeneralSecurityException",
    "java/lang/Exception"
);
exception!(
    NO_SUCH_ALGORITHM_EXCEPTION,
    "java/security/NoSuchAlgorithmException",
    "java/security/GeneralSecurityException"
);
exception!(
    NO_SUCH_PROVIDER_EXCEPTION,
    "java/security/NoSuchProviderException",
    "java/security/GeneralSecurityException"
);
exception!(
    KEY_EXCEPTION,
    "java/security/KeyException",
    "java/security/GeneralSecurityException"
);
exception!(
    INVALID_KEY_EXCEPTION,
    "java/security/InvalidKeyException",
    "java/security/KeyException"
);
exception!(
    UNRECOVERABLE_KEY_EXCEPTION,
    "java/security/UnrecoverableKeyException",
    "java/security/KeyException"
);
exception!(
    MALFORMED_URL_EXCEPTION,
    "java/net/MalformedURLException",
    "java/io/IOException"
);
exception!(
    KEY_STORE_EXCEPTION,
    "java/security/KeyStoreException",
    "java/security/GeneralSecurityException"
);
exception!(
    KEY_MANAGEMENT_EXCEPTION,
    "java/security/KeyManagementException",
    "java/security/KeyException"
);
exception!(
    INVALID_ALGORITHM_PARAMETER_EXCEPTION,
    "java/security/InvalidAlgorithmParameterException",
    "java/security/GeneralSecurityException"
);
exception!(
    SIGNATURE_EXCEPTION,
    "java/security/SignatureException",
    "java/security/GeneralSecurityException"
);
exception!(
    CERTIFICATE_EXCEPTION,
    "java/security/cert/CertificateException",
    "java/security/GeneralSecurityException"
);
exception!(
    CERTIFICATE_EXPIRED_EXCEPTION,
    "java/security/cert/CertificateExpiredException",
    "java/security/cert/CertificateException"
);
exception!(
    CERTIFICATE_NOT_YET_VALID_EXCEPTION,
    "java/security/cert/CertificateNotYetValidException",
    "java/security/cert/CertificateException"
);
exception!(
    ZIP_EXCEPTION,
    "java/util/zip/ZipException",
    "java/io/IOException"
);
exception!(
    NOSUCH_PADDING_EXCEPTION,
    "javax/crypto/NoSuchPaddingException",
    "java/security/GeneralSecurityException"
);
exception!(
    ILLEGAL_BLOCK_SIZE_EXCEPTION,
    "javax/crypto/IllegalBlockSizeException",
    "java/security/GeneralSecurityException"
);
exception!(
    BAD_PADDING_EXCEPTION,
    "javax/crypto/BadPaddingException",
    "java/security/GeneralSecurityException"
);
exception!(
    SHORT_BUFFER_EXCEPTION,
    "javax/crypto/ShortBufferException",
    "java/security/GeneralSecurityException"
);
exception!(
    AEAD_BAD_TAG_EXCEPTION,
    "javax/crypto/AEADBadTagException",
    "javax/crypto/BadPaddingException"
);
exception!(
    CERTIFICATE_PARSING_EXCEPTION,
    "java/security/cert/CertificateParsingException",
    "java/security/cert/CertificateException"
);
exception!(
    CRL_EXCEPTION,
    "java/security/cert/CRLException",
    "java/security/cert/CertificateException"
);
exception!(
    INVALID_KEY_SPEC_EXCEPTION,
    "java/security/spec/InvalidKeySpecException",
    "java/security/GeneralSecurityException"
);
exception!(
    INVALID_PARAMETER_SPEC_EXCEPTION,
    "java/security/spec/InvalidParameterSpecException",
    "java/security/GeneralSecurityException"
);
exception!(
    UNKNOWN_HOST_EXCEPTION,
    "java/net/UnknownHostException",
    "java/io/IOException"
);
exception!(
    DATA_FORMAT_EXCEPTION,
    "java/util/zip/DataFormatException",
    "java/lang/Exception"
);

fn type_not_present_ctor(
    vm: &mut Vm<'_>,
    context: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let type_name = ref_arg(args, 1);
    let (declaring, field) = vm.find_field(context.class, "typeName", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, type_name);
    exception_ctor(vm, context, args)?;
    void()
}

fn enum_constant_not_present_ctor(
    vm: &mut Vm<'_>,
    context: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let enum_type = ref_arg(args, 1);
    let constant_name = ref_arg(args, 2);
    if enum_type.is_null() || constant_name.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let (declaring, field) = vm.find_field(context.class, "enumType", "Ljava/lang/Class;")?;
    vm.set_instance_ref(this, declaring, field, enum_type);
    let (declaring, field) = vm.find_field(context.class, "constantName", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, constant_name);
    let enum_class = crate::lang::class_argument(vm, enum_type)?;
    let type_name = crate::binary_name(vm, enum_class);
    let constant = vm.string_or_empty(constant_name);
    let message = alloc::format!("{type_name}.{constant}");
    let message = vm.make_string(&message)?;
    let (declaring, field) = vm.find_field(context.class, "detailMessage", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, message);
    vm.capture_backtrace(this);
    void()
}

/// `java.lang.EnumConstantNotPresentException`.
pub(crate) const ENUM_CONSTANT_NOT_PRESENT_EXCEPTION: oxjvm_vm::NativeClass = class(
    "java/lang/EnumConstantNotPresentException",
    Some("java/lang/RuntimeException"),
    &[],
    ACC_PUBLIC,
    &[
        field(
            "enumType",
            "Ljava/lang/Class;",
            ACC_PRIVATE | ACC_FINAL,
            None,
        ),
        field(
            "constantName",
            "Ljava/lang/String;",
            ACC_PRIVATE | ACC_FINAL,
            None,
        ),
    ],
    &[
        method(
            "<init>",
            "(Ljava/lang/Class;Ljava/lang/String;)V",
            ACC_PUBLIC,
            enum_constant_not_present_ctor,
        ),
        method(
            "enumType",
            "()Ljava/lang/Class;",
            ACC_PUBLIC,
            |vm, _, args| {
                let this = receiver(args);
                let class = vm.class_of(this);
                let value = vm
                    .read_ref_field(this, class, "enumType", "Ljava/lang/Class;")
                    .unwrap_or(ObjectRef::NULL);
                object(value)
            },
        ),
        method(
            "constantName",
            "()Ljava/lang/String;",
            ACC_PUBLIC,
            |vm, _, args| {
                let this = receiver(args);
                let class = vm.class_of(this);
                let value = vm
                    .read_ref_field(this, class, "constantName", "Ljava/lang/String;")
                    .unwrap_or(ObjectRef::NULL);
                object(value)
            },
        ),
    ],
    None,
);

/// `java.lang.TypeNotPresentException`.
pub(crate) const TYPE_NOT_PRESENT_EXCEPTION: oxjvm_vm::NativeClass = class(
    "java/lang/TypeNotPresentException",
    Some("java/lang/RuntimeException"),
    &[],
    ACC_PUBLIC,
    &[field("typeName", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &[
        method(
            "<init>",
            "(Ljava/lang/String;)V",
            ACC_PUBLIC,
            type_not_present_ctor,
        ),
        method(
            "<init>",
            "(Ljava/lang/String;Ljava/lang/Throwable;)V",
            ACC_PUBLIC,
            type_not_present_ctor,
        ),
        method(
            "typeName",
            "()Ljava/lang/String;",
            ACC_PUBLIC,
            |vm, _, args| {
                let this = receiver(args);
                let class = vm.class_of(this);
                let value = vm
                    .read_ref_field(this, class, "typeName", "Ljava/lang/String;")
                    .unwrap_or(ObjectRef::NULL);
                object(value)
            },
        ),
    ],
    None,
);

/// `java.util.zip.ZipConstants`: a marker interface.
pub(crate) const ZIP_CONSTANTS: oxjvm_vm::NativeClass = class(
    "java/util/zip/ZipConstants",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);
exception!(
    FILE_SYSTEM_EXCEPTION,
    "java/nio/file/FileSystemException",
    "java/io/IOException"
);
exception!(
    NO_SUCH_FILE_EXCEPTION,
    "java/nio/file/NoSuchFileException",
    "java/nio/file/FileSystemException"
);
exception!(
    ACCESS_DENIED_EXCEPTION,
    "java/nio/file/AccessDeniedException",
    "java/nio/file/FileSystemException"
);
exception!(
    NOT_DIRECTORY_EXCEPTION,
    "java/nio/file/NotDirectoryException",
    "java/nio/file/FileSystemException"
);
exception!(
    DIRECTORY_NOT_EMPTY_EXCEPTION,
    "java/nio/file/DirectoryNotEmptyException",
    "java/nio/file/FileSystemException"
);
exception!(
    ILLEGAL_ACCESS_EXCEPTION,
    "java/lang/IllegalAccessException",
    "java/lang/ReflectiveOperationException"
);
exception!(
    NO_SUCH_FIELD_EXCEPTION,
    "java/lang/NoSuchFieldException",
    "java/lang/ReflectiveOperationException"
);
exception!(
    NO_SUCH_METHOD_EXCEPTION,
    "java/lang/NoSuchMethodException",
    "java/lang/ReflectiveOperationException"
);

exception!(
    UNSUPPORTED_CHARSET_EXCEPTION,
    "java/nio/charset/UnsupportedCharsetException",
    "java/lang/IllegalArgumentException"
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
    REJECTED_EXECUTION_EXCEPTION,
    "java/util/concurrent/RejectedExecutionException",
    "java/lang/RuntimeException"
);
exception!(
    FILE_SYSTEM_NOT_FOUND_EXCEPTION,
    "java/nio/file/FileSystemNotFoundException",
    "java/lang/RuntimeException"
);
exception!(
    PROVIDER_NOT_FOUND_EXCEPTION,
    "java/nio/file/ProviderNotFoundException",
    "java/lang/RuntimeException"
);
exception!(
    FILE_SYSTEM_ALREADY_EXISTS_EXCEPTION,
    "java/nio/file/FileSystemAlreadyExistsException",
    "java/lang/RuntimeException"
);
exception!(
    MALFORMED_INPUT_EXCEPTION,
    "java/nio/charset/MalformedInputException",
    "java/io/IOException"
);
exception!(
    UNMAPPABLE_CHARACTER_EXCEPTION,
    "java/nio/charset/UnmappableCharacterException",
    "java/io/IOException"
);
exception!(
    CODER_MALFUNCTION_ERROR,
    "java/nio/charset/CoderMalfunctionError",
    "java/lang/Error"
);
exception!(
    TIMEOUT_EXCEPTION,
    "java/util/concurrent/TimeoutException",
    "java/lang/Exception"
);
exception!(
    EXECUTION_EXCEPTION,
    "java/util/concurrent/ExecutionException",
    "java/lang/Exception"
);
exception!(
    BUFFER_OVERFLOW_EXCEPTION,
    "java/nio/BufferOverflowException",
    "java/lang/RuntimeException"
);
exception!(
    BUFFER_UNDERFLOW_EXCEPTION,
    "java/nio/BufferUnderflowException",
    "java/lang/RuntimeException"
);
exception!(
    INVALID_MARK_EXCEPTION,
    "java/nio/InvalidMarkException",
    "java/lang/RuntimeException"
);
exception!(
    READ_ONLY_BUFFER_EXCEPTION,
    "java/nio/ReadOnlyBufferException",
    "java/lang/UnsupportedOperationException"
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
exception!(
    SERVICE_CONFIGURATION_ERROR,
    "java/util/ServiceConfigurationError",
    "java/lang/Error"
);

/// `java.net.URISyntaxException`: a string that is not a valid URI reference.
pub(crate) const URI_SYNTAX_EXCEPTION: oxjvm_vm::NativeClass = class(
    "java/net/URISyntaxException",
    Some("java/lang/Exception"),
    &[],
    ACC_PUBLIC,
    &[
        field("input", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("reason", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("index", "I", ACC_PRIVATE, None),
    ],
    &[
        method(
            "<init>",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            ACC_PUBLIC,
            uri_syntax_init,
        ),
        method(
            "<init>",
            "(Ljava/lang/String;Ljava/lang/String;I)V",
            ACC_PUBLIC,
            uri_syntax_init,
        ),
        method(
            "getInput",
            "()Ljava/lang/String;",
            ACC_PUBLIC,
            |vm, _, args| {
                let this = receiver(args);
                let input = vm
                    .read_string_field(this, vm.class_of(this), "input")
                    .unwrap_or_default();
                object(vm.make_string(&input)?)
            },
        ),
        method("getIndex", "()I", ACC_PUBLIC, |vm, _, args| {
            object_ignore(Ok(Value::Int(read_int_field(vm, receiver(args), "index"))))
        }),
        method(
            "getReason",
            "()Ljava/lang/String;",
            ACC_PUBLIC,
            |vm, _, args| {
                let this = receiver(args);
                let reason = vm
                    .read_string_field(this, vm.class_of(this), "reason")
                    .unwrap_or_default();
                object(vm.make_string(&reason)?)
            },
        ),
    ],
    None,
);

fn object_ignore(value: Result<Value, VmError>) -> Result<Value, VmError> {
    value
}

fn uri_syntax_init(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let reason = crate::string_arg(vm, args[1])?;
    let input = crate::string_arg(vm, args[2])?;
    let index = if args.len() > 3 { args[3].as_int() } else { -1 };
    let class = vm.class_of(this);
    vm.set_string_field(this, class, "reason", &reason)?;
    vm.set_string_field(this, class, "input", &input)?;
    if let Some((declaring, field)) = vm.find_field_opt(class, "index", "I") {
        vm.set_instance_int(this, declaring, field, index);
    }
    let message = alloc::format!("{reason}: {input}");
    let exception = vm.resolve_class("java/lang/Exception")?;
    let (declaring, ctor) = vm
        .find_method(exception, "<init>", "(Ljava/lang/String;)V")
        .ok_or_else(|| VmError::internal("Exception(String) missing"))?;
    let text = vm.make_string(&message)?;
    vm.invoke_method(declaring, ctor, vec![Value::Ref(this), Value::Ref(text)])?;
    void()
}
