# Diagnostic explanations

`Code` in `crates/asm198x/src/contract.rs` is the registry for diagnostic wire
names, CLI lookup and documentation. Each entry names a Markdown file under
`crates/asm198x/docs/diagnostics/`. Keeping that source inside the published
crate lets `include_str!` work both in a checkout and in a registry build.

`cargo xtask docs` copies those embedded pages into the book and generates
their navigation. `cargo xtask docs --check` detects stale pages and links;
the explanation integration test compares every code's JSON name, CLI output
and book page. Adding a code without its source page fails compilation.
Edit the crate's Markdown source, then regenerate, rather than editing a
generated book page.

Error sites attach a typed code with `AsmError::with_code`; no classification
depends on matching diagnostic prose. Span enrichment and conversion to the
public `Diagnostic` preserve it. Lazy dialect refusals use an `Unsupported`
operation so an untaken branch remains silent. Ordinary source-requested
errors and constructs refused by the reference remain `AssemblyError`.

Classification begins with shared-engine relative branches, ca65 linking, vasm branch
encodings, cycle-ceiling violations and declared unsupported directives,
including lwasm's refused pragmas. It is not a claim that all historical
error sites are classified. An unknown instruction, malformed assertion,
incomplete timing coverage or signed index displacement must not be relabelled
as one of these merely because its message contains similar words.

The JSON envelope and `CONTRACT_VERSION` remain unchanged. `Code` gains
variants under its existing non-exhaustive public-draft policy. The engine's
public `AsmError` now carries `code`; Rust consumers constructing it as a
struct literal must supply that field. Public `AsmError::new` and
`AsmError::at` constructors supply the catch-all default and are preferred
for new callers. Its `Display` wording is unchanged. The CLI adds an
`--explain` hint for classified errors while retaining source and expansion
locations.
