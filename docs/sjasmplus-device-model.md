# sjasmplus's device model

What `DEVICE`, `PAGE` and `SLOT` do, established by probing SjASMPlus 1.21.0 on
2026-08-24 (`AMSTRADCPCPLUS` on 2026-09-02). Written down because none of it is derivable from our code, and the
manual is not the arbiter — the binary is.

## The devices

Fourteen names are accepted. Everything else — `SCORPION256`, `PENTAGON128`,
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
| `AMSTRADCPCPLUS` | 0..31 | 0..3 | 16K |
| `NOSLOT64K` | 0..31 | 0..0 | 64K |

Page and slot bounds were found by binary search over `DEVICE d` + `PAGE n` /
`SLOT n`, taking the highest `n` that assembles. The ZX Spectrum sizes are
`memory / 16K` throughout, so the pattern holds and the numbers are not
guesses from the names. `AMSTRADCPCPLUS` has 32 pages, four times the
`AMSTRADCPC6128`'s — 512 KiB, the largest cartridge `SAVECPR` accepts (sizes
1–32 in 16 KiB units).

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
are addressed and whether the write check fires. What they change is the
device's *memory*, which the `SAVE*` words that read pages see and `--raw`
does not.

`ORG` follows the same split. Forward or backward, it changes the logical
address but never pads, seeks, truncates, or overwrites the raw stream. A
negative `DS`/`DEFS`/`BLOCK` emits the `Negative BLOCK?` advisory and moves the
logical counter backwards, wrapping at 16 bits; it emits no fill bytes. Bytes
emitted after either kind of rewind append to `--raw` and overwrite the live
device memory at their new logical addresses.

## Where a write goes

Probed 2026-09-02 under `DEVICE AMSTRADCPCPLUS`, reading the pages back
through `SAVECPR` (bank `cbNN` is page N):

| source | where the bytes land |
|---|---|
| `ORG 0` / `$4000` / `$8000` / `$C000`, one byte each, no `PAGE` | `cb00[0]`, `cb01[0]`, `cb02[0]`, `cb03[0]` |
| `SLOT 1` `PAGE 5`, then `db` at `$4000` and at `$C000` | `cb05[0]` and `cb03[0]` |
| `PAGE 5` alone, same two writes | `cb01[0]` and `cb05[0]` |
| `ORG $4000` `db 1` `PAGE 5` `db 2` `PAGE 6` `db 3` | `cb01[0..3] = 1,2,3` |
| `ORG $4000` `db 1` `ORG $4000` `db 9` | `cb01[0] = 9`; `--raw` is `01 09` |
| nothing written, `SAVECPR "x",32` | 32 banks of zeros |

So: the slots start mapped to the page of the same number; the current slot
starts as the last one; `SLOT n` selects the current slot and `PAGE p` maps
page `p` into it; a byte emitted at address `A` is stored at
`pages[slots[A >> 14]][A & $3FFF]`, overwriting what was there. `--raw`
appends the same byte to its log regardless. The two are different views of
one emission. Both views are implemented: the raw image remains the emission
log, while #563 added the independently routed device-memory image used by
`SAVECPR` and future page-reading save directives.

The classic Spectrum devices use the ROM-derived 48K initial state recorded in
`syntheses/zx-spectrum/post-boot-ram.md`; all their additional pages are zero.
The Next, CPC, Plus, and NOSLOT devices start entirely empty (#318).

## What a careless implementation gets wrong

- **Accepting `DEVICE` and ignoring it.** The write check is real and
  observable; a program that overruns `$FFFF` is refused by sjasmplus and would
  be accepted by us.
- **Treating two pages at one address as an overlap.** They concatenate. The
  ordinary section-overlap refusal is wrong here.
- **Reading a page as the section its `PAGE` opened.** The section is the
  raw output's order; the page is memory, filled by address through the slot
  mapping. `ORG $4000` / `db 1` / `PAGE 5` / `db 2` puts both bytes in page 1.
- **Deriving page counts from the device name.** `ZXSPECTRUMNEXT` has 224
  pages of 8K, not 256 of 8K or 128 of 16K, and `NOSLOT64K` has 32 pages with
  a single slot. Both were probed.

## Symbol placement (#503 prerequisite)

`AssemblyResult.debug.symbol_pages` preserves the live placement of each
address symbol: expected slot, physical page, page size, and in-page offset.
It is keyed by the same symbol name as the existing captured symbol records;
constants have no entry. Absence means no known paged placement, not page zero.
The field is additive, defaults to empty when reading old JSON, and is omitted
from flat assembly results. No contract version changes.

The source definition fixes the placement. A later `PAGE`, `SLOT`, or `DEVICE`
cannot change an earlier label's page. `END label` preserves that label's
placement; a numeric entry address uses the mapping at `END`. A label on a
`PAGE` directive itself belongs to the mapping **before** the directive.

SjASMPlus 1.21.0 was probed on 2026-09-05, executable SHA-256
`454bfa33058d5a0f5323db1ee65bbdd2b5871f7d697c328e2ee1f1a950f0d78b`.
For two labels at `$C010` in Spectrum 128 pages 1 and 3, its CSPECTMAP gives
logical/physical pairs `$C010/$4010` and `$C010/$C010`; SLD independently
records pages 1 and 3. Its equal-valued EQU remains a constant. For the Next,
`before: PAGE 5` with page 4 previously mapped at `$E010` reports physical
`$8010` for `before` and `$A010` for the following label.

The repeatable native comparison is
`cargo test -p asm198x --test paged_symbols -- --include-ignored`.
Always-on tests cover all thirteen device geometries (8K, 16K, and 64K pages),
remapping one page into another slot,
device reset/disable, entry placement, byte identity, and old JSON payloads.

This is capture infrastructure, not an exporter release. #503 still needs
RGBASM bank placement, bank-aware Debug198x section projection, and the VICE,
NO$-style, and SjASMPlus writers with consuming-tool checks. Existing Debug198x
and listing rendering are unchanged by this slice; their flat section offsets
are not a substitute for `symbol_pages` when exporting banked addresses.
