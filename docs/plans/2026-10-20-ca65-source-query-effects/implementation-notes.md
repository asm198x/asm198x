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
