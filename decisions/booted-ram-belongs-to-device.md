# Decision: booted RAM belongs to `DEVICE`, and the absence of `DEVICE` is meaningful

**Status:** Active. Binding for Asm198x (accepted 2026-09-02). Supplies the
model [#318](https://github.com/asm198x/asm198x/issues/318) needs and settles
the question [#568](https://github.com/asm198x/asm198x/issues/568) raised.

**Date:** 2026-09-02.

## The decision

1. **No `DEVICE` declared: RAM is zero-filled, with the attribute file at
   `$38`.** This is what `--sna` does today and it does not change.
2. **A `DEVICE` declared: RAM starts as that machine booted** — system
   variables and the UDG letterforms included.
3. **An output format never declares a device.** `--sna` does not imply
   `DEVICE ZXSPECTRUM48`, and no future format flag implies one either.
4. **Under a declared `DEVICE`, the UDG letterforms are emitted**, knowing what
   they are. See § The firmware question.

## Why there is no conflict to resolve

The two reference assemblers disagree, so "match the reference" appears to have
two answers. It does not, because the two behaviours are unreachable from one
another:

```
nodev.asm(5): error: SAVESNA only allowed in real device emulation mode (See DEVICE)
```

sjasmplus refuses to write a snapshot without a device, and refuses
`DEVICE NONE` as well — consistent with
[`../docs/sjasmplus-device-model.md`](../docs/sjasmplus-device-model.md), which
found `NONE` behaves as no `DEVICE` line at all. pasmo has no device model, so
its `--sna` is the only snapshot reachable in that state.

No source can observe both behaviours. The directive is not a mode we invent to
reconcile two references; it is the precondition one of them already enforces.
Declaring a device selects which assembler you are asking to be measured
against.

## Rule 3 is the one doing the work

An earlier draft of this record had `--sna` supply a default declaration of
`ZXSPECTRUM48`, to keep the Code198x lesson command working unchanged. That is
backwards. It would push the pasmo path into booted-RAM behaviour and lose
parity we currently hold for nothing. Absence of `DEVICE` is not an omission to
be filled in; it is the pasmo memory model, and it must stay reachable.

## Measured, 2026-09-02

One source through three assemblers, snapshots loaded into `emu198x-spectrum`:

| | sysvars `$5C00`–`$5D5B` | UDGs `$FF58`–`$FFFF` | `RST 16` reached |
|---|---|---|---|
| pasmo `--sna` | 0/348 nonzero | 0/168 | no — derails at `$D2D1` |
| asm198x `--sna` | 0/348 | 0/168 | no — `$D2D1` |
| sjasmplus `DEVICE` + `SAVESNA` | 93/348 | 126/168 | yes |

`$5800` is `$38` in all three. We match pasmo's bytes and
reproduce its failure at the same instruction, which is what parity means here.

A program calling a ROM routine derails inside `CHAN_OPEN`, which reads `CHANS`
at `$5C4F` to find the channel table and gets zero. That is a property of a
pasmo-style snapshot, not a defect in it.

## Where the values come from

The post-boot state is a hardware fact, so it is sourced from the prose layers
and cited upward, per
[`multi-artifact-output.md`](multi-artifact-output.md)'s rule against
transcribing another tool's output. `reference/` holds the Sinclair manual's
system-variables map and Logan & O'Hara's ROM disassembly. The values are
**modelled, not tabulated** — `RAMTOP` and `P-RAMT` depend on RAM size, so a
table of constants is wrong before it is copied, and modelling generalises to
the other twelve devices, whose Amstrad members will not resemble the Spectrum
ones.

## The firmware question

126 of the 168 UDG bytes are a verbatim copy of the ROM's own character set —
confirmed byte-identical to `$3E08` read off real 48K firmware. Emitting them
means asm198x carries those bytes, as sjasmplus does; it produced them here
with no ROM available to it.

This is accepted deliberately, and narrowly:

- It applies only under a declared `DEVICE`. Nothing on the pasmo path emits
  them, and `--sna` is the path Code198x uses.
- Parity is the project's standing rule, and diverging by 168 bytes is a
  conformance failure that would show on any `SAVESNA` comparison.
- The system variables raise no such question. They are values the ROM
  computes, not expression it contains.

It remains a known exposure rather than a settled licence position. Amstrad's
1999 permission covers distributing the ROM *images*; it does not speak to an
assembler reproducing fragments of one in its output. Revisit if Asm198x's
distribution model changes, or if the letterforms can be derived from a
user-supplied ROM instead of carried.

## Related

- [#318](https://github.com/asm198x/asm198x/issues/318) — the fact this needs
- [#563](https://github.com/asm198x/asm198x/issues/563) — device memory paging
- [#568](https://github.com/asm198x/asm198x/issues/568) — the measurement, and
  the constraint it leaves for Code198x
- [`../docs/sjasmplus-device-model.md`](../docs/sjasmplus-device-model.md)
