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
            if let Some((declaring, field)) =
                vm.find_field_opt(class, "out", "Ljava/io/OutputStream;")
            {
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

const PRINT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 30] = [
    method(
        "<init>",
        "(Ljava/io/OutputStream;)V",
        ACC_PUBLIC,
        print_stream_init,
    ),
    method("<init>", "(I)V", ACC_PUBLIC, print_stream_init_kind),
    method("write", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let byte = (int_arg(args, 1) & 0xFF) as u8;
        print_stream_write(vm, receiver(args), &[byte])
    }),
    method(
        "<init>",
        "(Ljava/io/OutputStream;Z)V",
        ACC_PUBLIC,
        print_stream_init,
    ),
    method(
        "<init>",
        "(Ljava/io/OutputStream;ZLjava/lang/String;)V",
        ACC_PUBLIC,
        print_stream_init,
    ),
    method(
        "<init>",
        "(Ljava/io/File;)V",
        ACC_PUBLIC,
        print_stream_init_file,
    ),
    method(
        "<init>",
        "(Ljava/io/File;Ljava/lang/String;)V",
        ACC_PUBLIC,
        print_stream_init_file,
    ),
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        print_stream_init_string,
    ),
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
    method(
        "printf",
        "(Ljava/lang/String;[Ljava/lang/Object;)Ljava/io/PrintStream;",
        ACC_PUBLIC,
        print_formatted,
    ),
    method(
        "format",
        "(Ljava/lang/String;[Ljava/lang/Object;)Ljava/io/PrintStream;",
        ACC_PUBLIC,
        print_formatted,
    ),
];

fn print_stream_init(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    print_stream_out(vm, receiver(args), ref_arg(args, 1))
}

/// Point a print stream at an `OutputStream`; the kind stays 0 so writes reach `out`.
fn print_stream_out(vm: &mut Vm<'_>, this: ObjectRef, stream: ObjectRef) -> Result<Value, VmError> {
    let class = vm.class_of(this);
    if let Some((declaring, field)) = vm.find_field_opt(class, "out", "Ljava/io/OutputStream;") {
        vm.set_instance_ref(this, declaring, field, stream);
    }
    let (declaring, field) = vm.find_field(class, "kind", "I")?;
    vm.set_instance_int(this, declaring, field, 0);
    void()
}

/// `PrintStream(File)` / `PrintStream(File, String)`: a file output stream for the path.
fn print_stream_init_file(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let path = crate::file::file_path(vm, ref_arg(args, 1));
    let stream = new_file_output_stream(vm, &path)?;
    print_stream_out(vm, receiver(args), stream)
}

/// `PrintStream(String)`: a file output stream for the path.
fn print_stream_init_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let path = crate::string_arg(vm, args[1])?;
    let stream = new_file_output_stream(vm, &path)?;
    print_stream_out(vm, receiver(args), stream)
}

fn new_file_output_stream(vm: &mut Vm<'_>, path: &str) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/io/FileOutputStream")?;
    let instance = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(class, "<init>", "(Ljava/lang/String;)V")
        .ok_or_else(|| VmError::internal("FileOutputStream lacks its constructor"))?;
    let text = vm.make_string(path)?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![Value::Ref(instance), Value::Ref(text)],
    )?;
    Ok(instance)
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

/// Write raw bytes to a print stream: through its `OutputStream` when it has one, to the
/// host's stdout/stderr for the `System.out`/`System.err` streams otherwise.
fn print_stream_write(vm: &mut Vm<'_>, this: ObjectRef, bytes: &[u8]) -> Result<Value, VmError> {
    if this.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let class = vm.class_of(this);
    let out = vm
        .read_ref_field(this, class, "out", "Ljava/io/OutputStream;")
        .unwrap_or(ObjectRef::NULL);
    if !out.is_null() {
        let array = write_bytes_array(vm, bytes)?;
        let (declaring, method) = vm.resolve_virtual_method(out, "write", "([B)V")?;
        vm.invoke_method(declaring, method, vec![Value::Ref(out), Value::Ref(array)])?;
        return void();
    }
    let stream = stream_of(vm, this);
    vm.write_host(stream, bytes);
    void()
}

fn write_bytes_array(vm: &mut Vm<'_>, bytes: &[u8]) -> Result<ObjectRef, VmError> {
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

/// Write a value to a print stream, flushing is left to the host.
pub(crate) fn print_text(
    vm: &mut Vm<'_>,
    print_stream: ObjectRef,
    text: &str,
) -> Result<Value, VmError> {
    print_stream_write(vm, print_stream, text.as_bytes())
}

/// `PrintStream.printf`/`format`: the formatted string, without a newline.
fn print_formatted(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let stream = receiver(args);
    let format = crate::string_arg(vm, args[1])?;
    let arguments = object_array_elements(vm, ref_arg(args, 2));
    let text = crate::lang::format_string(vm, &format, &arguments)?;
    print_text(vm, stream, &text)?;
    object(stream)
}

fn object_array_elements(vm: &Vm<'_>, array: ObjectRef) -> Vec<ObjectRef> {
    if array.is_null() {
        return Vec::new();
    }
    match vm.heap.get(array).map(|object| &object.data) {
        Some(oxjvm_vm::ObjectData::Array(oxjvm_vm::ArrayData::Reference(elements))) => {
            elements.clone()
        }
        _ => Vec::new(),
    }
}

fn print_line(vm: &mut Vm<'_>, print_stream: ObjectRef, text: &str) -> Result<Value, VmError> {
    print_stream_write(vm, print_stream, text.as_bytes())?;
    print_stream_write(vm, print_stream, b"\n")
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

const INPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "read",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method("readAllBytes", "()[B", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let mut bytes = Vec::new();
        loop {
            let (declaring, method) = vm.resolve_virtual_method(this, "read", "()I")?;
            let value = vm
                .invoke_method(declaring, method, vec![Value::Ref(this)])?
                .as_int();
            if value < 0 {
                break;
            }
            bytes.push(value as u8);
        }
        object(write_bytes_array(vm, &bytes)?)
    }),
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

const BYTE_ARRAY_OUTPUT_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method("<init>", "()V", ACC_PUBLIC, byte_array_output_init),
    method("<init>", "(I)V", ACC_PUBLIC, byte_array_output_init),
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
    method("reset", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let (declaring, field) = vm.find_field(vm.class_of(this), "count", "I")?;
        vm.set_instance_int(this, declaring, field, 0);
        void()
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let bytes = byte_array_output_bytes(vm, receiver(args))?;
            object(vm.make_string(&String::from_utf8_lossy(&bytes))?)
        },
    ),
    method(
        "toString",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let bytes = byte_array_output_bytes(vm, receiver(args))?;
            object(vm.make_string(&String::from_utf8_lossy(&bytes))?)
        },
    ),
    method(
        "writeTo",
        "(Ljava/io/OutputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let bytes = byte_array_output_bytes(vm, receiver(args))?;
            let stream = ref_arg(args, 1);
            if stream.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let array = write_bytes_array(vm, &bytes)?;
            let (declaring, method) = vm.resolve_virtual_method(stream, "write", "([B)V")?;
            vm.invoke_method(
                declaring,
                method,
                vec![Value::Ref(stream), Value::Ref(array)],
            )?;
            void()
        },
    ),
];

/// The bytes a `ByteArrayOutputStream` has collected so far.
fn byte_array_output_bytes(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Vec<u8>, VmError> {
    let count = crate::lang::read_int_field(vm, this, "count").max(0);
    let buffer = vm
        .read_ref_field(this, vm.class_of(this), "buf", "[B")
        .unwrap_or(ObjectRef::NULL);
    let mut bytes = Vec::with_capacity(count as usize);
    for index in 0..count {
        bytes.push(vm.array_get(buffer, index)?.as_int() as u8);
    }
    Ok(bytes)
}

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

const WRITER_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method(
        "write",
        "(I)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "write",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        writer_write_string,
    ),
    method(
        "write",
        "(Ljava/lang/String;II)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = ref_arg(args, 1);
            if text.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let (class, method) =
                vm.resolve_virtual_method(text, "substring", "(II)Ljava/lang/String;")?;
            let slice = vm.invoke_method(
                class,
                method,
                vec![
                    Value::Ref(text),
                    Value::Int(int_arg(args, 2)),
                    Value::Int(int_arg(args, 2) + int_arg(args, 3)),
                ],
            )?;
            let this = receiver(args);
            let (declaring, method) =
                vm.resolve_virtual_method(this, "write", "(Ljava/lang/String;)V")?;
            vm.invoke_method(declaring, method, vec![Value::Ref(this), slice])
        },
    ),
    method("flush", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "append",
        "(Ljava/lang/CharSequence;)Ljava/io/Writer;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "append",
        "(Ljava/lang/CharSequence;II)Ljava/io/Writer;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "append",
        "(C)Ljava/io/Writer;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

/// `Writer.write(String)`: the units of the string through the abstract `write(int)`.
fn writer_write_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = ref_arg(args, 1);
    if text.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let value = vm.string_value(text).unwrap_or_default();
    let this = receiver(args);
    for unit in value.encode_utf16() {
        let (declaring, method) = vm.resolve_virtual_method(this, "write", "(I)V")?;
        vm.invoke_method(
            declaring,
            method,
            vec![Value::Ref(this), Value::Int(i32::from(unit))],
        )?;
    }
    void()
}

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

const PRINT_WRITER_METHODS: [oxjvm_vm::NativeMethodDef; 33] = [
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
        "<init>",
        "(Ljava/io/Writer;Z)V",
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
        "write",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 1));
            print_writer_text(vm, receiver(args), &text)
        },
    ),
    method(
        "write",
        "(Ljava/lang/String;II)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = ref_arg(args, 1);
            if text.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let (class, method) =
                vm.resolve_virtual_method(text, "substring", "(II)Ljava/lang/String;")?;
            let slice = vm.invoke_method(
                class,
                method,
                vec![
                    Value::Ref(text),
                    Value::Int(int_arg(args, 2)),
                    Value::Int(int_arg(args, 2) + int_arg(args, 3)),
                ],
            )?;
            let value = vm.string_or_empty(slice.as_ref());
            print_writer_text(vm, receiver(args), &value)
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
    method("write", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let text =
            char::from_u32(int_arg(args, 1) as u32).map_or_else(String::new, |ch| ch.to_string());
        print_writer_text(vm, receiver(args), &text)
    }),
    method("write", "([C)V", ACC_PUBLIC, |vm, _, args| {
        let text = print_writer_char_array(vm, ref_arg(args, 1), 0, -1)?;
        print_writer_text(vm, receiver(args), &text)
    }),
    method("write", "([CII)V", ACC_PUBLIC, |vm, _, args| {
        let text =
            print_writer_char_array(vm, ref_arg(args, 1), int_arg(args, 2), int_arg(args, 3))?;
        print_writer_text(vm, receiver(args), &text)
    }),
    method("print", "(C)V", ACC_PUBLIC, |vm, _, args| {
        let text =
            char::from_u32(int_arg(args, 1) as u32).map_or_else(String::new, |ch| ch.to_string());
        print_writer_text(vm, receiver(args), &text)
    }),
    method("print", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        let text = if crate::bool_arg(args, 1) {
            "true"
        } else {
            "false"
        };
        print_writer_text(vm, receiver(args), text)
    }),
    method("print", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let text = format::int_to_string(int_arg(args, 1));
        print_writer_text(vm, receiver(args), &text)
    }),
    method("print", "(J)V", ACC_PUBLIC, |vm, _, args| {
        let text = format::long_to_string(crate::long_arg(args, 1));
        print_writer_text(vm, receiver(args), &text)
    }),
    method("print", "(D)V", ACC_PUBLIC, |vm, _, args| {
        let text = format::double_to_string(crate::double_arg(args, 1));
        print_writer_text(vm, receiver(args), &text)
    }),
    method(
        "print",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm.display_value(
                &oxjvm_classfile::FieldType::Object("java/lang/Object".into()),
                args[1],
            )?;
            print_writer_text(vm, receiver(args), &text)
        },
    ),
    method("println", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        let text = alloc::format!(
            "{}\n",
            if crate::bool_arg(args, 1) {
                "true"
            } else {
                "false"
            }
        );
        print_writer_text(vm, receiver(args), &text)
    }),
    method("println", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let text = alloc::format!("{}\n", format::int_to_string(int_arg(args, 1)));
        print_writer_text(vm, receiver(args), &text)
    }),
    method(
        "println",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm.display_value(
                &oxjvm_classfile::FieldType::Object("java/lang/Object".into()),
                args[1],
            )?;
            print_writer_text(vm, receiver(args), &alloc::format!("{text}\n"))
        },
    ),
    method("checkError", "()Z", ACC_PUBLIC, |_, _, _| {
        crate::boolean(false)
    }),
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
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        print_writer_init_string,
    ),
    method(
        "<init>",
        "(Ljava/io/File;)V",
        ACC_PUBLIC,
        print_writer_init_file,
    ),
    method(
        "<init>",
        "(Ljava/io/File;Ljava/lang/String;)V",
        ACC_PUBLIC,
        print_writer_init_file,
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        print_writer_init_string,
    ),
    method(
        "<init>",
        "(Ljava/io/OutputStream;)V",
        ACC_PUBLIC,
        print_writer_init_stream,
    ),
    method(
        "<init>",
        "(Ljava/io/OutputStream;Z)V",
        ACC_PUBLIC,
        print_writer_init_stream,
    ),
    method("flush", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "printf",
        "(Ljava/lang/String;[Ljava/lang/Object;)Ljava/io/PrintWriter;",
        ACC_PUBLIC,
        print_writer_formatted,
    ),
    method(
        "format",
        "(Ljava/lang/String;[Ljava/lang/Object;)Ljava/io/PrintWriter;",
        ACC_PUBLIC,
        print_writer_formatted,
    ),
    method(
        "append",
        "(Ljava/lang/CharSequence;)Ljava/io/PrintWriter;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = vm.string_or_empty(ref_arg(args, 1));
            print_writer_text(vm, this, &text)?;
            object(this)
        },
    ),
    method(
        "append",
        "(Ljava/lang/CharSequence;II)Ljava/io/PrintWriter;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let sequence = ref_arg(args, 1);
            let start = int_arg(args, 2).max(0) as usize;
            let end = int_arg(args, 3).max(0) as usize;
            let mut text = String::new();
            if !sequence.is_null() {
                if let Some(units) = vm.string_utf16(sequence) {
                    let units = units.to_vec();
                    for unit in units.iter().take(end).skip(start) {
                        if let Some(ch) = char::from_u32(u32::from(*unit)) {
                            text.push(ch);
                        }
                    }
                }
            }
            print_writer_text(vm, this, &text)?;
            object(this)
        },
    ),
    method(
        "append",
        "(C)Ljava/io/PrintWriter;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let ch = char::from_u32(int_arg(args, 1) as u32 & 0xFFFF).unwrap_or('\u{FFFD}');
            let mut text = String::new();
            text.push(ch);
            print_writer_text(vm, this, &text)?;
            object(this)
        },
    ),
];

/// `PrintWriter.printf`/`format`.
fn print_writer_formatted(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let writer = receiver(args);
    let format = crate::string_arg(vm, args[1])?;
    let arguments = object_array_elements(vm, ref_arg(args, 2));
    let text = crate::lang::format_string(vm, &format, &arguments)?;
    print_writer_text(vm, writer, &text)?;
    object(writer)
}

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

/// The characters of a `char[]` slice (the whole array when `length` is negative).
fn print_writer_char_array(
    vm: &mut Vm<'_>,
    array: ObjectRef,
    offset: i32,
    length: i32,
) -> Result<String, VmError> {
    if array.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let total = vm.array_length(array)?;
    let start = offset.max(0) as usize;
    let count = if length < 0 {
        total.saturating_sub(start)
    } else {
        (length as usize).min(total.saturating_sub(start))
    };
    let mut text = String::new();
    for index in 0..count {
        let unit = vm.array_get(array, (start + index) as i32)?.as_int() as u32 & 0xFFFF;
        if let Some(ch) = char::from_u32(unit) {
            text.push(ch);
        }
    }
    Ok(text)
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

// -------------------------------------------------------------------------------------------
// StringWriter
// -------------------------------------------------------------------------------------------

const STRING_WRITER_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method("<init>", "()V", ACC_PUBLIC, string_writer_init),
    method("<init>", "(I)V", ACC_PUBLIC, string_writer_init),
    method(
        "write",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = ref_arg(args, 1);
            if text.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            string_writer_append(vm, receiver(args), text)
        },
    ),
    method(
        "write",
        "(Ljava/lang/String;II)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = ref_arg(args, 1);
            if text.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let (class, method) =
                vm.resolve_virtual_method(text, "substring", "(II)Ljava/lang/String;")?;
            let slice = vm.invoke_method(
                class,
                method,
                vec![
                    Value::Ref(text),
                    Value::Int(int_arg(args, 2)),
                    Value::Int(int_arg(args, 3)),
                ],
            )?;
            string_writer_append(vm, receiver(args), slice.as_ref())
        },
    ),
    method("write", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let ch = int_arg(args, 1) as u16;
        let text = vm.make_string_utf16(vec![ch])?;
        string_writer_append(vm, receiver(args), text)
    }),
    method(
        "append",
        "(Ljava/lang/CharSequence;)Ljava/io/Writer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let sequence = ref_arg(args, 1);
            if sequence.is_null() {
                let text = vm.make_string("null")?;
                string_writer_append(vm, receiver(args), text)?;
                return object(receiver(args));
            }
            let (class, method) =
                vm.resolve_virtual_method(sequence, "toString", "()Ljava/lang/String;")?;
            let text = vm.invoke_method(class, method, vec![Value::Ref(sequence)])?;
            string_writer_append(vm, receiver(args), text.as_ref())?;
            object(receiver(args))
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let buffer = string_writer_buffer(vm, receiver(args));
            if buffer.is_null() {
                return object(ObjectRef::NULL);
            }
            let (class, method) =
                vm.resolve_virtual_method(buffer, "toString", "()Ljava/lang/String;")?;
            vm.invoke_method(class, method, vec![Value::Ref(buffer)])
        },
    ),
    method("flush", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "getBuffer",
        "()Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        |vm, _, args| object(string_writer_buffer(vm, receiver(args))),
    ),
];

pub(crate) const STRING_WRITER: oxjvm_vm::NativeClass = class(
    "java/io/StringWriter",
    Some("java/io/Writer"),
    &[],
    ACC_PUBLIC,
    &[field("buf", "Ljava/lang/StringBuffer;", ACC_PRIVATE, None)],
    &STRING_WRITER_METHODS,
    None,
);

fn string_writer_init(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let buffer_class = vm.resolve_class("java/lang/StringBuffer")?;
    let buffer = vm.new_instance(buffer_class)?;
    if let Some((declaring, ctor)) = vm.find_method(buffer_class, "<init>", "()V") {
        vm.invoke_method(declaring, ctor, vec![Value::Ref(buffer)])?;
    }
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "buf", "Ljava/lang/StringBuffer;")?;
    vm.set_instance_ref(this, declaring, field, buffer);
    void()
}

fn string_writer_buffer(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    let class = vm.class_of(this);
    vm.read_ref_field(this, class, "buf", "Ljava/lang/StringBuffer;")
        .unwrap_or(ObjectRef::NULL)
}

fn string_writer_append(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    text: ObjectRef,
) -> Result<Value, VmError> {
    let buffer = string_writer_buffer(vm, this);
    if buffer.is_null() {
        return Err(VmError::internal("StringWriter without a buffer"));
    }
    let (class, method) = vm.resolve_virtual_method(
        buffer,
        "append",
        "(Ljava/lang/String;)Ljava/lang/StringBuffer;",
    )?;
    vm.invoke_method(class, method, vec![Value::Ref(buffer), Value::Ref(text)])?;
    void()
}

// -------------------------------------------------------------------------------------------
// FileWriter / PrintWriter(String|File)
// -------------------------------------------------------------------------------------------

/// Write `text` to the writer's file (append mode after the first write).
fn file_writer_write_text(vm: &mut Vm<'_>, this: ObjectRef, text: &str) -> Result<Value, VmError> {
    let class = vm.class_of(this);
    let path = vm
        .read_string_field(this, class, "path")
        .unwrap_or_default();
    let append = crate::lang::read_int_field(vm, this, "append") != 0;
    let written = vm
        .host
        .filesystem()
        .map(|fs| fs.write(&path, text.as_bytes(), append))
        .unwrap_or(false);
    if !written {
        return Err(vm.throw_new("java/io/IOException", Some(&path)));
    }
    crate::lang::write_int_field(vm, this, "append", 1);
    void()
}

const FILE_WRITER_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let path = crate::string_arg(vm, args[1])?;
            vm.set_string_field(this, vm.class_of(this), "path", &path)?;
            crate::lang::write_int_field(vm, this, "append", 0);
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Z)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let path = crate::string_arg(vm, args[1])?;
            vm.set_string_field(this, vm.class_of(this), "path", &path)?;
            crate::lang::write_int_field(vm, this, "append", i32::from(crate::bool_arg(args, 2)));
            void()
        },
    ),
    method("<init>", "(Ljava/io/File;)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let file = ref_arg(args, 1);
        let path = vm
            .read_string_field(file, vm.class_of(file), "path")
            .unwrap_or_default();
        vm.set_string_field(this, vm.class_of(this), "path", &path)?;
        crate::lang::write_int_field(vm, this, "append", 0);
        void()
    }),
    method("<init>", "(Ljava/io/File;Z)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let file = ref_arg(args, 1);
        let path = vm
            .read_string_field(file, vm.class_of(file), "path")
            .unwrap_or_default();
        vm.set_string_field(this, vm.class_of(this), "path", &path)?;
        crate::lang::write_int_field(vm, this, "append", i32::from(crate::bool_arg(args, 2)));
        void()
    }),
    method("write", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let text =
            char::from_u32(int_arg(args, 1) as u32 & 0xFFFF).map_or(String::new(), String::from);
        file_writer_write_text(vm, receiver(args), &text)
    }),
    method(
        "write",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 1));
            file_writer_write_text(vm, receiver(args), &text)
        },
    ),
    method("write", "([CII)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let array = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        let mut units = Vec::new();
        for index in 0..length {
            units.push(vm.array_get(array, offset + index)?.as_int() as u16);
        }
        let text = String::from_utf16_lossy(&units);
        file_writer_write_text(vm, this, &text)
    }),
    method("flush", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "append",
        "(Ljava/lang/CharSequence;)Ljava/io/Writer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = vm.string_or_empty(ref_arg(args, 1));
            file_writer_write_text(vm, this, &text)?;
            object(this)
        },
    ),
    method(
        "append",
        "(Ljava/lang/CharSequence;II)Ljava/io/Writer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let sequence = vm.string_or_empty(ref_arg(args, 1));
            let start = int_arg(args, 2).max(0) as usize;
            let end = int_arg(args, 3).max(0) as usize;
            let units: Vec<u16> = sequence.encode_utf16().collect();
            let text =
                String::from_utf16_lossy(&units[start.min(units.len())..end.min(units.len())]);
            file_writer_write_text(vm, this, &text)?;
            object(this)
        },
    ),
    method(
        "append",
        "(C)Ljava/io/Writer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let ch = char::from_u32(int_arg(args, 1) as u32 & 0xFFFF).unwrap_or('\u{FFFD}');
            let mut text = String::new();
            text.push(ch);
            file_writer_write_text(vm, this, &text)?;
            object(this)
        },
    ),
];

pub(crate) const FILE_WRITER: oxjvm_vm::NativeClass = class(
    "java/io/FileWriter",
    Some("java/io/Writer"),
    &[],
    ACC_PUBLIC,
    &[
        field("path", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("append", "I", ACC_PRIVATE, None),
    ],
    &FILE_WRITER_METHODS,
    None,
);

/// Wrap a fresh `FileWriter` around the `PrintWriter.out` field.
fn print_writer_wrap_file_writer(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    ctor: &str,
    argument: ObjectRef,
) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/io/FileWriter")?;
    let writer = vm.new_instance(class)?;
    let (declaring, method) = vm
        .find_method(class, "<init>", ctor)
        .ok_or_else(|| VmError::internal("FileWriter constructor missing"))?;
    vm.invoke_method(
        declaring,
        method,
        vec![Value::Ref(writer), Value::Ref(argument)],
    )?;
    let print_writer_class = vm.class_of(this);
    let (declaring, field) = vm.find_field(print_writer_class, "out", "Ljava/io/Writer;")?;
    vm.set_instance_ref(this, declaring, field, writer);
    void()
}

fn print_writer_init_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let path = crate::string_arg(vm, args[1])?;
    let text = vm.make_string(&path)?;
    print_writer_wrap_file_writer(vm, this, "(Ljava/lang/String;)V", text)
}

fn print_writer_init_file(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    print_writer_wrap_file_writer(vm, this, "(Ljava/io/File;)V", ref_arg(args, 1))
}

// -------------------------------------------------------------------------------------------
// OutputStreamWriter
// -------------------------------------------------------------------------------------------

fn io_bytes_array(vm: &mut Vm<'_>, bytes: &[u8]) -> Result<ObjectRef, VmError> {
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, bytes.len())?;
    for (index, byte) in bytes.iter().enumerate() {
        vm.array_set(array, index as i32, Value::Int(i32::from(*byte)), false)?;
    }
    Ok(array)
}

fn output_stream_writer_out(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "out", "Ljava/io/OutputStream;")
        .unwrap_or(ObjectRef::NULL)
}

fn output_stream_writer_write_text(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    text: &str,
) -> Result<Value, VmError> {
    let out = output_stream_writer_out(vm, this);
    if out.is_null() {
        return void();
    }
    let array = io_bytes_array(vm, text.as_bytes())?;
    let (class, method) = vm.resolve_virtual_method(out, "write", "([B)V")?;
    vm.invoke_method(class, method, vec![Value::Ref(out), Value::Ref(array)])?;
    void()
}

const OUTPUT_STREAM_WRITER_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method(
        "<init>",
        "(Ljava/io/OutputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "out", "Ljava/io/OutputStream;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/OutputStream;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "out", "Ljava/io/OutputStream;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/OutputStream;Ljava/nio/charset/Charset;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "out", "Ljava/io/OutputStream;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method("write", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let unit = int_arg(args, 1) as u32 & 0xFFFF;
        let ch = char::from_u32(unit).unwrap_or('\u{FFFD}');
        let mut text = String::new();
        text.push(ch);
        output_stream_writer_write_text(vm, receiver(args), &text)
    }),
    method(
        "write",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 1));
            output_stream_writer_write_text(vm, receiver(args), &text)
        },
    ),
    method("write", "([CII)V", ACC_PUBLIC, |vm, _, args| {
        let array = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        let mut units = Vec::new();
        for index in 0..length {
            units.push(vm.array_get(array, offset + index)?.as_int() as u16);
        }
        let text = String::from_utf16_lossy(&units);
        output_stream_writer_write_text(vm, receiver(args), &text)
    }),
    method("flush", "()V", ACC_PUBLIC, |vm, _, args| {
        let out = output_stream_writer_out(vm, receiver(args));
        if !out.is_null() {
            let (class, method) = vm.resolve_virtual_method(out, "flush", "()V")?;
            vm.invoke_method(class, method, vec![Value::Ref(out)])?;
        }
        void()
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let out = output_stream_writer_out(vm, receiver(args));
        if !out.is_null() {
            let (class, method) = vm.resolve_virtual_method(out, "close", "()V")?;
            vm.invoke_method(class, method, vec![Value::Ref(out)])?;
        }
        void()
    }),
];

pub(crate) const OUTPUT_STREAM_WRITER: oxjvm_vm::NativeClass = class(
    "java/io/OutputStreamWriter",
    Some("java/io/Writer"),
    &[],
    ACC_PUBLIC,
    &[field("out", "Ljava/io/OutputStream;", ACC_PRIVATE, None)],
    &OUTPUT_STREAM_WRITER_METHODS,
    None,
);

/// `PrintWriter(OutputStream, boolean)`: wrap in an OutputStreamWriter.
fn print_writer_init_stream(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let class = vm.resolve_class("java/io/OutputStreamWriter")?;
    let writer = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(class, "<init>", "(Ljava/io/OutputStream;)V")
        .ok_or_else(|| VmError::internal("OutputStreamWriter lacks its constructor"))?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![Value::Ref(writer), Value::Ref(ref_arg(args, 1))],
    )?;
    let print_class = vm.class_of(this);
    let (declaring, field) = vm.find_field(print_class, "out", "Ljava/io/Writer;")?;
    vm.set_instance_ref(this, declaring, field, writer);
    void()
}

// -------------------------------------------------------------------------------------------
// FileReader
// -------------------------------------------------------------------------------------------

fn file_reader_path(vm: &Vm<'_>, this: ObjectRef) -> String {
    vm.read_string_field(this, vm.class_of(this), "path")
        .unwrap_or_default()
}

fn file_reader_content(vm: &Vm<'_>, this: ObjectRef) -> String {
    vm.read_string_field(this, vm.class_of(this), "content")
        .unwrap_or_default()
}

fn file_reader_init(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let this = receiver(args);
    let path = file_reader_path(vm, this);
    let bytes = vm
        .host
        .filesystem()
        .and_then(|fs| fs.read(&path))
        .unwrap_or_default();
    let content = String::from_utf8_lossy(&bytes).into_owned();
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "content", "Ljava/lang/String;")?;
    let value = vm.make_string(&content)?;
    vm.set_instance_ref(this, declaring, field, value);
    crate::lang::write_int_field(vm, this, "pos", 0);
    let _ = path;
    void()
}

const FILE_READER_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "path", "Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            file_reader_init(vm, args)
        },
    ),
    method("<init>", "(Ljava/io/File;)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let file = ref_arg(args, 1);
        let path = vm
            .read_string_field(file, vm.class_of(file), "path")
            .unwrap_or_default();
        let path_value = vm.make_string(&path)?;
        let (declaring, field) = vm.find_field(vm.class_of(this), "path", "Ljava/lang/String;")?;
        vm.set_instance_ref(this, declaring, field, path_value);
        file_reader_init(vm, args)
    }),
    method("read", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let content = file_reader_content(vm, this);
        let pos = crate::lang::read_int_field(vm, this, "pos");
        let units: Vec<u16> = content.encode_utf16().collect();
        if pos < 0 || pos as usize >= units.len() {
            return int(-1);
        }
        crate::lang::write_int_field(vm, this, "pos", pos + 1);
        int(i32::from(units[pos as usize]))
    }),
    method("read", "([CII)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let content = file_reader_content(vm, this);
        let units: Vec<u16> = content.encode_utf16().collect();
        let mut pos = crate::lang::read_int_field(vm, this, "pos");
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        let array = ref_arg(args, 1);
        let mut count = 0;
        while count < length && (pos as usize) < units.len() {
            vm.array_set(
                array,
                offset + count,
                Value::Int(i32::from(units[pos as usize])),
                false,
            )?;
            pos += 1;
            count += 1;
        }
        crate::lang::write_int_field(vm, this, "pos", pos);
        if count == 0 { int(-1) } else { int(count) }
    }),
    method("read", "([C)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let content = file_reader_content(vm, this);
        let units: Vec<u16> = content.encode_utf16().collect();
        let mut pos = crate::lang::read_int_field(vm, this, "pos");
        let array = ref_arg(args, 1);
        let length = vm.array_length(array)? as i32;
        let mut count = 0;
        while count < length && (pos as usize) < units.len() {
            vm.array_set(
                array,
                count,
                Value::Int(i32::from(units[pos as usize])),
                false,
            )?;
            pos += 1;
            count += 1;
        }
        crate::lang::write_int_field(vm, this, "pos", pos);
        if count == 0 { int(-1) } else { int(count) }
    }),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
];

pub(crate) const FILE_READER: oxjvm_vm::NativeClass = class(
    "java/io/FileReader",
    Some("java/io/Reader"),
    &[],
    ACC_PUBLIC,
    &[
        field("path", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("content", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("pos", "I", ACC_PRIVATE, None),
    ],
    &FILE_READER_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// BufferedReader
// -------------------------------------------------------------------------------------------

fn buffered_reader_in(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "in", "Ljava/io/Reader;")
        .unwrap_or(ObjectRef::NULL)
}

fn buffered_reader_read_char(vm: &mut Vm<'_>, this: ObjectRef) -> Result<i32, VmError> {
    let reader = buffered_reader_in(vm, this);
    if reader.is_null() {
        return Ok(-1);
    }
    let (class, method) = vm.resolve_virtual_method(reader, "read", "()I")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(reader)])?
        .as_int())
}

const BUFFERED_READER_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "<init>",
        "(Ljava/io/Reader;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) = vm.find_field(vm.class_of(this), "in", "Ljava/io/Reader;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/Reader;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) = vm.find_field(vm.class_of(this), "in", "Ljava/io/Reader;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method("read", "()I", ACC_PUBLIC, |vm, _, args| {
        int(buffered_reader_read_char(vm, receiver(args))?)
    }),
    method("read", "([CII)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let array = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        let mut count = 0;
        while count < length {
            let value = buffered_reader_read_char(vm, this)?;
            if value < 0 {
                break;
            }
            vm.array_set(array, offset + count, Value::Int(value), false)?;
            count += 1;
        }
        if count == 0 { int(-1) } else { int(count) }
    }),
    method(
        "readLine",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let mut line = String::new();
            let mut read_any = false;
            loop {
                let value = buffered_reader_read_char(vm, this)?;
                if value < 0 {
                    break;
                }
                read_any = true;
                if value == i32::from(b'\n') {
                    break;
                }
                if value == i32::from(b'\r') {
                    continue;
                }
                line.push(char::from_u32(value as u32).unwrap_or('\u{FFFD}'));
            }
            if !read_any {
                return object(ObjectRef::NULL);
            }
            object(vm.make_string(&line)?)
        },
    ),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let reader = buffered_reader_in(vm, receiver(args));
        if !reader.is_null() {
            let (class, method) = vm.resolve_virtual_method(reader, "close", "()V")?;
            vm.invoke_method(class, method, vec![Value::Ref(reader)])?;
        }
        void()
    }),
    method("ready", "()Z", ACC_PUBLIC, |_, _, _| Ok(Value::Int(1))),
];

pub(crate) const BUFFERED_READER: oxjvm_vm::NativeClass = class(
    "java/io/BufferedReader",
    Some("java/io/Reader"),
    &[],
    ACC_PUBLIC,
    &[field("in", "Ljava/io/Reader;", ACC_PRIVATE, None)],
    &BUFFERED_READER_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// BufferedWriter
// -------------------------------------------------------------------------------------------

fn buffered_writer_out(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "out", "Ljava/io/Writer;")
        .unwrap_or(ObjectRef::NULL)
}

fn buffered_writer_call(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: &str,
    descriptor: &str,
    arguments: Vec<Value>,
) -> Result<Value, VmError> {
    let writer = buffered_writer_out(vm, this);
    if writer.is_null() {
        return void();
    }
    let (class, method) = vm.resolve_virtual_method(writer, name, descriptor)?;
    let mut full = vec![Value::Ref(writer)];
    full.extend(arguments);
    vm.invoke_method(class, method, full)
}

const BUFFERED_WRITER_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
    method(
        "<init>",
        "(Ljava/io/Writer;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) = vm.find_field(vm.class_of(this), "out", "Ljava/io/Writer;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/Writer;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) = vm.find_field(vm.class_of(this), "out", "Ljava/io/Writer;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method("write", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        buffered_writer_call(
            vm,
            this,
            "write",
            "(I)V",
            vec![Value::Int(int_arg(args, 1))],
        )
    }),
    method(
        "write",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            buffered_writer_call(
                vm,
                this,
                "write",
                "(Ljava/lang/String;)V",
                vec![Value::Ref(ref_arg(args, 1))],
            )
        },
    ),
    method("write", "([CII)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        buffered_writer_call(
            vm,
            this,
            "write",
            "([CII)V",
            vec![
                Value::Ref(ref_arg(args, 1)),
                Value::Int(int_arg(args, 2)),
                Value::Int(int_arg(args, 3)),
            ],
        )
    }),
    method("newLine", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let text = vm.make_string("\n")?;
        buffered_writer_call(
            vm,
            this,
            "write",
            "(Ljava/lang/String;)V",
            vec![Value::Ref(text)],
        )
    }),
    method("flush", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        buffered_writer_call(vm, this, "flush", "()V", Vec::new())
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        buffered_writer_call(vm, this, "close", "()V", Vec::new())
    }),
    method(
        "append",
        "(Ljava/lang/CharSequence;)Ljava/io/Writer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = vm.string_or_empty(ref_arg(args, 1));
            let text = vm.make_string(&text)?;
            buffered_writer_call(
                vm,
                this,
                "write",
                "(Ljava/lang/String;)V",
                vec![Value::Ref(text)],
            )?;
            object(this)
        },
    ),
    method(
        "append",
        "(Ljava/lang/CharSequence;II)Ljava/io/Writer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let sequence = vm.string_or_empty(ref_arg(args, 1));
            let start = int_arg(args, 2).max(0) as usize;
            let end = int_arg(args, 3).max(0) as usize;
            let units: Vec<u16> = sequence.encode_utf16().collect();
            let text: String =
                String::from_utf16_lossy(&units[start.min(units.len())..end.min(units.len())]);
            let text = vm.make_string(&text)?;
            buffered_writer_call(
                vm,
                this,
                "write",
                "(Ljava/lang/String;)V",
                vec![Value::Ref(text)],
            )?;
            object(this)
        },
    ),
    method(
        "append",
        "(C)Ljava/io/Writer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let ch = char::from_u32(int_arg(args, 1) as u32 & 0xFFFF).unwrap_or('\u{FFFD}');
            let mut text = String::new();
            text.push(ch);
            let text = vm.make_string(&text)?;
            buffered_writer_call(
                vm,
                this,
                "write",
                "(Ljava/lang/String;)V",
                vec![Value::Ref(text)],
            )?;
            object(this)
        },
    ),
];

pub(crate) const BUFFERED_WRITER: oxjvm_vm::NativeClass = class(
    "java/io/BufferedWriter",
    Some("java/io/Writer"),
    &[],
    ACC_PUBLIC,
    &[field("out", "Ljava/io/Writer;", ACC_PRIVATE, None)],
    &BUFFERED_WRITER_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Serialization metadata (reflection-only stubs)
// -------------------------------------------------------------------------------------------

const OBJECT_STREAM_CLASS_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "lookup",
        "(Ljava/lang/Class;)Ljava/io/ObjectStreamClass;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            // `lookup` initializes the target class while building the
            // descriptor.  A class whose initializer fails must propagate the
            // initialization error instead of producing a partial descriptor.
            let target = crate::lang::class_argument(vm, ref_arg(args, 0))?;
            vm.ensure_initialized(target)?;
            let class = vm.resolve_class("java/io/ObjectStreamClass")?;
            let instance = vm.new_instance(class)?;
            let (declaring, field) = vm.find_field(class, "compat", "Ljava/lang/Class;")?;
            vm.set_instance_ref(instance, declaring, field, ref_arg(args, 0));
            object(instance)
        },
    ),
    method(
        "getFields",
        "()[Ljava/io/ObjectStreamField;",
        ACC_PUBLIC,
        |vm, _, _| {
            let field = vm.resolve_class("java/io/ObjectStreamField")?;
            let array = vm.allocate_object_array(field, 0)?;
            object(array)
        },
    ),
    method("getSerialVersionUID", "()J", ACC_PUBLIC, |_, _, _| {
        Ok(Value::Long(0))
    }),
    method(
        "lookupAny",
        "(Ljava/lang/Class;)Ljava/io/ObjectStreamClass;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let target = crate::lang::class_argument(vm, ref_arg(args, 0))?;
            vm.ensure_initialized(target)?;
            let class = vm.resolve_class("java/io/ObjectStreamClass")?;
            let instance = vm.new_instance(class)?;
            let (declaring, field) = vm.find_field(class, "compat", "Ljava/lang/Class;")?;
            vm.set_instance_ref(instance, declaring, field, ref_arg(args, 0));
            object(instance)
        },
    ),
    method(
        "getField",
        "(Ljava/lang/String;)Ljava/io/ObjectStreamField;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "forClass",
        "()Ljava/lang/Class;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(
                vm.read_ref_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "compat",
                    "Ljava/lang/Class;",
                )
                .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = vm
                .read_ref_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "compat",
                    "Ljava/lang/Class;",
                )
                .unwrap_or(ObjectRef::NULL);
            let text = name.is_null().then(String::new).unwrap_or_default();
            object(vm.make_string(&text)?)
        },
    ),
];

/// `java.io.ObjectStreamClass`: only what reflective serialization metadata needs.
pub(crate) const OBJECT_STREAM_CLASS: oxjvm_vm::NativeClass = class(
    "java/io/ObjectStreamClass",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[field("compat", "Ljava/lang/Class;", ACC_PRIVATE, None)],
    &OBJECT_STREAM_CLASS_METHODS,
    None,
);

const OBJECT_STREAM_FIELD_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method("getName", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, _| {
        object(vm.make_string("")?)
    }),
    method("getOffset", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/Class;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let name = crate::string_arg(vm, args[1])?;
            vm.set_string_field(this, class, "name", &name)?;
            let (declaring, field) = vm.find_field(class, "type", "Ljava/lang/Class;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 2));
            crate::lang::write_int_field(vm, this, "primitive", 0);
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/Class;Z)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let name = crate::string_arg(vm, args[1])?;
            vm.set_string_field(this, class, "name", &name)?;
            let (declaring, field) = vm.find_field(class, "type", "Ljava/lang/Class;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 2));
            crate::lang::write_int_field(vm, this, "primitive", int_arg(args, 3));
            void()
        },
    ),
    method(
        "getType",
        "()Ljava/lang/Class;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(
                vm.read_ref_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "type",
                    "Ljava/lang/Class;",
                )
                .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method(
        "getTypeString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("")?),
    ),
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = vm
                .read_string_field(receiver(args), vm.class_of(receiver(args)), "name")
                .unwrap_or_default();
            object(vm.make_string(&name)?)
        },
    ),
    method("isPrimitive", "()Z", ACC_PUBLIC, |vm, _, args| {
        crate::boolean(crate::lang::read_int_field(vm, receiver(args), "primitive") != 0)
    }),
    method(
        "compareTo",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC,
        |_, _, _| int(0),
    ),
];

/// `java.io.ObjectStreamField`: a placeholder field descriptor.
pub(crate) const OBJECT_STREAM_FIELD: oxjvm_vm::NativeClass = class(
    "java/io/ObjectStreamField",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("type", "Ljava/lang/Class;", ACC_PRIVATE, None),
        field("primitive", "I", ACC_PRIVATE, None),
    ],
    &OBJECT_STREAM_FIELD_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// StringReader
// -------------------------------------------------------------------------------------------

const STRING_READER_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "str", "Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method("read", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        let next = crate::lang::read_int_field(vm, this, "next");
        let text = vm
            .read_ref_field(this, class, "str", "Ljava/lang/String;")
            .unwrap_or(ObjectRef::NULL);
        let Some(units) = vm.string_utf16(text) else {
            return int(-1);
        };
        if next < 0 || next as usize >= units.len() {
            return int(-1);
        }
        let unit = units[next as usize];
        crate::lang::write_int_field(vm, this, "next", next + 1);
        int(i32::from(unit))
    }),
    method("read", "([CII)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        let array = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        if length == 0 {
            return int(0);
        }
        let next = crate::lang::read_int_field(vm, this, "next");
        let text = vm
            .read_ref_field(this, class, "str", "Ljava/lang/String;")
            .unwrap_or(ObjectRef::NULL);
        let Some(units) = vm.string_utf16(text).map(<[u16]>::to_vec) else {
            return int(-1);
        };
        if next < 0 || next as usize >= units.len() {
            return int(-1);
        }
        let available = units.len() - next as usize;
        let count = available.min(length as usize);
        for index in 0..count {
            let value = Value::Int(i32::from(units[next as usize + index]));
            vm.array_set(array, offset + index as i32, value, false)?;
        }
        crate::lang::write_int_field(vm, this, "next", next + count as i32);
        int(count as i32)
    }),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
];

/// `java.io.StringReader`: reads UTF-16 units from a string.
pub(crate) const STRING_READER: oxjvm_vm::NativeClass = class(
    "java/io/StringReader",
    Some("java/io/Reader"),
    &[],
    ACC_PUBLIC,
    &[
        field("str", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("next", "I", ACC_PRIVATE, None),
    ],
    &STRING_READER_METHODS,
    None,
);
