# Configured far-address range audit

The selected data-query audit identified a separate range gap. Keep the #920
source under qualification unchanged; retain this audit on its own commit/ref.

1. Capture the native 24-bit endpoints, negatives, overflows, forward/backward
   expressions, numeric source queries, byte/word/extraction neighbors, included
   and macro inputs, inactive branches and unused definitions. Retain 90 sources,
   configurations, native streams and ca65/ld65 executable identities in
   native.json. Compare both the qualified #810 executable and data candidate;
   preserve before.json and after.json with exact binary hashes.
2. Keep probe.py location-independent within the plans tree. Re-run its native
   capture and compare every source/configuration, exit and byte result with the
   exploratory capture. Inspect ca65/mod.rs's actual faraddr lowering and the
   approved flat range design. File a separate GitHub issue with concrete
   symptoms, forward cases, counts and immutable evidence links.
3. Before a fix, identify every private Kind consumer and the existing unsigned
   Sized emission path. Preserve the original expression until final layout;
   a parse-only check misses forward values. Reuse the approved unsigned policy
   and existing data conventions, preserving extraction behavior and lazy source.
   Write the bounded implementation/files/tests plan before code changes. Do not
   introduce public schema, dependency or object-linker changes for this bug.
4. Require all native controls, formatter/debug/output-preservation invariants,
   every retained native match, full Rust/Lua and actual five-host qualification
   before any fix is merged. The current audit changes no assembler behavior.
