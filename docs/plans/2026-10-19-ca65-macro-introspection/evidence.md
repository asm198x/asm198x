# Preserve ca65 macro introspection tokens

Native ca65 V2.18 preserves `.definedmacro(emit)` and `.paramcount` inside
quoted byte strings, accepts ASCII keyword casing and keeps macro names and
parameters case-sensitive. The previous packaged tool replaces the first
literal with byte `31` instead of its 19 native bytes. A generated-body query
also answered before live text alias expansion (`00` instead of native `01`).

The shared private scanner now rewrites complete reserved tokens outside
strings/comments, preserves UTF-8 and leaves malformed operands for ordinary
diagnostics. User-symbol matching is unchanged. One-layer live expansion leaves
generated-body queries for the next selected read, after text substitution;
recursive pre-pass consumers keep their existing evaluation phase. No new trait
hook, dependency, AST, span or external contract shape is introduced.

`probe.py` and the fixture retain 35 native ca65/ld65 cases: 30 accepted and
five refused. Bounded CLI tests compare native status/bytes, formatter
idempotence, reassembled bytes and section/symbol debug shapes. The cohort
covers body/argument alias timing, rebinding, nested calls, conditionals,
repetitions, includes, token boundaries, strings/comments and malformed input.
Existing macro/capture/include/repetition tests also pass.

The previous real archive fails only the new introspection cohort on its first
native-byte check. The repaired Lua-enabled local archive passes all 48 required
cases, including 125 commands here, and rejects both wrong-byte and corrupt-
archive controls. Complete reports and the original red test are compressed
alongside the native fixture; local preflight has no producer-lock receipt, so
five-host CI qualification is still required before merge.

Local verification: 1,526 Rust tests passed (38 ignored); 63 Node tests passed.
Clippy with warnings denied, fmt, Ruff and generated docs passed. After rebasing
onto merged #768, every tested crate, script, workflow and lock/config tree was
verified identical to the tested tree.

Clean producer `4cd56a3b139464259f9b748828560b41ec45a8ad` matches all 564
unchanged upstream/native 4510 bytes, SHA-256
`8a5fcde630e55dd49403f7c6ffed25d8786e65e3d0a2723531b7ca3e2c0509ec`.
The retained focused report contains complete comparison bodies and fourteen
raw process streams. Its verifier checks all content/identities/completion and
rejects a changed byte. The full ecosystem ledger remains untouched.

The name-boundary controls exposed native refusal of dotted definitions that
Asm accepted. `deferred-dotted-definitions.json` retains that distinct gap,
tracked as #773 and being repaired next. Mutable/label-assignment syntax stays
separately tracked as #761/#762.
