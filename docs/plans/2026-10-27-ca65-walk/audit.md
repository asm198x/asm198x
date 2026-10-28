# Audit: what the ca65 walk still lacks

**Date:** 2026-10-27. **Scope:** what stands between the walk and step 7, the
switch; and what "fully supported" ca65 means beyond it.

"Fully supported" was set by the project owner on 2026-10-27: full coverage
for any machine the family might want to assemble for. That makes three
axes: the source language, ca65's CPUs, and ld65's linking.

## How it was measured

A throwaway branch, `audit/ca65-walk`, sends the old reader's entry points
(`assemble_ca65`, `assemble_ca65_files`, `assemble_ca65_files_with_config`
and their `_debug` forms) through the walk when `ASM198X_AUDIT_WALK` is set.
The `_debug` forms return the walk's bytes with an empty debug record. The
whole `asm198x` suite then ran twice, normally and through the walk.

| Run | Passed | Failed |
|---|---|---|
| The old reader | 1579 | 0 |
| The walk | 1454 | 125 |

Every disputed source was then run through ca65 V2.19 and through the walk
directly, since the old reader's fixtures were captured with ca65 V2.18.

## Finding 1: the old reader's fixtures pin ca65 V2.18

The native captures in `tests/fixtures/ca65-*.json` record `ca65 V2.18`. The
verdict corpus, and the walk, follow V2.19. The two versions disagree on at
least five of the disputed sources, and in each the walk already gives V2.19's
answer:

| Source | V2.18 | V2.19, and the walk |
|---|---|---|
| `@value=7` with no global label before it | refused | accepted |
| the same `.include` twice in an unnamed `.scope` inside `.repeat` | refused | accepted |
| `.defined(@value)` after a `.scope` closes | 1 | 0 |
| `outer::missing::VALUE=7` | refused | accepted |
| `.include` inside a macro body | the include's bytes come after the macro's | before |

So before step 7, every ca65 fixture should be captured again with V2.19, as
`decisions/verdicts-track-current-tools.md` asks of the verdict corpus. The
old reader is frozen (#940), so a V2.19 fact it misses becomes a gap against
that issue rather than a fix.

Done in 6f (2026-10-28): 301 cases are marked `old_reader_gap`. See the plan's
"6f" section.

## Finding 2: the walk fixes 40 recorded divergences

All 40 verdict-replay failures through the walk are tracked divergences of
the old reader that the walk now matches: `.sizeof`, `.match` and
`.xmatch`, `.const`, `.set` redefinition and others. At the switch their
markers come off the ledger.

## Finding 3: what the walk gets wrong

Each row was confirmed against ca65 V2.19 unless it says otherwise.

### The source language

| Gap | Example | ca65 V2.19 | The walk |
|---|---|---|---|
| An equate of a name defined later | `OTHER=VALUE`, `.word OTHER`, `VALUE=$34` | `34 00` | refused |
| The same, inside a scope | `OTHER=VALUE` in `.scope inner` before `VALUE=$34` | `a5 34` | refused |
| A name defined from itself in a scope | `RED=9`, then `RED=RED+1` in `.scope inner` | accepted | refused |
| An export of an equate of a later name | `RED=BLUE`, `BLUE=2`, `.export RED` | accepted | refused |
| `.import` in a scope hides the name outside | `RED=2`, then `.import RED` in a scope | `.defined(RED)` is 0 | 1 |
| A label on `.end`'s line | `finish:.end` | defined | undefined |
| Nothing after `.end` is read | `.end $100000000`; `.end`, then `.byte '` | ignored | refused |
| A label on `.macro`'s line | `entry:.macro emit` | defined | undefined |
| `.asciiz` takes only strings | `.asciiz "a",1` | refused | accepted |
| A define is expanded inside a repeat body | `.undefine VALUE` in `.repeat` | refused | accepted |
| A made string as a file name | `.include .concat("inc","é")` | reads `incé` | escapes the byte |
| A label difference in one segment is a constant | `.ifconst RIGHT-LEFT` | true | false |
| `.autoimport` | `.autoimport +` | accepted | not lowered |
| Records | `.struct`, `.union`, `.enum`, `.tag` | | not lowered |
| CPU commands | `.setcpu`, `.p02`, `.p4510`, `.smart`, `.pushcpu`, `.popcpu` | | not lowered |
| `.reloc` | | | not lowered |
| `.condes` | | refused, as the old reader says | not lowered |
| Listing and object words | `.list`, `.listbytes`, `.pagelen`, `.debuginfo`, `.dbg`, `.fileopt` | | not lowered |

Lang198x follows C, not ca65, for `.sprintf`'s widths (the preprocessing
plan's divergence list), so `.sprintf("%3s","é")` differing from the old
reader's fixture is expected. That test changes at the switch, not the walk.

### What the assembler gives besides bytes

These hold for the CLI, which always goes through the `_debug` entry points.

- **The Debug198x sidecar** (6d.5): sources, sections, symbols and lines.
- **The symbol table** (`--sym`), with each name's final address.
- **Listings** (`--lst`, `--lst-json`), including per-emission costs for a
  repeated include.
- **The memory map**: `AssemblyResult::areas`, and `free(AREA)` budgets.
- **Warnings**: the walk has no warnings channel. ca65 warns of a zero-page
  export of an absolute label, and the old reader counts the warnings for
  data dropped from `BSS` and `ZEROPAGE`.
- **The result's origin**: a linked image has no single flat origin, so it is
  `None`, but the walk gives `Some(0)`.

### Diagnostics

- **Columns and files**: an `.include` refusal points at the operand, and a
  cycle-budget refusal in an included file names that file. The walk points at
  the line's start, and at the root.
- **One error, or all of them**: ca65 reports every error on a line, and a few
  fixtures count them. The walk stops at the first. This is a design choice
  to make, not a gap to copy blindly.
- **Messages the old reader's own tests pin**, such as `.forceimport`'s and a
  missing segment's. These are rewritten at the switch where the walk's
  message is as good.

### Linker configurations

- An empty segment pinned in a custom config fills its area. The walk leaves
  it empty (`used_file_extents_match_captured_native_builds`).
- The default `VECTORS` slot and its error message, and an unknown segment's
  message naming the config, differ.

## Finding 4: ca65's CPUs

ca65 V2.19 has 13 CPU modes. cc65 ships an opcode test with its expected
bytes for 12 of them (`test/asm/opcodes/*.s` and `.ref`), which is a ready
measure; the 65816 has Asm198x's own verdicts.

| ca65 mode | Isa198x | Asm198x's ca65 today | The walk | Machines |
|---|---|---|---|---|
| `6502` | `mos6502` | main reader | yes | most of the family |
| `6502X` | `nmos6502_undocumented` | no | no | C64, NES and Atari code that uses illegal opcodes |
| `6502DTV` | `c64dtv2` | no | no | C64 DTV |
| `65SC02` | `mos65c02` (plain) | no | no | Atari Lynx, Watara Supervision |
| `65C02` | `mos65c02` (Rockwell) | no | no | Apple IIc and enhanced IIe, BBC Master, Commander X16 |
| `W65C02` | `mos65c02` (WDC) | no | no | WDC boards |
| `65CE02` | `csg65ce02` | no | no | |
| `4510` | `csg65ce02` (4502) | main reader, via `.setcpu` | no | Commodore 65 |
| `45GS02` | none | no | no | MEGA65 |
| `65816` | `mos65816` | separate reader | no | Apple IIgs, Super Nintendo |
| `HuC6280` | `huc6280` | separate reader | no | PC Engine |
| `M740` | none | no | no | Mitsubishi 740 microcontrollers |
| `sweet16` | none | no | no | Apple II's SWEET16 interpreter |

The machines in Code198x's system files that use this family are the
6502, 65SC02 (Lynx), 65C816 (Apple IIgs, Super Nintendo) and HuC6280 (PC
Engine). ca65's own linker configs add the Commodore 65, MEGA65, Commander
X16, Watara Supervision, Gamate, Creativision and others. "Any machine we
might want" therefore reaches every mode in the table.

`lang198x-ca65` has one profile, the 6502. Its README already names the CPU as
an axis of the profile, so each mode is a CPU axis value there and an ISA
extension in Asm198x, gated by cc65's opcode test for that CPU.

## Finding 5: linking

Asm198x assembles and links one program, and reads a bounded subset of
ld65's config language. Fuller coverage would need more of that language
(`FILES`, `FEATURES`, `SYMBOLS` and the output formats the cc65 targets use)
and, for multi-unit projects, assembling several units and linking them
together. Neither the old reader nor the walk does either today.

## What this suggests for the plan

1. **6d.5, outputs**: the Debug198x sidecar, symbol table, listings, memory
   map, result origin and a warnings channel.
2. **6e, the language gaps** in Finding 3, each measured on V2.19, with
   diagnostics' columns and files.
3. **6f, fixtures on V2.19**: capture the ca65 fixtures again, and record what
   the frozen old reader then misses as gaps against #940.
4. **Step 7, the switch**, gated by the whole suite passing through the walk,
   which this audit's branch can show at each step.
5. **Stage 4, CPUs**: every mode in Finding 4, one at a time, each gated by
   cc65's opcode test; 65816 and HuC6280 move onto the walk then.
6. **Stage 5, linking**: more of ld65's config language, and several units
   linked together.
