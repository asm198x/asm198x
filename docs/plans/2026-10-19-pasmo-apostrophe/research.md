# Refuse a malformed Pasmo character literal (#855)

The shared tokenizer admitted any three-character quote shape. A delimiter
inside that shape is not a character token in native PasmoNext v0.1.3: dw with
three apostrophes fails, while the preceding CLI emitted 27 00. The red native
fixture test observed that incorrect successful AssemblyResult before the fix.

The existing branch now excludes an unescaped delimiter in the middle. Ordinary
characters, packed/empty refusals, numeric apostrophe refusals and AF' retain
their native outcomes. No packed SjASMPlus grammar is admitted in Pasmo.

Both public Pasmo entry points match all six controls. Accepted formatter
round trips preserve bytes/symbols and are idempotent. Human/JSON failures in an
include preserve five prior outputs and both source files on z80 and z80n.
The mandatory package cohort executes 24 commands across those two modes, with
an omission control preventing qualification without it.

Local verification: 1,586 Rust tests pass (38 ignored), all 89 Node tests pass,
and Lua/all-target Clippy, rustfmt and generated-doc checks pass. A clean
producer and actual archive controls follow this source commit.
