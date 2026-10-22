//! `java.lang.ProcessHandle`: a handle on a process of the host.

use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::{boolean, class, field, long, method, object, receiver};

/// A handle on the process with the given id.
fn handle_for(vm: &mut Vm<'_>, pid: i64) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/lang/OjvmProcessHandle")?;
    let instance = vm.new_instance(class)?;
    crate::lang::write_long_field(vm, instance, "pid", pid);
    object(instance)
}

/// The information the host has about a process.
pub(crate) fn process_info(vm: &mut Vm<'_>, pid: i64) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/lang/OjvmProcessHandle$Info")?;
    let instance = vm.new_instance(class)?;
    crate::lang::write_long_field(vm, instance, "pid", pid);
    let command = vm
        .host
        .processes()
        .into_iter()
        .find(|process| i64::from(process.pid) == pid)
        .map(|process| process.command)
        .unwrap_or_default();
    let text = vm.make_string(&command)?;
    let (declaring, field) = vm.find_field(class, "command", "Ljava/lang/String;")?;
    vm.set_instance_ref(instance, declaring, field, text);
    object(instance)
}

const PROCESS_HANDLE_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method(
        "current",
        "()Ljava/lang/ProcessHandle;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let pid = vm.host.current_process_id();
            handle_for(vm, i64::from(pid))
        },
    ),
    method(
        "of",
        "(J)Ljava/lang/ProcessHandle;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| handle_for(vm, crate::lang::read_long_field(vm, receiver(args), "pid")),
    ),
    method(
        "allProcesses",
        "()Ljava/util/stream/Stream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let pids = vm
                .host
                .processes()
                .into_iter()
                .map(|process| i64::from(process.pid))
                .collect::<Vec<i64>>();
            let mut values = Vec::with_capacity(pids.len());
            for pid in pids {
                values.push(handle_for(vm, pid)?.as_ref());
            }
            object(crate::stream::stream_from(vm, values)?)
        },
    ),
    method("pid", "()J", ACC_PUBLIC, |vm, _, args| {
        long(crate::lang::read_long_field(vm, receiver(args), "pid"))
    }),
    method("isAlive", "()Z", ACC_PUBLIC, |vm, _, args| {
        let pid = crate::lang::read_long_field(vm, receiver(args), "pid");
        boolean(
            vm.host
                .processes()
                .iter()
                .any(|process| i64::from(process.pid) == pid),
        )
    }),
    method(
        "info",
        "()Ljava/lang/ProcessHandle$Info;",
        ACC_PUBLIC,
        |vm, _, args| {
            let pid = crate::lang::read_long_field(vm, receiver(args), "pid");
            process_info(vm, pid)
        },
    ),
    method("destroy", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method(
        "destroyForcibly",
        "()Ljava/lang/ProcessHandle;",
        ACC_PUBLIC,
        |_vm, _, args| object(receiver(args)),
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let pid = crate::lang::read_long_field(vm, receiver(args), "pid");
            let text = alloc::format!("ProcessHandle[pid={pid}]");
            object(vm.make_string(&text)?)
        },
    ),
];

const PROCESS_HANDLE_INFO_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method("pid", "()J", ACC_PUBLIC, |vm, _, args| {
        long(crate::lang::read_long_field(vm, receiver(args), "pid"))
    }),
    method(
        "command",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = vm.class_of(receiver(args));
            let command = vm
                .read_ref_field(receiver(args), class, "command", "Ljava/lang/String;")
                .unwrap_or(ObjectRef::NULL);
            crate::optional::optional_new(vm, command)
        },
    ),
    method(
        "commandLine",
        "()Ljava/util/Optional;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = vm.class_of(receiver(args));
            let command = vm
                .read_ref_field(receiver(args), class, "command", "Ljava/lang/String;")
                .unwrap_or(ObjectRef::NULL);
            crate::optional::optional_new(vm, command)
        },
    ),
];

/// `java.lang.ProcessHandle`.
pub(crate) const PROCESS_HANDLE: oxjvm_vm::NativeClass = class(
    "java/lang/ProcessHandle",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &PROCESS_HANDLE_METHODS,
    None,
);

/// The handle the runtime hands to the guest.
pub(crate) const OJVM_PROCESS_HANDLE: oxjvm_vm::NativeClass = class(
    "java/lang/OjvmProcessHandle",
    Some("java/lang/Object"),
    &["java/lang/ProcessHandle"],
    ACC_PUBLIC | ACC_FINAL,
    &[field("pid", "J", ACC_PRIVATE, None)],
    &[],
    None,
);

/// `java.lang.ProcessHandle$Info`.
pub(crate) const PROCESS_HANDLE_INFO: oxjvm_vm::NativeClass = class(
    "java/lang/ProcessHandle$Info",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &PROCESS_HANDLE_INFO_METHODS,
    None,
);

/// The information the runtime hands to the guest.
pub(crate) const OJVM_PROCESS_HANDLE_INFO: oxjvm_vm::NativeClass = class(
    "java/lang/OjvmProcessHandle$Info",
    Some("java/lang/Object"),
    &["java/lang/ProcessHandle$Info"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("pid", "J", ACC_PRIVATE, None),
        field("command", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &[],
    None,
);
