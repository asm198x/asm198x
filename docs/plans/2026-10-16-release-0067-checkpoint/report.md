# Asm198x 0.0.67 acceptance and release checkpoint

This closes the bounded five-journey audit in [#641](https://github.com/asm198x/asm198x/issues/641). It does not close the wider [#640](https://github.com/asm198x/asm198x/issues/640) correctness and compatibility programme. The actual 0.0.67 release archives pass the expanded gate on all five native targets, and downloaded public archives retain exactly the same archive, executable and producer-receipt identities.

## Released artifacts

[Release PR #680](https://github.com/asm198x/asm198x/pull/680) was squash-merged after all 37 exact-head checks and all seven triggered workflows completed acceptably. Source/tag: `46cedb445a01f8fefd9d2a96d63961414036fcf4` / `asm198x-v0.0.67`. Committed Cargo.lock SHA-256: `a249badd5d4855f1071caf88b54ab8faaeb739fa5605de256319c534903009de`.

- [Actual release run 38887216141](https://github.com/asm198x/asm198x/actions/runs/38887216141): five native archives pass 24 cases each; ten deliberate corruptions reject. Hosting starts after every native qualification succeeds. Release and Homebrew publication complete successfully. [Published release](https://github.com/asm198x/asm198x/releases/tag/asm198x-v0.0.67) timestamp: 2026-10-16 10:33:22 UTC.
- [Downloaded-public run 38888025382](https://github.com/asm198x/asm198x/actions/runs/38888025382): five native targets pass the same 24 cases without rebuilding; ten deliberate corruptions reject. A concurrently dispatched run 38888017733 was superseded by workflow concurrency and supplies no passing evidence here.
- Total across retained candidate/public phases: **240 passing cases and 20 rejected corruption controls**. All five candidate/public archive hashes, executable hashes and complete producer receipts agree. Published archive and receipt digests also match GitHub asset metadata in `release.json`.

`candidate/` and `published/` retain actual command/stream/exit reports, release plans, acquisition records and both controls. Producer receipts are embedded in each report: source commit, committed/before/after lock hashes, target/version, archive/executable hashes and producer job/run/attempt. These are unsigned producer-lock evidence; the compiled dependency/features graph and signing identity remain **unknown**. This is Asm198x archive qualification, not promotion of a joined umbrella version set.

The eight added cases cover ordinary disassembly option refusal; a 68000 branch at `$FF8000` targeting `$FF8004`; nested Pasmo repetition; independently recorded native bytes for quoted data in both diagnostic modes; formatter idempotence/byte preservation; conversion byte preservation; explicit single-source conversion limits; and safe refusal of unsupported SjASMPlus string decoding. Native byte facts come from the pinned fixture records in the release source. They are archived reference facts, not fresh native-assembler execution on each CI host.

`older-public-control.json` records the actual macOS ARM64 0.0.66 public archive: its original 16 cases pass and all eight new cases fail. The unit gate also rejects a previously qualified 16-case report. This prevents an older green cohort from substituting for the new requirements. The first local packaging invocation used the wrong output directory; the corrected invocation and full CI produced the qualifying archives. The initial NOP-only origin assertion was replaced with a relative branch so accepted-but-truncated origins cannot pass.

| Native target | Archive SHA-256 | Executable SHA-256 |
| --- | --- | --- |
| aarch64-apple-darwin | `edc7a74b7394ea00c47a4109b3bb8f0e492244006d91025a4e22cf5720e5fb51` | `cf169ea78f9000867dd08dc5e25b4017badbb2e8f2d60df46e6dae398548b941` |
| aarch64-unknown-linux-gnu | `2741acd2f5061ce4c717eaa1dd0685c40e70bec714b7c2d0be664f0e9d5c9b92` | `bce03f0ef1ece0588693137b1c2d4846572962e17ab733e7a2baa0110d93e068` |
| x86_64-apple-darwin | `d1588919b26dfbb52d2db9cedfaa1ab1629dc1f560f3a321bcc5eaa66e5055fd` | `366aee2c3818bb935364b36dd5bbf1f0640dc87304e0ed51011897253a0b459f` |
| x86_64-pc-windows-msvc | `fcc67b3abac24f03b898e02d39558c09f656fafe0206187c87cb64822b4de643` | `9d4142e94d0135c35870066eeeb4a1cb15290f7ddd35ffe65ccd77603cb547a4` |
| x86_64-unknown-linux-gnu | `1641899945f3d8dcd99ef88f0b8fffd220f142a03f8894dec45de3d31a2e0789` | `d47e7ec161e18c864a11114739d3c617040a4a8218c156c3e9858c43d3faffa6` |

Run `node docs/plans/2026-10-16-release-0067-checkpoint/verify.cjs` to verify retained-file hashes, exact case sets, host identity, timeline ordering, producer-lock/asset bindings and candidate/public equality. Run `node docs/plans/2026-10-16-release-0067-checkpoint/verify-controls.cjs` for six failing mutations: missing report, unavailable case, missing receipt, changed lock, substituted public executable and missing manifest entry. Mutations update affected file hashes, so checksum checks alone cannot supply the required rejection.

## Audit acceptance

The [initial joined report](https://github.com/stevehill1981/198x/blob/99cf0c3b184610fb88c6a2af507cf254c1ac14cd/docs/plans/2026-10-15-joined-workflow-audit-evidence/report.md) retains all five journeys, source/tool/environment identities, raw probes and repeated failures. Its downloaded Asm198x 0.0.65 executable is a different identity from later source builds and releases. The report distinguishes pass, mixed, not run and unknown; later fixes do not rewrite its historical results.

| Required journey | Recorded outcome and limits | Follow-through |
| --- | --- | --- |
| Initial setup | Downloaded macOS ARM64 archive extraction, version/help and first assembly pass. Installer, upgrade, PATH and Gatekeeper were not qualified. | Later native candidate/public qualification covers archive execution on five targets, not installation UX. |
| Real historical project | Three unchanged pinned modern ACME projects for historical hardware match fresh native bytes. An original period source archive and upstream build scripts were not run. | The complete pinned HelloWorld project remains in the archive gate. Broader actual ecosystem builds remain [#618](https://github.com/asm198x/asm198x/issues/618). |
| Invalid syntax/operand | Oversized operand rejected with source location in both modes; previous output preserved. Required sidecar failures exposed #643, JSON I/O exposed #644. | Fixed through #646/#648; later output-safety and archive controls cover these boundaries. |
| Macro/include error | Missing include names its includer; invalid included macro reports definition/invocation. Primary spans remain line-granular, with no universal suggestion claim. | Follow-on conversion audit found #686/#687; fixed through #692/#689. The archive gate exercises nested repetition and explicit conversion file-scope refusal. |
| Banked placement/symbol diagnosis | Valid Spectrum128 page changes preserve page-qualified symbols. Invalid PAGE rejection lost navigation (#645). General banked/linker correctness was not established. | #649 fixes source spans. Broader banked/layout correctness stays under #640 and existing children. |

The remaining two acceptance criteria are met by the linked specific findings and this review of findings, limits and next scope. Original findings #643–645 and subsequent bounded audit findings have merged fixes. The approved full SjASMPlus string-escape work remains [#696](https://github.com/asm198x/asm198x/issues/696); this release rejects unsupported decoding explicitly. The earlier [completion review](../2026-10-16-quality-completion-review.md) is a dated assessment, not a current release ledger.

Follow-on evidence includes [SAVE/output safety](../2026-10-15-save-output-workflow-audit/report.md), [release hosts](../2026-10-15-release-host-audit/REPORT.md), [ordinary disassembly](../2026-10-15-disassembly-workflow-audit/report.md) and [conversion](../2026-10-16-conversion-workflow-audit/report.md). Their findings remain tied to the versions actually observed.

## Remaining work and next bounded step

Address known correctness defects before opening another broad discovery batch. The retained open set includes ACME #602/#603, ca65 layout #609, Pasmo #629/#631/#634, SjASMPlus operators #635 and escape parity #696. Verify each report against current source and native facts before choosing its implementation scope. #618 owns actual ecosystem build execution and its compatibility ledger. No percentage of general dialect compatibility follows from these samples.

Joined Emu198x [#1771](https://github.com/emu198x/emu198x/issues/1771), [#1774](https://github.com/emu198x/emu198x/issues/1774) and [#1775](https://github.com/emu198x/emu198x/issues/1775) remain open. The retained clock improvement does not meet the A500 frame budget. No desktop/browser support level is promoted here.
