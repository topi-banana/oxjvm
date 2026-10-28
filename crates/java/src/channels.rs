//! `java.nio.channels` socket channels and the `Channels` adapters.
//!
//! oxjvm has no host sockets, so the channel objects model their state (open, connected,
//! blocking) and the byte-oriented operations are implemented over guest streams.  Connecting
//! succeeds without a peer; reading from a `SocketChannel` reports end-of-stream.

use alloc::vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::{boolean, class, field, int, method, object, receiver, ref_arg, void};

fn new_instance(vm: &mut Vm<'_>, class_name: &str) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class(class_name)?;
    let instance = vm.new_instance(class)?;
    if let Some((declaring, ctor)) = vm.find_method(class, "<init>", "()V") {
        vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
    }
    Ok(instance)
}

fn bool_field(vm: &Vm<'_>, this: ObjectRef, name: &str) -> bool {
    vm.read_instance_int_named(this, vm.class_of(this), name, "Z")
        .unwrap_or(0)
        != 0
}

fn set_bool(vm: &mut Vm<'_>, this: ObjectRef, name: &str, value: bool) -> Result<(), VmError> {
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, name, "Z")?;
    vm.set_instance_int(this, declaring, field, i32::from(value));
    Ok(())
}

fn set_int(vm: &mut Vm<'_>, this: ObjectRef, name: &str, value: i32) -> Result<(), VmError> {
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, name, "I")?;
    vm.set_instance_int(this, declaring, field, value);
    Ok(())
}

fn ref_field(vm: &Vm<'_>, this: ObjectRef, name: &str, descriptor: &str) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), name, descriptor)
        .unwrap_or(ObjectRef::NULL)
}

fn set_ref(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: &str,
    descriptor: &str,
    value: ObjectRef,
) -> Result<(), VmError> {
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, name, descriptor)?;
    vm.set_instance_ref(this, declaring, field, value);
    Ok(())
}

fn new_empty_set(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    new_instance(vm, "java/util/HashSet")
}

fn buffer_remaining(vm: &mut Vm<'_>, buffer: ObjectRef) -> Result<i32, VmError> {
    let (class, method) = vm.resolve_virtual_method(buffer, "remaining", "()I")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(buffer)])?
        .as_int())
}

fn buffer_position(vm: &mut Vm<'_>, buffer: ObjectRef) -> Result<i32, VmError> {
    let (class, method) = vm.resolve_virtual_method(buffer, "position", "()I")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(buffer)])?
        .as_int())
}

fn buffer_limit(vm: &mut Vm<'_>, buffer: ObjectRef) -> Result<i32, VmError> {
    let (class, method) = vm.resolve_virtual_method(buffer, "limit", "()I")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(buffer)])?
        .as_int())
}

fn buffer_put_byte(vm: &mut Vm<'_>, buffer: ObjectRef, byte: u8) -> Result<(), VmError> {
    let (class, method) = vm.resolve_virtual_method(buffer, "put", "(B)Ljava/nio/ByteBuffer;")?;
    vm.invoke_method(
        class,
        method,
        vec![Value::Ref(buffer), Value::Int(i32::from(byte))],
    )?;
    Ok(())
}

fn buffer_get_byte(vm: &mut Vm<'_>, buffer: ObjectRef) -> Result<u8, VmError> {
    let (class, method) = vm.resolve_virtual_method(buffer, "get", "()B")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(buffer)])?
        .as_int() as u8)
}

// -------------------------------------------------------------------------------------------
// SelectableChannel / AbstractSelectableChannel
// -------------------------------------------------------------------------------------------

const SELECTABLE_CHANNEL_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    abstract_method("validOps", "()I"),
    abstract_method("isRegistered", "()Z"),
    abstract_method(
        "keyFor",
        "(Ljava/nio/channels/Selector;)Ljava/nio/channels/SelectionKey;",
    ),
    abstract_method(
        "register",
        "(Ljava/nio/channels/Selector;I)Ljava/nio/channels/SelectionKey;",
    ),
    abstract_method(
        "register",
        "(Ljava/nio/channels/Selector;ILjava/lang/Object;)Ljava/nio/channels/SelectionKey;",
    ),
    abstract_method(
        "configureBlocking",
        "(Z)Ljava/nio/channels/SelectableChannel;",
    ),
    abstract_method("isBlocking", "()Z"),
    abstract_method("blockingLock", "()Ljava/lang/Object;"),
];

const fn abstract_method(
    name: &'static str,
    descriptor: &'static str,
) -> oxjvm_vm::NativeMethodDef {
    method(
        name,
        descriptor,
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    )
}

pub(crate) const SELECTABLE_CHANNEL: oxjvm_vm::NativeClass = class(
    "java/nio/channels/SelectableChannel",
    Some("java/nio/channels/spi/AbstractInterruptibleChannel"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[field("blocking", "Z", ACC_PRIVATE, None)],
    &SELECTABLE_CHANNEL_METHODS,
    None,
);

pub(crate) const ABSTRACT_SELECTABLE_CHANNEL: oxjvm_vm::NativeClass = class(
    "java/nio/channels/spi/AbstractSelectableChannel",
    Some("java/nio/channels/SelectableChannel"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

const ABSTRACT_INTERRUPTIBLE_CHANNEL_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method("isOpen", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("implCloseChannel", "()V", ACC_PROTECTED, |_, _, _| void()),
];

pub(crate) const ABSTRACT_INTERRUPTIBLE_CHANNEL: oxjvm_vm::NativeClass = class(
    "java/nio/channels/spi/AbstractInterruptibleChannel",
    Some("java/lang/Object"),
    &["java/nio/channels/Channel"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &ABSTRACT_INTERRUPTIBLE_CHANNEL_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// SocketChannel
// -------------------------------------------------------------------------------------------

fn socket_channel_init(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    set_bool(vm, this, "open", true)?;
    set_bool(vm, this, "blocking", true)?;
    set_bool(vm, this, "connected", false)?;
    Ok(())
}

fn socket_channel_finish_connect(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    set_bool(vm, this, "connected", true)?;
    set_bool(vm, this, "connectionPending", false)?;
    Ok(())
}

const SOCKET_CHANNEL_METHODS: [oxjvm_vm::NativeMethodDef; 29] = [
    method("<init>", "()V", ACC_PROTECTED, |vm, _, args| {
        socket_channel_init(vm, receiver(args))?;
        void()
    }),
    method(
        "open",
        "()Ljava/nio/channels/SocketChannel;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_instance(vm, "java/nio/channels/OjvmSocketChannel")?),
    ),
    method(
        "open",
        "(Ljava/net/ProtocolFamily;)Ljava/nio/channels/SocketChannel;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_instance(vm, "java/nio/channels/OjvmSocketChannel")?),
    ),
    method(
        "open",
        "(Ljava/net/SocketAddress;)Ljava/nio/channels/SocketChannel;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let channel = new_instance(vm, "java/nio/channels/OjvmSocketChannel")?;
            if !ref_arg(args, 0).is_null() {
                socket_channel_finish_connect(vm, channel)?;
            }
            object(channel)
        },
    ),
    method("read", "(Ljava/nio/ByteBuffer;)I", ACC_PUBLIC, |_, _, _| {
        int(-1)
    }),
    method(
        "read",
        "([Ljava/nio/ByteBuffer;II)J",
        ACC_PUBLIC,
        |_, _, _| Ok(Value::Long(-1)),
    ),
    method(
        "write",
        "(Ljava/nio/ByteBuffer;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let buffer = ref_arg(args, 1);
            if buffer.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let count = buffer_remaining(vm, buffer)?;
            let limit = buffer_limit(vm, buffer)?;
            let (class, method) =
                vm.resolve_virtual_method(buffer, "position", "(I)Ljava/nio/Buffer;")?;
            vm.invoke_method(class, method, vec![Value::Ref(buffer), Value::Int(limit)])?;
            int(count)
        },
    ),
    method(
        "write",
        "([Ljava/nio/ByteBuffer;II)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let buffers = ref_arg(args, 1);
            if buffers.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let offset = crate::int_arg(args, 2).max(0) as usize;
            let length = crate::int_arg(args, 3).max(0) as usize;
            let mut total: i64 = 0;
            for index in offset..offset + length {
                let buffer = vm.array_get(buffers, index as i32)?.as_ref();
                if buffer.is_null() {
                    continue;
                }
                let count = buffer_remaining(vm, buffer)?;
                let limit = buffer_limit(vm, buffer)?;
                let (class, method) =
                    vm.resolve_virtual_method(buffer, "position", "(I)Ljava/nio/Buffer;")?;
                vm.invoke_method(class, method, vec![Value::Ref(buffer), Value::Int(limit)])?;
                total += i64::from(count);
            }
            Ok(Value::Long(total))
        },
    ),
    method(
        "connect",
        "(Ljava/net/SocketAddress;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            socket_channel_finish_connect(vm, this)?;
            boolean(true)
        },
    ),
    method("finishConnect", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        socket_channel_finish_connect(vm, this)?;
        boolean(true)
    }),
    method("isConnected", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(bool_field(vm, receiver(args), "connected"))
    }),
    method("isConnectionPending", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(bool_field(vm, receiver(args), "connectionPending"))
    }),
    method(
        "getRemoteAddress",
        "()Ljava/net/SocketAddress;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "remote",
                "Ljava/net/SocketAddress;",
            ))
        },
    ),
    method(
        "getLocalAddress",
        "()Ljava/net/SocketAddress;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "local",
                "Ljava/net/SocketAddress;",
            ))
        },
    ),
    method("socket", "()Ljava/net/Socket;", ACC_PUBLIC, |vm, _, _| {
        object(new_instance(vm, "java/net/Socket")?)
    }),
    method("isOpen", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(bool_field(vm, receiver(args), "open"))
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        set_bool(vm, this, "open", false)?;
        set_bool(vm, this, "connected", false)?;
        void()
    }),
    method(
        "shutdownInput",
        "()Ljava/nio/channels/SocketChannel;",
        ACC_PUBLIC,
        |vm, _, args| {
            set_bool(vm, receiver(args), "inputShutdown", true)?;
            object(receiver(args))
        },
    ),
    method(
        "shutdownOutput",
        "()Ljava/nio/channels/SocketChannel;",
        ACC_PUBLIC,
        |vm, _, args| {
            set_bool(vm, receiver(args), "outputShutdown", true)?;
            object(receiver(args))
        },
    ),
    method("isInputShutdown", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(bool_field(vm, receiver(args), "inputShutdown"))
    }),
    method("isOutputShutdown", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(bool_field(vm, receiver(args), "outputShutdown"))
    }),
    method(
        "configureBlocking",
        "(Z)Ljava/nio/channels/SelectableChannel;",
        ACC_PUBLIC,
        |vm, _, args| {
            set_bool(vm, receiver(args), "blocking", crate::bool_arg(args, 1))?;
            object(receiver(args))
        },
    ),
    method("isBlocking", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(bool_field(vm, receiver(args), "blocking"))
    }),
    method(
        "register",
        "(Ljava/nio/channels/Selector;I)Ljava/nio/channels/SelectionKey;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "register",
        "(Ljava/nio/channels/Selector;ILjava/lang/Object;)Ljava/nio/channels/SelectionKey;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method("validOps", "()I", ACC_PUBLIC, |_, _, _| int(13)),
    method("isRegistered", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method(
        "keyFor",
        "(Ljava/nio/channels/Selector;)Ljava/nio/channels/SelectionKey;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "blockingLock",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
];

pub(crate) const SOCKET_CHANNEL: oxjvm_vm::NativeClass = class(
    "java/nio/channels/SocketChannel",
    Some("java/nio/channels/spi/AbstractSelectableChannel"),
    &[
        "java/nio/channels/ByteChannel",
        "java/nio/channels/GatheringByteChannel",
        "java/nio/channels/ScatteringByteChannel",
    ],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &SOCKET_CHANNEL_METHODS,
    None,
);

const OJVM_SOCKET_CHANNEL_METHODS: [oxjvm_vm::NativeMethodDef; 1] =
    [method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        socket_channel_init(vm, receiver(args))?;
        void()
    })];

pub(crate) const OJVM_SOCKET_CHANNEL: oxjvm_vm::NativeClass = class(
    "java/nio/channels/OjvmSocketChannel",
    Some("java/nio/channels/SocketChannel"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[
        field("open", "Z", ACC_PRIVATE, None),
        field("blocking", "Z", ACC_PRIVATE, None),
        field("connected", "Z", ACC_PRIVATE, None),
        field("connectionPending", "Z", ACC_PRIVATE, None),
        field("inputShutdown", "Z", ACC_PRIVATE, None),
        field("outputShutdown", "Z", ACC_PRIVATE, None),
        field("local", "Ljava/net/SocketAddress;", ACC_PRIVATE, None),
        field("remote", "Ljava/net/SocketAddress;", ACC_PRIVATE, None),
    ],
    &OJVM_SOCKET_CHANNEL_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// ServerSocketChannel
// -------------------------------------------------------------------------------------------

const SERVER_SOCKET_CHANNEL_METHODS: [oxjvm_vm::NativeMethodDef; 20] = [
    method("<init>", "()V", ACC_PROTECTED, |vm, _, args| {
        let this = receiver(args);
        set_bool(vm, this, "open", true)?;
        set_bool(vm, this, "blocking", true)?;
        void()
    }),
    method(
        "open",
        "()Ljava/nio/channels/ServerSocketChannel;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            object(new_instance(
                vm,
                "java/nio/channels/OjvmServerSocketChannel",
            )?)
        },
    ),
    method(
        "open",
        "(Ljava/net/ProtocolFamily;)Ljava/nio/channels/ServerSocketChannel;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            object(new_instance(
                vm,
                "java/nio/channels/OjvmServerSocketChannel",
            )?)
        },
    ),
    method(
        "bind",
        "(Ljava/net/SocketAddress;)Ljava/nio/channels/ServerSocketChannel;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let (class, method) = vm.resolve_virtual_method(ref_arg(args, 1), "getPort", "()I")?;
            let port = vm
                .invoke_method(class, method, vec![Value::Ref(ref_arg(args, 1))])?
                .as_int();
            set_bool(vm, this, "bound", true)?;
            set_int(vm, this, "localPort", port)?;
            object(this)
        },
    ),
    method(
        "bind",
        "(Ljava/net/SocketAddress;I)Ljava/nio/channels/ServerSocketChannel;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let address = ref_arg(args, 1);
            if !address.is_null() {
                let (class, method) = vm.resolve_virtual_method(address, "getPort", "()I")?;
                let port = vm
                    .invoke_method(class, method, vec![Value::Ref(address)])?
                    .as_int();
                set_int(vm, this, "localPort", port)?;
            }
            set_int(vm, this, "backlog", crate::int_arg(args, 2))?;
            set_bool(vm, this, "bound", true)?;
            object(this)
        },
    ),
    method(
        "accept",
        "()Ljava/nio/channels/SocketChannel;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "socket",
        "()Ljava/net/ServerSocket;",
        ACC_PUBLIC,
        |vm, _, _| object(new_instance(vm, "java/net/ServerSocket")?),
    ),
    method(
        "getLocalAddress",
        "()Ljava/net/SocketAddress;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(ref_field(
                vm,
                receiver(args),
                "local",
                "Ljava/net/SocketAddress;",
            ))
        },
    ),
    method("isOpen", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(bool_field(vm, receiver(args), "open"))
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        set_bool(vm, receiver(args), "open", false)?;
        void()
    }),
    method(
        "configureBlocking",
        "(Z)Ljava/nio/channels/SelectableChannel;",
        ACC_PUBLIC,
        |vm, _, args| {
            set_bool(vm, receiver(args), "blocking", crate::bool_arg(args, 1))?;
            object(receiver(args))
        },
    ),
    method("isBlocking", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(bool_field(vm, receiver(args), "blocking"))
    }),
    method(
        "register",
        "(Ljava/nio/channels/Selector;I)Ljava/nio/channels/SelectionKey;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "register",
        "(Ljava/nio/channels/Selector;ILjava/lang/Object;)Ljava/nio/channels/SelectionKey;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method("validOps", "()I", ACC_PUBLIC, |_, _, _| int(16)),
    method("isRegistered", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method(
        "keyFor",
        "(Ljava/nio/channels/Selector;)Ljava/nio/channels/SelectionKey;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "blockingLock",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method(
        "setOption",
        "(Ljava/net/SocketOption;Ljava/lang/Object;)Ljava/nio/channels/ServerSocketChannel;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method(
        "getOption",
        "(Ljava/net/SocketOption;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
];

pub(crate) const SERVER_SOCKET_CHANNEL: oxjvm_vm::NativeClass = class(
    "java/nio/channels/ServerSocketChannel",
    Some("java/nio/channels/spi/AbstractSelectableChannel"),
    &["java/nio/channels/NetworkChannel"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &SERVER_SOCKET_CHANNEL_METHODS,
    None,
);

pub(crate) const OJVM_SERVER_SOCKET_CHANNEL: oxjvm_vm::NativeClass = class(
    "java/nio/channels/OjvmServerSocketChannel",
    Some("java/nio/channels/ServerSocketChannel"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[
        field("open", "Z", ACC_PRIVATE, None),
        field("blocking", "Z", ACC_PRIVATE, None),
        field("bound", "Z", ACC_PRIVATE, None),
        field("localPort", "I", ACC_PRIVATE, None),
        field("backlog", "I", ACC_PRIVATE, None),
        field("local", "Ljava/net/SocketAddress;", ACC_PRIVATE, None),
    ],
    &[],
    None,
);

// -------------------------------------------------------------------------------------------
// Selector / SelectionKey
// -------------------------------------------------------------------------------------------

pub(crate) const SELECTOR_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method(
        "open",
        "()Ljava/nio/channels/Selector;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(new_instance(vm, "java/nio/channels/OjvmSelector")?),
    ),
    method("selectNow", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method("select", "(J)I", ACC_PUBLIC, |_, _, _| int(0)),
    method("select", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method(
        "selectedKeys",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        |vm, _, _| object(new_empty_set(vm)?),
    ),
    method("keys", "()Ljava/util/Set;", ACC_PUBLIC, |vm, _, _| {
        object(new_empty_set(vm)?)
    }),
    method(
        "wakeup",
        "()Ljava/nio/channels/Selector;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("isOpen", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
    method(
        "provider",
        "()Ljava/nio/channels/spi/SelectorProvider;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
];

pub(crate) const SELECTOR: oxjvm_vm::NativeClass = class(
    "java/nio/channels/Selector",
    Some("java/lang/Object"),
    &["java/io/Closeable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &SELECTOR_METHODS,
    None,
);

pub(crate) const OJVM_SELECTOR: oxjvm_vm::NativeClass = class(
    "java/nio/channels/OjvmSelector",
    Some("java/nio/channels/Selector"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[],
    &[],
    None,
);

pub(crate) const SELECTION_KEY: oxjvm_vm::NativeClass = class(
    "java/nio/channels/SelectionKey",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[
        field(
            "OP_READ",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(1)),
        ),
        field(
            "OP_WRITE",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(4)),
        ),
        field(
            "OP_CONNECT",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(8)),
        ),
        field(
            "OP_ACCEPT",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(16)),
        ),
    ],
    &[
        abstract_method("channel", "()Ljava/nio/channels/SelectableChannel;"),
        abstract_method("selector", "()Ljava/nio/channels/Selector;"),
        abstract_method("isValid", "()Z"),
        abstract_method("cancel", "()V"),
        abstract_method("interestOps", "()I"),
        abstract_method("interestOps", "(I)Ljava/nio/channels/SelectionKey;"),
        abstract_method("readyOps", "()I"),
        abstract_method("attachment", "()Ljava/lang/Object;"),
        abstract_method("attach", "(Ljava/lang/Object;)Ljava/lang/Object;"),
    ],
    None,
);

// -------------------------------------------------------------------------------------------
// Adapter channels used by java.nio.channels.Channels
// -------------------------------------------------------------------------------------------

const INPUT_STREAM_CHANNEL_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "<init>",
        "(Ljava/io/InputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            set_ref(
                vm,
                receiver(args),
                "in",
                "Ljava/io/InputStream;",
                ref_arg(args, 1),
            )?;
            void()
        },
    ),
    method(
        "read",
        "(Ljava/nio/ByteBuffer;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let buffer = ref_arg(args, 1);
            if buffer.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let stream = ref_field(vm, this, "in", "Ljava/io/InputStream;");
            let (class, method) = vm.resolve_virtual_method(stream, "read", "()I")?;
            let value = vm
                .invoke_method(class, method, vec![Value::Ref(stream)])?
                .as_int();
            if value < 0 {
                return int(-1);
            }
            buffer_put_byte(vm, buffer, value as u8)?;
            int(1)
        },
    ),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let stream = ref_field(vm, this, "in", "Ljava/io/InputStream;");
        if !stream.is_null() {
            let (class, method) = vm.resolve_virtual_method(stream, "close", "()V")?;
            vm.invoke_method(class, method, vec![Value::Ref(stream)])?;
        }
        void()
    }),
    method("isOpen", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
];

pub(crate) const INPUT_STREAM_CHANNEL: oxjvm_vm::NativeClass = class(
    "java/nio/channels/OjvmInputStreamChannel",
    Some("java/lang/Object"),
    &["java/nio/channels/ReadableByteChannel"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field("in", "Ljava/io/InputStream;", ACC_PRIVATE, None)],
    &INPUT_STREAM_CHANNEL_METHODS,
    None,
);

const OUTPUT_STREAM_CHANNEL_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "<init>",
        "(Ljava/io/OutputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            set_ref(
                vm,
                receiver(args),
                "out",
                "Ljava/io/OutputStream;",
                ref_arg(args, 1),
            )?;
            void()
        },
    ),
    method(
        "write",
        "(Ljava/nio/ByteBuffer;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let buffer = ref_arg(args, 1);
            if buffer.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let stream = ref_field(vm, this, "out", "Ljava/io/OutputStream;");
            let remaining = buffer_remaining(vm, buffer)?;
            let position = buffer_position(vm, buffer)?;
            for _ in 0..remaining {
                let byte = buffer_get_byte(vm, buffer)?;
                let (class, method) = vm.resolve_virtual_method(stream, "write", "(I)V")?;
                vm.invoke_method(
                    class,
                    method,
                    vec![Value::Ref(stream), Value::Int(i32::from(byte))],
                )?;
            }
            let _ = position;
            int(remaining)
        },
    ),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let stream = ref_field(vm, this, "out", "Ljava/io/OutputStream;");
        if !stream.is_null() {
            let (class, method) = vm.resolve_virtual_method(stream, "close", "()V")?;
            vm.invoke_method(class, method, vec![Value::Ref(stream)])?;
        }
        void()
    }),
    method("isOpen", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
];

pub(crate) const OUTPUT_STREAM_CHANNEL: oxjvm_vm::NativeClass = class(
    "java/nio/channels/OjvmOutputStreamChannel",
    Some("java/lang/Object"),
    &["java/nio/channels/WritableByteChannel"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field("out", "Ljava/io/OutputStream;", ACC_PRIVATE, None)],
    &OUTPUT_STREAM_CHANNEL_METHODS,
    None,
);

const CHANNEL_INPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "<init>",
        "(Ljava/nio/channels/ReadableByteChannel;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            set_ref(
                vm,
                receiver(args),
                "ch",
                "Ljava/nio/channels/ReadableByteChannel;",
                ref_arg(args, 1),
            )?;
            void()
        },
    ),
    method("read", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let channel = ref_field(vm, this, "ch", "Ljava/nio/channels/ReadableByteChannel;");
        let buffer = allocate_one_byte_buffer(vm)?;
        let (class, method) =
            vm.resolve_virtual_method(channel, "read", "(Ljava/nio/ByteBuffer;)I")?;
        let count = vm
            .invoke_method(class, method, vec![Value::Ref(channel), Value::Ref(buffer)])?
            .as_int();
        if count <= 0 {
            return int(-1);
        }
        let (gc, gm) = vm.resolve_virtual_method(buffer, "get", "(I)B")?;
        let byte = vm.invoke_method(gc, gm, vec![Value::Ref(buffer), Value::Int(0)])?;
        int(byte.as_int())
    }),
    method("available", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let channel = ref_field(vm, this, "ch", "Ljava/nio/channels/ReadableByteChannel;");
        if !channel.is_null() {
            if let Ok((class, method)) = vm.resolve_virtual_method(channel, "close", "()V") {
                let _ = vm.invoke_method(class, method, vec![Value::Ref(channel)]);
            }
        }
        void()
    }),
];

pub(crate) const CHANNEL_INPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/nio/channels/OjvmChannelInputStream",
    Some("java/io/InputStream"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field(
        "ch",
        "Ljava/nio/channels/ReadableByteChannel;",
        ACC_PRIVATE,
        None,
    )],
    &CHANNEL_INPUT_STREAM_METHODS,
    None,
);

fn allocate_one_byte_buffer(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/nio/ByteBuffer")?;
    let (declaring, method) = vm
        .find_method(class, "allocate", "(I)Ljava/nio/ByteBuffer;")
        .ok_or_else(|| VmError::internal("ByteBuffer.allocate"))?;
    Ok(vm
        .invoke_method(declaring, method, vec![Value::Int(1)])?
        .as_ref())
}

const CHANNEL_OUTPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "<init>",
        "(Ljava/nio/channels/WritableByteChannel;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            set_ref(
                vm,
                receiver(args),
                "ch",
                "Ljava/nio/channels/WritableByteChannel;",
                ref_arg(args, 1),
            )?;
            void()
        },
    ),
    method("write", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let channel = ref_field(vm, this, "ch", "Ljava/nio/channels/WritableByteChannel;");
        let byte = crate::int_arg(args, 1) as u8;
        let class = vm.resolve_class("java/nio/ByteBuffer")?;
        let (declaring, method) = vm
            .find_method(class, "wrap", "([B)Ljava/nio/ByteBuffer;")
            .ok_or_else(|| VmError::internal("ByteBuffer.wrap"))?;
        let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, 1)?;
        vm.array_set(array, 0, Value::Int(i32::from(byte)), false)?;
        let buffer = vm
            .invoke_method(declaring, method, vec![Value::Ref(array)])?
            .as_ref();
        let (cc, cm) = vm.resolve_virtual_method(channel, "write", "(Ljava/nio/ByteBuffer;)I")?;
        vm.invoke_method(cc, cm, vec![Value::Ref(channel), Value::Ref(buffer)])?;
        void()
    }),
    method("flush", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let channel = ref_field(vm, this, "ch", "Ljava/nio/channels/WritableByteChannel;");
        if !channel.is_null() {
            if let Ok((class, method)) = vm.resolve_virtual_method(channel, "close", "()V") {
                let _ = vm.invoke_method(class, method, vec![Value::Ref(channel)]);
            }
        }
        void()
    }),
];

pub(crate) const CHANNEL_OUTPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/nio/channels/OjvmChannelOutputStream",
    Some("java/io/OutputStream"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field(
        "ch",
        "Ljava/nio/channels/WritableByteChannel;",
        ACC_PRIVATE,
        None,
    )],
    &CHANNEL_OUTPUT_STREAM_METHODS,
    None,
);

const CHANNELS_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "newChannel",
        "(Ljava/io/InputStream;)Ljava/nio/channels/ReadableByteChannel;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let channel = new_instance(vm, "java/nio/channels/OjvmInputStreamChannel")?;
            set_ref(vm, channel, "in", "Ljava/io/InputStream;", ref_arg(args, 0))?;
            object(channel)
        },
    ),
    method(
        "newChannel",
        "(Ljava/io/OutputStream;)Ljava/nio/channels/WritableByteChannel;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let channel = new_instance(vm, "java/nio/channels/OjvmOutputStreamChannel")?;
            set_ref(
                vm,
                channel,
                "out",
                "Ljava/io/OutputStream;",
                ref_arg(args, 0),
            )?;
            object(channel)
        },
    ),
    method(
        "newInputStream",
        "(Ljava/nio/channels/ReadableByteChannel;)Ljava/io/InputStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let stream = new_instance(vm, "java/nio/channels/OjvmChannelInputStream")?;
            set_ref(
                vm,
                stream,
                "ch",
                "Ljava/nio/channels/ReadableByteChannel;",
                ref_arg(args, 0),
            )?;
            object(stream)
        },
    ),
    method(
        "newOutputStream",
        "(Ljava/nio/channels/WritableByteChannel;)Ljava/io/OutputStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let stream = new_instance(vm, "java/nio/channels/OjvmChannelOutputStream")?;
            set_ref(
                vm,
                stream,
                "ch",
                "Ljava/nio/channels/WritableByteChannel;",
                ref_arg(args, 0),
            )?;
            object(stream)
        },
    ),
];

pub(crate) const CHANNELS: oxjvm_vm::NativeClass = class(
    "java/nio/channels/Channels",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &CHANNELS_METHODS,
    None,
);

pub(crate) const GATHERING_BYTE_CHANNEL: oxjvm_vm::NativeClass = class(
    "java/nio/channels/GatheringByteChannel",
    Some("java/lang/Object"),
    &["java/nio/channels/WritableByteChannel"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[abstract_method("write", "([Ljava/nio/ByteBuffer;II)J")],
    None,
);

pub(crate) const SCATTERING_BYTE_CHANNEL: oxjvm_vm::NativeClass = class(
    "java/nio/channels/ScatteringByteChannel",
    Some("java/lang/Object"),
    &["java/nio/channels/ReadableByteChannel"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[abstract_method("read", "([Ljava/nio/ByteBuffer;II)J")],
    None,
);

pub(crate) const NETWORK_CHANNEL: oxjvm_vm::NativeClass = class(
    "java/nio/channels/NetworkChannel",
    Some("java/lang/Object"),
    &["java/nio/channels/Channel"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[
        abstract_method(
            "bind",
            "(Ljava/net/SocketAddress;)Ljava/nio/channels/NetworkChannel;",
        ),
        abstract_method("getLocalAddress", "()Ljava/net/SocketAddress;"),
        abstract_method(
            "setOption",
            "(Ljava/net/SocketOption;Ljava/lang/Object;)Ljava/nio/channels/NetworkChannel;",
        ),
        abstract_method("getOption", "(Ljava/net/SocketOption;)Ljava/lang/Object;"),
        abstract_method("supportedOptions", "()Ljava/util/Set;"),
    ],
    None,
);

pub(crate) const SELECTOR_PROVIDER: oxjvm_vm::NativeClass = class(
    "java/nio/channels/spi/SelectorProvider",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

pub(crate) const SELECTION_KEY_IMPL: oxjvm_vm::NativeClass = class(
    "java/nio/channels/spi/AbstractSelectionKey",
    Some("java/nio/channels/SelectionKey"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &[],
    None,
);
