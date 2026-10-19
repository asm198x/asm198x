# Generated macro heads with captured identifier preservation

The complete macro audit now has 120 cases: the original 118 controls plus two
captured-CPU regressions. Native accepts 67 and refuses 53. The targeted
96-case cohort accepts all 67 native images and refuses the other 29 inputs.
Twenty-four original static register/prefix macro-head false acceptances remain
explicitly tracked in #853; every exclusion has both a native refusal and a
positive acceptance in clean predecessor
`9b48a599d997e883505d60ce47ed65d76c05ff03`. Tests account for every audit case
exactly once and verify those positive predecessor outcomes.

Generated heads are classified before text folding, while the old linked
macro-header/call split strips only static labels. A generated definition
therefore reaches ordinary parsing and fails with `unsupported directive
.macro`. Use the existing balanced classifier to find the macro word without
evaluating inactive text. Fold only a selected label and bind it through the
existing path before definition/body execution. Apply the existing instruction
name check to fix #870. Definition-body capture, invocation arguments and the
shared macro grammar retain their existing timing and representation.

The initial pending producer applied register/prefix admission using the
current CPU. A follow-up 398-case audit found two new refusals: S/SP call labels
captured under 6502 remain identifier tokens after a switch to 4510. Native and
the preceding main producer emit `07 00 10`; the initial pending macro producer
refuses them at the outer macro invocation. The red-first extended test captures
that exact failure. Register/prefix identity needs the wider #853 capture model,
so it is removed from this slice rather than guessed from the final name.

The narrowed fix passes all 96 targeted raw/formatted outcomes and 67 native
images, including both captured CPU controls, with 61 explicit label
address/kind checks. The complete raw comparison has exactly the 24 positively
proved #853 differences. Across all 398 follow-up inputs it introduces no new
difference against the preceding main producer; the existing 204 differences
remain visible. Both former #862 raw-assembly gaps in the 46-case formatter
cohort are restored; eight unrelated raw gaps and two #762 formatted alias
gaps remain explicitly tracked.

The [398-case audit and both frozen producer comparisons](https://github.com/asm198x/asm198x/tree/575fec1b/docs/plans/2026-10-20-ca65-label-token-boundaries)
retain the new refusals and token-origin evidence. Earlier withdrawn trials are
[retained separately](https://github.com/asm198x/asm198x/tree/733ca4bb/docs/plans/2026-10-19-ca65-generated-macros).
#883 stays draft until the corrected shipping head has full local, clean replay
and five-platform package qualification. The shipping branch imports this slice
alone, preserving the independent prerequisite fixes already on main.
