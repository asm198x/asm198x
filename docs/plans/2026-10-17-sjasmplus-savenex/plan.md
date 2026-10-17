# SAVENEX V1.2 implementation plan

1. [done] Inspect decisions, artifact/AST/device seams and pinned native writer.
   Capture 53 bounded native cases with every image retained and independently
   verify image hashes/lengths. Record the concrete design in design.md.
2. [pending agreement] Agree the bounded V1.2 scope and public Nex enum extension.
3. [pending] Add primary writer provenance/layout facts in the reference library
   before implementing layout bytes. Extend probe.py and retained native facts
   for the edge cases listed in design.md.
4. [pending] Add red differential, formatter, in-memory I/O and CLI preservation
   tests in crates/asm198x/tests/sjasmplus_savenex.rs. Reproduce current explicit
   UnsupportedFeature failure; compare full images, not just header fields.
5. [pending] Add typed internal command lowering in dialects/sjasmplus.rs,
   operation/AST expression handling in engine.rs / ast.rs, standard-library-only
   layout helper nex.rs and module declaration lib.rs. Add Nex artifact format
   and source-ordered emit-session handling using existing device memory.
6. [pending] Add mandatory package replay to scripts/test-candidate-package.cjs
   and a missing-case rejection control to scripts/candidate-package.test.cjs.
   Run scoped tests, workspace Lua tests, Clippy, fmt, docs, Ruff and Node.
7. [pending] Commit clean producer, audit unchanged pinned SpecNext target,
   retain full report/image/streams and either exact NEX parity or next issue.
8. [pending] Push PR, inspect every final-head check/workflow and five actual
   package reports/receipts/negative controls, squash merge, update #739 and
   clean owned resources after positive remote retention checks.

Owned worktree: /Users/stevehill/Projects/198x/Asm198x/asm198x-savenex.
Owned temporary paths: /private/tmp/asm198x-savenex-*.
Reuse this worktree while design/implementation is pending; remove it once the
result is merged and retained remotely. Preserve the original checkout.
