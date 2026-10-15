# Audit distributed release hosts (#642)

Audit release 0.0.65 at `fa06f5617f32bec52ac14f3a25bdd8e13dae9287`,
separating actual release execution from source builds and fixes on main.

1. Inventory all five release archives and published checksums, installation
   paths, shipped CLI/API surfaces, and existing release/setup checks. Record
   host identities and unavailable combinations. Preserve unrelated work.
2. Add a bounded dependency-free Python audit under
   `docs/plans/2026-10-15-release-host-audit/`: checksummed archive acquisition,
   real source fixture plus independent reference bytes, repeated builds, invalid
   inputs, human/JSON diagnostic parity, filesystem/output safety, structured
   assembly and Lua when shipped. Every process has a timeout; record individual
   pass/fail/unavailable results and exact versions/hashes/arguments.
3. Reuse `.github/workflows/setup-action.yml` to install and audit the pinned
   current release on its four native hosted runners, retaining machine-readable
   reports. Audit Linux ARM64 locally in the existing Docker engine. Test the
   shell installer into an isolated task directory. Do not install host tools,
   alter global configuration, infer runtime support from compilation, or treat
   an audit runner's completion as all capability assertions passing.
4. Prove audit controls can detect wrong bytes and invalid checksums; inspect
   every result. File specific new findings or link existing issues and source
   fixes. Publish a host matrix, limits and minimal proposed release gates in the
   audit report. Keep missing claims explicit and coordinate #65/#108/#642.
5. Push the evidence and small existing-workflow extension through a PR. Wait
   for its checks, verify retained report hashes and merge only a green tested
   head. Update audit trackers with bounded conclusions; clean task worktree,
   downloads and task-owned Docker containers.


## Results

All five CLI targets executed the same 19 cases. The four hosted targets recorded
13 passes and six failures each. Local Linux ARM64 recorded 12 passes, the same
six failures, and one unavailable installer check (curl/wget absent). All failed
checks map to four earlier source fixes and the primary-image guard in #670;
no new platform-specific assembler defect
was established. A complete upstream target matched native ACME on every host.
The published npm Z80 package passed seven bounded API checks on the local Mac.

New findings: #667 (unavailable npm package in the README) and #668 (candidate
archive failure/output-safety gates). The reviewed report, host reports, pinned
identities, controls and integrity verifier are retained in the adjacent folder.
The verifier rejects altered counts even if the outer manifest is updated.
Local verification passed 13 setup tests, the installed PATH smoke and its
wrong-version control, byte/checksum corruption controls, Ruff and generated
docs. Hosted Setup action run 38739136853 completed successfully on all four
native runners; capability failures remain explicit and are not qualification.
