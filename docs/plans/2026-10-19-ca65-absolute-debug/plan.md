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
