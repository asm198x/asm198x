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

// ---------------------------------------------------------------------------
// Single-operand ALU (increment 4): CLR / COM / NEG / TEST / TSET / INC / DEC
// ---------------------------------------------------------------------------

/// A single-general-operand instruction. Its first word is
/// `MM base6 | field << 4 | low`: the operand register/pointer/index is the
/// **high** nibble of the second byte, and the **low** nibble is either a fixed
/// sub-opcode (`CLR`/`COM`/…) or `count − 1` (`INC`/`DEC`, count 1–16). The
/// operand uses R / IR / DA / X addressing (no immediate).
pub struct Mono {
    pub mnemonic: &'static str,
    pub base6: u8,
    /// Fixed low nibble for the sub-opcode ops; ignored when `count`.
    pub subop: u8,
    pub size: Size,
    /// The low nibble is a `count − 1` operand (`INC`/`DEC`), not a sub-opcode.
    pub count: bool,
    pub summary: &'static str,
}

/// The single-operand ALU instructions (increment 4).
pub const MONO: &[Mono] = &[
    Mono {
        mnemonic: "COM",
        base6: 0x0D,
        subop: 0,
        size: Size::Word,
        count: false,
        summary: "Complement",
    },
    Mono {
        mnemonic: "COMB",
        base6: 0x0C,
        subop: 0,
        size: Size::Byte,
        count: false,
        summary: "Complement byte",
    },
    Mono {
        mnemonic: "NEG",
        base6: 0x0D,
        subop: 2,
        size: Size::Word,
        count: false,
        summary: "Negate",
    },
    Mono {
        mnemonic: "NEGB",
        base6: 0x0C,
        subop: 2,
        size: Size::Byte,
        count: false,
        summary: "Negate byte",
    },
    Mono {
        mnemonic: "TEST",
        base6: 0x0D,
        subop: 4,
        size: Size::Word,
        count: false,
        summary: "Test",
    },
    Mono {
        mnemonic: "TESTB",
        base6: 0x0C,
        subop: 4,
        size: Size::Byte,
        count: false,
        summary: "Test byte",
    },
    Mono {
        mnemonic: "TSET",
        base6: 0x0D,
        subop: 6,
        size: Size::Word,
        count: false,
        summary: "Test and set",
    },
    Mono {
        mnemonic: "TSETB",
        base6: 0x0C,
        subop: 6,
        size: Size::Byte,
        count: false,
        summary: "Test and set byte",
    },
    Mono {
        mnemonic: "CLR",
        base6: 0x0D,
        subop: 8,
        size: Size::Word,
        count: false,
        summary: "Clear",
    },
    Mono {
        mnemonic: "CLRB",
        base6: 0x0C,
        subop: 8,
        size: Size::Byte,
        count: false,
        summary: "Clear byte",
    },
    Mono {
        mnemonic: "INC",
        base6: 0x29,
        subop: 0,
        size: Size::Word,
        count: true,
        summary: "Increment",
    },
    Mono {
        mnemonic: "INCB",
        base6: 0x28,
        subop: 0,
        size: Size::Byte,
        count: true,
        summary: "Increment byte",
    },
    Mono {
        mnemonic: "DEC",
        base6: 0x2B,
        subop: 0,
        size: Size::Word,
        count: true,
        summary: "Decrement",
    },
    Mono {
        mnemonic: "DECB",
        base6: 0x2A,
        subop: 0,
        size: Size::Byte,
        count: true,
        summary: "Decrement byte",
    },
];

/// Find a single-operand instruction by mnemonic (case-insensitive).
#[must_use]
pub fn mono_lookup(mnemonic: &str) -> Option<&'static Mono> {
    MONO.iter()
        .find(|m| m.mnemonic.eq_ignore_ascii_case(mnemonic))
}

/// Decode the single-operand instruction for opcode top byte `top` and the
/// second byte's low nibble `low` (a sub-opcode, or a count field), or `None`.
#[must_use]
pub fn mono_decode(top: u8, low: u8) -> Option<&'static Mono> {
    let base6 = top & 0x3F;
    MONO.iter()
        .find(|m| m.base6 == base6 && (m.count || m.subop == low))
}

// ---------------------------------------------------------------------------
// Stack (increment 5): PUSH / POP / PUSHL / POPL
// ---------------------------------------------------------------------------

/// A stack instruction. `PUSH @Rsp, src` / `POP dst, @Rsp`: the stack-pointer
/// register is the second byte's **high** nibble and the value operand's field
/// the **low** nibble, with `MM` selecting the value's addressing mode (R / IR /
/// DA / X). `PUSH` additionally has an immediate form encoded specially at
/// `base6` 0x0D with low nibble 9.
pub struct Stack {
    pub mnemonic: &'static str,
    pub base6: u8,
    pub size: Size,
    /// `PUSH`/`PUSHL` (source → stack) vs `POP`/`POPL` (stack → destination).
    pub push: bool,
    /// Has an immediate source form (`PUSH` only).
    pub has_imm: bool,
    pub summary: &'static str,
}

/// The `base6` of the special `PUSH @Rsp, #imm` form (low nibble 9).
pub const PUSH_IMM_BASE6: u8 = 0x0D;

/// The stack instructions (increment 5).
pub const STACK: &[Stack] = &[
    Stack {
        mnemonic: "PUSH",
        base6: 0x13,
        size: Size::Word,
        push: true,
        has_imm: true,
        summary: "Push",
    },
    Stack {
        mnemonic: "PUSHL",
        base6: 0x11,
        size: Size::Long,
        push: true,
        has_imm: false,
        summary: "Push long",
    },
    Stack {
        mnemonic: "POP",
        base6: 0x17,
        size: Size::Word,
        push: false,
        has_imm: false,
        summary: "Pop",
    },
    Stack {
        mnemonic: "POPL",
        base6: 0x15,
        size: Size::Long,
        push: false,
        has_imm: false,
        summary: "Pop long",
    },
];

/// Find a stack instruction by mnemonic (case-insensitive).
#[must_use]
pub fn stack_lookup(mnemonic: &str) -> Option<&'static Stack> {
    STACK
        .iter()
        .find(|s| s.mnemonic.eq_ignore_ascii_case(mnemonic))
}

/// Decode the stack instruction for opcode top byte `top`, or `None`.
#[must_use]
pub fn stack_decode(top: u8) -> Option<&'static Stack> {
    let base6 = top & 0x3F;
    STACK.iter().find(|s| s.base6 == base6)
}

// ---------------------------------------------------------------------------
// Shifts / rotates (increment 6): SLA/SRA/SLL/SRL, RL/RR/RLC/RRC
// ---------------------------------------------------------------------------

/// A shift or rotate's field shape. Both key on `base6` 0x33 (word/long) or
/// 0x32 (byte) with the operand register in the second byte's **high** nibble;
/// the **low** nibble's bit 0 distinguishes the two (1 = shift, 0 = rotate).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShiftKind {
    /// Arithmetic / logical shift. A **signed count word** follows: positive is
    /// left, negative is right, so `SLA`/`SRA` (and `SLL`/`SRL`) share one
    /// opcode, told apart by the count's sign. The magnitude range is the size's
    /// bit width (byte 8, word 16, long 32; left also allows 0).
    Shift,
    /// Rotate by 1 or 2. **No count word** — the rotate type and count pack into
    /// the low nibble as `type·4 + (count − 1)·2` (`RL` 0, `RR` 1, `RLC` 2,
    /// `RRC` 3).
    Rotate,
}

use ShiftKind::{Rotate, Shift as ShiftK};

/// One shift or rotate mnemonic.
pub struct Shift {
    pub mnemonic: &'static str,
    /// 0x32 (byte) or 0x33 (word / long).
    pub base6: u8,
    pub size: Size,
    pub kind: ShiftKind,
    /// [`Shift`](ShiftKind::Shift): the low-nibble sub-opcode (word logical 1 /
    /// arithmetic 9, long logical 5 / arithmetic 0xD).
    /// [`Rotate`](ShiftKind::Rotate): the rotate type (0–3), laid down as
    /// `type·4 + (count − 1)·2`.
    pub sel: u8,
    /// [`Shift`](ShiftKind::Shift) only: a right variant (`SRx`), so the encoder
    /// negates the count. (The decoder reads left/right from the count's sign.)
    pub right: bool,
    pub summary: &'static str,
}

/// The shift and rotate instructions (increment 6).
pub const SHIFTS: &[Shift] = &[
    // --- Shifts: word (base6 0x33) -----------------------------------------
    sh("SLA", 0x33, Word, 9, false, "Shift left arithmetic"),
    sh("SRA", 0x33, Word, 9, true, "Shift right arithmetic"),
    sh("SLL", 0x33, Word, 1, false, "Shift left logical"),
    sh("SRL", 0x33, Word, 1, true, "Shift right logical"),
    // --- Shifts: long (base6 0x33, distinct sub-opcode) --------------------
    sh("SLAL", 0x33, Long, 0xD, false, "Shift left arithmetic long"),
    sh("SRAL", 0x33, Long, 0xD, true, "Shift right arithmetic long"),
    sh("SLLL", 0x33, Long, 5, false, "Shift left logical long"),
    sh("SRLL", 0x33, Long, 5, true, "Shift right logical long"),
    // --- Shifts: byte (base6 0x32) -----------------------------------------
    sh("SLAB", 0x32, Byte, 9, false, "Shift left arithmetic byte"),
    sh("SRAB", 0x32, Byte, 9, true, "Shift right arithmetic byte"),
    sh("SLLB", 0x32, Byte, 1, false, "Shift left logical byte"),
    sh("SRLB", 0x32, Byte, 1, true, "Shift right logical byte"),
    // --- Rotates: word (base6 0x33) ----------------------------------------
    rot("RL", 0x33, Word, 0, "Rotate left"),
    rot("RR", 0x33, Word, 1, "Rotate right"),
    rot("RLC", 0x33, Word, 2, "Rotate left through carry"),
    rot("RRC", 0x33, Word, 3, "Rotate right through carry"),
    // --- Rotates: byte (base6 0x32) ----------------------------------------
    rot("RLB", 0x32, Byte, 0, "Rotate left byte"),
    rot("RRB", 0x32, Byte, 1, "Rotate right byte"),
    rot("RLCB", 0x32, Byte, 2, "Rotate left through carry byte"),
    rot("RRCB", 0x32, Byte, 3, "Rotate right through carry byte"),
];

const fn sh(
    mnemonic: &'static str,
    base6: u8,
    size: Size,
    sel: u8,
    right: bool,
    summary: &'static str,
) -> Shift {
    Shift {
        mnemonic,
        base6,
        size,
        kind: ShiftK,
        sel,
        right,
        summary,
    }
}

const fn rot(
    mnemonic: &'static str,
    base6: u8,
    size: Size,
    sel: u8,
    summary: &'static str,
) -> Shift {
    Shift {
        mnemonic,
        base6,
        size,
        kind: Rotate,
        sel,
        right: false,
        summary,
    }
}

/// The signed shift-count magnitude range for a size (byte 8, word 16, long 32).
#[must_use]
pub fn shift_max(size: Size) -> i64 {
    match size {
        Size::Byte => 8,
        Size::Long => 32,
        _ => 16,
    }
}

/// Find a shift / rotate instruction by mnemonic (case-insensitive).
#[must_use]
pub fn shift_lookup(mnemonic: &str) -> Option<&'static Shift> {
    SHIFTS
        .iter()
        .find(|s| s.mnemonic.eq_ignore_ascii_case(mnemonic))
}

/// Decode a shift for `(base6, subop)` and the count's sign (`right`), or `None`
/// if no shift occupies that sub-opcode (the word is then data).
#[must_use]
pub fn shift_decode(base6: u8, subop: u8, right: bool) -> Option<&'static Shift> {
    SHIFTS
        .iter()
        .find(|s| s.kind == ShiftK && s.base6 == base6 && s.sel == subop && s.right == right)
}

/// Decode a rotate for `(base6, type)` (the low nibble's high 3 bits), or `None`.
#[must_use]
pub fn rotate_decode(base6: u8, rtype: u8) -> Option<&'static Shift> {
    SHIFTS
        .iter()
        .find(|s| s.kind == Rotate && s.base6 == base6 && s.sel == rtype)
}

// ---------------------------------------------------------------------------
// Sign-extend (increment 6): EXTSB / EXTS / EXTSL
// ---------------------------------------------------------------------------

/// A sign-extend instruction. The first (and only) word is `0xB1` then the
/// operand register in the second byte's **high** nibble and a sub-opcode in the
/// low nibble. The register size widens per instruction: `EXTSB` sign-extends a
/// byte through a **word** register, `EXTS` a word through a **long** pair,
/// `EXTSL` a long through a **quad**.
pub struct Extend {
    pub mnemonic: &'static str,
    /// Low-nibble sub-opcode: `EXTSB` 0, `EXTSL` 7, `EXTS` 0xA.
    pub subop: u8,
    pub size: Size,
    pub summary: &'static str,
}

/// The top byte shared by every sign-extend instruction (`MM` = R, base6 0x31).
pub const EXTEND_TOP: u8 = 0xB1;

/// The sign-extend instructions (increment 6).
pub const EXTENDS: &[Extend] = &[
    Extend {
        mnemonic: "EXTSB",
        subop: 0,
        size: Size::Word,
        summary: "Extend sign byte",
    },
    Extend {
        mnemonic: "EXTS",
        subop: 0xA,
        size: Size::Long,
        summary: "Extend sign word",
    },
    Extend {
        mnemonic: "EXTSL",
        subop: 7,
        size: Size::Quad,
        summary: "Extend sign long",
    },
];

/// Find a sign-extend instruction by mnemonic (case-insensitive).
#[must_use]
pub fn extend_lookup(mnemonic: &str) -> Option<&'static Extend> {
    EXTENDS
        .iter()
        .find(|e| e.mnemonic.eq_ignore_ascii_case(mnemonic))
}

/// Decode a sign-extend instruction by its second byte's low nibble, or `None`.
#[must_use]
pub fn extend_decode(subop: u8) -> Option<&'static Extend> {
    EXTENDS.iter().find(|e| e.subop == subop)
}

// ---------------------------------------------------------------------------
// Bit (increment 7): BIT / SET / RES, static and dynamic
// ---------------------------------------------------------------------------

/// A bit-manipulation instruction (`BIT`/`SET`/`RES` + byte). Two encodings
/// share the `base6`:
///
/// - **Static** — a literal bit number. `MM base6 | field << 4 | b`: the operand
///   (register / `@Rn` / direct / indexed) is chosen by `MM` and the second
///   byte's **high** nibble exactly as the dyadic family does, and the **low**
///   nibble is the bit number (0–15 word, 0–7 byte). One word (+ an address word
///   for direct / indexed).
/// - **Dynamic** — the bit number in a **word** register. A two-word form at
///   `MM` = 00 with the second byte's high nibble **zero** (so it never collides
///   with static `@Rn`, whose pointer is 1–15 — R0 is not a legal base): word 1
///   is `base6 << 8 | bit-register`, word 2 is `target-register << 8`. The target
///   is register-only (word or byte per the size).
pub struct Bit {
    pub mnemonic: &'static str,
    /// `BIT` 0x27 / `BITB` 0x26 / `SET` 0x25 / `SETB` 0x24 / `RES` 0x23 /
    /// `RESB` 0x22.
    pub base6: u8,
    pub size: Size,
    pub summary: &'static str,
}

/// The bit-manipulation instructions (increment 7).
pub const BITS: &[Bit] = &[
    Bit {
        mnemonic: "BIT",
        base6: 0x27,
        size: Size::Word,
        summary: "Test bit",
    },
    Bit {
        mnemonic: "BITB",
        base6: 0x26,
        size: Size::Byte,
        summary: "Test bit byte",
    },
    Bit {
        mnemonic: "SET",
        base6: 0x25,
        size: Size::Word,
        summary: "Set bit",
    },
    Bit {
        mnemonic: "SETB",
        base6: 0x24,
        size: Size::Byte,
        summary: "Set bit byte",
    },
    Bit {
        mnemonic: "RES",
        base6: 0x23,
        size: Size::Word,
        summary: "Reset bit",
    },
    Bit {
        mnemonic: "RESB",
        base6: 0x22,
        size: Size::Byte,
        summary: "Reset bit byte",
    },
];

/// The highest bit number for a size (byte 7, word 15).
#[must_use]
pub fn bit_max(size: Size) -> i64 {
    match size {
        Size::Byte => 7,
        _ => 15,
    }
}

/// Find a bit instruction by mnemonic (case-insensitive).
#[must_use]
pub fn bit_lookup(mnemonic: &str) -> Option<&'static Bit> {
    BITS.iter()
        .find(|b| b.mnemonic.eq_ignore_ascii_case(mnemonic))
}

/// Decode a bit instruction by its opcode top byte's `base6`, or `None`.
#[must_use]
pub fn bit_decode(top: u8) -> Option<&'static Bit> {
    let base6 = top & 0x3F;
    BITS.iter().find(|b| b.base6 == base6)
}

// ---------------------------------------------------------------------------
// Multiply / divide (increment 8): MULT / MULTL / DIV / DIVL
// ---------------------------------------------------------------------------

/// A multiply / divide instruction. Dyadic-shaped — `MM base6 | field << 4 |
/// dest`, the source reached by R / IM / IR / DA / X exactly as the dyadic
/// family — but with **asymmetric operand sizes**: the destination is a double-
/// width accumulator (a long `rr` pair for `MULT`/`DIV`, a quad `rq` for
/// `MULTL`/`DIVL`) while the source (and its immediate width) is one size
/// smaller (`MULT`/`DIV` multiply/divide by a word, `MULTL`/`DIVL` by a long).
pub struct MulDiv {
    pub mnemonic: &'static str,
    pub base6: u8,
    /// The destination accumulator's size: [`Long`](Size::Long) (`rr`) or
    /// [`Quad`](Size::Quad) (`rq`).
    pub dest: Size,
    /// The source operand's size (and immediate width):
    /// [`Word`](Size::Word) or [`Long`](Size::Long).
    pub src: Size,
    pub summary: &'static str,
}

/// The multiply / divide instructions (increment 8).
pub const MULDIV: &[MulDiv] = &[
    MulDiv {
        mnemonic: "MULT",
        base6: 0x19,
        dest: Size::Long,
        src: Size::Word,
        summary: "Multiply (word)",
    },
    MulDiv {
        mnemonic: "MULTL",
        base6: 0x18,
        dest: Size::Quad,
        src: Size::Long,
        summary: "Multiply long",
    },
    MulDiv {
        mnemonic: "DIV",
        base6: 0x1B,
        dest: Size::Long,
        src: Size::Word,
        summary: "Divide (word)",
    },
    MulDiv {
        mnemonic: "DIVL",
        base6: 0x1A,
        dest: Size::Quad,
        src: Size::Long,
        summary: "Divide long",
    },
];

/// Whether register number `reg` is a legal register of `size` (a long `rr`
/// pair is even; a quad `rq` a multiple of four; word / byte unrestricted).
#[must_use]
pub fn reg_aligned(reg: u16, size: Size) -> bool {
    match size {
        Size::Long => reg.is_multiple_of(2),
        Size::Quad => reg.is_multiple_of(4),
        _ => true,
    }
}

/// Find a multiply / divide instruction by mnemonic (case-insensitive).
#[must_use]
pub fn muldiv_lookup(mnemonic: &str) -> Option<&'static MulDiv> {
    MULDIV
        .iter()
        .find(|m| m.mnemonic.eq_ignore_ascii_case(mnemonic))
}

/// Decode a multiply / divide instruction by its opcode top byte's `base6`, or
/// `None`.
#[must_use]
pub fn muldiv_decode(top: u8) -> Option<&'static MulDiv> {
    let base6 = top & 0x3F;
    MULDIV.iter().find(|m| m.base6 == base6)
}

// ---------------------------------------------------------------------------
// Block / string (increment 9): LDx / CPx / CPSx / TRxB / TRTxB (repeat group)
// ---------------------------------------------------------------------------

/// The operand shape of a block / string instruction. Every one is a **two-word**
/// form at `MM` = 10: word 1 is `TOP | pointer << 4 | op_nib`, word 2 is
/// `count << 8 | pointer-or-register << 4 | ctrl` (word 2's top nibble is always
/// zero). The shape fixes which word holds the source vs destination pointer and
/// whether a condition code rides the control nibble.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlockShape {
    /// `LDx  @Rd, @Rs, Rc` — source pointer in word 1, dest pointer in word 2;
    /// the control nibble is a fixed single/repeat marker.
    Load,
    /// `CPx  Rr, @Rs, Rc, cc` — source pointer in word 1, a data **register** in
    /// word 2, the condition code in the control nibble.
    Compare,
    /// `CPSx @Rd, @Rs, Rc, cc` — like [`Load`](BlockShape::Load) but the control
    /// nibble is a condition code.
    CompareString,
    /// `TRxB / TRTxB @Rd, @Rs, Rc` — dest pointer in word 1, source pointer in
    /// word 2 (the reverse of `LDx`); byte only, R1 (`RH1`) implied.
    Translate,
}

/// One block / string instruction.
pub struct Block {
    pub mnemonic: &'static str,
    /// `0x3B` word / `0x3A` byte (`LDx`/`CPx`/`CPSx`), `0x38` (`TRxB`/`TRTxB`).
    pub base6: u8,
    /// Word 1's low nibble — identifies the operation, direction, and (for
    /// `CPx`/`CPSx`) the repeat bit.
    pub op_nib: u8,
    /// Word 2's low nibble for the fixed-control shapes ([`Load`](BlockShape::Load)
    /// single/repeat, [`Translate`](BlockShape::Translate)); the condition-carrying
    /// shapes read a condition code here instead.
    pub ctrl: u8,
    pub shape: BlockShape,
    /// The data register's size for [`Compare`](BlockShape::Compare); word / byte
    /// otherwise selects only the top byte.
    pub size: Size,
    pub summary: &'static str,
}

impl Block {
    /// Whether the control nibble carries a condition code (`CPx`/`CPSx`) rather
    /// than a fixed value.
    #[must_use]
    pub fn has_cc(&self) -> bool {
        matches!(self.shape, BlockShape::Compare | BlockShape::CompareString)
    }
}

use BlockShape::{Compare, CompareString, Load, Translate};

const fn blk(
    mnemonic: &'static str,
    base6: u8,
    op_nib: u8,
    ctrl: u8,
    shape: BlockShape,
    size: Size,
    summary: &'static str,
) -> Block {
    Block {
        mnemonic,
        base6,
        op_nib,
        ctrl,
        shape,
        size,
        summary,
    }
}

/// The block / string instructions (increment 9).
pub const BLOCK: &[Block] = &[
    // Block move (word 0x3B / byte 0x3A): control nibble 8 single, 0 repeat.
    blk("LDI", 0x3B, 1, 8, Load, Word, "Load and increment"),
    blk("LDIR", 0x3B, 1, 0, Load, Word, "Load, increment and repeat"),
    blk("LDD", 0x3B, 9, 8, Load, Word, "Load and decrement"),
    blk("LDDR", 0x3B, 9, 0, Load, Word, "Load, decrement and repeat"),
    blk("LDIB", 0x3A, 1, 8, Load, Byte, "Load byte and increment"),
    blk(
        "LDIRB",
        0x3A,
        1,
        0,
        Load,
        Byte,
        "Load byte, increment and repeat",
    ),
    blk("LDDB", 0x3A, 9, 8, Load, Byte, "Load byte and decrement"),
    blk(
        "LDDRB",
        0x3A,
        9,
        0,
        Load,
        Byte,
        "Load byte, decrement and repeat",
    ),
    // Block compare: control nibble is the condition code.
    blk("CPI", 0x3B, 0, 0, Compare, Word, "Compare and increment"),
    blk(
        "CPIR",
        0x3B,
        4,
        0,
        Compare,
        Word,
        "Compare, increment and repeat",
    ),
    blk("CPD", 0x3B, 8, 0, Compare, Word, "Compare and decrement"),
    blk(
        "CPDR",
        0x3B,
        0xC,
        0,
        Compare,
        Word,
        "Compare, decrement and repeat",
    ),
    blk(
        "CPIB",
        0x3A,
        0,
        0,
        Compare,
        Byte,
        "Compare byte and increment",
    ),
    blk(
        "CPIRB",
        0x3A,
        4,
        0,
        Compare,
        Byte,
        "Compare byte, increment and repeat",
    ),
    blk(
        "CPDB",
        0x3A,
        8,
        0,
        Compare,
        Byte,
        "Compare byte and decrement",
    ),
    blk(
        "CPDRB",
        0x3A,
        0xC,
        0,
        Compare,
        Byte,
        "Compare byte, decrement and repeat",
    ),
    // Compare string: control nibble is the condition code.
    blk(
        "CPSI",
        0x3B,
        2,
        0,
        CompareString,
        Word,
        "Compare string and increment",
    ),
    blk(
        "CPSIR",
        0x3B,
        6,
        0,
        CompareString,
        Word,
        "Compare string, increment and repeat",
    ),
    blk(
        "CPSD",
        0x3B,
        0xA,
        0,
        CompareString,
        Word,
        "Compare string and decrement",
    ),
    blk(
        "CPSDR",
        0x3B,
        0xE,
        0,
        CompareString,
        Word,
        "Compare string, decrement and repeat",
    ),
    blk(
        "CPSIB",
        0x3A,
        2,
        0,
        CompareString,
        Byte,
        "Compare string byte and increment",
    ),
    blk(
        "CPSIRB",
        0x3A,
        6,
        0,
        CompareString,
        Byte,
        "Compare string byte, increment and repeat",
    ),
    blk(
        "CPSDB",
        0x3A,
        0xA,
        0,
        CompareString,
        Byte,
        "Compare string byte and decrement",
    ),
    blk(
        "CPSDRB",
        0x3A,
        0xE,
        0,
        CompareString,
        Byte,
        "Compare string byte, decrement and repeat",
    ),
    // Translate (byte only, 0x38): control nibble 0.
    blk(
        "TRIB",
        0x38,
        0,
        0,
        Translate,
        Byte,
        "Translate and increment",
    ),
    blk(
        "TRIRB",
        0x38,
        4,
        0,
        Translate,
        Byte,
        "Translate, increment and repeat",
    ),
    blk(
        "TRDB",
        0x38,
        8,
        0,
        Translate,
        Byte,
        "Translate and decrement",
    ),
    blk(
        "TRDRB",
        0x38,
        0xC,
        0,
        Translate,
        Byte,
        "Translate, decrement and repeat",
    ),
    // Translate and test (byte only): repeat forms carry control nibble 0xE.
    blk(
        "TRTIB",
        0x38,
        2,
        0,
        Translate,
        Byte,
        "Translate and test, increment",
    ),
    blk(
        "TRTIRB",
        0x38,
        6,
        0xE,
        Translate,
        Byte,
        "Translate and test, increment and repeat",
    ),
    blk(
        "TRTDB",
        0x38,
        0xA,
        0,
        Translate,
        Byte,
        "Translate and test, decrement",
    ),
    blk(
        "TRTDRB",
        0x38,
        0xE,
        0xE,
        Translate,
        Byte,
        "Translate and test, decrement and repeat",
    ),
];

/// Find a block / string instruction by mnemonic (case-insensitive).
#[must_use]
pub fn block_lookup(mnemonic: &str) -> Option<&'static Block> {
    BLOCK
        .iter()
        .find(|b| b.mnemonic.eq_ignore_ascii_case(mnemonic))
}

/// Decode a block / string instruction from its opcode top byte, word-1 low
/// nibble (`op_nib`), and word-2 low nibble (`ctrl`), or `None`. Requires
/// `MM` = 10; the condition-carrying shapes match any control nibble (it is the
/// code), the fixed-control shapes require an exact match.
#[must_use]
pub fn block_decode(top: u8, op_nib: u8, ctrl: u8) -> Option<&'static Block> {
    if top >> 6 != 2 {
        return None;
    }
    let base6 = top & 0x3F;
    BLOCK
        .iter()
        .find(|b| b.base6 == base6 && b.op_nib == op_nib && (b.has_cc() || b.ctrl == ctrl))
}

/// Operand size, which fixes register naming and immediate width.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Size {
    /// Byte registers `rh`/`rl`; a byte immediate replicated into a word.
    Byte,
    /// Word registers `r0`–`r15`; a 16-bit immediate.
    Word,
    /// Long register pairs `rr0`–`rr14`; a 32-bit immediate.
    Long,
    /// Quad register `rq0`/`rq4`/`rq8`/`rq12` (four consecutive words); the
    /// `EXTSL` operand only. No immediate.
    Quad,
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
