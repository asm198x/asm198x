# Ordered data operands (#795)

Reuse the approved source-aware expression design and #916's live context.
Keep this slice to data lists; repeat counts and other consumers follow.

1. Capture native byte/word/extractor/string lists, intra-operand ordering,
   current labels, scoped names and missing-name admission in
   `data-context-probe.py` and `data-context-native.json`. Compare the frozen
   #916 producer and retain the complete before result.
2. In `ca65/mod.rs`, let the selected reader supply a data parser after its
   ordinary label binding. Preserve the formatter's pure path and one labelled
   statement. In `ca65/project.rs`, retain data operands until that callback,
   consume each expression through the existing live context, and retain its
   original read requirements. Publish the current label's segment through the
   existing projection map before classifying its operands.
3. Add the measured native cases to `tests/fixtures/ca65-data-context.json`
   and the existing differential/debug/formatter/refusal harness. Add the
   required candidate cohort and omission guard in the package scripts.
4. Verify focused Rust tests, every prior native match, fmt/Clippy, Node and
   workspace tests. Freeze the producer before long CLI tests. Verify actual
   package reports, negative controls and the predecessor omission control.
5. Push a conventional commit and checked PR; read all required CI and five
   actual native archive qualifications before rebase merging. Retain evidence
   and clean only positively identified, finished task workspaces.

No new dependencies, expression types, namespaces or layout-time text values.
Ordinary label/procedure sizes remain #798.
