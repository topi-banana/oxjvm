//! `java.lang.StackWalker` (+ `$Option`, `$StackFrame`): snapshots of the
//! current thread's Java frames via [`Vm::capture_frames`].

use alloc::vec;
use alloc::vec::Vec;
use oxjvm_classfile::flags::*;

use crate::lang::abstract_method_pub;
use crate::{boolean, class, field, int, method, object, receiver, ref_arg, void};
use oxjvm_vm::{ClassId, ObjectRef, Value, Vm, VmError};

// -------------------------------------------------------------------------------------------
// StackWalker$Option
// -------------------------------------------------------------------------------------------

fn option_index(vm: &Vm<'_>, option: ObjectRef) -> i32 {
    crate::lang::read_int_field(vm, option, "ordinal")
}

fn stack_walker_option_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/lang/StackWalker$Option")?;
    for (index, name) in [
        "RETAIN_CLASS_REFERENCE",
        "SHOW_REFLECT_FRAMES",
        "SHOW_HIDDEN_FRAMES",
    ]
    .iter()
    .enumerate()
    {
        let instance = vm.new_instance(class)?;
        if let Some((declaring, field)) = vm.find_field_opt(class, "ordinal", "I") {
            vm.set_instance_int(instance, declaring, field, index as i32);
        }
        vm.set_static_value(
            class,
            name,
            "Ljava/lang/StackWalker$Option;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

const STACK_WALKER_OPTION_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("ordinal", "()I", ACC_PUBLIC, |vm, _, args| {
        int(option_index(vm, receiver(args)))
    }),
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let names = [
            "RETAIN_CLASS_REFERENCE",
            "SHOW_REFLECT_FRAMES",
            "SHOW_HIDDEN_FRAMES",
        ];
        let index = option_index(vm, receiver(args)).clamp(0, 2) as usize;
        object(vm.make_string(names[index])?)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let names = [
                "RETAIN_CLASS_REFERENCE",
                "SHOW_REFLECT_FRAMES",
                "SHOW_HIDDEN_FRAMES",
            ];
            let index = option_index(vm, receiver(args)).clamp(0, 2) as usize;
            object(vm.make_string(names[index])?)
        },
    ),
    method(
        "values",
        "()[Ljava/lang/StackWalker$Option;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/lang/StackWalker$Option")?;
            let array = vm.allocate_object_array(class, 3)?;
            for (index, name) in [
                "RETAIN_CLASS_REFERENCE",
                "SHOW_REFLECT_FRAMES",
                "SHOW_HIDDEN_FRAMES",
            ]
            .iter()
            .enumerate()
            {
                if let Some(option) =
                    vm.read_static_ref_if_set(class, name, "Ljava/lang/StackWalker$Option;")
                {
                    vm.array_set_ref(array, index, option)?;
                }
            }
            object(array)
        },
    ),
    method(
        "valueOf",
        "(Ljava/lang/String;)Ljava/lang/StackWalker$Option;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = vm.string_or_empty(ref_arg(args, 0));
            let class = vm.resolve_class("java/lang/StackWalker$Option")?;
            for candidate in [
                "RETAIN_CLASS_REFERENCE",
                "SHOW_REFLECT_FRAMES",
                "SHOW_HIDDEN_FRAMES",
            ] {
                if candidate == name {
                    if let Some(option) = vm.read_static_ref_if_set(
                        class,
                        candidate,
                        "Ljava/lang/StackWalker$Option;",
                    ) {
                        return object(option);
                    }
                }
            }
            Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some(&alloc::format!(
                    "No enum constant java.lang.StackWalker.Option.{name}"
                )),
            ))
        },
    ),
];

pub(crate) const STACK_WALKER_OPTION: oxjvm_vm::NativeClass = class(
    "java/lang/StackWalker$Option",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("ordinal", "I", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "RETAIN_CLASS_REFERENCE",
            "Ljava/lang/StackWalker$Option;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "SHOW_REFLECT_FRAMES",
            "Ljava/lang/StackWalker$Option;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "SHOW_HIDDEN_FRAMES",
            "Ljava/lang/StackWalker$Option;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &STACK_WALKER_OPTION_METHODS,
    Some(stack_walker_option_clinit),
);

// -------------------------------------------------------------------------------------------
// StackWalker$StackFrame
// -------------------------------------------------------------------------------------------

fn frame_field(vm: &Vm<'_>, this: ObjectRef, name: &str) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), name, "Ljava/lang/String;")
        .unwrap_or(ObjectRef::NULL)
}

const STACK_FRAME_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method(
        "getClassName",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "getMethodName",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "getDeclaringClass",
        "()Ljava/lang/Class;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "getFileName",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "getLineNumber",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "getByteCodeIndex",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "isNativeMethod",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "toStackTraceElement",
        "()Ljava/lang/StackTraceElement;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
];

pub(crate) const STACK_FRAME: oxjvm_vm::NativeClass = class(
    "java/lang/StackWalker$StackFrame",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &STACK_FRAME_METHODS,
    None,
);

const OJVM_STACK_FRAME_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
    method(
        "getClassName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = vm.string_or_empty(frame_field(vm, receiver(args), "className"));
            object(vm.make_string(&name)?)
        },
    ),
    method(
        "getMethodName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = vm.string_or_empty(frame_field(vm, receiver(args), "methodName"));
            object(vm.make_string(&name)?)
        },
    ),
    method(
        "getDeclaringClass",
        "()Ljava/lang/Class;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = vm.string_or_empty(frame_field(vm, receiver(args), "className"));
            let class = vm.resolve_class(&name)?;
            object(vm.class_object(class)?)
        },
    ),
    method(
        "getFileName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            if let Some(name) =
                vm.read_string_field(receiver(args), vm.class_of(receiver(args)), "fileName")
            {
                return object(vm.make_string(&name)?);
            }
            object(ObjectRef::NULL)
        },
    ),
    method("getLineNumber", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(
            vm,
            receiver(args),
            "lineNumber",
        ))
    }),
    method("getByteCodeIndex", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method("isNativeMethod", "()Z", ACC_PUBLIC, |_, _, _| {
        boolean(false)
    }),
    method(
        "toStackTraceElement",
        "()Ljava/lang/StackTraceElement;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class_name = vm
                .string_or_empty(frame_field(vm, this, "className"))
                .replace('/', ".");
            let method_name = vm.string_or_empty(frame_field(vm, this, "methodName"));
            let file_name = vm
                .read_string_field(this, vm.class_of(this), "fileName")
                .unwrap_or_default();
            let line = crate::lang::read_int_field(vm, this, "lineNumber");
            let element_class = vm.resolve_class("java/lang/StackTraceElement")?;
            let element = vm.new_instance(element_class)?;
            let (declaring, ctor) = vm
                .find_method(
                    element_class,
                    "<init>",
                    "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;I)V",
                )
                .ok_or_else(|| VmError::internal("StackTraceElement ctor missing"))?;
            let class_text = vm.make_string(&class_name)?;
            let method_text = vm.make_string(&method_name)?;
            let file_text = vm.make_string(&file_name)?;
            vm.invoke_method(
                declaring,
                ctor,
                vec![
                    Value::Ref(element),
                    Value::Ref(class_text),
                    Value::Ref(method_text),
                    Value::Ref(file_text),
                    Value::Int(line),
                ],
            )?;
            object(element)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm
                .string_or_empty(frame_field(vm, this, "className"))
                .replace('/', ".");
            let name = vm.string_or_empty(frame_field(vm, this, "methodName"));
            let file = vm
                .read_string_field(this, vm.class_of(this), "fileName")
                .unwrap_or_default();
            let line = crate::lang::read_int_field(vm, this, "lineNumber");
            object(vm.make_string(&alloc::format!("{class}.{name}({file}:{line})"))?)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() || vm.class_of(receiver(args)) != vm.class_of(other) {
                return boolean(false);
            }
            boolean(frame_key(vm, receiver(args)) == frame_key(vm, other))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let mut hash = 0i32;
        for unit in frame_key(vm, receiver(args)).encode_utf16() {
            hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
        }
        int(hash)
    }),
];

fn frame_key(vm: &Vm<'_>, frame: ObjectRef) -> alloc::string::String {
    alloc::format!(
        "{}:{}:{}",
        vm.string_or_empty(frame_field(vm, frame, "className")),
        vm.string_or_empty(frame_field(vm, frame, "methodName")),
        crate::lang::read_int_field(vm, frame, "lineNumber")
    )
}

pub(crate) const OJVM_STACK_FRAME: oxjvm_vm::NativeClass = class(
    "java/lang/OjvmStackFrame",
    Some("java/lang/Object"),
    &["java/lang/StackWalker$StackFrame"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[
        field("className", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("methodName", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("fileName", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("lineNumber", "I", ACC_PRIVATE, None),
    ],
    &OJVM_STACK_FRAME_METHODS,
    None,
);

fn new_stack_frame(
    vm: &mut Vm<'_>,
    frame: &oxjvm_vm::BacktraceFrame,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/lang/OjvmStackFrame")?;
    let instance = vm.new_instance(class)?;
    let class_name = vm.make_string(&frame.class_name)?;
    let (declaring, field) = vm.find_field(class, "className", "Ljava/lang/String;")?;
    vm.set_instance_ref(instance, declaring, field, class_name);
    let method_name = vm.make_string(&frame.method_name)?;
    let (declaring, field) = vm.find_field(class, "methodName", "Ljava/lang/String;")?;
    vm.set_instance_ref(instance, declaring, field, method_name);
    if let Some(file) = &frame.file_name {
        let file_name = vm.make_string(file)?;
        let (declaring, field) = vm.find_field(class, "fileName", "Ljava/lang/String;")?;
        vm.set_instance_ref(instance, declaring, field, file_name);
    }
    if let Some(line) = frame.line_number {
        if let Some((declaring, field)) = vm.find_field_opt(class, "lineNumber", "I") {
            vm.set_instance_int(instance, declaring, field, line as i32);
        }
    }
    Ok(instance)
}

// -------------------------------------------------------------------------------------------
// StackWalker
// -------------------------------------------------------------------------------------------

fn stack_walker_retain(vm: &mut Vm<'_>, value: Value) -> Result<bool, VmError> {
    match value {
        Value::Ref(reference) if !reference.is_null() => {
            if let Some(elements) = object_elements(vm, reference) {
                for element in elements {
                    if option_index(vm, element) == 0 {
                        return Ok(true);
                    }
                }
                return Ok(false);
            }
            // Treat a bare Option as a single value.
            if vm
                .class_name(vm.class_of(reference))
                .ends_with("StackWalker$Option")
            {
                return Ok(option_index(vm, reference) == 0);
            }
            Ok(false)
        }
        _ => Ok(false),
    }
}

fn object_elements(vm: &mut Vm<'_>, reference: ObjectRef) -> Option<Vec<ObjectRef>> {
    if vm
        .heap
        .get(reference)
        .is_some_and(|object| matches!(&object.data, oxjvm_vm::ObjectData::Array(_)))
    {
        let length = vm.array_length(reference).ok()?;
        let mut elements = Vec::new();
        for index in 0..length {
            elements.push(vm.array_get(reference, index as i32).ok()?.as_ref());
        }
        return Some(elements);
    }
    if vm.class_name(vm.class_of(reference)) == "java/util/HashSet" {
        return crate::util::collection_elements(vm, reference).ok();
    }
    None
}

fn stack_walker_instance(vm: &mut Vm<'_>, retain: bool) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/lang/StackWalker")?;
    let instance = vm.new_instance(class)?;
    if let Some((declaring, field)) = vm.find_field_opt(class, "retainClass", "I") {
        vm.set_instance_int(instance, declaring, field, i32::from(retain));
    }
    Ok(instance)
}

fn stack_frames_stream(vm: &mut Vm<'_>) -> Result<Value, VmError> {
    let frames = vm.capture_frames();
    let mut elements = Vec::new();
    for frame in &frames {
        elements.push(new_stack_frame(vm, frame)?);
    }
    object(crate::stream::stream_from(vm, elements)?)
}

const STACK_WALKER_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method(
        "getInstance",
        "()Ljava/lang/StackWalker;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let instance = stack_walker_instance(vm, false)?;
            object(instance)
        },
    ),
    method(
        "getInstance",
        "([Ljava/lang/StackWalker$Option;)Ljava/lang/StackWalker;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let retain = stack_walker_retain(vm, args[0])?;
            let instance = stack_walker_instance(vm, retain)?;
            object(instance)
        },
    ),
    method(
        "getInstance",
        "(Ljava/lang/StackWalker$Option;)Ljava/lang/StackWalker;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let retain = stack_walker_retain(vm, args[0])?;
            let instance = stack_walker_instance(vm, retain)?;
            object(instance)
        },
    ),
    method(
        "getInstance",
        "(Ljava/util/Set;I)Ljava/lang/StackWalker;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let retain = stack_walker_retain(vm, args[0])?;
            let instance = stack_walker_instance(vm, retain)?;
            object(instance)
        },
    ),
    method(
        "getInstance",
        "(Ljava/util/Set;)Ljava/lang/StackWalker;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let retain = stack_walker_retain(vm, args[0])?;
            let instance = stack_walker_instance(vm, retain)?;
            object(instance)
        },
    ),
    method(
        "walk",
        "(Ljava/util/function/Function;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let function = ref_arg(args, 1);
            let stream = stack_frames_stream(vm)?.as_ref();
            let (class, method) = vm.resolve_virtual_method(
                function,
                "apply",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
            )?;
            vm.invoke_method(
                class,
                method,
                vec![Value::Ref(function), Value::Ref(stream)],
            )
        },
    ),
    method(
        "getCallerClass",
        "()Ljava/lang/Class;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let retain = crate::lang::read_int_field(vm, this, "retainClass") != 0;
            if !retain {
                return Err(vm.throw_new(
                    "java/lang/UnsupportedOperationException",
                    Some("This stack walker does not have RETAIN_CLASS_REFERENCE access"),
                ));
            }
            let frames = vm.capture_frames();
            let Some(frame) = frames.get(1) else {
                return Err(vm.throw_new("java/lang/IllegalStateException", None));
            };
            let class: ClassId = vm.resolve_class(&frame.class_name)?;
            object(vm.class_object(class)?)
        },
    ),
    method(
        "forEach",
        "(Ljava/util/function/Consumer;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let consumer = ref_arg(args, 1);
            let frames = vm.capture_frames();
            let mut elements = Vec::new();
            for frame in &frames {
                elements.push(new_stack_frame(vm, frame)?);
            }
            let (class, method) =
                vm.resolve_virtual_method(consumer, "accept", "(Ljava/lang/Object;)V")?;
            for element in elements {
                vm.invoke_method(
                    class,
                    method,
                    vec![Value::Ref(consumer), Value::Ref(element)],
                )?;
            }
            void()
        },
    ),
];

pub(crate) const STACK_WALKER: oxjvm_vm::NativeClass = class(
    "java/lang/StackWalker",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[field("retainClass", "I", ACC_PRIVATE, None)],
    &STACK_WALKER_METHODS,
    None,
);
