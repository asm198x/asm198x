# Keep HuC6280 macro source through canonical formatting (#821)

The assembling path expands macros before its line walker. The formatter asks
for unexpanded source, but the HuC6280 walker then tries to dispatch `.macro`
and its template body as ordinary operations. The 65816 walker already keeps
opaque macro definitions and recognises calls by their case-sensitive names.
Follow that existing family convention; do not expand macros or preserve an
entire file as a formatter fallback.

1. Retain native ca65/ld65 assembly and clean-predecessor assembly/fmt outcomes
   for ordinary definitions, parameters, local labels, nested calls, quoted
   operands, labelled calls, comments, unused templates and include boundaries.
   Run matching 65816 controls. Keep inactive includes and template expressions
   unevaluated while formatting. Name unrelated conditional/end gaps separately.
2. In `crates/asm198x/src/dialects/ca65_huc6280.rs`, add the existing 65816
   walker's opaque-definition state and case-sensitive call recognition, using
   shared `Ca65Macros` grammar. Keep source/trivia/spans on normal AST nodes;
   retain canonical layout and authored operand spelling. Do not alter assembly
   expansion or shared MOS parsing.
3. Add native byte equivalence, canonical/idempotent round trips and JSON/debug
   tests in `crates/asm198x/tests/`, including missing-include formatting guards
   and included source/macro diagnostics. Promote all 185 measured #821 flags
   in the numeric-admission, literal-boundary and character-operand fixtures;
   remove the 18 stale closed #833/#834 flags under #902 after their verified replays.
4. Add required package and omission evidence. Show the preceding archive fails
   the new/prompted formatter checks. Run full Rust/Lua, Node, Clippy/fmt/Ruff/docs,
   clean native replays, archive/corruption controls and fresh 4510 checks.
5. Inspect exact-head CI and every actual five-host archive/control, rebase-merge,
   retain raw evidence remotely, then clean the owned worktree/cache/processes.
