# ca65 active-layout extents (#609)

The current source reproduced the report: a configured CODE area filled with
32,768 bytes failed with `segment CODE (32768 bytes) overlaps VECTORS at $FFFA`
(names are backtick-quoted in the actual diagnostic). This layout has no
VECTORS segment. The failing regression is retained in `red.log.gz`.

The hard-coded name/address guard has been removed. Actual lengths are checked
against resolved segment starts and the owning area's bounds before emission
and before calculating memory usage. This also prevents oversized file areas
from being silently truncated by final-image resizing, and catches RAM overflow.
The shared placement engine still owns file placement. Separate memory areas
can share CPU addresses, as native banked layouts do. The default layout keeps
its vector reservation and a diagnostic naming both segments, address and area.

## Independent reference

Native `/opt/homebrew/bin/ca65` and `/opt/homebrew/bin/ld65` both report V2.18
(the Homebrew cc65 suite is 2.19). Executable SHA-256:

- ca65: `1a57997819fd98814e4601b5160df589e7a1a24e1888a80fee51ae450d8fd0ea`
- ld65: `36bb196207a6b2c90a4aa991b419d6113885303951b14b60b455f51c777b3339`

`crates/asm198x/tests/ca65_config.rs::extent_cases` defines the exact inputs and
expected final images. The ordinary test checks the library; the ignored native
test assembles objects with ca65 and links final images with ld65. It compares
whole images for accepted cases and linker refusal for rejected cases. Objects
are never compared with final images. This measures placement, not NES booting.

The cases cover CODE and renamed PROGRAM filling the area, embedded vectors,
real area overflow, a pinned TABLE boundary and overlap, a spill into another
file area, separate banks sharing CPU addresses, and RAM overflow. The default
NROM boundary and helpful overlap diagnostic have a separate regression.

## Release gate

The existing native archive qualification now requires three additional cases:
`pasmo_word_arithmetic`, `ca65_configured_extent`, and `ca65_extent_refusal`.
Arithmetic uses seven independent native facts retained by #634, on both Pasmo
frontends. Configured ca65 payloads compare all 32 KiB including embedded
vectors under two segment names. Negative layout checks preserve the binary,
symbol, listing and debug outputs. The exact-case qualification guard and its
missing/failed/duplicate/unknown-case controls remain in force. Previous public
0.0.67 evidence is explicitly insufficient for this 27-case cohort.

## Verification

Lua-enabled workspace tests: 1456 passed, zero failed, 37 ignored. Strict workspace/all-target Clippy with Lua, rustfmt, and generated documentation checks passed. All seven ca65 configuration tests passed when the two native comparisons were enabled, covering eleven extent cases (six accepted, five refused). Node qualification/setup/provenance controls: 37 passed. Checked workflow regeneration passed. Logs are retained beside this report.
