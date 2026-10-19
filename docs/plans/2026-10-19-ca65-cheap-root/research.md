# Cheap-local anchor and spelling boundaries (#875)

The final 329 native controls accept 94 inputs and refuse 235. The clean
identifier producer 431560df6d7036be06c32351204ecd133c618f70 (CLI SHA-256
9cbec83b77db0d2e0199b6391df19c3a5591f161cd50388080fda0add31a0ed5)
has 167 differences: 149 missing-anchor acceptances, four cheap-label spelling
differences and fourteen anchored undefined .ifconst acceptances. The initial
327-case snapshot is retained; two label-only dot cases expose false admission
that a later invalid reference had masked.

The label splitter constructs a cheap key without checking ScopeEnv's anchor;
query_key and parse_value do likewise. constant_key_at already refuses a missing
anchor. Conditional .ifdef uses a direct cheap key too. Require the existing
anchor at selected binding/lookup before making the existing key. Keep one
ScopeEnv, unchanged key construction and lazy branch/body/include reading.

The label branch uses mos6502::is_ident, which wrongly rejects a digit after @
and admits dots. Its existing sibling is_cheap_ident matches native cheap-token
characters. Generated .ident("@1") remains invalid: FuncIdent requires a letter
or underscore after its optional @, unlike ordinary @1 source tokens. Do not
collapse the two native token grammars.

A separate .ifconst bug accepts an anchored undefined cheap symbol and emits the
else byte; native reports the unresolved symbol. This is distinct from root
anchor absence. The corresponding .const function control is already refused by
both. Retain fourteen named .ifconst controls for their own issue rather than
change conditional/deferred-reference semantics in the anchor fix.

Native anchors are established by ordinary labels/constants, named procedures,
selected branches and consumed include sources. Anonymous labels, origins,
segments, unused macros, bare emitted bytes, skipped labels and zero repeats do
not create one. Empty scopes still lack an anchor; a scope with output creates
its established exit anchor. Scope entry after an existing global preserves the
measured cheap context. Deferred includes observe an anchor created later in the
active macro/repeat stream. Those two positive controls must remain unchanged.

The selected reader now checks the existing ScopeEnv anchor before generating
its unchanged cheap key. Label binding, value/query markers and .ifdef lookup
use that check; constants retain their existing requirement with a clearer error.
The expression mapper uses the same local failure-collection pattern as existing
scope resolution. Label spelling uses existing is_cheap_ident, admitting plain
@1/@1near and refusing dots while leaving .ident string grammar unchanged.
The first compile caught a missing helper import; it was corrected without
changing the mechanism. The focused suites then pass.

All 315 scoped controls now match native, resolving 153 differences. Fourteen
anchored undefined .ifconst cases remain explicitly #879. Include/macro human/JSON
checks preserve five outputs and all sources, with the required missing-global
message and correct invocation frames. Canonical formatting stays syntax-only.
Actual native formatting preserves all 315 outcomes and 94 images, with no
assembly gaps. The restored 165-case identifier cohort preserves all outcomes and
61 images; only its existing two #876 constant gaps remain. Both #875 exclusions
were removed from that fixture and its complete-audit accounting now includes
all 165 controls. All 98 Node tests and Ruff checks pass. Full Rust/lints and clean
producer/project/archive qualification remain required before shipping.

Final full Rust passes all 1,605 tests (38 ignored). All 98 Node tests, Clippy,
rustfmt, Ruff and generated docs pass. The actual archive passes all 83 cohorts,
with 597 new cheap-local commands and 342 restored identifier commands; both
corruption controls fail. The preceding archive fails exactly those two cohorts.
The complete 329-case raw comparison leaves only the fourteen #879 cases. Freeze
this verified source and retain a complete clean 4510 replay before shipping.

Final clean producer `12af1dae8d81d8be968f717f5a2b3fb5870b5690`, tree
`bae12d46b903f2685995b9593f578fef4beb725c`, has CLI SHA-256
`e43eb5c6a1cab293f97d50cdf802aca055e8da840c1905437582d7edd7c4a1f6`.
Its actual archive passes all 83 cohorts, including 597 cheap-local commands and
342 restored identifier commands. Changed bytes fail exactly three reference
controls; corrupt archives fail SHA validation. Clean 4510 replay matches all
564 native bytes across seven processes/fourteen retained streams, and rejects
a changed-byte image. Disposable audit work was removed. Shipping code/harness
and both native audits embedded by the tests match the clean producer exactly.
