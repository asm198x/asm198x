# Retained ecosystem evidence

`current.json` is the reviewed native capture used by the generated compatibility
table and ordinary-CI replay of every native-qualified target. It records the exact manifest, source pins,
producer source/lock/executable identities, native tool/version processes,
commands, first failures and final artifact identities. Small reference bytes
and their original licence notices are retained for reproducible replay.
Upstream source trees remain an index and are fetched at their full pins.

The native capture was built from development producer revision
`652114a082cad7b094201e3ad7966544bb676a92`, retained on the remote evidence branch
`evidence/ecosystem-producer-2026-10-16`. Its version string was 0.0.69; the
source revision, not that pre-release string, identifies the producer. The
public 0.0.70 package qualification has its separate release checkpoint.

Issue links can be attached during triage; native process results and binary
identities are retained unchanged. Regeneration checks the complete report,
source pins, manifest/plan digests and positive native-control/artifact evidence.
Replay also records its own Asm producer and the original native capture date.
It does not turn an old native control into a fresh one.

The current ten-target result is six exact matches, two Asm rejections and two
native-control failures. These outcomes measure this pinned corpus. Emulator
execution and real hardware validation are separate work.
