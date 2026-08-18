# Paging cross-check — the banked fixture's third validation leg

Plan R10's third leg, and the last Emu198x item on the Debug198x v1 freeze
checklist (`decisions/debug198x-format.md` § *The freeze checklist*, item 3):
**cross-check this fixture's slot/page expectations against Emu198x's actual
Spectrum 128 paging model.** It is cross-repo by design — the fixture is written
here, but only a machine model can say whether it describes a real machine.

> **✅ Passed 2026-08-18** — emu198x/emu198x#986, `454baf55`. Six tests in
> `crates/runtime-sinclair-zx-spectrum/tests/debug198x_paging_cross_check.rs`,
> driving a real `Memory128K` into each paging state. No ROMs, no firmware, no
> skip path, so it always runs; nothing is asserted from the spec — every
> address the sidecar claims is checked by reading the byte back through the
> same `MemoryBus` the CPU uses.
>
> **Every hardware claim below holds.** The cross-check also found something the
> fixture could not: see *What leg 3 changed* at the end.

This document exists because of how the first cross-repo contact went wrong.
The Emu198x importer read the reader's consumer model out of type signatures
instead of finding it stated, concluded the lookups were broken, and filed
[#71](https://github.com/asm198x/asm198x/issues/71) against behaviour that was
correct (see the 2026-08-18 note in `decisions/debug198x-format.md`). The lesson
was that an unstated contract gets inferred, and inference is where the error
enters. So this leg states every claim the fixture makes, individually, with what
would falsify it — rather than handing over a fixture and asking "does this look
right?"

## Who arbitrates what

| Class | Claim about | Arbiter |
|-------|-------------|---------|
| **H** | Spectrum 128 hardware | **Emu198x's paging model** — the point of this leg |
| **T** | sjasmplus's SLD convention | The tool; recorded in [`spectrum128-banked-sld.md`](spectrum128-banked-sld.md), not Emu198x's to settle |
| **F** | How this fixture is constructed | This repo — stated so a checker does not mistake a fixture choice for a hardware assertion |

Only the **H** claims needed a verdict. **T** and **F** are listed so they were
not silently swept into the cross-check.

## What the fixture is

```
{"t":"section","id":0,"name":"bank1","space":{"slot":3,"page":1}}
{"t":"section","id":1,"name":"bank3","space":{"slot":3,"page":3}}
{"t":"symbol","name":"draw","kind":"label","section":0,"offset":16,"space":{"slot":3,"page":1}}
{"t":"symbol","name":"music","kind":"label","section":1,"offset":16,"space":{"slot":3,"page":3}}
{"t":"line","file":"spectrum128-banked.s","line":5,"section":0,"offset":16,"length":2}
{"t":"line","file":"spectrum128-banked.s","line":12,"section":1,"offset":16,"length":2}
```

Neither section carries a `base`: where its code lands depends on what is paged
in, so the consumer supplies it. `tests/debug198x_fixtures.rs` resolves both
sections at `0x4000 * slot`, i.e. `$C000` for slot 3.

## The claims

### H1 — slot 3 is `$C000–$FFFF`, and it is the pageable window

The fixture's arithmetic is `slot_addr = 0x4000 * slot`, so slot 3 is `$C000`,
covering `$C000–$FFFF`. It further assumes this is the window RAM banks page
*into*.

Our source layer agrees — `syntheses/zx-spectrum/128k-extras.md` § *The memory
map*: `$C000–$FFFF: RAM bank 0–7 (paged via $7FFD bits 0–2)`, with `$4000` (bank
5) and `$8000` (bank 2) fixed.

**Falsified if** Emu198x's model maps the switchable window elsewhere, or indexes
slots such that the switchable one is not 3.

### H2 — pages 1 and 3 are both legally pageable into slot 3

`syntheses/zx-spectrum/128k-extras.md`: *"The remaining six banks (0, 1, 3, 4, 6,
7) can be paged into `$C000–$FFFF` one at a time."* Both 1 and 3 are in that set.

**Falsified if** Emu198x's model refuses either bank in that window, or treats
bank 1 or 3 as fixed.

### H3 — one slot holds one page at a time

The whole design rests on this. It is why the base map can carry paging state,
why mapping two pages of one slot at once is meaningless, and why `symbol_at`
needs no page argument. `128k-extras.md` says the six banks page in *"one at a
time"*.

**Falsified if** Emu198x's model can present two banks in `$C000–$FFFF`
simultaneously — which would make the format's model wrong, not just the fixture.
This is the highest-consequence claim here.

### H4 — two symbols in different pages can share a CPU address

`draw` (page 1) and `music` (page 3) both resolve to `$C010`. This is the
condition `Space::Paged` exists for.

**Falsified if** Emu198x's model makes this unreachable in practice.

### H5 — `Section.space` can express what Emu198x's paging model does

The reverse direction, and the one that could still change the format while it is
draft. `Section.space` carries exactly `{slot, page}`. The question is whether a
debugger holding real 128K paging state can derive its base map from it, per
`crates/debug198x/src/lib.rs` (`BaseMap` docs):

```rust
let bases: BaseMap = info.sections.iter()
    .filter(|s| s.space == Some(Space::Paged { slot, page }))
    .map(|s| (s.id, slot_addr))
    .collect();
```

**Falsified if** the model holds paging state that `{slot, page}` cannot round-trip
— see the open questions below, which are the known candidates.

### T1 — the SLD projection (not Emu198x's to arbitrate)

`page * 0x4000 + offset_within_page`, checked in
[`spectrum128-banked-sld.md`](spectrum128-banked-sld.md). A sjasmplus convention.
Listed only so it is not mistaken for a hardware claim.

### F1–F3 — fixture construction (this repo's, not findings)

- **F1.** Pages are section-aligned, so `offset` within a section equals offset
  within its page. A convenience; the format does not require it.
- **F2.** Pages **1 and 3 are both contended banks** (`128k-extras.md`: odd banks
  1/3/5/7 share RAM chips with the ULA fetch). Real code paging into `$C000` for
  cycle-tight work uses uncontended bank 0 or 4. The fixture needs only *two
  distinct legally-pageable banks* and contention is irrelevant to a shape
  fixture — so this is **not** a modelling error to report. If it reads as one,
  say so and the fixture can move to 0 and 4.
- **F3.** The fixture is hand-authored (`"tool":"hand-authored"`). No emission
  path populates `space` yet, per AE3's no-fabrication rule.

## Open questions for the arbiter

These are genuinely undecided here, and a "no" on any is a format finding while
Debug198x is still draft — which is cheaper now than after the freeze.

1. **Are slots 0–2 ever expressible?** On a 128K only slot 3 switches; `$0000` is
   ROM and `$4000`/`$8000` are fixed banks 5 and 2. Should a section in bank 5 at
   `$4000` carry `space: {slot: 1, page: 5}`, or no `space` and a plain `base`?
   The format permits either; the fixture shows neither. **What does
   Emu198x need to symbolize an address in a fixed bank?**
2. **Does the ROM slot need expressing at all?** `{slot: 0, page: <rom>}` is
   representable. Useful, or noise?
3. **+2A/+3 special paging.** `$1FFD` gives all-RAM configurations where the
   fixed slots are not fixed. Does `{slot, page}` still describe those, or does
   the fixture's "spectrum128" scope mean 128/+2 only? **If +2A/+3 needs
   something the pair cannot carry, that is a draft-format finding.**
4. **Shadow screen, bank 7.** Bit 3 of `$7FFD` selects bank 7 as the *displayed*
   screen independently of what is paged at `$C000`. Purely a display concern, or
   does it touch address attribution?
5. **Slot numbering.** The fixture uses 0-based slots indexed by 16K window
   (`$C000` → 3). Does Emu198x number them the same way? A mismatch is a spec
   wording fix, not a format change — but it should be caught here rather than by
   a third implementer.

## The verdict

Each claim against the test that settles it, in
`crates/runtime-sinclair-zx-spectrum/tests/debug198x_paging_cross_check.rs`:

| Claim | Verdict | Settled by |
|-------|---------|-----------|
| **H1** slot 3 is `$C000–$FFFF`, the pageable window | ✅ holds | `slot_three_is_the_switchable_window_the_fixture_assumes` |
| **H2** pages 1 and 3 are both legally pageable | ✅ holds | `every_page_the_fixture_names_is_one_the_hardware_can_select` |
| **H3** one slot holds one page at a time | ✅ holds | `the_symbol_at_one_address_follows_the_machines_paging_state` |
| **H4** two symbols in different pages share a CPU address | ✅ holds | same, plus `a_page_the_image_has_no_code_in_answers_nothing` |
| **H5** `Section.space` expresses what the model does | ✅ holds, **with a correction** | `a_bank_live_in_two_slots_at_once_answers_at_both_addresses` |
| **T1** the SLD projection | ✅ now tied to real RAM | `the_sld_long_address_projection_matches_this_machines_banks` |

H3 was the highest-consequence claim — false would have meant the format's model
was wrong, not just the fixture. It holds.

T1 gained something the desk exercise could not give it: each long address is
checked to land inside the bank its page names, and `music`'s long address
coinciding with its CPU address is asserted as arithmetic (`3 * $4000 == $C000`)
rather than meaning, so the coincidence cannot later be read as a rule.

### Answers to the open questions

1. **Fixed banks — answered, yes.** `{slot: 1, page: 5}` is meaningful and works.
   The dual-window test describes slot 1 holding page 5 at `$4000` *and* slot 3
   holding page 5 at `$C000`, and the same symbol answers at both.
2. **The ROM slot** — not exercised. Still open, and still cheap: no consumer has
   wanted it.
3. **+2A/+3 `$1FFD` all-RAM configurations** — not exercised. The fixture's scope
   is 128/+2, and whether `{slot, page}` covers the +2A/+3 modes is untested.
4. **Shadow screen, bank 7** — not exercised. No consumer has needed display
   state in address attribution.
5. **Slot numbering — answered, yes.** Slot 3 is `$C000` and slot 1 is `$4000`:
   0-based by 16 KiB window, exactly as the fixture assumes.

Questions 2–4 are recorded as untested rather than resolved. None blocks the
freeze — each concerns a shape no producer emits and no consumer reads, and the
catch-all added on 2026-08-18 means a later shape can be added without breaking a
v1 reader.

## What leg 3 changed

The cross-check earned its keep by finding what the fixture alone could not.

Both fixture sections sit in slot 3, so the fixture cannot distinguish "join on
the page" from "join on the (slot, page) pair" — they agree on every record it
contains. Driving a real machine broke the tie: a 128K keeps bank 5 at `$4000`
permanently **and** can select it into `$C000`, so one page is live at two CPU
addresses at once. A pair match makes that impossible by construction.

`page` is the join key; `slot` records where the producer expected the bank. That
was stated correctly on the spec page and **wrongly** in the crate's own rustdoc,
which asserted the pair was the discriminator — and the worked example shipped
with `Section.space` matched the pair too. Fixed in asm198x/asm198x#76, with a
regression guard that fails under a pair match rather than passing under both.

The lesson is the one this format keeps re-teaching: a contract stated in one
place and contradicted in another is worse than one stated nowhere, because the
reader finds the wrong half first.
