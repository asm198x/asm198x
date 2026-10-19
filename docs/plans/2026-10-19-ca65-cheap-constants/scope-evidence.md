# Source-time ca65 scope and instruction binding (#778/#784)

PR #783 is held until these corrections pass the complete qualification gate.
A newly accepted procedure with a cheap constant otherwise emits 07 where native
emits 09. The original package disagrees with 54 of the first 92 retained native
scope/width cases. scope-package-red.json retains each comparison.

Native ca65 V2.18 preserves the current cheap anchor on ordinary scope entry.
A named procedure defines a real label and changes it. Neither closer restores
an outer anchor. Positive physical output causes an internal size definition on
closure, which starts another cheap namespace; an empty span does not. Struct
and union closure defines a size even when empty, while an empty enum preserves
the anchor. Member definitions are ordinary anchor events. The selected reader
now follows those events through the existing ScopeEnv.

The physical source cursor uses retained ISA form lengths and the linker's
alignment calculation, per segment. org/reloc do not change physical offsets.
Included character input deferred behind macro/repeat tokens is counted where
it is actually read, so it cannot retroactively give an already closed scope
size. The fixture covers these phase distinctions.

A forward constant or zero-page label must not shorten an earlier instruction.
The private projection retains its resolved ISA form/operands at the read point,
using only that operand's live dependencies. globalzp/importzp declarations can
hint the short form; exportzp alone cannot. The shared AST and external schemas
are unchanged. Retained operands receive the ordinary final scoped-name repair;
4510 relative16 adjustment happens once. Cheap debug constants use authored
names, without rewriting record-size auxiliary keys or symbol-export keys.

The final fixture has 102 native cases (100 accepted, two refused). Rust and
actual-package checks require native status/bytes, formatter idempotence,
reassembled bytes and unchanged debug symbol/section shapes. An included
post-scope undefined cheap reference preserves all five outputs in human/JSON
modes and reports the actual file/line. A debug control checks authored cheap
names and preservation of record auxiliary names.

The Lua-enabled local archive passes all 51 required cohorts. Wrong bytes fail
exactly reference_bytes/determinism/structured_success; a corrupted archive fails
its digest check. The earlier archive fails only ca65_scope_anchors under the
new gate. Local package reports are preflight evidence; exact-head five-host
producer-lock qualification is still mandatory before merge.

Two separately retained disagreements continue as #785 (later local constant
shadowing) and #786 (zero-length struct fields). Initial width-before.json.gz
used an incomplete ZEROPAGE linker configuration; it is diagnostic history,
not qualifying native evidence. The regenerated qualifying fixture has a
complete configuration accepted by both readers.

Final isolated verification: 1,534 Rust tests passed (38 ignored), 66 Node tests
passed, and Clippy/all features, fmt, Ruff and generated docs passed.
