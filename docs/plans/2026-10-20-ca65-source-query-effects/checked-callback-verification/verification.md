# Checked numeric callback verification

The shared text numeric callback can now return an existing AsmError. Pure
TextSyntax implementations retain their existing evaluate method through a
default adapter. Scope.number propagates checked failures without replacing
lookup spans, macro frames or the original diagnostic with a generic fold error.
The regression drives the real expansion path and verifies the full span;
pure literal and nonconstant controls retain their earlier behavior.

All 1,637 Rust/Lua tests pass (38 ignored), as do all 114 Node checks, all-target
Clippy, rustfmt and Ruff for the retained probes. Exact logs and hashes are here.

This is the first implementation seam for approved #795. It does not connect
live source evaluation yet and does not resolve the retained wrong-byte cases.
Source-order consumption, scoped constancy and the existing Deferred graph
remain required before #795 can close.
