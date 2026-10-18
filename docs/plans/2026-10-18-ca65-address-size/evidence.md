# ca65 address-size override progress and selection

The scoped-name tokenizer accepted a lone colon in its word-entry guard, but
its word loop consumed only paired colons. A single colon therefore emitted an
empty symbol repeatedly without advancing. The guard now admits leading colons
only as a `::` pair; malformed single colons reach the normal expression error.
All valid word-entry paths consume at least one character.

Native ca65 uses `z:` and `a:` before direct/indexed memory operands to select
zero-page and absolute forms. The linked ca65 reader now retains that intent
in its private payload, maps symbols normally, selects the exact existing ISA
form, and checks the selected unsigned width after resolution. Native prefixes
are contiguous and case-insensitive; whitespace after the colon is accepted,
but whitespace before it is not. Prefixes on immediate/indirect operands and
bit-branch expressions are refused as native ca65 does. The linked 6502/4510
path has no far form; the existing 65816 size resolver remains covered separately.

`probe.py` captures 113 ca65/ld65 V2.18 cases: 34 accepted and 79 refused.
`tests/fixtures/ca65-address-size.json` retains all sources/configurations,
commands, tool versions/hashes, raw streams and complete native bytes.
Accepted cases check exact bytes, formatter idempotence, reassembled bytes,
section bases and symbols. Includes, forward values, scopes/global names,
repeats, CPU variants, malformed colons and both formerly deferred PHD/PHW
address-overflow cases are covered. Human/JSON include refusals preserve binary,
symbols, debug data and both listing outputs.

Every CLI invocation in the regression test has a five-second deadline and
kills/reaps its child on timeout. The pre-fix red log shows that guard failing,
so the test cannot silently pass while the reader loops or wedge the test runner.
Actual-package replay requires the complete cohort and rejects its omission.

Local verification: 1,516 Rust tests pass (38 ignored); all 58 Node tests
pass. Clippy with warnings denied, Rust/Python formatting, Ruff and generated
docs checks pass. Adjacent compressed logs retain complete outputs.

The clean producer `8a7ebd08311a31149f71f12397d59eb796ec2210` still matches all
564 native/upstream reference bytes for the unchanged cc65 4510 opcode target,
SHA-256 `8a5fcde630e55dd49403f7c6ffed25d8786e65e3d0a2723531b7ca3e2c0509ec`.
`4510-after-address-size/` retains full comparison bodies and fourteen raw
streams. `verify_replay.py` checks their process/step identities, pinned source
and producer, and changed-byte rejection. The focused result does not replace
the complete ecosystem cohort's current report.
