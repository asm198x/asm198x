# Numeric separators (#725)

SjASMPlus 1.21.0 accepts apostrophes between digits in its decimal, hexadecimal
and binary forms. The original two audio instructions emit `f6e0e6e7`.
The replay fixture captures 65 real native executions: 44 accepted sources and
21 refusals. It includes prefix/suffix case, signed 32-bit limits, operators,
EQU/IF/ASSERT, data lists, strings and characters, AF', statement colons,
STRUCT initialisers, DEFINE and macro expansion. The bounded `probe.py` records
argv, executable SHA-256, version, diagnostics and bytes. It removes each old
native image before executing the next case.

The expression lexer ended the numeric token before an apostrophe and sent the
remainder to character-literal parsing. Independent quote scanners could also
hide commas, colons or a STRUCT closing brace behind that apostrophe. The
retained `red.log.gz` demonstrates both the expression rejection and a failed
CLI success control before the correction.

The lexer now retains numeric apostrophes only for SjASMPlus. Its number parser
selects the radix before validating separators within the digit body, so leading,
trailing and doubled separators, prefix/suffix splits, invalid digits and u32
overflow still fail. Existing signed 32-bit expression semantics remain intact.
Quote scanners use the same numeric-token boundary check outside quoted text;
the shared macro machinery obtains that check through its existing dialect seam.
Pasmo retains its previous grammar. Formatter checks compare bytes and symbols
through both Z80 and Z80N entry points, and require idempotent output.

Native substitution is deliberately retained: with `DEFINE FF 7`, `$F'FF` is
`$F'7`, yielding 0xf7. Macro parameters can replace both hex fragments (`F`
parameter 7 makes `$F'F` into `$7'7`). Treating the entire numeric spelling as
opaque during substitution would silently differ from the reference.

CLI controls run both CPUs with human and JSON diagnostics, assert the included
source's line 5, retain all five previous output artifacts on refusal, and check
that the input itself is unchanged. The new mandatory package case replays all
65 sources against the packaged executable; the old 0.0.71 qualification cannot
satisfy that expanded required-case set.

This correction covers separators in the existing decimal/hexadecimal/binary
number grammar. It does not establish complete SjASMPlus syntax compatibility.
The unchanged pinned whole-project build is checked separately below.


## Whole-project result and validation

The clean producer is `fa4014a428204ff2f50166950fda535dd0f583ef`.
`specnext-after-separators/report.json` and all ten adjacent process logs retain
its explicit ecosystem audit. The unmodified upstream pin remains
`e3211ea8ad2e83aca4d07d70bd8e09b7b51a9a1d`. Native assembly succeeds and emits
33,280 bytes, SHA-256
`5fbed92a7d60dab47de8b22162c9227df051eaf67c26e85e618b24f6429dd07f`;
the complete NEX bytes are retained as hex in that report. Asm198x now reaches
`src/sprite_data.asm:19` and reports ``invalid number `%` `` at the infix
remainder operator. This is a distinct next boundary, not a failure of the
accepted separator cases. `remainder-boundary.json` retains the minimal native
`ASSERT (5 % 2)==1` / `db 5 % 2,%101` control: native bytes `0105`, Asm rejection.

The final workspace test run passed 1,494 tests with 38 explicitly ignored;
Clippy with all targets and Lua passed with warnings denied. Formatting,
generated docs, Ruff and all 49 Node package/workflow tests passed. The
compressed validation logs are retained alongside the initial red test.

The next remainder boundary is tracked in [#731](https://github.com/asm198x/asm198x/issues/731).
