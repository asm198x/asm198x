# Dialect-conversion workflow audit

Two findings remain open: [#687](https://github.com/asm198x/asm198x/issues/687)
rejects native-valid Pasmo repetition inside a macro, and
[#686](https://github.com/asm198x/asm198x/issues/686) corrects misleading include
support claims. Fix #687 first because valid source cannot assemble. This audit
lands evidence and issues; it changes no production behavior.

The source-built CLI at `c96f2b89e474675a0f9f8d199907fd7d7f505011` reports
0.0.66, but it is not the public 0.0.66 executable. `observations.json` retains
its executable hash, Cargo.lock hash, compiler, host and exact native reference
identities. The host is macOS ARM64, Python 3.14.3, with PasmoNext v0.1.3 and
SjASMPlus 1.21.0 installed independently of Asm198x. Tracked production files
were clean. No dependency was added.

Twenty cases run twice in fresh directories produce 40 observations:

- 16 successful ordinary conversions, including file/stdout destinations,
  repetition/conditional comments, simple macros and the pasmonext route.
- 12 correct rejection controls: invalid source, unsupported pair/option, missing
  input/include and an invalid output destination. Every rejection has empty
  stdout and unchanged pre-existing files.
- Six observations of explicit in-place, hard-link and symlink output requests.
  They replace the requested source with converted text, and the migrated source
  still matches native bytes. The converter publishes no new-file restriction;
  this is recorded behavior, not a newly invented overwrite-policy violation.
- Four include/INCBIN limitations, despite existing files that ordinary Asm198x
  assembly and native Pasmo both assemble to `2A C9`. The converter calls its
  single-source verifier, while its help tells this CLI user that the CLI resolves
  files automatically. The reference also promises includes convert where the
  dialects agree. #502's accepted v1 scope defers multi-file fidelity, so #686's
  bounded correction is honest documentation and an actionable limitation
  message, not an unplanned multi-file converter.
- Two nested-macro/repetition defects. Native Pasmo accepts the exact fixture and
  emits `00 00`; native SjASMPlus emits the same bytes after changing the inner
  repetition closer to `ENDR`. Asm198x ordinary assembly and conversion both
  reject line 6 as an unmatched `ENDM`. The collector stops at the first closer;
  the source comment claiming the reference refuses nesting is contradicted by
  the retained fresh reference bytes. #687 follows the existing per-dialect
  collector seam and keeps broader converter fidelity separate.

Successful conversion outputs are assembled by both native tools and compared
byte-for-byte, rather than trusting the converter's own verification note.
Next-specific `swapnib` emits `ED 23 C9` when native SjASMPlus is invoked with
`--zxnext`. The same target invocation without that option deliberately fails;
its partial native output is retained as a negative reference control, not
attributed to the converter. This documents the required target CPU setting.

Raw arguments, exits, both streams and all conversion input/output bytes are
retained for every observation; reference output files and processes are retained
alongside them. Native elapsed times vary between runs, so repeat checks compare
CLI outcomes, artifacts and reference bytes rather than elapsed-time text.
Each process is limited to 30 seconds. This is a bounded local source-build slice,
not a new public-release or cross-platform qualification, and it does not cover
all macro forms, include graphs or dialect pairs.

The first fixture used macro name `m`; native Pasmo rejected it, so the mandatory
positive control stopped the probe. It was corrected to `setv` before the retained
runs. A deliberately failing executable is also rejected without producing an
observation file. The evidence verifier rejects an empty observation set even
when its manifest is recomputed. `controls.json` records those actual controls.
Ruff formatting/lint, retained-evidence verification and generated docs pass.

Rerun the probe with Python 3.13+, the recorded native tools on PATH and a freshly
built CLI; its executable hash will reflect the chosen build:

```sh
python3 docs/plans/2026-10-16-conversion-workflow-audit/probe.py /absolute/path/to/asm198x /tmp/conversion-observations.json
python3 docs/plans/2026-10-16-conversion-workflow-audit/verify.py
```

The verifier reads this retained run without requiring those executables. The
broader audit remains [#641](https://github.com/asm198x/asm198x/issues/641).
