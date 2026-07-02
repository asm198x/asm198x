//! Declarative instruction-set specifications for the 198x family CPUs.
//!
//! This crate is the **single source of truth for instruction encoding**:
//! mnemonic ↔ opcode bytes ↔ operand layout ↔ cycle counts ↔ affected flags.
//! Asm198x consumes it to assemble and disassemble; Emu198x validates its
//! hand-written decoders against it. The spec is *authored* from the primary
//! reference library (datasheets), not extracted from any emulator's decode
//! loop — see `decisions/asm198x-and-shared-isa-spec.md`.
//!
//! The types here describe **fixed-opcode-byte** CPUs (6502, Z80 and its
//! prefixes) as well as **field-based** variable-length encodings (68000-class
//! in [`m68k`]) and computed-operand CPUs (6809 in [`mos6809`]).
//!
//! Everything is `&'static` data so a whole instruction set is a compile-time
//! constant: zero dependencies, no allocation, diffable in review.

/// A complete instruction set for one CPU.
pub struct InstructionSet {
    /// Human name, e.g. `"MOS 6502"`.
    pub cpu: &'static str,
    /// Byte order for multi-byte operands.
    pub endianness: Endianness,
    /// Every mnemonic the CPU understands.
    pub instructions: &'static [Instruction],
}

impl InstructionSet {
    /// Find an instruction by mnemonic (case-sensitive; specs use upper-case).
    #[must_use]
    pub fn instruction(&self, mnemonic: &str) -> Option<&Instruction> {
        self.instructions.iter().find(|i| i.mnemonic == mnemonic)
    }

    /// Find the form for a mnemonic and mode label, scanning *every* entry with
    /// that mnemonic. A mnemonic's forms may be split across entries — e.g. the
    /// Z80 base `LD` and a separate `LD` entry for the IX/IY forms — to keep the
    /// spec readable; this looks across all of them.
    #[must_use]
    pub fn find_form(&self, mnemonic: &str, mode: &str) -> Option<&Form> {
        self.instructions
            .iter()
            .filter(|i| i.mnemonic == mnemonic)
            .find_map(|i| i.form(mode))
    }

    /// Whether any entry uses this mnemonic.
    #[must_use]
    pub fn has_mnemonic(&self, mnemonic: &str) -> bool {
        self.instructions.iter().any(|i| i.mnemonic == mnemonic)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Endianness {
    Little,
    Big,
}

/// One mnemonic and all the ways it can be encoded.
pub struct Instruction {
    /// Upper-case mnemonic, e.g. `"LDA"`.
    pub mnemonic: &'static str,
    /// One-line description, e.g. `"Load accumulator"`.
    pub summary: &'static str,
    /// One [`Form`] per addressing mode this mnemonic supports.
    pub forms: &'static [Form],
}

impl Instruction {
    /// Find the form for a given addressing-mode label (see [`Form::mode`]).
    #[must_use]
    pub fn form(&self, mode: &str) -> Option<&Form> {
        self.forms.iter().find(|f| f.mode == mode)
    }
}

/// One concrete encoding of an instruction — a single addressing mode.
pub struct Form {
    /// Fixed opcode bytes, in order. One byte for the 6502; a prefix sequence
    /// for prefixed Z80 opcodes (e.g. `&[0xCB, 0x40]`).
    pub opcode: &'static [u8],
    /// Dialect-facing addressing-mode label, e.g. `"immediate"`, `"absolute,x"`.
    /// The assembler's parser maps parsed operand syntax to this label, then
    /// looks the form up by it — so the label strings are a shared contract
    /// between this spec and each CPU's dialect front-end.
    pub mode: &'static str,
    /// Operand bytes emitted after the opcode, in order.
    pub operands: &'static [Operand],
    /// Trailing opcode bytes emitted *after* the operands. Empty for almost
    /// every form; used by the Z80 `DD CB` / `FD CB` group, whose final opcode
    /// byte follows the displacement operand (`DD CB <d> <op>`).
    pub suffix: &'static [u8],
    /// Timing.
    pub cycles: Cycles,
    /// Status flags affected, as a compact string, e.g. `"NZ"` or `"NZCV"`.
    /// Documentation- and disassembler-grade; the assembler ignores it.
    pub flags: &'static str,
    /// Undocumented / illegal opcode.
    pub undocumented: bool,
}

impl Form {
    /// Total encoded length in bytes: opcode bytes, operand bytes, and any
    /// trailing suffix opcode bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.opcode.len()
            + self.suffix.len()
            + self
                .operands
                .iter()
                .map(|o| o.bytes as usize)
                .sum::<usize>()
    }

    /// A form is never empty (it always has at least one opcode byte); this
    /// exists only to satisfy the `len`-without-`is_empty` lint cleanly.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        false
    }
}

/// One operand slot in an encoding: what kind of value, and how wide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Operand {
    pub kind: OperandKind,
    /// Width in bytes. Laid out in the instruction set's [`Endianness`].
    pub bytes: u8,
}

/// The genuinely CPU-agnostic operand categories the assembler needs in order
/// to lay bytes down. Addressing-mode *flavour* (zero-page vs absolute, which
/// index register) lives in the [`Form::mode`] label and the dialect parser;
/// here we only describe the bytes on the wire.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OperandKind {
    /// A literal value (immediate).
    Immediate,
    /// A big-endian immediate — laid down high byte first regardless of the
    /// instruction set's [`Endianness`]. The Z80N `push nn` (`ED 8A`) special
    /// case: uniquely in the Z80 set its 16-bit operand is stored big-endian.
    ImmediateBe,
    /// An absolute address or zero-page offset — distinguished by `bytes`.
    Address,
    /// A signed, PC-relative displacement (branches).
    RelativePc,
    /// A signed 8-bit offset added to an index register, e.g. the `d` in the
    /// Z80 `(IX+d)`. Emitted as one byte; range −128..=127.
    Displacement,
}

/// Per-form timing. Extra cycles are conditional and additive.
#[derive(Clone, Copy, Debug)]
pub struct Cycles {
    pub base: u8,
    /// Extra cycle when an indexed access crosses a page boundary.
    pub page_cross: u8,
    /// Extra cycle when a branch is taken (a further page-cross cycle on top
    /// is also possible on the 6502).
    pub branch_taken: u8,
}

impl Cycles {
    /// Fixed-cost form: `base` cycles, no conditional extras.
    #[must_use]
    pub const fn fixed(base: u8) -> Self {
        Self {
            base,
            page_cross: 0,
            branch_taken: 0,
        }
    }

    /// Indexed read that costs one more cycle across a page boundary.
    #[must_use]
    pub const fn page_crossing(base: u8) -> Self {
        Self {
            base,
            page_cross: 1,
            branch_taken: 0,
        }
    }

    /// Relative branch: `base` if not taken, `+1` if taken.
    #[must_use]
    pub const fn branch(base: u8) -> Self {
        Self {
            base,
            page_cross: 1,
            branch_taken: 1,
        }
    }
}

pub mod cdp1802;
pub mod huc6280;
pub mod i8080;
pub mod m6800;
pub mod m68k;
pub mod mos6502;
pub mod mos65816;
pub mod mos6809;
pub mod sm83;
pub mod z80;
