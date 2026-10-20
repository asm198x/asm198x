# Native source constancy through retained aliases

This is the source-classification portion of the approved #795 context design.
It fixes .ifconst/.ifnconst first; numeric text argument effects and live .const
integration remain required for #795. No scope namespace or layout values are
introduced. The existing Deferred source expression graph remains authoritative.

1. Add source-constancy-probe.py and retain native cases for direct and aliased
   differences, scoped/explicit names, aliases before later labels, negation,
   multiplication by zero, cycles, long chains, includes and macros. Compare
   the frozen 9dd103 predecessor before changing production code.
2. In dialects/ca65/deferred.rs expose an iterative dependency order over the
   retained source expressions, preserving strict current-scope source names.
   In dialects/ca65/project.rs extend the existing segment-weight classifier
   with those cached alias classifications, native unary cancellation and zero
   products. Keep current source values and cycle errors authoritative.
3. Extend tests/ca65_deferred_aliases.rs with a native source-constancy fixture;
   verify native bytes, refused outputs, debug shape and idempotent formatting.
   Add required ca65_source_constancy archive coverage and an omission guard.
4. Freeze the successful CLI and replay the original/extended #795 controls and
   predecessor source-phase/text matrices. Require every old match to remain;
   retain explicit remaining #795 gaps rather than broadening this slice.
5. Run full Rust/Lua, Node, Clippy/fmt/docs, a fresh 4510 replay, local archive
   and predecessor/corruption controls. Push a checked PR. Verify all five native
   archives and exact-head CI before rebase merge. Retain evidence and clean
   disposable directories while keeping the active #795 workspace and baseline.
