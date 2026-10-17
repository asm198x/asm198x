# DISPLAY formatting (#735)

1. Capture bounded SjASMPlus 1.21.0 executions for formats H/D/A/B/C,
   legacy L/T, case, option reset, quoted commas/escapes, signed/width values,
   forward/local/PC expressions, conditional and macro contexts, invalid options
   and malformed operands. Retain probe.py and fixture JSON.
2. Add failing native byte/note differential and formatter invariants in
   crates/asm198x/tests/sjasmplus_display_format.rs, plus CLI include diagnostics
   and preservation of all five output artifacts.
3. Correct DISPLAY parsing in crates/asm198x/src/dialects/sjasmplus.rs.
   Evaluate expressions from the existing SjasmEval walker in z80.rs using its
   current PC, scope and three-pass forward resolver. Keep Operation::Diagnose,
   AST and human/JSON contract shapes unchanged. Add a mandatory fixture-backed
   candidate package case and a contract regression control in scripts/.
4. Run focused/workspace Lua tests, Clippy, formatting, docs, Ruff and Node.
   Commit a clean producer; replay the unchanged pinned SpecNext target through
   the explicit ecosystem audit and retain the complete report/raw streams.
5. Push a focused PR; verify every exact-head check/workflow and all five actual
   host qualification reports including negative controls; squash merge; update
   #735, preserve producer refs and remove owned worktree/cache/temp resources.

Owned worktree: Asm198x/asm198x-display-format. Temporary paths have prefix
/private/tmp/asm198x-display-format-. Original checkout stays unchanged.
No dependency, public API/schema or new architectural pattern is proposed.
