# Canonical source after .end (#833)

The formatter used the semantic ca65 walker with text expansion disabled. That
still executed .end, mutated scope/record state and parsed numeric expressions.
A zero repeat lost its .end and following byte even though native assembly
emitted 08. The retained red token-completeness test observes that deletion.

The selected syntax-only reader reads every physical line, separating comments,
label heads, assignments and statements. It neither executes directives nor
requires semantic block closure. Source statements use the shared AST canonical
emitter; assignments carry a private, formatter-only marker with no fabricated
expression value. Both shared lowering paths reject that marker. No public API,
schema or dependency changes. Assembly keeps its existing structural reader.

The 26 native controls contain 22 accepted sources and four refusals. Actual
ca65/ld65 reassembly of every formatted source preserves native acceptance and
all accepted bytes. Canonical layout, complete tokens/comments, aligned constant
runs, idempotence, CRLF and missing-final-newline cases are checked. Human/JSON
formatted assembly failures preserve five outputs and both source files.

Native-valid root/called .end before an unclosed conditional still encounters
the separate assembly gap #844; the formatter retains both sources in full.
Formatting a labelled .end places the label before the directive and can be
assembled, while assembling its original attached spelling remains #830.
These assembly gaps are explicit fixture fields/outcomes, not native parity
claims. All 75 preceding actual-archive cohorts and the new 76-cohort archive
pass. All 91 Node tests, Lua/all-target Clippy, rustfmt, Ruff and generated-doc checks
pass. Full Rust verification passes: 1,591 tests, with 38 ignored. Clean producer
cafe5cd1d8510679f87607b7cf36d106a11913c0 (CLI SHA-256
371235c2826db37e56ba006a39b5469905d3ba49740e25a15a5df7adff28a25c)
passes all 76 actual-archive cohorts, including 78 new commands; both corruption
controls fail. All 26 native formatter outcomes were rechecked with that frozen
CLI. A clean pinned 4510 replay matches all 564 bytes, with seven successful
processes, fourteen streams and a failing changed-byte control. Exact-head CI
and five actual host packages remain required before merge.
