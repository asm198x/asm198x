# Source token cursor for generated ca65 fields (#795)

Status: proposed; no cursor implementation yet. The existing #795 expression
context remains approved. This proposal adds a shared prefix/token cursor for
field consumers, beyond the current one-expression callback.

## Measured requirement

The 40-case native matrix matches checked #920 completely. Scalar candidate
b69d510f regresses 18 cases and the following file trial regresses 21. Token
selectors generate commas, count/counter fields and condition/action fields.
`.res .left(3,{1,2})+1` means count 1 and fill 3. Selector argument effects must
still occur after any preceding ordinary symbol read. Earlier 320/200 native
controls, 1,644 workspace tests and five actual native archives did not cover
this boundary. Those attempts must not merge.

## Recommended: extend the existing expression reader

Use mos6502::ExprParser and its existing grammar, with an opt-in source token
cursor and expression-prefix entry. Keep parse_expr/parse_expr_source as strict
whole-expression wrappers for existing callers. Do not duplicate precedence,
operator parsing, scopes, text grammar or byte-string ownership.

The cursor owns unread tokens, position and original source/macro provenance.
At a selected Tok::SourceCall, a checked source hook may return native source
tokens (selectors/constructors) rather than exactly one Expr. Insert those
unread tokens at that position, preserving the surrounding tokens and opaque
StringTokens handles. Ordinary symbol/query hooks still run as each expression
is consumed. Do not expand later fields or re-admit computed bytes as physical
quoted source. Bound expansion with the existing text expansion limits.

A prefix parse returns the expression and the same cursor stopped at its real
separator. The ca65 field reader then consumes the separator and the next
numeric/name/action/string field from that cursor. Generated commas therefore
participate in the outer grammar; arithmetic after one stays in the next field.
A complete expression wrapper still rejects trailing tokens. Capture and lazy
branch rules, file opening, pending include order, layout and public Expr/AST
contracts stay unchanged. Generated identifier provenance must remain available
for existing admission rules; #918/#923 remain separately inventoried.

Exact seams: mos6502.rs ExprSource/tokenize/ExprParser and strict parse wrappers;
ca65/mod.rs parse_value_source/DataSource/directive fields; project.rs
SourceContext and repeat/conditional/data/file consumers; ca65_flat.rs existing
text-call/token selector adapters only where required by the checked hook. Keep
other dialects on their current pure defaults and existing expression grammar.
The failed private file callback extension can be reused only after the cursor
contract is proven, without moving the resolver or adding a public AST variant.

## Alternative

Introduce a complete ca65-family token reader above the shared numeric parser
and migrate all field consumers together. This would be a larger reader rewrite
with more grammar and formatter migration risk. Prefer extending the existing
parser and preserving its strict entry points.

A selector exception or whole-line expansion is not an alternative: it either
moves effects early or leaves valid generated fields broken.

## Plan and verification

1. Before implementation, expand the native matrix for left/right/mid selectors,
   nested/empty/generated delimiters, suffix arithmetic, preceding symbol reads,
   selector count queries, generated names/actions/messages, and malformed/lazy
   controls. Retain every old failure mode and explicit audit dispositions.
2. Prove a private prefix cursor through the existing expression grammar before
   migrating one bounded consumer. Verify the strict wrapper rejects trailing
   fields while the prefix reader leaves the exact unconsumed tokens. Test
   source order with native bytes, not expectations copied from the parser.
3. Migrate one consumer at a time; keep unsupported consumers on the existing
   checked behavior until their complete contracts pass. No new namespace,
   dependencies, layout strings, or public AST/API changes.
4. Preserve all matches of checked #920 and all earlier retained source-order
   matrices. Add mandatory generated-field package evidence and omission controls,
   native message/debug/formatter/refusal/output-preservation checks. Run full
   Rust/Lua, Clippy, Node, docs/version/ecosystem, fresh4510, actual five-host
   archives and all exact-source checks before merging a replacement.

The supplied AGENTS.md requires agreement before "Introducing new architectural
patterns" and significant convention changes. The original approval covered the
source expression context; this shared token-prefix cursor is a concrete added
reader contract. Ask for agreement before implementing it. Independent #921 can
continue from checked #920 meanwhile.
