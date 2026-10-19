# Native instruction-name labels (#849)

The 158-case native audit covers statement-head, absolute-qualified and
cheap-local names under 6502 and 4510, plus skipped source, unused/selected
macros, generated names and CPU stack changes. Native accepts 107 and refuses
51. The clean cc8a56a predecessor differs on 70 cases.

This slice resolves 20 instruction-name mismatches. The required fixture has
113 controls (77 accepted, 36 refused), all matching native. Thirty-five
absolute-qualified definition gaps remain #850. Fifteen register/address-size
prefix label cases remain #853; separate-gaps.json names each.
Those fifty cases retain full native/current evidence rather than being
asserted as correct refusals or acceptances in this slice.

The private instruction-name policy is shared with existing constant handling,
including native 4510 aliases. Only selected ordinary label heads are checked;
cheap-local names, inactive branches, unused macros and zero repeats preserve
their native identifier behavior. Formatting retains authored syntax.

The initial broader register guard was discarded before shipping: native Z:
is an adjacent address-size prefix, while Z : is a permitted 6502 label. Native
scanner lines 1152-1210 distinguish A/F/Z prefix adjacency and CPU-dependent
register tokens. Captured names generated with .ident require separate token
provenance investigation. No blanket register rule belongs in this change.
The unchanged cycle fixture using a: is also a native refusal for that follow-up;
its earlier rename was reverted when this slice was narrowed.

No public AST, dependency, feature switch, qualified-label support or register
lexical change is introduced. The 72 implemented formatter round trips
must preserve bytes, symbols and sections. Included-file failures must preserve
five outputs and source content in human and JSON modes.

Five native generated A/X/Y/Z/F positive controls guard against a blanket register-name blacklist. Their existing formatter refusal is #854; the required package still executes all five original assembly commands and verifies native bytes. The other 72 accepted cases require formatter idempotence and byte/symbol/section equality. The cohort has 329 required commands.

Final narrowed source: 1,584 Rust tests pass (38 ignored); the final 113-case focused fixture passes, as do 88 Node tests, Lua/all-target Clippy, fmt/Ruff and generated docs.

Clean producer 711ede39cbf320b3b42910aad3588dc0d27a0b95 has executable SHA-256 04cd785990ddfcb28a3f7ef250ae85ff3052b176b8422894934ac3f8859bb63b. All 113 clean fixture controls match native. Its actual archive passes 73 cohorts and all 329 new commands; changed bytes fail exactly the three byte controls and an altered archive fails SHA-256. The preceding archive fails only the new cohort. Clean pinned 4510 replay matches all 564 bytes, with seven processes and fourteen streams retained and a failing changed-byte control.
