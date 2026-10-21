# Linked numeric-condition input checkpoint

SourceContext::read now uses one strict numeric expression over the same owned
input as data/allocation/repeat fields. Generated separators remain visible;
conditional grammar rejects additional fields. Existing retain_read provides
scope binding, required names and earlier-definition/source-span preservation.

Verification passes 779 library tests, all 32 affected source/deferred,
repeat/end, lexical and macro contract tests, Clippy -D warnings and formatting.
The 39-matrix / 7,309-row unchanged-executable replay preserves all 6,865 checked
matches. Conditional coverage remains 197/200: the three existing generated
label-size refusals were not misreported as fixed by changing the input stream.
All successful receipts are retained in condition-checks/.

Name-only header, file, flat and syntax adapters and final full/archive
qualification remain. The owning #924 reader branch is not merge-qualified.
