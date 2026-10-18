# Native ca65 include input priority

The old selected sweep read an include immediately inside every macro/repeat.
Native ca65 opens the file at that directive, but its token input stack takes
priority over file characters. For `repeat 2 / byte I / include code.inc`, native
emits `00010707` while the old reader emitted `00070107`. Macro bodies show the
same ordering. Pending files are consumed in reverse open order, and their code
sees the scope/binding state after the outer token stream completes.

The selected sweep now records token boundaries using existing macro expansion
frames and repeat recursion, opens selected includes at their original point,
and consumes their contents in LIFO order at the outer boundary. A watermark
keeps lower pending files suspended while a loaded file runs its own macros.
Ordinary/conditional-only includes and binary emissions remain immediate.

Path lookup includes suspended character sources. Actual recursive ancestry is
tracked separately: repeated pending siblings and a child reusing a sibling are
valid, while recursive cycles/depth still fail. The existing ordinary loader
entry retains its guards through the shared helper. Binary lookup retains the
current source's first choice, with suspended directories available as fallback.
Native controls prove both lookup orders.

The multi-file cursor previously omitted macro origins on conditional/repeat
header nodes. Those existing spans now use the same origin map as ordinary
lines. This keeps a whole outer macro invocation active across nested blocks;
frame order is innermost first, so the last frame identifies that boundary.
No AST, span, public API or external format shape changed. The binding decision
records this measured input-priority refinement on the existing lazy sweep.

`probe.py` retains 44 qualifying native ca65/ld65 V2.18 cases: 33 accepted and 11
refused. The CLI checks native bytes, formatter idempotence, reassembled bytes,
section bases and symbol addresses. Human/JSON include errors preserve every
previous output and identify the included file/line; a cyclic include must fail
with the cycle diagnostic. Every child has a five-second kill/reap watchdog.
The red log detects the original native-refusal mismatch.

Package qualification requires 46 cases; this cohort executes 143 commands per
host. Omission is refused. A local real Lua-enabled archive passed every case
and rejected both wrong bytes and corrupt archives. Its compressed reports are
preflight evidence without a producer-lock receipt; five-host CI qualification
remains required before merge.

Local verification: 1,523 Rust tests passed, 38 ignored; 61 Node tests passed.
Clippy with warnings denied, Rust/Python formatting, Ruff and generated docs
checks passed. Compressed logs retain complete output.

`deferred-native.json` retains #768's separate text-symbol capture mismatch.
Further probes show valid macro source emits `0102` natively but `0202` in Asm:
the body captures at definition while arguments read at invocation. That is the
next slice, using the existing live macro-state/one-layer expansion pattern.
The unsupported .set/:= spellings remain tracked as #761/#762.
