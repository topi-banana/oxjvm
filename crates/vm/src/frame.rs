//! Java method activation frames.

use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::class::{ClassId, Code};
use crate::value::{ObjectRef, Value};

/// One activation of an interpreted Java method.
#[derive(Debug, Clone)]
pub struct Frame {
    /// The class declaring the executing method.
    pub class: ClassId,
    /// Index of the method in its class.
    pub method: u32,
    /// The method body.
    pub code: Arc<Code>,
    /// Local variables, one slot each; category-2 values occupy their value slot plus a `None`
    /// continuation slot.
    pub locals: Vec<Option<Value>>,
    /// The operand stack.
    pub stack: Vec<Value>,
    /// The program counter, pointing at the instruction being executed (or the one being resumed).
    pub pc: usize,
    /// The pc of the instruction currently faulting, used for exception-handler lookup even after
    /// `pc` has advanced past it.
    pub fault_pc: usize,
    /// Where the caller resumes when this frame returns.
    pub return_pc: usize,
    /// The intrinsic lock held by this frame for a `synchronized` method.
    pub monitor: Option<ObjectRef>,
    /// The line currently executing, for stack traces.
    pub line: u16,
}

impl Frame {
    /// Create a frame for a method.
    #[must_use]
    pub fn new(
        class: ClassId,
        method: u32,
        code: Arc<Code>,
        locals: Vec<Option<Value>>,
        return_pc: usize,
    ) -> Self {
        let max_locals = code.max_locals as usize;
        let mut locals = locals;
        locals.resize(max_locals, None);
        Self {
            class,
            method,
            code,
            locals,
            stack: Vec::new(),
            pc: 0,
            fault_pc: 0,
            return_pc,
            monitor: None,
            line: 0,
        }
    }

    /// The current method's opcode byte, if the pc is in range.
    #[must_use]
    pub fn opcode(&self) -> Option<u8> {
        self.code.bytes.get(self.pc).copied()
    }
}
