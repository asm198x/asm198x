# Live conditional expression context (#795)

The full source-context design is approved. Deliver the conditional expression
consumer first; data-list operands and other consumers remain tracked by #795.

1. Add an opt-in source context to mos6502.rs. Pure parsing stays unchanged.
   Ordinary atoms invoke the live read hook. Recognized source functions retain
   raw arguments and execute at their expression position, not during lexical
   recognition. Name-only queries never read their arguments as numbers.
2. Add a checked live numeric/constancy adapter to ca65_flat.rs. Reuse the
   existing text grammar, immutable byte strings and shared text expansion.
   Preserve genuine diagnostic spans and pure defaults.
3. In ca65/project.rs, keep numeric conditional heads unread until their live
   parser context runs. Delegate scope/reference lookup to Walker, classify
   .const through the existing Deferred/segment classifier, and retain ordinary
   dependencies for final admission even when a nested query returns a number.
4. Retain native root/scoped, alias, forward/missing, numeric-string, ordering,
   include/macro/lazy controls. Prove the predecessor fails the new fixture;
   require earlier native matches, formatter/debug/output preservation, complete
   Rust/Lua/Node/lint checks, fresh 4510 execution and actual native archives.
5. Push and checked rebase-merge the slice; retain evidence and clean owned
   completed directories. Continue data-list integration on the proven seam.
