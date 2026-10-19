# Enforce the native origin range first

Native pseudo.c DoOrg checks the evaluated origin against 0..0xFFFFFF before
entering absolute mode. The linked path evaluates the expression but omits
this check, accepting $1000000 and $FFFFFFFF. Add that bound at the existing
Kind::Org transition, preserving its statement source span and file offsets.

This small slice uses the seven retained native range cases plus human/JSON
include failure and output preservation checks. It does not claim source-time
constancy is fixed: the 57-case research matrix still contains forward-origin,
relative-PC, alias and scope discrepancies under #815. Keep that issue open.
Implement source-time validation only after its private evaluation design is
complete; a naive early fold would reject valid relative label differences.

No new dependency, public type, source grammar or architecture is needed.
