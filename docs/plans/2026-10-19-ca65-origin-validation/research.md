# Origin source-time findings

The completed 57-case native matrix has 30 accepted and 27 refused cases.
The explicit preceding counter-fix executable differs on 21. Two are the
above-24-bit origins split into #817; the others remain #815.

Native accepts previously defined constants, aliases that have become constant
before .org, absolute PC/labels, and differences of relative labels in the
same segment. It refuses a demanded forward constant, relative PC, cross-segment
label differences and an unqualified outer constant inside an open child scope.
A qualified closed-scope constant is accepted. These observations agree with
ConstExpression studying the expression before DoOrg changes the mode.

An alias of an absolute definition PC is native-valid; the preceding linked
path currently refuses it as undefined. Correcting only an origin's source
constancy would not fix that existing false refusal. Preserve the alias's
definition position and resolve the value before the origin transition.

Short-circuit operands still leave native symbol references: a forward VALUE
in 1||VALUE can be accepted if eventually defined, while a missing VALUE
refuses. Retain that final existence check separately from numeric demand.

The initial capture is retained as native-initial.json. Its lazy cyclic alias
case timed out after ten seconds (exit 124), and its macro parameter spelling
was invalid. Neither is qualification evidence. The completed matrix replaces
the cycle with a known short-circuit constant, but its revised macro parameter
control still has a syntax error: native ca65 has no comma between the macro
name and first parameter. That case is a syntax refusal, not a valid macro
execution control. Every completed native process exits 0 or 1; the seven
range qualification cases contain no macros. The follow-up numeric audit
retains corrected macro admission controls separately.

Do not implement a broad eager fold or flatten scope lookup. The next private
design must track source labels/definition PCs and relocatable segment terms,
reuse the existing scope spelling and deferred graph, cancel same-segment
relative terms, and preserve original source expressions for reference effects
and diagnostics. It needs no public AST or Debug198x change.

A further 32 native expression controls have 27 accepted and 5 refused cases.
Native accepts both 0*relative_label and relative_label*0, while relative_label&0,
relative_label|0 and relative_label^itself are not source-time constants.
Cancellation within (end-start) remains constant under multiplication or low
byte extraction. Aliases of relative labels cancel against their owners.
These cases rule out reusing segment_weights unchanged: its conservative
non-additive rule would falsely refuse native-valid multiplication by zero.
