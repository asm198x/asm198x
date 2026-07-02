# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.6](https://github.com/asm198x/asm198x/compare/isa-v0.0.5...isa-v0.0.6) - 2026-07-02

### Added

- add TI TMS7000 — Wave B, the family's largest single CPU
- *(2650)* range-check relative/page-zero/absolute operands like asl
- add Signetics 2650 — Wave B, four addressing modes via the seam
- add Fairchild F8 (3850) — Wave B, offset-byte-relative branches
- add National SC/MP (INS8060) — Wave B, pointer+displacement addressing
- add Intel 8048 (MCS-48) — first Wave-B CPU, three tools one chip
- add RCA CDP1802 (COSMAC) — ninth CPU, zero engine changes
- add the Motorola 6800 (roadmap Wave A)
- add the Intel 8080 (Wave A of the CPU-coverage roadmap)
- *(isa,disasm)* add the SM83 (Game Boy) spec + disassembler ([#8](https://github.com/asm198x/asm198x/pull/8))
- *(isa)* add HuC6280 exotic forms (Phase 2)
- *(isa)* add the HuC6280 fixed-slot instruction spec (#9 phase 1)
- *(isa)* add 6809 andcc/orcc/cmpu/cmps/swi2/swi3 and 65816 rtl
- *(68000)* add MOVEP — base-68000 ISA now complete
- *(68000)* add CCR/SR/USP moves and immediate-to-CCR/SR
- *(68000)* add TRAP, MOVEA, and EXG
- *(68000)* add ADDX/SUBX/ABCD/SBCD/CMPM (extended + BCD arithmetic)
- *(68000)* add STOP and LINK (reuse ImmWord)
- *(68000)* add memory shift/rotate forms (shift <ea> by one)
- *(68000)* add NEGX/NBCD/TAS/PEA/UNLK/CHK (slot-reusing)
- *(68000)* add the full Bcc/Scc/DBcc condition-code set
- *(68000)* add MULS/DIVS, shifts/rotates, BCHG/BCLR (mirror families)
- *(68000)* add control-flow instructions (JMP/JSR + returns)
- *(68000)* assemble and disassemble ORI/ANDI/EORI
- *(65816)* block moves, cop/wdm, bank-byte operator, 24-bit symbols
- *(65816)* native-mode core as a ca65 target extension of the 6502
- *(6809)* indexed addressing, register ops, fcc, and the disassembler
- *(6809)* add lwasm 6809 assembler over a computed-operand engine seam
- *(68000)* ADDI/SUBI/CMPI and the add#d16,An->lea optimization
- *(68000)* local-label scoping, ADDA/SUBA/CMPA, deferred ds/dcb counts
- *(68000)* shifts, bit ops, movem, and .s short branches
- *(68000)* add the regular instruction families
- *(68000)* field-based encoder foundation (vasm mot syntax)
- add Z80N (Spectrum Next) opcodes, gated by target not dialect
- complete the Z80 with the DD/FD (IX/IY) prefix group
- *(isa)* add the complete Z80 CB-prefixed group
- *(isa)* add the complete Z80 ED-prefixed group
- *(isa)* complete Z80 base page, opcodes 0x80-0xFF
- *(isa)* add Z80 base-page spec, opcodes 0x00-0x7F
- scaffold Asm198x workspace with a working 6502 assembler slice

### Fixed

- *(68000)* allow immediate EA on dynamic BTST Dn,#imm
- *(z80n)* accept mul operands (mul d,e / mul de)
- *(z80n)* encode PUSH nn immediate big-endian
- *(68000)* harden the disassembler/spec, enabling the conformance sweep

### Other

- *(isa)* note m68k and 6809 are now modelled
- release v0.0.5
- release v0.0.4
- rustfmt the workspace (unblocks the CI fmt check)
- format the workspace with rustfmt (1.95.0 toolchain)
- release v0.0.3
- release v0.0.2 ([#1](https://github.com/asm198x/asm198x/pull/1))

## [0.0.5](https://github.com/asm198x/asm198x/compare/isa-v0.0.4...isa-v0.0.5) - 2026-06-04

### Added

- *(68000)* add MOVEP — base-68000 ISA now complete
- *(68000)* add CCR/SR/USP moves and immediate-to-CCR/SR
- *(68000)* add TRAP, MOVEA, and EXG
- *(68000)* add ADDX/SUBX/ABCD/SBCD/CMPM (extended + BCD arithmetic)
- *(68000)* add STOP and LINK (reuse ImmWord)
- *(68000)* add memory shift/rotate forms (shift <ea> by one)
- *(68000)* add NEGX/NBCD/TAS/PEA/UNLK/CHK (slot-reusing)
- *(68000)* add the full Bcc/Scc/DBcc condition-code set
- *(68000)* add MULS/DIVS, shifts/rotates, BCHG/BCLR (mirror families)
- *(68000)* add control-flow instructions (JMP/JSR + returns)
- *(68000)* assemble and disassemble ORI/ANDI/EORI
- *(65816)* block moves, cop/wdm, bank-byte operator, 24-bit symbols
- *(65816)* native-mode core as a ca65 target extension of the 6502
- *(6809)* indexed addressing, register ops, fcc, and the disassembler
- *(6809)* add lwasm 6809 assembler over a computed-operand engine seam
- *(68000)* ADDI/SUBI/CMPI and the add#d16,An->lea optimization
- *(68000)* local-label scoping, ADDA/SUBA/CMPA, deferred ds/dcb counts
- *(68000)* shifts, bit ops, movem, and .s short branches
- *(68000)* add the regular instruction families
- *(68000)* field-based encoder foundation (vasm mot syntax)
- add Z80N (Spectrum Next) opcodes, gated by target not dialect
- complete the Z80 with the DD/FD (IX/IY) prefix group
- *(isa)* add the complete Z80 CB-prefixed group
- *(isa)* add the complete Z80 ED-prefixed group
- *(isa)* complete Z80 base page, opcodes 0x80-0xFF
- *(isa)* add Z80 base-page spec, opcodes 0x00-0x7F
- scaffold Asm198x workspace with a working 6502 assembler slice

### Fixed

- *(68000)* harden the disassembler/spec, enabling the conformance sweep

### Other

- release v0.0.4
- rustfmt the workspace (unblocks the CI fmt check)
- format the workspace with rustfmt (1.95.0 toolchain)
- release v0.0.3
- release v0.0.2 ([#1](https://github.com/asm198x/asm198x/pull/1))

## [0.0.4](https://github.com/asm198x/asm198x/compare/isa-v0.0.3...isa-v0.0.4) - 2026-06-03

### Added

- *(65816)* block moves, cop/wdm, bank-byte operator, 24-bit symbols
- *(65816)* native-mode core as a ca65 target extension of the 6502
- *(6809)* indexed addressing, register ops, fcc, and the disassembler
- *(6809)* add lwasm 6809 assembler over a computed-operand engine seam
- *(68000)* ADDI/SUBI/CMPI and the add#d16,An->lea optimization
- *(68000)* local-label scoping, ADDA/SUBA/CMPA, deferred ds/dcb counts
- *(68000)* shifts, bit ops, movem, and .s short branches
- *(68000)* add the regular instruction families
- *(68000)* field-based encoder foundation (vasm mot syntax)
- add Z80N (Spectrum Next) opcodes, gated by target not dialect
- complete the Z80 with the DD/FD (IX/IY) prefix group
- *(isa)* add the complete Z80 CB-prefixed group
- *(isa)* add the complete Z80 ED-prefixed group
- *(isa)* complete Z80 base page, opcodes 0x80-0xFF
- *(isa)* add Z80 base-page spec, opcodes 0x00-0x7F
- scaffold Asm198x workspace with a working 6502 assembler slice

### Fixed

- *(68000)* harden the disassembler/spec, enabling the conformance sweep

### Other

- rustfmt the workspace (unblocks the CI fmt check)
- format the workspace with rustfmt (1.95.0 toolchain)
- release v0.0.3
- release v0.0.2 ([#1](https://github.com/asm198x/asm198x/pull/1))

## [0.0.3] - 2026-06-02

### Other

- Lockstep version bump with the workspace (release-tooling fix in `asm198x`).

## [0.0.2](https://github.com/asm198x/asm198x/compare/isa-v0.0.1...isa-v0.0.2) - 2026-06-01

### Added

- Declarative 6502 instruction-set spec: every mnemonic's opcodes, operand
  layout, addressing modes, cycle counts, and flag effects as authored `const`
  data, plus the types that describe them. Dependency-free and standalone — the
  single source of truth the assembler consumes, and that Emu198x can later
  validate its hand-written decoders against.
