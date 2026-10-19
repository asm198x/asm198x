# Validate native .ident names before emitting source tokens

Native rejects a leading-space identifier string, but the text function currently
returns it verbatim and the source reader trims it into a different valid name.
A separate generated @cheap input with no preceding global symbol is refused by
native at anchor lookup, not by identifier-string validation. Keep those causes
separate. Both differences also exist with adjacent argument parentheses.

1. Retain the 40 source/name/spacing controls and clean producer identity. Probe
   lexical names with and without a global anchor, native diagnostic priority,
   constants/references, macro/repeat capture and inactive paths. Consult native
   ca65 V2.18 identifier-reader source if the measured character rules are unclear.
   File separate argument-validation and missing-anchor issues from the evidence.
2. Add red-first native bytes/refusal and official symbol/debug controls. Validate
   .ident string syntax through the existing text-function seam before emitting
   a bare token; do not trim a bad name into a different valid name or blacklist
   register/mnemonic spellings that belong to later statement-head semantics.
   Preserve lazy selection/capture timing and formatter source. No public shape,
   dependencies or new reader architecture.
3. Add required package cohort and omission guard. Run focused/full Rust, native
   formatting, Node, Clippy/fmt/Ruff/docs, previous/current real archives and both
   corruption controls. Freeze a clean producer and retain complete 4510 replay.
4. Ship isolated task commits on current main after #873, inspect exact-head CI,
   all five actual package reports/receipts and ten controls, rebase merge and
   clean/reuse owned workspaces. Resume #862 on these qualified prerequisites.
