# Accept native ca65 hexadecimal suffixes (#819)

All 30 completed native controls match: 23 accepted, seven refused. The actual
preceding executable differs on 21. Controls retain source/config/tool hashes
and every native/Asm process stream, status and explicit executable hash.
There are no timeout-as-refusal expectations.

Native scanner.c accepts digit-starting hexadecimal h/H tokens through
0xFFFFFFFF. The existing expression lexer retains the whole token, but all
three ca65 readers passed it to the MOS parser, which has only prefixed hex,
binary, characters and decimal. Add a private ca65 numeric callback at the
existing shared helper seam. Delegate other spellings unchanged. Token-value
comparison uses the same callback: .xmatch now agrees that 0FFh, $FF and 255
are the same numeric token. No dependency, public schema or grammar model
changes. Active suffix overflow still refuses before lazy numeric evaluation.

The new tests fail on preceding code; that log is retained. Broad flat-reader
checks independently found two older gaps using ordinary $FF input: .define
is refused by both flat readers (#820), and HuC6280 assembles a macro its
formatter refuses (#821). Native/preceding source and process evidence proves
both; 65816 formats that same macro successfully. Do not widen this numeric
fix into their text/AST repairs. The linked path checks all 30 cases; both
flat readers check the other 29 numeric cases. HuC6280 macro bytes remain
checked, with its known formatter failure tracked separately. Other accepted
cases check formatter idempotence, byte-identical reassembly and debug shape.
Included human/JSON suffix errors retain the path and line 2 while preserving
all five existing outputs and both source files.

Final checks pass 1,565 Rust tests (38 ignored), 80 Node tests, Clippy with
Lua/all targets/warnings denied, fmt, Ruff and generated docs. The final native
token identity control was added before the final frozen full/archive checks.
The actual local Lua archive passes all 65 required cohorts, including 109
commands here: all 30 linked controls plus positive/negative suffix controls
on each flat reader. Wrong-byte and corrupt-archive controls fail as intended.
The actual preceding archive fails only this cohort; its other 64 pass.

#788's approved skipped-source lexical check is still a separate implementation.
The #814 audit independently found global absolute cursor byte differences
across segment changes (#822); this numeric fix makes no counter change.
Clean unchanged 4510 producer replay and five actual host qualification
reports/receipts with all ten controls remain required before merge.
