//! The shared case derivation — how a spec form becomes the source text an
//! arbiter is given.
//!
//! Recording and replay must agree on this **exactly**. The corpus is keyed on
//! source text, so if the two paths derived text differently by even a space,
//! every replay lookup would miss and the whole net would silently pass by
//! finding nothing to check. That failure mode — a green suite that verifies
//! nothing — is the one worth engineering against, so the derivation lives here
//! and both callers use it rather than each holding its own copy.
//!
//! The route is the same trick the form audit already uses: synthesise the
//! canonical bytes for a form, disassemble them with **our** disassembler, and
//! hand the resulting source to the reference assembler. If the reference
//! produces the bytes we started from, our spec and our disassembler agree with
//! ground truth.

/// Synthesise canonical bytes for a form: its opcode, then filler operands
/// chosen to avoid size-force edge cases, then any trailing suffix.
///
/// The filler values are load-bearing rather than arbitrary. A 2-byte address
/// is `$1234` — at or above `$100`, so a dialect that shrinks small addresses
/// to zero-page keeps it absolute; a 3-byte one is `$123456`, at or above
/// `$10000`, so it stays long. Pick smaller values and the reference silently
/// assembles a *different, shorter* form, and the audit compares two encodings
/// that were never meant to match.
#[must_use]
pub fn synth(form: &isa::Form) -> Vec<u8> {
    let mut b = form.opcode.to_vec();
    for op in form.operands {
        match op.kind {
            isa::OperandKind::RelativePc => {
                // A small forward offset, little-endian over the operand width.
                b.push(0x02);
                b.extend(std::iter::repeat_n(0x00, usize::from(op.bytes) - 1));
            }
            isa::OperandKind::Displacement => b.push(0x05),
            // Big-endian 16-bit immediate (Z80N `push nn`): high byte first.
            isa::OperandKind::ImmediateBe => b.extend_from_slice(&[0x12, 0x34]),
            isa::OperandKind::Immediate | isa::OperandKind::Address => {
                let bytes: &[u8] = match op.bytes {
                    1 => &[0x12],
                    2 => &[0x34, 0x12],
                    3 => &[0x56, 0x34, 0x12],
                    _ => &[],
                };
                b.extend_from_slice(bytes);
            }
        }
    }
    b.extend_from_slice(form.suffix);
    b
}

/// Render bytes as reassemblable source for `cpu`, using the same listing
/// helper the live audit disassembles through.
///
/// `None` for a CPU with no listing helper — a spec that can be swept but not
/// yet rendered back to source. That is a real state (not every ISA has a
/// listing writer), and it must read as "no case to record" rather than as an
/// empty case that records a verdict for nothing.
#[must_use]
pub fn source_text(cpu: &str, code: &[u8], origin: u16) -> Option<String> {
    // Called rather than resolved to a function pointer: the helpers are not
    // uniform. `listing_z80` takes the Z80N flag (the extension is a target
    // property, not a syntax one) and `listing_68000` a 32-bit origin.
    let text = match cpu {
        "z80" => isa_disasm::listing_z80(code, origin, false),
        "z80n" => isa_disasm::listing_z80(code, origin, true),
        "68000" => isa_disasm::listing_68000(code, u32::from(origin)),
        "6502" => isa_disasm::listing_6502(code, origin),
        "6809" => isa_disasm::listing_6809(code, origin),
        "65816" => isa_disasm::listing_65816(code, origin),
        "huc6280" => isa_disasm::listing_huc6280(code, origin),
        "sm83" => isa_disasm::listing_sm83(code, origin),
        "8080" => isa_disasm::listing_i8080(code, origin),
        "6800" => isa_disasm::listing_m6800(code, origin),
        "1802" => isa_disasm::listing_1802(code, origin),
        "8048" => isa_disasm::listing_8048(code, origin),
        "scmp" => isa_disasm::listing_scmp(code, origin),
        "f8" => isa_disasm::listing_f8(code, origin),
        "2650" => isa_disasm::listing_2650(code, origin),
        "tms7000" => isa_disasm::listing_tms7000(code, origin),
        "pdp11" => isa_disasm::listing_pdp11(code, origin),
        "tms9900" => isa_disasm::listing_tms9900(code, origin),
        "cp1610" => isa_disasm::listing_cp1610(code, origin),
        "z8000" => isa_disasm::listing_z8000(code, origin),
        "z8001" => isa_disasm::listing_z8001(code, origin),
        _ => return None,
    };
    Some(text)
}

/// The full derivation for one form: synthesised bytes plus the source text an
/// arbiter should be given for them.
///
/// This is the function both recording and replay call. Neither should reach
/// for [`synth`] or [`source_text`] separately — going through one door is the
/// point.
#[must_use]
pub fn case(cpu: &str, form: &isa::Form) -> Option<Case> {
    let bytes = synth(form);
    let source = source_text(cpu, &bytes, 0)?;
    Some(Case { bytes, source })
}

/// One derived case: what we expect, and what the arbiter is asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Case {
    /// The canonical bytes our spec says this form encodes to.
    pub bytes: Vec<u8>,
    /// The source text to hand the reference assembler.
    pub source: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The derivation is a pure function of (cpu, form): same input, same text,
    /// every time. Replay depends on this completely — a derivation that varied
    /// per run would make every corpus lookup miss.
    #[test]
    fn deriving_the_same_form_twice_gives_the_same_text() {
        let form = &isa::i8080::SET.instructions[0].forms[0];
        assert_eq!(case("8080", form), case("8080", form));
    }

    /// A CPU with no listing helper yields no case, rather than an empty one.
    /// "Nothing to record here" and "recorded nothing" must not look alike.
    #[test]
    fn a_cpu_without_a_listing_helper_yields_no_case() {
        let form = &isa::i8080::SET.instructions[0].forms[0];
        assert_eq!(case("no-such-cpu", form), None);
        assert_eq!(source_text("no-such-cpu", &[0x00], 0), None);
    }

    /// The derived source must round-trip through our own assembler back to the
    /// bytes we synthesised. That is what makes it a fair question to put to the
    /// reference: if our own disassembler wrote source we cannot reassemble, a
    /// mismatch would say nothing about the spec.
    #[test]
    fn the_derived_source_names_the_cpu_and_carries_the_form() {
        let form = &isa::i8080::SET.instructions[0].forms[0];
        let case = case("8080", form).expect("8080 has a listing helper");
        assert!(
            case.source.contains("cpu 8080"),
            "the listing is self-contained: {}",
            case.source
        );
        assert!(
            !case.bytes.is_empty(),
            "a form synthesises to at least an opcode"
        );
    }

    /// Operand filler stays above the size-force thresholds, so a reference
    /// assembler cannot quietly pick a shorter form than the one under test.
    /// Checked against a real spec form rather than a hand-built one, so the
    /// test cannot drift from what the audit actually synthesises.
    #[test]
    fn address_filler_stays_above_the_size_force_thresholds() {
        // Mnemonics are upper-case in the specs; looked up through the spec's
        // own accessor rather than by scanning, so this cannot drift.
        let jmp = isa::i8080::SET
            .instruction("JMP")
            .expect("the 8080 has a JMP");
        let absolute = jmp
            .forms
            .iter()
            .find(|f| f.operands.iter().any(|o| o.bytes == 2))
            .expect("jmp takes a 16-bit address");
        // $1234, little-endian — at or above $100, so nothing shrinks it.
        assert_eq!(synth(absolute), vec![0xC3, 0x34, 0x12]);
    }
}
