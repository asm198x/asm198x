# ca65 definition-time text capture (#768)

Native probes show `0102` for a body/argument pair where the current reader emits
`0202`: existing text symbols capture when a macro is defined, while arguments
read at invocation. Repeat token capture likewise freezes already active text
symbols without interpreting untaken branches. This repairs a byte-changing bug.

1. Extend `probe.py` and `crates/asm198x/tests/fixtures/ca65-text-capture.json`
   with definition/invocation timing, branches, repeats, arguments, nested calls,
   macro visibility/local labels, includes and refusal controls. Reproduce with
   bounded CLI native/formatter/debug tests in `tests/ca65_text_capture.rs`.
2. Reuse the existing live macro pattern used by ACME: `MacroState` and
   `expand_one_at` in `dialects/macros.rs`. Keep macro definition lines unread
   during the structural ca65 parse, using private reader state to keep body
   block tokens together. In `ca65/mod.rs` preserve source instead of globally
   expanding macros before the selected sweep. Capture existing text tokens
   when a selected definition/repeat is reached; leave meaning lazy.
3. In `ca65/project.rs`, collect/register selected definitions with captured
   bodies and authored spans, then expand one invocation layer in the live
   environment. Preserve parameters, unique locals, labels, recursion bounds,
   macro frames and the include token boundaries already implemented. Reuse
   existing private AST/native payload shapes; change no public contract.
   Add only the small shared macro-state helper needed for registration and
   current defined-macro lookup. Document the measured phase ordering in
   `decisions/parse-affecting-directives.md`.
4. Require package replay/omission controls in the two candidate scripts. Run
   focused/native/formatter/debug/preservation tests, a real Lua-enabled local
   archive preflight and both corruption controls, Rust/Lua, Clippy, fmt, Ruff,
   docs and Node. Align with the merged include-stream slice before final CI
   and the clean unchanged 4510 replay; retain sources, streams and red controls.
5. Push the PR, inspect all exact-head workflows and all five actual package
   reports and negative controls, squash merge, clean finished worktrees and
   continue remaining compatibility findings.

Structural macro-body shielding uses complete source ranges only: an unclosed
macro in an untaken branch must not hide that branch's closing `.endif` (native
control). The AST is not cloneable, so repeat snapshots live in private capture
frames keyed by source file and macro provenance; nested macro input uses its
own captured body instead of inheriting a caller's repeat-token snapshot.
CLI test streams use files so long recursive diagnostics cannot fill a pipe
while the watchdog polls the child.

The expanded native boundary cohort proves counters are tokens rather than
numeric variables: an unparameterised macro body cannot see the caller's I,
and `.ident("I")` does not create a counter reference. Bind substitution to its
source file/macro provenance and remove the old numeric-environment fallback.

The preservation test exposed a layout/emit provenance gap: native private
statements retained file/line but dropped macro frames. Carry existing frame
metadata through statement placement and stamp backend errors from it, without
changing the diagnostic schema or fabricating source columns.

A paired five-run density check (0/50/500 definitions, 10,000 instructions)
found namespace copying on each live line: the 500-definition median increased
from 0.11s to 1.17s. Keep only new fragment definition positions and query the
existing namespace directly; preserve positional visibility and rerun native,
archive, full-suite and density checks after removing the copies.
