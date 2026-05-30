//! MOS Technology 6502 instruction set.
//!
//! The documented (legal) opcode set, authored from the 6502 datasheet and
//! cross-checked against the family's primary reference library. Undocumented
//! opcodes are out of scope for this first pass (`undocumented: true` is
//! reserved for when they land).
//!
//! Flag strings name the status bits each form affects, drawn from `NV-BDIZC`:
//! e.g. `"NZ"`, `"NZC"`, `"NZCV"`. `"NVDIZC"` means "all flags restored from
//! the stack" (`PLP`, `RTI`). The assembler ignores flags; they serve the
//! disassembler, documentation, and Emu198x conformance checks.

use crate::{Cycles, Endianness, Form, Instruction, InstructionSet, Operand, OperandKind};

const IMM8: Operand = Operand {
    kind: OperandKind::Immediate,
    bytes: 1,
};
const ADDR8: Operand = Operand {
    kind: OperandKind::Address,
    bytes: 1,
}; // zero page
const ADDR16: Operand = Operand {
    kind: OperandKind::Address,
    bytes: 2,
}; // absolute
const REL8: Operand = Operand {
    kind: OperandKind::RelativePc,
    bytes: 1,
};

const NONE: &[Operand] = &[];
const ONE_IMM: &[Operand] = &[IMM8];
const ONE_ZP: &[Operand] = &[ADDR8];
const ONE_ABS: &[Operand] = &[ADDR16];
const ONE_REL: &[Operand] = &[REL8];

/// The 6502 instruction set: the single source of truth for 6502 encoding.
pub const SET: InstructionSet = InstructionSet {
    cpu: "MOS 6502",
    endianness: Endianness::Little,
    instructions: INSTRUCTIONS,
};

/// Build one form. A `&[u8]` literal is already a `'static` slice in const
/// context, so opcode bytes are written inline at each call site.
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

macro_rules! inst {
    ($mnemonic:literal, $summary:literal, [ $($form:expr),* $(,)? ]) => {
        Instruction { mnemonic: $mnemonic, summary: $summary, forms: &[ $($form),* ] }
    };
}

#[rustfmt::skip]
const INSTRUCTIONS: &[Instruction] = &[
    inst!("LDA", "Load accumulator", [
        form(&[0xA9], "immediate",     ONE_IMM, Cycles::fixed(2),         "NZ"),
        form(&[0xA5], "zeropage",      ONE_ZP,  Cycles::fixed(3),         "NZ"),
        form(&[0xB5], "zeropage,x",    ONE_ZP,  Cycles::fixed(4),         "NZ"),
        form(&[0xAD], "absolute",      ONE_ABS, Cycles::fixed(4),         "NZ"),
        form(&[0xBD], "absolute,x",    ONE_ABS, Cycles::page_crossing(4), "NZ"),
        form(&[0xB9], "absolute,y",    ONE_ABS, Cycles::page_crossing(4), "NZ"),
        form(&[0xA1], "(indirect,x)",  ONE_ZP,  Cycles::fixed(6),         "NZ"),
        form(&[0xB1], "(indirect),y",  ONE_ZP,  Cycles::page_crossing(5), "NZ"),
    ]),
    inst!("LDX", "Load X register", [
        form(&[0xA2], "immediate",  ONE_IMM, Cycles::fixed(2),         "NZ"),
        form(&[0xA6], "zeropage",   ONE_ZP,  Cycles::fixed(3),         "NZ"),
        form(&[0xB6], "zeropage,y", ONE_ZP,  Cycles::fixed(4),         "NZ"),
        form(&[0xAE], "absolute",   ONE_ABS, Cycles::fixed(4),         "NZ"),
        form(&[0xBE], "absolute,y", ONE_ABS, Cycles::page_crossing(4), "NZ"),
    ]),
    inst!("LDY", "Load Y register", [
        form(&[0xA0], "immediate",  ONE_IMM, Cycles::fixed(2),         "NZ"),
        form(&[0xA4], "zeropage",   ONE_ZP,  Cycles::fixed(3),         "NZ"),
        form(&[0xB4], "zeropage,x", ONE_ZP,  Cycles::fixed(4),         "NZ"),
        form(&[0xAC], "absolute",   ONE_ABS, Cycles::fixed(4),         "NZ"),
        form(&[0xBC], "absolute,x", ONE_ABS, Cycles::page_crossing(4), "NZ"),
    ]),
    inst!("STA", "Store accumulator", [
        form(&[0x85], "zeropage",     ONE_ZP,  Cycles::fixed(3), ""),
        form(&[0x95], "zeropage,x",   ONE_ZP,  Cycles::fixed(4), ""),
        form(&[0x8D], "absolute",     ONE_ABS, Cycles::fixed(4), ""),
        form(&[0x9D], "absolute,x",   ONE_ABS, Cycles::fixed(5), ""),
        form(&[0x99], "absolute,y",   ONE_ABS, Cycles::fixed(5), ""),
        form(&[0x81], "(indirect,x)", ONE_ZP,  Cycles::fixed(6), ""),
        form(&[0x91], "(indirect),y", ONE_ZP,  Cycles::fixed(6), ""),
    ]),
    inst!("STX", "Store X register", [
        form(&[0x86], "zeropage",   ONE_ZP,  Cycles::fixed(3), ""),
        form(&[0x96], "zeropage,y", ONE_ZP,  Cycles::fixed(4), ""),
        form(&[0x8E], "absolute",   ONE_ABS, Cycles::fixed(4), ""),
    ]),
    inst!("STY", "Store Y register", [
        form(&[0x84], "zeropage",   ONE_ZP,  Cycles::fixed(3), ""),
        form(&[0x94], "zeropage,x", ONE_ZP,  Cycles::fixed(4), ""),
        form(&[0x8C], "absolute",   ONE_ABS, Cycles::fixed(4), ""),
    ]),
    inst!("TAX", "Transfer accumulator to X", [form(&[0xAA], "implied", NONE, Cycles::fixed(2), "NZ")]),
    inst!("TAY", "Transfer accumulator to Y", [form(&[0xA8], "implied", NONE, Cycles::fixed(2), "NZ")]),
    inst!("TXA", "Transfer X to accumulator", [form(&[0x8A], "implied", NONE, Cycles::fixed(2), "NZ")]),
    inst!("TYA", "Transfer Y to accumulator", [form(&[0x98], "implied", NONE, Cycles::fixed(2), "NZ")]),
    inst!("TSX", "Transfer stack pointer to X", [form(&[0xBA], "implied", NONE, Cycles::fixed(2), "NZ")]),
    inst!("TXS", "Transfer X to stack pointer", [form(&[0x9A], "implied", NONE, Cycles::fixed(2), "")]),
    inst!("PHA", "Push accumulator", [form(&[0x48], "implied", NONE, Cycles::fixed(3), "")]),
    inst!("PLA", "Pull accumulator", [form(&[0x68], "implied", NONE, Cycles::fixed(4), "NZ")]),
    inst!("PHP", "Push processor status", [form(&[0x08], "implied", NONE, Cycles::fixed(3), "")]),
    inst!("PLP", "Pull processor status", [form(&[0x28], "implied", NONE, Cycles::fixed(4), "NVDIZC")]),
    inst!("INX", "Increment X", [form(&[0xE8], "implied", NONE, Cycles::fixed(2), "NZ")]),
    inst!("INY", "Increment Y", [form(&[0xC8], "implied", NONE, Cycles::fixed(2), "NZ")]),
    inst!("DEX", "Decrement X", [form(&[0xCA], "implied", NONE, Cycles::fixed(2), "NZ")]),
    inst!("DEY", "Decrement Y", [form(&[0x88], "implied", NONE, Cycles::fixed(2), "NZ")]),
    inst!("INC", "Increment memory", [
        form(&[0xE6], "zeropage",   ONE_ZP,  Cycles::fixed(5), "NZ"),
        form(&[0xF6], "zeropage,x", ONE_ZP,  Cycles::fixed(6), "NZ"),
        form(&[0xEE], "absolute",   ONE_ABS, Cycles::fixed(6), "NZ"),
        form(&[0xFE], "absolute,x", ONE_ABS, Cycles::fixed(7), "NZ"),
    ]),
    inst!("DEC", "Decrement memory", [
        form(&[0xC6], "zeropage",   ONE_ZP,  Cycles::fixed(5), "NZ"),
        form(&[0xD6], "zeropage,x", ONE_ZP,  Cycles::fixed(6), "NZ"),
        form(&[0xCE], "absolute",   ONE_ABS, Cycles::fixed(6), "NZ"),
        form(&[0xDE], "absolute,x", ONE_ABS, Cycles::fixed(7), "NZ"),
    ]),
    inst!("ADC", "Add with carry", [
        form(&[0x69], "immediate",     ONE_IMM, Cycles::fixed(2),         "NZCV"),
        form(&[0x65], "zeropage",      ONE_ZP,  Cycles::fixed(3),         "NZCV"),
        form(&[0x75], "zeropage,x",    ONE_ZP,  Cycles::fixed(4),         "NZCV"),
        form(&[0x6D], "absolute",      ONE_ABS, Cycles::fixed(4),         "NZCV"),
        form(&[0x7D], "absolute,x",    ONE_ABS, Cycles::page_crossing(4), "NZCV"),
        form(&[0x79], "absolute,y",    ONE_ABS, Cycles::page_crossing(4), "NZCV"),
        form(&[0x61], "(indirect,x)",  ONE_ZP,  Cycles::fixed(6),         "NZCV"),
        form(&[0x71], "(indirect),y",  ONE_ZP,  Cycles::page_crossing(5), "NZCV"),
    ]),
    inst!("SBC", "Subtract with carry", [
        form(&[0xE9], "immediate",     ONE_IMM, Cycles::fixed(2),         "NZCV"),
        form(&[0xE5], "zeropage",      ONE_ZP,  Cycles::fixed(3),         "NZCV"),
        form(&[0xF5], "zeropage,x",    ONE_ZP,  Cycles::fixed(4),         "NZCV"),
        form(&[0xED], "absolute",      ONE_ABS, Cycles::fixed(4),         "NZCV"),
        form(&[0xFD], "absolute,x",    ONE_ABS, Cycles::page_crossing(4), "NZCV"),
        form(&[0xF9], "absolute,y",    ONE_ABS, Cycles::page_crossing(4), "NZCV"),
        form(&[0xE1], "(indirect,x)",  ONE_ZP,  Cycles::fixed(6),         "NZCV"),
        form(&[0xF1], "(indirect),y",  ONE_ZP,  Cycles::page_crossing(5), "NZCV"),
    ]),
    inst!("AND", "Logical AND", [
        form(&[0x29], "immediate",     ONE_IMM, Cycles::fixed(2),         "NZ"),
        form(&[0x25], "zeropage",      ONE_ZP,  Cycles::fixed(3),         "NZ"),
        form(&[0x35], "zeropage,x",    ONE_ZP,  Cycles::fixed(4),         "NZ"),
        form(&[0x2D], "absolute",      ONE_ABS, Cycles::fixed(4),         "NZ"),
        form(&[0x3D], "absolute,x",    ONE_ABS, Cycles::page_crossing(4), "NZ"),
        form(&[0x39], "absolute,y",    ONE_ABS, Cycles::page_crossing(4), "NZ"),
        form(&[0x21], "(indirect,x)",  ONE_ZP,  Cycles::fixed(6),         "NZ"),
        form(&[0x31], "(indirect),y",  ONE_ZP,  Cycles::page_crossing(5), "NZ"),
    ]),
    inst!("ORA", "Logical inclusive OR", [
        form(&[0x09], "immediate",     ONE_IMM, Cycles::fixed(2),         "NZ"),
        form(&[0x05], "zeropage",      ONE_ZP,  Cycles::fixed(3),         "NZ"),
        form(&[0x15], "zeropage,x",    ONE_ZP,  Cycles::fixed(4),         "NZ"),
        form(&[0x0D], "absolute",      ONE_ABS, Cycles::fixed(4),         "NZ"),
        form(&[0x1D], "absolute,x",    ONE_ABS, Cycles::page_crossing(4), "NZ"),
        form(&[0x19], "absolute,y",    ONE_ABS, Cycles::page_crossing(4), "NZ"),
        form(&[0x01], "(indirect,x)",  ONE_ZP,  Cycles::fixed(6),         "NZ"),
        form(&[0x11], "(indirect),y",  ONE_ZP,  Cycles::page_crossing(5), "NZ"),
    ]),
    inst!("EOR", "Exclusive OR", [
        form(&[0x49], "immediate",     ONE_IMM, Cycles::fixed(2),         "NZ"),
        form(&[0x45], "zeropage",      ONE_ZP,  Cycles::fixed(3),         "NZ"),
        form(&[0x55], "zeropage,x",    ONE_ZP,  Cycles::fixed(4),         "NZ"),
        form(&[0x4D], "absolute",      ONE_ABS, Cycles::fixed(4),         "NZ"),
        form(&[0x5D], "absolute,x",    ONE_ABS, Cycles::page_crossing(4), "NZ"),
        form(&[0x59], "absolute,y",    ONE_ABS, Cycles::page_crossing(4), "NZ"),
        form(&[0x41], "(indirect,x)",  ONE_ZP,  Cycles::fixed(6),         "NZ"),
        form(&[0x51], "(indirect),y",  ONE_ZP,  Cycles::page_crossing(5), "NZ"),
    ]),
    inst!("CMP", "Compare accumulator", [
        form(&[0xC9], "immediate",     ONE_IMM, Cycles::fixed(2),         "NZC"),
        form(&[0xC5], "zeropage",      ONE_ZP,  Cycles::fixed(3),         "NZC"),
        form(&[0xD5], "zeropage,x",    ONE_ZP,  Cycles::fixed(4),         "NZC"),
        form(&[0xCD], "absolute",      ONE_ABS, Cycles::fixed(4),         "NZC"),
        form(&[0xDD], "absolute,x",    ONE_ABS, Cycles::page_crossing(4), "NZC"),
        form(&[0xD9], "absolute,y",    ONE_ABS, Cycles::page_crossing(4), "NZC"),
        form(&[0xC1], "(indirect,x)",  ONE_ZP,  Cycles::fixed(6),         "NZC"),
        form(&[0xD1], "(indirect),y",  ONE_ZP,  Cycles::page_crossing(5), "NZC"),
    ]),
    inst!("CPX", "Compare X register", [
        form(&[0xE0], "immediate", ONE_IMM, Cycles::fixed(2), "NZC"),
        form(&[0xE4], "zeropage",  ONE_ZP,  Cycles::fixed(3), "NZC"),
        form(&[0xEC], "absolute",  ONE_ABS, Cycles::fixed(4), "NZC"),
    ]),
    inst!("CPY", "Compare Y register", [
        form(&[0xC0], "immediate", ONE_IMM, Cycles::fixed(2), "NZC"),
        form(&[0xC4], "zeropage",  ONE_ZP,  Cycles::fixed(3), "NZC"),
        form(&[0xCC], "absolute",  ONE_ABS, Cycles::fixed(4), "NZC"),
    ]),
    inst!("BIT", "Bit test", [
        form(&[0x24], "zeropage", ONE_ZP,  Cycles::fixed(3), "NZV"),
        form(&[0x2C], "absolute", ONE_ABS, Cycles::fixed(4), "NZV"),
    ]),
    inst!("ASL", "Arithmetic shift left", [
        form(&[0x0A], "accumulator", NONE,    Cycles::fixed(2), "NZC"),
        form(&[0x06], "zeropage",    ONE_ZP,  Cycles::fixed(5), "NZC"),
        form(&[0x16], "zeropage,x",  ONE_ZP,  Cycles::fixed(6), "NZC"),
        form(&[0x0E], "absolute",    ONE_ABS, Cycles::fixed(6), "NZC"),
        form(&[0x1E], "absolute,x",  ONE_ABS, Cycles::fixed(7), "NZC"),
    ]),
    inst!("LSR", "Logical shift right", [
        form(&[0x4A], "accumulator", NONE,    Cycles::fixed(2), "NZC"),
        form(&[0x46], "zeropage",    ONE_ZP,  Cycles::fixed(5), "NZC"),
        form(&[0x56], "zeropage,x",  ONE_ZP,  Cycles::fixed(6), "NZC"),
        form(&[0x4E], "absolute",    ONE_ABS, Cycles::fixed(6), "NZC"),
        form(&[0x5E], "absolute,x",  ONE_ABS, Cycles::fixed(7), "NZC"),
    ]),
    inst!("ROL", "Rotate left", [
        form(&[0x2A], "accumulator", NONE,    Cycles::fixed(2), "NZC"),
        form(&[0x26], "zeropage",    ONE_ZP,  Cycles::fixed(5), "NZC"),
        form(&[0x36], "zeropage,x",  ONE_ZP,  Cycles::fixed(6), "NZC"),
        form(&[0x2E], "absolute",    ONE_ABS, Cycles::fixed(6), "NZC"),
        form(&[0x3E], "absolute,x",  ONE_ABS, Cycles::fixed(7), "NZC"),
    ]),
    inst!("ROR", "Rotate right", [
        form(&[0x6A], "accumulator", NONE,    Cycles::fixed(2), "NZC"),
        form(&[0x66], "zeropage",    ONE_ZP,  Cycles::fixed(5), "NZC"),
        form(&[0x76], "zeropage,x",  ONE_ZP,  Cycles::fixed(6), "NZC"),
        form(&[0x6E], "absolute",    ONE_ABS, Cycles::fixed(6), "NZC"),
        form(&[0x7E], "absolute,x",  ONE_ABS, Cycles::fixed(7), "NZC"),
    ]),
    inst!("JMP", "Jump", [
        form(&[0x4C], "absolute", ONE_ABS, Cycles::fixed(3), ""),
        form(&[0x6C], "indirect", ONE_ABS, Cycles::fixed(5), ""),
    ]),
    inst!("JSR", "Jump to subroutine", [form(&[0x20], "absolute", ONE_ABS, Cycles::fixed(6), "")]),
    inst!("RTS", "Return from subroutine", [form(&[0x60], "implied", NONE, Cycles::fixed(6), "")]),
    inst!("RTI", "Return from interrupt", [form(&[0x40], "implied", NONE, Cycles::fixed(6), "NVDIZC")]),
    inst!("BRK", "Force interrupt", [form(&[0x00], "implied", NONE, Cycles::fixed(7), "I")]),
    inst!("NOP", "No operation", [form(&[0xEA], "implied", NONE, Cycles::fixed(2), "")]),
    inst!("BCC", "Branch if carry clear",    [form(&[0x90], "relative", ONE_REL, Cycles::branch(2), "")]),
    inst!("BCS", "Branch if carry set",      [form(&[0xB0], "relative", ONE_REL, Cycles::branch(2), "")]),
    inst!("BEQ", "Branch if equal",          [form(&[0xF0], "relative", ONE_REL, Cycles::branch(2), "")]),
    inst!("BMI", "Branch if minus",          [form(&[0x30], "relative", ONE_REL, Cycles::branch(2), "")]),
    inst!("BNE", "Branch if not equal",      [form(&[0xD0], "relative", ONE_REL, Cycles::branch(2), "")]),
    inst!("BPL", "Branch if positive",       [form(&[0x10], "relative", ONE_REL, Cycles::branch(2), "")]),
    inst!("BVC", "Branch if overflow clear", [form(&[0x50], "relative", ONE_REL, Cycles::branch(2), "")]),
    inst!("BVS", "Branch if overflow set",   [form(&[0x70], "relative", ONE_REL, Cycles::branch(2), "")]),
    inst!("CLC", "Clear carry",             [form(&[0x18], "implied", NONE, Cycles::fixed(2), "C")]),
    inst!("SEC", "Set carry",               [form(&[0x38], "implied", NONE, Cycles::fixed(2), "C")]),
    inst!("CLI", "Clear interrupt disable", [form(&[0x58], "implied", NONE, Cycles::fixed(2), "I")]),
    inst!("SEI", "Set interrupt disable",   [form(&[0x78], "implied", NONE, Cycles::fixed(2), "I")]),
    inst!("CLD", "Clear decimal",           [form(&[0xD8], "implied", NONE, Cycles::fixed(2), "D")]),
    inst!("SED", "Set decimal",             [form(&[0xF8], "implied", NONE, Cycles::fixed(2), "D")]),
    inst!("CLV", "Clear overflow",          [form(&[0xB8], "implied", NONE, Cycles::fixed(2), "V")]),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opcodes_are_unique() {
        let mut seen = [false; 256];
        for instruction in SET.instructions {
            for f in instruction.forms {
                assert_eq!(
                    f.opcode.len(),
                    1,
                    "6502 opcodes are single bytes: {}",
                    instruction.mnemonic
                );
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

    #[test]
    fn lengths_are_one_two_or_three_bytes() {
        for instruction in SET.instructions {
            for f in instruction.forms {
                let len = f.len();
                assert!(
                    (1..=3).contains(&len),
                    "{} {} has length {len}",
                    instruction.mnemonic,
                    f.mode
                );
            }
        }
    }

    #[test]
    fn known_encodings() {
        let lda = SET.instruction("LDA").expect("LDA exists");
        assert_eq!(lda.form("immediate").expect("LDA #").opcode, &[0xA9]);
        assert_eq!(lda.form("absolute,x").expect("LDA abs,x").opcode, &[0xBD]);

        let jmp = SET.instruction("JMP").expect("JMP exists");
        assert_eq!(jmp.form("indirect").expect("JMP ()").opcode, &[0x6C]);

        let sta = SET.instruction("STA").expect("STA exists");
        let absx = sta.form("absolute,x").expect("STA abs,x");
        assert_eq!(absx.opcode, &[0x9D]);
        // Writes always pay the indexing cycle — no page-cross variance.
        assert_eq!(absx.cycles.base, 5);
        assert_eq!(absx.cycles.page_cross, 0);

        let bne = SET.instruction("BNE").expect("BNE exists");
        let rel = bne.form("relative").expect("BNE rel");
        assert_eq!(rel.operands, &[REL8]);
        assert_eq!(rel.len(), 2);
    }
}
