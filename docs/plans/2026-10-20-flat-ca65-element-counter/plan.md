# Preserve the native counter of each data element (#910)

The 254-case audit leaves twelve wrong-byte cases: both flat CPUs use one
statement-start Expr::Pc for every element. Native `.word *,*` advances by two
between values. The engine's statement-start contract is shared with dialects
that deliberately use it; it must remain unchanged.

1. Retain 304 native direct, macro, include and unused-template controls for
   per-element widths, repeated/cancelling Pc terms, strings and byte-string
   constructors, prefix extraction, arithmetic, forward labels and second-value
   range refusal. Replay the frozen #909 producer before changing source.
2. Use existing Expr lowering in ca65_flat.rs to bind each Pc leaf to the
   statement Pc plus that element's byte offset. Keep the original source AST
   spelling. In ca65_816.rs/ca65_huc6280.rs give value_list its directive width;
   byte_list uses the emitted byte count, including decoded strings. Explicit
   extraction advances one byte per value; sized data advances by its width.
   A private exhaustive expression visitor follows existing AST rewrite helpers.
   No new Expr/Operation, engine counter policy, schema or eager evaluation.
3. Add native CLI and required archive coverage with file/JSON/native bytes,
   output preservation, macro/include locations, formatter idempotence and debug
   invariants. Tests with Pc-Pc and Pc+Pc distinguish per-leaf binding from an
   incorrect final-value adjustment. Keep linked/other dialect behavior intact.
4. Replay all 254 broader controls and preceding 80/132/104/148 cohorts, then
   full Rust/Lua, Node, lint/docs, frozen artifact, archive controls, fresh native
   and exact-head five-host CI evidence. Merge and clean completed task work.

The alternative is a new engine data-position policy, which would widen shared
conventions and need agreement. Splitting one authored list into AST statements
would change source/debug structure unnecessarily. Existing per-dialect Expr
lowering is the narrow correction for this well-specified native behavior.
