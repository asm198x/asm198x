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

// The source-preserving semantic AST (plan U2). A layer above the encoder;
// dialects lower into it and it lowers to Statement/Operation (U3 wires that).
mod ast;
mod contract;
mod dialect;
mod dialects;
mod engine;
// Debug-record renderings: the `.debug198x` sidecar builder + `--sym` /
// `--listing` text views of the same captured record (Debug198x U3, KTD2).
mod listing;
mod prg;
#[cfg(test)]
mod roundtrip_tests;
mod sna;
// The multi-file source model (language-surface U1): the loader seam
// (filesystem + in-memory), the FileId-allocating source map, and the include
// graph. Public as a module (not flattened into the crate root): consumers
// reach `source::SourceMap` etc.; the include-capable assemble entry points
// arrive in U2.
pub mod source;
// The shared source-provenance model (one Span across ast/engine/contract).
mod span;

// Disassembly lives in the dependency-free `isa-disasm` crate (only `isa` +
// std) so Emu198x can consume it without the assembler; re-exported here so the
// `asm198x` library API and CLI are unchanged.
// `AssemblyResult` (the one structured result, R1/U1) is the return type of
// every `assemble_*` entry point. `Assembly` stays exported as the engine's
// internal flat builder that `AssemblyResult` wraps.
pub use contract::{
    AssemblyResult, CONTRACT_VERSION, Code, Diagnostic, DiagnosticEnvelope, Fix, Severity,
    resolve_span_path,
};
pub use engine::{AsmError, Assembly, DebugData, LineRec, Warning};
pub use listing::{debug_info, render_listing, render_sym};
pub use span::{ExpansionFrame, FileId, Span};
// Re-exported so consumers of `Assembly.debug` need not depend on debug198x
// directly for the symbol types the engine captures.
pub use debug198x;
pub use isa_disasm::{
    Line, disassemble_1802, disassemble_2650, disassemble_6502, disassemble_6809, disassemble_8048,
    disassemble_65816, disassemble_68000, disassemble_cp1610, disassemble_f8, disassemble_huc6280,
    disassemble_i8080, disassemble_m6800, disassemble_pdp11, disassemble_scmp, disassemble_sm83,
    disassemble_tms7000, disassemble_tms9900, disassemble_z80, disassemble_z8000,
    disassemble_z8001, listing_1802, listing_2650, listing_6502, listing_6809, listing_8048,
    listing_65816, listing_68000, listing_cp1610, listing_f8, listing_huc6280, listing_i8080,
    listing_m6800, listing_pdp11, listing_scmp, listing_sm83, listing_tms7000, listing_tms9900,
    listing_z80, listing_z8000, listing_z8001,
};
pub use prg::prg;
pub use sna::sna_48k;

/// Assemble ACME-syntax 6502 source into a flat binary — the C64 curriculum's
/// dialect (`*=` to set the PC, `!byte`/`!word`/`!fill`, `name = value`).
/// Single-source: a `!src`/`!bin` directive is an error here — use
/// [`assemble_acme_files`] for a multi-file program.
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_acme(source: &str) -> Result<AssemblyResult, AsmError> {
    engine::assemble(source, &dialects::Acme).map(AssemblyResult::from)
}

/// Assemble a **multi-file** ACME program (language-surface U4): `source` is
/// the root file's text, `input_path` its name (entry 0 of the file table),
/// and `!src`/`!source` and `!bin`/`!binary` directives resolve through
/// `loader` — the CLI wires an [`FsLoader`](source::FsLoader) carrying the
/// input's directory and the `-I` search dirs; tests wire a
/// [`MemoryLoader`](source::MemoryLoader) (KTD8). Resolution happens inside
/// the live evaluation walk, so a `!src` in an untaken conditional branch
/// never loads (KTD1), and acme's probe-pinned `!bin "file"[, size[, skip]]`
/// window semantics (zero-padding past EOF) apply byte-identically. On
/// success, [`AssemblyResult::files`] holds the `FileId`→path table. The
/// single-source [`assemble_acme`] is unchanged and rejects both directives.
///
/// # Errors
/// A [`MultiFileError`] carrying the failure *and* the source map (file
/// table + include graph) built up to it, so a failure inside an included
/// file can still name its file and chain (KTD2).
pub fn assemble_acme_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    assemble_files(&dialects::Acme, source, input_path, loader)
}

/// Assemble ca65-syntax 6502 source for the NES and link it into a `.nes` ROM
/// image — the NES curriculum's toolchain (ca65 + ld65) in one step. Unlike the
/// flat assemblers, this returns the finished ROM bytes (iNES header + 32K PRG +
/// 8K CHR) because the output is the linker's, not a single origin's.
/// Single-source: a `.include`/`.incbin` directive is an error here — use
/// [`assemble_ca65_files`] for a multi-file program.
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_ca65(source: &str) -> Result<AssemblyResult, AsmError> {
    dialects::ca65::assemble(source).map(AssemblyResult::image)
}

/// Assemble + link a **multi-file** NES ca65 program (language-surface U5):
/// `source` is the root file's text, `input_path` its name (entry 0 of the
/// file table), and `.include`/`.incbin` directives resolve through `loader` —
/// the CLI wires an [`FsLoader`](source::FsLoader) carrying the input's
/// directory and the `-I` search dirs; tests wire a
/// [`MemoryLoader`](source::MemoryLoader) (KTD8). Resolution follows ca65's
/// probe-pinned ancestor-chain order and `.incbin`'s offset/size window
/// matches ca65 exactly (both re-confirmed under the ca65+ld65 NES link —
/// they are assembler-side semantics). Segment state, `=` constants, cheap
/// locals, and the anonymous-label stream all cross include boundaries as
/// ca65's textual splice does (probe-pinned). On success,
/// [`AssemblyResult::files`] holds the `FileId`→path table. The single-source
/// [`assemble_ca65`] is unchanged and rejects both directives.
///
/// # Errors
/// A [`MultiFileError`] carrying the failure *and* the source map (file
/// table + include graph) built up to it, so a failure inside an included
/// file can still name its file and chain (KTD2).
pub fn assemble_ca65_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    let mut map = source::SourceMap::new(input_path, source);
    match dialects::ca65::assemble_multi(&mut map, loader) {
        Ok((rom, _)) => {
            let mut result = AssemblyResult::image(rom);
            result.files = map.file_table();
            Ok(result)
        }
        Err(error) => Err(MultiFileError {
            error,
            source_map: Box::new(map),
        }),
    }
}

/// As [`assemble_ca65_files`], also returning the full
/// [`debug198x::DebugInfo`] read out of layout — the multi-file counterpart of
/// [`assemble_ca65_debug`]. `Header.sources` is the file table in `FileId`
/// order (KTD2: `sources[i] ⇔ FileId(i)`, the same convention as
/// [`AssemblyResult::files`]) and every line span names the file its statement
/// was written in, so bytes pulled in by an included file attribute to that
/// file. Bytes are identical to [`assemble_ca65_files`] by construction (one
/// code path; AE2).
///
/// # Errors
/// As [`assemble_ca65_files`].
pub fn assemble_ca65_files_debug(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<(AssemblyResult, debug198x::DebugInfo), MultiFileError> {
    let mut map = source::SourceMap::new(input_path, source);
    match dialects::ca65::assemble_multi(&mut map, loader) {
        Ok((rom, capture)) => {
            let files = map.file_table();
            let info = listing::capture_debug_info_multi(capture, "6502", "ca65", files.clone());
            let mut result = AssemblyResult::image(rom);
            result.files = files;
            Ok((result, info))
        }
        Err(error) => Err(MultiFileError {
            error,
            source_map: Box::new(map),
        }),
    }
}

/// Assemble + link ca65 source, also returning the full [`debug198x::DebugInfo`]
/// read out of layout (Debug198x U4, KTD4) — per-segment sections, symbols, and
/// line spans at post-link CPU addresses. `source_path` names the source in the
/// header and every line span. Bytes are identical to [`assemble_ca65`] by
/// construction (one code path; AE2).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_ca65_debug(
    source: &str,
    source_path: &str,
) -> Result<(AssemblyResult, debug198x::DebugInfo), AsmError> {
    let (rom, capture) = dialects::ca65::assemble_with_debug(source)?;
    Ok((
        AssemblyResult::image(rom),
        listing::capture_debug_info(capture, "6502", "ca65", source_path),
    ))
}

/// Reformat ca65-syntax NES source to canonical layout (the `--fmt` formatter).
/// Parses into the source-preserving semantic AST and emits canonical
/// same-dialect source — reassembling byte-identical to the input, with the
/// named, `@cheap`, and anonymous (`:`) label forms preserved.
///
/// # Errors
/// Returns an [`AsmError`] on any parse failure.
pub fn format_ca65(source: &str) -> Result<String, AsmError> {
    Ok(ast::emit(
        &dialects::ca65::parse_program(&isa::mos6502::SET, source)?,
        false,
    ))
}

/// Assemble Motorola-syntax 68000 source into a flat big-endian code image
/// (the Amiga curriculum's `vasm` dialect) with the optimizer on — matching
/// `vasmm68k_mot -Fbin`. Rejects multi-section sources (a flat binary holds one
/// section); use [`assemble_vasm_exe`] for those.
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_vasm(source: &str) -> Result<AssemblyResult, AsmError> {
    dialects::vasm::assemble(source).map(AssemblyResult::image)
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
pub fn assemble_vasm_warned(source: &str) -> Result<AssemblyResult, AsmError> {
    dialects::vasm::assemble_warned(source)
        .map(|(bytes, warnings)| AssemblyResult::image_warned(bytes, warnings))
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
pub fn assemble_vasm_exe(source: &str) -> Result<AssemblyResult, AsmError> {
    dialects::vasm::assemble_exe(source).map(AssemblyResult::image)
}

/// As [`assemble_vasm_warned`], also returning the full
/// [`debug198x::DebugInfo`] read out of assembly (Debug198x U5): the section
/// table, `(section, offset)` symbols, and line spans — **section-relative**,
/// with `base: None` throughout (hunks are relocatable; a consumer supplies
/// load addresses via a `BaseMap`, KTD7). Bytes are identical to
/// [`assemble_vasm_warned`] by construction (one `assemble_core` path; AE2).
///
/// # Errors
/// As [`assemble_vasm_warned`].
pub fn assemble_vasm_warned_debug(
    source: &str,
    source_path: &str,
) -> Result<(AssemblyResult, debug198x::DebugInfo), AsmError> {
    let (bytes, warnings, capture) = dialects::vasm::assemble_warned_with_debug(source)?;
    Ok((
        AssemblyResult::image_warned(bytes, warnings),
        listing::capture_debug_info(capture, "68000", "vasm", source_path),
    ))
}

/// As [`assemble_vasm_exe`], also returning the full [`debug198x::DebugInfo`]
/// read out of assembly (Debug198x U5) — the same section-relative record as
/// [`assemble_vasm_warned_debug`], describing the hunks the executable loads.
///
/// # Errors
/// As [`assemble_vasm_exe`].
pub fn assemble_vasm_exe_debug(
    source: &str,
    source_path: &str,
) -> Result<(AssemblyResult, debug198x::DebugInfo), AsmError> {
    let (bytes, capture) = dialects::vasm::assemble_exe_with_debug(source)?;
    Ok((
        AssemblyResult::image(bytes),
        listing::capture_debug_info(capture, "68000", "vasm", source_path),
    ))
}

/// Assemble a **multi-file** 68000 program to a flat binary (language-surface
/// U6): `source` is the root file's text, `input_path` its name (entry 0 of
/// the file table), and `include`/`incbin` directives resolve through
/// `loader` — the CLI wires an [`FsLoader`](source::FsLoader) carrying the
/// input's directory and the `-I` search dirs; tests wire a
/// [`MemoryLoader`](source::MemoryLoader) (KTD8). Resolution follows vasm's
/// probe-pinned anchor — every relative request, however deeply nested,
/// resolves against the **root input's directory** (vasm searches its process
/// cwd and the main source's directory, never the *including* file's; the
/// input's directory stands in for the cwd) then the `-I` dirs. State — the
/// local-label scope, `equ` constants feeding the optimizer's
/// `addq`/`lea`/`moveq` selections, the active `section` — threads through an
/// include and back out (textual-splice semantics, probe-pinned), and
/// `incbin`'s offset/length window matches vasm exactly (zero or negative
/// length means the rest of the file; an over-long length silently
/// truncates). Warnings ride the result; on success,
/// [`AssemblyResult::files`] holds the `FileId`→path table. The single-source
/// [`assemble_vasm_warned`] is unchanged and rejects both directives.
///
/// # Errors
/// A [`MultiFileError`] carrying the failure *and* the source map (file
/// table + include graph) built up to it, so a failure inside an included
/// file can still name its file and chain (KTD2).
pub fn assemble_vasm_warned_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    let mut map = source::SourceMap::new(input_path, source);
    match dialects::vasm::assemble_warned_multi(&mut map, loader) {
        Ok((bytes, warnings, _)) => {
            let mut result = AssemblyResult::image_warned(bytes, warnings);
            result.files = map.file_table();
            Ok(result)
        }
        Err(error) => Err(MultiFileError {
            error,
            source_map: Box::new(map),
        }),
    }
}

/// Assemble a **multi-file** 68000 program to an Amiga hunk executable
/// (language-surface U6): as [`assemble_vasm_warned_files`] — the same
/// `include`/`incbin` surface, resolution order, window semantics, and
/// cross-boundary state — serialized like [`assemble_vasm_exe`]
/// (`-Fhunkexe -kick1hunks`, debug symbol table omitted). The single-source
/// [`assemble_vasm_exe`] is unchanged and rejects both directives.
///
/// # Errors
/// A [`MultiFileError`] carrying the failure *and* the source map (KTD2).
pub fn assemble_vasm_exe_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    let mut map = source::SourceMap::new(input_path, source);
    match dialects::vasm::assemble_exe_multi(&mut map, loader) {
        Ok((bytes, warnings, _)) => {
            let mut result = AssemblyResult::image_warned(bytes, warnings);
            result.files = map.file_table();
            Ok(result)
        }
        Err(error) => Err(MultiFileError {
            error,
            source_map: Box::new(map),
        }),
    }
}

/// As [`assemble_vasm_warned_files`], also returning the full
/// [`debug198x::DebugInfo`] read out of assembly — the multi-file counterpart
/// of [`assemble_vasm_warned_debug`]. `Header.sources` is the file table in
/// `FileId` order (KTD2: `sources[i] ⇔ FileId(i)`, the same convention as
/// [`AssemblyResult::files`]) and every line span names the file its
/// statement was written in. Bytes are identical to
/// [`assemble_vasm_warned_files`] by construction (one `assemble_core` path;
/// AE2).
///
/// # Errors
/// As [`assemble_vasm_warned_files`].
pub fn assemble_vasm_warned_files_debug(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<(AssemblyResult, debug198x::DebugInfo), MultiFileError> {
    let mut map = source::SourceMap::new(input_path, source);
    match dialects::vasm::assemble_warned_multi(&mut map, loader) {
        Ok((bytes, warnings, capture)) => {
            let files = map.file_table();
            let info = listing::capture_debug_info_multi(capture, "68000", "vasm", files.clone());
            let mut result = AssemblyResult::image_warned(bytes, warnings);
            result.files = files;
            Ok((result, info))
        }
        Err(error) => Err(MultiFileError {
            error,
            source_map: Box::new(map),
        }),
    }
}

/// As [`assemble_vasm_exe_files`], also returning the full
/// [`debug198x::DebugInfo`] read out of assembly — the multi-file counterpart
/// of [`assemble_vasm_exe_debug`], describing the hunks the executable loads
/// with per-file line records (KTD2).
///
/// # Errors
/// As [`assemble_vasm_exe_files`].
pub fn assemble_vasm_exe_files_debug(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<(AssemblyResult, debug198x::DebugInfo), MultiFileError> {
    let mut map = source::SourceMap::new(input_path, source);
    match dialects::vasm::assemble_exe_multi(&mut map, loader) {
        Ok((bytes, warnings, capture)) => {
            let files = map.file_table();
            let info = listing::capture_debug_info_multi(capture, "68000", "vasm", files.clone());
            let mut result = AssemblyResult::image_warned(bytes, warnings);
            result.files = files;
            Ok((result, info))
        }
        Err(error) => Err(MultiFileError {
            error,
            source_map: Box::new(map),
        }),
    }
}

/// Reformat Motorola-syntax 68000 (`vasm`) source to canonical layout (the
/// `--fmt` formatter). Parses into the source-preserving semantic AST and emits
/// canonical same-dialect source — labels at column 0, operations indented,
/// comments preserved — reassembling byte-identical to the input. An
/// `include`/`incbin` directive renders verbatim; the target is never opened
/// (KTD1), so formatting succeeds when it is missing.
///
/// # Errors
/// Returns an [`AsmError`] on any parse failure.
pub fn format_vasm(source: &str) -> Result<String, AsmError> {
    Ok(ast::emit(&dialects::vasm::parse_program(source)?, false))
}

/// Assemble ca65-syntax 65816 source (native mode) into a flat binary — the
/// 65816 as a target extension of the 6502 (`isa::mos6502` + `isa::mos65816`).
/// Accumulator/index immediate width follows the `.a8`/`.a16`/`.i8`/`.i16`
/// directives. Matches `ca65 --cpu 65816` linked flat. Single-source: a
/// `.include`/`.incbin` directive is an error here — use
/// [`assemble_ca65_816_files`] for a multi-file program.
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_ca65_816(source: &str) -> Result<AssemblyResult, AsmError> {
    engine::assemble(source, &dialects::Ca65_816).map(AssemblyResult::from)
}

/// Assemble a **multi-file** ca65-65816 program (language-surface U4):
/// `source` is the root file's text, `input_path` its name (entry 0 of the
/// file table), and `.include`/`.incbin` directives resolve through `loader`
/// — the CLI wires an [`FsLoader`](source::FsLoader) carrying the input's
/// directory and the `-I` search dirs; tests wire a
/// [`MemoryLoader`](source::MemoryLoader) (KTD8). Resolution follows ca65's
/// probe-pinned order (the requesting file's directory, then each enclosing
/// includer's, innermost → outermost), state — constants *and* the
/// `.a8`/`.a16`/`.i8`/`.i16` width — threads through an include and back out,
/// and `.incbin`'s offset/size window matches ca65 exactly (a negative size
/// reads to EOF). On success, [`AssemblyResult::files`] holds the
/// `FileId`→path table. The single-source [`assemble_ca65_816`] is unchanged
/// and rejects both directives.
///
/// # Errors
/// A [`MultiFileError`] carrying the failure *and* the source map (file
/// table + include graph) built up to it, so a failure inside an included
/// file can still name its file and chain (KTD2).
pub fn assemble_ca65_816_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    assemble_files(&dialects::Ca65_816, source, input_path, loader)
}

/// Assemble ca65-syntax HuC6280 source into a flat little-endian binary — the
/// HuC6280 (PC Engine / TurboGrafx-16 CPU) as a target extension of the 6502
/// (`isa::mos6502` + `isa::huc6280`), mirroring the 65816 mechanism. Covers the
/// 65C02 additions, the Rockwell bit ops, and the HuC6280-specific instructions
/// (`st0`–`st2`, `tam`/`tma`, `tst`, `bsr`, and the block transfers). Matches
/// `ca65 --cpu huc6280` linked flat. Single-source: a `.include`/`.incbin`
/// directive is an error here — use [`assemble_ca65_huc6280_files`] for a
/// multi-file program.
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_ca65_huc6280(source: &str) -> Result<AssemblyResult, AsmError> {
    engine::assemble(source, &dialects::Ca65Huc6280).map(AssemblyResult::from)
}

/// Assemble a **multi-file** ca65-HuC6280 program (language-surface U4): as
/// [`assemble_ca65_816_files`] — the same ca65 `.include`/`.incbin` surface,
/// resolution order, and window semantics, over the HuC6280 target. The
/// single-source [`assemble_ca65_huc6280`] is unchanged and rejects both
/// directives.
///
/// # Errors
/// A [`MultiFileError`] carrying the failure *and* the source map (KTD2).
pub fn assemble_ca65_huc6280_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    assemble_files(&dialects::Ca65Huc6280, source, input_path, loader)
}

/// Assemble rgbasm-syntax SM83 (Game Boy) source into a flat binary at the
/// section's origin — the RGBDS dialect over [`isa::sm83`]. Covers the full
/// documented instruction set, `SECTION`/`db`/`dw`/`ds`/`EQU`, and `.local`
/// labels. Matches `rgbasm`/`rgblink` for the emitted bytes. Single-source:
/// an `INCLUDE`/`INCBIN` directive is an error here — use
/// [`assemble_rgbasm_files`] for a multi-file program.
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_rgbasm(source: &str) -> Result<AssemblyResult, AsmError> {
    engine::assemble(source, &dialects::Rgbasm).map(AssemblyResult::from)
}

/// Assemble a **multi-file** rgbasm program (language-surface U4): `source`
/// is the root file's text, `input_path` its name (entry 0 of the file
/// table), and `INCLUDE`/`INCBIN` directives resolve through `loader` — the
/// CLI wires an [`FsLoader`](source::FsLoader) carrying the input's directory
/// and the `-I` search dirs; tests wire a
/// [`MemoryLoader`](source::MemoryLoader) (KTD8). Resolution follows rgbasm's
/// probe-pinned anchor — every relative request, however deeply nested,
/// resolves against the **root input's directory** (rgbasm searches the
/// process cwd, never the including file's directory; the input's directory
/// stands in for the cwd) then the `-I` dirs. State — `DEF` constants
/// feeding `bit`/`rst`/`ds`, the `.local` scope's current global — threads
/// through an include and back out, and `INCBIN`'s offset/length window
/// matches rgbasm exactly (negative values rejected). On success,
/// [`AssemblyResult::files`] holds the `FileId`→path table. The
/// single-source [`assemble_rgbasm`] is unchanged and rejects both
/// directives.
///
/// # Errors
/// A [`MultiFileError`] carrying the failure *and* the source map (file
/// table + include graph) built up to it, so a failure inside an included
/// file can still name its file and chain (KTD2).
pub fn assemble_rgbasm_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    assemble_files(&dialects::Rgbasm, source, input_path, loader)
}

/// Assemble Intel-syntax 8080 source into a flat binary at the `org` — the
/// classic `MOV`/`MVI`/`LXI` mnemonics with radix-suffixed numbers (`42H`),
/// over [`isa::i8080`]. Matches `asl` (`cpu 8080`).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_i8080(source: &str) -> Result<AssemblyResult, AsmError> {
    engine::assemble(source, &dialects::I8080).map(AssemblyResult::from)
}

/// Assemble a **multi-file** Intel-8080 program (language-surface U4): `source`
/// is the root file's text, `input_path` its name (entry 0 of the file
/// table), and asl's `include`/`binclude` directives (quoted or bare names)
/// resolve through `loader` — the CLI wires an [`FsLoader`](source::FsLoader)
/// carrying the input's directory and the `-I` search dirs; tests wire a
/// [`MemoryLoader`](source::MemoryLoader) (KTD8). Resolution follows asl's
/// probe-pinned order: the requesting file's **own directory** (no cwd, no
/// root fallback), then the search dirs — and an extensionless `include`
/// request tries `name.inc` before the exact spelling. An `equ` defined
/// inside an include feeds the includer's later lines, and `binclude`'s
/// offset/length window matches asl exactly (strict: negatives and any
/// window past EOF are errors). On success, [`AssemblyResult::files`] holds
/// the `FileId`->path table. The single-source [`assemble_i8080`] is unchanged and
/// rejects both directives.
///
/// # Errors
/// A [`MultiFileError`] carrying the failure *and* the source map (file
/// table + include graph) built up to it, so a failure inside an included
/// file can still name its file and chain (KTD2).
pub fn assemble_i8080_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    assemble_files(&dialects::I8080, source, input_path, loader)
}

/// Assemble Motorola-syntax 6800 source into a flat big-endian binary at the
/// `org`, over [`isa::m6800`]. Motorola `$`-hex, `#` immediate, `$nn,X` indexed,
/// direct-vs-extended by size (or a `>`/`<` force). Matches `asl` (`cpu 6800`).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_m6800(source: &str) -> Result<AssemblyResult, AsmError> {
    engine::assemble(source, &dialects::M6800).map(AssemblyResult::from)
}

/// Assemble a **multi-file** Motorola-6800 program (language-surface U4): `source`
/// is the root file's text, `input_path` its name (entry 0 of the file
/// table), and asl's `include`/`binclude` directives (quoted or bare names)
/// resolve through `loader` — the CLI wires an [`FsLoader`](source::FsLoader)
/// carrying the input's directory and the `-I` search dirs; tests wire a
/// [`MemoryLoader`](source::MemoryLoader) (KTD8). Resolution follows asl's
/// probe-pinned order: the requesting file's **own directory** (no cwd, no
/// root fallback), then the search dirs — and an extensionless `include`
/// request tries `name.inc` before the exact spelling. An `equ` defined
/// inside an include feeds the includer's later lines, and `binclude`'s
/// offset/length window matches asl exactly (strict: negatives and any
/// window past EOF are errors). On success, [`AssemblyResult::files`] holds
/// the `FileId`->path table. The single-source [`assemble_m6800`] is unchanged and
/// rejects both directives.
///
/// # Errors
/// A [`MultiFileError`] carrying the failure *and* the source map (file
/// table + include graph) built up to it, so a failure inside an included
/// file can still name its file and chain (KTD2).
pub fn assemble_m6800_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    assemble_files(&dialects::M6800, source, input_path, loader)
}

/// Assemble asl-syntax RCA CDP1802 (COSMAC) source into a flat big-endian binary
/// at the `org`, over [`isa::cdp1802`]. Intel `H`-hex, bare register numbers, and
/// the page-relative short branch. Matches `asl` (`cpu 1802`).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_1802(source: &str) -> Result<AssemblyResult, AsmError> {
    engine::assemble(source, &dialects::Cdp1802).map(AssemblyResult::from)
}

/// Assemble a **multi-file** CDP1802 program (language-surface U4): `source`
/// is the root file's text, `input_path` its name (entry 0 of the file
/// table), and asl's `include`/`binclude` directives (quoted or bare names)
/// resolve through `loader` — the CLI wires an [`FsLoader`](source::FsLoader)
/// carrying the input's directory and the `-I` search dirs; tests wire a
/// [`MemoryLoader`](source::MemoryLoader) (KTD8). Resolution follows asl's
/// probe-pinned order: the requesting file's **own directory** (no cwd, no
/// root fallback), then the search dirs — and an extensionless `include`
/// request tries `name.inc` before the exact spelling. An `equ` defined
/// inside an include feeds the includer's later lines, and `binclude`'s
/// offset/length window matches asl exactly (strict: negatives and any
/// window past EOF are errors). On success, [`AssemblyResult::files`] holds
/// the `FileId`->path table. The single-source [`assemble_1802`] is unchanged and
/// rejects both directives.
///
/// # Errors
/// A [`MultiFileError`] carrying the failure *and* the source map (file
/// table + include graph) built up to it, so a failure inside an included
/// file can still name its file and chain (KTD2).
pub fn assemble_1802_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    assemble_files(&dialects::Cdp1802, source, input_path, loader)
}

/// Assemble asl-syntax Intel 8048 (MCS-48) source into a flat binary at the
/// `org`, over [`isa::i8048`]. Intel `H`-hex; the mode label is the operand
/// template; `JMP`/`CALL` pack the address page into the opcode via the
/// computed-operand seam. Matches `asl` (`cpu 8048`).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_8048(source: &str) -> Result<AssemblyResult, AsmError> {
    engine::assemble(source, &dialects::I8048 { romless: false }).map(AssemblyResult::from)
}

/// Assemble a **multi-file** 8048 program (language-surface U4): `source`
/// is the root file's text, `input_path` its name (entry 0 of the file
/// table), and asl's `include`/`binclude` directives (quoted or bare names)
/// resolve through `loader` — the CLI wires an [`FsLoader`](source::FsLoader)
/// carrying the input's directory and the `-I` search dirs; tests wire a
/// [`MemoryLoader`](source::MemoryLoader) (KTD8). Resolution follows asl's
/// probe-pinned order: the requesting file's **own directory** (no cwd, no
/// root fallback), then the search dirs — and an extensionless `include`
/// request tries `name.inc` before the exact spelling. An `equ` defined
/// inside an include feeds the includer's later lines, and `binclude`'s
/// offset/length window matches asl exactly (strict: negatives and any
/// window past EOF are errors). On success, [`AssemblyResult::files`] holds
/// the `FileId`->path table. The single-source [`assemble_8048`] is unchanged and
/// rejects both directives.
///
/// # Errors
/// A [`MultiFileError`] carrying the failure *and* the source map (file
/// table + include graph) built up to it, so a failure inside an included
/// file can still name its file and chain (KTD2).
pub fn assemble_8048_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    assemble_files(
        &dialects::I8048 { romless: false },
        source,
        input_path,
        loader,
    )
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
pub fn assemble_8039(source: &str) -> Result<AssemblyResult, AsmError> {
    engine::assemble(source, &dialects::I8048 { romless: true }).map(AssemblyResult::from)
}

/// Assemble a **multi-file** ROM-less MCS-48 program (language-surface U4): `source`
/// is the root file's text, `input_path` its name (entry 0 of the file
/// table), and asl's `include`/`binclude` directives (quoted or bare names)
/// resolve through `loader` — the CLI wires an [`FsLoader`](source::FsLoader)
/// carrying the input's directory and the `-I` search dirs; tests wire a
/// [`MemoryLoader`](source::MemoryLoader) (KTD8). Resolution follows asl's
/// probe-pinned order: the requesting file's **own directory** (no cwd, no
/// root fallback), then the search dirs — and an extensionless `include`
/// request tries `name.inc` before the exact spelling. An `equ` defined
/// inside an include feeds the includer's later lines, and `binclude`'s
/// offset/length window matches asl exactly (strict: negatives and any
/// window past EOF are errors). On success, [`AssemblyResult::files`] holds
/// the `FileId`->path table. The single-source [`assemble_8039`] is unchanged and
/// rejects both directives.
///
/// # Errors
/// A [`MultiFileError`] carrying the failure *and* the source map (file
/// table + include graph) built up to it, so a failure inside an included
/// file can still name its file and chain (KTD2).
pub fn assemble_8039_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    assemble_files(
        &dialects::I8048 { romless: true },
        source,
        input_path,
        loader,
    )
}

/// Assemble asl-syntax National SC/MP (INS8060) source into a flat binary at the
/// `org`, over [`isa::scmp`]. C-style numbers (`0x..` hex); `disp(ptr)` /
/// `@disp(ptr)` memory references (the literal `e` selects the E-register
/// index), pointer-exchange, and immediate forms. Matches `asl` (`cpu SC/MP`).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_scmp(source: &str) -> Result<AssemblyResult, AsmError> {
    engine::assemble(source, &dialects::Scmp).map(AssemblyResult::from)
}

/// Assemble a **multi-file** SC/MP program (language-surface U4): `source`
/// is the root file's text, `input_path` its name (entry 0 of the file
/// table), and asl's `include`/`binclude` directives (quoted or bare names)
/// resolve through `loader` — the CLI wires an [`FsLoader`](source::FsLoader)
/// carrying the input's directory and the `-I` search dirs; tests wire a
/// [`MemoryLoader`](source::MemoryLoader) (KTD8). Resolution follows asl's
/// probe-pinned order: the requesting file's **own directory** (no cwd, no
/// root fallback), then the search dirs — and an extensionless `include`
/// request tries `name.inc` before the exact spelling. An `equ` defined
/// inside an include feeds the includer's later lines, and `binclude`'s
/// offset/length window matches asl exactly (strict: negatives and any
/// window past EOF are errors). On success, [`AssemblyResult::files`] holds
/// the `FileId`->path table. The single-source [`assemble_scmp`] is unchanged and
/// rejects both directives.
///
/// # Errors
/// A [`MultiFileError`] carrying the failure *and* the source map (file
/// table + include graph) built up to it, so a failure inside an included
/// file can still name its file and chain (KTD2).
pub fn assemble_scmp_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    assemble_files(&dialects::Scmp, source, input_path, loader)
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
pub fn assemble_f8(source: &str) -> Result<AssemblyResult, AsmError> {
    engine::assemble(source, &dialects::F8).map(AssemblyResult::from)
}

/// Assemble a **multi-file** F8 program (language-surface U4): `source`
/// is the root file's text, `input_path` its name (entry 0 of the file
/// table), and asl's `include`/`binclude` directives (quoted or bare names)
/// resolve through `loader` — the CLI wires an [`FsLoader`](source::FsLoader)
/// carrying the input's directory and the `-I` search dirs; tests wire a
/// [`MemoryLoader`](source::MemoryLoader) (KTD8). Resolution follows asl's
/// probe-pinned order: the requesting file's **own directory** (no cwd, no
/// root fallback), then the search dirs — and an extensionless `include`
/// request tries `name.inc` before the exact spelling. An `equ` defined
/// inside an include feeds the includer's later lines, and `binclude`'s
/// offset/length window matches asl exactly (strict: negatives and any
/// window past EOF are errors). On success, [`AssemblyResult::files`] holds
/// the `FileId`->path table. The single-source [`assemble_f8`] is unchanged and
/// rejects both directives.
///
/// # Errors
/// A [`MultiFileError`] carrying the failure *and* the source map (file
/// table + include graph) built up to it, so a failure inside an included
/// file can still name its file and chain (KTD2).
pub fn assemble_f8_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    assemble_files(&dialects::F8, source, input_path, loader)
}

/// Assemble asl-syntax Signetics 2650 source into a flat binary at the `org`,
/// over [`isa::s2650`]. `$`-hex; the `mnemonic,reg`/`mnemonic,cc` comma syntax;
/// register / immediate / 7-bit relative (indirect `*`) / 15-bit absolute
/// (indirect + `,r3` indexing) addressing, the relative and absolute forms via
/// the computed-operand seam. Matches `asl` (`cpu 2650`).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_2650(source: &str) -> Result<AssemblyResult, AsmError> {
    engine::assemble(source, &dialects::S2650).map(AssemblyResult::from)
}

/// Assemble a **multi-file** 2650 program (language-surface U4): `source`
/// is the root file's text, `input_path` its name (entry 0 of the file
/// table), and asl's `include`/`binclude` directives (quoted or bare names)
/// resolve through `loader` — the CLI wires an [`FsLoader`](source::FsLoader)
/// carrying the input's directory and the `-I` search dirs; tests wire a
/// [`MemoryLoader`](source::MemoryLoader) (KTD8). Resolution follows asl's
/// probe-pinned order: the requesting file's **own directory** (no cwd, no
/// root fallback), then the search dirs — and an extensionless `include`
/// request tries `name.inc` before the exact spelling. An `equ` defined
/// inside an include feeds the includer's later lines, and `binclude`'s
/// offset/length window matches asl exactly (strict: negatives and any
/// window past EOF are errors). On success, [`AssemblyResult::files`] holds
/// the `FileId`->path table. The single-source [`assemble_2650`] is unchanged and
/// rejects both directives.
///
/// # Errors
/// A [`MultiFileError`] carrying the failure *and* the source map (file
/// table + include graph) built up to it, so a failure inside an included
/// file can still name its file and chain (KTD2).
pub fn assemble_2650_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    assemble_files(&dialects::S2650, source, input_path, loader)
}

/// Assemble asl-syntax TI TMS7000 source into a flat binary at the `org`, over
/// [`isa::tms7000`]. Intel `H`-hex; operands classified by prefix (`A`/`B`,
/// `%n` immediate, `Rn` register file, `Pn` peripheral, `@nnnn` direct, `*Rn`
/// indirect, `@nnnn(B)` indexed). Standard 8-bit relative jumps; `TRAP n`
/// encodes as `0xFF - n`. Matches `asl` (`cpu TMS70C00`).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_tms7000(source: &str) -> Result<AssemblyResult, AsmError> {
    engine::assemble(source, &dialects::Tms7000).map(AssemblyResult::from)
}

/// Assemble a **multi-file** TMS7000 program (language-surface U4): `source`
/// is the root file's text, `input_path` its name (entry 0 of the file
/// table), and asl's `include`/`binclude` directives (quoted or bare names)
/// resolve through `loader` — the CLI wires an [`FsLoader`](source::FsLoader)
/// carrying the input's directory and the `-I` search dirs; tests wire a
/// [`MemoryLoader`](source::MemoryLoader) (KTD8). Resolution follows asl's
/// probe-pinned order: the requesting file's **own directory** (no cwd, no
/// root fallback), then the search dirs — and an extensionless `include`
/// request tries `name.inc` before the exact spelling. An `equ` defined
/// inside an include feeds the includer's later lines, and `binclude`'s
/// offset/length window matches asl exactly (strict: negatives and any
/// window past EOF are errors). On success, [`AssemblyResult::files`] holds
/// the `FileId`->path table. The single-source [`assemble_tms7000`] is unchanged and
/// rejects both directives.
///
/// # Errors
/// A [`MultiFileError`] carrying the failure *and* the source map (file
/// table + include graph) built up to it, so a failure inside an included
/// file can still name its file and chain (KTD2).
pub fn assemble_tms7000_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    assemble_files(&dialects::Tms7000, source, input_path, loader)
}

/// Assemble asl-syntax DEC PDP-11 source into a flat **little-endian** binary at
/// the `org`, over [`isa::pdp11`]. Decimal-default numbers (`0x` hex), registers
/// `r0`–`r7` (`sp`/`pc`), and the eight addressing modes (`Rn`, `(Rn)`, `(Rn)+`,
/// `@(Rn)+`, `-(Rn)`, `@-(Rn)`, `X(Rn)`, `@X(Rn)`, plus `#n`, `@#n`, and
/// PC-relative `addr`/`@addr`). Covers the integer instruction set including EIS
/// and the J-11 additions; the FP11 floating-point set is out of scope. Matches
/// `asl` (`cpu MICROPDP-11/93`).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_pdp11(source: &str) -> Result<AssemblyResult, AsmError> {
    engine::assemble(source, &dialects::Pdp11).map(AssemblyResult::from)
}

/// Assemble a **multi-file** PDP-11 program (language-surface U4): `source`
/// is the root file's text, `input_path` its name (entry 0 of the file
/// table), and asl's `include`/`binclude` directives (quoted or bare names)
/// resolve through `loader` — the CLI wires an [`FsLoader`](source::FsLoader)
/// carrying the input's directory and the `-I` search dirs; tests wire a
/// [`MemoryLoader`](source::MemoryLoader) (KTD8). Resolution follows asl's
/// probe-pinned order: the requesting file's **own directory** (no cwd, no
/// root fallback), then the search dirs — and an extensionless `include`
/// request tries `name.inc` before the exact spelling. An `equ` defined
/// inside an include feeds the includer's later lines, and `binclude`'s
/// offset/length window matches asl exactly (strict: negatives and any
/// window past EOF are errors). On success, [`AssemblyResult::files`] holds
/// the `FileId`->path table. The single-source [`assemble_pdp11`] is unchanged and
/// rejects both directives.
///
/// # Errors
/// A [`MultiFileError`] carrying the failure *and* the source map (file
/// table + include graph) built up to it, so a failure inside an included
/// file can still name its file and chain (KTD2).
pub fn assemble_pdp11_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    assemble_files(&dialects::Pdp11, source, input_path, loader)
}

/// Assemble asl-syntax GI CP1610 source (the Mattel Intellivision CPU) into a
/// flat **big-endian** binary at the `org`, over [`isa::cp1610`] — one 16-bit
/// word per 10-bit decle. Intel `h`-hex, registers `r0`–`r7`, jzIntv / as1600
/// mnemonics. Built as sweep-verified increments; **increment 1** covers the
/// single-decle register / implied groups. Matches `asl` (`cpu CP-1600`).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_cp1610(source: &str) -> Result<AssemblyResult, AsmError> {
    engine::assemble(source, &dialects::Cp1610).map(AssemblyResult::from)
}

/// Assemble a **multi-file** CP1610 program (language-surface U4): as the
/// other asl-chip `*_files` entries (asl's quoted-or-bare `include`/
/// `binclude`, requester-directory resolution, the `.inc` extension default,
/// state threading through the boundary), with the CP1610's probe-pinned
/// `binclude` accounting: offset/length count **bytes**, and an N-byte
/// window occupies N **decles** — the image carries the N raw file bytes
/// followed by N zero bytes, exactly as `asl` (`cpu CP-1600`) + p2bin lay it
/// down (an odd byte count is legal, not padded to a decle boundary and not
/// packed two-per-decle). The single-source [`assemble_cp1610`] is unchanged
/// and rejects both directives.
///
/// # Errors
/// A [`MultiFileError`] carrying the failure *and* the source map (file
/// table + include graph) built up to it, so a failure inside an included
/// file can still name its file and chain (KTD2).
pub fn assemble_cp1610_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    assemble_files(&dialects::Cp1610, source, input_path, loader)
}

/// Assemble asl-syntax TI TMS9900 source into a flat **big-endian** binary at
/// the `org`, over [`isa::tms9900`]. Intel `h`-hex, registers `r0`–`r15`, and
/// the general-addressing modes (`Rn`, `*Rn`, `@addr`, `@addr(Rn)`, `*Rn+`).
/// Covers the base TMS9900 integer set (the TI-99/4A CPU); the TMS9995 /
/// TMS99105 supersets are out of scope. Matches `asl` (`cpu TMS9900`).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_tms9900(source: &str) -> Result<AssemblyResult, AsmError> {
    engine::assemble(source, &dialects::Tms9900).map(AssemblyResult::from)
}

/// Assemble a **multi-file** TMS9900 program (language-surface U4): `source`
/// is the root file's text, `input_path` its name (entry 0 of the file
/// table), and asl's `include`/`binclude` directives (quoted or bare names)
/// resolve through `loader` — the CLI wires an [`FsLoader`](source::FsLoader)
/// carrying the input's directory and the `-I` search dirs; tests wire a
/// [`MemoryLoader`](source::MemoryLoader) (KTD8). Resolution follows asl's
/// probe-pinned order: the requesting file's **own directory** (no cwd, no
/// root fallback), then the search dirs — and an extensionless `include`
/// request tries `name.inc` before the exact spelling. An `equ` defined
/// inside an include feeds the includer's later lines, and `binclude`'s
/// offset/length window matches asl exactly (strict: negatives and any
/// window past EOF are errors). On success, [`AssemblyResult::files`] holds
/// the `FileId`->path table. The single-source [`assemble_tms9900`] is unchanged and
/// rejects both directives.
///
/// # Errors
/// A [`MultiFileError`] carrying the failure *and* the source map (file
/// table + include graph) built up to it, so a failure inside an included
/// file can still name its file and chain (KTD2).
pub fn assemble_tms9900_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    assemble_files(&dialects::Tms9900, source, input_path, loader)
}

/// Assemble asl-syntax Zilog Z8000 (non-segmented Z8002) source into a flat
/// **big-endian** binary at the `org`, over [`isa::z8000`]. Intel `h`-hex, word
/// registers `r0`–`r15`, byte `rh`/`rl`. Built as sweep-verified increments;
/// **increment 1** covers the dyadic arithmetic / logic / load family across the
/// register, immediate, indirect, direct, and indexed modes. Matches `asl`
/// (`cpu Z8002`).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_z8000(source: &str) -> Result<AssemblyResult, AsmError> {
    engine::assemble(source, &dialects::Z8000 { seg: false }).map(AssemblyResult::from)
}

/// Assemble a **multi-file** Z8000 (Z8002) program (language-surface U4): `source`
/// is the root file's text, `input_path` its name (entry 0 of the file
/// table), and asl's `include`/`binclude` directives (quoted or bare names)
/// resolve through `loader` — the CLI wires an [`FsLoader`](source::FsLoader)
/// carrying the input's directory and the `-I` search dirs; tests wire a
/// [`MemoryLoader`](source::MemoryLoader) (KTD8). Resolution follows asl's
/// probe-pinned order: the requesting file's **own directory** (no cwd, no
/// root fallback), then the search dirs — and an extensionless `include`
/// request tries `name.inc` before the exact spelling. An `equ` defined
/// inside an include feeds the includer's later lines, and `binclude`'s
/// offset/length window matches asl exactly (strict: negatives and any
/// window past EOF are errors). On success, [`AssemblyResult::files`] holds
/// the `FileId`->path table. The single-source [`assemble_z8000`] is unchanged and
/// rejects both directives.
///
/// # Errors
/// A [`MultiFileError`] carrying the failure *and* the source map (file
/// table + include graph) built up to it, so a failure inside an included
/// file can still name its file and chain (KTD2).
pub fn assemble_z8000_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    assemble_files(&dialects::Z8000 { seg: false }, source, input_path, loader)
}

/// Assemble `asl`-syntax **segmented Z8001** source into a flat big-endian
/// binary. Like [`assemble_z8000`] but with segmented memory operands
/// (`<<seg>>offset` direct/indexed addresses, `@RRn` long-pair pointers).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_z8001(source: &str) -> Result<AssemblyResult, AsmError> {
    engine::assemble(source, &dialects::Z8000 { seg: true }).map(AssemblyResult::from)
}

/// Assemble a **multi-file** segmented Z8001 program (language-surface U4): `source`
/// is the root file's text, `input_path` its name (entry 0 of the file
/// table), and asl's `include`/`binclude` directives (quoted or bare names)
/// resolve through `loader` — the CLI wires an [`FsLoader`](source::FsLoader)
/// carrying the input's directory and the `-I` search dirs; tests wire a
/// [`MemoryLoader`](source::MemoryLoader) (KTD8). Resolution follows asl's
/// probe-pinned order: the requesting file's **own directory** (no cwd, no
/// root fallback), then the search dirs — and an extensionless `include`
/// request tries `name.inc` before the exact spelling. An `equ` defined
/// inside an include feeds the includer's later lines, and `binclude`'s
/// offset/length window matches asl exactly (strict: negatives and any
/// window past EOF are errors). On success, [`AssemblyResult::files`] holds
/// the `FileId`->path table. The single-source [`assemble_z8001`] is unchanged and
/// rejects both directives.
///
/// # Errors
/// A [`MultiFileError`] carrying the failure *and* the source map (file
/// table + include graph) built up to it, so a failure inside an included
/// file can still name its file and chain (KTD2).
pub fn assemble_z8001_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    assemble_files(&dialects::Z8000 { seg: true }, source, input_path, loader)
}

/// Assemble lwasm-syntax 6809 source into a flat big-endian binary — matching
/// `lwasm --6809 --raw`. Covers inherent, immediate, direct, extended, and
/// relative (short + long) addressing; indexed addressing is not yet supported.
/// Single-source: an `include`/`use`/`includebin` directive is an error here —
/// use [`assemble_lwasm_files`] for a multi-file program.
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_lwasm(source: &str) -> Result<AssemblyResult, AsmError> {
    engine::assemble(source, &dialects::Lwasm).map(AssemblyResult::from)
}

/// Assemble a **multi-file** lwasm program (language-surface U4): `source` is
/// the root file's text, `input_path` its name (entry 0 of the file table),
/// and `include`/`use` (both spellings, quoted or bare names) and
/// `includebin "file"[,offset[,length]]` directives resolve through `loader`
/// — the CLI wires an [`FsLoader`](source::FsLoader) carrying the input's
/// directory and the `-I` search dirs; tests wire a
/// [`MemoryLoader`](source::MemoryLoader) (KTD8). Resolution follows lwasm's
/// probe-pinned order — the **requesting file's own directory**, then the
/// `-I` dirs (no cwd, no root fallback, no ancestor hops). An `equ` defined
/// inside an include feeds the includer's later direct-vs-extended selection,
/// and `includebin`'s window matches lwasm exactly (a negative offset counts
/// back from EOF). On success, [`AssemblyResult::files`] holds the
/// `FileId`→path table. The single-source [`assemble_lwasm`] is unchanged
/// and rejects the directives.
///
/// # Errors
/// A [`MultiFileError`] carrying the failure *and* the source map (file
/// table + include graph) built up to it, so a failure inside an included
/// file can still name its file and chain (KTD2).
pub fn assemble_lwasm_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    assemble_files(&dialects::Lwasm, source, input_path, loader)
}

/// Assemble pasmo-syntax Z80 source into a flat binary, targeting a **plain
/// Z80** (Z80N opcodes are rejected, as vanilla pasmo rejects them).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_pasmo(source: &str) -> Result<AssemblyResult, AsmError> {
    engine::assemble(source, &dialects::Pasmo { z80n: false }).map(AssemblyResult::from)
}

/// Assemble pasmo-syntax Z80 source targeting the **ZX Spectrum Next (Z80N)** —
/// the same syntax as [`assemble_pasmo`] with the Z80N opcodes also available
/// (what `pasmonext` does).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_pasmonext(source: &str) -> Result<AssemblyResult, AsmError> {
    engine::assemble(source, &dialects::Pasmo { z80n: true }).map(AssemblyResult::from)
}

/// Assemble sjasmplus-syntax Z80 source targeting a plain Z80. Single-source:
/// an `include` directive is an error here — use
/// [`assemble_sjasmplus_files`] for a multi-file program.
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_sjasmplus(source: &str) -> Result<AssemblyResult, AsmError> {
    engine::assemble(source, &dialects::Sjasmplus { z80n: false }).map(AssemblyResult::from)
}

/// The failure shape of the include-capable entry points (language-surface
/// U2, KTD2): the assembly error **plus the source map built up to the
/// failure** — its [`file_table`](source::SourceMap::file_table) resolves the
/// error span's `FileId` and its
/// [`include_chain`](source::SourceMap::include_chain) yields the
/// `included from` notes. An error inside an included file is a failure-path
/// scenario, so the table must survive an `Err`; this type is where it rides.
#[derive(Debug)]
pub struct MultiFileError {
    /// The underlying assembly failure; its span's `FileId` indexes
    /// [`source_map`](Self::source_map)'s file table.
    pub error: AsmError,
    /// Every file loaded before the failure: the `FileId` table and the
    /// include graph. Boxed to keep the `Err` variant lean on the happy path
    /// (clippy `result_large_err`); methods read through the box unchanged.
    pub source_map: Box<source::SourceMap>,
}

impl std::fmt::Display for MultiFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

impl std::error::Error for MultiFileError {}

/// The shared body of the include-capable entry points: root at `FileId(0)`,
/// includes resolved through `loader`, and the file table populated on
/// success — or carried on the error (KTD2).
fn assemble_files(
    dialect: &dyn dialect::Dialect,
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    let mut map = source::SourceMap::new(input_path, source);
    match engine::assemble_multi(&mut map, loader, dialect) {
        Ok(assembly) => {
            let mut result = AssemblyResult::from(assembly);
            result.files = map.file_table();
            Ok(result)
        }
        Err(error) => Err(MultiFileError {
            error,
            source_map: Box::new(map),
        }),
    }
}

/// Assemble a **multi-file** sjasmplus program (language-surface U2):
/// `source` is the root file's text, `input_path` its name (entry 0 of the
/// file table), and `INCLUDE` directives resolve through `loader` — the CLI
/// wires an [`FsLoader`](source::FsLoader) carrying the input's directory and
/// the `-I` search dirs; tests wire a [`MemoryLoader`](source::MemoryLoader)
/// (KTD8). On success, [`AssemblyResult::files`] holds the `FileId`→path
/// table. The single-source [`assemble_sjasmplus`] is unchanged and rejects
/// includes.
///
/// # Errors
/// A [`MultiFileError`] carrying the failure *and* the source map (file
/// table + include graph) built up to it, so a failure inside an included
/// file can still name its file and chain (KTD2).
pub fn assemble_sjasmplus_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    assemble_files(
        &dialects::Sjasmplus { z80n: false },
        source,
        input_path,
        loader,
    )
}

/// As [`assemble_sjasmplus_files`], targeting the ZX Spectrum Next (Z80N).
///
/// # Errors
/// As [`assemble_sjasmplus_files`].
pub fn assemble_sjasmplus_next_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    assemble_files(
        &dialects::Sjasmplus { z80n: true },
        source,
        input_path,
        loader,
    )
}

/// Assemble a **multi-file** pasmo program (language-surface U3): as
/// [`assemble_sjasmplus_files`], but pasmo's multi-file surface today is only
/// the plain `incbin "file"` (probe-pinned — the reference has no
/// offset/length tail; its `include` lands in U4), resolved through `loader`'s
/// binary path. The single-source [`assemble_pasmo`] is unchanged and rejects
/// an `incbin` with a pointer here.
///
/// # Errors
/// A [`MultiFileError`] carrying the failure *and* the source map (KTD2).
pub fn assemble_pasmo_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    assemble_files(&dialects::Pasmo { z80n: false }, source, input_path, loader)
}

/// As [`assemble_pasmo_files`], targeting the ZX Spectrum Next (Z80N) — what
/// `pasmonext` does.
///
/// # Errors
/// As [`assemble_pasmo_files`].
pub fn assemble_pasmonext_files(
    source: &str,
    input_path: &str,
    loader: &dyn source::SourceLoader,
) -> Result<AssemblyResult, MultiFileError> {
    assemble_files(&dialects::Pasmo { z80n: true }, source, input_path, loader)
}

/// Assemble sjasmplus-syntax Z80 source targeting the ZX Spectrum Next (Z80N).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse, range, or
/// symbol-resolution failure.
pub fn assemble_sjasmplus_next(source: &str) -> Result<AssemblyResult, AsmError> {
    engine::assemble(source, &dialects::Sjasmplus { z80n: true }).map(AssemblyResult::from)
}

/// Format pasmo-syntax Z80 source (`asm198x fmt`): parse into the semantic AST
/// and emit canonical same-dialect source — labels at column 0, operations
/// indented, comments preserved in position, operand spelling untouched. The
/// result assembles byte-identical to the input and is idempotent (U5, AE7).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse failure.
pub fn format_pasmo(source: &str) -> Result<String, AsmError> {
    format_ast(&dialects::Pasmo { z80n: false }, source)
}

/// Format pasmonext-syntax (Z80N) source — see [`format_pasmo`].
///
/// # Errors
/// Returns an [`AsmError`] on any parse failure.
pub fn format_pasmonext(source: &str) -> Result<String, AsmError> {
    format_ast(&dialects::Pasmo { z80n: true }, source)
}

/// Format sjasmplus-syntax Z80 source — see [`format_pasmo`].
///
/// # Errors
/// Returns an [`AsmError`] on any parse failure.
pub fn format_sjasmplus(source: &str) -> Result<String, AsmError> {
    format_ast(&dialects::Sjasmplus { z80n: false }, source)
}

/// Format sjasmplus-syntax (Z80N) source — see [`format_pasmo`].
///
/// # Errors
/// Returns an [`AsmError`] on any parse failure.
pub fn format_sjasmplus_next(source: &str) -> Result<String, AsmError> {
    format_ast(&dialects::Sjasmplus { z80n: true }, source)
}

/// Format Intel-syntax 8080 source (`asm198x fmt --cpu 8080`): parse into the
/// semantic AST and emit canonical same-dialect source — labels at column 0,
/// operations indented, comments preserved in position, radix-suffixed operand
/// spelling untouched. The result assembles byte-identical to the input and is
/// idempotent (U6 extends the U5 formatter to the first fixed-slot CPU).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse failure.
pub fn format_i8080(source: &str) -> Result<String, AsmError> {
    format_ast(&dialects::I8080, source)
}

/// Reformat Intel 8048 (MCS-48) source to canonical layout (the `--fmt`
/// formatter). Reassembles byte-identical to the input.
///
/// # Errors
/// Returns an [`AsmError`] on any parse failure.
pub fn format_8048(source: &str) -> Result<String, AsmError> {
    format_ast(&dialects::I8048 { romless: false }, source)
}

/// As [`format_8048`], for the ROM-less MCS-48 parts (8035/8039/8040): the four
/// BUS-port instructions are rejected, matching assembly.
///
/// # Errors
/// Returns an [`AsmError`] on any parse failure.
pub fn format_8039(source: &str) -> Result<String, AsmError> {
    format_ast(&dialects::I8048 { romless: true }, source)
}

/// Reformat Fairchild F8 (3850) source to canonical layout (the `--fmt`
/// formatter). Reassembles byte-identical to the input.
///
/// # Errors
/// Returns an [`AsmError`] on any parse failure.
pub fn format_f8(source: &str) -> Result<String, AsmError> {
    format_ast(&dialects::F8, source)
}

/// Reformat Signetics 2650 source to canonical layout (the `--fmt` formatter).
/// Reassembles byte-identical to the input.
///
/// # Errors
/// Returns an [`AsmError`] on any parse failure.
pub fn format_2650(source: &str) -> Result<String, AsmError> {
    format_ast(&dialects::S2650, source)
}

/// Reformat TI TMS7000 source to canonical layout (the `--fmt` formatter).
/// Reassembles byte-identical to the input.
///
/// # Errors
/// Returns an [`AsmError`] on any parse failure.
pub fn format_tms7000(source: &str) -> Result<String, AsmError> {
    format_ast(&dialects::Tms7000, source)
}

/// Reformat ca65-syntax 65816 source to canonical layout (the `--fmt`
/// formatter). Reassembles byte-identical to the input — the `.a8`/`.a16` width
/// directives are preserved, so width-dependent immediates round-trip.
///
/// # Errors
/// Returns an [`AsmError`] on any parse failure.
pub fn format_ca65_816(source: &str) -> Result<String, AsmError> {
    format_ast(&dialects::Ca65_816, source)
}

/// Reformat ca65-syntax HuC6280 source to canonical layout (the `--fmt`
/// formatter). Reassembles byte-identical to the input.
///
/// # Errors
/// Returns an [`AsmError`] on any parse failure.
pub fn format_ca65_huc6280(source: &str) -> Result<String, AsmError> {
    format_ast(&dialects::Ca65Huc6280, source)
}

/// Reformat asl-syntax PDP-11 source to canonical layout (the `--fmt`
/// formatter). Reassembles byte-identical to the input — the first field-packed
/// CPU to route through the AST formatter.
///
/// # Errors
/// Returns an [`AsmError`] on any parse failure.
pub fn format_pdp11(source: &str) -> Result<String, AsmError> {
    format_ast(&dialects::Pdp11, source)
}

/// Reformat asl-syntax TMS9900 source to canonical layout (the `--fmt`
/// formatter). Reassembles byte-identical to the input.
///
/// # Errors
/// Returns an [`AsmError`] on any parse failure.
pub fn format_tms9900(source: &str) -> Result<String, AsmError> {
    format_ast(&dialects::Tms9900, source)
}

/// Reformat asl-syntax CP1610 source to canonical layout (the `--fmt`
/// formatter). Reassembles byte-identical to the input — the word-addressed
/// decles and the `SDBD` two-decle immediates round-trip.
///
/// # Errors
/// Returns an [`AsmError`] on any parse failure.
pub fn format_cp1610(source: &str) -> Result<String, AsmError> {
    format_ast(&dialects::Cp1610, source)
}

/// Reformat asl-syntax Zilog Z8000 (non-segmented Z8002) source to canonical
/// layout (the `--fmt` formatter). Reassembles byte-identical to the input.
///
/// # Errors
/// Returns an [`AsmError`] on any parse failure.
pub fn format_z8000(source: &str) -> Result<String, AsmError> {
    format_ast(&dialects::Z8000 { seg: false }, source)
}

/// Reformat asl-syntax Zilog Z8001 (segmented) source to canonical layout (the
/// `--fmt` formatter). Reassembles byte-identical to the input.
///
/// # Errors
/// Returns an [`AsmError`] on any parse failure.
pub fn format_z8001(source: &str) -> Result<String, AsmError> {
    format_ast(&dialects::Z8000 { seg: true }, source)
}

/// Format Motorola-syntax 6800 source (`asm198x fmt --cpu 6800`): parse into the
/// semantic AST and emit canonical same-dialect source — labels at column 0,
/// operations indented, comments preserved, `$`-hex operand spelling untouched.
/// The result assembles byte-identical to the input and is idempotent (U6).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse failure.
pub fn format_m6800(source: &str) -> Result<String, AsmError> {
    format_ast(&dialects::M6800, source)
}

/// Format asl-syntax CDP1802 (COSMAC) source (`asm198x fmt --cpu 1802`): parse
/// into the semantic AST and emit canonical same-dialect source — labels at
/// column 0, operations indented, comments preserved, `H`-hex operand spelling
/// untouched. Assembles byte-identical to the input and is idempotent (U6).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse failure.
pub fn format_1802(source: &str) -> Result<String, AsmError> {
    format_ast(&dialects::Cdp1802, source)
}

/// Format asl-syntax National SC/MP (INS8060) source (`asm198x fmt --cpu scmp`):
/// parse into the semantic AST and emit canonical same-dialect source — labels at
/// column 0, operations indented, comments preserved, C-style operand spelling
/// (`0x..`) untouched. Assembles byte-identical to the input and is idempotent (U6).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse failure.
pub fn format_scmp(source: &str) -> Result<String, AsmError> {
    format_ast(&dialects::Scmp, source)
}

/// Format rgbasm (RGBDS / Game Boy SM83) source (`asm198x fmt --cpu rgbasm`):
/// parse into the semantic AST and emit canonical same-dialect source — `name:`
/// labels at column 0, operations indented, `SECTION` directives and comments
/// preserved, and scoped `.local` labels re-emitted in source form. Assembles
/// byte-identical to the input and is idempotent (U6).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse failure.
pub fn format_rgbasm(source: &str) -> Result<String, AsmError> {
    format_ast(&dialects::Rgbasm, source)
}

/// Format lwasm (Motorola 6809) source (`asm198x fmt --cpu 6809`): parse into the
/// semantic AST and emit canonical same-dialect source — labels at column 0,
/// operations indented, comments preserved, operand spelling untouched. The 6809
/// is the first **computed-operand** CPU with a formatter: an instruction's
/// precomputed bytes are held verbatim (`Item::Encoded`) and re-emitted from the
/// node's source, so the result assembles byte-identical and is idempotent (U6).
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse failure.
pub fn format_lwasm(source: &str) -> Result<String, AsmError> {
    format_ast(&dialects::Lwasm, source)
}

/// Format ACME (C64 6502) source (`asm198x fmt --cpu acme`): parse into the
/// source-preserving semantic AST and emit the canonical layout — labels at
/// column 0, operations indented, comments repositioned, conditional (`!if`/
/// `!ifdef`/`!ifndef`) blocks canonicalised, and runs of `name = value`
/// constants re-aligned. Operand spelling is preserved; the result assembles
/// byte-identical and is idempotent. See `decisions/formatter-canonical-style.md`.
///
/// # Errors
/// Returns an [`AsmError`] (with source line) on any parse failure.
pub fn format_acme(source: &str) -> Result<String, AsmError> {
    format_ast(&dialects::Acme, source)
}

/// Parse with a dialect's AST front-end and emit canonical source. Errors if the
/// dialect has no formatter yet (no AST front-end).
fn format_ast(dialect: &dyn dialect::Dialect, source: &str) -> Result<String, AsmError> {
    match dialect.parse_ast(source)? {
        Some(program) => Ok(ast::emit(&program, dialect.equ_label_colon())),
        None => Err(AsmError::new(0, "no formatter for this dialect yet")),
    }
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
        assert_eq!(a.origin, Some(0x0200));
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
            assemble_vasm("\tsubi.b #16,d0\n").expect("subi").bytes,
            vec![0x04, 0x00, 0x00, 0x10]
        );
        assert_eq!(
            assemble_vasm("\taddi.w #100,d2\n").expect("addi").bytes,
            vec![0x06, 0x42, 0x00, 0x64]
        );
        // `cmp #imm,<memory>` aliases to cmpi (vasm uses the <ea>,Dn form only
        // for a data-register destination), so the two assemble identically.
        assert_eq!(
            assemble_vasm("\tcmp.w #1,(a0)\n").expect("cmp alias").bytes,
            assemble_vasm("\tcmpi.w #1,(a0)\n").expect("cmpi").bytes,
        );
    }

    #[test]
    fn vasm_out_of_range_ccr_sr_immediate_warns_not_errors() {
        // vasm warns (2037) but still assembles an out-of-range immediate to
        // CCR (byte) / SR (word); asm198x mirrors that — same bytes, plus a
        // non-fatal warning. In-range immediates warn about nothing.
        let r = assemble_vasm_warned("\tandi #$1234,ccr\n").expect("ccr");
        assert_eq!(r.bytes, vec![0x02, 0x3C, 0x12, 0x34]); // byte-identical to vasm
        assert_eq!(r.warnings.len(), 1);
        assert_eq!(r.warnings[0].line, 1);
        assert!(r.warnings[0].message.contains("out of range"));

        let r = assemble_vasm_warned("\tandi #$12345,sr\n").expect("sr");
        assert_eq!(r.bytes, vec![0x02, 0x7C, 0x23, 0x45]);
        assert_eq!(r.warnings.len(), 1);

        // In range: CCR byte ($FF) and SR word ($FFFF) raise no warning.
        let r = assemble_vasm_warned("\tandi #$ff,ccr\n\tandi #$ffff,sr\n").expect("ok");
        assert!(r.warnings.is_empty());
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
            let r = assemble_vasm_warned(src).expect(src);
            assert_eq!(r.bytes, *want, "bytes for {src:?}");
            assert_eq!(r.warnings.len(), 1, "one warning for {src:?}");
        }
        // In-range forms of the same instructions raise no warning.
        let r = assemble_vasm_warned("\tmoveq #5,d0\n\taddq.w #3,d0\n\ttrap #7\n").expect("ok");
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn vasm_pc_relative_round_trips() {
        // `move.w $10(pc),d0` at origin 0: disassembly renders the resolved
        // target, which re-assembles to the same bytes (displacement = target −
        // PC). The disassembler<->assembler PC-relative contract.
        let bytes = vec![0x30, 0x3A, 0x00, 0x0E];
        let text = listing_68000(&bytes, 0);
        assert_eq!(assemble_vasm(&text).expect("reassemble").bytes, bytes);
    }
}
