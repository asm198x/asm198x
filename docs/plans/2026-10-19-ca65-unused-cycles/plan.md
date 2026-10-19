# Preserve native unused alias validation (#800)

Native can accept an unused local circular definition if source-time address
width was known, while using that value still refuses. The deferred resolver
currently demands a numeric value for every immutable definition. This blocks
routing enum initializers through the same queue without introducing refusals.

1. Re-consult syntax/scoped-symbol/selected-reader decisions. Reproduce the
   distinction independently of enums. Probe known/unknown widths, source-time
   versus final outer declarations, direct/two-member cycles, dependency use,
   exports and numeric versus name-query uses. Retain all process evidence and
   actual predecessor status/bytes; bounded native timeouts are observations.
2. Verify the source explanation against native: SymDef studies width before
   assigning the expression; SymCheck restudies expressions whose width is still
   unknown. Byte emission studies used expressions. Write the concrete design
   from those observations, keeping undefined-name checks unconditional.
3. Reuse private Deferred source-time metadata and its iterative final graph in
   `ca65/deferred.rs`/`ca65/project.rs`/`ca65/mod.rs` as required. Preserve source
   width independently of value. Do not invent a numeric value for an unevaluated
   alias. Preserve undefined-name refusals and cycles that native validates.
   Shared AST/API/schema and dependencies remain unchanged.
4. Add native/formatter/debug and included refusal preservation coverage in
   `crates/asm198x/tests/ca65_unused_cycles.rs` and a retained fixture; add the
   mandatory package cohort/omission control to the existing scripts. Run the
   isolated complete Rust/Lua, Node, Clippy/fmt/Ruff/docs and actual archive
   controls; demonstrate the preceding archive fails the new cohort.
5. Commit/push verified work, retain clean unchanged 4510 replay, inspect all
   exact-head workflows and five actual package reports/ten controls, then
   squash through a conventional PR, verify closure/retention and clean up.
   Continue enum execution on the corrected queue afterwards.

The user delegated routine decisions and requested continuous audit/fix work.
This follows existing private expression and width seams, with no new public
shape or architectural pattern. No code changes precede the native evidence.

The concrete private design is in design.md. The selected scope closes #800
and #802; #801 linker output and #803 scoped export lookup stay explicit
follow-ups. Native crash observations are excluded from semantic expectations.
The final selected count is 164, with three include/formatter/debug tests and
a mandatory 353-command package replay.
