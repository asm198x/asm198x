//! Sharp SM83 (Game Boy / LR35902) instruction set.
//!
//! The SM83 is a custom 8-bit core — 8080-derived, Z80-*flavoured*, but neither.
//! It drops the Z80's `IX`/`IY`, shadow registers, `I`/`R`, interrupt modes,
//! `IN`/`OUT`, block ops, and the `S`/`P-V` flags; it adds `LDH`, `LD (HL+)`/
//! `(HL-)`, `LD HL,SP+e`, `ADD SP,e`, `SWAP`, and a two-byte `STOP`. The opcode
//! map diverges too far to layer over [`crate::z80`], so this is a **fresh
//! fixed-slot spec** — a single-byte main page plus the `CB` bit/rotate page.
//!
//! **Mode labels are `rgbasm` operand templates.** Registers are lower-case
//! (`a`, `[hl]`) and immediates are upper-case placeholders — `N` (8-bit), `NN`
//! (16-bit little-endian), `E` (a `jr` relative target), `D` (a signed byte for
//! the `sp` displacement ops). The case split matters: register `e` is a
//! lower-case letter, the `jr` placeholder `E` is upper-case, so they never
//! collide. The disassembler substitutes the placeholders; the `rgbasm` dialect
//! builds candidate labels from parsed operands and looks the form up by them.
//!
//! **Provenance.** Authored from Nintendo's *Game Boy Programming Manual v1.1*
//! (in the primary library at `reference/by-topic/cpu-sm83/`), with every opcode
//! cross-checked byte-for-byte against `rgbasm`/`rgblink` (RGBDS). Cycle counts
//! are the manual's machine-cycle (M-cycle) values and are documentation-grade
//! (the assembler ignores them).

use crate::{Cycles, Endianness, Form, Instruction, InstructionSet, Operand, OperandKind};

const IMM8: Operand = Operand {
    kind: OperandKind::Immediate,
    bytes: 1,
};
const IMM16: Operand = Operand {
    kind: OperandKind::Address,
    bytes: 2,
};
/// A `jr` PC-relative target byte.
const REL: Operand = Operand {
    kind: OperandKind::RelativePc,
    bytes: 1,
};
/// A signed byte added to `sp` (`add sp,e` / `ld hl,sp+e`).
const DISP: Operand = Operand {
    kind: OperandKind::Displacement,
    bytes: 1,
};

const NONE: &[Operand] = &[];
const ONE_N: &[Operand] = &[IMM8];
const ONE_NN: &[Operand] = &[IMM16];
const ONE_REL: &[Operand] = &[REL];
const ONE_DISP: &[Operand] = &[DISP];

pub const SET: InstructionSet = InstructionSet {
    cpu: "Sharp SM83 (Game Boy)",
    endianness: Endianness::Little,
    instructions: INSTRUCTIONS,
};

const fn form(
    opcode: &'static [u8],
    mode: &'static str,
    operands: &'static [Operand],
    cycles: Cycles,
    flags: &'static str,
) -> Form {
    Form {
        opcode,
        mode,
        operands,
        suffix: &[],
        cycles,
        flags,
        undocumented: false,
    }
}

macro_rules! inst {
    ($mnemonic:literal, $summary:literal, [ $($form:expr),* $(,)? ]) => {
        Instruction { mnemonic: $mnemonic, summary: $summary, forms: &[ $($form),* ] }
    };
}

/// One `CB`-prefixed form: `CB <op>`, no operand byte (the register and, for the
/// bit ops, the bit number are packed into `op`).
macro_rules! cbf {
    ($op:literal, $mode:literal, $cyc:literal, $fl:literal) => {
        form(&[0xCB, $op], $mode, NONE, Cycles::fixed($cyc), $fl)
    };
}

#[rustfmt::skip]
const INSTRUCTIONS: &[Instruction] = &[
    // ===================== control / misc =====================
    inst!("NOP",  "No operation",   [form(&[0x00], "", NONE, Cycles::fixed(1), "")]),
    inst!("STOP", "Stop / standby",  [form(&[0x10, 0x00], "", NONE, Cycles::fixed(1), "")]),
    inst!("HALT", "Halt CPU",        [form(&[0x76], "", NONE, Cycles::fixed(1), "")]),
    inst!("DI",   "Disable interrupts", [form(&[0xF3], "", NONE, Cycles::fixed(1), "")]),
    inst!("EI",   "Enable interrupts",  [form(&[0xFB], "", NONE, Cycles::fixed(1), "")]),

    // Accumulator/flag ops.
    inst!("RLCA", "Rotate A left",         [form(&[0x07], "", NONE, Cycles::fixed(1), "ZNHC")]),
    inst!("RRCA", "Rotate A right",        [form(&[0x0F], "", NONE, Cycles::fixed(1), "ZNHC")]),
    inst!("RLA",  "Rotate A left thru CY", [form(&[0x17], "", NONE, Cycles::fixed(1), "ZNHC")]),
    inst!("RRA",  "Rotate A right thru CY",[form(&[0x1F], "", NONE, Cycles::fixed(1), "ZNHC")]),
    inst!("DAA",  "Decimal adjust A",      [form(&[0x27], "", NONE, Cycles::fixed(1), "ZHC")]),
    inst!("CPL",  "Complement A",          [form(&[0x2F], "", NONE, Cycles::fixed(1), "NH")]),
    inst!("SCF",  "Set carry flag",        [form(&[0x37], "", NONE, Cycles::fixed(1), "NHC")]),
    inst!("CCF",  "Complement carry flag", [form(&[0x3F], "", NONE, Cycles::fixed(1), "NHC")]),

    // ===================== 8-bit loads =====================
    inst!("LD", "Load", [
        // LD r, r' — 0x40..0x7F, less 0x76 (HALT).
        form(&[0x40], "b,b", NONE, Cycles::fixed(1), ""), form(&[0x41], "b,c", NONE, Cycles::fixed(1), ""),
        form(&[0x42], "b,d", NONE, Cycles::fixed(1), ""), form(&[0x43], "b,e", NONE, Cycles::fixed(1), ""),
        form(&[0x44], "b,h", NONE, Cycles::fixed(1), ""), form(&[0x45], "b,l", NONE, Cycles::fixed(1), ""),
        form(&[0x46], "b,[hl]", NONE, Cycles::fixed(2), ""), form(&[0x47], "b,a", NONE, Cycles::fixed(1), ""),
        form(&[0x48], "c,b", NONE, Cycles::fixed(1), ""), form(&[0x49], "c,c", NONE, Cycles::fixed(1), ""),
        form(&[0x4A], "c,d", NONE, Cycles::fixed(1), ""), form(&[0x4B], "c,e", NONE, Cycles::fixed(1), ""),
        form(&[0x4C], "c,h", NONE, Cycles::fixed(1), ""), form(&[0x4D], "c,l", NONE, Cycles::fixed(1), ""),
        form(&[0x4E], "c,[hl]", NONE, Cycles::fixed(2), ""), form(&[0x4F], "c,a", NONE, Cycles::fixed(1), ""),
        form(&[0x50], "d,b", NONE, Cycles::fixed(1), ""), form(&[0x51], "d,c", NONE, Cycles::fixed(1), ""),
        form(&[0x52], "d,d", NONE, Cycles::fixed(1), ""), form(&[0x53], "d,e", NONE, Cycles::fixed(1), ""),
        form(&[0x54], "d,h", NONE, Cycles::fixed(1), ""), form(&[0x55], "d,l", NONE, Cycles::fixed(1), ""),
        form(&[0x56], "d,[hl]", NONE, Cycles::fixed(2), ""), form(&[0x57], "d,a", NONE, Cycles::fixed(1), ""),
        form(&[0x58], "e,b", NONE, Cycles::fixed(1), ""), form(&[0x59], "e,c", NONE, Cycles::fixed(1), ""),
        form(&[0x5A], "e,d", NONE, Cycles::fixed(1), ""), form(&[0x5B], "e,e", NONE, Cycles::fixed(1), ""),
        form(&[0x5C], "e,h", NONE, Cycles::fixed(1), ""), form(&[0x5D], "e,l", NONE, Cycles::fixed(1), ""),
        form(&[0x5E], "e,[hl]", NONE, Cycles::fixed(2), ""), form(&[0x5F], "e,a", NONE, Cycles::fixed(1), ""),
        form(&[0x60], "h,b", NONE, Cycles::fixed(1), ""), form(&[0x61], "h,c", NONE, Cycles::fixed(1), ""),
        form(&[0x62], "h,d", NONE, Cycles::fixed(1), ""), form(&[0x63], "h,e", NONE, Cycles::fixed(1), ""),
        form(&[0x64], "h,h", NONE, Cycles::fixed(1), ""), form(&[0x65], "h,l", NONE, Cycles::fixed(1), ""),
        form(&[0x66], "h,[hl]", NONE, Cycles::fixed(2), ""), form(&[0x67], "h,a", NONE, Cycles::fixed(1), ""),
        form(&[0x68], "l,b", NONE, Cycles::fixed(1), ""), form(&[0x69], "l,c", NONE, Cycles::fixed(1), ""),
        form(&[0x6A], "l,d", NONE, Cycles::fixed(1), ""), form(&[0x6B], "l,e", NONE, Cycles::fixed(1), ""),
        form(&[0x6C], "l,h", NONE, Cycles::fixed(1), ""), form(&[0x6D], "l,l", NONE, Cycles::fixed(1), ""),
        form(&[0x6E], "l,[hl]", NONE, Cycles::fixed(2), ""), form(&[0x6F], "l,a", NONE, Cycles::fixed(1), ""),
        form(&[0x70], "[hl],b", NONE, Cycles::fixed(2), ""), form(&[0x71], "[hl],c", NONE, Cycles::fixed(2), ""),
        form(&[0x72], "[hl],d", NONE, Cycles::fixed(2), ""), form(&[0x73], "[hl],e", NONE, Cycles::fixed(2), ""),
        form(&[0x74], "[hl],h", NONE, Cycles::fixed(2), ""), form(&[0x75], "[hl],l", NONE, Cycles::fixed(2), ""),
        form(&[0x77], "[hl],a", NONE, Cycles::fixed(2), ""),
        form(&[0x78], "a,b", NONE, Cycles::fixed(1), ""), form(&[0x79], "a,c", NONE, Cycles::fixed(1), ""),
        form(&[0x7A], "a,d", NONE, Cycles::fixed(1), ""), form(&[0x7B], "a,e", NONE, Cycles::fixed(1), ""),
        form(&[0x7C], "a,h", NONE, Cycles::fixed(1), ""), form(&[0x7D], "a,l", NONE, Cycles::fixed(1), ""),
        form(&[0x7E], "a,[hl]", NONE, Cycles::fixed(2), ""), form(&[0x7F], "a,a", NONE, Cycles::fixed(1), ""),

        // LD r, n — 0x06 + r*8.
        form(&[0x06], "b,N", ONE_N, Cycles::fixed(2), ""),
        form(&[0x0E], "c,N", ONE_N, Cycles::fixed(2), ""),
        form(&[0x16], "d,N", ONE_N, Cycles::fixed(2), ""),
        form(&[0x1E], "e,N", ONE_N, Cycles::fixed(2), ""),
        form(&[0x26], "h,N", ONE_N, Cycles::fixed(2), ""),
        form(&[0x2E], "l,N", ONE_N, Cycles::fixed(2), ""),
        form(&[0x36], "[hl],N", ONE_N, Cycles::fixed(3), ""),
        form(&[0x3E], "a,N", ONE_N, Cycles::fixed(2), ""),

        // Accumulator ↔ memory pairs.
        form(&[0x02], "[bc],a", NONE, Cycles::fixed(2), ""),
        form(&[0x12], "[de],a", NONE, Cycles::fixed(2), ""),
        form(&[0x22], "[hl+],a", NONE, Cycles::fixed(2), ""),
        form(&[0x32], "[hl-],a", NONE, Cycles::fixed(2), ""),
        form(&[0x0A], "a,[bc]", NONE, Cycles::fixed(2), ""),
        form(&[0x1A], "a,[de]", NONE, Cycles::fixed(2), ""),
        form(&[0x2A], "a,[hl+]", NONE, Cycles::fixed(2), ""),
        form(&[0x3A], "a,[hl-]", NONE, Cycles::fixed(2), ""),
        form(&[0xEA], "[NN],a", ONE_NN, Cycles::fixed(4), ""),
        form(&[0xFA], "a,[NN]", ONE_NN, Cycles::fixed(4), ""),

        // 16-bit immediate loads and SP moves.
        form(&[0x01], "bc,NN", ONE_NN, Cycles::fixed(3), ""),
        form(&[0x11], "de,NN", ONE_NN, Cycles::fixed(3), ""),
        form(&[0x21], "hl,NN", ONE_NN, Cycles::fixed(3), ""),
        form(&[0x31], "sp,NN", ONE_NN, Cycles::fixed(3), ""),
        form(&[0x08], "[NN],sp", ONE_NN, Cycles::fixed(5), ""),
        form(&[0xF9], "sp,hl", NONE, Cycles::fixed(2), ""),
        form(&[0xF8], "hl,sp+D", ONE_DISP, Cycles::fixed(3), "ZNHC"),
    ]),

    // LDI/LDD aliases (rgbasm accepts both these and LD (HL+)/(HL-)).
    inst!("LDI", "Load and increment HL", [
        form(&[0x22], "[hl],a", NONE, Cycles::fixed(2), ""),
        form(&[0x2A], "a,[hl]", NONE, Cycles::fixed(2), ""),
    ]),
    inst!("LDD", "Load and decrement HL", [
        form(&[0x32], "[hl],a", NONE, Cycles::fixed(2), ""),
        form(&[0x3A], "a,[hl]", NONE, Cycles::fixed(2), ""),
    ]),

    // LDH — high-page ($FF00+n) loads.
    inst!("LDH", "Load high page", [
        form(&[0xE0], "[$ff00+N],a", ONE_N, Cycles::fixed(3), ""),
        form(&[0xF0], "a,[$ff00+N]", ONE_N, Cycles::fixed(3), ""),
        form(&[0xE2], "[c],a", NONE, Cycles::fixed(2), ""),
        form(&[0xF2], "a,[c]", NONE, Cycles::fixed(2), ""),
    ]),

    // ===================== 8-bit arithmetic / logic =====================
    inst!("ADD", "Add", [
        form(&[0x80], "a,b", NONE, Cycles::fixed(1), "ZNHC"), form(&[0x81], "a,c", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0x82], "a,d", NONE, Cycles::fixed(1), "ZNHC"), form(&[0x83], "a,e", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0x84], "a,h", NONE, Cycles::fixed(1), "ZNHC"), form(&[0x85], "a,l", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0x86], "a,[hl]", NONE, Cycles::fixed(2), "ZNHC"), form(&[0x87], "a,a", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0xC6], "a,N", ONE_N, Cycles::fixed(2), "ZNHC"),
        // 16-bit adds to HL and the signed SP add.
        form(&[0x09], "hl,bc", NONE, Cycles::fixed(2), "NHC"),
        form(&[0x19], "hl,de", NONE, Cycles::fixed(2), "NHC"),
        form(&[0x29], "hl,hl", NONE, Cycles::fixed(2), "NHC"),
        form(&[0x39], "hl,sp", NONE, Cycles::fixed(2), "NHC"),
        form(&[0xE8], "sp,D", ONE_DISP, Cycles::fixed(4), "ZNHC"),
    ]),
    inst!("ADC", "Add with carry", [
        form(&[0x88], "a,b", NONE, Cycles::fixed(1), "ZNHC"), form(&[0x89], "a,c", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0x8A], "a,d", NONE, Cycles::fixed(1), "ZNHC"), form(&[0x8B], "a,e", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0x8C], "a,h", NONE, Cycles::fixed(1), "ZNHC"), form(&[0x8D], "a,l", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0x8E], "a,[hl]", NONE, Cycles::fixed(2), "ZNHC"), form(&[0x8F], "a,a", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0xCE], "a,N", ONE_N, Cycles::fixed(2), "ZNHC"),
    ]),
    inst!("SUB", "Subtract", [
        form(&[0x90], "a,b", NONE, Cycles::fixed(1), "ZNHC"), form(&[0x91], "a,c", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0x92], "a,d", NONE, Cycles::fixed(1), "ZNHC"), form(&[0x93], "a,e", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0x94], "a,h", NONE, Cycles::fixed(1), "ZNHC"), form(&[0x95], "a,l", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0x96], "a,[hl]", NONE, Cycles::fixed(2), "ZNHC"), form(&[0x97], "a,a", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0xD6], "a,N", ONE_N, Cycles::fixed(2), "ZNHC"),
    ]),
    inst!("SBC", "Subtract with carry", [
        form(&[0x98], "a,b", NONE, Cycles::fixed(1), "ZNHC"), form(&[0x99], "a,c", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0x9A], "a,d", NONE, Cycles::fixed(1), "ZNHC"), form(&[0x9B], "a,e", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0x9C], "a,h", NONE, Cycles::fixed(1), "ZNHC"), form(&[0x9D], "a,l", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0x9E], "a,[hl]", NONE, Cycles::fixed(2), "ZNHC"), form(&[0x9F], "a,a", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0xDE], "a,N", ONE_N, Cycles::fixed(2), "ZNHC"),
    ]),
    inst!("AND", "Logical AND", [
        form(&[0xA0], "a,b", NONE, Cycles::fixed(1), "ZNHC"), form(&[0xA1], "a,c", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0xA2], "a,d", NONE, Cycles::fixed(1), "ZNHC"), form(&[0xA3], "a,e", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0xA4], "a,h", NONE, Cycles::fixed(1), "ZNHC"), form(&[0xA5], "a,l", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0xA6], "a,[hl]", NONE, Cycles::fixed(2), "ZNHC"), form(&[0xA7], "a,a", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0xE6], "a,N", ONE_N, Cycles::fixed(2), "ZNHC"),
    ]),
    inst!("XOR", "Logical XOR", [
        form(&[0xA8], "a,b", NONE, Cycles::fixed(1), "ZNHC"), form(&[0xA9], "a,c", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0xAA], "a,d", NONE, Cycles::fixed(1), "ZNHC"), form(&[0xAB], "a,e", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0xAC], "a,h", NONE, Cycles::fixed(1), "ZNHC"), form(&[0xAD], "a,l", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0xAE], "a,[hl]", NONE, Cycles::fixed(2), "ZNHC"), form(&[0xAF], "a,a", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0xEE], "a,N", ONE_N, Cycles::fixed(2), "ZNHC"),
    ]),
    inst!("OR", "Logical OR", [
        form(&[0xB0], "a,b", NONE, Cycles::fixed(1), "ZNHC"), form(&[0xB1], "a,c", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0xB2], "a,d", NONE, Cycles::fixed(1), "ZNHC"), form(&[0xB3], "a,e", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0xB4], "a,h", NONE, Cycles::fixed(1), "ZNHC"), form(&[0xB5], "a,l", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0xB6], "a,[hl]", NONE, Cycles::fixed(2), "ZNHC"), form(&[0xB7], "a,a", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0xF6], "a,N", ONE_N, Cycles::fixed(2), "ZNHC"),
    ]),
    inst!("CP", "Compare", [
        form(&[0xB8], "a,b", NONE, Cycles::fixed(1), "ZNHC"), form(&[0xB9], "a,c", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0xBA], "a,d", NONE, Cycles::fixed(1), "ZNHC"), form(&[0xBB], "a,e", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0xBC], "a,h", NONE, Cycles::fixed(1), "ZNHC"), form(&[0xBD], "a,l", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0xBE], "a,[hl]", NONE, Cycles::fixed(2), "ZNHC"), form(&[0xBF], "a,a", NONE, Cycles::fixed(1), "ZNHC"),
        form(&[0xFE], "a,N", ONE_N, Cycles::fixed(2), "ZNHC"),
    ]),

    // INC/DEC — 8-bit (0x04/0x05 + r*8) and 16-bit register pairs.
    inst!("INC", "Increment", [
        form(&[0x04], "b", NONE, Cycles::fixed(1), "ZNH"), form(&[0x0C], "c", NONE, Cycles::fixed(1), "ZNH"),
        form(&[0x14], "d", NONE, Cycles::fixed(1), "ZNH"), form(&[0x1C], "e", NONE, Cycles::fixed(1), "ZNH"),
        form(&[0x24], "h", NONE, Cycles::fixed(1), "ZNH"), form(&[0x2C], "l", NONE, Cycles::fixed(1), "ZNH"),
        form(&[0x34], "[hl]", NONE, Cycles::fixed(3), "ZNH"), form(&[0x3C], "a", NONE, Cycles::fixed(1), "ZNH"),
        form(&[0x03], "bc", NONE, Cycles::fixed(2), ""), form(&[0x13], "de", NONE, Cycles::fixed(2), ""),
        form(&[0x23], "hl", NONE, Cycles::fixed(2), ""), form(&[0x33], "sp", NONE, Cycles::fixed(2), ""),
    ]),
    inst!("DEC", "Decrement", [
        form(&[0x05], "b", NONE, Cycles::fixed(1), "ZNH"), form(&[0x0D], "c", NONE, Cycles::fixed(1), "ZNH"),
        form(&[0x15], "d", NONE, Cycles::fixed(1), "ZNH"), form(&[0x1D], "e", NONE, Cycles::fixed(1), "ZNH"),
        form(&[0x25], "h", NONE, Cycles::fixed(1), "ZNH"), form(&[0x2D], "l", NONE, Cycles::fixed(1), "ZNH"),
        form(&[0x35], "[hl]", NONE, Cycles::fixed(3), "ZNH"), form(&[0x3D], "a", NONE, Cycles::fixed(1), "ZNH"),
        form(&[0x0B], "bc", NONE, Cycles::fixed(2), ""), form(&[0x1B], "de", NONE, Cycles::fixed(2), ""),
        form(&[0x2B], "hl", NONE, Cycles::fixed(2), ""), form(&[0x3B], "sp", NONE, Cycles::fixed(2), ""),
    ]),

    // ===================== jumps, calls, returns =====================
    inst!("JP", "Jump", [
        form(&[0xC3], "NN", ONE_NN, Cycles::fixed(4), ""),
        form(&[0xC2], "nz,NN", ONE_NN, Cycles::fixed(4), ""),
        form(&[0xCA], "z,NN", ONE_NN, Cycles::fixed(4), ""),
        form(&[0xD2], "nc,NN", ONE_NN, Cycles::fixed(4), ""),
        form(&[0xDA], "c,NN", ONE_NN, Cycles::fixed(4), ""),
        form(&[0xE9], "hl", NONE, Cycles::fixed(1), ""),
    ]),
    inst!("JR", "Relative jump", [
        form(&[0x18], "E", ONE_REL, Cycles::fixed(3), ""),
        form(&[0x20], "nz,E", ONE_REL, Cycles::fixed(3), ""),
        form(&[0x28], "z,E", ONE_REL, Cycles::fixed(3), ""),
        form(&[0x30], "nc,E", ONE_REL, Cycles::fixed(3), ""),
        form(&[0x38], "c,E", ONE_REL, Cycles::fixed(3), ""),
    ]),
    inst!("CALL", "Call subroutine", [
        form(&[0xCD], "NN", ONE_NN, Cycles::fixed(6), ""),
        form(&[0xC4], "nz,NN", ONE_NN, Cycles::fixed(6), ""),
        form(&[0xCC], "z,NN", ONE_NN, Cycles::fixed(6), ""),
        form(&[0xD4], "nc,NN", ONE_NN, Cycles::fixed(6), ""),
        form(&[0xDC], "c,NN", ONE_NN, Cycles::fixed(6), ""),
    ]),
    inst!("RET", "Return", [
        form(&[0xC9], "", NONE, Cycles::fixed(4), ""),
        form(&[0xC0], "nz", NONE, Cycles::fixed(2), ""),
        form(&[0xC8], "z", NONE, Cycles::fixed(2), ""),
        form(&[0xD0], "nc", NONE, Cycles::fixed(2), ""),
        form(&[0xD8], "c", NONE, Cycles::fixed(2), ""),
    ]),
    inst!("RETI", "Return from interrupt", [form(&[0xD9], "", NONE, Cycles::fixed(4), "")]),
    inst!("RST", "Restart", [
        form(&[0xC7], "00", NONE, Cycles::fixed(4), ""), form(&[0xCF], "08", NONE, Cycles::fixed(4), ""),
        form(&[0xD7], "10", NONE, Cycles::fixed(4), ""), form(&[0xDF], "18", NONE, Cycles::fixed(4), ""),
        form(&[0xE7], "20", NONE, Cycles::fixed(4), ""), form(&[0xEF], "28", NONE, Cycles::fixed(4), ""),
        form(&[0xF7], "30", NONE, Cycles::fixed(4), ""), form(&[0xFF], "38", NONE, Cycles::fixed(4), ""),
    ]),

    // ===================== stack =====================
    inst!("PUSH", "Push", [
        form(&[0xC5], "bc", NONE, Cycles::fixed(4), ""), form(&[0xD5], "de", NONE, Cycles::fixed(4), ""),
        form(&[0xE5], "hl", NONE, Cycles::fixed(4), ""), form(&[0xF5], "af", NONE, Cycles::fixed(4), ""),
    ]),
    inst!("POP", "Pop", [
        form(&[0xC1], "bc", NONE, Cycles::fixed(3), ""), form(&[0xD1], "de", NONE, Cycles::fixed(3), ""),
        form(&[0xE1], "hl", NONE, Cycles::fixed(3), ""), form(&[0xF1], "af", NONE, Cycles::fixed(3), "ZNHC"),
    ]),

    // ===================== CB prefix: rotates / shifts =====================
    // Register order is the SM83 standard b,c,d,e,h,l,[hl],a (codes 0..7);
    // `[hl]` (code 6) is the slower memory form.
    inst!("RLC", "Rotate left", [
        cbf!(0x00,"b",2,"ZNHC"), cbf!(0x01,"c",2,"ZNHC"), cbf!(0x02,"d",2,"ZNHC"), cbf!(0x03,"e",2,"ZNHC"),
        cbf!(0x04,"h",2,"ZNHC"), cbf!(0x05,"l",2,"ZNHC"), cbf!(0x06,"[hl]",4,"ZNHC"), cbf!(0x07,"a",2,"ZNHC"),
    ]),
    inst!("RRC", "Rotate right", [
        cbf!(0x08,"b",2,"ZNHC"), cbf!(0x09,"c",2,"ZNHC"), cbf!(0x0A,"d",2,"ZNHC"), cbf!(0x0B,"e",2,"ZNHC"),
        cbf!(0x0C,"h",2,"ZNHC"), cbf!(0x0D,"l",2,"ZNHC"), cbf!(0x0E,"[hl]",4,"ZNHC"), cbf!(0x0F,"a",2,"ZNHC"),
    ]),
    inst!("RL", "Rotate left thru CY", [
        cbf!(0x10,"b",2,"ZNHC"), cbf!(0x11,"c",2,"ZNHC"), cbf!(0x12,"d",2,"ZNHC"), cbf!(0x13,"e",2,"ZNHC"),
        cbf!(0x14,"h",2,"ZNHC"), cbf!(0x15,"l",2,"ZNHC"), cbf!(0x16,"[hl]",4,"ZNHC"), cbf!(0x17,"a",2,"ZNHC"),
    ]),
    inst!("RR", "Rotate right thru CY", [
        cbf!(0x18,"b",2,"ZNHC"), cbf!(0x19,"c",2,"ZNHC"), cbf!(0x1A,"d",2,"ZNHC"), cbf!(0x1B,"e",2,"ZNHC"),
        cbf!(0x1C,"h",2,"ZNHC"), cbf!(0x1D,"l",2,"ZNHC"), cbf!(0x1E,"[hl]",4,"ZNHC"), cbf!(0x1F,"a",2,"ZNHC"),
    ]),
    inst!("SLA", "Shift left arithmetic", [
        cbf!(0x20,"b",2,"ZNHC"), cbf!(0x21,"c",2,"ZNHC"), cbf!(0x22,"d",2,"ZNHC"), cbf!(0x23,"e",2,"ZNHC"),
        cbf!(0x24,"h",2,"ZNHC"), cbf!(0x25,"l",2,"ZNHC"), cbf!(0x26,"[hl]",4,"ZNHC"), cbf!(0x27,"a",2,"ZNHC"),
    ]),
    inst!("SRA", "Shift right arithmetic", [
        cbf!(0x28,"b",2,"ZNHC"), cbf!(0x29,"c",2,"ZNHC"), cbf!(0x2A,"d",2,"ZNHC"), cbf!(0x2B,"e",2,"ZNHC"),
        cbf!(0x2C,"h",2,"ZNHC"), cbf!(0x2D,"l",2,"ZNHC"), cbf!(0x2E,"[hl]",4,"ZNHC"), cbf!(0x2F,"a",2,"ZNHC"),
    ]),
    inst!("SWAP", "Swap nibbles", [
        cbf!(0x30,"b",2,"ZNHC"), cbf!(0x31,"c",2,"ZNHC"), cbf!(0x32,"d",2,"ZNHC"), cbf!(0x33,"e",2,"ZNHC"),
        cbf!(0x34,"h",2,"ZNHC"), cbf!(0x35,"l",2,"ZNHC"), cbf!(0x36,"[hl]",4,"ZNHC"), cbf!(0x37,"a",2,"ZNHC"),
    ]),
    inst!("SRL", "Shift right logical", [
        cbf!(0x38,"b",2,"ZNHC"), cbf!(0x39,"c",2,"ZNHC"), cbf!(0x3A,"d",2,"ZNHC"), cbf!(0x3B,"e",2,"ZNHC"),
        cbf!(0x3C,"h",2,"ZNHC"), cbf!(0x3D,"l",2,"ZNHC"), cbf!(0x3E,"[hl]",4,"ZNHC"), cbf!(0x3F,"a",2,"ZNHC"),
    ]),

    // ===================== CB prefix: bit ops (bit*8 + reg) =====================
    inst!("BIT", "Test bit", [
        cbf!(0x40,"0,b",2,"ZNH"), cbf!(0x41,"0,c",2,"ZNH"), cbf!(0x42,"0,d",2,"ZNH"), cbf!(0x43,"0,e",2,"ZNH"),
        cbf!(0x44,"0,h",2,"ZNH"), cbf!(0x45,"0,l",2,"ZNH"), cbf!(0x46,"0,[hl]",3,"ZNH"), cbf!(0x47,"0,a",2,"ZNH"),
        cbf!(0x48,"1,b",2,"ZNH"), cbf!(0x49,"1,c",2,"ZNH"), cbf!(0x4A,"1,d",2,"ZNH"), cbf!(0x4B,"1,e",2,"ZNH"),
        cbf!(0x4C,"1,h",2,"ZNH"), cbf!(0x4D,"1,l",2,"ZNH"), cbf!(0x4E,"1,[hl]",3,"ZNH"), cbf!(0x4F,"1,a",2,"ZNH"),
        cbf!(0x50,"2,b",2,"ZNH"), cbf!(0x51,"2,c",2,"ZNH"), cbf!(0x52,"2,d",2,"ZNH"), cbf!(0x53,"2,e",2,"ZNH"),
        cbf!(0x54,"2,h",2,"ZNH"), cbf!(0x55,"2,l",2,"ZNH"), cbf!(0x56,"2,[hl]",3,"ZNH"), cbf!(0x57,"2,a",2,"ZNH"),
        cbf!(0x58,"3,b",2,"ZNH"), cbf!(0x59,"3,c",2,"ZNH"), cbf!(0x5A,"3,d",2,"ZNH"), cbf!(0x5B,"3,e",2,"ZNH"),
        cbf!(0x5C,"3,h",2,"ZNH"), cbf!(0x5D,"3,l",2,"ZNH"), cbf!(0x5E,"3,[hl]",3,"ZNH"), cbf!(0x5F,"3,a",2,"ZNH"),
        cbf!(0x60,"4,b",2,"ZNH"), cbf!(0x61,"4,c",2,"ZNH"), cbf!(0x62,"4,d",2,"ZNH"), cbf!(0x63,"4,e",2,"ZNH"),
        cbf!(0x64,"4,h",2,"ZNH"), cbf!(0x65,"4,l",2,"ZNH"), cbf!(0x66,"4,[hl]",3,"ZNH"), cbf!(0x67,"4,a",2,"ZNH"),
        cbf!(0x68,"5,b",2,"ZNH"), cbf!(0x69,"5,c",2,"ZNH"), cbf!(0x6A,"5,d",2,"ZNH"), cbf!(0x6B,"5,e",2,"ZNH"),
        cbf!(0x6C,"5,h",2,"ZNH"), cbf!(0x6D,"5,l",2,"ZNH"), cbf!(0x6E,"5,[hl]",3,"ZNH"), cbf!(0x6F,"5,a",2,"ZNH"),
        cbf!(0x70,"6,b",2,"ZNH"), cbf!(0x71,"6,c",2,"ZNH"), cbf!(0x72,"6,d",2,"ZNH"), cbf!(0x73,"6,e",2,"ZNH"),
        cbf!(0x74,"6,h",2,"ZNH"), cbf!(0x75,"6,l",2,"ZNH"), cbf!(0x76,"6,[hl]",3,"ZNH"), cbf!(0x77,"6,a",2,"ZNH"),
        cbf!(0x78,"7,b",2,"ZNH"), cbf!(0x79,"7,c",2,"ZNH"), cbf!(0x7A,"7,d",2,"ZNH"), cbf!(0x7B,"7,e",2,"ZNH"),
        cbf!(0x7C,"7,h",2,"ZNH"), cbf!(0x7D,"7,l",2,"ZNH"), cbf!(0x7E,"7,[hl]",3,"ZNH"), cbf!(0x7F,"7,a",2,"ZNH"),
    ]),
    inst!("RES", "Reset bit", [
        cbf!(0x80,"0,b",2,""), cbf!(0x81,"0,c",2,""), cbf!(0x82,"0,d",2,""), cbf!(0x83,"0,e",2,""),
        cbf!(0x84,"0,h",2,""), cbf!(0x85,"0,l",2,""), cbf!(0x86,"0,[hl]",4,""), cbf!(0x87,"0,a",2,""),
        cbf!(0x88,"1,b",2,""), cbf!(0x89,"1,c",2,""), cbf!(0x8A,"1,d",2,""), cbf!(0x8B,"1,e",2,""),
        cbf!(0x8C,"1,h",2,""), cbf!(0x8D,"1,l",2,""), cbf!(0x8E,"1,[hl]",4,""), cbf!(0x8F,"1,a",2,""),
        cbf!(0x90,"2,b",2,""), cbf!(0x91,"2,c",2,""), cbf!(0x92,"2,d",2,""), cbf!(0x93,"2,e",2,""),
        cbf!(0x94,"2,h",2,""), cbf!(0x95,"2,l",2,""), cbf!(0x96,"2,[hl]",4,""), cbf!(0x97,"2,a",2,""),
        cbf!(0x98,"3,b",2,""), cbf!(0x99,"3,c",2,""), cbf!(0x9A,"3,d",2,""), cbf!(0x9B,"3,e",2,""),
        cbf!(0x9C,"3,h",2,""), cbf!(0x9D,"3,l",2,""), cbf!(0x9E,"3,[hl]",4,""), cbf!(0x9F,"3,a",2,""),
        cbf!(0xA0,"4,b",2,""), cbf!(0xA1,"4,c",2,""), cbf!(0xA2,"4,d",2,""), cbf!(0xA3,"4,e",2,""),
        cbf!(0xA4,"4,h",2,""), cbf!(0xA5,"4,l",2,""), cbf!(0xA6,"4,[hl]",4,""), cbf!(0xA7,"4,a",2,""),
        cbf!(0xA8,"5,b",2,""), cbf!(0xA9,"5,c",2,""), cbf!(0xAA,"5,d",2,""), cbf!(0xAB,"5,e",2,""),
        cbf!(0xAC,"5,h",2,""), cbf!(0xAD,"5,l",2,""), cbf!(0xAE,"5,[hl]",4,""), cbf!(0xAF,"5,a",2,""),
        cbf!(0xB0,"6,b",2,""), cbf!(0xB1,"6,c",2,""), cbf!(0xB2,"6,d",2,""), cbf!(0xB3,"6,e",2,""),
        cbf!(0xB4,"6,h",2,""), cbf!(0xB5,"6,l",2,""), cbf!(0xB6,"6,[hl]",4,""), cbf!(0xB7,"6,a",2,""),
        cbf!(0xB8,"7,b",2,""), cbf!(0xB9,"7,c",2,""), cbf!(0xBA,"7,d",2,""), cbf!(0xBB,"7,e",2,""),
        cbf!(0xBC,"7,h",2,""), cbf!(0xBD,"7,l",2,""), cbf!(0xBE,"7,[hl]",4,""), cbf!(0xBF,"7,a",2,""),
    ]),
    inst!("SET", "Set bit", [
        cbf!(0xC0,"0,b",2,""), cbf!(0xC1,"0,c",2,""), cbf!(0xC2,"0,d",2,""), cbf!(0xC3,"0,e",2,""),
        cbf!(0xC4,"0,h",2,""), cbf!(0xC5,"0,l",2,""), cbf!(0xC6,"0,[hl]",4,""), cbf!(0xC7,"0,a",2,""),
        cbf!(0xC8,"1,b",2,""), cbf!(0xC9,"1,c",2,""), cbf!(0xCA,"1,d",2,""), cbf!(0xCB,"1,e",2,""),
        cbf!(0xCC,"1,h",2,""), cbf!(0xCD,"1,l",2,""), cbf!(0xCE,"1,[hl]",4,""), cbf!(0xCF,"1,a",2,""),
        cbf!(0xD0,"2,b",2,""), cbf!(0xD1,"2,c",2,""), cbf!(0xD2,"2,d",2,""), cbf!(0xD3,"2,e",2,""),
        cbf!(0xD4,"2,h",2,""), cbf!(0xD5,"2,l",2,""), cbf!(0xD6,"2,[hl]",4,""), cbf!(0xD7,"2,a",2,""),
        cbf!(0xD8,"3,b",2,""), cbf!(0xD9,"3,c",2,""), cbf!(0xDA,"3,d",2,""), cbf!(0xDB,"3,e",2,""),
        cbf!(0xDC,"3,h",2,""), cbf!(0xDD,"3,l",2,""), cbf!(0xDE,"3,[hl]",4,""), cbf!(0xDF,"3,a",2,""),
        cbf!(0xE0,"4,b",2,""), cbf!(0xE1,"4,c",2,""), cbf!(0xE2,"4,d",2,""), cbf!(0xE3,"4,e",2,""),
        cbf!(0xE4,"4,h",2,""), cbf!(0xE5,"4,l",2,""), cbf!(0xE6,"4,[hl]",4,""), cbf!(0xE7,"4,a",2,""),
        cbf!(0xE8,"5,b",2,""), cbf!(0xE9,"5,c",2,""), cbf!(0xEA,"5,d",2,""), cbf!(0xEB,"5,e",2,""),
        cbf!(0xEC,"5,h",2,""), cbf!(0xED,"5,l",2,""), cbf!(0xEE,"5,[hl]",4,""), cbf!(0xEF,"5,a",2,""),
        cbf!(0xF0,"6,b",2,""), cbf!(0xF1,"6,c",2,""), cbf!(0xF2,"6,d",2,""), cbf!(0xF3,"6,e",2,""),
        cbf!(0xF4,"6,h",2,""), cbf!(0xF5,"6,l",2,""), cbf!(0xF6,"6,[hl]",4,""), cbf!(0xF7,"6,a",2,""),
        cbf!(0xF8,"7,b",2,""), cbf!(0xF9,"7,c",2,""), cbf!(0xFA,"7,d",2,""), cbf!(0xFB,"7,e",2,""),
        cbf!(0xFC,"7,h",2,""), cbf!(0xFD,"7,l",2,""), cbf!(0xFE,"7,[hl]",4,""), cbf!(0xFF,"7,a",2,""),
    ]),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spot_check_opcodes() {
        let op = |m: &str, mode: &str| SET.find_form(m, mode).map(|f| f.opcode);
        assert_eq!(op("NOP", ""), Some(&[0x00][..]));
        assert_eq!(op("STOP", ""), Some(&[0x10, 0x00][..]));
        assert_eq!(op("LD", "a,[hl]"), Some(&[0x7E][..]));
        assert_eq!(op("LD", "[hl+],a"), Some(&[0x22][..]));
        assert_eq!(op("LD", "hl,sp+D"), Some(&[0xF8][..]));
        assert_eq!(op("LDH", "[$ff00+N],a"), Some(&[0xE0][..]));
        assert_eq!(op("ADD", "sp,D"), Some(&[0xE8][..]));
        assert_eq!(op("JR", "nz,E"), Some(&[0x20][..]));
        assert_eq!(op("RST", "38"), Some(&[0xFF][..]));
        assert_eq!(op("SWAP", "a"), Some(&[0xCB, 0x37][..]));
        assert_eq!(op("BIT", "7,[hl]"), Some(&[0xCB, 0x7E][..]));
        assert_eq!(op("SET", "0,b"), Some(&[0xCB, 0xC0][..]));
    }

    #[test]
    fn ld_rr_block_excludes_halt() {
        // 0x76 is HALT, not LD [hl],[hl].
        assert!(SET.find_form("LD", "[hl],[hl]").is_none());
        assert_eq!(
            SET.find_form("HALT", "").map(|f| f.opcode),
            Some(&[0x76][..])
        );
    }

    #[test]
    fn cb_page_is_complete() {
        // Every CB second-byte 0x00..=0xFF is reachable by exactly one form.
        let mut seen = [false; 256];
        for insn in INSTRUCTIONS {
            for f in insn.forms {
                if f.opcode.len() == 2 && f.opcode[0] == 0xCB {
                    assert!(!seen[f.opcode[1] as usize], "dup CB {:02X}", f.opcode[1]);
                    seen[f.opcode[1] as usize] = true;
                }
            }
        }
        assert!(seen.iter().all(|&s| s), "CB page has gaps");
    }
}
