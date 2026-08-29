# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.46](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.45...asm198x-v0.0.46) - 2026-08-29

### Added

- *(asl)* share character translation ([#402](https://github.com/asm198x/asm198x/pull/402))

### Added

- **Shared ASL character translation.** All twelve ASL-syntax CPU front ends
  now accept `charset source,replacement`; mappings apply to strings and
  character literals, accumulate across included source, and reset to the
  identity table with a bare `charset`, matching ASL 1.42.

## [0.0.45](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.44...asm198x-v0.0.45) - 2026-08-29

### Added

- **Shared ASL enumerations.** All twelve ASL-syntax CPU front ends now accept
  `enum` lists. Members start at zero, explicit assignments reset the running
  value, later assignments may use earlier members, and each new directive
  starts a fresh sequence; the resulting constants are included in symbol and
  Debug198x output.

## [0.0.44](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.43...asm198x-v0.0.44) - 2026-08-29

### Added

- **Shared ASL input radix.** All twelve ASL-syntax CPU front ends now accept
  `radix 2` through `radix 36`. The selected base applies to unadorned numeric
  tokens, constants, instruction operands, and included source; changes made
  inside an include remain live when assembly returns to its caller.

## [0.0.43](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.42...asm198x-v0.0.43) - 2026-08-29

### Added

- **Shared ASL phased addresses.** All twelve ASL-syntax CPU front ends now
  accept nested `phase`/`dephase` regions, so labels and PC-relative operands
  use the claimed address while emitted bytes remain at their real location.
  Edge cases match asl 1.42: an unmatched `dephase` is inert, and a phase may
  remain open at end of source.

## [0.0.42](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.41...asm198x-v0.0.42) - 2026-08-29

### Added

- **Shared ASL alignment.** All twelve ASL-syntax CPU front ends now accept
  `align boundary[,fill]`, including non-power-of-two boundaries. The default
  `$FF` holes match `asl` plus `p2bin`; custom fill values occupy one target
  address unit, including full 16-bit decles on the word-addressed CP-1600.

## [0.0.41](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.40...asm198x-v0.0.41) - 2026-08-29

### Added

- **lwasm OS-9 modules.** `mod` emits and checksums the four- or six-field
  module header, establishes module-relative address zero, `os9` emits the
  system-call sequence, and `emod` closes the module with OS-9's CRC-24. The
  complete module is byte-identical to lwtools 4.25's OS-9 output.

## [0.0.40](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.39...asm198x-v0.0.40) - 2026-08-29

### Fixed

- *(lwasm)* warn for pass conditionals

### Added

- **lwasm string symbols and generated source.** `setstr` now defines and
  redefines general-string values, `%(name)` interpolates them where lwtools
  does, `includestr` assembles the constructed source lazily, and all twelve
  `ifstr` comparison forms are available. Generated source in an untaken
  conditional is never parsed.

- **Pasmo projects can include source files.** `include "file"` now uses the
  same loader-backed, requester-relative project model as the other multi-file
  dialects. Nested includes share constants and labels with their includer,
  preserve per-file diagnostics, and are loaded only in taken conditional
  branches.

## [0.0.39](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.38...asm198x-v0.0.39) - 2026-08-29

### Fixed

- *(ca65)* discard errors in dead branches ([#385](https://github.com/asm198x/asm198x/pull/385))
- *(ca65)* accept labels on block heads ([#384](https://github.com/asm198x/asm198x/pull/384))

## [0.0.38](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.37...asm198x-v0.0.38) - 2026-08-28

### Fixed

- *(lwasm)* match direct-page and reserved-region layout

## [0.0.37](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.36...asm198x-v0.0.37) - 2026-08-28

### Changed

- Publish the `asm198x` crate to crates.io for the first time. The executable
  ISA specs and shared disassemblers have graduated with their history to the
  neutral Isa198x project as `isa198x` and `isa198x-disasm` 0.1.0. Asm198x now
  consumes those registry releases instead of owning their source, and Emu198x
  can version the same shared layer without raw commit pins.

## [0.0.36](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.35...asm198x-v0.0.36) - 2026-08-28

Two Spectrum tape targets, and one crate leaves the workspace. `--tap` and
`--tzx` complete the tier-one Spectrum outputs alongside `.sna`, and
`debug198x` — the debug-info format Asm198x writes and Emu198x reads — now
comes from crates.io rather than from this repository.

### Added

- **Spectrum `.tap` and `.tzx`, with pasmo's auto-run BASIC loader.** `--tap`,
  `--tapbas`, `--tzx` and `--tzxbas` produce output byte-identical to PasmoNext
  v0.1.3. Two behaviours are worth knowing before scripting them: the tape
  block name is **the output path as given**, clipped to ten characters and
  space-padded — `sub/o.tap` names the block `sub/o.tap`, slash and extension
  included — and the loader's `RANDOMIZE USR` line is written only when the
  source has an `end` directive, so a source without one loads and stops. The
  `.tzx` files are version 1.13, which is pasmo's.
  ([#369](https://github.com/asm198x/asm198x/pull/369))

### Changed

- **`debug198x` is a published crate rather than a workspace member.** The
  debug-info format is the contract between Asm198x and Emu198x, and it now
  lives at [debug198x/debug198x](https://github.com/debug198x/debug198x) and on
  [crates.io](https://crates.io/crates/debug198x). Emu198x had been pinning
  three crates out of this repository at two raw revisions; `debug198x = "0.1"`
  replaces that arrangement for one of them. Its released versions restart at
  **0.1.0**, because the old 0.0.x numbers were this workspace's lockstep
  version and said nothing about the format. Nothing about the emitted sidecar
  changes — the full suite passes unchanged against the published crate — and
  the spec moved with the code, so `decisions/debug198x-format.md` is now in
  the new repository.
  ([#372](https://github.com/asm198x/asm198x/pull/372))

- **The toolchain tracks Rust 1.98.0**, up from a 1.95.0 pin that had been
  inherited by copying a sibling repository rather than chosen. One new lint
  had a single site in the workspace. Both compilers give the same 1223
  passing tests.
  ([#362](https://github.com/asm198x/asm198x/pull/362))

### Fixed

- **The 6809 dialect said indexed addressing was unsupported. It works.** Three
  comments claimed it, one of them on a public entry point that ships to
  docs.rs — so someone evaluating the crate for 6809 work read that it could
  not assemble most real 6809 code, indexed being the 6809's characteristic
  mode. The same header called the register-list operations `tfr`, `exg`,
  `pshs` and `puls` "the next increment", and those work too. All of it is
  byte-identical against `lwasm --6809 --raw`. What is genuinely outstanding on
  lwasm is ten directives, and the replacement comments point at
  `cargo xtask surface` for that rather than restating a list that can go stale
  the way these three did.
  ([#364](https://github.com/asm198x/asm198x/pull/364))

- **27 rustdoc warnings, four of them broken links on the first page a reader
  lands on.** `asm198x`'s crate-level architecture paragraph links to `engine`,
  `Dialect` and `dialects` to explain the engine ↔ dialect ↔ spec seam; all
  three are private, so docs.rs rendered them as plain text with no sign a link
  was intended. `sna_48k` named a type that has never existed. CI now builds
  the crate documentation with `-D warnings`, which is the part that stops the
  count growing unseen again.
  ([#366](https://github.com/asm198x/asm198x/pull/366))

## [0.0.35](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.34...asm198x-v0.0.35) - 2026-08-27

The release where the references' string features stopped being a gap. ca65 and
rgbasm both express whole idioms through text the assembler builds *before* it
parses — `STRFMT`, `.sprintf`, `.concat`, `.ident`, `.match` — and none of it
existed here. It does now, and as a source pre-pass rather than a second type in
the expression language, which is a decision with a record behind it. 364
outstanding reference words to **238**: ca65 alone goes from 78 to 24.

### Added

- **Strings are a source pre-pass, not a second type in the expression
  language.** An expression evaluates to an `i64`, and continues to. The
  references' string features are resolved before the parse, in the shape macro
  expansion already used: symbols are collected, functions are folded to the
  text they produce, and the ordinary parse reads a string literal or a number
  as it always would. The alternative — a string type in the expression tree —
  would make every dialect's evaluation two-typed and still could not answer
  ca65's token-list functions, which compare things that are neither number nor
  string. The one case a pre-pass cannot reach is a string function applied to a
  label's address; ca65 refuses that itself, and rgbasm's version of it is
  refused here by name rather than answered wrongly.
  ([#350](https://github.com/asm198x/asm198x/pull/350))

- **rgbasm's string vocabulary — thirteen words.** `EQUS`, `STRCAT`, `STRUPR`,
  `STRLWR`, `STRSUB`, `STRSLICE`, `STRLEN`, `STRCMP`, `STRFIND`, `STRIN`,
  `STRRIN`, `STRRPL` and `STRFMT`. The index conventions are the part worth
  knowing: `STRSUB` is 1-based and takes a *length*, `STRSLICE` is 0-based and
  takes an *end*, `STRFIND` answers a 0-based index or `-1`, and `STRIN`
  answers a 1-based one or `0`. `STRFMT` is printf's shape and not printf's
  rules — `%#x` writes `$ff`, `%#f` appends `q16` as a suffix, `%f` reads its
  argument as a raw Q16.16 value, and the flags come in a fixed order, so `%+#x`
  assembles where `%#+x` does not.
  ([#351](https://github.com/asm198x/asm198x/pull/351),
  [#353](https://github.com/asm198x/asm198x/pull/353))

- **ca65's string vocabulary — `.concat`, `.string`, `.ident`, `.sprintf`.**
  `.string` stringifies its argument's *token* rather than its value: with
  `N = 7`, `.string(N)` is `"N"`. `.ident` builds a name from text and resolves
  it like any other, forward references included. `.sprintf` is C's shape with
  ca65's own departures — `%x` is signed where `%X` is not, so the two disagree
  about a negative value; `%s` and `%c` pad on the right by default and on the
  left with `-`, the reverse of every other conversion; and `#` on `%x` shows
  its prefix even for zero. All 135 measured cases are in the test.
  ([#354](https://github.com/asm198x/asm198x/pull/354),
  [#357](https://github.com/asm198x/asm198x/pull/357))

- **ca65's token lists — `.match`, `.xmatch`, `.tcount`, `.blank`, `.left`,
  `.mid`, `.right`.** A token list is unevaluated source, so these answer over
  what is *written*. `.match` asks what each token **is** and `.xmatch` asks
  what it **says**: `.match({1},{2})` is 1 and `.xmatch({1},{2})` is 0, while
  `.match({a},{b})` is 0 because `a` is the accumulator and `b` is a name. The
  register set follows the CPU, which is observable — `.match({s},{q})` is 1 for
  a 6502 and 0 for a 65816, where `s` is the stack register.
  ([#359](https://github.com/asm198x/asm198x/pull/359))

- **ca65's four remaining predicates — `.const`, `.ismnem`, `.paramcount`,
  `.definedmacro`.** `.const` answers 0 for a constant defined *below* the line,
  which is what a pass walking in source order sees anyway, and errors for a
  name defined nowhere, which is what ca65 does rather than answering 0.
  `.ismnem` follows the CPU. `.paramcount` counts the **call site** and not the
  declared parameters, so a macro with two of them called with one answers 1.
  `.definedmacro` is answered above the line that asks — for a line inside a
  body, the line the macro was invoked on.
  ([#360](https://github.com/asm198x/asm198x/pull/360),
  [#361](https://github.com/asm198x/asm198x/pull/361))

- **rgbasm's fixed-point arithmetic.** Q16.16 literals with an optional `q`
  precision suffix, `MUL`, `DIV`, `FMOD`, `FLOOR`, `CEIL`, `ROUND`, `TZCOUNT`,
  `HIGH`, `LOW` and `dl`. Only the operations whose answers are exactly defined;
  the transcendental ones remain a named gap rather than an approximation.
  ([#343](https://github.com/asm198x/asm198x/pull/343))

- **vasm goes from 87 outstanding words to 53.** The listing controls, and the
  two that say something — `printt` and `printv`. Seven more conditional heads
  (`ifb`, `ifnb`, `ifc`, `ifnc`, `ifmi`, `ifpl`, `elseif`), where `elseif` reads
  its argument and ignores it. The offset counters, which turn out to be two
  rather than one: `rs` and `so` share a counter and `fo` counts down.
  ([#345](https://github.com/asm198x/asm198x/pull/345),
  [#347](https://github.com/asm198x/asm198x/pull/347),
  [#349](https://github.com/asm198x/asm198x/pull/349))

- **sjasmplus's sixteen data directives beyond the shared set.** The `dc`/`dz`
  string forms, the wide and graphic byte spellings, and the marked variants —
  where `dc` marks the last character of each *string*, not of the whole list.
  ([#348](https://github.com/asm198x/asm198x/pull/348))

- **ca65's structure, reference and listing vocabulary.** `.proc` and `.scope`,
  with names inside them qualified as ca65 qualifies them. The record types
  `.struct`, `.union`, `.enum`, `.tag` and `.sizeof`. `.ref`/`.referenced` and
  the two conditionals over them, where a use in a branch the assembler never
  took does not count. `.org`, `.reloc` and `.end` — `.org` moves the *address*
  and not the bytes. The processor words, with the processors we do not assemble
  named as the gap rather than silently accepted. And the eight words that
  address the listing rather than the bytes.
  ([#334](https://github.com/asm198x/asm198x/pull/334),
  [#338](https://github.com/asm198x/asm198x/pull/338),
  [#339](https://github.com/asm198x/asm198x/pull/339),
  [#340](https://github.com/asm198x/asm198x/pull/340),
  [#341](https://github.com/asm198x/asm198x/pull/341),
  [#342](https://github.com/asm198x/asm198x/pull/342))

### Fixed

- **A fixed-point literal truncated where rgbasm rounds.** `0.1` assembled to
  `$1999` against rgbasm's `$199A`, and `0.3` to `$4CCC` against `$4CCD`. The
  test that should have caught it pinned `3.7` as `$3B333` and read that as
  evidence of truncation — but `242483.2` rounds *down*, so both rules agree
  there, and the one probe that would have told them apart was never asked.
  ([#352](https://github.com/asm198x/asm198x/pull/352))

- **The formatter turned an else-if chain into source the assembler refused.**
  A leg is stored as a conditional nested in the else branch, and it came back
  out flat only for `.elseif`; rgbasm spells it `ELIF`, so its chains were
  re-emitted as `ELSE` followed by `ELIF`. The closer was the same bug's other
  half: the author writes one, the walk gives it to the innermost leg, and the
  outer block then derived `ENDIF` — which rgbasm refuses outright. With that
  honest, vasm's `elif` was no longer blocked and landed alongside it.
  ([#355](https://github.com/asm198x/asm198x/pull/355))

- **rgbasm's formatter was never checked, and could not format the text
  layer.** The invariant that says `fmt` must read what `asm` reads walks a list
  of dialects, and rgbasm was missing from it — so eight probes assembled and
  would not format with nothing to say so. All eight now do, and the ledger of
  known formatter gaps stays empty over one more dialect.
  ([#356](https://github.com/asm198x/asm198x/pull/356))

### Assurance

- **85 fuzz programs the 6809 corpus had never held.** Seeded differential
  programs recorded against lwasm, so the fuzzer's findings are replayable on a
  machine with no reference tools installed rather than re-derived each run.
  ([#344](https://github.com/asm198x/asm198x/pull/344))

## [0.0.34](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.33...asm198x-v0.0.34) - 2026-08-26

The release where source compatibility stopped being a claim about a few
dialects and started being measured against all six references. 476 words the
installed reference assemblers accept and asm198x did not are now 364; ACME
reaches **zero**. And an assembler can now write the files a source asks for
besides the machine code — including a Spectrum tape image.

### Added

- **A source can name the files it wants written.** ACME's `!to`, vasm's
  `output`, sjasmplus's `SAVEBIN` and `SAVETAP` all state an output file in the
  source rather than on the command line, and each is now honoured. `SAVETAP`
  writes a real `.tap`: the tape image the Spectrum's ROM loader reads, built on
  `format198x-sinclair-zx-spectrum-tap`, a crate graduated out of the emulator
  into Format198x and published so anything can read one.
  ([#319](https://github.com/asm198x/asm198x/pull/319),
  [#320](https://github.com/asm198x/asm198x/pull/320),
  [#321](https://github.com/asm198x/asm198x/pull/321))

- **ACME assembles what ACME assembles.** Every word ACME 0.97 accepts is
  accepted here: the conversion tables `!ct`/`!convtab`, the endian data family
  `!be16` through `!le32`, the condition loops `!while`/`!do`, the second
  location counter `!pseudopc`, `!xor` and `!scrxor`, `!addr`, `!skip`,
  `!initmem`, `!as`, `!rs`, `!eof`, `!fi`, `!raw`, `!hex`, `!symbollist` and
  `!cpu 6502`. 31 outstanding words to none.
  ([#289](https://github.com/asm198x/asm198x/pull/289) through
  [#303](https://github.com/asm198x/asm198x/pull/303))

- **lwasm goes from 53 outstanding words to 10.** The block and string data
  forms, the listing controls, `warning`/`msg`, `setdp` (which emits the offset
  within the page), `phase`/`dephase`, `set` and plain `if` on a redefinable
  symbol, `incl`/`lib`/`reorg`, `struct`/`endstruct`/`ends`, and the
  `pragma`/`opt`/`ifpragma`/`ifopt` family across 22 switches. The words it
  still refuses, it refuses by name and says why.
  ([#304](https://github.com/asm198x/asm198x/pull/304) through
  [#312](https://github.com/asm198x/asm198x/pull/312))

- **ca65 goes from 113 to 78.** Its operator vocabulary in both spellings —
  `&&` and `.and` are one operator, and the keyword now lands on its symbol
  twin's token rather than getting a second, nearly-right precedence. The
  plural byte extractors `.lobytes`/`.hibytes`/`.bankbytes`/`.faraddr`. Nine
  conditional heads beyond `.if`: `.ifblank`, `.ifconst` and the `.ifpNN` CPU
  tests, where `.ifconst` follows ca65's own rule that a difference of two
  labels in one segment is constant and a label alone is not.
  ([#323](https://github.com/asm198x/asm198x/pull/323),
  [#324](https://github.com/asm198x/asm198x/pull/324),
  [#326](https://github.com/asm198x/asm198x/pull/326),
  [#330](https://github.com/asm198x/asm198x/pull/330))

### Fixed

- **A data list split inside a function call.** `.word .max($100, $200), $3`
  was read as three items rather than two, because the operand splitter counted
  commas without counting parentheses. Anything passing a two-argument function
  in a data list assembled wrongly or was refused.
  ([#322](https://github.com/asm198x/asm198x/pull/322))

- **A value's range came from its width rather than its dialect.** The engine
  held one hardcoded answer — `-128..=0xFF` for a byte, `0..=0xFFFF` for a word
  — where the references disagree on two counts: ca65 refuses a negative value
  a byte directive would take elsewhere, and lwasm, vasm, rgbasm, pasmo and
  sjasmplus truncate a value out of range where acme and the asl-backed
  dialects call it an error. Probed at the corners across all seven.
  ([#291](https://github.com/asm198x/asm198x/pull/291))

- **ACME's retired spellings read as our gap.** Words ACME itself removed were
  declared unimplemented, which put them on the wrong side of the ledger: they
  are the reference's refusal, not ours.
  ([#288](https://github.com/asm198x/asm198x/pull/288))

### Assurance

None of this changes what the assembler does. It changes what is proven, and
what a wrong claim costs.

- **Every 68000 form is arbitrated against vasm.** All 838 rows of the
  specification, encoded and compared against the reference's own bytes.
  ([#284](https://github.com/asm198x/asm198x/pull/284))

- **Form coverage counted verdicts, not forms.** A form arbitrated by two
  versions of one tool counted twice, so a 68000 sweep under both installed
  vasm builds published "Form coverage: 1676/838 (200.0%)". It counts distinct
  forms now, keyed on the form's label rather than its source text — two forms
  can share source, since an assembler that canonicalises `move.w d1,a2` to
  `movea.w d1,a2` writes one line for both.
  ([#331](https://github.com/asm198x/asm198x/pull/331))

- **A version claim in the source has to name a version that was recorded.**
  Prose here cites reference-tool behaviour by version constantly, and nothing
  checked those citations against the corpus that observed them.
  ([#317](https://github.com/asm198x/asm198x/pull/317))

- **The ledger's pinned curriculum date is checked against the commit it
  names.** The revision itself is a CI checkout ref and cannot drift unseen;
  the date beside it was hand-maintained and checked by nothing.
  ([#286](https://github.com/asm198x/asm198x/pull/286))

- **The parity goal is written down, with a register of deferrals.** Every word
  a reference accepts and asm198x does not is either outstanding or deferred
  with a stated reason, and a deferral with no record is treated as a backlog
  item wearing a decision's clothes.
  ([#313](https://github.com/asm198x/asm198x/pull/313))

## [0.0.33](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.32...asm198x-v0.0.33) - 2026-08-25

Three changes to what asm198x accepts or emits, and a body of work behind them
proving the rest of it right. If you assemble Z8000 source, **this release
changes your output** — see the first entry.

### Fixed

- **`LDB Rbd, #data` assembled to four bytes where the reference produces two.**
  The Z8000 has two encodings for a byte immediate loaded into a register, and
  the CPU manual settles which to write: "although two formats exist for
  `LDB R, IM` the assembler always uses the short format". asm198x wrote the
  long one, and could not read the short one back at all — a binary built by
  `asl` disassembled to a `word` directive where the instruction should be. It
  now writes the short form and reads both. Because this is a size change and
  not only an encoding choice, anything position-dependent after an `LDB #`
  moves.
  ([#254](https://github.com/asm198x/asm198x/pull/254),
  [#252](https://github.com/asm198x/asm198x/issues/252))

- **A replay mismatch on a sweep chunk printed the byte vectors and nothing
  else.** The largest chunk is 4,096 instructions, so a single failure was
  around 147,000 characters that never said which instruction moved. It now
  names the first differing byte, the source line and the instruction there.
  ([#269](https://github.com/asm198x/asm198x/pull/269),
  [#268](https://github.com/asm198x/asm198x/issues/268))

### Added

- **The 6809 accepts `reset`, `rhf` and `hcf`.** `lwasm --6809` assembles all
  three, so source using them is no longer refused; the bytes match, `3E 14 14`.
  The disassembler never writes them, and that asymmetry is deliberate: `$14`
  is `SEXW` on the Hitachi 6309, so `fcb $14` is the reading that holds whichever
  part produced the byte, while `hcf` would assert both the part and that the
  byte was meant as code. Neither opcode has a defined result, so no working
  program contains one on purpose.
  ([#257](https://github.com/asm198x/asm198x/pull/257),
  [#233](https://github.com/asm198x/asm198x/issues/233))

### Changed

- **vasm's `xref`, `import` and `nref` are accepted when the name is defined.**
  vasm 2.0b refused them in binary output from both directions at once, so no
  program satisfied them and asm198x refused them too. 2.0f accepts them, which
  is the same rule as the seven visibility words beside them. Matching the
  reference means this changes with the reference.
  ([#282](https://github.com/asm198x/asm198x/pull/282))

### Assurance

None of this changes what the assembler does. It changes how much of it is
proven against a real reference assembler rather than asserted.

- **The Z8000 and Z8001 go from nothing to every row.** Both now arbitrate all
  271 rows of their specification against `asl` — encoding a representative
  instance of each row, disassembling it, and comparing the reference's bytes.
  The previous attempt placed 42 and was abandoned. The 6809, PDP-11, TMS9900
  and CP-1610 gained the same audit; eighteen of twenty CPUs now arbitrate every
  row they declare, and the two that do not say in the repository why.
  ([#242](https://github.com/asm198x/asm198x/pull/242) through
  [#259](https://github.com/asm198x/asm198x/pull/259))

- **A conformance ledger is published with the documentation**, generated from
  the corpus and held to it on every pull request, so a release cannot carry one
  that has stopped being true. It names the release, the corpus hash, the pinned
  curriculum revision, and per CPU the arbiter, its version and what it proved.
  ([#266](https://github.com/asm198x/asm198x/pull/266))

- **Arbitration coverage is governed rather than reported.** A shortfall states
  its size and its reason, a CPU that arbitrates nothing cannot merge, and a
  release will not tag while any shortfall is still owed.
  ([#261](https://github.com/asm198x/asm198x/pull/261),
  [#262](https://github.com/asm198x/asm198x/pull/262),
  [#263](https://github.com/asm198x/asm198x/pull/263))

## [0.0.32](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.31...asm198x-v0.0.32) - 2026-08-24

Two correctness fixes, both bigger than the issues that reported them. If you
assemble CP-1600 source, **this release changes your output** — see below.

### Fixed

- **The CP-1600 `byte` directive emitted the wrong bytes, and said nothing.**
  asl's `BYTE` takes a **16-bit** operand and emits its two bytes low-first, one
  byte per decle: `byte x'1234'` is the decle pair `0034 0012`, and `byte 1` is
  `0001 0000`. asm198x read the operand as 8 bits and packed one raw byte per
  item, so `byte 1,2,3` produced three bytes where asl writes six decles. Any
  CP-1600 program with a `byte` table was building a different image from the
  one asl builds. It now matches, byte for byte.

  The issue behind this suspected `byte` and `binclude` of disagreeing with each
  other. They never did — asl's listing shows one rule, one byte per decle, and
  only `byte` was misread.
  ([#235](https://github.com/asm198x/asm198x/pull/235),
  [#227](https://github.com/asm198x/asm198x/issues/227))

- **Nine 6809 instructions were missing**, and lwasm-syntax source using any of
  them was refused outright: `adca`, `adcb`, `sbca`, `sbcb`, `bita`, `bitb`,
  `cmpd`, `cmpy` and `cwai`. Add and subtract with carry, bit test, the two
  16-bit compares and wait-for-interrupt — `bita` and `cmpd` appear in ordinary
  6809 source. All four addressing modes each, 33 forms, every encoding read
  from lwasm and cross-checked against Motorola's 1981 programming manual.
  ([#232](https://github.com/asm198x/asm198x/pull/232),
  [#225](https://github.com/asm198x/asm198x/issues/225))

### Changed

- **Five CP-1600 spellings are no longer accepted: `db`, `dc.b`, `data`, `dw`
  and `dc.w`.** asl has none of them on this chip — each is an unknown
  instruction there — so source using them was never valid CP-1600 asl source,
  and asm198x accepting them meant a program that built here failed against the
  tool it claims compatibility with. If your source uses them, replace them with
  `byte` and `word`.
  ([#235](https://github.com/asm198x/asm198x/pull/235))

- **A CP-1600 `byte` statement with a string or character operand now warns.**
  asl drops the whole statement — `byte 1,"AB"` emits nothing, the `1`
  included — and reports neither an error nor a warning. The bytes are
  unchanged and still match asl exactly; asm198x now tells you the statement
  produced nothing, and that the numeric operands beside the text went with it.
  ([#237](https://github.com/asm198x/asm198x/pull/237))

## [0.0.31](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.30...asm198x-v0.0.31) - 2026-08-24

The release that closes four directive families across every reference at once —
diagnostics, assertions, comparisons and symbol visibility — and rebuilds
placement so the dialects with sections stop hand-rolling their own. Source that
uses any of these assembles now where it was refused before.

### Added

- **Comparison operators in expressions**, in all seven dialects that have them.
  Two facts here are per-dialect data, not a shared rule: **true is `$FF` in
  vasm, sjasmplus and pasmo and `1` in ca65, acme, rgbasm and lwasm** — `dc.b
  2=2` is `$FF` and `.byte 2=2` is `$01` from the same source shape — and the
  accepted spellings differ, with sjasmplus refusing `<>`, pasmo refusing both
  `==` and `<>`, and lwasm refusing `=`, `<=` and `>=`. Each was probed against
  the tool rather than read from a manual.
  ([#231](https://github.com/asm198x/asm198x/pull/231),
  [#229](https://github.com/asm198x/asm198x/issues/229),
  [#230](https://github.com/asm198x/asm198x/issues/230))

- **Assertions**: sjasmplus `ASSERT`, vasm `assert`, rgbasm `ASSERT` and
  `STATIC_ASSERT`, ca65 `.assert`. They fold after the symbols resolve, because
  an assertion **reaches forward** — `ASSERT fin-beg` above both labels is the
  point of having one. ca65's takes an action operand, so `warning` assembles
  anyway and `error` does not.
  ([#231](https://github.com/asm198x/asm198x/pull/231))

- **Directives that say something**: vasm `echo`, rgbasm `PRINT`/`PRINTLN`,
  sjasmplus `DISPLAY`, ca65 `.out` for text; acme `!error`/`!serious`/`!warn`,
  lwasm `error`, rgbasm `FAIL`/`WARN`, ca65 `.warning`/`.error`/`.fatal` for
  diagnostics. Print-style output arrives as a new `WarningKind::Note` rather
  than as a warning — it is not a complaint — and each reference's own radix is
  used, so a value reads `5` under vasm, `$5` under rgbasm and `0x0005` under
  sjasmplus.
  ([#231](https://github.com/asm198x/asm198x/pull/231))

- **Symbol visibility across every reference.** In a fused assemble-and-link
  these are checks rather than no-ops, and accepting them silently would take
  source the reference refuses: ca65 `.export`/`.exportzp` and vasm
  `xdef`/`public`/`global`/`export`/`entry`/`weak`/`extrn`/`comm` require the
  name be defined; ca65 `.import`/`.importzp` require that it is not; ca65
  `.global`/`.globalzp`/`.autoimport`, vasm `local`/`idnt` and rgbasm `EXPORT`
  ask nothing. `.export name := expr` defines the name it exports, and the `zp`
  spellings warn for a label outside the zero page but never for a constant.
  ([#231](https://github.com/asm198x/asm198x/pull/231))

- **sjasmplus's `DEVICE`, `PAGE` and `SLOT`.** Thirteen devices with their real
  page and slot bounds, and the write check that comes with them — which is on
  the 64K address space rather than on total memory, so a program that overruns
  `$FFFF` is refused even on a device with 8MB of pages. Two pages written at
  one address **concatenate** rather than colliding.
  ([#231](https://github.com/asm198x/asm198x/pull/231))

- **ca65 expression functions**: `.lobyte`, `.hibyte`, `.bankbyte`, `.loword`,
  `.hiword`, `.max`, `.min`, `.strlen`, `.strat`, and `.defined`/`.def`.
  `.defined` is answered **in source order** — 0 above the definition and 1
  below — while rgbasm's `BANK()` is answered after the whole program is read,
  because it reaches forward. Opposite treatments, and a careless test passes
  either way.
  ([#231](https://github.com/asm198x/asm198x/pull/231))

- **Alignment and padding**: ca65 `.align`, sjasmplus `ALIGN`, vasm
  `align`/`cnop`, and ACME's pad-to-a-stated-boundary, which is not the
  alignment ACME already had. vasm's operand is an **exponent** — `align 2` is a
  four-byte boundary — and its `cnop` pads with whole `NOP` words where one
  fits.
  ([#231](https://github.com/asm198x/asm198x/pull/231))

- **Segment shorthands** for the two dialects that have segments: ca65's
  `.code`/`.zeropage`/`.bss` place as their spelled-out segments, and
  `.pushseg`/`.popseg` restore the segment a reservation interrupted.
  ([#231](https://github.com/asm198x/asm198x/pull/231))

- **The directives each reference has that asm198x does not** are now declared
  rather than absent, so they are refused as real directives with a diagnostic
  saying the gap is ours — not as unknown words, which sends a reader with valid
  source looking for a typo. Ninety-seven for ca65, thirty-three for rgbasm, and
  the remaining four references swept the same way.
  ([#222](https://github.com/asm198x/asm198x/pull/222),
  [#223](https://github.com/asm198x/asm198x/pull/223),
  [#224](https://github.com/asm198x/asm198x/pull/224))

- **sjasmplus takes the optional leading dot on every directive**, so `.db` and
  `db` are one word to it and to us.
  ([#221](https://github.com/asm198x/asm198x/pull/221))

### Fixed

- **`equ` was capped at 24 bits in every dialect, and three references allow
  more.** ca65, vasm and rgbasm all take a 32-bit constant, and pasmo turns out
  to be the only reference that bounds one at all. Each dialect now states its
  own range.
  ([#228](https://github.com/asm198x/asm198x/issues/228))

- **Nine 6809 instructions were being reported as directives** by lwasm's
  declared surface. An instruction declared a directive points the reader at the
  wrong layer.
  ([#226](https://github.com/asm198x/asm198x/pull/226))

- **`xtask surface` counted a wider target as a gap**, and two concurrent runs
  corrupted each other's probe files — one run reported 386 words where the true
  figure was 538. Each run now gets its own scratch directory.
  ([#219](https://github.com/asm198x/asm198x/pull/219),
  [#231](https://github.com/asm198x/asm198x/pull/231))

### Changed

- **A word the reference itself refuses now says so.** `lwasm export` is
  `Only supported for object target`; vasm's `xref`, `import` and `nref` cannot
  be satisfied in binary output at all; ca65's `.forceimport` is unresolvable
  either way. These used to be refused with "the source is valid and the gap is
  ours", which is wrong in both halves and sends a reader off to wait for a
  feature that is never coming. They are no longer counted as outstanding work.
  ([#231](https://github.com/asm198x/asm198x/pull/231))

- **Placement is one implementation.** The NES ROM, a Game Boy bank, a sjasmplus
  page and a flat program's single section are laid out by the same code, where
  ca65 and vasm previously each hand-rolled a pass. `Warning` gains a `kind`
  field, additively.
  ([#231](https://github.com/asm198x/asm198x/pull/231))

## [0.0.30](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.29...asm198x-v0.0.30) - 2026-08-23

### Fixed

- **Nineteen `asl` directives that used to be refused as unknown mnemonics are
  now answered properly.** Five change nothing and are accepted and dropped —
  `outradix`, `message`, `warning`, `prtinit`, `shared`. Fourteen change what
  the source *means*, and are refused with a diagnostic that says the source is
  valid and the gap is ours rather than that the word does not exist:
  `relaxed`, `radix`, `phase`/`dephase`, `align`, `enum`, `charset`, `segment`,
  `save`/`restore`, `expect`/`endexpect`, `assume`, `function`.

  The distinction is the point. `relaxed on` makes `db 012` emit ten instead of
  twelve, so sweeping it into an ignore list would assemble a different program
  and report success. Each of the nineteen was probed against asl on five chips
  before being classified.
  ([#215](https://github.com/asm198x/asm198x/pull/215))

- **`supmode` is now accepted on the TMS9900**, which asl takes it on and we did
  not. ([#215](https://github.com/asm198x/asm198x/pull/215))

- **CP1610 listings speak strict asl.** `asm198x disasm` emitted Intel `0FFFFH`
  hex for the chip and opened each listing with `relaxed on` to make asl accept
  it. Strict `cpu CP-1600` takes its own `x'FFFF'` form and nothing else, so
  that is what listings use now — and the assembler reads it, wherever it comes
  from. ([#218](https://github.com/asm198x/asm198x/pull/218))

### Added

- **`cargo xtask supersede --cpu <CPU> --suite <suite>`** retires verdicts by
  scope rather than by divergence tag. Changing a generated listing strands
  every recorded fact keyed on the text it used to emit — those facts stay true
  and stop being about source the project produces — and there was no way to
  retire them. Maintainers only; the corpus stays append-only either way.
  ([#218](https://github.com/asm198x/asm198x/pull/218))

## [0.0.29](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.28...asm198x-v0.0.29) - 2026-08-23

### Fixed

- **Source that eight reference assemblers accept and this one refused now
  assembles.** Six of those refusals are closed in this release, and the
  differential suite's ledger of known gaps is **empty** for the first time —
  every snippet in it now matches its reference byte for byte.

  - **sjasmplus** takes `:` between statements, as hand-written Spectrum source
    does — `ld a,1 : ld b,2`. A label's own colon, `::`, a colon in a string or
    a comment all stay put.
    ([#210](https://github.com/asm198x/asm198x/pull/210))
  - **sjasmplus** resolves a condition against a symbol defined further down the
    file, across the same three passes the reference runs — and raises the same
    two warnings, including when the passes never settle.
    ([#211](https://github.com/asm198x/asm198x/pull/211))
  - **sjasmplus** takes the name-first `name MACRO` spelling as well as
    `MACRO name`. ([#207](https://github.com/asm198x/asm198x/pull/207))
  - **acme** takes `<`, `>` and `<>` in `!if`, and a one-character string
    wherever it wants a number — `!byte "a"`, `lda #"a"`.
    ([#207](https://github.com/asm198x/asm198x/pull/207))

- **acme sized a backward zero-page label as absolute** — `lda lbl` after
  `lbl` at $00 emitted `AD 00 00` where acme emits `A5 00`. Wrong size and
  wrong byte count, on source with nothing unusual in it. The only one of these
  that changed bytes rather than refusing to assemble.
  ([#207](https://github.com/asm198x/asm198x/pull/207))

- **vasm accepted a name defined twice**, where vasm itself refuses the
  program. That is the worst direction to differ in: an accidental collision
  got a working binary here and a build failure there.
  ([#209](https://github.com/asm198x/asm198x/pull/209))

- **Three advisories the references raise are no longer silent** — sjasmplus on
  a module left open at end of file, acme on an instruction that came out wider
  than it needed to be, and vasm answering `unknown instruction` for a mnemonic
  it knows. Matching a reference's bytes without its warnings was only half of
  matching it. ([#213](https://github.com/asm198x/asm198x/pull/213))

### Added

- **`cargo xtask surface` reports how much of each reference's own vocabulary
  this assembler takes.** It asks the tool: harvest the words in its binary,
  offer each one back to it, and whatever it does not call unknown is
  vocabulary. The existing coverage metric measures against *our* spec, so a
  form we never wrote down is invisible to it — this one cannot be, and it
  found the first thing it fixed. Needs the reference tools installed.
  ([#212](https://github.com/asm198x/asm198x/pull/212))

## [0.0.28](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.27...asm198x-v0.0.28) - 2026-08-23

### Added

- **sjasmplus source using `MODULE` now assembles.** A module prefixes the names
  defined inside it, `@` escapes to the global scope, nesting concatenates, and
  `EQU` is scoped while macros and `DEFINE`s are not — which is how a Spectrum
  project keeps two libraries from colliding. Resolution matches the reference
  exactly, including the part that is easy to get wrong: a name inside a module
  has two candidates, the fully-qualified one and the bare global one, and
  nothing in between. An inner module does not see an outer one's unqualified
  names. Twelve snippets are arbitrated byte-for-byte against SjASMPlus 1.21.0.
  ([#206](https://github.com/asm198x/asm198x/pull/206))

  One difference from the reference, which changes no bytes: it warns on a
  module left open at end of file and assembles anyway. asm198x accepts it
  silently.

  This was the last of the three items in the macro stage. Macros, repetition
  and modules are now all present in every dialect whose reference has them.

### Fixed

- **An error raised inside a macro expansion now says which macro it came
  from.** Two things were dropping the defined-at/invoked-at chain: parse-time
  errors lost it in every dialect, because the frames were only attached to an
  error that already carried a source span, and vasm lost it everywhere,
  because its errors are raised after the tree has been projected to its own
  layout form. Both now print what the other dialects already printed:

  ```text
  asm198x: v.asm:4: error: unknown instruction `FROBNICATE`
  in expansion of macro `bad` invoked at line 4
  ```

  Nested expansions list the whole chain, outermost last.
  ([#203](https://github.com/asm198x/asm198x/pull/203))

## [0.0.27](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.26...asm198x-v0.0.27) - 2026-08-23

### Added

- **Every dialect now assembles the macros, conditionals and repetition its own
  reference has.** This release finishes that: lwasm, vasm and rgbasm were the
  last three, and the table is complete.

  | dialect | macros | conditionals | repetition |
  |---|---|---|---|
  | acme | yes | yes | yes |
  | ca65 | yes | yes | yes |
  | lwasm | yes | **new** | the reference has none |
  | pasmo | yes | yes | yes |
  | rgbasm | **new** | **new** | **new** |
  | sjasmplus | yes | yes | yes |
  | vasm | yes | **new** | **new** |

  If you have been keeping a project on the real assembler because asm198x
  refused a `!if`, an `ifne`, a `REPT` or a `MACRO`, that reason is gone.
  ([#196](https://github.com/asm198x/asm198x/pull/196),
  [#198](https://github.com/asm198x/asm198x/pull/198),
  [#201](https://github.com/asm198x/asm198x/pull/201),
  [#202](https://github.com/asm198x/asm198x/pull/202))

- **Each dialect keeps its own spellings, including the ones that look like
  mistakes.** vasm takes `ifd`/`ifnd` and rejects `ifdef`; rgbasm takes `ELIF`
  and `ENDC` and rejects `ELSEIF` and `ENDIF`; lwasm takes `endc` *and* `endif`
  and compares each of `ifne`/`ifeq`/`ifgt`/`ifge`/`iflt`/`ifle` against zero.
  Every one of those was measured against the tool rather than read from a
  manual, and a spelling a reference refuses is refused here too.

- **Loop variables behave as each reference defines them**, which is four
  different things: acme's `!for` counts from 1 and survives the block, ca65's
  `.repeat` counts from 0 and stops existing at `.endrepeat`, vasm's `REPTN` is
  implicit and reads **-1** outside any `rept`, and rgbasm's `REPT` has none.

### Fixed

- **`fmt` rewrote a conditional's closing keyword.** It rendered every
  keyword-style closer as `ENDIF`, so lwasm's `endc` came back as `endif` —
  harmless there, since lwasm takes both — and rgbasm's `ENDC` came back as
  `ENDIF`, which **rgbasm does not accept**. Formatting an rgbasm file with a
  conditional in it produced source that would not assemble. The closer is now
  the word you wrote. ([#202](https://github.com/asm198x/asm198x/pull/202))

### Changed

- **rgbasm joined the differential corpus.** It had never been in it: the probe
  harness knew rgbasm's name but had no arm to assemble with, so every rgbasm
  probe was skipped and the suite stayed green while checking nothing. Wiring
  it up published the gap, then this release closed it.
  ([#200](https://github.com/asm198x/asm198x/pull/200))

## [0.0.26](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.25...asm198x-v0.0.26) - 2026-08-23

### Added

- **ca65 assembles conditionals and repetition.** `.if` / `.ifdef` / `.ifndef`
  / `.elseif` / `.else` / `.endif`, and `.repeat n[, var]` / `.endrepeat`, in
  any case. Real ca65 has had all of them since forever and asm198x answered
  `unsupported directive`, so a NES project using any of them did not build.
  The loop variable counts from **0** and stops existing at `.endrepeat`, both
  matching ca65 — and a condition inside the body can read it.
  ([#193](https://github.com/asm198x/asm198x/pull/193))
- **acme assembles `!for`.** Both spellings: `!for i, n` counts 1 to `n`, and
  `!for i, a, b` runs inclusive from `a` to `b` — **counting down** when `b` is
  below `a`, which is what acme does and is simple to get wrong.
  ([#190](https://github.com/asm198x/asm198x/pull/190))

### Fixed

- **A ca65 label may be indented.** `        start: dex` is legal ca65 and
  asm198x refused it with `unknown instruction \`START:\``, because two of the
  three ca65 front-ends required a label at column 0. In ca65 the *colon* makes
  a label and the column is irrelevant.

  It also made `fmt` unsafe, not just strict, which is how it was found:
  indenting a macro body is correct ca65 layout, so the formatter moved a
  body's label off column 0 and produced a file the same parser then refused.
  If you have formatted ca65 source containing a macro whose body defines a
  label, it is worth rebuilding.
  ([#188](https://github.com/asm198x/asm198x/pull/188))

### Changed

- **A dialect now gets a construct when its reference has one.** Conditional
  assembly used to be adopted per dialect only when something concrete demanded
  it. Measuring the gaps ended that: of the ten missing across macros,
  conditionals and repetition, **nine were features the reference already had
  and asm198x had never implemented** — so the rule was holding back
  compatibility work, not speculation. What stays gated is the reverse: giving
  a dialect something its reference does **not** have, which would accept
  source the real tool rejects.
  ([#190](https://github.com/asm198x/asm198x/pull/190))

## [0.0.25](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.24...asm198x-v0.0.25) - 2026-08-22

### Fixed

- **`fmt` deleted the body of every repetition block.** On sjasmplus source, a
  `DUP` or `REPT` block came back as its head alone — the body and the closer
  were dropped, silently, exit 0. Three lines in, one out. The guide describes
  `fmt` as safe to run over a project you have not read and shows you how to
  move the result over your original, so this is worth checking for if you have
  formatted sjasmplus source with a loop in it. Now rendered in full, with the
  closer kept exactly as you spelled it — `EDUP` stays `EDUP`, `endr` stays
  lowercase. ([#187](https://github.com/asm198x/asm198x/pull/187))
- **A diagnostic no longer names a directive your assembler does not have.**
  pasmo's "must be a constant here" suggested `DEFINE`, which is sjasmplus's;
  an unclosed pasmo repetition was reported as a missing `EDUP`, which is also
  sjasmplus's. Each message now comes from the dialect you are assembling.
  ([#187](https://github.com/asm198x/asm198x/pull/187))
- **`include` in pasmo said your source was wrong when it was not.** Real pasmo
  assembles it and we do not implement it, and refusing it as an unknown
  mnemonic sent readers to check their own file. It now says the spelling is
  recognised and unimplemented, which is a different thing and the one you can
  act on. The same pass gave every dialect one wording for a word it does not
  know. ([#181](https://github.com/asm198x/asm198x/pull/181))

### Added

- **Every dialect formats a file with macros in it.** ca65, vasm, lwasm, pasmo
  and acme would each assemble a file and then refuse to lay it out — 35 source
  files in the test corpus were in that state. A macro definition is now copied
  through exactly as written, its own indentation kept, because a body is a
  template rather than code: a parameter is not an operand, and a line of one
  may not be a whole instruction until the macro is called.
  ([#184](https://github.com/asm198x/asm198x/pull/184),
  [#183](https://github.com/asm198x/asm198x/pull/183))
- **pasmo: conditional assembly and repetition.** `IF` / `ELSE` / `ENDIF` and
  `REPT n` … `ENDM`, in any case, with the count folding against the constants
  above it. Both were in real pasmo and answered `unknown instruction` here.
  Only the spellings pasmo actually has: `IFDEF`, `IFNDEF`, `ELSEIF`, `ENDC`,
  the dotted forms, `DUP` and `ENDR` are all sjasmplus's, and all still
  refused. ([#187](https://github.com/asm198x/asm198x/pull/187))

### Changed

- The formatting guide now says what happens to a macro — the definition comes
  back with your indentation rather than the canonical one — and pasmo's dialect
  page lists its new `if`/`else`/`endif` and `rept` spellings. That page is the
  list the parser reads, so a spelling on it is one the assembler accepts and a
  spelling missing from it is one it refuses.
  ([#187](https://github.com/asm198x/asm198x/pull/187),
  [#184](https://github.com/asm198x/asm198x/pull/184))

## [0.0.24](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.23...asm198x-v0.0.24) - 2026-08-22

### Added

- **The documentation is searchable again.** mdBook carried search and its
  withdrawal left none. The index is generated from the pages themselves, so
  every mnemonic on all twenty-one CPU pages is findable with the line that
  describes it — searching `SBC` reaches the 6502, HuC6280, 65C816, SM83 and
  Z80 sections directly. Headings rather than full text, which is also what
  answers a prose question: "Where a relative include is looked for" beats
  every page that happens to use the word.
  ([#179](https://github.com/asm198x/asm198x/pull/179))
- Every dialect's macro and conditional vocabulary is declared, so a generated
  matrix shows which dialects have macros, repetition and conditional assembly
  rather than only the directives that reach the operation parser. Probing for
  it corrected a long-standing assumption: `equ` is part of the label grammar,
  not a directive. ([#178](https://github.com/asm198x/asm198x/pull/178))

### Changed

- `sha2` 0.10 → 0.11. Test and tooling only; the shipped binary does not link
  it and its output is unchanged.
  ([#151](https://github.com/asm198x/asm198x/pull/151))

## [0.0.23](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.22...asm198x-v0.0.23) - 2026-08-21

### Added

- Four guide pages, covering the things the options table names and nothing
  explained: **projects in more than one file**, **when assembling is not the
  last step**, **keeping source tidy** and **reading a binary back**.
  ([#168](https://github.com/asm198x/asm198x/pull/168),
  [#170](https://github.com/asm198x/asm198x/pull/170),
  [#172](https://github.com/asm198x/asm198x/pull/172))
- A relative `include` resolves against a different directory depending on the
  dialect — the including file's own, that plus each enclosing includer's, or
  the root input's — each pinned against its own reference assembler. That was
  true before and stated nowhere. The table is generated from
  `asm198x::includes::resolution()`, a new public accessor that most dialects
  answer straight off the semantics their multi-file walk runs on.
  ([#168](https://github.com/asm198x/asm198x/pull/168))
- Every listing, diagnostic and formatted example on the new pages is compared
  against what the binary prints, so a page cannot describe output the tool no
  longer produces. ([#172](https://github.com/asm198x/asm198x/pull/172))

### Fixed

- *Why asm198x* understated two capabilities by most of their coverage. It said
  `fmt` handled seven CPU families and not the 6502, and that `disasm` read
  6502 and Z80. Both cover **every dialect**. A test now runs each operation for
  every name `--dialect` accepts, so the claim fails the build if it stops being
  true. ([#171](https://github.com/asm198x/asm198x/pull/171))

## [0.0.22](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.21...asm198x-v0.0.22) - 2026-08-21

### Added

- Every dialect's directive vocabulary is now **declared data the parser
  dispatches from**, so a spelling the declaration does not carry cannot be
  accepted. Twenty-one dialects, with a test proving each declared spelling is
  recognised and each undeclared one refused.
  ([#162](https://github.com/asm198x/asm198x/pull/162),
  [#163](https://github.com/asm198x/asm198x/pull/163),
  [#164](https://github.com/asm198x/asm198x/pull/164),
  [#165](https://github.com/asm198x/asm198x/pull/165))
- The multi-file table on *Moving a project across* is generated from those
  declarations. It covers every dialect rather than five, and shows pasmo's
  missing `include` as the one gap.
  ([#165](https://github.com/asm198x/asm198x/pull/165))
- `fmt` and `disasm` have worked examples on the command-line page — both were
  described in prose and never shown. Formatting turns out to move a label onto
  its own line. ([#167](https://github.com/asm198x/asm198x/pull/167))

### Fixed

- The `--message-format=json` example on the command-line page reported column
  13 where the assembler reports 15, and had no source sample it was the output
  of. Every output block in the book — diagnostics, JSON, formatted listings and
  disassembly — is now compared against what the binary prints.
  ([#167](https://github.com/asm198x/asm198x/pull/167))
- The evidence figures on *Why asm198x* had drifted: 5,637 recorded verdicts
  against a corpus holding 5,625 live ones, and "nine differences" where nine is
  the number of recorded cases across six tracked differences. They are counted
  from the corpus now. ([#166](https://github.com/asm198x/asm198x/pull/166))

### Changed

- The introduction is orientation rather than a second pitch, now that *Why
  asm198x* carries the argument.
  ([#166](https://github.com/asm198x/asm198x/pull/166))

## [0.0.21](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.20...asm198x-v0.0.21) - 2026-08-21

### Added

- `Where we differ` — every known difference from the reference assemblers,
  generated from the verdict corpus. Six tracked differences across nine
  recorded cases. ([#158](https://github.com/asm198x/asm198x/pull/158))
- `Why asm198x` — what adopting it costs and what arrives with it, with each
  capability's scope stated. ([#158](https://github.com/asm198x/asm198x/pull/158))

## [0.0.20](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.19...asm198x-v0.0.20) - 2026-08-21

### Added

- A quickstart and an install page. The quickstart assembles one program for
  the C64, the Spectrum and the Amiga, showing that only `--dialect` and the
  output flag change between them. All three are assembled by CI with the real
  binary. ([#154](https://github.com/asm198x/asm198x/pull/154))

### Changed

- `The command line` keeps a one-line Homebrew command and points at the new
  install page for platforms, archives and the crates.io note.

## [0.0.19](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.18...asm198x-v0.0.19) - 2026-08-21

### Other

- *(docs)* lay the pages out at the URLs they are published at ([#153](https://github.com/asm198x/asm198x/pull/153))

## [0.0.18](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.17...asm198x-v0.0.18) - 2026-08-21

### Changed

- The documentation is published as part of <https://asm198x.github.io> rather
  than as a separate book behind `/docs/`. mdBook is withdrawn; the pages
  themselves are unchanged and still live in this repository, beside the code
  they describe. ([#148](https://github.com/asm198x/asm198x/pull/148))

## [0.0.17](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.16...asm198x-v0.0.17) - 2026-08-21

### Fixed

- *(isa)* list the C64 on the 6502 page, and derive the parity figures ([#146](https://github.com/asm198x/asm198x/pull/146))

## [0.0.16](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.15...asm198x-v0.0.16) - 2026-08-20

### Fixed

- *(vasm)* order hunk relocations the way vasm does, and check four times as much curriculum ([#144](https://github.com/asm198x/asm198x/pull/144))

## [0.0.15](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.14...asm198x-v0.0.15) - 2026-08-20

### Added

- *(cli)* prefix the reported version with v

## [0.0.14](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.13...asm198x-v0.0.14) - 2026-08-20

### Added

- *(docs)* assemble every book sample with the real binary (#61 R2) ([#134](https://github.com/asm198x/asm198x/pull/134))

## [0.0.13](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.12...asm198x-v0.0.13) - 2026-08-19

### Added

- *(cli)* generate the dialect list from one table (#93 follow-on) ([#131](https://github.com/asm198x/asm198x/pull/131))
- *(acme)* assemble brace-delimited macros with arity overloads ([#93](https://github.com/asm198x/asm198x/pull/93)) ([#129](https://github.com/asm198x/asm198x/pull/129))
- *(lwasm,vasm)* assemble positional-parameter macros ([#93](https://github.com/asm198x/asm198x/pull/93)) ([#127](https://github.com/asm198x/asm198x/pull/127))
- *(ca65)* assemble .macro/.endmacro with .local-scoped labels ([#93](https://github.com/asm198x/asm198x/pull/93)) ([#125](https://github.com/asm198x/asm198x/pull/125))
- *(pasmo)* assemble MACRO/ENDM with LOCAL-scoped labels ([#93](https://github.com/asm198x/asm198x/pull/93)) ([#123](https://github.com/asm198x/asm198x/pull/123))
- *(sjasmplus)* say which macro a failing line came out of ([#122](https://github.com/asm198x/asm198x/pull/122))
- *(sjasmplus)* repeat a block with DUP/EDUP and REPT/ENDR ([#121](https://github.com/asm198x/asm198x/pull/121))
- *(sjasmplus)* let macros invoke macros, and refuse to crash on recursion ([#120](https://github.com/asm198x/asm198x/pull/120))
- *(sjasmplus)* scope a macro's local labels to its expansion ([#119](https://github.com/asm198x/asm198x/pull/119))
- *(sjasmplus)* assemble macros, matching the reference byte for byte ([#118](https://github.com/asm198x/asm198x/pull/118))
- *(xtask)* measure how much of the spec the corpus actually arbitrates ([#114](https://github.com/asm198x/asm198x/pull/114))
- *(verdict-corpus)* cover the 68000 sweep by tagging what we know diverges ([#112](https://github.com/asm198x/asm198x/pull/112))
- *(verdict-corpus)* chunk the opcode sweep by mnemonic, and record it ([#111](https://github.com/asm198x/asm198x/pull/111))
- *(verdict-corpus)* record the curriculum, and give CI the source to check it ([#109](https://github.com/asm198x/asm198x/pull/109))
- *(verdict-corpus)* record both fuzzers, and name what they scope out ([#108](https://github.com/asm198x/asm198x/pull/108))
- *(verdict-corpus)* record the differential probes, gaps included ([#107](https://github.com/asm198x/asm198x/pull/107))
- *(verdict-corpus)* record the form audit, and replay it without the tools ([#106](https://github.com/asm198x/asm198x/pull/106))
- *(verdict-corpus)* record what the reference assemblers actually did ([#103](https://github.com/asm198x/asm198x/pull/103))
- *(cli)* answer --version, -V, and `version` ([#97](https://github.com/asm198x/asm198x/pull/97))

### Fixed

- *(macros)* let a label sit in front of a macro invocation ([#93](https://github.com/asm198x/asm198x/pull/93)) ([#124](https://github.com/asm198x/asm198x/pull/124))
- *(asl)* let a leading gap move the load address, not pad the image ([#102](https://github.com/asm198x/asm198x/pull/102))

### Other

- *(differential)* record what the references do with macros ([#117](https://github.com/asm198x/asm198x/pull/117))
- *(support)* capture which reference tool arbitrated, and which build ([#105](https://github.com/asm198x/asm198x/pull/105))
- *(conformance)* tell a reference's refusal from its absence ([#104](https://github.com/asm198x/asm198x/pull/104))
- *(differential)* repoint the sjasmplus gaps at their real issues ([#100](https://github.com/asm198x/asm198x/pull/100))

## [0.0.12](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.11...asm198x-v0.0.12) - 2026-08-19

### Added

- *(dist)* ship shell and PowerShell installers
- *(dist)* publish a Homebrew formula to asm198x/homebrew-tap
- *(cli)* [**breaking**] make the operation a subcommand

### Fixed

- *(cli)* say where `fmt` sent its output

### Other

- Merge pull request #91 from asm198x/link-leading-gap-issue
- *(dialect)* point the leading-gap note at its issue

## [0.0.11](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.10...asm198x-v0.0.11) - 2026-08-18

### Fixed

- *(asl)* align the ignore lists with what asl actually accepts

## [0.0.10](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.9...asm198x-v0.0.10) - 2026-08-18

### Added

- *(sjasmplus)* adopt ELSEIF chains and the dotted conditional spellings
- *(debug198x)* let a section state the page it lives in

### Fixed

- *(asl)* reserve space the way asl and p2bin do, not as zeros
- *(debug198x)* [**breaking**] carry unknown space shapes, and withdraw `bank`
- *(debug198x)* document the page as the join key, and prove it

### Other

- *(debug198x)* record leg 3's verdict against the fixture's claims
- *(debug198x)* state the banked fixture's claims for the leg-3 cross-check
- Merge pull request #72 from asm198x/debug198x-section-space

## [0.0.9](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.8...asm198x-v0.0.9) - 2026-07-10

### Added

- *(debug)* multi-file debug artifacts - per-file line records, spliced listings, and the fixture families
- *(conditionals)* sjasmplus adopts the shared CondEval walk - IF/IFDEF/IFNDEF/ELSE/ENDIF plus textual DEFINE
- *(locals)* ACME !zone scoping becomes real; the z80/rgbasm qualifiers consolidate onto the shared AST helper
- *(vasm)* include and incbin through the 68000 multipass path, byte-identical to vasm
- *(ca65-nes)* .include and .incbin through the assemble+link path, ROM-identical to ca65+ld65
- *(asl)* INCLUDE and BINCLUDE across all twelve asl-syntax chips, byte-identical to asl
- *(rgbasm,lwasm)* include and incbin for the Game Boy and 6809 dialects, byte-identical to their references
- *(ca65-flat)* .include and .incbin for the 65816 and HuC6280 dialects, byte-identical to ca65
- *(acme)* !src and !bin land through the evaluation walk, byte-identical to acme
- *(incbin)* binary inclusion lands for the z80 family, byte-identical to sjasmplus and pasmo
- *(include)* INCLUDE lands end-to-end, proven byte-identical on sjasmplus
- *(source)* the multi-file foundation - loader seam, FileId table, file:line:col rendering

### Fixed

- *(vasm)* support `!` as bitwise-OR and relocate `dc.l <label>` data
- *(review)* apply the nine validated findings from the multi-agent + cross-model review

### Other

- collapse collapsible_if in vasm dc.l reloc path
- Merge pull request #68 from asm198x/feat/language-surface
- *(source)* resolve includes before reading - a shared header is read once, not once per inclusion

## [0.0.8](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.7...asm198x-v0.0.8) - 2026-07-06

### Added

- *(debug)* emit the debug record from the vasm hunk path (U5)
- *(debug)* emit the debug record from the ca65 NES path (U4)
- *(debug)* emit the .debug198x sidecar, --sym, and --listing (U3)
- *(contract)* point diagnostics at the operand column (U3)
- version the core contract + record its freeze governance (U5)
- add --message-format=json to the CLI (core-contract U4)
- route NES ca65 (assemble+link) through the semantic AST
- route vasm (68000) assembly through the semantic AST
- give vasm (68000) an AST front-end for the --fmt formatter
- route Z8000 assembly through the semantic AST
- route CP1610 assembly through the semantic AST
- route TMS9900 assembly through the semantic AST
- route PDP-11 assembly through the semantic AST
- route ca65 HuC6280 assembly through the semantic AST
- route ca65 65816 assembly through the semantic AST
- *(tms7000)* route the TMS7000 dialect through the AST (0b straggler)
- *(2650)* route the Signetics 2650 dialect through the AST (0b straggler)
- *(f8)* route the Fairchild F8 dialect through the AST (0b straggler)
- *(8048)* route the MCS-48 dialect through the AST (0b straggler)
- *(contract)* rustc-shaped diagnostics on one shared span (U2)
- *(contract)* unify assembly output into one AssemblyResult (U1)
- *(ast)* idea 4 — ACME assembles by evaluating the conditional AST
- *(ast)* ACME/6502 formatter — canonical reflow with conditional blocks
- *(ast)* promote the conditional-block representation into the shared AST (idea 4)
- *(ast)* U6 — migrate the 6809 onto the AST (first computed-operand CPU)
- *(ast)* U6 — migrate rgbasm (Game Boy SM83) onto the AST
- *(ast)* U6 — migrate the National SC/MP onto the AST (fixed-slot)
- *(ast)* U6 — migrate the RCA CDP1802 onto the AST (fixed-slot)
- *(ast)* U6 — migrate the Motorola 6800 onto the AST (fixed-slot)
- *(ast)* U6 — migrate the Intel 8080 onto the AST (first fixed-slot CPU)
- *(ast)* U6 foundation — total lowering, retire the U1 spike
- *(ast)* U5 — asm198x fmt, the AST emit proof (AE7)
- *(ast)* U4 — carry Z80 comments as AST trivia
- *(ast)* U3 — route the Z80 front-end through the semantic AST
- *(ast)* U2 — the source-preserving semantic AST types
- *(dbg198x)* capture debug info in the engine (U2)

### Fixed

- *(review)* fmt round-trip bugs + restore the clippy gate

### Other

- *(debug)* the format decision record + CP1610 fixture (U7 — plan complete)
- *(debug)* the conformance fixture corpus (U6)
- rename the dbg198x crate to debug198x
- *(ast)* extract the shared conditional evaluator (CondEval)
- *(ast)* drop unused import in the U1 spike
- *(ast)* U1 validation spike — the neutral-AST gate

## [0.0.7](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.6...asm198x-v0.0.7) - 2026-07-03

### Added

- *(cp1610)* add SDBD double-byte immediate — completes the CPU (increment 6) ([#58](https://github.com/asm198x/asm198x/pull/58))
- *(cp1610)* add JUMP/JSR and word-addressing (increment 5) ([#57](https://github.com/asm198x/asm198x/pull/57))
- *(cp1610)* add memory / immediate addressing modes (increment 4) ([#56](https://github.com/asm198x/asm198x/pull/56))
- *(cp1610)* add relative branch group (increment 3) ([#54](https://github.com/asm198x/asm198x/pull/54))
- *(cp1610)* add shift / rotate group (increment 2) ([#53](https://github.com/asm198x/asm198x/pull/53))
- *(cp1610)* add GI CP1610 register/implied groups (increment 1) ([#52](https://github.com/asm198x/asm198x/pull/52))
- *(z8000)* add segmented Z8001 target (increment 12) ([#51](https://github.com/asm198x/asm198x/pull/51))
- *(z8000)* cleanup — TCC/LDK/RLDB/RRDB/LDR (complete Z8002 ISA)
- *(z8000)* increment 11 — CPU control / status group
- *(z8000)* increment 10 — privileged I/O group
- *(z8000)* increment 9 — block/string repeat group
- *(z8000)* increment 8 — multiply/divide (MULT/MULTL/DIV/DIVL)
- *(z8000)* increment 7 — bit ops (BIT/SET/RES, static and dynamic)
- *(z8000)* increment 6 — shifts/rotates/sign-extends
- *(z8000)* increment 5 — stack ops (PUSH/POP/PUSHL/POPL)
- *(z8000)* increment 4 — single-operand ALU (CLR/COM/NEG/TEST/TSET/INC/DEC)
- *(z8000)* increment 3 — program control (JP/CALL/JR/RET/DJNZ/CALR)
- *(z8000)* increment 2 — long ops, exchange, load address
- *(z8000)* increment 1 — the dyadic arithmetic/logic/load family
- add TI TMS9900 — Wave C, the TI-99/4A CPU
- add DEC PDP-11 — Wave B, the family's first 16-bit CPU

## [0.0.6](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.5...asm198x-v0.0.6) - 2026-07-02

### Added

- add TI TMS7000 — Wave B, the family's largest single CPU
- *(2650)* range-check relative/page-zero/absolute operands like asl
- add Signetics 2650 — Wave B, four addressing modes via the seam
- add ROM-less MCS-48 kin (8035/8039/8040) as an 8048 variant
- add Fairchild F8 (3850) — Wave B, offset-byte-relative branches
- add National SC/MP (INS8060) — Wave B, pointer+displacement addressing
- add Intel 8048 (MCS-48) — first Wave-B CPU, three tools one chip
- add RCA CDP1802 (COSMAC) — ninth CPU, zero engine changes
- add the Motorola 6800 (roadmap Wave A)
- add the Intel 8080 (Wave A of the CPU-coverage roadmap)
- *(asm)* accept rgbasm `@` current-PC symbol (#8 follow-up)
- *(asm)* rgbasm (Game Boy SM83) assemble dialect ([#8](https://github.com/asm198x/asm198x/pull/8))
- *(isa,disasm)* add the SM83 (Game Boy) spec + disassembler ([#8](https://github.com/asm198x/asm198x/pull/8))
- *(asm)* HuC6280 assembler + disassembler dialect (#9 phase 3)
- *(z80)* truncate out-of-range byte immediates; warning channel in the engine
- *(vasm)* warn (not error) on out-of-range immediates, matching vasm
- *(acme)* add the !set reassignable variable
- *(acme)* add the !align directive
- *(c64)* emit .prg output (--prg)
- *(spectrum)* emit 48K .sna snapshots (--sna)
- *(ca65-816)* add .dword/.dbyt/.asciiz; mark #26 differential gaps closed
- *(asm)* non-fatal warning channel; warn on out-of-range CCR/SR immediate
- *(ca65)* support anonymous labels (: / :- / :+)
- *(acme)* accept the !pet and !zone directives
- *(sjasmplus)* accept the byte directive (a db alias)
- *(lwasm)* add fill, zmb, and fqb directives
- *(ca65)* add .dword, .dbyt, and .asciiz directives
- *(isa)* add 6809 andcc/orcc/cmpu/cmps/swi2/swi3 and 65816 rtl

### Fixed

- *(ci)* clear the clippy errors breaking the Clippy job
- *(ca65)* clearer error for a segment outside the NES config
- *(acme)* correct operator precedence; add ^ power and XOR/EOR keyword
- *(acme)* require an explicit origin before code or data
- *(z80)* fold a constant-expression ds/defs count
- *(vasm)* assemble eor/and/or with an immediate operand
- *(vasm)* parse absolute-address size suffixes .w/.l
- *(z80)* accept radix number formats (0x, h/b/o/q suffix, # prefix)
- *(z80n)* accept mul operands (mul d,e / mul de)
- *(vasm)* accept adda/suba/cmpa mnemonics
- *(vasm)* parse new-style parenthesised 68k effective addresses
- *(dialects)* parse bitwise & shift operators in expressions
- *(z80n)* encode PUSH nn immediate big-endian

### Other

- *(differential)* note the ledger is gap-free, silence dead `gap`
- *(differential)* cover the Z80N extension ISA vs sjasmplus
- add source-direction differential audit (reference accepts, we reject)

## [0.0.5](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.4...asm198x-v0.0.5) - 2026-06-04

### Added

- *(68000)* add MOVEP — base-68000 ISA now complete
- *(68000)* add CCR/SR/USP moves and immediate-to-CCR/SR
- *(68000)* add TRAP, MOVEA, and EXG
- *(68000)* add ADDX/SUBX/ABCD/SBCD/CMPM (extended + BCD arithmetic)

### Other

- apply cargo fmt
- *(conformance)* extend the differential fuzzer to 6809 and 68000

## [0.0.4](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.3...asm198x-v0.0.4) - 2026-06-03

### Added

- *(65816)* spec-driven disassembler with m/x width tracking
- *(65816)* block moves, cop/wdm, bank-byte operator, 24-bit symbols
- *(65816)* native-mode core as a ca65 target extension of the 6502
- *(6809)* indexed addressing, register ops, fcc, and the disassembler
- *(6809)* add lwasm 6809 assembler over a computed-operand engine seam
- *(68000)* spec-driven disassembler
- *(68000)* bitwise/shift operators, indexed addressing, label(pc) — Stage 3 complete
- *(68000)* Amiga hunk-executable output (Stage 3, single-section)
- *(68000)* rewrite cmp #0,<ea> to tst <ea>
- *(68000)* drop zero d16(An) displacement to (An)
- *(68000)* convert add/sub #d16,An to lea for word size too
- *(68000)* ADDI/SUBI/CMPI and the add#d16,An->lea optimization
- *(68000)* Stage 2 optimizer — PC-relative, branch relaxation, addq/subq
- *(68000)* local-label scoping, ADDA/SUBA/CMPA, deferred ds/dcb counts
- *(68000)* shifts, bit ops, movem, and .s short branches
- *(68000)* add the regular instruction families
- *(68000)* field-based encoder foundation (vasm mot syntax)
- *(6502)* honor ACME hex-width sizing; full-binary disasm round-trip
- *(6502)* add spec-driven 6502 disassembler
- *(6502)* add ca65 dialect + bounded NES linker
- *(6502)* ACME text directives + constant-folded !fill
- *(6502)* ACME conditional assembly + value-based zero-page selection
- *(6502)* support ACME anonymous -/+ labels
- *(6502)* add ACME dialect front-end (foundation)
- *(z80)* add location counter and sjasmplus local-label scoping
- *(asm198x)* add the sjasmplus dialect over a shared Z80 syntax core
- add Z80N (Spectrum Next) opcodes, gated by target not dialect
- *(asm198x)* add a spec-driven Z80 disassembler
- complete the Z80 with the DD/FD (IX/IY) prefix group
- *(asm198x)* add vanilla pasmo as a first-class Z80 dialect
- *(asm198x)* resolve BIT/SET/RES bit numbers and defb strings in pasmonext
- *(asm198x)* expression arithmetic and IM operand resolution
- *(asm198x)* add the pasmo Z80 dialect front-end

### Fixed

- *(68000)* render PC-relative EA as a resolved target (closes the last gap)
- *(68000)* harden the disassembler/spec, enabling the conformance sweep
- *(68000)* correct branch relaxation fixpoint; complete Stage 2 flat-binary parity
- *(68000)* relax bare branches to short, not just explicit .s
- *(asm198x)* emit operands by their declared width

### Other

- rustfmt the workspace (unblocks the CI fmt check)
- *(conformance)* sweep-based audit for the non-form specs (6809)
- *(conformance)* spec-opcode audit + differential fuzzer vs the real tools
- collapse the four Expr evaluators into one shared core
- *(68000)* wire vasm byte-identity into the curriculum harness
- extract the disassembler into the dependency-free isa-disasm crate
- *(68000)* div_ceil and a Reloc type alias (clippy)
- rewrite README + correct disasm round-trip note; fix example
- format the workspace with rustfmt (1.95.0 toolchain)
- *(asm198x)* add opt-in curriculum byte-identity harness
- *(asm198x)* document flat-vs-linked split; refresh stale crate docs
- *(6502)* dedup shared acme/ca65 lexical helpers into the core
- *(6502)* extract shared dialects::mos6502 core
- *(6502)* retire generic placeholder, route 6502 to ACME
- *(asm198x)* name the Z80 dialect PasmoNext
- *(asm198x)* split engine, dialect, and spec into a three-way seam

## [0.0.3](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.2...asm198x-v0.0.3) - 2026-06-02

### Fixed

- Give the `isa` path dependency an explicit version requirement so
  `cargo package` succeeds. release-plz runs `cargo package` to compute the
  release diff; a path dependency without a version requirement fails it, which
  blocked release automation. Local builds still resolve `isa` via the path.

### Other

- Enable `git_only` so release-plz reads the previous version from the git tag
  rather than the (unused, `publish = false`) crates.io registry.

## [0.0.2](https://github.com/asm198x/asm198x/compare/asm198x-v0.0.1...asm198x-v0.0.2) - 2026-06-01

### Added

- Two-pass 6502 assembler — a library plus the `asm198x` CLI — built on the
  `isa` instruction-set spec. This first slice covers the common addressing
  modes, labels, the `<`/`>` byte-select operators, and the `.org` / `.byte` /
  `.word` directives. The 6502 dialect is an early subset; ca65 compatibility
  (arithmetic expressions, the full directive set, segments, macros) is still to
  come — see `decisions/syntax-stance.md`.
