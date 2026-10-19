# Proposed logical debug sections for absolute ca65 spans

Status: proposal awaiting design agreement. No implementation yet.

The existing producer emits one debug section per touched configured segment.
Labels, lines and cycle records share those section-relative offsets. After
`.org`, expression values follow the absolute PC while those debug locations
still follow physical segment offsets. The retained #814 example therefore
emits an item value of $2000 but resolves its debug label to $1000.

## Recommended representation

Use the existing Debug198x section/address and SectionDebug file-offset fields.
Append logical debug sections for absolute spans rather than adding a format
variant. A span has its absolute logical base and the physical output offset
where its bytes begin; label, line and cycle offsets are rebased together.
Original configured segment IDs remain reserved and unchanged. New IDs come
from after that range, including when only a sparse subset of segments exists.
`.reloc` returns to the original segment-relative coordinates. A label on a
mode directive remains in the context before the directive (#812).

The file bytes, physical placement/memory account, file IDs and source line
numbers do not change. Physical file offsets retain the mapping back to those
bytes. No hardware address space is invented; existing space/addressability
rules must be explicitly preserved. No public schema or dependency changes.

Absolute counter changes inside or across segments require separate span
contexts. Debug sections may be metadata-only when a label has no emitted
bytes; no missing file offset may silently become zero. No-origin programs
retain their current debug sections and IDs.

## Why the smaller alternatives are insufficient

* Reclassifying a label as Const loses the address kind: Debug198x explicitly
  distinguishes an address from a plain value.
* A zero-based section containing only absolute labels fixes symbol lookup but
  separates the labels from their physical line/cycle records. The existing
  DebugCaptureMulti.section_debug and label_costs_debug group by section ID;
  routine costs and budget lookup would lose those labels.
* An AbsoluteLabel protocol variant requires a separate format change and
  consumer agreement. The current section/file-offset model already has both
  coordinates needed for this correction.

## Required validation before merge

Resolve each sidecar label through its section and compare it with emitted
native words, including origins below the configured base and labels on mode
changes. Preserve file-coordinate mapping, formatter round trips, included
provenance, sparse/config section IDs, cycle ranges and both passing/failing
routine budgets. Test `.org`/`.reloc` within routines and be explicit when the
native logical label no longer identifies instructions in the same span; do
not invent a zero cost or silently relax a budget. Inspect non-CPU/file-only
areas before extending the representation there. All existing package cohorts
and five-host qualification remain required.

This changes the present one-debug-section-per-configured-segment convention.
The supplied AGENTS.md requires asking before significant deviations from
existing conventions; agreement is needed before implementation. Independent
origin-validation research (#815) can continue while this proposal is reviewed.
