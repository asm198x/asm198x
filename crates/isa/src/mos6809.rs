//! Motorola 6809 instruction set — opcode tables authored from the datasheet.
//!
//! Unlike the 6502/Z80 byte-opcode tables (fixed-width operand slots) and the
//! 68000 field tables, the 6809 is a hybrid: most modes are fixed byte-opcodes,
//! but indexed addressing uses a *computed postbyte* (+ 0/1/2 extension bytes).
//! So this spec just holds the per-mode opcodes; the `lwasm` dialect reads them
//! and computes the encoding into the engine's `Operation::Encoded` pieces. The
//! 6809 is big-endian.
//!
//! Opcodes are byte slices so the `$10`/`$11`-prefixed forms (LDY, CMPD, the
//! long conditional branches…) are uniform. An empty slice means the mode is
//! not supported by that mnemonic.

/// One 6809 instruction and the shape of its operands.
pub struct Insn {
    pub mnemonic: &'static str,
    pub kind: Kind,
}

/// The operand shape of an instruction — which addressing modes it supports.
pub enum Kind {
    /// No operand: just the opcode bytes (e.g. `rts`, `clra`, `nop`).
    Inherent(&'static [u8]),
    /// A PC-relative branch: the 8-bit (`short`) and 16-bit (`long`) opcodes.
    Branch {
        short: &'static [u8],
        long: &'static [u8],
    },
    /// A register/memory operation with some subset of the standard addressing
    /// modes. An empty opcode slice marks an unsupported mode. `width` is the
    /// immediate/data width in bytes (1 for `lda`, 2 for `ldx`/`ldd`).
    Mem {
        imm: &'static [u8],
        direct: &'static [u8],
        indexed: &'static [u8],
        extended: &'static [u8],
        width: u8,
    },
}

impl Insn {
    const fn mem(
        mnemonic: &'static str,
        imm: &'static [u8],
        direct: &'static [u8],
        indexed: &'static [u8],
        extended: &'static [u8],
        width: u8,
    ) -> Self {
        Insn {
            mnemonic,
            kind: Kind::Mem {
                imm,
                direct,
                indexed,
                extended,
                width,
            },
        }
    }
    const fn inh(mnemonic: &'static str, opcode: &'static [u8]) -> Self {
        Insn {
            mnemonic,
            kind: Kind::Inherent(opcode),
        }
    }
    const fn branch(mnemonic: &'static str, short: &'static [u8], long: &'static [u8]) -> Self {
        Insn {
            mnemonic,
            kind: Kind::Branch { short, long },
        }
    }
}

/// Look an instruction up by mnemonic (case-insensitive caller).
#[must_use]
pub fn lookup(mnemonic: &str) -> Option<&'static Insn> {
    SET.iter().find(|i| i.mnemonic == mnemonic)
}

/// A minimal big-endian [`InstructionSet`](crate::InstructionSet) for the 6809.
///
/// The 6809 dialect computes its own encoding into the engine's
/// `Operation::Encoded` pieces (it consults [`SET`], not `find_form`), so the
/// engine never looks a form up here — `instructions` is empty. This exists only
/// to carry the **big-endian** byte order the engine uses when laying down
/// `fcb`/`fdb` data and the computed value pieces.
pub static INSTRUCTION_SET: crate::InstructionSet = crate::InstructionSet {
    cpu: "Motorola 6809",
    endianness: crate::Endianness::Big,
    instructions: &[],
};

// Helper to keep the table readable: a one-byte opcode slice.
macro_rules! op {
    ($($b:expr),*) => { &[$($b),*] };
}

/// The 6809 instruction set (a representative subset; grows mechanically).
pub static SET: &[Insn] = &[
    // --- 8-bit loads/stores --------------------------------------------------
    Insn::mem("lda", op![0x86], op![0x96], op![0xA6], op![0xB6], 1),
    Insn::mem("ldb", op![0xC6], op![0xD6], op![0xE6], op![0xF6], 1),
    Insn::mem("sta", &[], op![0x97], op![0xA7], op![0xB7], 1),
    Insn::mem("stb", &[], op![0xD7], op![0xE7], op![0xF7], 1),
    // --- 16-bit loads/stores -------------------------------------------------
    Insn::mem("ldd", op![0xCC], op![0xDC], op![0xEC], op![0xFC], 2),
    Insn::mem("std", &[], op![0xDD], op![0xED], op![0xFD], 2),
    Insn::mem("ldx", op![0x8E], op![0x9E], op![0xAE], op![0xBE], 2),
    Insn::mem("stx", &[], op![0x9F], op![0xAF], op![0xBF], 2),
    Insn::mem("ldu", op![0xCE], op![0xDE], op![0xEE], op![0xFE], 2),
    Insn::mem("stu", &[], op![0xDF], op![0xEF], op![0xFF], 2),
    Insn::mem("ldy", op![0x10, 0x8E], op![0x10, 0x9E], op![0x10, 0xAE], op![0x10, 0xBE], 2),
    Insn::mem("sty", &[], op![0x10, 0x9F], op![0x10, 0xAF], op![0x10, 0xBF], 2),
    Insn::mem("lds", op![0x10, 0xCE], op![0x10, 0xDE], op![0x10, 0xEE], op![0x10, 0xFE], 2),
    Insn::mem("sts", &[], op![0x10, 0xDF], op![0x10, 0xEF], op![0x10, 0xFF], 2),
    // --- arithmetic / logic --------------------------------------------------
    Insn::mem("adda", op![0x8B], op![0x9B], op![0xAB], op![0xBB], 1),
    Insn::mem("addb", op![0xCB], op![0xDB], op![0xEB], op![0xFB], 1),
    Insn::mem("addd", op![0xC3], op![0xD3], op![0xE3], op![0xF3], 2),
    Insn::mem("suba", op![0x80], op![0x90], op![0xA0], op![0xB0], 1),
    Insn::mem("subb", op![0xC0], op![0xD0], op![0xE0], op![0xF0], 1),
    Insn::mem("subd", op![0x83], op![0x93], op![0xA3], op![0xB3], 2),
    Insn::mem("cmpa", op![0x81], op![0x91], op![0xA1], op![0xB1], 1),
    Insn::mem("cmpb", op![0xC1], op![0xD1], op![0xE1], op![0xF1], 1),
    Insn::mem("cmpx", op![0x8C], op![0x9C], op![0xAC], op![0xBC], 2),
    Insn::mem("anda", op![0x84], op![0x94], op![0xA4], op![0xB4], 1),
    Insn::mem("andb", op![0xC4], op![0xD4], op![0xE4], op![0xF4], 1),
    Insn::mem("ora", op![0x8A], op![0x9A], op![0xAA], op![0xBA], 1),
    Insn::mem("orb", op![0xCA], op![0xDA], op![0xEA], op![0xFA], 1),
    Insn::mem("eora", op![0x88], op![0x98], op![0xA8], op![0xB8], 1),
    Insn::mem("eorb", op![0xC8], op![0xD8], op![0xE8], op![0xF8], 1),
    // --- read-modify-write (no immediate) ------------------------------------
    Insn::mem("clr", &[], op![0x0F], op![0x6F], op![0x7F], 1),
    Insn::mem("inc", &[], op![0x0C], op![0x6C], op![0x7C], 1),
    Insn::mem("dec", &[], op![0x0A], op![0x6A], op![0x7A], 1),
    Insn::mem("tst", &[], op![0x0D], op![0x6D], op![0x7D], 1),
    Insn::mem("com", &[], op![0x03], op![0x63], op![0x73], 1),
    Insn::mem("neg", &[], op![0x00], op![0x60], op![0x70], 1),
    Insn::mem("lsr", &[], op![0x04], op![0x64], op![0x74], 1),
    Insn::mem("ror", &[], op![0x06], op![0x66], op![0x76], 1),
    Insn::mem("asr", &[], op![0x07], op![0x67], op![0x77], 1),
    Insn::mem("asl", &[], op![0x08], op![0x68], op![0x78], 1),
    Insn::mem("lsl", &[], op![0x08], op![0x68], op![0x78], 1),
    Insn::mem("rol", &[], op![0x09], op![0x69], op![0x79], 1),
    // --- control / jumps (no immediate) --------------------------------------
    Insn::mem("jmp", &[], op![0x0E], op![0x6E], op![0x7E], 0),
    Insn::mem("jsr", &[], op![0x9D], op![0xAD], op![0xBD], 0),
    // --- load effective address (indexed only) -------------------------------
    Insn::mem("leax", &[], &[], op![0x30], &[], 0),
    Insn::mem("leay", &[], &[], op![0x31], &[], 0),
    Insn::mem("leau", &[], &[], op![0x33], &[], 0),
    Insn::mem("leas", &[], &[], op![0x32], &[], 0),
    // --- inherent ------------------------------------------------------------
    Insn::inh("nop", op![0x12]),
    Insn::inh("sync", op![0x13]),
    Insn::inh("rts", op![0x39]),
    Insn::inh("rti", op![0x3B]),
    Insn::inh("swi", op![0x3F]),
    Insn::inh("abx", op![0x3A]),
    Insn::inh("mul", op![0x3D]),
    Insn::inh("sex", op![0x1D]),
    Insn::inh("daa", op![0x19]),
    Insn::inh("clra", op![0x4F]),
    Insn::inh("clrb", op![0x5F]),
    Insn::inh("nega", op![0x40]),
    Insn::inh("negb", op![0x50]),
    Insn::inh("coma", op![0x43]),
    Insn::inh("comb", op![0x53]),
    Insn::inh("inca", op![0x4C]),
    Insn::inh("incb", op![0x5C]),
    Insn::inh("deca", op![0x4A]),
    Insn::inh("decb", op![0x5A]),
    Insn::inh("tsta", op![0x4D]),
    Insn::inh("tstb", op![0x5D]),
    Insn::inh("lsra", op![0x44]),
    Insn::inh("lsrb", op![0x54]),
    Insn::inh("rora", op![0x46]),
    Insn::inh("rorb", op![0x56]),
    Insn::inh("asra", op![0x47]),
    Insn::inh("asrb", op![0x57]),
    Insn::inh("asla", op![0x48]),
    Insn::inh("aslb", op![0x58]),
    Insn::inh("lsla", op![0x48]),
    Insn::inh("lslb", op![0x58]),
    Insn::inh("rola", op![0x49]),
    Insn::inh("rolb", op![0x59]),
    // --- branches (short opcode, long opcode) --------------------------------
    Insn::branch("bra", op![0x20], op![0x16]),
    Insn::branch("brn", op![0x21], op![0x10, 0x21]),
    Insn::branch("bhi", op![0x22], op![0x10, 0x22]),
    Insn::branch("bls", op![0x23], op![0x10, 0x23]),
    Insn::branch("bcc", op![0x24], op![0x10, 0x24]),
    Insn::branch("bhs", op![0x24], op![0x10, 0x24]),
    Insn::branch("bcs", op![0x25], op![0x10, 0x25]),
    Insn::branch("blo", op![0x25], op![0x10, 0x25]),
    Insn::branch("bne", op![0x26], op![0x10, 0x26]),
    Insn::branch("beq", op![0x27], op![0x10, 0x27]),
    Insn::branch("bvc", op![0x28], op![0x10, 0x28]),
    Insn::branch("bvs", op![0x29], op![0x10, 0x29]),
    Insn::branch("bpl", op![0x2A], op![0x10, 0x2A]),
    Insn::branch("bmi", op![0x2B], op![0x10, 0x2B]),
    Insn::branch("bge", op![0x2C], op![0x10, 0x2C]),
    Insn::branch("blt", op![0x2D], op![0x10, 0x2D]),
    Insn::branch("bgt", op![0x2E], op![0x10, 0x2E]),
    Insn::branch("ble", op![0x2F], op![0x10, 0x2F]),
    Insn::branch("bsr", op![0x8D], op![0x17]),
];
