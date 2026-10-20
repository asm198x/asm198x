# Five-host qualification for #914

Qualified source: 499a58a29015f79a0b3897572db1e1ba9dc816c8.
Candidate run: 39454564938. All five native hosts passed on attempt one.
Every actual archive passes all 100 cohorts, including 313 source-constancy
cases and 1,228 commands. Independently inspected JSON bytes, refusal results,
idempotent formatting and formatted native bytes agree. Each host rejects wrong
bytes in the exact three required controls and rejects a corrupt archive hash.
All producer-lock receipts match the source, committed lock, run and target.

All 29 exact-head checks and all five workflows completed successfully. #914
was rebase-merged as fa615b0c2f0689deef80c1b64983adbb1bd03844; its tree equals
the qualified source tree, and the merge is retained on origin/main.

Local qualification retains all 1,638 Rust/Lua test passes (38 ignored), 115
Node passes, all-target Clippy/fmt/Ruff/docs checks, all prior native matches
across 2,194 cases, the 100-cohort local archive, predecessor and corruption
controls, and the complete fresh 564-byte/fourteen-stream 4510 replay.

#795 remains open for the live source-expression context, numeric string
argument ordering and final query admission. The classifier fixes 161 wrong-byte
outcomes; its unchanged undefined-query exclusion is explicit in the fixture.
