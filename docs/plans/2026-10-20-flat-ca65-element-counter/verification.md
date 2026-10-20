# Per-element counter verification (#910)

Both flat readers now bind each Pc leaf to its data element's byte offset through
an exhaustive private Expr visitor. Numeric lists pass the directive width;
byte lists count decoded literal/constructed bytes. The shared evaluator and
other dialects keep their statement-start Pc contract. No source callback,
expression evaluation, new operation or public shape is introduced. Original
Node.source remains authoritative for formatting.

All 304 native cases agree: 271 accepted and 33 refused. This fixes 165 wrong-byte
outputs and all 33 wrong acceptances while retaining the other 106 matches.
Pc+Pc and Pc-Pc controls distinguish correct leaf binding from a final-result
adjustment. The 254-case broader data-width sweep now agrees completely.

All five focused data suites pass, covering 768 supported native cases. Success
checks compare file/JSON/native bytes, use the official debug reader, and require
symbol/section round trips and idempotence. All 33 counter-range errors are also
formatted twice and reassembled: they remain rejected at the expected location,
and all four existing output files survive. The new required archive cohort
uses 1,216 commands and cannot be omitted. All 114 Node checks, all-target Clippy,
rustfmt and generated docs/version checks pass. Full workspace and frozen
artifact/native/five-host CI evidence remain required before merge.

The complete Rust/Lua workspace passes 1,636 tests (38 ignored), with all-target
Clippy and rustfmt green. All 114 Node checks pass after the coverage guard.
The frozen CLI from 0a67ffa43e7eadd15103c84ec0795b7ba8a60739 has SHA-256
9dd103083dba8a4ecd5148ad619d317b8ff3f965a371be9c4db6370aff10e8bc.
The subsequent c474b640d293fe911de12a22403b90a89800854f change touches only
integration-test/package-verifier coverage guards; a checked build has identical
binary bytes. Both identities and the exact two changed paths are retained.

All 99 archive cohorts pass, including the new 1,216-command cohort. The
preceding archive fails only the counter cohort. Wrong bytes fail exactly three
controls; corrupt archives fail SHA-256. Clean native replays match all 80 macro,
132 far-address, 148 address, 304 counter and 254 broad cases. The 108 wide-data
replay retains only its four explicit #895 inactive-source refusals.
Fresh release-mode 4510 replay from c474b640 matches all 564 bytes and fourteen
streams, rejects changed bytes and removes disposable work. Exact-head CI and
five actual native archive reports remain required before merge.

PR #913 passed all 29 exact-head checks and all five workflows. Independently
inspected native archives on ARM/Intel macOS, ARM/Intel Linux and Windows all
pass 99 cohorts, including all 304 counter cases and 1,216 commands. Every
archive also rejects wrong bytes and checksum corruption. All hosts passed on
attempt one. The rebase merge b059280c2a31004ebd6eb25df854be296481ec2f has
the exact qualified head tree and is an ancestor of origin/main. #910 is closed.
Complete reports, producer-lock identities and merge evidence are retained in
qualification-913.
