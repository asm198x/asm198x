//! Intel 8080 instruction set.
//!
//! The 8080 is the root of the Z80/SM83 lineage: the Z80 is a binary-compatible
//! superset and the SM83 an 8080-derived cousin. Its documented instructions
//! occupy the same opcode slots as the Z80's un-prefixed base, at identical
//! encodings — so this spec is cross-checked against both `asl` (`cpu 8080`) and
//! the shared opcodes in [`crate::z80`]. It is a fresh spec, not a Z80 subset
//! view, because the surface is entirely different: **Intel mnemonics**
//! (`MOV`/`MVI`/`LXI`/`STAX`/…), not Zilog's (`LD`).
//!
//! Fixed-slot, single-byte opcodes (no prefixes), little-endian for the 16-bit
//! immediate/address operands. Mode labels are the operand text with upper-case
//! placeholders `N` (8-bit immediate or port) and `NN` (16-bit immediate or
//! address); registers and pairs are lower-case letters, so they never collide.
//! Jumps and calls are **absolute** (the 8080 has no relative branch), so
//! disassembly is position-independent.
//!
//! **Provenance.** Authored from Intel's *8080 Assembly Language Programming
//! Manual* / *8080 Microcomputer Systems User's Manual* (primary library,
//! `reference/by-topic/cpu-8080/`), every opcode cross-checked byte-for-byte
//! against `asl`. Cycle counts are the manual's values, documentation-grade.

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
const ONE_N: &[Operand] = &[IMM8];
const ONE_NN: &[Operand] = &[ADDR16];

pub const SET: InstructionSet = InstructionSet {
    cpu: "Intel 8080",
    endianness: Endianness::Little,
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

macro_rules! inst {
    ($mnemonic:literal, $summary:literal, [ $($form:expr),* $(,)? ]) => {
        Instruction { mnemonic: $mnemonic, summary: $summary, forms: &[ $($form),* ] }
    };
}

/// A no-operand instruction: one opcode byte.
macro_rules! op0 {
    ($mnemonic:literal, $summary:literal, $op:literal, $flags:literal) => {
        inst!(
            $mnemonic,
            $summary,
            [form(&[$op], "", NONE, Cycles::fixed(4), $flags)]
        )
    };
}

/// A one-immediate instruction (`adi`/`cpi`/`in`/`out`): opcode + N.
macro_rules! op_n {
    ($mnemonic:literal, $summary:literal, $op:literal, $flags:literal) => {
        inst!(
            $mnemonic,
            $summary,
            [form(&[$op], "N", ONE_N, Cycles::fixed(7), $flags)]
        )
    };
}

/// A one-address instruction (`lda`/`jmp`/`call`/conditionals): opcode + NN.
macro_rules! op_nn {
    ($mnemonic:literal, $summary:literal, $op:literal) => {
        inst!(
            $mnemonic,
            $summary,
            [form(&[$op], "NN", ONE_NN, Cycles::fixed(10), "")]
        )
    };
}

/// One ALU register/memory form (`add b`, `add m`): a single register operand
/// and the `SZAPC` flags. `m` (the `(HL)` form) is a cycle slower.
macro_rules! alu {
    ($op:literal, $mode:literal, $cyc:literal) => {
        form(&[$op], $mode, NONE, Cycles::fixed($cyc), "SZAPC")
    };
}

#[rustfmt::skip]
const INSTRUCTIONS: &[Instruction] = &[
    // ===================== control / misc =====================
    op0!("NOP", "No operation", 0x00, ""),
    op0!("HLT", "Halt", 0x76, ""),
    op0!("EI",  "Enable interrupts", 0xFB, ""),
    op0!("DI",  "Disable interrupts", 0xF3, ""),
    op0!("XCHG","Exchange DE and HL", 0xEB, ""),
    op0!("XTHL","Exchange top of stack with HL", 0xE3, ""),
    op0!("SPHL","Move HL to SP", 0xF9, ""),
    op0!("PCHL","Move HL to PC", 0xE9, ""),
    // Accumulator/flag ops.
    op0!("RLC", "Rotate A left", 0x07, "C"),
    op0!("RRC", "Rotate A right", 0x0F, "C"),
    op0!("RAL", "Rotate A left through carry", 0x17, "C"),
    op0!("RAR", "Rotate A right through carry", 0x1F, "C"),
    op0!("DAA", "Decimal adjust A", 0x27, "SZAPC"),
    op0!("CMA", "Complement A", 0x2F, ""),
    op0!("STC", "Set carry", 0x37, "C"),
    op0!("CMC", "Complement carry", 0x3F, "C"),

    // ===================== MOV r, r'  (0x40..0x7F, less 0x76 = HLT) =====================
    inst!("MOV", "Move register", [
        form(&[0x40], "b,b", NONE, Cycles::fixed(5), ""), form(&[0x41], "b,c", NONE, Cycles::fixed(5), ""),
        form(&[0x42], "b,d", NONE, Cycles::fixed(5), ""), form(&[0x43], "b,e", NONE, Cycles::fixed(5), ""),
        form(&[0x44], "b,h", NONE, Cycles::fixed(5), ""), form(&[0x45], "b,l", NONE, Cycles::fixed(5), ""),
        form(&[0x46], "b,m", NONE, Cycles::fixed(7), ""), form(&[0x47], "b,a", NONE, Cycles::fixed(5), ""),
        form(&[0x48], "c,b", NONE, Cycles::fixed(5), ""), form(&[0x49], "c,c", NONE, Cycles::fixed(5), ""),
        form(&[0x4A], "c,d", NONE, Cycles::fixed(5), ""), form(&[0x4B], "c,e", NONE, Cycles::fixed(5), ""),
        form(&[0x4C], "c,h", NONE, Cycles::fixed(5), ""), form(&[0x4D], "c,l", NONE, Cycles::fixed(5), ""),
        form(&[0x4E], "c,m", NONE, Cycles::fixed(7), ""), form(&[0x4F], "c,a", NONE, Cycles::fixed(5), ""),
        form(&[0x50], "d,b", NONE, Cycles::fixed(5), ""), form(&[0x51], "d,c", NONE, Cycles::fixed(5), ""),
        form(&[0x52], "d,d", NONE, Cycles::fixed(5), ""), form(&[0x53], "d,e", NONE, Cycles::fixed(5), ""),
        form(&[0x54], "d,h", NONE, Cycles::fixed(5), ""), form(&[0x55], "d,l", NONE, Cycles::fixed(5), ""),
        form(&[0x56], "d,m", NONE, Cycles::fixed(7), ""), form(&[0x57], "d,a", NONE, Cycles::fixed(5), ""),
        form(&[0x58], "e,b", NONE, Cycles::fixed(5), ""), form(&[0x59], "e,c", NONE, Cycles::fixed(5), ""),
        form(&[0x5A], "e,d", NONE, Cycles::fixed(5), ""), form(&[0x5B], "e,e", NONE, Cycles::fixed(5), ""),
        form(&[0x5C], "e,h", NONE, Cycles::fixed(5), ""), form(&[0x5D], "e,l", NONE, Cycles::fixed(5), ""),
        form(&[0x5E], "e,m", NONE, Cycles::fixed(7), ""), form(&[0x5F], "e,a", NONE, Cycles::fixed(5), ""),
        form(&[0x60], "h,b", NONE, Cycles::fixed(5), ""), form(&[0x61], "h,c", NONE, Cycles::fixed(5), ""),
        form(&[0x62], "h,d", NONE, Cycles::fixed(5), ""), form(&[0x63], "h,e", NONE, Cycles::fixed(5), ""),
        form(&[0x64], "h,h", NONE, Cycles::fixed(5), ""), form(&[0x65], "h,l", NONE, Cycles::fixed(5), ""),
        form(&[0x66], "h,m", NONE, Cycles::fixed(7), ""), form(&[0x67], "h,a", NONE, Cycles::fixed(5), ""),
        form(&[0x68], "l,b", NONE, Cycles::fixed(5), ""), form(&[0x69], "l,c", NONE, Cycles::fixed(5), ""),
        form(&[0x6A], "l,d", NONE, Cycles::fixed(5), ""), form(&[0x6B], "l,e", NONE, Cycles::fixed(5), ""),
        form(&[0x6C], "l,h", NONE, Cycles::fixed(5), ""), form(&[0x6D], "l,l", NONE, Cycles::fixed(5), ""),
        form(&[0x6E], "l,m", NONE, Cycles::fixed(7), ""), form(&[0x6F], "l,a", NONE, Cycles::fixed(5), ""),
        form(&[0x70], "m,b", NONE, Cycles::fixed(7), ""), form(&[0x71], "m,c", NONE, Cycles::fixed(7), ""),
        form(&[0x72], "m,d", NONE, Cycles::fixed(7), ""), form(&[0x73], "m,e", NONE, Cycles::fixed(7), ""),
        form(&[0x74], "m,h", NONE, Cycles::fixed(7), ""), form(&[0x75], "m,l", NONE, Cycles::fixed(7), ""),
        form(&[0x77], "m,a", NONE, Cycles::fixed(7), ""),
        form(&[0x78], "a,b", NONE, Cycles::fixed(5), ""), form(&[0x79], "a,c", NONE, Cycles::fixed(5), ""),
        form(&[0x7A], "a,d", NONE, Cycles::fixed(5), ""), form(&[0x7B], "a,e", NONE, Cycles::fixed(5), ""),
        form(&[0x7C], "a,h", NONE, Cycles::fixed(5), ""), form(&[0x7D], "a,l", NONE, Cycles::fixed(5), ""),
        form(&[0x7E], "a,m", NONE, Cycles::fixed(7), ""), form(&[0x7F], "a,a", NONE, Cycles::fixed(5), ""),
    ]),

    // ===================== MVI r, d8 =====================
    inst!("MVI", "Move immediate", [
        form(&[0x06], "b,N", ONE_N, Cycles::fixed(7), ""),
        form(&[0x0E], "c,N", ONE_N, Cycles::fixed(7), ""),
        form(&[0x16], "d,N", ONE_N, Cycles::fixed(7), ""),
        form(&[0x1E], "e,N", ONE_N, Cycles::fixed(7), ""),
        form(&[0x26], "h,N", ONE_N, Cycles::fixed(7), ""),
        form(&[0x2E], "l,N", ONE_N, Cycles::fixed(7), ""),
        form(&[0x36], "m,N", ONE_N, Cycles::fixed(10), ""),
        form(&[0x3E], "a,N", ONE_N, Cycles::fixed(7), ""),
    ]),

    // ===================== LXI rp, d16 =====================
    inst!("LXI", "Load register pair immediate", [
        form(&[0x01], "b,NN", ONE_NN, Cycles::fixed(10), ""),
        form(&[0x11], "d,NN", ONE_NN, Cycles::fixed(10), ""),
        form(&[0x21], "h,NN", ONE_NN, Cycles::fixed(10), ""),
        form(&[0x31], "sp,NN", ONE_NN, Cycles::fixed(10), ""),
    ]),

    // ===================== accumulator/memory direct =====================
    op_nn!("LDA",  "Load A direct", 0x3A),
    op_nn!("STA",  "Store A direct", 0x32),
    op_nn!("LHLD", "Load HL direct", 0x2A),
    op_nn!("SHLD", "Store HL direct", 0x22),
    inst!("LDAX", "Load A indirect", [
        form(&[0x0A], "b", NONE, Cycles::fixed(7), ""),
        form(&[0x1A], "d", NONE, Cycles::fixed(7), ""),
    ]),
    inst!("STAX", "Store A indirect", [
        form(&[0x02], "b", NONE, Cycles::fixed(7), ""),
        form(&[0x12], "d", NONE, Cycles::fixed(7), ""),
    ]),

    // ===================== 8-bit arithmetic / logic (register) =====================
    inst!("ADD", "Add register", [alu!(0x80,"b",4), alu!(0x81,"c",4), alu!(0x82,"d",4), alu!(0x83,"e",4),
        alu!(0x84,"h",4), alu!(0x85,"l",4), alu!(0x86,"m",7), alu!(0x87,"a",4)]),
    inst!("ADC", "Add register with carry", [alu!(0x88,"b",4), alu!(0x89,"c",4), alu!(0x8A,"d",4), alu!(0x8B,"e",4),
        alu!(0x8C,"h",4), alu!(0x8D,"l",4), alu!(0x8E,"m",7), alu!(0x8F,"a",4)]),
    inst!("SUB", "Subtract register", [alu!(0x90,"b",4), alu!(0x91,"c",4), alu!(0x92,"d",4), alu!(0x93,"e",4),
        alu!(0x94,"h",4), alu!(0x95,"l",4), alu!(0x96,"m",7), alu!(0x97,"a",4)]),
    inst!("SBB", "Subtract register with borrow", [alu!(0x98,"b",4), alu!(0x99,"c",4), alu!(0x9A,"d",4), alu!(0x9B,"e",4),
        alu!(0x9C,"h",4), alu!(0x9D,"l",4), alu!(0x9E,"m",7), alu!(0x9F,"a",4)]),
    inst!("ANA", "AND register", [alu!(0xA0,"b",4), alu!(0xA1,"c",4), alu!(0xA2,"d",4), alu!(0xA3,"e",4),
        alu!(0xA4,"h",4), alu!(0xA5,"l",4), alu!(0xA6,"m",7), alu!(0xA7,"a",4)]),
    inst!("XRA", "XOR register", [alu!(0xA8,"b",4), alu!(0xA9,"c",4), alu!(0xAA,"d",4), alu!(0xAB,"e",4),
        alu!(0xAC,"h",4), alu!(0xAD,"l",4), alu!(0xAE,"m",7), alu!(0xAF,"a",4)]),
    inst!("ORA", "OR register", [alu!(0xB0,"b",4), alu!(0xB1,"c",4), alu!(0xB2,"d",4), alu!(0xB3,"e",4),
        alu!(0xB4,"h",4), alu!(0xB5,"l",4), alu!(0xB6,"m",7), alu!(0xB7,"a",4)]),
    inst!("CMP", "Compare register", [alu!(0xB8,"b",4), alu!(0xB9,"c",4), alu!(0xBA,"d",4), alu!(0xBB,"e",4),
        alu!(0xBC,"h",4), alu!(0xBD,"l",4), alu!(0xBE,"m",7), alu!(0xBF,"a",4)]),

    // ===================== 8-bit arithmetic / logic (immediate) =====================
    op_n!("ADI", "Add immediate", 0xC6, "SZAPC"),
    op_n!("ACI", "Add immediate with carry", 0xCE, "SZAPC"),
    op_n!("SUI", "Subtract immediate", 0xD6, "SZAPC"),
    op_n!("SBI", "Subtract immediate with borrow", 0xDE, "SZAPC"),
    op_n!("ANI", "AND immediate", 0xE6, "SZAPC"),
    op_n!("XRI", "XOR immediate", 0xEE, "SZAPC"),
    op_n!("ORI", "OR immediate", 0xF6, "SZAPC"),
    op_n!("CPI", "Compare immediate", 0xFE, "SZAPC"),

    // ===================== INR / DCR (register) =====================
    inst!("INR", "Increment register", [
        form(&[0x04], "b", NONE, Cycles::fixed(5), "SZAP"), form(&[0x0C], "c", NONE, Cycles::fixed(5), "SZAP"),
        form(&[0x14], "d", NONE, Cycles::fixed(5), "SZAP"), form(&[0x1C], "e", NONE, Cycles::fixed(5), "SZAP"),
        form(&[0x24], "h", NONE, Cycles::fixed(5), "SZAP"), form(&[0x2C], "l", NONE, Cycles::fixed(5), "SZAP"),
        form(&[0x34], "m", NONE, Cycles::fixed(10), "SZAP"), form(&[0x3C], "a", NONE, Cycles::fixed(5), "SZAP"),
    ]),
    inst!("DCR", "Decrement register", [
        form(&[0x05], "b", NONE, Cycles::fixed(5), "SZAP"), form(&[0x0D], "c", NONE, Cycles::fixed(5), "SZAP"),
        form(&[0x15], "d", NONE, Cycles::fixed(5), "SZAP"), form(&[0x1D], "e", NONE, Cycles::fixed(5), "SZAP"),
        form(&[0x25], "h", NONE, Cycles::fixed(5), "SZAP"), form(&[0x2D], "l", NONE, Cycles::fixed(5), "SZAP"),
        form(&[0x35], "m", NONE, Cycles::fixed(10), "SZAP"), form(&[0x3D], "a", NONE, Cycles::fixed(5), "SZAP"),
    ]),

    // ===================== register-pair ops =====================
    inst!("INX", "Increment register pair", [
        form(&[0x03], "b", NONE, Cycles::fixed(5), ""), form(&[0x13], "d", NONE, Cycles::fixed(5), ""),
        form(&[0x23], "h", NONE, Cycles::fixed(5), ""), form(&[0x33], "sp", NONE, Cycles::fixed(5), ""),
    ]),
    inst!("DCX", "Decrement register pair", [
        form(&[0x0B], "b", NONE, Cycles::fixed(5), ""), form(&[0x1B], "d", NONE, Cycles::fixed(5), ""),
        form(&[0x2B], "h", NONE, Cycles::fixed(5), ""), form(&[0x3B], "sp", NONE, Cycles::fixed(5), ""),
    ]),
    inst!("DAD", "Add register pair to HL", [
        form(&[0x09], "b", NONE, Cycles::fixed(10), "C"), form(&[0x19], "d", NONE, Cycles::fixed(10), "C"),
        form(&[0x29], "h", NONE, Cycles::fixed(10), "C"), form(&[0x39], "sp", NONE, Cycles::fixed(10), "C"),
    ]),
    inst!("PUSH", "Push register pair", [
        form(&[0xC5], "b", NONE, Cycles::fixed(11), ""), form(&[0xD5], "d", NONE, Cycles::fixed(11), ""),
        form(&[0xE5], "h", NONE, Cycles::fixed(11), ""), form(&[0xF5], "psw", NONE, Cycles::fixed(11), ""),
    ]),
    inst!("POP", "Pop register pair", [
        form(&[0xC1], "b", NONE, Cycles::fixed(10), ""), form(&[0xD1], "d", NONE, Cycles::fixed(10), ""),
        form(&[0xE1], "h", NONE, Cycles::fixed(10), ""), form(&[0xF1], "psw", NONE, Cycles::fixed(10), "SZAPC"),
    ]),

    // ===================== I/O =====================
    op_n!("IN",  "Input from port", 0xDB, ""),
    op_n!("OUT", "Output to port", 0xD3, ""),

    // ===================== jumps / calls / returns =====================
    op_nn!("JMP", "Jump", 0xC3),
    op_nn!("JNZ", "Jump if not zero", 0xC2),
    op_nn!("JZ",  "Jump if zero", 0xCA),
    op_nn!("JNC", "Jump if no carry", 0xD2),
    op_nn!("JC",  "Jump if carry", 0xDA),
    op_nn!("JPO", "Jump if parity odd", 0xE2),
    op_nn!("JPE", "Jump if parity even", 0xEA),
    op_nn!("JP",  "Jump if positive", 0xF2),
    op_nn!("JM",  "Jump if minus", 0xFA),
    op_nn!("CALL","Call", 0xCD),
    op_nn!("CNZ", "Call if not zero", 0xC4),
    op_nn!("CZ",  "Call if zero", 0xCC),
    op_nn!("CNC", "Call if no carry", 0xD4),
    op_nn!("CC",  "Call if carry", 0xDC),
    op_nn!("CPO", "Call if parity odd", 0xE4),
    op_nn!("CPE", "Call if parity even", 0xEC),
    op_nn!("CP",  "Call if positive", 0xF4),
    op_nn!("CM",  "Call if minus", 0xFC),
    op0!("RET", "Return", 0xC9, ""),
    op0!("RNZ", "Return if not zero", 0xC0, ""),
    op0!("RZ",  "Return if zero", 0xC8, ""),
    op0!("RNC", "Return if no carry", 0xD0, ""),
    op0!("RC",  "Return if carry", 0xD8, ""),
    op0!("RPO", "Return if parity odd", 0xE0, ""),
    op0!("RPE", "Return if parity even", 0xE8, ""),
    op0!("RP",  "Return if positive", 0xF0, ""),
    op0!("RM",  "Return if minus", 0xF8, ""),

    // RST n — the vector number (0..7) is packed into the opcode (C7 + n*8).
    inst!("RST", "Restart", [
        form(&[0xC7], "0", NONE, Cycles::fixed(11), ""), form(&[0xCF], "1", NONE, Cycles::fixed(11), ""),
        form(&[0xD7], "2", NONE, Cycles::fixed(11), ""), form(&[0xDF], "3", NONE, Cycles::fixed(11), ""),
        form(&[0xE7], "4", NONE, Cycles::fixed(11), ""), form(&[0xEF], "5", NONE, Cycles::fixed(11), ""),
        form(&[0xF7], "6", NONE, Cycles::fixed(11), ""), form(&[0xFF], "7", NONE, Cycles::fixed(11), ""),
    ]),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spot_check_opcodes() {
        let op = |m: &str, mode: &str| SET.find_form(m, mode).map(|f| f.opcode);
        assert_eq!(op("NOP", ""), Some(&[0x00][..]));
        assert_eq!(op("MOV", "a,b"), Some(&[0x78][..]));
        assert_eq!(op("MOV", "m,a"), Some(&[0x77][..]));
        assert_eq!(op("MVI", "a,N"), Some(&[0x3E][..]));
        assert_eq!(op("LXI", "h,NN"), Some(&[0x21][..]));
        assert_eq!(op("LXI", "sp,NN"), Some(&[0x31][..]));
        assert_eq!(op("LDA", "NN"), Some(&[0x3A][..]));
        assert_eq!(op("ADD", "b"), Some(&[0x80][..]));
        assert_eq!(op("CMP", "m"), Some(&[0xBE][..]));
        assert_eq!(op("CPI", "N"), Some(&[0xFE][..]));
        assert_eq!(op("PUSH", "psw"), Some(&[0xF5][..]));
        assert_eq!(op("DAD", "sp"), Some(&[0x39][..]));
        assert_eq!(op("JNZ", "NN"), Some(&[0xC2][..]));
        assert_eq!(op("RST", "7"), Some(&[0xFF][..]));
        assert_eq!(op("XCHG", ""), Some(&[0xEB][..]));
    }

    #[test]
    fn hlt_occupies_the_mov_hole() {
        // 0x76 is HLT, not MOV m,m.
        assert!(SET.find_form("MOV", "m,m").is_none());
        assert_eq!(
            SET.find_form("HLT", "").map(|f| f.opcode),
            Some(&[0x76][..])
        );
    }

    #[test]
    fn no_duplicate_opcodes() {
        let mut seen = [false; 256];
        for insn in INSTRUCTIONS {
            for f in insn.forms {
                assert_eq!(f.opcode.len(), 1, "{} is not single-byte", insn.mnemonic);
                let b = f.opcode[0] as usize;
                assert!(!seen[b], "duplicate opcode {b:02X} ({})", insn.mnemonic);
                seen[b] = true;
            }
        }
    }
}
