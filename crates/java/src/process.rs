//! `java.lang.Process`, the process a `ProcessBuilder` starts, and `java.lang.ProcessHandle`.
//!
//! The runtime runs a program through the host and keeps what it produced: the exit status and
//! the bytes the program wrote to its standard output and error.  A guest reads that output the
//! way the JDK lets it read the output of a finished process.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::{boolean, class, field, int, long, method, object, receiver, void};

/// Run a program and describe the result as a `java.lang.Process`.
fn spawn(
    vm: &mut Vm<'_>,
    program: &str,
    args: &[String],
    directory: Option<&str>,
) -> Result<ObjectRef, VmError> {
    let result = vm
        .host
        .run_process(program, args, directory)
        .map_err(|error| vm.throw_new("java/io/IOException", Some(&error.to_string())))?;
    let class = vm.resolve_class("java/lang/OjvmProcess")?;
    let instance = vm.new_instance(class)?;
    crate::lang::write_long_field(vm, instance, "pid", i64::from(result.pid));
    crate::lang::write_int_field(vm, instance, "status", result.status);
    let stdout = byte_array(vm, &result.stdout)?;
    let stderr = byte_array(vm, &result.stderr)?;
    let (declaring, field) = vm.find_field(class, "stdout", "[B")?;
    vm.set_instance_ref(instance, declaring, field, stdout);
    let (declaring, field) = vm.find_field(class, "stderr", "[B")?;
    vm.set_instance_ref(instance, declaring, field, stderr);
    Ok(instance)
}

/// A byte array holding the given bytes.
fn byte_array(vm: &mut Vm<'_>, bytes: &[u8]) -> Result<ObjectRef, VmError> {
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, bytes.len())?;
    for (index, byte) in bytes.iter().enumerate() {
        vm.array_set(
            array,
            index as i32,
            Value::Int(i32::from(*byte as i8)),
            false,
        )?;
    }
    Ok(array)
}

/// The bytes of a process output, as a `ByteArrayInputStream`.
fn output_stream(vm: &mut Vm<'_>, bytes: ObjectRef) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/io/ByteArrayInputStream")?;
    let instance = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(class, "<init>", "([B)V")
        .ok_or_else(|| VmError::internal("ByteArrayInputStream lacks its constructor"))?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![Value::Ref(instance), Value::Ref(bytes)],
    )?;
    object(instance)
}

/// The command of a `ProcessBuilder`, split into the program and its arguments.
fn process_builder_command(vm: &mut Vm<'_>, this: ObjectRef) -> Vec<String> {
    let class = vm.class_of(this);
    let command = vm
        .read_ref_field(this, class, "command", "Ljava/util/List;")
        .unwrap_or(ObjectRef::NULL);
    crate::util::array_list_elements(vm, command)
        .into_iter()
        .map(|element| vm.string_or_empty(element))
        .collect()
}

/// Start the program a `ProcessBuilder` describes.
pub(crate) fn process_builder_start(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Value, VmError> {
    let command = process_builder_command(vm, this);
    let Some(program) = command.first() else {
        return Err(vm.throw_new("java/io/IOException", Some("no command")));
    };
    let directory = vm
        .read_ref_field(this, vm.class_of(this), "directory", "Ljava/io/File;")
        .filter(|file| !file.is_null())
        .map(|file| crate::file::file_path(vm, file));
    let process = spawn(vm, program, &command[1..], directory.as_deref())?;
    object(process)
}

/// `Runtime.exec(String)`: the command is split on whitespace, the way a shell does.
pub(crate) fn runtime_exec(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let mut command = Vec::new();
    let first = args[1].as_ref();
    if !first.is_null() {
        let text = vm.string_or_empty(first);
        for word in text.split_whitespace() {
            command.push(word.to_string());
        }
    }
    if let Some(array) = args.get(2).map(|value| value.as_ref()) {
        if !array.is_null() {
            for element in crate::util::array_list_elements(vm, array) {
                command.push(vm.string_or_empty(element));
            }
        }
    }
    let Some(program) = command.first().cloned() else {
        return Err(vm.throw_new("java/io/IOException", Some("no command")));
    };
    let process = spawn(vm, &program, &command[1..], None)?;
    object(process)
}

/// `Runtime.exec(String[])`: the array names the program and its arguments.
pub(crate) fn runtime_exec_array(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let array = args[1].as_ref();
    let mut command = Vec::new();
    if !array.is_null() {
        let length = vm.array_length(array)?;
        for index in 0..length {
            let element = vm.array_get(array, index as i32)?.as_ref();
            command.push(vm.string_or_empty(element));
        }
    }
    let Some(program) = command.first().cloned() else {
        return Err(vm.throw_new("java/io/IOException", Some("no command")));
    };
    let process = spawn(vm, &program, &command[1..], None)?;
    object(process)
}

const PROCESS_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
    method("pid", "()J", ACC_PUBLIC, |vm, _, args| {
        long(crate::lang::read_long_field(vm, receiver(args), "pid"))
    }),
    method("waitFor", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "status"))
    }),
    method("exitValue", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "status"))
    }),
    method("isAlive", "()Z", ACC_PUBLIC, |_vm, _, _args| {
        // The runtime waits for the program before it hands the process over.
        boolean(false)
    }),
    method(
        "getInputStream",
        "()Ljava/io/InputStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = vm.class_of(receiver(args));
            let bytes = vm
                .read_ref_field(receiver(args), class, "stdout", "[B")
                .unwrap_or(ObjectRef::NULL);
            output_stream(vm, bytes)
        },
    ),
    method(
        "getErrorStream",
        "()Ljava/io/InputStream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = vm.class_of(receiver(args));
            let bytes = vm
                .read_ref_field(receiver(args), class, "stderr", "[B")
                .unwrap_or(ObjectRef::NULL);
            output_stream(vm, bytes)
        },
    ),
    method(
        "getOutputStream",
        "()Ljava/io/OutputStream;",
        ACC_PUBLIC,
        |vm, _, _| {
            // The program already ran, so its input goes nowhere.
            let class = vm.resolve_class("java/io/OutputStream")?;
            object(vm.new_instance(class)?)
        },
    ),
    method("destroy", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "waitFor",
        "(JLjava/util/concurrent/TimeUnit;)Z",
        ACC_PUBLIC,
        |_vm, _, args| {
            // The runtime waits for the program before it hands the process over.
            let _ = (args[1].as_long(), args[2].as_ref());
            boolean(true)
        },
    ),
    method(
        "inputReader",
        "()Ljava/io/BufferedReader;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = vm.class_of(receiver(args));
            let bytes = vm
                .read_ref_field(receiver(args), class, "stdout", "[B")
                .unwrap_or(ObjectRef::NULL);
            let stream = output_stream(vm, bytes)?.as_ref();
            let reader = vm.resolve_class("java/io/BufferedReader")?;
            let instance = vm.new_instance(reader)?;
            let (declaring, ctor) = vm
                .find_method(reader, "<init>", "(Ljava/io/Reader;)V")
                .ok_or_else(|| VmError::internal("BufferedReader lacks its constructor"))?;
            vm.invoke_method(
                declaring,
                ctor,
                vec![Value::Ref(instance), Value::Ref(stream)],
            )?;
            object(instance)
        },
    ),
    method(
        "outputWriter",
        "()Ljava/io/BufferedWriter;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = vm.class_of(receiver(args));
            let bytes = vm
                .read_ref_field(receiver(args), class, "stdout", "[B")
                .unwrap_or(ObjectRef::NULL);
            let stream = output_stream(vm, bytes)?.as_ref();
            let writer = vm.resolve_class("java/io/BufferedWriter")?;
            let instance = vm.new_instance(writer)?;
            let (declaring, ctor) = vm
                .find_method(writer, "<init>", "(Ljava/io/Writer;)V")
                .ok_or_else(|| VmError::internal("BufferedWriter lacks its constructor"))?;
            vm.invoke_method(
                declaring,
                ctor,
                vec![Value::Ref(instance), Value::Ref(stream)],
            )?;
            object(instance)
        },
    ),
    method(
        "destroyForcibly",
        "()Ljava/lang/Process;",
        ACC_PUBLIC,
        |_vm, _, args| object(receiver(args)),
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let pid = crate::lang::read_long_field(vm, receiver(args), "pid");
            let text = alloc::format!("Process[pid={pid}]");
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "info",
        "()Ljava/lang/ProcessHandle$Info;",
        ACC_PUBLIC,
        |vm, _, args| {
            let pid = crate::lang::read_long_field(vm, receiver(args), "pid");
            crate::process_handle::process_info(vm, pid)
        },
    ),
];

/// `java.lang.Process`: the handle a guest gets for a program it started.
pub(crate) const PROCESS: oxjvm_vm::NativeClass = class(
    "java/lang/Process",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &PROCESS_METHODS,
    None,
);

/// The process the runtime hands to the guest.
pub(crate) const OJVM_PROCESS: oxjvm_vm::NativeClass = class(
    "java/lang/OjvmProcess",
    Some("java/lang/Process"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("pid", "J", ACC_PRIVATE, None),
        field("status", "I", ACC_PRIVATE, None),
        field("stdout", "[B", ACC_PRIVATE, None),
        field("stderr", "[B", ACC_PRIVATE, None),
    ],
    &[],
    None,
);
