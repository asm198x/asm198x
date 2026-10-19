# Accept native whitespace before label colons (#848)

1. Retain native whitespace/token-boundary controls before changing code.
   Probe ASCII whitespace and non-ASCII refusals, plain/cheap/anonymous names,
   inactive source, macro/block heads, and reserved instruction names. Record
   unrelated accepted grammar gaps separately rather than changing their
   expectation to refusal.
2. If the native evidence agrees, extend the existing private split_label_head
   boundary and trim only the label's delimiter-adjacent whitespace when binding
   its symbol. Preserve scope separators, quoted/operand colons, literal .end
   priority, source positions, and the existing selected-only reserved names.
   No dependency, public schema or new reader architecture.
3. Retain native acceptance/bytes, formatter byte/idempotence/debug invariants,
   output preservation, a required real-package cohort and omission control.
   Run full Rust/Lua, Node, Clippy, fmt/Ruff/docs, clean/preceding archives and
   corruption controls, and a clean pinned 4510 replay.
4. Ship this slice on current main through exact-head checked rebase PR and
   inspect all five actual packages/receipts plus ten corruption controls.
   Clean task-owned workspaces once unique work and evidence are retained.
