//! Spec-driven disassembly.
//!
//! The decoder walks the **same** [`isa`] spec the assembler emits from, so the
//! two are guaranteed consistent: [`decode_one`] matches opcode bytes against
//! the instruction set and reads the operand bytes, with no hand-written decode
//! tables. This is the round-trip the authored-spec architecture was justified
//! by — assemble → disassemble → reassemble reproduces the bytes (see the
//! umbrella `asm198x-and-shared-isa-spec.md`).
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

/// Render a matched 6502 form by mapping its addressing-mode label to operand
/// syntax. One-byte operands print as `$XX`, two-byte as `$XXXX`; a relative
/// branch prints its absolute target.
fn render_6502(mnemonic: &str, form: &isa::Form, values: &[i64], addr: u16, len: usize) -> String {
    let v = values.first().copied().unwrap_or(0);
    let lo = (v & 0xFF) as u8;
    let word = (v & 0xFFFF) as u16;
    let operand = match form.mode {
        "implied" => String::new(),
        "accumulator" => "A".to_string(),
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
            // Reject the form if any EA slot decodes to a mode it doesn't allow.
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
    // Whether any EA in this instruction predecrements (movem mask is reversed).
    let predec = form.operands.iter().any(|slot| {
        matches!(slot, Slot::Ea { shift, dest, .. }
            if { let f = (word >> shift) & 0x3F;
                 let m = if *dest { f & 7 } else { (f >> 3) & 7 }; m == 4 })
    });

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
                if ext + 2 > code.len() {
                    return None;
                }
                let mask = be16(code, ext);
                ext += 2;
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
                ops.push(render_ea(mode, reg, size, code, &mut ext)?);
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
/// extension words from `*ext`.
fn render_ea(mode: u16, reg: u16, size: Size, code: &[u8], ext: &mut usize) -> Option<String> {
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
                // PC-relative: print the raw displacement so it re-encodes as a
                // literal `d(pc)` (the assembler stores it directly).
                let d = read_be(code, ext, 2)? as i16;
                format!("{d}(pc)")
            }
            3 => {
                let brief = read_be(code, ext, 2)? as u16;
                format!("{}(pc,{})", (brief & 0xFF) as i8, index_reg(brief))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assemble_pasmonext;

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

    /// The architectural payoff: every byte sequence the assembler emits
    /// disassembles to text that reassembles to the identical bytes.
    #[test]
    fn round_trips_through_the_assembler() {
        let source = "\
            org $8000\n\
            ld hl, $5800\n\
            ld a, $07\n\
            ld (hl), a\n\
            ldir\n\
            bit 7, (ix+5)\n\
            set 0, (iy-1)\n\
            add a, (ix+3)\n\
            ld (ix+2), $ff\n\
            jr $8000\n\
            ret\n";
        let original = assemble_pasmonext(source).expect("assemble");
        let listing = listing_z80(&original.bytes, original.origin, true);
        let reassembled = assemble_pasmonext(&listing).expect("reassemble");
        assert_eq!(reassembled.bytes, original.bytes, "listing was:\n{listing}");
    }

    fn one_6502(bytes: &[u8]) -> String {
        let lines = disassemble_6502(bytes, 0x0800);
        assert_eq!(lines.len(), 1, "expected one instruction, got {lines:?}");
        lines[0].text.clone()
    }

    #[test]
    fn decodes_6502_addressing_modes() {
        assert_eq!(one_6502(&[0xEA]), "NOP");
        assert_eq!(one_6502(&[0x0A]), "ASL A");
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

    /// Assembler output disassembles to text that reassembles to identical bytes.
    #[test]
    fn round_trips_through_acme() {
        let source = "\
            *= $0800\n\
            start:  lda #$00\n\
                    ldx #$08\n\
            loop:   sta $0400,x\n\
                    lda $10\n\
                    sta $d020\n\
                    lda ($20),y\n\
                    lda ($20,x)\n\
                    jmp ($1234)\n\
                    asl a\n\
                    dex\n\
                    bne loop\n\
                    rts\n";
        let original = crate::assemble_acme(source).expect("assemble");
        let listing = listing_6502(&original.bytes, original.origin);
        let reassembled = crate::assemble_acme(&listing).expect("reassemble");
        assert_eq!(reassembled.bytes, original.bytes, "listing was:\n{listing}");
    }

    #[test]
    fn round_trips_low_address_absolute() {
        // A low-address absolute (e.g. from data misread as code) must survive:
        // the disassembler emits 4-digit `$XXXX`, and acme's width rule keeps it
        // 16-bit on reassembly rather than collapsing to zero-page.
        let bytes = vec![0x9D, 0x00, 0x00, 0xAD, 0x10, 0x00, 0x60];
        let listing = listing_6502(&bytes, 0x0800);
        let re = crate::assemble_acme(&listing).expect("reassemble");
        assert_eq!(re.bytes, bytes, "listing:\n{listing}");
    }

    #[test]
    fn round_trips_z80n_opcodes() {
        let source = "\
            org $8000\n\
            swapnib\n\
            mul\n\
            add hl, a\n\
            add hl, $1234\n\
            nextreg $07, $02\n\
            push $abcd\n\
            ldirx\n";
        let original = assemble_pasmonext(source).expect("assemble");
        let listing = listing_z80(&original.bytes, original.origin, true);
        let reassembled = assemble_pasmonext(&listing).expect("reassemble");
        assert_eq!(reassembled.bytes, original.bytes, "listing was:\n{listing}");
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
    fn round_trips_m68k_pure_code() {
        // Pure code (no interleaved data) round-trips through the optimizing
        // assembler: the disassembly's explicit forms are optimizer-stable.
        let source = "\
            \tlea\t$dff000,a5\n\
            \tmove.l\t(a5),d0\n\
            \tand.l\td1,d0\n\
            loop:\n\
            \taddq.w\t#1,d0\n\
            \tcmp.w\t#100,d0\n\
            \tbne.s\tloop\n\
            \tmovem.l\td0-d3/a0-a1,-(sp)\n\
            \trts\n";
        let original = crate::assemble_vasm(source).expect("assemble");
        let listing = listing_68000(&original, 0);
        let reassembled = crate::assemble_vasm(&listing).expect("reassemble");
        assert_eq!(reassembled, original, "listing was:\n{listing}");
    }

    /// The whole optimized Amiga curriculum round-trips byte-exact when the
    /// disassembly is reassembled with the optimizer off — the listing captures
    /// each instruction's *encoded* form explicitly, so `-no-opt` reproduces it.
    /// (Reassembling with the optimizer on cannot be byte-exact for the data
    /// interleaved in the code stream: a data word that happens to decode as,
    /// say, `add #2,d0` would be re-optimized to `addq`.)
    #[test]
    fn round_trips_m68k_flat_curriculum() {
        // A representative flat (single-section) snippet exercising the
        // optimized forms the curriculum produces.
        let source = "\
            \tlea\tdata,a0\n\
            \tmove.l\t#data,d0\n\
            \tlea\t8(a0),a0\n\
            \tadd.l\t#$400,a1\n\
            \tcmp.w\t#0,d2\n\
            \tbne.s\tdata\n\
            data:\n\
            \tdc.w\t$0180,$0000\n\
            \tdc.l\t$deadbeef\n";
        let original = crate::dialects::vasm::assemble_with(source, true).expect("assemble");
        let listing = listing_68000(&original, 0);
        let re = crate::dialects::vasm::assemble_with(&listing, false).expect("reassemble");
        assert_eq!(re, original, "listing was:\n{listing}");
    }
}
