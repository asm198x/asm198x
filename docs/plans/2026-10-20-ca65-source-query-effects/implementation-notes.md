# Native controls before #795 implementation

The extended audit retains 128 cases: 114 accepted and 14 refused by native
ca65/ld65. The frozen counter-fix CLI differs on 69, with exact classifications
in extended-summary.json. All original 56 cases remain unchanged.

The new controls include missing and forward names, explicit outer and cheap
names, shadowing, alias chains/cycles, label differences, numeric query arguments,
nested constructors, repeats, inactive source and include/macro placement.

Native sprintf reads its arguments in order: formatting VALUE before .ref(VALUE)
produces "21"; reversing those arguments produces "02" and leaves the subsequent
query true. Folding all inner calls before reading earlier numeric arguments is
therefore insufficient even after whole-line query ordering is repaired.

The existing text fold_once walks innermost calls before parent arguments. The
new context must reach both the ordinary expression parser and numeric arguments
at their actual read positions. The existing scope_syms reference path and
Deferred definition graph remain the authorities; neither a flat prescan nor
a parallel name table can answer scoped constancy correctly.

## Confirmed integration seams

SourceConstants already borrows the live Walker for allocation folds. It reads
scoped references through scope_syms and evaluates against reader.consts without
copying the namespace. Reuse that view for source-aware value parsing.

The ordinary read enters through project::live_text then Walker::walk_line.
read_block_head uses the same live_text path. walk_line handles assignment RHS,
records, scopes and label anchors before calling parse_op. parse_op passes
SourceConstants into parse_directive and its allocation helpers, but ordinary
parse_value_list/data-list and instruction operand parsing still use the pure
parse_value entry. Thread the source expression seam through these actual
consumers; preserving only the condition reader would miss data and operands.

Reader::source_value currently parses with the pure expression core and only
then applies scope_syms. A parser hook must bind ordinary symbols as they are
read while leaving name-only query arguments free of ordinary read effects.
The current pure entry remains the formatter and other dialects' path.

The text scanner's fold_once/has_call recognition probes call function with an
empty argument vector at line zero. These probes must never execute live
lookup, reference creation or constancy callbacks. Actual source errors must
retain their span and classification through the linked line wrapper.

Deferred stores both definition.value and definition.source_value, with source
width/cycle classifications. The old segment_weights helper sees constants and
labels but does not follow that definition graph. Native alias constancy must
use the existing source graph, including its unresolved-current-scope rule,
rather than a second alias map or final layout values.

Nine additional alias-classifier controls are in alias-native.json. Native
recognizes unary cancellation and a label multiplied by zero as constant,
while a forward difference is still nonconstant at the earlier query. A missing
name cancelled against itself still fails assembly. Five of nine differ in
the frozen predecessor. The final source classifier must cover these boundaries
alongside the 128-case source-order matrix.
