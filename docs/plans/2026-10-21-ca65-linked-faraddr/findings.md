# Configured far-address results (#921)

The configured reader previously lowered each `.faraddr` value to three byte
extractors. This discarded its unsigned range and evaluated `*` at three
different PCs. Native `.org $ffff` followed by `.faraddr *` emits `ffff00`; the
old path emitted `ff0001`. Native rejects -1 and $1000000; the old path silently
emitted `ffffff` and `000000`.

The fix retains original expressions in private FarAddrs data variants through
symbol mapping, demand discovery, placement and final emission. The configured
linker's existing unsigned checker validates 0..16777215 before emission, and
PC advances three bytes once per value. Flat data operations and byte selectors
retain their existing paths.

The 190 native cases include all original 90, with identical original native
source/config/status/bytes. The checked #920 producer matched 139: 39 false
acceptances and 12 wrong-byte results. The fixed CLI matches all 190. The
mandatory fixture retains all cases and no exclusions. Integration checks use
the official debug reader, JSON/native bytes, formatter idempotence, formatted
reassembly, source/macro diagnostics and refused-output preservation.

All 29 prior matrices (4,791 cases) preserve every checked #920 match. Additional
40 generated-field, 18 repeat-name, 320 scalar and 200 file controls preserve
all predecessor matches. The generated-field matrix remains 40/40. Known
#795/#918/#923 differences remain inventoried; rejected #922 code is absent.

An added diagnostic assertion initially expected a macro definition line rather
than the invocation line: `negative_one_macro`, left 4, right 2. Native reports
line 4 and carries the definition as a note; the fixture now uses that native
primary location. The first historical replay selected a comparator that rejects
retained native timeout records. The established consumer comparator preserves
those audit outcomes and explicitly checks that no predecessor matches are lost.
Failed run logs are retained alongside successful replacement evidence.
