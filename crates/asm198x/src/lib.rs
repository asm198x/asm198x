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
mod prg;
#[cfg(test)]
mod roundtrip_tests;
mod sna;

// Disassembly lives in the dependency-free `isa-disasm` crate (only `isa` +
// std) so Emu198x can consume it without the assembler; re-exported here so the
// `asm198x` library API and CLI are unchanged.
pub use engine::{AsmError, Assembly, Warning};
pub use isa_disasm::{
    Line, disassemble_1802, disassemble_6502, disassemble_6809, disassemble_8048,
    disassemble_65816, disassemble_68000, disassemble_f8, disassemble_huc6280, disassemble_i8080,
    disassemble_m6800, disassemble_scmp, disassemble_sm83, disassemble_z80, listing_1802,
    listing_6502, listing_6809, listing_8048, listing_65816, listing_68000, listing_f8,
    listing_huc6280, listing_i8080, listing_m6800, listing_scmp, listing_sm83, listing_z80,
};
pub use prg::prg;
pub use sna::sna_48k;

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

/// As [`assemble_vasm`], but also returns any non-fatal [`Warning`]s raised
/// while assembling (e.g. an out-of-range immediate to CCR/SR, which vasm warns
/// on but still encodes). The returned bytes are identical to [`assemble_vasm`];
/// the warnings are advisory, so callers that only need bytes can use the
/// simpler function.
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_vasm_warned(source: &str) -> Result<(Vec<u8>, Vec<Warning>), AsmError> {
    dialects::vasm::assemble_warned(source)
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

/// Assemble ca65-syntax HuC6280 source into a flat little-endian binary — the
/// HuC6280 (PC Engine / TurboGrafx-16 CPU) as a target extension of the 6502
/// (`isa::mos6502` + `isa::huc6280`), mirroring the 65816 mechanism. Covers the
/// 65C02 additions, the Rockwell bit ops, and the HuC6280-specific instructions
/// (`st0`–`st2`, `tam`/`tma`, `tst`, `bsr`, and the block transfers). Matches
/// `ca65 --cpu huc6280` linked flat.
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_ca65_huc6280(source: &str) -> Result<Assembly, AsmError> {
    engine::assemble(source, &dialects::Ca65Huc6280)
}

/// Assemble rgbasm-syntax SM83 (Game Boy) source into a flat binary at the
/// section's origin — the RGBDS dialect over [`isa::sm83`]. Covers the full
/// documented instruction set, `SECTION`/`db`/`dw`/`ds`/`EQU`, and `.local`
/// labels. Matches `rgbasm`/`rgblink` for the emitted bytes.
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_rgbasm(source: &str) -> Result<Assembly, AsmError> {
    engine::assemble(source, &dialects::Rgbasm)
}

/// Assemble Intel-syntax 8080 source into a flat binary at the `org` — the
/// classic `MOV`/`MVI`/`LXI` mnemonics with radix-suffixed numbers (`42H`),
/// over [`isa::i8080`]. Matches `asl` (`cpu 8080`).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_i8080(source: &str) -> Result<Assembly, AsmError> {
    engine::assemble(source, &dialects::I8080)
}

/// Assemble Motorola-syntax 6800 source into a flat big-endian binary at the
/// `org`, over [`isa::m6800`]. Motorola `$`-hex, `#` immediate, `$nn,X` indexed,
/// direct-vs-extended by size (or a `>`/`<` force). Matches `asl` (`cpu 6800`).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_m6800(source: &str) -> Result<Assembly, AsmError> {
    engine::assemble(source, &dialects::M6800)
}

/// Assemble asl-syntax RCA CDP1802 (COSMAC) source into a flat big-endian binary
/// at the `org`, over [`isa::cdp1802`]. Intel `H`-hex, bare register numbers, and
/// the page-relative short branch. Matches `asl` (`cpu 1802`).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_1802(source: &str) -> Result<Assembly, AsmError> {
    engine::assemble(source, &dialects::Cdp1802)
}

/// Assemble asl-syntax Intel 8048 (MCS-48) source into a flat binary at the
/// `org`, over [`isa::i8048`]. Intel `H`-hex; the mode label is the operand
/// template; `JMP`/`CALL` pack the address page into the opcode via the
/// computed-operand seam. Matches `asl` (`cpu 8048`).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_8048(source: &str) -> Result<Assembly, AsmError> {
    engine::assemble(source, &dialects::I8048 { romless: false })
}

/// Assemble asl-syntax ROM-less MCS-48 source (8035/8039/8040 and CMOS kin) into
/// a flat binary at the `org`, over [`isa::i8048`]. Identical to
/// [`assemble_8048`] except the four BUS-port instructions (`ORL`/`ANL BUS,#`,
/// `OUTL BUS,A`, `INS A,BUS`) are rejected — on a ROM-less part the bus fetches
/// external program memory. Matches `asl` (`cpu 8039`).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure, or a BUS-port instruction.
pub fn assemble_8039(source: &str) -> Result<Assembly, AsmError> {
    engine::assemble(source, &dialects::I8048 { romless: true })
}

/// Assemble asl-syntax National SC/MP (INS8060) source into a flat binary at the
/// `org`, over [`isa::scmp`]. C-style numbers (`0x..` hex); `disp(ptr)` /
/// `@disp(ptr)` memory references (the literal `e` selects the E-register
/// index), pointer-exchange, and immediate forms. Matches `asl` (`cpu SC/MP`).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_scmp(source: &str) -> Result<Assembly, AsmError> {
    engine::assemble(source, &dialects::Scmp)
}

/// Assemble asl-syntax Fairchild F8 (3850) source into a flat binary at the
/// `org`, over [`isa::f8`]. Intel `H`-suffix numbers; scratchpad register forms
/// (`S`/`I`/`D` = 12/13/14), 4-bit immediate loads/ports, big-endian 16-bit
/// addresses, and relative branches (measured from the offset byte, emitted via
/// the computed-operand seam). Matches `asl` (`cpu F3850`).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_f8(source: &str) -> Result<Assembly, AsmError> {
    engine::assemble(source, &dialects::F8)
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

    #[test]
    fn vasm_immediate_ops_are_distinct_and_aliased() {
        // addi/subi/cmpi are their own mnemonics (the $06/$04/$0C encodings).
        assert_eq!(
            assemble_vasm("\tsubi.b #16,d0\n").expect("subi"),
            vec![0x04, 0x00, 0x00, 0x10]
        );
        assert_eq!(
            assemble_vasm("\taddi.w #100,d2\n").expect("addi"),
            vec![0x06, 0x42, 0x00, 0x64]
        );
        // `cmp #imm,<memory>` aliases to cmpi (vasm uses the <ea>,Dn form only
        // for a data-register destination), so the two assemble identically.
        assert_eq!(
            assemble_vasm("\tcmp.w #1,(a0)\n").expect("cmp alias"),
            assemble_vasm("\tcmpi.w #1,(a0)\n").expect("cmpi"),
        );
    }

    #[test]
    fn vasm_out_of_range_ccr_sr_immediate_warns_not_errors() {
        // vasm warns (2037) but still assembles an out-of-range immediate to
        // CCR (byte) / SR (word); asm198x mirrors that — same bytes, plus a
        // non-fatal warning. In-range immediates warn about nothing.
        let (bytes, warns) = assemble_vasm_warned("\tandi #$1234,ccr\n").expect("ccr");
        assert_eq!(bytes, vec![0x02, 0x3C, 0x12, 0x34]); // byte-identical to vasm
        assert_eq!(warns.len(), 1);
        assert_eq!(warns[0].line, 1);
        assert!(warns[0].message.contains("out of range"));

        let (bytes, warns) = assemble_vasm_warned("\tandi #$12345,sr\n").expect("sr");
        assert_eq!(bytes, vec![0x02, 0x7C, 0x23, 0x45]);
        assert_eq!(warns.len(), 1);

        // In range: CCR byte ($FF) and SR word ($FFFF) raise no warning.
        let (_, warns) = assemble_vasm_warned("\tandi #$ff,ccr\n\tandi #$ffff,sr\n").expect("ok");
        assert!(warns.is_empty());
    }

    #[test]
    fn vasm_out_of_range_immediates_warn_and_match_vasm() {
        // vasm warns (not errors) on an over-range immediate and keeps the low
        // bits; asm198x mirrors that — same bytes, plus a non-fatal warning.
        // (Previously asm198x errored on moveq/addq/trap and masked byte moves.)
        let cases: &[(&str, &[u8])] = &[
            ("\tmove.b #$1234,d0\n", &[0x10, 0x3C, 0x12, 0x34]),
            ("\tmoveq #$1FF,d0\n", &[0x70, 0xFF]),
            ("\taddq.w #9,d0\n", &[0x52, 0x40]),
            ("\ttrap #16\n", &[0x4E, 0x50]),
        ];
        for (src, want) in cases {
            let (bytes, warns) = assemble_vasm_warned(src).expect(src);
            assert_eq!(bytes, *want, "bytes for {src:?}");
            assert_eq!(warns.len(), 1, "one warning for {src:?}");
        }
        // In-range forms of the same instructions raise no warning.
        let (_, warns) =
            assemble_vasm_warned("\tmoveq #5,d0\n\taddq.w #3,d0\n\ttrap #7\n").expect("ok");
        assert!(warns.is_empty());
    }

    #[test]
    fn vasm_pc_relative_round_trips() {
        // `move.w $10(pc),d0` at origin 0: disassembly renders the resolved
        // target, which re-assembles to the same bytes (displacement = target −
        // PC). The disassembler<->assembler PC-relative contract.
        let bytes = vec![0x30, 0x3A, 0x00, 0x0E];
        let text = listing_68000(&bytes, 0);
        assert_eq!(assemble_vasm(&text).expect("reassemble"), bytes);
    }
}
