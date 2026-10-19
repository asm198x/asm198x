# Bind labels before scope and procedure openings (#863)

The current selected walk checks scope directives before splitting a source label.
`mark:.scope outer` therefore reaches operation parsing and fails; generated heads
reach the same path after live text evaluation. Bind an explicit head in the
existing selected projection before walking the scope-opening remainder. Preserve
the enclosing scope, address kind, implicit procedure symbol and native name rules.

1. Retain native probes for static/generated/nested heads, named/unnamed scope and
   procedure openings, cheap/anonymous/global anchors, parent qualification,
   duplicate and reserved names, inactive source, repeats/macros and include spans.
   Probe labelled scope closers separately rather than infer their semantics.
2. Add red-first differential Rust controls using native bytes, official debug
   symbols and formatter idempotence/bytes. Use the existing private selected
   `bind_head_label` seam; no AST/schema/dependency or reader redesign.
3. Add required package cohort and omission guard, run focused/full Rust, native
   formatting, Node, Clippy/fmt/Ruff/docs, prior/current archives and both negative
   controls. Retain clean producer and complete 4510 replay evidence.
4. After #867 merges, ship isolated commits on current main, inspect exact-head
   checks and all five actual package reports, receipts and ten negative controls;
   rebase merge and clean or reuse owned workspaces after retention verification.
