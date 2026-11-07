# Preserve exact embedded and generated text on Windows

## Root cause and reassessment

Candidate 1077 at 2445aab4 passed the prior alias, Debug198x sidecar, SAVE path
and killed-process corrections on Windows. Its later explanations test failed
at `crates/asm198x/tests/explanations.rs:19`: a diagnostic embedded from a CRLF
checkout does not begin with the required LF-terminated heading. The macro in
`crates/asm198x-ir/src/diagnostic.rs:41` embeds diagnostic Markdown bytes.
The test also compares the generated book page and CLI output byte for byte.

Paused implementation and completed a read-only inventory before another fix.
The parent approved the resulting 61-file scope. No runtime normalization,
blanket Markdown attributes, changed fixtures, skipped tests or relaxed gates.

## Consumers and exact scope

- Four embedded diagnostic sources: `diagnostic.rs:41` and explanation headings.
- Four book diagnostic pages: `tests/explanations.rs:23` exact embedded equality.
- 43 whole generated instruction/dialect/index/ledger pages:
  `xtask/src/docs.rs:110-145` renders and compares their complete text.
- Eight marked-block pages: `xtask/src/docs.rs:80-85` regenerates blocks and
  compares the rewritten text with the checked-in text.
- Two generated JSON outputs: `xtask/src/main.rs:198,219` compares rendered nav
  and search strings with `docs/book/nav.json` and `docs/book/search.json`.

Exact 61-file inventory:

- `crates/asm198x-ir/diagnostics/assembly-error.md`
- `crates/asm198x-ir/diagnostics/branch-out-of-range.md`
- `crates/asm198x-ir/diagnostics/cycle-budget-exceeded.md`
- `crates/asm198x-ir/diagnostics/unsupported-feature.md`
- `docs/book/src/reference/diagnostics/assembly-error.md`
- `docs/book/src/reference/diagnostics/branch-out-of-range.md`
- `docs/book/src/reference/diagnostics/cycle-budget-exceeded.md`
- `docs/book/src/reference/diagnostics/unsupported-feature.md`
- `docs/book/src/ledger.md`
- `docs/book/src/reference/dialects/1802.md`
- `docs/book/src/reference/dialects/2650.md`
- `docs/book/src/reference/dialects/65816.md`
- `docs/book/src/reference/dialects/6800.md`
- `docs/book/src/reference/dialects/8048.md`
- `docs/book/src/reference/dialects/8080.md`
- `docs/book/src/reference/dialects/acme.md`
- `docs/book/src/reference/dialects/ca65.md`
- `docs/book/src/reference/dialects/cp1610.md`
- `docs/book/src/reference/dialects/f8.md`
- `docs/book/src/reference/dialects/huc6280.md`
- `docs/book/src/reference/dialects/lwasm.md`
- `docs/book/src/reference/dialects/pasmo.md`
- `docs/book/src/reference/dialects/pdp11.md`
- `docs/book/src/reference/dialects/rgbasm.md`
- `docs/book/src/reference/dialects/scmp.md`
- `docs/book/src/reference/dialects/sjasmplus.md`
- `docs/book/src/reference/dialects/tms7000.md`
- `docs/book/src/reference/dialects/tms9900.md`
- `docs/book/src/reference/dialects/vasm.md`
- `docs/book/src/reference/dialects/z8000.md`
- `docs/book/src/reference/instructions.md`
- `docs/book/src/reference/instructions/cdp1802.md`
- `docs/book/src/reference/instructions/cp1610.md`
- `docs/book/src/reference/instructions/f8.md`
- `docs/book/src/reference/instructions/huc6280.md`
- `docs/book/src/reference/instructions/i8048.md`
- `docs/book/src/reference/instructions/i8080.md`
- `docs/book/src/reference/instructions/m6800.md`
- `docs/book/src/reference/instructions/m68k.md`
- `docs/book/src/reference/instructions/mos6502.md`
- `docs/book/src/reference/instructions/mos65816.md`
- `docs/book/src/reference/instructions/mos6809.md`
- `docs/book/src/reference/instructions/pdp11.md`
- `docs/book/src/reference/instructions/s2650.md`
- `docs/book/src/reference/instructions/scmp.md`
- `docs/book/src/reference/instructions/sm83.md`
- `docs/book/src/reference/instructions/tms7000.md`
- `docs/book/src/reference/instructions/tms9900.md`
- `docs/book/src/reference/instructions/z80.md`
- `docs/book/src/reference/instructions/z8000.md`
- `docs/book/src/reference/instructions/z80n.md`
- `docs/book/src/SUMMARY.md`
- `docs/book/src/compare.md`
- `docs/book/src/divergences.md`
- `docs/book/src/guide/multi-file.md`
- `docs/book/src/migrate.md`
- `docs/book/src/reference/dialects.md`
- `docs/book/src/reference/ecosystem.md`
- `docs/book/src/why.md`
- `docs/book/nav.json`
- `docs/book/search.json`

## Plan and verification

1. Preserve the failing simulated Windows checkout evidence: a disposable index
   at exact candidate 2445aab4 with its own attributes and `core.autocrlf=true`
   converts all 61 files to CRLF. All four embedded heading checks fail.
2. Add only the matching scoped rules in `.gitattributes`; repeat the checkout
   against task HEAD with the proposed attributes and require all 61 files to
   equal their Git blobs byte for byte. Keep controls: nine protected Debug198x
   sidecars, the protected Lua source and Cargo.lock stay exact; parsed JSON and
   line-oriented CLI/pasmonext/recovery text retain semantics without new rules.
3. Run all explanation tests and the exact generated-documentation checks via
   `cargo xtask docs --check`, plus diff validation. The audit found one
   production include_str site (diagnostics) and no include_bytes sites in
   crates/xtask; the remaining embedded fixtures are parsed JSON or the controls.
4. Push a checked PR and send its exact diff for review before merging. Require
   all current PR checks and the scheduled native archive qualifications, then
   refresh the release and require all complete current native checks.
5. Verify the remote retains the commit and remove the task worktree/cache.

Evidence: https://github.com/asm198x/asm198x/actions/runs/42394735115/job/128469728503

## Results

Before the rules, all 61 scoped files converted to CRLF and all four embedded
heading invariants failed. After the rules, the identical simulated Windows
checkout preserved every scoped file byte for byte against its Git blob, and
all four embedded headings passed. Existing protected bytes remained exact;
all four parsed/line-oriented exclusions still converted as expected and kept
their JSON values or logical lines. Disposable indexes/checkouts were removed.

All five explanation tests passed. `cargo xtask docs --check` reported 10
current generated blocks across 65 pages and 47 current generated pages; the
nav lists 64 present pages. `git diff --check` passed. The attribute-only change
does not alter any Markdown, JSON, source fixture, Rust code or assertion.
