# Correct the published package description (#675)

The authored description in `crates/asm198x/Cargo.toml` still says “6502 today;
more to follow.” Cargo metadata and the generated Homebrew formula repeat it.
The CLI already implements multiple CPU families, so remove the obsolete limit
without introducing a CPU count or a roadmap support claim.

1. Record the current `cargo metadata` description and generate the normal
   cargo-dist global artifacts to confirm the formula's description source.
2. Change only the package description to “Assemblers and disassemblers for
   retro CPUs in a single binary”. No runtime code, dependency or public API
   changes; no test pinning a prose string.
3. Inspect fresh Cargo metadata and generated formula output, run generated docs
   and release-workflow checks, then push a PR. Merge only after all triggered
   checks and workflows complete acceptably at the exact tested head.
4. Continue #641 with a bounded public-0.0.66 disassembly argument/error/output
   audit. Read the CLI contract and relevant decisions first; retain reproducible
   observations and file only verified findings, keeping deliberate behavior and
   existing issues separate.
5. Verify remote retention and clean task worktrees, scratch and build caches.
   A prose metadata fix is not substantial enough to cut another release by
   itself under `release-cadence.md`; the corrected description ships through the
   normal next release, with the public 0.0.66 record preserved.
