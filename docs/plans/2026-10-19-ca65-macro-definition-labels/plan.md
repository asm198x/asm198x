# Bind ca65 labels on macro definitions (#782)

Native accepts both entry:.macro emit and entry: .macro emit. The structural
macro-range shield and selected registration each inspect the entire line as
an unlabeled header, so neither recognises these forms. The label must bind
when the definition is read, including when the macro is never called.

1. Retain bounded native ca65/ld65 cases and predecessor failures. Probe both
   delimiters, .mac, indentation, cheap/anonymous labels, source positions,
   selected/dead branches, repeats, textual includes, macro bodies, qualified
   scopes and malformed headers. Measure body capture and label-anchor timing.
2. Add a private linked-ca65 header recogniser that peels the existing lexical
   label prefix. Use it in structural shielding and selected registration;
   leave the shared/flat MacroSyntax grammar unchanged. Bind selected definition
   labels through Walker::block_open and the existing placed-label path, with
   the actual file, scope, anonymous stream and expansion frames. Preserve
   authored macro/body text and live capture/registration timing.
3. Require native byte/status, formatter/idempotence/reassembly/debug checks,
   definition-address invariants and included human/JSON refusal preservation.
   Add mandatory package replay/omission controls. Run isolated full Rust/Lua,
   Clippy, fmt, Ruff, docs, Node and actual archive controls. Audit the clean
   unchanged 4510 producer; qualify all five exact-head platform packages.
4. Rebase onto merged #781, squash through a PR, verify producer retention and
   issue closure, clean the worktree and continue the native audit.

Apply syntax-stance, macro-expansion-framework and parse-affecting-directives.
No public AST/API/schema/trait, dependency or architectural change is needed.
The user delegated routine design decisions; native probes decide the phase.
