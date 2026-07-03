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

// ---------------------------------------------------------------------------
// I/O (increment 10): IN / OUT / SIN / SOUT + the block-I/O repeat group
// ---------------------------------------------------------------------------
//
// All privileged (`asl` needs `SUPMODE ON`). Everything is `MM` = 00. Simple
// I/O has a **direct**-port form (top `0x3B` word / `0x3A` byte, word 1 =
// `reg << 4 | sub`, then a port address word) and — for `IN`/`OUT` only — an
// **indirect** `@Rn`-port form (its own top byte, word 1 = `port << 4 | reg`).
// Block I/O reuses the block/string two-word Load shape at top `0x3B`/`0x3A`;
// the second byte's low nibble tells them apart (4–7 direct simple I/O,
// 0–3/8–B block I/O).

/// One simple I/O instruction (`IN`/`OUT`/`SIN`/`SOUT` + byte).
pub struct SimpleIo {
    pub mnemonic: &'static str,
    pub size: Size,
    /// The register is the data destination, so the operand order is `reg, port`
    /// (`IN`/`SIN`); otherwise `port, reg` (`OUT`/`SOUT`).
    pub input: bool,
    /// The direct-port sub-opcode (word 1's low nibble): `IN` 4, `SIN` 5, `OUT`
    /// 6, `SOUT` 7. The direct top byte is `0x3B` word / `0x3A` byte.
    pub direct_sub: u8,
    /// The top byte of the indirect (`@Rn` port) form, or `None` — special I/O
    /// (`SIN`/`SOUT`) has no indirect form.
    pub indirect_top: Option<u8>,
    pub summary: &'static str,
}

/// The direct-port top byte for a size (`0x3B` word / `0x3A` byte).
#[must_use]
pub fn io_direct_top(size: Size) -> u8 {
    if matches!(size, Size::Byte) {
        0x3A
    } else {
        0x3B
    }
}

/// The simple I/O instructions (increment 10).
pub const SIMPLE_IO: &[SimpleIo] = &[
    SimpleIo {
        mnemonic: "IN",
        size: Size::Word,
        input: true,
        direct_sub: 4,
        indirect_top: Some(0x3D),
        summary: "Input",
    },
    SimpleIo {
        mnemonic: "INB",
        size: Size::Byte,
        input: true,
        direct_sub: 4,
        indirect_top: Some(0x3C),
        summary: "Input byte",
    },
    SimpleIo {
        mnemonic: "SIN",
        size: Size::Word,
        input: true,
        direct_sub: 5,
        indirect_top: None,
        summary: "Special input",
    },
    SimpleIo {
        mnemonic: "SINB",
        size: Size::Byte,
        input: true,
        direct_sub: 5,
        indirect_top: None,
        summary: "Special input byte",
    },
    SimpleIo {
        mnemonic: "OUT",
        size: Size::Word,
        input: false,
        direct_sub: 6,
        indirect_top: Some(0x3F),
        summary: "Output",
    },
    SimpleIo {
        mnemonic: "OUTB",
        size: Size::Byte,
        input: false,
        direct_sub: 6,
        indirect_top: Some(0x3E),
        summary: "Output byte",
    },
    SimpleIo {
        mnemonic: "SOUT",
        size: Size::Word,
        input: false,
        direct_sub: 7,
        indirect_top: None,
        summary: "Special output",
    },
    SimpleIo {
        mnemonic: "SOUTB",
        size: Size::Byte,
        input: false,
        direct_sub: 7,
        indirect_top: None,
        summary: "Special output byte",
    },
];

/// Find a simple I/O instruction by mnemonic (case-insensitive).
#[must_use]
pub fn simple_io_lookup(mnemonic: &str) -> Option<&'static SimpleIo> {
    SIMPLE_IO
        .iter()
        .find(|i| i.mnemonic.eq_ignore_ascii_case(mnemonic))
}

/// Decode a direct-port simple I/O instruction from its size and sub-opcode.
#[must_use]
pub fn simple_io_direct(size: Size, sub: u8) -> Option<&'static SimpleIo> {
    SIMPLE_IO
        .iter()
        .find(|i| i.size == size && i.direct_sub == sub)
}

/// Decode an indirect-port (`@Rn`) simple I/O instruction from its top byte.
#[must_use]
pub fn simple_io_indirect(top: u8) -> Option<&'static SimpleIo> {
    SIMPLE_IO.iter().find(|i| i.indirect_top == Some(top))
}

/// One block-I/O instruction (`INI`/`OUTI`/…, special `SINI`/… + byte). A
/// two-word Load-shaped form (`@Rd, @Rs, Rc`) at top `0x3B` word / `0x3A` byte.
pub struct BlockIo {
    pub mnemonic: &'static str,
    pub size: Size,
    /// Word 1's low nibble: `IN` 0, `SIN` 1, `OUT` 2, `SOUT` 3, `+8` to
    /// decrement.
    pub op_nib: u8,
    /// Word 2's low nibble: 8 single, 0 repeat.
    pub ctrl: u8,
    pub summary: &'static str,
}

const fn bio(
    mnemonic: &'static str,
    size: Size,
    op_nib: u8,
    ctrl: u8,
    summary: &'static str,
) -> BlockIo {
    BlockIo {
        mnemonic,
        size,
        op_nib,
        ctrl,
        summary,
    }
}

/// The block-I/O instructions (increment 10).
pub const BLOCK_IO: &[BlockIo] = &[
    // Block input (op_nib 0 / decrement 8), single ctrl 8 / repeat ctrl 0.
    bio("INI", Word, 0, 8, "Input and increment"),
    bio("INIR", Word, 0, 0, "Input, increment and repeat"),
    bio("IND", Word, 8, 8, "Input and decrement"),
    bio("INDR", Word, 8, 0, "Input, decrement and repeat"),
    bio("INIB", Byte, 0, 8, "Input byte and increment"),
    bio("INIRB", Byte, 0, 0, "Input byte, increment and repeat"),
    bio("INDB", Byte, 8, 8, "Input byte and decrement"),
    bio("INDRB", Byte, 8, 0, "Input byte, decrement and repeat"),
    // Block output (op_nib 2 / decrement A).
    bio("OUTI", Word, 2, 8, "Output and increment"),
    bio("OTIR", Word, 2, 0, "Output, increment and repeat"),
    bio("OUTD", Word, 0xA, 8, "Output and decrement"),
    bio("OTDR", Word, 0xA, 0, "Output, decrement and repeat"),
    bio("OUTIB", Byte, 2, 8, "Output byte and increment"),
    bio("OTIRB", Byte, 2, 0, "Output byte, increment and repeat"),
    bio("OUTDB", Byte, 0xA, 8, "Output byte and decrement"),
    bio("OTDRB", Byte, 0xA, 0, "Output byte, decrement and repeat"),
    // Special block input (op_nib 1 / decrement 9).
    bio("SINI", Word, 1, 8, "Special input and increment"),
    bio("SINIR", Word, 1, 0, "Special input, increment and repeat"),
    bio("SIND", Word, 9, 8, "Special input and decrement"),
    bio("SINDR", Word, 9, 0, "Special input, decrement and repeat"),
    bio("SINIB", Byte, 1, 8, "Special input byte and increment"),
    bio(
        "SINIRB",
        Byte,
        1,
        0,
        "Special input byte, increment and repeat",
    ),
    bio("SINDB", Byte, 9, 8, "Special input byte and decrement"),
    bio(
        "SINDRB",
        Byte,
        9,
        0,
        "Special input byte, decrement and repeat",
    ),
    // Special block output (op_nib 3 / decrement B).
    bio("SOUTI", Word, 3, 8, "Special output and increment"),
    bio("SOTIR", Word, 3, 0, "Special output, increment and repeat"),
    bio("SOUTD", Word, 0xB, 8, "Special output and decrement"),
    bio(
        "SOTDR",
        Word,
        0xB,
        0,
        "Special output, decrement and repeat",
    ),
    bio("SOUTIB", Byte, 3, 8, "Special output byte and increment"),
    bio(
        "SOTIRB",
        Byte,
        3,
        0,
        "Special output byte, increment and repeat",
    ),
    bio("SOUTDB", Byte, 0xB, 8, "Special output byte and decrement"),
    bio(
        "SOTDRB",
        Byte,
        0xB,
        0,
        "Special output byte, decrement and repeat",
    ),
];

/// Find a block-I/O instruction by mnemonic (case-insensitive).
#[must_use]
pub fn block_io_lookup(mnemonic: &str) -> Option<&'static BlockIo> {
    BLOCK_IO
        .iter()
        .find(|b| b.mnemonic.eq_ignore_ascii_case(mnemonic))
}

/// Decode a block-I/O instruction from its size, word-1 low nibble, and word-2
/// low nibble.
#[must_use]
pub fn block_io_decode(size: Size, op_nib: u8, ctrl: u8) -> Option<&'static BlockIo> {
    BLOCK_IO
        .iter()
        .find(|b| b.size == size && b.op_nib == op_nib && b.ctrl == ctrl)
}

// ---------------------------------------------------------------------------
// CPU control (increment 11): NOP / HALT / EI / DI / IRET / LDCTL / LDPS /
// MSET / MRES / MBIT / MREQ / SETFLG / RESFLG / COMFLG / SC
// ---------------------------------------------------------------------------

/// The shape of a CPU-control instruction — each sub-group has its own encoding,
/// keyed by a distinct top byte on decode.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ControlKind {
    /// A fixed two-byte word with no operand (`NOP` 0x8D07, `HALT` 0x7A00,
    /// `IRET` 0x7B00, `MSET` 0x7B08, `MRES` 0x7B09, `MBIT` 0x7B0A).
    Fixed(u16),
    /// `MREQ Rd` — `0x7B00 | reg << 4 | 0x0D`.
    Mreq,
    /// `SETFLG`/`RESFLG`/`COMFLG flags` — top `0x8D`, `flag-mask << 4 | subop`
    /// (`SETFLG` 1, `RESFLG` 3, `COMFLG` 5).
    Flag(u8),
    /// `EI`/`DI vi[,nvi]` — top `0x7C`; the boolean is enable (`EI`).
    Intr(bool),
    /// `LDCTL`/`LDCTLB` — a register and a control register. Word (`0x7D`,
    /// `FCW`/`REFRESH`/`PSAP`/`NSP`) or byte (`0x8C`, `FLAGS` only).
    Ldctl(Size),
    /// `LDPS src` — load program status from an `IR`/`DA`/`X` memory operand
    /// (top `0x39` indirect, `0x79` direct/indexed).
    Ldps,
    /// `SC #n` — system call, `0x7F00 | n` (`n` 0–255).
    Sc,
}

/// One CPU-control instruction.
pub struct Control {
    pub mnemonic: &'static str,
    pub kind: ControlKind,
    pub summary: &'static str,
}

use ControlKind::{Fixed, Flag, Intr, Ldctl, Ldps, Mreq, Sc};

const fn ctrl(mnemonic: &'static str, kind: ControlKind, summary: &'static str) -> Control {
    Control {
        mnemonic,
        kind,
        summary,
    }
}

/// The CPU-control instructions (increment 11).
pub const CONTROLS: &[Control] = &[
    ctrl("NOP", Fixed(0x8D07), "No operation"),
    ctrl("HALT", Fixed(0x7A00), "Halt"),
    ctrl("IRET", Fixed(0x7B00), "Interrupt return"),
    ctrl("MSET", Fixed(0x7B08), "Multi-micro set"),
    ctrl("MRES", Fixed(0x7B09), "Multi-micro reset"),
    ctrl("MBIT", Fixed(0x7B0A), "Test multi-micro bit"),
    ctrl("MREQ", Mreq, "Multi-micro request"),
    ctrl("SETFLG", Flag(1), "Set flags"),
    ctrl("RESFLG", Flag(3), "Reset flags"),
    ctrl("COMFLG", Flag(5), "Complement flags"),
    ctrl("EI", Intr(true), "Enable interrupt"),
    ctrl("DI", Intr(false), "Disable interrupt"),
    ctrl("LDCTL", Ldctl(Size::Word), "Load control register"),
    ctrl("LDCTLB", Ldctl(Size::Byte), "Load control register byte"),
    ctrl("LDPS", Ldps, "Load program status"),
    ctrl("SC", Sc, "System call"),
];

/// Find a CPU-control instruction by mnemonic (case-insensitive).
#[must_use]
pub fn control_lookup(mnemonic: &str) -> Option<&'static Control> {
    CONTROLS
        .iter()
        .find(|c| c.mnemonic.eq_ignore_ascii_case(mnemonic))
}

/// The bit for a flag name (`C` 8, `Z` 4, `S` 2, `P`/`V` 1), or `None`.
#[must_use]
pub fn flag_bit(name: &str) -> Option<u8> {
    match name.trim().to_ascii_lowercase().as_str() {
        "c" => Some(8),
        "z" => Some(4),
        "s" => Some(2),
        "p" | "v" => Some(1),
        _ => None,
    }
}

/// The flag `(bit, canonical-name)` pairs in `C,Z,S,P` order, for rendering a
/// flag mask.
pub const FLAG_BITS: &[(u8, &str)] = &[(8, "c"), (4, "z"), (2, "s"), (1, "p")];

/// The 4-bit control-register code for a word control-register name (`FCW` 2,
/// `REFRESH` 3, `PSAP`/`PSAPOFF` 5, `NSP`/`NSPOFF` 7), or `None`. (The segmented
/// `PSAPSEG` 4 / `NSPSEG` 6 are Z8001-only and absent here.)
#[must_use]
pub fn word_ctrl_code(name: &str) -> Option<u8> {
    match name.trim().to_ascii_lowercase().as_str() {
        "fcw" => Some(2),
        "refresh" => Some(3),
        "psap" | "psapoff" => Some(5),
        "nsp" | "nspoff" => Some(7),
        _ => None,
    }
}

/// The canonical word control-register name for a code, or `None`.
#[must_use]
pub fn word_ctrl_name(code: u8) -> Option<&'static str> {
    match code {
        2 => Some("fcw"),
        3 => Some("refresh"),
        5 => Some("psap"),
        7 => Some("nsp"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Cleanup (increment 12-prep): TCC / LDK / RLDB / RRDB / LDR — the last
// non-segmented instructions, each a small one-off shape.
// ---------------------------------------------------------------------------

/// The shape of a "miscellaneous" instruction — each is a distinct one-off,
/// keyed on decode by its top byte.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MiscKind {
    /// `TCC`/`TCCB cc, Rd` — top `0xAF` word / `0xAE` byte, `reg << 4 | cc`
    /// (`cc` defaults to 8 = *always*, then omitted).
    Tcc,
    /// `LDK Rd, #n` — `0xBD`, `reg << 4 | n` (`n` 0–15, into a word register).
    Ldk,
    /// `RLDB`/`RRDB Rd, Rs` — rotate digit, top `0xBE` (`RLDB`) / `0xBC`
    /// (`RRDB`), `src << 4 | dst` (byte registers).
    Rotdig,
    /// `LDR`/`LDRB`/`LDRL Rd, addr` (and the `addr, Rs` store) — a PC-relative
    /// load, `reg` in the low nibble then a signed 16-bit `target − (PC + 4)`
    /// offset word. The load top byte is per size; the store form is `top | 2`.
    Ldr,
}

/// One miscellaneous instruction.
pub struct Misc {
    pub mnemonic: &'static str,
    pub kind: MiscKind,
    pub size: Size,
    /// The (load, for `LDR`) top byte.
    pub top: u8,
    pub summary: &'static str,
}

use MiscKind::{Ldk, Ldr, Rotdig, Tcc};

const fn misc(
    mnemonic: &'static str,
    kind: MiscKind,
    size: Size,
    top: u8,
    summary: &'static str,
) -> Misc {
    Misc {
        mnemonic,
        kind,
        size,
        top,
        summary,
    }
}

/// The miscellaneous instructions (final non-segmented cleanup).
pub const MISC: &[Misc] = &[
    misc("TCC", Tcc, Word, 0xAF, "Test condition code"),
    misc("TCCB", Tcc, Byte, 0xAE, "Test condition code byte"),
    misc("LDK", Ldk, Word, 0xBD, "Load constant"),
    misc("RLDB", Rotdig, Byte, 0xBE, "Rotate left digit"),
    misc("RRDB", Rotdig, Byte, 0xBC, "Rotate right digit"),
    misc("LDR", Ldr, Word, 0x31, "Load relative"),
    misc("LDRB", Ldr, Byte, 0x30, "Load relative byte"),
    misc("LDRL", Ldr, Long, 0x35, "Load relative long"),
];

/// Find a miscellaneous instruction by mnemonic (case-insensitive).
#[must_use]
pub fn misc_lookup(mnemonic: &str) -> Option<&'static Misc> {
    MISC.iter()
        .find(|m| m.mnemonic.eq_ignore_ascii_case(mnemonic))
}

/// Decode a miscellaneous instruction from its top byte, returning the entry and
/// whether it is the store form (`LDR` only).
#[must_use]
pub fn misc_decode(top: u8) -> Option<(&'static Misc, bool)> {
    MISC.iter().find_map(|m| {
        if m.top == top {
            Some((m, false))
        } else if matches!(m.kind, MiscKind::Ldr) && m.top | 2 == top {
            Some((m, true))
        } else {
            None
        }
    })
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
