# Capture ca65 text at native token boundaries

Native ca65 captures existing text names when reading a macro definition and
reads arguments at invocation: the body/argument control emits `0102`, while
the old packaged executable emits `0202`. Repeat bodies likewise capture
active names. Counters are tokens in their owning stream, so a caller counter
cannot leak into an unrelated macro body or appear through `.ident("I")`.

The linked 6502/4510 reader now uses the existing live macro namespace and
one-layer expansion mechanism in its selected projection sweep. Complete macro
bodies remain opaque to structural grouping; untaken branches remain unread.
Private capture/loop provenance keeps token substitutions in their source
stream. Existing macro frames now survive statement placement and backend
errors. No public AST, span, diagnostic, API or schema shape changed. Flat
65816/HuC6280 retain their previous path pending their own probes.

`probe.py` and the fixture retain 51 ca65/ld65 V2.18 cases: 35 accepted and
16 refused. CLI tests compare native bytes, formatting idempotence, reassembled
bytes and section/symbol debug shapes. Include/macro error controls exercise
parsing, selected expressions, layout and emission in human/JSON form and
preserve all previous outputs. Every CLI child has a five-second kill/reap bound.

The actual previous archive fails the new native-byte assertion (`0202` versus
`0102`). The optimized Lua-enabled local archive passes all 47 required cases,
including 156 commands in this cohort, and rejects wrong bytes and corrupt
archive checksums. Compressed reports are local preflight evidence without a
producer-lock receipt; five-host CI qualification remains required before merge.

A paired five-run density check caught namespace copying per source line. At
500 definitions/10,000 instructions, that version took 1.171s versus 0.111s
previously. Querying the existing namespace removes those copies. The repeated
paired median is 0.136s versus 0.110s; all 10,000 emitted bytes match on every
run. Retained samples include 0/50/500 definitions and executable/source hashes.

Local verification after optimization: 1,525 Rust tests passed, 38 ignored;
62 Node tests passed. Clippy with warnings denied, Rust/Python formatting,
Ruff and generated docs checks passed. Complete logs are compressed alongside
native failures and archive negative controls.

Clean producer `1d516f6c90675a21927b1aa2cceb6a9230f5d01d` matches all 564
unchanged upstream/native 4510 bytes, SHA-256
`8a5fcde630e55dd49403f7c6ffed25d8786e65e3d0a2723531b7ca3e2c0509ec`.
`4510-after-text-capture/` retains full comparison bodies and fourteen raw
streams. The verifier checks all identities, completion and content and rejects
a changed byte. The full ecosystem ledger remains untouched.

Further native probes found quoted/case-sensitive introspection replacement
errors (#770) and premature body-query evaluation before live text alias
expansion (#771). Those form the next focused slice; `.set` and `:=` support
remain separately tracked as #761/#762.
