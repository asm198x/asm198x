# Absolute debug coordinates (#814)

Native ca65/ld65 accepts 38 of 39 retained programs and refuses the duplicate
definition. All 39 outcomes and image bytes match the preceding executable at
f67f95885472337f2934145037de3de41993e51e, but 32 programs have metadata address
differences. The regression first fails with item resolving to 4096 rather
than native 32. Native debug output retains 121 label values and physical source
spans; 213 captured source ranges agree with those spans.

The placement pass already uses the correct logical PC after #822. Capture
still used each configured segment's physical offset, so an absolute origin
changed expression values without changing debugger addresses. Private logical
spans now retain both coordinates, reserve all configured IDs and rebase labels,
lines and cycles together. Non-CPU addressability still comes from configured
ranges; the HEADER fixture explicitly overlaps RAM to exercise that policy.
It does not infer addressability from a segment name.

Native listing and budget accounting now share physical routine intervals
within a named source segment, restoring each label's logical coordinates for
rendering. .org/.reloc do not truncate a routine; different source segments
remain independent. No missing file offset becomes zero. The capture adapter
groups records once by ID instead of scanning all records for every new span.

The 39-case regression covers native label kinds/values, sparse IDs, zero bases,
physical spans, constants, aliases, includes, macro/repeat expansion, non-CPU
and unmapped sections, metadata-only labels, unchanged no-origin records,
official Debug198x reading/lookups and formatter round trips. Fixed routine
totals agree in text/JSON; nine budget pairs pass at the exact ceiling and fail
one below it while preserving five outputs. Included assertions also retain
line 6 provenance in human/JSON modes. Existing qualified-name budget rejection
is separately tracked in #827; its listing total remains tested here.

Local verification: 1570 Rust tests passed, 38 ignored; 82 Node tests passed;
Clippy with Lua/all targets and warnings denied, fmt, Ruff and docs passed.
The actual archive passes all 67 mandatory cohorts; the new cohort executes
172 commands. The preceding archive fails only the new cohort and passes the
other 66. Wrong-byte corruption fails exactly reference_bytes, determinism and
structured_success; corrupt-archive SHA validation also fails closed.

#824 merged while independent CI changes landed. Its merged assembler patch
is byte-for-byte the tested patch on top of that concurrent CI commit; its
post-merge run was cancelled by newer CI work. Its completed exact-head
pre-merge five-host qualification remains retained. No foreign worktrees were
changed or removed.
