# Preserve recorded PasmoNext metadata on Windows

## Cause and audit correction

Candidate 1079 at a6b4f19a passed the previous native fixes and explanation
checks, then `pasmonext_measured::measured_programs_match` failed at line 366.
Six cases produced identical bytes but disagreed on the recorded third-pass
flag. The fixture parser splits at `\n=== ` before calling Rust `str::lines`.
On CRLF input, splitting consumes LF but leaves the preceding CR at the end of
each case. Rust retains that final standalone CR; the exact `--- passes 3`
comparison then misses it.

The preceding audit wrongly excluded this fixture using Python `splitlines`,
which removes standalone CR. That control did not model the actual Rust parser.
This correction uses the unchanged Rust test against real CRLF and LF checkouts.
No fixture baseline, pass-flag invariant, test or production parser changes.

## Focused comparable-consumer audit

Six literal-newline splitting sites exist in crates/xtask:

- `tests/pasmonext_measured.rs:224`: this recorded-text delimiter/metadata seam.
- `tests/sjasmplus_display_format.rs:44`: decoded JSON stderr; checkout CRLF is
  JSON whitespace and does not change escaped newline string values.
- `tests/debug_cli.rs:1005`: newly emitted CLI listing, not checkout text.
- `tests/conformance.rs:167`: Rust-generated disassembly, not checkout text.
- `xtask/src/evidence.rs:115`: generated prose, then whitespace tokenization.
- `src/dialects/z80.rs:804`: split_inclusive retains source slices for Lua;
  delimiter words use whitespace tokenization rather than exact metadata lines.

The last four do not consume the recorded metadata or discard its LF delimiter.
No further same-mechanism fixture rule is proposed.

## Plan

1. Remove the verified task-checkout copy first so an up-to-date index stat
   cannot avoid a rewrite. Before attributes, use `git -c core.autocrlf=true checkout-index --force` on
   `crates/asm198x/tests/fixtures/pasmonext-0.1.3-programs.txt`; verify real CRLF
   bytes differ from Git. Run the existing Rust measured-programs test and
   require the six recorded third-pass mismatches to reproduce.
2. Add only that file's `text eol=lf` rule to `.gitattributes`. Repeat the same
   checkout, require exact Git-blob bytes, and run all PasmoNext measured tests.
3. Exclusion control: simulate CRLF checkout of the parsed JSON fixture
   `sjasmplus-display-format.json` and run its actual Rust consumer unchanged.
   Restore its LF bytes afterward; do not add a rule for it.
4. Run diff checks, record results, commit/push and request parent review of the
   exact patch before merge. Require every applicable current-head PR check;
   only then refresh the release and require the complete native suite.
5. Verify remote retention and remove task worktree/cache and temporary files.

Evidence: https://github.com/asm198x/asm198x/actions/runs/42397564440/job/128478300820

## Actual Rust results

The verified CRLF fixture reproduced the exact six Windows mismatches through
`measured_programs_match` (exit 101), including identical byte values and the
missing third-pass flag. After the single LF rule, the identical checkout
command preserved exact Git-blob bytes and all 23 PasmoNext measured tests
passed. The pass-flag assertion and fixture contents are unchanged.

Actual Rust exclusion controls also passed with verified CRLF checkout bytes:
two SjASMPlus display tests consumed the JSON, seven CLI CPU tests consumed the
line-oriented CLI Markdown, and twelve recovery tests consumed the palette
source (one existing reference-ACME test remains ignored). Together 44 tests
passed. The previously excluded JSON alias fixture already passes its actual
Rust consumers in the linked Windows run. This replaces the inadequate Python
splitlines proxy with actual consumer evidence for every exclusion category.

All simulated fixture/document copies were restored to their Git-blob bytes.
`git diff --check` passed; the only tracked edits are the one-file attribute
rule and this plan. No baseline was refreshed.
