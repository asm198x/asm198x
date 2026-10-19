# Generated block heads (#861)

Fresh native ca65/ld65 controls: 99 cases, 64 accepted and 35 refused. The frozen
formatter producer 6283a1b3638908b6851be800cf1b9f06d38d78b8 (CLI SHA-256
c6d24d6aa5ee2e94561c78285332344c8713be16bf11de48ce5fadde7134316f)
reproduces the generated-opening failures. The structural classifier strips only
static labels; selected `read_block_head` already performs text evaluation and
binding in source order. Structural recognition must use the existing balanced
source helper without folding arguments or reading includes.

The matrix also exposes native-invalid literal registers/size prefixes and
instruction names falsely accepted on static block heads. New generated-head
admission must not make generated instruction names valid. Keep selected
validation at the block head: distinguish explicit generated register identifiers
from literal tokens, use the active CPU policy and existing instruction-name
predicate, then bind through the existing mechanism. The 115 end-label controls
must remain unchanged if the same private policy is shared.

Native also accepts six static/generated labels on .endif/.else/.endrepeat that
the current assembler loses or refuses. Retain those as a separate closing-
delimiter issue; this opening-head slice must not silently consume their labels.
No public AST/schema change or new reader architecture is proposed.
