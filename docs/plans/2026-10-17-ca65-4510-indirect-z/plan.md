# ca65 4510 indirect-Z (#722)

1. Reproduce the reported source and unchanged target using pinned ca65/ld65.
   Trace processor selection, operand parsing and existing ISA forms before
   changing production code. Capture native bytes, refusals and tool identities.
2. Record the root cause and agree any needed CPU-selection design correction.
   Add native differential and formatter/CLI regressions before a scoped fix.
   Preserve existing processor behavior and debug/include locations.
3. Require the new replay in candidate package qualification. Run workspace Lua
   tests, Clippy, formatting, documentation, native probe lint and Node tests.
4. Commit a clean producer and replay unchanged cc65 4510 opcode source, retaining
   exact output and complete streams. File any independent next boundary.
5. Push a PR, verify every exact-head workflow and all five actual package reports
   with negative controls, squash merge, retain producer refs and clean task-owned
   worktrees, builds, source clones and temporary files.
