# Native character boundaries in data operands (#900)

Keep this separate from physical lexical admission (#835/#899). The measured
`.byte ','` safe refusal comes from `mos6502::split_data_items`: it protects
double-quoted strings and parentheses, but treats a comma character as a list
separator. `split_top_level` also counts parentheses inside character tokens.

1. Retain native ca65/ld65 traces across byte, word, big-endian word and dword
   data; comma, double quote, apostrophe, backslash, semicolon and parentheses;
   lone, neighbouring, arithmetic and function operands; selected and macro
   source; linked ca65 and flat 65816/HuC6280 readers. Replay the clean preceding
   producer before changing source. Name unrelated text/formatter gaps by issue.
2. Add the smallest ca65-family operand-boundary reader at the existing data-list
   seam, protecting native character and string tokens before separator/depth
   handling. Reuse an existing native-aware scanner if it fits. Preserve raw
   spelling and private computed-string handles. Do not change shared MOS
   dialect grammar or add a new parser architecture.
3. Update only the verified ca65 data consumers in `ca65/mod.rs`, `ca65_816.rs`
   and `ca65_huc6280.rs` which lose character boundaries. Check all callers
   before editing. Keep lexical errors in their already approved admission seam.
4. Promote `ascii_44_direct` from the explicit #900 refusal in
   `tests/fixtures/ca65-quoted-admission.json`. Add native byte/refusal,
   output-preservation, source/debug, canonical/idempotent formatting and
   required archive/omission coverage. Run the earlier quoted/byte/text cohorts.
5. Run full Rust/Lua, Clippy/fmt/Ruff/docs and Node checks. Freeze a clean
   producer, inspect local package and corruption controls, fresh 4510 evidence,
   exact-head CI and all five native package archives; rebase-merge and clean
   the owned workspace/caches after retaining evidence.
