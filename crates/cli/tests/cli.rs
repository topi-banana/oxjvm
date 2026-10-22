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
fn runs_p1_reflection_fixture_with_files_and_urls() {
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../classfile/tests/fixtures");
    let output = Command::new(env!("CARGO_BIN_EXE_oxjvm"))
        .args(["run", "-cp"])
        .arg(&fixtures)
        .arg("demo.P1Reflection")
        .output()
        .expect("spawn oxjvm");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("parameter=sample"), "{stdout}");
    assert!(
        stdout.contains("classApi=true:0:true:java.lang.Number"),
        "{stdout}"
    );
    assert!(stdout.contains("url=file:/tmp/a/c.txt"), "{stdout}");
}

#[test]
fn runs_p1_nio_fixture_with_file_systems_and_channels() {
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../classfile/tests/fixtures");
    let tmp = std::env::temp_dir();
    let output = Command::new(env!("CARGO_BIN_EXE_oxjvm"))
        .args(["run", "-cp"])
        .arg(&fixtures)
        .arg(format!("-Djava.io.tmpdir={}", tmp.display()))
        .arg("demo.P1Nio")
        .output()
        .expect("spawn oxjvm");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(":true:5:hello:5:text/plain:true:"),
        "{stdout}"
    );
    assert!(stdout.trim_end().ends_with(":true"), "{stdout}");
}

#[test]
fn drops_a_definition_that_would_replace_a_pinned_property() {
    // JEP 223 pins the platform properties, so `-Djava.vm.name=Unexpected` must not make the VM
    // claim a different name; an ordinary property is still applied.
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../classfile/tests/fixtures");
    let output = Command::new(env!("CARGO_BIN_EXE_oxjvm"))
        .args(["run", "-cp"])
        .arg(&fixtures)
        .arg("-Djava.vm.name=Unexpected")
        .arg("-Djava.vm.vendor=Unexpected")
        .arg("-Doxjvm.test.property=applied")
        .arg("demo.P2Props")
        .output()
        .expect("spawn oxjvm");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("pinned=oxjvm:oxjvm:oxjvm:"), "{stdout}");
    assert!(stdout.contains("plain=applied"), "{stdout}");
    // The guest may still change a pinned property itself.
    assert!(stdout.contains("setByGuest=guest"), "{stdout}");
}

#[test]
fn runs_the_p2_api_fixture() {
    // The fixture touches the filesystem, so it runs through the CLI, which has one.
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../classfile/tests/fixtures");
    let output = Command::new(env!("CARGO_BIN_EXE_oxjvm"))
        .args(["run", "-cp"])
        .arg(&fixtures)
        .arg("demo.P2Apis")
        .output()
        .expect("spawn oxjvm");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "paths=a|b/c|/a/b/d|/x/y|d",
        "grouping={a=[aa, ab], b=[bc]}|{a=2, b=1}",
        "bits=31:23:0:63:3:8:1:8",
        "enums=[TUESDAY, WEDNESDAY]:[TUESDAY]:[MONDAY, TUESDAY, WEDNESDAY]",
        "hex=0x1.0p0:0x1.0p-1:0x1.0p0:0x1.8p0:0x0.0p0:-0x0.0p0:NaN:Infinity",
        "transfer=5:hello",
        "entries=k:7:k=7",
        "properties=true:true",
        "chars=4:ab",
        "redirects=READ:/tmp/x:redirect to read from file \"/tmp/x\":WRITE:redirect to write to file \"/tmp/y\":INHERIT:PIPE:WRITE",
    ] {
        assert!(
            stdout.contains(expected),
            "{expected} missing from {stdout}"
        );
    }
}

#[test]
fn runs_the_p2_process_fixture() {
    // A guest that starts another program needs the host, so the fixture runs through the CLI.
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../classfile/tests/fixtures");
    let output = Command::new(env!("CARGO_BIN_EXE_oxjvm"))
        .args(["run", "-cp"])
        .arg(&fixtures)
        .arg("demo.P2Process")
        .output()
        .expect("spawn oxjvm");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("started=hello:0"), "{stdout}");
    assert!(stdout.contains("executed=hello:0"), "{stdout}");
    // The handle of the running program: its id, that it is alive, and the programs the host sees.
    assert!(stdout.contains(":true:"), "{stdout}");
}

#[test]
fn runs_the_p2_files_fixture() {
    // The fixture touches the filesystem, so it runs through the CLI, which has one.
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../classfile/tests/fixtures");
    let output = Command::new(env!("CARGO_BIN_EXE_oxjvm"))
        .args(["run", "-cp"])
        .arg(&fixtures)
        .arg("demo.P2Files")
        .output()
        .expect("spawn oxjvm");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("copy=11:hello world"), "{stdout}");
    assert!(
        stdout.contains("lines=[one, two, three]:one|two|three|"),
        "{stdout}"
    );
    assert!(stdout.contains("compare=-1:5:true"), "{stdout}");
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

#[test]
fn accepts_the_memory_options() {
    let dir = scratch_dir("options");
    fs::write(dir.join("Hello.class"), build_hello_class()).expect("write class");
    for option in ["-Xmx256m", "-Xmx=256m", "-Xmx", "-Xms64m", "-Xss1m"] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_oxjvm"));
        command.args(["run", "-cp"]).arg(&dir);
        if option == "-Xmx" {
            command.arg(option).arg("256m");
        } else {
            command.arg(option);
        }
        let output = command.arg("Hello").output().expect("spawn oxjvm");
        assert!(
            output.status.success(),
            "{option} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "Hello from oxjvm!\n"
        );
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rejects_malformed_memory_options() {
    for option in ["-Xmx", "-Xmx12x", "-Xms", "-Xss", "-Xfoo1m"] {
        let output = Command::new(env!("CARGO_BIN_EXE_oxjvm"))
            .args(["run", option, "Hello"])
            .output()
            .expect("spawn oxjvm");
        assert!(!output.status.success(), "{option} was accepted");
        assert!(
            !String::from_utf8_lossy(&output.stderr).is_empty(),
            "{option} failed silently"
        );
    }
}

#[test]
fn rejects_an_initial_heap_larger_than_the_maximum() {
    let output = Command::new(env!("CARGO_BIN_EXE_oxjvm"))
        .args(["run", "-Xms64m", "-Xmx16m", "Hello"])
        .output()
        .expect("spawn oxjvm");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("-Xms"));
}

#[test]
fn a_tiny_heap_fails_to_start() {
    let dir = scratch_dir("tiny");
    fs::write(dir.join("Hello.class"), build_hello_class()).expect("write class");
    let output = Command::new(env!("CARGO_BIN_EXE_oxjvm"))
        .args(["run", "-Xmx64", "-cp"])
        .arg(&dir)
        .arg("Hello")
        .output()
        .expect("spawn oxjvm");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("OutOfMemoryError") || stderr.contains("heap space"),
        "stderr: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}
