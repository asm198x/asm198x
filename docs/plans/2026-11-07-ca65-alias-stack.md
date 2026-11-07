# Avoid a stack overflow while resolving ca65 aliases

Windows release CI crashes on the existing `long_forward_chain` fixture in
`crates/asm198x/tests/fixtures/ca65-deferred-aliases.json`: 501 linked names,
502 source lines, native output `34`. The earlier diagnostic-path test now
passes. Do not shrink the fixture or increase the process stack.

1. Build the CLI in this isolated worktree, extract the existing fixture and
   reproduce with a constrained stack. Temporarily instrument
   `crates/asm198x/src/dialects/ca65_walk.rs` to establish the overflowing
   traversal; remove instrumentation before committing.
2. Change only the confirmed alias traversal in `ca65_walk.rs` to use explicit
   work storage, preserving cycle detection, source-time values, byte widths
   and native output. Existing failure modes to preserve: true cycles must
   reject, unresolved aliases must reject where used, aliases must retain
   source-time width and short-circuit semantics, and failures must preserve
   previously written output files.
3. Add a constrained-stack regression in
   `crates/asm198x/tests/ca65_deferred_aliases.rs`, using the existing fixture
   and a subprocess so an overflow cannot abort the entire suite. Establish
   red on the old implementation and green on the fix. Exercise cycles and
   the existing native fixture suite, then relevant ca65 suites, formatting
   and Clippy. Compare the fixture directly with installed ca65/ld65 V2.19.
4. Push a checked PR. Land only after all current checks and native candidate
   qualification pass; verify Windows on the refreshed release candidate.

No new dependencies or unrelated cleanup. Stop and reassess after two or
three failed fix attempts.

## Evidence

- The unmodified CLI aborts with stack overflow under a 1 MiB process-stack
  limit. Temporary depth-100 instrumentation traces `Walk::known` →
  `FullNames::name` → `plain` → `Expr::eval_with` → `lower::value` →
  `Walk::known`. The instrumentation is removed.
- The small-stack regression fails against the old `ca65_walk.rs` with child
  exit signal 6 and `has overflowed its stack`; it passes against the fix.
  It also checks expression aliases and rejection of a 501-name cycle.
- Installed ca65 and ld65 V2.19 independently emit `34` for both the original
  fixture and the equivalent chain whose definitions add zero. The corrected
  CLI emits the same byte under the original 1 MiB limit.
- Final local checks pass: all 10 deferred-alias tests (including the large
  source-time/external-binding fixture collections), 3 absolute-debug tests,
  7 configuration tests and 6 cycle-budget tests, plus 82 ca65 library tests.
  Three existing opt-in native tests remain ignored by the default command;
  the original and expression-chain native comparisons above were run directly.
  Formatting and scoped library/test Clippy pass.
