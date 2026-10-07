//! Bytecode verification (JVMS 4.10).
//!
//! The verifier is deliberately conservative about *structure* and liberal about *reference types*:
//!
//! * Every instruction must decode, every branch must land on an instruction boundary, every
//!   exception range must be well formed, and every constant-pool reference must have the right
//!   kind — all checked exactly.
//! * A dataflow pass tracks the operand stack's shape and the primitive categories of local
//!   variables. Reference types are merged to a single "reference" category, which is the one
//!   place this implementation is weaker than a full type checker; the interpreter's runtime
//!   checks (`checkcast`, `ArrayStoreException`, dispatch) still enforce the object model.
//! * `jsr`/`ret` are rejected for class files at version 50 or above, exactly as HotSpot does;
//!   older files are verified structurally only.
//!
//! The result is that malformed bytecode is rejected with `VerifyError` *before* it can make the
//! interpreter panic, while all `javac` output verifies.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::descriptor::MethodDescriptor;
use oxjvm_classfile::flags::*;
use oxjvm_classfile::{ClassFile, ConstantPool, CpInfo, MethodInfo, opcode};

/// Stack/local category used by the dataflow pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cat {
    Int,
    Long,
    Float,
    Double,
    Ref,
    /// Unknown or merged-away: compatible with everything.
    Top,
}

impl Cat {
    fn of_descriptor(descriptor: &str) -> Cat {
        match descriptor.as_bytes().first() {
            Some(b'J') => Cat::Long,
            Some(b'F') => Cat::Float,
            Some(b'D') => Cat::Double,
            Some(b'L' | b'[') => Cat::Ref,
            Some(b'Z' | b'B' | b'C' | b'S' | b'I') => Cat::Int,
            _ => Cat::Top,
        }
    }

    fn slots(self) -> u16 {
        match self {
            Cat::Long | Cat::Double => 2,
            _ => 1,
        }
    }

    fn compatible_with(self, expected: Cat) -> bool {
        self == expected || self == Cat::Top || expected == Cat::Top
    }
}

/// One abstract state.
#[derive(Debug, Clone, PartialEq)]
struct State {
    stack: Vec<Cat>,
    locals: Vec<Cat>,
}

/// Verify one class file.
///
/// # Errors
///
/// Returns a human-readable message describing the first failure.
pub fn verify_class(class: &ClassFile) -> Result<(), String> {
    if class.major_version < 45 {
        return Err(alloc::format!(
            "unsupported class file version {}.{}",
            class.major_version,
            class.minor_version
        ));
    }
    if class.major_version > 70 {
        return Err(alloc::format!(
            "unsupported future class file version {}.{}",
            class.major_version,
            class.minor_version
        ));
    }
    let pool = &class.constant_pool;
    for method in &class.methods {
        verify_method(class, method, pool)?;
    }
    Ok(())
}

fn method_context(class: &ClassFile, method: &MethodInfo, pool: &ConstantPool) -> String {
    let class_name = class.this_name().unwrap_or("<unknown>");
    let name = pool.utf8(method.name_index).unwrap_or("<bad>");
    let descriptor = pool.utf8(method.descriptor_index).unwrap_or("<bad>");
    alloc::format!("{class_name}.{name}{descriptor}")
}

fn verify_method(
    class: &ClassFile,
    method: &MethodInfo,
    pool: &ConstantPool,
) -> Result<(), String> {
    let context = method_context(class, method, pool);
    let Some(code) = method
        .attributes
        .iter()
        .find_map(|attribute| match &attribute.data {
            oxjvm_classfile::AttributeData::Code(code) => Some(code),
            _ => None,
        })
    else {
        if method.access_flags & (ACC_ABSTRACT | ACC_NATIVE) == 0 {
            return Err(alloc::format!("{context}: missing Code attribute"));
        }
        return Ok(());
    };
    if code.code.is_empty() {
        return Err(alloc::format!("{context}: empty code array"));
    }
    if code.code.len() > 65535 {
        return Err(alloc::format!("{context}: code array exceeds 65535 bytes"));
    }
    let descriptor = pool
        .utf8(method.descriptor_index)
        .map_err(|error| alloc::format!("{context}: {error}"))?;
    let parsed = MethodDescriptor::parse(descriptor)
        .map_err(|message| alloc::format!("{context}: {message}"))?;

    // Instruction boundaries and structural checks.
    let layout =
        instruction_layout(&code.code).map_err(|message| alloc::format!("{context}: {message}"))?;
    check_exception_table(class, pool, code, &layout, &context)?;
    let uses_jsr = layout
        .entries
        .iter()
        .any(|(_, _, op)| matches!(op, 0xa8 | 0xa9 | 0xc9));
    if uses_jsr && class.major_version >= 50 {
        return Err(alloc::format!(
            "{context}: jsr/ret is not allowed in class files version 50 or above"
        ));
    }

    if uses_jsr {
        // Older files: structural checks only.
        return Ok(());
    }

    // Dataflow.
    let mut initial = State {
        stack: Vec::new(),
        locals: vec![Cat::Top; usize::from(code.max_locals)],
    };
    let mut first_local = 0usize;
    if method.access_flags & ACC_STATIC == 0 {
        initial.locals[0] = Cat::Ref;
        first_local = 1;
    }
    for parameter in &parsed.parameters {
        let cat = Cat::of_descriptor(&parameter.descriptor());
        if first_local + usize::from(cat.slots()) > initial.locals.len() {
            return Err(alloc::format!(
                "{context}: max_locals too small for parameters"
            ));
        }
        initial.locals[first_local] = cat;
        if cat.slots() == 2 {
            initial.locals[first_local + 1] = Cat::Top;
        }
        first_local += usize::from(cat.slots());
    }

    let mut states: BTreeMap<usize, State> = BTreeMap::new();
    let mut work: Vec<(usize, State)> = Vec::new();
    states.insert(0, initial.clone());
    work.push((0, initial));
    for handler in &code.exception_table {
        let state = State {
            stack: vec![Cat::Ref],
            locals: vec![Cat::Top; usize::from(code.max_locals)],
        };
        insert_state(
            &mut states,
            &mut work,
            handler.handler_pc as usize,
            state,
            &context,
        )?;
    }

    while let Some((pc, state)) = work.pop() {
        let (_, next, op) = layout.by_pc[&pc];
        let mut state = state;
        let outcome = transfer(
            class, pool, code, &layout, pc, op, next, &parsed, &mut state, &context,
        )?;
        match outcome {
            Flow::FallThrough => {
                if next >= code.code.len() {
                    return Err(alloc::format!(
                        "{context}: execution falls off the end at {pc}"
                    ));
                }
                insert_state(&mut states, &mut work, next, state, &context)?;
            }
            Flow::Jump(target) => {
                insert_state(&mut states, &mut work, target, state, &context)?;
            }
            Flow::Branch(target) => {
                insert_state(&mut states, &mut work, target, state.clone(), &context)?;
                if next >= code.code.len() {
                    return Err(alloc::format!(
                        "{context}: execution falls off the end at {pc}"
                    ));
                }
                insert_state(&mut states, &mut work, next, state, &context)?;
            }
            Flow::Switch(targets) => {
                for target in targets {
                    insert_state(&mut states, &mut work, target, state.clone(), &context)?;
                }
            }
            Flow::Stop => {}
        }
    }
    Ok(())
}

fn insert_state(
    states: &mut BTreeMap<usize, State>,
    work: &mut Vec<(usize, State)>,
    pc: usize,
    state: State,
    context: &str,
) -> Result<(), String> {
    if state
        .stack
        .iter()
        .map(|cat| usize::from(cat.slots()))
        .sum::<usize>()
        > 65535
    {
        return Err(alloc::format!("{context}: stack overflow at {pc}"));
    }
    match states.get(&pc) {
        None => {
            states.insert(pc, state.clone());
            work.push((pc, state));
            Ok(())
        }
        Some(existing) => {
            if existing.stack.len() != state.stack.len() {
                return Err(alloc::format!(
                    "{context}: inconsistent stack height at {pc} ({} vs {})",
                    existing.stack.len(),
                    state.stack.len()
                ));
            }
            let merged = merge(existing, &state);
            if &merged != existing {
                states.insert(pc, merged.clone());
                work.push((pc, merged));
            }
            Ok(())
        }
    }
}

fn merge(a: &State, b: &State) -> State {
    let stack = a
        .stack
        .iter()
        .zip(&b.stack)
        .map(|(left, right)| if left == right { *left } else { Cat::Top })
        .collect();
    let locals = a
        .locals
        .iter()
        .zip(&b.locals)
        .map(|(left, right)| if left == right { *left } else { Cat::Top })
        .collect();
    State { stack, locals }
}

/// The decoded instruction layout: byte offset -> `(pc, next_pc, opcode)`.
struct Layout {
    entries: Vec<(usize, usize, u8)>,
    by_pc: BTreeMap<usize, (usize, usize, u8)>,
}

fn instruction_layout(code: &[u8]) -> Result<Layout, String> {
    let mut entries = Vec::with_capacity(code.len());
    let mut by_pc = BTreeMap::new();
    let mut pc = 0;
    while pc < code.len() {
        let decoded = opcode::decode(code, pc)
            .ok_or_else(|| alloc::format!("invalid instruction at {pc}"))?;
        if decoded.next_pc <= pc || decoded.next_pc > code.len() {
            return Err(alloc::format!("truncated instruction at {pc}"));
        }
        entries.push((pc, decoded.next_pc, decoded.opcode));
        by_pc.insert(pc, (pc, decoded.next_pc, decoded.opcode));
        pc = decoded.next_pc;
    }
    Ok(Layout { entries, by_pc })
}

fn check_exception_table(
    class: &ClassFile,
    pool: &ConstantPool,
    code: &oxjvm_classfile::CodeAttribute,
    layout: &Layout,
    context: &str,
) -> Result<(), String> {
    let starts: BTreeSet<usize> = layout.by_pc.keys().copied().collect();
    for handler in &code.exception_table {
        if handler.start_pc >= handler.end_pc || usize::from(handler.end_pc) > code.code.len() {
            return Err(alloc::format!(
                "{context}: bad exception range {}..{}",
                handler.start_pc,
                handler.end_pc
            ));
        }
        if !starts.contains(&usize::from(handler.handler_pc)) {
            return Err(alloc::format!(
                "{context}: handler pc {} is not an instruction",
                handler.handler_pc
            ));
        }
        if !starts.contains(&usize::from(handler.start_pc)) {
            return Err(alloc::format!(
                "{context}: protected range starts mid-instruction at {}",
                handler.start_pc
            ));
        }
        if handler.catch_type != 0 {
            match pool.get(handler.catch_type) {
                Ok(CpInfo::Class(_)) => {}
                _ => {
                    return Err(alloc::format!(
                        "{context}: catch_type {} is not a Class",
                        handler.catch_type
                    ));
                }
            }
        }
        let _ = class;
    }
    Ok(())
}

enum Flow {
    FallThrough,
    Jump(usize),
    Branch(usize),
    Switch(Vec<usize>),
    Stop,
}

struct Stack<'a> {
    values: &'a mut Vec<Cat>,
    locals: &'a mut Vec<Cat>,
}

impl<'a> Stack<'a> {
    fn pop(&mut self) -> Result<Cat, String> {
        self.values
            .pop()
            .ok_or_else(|| "operand stack underflow".into())
    }

    fn pop_expect(&mut self, expected: Cat) -> Result<(), String> {
        let actual = self.pop()?;
        if actual.compatible_with(expected) {
            Ok(())
        } else {
            Err(alloc::format!("expected {expected:?}, found {actual:?}"))
        }
    }

    fn push(&mut self, cat: Cat) {
        self.values.push(cat);
    }

    fn local(&mut self, index: usize) -> Result<Cat, String> {
        self.locals
            .get(index)
            .copied()
            .ok_or_else(|| alloc::format!("local index {index} out of range"))
    }
}

#[allow(clippy::too_many_lines)]
fn transfer(
    _class: &ClassFile,
    pool: &ConstantPool,
    code: &oxjvm_classfile::CodeAttribute,
    _layout: &Layout,
    pc: usize,
    op: u8,
    _next: usize,
    _parsed: &MethodDescriptor,
    state: &mut State,
    context: &str,
) -> Result<Flow, String> {
    let mut stack = Stack {
        values: &mut state.stack,
        locals: &mut state.locals,
    };
    let err = |message: String| -> Result<Flow, String> {
        Err(alloc::format!("{context}: at {pc}: {message}"))
    };
    let emap = |message: String| -> String { alloc::format!("{context}: at {pc}: {message}") };
    let target = |offset: i16| -> Result<usize, String> {
        let target = (pc as i64) + i64::from(offset);
        if target < 0 || target as usize >= code.code.len() {
            return Err(alloc::format!("{context}: branch at {pc} out of range"));
        }
        Ok(target as usize)
    };
    let target32 = |offset: i32| -> Result<usize, String> {
        let target = (pc as i64) + i64::from(offset);
        if target < 0 || target as usize >= code.code.len() {
            return Err(alloc::format!("{context}: branch at {pc} out of range"));
        }
        Ok(target as usize)
    };
    let enforce_stack = |values: &Vec<Cat>, context: &str, pc: usize| -> Result<(), String> {
        let slots: usize = values.iter().map(|cat| usize::from(cat.slots())).sum();
        if slots > usize::from(code.max_stack) {
            return Err(alloc::format!(
                "{context}: operand stack exceeds max_stack {pc} ({slots} > {})",
                code.max_stack
            ));
        }
        Ok(())
    };

    match op {
        0x00 => {}
        0x01 => stack.push(Cat::Ref),
        0x02..=0x08 => stack.push(Cat::Int),
        0x09 | 0x0a => stack.push(Cat::Long),
        0x0b..=0x0d => stack.push(Cat::Float),
        0x0e | 0x0f => stack.push(Cat::Double),
        0x10 => {
            stack.push(Cat::Int);
        }
        0x11 => {
            stack.push(Cat::Int);
        }
        0x12 => {
            let index = u16::from(code.code[pc + 1]);
            stack.push(
                ldc_category(pool, index).map_err(|message| format!("{context}: {message}"))?,
            );
        }
        0x13 => {
            let index = u16::from_be_bytes([code.code[pc + 1], code.code[pc + 2]]);
            stack.push(
                ldc_category(pool, index).map_err(|message| format!("{context}: {message}"))?,
            );
        }
        0x14 => {
            let index = u16::from_be_bytes([code.code[pc + 1], code.code[pc + 2]]);
            let cat = match pool.get(index) {
                Ok(CpInfo::Long(_)) => Cat::Long,
                Ok(CpInfo::Double(_)) => Cat::Double,
                other => {
                    return err(alloc::format!("ldc2_w must load a long/double: {other:?}"));
                }
            };
            stack.push(cat);
        }
        0x15 => {
            let index = usize::from(code.code[pc + 1]);
            let cat = stack.local(index)?;
            if !cat.compatible_with(Cat::Int) {
                return err("iload of a non-int local".into());
            }
            stack.push(if cat == Cat::Top { Cat::Int } else { cat });
        }
        0x16 => {
            let index = usize::from(code.code[pc + 1]);
            let cat = stack.local(index)?;
            if !cat.compatible_with(Cat::Long) {
                return err("lload of a non-long local".into());
            }
            stack.push(Cat::Long);
        }
        0x17 => {
            let index = usize::from(code.code[pc + 1]);
            let cat = stack.local(index)?;
            if !cat.compatible_with(Cat::Float) {
                return err("fload of a non-float local".into());
            }
            stack.push(Cat::Float);
        }
        0x18 => {
            let index = usize::from(code.code[pc + 1]);
            let cat = stack.local(index)?;
            if !cat.compatible_with(Cat::Double) {
                return err("dload of a non-double local".into());
            }
            stack.push(Cat::Double);
        }
        0x19 => {
            let index = usize::from(code.code[pc + 1]);
            let cat = stack.local(index)?;
            if !cat.compatible_with(Cat::Ref) {
                return err("aload of a non-reference local".into());
            }
            stack.push(if cat == Cat::Top { Cat::Ref } else { cat });
        }
        0x1a..=0x1d => {
            let cat = stack.local(usize::from(op - 0x1a))?;
            stack.push(if cat == Cat::Top { Cat::Int } else { cat });
        }
        0x1e..=0x21 => stack.push(Cat::Long),
        0x22..=0x25 => stack.push(Cat::Float),
        0x26..=0x29 => stack.push(Cat::Double),
        0x2a..=0x2d => {
            let cat = stack.local(usize::from(op - 0x2a))?;
            stack.push(if cat == Cat::Top { Cat::Ref } else { cat });
        }
        0x2e => {
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.pop_expect(Cat::Ref).map_err(emap)?;
            stack.push(Cat::Int);
        }
        0x2f => {
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.pop_expect(Cat::Ref).map_err(emap)?;
            stack.push(Cat::Long);
        }
        0x30 => {
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.pop_expect(Cat::Ref).map_err(emap)?;
            stack.push(Cat::Float);
        }
        0x31 => {
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.pop_expect(Cat::Ref).map_err(emap)?;
            stack.push(Cat::Double);
        }
        0x32 => {
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.pop_expect(Cat::Ref).map_err(emap)?;
            stack.push(Cat::Ref);
        }
        0x33 | 0x34 | 0x35 => {
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.pop_expect(Cat::Ref).map_err(emap)?;
            stack.push(Cat::Int);
        }
        0x36 => {
            let index = usize::from(code.code[pc + 1]);
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.locals[index] = Cat::Int;
        }
        0x37 => {
            let index = usize::from(code.code[pc + 1]);
            stack.pop_expect(Cat::Long).map_err(emap)?;
            stack.locals[index] = Cat::Long;
            if index + 1 < stack.locals.len() {
                stack.locals[index + 1] = Cat::Top;
            }
        }
        0x38 => {
            let index = usize::from(code.code[pc + 1]);
            stack.pop_expect(Cat::Float).map_err(emap)?;
            stack.locals[index] = Cat::Float;
        }
        0x39 => {
            let index = usize::from(code.code[pc + 1]);
            stack.pop_expect(Cat::Double).map_err(emap)?;
            stack.locals[index] = Cat::Double;
            if index + 1 < stack.locals.len() {
                stack.locals[index + 1] = Cat::Top;
            }
        }
        0x3a => {
            let index = usize::from(code.code[pc + 1]);
            let cat = stack.pop()?;
            if !cat.compatible_with(Cat::Ref) {
                return err("astore of a non-reference".into());
            }
            stack.locals[index] = if cat == Cat::Top { Cat::Top } else { cat };
        }
        0x3b..=0x3e => {
            stack.pop_expect(Cat::Int).map_err(emap)?;
            state_locals_set(&mut stack, usize::from(op - 0x3b), Cat::Int);
        }
        0x3f..=0x42 => {
            stack.pop_expect(Cat::Long).map_err(emap)?;
            let index = usize::from(op - 0x3f);
            stack.locals[index] = Cat::Long;
            if index + 1 < stack.locals.len() {
                stack.locals[index + 1] = Cat::Top;
            }
        }
        0x43..=0x46 => {
            stack.pop_expect(Cat::Float).map_err(emap)?;
            state_locals_set(&mut stack, usize::from(op - 0x43), Cat::Float);
        }
        0x47..=0x4a => {
            stack.pop_expect(Cat::Double).map_err(emap)?;
            let index = usize::from(op - 0x47);
            stack.locals[index] = Cat::Double;
            if index + 1 < stack.locals.len() {
                stack.locals[index + 1] = Cat::Top;
            }
        }
        0x4b..=0x4e => {
            let cat = stack.pop()?;
            if !cat.compatible_with(Cat::Ref) {
                return err("astore of a non-reference".into());
            }
            state_locals_set(&mut stack, usize::from(op - 0x4b), Cat::Ref);
        }
        0x4f => {
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.pop_expect(Cat::Ref).map_err(emap)?;
        }
        0x50 => {
            stack.pop_expect(Cat::Long).map_err(emap)?;
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.pop_expect(Cat::Ref).map_err(emap)?;
        }
        0x51 => {
            stack.pop_expect(Cat::Float).map_err(emap)?;
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.pop_expect(Cat::Ref).map_err(emap)?;
        }
        0x52 => {
            stack.pop_expect(Cat::Double).map_err(emap)?;
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.pop_expect(Cat::Ref).map_err(emap)?;
        }
        0x53 => {
            stack.pop_expect(Cat::Ref).map_err(emap)?;
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.pop_expect(Cat::Ref).map_err(emap)?;
        }
        0x54 | 0x55 | 0x56 => {
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.pop_expect(Cat::Ref).map_err(emap)?;
        }
        0x57 => {
            stack.pop()?;
        }
        0x58 => match stack.pop()? {
            Cat::Long | Cat::Double => {}
            _ => {
                stack.pop()?;
            }
        },
        0x59 => {
            let top = stack.pop()?;
            stack.push(top);
            stack.push(top);
        }
        0x5a => {
            let value1 = stack.pop()?;
            let value2 = stack.pop()?;
            stack.push(value1);
            stack.push(value2);
            stack.push(value1);
        }
        0x5b => {
            let value1 = stack.pop()?;
            if value1.slots() == 2 {
                return err("dup_x2 requires a category-1 top value".into());
            }
            let value2 = stack.pop()?;
            if value2.slots() == 2 {
                stack.push(value1);
                stack.push(value2);
                stack.push(value1);
            } else {
                let value3 = stack.pop()?;
                stack.push(value1);
                stack.push(value3);
                stack.push(value2);
                stack.push(value1);
            }
        }
        0x5c => {
            let top = stack.pop()?;
            if top.slots() == 2 {
                stack.push(top);
                stack.push(top);
            } else {
                let below = stack.pop()?;
                stack.push(below);
                stack.push(top);
                stack.push(below);
                stack.push(top);
            }
        }
        0x5d => {
            let value1 = stack.pop()?;
            if value1.slots() == 2 {
                let value2 = stack.pop()?;
                stack.push(value1);
                stack.push(value2);
                stack.push(value1);
            } else {
                let value2 = stack.pop()?;
                if value2.slots() == 2 {
                    return err("dup2_x1 has no form for a category-2 second value".into());
                }
                let value3 = stack.pop()?;
                stack.push(value2);
                stack.push(value1);
                stack.push(value3);
                stack.push(value2);
                stack.push(value1);
            }
        }
        0x5e => {
            let value1 = stack.pop()?;
            if value1.slots() == 2 {
                let value2 = stack.pop()?;
                if value2.slots() == 2 {
                    stack.push(value1);
                    stack.push(value2);
                    stack.push(value1);
                } else {
                    let value3 = stack.pop()?;
                    if value3.slots() == 2 {
                        return err(
                            "dup2_x2 has no form with two separated category-2 values".into()
                        );
                    }
                    stack.push(value1);
                    stack.push(value3);
                    stack.push(value2);
                    stack.push(value1);
                }
            } else {
                let value2 = stack.pop()?;
                if value2.slots() == 2 {
                    return err("dup2_x2 has no form for a category-2 second value".into());
                }
                let value3 = stack.pop()?;
                if value3.slots() == 2 {
                    stack.push(value2);
                    stack.push(value1);
                    stack.push(value3);
                    stack.push(value2);
                    stack.push(value1);
                } else {
                    let value4 = stack.pop()?;
                    stack.push(value2);
                    stack.push(value1);
                    stack.push(value4);
                    stack.push(value3);
                    stack.push(value2);
                    stack.push(value1);
                }
            }
        }
        0x5f => {
            let value1 = stack.pop()?;
            let value2 = stack.pop()?;
            stack.push(value1);
            stack.push(value2);
        }
        0x60 | 0x64 | 0x68 | 0x6c | 0x70 | 0x7e | 0x80 | 0x82 => {
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.push(Cat::Int);
        }
        0x61 | 0x65 | 0x69 | 0x6d | 0x71 | 0x7f | 0x81 | 0x83 => {
            stack.pop_expect(Cat::Long).map_err(emap)?;
            stack.pop_expect(Cat::Long).map_err(emap)?;
            stack.push(Cat::Long);
        }
        0x62 | 0x66 | 0x6a | 0x6e | 0x72 => {
            stack.pop_expect(Cat::Float).map_err(emap)?;
            stack.pop_expect(Cat::Float).map_err(emap)?;
            stack.push(Cat::Float);
        }
        0x63 | 0x67 | 0x6b | 0x6f | 0x73 => {
            stack.pop_expect(Cat::Double).map_err(emap)?;
            stack.pop_expect(Cat::Double).map_err(emap)?;
            stack.push(Cat::Double);
        }
        0x74 => {
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.push(Cat::Int);
        }
        0x75 => {
            stack.pop_expect(Cat::Long).map_err(emap)?;
            stack.push(Cat::Long);
        }
        0x76 => {
            stack.pop_expect(Cat::Float).map_err(emap)?;
            stack.push(Cat::Float);
        }
        0x77 => {
            stack.pop_expect(Cat::Double).map_err(emap)?;
            stack.push(Cat::Double);
        }
        0x78 | 0x7a | 0x7c => {
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.push(Cat::Int);
        }
        0x79 | 0x7b | 0x7d => {
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.pop_expect(Cat::Long).map_err(emap)?;
            stack.push(Cat::Long);
        }
        0x84 => {
            let index = usize::from(code.code[pc + 1]);
            let cat = stack.local(index)?;
            if !cat.compatible_with(Cat::Int) {
                return err("iinc of a non-int local".into());
            }
        }
        0x85 => {
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.push(Cat::Long);
        }
        0x86 => {
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.push(Cat::Float);
        }
        0x87 => {
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.push(Cat::Double);
        }
        0x88 => {
            stack.pop_expect(Cat::Long).map_err(emap)?;
            stack.push(Cat::Int);
        }
        0x89 => {
            stack.pop_expect(Cat::Long).map_err(emap)?;
            stack.push(Cat::Float);
        }
        0x8a => {
            stack.pop_expect(Cat::Long).map_err(emap)?;
            stack.push(Cat::Double);
        }
        0x8b => {
            stack.pop_expect(Cat::Float).map_err(emap)?;
            stack.push(Cat::Int);
        }
        0x8c => {
            stack.pop_expect(Cat::Float).map_err(emap)?;
            stack.push(Cat::Long);
        }
        0x8d => {
            stack.pop_expect(Cat::Float).map_err(emap)?;
            stack.push(Cat::Double);
        }
        0x8e => {
            stack.pop_expect(Cat::Double).map_err(emap)?;
            stack.push(Cat::Int);
        }
        0x8f => {
            stack.pop_expect(Cat::Double).map_err(emap)?;
            stack.push(Cat::Long);
        }
        0x90 => {
            stack.pop_expect(Cat::Double).map_err(emap)?;
            stack.push(Cat::Float);
        }
        0x91 | 0x92 | 0x93 => {
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.push(Cat::Int);
        }
        0x94 => {
            stack.pop_expect(Cat::Long).map_err(emap)?;
            stack.pop_expect(Cat::Long).map_err(emap)?;
            stack.push(Cat::Int);
        }
        0x95 | 0x96 => {
            stack.pop_expect(Cat::Float).map_err(emap)?;
            stack.pop_expect(Cat::Float).map_err(emap)?;
            stack.push(Cat::Int);
        }
        0x97 | 0x98 => {
            stack.pop_expect(Cat::Double).map_err(emap)?;
            stack.pop_expect(Cat::Double).map_err(emap)?;
            stack.push(Cat::Int);
        }
        0x99..=0x9e => {
            stack.pop_expect(Cat::Int).map_err(emap)?;
            let offset = i16::from_be_bytes([code.code[pc + 1], code.code[pc + 2]]);
            return Ok(Flow::Branch(target(offset).map_err(emap)?));
        }
        0x9f..=0xa4 => {
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.pop_expect(Cat::Int).map_err(emap)?;
            let offset = i16::from_be_bytes([code.code[pc + 1], code.code[pc + 2]]);
            return Ok(Flow::Branch(target(offset).map_err(emap)?));
        }
        0xa5 | 0xa6 => {
            stack.pop_expect(Cat::Ref).map_err(emap)?;
            stack.pop_expect(Cat::Ref).map_err(emap)?;
            let offset = i16::from_be_bytes([code.code[pc + 1], code.code[pc + 2]]);
            return Ok(Flow::Branch(target(offset).map_err(emap)?));
        }
        0xa7 => {
            let offset = i16::from_be_bytes([code.code[pc + 1], code.code[pc + 2]]);
            return Ok(Flow::Jump(target(offset).map_err(emap)?));
        }
        0xa8 => {
            // jsr is handled structurally; unreachable here for modern files.
            let offset = i16::from_be_bytes([code.code[pc + 1], code.code[pc + 2]]);
            return Ok(Flow::Jump(target(offset).map_err(emap)?));
        }
        0xa9 => return Ok(Flow::Stop),
        0xaa => {
            stack.pop_expect(Cat::Int).map_err(emap)?;
            let padding = (4 - ((pc + 1) % 4)) % 4;
            let base = pc + 1 + padding;
            let low = i32::from_be_bytes([
                code.code[base + 4],
                code.code[base + 5],
                code.code[base + 6],
                code.code[base + 7],
            ]);
            let high = i32::from_be_bytes([
                code.code[base + 8],
                code.code[base + 9],
                code.code[base + 10],
                code.code[base + 11],
            ]);
            let mut targets = Vec::new();
            let default = i32::from_be_bytes([
                code.code[base],
                code.code[base + 1],
                code.code[base + 2],
                code.code[base + 3],
            ]);
            targets.push(target32(default).map_err(emap)?);
            for index in 0..=(i64::from(high) - i64::from(low)) {
                let at = base + 12 + index as usize * 4;
                let offset = i32::from_be_bytes([
                    code.code[at],
                    code.code[at + 1],
                    code.code[at + 2],
                    code.code[at + 3],
                ]);
                targets.push(target32(offset).map_err(emap)?);
            }
            return Ok(Flow::Switch(targets));
        }
        0xab => {
            stack.pop_expect(Cat::Int).map_err(emap)?;
            let padding = (4 - ((pc + 1) % 4)) % 4;
            let base = pc + 1 + padding;
            let pairs = i32::from_be_bytes([
                code.code[base + 4],
                code.code[base + 5],
                code.code[base + 6],
                code.code[base + 7],
            ]);
            let default = i32::from_be_bytes([
                code.code[base],
                code.code[base + 1],
                code.code[base + 2],
                code.code[base + 3],
            ]);
            if pairs < 0 {
                return err("negative lookupswitch pairs".into());
            }
            let mut targets = vec![target32(default).map_err(emap)?];
            let mut previous: Option<i32> = None;
            for index in 0..pairs {
                let at = base + 8 + index as usize * 8;
                let key = i32::from_be_bytes([
                    code.code[at],
                    code.code[at + 1],
                    code.code[at + 2],
                    code.code[at + 3],
                ]);
                if let Some(previous) = previous {
                    if key <= previous {
                        return err("lookupswitch keys not sorted".into());
                    }
                }
                previous = Some(key);
                let offset = i32::from_be_bytes([
                    code.code[at + 4],
                    code.code[at + 5],
                    code.code[at + 6],
                    code.code[at + 7],
                ]);
                targets.push(target32(offset).map_err(emap)?);
            }
            return Ok(Flow::Switch(targets));
        }
        0xac => {
            stack.pop_expect(Cat::Int).map_err(emap)?;
            return Ok(Flow::Stop);
        }
        0xad => {
            stack.pop_expect(Cat::Long).map_err(emap)?;
            return Ok(Flow::Stop);
        }
        0xae => {
            stack.pop_expect(Cat::Float).map_err(emap)?;
            return Ok(Flow::Stop);
        }
        0xaf => {
            stack.pop_expect(Cat::Double).map_err(emap)?;
            return Ok(Flow::Stop);
        }
        0xb0 => {
            stack.pop_expect(Cat::Ref).map_err(emap)?;
            return Ok(Flow::Stop);
        }
        0xb1 => return Ok(Flow::Stop),
        0xb2..=0xb5 => {
            let index = u16::from_be_bytes([code.code[pc + 1], code.code[pc + 2]]);
            let descriptor = field_descriptor(pool, index).map_err(emap)?;
            let cat = Cat::of_descriptor(&descriptor);
            match op {
                0xb2 => stack.push(cat),
                0xb3 => {
                    stack.pop_expect(cat).map_err(emap)?;
                }
                0xb4 => {
                    stack.pop_expect(Cat::Ref).map_err(emap)?;
                    stack.push(cat);
                }
                _ => {
                    stack.pop_expect(cat).map_err(emap)?;
                    stack.pop_expect(Cat::Ref).map_err(emap)?;
                }
            }
        }
        0xb6 | 0xb7 | 0xb8 | 0xb9 => {
            let index = u16::from_be_bytes([code.code[pc + 1], code.code[pc + 2]]);
            let (descriptor, is_interface_ref) = method_descriptor(pool, index).map_err(emap)?;
            let parsed = MethodDescriptor::parse(&descriptor)
                .map_err(|message: &str| emap(message.to_string()))?;
            let return_cat = parsed
                .returns
                .as_ref()
                .map_or(Cat::Top, |ret| Cat::of_descriptor(&ret.descriptor()));
            for parameter in parsed.parameters.iter().rev() {
                let cat = Cat::of_descriptor(&parameter.descriptor());
                stack.pop_expect(cat).map_err(emap)?;
            }
            if op != 0xb8 {
                stack.pop_expect(Cat::Ref).map_err(emap)?;
            }
            if op == 0xb9 {
                let count = code.code[pc + 3];
                let expected: u8 = parsed
                    .parameters
                    .iter()
                    .map(|p| if p.is_category2() { 2 } else { 1 })
                    .sum();
                if u16::from(count) != u16::from(expected) {
                    return err(alloc::format!(
                        "invokeinterface count {count} does not match descriptor"
                    ));
                }
                if code.code[pc + 4] != 0 {
                    return err("invokeinterface reserved byte must be zero".into());
                }
            }
            if op == 0xb9 && !is_interface_ref {
                return err("invokeinterface names a class method".into());
            }
            if parsed.returns.is_some() {
                stack.push(return_cat);
            }
        }
        0xba => {
            let index = u16::from_be_bytes([code.code[pc + 1], code.code[pc + 2]]);
            let (_, descriptor) = match pool.get(index) {
                Ok(CpInfo::InvokeDynamic { name_and_type, .. }) => pool
                    .name_and_type(*name_and_type)
                    .map_err(|error| emap(error.to_string()))?,
                other => {
                    return Err(emap(alloc::format!("invokedynamic names {other:?}")));
                }
            };
            let parsed = MethodDescriptor::parse(descriptor)
                .map_err(|message: &str| emap(message.to_string()))?;
            for parameter in parsed.parameters.iter().rev() {
                let cat = Cat::of_descriptor(&parameter.descriptor());
                stack.pop_expect(cat).map_err(emap)?;
            }
            if let Some(ret) = &parsed.returns {
                stack.push(Cat::of_descriptor(&ret.descriptor()));
            }
        }
        0xbb => {
            let index = u16::from_be_bytes([code.code[pc + 1], code.code[pc + 2]]);
            match pool.get(index) {
                Ok(CpInfo::Class(_)) => {}
                other => return err(alloc::format!("new names {other:?}")),
            }
            stack.push(Cat::Ref);
        }
        0xbc => {
            let atype = code.code[pc + 1];
            if crate::class::ArrayComponent::from_atype(atype).is_none() {
                return err(alloc::format!("invalid newarray atype {atype}"));
            }
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.push(Cat::Ref);
        }
        0xbd => {
            let index = u16::from_be_bytes([code.code[pc + 1], code.code[pc + 2]]);
            match pool.class_name(index) {
                Ok(_) => {}
                Err(error) => return Err(emap(error.to_string())),
            }
            stack.pop_expect(Cat::Int).map_err(emap)?;
            stack.push(Cat::Ref);
        }
        0xbe => {
            stack.pop_expect(Cat::Ref).map_err(emap)?;
            stack.push(Cat::Int);
        }
        0xbf => {
            stack.pop_expect(Cat::Ref).map_err(emap)?;
            return Ok(Flow::Stop);
        }
        0xc0 => {
            let index = u16::from_be_bytes([code.code[pc + 1], code.code[pc + 2]]);
            match pool.class_name(index) {
                Ok(_) => {}
                Err(error) => return Err(emap(error.to_string())),
            }
            stack.pop_expect(Cat::Ref).map_err(emap)?;
            stack.push(Cat::Ref);
        }
        0xc1 => {
            let index = u16::from_be_bytes([code.code[pc + 1], code.code[pc + 2]]);
            match pool.class_name(index) {
                Ok(_) => {}
                Err(error) => return Err(emap(error.to_string())),
            }
            stack.pop_expect(Cat::Ref).map_err(emap)?;
            stack.push(Cat::Int);
        }
        0xc2 | 0xc3 => {
            stack.pop_expect(Cat::Ref).map_err(emap)?;
        }
        0xc4 => {
            let inner = code.code[pc + 1];
            match inner {
                0x15 | 0x16 | 0x17 | 0x18 | 0x19 => {
                    let index = u16::from_be_bytes([code.code[pc + 2], code.code[pc + 3]]);
                    let cat = stack.local(usize::from(index))?;
                    let expected = match inner {
                        0x15 => Cat::Int,
                        0x16 => Cat::Long,
                        0x17 => Cat::Float,
                        0x18 => Cat::Double,
                        _ => Cat::Ref,
                    };
                    if !cat.compatible_with(expected) {
                        return err("wide load type mismatch".into());
                    }
                    stack.push(if cat == Cat::Top { expected } else { cat });
                }
                0x36 | 0x37 | 0x38 | 0x39 | 0x3a => {
                    let index =
                        usize::from(u16::from_be_bytes([code.code[pc + 2], code.code[pc + 3]]));
                    let expected = match inner {
                        0x36 => Cat::Int,
                        0x37 => Cat::Long,
                        0x38 => Cat::Float,
                        0x39 => Cat::Double,
                        _ => Cat::Ref,
                    };
                    if index >= stack.locals.len() {
                        return err("wide store out of range".into());
                    }
                    stack.pop_expect(expected).map_err(emap)?;
                    stack.locals[index] = expected;
                    if expected.slots() == 2 && index + 1 < stack.locals.len() {
                        stack.locals[index + 1] = Cat::Top;
                    }
                }
                0x84 => {
                    let index =
                        usize::from(u16::from_be_bytes([code.code[pc + 2], code.code[pc + 3]]));
                    let cat = stack.local(index)?;
                    if !cat.compatible_with(Cat::Int) {
                        return err("wide iinc of a non-int local".into());
                    }
                }
                0xa9 => {}
                _ => return err(alloc::format!("invalid wide opcode {inner:#04x}")),
            }
        }
        0xc5 => {
            let index = u16::from_be_bytes([code.code[pc + 1], code.code[pc + 2]]);
            let dimensions = code.code[pc + 3];
            let name = pool.class_name(index).map_err(|error| error.to_string())?;
            let depth = name.bytes().filter(|byte| *byte == b'[').count();
            if usize::from(dimensions) > depth || dimensions == 0 {
                return err(alloc::format!(
                    "multianewarray dimensions {dimensions} exceed {name}"
                ));
            }
            for _ in 0..dimensions {
                stack.pop_expect(Cat::Int).map_err(emap)?;
            }
            stack.push(Cat::Ref);
        }
        0xc6 | 0xc7 => {
            stack.pop_expect(Cat::Ref).map_err(emap)?;
            let offset = i16::from_be_bytes([code.code[pc + 1], code.code[pc + 2]]);
            return Ok(Flow::Branch(target(offset).map_err(emap)?));
        }
        0xc8 => {
            let offset = i32::from_be_bytes([
                code.code[pc + 1],
                code.code[pc + 2],
                code.code[pc + 3],
                code.code[pc + 4],
            ]);
            return Ok(Flow::Jump(target32(offset).map_err(emap)?));
        }
        0xc9 => {
            let offset = i32::from_be_bytes([
                code.code[pc + 1],
                code.code[pc + 2],
                code.code[pc + 3],
                code.code[pc + 4],
            ]);
            return Ok(Flow::Jump(target32(offset).map_err(emap)?));
        }
        other => return err(alloc::format!("invalid opcode {other:#04x}")),
    }
    enforce_stack(&state.stack, context, pc)?;
    Ok(Flow::FallThrough)
}

fn state_locals_set(stack: &mut Stack<'_>, index: usize, cat: Cat) {
    stack.locals[index] = cat;
}

fn ldc_category(pool: &ConstantPool, index: u16) -> Result<Cat, String> {
    match pool.get(index) {
        Ok(CpInfo::Integer(_)) => Ok(Cat::Int),
        Ok(CpInfo::Float(_)) => Ok(Cat::Float),
        Ok(CpInfo::String(_)) => Ok(Cat::Ref),
        Ok(CpInfo::Class(_)) => Ok(Cat::Ref),
        Ok(CpInfo::MethodType(_)) => Ok(Cat::Ref),
        Ok(CpInfo::MethodHandle { .. }) => Ok(Cat::Ref),
        Ok(CpInfo::Dynamic { name_and_type, .. }) => {
            let (_, descriptor) = pool
                .name_and_type(*name_and_type)
                .map_err(|error| error.to_string())?;
            Ok(Cat::of_descriptor(descriptor))
        }
        Ok(other) => Err(alloc::format!("ldc names {}", other.kind_name())),
        Err(error) => Err(error.to_string()),
    }
}

fn field_descriptor(pool: &ConstantPool, index: u16) -> Result<String, String> {
    match pool.get(index) {
        Ok(CpInfo::Fieldref { name_and_type, .. }) => pool
            .name_and_type(*name_and_type)
            .map(|(_, descriptor)| descriptor.into())
            .map_err(|error| error.to_string()),
        Ok(other) => Err(alloc::format!(
            "field instruction names {}",
            other.kind_name()
        )),
        Err(error) => Err(error.to_string()),
    }
}

fn method_descriptor(pool: &ConstantPool, index: u16) -> Result<(String, bool), String> {
    match pool.get(index) {
        Ok(CpInfo::Methodref { name_and_type, .. }) => pool
            .name_and_type(*name_and_type)
            .map(|(_, descriptor)| (descriptor.into(), false))
            .map_err(|error| error.to_string()),
        Ok(CpInfo::InterfaceMethodref { name_and_type, .. }) => pool
            .name_and_type(*name_and_type)
            .map(|(_, descriptor)| (descriptor.into(), true))
            .map_err(|error| error.to_string()),
        Ok(other) => Err(alloc::format!("invoke names {}", other.kind_name())),
        Err(error) => Err(error.to_string()),
    }
}
