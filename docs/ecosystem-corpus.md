# The ecosystem source corpus

The verdict corpus asks whether generated source produces the bytes a reference
assembler produced. The ecosystem corpus asks the broader question: **does an
unrelated, real project assemble unchanged?**

The index is [`ecosystem/corpus.json`](../ecosystem/corpus.json). Validate and
inspect it without network access:

```sh
cargo xtask ecosystem check
cargo xtask ecosystem list
```

Fetch every pinned tree, or one named project, into a directory outside this
repository:

```sh
cargo xtask ecosystem fetch /tmp/asm198x-ecosystem
cargo xtask ecosystem fetch /tmp/asm198x-ecosystem kindjie-6502assembly
cargo xtask ecosystem verify /tmp/asm198x-ecosystem
```

Fetch is intentionally inert: it clones and checks out the exact commit, then
checks that the recorded licence exists. It never runs a Makefile, script, or
assembler from the downloaded project.

## Admission rules

A project enters only when all of these are known:

1. It is independently authored source intended for a dialect Asm198x claims.
2. A full commit pins the observation.
3. Its licence explicitly permits the intended use, and the manifest points to
   the grant.
4. The native assembler invocation and build boundary are recoverable.
5. The source is kept unchanged and complete for that target.
6. The native control builds with the declared reference version. If a modern
   tool rejects old source, establish the historical tool first; that is a
   tool-generation finding, not an Asm198x result.

Build helpers and post-processors are recorded only where they affect the
artifact boundary. A failure in `rgbfix`, an image converter, an emulator, or a
disk packer is not an assembler rejection.

## Captured complete-target audit

The generated results below come from retained target records in
[`ecosystem/results/current.json`](../ecosystem/results/current.json).
A successful native control is required before interpreting an Asm198x result.
Missing tools, failed controls and unrun targets never count as compatibility
successes. Object files and final ROMs are distinct build boundaries.

<!-- generated: xtask ecosystem --markdown -->

Captured at **2026-10-16 15:29:40 UTC**. Asm198x revision `652114a082cad7b094201e3ad7966544bb676a92`; executable SHA-256 `cc168f4f4d9632836793a3f743d6ef6b956c9b02156bc6373e3b2a2d7863cf6d`.

Manifest SHA-256 `a853f20bf150f97a3238c3c92bbd12e68f9a63363ce189aa42abb52e3892a8ef`. These results measure assembly artifact equivalence; emulator execution and real hardware are separate checks. Unrun targets remain unknown.

| Project / target | Dialect | Result | First boundary / exact output |
|---|---|---|---|
| 6502Assembly / `hello-world` | acme | equivalent | `helloworld.prg` (2081 bytes, `16e5a63289e474e84706517c592c0ba7f2ae86b5278d7d1e6eeb9a0c5ef205fc`) |
| 6502Assembly / `interrupts` | acme | equivalent | `ex_interrupts.prg` (2156 bytes, `a7e26508f04fce76ad4a88ad49f3628b889a22b1754b9d7bbe30584ff42c1518`) |
| 6502Assembly / `charset` | acme | equivalent | `ex_charset.prg` (48266 bytes, `914399becf7e9c8c6716c32d87a39003494d7b128a124a7669af2d547a0a9938`) |
| NES-ca65-example / `example` | ca65 | equivalent | `example.nes` (40976 bytes, `3ea01a6d817c9be12bacf7459acf07bdd018eb43f7b5d4bc8da77c48bfac0cd1`) |
| cc65 6502DTV opcode test / `opcode-space` | ca65 | reference_failure | reference assemble step 1 failed: ca65: Invalid CPU: '6502dtv' [issue](https://github.com/asm198x/asm198x/issues/718) |
| cc65 65CE02 opcode test / `opcode-space` | ca65 | reference_failure | reference assemble step 1 failed: ca65: Invalid CPU: '65ce02' [issue](https://github.com/asm198x/asm198x/issues/718) |
| cc65 CSG 4510 opcode test / `opcode-space` | ca65 | assembler_rejected | assembler assemble step 1 failed: asm198x: 4510-opcodes.s: config: file area `MAIN` without `fill = yes` writes a partial extent, which is outside this bounded reader [issue](https://github.com/asm198x/asm198x/issues/716) |
| GB ASM Tutorial — Unbricked / `unbricked` | rgbasm | equivalent | `unbricked/bricks/unbricked.gb` (32768 bytes, `c235cb25c228259c80d2c2930cdefc2c22abd48a1e9affeedeabc5e3e570fb85`) |
| SpecNext Invaders / `game` | sjasmplus | assembler_rejected | assembler assemble step 1 failed: asm198x: /private/tmp/asm198x-ecosystem-qualified-audit/work/howprice-specnext-invaders--game/assembler/src/ayfxplay.asm:345: error: expected a value [issue](https://github.com/asm198x/asm198x/issues/717) |
| HelloAmi / `hello-ami` | vasm | equivalent | `HelloAmi` (244 bytes, `826d98fbcae710038965fb94e5366acf8bbf4af1ab11a61c3e2aad5945c32dab`) |
<!-- /generated -->

Use `cargo xtask ecosystem audit <sources> <new-results> --all` for an explicit
capture, or `--target=project/target` for a reviewed subset. Fetch stays inert.
The runner builds the clean checked-out Asm198x revision with `--locked`, then
uses separate complete source copies and bounded argv steps. Linux and macOS
process groups provide bounded helper-tree termination.

The default command fails for any non-equivalent target. `--record-findings`
retains compatibility findings for scheduled audits while still failing for
unavailable infrastructure or native control failures. A green capture job is
not a claim that every target is compatible; inspect each result.

Before publishing a reviewed capture, retain it at
`ecosystem/results/current.json`, run `cargo xtask ecosystem ledger
ecosystem/results/current.json --write`, then regenerate the book with
`cargo xtask docs`. `--check` validates evidence and table freshness. Ordinary
CI re-evaluates all native-qualified targets against captured artifact
identities. A changed byte, failed build or missing output in a known success
fails that replay; blocked targets retain their fresh next boundary.
Native capture dates remain separate from replay dates.

## Adding a project

Add a manifest row, using paths relative to the project checkout. Commands are
JSON arrays: one executable followed by its exact arguments, never a shell
string. Then run:

```sh
cargo xtask ecosystem check
cargo xtask ecosystem fetch /tmp/asm198x-ecosystem <project-id>
```

Review the upstream licence and build instructions at the pinned commit, run
the native control deliberately, and retain the first failing stage or exact final artifact comparisons in the
audit report. The published table is generated from those records.
