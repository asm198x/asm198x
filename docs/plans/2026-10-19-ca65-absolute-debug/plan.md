# Retain absolute ca65 label values in debug output (#814)

1. Re-consult the Debug198x/core contract, native AST, sections, layout and
   non-byte verification decisions. Inspect actual capture/section readers and
   existing section-relative Label semantics before choosing a representation.
   Retain native source/config/bytes and preceding debug mismatches for origins
   above/below the load base, mode-directive labels, aliases, scopes/cheap labels,
   multiple segments, includes/macros/repeats and field/name collisions.
2. Use the existing section/address contract if it can represent the values
   without a protocol change. Keep section IDs for configured segments, physical
   line/file coordinates and memory usage unchanged. Do not turn addresses into
   Const values or fabricate a hardware address space. Write the concrete design.
3. Correct the private capture in `ca65/mod.rs` and its existing section adapter
   only as required. Add `tests/ca65_absolute_debug.rs` and
   `tests/fixtures/ca65-absolute-debug.json`. Resolve each sidecar label through
   its section and compare with native emitted words; cover formatted reassembly,
   metadata-only sections, config ID gaps and five-output refusal preservation.
4. Add a mandatory package cohort and omission control in the existing scripts.
   Run isolated Rust/Lua, Clippy, fmt, Node, Ruff/docs and preceding/current real
   archives with both corruption controls. Push verified commits and retain a
   clean unchanged 4510 producer replay.
5. Wait for #816; inspect every exact-head workflow/check and five actual package
   reports/receipts plus ten controls. Squash, verify closure/retention, clean the
   finished workspace/cache and continue the next confirmed issue.

## Verified prerequisite

The 16-case native address audit exposes three cross-segment byte differences
under the default global absolute counter. Correct that separate #822 issue
before qualifying cross-segment metadata. Native segment.c GetPC and fragment
accounting use global AbsPC, independent of the active segment's linked base.
Do not introduce per-segment absolute mode as the default. The #814 logical
section design is approved; the source byte prerequisite remains separate.

## Concrete implementation files

- `crates/asm198x/src/dialects/ca65/mod.rs`: private passive span collector;
  labels use the prior mode PC, line/cycle records use the emitted PC, and all
  three use one span coordinate map. Reuse configured coordinates when equal;
  append IDs after the complete config range. Preserve CPU/space policy.
- `crates/asm198x/src/listing.rs`: group capture records by ID once, preserve
  original ordering, and use shared native cycle accounting for budgets.
- `crates/asm198x/src/cycles.rs`: shared physical source-segment accounting;
  retain each original symbol's section/offset when returning label totals.
  Keep unmapped/qualified scopes independent and never default missing mapping
  to zero. Identity must survive equal/aliased names and offsets.
- `crates/asm198x/src/listing/native.rs`: text and JSON use the same totals as
  budgets while showing each label's logical coordinates.
- `crates/asm198x/tests/ca65_absolute_debug.rs` and retained native fixture:
  compare labels with native ld65 symbol values and physical line ranges with
  retained native debug spans; exercise official Debug198x read/lookup, holes,
  included paths, formatter round trips, file-only metadata and routine budgets.
- `docs/book/src/reference/cli.md`: clarify source-segment straight-line totals
  across logical address changes; regenerate affected book data if changed.
- Existing two package scripts: one required metadata cohort and omission
  control, preceding red/current green, all five host reports and ten controls.

The byte prerequisite is now locally complete on #824, with all 63 native
controls and the unchanged 4510 replay retained. Merge qualification still
precedes final #814 merge.
