# Public 0.0.66 qualification

All five public archives pass the same 16 required cases as their release
candidates. Archive and extracted executable SHA-256 hashes match exactly
between the qualified candidates and fresh public downloads. All 20 deliberate
wrong-byte/archive-corruption controls across both phases reject.

- [Release and candidate qualification](https://github.com/asm198x/asm198x/actions/runs/38753688980).
- [Fresh public-download qualification](https://github.com/asm198x/asm198x/actions/runs/38754905144).
- [Public 0.0.66 assets](https://github.com/asm198x/asm198x/releases/tag/asm198x-v0.0.66).

The annotated tag resolves through tag object
`9fbfefe442729ec9d609ecd1fbbe674ab019d02b` to source commit
`35a07893a57cbb42c3a341cc293fc4b713af99f4`. These are actual 0.0.66 archives,
not a version-only assertion about source builds. Both phases execute on native
Windows x86-64, macOS ARM64/x86-64 and Linux ARM64/x86-64.

The final required candidate job completed at 14:48:17 UTC on 2026-10-15; hosting
started at 14:48:20. Homebrew publication and announcement completed successfully.
The retained workflow timeline verifies that ordering. Public qualification
downloaded the released manifest, archive and checksum without rebuilding.
Its acquisition records identify the public version separately from the gate's
generic candidate report label.

The 160 passing cases include executable architecture/version, independent small
assembly bytes, determinism, structured success/failure, previous-output
preservation, missing input, unknown/incompatible CPU rejection, hard-linked
source/sidecar preservation, formatter option rejection, Lua and an unchanged
pinned complete MIT-licensed HelloWorld project compared against independent
ACME bytes. Controls mutate output and archive bytes and require specific
failures. Required unavailable, missing or skipped cases cannot qualify.

`manifest.json` hashes every retained plan, acquisition record, report and
control, plus this report, the verifier and release timeline. The verifier
requires the complete five-target set, all required cases, native identities,
correct plans/versions, expected control failures, publication ordering and
identical candidate/public hashes. Negative controls also reject a removed
report, an unavailable required case after rehashing, and a changed public
executable identity after rehashing.

The verifier freezes the 0.0.66 case contract so later gate additions do not
invalidate historical evidence. Git attributes preserve exact LF bytes across
checkouts; setup CI executes the retained-evidence verifier on all four hosts.

The setup-action workflow now smoke-tests installed 0.0.66 from PATH on its
four supported targets. Its separate historical 0.0.65 installation preserves
the old version-specific observational probe unchanged. The existing setup-node
action moves from v6 to the project's already-used v7. Those setup checks must
pass in this report PR before merge. Linux ARM64 is archive-qualified; expanding
the setup action's four-target contract remains outside this change.

This reconciles the first post-fix public release for #668. The next bounded
SAVE audit is recorded in #673: 32 repeated observations, no new defect. The
broader #641/#640 audits and full compatibility/installer-upgrade coverage stay
open. Runtime I/O races and cross-file transactions are not claimed.

Verify retained evidence from the repository root:

```sh
node docs/plans/2026-10-15-post-fix-release-qualification/verify.cjs
```
