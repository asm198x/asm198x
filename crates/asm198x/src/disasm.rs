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
//! per-CPU. The Z80 renderer here treats the mode label as an operand template
//! (`A,(IX+d)` with `nn`/`n`/`d`/`e` placeholders), which the pasmo front-end
//! parses straight back. A 6502 renderer (whose mode labels are addressing-mode
//! names, not templates) is a later addition.

/// One disassembled instruction.
#[derive(Debug, Clone)]
pub struct Line {
    /// Address the instruction loads at.
    pub addr: u16,
    /// The raw encoded bytes.
    pub bytes: Vec<u8>,
    /// The reassemblable source text, e.g. `"LD A,(IX+$05)"`.
    pub text: String,
}

/// Disassemble a flat Z80 binary loaded at `origin`.
#[must_use]
pub fn disassemble_z80(code: &[u8], origin: u16) -> Vec<Line> {
    let set = &isa::z80::SET;
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < code.len() {
        let addr = origin.wrapping_add(pos as u16);
        if let Some((mnemonic, form, values, len)) = decode_one(set, code, pos) {
            out.push(Line {
                addr,
                bytes: code[pos..pos + len].to_vec(),
                text: render_z80(mnemonic, form, &values, addr, len),
            });
            pos += len;
        } else {
            // Not a known opcode here: emit the byte as data and move on.
            out.push(Line {
                addr,
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
pub fn listing_z80(code: &[u8], origin: u16) -> String {
    let mut s = format!("        org ${origin:04X}\n");
    for line in disassemble_z80(code, origin) {
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
    set: &'a isa::InstructionSet,
    code: &[u8],
    pos: usize,
) -> Option<(&'a str, &'a isa::Form, Vec<i64>, usize)> {
    for opcode_len in [2usize, 1] {
        if pos + opcode_len > code.len() {
            continue;
        }
        let opcode = &code[pos..pos + opcode_len];
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
    format!("{mnemonic} {}", render_operands(form.mode, values, addr, len))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assemble_pasmonext;

    fn one(bytes: &[u8]) -> String {
        let lines = disassemble_z80(bytes, 0x8000);
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
        let listing = listing_z80(&original.bytes, original.origin);
        let reassembled = assemble_pasmonext(&listing).expect("reassemble");
        assert_eq!(reassembled.bytes, original.bytes, "listing was:\n{listing}");
    }
}
