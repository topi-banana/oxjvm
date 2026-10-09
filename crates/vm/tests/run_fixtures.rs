//! End-to-end execution of real `javac` fixtures (JDK 25, class-file major 69).
//!
//! The fixtures are the same corpus the class-file codec round-trips; here they are *run*, and the
//! results are compared against the Java sources they were compiled from. This is the strongest
//! available "same behavior" check without a JDK in the loop.

use std::fs;
use std::path::{Path, PathBuf};

use oxjvm_classfile::attribute::{Attribute, AttributeData, CodeAttribute, ExceptionHandler};
use oxjvm_classfile::{ClassFile, ConstantPool, CpInfo, FieldInfo, MethodInfo};
use oxjvm_platform::{Host, MemoryClasses, Stream};
use oxjvm_vm::{ArrayComponent, Value, Vm, VmError};

/// A host holding every fixture class plus captured standard output.
#[derive(Default)]
struct FixtureHost {
    classes: MemoryClasses,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl FixtureHost {
    fn new() -> Self {
        Self::default()
    }

    fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

impl Host for FixtureHost {
    fn load_class(&mut self, internal_name: &str) -> Option<Vec<u8>> {
        self.classes.get(internal_name).map(<[u8]>::to_vec)
    }

    fn write(&mut self, stream: Stream, bytes: &[u8]) -> Result<(), oxjvm_platform::HostError> {
        match stream {
            Stream::Stdout => self.stdout.extend_from_slice(bytes),
            Stream::Stderr => self.stderr.extend_from_slice(bytes),
        }
        Ok(())
    }

    fn current_time_millis(&mut self) -> i64 {
        1_700_000_000_000
    }

    fn nano_time(&mut self) -> i64 {
        1_700_000_000_000_000_000
    }
}

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../classfile/tests/fixtures")
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("fixtures") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "class") {
            out.push(path);
        }
    }
}

fn fixture_host() -> FixtureHost {
    let mut host = FixtureHost::new();
    let mut files = Vec::new();
    walk(&fixtures_root(), &mut files);
    for path in files {
        let bytes = fs::read(&path).expect("read fixture");
        let class = ClassFile::read(&bytes).expect("parse fixture");
        let name = class.this_name().expect("name").to_string();
        host.classes.insert(&name, bytes);
    }
    host
}

fn call_int(vm: &mut Vm<'_>, class_name: &str, method: &str, args: Vec<Value>) -> i32 {
    let class = vm.resolve_class(class_name).expect("class");
    let (declaring, index) = vm.find_method(class, method, "(I)I").expect("method");
    vm.invoke_method(declaring, index, args)
        .expect("invoke")
        .as_int()
}

#[test]
fn runs_integer_loops() {
    let mut host = fixture_host();
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let class = vm.resolve_class("demo/Loops").expect("Loops");
    let (sum_class, sum) = vm.find_method(class, "sum", "(I)I").expect("sum");
    // The fixture has no constructor use, so call `sum` on a fresh instance.
    let instance = vm.new_instance(class).expect("instance");
    for (n, expected) in [(0i32, 0i32), (1, 0), (5, 10), (100, 4950)] {
        let result = vm
            .invoke_method(sum_class, sum, vec![Value::Ref(instance), Value::Int(n)])
            .expect("sum");
        assert_eq!(result.as_int(), expected, "sum({n})");
    }
    let (count_class, count) = vm.find_method(class, "count", "(I)I").expect("count");
    for (n, expected) in [(1i32, 1i32), (4, 4), (0, 1)] {
        let result = vm
            .invoke_method(
                count_class,
                count,
                vec![Value::Ref(instance), Value::Int(n)],
            )
            .expect("count");
        assert_eq!(result.as_int(), expected, "count({n})");
    }
    assert_ne!(sum_class.raw(), u32::MAX);
}

#[test]
fn runs_branches_and_locals() {
    let mut host = fixture_host();
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let branchy = vm.resolve_class("demo/Branchy").expect("Branchy");
    let instance = vm.new_instance(branchy).expect("instance");
    let (class, method) = vm.find_method(branchy, "max", "(II)I").expect("max");
    assert_eq!(
        vm.invoke_method(
            class,
            method,
            vec![Value::Ref(instance), Value::Int(3), Value::Int(9)]
        )
        .expect("max")
        .as_int(),
        9
    );
    let (class, method) = vm.find_method(branchy, "clamp", "(I)I").expect("clamp");
    assert_eq!(
        vm.invoke_method(class, method, vec![Value::Ref(instance), Value::Int(-5)])
            .expect("clamp")
            .as_int(),
        0
    );
    assert_eq!(
        vm.invoke_method(class, method, vec![Value::Ref(instance), Value::Int(150)])
            .expect("clamp")
            .as_int(),
        100
    );
    assert_eq!(
        vm.invoke_method(class, method, vec![Value::Ref(instance), Value::Int(42)])
            .expect("clamp")
            .as_int(),
        42
    );

    let locals = vm.resolve_class("demo/Locals").expect("Locals");
    let instance = vm.new_instance(locals).expect("instance");
    assert_eq!(
        call_int(
            &mut vm,
            "demo/Locals",
            "compute",
            vec![Value::Ref(instance), Value::Int(20)]
        ),
        41
    );
}

#[test]
fn runs_switches() {
    let mut host = fixture_host();
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let class = vm.resolve_class("demo/Switches").expect("Switches");
    let instance = vm.new_instance(class).expect("instance");
    let (declaring, method) = vm
        .find_method(class, "allReturn", "(I)I")
        .expect("allReturn");
    for (x, expected) in [(0, 10), (1, 11), (2, -1), (100, -1)] {
        assert_eq!(
            vm.invoke_method(declaring, method, vec![Value::Ref(instance), Value::Int(x)])
                .expect("allReturn")
                .as_int(),
            expected
        );
    }
    let (declaring, method) = vm.find_method(class, "sparse", "(I)I").expect("sparse");
    for (x, expected) in [(1, 100), (100, 1), (42, -1)] {
        assert_eq!(
            vm.invoke_method(declaring, method, vec![Value::Ref(instance), Value::Int(x)])
                .expect("sparse")
                .as_int(),
            expected
        );
    }
    let (declaring, method) = vm.find_method(class, "dense", "(I)V").expect("dense");
    let _ = (declaring, method);
}

#[test]
fn runs_array_operations() {
    let mut host = fixture_host();
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let class = vm.resolve_class("demo/Arrays").expect("Arrays");
    let instance = vm.new_instance(class).expect("instance");
    let array = vm.allocate_array_of(ArrayComponent::Int, 3).expect("array");
    for (index, value) in [7, 8, 9].into_iter().enumerate() {
        vm.array_set(array, index as i32, Value::Int(value), false)
            .expect("set");
    }
    let first_class = vm.resolve_class("demo/Arrays").expect("Arrays");
    let (declaring, method) = vm
        .find_method(first_class, "first", "([I)I")
        .expect("first");
    assert_eq!(
        vm.invoke_method(
            declaring,
            method,
            vec![Value::Ref(instance), Value::Ref(array)]
        )
        .expect("first")
        .as_int(),
        7
    );
    let (declaring, method) = vm
        .find_method(first_class, "firstTwo", "()I")
        .expect("firstTwo");
    assert_eq!(
        vm.invoke_method(declaring, method, vec![Value::Ref(instance)])
            .expect("firstTwo")
            .as_int(),
        7
    );
    let (declaring, method) = vm
        .find_method(first_class, "lenNew", "()I")
        .expect("lenNew");
    assert_eq!(
        vm.invoke_method(declaring, method, vec![Value::Ref(instance)])
            .expect("lenNew")
            .as_int(),
        1
    );
    let (declaring, method) = vm
        .find_method(first_class, "grid", "(II)[[I")
        .expect("grid");
    let grid = vm
        .invoke_method(
            declaring,
            method,
            vec![Value::Ref(instance), Value::Int(2), Value::Int(3)],
        )
        .expect("grid");
    assert_eq!(vm.array_length(grid.as_ref()).expect("length"), 2);
    let row = vm.array_get(grid.as_ref(), 1).expect("row");
    assert_eq!(vm.array_length(row.as_ref()).expect("row length"), 3);
}

#[test]
fn runs_virtual_and_interface_dispatch() {
    let mut host = fixture_host();
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let class = vm.resolve_class("demo/InvokeSpecialCalls").expect("class");
    let instance = vm.new_instance(class).expect("instance");
    let (declaring, constructor) = vm
        .find_method(class, "<init>", "(I)V")
        .expect("constructor");
    vm.invoke_method(
        declaring,
        constructor,
        vec![Value::Ref(instance), Value::Int(1)],
    )
    .expect("constructor");
    let (declaring, method) = vm
        .find_method(class, "callSuperclass", "(I)I")
        .expect("callSuperclass");
    assert_eq!(
        vm.invoke_method(declaring, method, vec![Value::Ref(instance), Value::Int(5)])
            .expect("callSuperclass")
            .as_int(),
        6
    );
    let (declaring, method) = vm
        .find_method(class, "callInterface", "(I)I")
        .expect("callInterface");
    assert_eq!(
        vm.invoke_method(declaring, method, vec![Value::Ref(instance), Value::Int(5)])
            .expect("callInterface")
            .as_int(),
        6
    );
}

#[test]
fn runs_string_builder_fixture() {
    let mut host = fixture_host();
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let class = vm.resolve_class("demo/Sb").expect("Sb");
    let instance = vm.new_instance(class).expect("instance");
    for (method, argument, expected) in
        [("greet", "world", "Hello, world!"), ("excl", "wow", "wow!")]
    {
        let (declaring, index) = vm
            .find_method(class, method, "(Ljava/lang/String;)Ljava/lang/String;")
            .expect("method");
        let argument = vm.make_string(argument).expect("argument");
        let result = vm
            .invoke_method(
                declaring,
                index,
                vec![Value::Ref(instance), Value::Ref(argument)],
            )
            .expect("invoke");
        assert_eq!(vm.string_value(result.as_ref()).as_deref(), Some(expected));
    }
    let (declaring, index) = vm
        .find_method(class, "seeded", "(II)Ljava/lang/String;")
        .expect("seeded");
    let result = vm
        .invoke_method(
            declaring,
            index,
            vec![Value::Ref(instance), Value::Int(1), Value::Int(2)],
        )
        .expect("seeded");
    assert_eq!(vm.string_value(result.as_ref()).as_deref(), Some("12"));
    let (declaring, index) = vm
        .find_method(class, "len", "(Ljava/lang/String;)I")
        .expect("len");
    let argument = vm.make_string("abcde").expect("argument");
    let result = vm
        .invoke_method(
            declaring,
            index,
            vec![Value::Ref(instance), Value::Ref(argument)],
        )
        .expect("len");
    assert_eq!(result.as_int(), 5);
}

// -------------------------------------------------------------------------------------------
// A generated hello-world class, exercising the native `System`/`PrintStream`/`String` classes.
// -------------------------------------------------------------------------------------------

fn build_hello_class() -> Vec<u8> {
    let mut pool = ConstantPool::new();
    let class_name = pool.push_class("Hello");
    let object_name = pool.push_class("java/lang/Object");
    let system_name = pool.push_class("java/lang/System");
    let print_stream = pool.push_class("java/io/PrintStream");
    let string_name = pool.push_class("java/lang/String");
    let message = pool.push_string("Hello, world!");
    let string_array = pool.push_class("[Ljava/lang/String;");

    // getstatic System.out:Ljava/io/PrintStream;
    let out_name_and_type = pool.push_name_and_type("out", "Ljava/io/PrintStream;");
    let out_field = pool.push(CpInfo::Fieldref {
        class: system_name,
        name_and_type: out_name_and_type,
    });
    // invokevirtual PrintStream.println(Ljava/lang/String;)V
    let println_name_and_type = pool.push_name_and_type("println", "(Ljava/lang/String;)V");
    let println = pool.push(CpInfo::Methodref {
        class: print_stream,
        name_and_type: println_name_and_type,
    });
    let _ = (string_name, string_array);
    assert!(message < 256);

    let mut code = Vec::new();
    code.push(0xb2); // getstatic
    code.extend_from_slice(&out_field.to_be_bytes());
    code.push(0x12); // ldc
    code.push(message as u8);
    code.push(0xb6); // invokevirtual
    code.extend_from_slice(&println.to_be_bytes());
    code.push(0xb1); // return

    let mut class = ClassFile {
        minor_version: 0,
        major_version: 52,
        constant_pool: pool,
        access_flags: oxjvm_classfile::flags::ACC_PUBLIC | oxjvm_classfile::flags::ACC_SUPER,
        this_class: class_name,
        super_class: object_name,
        interfaces: Vec::new(),
        fields: Vec::<FieldInfo>::new(),
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
                exception_table: Vec::<ExceptionHandler>::new(),
                attributes: Vec::new(),
            }),
        }],
    });
    let main_name_index = class.constant_pool.intern_utf8("main");
    let main_descriptor_index = class.constant_pool.intern_utf8("([Ljava/lang/String;)V");
    class.methods.push(MethodInfo {
        access_flags: oxjvm_classfile::flags::ACC_PUBLIC | oxjvm_classfile::flags::ACC_STATIC,
        name_index: main_name_index,
        descriptor_index: main_descriptor_index,
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
    // Fix the constructor's `invokespecial Object.<init>` operand now that indices are final.
    let name_and_type = class.constant_pool.push_name_and_type("<init>", "()V");
    let object_init_index = class.constant_pool.push(CpInfo::Methodref {
        class: class.super_class,
        name_and_type,
    });
    if let AttributeData::Code(code) = &mut class.methods[0].attributes[0].data {
        code.code[2] = (object_init_index >> 8) as u8;
        code.code[3] = (object_init_index & 0xFF) as u8;
    }
    class.write()
}

#[test]
fn runs_generated_hello_world() {
    let mut host = FixtureHost::new();
    let hello = build_hello_class();
    host.classes.insert("Hello", hello);
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let status = vm.run_main("Hello", &[]).expect("run");
    assert_eq!(status, 0);
    assert_eq!(host.stdout_text(), "Hello, world!\n");
}

#[test]
fn runs_generated_hello_world_with_arguments() {
    let mut host = FixtureHost::new();
    host.classes.insert("Hello", build_hello_class());
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let status = vm.run_main("Hello", &["a", "b"]).expect("run");
    assert_eq!(status, 0);
    assert_eq!(host.stdout_text(), "Hello, world!\n");
}

#[test]
fn reports_a_missing_main_class() {
    let mut host = FixtureHost::new();
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let status = vm.run_main("NoSuchClass", &[]).expect("run");
    assert_eq!(status, 1);
    assert!(host.stderr_text().contains("NoSuchClass"));
}

#[test]
fn throws_and_catches_java_exceptions() {
    // `demo/Tries` catches checked exceptions from `parse`, but its helper throws io exceptions
    // only when the string is empty in some paths; `IntCarried` exercises int-carried booleans and
    // chars, including an array round-trip, with no exception at all.
    let mut host = fixture_host();
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let class = vm.resolve_class("demo/IntCarried").expect("IntCarried");
    let instance = vm.new_instance(class).expect("instance");
    let (declaring, method) = vm
        .find_method(class, "booleanReturn", "()Z")
        .expect("booleanReturn");
    assert_eq!(
        vm.invoke_method(declaring, method, vec![Value::Ref(instance)])
            .expect("booleanReturn")
            .as_int(),
        1
    );
    let (declaring, method) = vm
        .find_method(class, "charReturn", "()C")
        .expect("charReturn");
    assert_eq!(
        vm.invoke_method(declaring, method, vec![Value::Ref(instance)])
            .expect("charReturn")
            .as_int(),
        i32::from('A' as u16)
    );
}

#[test]
fn throws_arithmetic_exceptions() {
    // A generated class dividing by zero must surface ArithmeticException.
    let mut host = FixtureHost::new();
    host.classes.insert("Div", build_div_class());
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let class = vm.resolve_class("Div").expect("Div");
    let instance = vm.new_instance(class).expect("instance");
    let (declaring, method) = vm.find_method(class, "div", "(II)I").expect("div");
    assert_eq!(
        vm.invoke_method(
            declaring,
            method,
            vec![Value::Ref(instance), Value::Int(6), Value::Int(2)]
        )
        .expect("div")
        .as_int(),
        3
    );
    let error = vm
        .invoke_method(
            declaring,
            method,
            vec![Value::Ref(instance), Value::Int(6), Value::Int(0)],
        )
        .expect_err("division by zero");
    let VmError::Thrown(exception) = error else {
        panic!("expected a Java exception");
    };
    let class = vm.class_of(exception);
    assert_eq!(vm.class_name(class), "java/lang/ArithmeticException");
}

fn build_div_class() -> Vec<u8> {
    let mut pool = ConstantPool::new();
    let class_name = pool.push_class("Div");
    let object_name = pool.push_class("java/lang/Object");
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
    let init = class.constant_pool.intern_utf8("<init>");
    class.methods.push(MethodInfo {
        access_flags: oxjvm_classfile::flags::ACC_PUBLIC,
        name_index: init,
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
    let div_name = class.constant_pool.intern_utf8("div");
    let div_descriptor = class.constant_pool.intern_utf8("(II)I");
    class.methods.push(MethodInfo {
        access_flags: oxjvm_classfile::flags::ACC_PUBLIC,
        name_index: div_name,
        descriptor_index: div_descriptor,
        attributes: vec![Attribute {
            name_index: class.constant_pool.intern_utf8("Code"),
            data: AttributeData::Code(CodeAttribute {
                max_stack: 2,
                max_locals: 3,
                code: vec![0x1b, 0x1c, 0x6c, 0xac],
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

fn build_safe_parse_class() -> Vec<u8> {
    let mut pool = ConstantPool::new();
    let class_name = pool.push_class("Safe");
    let object_name = pool.push_class("java/lang/Object");
    let integer_name = pool.push_class("java/lang/Integer");
    let parse_name_and_type = pool.push_name_and_type("parseInt", "(Ljava/lang/String;)I");
    let parse_int = pool.push(CpInfo::Methodref {
        class: integer_name,
        name_and_type: parse_name_and_type,
    });
    let exception_class = pool.push_class("java/lang/NumberFormatException");
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
    // safeParse([Ljava/lang/String; ... ) -> int with try/catch.
    let code = vec![
        0x2a, // aload_0
        0xb8,
        (parse_int >> 8) as u8,
        (parse_int & 0xFF) as u8, // invokestatic parseInt
        0xac,                     // ireturn
        0x4c,                     // astore_1
        0x10,
        0xFF, // bipush -1
        0xac, // ireturn
    ];
    let name_index = class.constant_pool.intern_utf8("safeParse");
    let descriptor_index = class.constant_pool.intern_utf8("(Ljava/lang/String;)I");
    class.methods.push(MethodInfo {
        access_flags: oxjvm_classfile::flags::ACC_PUBLIC | oxjvm_classfile::flags::ACC_STATIC,
        name_index,
        descriptor_index,
        attributes: vec![Attribute {
            name_index: class.constant_pool.intern_utf8("Code"),
            data: AttributeData::Code(CodeAttribute {
                max_stack: 1,
                max_locals: 2,
                code,
                exception_table: vec![ExceptionHandler {
                    start_pc: 0,
                    end_pc: 4,
                    handler_pc: 5,
                    catch_type: exception_class,
                }],
                attributes: Vec::new(),
            }),
        }],
    });
    class.write()
}

#[test]
fn catches_native_number_format_exceptions() {
    let mut host = FixtureHost::new();
    host.classes.insert("Safe", build_safe_parse_class());
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let class = vm.resolve_class("Safe").expect("Safe");
    let (declaring, method) = vm
        .find_method(class, "safeParse", "(Ljava/lang/String;)I")
        .expect("safeParse");
    let valid = vm.make_string("42").expect("string");
    assert_eq!(
        vm.invoke_method(declaring, method, vec![Value::Ref(valid)])
            .expect("valid")
            .as_int(),
        42
    );
    let invalid = vm.make_string("x").expect("string");
    assert_eq!(
        vm.invoke_method(declaring, method, vec![Value::Ref(invalid)])
            .expect("invalid")
            .as_int(),
        -1
    );
}

#[test]
fn collects_unreachable_objects_and_keeps_roots() {
    let mut host = fixture_host();
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let class = vm.resolve_class("demo/Locals").expect("Locals");
    let rooted = vm.new_instance(class).expect("rooted");
    for _ in 0..64 {
        let _ = vm.new_instance(class).expect("temporary");
    }
    let before = vm.heap.live;
    vm.gc();
    assert!(vm.heap.live < before, "gc should reclaim temporaries");
    assert!(
        vm.heap.get(rooted).is_none(),
        "references held only by Rust code are not roots; precise collection may reap them"
    );
    // Explicitly protect a root and verify it survives.
    let protected = vm.new_instance(class).expect("protected");
    vm.protect(protected);
    vm.gc();
    assert!(vm.heap.get(protected).is_some());
    vm.unprotect(protected);
}

#[test]
fn defaults_system_properties() {
    let mut host = fixture_host();
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let class = vm
        .resolve_class("demo/SystemProperties")
        .expect("SystemProperties");

    for (method, expected) in [
        ("vmName", Some("oxjvm")),
        ("missingWithDefault", Some("fallback")),
        ("missingWithNullDefault", None),
        ("missing", None),
    ] {
        let (declaring, index) = vm
            .find_method(class, method, "()Ljava/lang/String;")
            .expect(method);
        let value = vm
            .invoke_method(declaring, index, Vec::new())
            .expect(method);
        assert_eq!(
            vm.string_value(value.as_ref()).as_deref(),
            expected,
            "{method}"
        );
    }

    // A null key throws NullPointerException.
    let (declaring, index) = vm
        .find_method(class, "nullKey", "()Ljava/lang/String;")
        .expect("nullKey");
    let result = vm.invoke_method(declaring, index, Vec::new());
    let Err(error) = result else {
        panic!("expected a Java exception")
    };
    let VmError::Thrown(exception) = error else {
        panic!("expected a Java exception");
    };
    let thrown = vm.class_of(exception);
    assert_eq!(vm.class_name(thrown), "java/lang/NullPointerException");
}

#[test]
fn rejects_corrupted_bytecode() {
    let mut bytes = build_div_class();
    // `div` code begins with `iload_1 iload_2 idiv ireturn`; replace `ireturn` with an
    // undefined opcode and the verifier must refuse the class.
    let position = bytes
        .windows(4)
        .rposition(|window| window == [0x1b, 0x1c, 0x6c, 0xac])
        .expect("div code");
    bytes[position + 3] = 0xcb;
    let mut host = FixtureHost::new();
    host.classes.insert("Div", bytes);
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let error = vm.resolve_class("Div").expect_err("must be rejected");
    match error {
        VmError::InvalidCode { message, .. } => {
            assert!(
                message.contains("invalid"),
                "unexpected verifier message: {message}"
            );
        }
        other => panic!("expected InvalidCode, got {other:?}"),
    }
}
