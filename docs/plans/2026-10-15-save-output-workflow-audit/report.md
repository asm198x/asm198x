# SAVE placement and reporting audit

All 32 candidate observations pass the scoped invariants. This batch found no
new defect to file. It does not complete #641 or cross-platform compatibility.

The optimized macOS ARM64 CLI is built from #671's tested source commit
`8f3813f861debf658f5329f1f8f922cb6e5b82d3` (squashed to `30d13bf` on main).
Its version remains 0.0.65; it is a source candidate, not the public 0.0.65
binary. `observations.json` records exact binary hashes, version commands,
host, arguments, exits, stdout/stderr and every produced file's bytes.
The independent installed reference is SjASMPlus 1.21.0.

Eight workflows each run twice in human and JSON mode, in fresh directories:
relative names, parent names, absolute names, relative/absolute prefixes, a
prefix naming an absent directory, a missing SAVE parent and two SAVE requests.
All subprocesses have a 30-second bound. A parent-path fixture remains within
its own disposable container. Reference work-time text varies; repeat agreement
normalizes only that timing field while retaining raw observations.

- Successful writes have expected independent bytes: `2a` for the first SAVE,
  `2b2c` for the second, and `2a2b2c` for the complete primary image.
- Each SAVE note resolves to an actual file with those bytes. Human and JSON
  modes produce the same files. JSON retains all three machine-code bytes and
  describes each SAVE separately; notes have kind `Note`.
- Missing destinations return nonzero. Both reporting modes preserve the
  seeded previous primary image; JSON emits an Error array with empty stderr.
- Prefix and absolute-name behavior agrees with the reference in these cases.
  With a different primary output directory, unprefixed relative names resolve
  beside that output in Asm198x, while native SjASMPlus resolves at its working
  directory. This follows the binding `multi-artifact-output.md` choice for
  Asm198x; it is recorded as a policy difference, not an unapproved fix.
- Native SjASMPlus replaces its raw primary before the invalid SAVE failure.
  Asm198x's earlier destination preflight preserves it as intended by #643.

The probe asserts a nonempty exact observation count, repeat agreement, seeded
output preservation, actual bytes, structured result shape and file-note
agreement. Ruff and generated documentation checks pass. The initial probe
checks caught platform path canonicalization and primary-summary handling in
the audit harness; no product fix was needed.

Limits: local macOS ARM64 source build only; SAVEBIN rather than every SAVE
container; no concurrent filesystem replacement or transactional-I/O claim.
Windows/Linux candidate package qualification and post-publication verification
are tracked separately by #668. Future multi-artifact work must consult the
same binding decision before changing allowed path behavior.

Rerun with Python 3.13+:

```sh
python3 docs/plans/2026-10-15-save-output-workflow-audit/probe.py /absolute/path/to/asm198x /absolute/path/to/sjasmplus /tmp/save-observations.json
```
