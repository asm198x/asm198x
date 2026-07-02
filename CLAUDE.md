# Asm198x

A family of modern, single-binary assemblers and disassemblers for the 198x
family's target CPUs. One of three sibling projects under the `198x/` umbrella;
see [`../../CLAUDE.md`](../../CLAUDE.md) for umbrella context and cross-project rules,
and [`../../decisions/sibling-project-coordination.md`](../../decisions/sibling-project-coordination.md)
for the sibling relationship (Asm198x is the third sibling, peer to Code198x
and Emu198x — not a child of either).

## What this is

Modern, statically-linked, cross-platform assemblers to replace toolchains that
are getting hard to run: period assemblers need dead-OS emulation; modern
community ones are fragmented (a different tool and dialect per machine). One
Rust workspace, many crates — *not* one repo per CPU. Boring technology;
rescue over replace.

## Binding architecture

Read [`../../decisions/asm198x-and-shared-isa-spec.md`](../../decisions/asm198x-and-shared-isa-spec.md)
before changing the crate structure or the ISA layer. The load-bearing points:

- **Shared declarative ISA spec.** The [`isa`](crates/isa) crate is the single
  source of truth for instruction *encoding*. Asm198x consumes it to assemble
  and disassemble; Emu198x validates its hand-written decoders against it. The
  spec is **authored** from the primary reference library (datasheets), **not
  extracted** from any emulator's decode loop.
- **`isa` stays dependency-free and standalone**, so Emu198x can depend on it
  without pulling in the assembler. It lives here for now; promotion to a
  neutral location is deferred until Emu198x actually consumes it.
- **Source-compatible per machine.** The dialect lives in each CPU's parser
  front-end; the encoding underneath is the shared spec. Detail and per-CPU
  dialect targets are in [`decisions/`](decisions/).

## Crate layout

Two crates today; split further only when the per-CPU `isa` boundary or
Emu198x's consumption makes it real.

- [`crates/isa`](crates/isa) — instruction-set specs (types + `mos6502` + `z80`
  + `m68k` + `mos6809` + `mos65816` + `huc6280` + `sm83` + `i8080` + `m6800` +
  `cdp1802` + `i8048` + `scmp` + `f8`; the Z80 set
  includes the Z80N extensions, `huc6280` is a 65C02-superset extension over
  `mos6502`, and `sm83`/`i8080`/`cdp1802`/`i8048`/`scmp`/`f8` are standalone fresh specs, `m6800` is the big-endian Motorola-family root of the 6809). Zero dependencies.
- [`crates/isa-disasm`](crates/isa-disasm) — the spec-driven disassemblers
  (6502, Z80, 68000, 6809, 65816, HuC6280, SM83, 8080, 6800, 1802, 8048, SC/MP, F8), decoding against `isa`.
  Depends only on `isa` + std, so Emu198x can consume disassembly without the
  assembler. See [`decisions/disassembler-crate.md`](decisions/disassembler-crate.md).
- [`crates/asm198x`](crates/asm198x) — the library (dialect-agnostic engine,
  the shared per-CPU cores, the dialect front-ends) and the `asm198x` CLI. It
  re-exports the disassembler from `isa-disasm`.

Delivered so far, all validated byte-identical against the real tool on the
curriculum corpus:

- **6502** — `acme` (C64) and `ca65` (NES) front-ends over a shared
  `dialects::mos6502` core, plus a spec-driven 6502 disassembler. ca65 also
  carries a **bounded ld65-style linker** for the fixed NES config (it emits a
  `.nes` ROM, not a flat binary — see the flat-vs-linked note in the library
  crate docs and `decisions/syntax-stance.md`).
- **Z80** — `pasmo`/`pasmonext` and `sjasmplus` front-ends over a shared
  `dialects::z80` core, the Z80N target, and a spec-driven Z80 disassembler.
- **6809** — `lwasm` front-end (`dialects::lwasm`) over the `isa::mos6809` spec,
  plus a spec-driven 6809 disassembler. First user of the engine's
  **computed-operand seam** (`Operation::Encoded` / `Piece`), for CPUs whose
  operands are computed rather than fixed-width slots. All addressing modes
  (including the full indexed set — the computed postbyte + 0/1/2 extension
  bytes, auto inc/dec, accumulator offsets, indirect, PC-relative), the register
  ops (`tfr`/`exg`/`pshs`/`puls`/`pshu`/`pulu`), and `org`/`equ`/`fcb`/`fdb`/
  `fcc`/`rmb` are landed and validated byte-identical against `lwasm --6809
  --raw`, with assemble→disassemble→reassemble round-trip.
- **65816** — `ca65` syntax (`dialects::ca65_816`) as a **target extension** of
  the 6502: `isa::mos6502` (primary) + `isa::mos65816` (extension), the
  `z80::NEXT` mechanism. Native-mode core: the `m`/`x` immediate width
  (`.a8`/`.a16`/`.i8`/`.i16` → `"immediate"`/`"immediate16"` fixed-slot forms,
  no `Encoded` seam), all new addressing modes (long, `[dp]`, stack-relative, …),
  `z:`/`a:`/`f:` size forces with fall-up, long calls/jumps, the new
  instructions, `mvn`/`mvp`, `cop`/`wdm`, and the `^` bank-byte operator. The
  engine carries 24-bit operands and an `i64` symbol table. A spec-driven
  disassembler tracks `m`/`x` via `rep`/`sep` (emitting `.aXX`/`.iXX`) so
  width-switching code round-trips. Validated byte-identical against `ca65 --cpu
  65816` (flat). Deferred: `.smart` and `@cheap` locals (source conveniences).
- **HuC6280** — `ca65` syntax (`dialects::ca65_huc6280`, `--cpu huc6280`, also
  `pce`) as a **target extension** of the 6502: `isa::mos6502` (primary) +
  `isa::huc6280` (extension, the 65816 pattern). The PC Engine / TurboGrafx-16
  CPU is a 65C02 superset, so the extension carries the 65C02 additions, the
  Rockwell bit ops (`rmb`/`smb`/`bbr`/`bbs`), and the HuC6280-specific
  instructions — `st0`–`st2`, `tam`/`tma`, `tst`, `bsr`, and the block transfers
  `tii`/`tdd`/`tin`/`tia`/`tai`. Every form is fixed-slot (the block transfers
  are opcode + three 16-bit little-endian words, so no `Encoded` seam is
  needed); `z:`/`a:` size forces round-trip low absolutes. A spec-driven
  disassembler (extension searched first) reassembles byte-exact. Validated
  byte-identical against `ca65 --cpu huc6280` across all 1358 audited spec forms.
- **SM83** — `rgbasm` syntax (`dialects::rgbasm`, `--cpu rgbasm`, also `sm83`/
  `gb`) over a **fresh standalone** `isa::sm83` spec (the Game Boy / LR35902 is
  8080-derived and Z80-flavoured but neither, so not a Z80 extension): the
  single-byte main page + the `CB` page, with the SM83-only ops (`ldh`, `ld
  [hl+]/[hl-]`, `ld hl,sp+e`, `add sp,e`, `swap`, two-byte `stop`). Mode labels
  are rgbasm operand templates with upper-case immediate placeholders so they
  never collide with the lower-case register letters. A spec-driven disassembler
  reassembles byte-exact. Validated byte-identical against `rgbasm`/`rgblink`
  (RGBDS) — the spec sweep across every form, plus a full-program differential.
  See [`decisions/sm83-addition.md`](decisions/sm83-addition.md).
- **8080** — Intel syntax (`dialects::i8080`, `--cpu 8080`) over a fresh
  standalone `isa::i8080` spec. The root of the Z80/SM83 lineage: its documented
  opcodes share the Z80's un-prefixed encodings, but the surface is entirely
  Intel (`MOV`/`MVI`/`LXI`/`STAX`/…) with radix-suffixed numbers (`42H`/`101B`/
  `377Q`) — so a dedicated number lexer, and a spec keyed by Intel mode labels.
  Single-byte opcodes, absolute jumps (position-independent disassembly). First
  CPU of the full-coverage roadmap and the debut of `asl` as the reference
  arbiter; validated byte-identical against `asl` (`cpu 8080`) across every form.
  See [`../../decisions/asm198x-cpu-coverage-roadmap.md`](../../decisions/asm198x-cpu-coverage-roadmap.md).
- **6800** — Motorola syntax (`dialects::m6800`, `--cpu 6800`) over a fresh
  standalone `isa::m6800` spec. The 6809's ancestor and a 6502 sibling:
  **big-endian**, `$`-hex, a regular opcode map (immediate `+0`, direct `+0x10`,
  indexed `+0x20`, extended `+0x30`; B-accumulator `+0x40`). Six addressing modes
  (inherent / immediate / direct / extended / indexed / relative) with
  direct-vs-extended by size or a `>`/`<` force, exactly as the 6502 chooses
  zero-page vs absolute — so it reuses the shared `$`-hex lexer. Validated
  byte-identical against `asl` (`cpu 6800`) across every form.
- **CDP1802** — RCA COSMAC, Intel-`H`-suffix syntax (`dialects::cdp1802`,
  `--cpu 1802`, also `cdp1802`/`cosmac`) over a fresh standalone `isa::cdp1802`
  spec. **Big-endian** (the two-byte long-branch address). Landed with **zero
  engine changes**: the register ops
  pack the register number 0..15 into the opcode's low nibble (`INC 3` = `0x13`,
  enumerated one form per register keyed by the number as its mode label, no
  operand byte); the **page-relative short branch** (`0x30`–`0x3F`) — the roadmap
  footnoted it as needing the computed-operand seam — turned out to need only
  `Expr::Lo(target)` as a plain
  one-byte operand (the low byte replaces the PC's low 8 bits), so it stayed pure
  Tier 0. Inherent / immediate / long-branch round out the map.
  The dialect reuses the 8080's Intel number lexer. Validated byte-identical
  against `asl` (`cpu 1802`) across every form. (Deferred nicety: the dialect
  does not yet enforce `asl`'s same-page check on short branches — it needs the
  resolved address at parse time.)
- **8048** — Intel MCS-48 syntax (`dialects::i8048`, `--cpu 8048`, also
  `mcs48`) over a fresh standalone `isa::i8048` spec. The first Wave-B CPU, and
  the first to combine **all three** modelling tools in one chip with **zero
  engine changes**: most instructions are fixed forms whose **mode label is the
  operand template** (the SM83 idiom — `a,r3`, `psw,a`, `bus,#N`, register ops
  enumerated one form per register); the **page-relative conditional jumps**
  (`jc`/`jnz`/`jb0`…/`djnz`) supply `Lo(target)` as a plain byte (the 1802
  trick); and **JMP/CALL** — whose 11-bit address packs its high 3 bits into the
  opcode (`base | (addr>>8 & 7)<<5`) — ride the **computed-operand seam** (the
  6809 tool), the opcode byte built as an `Expr` since the address may be a
  forward label. The dialect reuses the 8080's Intel `H`-suffix lexer. Validated
  byte-identical against `asl` (`cpu 8048`) across every spec form, with the
  computed JMP/CALL cross-checked against `asl` at every address page. (Same
  deferred same-page nicety as the 1802.) This addition also made `--cpu` accept
  the single-dialect chips directly (`--cpu 8048`/`6800`/`1802`/`8080`), matching
  the documented flag.
  - **ROM-less MCS-48 kin** (`--cpu 8035`/`8039`/`8040`, incl. CMOS `80c35`/
    `80c39`/`80c40`) — same `isa::i8048` encoding, no new spec. The one real
    difference: the ROM-less parts commit the bus to fetching external program
    memory, so the four **BUS-port instructions** (`ORL`/`ANL BUS,#`, `OUTL
    BUS,A`, `INS A,BUS`) are rejected (`assemble_8039`, `I8048 { romless }`).
    `asl` (`cpu 8039`) enforces the same restriction; validated byte-identical
    against it across every non-BUS form. The ROM'd larger parts (`8049`/`8050`/
    `80c48`/`80c49`) are plain aliases of the full 8048. The 8021/8022 (reduced
    ISA) and 8041/8042 (UPI) are deliberately *not* aliased — they are different
    instruction sets.
- **SC/MP** — National Semiconductor INS8060 syntax (`dialects::scmp`,
  `--cpu scmp`, also `sc/mp`/`ins8060`) over a fresh standalone `isa::scmp`
  spec. All fixed-slot, **zero engine changes**. The interesting part is the
  addressing: memory is reached through a **pointer register + signed 8-bit
  displacement** — `disp(ptr)` / `@disp(ptr)` — with the pointer (0..3) and the
  `@` auto-index bit resolved at parse time into the form (modes `"0"`..`"3"`,
  `"@1"`..`"@3"`, like the 1802 register nibble), the displacement laid down as
  the following byte. The literal `e` is the E-register index (displacement byte
  `0x80`); the ALU immediates (`LDI`/`ANI`/…) occupy the auto-index-`P0` opcode
  slot, so `@` is pointer-1..3 only. `asl`'s SC/MP mode uses **C-style numbers**
  (`0x..` hex), so this is the first dialect with a `0x` lexer. Validated
  byte-identical against `asl` (`cpu SC/MP`) across every form.
- **F8** — Fairchild F8 (3850) syntax (`dialects::f8`, `--cpu f8`, also `3850`/
  `f3850`/`channelf`) over a fresh standalone `isa::f8` spec. The CPU of the
  **Fairchild Channel F** (1976, the first ROM-cartridge console). **Big-endian**
  (the 16-bit `PI`/`JMP`/`DCI` address), Intel `H`-suffix numbers (reusing the
  8080 lexer). The scratchpad ops (`DS`/`AS`/`ASD`/`XS`/`NS`) and register moves
  (`LR A,r` / `LR r,A`) pack a 4-bit register field into the opcode nibble
  (0–11 direct; `S`/`I`/`D` = 12/13/14 reaching the `ISAR`-pointed register) —
  the 1802/SC-MP idiom; `LIS`/`INS`/`OUTS` (0–15) and `LISU`/`LISL` (0–7) pack a
  value the same way. The novel part is the **relative branch**: the F8 measures
  the signed offset from the *offset byte itself* (one past the opcode), so
  branches (`BT`/`BF` with a test mask, the named `BR`/`BP`/`BC`/`BZ`/`BM`/`BNC`/
  `BNZ`/`BNO`, and `BR7`) ride the **computed-operand seam** (the 6809 tool) with
  a `target + 1` correction against the engine's end-of-instruction `rel` base —
  still **zero engine changes**. `CLR` is a dialect alias for `LIS 0`. Validated
  byte-identical against `asl` (`cpu F3850`) across every spec form.

The engine ↔ dialect ↔ spec seam (and, for ca65, the assemble + link path that
bypasses the flat engine) is documented at the top of `crates/asm198x/src/lib.rs`.
The encoding-model taxonomy (fixed slots / field-packed / computed operand) and
the computed-operand seam are in `../../decisions/packaging-and-cpu-roadmap.md`.

## How correctness is checked

Four layers, each against the real reference assemblers (all `#[ignore]`d — they
need the tools installed — and degrading gracefully when one is absent):

- **`tests/curriculum`** — curated curriculum programs, byte-identical to the
  reference tool, plus assemble→disassemble→reassemble round-trip (our own asm).
- **`tests/conformance`** — three checks, all making the reference tool the
  arbiter by reusing the disassemblers (synthesise bytes → disassemble →
  reassemble with the *reference*): every form-based spec's opcode
  (`spec_opcodes_match_reference`: 6502/Z80/65816/HuC6280/SM83/8080/6800/1802/8048/8039/SC-MP/F8), an opcode-space sweep for
  the non-form specs (`spec_sweep_matches_reference`: 6809 and 68000 — ~33k
  decodable encodings), and a seeded differential fuzzer over random programs
  reassembled by both our asm and the reference (`differential_fuzz`).
  Position-dependent instructions (branches, PC-relative EA) can't be batched, so
  they have targeted round-trip tests instead.

See [`decisions/spec-conformance-and-fuzzing.md`](decisions/spec-conformance-and-fuzzing.md).

## Build-time discipline

The workspace bakes in the levers that keep builds fast — `default-members`
scoped to the CLI, and a `[profile.dev]` that drops full debuginfo (the biggest
`cargo test` cost). Assemblers are featherweight (no `wgpu`/audio/GUI), so this
should stay in the seconds. If a build ever feels slow, the cause is the
dependency graph or profile — never the repo boundary. (Background: this was
measured on Emu198x, whose pain was `cargo test` linking hundreds of
debuginfo-heavy binaries, not its crate count.)

## Where things live

- [`decisions/`](decisions/) — Asm198x-only decisions (syntax stance, dialect
  targets). Cross-project decisions live in [`../../decisions/`](../../decisions/).
- [`crates/`](crates/) — the Rust workspace.
- [`examples/`](examples/) — sample source.

Hardware facts come from the umbrella primary library at [`../../reference/`](../../reference/)
and syntheses at [`../../syntheses/`](../../syntheses/), per
[`../../decisions/shared-hardware-reference-canon.md`](../../decisions/shared-hardware-reference-canon.md).
The `isa` spec is the machine-readable distillation of the encoding slice of
those facts; it cites the library, not the other way round.
