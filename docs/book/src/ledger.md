# Conformance ledger

What the recorded verdict corpus proves, per CPU. Every row is an
observation of a real reference assembler, not an expectation written
by hand. Regenerate with `cargo xtask ledger`.

- **Release:** `v0.0.72`
- **Corpus hash:** `3fd4853742ba01d4f71926d7081819bc7ae56ae2927a3827a4269d6152e4a4a6`
- **Pinned curriculum:** `f11698d0b51b3a6c1209cec3d649229aa35af97f` (2026-08-27)
- **CPUs:** 23, holding 9687 verdict(s)

## 1802

Form coverage: **255/255** (100.0%)

| arbiter | version | verdicts |
|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 311] | form 255 |

No tracked divergences.

## 2650

Form coverage: **245/245** (100.0%)

| arbiter | version | verdicts |
|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 311] | form 245 |

No tracked divergences.

## 6502

Form coverage: **151/151** (100.0%)

| arbiter | version | verdicts |
|---|---|---|
| `acme` | This is ACME, release 0.97 ("Zem"), 1 Aug 2025 | curriculum 138, form 151, fuzz 100, probe 103 |
| `ca65` | ca65 V2.19 - Git 8d86ca7 | curriculum 51, form 151, fuzz 100, probe 158 |

Tracked divergences — differences we know about and check:

- `issue-761` — 1 case(s)
- `issue-798` — 1 case(s)
- `issue-844` — 1 case(s)
- `issue-850` — 1 case(s)
- `issue-866` — 2 case(s)
- `issue-868` — 1 case(s)
- `issue-872` — 1 case(s)
- `issue-876` — 1 case(s)
- `issue-891` — 1 case(s)
- `issue-918` — 1 case(s)
- `issue-940` — 12 case(s)

## 65816

Form coverage: **270/270** (100.0%)

| arbiter | version | verdicts |
|---|---|---|
| `ca65` | ca65 V2.19 - Git 8d86ca7 | form 270, probe 70 |

Tracked divergences — differences we know about and check:

- `issue-820` — 1 case(s)
- `issue-895` — 1 case(s)
- `issue-928` — 1 case(s)

## 6800

Form coverage: **197/197** (100.0%)

| arbiter | version | verdicts |
|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 311] | form 197 |

No tracked divergences.

## 68000

Form coverage: **838/838** (100.0%)

| arbiter | version | verdicts |
|---|---|---|
| `vasmm68k_mot` | vasm 2.0f (c) in 2002-2026 Volker Barthelmann | curriculum 106, form 838, fuzz 91, probe 79, sweep-chunk 160 |

Tracked divergences — differences we know about and check:

- `issue-110` — 4 case(s)

## 6809

Form coverage: **277/280** (98.9%)

| arbiter | version | verdicts |
|---|---|---|
| `lwasm` | lwasm from lwtools 4.25 | form 277, fuzz 198, probe 50, sweep-chunk 98 |

No tracked divergences.

## 8039

Form coverage: **210/214** (98.1%)

| arbiter | version | verdicts |
|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 311] | form 210 |

No tracked divergences.

## 8048

Form coverage: **214/214** (100.0%)

| arbiter | version | verdicts |
|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 311] | form 214 |

No tracked divergences.

## 8080

Form coverage: **244/244** (100.0%)

| arbiter | version | verdicts |
|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 311] | form 244 |

No tracked divergences.

## CP1610

Form coverage: **30/30** (100.0%)

| arbiter | version | verdicts |
|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 311] | form 30, sweep-chunk 50 |

No tracked divergences.

## F8

Form coverage: **253/253** (100.0%)

| arbiter | version | verdicts |
|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 311] | form 253 |

No tracked divergences.

## PDP-11

Form coverage: **96/96** (100.0%)

| arbiter | version | verdicts |
|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 311] | form 96, sweep-chunk 78 |

No tracked divergences.

## SC/MP

Form coverage: **121/121** (100.0%)

| arbiter | version | verdicts |
|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 311] | form 121 |

No tracked divergences.

## SM83

| arbiter | version | verdicts |
|---|---|---|
| `rgbasm` | rgbasm v1.0.4 | probe 59 |

No tracked divergences.

## TMS7000

Form coverage: **203/203** (100.0%)

| arbiter | version | verdicts |
|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 311] | form 203 |

No tracked divergences.

## TMS9900

Form coverage: **69/69** (100.0%)

| arbiter | version | verdicts |
|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 311] | form 69, sweep-chunk 56 |

No tracked divergences.

## Z80

Form coverage: **796/796** (100.0%)

| arbiter | version | verdicts |
|---|---|---|
| `pasmo` | PasmoNext v0.1.3 (PC) (C) 2004-2005 Julian Albo | curriculum 161, form 796, fuzz 200, probe 73 |
| `sjasmplus` | SjASMPlus Z80 Cross-Assembler v1.24.0 (https://github.com/z00m128/sjasmplus) | curriculum 161, form 796, fuzz 100, probe 189 |

No tracked divergences.

## Z8000

Form coverage: **271/271** (100.0%)

| arbiter | version | verdicts |
|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 311] | form 271, sweep-chunk 96 |

No tracked divergences.

## Z8001

Form coverage: **271/271** (100.0%)

| arbiter | version | verdicts |
|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 311] | form 271, sweep-chunk 96 |

No tracked divergences.

## Z80N

| arbiter | version | verdicts |
|---|---|---|
| `pasmo` | PasmoNext v0.1.3 (PC) (C) 2004-2005 Julian Albo | form 30 |
| `sjasmplus` | SjASMPlus Z80 Cross-Assembler v1.24.0 (https://github.com/z00m128/sjasmplus) | form 30, fuzz 100, probe 10 |

Tracked divergences — differences we know about and check:

- `issue-984` — 1 case(s)

## huc6280

Form coverage: **234/234** (100.0%)

| arbiter | version | verdicts |
|---|---|---|
| `ca65` | ca65 V2.19 - Git 8d86ca7 | form 234 |

No tracked divergences.

## sm83

Form coverage: **504/504** (100.0%)

| arbiter | version | verdicts |
|---|---|---|
| `rgbasm` | rgbasm v1.0.4 | form 504 |

No tracked divergences.

