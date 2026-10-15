# Reject unsupported ordinary-disassembly options (#677)

The shared parser accepts assembly/formatting options, then ordinary disassembly
returns before their later validation. A successful `disasm -o old.s input.bin`
therefore leaves old source in place. Follow the existing mode-validation pattern:
reject unsupported output, container, linker, include, definition and diagnostic
options before input reads. Preserve stdout listings, CPU/origin selection and the
separate recovery parser's supported new-file output. No dependency, decoder,
output feature or JSON schema change is needed.

1. Add `crates/asm198x/tests/cli_disasm.rs` with isolated temporary directories.
   Reproduce the accepted ignored options, including aliases and explicit default
   diagnostic/symbol formats. Check errors, empty stdout, source/output preservation
   and option validation before a missing-input read. Run against unchanged code
   and retain the red result in this plan.
2. Change only mode validation/presence tracking in
   `crates/asm198x/src/main.rs`. Preserve the recovery dispatch and valid assembly
   and formatting paths. Run new regressions plus existing CLI, formatting,
   JSON, output-safety and recovery suites, rustfmt, Clippy and generated docs.
3. Push a self-contained conventional-commit PR closing #677. Merge only after
   all triggered checks/workflows finish acceptably at its exact head. Update
   the issue with verified behavior, then remove the worktree and task caches
   after confirming commit retention and inspecting untracked/ignored files.

Verification results will be recorded below before committing.

## Verification

- Before the production change, the final regression suite exited 101: two valid
  listing controls passed and the rejection test failed with
  `["-o", "out.s"]/input.bin: Output { status: ExitStatus(unix_wait_status(0)), stdout: "        *= $8000\n        LDA #$05\n        RTS\n", stderr: "disassembled 3 byte(s) at $8000\n" }`.
  This is the silent-success defect, with a valid baseline in the same run.
- During test construction, the controls were corrected to preserve the normal
  stderr summary and to compare 68000 with the decoder directly: assembly's
  unsupported `org` directive is outside this fix. No production workaround was
  added for either harness assumption.
- After the change, all 173 selected tests passed, with two reference-tool tests
  intentionally ignored. These cover the CLI binary, CPU/dialect routing, EQU,
  JSON, output safety, debug artifacts, formatting and recovery. The three new
  tests include 112 rejection scenarios (28 spellings × two input states × two
  destination states), 6502/Z80 assembly round trips and 68000 decoder agreement
  at default and explicit origins. Existing recovery verifies new-file output,
  reassembly, and refusal to overwrite source/input aliases.
- The first lint run rejected test `unwrap()` calls:
  `error: used unwrap() on a Result value`. They now use descriptive `expect()`
  assertions; the unmodified workspace Clippy gate passes with `-D warnings`.
- Final new regressions, `cargo fmt --all --check`, `git diff --check` and
  `cargo xtask docs --check` pass. Generated docs report nine blocks across 64
  pages and 47 generated pages current, with all 63 navigation pages present.

The fix adds no dependency and changes no decoder, JSON schema, formatting or
recovery implementation. The prior audit's 0.0.66 observations stay historical;
this plan records verification of the source correction.
