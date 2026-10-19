# Preserve native counter positions (#812/#813)

All 145 completed native cases match: 126 accepted and 19 refused. The actual
preceding scoped-export executable disagrees on 136. The matrix covers labels
on or just before .org/.reloc, global/cheap/anonymous/scoped/procedure labels,
offsets, aliases, macro/repeat/include streams, all four data widths, strings,
low/high operators, instruction baseline, overflow and lazy value demand.
Native sources/configs/process streams/statuses and explicit executable hashes
are retained. No native timeout is an expected refusal.

Native ca65 main.c defines a label before HandlePseudo. The linked emitter
instead changed the PC before inserting it. Moving that existing update after
label definition fixes the value while preserving deferred definition placement
positions. Data loops also reused one PC for every expression; advance by
1/2/2/4 bytes. The existing lazy demand walk now uses the same positions, so a
counter-dependent short-circuit does not force an otherwise unused cycle.
Sixteen controls prove both skipped and demanded branches across all widths.
No instruction evaluation rule or public data shape changes.

The first full run found an old unit expectation that pinned the data bug:
`left: [0, 32, 3, 32]` / `right: [0, 32, 1, 32]`.
The exact source was run through native with CODE at $8000; it emits
`112200200320330680`, proving the second word reads $2003. The corrected unit
expectation and its new retained native capture agree. The failed log remains;
the test was not disabled or weakened.

Final checks pass 1,561 Rust tests (38 ignored), 78 Node tests, Clippy with Lua
and warnings denied, fmt, Ruff and generated docs. Accepted cases pin formatter
idempotence, reassembly bytes and stable debug shape. The initial relative label
also agrees with its debug section address. Included human/JSON range errors
preserve all five old outputs and both source files.

The actual Lua archive passes all 63 required cohorts, including 523 commands
here. Both corruption controls fail as intended. The actual preceding archive
fails only this cohort; its other 62 pass. Local archives remain artifact-only;
clean producer replay and five-host qualification remain required before merge.

The absolute-org debug value mismatch is independently reproduced before/after
and tracked in #814. Origin validation is #815: the native high-origin rows
fail at .org, so their refusal is not evidence of native operand overflow.
Separate follow-up captures retain those facts; the current correction leaves
no new origin acceptance. Missing size attributes remain #798, whose 100-case
research is retained on fix/ca65-symbol-sizes before this higher-priority fix.
