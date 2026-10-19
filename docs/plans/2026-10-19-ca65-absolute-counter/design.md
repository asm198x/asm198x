# Track the native absolute cursor separately from placement

Pinned native segment.c increments global AbsPC for every fragment when default
absolute mode is active. GetPC returns that cursor, independently of the active
segment's physical PC. A segment switch does not translate the absolute cursor.
The current constant bias works in one segment and produces wrong values when
linked segment bases/offsets change.

Replace pseudo plus absolute_mode with Option<i64> absolute_pc in the existing
placement loop. Before any mode update, use its value (or the normal linked
address in relative mode) for labels and deferred definition positions. Org
uses that same prior PC when evaluating its argument, enforces the retained
24-bit setter bound and replaces the cursor. Reloc clears it globally. Emit
uses the cursor after mode handling; each resolved statement advances it by the
same size already advancing that statement's physical segment offset.

Keep physical alignment based on the active segment's PC, as native SegAlign
uses ActiveSeg->PC. Keep per-data-operand increments and instruction-PC rules
as they stand. The existing constant-bit-branch guard tests cursor presence.
Do not introduce native optional OrgPerSeg mode: it is not the default and the
feature is not currently dispatched. No extra pass, dependency or public type.

Debug symbols still retain physical coordinates here, a separately approved
#814 correction. Numeric source-time origins remain #815. This slice corrects
emitted symbol/counter values so that metadata can later be compared with truth.
