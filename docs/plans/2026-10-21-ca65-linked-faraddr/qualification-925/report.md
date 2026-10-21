# Qualified #921 source

Source `1c7e89d6321ebcc394af15473f6894830aec8e02`, tree
`aa6530bbd71a11c582a43ec1dac6ed60334c56a3`, checked base
`15a2c6dc3492a88f77495e4fa162189e43870c82`. All five actual native
archives qualify 105 mandatory cohorts. Each runs 598 new far-address commands
and both corruption controls. The virtual checkout is
`4fabd2a753281d6c751a19f783c536b5b0d44568`, with source as its second parent
and exactly the source tree. Every committed/before/after lock hash is
`0ee4b612d610b2d581a5b32e1666f431bdfcf55b99cfaed65841aa48e369958a`.

The complete Rust/Lua workspace passes 1,644 tests in 110 binaries, with the
existing 38 ignored tests. All 115 Node tests, all-target Clippy, formatting,
docs, versions, ecosystem validation and ledger checks pass. Fresh4510 verifies
564 native bytes, fourteen streams and a rejected changed-byte control. Its
disposable work directory is gone.

The local archive qualifies all 105 cohorts; deliberately wrong bytes fail
exactly reference_bytes/determinism/structured_success, archive corruption
fails its checksum, and checked #920 fails only ca65_linked_faraddr. Local
receipts are artifact-only; the five CI receipts verify producer locks.

All 4,791 historical cases preserve predecessor matches; both debug and frozen
release CLI were replayed. The 578 additional reader controls also preserve
predecessor matches, including all 40 generated-field controls. A further 100
canonical-reader controls remain unchanged (91 matches, nine known #795/#924
differences). None of rejected #922's reader code is in this source.

The manifest retains uncompressed and stored hashes for complete process
reports, successful checks, failed diagnostic-expectation/preflight attempts,
source identities and replay results. Native summary records every archive
and executable digest. Exact-source checks completed successfully; final PR
metadata checks must complete after the final description edit before merge.
