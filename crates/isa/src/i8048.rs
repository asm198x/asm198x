//! Intel 8048 (MCS-48) instruction set.
//!
//! The 8048 is an 8-bit microcontroller: a resident accumulator, eight working
//! registers `R0`–`R7` (a bank-switched register file), two indirect pointers
//! `@R0`/`@R1`, and a small on-chip program/data space. The instruction surface
//! is Intel-flavoured but its own — `MOV A,Rr` / `ORL A,#data` / `DJNZ Rr,addr`
//! / `SEL RB0` — with numbers in Intel `H`-suffix hex.
//!
//! Encoding shapes:
//! - **fixed forms** — the great majority. Single-byte opcodes (a few carry a
//!   trailing immediate byte). Each form's **mode label is its operand
//!   template**: the fixed operand keywords (`a`, `psw`, `p1`, `bus`, `t`,
//!   `rb0`, `c`, `f0`, `@a`, `@r0`, …) appear verbatim, register-indexed
//!   operands enumerate one form per register (`a,r0`…`a,r7`, `INC A,Rr` low
//!   three bits into the opcode), and an 8-bit immediate is the placeholder
//!   `#N`. The dialect and disassembler both read the template straight off the
//!   label, so it doubles as the render string.
//! - **page-relative conditional jumps** — `JC`/`JNZ`/`JB0`…`JB7`/`DJNZ Rr,addr`
//!   are opcode + an 8-bit address *within the current 256-byte page* (the byte
//!   replaces the low 8 bits of the PC). Modelled with a `rel` operand byte; the
//!   dialect supplies `Lo(target)` (like the CDP1802 short branch), no engine
//!   path.
//! - **JMP / CALL** — an 11-bit absolute address whose high 3 bits are packed
//!   into the opcode (`opcode = base | (addr>>8 & 7)<<5`) and low 8 bits follow.
//!   The opcode byte is therefore a function of a possibly-forward address, so
//!   these are **not** spec forms — the dialect emits them through the
//!   computed-operand seam (the opcode byte built as an `Expr`), and the
//!   disassembler decodes the two opcode families directly. See the dialect.
//!
//! **Provenance.** Authored from Intel's *MCS-48 Family of Single Chip
//! Microcomputers User's Manual* (9800270D, primary library,
//! `reference/by-topic/cpu-8048/`), every opcode cross-checked byte-for-byte against `asl`
//! (`cpu 8048`). Cycle counts are byte-length-derived approximations
//! (documentation-grade); the 8048's exact machine-cycle timing is in the
//! manual.

use crate::{Cycles, Endianness, Form, Instruction, InstructionSet, Operand, OperandKind};

const IMM8: Operand = Operand {
    kind: OperandKind::Immediate,
    bytes: 1,
};
const NONE: &[Operand] = &[];
const ONE_IMM8: &[Operand] = &[IMM8];

pub const SET: InstructionSet = InstructionSet {
    cpu: "Intel 8048 (MCS-48)",
    endianness: Endianness::Little,
    instructions: INSTRUCTIONS,
};

const fn f(
    opcode: &'static [u8],
    mode: &'static str,
    operands: &'static [Operand],
    cycles: u8,
    flags: &'static str,
) -> Form {
    Form {
        opcode,
        mode,
        operands,
        suffix: &[],
        cycles: Cycles::fixed(cycles),
        flags,
        undocumented: false,
    }
}

/// Accumulator arithmetic/logic with the shape `A,Rr` (×8) + `A,@Ri` (×2) +
/// `A,#data`: `ADD`/`ADDC`/`XRL`.
macro_rules! acc {
    ($mn:literal, $sum:literal, $rb:literal, $ib:literal, $im:literal, $fl:literal) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[
                f(&[$rb + 0], "a,r0", NONE, 1, $fl),
                f(&[$rb + 1], "a,r1", NONE, 1, $fl),
                f(&[$rb + 2], "a,r2", NONE, 1, $fl),
                f(&[$rb + 3], "a,r3", NONE, 1, $fl),
                f(&[$rb + 4], "a,r4", NONE, 1, $fl),
                f(&[$rb + 5], "a,r5", NONE, 1, $fl),
                f(&[$rb + 6], "a,r6", NONE, 1, $fl),
                f(&[$rb + 7], "a,r7", NONE, 1, $fl),
                f(&[$ib + 0], "a,@r0", NONE, 1, $fl),
                f(&[$ib + 1], "a,@r1", NONE, 1, $fl),
                f(&[$im], "a,#N", ONE_IMM8, 2, $fl),
            ],
        }
    };
}

/// `ANL`/`ORL`: the `acc` shape plus the `BUS,#data` / `Pp,#data` port masks
/// (`$busbase` = `BUS`, `+1` = `P1`, `+2` = `P2`).
macro_rules! logic {
    ($mn:literal, $sum:literal, $rb:literal, $ib:literal, $im:literal, $busbase:literal) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[
                f(&[$rb + 0], "a,r0", NONE, 1, ""),
                f(&[$rb + 1], "a,r1", NONE, 1, ""),
                f(&[$rb + 2], "a,r2", NONE, 1, ""),
                f(&[$rb + 3], "a,r3", NONE, 1, ""),
                f(&[$rb + 4], "a,r4", NONE, 1, ""),
                f(&[$rb + 5], "a,r5", NONE, 1, ""),
                f(&[$rb + 6], "a,r6", NONE, 1, ""),
                f(&[$rb + 7], "a,r7", NONE, 1, ""),
                f(&[$ib + 0], "a,@r0", NONE, 1, ""),
                f(&[$ib + 1], "a,@r1", NONE, 1, ""),
                f(&[$im], "a,#N", ONE_IMM8, 2, ""),
                f(&[$busbase + 0], "bus,#N", ONE_IMM8, 2, ""),
                f(&[$busbase + 1], "p1,#N", ONE_IMM8, 2, ""),
                f(&[$busbase + 2], "p2,#N", ONE_IMM8, 2, ""),
            ],
        }
    };
}

/// A page-relative conditional jump: opcode + an 8-bit same-page address.
macro_rules! rel {
    ($mn:literal, $sum:literal, $op:literal) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[f(&[$op], "rel", ONE_IMM8, 2, "")],
        }
    };
}

/// A single fixed keyword form (no value): `mode` is the literal operand text.
macro_rules! kw {
    ($mn:literal, $sum:literal, $op:literal, $mode:literal) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[f(&[$op], $mode, NONE, 1, "")],
        }
    };
    ($mn:literal, $sum:literal, [ $( ($op:literal, $mode:literal) ),* $(,)? ]) => {
        Instruction {
            mnemonic: $mn,
            summary: $sum,
            forms: &[ $( f(&[$op], $mode, NONE, 1, "") ),* ],
        }
    };
}

#[rustfmt::skip]
const INSTRUCTIONS: &[Instruction] = &[
    // ===================== accumulator arithmetic / logic =====================
    acc!("ADD",  "Add to A",           0x68, 0x60, 0x03, "C AC"),
    acc!("ADDC", "Add with carry to A", 0x78, 0x70, 0x13, "C AC"),
    logic!("ANL", "AND with A",         0x58, 0x50, 0x53, 0x98),
    logic!("ORL", "OR with A",          0x48, 0x40, 0x43, 0x88),
    acc!("XRL",  "XOR with A",          0xD8, 0xD0, 0xD3, ""),

    // ===================== increment / decrement =====================
    Instruction { mnemonic: "INC", summary: "Increment", forms: &[
        f(&[0x17], "a", NONE, 1, ""),
        f(&[0x18], "r0", NONE, 1, ""), f(&[0x19], "r1", NONE, 1, ""),
        f(&[0x1A], "r2", NONE, 1, ""), f(&[0x1B], "r3", NONE, 1, ""),
        f(&[0x1C], "r4", NONE, 1, ""), f(&[0x1D], "r5", NONE, 1, ""),
        f(&[0x1E], "r6", NONE, 1, ""), f(&[0x1F], "r7", NONE, 1, ""),
        f(&[0x10], "@r0", NONE, 1, ""), f(&[0x11], "@r1", NONE, 1, ""),
    ] },
    Instruction { mnemonic: "DEC", summary: "Decrement", forms: &[
        f(&[0x07], "a", NONE, 1, ""),
        f(&[0xC8], "r0", NONE, 1, ""), f(&[0xC9], "r1", NONE, 1, ""),
        f(&[0xCA], "r2", NONE, 1, ""), f(&[0xCB], "r3", NONE, 1, ""),
        f(&[0xCC], "r4", NONE, 1, ""), f(&[0xCD], "r5", NONE, 1, ""),
        f(&[0xCE], "r6", NONE, 1, ""), f(&[0xCF], "r7", NONE, 1, ""),
    ] },

    // ===================== accumulator unary =====================
    Instruction { mnemonic: "CLR", summary: "Clear", forms: &[
        f(&[0x27], "a", NONE, 1, ""), f(&[0x97], "c", NONE, 1, "C"),
        f(&[0x85], "f0", NONE, 1, ""), f(&[0xA5], "f1", NONE, 1, ""),
    ] },
    Instruction { mnemonic: "CPL", summary: "Complement", forms: &[
        f(&[0x37], "a", NONE, 1, ""), f(&[0xA7], "c", NONE, 1, "C"),
        f(&[0x95], "f0", NONE, 1, ""), f(&[0xB5], "f1", NONE, 1, ""),
    ] },
    kw!("DA",   "Decimal-adjust A",  0x57, "a"),
    kw!("SWAP", "Swap nibbles of A", 0x47, "a"),
    kw!("RL",   "Rotate A left",             0xE7, "a"),
    kw!("RLC",  "Rotate A left through carry",  0xF7, "a"),
    kw!("RR",   "Rotate A right",            0x77, "a"),
    kw!("RRC",  "Rotate A right through carry", 0x67, "a"),

    // ===================== I/O =====================
    kw!("IN",   "Input port to A", [ (0x09, "a,p1"), (0x0A, "a,p2") ]),
    kw!("INS",  "Input BUS to A",  0x08, "a,bus"),
    kw!("OUTL", "Output A to port", [ (0x02, "bus,a"), (0x39, "p1,a"), (0x3A, "p2,a") ]),
    kw!("MOVD", "Move 4-bit port", [
        (0x0C, "a,p4"), (0x0D, "a,p5"), (0x0E, "a,p6"), (0x0F, "a,p7"),
        (0x3C, "p4,a"), (0x3D, "p5,a"), (0x3E, "p6,a"), (0x3F, "p7,a"),
    ]),
    kw!("ANLD", "AND A to 4-bit port", [
        (0x9C, "p4,a"), (0x9D, "p5,a"), (0x9E, "p6,a"), (0x9F, "p7,a"),
    ]),
    kw!("ORLD", "OR A to 4-bit port", [
        (0x8C, "p4,a"), (0x8D, "p5,a"), (0x8E, "p6,a"), (0x8F, "p7,a"),
    ]),

    // ===================== moves =====================
    Instruction { mnemonic: "MOV", summary: "Move", forms: &[
        f(&[0xF8], "a,r0", NONE, 1, ""), f(&[0xF9], "a,r1", NONE, 1, ""),
        f(&[0xFA], "a,r2", NONE, 1, ""), f(&[0xFB], "a,r3", NONE, 1, ""),
        f(&[0xFC], "a,r4", NONE, 1, ""), f(&[0xFD], "a,r5", NONE, 1, ""),
        f(&[0xFE], "a,r6", NONE, 1, ""), f(&[0xFF], "a,r7", NONE, 1, ""),
        f(&[0xF0], "a,@r0", NONE, 1, ""), f(&[0xF1], "a,@r1", NONE, 1, ""),
        f(&[0x23], "a,#N", ONE_IMM8, 2, ""),
        f(&[0xA8], "r0,a", NONE, 1, ""), f(&[0xA9], "r1,a", NONE, 1, ""),
        f(&[0xAA], "r2,a", NONE, 1, ""), f(&[0xAB], "r3,a", NONE, 1, ""),
        f(&[0xAC], "r4,a", NONE, 1, ""), f(&[0xAD], "r5,a", NONE, 1, ""),
        f(&[0xAE], "r6,a", NONE, 1, ""), f(&[0xAF], "r7,a", NONE, 1, ""),
        f(&[0xA0], "@r0,a", NONE, 1, ""), f(&[0xA1], "@r1,a", NONE, 1, ""),
        f(&[0xB8], "r0,#N", ONE_IMM8, 2, ""), f(&[0xB9], "r1,#N", ONE_IMM8, 2, ""),
        f(&[0xBA], "r2,#N", ONE_IMM8, 2, ""), f(&[0xBB], "r3,#N", ONE_IMM8, 2, ""),
        f(&[0xBC], "r4,#N", ONE_IMM8, 2, ""), f(&[0xBD], "r5,#N", ONE_IMM8, 2, ""),
        f(&[0xBE], "r6,#N", ONE_IMM8, 2, ""), f(&[0xBF], "r7,#N", ONE_IMM8, 2, ""),
        f(&[0xB0], "@r0,#N", ONE_IMM8, 2, ""), f(&[0xB1], "@r1,#N", ONE_IMM8, 2, ""),
        f(&[0xC7], "a,psw", NONE, 1, ""), f(&[0xD7], "psw,a", NONE, 1, ""),
        f(&[0x42], "a,t", NONE, 1, ""), f(&[0x62], "t,a", NONE, 1, ""),
    ] },
    Instruction { mnemonic: "XCH", summary: "Exchange A", forms: &[
        f(&[0x28], "a,r0", NONE, 1, ""), f(&[0x29], "a,r1", NONE, 1, ""),
        f(&[0x2A], "a,r2", NONE, 1, ""), f(&[0x2B], "a,r3", NONE, 1, ""),
        f(&[0x2C], "a,r4", NONE, 1, ""), f(&[0x2D], "a,r5", NONE, 1, ""),
        f(&[0x2E], "a,r6", NONE, 1, ""), f(&[0x2F], "a,r7", NONE, 1, ""),
        f(&[0x20], "a,@r0", NONE, 1, ""), f(&[0x21], "a,@r1", NONE, 1, ""),
    ] },
    kw!("XCHD", "Exchange A low nibble", [ (0x30, "a,@r0"), (0x31, "a,@r1") ]),
    kw!("MOVX", "Move external data", [
        (0x80, "a,@r0"), (0x81, "a,@r1"), (0x90, "@r0,a"), (0x91, "@r1,a"),
    ]),
    kw!("MOVP",  "Move program memory to A",   0xA3, "a,@a"),
    kw!("MOVP3", "Move page-3 program memory", 0xE3, "a,@a"),

    // ===================== branch / control =====================
    kw!("JMPP", "Jump indirect within page", 0xB3, "@a"),
    Instruction { mnemonic: "RET",  summary: "Return",             forms: &[f(&[0x83], "", NONE, 2, "")] },
    Instruction { mnemonic: "RETR", summary: "Return, restore PSW", forms: &[f(&[0x93], "", NONE, 2, "")] },

    rel!("JC",   "Jump if carry",        0xF6),
    rel!("JNC",  "Jump if no carry",     0xE6),
    rel!("JZ",   "Jump if A zero",       0xC6),
    rel!("JNZ",  "Jump if A not zero",   0x96),
    rel!("JT0",  "Jump if T0 high",      0x36),
    rel!("JNT0", "Jump if T0 low",       0x26),
    rel!("JT1",  "Jump if T1 high",      0x56),
    rel!("JNT1", "Jump if T1 low",       0x46),
    rel!("JF0",  "Jump if F0 set",       0xB6),
    rel!("JF1",  "Jump if F1 set",       0x76),
    rel!("JTF",  "Jump if timer flag",   0x16),
    rel!("JNI",  "Jump if interrupt",    0x86),
    rel!("JB0",  "Jump if A bit 0 set",  0x12),
    rel!("JB1",  "Jump if A bit 1 set",  0x32),
    rel!("JB2",  "Jump if A bit 2 set",  0x52),
    rel!("JB3",  "Jump if A bit 3 set",  0x72),
    rel!("JB4",  "Jump if A bit 4 set",  0x92),
    rel!("JB5",  "Jump if A bit 5 set",  0xB2),
    rel!("JB6",  "Jump if A bit 6 set",  0xD2),
    rel!("JB7",  "Jump if A bit 7 set",  0xF2),
    Instruction { mnemonic: "DJNZ", summary: "Decrement and jump if not zero", forms: &[
        f(&[0xE8], "r0,rel", ONE_IMM8, 2, ""), f(&[0xE9], "r1,rel", ONE_IMM8, 2, ""),
        f(&[0xEA], "r2,rel", ONE_IMM8, 2, ""), f(&[0xEB], "r3,rel", ONE_IMM8, 2, ""),
        f(&[0xEC], "r4,rel", ONE_IMM8, 2, ""), f(&[0xED], "r5,rel", ONE_IMM8, 2, ""),
        f(&[0xEE], "r6,rel", ONE_IMM8, 2, ""), f(&[0xEF], "r7,rel", ONE_IMM8, 2, ""),
    ] },

    kw!("EN",   "Enable",  [ (0x05, "i"), (0x25, "tcnti") ]),
    kw!("DIS",  "Disable", [ (0x15, "i"), (0x35, "tcnti") ]),
    kw!("SEL",  "Select bank", [ (0xC5, "rb0"), (0xD5, "rb1"), (0xE5, "mb0"), (0xF5, "mb1") ]),
    kw!("ENT0", "Enable T0 clock output", 0x75, "clk"),
    kw!("STRT", "Start timer/counter", [ (0x55, "t"), (0x45, "cnt") ]),
    kw!("STOP", "Stop timer/counter", 0x65, "tcnt"),
    kw!("NOP",  "No operation", 0x00, ""),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spot_check_opcodes() {
        let find = |mn: &str, mode: &str| {
            SET.instruction(mn)
                .and_then(|i| i.form(mode))
                .unwrap_or_else(|| panic!("no {mn} {mode}"))
                .opcode[0]
        };
        assert_eq!(find("ADD", "a,r0"), 0x68);
        assert_eq!(find("ADD", "a,r7"), 0x6F);
        assert_eq!(find("ADD", "a,@r1"), 0x61);
        assert_eq!(find("ADD", "a,#N"), 0x03);
        assert_eq!(find("MOV", "a,#N"), 0x23);
        assert_eq!(find("MOV", "r7,#N"), 0xBF);
        assert_eq!(find("MOV", "a,psw"), 0xC7);
        assert_eq!(find("INC", "@r0"), 0x10);
        assert_eq!(find("DEC", "r7"), 0xCF);
        assert_eq!(find("ANL", "p2,#N"), 0x9A);
        assert_eq!(find("ORL", "bus,#N"), 0x88);
        assert_eq!(find("JZ", "rel"), 0xC6);
        assert_eq!(find("JB7", "rel"), 0xF2);
        assert_eq!(find("DJNZ", "r3,rel"), 0xEB);
        assert_eq!(find("SEL", "mb1"), 0xF5);
        assert_eq!(find("MOVX", "@r1,a"), 0x91);
        assert_eq!(find("NOP", ""), 0x00);
    }

    #[test]
    fn no_duplicate_opcodes() {
        // JMP/CALL live outside the spec (computed), so every spec opcode is
        // unique across all forms.
        let mut seen = [false; 256];
        for insn in SET.instructions {
            for form in insn.forms {
                let op = form.opcode[0] as usize;
                assert!(
                    !seen[op],
                    "duplicate opcode {op:#04X} at {} {}",
                    insn.mnemonic, form.mode
                );
                seen[op] = true;
            }
        }
    }

    #[test]
    fn form_lengths() {
        // Fixed forms are 1 byte; immediate/rel forms are 2.
        for insn in SET.instructions {
            for form in insn.forms {
                let len = 1 + form
                    .operands
                    .iter()
                    .map(|o| o.bytes as usize)
                    .sum::<usize>();
                assert!(
                    (1..=2).contains(&len),
                    "{} {} len {len}",
                    insn.mnemonic,
                    form.mode
                );
            }
        }
    }
}
