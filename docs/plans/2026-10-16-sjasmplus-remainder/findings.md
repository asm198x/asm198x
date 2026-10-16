# SjASMPlus remainder (#731)

The retained native fixture has 236 executions of SjASMPlus 1.21.0: 224 accepted
sources and 12 refusals. Its executable identity, version, argv, diagnostics and
bytes come from the bounded `probe.py`. It deletes each previous image before
running the next source. `red.log.gz` retains the initial failing differential
and CLI success controls before the correction.

`%`, `MOD` and `mod` are left-associative at the multiplication/division level.
The dividend determines a negative remainder's sign. Operands and intermediate
results retain the existing signed 32-bit expression semantics. Prefix `%`
continues to introduce binary digits: `5%10` is 5 remainder decimal 10, while
`5%%10` is 5 remainder binary 10, yielding 1. Case-mixed `Mod`/`mOd` are refused.
`MOD` and `mod` are also legal symbols when a value is required, including
`MOD equ 3` followed by `8 MOD MOD`.

The old lexer always read `%` as a number prefix, and its word-operator table
had no remainder spelling. The existing private token enum now admits remainder
only after a complete operand. The multiplicative parser lowers it to the
neutral engine's existing `BinOp::Mod`, with the same signed-word wrapper as
multiplication and division. The engine, public expression API, AST and Pasmo
parser are unchanged. Pasmo still accepts its binary literals and MOD symbols
while refusing these SjASMPlus infix spellings.

DEFINE expansion happens before this distinction: with `DEFINE DIVISOR %10`,
`5%DIVISOR` must become `5%%10`. The old preprocessor hid the identifier inside
a purported binary-number run. Leaving `%` as punctuation during DEFINE
substitution also preserves the native prefix case `DEFINE DIGITS 101` followed
by `%DIGITS`. Hexadecimal and digit-led DEFINE behavior is left for a separate
measured follow-up rather than broadening this correction.

The matrix covers signed values, aliases, precedence/associativity, intermediate
wrapping, numeric separators, numeric/parenthesized/symbol/location-counter
operands, binary prefixes, DEFINE and macro expansion, STRUCT initialisers,
forward constants, EQU/data/IF/ASSERT, zero divisors and eager logical operands.
Accepted cases round-trip bytes and symbols through the formatter and require
idempotence in both Z80 and Z80N entry points. CLI controls check included line 5
in human and JSON diagnostics and preserve the source plus five previous output
artifacts on failure. The package gate replays every case against the actual
packaged executable.

The reference [multiplicative parser at v1.21.0](https://github.com/z00m128/sjasmplus/blob/v1.21.0/sjasm/parser.cpp#L184)
uses native signed arithmetic and explicitly diagnoses a zero divisor. The
`INT_MIN % -1` C++ overflow case is excluded from the portable matrix, as the
previous width audit excluded `INT_MIN / -1`; an ARM observation cannot establish
cross-host behavior for undefined native arithmetic. This is not a claim of
complete SjASMPlus expression compatibility.

Final local validation: 1,497 workspace tests passed, 38 explicitly ignored.
Clippy with all targets/Lua and warnings denied, formatting, generated docs,
Ruff and all 50 Node package/workflow checks passed. Their logs are retained.


## Unchanged SpecNext build

The clean producer `fee20f09e7e3f234cbd7cd9178c6175740f4991a` was built and
executed by the explicit ecosystem runner against upstream pin
`e3211ea8ad2e83aca4d07d70bd8e09b7b51a9a1d`. The original native build succeeds
and produces the same 33,280-byte NEX, SHA-256
`5fbed92a7d60dab47de8b22162c9227df051eaf67c26e85e618b24f6429dd07f`.
`specnext-after-remainder/report.json` retains that complete image as hex and
all ten process streams are retained alongside it and checked against the report.

Asm198x passes the original remainder assertion and reaches `src/main.asm:272`,
then refuses `DISPLAY "Code start: ",/H,CODE_START` with `expected a value`.
`display-boundary.json` retains a minimal native control (one emitted byte `01`)
and the Asm refusal. This is a separate display-format boundary; the complete
project is still not equivalent.

An independent pre-existing DEFINE bug is captured in `define-boundary.json`.
Native `DEFINE FF 7` followed by `db $FF` or `db #FF` emits `07`; Asm emits `ff`
without an error. Native `DEFINE X 3` followed by `db 1X` emits `0d`, while Asm
rejects the number. Both executables, versions, hashes, exact source, diagnostics
and bytes are retained. The hex/digit-led protection in `substitute_once` is
unchanged by this remainder correction and needs its own native-parity fix.
