# Native boundary before implementation

The 650-case ca65/ld65 V2.18 audit has 554 accepts and 96 refusals. The frozen
qualified #916 producer differs on 207. All sources/configs/process streams,
tool identities, bytes and the producer hash are retained in native.json and
before.json. This is a baseline, not a claim of support.

An import/global allocates a current-scope unresolved binding and masks an outer
value, but native .defined still returns false. A query-only .defined does not
create an ordinary read; .const does. An explicitly exported root value can
satisfy the external at final linking while source-time .const remains false.
External RED-RED and RED*0 classify constant; <RED and RED+0 do not.

An absolute import with final exported RED=2 selects AD0200; its zp spelling
selects A502. A bare absolute external used as byte data is refused even though
the final value fits a byte; a low-byte extractor and algebraic cancellation
are separate native boundaries. Final values cannot stand in for source width.

A global before a local forward definition selects absolute while unresolved,
then zero-page once RED=2 is read. A globalzp pins the unresolved use to zero
page. Exportzp alone does not shorten an unresolved forward instruction. All
three operations are measured before and after local definitions.

The coupled root cause is visible in the current seams: import operands stay
bare; global lookup can fall back to an outer definition; source hint/final
lookup has no external declaration key; and the fused final value model lacks
an explicit export-to-external binding step. Qualifying only the import guard
was already rejected with nine native regressions. Finish these responsibilities
together, using metadata under the existing lexical keys, not a second scope
model, numeric placeholders or copied final values in the source environment.
