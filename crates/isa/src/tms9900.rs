//! TI TMS9900 instruction set — the 16-bit processor at the heart of the
//! **TI-99/4A** and one of the first single-chip 16-bit CPUs. **Big-endian**;
//! every instruction is one 16-bit word plus 0–2 extension words.
//!
//! Like [`crate::pdp11`], the TMS9900 packs its operands as **fields inside the
//! opcode word**, so this module is a **bespoke table** (one [`Insn`] per
//! mnemonic: base opcode + a [`Class`] fixing the field layout), keyed by both
//! the dialect and a field-based disassembler — not the [`crate::Form`] model.
//!
//! Its defining trait is the **workspace-register model**: the sixteen general
//! registers `R0`–`R15` live in RAM, pointed to by the workspace pointer `WP`.
//! Most operands use the shared **general addressing** T-field: workspace
//! register `Rn` (mode 0), indirect `*Rn` (1), symbolic `@addr` / indexed
//! `@addr(Rn)` (mode 2 — `Rn = 0` is symbolic, else indexed; `R0` cannot index),
//! and autoincrement `*Rn+` (3). A symbolic/indexed operand appends one
//! **absolute** address word (no PC-relative form — only the jumps are relative).
//!
//! The nine instruction formats map onto the [`Class`] variants. Scope is the
//! base TMS9900 integer set (the TI-99/4A CPU); the TMS9995 / TMS99105
//! supersets are out of scope. Every base opcode is validated byte-for-byte
//! against `asl` (`cpu TMS9900`) — see `crates/asm198x/tests/conformance.rs`.

use crate::{Endianness, InstructionSet};

/// The field layout of an instruction — one per TMS9900 instruction format.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    /// Format I — two general operands: `base | Td<<10 | D<<6 | Ts<<4 | S`.
    /// `MOV`, `A`, `C`, `S`, `SOC`, `SZC` and byte forms.
    DualGeneral,
    /// Format II jump — word-scaled PC-relative 8-bit offset: `base | (off & 0xFF)`,
    /// target `= PC + 2 + 2·off`. `JMP`, `JEQ`, …
    Jump,
    /// Format II CRU-bit — signed 8-bit CRU offset (not PC-relative):
    /// `base | (disp & 0xFF)`. `SBO`, `SBZ`, `TB`.
    Cru,
    /// Format III / IX — a general source + a register destination:
    /// `base | D<<6 | Ts<<4 | S`; syntax `src, Rd`. `COC`, `CZC`, `XOR`, `MPY`,
    /// `DIV`.
    DualRegDst,
    /// Format IX `XOP` — a general source + a 0–15 XOP number, same layout as
    /// [`Class::DualRegDst`]; syntax `src, n`.
    Xop,
    /// Format IV — a general source + a 1–16 CRU bit count (16 encodes as 0):
    /// `base | (count & 0xF)<<6 | Ts<<4 | S`; syntax `src, count`. `LDCR`, `STCR`.
    CruMulti,
    /// Format V — a workspace register + a 0–15 shift count (0 = count in `R0`):
    /// `base | count<<4 | W`; syntax `Rw, count`. `SLA`, `SRA`, `SRC`, `SRL`.
    Shift,
    /// Format VI — one general operand: `base | Ts<<4 | S`. `B`, `BL`, `BLWP`,
    /// `CLR`, `INC`, `X`, …
    SingleGeneral,
    /// Format VII — no operand, fixed opcode. `IDLE`, `RTWP`, …
    Control,
    /// Format VIII — a workspace register + a 16-bit immediate word:
    /// `base | W`, then the immediate. `LI`, `AI`, `ANDI`, `ORI`, `CI`.
    ImmReg,
    /// Format VIII — a 16-bit immediate word only: `base`, then the immediate.
    /// `LWPI`, `LIMI`.
    ImmOnly,
    /// Format VIII — a workspace register only: `base | W`. `STST`, `STWP`.
    StoreReg,
}

/// One TMS9900 mnemonic: its base opcode (operand fields zero) and [`Class`].
pub struct Insn {
    pub mnemonic: &'static str,
    pub base: u16,
    pub class: Class,
    pub summary: &'static str,
}

impl Class {
    /// The fixed (non-field) bits of the opcode word for this class — masking a
    /// word with it yields the `base` to look up. [`decode`] tries classes from
    /// the widest mask to the narrowest to disambiguate shared opcode space.
    #[must_use]
    pub const fn mask(self) -> u16 {
        match self {
            Class::Control | Class::ImmOnly => 0xFFFF,
            Class::ImmReg | Class::StoreReg => 0xFFF0,
            Class::SingleGeneral => 0xFFC0,
            Class::Shift | Class::Jump | Class::Cru => 0xFF00,
            Class::DualRegDst | Class::Xop | Class::CruMulti => 0xFC00,
            Class::DualGeneral => 0xF000,
        }
    }
}

/// Find an instruction by mnemonic (case-insensitive).
#[must_use]
pub fn lookup(mnemonic: &str) -> Option<&'static Insn> {
    INSTRUCTIONS
        .iter()
        .find(|i| i.mnemonic.eq_ignore_ascii_case(mnemonic))
}

/// Identify the instruction encoded in `word`. Classes are tried from the widest
/// fixed-bit mask to the narrowest so a more-specific opcode is matched before a
/// broader one sharing the region; within a mask width the regions are disjoint,
/// so table membership resolves the rest.
#[must_use]
pub fn decode(word: u16) -> Option<&'static Insn> {
    const ORDER: &[Class] = &[
        Class::Control,
        Class::ImmOnly,
        Class::ImmReg,
        Class::StoreReg,
        Class::SingleGeneral,
        Class::Shift,
        Class::Jump,
        Class::Cru,
        Class::DualRegDst,
        Class::Xop,
        Class::CruMulti,
        Class::DualGeneral,
    ];
    for &class in ORDER {
        let base = word & class.mask();
        if let Some(insn) = INSTRUCTIONS
            .iter()
            .find(|i| i.class == class && i.base == base)
        {
            return Some(insn);
        }
    }
    None
}

/// Minimal set for the `Dialect` trait — the TMS9900 dialect encodes through the
/// computed-operand seam, not [`Form`]s, so only the endianness is load-bearing.
pub const SET: InstructionSet = InstructionSet {
    cpu: "TI TMS9900",
    endianness: Endianness::Big,
    instructions: &[],
};

use Class::{
    Control, Cru, CruMulti, DualGeneral, DualRegDst, ImmOnly, ImmReg, Jump, Shift, SingleGeneral,
    StoreReg, Xop,
};

/// Every base-9900 mnemonic, opcode validated against `asl` (`cpu TMS9900`).
pub const INSTRUCTIONS: &[Insn] = &[
    // --- Format VIII: immediate + store ------------------------------------
    Insn {
        mnemonic: "LI",
        base: 0x0200,
        class: ImmReg,
        summary: "Load immediate",
    },
    Insn {
        mnemonic: "AI",
        base: 0x0220,
        class: ImmReg,
        summary: "Add immediate",
    },
    Insn {
        mnemonic: "ANDI",
        base: 0x0240,
        class: ImmReg,
        summary: "AND immediate",
    },
    Insn {
        mnemonic: "ORI",
        base: 0x0260,
        class: ImmReg,
        summary: "OR immediate",
    },
    Insn {
        mnemonic: "CI",
        base: 0x0280,
        class: ImmReg,
        summary: "Compare immediate",
    },
    Insn {
        mnemonic: "STWP",
        base: 0x02A0,
        class: StoreReg,
        summary: "Store workspace pointer",
    },
    Insn {
        mnemonic: "STST",
        base: 0x02C0,
        class: StoreReg,
        summary: "Store status register",
    },
    Insn {
        mnemonic: "LWPI",
        base: 0x02E0,
        class: ImmOnly,
        summary: "Load workspace pointer immediate",
    },
    Insn {
        mnemonic: "LIMI",
        base: 0x0300,
        class: ImmOnly,
        summary: "Load interrupt mask immediate",
    },
    // --- Format VII: control (no operand) ----------------------------------
    Insn {
        mnemonic: "IDLE",
        base: 0x0340,
        class: Control,
        summary: "Idle",
    },
    Insn {
        mnemonic: "RSET",
        base: 0x0360,
        class: Control,
        summary: "Reset",
    },
    Insn {
        mnemonic: "RTWP",
        base: 0x0380,
        class: Control,
        summary: "Return with workspace pointer",
    },
    Insn {
        mnemonic: "CKON",
        base: 0x03A0,
        class: Control,
        summary: "Clock on",
    },
    Insn {
        mnemonic: "CKOF",
        base: 0x03C0,
        class: Control,
        summary: "Clock off",
    },
    Insn {
        mnemonic: "LREX",
        base: 0x03E0,
        class: Control,
        summary: "Load or restart execution",
    },
    // --- Format VI: single general operand ---------------------------------
    Insn {
        mnemonic: "BLWP",
        base: 0x0400,
        class: SingleGeneral,
        summary: "Branch and load workspace pointer",
    },
    Insn {
        mnemonic: "B",
        base: 0x0440,
        class: SingleGeneral,
        summary: "Branch",
    },
    Insn {
        mnemonic: "X",
        base: 0x0480,
        class: SingleGeneral,
        summary: "Execute",
    },
    Insn {
        mnemonic: "CLR",
        base: 0x04C0,
        class: SingleGeneral,
        summary: "Clear",
    },
    Insn {
        mnemonic: "NEG",
        base: 0x0500,
        class: SingleGeneral,
        summary: "Negate",
    },
    Insn {
        mnemonic: "INV",
        base: 0x0540,
        class: SingleGeneral,
        summary: "Invert",
    },
    Insn {
        mnemonic: "INC",
        base: 0x0580,
        class: SingleGeneral,
        summary: "Increment",
    },
    Insn {
        mnemonic: "INCT",
        base: 0x05C0,
        class: SingleGeneral,
        summary: "Increment by two",
    },
    Insn {
        mnemonic: "DEC",
        base: 0x0600,
        class: SingleGeneral,
        summary: "Decrement",
    },
    Insn {
        mnemonic: "DECT",
        base: 0x0640,
        class: SingleGeneral,
        summary: "Decrement by two",
    },
    Insn {
        mnemonic: "BL",
        base: 0x0680,
        class: SingleGeneral,
        summary: "Branch and link",
    },
    Insn {
        mnemonic: "SWPB",
        base: 0x06C0,
        class: SingleGeneral,
        summary: "Swap bytes",
    },
    Insn {
        mnemonic: "SETO",
        base: 0x0700,
        class: SingleGeneral,
        summary: "Set to ones",
    },
    Insn {
        mnemonic: "ABS",
        base: 0x0740,
        class: SingleGeneral,
        summary: "Absolute value",
    },
    // --- Format V: shift ---------------------------------------------------
    Insn {
        mnemonic: "SRA",
        base: 0x0800,
        class: Shift,
        summary: "Shift right arithmetic",
    },
    Insn {
        mnemonic: "SRL",
        base: 0x0900,
        class: Shift,
        summary: "Shift right logical",
    },
    Insn {
        mnemonic: "SLA",
        base: 0x0A00,
        class: Shift,
        summary: "Shift left arithmetic",
    },
    Insn {
        mnemonic: "SRC",
        base: 0x0B00,
        class: Shift,
        summary: "Shift right circular",
    },
    // --- Format II: jumps + CRU bit ----------------------------------------
    Insn {
        mnemonic: "JMP",
        base: 0x1000,
        class: Jump,
        summary: "Jump unconditional",
    },
    Insn {
        mnemonic: "JLT",
        base: 0x1100,
        class: Jump,
        summary: "Jump if less than",
    },
    Insn {
        mnemonic: "JLE",
        base: 0x1200,
        class: Jump,
        summary: "Jump if low or equal",
    },
    Insn {
        mnemonic: "JEQ",
        base: 0x1300,
        class: Jump,
        summary: "Jump if equal",
    },
    Insn {
        mnemonic: "JHE",
        base: 0x1400,
        class: Jump,
        summary: "Jump if high or equal",
    },
    Insn {
        mnemonic: "JGT",
        base: 0x1500,
        class: Jump,
        summary: "Jump if greater than",
    },
    Insn {
        mnemonic: "JNE",
        base: 0x1600,
        class: Jump,
        summary: "Jump if not equal",
    },
    Insn {
        mnemonic: "JNC",
        base: 0x1700,
        class: Jump,
        summary: "Jump if no carry",
    },
    Insn {
        mnemonic: "JOC",
        base: 0x1800,
        class: Jump,
        summary: "Jump on carry",
    },
    Insn {
        mnemonic: "JNO",
        base: 0x1900,
        class: Jump,
        summary: "Jump if no overflow",
    },
    Insn {
        mnemonic: "JL",
        base: 0x1A00,
        class: Jump,
        summary: "Jump if logical low",
    },
    Insn {
        mnemonic: "JH",
        base: 0x1B00,
        class: Jump,
        summary: "Jump if logical high",
    },
    Insn {
        mnemonic: "JOP",
        base: 0x1C00,
        class: Jump,
        summary: "Jump if odd parity",
    },
    Insn {
        mnemonic: "SBO",
        base: 0x1D00,
        class: Cru,
        summary: "Set CRU bit to one",
    },
    Insn {
        mnemonic: "SBZ",
        base: 0x1E00,
        class: Cru,
        summary: "Set CRU bit to zero",
    },
    Insn {
        mnemonic: "TB",
        base: 0x1F00,
        class: Cru,
        summary: "Test CRU bit",
    },
    // --- Format III / IX: general source + register/XOP --------------------
    Insn {
        mnemonic: "COC",
        base: 0x2000,
        class: DualRegDst,
        summary: "Compare ones corresponding",
    },
    Insn {
        mnemonic: "CZC",
        base: 0x2400,
        class: DualRegDst,
        summary: "Compare zeros corresponding",
    },
    Insn {
        mnemonic: "XOR",
        base: 0x2800,
        class: DualRegDst,
        summary: "Exclusive or",
    },
    Insn {
        mnemonic: "XOP",
        base: 0x2C00,
        class: Xop,
        summary: "Extended operation",
    },
    Insn {
        mnemonic: "MPY",
        base: 0x3800,
        class: DualRegDst,
        summary: "Multiply",
    },
    Insn {
        mnemonic: "DIV",
        base: 0x3C00,
        class: DualRegDst,
        summary: "Divide",
    },
    // --- Format IV: CRU multi-bit ------------------------------------------
    Insn {
        mnemonic: "LDCR",
        base: 0x3000,
        class: CruMulti,
        summary: "Load CRU",
    },
    Insn {
        mnemonic: "STCR",
        base: 0x3400,
        class: CruMulti,
        summary: "Store CRU",
    },
    // --- Format I: dual general operand ------------------------------------
    Insn {
        mnemonic: "SZC",
        base: 0x4000,
        class: DualGeneral,
        summary: "Set zeros corresponding",
    },
    Insn {
        mnemonic: "SZCB",
        base: 0x5000,
        class: DualGeneral,
        summary: "Set zeros corresponding, byte",
    },
    Insn {
        mnemonic: "S",
        base: 0x6000,
        class: DualGeneral,
        summary: "Subtract",
    },
    Insn {
        mnemonic: "SB",
        base: 0x7000,
        class: DualGeneral,
        summary: "Subtract byte",
    },
    Insn {
        mnemonic: "C",
        base: 0x8000,
        class: DualGeneral,
        summary: "Compare",
    },
    Insn {
        mnemonic: "CB",
        base: 0x9000,
        class: DualGeneral,
        summary: "Compare byte",
    },
    Insn {
        mnemonic: "A",
        base: 0xA000,
        class: DualGeneral,
        summary: "Add",
    },
    Insn {
        mnemonic: "AB",
        base: 0xB000,
        class: DualGeneral,
        summary: "Add byte",
    },
    Insn {
        mnemonic: "MOV",
        base: 0xC000,
        class: DualGeneral,
        summary: "Move",
    },
    Insn {
        mnemonic: "MOVB",
        base: 0xD000,
        class: DualGeneral,
        summary: "Move byte",
    },
    Insn {
        mnemonic: "SOC",
        base: 0xE000,
        class: DualGeneral,
        summary: "Set ones corresponding",
    },
    Insn {
        mnemonic: "SOCB",
        base: 0xF000,
        class: DualGeneral,
        summary: "Set ones corresponding, byte",
    },
];
