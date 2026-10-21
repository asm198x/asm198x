# Generated fields block the ordered-reader candidates

All 40 new native cases match checked #920. Native accepts 37 and refuses 3.
The scalar b69d510f candidate loses 18 previous matches; the following file
trial loses 21, despite fixing all 63 of its initial 200-case differences.
Both candidates are rejected for merging. #922 is a draft. No code from either
attempt has reached main. The file trial is retained as an apply-checked patch
and exact source/executable hashes; its source edits were rolled back.

Native token selectors return source tokens, including field separators.
`.res .left(3,{1,2})` emits 02 on native and checked #920. Splitting a field
before reading the selector turns the generated comma into an expression error.
The same problem affects repeat count/counter fields, assertion condition/action
fields, alignment arguments and incbin windows. Arithmetic after a generated
separator belongs to the next field, so expanding the selector and parsing only
its first expression cannot repair the problem. A whole-line effect pass would
also move selector argument reads before preceding names.

mos6502::tokenize currently builds Tok::SourceCall, whose atom callback returns
one Expr. parse_expr_source requires complete token consumption. These are the
verified seams: a source prefix/token cursor must expose generated separators to
the outer field reader at their actual read position. Another string splitter
or ad hoc selector exception would preserve the mechanism that failed.

The engineering rollback rule applies after these two candidate revisions.
Return independent implementation to checked #920; the next reader attempt
needs the concrete token-cursor design and all failure modes pinned first.
