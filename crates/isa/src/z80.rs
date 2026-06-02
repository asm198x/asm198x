//! Zilog Z80 instruction set.
//!
//! Authored from the Z80 datasheet / Zilog programming manual and cross-checked
//! against the family's Fuse- and ZEXALL-validated `zilog-z80` decoder in
//! Emu198x — the spec is *authored*, not extracted (see
//! `../../../decisions/asm198x-and-shared-isa-spec.md`).
//!
//! ## Mode labels are operand signatures
//!
//! The Z80 encodes most operands (registers, conditions) *in the opcode byte*,
//! not as trailing bytes. So a [`Form`]'s `mode` is the full operand signature
//! as written in source, minus the mnemonic: `LD A,B` is mnemonic `"LD"`, mode
//! `"A,B"`; `LD B,n` is mode `"B,n"`; `JR NZ,e` is mnemonic `"JR"`, mode
//! `"NZ,e"`. Only the parts that become bytes on the wire appear in
//! `operands`: `n` (8-bit immediate), `nn` (16-bit immediate), `(nn)` (16-bit
//! address), the I/O port in `IN`/`OUT`, and `e` (relative displacement).
//! Registers and condition codes carry no operand bytes. Each dialect maps its
//! parsed operands to these labels.
//!
//! ## Flags
//!
//! Flag strings name the bits a form modifies, drawn from `SZHPNC` — S (sign),
//! Z (zero), H (half-carry), P (parity/overflow, the P/V bit), N (add/subtract),
//! C (carry). `""` means no documented flag is touched. A letter means the bit
//! is *modified* (set, reset, or computed) — the precise rule is the
//! datasheet's. The undocumented F3/F5 bits are not tracked. The assembler
//! ignores flags; they serve the disassembler, documentation, and conformance.
//!
//! ## Cycles
//!
//! `base` is the T-state count. Conditional control flow uses
//! [`cond`]: `base` is the not-taken cost and `branch_taken` the extra T-states
//! when the condition holds (the Z80 has no 6502-style page-cross term).
//!
//! ## Coverage
//!
//! This module is authored in slices. **Landed: the complete unprefixed base
//! page `0x00..=0xFF`** (less the four prefix bytes) — the load group, 8-bit
//! and 16-bit arithmetic, INC/DEC, the full control-flow set (JP/CALL/RET/JR/
//! DJNZ/RST), stack ops, the accumulator/flag ops, and block-free I/O. **TODO:**
//! the `CB` (bit/rotate), `ED` (extended), and `DD`/`FD` (IX/IY) prefix groups.

use crate::{Cycles, Endianness, Form, Instruction, InstructionSet, Operand, OperandKind};

const IMM8: Operand = Operand {
    kind: OperandKind::Immediate,
    bytes: 1,
}; // n
const IMM16: Operand = Operand {
    kind: OperandKind::Immediate,
    bytes: 2,
}; // nn
const ADDR16: Operand = Operand {
    kind: OperandKind::Address,
    bytes: 2,
}; // (nn)
const REL8: Operand = Operand {
    kind: OperandKind::RelativePc,
    bytes: 1,
}; // e

const NONE: &[Operand] = &[];
const ONE_N: &[Operand] = &[IMM8];
const ONE_NN: &[Operand] = &[IMM16];
const ONE_ADDR: &[Operand] = &[ADDR16];
const ONE_E: &[Operand] = &[REL8];

/// The Z80 instruction set: the single source of truth for Z80 encoding.
pub const SET: InstructionSet = InstructionSet {
    cpu: "Zilog Z80",
    endianness: Endianness::Little,
    instructions: INSTRUCTIONS,
};

/// Build one form (see [`crate::Form`]). Opcode bytes are a `'static` slice
/// literal, written inline at each call site — one byte for base-page opcodes,
/// a prefix sequence for the `CB`/`ED`/`DD`/`FD` groups.
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
        cycles,
        flags,
        undocumented: false,
    }
}

/// Conditional control flow: `not_taken` T-states if the condition fails, plus
/// `taken_extra` more when it holds.
const fn cond(not_taken: u8, taken_extra: u8) -> Cycles {
    Cycles {
        base: not_taken,
        page_cross: 0,
        branch_taken: taken_extra,
    }
}

macro_rules! inst {
    ($mnemonic:literal, $summary:literal, [ $($form:expr),* $(,)? ]) => {
        Instruction { mnemonic: $mnemonic, summary: $summary, forms: &[ $($form),* ] }
    };
}

#[rustfmt::skip]
const INSTRUCTIONS: &[Instruction] = &[
    inst!("NOP", "No operation", [form(&[0x00], "", NONE, Cycles::fixed(4), "")]),

    inst!("LD", "Load", [
        // 8-bit register <- register (0x40..=0x7F, less HALT at 0x76).
        form(&[0x40], "B,B",    NONE, Cycles::fixed(4), ""),
        form(&[0x41], "B,C",    NONE, Cycles::fixed(4), ""),
        form(&[0x42], "B,D",    NONE, Cycles::fixed(4), ""),
        form(&[0x43], "B,E",    NONE, Cycles::fixed(4), ""),
        form(&[0x44], "B,H",    NONE, Cycles::fixed(4), ""),
        form(&[0x45], "B,L",    NONE, Cycles::fixed(4), ""),
        form(&[0x46], "B,(HL)", NONE, Cycles::fixed(7), ""),
        form(&[0x47], "B,A",    NONE, Cycles::fixed(4), ""),
        form(&[0x48], "C,B",    NONE, Cycles::fixed(4), ""),
        form(&[0x49], "C,C",    NONE, Cycles::fixed(4), ""),
        form(&[0x4A], "C,D",    NONE, Cycles::fixed(4), ""),
        form(&[0x4B], "C,E",    NONE, Cycles::fixed(4), ""),
        form(&[0x4C], "C,H",    NONE, Cycles::fixed(4), ""),
        form(&[0x4D], "C,L",    NONE, Cycles::fixed(4), ""),
        form(&[0x4E], "C,(HL)", NONE, Cycles::fixed(7), ""),
        form(&[0x4F], "C,A",    NONE, Cycles::fixed(4), ""),
        form(&[0x50], "D,B",    NONE, Cycles::fixed(4), ""),
        form(&[0x51], "D,C",    NONE, Cycles::fixed(4), ""),
        form(&[0x52], "D,D",    NONE, Cycles::fixed(4), ""),
        form(&[0x53], "D,E",    NONE, Cycles::fixed(4), ""),
        form(&[0x54], "D,H",    NONE, Cycles::fixed(4), ""),
        form(&[0x55], "D,L",    NONE, Cycles::fixed(4), ""),
        form(&[0x56], "D,(HL)", NONE, Cycles::fixed(7), ""),
        form(&[0x57], "D,A",    NONE, Cycles::fixed(4), ""),
        form(&[0x58], "E,B",    NONE, Cycles::fixed(4), ""),
        form(&[0x59], "E,C",    NONE, Cycles::fixed(4), ""),
        form(&[0x5A], "E,D",    NONE, Cycles::fixed(4), ""),
        form(&[0x5B], "E,E",    NONE, Cycles::fixed(4), ""),
        form(&[0x5C], "E,H",    NONE, Cycles::fixed(4), ""),
        form(&[0x5D], "E,L",    NONE, Cycles::fixed(4), ""),
        form(&[0x5E], "E,(HL)", NONE, Cycles::fixed(7), ""),
        form(&[0x5F], "E,A",    NONE, Cycles::fixed(4), ""),
        form(&[0x60], "H,B",    NONE, Cycles::fixed(4), ""),
        form(&[0x61], "H,C",    NONE, Cycles::fixed(4), ""),
        form(&[0x62], "H,D",    NONE, Cycles::fixed(4), ""),
        form(&[0x63], "H,E",    NONE, Cycles::fixed(4), ""),
        form(&[0x64], "H,H",    NONE, Cycles::fixed(4), ""),
        form(&[0x65], "H,L",    NONE, Cycles::fixed(4), ""),
        form(&[0x66], "H,(HL)", NONE, Cycles::fixed(7), ""),
        form(&[0x67], "H,A",    NONE, Cycles::fixed(4), ""),
        form(&[0x68], "L,B",    NONE, Cycles::fixed(4), ""),
        form(&[0x69], "L,C",    NONE, Cycles::fixed(4), ""),
        form(&[0x6A], "L,D",    NONE, Cycles::fixed(4), ""),
        form(&[0x6B], "L,E",    NONE, Cycles::fixed(4), ""),
        form(&[0x6C], "L,H",    NONE, Cycles::fixed(4), ""),
        form(&[0x6D], "L,L",    NONE, Cycles::fixed(4), ""),
        form(&[0x6E], "L,(HL)", NONE, Cycles::fixed(7), ""),
        form(&[0x6F], "L,A",    NONE, Cycles::fixed(4), ""),
        form(&[0x70], "(HL),B", NONE, Cycles::fixed(7), ""),
        form(&[0x71], "(HL),C", NONE, Cycles::fixed(7), ""),
        form(&[0x72], "(HL),D", NONE, Cycles::fixed(7), ""),
        form(&[0x73], "(HL),E", NONE, Cycles::fixed(7), ""),
        form(&[0x74], "(HL),H", NONE, Cycles::fixed(7), ""),
        form(&[0x75], "(HL),L", NONE, Cycles::fixed(7), ""),
        form(&[0x77], "(HL),A", NONE, Cycles::fixed(7), ""),
        form(&[0x78], "A,B",    NONE, Cycles::fixed(4), ""),
        form(&[0x79], "A,C",    NONE, Cycles::fixed(4), ""),
        form(&[0x7A], "A,D",    NONE, Cycles::fixed(4), ""),
        form(&[0x7B], "A,E",    NONE, Cycles::fixed(4), ""),
        form(&[0x7C], "A,H",    NONE, Cycles::fixed(4), ""),
        form(&[0x7D], "A,L",    NONE, Cycles::fixed(4), ""),
        form(&[0x7E], "A,(HL)", NONE, Cycles::fixed(7), ""),
        form(&[0x7F], "A,A",    NONE, Cycles::fixed(4), ""),
        // 8-bit register <- immediate.
        form(&[0x06], "B,n",    ONE_N, Cycles::fixed(7),  ""),
        form(&[0x0E], "C,n",    ONE_N, Cycles::fixed(7),  ""),
        form(&[0x16], "D,n",    ONE_N, Cycles::fixed(7),  ""),
        form(&[0x1E], "E,n",    ONE_N, Cycles::fixed(7),  ""),
        form(&[0x26], "H,n",    ONE_N, Cycles::fixed(7),  ""),
        form(&[0x2E], "L,n",    ONE_N, Cycles::fixed(7),  ""),
        form(&[0x36], "(HL),n", ONE_N, Cycles::fixed(10), ""),
        form(&[0x3E], "A,n",    ONE_N, Cycles::fixed(7),  ""),
        // 16-bit register pair <- immediate.
        form(&[0x01], "BC,nn",  ONE_NN, Cycles::fixed(10), ""),
        form(&[0x11], "DE,nn",  ONE_NN, Cycles::fixed(10), ""),
        form(&[0x21], "HL,nn",  ONE_NN, Cycles::fixed(10), ""),
        form(&[0x31], "SP,nn",  ONE_NN, Cycles::fixed(10), ""),
        // Loads/stores through register pairs and absolute addresses.
        form(&[0x02], "(BC),A", NONE,     Cycles::fixed(7),  ""),
        form(&[0x12], "(DE),A", NONE,     Cycles::fixed(7),  ""),
        form(&[0x0A], "A,(BC)", NONE,     Cycles::fixed(7),  ""),
        form(&[0x1A], "A,(DE)", NONE,     Cycles::fixed(7),  ""),
        form(&[0x22], "(nn),HL", ONE_ADDR, Cycles::fixed(16), ""),
        form(&[0x2A], "HL,(nn)", ONE_ADDR, Cycles::fixed(16), ""),
        form(&[0x32], "(nn),A",  ONE_ADDR, Cycles::fixed(13), ""),
        form(&[0x3A], "A,(nn)",  ONE_ADDR, Cycles::fixed(13), ""),
        // 16-bit register transfer.
        form(&[0xF9], "SP,HL",   NONE,     Cycles::fixed(6),  ""),
    ]),

    inst!("EX", "Exchange", [
        form(&[0x08], "AF,AF'",  NONE, Cycles::fixed(4),  ""),
        form(&[0xEB], "DE,HL",   NONE, Cycles::fixed(4),  ""),
        form(&[0xE3], "(SP),HL", NONE, Cycles::fixed(19), ""),
    ]),

    inst!("DJNZ", "Decrement B and jump if not zero", [
        form(&[0x10], "e", ONE_E, cond(8, 5), ""),
    ]),
    inst!("JR", "Jump relative", [
        form(&[0x18], "e",    ONE_E, cond(7, 5), ""),
        form(&[0x20], "NZ,e", ONE_E, cond(7, 5), ""),
        form(&[0x28], "Z,e",  ONE_E, cond(7, 5), ""),
        form(&[0x30], "NC,e", ONE_E, cond(7, 5), ""),
        form(&[0x38], "C,e",  ONE_E, cond(7, 5), ""),
    ]),

    inst!("ADD", "Add", [
        // 16-bit add to HL: H, N (reset), C; S/Z/P unaffected.
        form(&[0x09], "HL,BC", NONE, Cycles::fixed(11), "HNC"),
        form(&[0x19], "HL,DE", NONE, Cycles::fixed(11), "HNC"),
        form(&[0x29], "HL,HL", NONE, Cycles::fixed(11), "HNC"),
        form(&[0x39], "HL,SP", NONE, Cycles::fixed(11), "HNC"),
        // 8-bit add to A.
        form(&[0x80], "A,B",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0x81], "A,C",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0x82], "A,D",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0x83], "A,E",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0x84], "A,H",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0x85], "A,L",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0x86], "A,(HL)", NONE,  Cycles::fixed(7), "SZHPNC"),
        form(&[0x87], "A,A",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xC6], "A,n",    ONE_N, Cycles::fixed(7), "SZHPNC"),
    ]),
    inst!("ADC", "Add with carry", [
        form(&[0x88], "A,B",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0x89], "A,C",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0x8A], "A,D",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0x8B], "A,E",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0x8C], "A,H",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0x8D], "A,L",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0x8E], "A,(HL)", NONE,  Cycles::fixed(7), "SZHPNC"),
        form(&[0x8F], "A,A",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xCE], "A,n",    ONE_N, Cycles::fixed(7), "SZHPNC"),
    ]),
    inst!("SUB", "Subtract", [
        form(&[0x90], "B",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0x91], "C",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0x92], "D",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0x93], "E",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0x94], "H",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0x95], "L",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0x96], "(HL)", NONE,  Cycles::fixed(7), "SZHPNC"),
        form(&[0x97], "A",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xD6], "n",    ONE_N, Cycles::fixed(7), "SZHPNC"),
    ]),
    inst!("SBC", "Subtract with carry", [
        // 8-bit subtract-with-carry from A. (SBC HL,ss is an ED-prefix op.)
        form(&[0x98], "A,B",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0x99], "A,C",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0x9A], "A,D",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0x9B], "A,E",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0x9C], "A,H",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0x9D], "A,L",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0x9E], "A,(HL)", NONE,  Cycles::fixed(7), "SZHPNC"),
        form(&[0x9F], "A,A",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xDE], "A,n",    ONE_N, Cycles::fixed(7), "SZHPNC"),
    ]),
    inst!("AND", "Logical AND", [
        form(&[0xA0], "B",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xA1], "C",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xA2], "D",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xA3], "E",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xA4], "H",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xA5], "L",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xA6], "(HL)", NONE,  Cycles::fixed(7), "SZHPNC"),
        form(&[0xA7], "A",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xE6], "n",    ONE_N, Cycles::fixed(7), "SZHPNC"),
    ]),
    inst!("XOR", "Logical exclusive OR", [
        form(&[0xA8], "B",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xA9], "C",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xAA], "D",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xAB], "E",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xAC], "H",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xAD], "L",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xAE], "(HL)", NONE,  Cycles::fixed(7), "SZHPNC"),
        form(&[0xAF], "A",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xEE], "n",    ONE_N, Cycles::fixed(7), "SZHPNC"),
    ]),
    inst!("OR", "Logical inclusive OR", [
        form(&[0xB0], "B",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xB1], "C",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xB2], "D",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xB3], "E",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xB4], "H",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xB5], "L",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xB6], "(HL)", NONE,  Cycles::fixed(7), "SZHPNC"),
        form(&[0xB7], "A",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xF6], "n",    ONE_N, Cycles::fixed(7), "SZHPNC"),
    ]),
    inst!("CP", "Compare with A", [
        form(&[0xB8], "B",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xB9], "C",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xBA], "D",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xBB], "E",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xBC], "H",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xBD], "L",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xBE], "(HL)", NONE,  Cycles::fixed(7), "SZHPNC"),
        form(&[0xBF], "A",    NONE,  Cycles::fixed(4), "SZHPNC"),
        form(&[0xFE], "n",    ONE_N, Cycles::fixed(7), "SZHPNC"),
    ]),

    inst!("INC", "Increment", [
        // 16-bit pairs: no flags.
        form(&[0x03], "BC",   NONE, Cycles::fixed(6),  ""),
        form(&[0x13], "DE",   NONE, Cycles::fixed(6),  ""),
        form(&[0x23], "HL",   NONE, Cycles::fixed(6),  ""),
        form(&[0x33], "SP",   NONE, Cycles::fixed(6),  ""),
        // 8-bit: S Z H P/V N (carry untouched).
        form(&[0x04], "B",    NONE, Cycles::fixed(4),  "SZHPN"),
        form(&[0x0C], "C",    NONE, Cycles::fixed(4),  "SZHPN"),
        form(&[0x14], "D",    NONE, Cycles::fixed(4),  "SZHPN"),
        form(&[0x1C], "E",    NONE, Cycles::fixed(4),  "SZHPN"),
        form(&[0x24], "H",    NONE, Cycles::fixed(4),  "SZHPN"),
        form(&[0x2C], "L",    NONE, Cycles::fixed(4),  "SZHPN"),
        form(&[0x34], "(HL)", NONE, Cycles::fixed(11), "SZHPN"),
        form(&[0x3C], "A",    NONE, Cycles::fixed(4),  "SZHPN"),
    ]),
    inst!("DEC", "Decrement", [
        form(&[0x0B], "BC",   NONE, Cycles::fixed(6),  ""),
        form(&[0x1B], "DE",   NONE, Cycles::fixed(6),  ""),
        form(&[0x2B], "HL",   NONE, Cycles::fixed(6),  ""),
        form(&[0x3B], "SP",   NONE, Cycles::fixed(6),  ""),
        form(&[0x05], "B",    NONE, Cycles::fixed(4),  "SZHPN"),
        form(&[0x0D], "C",    NONE, Cycles::fixed(4),  "SZHPN"),
        form(&[0x15], "D",    NONE, Cycles::fixed(4),  "SZHPN"),
        form(&[0x1D], "E",    NONE, Cycles::fixed(4),  "SZHPN"),
        form(&[0x25], "H",    NONE, Cycles::fixed(4),  "SZHPN"),
        form(&[0x2D], "L",    NONE, Cycles::fixed(4),  "SZHPN"),
        form(&[0x35], "(HL)", NONE, Cycles::fixed(11), "SZHPN"),
        form(&[0x3D], "A",    NONE, Cycles::fixed(4),  "SZHPN"),
    ]),

    // Accumulator and flag operations (z = 7).
    inst!("RLCA", "Rotate A left circular",  [form(&[0x07], "", NONE, Cycles::fixed(4), "HNC")]),
    inst!("RRCA", "Rotate A right circular", [form(&[0x0F], "", NONE, Cycles::fixed(4), "HNC")]),
    inst!("RLA",  "Rotate A left through carry",  [form(&[0x17], "", NONE, Cycles::fixed(4), "HNC")]),
    inst!("RRA",  "Rotate A right through carry", [form(&[0x1F], "", NONE, Cycles::fixed(4), "HNC")]),
    inst!("DAA",  "Decimal adjust A",        [form(&[0x27], "", NONE, Cycles::fixed(4), "SZHPC")]),
    inst!("CPL",  "Complement A",            [form(&[0x2F], "", NONE, Cycles::fixed(4), "HN")]),
    inst!("SCF",  "Set carry flag",          [form(&[0x37], "", NONE, Cycles::fixed(4), "HNC")]),
    inst!("CCF",  "Complement carry flag",   [form(&[0x3F], "", NONE, Cycles::fixed(4), "HNC")]),

    inst!("HALT", "Halt", [form(&[0x76], "", NONE, Cycles::fixed(4), "")]),

    // Stack.
    inst!("PUSH", "Push register pair", [
        form(&[0xC5], "BC", NONE, Cycles::fixed(11), ""),
        form(&[0xD5], "DE", NONE, Cycles::fixed(11), ""),
        form(&[0xE5], "HL", NONE, Cycles::fixed(11), ""),
        form(&[0xF5], "AF", NONE, Cycles::fixed(11), ""),
    ]),
    inst!("POP", "Pop register pair", [
        form(&[0xC1], "BC", NONE, Cycles::fixed(10), ""),
        form(&[0xD1], "DE", NONE, Cycles::fixed(10), ""),
        form(&[0xE1], "HL", NONE, Cycles::fixed(10), ""),
        // POP AF loads F from the stack, so every flag is restored.
        form(&[0xF1], "AF", NONE, Cycles::fixed(10), "SZHPNC"),
    ]),

    // Control flow.
    inst!("JP", "Jump", [
        form(&[0xC3], "nn",    ONE_ADDR, Cycles::fixed(10), ""),
        form(&[0xE9], "(HL)",  NONE,     Cycles::fixed(4),  ""),
        form(&[0xC2], "NZ,nn", ONE_ADDR, Cycles::fixed(10), ""),
        form(&[0xCA], "Z,nn",  ONE_ADDR, Cycles::fixed(10), ""),
        form(&[0xD2], "NC,nn", ONE_ADDR, Cycles::fixed(10), ""),
        form(&[0xDA], "C,nn",  ONE_ADDR, Cycles::fixed(10), ""),
        form(&[0xE2], "PO,nn", ONE_ADDR, Cycles::fixed(10), ""),
        form(&[0xEA], "PE,nn", ONE_ADDR, Cycles::fixed(10), ""),
        form(&[0xF2], "P,nn",  ONE_ADDR, Cycles::fixed(10), ""),
        form(&[0xFA], "M,nn",  ONE_ADDR, Cycles::fixed(10), ""),
    ]),
    inst!("CALL", "Call subroutine", [
        form(&[0xCD], "nn",    ONE_ADDR, Cycles::fixed(17), ""),
        form(&[0xC4], "NZ,nn", ONE_ADDR, cond(10, 7), ""),
        form(&[0xCC], "Z,nn",  ONE_ADDR, cond(10, 7), ""),
        form(&[0xD4], "NC,nn", ONE_ADDR, cond(10, 7), ""),
        form(&[0xDC], "C,nn",  ONE_ADDR, cond(10, 7), ""),
        form(&[0xE4], "PO,nn", ONE_ADDR, cond(10, 7), ""),
        form(&[0xEC], "PE,nn", ONE_ADDR, cond(10, 7), ""),
        form(&[0xF4], "P,nn",  ONE_ADDR, cond(10, 7), ""),
        form(&[0xFC], "M,nn",  ONE_ADDR, cond(10, 7), ""),
    ]),
    inst!("RET", "Return", [
        form(&[0xC9], "",   NONE, Cycles::fixed(10), ""),
        form(&[0xC0], "NZ", NONE, cond(5, 6), ""),
        form(&[0xC8], "Z",  NONE, cond(5, 6), ""),
        form(&[0xD0], "NC", NONE, cond(5, 6), ""),
        form(&[0xD8], "C",  NONE, cond(5, 6), ""),
        form(&[0xE0], "PO", NONE, cond(5, 6), ""),
        form(&[0xE8], "PE", NONE, cond(5, 6), ""),
        form(&[0xF0], "P",  NONE, cond(5, 6), ""),
        form(&[0xF8], "M",  NONE, cond(5, 6), ""),
    ]),
    inst!("RST", "Restart", [
        form(&[0xC7], "00", NONE, Cycles::fixed(11), ""),
        form(&[0xCF], "08", NONE, Cycles::fixed(11), ""),
        form(&[0xD7], "10", NONE, Cycles::fixed(11), ""),
        form(&[0xDF], "18", NONE, Cycles::fixed(11), ""),
        form(&[0xE7], "20", NONE, Cycles::fixed(11), ""),
        form(&[0xEF], "28", NONE, Cycles::fixed(11), ""),
        form(&[0xF7], "30", NONE, Cycles::fixed(11), ""),
        form(&[0xFF], "38", NONE, Cycles::fixed(11), ""),
    ]),

    // Block-free I/O and interrupt control.
    inst!("OUT", "Output to port", [form(&[0xD3], "(n),A", ONE_N, Cycles::fixed(11), "")]),
    inst!("IN",  "Input from port", [form(&[0xDB], "A,(n)", ONE_N, Cycles::fixed(11), "")]),
    inst!("EXX", "Exchange register set", [form(&[0xD9], "", NONE, Cycles::fixed(4), "")]),
    inst!("DI",  "Disable interrupts", [form(&[0xF3], "", NONE, Cycles::fixed(4), "")]),
    inst!("EI",  "Enable interrupts",  [form(&[0xFB], "", NONE, Cycles::fixed(4), "")]),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Every single-byte opcode is one base-page instruction, present exactly
    /// once — except the four prefix bytes (`CB`, `DD`, `ED`, `FD`), which
    /// introduce the prefix groups and are never base-page instructions on
    /// their own. This is the completeness + uniqueness gate: a typo, an
    /// omission, or a stray prefix opcode fails here. (Multi-byte prefixed
    /// forms are excluded, so the test keeps working as the prefix slices land.)
    #[test]
    fn base_page_is_complete_and_unique() {
        const PREFIXES: [u8; 4] = [0xCB, 0xDD, 0xED, 0xFD];
        let mut seen = [false; 256];
        for instruction in SET.instructions {
            for f in instruction.forms {
                if f.opcode.len() == 1 {
                    let op = f.opcode[0] as usize;
                    assert!(
                        !seen[op],
                        "duplicate opcode ${:02X} ({} {})",
                        op, instruction.mnemonic, f.mode
                    );
                    seen[op] = true;
                }
            }
        }
        for (op, present) in seen.iter().enumerate() {
            let is_prefix = PREFIXES.contains(&(op as u8));
            assert_eq!(
                *present, !is_prefix,
                "opcode ${op:02X}: present={present}, expected present={}",
                !is_prefix
            );
        }
    }

    /// Each mnemonic appears once, so `SET.instruction()` finds all its forms.
    #[test]
    fn mnemonics_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for instruction in SET.instructions {
            assert!(
                seen.insert(instruction.mnemonic),
                "duplicate mnemonic `{}`",
                instruction.mnemonic
            );
        }
    }

    /// Spot-checks transcribed from the Fuse/ZEXALL-validated Emu198x decoder.
    #[test]
    fn known_encodings() {
        let ld = SET.instruction("LD").expect("LD exists");
        assert_eq!(ld.form("A,n").expect("LD A,n").opcode, &[0x3E]);
        assert_eq!(ld.form("A,B").expect("LD A,B").opcode, &[0x78]);
        assert_eq!(ld.form("(HL),A").expect("LD (HL),A").opcode, &[0x77]);
        assert_eq!(ld.form("HL,nn").expect("LD HL,nn").opcode, &[0x21]);
        assert_eq!(ld.form("A,(nn)").expect("LD A,(nn)").opcode, &[0x3A]);
        assert_eq!(ld.form("HL,nn").expect("len").len(), 3);

        assert_eq!(
            SET.instruction("HALT").expect("HALT").form("").expect("form").opcode,
            &[0x76]
        );

        let jr = SET.instruction("JR").expect("JR exists");
        let nz = jr.form("NZ,e").expect("JR NZ,e");
        assert_eq!(nz.opcode, &[0x20]);
        assert_eq!(nz.operands, &[REL8]);
        assert_eq!(nz.len(), 2);
        assert_eq!(nz.cycles.base, 7);
        assert_eq!(nz.cycles.branch_taken, 5);

        let add = SET.instruction("ADD").expect("ADD exists");
        assert_eq!(add.form("HL,DE").expect("ADD HL,DE").opcode, &[0x19]);
        assert_eq!(add.form("A,B").expect("ADD A,B").opcode, &[0x80]);

        // Upper-half spot-checks.
        assert_eq!(SET.instruction("JP").expect("JP").form("nn").expect("JP nn").opcode, &[0xC3]);
        assert_eq!(SET.instruction("CALL").expect("CALL").form("nn").expect("CALL nn").opcode, &[0xCD]);
        assert_eq!(SET.instruction("RET").expect("RET").form("").expect("RET").opcode, &[0xC9]);
        assert_eq!(SET.instruction("CP").expect("CP").form("(HL)").expect("CP (HL)").opcode, &[0xBE]);
        assert_eq!(SET.instruction("PUSH").expect("PUSH").form("AF").expect("PUSH AF").opcode, &[0xF5]);
        assert_eq!(SET.instruction("RST").expect("RST").form("38").expect("RST 38").opcode, &[0xFF]);
        assert_eq!(SET.instruction("LD").expect("LD").form("SP,HL").expect("LD SP,HL").opcode, &[0xF9]);

        // CALL cc pays 7 extra T-states when taken; JP cc is a flat 10.
        let call_nz = SET.instruction("CALL").expect("CALL").form("NZ,nn").expect("CALL NZ,nn");
        assert_eq!((call_nz.cycles.base, call_nz.cycles.branch_taken), (10, 7));
    }
}
