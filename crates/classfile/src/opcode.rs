//! The JVM instruction set (JVMS ch. 6), one constant per opcode plus a decoder.
//!
//! The interpreter dispatches on the raw opcode byte; this table exists so disassemblers, tracers
//! and verifiers can name instructions and compute their lengths without duplicating the set.

/// The mnemonic of an opcode, or `None` for an undefined (reserved) value.
#[must_use]
pub const fn name(opcode: u8) -> Option<&'static str> {
    Some(match opcode {
        0x00 => "nop",
        0x01 => "aconst_null",
        0x02 => "iconst_m1",
        0x03 => "iconst_0",
        0x04 => "iconst_1",
        0x05 => "iconst_2",
        0x06 => "iconst_3",
        0x07 => "iconst_4",
        0x08 => "iconst_5",
        0x09 => "lconst_0",
        0x0a => "lconst_1",
        0x0b => "fconst_0",
        0x0c => "fconst_1",
        0x0d => "fconst_2",
        0x0e => "dconst_0",
        0x0f => "dconst_1",
        0x10 => "bipush",
        0x11 => "sipush",
        0x12 => "ldc",
        0x13 => "ldc_w",
        0x14 => "ldc2_w",
        0x15 => "iload",
        0x16 => "lload",
        0x17 => "fload",
        0x18 => "dload",
        0x19 => "aload",
        0x1a => "iload_0",
        0x1b => "iload_1",
        0x1c => "iload_2",
        0x1d => "iload_3",
        0x1e => "lload_0",
        0x1f => "lload_1",
        0x20 => "lload_2",
        0x21 => "lload_3",
        0x22 => "fload_0",
        0x23 => "fload_1",
        0x24 => "fload_2",
        0x25 => "fload_3",
        0x26 => "dload_0",
        0x27 => "dload_1",
        0x28 => "dload_2",
        0x29 => "dload_3",
        0x2a => "aload_0",
        0x2b => "aload_1",
        0x2c => "aload_2",
        0x2d => "aload_3",
        0x2e => "iaload",
        0x2f => "laload",
        0x30 => "faload",
        0x31 => "daload",
        0x32 => "aaload",
        0x33 => "baload",
        0x34 => "caload",
        0x35 => "saload",
        0x36 => "istore",
        0x37 => "lstore",
        0x38 => "fstore",
        0x39 => "dstore",
        0x3a => "astore",
        0x3b => "istore_0",
        0x3c => "istore_1",
        0x3d => "istore_2",
        0x3e => "istore_3",
        0x3f => "lstore_0",
        0x40 => "lstore_1",
        0x41 => "lstore_2",
        0x42 => "lstore_3",
        0x43 => "fstore_0",
        0x44 => "fstore_1",
        0x45 => "fstore_2",
        0x46 => "fstore_3",
        0x47 => "dstore_0",
        0x48 => "dstore_1",
        0x49 => "dstore_2",
        0x4a => "dstore_3",
        0x4b => "astore_0",
        0x4c => "astore_1",
        0x4d => "astore_2",
        0x4e => "astore_3",
        0x4f => "iastore",
        0x50 => "lastore",
        0x51 => "fastore",
        0x52 => "dastore",
        0x53 => "aastore",
        0x54 => "bastore",
        0x55 => "castore",
        0x56 => "sastore",
        0x57 => "pop",
        0x58 => "pop2",
        0x59 => "dup",
        0x5a => "dup_x1",
        0x5b => "dup_x2",
        0x5c => "dup2",
        0x5d => "dup2_x1",
        0x5e => "dup2_x2",
        0x5f => "swap",
        0x60 => "iadd",
        0x61 => "ladd",
        0x62 => "fadd",
        0x63 => "dadd",
        0x64 => "isub",
        0x65 => "lsub",
        0x66 => "fsub",
        0x67 => "dsub",
        0x68 => "imul",
        0x69 => "lmul",
        0x6a => "fmul",
        0x6b => "dmul",
        0x6c => "idiv",
        0x6d => "ldiv",
        0x6e => "fdiv",
        0x6f => "ddiv",
        0x70 => "irem",
        0x71 => "lrem",
        0x72 => "frem",
        0x73 => "drem",
        0x74 => "ineg",
        0x75 => "lneg",
        0x76 => "fneg",
        0x77 => "dneg",
        0x78 => "ishl",
        0x79 => "lshl",
        0x7a => "ishr",
        0x7b => "lshr",
        0x7c => "iushr",
        0x7d => "lushr",
        0x7e => "iand",
        0x7f => "land",
        0x80 => "ior",
        0x81 => "lor",
        0x82 => "ixor",
        0x83 => "lxor",
        0x84 => "iinc",
        0x85 => "i2l",
        0x86 => "i2f",
        0x87 => "i2d",
        0x88 => "l2i",
        0x89 => "l2f",
        0x8a => "l2d",
        0x8b => "f2i",
        0x8c => "f2l",
        0x8d => "f2d",
        0x8e => "d2i",
        0x8f => "d2l",
        0x90 => "d2f",
        0x91 => "i2b",
        0x92 => "i2c",
        0x93 => "i2s",
        0x94 => "lcmp",
        0x95 => "fcmpl",
        0x96 => "fcmpg",
        0x97 => "dcmpl",
        0x98 => "dcmpg",
        0x99 => "ifeq",
        0x9a => "ifne",
        0x9b => "iflt",
        0x9c => "ifge",
        0x9d => "ifgt",
        0x9e => "ifle",
        0x9f => "if_icmpeq",
        0xa0 => "if_icmpne",
        0xa1 => "if_icmplt",
        0xa2 => "if_icmpge",
        0xa3 => "if_icmpgt",
        0xa4 => "if_icmple",
        0xa5 => "if_acmpeq",
        0xa6 => "if_acmpne",
        0xa7 => "goto",
        0xa8 => "jsr",
        0xa9 => "ret",
        0xaa => "tableswitch",
        0xab => "lookupswitch",
        0xac => "ireturn",
        0xad => "lreturn",
        0xae => "freturn",
        0xaf => "dreturn",
        0xb0 => "areturn",
        0xb1 => "return",
        0xb2 => "getstatic",
        0xb3 => "putstatic",
        0xb4 => "getfield",
        0xb5 => "putfield",
        0xb6 => "invokevirtual",
        0xb7 => "invokespecial",
        0xb8 => "invokestatic",
        0xb9 => "invokeinterface",
        0xba => "invokedynamic",
        0xbb => "new",
        0xbc => "newarray",
        0xbd => "anewarray",
        0xbe => "arraylength",
        0xbf => "athrow",
        0xc0 => "checkcast",
        0xc1 => "instanceof",
        0xc2 => "monitorenter",
        0xc3 => "monitorexit",
        0xc4 => "wide",
        0xc5 => "multianewarray",
        0xc6 => "ifnull",
        0xc7 => "ifnonnull",
        0xc8 => "goto_w",
        0xc9 => "jsr_w",
        0xca => "breakpoint",
        0xfe => "impdep1",
        0xff => "impdep2",
        _ => return None,
    })
}

/// Length in bytes of the fixed part of an instruction after the opcode byte.
///
/// Returns `None` for `tableswitch`, `lookupswitch`, `wide`, and invalid opcodes, whose lengths
/// depend on the bytes that follow.
#[must_use]
pub const fn fixed_operand_len(opcode: u8) -> Option<usize> {
    Some(match opcode {
        0x00..=0x0f => 0,
        0x10 | 0x12 => 1,
        0x11 | 0x13 | 0x14 => 2,
        0x15..=0x19 => 1,
        0x1a..=0x35 => 0,
        0x36..=0x3a => 1,
        0x3b..=0x83 => 0,
        0x84 => 2,
        0x85..=0x98 => 0,
        0x99..=0xa8 => 2,
        0xa9 => 1,
        0xac..=0xb1 => 0,
        0xb2..=0xb8 => 2,
        0xb9 => 4,
        0xba => 4,
        0xbb => 2,
        0xbc => 1,
        0xbd => 2,
        0xbe => 0,
        0xbf => 0,
        0xc0 | 0xc1 => 2,
        0xc2 | 0xc3 => 0,
        0xc5 => 3,
        0xc6 | 0xc7 => 2,
        0xc8 | 0xc9 => 4,
        _ => return None,
    })
}

/// A decoded instruction, borrowing its operand bytes from the method's code array.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodedInstruction<'a> {
    /// Offset of the opcode.
    pub pc: usize,
    /// The opcode byte.
    pub opcode: u8,
    /// The mnemonic, when known.
    pub name: Option<&'static str>,
    /// Operand bytes, including any `wide` prefix bytes.
    pub operands: &'a [u8],
    /// Offset of the next instruction.
    pub next_pc: usize,
}

/// Decode the instruction at `pc`, handling `wide`, `tableswitch`, and `lookupswitch` padding.
///
/// Returns `None` when `pc` is out of range or the instruction is truncated or undefined.
#[must_use]
pub fn decode(code: &[u8], pc: usize) -> Option<DecodedInstruction<'_>> {
    let &opcode = code.get(pc)?;
    if opcode == 0xc4 {
        // wide <opcode> <index> [const], where const is two bytes for iinc.
        let inner = *code.get(pc + 1)?;
        let len = match inner {
            0x84 => 5,
            0x15..=0x19 | 0x36..=0x3a | 0xa9 => 3,
            _ => return None,
        };
        let end = pc.checked_add(1 + len)?;
        let operands = code.get(pc + 1..end)?;
        return Some(DecodedInstruction {
            pc,
            opcode,
            name: name(opcode),
            operands,
            next_pc: end,
        });
    }
    if opcode == 0xaa {
        // tableswitch: pad to a 4-byte boundary relative to the start of the code array.
        let padding = (4 - ((pc + 1) % 4)) % 4;
        let base = pc + 1 + padding;
        let default = read_i32(code, base)?;
        let low = read_i32(code, base + 4)?;
        let high = read_i32(code, base + 8)?;
        if high < low {
            return None;
        }
        let count = usize::try_from(i64::from(high) - i64::from(low) + 1).ok()?;
        let end = base.checked_add(12 + count * 4)?;
        let operands = code.get(pc + 1..end)?;
        let _ = default;
        return Some(DecodedInstruction {
            pc,
            opcode,
            name: name(opcode),
            operands,
            next_pc: end,
        });
    }
    if opcode == 0xab {
        let padding = (4 - ((pc + 1) % 4)) % 4;
        let base = pc + 1 + padding;
        let _default = read_i32(code, base)?;
        let pairs = read_i32(code, base + 4)?;
        if pairs < 0 {
            return None;
        }
        let end = base.checked_add(8 + usize::try_from(pairs).ok()? * 8)?;
        let operands = code.get(pc + 1..end)?;
        return Some(DecodedInstruction {
            pc,
            opcode,
            name: name(opcode),
            operands,
            next_pc: end,
        });
    }
    let extra = fixed_operand_len(opcode)?;
    let end = pc.checked_add(1 + extra)?;
    let operands = code.get(pc + 1..end)?;
    Some(DecodedInstruction {
        pc,
        opcode,
        name: name(opcode),
        operands,
        next_pc: end,
    })
}

fn read_i32(code: &[u8], at: usize) -> Option<i32> {
    let bytes = code.get(at..at + 4)?;
    Some(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}
