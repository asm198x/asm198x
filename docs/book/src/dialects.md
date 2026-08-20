# Dialects

`--dialect` selects the **source syntax**, not the CPU. Each front-end matches
an existing assembler, so pick the assembler the source was written for, not the
machine it runs on.

Where a dialect serves more than one CPU, `--cpu` picks the target — see
[Targets](cli.md#targets).

<!-- generated: asm198x dialects --markdown -->
| Dialect | Syntax of | Also accepted |
|---|---|---|
| `acme` | C64 6502, ACME syntax | `6502`, `mos6502` |
| `ca65` | NES 6502, ca65 syntax (assemble + link) | `nes` |
| `65816` | 65816, ca65 syntax | `816`, `ca65-816` |
| `huc6280` | PC Engine HuC6280, ca65 syntax | `pce`, `pc-engine` |
| `vasm` | Amiga 68000, vasm Motorola syntax | `68000`, `m68k`, `mot` |
| `lwasm` | 6809, lwasm syntax | `6809` |
| `rgbasm` | Game Boy SM83, RGBDS syntax | `sm83`, `gb`, `gameboy`, `game-boy` |
| `pasmo` | Z80, pasmo syntax |  |
| `pasmonext` | Z80, pasmo syntax, Spectrum Next target by default |  |
| `sjasmplus` | Z80, sjasmplus syntax | `sjasm` |
| `8080` | Intel 8080, Intel syntax | `i8080`, `intel8080` |
| `6800` | Motorola 6800, Motorola syntax | `m6800` |
| `1802` | RCA COSMAC CDP1802 | `cdp1802`, `cosmac` |
| `8048` | MCS-48 with on-chip ROM | `i8048`, `mcs48`, `mcs-48`, `8049`, `8050`, `80c48`, `80c49` |
| `8035` | MCS-48, ROM-less parts — the four BUS instructions are refused | `8039`, `8040`, `80c35`, `80c39`, `80c40` |
| `scmp` | National SC/MP (INS8060) | `sc/mp`, `ins8060` |
| `f8` | Fairchild F8 (3850), Channel F | `3850`, `f3850`, `channelf`, `channel-f` |
| `2650` | Signetics 2650 | `s2650`, `signetics2650` |
| `tms7000` | TI TMS7000 | `7000`, `tms70c00` |
| `pdp11` | DEC PDP-11 | `pdp-11`, `lsi11`, `lsi-11` |
| `tms9900` | TI TMS9900 (TI-99/4A) | `9900`, `ti99` |
| `cp1610` | GI CP1610 (Intellivision) | `cp1600`, `cp-1600`, `intellivision`, `intv` |
| `z8000` | Zilog Z8000, non-segmented | `z8002` |
| `z8001` | Zilog Z8001, segmented |  |
<!-- /generated -->

This table comes from the same list `--dialect` resolves against, so it always
matches what the assembler accepts.

## Picking one

The dialect names the **assembler**, which the source itself usually shows:
`!macro` and `!byte` are ACME, `.macro` and `.segment` are ca65, a `\1` inside a
macro body is lwasm or vasm.

Where a machine has a conventional choice:

| Machine | Usual dialect |
|---|---|
| C64 | `acme` |
| NES | `ca65` |
| ZX Spectrum | `pasmo`, or `sjasmplus` for the wider directive surface |
| ZX Spectrum Next | `pasmonext` |
| Amiga | `vasm` |
| Game Boy | `rgbasm` |
| PC Engine | `huc6280` |
| Dragon / CoCo | `lwasm` |

That is convention, not a constraint: any dialect assembles any source it can
parse, and the CPU follows the dialect unless `--cpu` says otherwise.
