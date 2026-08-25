# Decision: an undocumented opcode is input, not output

**Status:** Active. Binding for Asm198x (accepted 2026-08-25). Applies the
house rule in [`syntax-stance.md`](syntax-stance.md) to a case where the
reference only speaks in one direction.

**Date:** 2026-08-25.

## The question

`lwasm` from lwtools 4.25 assembles three 6809 opcodes that appear in no
Motorola document:

| mnemonic | opcode |
|---|---|
| `reset` | `$3E` |
| `rhf` | `$14` |
| `hcf` | `$14` |

`rhf` and `hcf` are two spellings of the same byte. Do we take them, and if so
does the disassembler ever write them?

## What the reference does

Not what [#233](https://github.com/asm198x/asm198x/issues/233) recorded. The
issue said each assembles with no diagnostic; probed again, `lwasm` **refuses
all three by default**:

```
Illegal use of 6809 instruction in 6309 mode (hcf)
```

Its default target is the Hitachi 6309, and it takes them only under `--6809`.
The reason is the decisive fact here: **`$14` is `SEXW` on the 6309**, a
documented instruction. `lwasm` is not tolerating a stray byte. It models these
as 6809-specific and refuses them where the byte means something else.

## The decision

**In on input.** Our `--cpu 6809` is documented as lwasm syntax, `lwasm --6809`
accepts these, and the house rule is to match the reference rather than
out-converge it. Refusing source the reference accepts, on the exact target we
claim to match, would be us being stricter than the thing we are compatible
with. Our bytes agree: `3E 14 14`.

**Out on output.** The disassembler never writes them. `decode_6809` skips any
row marked `undocumented`.

**Cited to the reference, not the manufacturer.** These are the first rows in
`mos6809.rs` whose provenance is a third-party assembler, so they say so where
a reader meets them: an `undocumented: bool` on `Insn`, set through a separate
`inh_undocumented` constructor, and a comment naming lwtools 4.25 and the
probe. `crate::Row` already carried the flag through the enumeration seam.

## Why output is different from input

Accepting a byte costs nothing. Writing one makes a claim.

`fcb $14` says *this byte has no 6809 instruction meaning*. That is true
whichever part of the family produced it. `hcf` says two further things, and
neither is established by the byte: that this is 6809 code rather than 6309,
where the same byte is `SEXW`; and that the byte was meant as code at all.

Neither opcode has a defined result — `hcf` hangs the processor until reset —
so no working program contains one on purpose. A `$14` in a byte stream is
overwhelmingly data, or a misaligned read, and the disassembler should not
dress either as an instruction.

## Why the Z80 differs, and why that is not inconsistency

The Z80 declares eight undocumented forms (the `CB`-prefix `SLL` group) and
**does** disassemble them. The distinguishing property is not "documented"; it
is whether the opcode does something a programmer would choose. `SLL` is a
working shift that real software uses deliberately, so writing it tells the
reader something true. `hcf` is not, so writing it would not.

The spec marks both the same way. What a consumer does with the mark is the
consumer's call, and the two consumers here reach different answers from the
same flag for a stated reason.

## Consequences

- The 6809 form audit cannot arbitrate these rows: it works *through* the
  disassembler, which by this decision never emits them. It reports them by
  name as input-only rather than counting them as a gap.
- `cargo xtask surface` no longer lists them as outstanding lwasm words.

## Drift triggers

- **"The spec knows the mnemonic, so the disassembler should print it"** — the
  spec knows what a byte *can* mean. The disassembler asserts what it *does*
  mean, and for these bytes that is not established.
- **"Be symmetric, like the Z80"** — symmetry is not the rule; the rule is
  whether the mnemonic is a true statement about the byte. Re-read the section
  above before changing this.
- **"lwasm assembles it, so we must disassemble it"** — `lwasm` has no
  disassembler. There is no reference behaviour to match in that direction,
  which is why this is a decision rather than a transcription.
