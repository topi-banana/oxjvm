//! `java.io`: the stream classes oxjvm implements natively.

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError, format};

use crate::{class, field, int, int_arg, method, object, receiver, ref_arg, void};

// -------------------------------------------------------------------------------------------
// Exceptions
// -------------------------------------------------------------------------------------------

const IO_EXCEPTION_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "<init>",
        "()V",
        ACC_PUBLIC,
        crate::throwable::exception_ctor,
    ),
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        crate::throwable::exception_ctor,
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/Throwable;)V",
        ACC_PUBLIC,
        crate::throwable::exception_ctor,
    ),
    method(
        "<init>",
        "(Ljava/lang/Throwable;)V",
        ACC_PUBLIC,
        crate::throwable::exception_ctor,
    ),
];

macro_rules! io_exception {
    ($constant:ident, $name:literal, $super:literal) => {
        pub(crate) const $constant: oxjvm_vm::NativeClass = class(
            $name,
            Some($super),
            &[],
            ACC_PUBLIC,
            &[],
            &IO_EXCEPTION_METHODS,
            None,
        );
    };
}

io_exception!(IO_EXCEPTION, "java/io/IOException", "java/lang/Exception");
io_exception!(
    FILE_NOT_FOUND_EXCEPTION,
    "java/io/FileNotFoundException",
    "java/io/IOException"
);
io_exception!(
    UNSUPPORTED_ENCODING_EXCEPTION,
    "java/io/UnsupportedEncodingException",
    "java/io/IOException"
);
io_exception!(EOF_EXCEPTION, "java/io/EOFException", "java/io/IOException");

pub(crate) const CLOSEABLE: oxjvm_vm::NativeClass = class(
    "java/io/Closeable",
    Some("java/lang/Object"),
    &["java/lang/AutoCloseable"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[method(
        "close",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    )],
    None,
);

pub(crate) const FLUSHABLE: oxjvm_vm::NativeClass = class(
    "java/io/Flushable",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[method(
        "flush",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    )],
    None,
);

// -------------------------------------------------------------------------------------------
// OutputStream / PrintStream
// -------------------------------------------------------------------------------------------

const OUTPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "write",
        "(I)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method("write", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let _class = vm.class_of(this);
        let length = vm.array_length(ref_arg(args, 1))?;
        let array = ref_arg(args, 1);
        let (declaring, method) = vm.resolve_virtual_method(this, "write", "([BII)V")?;
        vm.invoke_method(
            declaring,
            method,
            vec![
                Value::Ref(this),
                Value::Ref(array),
                Value::Int(0),
                Value::Int(length as i32),
            ],
        )?;
        void()
    }),
    method("write", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        let array = ref_arg(args, 1);
        for index in int_arg(args, 2)..int_arg(args, 2) + int_arg(args, 3) {
            let byte = vm.array_get(array, index)?.as_int();
            let (declaring, method) = vm.resolve_virtual_method(receiver(args), "write", "(I)V")?;
            vm.invoke_method(
                declaring,
                method,
                vec![Value::Ref(receiver(args)), Value::Int(byte)],
            )?;
        }
        void()
    }),
    method("flush", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
];

pub(crate) const OUTPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/io/OutputStream",
    Some("java/lang/Object"),
    &["java/io/Closeable", "java/io/Flushable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &OUTPUT_STREAM_METHODS,
    None,
);

const FILTER_OUTPUT_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "<init>",
        "(Ljava/io/OutputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            if let Ok((declaring, field)) = vm.find_field(class, "out", "Ljava/io/OutputStream;") {
                vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            }
            void()
        },
    ),
    method("write", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let out = vm
            .read_ref_field(this, vm.class_of(this), "out", "Ljava/io/OutputStream;")
            .unwrap_or(ObjectRef::NULL);
        let (declaring, method) = vm.resolve_virtual_method(out, "write", "(I)V")?;
        vm.invoke_method(
            declaring,
            method,
            vec![Value::Ref(out), Value::Int(int_arg(args, 1))],
        )?;
        void()
    }),
    method("flush", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let out = vm
            .read_ref_field(this, vm.class_of(this), "out", "Ljava/io/OutputStream;")
            .unwrap_or(ObjectRef::NULL);
        if !out.is_null() {
            let (declaring, method) = vm.resolve_virtual_method(out, "flush", "()V")?;
            vm.invoke_method(declaring, method, vec![Value::Ref(out)])?;
        }
        void()
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let out = vm
            .read_ref_field(
                receiver(args),
                vm.class_of(receiver(args)),
                "out",
                "Ljava/io/OutputStream;",
            )
            .unwrap_or(ObjectRef::NULL);
        if !out.is_null() {
            let (declaring, method) = vm.resolve_virtual_method(out, "close", "()V")?;
            vm.invoke_method(declaring, method, vec![Value::Ref(out)])?;
        }
        void()
    }),
];

pub(crate) const FILTER_OUTPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/io/FilterOutputStream",
    Some("java/io/OutputStream"),
    &[],
    ACC_PUBLIC,
    &[field("out", "Ljava/io/OutputStream;", ACC_PROTECTED, None)],
    &FILTER_OUTPUT_METHODS,
    None,
);

const PRINT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 22] = [
    method(
        "<init>",
        "(Ljava/io/OutputStream;)V",
        ACC_PUBLIC,
        print_stream_init,
    ),
    method("<init>", "(I)V", ACC_PUBLIC, print_stream_init_kind),
    method("print", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        let text = if crate::bool_arg(args, 1) {
            "true"
        } else {
            "false"
        };
        print_text(vm, receiver(args), text)
    }),
    method("print", "(C)V", ACC_PUBLIC, |_, _, _| void()),
    method("print", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let text = format::int_to_string(int_arg(args, 1));
        print_text(vm, receiver(args), &text)
    }),
    method("print", "(J)V", ACC_PUBLIC, |vm, _, args| {
        let text = format::long_to_string(crate::long_arg(args, 1));
        print_text(vm, receiver(args), &text)
    }),
    method("print", "(F)V", ACC_PUBLIC, |vm, _, args| {
        let text = format::float_to_string(crate::float_arg(args, 1));
        print_text(vm, receiver(args), &text)
    }),
    method("print", "(D)V", ACC_PUBLIC, |vm, _, args| {
        let text = format::double_to_string(crate::double_arg(args, 1));
        print_text(vm, receiver(args), &text)
    }),
    method("print", "([C)V", ACC_PUBLIC, print_chars),
    method(
        "print",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 1));
            print_text(vm, receiver(args), &text)
        },
    ),
    method(
        "print",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm.display_value(
                &oxjvm_classfile::FieldType::Object("java/lang/Object".into()),
                args[1],
            )?;
            print_text(vm, receiver(args), &text)
        },
    ),
    method("println", "()V", ACC_PUBLIC, |vm, _, args| {
        print_text(vm, receiver(args), "\n")
    }),
    method("println", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        let text = alloc::format!(
            "{}\n",
            if crate::bool_arg(args, 1) {
                "true"
            } else {
                "false"
            }
        );
        print_text(vm, receiver(args), &text)
    }),
    method("println", "(C)V", ACC_PUBLIC, println_char),
    method("println", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let text = format::int_to_string(int_arg(args, 1));
        print_line(vm, receiver(args), &text)
    }),
    method("println", "(J)V", ACC_PUBLIC, |vm, _, args| {
        let text = format::long_to_string(crate::long_arg(args, 1));
        print_line(vm, receiver(args), &text)
    }),
    method("println", "(F)V", ACC_PUBLIC, |vm, _, args| {
        let text = format::float_to_string(crate::float_arg(args, 1));
        print_line(vm, receiver(args), &text)
    }),
    method("println", "(D)V", ACC_PUBLIC, |vm, _, args| {
        let text = format::double_to_string(crate::double_arg(args, 1));
        print_line(vm, receiver(args), &text)
    }),
    method(
        "println",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 1));
            print_line(vm, receiver(args), &text)
        },
    ),
    method(
        "println",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm.display_value(
                &oxjvm_classfile::FieldType::Object("java/lang/Object".into()),
                args[1],
            )?;
            print_line(vm, receiver(args), &text)
        },
    ),
    method("flush", "()V", ACC_PUBLIC, |vm, _, args| {
        vm.flush_host(stream_of(vm, receiver(args)));
        void()
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        vm.flush_host(stream_of(vm, receiver(args)));
        void()
    }),
];

fn print_stream_init(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let class = vm.class_of(this);
    if let Ok((declaring, field)) = vm.find_field(class, "out", "Ljava/io/OutputStream;") {
        vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
    }
    let (declaring, field) = vm.find_field(class, "kind", "I")?;
    vm.set_instance_int(this, declaring, field, 1);
    void()
}

fn print_stream_init_kind(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "kind", "I")?;
    vm.set_instance_int(this, declaring, field, int_arg(args, 1));
    void()
}

fn stream_of(vm: &Vm<'_>, print_stream: ObjectRef) -> oxjvm_platform::Stream {
    let kind = crate::lang::read_int_field(vm, print_stream, "kind");
    if kind == 2 {
        oxjvm_platform::Stream::Stderr
    } else {
        oxjvm_platform::Stream::Stdout
    }
}

/// Write a value to a print stream, flushing is left to the host.
pub(crate) fn print_text(
    vm: &mut Vm<'_>,
    print_stream: ObjectRef,
    text: &str,
) -> Result<Value, VmError> {
    if print_stream.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let stream = stream_of(vm, print_stream);
    vm.write_host(stream, text.as_bytes());
    void()
}

fn print_line(vm: &mut Vm<'_>, print_stream: ObjectRef, text: &str) -> Result<Value, VmError> {
    if print_stream.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let stream = stream_of(vm, print_stream);
    vm.write_host(stream, text.as_bytes());
    vm.write_host(stream, b"\n");
    void()
}

fn print_chars(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let array = ref_arg(args, 1);
    let length = vm.array_length(array)?;
    let mut text = String::new();
    for index in 0..length {
        let unit = vm.array_get(array, index as i32)?.as_int() as u32 & 0xFFFF;
        if let Some(ch) = char::from_u32(unit) {
            text.push(ch);
        }
    }
    print_text(vm, receiver(args), &text)
}

fn println_char(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let unit = int_arg(args, 1) as u32 & 0xFFFF;
    let text = char::from_u32(unit).map_or_else(String::new, |ch| ch.to_string());
    print_line(vm, receiver(args), &text)
}

pub(crate) const PRINT_STREAM: oxjvm_vm::NativeClass = class(
    "java/io/PrintStream",
    Some("java/io/FilterOutputStream"),
    &[
        "java/io/Closeable",
        "java/io/Flushable",
        "java/lang/Appendable",
    ],
    ACC_PUBLIC,
    &[
        field("out", "Ljava/io/OutputStream;", ACC_PROTECTED, None),
        field("kind", "I", ACC_PRIVATE | ACC_SYNTHETIC, None),
    ],
    &PRINT_STREAM_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// InputStream / ByteArray streams
// -------------------------------------------------------------------------------------------

const INPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "read",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method("read", "([B)I", ACC_PUBLIC, |vm, _, args| {
        let (declaring, method) = vm.resolve_virtual_method(receiver(args), "read", "([BII)I")?;
        vm.invoke_method(
            declaring,
            method,
            vec![
                Value::Ref(receiver(args)),
                args[1],
                Value::Int(0),
                Value::Int(vm.array_length(args[1].as_ref())? as i32),
            ],
        )
    }),
    method("read", "([BII)I", ACC_PUBLIC, |vm, _, args| {
        let array = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        if length == 0 {
            return int(0);
        }
        match crate::lang::read_input_byte(vm, receiver(args))? {
            None => int(-1),
            Some(byte) => {
                vm.array_set(array, offset, Value::Int(byte), false)?;
                int(1)
            }
        }
    }),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
];

pub(crate) const INPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/io/InputStream",
    Some("java/lang/Object"),
    &["java/io/Closeable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &INPUT_STREAM_METHODS,
    None,
);

const BYTE_ARRAY_INPUT_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method("<init>", "([B)V", ACC_PUBLIC, byte_array_input_init),
    method(
        "read",
        "()I",
        ACC_PUBLIC,
        |vm, _, args| match crate::lang::read_input_byte(vm, receiver(args))? {
            None => int(-1),
            Some(byte) => int(byte),
        },
    ),
    method("available", "()I", ACC_PUBLIC, |vm, _, args| {
        let count = vm.array_length(ref_arg(args, 1)).unwrap_or(0);
        let _ = count;
        int(0)
    }),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("mark", "(I)V", ACC_PUBLIC, |_, _, _| void()),
    method("reset", "()V", ACC_PUBLIC, |_, _, _| void()),
];

fn byte_array_input_init(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "buf", "[B")?;
    vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
    let (declaring, field) = vm.find_field(class, "pos", "I")?;
    vm.set_instance_int(this, declaring, field, 0);
    void()
}

pub(crate) const BYTE_ARRAY_INPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/io/ByteArrayInputStream",
    Some("java/io/InputStream"),
    &[],
    ACC_PUBLIC,
    &[
        field("buf", "[B", ACC_PROTECTED, None),
        field("pos", "I", ACC_PROTECTED, None),
    ],
    &BYTE_ARRAY_INPUT_METHODS,
    None,
);

const BYTE_ARRAY_OUTPUT_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method("<init>", "()V", ACC_PUBLIC, byte_array_output_init),
    method("write", "(I)V", ACC_PUBLIC, byte_array_output_write),
    method("write", "([BII)V", ACC_PUBLIC, |vm, ctx, args| {
        let array = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        for index in 0..length {
            let byte = vm.array_get(array, offset + index)?.as_int();
            byte_array_output_write(vm, ctx, &[args[0], Value::Int(byte)])?;
        }
        void()
    }),
    method("flush", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("toByteArray", "()[B", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let count = crate::lang::read_int_field(vm, this, "count");
        let buffer = vm
            .read_ref_field(this, vm.class_of(this), "buf", "[B")
            .unwrap_or(ObjectRef::NULL);
        let mut bytes = Vec::with_capacity(count.max(0) as usize);
        for index in 0..count {
            bytes.push(vm.array_get(buffer, index)?.as_int() as i8);
        }
        let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, bytes.len())?;
        for (index, byte) in bytes.into_iter().enumerate() {
            vm.array_set(array, index as i32, Value::Int(i32::from(byte)), false)?;
        }
        object(array)
    }),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "count"))
    }),
];

fn byte_array_output_init(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let class = vm.class_of(this);
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, 32)?;
    let (declaring, field) = vm.find_field(class, "buf", "[B")?;
    vm.set_instance_ref(this, declaring, field, array);
    let (declaring, field) = vm.find_field(class, "count", "I")?;
    vm.set_instance_int(this, declaring, field, 0);
    void()
}

fn byte_array_output_write(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let byte = int_arg(args, 1);
    let count = crate::lang::read_int_field(vm, this, "count");
    let mut buffer = vm
        .read_ref_field(this, vm.class_of(this), "buf", "[B")
        .unwrap_or(ObjectRef::NULL);
    if vm.array_length(buffer)? as i32 <= count {
        let class = vm.class_of(this);
        let (declaring, field) = vm.find_field(class, "buf", "[B")?;
        let resized =
            vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, (count as usize + 1) * 2)?;
        for index in 0..count {
            let value = vm.array_get(buffer, index)?;
            vm.array_set(resized, index, value, false)?;
        }
        vm.set_instance_ref(this, declaring, field, resized);
        buffer = resized;
    }
    vm.array_set(buffer, count, Value::Int(byte as i8 as i32), false)?;
    crate::lang::write_int_field(vm, this, "count", count + 1);
    void()
}

pub(crate) const BYTE_ARRAY_OUTPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/io/ByteArrayOutputStream",
    Some("java/io/OutputStream"),
    &[],
    ACC_PUBLIC,
    &[
        field("buf", "[B", ACC_PROTECTED, None),
        field("count", "I", ACC_PROTECTED, None),
    ],
    &BYTE_ARRAY_OUTPUT_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Reader / Writer / PrintWriter (minimal)
// -------------------------------------------------------------------------------------------

const READER_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "read",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
];

pub(crate) const READER: oxjvm_vm::NativeClass = class(
    "java/io/Reader",
    Some("java/lang/Object"),
    &["java/io/Closeable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &READER_METHODS,
    None,
);

const WRITER_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method(
        "write",
        "(I)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method("flush", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
];

pub(crate) const WRITER: oxjvm_vm::NativeClass = class(
    "java/io/Writer",
    Some("java/lang/Object"),
    &[
        "java/io/Closeable",
        "java/io/Flushable",
        "java/lang/Appendable",
    ],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &WRITER_METHODS,
    None,
);

const PRINT_WRITER_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "<init>",
        "(Ljava/io/Writer;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "out", "Ljava/io/Writer;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method(
        "print",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 1));
            print_writer_text(vm, receiver(args), &text)
        },
    ),
    method(
        "println",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 1));
            print_writer_text(vm, receiver(args), &alloc::format!("{text}\n"))
        },
    ),
    method("println", "()V", ACC_PUBLIC, |vm, _, args| {
        print_writer_text(vm, receiver(args), "\n")
    }),
    method("flush", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
];

fn print_writer_text(vm: &mut Vm<'_>, this: ObjectRef, text: &str) -> Result<Value, VmError> {
    let out = vm
        .read_ref_field(this, vm.class_of(this), "out", "Ljava/io/Writer;")
        .unwrap_or(ObjectRef::NULL);
    if out.is_null() {
        return void();
    }
    let (declaring, method) = vm.resolve_virtual_method(out, "write", "(Ljava/lang/String;)V")?;
    let _ = (declaring, method);
    // Fall back to character-by-character writes through the abstract `write(int)`.
    for ch in text.chars() {
        let (declaring, method) = vm.resolve_virtual_method(out, "write", "(I)V")?;
        vm.invoke_method(
            declaring,
            method,
            vec![Value::Ref(out), Value::Int(ch as i32)],
        )?;
    }
    void()
}

pub(crate) const PRINT_WRITER: oxjvm_vm::NativeClass = class(
    "java/io/PrintWriter",
    Some("java/io/Writer"),
    &[],
    ACC_PUBLIC,
    &[field("out", "Ljava/io/Writer;", ACC_PROTECTED, None)],
    &PRINT_WRITER_METHODS,
    None,
);
