# Audit ca65 label tokens and colon separation (#848/#853)

Native F/Z followed immediately by a colon are size-prefix tokens; with a gap
they can be identifier labels. Registers also depend on CPU state, and .ident,
.define and macro arguments can carry tokens into a different source context.
Audit those boundaries before choosing a validation fix. #883 qualification
continues independently on its exact shipping head.

1. Capture native ca65/ld65 tool identities, inputs, processes and complete
   images in this directory's `probe.py`/`native.json`. Cover 6502/4510, attached,
   space/tab-separated colons, literal/.ident/.define/argument label origins,
   CPU changes, scopes/includes/selected/unused/repeated source and failed names.
2. Compare all outcomes with frozen clean producer
   b41e3ee95852ef74a5f2aba817dc150d39364641 using this directory's `compare.py`.
   Keep every difference visible. Check canonical formatting against native for
   accepted source, including mandatory F/Z token separation. Distinguish
   existing #850 qualified definitions and closing-label gaps if encountered.
3. Inspect `crates/asm198x/src/dialects/ca65/{mod,project,format}.rs`,
   `crates/asm198x/src/dialects/ca65_flat.rs`, the existing macro/capture seam and
   `crates/asm198x/src/ast.rs` label rendering. Explain where original token kind,
   colon adjacency or captured CPU state is lost. Do not infer a name blacklist
   from the first failing example.
4. Write the smallest evidence-backed design here before production edits.
   Reconsult syntax/canonical-format/source-order/capture decisions and stop at
   any matching drift trigger. Meaningful token separation is syntax; incidental
   author spacing stays canonical. Ask for any new architecture/API/schema or
   significant convention change required by the actual findings.
5. If the design fits the existing private seams, add red-first targeted native
   byte/address/formatter and diagnostic-preservation tests plus required package
   cohort/omission checks in the usual Rust fixtures and candidate scripts.
   Then use full Rust/Node/lint/docs, prior/current actual archives and corruption
   controls, a clean producer/4510 replay, exact-head CI and all five actual
   package receipts/ten controls before checked rebase merge and cleanup.
