# Native unary-plus admission audit

The allocation replay preserves every checked match but three canonical-field
observations still refuse native `.res .left(2,{1,2})+1`, which emits 01.
The generated operands are `1,+1`; the shared numeric parser has no unary-plus
branch. Before changing grammar, measure physical unary plus, nested/product
precedence, generated fill, character/query values, an incomplete operand and
inert captured tokens across all three ca65 CPUs and five source contexts.

Capture exact native processes and bytes with the existing reference harness;
compare the current checked allocation implementation without changing it.
Search existing issues before filing a distinct arithmetic admission bug. If
confirmed, keep any fix on the shared operator grammar and gate it to ca65;
other dialects retain their existing grammar. Do not implement a second parser
or waive unrelated inactive flat-reader gaps. Retain all evidence before cleanup.
