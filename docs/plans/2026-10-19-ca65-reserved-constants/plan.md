# Refuse native reserved names in ca65 definitions (#775)

Native rejects unqualified instruction names and register tokens as immutable
definitions. Qualified/global names can bypass instruction-head recognition,
while register tokens remain reserved. CPU selection changes the vocabulary:
Z and SP are valid names on 6502 and reserved on 4510.

1. Retain native ca65/ld65 probes in probe.py and
   tests/fixtures/ca65-reserved-constants.json. Sweep candidate mnemonic names
   from the existing ISA/source tables through both native CPUs; those tables
   supply candidates, not assumed outcomes. Cover keyword casing, qualified
   components, CPU selectors/stack, aliases, dead branches and ordinary names.
   Reproduce with bounded tests/ca65_reserved_constants.rs.
2. In ca65/mod.rs constant_key, reject measured register tokens and instruction
   heads using the active existing ISA lookup plus measured source-vocabulary
   alias/exception handling. Preserve explicit global/qualified definitions
   accepted natively and cheap tokens' independent grammar. Do not alter
   references, other dialects or public AST/API/schema shapes.
3. Require package replay and omission control in the two candidate scripts.
   Run full Rust/Lua, Clippy, fmt, Ruff, docs, Node, a real Lua-enabled archive
   and both corruption controls; retain complete native evidence and a clean
   unchanged 4510 replay. Rebase onto the merged cheap-constant slice.
4. Push a focused PR, inspect exact-head CI and five actual package reports,
   squash merge, verify producer retention/closure, clean the worktree and
   continue the next measured mismatch.

Follow syntax-stance.md, macro-expansion-framework.md and the selected/lazy
reader decisions. The user delegated routine design decisions. No dependency,
new architectural pattern or public shape change is needed.
