# ca65 4510 bit branches

#745 reproduces before the fix: `red.log.gz` retains the failing library and
CLI tests. The parser treated every comma operand as an X/Y index. The new
private payload holds both expressions and the selected published ISA form;
scoping/repeat symbol mapping visits both expressions.

`probe.py` captures 148 ca65/ld65 V2.18 controls: 74 accept and 74 refuse.
The complete sources, commands, streams, tool versions and executable SHA-256
identities live in `crates/asm198x/tests/fixtures/ca65-4510-bit-branches.json`.
All sixteen BBR/BBS mnemonics have zero-page width controls; targets cover PC,
labels, forward/backward signed range edges, repeats, includes and selectors.

Native `bbr0 $02,*+$34` emits `0f0233`. Native `.org $20 / bbr0 *,*+3`
emits `0f2102`. `PutBitBranch` in pinned cc65 source
`e11fb5c39371046ebe25485f984f644c5a0d65d3/src/ca65/instr.c:2079`
emits the opcode, reads/emits the zero-page expression, then reads the target
through `GenBranchExpr(1)`. This explains opcode+1 and opcode+2 PC evaluation.
Labels retain the instruction-end displacement baseline. Constant-only targets
are refused in relocatable mode; `.org` permits absolute targets and `.reloc`
restores refusal, even when `.org` left the placement bias zero.

Accepted cases check exact native bytes; all except the pre-existing scoped
constant formatter boundary also check format idempotence and reassembled
bytes, addresses and debug symbols. CLI tests cover successful include origins,
human/JSON refusal locations, and preservation of binary and four sidecars.
The actual package gate requires all 148 captures on every host.

The independent scoped-constant formatter failure is tracked in #747.
`scoped-constant-boundary.json` retains the ordinary 6502 reproduction, native
and Asm byte equality, formatted source and second-format error. Its assembly
coverage remains in the bit-branch fixture; scoped labels have their own
successful formatter control. No formatter behavior changes in this fix.

Local verification: 1,512 Rust tests pass (38 ignored), all 56 Node tests
pass, Clippy with warnings denied, Rust/Python format checks, Ruff and generated
docs checks pass. Adjacent compressed logs retain complete check output.

The complete clean replay from producer
`968f1d1e407bc924efbc6eb32e1c7f037b856ee9` now reaches line 265:
`unknown instruction PHD` (#748). Native output remains exactly the pinned
564-byte upstream reference (SHA-256
`8a5fcde630e55dd49403f7c6ffed25d8786e65e3d0a2723531b7ca3e2c0509ec`).
`4510-after-bit-branches/` retains the report and all fourteen raw streams;
`verify_replay.py --producer <commit> <result-directory>` verifies their
identity and rejects a changed native byte. The focused replay does not replace
the full ecosystem cohort's current report or claim whole-target equivalence.

`word-push-boundary.json` retains twelve paired PHD/PHW processes from the
same producer. Four valid PHD cases fail while the paired PHW bytes match;
native ca65 explicitly declares the alias. It also exposes native PHW overflow
refusals that Asm currently truncates (#749). These independent word-push
boundaries are retained for the next slice, without expanding this fix.
