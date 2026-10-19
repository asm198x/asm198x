# Omit native BSS file bytes without skipping validation (#801/#806)

The native matrix contains 216 completed cases: 152 accepted and 64 refused.
It covers zp/bss/ro/rw types, filled/unfilled and shared/separate file areas,
reservations, data and instruction errors, labels, PC widths, relative branches,
explicit absolute forcing and alignment/start gaps. Native commands, versions,
source/config text, stdout/stderr, status and output bytes are retained.
The clean preceding enum executable disagrees on 72 cases; the corrected
executable matches every byte and acceptance outcome. The preceding executable
identity is retained separately. Native initialized-data warnings are #807.

ld65's writer advances memory for every present segment, writes initialized
content only for ro/rw, writes preceding alignment/start gaps for initialized
or filled segments, and fills omitted segment spans only in filled areas.
The bounded layout previously derived file placement from CPU placement and
ignored type. Separate private file cursors preserve both behaviors. Omitted
file content still goes through existing expression/range checks, preserving
included-definition spans and all five old outputs on refusal. Raw bytes and
reservations require no temporary validation buffer.

The direct-PC width query previously had no segment context, unlike alias
width queries. Passing the existing byte-segment context fixes #806 without
changing explicit absolute forcing or relative branch calculation. Accepted
cases retain formatter idempotence, bytes and debug section/symbol shape.

The first full run caught an absent pinned VECTORS regression:
`left: Some(644)` / `right: Some(32778)`.
An absent segment keeps its potential file address without consuming bytes
or advancing either cursor. The existing expectation is unchanged; both the
initial failing log and final passing log are retained.

Final checks pass 1,555 Rust tests (38 ignored), 75 Node tests, Clippy across
workspace targets with Lua and warnings denied, fmt, Ruff and generated docs.
The expanded 216-case focused test also passes. The actual local Lua archive
passes all 60 mandatory cohorts, including 672 commands here, and both archive
corruption controls fail for their intended reasons. The actual preceding
archive fails only this cohort while its other 59 pass. Local archives
are artifact-only; clean producer replay and five-host qualification remain
required before merge.

Mixed-fill raw probes remain research evidence and are not qualification
cases: differing per-area fill bytes exceed the existing one-fill image seam.
The supported matrix uses the same fill byte for filled areas. No existing
fixture was removed or weakened; no shared schema or dependency changed.
