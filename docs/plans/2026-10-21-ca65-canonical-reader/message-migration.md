# Next linked slice: assertion and message fields

Continue the approved #924 migration after #931 is checked and pushed. Existing
scalar facts retain wrong source-reference results in assertion/action/message
and output operands. The local native pseudo.c confirms separate grammars:
DoOut advances once after its string and requires statement termination, while
DoWarning/DoError use SkipUntilSep. Token constructors still run during that
skip; numeric calls and ordinary skipped names do not become expressions.

1. In ca65/reader.rs add assertion consumption on the existing input: one Expr,
   comma, one identifier action, optional comma/string message, then end. Read
   even a successful assertion's message in source order. Preserve generated
   commas, generated action identifiers and deferred assertion evaluation.
2. Add message consumption using StringTokens' existing text-prefix decoding.
   `.out` requires termination after its one string. Warning/error/fatal take
   the leading string and drain the remainder through token-level constructors,
   preserving inert names/numeric calls in skipped tail tokens. Keep existing
   severities, diagnostic emission and failure/output policies.
3. Extend the private source-consumer seam in ca65/mod.rs/project.rs for these
   operands; stop the whole-line text pre-pass before their fields, while the
   ordinary label/scope/span mechanisms remain responsible for the statement.
4. Qualify every assertion/message case in the retained 320 scalar inventory,
   including ignored-tail constructor effects and malformed action/tail cases.
   Require all prior matches across the 39 matrices and full CLI contracts;
   run appropriate Rust/Lua/Clippy/fmt checks before committing/pushing.

Repeat/count/name headers, conditional adapters, file fields, flat readers and
canonical syntax-only reading remain subsequent slices. No public API,
dependency, Expr value type, namespace or binding decision changes here.
