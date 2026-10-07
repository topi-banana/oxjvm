#![no_std]
#![deny(unsafe_code)]
//! WebAssembly entry points for the oxjvm runtime.
//!
//! The runtime itself is already `no_std` and thread-free: Java threads are cooperative and run on
//! the calling thread, so a wasm engine that supports shared memory and threads can host several
//! VMs (or several guests) without the runtime requiring `std::thread`. This crate only provides
//! the plumbing a wasm host needs:
//!
//! * [`run`] executes a main class from in-memory class bytes, with output delivered to a callback.
//! * [`MemoryHost`] is the host implementation, replaceable with any [`oxjvm_platform::Host`].
//!
//! Build the portable core for `wasm32-unknown-unknown`:
//!
//! ```sh
//! cargo build -p oxjvm-classfile -p oxjvm-platform -p oxjvm-vm -p oxjvm-java \
//!     --target wasm32-unknown-unknown
//! ```
//!
//! A wasm binary that links this crate needs a global allocator and a panic handler (the usual
//! `#[global_allocator]`/`#[panic_handler]` pair); both are host policy, so they are left out here.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use oxjvm_platform::{Host, HostError, Stream};
use oxjvm_vm::Vm;

/// A host that serves classes from memory and forwards output to a callback.
pub struct MemoryHost<'a> {
    classes: BTreeMap<String, Vec<u8>>,
    sink: &'a mut dyn FnMut(Stream, &[u8]),
    properties: BTreeMap<String, String>,
}

impl<'a> MemoryHost<'a> {
    /// Create a host with no classes and the given output callback.
    pub fn new(sink: &'a mut dyn FnMut(Stream, &[u8])) -> Self {
        Self {
            classes: BTreeMap::new(),
            sink,
            properties: BTreeMap::new(),
        }
    }

    /// Insert a class by internal name.
    pub fn insert(&mut self, internal_name: &str, bytes: Vec<u8>) {
        self.classes.insert(String::from(internal_name), bytes);
    }

    /// Insert a class, taking its name from the class file itself.
    ///
    /// # Errors
    ///
    /// Returns a parse error when the header cannot be read.
    pub fn insert_parsed(&mut self, bytes: Vec<u8>) -> Result<(), oxjvm_classfile::ParseError> {
        let class = oxjvm_classfile::ClassFile::read(&bytes)?;
        let name = class.this_name()?.to_string();
        self.insert(&name, bytes);
        Ok(())
    }

    /// Add a system property.
    pub fn set_property(&mut self, key: &str, value: &str) {
        self.properties
            .insert(String::from(key), String::from(value));
    }
}

impl Host for MemoryHost<'_> {
    fn load_class(&mut self, internal_name: &str) -> Option<Vec<u8>> {
        self.classes.get(internal_name).cloned()
    }

    fn write(&mut self, stream: Stream, bytes: &[u8]) -> Result<(), HostError> {
        (self.sink)(stream, bytes);
        Ok(())
    }

    fn property(&mut self, key: &str) -> Option<String> {
        self.properties.get(key).cloned()
    }
}

/// Run a main class with in-memory class bytes, returning the process status.
///
/// Output is delivered to `sink`; this function never touches a file, a clock, or an OS thread, so
/// it works on any wasm engine.
pub fn run(
    main_class: &str,
    args: &[&str],
    classes: &[&[u8]],
    sink: &mut dyn FnMut(Stream, &[u8]),
) -> Result<i32, oxjvm_vm::VmError> {
    let mut host = MemoryHost::new(sink);
    for bytes in classes {
        let _ = host.insert_parsed(bytes.to_vec());
    }
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    vm.run_main(main_class, args)
}
