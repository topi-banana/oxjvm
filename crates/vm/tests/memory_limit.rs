//! The heap budget (`-Xms`/`-Xmx`) and the stack depth limit (`-Xss`).

use oxjvm_classfile::attribute::{Attribute, AttributeData, CodeAttribute};
use oxjvm_classfile::{ClassFile, ConstantPool, CpInfo, MethodInfo};
use oxjvm_platform::{Host, HostError, MemoryClasses, Stream};
use oxjvm_vm::{ArrayComponent, ObjectData, Value, Vm, VmError, VmOptions};

/// A host with no class path and captured output.
#[derive(Default)]
struct TestHost {
    classes: MemoryClasses,
    output: Vec<u8>,
}

impl Host for TestHost {
    fn load_class(&mut self, internal_name: &str) -> Option<Vec<u8>> {
        self.classes.get(internal_name).map(<[u8]>::to_vec)
    }

    fn write(&mut self, _stream: Stream, bytes: &[u8]) -> Result<(), HostError> {
        self.output.extend_from_slice(bytes);
        Ok(())
    }
}

/// Build a class with a static method that calls itself forever.
fn recursive_class() -> Vec<u8> {
    let mut pool = ConstantPool::new();
    let class_name = pool.push_class("Recurse");
    let object_name = pool.push_class("java/lang/Object");
    let recurse_name_and_type = pool.push_name_and_type("recurse", "()I");
    let recurse = pool.push(CpInfo::Methodref {
        class: class_name,
        name_and_type: recurse_name_and_type,
    });
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
    // `invokestatic Recurse.recurse:()I; ireturn`
    class.methods.push(MethodInfo {
        access_flags: oxjvm_classfile::flags::ACC_PUBLIC | oxjvm_classfile::flags::ACC_STATIC,
        name_index: class.constant_pool.intern_utf8("recurse"),
        descriptor_index: class.constant_pool.intern_utf8("()I"),
        attributes: vec![Attribute {
            name_index: class.constant_pool.intern_utf8("Code"),
            data: AttributeData::Code(CodeAttribute {
                max_stack: 1,
                max_locals: 0,
                code: vec![0xb8, (recurse >> 8) as u8, (recurse & 0xFF) as u8, 0xac],
                exception_table: Vec::new(),
                attributes: Vec::new(),
            }),
        }],
    });
    class.write()
}

/// The class of a thrown exception.
fn thrown_class(vm: &Vm<'_>, error: VmError) -> String {
    let VmError::Thrown(exception) = error else {
        panic!("expected a thrown exception, got {error:?}");
    };
    vm.class_name(vm.class_of(exception)).to_string()
}

#[test]
fn an_unbounded_heap_never_runs_out() {
    let mut host = TestHost::default();
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let array = vm
        .allocate_array_of(ArrayComponent::Byte, 1 << 20)
        .expect("array");
    assert_eq!(vm.array_length(array).expect("length"), 1 << 20);
    assert_eq!(vm.heap_max_bytes(), None);
}

#[test]
fn a_full_heap_raises_out_of_memory() {
    let mut host = TestHost::default();
    let mut vm = Vm::with_options(
        &mut host,
        oxjvm_java::natives(),
        VmOptions::with_max_heap_bytes(256 * 1024),
    );
    // The runtime itself fits in the budget, so a small allocation still works.
    let small = vm
        .allocate_array_of(ArrayComponent::Byte, 1024)
        .expect("small array");
    assert_eq!(vm.array_length(small).expect("length"), 1024);
    // A large one does not.
    let error = vm
        .allocate_array_of(ArrayComponent::Byte, 1 << 20)
        .expect_err("out of memory");
    assert_eq!(thrown_class(&vm, error), "java/lang/OutOfMemoryError");
}

#[test]
fn a_huge_array_raises_out_of_memory_instead_of_exhausting_the_host() {
    let mut host = TestHost::default();
    let mut vm = Vm::with_options(
        &mut host,
        oxjvm_java::natives(),
        VmOptions::with_max_heap_bytes(1024 * 1024),
    );
    // Two gigabytes of bytes: the budget has to be checked before the elements are allocated.
    let error = vm
        .allocate_array_of(ArrayComponent::Byte, 2 * 1024 * 1024 * 1024)
        .expect_err("out of memory");
    assert_eq!(thrown_class(&vm, error), "java/lang/OutOfMemoryError");
}

#[test]
fn the_collector_reclaims_garbage_before_the_heap_is_full() {
    let mut host = TestHost::default();
    let mut vm = Vm::with_options(
        &mut host,
        oxjvm_java::natives(),
        VmOptions::with_max_heap_bytes(1024 * 1024),
    );
    // None of these arrays is reachable once the next one is allocated, so the collector has to
    // keep the heap inside the budget.
    for _ in 0..512 {
        let array = vm
            .allocate_array_of(ArrayComponent::Byte, 16 * 1024)
            .expect("array");
        assert_eq!(vm.array_length(array).expect("length"), 16 * 1024);
    }
    assert!(
        vm.heap_used_bytes() <= 1024 * 1024,
        "the heap grew to {} bytes",
        vm.heap_used_bytes()
    );
}

#[test]
fn the_working_size_grows_toward_the_maximum() {
    let mut host = TestHost::default();
    let options = VmOptions::with_heap_sizes(64 * 1024, 1024 * 1024);
    let mut vm = Vm::with_options(&mut host, oxjvm_java::natives(), options);
    assert_eq!(vm.heap_total_bytes(), 64 * 1024);
    // A live set that does not fit in the working size makes the collector grow it.
    let array = vm
        .allocate_array_of(ArrayComponent::Byte, 256 * 1024)
        .expect("array");
    vm.protect(array);
    vm.gc();
    assert_eq!(vm.array_length(array).expect("length"), 256 * 1024);
    assert!(
        vm.heap_total_bytes() > 64 * 1024,
        "the working size stayed at {}",
        vm.heap_total_bytes()
    );
    assert!(vm.heap_total_bytes() <= 1024 * 1024);
}

#[test]
fn the_free_memory_is_what_the_total_leaves_over() {
    let mut host = TestHost::default();
    let options = VmOptions::with_heap_sizes(1024 * 1024, 4 * 1024 * 1024);
    let mut vm = Vm::with_options(&mut host, oxjvm_java::natives(), options);
    let class = vm.resolve_class("java/lang/Runtime").expect("Runtime");
    let (declaring, index) = vm
        .find_method(class, "freeMemory", "()J")
        .expect("freeMemory");
    let free = vm
        .invoke_method(declaring, index, vec![])
        .expect("freeMemory")
        .as_long();
    let (declaring, index) = vm
        .find_method(class, "totalMemory", "()J")
        .expect("totalMemory");
    let total = vm
        .invoke_method(declaring, index, vec![])
        .expect("totalMemory")
        .as_long();
    let used = vm.heap_used_bytes() as i64;
    assert_eq!(free, total - used);
    assert!(free >= 0);
}

#[test]
fn the_used_bytes_follow_the_live_set() {
    let mut host = TestHost::default();
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let before = vm.heap_used_bytes();
    let array = vm
        .allocate_array_of(ArrayComponent::Int, 1024)
        .expect("array");
    assert_eq!(vm.array_length(array).expect("length"), 1024);
    // 1024 four-byte elements, plus the object header.
    assert!(vm.heap_used_bytes() >= before + 4096);
    vm.gc();
    let after = vm.heap_used_bytes();
    assert!(after < vm.heap_used_bytes() + 4096 + 4096);
    assert!(after < before + 4096 + 4096);
}

#[test]
fn runtime_reports_the_configured_budget() {
    let mut host = TestHost::default();
    let options = VmOptions::with_heap_sizes(256 * 1024, 1024 * 1024);
    let mut vm = Vm::with_options(&mut host, oxjvm_java::natives(), options);
    let class = vm.resolve_class("java/lang/Runtime").expect("Runtime");
    let (declaring, index) = vm
        .find_method(class, "maxMemory", "()J")
        .expect("maxMemory");
    let maximum = vm
        .invoke_method(declaring, index, vec![])
        .expect("maxMemory")
        .as_long();
    assert_eq!(maximum, 1024 * 1024);
    let (declaring, index) = vm
        .find_method(class, "totalMemory", "()J")
        .expect("totalMemory");
    let total = vm
        .invoke_method(declaring, index, vec![])
        .expect("totalMemory")
        .as_long();
    assert!(total >= 256 * 1024, "total memory was {total}");
}

#[test]
fn an_unbounded_heap_reports_no_maximum() {
    let mut host = TestHost::default();
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let class = vm.resolve_class("java/lang/Runtime").expect("Runtime");
    let (declaring, index) = vm
        .find_method(class, "maxMemory", "()J")
        .expect("maxMemory");
    let maximum = vm
        .invoke_method(declaring, index, vec![])
        .expect("maxMemory")
        .as_long();
    assert_eq!(maximum, i64::MAX);
}

#[test]
fn a_shallow_stack_raises_stack_overflow() {
    let mut host = TestHost::default();
    host.classes.insert("Recurse", recursive_class());
    let options = VmOptions {
        max_stack_frames: 64,
        ..VmOptions::default()
    };
    let mut vm = Vm::with_options(&mut host, oxjvm_java::natives(), options);
    let class = vm.resolve_class("Recurse").expect("Recurse");
    let (declaring, index) = vm.find_method(class, "recurse", "()I").expect("recurse");
    let error = vm
        .invoke_method(declaring, index, vec![])
        .expect_err("the stack is exhausted");
    assert_eq!(thrown_class(&vm, error), "java/lang/StackOverflowError");
}

/// Fill the heap up to a few bytes below its budget, so that the next allocation does not
/// fit and the collector has to run to reclaim the garbage.
fn fill_the_heap(vm: &mut Vm<'_>, limit: u64) -> oxjvm_vm::ObjectRef {
    let headroom = limit - vm.heap_used_bytes();
    vm.allocate_array_of(ArrayComponent::Byte, (headroom - 32) as usize)
        .expect("filler")
}

#[test]
fn a_collection_during_interning_keeps_the_object_alive() {
    let mut host = TestHost::default();
    let limit = 256 * 1024;
    let mut vm = Vm::with_options(
        &mut host,
        oxjvm_java::natives(),
        VmOptions::with_max_heap_bytes(limit),
    );
    let class = vm.resolve_class("java/lang/Thread").expect("Thread");
    let object = vm.new_instance(class).expect("instance");
    let _filler = fill_the_heap(&mut vm, limit);
    // Interning the name may collect, and the object is not reachable from a root yet.
    vm.set_string_field(object, class, "name", "worker")
        .expect("set name");
    assert_eq!(
        vm.class_name(vm.class_of(object)),
        "java/lang/Thread",
        "the object was collected and its slot reused"
    );
    assert_eq!(
        vm.read_string_field(object, class, "name").as_deref(),
        Some("worker"),
        "the name was lost"
    );
}

#[test]
fn a_payload_replacement_that_does_not_fit_raises_out_of_memory() {
    let mut host = TestHost::default();
    let limit = 256 * 1024;
    let mut vm = Vm::with_options(
        &mut host,
        oxjvm_java::natives(),
        VmOptions::with_max_heap_bytes(limit),
    );
    let class = vm.resolve_class("java/lang/String").expect("String");
    let object = vm.new_instance(class).expect("instance");
    // The filler stays alive, so the collector cannot make room for the replacement.
    let filler = fill_the_heap(&mut vm, limit);
    vm.protect(filler);
    // The replacement is charged against the budget like any other allocation.
    let error = vm
        .replace_payload(object, ObjectData::String(vec![0u16; 64 * 1024]))
        .expect_err("out of memory");
    assert_eq!(thrown_class(&vm, error), "java/lang/OutOfMemoryError");
    // The payload was left alone.
    assert!(vm.string_utf16(object).is_none());
}

#[test]
fn a_payload_replacement_that_fits_keeps_the_accounting_straight() {
    let mut host = TestHost::default();
    let limit = 256 * 1024;
    let mut vm = Vm::with_options(
        &mut host,
        oxjvm_java::natives(),
        VmOptions::with_max_heap_bytes(limit),
    );
    let class = vm.resolve_class("java/lang/String").expect("String");
    let object = vm.new_instance(class).expect("instance");
    let before = vm.heap_used_bytes();
    vm.replace_payload(object, ObjectData::String(vec![0u16; 1024]))
        .expect("replace");
    // A thousand UTF-16 code units, plus the object header.
    assert_eq!(vm.heap_used_bytes(), before + 2 * 1024);
}

#[test]
fn a_string_that_does_not_fit_raises_out_of_memory() {
    let mut host = TestHost::default();
    let limit = 256 * 1024;
    let mut vm = Vm::with_options(
        &mut host,
        oxjvm_java::natives(),
        VmOptions::with_max_heap_bytes(limit),
    );
    let class = vm.resolve_class("java/lang/String").expect("String");
    // The characters take half the budget; the string they make takes the other half,
    // which does not fit next to them.
    let chars = vm
        .allocate_array_of(ArrayComponent::Char, 100 * 1024)
        .expect("chars");
    let string = vm.new_instance(class).expect("string");
    // The collector runs inside the native `String(char[])` constructor now; in guest code the
    // caller's frame roots both objects, so register them the same way here.
    vm.protect(chars);
    vm.protect(string);
    let (declaring, index) = vm.find_method(class, "<init>", "([C)V").expect("ctor");
    let error = vm
        .invoke_method(
            declaring,
            index,
            vec![Value::Ref(string), Value::Ref(chars)],
        )
        .expect_err("out of memory");
    assert_eq!(thrown_class(&vm, error), "java/lang/OutOfMemoryError");
}
