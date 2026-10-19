# Accept native ca65 hexadecimal suffixes

1. Re-consult syntax, shared front-end, native AST, source phase and diagnostic
   decisions. Retain native active h/H literal controls across values, contexts,
   leading zeros, widths, macro/text use, refusal and quoted/comment baseline.
2. Add a private shared ca65 numeric callback in ca65_flat.rs. Keep ordinary
   numbers delegated to the existing MOS parser, accept numeric h/H tokens up
   to native 0xFFFFFFFF, and route linked/65816/HuC6280 readers and token-number
   classification through it. Leave skipped-source range validation in #788.
3. Prove native bytes/status and formatter/debug invariants, all three ca65
   entry points, included diagnostics and preservation of existing outputs.
   Add the required package cohort and omission control, proving predecessor red.
4. Run Rust/Lua, Clippy, fmt, Node, Ruff/docs, actual local archive and controls,
   clean unchanged 4510 replay. Push task commits and inspect exact-head CI plus
   five actual host package reports and corruption controls before squash.
5. Verify closure and remote retention, clean finished worktree/build caches.
