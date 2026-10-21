# Canonical token ownership and admission

Keep the shared native TokenKind attached to each source token through
construction, capture and selection. Physical A is a register token; .ident's
result is an identifier token even when its spelling is A. Selecting a physical
token keeps its kind; selecting a constructed identifier keeps that identifier
kind. The two retained provenance matrices establish the distinction and expose
#876, #918, #923 and the separately filed byte/query admission gap.

A token owns a shared immutable source spelling plus its byte range. Clone the
source ownership when tokens move between input/capture/expansion frames;
selection clones tokens rather than rendering and re-lexing their spelling.
This is private reader data. AST, Expr, scope keys, StringTokens and public
contracts retain their existing ownership and meaning. A token's quoted text
or private string handle remains one token; arbitrary cooked byte strings are
not serialized into new physical quoted source.

The shared numeric parser borrows a token source. Numeric lookahead must cache
one token and must not repeat effects. Advancing discards that token without
reading ahead. Ordinary identifier consumption must perform its source read
before expansion of a later function or field. Keep function-name lookahead
separate from an ordinary identifier's consumption; the current numeric Sym
branch checks following parentheses before its symbol callback, so a canonical
input must not make that check expand later source prematurely.

The canonical reader handles names, numeric expressions, text/action fields,
function arguments and file windows on the same ordered stream. Use the shared
numeric operator grammar and existing text function grammar. Constructor output
and selected token lists retain kinds; no per-line contains-.ident exemption,
spelling-only register bypass, second scope namespace or string-valued Expr.
Validate each name at its consuming grammar position, rather than passing an
export reference through constant-definition admission. Preserve the binding
rules for statement-head mnemonics, qualified and cheap names.

Migrate linked ca65 consumers first, then the ca65 flat-family adapters and
syntax-only mode. Keep capture/include order, approved physical lexical checks,
formatter idempotence and debug/source spans under their existing decisions.
Do not claim a matching refusal when an earlier unrelated error masks the
intended consumer. Add native accepted counterparts and first-error checks.
