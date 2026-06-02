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
mod disasm;
mod engine;

pub use disasm::{disassemble_z80, listing_z80, Line};
pub use engine::{Assembly, AsmError};

/// Assemble ACME-syntax 6502 source into a flat binary — the C64 curriculum's
/// dialect (`*=` to set the PC, `!byte`/`!word`/`!fill`, `name = value`).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_acme(source: &str) -> Result<Assembly, AsmError> {
    engine::assemble(source, &dialects::Acme)
}

/// Assemble ca65-syntax 6502 source for the NES and link it into a `.nes` ROM
/// image — the NES curriculum's toolchain (ca65 + ld65) in one step. Unlike the
/// flat assemblers, this returns the finished ROM bytes (iNES header + 32K PRG +
/// 8K CHR) because the output is the linker's, not a single origin's.
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_ca65(source: &str) -> Result<Vec<u8>, AsmError> {
    dialects::ca65::assemble(source)
}

/// Assemble pasmo-syntax Z80 source into a flat binary, targeting a **plain
/// Z80** (Z80N opcodes are rejected, as vanilla pasmo rejects them).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_pasmo(source: &str) -> Result<Assembly, AsmError> {
    engine::assemble(source, &dialects::Pasmo { z80n: false })
}

/// Assemble pasmo-syntax Z80 source targeting the **ZX Spectrum Next (Z80N)** —
/// the same syntax as [`assemble_pasmo`] with the Z80N opcodes also available
/// (what `pasmonext` does).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_pasmonext(source: &str) -> Result<Assembly, AsmError> {
    engine::assemble(source, &dialects::Pasmo { z80n: true })
}

/// Assemble sjasmplus-syntax Z80 source targeting a plain Z80.
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_sjasmplus(source: &str) -> Result<Assembly, AsmError> {
    engine::assemble(source, &dialects::Sjasmplus { z80n: false })
}

/// Assemble sjasmplus-syntax Z80 source targeting the ZX Spectrum Next (Z80N).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_sjasmplus_next(source: &str) -> Result<Assembly, AsmError> {
    engine::assemble(source, &dialects::Sjasmplus { z80n: true })
}

#[cfg(test)]
mod tests {
    use super::*;

    // End-to-end smoke tests over the public API. The per-dialect behaviour is
    // covered in each dialect module; these just confirm the entry points wire
    // through the engine correctly.

    #[test]
    fn assembles_countdown_loop_via_acme() {
        let source = "\
            ; count down X, storing A across a page\n\
                    *= $0200\n\
            start:  lda #$00\n\
                    ldx #$08\n\
            loop:   sta $0400,x\n\
                    dex\n\
                    bne loop\n\
                    rts\n";
        let a = assemble_acme(source).expect("assembles");
        assert_eq!(a.origin, 0x0200);
        assert_eq!(
            a.bytes,
            vec![0xA9, 0x00, 0xA2, 0x08, 0x9D, 0x00, 0x04, 0xCA, 0xD0, 0xFA, 0x60]
        );
        assert_eq!(a.symbols.get("start"), Some(&0x0200));
        assert_eq!(a.symbols.get("loop"), Some(&0x0204));
    }

    #[test]
    fn reports_unknown_instruction_with_line() {
        let err = assemble_acme("\n    frob $10\n").expect_err("unknown mnemonic");
        assert_eq!(err.line, 2);
    }

    #[test]
    fn z80_entry_points_wire_through() {
        assert_eq!(assemble_pasmo("ld a, 0").expect("pasmo").bytes, vec![0x3E, 0x00]);
        assert_eq!(assemble_sjasmplus("ld a, 0").expect("sjasm").bytes, vec![0x3E, 0x00]);
    }
}
