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

## Capture rules measured after the preparation commits

The 330-case numeric capture audit and 225-case brace boundary audit are retained
at 8156fd54c08ec08300a61b6d75439b332f827fe7 on audit/ca65-captured-numbers-928.
They add required failure modes under #928/#930, without authorizing a different
architecture or changing the public contract.

- Numeric calls inside captured token lists remain tokens. Flat readers fold
  const and mnemonic queries early, changing token counts or looking up names
  that native merely captures. Numeric arguments of constructors still evaluate:
  sprintf's const argument is an accepted control, not an exception to ignore.
- In the measured brace captures, an opening brace is a token. The first physical
  closing brace ends the list; arbitrary balanced nesting is the wrong grammar.
  Native accepts `.tcount({{1})` and refuses `.tcount({{1}})`. It accepts two
  arguments in `.match({{VALUE},{{OTHER})`; the old balanced splitter hides the
  comma. Quoted and constructed string braces stay string tokens. Generated
  selector output containing brace tokens needs additional native controls before
  extending the physical-token boundary rule to that output.
- Keep this grammar in the ca65 reader. Changing the shared argument splitter's
  brace treatment globally would alter independent dialect grammars.

The empty-call interruption (#927/#929) also establishes that real no-argument
calls differ from line-zero recognition probes. Native blank/tcount return 1/0;
other captured empty functions are ordinary errors. The shared numeric syntax
parser must accept an empty vector for a dialect builder to validate, so valid
calls format without expansion. Preserve the flat text-pass macro-origin remap.
Do not reintroduce the old first-argument requirement when moving the borrowed
numeric parser preparation onto the checked successor.
