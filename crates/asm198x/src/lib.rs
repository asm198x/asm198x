//! Asm198x — a family of modern assemblers for retro CPUs.
//!
//! The crate is built around one **dialect-agnostic engine** and a set of
//! **dialect front-ends**. The engine ([`engine`]) owns the two-pass driver,
//! symbol table, expression evaluation, directive semantics, and byte
//! emission — none of it CPU- or syntax-specific. A [`Dialect`](dialect::Dialect)
//! ([`dialects`]) tokenises one source syntax and resolves each instruction's
//! addressing mode against an [`isa`] spec, producing the engine's statement
//! stream. Instruction *encoding* comes entirely from the shared [`isa`] spec.
//!
//! This three-way seam — **engine ↔ dialect ↔ spec** — is what lets one binary
//! span many CPUs and many source dialects: a new dialect is a new module in
//! [`dialects`], a new CPU is a new spec in [`isa`], and the engine is reused
//! unchanged. See `decisions/syntax-stance.md` and the umbrella decision
//! `asm198x-and-shared-isa-spec.md`.

mod dialect;
mod dialects;
mod engine;

pub use engine::{Assembly, AsmError};

/// Assemble 6502 source into a flat binary, using the (early, ca65/ACME-shaped)
/// 6502 dialect.
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_6502(source: &str) -> Result<Assembly, AsmError> {
    engine::assemble(source, &dialects::Mos6502)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembles_countdown_loop() {
        let source = "\
            ; count down X, storing A across a page\n\
                    .org $0200\n\
            start:  lda #$00\n\
                    ldx #$08\n\
            loop:   sta $0400,x\n\
                    dex\n\
                    bne loop\n\
                    rts\n";
        let a = assemble_6502(source).expect("assembles");
        assert_eq!(a.origin, 0x0200);
        assert_eq!(
            a.bytes,
            vec![
                0xA9, 0x00, 0xA2, 0x08, 0x9D, 0x00, 0x04, 0xCA, 0xD0, 0xFA, 0x60
            ]
        );
        assert_eq!(a.symbols.get("start"), Some(&0x0200));
        assert_eq!(a.symbols.get("loop"), Some(&0x0204));
    }

    #[test]
    fn chooses_zero_page_over_absolute() {
        let zp = assemble_6502("lda $10").expect("zp");
        assert_eq!(zp.bytes, vec![0xA5, 0x10]);
        let abs = assemble_6502("lda $1234").expect("abs");
        assert_eq!(abs.bytes, vec![0xAD, 0x34, 0x12]); // little-endian
    }

    #[test]
    fn indexed_and_immediate() {
        assert_eq!(
            assemble_6502("sta $00,x").expect("zpx").bytes,
            vec![0x95, 0x00]
        );
        assert_eq!(
            assemble_6502("lda #'A'").expect("char").bytes,
            vec![0xA9, 0x41]
        );
        assert_eq!(
            assemble_6502("lda #%00001111").expect("bin").bytes,
            vec![0xA9, 0x0F]
        );
    }

    #[test]
    fn high_low_byte_operators() {
        // `<` takes the low byte, `>` the high byte.
        assert_eq!(
            assemble_6502("lda #<$1234").expect("lo").bytes,
            vec![0xA9, 0x34]
        );
        assert_eq!(
            assemble_6502("ldx #>$1234").expect("hi").bytes,
            vec![0xA2, 0x12]
        );
    }

    #[test]
    fn rejects_oversized_immediate() {
        let err = assemble_6502("lda #$1234").expect_err("immediate too big");
        assert!(err.message.contains("byte"), "unexpected: {err}");
    }

    #[test]
    fn reports_unknown_instruction_with_line() {
        let err = assemble_6502("\n    frob $10\n").expect_err("unknown mnemonic");
        assert_eq!(err.line, 2);
    }
}
