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
//!
//! ## Two output shapes: flat vs linked
//!
//! Most dialects ([`assemble_acme`], [`assemble_pasmo`], …) implement the
//! `Dialect` trait and run through that engine, producing a flat [`Assembly`]
//! at one origin. **ca65** ([`assemble_ca65`]) is the exception: it is an
//! assembler whose output is normally linked by ld65, so it does *not* implement
//! `Dialect` or use the flat engine. Instead it reuses only the genuinely shared
//! parts — the 6502 operand/expression core (`dialects::mos6502`) and the
//! [`isa`] spec — and runs its own assemble + (bounded) link pass, returning the
//! finished `.nes` ROM bytes. The asymmetry is deliberate: linking places code
//! into segments at config-defined addresses, which the single-origin engine has
//! no notion of. See the linker scope note in `decisions/syntax-stance.md`.
//!
//! Disassembly ([`disassemble_z80`]/[`disassemble_6502`]) is the inverse, driven
//! by the same [`isa`] spec the assemblers emit from.

mod dialect;
mod dialects;
mod engine;
#[cfg(test)]
mod roundtrip_tests;

// Disassembly lives in the dependency-free `isa-disasm` crate (only `isa` +
// std) so Emu198x can consume it without the assembler; re-exported here so the
// `asm198x` library API and CLI are unchanged.
pub use engine::{AsmError, Assembly};
pub use isa_disasm::{
    Line, disassemble_6502, disassemble_6809, disassemble_68000, disassemble_z80, listing_6502,
    listing_6809, listing_68000, listing_z80,
};

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

/// Assemble Motorola-syntax 68000 source into a flat big-endian code image
/// (the Amiga curriculum's `vasm` dialect) with the optimizer on — matching
/// `vasmm68k_mot -Fbin`. Rejects multi-section sources (a flat binary holds one
/// section); use [`assemble_vasm_exe`] for those.
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_vasm(source: &str) -> Result<Vec<u8>, AsmError> {
    dialects::vasm::assemble(source)
}

/// Assemble Motorola-syntax 68000 source into an Amiga hunk executable —
/// matching `vasmm68k_mot -Fhunkexe -kick1hunks` for everything the AmigaDOS
/// loader consumes (header, code/data/bss hunks, reloc32 tables). The optional
/// debug symbol table is omitted (see the Stage 3 decision in
/// `decisions/syntax-stance.md`).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_vasm_exe(source: &str) -> Result<Vec<u8>, AsmError> {
    dialects::vasm::assemble_exe(source)
}

/// Assemble ca65-syntax 65816 source (native mode) into a flat binary — the
/// 65816 as a target extension of the 6502 (`isa::mos6502` + `isa::mos65816`).
/// Accumulator/index immediate width follows the `.a8`/`.a16`/`.i8`/`.i16`
/// directives. Matches `ca65 --cpu 65816` linked flat.
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_ca65_816(source: &str) -> Result<Assembly, AsmError> {
    engine::assemble(source, &dialects::Ca65_816)
}

/// Assemble lwasm-syntax 6809 source into a flat big-endian binary — matching
/// `lwasm --6809 --raw`. Covers inherent, immediate, direct, extended, and
/// relative (short + long) addressing; indexed addressing is not yet supported.
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_lwasm(source: &str) -> Result<Assembly, AsmError> {
    engine::assemble(source, &dialects::Lwasm)
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
            vec![
                0xA9, 0x00, 0xA2, 0x08, 0x9D, 0x00, 0x04, 0xCA, 0xD0, 0xFA, 0x60
            ]
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
        assert_eq!(
            assemble_pasmo("ld a, 0").expect("pasmo").bytes,
            vec![0x3E, 0x00]
        );
        assert_eq!(
            assemble_sjasmplus("ld a, 0").expect("sjasm").bytes,
            vec![0x3E, 0x00]
        );
    }
}
