# Reject ignored CLI CPU selections (#608)

The resolver currently considers non-Z80 CPU names only when no explicit dialect
is present. Consequently pasmo accepts both `--cpu 68000` and `--cpu nonsense`
and emits Z80 instructions. Validate the target independently, then check that
the chosen frontend can apply it. Preserve CPU-only dialect aliases, existing
Z80/Z80N selection and lexical source directives; add no new target capabilities.

1. Add `crates/asm198x/tests/cli_cpu.rs`: reproduce both reports before changing
   production code. Exercise human/JSON errors, output preservation, all documented
   dialect aliases, compatible/incompatible pairs, Z80N reference bytes and lexical
   ACME CPU switches. Use the published CLI matrix as the compatibility fixture.
2. Update `crates/asm198x/src/main.rs`: separate dialect lookup from CPU resolution;
   reject unknown/incompatible targets before input/output processing; use existing
   JSON diagnostics. Share resolution across assembly, formatting and disassembly.
   Recovery already rejects unsupported selectors and needs no change.
3. Publish the supported CLI CPU/dialect matrix in
   `docs/book/src/reference/cli.md`, distinguishing initial CLI targets from lexical
   source directives. Update help and add the regression target to the existing
   cross-platform Lua job in `.github/workflows/ci.yml`.
4. Verify the regressions fail on the old resolver and pass after the fix. Run
   workspace tests, strict Clippy, formatting, generated-doc checks and the locked
   standalone web check with two build jobs and an external Cargo target directory.
5. Commit, push, open a PR closing #608, wait for all CI results, squash-merge the
   tested head, verify its tree on main, record the result on audit #641, and remove
   this task's worktree, build cache and scratch evidence after preserving commits.

## Evidence

On base `66f7c80aa4f2eaa2117ab9f1617d0066a8eb349e`, both issue commands
(`--dialect pasmo --cpu 68000` and `--cpu nonsense`) exited 0 and emitted
`00`, as did the valid `--cpu z80` control. The new artifact regression failed
before the resolver change with `ignored --cpu nonsense` and a success summary
showing that the binary, debug sidecar, symbols and listing had all been written.

After the change, all five CLI regression tests pass. The matrix test reads the
published 24-row table, requires exactly one row per supported dialect, checks
every documented alias, and tests each dialect against every row's default CPU.
The opcode test compares Z80N `mul d,e` with the saved SjASMPlus verdict `ED30`
and requires base Z80 to reject it. The ACME test compares CLI bytes with library
assembly and verifies switching back removes the extension.

Local verification: `cargo test --workspace` passed (1,402 passed, zero failed,
33 ignored); `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo fmt --all --check`, `cargo xtask docs --check`, and the locked
`asm198x-web` check for `wasm32-unknown-unknown` all passed.
