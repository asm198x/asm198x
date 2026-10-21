# Token-classifier extraction

The ca65-family library tests pass 120/120; the Lua-enabled CLI builds and
all-target Clippy passes. All 35 frozen #925 matrices, 5,659 cases, retain
exactly the same native-match verdicts (5,417 matches). The 242 remaining rows
are existing audit gaps, including duplicated contexts; this is not a claim
of complete native parity. The source classifier is unchanged apart from its
module location, visibility and the final blank line rustfmt removes.

This is the first preparation commit for the approved full reader redesign.
The numeric parser, source effects, scopes and field consumers are unchanged.
The existing shared classifier now lives in ca65/reader.rs and current token
functions consume it. No second token grammar has been introduced.

The frozen baseline executable is the checked #925 source,
`1c7e89d6321ebcc394af15473f6894830aec8e02`, SHA-256
`731d26ee912455c7bdaf39a3529f72ba573e4af7da8121e37046c936a714a2af`.
#925 main is `ee522abd40a0bfa7a61b5581679679690717ccff`, with the identical
qualified source tree. The old #920 artifacts and completed diagnostics have
been retained and cleaned. The approved migration reuses the sole linked task
worktree and the two existing caches. Full-reader qualification remains future
work; #925's five native archive receipts do not qualify this new commit.
