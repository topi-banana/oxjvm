//! CLI integration: build a tiny class, run it through the real binary, and disassemble it.

use std::fs;
use std::process::Command;

use oxjvm_classfile::attribute::{Attribute, AttributeData, CodeAttribute};
use oxjvm_classfile::{ClassFile, ConstantPool, CpInfo, MethodInfo};

fn build_hello_class() -> Vec<u8> {
    let mut pool = ConstantPool::new();
    let class_name = pool.push_class("Hello");
    let object_name = pool.push_class("java/lang/Object");
    let system_name = pool.push_class("java/lang/System");
    let print_stream = pool.push_class("java/io/PrintStream");
    let message = pool.push_string("Hello from oxjvm!");
    let out_name_and_type = pool.push_name_and_type("out", "Ljava/io/PrintStream;");
    let out_field = pool.push(CpInfo::Fieldref {
        class: system_name,
        name_and_type: out_name_and_type,
    });
    let println_name_and_type = pool.push_name_and_type("println", "(Ljava/lang/String;)V");
    let println = pool.push(CpInfo::Methodref {
        class: print_stream,
        name_and_type: println_name_and_type,
    });
    assert!(message < 256);

    let mut code = Vec::new();
    code.push(0xb2);
    code.extend_from_slice(&out_field.to_be_bytes());
    code.push(0x12);
    code.push(message as u8);
    code.push(0xb6);
    code.extend_from_slice(&println.to_be_bytes());
    code.push(0xb1);

    let mut class = ClassFile {
        minor_version: 0,
        major_version: 52,
        constant_pool: pool,
        access_flags: oxjvm_classfile::flags::ACC_PUBLIC | oxjvm_classfile::flags::ACC_SUPER,
        this_class: class_name,
        super_class: object_name,
        interfaces: Vec::new(),
        fields: Vec::new(),
        methods: Vec::new(),
        attributes: Vec::new(),
    };
    let constructor_descriptor = class.constant_pool.intern_utf8("()V");
    let constructor_name = class.constant_pool.intern_utf8("<init>");
    class.methods.push(MethodInfo {
        access_flags: oxjvm_classfile::flags::ACC_PUBLIC,
        name_index: constructor_name,
        descriptor_index: constructor_descriptor,
        attributes: vec![Attribute {
            name_index: class.constant_pool.intern_utf8("Code"),
            data: AttributeData::Code(CodeAttribute {
                max_stack: 1,
                max_locals: 1,
                code: vec![0x2a, 0xb7, 0x00, 0x00, 0xb1],
                exception_table: Vec::new(),
                attributes: Vec::new(),
            }),
        }],
    });
    let main_name = class.constant_pool.intern_utf8("main");
    let main_descriptor = class.constant_pool.intern_utf8("([Ljava/lang/String;)V");
    class.methods.push(MethodInfo {
        access_flags: oxjvm_classfile::flags::ACC_PUBLIC | oxjvm_classfile::flags::ACC_STATIC,
        name_index: main_name,
        descriptor_index: main_descriptor,
        attributes: vec![Attribute {
            name_index: class.constant_pool.intern_utf8("Code"),
            data: AttributeData::Code(CodeAttribute {
                max_stack: 2,
                max_locals: 1,
                code,
                exception_table: Vec::new(),
                attributes: Vec::new(),
            }),
        }],
    });
    let name_and_type = class.constant_pool.push_name_and_type("<init>", "()V");
    let object_init = class.constant_pool.push(CpInfo::Methodref {
        class: class.super_class,
        name_and_type,
    });
    if let AttributeData::Code(code) = &mut class.methods[0].attributes[0].data {
        code.code[2] = (object_init >> 8) as u8;
        code.code[3] = (object_init & 0xFF) as u8;
    }
    class.write()
}

fn scratch_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("oxjvm-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

#[test]
fn runs_a_class_file_from_a_directory_classpath() {
    let dir = scratch_dir("run");
    fs::write(dir.join("Hello.class"), build_hello_class()).expect("write class");
    let output = Command::new(env!("CARGO_BIN_EXE_oxjvm"))
        .args(["run", "-cp"])
        .arg(&dir)
        .arg("Hello")
        .output()
        .expect("spawn oxjvm");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Hello from oxjvm!\n"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn disassembles_and_inspects_a_class_file() {
    let dir = scratch_dir("disasm");
    let path = dir.join("Hello.class");
    fs::write(&path, build_hello_class()).expect("write class");
    let output = Command::new(env!("CARGO_BIN_EXE_oxjvm"))
        .arg("disasm")
        .arg(&path)
        .output()
        .expect("spawn oxjvm");
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("main([Ljava/lang/String;)V"), "{text}");
    assert!(text.contains("getstatic"), "{text}");
    assert!(text.contains("invokevirtual"), "{text}");

    let output = Command::new(env!("CARGO_BIN_EXE_oxjvm"))
        .arg("inspect")
        .arg(&path)
        .output()
        .expect("spawn oxjvm");
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("class Hello"), "{text}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn reports_a_missing_main_class_with_status_one() {
    let output = Command::new(env!("CARGO_BIN_EXE_oxjvm"))
        .args(["run", "-cp", "/nonexistent", "Nope"])
        .output()
        .expect("spawn oxjvm");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Nope"));
}
