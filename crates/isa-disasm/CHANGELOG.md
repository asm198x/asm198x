# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
