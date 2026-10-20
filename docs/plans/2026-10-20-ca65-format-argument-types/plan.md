# Audit native numeric and string .sprintf argument types

#884 found numeric selector positions accepting decoded string values. Audit
.sprintf separately before assuming it has the same type rules or mechanism.
PR #885 qualification continues independently on its fixed shipping head.

1. Capture native tool identities, complete inputs/processes/images in probe.py
   and native.json here. Cover numeric conversions and %s, numeric/string/char/
   nested/parenthesized arguments, mixed formats, define/macro/include/selected
   source timing, scope controls and constructed quote/backslash byte values.
2. Compare with frozen clean producer 3518aaa314a7b6632b545c1f75d047cf14fddc86.
   Classify wrong acceptance, wrong bytes and existing scoped visibility gaps
   separately. Do not assign a mechanism from a format's name alone.
3. Read the .sprintf implementation in crates/asm198x/src/dialects/ca65_flat.rs
   and the shared Scope/Arg behavior in dialects/text.rs against observed native
   outcomes. File verified new symptoms as GitHub issues, with exact reproducer,
   producer/tool identities and retained evidence; update existing issues when
   they already cover a boundary.
4. Choose and write a focused design before any production edits. Reuse existing
   ca65 type-admission seams when supported by evidence; keep generated-string
   representation and wider lexical/capture changes separate if they require
   significant architecture or convention changes and design agreement.
