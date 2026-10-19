# Native reserved label tokens (#849)

The 158-case native audit covers statement-head, absolute-qualified and
cheap-local names under 6502 and 4510, plus skipped source, unused/selected
macros, generated names and CPU stack changes. ca65 accepts 107 and refuses
51. The clean cc8a56a predecessor differs on 70 cases.

Thirty-five are already accepted absolute-qualified definitions refused by
Asm's label grammar; these remain #850, with complete native/current evidence
retained in this audit. They are excluded from the 123-case required fixture,
rather than asserted as correct refusals. This slice resolves the other 35
mismatches; all 123 retained acceptance/byte/refusal controls now match native.

Ordinary labels reserve active instruction names and A/X/Y/Z tokens. 4510 also
reserves S/SP. Native leaves BRU/LBRU aliases available, and permits all these
names as cheap locals. The existing instruction-name policy for constants is
shared without changing constant/register rules. Validation is selected-only:
inactive branches, unused macros and zero repeats do not reject authored
instruction-name labels. The formatter preserves their syntax. No public AST,
new dependency, feature switch or qualified-definition support is introduced.

Both integration tests pass, including all 72 accepted fixture formatter
round trips and five-output preservation with included-file diagnostics in
human and JSON modes. Source and include content remain unchanged.

The full suite exposed one pre-existing cycle fixture using reserved `a:`. Native rejects the original fixture; renaming that label to `routine_one` preserves its instruction bytes and cycle assertions. Both native outcomes are retained in cycle-fixture-native.json.

Local verification: 1,584 Rust tests pass (38 ignored), all 88 Node tests pass, and Lua/all-target Clippy, fmt/Ruff and generated docs checks pass.
