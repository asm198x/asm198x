# Completed precommit data verification

The complete Lua workspace suite passed 1,643 tests with 38 ignored. Focused
Rust tests, Lua Clippy, formatting, 115 Node/setup tests and docs/version/ecosystem
checks pass. All 515 data controls match native in debug and isolated release
builds. Every previous native match survives across 26 matrices / 4,136 cases.
These are precommit verification records; native archive and exact-head CI
qualification remain required before merge. The numeric audit still has 30
differences across 50 cases.
