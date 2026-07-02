//! Hudson HuC6280 instruction set — the **extension** over the NMOS 6502.
//!
//! The HuC6280 (PC Engine / TurboGrafx-16 CPU) is a 65C02 superset. Like
//! [`crate::mos65816`] and [`crate::z80::NEXT`], this is an *extension*
//! [`InstructionSet`](crate::InstructionSet) layered on [`crate::mos6502`]: the
//! engine consults the 6502 set first, then this. It carries only what the NMOS
//! 6502 lacks — the 65C02 additions the HuC6280 inherits, the Rockwell bit
//! instructions (`rmb`/`smb`/`bbr`/`bbs`), and the HuC6280-specific instructions.
//!
//! This module holds the **fixed-slot** forms — everything whose encoding is an
//! opcode plus fixed-width operand slots. The HuC6280's exotic forms (the
//! block transfers `tii`/`tdd`/`tia`/`tai`/`tin`, `st0`–`st2`, `tam`/`tma`,
//! `tst`, `bsr`) are computed-operand / multi-field encodings that the engine
//! lays down through the `Operation::Encoded` seam; they are added in a later
//! increment (see `decisions/huc6280-addition.md`).
//!
//! **Provenance.** Authored from the manufacturer's *HuC6280 CMOS 8-bit
//! Microprocessor Software Manual* (in the primary library at
//! `reference/by-topic/cpu-huc6280/`), with every opcode cross-checked
//! byte-for-byte against `ca65 --cpu huc6280`. Cycle counts are published
//! base values and are documentation-grade (the assembler ignores them).

use crate::{Cycles, Endianness, Form, Instruction, InstructionSet, Operand, OperandKind};

const IMM8: Operand = Operand {
    kind: OperandKind::Immediate,
    bytes: 1,
};
const DP: Operand = Operand {
    kind: OperandKind::Address,
    bytes: 1,
};
const ABS: Operand = Operand {
    kind: OperandKind::Address,
    bytes: 2,
};
const REL: Operand = Operand {
    kind: OperandKind::RelativePc,
    bytes: 1,
};

const NONE: &[Operand] = &[];
const ONE_IMM8: &[Operand] = &[IMM8];
const ONE_DP: &[Operand] = &[DP];
const ONE_ABS: &[Operand] = &[ABS];
const ONE_REL: &[Operand] = &[REL];
/// `bbr`/`bbs`: a zero-page byte to test, then a PC-relative branch target.
const DP_REL: &[Operand] = &[DP, REL];

/// The HuC6280 extension set: what the 6502 set lacks. Fixed-slot forms only.
pub const SET: InstructionSet = InstructionSet {
    cpu: "Hudson HuC6280 (extension)",
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

/// An implied-mode instruction: one opcode byte, no operands.
macro_rules! implied {
    ($mnemonic:literal, $summary:literal, $op:literal, $flags:literal) => {
        inst!(
            $mnemonic,
            $summary,
            [form(&[$op], "implied", NONE, Cycles::fixed(2), $flags)]
        )
    };
}

/// A Rockwell `rmb`/`smb` bit op: one zero-page operand.
macro_rules! bit_zp {
    ($mnemonic:literal, $op:literal) => {
        inst!(
            $mnemonic,
            "Reset/set memory bit",
            [form(&[$op], "zeropage", ONE_DP, Cycles::fixed(7), "")]
        )
    };
}

/// A Rockwell `bbr`/`bbs` branch-on-bit: zero-page byte, then relative target.
macro_rules! bit_branch {
    ($mnemonic:literal, $op:literal) => {
        inst!(
            $mnemonic,
            "Branch on memory bit",
            [form(
                &[$op],
                "zeropage,relative",
                DP_REL,
                Cycles::branch(6),
                ""
            )]
        )
    };
}

#[rustfmt::skip]
const INSTRUCTIONS: &[Instruction] = &[
    // --- HuC6280 implied register ops ----------------------------------------
    implied!("SAX", "Swap A and X",           0x22, "NZ"),
    implied!("SAY", "Swap A and Y",           0x42, "NZ"),
    implied!("SXY", "Swap X and Y",           0x02, "NZ"),
    implied!("CLA", "Clear accumulator",      0x62, ""),
    implied!("CLX", "Clear X",                0x82, ""),
    implied!("CLY", "Clear Y",                0xC2, ""),
    implied!("CSL", "Clock select low (slow)",0x54, ""),
    implied!("CSH", "Clock select high (fast)",0xD4, ""),
    implied!("SET", "Set T flag",             0xF4, "T"),

    // --- 65C02 stack push/pull of X/Y (implied) ------------------------------
    implied!("PHX", "Push X",  0xDA, ""),
    implied!("PHY", "Push Y",  0x5A, ""),
    implied!("PLX", "Pull X",  0xFA, "NZ"),
    implied!("PLY", "Pull Y",  0x7A, "NZ"),

    // --- 65C02 INC/DEC accumulator (memory forms are 6502) -------------------
    inst!("INC", "Increment", [form(&[0x1A], "accumulator", NONE, Cycles::fixed(2), "NZ")]),
    inst!("DEC", "Decrement", [form(&[0x3A], "accumulator", NONE, Cycles::fixed(2), "NZ")]),

    // --- 65C02 BRA: branch always (8-bit relative) ---------------------------
    inst!("BRA", "Branch always", [form(&[0x80], "relative", ONE_REL, Cycles::branch(4), "")]),

    // --- 65C02 STZ: store zero -----------------------------------------------
    inst!("STZ", "Store zero", [
        form(&[0x64], "zeropage",   ONE_DP,  Cycles::fixed(4), ""),
        form(&[0x74], "zeropage,x", ONE_DP,  Cycles::fixed(4), ""),
        form(&[0x9C], "absolute",   ONE_ABS, Cycles::fixed(5), ""),
        form(&[0x9E], "absolute,x", ONE_ABS, Cycles::fixed(5), ""),
    ]),

    // --- 65C02 TRB/TSB: test-and-reset/set bits ------------------------------
    inst!("TRB", "Test and reset bits", [
        form(&[0x14], "zeropage", ONE_DP,  Cycles::fixed(6), "Z"),
        form(&[0x1C], "absolute", ONE_ABS, Cycles::fixed(7), "Z"),
    ]),
    inst!("TSB", "Test and set bits", [
        form(&[0x04], "zeropage", ONE_DP,  Cycles::fixed(6), "Z"),
        form(&[0x0C], "absolute", ONE_ABS, Cycles::fixed(7), "Z"),
    ]),

    // --- 65C02 BIT additions (immediate + indexed; dp/abs are 6502) ----------
    inst!("BIT", "Bit test", [
        form(&[0x89], "immediate",  ONE_IMM8, Cycles::fixed(2), "Z"),
        form(&[0x34], "zeropage,x", ONE_DP,   Cycles::fixed(4), "NZV"),
        form(&[0x3C], "absolute,x", ONE_ABS,  Cycles::fixed(5), "NZV"),
    ]),

    // --- 65C02 JMP (abs,x) ---------------------------------------------------
    inst!("JMP", "Jump", [form(&[0x7C], "(absolute,x)", ONE_ABS, Cycles::fixed(7), "")]),

    // --- 65C02 (dp) indirect for the eight accumulator ALU ops ---------------
    // Opcode is the op's column | 0x12 (verified against ca65).
    inst!("ORA", "OR accumulator",           [form(&[0x12], "(indirect)", ONE_DP, Cycles::fixed(7), "NZ")]),
    inst!("AND", "AND accumulator",          [form(&[0x32], "(indirect)", ONE_DP, Cycles::fixed(7), "NZ")]),
    inst!("EOR", "Exclusive-OR accumulator", [form(&[0x52], "(indirect)", ONE_DP, Cycles::fixed(7), "NZ")]),
    inst!("ADC", "Add with carry",           [form(&[0x72], "(indirect)", ONE_DP, Cycles::fixed(7), "NZCV")]),
    inst!("STA", "Store accumulator",        [form(&[0x92], "(indirect)", ONE_DP, Cycles::fixed(7), "")]),
    inst!("LDA", "Load accumulator",         [form(&[0xB2], "(indirect)", ONE_DP, Cycles::fixed(7), "NZ")]),
    inst!("CMP", "Compare accumulator",      [form(&[0xD2], "(indirect)", ONE_DP, Cycles::fixed(7), "NZC")]),
    inst!("SBC", "Subtract with carry",      [form(&[0xF2], "(indirect)", ONE_DP, Cycles::fixed(7), "NZCV")]),

    // --- Rockwell RMB0-7 / SMB0-7: reset/set a memory bit (zero-page) --------
    bit_zp!("RMB0", 0x07), bit_zp!("RMB1", 0x17), bit_zp!("RMB2", 0x27), bit_zp!("RMB3", 0x37),
    bit_zp!("RMB4", 0x47), bit_zp!("RMB5", 0x57), bit_zp!("RMB6", 0x67), bit_zp!("RMB7", 0x77),
    bit_zp!("SMB0", 0x87), bit_zp!("SMB1", 0x97), bit_zp!("SMB2", 0xA7), bit_zp!("SMB3", 0xB7),
    bit_zp!("SMB4", 0xC7), bit_zp!("SMB5", 0xD7), bit_zp!("SMB6", 0xE7), bit_zp!("SMB7", 0xF7),

    // --- Rockwell BBR0-7 / BBS0-7: branch on a memory bit (zp, then rel) -----
    bit_branch!("BBR0", 0x0F), bit_branch!("BBR1", 0x1F), bit_branch!("BBR2", 0x2F), bit_branch!("BBR3", 0x3F),
    bit_branch!("BBR4", 0x4F), bit_branch!("BBR5", 0x5F), bit_branch!("BBR6", 0x6F), bit_branch!("BBR7", 0x7F),
    bit_branch!("BBS0", 0x8F), bit_branch!("BBS1", 0x9F), bit_branch!("BBS2", 0xAF), bit_branch!("BBS3", 0xBF),
    bit_branch!("BBS4", 0xCF), bit_branch!("BBS5", 0xDF), bit_branch!("BBS6", 0xEF), bit_branch!("BBS7", 0xFF),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Spot-check opcodes against the values verified with `ca65 --cpu huc6280`.
    #[test]
    fn opcodes_match_reference() {
        let op = |m: &str, mode: &str| SET.find_form(m, mode).map(|f| f.opcode);
        // HuC6280 implied register ops.
        assert_eq!(op("SAX", "implied"), Some(&[0x22][..]));
        assert_eq!(op("CSH", "implied"), Some(&[0xD4][..]));
        assert_eq!(op("SET", "implied"), Some(&[0xF4][..]));
        // 65C02 additions.
        assert_eq!(op("STZ", "absolute"), Some(&[0x9C][..]));
        assert_eq!(op("BRA", "relative"), Some(&[0x80][..]));
        assert_eq!(op("LDA", "(indirect)"), Some(&[0xB2][..]));
        assert_eq!(op("JMP", "(absolute,x)"), Some(&[0x7C][..]));
        // Rockwell bit ops.
        assert_eq!(op("RMB0", "zeropage"), Some(&[0x07][..]));
        assert_eq!(op("SMB7", "zeropage"), Some(&[0xF7][..]));
        assert_eq!(op("BBR0", "zeropage,relative"), Some(&[0x0F][..]));
        assert_eq!(op("BBS7", "zeropage,relative"), Some(&[0xFF][..]));
    }

    /// `bbr`/`bbs` carry two operands: the tested zero-page byte, then the
    /// relative branch target.
    #[test]
    fn bit_branch_has_two_operands() {
        let f = SET.find_form("BBR0", "zeropage,relative").expect("bbr0");
        assert_eq!(f.operands.len(), 2);
        assert_eq!(f.operands[0].kind, OperandKind::Address);
        assert_eq!(f.operands[1].kind, OperandKind::RelativePc);
        assert_eq!(f.len(), 3); // opcode + zp + rel
    }
}
