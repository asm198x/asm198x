# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.7](https://github.com/asm198x/asm198x/compare/isa-disasm-v0.0.6...isa-disasm-v0.0.7) - 2026-07-03

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
- add TI TMS7000 — Wave B, the family's largest single CPU
- add Signetics 2650 — Wave B, four addressing modes via the seam
- add Fairchild F8 (3850) — Wave B, offset-byte-relative branches
- add National SC/MP (INS8060) — Wave B, pointer+displacement addressing
- add Intel 8048 (MCS-48) — first Wave-B CPU, three tools one chip
- add RCA CDP1802 (COSMAC) — ninth CPU, zero engine changes
- add the Motorola 6800 (roadmap Wave A)
- add the Intel 8080 (Wave A of the CPU-coverage roadmap)
- *(asm)* rgbasm (Game Boy SM83) assemble dialect ([#8](https://github.com/asm198x/asm198x/pull/8))
- *(isa,disasm)* add the SM83 (Game Boy) spec + disassembler ([#8](https://github.com/asm198x/asm198x/pull/8))
- *(asm)* HuC6280 assembler + disassembler dialect (#9 phase 3)
- *(68000)* add MOVEP — base-68000 ISA now complete
- *(68000)* add CCR/SR/USP moves and immediate-to-CCR/SR
- *(68000)* add TRAP, MOVEA, and EXG
- *(68000)* add ADDX/SUBX/ABCD/SBCD/CMPM (extended + BCD arithmetic)
- *(68000)* add NEGX/NBCD/TAS/PEA/UNLK/CHK (slot-reusing)
- *(68000)* add the full Bcc/Scc/DBcc condition-code set
- *(68000)* add MULS/DIVS, shifts/rotates, BCHG/BCLR (mirror families)
- *(68000)* add control-flow instructions (JMP/JSR + returns)
- *(isa-disasm)* add decode_one_6502/6809 for single-instruction callback decode
- *(68000)* assemble and disassemble ORI/ANDI/EORI
- *(65816)* spec-driven disassembler with m/x width tracking
- *(6809)* indexed addressing, register ops, fcc, and the disassembler

### Fixed

- *(68000)* allow immediate EA on dynamic BTST Dn,#imm
- *(z80n)* encode PUSH nn immediate big-endian
- *(68000)* reject byte immediates with a non-zero extension high byte
- *(68000)* render PC-relative EA as a resolved target (closes the last gap)
- *(68000)* harden the disassembler/spec, enabling the conformance sweep

### Other

- release v0.0.6
- release v0.0.5
- apply cargo fmt
- release v0.0.4
- rustfmt the workspace (unblocks the CI fmt check)
- *(conformance)* sweep-based audit for the non-form specs (6809)
- *(conformance)* spec-opcode audit + differential fuzzer vs the real tools
- extract the disassembler into the dependency-free isa-disasm crate

## [0.0.6](https://github.com/asm198x/asm198x/compare/isa-disasm-v0.0.5...isa-disasm-v0.0.6) - 2026-07-02

### Added

- add TI TMS7000 — Wave B, the family's largest single CPU
- add Signetics 2650 — Wave B, four addressing modes via the seam
- add Fairchild F8 (3850) — Wave B, offset-byte-relative branches
- add National SC/MP (INS8060) — Wave B, pointer+displacement addressing
- add Intel 8048 (MCS-48) — first Wave-B CPU, three tools one chip
- add RCA CDP1802 (COSMAC) — ninth CPU, zero engine changes
- add the Motorola 6800 (roadmap Wave A)
- add the Intel 8080 (Wave A of the CPU-coverage roadmap)
- *(asm)* rgbasm (Game Boy SM83) assemble dialect ([#8](https://github.com/asm198x/asm198x/pull/8))
- *(isa,disasm)* add the SM83 (Game Boy) spec + disassembler ([#8](https://github.com/asm198x/asm198x/pull/8))
- *(asm)* HuC6280 assembler + disassembler dialect (#9 phase 3)
- *(68000)* add MOVEP — base-68000 ISA now complete
- *(68000)* add CCR/SR/USP moves and immediate-to-CCR/SR
- *(68000)* add TRAP, MOVEA, and EXG
- *(68000)* add ADDX/SUBX/ABCD/SBCD/CMPM (extended + BCD arithmetic)
- *(68000)* add NEGX/NBCD/TAS/PEA/UNLK/CHK (slot-reusing)
- *(68000)* add the full Bcc/Scc/DBcc condition-code set
- *(68000)* add MULS/DIVS, shifts/rotates, BCHG/BCLR (mirror families)
- *(68000)* add control-flow instructions (JMP/JSR + returns)
- *(isa-disasm)* add decode_one_6502/6809 for single-instruction callback decode
- *(68000)* assemble and disassemble ORI/ANDI/EORI
- *(65816)* spec-driven disassembler with m/x width tracking
- *(6809)* indexed addressing, register ops, fcc, and the disassembler

### Fixed

- *(68000)* allow immediate EA on dynamic BTST Dn,#imm
- *(z80n)* encode PUSH nn immediate big-endian
- *(68000)* reject byte immediates with a non-zero extension high byte
- *(68000)* render PC-relative EA as a resolved target (closes the last gap)
- *(68000)* harden the disassembler/spec, enabling the conformance sweep

### Other

- release v0.0.5
- apply cargo fmt
- release v0.0.4
- rustfmt the workspace (unblocks the CI fmt check)
- *(conformance)* sweep-based audit for the non-form specs (6809)
- *(conformance)* spec-opcode audit + differential fuzzer vs the real tools
- extract the disassembler into the dependency-free isa-disasm crate

## [0.0.5](https://github.com/asm198x/asm198x/compare/isa-disasm-v0.0.4...isa-disasm-v0.0.5) - 2026-06-04

### Added

- *(68000)* add MOVEP — base-68000 ISA now complete
- *(68000)* add CCR/SR/USP moves and immediate-to-CCR/SR
- *(68000)* add TRAP, MOVEA, and EXG
- *(68000)* add ADDX/SUBX/ABCD/SBCD/CMPM (extended + BCD arithmetic)
- *(68000)* add NEGX/NBCD/TAS/PEA/UNLK/CHK (slot-reusing)
- *(68000)* add the full Bcc/Scc/DBcc condition-code set
- *(68000)* add MULS/DIVS, shifts/rotates, BCHG/BCLR (mirror families)
- *(68000)* add control-flow instructions (JMP/JSR + returns)
- *(isa-disasm)* add decode_one_6502/6809 for single-instruction callback decode
- *(68000)* assemble and disassemble ORI/ANDI/EORI
- *(65816)* spec-driven disassembler with m/x width tracking
- *(6809)* indexed addressing, register ops, fcc, and the disassembler

### Fixed

- *(68000)* reject byte immediates with a non-zero extension high byte
- *(68000)* render PC-relative EA as a resolved target (closes the last gap)
- *(68000)* harden the disassembler/spec, enabling the conformance sweep

### Other

- apply cargo fmt
- release v0.0.4
- rustfmt the workspace (unblocks the CI fmt check)
- *(conformance)* sweep-based audit for the non-form specs (6809)
- *(conformance)* spec-opcode audit + differential fuzzer vs the real tools
- extract the disassembler into the dependency-free isa-disasm crate

## [0.0.4](https://github.com/asm198x/asm198x/compare/isa-disasm-v0.0.3...isa-disasm-v0.0.4) - 2026-06-03

### Added

- *(65816)* spec-driven disassembler with m/x width tracking
- *(6809)* indexed addressing, register ops, fcc, and the disassembler

### Fixed

- *(68000)* render PC-relative EA as a resolved target (closes the last gap)
- *(68000)* harden the disassembler/spec, enabling the conformance sweep

### Other

- rustfmt the workspace (unblocks the CI fmt check)
- *(conformance)* sweep-based audit for the non-form specs (6809)
- *(conformance)* spec-opcode audit + differential fuzzer vs the real tools
- extract the disassembler into the dependency-free isa-disasm crate
