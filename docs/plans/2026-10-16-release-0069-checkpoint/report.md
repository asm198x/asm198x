# 0.0.69 native release checkpoint

The corrected source-to-public-archive checkpoint passed on all five native
targets. The actual tag-triggered candidate run and a separate fresh-public-
download run each passed all 28 required cases: 280 passing case results across
ten native executions. Every candidate/public archive hash, executable hash and
producer receipt agrees. Both workflows completed successfully; hosting started
only after all five candidate qualifications completed.

- Release: [asm198x-v0.0.69](https://github.com/asm198x/asm198x/releases/tag/asm198x-v0.0.69)
- Source: `7f4d65255dab43abb7c485fbd45212526f908bcb`
- Committed Cargo.lock SHA-256: `2a5b6ea0c33464cb57953b68d102fa244fd8f224bb772dcff4523053559732b7`
- Candidate/publication: [38908454874](https://github.com/asm198x/asm198x/actions/runs/38908454874)
- Published downloads: [38909574281](https://github.com/asm198x/asm198x/actions/runs/38909574281)

## What this delivers

#609's configured ca65 area can use all 32 KiB, including vectors embedded in
CODE or a renamed segment. Real ROM/RAM overflow and actual segment overlaps
are refused before emission can truncate bytes. The default NROM vector
reservation and helpful diagnostic are retained. The source tests independently
compared sixteen extent cases with ca65/ld65 V2.18: eight complete-image matches
and eight native refusals.

A subsequent native probe caught #704: an absent segment in a supplied config
must not reserve its pinned start or alignment, while an explicitly selected
empty segment still participates in placement. The correction landed through
[#705](https://github.com/asm198x/asm198x/pull/705), after all 32 exact-head checks
and five triggered workflows passed or were appropriately skipped. Its source
regressions passed the Lua-enabled workspace suite (1,456 passed, zero failed,
37 ignored), strict Clippy, formatting and generated-documentation checks.

The release also delivers #634's native Pasmo/PasmoNext word arithmetic from
[#701](https://github.com/asm198x/asm198x/pull/701). Seven retained native arithmetic
facts execute against both frontends in the package gate. Other new package
checks compare full configured ca65 payloads and prove that failed layouts
preserve previous binary, symbols, listing and debug files.

[Release PR #706](https://github.com/asm198x/asm198x/pull/706) merged only after all
36 exact-head checks and seven workflows completed successfully or were
appropriately skipped. Its checked tree equals the release commit's tree.
The resolved annotated tag and committed lock digest were independently read
from Git. Both pre-merge snapshots are retained here.

## Native archive identities

| Target | Candidate/public archive SHA-256 | Cases per phase |
|---|---|---|
| aarch64-apple-darwin | `1e58746efe2dce1d4dc5a9e999500cb421ed027b25d88ab315eaa28dfa3249e2` | 28 passed |
| x86_64-apple-darwin | `3144c1718dc2e245521cef063167aaff67802fc4124d479fccb75db47279040a` | 28 passed |
| aarch64-unknown-linux-gnu | `ad5a0f1fe17ba9689805474529ed73a8a70b22b87b09e13defd2475f4c5b147e` | 28 passed |
| x86_64-unknown-linux-gnu | `9654634a3690718e81a38c7a5f5cfe33f647e018733faaaf9ba1ed9b9d3d9d26` | 28 passed |
| x86_64-pc-windows-msvc | `02937fa16cfee44c59628740fce3044dc76650a3f2bf7fc2cb786e26646eff4a` | 28 passed |

The retained public release API response binds archive and receipt digests to
uploaded assets. Each phase retains its actual plan, acquisition record,
qualification report and wrong-byte/corrupt-archive rejection results. All twenty
corruption controls refused their mutations. A fresh public 0.0.67 Mac ARM
control passed the older 24 cases and failed exactly the four newly required
arithmetic/layout cases. Historical 0.0.67 evidence remains unchanged.

Run `node docs/plans/2026-10-16-release-0069-checkpoint/verify.cjs` to check the
frozen 28-case cohort, retained-file hashes, native hosts, publication ordering,
source/lock receipts and candidate/public identity. `verify-controls.cjs` also
rejects seven missing or rehashed evidence mutations, including deletion of the
new absent-segment case. This verifier is independent of future gate additions.

The receipts establish producer source/lock agreement and artifact identity.
They are unsigned and explicitly report an unknown compiled dependency graph;
this is not a signed build attestation or a coordinated family version-set
promotion.

## Interrupted version and remaining scope

Native publication for 0.0.68 was cancelled after finding #704. Its immutable
crate/tag remain; that version is not claimed as a qualified public native
release. The cancelled timeline and reasoning remain in
[`../2026-10-16-ca65-absent-segments/report.md`](../2026-10-16-ca65-absent-segments/report.md)
and the interrupted 0.0.68 checkpoint report.

This completes this bounded release slice. #640 remains open for further
ecosystem work, including #629/#631, #635, #696 and #618. It is not a claim of
complete dialect compatibility or completion of the separate emulator work.
