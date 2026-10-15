# Preserve wider 68000 disassembly origins (#678)

The shared CLI parses every `--org` as u16 before selecting the disassembler.
The existing 68000 decoder accepts u32 and already computes wider branch targets.
Change only that boundary; keep ordinary narrow decoders and recovery bounded to
16-bit origins. No decoder, dependency, assembly placement or recovery expansion.

1. Extend `crates/asm198x/tests/cli_disasm.rs` with position-dependent 68000
   branches at low, above-16-bit and wider u32 origins. Compare CLI output with
   the decoder, including aliases and address spellings. Verify narrow targets,
   recovery, malformed addresses and u32 overflow reject before reading input.
   Run the regression against unchanged production code to prove it fails.
2. Update origin parsing/validation in `crates/asm198x/src/main.rs`; preserve the
   existing u16 parser used by recovery. Validate target width before file reads.
   Run CLI/recovery tests, rustfmt, workspace Clippy and generated docs.
3. Push a self-contained PR closing #678, wait for all exact-head checks and
   triggered workflows, squash merge, and verify remote retention. Remove the
   inspected task worktree, build cache and scratch files before completion.

Verification results will be retained below.

## Verification

- Against unchanged production code, the finalized six-test disassembly suite
  exited 101: five controls passed, and the wider-origin regression failed with
  `asm198x: invalid address ` followed by `` `0x10000` ``. The normal low-origin
  branch controls passed before reaching that failure. An initial test typo used
  unsupported CPU alias `m68000`; it was corrected to the existing `m68k` alias
  before this red run. No production alias was added.
- After the fix, all 176 tests across the selected CLI binary, CPU/dialect, EQU,
  JSON, output-safety, debug, formatting and recovery suites pass; two tests
  requiring external reference tools remain intentionally ignored.
- The position-dependent `60 02` branch agrees with the existing decoder in 48
  CLI runs: eight origin spellings/values, three selection forms, and both
  origin/selection orders. This covers omitted/low origins, `$10000`/`0x10000`/
  decimal 65536, `$12345678`, and the decoder's u32 wraparound boundaries. The
  CLI retains the same summary format and does not truncate the supplied value.
- All other documented dialects still reject origins above u16 before reading
  present or missing inputs. Assembly and recovery retain their u16 validation;
  the maximum u16 origin still matches the 6502 decoder. Malformed values and
  u32 overflow fail before reads, even when followed by another valid `--org`.
- `cargo clippy --workspace --all-targets -- -D warnings`, rustfmt, generated
  docs and diff checks pass. The recovery implementation and its u16 parser
  are unchanged. No dependency or executable ISA change is needed.

Task-owned test files are isolated with a dedicated TMPDIR, so cleanup can remove
that directory without touching unrelated shared temporary files. The historical
0.0.66 audit remains unchanged; this correction ships in the next normal release.
