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
//! This module is authored in slices. **Landed: the unprefixed base page
//! `0x00..=0x7F`** — the load group, 16-bit arithmetic, INC/DEC, relative
//! jumps, and the accumulator/flag ops. **TODO:** `0x80..=0xFF` (8-bit ALU,
//! stack, control flow, I/O, `RST`); then the `CB`, `ED`, and `DD`/`FD`
//! (IX/IY) prefix groups.

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
    ]),

    inst!("EX", "Exchange", [
        form(&[0x08], "AF,AF'", NONE, Cycles::fixed(4), ""),
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
        // 16-bit add to HL. (8-bit ADD A,s lands with the 0x80..=0xFF slice.)
        form(&[0x09], "HL,BC", NONE, Cycles::fixed(11), "HNC"),
        form(&[0x19], "HL,DE", NONE, Cycles::fixed(11), "HNC"),
        form(&[0x29], "HL,HL", NONE, Cycles::fixed(11), "HNC"),
        form(&[0x39], "HL,SP", NONE, Cycles::fixed(11), "HNC"),
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
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Every opcode in `0x00..=0x7F` is one base-page instruction, present
    /// exactly once. This is the completeness + uniqueness gate for the slice:
    /// a typo or a missing entry fails here. (All forms in this range are
    /// single-byte; prefixed and `>= 0x80` forms are excluded so the test keeps
    /// working as later slices land.)
    #[test]
    fn base_page_low_half_is_complete_and_unique() {
        let mut seen = [false; 0x80];
        for instruction in SET.instructions {
            for f in instruction.forms {
                if f.opcode.len() == 1 && f.opcode[0] < 0x80 {
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
            assert!(present, "missing opcode ${op:02X} in 0x00..=0x7F");
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
    }
}
