# sjasmplus's device model

What `DEVICE`, `PAGE` and `SLOT` do, established by probing SjASMPlus 1.21.0 on
2026-08-24. Written down because none of it is derivable from our code, and the
manual is not the arbiter — the binary is.

## The devices

Thirteen names are accepted. Everything else — `SCORPION256`, `PENTAGON128`,
`ATM512`, and any nonsense word — is refused.

| device | pages | slots | slot size |
|---|---|---|---|
| `NONE` | unbounded | unbounded | — |
| `ZXSPECTRUM48` | 0..3 | 0..3 | 16K |
| `ZXSPECTRUM128` | 0..7 | 0..3 | 16K |
| `ZXSPECTRUM256` | 0..15 | 0..3 | 16K |
| `ZXSPECTRUM512` | 0..31 | 0..3 | 16K |
| `ZXSPECTRUM1024` | 0..63 | 0..3 | 16K |
| `ZXSPECTRUM2048` | 0..127 | 0..3 | 16K |
| `ZXSPECTRUM4096` | 0..255 | 0..3 | 16K |
| `ZXSPECTRUM8192` | 0..511 | 0..3 | 16K |
| `ZXSPECTRUMNEXT` | 0..223 | 0..7 | 8K |
| `AMSTRADCPC464` | 0..3 | 0..3 | 16K |
| `AMSTRADCPC6128` | 0..7 | 0..3 | 16K |
| `NOSLOT64K` | 0..31 | 0..0 | 64K |

Page and slot bounds were found by binary search over `DEVICE d` + `PAGE n` /
`SLOT n`, taking the highest `n` that assembles. The ZX Spectrum sizes are
`memory / 16K` throughout, so the pattern holds and the numbers are not
guesses from the names.

`DEVICE NONE` is not a device with no memory: it behaves exactly as no `DEVICE`
line at all — no bounds and no write check.

## The write check

**Any** device — even `NOSLOT64K`, even `ZXSPECTRUM8192` with 8MB of pages —
enables one check, and it is on the 64K address space rather than on total
memory:

```
 DEVICE ZXSPECTRUM48
 ORG $FFFE
 db 1,2,3,4
```
```
error: Write outside of device memory at: 65536
```

Every device gives the same number, 65536. Without a device (or with `NONE`)
the same source assembles silently. So the device's memory size bounds `PAGE`,
and nothing else; what it does *not* do is let a program write past `$FFFF`.

## `PAGE` and `SLOT` in the raw output

`--raw` is an emission log, not a memory image. Two pages written at the same
address **concatenate** rather than colliding:

```
 DEVICE ZXSPECTRUM128
 SLOT 3
 PAGE 1
 ORG $C000
 db $11
 PAGE 2
 db $22
```
```
raw: 11 22
```

So a page is a section in the sense
`decisions/sections-in-the-shared-engine.md` means: addressed where the `ORG`
puts it, placed consecutively in the image. `SLOT` selects which slot a
subsequent `PAGE` applies to and does not itself move the output.

Neither `DEVICE` nor `PAGE` changes the bytes `--raw` writes — only where they
are addressed and whether the write check fires. Their effect on the `SAVE*`
family is a separate question, and that family is not implemented.

## What a careless implementation gets wrong

- **Accepting `DEVICE` and ignoring it.** The write check is real and
  observable; a program that overruns `$FFFF` is refused by sjasmplus and would
  be accepted by us.
- **Treating two pages at one address as an overlap.** They concatenate. The
  ordinary section-overlap refusal is wrong here.
- **Deriving page counts from the device name.** `ZXSPECTRUMNEXT` has 224
  pages of 8K, not 256 of 8K or 128 of 16K, and `NOSLOT64K` has 32 pages with
  a single slot. Both were probed.
