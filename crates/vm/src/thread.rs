//! Java threads and the cooperative scheduler.
//!
//! oxjvm runs Java threads cooperatively inside one OS thread: a scheduler round-robins runnable
//! threads, and `monitorenter`, `Object.wait`, `Thread.sleep`, and `Thread.join` move a thread
//! between states instead of blocking the host. This is deterministic, needs no OS threads, and
//! works unchanged on `wasm32` as long as the guest uses the standard Java synchronization APIs.

use alloc::string::String;
use alloc::vec::Vec;

use crate::Vm;
use crate::error::VmError;
use crate::frame::Frame;
use crate::value::{ObjectRef, Value};

/// A thread's scheduling state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreadState {
    /// Ready to run.
    Runnable,
    /// Yielded cooperatively and waiting for its next slice.
    Yielded,
    /// Sleeping until the given virtual time (milliseconds).
    Sleeping(i64),
    /// Waiting to enter a monitor held by another thread.
    BlockedMonitor(ObjectRef),
    /// Waiting in `Object.wait`, optionally with a timeout deadline.
    Waiting {
        /// The monitor waited on.
        monitor: ObjectRef,
        /// Absolute virtual deadline, if a timeout was given.
        deadline: Option<i64>,
    },
    /// Waiting for another thread to terminate.
    Joining {
        /// Index of the joined thread.
        target: usize,
        /// Absolute virtual deadline, if a timeout was given.
        deadline: Option<i64>,
    },
    /// Finished.
    Terminated,
}

/// A Java thread and its frame stack.
#[derive(Debug)]
pub struct JavaThread {
    /// Index of this thread in the VM's thread table.
    pub index: usize,
    /// The `java.lang.Thread` object.
    pub object: ObjectRef,
    /// The thread name.
    pub name: String,
    /// Its frames, oldest first.
    pub frames: Vec<Frame>,
    /// Scheduling state.
    pub state: ThreadState,
    /// Priority (kept for `getPriority`, not used for scheduling).
    pub priority: i32,
    /// Whether the thread is a daemon.
    pub daemon: bool,
    /// The uncaught exception, if any.
    pub uncaught: Option<ObjectRef>,
    /// The interrupt flag.
    pub interrupted: bool,
    /// Whether `start` has been called.
    pub started: bool,
    /// A pending resume action after a cooperative suspension.
    pub resume: crate::Resume,
    /// Exception-unwind floor: unwinding below this depth returns to the nested caller.
    pub floor: usize,
}

impl JavaThread {
    /// A fresh, unstarted thread.
    #[must_use]
    pub fn new(index: usize, object: ObjectRef, name: String, daemon: bool) -> Self {
        Self {
            index,
            object,
            name,
            frames: Vec::new(),
            state: ThreadState::Runnable,
            priority: 5,
            daemon,
            uncaught: None,
            interrupted: false,
            started: false,
            resume: crate::Resume::None,
            floor: 0,
        }
    }

    /// Whether the thread can execute an instruction now.
    #[must_use]
    pub const fn is_runnable(&self) -> bool {
        matches!(self.state, ThreadState::Runnable | ThreadState::Yielded)
    }

    /// Depth of the frame stack.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.frames.len()
    }
}

impl<'a> Vm<'a> {
    /// The currently scheduled thread.
    pub(crate) fn current_thread(&self) -> &JavaThread {
        &self.threads[self.current]
    }

    /// The currently scheduled thread, mutably.
    pub(crate) fn current_thread_mut(&mut self) -> &mut JavaThread {
        &mut self.threads[self.current]
    }

    /// The top frame of the current thread.
    pub(crate) fn frame(&self) -> &Frame {
        self.current_thread()
            .frames
            .last()
            .expect("no active frame")
    }

    /// The top frame of the current thread, mutably.
    pub(crate) fn frame_mut(&mut self) -> &mut Frame {
        self.current_thread_mut()
            .frames
            .last_mut()
            .expect("no active frame")
    }

    /// Push a value onto the current frame's operand stack.
    pub(crate) fn push(&mut self, value: Value) {
        self.frame_mut().stack.push(value);
    }

    /// Pop a value; panics with the instruction context when the stack is empty (a verifier bug).
    pub(crate) fn pop(&mut self) -> Value {
        if self.frame().stack.is_empty() {
            let frame = self.frame();
            let class = &self.classes.get(frame.class);
            let method = &class.methods[frame.method as usize];
            let context = alloc::format!(
                "{}.{}{} at pc {}",
                class.name,
                method.name,
                method.descriptor,
                frame.pc
            );
            panic!("operand stack underflow in {context}");
        }
        self.frame_mut().stack.pop().expect("checked")
    }

    /// Pop an int-shaped value.
    pub(crate) fn pop_int(&mut self) -> i32 {
        self.pop().as_int()
    }

    /// Pop a reference.
    pub(crate) fn pop_ref(&mut self) -> ObjectRef {
        self.pop().as_ref()
    }

    /// Peek `depth` values down from the top (`0` is the top).
    pub(crate) fn peek(&self, depth: usize) -> Value {
        let stack = &self.frame().stack;
        stack[stack.len() - 1 - depth]
    }

    /// Set a local variable and clear any category-2 continuation slot.
    pub(crate) fn set_local(&mut self, index: usize, value: Value) {
        let frame = self.frame_mut();
        let slots = value.slots() as usize;
        frame.locals[index] = Some(value);
        if slots == 2 && index + 1 < frame.locals.len() {
            frame.locals[index + 1] = None;
        }
    }

    /// Read a local variable.
    pub(crate) fn local(&self, index: usize) -> Value {
        match self.frame().locals.get(index).copied().flatten() {
            Some(value) => value,
            None => Value::Int(0),
        }
    }

    /// The current virtual time in milliseconds.
    #[must_use]
    pub fn virtual_time_millis(&self) -> i64 {
        self.virtual_time
    }

    /// Create a `java.lang.Thread` object. The returned index identifies the scheduler record.
    pub(crate) fn create_thread(
        &mut self,
        name: &str,
        daemon: bool,
        runnable: Option<ObjectRef>,
    ) -> Result<usize, VmError> {
        let thread_class = self.resolve_class("java/lang/Thread")?;
        let object = self.new_instance(thread_class)?;
        self.set_string_field(object, thread_class, "name", name)?;
        self.set_boolean_field(object, thread_class, "daemon", daemon)?;
        if let Some(target) = runnable {
            let (declaring, field) =
                self.find_field(thread_class, "target", "Ljava/lang/Runnable;")?;
            self.set_instance_ref(object, declaring, field, target);
        }
        let index = self.threads.len();
        let mut thread = JavaThread::new(index, object, name.into(), daemon);
        thread.started = runnable.is_some();
        self.threads.push(thread);
        self.thread_objects.insert(object, index);
        Ok(index)
    }

    /// Start a Java thread from a `java.lang.Thread` object: schedule `run()V` on it.
    pub fn start_thread(&mut self, object: ObjectRef) -> Result<(), VmError> {
        let thread_class = self.class_of(object);
        let (declaring, field) = self.find_field(thread_class, "started", "Z")?;
        let already = self
            .read_instance_int(object, declaring, field)
            .unwrap_or(0);
        if already != 0 {
            return Err(self.throw_new(
                "java/lang/IllegalThreadStateException",
                Some("thread already started"),
            ));
        }
        self.set_boolean_field(object, thread_class, "started", true)?;
        let index = match self.thread_objects.get(&object) {
            Some(&index) => index,
            None => {
                let name = self
                    .read_string_field(object, thread_class, "name")
                    .unwrap_or_else(|| String::from("Thread"));
                let (daemon_class, daemon_field) = self.find_field(thread_class, "daemon", "Z")?;
                let daemon = self
                    .read_instance_int(object, daemon_class, daemon_field)
                    .unwrap_or(0)
                    != 0;
                let index = self.threads.len();
                let mut thread = JavaThread::new(index, object, name, daemon);
                thread.started = true;
                self.threads.push(thread);
                self.thread_objects.insert(object, index);
                index
            }
        };
        self.threads[index].state = ThreadState::Runnable;
        self.threads[index].started = true;
        // Push a frame for `run()V`, dispatched virtually so subclasses override.
        let (class, method) = self.resolve_virtual_method(object, "run", "()V")?;
        let frame = self.build_frame(class, method, alloc::vec![Value::Ref(object)], 0, None)?;
        self.threads[index].frames.push(frame);
        Ok(())
    }

    /// Find the next runnable thread, waking sleepers whose deadline has passed.
    pub(crate) fn pick_thread(&mut self) -> Option<usize> {
        let now = self.virtual_time;
        // First pass: wake threads whose deadlines expired.
        let mut wake: Vec<usize> = Vec::new();
        for (index, thread) in self.threads.iter().enumerate() {
            let expired = match &thread.state {
                ThreadState::Sleeping(deadline) => now >= *deadline,
                ThreadState::Waiting {
                    deadline: Some(deadline),
                    ..
                } => now >= *deadline,
                ThreadState::Joining {
                    target,
                    deadline: Some(deadline),
                } => now >= *deadline || self.threads[*target].state == ThreadState::Terminated,
                _ => false,
            };
            let joined = match &thread.state {
                ThreadState::Joining {
                    target,
                    deadline: None,
                } => self.threads[*target].state == ThreadState::Terminated,
                _ => false,
            };
            if expired || joined {
                wake.push(index);
            }
        }
        for index in wake {
            self.threads[index].state = ThreadState::Runnable;
        }
        // Prefer the current thread unless it voluntarily yielded.
        let current = self.current;
        if current < self.threads.len() {
            let thread = &mut self.threads[current];
            if thread.is_runnable() && thread.state != ThreadState::Yielded {
                return Some(current);
            }
        }
        let count = self.threads.len();
        for offset in 1..=count {
            let index = (current + offset) % count;
            let thread = &mut self.threads[index];
            if thread.is_runnable() {
                thread.state = ThreadState::Runnable;
                return Some(index);
            }
        }
        // A yielded thread with nobody else to run keeps the CPU.
        if current < self.threads.len() && self.threads[current].is_runnable() {
            return Some(current);
        }
        None
    }

    /// Advance the virtual clock when every thread is sleeping or joining with a deadline.
    ///
    /// Returns true when time moved (and threads may have woken).
    pub(crate) fn advance_virtual_time(&mut self) -> bool {
        let mut earliest: Option<i64> = None;
        for thread in &self.threads {
            let deadline = match thread.state {
                ThreadState::Sleeping(deadline) => Some(deadline),
                ThreadState::Waiting {
                    deadline: Some(deadline),
                    ..
                }
                | ThreadState::Joining {
                    deadline: Some(deadline),
                    ..
                } => Some(deadline),
                _ => None,
            };
            if let Some(deadline) = deadline {
                earliest = Some(earliest.map_or(deadline, |current: i64| current.min(deadline)));
            }
        }
        match earliest {
            Some(deadline) if deadline > self.virtual_time => {
                self.virtual_time = deadline;
                true
            }
            _ => false,
        }
    }

    /// Move the current thread to a state and schedule someone else.
    pub(crate) fn park_current(&mut self, state: ThreadState) {
        self.current_thread_mut().state = state;
    }

    /// Mark a thread terminated and wake joiners and monitor entrants.
    pub(crate) fn terminate_current(&mut self) {
        let index = self.current;
        self.threads[index].frames.clear();
        self.threads[index].state = ThreadState::Terminated;
        for thread in &mut self.threads {
            if let ThreadState::Joining { target, .. } = thread.state {
                if target == index {
                    thread.state = ThreadState::Runnable;
                }
            }
        }
        // Release every monitor the thread still holds.
        let capacity = self.heap.capacity_slots();
        for slot in 0..capacity {
            let reference = ObjectRef::from_raw(slot as u32 + 1);
            let owns = self
                .heap
                .get(reference)
                .is_some_and(|object| object.monitor.owner == Some(index));
            if owns {
                self.heap.get_mut(reference).expect("checked").monitor.owner = None;
                let mut wake = None;
                let mut count = 0;
                if let Some(object) = self.heap.get_mut(reference) {
                    object.monitor.count = 0;
                    wake = object.monitor.entrants.pop_front();
                    count = object.monitor.entrants.len();
                }
                let _ = count;
                if let Some(thread) = wake {
                    if thread != index {
                        self.threads[thread].state = ThreadState::Runnable;
                    }
                }
            }
        }
    }
}
