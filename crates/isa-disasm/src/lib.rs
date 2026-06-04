//! Spec-driven disassembly — a dependency-free crate (only [`isa`] + std).
//!
//! Disassemblers for 6502, Z80, and 68000 that decode against the **same**
//! [`isa`] spec the assembler emits from, so the two are guaranteed consistent.
//! This crate carries no assembler, parser, or CLI, so a consumer like Emu198x
//! can render running code without pulling in the toolchain — see
//! `../../decisions/disassembler-crate.md`.
//!
//! For the byte-opcode CPUs (6502/Z80), [`decode_one`] matches opcode bytes
//! against the instruction set and reads the operand bytes, with no hand-written
//! decode tables; the 68000 is field-based (the inverse of its encoder). The
//! round-trip the authored-spec architecture was justified by — assemble →
//! disassemble → reassemble reproduces the bytes — is exercised in `asm198x`
//! (which has the assembler half). See the umbrella `asm198x-and-shared-isa-spec.md`.
//!
//! Decoding is CPU-agnostic; only the *rendering* of a matched form to text is
//! per-CPU. The Z80 renderer treats the mode label as an operand template
//! (`A,(IX+d)` with `nn`/`n`/`d`/`e` placeholders), which the pasmo front-end
//! parses straight back. The 6502 renderer instead maps the mode *name*
//! (`zeropage,x`, `(indirect),y`, …) to acme/ca65 operand syntax.
//!
//! Round-trip is byte-exact, even for flat binaries that interleave data. The
//! 6502 renderer prints absolute operands as 4-digit `$XXXX` and zero-page as
//! 2-digit `$XX`, and acme's hex-width rule re-encodes each at the same size —
//! so a low-address absolute (data misread as code, e.g. `$7E $00 $00` →
//! `ROR $0000,X`) stays 16-bit rather than collapsing to zero-page. The whole
//! C64 curriculum round-trips assemble → disassemble → reassemble identically.
//!
//! The one thing flat disassembly inherently cannot do is tell code from data:
//! embedded data (the C64 BASIC stub, sprite/CHR data, screen text) is shown as
//! instructions. That affects readability, not correctness — it still
//! re-encodes to the original bytes.

/// One disassembled instruction.
#[derive(Debug, Clone)]
pub struct Line {
    /// Address the instruction loads at (16-bit for 6502/Z80, 32-bit for 68000).
    pub addr: u32,
    /// The raw encoded bytes.
    pub bytes: Vec<u8>,
    /// The reassemblable source text, e.g. `"LD A,(IX+$05)"`.
    pub text: String,
}

/// Disassemble a flat Z80 binary loaded at `origin`. With `z80n`, the Spectrum
/// Next's Z80N opcodes are also decoded; otherwise only standard Z80 is.
#[must_use]
pub fn disassemble_z80(code: &[u8], origin: u16, z80n: bool) -> Vec<Line> {
    let sets: &[&isa::InstructionSet] = if z80n {
        &[&isa::z80::SET, &isa::z80::NEXT]
    } else {
        &[&isa::z80::SET]
    };
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < code.len() {
        let addr = origin.wrapping_add(pos as u16);
        if let Some((mnemonic, form, values, len)) = decode_one(sets, code, pos) {
            out.push(Line {
                addr: u32::from(addr),
                bytes: code[pos..pos + len].to_vec(),
                text: render_z80(mnemonic, form, &values, addr, len),
            });
            pos += len;
        } else {
            // Not a known opcode here: emit the byte as data and move on.
            out.push(Line {
                addr: u32::from(addr),
                bytes: vec![code[pos]],
                text: format!("DEFB ${:02X}", code[pos]),
            });
            pos += 1;
        }
    }
    out
}

/// Render a disassembly as reassemblable source text (one instruction per line).
#[must_use]
pub fn listing_z80(code: &[u8], origin: u16, z80n: bool) -> String {
    let mut s = format!("        org ${origin:04X}\n");
    for line in disassemble_z80(code, origin, z80n) {
        s.push_str("        ");
        s.push_str(&line.text);
        s.push('\n');
    }
    s
}

// ---------------------------------------------------------------------------
// Decode (CPU-agnostic): match opcode bytes against the spec
// ---------------------------------------------------------------------------

/// Match the bytes at `pos` against the instruction set, returning the matched
/// mnemonic, form, the read operand values (in form-slot order), and the total
/// encoded length. Tries a two-byte opcode before a one-byte one, since every
/// prefixed opcode (`CB`/`ED`/`DD`/`FD`) is two bytes and no prefix byte is
/// itself a one-byte opcode.
fn decode_one<'a>(
    sets: &[&'a isa::InstructionSet],
    code: &[u8],
    pos: usize,
) -> Option<(&'a str, &'a isa::Form, Vec<i64>, usize)> {
    for opcode_len in [2usize, 1] {
        if pos + opcode_len > code.len() {
            continue;
        }
        let opcode = &code[pos..pos + opcode_len];
        for set in sets {
            for insn in set.instructions {
                for form in insn.forms {
                    if form.opcode != opcode {
                        continue;
                    }
                    let operand_len: usize = form.operands.iter().map(|o| o.bytes as usize).sum();
                    let suffix_at = pos + opcode_len + operand_len;
                    let end = suffix_at + form.suffix.len();
                    if end > code.len() {
                        continue;
                    }
                    if code[suffix_at..end] != *form.suffix {
                        continue;
                    }
                    let values = read_operands(form, &code[pos + opcode_len..], set.endianness);
                    return Some((insn.mnemonic, form, values, end - pos));
                }
            }
        }
    }
    None
}

/// Read a form's operand bytes into raw integer values (signed for relative and
/// displacement operands), in the instruction set's endianness.
fn read_operands(form: &isa::Form, rest: &[u8], endianness: isa::Endianness) -> Vec<i64> {
    let mut values = Vec::new();
    let mut off = 0;
    for operand in form.operands {
        match operand.kind {
            isa::OperandKind::RelativePc | isa::OperandKind::Displacement => {
                values.push(i64::from(rest[off] as i8));
                off += 1;
            }
            isa::OperandKind::Immediate | isa::OperandKind::Address => match operand.bytes {
                1 => {
                    values.push(i64::from(rest[off]));
                    off += 1;
                }
                2 => {
                    let (lo, hi) = match endianness {
                        isa::Endianness::Little => (rest[off], rest[off + 1]),
                        isa::Endianness::Big => (rest[off + 1], rest[off]),
                    };
                    values.push(i64::from(u16::from(lo) | (u16::from(hi) << 8)));
                    off += 2;
                }
                _ => {}
            },
        }
    }
    values
}

// ---------------------------------------------------------------------------
// Render (Z80): fill the mode-label template with operand values
// ---------------------------------------------------------------------------

fn render_z80(mnemonic: &str, form: &isa::Form, values: &[i64], addr: u16, len: usize) -> String {
    if form.mode.is_empty() {
        return mnemonic.to_string();
    }
    // RST's target is the mode label as two hex digits; emit it as `$nn` so it
    // reassembles (the front-end reads `$` as hex).
    if mnemonic == "RST" {
        return format!("{mnemonic} ${}", form.mode);
    }
    format!(
        "{mnemonic} {}",
        render_operands(form.mode, values, addr, len)
    )
}

/// Substitute the `nn`/`n`/`+d`/`e` placeholders in a mode label with formatted
/// operand values, left to right (placeholders appear in operand-slot order).
fn render_operands(mode: &str, values: &[i64], addr: u16, len: usize) -> String {
    let bytes = mode.as_bytes();
    let mut out = String::new();
    let mut vi = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"nn") {
            out.push_str(&format!("${:04X}", values[vi] as u16));
            vi += 1;
            i += 2;
        } else if bytes[i] == b'+' && i + 1 < bytes.len() && bytes[i + 1] == b'd' {
            let d = values[vi];
            vi += 1;
            i += 2;
            if d < 0 {
                out.push_str(&format!("-${:02X}", (-d) as u8));
            } else {
                out.push_str(&format!("+${:02X}", d as u8));
            }
        } else if bytes[i] == b'n' {
            out.push_str(&format!("${:02X}", values[vi] as u8));
            vi += 1;
            i += 1;
        } else if bytes[i] == b'e' {
            let target = addr
                .wrapping_add(len as u16)
                .wrapping_add(values[vi] as u16);
            out.push_str(&format!("${target:04X}"));
            vi += 1;
            i += 1;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 6502 disassembly
// ---------------------------------------------------------------------------

/// Disassemble a flat 6502 binary loaded at `origin`. The text is acme/ca65
/// instruction syntax; an unrecognised byte becomes a `!byte` datum.
#[must_use]
pub fn disassemble_6502(code: &[u8], origin: u16) -> Vec<Line> {
    let sets: &[&isa::InstructionSet] = &[&isa::mos6502::SET];
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < code.len() {
        let addr = origin.wrapping_add(pos as u16);
        if let Some((mnemonic, form, values, len)) = decode_one(sets, code, pos) {
            out.push(Line {
                addr: u32::from(addr),
                bytes: code[pos..pos + len].to_vec(),
                text: render_6502(mnemonic, form, &values, addr, len),
            });
            pos += len;
        } else {
            out.push(Line {
                addr: u32::from(addr),
                bytes: vec![code[pos]],
                text: format!("!byte ${:02X}", code[pos]),
            });
            pos += 1;
        }
    }
    out
}

/// Render a 6502 disassembly as reassemblable acme source (one per line).
#[must_use]
pub fn listing_6502(code: &[u8], origin: u16) -> String {
    let mut s = format!("        *= ${origin:04X}\n");
    for line in disassemble_6502(code, origin) {
        s.push_str("        ");
        s.push_str(&line.text);
        s.push('\n');
    }
    s
}

/// Disassemble the single instruction at `addr`, reading its bytes through
/// `read` (a machine's memory peek), and return its rendered text and byte
/// length. This is the single-instruction, callback-shaped entry point Emu198x
/// consumes for its `disasm` debug command — mirroring the
/// `zilog_z80::disassemble(addr, read)` shape it already uses — where the
/// slice-based [`disassemble_6502`] serves flat-buffer listings. An undecodable
/// byte renders as data (`!byte $XX`, length 1), the same as the slice path.
#[must_use]
pub fn decode_one_6502(addr: u16, read: impl Fn(u16) -> u8) -> Option<(String, u8)> {
    // 3 bytes is the longest 6502 instruction; `disassemble_6502` consumes only
    // the first instruction, so any trailing bytes are ignored.
    let buf: Vec<u8> = (0..3).map(|i| read(addr.wrapping_add(i))).collect();
    let line = disassemble_6502(&buf, addr).into_iter().next()?;
    Some((line.text, line.bytes.len() as u8))
}

/// Render a matched 6502 form by mapping its addressing-mode label to operand
/// syntax. One-byte operands print as `$XX`, two-byte as `$XXXX`; a relative
/// branch prints its absolute target.
fn render_6502(mnemonic: &str, form: &isa::Form, values: &[i64], addr: u16, len: usize) -> String {
    let v = values.first().copied().unwrap_or(0);
    let lo = (v & 0xFF) as u8;
    let word = (v & 0xFFFF) as u16;
    let operand = match form.mode {
        // Accumulator mode renders as the bare mnemonic: acme rejects `asl a`
        // (it wants `asl`), and our parser reads the no-operand form as
        // accumulator where that is the only operand-less form.
        "implied" | "accumulator" => String::new(),
        "immediate" => format!("#${lo:02X}"),
        "zeropage" => format!("${lo:02X}"),
        "zeropage,x" => format!("${lo:02X},X"),
        "zeropage,y" => format!("${lo:02X},Y"),
        "absolute" => format!("${word:04X}"),
        "absolute,x" => format!("${word:04X},X"),
        "absolute,y" => format!("${word:04X},Y"),
        "indirect" => format!("(${word:04X})"),
        "(indirect,x)" => format!("(${lo:02X},X)"),
        "(indirect),y" => format!("(${lo:02X}),Y"),
        "relative" => {
            let target = addr.wrapping_add(len as u16).wrapping_add(v as u16);
            format!("${target:04X}")
        }
        other => other.to_string(),
    };
    if operand.is_empty() {
        mnemonic.to_string()
    } else {
        format!("{mnemonic} {operand}")
    }
}

// ---------------------------------------------------------------------------
// 68000 disassembly (field-based: the inverse of the m68k encoder)
// ---------------------------------------------------------------------------

use isa::m68k::{self, Size, SizeEnc, Slot, ea};

/// Disassemble a flat 68000 big-endian code image loaded at `origin`, rendering
/// vasm Motorola syntax. A word that matches no known instruction (or trailing
/// odd byte) becomes a `dc.w`/`dc.b` datum. Decodes the curriculum instruction
/// subset; unknown opcodes fall back to data.
#[must_use]
pub fn disassemble_68000(code: &[u8], origin: u32) -> Vec<Line> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos + 2 <= code.len() {
        let addr = origin.wrapping_add(pos as u32);
        if let Some((text, len)) = decode_m68k(code, pos, addr) {
            out.push(Line {
                addr,
                bytes: code[pos..pos + len].to_vec(),
                text,
            });
            pos += len;
        } else {
            let word = be16(code, pos);
            out.push(Line {
                addr,
                bytes: code[pos..pos + 2].to_vec(),
                text: format!("dc.w ${word:04X}"),
            });
            pos += 2;
        }
    }
    if pos < code.len() {
        out.push(Line {
            addr: origin.wrapping_add(pos as u32),
            bytes: vec![code[pos]],
            text: format!("dc.b ${:02X}", code[pos]),
        });
    }
    out
}

/// Render a 68000 disassembly as reassemblable vasm source (one per line).
#[must_use]
pub fn listing_68000(code: &[u8], origin: u32) -> String {
    let mut s = String::new();
    for line in disassemble_68000(code, origin) {
        s.push('\t');
        s.push_str(&line.text);
        s.push('\n');
    }
    s
}

fn be16(code: &[u8], pos: usize) -> u16 {
    u16::from(code[pos]) << 8 | u16::from(code[pos + 1])
}

/// The mask of bits that vary within an instruction word for this form (size
/// field plus each slot's register/quick/EA field). Bits outside the mask are
/// fixed and must equal `form.base`.
fn m68k_var_mask(form: &m68k::Form) -> u16 {
    let mut m = match form.size {
        SizeEnc::Fixed(_) => 0,
        SizeEnc::Std6 => 0b11 << 6,
        SizeEnc::Move => 0b11 << 12,
        SizeEnc::WL { shift } => 1 << shift,
    };
    for slot in form.operands {
        m |= match slot {
            Slot::Dn { shift } | Slot::An { shift } | Slot::Quick3 { shift } => 0b111 << shift,
            Slot::Quick8 | Slot::BranchW => 0xFF,
            Slot::Ea { shift, .. } => 0b11_1111 << shift,
            Slot::DispW | Slot::ImmWord | Slot::ImmSized | Slot::RegList => 0,
        };
    }
    m
}

/// Decode the size field of an instruction word, or `None` if the bits don't
/// name a valid size (which rules the form out — e.g. `11` in `Std6` marks an
/// `adda`/`cmpa`, not an `add`/`cmp`).
fn m68k_size(enc: SizeEnc, word: u16) -> Option<Size> {
    match enc {
        SizeEnc::Fixed(s) => Some(s),
        SizeEnc::Std6 => match (word >> 6) & 3 {
            0 => Some(Size::B),
            1 => Some(Size::W),
            2 => Some(Size::L),
            _ => None,
        },
        SizeEnc::Move => match (word >> 12) & 3 {
            1 => Some(Size::B),
            3 => Some(Size::W),
            2 => Some(Size::L),
            _ => None,
        },
        SizeEnc::WL { shift } => Some(if (word >> shift) & 1 == 1 {
            Size::L
        } else {
            Size::W
        }),
    }
}

/// The `ea::` mode bit for a decoded (mode, reg) effective address.
fn ea_bit(mode: u16, reg: u16) -> u16 {
    match mode {
        0 => ea::DN,
        1 => ea::AN,
        2 => ea::AI,
        3 => ea::PI,
        4 => ea::PD,
        5 => ea::DI,
        6 => ea::IX,
        7 => match reg {
            0 => ea::AW,
            1 => ea::AL,
            2 => ea::PCD,
            3 => ea::PCX,
            4 => ea::IMM,
            _ => 0,
        },
        _ => 0,
    }
}

/// Decode the one instruction at `pos`, returning its rendered text and byte
/// length. Among all forms whose fixed bits match and whose fields are valid,
/// the most specific (most fixed bits) wins — so e.g. `addi` beats `move`.
fn decode_m68k(code: &[u8], pos: usize, addr: u32) -> Option<(String, usize)> {
    let word = be16(code, pos);
    let mut best: Option<(&str, &m68k::Form, Size, u32)> = None;
    for insn in m68k::SET.instructions {
        for form in insn.forms {
            let mask = m68k_var_mask(form);
            if word & !mask != form.base {
                continue;
            }
            let Some(size) = m68k_size(form.size, word) else {
                continue;
            };
            // Reject the form if any EA slot decodes to a mode it doesn't allow,
            // or to An with a byte size (An is never a byte operand on the
            // 68000 — `move.b a0,d0` is illegal even though An is in the mask).
            let ea_ok = form.operands.iter().all(|slot| {
                let Slot::Ea { shift, modes, dest } = slot else {
                    return true;
                };
                let field = (word >> shift) & 0x3F;
                let (mode, reg) = if *dest {
                    (field & 7, (field >> 3) & 7)
                } else {
                    ((field >> 3) & 7, field & 7)
                };
                if mode == 1 && size == Size::B {
                    return false;
                }
                modes.allows(ea_bit(mode, reg))
            });
            if !ea_ok {
                continue;
            }
            let fixed = (!mask).count_ones();
            if best.is_none_or(|(_, _, _, b)| fixed > b) {
                best = Some((insn.mnemonic, form, size, fixed));
            }
        }
    }
    let (mnemonic, form, size, _) = best?;
    render_m68k(mnemonic, form, size, word, code, pos, addr)
}

/// Render a matched 68000 form, reading extension words as each slot needs.
/// Returns `None` if an extension word runs past the end of the code.
fn render_m68k(
    mnemonic: &str,
    form: &m68k::Form,
    size: Size,
    word: u16,
    code: &[u8],
    pos: usize,
    addr: u32,
) -> Option<(String, usize)> {
    let mut ext = pos + 2; // next extension word offset
    let mut ops: Vec<String> = Vec::new();
    let mut suffix = size_suffix(form.size, size);
    // The bit ops (btst/bchg/bclr/bset) take no size suffix: the width is
    // implied by the EA (.l on a data register, .b on memory), and vasm rejects
    // an explicit `.b` on a register. Sizeless round-trips for both.
    if matches!(
        mnemonic.to_ascii_lowercase().as_str(),
        "btst" | "bchg" | "bclr" | "bset"
    ) {
        suffix = String::new();
    }
    // Whether any EA in this instruction predecrements (movem mask is reversed).
    let predec = form.operands.iter().any(|slot| {
        matches!(slot, Slot::Ea { shift, dest, .. }
            if { let f = (word >> shift) & 0x3F;
                 let m = if *dest { f & 7 } else { (f >> 3) & 7 }; m == 4 })
    });
    // MOVEM's register-mask word always follows the opcode immediately, before
    // any EA extension — even in the load form where the reg list is the second
    // operand. Read it up front so the EA's displacement reads after it.
    let movem_mask = if form.operands.iter().any(|s| matches!(s, Slot::RegList)) {
        Some(read_be(code, &mut ext, 2)? as u16)
    } else {
        None
    };

    for slot in form.operands {
        match slot {
            Slot::Dn { shift } => ops.push(format!("d{}", (word >> shift) & 7)),
            Slot::An { shift } => ops.push(format!("a{}", (word >> shift) & 7)),
            Slot::Quick8 => ops.push(format!("#{}", (word & 0xFF) as i8)),
            Slot::Quick3 { shift } => {
                let v = (word >> shift) & 7;
                ops.push(format!("#{}", if v == 0 { 8 } else { v }));
            }
            Slot::ImmWord => {
                ops.push(format!("#{}", read_be(code, &mut ext, 2)?));
            }
            Slot::ImmSized => {
                let n = if matches!(size, Size::L) { 4 } else { 2 };
                ops.push(format!("#{}", read_be(code, &mut ext, n)?));
            }
            Slot::RegList => {
                let mask = movem_mask?;
                let mask = if predec { mask.reverse_bits() } else { mask };
                ops.push(render_reglist(mask));
            }
            Slot::BranchW => {
                // Short form carries the displacement in the opcode's low byte;
                // a zero low byte means the word form, with a 16-bit extension.
                let low = (word & 0xFF) as i8;
                let (disp, is_short) = if low != 0 {
                    (i64::from(low), true)
                } else {
                    (i64::from(read_be(code, &mut ext, 2)? as i16), false)
                };
                let target = addr.wrapping_add(2).wrapping_add(disp as u32);
                suffix = if is_short { ".s" } else { ".w" }.to_string();
                ops.push(format!("${target:X}"));
            }
            Slot::DispW => {
                let disp = i64::from(read_be(code, &mut ext, 2)? as i16);
                let target = addr.wrapping_add(2).wrapping_add(disp as u32);
                ops.push(format!("${target:X}"));
            }
            Slot::Ea { shift, dest, .. } => {
                let field = (word >> shift) & 0x3F;
                let (mode, reg) = if *dest {
                    (field & 7, (field >> 3) & 7)
                } else {
                    ((field >> 3) & 7, field & 7)
                };
                ops.push(render_ea(mode, reg, size, code, &mut ext, addr, pos)?);
            }
        }
    }

    let text = if ops.is_empty() {
        format!("{}{}", mnemonic.to_ascii_lowercase(), suffix)
    } else {
        format!(
            "{}{} {}",
            mnemonic.to_ascii_lowercase(),
            suffix,
            ops.join(",")
        )
    };
    Some((text, ext - pos))
}

/// The `.b`/`.w`/`.l` suffix to print for a form's size. A fixed word size is
/// implicit (lea, jsr, swap…) and prints nothing.
fn size_suffix(enc: SizeEnc, size: Size) -> String {
    match enc {
        SizeEnc::Fixed(Size::W) => String::new(),
        _ => match size {
            Size::B => ".b",
            Size::W => ".w",
            Size::L => ".l",
        }
        .to_string(),
    }
}

/// Read `n` (2 or 4) big-endian bytes at `*off` as a value, advancing `*off`.
fn read_be(code: &[u8], off: &mut usize, n: usize) -> Option<i64> {
    if *off + n > code.len() {
        return None;
    }
    let mut v: i64 = 0;
    for k in 0..n {
        v = (v << 8) | i64::from(code[*off + k]);
    }
    *off += n;
    Some(v)
}

/// Render an effective address from its decoded (mode, reg), reading any
/// extension words from `*ext`. `addr`/`pos` give the instruction's address and
/// code offset, so a PC-relative displacement can be rendered as its resolved
/// target (the address of the extension word it sits in, plus the displacement).
fn render_ea(
    mode: u16,
    reg: u16,
    size: Size,
    code: &[u8],
    ext: &mut usize,
    addr: u32,
    pos: usize,
) -> Option<String> {
    Some(match mode {
        0 => format!("d{reg}"),
        1 => format!("a{reg}"),
        2 => format!("(a{reg})"),
        3 => format!("(a{reg})+"),
        4 => format!("-(a{reg})"),
        5 => {
            let d = read_be(code, ext, 2)? as i16;
            format!("{d}(a{reg})")
        }
        6 => {
            let brief = read_be(code, ext, 2)? as u16;
            format!("{}(a{reg},{})", (brief & 0xFF) as i8, index_reg(brief))
        }
        7 => match reg {
            0 => {
                let v = read_be(code, ext, 2)? as i16;
                format!("${:X}.w", v as u16)
            }
            1 => {
                let v = read_be(code, ext, 4)? as u32;
                format!("${v:X}")
            }
            2 => {
                // PC-relative: render the resolved target. vasm reads `T(pc)` as
                // the address T and re-derives the displacement T − (ext-word
                // address), so a target round-trips where a raw displacement
                // would not.
                let ea = addr.wrapping_add((*ext - pos) as u32);
                let d = read_be(code, ext, 2)? as i16;
                let target = ea.wrapping_add(d as u32);
                format!("${target:X}(pc)")
            }
            3 => {
                let ea = addr.wrapping_add((*ext - pos) as u32);
                let brief = read_be(code, ext, 2)? as u16;
                let target = ea.wrapping_add(i32::from((brief & 0xFF) as i8) as u32);
                format!("${target:X}(pc,{})", index_reg(brief))
            }
            4 => {
                let n = if matches!(size, Size::L) { 4 } else { 2 };
                format!("#{}", read_be(code, ext, n)?)
            }
            _ => return None,
        },
        _ => return None,
    })
}

/// Decode a brief-extension-word index register into `d0.w`/`a3.l` syntax.
fn index_reg(brief: u16) -> String {
    let da = if brief & 0x8000 != 0 { 'a' } else { 'd' };
    let num = (brief >> 12) & 7;
    let sz = if brief & 0x0800 != 0 { 'l' } else { 'w' };
    format!("{da}{num}.{sz}")
}

/// Render a MOVEM register-list mask as `d0-d3/a0-a1` (consecutive runs joined).
fn render_reglist(mask: u16) -> String {
    let mut groups: Vec<String> = Vec::new();
    for (base, prefix) in [(0u16, 'd'), (8u16, 'a')] {
        let mut i = 0u16;
        while i < 8 {
            if mask & (1 << (base + i)) != 0 {
                let start = i;
                while i < 8 && mask & (1 << (base + i)) != 0 {
                    i += 1;
                }
                let end = i - 1;
                if start == end {
                    groups.push(format!("{prefix}{start}"));
                } else {
                    groups.push(format!("{prefix}{start}-{prefix}{end}"));
                }
            } else {
                i += 1;
            }
        }
    }
    groups.join("/")
}

// ---------------------------------------------------------------------------
// 6809 disassembly (byte-opcode + computed postbyte: the inverse of lwasm)
// ---------------------------------------------------------------------------

use isa::mos6809::{self, Kind};

/// Disassemble a flat 6809 big-endian binary loaded at `origin`, rendering
/// lwasm syntax. A byte matching no known opcode becomes an `fcb` datum.
#[must_use]
pub fn disassemble_6809(code: &[u8], origin: u16) -> Vec<Line> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < code.len() {
        let addr = origin.wrapping_add(pos as u16);
        if let Some((text, len)) = decode_6809(code, pos, addr) {
            out.push(Line {
                addr: u32::from(addr),
                bytes: code[pos..pos + len].to_vec(),
                text,
            });
            pos += len;
        } else {
            out.push(Line {
                addr: u32::from(addr),
                bytes: vec![code[pos]],
                text: format!("fcb ${:02X}", code[pos]),
            });
            pos += 1;
        }
    }
    out
}

/// Render a 6809 disassembly as reassemblable lwasm source (one per line).
#[must_use]
pub fn listing_6809(code: &[u8], origin: u16) -> String {
    let mut s = format!("        org ${origin:04X}\n");
    for line in disassemble_6809(code, origin) {
        s.push_str("        ");
        s.push_str(&line.text);
        s.push('\n');
    }
    s
}

/// Single-instruction counterpart of [`disassemble_6809`], in the callback shape
/// Emu198x consumes for `disasm` — see [`decode_one_6502`]. An undecodable byte
/// renders as data (`fcb $XX`, length 1).
#[must_use]
pub fn decode_one_6809(addr: u16, read: impl Fn(u16) -> u8) -> Option<(String, u8)> {
    // 5 bytes is the longest 6809 instruction (e.g. `$10 $AE $89 nn nn`).
    let buf: Vec<u8> = (0..5).map(|i| read(addr.wrapping_add(i))).collect();
    let line = disassemble_6809(&buf, addr).into_iter().next()?;
    Some((line.text, line.bytes.len() as u8))
}

/// Decode the one 6809 instruction at `pos`. Opcodes are unique per
/// (mnemonic, mode), so the first matching entry in the set is the decode; a
/// prefixed (`$10`/`$11`) opcode is two bytes and no one-byte opcode is a prefix
/// byte, so a full-slice match is unambiguous regardless of order.
fn decode_6809(code: &[u8], pos: usize, addr: u16) -> Option<(String, usize)> {
    let matches = |op: &[u8]| {
        !op.is_empty() && code.len() - pos >= op.len() && code[pos..pos + op.len()] == *op
    };
    for insn in mos6809::SET {
        let m = insn.mnemonic;
        match &insn.kind {
            Kind::Inherent(op) if matches(op) => return Some((m.to_string(), op.len())),
            Kind::Branch { short, long } => {
                if matches(short) {
                    let off = i16::from(*code.get(pos + short.len())? as i8);
                    let len = short.len() + 1;
                    let target = addr.wrapping_add(len as u16).wrapping_add(off as u16);
                    return Some((format!("{m} ${target:04X}"), len));
                }
                if matches(long) {
                    let off = be16_at(code, pos + long.len())? as i16;
                    let len = long.len() + 2;
                    let target = addr.wrapping_add(len as u16).wrapping_add(off as u16);
                    return Some((format!("l{m} ${target:04X}"), len));
                }
            }
            Kind::Mem {
                imm,
                direct,
                indexed,
                extended,
                width,
            } => {
                if matches(imm) {
                    let o = pos + imm.len();
                    let (val, n) = if *width == 2 {
                        (be16_at(code, o)?, 2)
                    } else {
                        (u16::from(*code.get(o)?), 1)
                    };
                    let lit = if n == 2 {
                        format!("#${val:04X}")
                    } else {
                        format!("#${val:02X}")
                    };
                    return Some((format!("{m} {lit}"), imm.len() + n));
                }
                if matches(direct) {
                    let v = *code.get(pos + direct.len())?;
                    return Some((format!("{m} ${v:02X}"), direct.len() + 1));
                }
                if matches(extended) {
                    let v = be16_at(code, pos + extended.len())?;
                    // Force `>` when the address would re-parse as a byte (direct).
                    let op = if v < 0x100 {
                        format!(">${v:04X}")
                    } else {
                        format!("${v:04X}")
                    };
                    return Some((format!("{m} {op}"), extended.len() + 2));
                }
                if matches(indexed) {
                    return decode_indexed_6809(m, indexed.len(), code, pos, addr);
                }
            }
            Kind::Transfer(op) if code.get(pos) == Some(op) => {
                let post = *code.get(pos + 1)?;
                let src = mos6809::transfer_reg_name(post >> 4)?;
                let dst = mos6809::transfer_reg_name(post & 0xF)?;
                return Some((format!("{m} {src},{dst}"), 2));
            }
            Kind::Stack { opcode, u_stack } if code.get(pos) == Some(opcode) => {
                let regs = render_stack_6809(*code.get(pos + 1)?, *u_stack);
                return Some((format!("{m} {regs}"), 2));
            }
            _ => {}
        }
    }
    None
}

/// Decode and render a 6809 indexed operand (postbyte + 0/1/2 extension bytes)
/// into lwasm syntax that re-encodes to the same bytes. Where the natural
/// rendering would re-assemble to a smaller form, a `<`/`>` size force is added.
fn decode_indexed_6809(
    m: &str,
    opcode_len: usize,
    code: &[u8],
    pos: usize,
    addr: u16,
) -> Option<(String, usize)> {
    let post = *code.get(pos + opcode_len)?;
    let mut len = opcode_len + 1;
    let reg = mos6809::index_reg_name(post >> 5);

    // bit 7 clear: a 5-bit signed offset embedded in the postbyte.
    if post & 0x80 == 0 {
        let off = ((post & 0x1F) as i8) << 3 >> 3; // sign-extend 5 bits
        return Some((format!("{m} {off},{reg}"), len));
    }

    let indirect = post & 0x10 != 0;
    let mut ext = String::new();
    let inner = match post & 0x0F {
        // Single auto inc/dec has no indirect form (`[,r+]`/`[,-r]` are invalid).
        0x0 if !indirect => format!(",{reg}+"),
        0x1 => format!(",{reg}++"),
        0x2 if !indirect => format!(",-{reg}"),
        0x3 => format!(",--{reg}"),
        0x4 => format!(",{reg}"),
        0x5 => format!("b,{reg}"),
        0x6 => format!("a,{reg}"),
        0xB => format!("d,{reg}"),
        0x8 => {
            let n = i16::from(*code.get(pos + len)? as i8);
            len += 1;
            // A small offset (5-bit range) would re-encode to 5-bit unless the
            // indirect form (no 5-bit) is in play; force 8-bit when not.
            let force = if !indirect && (-16..=15).contains(&n) {
                "<"
            } else {
                ""
            };
            format!("{force}{n},{reg}")
        }
        0x9 => {
            let v = be16_at(code, pos + len)?;
            len += 2;
            // A value ≤ 127 would re-encode 5-/8-bit; force 16-bit.
            let force = if v <= 127 { ">" } else { "" };
            format!("{force}${v:04X},{reg}")
        }
        0xC => {
            let off = i16::from(*code.get(pos + len)? as i8);
            len += 1;
            let target = addr.wrapping_add(len as u16).wrapping_add(off as u16);
            // 8-bit PCR must be forced (the assembler defaults PCR to 16-bit).
            format!("<${target:04X},pcr")
        }
        0xD => {
            let off = be16_at(code, pos + len)? as i16;
            len += 2;
            let target = addr.wrapping_add(len as u16).wrapping_add(off as u16);
            format!("${target:04X},pcr")
        }
        // Extended indirect `[addr]` is exactly $9F: the indirect bit set and
        // the register field zero. Any other $.F postbyte ($8F, $BF, …) is
        // reserved.
        0xF if indirect && post & 0x60 == 0 => {
            let v = be16_at(code, pos + len)?;
            len += 2;
            ext = format!("${v:04X}");
            String::new()
        }
        _ => return None,
    };
    let text = if post & 0x0F == 0xF {
        format!("[{ext}]")
    } else if indirect {
        format!("[{inner}]")
    } else {
        inner
    };
    Some((format!("{m} {text}"), len))
}

/// Render a push/pull register bitmask as a register list in bit order.
fn render_stack_6809(mask: u8, u_stack: bool) -> String {
    let names = mos6809::stack_regs(u_stack);
    let mut regs = Vec::new();
    for (i, name) in names.iter().enumerate() {
        if mask & (1 << i) != 0 {
            regs.push(*name);
        }
    }
    regs.join(",")
}

/// Read a big-endian 16-bit word at `o`, or `None` if it runs past the end.
fn be16_at(code: &[u8], o: usize) -> Option<u16> {
    Some(u16::from(*code.get(o)?) << 8 | u16::from(*code.get(o + 1)?))
}

// ---------------------------------------------------------------------------
// 65816 disassembly (6502 base + the extension, with m/x width tracking)
// ---------------------------------------------------------------------------

/// Disassemble a flat 65816 native-mode binary loaded at `origin`, rendering
/// ca65 syntax. Decodes against the 6502 set plus the 65816 extension.
///
/// The accumulator/index immediate width is not recoverable from the byte stream
/// alone, so this tracks it by interpreting `rep`/`sep` as it linearly decodes
/// (native reset state is 8-bit) and emits the matching `.a8`/`.a16`/`.i8`/`.i16`
/// directive whenever it changes — so the listing re-assembles byte-exact. Code
/// whose width is set out of band (no preceding `rep`/`sep`) is the inherent
/// limit of flat disassembly; it affects only the immediate-width instructions.
#[must_use]
pub fn disassemble_65816(code: &[u8], origin: u16) -> Vec<Line> {
    let mut out = Vec::new();
    let (mut a, mut i) = (1u8, 1u8); // immediate widths in bytes
    let mut pos = 0;
    while pos < code.len() {
        let addr = origin.wrapping_add(pos as u16);
        if let Some((mn, mode, vals, len)) = decode_65816(code, pos, addr, a, i) {
            out.push(Line {
                addr: u32::from(addr),
                bytes: code[pos..pos + len].to_vec(),
                text: render_816(mn, mode, &vals, addr, len),
            });
            // rep clears status bits (→ 16-bit), sep sets them (→ 8-bit);
            // bit 5 (0x20) is the accumulator width, bit 4 (0x10) the index.
            if (mn == "REP" || mn == "SEP") && !vals.is_empty() {
                let to = if mn == "REP" { 2 } else { 1 };
                if vals[0] & 0x20 != 0 && a != to {
                    a = to;
                    out.push(directive_816(addr, if to == 2 { ".a16" } else { ".a8" }));
                }
                if vals[0] & 0x10 != 0 && i != to {
                    i = to;
                    out.push(directive_816(addr, if to == 2 { ".i16" } else { ".i8" }));
                }
            }
            pos += len;
        } else {
            out.push(Line {
                addr: u32::from(addr),
                bytes: vec![code[pos]],
                text: format!(".byte ${:02X}", code[pos]),
            });
            pos += 1;
        }
    }
    out
}

/// Render a 65816 disassembly as reassemblable ca65 source.
#[must_use]
pub fn listing_65816(code: &[u8], origin: u16) -> String {
    let mut s = format!("        .org ${origin:04X}\n");
    for line in disassemble_65816(code, origin) {
        s.push_str("        ");
        s.push_str(&line.text);
        s.push('\n');
    }
    s
}

/// A width-state directive line — assembler state, so it carries no bytes.
fn directive_816(addr: u16, text: &str) -> Line {
    Line {
        addr: u32::from(addr),
        bytes: Vec::new(),
        text: text.to_string(),
    }
}

/// Decode one 65816 instruction at `pos` given the current immediate widths.
/// Opcodes are single bytes; the only ambiguity is the width-variable immediate,
/// resolved by `a_width`/`i_width`.
fn decode_65816(
    code: &[u8],
    pos: usize,
    _addr: u16,
    a_width: u8,
    i_width: u8,
) -> Option<(&'static str, &'static str, Vec<i64>, usize)> {
    let b = *code.get(pos)?;
    // Candidate forms for this opcode, across the 6502 set and the extension.
    let mut cands: Vec<(&'static str, &'static isa::Form)> = Vec::new();
    for set in [&isa::mos6502::SET, &isa::mos65816::SET] {
        for insn in set.instructions {
            for form in insn.forms {
                if form.opcode == [b] {
                    cands.push((insn.mnemonic, form));
                }
            }
        }
    }
    // More than one candidate means an immediate pair (same mnemonic): pick the
    // 8- or 16-bit form by the relevant width.
    let (mn, form) = if cands.len() == 1 {
        cands[0]
    } else {
        let want = if matches!(cands[0].0, "LDX" | "LDY" | "CPX" | "CPY") {
            i_width
        } else {
            a_width
        };
        let want_mode = if want == 2 {
            "immediate16"
        } else {
            "immediate"
        };
        *cands.iter().find(|(_, f)| f.mode == want_mode)?
    };

    // Read the operand value(s), little-endian; sign-extend PC-relative ones.
    let mut vals = Vec::new();
    let mut off = pos + 1;
    for operand in form.operands {
        let n = operand.bytes as usize;
        let mut v: i64 = 0;
        for k in 0..n {
            v |= i64::from(*code.get(off + k)?) << (8 * k);
        }
        if matches!(operand.kind, isa::OperandKind::RelativePc) {
            let bits = n * 8;
            if v & (1 << (bits - 1)) != 0 {
                v -= 1 << bits;
            }
        }
        vals.push(v);
        off += n;
    }
    Some((mn, form.mode, vals, off - pos))
}

/// Render a decoded 65816 instruction to ca65 syntax. Where the natural operand
/// would re-assemble to a smaller addressing mode, an `a:`/`f:` size force pins
/// it (the ca65 analogue of the 6809 `<`/`>` forces).
fn render_816(mn: &str, mode: &str, vals: &[i64], addr: u16, len: usize) -> String {
    let m = mn.to_ascii_lowercase();
    let v = vals.first().copied().unwrap_or(0);
    let force_abs = if v < 0x100 { "a:" } else { "" };
    let force_long = if v < 0x1_0000 { "f:" } else { "" };
    let operand = match mode {
        "implied" => String::new(),
        "accumulator" => "a".to_string(),
        "immediate" => format!("#${:02X}", v & 0xFF),
        "immediate16" => format!("#${:04X}", v & 0xFFFF),
        "signature" => format!("${:02X}", v & 0xFF),
        "zeropage" => format!("${v:02X}"),
        "zeropage,x" => format!("${v:02X},x"),
        "zeropage,y" => format!("${v:02X},y"),
        "absolute" => format!("{force_abs}${v:04X}"),
        "absolute,x" => format!("{force_abs}${v:04X},x"),
        "absolute,y" => format!("{force_abs}${v:04X},y"),
        "long" => format!("{force_long}${v:06X}"),
        "long,x" => format!("{force_long}${v:06X},x"),
        "(indirect)" => format!("(${v:02X})"),
        "(indirect,x)" => format!("(${v:02X},x)"),
        "(indirect),y" => format!("(${v:02X}),y"),
        "[indirect]" => format!("[${v:02X}]"),
        "[indirect],y" => format!("[${v:02X}],y"),
        "stack,s" => format!("${v:02X},s"),
        "(stack,s),y" => format!("(${v:02X},s),y"),
        "indirect" => format!("(${v:04X})"),
        "[absolute]" => format!("[${v:04X}]"),
        "(absolute,x)" => format!("(${v:04X},x)"),
        "relative" => {
            let target = addr.wrapping_add(len as u16).wrapping_add(v as u16);
            format!("${target:04X}")
        }
        "relative16" => {
            let target = addr.wrapping_add(len as u16).wrapping_add(v as u16);
            format!("${target:04X}")
        }
        // mvn/mvp operands are [dest, src]; the source is written first.
        "block-move" => format!(
            "#${:02X},#${:02X}",
            vals.get(1).copied().unwrap_or(0) & 0xFF,
            v & 0xFF
        ),
        other => other.to_string(),
    };
    if operand.is_empty() {
        m
    } else {
        format!("{m} {operand}")
    }
}

// Round-trip tests (assemble → disassemble → reassemble) live in the `asm198x`
// crate, which has the assembler; here we test decode + render in isolation.
#[cfg(test)]
mod tests {
    use super::*;

    fn one(bytes: &[u8]) -> String {
        let lines = disassemble_z80(bytes, 0x8000, false);
        assert_eq!(lines.len(), 1, "expected one instruction, got {lines:?}");
        lines[0].text.clone()
    }

    #[test]
    fn decodes_representative_opcodes() {
        assert_eq!(one(&[0x00]), "NOP");
        assert_eq!(one(&[0x3E, 0x42]), "LD A,$42");
        assert_eq!(one(&[0x21, 0x00, 0x58]), "LD HL,$5800");
        assert_eq!(one(&[0xC3, 0x34, 0x12]), "JP $1234");
        assert_eq!(one(&[0xED, 0xB0]), "LDIR");
        assert_eq!(one(&[0xCB, 0x46]), "BIT 0,(HL)");
        assert_eq!(one(&[0xDD, 0x7E, 0x05]), "LD A,(IX+$05)");
        assert_eq!(one(&[0xDD, 0x36, 0x05, 0x0A]), "LD (IX+$05),$0A");
        assert_eq!(one(&[0xFD, 0xCB, 0xFF, 0x7E]), "BIT 7,(IY-$01)");
        assert_eq!(one(&[0xFF]), "RST $38");
    }

    #[test]
    fn relative_branch_targets_are_absolute() {
        // JR at $8000, length 2, offset +5 -> target $8007.
        assert_eq!(one(&[0x18, 0x05]), "JR $8007");
    }

    fn one_6502(bytes: &[u8]) -> String {
        let lines = disassemble_6502(bytes, 0x0800);
        assert_eq!(lines.len(), 1, "expected one instruction, got {lines:?}");
        lines[0].text.clone()
    }

    #[test]
    fn decodes_6502_addressing_modes() {
        assert_eq!(one_6502(&[0xEA]), "NOP");
        // Accumulator mode renders bare (acme rejects `ASL A`).
        assert_eq!(one_6502(&[0x0A]), "ASL");
        assert_eq!(one_6502(&[0xA9, 0x42]), "LDA #$42");
        assert_eq!(one_6502(&[0xA5, 0x10]), "LDA $10");
        assert_eq!(one_6502(&[0xB5, 0x10]), "LDA $10,X");
        assert_eq!(one_6502(&[0xAD, 0x34, 0x12]), "LDA $1234");
        assert_eq!(one_6502(&[0x9D, 0x00, 0x04]), "STA $0400,X");
        assert_eq!(one_6502(&[0x99, 0x00, 0x04]), "STA $0400,Y");
        assert_eq!(one_6502(&[0x6C, 0x34, 0x12]), "JMP ($1234)");
        assert_eq!(one_6502(&[0xA1, 0x20]), "LDA ($20,X)");
        assert_eq!(one_6502(&[0xB1, 0x20]), "LDA ($20),Y");
    }

    #[test]
    fn relative_branch_target_is_absolute_6502() {
        // BNE at $0800, length 2, offset -2 ($FE) -> target $0800.
        assert_eq!(one_6502(&[0xD0, 0xFE]), "BNE $0800");
    }

    #[test]
    fn unknown_byte_becomes_datum_6502() {
        // $02 is not an official opcode.
        assert_eq!(one_6502(&[0x02]), "!byte $02");
    }

    #[test]
    fn decode_one_6502_reads_a_single_instruction() {
        // LDA #$12 then NOP; the callback form returns only the first, with its
        // byte length, reading through the closure (a machine's memory peek).
        let mem = [0xA9u8, 0x12, 0xEA];
        let got = decode_one_6502(0x0800, |a| mem[(a - 0x0800) as usize]);
        assert_eq!(got, Some(("LDA #$12".to_string(), 2)));
    }

    #[test]
    fn decode_one_6809_reads_a_single_instruction() {
        // NOP ($12) is one byte; the trailing bytes are ignored.
        let mem = [0x12u8, 0x39, 0x00, 0x00, 0x00];
        let got = decode_one_6809(0x1000, |a| mem[(a - 0x1000) as usize]);
        assert_eq!(got, Some(("nop".to_string(), 1)));
    }

    fn one_m68k(bytes: &[u8]) -> String {
        let lines = disassemble_68000(bytes, 0x1000);
        assert_eq!(lines.len(), 1, "expected one instruction, got {lines:?}");
        lines[0].text.clone()
    }

    #[test]
    fn decodes_m68k_representative_opcodes() {
        assert_eq!(one_m68k(&[0x4E, 0x75]), "rts");
        assert_eq!(one_m68k(&[0x70, 0x00]), "moveq.l #0,d0");
        assert_eq!(one_m68k(&[0x52, 0x40]), "addq.w #1,d0");
        assert_eq!(one_m68k(&[0xD0, 0x41]), "add.w d1,d0");
        // lea $dff000,a5 (abs.l)
        assert_eq!(
            one_m68k(&[0x4B, 0xF9, 0x00, 0xDF, 0xF0, 0x00]),
            "lea.l $DFF000,a5"
        );
        // move.w #$7fff,$09a(a5) -> d16(a5)
        assert_eq!(
            one_m68k(&[0x3B, 0x7C, 0x7F, 0xFF, 0x00, 0x9A]),
            "move.w #32767,154(a5)"
        );
        // movem.l d0-d3/a0-a1,-(a7)
        assert_eq!(
            one_m68k(&[0x48, 0xE7, 0xF0, 0xC0]),
            "movem.l d0-d3/a0-a1,-(a7)"
        );
        // indexed: tst.w 0(a0,d1.w)
        assert_eq!(one_m68k(&[0x4A, 0x70, 0x10, 0x00]), "tst.w 0(a0,d1.w)");
    }

    #[test]
    fn m68k_short_branch_target_is_absolute() {
        // bne.s at $1000, length 2, disp -8 ($F8) -> target $FFA.
        assert_eq!(one_m68k(&[0x66, 0xF8]), "bne.s $FFA");
    }

    #[test]
    fn m68k_immediate_ops_are_distinct_mnemonics() {
        // $06/$04/$0C are addi/subi/cmpi, not add/sub/cmp.
        assert_eq!(one_m68k(&[0x06, 0x00, 0x00, 0x10]), "addi.b #16,d0");
        assert_eq!(one_m68k(&[0x04, 0x00, 0x00, 0x10]), "subi.b #16,d0");
        assert_eq!(one_m68k(&[0x0C, 0x00, 0x00, 0x10]), "cmpi.b #16,d0");
        // $00/$02/$0A are the bitwise counterparts ori/andi/eori. These were
        // absent from the spec — a missing instruction the form-audit, the
        // sweep, and the curriculum were all structurally blind to (only a
        // cross-check against an independent decoder surfaced it).
        assert_eq!(one_m68k(&[0x00, 0x00, 0x00, 0x10]), "ori.b #16,d0");
        assert_eq!(one_m68k(&[0x02, 0x40, 0x12, 0x34]), "andi.w #4660,d0");
        assert_eq!(
            one_m68k(&[0x0A, 0x80, 0x12, 0x34, 0x56, 0x78]),
            "eori.l #305419896,d0"
        );
    }

    #[test]
    fn m68k_bit_ops_are_sizeless() {
        // btst d0,d0 ($0100) is `.l` on a register — rendered sizeless (vasm
        // rejects `btst.b` on a register).
        assert_eq!(one_m68k(&[0x01, 0x00]), "btst d0,d0");
    }

    #[test]
    fn m68k_control_flow() {
        // 68000-completeness family 1: JMP/JSR (control-addressing EA) and the
        // no-operand returns. All unsized — no size suffix.
        assert_eq!(one_m68k(&[0x4E, 0xD0]), "jmp (a0)");
        assert_eq!(one_m68k(&[0x4E, 0x90]), "jsr (a0)");
        assert_eq!(one_m68k(&[0x4E, 0x73]), "rte");
        assert_eq!(one_m68k(&[0x4E, 0x77]), "rtr");
        assert_eq!(one_m68k(&[0x4E, 0x76]), "trapv");
        assert_eq!(one_m68k(&[0x4E, 0x70]), "reset");
        assert_eq!(one_m68k(&[0x4A, 0xFC]), "illegal");
    }

    #[test]
    fn m68k_mirror_families() {
        // Families mirroring existing entries: MULS/DIVS (Fixed(W) -> suffixless,
        // like MULU/DIVU), shifts/rotates (Std6 -> sized), BCHG/BCLR (bit ops
        // render sizeless). Encodings confirmed byte-identical against vasm.
        assert_eq!(one_m68k(&[0xC3, 0xC0]), "muls d0,d1");
        assert_eq!(one_m68k(&[0x83, 0xC0]), "divs d0,d1");
        assert_eq!(one_m68k(&[0xE2, 0x40]), "asr.w #1,d0");
        assert_eq!(one_m68k(&[0xE2, 0x60]), "asr.w d1,d0");
        assert_eq!(one_m68k(&[0xE7, 0x82]), "asl.l #3,d2");
        assert_eq!(one_m68k(&[0xE2, 0x50]), "roxr.w #1,d0");
        assert_eq!(one_m68k(&[0xE3, 0xB8]), "rol.l d1,d0");
        assert_eq!(one_m68k(&[0x08, 0x50, 0x00, 0x05]), "bchg #5,(a0)");
        assert_eq!(one_m68k(&[0x01, 0x41]), "bchg d0,d1");
        assert_eq!(one_m68k(&[0x01, 0x90]), "bclr d0,(a0)");
    }

    #[test]
    fn m68k_condition_codes() {
        // Condition-code breadth (family 6): Bcc (short form here, target
        // resolved against origin $1000), Scc (byte EA), DBcc (Dn + word disp).
        assert_eq!(one_m68k(&[0x62, 0x10]), "bhi.s $1012");
        assert_eq!(one_m68k(&[0x65, 0x10]), "bcs.s $1012");
        assert_eq!(one_m68k(&[0x69, 0x10]), "bvs.s $1012");
        assert_eq!(one_m68k(&[0x52, 0xC0]), "shi.b d0");
        assert_eq!(one_m68k(&[0x5F, 0xC2]), "sle.b d2");
        assert_eq!(one_m68k(&[0x52, 0xC8, 0x00, 0x10]), "dbhi d0,$1012");
        assert_eq!(one_m68k(&[0x5F, 0xC8, 0x00, 0x10]), "dble d0,$1012");
    }

    #[test]
    fn m68k_movem_load_reads_mask_before_displacement() {
        // movem.w 16(a0),d5: mask word ($0020 = d5) comes first, then the EA
        // displacement ($0010 = 16) — not in operand-display order.
        assert_eq!(
            one_m68k(&[0x4C, 0xA8, 0x00, 0x20, 0x00, 0x10]),
            "movem.w 16(a0),d5"
        );
    }

    #[test]
    fn m68k_pc_relative_renders_resolved_target() {
        // move.w $10(pc),d0 at $1000: ext word at $1002, disp $000E -> target
        // $1010. Rendered as the target so it round-trips (vasm re-derives the
        // displacement from it).
        assert_eq!(one_m68k(&[0x30, 0x3A, 0x00, 0x0E]), "move.w $1010(pc),d0");
    }

    #[test]
    fn m68k_rejects_byte_on_address_register() {
        // move.b a0,d0 ($1008) is illegal (An is never a byte operand): decoded
        // as data, not a bogus instruction.
        assert_eq!(one_m68k(&[0x10, 0x08]), "dc.w $1008");
    }

    fn one_6809(bytes: &[u8]) -> String {
        let lines = disassemble_6809(bytes, 0x1000);
        assert_eq!(lines.len(), 1, "expected one instruction, got {lines:?}");
        lines[0].text.clone()
    }

    #[test]
    fn decodes_6809_modes() {
        assert_eq!(one_6809(&[0x39]), "rts");
        assert_eq!(one_6809(&[0x86, 0x42]), "lda #$42");
        assert_eq!(one_6809(&[0x8E, 0x12, 0x34]), "ldx #$1234");
        assert_eq!(one_6809(&[0x96, 0x20]), "lda $20");
        assert_eq!(one_6809(&[0xB6, 0x12, 0x34]), "lda $1234");
        // Extended low address forces `>` so it doesn't collapse to direct.
        assert_eq!(one_6809(&[0xB6, 0x00, 0x20]), "lda >$0020");
    }

    #[test]
    fn decodes_6809_indexed() {
        assert_eq!(one_6809(&[0xA6, 0x84]), "lda ,x");
        assert_eq!(one_6809(&[0xA6, 0x05]), "lda 5,x");
        assert_eq!(one_6809(&[0xA6, 0x10]), "lda -16,x"); // 5-bit -16
        assert_eq!(one_6809(&[0xA6, 0xA4]), "lda ,y");
        assert_eq!(one_6809(&[0xA6, 0x80]), "lda ,x+");
        assert_eq!(one_6809(&[0xA6, 0x81]), "lda ,x++");
        assert_eq!(one_6809(&[0xA6, 0x83]), "lda ,--x");
        assert_eq!(one_6809(&[0xA6, 0x86]), "lda a,x");
        assert_eq!(one_6809(&[0xA6, 0x8B]), "lda d,x");
        // 8-bit offset within 5-bit range forces `<`; outside it does not.
        assert_eq!(one_6809(&[0xA6, 0x88, 0x05]), "lda <5,x");
        assert_eq!(one_6809(&[0xA6, 0x88, 0x64]), "lda 100,x");
        // 16-bit offset ≤127 forces `>`; a real address does not.
        assert_eq!(one_6809(&[0xA6, 0x89, 0x12, 0x34]), "lda $1234,x");
        // Indirect (no 5-bit form) and extended indirect.
        assert_eq!(one_6809(&[0xA6, 0x94]), "lda [,x]");
        assert_eq!(one_6809(&[0xA6, 0x98, 0x05]), "lda [5,x]");
        assert_eq!(one_6809(&[0xA6, 0x9F, 0x20, 0x00]), "lda [$2000]");
    }

    #[test]
    fn decodes_6809_registers_and_branches() {
        assert_eq!(one_6809(&[0x1F, 0x89]), "tfr a,b");
        assert_eq!(one_6809(&[0x1E, 0x10]), "exg x,d");
        assert_eq!(one_6809(&[0x34, 0x16]), "pshs a,b,x");
        assert_eq!(one_6809(&[0x36, 0x46]), "pshu a,b,s");
        // bra at $1000 len 2, offset -2 -> $1000.
        assert_eq!(one_6809(&[0x20, 0xFE]), "bra $1000");
        // lbra at $1000 len 3, offset -3 -> $1000.
        assert_eq!(one_6809(&[0x16, 0xFF, 0xFD]), "lbra $1000");
        // conditional long branch is $10-prefixed.
        assert_eq!(one_6809(&[0x10, 0x27, 0xFF, 0xFC]), "lbeq $1000");
    }

    fn lines_65816(bytes: &[u8]) -> Vec<String> {
        disassemble_65816(bytes, 0x1000)
            .into_iter()
            .map(|l| l.text)
            .collect()
    }

    #[test]
    fn decodes_65816_modes() {
        // 8-bit by default; the new addressing modes render in ca65 syntax.
        assert_eq!(lines_65816(&[0xA9, 0x42]), vec!["lda #$42"]);
        assert_eq!(lines_65816(&[0xAF, 0x56, 0x34, 0x12]), vec!["lda $123456"]);
        assert_eq!(lines_65816(&[0xA7, 0x12]), vec!["lda [$12]"]);
        assert_eq!(lines_65816(&[0xA3, 0x03]), vec!["lda $03,s"]);
        assert_eq!(lines_65816(&[0x22, 0x56, 0x34, 0x12]), vec!["jsl $123456"]);
        // A low long/abs value is force-sized so it can't shrink on re-assembly.
        assert_eq!(lines_65816(&[0xAD, 0x12, 0x00]), vec!["lda a:$0012"]);
        assert_eq!(
            lines_65816(&[0xAF, 0x12, 0x00, 0x00]),
            vec!["lda f:$000012"]
        );
        // block move renders source-first; cop/wdm a bare byte.
        assert_eq!(lines_65816(&[0x54, 0x7F, 0x7E]), vec!["mvn #$7E,#$7F"]);
        assert_eq!(lines_65816(&[0x02, 0x12]), vec!["cop $12"]);
    }

    #[test]
    fn tracks_mx_width_and_emits_directives() {
        // rep #$30 widens both; the following immediate is then 16-bit.
        let lines = lines_65816(&[0xC2, 0x30, 0xA9, 0x34, 0x12]);
        assert_eq!(lines, vec!["rep #$30", ".a16", ".i16", "lda #$1234"]);
        // sep #$20 narrows the accumulator; a 16-bit X immediate stays 16-bit.
        let lines = lines_65816(&[0xC2, 0x30, 0xE2, 0x20, 0xA9, 0x42, 0xA2, 0x34, 0x12]);
        assert_eq!(
            lines,
            vec![
                "rep #$30",
                ".a16",
                ".i16",
                "sep #$20",
                ".a8",
                "lda #$42",
                "ldx #$1234"
            ]
        );
    }
}
