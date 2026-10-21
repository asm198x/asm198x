# Next canonical-reader slice: linked allocation fields

After the data checkpoint is verified and pushed, migrate `.res`, `.align` and
`.org` through the same input. This is the next part of the approved full reader,
not a separate design or merge proposal.

1. In ca65/reader.rs expose typed allocation consumers that read the count or
   boundary before a possible fill. Preserve generated commas and suffix
   arithmetic. `.org` keeps an Expr for existing placement handling; allocation
   bounds use existing source-time constant evaluation and range checks.
2. In ca65/mod.rs extend the private source-consumer seam to dispatch those
   fields; retain pure syntax parsing for formatter entry points. In
   ca65/project.rs avoid whole-line text expansion for these operands, bind the
   label through the ordinary reader, and retain existing spans/requirements.
3. Replay the 320 scalar cases and all 40 generated-field controls first. Require
   native `.res .left(3,{1,2})+1` to emit 03, and all prior matches to survive.
   Then run all retained matrices and appropriate Rust/Lua, contract and Clippy
   checks before committing and pushing. Keep the full-reader branch unmerged.

Assertion, conditional/repeat, filename/header, flat and syntax adapters remain
subsequent slices. No public API, Expr value type, dependency, scope namespace or
binding decision changes in this slice.
