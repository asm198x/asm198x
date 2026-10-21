# #931 shared-grammar correction

After the constructor-timing checkpoint is checked and pushed:

1. Add unary plus to mos6502::ExprParser::unary under the existing ca65 logical
   grammar switch. Its only production callers with that switch enabled are
   ca65, ca65-816 and ca65-huc6280; preserve the other dialects' flags and behavior.
   Consume the plus and recurse through the same unary grammar, retaining
   precedence instead of inserting an arithmetic expression or a second parser.
2. Qualify all 117 native cases outside the 18 already tracked inactive flat
   cases (#895). Keep complete 135-case input/name inventory and explicit gap
   ownership. Cover malformed plus, captured plus tokens, all three CPUs,
   formatting, byte/debug identity and output preservation.
3. Require the canonical-field matrix to reach 100/100 and retain all 6,790
   matches in the 39 matrices. Re-run the 35 constructor-boundary controls and
   appropriate Rust/Lua/Clippy/fmt checks. Push the checked small fix; leave #931
   open until its owning full-reader branch is qualified and merged.

The native facts and root cause are retained in this folder. This extends an
existing ca65 operator grammar; it changes no dependency, public API, namespace,
Expr value type or binding decision.
