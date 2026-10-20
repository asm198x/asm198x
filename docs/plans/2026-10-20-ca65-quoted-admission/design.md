# Proposal: native quoted-token lexical admission (#835, #899)

Status: approved on 2026-10-20: extend lexical admission to characters and
quoted strings (option 1). Implementation follows the steps below. The 139-case native audit has 70 acceptances and 69 refusals. The
clean preceding producer has 33 wrong acceptances: 27 character cases and six
quoted-string cases. Three character cases are selected-source UTF-8 characters;
30 errors are in inactive branches, unused macros and zero-repeat captures.

## Binding boundary

`decisions/parse-affecting-directives.md` authorizes only native numeric-token
checks in skipped/captured source. It explicitly states: “Character-literal
errors remain a separate lexical scope (#835); this exception does not authorize
them.” The approved extension below supersedes that numeric-only limit. Directives,
expressions, names and includes remain lazy.

## Options

1. Extend lexical admission to both characters and quoted strings (recommended).
   Check native physical-token shape at the current numeric admission/capture
   seams. Reject invalid one-byte character forms and unterminated double-quoted
   tokens. Keep source meaning selected and lazy. This covers all 33 measured
   wrong acceptances with one explicit lexical policy.
2. Extend to characters only. Fix the 27 character cases under #835 and keep
   the six string cases under #899. This is the smallest amendment, but leaves
   the two quoted token forms with different admission policies.
3. Retain numeric-only admission. Keep both issues open with the complete
   native evidence; no reader behavior changes.

## Proposed shape

Extend the private `ca65_flat::validate_numeric_tokens` admission helper (rename
it to describe its approved scope) and its existing selected-sweep and macro/
repeat capture callers. Check physical lexical tokens, not emitted data operands.
Do not add a second expansion pass, eager directives, expression evaluation,
name lookup, inactive text expansion or inactive include I/O. Do not add strings
to Expr, layout callbacks, dependencies or public schemas.

Native characters contain one source byte with the required native quote shape;
UTF-8 character counts are not byte counts. Preserve valid apostrophe/backslash
controls. Native strings have raw backslashes, so C-style escape decoding must
not be introduced. Adjacent complete string tokens in an untaken branch remain
lexically valid even when they would be an invalid selected `.byte` operand.
The strict data-literal rule from #887 must not run on skipped operands.

Admission follows existing read/capture order and native source positions.
Source after an active `.end` remains unread. An `.end` in an untaken branch
must not hide later errors. Computed byte-string values retain their identity
and are never reinterpreted as physical quoted syntax. The formatter stays a
canonical syntax-only reader and preserves the existing after-`.end` contract.

## Delivery and verification after approval

1. Amend the binding decision with the agreed lexical forms and explicit lazy
   exclusions. Preserve the old numeric-only paragraph as superseded reasoning
   rather than silently changing its meaning.
2. Before implementation, extend capture/`.end`/include-order controls and replay
   against the clean predecessor. Native can report more than one lexical error;
   preserve the first error's file/line and the established fail-fast contract.
3. Change `ca65_flat.rs` and the existing calls in `ca65/project.rs`; update
   native admission tests under `crates/asm198x/tests/`, without touching shared
   MOS dialect grammar. Retain all 139 cases and full coverage accounting.
4. Check native accept/refuse outcomes, valid bytes, untouched inactive names and
   include I/O, source/macro frames, output preservation and formatter round trips.
   Keep all numeric admission, byte-string, text-consumer and 4510 controls.
5. Run full Rust/Lua and Node suites, Clippy/fmt/Ruff/docs; freeze a clean producer,
   local package/corruption controls, actual five-host archives and exact-head CI.
   Rebase-merge and remove the owned workspace/caches after retaining evidence.
