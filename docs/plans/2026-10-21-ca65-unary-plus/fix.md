# #931 shared-grammar correction

After the constructor-timing checkpoint is checked and pushed:

1. Add an explicit private unary_plus option to the existing ExprOpts pattern,
   enabling it only for ca65, ca65-816 and ca65-huc6280. Inspection of all callers
   corrects the initial plan: RGBDS also uses the logical switch, so it cannot
   serve as a ca65-only guard. Keep the other dialects' new option false.
   Consume the plus in ExprParser::unary and recurse through that same grammar,
   retaining precedence without inserting arithmetic or a second parser.
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
