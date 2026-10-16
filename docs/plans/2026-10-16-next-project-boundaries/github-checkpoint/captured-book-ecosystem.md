# Complete-project compatibility

This table compares complete, pinned upstream build targets with native tools.
A target matches only when its native control succeeds and every declared final
artifact has identical bytes under Asm198x. An object file is never compared
with a final ROM or executable.

<!-- generated: xtask ecosystem --markdown -->

Captured at **2026-10-16 18:15:50 UTC**. Asm198x revision `c85d1ddf0608013a52b3a3d072e68892d658e255`; executable SHA-256 `0ecb53e2cee965c5d5b921100c929f4d147d86b9fb5d49034513779670b0edf6`.

Manifest SHA-256 `a853f20bf150f97a3238c3c92bbd12e68f9a63363ce189aa42abb52e3892a8ef`. These results measure assembly artifact equivalence; emulator execution and real hardware are separate checks. Unrun targets remain unknown.

| Project / target | Dialect | Result | First boundary / exact output |
|---|---|---|---|
| 6502Assembly / `hello-world` | acme | equivalent | `helloworld.prg` (2081 bytes, `16e5a63289e474e84706517c592c0ba7f2ae86b5278d7d1e6eeb9a0c5ef205fc`) |
| 6502Assembly / `interrupts` | acme | equivalent | `ex_interrupts.prg` (2156 bytes, `a7e26508f04fce76ad4a88ad49f3628b889a22b1754b9d7bbe30584ff42c1518`) |
| 6502Assembly / `charset` | acme | equivalent | `ex_charset.prg` (48266 bytes, `914399becf7e9c8c6716c32d87a39003494d7b128a124a7669af2d547a0a9938`) |
| NES-ca65-example / `example` | ca65 | equivalent | `example.nes` (40976 bytes, `3ea01a6d817c9be12bacf7459acf07bdd018eb43f7b5d4bc8da77c48bfac0cd1`) |
| cc65 6502DTV opcode test / `opcode-space` | ca65 | reference_failure | reference assemble step 1 failed: ca65: Invalid CPU: '6502dtv' [issue](https://github.com/asm198x/asm198x/issues/718) |
| cc65 65CE02 opcode test / `opcode-space` | ca65 | reference_failure | reference assemble step 1 failed: ca65: Invalid CPU: '65ce02' [issue](https://github.com/asm198x/asm198x/issues/718) |
| cc65 CSG 4510 opcode test / `opcode-space` | ca65 | assembler_rejected | assembler assemble step 1 failed: asm198x: 4510-opcodes.s:22: error: `ORA` has no (indirect),z form [issue](https://github.com/asm198x/asm198x/issues/716) |
| GB ASM Tutorial — Unbricked / `unbricked` | rgbasm | equivalent | `unbricked/bricks/unbricked.gb` (32768 bytes, `c235cb25c228259c80d2c2930cdefc2c22abd48a1e9affeedeabc5e3e570fb85`) |
| SpecNext Invaders / `game` | sjasmplus | assembler_rejected | assembler assemble step 1 failed: asm198x: src/audio.asm:11: error: malformed character literal [issue](https://github.com/asm198x/asm198x/issues/717) |
| HelloAmi / `hello-ami` | vasm | equivalent | `HelloAmi` (244 bytes, `826d98fbcae710038965fb94e5366acf8bbf4af1ab11a61c3e2aad5945c32dab`) |
<!-- /generated -->

A reference failure says the native control did not build the pinned source.
It does not establish an Asm198x failure. An infrastructure result names a
missing tool, unreviewed version, timeout or unavailable input. Unrun targets
remain unknown. Each retained record includes the exact source revision,
Asm198x producer revision, executable hashes, arguments, process streams and
artifact hashes. Native capture and reference replay have separate dates.

Acquisition remains inert:

```sh
cargo xtask ecosystem check
cargo xtask ecosystem fetch /tmp/ecosystem-sources
```

Run reviewed plans only through the explicit audit command, with a new result
directory and a target selection:

```sh
cargo xtask ecosystem audit /tmp/ecosystem-sources /tmp/ecosystem-results --all
```

The audit builds the clean checked-out Asm198x revision with `--locked` and Lua,
then uses separate complete source copies for the two toolchains. It does not
run an upstream Makefile or shell script. Execution requires Linux or macOS
process-group support. Every command has time and output limits; prior result
directories are never overwritten.

By default, any non-equivalent target returns a failure status. A scheduled
capture uses `--record-findings` to retain known compatibility findings while
still failing for unavailable infrastructure or native control failures. Its
job status is not a compatibility claim; the per-target outcomes are.

Ordinary CI re-evaluates every target with a successful native control, using
its retained final-artifact identities. Known successes are hard regression
gates; blocked targets get fresh first-boundary results after each change.
The report keeps the original native capture date, so replay cannot make old
native evidence appear fresh. Source commits and the reviewed manifest must
still match. Emulator execution and real hardware validation are separate work.

The [corpus and admission rules](https://github.com/asm198x/asm198x/blob/main/docs/ecosystem-corpus.md)
explain source provenance and build adapters.
