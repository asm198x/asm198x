# Read ca65 label delimiters without requiring whitespace (#781)

Native accepts entry:nop, @local:nop and :nop. The linked reader splits only
on whitespace, so it treats each entire token as a mnemonic. Native also
accepts attached labels before macro calls, conditional/repeat heads and
include/incbin directives, including indented macro-call labels.

1. Retain bounded native probes in probe.py and
   tests/fixtures/ca65-label-delimiters.json. Cover whitespace controls,
   anonymous references, CPU selection, text names, macros, block heads,
   include/incbin, quoted/comment colons and :: scope-token controls.
   Keep leading labels on macro definitions as a separate retained gap.
2. Add one private lexical label-prefix splitter in ca65/mod.rs that recognises
   a single colon delimiter in the initial token and skips :: pairs. Reuse it
   in split_label_symbol and structural block_keyword. In ca65/project.rs,
   separate an attached prefix before passing a macro call to the existing
   one-layer expander; ca65's own call input can discard leading indentation.
   Keep ordinary AST binding, anonymous IDs, scopes and provenance unchanged;
   do not alter the shared macro grammar or flat readers.
3. Add bounded tests/ca65_label_delimiters.rs for native bytes, formatter
   idempotence/reassembly/debug addresses, diagnostics and output preservation.
   Require package replay/omission in the two candidate scripts; run full
   Rust/Lua, Clippy, fmt, Ruff, docs, Node and actual archive controls. Retain
   a clean unchanged 4510 replay after rebasing onto merged #775.
4. Push, inspect exact-head CI and five actual package reports and controls,
   squash merge, verify retained producers/closure and clean the worktree.

Use the existing source-preserving AST and selected reader under syntax-stance,
macro-expansion-framework and parse-affecting-directives. No dependency, trait
hook, public contract or architectural change is required. The user delegated
routine design decisions.

The full book check caught whitespace normalization shifting an existing include
request from column 18 to column 10. Keep ordinary lines untouched and normalize
only confirmed macro-call input. A separate CLI test derives request columns
from each authored line, with attached/spaced/indented prefixes and preservation.
Build targets are now per worktree: the retained cache red control and isolated
rerun prove the product source was unchanged when an older shared CLI was found.
