# Separate file packing from memory occupancy

Retain the parsed segment type as a private initialized flag. Both zp and bss
occupy memory without writing segment bytes. Keep the existing CPU cursor for
addresses, alignment, overlap and capacity, and add a separate per-area file
cursor. Alignment/start gaps contribute when a segment writes or the area is
filled; segment length contributes when initialized or filled. An unfilled
area's file size follows its file cursor, not its occupied memory extent.

The native 216-case matrix pins fill, shared-area behavior and alignment/start
gaps (152 accepted, 64 refused). Filled areas
retain their configured span and fill bytes; unfilled areas omit BSS data even
before later initialized data. Existing shared lay_out remains the serializer.

Before omitting non-file content, validate its expressions through the existing
emitter into a bounded temporary buffer. Skip raw/reserved payloads that need
no expression evaluation; keep diagnostics and visibility checks. This prevents
a file-packing fix from accepting bad ranges or undefined names.

Direct PC instruction-width differences (#806) use the existing source-time
width query: pass the current segment's byte-address context for `*`, just as
alias definitions already do. Explicit absolute forcing remains authoritative.
Retain every completed native case, including direct PC and forced-width controls.
No public shape, dependency or new linker is introduced.

Native initialized-data warnings are tracked separately in #807. The earlier
mixed-fill captures remain raw research evidence: per-area differing fill values
are outside the existing one-fill image contract and are not qualification cases.
