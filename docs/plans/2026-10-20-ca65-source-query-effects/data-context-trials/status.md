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

## Rolled-back scoped import guard

The final 515-case data matrix passes on 48e2428d, but the complete workspace and
actual native archives reject the candidate because `import_queries_before_definition`
is incorrectly accepted. Native refuses the definition as already imported.
The old scoped-constancy refusal had masked the existing #810 import-key defect.

A narrow trial qualified the imported guard through constant_key_at. That
restored the new refusal but lost nine previous matches in the 274-case native
import/export matrix: four import/importzp fallback cases and five mixed
visibility cases. The old producer matches 223; the trial matches 246, with nine
regressions, so its higher total is not qualification. The patch is retained
and rolled back. No import-guard trial was committed or pushed as source.
The approved #803 design explicitly deferred coupled import/global allocation,
same-file external resolution and source address-size behavior to #810.
Stop this attempt under the user-supplied rollback rule; agree the next scope
before further implementation. PR #917 must remain unmerged.
