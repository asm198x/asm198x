//! 68000 instruction-set spec — **field-based** encoding.
//!
//! Unlike the byte-opcode CPUs (6502, Z80), the 68000 packs the operation size,
//! register numbers, and one or two 6-bit *effective-address* fields into a
//! single 16-bit opcode word, then follows it with 0–4 extension words. So it
//! gets its own representation here rather than the shared [`crate::Form`]: each
//! [`Form`] is a base opcode word plus a list of [`Slot`]s describing which bits
//! each operand fills, modelled on the table layout in Musashi's `m68k_in.c` and
//! the encodings in the Motorola M68000 Programmer's Reference Manual
//! (`reference/by-topic/cpu-68000/m68000prm.md`).
//!
//! The 68000 is big-endian: every word is emitted high byte first.

/// Operation size.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Size {
    B,
    W,
    L,
}

impl Size {
    /// Bytes a value of this size occupies.
    #[must_use]
    pub fn bytes(self) -> usize {
        match self {
            Size::B | Size::W => 2, // a byte still rides in a full extension word
            Size::L => 4,
        }
    }
}

/// How the size is encoded into the opcode word (it varies by instruction
/// family — the 68000 is not uniform here).
#[derive(Clone, Copy)]
pub enum SizeEnc {
    /// No size field — the form is fixed (e.g. `RTS`, `MOVEQ` is always long).
    Fixed(Size),
    /// Standard 2-bit field at bits 6–7: `00`=B, `01`=W, `10`=L.
    Std6,
    /// `MOVE`'s 2-bit field at bits 12–13: `01`=B, `11`=W, `10`=L.
    Move,
}

/// The allowed-effective-address mask, one bit per addressing mode. Mirrors the
/// 12-mode classification used throughout the 68000 encoding tables.
#[derive(Clone, Copy)]
pub struct EaModes(pub u16);

/// Mode bits for [`EaModes`], OR-ed together in a form's allowed mask.
pub mod ea {
    pub const DN: u16 = 1 << 0; // Dn
    pub const AN: u16 = 1 << 1; // An
    pub const AI: u16 = 1 << 2; // (An)
    pub const PI: u16 = 1 << 3; // (An)+
    pub const PD: u16 = 1 << 4; // -(An)
    pub const DI: u16 = 1 << 5; // d16(An)
    pub const IX: u16 = 1 << 6; // d8(An,Xn)
    pub const AW: u16 = 1 << 7; // (xxx).W
    pub const AL: u16 = 1 << 8; // (xxx).L
    pub const PCD: u16 = 1 << 9; // d16(PC)
    pub const PCX: u16 = 1 << 10; // d8(PC,Xn)
    pub const IMM: u16 = 1 << 11; // #imm

    /// Every mode (the `A+-DXWLdxI` plus Dn/An of the tables).
    pub const ALL: u16 = DN | AN | AI | PI | PD | DI | IX | AW | AL | PCD | PCX | IMM;
    /// Data-alterable: no An, no PC-relative, no immediate.
    pub const DATA_ALT: u16 = DN | AI | PI | PD | DI | IX | AW | AL;
    /// Memory-alterable: alterable minus Dn/An.
    pub const MEM_ALT: u16 = AI | PI | PD | DI | IX | AW | AL;
    /// Any readable source (alterable + PC-relative + immediate + An).
    pub const ALL_SRC: u16 = ALL;
    /// Control modes (no Dn/An, no postinc/predec, no immediate) — e.g. `LEA`.
    pub const CONTROL: u16 = AI | DI | IX | AW | AL | PCD | PCX;
}

impl EaModes {
    #[must_use]
    pub fn allows(self, bit: u16) -> bool {
        self.0 & bit != 0
    }
}

/// One operand slot: the syntax it accepts and where its bits land in the
/// opcode word.
#[derive(Clone, Copy)]
pub enum Slot {
    /// A general effective address; its 6-bit field sits at `shift`. With
    /// `dest`, the MOVE destination layout is used (register in the high 3 bits,
    /// mode in the low 3) instead of the normal mode-then-register order.
    Ea { shift: u8, modes: EaModes, dest: bool },
    /// A data-register number (3 bits) at `shift`.
    Dn { shift: u8 },
    /// An address-register number (3 bits) at `shift`.
    An { shift: u8 },
    /// `MOVEQ`'s signed 8-bit immediate at bits 0–7.
    Quick8,
    /// A PC-relative branch displacement (`BRA`/`BSR`/`Bcc`), encoded as the
    /// word form: opcode byte stays `00`, a 16-bit displacement extension word
    /// follows. (Short-form selection is the Stage-2 optimizer's job.)
    BranchW,
    /// A `DBcc` displacement: always a 16-bit extension word.
    DispW,
}

/// One concrete encoding shape of a mnemonic.
pub struct Form {
    /// Fixed opcode-word bits; variable fields are `0` here and filled in.
    pub base: u16,
    pub size: SizeEnc,
    /// Operand slots, in source order.
    pub operands: &'static [Slot],
}

/// One mnemonic and its forms.
pub struct Insn {
    pub mnemonic: &'static str,
    pub summary: &'static str,
    pub forms: &'static [Form],
}

/// The 68000 instruction set (the curriculum subset, growing).
pub struct Spec {
    pub instructions: &'static [Insn],
}

impl Spec {
    #[must_use]
    pub fn instruction(&self, mnemonic: &str) -> Option<&Insn> {
        self.instructions.iter().find(|i| i.mnemonic == mnemonic)
    }
}

use ea::*;

/// Helper: a form with a normal source EA at bits 0–5.
const fn ea_src(modes: u16) -> Slot {
    Slot::Ea { shift: 0, modes: EaModes(modes), dest: false }
}

pub const SET: Spec = Spec {
    instructions: &[
        Insn {
            mnemonic: "RTS",
            summary: "Return from subroutine",
            forms: &[Form { base: 0x4E75, size: SizeEnc::Fixed(Size::W), operands: &[] }],
        },
        Insn {
            mnemonic: "NOP",
            summary: "No operation",
            forms: &[Form { base: 0x4E71, size: SizeEnc::Fixed(Size::W), operands: &[] }],
        },
        Insn {
            mnemonic: "MOVEQ",
            summary: "Move quick (sign-extended 8-bit to long)",
            forms: &[Form {
                base: 0x7000,
                size: SizeEnc::Fixed(Size::L),
                operands: &[Slot::Quick8, Slot::Dn { shift: 9 }],
            }],
        },
        Insn {
            mnemonic: "MOVE",
            summary: "Move data",
            forms: &[Form {
                base: 0x0000,
                size: SizeEnc::Move,
                operands: &[
                    ea_src(ALL),
                    Slot::Ea { shift: 6, modes: EaModes(DATA_ALT | AN), dest: true },
                ],
            }],
        },
        Insn {
            mnemonic: "LEA",
            summary: "Load effective address",
            forms: &[Form {
                base: 0x41C0,
                size: SizeEnc::Fixed(Size::L),
                operands: &[ea_src(CONTROL), Slot::An { shift: 9 }],
            }],
        },
        // ADD: <ea>,Dn (er) and Dn,<ea> (re).
        Insn {
            mnemonic: "ADD",
            summary: "Add binary",
            forms: &[
                Form {
                    base: 0xD000,
                    size: SizeEnc::Std6,
                    operands: &[ea_src(ALL), Slot::Dn { shift: 9 }],
                },
                Form {
                    base: 0xD100,
                    size: SizeEnc::Std6,
                    operands: &[Slot::Dn { shift: 9 }, ea_src(MEM_ALT)],
                },
            ],
        },
        Insn {
            mnemonic: "BRA",
            summary: "Branch always",
            forms: &[Form { base: 0x6000, size: SizeEnc::Fixed(Size::W), operands: &[Slot::BranchW] }],
        },
        Insn {
            mnemonic: "BSR",
            summary: "Branch to subroutine",
            forms: &[Form { base: 0x6100, size: SizeEnc::Fixed(Size::W), operands: &[Slot::BranchW] }],
        },
    ],
};
