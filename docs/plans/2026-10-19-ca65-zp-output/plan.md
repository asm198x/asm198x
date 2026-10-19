# Match zero-page file placement (#801)

Native ca65/ld65 omits initialized bytes from a configured type=zp segment even
when its memory area names the output file. Asm198x currently writes them. The
control in #801 emits 09 natively and 0709 here. Reproduce before changing the
bounded layout or emitter.

1. Re-consult syntax, shared sections, native AST, I/O and diagnostics decisions.
   Inspect `ca65_layout.rs`, `ca65/link.rs`, `ca65/mod.rs` and source-time mode
   selection in `ca65/project.rs`/`ca65/deferred.rs`. Retain bounded native
   source/config/process outcomes for zp/bss/ro/rw, shared versus separate file
   areas, fills/reservations, initialized data, expressions/refusals, labels,
   physical offsets and the PC. Compare an explicit preceding executable.
2. Write the concrete design from native facts and the existing layout seam.
   Keep memory occupancy, address binding, validation and file output separate.
   In particular, omitted output must not bypass expression/range validation.
   Do not add a parallel linker or a public schema/dependency change.
3. Implement the smallest measured correction in those existing private paths.
   Add `crates/asm198x/tests/ca65_zp_output.rs`, a retained fixture, native byte/
   status and formatter/debug/area invariants, and included refusal preservation.
   Add mandatory replay/omission coverage to the existing package scripts.
4. Verify isolated Rust/Lua, Node, Clippy/fmt/Ruff/docs, actual archive and both
   corruption controls, including the actual preceding archive's new-cohort
   failure. Commit/push verified work and retain clean unchanged 4510 replay.
5. Inspect all exact-head workflows and five actual package reports/ten controls,
   squash through a conventional PR after the parent, verify retention/closure,
   clean finished task workspaces/caches and continue the next measured bug.

The user asked for continuous audit/fix work and delegated routine decisions.
The selected slice remains inside existing native layout/emission seams. Any
new dependency, public shape or architecture would require a separate decision.
