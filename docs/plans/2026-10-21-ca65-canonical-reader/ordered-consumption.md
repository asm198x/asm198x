# Ordered canonical-reader consumption

Continue the approved #924 reader on 98be817ee5b462378caae5f3c557a05ad17c1c2b.
The retained native matrices explain the failures; do not reuse rejected #922.

1. In mos6502.rs distinguish ordinary source names from callable symbol tokens.
   Give names and source calls explicit consumption hooks. Buffered names
   retain their existing before-lookahead semantics; the canonical reader must
   finish native scoped-name lookahead before recording the reference. #932's
   retained audit corrects the original stronger claim: an adjacent constructor
   can run here, while a physical operator stops this one-token lookahead.
   Read typed call arguments when the atom is consumed. Preserve #929 empty-call
   grammar and verify both the buffered contract and native consumption points.
2. In ca65/reader.rs build one owned input stream with generated frames consumed
   before suspended input. Constructor expansion occurs at the consumed source
   position; captured numeric calls remain tokens. Preserve kinds across ident
   and selection, and use the measured non-nesting capture boundary. Share the
   numeric grammar and TextSyntax/StringTokens instead of copying arithmetic,
   string decoding or scope resolution. Add native selector-generated brace
   controls before extending capture rules to generated punctuation.
3. Route linked fields through that reader, then flat/syntax adapters per the
   main plan. Whole-field consumers must leave generated separators visible to
   the directive grammar. Keep existing scope/required-name/span mechanisms.
   No whole-line expansion, private namespace, string Expr, public API break or
   dependency is introduced. A partial migration is not ready to merge.
4. Retain every checked match across all 39 matrices and the full contract
   fixture; run appropriate Rust/Lua/Clippy/fmt checks per commit. Push checked
   checkpoints, retain complete receipts, and delete task diagnostics only after
   remote recovery and process/file verification. Final qualification remains
   the full main-plan gate, including five actual native archives.
