# Next linked slice: whole numeric condition input

After the repeat checkpoint is checked and pushed, move SourceContext::read
onto a strict single-expression entry over the existing owned input. This is
the remaining numeric conditional adapter (.if/.elseif/.ifconst/.ifnconst),
also shared by the legacy live-number compatibility callbacks.

1. Add reader::value over Input/Numeric, require end after one Expr, and retain
   the existing SourceContext::retain_read scope/span/requirement handling.
   Generated commas are visible and rejected by conditional grammar rather
   than hidden in a one-Expr selector callback. Use no alternate arithmetic or
   source-name resolver.
2. Preserve selected/lazy branch reads, earlier-definition diagnostics, cheap
   anchors, macro/repeat capture admission and pending includes. Keep name-only
   .ifdef/.ifndef behavior separate until its typed header migration.
3. Replay the 200 conditional controls and every predecessor match across the
   39 matrices. The existing three sizeof-generated-label refusals are a
   distinct source-size gap; do not claim that changing input fixes it.
   Require the source/conditional/deferred contracts and affected branch,
   macro/loop/end/lexical suites, library, Clippy and formatting before pushing.

This extends the approved canonical reader. There is no new dependency, public
API, Expr value type, namespace, layout state or binding-decision change.
