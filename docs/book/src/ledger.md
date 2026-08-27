# Conformance ledger

What the recorded verdict corpus proves, per CPU. Every row is an
observation of a real reference assembler, not an expectation written
by hand. Regenerate with `cargo xtask ledger`.

- **Release:** `v0.0.34`
- **Corpus hash:** `dc73233f3b794a3bc30765c2aabd5e8851ff985a4d66969d8c52bcb95620b4b4`
- **Pinned curriculum:** `5435e540cf393c1956362458d5ff0fca3ff705f2` (2026-08-14)
- **CPUs:** 23, holding 7928 verdict(s)

## 1802

Form coverage: **255/255** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 1 | form 255 |

No tracked divergences.

## 2650

Form coverage: **245/245** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 1 | form 245 |

No tracked divergences.

## 6502

Form coverage: **151/151** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `acme` | This is ACME, release 0.97 ("Zem"), 28 June 2020 | 1 | curriculum 138, form 151, fuzz 100, probe 99 |
| `ca65` | ca65 V2.18 - N/A | 1 | curriculum 51 |

Tracked divergences — differences we know about and check:

- `issue-93` — 2 case(s)

## 65816

Form coverage: **270/270** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `ca65` | ca65 V2.18 - N/A | 1 | form 270, probe 50 |

Tracked divergences — differences we know about and check:

- `issue-228` — 1 case(s)
- `issue-93` — 2 case(s)

## 6800

Form coverage: **197/197** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 1 | form 197 |

No tracked divergences.

## 68000

Form coverage: **838/838** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `vasmm68k_mot` | vasm 2.0b (c) in 2002-2025 Volker Barthelmann | 1 | curriculum 106, fuzz 100, probe 76, sweep-chunk 160 |
| `vasmm68k_mot` | vasm 2.0f (c) in 2002-2026 Volker Barthelmann | 1 | form 838 |

Tracked divergences — differences we know about and check:

- `canonicalisation-68000-0812FE97` — 1 case(s)
- `canonicalisation-68000-08AC909FCDFA` — 1 case(s)
- `canonicalisation-68000-08F7F12AF8BB` — 1 case(s)
- `issue-110` — 4 case(s)
- `issue-93` — 2 case(s)

## 6809

Form coverage: **277/280** (98.9%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `lwasm` | lwasm from lwtools 4.25 | 1 | form 277, fuzz 198, probe 50, sweep-chunk 98 |

Tracked divergences — differences we know about and check:

- `issue-93` — 2 case(s)

## 8039

Form coverage: **210/214** (98.1%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 1 | form 210 |

No tracked divergences.

## 8048

Form coverage: **214/214** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 1 | form 214 |

No tracked divergences.

## 8080

Form coverage: **244/244** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 1 | form 244 |

No tracked divergences.

## CP1610

Form coverage: **30/30** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 1 | form 30, sweep-chunk 100 |

No tracked divergences.

## F8

Form coverage: **253/253** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 1 | form 253 |

No tracked divergences.

## PDP-11

Form coverage: **96/96** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 1 | form 96, sweep-chunk 78 |

No tracked divergences.

## SC/MP

Form coverage: **121/121** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 1 | form 121 |

No tracked divergences.

## SM83

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `rgbasm` | rgbasm v1.0.3 | 1 | probe 54 |

Tracked divergences — differences we know about and check:

- `issue-199` — 13 case(s)

## TMS7000

Form coverage: **203/203** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 1 | form 203 |

No tracked divergences.

## TMS9900

Form coverage: **69/69** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 1 | form 69, sweep-chunk 56 |

No tracked divergences.

## Z80

Form coverage: **704/704** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `pasmo` | PasmoNext v0.1.3 (PC) (C) 2004-2005 Julian Albo | 1 | curriculum 161, form 704, fuzz 100, probe 36 |
| `sjasmplus` | SjASMPlus Z80 Cross-Assembler v1.21.0 (https://github.com/z00m128/sjasmplus) | 1 | curriculum 161, probe 96 |

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
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 1 | form 271, sweep-chunk 97 |

No tracked divergences.

## Z8001

Form coverage: **271/271** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `asl` | Macro Assembler 1.42 Beta [Bld 309] | 1 | form 271, sweep-chunk 97 |

No tracked divergences.

## Z80N

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `sjasmplus` | SjASMPlus Z80 Cross-Assembler v1.21.0 (https://github.com/z00m128/sjasmplus) | 1 | probe 9 |

No tracked divergences.

## huc6280

Form coverage: **234/234** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `ca65` | ca65 V2.18 - N/A | 1 | form 234 |

No tracked divergences.

## sm83

Form coverage: **504/504** (100.0%)

| arbiter | version | binaries | verdicts |
|---|---|---|---|
| `rgbasm` | rgbasm v1.0.3 | 1 | form 504 |

No tracked divergences.

