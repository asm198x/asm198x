//! DEC PDP-11 instruction set — the 16-bit minicomputer that anchored Unix
//! and C. **Little-endian**; every instruction is one 16-bit word plus 0–2
//! extension words.
//!
//! Unlike the fixed-opcode-byte specs ([`crate::mos6502`], [`crate::z80`]),
//! the PDP-11 packs its operands as **fields inside the opcode word** — a
//! 3-bit mode + 3-bit register per operand — so enumerating one [`Form`] per
//! addressing-mode combination is impractical (8 modes × 8 registers, squared
//! for two-operand instructions). This module is therefore a **bespoke table**:
//! one [`Insn`] per mnemonic carrying its base opcode and its [`Class`] (which
//! fixes the field layout). The dialect front-end and the disassembler both key
//! off this table — the dialect packs the parsed operand fields into `base` and
//! emits any extension words through the engine's computed-operand seam; the
//! disassembler masks the class's fixed bits back to a `base` to name the
//! instruction, then decodes the fields.
//!
//! The eight addressing modes (mode field 0–7): register `Rn` (0),
//! register-deferred `(Rn)` (1), autoincrement `(Rn)+` (2),
//! autoincrement-deferred `@(Rn)+` (3), autodecrement `-(Rn)` (4),
//! autodecrement-deferred `@-(Rn)` (5), index `X(Rn)` (6), index-deferred
//! `@X(Rn)` (7). With the PC (`R7`) modes 2/3/6/7 become the immediate `#n`,
//! absolute `@#n`, relative `addr`, and relative-deferred `@addr` forms.
//!
//! Scope: the complete integer instruction set (base + EIS `MUL`/`DIV`/`ASH`/
//! `ASHC`, the J-11 `MTPS`/`MFPS`/`CSM`/`TSTSET`/`WRTLCK`) as accepted by `asl`'s
//! most-complete model, `MICROPDP-11/93`. The FP11 floating-point instruction
//! set is a separate coprocessor ISA (the analogue of the 68000's FPU) and is
//! out of scope here. Every base opcode is validated byte-for-byte against
//! `asl` — see `crates/asm198x/tests/conformance.rs`.

use crate::{Endianness, InstructionSet};

/// The field layout of an instruction — how the operand fields pack into the
/// 16-bit opcode word, and therefore how the dialect parses operands and the
/// disassembler decodes them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    /// Two general operands: `base | src6 << 6 | dst6`, each a 6-bit
    /// mode/register field. `MOV`, `ADD`, `CMPB`, …
    Double,
    /// One general operand in the low 6 bits: `base | dst6`. `CLR`, `INC`,
    /// `JMP`, `SWAB`, `TSTB`, …
    Single,
    /// 8-bit signed, word-scaled PC offset: `base | (offset & 0xFF)`, target
    /// `= PC + 2 + 2·offset`. `BR`, `BNE`, …
    Branch,
    /// Register (bits 6–8) + a 6-bit **backward** word offset (bits 0–5):
    /// `base | reg << 6 | offset`, target `= PC + 2 − 2·offset`. `SOB`.
    Sob,
    /// Register (bits 6–8) + a destination operand (bits 0–5):
    /// `base | reg << 6 | dst6`; syntax `reg, dst`. `JSR`.
    Jsr,
    /// Register in the low 3 bits: `base | reg`. `RTS`.
    Rts,
    /// A source operand (bits 0–5) + register (bits 6–8):
    /// `base | reg << 6 | src6`; syntax `src, reg`. The EIS `MUL`/`DIV`/`ASH`/
    /// `ASHC`.
    RegSrc,
    /// Register (bits 6–8) + a destination operand (bits 0–5):
    /// `base | reg << 6 | dst6`; syntax `reg, dst`. `XOR`.
    Xor,
    /// 8-bit trap operand: `base | (n & 0xFF)`. `EMT`, `TRAP`.
    Trap,
    /// 6-bit count: `base | (n & 0x3F)`. `MARK`.
    Mark,
    /// 3-bit priority level: `base | (n & 7)`. `SPL`.
    Spl,
    /// No operand — the whole opcode is fixed. `HALT`, `RTI`, `NOP`, the
    /// condition-code ops `CLC`/`SEC`/`CCC`/`SCC`, …
    NoArg,
}

/// One PDP-11 mnemonic: its base opcode (with all operand fields zero) and the
/// [`Class`] that fixes how the fields pack.
pub struct Insn {
    pub mnemonic: &'static str,
    pub base: u16,
    pub class: Class,
    pub summary: &'static str,
}

impl Class {
    /// The fixed (non-field) bits of the opcode word for this class — masking a
    /// word with it yields the `base` to look up. Ordering the decode from the
    /// widest mask to the narrowest disambiguates classes that share opcode
    /// space; see [`decode`].
    #[must_use]
    pub const fn mask(self) -> u16 {
        match self {
            Class::NoArg => 0xFFFF,
            Class::Rts | Class::Spl => 0xFFF8,
            Class::Mark | Class::Single => 0xFFC0,
            Class::Trap | Class::Branch => 0xFF00,
            Class::Jsr | Class::Sob | Class::RegSrc | Class::Xor => 0xFE00,
            Class::Double => 0xF000,
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

/// Identify the instruction encoded in `word`, returning its [`Insn`] and the
/// masked `base`. Classes are tried from the widest fixed-bit mask to the
/// narrowest so a more-specific opcode (e.g. `RTS` at `0o000200`) is matched
/// before a broader one (a `Single` op) that shares the region.
#[must_use]
pub fn decode(word: u16) -> Option<&'static Insn> {
    // Priority order: exact, then narrowing masks. Within a mask width the
    // regions are disjoint, so table membership resolves the rest.
    const ORDER: &[Class] = &[
        Class::NoArg,
        Class::Rts,
        Class::Spl,
        Class::Mark,
        Class::Single,
        Class::Trap,
        Class::Branch,
        Class::Jsr,
        Class::Sob,
        Class::RegSrc,
        Class::Xor,
        Class::Double,
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

/// Minimal set for the [`Dialect`](../../asm198x/dialect/trait.Dialect.html)
/// trait — the PDP-11 dialect encodes through the computed-operand seam, not
/// [`Form`]s, so only the endianness is load-bearing here.
pub const SET: InstructionSet = InstructionSet {
    cpu: "DEC PDP-11",
    endianness: Endianness::Little,
    instructions: &[],
};

use Class::{Branch, Double, Jsr, Mark, NoArg, RegSrc, Rts, Single, Sob, Spl, Trap, Xor};

/// Every integer-CPU mnemonic, base opcode validated against `asl`
/// (`MICROPDP-11/93`).
pub const INSTRUCTIONS: &[Insn] = &[
    // --- Double-operand (word) ---------------------------------------------
    Insn {
        mnemonic: "MOV",
        base: 0x1000,
        class: Double,
        summary: "Move word",
    },
    Insn {
        mnemonic: "CMP",
        base: 0x2000,
        class: Double,
        summary: "Compare word",
    },
    Insn {
        mnemonic: "BIT",
        base: 0x3000,
        class: Double,
        summary: "Bit test word",
    },
    Insn {
        mnemonic: "BIC",
        base: 0x4000,
        class: Double,
        summary: "Bit clear word",
    },
    Insn {
        mnemonic: "BIS",
        base: 0x5000,
        class: Double,
        summary: "Bit set word",
    },
    Insn {
        mnemonic: "ADD",
        base: 0x6000,
        class: Double,
        summary: "Add word",
    },
    Insn {
        mnemonic: "SUB",
        base: 0xE000,
        class: Double,
        summary: "Subtract word",
    },
    // --- Double-operand (byte) ---------------------------------------------
    Insn {
        mnemonic: "MOVB",
        base: 0x9000,
        class: Double,
        summary: "Move byte",
    },
    Insn {
        mnemonic: "CMPB",
        base: 0xA000,
        class: Double,
        summary: "Compare byte",
    },
    Insn {
        mnemonic: "BITB",
        base: 0xB000,
        class: Double,
        summary: "Bit test byte",
    },
    Insn {
        mnemonic: "BICB",
        base: 0xC000,
        class: Double,
        summary: "Bit clear byte",
    },
    Insn {
        mnemonic: "BISB",
        base: 0xD000,
        class: Double,
        summary: "Bit set byte",
    },
    // --- Single-operand (word) ---------------------------------------------
    Insn {
        mnemonic: "JMP",
        base: 0x0040,
        class: Single,
        summary: "Jump",
    },
    Insn {
        mnemonic: "SWAB",
        base: 0x00C0,
        class: Single,
        summary: "Swap bytes",
    },
    Insn {
        mnemonic: "CLR",
        base: 0x0A00,
        class: Single,
        summary: "Clear word",
    },
    Insn {
        mnemonic: "COM",
        base: 0x0A40,
        class: Single,
        summary: "Complement word",
    },
    Insn {
        mnemonic: "INC",
        base: 0x0A80,
        class: Single,
        summary: "Increment word",
    },
    Insn {
        mnemonic: "DEC",
        base: 0x0AC0,
        class: Single,
        summary: "Decrement word",
    },
    Insn {
        mnemonic: "NEG",
        base: 0x0B00,
        class: Single,
        summary: "Negate word",
    },
    Insn {
        mnemonic: "ADC",
        base: 0x0B40,
        class: Single,
        summary: "Add carry word",
    },
    Insn {
        mnemonic: "SBC",
        base: 0x0B80,
        class: Single,
        summary: "Subtract carry word",
    },
    Insn {
        mnemonic: "TST",
        base: 0x0BC0,
        class: Single,
        summary: "Test word",
    },
    Insn {
        mnemonic: "ROR",
        base: 0x0C00,
        class: Single,
        summary: "Rotate right word",
    },
    Insn {
        mnemonic: "ROL",
        base: 0x0C40,
        class: Single,
        summary: "Rotate left word",
    },
    Insn {
        mnemonic: "ASR",
        base: 0x0C80,
        class: Single,
        summary: "Arithmetic shift right word",
    },
    Insn {
        mnemonic: "ASL",
        base: 0x0CC0,
        class: Single,
        summary: "Arithmetic shift left word",
    },
    Insn {
        mnemonic: "MFPI",
        base: 0x0D40,
        class: Single,
        summary: "Move from previous instruction space",
    },
    Insn {
        mnemonic: "MTPI",
        base: 0x0D80,
        class: Single,
        summary: "Move to previous instruction space",
    },
    Insn {
        mnemonic: "SXT",
        base: 0x0DC0,
        class: Single,
        summary: "Sign extend",
    },
    Insn {
        mnemonic: "CSM",
        base: 0x0E00,
        class: Single,
        summary: "Call to supervisor mode",
    },
    Insn {
        mnemonic: "TSTSET",
        base: 0x0E80,
        class: Single,
        summary: "Test and set",
    },
    Insn {
        mnemonic: "WRTLCK",
        base: 0x0EC0,
        class: Single,
        summary: "Read-lock, write-unlock",
    },
    // --- Single-operand (byte) ---------------------------------------------
    Insn {
        mnemonic: "CLRB",
        base: 0x8A00,
        class: Single,
        summary: "Clear byte",
    },
    Insn {
        mnemonic: "COMB",
        base: 0x8A40,
        class: Single,
        summary: "Complement byte",
    },
    Insn {
        mnemonic: "INCB",
        base: 0x8A80,
        class: Single,
        summary: "Increment byte",
    },
    Insn {
        mnemonic: "DECB",
        base: 0x8AC0,
        class: Single,
        summary: "Decrement byte",
    },
    Insn {
        mnemonic: "NEGB",
        base: 0x8B00,
        class: Single,
        summary: "Negate byte",
    },
    Insn {
        mnemonic: "ADCB",
        base: 0x8B40,
        class: Single,
        summary: "Add carry byte",
    },
    Insn {
        mnemonic: "SBCB",
        base: 0x8B80,
        class: Single,
        summary: "Subtract carry byte",
    },
    Insn {
        mnemonic: "TSTB",
        base: 0x8BC0,
        class: Single,
        summary: "Test byte",
    },
    Insn {
        mnemonic: "RORB",
        base: 0x8C00,
        class: Single,
        summary: "Rotate right byte",
    },
    Insn {
        mnemonic: "ROLB",
        base: 0x8C40,
        class: Single,
        summary: "Rotate left byte",
    },
    Insn {
        mnemonic: "ASRB",
        base: 0x8C80,
        class: Single,
        summary: "Arithmetic shift right byte",
    },
    Insn {
        mnemonic: "ASLB",
        base: 0x8CC0,
        class: Single,
        summary: "Arithmetic shift left byte",
    },
    Insn {
        mnemonic: "MTPS",
        base: 0x8D00,
        class: Single,
        summary: "Move byte to processor status",
    },
    Insn {
        mnemonic: "MFPD",
        base: 0x8D40,
        class: Single,
        summary: "Move from previous data space",
    },
    Insn {
        mnemonic: "MTPD",
        base: 0x8D80,
        class: Single,
        summary: "Move to previous data space",
    },
    Insn {
        mnemonic: "MFPS",
        base: 0x8DC0,
        class: Single,
        summary: "Move byte from processor status",
    },
    // --- Branches ----------------------------------------------------------
    Insn {
        mnemonic: "BR",
        base: 0x0100,
        class: Branch,
        summary: "Branch (unconditional)",
    },
    Insn {
        mnemonic: "BNE",
        base: 0x0200,
        class: Branch,
        summary: "Branch if not equal",
    },
    Insn {
        mnemonic: "BEQ",
        base: 0x0300,
        class: Branch,
        summary: "Branch if equal",
    },
    Insn {
        mnemonic: "BGE",
        base: 0x0400,
        class: Branch,
        summary: "Branch if greater or equal",
    },
    Insn {
        mnemonic: "BLT",
        base: 0x0500,
        class: Branch,
        summary: "Branch if less than",
    },
    Insn {
        mnemonic: "BGT",
        base: 0x0600,
        class: Branch,
        summary: "Branch if greater than",
    },
    Insn {
        mnemonic: "BLE",
        base: 0x0700,
        class: Branch,
        summary: "Branch if less or equal",
    },
    Insn {
        mnemonic: "BPL",
        base: 0x8000,
        class: Branch,
        summary: "Branch if plus",
    },
    Insn {
        mnemonic: "BMI",
        base: 0x8100,
        class: Branch,
        summary: "Branch if minus",
    },
    Insn {
        mnemonic: "BHI",
        base: 0x8200,
        class: Branch,
        summary: "Branch if higher",
    },
    Insn {
        mnemonic: "BLOS",
        base: 0x8300,
        class: Branch,
        summary: "Branch if lower or same",
    },
    Insn {
        mnemonic: "BVC",
        base: 0x8400,
        class: Branch,
        summary: "Branch if overflow clear",
    },
    Insn {
        mnemonic: "BVS",
        base: 0x8500,
        class: Branch,
        summary: "Branch if overflow set",
    },
    Insn {
        mnemonic: "BCC",
        base: 0x8600,
        class: Branch,
        summary: "Branch if carry clear",
    },
    Insn {
        mnemonic: "BCS",
        base: 0x8700,
        class: Branch,
        summary: "Branch if carry set",
    },
    // Aliases (same encodings; `decode` prefers the canonical BCC/BCS above).
    Insn {
        mnemonic: "BHIS",
        base: 0x8600,
        class: Branch,
        summary: "Branch if higher or same (= BCC)",
    },
    Insn {
        mnemonic: "BLO",
        base: 0x8700,
        class: Branch,
        summary: "Branch if lower (= BCS)",
    },
    // --- EIS + XOR + SOB + JSR ---------------------------------------------
    Insn {
        mnemonic: "MUL",
        base: 0x7000,
        class: RegSrc,
        summary: "Multiply",
    },
    Insn {
        mnemonic: "DIV",
        base: 0x7200,
        class: RegSrc,
        summary: "Divide",
    },
    Insn {
        mnemonic: "ASH",
        base: 0x7400,
        class: RegSrc,
        summary: "Arithmetic shift",
    },
    Insn {
        mnemonic: "ASHC",
        base: 0x7600,
        class: RegSrc,
        summary: "Arithmetic shift combined",
    },
    Insn {
        mnemonic: "XOR",
        base: 0x7800,
        class: Xor,
        summary: "Exclusive or",
    },
    Insn {
        mnemonic: "SOB",
        base: 0x7E00,
        class: Sob,
        summary: "Subtract one and branch",
    },
    Insn {
        mnemonic: "JSR",
        base: 0x0800,
        class: Jsr,
        summary: "Jump to subroutine",
    },
    // --- Traps + stack-frame + priority ------------------------------------
    Insn {
        mnemonic: "EMT",
        base: 0x8800,
        class: Trap,
        summary: "Emulator trap",
    },
    Insn {
        mnemonic: "TRAP",
        base: 0x8900,
        class: Trap,
        summary: "Trap",
    },
    Insn {
        mnemonic: "MARK",
        base: 0x0D00,
        class: Mark,
        summary: "Mark (stack cleanup)",
    },
    Insn {
        mnemonic: "SPL",
        base: 0x0098,
        class: Spl,
        summary: "Set priority level",
    },
    Insn {
        mnemonic: "RTS",
        base: 0x0080,
        class: Rts,
        summary: "Return from subroutine",
    },
    // --- No-operand + condition codes --------------------------------------
    Insn {
        mnemonic: "HALT",
        base: 0x0000,
        class: NoArg,
        summary: "Halt",
    },
    Insn {
        mnemonic: "WAIT",
        base: 0x0001,
        class: NoArg,
        summary: "Wait for interrupt",
    },
    Insn {
        mnemonic: "RTI",
        base: 0x0002,
        class: NoArg,
        summary: "Return from interrupt",
    },
    Insn {
        mnemonic: "BPT",
        base: 0x0003,
        class: NoArg,
        summary: "Breakpoint trap",
    },
    Insn {
        mnemonic: "IOT",
        base: 0x0004,
        class: NoArg,
        summary: "I/O trap",
    },
    Insn {
        mnemonic: "RESET",
        base: 0x0005,
        class: NoArg,
        summary: "Reset external bus",
    },
    Insn {
        mnemonic: "RTT",
        base: 0x0006,
        class: NoArg,
        summary: "Return from interrupt (inhibit T)",
    },
    Insn {
        mnemonic: "MFPT",
        base: 0x0007,
        class: NoArg,
        summary: "Move from processor type",
    },
    Insn {
        mnemonic: "NOP",
        base: 0x00A0,
        class: NoArg,
        summary: "No operation",
    },
    Insn {
        mnemonic: "CLC",
        base: 0x00A1,
        class: NoArg,
        summary: "Clear C",
    },
    Insn {
        mnemonic: "CLV",
        base: 0x00A2,
        class: NoArg,
        summary: "Clear V",
    },
    Insn {
        mnemonic: "CLZ",
        base: 0x00A4,
        class: NoArg,
        summary: "Clear Z",
    },
    Insn {
        mnemonic: "CLN",
        base: 0x00A8,
        class: NoArg,
        summary: "Clear N",
    },
    Insn {
        mnemonic: "CCC",
        base: 0x00AF,
        class: NoArg,
        summary: "Clear all condition codes",
    },
    Insn {
        mnemonic: "SEC",
        base: 0x00B1,
        class: NoArg,
        summary: "Set C",
    },
    Insn {
        mnemonic: "SEV",
        base: 0x00B2,
        class: NoArg,
        summary: "Set V",
    },
    Insn {
        mnemonic: "SEZ",
        base: 0x00B4,
        class: NoArg,
        summary: "Set Z",
    },
    Insn {
        mnemonic: "SEN",
        base: 0x00B8,
        class: NoArg,
        summary: "Set N",
    },
    Insn {
        mnemonic: "SCC",
        base: 0x00BF,
        class: NoArg,
        summary: "Set all condition codes",
    },
];
