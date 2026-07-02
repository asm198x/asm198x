//! Fairchild F8 (3850) instruction set.
//!
//! The F8 is the 8-bit CPU at the heart of the **Fairchild Channel F** (1976),
//! the first ROM-cartridge games console. It is a multi-chip family (the 3850
//! CPU plus 3851 PSU / 3852 / 3853 support chips); the program-visible model is
//! the accumulator `A`, the status register `W`, the indirect scratchpad address
//! register `ISAR`, the **64 scratchpad registers** (0–63), the 16-bit data
//! counter `DC`, and the program/stack counters `PC0`/`PC1`.
//!
//! Encoding shapes:
//! - **inherent** — a single opcode (`LM`, `ST`, `COM`, `LNK`, `POP`, `NOP`,
//!   `XDC`, the memory ALU ops `AM`/`AMD`/`NM`/`OM`/`XM`/`CM`/`ADC`, …). Mode `""`.
//! - **register nibble** — the scratchpad ops `DS`/`AS`/`ASD`/`XS`/`NS` and the
//!   register moves `LR A,r` / `LR r,A` pack a 4-bit register field into the
//!   opcode's low nibble (0–11 direct; 12/13/14 = `S`/`I`/`D`, reaching the
//!   register `ISAR` points at with no change / post-increment / post-decrement).
//!   One form per value, modes `"0"`..`"15"` (and `"a,0"`.. / `"0,a"`.. for `LR`).
//!   No following byte — the same idiom as the CDP1802 and SC/MP register forms.
//! - **immediate-nibble** — `LIS`/`INS`/`OUTS` (4-bit value 0–15) and
//!   `LISU`/`LISL` (3-bit octal digit 0–7) pack the value into the opcode. Modes
//!   `"0"`..
//! - **immediate byte** — the ALU immediates `LI`/`NI`/`OI`/`XI`/`AI`/`CI`
//!   (mode `"imm"`) and the port ops `IN`/`OUT` (mode `"port"`): opcode + one byte.
//! - **16-bit address** — `PI`/`JMP`/`DCI`: opcode + a **big-endian** address.
//!   Mode `"abs"`.
//! - **relative branch** — `BT`/`BF` (with a test mask in the opcode nibble), the
//!   named `BR`/`BP`/… convenience mnemonics, and `BR7`: opcode + a signed 8-bit
//!   offset. The offset is measured from the address of the **offset byte itself**
//!   (one byte after the opcode), not from the following instruction — so the
//!   dialect emits branches through the computed-operand seam with a `+1`
//!   correction (see the dialect). Here the forms carry a [`OperandKind::RelativePc`]
//!   byte so the disassembler and the conformance sweep see the wire shape.
//!
//! `LR` is a single mnemonic with many forms: the fixed register pairs
//! (`A,KU`.., `DC,H`, `W,J`, …) plus the scratchpad-register moves. `CLR` is
//! `LIS 0` (opcode `0x70`) — provided as a dialect alias, not a separate form.
//!
//! **Provenance.** Authored from Fairchild's *F8 Guide to Programming*
//! (67095664, primary library, `reference/by-topic/cpu-f8/`), every opcode
//! cross-checked byte-for-byte against `asl` (`cpu F3850`). Cycle counts are
//! byte-length-derived approximations (documentation-grade).

use crate::{Cycles, Endianness, Form, Instruction, InstructionSet, Operand, OperandKind};

const BYTE: Operand = Operand {
    kind: OperandKind::Immediate,
    bytes: 1,
};
const ADDR16: Operand = Operand {
    kind: OperandKind::Address,
    bytes: 2,
};
const REL: Operand = Operand {
    kind: OperandKind::RelativePc,
    bytes: 1,
};
const NONE: &[Operand] = &[];
const ONE_BYTE: &[Operand] = &[BYTE];
const ONE_ADDR: &[Operand] = &[ADDR16];
const ONE_REL: &[Operand] = &[REL];

pub const SET: InstructionSet = InstructionSet {
    cpu: "Fairchild F8 (3850)",
    endianness: Endianness::Big,
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

/// An immediate: opcode + one byte, mode `$mode`.
macro_rules! imm {
    ($mn:literal, $sum:literal, $op:literal, $mode:literal) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[f(&[$op], $mode, ONE_BYTE, 2, "")],
        }
    };
}

/// A 16-bit big-endian absolute (opcode + address): `PI`/`JMP`/`DCI`.
macro_rules! abs {
    ($mn:literal, $sum:literal, $op:literal) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[f(&[$op], "abs", ONE_ADDR, 3, "")],
        }
    };
}

/// 16 forms `opcode|0`..`opcode|15`, modes `"0"`..`"15"`, operands `$ops`.
/// The register/value nibble idiom (scratchpad ops, `LIS`/`INS`/`OUTS`, `BF`).
macro_rules! nib16 {
    ($mn:literal, $sum:literal, $base:literal, $ops:expr, $cy:literal) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[
                f(&[$base + 0], "0", $ops, $cy, ""),
                f(&[$base + 1], "1", $ops, $cy, ""),
                f(&[$base + 2], "2", $ops, $cy, ""),
                f(&[$base + 3], "3", $ops, $cy, ""),
                f(&[$base + 4], "4", $ops, $cy, ""),
                f(&[$base + 5], "5", $ops, $cy, ""),
                f(&[$base + 6], "6", $ops, $cy, ""),
                f(&[$base + 7], "7", $ops, $cy, ""),
                f(&[$base + 8], "8", $ops, $cy, ""),
                f(&[$base + 9], "9", $ops, $cy, ""),
                f(&[$base + 10], "10", $ops, $cy, ""),
                f(&[$base + 11], "11", $ops, $cy, ""),
                f(&[$base + 12], "12", $ops, $cy, ""),
                f(&[$base + 13], "13", $ops, $cy, ""),
                f(&[$base + 14], "14", $ops, $cy, ""),
                f(&[$base + 15], "15", $ops, $cy, ""),
            ],
        }
    };
}

/// 8 forms `opcode|0`..`opcode|7`, modes `"0"`..`"7"`: `LISU`/`LISL`, `BT`.
macro_rules! nib8 {
    ($mn:literal, $sum:literal, $base:literal, $ops:expr, $cy:literal) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[
                f(&[$base + 0], "0", $ops, $cy, ""),
                f(&[$base + 1], "1", $ops, $cy, ""),
                f(&[$base + 2], "2", $ops, $cy, ""),
                f(&[$base + 3], "3", $ops, $cy, ""),
                f(&[$base + 4], "4", $ops, $cy, ""),
                f(&[$base + 5], "5", $ops, $cy, ""),
                f(&[$base + 6], "6", $ops, $cy, ""),
                f(&[$base + 7], "7", $ops, $cy, ""),
            ],
        }
    };
}

/// `LR A,r`: 16 forms modes `"a,0"`..`"a,15"`, opcode `0x40|r`.
macro_rules! lr_a {
    () => {
        Instruction {
            mnemonic: "LR",
            summary: "Load A from scratchpad register",
            forms: &[
                f(&[0x40], "a,0", NONE, 1, ""),
                f(&[0x41], "a,1", NONE, 1, ""),
                f(&[0x42], "a,2", NONE, 1, ""),
                f(&[0x43], "a,3", NONE, 1, ""),
                f(&[0x44], "a,4", NONE, 1, ""),
                f(&[0x45], "a,5", NONE, 1, ""),
                f(&[0x46], "a,6", NONE, 1, ""),
                f(&[0x47], "a,7", NONE, 1, ""),
                f(&[0x48], "a,8", NONE, 1, ""),
                f(&[0x49], "a,9", NONE, 1, ""),
                f(&[0x4A], "a,10", NONE, 1, ""),
                f(&[0x4B], "a,11", NONE, 1, ""),
                f(&[0x4C], "a,12", NONE, 1, ""),
                f(&[0x4D], "a,13", NONE, 1, ""),
                f(&[0x4E], "a,14", NONE, 1, ""),
                f(&[0x4F], "a,15", NONE, 1, ""),
            ],
        }
    };
}

/// `LR r,A`: 16 forms modes `"0,a"`..`"15,a"`, opcode `0x50|r`.
macro_rules! lr_r {
    () => {
        Instruction {
            mnemonic: "LR",
            summary: "Store A to scratchpad register",
            forms: &[
                f(&[0x50], "0,a", NONE, 1, ""),
                f(&[0x51], "1,a", NONE, 1, ""),
                f(&[0x52], "2,a", NONE, 1, ""),
                f(&[0x53], "3,a", NONE, 1, ""),
                f(&[0x54], "4,a", NONE, 1, ""),
                f(&[0x55], "5,a", NONE, 1, ""),
                f(&[0x56], "6,a", NONE, 1, ""),
                f(&[0x57], "7,a", NONE, 1, ""),
                f(&[0x58], "8,a", NONE, 1, ""),
                f(&[0x59], "9,a", NONE, 1, ""),
                f(&[0x5A], "10,a", NONE, 1, ""),
                f(&[0x5B], "11,a", NONE, 1, ""),
                f(&[0x5C], "12,a", NONE, 1, ""),
                f(&[0x5D], "13,a", NONE, 1, ""),
                f(&[0x5E], "14,a", NONE, 1, ""),
                f(&[0x5F], "15,a", NONE, 1, ""),
            ],
        }
    };
}

#[rustfmt::skip]
const INSTRUCTIONS: &[Instruction] = &[
    // ===================== LR: fixed register pairs =====================
    Instruction { mnemonic: "LR", summary: "Load register", forms: &[
        f(&[0x00], "a,ku", NONE, 1, ""), f(&[0x01], "a,kl", NONE, 1, ""),
        f(&[0x02], "a,qu", NONE, 1, ""), f(&[0x03], "a,ql", NONE, 1, ""),
        f(&[0x04], "ku,a", NONE, 1, ""), f(&[0x05], "kl,a", NONE, 1, ""),
        f(&[0x06], "qu,a", NONE, 1, ""), f(&[0x07], "ql,a", NONE, 1, ""),
        f(&[0x08], "k,p", NONE, 1, ""),  f(&[0x09], "p,k", NONE, 1, ""),
        f(&[0x0A], "a,is", NONE, 1, ""), f(&[0x0B], "is,a", NONE, 1, ""),
        f(&[0x0D], "p0,q", NONE, 1, ""), f(&[0x0E], "q,dc", NONE, 1, ""),
        f(&[0x0F], "dc,q", NONE, 1, ""), f(&[0x10], "dc,h", NONE, 1, ""),
        f(&[0x11], "h,dc", NONE, 1, ""),
        f(&[0x1D], "w,j", NONE, 1, ""),  f(&[0x1E], "j,w", NONE, 1, ""),
    ] },
    lr_a!(),
    lr_r!(),

    // ===================== inherent control / memory =====================
    inh!("PK",  "Store PC1 into PC0 via K", 0x0C),
    inh!("LM",  "Load A from memory (DC)", 0x16),
    inh!("ST",  "Store A to memory (DC)", 0x17),
    inh!("COM", "Complement A", 0x18),
    inh!("LNK", "Add carry link to A", 0x19),
    inh!("DI",  "Disable interrupt", 0x1A),
    inh!("EI",  "Enable interrupt", 0x1B),
    inh!("POP", "Pop: PC1 into PC0", 0x1C),
    inh!("INC", "Increment A", 0x1F),
    inh!("NOP", "No operation", 0x2B),
    inh!("XDC", "Exchange DC and DC1", 0x2C),
    inh!("AM",  "Add memory to A", 0x88),
    inh!("AMD", "Decimal add memory to A", 0x89),
    inh!("NM",  "AND memory into A", 0x8A),
    inh!("OM",  "OR memory into A", 0x8B),
    inh!("XM",  "XOR memory into A", 0x8C),
    inh!("CM",  "Compare memory with A", 0x8D),
    inh!("ADC", "Add A to DC", 0x8E),

    // ===================== shifts (by 1 or 4) =====================
    Instruction { mnemonic: "SR", summary: "Shift A right", forms: &[
        f(&[0x12], "1", NONE, 1, ""), f(&[0x14], "4", NONE, 1, ""),
    ] },
    Instruction { mnemonic: "SL", summary: "Shift A left", forms: &[
        f(&[0x13], "1", NONE, 1, ""), f(&[0x15], "4", NONE, 1, ""),
    ] },

    // ===================== immediate byte =====================
    imm!("LI", "Load immediate to A", 0x20, "imm"),
    imm!("NI", "AND immediate", 0x21, "imm"),
    imm!("OI", "OR immediate", 0x22, "imm"),
    imm!("XI", "XOR immediate", 0x23, "imm"),
    imm!("AI", "Add immediate", 0x24, "imm"),
    imm!("CI", "Compare immediate", 0x25, "imm"),
    imm!("IN", "Input port to A", 0x26, "port"),
    imm!("OUT", "Output A to port", 0x27, "port"),

    // ===================== 16-bit big-endian address =====================
    abs!("PI",  "Call subroutine immediate", 0x28),
    abs!("JMP", "Jump immediate", 0x29),
    abs!("DCI", "Load DC immediate", 0x2A),

    // ===================== scratchpad register nibble =====================
    nib16!("DS",  "Decrement scratchpad", 0x30, NONE, 1),
    nib16!("AS",  "Add scratchpad to A", 0xC0, NONE, 1),
    nib16!("ASD", "Decimal add scratchpad to A", 0xD0, NONE, 1),
    nib16!("XS",  "XOR scratchpad into A", 0xE0, NONE, 1),
    nib16!("NS",  "AND scratchpad into A", 0xF0, NONE, 1),

    // ===================== immediate-nibble loads / ports =====================
    nib8!("LISU", "Load upper octal digit of ISAR", 0x60, NONE, 1),
    nib8!("LISL", "Load lower octal digit of ISAR", 0x68, NONE, 1),
    nib16!("LIS", "Load 4-bit immediate to A", 0x70, NONE, 1),
    nib16!("INS",  "Input short port to A", 0xA0, NONE, 1),
    nib16!("OUTS", "Output A to short port", 0xB0, NONE, 1),

    // ===================== branches (relative, offset-byte base) =====================
    nib8!("BT",  "Branch on test mask true", 0x80, ONE_REL, 2),
    nib16!("BF", "Branch on test mask false", 0x90, ONE_REL, 2),
    Instruction { mnemonic: "BR7", summary: "Branch if ISAR low octal != 7", forms: &[
        f(&[0x8F], "", ONE_REL, 2, ""),
    ] },
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
        assert_eq!(op("LR", "a,ku"), 0x00);
        assert_eq!(op("LR", "h,dc"), 0x11);
        assert_eq!(op("LR", "a,3"), 0x43);
        assert_eq!(op("LR", "a,12"), 0x4C);
        assert_eq!(op("LR", "15,a"), 0x5F);
        assert_eq!(op("LR", "w,j"), 0x1D);
        assert_eq!(op("DS", "1"), 0x31);
        assert_eq!(op("AS", "12"), 0xCC);
        assert_eq!(op("NS", "0"), 0xF0);
        assert_eq!(op("LI", "imm"), 0x20);
        assert_eq!(op("IN", "port"), 0x26);
        assert_eq!(op("DCI", "abs"), 0x2A);
        assert_eq!(op("LIS", "5"), 0x75);
        assert_eq!(op("LISU", "3"), 0x63);
        assert_eq!(op("LISL", "6"), 0x6E);
        assert_eq!(op("INS", "4"), 0xA4);
        assert_eq!(op("OUTS", "15"), 0xBF);
        assert_eq!(op("BT", "1"), 0x81);
        assert_eq!(op("BF", "0"), 0x90);
        assert_eq!(op("BF", "15"), 0x9F);
        assert_eq!(op("BR7", ""), 0x8F);
        assert_eq!(op("SL", "4"), 0x15);
    }

    #[test]
    fn no_duplicate_opcodes() {
        // `CLR` (`0x70`) is a dialect alias for `LIS 0`, not a spec form, so
        // every spec opcode is unique across all forms.
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
        // Inherent/register/nibble forms are 1 byte; immediate and branch 2;
        // the 16-bit absolutes 3.
        for insn in SET.instructions {
            for form in insn.forms {
                let len = form.len();
                assert!(
                    (1..=3).contains(&len),
                    "{} {} len {len}",
                    insn.mnemonic,
                    form.mode
                );
            }
        }
    }
}
