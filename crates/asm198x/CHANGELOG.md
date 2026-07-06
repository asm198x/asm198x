# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.8](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.7...asm198x-v0.0.8) - 2026-07-06

### Added

- *(debug)* emit the debug record from the vasm hunk path (U5)
- *(debug)* emit the debug record from the ca65 NES path (U4)
- *(debug)* emit the .debug198x sidecar, --sym, and --listing (U3)
- *(contract)* point diagnostics at the operand column (U3)
- version the core contract + record its freeze governance (U5)
- add --message-format=json to the CLI (core-contract U4)
- route NES ca65 (assemble+link) through the semantic AST
- route vasm (68000) assembly through the semantic AST
- give vasm (68000) an AST front-end for the --fmt formatter
- route Z8000 assembly through the semantic AST
- route CP1610 assembly through the semantic AST
- route TMS9900 assembly through the semantic AST
- route PDP-11 assembly through the semantic AST
- route ca65 HuC6280 assembly through the semantic AST
- route ca65 65816 assembly through the semantic AST
- *(tms7000)* route the TMS7000 dialect through the AST (0b straggler)
- *(2650)* route the Signetics 2650 dialect through the AST (0b straggler)
- *(f8)* route the Fairchild F8 dialect through the AST (0b straggler)
- *(8048)* route the MCS-48 dialect through the AST (0b straggler)
- *(contract)* rustc-shaped diagnostics on one shared span (U2)
- *(contract)* unify assembly output into one AssemblyResult (U1)
- *(ast)* idea 4 — ACME assembles by evaluating the conditional AST
- *(ast)* ACME/6502 formatter — canonical reflow with conditional blocks
- *(ast)* promote the conditional-block representation into the shared AST (idea 4)
- *(ast)* U6 — migrate the 6809 onto the AST (first computed-operand CPU)
- *(ast)* U6 — migrate rgbasm (Game Boy SM83) onto the AST
- *(ast)* U6 — migrate the National SC/MP onto the AST (fixed-slot)
- *(ast)* U6 — migrate the RCA CDP1802 onto the AST (fixed-slot)
- *(ast)* U6 — migrate the Motorola 6800 onto the AST (fixed-slot)
- *(ast)* U6 — migrate the Intel 8080 onto the AST (first fixed-slot CPU)
- *(ast)* U6 foundation — total lowering, retire the U1 spike
- *(ast)* U5 — asm198x fmt, the AST emit proof (AE7)
- *(ast)* U4 — carry Z80 comments as AST trivia
- *(ast)* U3 — route the Z80 front-end through the semantic AST
- *(ast)* U2 — the source-preserving semantic AST types
- *(dbg198x)* capture debug info in the engine (U2)

### Fixed

- *(review)* fmt round-trip bugs + restore the clippy gate

### Other

- *(debug)* the format decision record + CP1610 fixture (U7 — plan complete)
- *(debug)* the conformance fixture corpus (U6)
- rename the dbg198x crate to debug198x
- *(ast)* extract the shared conditional evaluator (CondEval)
- *(ast)* drop unused import in the U1 spike
- *(ast)* U1 validation spike — the neutral-AST gate

## [0.0.7](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.6...asm198x-v0.0.7) - 2026-07-03

### Added

- *(cp1610)* add SDBD double-byte immediate — completes the CPU (increment 6) ([#58](https://github.com/asm198x/asm198x/pull/58))
- *(cp1610)* add JUMP/JSR and word-addressing (increment 5) ([#57](https://github.com/asm198x/asm198x/pull/57))
- *(cp1610)* add memory / immediate addressing modes (increment 4) ([#56](https://github.com/asm198x/asm198x/pull/56))
- *(cp1610)* add relative branch group (increment 3) ([#54](https://github.com/asm198x/asm198x/pull/54))
- *(cp1610)* add shift / rotate group (increment 2) ([#53](https://github.com/asm198x/asm198x/pull/53))
- *(cp1610)* add GI CP1610 register/implied groups (increment 1) ([#52](https://github.com/asm198x/asm198x/pull/52))
- *(z8000)* add segmented Z8001 target (increment 12) ([#51](https://github.com/asm198x/asm198x/pull/51))
- *(z8000)* cleanup — TCC/LDK/RLDB/RRDB/LDR (complete Z8002 ISA)
- *(z8000)* increment 11 — CPU control / status group
- *(z8000)* increment 10 — privileged I/O group
- *(z8000)* increment 9 — block/string repeat group
- *(z8000)* increment 8 — multiply/divide (MULT/MULTL/DIV/DIVL)
- *(z8000)* increment 7 — bit ops (BIT/SET/RES, static and dynamic)
- *(z8000)* increment 6 — shifts/rotates/sign-extends
- *(z8000)* increment 5 — stack ops (PUSH/POP/PUSHL/POPL)
- *(z8000)* increment 4 — single-operand ALU (CLR/COM/NEG/TEST/TSET/INC/DEC)
- *(z8000)* increment 3 — program control (JP/CALL/JR/RET/DJNZ/CALR)
- *(z8000)* increment 2 — long ops, exchange, load address
- *(z8000)* increment 1 — the dyadic arithmetic/logic/load family
- add TI TMS9900 — Wave C, the TI-99/4A CPU
- add DEC PDP-11 — Wave B, the family's first 16-bit CPU

## [0.0.6](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.5...asm198x-v0.0.6) - 2026-07-02

### Added

- add TI TMS7000 — Wave B, the family's largest single CPU
- *(2650)* range-check relative/page-zero/absolute operands like asl
- add Signetics 2650 — Wave B, four addressing modes via the seam
- add ROM-less MCS-48 kin (8035/8039/8040) as an 8048 variant
- add Fairchild F8 (3850) — Wave B, offset-byte-relative branches
- add National SC/MP (INS8060) — Wave B, pointer+displacement addressing
- add Intel 8048 (MCS-48) — first Wave-B CPU, three tools one chip
- add RCA CDP1802 (COSMAC) — ninth CPU, zero engine changes
- add the Motorola 6800 (roadmap Wave A)
- add the Intel 8080 (Wave A of the CPU-coverage roadmap)
- *(asm)* accept rgbasm `@` current-PC symbol (#8 follow-up)
- *(asm)* rgbasm (Game Boy SM83) assemble dialect ([#8](https://github.com/asm198x/asm198x/pull/8))
- *(isa,disasm)* add the SM83 (Game Boy) spec + disassembler ([#8](https://github.com/asm198x/asm198x/pull/8))
- *(asm)* HuC6280 assembler + disassembler dialect (#9 phase 3)
- *(z80)* truncate out-of-range byte immediates; warning channel in the engine
- *(vasm)* warn (not error) on out-of-range immediates, matching vasm
- *(acme)* add the !set reassignable variable
- *(acme)* add the !align directive
- *(c64)* emit .prg output (--prg)
- *(spectrum)* emit 48K .sna snapshots (--sna)
- *(ca65-816)* add .dword/.dbyt/.asciiz; mark #26 differential gaps closed
- *(asm)* non-fatal warning channel; warn on out-of-range CCR/SR immediate
- *(ca65)* support anonymous labels (: / :- / :+)
- *(acme)* accept the !pet and !zone directives
- *(sjasmplus)* accept the byte directive (a db alias)
- *(lwasm)* add fill, zmb, and fqb directives
- *(ca65)* add .dword, .dbyt, and .asciiz directives
- *(isa)* add 6809 andcc/orcc/cmpu/cmps/swi2/swi3 and 65816 rtl

### Fixed

- *(ci)* clear the clippy errors breaking the Clippy job
- *(ca65)* clearer error for a segment outside the NES config
- *(acme)* correct operator precedence; add ^ power and XOR/EOR keyword
- *(acme)* require an explicit origin before code or data
- *(z80)* fold a constant-expression ds/defs count
- *(vasm)* assemble eor/and/or with an immediate operand
- *(vasm)* parse absolute-address size suffixes .w/.l
- *(z80)* accept radix number formats (0x, h/b/o/q suffix, # prefix)
- *(z80n)* accept mul operands (mul d,e / mul de)
- *(vasm)* accept adda/suba/cmpa mnemonics
- *(vasm)* parse new-style parenthesised 68k effective addresses
- *(dialects)* parse bitwise & shift operators in expressions
- *(z80n)* encode PUSH nn immediate big-endian

### Other

- *(differential)* note the ledger is gap-free, silence dead `gap`
- *(differential)* cover the Z80N extension ISA vs sjasmplus
- add source-direction differential audit (reference accepts, we reject)

## [0.0.5](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.4...asm198x-v0.0.5) - 2026-06-04

### Added

- *(68000)* add MOVEP — base-68000 ISA now complete
- *(68000)* add CCR/SR/USP moves and immediate-to-CCR/SR
- *(68000)* add TRAP, MOVEA, and EXG
- *(68000)* add ADDX/SUBX/ABCD/SBCD/CMPM (extended + BCD arithmetic)

### Other

- apply cargo fmt
- *(conformance)* extend the differential fuzzer to 6809 and 68000

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
