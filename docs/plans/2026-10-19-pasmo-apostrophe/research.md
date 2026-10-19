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

Clean producer ce867773e5d1d54b06229dda0b9831e81df889be has executable SHA-256 c6a28a48ec8b51a2ede3cc6b379a97f027c3d8e047ca3c32312283492eaf8e75. Its actual archive passes all 74 required cohorts and all 24 new commands; changed bytes and archive controls fail. The preceding archive fails only the new cohort. The clean pinned SpecNext replay matches all 33,280 native NEX bytes (SHA-256 5fbed92a7d60dab47de8b22162c9227df051eaf67c26e85e618b24f6429dd07f), with five successful processes, ten streams, full comparison bytes and a failing changed-byte control.
