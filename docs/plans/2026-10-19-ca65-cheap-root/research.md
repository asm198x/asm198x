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
