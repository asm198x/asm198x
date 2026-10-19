# Validate ca65 record field counts before allocating (#786)

Native refuses nonpositive struct/union field counts. The current record
reader multiplies them directly and a union's maximum silently turns negative
fields into zero. Validate the count itself: a valid empty tagged record has
zero size despite its positive instance count, and must remain accepted.

1. Retain native ca65/ld65 cases for all existing allocators, named/anonymous
   struct and union fields, missing/negative/zero/positive/boundary counts and
   empty/nonempty tagged records. Keep output tiny by emitting only sizeof
   values. Measure native limits before choosing the exact validation.
2. Add the measured check in ca65/mod.rs record_field before multiplication,
   symbol insertion and allocation. Native also requires a res argument, bounds
   each allocated field to 24 bits; retain paired limits for all unit sizes.
   Empty tags accept positive counts beyond 32 bits, so do not impose a
   count ceiling or confuse zero allocated bytes with an invalid count.
   Use checked multiplication. Preserve empty records, ordinary res/data
   directives and record symbol/size/debug contracts. No new public shape.
3. Test native status/bytes, formatter idempotence/reassembly/debug shape and
   included human/JSON refusals preserving all five outputs. Require actual
   packaged replay and omission control; prove the preceding archive fails.
   Run isolated Rust/Lua, Clippy, fmt, Ruff, docs, Node and both archive controls.
4. Retain a clean unchanged 4510 producer replay after the parent merges;
   require complete exact-head CI and all five actual package reports, squash
   through a PR, verify retained commits/closure, clean and continue #785.

Use syntax-stance and the existing native record/selected-reader paths. This
is validation of measured invalid source, with no dependency or API change.
The user delegated routine decisions. Do not reject positive-count empty tags.
