# Quality completion review — 16 October 2026

The assembler has a qualified baseline and substantial regression coverage.
The wider quality programme remains open. Issue counts and passing test counts
do not supply a meaningful percentage for compatibility or workflow completion.

| Scope | Evidence present | Remaining boundary |
| --- | --- | --- |
| Five assembler journeys (#641) | The [joined baseline report](https://github.com/stevehill1981/198x/blob/99cf0c3b184610fb88c6a2af507cf254c1ac14cd/docs/plans/2026-10-15-joined-workflow-audit-evidence/report.md) samples all five with exact identities, results and limits. Follow-on reports cover output safety, formatting, CPU selection, packaging, disassembly and conversion. | Consolidate the acceptance evidence and limits into a close-out review. The baseline's project examples are modern source for historical hardware; it explicitly does not claim an original period archive or execution of upstream build scripts. |
| Defect delivery | Original findings #643–645 and subsequent bounded findings have merged fixes and regressions. #693 is merged through #694. | #695 is the current correction. Other open correctness issues include #602, #603, #609, #629, #631, #634 and #635; these are separate from completing an audit report. |
| Published CLI | Public 0.0.66 was qualified on all five native archive targets with 160 passing cases and 20 rejected corruption controls across candidate/public phases. [Retained report](2026-10-15-post-fix-release-qualification/REPORT.md). | The latest public tag is `asm198x-v0.0.66`, source `35a07893a57cbb42c3a341cc293fc4b713af99f4`. Later source fixes are not thereby released or publicly requalified. [Release PR #680](https://github.com/asm198x/asm198x/pull/680) for 0.0.67 remains open at review time. |
| Real-project compatibility | Three unchanged pinned ACME targets matched fresh native bytes in the initial audit; the package gate includes a pinned complete HelloWorld project. | [#618](https://github.com/asm198x/asm198x/issues/618) still tracks execution of ecosystem build targets and their compatibility ledger. These samples do not establish broad upstream-build compatibility. |
| Wider Asm198x programme (#640) | Independent byte facts, negative controls, safe-output checks, structured diagnostics and cross-platform archive qualification exist. | Banked/linker behavior, remaining reference differences, broader corpus execution and planned developer surfaces still have open work. Opcode arbitration and internal round trips do not prove all source workflows. |
| Joined Emu198x work | The initial report records bounded native/browser observations and controls. | [#1771](https://github.com/emu198x/emu198x/issues/1771), [#1774](https://github.com/emu198x/emu198x/issues/1774) and the A500 frame-budget finding [#1775](https://github.com/emu198x/emu198x/issues/1775) remain open. The retained clock improvement does not close the frame-budget requirement. |

## Recommended finish line

After #695 lands with its agreed unsupported-escape guard and a separate
escape-parity follow-up, make one acceptance and release checkpoint: link each #641 journey to its
evidence, identify what is passed, failed, skipped or unknown, and record the
remaining work under the existing child issues. Review the authored 0.0.67
release and qualify its actual public artifacts before calling the later fixes
delivered to users. Preserve the historical 0.0.66 evidence as historical.

The audit can have a finite close-out while follow-up implementation continues
under #640. Its recorded limits must remain visible; closing it would not mean
every dialect, historical project, installer or emulator host is qualified.
The wider programme should progress against named acceptance milestones rather
than treating a sequence of nearby parser fixes as a completion metric.
