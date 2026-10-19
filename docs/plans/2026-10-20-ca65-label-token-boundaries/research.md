# Label token capture and colon boundaries

The 398-case native audit accepts 217 inputs and refuses 181. It covers attached/space/tab delimiters,
literal/generated/define/argument label origins, CPU changes, captured call
heads and selected/inactive/include/scoped controls. A direct define or macro
argument containing F produces an identifier token: `NAME:` or `arg:` can bind
F even though a literal authored `F:` is a size-prefix token. Text substitution
alone does not retain that distinction.

More urgently, pending macro producer b41e3ee95852ef74a5f2aba817dc150d39364641
introduces two refusals relative to clean predecessor
9b48a599d997e883505d60ce47ed65d76c05ff03. A macro body captures `S:emit` or
`SP:emit` under 6502, switches to 4510 before invocation, and still has a valid
identifier head natively. Native emits `07 00 10`; the pending producer reports
`reserved label name S` or SP at the macro invocation. Its selected head checker
uses the current extension set, although these tokens were read under 6502.

The pending #883 stays draft and is not eligible to merge. Narrow #862/#870 to
generated heads and instruction-name admission. Keep the 24 pre-existing static
register/prefix macro-head false acceptances explicitly under #853, with positive
native/predecessor evidence and complete coverage accounting. Add both captured
CPU regressions to the macro audit/tests before requalification. Do not guess
capture identity from a final string or blacklist names without their token kind.

The broader audit has 204 differences before the macro slice and 202 after it:
four existing false acceptances improve, but the two new valid refusals make
that register/prefix extension unsafe. Retain all outcomes, not just the net
count. A subsequent #848/#853 design must preserve native token origin, colon
adjacency and CPU capture; significant new machinery needs design agreement.
