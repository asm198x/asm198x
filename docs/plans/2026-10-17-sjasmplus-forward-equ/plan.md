# SjASMPlus forward EQU (#742)

1. Capture bounded SjASMPlus 1.21.0 runs for derived forward constants, chains,
   PC/local/module/include bindings, arithmetic, missing symbols and cycles.
   Retain sources, exit status, diagnostics and exact output in a test fixture.
2. Add failing differential/formatter and CLI preservation tests in
   `crates/asm198x/tests/sjasmplus_forward_equ.rs`. Explain the failure in this
   folder before changing `crates/asm198x/src/dialects/z80.rs`. Reuse its existing
   three-pass resolver; keep other dialects and final missing-symbol errors.
3. Require fixture replay in `scripts/test-candidate-package.cjs` and test the
   qualification requirement in `scripts/candidate-package.test.cjs`.
4. Run workspace Lua tests, Clippy, formatter/docs checks, probe lint and Node
   checks. Commit a clean producer; rerun the unchanged pinned SpecNext target
   with `cargo xtask ecosystem audit`, retaining complete results and streams.
5. Push a PR, verify every exact-head workflow and all five downloaded package
   reports plus negative controls, squash merge, retain producer refs, and
   remove task worktrees/builds/temporary sources. File a subsequent boundary
   separately if the unchanged target still fails.
