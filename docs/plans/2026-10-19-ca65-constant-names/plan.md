# Validate ca65 constant names against native tokens (#773)

The shared MOS identifier helper accepts dots for other dialects. The linked
ca65 constant reader reuses it and accepts `.bogus=7`, which native ca65 refuses.

1. Retain bounded ca65/ld65 declaration probes in probe.py and
   tests/fixtures/ca65-constant-names.json. Check plain names, underscores,
   digits, dot-prefixed reserved tokens, dots, qualified definitions, scopes,
   includes, macros and repetitions before changing code. Retain separate
   native probes for cheap locals, register/mnemonic names and untaken lexical
   errors as deferred gaps; do not change those mechanisms in this slice.
2. Tighten only the linked ca65 declaration-name check in constant_key in
   dialects/ca65/mod.rs, using measured native token rules. Preserve authored
   names, existing scope resolution, formatter behaviour and lazy selection.
   Keep the shared MOS grammar unchanged. Add bounded CLI/native/formatter/debug
   and output-preservation coverage in tests/ca65_constant_names.rs.
3. Add required archive replay and omission control in the two candidate
   scripts. Run Rust/Lua, Clippy, fmt, Ruff, docs, Node and real Lua-enabled
   archive checks with both corruption controls; retain a clean 4510 replay.
4. Rebase onto the merged introspection fix, push a focused PR, inspect all
   exact-head checks and five actual package reports, squash merge, verify
   closure/retained producer commits and clean the finished worktree.

Follow syntax-stance.md, parse-affecting-directives.md and the existing scoped
constant decisions reflected in the prior scoped-constant plan. This is a
small native validation repair without dependencies or public shape changes.
