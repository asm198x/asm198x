//! WDC 65C816 instruction set — the **extension** over the NMOS 6502.
//!
//! The 65816 keeps every documented 6502 opcode encoding unchanged and adds new
//! opcodes (in the 6502's undefined slots) plus new addressing modes on existing
//! mnemonics. So this is an *extension* [`InstructionSet`](crate::InstructionSet)
//! layered on [`crate::mos6502`] exactly as [`crate::z80::NEXT`] layers on the
//! Z80: the engine consults the 6502 set first, then this. It carries only what
//! the 6502 set lacks — never a duplicate of a 6502 form.
//!
//! It also includes the 65C02 additions the 65816 inherits (`bra`, `stz`,
//! `phx`/`phy`/`plx`/`ply`, `trb`/`tsb`, the `(dp)` indirect, `bit #`/`bit dp,x`/
//! `bit abs,x`, `inc a`/`dec a`, `jmp (abs,x)`).
//!
//! **Accumulator/index width (`m`/`x`).** In native mode the accumulator and
//! index immediates are 8- or 16-bit per the `m`/`x` status flags. The encoding
//! is the *same opcode* with a 1- or 2-byte operand, so each width is a distinct
//! form (`"immediate"` vs `"immediate16"`); the dialect picks one from its
//! parse-time `.a8`/`.a16`/`.i8`/`.i16` state. The engine never tracks the flag.
//!
//! Opcodes here are verified byte-for-byte against `ca65 --cpu 65816`. Cycle
//! counts are the published native-mode base values (DP=0, no page cross, 8-bit
//! `m`/`x`); the assembler ignores them and the disassembler does not yet
//! consume this set, so they are documentation-grade like the rest of `isa`.
//!
//! Deferred (increment 2): the block moves `mvn`/`mvp`, and `cop`/`wdm`.

use crate::{Cycles, Endianness, Form, Instruction, InstructionSet, Operand, OperandKind};

const IMM8: Operand = Operand {
    kind: OperandKind::Immediate,
    bytes: 1,
};
const IMM16: Operand = Operand {
    kind: OperandKind::Immediate,
    bytes: 2,
};
const DP: Operand = Operand {
    kind: OperandKind::Address,
    bytes: 1,
};
const ABS: Operand = Operand {
    kind: OperandKind::Address,
    bytes: 2,
};
const LONG: Operand = Operand {
    kind: OperandKind::Address,
    bytes: 3,
};
const REL16: Operand = Operand {
    kind: OperandKind::RelativePc,
    bytes: 2,
};

const NONE: &[Operand] = &[];
const ONE_IMM8: &[Operand] = &[IMM8];
const ONE_IMM16: &[Operand] = &[IMM16];
const ONE_DP: &[Operand] = &[DP];
const ONE_ABS: &[Operand] = &[ABS];
const ONE_LONG: &[Operand] = &[LONG];
const ONE_REL16: &[Operand] = &[REL16];

/// The 65816 extension set: everything the 6502 set lacks.
pub const SET: InstructionSet = InstructionSet {
    cpu: "WDC 65C816 (extension)",
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

/// The new addressing modes shared by the eight accumulator ALU ops. `base`
/// gives each op its opcode column; the mode→column offsets are uniform across
/// the group (verified against ca65). `imm16` is the 16-bit immediate opcode
/// (`None` for `sta`, which has no immediate form).
macro_rules! alu816 {
    ($mnemonic:literal, $summary:literal, $base:literal, imm16 = $imm16:literal) => {
        inst!(
            $mnemonic,
            $summary,
            [
                form(
                    &[$base | 0x09],
                    "immediate16",
                    ONE_IMM16,
                    Cycles::fixed(3),
                    "NZ"
                ),
                form(
                    &[$base | 0x12],
                    "(indirect)",
                    ONE_DP,
                    Cycles::fixed(5),
                    "NZ"
                ),
                form(
                    &[$base | 0x07],
                    "[indirect]",
                    ONE_DP,
                    Cycles::fixed(6),
                    "NZ"
                ),
                form(
                    &[$base | 0x17],
                    "[indirect],y",
                    ONE_DP,
                    Cycles::fixed(6),
                    "NZ"
                ),
                form(&[$base | 0x0F], "long", ONE_LONG, Cycles::fixed(5), "NZ"),
                form(&[$base | 0x1F], "long,x", ONE_LONG, Cycles::fixed(5), "NZ"),
                form(&[$base | 0x03], "stack,s", ONE_DP, Cycles::fixed(4), "NZ"),
                form(
                    &[$base | 0x13],
                    "(stack,s),y",
                    ONE_DP,
                    Cycles::fixed(7),
                    "NZ"
                ),
            ]
        )
    };
    // `sta` has the same new modes but no immediate.
    ($mnemonic:literal, $summary:literal, $base:literal, no_imm) => {
        inst!(
            $mnemonic,
            $summary,
            [
                form(&[$base | 0x12], "(indirect)", ONE_DP, Cycles::fixed(5), ""),
                form(&[$base | 0x07], "[indirect]", ONE_DP, Cycles::fixed(6), ""),
                form(
                    &[$base | 0x17],
                    "[indirect],y",
                    ONE_DP,
                    Cycles::fixed(6),
                    ""
                ),
                form(&[$base | 0x0F], "long", ONE_LONG, Cycles::fixed(5), ""),
                form(&[$base | 0x1F], "long,x", ONE_LONG, Cycles::fixed(5), ""),
                form(&[$base | 0x03], "stack,s", ONE_DP, Cycles::fixed(4), ""),
                form(&[$base | 0x13], "(stack,s),y", ONE_DP, Cycles::fixed(7), ""),
            ]
        )
    };
}

#[rustfmt::skip]
const INSTRUCTIONS: &[Instruction] = &[
    // --- accumulator ALU group: the new 65816 addressing modes ---------------
    alu816!("ORA", "OR accumulator",          0x00, imm16 = 0x09),
    alu816!("AND", "AND accumulator",         0x20, imm16 = 0x29),
    alu816!("EOR", "Exclusive-OR accumulator",0x40, imm16 = 0x49),
    alu816!("ADC", "Add with carry",          0x60, imm16 = 0x69),
    alu816!("LDA", "Load accumulator",        0xA0, imm16 = 0xA9),
    alu816!("CMP", "Compare accumulator",     0xC0, imm16 = 0xC9),
    alu816!("SBC", "Subtract with carry",     0xE0, imm16 = 0xE9),
    alu816!("STA", "Store accumulator",       0x80, no_imm),

    // --- 16-bit immediates for the index ops (the rest share 6502 modes) -----
    inst!("LDX", "Load X register",  [form(&[0xA2], "immediate16", ONE_IMM16, Cycles::fixed(3), "NZ")]),
    inst!("LDY", "Load Y register",  [form(&[0xA0], "immediate16", ONE_IMM16, Cycles::fixed(3), "NZ")]),
    inst!("CPX", "Compare X",        [form(&[0xE0], "immediate16", ONE_IMM16, Cycles::fixed(3), "NZC")]),
    inst!("CPY", "Compare Y",        [form(&[0xC0], "immediate16", ONE_IMM16, Cycles::fixed(3), "NZC")]),

    // --- BIT additions (immediate and indexed; dp/abs are 6502) --------------
    inst!("BIT", "Bit test", [
        form(&[0x89], "immediate",   ONE_IMM8,  Cycles::fixed(2), "Z"),
        form(&[0x89], "immediate16", ONE_IMM16, Cycles::fixed(3), "Z"),
        form(&[0x34], "zeropage,x",  ONE_DP,    Cycles::fixed(4), "NZV"),
        form(&[0x3C], "absolute,x",  ONE_ABS,   Cycles::fixed(4), "NZV"),
    ]),

    // --- STZ: store zero (whole instruction new) -----------------------------
    inst!("STZ", "Store zero", [
        form(&[0x64], "zeropage",   ONE_DP,  Cycles::fixed(3), ""),
        form(&[0x74], "zeropage,x", ONE_DP,  Cycles::fixed(4), ""),
        form(&[0x9C], "absolute",   ONE_ABS, Cycles::fixed(4), ""),
        form(&[0x9E], "absolute,x", ONE_ABS, Cycles::fixed(5), ""),
    ]),

    // --- INC/DEC accumulator (the memory forms are 6502) ---------------------
    inst!("INC", "Increment", [form(&[0x1A], "accumulator", NONE, Cycles::fixed(2), "NZ")]),
    inst!("DEC", "Decrement", [form(&[0x3A], "accumulator", NONE, Cycles::fixed(2), "NZ")]),

    // --- TRB/TSB: test-and-reset/set bits (whole instruction new) ------------
    inst!("TRB", "Test and reset bits", [
        form(&[0x14], "zeropage", ONE_DP,  Cycles::fixed(5), "Z"),
        form(&[0x1C], "absolute", ONE_ABS, Cycles::fixed(6), "Z"),
    ]),
    inst!("TSB", "Test and set bits", [
        form(&[0x04], "zeropage", ONE_DP,  Cycles::fixed(5), "Z"),
        form(&[0x0C], "absolute", ONE_ABS, Cycles::fixed(6), "Z"),
    ]),

    // --- jumps / subroutine: new modes and long forms ------------------------
    inst!("JMP", "Jump", [
        form(&[0x5C], "long",         ONE_LONG, Cycles::fixed(4), ""),
        form(&[0xDC], "[absolute]",   ONE_ABS,  Cycles::fixed(6), ""),
        form(&[0x7C], "(absolute,x)", ONE_ABS,  Cycles::fixed(6), ""),
    ]),
    inst!("JML", "Jump long", [
        form(&[0x5C], "long",       ONE_LONG, Cycles::fixed(4), ""),
        form(&[0xDC], "[absolute]", ONE_ABS,  Cycles::fixed(6), ""),
    ]),
    inst!("JSR", "Jump to subroutine", [
        form(&[0xFC], "(absolute,x)", ONE_ABS, Cycles::fixed(8), ""),
    ]),
    inst!("JSL", "Jump to subroutine long", [
        form(&[0x22], "long", ONE_LONG, Cycles::fixed(8), ""),
    ]),

    // --- push effective address / indirect / relative ------------------------
    inst!("PEA", "Push effective absolute address", [form(&[0xF4], "absolute",   ONE_ABS,   Cycles::fixed(5), "")]),
    inst!("PEI", "Push effective indirect address", [form(&[0xD4], "(indirect)", ONE_DP,    Cycles::fixed(6), "")]),
    inst!("PER", "Push effective relative address", [form(&[0x62], "relative16", ONE_REL16, Cycles::fixed(6), "")]),
    inst!("BRL", "Branch long",                     [form(&[0x82], "relative16", ONE_REL16, Cycles::fixed(4), "")]),

    // --- BRA: branch always (8-bit, 65C02) -----------------------------------
    inst!("BRA", "Branch always", [form(&[0x80], "relative", &[Operand { kind: OperandKind::RelativePc, bytes: 1 }], Cycles::fixed(3), "")]),

    // --- REP/SEP: reset/set status bits (8-bit immediate) --------------------
    inst!("REP", "Reset status bits", [form(&[0xC2], "immediate", ONE_IMM8, Cycles::fixed(3), "NVMXDIZC")]),
    inst!("SEP", "Set status bits",   [form(&[0xE2], "immediate", ONE_IMM8, Cycles::fixed(3), "NVMXDIZC")]),

    // --- register transfers (implied) ----------------------------------------
    inst!("TCD", "Transfer C to direct page",      [form(&[0x5B], "implied", NONE, Cycles::fixed(2), "NZ")]),
    inst!("TDC", "Transfer direct page to C",      [form(&[0x7B], "implied", NONE, Cycles::fixed(2), "NZ")]),
    inst!("TCS", "Transfer C to stack pointer",    [form(&[0x1B], "implied", NONE, Cycles::fixed(2), "")]),
    inst!("TSC", "Transfer stack pointer to C",    [form(&[0x3B], "implied", NONE, Cycles::fixed(2), "NZ")]),
    inst!("TXY", "Transfer X to Y",                [form(&[0x9B], "implied", NONE, Cycles::fixed(2), "NZ")]),
    inst!("TYX", "Transfer Y to X",                [form(&[0xBB], "implied", NONE, Cycles::fixed(2), "NZ")]),
    inst!("XBA", "Exchange B and A",               [form(&[0xEB], "implied", NONE, Cycles::fixed(3), "NZ")]),
    inst!("XCE", "Exchange carry and emulation",   [form(&[0xFB], "implied", NONE, Cycles::fixed(2), "C")]),

    // --- stack push/pull (implied) -------------------------------------------
    inst!("PHB", "Push data bank",       [form(&[0x8B], "implied", NONE, Cycles::fixed(3), "")]),
    inst!("PLB", "Pull data bank",       [form(&[0xAB], "implied", NONE, Cycles::fixed(4), "NZ")]),
    inst!("PHD", "Push direct page",     [form(&[0x0B], "implied", NONE, Cycles::fixed(4), "")]),
    inst!("PLD", "Pull direct page",     [form(&[0x2B], "implied", NONE, Cycles::fixed(5), "NZ")]),
    inst!("PHK", "Push program bank",    [form(&[0x4B], "implied", NONE, Cycles::fixed(3), "")]),
    inst!("PHX", "Push X",               [form(&[0xDA], "implied", NONE, Cycles::fixed(3), "")]),
    inst!("PHY", "Push Y",               [form(&[0x5A], "implied", NONE, Cycles::fixed(3), "")]),
    inst!("PLX", "Pull X",               [form(&[0xFA], "implied", NONE, Cycles::fixed(4), "NZ")]),
    inst!("PLY", "Pull Y",               [form(&[0x7A], "implied", NONE, Cycles::fixed(4), "NZ")]),

    // --- misc control (implied) ----------------------------------------------
    inst!("WAI", "Wait for interrupt",   [form(&[0xCB], "implied", NONE, Cycles::fixed(3), "")]),
    inst!("STP", "Stop the clock",       [form(&[0xDB], "implied", NONE, Cycles::fixed(3), "")]),
    inst!("RTL", "Return from subroutine long", [form(&[0x6B], "implied", NONE, Cycles::fixed(6), "")]),

    // --- block moves: opcode, then dest-bank, src-bank (note the order) ------
    inst!("MVN", "Block move next", [form(&[0x54], "block-move", &[DP, DP], Cycles::fixed(7), "")]),
    inst!("MVP", "Block move previous", [form(&[0x44], "block-move", &[DP, DP], Cycles::fixed(7), "")]),

    // --- co-processor / reserved (a bare signature byte, no `#`) --------------
    inst!("COP", "Co-processor enable", [form(&[0x02], "signature", ONE_IMM8, Cycles::fixed(7), "")]),
    inst!("WDM", "Reserved (no-op)",    [form(&[0x42], "signature", ONE_IMM8, Cycles::fixed(2), "")]),
];
