//! Motorola 6800 instruction set.
//!
//! The 6800 is the 6809's ancestor and a Motorola-family sibling of the 6502:
//! **big-endian**, `$`-prefixed hex, and a regular opcode map. Addressing modes
//! sit in a byte's low nibble bits — for the accumulator ops, immediate `+0x00`,
//! direct `+0x10`, indexed `+0x20`, extended `+0x30`, with the B-accumulator
//! variant `+0x40` above the A form. It is fixed-slot: single-byte opcodes plus
//! the operand bytes, all laid down big-endian.
//!
//! Addressing modes: **inherent** (no operand), **immediate** (`#$nn`, 8- or
//! 16-bit), **direct** (an 8-bit address), **extended** (a 16-bit address),
//! **indexed** (`$nn,X` — an 8-bit unsigned offset from X), and **relative**
//! (8-bit signed branch). Direct-vs-extended is chosen by operand size in the
//! dialect, exactly as the 6502 chooses zero-page vs absolute.
//!
//! **Provenance.** Authored from Motorola's *M6800 Microprocessor Programming
//! Manual* (primary library, `reference/by-topic/cpu-6800/`), every opcode
//! cross-checked byte-for-byte against `asl` (`cpu 6800`). Cycle counts are the
//! manual's values, documentation-grade.

use crate::{Cycles, Endianness, Form, Instruction, InstructionSet, Operand, OperandKind};

const IMM8: Operand = Operand {
    kind: OperandKind::Immediate,
    bytes: 1,
};
const IMM16: Operand = Operand {
    kind: OperandKind::Immediate,
    bytes: 2,
};
/// An 8-bit direct-page address.
const DIR: Operand = Operand {
    kind: OperandKind::Address,
    bytes: 1,
};
/// A 16-bit extended address.
const EXT: Operand = Operand {
    kind: OperandKind::Address,
    bytes: 2,
};
/// An 8-bit unsigned offset from the X register.
const IDX: Operand = Operand {
    kind: OperandKind::Immediate,
    bytes: 1,
};
/// An 8-bit signed PC-relative branch target.
const REL: Operand = Operand {
    kind: OperandKind::RelativePc,
    bytes: 1,
};

const NONE: &[Operand] = &[];
const ONE_IMM8: &[Operand] = &[IMM8];
const ONE_IMM16: &[Operand] = &[IMM16];
const ONE_DIR: &[Operand] = &[DIR];
const ONE_EXT: &[Operand] = &[EXT];
const ONE_IDX: &[Operand] = &[IDX];
const ONE_REL: &[Operand] = &[REL];

pub const SET: InstructionSet = InstructionSet {
    cpu: "Motorola 6800",
    endianness: Endianness::Big,
    instructions: INSTRUCTIONS,
};

const fn form(
    opcode: &'static [u8],
    mode: &'static str,
    operands: &'static [Operand],
    cycles: Cycles,
    flags: &'static str,
) -> Form {
    Form {
        opcode,
        mode,
        operands,
        suffix: &[],
        cycles,
        flags,
        undocumented: false,
    }
}

/// An inherent (no-operand) instruction.
macro_rules! inh {
    ($mnemonic:literal, $summary:literal, $op:literal, $flags:literal) => {
        Instruction {
            mnemonic: $mnemonic,
            summary: $summary,
            forms: &[form(&[$op], "inherent", NONE, Cycles::fixed(2), $flags)],
        }
    };
}

/// A relative (8-bit signed branch) instruction.
macro_rules! rel {
    ($mnemonic:literal, $summary:literal, $op:literal) => {
        Instruction {
            mnemonic: $mnemonic,
            summary: $summary,
            forms: &[form(&[$op], "relative", ONE_REL, Cycles::fixed(4), "")],
        }
    };
}

/// A dual-mode accumulator op with an **8-bit** immediate: immediate (`$imm`),
/// direct (`+0x10`), indexed (`+0x20`), extended (`+0x30`).
macro_rules! acc4 {
    ($mnemonic:literal, $summary:literal, $imm:literal, $flags:literal) => {
        Instruction {
            mnemonic: $mnemonic,
            summary: $summary,
            forms: &[
                form(&[$imm], "immediate", ONE_IMM8, Cycles::fixed(2), $flags),
                form(&[$imm + 0x10], "direct", ONE_DIR, Cycles::fixed(3), $flags),
                form(&[$imm + 0x20], "indexed", ONE_IDX, Cycles::fixed(5), $flags),
                form(
                    &[$imm + 0x30],
                    "extended",
                    ONE_EXT,
                    Cycles::fixed(4),
                    $flags,
                ),
            ],
        }
    };
}

/// A dual-mode op with a **16-bit** immediate (`ldx`/`lds`/`cpx`).
macro_rules! acc4w {
    ($mnemonic:literal, $summary:literal, $imm:literal, $flags:literal) => {
        Instruction {
            mnemonic: $mnemonic,
            summary: $summary,
            forms: &[
                form(&[$imm], "immediate", ONE_IMM16, Cycles::fixed(3), $flags),
                form(&[$imm + 0x10], "direct", ONE_DIR, Cycles::fixed(4), $flags),
                form(&[$imm + 0x20], "indexed", ONE_IDX, Cycles::fixed(6), $flags),
                form(
                    &[$imm + 0x30],
                    "extended",
                    ONE_EXT,
                    Cycles::fixed(5),
                    $flags,
                ),
            ],
        }
    };
}

/// A store-style op: direct (`$dir`), indexed (`+0x10`), extended (`+0x20`) —
/// no immediate (storing an immediate is meaningless). Covers `staa`/`stab`
/// and the 16-bit `stx`/`sts` (the operand is still a 1-byte direct address).
macro_rules! acc3 {
    ($mnemonic:literal, $summary:literal, $dir:literal, $flags:literal) => {
        Instruction {
            mnemonic: $mnemonic,
            summary: $summary,
            forms: &[
                form(&[$dir], "direct", ONE_DIR, Cycles::fixed(4), $flags),
                form(&[$dir + 0x10], "indexed", ONE_IDX, Cycles::fixed(6), $flags),
                form(
                    &[$dir + 0x20],
                    "extended",
                    ONE_EXT,
                    Cycles::fixed(5),
                    $flags,
                ),
            ],
        }
    };
}

/// A single-operand memory op: indexed (`$idx`), extended (`+0x10`). Covers
/// `neg`/`com`/…/`clr` and `jmp`/`jsr` (their accumulator forms are separate
/// inherent mnemonics — `nega`, `clrb`, …).
macro_rules! mem2 {
    ($mnemonic:literal, $summary:literal, $idx:literal, $flags:literal) => {
        Instruction {
            mnemonic: $mnemonic,
            summary: $summary,
            forms: &[
                form(&[$idx], "indexed", ONE_IDX, Cycles::fixed(6), $flags),
                form(
                    &[$idx + 0x10],
                    "extended",
                    ONE_EXT,
                    Cycles::fixed(6),
                    $flags,
                ),
            ],
        }
    };
}

#[rustfmt::skip]
const INSTRUCTIONS: &[Instruction] = &[
    // ===================== dual-accumulator ALU (A imm base, B = +0x40) =====================
    acc4!("SUBA", "Subtract from A", 0x80, "NZVC"), acc4!("SUBB", "Subtract from B", 0xC0, "NZVC"),
    acc4!("CMPA", "Compare A", 0x81, "NZVC"),       acc4!("CMPB", "Compare B", 0xC1, "NZVC"),
    acc4!("SBCA", "Subtract with carry from A", 0x82, "NZVC"), acc4!("SBCB", "Subtract with carry from B", 0xC2, "NZVC"),
    acc4!("ANDA", "AND A", 0x84, "NZV"),            acc4!("ANDB", "AND B", 0xC4, "NZV"),
    acc4!("BITA", "Bit test A", 0x85, "NZV"),       acc4!("BITB", "Bit test B", 0xC5, "NZV"),
    acc4!("LDAA", "Load A", 0x86, "NZV"),           acc4!("LDAB", "Load B", 0xC6, "NZV"),
    acc4!("EORA", "Exclusive-OR A", 0x88, "NZV"),   acc4!("EORB", "Exclusive-OR B", 0xC8, "NZV"),
    acc4!("ADCA", "Add with carry to A", 0x89, "NZVC"), acc4!("ADCB", "Add with carry to B", 0xC9, "NZVC"),
    acc4!("ORAA", "OR A", 0x8A, "NZV"),             acc4!("ORAB", "OR B", 0xCA, "NZV"),
    acc4!("ADDA", "Add to A", 0x8B, "NZVC"),        acc4!("ADDB", "Add to B", 0xCB, "NZVC"),

    // Stores (no immediate).
    acc3!("STAA", "Store A", 0x97, "NZV"), acc3!("STAB", "Store B", 0xD7, "NZV"),

    // ===================== 16-bit index/stack ops =====================
    acc4w!("CPX", "Compare index register", 0x8C, "NZV"),
    acc4w!("LDS", "Load stack pointer", 0x8E, "NZV"),
    acc4w!("LDX", "Load index register", 0xCE, "NZV"),
    acc3!("STS", "Store stack pointer", 0x9F, "NZV"),
    acc3!("STX", "Store index register", 0xDF, "NZV"),

    // ===================== single-operand memory ops (indexed/extended) =====================
    mem2!("NEG", "Negate", 0x60, "NZVC"),
    mem2!("COM", "Complement", 0x63, "NZVC"),
    mem2!("LSR", "Logical shift right", 0x64, "NZVC"),
    mem2!("ROR", "Rotate right", 0x66, "NZVC"),
    mem2!("ASR", "Arithmetic shift right", 0x67, "NZVC"),
    mem2!("ASL", "Arithmetic shift left", 0x68, "NZVC"),
    mem2!("ROL", "Rotate left", 0x69, "NZVC"),
    mem2!("DEC", "Decrement", 0x6A, "NZV"),
    mem2!("INC", "Increment", 0x6C, "NZV"),
    mem2!("TST", "Test", 0x6D, "NZV"),
    mem2!("JMP", "Jump", 0x6E, ""),
    mem2!("CLR", "Clear", 0x6F, "NZVC"),
    mem2!("JSR", "Jump to subroutine", 0xAD, ""),

    // ===================== relative branches =====================
    rel!("BRA", "Branch always", 0x20),
    rel!("BHI", "Branch if higher", 0x22),
    rel!("BLS", "Branch if lower or same", 0x23),
    rel!("BCC", "Branch if carry clear", 0x24),
    rel!("BCS", "Branch if carry set", 0x25),
    rel!("BNE", "Branch if not equal", 0x26),
    rel!("BEQ", "Branch if equal", 0x27),
    rel!("BVC", "Branch if overflow clear", 0x28),
    rel!("BVS", "Branch if overflow set", 0x29),
    rel!("BPL", "Branch if plus", 0x2A),
    rel!("BMI", "Branch if minus", 0x2B),
    rel!("BGE", "Branch if greater or equal", 0x2C),
    rel!("BLT", "Branch if less than", 0x2D),
    rel!("BGT", "Branch if greater than", 0x2E),
    rel!("BLE", "Branch if less or equal", 0x2F),
    rel!("BSR", "Branch to subroutine", 0x8D),

    // ===================== inherent =====================
    inh!("NOP", "No operation", 0x01, ""),
    inh!("TAP", "Transfer A to CC", 0x06, "HINZVC"),
    inh!("TPA", "Transfer CC to A", 0x07, ""),
    inh!("INX", "Increment X", 0x08, "Z"),
    inh!("DEX", "Decrement X", 0x09, "Z"),
    inh!("CLV", "Clear overflow", 0x0A, "V"),
    inh!("SEV", "Set overflow", 0x0B, "V"),
    inh!("CLC", "Clear carry", 0x0C, "C"),
    inh!("SEC", "Set carry", 0x0D, "C"),
    inh!("CLI", "Clear interrupt mask", 0x0E, "I"),
    inh!("SEI", "Set interrupt mask", 0x0F, "I"),
    inh!("SBA", "Subtract B from A", 0x10, "NZVC"),
    inh!("CBA", "Compare A with B", 0x11, "NZVC"),
    inh!("TAB", "Transfer A to B", 0x16, "NZV"),
    inh!("TBA", "Transfer B to A", 0x17, "NZV"),
    inh!("DAA", "Decimal adjust A", 0x19, "NZVC"),
    inh!("ABA", "Add B to A", 0x1B, "HNZVC"),
    inh!("TSX", "Transfer SP to X", 0x30, ""),
    inh!("INS", "Increment SP", 0x31, ""),
    inh!("PULA", "Pull A", 0x32, ""),
    inh!("PULB", "Pull B", 0x33, ""),
    inh!("DES", "Decrement SP", 0x34, ""),
    inh!("TXS", "Transfer X to SP", 0x35, ""),
    inh!("PSHA", "Push A", 0x36, ""),
    inh!("PSHB", "Push B", 0x37, ""),
    inh!("RTS", "Return from subroutine", 0x39, ""),
    inh!("RTI", "Return from interrupt", 0x3B, "HINZVC"),
    inh!("WAI", "Wait for interrupt", 0x3E, ""),
    inh!("SWI", "Software interrupt", 0x3F, ""),
    // Accumulator single-operand forms (A = 0x4x, B = 0x5x).
    inh!("NEGA", "Negate A", 0x40, "NZVC"), inh!("NEGB", "Negate B", 0x50, "NZVC"),
    inh!("COMA", "Complement A", 0x43, "NZVC"), inh!("COMB", "Complement B", 0x53, "NZVC"),
    inh!("LSRA", "Logical shift right A", 0x44, "NZVC"), inh!("LSRB", "Logical shift right B", 0x54, "NZVC"),
    inh!("RORA", "Rotate right A", 0x46, "NZVC"), inh!("RORB", "Rotate right B", 0x56, "NZVC"),
    inh!("ASRA", "Arithmetic shift right A", 0x47, "NZVC"), inh!("ASRB", "Arithmetic shift right B", 0x57, "NZVC"),
    inh!("ASLA", "Arithmetic shift left A", 0x48, "NZVC"), inh!("ASLB", "Arithmetic shift left B", 0x58, "NZVC"),
    inh!("ROLA", "Rotate left A", 0x49, "NZVC"), inh!("ROLB", "Rotate left B", 0x59, "NZVC"),
    inh!("DECA", "Decrement A", 0x4A, "NZV"), inh!("DECB", "Decrement B", 0x5A, "NZV"),
    inh!("INCA", "Increment A", 0x4C, "NZV"), inh!("INCB", "Increment B", 0x5C, "NZV"),
    inh!("TSTA", "Test A", 0x4D, "NZV"), inh!("TSTB", "Test B", 0x5D, "NZV"),
    inh!("CLRA", "Clear A", 0x4F, "NZVC"), inh!("CLRB", "Clear B", 0x5F, "NZVC"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spot_check_opcodes() {
        let op = |m: &str, mode: &str| SET.find_form(m, mode).map(|f| f.opcode);
        assert_eq!(op("LDAA", "immediate"), Some(&[0x86][..]));
        assert_eq!(op("LDAA", "direct"), Some(&[0x96][..]));
        assert_eq!(op("LDAA", "indexed"), Some(&[0xA6][..]));
        assert_eq!(op("LDAA", "extended"), Some(&[0xB6][..]));
        assert_eq!(op("LDAB", "immediate"), Some(&[0xC6][..]));
        assert_eq!(op("STAA", "extended"), Some(&[0xB7][..]));
        assert_eq!(op("LDX", "immediate"), Some(&[0xCE][..]));
        assert_eq!(op("LDX", "extended"), Some(&[0xFE][..]));
        assert_eq!(op("CPX", "immediate"), Some(&[0x8C][..]));
        assert_eq!(op("STX", "extended"), Some(&[0xFF][..]));
        assert_eq!(op("JMP", "extended"), Some(&[0x7E][..]));
        assert_eq!(op("JSR", "indexed"), Some(&[0xAD][..]));
        assert_eq!(op("BRA", "relative"), Some(&[0x20][..]));
        assert_eq!(op("NEGA", "inherent"), Some(&[0x40][..]));
        assert_eq!(op("CLRB", "inherent"), Some(&[0x5F][..]));
        assert_eq!(op("ABA", "inherent"), Some(&[0x1B][..]));
    }

    #[test]
    fn immediate_width_matches_the_op() {
        // 8-bit accumulator immediate vs 16-bit index immediate.
        assert_eq!(SET.find_form("LDAA", "immediate").map(Form::len), Some(2));
        assert_eq!(SET.find_form("LDX", "immediate").map(Form::len), Some(3));
    }

    #[test]
    fn no_duplicate_opcodes() {
        let mut seen = [false; 256];
        for insn in INSTRUCTIONS {
            for f in insn.forms {
                assert_eq!(f.opcode.len(), 1, "{} not single-byte", insn.mnemonic);
                let b = f.opcode[0] as usize;
                assert!(!seen[b], "duplicate opcode {b:02X} ({})", insn.mnemonic);
                seen[b] = true;
            }
        }
    }
}
