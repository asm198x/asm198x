# Actual GitHub ecosystem checkpoint

PR [#720](https://github.com/asm198x/asm198x/pull/720) merged after 33 checks and all five triggered workflows finished successfully or skipped at exact head `5b97360f5df3612721c18142a01e40718046eec8`. `implementation-premerge.json` retains that snapshot. Linux CI built PR merge ref `7bdd621757b391d9598eb639d253a4e49bcc9648`; its parent includes the final branch head. The downloaded report has eight native-qualified targets: six exact matches and two known Asm rejections, with the original native capture date preserved.

The manually dispatched [native run 38932324538](https://github.com/asm198x/asm198x/actions/runs/38932324538) built merged main revision `21464e98864f673fd586c46260c4046c03662fcd`. It retained all ten targets: six exact final-artifact matches, two Asm rejections (#716/#717), and two native-control failures (#718). The native image used reviewed RGBDS 1.0.3 and vasm 2.0f; the earlier local capture used 1.0.4 and 2.0b. Each observation retains its own executable hashes and version probes.

The capture step exited 1 with:

```text
xtask ecosystem: audit contains non-equivalent or unavailable targets; inspect report.json
```

The failed controls report `ca65: Invalid CPU: '6502dtv'` and `ca65: Invalid CPU: '65ce02'`. Asm198x was not run for those targets. Complete-result validation, table rendering and artifact upload all succeeded after that failure. Both uploaded tables match local regeneration byte for byte. This is a completed audit with explicit failed controls, not an all-compatible claim.

`isolated-native-probe.json` also retains a real complete HelloAmi audit at the final implementation head. A direct vasm probe first demonstrated its `a.out` side effect in a fresh temporary directory. The runner's probe ran in its own disposable directory; an existing ignored sentinel in the task checkout survived unchanged, the directory was removed, and both complete 244-byte executables still matched.

Run the frozen verifier and its controls without native tools or the evolving xtask:

```sh
node docs/plans/2026-10-16-ecosystem-execution/github-checkpoint/verify.cjs
node docs/plans/2026-10-16-ecosystem-execution/github-checkpoint/verify-controls.cjs
```

The file manifest covers retained evidence, notices, snapshots and verifier sources. Seven controls reject a missing report and rehashed substitutions of control outcomes, final bytes, target membership, native steps, producer identity and the sentinel hash. These records are inspectable source/build evidence; their file hashes are not signed attestations.
