# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
