# Write source-requested artifacts in JSON mode

Issue: https://github.com/asm198x/asm198x/issues/623
Depends on the source-named output fix in #624.

The verified `SAVEBIN` omission matches the symbol-map test's current no-write
assertions but contradicts `decisions/multi-artifact-output.md`. The user chose
to honor source directives in JSON mode and update the conflicting test.

1. Update `crates/asm198x/tests/sjasm_symbol_exports.rs` to compare actual human
   and JSON map files. Add CLI artifact tests for output-directory selection,
   `--outprefix`, absolute/parent paths, write failure, and no default image.
   Run them red before changing the implementation.
2. Pass `--outprefix` to `emit_json` and reuse `write_artifacts` in
   `crates/asm198x/src/main.rs`. Keep the library pure and result bytes raw.
   Record resolved successful writes using the existing note representation.
3. Clarify JSON behavior in `docs/book/src/reference/cli.md` and the binding
   decision. Run CLI and symbol-export regressions, workspace tests, Clippy,
   formatting and documentation checks. Open a separate PR, then coordinate
   normal green merge/release gates with #624 and the current release.

SAVE paths retain their existing policy: source-relative default output
directory, CLI-selected output directory or `--outprefix`, and accepted absolute
and parent paths. This differs deliberately from ACME's confined `!to` names.
No dependencies, new reporting schema, or filesystem writes in the library.

## Verification findings

- The updated symbol-map test fails before the JSON write is wired in.
- The path matrix also exposes a human-mode omission: `--tapbas` skips the
  SAVE write because the call sits inside the raw-image branch. Moving that
  same call after primary-image selection fixes both reporting modes.
- The focused run passes all 19 CLI tests and four symbol-export tests; three
  external-consumer tests remain ignored in that run. The installed SjASMPlus
  1.21.0 reference comparison is then run explicitly and passes its one test.
- Workspace Clippy, formatting, and generated documentation checks pass.
- JSON write notes use the existing `Note` kind, not advisory warnings or
  errors. Tests assert success, empty diagnostics, exact note count and actual
  bytes on disk. Library assembly and reference verdicts receive no CLI notes.
