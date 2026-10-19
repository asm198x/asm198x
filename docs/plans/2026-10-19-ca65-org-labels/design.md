# Keep the old label PC and mode until the label is defined

Native ca65 main.c calls SymDef(GenCurrentPC()) before HandlePseudo(). The
linked reader instead applies .org/.reloc before inserting the label, despite
capturing deferred definition positions under the old PC. Move that existing
mode/counter update block after label definition/debug capture and before the
remaining statement handling. This fixes the emitted label value without another counter or placement pass.

Keep all other expressions and offsets unchanged. Native controls must compare
labels on the directive against labels immediately before it, with the same
aliases and subsequent PC uses. Preserve idempotent formatting, debug shape,
output preservation and included locations on refusal. No public schema,
dependency or source grammar changes are needed.

The paired label controls also expose a second counter bug: data operands all
read the statement-start PC. Native advances the PC after each byte/word/dbyte/
dword operand. Correct those four existing emission loops with their own width
increments, leaving instruction-PC rules unchanged. This is a second concrete
counter observation within the same retained matrix; track it separately and
qualify both fixes together. Add positive width/range, strings, aliases and
instruction baseline controls before changing the loops.

The lazy-value demand walk must use the same per-operand PCs as emission.
Sixteen native cyclic-alias controls prove both accepted skipped branches and
refused demanded branches across all four widths; advancing only the emitter
would introduce new refusals of native valid input. Keep instruction demand
under its existing statement-PC rule.

Absolute `.org` labels still use physical section coordinates in debug output;
that existing value mismatch is tracked in #814. Preserve debug round-trip
shape here and assert agreement with the native emitted label for the initial
relative-to-absolute case. Do not change the public label/address model in this
byte-counter correction.
