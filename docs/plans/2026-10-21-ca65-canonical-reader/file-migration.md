# Next linked slice: include/incbin fields

The retained file audit has 200 cases and 63 source-order differences. Finish
numeric conditional input first, then move the complete filename/window stream
onto the same owned reader without opening a file during parsing.

1. In ca65/reader.rs consume one StringTokens text value for the request. Include
   requires end; incbin optionally reads comma/offset and comma/size through
   source-time numeric parsing, then end. Keep generated separators, filename
   constructor effects, zero-size reads and established range/load checks.
2. Extend the existing private selected-line callback in ca65/mod.rs with a
   typed result distinguishing a statement Kind from WalkDirective. Invoke it
   after normal label binding and before legacy file parsing; retain existing
   DirectiveLine source/trivia/operand spans. Pure syntax/file adapters and
   other callers keep the legacy path.
3. In project.rs stop whole-line text expansion for selected include/incbin
   operands and have the existing SourceContext produce that typed result.
   Retain required names/deferred errors and publish the bound label's segment
   before reading operands. Keep resolve_at_sweep/open_include_in_stream/
   incbin_payload, pending token-source priority and all filesystem/memory-loader
   boundaries unchanged. No parse-time include I/O is introduced.
4. Qualify all 200 file controls through filesystem and memory paths, including
   scoped/forward/cyclic values, labels, zero/end/negative/large windows and
   malformed tails. Retain every previous matrix match and the existing
   text-consumer/alias/output/debug/formatter contracts; library/Clippy/fmt must
   pass before pushing.

This is the approved reader adapter at its existing walk seam. The typed result
is private; no public AST/schema, namespace, dependency, Expr value type or
binding decision changes. Flat readers and canonical syntax-only reading remain
later migrations; all current archive gates remain mandatory before merge.
