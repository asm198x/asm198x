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
//! ## The dyadic family (increments 1–2)
//!
//! The arithmetic / logic / load / exchange / load-address family shares one
//! first-word shape:
//!
//! ```text
//!   MM bbbbbb   ssss dddd
//! ```
//!
//! `MM` (bits 15–14) is the addressing-mode group and `bbbbbb` ([`Insn::base6`])
//! the mode-independent opcode: a form's top byte is `MM << 6 | base6`, so the
//! same op is `base6` (IR / IM), `0x40 | base6` (DA / X), and `0x80 | base6`
//! (R). Within a group the **source field** (`ssss`) picks the exact mode — zero
//! selects immediate (IM) over indirect (IR), and direct (DA) over indexed (X).
//! The second byte is `source-field << 4 | destination-register`. `LD` also has
//! **store** forms (register → memory), a distinct `base6` in the IR / DA / X
//! groups with the pointer/index in the high nibble and the source register in
//! the low. Byte immediates replicate the byte into both halves of the word;
//! long immediates are 32 bits (two words).
//!
//! Every opcode is validated byte-for-byte against `asl` (`cpu Z8002`).

use crate::{Endianness, InstructionSet};

/// The field layout of an instruction group. Grows per increment.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    /// The dyadic arithmetic / logic / load / exchange / load-address family.
    Dyadic,
}

// ---------------------------------------------------------------------------
// Program control (increment 3): JP / CALL / JR / RET / DJNZ / CALR
// ---------------------------------------------------------------------------

/// A program-control instruction's shape.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CtlKind {
    /// `JP cc, dst` / `CALL dst` — a memory operand in IR / DA / X, the low
    /// nibble a condition code (`JP`) or zero (`CALL`).
    Jump,
    /// `JR cc, addr` — `0xE0 | cc` then a word-scaled signed 8-bit PC offset
    /// (`target = PC + 2·disp`).
    Jr,
    /// `RET cc` — `0x9E00 | cc`.
    Ret,
    /// `DJNZ r, addr` / `DBJNZ rb, addr` — `0xF0 | reg`, then `w` and a 7-bit
    /// **backward** word offset (`target = PC − 2·disp`).
    Djnz,
    /// `CALR addr` — `0xD0..` a 12-bit **backward** word offset
    /// (`target = PC − 2·disp`).
    Calr,
}

/// One program-control mnemonic.
pub struct Ctl {
    pub mnemonic: &'static str,
    pub kind: CtlKind,
    /// The base opcode (top byte for `Jr`/`Djnz`/`Calr`, full word for `Ret`,
    /// the `MM`-independent `base6` for `Jump`).
    pub base: u16,
    /// Allowed addressing modes ([`Jump`](CtlKind::Jump) only).
    pub modes: u8,
    /// Carries a condition code in its low nibble (`JP`, `JR`, `RET`).
    pub cc: bool,
    /// Byte form (`DBJNZ`).
    pub byte: bool,
    pub summary: &'static str,
}

use CtlKind::{Calr, Djnz, Jr, Jump, Ret};

/// The program-control instructions (increment 3).
pub const CONTROL: &[Ctl] = &[
    Ctl {
        mnemonic: "JP",
        kind: Jump,
        base: 0x1E,
        modes: IR | DA | X,
        cc: true,
        byte: false,
        summary: "Jump",
    },
    Ctl {
        mnemonic: "CALL",
        kind: Jump,
        base: 0x1F,
        modes: IR | DA | X,
        cc: false,
        byte: false,
        summary: "Call",
    },
    Ctl {
        mnemonic: "JR",
        kind: Jr,
        base: 0xE0,
        modes: 0,
        cc: true,
        byte: false,
        summary: "Jump relative",
    },
    Ctl {
        mnemonic: "RET",
        kind: Ret,
        base: 0x9E00,
        modes: 0,
        cc: true,
        byte: false,
        summary: "Return",
    },
    Ctl {
        mnemonic: "DJNZ",
        kind: Djnz,
        base: 0xF0,
        modes: 0,
        cc: false,
        byte: false,
        summary: "Decrement and jump if not zero",
    },
    Ctl {
        mnemonic: "DBJNZ",
        kind: Djnz,
        base: 0xF0,
        modes: 0,
        cc: false,
        byte: true,
        summary: "Decrement byte and jump if not zero",
    },
    Ctl {
        mnemonic: "CALR",
        kind: Calr,
        base: 0xD0,
        modes: 0,
        cc: false,
        byte: false,
        summary: "Call relative",
    },
];

/// Find a program-control instruction by mnemonic (case-insensitive).
#[must_use]
pub fn ctl_lookup(mnemonic: &str) -> Option<&'static Ctl> {
    CONTROL
        .iter()
        .find(|c| c.mnemonic.eq_ignore_ascii_case(mnemonic))
}

/// The 4-bit value of a condition-code mnemonic (case-insensitive), or `None`.
#[must_use]
pub fn cc_value(name: &str) -> Option<u8> {
    CC.iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|&(_, v)| v)
}

/// The canonical condition-code mnemonic for a 4-bit value, or `None` for the
/// always-true code 8 (rendered as no condition).
#[must_use]
pub fn cc_name(v: u8) -> Option<&'static str> {
    match v {
        8 => None,
        _ => CC.iter().find(|(_, val)| *val == v).map(|&(n, _)| n),
    }
}

/// Condition codes: canonical name first for each value, aliases after.
const CC: &[(&str, u8)] = &[
    ("f", 0),
    ("lt", 1),
    ("le", 2),
    ("ule", 3),
    ("ov", 4),
    ("mi", 5),
    ("eq", 6),
    ("c", 7),
    ("ge", 9),
    ("gt", 10),
    ("ugt", 11),
    ("nov", 12),
    ("pl", 13),
    ("ne", 14),
    ("nc", 15),
    // aliases
    ("pe", 4),
    ("z", 6),
    ("ult", 7),
    ("po", 12),
    ("nz", 14),
    ("uge", 15),
];

/// Operand size, which fixes register naming and immediate width.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Size {
    /// Byte registers `rh`/`rl`; a byte immediate replicated into a word.
    Byte,
    /// Word registers `r0`–`r15`; a 16-bit immediate.
    Word,
    /// Long register pairs `rr0`–`rr14`; a 32-bit immediate.
    Long,
    /// An effective address into a word register (`LDA`); no immediate.
    Address,
}

/// Addressing-mode bits for [`Insn::modes`].
pub const IM: u8 = 1; // immediate #n
pub const IR: u8 = 2; // indirect register @Rn
pub const DA: u8 = 4; // direct address
pub const X: u8 = 8; // indexed addr(Rn)
pub const R: u8 = 16; // register

/// One Z8000 mnemonic in the dyadic family.
pub struct Insn {
    pub mnemonic: &'static str,
    /// The mode-independent low 6 bits of the opcode top byte.
    pub base6: u8,
    pub size: Size,
    /// Bitmask of the addressing modes this entry allows ([`IM`]…[`R`]).
    pub modes: u8,
    /// A register → memory store form (`LD`/`LDB`/`LDL` with a memory
    /// destination); the source register is the second byte's low nibble.
    pub store: bool,
    pub class: Class,
    pub summary: &'static str,
}

/// The addressing-mode bit for group `mm` (0–2) given the source field `ssss`.
#[must_use]
pub fn mode_of(mm: u8, field: u16) -> u8 {
    match mm {
        0 => {
            if field == 0 {
                IM
            } else {
                IR
            }
        }
        1 => {
            if field == 0 {
                DA
            } else {
                X
            }
        }
        2 => R,
        _ => 0,
    }
}

/// Find the (first) non-store instruction entry for a mnemonic. `LD`/`LDB`/`LDL`
/// resolve to their **load** entry; the dialect selects the store entry itself
/// when the destination is memory.
#[must_use]
pub fn lookup(mnemonic: &str) -> Option<&'static Insn> {
    INSTRUCTIONS
        .iter()
        .find(|i| i.mnemonic.eq_ignore_ascii_case(mnemonic) && !i.store)
}

/// The store entry for a mnemonic (register → memory).
#[must_use]
pub fn store_entry(mnemonic: &str) -> Option<&'static Insn> {
    INSTRUCTIONS
        .iter()
        .find(|i| i.mnemonic.eq_ignore_ascii_case(mnemonic) && i.store)
}

/// Decode the dyadic instruction for opcode top byte `top` given the source
/// field `field`, or `None` if no entry covers that (base, mode) — in which case
/// the byte is data this increment doesn't yet decode.
#[must_use]
pub fn decode(top: u8, field: u16) -> Option<&'static Insn> {
    let base6 = top & 0x3F;
    let mode = mode_of(top >> 6, field);
    INSTRUCTIONS
        .iter()
        .find(|i| i.base6 == base6 && i.modes & mode != 0)
}

/// Minimal set for the `Dialect` trait — the Z8000 dialect encodes through the
/// computed-operand seam, so only the (big-endian) endianness is load-bearing.
pub const SET: InstructionSet = InstructionSet {
    cpu: "Zilog Z8000",
    endianness: Endianness::Big,
    instructions: &[],
};

use Class::Dyadic;
use Size::{Address, Byte, Long, Word};

const MEM: u8 = IR | DA | X; // store destinations
const ALL: u8 = IM | IR | DA | X | R; // full source set

/// The dyadic family (increments 1–2). `base6` is the mode-independent opcode:
/// for a word op it is `0x40 | op<<1 | 1`-style bits, but the table just carries
/// the value verified against `asl`, so no derivation is needed here.
pub const INSTRUCTIONS: &[Insn] = &[
    // --- Increment 1: word / byte arithmetic, logic, compare, load ---------
    Insn {
        mnemonic: "ADD",
        base6: 0x01,
        size: Word,
        modes: ALL,
        store: false,
        class: Dyadic,
        summary: "Add",
    },
    Insn {
        mnemonic: "ADDB",
        base6: 0x00,
        size: Byte,
        modes: ALL,
        store: false,
        class: Dyadic,
        summary: "Add byte",
    },
    Insn {
        mnemonic: "SUB",
        base6: 0x03,
        size: Word,
        modes: ALL,
        store: false,
        class: Dyadic,
        summary: "Subtract",
    },
    Insn {
        mnemonic: "SUBB",
        base6: 0x02,
        size: Byte,
        modes: ALL,
        store: false,
        class: Dyadic,
        summary: "Subtract byte",
    },
    Insn {
        mnemonic: "OR",
        base6: 0x05,
        size: Word,
        modes: ALL,
        store: false,
        class: Dyadic,
        summary: "Or",
    },
    Insn {
        mnemonic: "ORB",
        base6: 0x04,
        size: Byte,
        modes: ALL,
        store: false,
        class: Dyadic,
        summary: "Or byte",
    },
    Insn {
        mnemonic: "AND",
        base6: 0x07,
        size: Word,
        modes: ALL,
        store: false,
        class: Dyadic,
        summary: "And",
    },
    Insn {
        mnemonic: "ANDB",
        base6: 0x06,
        size: Byte,
        modes: ALL,
        store: false,
        class: Dyadic,
        summary: "And byte",
    },
    Insn {
        mnemonic: "XOR",
        base6: 0x09,
        size: Word,
        modes: ALL,
        store: false,
        class: Dyadic,
        summary: "Exclusive or",
    },
    Insn {
        mnemonic: "XORB",
        base6: 0x08,
        size: Byte,
        modes: ALL,
        store: false,
        class: Dyadic,
        summary: "Exclusive or byte",
    },
    Insn {
        mnemonic: "CP",
        base6: 0x0B,
        size: Word,
        modes: ALL,
        store: false,
        class: Dyadic,
        summary: "Compare",
    },
    Insn {
        mnemonic: "CPB",
        base6: 0x0A,
        size: Byte,
        modes: ALL,
        store: false,
        class: Dyadic,
        summary: "Compare byte",
    },
    Insn {
        mnemonic: "LD",
        base6: 0x21,
        size: Word,
        modes: ALL,
        store: false,
        class: Dyadic,
        summary: "Load",
    },
    Insn {
        mnemonic: "LDB",
        base6: 0x20,
        size: Byte,
        modes: ALL,
        store: false,
        class: Dyadic,
        summary: "Load byte",
    },
    Insn {
        mnemonic: "ADC",
        base6: 0x35,
        size: Word,
        modes: R,
        store: false,
        class: Dyadic,
        summary: "Add with carry",
    },
    Insn {
        mnemonic: "ADCB",
        base6: 0x34,
        size: Byte,
        modes: R,
        store: false,
        class: Dyadic,
        summary: "Add with carry byte",
    },
    Insn {
        mnemonic: "SBC",
        base6: 0x37,
        size: Word,
        modes: R,
        store: false,
        class: Dyadic,
        summary: "Subtract with carry",
    },
    Insn {
        mnemonic: "SBCB",
        base6: 0x36,
        size: Byte,
        modes: R,
        store: false,
        class: Dyadic,
        summary: "Subtract with carry byte",
    },
    Insn {
        mnemonic: "LD",
        base6: 0x2F,
        size: Word,
        modes: MEM,
        store: true,
        class: Dyadic,
        summary: "Load (store to memory)",
    },
    Insn {
        mnemonic: "LDB",
        base6: 0x2E,
        size: Byte,
        modes: MEM,
        store: true,
        class: Dyadic,
        summary: "Load byte (store to memory)",
    },
    // --- Increment 2: long arithmetic/load, exchange, load address ---------
    Insn {
        mnemonic: "CPL",
        base6: 0x10,
        size: Long,
        modes: ALL,
        store: false,
        class: Dyadic,
        summary: "Compare long",
    },
    Insn {
        mnemonic: "SUBL",
        base6: 0x12,
        size: Long,
        modes: ALL,
        store: false,
        class: Dyadic,
        summary: "Subtract long",
    },
    Insn {
        mnemonic: "LDL",
        base6: 0x14,
        size: Long,
        modes: ALL,
        store: false,
        class: Dyadic,
        summary: "Load long",
    },
    Insn {
        mnemonic: "ADDL",
        base6: 0x16,
        size: Long,
        modes: ALL,
        store: false,
        class: Dyadic,
        summary: "Add long",
    },
    Insn {
        mnemonic: "LDL",
        base6: 0x1D,
        size: Long,
        modes: MEM,
        store: true,
        class: Dyadic,
        summary: "Load long (store to memory)",
    },
    Insn {
        mnemonic: "EX",
        base6: 0x2D,
        size: Word,
        modes: IR | DA | X | R,
        store: false,
        class: Dyadic,
        summary: "Exchange",
    },
    Insn {
        mnemonic: "EXB",
        base6: 0x2C,
        size: Byte,
        modes: IR | DA | X | R,
        store: false,
        class: Dyadic,
        summary: "Exchange byte",
    },
    // `base6` 0x36 is shared with SBCB, disambiguated by mode (LDA is DA/X only).
    Insn {
        mnemonic: "LDA",
        base6: 0x36,
        size: Address,
        modes: DA | X,
        store: false,
        class: Dyadic,
        summary: "Load address",
    },
];
