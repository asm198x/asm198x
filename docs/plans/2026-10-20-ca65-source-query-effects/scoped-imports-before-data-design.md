# Scoped imports before ordered data queries (#810 / #795)

Status: approved on 2026-10-21. The user said “Keep going” after the concrete
recommendation and scope question. Finish #810 first, then return to draft #917.

PR #917's 515 data controls match native, fixing 186 differences, and its
release replay retains every match across 2,466 cases. It cannot merge: the
full suite and native archives show import_queries_before_definition is now
accepted, while ca65 rejects RED's definition as already imported. Earlier
scoped .const refusal masked the existing import binding defect.

The attempted narrow guard fixes that symptom but regresses nine of the 274
retained import/export controls. The exact patch, before/after results and
failures are retained in data-context-trials. It has been rolled back.

The #803 design explicitly leaves import/global allocation, same-file external
resolution and address-size behavior together in #810. That boundary is real:
qualifying an import guard alone changes fallback and mixed visibility.

## Recommended: finish #810's existing contract first

Keep #917 as a draft. Work from the last merged main and the qualified #916
baseline; preserve #917's source and all audit evidence on their remote refs.
Use the existing selected reader, shared scope environment, private deferred
classifications and fused single-translation-unit linker. No dependencies,
public AST/schema changes, separate object-file linker or new namespace.

1. Expand docs/plans/2026-10-19-ca65-scoped-exports/probe.py and its retained
   274-case matrix with selected import/global declarations, immutable/label
   collisions, name queries, repeated declarations, explicit same-file exports,
   default/zp address-size, includes/macros and refusal ownership. Keep exact
   native tool identities and every accepted byte/refused output invariant.
2. In crates/asm198x/src/dialects/ca65/mod.rs, bind declarations to their actual
   lexical scope through the existing environment. A local external declaration
   must mask an outer value. Reject a definition of that same binding without
   prohibiting a distinct enclosing binding. Keep declaration and read effects
   positional. Review the existing scope_syms, constant_key_at, source query,
   final scope_targets and imported-definition guard seams before changes.
3. In ca65/project.rs, ca65/deferred.rs and ca65/link.rs, keep native source
   address-size/constancy distinct from final exported values. Resolve explicit
   same-file exports through the existing fused model. An absolute external
   with exported value 2 must still select AD0200; its zp declaration selects
   A502. Do not treat final value 2 as proof of source-time constancy or width.
4. Add the complete native contract to the existing tests and mandatory archive
   qualification. Require zero lost prior matches, exact byte/debug/formatter
   invariants and unchanged files on refusal. Run all workspace checks, fresh
   4510 verification and five actual native archive reports before merging.
5. Rebase #917 onto that checked change, rerun the 515 data cases, full prior
   matrices and complete qualification. Keep #795 open until its remaining
   repeat/allocation/assertion/file/string numeric consumers are integrated.

Write a bite-sized implementation plan with exact functions after agreement
and inspect any additional binding decisions before changing classifications.
If the existing private structures cannot express this without a significant
new architectural pattern, stop with the concrete additional design.

## Smaller alternative: narrow #917 first

Return #917 to a root-scope data slice and retain explicit refusals at the
unqualified import/scoped boundary until #810 is complete. Re-audit every native
case; do not silently remove supported cases or weaken an old fixture. This
ships less of #795 and may need additional measured boundary exclusions, but
avoids implementing the coupled external contract in this slice.

## Remaining measured consumers

A separate 50-case native audit has 30 active differences (direct/include/macro)
and 20 passing lazy controls. It covers repeat counts, reservations/fill,
alignment/fill, assertions, numeric output strings, incbin bounds and include
names. All native cases accept. Preserve these under #795; do not treat data
integration or the original 56/128-query matrices as full issue completion.
