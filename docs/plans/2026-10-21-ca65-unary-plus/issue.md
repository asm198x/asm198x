The ca65 expression readers refuse native unary `+`, including a fill operand exposed by a token selector.

```asm
.byte +1
.byte 2*+3
.res .left(2,{1,2})+1
```

Native ca65/ld65 V2.18 emits `01`, `06` and `01` respectively. We refuse these with `expected a value`. The selector example produces operands `1,+1`; preserving the generated comma alone cannot make it work.

The retained [135-case audit](https://github.com/asm198x/asm198x/tree/b5cec81ae0841737c0d423eee5dd0927a3f13d78/docs/plans/2026-10-21-ca65-unary-plus) covers all three ca65 CPUs in direct, included, macro, inactive and unused source. Both checked #929 and the allocation reader match 54 cases. There are 63 unary-plus admission gaps and 18 unrelated inactive flat-reader gaps tracked by #895. Native process streams, bytes and executable hashes are retained.

The shared `mos6502::ExprParser::unary` has no plus branch. Add this native operator through an explicit private per-dialect option, retaining other dialects' behavior. RGBDS also enables logical operators, so the logical switch cannot serve as a ca65-only guard. Cover character/query operands, precedence, malformed `+`, inert captured plus tokens, formatting and output preservation. Keep every checked predecessor match. This is a narrow arithmetic follow-up to #924/#795; do not implement a second expression parser.
