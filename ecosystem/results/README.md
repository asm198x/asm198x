# Retained ecosystem evidence

`current.json` is the reviewed native capture used by the generated compatibility
table and ordinary-CI replay of every native-qualified target. It records the exact manifest, source pins,
producer source/lock/executable identities, native tool/version processes,
commands, first failures and final artifact identities. Small reference bytes
and their original licence notices are retained for reproducible replay.
Upstream source trees remain an index and are fetched at their full pins.

The current capture was built from main revision
`c85d1ddf0608013a52b3a3d072e68892d658e255` by the actual
[native workflow](https://github.com/asm198x/asm198x/actions/runs/38945646806).
Its complete 84-file artifact includes the report, 80 process logs, both generated
tables and a live Linux replay of all 646 native shift cases. The ZIP, API identity,
inventory, reports and independent verifier are retained in
`docs/plans/2026-10-16-next-project-boundaries/github-checkpoint/`.
The earlier main/development captures remain in
`docs/plans/2026-10-16-ecosystem-execution/github-checkpoint/`; the development
producer is retained on `evidence/ecosystem-producer-2026-10-16`.
Public package qualification is versioned separately in release checkpoints.

Issue links can be attached during triage; native process results and binary
identities are retained unchanged. Regeneration checks the complete report,
source pins, manifest/plan digests and positive native-control/artifact evidence.
Replay also records its own Asm producer and the original native capture date.
It does not turn an old native control into a fresh one.

The current ten-target result is six exact matches, two Asm rejections and two
native-control failures. The Asm boundaries are now #722 (4510 indirect-Z forms)
and #725 (SjASMPlus digit separators); #718 tracks the two failed ca65 controls.
These outcomes measure this pinned corpus. Emulator
execution and real hardware validation are separate work.
