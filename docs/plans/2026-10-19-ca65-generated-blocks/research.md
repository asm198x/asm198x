# Generated block heads (#861)

Fresh native ca65/ld65 controls: 99 cases, 64 accepted and 35 refused. The frozen
formatter producer 6283a1b3638908b6851be800cf1b9f06d38d78b8 (CLI SHA-256
c6d24d6aa5ee2e94561c78285332344c8713be16bf11de48ce5fadde7134316f)
reproduces the generated-opening failures. The structural classifier strips only
static labels; selected `read_block_head` already performs text evaluation and
binding in source order. Structural recognition must use the existing balanced
source helper without folding arguments or reading includes.

The matrix also exposes native-invalid literal registers/size prefixes and
instruction names falsely accepted on static block heads. New generated-head
admission must not make generated instruction names valid. Keep selected
validation at the block head: distinguish explicit generated register identifiers
from literal tokens, use the active CPU policy and existing instruction-name
predicate, then bind through the existing mechanism. The 115 end-label controls
must remain unchanged if the same private policy is shared.

Native also accepts six static/generated labels on .endif/.else/.endrepeat that
the current assembler loses or refuses. Retain those as a separate closing-
delimiter issue; this opening-head slice must not silently consume their labels.
No public AST/schema change or new reader architecture is proposed.

The first production change passes the 99-case opening/name matrix: 58 native-
accepted inputs match bytes and 35 native refusals are rejected. Only six named
closing-label cases remain #866. This fixes 50 formerly refused valid cases and
28 false acceptances, including the four instruction-name block cases in #865;
ordinary register/prefix-label policy under #853 remains separate.

Existing formatter cases had three #861 raw-assembly gap markers; those were
removed after their actual sources began assembling correctly. The generated-
format test now asserts 14 remaining raw gaps. One new macro diagnostic test
initially expected the body line; the existing contract and retained human/JSON
output locate expansion errors at the invocation, with defined/invoked frames.
The test now asserts that established contract, including the frame and include
path. No diagnostic or public schema was changed.

Actual native formatting retains all 99 outcomes and all 64 accepted images.
All 64 formatted valid sources also assemble here. Twenty-four formatted invalid
register labels still falsely succeed as before #853; each is checked against
the preceding frozen CLI byte-for-byte. All original native-invalid block heads
are refused by the new binding policy. Full Rust/package qualification follows.

Focused Rust suites, all 94 Node tests, Lua/all-target Clippy, rustfmt, Ruff and
generated docs pass. Full Rust, actual archives and clean producer/project
qualification remain pending before shipping.

The initial actual archive passes all 79 required cohorts, including 273 new
commands; changed bytes and archive corruption fail. All 94 Node tests and
Clippy/fmt/Ruff/docs pass. Final full Rust and clean producer/replay evidence
remain required before merge.

Final clean qualification passes: 1,597 Rust tests (38 ignored), 94 Node tests,
Lua/all-target Clippy, rustfmt, Ruff and docs. The frozen clean producer
`a3ef9a534f53fc733526fbbc5c211328966e04e1` has CLI SHA-256
`846c8a501b5179a47060c7d137456192154117531470b7c3723cec0b95debbe7`.
Its actual archive passes all 79 cohorts, including 273 block-head commands;
changed bytes fail exactly three reference controls and archive corruption fails
SHA-256 validation. The preceding archive fails exactly the new block-head cohort
and the generated-format cohort whose three resolved #861 gaps were removed.

Clean 4510 replay matches all 564 native bytes across seven processes and fourteen
retained streams. The changed-byte guard fails as required; disposable audit work
was removed. Shipping source and harness match the clean producer exactly.
