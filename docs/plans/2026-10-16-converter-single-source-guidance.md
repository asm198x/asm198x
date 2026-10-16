# Correct converter file-resolution guidance (#686)

Conversion verifies two source strings with the single-source assembler APIs.
It has no loader. Existing INCLUDE/INCBIN files therefore fail verification,
but the CLI reference promises include conversion and the underlying error
tells a conversion CLI user that files are resolved automatically.

1. Reproduce existing-file INCLUDE and INCBIN failures with both source dialects.
   Add library/CLI regressions to `crates/asm198x/tests/convert.rs` proving the
   wrong advice, accurate source locations, empty stdout, and preserved output.
   Exercise ordinary CLI assembly as the positive file-resolution control.
2. Adapt only the converter's input-verification error context in
   `crates/asm198x/src/convert.rs`. Retain the underlying unresolved request,
   code and span, replacing the single-source API's ordinary-CLI advice with
   the converter limitation and a self-contained-source action. Check dormant
   directives are not rejected by an eager source scan.
3. Correct `docs/book/src/reference/cli.md` and the public `convert` API docs:
   conversion is single-source; macros convert only when both verifiers agree.
   No multi-file migration feature or change to ordinary assembly diagnostics.
4. Run converter and nested-macro regression tests, strict Clippy, rustfmt and
   generated documentation checks. Push a separate PR; verify all exact-head
   CI before squash merge. Retain merge evidence and clean temporary files.

The roadmap and I/O decisions keep this a correction to the existing converter
boundary. No new dependencies, public error shape, filesystem loader or AST
preflight validation is needed.

## Verification

The library and CLI regressions failed before the fix with the single-source
API's incorrect CLI advice. A harness assertion was corrected to use the
converter's existing `input.asm: line 2:` rendering before recording the final
red run. All seven hermetic converter tests and six nested-repetition tests
pass after the correction. The native conversion differential also passes
against PasmoNext v0.1.3 and SjASMPlus 1.21.0.

The positive controls assemble existing text/binary inclusions to `2A C9` in
both ordinary CLI dialects. Failed conversion writes no stdout, preserves the
previous source output and input, and retains the underlying error code/span.
Untaken file directives still verify; no eager AST scan rejects them. Ordinary
single-source APIs keep their existing multi-file-client advice.

Strict workspace/all-target Clippy, rustfmt and generated documentation checks
pass. The generated-doc check requires no generated-page change for this edit.
The correction adapts the existing single-source error hint only at the
converter boundary, without changing its public error shape or loader model.
