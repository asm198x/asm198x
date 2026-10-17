# Scoped 4510 processor selection

Approved by the user in this session before implementation.

The red configured-link test reproduces #722 exactly. The existing 4510 ISA
extension already defines ORA `(indirect),z` as `12` followed by one address
byte. The ca65 reader/linker always uses the base 6502 instruction set. A
`.setcpu "4510"` becomes a deferred error; address/form resolution fails on
ORA before that error is emitted. Changing only the operand spelling or ISA
would leave the actual selection missing.

Recommended scope: extend the existing lazy ca65 reader with a 6502/4510
selection and its native save/restore stack. Apply live selectors across
includes, selected branches and repeats. Record the active existing ISA
extension on each private instruction payload, then resolve forms across the
base and extension using `mos6502::resolve_mode_in_sets`. Keep the configured
link/debug APIs and shared engine unchanged. No new dependencies or ISA facts.
Other currently unsupported ca65 processors retain their refusal.

Files: `crates/asm198x/src/dialects/ca65/{mod,project,link}.rs`; only extend the
existing `ca65_flat::Target` token/reflection choice if native probes require
it. Tests and package replay cover all eight indirect-Z accumulator forms,
operand widths/refusals, base instructions, selection/restoration, includes,
dead/live branches and source/debug identity. Full unchanged 4510 target replay
then identifies any independent instruction families still missing.

Alternative: implement all ca65 processor selections in one change. This would
also involve the separate 65816/HuC6280 assembly paths and their state models;
it is substantially wider than #722 and would delay its native byte proof.
