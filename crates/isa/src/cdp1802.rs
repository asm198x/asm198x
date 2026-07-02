//! RCA CDP1802 (COSMAC) instruction set.
//!
//! The 1802 is an 8-bit CPU with sixteen 16-bit registers and a famously regular
//! opcode map: for the register ops the low nibble is the register number
//! (`INC N` = `0x1N`), and the operation groups sit in the high nibble. Numbers
//! are Intel `H`-suffix hex; the CPU is big-endian for the 16-bit long-branch
//! address.
//!
//! The instruction shapes:
//! - **register** — the register number 0..15 is packed into the opcode's low
//!   nibble, so each of these mnemonics is enumerated as one form per register
//!   (like the 8080 `RST` vector), keyed by the number as its mode label.
//! - **inherent** — a fixed single byte.
//! - **immediate** — a byte follows the opcode (`LDI`/`ANI`/… and the `7x`
//!   immediates).
//! - **short** — a **page-relative** branch: the operand byte replaces the low
//!   byte of the program counter, so the target must be on the current 256-byte
//!   page. The assembler emits the *low byte* of the target — the dialect
//!   supplies `Lo(target)` as a plain one-byte operand, no special engine path.
//! - **long** — a two-byte absolute branch address (big-endian).
//!
//! **Provenance.** Authored from RCA's *User Manual for the CDP1802 COSMAC
//! Microprocessor* (primary library, `reference/by-topic/cpu-cdp1802/`), every
//! opcode cross-checked byte-for-byte against `asl` (`cpu 1802`).

use crate::{Cycles, Endianness, Form, Instruction, InstructionSet, Operand, OperandKind};

const IMM8: Operand = Operand {
    kind: OperandKind::Immediate,
    bytes: 1,
};
const ADDR16: Operand = Operand {
    kind: OperandKind::Address,
    bytes: 2,
};
const NONE: &[Operand] = &[];
const ONE_IMM8: &[Operand] = &[IMM8];
const ONE_ADDR16: &[Operand] = &[ADDR16];

pub const SET: InstructionSet = InstructionSet {
    cpu: "RCA CDP1802 (COSMAC)",
    endianness: Endianness::Big,
    instructions: INSTRUCTIONS,
};

const fn form(
    opcode: &'static [u8],
    mode: &'static str,
    operands: &'static [Operand],
    flags: &'static str,
) -> Form {
    Form {
        opcode,
        mode,
        operands,
        suffix: &[],
        cycles: Cycles::fixed(2),
        flags,
        undocumented: false,
    }
}

/// An inherent (single-byte) instruction.
macro_rules! inh {
    ($mnemonic:literal, $summary:literal, $op:literal, $flags:literal) => {
        Instruction {
            mnemonic: $mnemonic,
            summary: $summary,
            forms: &[form(&[$op], "inherent", NONE, $flags)],
        }
    };
}

/// An immediate instruction: opcode + one byte.
macro_rules! imm {
    ($mnemonic:literal, $summary:literal, $op:literal, $flags:literal) => {
        Instruction {
            mnemonic: $mnemonic,
            summary: $summary,
            forms: &[form(&[$op], "immediate", ONE_IMM8, $flags)],
        }
    };
}

/// A short (page-relative) branch: opcode + the low byte of the target.
macro_rules! sbr {
    ($mnemonic:literal, $summary:literal, $op:literal) => {
        Instruction {
            mnemonic: $mnemonic,
            summary: $summary,
            forms: &[form(&[$op], "short", ONE_IMM8, "")],
        }
    };
}

/// A long branch: opcode + a 2-byte absolute (big-endian) address.
macro_rules! lbr {
    ($mnemonic:literal, $summary:literal, $op:literal) => {
        Instruction {
            mnemonic: $mnemonic,
            summary: $summary,
            forms: &[form(&[$op], "long", ONE_ADDR16, "")],
        }
    };
}

/// A register op whose low nibble is the register number 0..15. One form per
/// register, keyed by the decimal number as its mode label.
macro_rules! reg16 {
    ($mnemonic:literal, $summary:literal, $base:literal, $flags:literal) => {
        Instruction {
            mnemonic: $mnemonic,
            summary: $summary,
            forms: &[
                form(&[$base + 0x0], "0", NONE, $flags),
                form(&[$base + 0x1], "1", NONE, $flags),
                form(&[$base + 0x2], "2", NONE, $flags),
                form(&[$base + 0x3], "3", NONE, $flags),
                form(&[$base + 0x4], "4", NONE, $flags),
                form(&[$base + 0x5], "5", NONE, $flags),
                form(&[$base + 0x6], "6", NONE, $flags),
                form(&[$base + 0x7], "7", NONE, $flags),
                form(&[$base + 0x8], "8", NONE, $flags),
                form(&[$base + 0x9], "9", NONE, $flags),
                form(&[$base + 0xA], "10", NONE, $flags),
                form(&[$base + 0xB], "11", NONE, $flags),
                form(&[$base + 0xC], "12", NONE, $flags),
                form(&[$base + 0xD], "13", NONE, $flags),
                form(&[$base + 0xE], "14", NONE, $flags),
                form(&[$base + 0xF], "15", NONE, $flags),
            ],
        }
    };
}

#[rustfmt::skip]
const INSTRUCTIONS: &[Instruction] = &[
    // ===================== register ops (low nibble = register 0..15) =====================
    // LDN is 0x01..0x0F — 0x00 is IDL, so it has no register-0 form.
    Instruction { mnemonic: "LDN", summary: "Load via N", forms: &[
        form(&[0x01], "1", NONE, ""), form(&[0x02], "2", NONE, ""), form(&[0x03], "3", NONE, ""),
        form(&[0x04], "4", NONE, ""), form(&[0x05], "5", NONE, ""), form(&[0x06], "6", NONE, ""),
        form(&[0x07], "7", NONE, ""), form(&[0x08], "8", NONE, ""), form(&[0x09], "9", NONE, ""),
        form(&[0x0A], "10", NONE, ""), form(&[0x0B], "11", NONE, ""), form(&[0x0C], "12", NONE, ""),
        form(&[0x0D], "13", NONE, ""), form(&[0x0E], "14", NONE, ""), form(&[0x0F], "15", NONE, ""),
    ] },
    reg16!("INC", "Increment register", 0x10, ""),
    reg16!("DEC", "Decrement register", 0x20, ""),
    reg16!("LDA", "Load advance", 0x40, ""),
    reg16!("STR", "Store via N", 0x50, ""),
    reg16!("GLO", "Get low byte", 0x80, ""),
    reg16!("GHI", "Get high byte", 0x90, ""),
    reg16!("PLO", "Put low byte", 0xA0, ""),
    reg16!("PHI", "Put high byte", 0xB0, ""),
    reg16!("SEP", "Set program register", 0xD0, ""),
    reg16!("SEX", "Set data register", 0xE0, ""),

    // OUT N / INP N — the low nibble is a 1..7 I/O port select.
    Instruction { mnemonic: "OUT", summary: "Output", forms: &[
        form(&[0x61], "1", NONE, ""), form(&[0x62], "2", NONE, ""), form(&[0x63], "3", NONE, ""),
        form(&[0x64], "4", NONE, ""), form(&[0x65], "5", NONE, ""), form(&[0x66], "6", NONE, ""),
        form(&[0x67], "7", NONE, ""),
    ] },
    Instruction { mnemonic: "INP", summary: "Input", forms: &[
        form(&[0x69], "1", NONE, ""), form(&[0x6A], "2", NONE, ""), form(&[0x6B], "3", NONE, ""),
        form(&[0x6C], "4", NONE, ""), form(&[0x6D], "5", NONE, ""), form(&[0x6E], "6", NONE, ""),
        form(&[0x6F], "7", NONE, ""),
    ] },

    // ===================== inherent =====================
    inh!("IDL", "Idle", 0x00, ""),
    inh!("IRX", "Increment R(X)", 0x60, ""),
    inh!("RET", "Return", 0x70, ""),
    inh!("DIS", "Disable interrupts", 0x71, ""),
    inh!("LDXA", "Load via X and advance", 0x72, ""),
    inh!("STXD", "Store via X and decrement", 0x73, ""),
    inh!("ADC", "Add with carry", 0x74, "DF"),
    inh!("SDB", "Subtract D with borrow", 0x75, "DF"),
    inh!("SHRC", "Shift right with carry", 0x76, "DF"),
    inh!("SMB", "Subtract memory with borrow", 0x77, "DF"),
    inh!("SAV", "Save T", 0x78, ""),
    inh!("MARK", "Push X,P to stack", 0x79, ""),
    inh!("REQ", "Reset Q", 0x7A, "Q"),
    inh!("SEQ", "Set Q", 0x7B, "Q"),
    inh!("SHLC", "Shift left with carry", 0x7E, "DF"),
    inh!("LDX", "Load via X", 0xF0, ""),
    inh!("OR", "Logical OR", 0xF1, ""),
    inh!("AND", "Logical AND", 0xF2, ""),
    inh!("XOR", "Exclusive OR", 0xF3, ""),
    inh!("ADD", "Add", 0xF4, "DF"),
    inh!("SD", "Subtract D", 0xF5, "DF"),
    inh!("SHR", "Shift right", 0xF6, "DF"),
    inh!("SM", "Subtract memory", 0xF7, "DF"),
    inh!("SHL", "Shift left", 0xFE, "DF"),
    // C-page inherent skips.
    inh!("NOP", "No operation", 0xC4, ""),
    inh!("LSNQ", "Long skip if Q = 0", 0xC5, ""),
    inh!("LSNZ", "Long skip if D not zero", 0xC6, ""),
    inh!("LSNF", "Long skip if DF = 0", 0xC7, ""),
    inh!("LSKP", "Long skip", 0xC8, ""),
    inh!("LSIE", "Long skip if interrupts enabled", 0xCC, ""),
    inh!("LSQ", "Long skip if Q = 1", 0xCD, ""),
    inh!("LSZ", "Long skip if D zero", 0xCE, ""),
    inh!("LSDF", "Long skip if DF = 1", 0xCF, ""),

    // ===================== immediate =====================
    imm!("ADCI", "Add with carry immediate", 0x7C, "DF"),
    imm!("SDBI", "Subtract D with borrow immediate", 0x7D, "DF"),
    imm!("SMBI", "Subtract memory with borrow immediate", 0x7F, "DF"),
    imm!("LDI", "Load immediate", 0xF8, ""),
    imm!("ORI", "OR immediate", 0xF9, ""),
    imm!("ANI", "AND immediate", 0xFA, ""),
    imm!("XRI", "Exclusive-OR immediate", 0xFB, ""),
    imm!("ADI", "Add immediate", 0xFC, "DF"),
    imm!("SDI", "Subtract D immediate", 0xFD, "DF"),
    imm!("SMI", "Subtract memory immediate", 0xFF, "DF"),

    // ===================== short (page-relative) branches =====================
    sbr!("BR", "Branch", 0x30),
    sbr!("BQ", "Branch if Q = 1", 0x31),
    sbr!("BZ", "Branch if D = 0", 0x32),
    sbr!("BDF", "Branch if DF = 1", 0x33),
    sbr!("B1", "Branch if EF1", 0x34),
    sbr!("B2", "Branch if EF2", 0x35),
    sbr!("B3", "Branch if EF3", 0x36),
    sbr!("B4", "Branch if EF4", 0x37),
    sbr!("NBR", "No short branch (skip)", 0x38),
    sbr!("BNQ", "Branch if Q = 0", 0x39),
    sbr!("BNZ", "Branch if D not zero", 0x3A),
    sbr!("BNF", "Branch if DF = 0", 0x3B),
    sbr!("BN1", "Branch if not EF1", 0x3C),
    sbr!("BN2", "Branch if not EF2", 0x3D),
    sbr!("BN3", "Branch if not EF3", 0x3E),
    sbr!("BN4", "Branch if not EF4", 0x3F),

    // ===================== long branches =====================
    lbr!("LBR", "Long branch", 0xC0),
    lbr!("LBQ", "Long branch if Q = 1", 0xC1),
    lbr!("LBZ", "Long branch if D = 0", 0xC2),
    lbr!("LBDF", "Long branch if DF = 1", 0xC3),
    lbr!("LBNQ", "Long branch if Q = 0", 0xC9),
    lbr!("LBNZ", "Long branch if D not zero", 0xCA),
    lbr!("LBNF", "Long branch if DF = 0", 0xCB),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spot_check_opcodes() {
        let op = |m: &str, mode: &str| SET.find_form(m, mode).map(|f| f.opcode);
        assert_eq!(op("INC", "3"), Some(&[0x13][..]));
        assert_eq!(op("INC", "10"), Some(&[0x1A][..]));
        assert_eq!(op("LDN", "7"), Some(&[0x07][..]));
        assert_eq!(op("GLO", "5"), Some(&[0x85][..]));
        assert_eq!(op("SEP", "15"), Some(&[0xDF][..]));
        assert_eq!(op("OUT", "4"), Some(&[0x64][..]));
        assert_eq!(op("INP", "4"), Some(&[0x6C][..]));
        assert_eq!(op("LDI", "immediate"), Some(&[0xF8][..]));
        assert_eq!(op("ADCI", "immediate"), Some(&[0x7C][..]));
        assert_eq!(op("BR", "short"), Some(&[0x30][..]));
        assert_eq!(op("BNF", "short"), Some(&[0x3B][..]));
        assert_eq!(op("LBR", "long"), Some(&[0xC0][..]));
        assert_eq!(op("LBNZ", "long"), Some(&[0xCA][..]));
        assert_eq!(op("NOP", "inherent"), Some(&[0xC4][..]));
        assert_eq!(op("IDL", "inherent"), Some(&[0x00][..]));
    }

    #[test]
    fn form_lengths() {
        assert_eq!(SET.find_form("INC", "3").map(Form::len), Some(1)); // register in opcode
        assert_eq!(SET.find_form("LDI", "immediate").map(Form::len), Some(2));
        assert_eq!(SET.find_form("BR", "short").map(Form::len), Some(2));
        assert_eq!(SET.find_form("LBR", "long").map(Form::len), Some(3));
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
