# Root cause

The configured-link minimal test reproduces #722: `.setcpu "4510"` followed by
`ora ($07),z` fails at line 2 with ``ORA has no (indirect),z form``. The parser
already classifies this operand as `IndirectIndexedZ`. Published isa198x 0.1.7
already defines all eight 4510 accumulator forms in `csg65ce02::CSG4502_SET`.

The ca65 reader carries only the base 6502 set. Its processor directive handler
turns every known non-6502 selection into an unsupported-processor error. The
lazy reader retains that error as a fatal `Kind::Message`; the linker resolves
instruction sizes before emitting messages, so the later missing ORA form masks
the earlier selection error. The observed diagnostic therefore does not prove
that `.setcpu` changed the opcode set.

Fixing the ISA or relabelling the operand would not address that missing
selection. The proposed scope carries native 6502/4510 state through the
existing lazy reader and records the active extension on each instruction.
The shared engine and public configured-link API need no change. Native probe
facts currently retain 116 executions, including CPU/refusal controls and
save/restore, conditional and reflection behavior.

Implementation retains the active extension on every instruction and uses the
existing union-form resolver. CPU-dependent text calls move into the existing
lazy read, so `.ismnem` and its nested expressions see the live selection.
The previous explicit scoped `.const` refusal remains in place. Live constants
and known names are borrowed for each line. Native controls pin CPU register
tokens and the repeat-entry tokenisation rule. The formatter preserves CPU
selectors and stack directives. Long relative forms use the existing ACME
65CE02 one-byte target bias and a two-byte displacement in the ca65 emitter.

The final fixture currently contains 139 captures: 49 accepted, 90 refused.
Both the focused fixture/CLI suite and the 75 pre-existing ca65 tests pass.
