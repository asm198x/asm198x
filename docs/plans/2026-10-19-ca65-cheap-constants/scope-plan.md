# Complete source-time cheap binding before merging #783 (#778)

The next native matrix found a valid procedure emits 09 natively but 07 here.
Hold #783 until this is fixed. Do not introduce accepted wrong-byte programs
with the new cheap-constant support. The current 44 native cases show 25
namespace/byte disagreements, including emitting scopes and record closures.

1. Extend scope-probe.py and a required ca65-scope-anchors.json fixture with
   empty/zero-byte/emitting scopes, procedures, nested frames, record closures,
   segment switches, alignments across offsets and textual include/macro/repeat
   input. Repair the width probe's ZEROPAGE link configuration before using its
   complete-program outcomes. Retain wrong-byte controls in bounded CLI tests.
2. Reuse the existing private bit-branch pattern: resolve each ordinary
   instruction's ISA form in the selected projection using only its source-time
   operand dependencies, then retain that form/operands in a private selected-statement
   metadata for later layout/emission. No AST/public shape changes. This fixes
   the independently measured forward-constant width bug and lets the source
   cursor share the existing link::resolve sizing logic instead of inventing
   a second encoder. Never copy the complete namespace on every instruction.
3. Keep per-segment physical offsets in the existing selected reader; ordinary
   org/reloc still change reported addresses rather than byte placement. Mark
   the innermost open scope when a statement emits positive length; propagate
   that fact when it closes. Native scope entry preserves the current global
   except for a named procedure's actual label. Closure preserves the current
   global unless a native internal size definition creates a fresh inaccessible
   anchor. Record/member definitions follow their measured global events.
   Preserve includes, token-source priority and macro provenance.
4. Preserve the authored name of cheap constants in debug symbols using private
   definition metadata, consistent with existing cheap-label debug names. Do
   not change the canonical symbol-export contract or add public fields.
5. Require both native cohorts in package qualification, red-check the previous
   archive, run isolated Rust/Lua/Clippy/fmt/Ruff/docs/Node/archive controls and
   repeat the clean unchanged 4510 producer replay. Amend #783 around the final
   scope/form behaviour, inspect all exact-head checks/five real reports, then
   merge and continue queued #775/#781/#782 work.

The private Kind already retains selected ISA forms for bit branches; private
projection metadata retains the same resolved product for ordinary instructions.
This is a cached encoder result, not new scope state on the shared Statement.
The shared AST, engine/dialect/ISA seam and external formats remain unchanged.
The user delegated routine design decisions. No dependency is required.
