# ca65 4510 PHD alias

#748 fails because the ca65 parser asks the ISA spec for `PHD`, while the
published 65CE02/4510 spec names the instruction `PHW`. Native ca65 accepts
both spellings. The fix maps the alias only while the linked ca65 reader has
its 4510 extension selected, and adds that spelling to 4510 mnemonic reflection.
The separate 65816 front-end keeps its distinct operand-free PHD instruction.
No shared ISA, public API, dependency or other dialect changes are needed.

`probe.py` retains 99 ca65/ld65 V2.18 replay captures (50 accepted, 49 refused) in
`crates/asm198x/tests/fixtures/ca65-4510-phd.json`. Sources, configurations,
commands, raw streams, native bytes, versions and executable SHA-256 identities
are included. They cover both word forms and case variants, valid word width
edges, CPU selection/restoration and reflection, forward/scoped labels,
repeats/macros/includes, malformed operands, and three 65816 controls.

Tests check exact native bytes/refusals, formatter idempotence and bytes,
addresses and debug symbols. CLI tests preserve source locations and previous
binary, symbols, debug and both listing outputs on human/JSON refusal. The
actual-package gate requires all captures on each host; removing the new
case makes qualification fail. Both spellings enforce native unsigned-word bounds after symbols and PCs
resolve (#749). A private resolved-instruction flag selects the existing
unsigned width checker only for 4510 PHW forms; the old emission path masked
operands with 0xffff and silently changed negative and oversized values.
Direct and forward constants, negative signed-edge values, oversized addresses,
PCs and labels are captured; failed CLI assembly preserves all previous outputs.
`width-red.log.gz` proves the pre-check mismatch. 65816 PHD retains its separate
operand rules.

The expanded native sweep also retains two `a:` address-size override captures
outside the word-emission replay cohort. Both hit an independent parser hang
before emission (#751); a normal base-6502 `lda a:$1234` does too.
`probe_address_override.py` records five bounded native/Asm processes with
explicit timeout results and kills/reaps every timed-out child. That independent
reader bug is neither a normal refusal nor a passing word-emission control.

Local verification: 1,514 Rust tests pass (38 ignored), all 57 Node tests
pass, Clippy with warnings denied, Rust/Python formatting, Ruff and generated
docs checks pass. Compressed logs retain complete output; alias and width
red logs separately prove the original failures. The 99-case word-push cohort
is mandatory on every actual-package host.
