# Preserve formatter source containing .end (#833)

Native ca65 accepts an unused macro or zero-repeat body containing .end without
executing it. Our formatter currently executes the stop directive while parsing
source. It truncates a macro to an unclosed header, or changes a zero repeat
followed by byte 08 into an empty image. Paired native and clean-predecessor
streams are retained in ../2026-10-19-ca65-repeat-end/formatter-repeat-loss.json
and ../2026-10-19-ca65-numeric-admission/predecessor-full.json.

Merely preserving the .end line is insufficient. After a called macro stops
assembly, the rest of the file can contain oversized numbers or unclosed blocks
which native never reads. A formatter must retain those characters without
executing the macro or interpreting unread source. The current formatter shares
number, scope and block validation with the semantic reader.

## A. Preserve the complete source when it contains literal .end (recommended)

In ca65::parse_program only for Expand::No, recognize a literal .end instruction
head outside quoted/comment text, including an optional label. Return existing
Item::Verbatim nodes for every authored line. Keep source text, labels, comments,
indentation and line spans; retain the formatter's normal newline convention.
No expression, directive, name, macro or include evaluation. Assembly, the
selected sweep, AST/public schemas and dependencies are unchanged.

This prevents source loss immediately. Files containing literal .end are kept
as authored instead of canonically formatted; files without it keep their
existing formatter. Aliases which manufacture .end are separate source cases,
not an invitation to expand text macros in the formatter. Track canonical
formatting for literal .end separately if needed.

## B. Finish a canonical syntax-only reader first

Separate canonical formatting from semantic numeric/scope/block validation,
while preserving complete macro/repeat captures and unread tails after dynamic
.end execution. This retains canonical layout but is a larger reader change;
several current parser assumptions need independent native controls. It should
not be improvised as another exception in the semantic reader.

## Approval boundary

Option A changes the existing canonical-formatting convention for this narrow
set of files. The AGENTS.md ask-first rule for significant convention departures
therefore applies. This proposal is for design agreement, not implemented code.
The user already authorized investigating/fixing #833; no dependency or public
schema change is proposed.
