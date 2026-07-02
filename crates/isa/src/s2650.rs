//! Signetics 2650 instruction set.
//!
//! The 2650 (1975) is an 8-bit CPU with a distinctive architecture: seven
//! registers (`R0` plus two banks of `R1`–`R3`, selected by the `RS` bit), a
//! 15-bit address space (32K), and a two-byte program status word (`PSU`/`PSL`).
//! It powered the Interton VC 4000, Emerson Arcadia 2001, and Signetics
//! Instructor 50.
//!
//! Its instruction set is exceptionally regular: the low two bits of the opcode
//! select a register (`R0`–`R3`) or, for the branches, a condition code
//! (`EQ`/`GT`/`LT`/`UN` = 0/1/2/3). Most operations come in four **addressing
//! modes**, distinguished by the mnemonic suffix:
//!
//! - **Z** — register: the operand *is* `R0` and the register field names the
//!   other operand (`LODZ r1`, opcode only). Mode label `"r0"`..`"r3"`.
//! - **I** — immediate: opcode + one data byte (`LODI,r0 $42`).
//! - **R** — relative: opcode + a **7-bit signed** displacement (`−64..+63`)
//!   measured from the following instruction, with bit 7 the **indirect** flag
//!   (`*`). Emitted via the computed-operand seam.
//! - **A** — absolute: opcode + a **15-bit** address (big-endian), bit 15 the
//!   indirect flag, and bits 14-13 an **index control** (`,R3` / `,R3,+` /
//!   `,R3,-`) — indexing forces the register field to R3 and the operand to R0.
//!   Emitted via the seam.
//!
//! The register (`Z`) forms have documented quirks the dialect handles: `LODZ,R0`
//! is redundant and `asl` encodes it as `IORZ,R0` (`0x60`); `STRZ,R0` (`0xC0`)
//! and `ANDZ,R0` (`0x40`) are the `NOP` and `HALT` slots and are illegal — so
//! those three `Z` forms enumerate `r1`–`r3` only. The branch-on-false
//! unconditional opcodes are repurposed (they would never branch): `BXA`/`BSXA`
//! are plain aliases of `BCFA,UN`/`BSFA,UN` (identical absolute encoding), while
//! `ZBRR`/`ZBSR` share the `BCFR,UN`/`BSFR,UN` opcodes but use *page-zero*
//! relative addressing — all handled in the dialect, not as separate opcodes.
//!
//! **Provenance.** Authored from Signetics' *2650 Microprocessor Manual* (1975,
//! primary library, `reference/by-topic/cpu-2650/`, Table 2 the opcode chart),
//! every opcode cross-checked byte-for-byte against `asl` (`cpu 2650`). Cycle
//! counts are the manual's; flags are documentation-grade.

use crate::{Cycles, Endianness, Form, Instruction, InstructionSet, Operand, OperandKind};

const IMM: Operand = Operand {
    kind: OperandKind::Immediate,
    bytes: 1,
};
const REL: Operand = Operand {
    kind: OperandKind::RelativePc,
    bytes: 1,
};
const ADDR: Operand = Operand {
    kind: OperandKind::Address,
    bytes: 2,
};
const NONE: &[Operand] = &[];
const ONE_IMM: &[Operand] = &[IMM];
const ONE_REL: &[Operand] = &[REL];
const ONE_ADDR: &[Operand] = &[ADDR];

pub const SET: InstructionSet = InstructionSet {
    cpu: "Signetics 2650",
    endianness: Endianness::Big,
    instructions: INSTRUCTIONS,
};

const fn f(
    opcode: &'static [u8],
    mode: &'static str,
    operands: &'static [Operand],
    cycles: u8,
) -> Form {
    Form {
        opcode,
        mode,
        operands,
        suffix: &[],
        cycles: Cycles::fixed(cycles),
        flags: "",
        undocumented: false,
    }
}

/// Four register forms `r0`..`r3` at `base|r`.
macro_rules! reg4 {
    ($mn:literal, $sum:literal, $base:literal, $ops:expr, $cy:literal) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[
                f(&[$base + 0], "r0", $ops, $cy),
                f(&[$base + 1], "r1", $ops, $cy),
                f(&[$base + 2], "r2", $ops, $cy),
                f(&[$base + 3], "r3", $ops, $cy),
            ],
        }
    };
}

/// Three register forms `r1`..`r3` (the `Z` forms whose `r0` slot is special).
macro_rules! reg3 {
    ($mn:literal, $sum:literal, $base:literal, $ops:expr, $cy:literal) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[
                f(&[$base + 1], "r1", $ops, $cy),
                f(&[$base + 2], "r2", $ops, $cy),
                f(&[$base + 3], "r3", $ops, $cy),
            ],
        }
    };
}

/// Four condition-code forms `eq`/`gt`/`lt`/`un` at `base|cc` (0/1/2/3).
macro_rules! cc4 {
    ($mn:literal, $sum:literal, $base:literal, $ops:expr, $cy:literal) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[
                f(&[$base + 0], "eq", $ops, $cy),
                f(&[$base + 1], "gt", $ops, $cy),
                f(&[$base + 2], "lt", $ops, $cy),
                f(&[$base + 3], "un", $ops, $cy),
            ],
        }
    };
}

/// A single fixed opcode with no register/condition suffix.
macro_rules! one {
    ($mn:literal, $sum:literal, $op:literal, $ops:expr, $cy:literal) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[f(&[$op], "", $ops, $cy)],
        }
    };
}

#[rustfmt::skip]
const INSTRUCTIONS: &[Instruction] = &[
    // ===================== load / store =====================
    reg3!("LODZ", "Load register zero", 0x00, NONE, 2),
    reg4!("LODI", "Load immediate", 0x04, ONE_IMM, 2),
    reg4!("LODR", "Load relative", 0x08, ONE_REL, 3),
    reg4!("LODA", "Load absolute", 0x0C, ONE_ADDR, 4),
    reg3!("STRZ", "Store register zero", 0xC0, NONE, 2),
    reg4!("STRR", "Store relative", 0xC8, ONE_REL, 3),
    reg4!("STRA", "Store absolute", 0xCC, ONE_ADDR, 4),

    // ===================== arithmetic =====================
    reg4!("ADDZ", "Add to register zero", 0x80, NONE, 2),
    reg4!("ADDI", "Add immediate", 0x84, ONE_IMM, 2),
    reg4!("ADDR", "Add relative", 0x88, ONE_REL, 3),
    reg4!("ADDA", "Add absolute", 0x8C, ONE_ADDR, 4),
    reg4!("SUBZ", "Subtract from register zero", 0xA0, NONE, 2),
    reg4!("SUBI", "Subtract immediate", 0xA4, ONE_IMM, 2),
    reg4!("SUBR", "Subtract relative", 0xA8, ONE_REL, 3),
    reg4!("SUBA", "Subtract absolute", 0xAC, ONE_ADDR, 4),
    reg4!("DAR",  "Decimal adjust register", 0x94, NONE, 3),

    // ===================== logical =====================
    reg3!("ANDZ", "AND to register zero", 0x40, NONE, 2),
    reg4!("ANDI", "AND immediate", 0x44, ONE_IMM, 2),
    reg4!("ANDR", "AND relative", 0x48, ONE_REL, 3),
    reg4!("ANDA", "AND absolute", 0x4C, ONE_ADDR, 4),
    reg4!("IORZ", "Inclusive-OR to register zero", 0x60, NONE, 2),
    reg4!("IORI", "Inclusive-OR immediate", 0x64, ONE_IMM, 2),
    reg4!("IORR", "Inclusive-OR relative", 0x68, ONE_REL, 3),
    reg4!("IORA", "Inclusive-OR absolute", 0x6C, ONE_ADDR, 4),
    reg4!("EORZ", "Exclusive-OR to register zero", 0x20, NONE, 2),
    reg4!("EORI", "Exclusive-OR immediate", 0x24, ONE_IMM, 2),
    reg4!("EORR", "Exclusive-OR relative", 0x28, ONE_REL, 3),
    reg4!("EORA", "Exclusive-OR absolute", 0x2C, ONE_ADDR, 4),
    reg4!("COMZ", "Compare to register zero", 0xE0, NONE, 2),
    reg4!("COMI", "Compare immediate", 0xE4, ONE_IMM, 2),
    reg4!("COMR", "Compare relative", 0xE8, ONE_REL, 3),
    reg4!("COMA", "Compare absolute", 0xEC, ONE_ADDR, 4),

    // ===================== rotate =====================
    reg4!("RRR", "Rotate register right", 0x50, NONE, 2),
    reg4!("RRL", "Rotate register left", 0xD0, NONE, 2),

    // ===================== branch on condition =====================
    cc4!("BCTR", "Branch on condition true, relative", 0x18, ONE_REL, 3),
    cc4!("BCTA", "Branch on condition true, absolute", 0x1C, ONE_ADDR, 3),
    cc4!("BCFR", "Branch on condition false, relative", 0x98, ONE_REL, 3),
    cc4!("BCFA", "Branch on condition false, absolute", 0x9C, ONE_ADDR, 3),
    cc4!("BSTR", "Branch to subroutine on condition true, relative", 0x38, ONE_REL, 3),
    cc4!("BSTA", "Branch to subroutine on condition true, absolute", 0x3C, ONE_ADDR, 3),
    cc4!("BSFR", "Branch to subroutine on condition false, relative", 0xB8, ONE_REL, 3),
    cc4!("BSFA", "Branch to subroutine on condition false, absolute", 0xBC, ONE_ADDR, 3),

    // ===================== branch on register =====================
    reg4!("BRNR", "Branch on register non-zero, relative", 0x58, ONE_REL, 3),
    reg4!("BRNA", "Branch on register non-zero, absolute", 0x5C, ONE_ADDR, 3),
    reg4!("BIRR", "Branch on incrementing register, relative", 0xD8, ONE_REL, 3),
    reg4!("BIRA", "Branch on incrementing register, absolute", 0xDC, ONE_ADDR, 3),
    reg4!("BDRR", "Branch on decrementing register, relative", 0xF8, ONE_REL, 3),
    reg4!("BDRA", "Branch on decrementing register, absolute", 0xFC, ONE_ADDR, 3),
    reg4!("BSNR", "Branch to subroutine on non-zero register, relative", 0x78, ONE_REL, 3),
    reg4!("BSNA", "Branch to subroutine on non-zero register, absolute", 0x7C, ONE_ADDR, 3),

    // ===================== return =====================
    cc4!("RETC", "Return from subroutine, conditional", 0x14, NONE, 3),
    cc4!("RETE", "Return and enable interrupt, conditional", 0x34, NONE, 3),

    // ===================== input / output =====================
    reg4!("REDC", "Read control", 0x30, NONE, 2),
    reg4!("REDD", "Read data", 0x70, NONE, 2),
    reg4!("WRTC", "Write control", 0xB0, NONE, 2),
    reg4!("WRTD", "Write data", 0xF0, NONE, 2),
    reg4!("REDE", "Read extended", 0x54, ONE_IMM, 3),
    reg4!("WRTE", "Write extended", 0xD4, ONE_IMM, 3),

    // ===================== program status =====================
    one!("LPSU", "Load program status, upper", 0x92, NONE, 2),
    one!("LPSL", "Load program status, lower", 0x93, NONE, 2),
    one!("SPSU", "Store program status, upper", 0x12, NONE, 2),
    one!("SPSL", "Store program status, lower", 0x13, NONE, 2),
    one!("CPSU", "Clear program status, upper, masked", 0x74, ONE_IMM, 3),
    one!("CPSL", "Clear program status, lower, masked", 0x75, ONE_IMM, 3),
    one!("PPSU", "Preset program status, upper, masked", 0x76, ONE_IMM, 3),
    one!("PPSL", "Preset program status, lower, masked", 0x77, ONE_IMM, 3),
    one!("TPSU", "Test program status, upper, masked", 0xB4, ONE_IMM, 3),
    one!("TPSL", "Test program status, lower, masked", 0xB5, ONE_IMM, 3),

    // ===================== misc =====================
    one!("HALT", "Halt, enter wait state", 0x40, NONE, 2),
    one!("NOP",  "No operation", 0xC0, NONE, 2),
    reg4!("TMI", "Test under mask immediate", 0xF4, ONE_IMM, 3),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn op(mn: &str, mode: &str) -> u8 {
        SET.find_form(mn, mode)
            .unwrap_or_else(|| panic!("no {mn} {mode}"))
            .opcode[0]
    }

    #[test]
    fn spot_check_opcodes() {
        assert_eq!(op("LODZ", "r1"), 0x01);
        assert_eq!(op("LODI", "r0"), 0x04);
        assert_eq!(op("LODR", "r3"), 0x0B);
        assert_eq!(op("LODA", "r3"), 0x0F);
        assert_eq!(op("STRZ", "r1"), 0xC1);
        assert_eq!(op("ADDI", "r0"), 0x84);
        assert_eq!(op("IORZ", "r0"), 0x60);
        assert_eq!(op("EORZ", "r0"), 0x20);
        assert_eq!(op("COMZ", "r0"), 0xE0);
        assert_eq!(op("DAR", "r0"), 0x94);
        assert_eq!(op("RRR", "r0"), 0x50);
        assert_eq!(op("RRL", "r3"), 0xD3);
        assert_eq!(op("BCTR", "eq"), 0x18);
        assert_eq!(op("BCTR", "un"), 0x1B);
        assert_eq!(op("BCFA", "un"), 0x9F);
        assert_eq!(op("BRNR", "r1"), 0x59);
        assert_eq!(op("BSFR", "un"), 0xBB);
        assert_eq!(op("RETC", "un"), 0x17);
        assert_eq!(op("RETE", "un"), 0x37);
        assert_eq!(op("REDC", "r0"), 0x30);
        assert_eq!(op("WRTE", "r0"), 0xD4);
        assert_eq!(op("LPSU", ""), 0x92);
        assert_eq!(op("CPSL", ""), 0x75);
        assert_eq!(op("HALT", ""), 0x40);
        assert_eq!(op("NOP", ""), 0xC0);
        assert_eq!(op("TMI", "r0"), 0xF4);
    }

    #[test]
    fn no_duplicate_opcodes() {
        // The `Z` r0 quirks (LODZ,R0 → IORZ,R0; STRZ,R0 = NOP; ANDZ,R0 = HALT)
        // and the ZBRR/BXA/ZBSR/BSXA aliases live in the dialect, so every spec
        // opcode is unique.
        let mut seen = [false; 256];
        for insn in SET.instructions {
            for form in insn.forms {
                let o = form.opcode[0] as usize;
                assert!(
                    !seen[o],
                    "duplicate opcode {o:#04X} at {} {}",
                    insn.mnemonic, form.mode
                );
                seen[o] = true;
            }
        }
    }

    #[test]
    fn form_lengths() {
        // Z/misc = 1 byte, immediate/relative = 2, absolute = 3.
        for insn in SET.instructions {
            for form in insn.forms {
                assert!(
                    (1..=3).contains(&form.len()),
                    "{} {}",
                    insn.mnemonic,
                    form.mode
                );
            }
        }
    }
}
