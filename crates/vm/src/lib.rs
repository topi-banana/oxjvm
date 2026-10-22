#![no_std]
#![deny(unsafe_code)]
//! `oxjvm-vm`: a pure-Rust JVM class-file runtime.
//!
//! The runtime is `no_std + alloc`, has no I/O of its own, and executes Java bytecode exactly as
//! JVMS 6 defines each instruction. Host interaction — class bytes, standard output, clocks,
//! process exit — passes through [`oxjvm_platform::Host`].
//!
//! # Life cycle
//!
//! ```no_run
//! # use oxjvm_platform::MemoryClasses;
//! let mut host = MemoryClasses::new(); // supply the class bytes through a Host
//! let mut vm = oxjvm_vm::Vm::new(&mut host, oxjvm_java::natives());
//! let status = vm.run_main("Hello", &[]).unwrap();
//! assert_eq!(status, 0);
//! ```
//!
//! `run_main` loads the class, runs its `<clinit>`, invokes `main([Ljava/lang/String;)V`, and
//! drives the cooperative scheduler until the main thread and every non-daemon thread finish.
//! Uncaught exceptions print a stack trace and yield exit status 1, like the `java` launcher.

extern crate alloc;

pub mod class;
pub mod error;
pub mod format;
pub mod frame;
pub mod heap;
pub mod interpreter;
pub mod loader;
pub mod options;
pub mod support;
pub mod thread;
pub mod value;
pub mod verifier;

pub use class::{
    ArrayComponent, Class, ClassId, ClassKind, ClassState, Field, Method, NativeClass,
    NativeClinit, NativeConstant, NativeContext, NativeFieldDef, NativeFn, NativeMethodDef,
};
pub use error::VmError;
pub use frame::Frame;
pub use heap::{ArrayData, BacktraceFrame, MethodHandleValue, ObjectData, OutOfMemory};
pub use loader::ResolvedMethod;
pub use options::{VmOptions, parse_memory_size};
pub use thread::{JavaThread, ThreadState};
pub use value::{ObjectRef, Value};

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_platform::{Host, Stream};

use crate::class::ClassTable;
use crate::heap::Heap;
use crate::value::ObjectRef as Ref;

/// A resolved `invokedynamic` call site.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) enum CallSite {
    /// A string-concatenation site.
    Concat {
        /// The recipe with `\u{1}` placeholders (constants are in `constants`).
        recipe: String,
        /// The static constants substituted for `\u{1}`.
        constants: Vec<ConcatConstant>,
    },
    /// A lambda/metafactory site.
    Lambda {
        /// The synthetic class implementing the functional interface.
        class: ClassId,
    },
    /// A `ConstantBootstraps`-style direct target.
    Direct {
        /// Target class.
        class: ClassId,
        /// Target method index.
        method: u32,
        /// Whether the target is static.
        is_static: bool,
    },
}

/// A string-concat constant.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) enum ConcatConstant {
    /// A string constant.
    Str(String),
    /// An `int` constant.
    Int(i32),
    /// A `long` constant.
    Long(i64),
    /// A `float` constant.
    Float(f32),
    /// A `double` constant.
    Double(f64),
    /// A `char` constant.
    Char(u16),
}

/// The definition of a dynamically created lambda class.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct LambdaDef {
    /// The target of the lambda body.
    pub target: MethodHandleValue,
    /// The SAM method name.
    pub sam_name: String,
    /// The SAM method descriptor.
    pub sam_descriptor: String,
    /// Field slots holding captured values, in call-site order.
    pub captured_slots: Vec<u16>,
}

/// A pending resume action for a thread that was suspended mid-operation.
#[derive(Debug, Clone, PartialEq)]
pub enum Resume {
    /// Nothing pending.
    None,
    /// Re-enter a monitor, then continue at the already-advanced pc.
    EnterMonitor {
        /// The monitor object.
        object: Ref,
    },
    /// Enter a monitor, then invoke a method.
    Invoke {
        /// The class declaring the method.
        class: ClassId,
        /// The method index.
        method: u32,
        /// Receiver plus arguments (receiver first for instance methods).
        args: Vec<Value>,
        /// The caller's return pc.
        return_pc: usize,
    },
    /// Enter a monitor, then call a native method.
    NativeCall {
        /// The class declaring the method.
        class: ClassId,
        /// The method index.
        method: u32,
        /// Receiver plus arguments.
        args: Vec<Value>,
    },
    /// Return from `Object.wait` after re-acquiring the monitor.
    WaitReturn {
        /// The monitor object.
        object: Ref,
        /// Whether the wait timed out.
        timed_out: bool,
    },
}

/// The oxjvm virtual machine.
pub struct Vm<'a> {
    /// The host boundary.
    pub host: &'a mut dyn Host,
    /// All loaded classes.
    pub classes: ClassTable,
    /// The object heap.
    pub heap: Heap,
    /// Native class definitions, consulted before the host class path.
    pub(crate) natives: &'static [NativeClass],
    /// Cooperative thread table.
    pub(crate) threads: Vec<JavaThread>,
    /// Index of the scheduled thread.
    pub(crate) current: usize,
    /// Thread objects to thread indices.
    pub(crate) thread_objects: BTreeMap<Ref, usize>,
    /// Interned strings.
    pub(crate) interned: BTreeMap<String, Ref>,
    /// Classes currently being loaded, for circularity detection.
    pub(crate) loading: BTreeSet<String>,
    /// Resolved dynamic call sites.
    pub(crate) call_sites: BTreeMap<(u32, u16), CallSite>,
    /// Resolved field references, cached per `(class, cp_index)`.
    pub(crate) resolved_fields: BTreeMap<(u32, u16), (ClassId, u32, bool)>,
    /// Resolved method references, cached per `(class, cp_index)`.
    pub(crate) resolved_methods: BTreeMap<(u32, u16), ResolvedMethod>,
    /// Definitions of dynamically created lambda classes.
    pub(crate) lambda_defs: BTreeMap<ClassId, LambdaDef>,
    /// Captured backtraces by throwable.
    pub(crate) backtraces: BTreeMap<Ref, Vec<BacktraceFrame>>,
    /// Nested-call exception floor, per thread (see [`JavaThread::floor`]).
    pub(crate) return_slot: Option<Value>,
    /// Depth of nested native frames, to suppress GC while a host function holds references on the
    /// Rust stack.
    pub(crate) in_native: u32,
    /// Extra GC roots registered by natives.
    pub(crate) temp_roots: Vec<Ref>,
    /// `System.out`.
    pub(crate) out: Ref,
    /// `System.err`.
    pub(crate) err: Ref,
    /// Index of the main thread.
    pub(crate) main_thread: usize,
    /// Virtual clock used by sleeping threads.
    pub(crate) virtual_time: i64,
    /// RNG state for `Math.random`/`Random`.
    pub(crate) random_state: u64,
    /// Allocations until the next collection.
    pub(crate) gc_countdown: usize,
    /// Whether a collection is due.
    pub(crate) gc_requested: bool,
    /// The runtime configuration (heap budget, stack depth).
    pub options: VmOptions,
    /// The heap size the collector currently works within. It starts at `-Xms` and grows toward
    /// `-Xmx` as the live set demands.
    pub(crate) commit_bytes: u64,
    /// Set while the `OutOfMemoryError` is being created, so the exception itself can be
    /// allocated past the budget.
    pub(crate) oom_throw: bool,
    /// The error that stopped the boot sequence, if any.
    pub(crate) boot_error: Option<VmError>,
    /// Print each executed instruction (debugging aid).
    pub(crate) trace: bool,
    /// Primitive classes by base type.
    pub(crate) primitive_classes: BTreeMap<oxjvm_classfile::BaseType, ClassId>,
    /// Guest-set system properties (`System.setProperty`), checked before the defaults/host.
    pub(crate) properties: BTreeMap<String, String>,
    /// Whether the boot sequence has completed.
    pub(crate) booted: bool,
    /// Cached boxed primitives, keyed by wrapper tag and value, so the wrapper classes can keep
    /// their identity guarantees.
    pub(crate) boxed_cache: BTreeMap<(u8, i64), Ref>,
}

impl<'a> Vm<'a> {
    /// Create a VM over a host and a native class registry.
    ///
    /// The registry must contain `java/lang/Object`; every other native class is loaded on first
    /// use. [`crate::loader`] describes the boot sequence.
    ///
    /// The heap is unbounded; use [`Vm::with_options`] to give it a budget.
    #[must_use]
    pub fn new(host: &'a mut dyn Host, natives: &'static [NativeClass]) -> Self {
        Self::with_options(host, natives, VmOptions::default())
    }

    /// Create a VM over a host and a native class registry, with an explicit configuration.
    ///
    /// The registry must contain `java/lang/Object`; every other native class is loaded on first
    /// use. [`crate::loader`] describes the boot sequence.
    #[must_use]
    pub fn with_options(
        host: &'a mut dyn Host,
        natives: &'static [NativeClass],
        options: VmOptions,
    ) -> Self {
        let random_state = host.random_seed();
        let limit_bytes = options.max_heap_bytes;
        let commit_bytes = options.commit_bytes();
        let mut vm = Self {
            host,
            classes: ClassTable::new(),
            heap: Heap::new(),
            natives,
            threads: Vec::new(),
            current: 0,
            thread_objects: BTreeMap::new(),
            interned: BTreeMap::new(),
            loading: BTreeSet::new(),
            call_sites: BTreeMap::new(),
            resolved_fields: BTreeMap::new(),
            resolved_methods: BTreeMap::new(),
            lambda_defs: BTreeMap::new(),
            backtraces: BTreeMap::new(),
            return_slot: None,
            in_native: 0,
            temp_roots: Vec::new(),
            out: Ref::NULL,
            err: Ref::NULL,
            main_thread: 0,
            virtual_time: 0,
            random_state,
            gc_countdown: 64 * 1024,
            gc_requested: false,
            options,
            commit_bytes,
            oom_throw: false,
            boot_error: None,
            trace: false,
            primitive_classes: BTreeMap::new(),
            properties: BTreeMap::new(),
            booted: false,
            boxed_cache: BTreeMap::new(),
        };
        vm.heap.set_limit_bytes(limit_bytes);
        vm.boot();
        vm
    }

    /// Boot the runtime: load the root classes and create the main thread.
    ///
    /// A failure here (a heap budget too small to hold the root classes, say) is remembered and
    /// reported by the first [`Vm::run_main`].
    ///
    /// # Panics
    ///
    /// Panics when the native registry has no `java/lang/Object`, which is a host programming
    /// error rather than a guest condition.
    fn boot(&mut self) {
        if let Err(error) = self.boot_runtime() {
            self.boot_error = Some(error);
        }
    }

    fn boot_runtime(&mut self) -> Result<(), VmError> {
        let object = self
            .natives
            .iter()
            .find(|native| native.name == "java/lang/Object")
            .expect("the native registry must define java/lang/Object");
        self.load_native_class(object)?;
        // Primitive type classes, for `Integer.TYPE`, `void.class`, and array components.
        for base in [
            oxjvm_classfile::BaseType::Boolean,
            oxjvm_classfile::BaseType::Byte,
            oxjvm_classfile::BaseType::Char,
            oxjvm_classfile::BaseType::Short,
            oxjvm_classfile::BaseType::Int,
            oxjvm_classfile::BaseType::Long,
            oxjvm_classfile::BaseType::Float,
            oxjvm_classfile::BaseType::Double,
            oxjvm_classfile::BaseType::Void,
        ] {
            let name = match base {
                oxjvm_classfile::BaseType::Boolean => "boolean",
                oxjvm_classfile::BaseType::Byte => "byte",
                oxjvm_classfile::BaseType::Char => "char",
                oxjvm_classfile::BaseType::Short => "short",
                oxjvm_classfile::BaseType::Int => "int",
                oxjvm_classfile::BaseType::Long => "long",
                oxjvm_classfile::BaseType::Float => "float",
                oxjvm_classfile::BaseType::Double => "double",
                oxjvm_classfile::BaseType::Void => "void",
            };
            let object_class = ClassId::OBJECT;
            let id = self.classes.insert(Class {
                id: ClassId(0),
                name: name.into(),
                super_class: Some(object_class),
                interfaces: Vec::new(),
                access_flags: oxjvm_classfile::flags::ACC_PUBLIC
                    | oxjvm_classfile::flags::ACC_FINAL
                    | oxjvm_classfile::flags::ACC_ABSTRACT,
                state: ClassState::Initialized,
                constant_pool: oxjvm_classfile::ConstantPool::new(),
                fields: Vec::new(),
                methods: Vec::new(),
                static_values: Vec::new(),
                instance_defaults: Vec::new(),
                instance_slots: 0,
                source_file: None,
                bootstrap_methods: Vec::new(),
                class_object: None,
                kind: ClassKind::Primitive,
                component: None,
                component_class: None,
                primitive: Some(base),
                native_definition: None,
                initialization_error: None,
                nest_host: None,
                enclosing_class_name: None,
                declaring_class_name: None,
                permitted_subclasses: Vec::new(),
                is_enum: false,
                annotations: Vec::new(),
                verified: true,
            });
            self.classes.get_mut(id).id = id;
            self.primitive_classes.insert(base, id);
        }
        // The main thread.
        let main = self.create_thread("main", false, None)?;
        self.main_thread = main;
        self.booted = true;
        Ok(())
    }

    /// Run a main class to completion.
    ///
    /// Returns the process exit status: `0` on normal completion, `1` when `main` throws an
    /// uncaught exception (which is also printed to `System.err`), or the status passed to
    /// `System.exit`.
    pub fn run_main(&mut self, main_class: &str, args: &[&str]) -> Result<i32, VmError> {
        self.booted = true;
        // A boot that ran out of heap is reported here, like the `java` launcher does.
        if let Some(error) = self.boot_error.take() {
            return Err(error);
        }
        // `java a.b.C` names the class in binary form; the internal name uses slashes.
        let internal_name = main_class.replace('.', "/");
        let class = match self.resolve_class(&internal_name) {
            Ok(class) => class,
            Err(error) => {
                let message = alloc::format!(
                    "Error: Could not find or load main class {}",
                    main_class.replace('/', ".")
                );
                let _ = self.host.write(Stream::Stderr, message.as_bytes());
                let _ = self.host.write(Stream::Stderr, b"\n");
                let cause = alloc::format!("Caused by: {error}");
                let _ = self.host.write(Stream::Stderr, cause.as_bytes());
                let _ = self.host.write(Stream::Stderr, b"\n");
                if let VmError::Thrown(exception) = error {
                    self.print_uncaught("main", exception);
                }
                return Ok(1);
            }
        };
        let Some(method) = self.find_method(class, "main", "([Ljava/lang/String;)V") else {
            let message = alloc::format!(
                "Error: Main method not found in class {}",
                main_class.replace('/', ".")
            );
            let _ = self.host.write(Stream::Stderr, message.as_bytes());
            let _ = self.host.write(Stream::Stderr, b"\n");
            return Ok(1);
        };
        let args_array = self.make_args_array(args)?;
        let main_thread = self.current;
        let result = self.invoke_method(method.0, method.1, vec![Value::Ref(args_array)]);
        let mut status = match result {
            Ok(_) => 0,
            Err(VmError::Thrown(exception)) => {
                self.print_uncaught("main", exception);
                1
            }
            Err(VmError::Exit(status)) => return Ok(status),
            Err(other) => return Err(other),
        };
        // The JVM keeps running until every non-daemon thread has finished.
        while self.has_alive_non_daemon() {
            self.schedule_step()?;
        }
        // `main`'s uncaught exception can be observed by a nested invocation rather than the
        // launcher's own (`<clinit>` driven by another thread runs the whole scheduler, so it may
        // terminate `main` for us).  `handle_uncaught` has already printed the trace; surface the
        // non-zero status the launcher owes in that case too.
        if status == 0
            && self
                .threads
                .get(main_thread)
                .is_some_and(|thread| thread.uncaught.is_some())
        {
            status = 1;
        }
        Ok(status)
    }

    /// Run a `public static void main(String[])` from in-memory class bytes, for tests and wasm.
    pub fn run_main_with_classes(
        &mut self,
        main_class: &str,
        args: &[&str],
        classes: &[&[u8]],
    ) -> Result<i32, VmError> {
        for bytes in classes {
            let parsed = oxjvm_classfile::ClassFile::read(bytes)
                .map_err(|error| VmError::invalid_code("class path", error.to_string()))?;
            let name = parsed
                .this_name()
                .map_err(|error| VmError::invalid_code("class path", error.to_string()))?
                .to_string();
            if self.classes.by_name(&name).is_none() {
                self.define_class(&name, bytes)?;
            }
        }
        self.run_main(main_class, args)
    }

    /// Build the `String[]` handed to `main`.
    fn make_args_array(&mut self, args: &[&str]) -> Result<Ref, VmError> {
        let string_class = self.resolve_class("java/lang/String")?;
        let array_class = self.array_class_for(string_class)?;
        let array = self.allocate_array(array_class, args.len())?;
        // The array is not reachable from a root until `main` receives it, so it is rooted
        // while it is filled: interning the arguments may collect.
        self.with_protected(array, |vm| {
            for (index, text) in args.iter().enumerate() {
                let string = vm.intern(text)?;
                vm.array_set_ref(array, index, string)?;
            }
            Ok::<(), VmError>(())
        })?;
        Ok(array)
    }

    // -----------------------------------------------------------------------------------------
    // Strings
    // -----------------------------------------------------------------------------------------

    /// Intern a Rust string as a `java.lang.String`, returning the canonical object.
    ///
    /// Interning allocates, and that allocation may run a collection when the heap budget
    /// is exhausted, so a caller that holds a reference only on the Rust stack has to root
    /// it with [`Vm::protect`] or [`Vm::with_protected`] for the duration.
    ///
    /// # Errors
    ///
    /// Raises `java.lang.OutOfMemoryError` when the heap budget cannot hold the string.
    pub fn intern(&mut self, text: &str) -> Result<Ref, VmError> {
        if let Some(&existing) = self.interned.get(text) {
            return Ok(existing);
        }
        let string_class = self
            .resolve_class("java/lang/String")
            .expect("java/lang/String must exist to intern strings");
        let units: Vec<u16> = text.encode_utf16().collect();
        let reference = self.allocate_object(string_class, ObjectData::String(units))?;
        self.interned.insert(text.into(), reference);
        Ok(reference)
    }

    /// Look up a cached boxed primitive (tag is the wrapper's primitive descriptor byte).
    #[must_use]
    pub fn cached_boxed(&self, tag: u8, value: i64) -> Option<Ref> {
        self.boxed_cache.get(&(tag, value)).copied()
    }

    /// Remember a boxed primitive so later boxes of the same value reuse the instance.
    pub fn cache_boxed(&mut self, tag: u8, value: i64, reference: Ref) {
        self.boxed_cache.insert((tag, value), reference);
    }

    /// Create a non-interned `java.lang.String` from UTF-16 code units.
    pub fn make_string_utf16(&mut self, units: Vec<u16>) -> Result<Ref, VmError> {
        let string_class = self.resolve_class("java/lang/String")?;
        self.allocate_object(string_class, ObjectData::String(units))
    }

    /// Create a `java.lang.String` from a Rust string.
    pub fn make_string(&mut self, text: &str) -> Result<Ref, VmError> {
        self.make_string_utf16(text.encode_utf16().collect())
    }

    /// The UTF-16 payload of a string object.
    #[must_use]
    pub fn string_utf16(&self, reference: Ref) -> Option<&[u16]> {
        match &self.heap.get(reference)?.data {
            ObjectData::String(units) => Some(units),
            _ => None,
        }
    }

    /// The Rust text of a string object, replacing unpaired surrogates with `U+FFFD`.
    #[must_use]
    pub fn string_value(&self, reference: Ref) -> Option<String> {
        let units = self.string_utf16(reference)?;
        Some(String::from_utf16_lossy(units))
    }

    /// The Rust text of a `java.lang.String`, or an empty string for null/other objects.
    #[must_use]
    pub fn string_or_empty(&self, reference: Ref) -> String {
        self.string_value(reference).unwrap_or_default()
    }

    // -----------------------------------------------------------------------------------------
    // Class objects and instance checks
    // -----------------------------------------------------------------------------------------

    /// The runtime `java.lang.Class` object for a class, created lazily.
    pub fn class_object(&mut self, class: ClassId) -> Result<Ref, VmError> {
        if let Some(reference) = self.classes.get(class).class_object {
            return Ok(reference);
        }
        let class_class = self.resolve_class("java/lang/Class")?;
        let reference = self.allocate_object(class_class, ObjectData::Class(class))?;
        self.classes.get_mut(class).class_object = Some(reference);
        Ok(reference)
    }

    /// The class of an object.
    #[must_use]
    pub fn class_of(&self, reference: Ref) -> ClassId {
        self.heap
            .get(reference)
            .map_or(ClassId::OBJECT, |object| object.class)
    }

    /// The internal name of a class.
    #[must_use]
    pub fn class_name(&self, class: ClassId) -> &str {
        &self.classes.get(class).name
    }

    /// The lambda definition of a class, or of the class it extends.
    ///
    /// A lambda body is registered for the class the runtime generated for it.  A class that
    /// extends such a lambda inherits its SAM implementation, so the definition is looked up
    /// along the superclass chain as well.
    pub(crate) fn lambda_definition(&self, class: ClassId) -> Option<&LambdaDef> {
        let mut current = Some(class);
        while let Some(id) = current {
            if let Some(definition) = self.lambda_defs.get(&id) {
                return Some(definition);
            }
            current = self.classes.get(id).super_class;
        }
        None
    }

    /// The nest host of a class: its `NestHost` attribute, or the class itself.
    ///
    /// JVMS 5.4.4: a class with no `NestHost` attribute is its own nest host.
    #[must_use]
    pub fn nest_host(&self, class: ClassId) -> ClassId {
        self.classes.get(class).nest_host.unwrap_or(class)
    }

    /// The class of the innermost Java frame on the current thread.
    ///
    /// Native methods do not push a frame, so during a native call this is the
    /// Java caller that invoked it.
    #[must_use]
    pub fn caller_class(&self) -> Option<ClassId> {
        self.current_thread().frames.last().map(|frame| frame.class)
    }

    /// Whether `reference` is an instance of `class` (null is never an instance).
    #[must_use]
    pub fn is_instance(&self, reference: Ref, class: ClassId) -> bool {
        let Some(object) = self.heap.get(reference) else {
            return false;
        };
        self.class_is_subtype(object.class, class)
    }

    /// Whether `sub` is assignable to `sup` (JVMS 5.4.4 / JLS 5.5.1).
    #[must_use]
    pub fn class_is_subtype(&self, sub: ClassId, sup: ClassId) -> bool {
        let sub_class = self.classes.get(sub);
        let sup_class = self.classes.get(sup);
        if sub == sup {
            return true;
        }
        if sup_class.kind == ClassKind::Array {
            // An array is a subtype of Object, Cloneable, Serializable, and any array type whose
            // component it can be assigned to (covariance).
            if sub_class.kind != ClassKind::Array {
                return sup_class.name == "java/lang/Object"
                    || sup_class.name == "java/lang/Cloneable"
                    || sup_class.name == "java/io/Serializable";
            }
            return match (&sub_class.component, &sup_class.component) {
                (Some(component), Some(sup_component)) => match (component, sup_component) {
                    (ArrayComponent::Reference, ArrayComponent::Reference) => {
                        let sub_component = sub_class.component_class.expect("component class");
                        let sup_component = sup_class.component_class.expect("component class");
                        self.class_is_subtype(sub_component, sup_component)
                    }
                    (a, b) => a == b,
                },
                _ => false,
            };
        }
        if sub_class.kind == ClassKind::Primitive || sup_class.kind == ClassKind::Primitive {
            return false;
        }
        if let Some(parent) = sub_class.super_class {
            if self.class_is_subtype(parent, sup) {
                return true;
            }
        }
        for interface in &sub_class.interfaces {
            if self.class_is_subtype(*interface, sup) {
                return true;
            }
        }
        false
    }

    // -----------------------------------------------------------------------------------------
    // Arrays
    // -----------------------------------------------------------------------------------------

    /// Allocate a primitive array of `len` elements.
    pub fn allocate_array_of(
        &mut self,
        component: ArrayComponent,
        len: usize,
    ) -> Result<Ref, VmError> {
        let class = self.array_class_of_component(component, None)?;
        self.allocate_array(class, len)
    }

    /// Allocate an array of a known array class.
    pub fn allocate_array(&mut self, class: ClassId, len: usize) -> Result<Ref, VmError> {
        let component = self
            .classes
            .get(class)
            .component
            .clone()
            .ok_or_else(|| VmError::internal("allocate_array on a non-array class"))?;
        self.maybe_gc();
        // The budget is checked before the elements are allocated: a huge array must raise
        // `OutOfMemoryError` rather than exhaust the host allocator.
        self.ensure_heap_budget(ArrayData::size_of(&component, len))?;
        let Some(data) = ArrayData::zeroed(&component, len) else {
            return Err(self.throw_out_of_memory());
        };
        self.allocate_object(class, ObjectData::Array(data))
    }

    /// Allocate an object array with the given component class.
    pub fn allocate_object_array(
        &mut self,
        component_class: ClassId,
        len: usize,
    ) -> Result<Ref, VmError> {
        let class = self.array_class_for(component_class)?;
        self.allocate_array(class, len)
    }

    /// The length of an array.
    pub fn array_length(&self, reference: Ref) -> Result<usize, VmError> {
        match self.heap.get(reference) {
            Some(object) => match &object.data {
                ObjectData::Array(array) => Ok(array.len()),
                _ => Err(VmError::internal("arraylength on a non-array")),
            },
            None => Err(VmError::internal("arraylength on null")),
        }
    }

    /// The length of an array, with the checks the JVM's `arraylength` performs.
    ///
    /// # Errors
    ///
    /// Throws `NullPointerException` for a null reference and `ClassCastException` for a
    /// reference that does not point at an array, instead of reporting an internal error: a
    /// guest that reaches such a reference has to see the exception the JDK raises.
    pub fn array_length_or_throw(&mut self, reference: Ref) -> Result<usize, VmError> {
        if reference.is_null() {
            return Err(self.throw_new("java/lang/NullPointerException", None));
        }
        match self.heap.get(reference).map(|object| &object.data) {
            Some(ObjectData::Array(array)) => Ok(array.len()),
            _ => Err(self.throw_new("java/lang/ClassCastException", Some("not an array"))),
        }
    }

    /// Read an array element.
    ///
    /// # Errors
    ///
    /// Throws `NullPointerException` or `ArrayIndexOutOfBoundsException` like the JVM.
    pub fn array_get(&mut self, reference: Ref, index: i32) -> Result<Value, VmError> {
        if reference.is_null() {
            return Err(self.throw_new("java/lang/NullPointerException", None));
        }
        let length = self.array_length_or_throw(reference)?;
        if index < 0 || index as usize >= length {
            return Err(self.array_index_error(index, length));
        }
        match &self.heap.get(reference).expect("checked").data {
            ObjectData::Array(array) => Ok(array.get(index as usize)),
            _ => Err(VmError::internal("array load on a non-array")),
        }
    }

    /// Write an array element, checking `ArrayStoreException` for object arrays.
    ///
    /// # Errors
    ///
    /// Throws the exceptions the `*astore` family defines.
    pub fn array_set(
        &mut self,
        reference: Ref,
        index: i32,
        value: Value,
        checked: bool,
    ) -> Result<(), VmError> {
        if reference.is_null() {
            return Err(self.throw_new("java/lang/NullPointerException", None));
        }
        let length = self.array_length_or_throw(reference)?;
        if index < 0 || index as usize >= length {
            return Err(self.array_index_error(index, length));
        }
        if checked {
            let component = self.classes.get(self.class_of(reference)).component_class;
            if let (Some(component), Value::Ref(object)) = (component, value) {
                if !object.is_null() && !self.is_instance(object, component) {
                    let description = self.describe_value(value);
                    return Err(self.throw_new("java/lang/ArrayStoreException", Some(&description)));
                }
            }
        }
        match &mut self.heap.get_mut(reference).expect("checked").data {
            ObjectData::Array(array) => {
                array.set(index as usize, value);
                Ok(())
            }
            _ => Err(VmError::internal("array store on a non-array")),
        }
    }

    /// Write a reference element into an object array.
    ///
    /// # Errors
    ///
    /// Throws the exceptions the `*astore` family defines.
    pub fn array_set_ref(
        &mut self,
        array: ObjectRef,
        index: usize,
        value: ObjectRef,
    ) -> Result<(), VmError> {
        self.array_set(array, index as i32, Value::Ref(value), true)
    }

    fn array_index_error(&mut self, index: i32, length: usize) -> VmError {
        let message = alloc::format!("Index {index} out of bounds for length {length}");
        self.throw_new("java/lang/ArrayIndexOutOfBoundsException", Some(&message))
    }

    // -----------------------------------------------------------------------------------------
    // Exceptions and diagnostics
    // -----------------------------------------------------------------------------------------

    /// Create and throw an exception of `class_name` with an optional message.
    ///
    /// The returned error is always [`VmError::Thrown`] unless constructing the exception itself
    /// threw, in which case that error is returned instead.
    pub fn throw_new(&mut self, class_name: &str, message: Option<&str>) -> VmError {
        let result = (|| -> Result<Ref, VmError> {
            let class = self.resolve_class(class_name)?;
            let message_value = match message {
                Some(text) => Some(self.intern(text)?),
                None => None,
            };
            let (constructor_class, constructor) = if message.is_some() {
                self.find_method(class, "<init>", "(Ljava/lang/String;)V")
            } else {
                self.find_method(class, "<init>", "()V")
            }
            .ok_or_else(|| VmError::internal("exception class lacks a constructor"))?;
            let object = self.new_instance(class)?;
            let args = match message_value {
                Some(message) => vec![Value::Ref(object), Value::Ref(message)],
                None => vec![Value::Ref(object)],
            };
            self.invoke_method(constructor_class, constructor, args)?;
            Ok(object)
        })();
        match result {
            Ok(exception) => VmError::Thrown(exception),
            Err(error) => error,
        }
    }

    /// Create and throw an exception of `class_name` wrapping `cause`, the way the JVM links an
    /// `ExceptionInInitializerError` to the exception its `<clinit>` threw.
    ///
    /// Uses the `(Ljava/lang/Throwable;)V` constructor so `getCause`/`printStackTrace` report the
    /// original throwable.  Falls back to a plain `throw_new` if the class has no such constructor.
    pub fn throw_new_with_cause(&mut self, class_name: &str, cause: Ref) -> VmError {
        let result = (|| -> Result<Ref, VmError> {
            let class = self.resolve_class(class_name)?;
            let Some((constructor_class, constructor)) =
                self.find_method(class, "<init>", "(Ljava/lang/Throwable;)V")
            else {
                return match self.throw_new(class_name, None) {
                    VmError::Thrown(exception) => Ok(exception),
                    other => Err(other),
                };
            };
            let object = self.new_instance(class)?;
            self.invoke_method(
                constructor_class,
                constructor,
                vec![Value::Ref(object), Value::Ref(cause)],
            )?;
            Ok(object)
        })();
        match result {
            Ok(exception) => VmError::Thrown(exception),
            Err(error) => error,
        }
    }

    /// Report an uncaught exception exactly like the `java` launcher: the thread banner followed
    /// by the throwable's stack trace, on `System.err`.
    pub fn print_uncaught(&mut self, thread_name: &str, exception: Ref) {
        let header = alloc::format!("Exception in thread \"{thread_name}\" ");
        let _ = self.host.write(Stream::Stderr, header.as_bytes());
        self.print_throwable(Stream::Stderr, exception);
    }

    /// Print a throwable's `toString` line and stack trace.
    pub fn print_throwable(&mut self, stream: Stream, exception: Ref) {
        let text = self.throwable_text(exception);
        let _ = self.host.write(stream, text.as_bytes());
    }

    /// The full text `printStackTrace` would print: the `toString` line, frames, and causes.
    pub fn throwable_text(&mut self, exception: Ref) -> String {
        let mut text = self.throwable_to_string(exception);
        text.push('\n');
        let mut frames = self.backtraces.get(&exception).cloned().unwrap_or_default();
        // Fall back to the live frames only when no backtrace was recorded;
        // otherwise the frames would be printed twice.
        if frames.is_empty() {
            frames.extend(self.capture_frames());
        }
        for frame in &frames {
            text.push_str(&self.frame_line(frame));
        }
        for cause in self.throwable_causes(exception) {
            text.push_str(&alloc::format!(
                "Caused by: {}\n",
                self.throwable_to_string(cause)
            ));
            for frame in self.backtraces.get(&cause).cloned().unwrap_or_default() {
                text.push_str(&self.frame_line(&frame));
            }
        }
        text
    }

    fn frame_line(&self, frame: &BacktraceFrame) -> String {
        let location = match (&frame.file_name, frame.line_number) {
            (Some(file), Some(line)) => alloc::format!("({file}:{line})"),
            (Some(file), None) => alloc::format!("({file})"),
            (None, _) => "(Unknown Source)".into(),
        };
        alloc::format!(
            "\tat {}.{}{}\n",
            frame.class_name.replace('/', "."),
            frame.method_name,
            location
        )
    }

    /// `Throwable.toString()` without invoking Java code.
    pub fn throwable_to_string(&mut self, exception: Ref) -> String {
        if exception.is_null() {
            return "null".into();
        }
        let class = self.class_of(exception);
        let name = self.class_name(class).replace('/', ".");
        let message = self
            .read_string_field_any(exception, "detailMessage")
            .or_else(|| self.read_string_field_any(exception, "message"));
        match message {
            Some(message) if !message.is_empty() => alloc::format!("{name}: {message}"),
            _ => name,
        }
    }

    fn throwable_causes(&mut self, exception: Ref) -> Vec<Ref> {
        let Some(cause) = self.read_ref_field_any(exception, "cause") else {
            return Vec::new();
        };
        if cause.is_null() || cause == exception {
            return Vec::new();
        }
        let mut out = vec![cause];
        let mut current = cause;
        let mut guard = 0;
        while guard < 16 {
            guard += 1;
            match self.read_ref_field_any(current, "cause") {
                Some(next) if !next.is_null() && next != current => {
                    out.push(next);
                    current = next;
                }
                _ => break,
            }
        }
        out
    }

    /// The captured Java frames of the current thread, innermost first.
    #[must_use]
    pub fn capture_frames(&self) -> Vec<BacktraceFrame> {
        let thread = self.current_thread();
        thread
            .frames
            .iter()
            .rev()
            .map(|frame| {
                let class = self.classes.get(frame.class);
                let method = &class.methods[frame.method as usize];
                BacktraceFrame {
                    class_name: class.name.clone(),
                    method_name: method.name.clone(),
                    file_name: class.source_file.clone(),
                    line_number: Some(frame.line),
                }
            })
            .collect()
    }

    // -----------------------------------------------------------------------------------------
    // Allocation and the heap budget
    // -----------------------------------------------------------------------------------------

    /// Allocate an object, enforcing the heap budget.
    ///
    /// A collection runs before the budget is declared exhausted, so a heap that only holds
    /// garbage is reclaimed first. An allocation that still does not fit raises
    /// `java.lang.OutOfMemoryError`.
    pub(crate) fn allocate_object(
        &mut self,
        class: ClassId,
        data: ObjectData,
    ) -> Result<Ref, VmError> {
        if self.oom_throw {
            // The `OutOfMemoryError` itself is created past the budget, so the failure can
            // always be reported.
            return Ok(self.heap.allocate_unchecked(class, data));
        }
        self.ensure_heap_budget(data.object_size())?;
        match self.heap.allocate(class, data) {
            Ok(reference) => Ok(reference),
            Err(_) => Err(self.throw_out_of_memory()),
        }
    }

    /// Make sure `bytes` fit in the heap budget, collecting first when they do not.
    ///
    /// # Errors
    ///
    /// Raises `java.lang.OutOfMemoryError` when the heap cannot hold the allocation even after
    /// a collection.
    pub(crate) fn ensure_heap_budget(&mut self, bytes: u64) -> Result<(), VmError> {
        if self.oom_throw || self.heap.can_fit(bytes) {
            return Ok(());
        }
        if self.in_native == 0 {
            // Reclaim what the collector can before declaring the heap full. A native call
            // holds raw references on the Rust stack, so no collection may run there.
            self.gc();
        }
        if self.heap.can_fit(bytes) {
            return Ok(());
        }
        Err(self.throw_out_of_memory())
    }

    /// Create and throw `java.lang.OutOfMemoryError`.
    fn throw_out_of_memory(&mut self) -> VmError {
        // The exception object is allocated past the budget, so the failure is reportable even
        // when the heap is completely full.
        self.oom_throw = true;
        let error = self.throw_new("java/lang/OutOfMemoryError", Some("Java heap space"));
        self.oom_throw = false;
        error
    }

    /// Create and throw `java.lang.StackOverflowError`.
    ///
    /// The exception is built without pushing a Java frame — the constructor would need one, and
    /// the stack is exactly what ran out. Reporting the overflow therefore cannot exhaust the
    /// stack again.
    pub(crate) fn throw_stack_overflow(&mut self) -> VmError {
        let result = (|| -> Result<Ref, VmError> {
            let class = self.resolve_class("java/lang/StackOverflowError")?;
            let exception = self.new_instance(class)?;
            self.capture_backtrace(exception);
            Ok(exception)
        })();
        match result {
            Ok(exception) => VmError::Thrown(exception),
            Err(error) => error,
        }
    }

    /// The bytes the live objects occupy.
    #[must_use]
    pub fn heap_used_bytes(&self) -> u64 {
        self.heap.used_bytes()
    }

    /// Replace an object's payload, keeping the heap accounting in step.
    ///
    /// The growth the replacement adds is charged against the heap budget, like any other
    /// allocation: a replacement that does not fit raises `java.lang.OutOfMemoryError`.
    ///
    /// # Errors
    ///
    /// Fails when the reference is null or no longer live, and raises
    /// `java.lang.OutOfMemoryError` when the replacement does not fit in the budget.
    pub fn replace_payload(&mut self, reference: Ref, data: ObjectData) -> Result<(), VmError> {
        let previous = self.heap.object_size(reference).unwrap_or(0);
        let growth = data.object_size().saturating_sub(previous);
        // The object is rooted while the budget is checked, because checking it may
        // collect.
        self.with_protected(reference, |vm| vm.ensure_heap_budget(growth))?;
        self.heap
            .replace_payload(reference, data)
            .map(|_| ())
            .ok_or_else(|| VmError::internal("replace_payload on a dead object"))
    }

    /// The committed heap size: `-Xms`, capped by `-Xmx`, and never below the live set.
    #[must_use]
    pub fn heap_total_bytes(&self) -> u64 {
        self.commit_bytes.max(self.heap.used_bytes())
    }

    /// The maximum heap size, or `None` when the heap is unbounded.
    #[must_use]
    pub fn heap_max_bytes(&self) -> Option<u64> {
        self.options.max_heap_bytes()
    }

    /// The bytes still available before the collector runs.
    #[must_use]
    pub fn heap_free_bytes(&self) -> u64 {
        self.heap_total_bytes()
            .saturating_sub(self.heap.used_bytes())
    }

    // -----------------------------------------------------------------------------------------
    // Garbage collection
    // -----------------------------------------------------------------------------------------

    /// Run a collection now.
    pub fn gc(&mut self) {
        let roots = self.gc_roots();
        self.heap.collect(roots.into_iter());
        self.gc_requested = false;
        self.gc_countdown = 64 * 1024;
        self.grow_commit_bytes();
    }

    /// Grow the heap the collector works within when the live set needs more room.
    ///
    /// The size doubles until it reaches the maximum, so a program whose live set keeps growing
    /// collects less and less often instead of collecting on every allocation.
    fn grow_commit_bytes(&mut self) {
        let used = self.heap.used_bytes();
        if used < self.commit_bytes {
            return;
        }
        let doubled = self
            .commit_bytes
            .saturating_mul(2)
            .max(used.saturating_mul(2));
        let limit = self.options.max_heap_bytes.unwrap_or(u64::MAX);
        self.commit_bytes = doubled.min(limit).max(used);
    }

    fn gc_roots(&self) -> Vec<Ref> {
        let mut roots = Vec::new();
        roots.push(self.out);
        roots.push(self.err);
        roots.extend(self.interned.values().copied());
        roots.extend(self.boxed_cache.values().copied());
        roots.extend(self.temp_roots.iter().copied());
        roots.extend(self.backtraces.keys().copied());
        for thread in &self.threads {
            roots.push(thread.object);
            if let Some(exception) = thread.uncaught {
                roots.push(exception);
            }
            for frame in &thread.frames {
                roots.push(frame.monitor.unwrap_or(Ref::NULL));
                for local in &frame.locals {
                    if let Some(Value::Ref(reference)) = local {
                        roots.push(*reference);
                    }
                }
                for value in &frame.stack {
                    if let Value::Ref(reference) = value {
                        roots.push(*reference);
                    }
                }
            }
        }
        for class in self.classes.all() {
            if let Some(reference) = class.class_object {
                roots.push(reference);
            }
            for value in &class.static_values {
                if let Value::Ref(reference) = value {
                    roots.push(*reference);
                }
            }
        }
        for call_site in self.call_sites.values() {
            if let CallSite::Lambda { class } = call_site {
                let _ = class;
            }
        }
        roots
    }

    /// Request a collection once the heap has grown past its committed size, or once enough
    /// allocations have happened.
    pub(crate) fn maybe_gc(&mut self) {
        if self.in_native > 0 {
            return;
        }
        if self.gc_requested {
            self.gc();
            return;
        }
        // The heap has grown past the size the collector works within: collect.
        if self.heap.used_bytes() >= self.commit_bytes {
            self.gc_requested = true;
            return;
        }
        // The allocation counter keeps the collector running in an unbounded heap, where the
        // committed size is only a default.
        if self.gc_countdown == 0 {
            self.gc_requested = true;
        } else {
            self.gc_countdown -= 1;
        }
    }

    // -----------------------------------------------------------------------------------------
    // Printing helpers
    // -----------------------------------------------------------------------------------------

    /// Write to a standard stream through the host.
    pub fn write_host(&mut self, stream: Stream, bytes: &[u8]) {
        let _ = self.host.write(stream, bytes);
    }

    /// Flush a standard stream through the host.
    pub fn flush_host(&mut self, stream: Stream) {
        let _ = self.host.flush(stream);
    }

    /// A diagnostic description of a value, used in exception messages.
    pub fn describe_value(&mut self, value: Value) -> String {
        match value {
            Value::Ref(reference) if reference.is_null() => "null".into(),
            Value::Ref(reference) => match &self.heap.get(reference).map(|o| &o.data) {
                Some(ObjectData::String(units)) => {
                    let text = String::from_utf16_lossy(units);
                    alloc::format!("\"{text}\"")
                }
                _ => alloc::format!(
                    "{}@{}",
                    self.class_name(self.class_of(reference)),
                    reference.raw()
                ),
            },
            Value::Int(value) => value.to_string(),
            Value::Long(value) => value.to_string(),
            Value::Float(value) => value.to_string(),
            Value::Double(value) => value.to_string(),
        }
    }
}

impl Value {
    /// The number of operand-stack slots, as a free function for native authors.
    #[must_use]
    pub const fn slot_count(value: &Value) -> u16 {
        value.slots()
    }
}
