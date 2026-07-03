//! General Instrument **CP1610** instruction set — the 16-bit CPU of the Mattel
//! **Intellivision**. **Big-endian**; every instruction is one or more 10-bit
//! "decle" words, each stored (by `asl` / `p2bin` and the standard ROM image
//! format) as a **big-endian 16-bit word** whose top six bits are zero. So the
//! decle space is `0x000..=0x3FF` and this spec masks accordingly.
//!
//! Like [`crate::tms9900`], the CP1610 packs its operands as **fields inside the
//! opcode word**, so this module is a **bespoke table** (one [`Insn`] per
//! mnemonic: base opcode + a [`Class`] fixing the field layout), keyed by both
//! the dialect and a field-based disassembler — not the [`crate::Form`] model.
//!
//! Scope is built up as sweep-verified increments (see
//! `../../decisions/` in the umbrella and the crate `decisions/`). **Increment 1**
//! covers the single-decle register / implied groups: the control ops, the
//! register-unary arithmetic (`INCR`/`DECR`/`COMR`/`NEGR`/`ADCR`), status
//! transfer (`GSWD`/`RSWD`), and the register-register dyadic group
//! (`MOVR`/`ADDR`/`SUBR`/`CMPR`/`ANDR`/`XORR`). The memory, immediate, shift, and
//! branch groups arrive in later increments. Every base opcode is validated
//! byte-for-byte against `asl` (`cpu CP-1600`) — see
//! `crates/asm198x/tests/conformance.rs`.

use crate::{Endianness, InstructionSet};

/// The field layout of an instruction — one per CP1610 encoding shape covered so
/// far.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    /// Fixed opcode, no operand: `base`. `HLT`, `SDBD`, `EIS`, `DIS`, `TCI`,
    /// `CLRC`, `SETC`, `NOP`, `SIN`.
    Implied,
    /// A single register `R0`–`R7` in the low three bits: `base | reg`. `INCR`,
    /// `DECR`, `COMR`, `NEGR`, `ADCR`, `RSWD`.
    RegUnary,
    /// A single register `R0`–`R3` in the low two bits: `base | reg`. `GSWD`
    /// (its register field is only two bits wide — `R4`–`R7` are not encodable).
    GetStatus,
    /// Two registers — source in bits `5:3`, destination in bits `2:0`:
    /// `base | src << 3 | dst`. `MOVR`, `ADDR`, `SUBR`, `CMPR`, `ANDR`, `XORR`.
    RegReg,
}

/// One CP1610 mnemonic: its base opcode (operand fields zero) and [`Class`].
pub struct Insn {
    pub mnemonic: &'static str,
    pub base: u16,
    pub class: Class,
    pub summary: &'static str,
}

impl Class {
    /// The fixed (non-field) bits of the opcode word for this class — masking a
    /// (10-bit) word with it yields the `base` to look up. [`decode`] tries
    /// classes from the widest mask to the narrowest to disambiguate shared
    /// opcode space.
    #[must_use]
    pub const fn mask(self) -> u16 {
        match self {
            Class::Implied => 0x3FF,
            Class::GetStatus => 0x3FC,
            Class::RegUnary => 0x3F8,
            Class::RegReg => 0x3C0,
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

/// Identify the instruction encoded in `word` (a 10-bit decle). Classes are tried
/// from the widest fixed-bit mask to the narrowest so a more-specific opcode
/// (e.g. the fixed `NOP` at `0x034`) is matched before a broader one sharing the
/// region (`GSWD`'s `0x030` block). Returns `None` for a word outside the decle
/// space or one no table entry claims.
#[must_use]
pub fn decode(word: u16) -> Option<&'static Insn> {
    if word > 0x3FF {
        return None; // not a valid 10-bit decle
    }
    const ORDER: &[Class] = &[
        Class::Implied,
        Class::GetStatus,
        Class::RegUnary,
        Class::RegReg,
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

/// Minimal set for the `Dialect` trait — the CP1610 dialect encodes through the
/// computed-operand seam, not [`Form`](crate::Form)s, so only the endianness is
/// load-bearing.
pub const SET: InstructionSet = InstructionSet {
    cpu: "GI CP1610",
    endianness: Endianness::Big,
    instructions: &[],
};

use Class::{GetStatus, Implied, RegReg, RegUnary};

/// Every base opcode covered so far, validated against `asl` (`cpu CP-1600`).
pub const INSTRUCTIONS: &[Insn] = &[
    // --- Implied (fixed opcode, no operand) --------------------------------
    Insn {
        mnemonic: "HLT",
        base: 0x000,
        class: Implied,
        summary: "Halt until interrupt",
    },
    Insn {
        mnemonic: "SDBD",
        base: 0x001,
        class: Implied,
        summary: "Set double-byte-data mode (prefix)",
    },
    Insn {
        mnemonic: "EIS",
        base: 0x002,
        class: Implied,
        summary: "Enable interrupt system",
    },
    Insn {
        mnemonic: "DIS",
        base: 0x003,
        class: Implied,
        summary: "Disable interrupt system",
    },
    Insn {
        mnemonic: "TCI",
        base: 0x005,
        class: Implied,
        summary: "Terminate current interrupt",
    },
    Insn {
        mnemonic: "CLRC",
        base: 0x006,
        class: Implied,
        summary: "Clear carry",
    },
    Insn {
        mnemonic: "SETC",
        base: 0x007,
        class: Implied,
        summary: "Set carry",
    },
    Insn {
        mnemonic: "NOP",
        base: 0x034,
        class: Implied,
        summary: "No operation",
    },
    Insn {
        mnemonic: "SIN",
        base: 0x036,
        class: Implied,
        summary: "Software interrupt (external)",
    },
    // --- Register-unary arithmetic (base | reg, R0–R7) ---------------------
    Insn {
        mnemonic: "INCR",
        base: 0x008,
        class: RegUnary,
        summary: "Increment register",
    },
    Insn {
        mnemonic: "DECR",
        base: 0x010,
        class: RegUnary,
        summary: "Decrement register",
    },
    Insn {
        mnemonic: "COMR",
        base: 0x018,
        class: RegUnary,
        summary: "One's-complement register",
    },
    Insn {
        mnemonic: "NEGR",
        base: 0x020,
        class: RegUnary,
        summary: "Two's-complement (negate) register",
    },
    Insn {
        mnemonic: "ADCR",
        base: 0x028,
        class: RegUnary,
        summary: "Add carry into register",
    },
    Insn {
        mnemonic: "RSWD",
        base: 0x038,
        class: RegUnary,
        summary: "Restore status word from register",
    },
    // --- Status transfer (base | reg, R0–R3 only) --------------------------
    Insn {
        mnemonic: "GSWD",
        base: 0x030,
        class: GetStatus,
        summary: "Get status word to register",
    },
    // --- Register-register dyadic (base | src << 3 | dst, R0–R7) ------------
    Insn {
        mnemonic: "MOVR",
        base: 0x080,
        class: RegReg,
        summary: "Move register to register",
    },
    Insn {
        mnemonic: "ADDR",
        base: 0x0C0,
        class: RegReg,
        summary: "Add register to register",
    },
    Insn {
        mnemonic: "SUBR",
        base: 0x100,
        class: RegReg,
        summary: "Subtract register from register",
    },
    Insn {
        mnemonic: "CMPR",
        base: 0x140,
        class: RegReg,
        summary: "Compare registers",
    },
    Insn {
        mnemonic: "ANDR",
        base: 0x180,
        class: RegReg,
        summary: "AND register with register",
    },
    Insn {
        mnemonic: "XORR",
        base: 0x1C0,
        class: RegReg,
        summary: "XOR register with register",
    },
];
