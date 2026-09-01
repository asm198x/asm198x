# Conformance ledger

What the recorded verdict corpus proves, per CPU. Every row is an
observation of a real reference assembler, not an expectation written
by hand. Regenerate with `cargo xtask ledger`.

- **Release:** `v0.0.55`
- **Corpus hash:** `9d2c6c5d95f913128dffc48f6a43cd330493def48aa0044f698ae675e84b1624`
- **Pinned curriculum:** `f11698d0b51b3a6c1209cec3d649229aa35af97f` (2026-08-27)
- **CPUs:** 23, holding 15948 verdict(s)

## 1802

Form coverage: **255/255** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 2 | form 510 |

No tracked divergences.

## 2650

Form coverage: **245/245** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 2 | form 490 |

No tracked divergences.

## 6502

Form coverage: **151/151** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `acme` | This is ACME, release 0.97 ("Zem"), 28 June 2020 | 2 | curriculum 276, form 302, fuzz 200, probe 208 |
| `ca65` | ca65 V2.18 - N/A | 2 | curriculum 102 |

Tracked divergences — differences we know about and check:

- `issue-93` — 2 case(s)

## 65816

Form coverage: **270/270** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `ca65` | ca65 V2.18 - N/A | 2 | form 540, probe 134 |

Tracked divergences — differences we know about and check:

- `issue-228` — 1 case(s)
- `issue-93` — 2 case(s)

## 6800

Form coverage: **197/197** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 2 | form 394 |

No tracked divergences.

## 68000

Form coverage: **838/838** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `vasmm68k_mot` | vasm 2.0b (c) in 2002-2025 Volker Barthelmann | 1 | curriculum 106, fuzz 100, probe 80, sweep-chunk 160 |
| `vasmm68k_mot` | vasm 2.0f (c) in 2002-2026 Volker Barthelmann | 2 | curriculum 106, form 1676, fuzz 91, probe 77, sweep-chunk 160 |

Tracked divergences — differences we know about and check:

- `canonicalisation-68000-0812FE97` — 1 case(s)
- `canonicalisation-68000-08AC909FCDFA` — 1 case(s)
- `canonicalisation-68000-08F7F12AF8BB` — 1 case(s)
- `issue-110` — 8 case(s)
- `issue-93` — 2 case(s)

## 6809

Form coverage: **277/280** (98.9%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `lwasm` | lwasm from lwtools 4.25 | 2 | form 554, fuzz 298, probe 100, sweep-chunk 196 |

Tracked divergences — differences we know about and check:

- `issue-93` — 2 case(s)

## 8039

Form coverage: **210/214** (98.1%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 2 | form 420 |

No tracked divergences.

## 8048

Form coverage: **214/214** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 2 | form 428 |

No tracked divergences.

## 8080

Form coverage: **244/244** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 2 | form 488 |

No tracked divergences.

## CP1610

Form coverage: **30/30** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 2 | form 60, sweep-chunk 150 |

No tracked divergences.

## F8

Form coverage: **253/253** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 2 | form 506 |

No tracked divergences.

## PDP-11

Form coverage: **96/96** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 2 | form 192, sweep-chunk 156 |

No tracked divergences.

## SC/MP

Form coverage: **121/121** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 2 | form 242 |

No tracked divergences.

## SM83

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `rgbasm` | rgbasm v1.0.3 | 2 | probe 130 |

Tracked divergences — differences we know about and check:

- `issue-199` — 13 case(s)

## TMS7000

Form coverage: **203/203** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 2 | form 406 |

No tracked divergences.

## TMS9900

Form coverage: **69/69** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 2 | form 138, sweep-chunk 112 |

No tracked divergences.

## Z80

Form coverage: **796/796** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `pasmo` | PasmoNext v0.1.3 (PC) (C) 2004-2005 Julian Albo | 2 | curriculum 322, form 1592, fuzz 200, probe 72 |
| `sjasmplus` | SjASMPlus Z80 Cross-Assembler v1.21.0 (https://github.com/z00m128/sjasmplus) | 2 | curriculum 322, probe 188 |

Tracked divergences — differences we know about and check:

- `issue-205` — 1 case(s)
- `issue-230` — 1 case(s)
- `issue-93` — 4 case(s)
- `issue-98` — 1 case(s)
- `issue-99` — 1 case(s)

## Z8000

Form coverage: **271/271** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 2 | form 542, sweep-chunk 193 |

No tracked divergences.

## Z8001

Form coverage: **271/271** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 2 | form 542, sweep-chunk 193 |

No tracked divergences.

## Z80N

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `sjasmplus` | SjASMPlus Z80 Cross-Assembler v1.21.0 (https://github.com/z00m128/sjasmplus) | 2 | probe 18 |

No tracked divergences.

## huc6280

Form coverage: **234/234** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `ca65` | ca65 V2.18 - N/A | 2 | form 468 |

No tracked divergences.

## sm83

Form coverage: **504/504** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `rgbasm` | rgbasm v1.0.3 | 2 | form 1008 |

No tracked divergences.

