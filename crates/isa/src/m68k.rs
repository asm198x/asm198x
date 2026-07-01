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
    /// A single word/long bit at `shift`: `0`=W, `1`=L (e.g. `EXT`, `MOVEM`).
    WL { shift: u8 },
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
    /// Data addressing: everything readable except An.
    pub const DATA: u16 = DN | AI | PI | PD | DI | IX | AW | AL | PCD | PCX | IMM;
    /// Data addressing minus immediate — a readable, non-immediate destination,
    /// e.g. the bit `BTST` tests (you cannot test a bit *in* a literal).
    pub const DATA_NOIMM: u16 = DN | AI | PI | PD | DI | IX | AW | AL | PCD | PCX;
    /// Alterable: writable destinations (no PC-relative, no immediate).
    pub const ALT: u16 = DN | AN | AI | PI | PD | DI | IX | AW | AL;
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
    Ea {
        shift: u8,
        modes: EaModes,
        dest: bool,
    },
    /// A data-register number (3 bits) at `shift`.
    Dn { shift: u8 },
    /// An address-register number (3 bits) at `shift`.
    An { shift: u8 },
    /// `MOVEQ`'s signed 8-bit immediate at bits 0–7.
    Quick8,
    /// A 3-bit quick immediate at `shift` (`ADDQ`/`SUBQ`): value 1–8, with 8
    /// encoded as `000`.
    Quick3 { shift: u8 },
    /// A PC-relative branch displacement (`BRA`/`BSR`/`Bcc`), encoded as the
    /// word form: opcode byte stays `00`, a 16-bit displacement extension word
    /// follows. (Short-form selection is the Stage-2 optimizer's job.)
    BranchW,
    /// A `DBcc` displacement: always a 16-bit extension word.
    DispW,
    /// An immediate emitted as a full 16-bit extension word (the static bit
    /// number of `BTST`/`BSET #n,<ea>`), placed before the EA's own extension.
    ImmWord,
    /// An immediate source sized by the instruction (`ADDI`/`SUBI`/`CMPI`): one
    /// extension word for byte/word, two for long. Carries no opcode-word bits;
    /// emitted before the destination EA's extension.
    ImmSized,
    /// A register-list mask (`MOVEM`): a 16-bit extension word, reversed when the
    /// effective address is predecrement.
    RegList,
    /// An address register named in a fixed indirect addressing mode — the
    /// register *number* sits in the opcode at `shift` (no 6-bit EA field, no
    /// extension word), and `mode` is the required EA mode: 4 = `-(An)`
    /// (`ADDX`/`SUBX`/`ABCD`/`SBCD` predecrement form), 3 = `(An)+` (`CMPM`).
    AddrIndirect { shift: u8, mode: u8 },
    /// A 4-bit immediate vector packed into the opcode's low nibble (`TRAP #v`,
    /// 0–15). No extension word.
    Vec4,
    /// The condition-code register operand (`ccr`). A fixed token — no opcode
    /// bits, no extension word.
    Ccr,
    /// The status register operand (`sr`). A fixed token, as [`Slot::Ccr`].
    Sr,
    /// The user stack pointer operand (`usp`). A fixed token, as [`Slot::Ccr`].
    Usp,
    /// `MOVEP`'s `d16(Ay)` operand: the address register sits in bits 0–2 (mode
    /// bits 3–5 are fixed `001` in the base), followed by a mandatory 16-bit
    /// displacement extension word. Distinct from a general `(d16,An)` EA — there
    /// is no 6-bit mode field, and the displacement is never dropped when zero.
    MovepDisp,
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
    Slot::Ea {
        shift: 0,
        modes: EaModes(modes),
        dest: false,
    }
}

pub const SET: Spec = Spec {
    instructions: &[
        Insn {
            mnemonic: "RTS",
            summary: "Return from subroutine",
            forms: &[Form {
                base: 0x4E75,
                size: SizeEnc::Fixed(Size::W),
                operands: &[],
            }],
        },
        Insn {
            mnemonic: "NOP",
            summary: "No operation",
            forms: &[Form {
                base: 0x4E71,
                size: SizeEnc::Fixed(Size::W),
                operands: &[],
            }],
        },
        // --- Control flow (68000-completeness burndown, family 1; see
        // decisions/68000-isa-completeness.md). JMP/JSR reuse LEA's control-
        // addressing EA; the no-operand returns mirror RTS. All unsized, so
        // Fixed(Size::W) — which renders no suffix. ---
        Insn {
            mnemonic: "JMP",
            summary: "Jump",
            forms: &[Form {
                base: 0x4EC0,
                size: SizeEnc::Fixed(Size::W),
                operands: &[ea_src(CONTROL)],
            }],
        },
        Insn {
            mnemonic: "JSR",
            summary: "Jump to subroutine",
            forms: &[Form {
                base: 0x4E80,
                size: SizeEnc::Fixed(Size::W),
                operands: &[ea_src(CONTROL)],
            }],
        },
        Insn {
            mnemonic: "RTE",
            summary: "Return from exception",
            forms: &[Form {
                base: 0x4E73,
                size: SizeEnc::Fixed(Size::W),
                operands: &[],
            }],
        },
        Insn {
            mnemonic: "RTR",
            summary: "Return and restore condition codes",
            forms: &[Form {
                base: 0x4E77,
                size: SizeEnc::Fixed(Size::W),
                operands: &[],
            }],
        },
        Insn {
            mnemonic: "TRAPV",
            summary: "Trap on overflow",
            forms: &[Form {
                base: 0x4E76,
                size: SizeEnc::Fixed(Size::W),
                operands: &[],
            }],
        },
        Insn {
            mnemonic: "TRAP",
            summary: "Trap",
            forms: &[Form {
                base: 0x4E40,
                size: SizeEnc::Fixed(Size::W),
                operands: &[Slot::Vec4],
            }],
        },
        Insn {
            mnemonic: "RESET",
            summary: "Reset external devices",
            forms: &[Form {
                base: 0x4E70,
                size: SizeEnc::Fixed(Size::W),
                operands: &[],
            }],
        },
        Insn {
            mnemonic: "ILLEGAL",
            summary: "Take the illegal-instruction trap",
            forms: &[Form {
                base: 0x4AFC,
                size: SizeEnc::Fixed(Size::W),
                operands: &[],
            }],
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
        // MOVEA — MOVE with an An destination (word/long only; `movea.b` is
        // illegal, caught by the An-byte rejection in the decoder). Listed before
        // MOVE so an An-destination move disassembles as `movea`, the faithful
        // mnemonic; MOVE keeps An in its destination modes so plain `move ...,An`
        // source still assembles (vasm accepts it as a movea alias).
        Insn {
            mnemonic: "MOVEA",
            summary: "Move address",
            forms: &[Form {
                // Same base as MOVE; the An-only destination EA encodes the
                // mode-001 field itself, and that restriction is what separates
                // the two forms during decode.
                base: 0x0000,
                size: SizeEnc::Move,
                operands: &[
                    ea_src(ALL),
                    Slot::Ea {
                        shift: 6,
                        modes: EaModes(AN),
                        dest: true,
                    },
                ],
            }],
        },
        Insn {
            mnemonic: "MOVE",
            summary: "Move data",
            forms: &[
                Form {
                    base: 0x0000,
                    size: SizeEnc::Move,
                    operands: &[
                        ea_src(ALL),
                        Slot::Ea {
                            shift: 6,
                            modes: EaModes(DATA_ALT | AN),
                            dest: true,
                        },
                    ],
                },
                // Control-register moves (always word-wide, no size suffix). The
                // `$x0C0`/`$x6C0` slots these occupy are the size-field-11 holes
                // of NEGX/NEG/NOT, so they never collide with those Std6 forms.
                // `move <ea>,ccr` / `move <ea>,sr` (to-CCR is 68000; `move
                // ccr,<ea>` from-CCR is 68010+, so it is intentionally absent).
                Form {
                    base: 0x44C0,
                    size: SizeEnc::Fixed(Size::W),
                    operands: &[ea_src(DATA), Slot::Ccr],
                },
                Form {
                    base: 0x46C0,
                    size: SizeEnc::Fixed(Size::W),
                    operands: &[ea_src(DATA), Slot::Sr],
                },
                // `move sr,<ea>` — the destination is data-alterable.
                Form {
                    base: 0x40C0,
                    size: SizeEnc::Fixed(Size::W),
                    operands: &[Slot::Sr, ea_src(DATA_ALT)],
                },
                // USP moves (privileged): `move usp,An` and `move An,usp`.
                Form {
                    base: 0x4E68,
                    size: SizeEnc::Fixed(Size::W),
                    operands: &[Slot::Usp, Slot::An { shift: 0 }],
                },
                Form {
                    base: 0x4E60,
                    size: SizeEnc::Fixed(Size::W),
                    operands: &[Slot::An { shift: 0 }, Slot::Usp],
                },
            ],
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
                // ADDA: an An destination (word/long only, size bit at 8).
                Form {
                    base: 0xD0C0,
                    size: SizeEnc::WL { shift: 8 },
                    operands: &[ea_src(ALL), Slot::An { shift: 9 }],
                },
            ],
        },
        // ADDI/SUBI/CMPI are *distinct mnemonics*, not forms of ADD/SUB/CMP:
        // vasm assembles `add #imm,Dn` to the ADD-with-immediate-EA encoding
        // ($D03C…) and only `addi` (or `add #imm,<mem>`, which the dialect
        // aliases) to $06xx. Keeping them separate is what lets the disassembler
        // render `$06xx` as `addi` so it round-trips.
        Insn {
            mnemonic: "ADDI",
            summary: "Add immediate",
            forms: &[Form {
                base: 0x0600,
                size: SizeEnc::Std6,
                operands: &[Slot::ImmSized, ea_src(DATA_ALT)],
            }],
        },
        Insn {
            mnemonic: "SUBI",
            summary: "Subtract immediate",
            forms: &[Form {
                base: 0x0400,
                size: SizeEnc::Std6,
                operands: &[Slot::ImmSized, ea_src(DATA_ALT)],
            }],
        },
        Insn {
            mnemonic: "CMPI",
            summary: "Compare immediate",
            forms: &[Form {
                base: 0x0C00,
                size: SizeEnc::Std6,
                operands: &[Slot::ImmSized, ea_src(DATA_ALT)],
            }],
        },
        // ORI/ANDI/EORI — the bitwise counterparts of ADDI/SUBI/CMPI, and
        // distinct mnemonics for the same reason: `or`/`and #imm,Dn` assemble to
        // the immediate-source EA encoding ($807C/$C07C…), so only `ori`/`andi`
        // reach $00xx/$02xx. `eor #imm` has no immediate-source form, so vasm
        // routes it to EORI ($0Axx) — which the disassembler emits, closing the
        // round-trip without a dialect alias. (The CCR/SR target forms — $003C,
        // $007C, … — need a dedicated operand slot and are not yet modelled.)
        // ORI/ANDI/EORI carry a status-register variant: the `#imm,<ea>` form
        // with the immediate-EA bit pattern ($x03C/$x07C) is illegal as a normal
        // EA (immediate isn't alterable), so it is repurposed for `#imm,CCR`
        // (byte, low half of the word) and `#imm,SR` (word). Rendered suffixless,
        // so the size is `Fixed(W)` and the immediate is a single `ImmWord`.
        Insn {
            mnemonic: "ORI",
            summary: "Inclusive-OR immediate",
            forms: &[
                Form {
                    base: 0x0000,
                    size: SizeEnc::Std6,
                    operands: &[Slot::ImmSized, ea_src(DATA_ALT)],
                },
                Form {
                    base: 0x003C,
                    size: SizeEnc::Fixed(Size::W),
                    operands: &[Slot::ImmWord, Slot::Ccr],
                },
                Form {
                    base: 0x007C,
                    size: SizeEnc::Fixed(Size::W),
                    operands: &[Slot::ImmWord, Slot::Sr],
                },
            ],
        },
        Insn {
            mnemonic: "ANDI",
            summary: "AND immediate",
            forms: &[
                Form {
                    base: 0x0200,
                    size: SizeEnc::Std6,
                    operands: &[Slot::ImmSized, ea_src(DATA_ALT)],
                },
                Form {
                    base: 0x023C,
                    size: SizeEnc::Fixed(Size::W),
                    operands: &[Slot::ImmWord, Slot::Ccr],
                },
                Form {
                    base: 0x027C,
                    size: SizeEnc::Fixed(Size::W),
                    operands: &[Slot::ImmWord, Slot::Sr],
                },
            ],
        },
        Insn {
            mnemonic: "EORI",
            summary: "Exclusive-OR immediate",
            forms: &[
                Form {
                    base: 0x0A00,
                    size: SizeEnc::Std6,
                    operands: &[Slot::ImmSized, ea_src(DATA_ALT)],
                },
                Form {
                    base: 0x0A3C,
                    size: SizeEnc::Fixed(Size::W),
                    operands: &[Slot::ImmWord, Slot::Ccr],
                },
                Form {
                    base: 0x0A7C,
                    size: SizeEnc::Fixed(Size::W),
                    operands: &[Slot::ImmWord, Slot::Sr],
                },
            ],
        },
        Insn {
            mnemonic: "BRA",
            summary: "Branch always",
            forms: &[Form {
                base: 0x6000,
                size: SizeEnc::Fixed(Size::W),
                operands: &[Slot::BranchW],
            }],
        },
        Insn {
            mnemonic: "BSR",
            summary: "Branch to subroutine",
            forms: &[Form {
                base: 0x6100,
                size: SizeEnc::Fixed(Size::W),
                operands: &[Slot::BranchW],
            }],
        },
        // --- Conditional branches (condition in bits 8–11) ---
        Insn {
            mnemonic: "BEQ",
            summary: "Branch if equal",
            forms: BRANCH_EQ,
        },
        Insn {
            mnemonic: "BNE",
            summary: "Branch if not equal",
            forms: BRANCH_NE,
        },
        // Remaining Bcc condition codes (mirror the others; cc in bits 8–11).
        Insn {
            mnemonic: "BHI",
            summary: "Branch if higher",
            forms: BRANCH_HI,
        },
        Insn {
            mnemonic: "BLS",
            summary: "Branch if lower or same",
            forms: BRANCH_LS,
        },
        Insn {
            mnemonic: "BCC",
            summary: "Branch if carry clear (higher or same)",
            forms: BRANCH_CC,
        },
        Insn {
            mnemonic: "BCS",
            summary: "Branch if carry set (lower)",
            forms: BRANCH_CS,
        },
        Insn {
            mnemonic: "BVC",
            summary: "Branch if overflow clear",
            forms: BRANCH_VC,
        },
        Insn {
            mnemonic: "BVS",
            summary: "Branch if overflow set",
            forms: BRANCH_VS,
        },
        Insn {
            mnemonic: "BGE",
            summary: "Branch if greater or equal",
            forms: BRANCH_GE,
        },
        Insn {
            mnemonic: "BLT",
            summary: "Branch if less than",
            forms: BRANCH_LT,
        },
        Insn {
            mnemonic: "BGT",
            summary: "Branch if greater than",
            forms: BRANCH_GT,
        },
        Insn {
            mnemonic: "BLE",
            summary: "Branch if less or equal",
            forms: BRANCH_LE,
        },
        Insn {
            mnemonic: "BMI",
            summary: "Branch if minus",
            forms: BRANCH_MI,
        },
        Insn {
            mnemonic: "BPL",
            summary: "Branch if plus",
            forms: BRANCH_PL,
        },
        // --- Two-operand arithmetic/logic: <ea>,Dn (er) and Dn,<ea> (re) ---
        Insn {
            mnemonic: "SUB",
            summary: "Subtract binary",
            forms: &[
                Form {
                    base: 0x9000,
                    size: SizeEnc::Std6,
                    operands: &[ea_src(ALL), Slot::Dn { shift: 9 }],
                },
                Form {
                    base: 0x9100,
                    size: SizeEnc::Std6,
                    operands: &[Slot::Dn { shift: 9 }, ea_src(MEM_ALT)],
                },
                // SUBA: an An destination (word/long only, size bit at 8).
                Form {
                    base: 0x90C0,
                    size: SizeEnc::WL { shift: 8 },
                    operands: &[ea_src(ALL), Slot::An { shift: 9 }],
                },
            ],
        },
        Insn {
            mnemonic: "AND",
            summary: "Logical AND",
            forms: &[
                Form {
                    base: 0xC000,
                    size: SizeEnc::Std6,
                    operands: &[ea_src(DATA), Slot::Dn { shift: 9 }],
                },
                Form {
                    base: 0xC100,
                    size: SizeEnc::Std6,
                    operands: &[Slot::Dn { shift: 9 }, ea_src(MEM_ALT)],
                },
            ],
        },
        Insn {
            mnemonic: "OR",
            summary: "Logical OR",
            forms: &[
                Form {
                    base: 0x8000,
                    size: SizeEnc::Std6,
                    operands: &[ea_src(DATA), Slot::Dn { shift: 9 }],
                },
                Form {
                    base: 0x8100,
                    size: SizeEnc::Std6,
                    operands: &[Slot::Dn { shift: 9 }, ea_src(MEM_ALT)],
                },
            ],
        },
        Insn {
            mnemonic: "CMP",
            summary: "Compare",
            forms: &[
                Form {
                    base: 0xB000,
                    size: SizeEnc::Std6,
                    operands: &[ea_src(ALL), Slot::Dn { shift: 9 }],
                },
                // CMPA: an An destination (word/long only, size bit at 8).
                Form {
                    base: 0xB0C0,
                    size: SizeEnc::WL { shift: 8 },
                    operands: &[ea_src(ALL), Slot::An { shift: 9 }],
                },
            ],
        },
        Insn {
            mnemonic: "EOR",
            summary: "Exclusive OR",
            forms: &[Form {
                base: 0xB100,
                size: SizeEnc::Std6,
                operands: &[Slot::Dn { shift: 9 }, ea_src(DATA_ALT)],
            }],
        },
        Insn {
            mnemonic: "MULU",
            summary: "Unsigned multiply",
            forms: &[Form {
                base: 0xC0C0,
                size: SizeEnc::Fixed(Size::W),
                operands: &[ea_src(DATA), Slot::Dn { shift: 9 }],
            }],
        },
        Insn {
            mnemonic: "DIVU",
            summary: "Unsigned divide",
            forms: &[Form {
                base: 0x80C0,
                size: SizeEnc::Fixed(Size::W),
                operands: &[ea_src(DATA), Slot::Dn { shift: 9 }],
            }],
        },
        // Signed multiply/divide — mirror MULU/DIVU (opmode 111 vs 011).
        Insn {
            mnemonic: "MULS",
            summary: "Signed multiply",
            forms: &[Form {
                base: 0xC1C0,
                size: SizeEnc::Fixed(Size::W),
                operands: &[ea_src(DATA), Slot::Dn { shift: 9 }],
            }],
        },
        Insn {
            mnemonic: "DIVS",
            summary: "Signed divide",
            forms: &[Form {
                base: 0x81C0,
                size: SizeEnc::Fixed(Size::W),
                operands: &[ea_src(DATA), Slot::Dn { shift: 9 }],
            }],
        },
        // --- Extended/BCD arithmetic and CMPM ---
        // Each takes either two data registers (`Dn,Dn`) or two predecrement
        // address registers (`-(An),-(An)`); the mode bit (3) picks between them.
        // ADDX/SUBX are size-coded (Std6); ABCD/SBCD are byte-only. CMPM is the
        // postincrement-only memory compare (`(An)+,(An)+`).
        Insn {
            mnemonic: "ADDX",
            summary: "Add extended",
            forms: &[
                Form {
                    base: 0xD100,
                    size: SizeEnc::Std6,
                    operands: &[Slot::Dn { shift: 0 }, Slot::Dn { shift: 9 }],
                },
                Form {
                    base: 0xD108,
                    size: SizeEnc::Std6,
                    operands: &[
                        Slot::AddrIndirect { shift: 0, mode: 4 },
                        Slot::AddrIndirect { shift: 9, mode: 4 },
                    ],
                },
            ],
        },
        Insn {
            mnemonic: "SUBX",
            summary: "Subtract extended",
            forms: &[
                Form {
                    base: 0x9100,
                    size: SizeEnc::Std6,
                    operands: &[Slot::Dn { shift: 0 }, Slot::Dn { shift: 9 }],
                },
                Form {
                    base: 0x9108,
                    size: SizeEnc::Std6,
                    operands: &[
                        Slot::AddrIndirect { shift: 0, mode: 4 },
                        Slot::AddrIndirect { shift: 9, mode: 4 },
                    ],
                },
            ],
        },
        Insn {
            mnemonic: "ABCD",
            summary: "Add decimal with extend",
            forms: &[
                Form {
                    base: 0xC100,
                    size: SizeEnc::Fixed(Size::B),
                    operands: &[Slot::Dn { shift: 0 }, Slot::Dn { shift: 9 }],
                },
                Form {
                    base: 0xC108,
                    size: SizeEnc::Fixed(Size::B),
                    operands: &[
                        Slot::AddrIndirect { shift: 0, mode: 4 },
                        Slot::AddrIndirect { shift: 9, mode: 4 },
                    ],
                },
            ],
        },
        Insn {
            mnemonic: "SBCD",
            summary: "Subtract decimal with extend",
            forms: &[
                Form {
                    base: 0x8100,
                    size: SizeEnc::Fixed(Size::B),
                    operands: &[Slot::Dn { shift: 0 }, Slot::Dn { shift: 9 }],
                },
                Form {
                    base: 0x8108,
                    size: SizeEnc::Fixed(Size::B),
                    operands: &[
                        Slot::AddrIndirect { shift: 0, mode: 4 },
                        Slot::AddrIndirect { shift: 9, mode: 4 },
                    ],
                },
            ],
        },
        Insn {
            mnemonic: "CMPM",
            summary: "Compare memory with postincrement",
            forms: &[Form {
                base: 0xB108,
                size: SizeEnc::Std6,
                operands: &[
                    Slot::AddrIndirect { shift: 0, mode: 3 },
                    Slot::AddrIndirect { shift: 9, mode: 3 },
                ],
            }],
        },
        // EXG — exchange a full pair of registers (always long; no size suffix).
        // The opmode field (bits 6–8) names the pair kind: 01000 = Dx,Dy;
        // 01001 = Ax,Ay; 10001 = Dx,Ay. The first register sits at bit 9, the
        // second at bit 0. The reversed `Ay,Dx` source order maps to the same
        // `Dx,Ay` encoding (a fourth, assemble-only form); the Dx,Ay form is
        // listed first so it wins the decode and renders the canonical order.
        Insn {
            mnemonic: "EXG",
            summary: "Exchange registers",
            forms: &[
                Form {
                    base: 0xC140,
                    size: SizeEnc::Fixed(Size::W),
                    operands: &[Slot::Dn { shift: 9 }, Slot::Dn { shift: 0 }],
                },
                Form {
                    base: 0xC148,
                    size: SizeEnc::Fixed(Size::W),
                    operands: &[Slot::An { shift: 9 }, Slot::An { shift: 0 }],
                },
                Form {
                    base: 0xC188,
                    size: SizeEnc::Fixed(Size::W),
                    operands: &[Slot::Dn { shift: 9 }, Slot::An { shift: 0 }],
                },
                Form {
                    base: 0xC188,
                    size: SizeEnc::Fixed(Size::W),
                    operands: &[Slot::An { shift: 0 }, Slot::Dn { shift: 9 }],
                },
            ],
        },
        // --- Single effective-address operations ---
        Insn {
            mnemonic: "TST",
            summary: "Test (set flags)",
            forms: &[Form {
                base: 0x4A00,
                size: SizeEnc::Std6,
                operands: &[ea_src(DATA_ALT)],
            }],
        },
        Insn {
            mnemonic: "CLR",
            summary: "Clear",
            forms: &[Form {
                base: 0x4200,
                size: SizeEnc::Std6,
                operands: &[ea_src(DATA_ALT)],
            }],
        },
        Insn {
            mnemonic: "NEG",
            summary: "Negate",
            forms: &[Form {
                base: 0x4400,
                size: SizeEnc::Std6,
                operands: &[ea_src(DATA_ALT)],
            }],
        },
        Insn {
            mnemonic: "NOT",
            summary: "Logical complement",
            forms: &[Form {
                base: 0x4600,
                size: SizeEnc::Std6,
                operands: &[ea_src(DATA_ALT)],
            }],
        },
        // More single-operand ops that reuse existing slots (no new Slot kind).
        Insn {
            mnemonic: "NEGX",
            summary: "Negate with extend",
            forms: &[Form {
                base: 0x4000,
                size: SizeEnc::Std6,
                operands: &[ea_src(DATA_ALT)],
            }],
        },
        Insn {
            mnemonic: "NBCD",
            summary: "Negate BCD with extend",
            forms: &[Form {
                base: 0x4800,
                size: SizeEnc::Fixed(Size::B),
                operands: &[ea_src(DATA_ALT)],
            }],
        },
        Insn {
            mnemonic: "TAS",
            summary: "Test and set (atomic)",
            forms: &[Form {
                base: 0x4AC0,
                size: SizeEnc::Fixed(Size::B),
                operands: &[ea_src(DATA_ALT)],
            }],
        },
        Insn {
            mnemonic: "PEA",
            summary: "Push effective address",
            forms: &[Form {
                base: 0x4840,
                size: SizeEnc::Fixed(Size::L),
                operands: &[ea_src(CONTROL)],
            }],
        },
        Insn {
            mnemonic: "LINK",
            summary: "Link and allocate stack frame",
            forms: &[Form {
                base: 0x4E50,
                size: SizeEnc::Fixed(Size::W),
                operands: &[Slot::An { shift: 0 }, Slot::ImmWord],
            }],
        },
        Insn {
            mnemonic: "UNLK",
            summary: "Unlink stack frame",
            forms: &[Form {
                base: 0x4E58,
                size: SizeEnc::Fixed(Size::W),
                operands: &[Slot::An { shift: 0 }],
            }],
        },
        Insn {
            mnemonic: "STOP",
            summary: "Load status register and stop",
            forms: &[Form {
                base: 0x4E72,
                size: SizeEnc::Fixed(Size::W),
                operands: &[Slot::ImmWord],
            }],
        },
        Insn {
            mnemonic: "CHK",
            summary: "Check register against bounds",
            forms: &[Form {
                base: 0x4180,
                size: SizeEnc::Fixed(Size::W),
                operands: &[ea_src(DATA), Slot::Dn { shift: 9 }],
            }],
        },
        // --- Quick-immediate add/subtract (1–8) ---
        Insn {
            mnemonic: "ADDQ",
            summary: "Add quick",
            forms: &[Form {
                base: 0x5000,
                size: SizeEnc::Std6,
                operands: &[Slot::Quick3 { shift: 9 }, ea_src(ALT)],
            }],
        },
        Insn {
            mnemonic: "SUBQ",
            summary: "Subtract quick",
            forms: &[Form {
                base: 0x5100,
                size: SizeEnc::Std6,
                operands: &[Slot::Quick3 { shift: 9 }, ea_src(ALT)],
            }],
        },
        // --- Register operations ---
        Insn {
            mnemonic: "SWAP",
            summary: "Swap register halves",
            forms: &[Form {
                base: 0x4840,
                size: SizeEnc::Fixed(Size::W),
                operands: &[Slot::Dn { shift: 0 }],
            }],
        },
        Insn {
            mnemonic: "EXT",
            summary: "Sign-extend",
            forms: &[Form {
                base: 0x4880,
                size: SizeEnc::WL { shift: 6 },
                operands: &[Slot::Dn { shift: 0 }],
            }],
        },
        // --- Decrement-and-branch (counter in bits 0–2, 16-bit displacement) ---
        Insn {
            mnemonic: "DBF",
            summary: "Decrement and branch (never on condition)",
            forms: &[Form {
                base: 0x51C8,
                size: SizeEnc::Fixed(Size::W),
                operands: &[Slot::Dn { shift: 0 }, Slot::DispW],
            }],
        },
        Insn {
            mnemonic: "DBRA",
            summary: "Decrement and branch (alias of DBF)",
            forms: &[Form {
                base: 0x51C8,
                size: SizeEnc::Fixed(Size::W),
                operands: &[Slot::Dn { shift: 0 }, Slot::DispW],
            }],
        },
        // Remaining DBcc condition codes (mirror DBF; cc in bits 8–11). DBcc
        // decrements and branches while the condition is false.
        Insn {
            mnemonic: "DBT",
            summary: "Decrement and branch unless true",
            forms: &[Form {
                base: 0x50C8,
                size: SizeEnc::Fixed(Size::W),
                operands: &[Slot::Dn { shift: 0 }, Slot::DispW],
            }],
        },
        Insn {
            mnemonic: "DBHI",
            summary: "Decrement and branch unless higher",
            forms: &[Form {
                base: 0x52C8,
                size: SizeEnc::Fixed(Size::W),
                operands: &[Slot::Dn { shift: 0 }, Slot::DispW],
            }],
        },
        Insn {
            mnemonic: "DBLS",
            summary: "Decrement and branch unless lower or same",
            forms: &[Form {
                base: 0x53C8,
                size: SizeEnc::Fixed(Size::W),
                operands: &[Slot::Dn { shift: 0 }, Slot::DispW],
            }],
        },
        Insn {
            mnemonic: "DBCC",
            summary: "Decrement and branch unless carry clear",
            forms: &[Form {
                base: 0x54C8,
                size: SizeEnc::Fixed(Size::W),
                operands: &[Slot::Dn { shift: 0 }, Slot::DispW],
            }],
        },
        Insn {
            mnemonic: "DBCS",
            summary: "Decrement and branch unless carry set",
            forms: &[Form {
                base: 0x55C8,
                size: SizeEnc::Fixed(Size::W),
                operands: &[Slot::Dn { shift: 0 }, Slot::DispW],
            }],
        },
        Insn {
            mnemonic: "DBNE",
            summary: "Decrement and branch unless not equal",
            forms: &[Form {
                base: 0x56C8,
                size: SizeEnc::Fixed(Size::W),
                operands: &[Slot::Dn { shift: 0 }, Slot::DispW],
            }],
        },
        Insn {
            mnemonic: "DBEQ",
            summary: "Decrement and branch unless equal",
            forms: &[Form {
                base: 0x57C8,
                size: SizeEnc::Fixed(Size::W),
                operands: &[Slot::Dn { shift: 0 }, Slot::DispW],
            }],
        },
        Insn {
            mnemonic: "DBVC",
            summary: "Decrement and branch unless overflow clear",
            forms: &[Form {
                base: 0x58C8,
                size: SizeEnc::Fixed(Size::W),
                operands: &[Slot::Dn { shift: 0 }, Slot::DispW],
            }],
        },
        Insn {
            mnemonic: "DBVS",
            summary: "Decrement and branch unless overflow set",
            forms: &[Form {
                base: 0x59C8,
                size: SizeEnc::Fixed(Size::W),
                operands: &[Slot::Dn { shift: 0 }, Slot::DispW],
            }],
        },
        Insn {
            mnemonic: "DBPL",
            summary: "Decrement and branch unless plus",
            forms: &[Form {
                base: 0x5AC8,
                size: SizeEnc::Fixed(Size::W),
                operands: &[Slot::Dn { shift: 0 }, Slot::DispW],
            }],
        },
        Insn {
            mnemonic: "DBMI",
            summary: "Decrement and branch unless minus",
            forms: &[Form {
                base: 0x5BC8,
                size: SizeEnc::Fixed(Size::W),
                operands: &[Slot::Dn { shift: 0 }, Slot::DispW],
            }],
        },
        Insn {
            mnemonic: "DBGE",
            summary: "Decrement and branch unless greater or equal",
            forms: &[Form {
                base: 0x5CC8,
                size: SizeEnc::Fixed(Size::W),
                operands: &[Slot::Dn { shift: 0 }, Slot::DispW],
            }],
        },
        Insn {
            mnemonic: "DBLT",
            summary: "Decrement and branch unless less than",
            forms: &[Form {
                base: 0x5DC8,
                size: SizeEnc::Fixed(Size::W),
                operands: &[Slot::Dn { shift: 0 }, Slot::DispW],
            }],
        },
        Insn {
            mnemonic: "DBGT",
            summary: "Decrement and branch unless greater than",
            forms: &[Form {
                base: 0x5EC8,
                size: SizeEnc::Fixed(Size::W),
                operands: &[Slot::Dn { shift: 0 }, Slot::DispW],
            }],
        },
        Insn {
            mnemonic: "DBLE",
            summary: "Decrement and branch unless less or equal",
            forms: &[Form {
                base: 0x5FC8,
                size: SizeEnc::Fixed(Size::W),
                operands: &[Slot::Dn { shift: 0 }, Slot::DispW],
            }],
        },
        // --- Set-on-condition (byte effective address) ---
        Insn {
            mnemonic: "SNE",
            summary: "Set if not equal",
            forms: &[Form {
                base: 0x56C0,
                size: SizeEnc::Fixed(Size::B),
                operands: &[ea_src(DATA_ALT)],
            }],
        },
        Insn {
            mnemonic: "SEQ",
            summary: "Set if equal",
            forms: &[Form {
                base: 0x57C0,
                size: SizeEnc::Fixed(Size::B),
                operands: &[ea_src(DATA_ALT)],
            }],
        },
        // Remaining Scc condition codes (mirror SEQ/SNE; cc in bits 8–11).
        Insn {
            mnemonic: "ST",
            summary: "Set always",
            forms: &[Form {
                base: 0x50C0,
                size: SizeEnc::Fixed(Size::B),
                operands: &[ea_src(DATA_ALT)],
            }],
        },
        Insn {
            mnemonic: "SF",
            summary: "Set never (clear)",
            forms: &[Form {
                base: 0x51C0,
                size: SizeEnc::Fixed(Size::B),
                operands: &[ea_src(DATA_ALT)],
            }],
        },
        Insn {
            mnemonic: "SHI",
            summary: "Set if higher",
            forms: &[Form {
                base: 0x52C0,
                size: SizeEnc::Fixed(Size::B),
                operands: &[ea_src(DATA_ALT)],
            }],
        },
        Insn {
            mnemonic: "SLS",
            summary: "Set if lower or same",
            forms: &[Form {
                base: 0x53C0,
                size: SizeEnc::Fixed(Size::B),
                operands: &[ea_src(DATA_ALT)],
            }],
        },
        Insn {
            mnemonic: "SCC",
            summary: "Set if carry clear (higher or same)",
            forms: &[Form {
                base: 0x54C0,
                size: SizeEnc::Fixed(Size::B),
                operands: &[ea_src(DATA_ALT)],
            }],
        },
        Insn {
            mnemonic: "SCS",
            summary: "Set if carry set (lower)",
            forms: &[Form {
                base: 0x55C0,
                size: SizeEnc::Fixed(Size::B),
                operands: &[ea_src(DATA_ALT)],
            }],
        },
        Insn {
            mnemonic: "SVC",
            summary: "Set if overflow clear",
            forms: &[Form {
                base: 0x58C0,
                size: SizeEnc::Fixed(Size::B),
                operands: &[ea_src(DATA_ALT)],
            }],
        },
        Insn {
            mnemonic: "SVS",
            summary: "Set if overflow set",
            forms: &[Form {
                base: 0x59C0,
                size: SizeEnc::Fixed(Size::B),
                operands: &[ea_src(DATA_ALT)],
            }],
        },
        Insn {
            mnemonic: "SPL",
            summary: "Set if plus",
            forms: &[Form {
                base: 0x5AC0,
                size: SizeEnc::Fixed(Size::B),
                operands: &[ea_src(DATA_ALT)],
            }],
        },
        Insn {
            mnemonic: "SMI",
            summary: "Set if minus",
            forms: &[Form {
                base: 0x5BC0,
                size: SizeEnc::Fixed(Size::B),
                operands: &[ea_src(DATA_ALT)],
            }],
        },
        Insn {
            mnemonic: "SGE",
            summary: "Set if greater or equal",
            forms: &[Form {
                base: 0x5CC0,
                size: SizeEnc::Fixed(Size::B),
                operands: &[ea_src(DATA_ALT)],
            }],
        },
        Insn {
            mnemonic: "SLT",
            summary: "Set if less than",
            forms: &[Form {
                base: 0x5DC0,
                size: SizeEnc::Fixed(Size::B),
                operands: &[ea_src(DATA_ALT)],
            }],
        },
        Insn {
            mnemonic: "SGT",
            summary: "Set if greater than",
            forms: &[Form {
                base: 0x5EC0,
                size: SizeEnc::Fixed(Size::B),
                operands: &[ea_src(DATA_ALT)],
            }],
        },
        Insn {
            mnemonic: "SLE",
            summary: "Set if less or equal",
            forms: &[Form {
                base: 0x5FC0,
                size: SizeEnc::Fixed(Size::B),
                operands: &[ea_src(DATA_ALT)],
            }],
        },
        // --- Logical shifts: #count,Dn (immediate) and Dn,Dn (register) ---
        Insn {
            mnemonic: "LSL",
            summary: "Logical shift left",
            forms: &[
                Form {
                    base: 0xE108,
                    size: SizeEnc::Std6,
                    operands: &[Slot::Quick3 { shift: 9 }, Slot::Dn { shift: 0 }],
                },
                Form {
                    base: 0xE128,
                    size: SizeEnc::Std6,
                    operands: &[Slot::Dn { shift: 9 }, Slot::Dn { shift: 0 }],
                },
                Form {
                    base: 0xE3C0,
                    size: SizeEnc::Fixed(Size::W),
                    operands: &[ea_src(MEM_ALT)],
                },
            ],
        },
        Insn {
            mnemonic: "LSR",
            summary: "Logical shift right",
            forms: &[
                Form {
                    base: 0xE008,
                    size: SizeEnc::Std6,
                    operands: &[Slot::Quick3 { shift: 9 }, Slot::Dn { shift: 0 }],
                },
                Form {
                    base: 0xE028,
                    size: SizeEnc::Std6,
                    operands: &[Slot::Dn { shift: 9 }, Slot::Dn { shift: 0 }],
                },
                Form {
                    base: 0xE2C0,
                    size: SizeEnc::Fixed(Size::W),
                    operands: &[ea_src(MEM_ALT)],
                },
            ],
        },
        // Shifts/rotates (register forms) — mirror LSL/LSR; the type field is
        // bits 4-3 (00=AS, 01=LS, 10=ROX, 11=RO), direction is bit 8, and bit 5
        // selects immediate-count (`Quick3`) vs register-count (`Dn`).
        Insn {
            mnemonic: "ASR",
            summary: "Arithmetic shift right",
            forms: &[
                Form {
                    base: 0xE000,
                    size: SizeEnc::Std6,
                    operands: &[Slot::Quick3 { shift: 9 }, Slot::Dn { shift: 0 }],
                },
                Form {
                    base: 0xE020,
                    size: SizeEnc::Std6,
                    operands: &[Slot::Dn { shift: 9 }, Slot::Dn { shift: 0 }],
                },
                Form {
                    base: 0xE0C0,
                    size: SizeEnc::Fixed(Size::W),
                    operands: &[ea_src(MEM_ALT)],
                },
            ],
        },
        Insn {
            mnemonic: "ASL",
            summary: "Arithmetic shift left",
            forms: &[
                Form {
                    base: 0xE100,
                    size: SizeEnc::Std6,
                    operands: &[Slot::Quick3 { shift: 9 }, Slot::Dn { shift: 0 }],
                },
                Form {
                    base: 0xE120,
                    size: SizeEnc::Std6,
                    operands: &[Slot::Dn { shift: 9 }, Slot::Dn { shift: 0 }],
                },
                Form {
                    base: 0xE1C0,
                    size: SizeEnc::Fixed(Size::W),
                    operands: &[ea_src(MEM_ALT)],
                },
            ],
        },
        Insn {
            mnemonic: "ROXR",
            summary: "Rotate right through extend",
            forms: &[
                Form {
                    base: 0xE010,
                    size: SizeEnc::Std6,
                    operands: &[Slot::Quick3 { shift: 9 }, Slot::Dn { shift: 0 }],
                },
                Form {
                    base: 0xE030,
                    size: SizeEnc::Std6,
                    operands: &[Slot::Dn { shift: 9 }, Slot::Dn { shift: 0 }],
                },
                Form {
                    base: 0xE4C0,
                    size: SizeEnc::Fixed(Size::W),
                    operands: &[ea_src(MEM_ALT)],
                },
            ],
        },
        Insn {
            mnemonic: "ROXL",
            summary: "Rotate left through extend",
            forms: &[
                Form {
                    base: 0xE110,
                    size: SizeEnc::Std6,
                    operands: &[Slot::Quick3 { shift: 9 }, Slot::Dn { shift: 0 }],
                },
                Form {
                    base: 0xE130,
                    size: SizeEnc::Std6,
                    operands: &[Slot::Dn { shift: 9 }, Slot::Dn { shift: 0 }],
                },
                Form {
                    base: 0xE5C0,
                    size: SizeEnc::Fixed(Size::W),
                    operands: &[ea_src(MEM_ALT)],
                },
            ],
        },
        Insn {
            mnemonic: "ROR",
            summary: "Rotate right",
            forms: &[
                Form {
                    base: 0xE018,
                    size: SizeEnc::Std6,
                    operands: &[Slot::Quick3 { shift: 9 }, Slot::Dn { shift: 0 }],
                },
                Form {
                    base: 0xE038,
                    size: SizeEnc::Std6,
                    operands: &[Slot::Dn { shift: 9 }, Slot::Dn { shift: 0 }],
                },
                Form {
                    base: 0xE6C0,
                    size: SizeEnc::Fixed(Size::W),
                    operands: &[ea_src(MEM_ALT)],
                },
            ],
        },
        Insn {
            mnemonic: "ROL",
            summary: "Rotate left",
            forms: &[
                Form {
                    base: 0xE118,
                    size: SizeEnc::Std6,
                    operands: &[Slot::Quick3 { shift: 9 }, Slot::Dn { shift: 0 }],
                },
                Form {
                    base: 0xE138,
                    size: SizeEnc::Std6,
                    operands: &[Slot::Dn { shift: 9 }, Slot::Dn { shift: 0 }],
                },
                Form {
                    base: 0xE7C0,
                    size: SizeEnc::Fixed(Size::W),
                    operands: &[ea_src(MEM_ALT)],
                },
            ],
        },
        // --- Bit test/set: #n,<ea> (static) and Dn,<ea> (dynamic) ---
        Insn {
            mnemonic: "BTST",
            summary: "Test a bit",
            forms: &[
                // Static `BTST #bit,<ea>` — immediate destination is illegal
                // (Musashi `btst_s` mask `0xbfb` clears the immediate bit).
                Form {
                    base: 0x0800,
                    size: SizeEnc::Fixed(Size::B),
                    operands: &[Slot::ImmWord, ea_src(DATA_NOIMM)],
                },
                // Dynamic `BTST Dn,<ea>` — alone among the bit ops, this reads
                // its destination, so an immediate is a legal EA: `btst dN,#imm`
                // tests a bit of a literal (Musashi `btst_r` mask `0xbff`
                // includes the immediate bit; the MC68000 PRM lists it). Hence
                // `DATA` (= `DATA_NOIMM | IMM`) rather than the bit ops' usual
                // `DATA_NOIMM`.
                Form {
                    base: 0x0100,
                    size: SizeEnc::Fixed(Size::B),
                    operands: &[Slot::Dn { shift: 9 }, ea_src(DATA)],
                },
            ],
        },
        Insn {
            mnemonic: "BSET",
            summary: "Test and set a bit",
            forms: &[
                Form {
                    base: 0x08C0,
                    size: SizeEnc::Fixed(Size::B),
                    operands: &[Slot::ImmWord, ea_src(DATA_ALT)],
                },
                Form {
                    base: 0x01C0,
                    size: SizeEnc::Fixed(Size::B),
                    operands: &[Slot::Dn { shift: 9 }, ea_src(DATA_ALT)],
                },
            ],
        },
        // BCHG/BCLR — mirror BSET (cc bits 7-6 = 01/10 vs BSET's 11); alterable
        // EA only, so DATA_ALT like BSET (BTST is the read-only one).
        Insn {
            mnemonic: "BCHG",
            summary: "Test and change a bit",
            forms: &[
                Form {
                    base: 0x0840,
                    size: SizeEnc::Fixed(Size::B),
                    operands: &[Slot::ImmWord, ea_src(DATA_ALT)],
                },
                Form {
                    base: 0x0140,
                    size: SizeEnc::Fixed(Size::B),
                    operands: &[Slot::Dn { shift: 9 }, ea_src(DATA_ALT)],
                },
            ],
        },
        Insn {
            mnemonic: "BCLR",
            summary: "Test and clear a bit",
            forms: &[
                Form {
                    base: 0x0880,
                    size: SizeEnc::Fixed(Size::B),
                    operands: &[Slot::ImmWord, ea_src(DATA_ALT)],
                },
                Form {
                    base: 0x0180,
                    size: SizeEnc::Fixed(Size::B),
                    operands: &[Slot::Dn { shift: 9 }, ea_src(DATA_ALT)],
                },
            ],
        },
        // --- Move multiple registers (mask word; reversed for predecrement) ---
        Insn {
            mnemonic: "MOVEM",
            summary: "Move multiple registers",
            forms: &[
                // store: reglist -> memory. Control-alterable plus predecrement
                // only — *not* postincrement (that is the load form's mode).
                Form {
                    base: 0x4880,
                    size: SizeEnc::WL { shift: 6 },
                    operands: &[Slot::RegList, ea_src(AI | PD | DI | IX | AW | AL)],
                },
                // load: memory -> reglist
                Form {
                    base: 0x4C80,
                    size: SizeEnc::WL { shift: 6 },
                    operands: &[
                        ea_src(
                            ea::AI | ea::PI | ea::DI | ea::IX | ea::AW | ea::AL | ea::PCD | ea::PCX,
                        ),
                        Slot::RegList,
                    ],
                },
            ],
        },
        // MOVEP — move data between a data register and alternate bytes of a
        // `d16(Ay)` peripheral address. Bit 7 is the direction (0 = mem->reg,
        // 1 = reg->mem), bit 6 the size (WL). The `001` in bits 3–5 of the base
        // is the EA-mode marker that keeps these off the dynamic bit ops (whose
        // An-mode EA is illegal there).
        Insn {
            mnemonic: "MOVEP",
            summary: "Move peripheral data",
            forms: &[
                // d16(Ay),Dx
                Form {
                    base: 0x0108,
                    size: SizeEnc::WL { shift: 6 },
                    operands: &[Slot::MovepDisp, Slot::Dn { shift: 9 }],
                },
                // Dx,d16(Ay)
                Form {
                    base: 0x0188,
                    size: SizeEnc::WL { shift: 6 },
                    operands: &[Slot::Dn { shift: 9 }, Slot::MovepDisp],
                },
            ],
        },
    ],
};

// Conditional-branch forms (one PC-relative target each); the condition lives
// in bits 8–11 of the base word.
const BRANCH_EQ: &[Form] = &[Form {
    base: 0x6700,
    size: SizeEnc::Fixed(Size::W),
    operands: &[Slot::BranchW],
}];
const BRANCH_NE: &[Form] = &[Form {
    base: 0x6600,
    size: SizeEnc::Fixed(Size::W),
    operands: &[Slot::BranchW],
}];
const BRANCH_GE: &[Form] = &[Form {
    base: 0x6C00,
    size: SizeEnc::Fixed(Size::W),
    operands: &[Slot::BranchW],
}];
const BRANCH_LT: &[Form] = &[Form {
    base: 0x6D00,
    size: SizeEnc::Fixed(Size::W),
    operands: &[Slot::BranchW],
}];
const BRANCH_GT: &[Form] = &[Form {
    base: 0x6E00,
    size: SizeEnc::Fixed(Size::W),
    operands: &[Slot::BranchW],
}];
const BRANCH_LE: &[Form] = &[Form {
    base: 0x6F00,
    size: SizeEnc::Fixed(Size::W),
    operands: &[Slot::BranchW],
}];
const BRANCH_MI: &[Form] = &[Form {
    base: 0x6B00,
    size: SizeEnc::Fixed(Size::W),
    operands: &[Slot::BranchW],
}];
const BRANCH_PL: &[Form] = &[Form {
    base: 0x6A00,
    size: SizeEnc::Fixed(Size::W),
    operands: &[Slot::BranchW],
}];
const BRANCH_HI: &[Form] = &[Form {
    base: 0x6200,
    size: SizeEnc::Fixed(Size::W),
    operands: &[Slot::BranchW],
}];
const BRANCH_LS: &[Form] = &[Form {
    base: 0x6300,
    size: SizeEnc::Fixed(Size::W),
    operands: &[Slot::BranchW],
}];
const BRANCH_CC: &[Form] = &[Form {
    base: 0x6400,
    size: SizeEnc::Fixed(Size::W),
    operands: &[Slot::BranchW],
}];
const BRANCH_CS: &[Form] = &[Form {
    base: 0x6500,
    size: SizeEnc::Fixed(Size::W),
    operands: &[Slot::BranchW],
}];
const BRANCH_VC: &[Form] = &[Form {
    base: 0x6800,
    size: SizeEnc::Fixed(Size::W),
    operands: &[Slot::BranchW],
}];
const BRANCH_VS: &[Form] = &[Form {
    base: 0x6900,
    size: SizeEnc::Fixed(Size::W),
    operands: &[Slot::BranchW],
}];
