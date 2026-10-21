# File-operand source-order baseline

The portable native probe reproduces all 200 sources/configs/statuses/bytes with
unchanged tool hashes. Native accepts 170 and refuses 30; the frozen corrected
scalar producer differs on 63. All original 100 outcomes remain in this matrix.

In project::sweep_line, include/incbin still take the whole-line live_text path.
Their filename constructors therefore use the old pure numeric context. Later,
Walker::read_line binds the directive label and calls walk_directive before the
selected data callback. Incbin then folds numeric fields with SourceConstants.
Neither filename constructors nor their nested numeric source queries share the
ordered SourceContext used by data/scalar consumers.

Extend the existing private callback's result with the already existing
WalkDirective alternative. The ordinary reader remains responsible for label
binding, spans and trivia before invoking it; the existing resolver remains
responsible for opening files, suspended include order and byte windows.
Walker::walk_directive receives the optional existing DataSource adapter.
Include names expand at that position; incbin expands only the first filename
field, then invokes checked source-number folds on offset and size in order.
The pure/formatter path keeps its current default. No public AST/API, namespace,
file resolver, dependency or binding decision changes.
