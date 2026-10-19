# Preserve native lexical numeric checks in skipped source

The 72 native controls accept 48 and refuse 24. Decimal tokens accept through
4294967299, including 4294967296 for an empty tag; they refuse 4294967300.
Hexadecimal and binary tokens stop at 0xFFFFFFFF. Leading zeros are allowed.
Arithmetic cancellation and zero-size tags do not remove lexical failures.
Every oversized token is also refused inside `.if 0`.

Native scanner.c checks hexadecimal/binary overflow while consuming digits.
Decimal/suffixed hexadecimal conversion checks the previous partial value
against floor(0xFFFFFFFF/base) before each digit; this explains the unusual
decimal endpoint. Match the observed scanner rather than imposing a final
32-bit arithmetic or field-count limit.

This conflicts with the broad wording in decisions/parse-affecting-directives.md:
“An untaken branch is never tokenised at all”. Its purpose—prevent semantic
reading under the wrong text environment—still applies. Numeric token overflow
is an earlier lexical error that native preserves even when meaning is skipped.

## Proposed binding clarification (awaiting approval)

Append a narrow exception to the existing decision:

> Native lexical failures can occur in untaken source. Validate numeric tokens
> at the character/token admission boundary, preserving quoted text, comments,
> source locations and native macro/repeat capture order. This does not parse
> directives, evaluate expressions, resolve names, expand inactive text symbols
> or open inactive includes. The selected-source rule continues to govern those
> operations. Decimal/hex/binary limits are pinned by #788's native fixture.

## Implementation after agreement

Use the existing ca65 token lexer/reader boundaries. Validate authored numeric
tokens where native consumes them, including skipped conditional text and
macro/repeat definitions, without invoking the expression parser on skipped
source. Reuse the same token limit check for active linked/flat CPU paths.
Retain source spelling and formatter output; no AST/schema/dependency changes.

Before code, extend controls for quoted/comment digits, text definitions,
unused/called macros, zero-repeat bodies, `.end`, missing inactive includes,
include error spans and available ca65 CPU paths. Bound every probe. Verify
native-valid large empty-tag counts and existing selected-source parity cases.
Do not broaden semantic parsing in an untaken branch to obtain this check.
