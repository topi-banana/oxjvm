//! The API natives use: threads, backtraces, randomness, and polymorphic method-handle calls.

use alloc::string::String;
use alloc::vec::Vec;

use crate::Vm;
use crate::class::ClassId;
use crate::error::VmError;
use crate::heap::{BacktraceFrame, MethodHandleValue, ObjectData};
use crate::thread::ThreadState;
use crate::value::{ObjectRef, Value};

impl<'a> Vm<'a> {
    /// The `java.lang.Thread` object of the currently running thread.
    #[must_use]
    pub fn current_thread_object(&self) -> ObjectRef {
        self.current_thread().object
    }

    /// The name of a thread object.
    #[must_use]
    pub fn thread_name_of(&self, object: ObjectRef) -> Option<String> {
        let class = self.class_of(object);
        self.read_string_field(object, class, "name")
    }

    /// Whether a thread object has been started and has not yet terminated.
    #[must_use]
    pub fn thread_is_alive(&self, object: ObjectRef) -> bool {
        match self.thread_objects.get(&object) {
            Some(&index) => self.threads[index].state != ThreadState::Terminated,
            None => false,
        }
    }

    /// The index of a started thread, if any.
    #[must_use]
    pub fn thread_record(&self, object: ObjectRef) -> Option<usize> {
        self.thread_objects.get(&object).copied()
    }

    /// Sleep the current thread until the virtual clock passes `millis` from now.
    pub fn sleep_current(&mut self, millis: i64) {
        let deadline = self.virtual_time.saturating_add(millis.max(0));
        self.park_current(ThreadState::Sleeping(deadline));
    }

    /// Yield the current thread's slice.
    pub fn yield_current(&mut self) {
        self.park_current(ThreadState::Yielded);
    }

    /// Wait for another thread to terminate, with an optional timeout in milliseconds.
    pub fn join_thread(&mut self, target: ObjectRef, timeout_millis: Option<i64>) {
        let Some(index) = self.thread_objects.get(&target).copied() else {
            return;
        };
        if self.threads[index].state == ThreadState::Terminated {
            return;
        }
        let deadline = timeout_millis.map(|millis| self.virtual_time.saturating_add(millis.max(0)));
        self.park_current(ThreadState::Joining {
            target: index,
            deadline,
        });
    }

    /// Set a thread's interrupt flag and wake it if it is parked.
    pub fn interrupt_thread(&mut self, object: ObjectRef) {
        let Some(index) = self.thread_objects.get(&object).copied() else {
            return;
        };
        self.threads[index].interrupted = true;
        let state = self.threads[index].state.clone();
        match state {
            ThreadState::Sleeping(_)
            | ThreadState::Waiting { .. }
            | ThreadState::Joining { .. }
            | ThreadState::BlockedMonitor(_) => {
                self.threads[index].state = ThreadState::Runnable;
            }
            _ => {}
        }
    }

    /// A thread's interrupt flag.
    #[must_use]
    pub fn thread_interrupted_flag(&self, object: ObjectRef) -> bool {
        self.thread_objects
            .get(&object)
            .is_some_and(|&index| self.threads[index].interrupted)
    }

    /// Clear a thread's interrupt flag.
    pub fn clear_thread_interrupted(&mut self, object: ObjectRef) {
        if let Some(&index) = self.thread_objects.get(&object) {
            self.threads[index].interrupted = false;
        }
    }

    /// Rename a thread.
    ///
    /// # Errors
    ///
    /// Propagates field lookup failures.
    pub fn set_thread_name(&mut self, object: ObjectRef, name: &str) -> Result<(), VmError> {
        let class = self.class_of(object);
        self.set_string_field(object, class, "name", name)?;
        if let Some(&index) = self.thread_objects.get(&object) {
            self.threads[index].name = name.into();
        }
        Ok(())
    }

    /// A thread's priority.
    #[must_use]
    pub fn thread_priority(&self, object: ObjectRef) -> Option<i32> {
        self.thread_record(object)
            .map(|index| self.threads[index].priority)
    }

    /// Set a thread's priority.
    pub fn set_thread_priority(&mut self, object: ObjectRef, priority: i32) {
        if let Some(&index) = self.thread_objects.get(&object) {
            self.threads[index].priority = priority;
        }
        if let Ok((declaring, field)) = self.find_field(self.class_of(object), "priority", "I") {
            self.set_instance_int(object, declaring, field, priority);
        }
    }

    /// Whether a thread object is a daemon.
    #[must_use]
    pub fn thread_is_daemon(&self, object: ObjectRef) -> bool {
        self.thread_objects
            .get(&object)
            .is_some_and(|&index| self.threads[index].daemon)
    }

    /// Set a thread object's daemon flag.
    ///
    /// # Errors
    ///
    /// Propagates field lookup failures.
    pub fn set_thread_daemon(&mut self, object: ObjectRef, daemon: bool) -> Result<(), VmError> {
        let class = self.class_of(object);
        self.set_boolean_field(object, class, "daemon", daemon)?;
        if let Some(&index) = self.thread_objects.get(&object) {
            self.threads[index].daemon = daemon;
        }
        Ok(())
    }

    /// A thread's identity id (its index + 1, stable and positive).
    #[must_use]
    pub fn thread_id(&self, object: ObjectRef) -> i64 {
        self.thread_objects
            .get(&object)
            .map_or(0, |&index| index as i64 + 1)
    }

    /// The number of live threads.
    #[must_use]
    pub fn live_thread_count(&self) -> usize {
        self.threads
            .iter()
            .filter(|thread| thread.state != ThreadState::Terminated)
            .count()
    }

    /// Whether `owner` currently holds the monitor of `object`.
    #[must_use]
    pub fn holds_monitor(&self, object: ObjectRef, owner: ObjectRef) -> bool {
        let Some(&owner_index) = self.thread_objects.get(&owner) else {
            return false;
        };
        self.heap
            .get(object)
            .is_some_and(|heap_object| heap_object.monitor.owner == Some(owner_index))
    }

    /// Build a `VmError::Exit` for `System.exit`.
    #[must_use]
    pub const fn exit_vm(status: i32) -> VmError {
        VmError::Exit(status)
    }

    /// Wall-clock milliseconds from the host.
    pub fn host_time_millis(&mut self) -> i64 {
        self.host.current_time_millis()
    }

    /// Monotonic nanoseconds from the host.
    pub fn host_nano_time(&mut self) -> i64 {
        self.host.nano_time()
    }

    /// A system property from the host.
    pub fn host_property(&mut self, key: &str) -> Option<String> {
        self.host.property(key)
    }

    /// The next pseudo-random `u64` (xorshift64*).
    pub fn random_u64(&mut self) -> u64 {
        let mut state = self.random_state;
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        self.random_state = state;
        state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// The next pseudo-random `i64`.
    pub fn random_i64(&mut self) -> i64 {
        self.random_u64() as i64
    }

    /// A pseudo-random `f64` in `[0, 1)`, like `Math.random`.
    pub fn random_f64(&mut self) -> f64 {
        (self.random_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Seed the generator.
    pub fn seed_random(&mut self, seed: i64) {
        self.random_state = (seed as u64) ^ 0x9E37_79B9_7F4A_7C15;
    }

    /// Capture the current thread's Java frames into a throwable.
    pub fn capture_backtrace(&mut self, throwable: ObjectRef) {
        if self.backtraces.contains_key(&throwable) {
            return;
        }
        let frames = self.capture_frames();
        self.backtraces.insert(throwable, frames);
    }

    /// The captured frames of a throwable.
    #[must_use]
    pub fn backtrace(&self, throwable: ObjectRef) -> Vec<BacktraceFrame> {
        self.backtraces.get(&throwable).cloned().unwrap_or_default()
    }

    /// Replace a throwable's captured frames.
    pub fn set_backtrace(&mut self, throwable: ObjectRef, frames: Vec<BacktraceFrame>) {
        self.backtraces.insert(throwable, frames);
    }

    /// The raw method handle stored in a `java.lang.invoke.MethodHandle` object.
    #[must_use]
    pub fn method_handle_of(&self, object: ObjectRef) -> Option<MethodHandleValue> {
        match &self.heap.get(object)?.data {
            ObjectData::MethodHandle(handle) => Some(handle.clone()),
            _ => None,
        }
    }

    /// The method-type descriptor stored in a `java.lang.invoke.MethodType` object.
    #[must_use]
    pub fn method_type_of(&self, object: ObjectRef) -> Option<String> {
        match &self.heap.get(object)?.data {
            ObjectData::MethodType(descriptor) => Some(descriptor.clone()),
            _ => None,
        }
    }

    /// Register an extra GC root for the duration of a native call.
    pub fn protect(&mut self, object: ObjectRef) {
        self.temp_roots.push(object);
    }

    /// Remove a previously protected root.
    pub fn unprotect(&mut self, object: ObjectRef) {
        if let Some(index) = self.temp_roots.iter().rposition(|root| *root == object) {
            self.temp_roots.remove(index);
        }
    }

    /// The class of `System.out`.
    #[must_use]
    pub fn stdout_object(&self) -> ObjectRef {
        self.out
    }

    /// The class of `System.err`.
    #[must_use]
    pub fn stderr_object(&self) -> ObjectRef {
        self.err
    }

    /// Set `System.out`.
    pub fn set_stdout_object(&mut self, object: ObjectRef) {
        self.out = object;
    }

    /// Set `System.err`.
    pub fn set_stderr_object(&mut self, object: ObjectRef) {
        self.err = object;
    }

    /// The primitive class for a base type (for `Integer.TYPE`).
    ///
    /// # Errors
    ///
    /// Fails only if the primitive class was never registered, which cannot happen after boot.
    pub fn primitive_class(&self, base: oxjvm_classfile::BaseType) -> Result<ClassId, VmError> {
        self.primitive_classes
            .get(&base)
            .copied()
            .ok_or_else(|| VmError::internal("missing primitive class"))
    }

    /// Look up a method handle target on a class by name and descriptor.
    ///
    /// # Errors
    ///
    /// Throws `NoSuchMethodException`.
    pub fn find_method_handle_target(
        &mut self,
        class: ClassId,
        name: &str,
        descriptor: &str,
        static_only: bool,
        is_virtual: bool,
        constructor: bool,
    ) -> Result<MethodHandleValue, VmError> {
        let found = if constructor {
            self.find_method(class, "<init>", descriptor)
        } else {
            self.find_method(class, name, descriptor)
        };
        let Some((declaring, method)) = found else {
            return Err(self.throw_new(
                "java/lang/NoSuchMethodException",
                Some(&alloc::format!("{name}{descriptor}")),
            ));
        };
        let definition = &self.classes.get(declaring).methods[method as usize];
        Ok(if constructor {
            MethodHandleValue::New { class: declaring }
        } else if definition.is_static() || static_only {
            MethodHandleValue::Static {
                class: declaring,
                method,
            }
        } else if is_virtual {
            MethodHandleValue::Virtual {
                class: declaring,
                method,
            }
        } else {
            MethodHandleValue::Special {
                class: declaring,
                method,
            }
        })
    }

    /// Enable or disable the per-instruction trace (written to standard error through the host).
    pub fn set_trace(&mut self, enabled: bool) {
        self.trace = enabled;
    }

    /// Resolve a class, returning `None` instead of throwing when it is unavailable.
    pub fn resolve_class_lenient_pub(&mut self, name: &str) -> Option<ClassId> {
        self.resolve_class(name).ok()
    }

    /// Shallow-copy an object (`Object.clone`): fields and array elements are copied, the class is
    /// shared, and the identity hash and monitor are fresh.
    ///
    /// # Errors
    ///
    /// Throws `NullPointerException` for null.
    pub fn clone_object(&mut self, reference: ObjectRef) -> Result<ObjectRef, VmError> {
        let Some(object) = self.heap.get(reference) else {
            return Err(self.throw_new("java/lang/NullPointerException", None));
        };
        let class = object.class;
        let data = object.data.clone();
        self.maybe_gc();
        Ok(self.heap.allocate(class, data))
    }

    /// `System.arraycopy`: bounds- and type-checked array copying, including overlap.
    ///
    /// # Errors
    ///
    /// Throws `NullPointerException`, `ArrayStoreException`, or
    /// `ArrayIndexOutOfBoundsException` exactly as the JDK does.
    pub fn array_copy(
        &mut self,
        source: ObjectRef,
        source_pos: i32,
        destination: ObjectRef,
        destination_pos: i32,
        length: i32,
    ) -> Result<(), VmError> {
        if source.is_null() || destination.is_null() {
            return Err(self.throw_new("java/lang/NullPointerException", None));
        }
        if length < 0 || source_pos < 0 || destination_pos < 0 {
            return Err(self.throw_new("java/lang/ArrayIndexOutOfBoundsException", None));
        }
        let (source_length, destination_length) = {
            let source_array = self.heap.get(source).and_then(|object| object.data_array());
            let destination_array = self
                .heap
                .get(destination)
                .and_then(|object| object.data_array());
            match (source_array, destination_array) {
                (Some(source_array), Some(destination_array)) => {
                    (source_array.len(), destination_array.len())
                }
                _ => return Err(self.throw_new("java/lang/ArrayStoreException", None)),
            }
        };
        if (source_pos as usize).saturating_add(length as usize) > source_length
            || (destination_pos as usize).saturating_add(length as usize) > destination_length
        {
            return Err(self.throw_new("java/lang/ArrayIndexOutOfBoundsException", None));
        }
        let values: Vec<Value> = {
            let source_array = self
                .heap
                .get(source)
                .and_then(|object| object.data_array())
                .expect("checked");
            (0..length as usize)
                .map(|offset| source_array.get(source_pos as usize + offset))
                .collect()
        };
        for (offset, value) in values.into_iter().enumerate() {
            self.array_set(destination, destination_pos + offset as i32, value, true)?;
        }
        Ok(())
    }
}
