# Decision: the assemble I/O model — broad validatable input, one IR, native-only output

**Status:** Active. Binding for Asm198x. Extends
[`syntax-stance.md`](syntax-stance.md) (per-CPU dialect choices),
[`packaging-and-cpu-roadmap.md`](packaging-and-cpu-roadmap.md) (output containers,
linking, CPU roadmap), and [`spec-conformance-and-fuzzing.md`](spec-conformance-and-fuzzing.md)
(how correctness is checked). Re-anchors that roadmap's "dialect choice follows
consumption, not popularity" stance (see below).

**Date:** 2026-06-04.

## The decision

Three load-bearing principles for how source goes in and bytes come out.

1. **Broad input, but only *validatable* dialects.** Asm198x aims to accept the
   assembly a developer already has for a supported architecture, whatever
   assembler it was written for — *provided* that assembler can serve as a
   byte-for-byte **reference**. Known assemblers only; no dialect ships without a
   runnable reference to diff against.
2. **One internal representation.** Every dialect front-end is a thin parser that
   normalises into a single shared IR; the engine emits from the IR. There is no
   "pasmo engine" or "sjasmplus engine" — those are *front-ends* over one core.
3. **Native output only — never a bespoke Asm198x format. Never ever.** The IR
   renders to the platform's *native* containers (`.prg`, `.sna`, `.tap`, hunk
   exe, `.nes`, …) and to nothing else. The IR is internal plumbing; it is never
   serialised as a shippable artifact.

Corollary: **Asm198x and Emu198x meet only at native formats.** Emu198x loads
native containers (it must, to be a real emulator); Asm198x exports them. There
is no private Asm198x→Emu198x channel and no shared custom format between them.

## Input: known dialects only, each reference-validated

A dialect earns a front-end when — and only when — its reference assembler can be
*run* to produce ground truth. That reference is a **validation-time dependency,
never a runtime one**: it runs however it must — natively, in Docker, or (for a
period assembler that needs a dead OS) **under Emu198x emulation** — its output is
diffed against ours over the curriculum corpus and an opcode sweep, and then it is
never needed again. A user of Asm198x never installs it.

This closes the rescue loop. Asm198x exists because period assemblers need
dead-OS emulation to run and modern ones are fragmented; validating a modern
reimplementation by running the *original* period tool under Emu198x is that same
rescue motivation applied to QA — emulate the dead tool once, to retire the need
for it forever.

**This re-anchors the old stance.** `packaging-and-cpu-roadmap.md` said dialect
choice "follows Code198x/Emu198x consumption, not popularity." That gated dialects
to *only what the curriculum consumes*. The gate is now **validatability**, and
the goal is **breadth** within it: support the known assemblers for an
architecture, prioritised by what Code198x/Emu198x consume, bounded by what we can
diff against a reference. Consumption sets *priority*; a runnable reference sets
the *gate*; breadth is the *aim*.

## Internal representation: one IR, thin front-ends

This is largely the existing architecture, stated as a principle. `acme` and
`ca65` are front-ends over a shared `dialects::mos6502` core; `pasmo`/PasmoNext
and `sjasmplus` over a shared `dialects::z80` core; `lwasm` over `dialects::lwasm`;
`ca65` (65816) over the 6502 core + `mos65816` extension. Each parses its dialect
and hands the engine a neutral operation stream; the engine owns two-pass
resolution, the symbol table, `org`/`equ`, and emission.

**The one seam:** 68000/vasm runs its own driver (`assemble_core`) because the
68000 needs layout/relaxation/relocation machinery the flat engine lacks. That
split is by **CPU complexity, not by assembler** — exactly *not* the per-dialect
duplication this decision forbids — and unifying the two drivers is already
deferred with a trigger (a second CPU needing relocation), per
`packaging-and-cpu-roadmap.md`. Acceptable seam; named, not hidden.

## Output: native containers off the IR

Output is an **output-container layer keyed by target machine**, decoupled from
assembly: the IR renders to each native format through a thin serialiser. Each
serialiser carries a **correctness tier**:

- **Reference-backed** — a reference tool emits the format, so the serialiser is
  validated byte-identical: `.prg` (acme `cbm`), `.sna` / `.tap` (pasmo + sjasmplus),
  `.tzx` / `+3DOS` (pasmo), `.nex` (sjasmplus), hunk exe (vasm), `.nes`
  (ca65+ld65).
- **Emulator-load-validated** — *no* reference assembler emits it, so it is built
  from spec and validated by loading it (in Emu198x or a known-good emulator) to
  the expected state: `.z80`, `.szx`.

**The (dialect × format) matrix exceeds any single reference tool.** ACME assembles
C64 source and emits `.prg`; it cannot emit a tape. So an "off-diagonal" pairing —
acme-syntax source rendered to a tape format — has *no single* reference tool, and
rests on the **composed** guarantee (front-end verified against its dialect's
reference + serialiser verified against its format's reference + a dialect-neutral
IR) plus an emulator-load check, not a one-shot byte-diff. The single IR (principle
2) is exactly what makes this composition sound: any validatable input can meet any
validatable output through a neutral pivot.

### Program framings vs filesystem volumes — the scope line

Asm198x emits **program framings**: the assembled image and its loader-ready
forms — `.bin`/`.prg`/hunk exe/`.nes` (executables), `.sna`/`.z80`/`.szx`
(machine-state snapshots), `.tap`/`.tzx` (sequential tape streams), `+3DOS` (a
single-file *headered* form). Each is "the program, or its state, framed for a
loader" — no filesystem.

It does **not author filesystem volumes**: `.d64`, `.adf`, `.dsk`, `.trd`
(TR-DOS), `.mdr` (Interface 1 Microdrive), `.g64`. Each needs a directory/catalog,
an allocation map, and files placed into a medium — disk authoring, a separate
competence owned by the disk-authoring sibling **Build198x**, validated by
emulator-load or a disk tool (`xdftool`, `c1541`), not an assembler reference.

**The line is filesystem-*volume* vs single-program *framing*, and it is
categorical — even where a reference assembler can dribble a volume out.**
sjasmplus's `SAVETRD` emits a `.trd`; we still keep it out, because the
alternative is Asm198x absorbing TR-DOS / FFS / CBM-DOS / Microdrive filesystem
implementations — scope it must not take on. The split is principled, not
arbitrary: `.tap` (stream) is in, `.mdr` (catalog over a tape loop) is out;
`+3DOS` (single headered file) is in, `.trd` (TR-DOS volume) is out. The
validatability tiers above then apply *within* the in-scope framings.

**Per-platform output targets** (the menu, not a commitment to build all at once):

- **Amiga** — hunk exe ✅ (done), flat `.bin`. `.adf` is a **filesystem volume** — out (disk authoring, per the scope line above). Bootblock-takeover software has no hunk-exe deliverable; there, Asm198x still emits the bootblock + payload (raw/hunk) and the bootable `.adf` is assembled by the disk-authoring tool.
- **NES** — `.nes` ✅ (done, bounded ld65 config).
- **C64** — `.prg` (the only assembler-native format; ACME does only plain/cbm/apple). `.crt` is a *different target class* (cartridge ROM, not a RAM program) — deferred unless cartridge dev appears. `.d64`/`.t64` are filesystem volumes and the C64 `.tap` is a low-level cassette image — none is assembler-native; all out (Build198x).
- **Spectrum** — sequence by tier: **`.sna` + `.tap` + `.tzx`** first (reference-backed; tapes also carry the authentic `LOAD` experience), then **`.z80`** (the universally loadable snapshot; emulator-load-validated), with `.szx` / `+3DOS` / `.nex` as later additions when a need is concrete. `.trd` (TR-DOS) and `.mdr` (Microdrive) are filesystem volumes — out.

### Format selection: flag or directive, fail-fast on conflict

A target format may be chosen by **CLI flag** (pasmo/acme style: `--sna`, `-f cbm`)
**or** by an **in-source directive** (sjasmplus style: `SAVESNA`, `SAVETAP`), so
either dialect's source stays source-compatible. Resolution:

- **Neither** — default to flat `.bin` (today's behaviour).
- **One** — use it.
- **Both, agreeing** (same format *and* same key params — output target, start
  address) — allow; the flag is redundant, not an error.
- **Both, conflicting** — **error, fail-fast.** "Conflict" is format mismatch
  (`--sna` + `SAVETAP`) *or* key-param mismatch (`--tzx` + `SAVETAP foo,$8000`).
- **Several directives** of different formats — *allowed*; that is intentional
  multi-output (sjasmplus emits a `.sna` and a `.tap` from one source). A single
  CLI flag is single-output. A flag that contradicts a directive is the conflict
  case above; a flag alongside agreeing directives is fine.

## Linking: fused — entailed by native-only, not merely deferred

The owning decision is [`packaging-and-cpu-roadmap.md`](packaging-and-cpu-roadmap.md)
§ 3: assemble-and-link stays **fused** (one source → final native image; no
standalone `ld`-style linker, no object format), deferred as YAGNI until a real
separate-compilation need appears. Principle 3 here *strengthens* that from
"deferred" to **entailed**: a standalone linker consumes **object files**, and an
object file is a serialised, non-native intermediate — precisely what "never
serialise the IR, native output only" forbids. So the native serialisers (hunk,
`.nes`) do their relocation and section layout **inside** the fused IR→container
step, and the IR carries what that needs (sections, symbols, relocations) without
ever being written out.

The bar for separate compilation is therefore higher than "someone wants it": it
would mean adopting an **existing, validatable** object format (ca65's `.o`, ELF,
…), so the reference-diff model still holds — gated on a runnable reference like
any dialect, and a decision owned by `packaging-and-cpu-roadmap.md` § 3, not a
casual feature add. The fused stance stands; this is *why* it's principled.

## Drift triggers

- **"Add a dialect we can't run a reference for"** — no. Validatability is the
  gate; if we can't diff it byte-for-byte against the real tool (native, Docker, or
  the period tool under Emu198x), it does not ship.
- **"Invent an Asm198x output/interchange format"** — **never.** Native containers
  only. The IR is internal and is never serialised as an artifact.
- **"Wire Asm198x directly into Emu198x for loading"** — no; they meet at native
  formats, nowhere else.
- **"Keep a separate engine per assembler"** — no; front-ends over one IR. The
  only driver split is by CPU complexity (68000/vasm), deferred-unify, not by
  dialect.
- **"Pick dialects by popularity"** *or* **"only support what the curriculum
  consumes"** — neither extreme: validatability-gated breadth, consumption-prioritised.
  This supersedes the older consumption-only framing in
  `packaging-and-cpu-roadmap.md`.
- **"Flat `.bin` is good enough, skip the native container"** — only where the
  consumer loads flat. Otherwise the container *is* the artifact the emulator and
  real hardware load.
- **"Author a filesystem volume (`.adf`/`.d64`/`.dsk`/`.trd`/`.mdr`) — a reference
  can emit it / the curriculum needs a disk"** — no; volumes are disk authoring,
  owned by the **Build198x** sibling. Asm198x emits program framings only
  (executable / snapshot / tape), even when a reference assembler can dribble a
  volume out. The line is filesystem-volume vs single-program framing.
- **"Add a standalone linker / object-file format"** — an object file is a
  serialised non-native intermediate, which principle 3 forbids; linking stays
  fused. Reopening it means adopting an *existing, validatable* object format — a
  separate, high-bar decision owned by `packaging-and-cpu-roadmap.md` § 3.

## Log

### 2026-06-04 — Decided

Crystallised during the dev-tooling-migration discussion (umbrella
[`code198x-dev-tooling-migration.md`](../../../decisions/code198x-dev-tooling-migration.md)),
which surfaced that Asm198x emits flat `.bin` for the 6502/Z80 dialects where the
curriculum needs `.prg` (C64) and `.sna` (Spectrum). Working through the output
question raised, and rejected, a tempting wrong turn — a single "standard" Asm198x
artifact that Emu198x would load — on three grounds: it wouldn't remove the
per-format work (it relocates it behind a translator that still must match the
reference byte-for-byte), it regresses the reference-backed correctness model (our
own format has no reference), and it walls the learner's artifact off from Fuse /
real hardware / WoS. The resolution is the three principles above: broad
*validatable* input → one IR → native output only, never a bespoke format, with
Emu198x and Asm198x meeting solely at native containers. The "known assemblers
only" bound was chosen explicitly as a *validatability* constraint, not a
popularity one — every dialect must be diffable against a runnable reference, even
if that reference is a dead-OS period tool run under Emu198x.

### 2026-06-04 — Disk authoring split to a new sibling, Build198x

The scope line's "out" side gained a home. Filesystem-volume / disk-media
authoring (ADF, D64, DSK, TRD, MDR, bootable images) is a **separate sibling**,
**Build198x** (GitHub org grabbed 2026-06-04) — not Cat198x, and not an ad-hoc
build step. The boundary: Asm198x emits program framings; Build198x masters them
onto media. Build198x's full scope (disk/media mastering vs broader build
orchestration) and its own umbrella record are still to be settled.

### 2026-07-08 — Tape framing vs tape mastering clarified

Build198x's tape-master demand gate (Gloaming's cassette packaging, fired
2026-07-03) reopened where a bootable tape sits on the seam. Resolved at the
umbrella —
[`tape-framing-vs-mastering.md`](../../../decisions/tape-framing-vs-mastering.md)
— by splitting on **composition**, which *confirms* this record rather than
amending it: a `.tap`/`.tzx` whose content is the assembled program and
nothing else stays an Asm198x framing, **including** the minimal auto-run
BASIC stub where the reference tool emits one (pasmo `--tapbas`, sjasmplus
`SAVETAP` — stub parity is entailed by byte-identical dialect validation).
Composing *multiple* artifacts onto a tape — an authored BASIC loader, a
SCREEN$ loading screen, multiload — is mastering, owned by Build198x, exactly
parallel to the bootblock-vs-bootable-ADF split above. The Spectrum
`.tap`/`.tzx` first-wave targets in this record are unchanged.
