//! National Semiconductor SC/MP (INS8060) instruction set.
//!
//! The SC/MP ("Simple Cost-effective MicroProcessor") is an 8-bit CPU with a
//! single accumulator `AC`, an extension register `E`, a status register, and
//! four 16-bit pointer registers `P0`–`P3` — `P0` is the program counter. It
//! has no 16-bit memory operand in the instruction stream: memory is reached
//! through a **pointer register plus a signed 8-bit displacement**.
//!
//! Encoding shapes (all fixed-slot — no engine seam):
//! - **inherent** — a single opcode (`HALT`, `CCL`, the shifts, the `E`-register
//!   ALU ops `ANE`/`ORE`/…). Mode `""`.
//! - **pointer-exchange** — `XPAL`/`XPAH`/`XPPC` take a bare pointer number 0..3
//!   in the opcode's low two bits, no following byte. Mode `"0"`..`"3"`.
//! - **memory reference** — `LD`/`ST`/`AND`/`OR`/`XOR`/`DAD`/`ADD`/`CAD` take a
//!   `disp(ptr)` operand: the pointer in the low two bits, an optional `@`
//!   auto-index bit (`0x04`), then a signed displacement byte. The displacement
//!   value `0x80` (-128) means "use `E` as the displacement". `@` is only valid
//!   for pointers 1..3 — auto-indexing `P0` (the PC) *is* the immediate form.
//!   Modes `"0"`..`"3"` and `"@1"`..`"@3"`.
//! - **memory increment/decrement** — `ILD`/`DLD`: `disp(ptr)`, no `@`. Modes
//!   `"0"`..`"3"`.
//! - **transfer** — `JMP`/`JP`/`JZ`/`JNZ`: `disp(ptr)`, pointer-relative, no `@`
//!   (the `0x04` bit selects the condition). Modes `"0"`..`"3"`.
//! - **immediate** — `LDI`/`ANI`/`ORI`/`XRI`/`DAI`/`ADI`/`CAI` and the delay
//!   `DLY`: opcode + one byte. Mode `"imm"`. (The ALU immediates occupy the
//!   auto-index-`P0` opcode slot `base|0x04`.)
//!
//! Numbers under `asl` are C-style (`0x..` hex, decimal); the disassembler emits
//! signed decimal displacements and `0x..` immediates.
//!
//! **Provenance.** Authored from National Semiconductor's *SC/MP Technical
//! Description* / INS8060 data (primary library, `reference/by-topic/
//! cpu-scmp/`), every opcode cross-checked byte-for-byte against `asl`
//! (`cpu SC/MP`). Cycle counts are byte-length-derived approximations
//! (documentation-grade).

use crate::{Cycles, Endianness, Form, Instruction, InstructionSet, Operand, OperandKind};

const BYTE: Operand = Operand {
    kind: OperandKind::Immediate,
    bytes: 1,
};
const NONE: &[Operand] = &[];
const ONE_BYTE: &[Operand] = &[BYTE];

pub const SET: InstructionSet = InstructionSet {
    cpu: "National SC/MP (INS8060)",
    endianness: Endianness::Little,
    instructions: INSTRUCTIONS,
};

const fn f(
    opcode: &'static [u8],
    mode: &'static str,
    operands: &'static [Operand],
    cycles: u8,
    flags: &'static str,
) -> Form {
    Form {
        opcode,
        mode,
        operands,
        suffix: &[],
        cycles: Cycles::fixed(cycles),
        flags,
        undocumented: false,
    }
}

/// An inherent (single-byte) instruction.
macro_rules! inh {
    ($mn:literal, $sum:literal, $op:literal) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[f(&[$op], "", NONE, 1, "")],
        }
    };
}

/// Pointer-exchange: a bare pointer number 0..3 in the low two bits, no byte.
macro_rules! ptrx {
    ($mn:literal, $sum:literal, $base:literal) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[
                f(&[$base + 0], "0", NONE, 1, ""),
                f(&[$base + 1], "1", NONE, 1, ""),
                f(&[$base + 2], "2", NONE, 1, ""),
                f(&[$base + 3], "3", NONE, 1, ""),
            ],
        }
    };
}

/// Memory reference with the `@` auto-index option (pointers 1..3).
macro_rules! memref {
    ($mn:literal, $sum:literal, $base:literal) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[
                f(&[$base + 0], "0", ONE_BYTE, 2, ""),
                f(&[$base + 1], "1", ONE_BYTE, 2, ""),
                f(&[$base + 2], "2", ONE_BYTE, 2, ""),
                f(&[$base + 3], "3", ONE_BYTE, 2, ""),
                f(&[$base + 5], "@1", ONE_BYTE, 2, ""),
                f(&[$base + 6], "@2", ONE_BYTE, 2, ""),
                f(&[$base + 7], "@3", ONE_BYTE, 2, ""),
            ],
        }
    };
}

/// Displacement-only memory reference (no `@`): `ILD`/`DLD` and the transfers.
macro_rules! ptr4 {
    ($mn:literal, $sum:literal, $base:literal) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[
                f(&[$base + 0], "0", ONE_BYTE, 2, ""),
                f(&[$base + 1], "1", ONE_BYTE, 2, ""),
                f(&[$base + 2], "2", ONE_BYTE, 2, ""),
                f(&[$base + 3], "3", ONE_BYTE, 2, ""),
            ],
        }
    };
}

/// An immediate (opcode + one byte).
macro_rules! imm {
    ($mn:literal, $sum:literal, $op:literal) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[f(&[$op], "imm", ONE_BYTE, 2, "")],
        }
    };
}

#[rustfmt::skip]
const INSTRUCTIONS: &[Instruction] = &[
    // ===================== inherent =====================
    inh!("HALT", "Halt", 0x00),
    inh!("XAE",  "Exchange AC and E", 0x01),
    inh!("CCL",  "Clear carry/link", 0x02),
    inh!("SCL",  "Set carry/link", 0x03),
    inh!("DINT", "Disable interrupt", 0x04),
    inh!("IEN",  "Enable interrupt", 0x05),
    inh!("CSA",  "Copy status to AC", 0x06),
    inh!("CAS",  "Copy AC to status", 0x07),
    inh!("NOP",  "No operation", 0x08),
    inh!("SIO",  "Serial I/O", 0x19),
    inh!("SR",   "Shift right", 0x1C),
    inh!("SRL",  "Shift right with link", 0x1D),
    inh!("RR",   "Rotate right", 0x1E),
    inh!("RRL",  "Rotate right with link", 0x1F),
    inh!("LDE",  "Load AC from E", 0x40),
    inh!("ANE",  "AND E into AC", 0x50),
    inh!("ORE",  "OR E into AC", 0x58),
    inh!("XRE",  "XOR E into AC", 0x60),
    inh!("DAE",  "Decimal add E to AC", 0x68),
    inh!("ADE",  "Add E to AC", 0x70),
    inh!("CAE",  "Complement-add E to AC", 0x78),

    // ===================== pointer exchange =====================
    ptrx!("XPAL", "Exchange pointer low with AC", 0x30),
    ptrx!("XPAH", "Exchange pointer high with AC", 0x34),
    ptrx!("XPPC", "Exchange pointer with PC", 0x3C),

    // ===================== memory reference (with @) =====================
    memref!("LD",  "Load AC", 0xC0),
    memref!("ST",  "Store AC", 0xC8),
    memref!("AND", "AND to AC", 0xD0),
    memref!("OR",  "OR to AC", 0xD8),
    memref!("XOR", "XOR to AC", 0xE0),
    memref!("DAD", "Decimal add to AC", 0xE8),
    memref!("ADD", "Add to AC", 0xF0),
    memref!("CAD", "Complement-add to AC", 0xF8),

    // ===================== memory increment / decrement (no @) =====================
    ptr4!("ILD", "Increment and load", 0xA8),
    ptr4!("DLD", "Decrement and load", 0xB8),

    // ===================== transfer (no @) =====================
    ptr4!("JMP", "Jump", 0x90),
    ptr4!("JP",  "Jump if positive", 0x94),
    ptr4!("JZ",  "Jump if zero", 0x98),
    ptr4!("JNZ", "Jump if not zero", 0x9C),

    // ===================== immediate =====================
    imm!("LDI", "Load immediate", 0xC4),
    imm!("ANI", "AND immediate", 0xD4),
    imm!("ORI", "OR immediate", 0xDC),
    imm!("XRI", "XOR immediate", 0xE4),
    imm!("DAI", "Decimal-add immediate", 0xEC),
    imm!("ADI", "Add immediate", 0xF4),
    imm!("CAI", "Complement-add immediate", 0xFC),
    imm!("DLY", "Delay", 0x8F),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn op(mn: &str, mode: &str) -> u8 {
        SET.instruction(mn)
            .and_then(|i| i.form(mode))
            .unwrap_or_else(|| panic!("no {mn} {mode}"))
            .opcode[0]
    }

    #[test]
    fn spot_check_opcodes() {
        assert_eq!(op("NOP", ""), 0x08);
        assert_eq!(op("XAE", ""), 0x01);
        assert_eq!(op("CAE", ""), 0x78);
        assert_eq!(op("XPAL", "1"), 0x31);
        assert_eq!(op("XPPC", "3"), 0x3F);
        assert_eq!(op("LD", "1"), 0xC1);
        assert_eq!(op("LD", "@2"), 0xC6);
        assert_eq!(op("ST", "@1"), 0xCD);
        assert_eq!(op("CAD", "@3"), 0xFF);
        assert_eq!(op("ILD", "1"), 0xA9);
        assert_eq!(op("JMP", "0"), 0x90);
        assert_eq!(op("JNZ", "3"), 0x9F);
        assert_eq!(op("LDI", "imm"), 0xC4);
        assert_eq!(op("XRI", "imm"), 0xE4);
        assert_eq!(op("DLY", "imm"), 0x8F);
    }

    #[test]
    fn no_duplicate_opcodes() {
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
        for insn in SET.instructions {
            for form in insn.forms {
                let len = 1 + form
                    .operands
                    .iter()
                    .map(|o| o.bytes as usize)
                    .sum::<usize>();
                assert!(
                    (1..=2).contains(&len),
                    "{} {} len {len}",
                    insn.mnemonic,
                    form.mode
                );
            }
        }
    }
}
