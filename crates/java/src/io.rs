//! `java.io`: the stream classes oxjvm implements natively.

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError, format};

use crate::lang::{read_int_field, write_int_field};
use crate::{boolean, class, field, int, int_arg, long, method, object, receiver, ref_arg, void};

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
io_exception!(
    OBJECT_STREAM_EXCEPTION,
    "java/io/ObjectStreamException",
    "java/io/IOException"
);
io_exception!(
    INVALID_OBJECT_EXCEPTION,
    "java/io/InvalidObjectException",
    "java/io/ObjectStreamException"
);

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

const OUTPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
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
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        reader_check_range(vm, array, offset, length)?;
        for index in offset..offset + length {
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
    method(
        "nullOutputStream",
        "()Ljava/io/OutputStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/io/OutputStream$NullOutputStream")?;
            object(vm.new_instance(class)?)
        },
    ),
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
        let this = receiver(args);
        // FilterOutputStream.close flushes through the virtual flush() so
        // buffering subclasses drain before the underlying stream closes.
        let (flush_class, flush_method) = vm.resolve_virtual_method(this, "flush", "()V")?;
        vm.invoke_method(flush_class, flush_method, vec![Value::Ref(this)])?;
        let out = vm
            .read_ref_field(this, vm.class_of(this), "out", "Ljava/io/OutputStream;")
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

const PRINT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 39] = [
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
        "(Ljava/io/OutputStream;ZLjava/nio/charset/Charset;)V",
        ACC_PUBLIC,
        print_stream_init,
    ),
    method(
        "<init>",
        "(Ljava/io/OutputStream;Ljava/nio/charset/Charset;)V",
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
        "(Ljava/io/File;Ljava/nio/charset/Charset;)V",
        ACC_PUBLIC,
        print_stream_init_file,
    ),
    method("checkError", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        print_stream_flush(vm, this)?;
        // A nested PrintStream owns its own trouble flag; ask it directly.
        let out = print_stream_out_ref(vm, this);
        if !out.is_null() {
            let print_stream = vm.resolve_class("java/io/PrintStream")?;
            if vm.is_instance(out, print_stream) {
                let (class, method) = vm.resolve_virtual_method(out, "checkError", "()Z")?;
                let value = vm.invoke_method(class, method, vec![Value::Ref(out)])?;
                return boolean(value.as_int() != 0);
            }
        }
        boolean(read_int_field(vm, this, "trouble") != 0)
    }),
    method("clearError", "()V", ACC_PROTECTED, |vm, _, args| {
        write_int_field(vm, receiver(args), "trouble", 0);
        void()
    }),
    method("writeBytes", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let bytes = print_stream_byte_arg(vm, ref_arg(args, 1))?;
        print_stream_write(vm, this, &bytes)?;
        print_stream_autoflush(vm, this)?;
        void()
    }),
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        print_stream_init_string,
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        print_stream_init_string,
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/nio/charset/Charset;)V",
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
    method("print", "(C)V", ACC_PUBLIC, |vm, _, args| {
        let unit = int_arg(args, 1) as u32 & 0xFFFF;
        let text = char::from_u32(unit).map_or_else(String::new, |ch| ch.to_string());
        print_text(vm, receiver(args), &text)
    }),
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
        print_line(vm, receiver(args), "")
    }),
    method("println", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        let text = if crate::bool_arg(args, 1) {
            "true"
        } else {
            "false"
        };
        print_line(vm, receiver(args), text)
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
        print_stream_flush(vm, receiver(args))?;
        void()
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if read_int_field(vm, this, "closed") != 0 {
            return void();
        }
        write_int_field(vm, this, "closed", 1);
        print_stream_flush(vm, this)?;
        print_stream_call_out(vm, this, "close", "()V")?;
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
    method(
        "charset",
        "()Ljava/nio/charset/Charset;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let charset = vm
                .read_ref_field(this, class, "charset", "Ljava/nio/charset/Charset;")
                .unwrap_or(ObjectRef::NULL);
            if !charset.is_null() {
                return object(charset);
            }
            let charset_class = vm.resolve_class("java/nio/charset/Charset")?;
            let (declaring, method) = vm
                .find_method(
                    charset_class,
                    "defaultCharset",
                    "()Ljava/nio/charset/Charset;",
                )
                .ok_or_else(|| VmError::internal("Charset.defaultCharset"))?;
            vm.invoke_method(declaring, method, vec![])
        },
    ),
];

fn print_stream_init(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    print_stream_out(vm, this, ref_arg(args, 1))?;
    // The boolean second argument (when present) selects auto-flush.
    let auto_flush = matches!(args.get(2), Some(Value::Int(flag)) if *flag != 0);
    write_int_field(vm, this, "autoFlush", i32::from(auto_flush));
    let charset_class = vm.resolve_class("java/nio/charset/Charset")?;
    let mut charset = ObjectRef::NULL;
    for arg in args.iter().skip(2) {
        let Value::Ref(reference) = arg else { continue };
        if reference.is_null() {
            continue;
        }
        if vm.is_instance(*reference, charset_class) {
            charset = *reference;
            break;
        }
        if vm.string_value(*reference).is_some() {
            let (declaring, method) = vm
                .find_method(
                    charset_class,
                    "forName",
                    "(Ljava/lang/String;)Ljava/nio/charset/Charset;",
                )
                .ok_or_else(|| VmError::internal("Charset.forName"))?;
            charset = vm
                .invoke_method(declaring, method, vec![Value::Ref(*reference)])?
                .as_ref();
            break;
        }
    }
    if !charset.is_null() {
        let class = vm.class_of(this);
        if let Some((declaring, field)) =
            vm.find_field_opt(class, "charset", "Ljava/nio/charset/Charset;")
        {
            vm.set_instance_ref(this, declaring, field, charset);
        }
    }
    void()
}

/// Point a print stream at an `OutputStream`; the kind stays 0 so writes reach `out`.
fn print_stream_out(vm: &mut Vm<'_>, this: ObjectRef, stream: ObjectRef) -> Result<Value, VmError> {
    if stream.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
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
    let kind = read_int_field(vm, print_stream, "kind");
    if kind == 2 {
        oxjvm_platform::Stream::Stderr
    } else {
        oxjvm_platform::Stream::Stdout
    }
}

/// Write raw bytes to a print stream: through its `OutputStream` when it has one, to the
/// host's stdout/stderr for the `System.out`/`System.err` streams otherwise.
fn print_stream_out_ref(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "out", "Ljava/io/OutputStream;")
        .unwrap_or(ObjectRef::NULL)
}

/// Call a void method on the underlying stream; an IOException sets the
/// trouble flag instead of propagating, the way PrintStream reports errors.
fn print_stream_call_out(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: &str,
    descriptor: &str,
) -> Result<(), VmError> {
    let out = print_stream_out_ref(vm, this);
    if out.is_null() {
        return Ok(());
    }
    let (class, method) = vm.resolve_virtual_method(out, name, descriptor)?;
    match vm.invoke_method(class, method, vec![Value::Ref(out)]) {
        Ok(_) => Ok(()),
        Err(VmError::Thrown(exception)) => {
            let io_exception = vm.resolve_class("java/io/IOException")?;
            if vm.is_instance(exception, io_exception) {
                write_int_field(vm, this, "trouble", 1);
                Ok(())
            } else {
                Err(VmError::Thrown(exception))
            }
        }
        Err(other) => Err(other),
    }
}

fn print_stream_flush(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    if print_stream_out_ref(vm, this).is_null() {
        vm.flush_host(stream_of(vm, this));
        return Ok(());
    }
    print_stream_call_out(vm, this, "flush", "()V")
}

fn print_stream_autoflush(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    if read_int_field(vm, this, "autoFlush") != 0 {
        print_stream_flush(vm, this)?;
    }
    Ok(())
}

fn print_stream_write(vm: &mut Vm<'_>, this: ObjectRef, bytes: &[u8]) -> Result<Value, VmError> {
    if this.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    if read_int_field(vm, this, "closed") != 0 {
        write_int_field(vm, this, "trouble", 1);
        return void();
    }
    let out = print_stream_out_ref(vm, this);
    if !out.is_null() {
        let array = write_bytes_array(vm, bytes)?;
        let (declaring, method) = vm.resolve_virtual_method(out, "write", "([B)V")?;
        match vm.invoke_method(declaring, method, vec![Value::Ref(out), Value::Ref(array)]) {
            Ok(_) => {}
            Err(VmError::Thrown(exception)) => {
                let io_exception = vm.resolve_class("java/io/IOException")?;
                if vm.is_instance(exception, io_exception) {
                    write_int_field(vm, this, "trouble", 1);
                    return void();
                }
                return Err(VmError::Thrown(exception));
            }
            Err(other) => return Err(other),
        }
        return void();
    }
    let stream = stream_of(vm, this);
    vm.write_host(stream, bytes);
    void()
}

/// The bytes of a `byte[]` argument, with the JDK's null check.
fn print_stream_byte_arg(vm: &mut Vm<'_>, array: ObjectRef) -> Result<Vec<u8>, VmError> {
    if array.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    match vm.heap.get(array).map(|object| &object.data) {
        Some(oxjvm_vm::ObjectData::Array(oxjvm_vm::ArrayData::Byte(bytes))) => {
            Ok(bytes.iter().map(|byte| *byte as u8).collect())
        }
        _ => Err(vm.throw_new("java/lang/NullPointerException", None)),
    }
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
    let text = crate::lang::format_string(vm, &format, &arguments, "")?;
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
    print_stream_write(vm, print_stream, b"\n")?;
    print_stream_autoflush(vm, print_stream)?;
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
        field("charset", "Ljava/nio/charset/Charset;", ACC_PRIVATE, None),
        field("trouble", "I", ACC_PRIVATE, None),
        field("autoFlush", "I", ACC_PRIVATE, None),
        field("closed", "I", ACC_PRIVATE, None),
    ],
    &PRINT_STREAM_METHODS,
    None,
);

const FILE_DESCRIPTOR_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("valid", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("java.io.FileDescriptor")?),
    ),
];

/// `java.io.FileDescriptor`: the VM models file streams as in-memory buffers,
/// so the descriptor is always valid while the stream is open.
pub(crate) const FILE_DESCRIPTOR: oxjvm_vm::NativeClass = class(
    "java/io/FileDescriptor",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &FILE_DESCRIPTOR_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// InputStream / ByteArray streams
// -------------------------------------------------------------------------------------------

const INPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 15] = [
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
        let this = receiver(args);
        let array = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        reader_check_range(vm, array, offset, length)?;
        if length == 0 {
            return int(0);
        }
        // The first byte goes through the overridable read(); its IOException
        // propagates. Later failures end the read with the count so far, the
        // way InputStream.read(byte[],int,int) does.
        let (read_class, read_method) = vm.resolve_virtual_method(this, "read", "()I")?;
        let first = vm
            .invoke_method(read_class, read_method, vec![Value::Ref(this)])?
            .as_int();
        if first < 0 {
            return int(-1);
        }
        vm.array_set(array, offset, Value::Int(first), false)?;
        let mut count = 1;
        while count < length {
            let (read_class, read_method) = vm.resolve_virtual_method(this, "read", "()I")?;
            match vm.invoke_method(read_class, read_method, vec![Value::Ref(this)]) {
                Ok(value) => {
                    let byte = value.as_int();
                    if byte < 0 {
                        break;
                    }
                    vm.array_set(array, offset + count, Value::Int(byte), false)?;
                    count += 1;
                }
                Err(VmError::Thrown(exception)) => {
                    let io_exception = vm.resolve_class("java/io/IOException")?;
                    if vm.is_instance(exception, io_exception) {
                        break;
                    }
                    return Err(VmError::Thrown(exception));
                }
                Err(other) => return Err(other),
            }
        }
        int(count)
    }),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("skip", "(J)J", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let mut remaining = args[1].as_long().max(0);
        let mut skipped: i64 = 0;
        while remaining > 0 {
            let (declaring, method) = vm.resolve_virtual_method(this, "read", "()I")?;
            let value = vm
                .invoke_method(declaring, method, vec![Value::Ref(this)])?
                .as_int();
            if value < 0 {
                break;
            }
            skipped += 1;
            remaining -= 1;
        }
        Ok(Value::Long(skipped))
    }),
    method("available", "()I", ACC_PUBLIC, |_, _, _| int(0)),
    method("mark", "(I)V", ACC_PUBLIC, |_, _, _| void()),
    method("reset", "()V", ACC_PUBLIC, |vm, _, _| {
        Err(vm.throw_new("java/io/IOException", Some("mark/reset not supported")))
    }),
    method("markSupported", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("readNBytes", "(I)[B", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let limit = int_arg(args, 1).max(0) as usize;
        let mut bytes = Vec::new();
        while bytes.len() < limit {
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
    method("readNBytes", "([BII)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let array = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        let mut read = 0;
        while read < length {
            let (declaring, method) = vm.resolve_virtual_method(this, "read", "()I")?;
            let value = vm
                .invoke_method(declaring, method, vec![Value::Ref(this)])?
                .as_int();
            if value < 0 {
                break;
            }
            vm.array_set(array, offset + read, Value::Int(value), false)?;
            read += 1;
        }
        int(read)
    }),
    method(
        "nullInputStream",
        "()Ljava/io/InputStream;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/io/InputStream$NullInputStream")?;
            object(vm.new_instance(class)?)
        },
    ),
    method("skipNBytes", "(J)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let mut remaining = crate::long_arg(args, 1);
        while remaining > 0 {
            let (skip_class, skip_method) = vm.resolve_virtual_method(this, "skip", "(J)J")?;
            let skipped = vm
                .invoke_method(
                    skip_class,
                    skip_method,
                    vec![Value::Ref(this), Value::Long(remaining)],
                )?
                .as_long();
            // A skip that reports a negative count or more than requested
            // cannot be trusted to make progress.
            if skipped < 0 || skipped > remaining {
                return Err(vm.throw_new("java/io/IOException", Some("Unable to skip exactly")));
            }
            if skipped > 0 {
                remaining -= skipped;
                continue;
            }
            let (read_class, read_method) = vm.resolve_virtual_method(this, "read", "()I")?;
            if vm
                .invoke_method(read_class, read_method, vec![Value::Ref(this)])?
                .as_int()
                < 0
            {
                return Err(vm.throw_new("java/io/EOFException", None));
            }
            remaining -= 1;
        }
        void()
    }),
    method(
        "transferTo",
        "(Ljava/io/OutputStream;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let out = ref_arg(args, 1);
            if out.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("out")));
            }
            let buffer = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, 8192)?;
            let mut total = 0i64;
            loop {
                let (read_class, read_method) =
                    vm.resolve_virtual_method(this, "read", "([BII)I")?;
                let count = vm
                    .invoke_method(
                        read_class,
                        read_method,
                        vec![
                            Value::Ref(this),
                            Value::Ref(buffer),
                            Value::Int(0),
                            Value::Int(8192),
                        ],
                    )?
                    .as_int();
                if count < 0 {
                    break;
                }
                let (write_class, write_method) =
                    vm.resolve_virtual_method(out, "write", "([BII)V")?;
                vm.invoke_method(
                    write_class,
                    write_method,
                    vec![
                        Value::Ref(out),
                        Value::Ref(buffer),
                        Value::Int(0),
                        Value::Int(count),
                    ],
                )?;
                total += i64::from(count);
            }
            long(total)
        },
    ),
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

const BYTE_ARRAY_INPUT_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method("<init>", "([B)V", ACC_PUBLIC, byte_array_input_init),
    method("<init>", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        let buf = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        let total = if buf.is_null() {
            0
        } else {
            vm.array_length(buf)? as i32
        };
        let count = (offset + length).min(total);
        let (declaring, field) = vm.find_field(class, "buf", "[B")?;
        vm.set_instance_ref(this, declaring, field, buf);
        let (declaring, field) = vm.find_field(class, "pos", "I")?;
        vm.set_instance_int(this, declaring, field, offset);
        let (declaring, field) = vm.find_field(class, "count", "I")?;
        vm.set_instance_int(this, declaring, field, count);
        let (declaring, field) = vm.find_field(class, "mark", "I")?;
        vm.set_instance_int(this, declaring, field, offset);
        void()
    }),
    method(
        "read",
        "()I",
        ACC_PUBLIC,
        |vm, _, args| match crate::lang::read_input_byte(vm, receiver(args))? {
            None => int(-1),
            Some(byte) => int(byte),
        },
    ),
    method("read", "([BII)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let target = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let count = int_arg(args, 3);
        // Bounds are validated before the zero-length and EOF shortcuts.
        reader_check_range(vm, target, offset, count)?;
        if count == 0 {
            return int(0);
        }
        let class = vm.class_of(this);
        let buf = vm
            .read_ref_field(this, class, "buf", "[B")
            .unwrap_or(ObjectRef::NULL);
        let pos = vm
            .read_instance_int_named(this, class, "pos", "I")
            .unwrap_or(0);
        if buf.is_null() {
            return int(-1);
        }
        let length = vm
            .read_instance_int_named(this, class, "count", "I")
            .unwrap_or(vm.array_length(buf)? as i32);
        if pos >= length {
            return int(-1);
        }
        let count = count.min(length - pos);
        for index in 0..count {
            let byte = vm.array_get(buf, pos + index)?;
            vm.array_set(target, offset + index, byte, false)?;
        }
        let (declaring, field) = vm.find_field(class, "pos", "I")?;
        vm.set_instance_int(this, declaring, field, pos + count);
        int(count)
    }),
    method("available", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        let buf = vm
            .read_ref_field(this, class, "buf", "[B")
            .unwrap_or(ObjectRef::NULL);
        let pos = vm
            .read_instance_int_named(this, class, "pos", "I")
            .unwrap_or(0);
        if buf.is_null() {
            return int(0);
        }
        let length = vm
            .read_instance_int_named(this, class, "count", "I")
            .unwrap_or(vm.array_length(buf)? as i32);
        int((length - pos).max(0))
    }),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("mark", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        let pos = vm
            .read_instance_int_named(this, class, "pos", "I")
            .unwrap_or(0);
        let (declaring, field) = vm.find_field(class, "mark", "I")?;
        vm.set_instance_int(this, declaring, field, pos);
        void()
    }),
    method("reset", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        let mark = vm
            .read_instance_int_named(this, class, "mark", "I")
            .unwrap_or(0);
        let (declaring, field) = vm.find_field(class, "pos", "I")?;
        vm.set_instance_int(this, declaring, field, mark);
        void()
    }),
    method("markSupported", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
    method(
        "transferTo",
        "(Ljava/io/OutputStream;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let out = ref_arg(args, 1);
            if out.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("out")));
            }
            let class = vm.class_of(this);
            let buf = vm
                .read_ref_field(this, class, "buf", "[B")
                .unwrap_or(ObjectRef::NULL);
            let pos = vm
                .read_instance_int_named(this, class, "pos", "I")
                .unwrap_or(0);
            let count = vm
                .read_instance_int_named(this, class, "count", "I")
                .unwrap_or(if buf.is_null() {
                    0
                } else {
                    vm.array_length(buf)? as i32
                });
            let len = count - pos;
            if buf.is_null() || len <= 0 {
                return long(0);
            }
            // Only a few JDK stream types are trusted not to scribble on the
            // array they are handed; everyone else gets a temporary copy so
            // the internal buffer stays intact (jdk TransferToTrusted).
            let trusted = {
                let name = vm.class_name(vm.class_of(out));
                name == "java/io/ByteArrayOutputStream"
                    || name == "java/io/FileOutputStream"
                    || name == "java/io/PipedOutputStream"
            };
            let (write_class, write_method) = vm.resolve_virtual_method(out, "write", "([BII)V")?;
            let max_transfer = 128 * 1024;
            let mut written = 0;
            if trusted {
                while written < len {
                    let nbyte = (len - written).min(max_transfer);
                    vm.invoke_method(
                        write_class,
                        write_method,
                        vec![
                            Value::Ref(out),
                            Value::Ref(buf),
                            Value::Int(pos + written),
                            Value::Int(nbyte),
                        ],
                    )?;
                    written += nbyte;
                }
            } else {
                let tmp = vm.allocate_array_of(
                    oxjvm_vm::ArrayComponent::Byte,
                    len.min(max_transfer) as usize,
                )?;
                while written < len {
                    let nbyte = (len - written).min(max_transfer);
                    for index in 0..nbyte {
                        let byte = vm.array_get(buf, pos + written + index)?;
                        vm.array_set(tmp, index, byte, false)?;
                    }
                    vm.invoke_method(
                        write_class,
                        write_method,
                        vec![
                            Value::Ref(out),
                            Value::Ref(tmp),
                            Value::Int(0),
                            Value::Int(nbyte),
                        ],
                    )?;
                    written += nbyte;
                }
            }
            let (declaring, field) = vm.find_field(class, "pos", "I")?;
            vm.set_instance_int(this, declaring, field, count);
            long(i64::from(len))
        },
    ),
];

fn byte_array_input_init(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let class = vm.class_of(this);
    let buf = ref_arg(args, 1);
    let (declaring, field) = vm.find_field(class, "buf", "[B")?;
    vm.set_instance_ref(this, declaring, field, buf);
    let (declaring, field) = vm.find_field(class, "pos", "I")?;
    vm.set_instance_int(this, declaring, field, 0);
    let total = if buf.is_null() {
        0
    } else {
        vm.array_length(buf)? as i32
    };
    let (declaring, field) = vm.find_field(class, "count", "I")?;
    vm.set_instance_int(this, declaring, field, total);
    let (declaring, field) = vm.find_field(class, "mark", "I")?;
    vm.set_instance_int(this, declaring, field, 0);
    void()
}

// -------------------------------------------------------------------------------------------
// PushbackInputStream / SequenceInputStream
// -------------------------------------------------------------------------------------------

fn pushback_buffer(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "buf", "[B")
        .unwrap_or(ObjectRef::NULL)
}

fn pushback_pos(vm: &Vm<'_>, this: ObjectRef) -> i32 {
    read_int_field(vm, this, "pos")
}

fn pushback_in(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "in", "Ljava/io/InputStream;")
        .unwrap_or(ObjectRef::NULL)
}

fn pushback_ensure_open(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    if pushback_in(vm, this).is_null() {
        return Err(vm.throw_new("java/io/IOException", Some("Stream closed")));
    }
    Ok(())
}

fn stream_read_byte(vm: &mut Vm<'_>, stream: ObjectRef) -> Result<i32, VmError> {
    if stream.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let (class, method) = vm.resolve_virtual_method(stream, "read", "()I")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(stream)])?
        .as_int())
}

const PUSHBACK_INPUT_METHODS: [oxjvm_vm::NativeMethodDef; 13] = [
    method(
        "<init>",
        "(Ljava/io/InputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "in", "Ljava/io/InputStream;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            let buf = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, 1)?;
            let (declaring, field) = vm.find_field(vm.class_of(this), "buf", "[B")?;
            vm.set_instance_ref(this, declaring, field, buf);
            write_int_field(vm, this, "pos", 1);
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/InputStream;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let size = int_arg(args, 2);
            if size <= 0 {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("size <= 0")));
            }
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "in", "Ljava/io/InputStream;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            let buf = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, size as usize)?;
            let (declaring, field) = vm.find_field(vm.class_of(this), "buf", "[B")?;
            vm.set_instance_ref(this, declaring, field, buf);
            write_int_field(vm, this, "pos", size);
            void()
        },
    ),
    method("read", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        pushback_ensure_open(vm, this)?;
        let buf = pushback_buffer(vm, this);
        let pos = pushback_pos(vm, this);
        let length = vm.array_length(buf)? as i32;
        if pos < length {
            let value = vm.array_get(buf, pos)?.as_int() & 0xff;
            write_int_field(vm, this, "pos", pos + 1);
            return int(value);
        }
        int(stream_read_byte(vm, pushback_in(vm, this))?)
    }),
    method("read", "([BII)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        pushback_ensure_open(vm, this)?;
        let dst = ref_arg(args, 1);
        let mut offset = int_arg(args, 2);
        let mut length = int_arg(args, 3);
        if dst.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        let total = vm.array_length(dst)? as i32;
        if offset < 0 || length < 0 || length > total - offset {
            return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
        }
        if length == 0 {
            return int(0);
        }
        let buf = pushback_buffer(vm, this);
        let mut pos = pushback_pos(vm, this);
        let buf_len = vm.array_length(buf)? as i32;
        let mut count = 0;
        while pos < buf_len && length > 0 {
            let value = vm.array_get(buf, pos)?.as_int();
            vm.array_set(dst, offset, Value::Int(value), true)?;
            pos += 1;
            offset += 1;
            length -= 1;
            count += 1;
        }
        write_int_field(vm, this, "pos", pos);
        if length > 0 {
            let stream = pushback_in(vm, this);
            let (class, method) = vm.resolve_virtual_method(stream, "read", "([BII)I")?;
            let read = vm
                .invoke_method(
                    class,
                    method,
                    vec![
                        Value::Ref(stream),
                        Value::Ref(dst),
                        Value::Int(offset),
                        Value::Int(length),
                    ],
                )?
                .as_int();
            if read > 0 {
                count += read;
            } else if count == 0 {
                return int(read);
            }
        }
        int(count)
    }),
    method("unread", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        pushback_ensure_open(vm, this)?;
        let pos = pushback_pos(vm, this);
        if pos == 0 {
            return Err(vm.throw_new("java/io/IOException", Some("Push back buffer is full")));
        }
        let buf = pushback_buffer(vm, this);
        vm.array_set(buf, pos - 1, Value::Int(int_arg(args, 1)), true)?;
        write_int_field(vm, this, "pos", pos - 1);
        void()
    }),
    method("unread", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        pushback_ensure_open(vm, this)?;
        let src = ref_arg(args, 1);
        if src.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        let length = vm.array_length(src)? as i32;
        let pos = pushback_pos(vm, this);
        if length > pos {
            return Err(vm.throw_new("java/io/IOException", Some("Push back buffer is full")));
        }
        let buf = pushback_buffer(vm, this);
        for index in 0..length {
            let value = vm.array_get(src, index)?.as_int();
            vm.array_set(buf, pos - length + index, Value::Int(value), true)?;
        }
        write_int_field(vm, this, "pos", pos - length);
        void()
    }),
    method("unread", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        pushback_ensure_open(vm, this)?;
        let src = ref_arg(args, 1);
        if src.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        let total = vm.array_length(src)? as i32;
        if offset < 0 || length < 0 || length > total - offset {
            return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
        }
        let pos = pushback_pos(vm, this);
        if length > pos {
            return Err(vm.throw_new("java/io/IOException", Some("Push back buffer is full")));
        }
        let buf = pushback_buffer(vm, this);
        for index in 0..length {
            let value = vm.array_get(src, offset + index)?.as_int();
            vm.array_set(buf, pos - length + index, Value::Int(value), true)?;
        }
        write_int_field(vm, this, "pos", pos - length);
        void()
    }),
    method("available", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        pushback_ensure_open(vm, this)?;
        let buf = pushback_buffer(vm, this);
        let pushed = vm.array_length(buf)? as i32 - pushback_pos(vm, this);
        let stream = pushback_in(vm, this);
        let (class, method) = vm.resolve_virtual_method(stream, "available", "()I")?;
        let available = vm
            .invoke_method(class, method, vec![Value::Ref(stream)])?
            .as_int();
        int(pushed.max(0) + available)
    }),
    method("markSupported", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("mark", "(I)V", ACC_PUBLIC, |_, _, _| void()),
    method("reset", "()V", ACC_PUBLIC, |vm, _, _| {
        Err(vm.throw_new("java/io/IOException", Some("mark/reset not supported")))
    }),
    method("skip", "(J)J", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        pushback_ensure_open(vm, this)?;
        let mut remaining = crate::long_arg(args, 1);
        if remaining <= 0 {
            return long(0);
        }
        let buf = pushback_buffer(vm, this);
        let mut pos = pushback_pos(vm, this);
        let buf_len = vm.array_length(buf)? as i32;
        let mut skipped = 0i64;
        while pos < buf_len && remaining > 0 {
            pos += 1;
            remaining -= 1;
            skipped += 1;
        }
        write_int_field(vm, this, "pos", pos);
        if remaining > 0 {
            let stream = pushback_in(vm, this);
            let (class, method) = vm.resolve_virtual_method(stream, "skip", "(J)J")?;
            skipped += vm
                .invoke_method(
                    class,
                    method,
                    vec![Value::Ref(stream), Value::Long(remaining)],
                )?
                .as_long();
        }
        long(skipped)
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let stream = pushback_in(vm, this);
        if stream.is_null() {
            return void();
        }
        let (class, method) = vm.resolve_virtual_method(stream, "close", "()V")?;
        vm.invoke_method(class, method, vec![Value::Ref(stream)])?;
        // Closing detaches the source and the pushback buffer so later
        // reads, unreads, skips and available calls report "Stream closed".
        let declaring_class = vm.class_of(this);
        if let Some((declaring, field)) =
            vm.find_field_opt(declaring_class, "in", "Ljava/io/InputStream;")
        {
            vm.set_instance_ref(this, declaring, field, ObjectRef::NULL);
        }
        if let Some((declaring, field)) = vm.find_field_opt(declaring_class, "buf", "[B") {
            vm.set_instance_ref(this, declaring, field, ObjectRef::NULL);
        }
        void()
    }),
];

pub(crate) const PUSHBACK_INPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/io/PushbackInputStream",
    Some("java/io/FilterInputStream"),
    &[],
    ACC_PUBLIC,
    &[
        field("buf", "[B", ACC_PROTECTED, None),
        field("pos", "I", ACC_PROTECTED, None),
    ],
    &PUSHBACK_INPUT_METHODS,
    None,
);

fn sequence_streams(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "streams", "Ljava/util/ArrayList;")
        .unwrap_or(ObjectRef::NULL)
}

fn sequence_current(vm: &mut Vm<'_>, this: ObjectRef) -> Result<ObjectRef, VmError> {
    let streams = sequence_streams(vm, this);
    let index = read_int_field(vm, this, "index");
    let (class, method) = vm.resolve_virtual_method(streams, "size", "()I")?;
    let size = vm
        .invoke_method(class, method, vec![Value::Ref(streams)])?
        .as_int();
    if index >= size {
        return Ok(ObjectRef::NULL);
    }
    let (class, method) = vm.resolve_virtual_method(streams, "get", "(I)Ljava/lang/Object;")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(streams), Value::Int(index)])?
        .as_ref())
}

fn sequence_advance(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    let current = sequence_current(vm, this)?;
    if !current.is_null() {
        let (class, method) = vm.resolve_virtual_method(current, "close", "()V")?;
        vm.invoke_method(class, method, vec![Value::Ref(current)])?;
    }
    write_int_field(vm, this, "index", read_int_field(vm, this, "index") + 1);
    Ok(())
}

const SEQUENCE_INPUT_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "<init>",
        "(Ljava/io/InputStream;Ljava/io/InputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let first = ref_arg(args, 1);
            let second = ref_arg(args, 2);
            if first.is_null() || second.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let list = crate::util::array_list_from(vm, &[first, second])?;
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "streams", "Ljava/util/ArrayList;")?;
            vm.set_instance_ref(this, declaring, field, list);
            write_int_field(vm, this, "index", 0);
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/util/Enumeration;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let enumeration = ref_arg(args, 1);
            if enumeration.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let list = crate::util::array_list_from(vm, &[])?;
            loop {
                let (class, method) =
                    vm.resolve_virtual_method(enumeration, "hasMoreElements", "()Z")?;
                let more = vm
                    .invoke_method(class, method, vec![Value::Ref(enumeration)])?
                    .as_int()
                    != 0;
                if !more {
                    break;
                }
                let (class, method) =
                    vm.resolve_virtual_method(enumeration, "nextElement", "()Ljava/lang/Object;")?;
                let stream = vm
                    .invoke_method(class, method, vec![Value::Ref(enumeration)])?
                    .as_ref();
                let (class, method) =
                    vm.resolve_virtual_method(list, "add", "(Ljava/lang/Object;)Z")?;
                vm.invoke_method(class, method, vec![Value::Ref(list), Value::Ref(stream)])?;
            }
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "streams", "Ljava/util/ArrayList;")?;
            vm.set_instance_ref(this, declaring, field, list);
            write_int_field(vm, this, "index", 0);
            void()
        },
    ),
    method("read", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        loop {
            let current = sequence_current(vm, this)?;
            if current.is_null() {
                return int(-1);
            }
            let value = stream_read_byte(vm, current)?;
            if value >= 0 {
                return int(value);
            }
            sequence_advance(vm, this)?;
        }
    }),
    method("read", "([BII)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let dst = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        if dst.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        if length == 0 {
            return int(0);
        }
        loop {
            let current = sequence_current(vm, this)?;
            if current.is_null() {
                return int(-1);
            }
            let (class, method) = vm.resolve_virtual_method(current, "read", "([BII)I")?;
            let read = vm
                .invoke_method(
                    class,
                    method,
                    vec![
                        Value::Ref(current),
                        Value::Ref(dst),
                        Value::Int(offset),
                        Value::Int(length),
                    ],
                )?
                .as_int();
            if read >= 0 {
                return int(read);
            }
            sequence_advance(vm, this)?;
        }
    }),
    method("available", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let current = sequence_current(vm, this)?;
        if current.is_null() {
            return int(0);
        }
        let (class, method) = vm.resolve_virtual_method(current, "available", "()I")?;
        vm.invoke_method(class, method, vec![Value::Ref(current)])
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let streams = sequence_streams(vm, this);
        let (class, method) = vm.resolve_virtual_method(streams, "size", "()I")?;
        let size = vm
            .invoke_method(class, method, vec![Value::Ref(streams)])?
            .as_int();
        let start = read_int_field(vm, this, "index");
        // Every component stream is closed; the first failure is thrown and
        // the rest are attached to it as suppressed exceptions.
        let mut primary: Option<ObjectRef> = None;
        for index in start..size {
            let (class, method) =
                vm.resolve_virtual_method(streams, "get", "(I)Ljava/lang/Object;")?;
            let stream = vm
                .invoke_method(class, method, vec![Value::Ref(streams), Value::Int(index)])?
                .as_ref();
            if stream.is_null() {
                continue;
            }
            let (class, method) = vm.resolve_virtual_method(stream, "close", "()V")?;
            match vm.invoke_method(class, method, vec![Value::Ref(stream)]) {
                Ok(_) => {}
                Err(VmError::Thrown(exception)) => match primary {
                    None => primary = Some(exception),
                    Some(first) if first != exception => {
                        let (class, method) = vm.resolve_virtual_method(
                            first,
                            "addSuppressed",
                            "(Ljava/lang/Throwable;)V",
                        )?;
                        vm.invoke_method(
                            class,
                            method,
                            vec![Value::Ref(first), Value::Ref(exception)],
                        )?;
                    }
                    _ => {}
                },
                Err(other) => return Err(other),
            }
        }
        write_int_field(vm, this, "index", size);
        if let Some(exception) = primary {
            return Err(VmError::Thrown(exception));
        }
        void()
    }),
];

pub(crate) const SEQUENCE_INPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/io/SequenceInputStream",
    Some("java/io/InputStream"),
    &[],
    ACC_PUBLIC,
    &[
        field("streams", "Ljava/util/ArrayList;", ACC_PRIVATE, None),
        field("index", "I", ACC_PRIVATE, None),
    ],
    &SEQUENCE_INPUT_METHODS,
    None,
);

pub(crate) const BYTE_ARRAY_INPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/io/ByteArrayInputStream",
    Some("java/io/InputStream"),
    &[],
    ACC_PUBLIC,
    &[
        field("buf", "[B", ACC_PROTECTED, None),
        field("pos", "I", ACC_PROTECTED, None),
        field("count", "I", ACC_PROTECTED, None),
        field("mark", "I", ACC_PROTECTED, None),
    ],
    &BYTE_ARRAY_INPUT_METHODS,
    None,
);

const BYTE_ARRAY_OUTPUT_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
    method("<init>", "()V", ACC_PUBLIC, byte_array_output_init),
    method("<init>", "(I)V", ACC_PUBLIC, byte_array_output_init),
    method("write", "(I)V", ACC_PUBLIC, byte_array_output_write),
    method("write", "([BII)V", ACC_PUBLIC, |vm, ctx, args| {
        let array = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        reader_check_range(vm, array, offset, length)?;
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
        let count = read_int_field(vm, this, "count");
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
        int(read_int_field(vm, receiver(args), "count"))
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
            let name = crate::string_arg(vm, args[1])?;
            let text = match crate::charset::canonical_name(&name) {
                Some(canonical) => {
                    String::from_utf16_lossy(&crate::charset::decode(&bytes, canonical))
                }
                None => String::from_utf8_lossy(&bytes).into_owned(),
            };
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toString",
        "(I)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let bytes = byte_array_output_bytes(vm, receiver(args))?;
            let hibyte = (int_arg(args, 1) & 0xff) as u16;
            let units: Vec<u16> = bytes
                .iter()
                .map(|byte| (hibyte << 8) | u16::from(*byte))
                .collect();
            object(vm.make_string(&String::from_utf16_lossy(&units))?)
        },
    ),
    method(
        "toString",
        "(Ljava/nio/charset/Charset;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let bytes = byte_array_output_bytes(vm, receiver(args))?;
            let name = crate::charset::name_of(vm, ref_arg(args, 1));
            let text = match crate::charset::canonical_name(&name) {
                Some(canonical) => {
                    String::from_utf16_lossy(&crate::charset::decode(&bytes, canonical))
                }
                None => String::from_utf8_lossy(&bytes).into_owned(),
            };
            object(vm.make_string(&text)?)
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
    let count = read_int_field(vm, this, "count").max(0);
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
    let count = read_int_field(vm, this, "count");
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
    write_int_field(vm, this, "count", count + 1);
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

fn reader_read_chars(
    vm: &mut Vm<'_>,
    args: &[Value],
    offset: i32,
    length: Option<i32>,
) -> Result<Value, VmError> {
    let this = receiver(args);
    let array = ref_arg(args, 1);
    if array.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let capacity = vm.array_length(array)? as i32;
    let length = length.unwrap_or(capacity - offset);
    let end = i64::from(offset) + i64::from(length);
    if offset < 0 || length < 0 || end > i64::from(capacity) {
        return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
    }
    if length == 0 {
        return int(0);
    }
    let mut count = 0;
    while count < length {
        let (class, method) = vm.resolve_virtual_method(this, "read", "()I")?;
        let value = vm.invoke_method(class, method, vec![Value::Ref(this)])?;
        let unit = value.as_int();
        if unit < 0 {
            break;
        }
        vm.array_set(array, offset + count, Value::Int(unit), false)?;
        count += 1;
    }
    int(if count == 0 && length > 0 { -1 } else { count })
}

fn reader_check_range(
    vm: &mut Vm<'_>,
    array: ObjectRef,
    offset: i32,
    length: i32,
) -> Result<(), VmError> {
    if array.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let capacity = vm.array_length(array)? as i32;
    let end = i64::from(offset) + i64::from(length);
    if offset < 0 || length < 0 || end > i64::from(capacity) {
        return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
    }
    Ok(())
}

const READER_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        if let Some((declaring, field)) = vm.find_field_opt(class, "lock", "Ljava/lang/Object;") {
            vm.set_instance_ref(this, declaring, field, this);
        }
        void()
    }),
    method(
        "read",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method("read", "([C)I", ACC_PUBLIC, |vm, _, args| {
        reader_read_chars(vm, args, 0, None)
    }),
    method("read", "([CII)I", ACC_PUBLIC, |vm, _, args| {
        reader_read_chars(vm, args, int_arg(args, 2), Some(int_arg(args, 3)))
    }),
    method(
        "read",
        "(Ljava/nio/CharBuffer;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let target = ref_arg(args, 1);
            if target.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let (read_only_class, read_only_method) =
                vm.resolve_virtual_method(target, "isReadOnly", "()Z")?;
            if vm
                .invoke_method(read_only_class, read_only_method, vec![Value::Ref(target)])?
                .as_int()
                != 0
            {
                return Err(vm.throw_new("java/nio/ReadOnlyBufferException", None));
            }
            let (has_array_class, has_array_method) =
                vm.resolve_virtual_method(target, "hasArray", "()Z")?;
            let has_array = vm
                .invoke_method(has_array_class, has_array_method, vec![Value::Ref(target)])?
                .as_int()
                != 0;
            if has_array {
                let (array_class, array_method) =
                    vm.resolve_virtual_method(target, "array", "()[C")?;
                let cbuf = vm
                    .invoke_method(array_class, array_method, vec![Value::Ref(target)])?
                    .as_ref();
                let (position_class, position_method) =
                    vm.resolve_virtual_method(target, "position", "()I")?;
                let pos = vm
                    .invoke_method(position_class, position_method, vec![Value::Ref(target)])?
                    .as_int();
                let (limit_class, limit_method) =
                    vm.resolve_virtual_method(target, "limit", "()I")?;
                let limit = vm
                    .invoke_method(limit_class, limit_method, vec![Value::Ref(target)])?
                    .as_int();
                let rem = (limit - pos).max(0);
                let (offset_class, offset_method) =
                    vm.resolve_virtual_method(target, "arrayOffset", "()I")?;
                let array_offset = vm
                    .invoke_method(offset_class, offset_method, vec![Value::Ref(target)])?
                    .as_int();
                let (read_class, read_method) =
                    vm.resolve_virtual_method(this, "read", "([CII)I")?;
                let nread = vm
                    .invoke_method(
                        read_class,
                        read_method,
                        vec![
                            Value::Ref(this),
                            Value::Ref(cbuf),
                            Value::Int(array_offset + pos),
                            Value::Int(rem),
                        ],
                    )?
                    .as_int();
                if nread > 0 {
                    let (set_position_class, set_position_method) =
                        vm.resolve_virtual_method(target, "position", "(I)Ljava/nio/CharBuffer;")?;
                    vm.invoke_method(
                        set_position_class,
                        set_position_method,
                        vec![Value::Ref(target), Value::Int(pos + nread)],
                    )?;
                }
                int(nread)
            } else {
                let (remaining_class, remaining_method) =
                    vm.resolve_virtual_method(target, "remaining", "()I")?;
                let len = vm
                    .invoke_method(remaining_class, remaining_method, vec![Value::Ref(target)])?
                    .as_int();
                let cbuf = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Char, len as usize)?;
                let (read_class, read_method) =
                    vm.resolve_virtual_method(this, "read", "([CII)I")?;
                let nread = vm
                    .invoke_method(
                        read_class,
                        read_method,
                        vec![
                            Value::Ref(this),
                            Value::Ref(cbuf),
                            Value::Int(0),
                            Value::Int(len),
                        ],
                    )?
                    .as_int();
                if nread > 0 {
                    let (put_class, put_method) =
                        vm.resolve_virtual_method(target, "put", "([CII)Ljava/nio/CharBuffer;")?;
                    vm.invoke_method(
                        put_class,
                        put_method,
                        vec![
                            Value::Ref(target),
                            Value::Ref(cbuf),
                            Value::Int(0),
                            Value::Int(nread),
                        ],
                    )?;
                }
                int(nread)
            }
        },
    ),
    method("skip", "(J)J", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let remaining = crate::long_arg(args, 1);
        if remaining < 0 {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("skip value is negative"),
            ));
        }
        if remaining == 0 {
            return long(0);
        }
        let chunk = remaining.min(8192) as usize;
        let buffer = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Char, chunk)?;
        let (class, method) = vm.resolve_virtual_method(this, "read", "([CII)I")?;
        let mut skipped = 0i64;
        while skipped < remaining {
            let want = (remaining - skipped).min(chunk as i64) as i32;
            let count = vm
                .invoke_method(
                    class,
                    method,
                    vec![
                        Value::Ref(this),
                        Value::Ref(buffer),
                        Value::Int(0),
                        Value::Int(want),
                    ],
                )?
                .as_int();
            if count <= 0 {
                break;
            }
            skipped += i64::from(count);
        }
        long(skipped)
    }),
    method("ready", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("markSupported", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("mark", "(I)V", ACC_PUBLIC, |_, _, _| void()),
    method("reset", "()V", ACC_PUBLIC, |vm, _, _| {
        Err(vm.throw_new("java/io/IOException", Some("mark/reset not supported")))
    }),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "of",
        "(Ljava/lang/CharSequence;)Ljava/io/Reader;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let sequence = ref_arg(args, 0);
            if sequence.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let text = if vm.string_utf16(sequence).is_some() {
                sequence
            } else {
                let (class, method) =
                    vm.resolve_virtual_method(sequence, "toString", "()Ljava/lang/String;")?;
                vm.invoke_method(class, method, vec![Value::Ref(sequence)])?
                    .as_ref()
            };
            let class = vm.resolve_class("java/io/StringReader")?;
            let instance = vm.new_instance(class)?;
            let (declaring, ctor) = vm
                .find_method(class, "<init>", "(Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("StringReader lacks its constructor"))?;
            vm.invoke_method(
                declaring,
                ctor,
                vec![Value::Ref(instance), Value::Ref(text)],
            )?;
            object(instance)
        },
    ),
    method(
        "nullReader",
        "()Ljava/io/Reader;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/io/Reader$NullReader")?;
            object(vm.new_instance(class)?)
        },
    ),
    method(
        "transferTo",
        "(Ljava/io/Writer;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let out = ref_arg(args, 1);
            if out.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("out")));
            }
            let buffer = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Char, 8192)?;
            let mut total = 0i64;
            loop {
                let (read_class, read_method) =
                    vm.resolve_virtual_method(this, "read", "([CII)I")?;
                let count = vm
                    .invoke_method(
                        read_class,
                        read_method,
                        vec![
                            Value::Ref(this),
                            Value::Ref(buffer),
                            Value::Int(0),
                            Value::Int(8192),
                        ],
                    )?
                    .as_int();
                if count < 0 {
                    break;
                }
                let (write_class, write_method) =
                    vm.resolve_virtual_method(out, "write", "([CII)V")?;
                vm.invoke_method(
                    write_class,
                    write_method,
                    vec![
                        Value::Ref(out),
                        Value::Ref(buffer),
                        Value::Int(0),
                        Value::Int(count),
                    ],
                )?;
                total += i64::from(count);
            }
            long(total)
        },
    ),
];

pub(crate) const READER: oxjvm_vm::NativeClass = class(
    "java/io/Reader",
    Some("java/lang/Object"),
    &["java/io/Closeable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[field("lock", "Ljava/lang/Object;", ACC_PROTECTED, None)],
    &READER_METHODS,
    None,
);

const WRITER_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
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
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let csq = ref_arg(args, 1);
            if csq.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let (tc, tm) = vm.resolve_virtual_method(csq, "toString", "()Ljava/lang/String;")?;
            let text = vm.invoke_method(tc, tm, vec![Value::Ref(csq)])?.as_ref();
            let (wc, wm) = vm.resolve_virtual_method(this, "write", "(Ljava/lang/String;)V")?;
            vm.invoke_method(wc, wm, vec![Value::Ref(this), Value::Ref(text)])?;
            object(this)
        },
    ),
    method(
        "append",
        "(Ljava/lang/CharSequence;II)Ljava/io/Writer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let csq = ref_arg(args, 1);
            if csq.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let (sc, sm) =
                vm.resolve_virtual_method(csq, "subSequence", "(II)Ljava/lang/CharSequence;")?;
            let sub = vm
                .invoke_method(
                    sc,
                    sm,
                    vec![
                        Value::Ref(csq),
                        Value::Int(int_arg(args, 2)),
                        Value::Int(int_arg(args, 3)),
                    ],
                )?
                .as_ref();
            let (tc, tm) = vm.resolve_virtual_method(sub, "toString", "()Ljava/lang/String;")?;
            let text = vm.invoke_method(tc, tm, vec![Value::Ref(sub)])?.as_ref();
            let (wc, wm) = vm.resolve_virtual_method(this, "write", "(Ljava/lang/String;)V")?;
            vm.invoke_method(wc, wm, vec![Value::Ref(this), Value::Ref(text)])?;
            object(this)
        },
    ),
    method(
        "append",
        "(C)Ljava/io/Writer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (wc, wm) = vm.resolve_virtual_method(this, "write", "(I)V")?;
            vm.invoke_method(wc, wm, vec![Value::Ref(this), Value::Int(int_arg(args, 1))])?;
            object(this)
        },
    ),
    method(
        "nullWriter",
        "()Ljava/io/Writer;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/io/Writer$NullWriter")?;
            object(vm.new_instance(class)?)
        },
    ),
    method("write", "([C)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let array = ref_arg(args, 1);
        let length = vm.array_length(array)? as i32;
        let (declaring, method) = vm.resolve_virtual_method(this, "write", "([CII)V")?;
        vm.invoke_method(
            declaring,
            method,
            vec![
                Value::Ref(this),
                Value::Ref(array),
                Value::Int(0),
                Value::Int(length),
            ],
        )?;
        void()
    }),
    method("write", "([CII)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let array = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        reader_check_range(vm, array, offset, length)?;
        for index in offset..offset + length {
            let unit = vm.array_get(array, index)?.as_int();
            let (declaring, method) = vm.resolve_virtual_method(this, "write", "(I)V")?;
            vm.invoke_method(declaring, method, vec![Value::Ref(this), Value::Int(unit)])?;
        }
        void()
    }),
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

// -------------------------------------------------------------------------------------------
// Null streams: InputStream.nullInputStream / OutputStream.nullOutputStream /
// Reader.nullReader / Writer.nullWriter. Each discards or supplies nothing and
// throws IOException once closed.
// -------------------------------------------------------------------------------------------

fn null_stream_ensure_open(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    if read_int_field(vm, this, "closed") != 0 {
        return Err(vm.throw_new("java/io/IOException", Some("Stream closed")));
    }
    Ok(())
}

fn null_stream_close(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Value, VmError> {
    write_int_field(vm, this, "closed", 1);
    void()
}

const NULL_INPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method("read", "()I", ACC_PUBLIC, |vm, _, args| {
        null_stream_ensure_open(vm, receiver(args))?;
        int(-1)
    }),
    method("read", "([BII)I", ACC_PUBLIC, |vm, _, args| {
        null_stream_ensure_open(vm, receiver(args))?;
        reader_check_range(vm, ref_arg(args, 1), int_arg(args, 2), int_arg(args, 3))?;
        if int_arg(args, 3) == 0 {
            return int(0);
        }
        int(-1)
    }),
    method("available", "()I", ACC_PUBLIC, |vm, _, args| {
        null_stream_ensure_open(vm, receiver(args))?;
        int(0)
    }),
    method("skip", "(J)J", ACC_PUBLIC, |vm, _, args| {
        null_stream_ensure_open(vm, receiver(args))?;
        long(0)
    }),
    method("skipNBytes", "(J)V", ACC_PUBLIC, |vm, _, args| {
        null_stream_ensure_open(vm, receiver(args))?;
        if crate::long_arg(args, 1) > 0 {
            return Err(vm.throw_new("java/io/EOFException", None));
        }
        void()
    }),
    method("readNBytes", "(I)[B", ACC_PUBLIC, |vm, _, args| {
        null_stream_ensure_open(vm, receiver(args))?;
        if int_arg(args, 1) < 0 {
            return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
        }
        object(write_bytes_array(vm, &[])?)
    }),
    method(
        "transferTo",
        "(Ljava/io/OutputStream;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("out")));
            }
            null_stream_ensure_open(vm, receiver(args))?;
            long(0)
        },
    ),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        null_stream_close(vm, receiver(args))
    }),
];

pub(crate) const NULL_INPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/io/InputStream$NullInputStream",
    Some("java/io/InputStream"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field("closed", "I", ACC_PRIVATE, None)],
    &NULL_INPUT_STREAM_METHODS,
    None,
);

const NULL_OUTPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method("write", "(I)V", ACC_PUBLIC, |vm, _, args| {
        null_stream_ensure_open(vm, receiver(args))?;
        void()
    }),
    method("write", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        null_stream_ensure_open(vm, receiver(args))?;
        reader_check_range(vm, ref_arg(args, 1), int_arg(args, 2), int_arg(args, 3))?;
        void()
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        null_stream_close(vm, receiver(args))
    }),
];

pub(crate) const NULL_OUTPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/io/OutputStream$NullOutputStream",
    Some("java/io/OutputStream"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field("closed", "I", ACC_PRIVATE, None)],
    &NULL_OUTPUT_STREAM_METHODS,
    None,
);

const NULL_READER_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method("read", "()I", ACC_PUBLIC, |vm, _, args| {
        null_stream_ensure_open(vm, receiver(args))?;
        int(-1)
    }),
    method("read", "([CII)I", ACC_PUBLIC, |vm, _, args| {
        null_stream_ensure_open(vm, receiver(args))?;
        reader_check_range(vm, ref_arg(args, 1), int_arg(args, 2), int_arg(args, 3))?;
        if int_arg(args, 3) == 0 {
            return int(0);
        }
        int(-1)
    }),
    method(
        "read",
        "(Ljava/nio/CharBuffer;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            null_stream_ensure_open(vm, this)?;
            let target = ref_arg(args, 1);
            let (class, method) = vm.resolve_virtual_method(target, "remaining", "()I")?;
            let remaining = vm
                .invoke_method(class, method, vec![Value::Ref(target)])?
                .as_int();
            int(if remaining <= 0 { 0 } else { -1 })
        },
    ),
    method("ready", "()Z", ACC_PUBLIC, |vm, _, args| {
        null_stream_ensure_open(vm, receiver(args))?;
        boolean(false)
    }),
    method("skip", "(J)J", ACC_PUBLIC, |vm, _, args| {
        null_stream_ensure_open(vm, receiver(args))?;
        long(0)
    }),
    method(
        "transferTo",
        "(Ljava/io/Writer;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("out")));
            }
            null_stream_ensure_open(vm, receiver(args))?;
            long(0)
        },
    ),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        null_stream_close(vm, receiver(args))
    }),
];

pub(crate) const NULL_READER: oxjvm_vm::NativeClass = class(
    "java/io/Reader$NullReader",
    Some("java/io/Reader"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field("closed", "I", ACC_PRIVATE, None)],
    &NULL_READER_METHODS,
    None,
);

const NULL_WRITER_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method("write", "(I)V", ACC_PUBLIC, |vm, _, args| {
        null_stream_ensure_open(vm, receiver(args))?;
        void()
    }),
    method(
        "write",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            null_stream_ensure_open(vm, receiver(args))?;
            void()
        },
    ),
    method(
        "write",
        "(Ljava/lang/String;II)V",
        ACC_PUBLIC,
        |vm, _, args| {
            null_stream_ensure_open(vm, receiver(args))?;
            void()
        },
    ),
    method("write", "([CII)V", ACC_PUBLIC, |vm, _, args| {
        null_stream_ensure_open(vm, receiver(args))?;
        reader_check_range(vm, ref_arg(args, 1), int_arg(args, 2), int_arg(args, 3))?;
        void()
    }),
    method(
        "append",
        "(C)Ljava/io/Writer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            null_stream_ensure_open(vm, this)?;
            object(this)
        },
    ),
    method(
        "append",
        "(Ljava/lang/CharSequence;)Ljava/io/Writer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            null_stream_ensure_open(vm, this)?;
            object(this)
        },
    ),
    method(
        "append",
        "(Ljava/lang/CharSequence;II)Ljava/io/Writer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            null_stream_ensure_open(vm, this)?;
            object(this)
        },
    ),
    method("flush", "()V", ACC_PUBLIC, |vm, _, args| {
        null_stream_ensure_open(vm, receiver(args))?;
        void()
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        null_stream_close(vm, receiver(args))
    }),
];

pub(crate) const NULL_WRITER: oxjvm_vm::NativeClass = class(
    "java/io/Writer$NullWriter",
    Some("java/io/Writer"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field("closed", "I", ACC_PRIVATE, None)],
    &NULL_WRITER_METHODS,
    None,
);

const PRINT_WRITER_METHODS: [oxjvm_vm::NativeMethodDef; 36] = [
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
            write_int_field(vm, this, "autoFlush", i32::from(crate::bool_arg(args, 2)));
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
        let this = receiver(args);
        let text = if crate::bool_arg(args, 1) {
            "true"
        } else {
            "false"
        };
        print_writer_text(vm, this, text)?;
        let (class, method) = vm.resolve_virtual_method(this, "println", "()V")?;
        vm.invoke_method(class, method, vec![Value::Ref(this)])
    }),
    method("println", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        print_writer_text(vm, this, &format::int_to_string(int_arg(args, 1)))?;
        let (class, method) = vm.resolve_virtual_method(this, "println", "()V")?;
        vm.invoke_method(class, method, vec![Value::Ref(this)])
    }),
    method("println", "(C)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let unit = int_arg(args, 1) as u32 & 0xFFFF;
        let text = char::from_u32(unit).map_or_else(String::new, |ch| ch.to_string());
        print_writer_text(vm, this, &text)?;
        let (class, method) = vm.resolve_virtual_method(this, "println", "()V")?;
        vm.invoke_method(class, method, vec![Value::Ref(this)])
    }),
    method(
        "println",
        "(Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = vm.display_value(
                &oxjvm_classfile::FieldType::Object("java/lang/Object".into()),
                args[1],
            )?;
            print_writer_text(vm, this, &text)?;
            let (class, method) = vm.resolve_virtual_method(this, "println", "()V")?;
            vm.invoke_method(class, method, vec![Value::Ref(this)])
        },
    ),
    method("checkError", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        print_writer_flush(vm, this)?;
        boolean(read_int_field(vm, this, "trouble") != 0)
    }),
    method("setError", "()V", ACC_PROTECTED, |vm, _, args| {
        write_int_field(vm, receiver(args), "trouble", 1);
        void()
    }),
    method("clearError", "()V", ACC_PROTECTED, |vm, _, args| {
        write_int_field(vm, receiver(args), "trouble", 0);
        void()
    }),
    method(
        "println",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = vm.string_or_empty(ref_arg(args, 1));
            print_writer_text(vm, this, &text)?;
            let (class, method) = vm.resolve_virtual_method(this, "println", "()V")?;
            vm.invoke_method(class, method, vec![Value::Ref(this)])
        },
    ),
    method("println", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        print_writer_text(vm, this, "\n")?;
        if read_int_field(vm, this, "autoFlush") != 0 {
            print_writer_flush(vm, this)?;
        }
        void()
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
    method("flush", "()V", ACC_PUBLIC, |vm, _, args| {
        print_writer_flush(vm, receiver(args))?;
        void()
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if read_int_field(vm, this, "closed") != 0 {
            return void();
        }
        write_int_field(vm, this, "closed", 1);
        print_writer_flush(vm, this)?;
        let out = vm
            .read_ref_field(this, vm.class_of(this), "out", "Ljava/io/Writer;")
            .unwrap_or(ObjectRef::NULL);
        if !out.is_null() {
            let (class, method) = vm.resolve_virtual_method(out, "close", "()V")?;
            match vm.invoke_method(class, method, vec![Value::Ref(out)]) {
                Ok(_) => {}
                Err(VmError::Thrown(exception)) => {
                    let io_exception = vm.resolve_class("java/io/IOException")?;
                    if vm.is_instance(exception, io_exception) {
                        write_int_field(vm, this, "trouble", 1);
                    } else {
                        return Err(VmError::Thrown(exception));
                    }
                }
                Err(other) => return Err(other),
            }
        }
        void()
    }),
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
    let text = crate::lang::format_string(vm, &format, &arguments, "")?;
    print_writer_text(vm, writer, &text)?;
    object(writer)
}

fn print_writer_flush(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    let out = vm
        .read_ref_field(this, vm.class_of(this), "out", "Ljava/io/Writer;")
        .unwrap_or(ObjectRef::NULL);
    if out.is_null() {
        return Ok(());
    }
    let (class, method) = vm.resolve_virtual_method(out, "flush", "()V")?;
    match vm.invoke_method(class, method, vec![Value::Ref(out)]) {
        Ok(_) => Ok(()),
        Err(VmError::Thrown(exception)) => {
            let io_exception = vm.resolve_class("java/io/IOException")?;
            if vm.is_instance(exception, io_exception) {
                write_int_field(vm, this, "trouble", 1);
                Ok(())
            } else {
                Err(VmError::Thrown(exception))
            }
        }
        Err(other) => Err(other),
    }
}

fn print_writer_text(vm: &mut Vm<'_>, this: ObjectRef, text: &str) -> Result<Value, VmError> {
    if read_int_field(vm, this, "closed") != 0 {
        write_int_field(vm, this, "trouble", 1);
        return void();
    }
    let out = vm
        .read_ref_field(this, vm.class_of(this), "out", "Ljava/io/Writer;")
        .unwrap_or(ObjectRef::NULL);
    if out.is_null() {
        return void();
    }
    // Fall back to character-by-character writes through the abstract `write(int)`.
    for ch in text.chars() {
        let (declaring, method) = vm.resolve_virtual_method(out, "write", "(I)V")?;
        match vm.invoke_method(
            declaring,
            method,
            vec![Value::Ref(out), Value::Int(ch as i32)],
        ) {
            Ok(_) => {}
            Err(VmError::Thrown(exception)) => {
                let io_exception = vm.resolve_class("java/io/IOException")?;
                if vm.is_instance(exception, io_exception) {
                    write_int_field(vm, this, "trouble", 1);
                    return void();
                }
                return Err(VmError::Thrown(exception));
            }
            Err(other) => return Err(other),
        }
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
    &[
        field("out", "Ljava/io/Writer;", ACC_PROTECTED, None),
        field("autoFlush", "I", ACC_PRIVATE, None),
        field("trouble", "I", ACC_PRIVATE, None),
        field("closed", "I", ACC_PRIVATE, None),
    ],
    &PRINT_WRITER_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// StringWriter
// -------------------------------------------------------------------------------------------

const STRING_WRITER_METHODS: [oxjvm_vm::NativeMethodDef; 13] = [
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
    method(
        "append",
        "(Ljava/lang/CharSequence;)Ljava/io/StringWriter;",
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
        "append",
        "(C)Ljava/io/StringWriter;",
        ACC_PUBLIC,
        |vm, _, args| {
            let ch = int_arg(args, 1) as u16;
            let text = vm.make_string_utf16(vec![ch])?;
            string_writer_append(vm, receiver(args), text)?;
            object(receiver(args))
        },
    ),
    method(
        "append",
        "(Ljava/lang/CharSequence;II)Ljava/io/StringWriter;",
        ACC_PUBLIC,
        |vm, _, args| {
            let mut sequence = ref_arg(args, 1);
            if sequence.is_null() {
                sequence = vm.make_string("null")?;
            }
            let (class, method) =
                vm.resolve_virtual_method(sequence, "subSequence", "(II)Ljava/lang/CharSequence;")?;
            let sub = vm.invoke_method(
                class,
                method,
                vec![
                    Value::Ref(sequence),
                    Value::Int(int_arg(args, 2)),
                    Value::Int(int_arg(args, 3)),
                ],
            )?;
            let (class, method) =
                vm.resolve_virtual_method(sub.as_ref(), "toString", "()Ljava/lang/String;")?;
            let text = vm.invoke_method(class, method, vec![sub])?;
            string_writer_append(vm, receiver(args), text.as_ref())?;
            object(receiver(args))
        },
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
    let append = read_int_field(vm, this, "append") != 0;
    let written = vm
        .host
        .filesystem()
        .map(|fs| fs.write(&path, text.as_bytes(), append))
        .unwrap_or(false);
    if !written {
        return Err(vm.throw_new("java/io/IOException", Some(&path)));
    }
    write_int_field(vm, this, "append", 1);
    void()
}

const FILE_WRITER_METHODS: [oxjvm_vm::NativeMethodDef; 16] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let path = crate::string_arg(vm, args[1])?;
            vm.set_string_field(this, vm.class_of(this), "path", &path)?;
            write_int_field(vm, this, "append", 0);
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
            write_int_field(vm, this, "append", i32::from(crate::bool_arg(args, 2)));
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
        write_int_field(vm, this, "append", 0);
        void()
    }),
    method("<init>", "(Ljava/io/File;Z)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let file = ref_arg(args, 1);
        let path = vm
            .read_string_field(file, vm.class_of(file), "path")
            .unwrap_or_default();
        vm.set_string_field(this, vm.class_of(this), "path", &path)?;
        write_int_field(vm, this, "append", i32::from(crate::bool_arg(args, 2)));
        void()
    }),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/nio/charset/Charset;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let path = crate::string_arg(vm, args[1])?;
            vm.set_string_field(this, vm.class_of(this), "path", &path)?;
            write_int_field(vm, this, "append", 0);
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/File;Ljava/nio/charset/Charset;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let file = ref_arg(args, 1);
            let path = vm
                .read_string_field(file, vm.class_of(file), "path")
                .unwrap_or_default();
            vm.set_string_field(this, vm.class_of(this), "path", &path)?;
            write_int_field(vm, this, "append", 0);
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/nio/charset/Charset;Z)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let path = crate::string_arg(vm, args[1])?;
            vm.set_string_field(this, vm.class_of(this), "path", &path)?;
            write_int_field(vm, this, "append", i32::from(crate::bool_arg(args, 3)));
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/File;Ljava/nio/charset/Charset;Z)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let file = ref_arg(args, 1);
            let path = vm
                .read_string_field(file, vm.class_of(file), "path")
                .unwrap_or_default();
            vm.set_string_field(this, vm.class_of(this), "path", &path)?;
            write_int_field(vm, this, "append", i32::from(crate::bool_arg(args, 3)));
            void()
        },
    ),
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
    let canonical = vm
        .read_string_field(this, vm.class_of(this), "charset")
        .unwrap_or_default();
    let bytes = if canonical.is_empty() {
        text.as_bytes().to_vec()
    } else {
        crate::charset::encode(text, &canonical)
    };
    let array = io_bytes_array(vm, &bytes)?;
    let (class, method) = vm.resolve_virtual_method(out, "write", "([B)V")?;
    vm.invoke_method(class, method, vec![Value::Ref(out), Value::Ref(array)])?;
    void()
}

fn output_stream_writer_init(
    vm: &mut Vm<'_>,
    args: &[Value],
    canonical: &str,
) -> Result<Value, VmError> {
    let this = receiver(args);
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "out", "Ljava/io/OutputStream;")?;
    vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
    let charset = vm.make_string(canonical)?;
    let (declaring, field) = vm.find_field(class, "charset", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, charset);
    let encoding = vm.make_string(historical_encoding(canonical))?;
    let (declaring, field) = vm.find_field(class, "encoding", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, encoding);
    void()
}

const OUTPUT_STREAM_WRITER_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method(
        "<init>",
        "(Ljava/io/OutputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| output_stream_writer_init(vm, args, "UTF-8"),
    ),
    method(
        "<init>",
        "(Ljava/io/OutputStream;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = crate::string_arg(vm, args[2])?;
            match crate::charset::canonical_name(&name) {
                Some(canonical) => output_stream_writer_init(vm, args, canonical),
                None => Err(vm.throw_new("java/io/UnsupportedEncodingException", Some(&name))),
            }
        },
    ),
    method(
        "<init>",
        "(Ljava/io/OutputStream;Ljava/nio/charset/Charset;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let canonical = input_stream_reader_charset(vm, ref_arg(args, 2))?;
            output_stream_writer_init(vm, args, canonical)
        },
    ),
    method(
        "<init>",
        "(Ljava/io/OutputStream;Ljava/nio/charset/CharsetEncoder;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let encoder = ref_arg(args, 2);
            if encoder.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let (class, method) =
                vm.resolve_virtual_method(encoder, "charset", "()Ljava/nio/charset/Charset;")?;
            let charset = vm
                .invoke_method(class, method, vec![Value::Ref(encoder)])?
                .as_ref();
            let canonical = input_stream_reader_charset(vm, charset)?;
            output_stream_writer_init(vm, args, canonical)
        },
    ),
    method(
        "getEncoding",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let encoding = vm
                .read_ref_field(this, vm.class_of(this), "encoding", "Ljava/lang/String;")
                .unwrap_or(ObjectRef::NULL);
            object(encoding)
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
    &[
        field("out", "Ljava/io/OutputStream;", ACC_PRIVATE, None),
        field("charset", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("encoding", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
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

fn file_reader_init(vm: &mut Vm<'_>, args: &[Value], canonical: &str) -> Result<Value, VmError> {
    let this = receiver(args);
    let path = file_reader_path(vm, this);
    let bytes = vm
        .host
        .filesystem()
        .and_then(|fs| fs.read(&path))
        .unwrap_or_default();
    let content = String::from_utf16_lossy(&crate::charset::decode(&bytes, canonical));
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "content", "Ljava/lang/String;")?;
    let value = vm.make_string(&content)?;
    vm.set_instance_ref(this, declaring, field, value);
    write_int_field(vm, this, "pos", 0);
    let _ = path;
    void()
}

/// Resolve the charset argument of a `FileReader` constructor; a null charset is an NPE.
fn file_reader_charset(vm: &mut Vm<'_>, charset: ObjectRef) -> Result<&'static str, VmError> {
    if charset.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let name = crate::charset::name_of(vm, charset);
    Ok(crate::charset::canonical_name(&name).unwrap_or("UTF-8"))
}

const FILE_READER_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "path", "Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            file_reader_init(vm, args, "UTF-8")
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
        file_reader_init(vm, args, "UTF-8")
    }),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/nio/charset/Charset;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let canonical = file_reader_charset(vm, ref_arg(args, 2))?;
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "path", "Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            file_reader_init(vm, args, canonical)
        },
    ),
    method(
        "<init>",
        "(Ljava/io/File;Ljava/nio/charset/Charset;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let canonical = file_reader_charset(vm, ref_arg(args, 2))?;
            let file = ref_arg(args, 1);
            let path = vm
                .read_string_field(file, vm.class_of(file), "path")
                .unwrap_or_default();
            let path_value = vm.make_string(&path)?;
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "path", "Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, path_value);
            file_reader_init(vm, args, canonical)
        },
    ),
    method("read", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let content = file_reader_content(vm, this);
        let pos = read_int_field(vm, this, "pos");
        let units: Vec<u16> = content.encode_utf16().collect();
        if pos < 0 || pos as usize >= units.len() {
            return int(-1);
        }
        write_int_field(vm, this, "pos", pos + 1);
        int(i32::from(units[pos as usize]))
    }),
    method("read", "([CII)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let content = file_reader_content(vm, this);
        let units: Vec<u16> = content.encode_utf16().collect();
        let mut pos = read_int_field(vm, this, "pos");
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        let array = ref_arg(args, 1);
        reader_check_range(vm, array, offset, length)?;
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
        write_int_field(vm, this, "pos", pos);
        if count == 0 { int(-1) } else { int(count) }
    }),
    method("read", "([C)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let content = file_reader_content(vm, this);
        let units: Vec<u16> = content.encode_utf16().collect();
        let mut pos = read_int_field(vm, this, "pos");
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
        write_int_field(vm, this, "pos", pos);
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

fn buffered_reader_cb(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "cb", "[C")
        .unwrap_or(ObjectRef::NULL)
}

fn buffered_reader_ensure_open(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    if read_int_field(vm, this, "closed") != 0 {
        return Err(vm.throw_new("java/io/IOException", Some("Stream closed")));
    }
    Ok(())
}

fn buffered_reader_ctor(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    reader: ObjectRef,
    size: i32,
) -> Result<(), VmError> {
    if reader.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    if size <= 0 {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("Buffer size <= 0"),
        ));
    }
    let (declaring, field) = vm.find_field(vm.class_of(this), "in", "Ljava/io/Reader;")?;
    vm.set_instance_ref(this, declaring, field, reader);
    let cb = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Char, size as usize)?;
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "cb", "[C")?;
    vm.set_instance_ref(this, declaring, field, cb);
    write_int_field(vm, this, "markedChar", -1);
    Ok(())
}

/// Whether the underlying reader has input available without blocking.
fn buffered_reader_in_ready(vm: &mut Vm<'_>, this: ObjectRef) -> Result<bool, VmError> {
    let reader = buffered_reader_in(vm, this);
    if reader.is_null() {
        return Ok(false);
    }
    let (class, method) = vm.resolve_virtual_method(reader, "ready", "()Z")?;
    let value = vm.invoke_method(class, method, vec![Value::Ref(reader)])?;
    Ok(value.as_int() != 0)
}

/// Refill the internal buffer, honoring an active mark the way the JDK does:
/// compact marked data to the front (growing the buffer when the read-ahead
/// limit demands it) and retry reads that return zero.
fn buffered_reader_fill(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    let mut cb = buffered_reader_cb(vm, this);
    let mut n_chars = read_int_field(vm, this, "nChars");
    let mut next_char = read_int_field(vm, this, "nextChar");
    let marked = read_int_field(vm, this, "markedChar");
    let limit = read_int_field(vm, this, "readAheadLimit");
    let dst;
    if marked < 0 {
        dst = 0;
    } else {
        let delta = next_char - marked;
        if delta >= limit {
            write_int_field(vm, this, "markedChar", -2);
            write_int_field(vm, this, "readAheadLimit", 0);
            dst = 0;
        } else {
            let capacity = vm.array_length(cb)? as i32;
            if limit > capacity {
                let new_cb =
                    vm.allocate_array_of(oxjvm_vm::ArrayComponent::Char, limit as usize)?;
                for index in 0..delta {
                    let unit = vm.array_get(cb, marked + index)?;
                    vm.array_set(new_cb, index, unit, false)?;
                }
                cb = new_cb;
                let class = vm.class_of(this);
                let (declaring, field) = vm.find_field(class, "cb", "[C")?;
                vm.set_instance_ref(this, declaring, field, cb);
            } else if marked > 0 {
                for index in 0..delta {
                    let unit = vm.array_get(cb, marked + index)?;
                    vm.array_set(cb, index, unit, false)?;
                }
            }
            write_int_field(vm, this, "markedChar", 0);
            n_chars = delta;
            next_char = delta;
            dst = delta;
        }
    }
    let reader = buffered_reader_in(vm, this);
    if !reader.is_null() {
        let capacity = vm.array_length(cb)? as i32;
        let (class, method) = vm.resolve_virtual_method(reader, "read", "([CII)I")?;
        let count = loop {
            let n = vm
                .invoke_method(
                    class,
                    method,
                    vec![
                        Value::Ref(reader),
                        Value::Ref(cb),
                        Value::Int(dst),
                        Value::Int(capacity - dst),
                    ],
                )?
                .as_int();
            if n != 0 {
                break n;
            }
        };
        if count > 0 {
            n_chars = dst + count;
            next_char = dst;
        }
    }
    write_int_field(vm, this, "nChars", n_chars);
    write_int_field(vm, this, "nextChar", next_char);
    Ok(())
}

/// Single-character read over the internal buffer, skipping a `\n` left over
/// from a `\r\n` terminator when `skipLF` is set.
fn buffered_reader_read_char(vm: &mut Vm<'_>, this: ObjectRef) -> Result<i32, VmError> {
    buffered_reader_ensure_open(vm, this)?;
    loop {
        let mut next = read_int_field(vm, this, "nextChar");
        if next >= read_int_field(vm, this, "nChars") {
            buffered_reader_fill(vm, this)?;
            next = read_int_field(vm, this, "nextChar");
            if next >= read_int_field(vm, this, "nChars") {
                return Ok(-1);
            }
        }
        let cb = buffered_reader_cb(vm, this);
        if read_int_field(vm, this, "skipLF") != 0 {
            write_int_field(vm, this, "skipLF", 0);
            if vm.array_get(cb, next)?.as_int() == i32::from(b'\n') {
                write_int_field(vm, this, "nextChar", next + 1);
                continue;
            }
        }
        let unit = vm.array_get(cb, next)?.as_int();
        write_int_field(vm, this, "nextChar", next + 1);
        return Ok(unit);
    }
}

/// One underlying read into the caller's array, bypassing the buffer for
/// large unmarked requests exactly like the JDK's `read1`.
fn buffered_reader_read1(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    array: ObjectRef,
    off: i32,
    len: i32,
) -> Result<i32, VmError> {
    let mut next = read_int_field(vm, this, "nextChar");
    let mut n_chars = read_int_field(vm, this, "nChars");
    if next >= n_chars {
        let capacity = vm.array_length(buffered_reader_cb(vm, this))? as i32;
        if len >= capacity
            && read_int_field(vm, this, "markedChar") < 0
            && read_int_field(vm, this, "skipLF") == 0
        {
            let reader = buffered_reader_in(vm, this);
            if reader.is_null() {
                return Ok(-1);
            }
            let (class, method) = vm.resolve_virtual_method(reader, "read", "([CII)I")?;
            return Ok(vm
                .invoke_method(
                    class,
                    method,
                    vec![
                        Value::Ref(reader),
                        Value::Ref(array),
                        Value::Int(off),
                        Value::Int(len),
                    ],
                )?
                .as_int());
        }
        buffered_reader_fill(vm, this)?;
        next = read_int_field(vm, this, "nextChar");
        n_chars = read_int_field(vm, this, "nChars");
    }
    if next >= n_chars {
        return Ok(-1);
    }
    let cb = buffered_reader_cb(vm, this);
    if read_int_field(vm, this, "skipLF") != 0 {
        write_int_field(vm, this, "skipLF", 0);
        if vm.array_get(cb, next)?.as_int() == i32::from(b'\n') {
            next += 1;
            write_int_field(vm, this, "nextChar", next);
            if next >= n_chars {
                buffered_reader_fill(vm, this)?;
                next = read_int_field(vm, this, "nextChar");
                n_chars = read_int_field(vm, this, "nChars");
                if next >= n_chars {
                    return Ok(-1);
                }
            }
        }
    }
    let count = len.min(n_chars - next);
    for index in 0..count {
        let unit = vm.array_get(cb, next + index)?;
        vm.array_set(array, off + index, unit, false)?;
    }
    write_int_field(vm, this, "nextChar", next + count);
    Ok(count)
}

fn buffered_reader_read_chars(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    array: ObjectRef,
    off: i32,
    len: i32,
) -> Result<i32, VmError> {
    buffered_reader_ensure_open(vm, this)?;
    reader_check_range(vm, array, off, len)?;
    if len == 0 {
        return Ok(0);
    }
    let mut count = buffered_reader_read1(vm, this, array, off, len)?;
    if count <= 0 {
        return Ok(count);
    }
    let reader = buffered_reader_in(vm, this);
    while count < len && !reader.is_null() {
        let (ready_class, ready_method) = vm.resolve_virtual_method(reader, "ready", "()Z")?;
        let ready = vm
            .invoke_method(ready_class, ready_method, vec![Value::Ref(reader)])?
            .as_int()
            != 0;
        if !ready {
            break;
        }
        let more = buffered_reader_read1(vm, this, array, off + count, len - count)?;
        if more <= 0 {
            break;
        }
        count += more;
    }
    Ok(count)
}

/// Line-oriented read over the buffer; a trailing `\r` sets `skipLF` so the
/// next read consumes a paired `\n` without touching the underlying reader.
fn buffered_reader_read_line(vm: &mut Vm<'_>, this: ObjectRef) -> Result<ObjectRef, VmError> {
    buffered_reader_ensure_open(vm, this)?;
    let mut units: Vec<u16> = Vec::new();
    let mut omit_lf = read_int_field(vm, this, "skipLF") != 0;
    loop {
        let mut next = read_int_field(vm, this, "nextChar");
        let mut n_chars = read_int_field(vm, this, "nChars");
        if next >= n_chars {
            buffered_reader_fill(vm, this)?;
            next = read_int_field(vm, this, "nextChar");
            n_chars = read_int_field(vm, this, "nChars");
        }
        if next >= n_chars {
            if units.is_empty() {
                return Ok(ObjectRef::NULL);
            }
            return vm.make_string_utf16(units);
        }
        let cb = buffered_reader_cb(vm, this);
        if omit_lf && vm.array_get(cb, next)?.as_int() == i32::from(b'\n') {
            next += 1;
            write_int_field(vm, this, "nextChar", next);
        }
        write_int_field(vm, this, "skipLF", 0);
        omit_lf = false;
        let mut end = next;
        let mut terminator = 0;
        while end < n_chars {
            let unit = vm.array_get(cb, end)?.as_int();
            if unit == i32::from(b'\n') || unit == i32::from(b'\r') {
                terminator = unit;
                break;
            }
            end += 1;
        }
        for index in next..end {
            units.push(vm.array_get(cb, index)?.as_int() as u16);
        }
        write_int_field(vm, this, "nextChar", end);
        if terminator != 0 {
            write_int_field(vm, this, "nextChar", end + 1);
            if terminator == i32::from(b'\r') {
                write_int_field(vm, this, "skipLF", 1);
            }
            return vm.make_string_utf16(units);
        }
    }
}

const BUFFERED_READER_METHODS: [oxjvm_vm::NativeMethodDef; 13] = [
    method(
        "<init>",
        "(Ljava/io/Reader;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            buffered_reader_ctor(vm, receiver(args), ref_arg(args, 1), 8192)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/Reader;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            buffered_reader_ctor(vm, receiver(args), ref_arg(args, 1), int_arg(args, 2))?;
            void()
        },
    ),
    method("read", "()I", ACC_PUBLIC, |vm, _, args| {
        int(buffered_reader_read_char(vm, receiver(args))?)
    }),
    method("read", "([C)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let array = ref_arg(args, 1);
        if array.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        let length = vm.array_length(array)? as i32;
        // Dispatch through read(char[],int,int) so subclasses such as
        // LineNumberReader see every character that is read.
        let (class, method) = vm.resolve_virtual_method(this, "read", "([CII)I")?;
        vm.invoke_method(
            class,
            method,
            vec![
                Value::Ref(this),
                Value::Ref(array),
                Value::Int(0),
                Value::Int(length),
            ],
        )
    }),
    method("read", "([CII)I", ACC_PUBLIC, |vm, _, args| {
        int(buffered_reader_read_chars(
            vm,
            receiver(args),
            ref_arg(args, 1),
            int_arg(args, 2),
            int_arg(args, 3),
        )?)
    }),
    method(
        "readLine",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let line = buffered_reader_read_line(vm, receiver(args))?;
            object(line)
        },
    ),
    method(
        "lines",
        "()Ljava/util/stream/Stream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let mut lines = Vec::new();
            loop {
                let (class, method) =
                    vm.resolve_virtual_method(this, "readLine", "()Ljava/lang/String;")?;
                let line = vm
                    .invoke_method(class, method, vec![Value::Ref(this)])?
                    .as_ref();
                if line.is_null() {
                    break;
                }
                lines.push(line);
            }
            object(crate::stream::stream_from(vm, lines)?)
        },
    ),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if read_int_field(vm, this, "closed") != 0 {
            return void();
        }
        write_int_field(vm, this, "closed", 1);
        let reader = buffered_reader_in(vm, this);
        if !reader.is_null() {
            let (class, method) = vm.resolve_virtual_method(reader, "close", "()V")?;
            vm.invoke_method(class, method, vec![Value::Ref(reader)])?;
        }
        void()
    }),
    method("ready", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        buffered_reader_ensure_open(vm, this)?;
        // A pending `\r\n` skip is resolved here so ready() reports EOF
        // without forcing a read that a bounded source would reject.
        if read_int_field(vm, this, "skipLF") != 0 {
            let mut next = read_int_field(vm, this, "nextChar");
            if next >= read_int_field(vm, this, "nChars") && buffered_reader_in_ready(vm, this)? {
                buffered_reader_fill(vm, this)?;
                next = read_int_field(vm, this, "nextChar");
            }
            if next < read_int_field(vm, this, "nChars") {
                let cb = buffered_reader_cb(vm, this);
                if vm.array_get(cb, next)?.as_int() == i32::from(b'\n') {
                    write_int_field(vm, this, "nextChar", next + 1);
                }
                write_int_field(vm, this, "skipLF", 0);
            }
        }
        if read_int_field(vm, this, "nextChar") < read_int_field(vm, this, "nChars") {
            return boolean(true);
        }
        boolean(buffered_reader_in_ready(vm, this)?)
    }),
    method("mark", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        buffered_reader_ensure_open(vm, this)?;
        let limit = int_arg(args, 1);
        if limit < 0 {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("Read-ahead limit < 0"),
            ));
        }
        write_int_field(vm, this, "readAheadLimit", limit);
        write_int_field(vm, this, "markedChar", read_int_field(vm, this, "nextChar"));
        write_int_field(vm, this, "markedSkipLF", read_int_field(vm, this, "skipLF"));
        void()
    }),
    method("reset", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        buffered_reader_ensure_open(vm, this)?;
        let marked = read_int_field(vm, this, "markedChar");
        if marked < 0 {
            let message = if marked == -2 {
                "Mark invalid"
            } else {
                "Stream not marked"
            };
            return Err(vm.throw_new("java/io/IOException", Some(message)));
        }
        write_int_field(vm, this, "nextChar", marked);
        let marked_skip = read_int_field(vm, this, "markedSkipLF");
        write_int_field(vm, this, "skipLF", marked_skip);
        void()
    }),
    method("markSupported", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
    method("skip", "(J)J", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let n = crate::long_arg(args, 1);
        if n < 0 {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("skip value is negative"),
            ));
        }
        buffered_reader_ensure_open(vm, this)?;
        let mut remaining = n;
        while remaining > 0 {
            let mut next = read_int_field(vm, this, "nextChar");
            let mut n_chars = read_int_field(vm, this, "nChars");
            if next >= n_chars {
                buffered_reader_fill(vm, this)?;
                next = read_int_field(vm, this, "nextChar");
                n_chars = read_int_field(vm, this, "nChars");
            }
            if next >= n_chars {
                break;
            }
            if read_int_field(vm, this, "skipLF") != 0 {
                write_int_field(vm, this, "skipLF", 0);
                let cb = buffered_reader_cb(vm, this);
                if vm.array_get(cb, next)?.as_int() == i32::from(b'\n') {
                    next += 1;
                    write_int_field(vm, this, "nextChar", next);
                }
            }
            let available = i64::from(n_chars - next);
            if remaining <= available {
                write_int_field(vm, this, "nextChar", next + remaining as i32);
                remaining = 0;
                break;
            }
            remaining -= available;
            write_int_field(vm, this, "nextChar", n_chars);
        }
        long(n - remaining)
    }),
];

pub(crate) const BUFFERED_READER: oxjvm_vm::NativeClass = class(
    "java/io/BufferedReader",
    Some("java/io/Reader"),
    &[],
    ACC_PUBLIC,
    &[
        field("in", "Ljava/io/Reader;", ACC_PRIVATE, None),
        field("cb", "[C", ACC_PRIVATE, None),
        field("nChars", "I", ACC_PRIVATE, None),
        field("nextChar", "I", ACC_PRIVATE, None),
        field("markedChar", "I", ACC_PRIVATE, None),
        field("readAheadLimit", "I", ACC_PRIVATE, None),
        field("markedSkipLF", "I", ACC_PRIVATE, None),
        field("skipLF", "I", ACC_PRIVATE, None),
        field("closed", "I", ACC_PRIVATE, None),
    ],
    &BUFFERED_READER_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// LineNumberReader
// -------------------------------------------------------------------------------------------

const LINE_NUMBER_NONE: i32 = 0;
const LINE_NUMBER_CHAR: i32 = 1;
const LINE_NUMBER_EOL: i32 = 2;
const LINE_NUMBER_EOF: i32 = 3;

fn line_number_flag(vm: &Vm<'_>, this: ObjectRef, name: &str) -> bool {
    vm.read_instance_int_named(this, vm.class_of(this), name, "Z")
        .unwrap_or(0)
        != 0
}

fn line_number_set_flag(vm: &mut Vm<'_>, this: ObjectRef, name: &str, value: bool) {
    let class = vm.class_of(this);
    if let Some((declaring, field)) = vm.find_field_opt(class, name, "Z") {
        vm.set_instance_int(this, declaring, field, i32::from(value));
    }
}

fn line_number_prev(vm: &Vm<'_>, this: ObjectRef) -> i32 {
    read_int_field(vm, this, "prevChar")
}

fn line_number_set_prev(vm: &mut Vm<'_>, this: ObjectRef, value: i32) {
    write_int_field(vm, this, "prevChar", value);
}

fn line_number_bump(vm: &mut Vm<'_>, this: ObjectRef) {
    let line = read_int_field(vm, this, "lineNumber");
    write_int_field(vm, this, "lineNumber", line + 1);
}

/// `LineNumberReader.read()`: normalizes `\r` and `\r\n` to `\n` and counts lines.
fn line_number_reader_read(vm: &mut Vm<'_>, this: ObjectRef) -> Result<i32, VmError> {
    let mut c = buffered_reader_read_char(vm, this)?;
    if line_number_flag(vm, this, "skipLF") {
        if c == i32::from(b'\n') {
            c = buffered_reader_read_char(vm, this)?;
        }
        line_number_set_flag(vm, this, "skipLF", false);
    }
    match c {
        13 | 10 => {
            if c == 13 {
                line_number_set_flag(vm, this, "skipLF", true);
            }
            line_number_bump(vm, this);
            line_number_set_prev(vm, this, LINE_NUMBER_EOL);
            Ok(i32::from(b'\n'))
        }
        -1 => {
            if line_number_prev(vm, this) == LINE_NUMBER_CHAR {
                line_number_bump(vm, this);
            }
            line_number_set_prev(vm, this, LINE_NUMBER_EOF);
            Ok(-1)
        }
        _ => {
            line_number_set_prev(vm, this, LINE_NUMBER_CHAR);
            Ok(c)
        }
    }
}

/// `LineNumberReader.read(char[], int, int)`: bulk read with line counting.
fn line_number_reader_read_chars(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    array: ObjectRef,
    offset: i32,
    length: i32,
) -> Result<i32, VmError> {
    reader_check_range(vm, array, offset, length)?;
    if length == 0 {
        return Ok(0);
    }
    let mut count = 0;
    while count < length {
        let value = buffered_reader_read_char(vm, this)?;
        if value < 0 {
            break;
        }
        vm.array_set(array, offset + count, Value::Int(value), false)?;
        count += 1;
    }
    if count == 0 {
        if line_number_prev(vm, this) == LINE_NUMBER_CHAR {
            line_number_bump(vm, this);
        }
        line_number_set_prev(vm, this, LINE_NUMBER_EOF);
        return Ok(-1);
    }
    let mut skip_lf = line_number_flag(vm, this, "skipLF");
    for index in offset..offset + count {
        let c = vm.array_get(array, index)?.as_int();
        if skip_lf {
            skip_lf = false;
            if c == i32::from(b'\n') {
                continue;
            }
        }
        if c == 13 {
            skip_lf = true;
            line_number_bump(vm, this);
        } else if c == 10 {
            line_number_bump(vm, this);
        }
    }
    line_number_set_flag(vm, this, "skipLF", skip_lf);
    let last = vm.array_get(array, offset + count - 1)?.as_int();
    line_number_set_prev(
        vm,
        this,
        if last == 13 || last == 10 {
            LINE_NUMBER_EOL
        } else {
            LINE_NUMBER_CHAR
        },
    );
    Ok(count)
}

const LINE_NUMBER_READER_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method(
        "<init>",
        "(Ljava/io/Reader;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            buffered_reader_ctor(vm, receiver(args), ref_arg(args, 1), 8192)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/Reader;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            buffered_reader_ctor(vm, receiver(args), ref_arg(args, 1), int_arg(args, 2))?;
            void()
        },
    ),
    method("setLineNumber", "(I)V", ACC_PUBLIC, |vm, _, args| {
        write_int_field(vm, receiver(args), "lineNumber", int_arg(args, 1));
        void()
    }),
    method("getLineNumber", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "lineNumber"))
    }),
    method("read", "()I", ACC_PUBLIC, |vm, _, args| {
        int(line_number_reader_read(vm, receiver(args))?)
    }),
    method("read", "([CII)I", ACC_PUBLIC, |vm, _, args| {
        int(line_number_reader_read_chars(
            vm,
            receiver(args),
            ref_arg(args, 1),
            int_arg(args, 2),
            int_arg(args, 3),
        )?)
    }),
    method(
        "readLine",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let mut line = String::new();
            let mut read_any = false;
            let mut terminated = false;
            let mut ended_cr = false;
            let mut omit_lf = line_number_flag(vm, this, "skipLF");
            loop {
                let c = buffered_reader_read_char(vm, this)?;
                if c < 0 {
                    break;
                }
                if omit_lf {
                    omit_lf = false;
                    if c == i32::from(b'\n') {
                        continue;
                    }
                }
                if c == 10 || c == 13 {
                    terminated = true;
                    ended_cr = c == 13;
                    break;
                }
                read_any = true;
                line.push(char::from_u32(c as u32).unwrap_or('\u{FFFD}'));
            }
            line_number_set_flag(vm, this, "skipLF", ended_cr);
            if read_any || terminated {
                line_number_bump(vm, this);
                line_number_set_prev(
                    vm,
                    this,
                    if terminated {
                        LINE_NUMBER_EOL
                    } else {
                        LINE_NUMBER_EOF
                    },
                );
                object(vm.make_string(&line)?)
            } else {
                if line_number_prev(vm, this) == LINE_NUMBER_CHAR {
                    line_number_bump(vm, this);
                }
                line_number_set_prev(vm, this, LINE_NUMBER_EOF);
                object(ObjectRef::NULL)
            }
        },
    ),
    method("skip", "(J)J", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let n = crate::long_arg(args, 1);
        if n < 0 {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("skip() value is negative"),
            ));
        }
        let chunk = n.min(8192) as usize;
        let buffer = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Char, chunk)?;
        let (class, method) = vm.resolve_virtual_method(this, "read", "([CII)I")?;
        let mut remaining = n;
        while remaining > 0 {
            let want = remaining.min(chunk as i64) as i32;
            let got = vm
                .invoke_method(
                    class,
                    method,
                    vec![
                        Value::Ref(this),
                        Value::Ref(buffer),
                        Value::Int(0),
                        Value::Int(want),
                    ],
                )?
                .as_int();
            if got < 0 {
                break;
            }
            remaining -= i64::from(got);
        }
        if n - remaining > 0 {
            line_number_set_prev(vm, this, LINE_NUMBER_NONE);
        }
        long(n - remaining)
    }),
    method("mark", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let mut limit = int_arg(args, 1);
        if line_number_flag(vm, this, "skipLF") {
            limit += 1;
        }
        let super_class = vm.resolve_class("java/io/BufferedReader")?;
        let (declaring, method) = vm
            .find_method(super_class, "mark", "(I)V")
            .ok_or_else(|| VmError::internal("BufferedReader.mark missing"))?;
        vm.invoke_method(declaring, method, vec![Value::Ref(this), Value::Int(limit)])?;
        let line = read_int_field(vm, this, "lineNumber");
        write_int_field(vm, this, "markedLineNumber", line);
        let skip = line_number_flag(vm, this, "skipLF");
        line_number_set_flag(vm, this, "markedSkipLF", skip);
        void()
    }),
    method("reset", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let super_class = vm.resolve_class("java/io/BufferedReader")?;
        let (declaring, method) = vm
            .find_method(super_class, "reset", "()V")
            .ok_or_else(|| VmError::internal("BufferedReader.reset missing"))?;
        vm.invoke_method(declaring, method, vec![Value::Ref(this)])?;
        let marked = read_int_field(vm, this, "markedLineNumber");
        write_int_field(vm, this, "lineNumber", marked);
        let skip = line_number_flag(vm, this, "markedSkipLF");
        line_number_set_flag(vm, this, "skipLF", skip);
        void()
    }),
];

/// `java.io.LineNumberReader`: a buffered reader that tracks line numbers, treating
/// `\n`, `\r`, and `\r\n` as line terminators.
pub(crate) const LINE_NUMBER_READER: oxjvm_vm::NativeClass = class(
    "java/io/LineNumberReader",
    Some("java/io/BufferedReader"),
    &[],
    ACC_PUBLIC,
    &[
        field("lineNumber", "I", ACC_PRIVATE, None),
        field("markedLineNumber", "I", ACC_PRIVATE, None),
        field("skipLF", "Z", ACC_PRIVATE, None),
        field("markedSkipLF", "Z", ACC_PRIVATE, None),
        field("prevChar", "I", ACC_PRIVATE, None),
    ],
    &LINE_NUMBER_READER_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// InputStreamReader
// -------------------------------------------------------------------------------------------

fn input_stream_reader_content(vm: &Vm<'_>, this: ObjectRef) -> String {
    vm.read_string_field(this, vm.class_of(this), "content")
        .unwrap_or_default()
}

/// Historical encoding names reported by `InputStreamReader.getEncoding`.
fn historical_encoding(canonical: &str) -> &'static str {
    match canonical {
        "US-ASCII" => "ASCII",
        "ISO-8859-1" => "ISO8859_1",
        "UTF-16BE" => "UnicodeBigUnmarked",
        "UTF-16LE" => "UnicodeLittleUnmarked",
        "UTF-16" => "UTF-16",
        _ => "UTF8",
    }
}

fn input_stream_reader_init(
    vm: &mut Vm<'_>,
    args: &[Value],
    canonical: &str,
) -> Result<Value, VmError> {
    let this = receiver(args);
    let stream = ref_arg(args, 1);
    let mut bytes: Vec<u8> = Vec::new();
    if !stream.is_null() {
        while let Ok((class, method)) = vm.resolve_virtual_method(stream, "read", "()I") {
            let value = vm
                .invoke_method(class, method, vec![Value::Ref(stream)])?
                .as_int();
            if value < 0 {
                break;
            }
            bytes.push(value as u8);
        }
    }
    let content = String::from_utf16_lossy(&crate::charset::decode(&bytes, canonical));
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "content", "Ljava/lang/String;")?;
    let value = vm.make_string(&content)?;
    vm.set_instance_ref(this, declaring, field, value);
    let encoding = vm.make_string(historical_encoding(canonical))?;
    let (declaring, field) = vm.find_field(class, "encoding", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, encoding);
    write_int_field(vm, this, "pos", 0);
    void()
}

/// Resolve the charset argument of an `InputStreamReader` constructor to a canonical name.
fn input_stream_reader_charset(
    vm: &mut Vm<'_>,
    charset: ObjectRef,
) -> Result<&'static str, VmError> {
    if charset.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let name = crate::charset::name_of(vm, charset);
    crate::charset::canonical_name(&name)
        .ok_or_else(|| vm.throw_new("java/io/UnsupportedEncodingException", Some(&name)))
}

const INPUT_STREAM_READER_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method(
        "<init>",
        "(Ljava/io/InputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| input_stream_reader_init(vm, args, "UTF-8"),
    ),
    method(
        "<init>",
        "(Ljava/io/InputStream;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = crate::string_arg(vm, args[2])?;
            match crate::charset::canonical_name(&name) {
                Some(canonical) => input_stream_reader_init(vm, args, canonical),
                None => Err(vm.throw_new("java/io/UnsupportedEncodingException", Some(&name))),
            }
        },
    ),
    method(
        "<init>",
        "(Ljava/io/InputStream;Ljava/nio/charset/Charset;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let canonical = input_stream_reader_charset(vm, ref_arg(args, 2))?;
            input_stream_reader_init(vm, args, canonical)
        },
    ),
    method(
        "<init>",
        "(Ljava/io/InputStream;Ljava/nio/charset/CharsetDecoder;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let decoder = ref_arg(args, 2);
            if decoder.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let (class, method) =
                vm.resolve_virtual_method(decoder, "charset", "()Ljava/nio/charset/Charset;")?;
            let charset = vm
                .invoke_method(class, method, vec![Value::Ref(decoder)])?
                .as_ref();
            let canonical = input_stream_reader_charset(vm, charset)?;
            input_stream_reader_init(vm, args, canonical)
        },
    ),
    method(
        "getEncoding",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let encoding = vm
                .read_ref_field(this, vm.class_of(this), "encoding", "Ljava/lang/String;")
                .unwrap_or(ObjectRef::NULL);
            object(encoding)
        },
    ),
    method("read", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let content = input_stream_reader_content(vm, this);
        let pos = read_int_field(vm, this, "pos");
        let units: Vec<u16> = content.encode_utf16().collect();
        if pos < 0 || pos as usize >= units.len() {
            return int(-1);
        }
        write_int_field(vm, this, "pos", pos + 1);
        int(i32::from(units[pos as usize]))
    }),
    method("read", "([CII)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let content = input_stream_reader_content(vm, this);
        let units: Vec<u16> = content.encode_utf16().collect();
        let mut pos = read_int_field(vm, this, "pos");
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        let array = ref_arg(args, 1);
        reader_check_range(vm, array, offset, length)?;
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
        write_int_field(vm, this, "pos", pos);
        if count == 0 { int(-1) } else { int(count) }
    }),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
];

pub(crate) const INPUT_STREAM_READER: oxjvm_vm::NativeClass = class(
    "java/io/InputStreamReader",
    Some("java/io/Reader"),
    &[],
    ACC_PUBLIC,
    &[
        field("content", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("pos", "I", ACC_PRIVATE, None),
        field("encoding", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &INPUT_STREAM_READER_METHODS,
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
            let out = ref_arg(args, 1);
            if out.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let (declaring, field) = vm.find_field(vm.class_of(this), "out", "Ljava/io/Writer;")?;
            vm.set_instance_ref(this, declaring, field, out);
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/Writer;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let out = ref_arg(args, 1);
            if out.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            if int_arg(args, 2) <= 0 {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("Buffer size <= 0"),
                ));
            }
            let (declaring, field) = vm.find_field(vm.class_of(this), "out", "Ljava/io/Writer;")?;
            vm.set_instance_ref(this, declaring, field, out);
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
            write_int_field(vm, this, "primitive", 0);
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
            write_int_field(vm, this, "primitive", int_arg(args, 3));
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
        boolean(read_int_field(vm, receiver(args), "primitive") != 0)
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

fn string_reader_ensure_open(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    if read_int_field(vm, this, "closed") != 0 {
        return Err(vm.throw_new("java/io/IOException", Some("Stream closed")));
    }
    Ok(())
}

const STRING_READER_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = ref_arg(args, 1);
            if text.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "str", "Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, text);
            void()
        },
    ),
    method("read", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        string_reader_ensure_open(vm, this)?;
        let class = vm.class_of(this);
        let next = read_int_field(vm, this, "next");
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
        write_int_field(vm, this, "next", next + 1);
        int(i32::from(unit))
    }),
    method("read", "([CII)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        string_reader_ensure_open(vm, this)?;
        let class = vm.class_of(this);
        let array = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        reader_check_range(vm, array, offset, length)?;
        if length == 0 {
            return int(0);
        }
        let next = read_int_field(vm, this, "next");
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
        write_int_field(vm, this, "next", next + count as i32);
        int(count as i32)
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        write_int_field(vm, receiver(args), "closed", 1);
        void()
    }),
    method("mark", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        string_reader_ensure_open(vm, this)?;
        if int_arg(args, 1) < 0 {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("Read-ahead limit < 0"),
            ));
        }
        let next = read_int_field(vm, this, "next");
        write_int_field(vm, this, "mark", next);
        void()
    }),
    method("reset", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        string_reader_ensure_open(vm, this)?;
        let mark = read_int_field(vm, this, "mark");
        write_int_field(vm, this, "next", mark);
        void()
    }),
    method("markSupported", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
    method("ready", "()Z", ACC_PUBLIC, |vm, _, args| {
        string_reader_ensure_open(vm, receiver(args))?;
        boolean(true)
    }),
    method("skip", "(J)J", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        string_reader_ensure_open(vm, this)?;
        let ns = crate::long_arg(args, 1);
        let class = vm.class_of(this);
        let text = vm
            .read_ref_field(this, class, "str", "Ljava/lang/String;")
            .unwrap_or(ObjectRef::NULL);
        let length = vm.string_utf16(text).map(<[u16]>::len).unwrap_or(0) as i64;
        let next = i64::from(read_int_field(vm, this, "next"));
        // StringReader skips backwards too, clamped to the start of the
        // string; once the string is exhausted every skip returns zero.
        let n = if next >= length {
            0
        } else {
            (length - next).min(ns).max(-next)
        };
        write_int_field(vm, this, "next", (next + n) as i32);
        long(n)
    }),
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
        field("mark", "I", ACC_PRIVATE, None),
        field("closed", "I", ACC_PRIVATE, None),
    ],
    &STRING_READER_METHODS,
    None,
);

const STRING_BUFFER_INPUT_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = ref_arg(args, 1);
            if text.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "buffer", "Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, text);
            let length = vm.string_utf16(text).map_or(0, |units| units.len()) as i32;
            write_int_field(vm, this, "count", length);
            void()
        },
    ),
    method("read", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let pos = read_int_field(vm, this, "pos");
        let count = read_int_field(vm, this, "count");
        if pos >= count {
            return int(-1);
        }
        let class = vm.class_of(this);
        let text = vm
            .read_ref_field(this, class, "buffer", "Ljava/lang/String;")
            .unwrap_or(ObjectRef::NULL);
        let Some(units) = vm.string_utf16(text) else {
            return int(-1);
        };
        let unit = units[pos as usize];
        write_int_field(vm, this, "pos", pos + 1);
        int(i32::from(unit) & 0xFF)
    }),
    method("read", "([BII)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let array = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        reader_check_range(vm, array, offset, length)?;
        let pos = read_int_field(vm, this, "pos");
        let count = read_int_field(vm, this, "count");
        if pos >= count {
            return int(-1);
        }
        let length = length.min(count - pos);
        if length <= 0 {
            return int(0);
        }
        let class = vm.class_of(this);
        let text = vm
            .read_ref_field(this, class, "buffer", "Ljava/lang/String;")
            .unwrap_or(ObjectRef::NULL);
        let Some(units) = vm.string_utf16(text) else {
            return int(-1);
        };
        let bytes: Vec<i32> = (0..length)
            .map(|index| i32::from(units[(pos + index) as usize]) & 0xFF)
            .collect();
        for (index, byte) in bytes.into_iter().enumerate() {
            vm.array_set(array, offset + index as i32, Value::Int(byte), true)?;
        }
        write_int_field(vm, this, "pos", pos + length);
        int(length)
    }),
    method("skip", "(J)J", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let pos = read_int_field(vm, this, "pos");
        let count = read_int_field(vm, this, "count");
        let requested = crate::long_arg(args, 1);
        let skipped = requested.min(i64::from(count - pos)).max(0);
        write_int_field(vm, this, "pos", pos + skipped as i32);
        long(skipped)
    }),
    method("available", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        int(read_int_field(vm, this, "count") - read_int_field(vm, this, "pos"))
    }),
    method("reset", "()V", ACC_PUBLIC, |vm, _, args| {
        write_int_field(vm, receiver(args), "pos", 0);
        void()
    }),
];

pub(crate) const STRING_BUFFER_INPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/io/StringBufferInputStream",
    Some("java/io/InputStream"),
    &[],
    ACC_PUBLIC,
    &[
        field("buffer", "Ljava/lang/String;", ACC_PROTECTED, None),
        field("pos", "I", ACC_PROTECTED, None),
        field("count", "I", ACC_PROTECTED, None),
    ],
    &STRING_BUFFER_INPUT_METHODS,
    None,
);

fn pushback_reader_in(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "in", "Ljava/io/Reader;")
        .unwrap_or(ObjectRef::NULL)
}

fn pushback_reader_ensure_open(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    if pushback_reader_in(vm, this).is_null() {
        return Err(vm.throw_new("java/io/IOException", Some("Stream closed")));
    }
    Ok(())
}

fn pushback_reader_buf(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "buf", "[I")
        .unwrap_or(ObjectRef::NULL)
}

fn pushback_reader_ctor(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    input: ObjectRef,
    size: i32,
) -> Result<(), VmError> {
    if size <= 0 {
        return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("size <= 0")));
    }
    let (declaring, field) = vm.find_field(vm.class_of(this), "in", "Ljava/io/Reader;")?;
    vm.set_instance_ref(this, declaring, field, input);
    let buf = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Int, size as usize)?;
    let (declaring, field) = vm.find_field(vm.class_of(this), "buf", "[I")?;
    vm.set_instance_ref(this, declaring, field, buf);
    write_int_field(vm, this, "count", 0);
    Ok(())
}

fn pushback_reader_unread_int(vm: &mut Vm<'_>, this: ObjectRef, ch: i32) -> Result<(), VmError> {
    let buf = pushback_reader_buf(vm, this);
    let count = read_int_field(vm, this, "count");
    let capacity = vm.array_length(buf)? as i32;
    if count >= capacity {
        return Err(vm.throw_new("java/io/IOException", Some("Pushback buffer overflow")));
    }
    vm.array_set(buf, count, Value::Int(ch), true)?;
    write_int_field(vm, this, "count", count + 1);
    Ok(())
}

const PUSHBACK_READER_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method(
        "<init>",
        "(Ljava/io/Reader;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            pushback_reader_ctor(vm, receiver(args), ref_arg(args, 1), 1)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/Reader;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            pushback_reader_ctor(vm, receiver(args), ref_arg(args, 1), int_arg(args, 2))?;
            void()
        },
    ),
    method("read", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        pushback_reader_ensure_open(vm, this)?;
        let count = read_int_field(vm, this, "count");
        if count > 0 {
            let buf = pushback_reader_buf(vm, this);
            let ch = vm.array_get(buf, count - 1)?.as_int();
            write_int_field(vm, this, "count", count - 1);
            int(ch)
        } else {
            let input = pushback_reader_in(vm, this);
            let (class, method) = vm.resolve_virtual_method(input, "read", "()I")?;
            vm.invoke_method(class, method, vec![Value::Ref(input)])
        }
    }),
    method("read", "([CII)I", ACC_PUBLIC, |vm, _, args| {
        pushback_reader_ensure_open(vm, receiver(args))?;
        reader_read_chars(vm, args, int_arg(args, 2), Some(int_arg(args, 3)))
    }),
    method("unread", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        pushback_reader_ensure_open(vm, this)?;
        pushback_reader_unread_int(vm, this, int_arg(args, 1))?;
        void()
    }),
    method("unread", "([C)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let array = ref_arg(args, 1);
        let length = vm.array_length(array)? as i32;
        let (class, method) = vm.resolve_virtual_method(this, "unread", "([CII)V")?;
        vm.invoke_method(
            class,
            method,
            vec![
                Value::Ref(this),
                Value::Ref(array),
                Value::Int(0),
                Value::Int(length),
            ],
        )?;
        void()
    }),
    method("unread", "([CII)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        pushback_reader_ensure_open(vm, this)?;
        let array = ref_arg(args, 1);
        if array.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        let capacity = vm.array_length(array)? as i32;
        if offset < 0 || length < 0 || i64::from(offset) + i64::from(length) > i64::from(capacity) {
            return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
        }
        let buf = pushback_reader_buf(vm, this);
        let count = read_int_field(vm, this, "count");
        let buf_capacity = vm.array_length(buf)? as i32;
        if count + length > buf_capacity {
            return Err(vm.throw_new("java/io/IOException", Some("Pushback buffer overflow")));
        }
        for index in (offset..offset + length).rev() {
            let ch = vm.array_get(array, index)?.as_int();
            pushback_reader_unread_int(vm, this, ch)?;
        }
        void()
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let input = pushback_reader_in(vm, this);
        if input.is_null() {
            return void();
        }
        let (class, method) = vm.resolve_virtual_method(input, "close", "()V")?;
        vm.invoke_method(class, method, vec![Value::Ref(input)])?;
        // Closing detaches the source and the pushback buffer so later
        // reads and unreads report "Stream closed".
        let declaring_class = vm.class_of(this);
        if let Some((declaring, field)) =
            vm.find_field_opt(declaring_class, "in", "Ljava/io/Reader;")
        {
            vm.set_instance_ref(this, declaring, field, ObjectRef::NULL);
        }
        if let Some((declaring, field)) = vm.find_field_opt(declaring_class, "buf", "[I") {
            vm.set_instance_ref(this, declaring, field, ObjectRef::NULL);
        }
        void()
    }),
    method("ready", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        pushback_reader_ensure_open(vm, this)?;
        if read_int_field(vm, this, "count") > 0 {
            return boolean(true);
        }
        let input = pushback_reader_in(vm, this);
        let (class, method) = vm.resolve_virtual_method(input, "ready", "()Z")?;
        vm.invoke_method(class, method, vec![Value::Ref(input)])
    }),
    method("markSupported", "()Z", ACC_PUBLIC, |_, _, _| boolean(false)),
    method("mark", "(I)V", ACC_PUBLIC, |vm, _, _| {
        Err(vm.throw_new("java/io/IOException", Some("mark/reset not supported")))
    }),
    method("reset", "()V", ACC_PUBLIC, |vm, _, _| {
        Err(vm.throw_new("java/io/IOException", Some("mark/reset not supported")))
    }),
];

/// `java.io.PushbackReader`: a Reader with a pushback buffer for unreading characters.
pub(crate) const PUSHBACK_READER: oxjvm_vm::NativeClass = class(
    "java/io/PushbackReader",
    Some("java/io/Reader"),
    &[],
    ACC_PUBLIC,
    &[
        field("in", "Ljava/io/Reader;", ACC_PRIVATE, None),
        field("buf", "[I", ACC_PRIVATE, None),
        field("count", "I", ACC_PRIVATE, None),
    ],
    &PUSHBACK_READER_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// RandomAccessFile
// -------------------------------------------------------------------------------------------

/// Read the current contents of a `RandomAccessFile`'s backing byte array.
fn raf_bytes(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Vec<u8>, VmError> {
    let data = vm
        .read_ref_field(this, vm.class_of(this), "data", "[B")
        .unwrap_or(ObjectRef::NULL);
    if data.is_null() {
        return Ok(Vec::new());
    }
    crate::lang::array_bytes(vm, data)
}

/// Replace a `RandomAccessFile`'s backing byte array with `bytes`.
fn raf_set_bytes(vm: &mut Vm<'_>, this: ObjectRef, bytes: &[u8]) -> Result<(), VmError> {
    let data = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, bytes.len())?;
    for (index, byte) in bytes.iter().enumerate() {
        vm.array_set(
            data,
            index as i32,
            Value::Int(i32::from(*byte as i8)),
            false,
        )?;
    }
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "data", "[B")?;
    vm.set_instance_ref(this, declaring, field, data);
    Ok(())
}

/// Flush a writable `RandomAccessFile`'s in-memory contents back to the host filesystem.
fn raf_flush(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    let writable = vm
        .read_instance_int_named(this, vm.class_of(this), "writable", "Z")
        .unwrap_or(0)
        != 0;
    if !writable {
        return Ok(());
    }
    let path = vm
        .read_string_field(this, vm.class_of(this), "path")
        .unwrap_or_default();
    let bytes = raf_bytes(vm, this)?;
    let ok = vm
        .host
        .filesystem()
        .map(|fs| fs.write(&path, &bytes, false))
        .unwrap_or(false);
    if !ok {
        return Err(vm.throw_new(
            "java/io/IOException",
            Some(&alloc::format!("Cannot write to {path}")),
        ));
    }
    Ok(())
}

fn raf_init(vm: &mut Vm<'_>, this: ObjectRef, path: &str, mode: &str) -> Result<Value, VmError> {
    if !matches!(mode, "r" | "rw" | "rws" | "rwd") {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some(&alloc::format!("Illegal mode \"{mode}\"")),
        ));
    }
    let writable = mode != "r";
    let bytes = vm.host.filesystem().and_then(|fs| fs.read(path));
    let bytes = match bytes {
        Some(bytes) => bytes,
        None if writable => Vec::new(),
        None => {
            return Err(vm.throw_new("java/io/FileNotFoundException", Some(path)));
        }
    };
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "path", "Ljava/lang/String;")?;
    let text = vm.make_string(path)?;
    vm.set_instance_ref(this, declaring, field, text);
    raf_set_bytes(vm, this, &bytes)?;
    write_int_field(vm, this, "position", 0);
    if let Some((declaring, field)) = vm.find_field_opt(class, "writable", "Z") {
        vm.set_instance_int(this, declaring, field, i32::from(writable));
    }
    if writable {
        raf_flush(vm, this)?;
    }
    void()
}

/// Write `bytes` at the current file pointer, extending the file as needed, then flush.
fn raf_write(vm: &mut Vm<'_>, this: ObjectRef, bytes: &[u8]) -> Result<Value, VmError> {
    let writable = vm
        .read_instance_int_named(this, vm.class_of(this), "writable", "Z")
        .unwrap_or(0)
        != 0;
    if !writable {
        return Err(vm.throw_new("java/io/IOException", Some("Bad file descriptor")));
    }
    let position = read_int_field(vm, this, "position") as usize;
    let mut data = raf_bytes(vm, this)?;
    let end = position + bytes.len();
    if end > data.len() {
        data.resize(end, 0);
    }
    data[position..end].copy_from_slice(bytes);
    raf_set_bytes(vm, this, &data)?;
    write_int_field(vm, this, "position", end as i32);
    raf_flush(vm, this)?;
    void()
}

const RANDOM_ACCESS_FILE_METHODS: [oxjvm_vm::NativeMethodDef; 41] = [
    method(
        "<init>",
        "(Ljava/io/File;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = crate::file::file_path(vm, ref_arg(args, 1));
            let mode = crate::string_arg(vm, args[2])?;
            raf_init(vm, receiver(args), &path, &mode)
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let path = crate::string_arg(vm, args[1])?;
            let mode = crate::string_arg(vm, args[2])?;
            raf_init(vm, receiver(args), &path, &mode)
        },
    ),
    method("length", "()J", ACC_PUBLIC, |vm, _, args| {
        long(raf_bytes(vm, receiver(args))?.len() as i64)
    }),
    method("getFilePointer", "()J", ACC_PUBLIC, |vm, _, args| {
        long(i64::from(read_int_field(vm, receiver(args), "position")))
    }),
    method("seek", "(J)V", ACC_PUBLIC, |vm, _, args| {
        let pos = crate::long_arg(args, 1);
        if pos < 0 {
            return Err(vm.throw_new("java/io/IOException", Some("Negative seek offset")));
        }
        write_int_field(vm, receiver(args), "position", pos as i32);
        void()
    }),
    method("read", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let position = read_int_field(vm, this, "position") as usize;
        let data = raf_bytes(vm, this)?;
        if position >= data.len() {
            return int(-1);
        }
        write_int_field(vm, this, "position", (position + 1) as i32);
        int(i32::from(data[position]))
    }),
    method("read", "([B)I", ACC_PUBLIC, |vm, _, args| {
        let target = ref_arg(args, 1);
        let length = vm.array_length(target)? as i32;
        raf_read_into(vm, receiver(args), target, 0, length)
    }),
    method("read", "([BII)I", ACC_PUBLIC, |vm, _, args| {
        raf_read_into(
            vm,
            receiver(args),
            ref_arg(args, 1),
            int_arg(args, 2),
            int_arg(args, 3),
        )
    }),
    method("readFully", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let target = ref_arg(args, 1);
        let length = vm.array_length(target)? as i32;
        raf_read_fully(vm, receiver(args), target, 0, length)
    }),
    method("readFully", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        raf_read_fully(
            vm,
            receiver(args),
            ref_arg(args, 1),
            int_arg(args, 2),
            int_arg(args, 3),
        )
    }),
    method("skipBytes", "(I)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let n = int_arg(args, 1).max(0) as usize;
        let position = read_int_field(vm, this, "position") as usize;
        let length = raf_bytes(vm, this)?.len();
        let next = (position + n).min(length);
        write_int_field(vm, this, "position", next as i32);
        int((next - position) as i32)
    }),
    method("write", "(I)V", ACC_PUBLIC, |vm, _, args| {
        raf_write(vm, receiver(args), &[int_arg(args, 1) as u8])
    }),
    method("write", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
        raf_write(vm, receiver(args), &bytes)
    }),
    method("write", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        let bytes = crate::lang::array_bytes(vm, ref_arg(args, 1))?;
        let offset = int_arg(args, 2);
        let count = int_arg(args, 3);
        if offset < 0 || count < 0 || (offset + count) as usize > bytes.len() {
            return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
        }
        raf_write(
            vm,
            receiver(args),
            &bytes[offset as usize..(offset + count) as usize],
        )
    }),
    method(
        "writeBytes",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let units = vm
                .string_utf16(ref_arg(args, 1))
                .map(<[u16]>::to_vec)
                .unwrap_or_default();
            let bytes: Vec<u8> = units.iter().map(|unit| *unit as u8).collect();
            raf_write(vm, receiver(args), &bytes)
        },
    ),
    method(
        "writeChars",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let units = vm
                .string_utf16(ref_arg(args, 1))
                .map(<[u16]>::to_vec)
                .unwrap_or_default();
            let mut bytes = Vec::with_capacity(units.len() * 2);
            for unit in units {
                bytes.push((unit >> 8) as u8);
                bytes.push(unit as u8);
            }
            raf_write(vm, receiver(args), &bytes)
        },
    ),
    method(
        "readLine",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let mut position = read_int_field(vm, this, "position") as usize;
            let data = raf_bytes(vm, this)?;
            if position >= data.len() {
                return object(ObjectRef::NULL);
            }
            let mut line = String::new();
            while position < data.len() {
                let c = data[position];
                position += 1;
                if c == b'\n' {
                    break;
                }
                if c == b'\r' {
                    if position < data.len() && data[position] == b'\n' {
                        position += 1;
                    }
                    break;
                }
                line.push(c as char);
            }
            write_int_field(vm, this, "position", position as i32);
            object(vm.make_string(&line)?)
        },
    ),
    method("setLength", "(J)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let new_length = crate::long_arg(args, 1);
        if new_length < 0 {
            return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
        }
        let mut data = raf_bytes(vm, this)?;
        data.resize(new_length as usize, 0);
        raf_set_bytes(vm, this, &data)?;
        raf_flush(vm, this)?;
        void()
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        raf_flush(vm, this)?;
        // Closing the file also closes the channel handed out by getChannel.
        let class = vm.class_of(this);
        let channel = vm
            .read_ref_field(this, class, "channel", "Ljava/nio/channels/FileChannel;")
            .unwrap_or(ObjectRef::NULL);
        if !channel.is_null() {
            let (ch_class, ch_method) = vm.resolve_virtual_method(channel, "close", "()V")?;
            vm.invoke_method(ch_class, ch_method, vec![Value::Ref(channel)])?;
        }
        void()
    }),
    method(
        "getFD",
        "()Ljava/io/FileDescriptor;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/io/FileDescriptor")?;
            object(vm.new_instance(class)?)
        },
    ),
    method(
        "getChannel",
        "()Ljava/nio/channels/FileChannel;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let existing = vm
                .read_ref_field(this, class, "channel", "Ljava/nio/channels/FileChannel;")
                .unwrap_or(ObjectRef::NULL);
            if !existing.is_null() {
                return object(existing);
            }
            let path = vm
                .read_string_field(this, class, "path")
                .unwrap_or_default();
            let writable = read_int_field(vm, this, "writable") != 0;
            let channel = crate::nio::new_file_channel(vm, &path, 0, writable)?;
            if let Some((declaring, field)) =
                vm.find_field_opt(class, "channel", "Ljava/nio/channels/FileChannel;")
            {
                vm.set_instance_ref(this, declaring, field, channel);
            }
            object(channel)
        },
    ),
    method("readBoolean", "()Z", ACC_PUBLIC, |vm, _, args| {
        let bytes = raf_read_n(vm, receiver(args), 1)?;
        boolean(bytes[0] != 0)
    }),
    method("readByte", "()B", ACC_PUBLIC, |vm, _, args| {
        let bytes = raf_read_n(vm, receiver(args), 1)?;
        int(i32::from(bytes[0] as i8))
    }),
    method("readUnsignedByte", "()I", ACC_PUBLIC, |vm, _, args| {
        let bytes = raf_read_n(vm, receiver(args), 1)?;
        int(i32::from(bytes[0]))
    }),
    method("readShort", "()S", ACC_PUBLIC, |vm, _, args| {
        let bytes = raf_read_n(vm, receiver(args), 2)?;
        int(i32::from(be16(&bytes) as i16))
    }),
    method("readUnsignedShort", "()I", ACC_PUBLIC, |vm, _, args| {
        let bytes = raf_read_n(vm, receiver(args), 2)?;
        int(i32::from(be16(&bytes)))
    }),
    method("readChar", "()C", ACC_PUBLIC, |vm, _, args| {
        let bytes = raf_read_n(vm, receiver(args), 2)?;
        int(i32::from(be16(&bytes)))
    }),
    method("readInt", "()I", ACC_PUBLIC, |vm, _, args| {
        let bytes = raf_read_n(vm, receiver(args), 4)?;
        int(be32(&bytes) as i32)
    }),
    method("readLong", "()J", ACC_PUBLIC, |vm, _, args| {
        let bytes = raf_read_n(vm, receiver(args), 8)?;
        long(be64(&bytes) as i64)
    }),
    method("readFloat", "()F", ACC_PUBLIC, |vm, _, args| {
        let bytes = raf_read_n(vm, receiver(args), 4)?;
        crate::float(f32::from_bits(be32(&bytes)))
    }),
    method("readDouble", "()D", ACC_PUBLIC, |vm, _, args| {
        let bytes = raf_read_n(vm, receiver(args), 8)?;
        crate::double(f64::from_bits(be64(&bytes)))
    }),
    method(
        "readUTF",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let header = raf_read_n(vm, this, 2)?;
            let length = usize::from(be16(&header));
            let bytes = raf_read_n(vm, this, length)?;
            let units = decode_modified_utf8(&bytes).map_err(|message| {
                vm.throw_new("java/io/UTFDataFormatException", Some(&message))
            })?;
            object(vm.make_string_utf16(units)?)
        },
    ),
    method("writeBoolean", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        raf_write(
            vm,
            receiver(args),
            &[if int_arg(args, 1) != 0 { 1 } else { 0 }],
        )
    }),
    method("writeByte", "(I)V", ACC_PUBLIC, |vm, _, args| {
        raf_write(vm, receiver(args), &[(int_arg(args, 1) & 0xFF) as u8])
    }),
    method("writeShort", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let value = int_arg(args, 1) as u16;
        raf_write(vm, receiver(args), &value.to_be_bytes())
    }),
    method("writeChar", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let value = int_arg(args, 1) as u16;
        raf_write(vm, receiver(args), &value.to_be_bytes())
    }),
    method("writeInt", "(I)V", ACC_PUBLIC, |vm, _, args| {
        raf_write(vm, receiver(args), &int_arg(args, 1).to_be_bytes())
    }),
    method("writeLong", "(J)V", ACC_PUBLIC, |vm, _, args| {
        raf_write(vm, receiver(args), &crate::long_arg(args, 1).to_be_bytes())
    }),
    method("writeFloat", "(F)V", ACC_PUBLIC, |vm, _, args| {
        raf_write(
            vm,
            receiver(args),
            &crate::float_arg(args, 1).to_bits().to_be_bytes(),
        )
    }),
    method("writeDouble", "(D)V", ACC_PUBLIC, |vm, _, args| {
        raf_write(
            vm,
            receiver(args),
            &crate::double_arg(args, 1).to_bits().to_be_bytes(),
        )
    }),
    method(
        "writeUTF",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let units = vm
                .string_utf16(ref_arg(args, 1))
                .map(<[u16]>::to_vec)
                .unwrap_or_default();
            let encoded = modified_utf8(&units);
            if encoded.len() > 0xFFFF {
                return Err(vm.throw_new(
                    "java/io/UTFDataFormatException",
                    Some("encoded string too long"),
                ));
            }
            let mut bytes = Vec::with_capacity(encoded.len() + 2);
            bytes.extend_from_slice(&(encoded.len() as u16).to_be_bytes());
            bytes.extend_from_slice(&encoded);
            raf_write(vm, receiver(args), &bytes)
        },
    ),
];

fn raf_read_into(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    target: ObjectRef,
    offset: i32,
    length: i32,
) -> Result<Value, VmError> {
    let total = vm.array_length(target)? as i32;
    if offset < 0 || length < 0 || offset + length > total {
        return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
    }
    if length == 0 {
        return int(0);
    }
    let position = read_int_field(vm, this, "position") as usize;
    let data = raf_bytes(vm, this)?;
    if position >= data.len() {
        return int(-1);
    }
    let count = (length as usize).min(data.len() - position);
    for index in 0..count {
        vm.array_set(
            target,
            offset + index as i32,
            Value::Int(i32::from(data[position + index] as i8)),
            false,
        )?;
    }
    write_int_field(vm, this, "position", (position + count) as i32);
    int(count as i32)
}

fn raf_read_fully(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    target: ObjectRef,
    offset: i32,
    length: i32,
) -> Result<Value, VmError> {
    let got = raf_read_into(vm, this, target, offset, length)?.as_int();
    if got < length {
        return Err(vm.throw_new("java/io/EOFException", None));
    }
    void()
}

/// Read exactly `count` bytes at the file pointer, advancing it; `EOFException` if short.
fn raf_read_n(vm: &mut Vm<'_>, this: ObjectRef, count: usize) -> Result<Vec<u8>, VmError> {
    let position = read_int_field(vm, this, "position") as usize;
    let data = raf_bytes(vm, this)?;
    if position + count > data.len() {
        return Err(vm.throw_new("java/io/EOFException", None));
    }
    let bytes = data[position..position + count].to_vec();
    write_int_field(vm, this, "position", (position + count) as i32);
    Ok(bytes)
}

pub(crate) const RANDOM_ACCESS_FILE: oxjvm_vm::NativeClass = class(
    "java/io/RandomAccessFile",
    Some("java/lang/Object"),
    &[
        "java/io/Closeable",
        "java/io/DataInput",
        "java/io/DataOutput",
    ],
    ACC_PUBLIC,
    &[
        field("path", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("data", "[B", ACC_PRIVATE, None),
        field("position", "I", ACC_PRIVATE, None),
        field("writable", "Z", ACC_PRIVATE, None),
        field(
            "channel",
            "Ljava/nio/channels/FileChannel;",
            ACC_PRIVATE,
            None,
        ),
    ],
    &RANDOM_ACCESS_FILE_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// FilterInputStream / BufferedInputStream / BufferedOutputStream
//
// The buffered streams delegate every operation to the wrapped stream.  Buffering would only
// change host I/O scheduling, not observable Java semantics, and delegation keeps the
// mark/reset contract of the wrapped stream intact.
// -------------------------------------------------------------------------------------------

fn filter_input_stream_in(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field_any(this, "in").unwrap_or(ObjectRef::NULL)
}

fn delegate_input(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: &str,
    descriptor: &str,
    args: Vec<Value>,
) -> Result<Value, VmError> {
    let stream = filter_input_stream_in(vm, this);
    let (class, method) = vm.resolve_virtual_method(stream, name, descriptor)?;
    vm.invoke_method(class, method, args)
}

const FILTER_INPUT_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method(
        "<init>",
        "(Ljava/io/InputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "in", "Ljava/io/InputStream;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method("read", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        delegate_input(
            vm,
            this,
            "read",
            "()I",
            vec![Value::Ref(filter_input_stream_in(vm, this))],
        )
    }),
    method("read", "([B)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        delegate_input(
            vm,
            this,
            "read",
            "([B)I",
            vec![
                Value::Ref(filter_input_stream_in(vm, this)),
                Value::Ref(ref_arg(args, 1)),
            ],
        )
    }),
    method("read", "([BII)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        delegate_input(
            vm,
            this,
            "read",
            "([BII)I",
            vec![
                Value::Ref(filter_input_stream_in(vm, this)),
                Value::Ref(ref_arg(args, 1)),
                Value::Int(int_arg(args, 2)),
                Value::Int(int_arg(args, 3)),
            ],
        )
    }),
    method("skip", "(J)J", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        delegate_input(
            vm,
            this,
            "skip",
            "(J)J",
            vec![Value::Ref(filter_input_stream_in(vm, this)), args[1]],
        )
    }),
    method("available", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        delegate_input(
            vm,
            this,
            "available",
            "()I",
            vec![Value::Ref(filter_input_stream_in(vm, this))],
        )
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let stream = filter_input_stream_in(vm, this);
        let (class, method) = vm.resolve_virtual_method(stream, "close", "()V")?;
        vm.invoke_method(class, method, vec![Value::Ref(stream)])?;
        void()
    }),
    method("mark", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let stream = filter_input_stream_in(vm, this);
        let (class, method) = vm.resolve_virtual_method(stream, "mark", "(I)V")?;
        vm.invoke_method(
            class,
            method,
            vec![Value::Ref(stream), Value::Int(int_arg(args, 1))],
        )?;
        void()
    }),
    method("reset", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let stream = filter_input_stream_in(vm, this);
        let (class, method) = vm.resolve_virtual_method(stream, "reset", "()V")?;
        vm.invoke_method(class, method, vec![Value::Ref(stream)])?;
        void()
    }),
];

pub(crate) const FILTER_INPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/io/FilterInputStream",
    Some("java/io/InputStream"),
    &[],
    ACC_PUBLIC,
    &[field("in", "Ljava/io/InputStream;", ACC_PROTECTED, None)],
    &FILTER_INPUT_METHODS,
    None,
);

const BUFFERED_INPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "<init>",
        "(Ljava/io/InputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "in", "Ljava/io/InputStream;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/InputStream;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if int_arg(args, 2) <= 0 {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("size <= 0")));
            }
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "in", "Ljava/io/InputStream;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method("read", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        buffered_input_check_open(vm, this)?;
        delegate_input(
            vm,
            this,
            "read",
            "()I",
            vec![Value::Ref(filter_input_stream_in(vm, this))],
        )
    }),
    method("read", "([B)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        buffered_input_check_open(vm, this)?;
        delegate_input(
            vm,
            this,
            "read",
            "([B)I",
            vec![
                Value::Ref(filter_input_stream_in(vm, this)),
                Value::Ref(ref_arg(args, 1)),
            ],
        )
    }),
    method("read", "([BII)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        buffered_input_check_open(vm, this)?;
        // Bounds are validated even for a zero-length request.
        reader_check_range(vm, ref_arg(args, 1), int_arg(args, 2), int_arg(args, 3))?;
        if int_arg(args, 3) == 0 {
            return int(0);
        }
        delegate_input(
            vm,
            this,
            "read",
            "([BII)I",
            vec![
                Value::Ref(filter_input_stream_in(vm, this)),
                Value::Ref(ref_arg(args, 1)),
                Value::Int(int_arg(args, 2)),
                Value::Int(int_arg(args, 3)),
            ],
        )
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if read_int_field(vm, this, "closed") != 0 {
            return void();
        }
        write_int_field(vm, this, "closed", 1);
        let stream = filter_input_stream_in(vm, this);
        if !stream.is_null() {
            let (class, method) = vm.resolve_virtual_method(stream, "close", "()V")?;
            vm.invoke_method(class, method, vec![Value::Ref(stream)])?;
        }
        void()
    }),
];

/// `BufferedInputStream` rejects reads once closed, even zero-length ones.
fn buffered_input_check_open(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    if read_int_field(vm, this, "closed") != 0 {
        return Err(vm.throw_new("java/io/IOException", Some("Stream closed")));
    }
    Ok(())
}

pub(crate) const BUFFERED_INPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/io/BufferedInputStream",
    Some("java/io/FilterInputStream"),
    &[],
    ACC_PUBLIC,
    &[field("closed", "I", ACC_PRIVATE, None)],
    &BUFFERED_INPUT_STREAM_METHODS,
    None,
);

/// Drain the buffered bytes into the underlying stream.
fn buffered_output_flush_buffer(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    let count = read_int_field(vm, this, "count");
    if count <= 0 {
        return Ok(());
    }
    let class = vm.class_of(this);
    let buf = vm
        .read_ref_field(this, class, "buf", "[B")
        .unwrap_or(ObjectRef::NULL);
    let out = vm
        .read_ref_field(this, class, "out", "Ljava/io/OutputStream;")
        .unwrap_or(ObjectRef::NULL);
    if !buf.is_null() && !out.is_null() {
        let (declaring, method) = vm.resolve_virtual_method(out, "write", "([BII)V")?;
        vm.invoke_method(
            declaring,
            method,
            vec![
                Value::Ref(out),
                Value::Ref(buf),
                Value::Int(0),
                Value::Int(count),
            ],
        )?;
    }
    write_int_field(vm, this, "count", 0);
    Ok(())
}

const BUFFERED_OUTPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "<init>",
        "(Ljava/io/OutputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let out = ref_arg(args, 1);
            if out.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "out", "Ljava/io/OutputStream;")?;
            vm.set_instance_ref(this, declaring, field, out);
            let buf = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, 8192)?;
            let (declaring, field) = vm.find_field(class, "buf", "[B")?;
            vm.set_instance_ref(this, declaring, field, buf);
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/OutputStream;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let size = int_arg(args, 2);
            if size <= 0 {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("size <= 0")));
            }
            let out = ref_arg(args, 1);
            if out.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "out", "Ljava/io/OutputStream;")?;
            vm.set_instance_ref(this, declaring, field, out);
            let buf = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Byte, size as usize)?;
            let (declaring, field) = vm.find_field(class, "buf", "[B")?;
            vm.set_instance_ref(this, declaring, field, buf);
            void()
        },
    ),
    method("write", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        let buf = vm
            .read_ref_field(this, class, "buf", "[B")
            .unwrap_or(ObjectRef::NULL);
        if buf.is_null() {
            let out = vm
                .read_ref_field(this, class, "out", "Ljava/io/OutputStream;")
                .unwrap_or(ObjectRef::NULL);
            let (declaring, method) = vm.resolve_virtual_method(out, "write", "(I)V")?;
            return vm.invoke_method(
                declaring,
                method,
                vec![Value::Ref(out), Value::Int(int_arg(args, 1))],
            );
        }
        if read_int_field(vm, this, "count") >= vm.array_length(buf)? as i32 {
            buffered_output_flush_buffer(vm, this)?;
        }
        let count = read_int_field(vm, this, "count");
        let byte = (int_arg(args, 1) & 0xFF) as u8;
        vm.array_set(buf, count, Value::Int(i32::from(byte as i8)), false)?;
        write_int_field(vm, this, "count", count + 1);
        void()
    }),
    method("write", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let array = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        if array.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        let total = vm.array_length(array)? as i32;
        if offset < 0 || length < 0 || i64::from(offset) + i64::from(length) > i64::from(total) {
            return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
        }
        let class = vm.class_of(this);
        let buf = vm
            .read_ref_field(this, class, "buf", "[B")
            .unwrap_or(ObjectRef::NULL);
        let out = vm
            .read_ref_field(this, class, "out", "Ljava/io/OutputStream;")
            .unwrap_or(ObjectRef::NULL);
        // A request at least as large as the buffer bypasses it entirely.
        if buf.is_null() || length >= vm.array_length(buf)? as i32 {
            buffered_output_flush_buffer(vm, this)?;
            let (declaring, method) = vm.resolve_virtual_method(out, "write", "([BII)V")?;
            return vm.invoke_method(
                declaring,
                method,
                vec![
                    Value::Ref(out),
                    Value::Ref(array),
                    Value::Int(offset),
                    Value::Int(length),
                ],
            );
        }
        if length > vm.array_length(buf)? as i32 - read_int_field(vm, this, "count") {
            buffered_output_flush_buffer(vm, this)?;
        }
        let count = read_int_field(vm, this, "count");
        for index in 0..length {
            let byte = vm.array_get(array, offset + index)?;
            vm.array_set(buf, count + index, byte, false)?;
        }
        write_int_field(vm, this, "count", count + length);
        void()
    }),
    method("flush", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        buffered_output_flush_buffer(vm, this)?;
        let out = vm
            .read_ref_field(this, vm.class_of(this), "out", "Ljava/io/OutputStream;")
            .unwrap_or(ObjectRef::NULL);
        if !out.is_null() {
            let (declaring, method) = vm.resolve_virtual_method(out, "flush", "()V")?;
            vm.invoke_method(declaring, method, vec![Value::Ref(out)])?;
        }
        void()
    }),
];

pub(crate) const BUFFERED_OUTPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/io/BufferedOutputStream",
    Some("java/io/FilterOutputStream"),
    &[],
    ACC_PUBLIC,
    &[
        field("buf", "[B", ACC_PRIVATE, None),
        field("count", "I", ACC_PRIVATE, None),
    ],
    &BUFFERED_OUTPUT_STREAM_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// DataInput / DataOutput
// -------------------------------------------------------------------------------------------

/// Read one byte from an `InputStream` through `read()I`; -1 marks end of stream.
fn data_read_byte(vm: &mut Vm<'_>, stream: ObjectRef) -> Result<i32, VmError> {
    let (class, method) = vm.resolve_virtual_method(stream, "read", "()I")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(stream)])?
        .as_int())
}

/// Read exactly `count` bytes, raising `EOFException` when the stream ends early.
fn data_read_fully(vm: &mut Vm<'_>, stream: ObjectRef, count: usize) -> Result<Vec<u8>, VmError> {
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let byte = data_read_byte(vm, stream)?;
        if byte < 0 {
            return Err(vm.throw_new("java/io/EOFException", None));
        }
        out.push(byte as u8);
    }
    Ok(out)
}

/// Write bytes to an `OutputStream` one byte at a time through `write(I)V`.
fn data_write_bytes(vm: &mut Vm<'_>, stream: ObjectRef, bytes: &[u8]) -> Result<(), VmError> {
    let (class, method) = vm.resolve_virtual_method(stream, "write", "(I)V")?;
    for byte in bytes {
        vm.invoke_method(
            class,
            method,
            vec![Value::Ref(stream), Value::Int(i32::from(*byte))],
        )?;
    }
    Ok(())
}

fn be16(bytes: &[u8]) -> u16 {
    (u16::from(bytes[0]) << 8) | u16::from(bytes[1])
}
fn be32(bytes: &[u8]) -> u32 {
    (u32::from(bytes[0]) << 24)
        | (u32::from(bytes[1]) << 16)
        | (u32::from(bytes[2]) << 8)
        | u32::from(bytes[3])
}
fn be64(bytes: &[u8]) -> u64 {
    (u64::from(be32(&bytes[0..4])) << 32) | u64::from(be32(&bytes[4..8]))
}

/// Encode a string as JDK "modified UTF-8" (the `DataOutput.writeUTF` format).
pub(crate) fn modified_utf8(units: &[u16]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut index = 0;
    while index < units.len() {
        let unit = units[index];
        let ch = if (0xD800..=0xDBFF).contains(&unit)
            && index + 1 < units.len()
            && (0xDC00..=0xDFFF).contains(&units[index + 1])
        {
            let pair = 0x10000
                + ((u32::from(unit) - 0xD800) << 10)
                + (u32::from(units[index + 1]) - 0xDC00);
            index += 1;
            pair
        } else {
            u32::from(unit)
        };
        if (0x0001..=0x007F).contains(&ch) {
            out.push(ch as u8);
        } else if ch <= 0x07FF {
            out.push((0xC0 | (ch >> 6)) as u8);
            out.push((0x80 | (ch & 0x3F)) as u8);
        } else if ch <= 0xFFFF {
            out.push((0xE0 | (ch >> 12)) as u8);
            out.push((0x80 | ((ch >> 6) & 0x3F)) as u8);
            out.push((0x80 | (ch & 0x3F)) as u8);
        } else {
            // Supplementary characters are written as a surrogate pair, each in 3 bytes.
            let hi = 0xD800 + ((ch - 0x10000) >> 10);
            let lo = 0xDC00 + ((ch - 0x10000) & 0x3FF);
            for surrogate in [hi, lo] {
                out.push((0xE0 | (surrogate >> 12)) as u8);
                out.push((0x80 | ((surrogate >> 6) & 0x3F)) as u8);
                out.push((0x80 | (surrogate & 0x3F)) as u8);
            }
        }
        index += 1;
    }
    out
}

/// Decode JDK "modified UTF-8" back into UTF-16 units.
fn decode_modified_utf8(bytes: &[u8]) -> Result<Vec<u16>, String> {
    let mut out = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let b0 = bytes[index];
        let ch = if b0 < 0x80 {
            index += 1;
            u32::from(b0)
        } else if b0 & 0xE0 == 0xC0 {
            if index + 1 >= bytes.len() {
                return Err(alloc::format!("malformed input around byte {index}"));
            }
            if bytes[index + 1] & 0xC0 != 0x80 {
                return Err(alloc::format!("malformed input around byte {index}"));
            }
            let value = ((u32::from(b0) & 0x1F) << 6) | (u32::from(bytes[index + 1]) & 0x3F);
            index += 2;
            value
        } else if b0 & 0xF0 == 0xE0 {
            if index + 2 >= bytes.len() {
                return Err(alloc::format!("malformed input around byte {index}"));
            }
            if bytes[index + 1] & 0xC0 != 0x80 || bytes[index + 2] & 0xC0 != 0x80 {
                return Err(alloc::format!("malformed input around byte {index}"));
            }
            let value = ((u32::from(b0) & 0x0F) << 12)
                | ((u32::from(bytes[index + 1]) & 0x3F) << 6)
                | (u32::from(bytes[index + 2]) & 0x3F);
            index += 3;
            value
        } else {
            return Err(alloc::format!("malformed input around byte {index}"));
        };
        out.push(ch as u16);
    }
    Ok(out)
}

const DATA_INPUT_METHODS: [oxjvm_vm::NativeMethodDef; 15] = [
    method(
        "readFully",
        "([B)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "readFully",
        "([BII)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "skipBytes",
        "(I)I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "readBoolean",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "readByte",
        "()B",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "readUnsignedByte",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "readShort",
        "()S",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "readUnsignedShort",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "readChar",
        "()C",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "readInt",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "readLong",
        "()J",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "readFloat",
        "()F",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "readDouble",
        "()D",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "readLine",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "readUTF",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const DATA_INPUT: oxjvm_vm::NativeClass = class(
    "java/io/DataInput",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &DATA_INPUT_METHODS,
    None,
);

const DATA_OUTPUT_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method(
        "write",
        "(I)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "write",
        "([B)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "write",
        "([BII)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "writeBoolean",
        "(Z)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "writeByte",
        "(I)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "writeShort",
        "(I)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "writeChar",
        "(I)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "writeInt",
        "(I)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "writeLong",
        "(J)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "writeFloat",
        "(F)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "writeDouble",
        "(D)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "writeUTF",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const DATA_OUTPUT: oxjvm_vm::NativeClass = class(
    "java/io/DataOutput",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &DATA_OUTPUT_METHODS,
    None,
);

fn data_input_stream(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field_any(this, "in").unwrap_or(ObjectRef::NULL)
}

const DATA_INPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 16] = [
    method(
        "<init>",
        "(Ljava/io/InputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "in", "Ljava/io/InputStream;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method("readFully", "([B)V", ACC_PUBLIC, |vm, _, args| {
        let array = ref_arg(args, 1);
        let length = vm.array_length(array)? as i32;
        let bytes = data_read_fully(vm, data_input_stream(vm, receiver(args)), length as usize)?;
        for (index, byte) in bytes.iter().enumerate() {
            vm.array_set(
                array,
                index as i32,
                Value::Int(i32::from(*byte as i8)),
                false,
            )?;
        }
        void()
    }),
    method("readFully", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        let array = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        reader_check_range(vm, array, offset, length)?;
        let bytes = data_read_fully(vm, data_input_stream(vm, receiver(args)), length as usize)?;
        for (index, byte) in bytes.iter().enumerate() {
            vm.array_set(
                array,
                offset + index as i32,
                Value::Int(i32::from(*byte as i8)),
                false,
            )?;
        }
        void()
    }),
    method("skipBytes", "(I)I", ACC_PUBLIC, |vm, _, args| {
        let mut remaining = int_arg(args, 1).max(0);
        let stream = data_input_stream(vm, receiver(args));
        let mut skipped = 0;
        while remaining > 0 {
            let byte = data_read_byte(vm, stream)?;
            if byte < 0 {
                break;
            }
            skipped += 1;
            remaining -= 1;
        }
        int(skipped)
    }),
    method("readBoolean", "()Z", ACC_PUBLIC, |vm, _, args| {
        let bytes = data_read_fully(vm, data_input_stream(vm, receiver(args)), 1)?;
        boolean(bytes[0] != 0)
    }),
    method("readByte", "()B", ACC_PUBLIC, |vm, _, args| {
        let bytes = data_read_fully(vm, data_input_stream(vm, receiver(args)), 1)?;
        int(i32::from(bytes[0] as i8))
    }),
    method("readUnsignedByte", "()I", ACC_PUBLIC, |vm, _, args| {
        let bytes = data_read_fully(vm, data_input_stream(vm, receiver(args)), 1)?;
        int(i32::from(bytes[0]))
    }),
    method("readShort", "()S", ACC_PUBLIC, |vm, _, args| {
        let bytes = data_read_fully(vm, data_input_stream(vm, receiver(args)), 2)?;
        int(i32::from(be16(&bytes) as i16))
    }),
    method("readUnsignedShort", "()I", ACC_PUBLIC, |vm, _, args| {
        let bytes = data_read_fully(vm, data_input_stream(vm, receiver(args)), 2)?;
        int(i32::from(be16(&bytes)))
    }),
    method("readChar", "()C", ACC_PUBLIC, |vm, _, args| {
        let bytes = data_read_fully(vm, data_input_stream(vm, receiver(args)), 2)?;
        int(i32::from(be16(&bytes)))
    }),
    method("readInt", "()I", ACC_PUBLIC, |vm, _, args| {
        let bytes = data_read_fully(vm, data_input_stream(vm, receiver(args)), 4)?;
        int(be32(&bytes) as i32)
    }),
    method("readLong", "()J", ACC_PUBLIC, |vm, _, args| {
        let bytes = data_read_fully(vm, data_input_stream(vm, receiver(args)), 8)?;
        long(be64(&bytes) as i64)
    }),
    method("readFloat", "()F", ACC_PUBLIC, |vm, _, args| {
        let bytes = data_read_fully(vm, data_input_stream(vm, receiver(args)), 4)?;
        crate::float(f32::from_bits(be32(&bytes)))
    }),
    method("readDouble", "()D", ACC_PUBLIC, |vm, _, args| {
        let bytes = data_read_fully(vm, data_input_stream(vm, receiver(args)), 8)?;
        crate::double(f64::from_bits(be64(&bytes)))
    }),
    method(
        "readLine",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let stream = data_input_stream(vm, receiver(args));
            let mut units = Vec::new();
            loop {
                let byte = data_read_byte(vm, stream)?;
                if byte < 0 {
                    if units.is_empty() {
                        return object(ObjectRef::NULL);
                    }
                    break;
                }
                if byte == i32::from(b'\n') {
                    break;
                }
                if byte == i32::from(b'\r') {
                    let next = data_read_byte(vm, stream)?;
                    if next >= 0 && next != i32::from(b'\n') {
                        // A lone CR ends the line; push the next byte back when
                        // the underlying stream supports it, like the JDK does.
                        if let Ok((unread_class, unread_method)) =
                            vm.resolve_virtual_method(stream, "unread", "(I)V")
                        {
                            vm.invoke_method(
                                unread_class,
                                unread_method,
                                vec![Value::Ref(stream), Value::Int(next)],
                            )?;
                        }
                    }
                    break;
                }
                units.push(byte as u16);
            }
            object(vm.make_string_utf16(units)?)
        },
    ),
    method(
        "readUTF",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let stream = data_input_stream(vm, receiver(args));
            let header = data_read_fully(vm, stream, 2)?;
            let length = usize::from(be16(&header));
            let bytes = data_read_fully(vm, stream, length)?;
            let units = decode_modified_utf8(&bytes).map_err(|message| {
                vm.throw_new("java/io/UTFDataFormatException", Some(&message))
            })?;
            object(vm.make_string_utf16(units)?)
        },
    ),
];

pub(crate) const DATA_INPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/io/DataInputStream",
    Some("java/io/FilterInputStream"),
    &["java/io/DataInput"],
    ACC_PUBLIC,
    &[],
    &DATA_INPUT_STREAM_METHODS,
    None,
);

fn data_output_stream(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field_any(this, "out")
        .unwrap_or(ObjectRef::NULL)
}

fn data_output_count(vm: &mut Vm<'_>, this: ObjectRef, bytes: &[u8]) -> Result<(), VmError> {
    data_write_bytes(vm, data_output_stream(vm, this), bytes)?;
    let written = read_int_field(vm, this, "written");
    // The JDK saturates the counter at Integer.MAX_VALUE instead of wrapping.
    let total = i64::from(written) + bytes.len() as i64;
    write_int_field(vm, this, "written", total.min(i64::from(i32::MAX)) as i32);
    Ok(())
}

const DATA_OUTPUT_STREAM_METHODS: [oxjvm_vm::NativeMethodDef; 16] = [
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
    method("writeBoolean", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        data_output_count(
            vm,
            receiver(args),
            &[if int_arg(args, 1) != 0 { 1 } else { 0 }],
        )?;
        void()
    }),
    method("writeByte", "(I)V", ACC_PUBLIC, |vm, _, args| {
        data_output_count(vm, receiver(args), &[(int_arg(args, 1) & 0xFF) as u8])?;
        void()
    }),
    method("writeShort", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let value = int_arg(args, 1) as u16;
        data_output_count(vm, receiver(args), &value.to_be_bytes())?;
        void()
    }),
    method("writeChar", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let value = int_arg(args, 1) as u16;
        data_output_count(vm, receiver(args), &value.to_be_bytes())?;
        void()
    }),
    method("writeInt", "(I)V", ACC_PUBLIC, |vm, _, args| {
        data_output_count(vm, receiver(args), &int_arg(args, 1).to_be_bytes())?;
        void()
    }),
    method("writeLong", "(J)V", ACC_PUBLIC, |vm, _, args| {
        data_output_count(vm, receiver(args), &crate::long_arg(args, 1).to_be_bytes())?;
        void()
    }),
    method("writeFloat", "(F)V", ACC_PUBLIC, |vm, _, args| {
        data_output_count(
            vm,
            receiver(args),
            &crate::float_arg(args, 1).to_bits().to_be_bytes(),
        )?;
        void()
    }),
    method("writeDouble", "(D)V", ACC_PUBLIC, |vm, _, args| {
        data_output_count(
            vm,
            receiver(args),
            &crate::double_arg(args, 1).to_bits().to_be_bytes(),
        )?;
        void()
    }),
    method(
        "writeBytes",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = ref_arg(args, 1);
            let units = vm.string_utf16(text).unwrap_or_default().to_vec();
            let bytes: Vec<u8> = units.iter().map(|unit| (*unit & 0xFF) as u8).collect();
            data_output_count(vm, receiver(args), &bytes)?;
            void()
        },
    ),
    method(
        "writeChars",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = ref_arg(args, 1);
            let units = vm.string_utf16(text).unwrap_or_default().to_vec();
            let mut bytes = Vec::with_capacity(units.len() * 2);
            for unit in units {
                bytes.extend_from_slice(&unit.to_be_bytes());
            }
            data_output_count(vm, receiver(args), &bytes)?;
            void()
        },
    ),
    method(
        "writeUTF",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = ref_arg(args, 1);
            let units = vm.string_utf16(text).unwrap_or_default().to_vec();
            let encoded = modified_utf8(&units);
            if encoded.len() > 0xFFFF {
                return Err(vm.throw_new(
                    "java/io/UTFDataFormatException",
                    Some("encoded string too long"),
                ));
            }
            let mut bytes = Vec::with_capacity(encoded.len() + 2);
            bytes.extend_from_slice(&(encoded.len() as u16).to_be_bytes());
            bytes.extend_from_slice(&encoded);
            data_output_count(vm, receiver(args), &bytes)?;
            void()
        },
    ),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        let written = read_int_field(vm, receiver(args), "written");
        int(if written > 0 { written } else { i32::MAX })
    }),
    method("flush", "()V", ACC_PUBLIC, |vm, _, args| {
        let out = data_output_stream(vm, receiver(args));
        if !out.is_null() {
            let (class, method) = vm.resolve_virtual_method(out, "flush", "()V")?;
            vm.invoke_method(class, method, vec![Value::Ref(out)])?;
        }
        void()
    }),
    method("write", "(I)V", ACC_PUBLIC, |vm, _, args| {
        data_output_count(vm, receiver(args), &[(int_arg(args, 1) & 0xFF) as u8])?;
        void()
    }),
    method("write", "([BII)V", ACC_PUBLIC, |vm, _, args| {
        let array = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        reader_check_range(vm, array, offset, length)?;
        let mut bytes = Vec::with_capacity(length as usize);
        for index in 0..length {
            bytes.push(vm.array_get(array, offset + index)?.as_int() as u8);
        }
        data_output_count(vm, receiver(args), &bytes)?;
        void()
    }),
];

pub(crate) const DATA_OUTPUT_STREAM: oxjvm_vm::NativeClass = class(
    "java/io/DataOutputStream",
    Some("java/io/FilterOutputStream"),
    &["java/io/DataOutput"],
    ACC_PUBLIC,
    &[field("written", "I", ACC_PROTECTED, None)],
    &DATA_OUTPUT_STREAM_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// CharArrayReader / CharArrayWriter
// -------------------------------------------------------------------------------------------

const CHAR_ARRAY_READER_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method("<init>", "([C)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let array = ref_arg(args, 1);
        if array.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        let class = vm.class_of(this);
        let length = vm.array_length(array)? as i32;
        let (declaring, field) = vm.find_field(class, "buf", "[C")?;
        vm.set_instance_ref(this, declaring, field, array);
        write_int_field(vm, this, "pos", 0);
        write_int_field(vm, this, "markedPos", 0);
        write_int_field(vm, this, "count", length);
        void()
    }),
    method("<init>", "([CII)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let array = ref_arg(args, 1);
        if array.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        let buf_len = vm.array_length(array)? as i32;
        // The JDK allows offset+length past the buffer end (count is clamped)
        // but rejects negative values and int overflow of the sum; unlike
        // most range checks CharArrayReader reports IllegalArgumentException.
        let end = i64::from(offset) + i64::from(length);
        if offset < 0 || offset > buf_len || length < 0 || end > i64::from(i32::MAX) {
            return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
        }
        let class = vm.class_of(this);
        let (declaring, field) = vm.find_field(class, "buf", "[C")?;
        vm.set_instance_ref(this, declaring, field, array);
        write_int_field(vm, this, "pos", offset);
        write_int_field(vm, this, "markedPos", offset);
        write_int_field(vm, this, "count", end.min(i64::from(buf_len)) as i32);
        void()
    }),
    method("read", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        let pos = read_int_field(vm, this, "pos");
        let count = read_int_field(vm, this, "count");
        if pos >= count {
            return int(-1);
        }
        let buf = vm
            .read_ref_field(this, class, "buf", "[C")
            .unwrap_or(ObjectRef::NULL);
        let value = vm.array_get(buf, pos)?.as_int();
        write_int_field(vm, this, "pos", pos + 1);
        int(value)
    }),
    method("read", "([CII)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        let target = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        reader_check_range(vm, target, offset, length)?;
        if length == 0 {
            return int(0);
        }
        let pos = read_int_field(vm, this, "pos");
        let count = read_int_field(vm, this, "count");
        if pos >= count {
            return int(-1);
        }
        let buf = vm
            .read_ref_field(this, class, "buf", "[C")
            .unwrap_or(ObjectRef::NULL);
        let available = count - pos;
        let n = available.min(length);
        for index in 0..n {
            let value = vm.array_get(buf, pos + index)?;
            vm.array_set(target, offset + index, value, false)?;
        }
        write_int_field(vm, this, "pos", pos + n);
        int(n)
    }),
    method("skip", "(J)J", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let pos = read_int_field(vm, this, "pos");
        let count = read_int_field(vm, this, "count");
        let available = i64::from((count - pos).max(0));
        let requested = crate::long_arg(args, 1).max(0);
        let n = available.min(requested);
        write_int_field(vm, this, "pos", pos + n as i32);
        long(n)
    }),
    method("ready", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let pos = read_int_field(vm, this, "pos");
        let count = read_int_field(vm, this, "count");
        boolean(pos < count)
    }),
    method("markSupported", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
    method("mark", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let pos = read_int_field(vm, this, "pos");
        write_int_field(vm, this, "markedPos", pos);
        void()
    }),
    method("reset", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let marked = read_int_field(vm, this, "markedPos");
        write_int_field(vm, this, "pos", marked);
        void()
    }),
];

pub(crate) const CHAR_ARRAY_READER: oxjvm_vm::NativeClass = class(
    "java/io/CharArrayReader",
    Some("java/io/Reader"),
    &[],
    ACC_PUBLIC,
    &[
        field("buf", "[C", ACC_PROTECTED, None),
        field("pos", "I", ACC_PROTECTED, None),
        field("markedPos", "I", ACC_PROTECTED, None),
        field("count", "I", ACC_PROTECTED, None),
    ],
    &CHAR_ARRAY_READER_METHODS,
    None,
);

fn char_array_writer_units(vm: &mut Vm<'_>, this: ObjectRef) -> Vec<u16> {
    let class = vm.class_of(this);
    let buf = vm
        .read_ref_field(this, class, "buf", "[C")
        .unwrap_or(ObjectRef::NULL);
    let count = read_int_field(vm, this, "count").max(0) as usize;
    let mut units = Vec::with_capacity(count);
    for index in 0..count {
        if let Ok(value) = vm.array_get(buf, index as i32) {
            units.push(value.as_int() as u16);
        }
    }
    units
}

fn char_array_writer_set(vm: &mut Vm<'_>, this: ObjectRef, units: &[u16]) -> Result<(), VmError> {
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Char, units.len())?;
    for (index, unit) in units.iter().enumerate() {
        vm.array_set(array, index as i32, Value::Int(i32::from(*unit)), false)?;
    }
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "buf", "[C")?;
    vm.set_instance_ref(this, declaring, field, array);
    write_int_field(vm, this, "count", units.len() as i32);
    Ok(())
}

const CHAR_ARRAY_WRITER_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        char_array_writer_set(vm, receiver(args), &[])?;
        void()
    }),
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        if int_arg(args, 1) < 0 {
            return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("negative size")));
        }
        char_array_writer_set(vm, receiver(args), &[])?;
        void()
    }),
    method("write", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let mut units = char_array_writer_units(vm, this);
        units.push((int_arg(args, 1) & 0xFFFF) as u16);
        char_array_writer_set(vm, this, &units)?;
        void()
    }),
    method("write", "([CII)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let array = ref_arg(args, 1);
        let offset = int_arg(args, 2);
        let length = int_arg(args, 3);
        reader_check_range(vm, array, offset, length)?;
        let mut units = char_array_writer_units(vm, this);
        for index in 0..length {
            units.push(vm.array_get(array, offset + index)?.as_int() as u16);
        }
        char_array_writer_set(vm, this, &units)?;
        void()
    }),
    method(
        "write",
        "(Ljava/lang/String;II)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = ref_arg(args, 1);
            let offset = int_arg(args, 2).max(0) as usize;
            let length = int_arg(args, 3).max(0) as usize;
            let source = vm.string_utf16(text).unwrap_or_default().to_vec();
            let end = (offset + length).min(source.len());
            let mut units = char_array_writer_units(vm, this);
            units.extend_from_slice(&source[offset.min(source.len())..end]);
            char_array_writer_set(vm, this, &units)?;
            void()
        },
    ),
    method(
        "writeTo",
        "(Ljava/io/Writer;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let writer = ref_arg(args, 1);
            let units = char_array_writer_units(vm, this);
            let text = vm.make_string_utf16(units)?;
            let (class, method) =
                vm.resolve_virtual_method(writer, "write", "(Ljava/lang/String;)V")?;
            vm.invoke_method(class, method, vec![Value::Ref(writer), Value::Ref(text)])?;
            void()
        },
    ),
    method("reset", "()V", ACC_PUBLIC, |vm, _, args| {
        char_array_writer_set(vm, receiver(args), &[])?;
        void()
    }),
    method("toCharArray", "()[C", ACC_PUBLIC, |vm, _, args| {
        let units = char_array_writer_units(vm, receiver(args));
        let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Char, units.len())?;
        for (index, unit) in units.iter().enumerate() {
            vm.array_set(array, index as i32, Value::Int(i32::from(*unit)), false)?;
        }
        object(array)
    }),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "count"))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let units = char_array_writer_units(vm, receiver(args));
            object(vm.make_string_utf16(units)?)
        },
    ),
    method("flush", "()V", ACC_PUBLIC, |_, _, _| void()),
];

pub(crate) const CHAR_ARRAY_WRITER: oxjvm_vm::NativeClass = class(
    "java/io/CharArrayWriter",
    Some("java/io/Writer"),
    &[],
    ACC_PUBLIC,
    &[
        field("buf", "[C", ACC_PROTECTED, None),
        field("count", "I", ACC_PROTECTED, None),
    ],
    &CHAR_ARRAY_WRITER_METHODS,
    None,
);
