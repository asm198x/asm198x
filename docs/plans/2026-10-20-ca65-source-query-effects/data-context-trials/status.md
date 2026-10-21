# Preflight evidence

These are completed development runs, not the final qualified producer.
The initial 360-case implementation passed its measured cases. Additional
selector/string admission probes found missing list expansion and eight
retained byte-string differences, all repaired before final qualification.
The first replay incorrectly omitted --flat and stopped before the conditional
baseline because that retained file is named conditional-after.json. The
corrected 23-dataset run retains every native match across 2,466 cases.

The initial full run failed two obsolete library expectations (773 passed):
`undefined symbol ext` and the assertion about `undefined symbol zzz`.
Native ca65/ld65 confirms referenced unresolved imports fail linking and scoped
constants are accepted. The next library run (774 passed) failed only its new
missing-scope message assertion (`no such scope pa`); that expectation was made
precise. No implementation was weakened to satisfy these legacy assertions.
The completed final focused run, later full run and release replay are retained
separately. These logs do not qualify the final source or count as green checks.

## Superseded first PR head ce3955b3

The 500-case release replay and all 2,466 previous native cases passed, and a
fresh 4510 audit passed. The full workspace run then exposed a real regression:
`outer::.ident` was treated as a numeric function. Its exact failure is retained
in the ce3955 qualified-full log. The existing generated-format fixture pinned
this case; the final data audit adds direct/include/macro/lazy and ordered
constructor controls. The source-aware parser recognizes the qualified `.ident`
component at its read position. Both active PR workflows were cancelled and the
bounded monitor stopped; ce3955 is not qualified for merge. Its producer and
replay receipts are retained here only as superseded preflight evidence.
