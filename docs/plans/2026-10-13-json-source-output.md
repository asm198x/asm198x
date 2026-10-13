# Honor source-named outputs in JSON mode

Issue: https://github.com/asm198x/asm198x/issues/621

The JSON CLI returns before the human path resolves `requested_output` and
`requested_symbols`. Preserve the existing library result and reuse the existing
path and framing helpers. Plain JSON assembly keeps its no-file default.

1. Add CLI regressions in `crates/asm198x/tests/cli_json.rs`: compare human and
   JSON ACME CBM output and symbol files; exercise source-relative paths, unsafe
   path rejection, CLI overrides, and the plain JSON default. Run them before
   changing production code and retain their failures in the PR evidence.
2. Resolve the source requests in `crates/asm198x/src/main.rs`'s JSON success
   branch, after CLI container selection. Keep raw `AssemblyResult.bytes` and
   JSON stdout unchanged; reuse `source_named_path`, `frame_output`, and
   `write_debug_artifacts`.
3. Probe adjacent vasm `output` and sjasmplus `SAVEBIN`/symbol-map paths. The
   latter already has an explicit test promising no writes in JSON mode; record
   the decision/test discrepancy separately before changing that behavior.
4. Run CLI tests, workspace tests, formatting, and Clippy; push a focused PR.
   Rebase-merge after green CI, coordinated with #620 and its release, then
   follow the normal release-plz and authored-changelog gates.

No dependencies, schema changes, dialect semantics, or library writes.

## Verification

- The three source-output regressions failed before the fix: absent primary
  output, absent symbols, and a successful exit for a disallowed source path.
- The final CLI suite passes 17 tests, including sidecar collision rejection.
- `cargo test --locked --workspace` passes (1,355 passed, 32 ignored); the final
  added collision test also passes in the 17-test CLI run.
- Workspace Clippy with `-D warnings`, `cargo fmt --all --check`, and generated
  documentation checks pass.
- ACME 0.97 independently emits the same plain (`60`), CBM (`008060`), and Apple
  (`0080010060`) bytes as the JSON CLI for the one-instruction fixture.
- The vasm probe succeeds in both reporting modes but writes its named output
  only in human mode before this fix. The regression now compares both files.
- The independently reproduced `SAVEBIN` discrepancy and the existing map-output
  test are tracked in [#623](https://github.com/asm198x/asm198x/issues/623).
