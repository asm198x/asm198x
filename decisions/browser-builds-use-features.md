# Decision: browser builds select features

**Status:** Active. Binding for Asm198x.

**Date:** 2026-09-10.

## The decision

Keep one native `asm198x` tool. Browser consumers may select CPU architectures
at compile time through `asm198x-web` features. The default browser package,
`@asm198x/web`, includes every dialect; an architecture package such as
`@asm198x/z80` builds the same library with fewer entry points exposed.

This adopts the feature-gated path already allowed by
[`packaging-and-cpu-roadmap.md`](packaging-and-cpu-roadmap.md). It does not
introduce runtime plugin loading, dynamic libraries, or an extension ABI, and
does not amend the single-binary premise in
[`why-not-llvm.md`](why-not-llvm.md).

## Evidence and trade-off

[Issue #495](https://github.com/asm198x/asm198x/issues/495) deferred this choice
until the browser work produced measurements. The web README's recorded
2026-09-04 baseline is about 503 KB gzipped for all architectures and 185 KB
for Z80, built at `opt-level = "z"` with wasm-opt. These are historical bundle
measurements, not fixed budgets or freshly measured release sizes.

There is a shipped consumer: Code198x's `AssembleAndRun.astro` imports
`@asm198x/z80` lazily, assembles Spectrum lesson source, and passes the result
to its Emu198x-backed runner. See the consumer evidence in
[#493](https://github.com/asm198x/asm198x/issues/493#issuecomment-5543449470).
The complete assembler is small enough to remain useful as one package; a
lesson embedding an emulator still benefits from downloading fewer bytes.

The web shell names only the selected assembler entry points, allowing the
linker to discard unused code. `dialects()` reports the selected build's
capabilities. Architecture packages share one release version and assembler
implementation, at the cost of building and distributing more artifacts.

No demonstrated consumer needs to install new CPU support into a running
assembler. A runtime plugin interface would add compatibility and deployment
obligations without improving this browser download case.

## Reassessment

Revisit the build selections when a consumer's measured download or build cost
justifies it. A runtime extension proposal must identify a need that a
compile-time selection cannot meet and explicitly reconsider the packaging
decision. Internal crate boundaries remain governed by that decision; this
browser choice does not require further crate splitting.
