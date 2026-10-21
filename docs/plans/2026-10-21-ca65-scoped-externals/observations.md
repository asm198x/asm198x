# Scoped external bindings

The complete ca65/ld65 V2.18 audit has 1,390 cases: 1,126 accepts and 264
refusals. The frozen qualified #916 executable differs on 444. The development
implementation differs on 12; all twelve are explicit refusals of valid numeric
source-query expressions assigned to #795. The 274-case prior visibility audit
improves from 223 to 266 matches, retaining every previous match. Its eight
remaining scoped data-constancy cases also belong to #795.

Sources, configurations, native process streams and tool hashes are retained
in native.json. before.json and visibility-before.json identify the frozen
predecessor executable. after.json, visibility-after.json and
visibility-token-final-after.json identify the development executable. These
are native comparisons, not release qualification. Earlier development
comparisons are retained under their original first/second/third/consumer names.

An import/global allocates an unresolved binding in its current scope and masks
an outer value, while native .defined returns false. Name-only queries do not
create ordinary reads; .const does. Imported RED-RED and RED*0 classify constant;
pending global or ordinary unresolved names do not. Logical short-circuit
expressions can classify constant while retaining their ordinary source reads.

An explicit export can satisfy an external's final value without changing its
source constancy or width. With exported RED=2, an absolute import selects
AD0200; importzp selects A502. Byte data and immediates using the absolute
external are refused even though 2 fits a byte. Low-byte extraction and
algebraic cancellation have distinct native outcomes. A forward global that
later becomes a local constant may emit byte data at assembly completion.

A global before a local forward definition selects an absolute instruction,
then zero page once RED=2 is read. Globalzp pins the unresolved use to zero
page. Exportzp alone does not shorten a forward instruction. Mixed export and
global declarations preserve the measured declaration-order width behavior.
Size mismatches and import/export conflicts refuse at the declaration.

The root cause spans declaration allocation, source classification, address
size and fused linking. Previously imports stayed bare, globals could find a
parent, and no explicit external-to-export binding preserved the original
source type. The fix records declaration facts under existing lexical keys,
validates source-width constraints before substituting exported values, and
retains the separate source expression graph. It introduces no namespace tree,
public AST fields, dependency, object linker or layout-time string evaluation.

The fixture inventories all 1,670 audited names: 1,390 new cases, 274 old cases
and six lexical controls. Its 1,646 supported cases include 1,265 native accepts
and 381 refusals. Twenty exclusions remain #795; four lexical audit-only cases
remain #918. The latter separates native-invalid plain register spellings from
valid names produced by .ident. Existing generated-name matches are preserved.
verify_comparisons.py checks the complete disposition and rejects any lost
predecessor match. The Rust harness and mandatory ca65_scoped_externals package
cohort verify bytes, JSON, formatting, symbols, sections and refusal output
preservation. The fixture generator rejects missing or duplicate audit names.
