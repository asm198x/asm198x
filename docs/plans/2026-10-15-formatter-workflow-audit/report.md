# Formatter and command-mode audit, 2026-10-15

Three reproducible findings are filed under #641. This is a macOS source-build slice after the output-safety fixes, not a release/host campaign or an audit of every dialect.

| Finding | Observed result | Follow-up |
| --- | --- | --- |
| Formatter comment loss | ACME conditional/else, FOR and post-tested DO delimiters lose comments; sjasmplus DUP loses opener/closer comments | [#656](https://github.com/asm198x/asm198x/issues/656) |
| Command-line constant provenance | Each Pasmo `--equ` shifts root diagnostic, debug-line and cycle-line numbers by one | [#657](https://github.com/asm198x/asm198x/issues/657) |
| Incompatible formatter options | `fmt` accepts --prg, --tapbas, --exe and --gb-rom, then replaces the destination with source text | [#658](https://github.com/asm198x/asm198x/issues/658) |

I recommend fixing #657 first: it affects valid multi-target builds and both successful and failing source locations. #656 loses user comments during valid formatting. #658 concerns invalid command combinations that should fail before writing. The fixes should retain existing formatter, loader and diagnostic boundaries.

## Identity and method

- Source: `b2835c87886f76fdb4686901c7ed1aeb930c826a`, clean production tree after #653–655. The audit adds only this evidence folder.
- Build: `cargo build --locked -p asm198x --bin asm198x`, default features, Rust 1.98.0, two build jobs, external Cargo target directory. [Build log](build.log).
- CLI reports `asm198x v0.0.65`; this is a source build of the stated commit, not a downloaded 0.0.65 release.
- Binary SHA-256: `eb473a460793d44f8b6571ab47646b398a3a553dc9a7705e7f3c352095567d5b`.
- Host: macOS 27 arm64; Python 3.14.3. Exact host identity is in each run's JSON.
- [Probe](probe.py), [first run](run-1.json), [second run](run-2.json). Fresh directories per case, no network or external reference tools, 15-second subprocess limits. Both runs match exactly.

Each run records 23 cases: 14 violate at least one checked invariant; nine pass all their checks. Across both runs that is 46 case outcomes. These are audit cases, not 46 passing tests. Cases contain multiple CLI invocations; full argv, status, stdout, stderr, source and relevant artifacts are retained in JSON.

Successful assembly/format controls are mandatory: the probe raises if they fail, before reporting a suspected defect. A malformed Game Boy fixture initially failed this gate because it lacked a complete header; it was corrected before either retained run. No findings rely on that invalid fixture. The probe exits successfully when evidence collection succeeds, even when it records violations; read `summary` and each case's `violations`.

## Findings and controls

For #656, all four failing samples still assemble to byte-identical output and remain idempotent. Only the comment-preservation invariant fails. ACME zone/macro blocks and sjasmplus IF preserve their marked comments. The ACME parser drops closer comments when returning its Closer; the shared emitter reconstructs delimiters. The Z80 repeat builder discards trailing trivia. This traces the observed loss without claiming all structured constructs are affected.

For #657, a two-line invalid source reports line 2 with no definitions, line 3 with one and line 4 with two, in human and JSON reporting. All reject and preserve the seeded previous image. A valid two-line source still emits bytes `[62, 1]`, while its JSON debug/cycle rows similarly move from 2 to 3 or 4. `apply_equ_definitions` prepends synthetic source lines without preserving the original root-line identity. Include-file, macro and synthetic-definition error provenance are follow-up regression requirements, not measured results here.

For #658, each of the four sources independently assembles with its requested container and formats without that flag. Combining the flag with `fmt` returns 0 and replaces seeded destination bytes with formatted text. Existing `fmt --sym` and `fmt --equ` validation rejects and preserves the destination. An unterminated ACME conditional also rejects without replacing the destination. Other container spellings are listed as acceptance coverage in #658, not claimed as observed here.

## Verification and limits

Ruff formatting and lint checks pass for the probe. `cargo xtask docs --check` passes: nine generated blocks across 64 pages and 47 generated pages are current; all 63 navigation pages exist. [Docs log](docs.log). No production Rust changes were made, so this batch does not claim a fresh full workspace test run.

No duplicate open issue covered these symptoms; searches also checked closed issues, including #405's original command-line definition request. This batch does not retest the earlier output-safety matrix, install packages, run Windows/Linux, compare reference assemblers, or extend the A500 performance work. #641 and #642 remain open for their broader acceptance.

## Re-run

From a checkout with the same production source:

```sh
CARGO_TARGET_DIR=/tmp/asm198x-audit-target cargo build --locked -p asm198x --bin asm198x
python3 docs/plans/2026-10-15-formatter-workflow-audit/probe.py \
  --binary /tmp/asm198x-audit-target/debug/asm198x \
  --output /tmp/asm198x-formatter-audit.json
```

The probe records the current checkout's HEAD and supplied binary hash separately. Build from that checkout before interpreting its results. Temporary fixture directories are removed automatically.
