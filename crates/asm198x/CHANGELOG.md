# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.2](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.1...asm198x-v0.0.2) - 2026-06-01

### Added

- Two-pass 6502 assembler — a library plus the `asm198x` CLI — built on the
  `isa` instruction-set spec. This first slice covers the common addressing
  modes, labels, the `<`/`>` byte-select operators, and the `.org` / `.byte` /
  `.word` directives. The 6502 dialect is an early subset; ca65 compatibility
  (arithmetic expressions, the full directive set, segments, macros) is still to
  come — see `decisions/syntax-stance.md`.
