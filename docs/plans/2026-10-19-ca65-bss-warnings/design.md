# Match ld65's nonzero-content warning

The 272 native controls (256 accepted, 16 refused) establish that initialization
alone is not the trigger. `GetNonBSSSection` in pinned cc65 `src/ld65/segments.c`
checks literal fragments for nonzero bytes and expression fragments for a nonzero
value. Reservations are ignored. `src/ld65/config.c` warns once per configured
zp/bss segment, including non-file areas. All-zero `.byte`, `.word`, `.dword`,
BRK, binary content and explicit fills do not warn. Nonzero data, resolved
expressions, binary content, fill and instructions do. Empty content does not.

Keep a private `has_initializer` flag on each placed statement because resolving
`.res` and `.align` currently merges absent and explicit fills. Build the set of
configured uninitialized segments from the existing layout data. In the existing
omitted-content validation branch, inspect raw bytes and explicit nonempty fill
without allocating, and inspect already validated emitted bytes for expressions
and instructions. Default reservation/alignment fill is not initialized content.
Warn once per segment using the existing advisory Warning with the first
contributing statement's file and line. No bytes, public types or dependencies
change; native warning wording/config position is tool chrome, while our own
source provenance and diagnostic contract are tested.

Preserve expression/range refusal before outputs are written. The #801 matrix
continues to pin bytes; this cohort additionally checks warning counts in human
and JSON results against native facts. Retain both zero/nonzero and duplicate/
multi-segment controls so a warning on every data statement cannot pass.

The human CLI's ca65 branch also omits the warning-rendering loop already used
by vasm and flat dialects. Route the returned ca65 warnings through that same
existing display pattern, resolving included paths via the result file table.
JSON already carries these warnings. Pin both paths so library-only success
cannot conceal the missing end-user feedback.
