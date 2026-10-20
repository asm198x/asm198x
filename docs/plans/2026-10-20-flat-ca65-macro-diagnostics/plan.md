# Restore flat ca65 backend macro diagnostic context

Native ca65 reports an included macro's range error at broken.inc:4 with a
note at the macro body on line 2. Both flat CPU readers retain the first file
and line but lose expansion_frames for that backend error. The clean preceding
producer and the formatter trial reproduce the same loss.

1. File the measured symptom with native and two actual Asm traces.
2. Trace node/statement placement through flat lowering and backend diagnostics;
   compare the existing linked reader's context preservation. Explain the loss
   before editing. Prefer the existing private placement/source-map seam.
3. Add native-backed direct, included, nested and backend/parser error controls
   with preserved output artifacts. Restore existing frame fields without a
   public schema change; seek agreement if a new architecture is necessary.
4. Run full-slice native/CLI/debug, package, CI and cleanup checks before merge.
