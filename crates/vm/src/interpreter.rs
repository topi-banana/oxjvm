//! The bytecode interpreter and cooperative scheduler.
//!
//! One `step` executes exactly one JVM instruction (or one pending resume action). The scheduler
//! ([`Vm::drive_until`]) runs steps until a requested frame depth is reached, parking and waking
//! threads cooperatively. Exception dispatch, synchronization, `invokedynamic`, and array
//! operations all live here, one match arm per JVMS 6 instruction.

use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::descriptor::MethodDescriptor;
use oxjvm_classfile::flags::*;
use oxjvm_classfile::{BaseType, CpInfo, FieldType, opcode};

use crate::class::{ArrayComponent, ClassId, Code, Handler};
use crate::error::VmError;
use crate::frame::Frame;
use crate::heap::{MethodHandleValue, ObjectData};
use crate::loader::ResolvedMethod;
use crate::thread::ThreadState;
use crate::value::{ObjectRef, Value, compare_f32, compare_f64};
use crate::{CallSite, ConcatConstant, Resume, Vm, format};

/// The result of one `step`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    /// One instruction executed.
    Executed,
    /// The thread parked itself (monitor, sleep, wait, join).
    Suspended,
    /// The thread's frame stack is empty.
    Terminated,
}

/// Whether an instruction completed or suspended part-way.
pub(crate) enum Outcome {
    /// Continue with the next instruction.
    Continue,
    /// The thread is now parked; the scheduler decides what runs next.
    Suspend,
}

impl<'a> Vm<'a> {
    // -----------------------------------------------------------------------------------------
    // Scheduler
    // -----------------------------------------------------------------------------------------

    /// Run steps until `thread`'s frame stack is back to `base_depth` and it has no pending
    /// resume action.
    ///
    /// Uncaught exceptions in other threads print a stack trace and terminate them; an uncaught
    /// exception in `thread` propagates out once its stack reaches `base_depth`.
    pub(crate) fn drive_until(&mut self, thread: usize, base_depth: usize) -> Result<(), VmError> {
        loop {
            let resumed = self.threads[thread].resume != Resume::None;
            if self.threads[thread].depth() <= base_depth && !resumed {
                return Ok(());
            }
            if self.threads[thread].is_runnable() {
                self.current = thread;
            } else {
                match self.pick_thread() {
                    Some(next) => self.current = next,
                    None => {
                        if !self.advance_virtual_time() {
                            return Err(VmError::internal(
                                "deadlock: every thread is blocked or sleeping",
                            ));
                        }
                        continue;
                    }
                }
            }
            match self.step() {
                Ok(Step::Terminated) => self.terminate_current(),
                Ok(Step::Executed | Step::Suspended) => {}
                Err(VmError::Thrown(exception)) => {
                    if self.current == thread && self.threads[thread].depth() <= base_depth {
                        return Err(VmError::Thrown(exception));
                    }
                    self.handle_uncaught(self.current, exception);
                }
                Err(other) => return Err(other),
            }
        }
    }

    /// Schedule one step of any runnable thread. Used after `main` returns to let remaining
    /// non-daemon threads finish.
    pub(crate) fn schedule_step(&mut self) -> Result<(), VmError> {
        match self.pick_thread() {
            Some(next) => self.current = next,
            None => {
                if !self.advance_virtual_time() {
                    return Ok(()); // nothing left to do
                }
                self.current = self.pick_thread().unwrap_or(0);
            }
        }
        match self.step() {
            Ok(Step::Terminated) => self.terminate_current(),
            Ok(Step::Executed | Step::Suspended) => {}
            Err(VmError::Thrown(exception)) => {
                let thread = self.current;
                self.handle_uncaught(thread, exception);
            }
            Err(other) => return Err(other),
        }
        Ok(())
    }

    /// Whether any non-daemon thread is still alive.
    #[must_use]
    pub(crate) fn has_alive_non_daemon(&self) -> bool {
        self.threads
            .iter()
            .any(|thread| !thread.daemon && thread.state != ThreadState::Terminated)
    }

    /// Print and record an uncaught exception, then terminate the thread.
    pub(crate) fn handle_uncaught(&mut self, thread: usize, exception: ObjectRef) {
        let name = self.threads[thread].name.clone();
        self.threads[thread].uncaught = Some(exception);
        self.current = thread;
        self.print_uncaught(&name, exception);
        self.terminate_current();
    }

    // -----------------------------------------------------------------------------------------
    // Steps
    // -----------------------------------------------------------------------------------------

    /// Execute one instruction or resume action of the current thread.
    pub(crate) fn step(&mut self) -> Result<Step, VmError> {
        if self.current_thread().frames.is_empty() {
            return Ok(Step::Terminated);
        }
        if !self.current_thread().is_runnable() {
            return Ok(Step::Suspended);
        }
        if self.current_thread().resume != Resume::None {
            return self.perform_resume();
        }
        let pc = self.frame().pc;
        let Some(opcode_byte) = self.frame().opcode() else {
            let context = self.frame_context();
            return Err(VmError::invalid_code(
                context,
                "program counter out of range",
            ));
        };
        let code = self.frame().code.clone();
        let Some(decoded) = opcode::decode(&code.bytes, pc) else {
            let context = self.frame_context();
            return Err(VmError::invalid_code(context, "undecodable instruction"));
        };
        let next = decoded.next_pc;
        if self.trace {
            let name = decoded.name.unwrap_or("<invalid>");
            let context = self.frame_context();
            let trace = alloc::format!("  at {context} pc={pc} {name}\n");
            self.host
                .write(oxjvm_platform::Stream::Stderr, trace.as_bytes())
                .ok();
        }
        {
            let frame = self.frame_mut();
            frame.fault_pc = pc;
            frame.pc = next;
            frame.line = code.line_for(pc as u16).unwrap_or(frame.line);
        }
        match self.execute(opcode_byte, pc, next, &code) {
            Ok(Outcome::Continue) => Ok(Step::Executed),
            Ok(Outcome::Suspend) => Ok(Step::Suspended),
            Err(VmError::Thrown(exception)) => {
                self.unwind_exception(exception)?;
                Ok(Step::Executed)
            }
            Err(other) => Err(other),
        }
    }

    /// A `Class.method:line` context string for diagnostics.
    fn frame_context(&self) -> String {
        let frame = self.frame();
        let class = &self.classes.get(frame.class);
        let method = &class.methods[frame.method as usize];
        alloc::format!("{}.{}{}", class.name, method.name, method.descriptor)
    }

    /// Perform the pending resume action of the current thread.
    fn perform_resume(&mut self) -> Result<Step, VmError> {
        let resume = core::mem::replace(&mut self.current_thread_mut().resume, Resume::None);
        match resume {
            Resume::None => Ok(Step::Executed),
            Resume::EnterMonitor { object } => {
                if self.try_enter_monitor(object) {
                    Ok(Step::Executed)
                } else {
                    self.park_current(ThreadState::BlockedMonitor(object));
                    self.current_thread_mut().resume = Resume::EnterMonitor { object };
                    Ok(Step::Suspended)
                }
            }
            Resume::Invoke {
                class,
                method,
                args,
                return_pc,
            } => {
                let monitor = self.method_monitor(class, method, &args)?;
                let acquired = match monitor {
                    Some(object) => self.try_enter_monitor(object),
                    None => true,
                };
                if acquired {
                    self.push_method_frame(class, method, args, return_pc, monitor)?;
                    Ok(Step::Executed)
                } else {
                    let object = monitor.expect("contended monitor");
                    self.park_current(ThreadState::BlockedMonitor(object));
                    self.current_thread_mut().resume = Resume::Invoke {
                        class,
                        method,
                        args,
                        return_pc,
                    };
                    Ok(Step::Suspended)
                }
            }
            Resume::NativeCall {
                class,
                method,
                args,
            } => {
                let monitor = self.method_monitor(class, method, &args)?;
                let acquired = match monitor {
                    Some(object) => self.try_enter_monitor(object),
                    None => true,
                };
                if acquired {
                    let value = self.call_native_fn(class, method, &args)?;
                    if let Some(object) = monitor {
                        self.exit_monitor(object);
                    }
                    self.return_slot = Some(value);
                    Ok(Step::Executed)
                } else {
                    let object = monitor.expect("contended monitor");
                    self.park_current(ThreadState::BlockedMonitor(object));
                    self.current_thread_mut().resume = Resume::NativeCall {
                        class,
                        method,
                        args,
                    };
                    Ok(Step::Suspended)
                }
            }
            Resume::WaitReturn { object, timed_out } => {
                if self.try_enter_monitor(object) {
                    let _ = timed_out;
                    Ok(Step::Executed)
                } else {
                    self.park_current(ThreadState::BlockedMonitor(object));
                    self.current_thread_mut().resume = Resume::WaitReturn { object, timed_out };
                    Ok(Step::Suspended)
                }
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Frames, calls, natives
    // -----------------------------------------------------------------------------------------

    /// Construct a frame for a method call, laying arguments out in local slots.
    pub(crate) fn build_frame(
        &mut self,
        class: ClassId,
        method: u32,
        args: Vec<Value>,
        return_pc: usize,
        monitor: Option<ObjectRef>,
    ) -> Result<Frame, VmError> {
        let definition = &self.classes.get(class).methods[method as usize];
        let Some(code) = definition.code.clone() else {
            return Err(VmError::internal("frame for a method without code"));
        };
        let mut locals: Vec<Option<Value>> = Vec::with_capacity(code.max_locals as usize);
        for value in args {
            locals.push(Some(value));
            if value.slots() == 2 {
                locals.push(None);
            }
        }
        let mut frame = Frame::new(class, method, code, locals, return_pc);
        frame.monitor = monitor;
        Ok(frame)
    }

    /// Push a method frame onto the current thread.
    pub(crate) fn push_method_frame(
        &mut self,
        class: ClassId,
        method: u32,
        args: Vec<Value>,
        return_pc: usize,
        monitor: Option<ObjectRef>,
    ) -> Result<(), VmError> {
        let frame = self.build_frame(class, method, args, return_pc, monitor)?;
        let depth = self.current_thread().depth();
        if depth >= 65_536 {
            return Err(self.throw_new("java/lang/StackOverflowError", None));
        }
        self.current_thread_mut().frames.push(frame);
        Ok(())
    }

    /// The monitor a synchronized method uses: the receiver, or the class object for statics.
    fn method_monitor(
        &mut self,
        class: ClassId,
        method: u32,
        args: &[Value],
    ) -> Result<Option<ObjectRef>, VmError> {
        let definition = &self.classes.get(class).methods[method as usize];
        if definition.access_flags & ACC_SYNCHRONIZED == 0 {
            return Ok(None);
        }
        if definition.is_static() {
            return Ok(Some(self.class_object(class)?));
        }
        let receiver = args.first().copied().unwrap_or(Value::Ref(ObjectRef::NULL));
        if receiver.is_null_ref() {
            return Err(self.throw_new("java/lang/NullPointerException", None));
        }
        Ok(Some(receiver.as_ref()))
    }

    /// Call a native implementation.
    fn call_native_fn(
        &mut self,
        class: ClassId,
        method: u32,
        args: &[Value],
    ) -> Result<Value, VmError> {
        let function = self.classes.get(class).methods[method as usize].native;
        let Some(function) = function else {
            return Err(VmError::internal("call_native_fn on a non-native method"));
        };
        self.in_native += 1;
        let result = function(self, crate::class::NativeContext { class, method }, args);
        self.in_native -= 1;
        result
    }

    /// The return type category of a method, for pushing native results.
    fn method_returns(&self, class: ClassId, method: u32) -> Option<FieldType> {
        let descriptor = &self.classes.get(class).methods[method as usize].descriptor;
        MethodDescriptor::parse(descriptor)
            .ok()
            .and_then(|parsed| parsed.returns)
    }

    /// Invoke a method and wait for its result. This is the API natives and the launcher use; it
    /// handles monitor contention and scheduling internally.
    ///
    /// # Errors
    ///
    /// Propagates thrown exceptions and process exit.
    pub fn invoke_method(
        &mut self,
        class: ClassId,
        method: u32,
        args: Vec<Value>,
    ) -> Result<Value, VmError> {
        let definition = self.classes.get(class).methods[method as usize].clone();
        if definition.is_static() {
            self.ensure_initialized(definition.owner)?;
        }
        if definition.is_abstract() {
            let name = self.classes.get(class).name.clone();
            return Err(self.throw_new(
                "java/lang/AbstractMethodError",
                Some(&alloc::format!(
                    "{name}.{}{}",
                    definition.name,
                    definition.descriptor
                )),
            ));
        }
        let thread = self.current;
        let floor = self.threads[thread].depth();
        let saved_floor = self.threads[thread].floor;
        self.threads[thread].floor = floor;
        let result = if let Some(_native) = definition.native {
            self.invoke_native_blocking(class, method, args)
        } else {
            let monitor = self.method_monitor(class, method, &args)?;
            let acquired = match monitor {
                Some(object) => self.try_enter_monitor(object),
                None => true,
            };
            if acquired {
                self.push_method_frame(class, method, args, 0, monitor)?;
            } else {
                let object = monitor.expect("contended monitor");
                self.park_current(ThreadState::BlockedMonitor(object));
                self.current_thread_mut().resume = Resume::Invoke {
                    class,
                    method,
                    args,
                    return_pc: 0,
                };
            }
            self.return_slot = None;
            let driven = self.drive_until(thread, floor);
            match driven {
                Ok(()) => Ok(self.return_slot.take().unwrap_or(Value::Int(0))),
                Err(error) => Err(error),
            }
        };
        self.threads[thread].floor = saved_floor;
        result
    }

    /// Invoke a native method and wait for its result.
    fn invoke_native_blocking(
        &mut self,
        class: ClassId,
        method: u32,
        args: Vec<Value>,
    ) -> Result<Value, VmError> {
        let monitor = self.method_monitor(class, method, &args)?;
        let acquired = match monitor {
            Some(object) => self.try_enter_monitor(object),
            None => true,
        };
        if !acquired {
            let object = monitor.expect("contended monitor");
            let thread = self.current;
            let floor = self.threads[thread].depth();
            self.park_current(ThreadState::BlockedMonitor(object));
            self.current_thread_mut().resume = Resume::NativeCall {
                class,
                method,
                args,
            };
            self.return_slot = None;
            self.drive_until(thread, floor)?;
            return Ok(self.return_slot.take().unwrap_or(Value::Int(0)));
        }
        let value = self.call_native_fn(class, method, &args)?;
        if let Some(object) = monitor {
            self.exit_monitor(object);
        }
        Ok(value)
    }

    /// Continue a call already resolved by an `invoke*` instruction.
    fn begin_invoke(
        &mut self,
        class: ClassId,
        method: u32,
        args: Vec<Value>,
        return_pc: usize,
    ) -> Result<Outcome, VmError> {
        let definition = self.classes.get(class).methods[method as usize].clone();
        if definition.is_abstract() {
            let name = self.classes.get(class).name.clone();
            return Err(self.throw_new(
                "java/lang/AbstractMethodError",
                Some(&alloc::format!(
                    "{name}.{}{}",
                    definition.name,
                    definition.descriptor
                )),
            ));
        }
        let monitor = self.method_monitor(class, method, &args)?;
        if let Some(native) = definition.native {
            let acquired = match monitor {
                Some(object) => self.try_enter_monitor(object),
                None => true,
            };
            if !acquired {
                let object = monitor.expect("contended monitor");
                self.park_current(ThreadState::BlockedMonitor(object));
                self.current_thread_mut().resume = Resume::NativeCall {
                    class,
                    method,
                    args,
                };
                return Ok(Outcome::Suspend);
            }
            let result = self.call_native_fn(class, method, &args);
            if let Some(object) = monitor {
                self.exit_monitor(object);
            }
            let value = result?;
            if self.method_returns(class, method).is_some() {
                self.push(value);
            }
            let _ = native;
            return Ok(Outcome::Continue);
        }
        let acquired = match monitor {
            Some(object) => self.try_enter_monitor(object),
            None => true,
        };
        if !acquired {
            let object = monitor.expect("contended monitor");
            self.park_current(ThreadState::BlockedMonitor(object));
            self.current_thread_mut().resume = Resume::Invoke {
                class,
                method,
                args,
                return_pc,
            };
            return Ok(Outcome::Suspend);
        }
        self.push_method_frame(class, method, args, return_pc, monitor)?;
        Ok(Outcome::Continue)
    }

    /// Pop arguments for a descriptor, returning them in declaration order.
    fn pop_arguments(&mut self, descriptor: &MethodDescriptor) -> Vec<Value> {
        let mut values = vec![Value::Int(0); descriptor.parameters.len()];
        for (index, parameter) in descriptor.parameters.iter().enumerate().rev() {
            let value = self.pop();
            values[index] = match parameter {
                FieldType::Base(BaseType::Float) if matches!(value, Value::Int(_)) => {
                    Value::Float(value.as_int() as f32)
                }
                _ => value,
            };
        }
        values
    }

    // -----------------------------------------------------------------------------------------
    // Exceptions
    // -----------------------------------------------------------------------------------------

    /// Find a handler for a thrown exception in the current frame; pop frames otherwise.
    ///
    /// Returns `Ok(())` when control has been transferred to a handler, and
    /// [`VmError::Thrown`] when the exception escapes the current thread's floor.
    fn unwind_exception(&mut self, exception: ObjectRef) -> Result<(), VmError> {
        loop {
            let thread_index = self.current;
            let floor = self.threads[thread_index].floor;
            if self.threads[thread_index].depth() <= floor {
                return Err(VmError::Thrown(exception));
            }
            let fault_pc = self.frame().fault_pc;
            let class = self.frame().class;
            let code = self.frame().code.clone();
            let mut handler: Option<Handler> = None;
            for candidate in &code.exception_table {
                if usize::from(candidate.start_pc) <= fault_pc
                    && fault_pc < usize::from(candidate.end_pc)
                {
                    let matches = if candidate.catch_type_index == 0 {
                        true
                    } else {
                        let name = self
                            .classes
                            .get(class)
                            .constant_pool
                            .class_name(candidate.catch_type_index)
                            .map(str::to_string);
                        match name {
                            Ok(name) => {
                                let catch_class = self.resolve_class(&name)?;
                                self.is_instance(exception, catch_class)
                            }
                            Err(_) => false,
                        }
                    };
                    if matches {
                        handler = Some(*candidate);
                        break;
                    }
                }
            }
            if let Some(handler) = handler {
                let frame = self.frame_mut();
                frame.stack.clear();
                frame.stack.push(Value::Ref(exception));
                frame.pc = usize::from(handler.handler_pc);
                return Ok(());
            }
            let monitor = self.frame().monitor;
            if let Some(monitor) = monitor {
                self.exit_monitor(monitor);
            }
            self.current_thread_mut().frames.pop();
        }
    }

    // -----------------------------------------------------------------------------------------
    // Monitors
    // -----------------------------------------------------------------------------------------

    /// Try to enter a monitor; on failure the caller parks and records a resume action.
    pub fn try_enter_monitor(&mut self, object: ObjectRef) -> bool {
        let thread = self.current;
        let Some(heap_object) = self.heap.get_mut(object) else {
            return true; // dead reference: nothing to lock
        };
        if heap_object.monitor.owner.is_none() || heap_object.monitor.owner == Some(thread) {
            heap_object.monitor.owner = Some(thread);
            heap_object.monitor.count += 1;
            true
        } else {
            heap_object.monitor.entrants.push_back(thread);
            false
        }
    }

    /// Release one hold of a monitor, waking one entrant when it becomes free.
    pub fn exit_monitor(&mut self, object: ObjectRef) {
        let mut wake = None;
        if let Some(heap_object) = self.heap.get_mut(object) {
            if heap_object.monitor.count > 0 {
                heap_object.monitor.count -= 1;
            }
            if heap_object.monitor.count == 0 {
                heap_object.monitor.owner = None;
                wake = heap_object.monitor.entrants.pop_front();
            }
        }
        if let Some(thread) = wake {
            if thread != self.current {
                self.threads[thread].state = ThreadState::Runnable;
            }
        }
    }

    /// Wait on a monitor (`Object.wait`), parking until notified or timed out.
    ///
    /// Returns `true` when the wait completed normally.
    pub fn monitor_wait(&mut self, object: ObjectRef, timeout: Option<i64>) -> bool {
        let current = self.current;
        let Some(heap_object) = self.heap.get_mut(object) else {
            return true;
        };
        if heap_object.monitor.owner != Some(current) {
            return false;
        }
        // Release the monitor completely while waiting.
        let saved_count = heap_object.monitor.count;
        heap_object.monitor.count = 0;
        heap_object.monitor.owner = None;
        heap_object.monitor.waiters.push_back(current);
        let deadline = timeout.map(|millis| {
            let now = self.virtual_time;
            now.saturating_add(millis.max(0))
        });
        self.park_current(ThreadState::Waiting {
            monitor: object,
            deadline,
        });
        self.current_thread_mut().resume = Resume::WaitReturn {
            object,
            timed_out: false,
        };
        let _ = saved_count;
        // Re-acquiring happens through `Resume::WaitReturn`; the caller records the hold count.
        true
    }

    /// Wake one or all threads waiting on a monitor.
    pub fn monitor_notify(&mut self, object: ObjectRef, all: bool) {
        let mut wake: Vec<usize> = Vec::new();
        if let Some(heap_object) = self.heap.get_mut(object) {
            let count = if all {
                heap_object.monitor.waiters.len()
            } else {
                usize::from(!heap_object.monitor.waiters.is_empty())
            };
            for _ in 0..count {
                if let Some(thread) = heap_object.monitor.waiters.pop_front() {
                    heap_object.monitor.entrants.push_back(thread);
                    wake.push(thread);
                }
            }
        }
        for thread in wake {
            if thread != self.current {
                self.threads[thread].state = ThreadState::Runnable;
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Constants and dynamic call sites
    // -----------------------------------------------------------------------------------------

    fn load_constant(&mut self, class: ClassId, index: u16) -> Result<Value, VmError> {
        let info = self.classes.get(class).constant_pool.get(index).cloned();
        let info = info.map_err(|error| {
            VmError::invalid_code(self.classes.get(class).name.clone(), error.to_string())
        })?;
        Ok(match info {
            CpInfo::Integer(value) => Value::Int(value),
            CpInfo::Float(bits) => Value::Float(f32::from_bits(bits)),
            CpInfo::Long(value) => Value::Long(value),
            CpInfo::Double(bits) => Value::Double(f64::from_bits(bits)),
            CpInfo::String(string_index) => {
                let text = self
                    .classes
                    .get(class)
                    .constant_pool
                    .utf8(string_index)
                    .map(str::to_string)
                    .map_err(|error| {
                        VmError::invalid_code(
                            self.classes.get(class).name.clone(),
                            error.to_string(),
                        )
                    })?;
                Value::Ref(self.intern(&text))
            }
            CpInfo::Class(class_index) => {
                let name = self
                    .classes
                    .get(class)
                    .constant_pool
                    .class_name(class_index)
                    .map(str::to_string)
                    .map_err(|error| {
                        VmError::invalid_code(
                            self.classes.get(class).name.clone(),
                            error.to_string(),
                        )
                    })?;
                let resolved = self.resolve_class(&name)?;
                Value::Ref(self.class_object(resolved)?)
            }
            CpInfo::MethodType(descriptor_index) => {
                let descriptor = self
                    .classes
                    .get(class)
                    .constant_pool
                    .utf8(descriptor_index)
                    .map(str::to_string)
                    .map_err(|error| {
                        VmError::invalid_code(
                            self.classes.get(class).name.clone(),
                            error.to_string(),
                        )
                    })?;
                Value::Ref(self.make_method_type(&descriptor)?)
            }
            CpInfo::MethodHandle { .. } => {
                let handle = self.method_handle_value(class, index)?;
                Value::Ref(self.make_method_handle(handle)?)
            }
            CpInfo::Dynamic { .. } => {
                return Err(self.throw_new(
                    "java/lang/BootstrapMethodError",
                    Some("dynamic constants are not supported"),
                ));
            }
            other => {
                return Err(VmError::invalid_code(
                    self.classes.get(class).name.clone(),
                    alloc::format!("ldc names {}", other.kind_name()),
                ));
            }
        })
    }

    /// Create a `java.lang.invoke.MethodType` object.
    pub fn make_method_type(&mut self, descriptor: &str) -> Result<ObjectRef, VmError> {
        let class = self.resolve_class("java/lang/invoke/MethodType")?;
        self.maybe_gc();
        Ok(self
            .heap
            .allocate(class, ObjectData::MethodType(descriptor.into())))
    }

    /// Create a `java.lang.invoke.MethodHandle` object.
    pub fn make_method_handle(&mut self, value: MethodHandleValue) -> Result<ObjectRef, VmError> {
        let class = self.resolve_class("java/lang/invoke/MethodHandle")?;
        self.maybe_gc();
        Ok(self.heap.allocate(class, ObjectData::MethodHandle(value)))
    }

    /// Format a value for string concatenation, following `String.valueOf` for its descriptor.
    pub fn display_value(&mut self, ty: &FieldType, value: Value) -> Result<String, VmError> {
        let text = match ty {
            FieldType::Base(base) => match base {
                BaseType::Boolean => {
                    if value.as_int() != 0 {
                        "true".into()
                    } else {
                        "false".into()
                    }
                }
                BaseType::Char => {
                    let unit = value.as_int() as u32 & 0xFFFF;
                    char::from_u32(unit).map_or_else(|| "\u{FFFD}".into(), |ch| ch.to_string())
                }
                BaseType::Byte | BaseType::Short | BaseType::Int => {
                    format::int_to_string(value.as_int())
                }
                BaseType::Long => format::long_to_string(value.as_long()),
                BaseType::Float => format::float_to_string(value.as_float()),
                BaseType::Double => format::double_to_string(value.as_double()),
                BaseType::Void => String::new(),
            },
            FieldType::Object(_) | FieldType::Array(_) => {
                let reference = value.as_ref();
                if reference.is_null() {
                    "null".into()
                } else if let Some(text) = self.string_value(reference) {
                    text
                } else {
                    let (class, method) =
                        self.resolve_virtual_method(reference, "toString", "()Ljava/lang/String;")?;
                    let result = self.invoke_method(class, method, vec![Value::Ref(reference)])?;
                    self.string_value(result.as_ref()).unwrap_or_default()
                }
            }
        };
        Ok(text)
    }

    fn execute_indy(
        &mut self,
        class: ClassId,
        cp_index: u16,
        next: usize,
    ) -> Result<Outcome, VmError> {
        let (descriptor, _name) = {
            let (name, descriptor) = self
                .classes
                .get(class)
                .constant_pool
                .get(cp_index)
                .and_then(|info| match info {
                    CpInfo::InvokeDynamic { name_and_type, .. } => Ok(*name_and_type),
                    _ => Err(oxjvm_classfile::ParseError::Other("invokedynamic")),
                })
                .and_then(|name_and_type| {
                    self.classes
                        .get(class)
                        .constant_pool
                        .name_and_type(name_and_type)
                })
                .map(|(name, descriptor)| (name.to_string(), descriptor.to_string()))
                .map_err(|error| {
                    VmError::invalid_code(self.classes.get(class).name.clone(), error.to_string())
                })?;
            (descriptor, name)
        };
        let parsed = MethodDescriptor::parse(&descriptor).map_err(|message| {
            VmError::invalid_code(self.classes.get(class).name.clone(), message)
        })?;
        let site = self.resolve_call_site(class, cp_index)?;
        match site {
            CallSite::Concat { recipe, constants } => {
                let values = self.pop_arguments(&parsed);
                let text = self.concat(&recipe, &constants, &parsed, &values)?;
                let string = self.intern(&text);
                self.push(Value::Ref(string));
            }
            CallSite::Lambda {
                class: lambda_class,
            } => {
                let args = self.pop_arguments(&parsed);
                let object = self.new_instance(lambda_class)?;
                let constructor_descriptor = {
                    let mut text = String::from("(");
                    for parameter in &parsed.parameters {
                        text.push_str(&parameter.descriptor());
                    }
                    text.push_str(")V");
                    text
                };
                let (constructor_class, constructor) = self
                    .find_method(lambda_class, "<init>", &constructor_descriptor)
                    .ok_or_else(|| VmError::internal("lambda class lacks its constructor"))?;
                let mut full = vec![Value::Ref(object)];
                full.extend(args);
                self.begin_invoke(constructor_class, constructor, full, next)?;
                self.push(Value::Ref(object));
            }
            CallSite::Direct {
                class: target_class,
                method,
                is_static,
            } => {
                let args = self.pop_arguments(&parsed);
                if is_static {
                    self.ensure_initialized(target_class)?;
                    self.begin_invoke(target_class, method, args, next)?;
                } else {
                    let receiver = args[0].as_ref();
                    let (declaring, index) = self.resolve_virtual_method(
                        receiver,
                        &self.classes.get(target_class).methods[method as usize]
                            .name
                            .clone(),
                        &self.classes.get(target_class).methods[method as usize]
                            .descriptor
                            .clone(),
                    )?;
                    self.begin_invoke(declaring, index, args, next)?;
                }
            }
        }
        Ok(Outcome::Continue)
    }

    fn concat(
        &mut self,
        recipe: &str,
        constants: &[ConcatConstant],
        descriptor: &MethodDescriptor,
        values: &[Value],
    ) -> Result<String, VmError> {
        let mut out = String::new();
        let mut argument = 0usize;
        let mut constant = 0usize;
        for ch in recipe.chars() {
            match ch {
                '\u{1}' => {
                    if let Some(value) = values.get(argument) {
                        let ty = descriptor
                            .parameters
                            .get(argument)
                            .cloned()
                            .unwrap_or(FieldType::Object("java/lang/Object".into()));
                        out.push_str(&self.display_value(&ty, *value)?);
                    }
                    argument += 1;
                }
                '\u{2}' => {
                    if let Some(value) = constants.get(constant) {
                        match value {
                            ConcatConstant::Str(text) => out.push_str(text),
                            ConcatConstant::Int(value) => {
                                out.push_str(&format::int_to_string(*value))
                            }
                            ConcatConstant::Long(value) => {
                                out.push_str(&format::long_to_string(*value))
                            }
                            ConcatConstant::Float(value) => {
                                out.push_str(&format::float_to_string(*value));
                            }
                            ConcatConstant::Double(value) => {
                                out.push_str(&format::double_to_string(*value));
                            }
                            ConcatConstant::Char(value) => {
                                if let Some(ch) = char::from_u32(u32::from(*value)) {
                                    out.push(ch);
                                }
                            }
                        }
                    }
                    constant += 1;
                }
                other => out.push(other),
            }
        }
        Ok(out)
    }

    // -----------------------------------------------------------------------------------------
    // Arrays
    // -----------------------------------------------------------------------------------------

    fn allocate_multi(&mut self, class: ClassId, dimensions: &[i32]) -> Result<ObjectRef, VmError> {
        let length = dimensions[0];
        if length < 0 {
            return Err(self.throw_new("java/lang/NegativeArraySizeException", None));
        }
        let array = self.allocate_array(class, length as usize)?;
        if dimensions.len() > 1 {
            let component_class = self.classes.get(class).component_class;
            let component = component_class
                .ok_or_else(|| VmError::internal("multianewarray without a component class"))?;
            for index in 0..length {
                let child = self.allocate_multi(component, &dimensions[1..])?;
                self.array_set(array, index, Value::Ref(child), false)?;
            }
        }
        Ok(array)
    }

    // -----------------------------------------------------------------------------------------
    // The instruction dispatch
    // -----------------------------------------------------------------------------------------

    #[allow(clippy::too_many_lines)]
    fn execute(
        &mut self,
        op: u8,
        pc: usize,
        next: usize,
        code: &Arc<Code>,
    ) -> Result<Outcome, VmError> {
        let bytes = &code.bytes;
        let class = self.frame().class;
        match op {
            0x00 => {}
            0x01 => self.push(Value::Ref(ObjectRef::NULL)),
            0x02 => self.push(Value::Int(-1)),
            0x03..=0x08 => self.push(Value::Int(i32::from(op - 0x03))),
            0x09 | 0x0a => self.push(Value::Long(i64::from(op - 0x09))),
            0x0b..=0x0d => self.push(Value::Float(f32::from(op - 0x0b))),
            0x0e | 0x0f => self.push(Value::Double(f64::from(op - 0x0e))),
            0x10 => self.push(Value::Int(i32::from(bytes[pc + 1] as i8))),
            0x11 => {
                let value = i16::from_be_bytes([bytes[pc + 1], bytes[pc + 2]]);
                self.push(Value::Int(i32::from(value)));
            }
            0x12 => {
                let index = u16::from(bytes[pc + 1]);
                let value = self.load_constant(class, index)?;
                self.push(value);
            }
            0x13 => {
                let index = u16::from_be_bytes([bytes[pc + 1], bytes[pc + 2]]);
                let value = self.load_constant(class, index)?;
                self.push(value);
            }
            0x14 => {
                let index = u16::from_be_bytes([bytes[pc + 1], bytes[pc + 2]]);
                let value = self.load_constant(class, index)?;
                self.push(value);
            }
            0x15 | 0x16 | 0x17 | 0x18 | 0x19 => {
                let index = usize::from(bytes[pc + 1]);
                let value = self.local(index);
                self.push(value);
            }
            0x1a..=0x1d => {
                let value = self.local(usize::from(op - 0x1a));
                self.push(value);
            }
            0x1e..=0x21 => {
                let value = self.local(usize::from(op - 0x1e));
                self.push(value);
            }
            0x22..=0x25 => {
                let value = self.local(usize::from(op - 0x22));
                self.push(value);
            }
            0x26..=0x29 => {
                let value = self.local(usize::from(op - 0x26));
                self.push(value);
            }
            0x2a..=0x2d => {
                let value = self.local(usize::from(op - 0x2a));
                self.push(value);
            }
            0x2e..=0x35 => {
                let index = self.pop_int();
                let reference = self.pop_ref();
                let value = self.array_get(reference, index)?;
                self.push(value);
            }
            0x36..=0x3a => {
                let index = usize::from(bytes[pc + 1]);
                let value = self.pop();
                self.set_local(index, value);
            }
            0x3b..=0x3e => {
                let value = self.pop();
                self.set_local(usize::from(op - 0x3b), value);
            }
            0x3f..=0x42 => {
                let value = self.pop();
                self.set_local(usize::from(op - 0x3f), value);
            }
            0x43..=0x46 => {
                let value = self.pop();
                self.set_local(usize::from(op - 0x43), value);
            }
            0x47..=0x4a => {
                let value = self.pop();
                self.set_local(usize::from(op - 0x47), value);
            }
            0x4b..=0x4e => {
                let value = self.pop();
                self.set_local(usize::from(op - 0x4b), value);
            }
            0x4f..=0x56 => {
                let value = self.pop();
                let index = self.pop_int();
                let reference = self.pop_ref();
                self.array_set(reference, index, value, op == 0x53)?;
            }
            0x57 => {
                self.pop();
            }
            0x58 => {
                let value = self.pop();
                if !value.is_category2() {
                    self.pop();
                }
            }
            0x59 => {
                let value = self.peek(0);
                self.push(value);
            }
            0x5a => {
                let value1 = self.pop();
                let value2 = self.pop();
                self.push(value1);
                self.push(value2);
                self.push(value1);
            }
            0x5b => {
                let value1 = self.pop();
                let value2 = self.pop();
                if value2.is_category2() {
                    self.push(value1);
                    self.push(value2);
                    self.push(value1);
                } else {
                    let value3 = self.pop();
                    self.push(value1);
                    self.push(value3);
                    self.push(value2);
                    self.push(value1);
                }
            }
            0x5c => {
                let value1 = self.pop();
                if value1.is_category2() {
                    self.push(value1);
                    self.push(value1);
                } else {
                    let value2 = self.pop();
                    self.push(value2);
                    self.push(value1);
                    self.push(value2);
                    self.push(value1);
                }
            }
            0x5d => {
                let value1 = self.pop();
                if value1.is_category2() {
                    let value2 = self.pop();
                    self.push(value1);
                    self.push(value2);
                    self.push(value1);
                } else {
                    let value2 = self.pop();
                    let value3 = self.pop();
                    self.push(value2);
                    self.push(value1);
                    self.push(value3);
                    self.push(value2);
                    self.push(value1);
                }
            }
            0x5e => {
                let value1 = self.pop();
                if value1.is_category2() {
                    let value2 = self.pop();
                    if value2.is_category2() {
                        self.push(value1);
                        self.push(value2);
                        self.push(value1);
                    } else {
                        let value3 = self.pop();
                        self.push(value1);
                        self.push(value3);
                        self.push(value2);
                        self.push(value1);
                    }
                } else {
                    let value2 = self.pop();
                    let value3 = self.pop();
                    if value3.is_category2() {
                        self.push(value2);
                        self.push(value1);
                        self.push(value3);
                        self.push(value2);
                        self.push(value1);
                    } else {
                        let value4 = self.pop();
                        self.push(value2);
                        self.push(value1);
                        self.push(value4);
                        self.push(value3);
                        self.push(value2);
                        self.push(value1);
                    }
                }
            }
            0x5f => {
                let value1 = self.pop();
                let value2 = self.pop();
                self.push(value1);
                self.push(value2);
            }
            0x60 => {
                let b = self.pop_int();
                let a = self.pop_int();
                self.push(Value::Int(a.wrapping_add(b)));
            }
            0x61 => {
                let b = self.pop().as_long();
                let a = self.pop().as_long();
                self.push(Value::Long(a.wrapping_add(b)));
            }
            0x62 => {
                let b = self.pop().as_float();
                let a = self.pop().as_float();
                self.push(Value::Float(a + b));
            }
            0x63 => {
                let b = self.pop().as_double();
                let a = self.pop().as_double();
                self.push(Value::Double(a + b));
            }
            0x64 => {
                let b = self.pop_int();
                let a = self.pop_int();
                self.push(Value::Int(a.wrapping_sub(b)));
            }
            0x65 => {
                let b = self.pop().as_long();
                let a = self.pop().as_long();
                self.push(Value::Long(a.wrapping_sub(b)));
            }
            0x66 => {
                let b = self.pop().as_float();
                let a = self.pop().as_float();
                self.push(Value::Float(a - b));
            }
            0x67 => {
                let b = self.pop().as_double();
                let a = self.pop().as_double();
                self.push(Value::Double(a - b));
            }
            0x68 => {
                let b = self.pop_int();
                let a = self.pop_int();
                self.push(Value::Int(a.wrapping_mul(b)));
            }
            0x69 => {
                let b = self.pop().as_long();
                let a = self.pop().as_long();
                self.push(Value::Long(a.wrapping_mul(b)));
            }
            0x6a => {
                let b = self.pop().as_float();
                let a = self.pop().as_float();
                self.push(Value::Float(a * b));
            }
            0x6b => {
                let b = self.pop().as_double();
                let a = self.pop().as_double();
                self.push(Value::Double(a * b));
            }
            0x6c => {
                let b = self.pop_int();
                let a = self.pop_int();
                if b == 0 {
                    return Err(self.throw_new("java/lang/ArithmeticException", Some("/ by zero")));
                }
                self.push(Value::Int(a.wrapping_div(b)));
            }
            0x6d => {
                let b = self.pop().as_long();
                let a = self.pop().as_long();
                if b == 0 {
                    return Err(self.throw_new("java/lang/ArithmeticException", Some("/ by zero")));
                }
                self.push(Value::Long(a.wrapping_div(b)));
            }
            0x6e => {
                let b = self.pop().as_float();
                let a = self.pop().as_float();
                self.push(Value::Float(a / b));
            }
            0x6f => {
                let b = self.pop().as_double();
                let a = self.pop().as_double();
                self.push(Value::Double(a / b));
            }
            0x70 => {
                let b = self.pop_int();
                let a = self.pop_int();
                if b == 0 {
                    return Err(self.throw_new("java/lang/ArithmeticException", Some("/ by zero")));
                }
                self.push(Value::Int(a.wrapping_rem(b)));
            }
            0x71 => {
                let b = self.pop().as_long();
                let a = self.pop().as_long();
                if b == 0 {
                    return Err(self.throw_new("java/lang/ArithmeticException", Some("/ by zero")));
                }
                self.push(Value::Long(a.wrapping_rem(b)));
            }
            0x72 => {
                let b = self.pop().as_float();
                let a = self.pop().as_float();
                self.push(Value::Float(a % b));
            }
            0x73 => {
                let b = self.pop().as_double();
                let a = self.pop().as_double();
                self.push(Value::Double(a % b));
            }
            0x74 => {
                let value = self.pop_int();
                self.push(Value::Int(value.wrapping_neg()));
            }
            0x75 => {
                let value = self.pop().as_long();
                self.push(Value::Long(value.wrapping_neg()));
            }
            0x76 => {
                let value = self.pop().as_float();
                self.push(Value::Float(-value));
            }
            0x77 => {
                let value = self.pop().as_double();
                self.push(Value::Double(-value));
            }
            0x78 => {
                let b = self.pop_int();
                let a = self.pop_int();
                self.push(Value::Int(a.wrapping_shl(b as u32)));
            }
            0x79 => {
                let b = self.pop_int();
                let a = self.pop().as_long();
                self.push(Value::Long(a.wrapping_shl(b as u32)));
            }
            0x7a => {
                let b = self.pop_int();
                let a = self.pop_int();
                self.push(Value::Int(a.wrapping_shr(b as u32)));
            }
            0x7b => {
                let b = self.pop_int();
                let a = self.pop().as_long();
                self.push(Value::Long(a.wrapping_shr(b as u32)));
            }
            0x7c => {
                let b = self.pop_int();
                let a = self.pop_int();
                self.push(Value::Int((a as u32).wrapping_shr(b as u32) as i32));
            }
            0x7d => {
                let b = self.pop_int();
                let a = self.pop().as_long();
                self.push(Value::Long((a as u64).wrapping_shr(b as u32) as i64));
            }
            0x7e => {
                let b = self.pop_int();
                let a = self.pop_int();
                self.push(Value::Int(a & b));
            }
            0x7f => {
                let b = self.pop().as_long();
                let a = self.pop().as_long();
                self.push(Value::Long(a & b));
            }
            0x80 => {
                let b = self.pop_int();
                let a = self.pop_int();
                self.push(Value::Int(a | b));
            }
            0x81 => {
                let b = self.pop().as_long();
                let a = self.pop().as_long();
                self.push(Value::Long(a | b));
            }
            0x82 => {
                let b = self.pop_int();
                let a = self.pop_int();
                self.push(Value::Int(a ^ b));
            }
            0x83 => {
                let b = self.pop().as_long();
                let a = self.pop().as_long();
                self.push(Value::Long(a ^ b));
            }
            0x84 => {
                let index = usize::from(bytes[pc + 1]);
                let constant = i32::from(bytes[pc + 2] as i8);
                let value = self.local(index).as_int().wrapping_add(constant);
                self.set_local(index, Value::Int(value));
            }
            0x85 => {
                let value = self.pop_int();
                self.push(Value::Long(i64::from(value)));
            }
            0x86 => {
                let value = self.pop_int();
                self.push(Value::Float(value as f32));
            }
            0x87 => {
                let value = self.pop_int();
                self.push(Value::Double(f64::from(value)));
            }
            0x88 => {
                let value = self.pop().as_long();
                self.push(Value::Int(value as i32));
            }
            0x89 => {
                let value = self.pop().as_long();
                self.push(Value::Float(value as f32));
            }
            0x8a => {
                let value = self.pop().as_long();
                self.push(Value::Double(value as f64));
            }
            0x8b => {
                let value = self.pop().as_float();
                self.push(Value::Int(value as i32));
            }
            0x8c => {
                let value = self.pop().as_float();
                self.push(Value::Long(value as i64));
            }
            0x8d => {
                let value = self.pop().as_float();
                self.push(Value::Double(f64::from(value)));
            }
            0x8e => {
                let value = self.pop().as_double();
                self.push(Value::Int(value as i32));
            }
            0x8f => {
                let value = self.pop().as_double();
                self.push(Value::Long(value as i64));
            }
            0x90 => {
                let value = self.pop().as_double();
                self.push(Value::Float(value as f32));
            }
            0x91 => {
                let value = self.pop_int();
                self.push(Value::Int(i32::from(value as i8)));
            }
            0x92 => {
                let value = self.pop_int();
                self.push(Value::Int(i32::from(value as u16)));
            }
            0x93 => {
                let value = self.pop_int();
                self.push(Value::Int(i32::from(value as i16)));
            }
            0x94 => {
                let b = self.pop().as_long();
                let a = self.pop().as_long();
                self.push(Value::Int(match a.cmp(&b) {
                    core::cmp::Ordering::Less => -1,
                    core::cmp::Ordering::Equal => 0,
                    core::cmp::Ordering::Greater => 1,
                }));
            }
            0x95 | 0x96 => {
                let b = self.pop().as_float();
                let a = self.pop().as_float();
                let nan = if op == 0x95 { -1 } else { 1 };
                self.push(Value::Int(compare_f32(a, b, nan)));
            }
            0x97 | 0x98 => {
                let b = self.pop().as_double();
                let a = self.pop().as_double();
                let nan = if op == 0x97 { -1 } else { 1 };
                self.push(Value::Int(compare_f64(a, b, nan)));
            }
            0x99..=0x9e => {
                let value = self.pop_int();
                let offset = i16::from_be_bytes([bytes[pc + 1], bytes[pc + 2]]);
                let take = match op {
                    0x99 => value == 0,
                    0x9a => value != 0,
                    0x9b => value < 0,
                    0x9c => value >= 0,
                    0x9d => value > 0,
                    _ => value <= 0,
                };
                if take {
                    self.frame_mut().pc = branch_target(pc, i64::from(offset))?;
                }
            }
            0x9f..=0xa4 => {
                let b = self.pop_int();
                let a = self.pop_int();
                let offset = i16::from_be_bytes([bytes[pc + 1], bytes[pc + 2]]);
                let take = match op {
                    0x9f => a == b,
                    0xa0 => a != b,
                    0xa1 => a < b,
                    0xa2 => a >= b,
                    0xa3 => a > b,
                    _ => a <= b,
                };
                if take {
                    self.frame_mut().pc = branch_target(pc, i64::from(offset))?;
                }
            }
            0xa5 | 0xa6 => {
                let b = self.pop_ref();
                let a = self.pop_ref();
                let offset = i16::from_be_bytes([bytes[pc + 1], bytes[pc + 2]]);
                let equal = a == b;
                let take = if op == 0xa5 { equal } else { !equal };
                if take {
                    self.frame_mut().pc = branch_target(pc, i64::from(offset))?;
                }
            }
            0xa7 => {
                let offset = i16::from_be_bytes([bytes[pc + 1], bytes[pc + 2]]);
                self.frame_mut().pc = branch_target(pc, i64::from(offset))?;
            }
            0xa8 => {
                let offset = i16::from_be_bytes([bytes[pc + 1], bytes[pc + 2]]);
                self.push(Value::Int(next as i32));
                self.frame_mut().pc = branch_target(pc, i64::from(offset))?;
            }
            0xa9 => {
                let index = usize::from(bytes[pc + 1]);
                let address = self.local(index).as_int();
                self.frame_mut().pc = address as usize;
            }
            0xaa => {
                let value = self.pop_int();
                let padding = (4 - ((pc + 1) % 4)) % 4;
                let base = pc + 1 + padding;
                let default = read_i32(bytes, base);
                let low = read_i32(bytes, base + 4);
                let high = read_i32(bytes, base + 8);
                let mut target = default;
                if value >= low && value <= high {
                    let at = base + 12 + (i64::from(value) - i64::from(low)) as usize * 4;
                    target = read_i32(bytes, at);
                }
                self.frame_mut().pc = branch_target(pc, i64::from(target))?;
            }
            0xab => {
                let key = self.pop_int();
                let padding = (4 - ((pc + 1) % 4)) % 4;
                let base = pc + 1 + padding;
                let default = read_i32(bytes, base);
                let pairs = read_i32(bytes, base + 4);
                let mut target = default;
                for index in 0..pairs {
                    let at = base + 8 + index as usize * 8;
                    let candidate = read_i32(bytes, at);
                    if candidate == key {
                        target = read_i32(bytes, at + 4);
                        break;
                    }
                }
                self.frame_mut().pc = branch_target(pc, i64::from(target))?;
            }
            0xac..=0xb0 => {
                let value = self.pop();
                self.return_from_frame(Some(value));
                return Ok(Outcome::Continue);
            }
            0xb1 => {
                self.return_from_frame(None);
                return Ok(Outcome::Continue);
            }
            0xb2 => {
                let index = u16::from_be_bytes([bytes[pc + 1], bytes[pc + 2]]);
                let (declaring, field, is_static) = self.resolved_field(class, index)?;
                if !is_static {
                    return Err(self.throw_new("java/lang/IncompatibleClassChangeError", None));
                }
                self.ensure_initialized(declaring)?;
                let slot = self.classes.get(declaring).fields[field as usize].slot as usize;
                let value = self.classes.get(declaring).static_values[slot];
                self.push(value);
            }
            0xb3 => {
                let index = u16::from_be_bytes([bytes[pc + 1], bytes[pc + 2]]);
                let (declaring, field, is_static) = self.resolved_field(class, index)?;
                if !is_static {
                    return Err(self.throw_new("java/lang/IncompatibleClassChangeError", None));
                }
                let value = self.pop();
                self.check_final_field_write(class, declaring, field, true)?;
                self.ensure_initialized(declaring)?;
                let slot = self.classes.get(declaring).fields[field as usize].slot as usize;
                self.classes.get_mut(declaring).static_values[slot] = value;
            }
            0xb4 => {
                let index = u16::from_be_bytes([bytes[pc + 1], bytes[pc + 2]]);
                let (declaring, field, is_static) = self.resolved_field(class, index)?;
                if is_static {
                    return Err(self.throw_new("java/lang/IncompatibleClassChangeError", None));
                }
                let receiver = self.pop_ref();
                if receiver.is_null() {
                    return Err(self.throw_new("java/lang/NullPointerException", None));
                }
                let slot = self.classes.get(declaring).fields[field as usize].slot as usize;
                let value = match self.heap.get(receiver).map(|object| &object.data) {
                    Some(ObjectData::Instance(fields)) => fields[slot],
                    _ => Value::Int(0),
                };
                self.push(value);
            }
            0xb5 => {
                let index = u16::from_be_bytes([bytes[pc + 1], bytes[pc + 2]]);
                let (declaring, field, is_static) = self.resolved_field(class, index)?;
                if is_static {
                    return Err(self.throw_new("java/lang/IncompatibleClassChangeError", None));
                }
                let value = self.pop();
                let receiver = self.pop_ref();
                if receiver.is_null() {
                    return Err(self.throw_new("java/lang/NullPointerException", None));
                }
                self.check_final_field_write(class, declaring, field, false)?;
                let slot = self.classes.get(declaring).fields[field as usize].slot as usize;
                if let Some(ObjectData::Instance(fields)) =
                    self.heap.get_mut(receiver).map(|object| &mut object.data)
                {
                    fields[slot] = value;
                }
            }
            0xb6 | 0xb7 | 0xb8 | 0xb9 => {
                let index = u16::from_be_bytes([bytes[pc + 1], bytes[pc + 2]]);
                if op == 0xb6 {
                    if let Some(outcome) = self.try_polymorphic_handle_invoke(class, index)? {
                        let _ = next;
                        return Ok(outcome);
                    }
                }
                let resolved = self.resolved_method(class, index)?;
                let name = self.classes.get(resolved.class).methods[resolved.method as usize]
                    .name
                    .clone();
                let descriptor_text = self.classes.get(resolved.class).methods
                    [resolved.method as usize]
                    .descriptor
                    .clone();
                let descriptor = MethodDescriptor::parse(&descriptor_text).map_err(|message| {
                    VmError::invalid_code(self.classes.get(class).name.clone(), message)
                })?;
                match op {
                    0xb8 => {
                        let args = self.pop_arguments(&descriptor);
                        self.ensure_initialized(resolved.class)?;
                        return self.begin_invoke(resolved.class, resolved.method, args, next);
                    }
                    0xb7 => {
                        let arguments = self.pop_arguments(&descriptor);
                        let receiver = self.pop_ref();
                        let mut args = vec![Value::Ref(receiver)];
                        args.extend(arguments);
                        return self.begin_invoke(resolved.class, resolved.method, args, next);
                    }
                    _ => {
                        let arguments = self.pop_arguments(&descriptor);
                        let receiver = self.pop_ref();
                        let mut args = vec![Value::Ref(receiver)];
                        args.extend(arguments);
                        let (declaring, method) =
                            self.resolve_virtual_method(receiver, &name, &descriptor_text)?;
                        return self.begin_invoke(declaring, method, args, next);
                    }
                }
            }
            0xba => {
                let index = u16::from_be_bytes([bytes[pc + 1], bytes[pc + 2]]);
                return self.execute_indy(class, index, next);
            }
            0xbb => {
                let index = u16::from_be_bytes([bytes[pc + 1], bytes[pc + 2]]);
                let name = self
                    .classes
                    .get(class)
                    .constant_pool
                    .class_name(index)
                    .map(str::to_string)
                    .map_err(|error| {
                        VmError::invalid_code(
                            self.classes.get(class).name.clone(),
                            error.to_string(),
                        )
                    })?;
                let resolved = self.resolve_class(&name)?;
                let (is_interface, is_abstract) = {
                    let target = self.classes.get(resolved);
                    (target.is_interface(), target.is_abstract())
                };
                if is_interface || is_abstract {
                    return Err(self.throw_new(
                        "java/lang/InstantiationError",
                        Some(&name.replace('/', ".")),
                    ));
                }
                self.ensure_initialized(resolved)?;
                let object = self.new_instance(resolved)?;
                self.push(Value::Ref(object));
            }
            0xbc => {
                let atype = bytes[pc + 1];
                let component = ArrayComponent::from_atype(atype).ok_or_else(|| {
                    VmError::invalid_code(
                        self.classes.get(class).name.clone(),
                        "bad newarray atype",
                    )
                })?;
                let length = self.pop_int();
                if length < 0 {
                    return Err(self.throw_new("java/lang/NegativeArraySizeException", None));
                }
                let array = self.allocate_array_of(component, length as usize)?;
                self.push(Value::Ref(array));
            }
            0xbd => {
                let index = u16::from_be_bytes([bytes[pc + 1], bytes[pc + 2]]);
                let name = self
                    .classes
                    .get(class)
                    .constant_pool
                    .class_name(index)
                    .map(str::to_string)
                    .map_err(|error| {
                        VmError::invalid_code(
                            self.classes.get(class).name.clone(),
                            error.to_string(),
                        )
                    })?;
                let length = self.pop_int();
                if length < 0 {
                    return Err(self.throw_new("java/lang/NegativeArraySizeException", None));
                }
                let component = self.resolve_class(&name)?;
                let array = self.allocate_object_array(component, length as usize)?;
                self.push(Value::Ref(array));
            }
            0xbe => {
                let reference = self.pop_ref();
                if reference.is_null() {
                    return Err(self.throw_new("java/lang/NullPointerException", None));
                }
                let length = self.array_length(reference)?;
                self.push(Value::Int(length as i32));
            }
            0xbf => {
                let reference = self.pop_ref();
                if reference.is_null() {
                    return Err(self.throw_new("java/lang/NullPointerException", None));
                }
                return Err(VmError::Thrown(reference));
            }
            0xc0 => {
                let index = u16::from_be_bytes([bytes[pc + 1], bytes[pc + 2]]);
                let name = self
                    .classes
                    .get(class)
                    .constant_pool
                    .class_name(index)
                    .map(str::to_string)
                    .map_err(|error| {
                        VmError::invalid_code(
                            self.classes.get(class).name.clone(),
                            error.to_string(),
                        )
                    })?;
                let target = self.resolve_class(&name)?;
                let reference = self.pop_ref();
                if reference.is_null() || self.is_instance(reference, target) {
                    self.push(Value::Ref(reference));
                } else {
                    let from = self.class_name(self.class_of(reference)).replace('/', ".");
                    let to = self.class_name(target).replace('/', ".");
                    return Err(self.throw_new(
                        "java/lang/ClassCastException",
                        Some(&alloc::format!("class {from} cannot be cast to class {to}")),
                    ));
                }
            }
            0xc1 => {
                let index = u16::from_be_bytes([bytes[pc + 1], bytes[pc + 2]]);
                let name = self
                    .classes
                    .get(class)
                    .constant_pool
                    .class_name(index)
                    .map(str::to_string)
                    .map_err(|error| {
                        VmError::invalid_code(
                            self.classes.get(class).name.clone(),
                            error.to_string(),
                        )
                    })?;
                let target = self.resolve_class(&name)?;
                let reference = self.pop_ref();
                let result = !reference.is_null() && self.is_instance(reference, target);
                self.push(Value::Int(i32::from(result)));
            }
            0xc2 => {
                let reference = self.pop_ref();
                if reference.is_null() {
                    return Err(self.throw_new("java/lang/NullPointerException", None));
                }
                if !self.try_enter_monitor(reference) {
                    self.park_current(ThreadState::BlockedMonitor(reference));
                    self.current_thread_mut().resume = Resume::EnterMonitor { object: reference };
                    return Ok(Outcome::Suspend);
                }
            }
            0xc3 => {
                let reference = self.pop_ref();
                if reference.is_null() {
                    return Err(self.throw_new("java/lang/NullPointerException", None));
                }
                let owner = self
                    .heap
                    .get(reference)
                    .map(|object| object.monitor.owner)
                    .flatten();
                if owner != Some(self.current) {
                    return Err(self.throw_new(
                        "java/lang/IllegalMonitorStateException",
                        Some("current thread is not owner"),
                    ));
                }
                self.exit_monitor(reference);
            }
            0xc4 => {
                let inner = bytes[pc + 1];
                match inner {
                    0x15 | 0x16 | 0x17 | 0x18 | 0x19 => {
                        let index = usize::from(u16::from_be_bytes([bytes[pc + 2], bytes[pc + 3]]));
                        let value = self.local(index);
                        self.push(value);
                    }
                    0x36..=0x3a => {
                        let index = usize::from(u16::from_be_bytes([bytes[pc + 2], bytes[pc + 3]]));
                        let value = self.pop();
                        self.set_local(index, value);
                    }
                    0x84 => {
                        let index = usize::from(u16::from_be_bytes([bytes[pc + 2], bytes[pc + 3]]));
                        let constant = i16::from_be_bytes([bytes[pc + 4], bytes[pc + 5]]);
                        let value = self.local(index).as_int().wrapping_add(i32::from(constant));
                        self.set_local(index, Value::Int(value));
                    }
                    0xa9 => {
                        let index = usize::from(u16::from_be_bytes([bytes[pc + 2], bytes[pc + 3]]));
                        let address = self.local(index).as_int();
                        self.frame_mut().pc = address as usize;
                    }
                    other => {
                        return Err(VmError::invalid_code(
                            self.classes.get(class).name.clone(),
                            alloc::format!("bad wide opcode {other:#04x}"),
                        ));
                    }
                }
            }
            0xc5 => {
                let index = u16::from_be_bytes([bytes[pc + 1], bytes[pc + 2]]);
                let dimensions = bytes[pc + 3];
                let name = self
                    .classes
                    .get(class)
                    .constant_pool
                    .class_name(index)
                    .map(str::to_string)
                    .map_err(|error| {
                        VmError::invalid_code(
                            self.classes.get(class).name.clone(),
                            error.to_string(),
                        )
                    })?;
                let resolved = self.resolve_class(&name)?;
                let mut lengths = vec![0i32; usize::from(dimensions)];
                for slot in lengths.iter_mut().rev() {
                    *slot = self.pop_int();
                }
                let array = self.allocate_multi(resolved, &lengths)?;
                self.push(Value::Ref(array));
            }
            0xc6 | 0xc7 => {
                let reference = self.pop_ref();
                let offset = i16::from_be_bytes([bytes[pc + 1], bytes[pc + 2]]);
                let is_null = reference.is_null();
                let take = if op == 0xc6 { is_null } else { !is_null };
                if take {
                    self.frame_mut().pc = branch_target(pc, i64::from(offset))?;
                }
            }
            0xc8 => {
                let offset = read_i32(bytes, pc + 1);
                self.frame_mut().pc = branch_target(pc, i64::from(offset))?;
            }
            0xc9 => {
                let offset = read_i32(bytes, pc + 1);
                self.push(Value::Int(next as i32));
                self.frame_mut().pc = branch_target(pc, i64::from(offset))?;
            }
            other => {
                return Err(VmError::invalid_code(
                    self.classes.get(class).name.clone(),
                    alloc::format!("invalid opcode {other:#04x}"),
                ));
            }
        }
        Ok(Outcome::Continue)
    }

    /// Return from the current frame, transferring control to its caller.
    fn return_from_frame(&mut self, value: Option<Value>) {
        let frame = self
            .current_thread_mut()
            .frames
            .pop()
            .expect("return without a frame");
        if let Some(monitor) = frame.monitor {
            self.exit_monitor(monitor);
        }
        self.return_slot = value.or(Some(Value::Int(0)));
        if let Some(caller) = self.current_thread_mut().frames.last_mut() {
            caller.pc = frame.return_pc;
            if let Some(value) = value {
                caller.stack.push(value);
            }
        }
    }

    /// Reject writes to final fields from outside `<clinit>` (JVMS 6.5 putfield/putstatic).
    fn check_final_field_write(
        &mut self,
        current_class: ClassId,
        declaring: ClassId,
        field: u32,
        _is_static: bool,
    ) -> Result<(), VmError> {
        let definition = &self.classes.get(declaring).fields[field as usize];
        if definition.access_flags & ACC_FINAL == 0 {
            return Ok(());
        }
        if current_class == declaring {
            let frame = self.frame();
            let method = &self.classes.get(frame.class).methods[frame.method as usize];
            if method.name == "<clinit>" {
                return Ok(());
            }
        }
        // `putfield` of a final field is also legal inside the declaring class's constructors.
        if current_class == declaring && !_is_static {
            let frame = self.frame();
            let method = &self.classes.get(frame.class).methods[frame.method as usize];
            if method.name == "<init>" {
                return Ok(());
            }
        }
        Err(VmError::internal("final field write rejected"))
    }

    /// Resolve and cache a field reference.
    fn resolved_field(
        &mut self,
        class: ClassId,
        index: u16,
    ) -> Result<(ClassId, u32, bool), VmError> {
        if let Some(&cached) = self.resolved_fields.get(&(class.raw(), index)) {
            return Ok(cached);
        }
        let resolved = self.resolve_field_ref(class, index)?;
        self.resolved_fields.insert((class.raw(), index), resolved);
        Ok(resolved)
    }

    /// Resolve and cache a method reference.
    fn resolved_method(&mut self, class: ClassId, index: u16) -> Result<ResolvedMethod, VmError> {
        if let Some(&cached) = self.resolved_methods.get(&(class.raw(), index)) {
            return Ok(cached);
        }
        let resolved = self.resolve_method_ref(class, index)?;
        self.resolved_methods.insert((class.raw(), index), resolved);
        Ok(resolved)
    }

    /// Handle the signature-polymorphic `MethodHandle.invoke`/`invokeExact`/`invokeBasic` family.
    ///
    /// These methods have no fixed descriptor: the call site's descriptor describes the real
    /// argument and return types, exactly as JVMS 5.4.3.5 specifies.
    fn try_polymorphic_handle_invoke(
        &mut self,
        class: ClassId,
        cp_index: u16,
    ) -> Result<Option<Outcome>, VmError> {
        let (_name, descriptor, owner_name) = {
            let target = self.classes.get(class);
            let pool = &target.constant_pool;
            let Ok(CpInfo::Methodref {
                class: class_index,
                name_and_type,
            }) = pool.get(cp_index)
            else {
                return Ok(None);
            };
            let (name, descriptor) = pool
                .name_and_type(*name_and_type)
                .map_err(|error| VmError::invalid_code(target.name.clone(), error.to_string()))?;
            if name != "invoke" && name != "invokeExact" && name != "invokeBasic" {
                return Ok(None);
            }
            let owner = pool
                .class_name(*class_index)
                .map_err(|error| VmError::invalid_code(target.name.clone(), error.to_string()))?;
            (name.to_string(), descriptor.to_string(), owner.to_string())
        };
        let handle_class = match self.resolve_class("java/lang/invoke/MethodHandle") {
            Ok(class) => class,
            Err(_) => return Ok(None),
        };
        let owner = self.resolve_class(&owner_name)?;
        if !self.class_is_subtype(owner, handle_class) {
            return Ok(None);
        }
        let parsed = MethodDescriptor::parse(&descriptor).map_err(|message| {
            VmError::invalid_code(self.classes.get(class).name.clone(), message)
        })?;
        let args = self.pop_arguments(&parsed);
        let receiver = self.pop_ref();
        if receiver.is_null() {
            return Err(self.throw_new("java/lang/NullPointerException", None));
        }
        let Some(handle) = self.method_handle_of(receiver) else {
            return Err(self.throw_new(
                "java/lang/ClassCastException",
                Some("receiver is not a MethodHandle"),
            ));
        };
        let value = self.invoke_handle(&handle, &args)?;
        if parsed.returns.is_some() {
            self.push(value);
        }
        Ok(Some(Outcome::Continue))
    }
}

fn branch_target(pc: usize, offset: i64) -> Result<usize, VmError> {
    let target = pc as i64 + offset;
    if target < 0 {
        return Err(VmError::internal("negative branch target"));
    }
    Ok(target as usize)
}

fn read_i32(bytes: &[u8], at: usize) -> i32 {
    i32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}
