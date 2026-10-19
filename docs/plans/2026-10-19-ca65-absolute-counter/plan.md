# Keep ca65 absolute PC independent of linked segment addresses (#822)

1. Re-consult native AST, source phase, layout, sections and diagnostic records.
   Retain native segment.c GetPC/NewFragment evidence and byte controls across
   CODE/RODATA/BSS/ZEROPAGE, modes, data widths, reservations/alignment,
   push/pop, aliases and macro/include/repeat streams. Compare explicit preceding
   executable; keep source-origin constancy (#815) and metadata (#814) separate.
2. In ca65/mod.rs replace the constant physical-address bias with the existing
   placement pass's independent optional absolute cursor. Bind labels and
   deferred definition PCs before mode changes, use the cursor for .org RHS and
   emission, advance it by each resolved statement size, reset only through
   .org/.reloc. Keep physical offsets, alignment, layout and data/instruction
   operand evaluation rules unchanged. No new architecture or public shape.
3. Add tests/ca65_absolute_counter.rs and retained fixture. Pin native bytes,
   accepted/refused mode cases, format/debug round trips, included spans and
   five-output preservation. Add a required package cohort/omission control.
4. Freeze source/fixtures; verify predecessor red, current Rust/Lua, Clippy,
   fmt, Node, Ruff/docs and actual local packages with both corruption controls.
   Push verified commits, retain clean unchanged 4510 producer replay.
5. Wait for #823, then inspect exact-head CI and five actual host reports with
   ten controls; squash, verify closure/remote retention and clean task resources.
   Resume the approved #814 logical-debug-section implementation on native-correct
   addresses; the approved #788 lexical exception also remains authorized.
