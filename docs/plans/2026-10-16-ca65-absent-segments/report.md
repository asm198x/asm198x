# Absent custom segments (#704)

A native follow-up found a regression in #703 before completion of the release
checkpoint. A full PROGRAM area with a configured, absent TABLE at $FFFA is
accepted by native ca65/ld65 and public 0.0.67, but #703's validation rejected it.
The failing current-source regression is retained in `red.log.gz`.

Native ca65/ld65 V2.18 also distinguishes absence from zero length: explicitly
selecting TABLE with `.segment`, a label or `.res 0` makes ld65 reject its pinned
start behind PROGRAM's end. An absent aligned segment does not move the cursor
for the next floating segment. Checking only emitted byte lengths loses this
information.

The existing projection now records explicit segment selections, in the same
selected-branch/include walk as its statements. Custom layout sizing includes
those selections as zero-length entries. Resolution and final extent validation
skip absent declarations; explicitly selected empty segments still participate.
The built-in NROM reservation remains unchanged. Public types, placement engine,
Debug198x schema and debug section capture are unchanged.

The native comparison in `crates/asm198x/tests/ca65_config.rs` now covers sixteen
extent cases: eight accepted full-image comparisons and eight native refusals.
The seven configuration tests include default NROM boundary/error checks and
the existing floating layout differential. Native tool identities are the same
freshly measured V2.18 binaries retained in the #609 report.

The native archive gate now requires 28 cases. The new `ca65_absent_segments`
case checks the absent pinned segment, explicitly selected empty refusal with
output preservation, and an absent aligned segment before a floating tail.
Public 0.0.67 fails exactly the four new arithmetic/layout cohort cases; the
other 24 still pass. This is a negative control, not qualification of 0.0.67.

## Interrupted 0.0.68 publication

Release PR #702 merged as `eecde0674b8be684a3b14fe75301a76f0d24e557`, after all
36 exact-head checks and seven triggered workflows completed successfully or
were appropriately skipped. The checked PR tree equals that merge tree.

The subsequent native Release run `38903970383` was cancelled after discovery
of this regression. Its host job is cancelled with no steps executed; its
retained timeline is `cancelled-0068-timeline.json`. The immutable 0.0.68 tag
and published crate are preserved. Native archive publication was not completed;
0.0.68 is not claimed as a qualified public native release. The checkpoint moves
to a subsequent patch containing this correction.

## Verification

Lua-enabled workspace: 1456 passed, zero failed, 37 ignored. All seven configuration tests passed with native tools enabled. Strict workspace/all-target Clippy with Lua, rustfmt, generated documentation and checked release-workflow regeneration passed. All 37 Node qualification/provenance/setup control tests passed. Red, native and broader logs are retained here.
