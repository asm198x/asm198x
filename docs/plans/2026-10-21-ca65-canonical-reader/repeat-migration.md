# Next linked slice: repeat count and counter header

The scalar audit's remaining 27 differences are nine repeat bodies in three
active source contexts. The existing 18-case counter-name audit independently
shows physical A/X/Y are invalid header identifiers, while .ident-generated
spellings are valid. Preserve those token kinds instead of validating spelling.

1. In ca65/reader.rs add a complete repeat-header consumer: source-time count,
   optional comma and one identifier token, then end. Use the shared numeric
   grammar and existing constant/source requirements; reject negative counts.
2. In project.rs keep repeat operands out of whole-line text expansion in
   read_block_head, retaining normal label handling. Have fold_repeat borrow
   SourceContext with the actual span, then retain its source-name requirements
   in the existing deferred graph. Keep token capture, pending include priority,
   iteration/termination bounds and counter substitution mechanisms intact.
3. Require all 320 scalar controls and all 18 name controls. Retain the existing
   40 generated-field, 100 canonical-field and 35 constructor-boundary controls,
   and every predecessor match across the 39 matrices. Add full CLI contracts
   for the complete repeat subset; check macro/conditional/end/lexical seams.

Do not infer new counter-body binding semantics from a header token: constructed
names in bodies and physical register tokens need separate native evidence.
The approved full reader still requires conditional/file/flat/syntax adapters
and final archive qualification after this slice. No dependency, public API,
Expr value type, scope namespace or binding decision changes are proposed.
