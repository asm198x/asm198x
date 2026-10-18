# Procedure closers, repeat tokens and UTF-8

Native ca65 treats unnamed `.proc` as deprecated `.scope`, including its
`.endscope` closer. The private scope stack now records that effective kind;
named procedures retain their own address labels and `.endproc` closer.

The old repeat sweep substituted operands after parsing, so it accepted counter
names on immutable definition/label lines, missed scoped uses, and permitted a
nested counter name that native had already replaced with a number. The live
reader now substitutes counter tokens before parsing selected lines and block
heads, and after text-symbol expansion. It does not rewrite the source AST or
read untaken branches. Counter headers require an identifier. Existing macro
substitution supplies quoted-token handling; a ca65 repeat boundary adapter
protects cheap-local markers and numeric prefixes. Outer counters stop at an
include file's input boundary. The old delayed operand substitution is removed.

The shared substitution scanner appended each UTF-8 byte as a Unicode scalar,
changing `c3a907` to `c383c2a907` in an ordinary ca65 macro. It now copies complete
code points while scanning ASCII token boundaries. Native macro and repeat
string controls verify the original bytes and formatter round trips.

`probe.py` retains 35 qualifying native ca65/ld65 cases: 21 accepted and 14
refused. They cover named/unnamed closers, scopes, definitions and labels,
qualified names, nested/zero repeats, substrings/case, cheap locals, numeric
literals, quoted UTF-8, text definitions, conditionals and include boundaries.
CLI tests check native bytes, idempotent formatting, reassembled bytes, section
bases and symbol addresses. Human/JSON include errors retain their location and
preserve all previous outputs. Every child has a five-second watchdog and is
killed/reaped on timeout. The retained red log detects the original closer gap.

Package qualification requires 45 cases. This cohort executes 98 CLI commands
per host, including all accepted formatter/byte/debug round trips. Omission is
refused. A local archive containing the real Lua-enabled debug executable passed
all 45 cases; its compressed report is a preflight, without a producer-lock
receipt, and does not replace the five CI package qualifications.

Local checks: 1,521 Rust tests passed, 38 ignored; 60 Node tests passed. Clippy
with warnings denied, Rust/Python formatting, Ruff and generated docs pass.
After rebasing onto the scoped-constant squash, every tested crate, fixture,
script, xtask, lockfile and build configuration tree matches `tested-tree.json`.
Compressed logs retain complete outputs.

`deferred-native.json` retains the .set/:= gaps (#761/#762) and the repeated
include binding gap (#765). Additional ordering probes show that native consumes
macro/repeat token streams before include character streams; #765/#766 are the
next bug slice. Conditional-only includes remain immediate in both tools.

Clean producer `70751eb608d57db8e1c44c345deea6d1c7c65d43` still matches all
564 bytes of the unchanged pinned cc65 4510 target (SHA-256
`8a5fcde630e55dd49403f7c6ffed25d8786e65e3d0a2723531b7ca3e2c0509ec`).
`4510-after-scope-repetition/` retains full bytes and fourteen raw streams.
The verifier checks source/producer identity, process completion and content,
and rejects a changed byte. The complete ecosystem report remains unchanged.
