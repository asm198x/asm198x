# ca65 4510 PHD alias

#748 fails because the ca65 parser asks the ISA spec for `PHD`, while the
published 65CE02/4510 spec names the instruction `PHW`. Native ca65 accepts
both spellings. The fix maps the alias only while the linked ca65 reader has
its 4510 extension selected, and adds that spelling to 4510 mnemonic reflection.
The separate 65816 front-end keeps its distinct operand-free PHD instruction.
No shared ISA, public API, dependency or other dialect changes are needed.

`probe.py` retains 53 ca65/ld65 V2.18 captures (44 accepted, nine refused) in
`crates/asm198x/tests/fixtures/ca65-4510-phd.json`. Sources, configurations,
commands, raw streams, native bytes, versions and executable SHA-256 identities
are included. They cover both word forms and case variants, valid word width
edges, CPU selection/restoration and reflection, forward/scoped labels,
repeats/macros/includes, malformed operands, and three 65816 controls.

Tests check exact native bytes/refusals, formatter idempotence and bytes,
addresses and debug symbols. CLI tests preserve source locations and previous
binary, symbols, debug and both listing outputs on human/JSON refusal. The
actual-package gate requires all captures on each host; removing the new
case makes qualification fail. Existing PHW overflow truncation (#749) remains
separate, as captured in the preceding bit-branch slice.

Local checks: 1,514 Rust tests pass (38 ignored), all 57 Node tests pass,
Clippy with warnings denied, Rust/Python formatting, Ruff and generated docs
checks pass. Compressed logs alongside this file retain the complete output.
The pre-fix red log proves the native alias refusal in library and CLI paths.
