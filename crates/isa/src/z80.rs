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
//! DJNZ/RST), stack ops, the accumulator/flag ops, and block-free I/O — **plus
//! the complete `ED` (extended) group**: block transfer/search (LDIR/LDDR/CPIR
//! …), block I/O, 16-bit ADC/SBC HL, 16-bit LD (nn), IN/OUT (C), IM, NEG,
//! RRD/RLD, RETI/RETN — the complete `CB` group (rotates/shifts and BIT/RES/SET
//! over every register slot), **and the complete `DD`/`FD` (IX/IY) group**:
//! 16-bit index ops, `(IX+d)`/`(IY+d)` load/store/ALU/INC/DEC, the two-operand
//! `LD (IX+d),n`, and the `DD CB`/`FD CB` bit/rotate forms. The **documented**
//! Z80 is complete; the undocumented IXH/IXL half-registers and SLL `(IX+d)` are
//! omitted, and the Spectrum Next's Z80N opcodes are a separate extension.

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
const DISP8: Operand = Operand {
    kind: OperandKind::Displacement,
    bytes: 1,
}; // the d in (IX+d)
const IMM16_BE: Operand = Operand {
    kind: OperandKind::ImmediateBe,
    bytes: 2,
}; // the big-endian nn of Z80N `push nn`

const NONE: &[Operand] = &[];
const ONE_N: &[Operand] = &[IMM8];
const ONE_NN: &[Operand] = &[IMM16];
const ONE_NN_BE: &[Operand] = &[IMM16_BE]; // Z80N `push nn`
const ONE_ADDR: &[Operand] = &[ADDR16];
const ONE_E: &[Operand] = &[REL8];
const ONE_DISP: &[Operand] = &[DISP8];
const DISP_THEN_N: &[Operand] = &[DISP8, IMM8]; // LD (IX+d),n

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
        suffix: &[],
        cycles,
        flags,
        undocumented: false,
    }
}

/// Build one undocumented form (e.g. the CB-prefix `SLL`).
const fn form_u(
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
        undocumented: true,
    }
}

/// Build a `DD CB`/`FD CB` form: a two-byte prefix, the displacement operand,
/// then a trailing opcode byte (`DD CB <d> <op>`).
const fn form_ddcb(
    opcode: &'static [u8],
    suffix: &'static [u8],
    mode: &'static str,
    cycles: Cycles,
    flags: &'static str,
) -> Form {
    Form {
        opcode,
        mode,
        operands: ONE_DISP,
        suffix,
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
        // ED-prefixed: 16-bit load/store and the I/R registers. (The ED
        // encodings of LD (nn),HL / LD HL,(nn) are redundant with the shorter
        // base-page 0x22/0x2A, so they are omitted -- the assembler picks the
        // short form, as pasmo does.)
        form(&[0xED, 0x43], "(nn),BC", ONE_ADDR, Cycles::fixed(20), ""),
        form(&[0xED, 0x4B], "BC,(nn)", ONE_ADDR, Cycles::fixed(20), ""),
        form(&[0xED, 0x53], "(nn),DE", ONE_ADDR, Cycles::fixed(20), ""),
        form(&[0xED, 0x5B], "DE,(nn)", ONE_ADDR, Cycles::fixed(20), ""),
        form(&[0xED, 0x73], "(nn),SP", ONE_ADDR, Cycles::fixed(20), ""),
        form(&[0xED, 0x7B], "SP,(nn)", ONE_ADDR, Cycles::fixed(20), ""),
        form(&[0xED, 0x47], "I,A",     NONE,     Cycles::fixed(9),  ""),
        form(&[0xED, 0x4F], "R,A",     NONE,     Cycles::fixed(9),  ""),
        form(&[0xED, 0x57], "A,I",     NONE,     Cycles::fixed(9),  "SZHPN"),
        form(&[0xED, 0x5F], "A,R",     NONE,     Cycles::fixed(9),  "SZHPN"),
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
        // ED-prefixed 16-bit add-with-carry to HL.
        form(&[0xED, 0x4A], "HL,BC", NONE, Cycles::fixed(15), "SZHPNC"),
        form(&[0xED, 0x5A], "HL,DE", NONE, Cycles::fixed(15), "SZHPNC"),
        form(&[0xED, 0x6A], "HL,HL", NONE, Cycles::fixed(15), "SZHPNC"),
        form(&[0xED, 0x7A], "HL,SP", NONE, Cycles::fixed(15), "SZHPNC"),
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
        // ED-prefixed 16-bit subtract-with-carry from HL.
        form(&[0xED, 0x42], "HL,BC", NONE, Cycles::fixed(15), "SZHPNC"),
        form(&[0xED, 0x52], "HL,DE", NONE, Cycles::fixed(15), "SZHPNC"),
        form(&[0xED, 0x62], "HL,HL", NONE, Cycles::fixed(15), "SZHPNC"),
        form(&[0xED, 0x72], "HL,SP", NONE, Cycles::fixed(15), "SZHPNC"),
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

    // I/O (port immediate is base page; port-(C) forms are ED-prefixed).
    inst!("OUT", "Output to port", [
        form(&[0xD3],       "(n),A", ONE_N, Cycles::fixed(11), ""),
        form(&[0xED, 0x41], "(C),B", NONE,  Cycles::fixed(12), ""),
        form(&[0xED, 0x49], "(C),C", NONE,  Cycles::fixed(12), ""),
        form(&[0xED, 0x51], "(C),D", NONE,  Cycles::fixed(12), ""),
        form(&[0xED, 0x59], "(C),E", NONE,  Cycles::fixed(12), ""),
        form(&[0xED, 0x61], "(C),H", NONE,  Cycles::fixed(12), ""),
        form(&[0xED, 0x69], "(C),L", NONE,  Cycles::fixed(12), ""),
        form(&[0xED, 0x79], "(C),A", NONE,  Cycles::fixed(12), ""),
    ]),
    inst!("IN", "Input from port", [
        form(&[0xDB],       "A,(n)", ONE_N, Cycles::fixed(11), ""),
        form(&[0xED, 0x40], "B,(C)", NONE,  Cycles::fixed(12), "SZHPN"),
        form(&[0xED, 0x48], "C,(C)", NONE,  Cycles::fixed(12), "SZHPN"),
        form(&[0xED, 0x50], "D,(C)", NONE,  Cycles::fixed(12), "SZHPN"),
        form(&[0xED, 0x58], "E,(C)", NONE,  Cycles::fixed(12), "SZHPN"),
        form(&[0xED, 0x60], "H,(C)", NONE,  Cycles::fixed(12), "SZHPN"),
        form(&[0xED, 0x68], "L,(C)", NONE,  Cycles::fixed(12), "SZHPN"),
        form(&[0xED, 0x78], "A,(C)", NONE,  Cycles::fixed(12), "SZHPN"),
    ]),
    inst!("EXX", "Exchange register set", [form(&[0xD9], "", NONE, Cycles::fixed(4), "")]),
    inst!("DI",  "Disable interrupts", [form(&[0xF3], "", NONE, Cycles::fixed(4), "")]),
    inst!("EI",  "Enable interrupts",  [form(&[0xFB], "", NONE, Cycles::fixed(4), "")]),

    // ============================ ED prefix ============================
    // Extended group. Opcodes cross-checked against the Fuse/ZEXALL-validated
    // Emu198x decoder. (16-bit LD/IN/OUT(C)/ADC/SBC ED forms live on their
    // mnemonics above.) The block-I/O group's flag detail is hardware-fuzzy --
    // some bits are officially undefined -- but the opcodes are exact and the
    // assembler ignores flags.
    inst!("NEG",  "Negate accumulator", [form(&[0xED, 0x44], "", NONE, Cycles::fixed(8), "SZHPNC")]),
    inst!("RETN", "Return from NMI",     [form(&[0xED, 0x45], "", NONE, Cycles::fixed(14), "")]),
    inst!("RETI", "Return from interrupt", [form(&[0xED, 0x4D], "", NONE, Cycles::fixed(14), "")]),
    inst!("RRD",  "Rotate right decimal", [form(&[0xED, 0x67], "", NONE, Cycles::fixed(18), "SZHPN")]),
    inst!("RLD",  "Rotate left decimal",  [form(&[0xED, 0x6F], "", NONE, Cycles::fixed(18), "SZHPN")]),
    inst!("IM", "Set interrupt mode", [
        form(&[0xED, 0x46], "0", NONE, Cycles::fixed(8), ""),
        form(&[0xED, 0x56], "1", NONE, Cycles::fixed(8), ""),
        form(&[0xED, 0x5E], "2", NONE, Cycles::fixed(8), ""),
    ]),
    // Block transfer / search.
    inst!("LDI",  "Block load, increment",  [form(&[0xED, 0xA0], "", NONE, Cycles::fixed(16), "HPN")]),
    inst!("LDD",  "Block load, decrement",  [form(&[0xED, 0xA8], "", NONE, Cycles::fixed(16), "HPN")]),
    inst!("LDIR", "Block load, inc, repeat", [form(&[0xED, 0xB0], "", NONE, cond(16, 5), "HPN")]),
    inst!("LDDR", "Block load, dec, repeat", [form(&[0xED, 0xB8], "", NONE, cond(16, 5), "HPN")]),
    inst!("CPI",  "Block compare, increment", [form(&[0xED, 0xA1], "", NONE, Cycles::fixed(16), "SZHPN")]),
    inst!("CPD",  "Block compare, decrement", [form(&[0xED, 0xA9], "", NONE, Cycles::fixed(16), "SZHPN")]),
    inst!("CPIR", "Block compare, inc, repeat", [form(&[0xED, 0xB1], "", NONE, cond(16, 5), "SZHPN")]),
    inst!("CPDR", "Block compare, dec, repeat", [form(&[0xED, 0xB9], "", NONE, cond(16, 5), "SZHPN")]),
    // Block I/O (flag detail hardware-fuzzy; opcodes exact).
    inst!("INI",  "Block input, increment",  [form(&[0xED, 0xA2], "", NONE, Cycles::fixed(16), "SZHPNC")]),
    inst!("IND",  "Block input, decrement",  [form(&[0xED, 0xAA], "", NONE, Cycles::fixed(16), "SZHPNC")]),
    inst!("INIR", "Block input, inc, repeat", [form(&[0xED, 0xB2], "", NONE, cond(16, 5), "SZHPNC")]),
    inst!("INDR", "Block input, dec, repeat", [form(&[0xED, 0xBA], "", NONE, cond(16, 5), "SZHPNC")]),
    inst!("OUTI", "Block output, increment", [form(&[0xED, 0xA3], "", NONE, Cycles::fixed(16), "SZHPNC")]),
    inst!("OUTD", "Block output, decrement", [form(&[0xED, 0xAB], "", NONE, Cycles::fixed(16), "SZHPNC")]),
    inst!("OTIR", "Block output, inc, repeat", [form(&[0xED, 0xB3], "", NONE, cond(16, 5), "SZHPNC")]),
    inst!("OTDR", "Block output, dec, repeat", [form(&[0xED, 0xBB], "", NONE, cond(16, 5), "SZHPNC")]),

    // ============================ CB prefix ============================
    // Rotates/shifts (x=0) and bit operations (BIT/RES/SET, x=1..3), each over
    // the eight register slots B C D E H L (HL) A. Register forms cost 8
    // T-states; the (HL) form costs 15 (12 for BIT). Cross-checked against the
    // validated Emu198x decoder. SLL (0x30..0x37) is undocumented.
    inst!("RLC", "Rotate left circular", [
        form(&[0xCB, 0x00], "B", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x01], "C", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x02], "D", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x03], "E", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x04], "H", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x05], "L", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x06], "(HL)", NONE, Cycles::fixed(15), "SZHPNC"),
        form(&[0xCB, 0x07], "A", NONE, Cycles::fixed(8), "SZHPNC"),
    ]),
    inst!("RRC", "Rotate right circular", [
        form(&[0xCB, 0x08], "B", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x09], "C", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x0A], "D", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x0B], "E", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x0C], "H", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x0D], "L", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x0E], "(HL)", NONE, Cycles::fixed(15), "SZHPNC"),
        form(&[0xCB, 0x0F], "A", NONE, Cycles::fixed(8), "SZHPNC"),
    ]),
    inst!("RL", "Rotate left through carry", [
        form(&[0xCB, 0x10], "B", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x11], "C", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x12], "D", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x13], "E", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x14], "H", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x15], "L", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x16], "(HL)", NONE, Cycles::fixed(15), "SZHPNC"),
        form(&[0xCB, 0x17], "A", NONE, Cycles::fixed(8), "SZHPNC"),
    ]),
    inst!("RR", "Rotate right through carry", [
        form(&[0xCB, 0x18], "B", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x19], "C", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x1A], "D", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x1B], "E", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x1C], "H", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x1D], "L", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x1E], "(HL)", NONE, Cycles::fixed(15), "SZHPNC"),
        form(&[0xCB, 0x1F], "A", NONE, Cycles::fixed(8), "SZHPNC"),
    ]),
    inst!("SLA", "Shift left arithmetic", [
        form(&[0xCB, 0x20], "B", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x21], "C", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x22], "D", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x23], "E", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x24], "H", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x25], "L", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x26], "(HL)", NONE, Cycles::fixed(15), "SZHPNC"),
        form(&[0xCB, 0x27], "A", NONE, Cycles::fixed(8), "SZHPNC"),
    ]),
    inst!("SRA", "Shift right arithmetic", [
        form(&[0xCB, 0x28], "B", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x29], "C", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x2A], "D", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x2B], "E", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x2C], "H", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x2D], "L", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x2E], "(HL)", NONE, Cycles::fixed(15), "SZHPNC"),
        form(&[0xCB, 0x2F], "A", NONE, Cycles::fixed(8), "SZHPNC"),
    ]),
    inst!("SLL", "Shift left logical (undocumented)", [
        form_u(&[0xCB, 0x30], "B", NONE, Cycles::fixed(8), "SZHPNC"),
        form_u(&[0xCB, 0x31], "C", NONE, Cycles::fixed(8), "SZHPNC"),
        form_u(&[0xCB, 0x32], "D", NONE, Cycles::fixed(8), "SZHPNC"),
        form_u(&[0xCB, 0x33], "E", NONE, Cycles::fixed(8), "SZHPNC"),
        form_u(&[0xCB, 0x34], "H", NONE, Cycles::fixed(8), "SZHPNC"),
        form_u(&[0xCB, 0x35], "L", NONE, Cycles::fixed(8), "SZHPNC"),
        form_u(&[0xCB, 0x36], "(HL)", NONE, Cycles::fixed(15), "SZHPNC"),
        form_u(&[0xCB, 0x37], "A", NONE, Cycles::fixed(8), "SZHPNC"),
    ]),
    inst!("SRL", "Shift right logical", [
        form(&[0xCB, 0x38], "B", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x39], "C", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x3A], "D", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x3B], "E", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x3C], "H", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x3D], "L", NONE, Cycles::fixed(8), "SZHPNC"),
        form(&[0xCB, 0x3E], "(HL)", NONE, Cycles::fixed(15), "SZHPNC"),
        form(&[0xCB, 0x3F], "A", NONE, Cycles::fixed(8), "SZHPNC"),
    ]),
    inst!("BIT", "Test bit", [
        form(&[0xCB, 0x40], "0,B", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x41], "0,C", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x42], "0,D", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x43], "0,E", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x44], "0,H", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x45], "0,L", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x46], "0,(HL)", NONE, Cycles::fixed(12), "SZHPN"),
        form(&[0xCB, 0x47], "0,A", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x48], "1,B", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x49], "1,C", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x4A], "1,D", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x4B], "1,E", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x4C], "1,H", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x4D], "1,L", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x4E], "1,(HL)", NONE, Cycles::fixed(12), "SZHPN"),
        form(&[0xCB, 0x4F], "1,A", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x50], "2,B", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x51], "2,C", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x52], "2,D", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x53], "2,E", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x54], "2,H", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x55], "2,L", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x56], "2,(HL)", NONE, Cycles::fixed(12), "SZHPN"),
        form(&[0xCB, 0x57], "2,A", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x58], "3,B", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x59], "3,C", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x5A], "3,D", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x5B], "3,E", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x5C], "3,H", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x5D], "3,L", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x5E], "3,(HL)", NONE, Cycles::fixed(12), "SZHPN"),
        form(&[0xCB, 0x5F], "3,A", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x60], "4,B", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x61], "4,C", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x62], "4,D", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x63], "4,E", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x64], "4,H", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x65], "4,L", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x66], "4,(HL)", NONE, Cycles::fixed(12), "SZHPN"),
        form(&[0xCB, 0x67], "4,A", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x68], "5,B", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x69], "5,C", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x6A], "5,D", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x6B], "5,E", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x6C], "5,H", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x6D], "5,L", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x6E], "5,(HL)", NONE, Cycles::fixed(12), "SZHPN"),
        form(&[0xCB, 0x6F], "5,A", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x70], "6,B", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x71], "6,C", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x72], "6,D", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x73], "6,E", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x74], "6,H", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x75], "6,L", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x76], "6,(HL)", NONE, Cycles::fixed(12), "SZHPN"),
        form(&[0xCB, 0x77], "6,A", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x78], "7,B", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x79], "7,C", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x7A], "7,D", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x7B], "7,E", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x7C], "7,H", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x7D], "7,L", NONE, Cycles::fixed(8), "SZHPN"),
        form(&[0xCB, 0x7E], "7,(HL)", NONE, Cycles::fixed(12), "SZHPN"),
        form(&[0xCB, 0x7F], "7,A", NONE, Cycles::fixed(8), "SZHPN"),
    ]),
    inst!("RES", "Reset bit", [
        form(&[0xCB, 0x80], "0,B", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x81], "0,C", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x82], "0,D", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x83], "0,E", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x84], "0,H", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x85], "0,L", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x86], "0,(HL)", NONE, Cycles::fixed(15), ""),
        form(&[0xCB, 0x87], "0,A", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x88], "1,B", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x89], "1,C", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x8A], "1,D", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x8B], "1,E", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x8C], "1,H", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x8D], "1,L", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x8E], "1,(HL)", NONE, Cycles::fixed(15), ""),
        form(&[0xCB, 0x8F], "1,A", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x90], "2,B", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x91], "2,C", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x92], "2,D", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x93], "2,E", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x94], "2,H", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x95], "2,L", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x96], "2,(HL)", NONE, Cycles::fixed(15), ""),
        form(&[0xCB, 0x97], "2,A", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x98], "3,B", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x99], "3,C", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x9A], "3,D", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x9B], "3,E", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x9C], "3,H", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x9D], "3,L", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0x9E], "3,(HL)", NONE, Cycles::fixed(15), ""),
        form(&[0xCB, 0x9F], "3,A", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xA0], "4,B", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xA1], "4,C", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xA2], "4,D", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xA3], "4,E", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xA4], "4,H", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xA5], "4,L", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xA6], "4,(HL)", NONE, Cycles::fixed(15), ""),
        form(&[0xCB, 0xA7], "4,A", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xA8], "5,B", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xA9], "5,C", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xAA], "5,D", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xAB], "5,E", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xAC], "5,H", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xAD], "5,L", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xAE], "5,(HL)", NONE, Cycles::fixed(15), ""),
        form(&[0xCB, 0xAF], "5,A", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xB0], "6,B", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xB1], "6,C", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xB2], "6,D", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xB3], "6,E", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xB4], "6,H", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xB5], "6,L", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xB6], "6,(HL)", NONE, Cycles::fixed(15), ""),
        form(&[0xCB, 0xB7], "6,A", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xB8], "7,B", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xB9], "7,C", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xBA], "7,D", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xBB], "7,E", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xBC], "7,H", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xBD], "7,L", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xBE], "7,(HL)", NONE, Cycles::fixed(15), ""),
        form(&[0xCB, 0xBF], "7,A", NONE, Cycles::fixed(8), ""),
    ]),
    inst!("SET", "Set bit", [
        form(&[0xCB, 0xC0], "0,B", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xC1], "0,C", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xC2], "0,D", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xC3], "0,E", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xC4], "0,H", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xC5], "0,L", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xC6], "0,(HL)", NONE, Cycles::fixed(15), ""),
        form(&[0xCB, 0xC7], "0,A", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xC8], "1,B", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xC9], "1,C", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xCA], "1,D", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xCB], "1,E", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xCC], "1,H", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xCD], "1,L", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xCE], "1,(HL)", NONE, Cycles::fixed(15), ""),
        form(&[0xCB, 0xCF], "1,A", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xD0], "2,B", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xD1], "2,C", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xD2], "2,D", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xD3], "2,E", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xD4], "2,H", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xD5], "2,L", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xD6], "2,(HL)", NONE, Cycles::fixed(15), ""),
        form(&[0xCB, 0xD7], "2,A", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xD8], "3,B", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xD9], "3,C", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xDA], "3,D", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xDB], "3,E", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xDC], "3,H", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xDD], "3,L", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xDE], "3,(HL)", NONE, Cycles::fixed(15), ""),
        form(&[0xCB, 0xDF], "3,A", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xE0], "4,B", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xE1], "4,C", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xE2], "4,D", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xE3], "4,E", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xE4], "4,H", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xE5], "4,L", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xE6], "4,(HL)", NONE, Cycles::fixed(15), ""),
        form(&[0xCB, 0xE7], "4,A", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xE8], "5,B", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xE9], "5,C", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xEA], "5,D", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xEB], "5,E", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xEC], "5,H", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xED], "5,L", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xEE], "5,(HL)", NONE, Cycles::fixed(15), ""),
        form(&[0xCB, 0xEF], "5,A", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xF0], "6,B", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xF1], "6,C", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xF2], "6,D", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xF3], "6,E", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xF4], "6,H", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xF5], "6,L", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xF6], "6,(HL)", NONE, Cycles::fixed(15), ""),
        form(&[0xCB, 0xF7], "6,A", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xF8], "7,B", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xF9], "7,C", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xFA], "7,D", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xFB], "7,E", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xFC], "7,H", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xFD], "7,L", NONE, Cycles::fixed(8), ""),
        form(&[0xCB, 0xFE], "7,(HL)", NONE, Cycles::fixed(15), ""),
        form(&[0xCB, 0xFF], "7,A", NONE, Cycles::fixed(8), ""),
    ]),

    // ===================== DD prefix (IX) / FD prefix (IY) =====================
    // The index-register group. A mnemonic's IX/IY forms live on their own
    // entries (find_form scans all entries with a mnemonic), keeping this group
    // readable. The undocumented IXH/IXL/IYH/IYL half-register ops are omitted,
    // as is the undocumented SLL (IX+d)/(IY+d). DD CB / FD CB forms carry the
    // displacement before a trailing opcode byte (see form_ddcb).

    // --- IX (DD) ---
    inst!("ADD", "Add to IX", [
        form(&[0xDD, 0x09], "IX,BC",    NONE,     Cycles::fixed(15), "HNC"),
        form(&[0xDD, 0x19], "IX,DE",    NONE,     Cycles::fixed(15), "HNC"),
        form(&[0xDD, 0x29], "IX,IX",    NONE,     Cycles::fixed(15), "HNC"),
        form(&[0xDD, 0x39], "IX,SP",    NONE,     Cycles::fixed(15), "HNC"),
        form(&[0xDD, 0x86], "A,(IX+d)", ONE_DISP, Cycles::fixed(19), "SZHPNC"),
    ]),
    inst!("ADC", "Add with carry, IX", [form(&[0xDD, 0x8E], "A,(IX+d)", ONE_DISP, Cycles::fixed(19), "SZHPNC")]),
    inst!("SUB", "Subtract, IX",       [form(&[0xDD, 0x96], "(IX+d)",   ONE_DISP, Cycles::fixed(19), "SZHPNC")]),
    inst!("SBC", "Subtract carry, IX", [form(&[0xDD, 0x9E], "A,(IX+d)", ONE_DISP, Cycles::fixed(19), "SZHPNC")]),
    inst!("AND", "AND, IX",            [form(&[0xDD, 0xA6], "(IX+d)",   ONE_DISP, Cycles::fixed(19), "SZHPNC")]),
    inst!("XOR", "XOR, IX",            [form(&[0xDD, 0xAE], "(IX+d)",   ONE_DISP, Cycles::fixed(19), "SZHPNC")]),
    inst!("OR",  "OR, IX",             [form(&[0xDD, 0xB6], "(IX+d)",   ONE_DISP, Cycles::fixed(19), "SZHPNC")]),
    inst!("CP",  "Compare, IX",        [form(&[0xDD, 0xBE], "(IX+d)",   ONE_DISP, Cycles::fixed(19), "SZHPNC")]),
    inst!("INC", "Increment IX", [
        form(&[0xDD, 0x23], "IX",     NONE,     Cycles::fixed(10), ""),
        form(&[0xDD, 0x34], "(IX+d)", ONE_DISP, Cycles::fixed(23), "SZHPN"),
    ]),
    inst!("DEC", "Decrement IX", [
        form(&[0xDD, 0x2B], "IX",     NONE,     Cycles::fixed(10), ""),
        form(&[0xDD, 0x35], "(IX+d)", ONE_DISP, Cycles::fixed(23), "SZHPN"),
    ]),
    inst!("PUSH", "Push IX", [form(&[0xDD, 0xE5], "IX", NONE, Cycles::fixed(15), "")]),
    inst!("POP",  "Pop IX",  [form(&[0xDD, 0xE1], "IX", NONE, Cycles::fixed(14), "")]),
    inst!("EX",   "Exchange IX", [form(&[0xDD, 0xE3], "(SP),IX", NONE, Cycles::fixed(23), "")]),
    inst!("JP",   "Jump IX", [form(&[0xDD, 0xE9], "(IX)", NONE, Cycles::fixed(8), "")]),
    inst!("LD", "Load IX", [
        form(&[0xDD, 0x21], "IX,nn",    ONE_NN,      Cycles::fixed(14), ""),
        form(&[0xDD, 0x22], "(nn),IX",  ONE_ADDR,    Cycles::fixed(20), ""),
        form(&[0xDD, 0x2A], "IX,(nn)",  ONE_ADDR,    Cycles::fixed(20), ""),
        form(&[0xDD, 0xF9], "SP,IX",    NONE,        Cycles::fixed(10), ""),
        form(&[0xDD, 0x36], "(IX+d),n", DISP_THEN_N, Cycles::fixed(19), ""),
        form(&[0xDD, 0x46], "B,(IX+d)", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xDD, 0x4E], "C,(IX+d)", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xDD, 0x56], "D,(IX+d)", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xDD, 0x5E], "E,(IX+d)", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xDD, 0x66], "H,(IX+d)", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xDD, 0x6E], "L,(IX+d)", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xDD, 0x7E], "A,(IX+d)", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xDD, 0x70], "(IX+d),B", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xDD, 0x71], "(IX+d),C", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xDD, 0x72], "(IX+d),D", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xDD, 0x73], "(IX+d),E", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xDD, 0x74], "(IX+d),H", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xDD, 0x75], "(IX+d),L", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xDD, 0x77], "(IX+d),A", ONE_DISP,    Cycles::fixed(19), ""),
    ]),
    inst!("RLC", "Rotate left circular (IX+d)",  [form_ddcb(&[0xDD, 0xCB], &[0x06], "(IX+d)", Cycles::fixed(23), "SZHPNC")]),
    inst!("RRC", "Rotate right circular (IX+d)", [form_ddcb(&[0xDD, 0xCB], &[0x0E], "(IX+d)", Cycles::fixed(23), "SZHPNC")]),
    inst!("RL",  "Rotate left (IX+d)",           [form_ddcb(&[0xDD, 0xCB], &[0x16], "(IX+d)", Cycles::fixed(23), "SZHPNC")]),
    inst!("RR",  "Rotate right (IX+d)",          [form_ddcb(&[0xDD, 0xCB], &[0x1E], "(IX+d)", Cycles::fixed(23), "SZHPNC")]),
    inst!("SLA", "Shift left arithmetic (IX+d)", [form_ddcb(&[0xDD, 0xCB], &[0x26], "(IX+d)", Cycles::fixed(23), "SZHPNC")]),
    inst!("SRA", "Shift right arithmetic (IX+d)",[form_ddcb(&[0xDD, 0xCB], &[0x2E], "(IX+d)", Cycles::fixed(23), "SZHPNC")]),
    inst!("SRL", "Shift right logical (IX+d)",   [form_ddcb(&[0xDD, 0xCB], &[0x3E], "(IX+d)", Cycles::fixed(23), "SZHPNC")]),
    inst!("BIT", "Test bit (IX+d)", [
        form_ddcb(&[0xDD, 0xCB], &[0x46], "0,(IX+d)", Cycles::fixed(20), "SZHPN"),
        form_ddcb(&[0xDD, 0xCB], &[0x4E], "1,(IX+d)", Cycles::fixed(20), "SZHPN"),
        form_ddcb(&[0xDD, 0xCB], &[0x56], "2,(IX+d)", Cycles::fixed(20), "SZHPN"),
        form_ddcb(&[0xDD, 0xCB], &[0x5E], "3,(IX+d)", Cycles::fixed(20), "SZHPN"),
        form_ddcb(&[0xDD, 0xCB], &[0x66], "4,(IX+d)", Cycles::fixed(20), "SZHPN"),
        form_ddcb(&[0xDD, 0xCB], &[0x6E], "5,(IX+d)", Cycles::fixed(20), "SZHPN"),
        form_ddcb(&[0xDD, 0xCB], &[0x76], "6,(IX+d)", Cycles::fixed(20), "SZHPN"),
        form_ddcb(&[0xDD, 0xCB], &[0x7E], "7,(IX+d)", Cycles::fixed(20), "SZHPN"),
    ]),
    inst!("RES", "Reset bit (IX+d)", [
        form_ddcb(&[0xDD, 0xCB], &[0x86], "0,(IX+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xDD, 0xCB], &[0x8E], "1,(IX+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xDD, 0xCB], &[0x96], "2,(IX+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xDD, 0xCB], &[0x9E], "3,(IX+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xDD, 0xCB], &[0xA6], "4,(IX+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xDD, 0xCB], &[0xAE], "5,(IX+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xDD, 0xCB], &[0xB6], "6,(IX+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xDD, 0xCB], &[0xBE], "7,(IX+d)", Cycles::fixed(23), ""),
    ]),
    inst!("SET", "Set bit (IX+d)", [
        form_ddcb(&[0xDD, 0xCB], &[0xC6], "0,(IX+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xDD, 0xCB], &[0xCE], "1,(IX+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xDD, 0xCB], &[0xD6], "2,(IX+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xDD, 0xCB], &[0xDE], "3,(IX+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xDD, 0xCB], &[0xE6], "4,(IX+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xDD, 0xCB], &[0xEE], "5,(IX+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xDD, 0xCB], &[0xF6], "6,(IX+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xDD, 0xCB], &[0xFE], "7,(IX+d)", Cycles::fixed(23), ""),
    ]),

    // --- IY (FD): identical to IX with the FD prefix and IY labels ---
    inst!("ADD", "Add to IY", [
        form(&[0xFD, 0x09], "IY,BC",    NONE,     Cycles::fixed(15), "HNC"),
        form(&[0xFD, 0x19], "IY,DE",    NONE,     Cycles::fixed(15), "HNC"),
        form(&[0xFD, 0x29], "IY,IY",    NONE,     Cycles::fixed(15), "HNC"),
        form(&[0xFD, 0x39], "IY,SP",    NONE,     Cycles::fixed(15), "HNC"),
        form(&[0xFD, 0x86], "A,(IY+d)", ONE_DISP, Cycles::fixed(19), "SZHPNC"),
    ]),
    inst!("ADC", "Add with carry, IY", [form(&[0xFD, 0x8E], "A,(IY+d)", ONE_DISP, Cycles::fixed(19), "SZHPNC")]),
    inst!("SUB", "Subtract, IY",       [form(&[0xFD, 0x96], "(IY+d)",   ONE_DISP, Cycles::fixed(19), "SZHPNC")]),
    inst!("SBC", "Subtract carry, IY", [form(&[0xFD, 0x9E], "A,(IY+d)", ONE_DISP, Cycles::fixed(19), "SZHPNC")]),
    inst!("AND", "AND, IY",            [form(&[0xFD, 0xA6], "(IY+d)",   ONE_DISP, Cycles::fixed(19), "SZHPNC")]),
    inst!("XOR", "XOR, IY",            [form(&[0xFD, 0xAE], "(IY+d)",   ONE_DISP, Cycles::fixed(19), "SZHPNC")]),
    inst!("OR",  "OR, IY",             [form(&[0xFD, 0xB6], "(IY+d)",   ONE_DISP, Cycles::fixed(19), "SZHPNC")]),
    inst!("CP",  "Compare, IY",        [form(&[0xFD, 0xBE], "(IY+d)",   ONE_DISP, Cycles::fixed(19), "SZHPNC")]),
    inst!("INC", "Increment IY", [
        form(&[0xFD, 0x23], "IY",     NONE,     Cycles::fixed(10), ""),
        form(&[0xFD, 0x34], "(IY+d)", ONE_DISP, Cycles::fixed(23), "SZHPN"),
    ]),
    inst!("DEC", "Decrement IY", [
        form(&[0xFD, 0x2B], "IY",     NONE,     Cycles::fixed(10), ""),
        form(&[0xFD, 0x35], "(IY+d)", ONE_DISP, Cycles::fixed(23), "SZHPN"),
    ]),
    inst!("PUSH", "Push IY", [form(&[0xFD, 0xE5], "IY", NONE, Cycles::fixed(15), "")]),
    inst!("POP",  "Pop IY",  [form(&[0xFD, 0xE1], "IY", NONE, Cycles::fixed(14), "")]),
    inst!("EX",   "Exchange IY", [form(&[0xFD, 0xE3], "(SP),IY", NONE, Cycles::fixed(23), "")]),
    inst!("JP",   "Jump IY", [form(&[0xFD, 0xE9], "(IY)", NONE, Cycles::fixed(8), "")]),
    inst!("LD", "Load IY", [
        form(&[0xFD, 0x21], "IY,nn",    ONE_NN,      Cycles::fixed(14), ""),
        form(&[0xFD, 0x22], "(nn),IY",  ONE_ADDR,    Cycles::fixed(20), ""),
        form(&[0xFD, 0x2A], "IY,(nn)",  ONE_ADDR,    Cycles::fixed(20), ""),
        form(&[0xFD, 0xF9], "SP,IY",    NONE,        Cycles::fixed(10), ""),
        form(&[0xFD, 0x36], "(IY+d),n", DISP_THEN_N, Cycles::fixed(19), ""),
        form(&[0xFD, 0x46], "B,(IY+d)", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xFD, 0x4E], "C,(IY+d)", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xFD, 0x56], "D,(IY+d)", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xFD, 0x5E], "E,(IY+d)", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xFD, 0x66], "H,(IY+d)", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xFD, 0x6E], "L,(IY+d)", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xFD, 0x7E], "A,(IY+d)", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xFD, 0x70], "(IY+d),B", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xFD, 0x71], "(IY+d),C", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xFD, 0x72], "(IY+d),D", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xFD, 0x73], "(IY+d),E", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xFD, 0x74], "(IY+d),H", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xFD, 0x75], "(IY+d),L", ONE_DISP,    Cycles::fixed(19), ""),
        form(&[0xFD, 0x77], "(IY+d),A", ONE_DISP,    Cycles::fixed(19), ""),
    ]),
    inst!("RLC", "Rotate left circular (IY+d)",  [form_ddcb(&[0xFD, 0xCB], &[0x06], "(IY+d)", Cycles::fixed(23), "SZHPNC")]),
    inst!("RRC", "Rotate right circular (IY+d)", [form_ddcb(&[0xFD, 0xCB], &[0x0E], "(IY+d)", Cycles::fixed(23), "SZHPNC")]),
    inst!("RL",  "Rotate left (IY+d)",           [form_ddcb(&[0xFD, 0xCB], &[0x16], "(IY+d)", Cycles::fixed(23), "SZHPNC")]),
    inst!("RR",  "Rotate right (IY+d)",          [form_ddcb(&[0xFD, 0xCB], &[0x1E], "(IY+d)", Cycles::fixed(23), "SZHPNC")]),
    inst!("SLA", "Shift left arithmetic (IY+d)", [form_ddcb(&[0xFD, 0xCB], &[0x26], "(IY+d)", Cycles::fixed(23), "SZHPNC")]),
    inst!("SRA", "Shift right arithmetic (IY+d)",[form_ddcb(&[0xFD, 0xCB], &[0x2E], "(IY+d)", Cycles::fixed(23), "SZHPNC")]),
    inst!("SRL", "Shift right logical (IY+d)",   [form_ddcb(&[0xFD, 0xCB], &[0x3E], "(IY+d)", Cycles::fixed(23), "SZHPNC")]),
    inst!("BIT", "Test bit (IY+d)", [
        form_ddcb(&[0xFD, 0xCB], &[0x46], "0,(IY+d)", Cycles::fixed(20), "SZHPN"),
        form_ddcb(&[0xFD, 0xCB], &[0x4E], "1,(IY+d)", Cycles::fixed(20), "SZHPN"),
        form_ddcb(&[0xFD, 0xCB], &[0x56], "2,(IY+d)", Cycles::fixed(20), "SZHPN"),
        form_ddcb(&[0xFD, 0xCB], &[0x5E], "3,(IY+d)", Cycles::fixed(20), "SZHPN"),
        form_ddcb(&[0xFD, 0xCB], &[0x66], "4,(IY+d)", Cycles::fixed(20), "SZHPN"),
        form_ddcb(&[0xFD, 0xCB], &[0x6E], "5,(IY+d)", Cycles::fixed(20), "SZHPN"),
        form_ddcb(&[0xFD, 0xCB], &[0x76], "6,(IY+d)", Cycles::fixed(20), "SZHPN"),
        form_ddcb(&[0xFD, 0xCB], &[0x7E], "7,(IY+d)", Cycles::fixed(20), "SZHPN"),
    ]),
    inst!("RES", "Reset bit (IY+d)", [
        form_ddcb(&[0xFD, 0xCB], &[0x86], "0,(IY+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xFD, 0xCB], &[0x8E], "1,(IY+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xFD, 0xCB], &[0x96], "2,(IY+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xFD, 0xCB], &[0x9E], "3,(IY+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xFD, 0xCB], &[0xA6], "4,(IY+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xFD, 0xCB], &[0xAE], "5,(IY+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xFD, 0xCB], &[0xB6], "6,(IY+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xFD, 0xCB], &[0xBE], "7,(IY+d)", Cycles::fixed(23), ""),
    ]),
    inst!("SET", "Set bit (IY+d)", [
        form_ddcb(&[0xFD, 0xCB], &[0xC6], "0,(IY+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xFD, 0xCB], &[0xCE], "1,(IY+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xFD, 0xCB], &[0xD6], "2,(IY+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xFD, 0xCB], &[0xDE], "3,(IY+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xFD, 0xCB], &[0xE6], "4,(IY+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xFD, 0xCB], &[0xEE], "5,(IY+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xFD, 0xCB], &[0xF6], "6,(IY+d)", Cycles::fixed(23), ""),
        form_ddcb(&[0xFD, 0xCB], &[0xFE], "7,(IY+d)", Cycles::fixed(23), ""),
    ]),
];

// ============================ Z80N (Spectrum Next) ============================

const TWO_N: &[Operand] = &[IMM8, IMM8]; // NEXTREG reg,val

/// The ZX Spectrum Next's **Z80N** extended opcodes — all ED-prefixed. These
/// are an extension set, available when targeting the Next; standard-Z80
/// assembly does not see them. One surprise the reference tools and the Next
/// documentation agree on: `PUSH nn`'s immediate is stored **big-endian** (high
/// byte first) — the only big-endian operand in the whole Z80/Z80N set — so it
/// uses [`IMM16_BE`], not the usual little-endian `IMM16`.
pub const NEXT: InstructionSet = InstructionSet {
    cpu: "Zilog Z80N",
    endianness: Endianness::Little,
    instructions: NEXT_INSTRUCTIONS,
};

#[rustfmt::skip]
const NEXT_INSTRUCTIONS: &[Instruction] = &[
    inst!("SWAPNIB", "Swap the nibbles of A",          [form(&[0xED, 0x23], "",    NONE,  Cycles::fixed(8),  "")]),
    inst!("MIRROR",  "Mirror the bits of A",           [form(&[0xED, 0x24], "A",   NONE,  Cycles::fixed(8),  "")]),
    inst!("TEST",    "AND A with n, set flags only",   [form(&[0xED, 0x27], "n",   ONE_N, Cycles::fixed(11), "SZHPNC")]),
    inst!("BSLA",    "Barrel shift left arithmetic",   [form(&[0xED, 0x28], "DE,B", NONE, Cycles::fixed(8),  "")]),
    inst!("BSRA",    "Barrel shift right arithmetic",  [form(&[0xED, 0x29], "DE,B", NONE, Cycles::fixed(8),  "")]),
    inst!("BSRL",    "Barrel shift right logical",     [form(&[0xED, 0x2A], "DE,B", NONE, Cycles::fixed(8),  "")]),
    inst!("BSRF",    "Barrel shift right feed",        [form(&[0xED, 0x2B], "DE,B", NONE, Cycles::fixed(8),  "")]),
    inst!("BRLC",    "Barrel rotate left circular",    [form(&[0xED, 0x2C], "DE,B", NONE, Cycles::fixed(8),  "")]),
    inst!("MUL",     "8x8 multiply D*E into DE",       [form(&[0xED, 0x30], "",     NONE, Cycles::fixed(8),  "")]),
    inst!("ADD", "Add (Z80N)", [
        form(&[0xED, 0x31], "HL,A",  NONE,   Cycles::fixed(8),  ""),
        form(&[0xED, 0x32], "DE,A",  NONE,   Cycles::fixed(8),  ""),
        form(&[0xED, 0x33], "BC,A",  NONE,   Cycles::fixed(8),  ""),
        form(&[0xED, 0x34], "HL,nn", ONE_NN, Cycles::fixed(16), ""),
        form(&[0xED, 0x35], "DE,nn", ONE_NN, Cycles::fixed(16), ""),
        form(&[0xED, 0x36], "BC,nn", ONE_NN, Cycles::fixed(16), ""),
    ]),
    inst!("PUSH", "Push a 16-bit immediate", [form(&[0xED, 0x8A], "nn", ONE_NN_BE, Cycles::fixed(23), "")]),
    inst!("OUTINB", "Output (HL) to port (C), inc HL",   [form(&[0xED, 0x90], "", NONE, Cycles::fixed(16), "")]),
    inst!("NEXTREG", "Set a Next hardware register", [
        form(&[0xED, 0x91], "n,n", TWO_N, Cycles::fixed(20), ""),
        form(&[0xED, 0x92], "n,A", ONE_N, Cycles::fixed(17), ""),
    ]),
    inst!("PIXELDN", "Advance HL to the next pixel row", [form(&[0xED, 0x93], "", NONE, Cycles::fixed(8), "")]),
    inst!("PIXELAD", "Compute pixel address into HL",    [form(&[0xED, 0x94], "", NONE, Cycles::fixed(8), "")]),
    inst!("SETAE",   "Set A to the E pixel mask",        [form(&[0xED, 0x95], "", NONE, Cycles::fixed(8), "")]),
    inst!("LDIX",    "Block load (transparent), inc",    [form(&[0xED, 0xA4], "", NONE, Cycles::fixed(16), "")]),
    inst!("LDWS",    "Load word, special",               [form(&[0xED, 0xA5], "", NONE, Cycles::fixed(14), "")]),
    inst!("LDDX",    "Block load (transparent), dec",    [form(&[0xED, 0xAC], "", NONE, Cycles::fixed(16), "")]),
    inst!("LDIRX",   "Repeat LDIX",                      [form(&[0xED, 0xB4], "", NONE, cond(16, 5), "")]),
    inst!("LDPIRX",  "Repeat LDIX over a pattern",       [form(&[0xED, 0xB7], "", NONE, cond(16, 5), "")]),
    inst!("LDDRX",   "Repeat LDDX",                      [form(&[0xED, 0xBC], "", NONE, cond(16, 5), "")]),
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

    /// A mnemonic's forms may span entries (base `LD` and the IX/IY `LD`), but
    /// no `(mnemonic, mode)` pair may repeat — otherwise `find_form` would be
    /// ambiguous.
    #[test]
    fn no_duplicate_mnemonic_and_mode() {
        let mut seen = std::collections::BTreeSet::new();
        for instruction in SET.instructions {
            for f in instruction.forms {
                assert!(
                    seen.insert((instruction.mnemonic, f.mode)),
                    "duplicate (mnemonic, mode): `{}` `{}`",
                    instruction.mnemonic,
                    f.mode
                );
            }
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
            SET.instruction("HALT")
                .expect("HALT")
                .form("")
                .expect("form")
                .opcode,
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
        assert_eq!(
            SET.instruction("JP")
                .expect("JP")
                .form("nn")
                .expect("JP nn")
                .opcode,
            &[0xC3]
        );
        assert_eq!(
            SET.instruction("CALL")
                .expect("CALL")
                .form("nn")
                .expect("CALL nn")
                .opcode,
            &[0xCD]
        );
        assert_eq!(
            SET.instruction("RET")
                .expect("RET")
                .form("")
                .expect("RET")
                .opcode,
            &[0xC9]
        );
        assert_eq!(
            SET.instruction("CP")
                .expect("CP")
                .form("(HL)")
                .expect("CP (HL)")
                .opcode,
            &[0xBE]
        );
        assert_eq!(
            SET.instruction("PUSH")
                .expect("PUSH")
                .form("AF")
                .expect("PUSH AF")
                .opcode,
            &[0xF5]
        );
        assert_eq!(
            SET.instruction("RST")
                .expect("RST")
                .form("38")
                .expect("RST 38")
                .opcode,
            &[0xFF]
        );
        assert_eq!(
            SET.instruction("LD")
                .expect("LD")
                .form("SP,HL")
                .expect("LD SP,HL")
                .opcode,
            &[0xF9]
        );

        // CALL cc pays 7 extra T-states when taken; JP cc is a flat 10.
        let call_nz = SET
            .instruction("CALL")
            .expect("CALL")
            .form("NZ,nn")
            .expect("CALL NZ,nn");
        assert_eq!((call_nz.cycles.base, call_nz.cycles.branch_taken), (10, 7));
    }

    /// ED-prefixed forms: two-byte opcodes, unique in their second byte, with
    /// encodings cross-checked against the validated decoder.
    #[test]
    fn ed_group_encodings_and_uniqueness() {
        assert_eq!(
            SET.instruction("LDIR")
                .expect("LDIR")
                .form("")
                .expect("form")
                .opcode,
            &[0xED, 0xB0]
        );
        assert_eq!(
            SET.instruction("NEG")
                .expect("NEG")
                .form("")
                .expect("form")
                .opcode,
            &[0xED, 0x44]
        );
        assert_eq!(
            SET.instruction("IM")
                .expect("IM")
                .form("1")
                .expect("IM 1")
                .opcode,
            &[0xED, 0x56]
        );
        assert_eq!(
            SET.instruction("SBC")
                .expect("SBC")
                .form("HL,DE")
                .expect("SBC HL,DE")
                .opcode,
            &[0xED, 0x52]
        );

        let mut seen = std::collections::BTreeSet::new();
        for instruction in SET.instructions {
            for f in instruction.forms {
                if f.opcode.first() == Some(&0xED) {
                    assert_eq!(
                        f.opcode.len(),
                        2,
                        "{} {} malformed ED opcode",
                        instruction.mnemonic,
                        f.mode
                    );
                    assert!(
                        seen.insert(f.opcode[1]),
                        "duplicate ED opcode $ED ${:02X} ({} {})",
                        f.opcode[1],
                        instruction.mnemonic,
                        f.mode
                    );
                }
            }
        }
    }

    /// The CB group is the full 256-entry second-byte space, present exactly
    /// once — the completeness + uniqueness gate for the bit/rotate page.
    #[test]
    fn cb_group_is_complete_and_unique() {
        let mut seen = [false; 256];
        for instruction in SET.instructions {
            for f in instruction.forms {
                if f.opcode.first() == Some(&0xCB) {
                    assert_eq!(
                        f.opcode.len(),
                        2,
                        "{} {} malformed CB opcode",
                        instruction.mnemonic,
                        f.mode
                    );
                    let op = f.opcode[1] as usize;
                    assert!(
                        !seen[op],
                        "duplicate CB opcode $CB ${:02X} ({} {})",
                        op, instruction.mnemonic, f.mode
                    );
                    seen[op] = true;
                }
            }
        }
        for (op, present) in seen.iter().enumerate() {
            assert!(present, "missing CB opcode $CB ${op:02X}");
        }

        // Spot-checks against the validated decoder grid.
        assert_eq!(
            SET.instruction("BIT")
                .expect("BIT")
                .form("7,(HL)")
                .expect("f")
                .opcode,
            &[0xCB, 0x7E]
        );
        assert_eq!(
            SET.instruction("SET")
                .expect("SET")
                .form("0,A")
                .expect("f")
                .opcode,
            &[0xCB, 0xC7]
        );
        assert_eq!(
            SET.instruction("RLC")
                .expect("RLC")
                .form("B")
                .expect("f")
                .opcode,
            &[0xCB, 0x00]
        );
    }

    /// DD/FD (IX/IY) forms, including the two-operand and DD-CB encodings.
    #[test]
    fn index_register_encodings() {
        // 16-bit and indexed forms (mnemonic forms span entries -> find_form).
        assert_eq!(
            SET.find_form("LD", "IX,nn").expect("ld ix,nn").opcode,
            &[0xDD, 0x21]
        );
        assert_eq!(
            SET.find_form("ADD", "IY,SP").expect("add iy,sp").opcode,
            &[0xFD, 0x39]
        );
        assert_eq!(
            SET.find_form("JP", "(IX)").expect("jp (ix)").opcode,
            &[0xDD, 0xE9]
        );

        // LD (IX+d),n: opcode DD 36, then a displacement and an immediate byte.
        let ldn = SET.find_form("LD", "(IX+d),n").expect("ld (ix+d),n");
        assert_eq!(ldn.opcode, &[0xDD, 0x36]);
        assert_eq!(ldn.operands.len(), 2);
        assert_eq!(ldn.len(), 4); // DD 36 d n

        // DD CB: prefix, displacement operand, trailing opcode byte.
        let bit = SET.find_form("BIT", "7,(IY+d)").expect("bit 7,(iy+d)");
        assert_eq!(bit.opcode, &[0xFD, 0xCB]);
        assert_eq!(bit.suffix, &[0x7E]);
        assert_eq!(bit.len(), 4); // FD CB d 7E
    }
}
