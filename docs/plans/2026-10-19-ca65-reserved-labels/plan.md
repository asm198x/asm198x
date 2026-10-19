# Reject native instruction-name label heads (#849)

1. Retain a native matrix for ordinary, absolute-qualified and cheap-local
   label definitions under 6502 and 4510 CPU state, plus inactive/captured and
   text-generated controls. Compare the clean cc8a56a predecessor before code.
2. Reuse the existing ca65 instruction-name policy only where the native label
   matrix agrees; preserve selected-source reading and formatter syntax.
   Do not add feature switches, new dependencies, public schema changes or
   qualified-label definition support (#850), register/prefix token changes (#853),
   or computed-label formatting (#854). If native label/constant rules
   differ, retain that distinction rather than broadening the policy.
3. Add exact native acceptance/bytes and output-preservation controls; require
   the packaged binary to replay them. Verify focused then full Rust/Lua,
   Node, Clippy, formatting/docs and a clean pinned 4510 replay.
4. Ship this verified slice as its own commit beside #842 in a checked rebase PR, inspect
   all five actual candidate reports/receipts and ten corruption controls,
   and clean owned workspaces when retained work permits removal.
