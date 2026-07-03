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
            // A big-endian 16-bit immediate (Z80N `push nn`): high byte first,
            // regardless of the set's little-endian default.
            isa::OperandKind::ImmediateBe => {
                let (hi, lo) = (rest[off], rest[off + 1]);
                values.push(i64::from(u16::from(lo) | (u16::from(hi) << 8)));
                off += 2;
            }
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
            Slot::Dn { shift }
            | Slot::An { shift }
            | Slot::Quick3 { shift }
            | Slot::AddrIndirect { shift, .. } => 0b111 << shift,
            // MOVEP's Ay sits in bits 0–2 (mode marker 001 is fixed in the base).
            Slot::MovepDisp => 0b111,
            Slot::Quick8 | Slot::BranchW => 0xFF,
            Slot::Vec4 => 0xF,
            Slot::Ea { shift, .. } => 0b11_1111 << shift,
            Slot::DispW
            | Slot::ImmWord
            | Slot::ImmSized
            | Slot::RegList
            | Slot::Ccr
            | Slot::Sr
            | Slot::Usp => 0,
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
            // A byte-sized immediate (addi.b/ori.b/… #imm) rides in the low byte
            // of its extension word; the high byte is a fixed-zero field. A
            // non-zero high byte is not an encoding the reference tool emits, so
            // the form doesn't match — the bytes are data. In every such form
            // (`[ImmSized, <ea>]`) the immediate is the first extension word.
            let imm_ok = size != Size::B
                || !matches!(form.operands.first(), Some(Slot::ImmSized))
                || code.get(pos + 2) == Some(&0);
            if !imm_ok {
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
            Slot::AddrIndirect { shift, mode } => {
                let reg = (word >> shift) & 7;
                ops.push(if *mode == 4 {
                    format!("-(a{reg})")
                } else {
                    format!("(a{reg})+")
                });
            }
            Slot::Quick8 => ops.push(format!("#{}", (word & 0xFF) as i8)),
            Slot::Vec4 => ops.push(format!("#{}", word & 0xF)),
            Slot::Ccr => ops.push("ccr".to_string()),
            Slot::Sr => ops.push("sr".to_string()),
            Slot::Usp => ops.push("usp".to_string()),
            Slot::MovepDisp => {
                let reg = word & 7;
                let disp = read_be(code, &mut ext, 2)? as i16;
                ops.push(format!("{disp}(a{reg})"));
            }
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

// ---------------------------------------------------------------------------
// HuC6280 disassembly (6502 base + the HuC6280 extension)
// ---------------------------------------------------------------------------

/// Disassemble a flat HuC6280 (PC Engine) binary loaded at `origin`, rendering
/// ca65 syntax. Decodes against the 6502 set plus the HuC6280 extension, the
/// extension winning any opcode that both define. Unlike the 65816 there is no
/// width-variable immediate, so every opcode decodes to one fixed form.
#[must_use]
pub fn disassemble_huc6280(code: &[u8], origin: u16) -> Vec<Line> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < code.len() {
        let addr = origin.wrapping_add(pos as u16);
        if let Some((mn, mode, vals, len)) = decode_huc6280(code, pos) {
            out.push(Line {
                addr: u32::from(addr),
                bytes: code[pos..pos + len].to_vec(),
                text: render_huc6280(mn, mode, &vals, addr, len),
            });
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

/// Render a HuC6280 disassembly as reassemblable ca65 source.
#[must_use]
pub fn listing_huc6280(code: &[u8], origin: u16) -> String {
    let mut s = format!("        .org ${origin:04X}\n");
    for line in disassemble_huc6280(code, origin) {
        s.push_str("        ");
        s.push_str(&line.text);
        s.push('\n');
    }
    s
}

/// Decode one HuC6280 instruction at `pos`. The extension is searched first so
/// its forms win any opcode the base 6502 set also carries.
fn decode_huc6280(
    code: &[u8],
    pos: usize,
) -> Option<(&'static str, &'static str, Vec<i64>, usize)> {
    let b = *code.get(pos)?;
    let (mn, form) = [&isa::huc6280::SET, &isa::mos6502::SET]
        .into_iter()
        .flat_map(|set| set.instructions.iter())
        .flat_map(|insn| insn.forms.iter().map(move |f| (insn.mnemonic, f)))
        .find(|(_, f)| f.opcode == [b])?;

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

/// Render a decoded HuC6280 instruction to ca65 syntax. A low-address `absolute`
/// operand takes an `a:` force so it re-assembles to the absolute form rather
/// than zero-page (the same pin the 65816 renderer uses).
fn render_huc6280(mn: &str, mode: &str, vals: &[i64], addr: u16, len: usize) -> String {
    let m = mn.to_ascii_lowercase();
    let v = vals.first().copied().unwrap_or(0);
    // Force absolute when a 16-bit operand would otherwise fold to zero-page.
    let abs = |val: i64| -> String {
        let force = if (0..0x100).contains(&val) { "a:" } else { "" };
        format!("{force}${:04X}", val & 0xFFFF)
    };
    let branch = |off: i64| addr.wrapping_add(len as u16).wrapping_add(off as u16);
    let operand = match mode {
        "implied" => String::new(),
        "accumulator" => "a".to_string(),
        "immediate" => format!("#${:02X}", v & 0xFF),
        "zeropage" => format!("${:02X}", v & 0xFF),
        "zeropage,x" => format!("${:02X},x", v & 0xFF),
        "zeropage,y" => format!("${:02X},y", v & 0xFF),
        "absolute" => abs(v),
        "absolute,x" => format!("{},x", abs(v)),
        "absolute,y" => format!("{},y", abs(v)),
        "indirect" => format!("(${:04X})", v & 0xFFFF),
        "(absolute,x)" => format!("(${:04X},x)", v & 0xFFFF),
        "(indirect)" => format!("(${:02X})", v & 0xFF),
        "(indirect,x)" => format!("(${:02X},x)", v & 0xFF),
        "(indirect),y" => format!("(${:02X}),y", v & 0xFF),
        "relative" => format!("${:04X}", branch(v)),
        // bbr/bbs: `<zp>, <target>` — the second operand is the relative offset.
        "zeropage,relative" => format!(
            "${:02X}, ${:04X}",
            v & 0xFF,
            branch(vals.get(1).copied().unwrap_or(0))
        ),
        // tst: `#<mask>, <mem>`.
        "immediate,zeropage" => {
            format!(
                "#${:02X}, ${:02X}",
                v & 0xFF,
                vals.get(1).copied().unwrap_or(0) & 0xFF
            )
        }
        "immediate,zeropage,x" => {
            format!(
                "#${:02X}, ${:02X},x",
                v & 0xFF,
                vals.get(1).copied().unwrap_or(0) & 0xFF
            )
        }
        "immediate,absolute" => {
            format!(
                "#${:02X}, {}",
                v & 0xFF,
                abs(vals.get(1).copied().unwrap_or(0))
            )
        }
        "immediate,absolute,x" => {
            format!(
                "#${:02X}, {},x",
                v & 0xFF,
                abs(vals.get(1).copied().unwrap_or(0))
            )
        }
        // Block transfers: `<src>, <dst>, <len>`, all 16-bit.
        "block" => format!(
            "${:04X}, ${:04X}, ${:04X}",
            v & 0xFFFF,
            vals.get(1).copied().unwrap_or(0) & 0xFFFF,
            vals.get(2).copied().unwrap_or(0) & 0xFFFF
        ),
        other => other.to_string(),
    };
    if operand.is_empty() {
        m
    } else {
        format!("{m} {operand}")
    }
}

// ---------------------------------------------------------------------------
// SM83 (Game Boy) disassembly — single-byte main page + the CB page
// ---------------------------------------------------------------------------

/// Disassemble a flat SM83 binary loaded at `origin`, rendering rgbasm syntax.
/// The main page is single-byte; `CB` is a two-byte prefix and `STOP` is the
/// two-byte `10 00`. Unknown bytes fall back to `db`.
#[must_use]
pub fn disassemble_sm83(code: &[u8], origin: u16) -> Vec<Line> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < code.len() {
        let addr = origin.wrapping_add(pos as u16);
        if let Some((mn, mode, vals, len)) = decode_sm83(code, pos) {
            out.push(Line {
                addr: u32::from(addr),
                bytes: code[pos..pos + len].to_vec(),
                text: render_sm83(mn, mode, &vals, addr, len),
            });
            pos += len;
        } else {
            out.push(Line {
                addr: u32::from(addr),
                bytes: vec![code[pos]],
                text: format!("db ${:02X}", code[pos]),
            });
            pos += 1;
        }
    }
    out
}

/// Render an SM83 disassembly as reassemblable rgbasm source.
#[must_use]
pub fn listing_sm83(code: &[u8], origin: u16) -> String {
    let mut s = format!("SECTION \"code\", ROM0[${origin:04X}]\n");
    for line in disassemble_sm83(code, origin) {
        s.push_str("    ");
        s.push_str(&line.text);
        s.push('\n');
    }
    s
}

/// Decode one SM83 instruction at `pos`. Matches the opcode (one byte, or the
/// `CB`/`STOP` two-byte forms) against the spec, then reads operand bytes.
fn decode_sm83(code: &[u8], pos: usize) -> Option<(&'static str, &'static str, Vec<i64>, usize)> {
    let b0 = *code.get(pos)?;
    let b1 = code.get(pos + 1).copied();
    // An opcode matches if its bytes match the stream. Two-byte opcodes (CB <op>
    // and STOP = 10 00) are tried first so they win over any one-byte form.
    let matches = |op: &[u8]| match op {
        [x, y] => *x == b0 && b1 == Some(*y),
        [x] => *x == b0,
        _ => false,
    };
    let forms = || {
        isa::sm83::SET
            .instructions
            .iter()
            .flat_map(|insn| insn.forms.iter().map(move |f| (insn.mnemonic, f)))
    };
    let (mn, form) = forms()
        .find(|(_, f)| f.opcode.len() == 2 && matches(f.opcode))
        .or_else(|| forms().find(|(_, f)| f.opcode.len() == 1 && matches(f.opcode)))?;

    let mut vals = Vec::new();
    let mut off = pos + form.opcode.len();
    for operand in form.operands {
        let n = operand.bytes as usize;
        let mut v: i64 = 0;
        for k in 0..n {
            v |= i64::from(*code.get(off + k)?) << (8 * k);
        }
        if matches!(
            operand.kind,
            isa::OperandKind::RelativePc | isa::OperandKind::Displacement
        ) {
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

/// Render a decoded SM83 instruction to rgbasm syntax by substituting the
/// upper-case placeholders in the mode label (`NN`/`N` immediates, `E` a `jr`
/// target, `+D`/`D` a signed `sp` displacement). Lower-case register text passes
/// through verbatim.
fn render_sm83(mnemonic: &str, mode: &str, values: &[i64], addr: u16, len: usize) -> String {
    let m = mnemonic.to_ascii_lowercase();
    if mode.is_empty() {
        return m;
    }
    // RST's target is the mode label as hex digits.
    if mnemonic == "RST" {
        return format!("{m} ${mode}");
    }
    let bytes = mode.as_bytes();
    let mut out = String::new();
    let mut vi = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"NN") {
            out.push_str(&format!("${:04X}", values[vi] as u16));
            vi += 1;
            i += 2;
        } else if bytes[i] == b'+' && bytes.get(i + 1) == Some(&b'D') {
            let d = values[vi];
            vi += 1;
            i += 2;
            out.push_str(&signed_hex(d, "+"));
        } else if bytes[i] == b'N' {
            out.push_str(&format!("${:02X}", values[vi] as u8));
            vi += 1;
            i += 1;
        } else if bytes[i] == b'D' {
            let d = values[vi];
            vi += 1;
            i += 1;
            out.push_str(&signed_hex(d, ""));
        } else if bytes[i] == b'E' {
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
    format!("{m} {out}")
}

/// A signed byte as `<lead>$XX` (positive) or `-$XX` (negative), for the `sp`
/// displacement ops. `lead` is `"+"` inside a `sp+D` context, empty for a bare
/// operand.
fn signed_hex(d: i64, lead: &str) -> String {
    if d < 0 {
        format!("-${:02X}", (-d) as u8)
    } else {
        format!("{lead}${:02X}", d as u8)
    }
}

// ---------------------------------------------------------------------------
// Intel 8080 disassembly — single-byte opcodes, Intel mnemonics
// ---------------------------------------------------------------------------

/// Disassemble a flat 8080 binary loaded at `origin`, rendering `asl` Intel
/// syntax. Every opcode is a single byte; jumps/calls are absolute, so the
/// output is position-independent. An unknown byte becomes a `db` datum.
#[must_use]
pub fn disassemble_i8080(code: &[u8], origin: u16) -> Vec<Line> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < code.len() {
        let addr = origin.wrapping_add(pos as u16);
        if let Some((mn, mode, vals, len)) = decode_i8080(code, pos) {
            out.push(Line {
                addr: u32::from(addr),
                bytes: code[pos..pos + len].to_vec(),
                text: render_i8080(mn, mode, &vals),
            });
            pos += len;
        } else {
            out.push(Line {
                addr: u32::from(addr),
                bytes: vec![code[pos]],
                text: format!("db 0{:02X}H", code[pos]),
            });
            pos += 1;
        }
    }
    out
}

/// Render an 8080 disassembly as reassemblable `asl` source (Intel syntax).
#[must_use]
pub fn listing_i8080(code: &[u8], origin: u16) -> String {
    let mut s = format!("\tcpu 8080\n\torg 0{origin:04X}H\n");
    for line in disassemble_i8080(code, origin) {
        s.push('\t');
        s.push_str(&line.text);
        s.push('\n');
    }
    s.push_str("\tend\n");
    s
}

fn decode_i8080(code: &[u8], pos: usize) -> Option<(&'static str, &'static str, Vec<i64>, usize)> {
    let b = *code.get(pos)?;
    let (mn, form) = isa::i8080::SET
        .instructions
        .iter()
        .flat_map(|insn| insn.forms.iter().map(move |f| (insn.mnemonic, f)))
        .find(|(_, f)| f.opcode == [b])?;

    let mut vals = Vec::new();
    let mut off = pos + 1;
    for operand in form.operands {
        let n = operand.bytes as usize;
        let mut v: i64 = 0;
        for k in 0..n {
            v |= i64::from(*code.get(off + k)?) << (8 * k);
        }
        vals.push(v);
        off += n;
    }
    Some((mn, form.mode, vals, off - pos))
}

/// Render a decoded 8080 instruction to Intel syntax, substituting the `N`/`NN`
/// placeholders with `H`-suffix hex. A leading `0` keeps every literal
/// digit-first (asl rejects a hex literal that starts with a letter). `rst`'s
/// vector number is the mode label itself; registers pass through verbatim.
fn render_i8080(mnemonic: &str, mode: &str, values: &[i64]) -> String {
    let m = mnemonic.to_ascii_lowercase();
    if mode.is_empty() {
        return m;
    }
    if mnemonic == "RST" {
        return format!("{m} {mode}");
    }
    let bytes = mode.as_bytes();
    let mut out = String::new();
    let mut vi = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"NN") {
            out.push_str(&format!("0{:04X}H", values[vi] as u16));
            vi += 1;
            i += 2;
        } else if bytes[i] == b'N' {
            out.push_str(&format!("0{:02X}H", values[vi] as u8));
            vi += 1;
            i += 1;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    format!("{m} {out}")
}

// ---------------------------------------------------------------------------
// Motorola 6800 disassembly — single-byte opcodes, big-endian, Motorola syntax
// ---------------------------------------------------------------------------

/// Disassemble a flat 6800 binary loaded at `origin`, rendering `asl` Motorola
/// syntax. Single-byte opcodes; 16-bit operands are big-endian. Branches are
/// PC-relative, so the listing carries its origin. Unknown bytes become `fcb`.
#[must_use]
pub fn disassemble_m6800(code: &[u8], origin: u16) -> Vec<Line> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < code.len() {
        let addr = origin.wrapping_add(pos as u16);
        if let Some((mn, mode, vals, len)) = decode_m6800(code, pos) {
            out.push(Line {
                addr: u32::from(addr),
                bytes: code[pos..pos + len].to_vec(),
                text: render_m6800(mn, mode, &vals, addr, len),
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

/// Render a 6800 disassembly as reassemblable `asl` source (Motorola syntax).
#[must_use]
pub fn listing_m6800(code: &[u8], origin: u16) -> String {
    let mut s = format!("\tcpu 6800\n\torg ${origin:04X}\n");
    for line in disassemble_m6800(code, origin) {
        s.push('\t');
        s.push_str(&line.text);
        s.push('\n');
    }
    s.push_str("\tend\n");
    s
}

fn decode_m6800(code: &[u8], pos: usize) -> Option<(&'static str, &'static str, Vec<i64>, usize)> {
    let b = *code.get(pos)?;
    let (mn, form) = isa::m6800::SET
        .instructions
        .iter()
        .flat_map(|insn| insn.forms.iter().map(move |f| (insn.mnemonic, f)))
        .find(|(_, f)| f.opcode == [b])?;

    let mut vals = Vec::new();
    let mut off = pos + 1;
    for operand in form.operands {
        let n = operand.bytes as usize;
        // 6800 is big-endian: high byte first.
        let mut v: i64 = 0;
        for k in 0..n {
            v = (v << 8) | i64::from(*code.get(off + k)?);
        }
        if matches!(operand.kind, isa::OperandKind::RelativePc) && v & 0x80 != 0 {
            v -= 0x100;
        }
        vals.push(v);
        off += n;
    }
    Some((mn, form.mode, vals, off - pos))
}

/// Render a decoded 6800 instruction to Motorola `asl` syntax. The mode label
/// selects the operand shape; an immediate's width comes from the instruction
/// length (2 bytes ⇒ 8-bit, 3 ⇒ 16-bit).
fn render_m6800(mnemonic: &str, mode: &str, values: &[i64], addr: u16, len: usize) -> String {
    let m = mnemonic.to_ascii_lowercase();
    let v = values.first().copied().unwrap_or(0);
    let operand = match mode {
        "inherent" => String::new(),
        "immediate" if len == 3 => format!("#${:04X}", v & 0xFFFF),
        "immediate" => format!("#${:02X}", v & 0xFF),
        "direct" => format!("${:02X}", v & 0xFF),
        "extended" => format!("${:04X}", v & 0xFFFF),
        "indexed" => format!("${:02X},x", v & 0xFF),
        "relative" => {
            let target = addr.wrapping_add(len as u16).wrapping_add(v as u16);
            format!("${target:04X}")
        }
        other => other.to_string(),
    };
    if operand.is_empty() {
        m
    } else {
        format!("{m} {operand}")
    }
}

// ---------------------------------------------------------------------------
// RCA CDP1802 (COSMAC) disassembly — single-byte opcodes, big-endian
// ---------------------------------------------------------------------------

/// Disassemble a flat CDP1802 binary loaded at `origin`, rendering `asl` syntax.
/// Register ops carry the register number in the opcode low nibble; the short
/// branch's operand byte is the low byte of a same-page target, reconstructed
/// against the instruction's page. Unknown bytes become `db`.
#[must_use]
pub fn disassemble_1802(code: &[u8], origin: u16) -> Vec<Line> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < code.len() {
        let addr = origin.wrapping_add(pos as u16);
        if let Some((mn, mode, vals, len)) = decode_1802(code, pos) {
            out.push(Line {
                addr: u32::from(addr),
                bytes: code[pos..pos + len].to_vec(),
                text: render_1802(mn, mode, &vals, addr),
            });
            pos += len;
        } else {
            out.push(Line {
                addr: u32::from(addr),
                bytes: vec![code[pos]],
                text: format!("db 0{:02X}H", code[pos]),
            });
            pos += 1;
        }
    }
    out
}

/// Render a CDP1802 disassembly as reassemblable `asl` source.
#[must_use]
pub fn listing_1802(code: &[u8], origin: u16) -> String {
    let mut s = format!("\tcpu 1802\n\torg 0{origin:04X}H\n");
    for line in disassemble_1802(code, origin) {
        s.push('\t');
        s.push_str(&line.text);
        s.push('\n');
    }
    s.push_str("\tend\n");
    s
}

fn decode_1802(code: &[u8], pos: usize) -> Option<(&'static str, &'static str, Vec<i64>, usize)> {
    let b = *code.get(pos)?;
    let (mn, form) = isa::cdp1802::SET
        .instructions
        .iter()
        .flat_map(|insn| insn.forms.iter().map(move |f| (insn.mnemonic, f)))
        .find(|(_, f)| f.opcode == [b])?;

    let mut vals = Vec::new();
    let mut off = pos + 1;
    for operand in form.operands {
        let n = operand.bytes as usize;
        // Big-endian.
        let mut v: i64 = 0;
        for k in 0..n {
            v = (v << 8) | i64::from(*code.get(off + k)?);
        }
        vals.push(v);
        off += n;
    }
    Some((mn, form.mode, vals, off - pos))
}

/// Render a decoded CDP1802 instruction to `asl` syntax. The known mode labels
/// (`inherent`/`immediate`/`short`/`long`) select the operand shape; any other
/// label is a register op whose mode label *is* the register number.
fn render_1802(mnemonic: &str, mode: &str, values: &[i64], addr: u16) -> String {
    let m = mnemonic.to_ascii_lowercase();
    let v = values.first().copied().unwrap_or(0);
    match mode {
        "inherent" => m,
        "immediate" => format!("{m} 0{:02X}H", v & 0xFF),
        // Short branch: the low byte on the instruction's page.
        "short" => format!("{m} 0{:04X}H", (addr & 0xFF00) | (v as u16 & 0xFF)),
        "long" => format!("{m} 0{:04X}H", v & 0xFFFF),
        reg => format!("{m} {reg}"),
    }
}

/// Disassemble an Intel 8048 (MCS-48) program.
///
/// Spec forms decode by opcode; the two `JMP`/`CALL` opcode families (11-bit
/// absolute, high 3 bits packed into the opcode) are handled directly, since
/// they are not spec forms.
#[must_use]
pub fn disassemble_8048(code: &[u8], origin: u16) -> Vec<Line> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < code.len() {
        let addr = origin.wrapping_add(pos as u16);
        let b = code[pos];
        if let Some((mn, mode, vals, len)) = decode_8048(code, pos) {
            out.push(Line {
                addr: u32::from(addr),
                bytes: code[pos..pos + len].to_vec(),
                text: render_8048(mn, mode, &vals, addr),
            });
            pos += len;
        } else if (b & 0x1F == 0x04 || b & 0x1F == 0x14) && pos + 1 < code.len() {
            // JMP (…00100) / CALL (…10100): opcode carries address bits 10-8.
            let mn = if b & 0x1F == 0x04 { "jmp" } else { "call" };
            let target = ((u16::from(b) >> 5) & 0x07) << 8 | u16::from(code[pos + 1]);
            out.push(Line {
                addr: u32::from(addr),
                bytes: vec![b, code[pos + 1]],
                text: format!("{mn} 0{target:04X}H"),
            });
            pos += 2;
        } else {
            out.push(Line {
                addr: u32::from(addr),
                bytes: vec![b],
                text: format!("db 0{b:02X}H"),
            });
            pos += 1;
        }
    }
    out
}

/// Render an 8048 disassembly as reassemblable `asl` source.
#[must_use]
pub fn listing_8048(code: &[u8], origin: u16) -> String {
    let mut s = format!("\tcpu 8048\n\torg 0{origin:04X}H\n");
    for line in disassemble_8048(code, origin) {
        s.push('\t');
        s.push_str(&line.text);
        s.push('\n');
    }
    s.push_str("\tend\n");
    s
}

fn decode_8048(code: &[u8], pos: usize) -> Option<(&'static str, &'static str, Vec<i64>, usize)> {
    let b = *code.get(pos)?;
    let (mn, form) = isa::i8048::SET
        .instructions
        .iter()
        .flat_map(|insn| insn.forms.iter().map(move |f| (insn.mnemonic, f)))
        .find(|(_, f)| f.opcode == [b])?;

    let mut vals = Vec::new();
    let mut off = pos + 1;
    for operand in form.operands {
        // Every 8048 operand is a single byte.
        vals.push(i64::from(*code.get(off)?));
        off += operand.bytes as usize;
    }
    Some((mn, form.mode, vals, off - pos))
}

/// Render a decoded 8048 instruction to `asl` syntax. The mode label is the
/// operand template: `rel`/`…,rel` are page-relative jumps (the byte is the low
/// 8 bits of the same-page target), `#N` is an 8-bit immediate, and anything
/// else is fixed operand text.
fn render_8048(mnemonic: &str, mode: &str, values: &[i64], addr: u16) -> String {
    let m = mnemonic.to_ascii_lowercase();
    let v = values.first().copied().unwrap_or(0);
    if mode.is_empty() {
        return m;
    }
    if mode == "rel" {
        let target = (addr & 0xFF00) | (v as u16 & 0xFF);
        return format!("{m} 0{target:04X}H");
    }
    if let Some(reg) = mode.strip_suffix(",rel") {
        let target = (addr & 0xFF00) | (v as u16 & 0xFF);
        return format!("{m} {reg},0{target:04X}H");
    }
    if mode.contains("#N") {
        let imm = format!("#0{:02X}H", v & 0xFF);
        return format!("{m} {}", mode.replace("#N", &imm));
    }
    format!("{m} {mode}")
}

/// Disassemble a National SC/MP (INS8060) program.
#[must_use]
pub fn disassemble_scmp(code: &[u8], origin: u16) -> Vec<Line> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < code.len() {
        let addr = origin.wrapping_add(pos as u16);
        if let Some((mn, mode, vals, len)) = decode_scmp(code, pos) {
            out.push(Line {
                addr: u32::from(addr),
                bytes: code[pos..pos + len].to_vec(),
                text: render_scmp(mn, mode, &vals),
            });
            pos += len;
        } else {
            out.push(Line {
                addr: u32::from(addr),
                bytes: vec![code[pos]],
                text: format!("db 0x{:02X}", code[pos]),
            });
            pos += 1;
        }
    }
    out
}

/// Render an SC/MP disassembly as reassemblable `asl` source.
#[must_use]
pub fn listing_scmp(code: &[u8], origin: u16) -> String {
    let mut s = format!("\tcpu SC/MP\n\torg 0x{origin:04X}\n");
    for line in disassemble_scmp(code, origin) {
        s.push('\t');
        s.push_str(&line.text);
        s.push('\n');
    }
    s.push_str("\tend\n");
    s
}

fn decode_scmp(code: &[u8], pos: usize) -> Option<(&'static str, &'static str, Vec<i64>, usize)> {
    let b = *code.get(pos)?;
    let (mn, form) = isa::scmp::SET
        .instructions
        .iter()
        .flat_map(|insn| insn.forms.iter().map(move |f| (insn.mnemonic, f)))
        .find(|(_, f)| f.opcode == [b])?;

    let mut vals = Vec::new();
    let mut off = pos + 1;
    for operand in form.operands {
        vals.push(i64::from(*code.get(off)?));
        off += operand.bytes as usize;
    }
    Some((mn, form.mode, vals, off - pos))
}

/// Render a decoded SC/MP instruction to `asl` syntax. Mode `""` is inherent,
/// `"imm"` an immediate byte, and a bare `"0".."3"`/`"@1".."@3"` is a pointer
/// form — an operand-less pointer exchange, or a `disp(ptr)` memory reference
/// where the byte `0x80` renders as the literal `e` (the E-register index).
fn render_scmp(mnemonic: &str, mode: &str, values: &[i64]) -> String {
    let m = mnemonic.to_ascii_lowercase();
    if mode.is_empty() {
        return m;
    }
    if mode == "imm" {
        return format!("{m} 0x{:02X}", values.first().copied().unwrap_or(0) & 0xFF);
    }
    let at = if mode.starts_with('@') { "@" } else { "" };
    let ptr = mode.trim_start_matches('@');
    match values.first() {
        // Pointer exchange: no displacement byte.
        None => format!("{m} {ptr}"),
        // Memory reference: 0x80 selects the E register; else signed displacement.
        Some(&v) if v & 0xFF == 0x80 => format!("{m} {at}e({ptr})"),
        Some(&v) => format!("{m} {at}{}({ptr})", v as u8 as i8),
    }
}

/// Disassemble a Fairchild F8 (3850) program.
#[must_use]
pub fn disassemble_f8(code: &[u8], origin: u16) -> Vec<Line> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < code.len() {
        let addr = origin.wrapping_add(pos as u16);
        if let Some((mn, mode, vals, len)) = decode_f8(code, pos) {
            out.push(Line {
                addr: u32::from(addr),
                bytes: code[pos..pos + len].to_vec(),
                text: render_f8(mn, mode, &vals, addr),
            });
            pos += len;
        } else {
            out.push(Line {
                addr: u32::from(addr),
                bytes: vec![code[pos]],
                text: format!("db 0{:02X}H", code[pos]),
            });
            pos += 1;
        }
    }
    out
}

/// Render an F8 disassembly as reassemblable `asl` source.
#[must_use]
pub fn listing_f8(code: &[u8], origin: u16) -> String {
    let mut s = format!("\tcpu F3850\n\torg 0{origin:04X}H\n");
    for line in disassemble_f8(code, origin) {
        s.push('\t');
        s.push_str(&line.text);
        s.push('\n');
    }
    s.push_str("\tend\n");
    s
}

fn decode_f8(code: &[u8], pos: usize) -> Option<(&'static str, &'static str, Vec<i64>, usize)> {
    let b = *code.get(pos)?;
    let (mn, form) = isa::f8::SET
        .instructions
        .iter()
        .flat_map(|insn| insn.forms.iter().map(move |f| (insn.mnemonic, f)))
        .find(|(_, f)| f.opcode == [b])?;

    let mut vals = Vec::new();
    let mut off = pos + 1;
    for operand in form.operands {
        // F8 lays multi-byte operands (the 16-bit address) big-endian.
        let w = operand.bytes as usize;
        let mut v: i64 = 0;
        for k in 0..w {
            v = (v << 8) | i64::from(*code.get(off + k)?);
        }
        vals.push(v);
        off += w;
    }
    Some((mn, form.mode, vals, off - pos))
}

/// Render a decoded F8 instruction to `asl` syntax. Branches (`BT`/`BF`/`BR7`)
/// resolve their signed offset — measured from the offset byte, one past the
/// opcode — to an absolute target. Every other mode label *is* the operand text
/// (register nibble, `LR d,s`, `LIS n`, shift count), bar the `imm`/`port` byte
/// and the 16-bit `abs`.
fn render_f8(mnemonic: &str, mode: &str, values: &[i64], addr: u16) -> String {
    let m = mnemonic.to_ascii_lowercase();
    let v = values.first().copied().unwrap_or(0);
    match mnemonic {
        "BR7" => {
            let target = addr.wrapping_add(1).wrapping_add(v as i8 as u16);
            return format!("br7 0{target:04X}H");
        }
        "BT" | "BF" => {
            let target = addr.wrapping_add(1).wrapping_add(v as i8 as u16);
            return format!("{m} {mode},0{target:04X}H");
        }
        _ => {}
    }
    match mode {
        "" => m,
        "imm" | "port" => format!("{m} 0{:02X}H", v & 0xFF),
        "abs" => format!("{m} 0{:04X}H", v & 0xFFFF),
        _ => format!("{m} {mode}"),
    }
}

/// Disassemble a Signetics 2650 program.
#[must_use]
pub fn disassemble_2650(code: &[u8], origin: u16) -> Vec<Line> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < code.len() {
        let addr = origin.wrapping_add(pos as u16);
        if let Some((mn, mode, kind, value, len)) = decode_2650(code, pos) {
            out.push(Line {
                addr: u32::from(addr),
                bytes: code[pos..pos + len].to_vec(),
                text: render_2650(mn, mode, kind, value, addr),
            });
            pos += len;
        } else {
            out.push(Line {
                addr: u32::from(addr),
                bytes: vec![code[pos]],
                text: format!("db ${:02X}", code[pos]),
            });
            pos += 1;
        }
    }
    out
}

/// Render a 2650 disassembly as reassemblable `asl` source.
#[must_use]
pub fn listing_2650(code: &[u8], origin: u16) -> String {
    let mut s = format!("\tcpu 2650\n\torg ${origin:04X}\n");
    for line in disassemble_2650(code, origin) {
        s.push('\t');
        s.push_str(&line.text);
        s.push('\n');
    }
    s.push_str("\tend\n");
    s
}

fn decode_2650(
    code: &[u8],
    pos: usize,
) -> Option<(
    &'static str,
    &'static str,
    Option<isa::OperandKind>,
    i64,
    usize,
)> {
    let b = *code.get(pos)?;
    let (mn, form) = isa::s2650::SET
        .instructions
        .iter()
        .flat_map(|insn| insn.forms.iter().map(move |f| (insn.mnemonic, f)))
        .find(|(_, f)| f.opcode == [b])?;
    let kind = form.operands.first().map(|o| o.kind);
    let (value, len) = match kind {
        None => (0, 1),
        Some(isa::OperandKind::Immediate | isa::OperandKind::RelativePc) => {
            (i64::from(*code.get(pos + 1)?), 2)
        }
        Some(isa::OperandKind::Address) => {
            let hi = i64::from(*code.get(pos + 1)?);
            let lo = i64::from(*code.get(pos + 2)?);
            ((hi << 8) | lo, 3)
        }
        _ => (0, 1),
    };
    Some((mn, form.mode, kind, value, len))
}

/// Render a decoded 2650 instruction to `asl` syntax. The mode label is the
/// register (`r0`-`r3`) or condition (`eq`/`gt`/`lt`/`un`); relative operands
/// resolve their 7-bit signed displacement (bit 7 = indirect `*`) to an absolute
/// target; absolute operands carry bit 15 = indirect and, for the memory-
/// reference ops, bits 14-13 = the `,r3` index control.
fn render_2650(
    mnemonic: &str,
    mode: &str,
    kind: Option<isa::OperandKind>,
    value: i64,
    addr: u16,
) -> String {
    let m = mnemonic.to_ascii_lowercase();
    let sel = if mode.is_empty() {
        String::new()
    } else {
        format!(",{mode}")
    };
    match kind {
        None => format!("{m}{sel}"),
        Some(isa::OperandKind::Immediate) => format!("{m}{sel} ${:02X}", value & 0xFF),
        Some(isa::OperandKind::RelativePc) => {
            let byte = value as u8;
            let star = if byte & 0x80 != 0 { "*" } else { "" };
            let d7 = byte & 0x7F;
            let signed = if d7 >= 0x40 {
                i32::from(d7) - 0x80
            } else {
                i32::from(d7)
            };
            let target = addr.wrapping_add(2).wrapping_add(signed as u16);
            format!("{m}{sel} {star}${target:04X}")
        }
        Some(isa::OperandKind::Address) => {
            let w = value as u16;
            let star = if w & 0x8000 != 0 { "*" } else { "" };
            let memref = is_memref_abs_mn(mnemonic);
            if memref && mode == "r3" {
                let ctrl = (w >> 13) & 3;
                let a = w & 0x1FFF;
                if ctrl == 0 {
                    format!("{m},r3 {star}${a:04X}")
                } else {
                    let auto = match ctrl {
                        1 => ",+",
                        2 => ",-",
                        _ => "",
                    };
                    format!("{m},r0 {star}${a:04X},r3{auto}")
                }
            } else {
                let a = w & 0x7FFF;
                format!("{m}{sel} {star}${a:04X}")
            }
        }
        _ => m,
    }
}

/// The memory-reference absolute mnemonics support `,r3` indexing (a copy of the
/// dialect's predicate; kept here so the dependency-free disassembler needs no
/// dialect dependency).
fn is_memref_abs_mn(mn: &str) -> bool {
    matches!(
        mn,
        "LODA" | "STRA" | "ADDA" | "SUBA" | "ANDA" | "IORA" | "EORA" | "COMA"
    )
}

/// Disassemble a TI TMS7000 program.
#[must_use]
pub fn disassemble_tms7000(code: &[u8], origin: u16) -> Vec<Line> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < code.len() {
        let addr = origin.wrapping_add(pos as u16);
        let b = code[pos];
        if b >= 0xE8 {
            // TRAP n occupies 0xE8..=0xFF as 0xFF - n; not a spec form.
            out.push(Line {
                addr: u32::from(addr),
                bytes: vec![b],
                text: format!("trap {}", 0xFF - b),
            });
            pos += 1;
        } else if let Some((mn, form, vals, len)) = decode_tms7000(code, pos) {
            out.push(Line {
                addr: u32::from(addr),
                bytes: code[pos..pos + len].to_vec(),
                text: render_tms7000(mn, form, &vals, addr, len),
            });
            pos += len;
        } else {
            out.push(Line {
                addr: u32::from(addr),
                bytes: vec![b],
                text: format!("db 0{b:02X}H"),
            });
            pos += 1;
        }
    }
    out
}

/// Render a TMS7000 disassembly as reassemblable `asl` source.
#[must_use]
pub fn listing_tms7000(code: &[u8], origin: u16) -> String {
    let mut s = format!("\tcpu TMS70C00\n\torg 0{origin:04X}H\n");
    for line in disassemble_tms7000(code, origin) {
        s.push('\t');
        s.push_str(&line.text);
        s.push('\n');
    }
    s.push_str("\tend\n");
    s
}

fn decode_tms7000(
    code: &[u8],
    pos: usize,
) -> Option<(&'static str, &'static isa::Form, Vec<i64>, usize)> {
    let b = *code.get(pos)?;
    let (mn, form) = isa::tms7000::SET
        .instructions
        .iter()
        .flat_map(|insn| insn.forms.iter().map(move |f| (insn.mnemonic, f)))
        .find(|(_, f)| f.opcode == [b])?;
    let mut vals = Vec::new();
    let mut off = pos + 1;
    for operand in form.operands {
        match operand.bytes {
            2 => {
                let hi = i64::from(*code.get(off)?);
                let lo = i64::from(*code.get(off + 1)?);
                vals.push((hi << 8) | lo);
                off += 2;
            }
            _ => {
                vals.push(i64::from(*code.get(off)?));
                off += 1;
            }
        }
    }
    Some((mn, form, vals, off - pos))
}

fn tms_reg(v: i64) -> String {
    format!("r{}", v & 0xFF)
}
fn tms_per(v: i64) -> String {
    format!("p{}", v & 0xFF)
}
fn tms_imm(v: i64) -> String {
    format!("%0{:02X}H", v & 0xFF)
}
fn tms_word(v: i64) -> String {
    format!("0{:04X}H", v & 0xFFFF)
}

/// Render the operand text (no mnemonic, no trailing jump target) for a mode.
fn tms_operands(mn: &str, mode: &str, vals: &[i64]) -> String {
    let v0 = vals.first().copied().unwrap_or(0);
    let v1 = vals.get(1).copied().unwrap_or(0);
    if mn == "MOVD" {
        return match mode {
            "%n,rn" => format!("%{},{}", tms_word(v0), tms_reg(v1)),
            "%n(b),rn" => format!("%{}(b),{}", tms_word(v0), tms_reg(v1)),
            "rn,rn" => format!("{},{}", tms_reg(v0), tms_reg(v1)),
            _ => mode.to_string(),
        };
    }
    match mode {
        "a" | "b" | "st" | "b,a" | "a,b" => mode.to_string(),
        "rn" => tms_reg(v0),
        "rn,a" => format!("{},a", tms_reg(v0)),
        "%n,a" => format!("{},a", tms_imm(v0)),
        "rn,b" => format!("{},b", tms_reg(v0)),
        "rn,rn" => format!("{},{}", tms_reg(v0), tms_reg(v1)),
        "%n,b" => format!("{},b", tms_imm(v0)),
        "%n,rn" => format!("{},{}", tms_imm(v0), tms_reg(v1)),
        "a,rn" => format!("a,{}", tms_reg(v0)),
        "b,rn" => format!("b,{}", tms_reg(v0)),
        "pn,a" => format!("{},a", tms_per(v0)),
        "pn,b" => format!("{},b", tms_per(v0)),
        "a,pn" => format!("a,{}", tms_per(v0)),
        "b,pn" => format!("b,{}", tms_per(v0)),
        "%n,pn" => format!("{},{}", tms_imm(v0), tms_per(v1)),
        "@" => format!("@{}", tms_word(v0)),
        "*" => format!("*{}", tms_reg(v0)),
        "@(b)" => format!("@{}(b)", tms_word(v0)),
        other => other.to_string(),
    }
}

/// Render a decoded TMS7000 instruction to `asl` syntax. Operand flavour comes
/// from the mode label (register / immediate / peripheral / address); a trailing
/// `RelativePc` operand (the bit-test-and-jump and `DJNZ` ops) resolves to an
/// absolute target, as do the plain conditional jumps.
fn render_tms7000(mn: &str, form: &isa::Form, vals: &[i64], addr: u16, len: usize) -> String {
    let m = mn.to_ascii_lowercase();
    let has_rel = form
        .operands
        .last()
        .is_some_and(|o| o.kind == isa::OperandKind::RelativePc);

    // A plain conditional jump: an empty mode with a single relative operand.
    // (DJNZ A/B also has a lone relative operand, but its mode is "a"/"b".)
    if has_rel && form.operands.len() == 1 && form.mode.is_empty() {
        let target = addr
            .wrapping_add(len as u16)
            .wrapping_add(vals[0] as i8 as u16);
        return format!("{m} {}", tms_word(i64::from(target)));
    }
    // No operands: implied, or a fixed keyword operand (a / b / st).
    if form.operands.is_empty() {
        return if form.mode.is_empty() {
            m
        } else {
            format!("{m} {}", form.mode)
        };
    }

    let n_data = form.operands.len() - usize::from(has_rel);
    let operands = tms_operands(mn, form.mode, &vals[..n_data]);
    if has_rel {
        let target = addr
            .wrapping_add(len as u16)
            .wrapping_add(vals[n_data] as i8 as u16);
        format!("{m} {operands},{}", tms_word(i64::from(target)))
    } else {
        format!("{m} {operands}")
    }
}

// ---------------------------------------------------------------------------
// DEC PDP-11
// ---------------------------------------------------------------------------

/// Disassemble a flat PDP-11 binary loaded at `origin`. The PDP-11 is
/// **little-endian** with 16-bit words; each instruction is one opcode word plus
/// 0–2 extension words (index displacements, immediates, absolute addresses, or
/// PC-relative displacements). Undecodable words render as `word` data, a
/// trailing odd byte as `byte`.
#[must_use]
pub fn disassemble_pdp11(code: &[u8], origin: u16) -> Vec<Line> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < code.len() {
        let addr = origin.wrapping_add(pos as u16);
        if pos + 1 >= code.len() {
            out.push(Line {
                addr: u32::from(addr),
                bytes: vec![code[pos]],
                text: format!("byte 0x{:02X}", code[pos]),
            });
            pos += 1;
            continue;
        }
        let word = u16::from_le_bytes([code[pos], code[pos + 1]]);
        match decode_pdp11(code, pos, addr) {
            Some((text, len)) => {
                out.push(Line {
                    addr: u32::from(addr),
                    bytes: code[pos..pos + len].to_vec(),
                    text,
                });
                pos += len;
            }
            None => {
                out.push(Line {
                    addr: u32::from(addr),
                    bytes: code[pos..pos + 2].to_vec(),
                    text: format!("word 0x{word:04X}"),
                });
                pos += 2;
            }
        }
    }
    out
}

/// Render a PDP-11 disassembly as reassemblable `asl` source.
#[must_use]
pub fn listing_pdp11(code: &[u8], origin: u16) -> String {
    let mut s = format!("\tcpu MICROPDP-11/93\n\torg 0x{origin:04X}\n");
    for line in disassemble_pdp11(code, origin) {
        s.push('\t');
        s.push_str(&line.text);
        s.push('\n');
    }
    s.push_str("\tend\n");
    s
}

/// Render one operand from its 6-bit field, reading an extension word from
/// `code` at byte offset `ext_off` when the mode needs one. Returns the operand
/// text and the number of extension words consumed (0 or 1), or `None` if the
/// needed word runs past the buffer.
///
/// `instr_addr` is the opcode word's address and `ext_ord` the 0-based index of
/// this operand's extension word among the instruction's — together they place
/// the PC-relative base (the address just past this word). `imm_ok` says whether
/// `asl` accepts immediate mode (`#n`) here: it only does in a *source* field,
/// so in a destination field the same encoding (mode 2, reg 7) is rendered as
/// the raw `(pc)+` (no extension word — the value becomes the next datum), which
/// `asl` accepts anywhere.
fn pdp11_read_ea(
    code: &[u8],
    ext_off: usize,
    field: u16,
    instr_addr: u16,
    ext_ord: usize,
    imm_ok: bool,
) -> Option<(String, usize)> {
    let (mode, reg) = (field >> 3, field & 7);
    // Which reg-7 / index modes carry an extension word here.
    let has_ext = match mode {
        6 | 7 => true,
        3 if reg == 7 => true,
        2 if reg == 7 => imm_ok,
        _ => false,
    };
    if has_ext && ext_off + 2 > code.len() {
        return None;
    }
    let ext = || u16::from_le_bytes([code[ext_off], code[ext_off + 1]]);

    if reg == 7 {
        match mode {
            2 if imm_ok => return Some((format!("#0x{:04X}", ext()), 1)),
            3 => return Some((format!("@#0x{:04X}", ext()), 1)),
            6 | 7 => {
                // Relative: base = the address just past this extension word.
                let base = instr_addr.wrapping_add(4 + 2 * ext_ord as u16);
                let target = base.wrapping_add(ext());
                let s = if mode == 6 {
                    format!("0x{target:04X}")
                } else {
                    format!("@0x{target:04X}")
                };
                return Some((s, 1));
            }
            _ => {} // modes 0,1,4,5 (and 2 when !imm_ok) fall through to r7 forms
        }
    }
    let r = format!("r{reg}");
    let s = match mode {
        0 => r,
        1 => format!("({r})"),
        2 => format!("({r})+"),
        3 => format!("@({r})+"),
        4 => format!("-({r})"),
        5 => format!("@-({r})"),
        6 => return Some((format!("0x{:04X}({r})", ext()), 1)),
        7 => return Some((format!("@0x{:04X}({r})", ext()), 1)),
        _ => unreachable!(),
    };
    Some((s, 0))
}

/// Whether `asl` accepts immediate mode (`#n`) in a single-operand instruction's
/// operand — true only for the ops whose operand is an ISA *source* field
/// (`JMP`'s address, the previous-space / status *reads*), false for the
/// read-modify-write and store ops.
fn pdp11_single_imm_ok(mn: &str) -> bool {
    matches!(mn, "JMP" | "MFPI" | "MFPD" | "MTPS" | "CSM")
}

/// Decode one PDP-11 instruction at byte offset `pos`, `instr_addr` its load
/// address. Returns the reassemblable text and the total byte length, or `None`
/// for an undecodable / illegal word (rendered as `word` data by the caller).
fn decode_pdp11(code: &[u8], pos: usize, instr_addr: u16) -> Option<(String, usize)> {
    use isa::pdp11::Class;
    let word = u16::from_le_bytes([code[pos], code[pos + 1]]);
    let insn = isa::pdp11::decode(word)?;
    let mn = insn.mnemonic.to_ascii_lowercase();

    // Read an operand at the running extension-word cursor, advancing it.
    let mut ord = 0usize;
    let mut off = pos + 2;
    let mut read = |field: u16, imm_ok: bool| -> Option<String> {
        let (s, w) = pdp11_read_ea(code, off, field, instr_addr, ord, imm_ok)?;
        ord += w;
        off += 2 * w;
        Some(s)
    };

    let text = match insn.class {
        Class::Double => {
            let (src, dst) = ((word >> 6) & 0x3F, word & 0x3F);
            let dst_imm_ok = matches!(insn.mnemonic, "CMP" | "BIT" | "CMPB" | "BITB");
            let s = read(src, true)?;
            let d = read(dst, dst_imm_ok)?;
            format!("{mn} {s},{d}")
        }
        Class::Single => {
            let dst = word & 0x3F;
            // JMP / WRTLCK on a register (mode 0) are illegal — `asl` rejects them.
            if matches!(insn.mnemonic, "JMP" | "WRTLCK") && dst >> 3 == 0 {
                return None;
            }
            let d = read(dst, pdp11_single_imm_ok(insn.mnemonic))?;
            format!("{mn} {d}")
        }
        Class::Jsr => {
            let (reg, dst) = ((word >> 6) & 7, word & 0x3F);
            let d = read(dst, true)?;
            format!("{mn} r{reg},{d}")
        }
        Class::RegSrc => {
            let (reg, src) = ((word >> 6) & 7, word & 0x3F);
            let s = read(src, true)?;
            format!("{mn} {s},r{reg}")
        }
        Class::Xor => {
            let (reg, dst) = ((word >> 6) & 7, word & 0x3F);
            let d = read(dst, false)?;
            format!("{mn} r{reg},{d}")
        }
        Class::Rts => format!("{mn} r{}", word & 7),
        Class::Branch => {
            let off8 = i32::from(word & 0xFF) - if word & 0x80 != 0 { 256 } else { 0 };
            let target = instr_addr.wrapping_add(2).wrapping_add((2 * off8) as u16);
            format!("{mn} 0x{target:04X}")
        }
        Class::Sob => {
            let (reg, off6) = ((word >> 6) & 7, word & 0x3F);
            let target = instr_addr.wrapping_add(2).wrapping_sub(2 * off6);
            format!("{mn} r{reg},0x{target:04X}")
        }
        Class::Trap => format!("{mn} 0x{:02X}", word & 0xFF),
        Class::Mark => format!("{mn} 0x{:02X}", word & 0x3F),
        Class::Spl => format!("{mn} {}", word & 7),
        Class::NoArg => mn,
    };
    Some((text, off - pos))
}

// ---------------------------------------------------------------------------
// TI TMS9900
// ---------------------------------------------------------------------------

/// Disassemble a flat TMS9900 binary loaded at `origin`. The TMS9900 is
/// **big-endian** with 16-bit words; each instruction is one opcode word plus
/// 0–2 extension words (symbolic addresses, immediates). Undecodable words
/// render as `word` data, a trailing odd byte as `byte`.
#[must_use]
pub fn disassemble_tms9900(code: &[u8], origin: u16) -> Vec<Line> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < code.len() {
        let addr = origin.wrapping_add(pos as u16);
        if pos + 1 >= code.len() {
            out.push(Line {
                addr: u32::from(addr),
                bytes: vec![code[pos]],
                text: format!("byte 0{:02X}H", code[pos]),
            });
            pos += 1;
            continue;
        }
        let word = u16::from_be_bytes([code[pos], code[pos + 1]]);
        match decode_tms9900(code, pos, addr) {
            Some((text, len)) => {
                out.push(Line {
                    addr: u32::from(addr),
                    bytes: code[pos..pos + len].to_vec(),
                    text,
                });
                pos += len;
            }
            None => {
                out.push(Line {
                    addr: u32::from(addr),
                    bytes: code[pos..pos + 2].to_vec(),
                    text: format!("word 0{word:04X}H"),
                });
                pos += 2;
            }
        }
    }
    out
}

/// Render a TMS9900 disassembly as reassemblable `asl` source.
#[must_use]
pub fn listing_tms9900(code: &[u8], origin: u16) -> String {
    let mut s = format!("\tcpu TMS9900\n\torg 0{origin:04X}H\n");
    for line in disassemble_tms9900(code, origin) {
        s.push('\t');
        s.push_str(&line.text);
        s.push('\n');
    }
    s.push_str("\tend\n");
    s
}

/// Render a general-addressing operand from its 2-bit `T` mode and 4-bit
/// register, reading an absolute address word from `code` at `ext_off` for the
/// symbolic / indexed modes. Returns the text and extension words consumed
/// (0 or 1), or `None` if a needed word runs past the buffer.
fn tms9900_general(code: &[u8], ext_off: usize, t: u16, reg: u16) -> Option<(String, usize)> {
    Some(match t {
        0 => (format!("r{reg}"), 0),
        1 => (format!("*r{reg}"), 0),
        3 => (format!("*r{reg}+"), 0),
        _ => {
            if ext_off + 2 > code.len() {
                return None;
            }
            let addr = u16::from_be_bytes([code[ext_off], code[ext_off + 1]]);
            let s = if reg == 0 {
                format!("@0{addr:04X}H")
            } else {
                format!("@0{addr:04X}H(r{reg})")
            };
            (s, 1)
        }
    })
}

/// Decode one TMS9900 instruction at byte offset `pos`, `instr_addr` its load
/// address. Returns the reassemblable text and total byte length, or `None` for
/// an undecodable word (rendered as `word` data by the caller).
fn decode_tms9900(code: &[u8], pos: usize, instr_addr: u16) -> Option<(String, usize)> {
    use isa::tms9900::Class;
    let word = u16::from_be_bytes([code[pos], code[pos + 1]]);
    let insn = isa::tms9900::decode(word)?;
    let mn = insn.mnemonic.to_ascii_lowercase();
    // A 16-bit big-endian word at extension slot `ord`.
    let ext_word = |ord: usize| -> Option<u16> {
        let off = pos + 2 + 2 * ord;
        if off + 2 > code.len() {
            return None;
        }
        Some(u16::from_be_bytes([code[off], code[off + 1]]))
    };

    let text = match insn.class {
        Class::DualGeneral => {
            let (td, d) = ((word >> 10) & 3, (word >> 6) & 0xF);
            let (ts, s) = ((word >> 4) & 3, word & 0xF);
            let (src, sw) = tms9900_general(code, pos + 2, ts, s)?;
            let (dst, dw) = tms9900_general(code, pos + 2 + 2 * sw, td, d)?;
            return Some((format!("{mn} {src},{dst}"), 2 + 2 * (sw + dw)));
        }
        Class::DualRegDst => {
            let d = (word >> 6) & 0xF;
            let (src, sw) = tms9900_general(code, pos + 2, (word >> 4) & 3, word & 0xF)?;
            return Some((format!("{mn} {src},r{d}"), 2 + 2 * sw));
        }
        Class::Xop => {
            let n = (word >> 6) & 0xF;
            let (src, sw) = tms9900_general(code, pos + 2, (word >> 4) & 3, word & 0xF)?;
            return Some((format!("{mn} {src},{n}"), 2 + 2 * sw));
        }
        Class::CruMulti => {
            let c = (word >> 6) & 0xF;
            let count = if c == 0 { 16 } else { c };
            let (src, sw) = tms9900_general(code, pos + 2, (word >> 4) & 3, word & 0xF)?;
            return Some((format!("{mn} {src},{count}"), 2 + 2 * sw));
        }
        Class::Shift => {
            let (c, w) = ((word >> 4) & 0xF, word & 0xF);
            format!("{mn} r{w},{c}")
        }
        Class::SingleGeneral => {
            let (src, sw) = tms9900_general(code, pos + 2, (word >> 4) & 3, word & 0xF)?;
            return Some((format!("{mn} {src}"), 2 + 2 * sw));
        }
        Class::Control => mn,
        Class::ImmReg => {
            let imm = ext_word(0)?;
            return Some((format!("{mn} r{},0{imm:04X}H", word & 0xF), 4));
        }
        Class::ImmOnly => {
            let imm = ext_word(0)?;
            return Some((format!("{mn} 0{imm:04X}H"), 4));
        }
        Class::StoreReg => format!("{mn} r{}", word & 0xF),
        Class::Jump => {
            let off = i32::from(word & 0xFF) - if word & 0x80 != 0 { 256 } else { 0 };
            let target = instr_addr.wrapping_add(2).wrapping_add((2 * off) as u16);
            format!("{mn} 0{target:04X}H")
        }
        Class::Cru => {
            let disp = i32::from(word & 0xFF) - if word & 0x80 != 0 { 256 } else { 0 };
            format!("{mn} {disp}")
        }
    };
    Some((text, 2))
}

// ---------------------------------------------------------------------------
// GI CP1610 (Mattel Intellivision) — built as increments; see the crate
// `decisions/`. Increment 1: the single-decle register / implied groups.
// ---------------------------------------------------------------------------

/// Disassemble a flat CP1610 binary loaded at `origin`. The CP1610 is
/// **big-endian**; each 10-bit decle is stored as a 16-bit word (top six bits
/// zero). Groups not yet implemented (and undecodable words) render as `word`
/// data.
#[must_use]
pub fn disassemble_cp1610(code: &[u8], origin: u16) -> Vec<Line> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < code.len() {
        // The CP1610 is word-addressed: each decle is two bytes, so the address
        // advances by one per two bytes consumed.
        let addr = origin.wrapping_add((pos / 2) as u16);
        if pos + 1 >= code.len() {
            out.push(Line {
                addr: u32::from(addr),
                bytes: vec![code[pos]],
                text: format!("byte 0{:02X}H", code[pos]),
            });
            pos += 1;
            continue;
        }
        let word = u16::from_be_bytes([code[pos], code[pos + 1]]);
        match decode_cp1610(code, pos, addr) {
            Some((text, len)) => {
                out.push(Line {
                    addr: u32::from(addr),
                    bytes: code[pos..pos + len].to_vec(),
                    text,
                });
                pos += len;
            }
            None => {
                out.push(Line {
                    addr: u32::from(addr),
                    bytes: code[pos..pos + 2].to_vec(),
                    text: format!("word 0{word:04X}H"),
                });
                pos += 2;
            }
        }
    }
    out
}

/// Render a CP1610 disassembly as reassemblable `asl` source (`cpu CP-1600`).
///
/// `relaxed on` enables asl's Intel `h`-suffix hex in CP-1600 mode (which
/// otherwise takes only decimal and its `x'…'` hex form), keeping the emitted
/// numbers in the house-standard `0XXXXH` style shared with every other listing.
#[must_use]
pub fn listing_cp1610(code: &[u8], origin: u16) -> String {
    let mut s = format!("\tcpu CP-1600\n\trelaxed on\n\torg 0{origin:04X}H\n");
    for line in disassemble_cp1610(code, origin) {
        s.push('\t');
        s.push_str(&line.text);
        s.push('\n');
    }
    s.push_str("\tend\n");
    s
}

/// Decode one CP1610 instruction at byte offset `pos`, `addr` its (byte) load
/// address. Returns the reassemblable text and byte length, or `None` for an
/// undecodable word / a branch missing its magnitude (rendered as `word` data by
/// the caller).
fn decode_cp1610(code: &[u8], pos: usize, addr: u16) -> Option<(String, usize)> {
    use isa::cp1610::Class;
    let word = u16::from_be_bytes([code[pos], code[pos + 1]]);

    // Branch page (0x200–0x23F): a two-decle relative branch. The magnitude word
    // follows; the target is measured from two decles past the opcode, backward
    // branches biased by one (`EA = PC - mag - 1`).
    if word & 0x3C0 == 0x200 {
        if pos + 4 > code.len() {
            return None; // magnitude word runs past the buffer
        }
        let mag = u16::from_be_bytes([code[pos + 2], code[pos + 3]]);
        // Decle address two words past the opcode; backward is biased by one.
        let pc = addr.wrapping_add(2);
        let target = if word & 0x20 != 0 {
            pc.wrapping_sub(mag).wrapping_sub(1)
        } else {
            pc.wrapping_add(mag)
        };
        let text = if word & 0x10 != 0 {
            format!("bext 0{target:04X}H,{}", word & 0xF)
        } else if word & 0xF == 8 {
            // Branch-never (a two-word no-op). Only the canonical `NOPP`
            // (opcode 0x208, zero magnitude) round-trips — asl always emits that
            // form — so any other cond-8 shape is left as data.
            if word != 0x208 || mag != 0 {
                return None;
            }
            "nopp".to_string()
        } else {
            format!(
                "{} 0{target:04X}H",
                isa::cp1610::BRANCH_CONDS[(word & 0xF) as usize]
            )
        };
        return Some((text, 4));
    }

    // Memory region (0x240–0x3FF): `base | mm << 3 | reg`. `mm` picks the mode —
    // 0 direct (a following address word), 1–6 indirect `@R1`–`@R6`, 7 immediate
    // (a following value word). `MVO` stores, so its register operand comes first.
    if word >= 0x240 {
        let fam = isa::cp1610::mem_family_by_base(word & 0x3C0)?;
        let mn = fam.mnemonic.to_ascii_lowercase();
        let (mode, reg) = ((word >> 3) & 7, word & 7);
        return match mode {
            0 => {
                if pos + 4 > code.len() {
                    return None; // address word runs past the buffer
                }
                let a = u16::from_be_bytes([code[pos + 2], code[pos + 3]]);
                let text = if fam.store {
                    format!("{mn} r{reg},0{a:04X}H")
                } else {
                    format!("{mn} 0{a:04X}H,r{reg}")
                };
                Some((text, 4))
            }
            1..=6 => {
                let text = if fam.store {
                    format!("{mn}@ r{reg},r{mode}")
                } else {
                    format!("{mn}@ r{mode},r{reg}")
                };
                Some((text, 2))
            }
            _ => {
                // Mode 7: immediate (loads / ALU only). `MVO` has no immediate
                // form, so a store here is not a decodable instruction.
                if fam.store || pos + 4 > code.len() {
                    return None;
                }
                let imm = u16::from_be_bytes([code[pos + 2], code[pos + 3]]);
                Some((format!("{mn}i 0{imm:04X}H,r{reg}"), 4))
            }
        };
    }

    // Jump / call prefix (0x0004): a three-decle form. The second decle carries
    // the return register (bits 9:8), interrupt action (bits 1:0), and the address
    // high six bits (bits 7:2); the third carries the low ten bits.
    if word == 0x0004 {
        if pos + 6 > code.len() {
            return None; // the two following decles run past the buffer
        }
        let d2 = u16::from_be_bytes([code[pos + 2], code[pos + 3]]);
        let d3 = u16::from_be_bytes([code[pos + 4], code[pos + 5]]);
        let target = (((d2 >> 2) & 0x3F) << 10) | (d3 & 0x3FF);
        let (rr, ii) = ((d2 >> 8) & 3, d2 & 3);
        if ii > 2 {
            return None; // no interrupt action encodes as 3
        }
        let text = if rr == 3 {
            let mn = ["j", "je", "jd"][ii as usize];
            format!("{mn} 0{target:04X}H")
        } else {
            let mn = ["jsr", "jsre", "jsrd"][ii as usize];
            format!("{mn} r{},0{target:04X}H", rr + 4)
        };
        return Some((text, 6));
    }

    let insn = isa::cp1610::decode(word)?;
    let mn = insn.mnemonic.to_ascii_lowercase();
    let text = match insn.class {
        Class::Implied => mn,
        Class::RegUnary => format!("{mn} r{}", word & 7),
        Class::GetStatus => format!("{mn} r{}", word & 3),
        Class::RegReg => format!("{mn} r{},r{}", (word >> 3) & 7, word & 7),
        Class::Shift => {
            // Count is once (bit 2 clear) or twice (set); once is written bare.
            if word & 0x4 != 0 {
                format!("{mn} r{},2", word & 3)
            } else {
                format!("{mn} r{}", word & 3)
            }
        }
    };
    Some((text, 2))
}

// ---------------------------------------------------------------------------
// Zilog Z8000 (non-segmented Z8002) — built as increments; see
// `decisions/z8000-staged-build.md`. Increment 1: the dyadic family.
// ---------------------------------------------------------------------------

/// Disassemble a flat Z8000 (non-segmented Z8002) binary loaded at `origin`. The
/// Z8000 is **big-endian** with 16-bit words. Instruction groups not yet
/// implemented (and undecodable words) render as `word` data.
#[must_use]
pub fn disassemble_z8000(code: &[u8], origin: u16) -> Vec<Line> {
    disassemble_z8000_impl(code, origin, false)
}

/// Disassemble a flat **segmented Z8001** binary — like [`disassemble_z8000`]
/// but with segmented memory operands (`<<seg>>offset` addresses, `@RRn`
/// pointers).
#[must_use]
pub fn disassemble_z8001(code: &[u8], origin: u16) -> Vec<Line> {
    disassemble_z8000_impl(code, origin, true)
}

fn disassemble_z8000_impl(code: &[u8], origin: u16, seg: bool) -> Vec<Line> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < code.len() {
        let addr = origin.wrapping_add(pos as u16);
        if pos + 1 >= code.len() {
            out.push(Line {
                addr: u32::from(addr),
                bytes: vec![code[pos]],
                text: format!("byte 0{:02X}H", code[pos]),
            });
            pos += 1;
            continue;
        }
        let word = u16::from_be_bytes([code[pos], code[pos + 1]]);
        match decode_ctl_z8000(code, pos, addr, seg)
            .or_else(|| decode_mono_z8000(code, pos, seg))
            .or_else(|| decode_stack_z8000(code, pos, seg))
            .or_else(|| decode_shift_z8000(code, pos))
            .or_else(|| decode_exts_z8000(code, pos))
            .or_else(|| decode_bit_z8000(code, pos, seg))
            .or_else(|| decode_muldiv_z8000(code, pos, seg))
            .or_else(|| decode_block_z8000(code, pos, seg))
            .or_else(|| decode_io_z8000(code, pos, seg))
            .or_else(|| decode_control_z8000(code, pos, seg))
            .or_else(|| decode_misc_z8000(code, pos, addr))
            .or_else(|| decode_z8000(code, pos, seg))
        {
            Some((text, len)) => {
                out.push(Line {
                    addr: u32::from(addr),
                    bytes: code[pos..pos + len].to_vec(),
                    text,
                });
                pos += len;
            }
            None => {
                out.push(Line {
                    addr: u32::from(addr),
                    bytes: code[pos..pos + 2].to_vec(),
                    text: format!("word 0{word:04X}H"),
                });
                pos += 2;
            }
        }
    }
    out
}

/// Render a non-segmented Z8002 disassembly as reassemblable `asl` source.
#[must_use]
pub fn listing_z8000(code: &[u8], origin: u16) -> String {
    listing_z8000_impl(code, origin, false)
}

/// Render a segmented Z8001 disassembly as reassemblable `asl` source.
#[must_use]
pub fn listing_z8001(code: &[u8], origin: u16) -> String {
    listing_z8000_impl(code, origin, true)
}

fn listing_z8000_impl(code: &[u8], origin: u16, seg: bool) -> String {
    // `supmode on` lets `asl` assemble the privileged I/O group; harmless for
    // every other instruction.
    let cpu = if seg { "Z8001" } else { "Z8002" };
    let mut s = format!("\tcpu {cpu}\n\tsupmode on\n\torg 0{origin:04X}H\n");
    let lines = if seg {
        disassemble_z8001(code, origin)
    } else {
        disassemble_z8000(code, origin)
    };
    for line in lines {
        s.push('\t');
        s.push_str(&line.text);
        s.push('\n');
    }
    s.push_str("\tend\n");
    s
}

/// Read a Z8000 direct / indexed address operand starting at byte `at`: a 16-bit
/// address (non-segmented) or a two-word long-form `<<seg>>offset` (segmented).
/// Returns the rendered address and its byte length, or `None` if it runs past
/// the end or (segmented) is not a canonical long-form segment word.
fn z8000_addr(code: &[u8], at: usize, seg: bool) -> Option<(String, usize)> {
    if seg {
        if at + 4 > code.len() {
            return None;
        }
        let w1 = u16::from_be_bytes([code[at], code[at + 1]]);
        // Long form: bit 15 set, segment in bits 14–8, low byte zero.
        if w1 & 0x80FF != 0x8000 {
            return None;
        }
        let s = (w1 >> 8) & 0x7F;
        let off = u16::from_be_bytes([code[at + 2], code[at + 3]]);
        Some((format!("<<{s}>>0{off:04X}H"), 4))
    } else {
        if at + 2 > code.len() {
            return None;
        }
        let a = u16::from_be_bytes([code[at], code[at + 1]]);
        Some((format!("0{a:04X}H"), 2))
    }
}

/// A memory pointer register: `@Rn` (non-segmented) or `@RRn` (a segmented long
/// pair, which must be an **even** register — an odd field is `None`, not a
/// canonical encoding).
fn z8000_ptr(field: u16, seg: bool) -> Option<String> {
    if seg {
        field.is_multiple_of(2).then(|| format!("@rr{field}"))
    } else {
        Some(format!("@r{field}"))
    }
}

/// A Z8000 register name for a given operand [`Size`]: `rN` (word/address),
/// `rhN`/`rlN` (byte), or `rrN` (long).
fn z8000_reg(n: u16, size: isa::z8000::Size) -> String {
    use isa::z8000::Size;
    match size {
        Size::Byte if n < 8 => format!("rh{n}"),
        Size::Byte => format!("rl{}", n - 8),
        Size::Long => format!("rr{n}"),
        Size::Quad => format!("rq{n}"),
        Size::Word | Size::Address => format!("r{n}"),
    }
}

/// A condition-code prefix (`"eq,"`), or empty for the always-true code.
fn z8000_cc_prefix(cc: u8) -> String {
    isa::z8000::cc_name(cc).map_or_else(String::new, |n| format!("{n},"))
}

/// Decode one Z8000 program-control instruction (`JP`/`CALL`/`JR`/`RET`/`DJNZ`/
/// `CALR`) at `pos`, `addr` its load address (for the relative targets), or
/// `None` if the word is not one of them.
fn decode_ctl_z8000(code: &[u8], pos: usize, addr: u16, seg: bool) -> Option<(String, usize)> {
    use isa::z8000::{DA, IR, Size, X, mode_of};
    let word = u16::from_be_bytes([code[pos], code[pos + 1]]);
    let (top, second) = ((word >> 8) as u8, word & 0xFF);

    match top >> 4 {
        // JR cc, addr — word-scaled signed 8-bit, target = PC + 2·disp.
        0xE => {
            let disp = i32::from(second as i8);
            let target = addr.wrapping_add(2).wrapping_add((2 * disp) as u16);
            Some((
                format!("jr {}0{target:04X}H", z8000_cc_prefix(top & 0xF)),
                2,
            ))
        }
        // DJNZ / DBJNZ r, addr — 7-bit backward, target = PC − 2·disp.
        0xF => {
            let (reg, w, disp) = (u16::from(top & 0xF), second >> 7, i32::from(second & 0x7F));
            let target = addr.wrapping_add(2).wrapping_sub((2 * disp) as u16);
            let (mn, rn) = if w == 1 {
                ("djnz", format!("r{reg}"))
            } else {
                ("dbjnz", z8000_reg(reg, Size::Byte))
            };
            Some((format!("{mn} {rn},0{target:04X}H"), 2))
        }
        // CALR addr — 12-bit backward, target = PC − 2·disp.
        0xD => {
            let raw = (u16::from(top & 0xF) << 8) | second;
            let disp = if raw & 0x800 != 0 {
                i32::from(raw) - 0x1000
            } else {
                i32::from(raw)
            };
            let target = addr.wrapping_add(2).wrapping_sub((2 * disp) as u16);
            Some((format!("calr 0{target:04X}H"), 2))
        }
        _ => {
            // RET cc — 0x9E0X.
            if top == 0x9E && second < 16 {
                let ret = isa::z8000::cc_name(second as u8)
                    .map_or_else(|| "ret".to_string(), |n| format!("ret {n}"));
                return Some((ret, 2));
            }
            // JP cc, dst / CALL dst — a memory operand, cc in the low nibble (JP).
            let base6 = top & 0x3F;
            if base6 != 0x1E && base6 != 0x1F {
                return None;
            }
            let is_jp = base6 == 0x1E;
            let field = second >> 4;
            let mode = mode_of(top >> 6, field);
            if mode & (IR | DA | X) == 0 {
                return None;
            }
            let cc_low = second & 0xF;
            if !is_jp && cc_low != 0 {
                return None; // CALL has no condition code
            }
            let (dst, len) = match mode {
                IR => (z8000_ptr(field, seg)?, 2),
                DA => {
                    let (a, n) = z8000_addr(code, pos + 2, seg)?;
                    (a, 2 + n)
                }
                X => {
                    let (a, n) = z8000_addr(code, pos + 2, seg)?;
                    (format!("{a}(r{field})"), 2 + n)
                }
                _ => return None,
            };
            let (mn, pre) = if is_jp {
                ("jp", z8000_cc_prefix(cc_low as u8))
            } else {
                ("call", String::new())
            };
            Some((format!("{mn} {pre}{dst}"), len))
        }
    }
}

/// Decode one Z8000 single-operand ALU instruction (`CLR`/`COM`/`NEG`/`TEST`/
/// `TSET`, `INC`/`DEC`) at `pos`, or `None`. The operand is the second byte's
/// high nibble; the low nibble is a sub-opcode or a count.
fn decode_mono_z8000(code: &[u8], pos: usize, seg: bool) -> Option<(String, usize)> {
    use isa::z8000::{DA, IR, R, X, mode_of};
    let word = u16::from_be_bytes([code[pos], code[pos + 1]]);
    let (top, second) = ((word >> 8) as u8, word & 0xFF);
    let (field, low) = (second >> 4, (second & 0xF) as u8);
    let m = isa::z8000::mono_decode(top, low)?;
    let mn = m.mnemonic.to_ascii_lowercase();
    let mode = mode_of(top >> 6, field);

    let (operand, len) = match mode {
        R => (z8000_reg(field, m.size), 2),
        IR => (z8000_ptr(field, seg)?, 2),
        DA => {
            let (a, n) = z8000_addr(code, pos + 2, seg)?;
            (a, 2 + n)
        }
        X => {
            let (a, n) = z8000_addr(code, pos + 2, seg)?;
            (format!("{a}(r{field})"), 2 + n)
        }
        _ => return None, // IM is not a valid single-operand mode
    };
    if m.count {
        Some((format!("{mn} {operand},#{}", u16::from(low) + 1), len))
    } else {
        Some((format!("{mn} {operand}"), len))
    }
}

/// Decode one Z8000 stack instruction (`PUSH`/`POP`/`PUSHL`/`POPL`) at `pos`, or
/// `None`. The stack pointer is the second byte's high nibble, the value
/// operand's field the low nibble.
fn decode_stack_z8000(code: &[u8], pos: usize, seg: bool) -> Option<(String, usize)> {
    use isa::z8000::{DA, IR, R, X, mode_of};
    let word = u16::from_be_bytes([code[pos], code[pos + 1]]);
    let (top, second) = ((word >> 8) as u8, word & 0xFF);
    let (sp, low) = (second >> 4, second & 0xF);
    if sp == 0 {
        return None; // R0 cannot be a stack pointer
    }
    let sp_ptr = z8000_ptr(sp, seg)?;
    let addr16 =
        || (pos + 4 <= code.len()).then(|| u16::from_be_bytes([code[pos + 2], code[pos + 3]]));

    // Special: PUSH @Rsp, #imm — MM = 00, so the whole top byte is 0x0D. The
    // immediate is a word even in segmented mode.
    if top == isa::z8000::PUSH_IMM_BASE6 && low == 9 {
        return Some((format!("push {sp_ptr},#0{:04X}H", addr16()?), 4));
    }

    let s = isa::z8000::stack_decode(top)?;
    let mode = mode_of(top >> 6, low);
    // A long value register (PUSHL/POPL, R mode) must be an even pair.
    if s.size == isa::z8000::Size::Long && mode == R && low % 2 == 1 {
        return None;
    }
    let (value, len) = match mode {
        R => (z8000_reg(low, s.size), 2),
        IR => (z8000_ptr(low, seg)?, 2),
        DA => {
            let (a, n) = z8000_addr(code, pos + 2, seg)?;
            (a, 2 + n)
        }
        X => {
            let (a, n) = z8000_addr(code, pos + 2, seg)?;
            (format!("{a}(r{low})"), 2 + n)
        }
        _ => return None, // no immediate at these base6 (PUSH #imm is separate)
    };
    let text = if s.push {
        format!("{} {sp_ptr},{value}", s.mnemonic.to_ascii_lowercase())
    } else {
        format!("{} {value},{sp_ptr}", s.mnemonic.to_ascii_lowercase())
    };
    Some((text, len))
}

/// Decode one Z8000 shift or rotate (`SLA`/`SRA`/`SLL`/`SRL`, `RL`/`RR`/`RLC`/
/// `RRC`, + byte/long) at `pos`, or `None`. Both key on top byte `0xB2` (byte) /
/// `0xB3` (word/long) with the operand register in the second byte's high
/// nibble; the low nibble's bit 0 tells shift (1) from rotate (0). A shift takes
/// a trailing signed count word (its sign selecting left/right); a rotate packs
/// its count.
fn decode_shift_z8000(code: &[u8], pos: usize) -> Option<(String, usize)> {
    use isa::z8000::Size;
    let (top, second) = (code[pos], code[pos + 1]);
    // The group is `MM` = 10 (register), so the top byte is exactly `0xB2`
    // (byte) or `0xB3` (word/long) — not merely `base6` 0x32/0x33 with any `MM`.
    if top != 0xB2 && top != 0xB3 {
        return None;
    }
    let base6 = top & 0x3F;
    let (reg, low) = (u16::from(second >> 4), second & 0xF);

    if low & 1 == 1 {
        // Shift: a count word follows, its sign selecting left (≥ 0) / right (< 0).
        if pos + 4 > code.len() {
            return None;
        }
        // The size (from the sub-opcode, shared by both directions) fixes the
        // count's width: a byte shift's count is the signed low byte (high byte
        // zero); a word / long shift's is the full 16-bit signed word.
        let size = isa::z8000::shift_decode(base6, low, false)?.size;
        if size == Size::Long && reg % 2 == 1 {
            return None; // a long shift takes an even register pair
        }
        let (hi, lo) = (code[pos + 2], code[pos + 3]);
        let count = if size == Size::Byte {
            if hi != 0 {
                return None; // a byte count occupies the low byte only
            }
            i64::from(lo as i8)
        } else {
            i64::from(i16::from_be_bytes([hi, lo]))
        };
        let max = isa::z8000::shift_max(size);
        if count > max || count < -max {
            return None; // out of range for this size — `asl` would reject it
        }
        let sh = isa::z8000::shift_decode(base6, low, count < 0)?;
        Some((
            format!(
                "{} {},#{}",
                sh.mnemonic.to_ascii_lowercase(),
                z8000_reg(reg, size),
                count.abs()
            ),
            4,
        ))
    } else {
        // Rotate: the type is the low nibble's high bits, the count its bit 1.
        let sh = isa::z8000::rotate_decode(base6, low >> 2)?;
        let count = u16::from((low >> 1) & 1) + 1;
        Some((
            format!(
                "{} {},#{count}",
                sh.mnemonic.to_ascii_lowercase(),
                z8000_reg(reg, sh.size)
            ),
            2,
        ))
    }
}

/// Decode one Z8000 sign-extend (`EXTSB`/`EXTS`/`EXTSL`) at `pos`, or `None`. The
/// top byte is `0xB1`; the operand register is the second byte's high nibble and
/// the sub-opcode its low nibble.
fn decode_exts_z8000(code: &[u8], pos: usize) -> Option<(String, usize)> {
    use isa::z8000::Size;
    let (top, second) = (code[pos], code[pos + 1]);
    if top != isa::z8000::EXTEND_TOP {
        return None;
    }
    let (reg, subop) = (u16::from(second >> 4), second & 0xF);
    let e = isa::z8000::extend_decode(subop)?;
    // A long pair must be even; a quad a multiple of four.
    match e.size {
        Size::Long if reg % 2 == 1 => return None,
        Size::Quad if reg % 4 != 0 => return None,
        _ => {}
    }
    Some((
        format!(
            "{} {}",
            e.mnemonic.to_ascii_lowercase(),
            z8000_reg(reg, e.size)
        ),
        2,
    ))
}

/// Decode one Z8000 bit instruction (`BIT`/`SET`/`RES` + byte) at `pos`, or
/// `None`. `MM` and the second byte's high nibble select the form exactly as the
/// dyadic family does, with the low nibble a **bit number**; `MM` = 00 with a
/// high nibble of zero is the two-word **dynamic** form (the bit number in a
/// word register, the target register in word 2).
fn decode_bit_z8000(code: &[u8], pos: usize, seg: bool) -> Option<(String, usize)> {
    let (top, second) = (code[pos], code[pos + 1]);
    let b = isa::z8000::bit_decode(top)?;
    let (field, low) = (u16::from(second >> 4), second & 0xF);
    let mn = b.mnemonic.to_ascii_lowercase();
    let bmax = isa::z8000::bit_max(b.size) as u16;

    match top >> 6 {
        // Static register (`MM` = 10).
        2 => {
            if u16::from(low) > bmax {
                return None; // bit number out of range for this size
            }
            Some((format!("{mn} {},#{low}", z8000_reg(field, b.size)), 2))
        }
        // `MM` = 00: static `@Rn` (pointer 1–15) or the dynamic form (nibble 0).
        0 => {
            if field == 0 {
                // Dynamic: word 2 is `target << 8` (registers only — never a
                // segmented address); anything else is not a canonical encoding.
                if pos + 4 > code.len() {
                    return None;
                }
                let w2 = u16::from_be_bytes([code[pos + 2], code[pos + 3]]);
                if w2 & 0xF0FF != 0 {
                    return None;
                }
                let target = (w2 >> 8) & 0xF;
                Some((format!("{mn} {},r{low}", z8000_reg(target, b.size)), 4))
            } else {
                if u16::from(low) > bmax {
                    return None;
                }
                Some((format!("{mn} {},#{low}", z8000_ptr(field, seg)?), 2))
            }
        }
        // Static direct / indexed (`MM` = 01).
        1 => {
            if u16::from(low) > bmax {
                return None;
            }
            let (a, n) = z8000_addr(code, pos + 2, seg)?;
            let target = if field == 0 {
                a
            } else {
                format!("{a}(r{field})")
            };
            Some((format!("{mn} {target},#{low}"), 2 + n))
        }
        _ => None, // `MM` = 11 is not a bit op
    }
}

/// Decode one Z8000 multiply / divide (`MULT`/`MULTL`/`DIV`/`DIVL`) at `pos`, or
/// `None`. Dyadic-shaped, but the destination accumulator is double-width (long
/// `rr` / quad `rq`) and the source one size smaller (word / long).
fn decode_muldiv_z8000(code: &[u8], pos: usize, seg: bool) -> Option<(String, usize)> {
    use isa::z8000::{DA, IM, IR, R, Size, X, mode_of, reg_aligned};
    let (top, second) = (code[pos], code[pos + 1]);
    let md = isa::z8000::muldiv_decode(top)?;
    let field = u16::from(second >> 4);
    let dest = u16::from(second & 0xF);
    let mode = mode_of(top >> 6, field);
    // The accumulator must be an aligned register, and a register source too.
    if !reg_aligned(dest, md.dest) {
        return None;
    }
    if mode == R && !reg_aligned(field, md.src) {
        return None;
    }
    let mn = md.mnemonic.to_ascii_lowercase();
    let dst = z8000_reg(dest, md.dest);
    let addr16 =
        || (pos + 4 <= code.len()).then(|| u16::from_be_bytes([code[pos + 2], code[pos + 3]]));

    let (src, len) = match mode {
        R => (z8000_reg(field, md.src), 2),
        IR => (z8000_ptr(field, seg)?, 2),
        DA => {
            let (a, n) = z8000_addr(code, pos + 2, seg)?;
            (a, 2 + n)
        }
        X => {
            let (a, n) = z8000_addr(code, pos + 2, seg)?;
            (format!("{a}(r{field})"), 2 + n)
        }
        IM => match md.src {
            Size::Long => {
                if pos + 6 > code.len() {
                    return None;
                }
                let hi = u32::from(addr16()?);
                let lo = u32::from(u16::from_be_bytes([code[pos + 4], code[pos + 5]]));
                (format!("#0{:08X}H", (hi << 16) | lo), 6)
            }
            _ => (format!("#0{:04X}H", addr16()?), 4),
        },
        _ => return None,
    };
    Some((format!("{mn} {dst},{src}"), len))
}

/// Decode one Z8000 block / string instruction (`LDx`/`CPx`/`CPSx`/`TRxB`/
/// `TRTxB`) at `pos`, or `None`. A two-word form: word 1 carries one pointer and
/// the operation nibble, word 2 the count register, the other pointer / data
/// register, and the control nibble. Word 2's top nibble must be zero.
fn decode_block_z8000(code: &[u8], pos: usize, seg: bool) -> Option<(String, usize)> {
    use isa::z8000::BlockShape;
    if pos + 4 > code.len() {
        return None;
    }
    let (top, w1_second) = (code[pos], code[pos + 1]);
    let word2 = u16::from_be_bytes([code[pos + 2], code[pos + 3]]);
    if word2 >> 12 != 0 {
        return None; // word 2's top nibble is always zero
    }
    let op_nib = w1_second & 0xF;
    let ctrl = (word2 & 0xF) as u8;
    let b = isa::z8000::block_decode(top, op_nib, ctrl)?;
    let field1 = u16::from(w1_second >> 4); // word 1's pointer
    let field2 = (word2 >> 4) & 0xF; // word 2's pointer / data register
    let count = (word2 >> 8) & 0xF;
    let p1 = z8000_ptr(field1, seg)?; // word 1 is always a pointer
    let mn = b.mnemonic.to_ascii_lowercase();
    let cc = if b.has_cc() {
        isa::z8000::cc_name(ctrl).map_or_else(String::new, |n| format!(",{n}"))
    } else {
        String::new()
    };

    let text = match b.shape {
        // Source in word 1, destination pointer in word 2.
        BlockShape::Load | BlockShape::CompareString => {
            format!("{mn} {},{p1},r{count}{cc}", z8000_ptr(field2, seg)?)
        }
        // Source in word 1, data register in word 2 (not a pointer).
        BlockShape::Compare => {
            format!("{mn} {},{p1},r{count}{cc}", z8000_reg(field2, b.size))
        }
        // Destination in word 1, source in word 2 (the reverse of `LDx`).
        BlockShape::Translate => format!("{mn} {p1},{},r{count}", z8000_ptr(field2, seg)?),
    };
    Some((text, 4))
}

/// Decode one Z8000 I/O instruction (simple `IN`/`OUT`/`SIN`/`SOUT` + byte, or
/// the block-I/O repeat group) at `pos`, or `None`. All are `MM` = 00: top
/// `0x3B`/`0x3A` is a direct-port simple I/O (word-1 low nibble 4–7) or block
/// I/O (a two-word Load form, low nibble 0–3/8–B); `0x3C`–`0x3F` is an indirect
/// (`@Rn`-port) simple I/O.
fn decode_io_z8000(code: &[u8], pos: usize, seg: bool) -> Option<(String, usize)> {
    use isa::z8000::Size;
    let top = code[pos];
    match top {
        0x3A | 0x3B => {
            let size = if top == 0x3B { Size::Word } else { Size::Byte };
            let second = code[pos + 1];
            let low = second & 0xF;
            if (4..=7).contains(&low) {
                // Direct-port simple I/O: register in the high nibble, then a
                // port address word.
                if pos + 4 > code.len() {
                    return None;
                }
                let sio = isa::z8000::simple_io_direct(size, low)?;
                let reg = z8000_reg(u16::from(second >> 4), size);
                let port = u16::from_be_bytes([code[pos + 2], code[pos + 3]]);
                let mn = sio.mnemonic.to_ascii_lowercase();
                let text = if sio.input {
                    format!("{mn} {reg},0{port:04X}H")
                } else {
                    format!("{mn} 0{port:04X}H,{reg}")
                };
                Some((text, 4))
            } else {
                // Block I/O: a two-word Load form (`@Rd, @Rs, Rc`).
                if pos + 4 > code.len() {
                    return None;
                }
                let word2 = u16::from_be_bytes([code[pos + 2], code[pos + 3]]);
                if word2 >> 12 != 0 {
                    return None;
                }
                let bio = isa::z8000::block_io_decode(size, low, (word2 & 0xF) as u8)?;
                let src = u16::from(second >> 4);
                let dst = (word2 >> 4) & 0xF;
                let count = (word2 >> 8) & 0xF;
                // The memory pointer is `@rr` in segmented mode; the I/O pointer
                // stays `@r`. `op_nib` bit 1 marks output (memory is the source).
                let (dst_seg, src_seg) = if low & 2 != 0 {
                    (false, seg)
                } else {
                    (seg, false)
                };
                Some((
                    format!(
                        "{} {},{},r{count}",
                        bio.mnemonic.to_ascii_lowercase(),
                        z8000_ptr(dst, dst_seg)?,
                        z8000_ptr(src, src_seg)?
                    ),
                    4,
                ))
            }
        }
        // Indirect (`@Rn`-port) simple I/O: port in the high nibble, register in
        // the low.
        0x3C..=0x3F => {
            let sio = isa::z8000::simple_io_indirect(top)?;
            let second = code[pos + 1];
            let port = u16::from(second >> 4);
            if port == 0 {
                return None; // R0 is not a legal base register
            }
            let reg = z8000_reg(u16::from(second & 0xF), sio.size);
            let mn = sio.mnemonic.to_ascii_lowercase();
            let text = if sio.input {
                format!("{mn} {reg},@r{port}")
            } else {
                format!("{mn} @r{port},{reg}")
            };
            Some((text, 2))
        }
        _ => None,
    }
}

/// Render a Z8000 flag mask (`SETFLG`/`RESFLG`/`COMFLG`) in canonical `C,Z,S,P`
/// order.
fn z8000_flags(mask: u8) -> String {
    isa::z8000::FLAG_BITS
        .iter()
        .filter(|(bit, _)| mask & bit != 0)
        .map(|(_, n)| *n)
        .collect::<Vec<_>>()
        .join(",")
}

/// Decode one Z8000 CPU-control instruction (`NOP`/`HALT`/`EI`/`DI`/`IRET`/
/// `LDCTL`/`LDPS`/`MSET`/`MRES`/`MBIT`/`MREQ`/`SETFLG`/`RESFLG`/`COMFLG`/`SC`) at
/// `pos`, or `None`. Each sub-group has a distinct top byte.
fn decode_control_z8000(code: &[u8], pos: usize, seg: bool) -> Option<(String, usize)> {
    use isa::z8000::Size;
    let (top, second) = (code[pos], code[pos + 1]);
    let (hi, low) = (u16::from(second >> 4), second & 0xF);
    match top {
        0x7A => (second == 0).then(|| ("halt".to_string(), 2)),
        0x7B => match second {
            0x00 => Some(("iret".to_string(), 2)),
            0x08 => Some(("mset".to_string(), 2)),
            0x09 => Some(("mres".to_string(), 2)),
            0x0A => Some(("mbit".to_string(), 2)),
            _ if low == 0xD => Some((format!("mreq r{hi}"), 2)),
            _ => None,
        },
        // EI / DI: bit 2 is enable; the low two bits mark the *excluded*
        // interrupts (bit 1 = vi, bit 0 = nvi). Both excluded / high bits set
        // are not canonical.
        0x7C => {
            if second & 0xF8 != 0 || second & 3 == 3 {
                return None;
            }
            let mn = if second & 4 != 0 { "ei" } else { "di" };
            let mut ints = Vec::new();
            if second & 2 == 0 {
                ints.push("vi");
            }
            if second & 1 == 0 {
                ints.push("nvi");
            }
            Some((format!("{mn} {}", ints.join(",")), 2))
        }
        // LDCTL word: register in the high nibble, control-register code (with a
        // store bit) in the low.
        0x7D => {
            let name = isa::z8000::word_ctrl_name(low & 7, seg)?;
            let reg = format!("r{hi}");
            let text = if low & 8 != 0 {
                format!("ldctl {name},{reg}")
            } else {
                format!("ldctl {reg},{name}")
            };
            Some((text, 2))
        }
        0x7F => Some((format!("sc #0{second:02X}H"), 2)),
        // LDCTLB byte: FLAGS only (low nibble 1 load / 9 store).
        0x8C => {
            let store = match low {
                1 => false,
                9 => true,
                _ => return None,
            };
            let reg = z8000_reg(hi, Size::Byte);
            let text = if store {
                format!("ldctlb flags,{reg}")
            } else {
                format!("ldctlb {reg},flags")
            };
            Some((text, 2))
        }
        // Flag ops (subop 1/3/5) and NOP (subop 7, mask 0) share top 0x8D.
        0x8D => {
            let mask = second >> 4;
            match low {
                7 => (mask == 0).then(|| ("nop".to_string(), 2)),
                1 | 3 | 5 if mask != 0 => {
                    let mn = match low {
                        1 => "setflg",
                        3 => "resflg",
                        _ => "comflg",
                    };
                    Some((format!("{mn} {}", z8000_flags(mask)), 2))
                }
                _ => None,
            }
        }
        // LDPS: indirect (`@Rn`) at 0x39, direct / indexed at 0x79.
        0x39 => {
            if low == 0 && hi != 0 {
                Some((format!("ldps {}", z8000_ptr(hi, seg)?), 2))
            } else {
                None
            }
        }
        0x79 => {
            if low != 0 {
                return None;
            }
            let (a, n) = z8000_addr(code, pos + 2, seg)?;
            let text = if hi == 0 {
                format!("ldps {a}")
            } else {
                format!("ldps {a}(r{hi})")
            };
            Some((text, 2 + n))
        }
        _ => None,
    }
}

/// Decode one Z8000 miscellaneous instruction (`TCC`/`TCCB`, `LDK`, `RLDB`/
/// `RRDB`, `LDR`/`LDRB`/`LDRL`) at `pos`, `addr` its load address (for the
/// PC-relative `LDR` target), or `None`.
fn decode_misc_z8000(code: &[u8], pos: usize, addr: u16) -> Option<(String, usize)> {
    use isa::z8000::{MiscKind, Size};
    let (m, store) = isa::z8000::misc_decode(code[pos])?;
    let second = code[pos + 1];
    let (hi, low) = (u16::from(second >> 4), u16::from(second & 0xF));
    let mn = m.mnemonic.to_ascii_lowercase();
    match m.kind {
        // Register in the high nibble, condition code in the low (omitted when
        // "always").
        MiscKind::Tcc => {
            let reg = z8000_reg(hi, m.size);
            let text = match isa::z8000::cc_name(low as u8) {
                Some(c) => format!("{mn} {c},{reg}"),
                None => format!("{mn} {reg}"),
            };
            Some((text, 2))
        }
        MiscKind::Ldk => Some((format!("{mn} r{hi},#{low}"), 2)),
        // Source in the high nibble, destination in the low (byte registers).
        MiscKind::Rotdig => Some((
            format!(
                "{mn} {},{}",
                z8000_reg(low, Size::Byte),
                z8000_reg(hi, Size::Byte)
            ),
            2,
        )),
        // Register in the low nibble, then a signed `target − (PC + 4)` offset.
        MiscKind::Ldr => {
            if pos + 4 > code.len() {
                return None;
            }
            let reg = z8000_reg(low, m.size);
            let off = i16::from_be_bytes([code[pos + 2], code[pos + 3]]);
            let target = addr.wrapping_add(4).wrapping_add(off as u16);
            let text = if store {
                format!("{mn} 0{target:04X}H,{reg}")
            } else {
                format!("{mn} {reg},0{target:04X}H")
            };
            Some((text, 4))
        }
    }
}

/// Decode one Z8000 dyadic instruction at byte offset `pos`. Returns the
/// reassemblable text and total byte length, or `None` for a word this
/// increment doesn't decode (rendered as `word` data by the caller).
fn decode_z8000(code: &[u8], pos: usize, seg: bool) -> Option<(String, usize)> {
    use isa::z8000::{DA, IM, IR, R, Size, X};
    let word = u16::from_be_bytes([code[pos], code[pos + 1]]);
    let (top, second) = ((word >> 8) as u8, word & 0xFF);
    let field = second >> 4;
    let insn = isa::z8000::decode(top, field)?;
    let mn = insn.mnemonic.to_ascii_lowercase();
    let mode = isa::z8000::mode_of(top >> 6, field);
    let reg = second & 0xF; // dest reg (load) or source reg (store)

    // Long operands are register pairs, so their register numbers must be even;
    // an odd field is not a canonical encoding (the low-nibble register always,
    // and the high-nibble register too when it is itself a long register in R
    // mode). `asl` rejects the odd forms, so decode them as data.
    if insn.size == Size::Long && (reg % 2 == 1 || (mode == R && field % 2 == 1)) {
        return None;
    }
    // In segmented mode `LDA` loads a 32-bit address into a long pair, so its
    // destination register is named as (and must be) an even pair.
    let dest_size = if seg && insn.size == Size::Address {
        if reg % 2 == 1 {
            return None;
        }
        Size::Long
    } else {
        insn.size
    };

    // Read the extension word(s): a 16-bit address / immediate, or a 32-bit long
    // immediate. Returns the rendered source text and the total instruction len.
    let addr16 = || -> Option<u16> {
        (pos + 4 <= code.len()).then(|| u16::from_be_bytes([code[pos + 2], code[pos + 3]]))
    };

    if insn.store {
        // Register → memory: `field` is the pointer/index, `reg` the source.
        let src = z8000_reg(reg, insn.size);
        let (dst, len) = match mode {
            IR => (z8000_ptr(field, seg)?, 2),
            DA => {
                let (a, n) = z8000_addr(code, pos + 2, seg)?;
                (a, 2 + n)
            }
            X => {
                let (a, n) = z8000_addr(code, pos + 2, seg)?;
                (format!("{a}(r{field})"), 2 + n)
            }
            _ => return None,
        };
        return Some((format!("{mn} {dst},{src}"), len));
    }

    // Source into register: `field` is the source field, `reg` the destination.
    let dst = z8000_reg(reg, dest_size);
    let (src, len) = match mode {
        R => (z8000_reg(field, insn.size), 2),
        IR => (z8000_ptr(field, seg)?, 2),
        DA => {
            let (a, n) = z8000_addr(code, pos + 2, seg)?;
            (a, 2 + n)
        }
        X => {
            let (a, n) = z8000_addr(code, pos + 2, seg)?;
            (format!("{a}(r{field})"), 2 + n)
        }
        IM => match insn.size {
            Size::Byte => {
                // Byte immediates replicate the byte; halves that differ are not
                // a canonical encoding, so treat as data.
                let imm = addr16()?;
                if imm >> 8 != imm & 0xFF {
                    return None;
                }
                (format!("#0{:02X}H", imm & 0xFF), 4)
            }
            Size::Long => {
                if pos + 6 > code.len() {
                    return None;
                }
                let hi = u32::from(addr16()?);
                let lo = u32::from(u16::from_be_bytes([code[pos + 4], code[pos + 5]]));
                (format!("#0{:08X}H", (hi << 16) | lo), 6)
            }
            _ => (format!("#0{:04X}H", addr16()?), 4),
        },
        _ => return None,
    };
    Some((format!("{mn} {dst},{src}"), len))
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
    fn m68k_dynamic_btst_immediate_decodes() {
        // Dynamic `BTST Dn,#imm` (immediate is a legal EA for this bit op only);
        // previously rendered as dc.w. The byte value rides the low half of a
        // word extension. Byte-identical assemble/disassemble vs vasm.
        assert_eq!(one_m68k(&[0x01, 0x3C, 0x00, 0x12]), "btst d0,#18");
        assert_eq!(one_m68k(&[0x0F, 0x3C, 0x00, 0xAA]), "btst d7,#170");
        // The static form `BTST #bit,#imm` ($083C) stays illegal — data.
        assert_eq!(one_m68k(&[0x08, 0x3C]), "dc.w $083C");
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
    fn m68k_single_operand_extras() {
        // Slot-reusing single-/two-operand ops: TAS/NBCD (byte), NEGX (sized),
        // PEA (long, control EA), UNLK (An), CHK (ea,Dn).
        assert_eq!(one_m68k(&[0x4A, 0xC0]), "tas.b d0");
        assert_eq!(one_m68k(&[0x40, 0x40]), "negx.w d0");
        assert_eq!(one_m68k(&[0x48, 0x00]), "nbcd.b d0");
        assert_eq!(one_m68k(&[0x48, 0x50]), "pea.l (a0)");
        assert_eq!(one_m68k(&[0x4E, 0x58]), "unlk a0");
        assert_eq!(one_m68k(&[0x43, 0x80]), "chk d0,d1");
    }

    #[test]
    fn m68k_extended_and_bcd() {
        // Register form (Dn,Dn) and predecrement form (-(An),-(An)), the mode
        // bit (3) selecting between them. ADDX/SUBX size-coded; ABCD/SBCD byte;
        // CMPM postincrement-only ((An)+,(An)+). Bytes are vasm ground truth.
        assert_eq!(one_m68k(&[0xD1, 0x41]), "addx.w d1,d0");
        assert_eq!(one_m68k(&[0xD1, 0x49]), "addx.w -(a1),-(a0)");
        assert_eq!(one_m68k(&[0x91, 0x41]), "subx.w d1,d0");
        assert_eq!(one_m68k(&[0x91, 0x49]), "subx.w -(a1),-(a0)");
        assert_eq!(one_m68k(&[0xC1, 0x01]), "abcd.b d1,d0");
        assert_eq!(one_m68k(&[0xC1, 0x09]), "abcd.b -(a1),-(a0)");
        assert_eq!(one_m68k(&[0x81, 0x01]), "sbcd.b d1,d0");
        assert_eq!(one_m68k(&[0x81, 0x09]), "sbcd.b -(a1),-(a0)");
        assert_eq!(one_m68k(&[0xB1, 0x49]), "cmpm.w (a1)+,(a0)+");
        assert_eq!(one_m68k(&[0xB5, 0x8B]), "cmpm.l (a3)+,(a2)+");
    }

    #[test]
    fn m68k_trap_movea_exg() {
        // TRAP packs a 4-bit vector in the opcode's low nibble.
        assert_eq!(one_m68k(&[0x4E, 0x40]), "trap #0");
        assert_eq!(one_m68k(&[0x4E, 0x4F]), "trap #15");
        // MOVEA — an An destination disassembles as `movea` (listed before MOVE
        // so it wins the decode). Word/long only; `movea.b` can't be encoded.
        assert_eq!(one_m68k(&[0x32, 0x40]), "movea.w d0,a1");
        assert_eq!(one_m68k(&[0x22, 0x48]), "movea.l a0,a1");
        assert_eq!(
            one_m68k(&[0x20, 0x7C, 0x00, 0x00, 0x00, 0x04]),
            "movea.l #4,a0"
        );
        // EXG — three register-pair kinds; always long, no suffix.
        assert_eq!(one_m68k(&[0xC1, 0x41]), "exg d0,d1");
        assert_eq!(one_m68k(&[0xC1, 0x49]), "exg a0,a1");
        assert_eq!(one_m68k(&[0xC1, 0x89]), "exg d0,a1");
    }

    #[test]
    fn m68k_control_register_moves() {
        // CCR/SR/USP moves — rendered suffixless. These occupy the size-field-11
        // holes of NEGX/NEG/NOT, so they don't shadow those ops.
        assert_eq!(one_m68k(&[0x44, 0xC0]), "move d0,ccr"); // <ea>,ccr (68000)
        assert_eq!(one_m68k(&[0x46, 0xC0]), "move d0,sr"); // <ea>,sr
        assert_eq!(one_m68k(&[0x40, 0xC0]), "move sr,d0"); // sr,<ea>
        assert_eq!(one_m68k(&[0x4E, 0x68]), "move usp,a0"); // usp,An
        assert_eq!(one_m68k(&[0x4E, 0x63]), "move a3,usp"); // An,usp
    }

    #[test]
    fn m68k_immediate_to_ccr_sr() {
        // ORI/ANDI/EORI #imm,CCR (byte in the word's low half) and #imm,SR (word).
        assert_eq!(one_m68k(&[0x00, 0x3C, 0x00, 0x02]), "ori #2,ccr");
        assert_eq!(one_m68k(&[0x02, 0x3C, 0x00, 0x01]), "andi #1,ccr");
        assert_eq!(one_m68k(&[0x0A, 0x3C, 0x00, 0x04]), "eori #4,ccr");
        assert_eq!(one_m68k(&[0x00, 0x7C, 0x56, 0x78]), "ori #22136,sr");
        assert_eq!(one_m68k(&[0x02, 0x7C, 0x12, 0x34]), "andi #4660,sr");
        assert_eq!(one_m68k(&[0x0A, 0x7C, 0x00, 0xFF]), "eori #255,sr");
    }

    #[test]
    fn m68k_movep() {
        // MOVEP — d16(Ay) <-> Dx, both directions and sizes. Bit 7 = direction,
        // bit 6 = size; the mandatory displacement is a 16-bit extension word.
        assert_eq!(one_m68k(&[0x01, 0x08, 0x00, 0x00]), "movep.w 0(a0),d0");
        assert_eq!(one_m68k(&[0x07, 0x4A, 0x00, 0x00]), "movep.l 0(a2),d3");
        assert_eq!(one_m68k(&[0x01, 0x88, 0x00, 0x08]), "movep.w d0,8(a0)");
        assert_eq!(one_m68k(&[0x07, 0xCA, 0x00, 0x08]), "movep.l d3,8(a2)");
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
