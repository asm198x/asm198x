# Corrected local qualification for #914

The frozen producer is 499a58a29015f79a0b3897572db1e1ba9dc816c8, CLI SHA-256
1d5c767f35ac32e5acdb3543db9a06cc320802d194bf99b83c72f70a333afc2a.
All 1,638 Rust/Lua tests pass (38 ignored); all 115 Node tests, all-target Clippy,
rustfmt, Ruff and generated documentation/version checks pass.

All prior native matches survive across 2,194 retained cases. The 314-case
classifier audit fixes 161 wrong-byte outputs and retains one unchanged #795
undefined-query wrong acceptance, named in permanent fixture metadata. The
other source/text matrices retain their explicit pre-existing gaps.

All 100 local archive cohorts pass, including the 313-case/1,228-command source
constancy cohort. The previous archive fails only that cohort. Wrong bytes fail
exactly the reference-bytes, determinism and structured-success controls;
checksum corruption is rejected before execution. Local checks are artifact-only
and do not claim the producer-lock receipts required from actual CI archives.

A fresh release-mode 4510 build matches all 564 native bytes and fourteen
streams, rejects changed bytes, and removes its disposable work directory.
Complete streams, native replays, process identities and archives' hashes are
retained here. Exact-head CI and all five native reports remain required.
