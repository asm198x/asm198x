//! Zilog Z8000 instruction set — the family's largest ISA (110 instruction
//! types, eight addressing modes, word/byte/long sizes, segmented and
//! non-segmented models). **Big-endian**; built as sweep-verified **increments**
//! (see `decisions/z8000-staged-build.md`), so this table grows one instruction
//! group at a time and everything not yet covered decodes to `word` data.
//!
//! Like [`crate::pdp11`] and [`crate::tms9900`], operands are **fields inside the
//! opcode word**, so this is a **bespoke table** keyed by the dialect and a
//! field-based disassembler, not the [`crate::Form`] model. Target `cpu Z8002`
//! (non-segmented) first.
//!
//! **Increment 1 — the dyadic family.** `ADD`/`SUB`/`OR`/`AND`/`XOR`/`CP`/`LD`
//! (+ `ADC`/`SBC`, register-only) and their byte forms share one first word:
//!
//! ```text
//!   MM ooooo b   ssss dddd
//! ```
//!
//! `MM` (bits 15–14) is the addressing-mode group — `00` → `@Rs` (IR) when
//! `ssss ≠ 0` else immediate (IM, word follows); `01` → direct address (DA, word
//! follows) when `ssss = 0` else indexed (X, `addr(Rs)`); `10` → register (R).
//! `ooooo` (bits 13–9) is the operation, `b` (bit 8) is word (1) / byte (0), and
//! the second byte is `source-field << 4 | destination-register`. `LD` also has
//! **store** forms (register → memory) that reuse operation `0x17` in the IR /
//! DA / X groups, with the second byte's high nibble the memory pointer/index
//! and the low nibble the source register. Byte immediates replicate the byte
//! into both halves of the word (`addb rl1,#12h` → `0009 1212`).
//!
//! Every opcode is validated byte-for-byte against `asl` (`cpu Z8002`).

use crate::{Endianness, InstructionSet};

/// The field layout of an instruction group. Grows per increment.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    /// The dyadic arithmetic / logic / load family (increment 1).
    Dyadic,
}

/// One Z8000 mnemonic.
pub struct Insn {
    pub mnemonic: &'static str,
    /// The 5-bit operation code (`ooooo`).
    pub op: u8,
    /// Byte operation (`b = 0`) rather than word (`b = 1`).
    pub byte: bool,
    /// Only the register source mode is legal (`ADC`/`SBC`).
    pub reg_only: bool,
    /// A register → memory store form (`LD`/`LDB` with a memory destination);
    /// `op` is the store operation code.
    pub store: bool,
    pub class: Class,
    pub summary: &'static str,
}

/// Find the (first) instruction entry for a mnemonic (case-insensitive). `LD`
/// and `LDB` resolve to their **load** entry; the dialect selects the store
/// entry itself when the destination is memory.
#[must_use]
pub fn lookup(mnemonic: &str) -> Option<&'static Insn> {
    let m = mnemonic;
    INSTRUCTIONS
        .iter()
        .find(|i| i.mnemonic.eq_ignore_ascii_case(m) && !i.store)
}

/// The store entry for `LD`/`LDB` (register → memory).
#[must_use]
pub fn store_entry(mnemonic: &str) -> Option<&'static Insn> {
    INSTRUCTIONS
        .iter()
        .find(|i| i.mnemonic.eq_ignore_ascii_case(mnemonic) && i.store)
}

/// Decode the dyadic instruction for opcode top byte `top` in mode group `mm`
/// (0–2), or `None` if no dyadic entry matches. `mm = 2` (register) selects the
/// non-store form; `mm = 0`/`1` (memory) prefers the store form, else a
/// memory-capable load form (never the register-only `ADC`/`SBC`).
#[must_use]
pub fn decode_dyadic(top: u8, mm: u8) -> Option<&'static Insn> {
    let op = (top >> 1) & 0x1F;
    let byte = top & 1 == 0;
    if mm == 2 {
        INSTRUCTIONS
            .iter()
            .find(|i| i.op == op && i.byte == byte && !i.store)
    } else {
        INSTRUCTIONS
            .iter()
            .find(|i| i.op == op && i.byte == byte && i.store)
            .or_else(|| {
                INSTRUCTIONS
                    .iter()
                    .find(|i| i.op == op && i.byte == byte && !i.store && !i.reg_only)
            })
    }
}

/// Minimal set for the `Dialect` trait — the Z8000 dialect encodes through the
/// computed-operand seam, so only the (big-endian) endianness is load-bearing.
pub const SET: InstructionSet = InstructionSet {
    cpu: "Zilog Z8000",
    endianness: Endianness::Big,
    instructions: &[],
};

use Class::Dyadic;

/// Increment 1: the dyadic family — the source-into-register arithmetic / logic
/// / load ops and their byte forms, the register-only `ADC`/`SBC`, and the `LD`
/// store forms.
pub const INSTRUCTIONS: &[Insn] = &[
    Insn {
        mnemonic: "ADD",
        op: 0x00,
        byte: false,
        reg_only: false,
        store: false,
        class: Dyadic,
        summary: "Add",
    },
    Insn {
        mnemonic: "ADDB",
        op: 0x00,
        byte: true,
        reg_only: false,
        store: false,
        class: Dyadic,
        summary: "Add byte",
    },
    Insn {
        mnemonic: "SUB",
        op: 0x01,
        byte: false,
        reg_only: false,
        store: false,
        class: Dyadic,
        summary: "Subtract",
    },
    Insn {
        mnemonic: "SUBB",
        op: 0x01,
        byte: true,
        reg_only: false,
        store: false,
        class: Dyadic,
        summary: "Subtract byte",
    },
    Insn {
        mnemonic: "OR",
        op: 0x02,
        byte: false,
        reg_only: false,
        store: false,
        class: Dyadic,
        summary: "Or",
    },
    Insn {
        mnemonic: "ORB",
        op: 0x02,
        byte: true,
        reg_only: false,
        store: false,
        class: Dyadic,
        summary: "Or byte",
    },
    Insn {
        mnemonic: "AND",
        op: 0x03,
        byte: false,
        reg_only: false,
        store: false,
        class: Dyadic,
        summary: "And",
    },
    Insn {
        mnemonic: "ANDB",
        op: 0x03,
        byte: true,
        reg_only: false,
        store: false,
        class: Dyadic,
        summary: "And byte",
    },
    Insn {
        mnemonic: "XOR",
        op: 0x04,
        byte: false,
        reg_only: false,
        store: false,
        class: Dyadic,
        summary: "Exclusive or",
    },
    Insn {
        mnemonic: "XORB",
        op: 0x04,
        byte: true,
        reg_only: false,
        store: false,
        class: Dyadic,
        summary: "Exclusive or byte",
    },
    Insn {
        mnemonic: "CP",
        op: 0x05,
        byte: false,
        reg_only: false,
        store: false,
        class: Dyadic,
        summary: "Compare",
    },
    Insn {
        mnemonic: "CPB",
        op: 0x05,
        byte: true,
        reg_only: false,
        store: false,
        class: Dyadic,
        summary: "Compare byte",
    },
    Insn {
        mnemonic: "LD",
        op: 0x10,
        byte: false,
        reg_only: false,
        store: false,
        class: Dyadic,
        summary: "Load",
    },
    Insn {
        mnemonic: "LDB",
        op: 0x10,
        byte: true,
        reg_only: false,
        store: false,
        class: Dyadic,
        summary: "Load byte",
    },
    Insn {
        mnemonic: "ADC",
        op: 0x1A,
        byte: false,
        reg_only: true,
        store: false,
        class: Dyadic,
        summary: "Add with carry",
    },
    Insn {
        mnemonic: "ADCB",
        op: 0x1A,
        byte: true,
        reg_only: true,
        store: false,
        class: Dyadic,
        summary: "Add with carry byte",
    },
    Insn {
        mnemonic: "SBC",
        op: 0x1B,
        byte: false,
        reg_only: true,
        store: false,
        class: Dyadic,
        summary: "Subtract with carry",
    },
    Insn {
        mnemonic: "SBCB",
        op: 0x1B,
        byte: true,
        reg_only: true,
        store: false,
        class: Dyadic,
        summary: "Subtract with carry byte",
    },
    // LD store forms (register → memory), operation 0x17 in the memory groups.
    Insn {
        mnemonic: "LD",
        op: 0x17,
        byte: false,
        reg_only: false,
        store: true,
        class: Dyadic,
        summary: "Load (store to memory)",
    },
    Insn {
        mnemonic: "LDB",
        op: 0x17,
        byte: true,
        reg_only: false,
        store: true,
        class: Dyadic,
        summary: "Load byte (store to memory)",
    },
];
