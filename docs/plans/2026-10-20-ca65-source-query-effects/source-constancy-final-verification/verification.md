# Corrected source classifier

The 314-case native audit accepts 305 and refuses nine. The predecessor has
161 wrong-byte results and one existing undefined-query wrong acceptance.
All 313 supported cases pass permanent native file/JSON bytes, official debug
reads, idempotent formatting, symbol/section round trips and four-output failure
preservation. One unchanged explicit_outer_alias_scope gap remains #795 and
is named in fixture metadata. The required archive cohort uses 1,228 commands.

Each alias is classified once in dependency order. Pc reads use its existing
private definition placement's segment, while the query's own Pc uses the
query segment. No new lexical state or scope environment is introduced. All
sixteen native PC/segment controls are permanent, and the cross-segment test
was demonstrated red on the first implementation before the placement fix.

Focused tests, all 115 Node checks, all-target Clippy, rustfmt, Ruff and generated
docs/version checks pass. Full tests, frozen replays, fresh release replay,
archive controls and five-host CI must be rerun on the corrected producer.
Live .const and numeric string argument effects remain #795 work.
