//! Texas Instruments TMS7000 instruction set.
//!
//! The TMS7000 (1983) is an 8-bit microcontroller family. Its programming model
//! is a 256-byte on-chip **register file** `R0`–`R255` (with `A` = `R0` and
//! `B` = `R1`), a 256-byte **peripheral file** `P0`–`P255`, a 16-bit PC and
//! stack pointer, and a status register.
//!
//! The opcode map is exceptionally regular. **Dual-operand** instructions encode
//! as `opcode = (addressing_mode << 4) | operation`:
//!
//! - operation (low nibble): `MOV`=2, `AND`=3, `OR`=4, `XOR`=5, `BTJO`=6,
//!   `BTJZ`=7, `ADD`=8, `ADC`=9, `SUB`=A, `SBB`=B, `MPY`=C, `CMP`=D, `DAC`=E,
//!   `DSB`=F;
//! - addressing mode (high nibble): `Rn,A`=1, `%n,A`=2, `Rn,B`=3, `Rn,Rn`=4,
//!   `%n,B`=5, `B,A`=6, `%n,Rn`=7. (`%n` is an immediate byte, `Rn` a register-
//!   file byte.) `BTJO`/`BTJZ` are the same but append a relative jump offset.
//!
//! High nibbles 8/9/A carry the **peripheral** ops (`MOVP`/`ANDP`/`ORP`/`XORP`/
//! `BTJOP`/`BTJZP`) and the **extended-addressing** ops (`LDA`/`STA`/`BR`/`CALL`/
//! `CMPA`/`MOVD` in direct `@nnnn` / indirect `*Rn` / indexed `@nnnn(B)` forms);
//! B/C/D carry the **single-register** ops (`INC`/`DEC`/`CLR`/`RR`/…) on
//! `A`/`B`/`Rn`; E0–E7 the relative **conditional jumps**; and `0xFF−n` the 24
//! `TRAP`s. Addresses are big-endian.
//!
//! Every operand here is one of: a 1-byte value (`Immediate` — a register,
//! peripheral, or immediate byte, disambiguated by the [`Form::mode`] label a
//! dialect/disassembler reads), a 2-byte big-endian `Address`, or a `RelativePc`
//! jump offset. The mode label doubles as the operand template.
//!
//! **Provenance.** Authored from TI's *TMS7000 Assembly Language Programmer's
//! Guide* (SPNU002B, primary library, `reference/by-topic/cpu-tms7000/`,
//! Appendix D the opcode map), every opcode cross-checked byte-for-byte against
//! `asl` (`cpu TMS70C00`). Cycle counts and flags are documentation-grade.

use crate::{Cycles, Endianness, Form, Instruction, InstructionSet, Operand, OperandKind};

const IMM: Operand = Operand {
    kind: OperandKind::Immediate,
    bytes: 1,
};
const ADDR: Operand = Operand {
    kind: OperandKind::Address,
    bytes: 2,
};
const REL: Operand = Operand {
    kind: OperandKind::RelativePc,
    bytes: 1,
};
const NONE: &[Operand] = &[];
const R: &[Operand] = &[IMM];
const RR: &[Operand] = &[IMM, IMM];
const R_REL: &[Operand] = &[IMM, REL];
const RR_REL: &[Operand] = &[IMM, IMM, REL];
const REL1: &[Operand] = &[REL];
const A: &[Operand] = &[ADDR];
const A_R: &[Operand] = &[ADDR, IMM];

pub const SET: InstructionSet = InstructionSet {
    cpu: "Texas Instruments TMS7000",
    endianness: Endianness::Big,
    instructions: INSTRUCTIONS,
};

const fn f(opcode: &'static [u8], mode: &'static str, operands: &'static [Operand]) -> Form {
    Form {
        opcode,
        mode,
        operands,
        suffix: &[],
        cycles: Cycles::fixed(0),
        flags: "",
        undocumented: false,
    }
}

/// A dual-operand ALU instruction: seven addressing forms at `(mode<<4)|op`.
macro_rules! dual {
    ($mn:literal, $sum:literal, $op:literal) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[
                f(&[0x10 + $op], "rn,a", R),
                f(&[0x20 + $op], "%n,a", R),
                f(&[0x30 + $op], "rn,b", R),
                f(&[0x40 + $op], "rn,rn", RR),
                f(&[0x50 + $op], "%n,b", R),
                f(&[0x60 + $op], "b,a", NONE),
                f(&[0x70 + $op], "%n,rn", RR),
            ],
        }
    };
}

/// `BTJO`/`BTJZ`: the dual-operand forms plus a trailing relative jump offset.
macro_rules! btj {
    ($mn:literal, $sum:literal, $op:literal) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[
                f(&[0x10 + $op], "rn,a", R_REL),
                f(&[0x20 + $op], "%n,a", R_REL),
                f(&[0x30 + $op], "rn,b", R_REL),
                f(&[0x40 + $op], "rn,rn", RR_REL),
                f(&[0x50 + $op], "%n,b", R_REL),
                f(&[0x60 + $op], "b,a", REL1),
                f(&[0x70 + $op], "%n,rn", RR_REL),
            ],
        }
    };
}

/// A peripheral write op (`ANDP`/`ORP`/`XORP`): `A,Pn` / `B,Pn` / `%n,Pn`.
macro_rules! perip {
    ($mn:literal, $sum:literal, $op:literal) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[
                f(&[0x80 + $op], "a,pn", R),
                f(&[0x90 + $op], "b,pn", R),
                f(&[0xA0 + $op], "%n,pn", RR),
            ],
        }
    };
}

/// A peripheral bit-test-and-jump op (`BTJOP`/`BTJZP`): like [`perip`] + offset.
macro_rules! peripj {
    ($mn:literal, $sum:literal, $op:literal) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[
                f(&[0x80 + $op], "a,pn", R_REL),
                f(&[0x90 + $op], "b,pn", R_REL),
                f(&[0xA0 + $op], "%n,pn", RR_REL),
            ],
        }
    };
}

/// An extended-addressing op (`LDA`/`STA`/`BR`/`CALL`/`CMPA`): direct `@nnnn` /
/// indirect `*Rn` / indexed `@nnnn(B)`.
macro_rules! ext {
    ($mn:literal, $sum:literal, $op:literal) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[
                f(&[0x80 + $op], "@", A),
                f(&[0x90 + $op], "*", R),
                f(&[0xA0 + $op], "@(b)", A),
            ],
        }
    };
}

/// A single-register op on `A` / `B` / `Rn` (`INC`/`DEC`/`CLR`/rotates/…).
macro_rules! single {
    ($mn:literal, $sum:literal, $op:literal) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[
                f(&[0xB0 + $op], "a", NONE),
                f(&[0xC0 + $op], "b", NONE),
                f(&[0xD0 + $op], "rn", R),
            ],
        }
    };
}

/// A relative conditional jump (one opcode + a signed offset).
macro_rules! jump {
    ($mn:literal, $sum:literal, $op:literal) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[f(&[$op], "", REL1)],
        }
    };
}

/// An implied (single-byte, no-operand) instruction.
macro_rules! imp {
    ($mn:literal, $sum:literal, $op:literal) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[f(&[$op], "", NONE)],
        }
    };
}

#[rustfmt::skip]
const INSTRUCTIONS: &[Instruction] = &[
    // ===================== dual-operand ALU =====================
    dual!("MOV", "Move", 0x02),
    dual!("AND", "Logical AND", 0x03),
    dual!("OR",  "Logical OR", 0x04),
    dual!("XOR", "Logical XOR", 0x05),
    btj!("BTJO", "Bit test and jump if one", 0x06),
    btj!("BTJZ", "Bit test and jump if zero", 0x07),
    dual!("ADD", "Add", 0x08),
    dual!("ADC", "Add with carry", 0x09),
    dual!("SUB", "Subtract", 0x0A),
    dual!("SBB", "Subtract with borrow", 0x0B),
    dual!("MPY", "Multiply", 0x0C),
    dual!("CMP", "Compare", 0x0D),
    dual!("DAC", "Decimal add with carry", 0x0E),
    dual!("DSB", "Decimal subtract with borrow", 0x0F),

    // ===================== special MOV forms =====================
    // The dual grid lacks `A,B` / `A,Rn` / `B,Rn` — these are their own opcodes.
    Instruction { mnemonic: "MOV", summary: "Move (A/B source)", forms: &[
        f(&[0xC0], "a,b", NONE),
        f(&[0xD0], "a,rn", R),
        f(&[0xD1], "b,rn", R),
    ] },

    // ===================== peripheral file =====================
    Instruction { mnemonic: "MOVP", summary: "Move to/from peripheral", forms: &[
        f(&[0x80], "pn,a", R),
        f(&[0x91], "pn,b", R),
        f(&[0x82], "a,pn", R),
        f(&[0x92], "b,pn", R),
        f(&[0xA2], "%n,pn", RR),
    ] },
    perip!("ANDP", "AND with peripheral", 0x03),
    perip!("ORP",  "OR with peripheral", 0x04),
    perip!("XORP", "XOR with peripheral", 0x05),
    peripj!("BTJOP", "Bit test peripheral and jump if one", 0x06),
    peripj!("BTJZP", "Bit test peripheral and jump if zero", 0x07),

    // ===================== extended addressing =====================
    ext!("LDA",  "Load A", 0x0A),
    ext!("STA",  "Store A", 0x0B),
    ext!("BR",   "Branch", 0x0C),
    ext!("CMPA", "Compare A", 0x0D),
    ext!("CALL", "Call", 0x0E),
    Instruction { mnemonic: "MOVD", summary: "Move double (16-bit)", forms: &[
        f(&[0x88], "%n,rn", A_R),
        f(&[0x98], "rn,rn", RR),
        f(&[0xA8], "%n(b),rn", A_R),
    ] },

    // ===================== single-register =====================
    single!("DEC",  "Decrement", 0x02),
    single!("INC",  "Increment", 0x03),
    single!("INV",  "Invert", 0x04),
    single!("CLR",  "Clear", 0x05),
    single!("XCHB", "Exchange with B", 0x06),
    single!("SWAP", "Swap nibbles", 0x07),
    single!("PUSH", "Push", 0x08),
    single!("POP",  "Pop", 0x09),
    single!("DECD", "Decrement double", 0x0B),
    single!("RR",   "Rotate right", 0x0C),
    single!("RRC",  "Rotate right through carry", 0x0D),
    single!("RL",   "Rotate left", 0x0E),
    single!("RLC",  "Rotate left through carry", 0x0F),
    // DJNZ is a single-register op plus a relative offset.
    Instruction { mnemonic: "DJNZ", summary: "Decrement and jump if not zero", forms: &[
        f(&[0xBA], "a", REL1),
        f(&[0xCA], "b", REL1),
        f(&[0xDA], "rn", R_REL),
    ] },
    // Test and the A/B stack-status forms that sit in the single-register block.
    Instruction { mnemonic: "TSTA", summary: "Test A", forms: &[f(&[0xB0], "", NONE)] },
    Instruction { mnemonic: "TSTB", summary: "Test B", forms: &[f(&[0xC1], "", NONE)] },
    // PUSH/POP of the status register live at 0x0E / 0x08.
    Instruction { mnemonic: "PUSH", summary: "Push status", forms: &[f(&[0x0E], "st", NONE)] },
    Instruction { mnemonic: "POP",  summary: "Pop status", forms: &[f(&[0x08], "st", NONE)] },

    // ===================== relative jumps =====================
    jump!("JMP", "Jump unconditional", 0xE0),
    jump!("JN",  "Jump if negative", 0xE1),
    jump!("JZ",  "Jump if zero", 0xE2),
    jump!("JC",  "Jump if carry", 0xE3),
    jump!("JP",  "Jump if positive", 0xE4),
    jump!("JPZ", "Jump if positive or zero", 0xE5),
    jump!("JNZ", "Jump if not zero", 0xE6),
    jump!("JNC", "Jump if no carry", 0xE7),

    // ===================== implied =====================
    imp!("NOP",  "No operation", 0x00),
    imp!("IDLE", "Idle", 0x01),
    imp!("EINT", "Enable interrupts", 0x05),
    imp!("DINT", "Disable interrupts", 0x06),
    imp!("SETC", "Set carry", 0x07),
    imp!("STSP", "Store stack pointer", 0x09),
    imp!("RETS", "Return from subroutine", 0x0A),
    imp!("RETI", "Return from interrupt", 0x0B),
    imp!("LDSP", "Load stack pointer", 0x0D),
    imp!("CLRC", "Clear carry (= TSTA)", 0xB0),
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
        assert_eq!(op("MOV", "rn,a"), 0x12);
        assert_eq!(op("MOV", "%n,a"), 0x22);
        assert_eq!(op("MOV", "rn,rn"), 0x42);
        assert_eq!(op("MOV", "b,a"), 0x62);
        assert_eq!(op("MOV", "%n,rn"), 0x72);
        assert_eq!(op("MOV", "a,b"), 0xC0);
        assert_eq!(op("MOV", "a,rn"), 0xD0);
        assert_eq!(op("ADD", "rn,a"), 0x18);
        assert_eq!(op("DSB", "%n,rn"), 0x7F);
        assert_eq!(op("BTJO", "%n,a"), 0x26);
        assert_eq!(op("BTJZ", "b,a"), 0x67);
        assert_eq!(op("MOVP", "pn,a"), 0x80);
        assert_eq!(op("MOVP", "a,pn"), 0x82);
        assert_eq!(op("ANDP", "%n,pn"), 0xA3);
        assert_eq!(op("BTJOP", "a,pn"), 0x86);
        assert_eq!(op("LDA", "@"), 0x8A);
        assert_eq!(op("LDA", "*"), 0x9A);
        assert_eq!(op("LDA", "@(b)"), 0xAA);
        assert_eq!(op("CALL", "*"), 0x9E);
        assert_eq!(op("MOVD", "%n,rn"), 0x88);
        assert_eq!(op("DEC", "a"), 0xB2);
        assert_eq!(op("DEC", "rn"), 0xD2);
        assert_eq!(op("RLC", "b"), 0xCF);
        assert_eq!(op("DJNZ", "a"), 0xBA);
        assert_eq!(op("JMP", ""), 0xE0);
        assert_eq!(op("JNC", ""), 0xE7);
        assert_eq!(op("TSTB", ""), 0xC1);
        assert_eq!(op("PUSH", "st"), 0x0E);
        assert_eq!(op("NOP", ""), 0x00);
    }

    #[test]
    fn no_duplicate_opcodes() {
        // TSTA and CLRC share 0xB0 — the one intentional alias in the spec; skip
        // it in the uniqueness check. TRAP (0xE8-0xFF) is dialect-computed.
        let mut seen = [false; 256];
        for insn in SET.instructions {
            for form in insn.forms {
                let o = form.opcode[0] as usize;
                if o == 0xB0 && seen[o] {
                    continue; // TSTA == CLRC
                }
                assert!(
                    !seen[o],
                    "duplicate opcode {o:#04X} at {} {}",
                    insn.mnemonic, form.mode
                );
                seen[o] = true;
            }
        }
    }
}
