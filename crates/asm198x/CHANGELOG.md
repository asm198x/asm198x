# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.4](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.3...asm198x-v0.0.4) - 2026-06-03

### Added

- *(65816)* spec-driven disassembler with m/x width tracking
- *(65816)* block moves, cop/wdm, bank-byte operator, 24-bit symbols
- *(65816)* native-mode core as a ca65 target extension of the 6502
- *(6809)* indexed addressing, register ops, fcc, and the disassembler
- *(6809)* add lwasm 6809 assembler over a computed-operand engine seam
- *(68000)* spec-driven disassembler
- *(68000)* bitwise/shift operators, indexed addressing, label(pc) — Stage 3 complete
- *(68000)* Amiga hunk-executable output (Stage 3, single-section)
- *(68000)* rewrite cmp #0,<ea> to tst <ea>
- *(68000)* drop zero d16(An) displacement to (An)
- *(68000)* convert add/sub #d16,An to lea for word size too
- *(68000)* ADDI/SUBI/CMPI and the add#d16,An->lea optimization
- *(68000)* Stage 2 optimizer — PC-relative, branch relaxation, addq/subq
- *(68000)* local-label scoping, ADDA/SUBA/CMPA, deferred ds/dcb counts
- *(68000)* shifts, bit ops, movem, and .s short branches
- *(68000)* add the regular instruction families
- *(68000)* field-based encoder foundation (vasm mot syntax)
- *(6502)* honor ACME hex-width sizing; full-binary disasm round-trip
- *(6502)* add spec-driven 6502 disassembler
- *(6502)* add ca65 dialect + bounded NES linker
- *(6502)* ACME text directives + constant-folded !fill
- *(6502)* ACME conditional assembly + value-based zero-page selection
- *(6502)* support ACME anonymous -/+ labels
- *(6502)* add ACME dialect front-end (foundation)
- *(z80)* add location counter and sjasmplus local-label scoping
- *(asm198x)* add the sjasmplus dialect over a shared Z80 syntax core
- add Z80N (Spectrum Next) opcodes, gated by target not dialect
- *(asm198x)* add a spec-driven Z80 disassembler
- complete the Z80 with the DD/FD (IX/IY) prefix group
- *(asm198x)* add vanilla pasmo as a first-class Z80 dialect
- *(asm198x)* resolve BIT/SET/RES bit numbers and defb strings in pasmonext
- *(asm198x)* expression arithmetic and IM operand resolution
- *(asm198x)* add the pasmo Z80 dialect front-end

### Fixed

- *(68000)* render PC-relative EA as a resolved target (closes the last gap)
- *(68000)* harden the disassembler/spec, enabling the conformance sweep
- *(68000)* correct branch relaxation fixpoint; complete Stage 2 flat-binary parity
- *(68000)* relax bare branches to short, not just explicit .s
- *(asm198x)* emit operands by their declared width

### Other

- rustfmt the workspace (unblocks the CI fmt check)
- *(conformance)* sweep-based audit for the non-form specs (6809)
- *(conformance)* spec-opcode audit + differential fuzzer vs the real tools
- collapse the four Expr evaluators into one shared core
- *(68000)* wire vasm byte-identity into the curriculum harness
- extract the disassembler into the dependency-free isa-disasm crate
- *(68000)* div_ceil and a Reloc type alias (clippy)
- rewrite README + correct disasm round-trip note; fix example
- format the workspace with rustfmt (1.95.0 toolchain)
- *(asm198x)* add opt-in curriculum byte-identity harness
- *(asm198x)* document flat-vs-linked split; refresh stale crate docs
- *(6502)* dedup shared acme/ca65 lexical helpers into the core
- *(6502)* extract shared dialects::mos6502 core
- *(6502)* retire generic placeholder, route 6502 to ACME
- *(asm198x)* name the Z80 dialect PasmoNext
- *(asm198x)* split engine, dialect, and spec into a three-way seam

## [0.0.3](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.2...asm198x-v0.0.3) - 2026-06-02

### Fixed

- Give the `isa` path dependency an explicit version requirement so
  `cargo package` succeeds. release-plz runs `cargo package` to compute the
  release diff; a path dependency without a version requirement fails it, which
  blocked release automation. Local builds still resolve `isa` via the path.

### Other

- Enable `git_only` so release-plz reads the previous version from the git tag
  rather than the (unused, `publish = false`) crates.io registry.

## [0.0.2](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.1...asm198x-v0.0.2) - 2026-06-01

### Added

- Two-pass 6502 assembler — a library plus the `asm198x` CLI — built on the
  `isa` instruction-set spec. This first slice covers the common addressing
  modes, labels, the `<`/`>` byte-select operators, and the `.org` / `.byte` /
  `.word` directives. The 6502 dialect is an early subset; ca65 compatibility
  (arithmetic expressions, the full directive set, segments, macros) is still to
  come — see `decisions/syntax-stance.md`.
