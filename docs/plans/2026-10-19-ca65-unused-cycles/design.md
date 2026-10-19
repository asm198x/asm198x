# Freeze definition width; validate values when native does

The root cause is unconditional final numeric resolution. Native first studies
a definition's width before installing its expression. At the end it studies
expressions whose width is still unknown. Actual value uses and later alias
initializers also study their referenced expressions. This explains the
accepted known-width unused cycles and the refused unknown-width/used cycles.
Installed native captures pin this behavior; upstream SymDef/SymCheck/StudySymbol
provide implementation precedent, not a substitute version for the oracle.

Use the existing private definition and operand-hint seams:

- Retain each definition's source width and source PC segment width. Extend the
  sparse hint with a cycle observation, using existing placement positions to
  locate the original definition diagnostic. Compute this before insertion,
  so the new name is still unresolved, as in native SymDef. A later initializer
  that traverses an already-complete cycle refuses at the original definition.
- Keep the cache invalidation paths. Numeric operations use Expr::eval_with;
  logical short-circuit folds use that same evaluator. Try one source numeric
  fold at insertion even with missing syntactic dependencies, so `0&&SELF`
  becomes zero. Incremental propagation still waits for remaining dependencies
  and avoids repeatedly scanning large fan-in expressions.
- Continue checking all retained expressions for undefined names, including
  unused definitions and short-circuited operands. Complete the existing
  iterative numeric queue. A remaining cycle must refuse if its source width
  was unknown or its numeric value is demanded. Otherwise keep it unevaluated.
- Derive numeric demands after ready aliases and labels have values, by running
  the shared evaluator over the existing placed expressions. Record unresolved
  lookups; do not invent values to visit both sides. This preserves lazy numeric
  uses such as `0&&CYCLIC`. Visibility claims demand their exported values.
- Omit unevaluated constants from the numeric debug symbol list; never index a
  missing value or invent zero. Formatter/source shape and the public contract
  remain unchanged. Preserve include definition spans and existing outputs.

Freezing the PC segment width also corrects #802: a ZEROPAGE alias of `*` used
later in CODE selects zero page. Keep the unrelated `type=zp` file-emission
mismatch as #801. Two cases from this research exercise that linker mismatch
and stay explicitly deferred. Native export/global circular cases crash with
signal 11; retain four observations separately and do not use a crash as a
reference refusal. Controlled Asm refusal remains appropriate for those inputs.

The user delegated routine design decisions. This extends existing private
metadata/evaluation paths without a dependency, shared IR or public schema
change. The first forced-width report lacked a ZEROPAGE config assignment; it
is retained as missing-zp-config evidence, and the batch was rerun with a
supported config before interpretation.
