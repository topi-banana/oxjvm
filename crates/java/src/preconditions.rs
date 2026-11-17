//! `jdk.internal.util.Preconditions`: range checks with pluggable exception formatters.
//!
//! The check methods accept a `BiFunction<String, List<Number>, X>` formatter.  On a failed
//! check the formatter is applied with the check kind and an unmodifiable list of the
//! out-of-bounds arguments; a `null` formatter (or one returning `null`) falls back to an
//! `IndexOutOfBoundsException` carrying the standard message.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use oxjvm_classfile::flags::*;

use crate::{class, field, int, long, method, object, receiver, ref_arg, string_arg};
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

pub(crate) const PRECONDITIONS: oxjvm_vm::NativeClass = class(
    "jdk/internal/util/Preconditions",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field(
            "SIOOBE_FORMATTER",
            "Ljava/util/function/BiFunction;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "AIOOBE_FORMATTER",
            "Ljava/util/function/BiFunction;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "IOOBE_FORMATTER",
            "Ljava/util/function/BiFunction;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &PRECONDITIONS_METHODS,
    Some(preconditions_clinit),
);

const PRECONDITIONS_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "outOfBoundsExceptionFormatter",
        "(Ljava/util/function/Function;)Ljava/util/function/BiFunction;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(new_oob_formatter(vm, ref_arg(args, 0))?),
    ),
    method(
        "checkIndex",
        "(IILjava/util/function/BiFunction;)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let index = crate::int_arg(args, 0);
            let length = crate::int_arg(args, 1);
            if index >= 0 && index < length {
                return int(index);
            }
            let values = [
                crate::boxed::boxed_int(vm, index)?.as_ref(),
                crate::boxed::boxed_int(vm, length)?.as_ref(),
            ];
            Err(oob_error(vm, "checkIndex", &values, ref_arg(args, 2))?)
        },
    ),
    method(
        "checkIndex",
        "(JJLjava/util/function/BiFunction;)J",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let index = crate::long_arg(args, 0);
            let length = crate::long_arg(args, 1);
            if index >= 0 && index < length {
                return long(index);
            }
            let values = [
                crate::boxed::boxed_long(vm, index)?.as_ref(),
                crate::boxed::boxed_long(vm, length)?.as_ref(),
            ];
            Err(oob_error(vm, "checkIndex", &values, ref_arg(args, 2))?)
        },
    ),
    method(
        "checkFromToIndex",
        "(IIILjava/util/function/BiFunction;)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let from = crate::int_arg(args, 0);
            let to = crate::int_arg(args, 1);
            let length = crate::int_arg(args, 2);
            if from >= 0 && from <= to && to <= length {
                return int(from);
            }
            let values = [
                crate::boxed::boxed_int(vm, from)?.as_ref(),
                crate::boxed::boxed_int(vm, to)?.as_ref(),
                crate::boxed::boxed_int(vm, length)?.as_ref(),
            ];
            Err(oob_error(
                vm,
                "checkFromToIndex",
                &values,
                ref_arg(args, 3),
            )?)
        },
    ),
    method(
        "checkFromToIndex",
        "(JJJLjava/util/function/BiFunction;)J",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let from = crate::long_arg(args, 0);
            let to = crate::long_arg(args, 1);
            let length = crate::long_arg(args, 2);
            if from >= 0 && from <= to && to <= length {
                return long(from);
            }
            let values = [
                crate::boxed::boxed_long(vm, from)?.as_ref(),
                crate::boxed::boxed_long(vm, to)?.as_ref(),
                crate::boxed::boxed_long(vm, length)?.as_ref(),
            ];
            Err(oob_error(
                vm,
                "checkFromToIndex",
                &values,
                ref_arg(args, 3),
            )?)
        },
    ),
    method(
        "checkFromIndexSize",
        "(IIILjava/util/function/BiFunction;)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let from = crate::int_arg(args, 0);
            let size = crate::int_arg(args, 1);
            let length = crate::int_arg(args, 2);
            if from >= 0 && size >= 0 && i64::from(from) + i64::from(size) <= i64::from(length) {
                return int(from);
            }
            let values = [
                crate::boxed::boxed_int(vm, from)?.as_ref(),
                crate::boxed::boxed_int(vm, size)?.as_ref(),
                crate::boxed::boxed_int(vm, length)?.as_ref(),
            ];
            Err(oob_error(
                vm,
                "checkFromIndexSize",
                &values,
                ref_arg(args, 3),
            )?)
        },
    ),
    method(
        "checkFromIndexSize",
        "(JJJLjava/util/function/BiFunction;)J",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let from = crate::long_arg(args, 0);
            let size = crate::long_arg(args, 1);
            let length = crate::long_arg(args, 2);
            let in_bounds =
                from >= 0 && size >= 0 && from.checked_add(size).is_some_and(|end| end <= length);
            if in_bounds {
                return long(from);
            }
            let values = [
                crate::boxed::boxed_long(vm, from)?.as_ref(),
                crate::boxed::boxed_long(vm, size)?.as_ref(),
                crate::boxed::boxed_long(vm, length)?.as_ref(),
            ];
            Err(oob_error(
                vm,
                "checkFromIndexSize",
                &values,
                ref_arg(args, 3),
            )?)
        },
    ),
];

fn preconditions_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("jdk/internal/util/Preconditions")?;
    for (name, kind) in [
        ("SIOOBE_FORMATTER", 2),
        ("AIOOBE_FORMATTER", 1),
        ("IOOBE_FORMATTER", 0),
    ] {
        let factory_class = vm.resolve_class("jdk/internal/util/OjvmExceptionFactory")?;
        let factory = vm.new_instance(factory_class)?;
        if let Some((declaring, field)) = vm.find_field_opt(factory_class, "kind", "I") {
            vm.set_instance_int(factory, declaring, field, kind);
        }
        let formatter = new_oob_formatter(vm, factory)?;
        vm.set_static_value(
            class,
            name,
            "Ljava/util/function/BiFunction;",
            Value::Ref(formatter),
        );
    }
    Ok(())
}

/// A `BiFunction` that formats the out-of-bounds message and relays it to an exception factory.
pub(crate) const OOB_FORMATTER: oxjvm_vm::NativeClass = class(
    "jdk/internal/util/OjvmOutOfBoundsFormatter",
    Some("java/lang/Object"),
    &["java/util/function/BiFunction"],
    ACC_PUBLIC,
    &[field(
        "factory",
        "Ljava/util/function/Function;",
        ACC_PRIVATE,
        None,
    )],
    &OOB_FORMATTER_METHODS,
    None,
);

const OOB_FORMATTER_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "apply",
    "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
    ACC_PUBLIC,
    |vm, _, args| {
        let this = receiver(args);
        let kind = if args[1].as_ref().is_null() {
            None
        } else {
            Some(string_arg(vm, args[1])?)
        };
        let list = args[2].as_ref();
        let message = oob_message(vm, kind.as_deref(), list)?;
        let factory = vm
            .read_ref_field(
                this,
                vm.class_of(this),
                "factory",
                "Ljava/util/function/Function;",
            )
            .unwrap_or(ObjectRef::NULL);
        let message_string = vm.intern(&message)?;
        let (declaring, method) =
            vm.resolve_virtual_method(factory, "apply", "(Ljava/lang/Object;)Ljava/lang/Object;")?;
        vm.invoke_method(
            declaring,
            method,
            vec![Value::Ref(factory), Value::Ref(message_string)],
        )
    },
)];

/// A `Function<String, X>` producing one of the three out-of-bounds exception types.
pub(crate) const EXCEPTION_FACTORY: oxjvm_vm::NativeClass = class(
    "jdk/internal/util/OjvmExceptionFactory",
    Some("java/lang/Object"),
    &["java/util/function/Function"],
    ACC_PUBLIC,
    &[field("kind", "I", ACC_PRIVATE, None)],
    &EXCEPTION_FACTORY_METHODS,
    None,
);

const EXCEPTION_FACTORY_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "apply",
    "(Ljava/lang/Object;)Ljava/lang/Object;",
    ACC_PUBLIC,
    |vm, _, args| {
        let this = receiver(args);
        let kind = crate::lang::read_int_field(vm, this, "kind");
        let message = string_arg(vm, args[1])?;
        let name = match kind {
            1 => "java/lang/ArrayIndexOutOfBoundsException",
            2 => "java/lang/StringIndexOutOfBoundsException",
            _ => "java/lang/IndexOutOfBoundsException",
        };
        match vm.throw_new(name, Some(&message)) {
            VmError::Thrown(exception) => object(exception),
            other => Err(other),
        }
    },
)];

fn new_oob_formatter(vm: &mut Vm<'_>, factory: ObjectRef) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("jdk/internal/util/OjvmOutOfBoundsFormatter")?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "factory", "Ljava/util/function/Function;")?;
    vm.set_instance_ref(instance, declaring, field, factory);
    Ok(instance)
}

/// Build the exception for a failed check: apply the formatter, falling back to a plain
/// `IndexOutOfBoundsException` when it is absent or returns `null`.
fn oob_error(
    vm: &mut Vm<'_>,
    kind: &str,
    values: &[ObjectRef],
    formatter: ObjectRef,
) -> Result<VmError, VmError> {
    let list = immutable_list(vm, values)?;
    if !formatter.is_null() {
        let kind_string = vm.intern(kind)?;
        let (declaring, method) = vm.resolve_virtual_method(
            formatter,
            "apply",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        )?;
        let result = vm.invoke_method(
            declaring,
            method,
            vec![
                Value::Ref(formatter),
                Value::Ref(kind_string),
                Value::Ref(list),
            ],
        )?;
        let exception = result.as_ref();
        if !exception.is_null() {
            return Ok(VmError::Thrown(exception));
        }
    }
    let message = oob_message(vm, Some(kind), list)?;
    Ok(vm.throw_new("java/lang/IndexOutOfBoundsException", Some(&message)))
}

fn immutable_list(vm: &mut Vm<'_>, values: &[ObjectRef]) -> Result<ObjectRef, VmError> {
    let list = crate::util::array_list_from(vm, values)?;
    crate::lang::write_int_field(vm, list, "immutable", 1);
    Ok(list)
}

/// The standard out-of-bounds message, matching `Preconditions.outOfBoundsMessage`.
fn oob_message(vm: &mut Vm<'_>, kind: Option<&str>, list: ObjectRef) -> Result<String, VmError> {
    if list.is_null() {
        return Ok(match kind {
            None => String::from("Range check failed"),
            Some(kind) => alloc::format!("Range check failed: {kind}"),
        });
    }
    let (declaring, size_method) = vm.resolve_virtual_method(list, "size", "()I")?;
    let size = vm
        .invoke_method(declaring, size_method, vec![Value::Ref(list)])?
        .as_int()
        .max(0) as usize;
    let mut texts: Vec<String> = Vec::new();
    for index in 0..size {
        let (declaring, get) = vm.resolve_virtual_method(list, "get", "(I)Ljava/lang/Object;")?;
        let element = vm
            .invoke_method(
                declaring,
                get,
                vec![Value::Ref(list), Value::Int(index as i32)],
            )?
            .as_ref();
        let (declaring, to_string) =
            vm.resolve_virtual_method(element, "toString", "()Ljava/lang/String;")?;
        let text = vm.invoke_method(declaring, to_string, vec![Value::Ref(element)])?;
        texts.push(string_arg(vm, text)?);
    }
    match kind {
        Some("checkIndex") if texts.len() == 2 => Ok(alloc::format!(
            "Index {} out of bounds for length {}",
            texts[0],
            texts[1]
        )),
        Some("checkFromToIndex") if texts.len() == 3 => Ok(alloc::format!(
            "Range [{}, {}) out of bounds for length {}",
            texts[0],
            texts[1],
            texts[2]
        )),
        Some("checkFromIndexSize") if texts.len() == 3 => Ok(alloc::format!(
            "Range [{}, {} + {}) out of bounds for length {}",
            texts[0],
            texts[0],
            texts[1],
            texts[2]
        )),
        _ => {
            let joined = texts.join(", ");
            Ok(match kind {
                Some(kind) => alloc::format!("Range check failed: {kind} [{joined}]"),
                None => alloc::format!("Range check failed: [{joined}]"),
            })
        }
    }
}
