# Read native whitespace before text-function arguments

The shared text scanner checks for `(` immediately after a function word in both
fold_once and has_call. Native ca65 admits spaces/tabs between .ident (and other
text functions) and their argument parentheses. The #862 trial exposed this as a
prerequisite: a canonical standalone generated label could not be assembled.

1. Capture native controls for adjacent/spaced/tabbed and mixed-case functions,
   nested call ordering, generated labels and scalar/string consumers, quoted
   text/comments, skipped sources and malformed/unknown calls. Retain clean
   scope-producer outcomes before changing code, then file the confirmed bug.
2. Add red-first native byte/debug/formatter controls. Extend the existing
   private TextSyntax grammar seam with ca65's measured whitespace posture;
   retain other dialects' defaults. Use one opening-parenthesis locator in
   fold_once and has_call so nested calls use the same token boundary. No
   public AST/schema/dependency or separate preprocessor.
3. Add the required package cohort and omission guard. Verify focused/full Rust,
   native formatter outcomes, Node, Clippy/fmt/Ruff/docs, prior/current archives,
   corruption controls and a clean producer/4510 replay with complete streams.
4. Ship isolated commits on current main after #869; inspect exact-head CI and
   all five actual package reports/receipts and ten corruption controls before
   rebase merge. Resume #862 only after this prerequisite is qualified.
