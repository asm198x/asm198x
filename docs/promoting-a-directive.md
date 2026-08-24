# Promoting a directive

The most-repeated task in this repo: moving one word from a dialect's
`KnownUnsupported` list to `Category::Operation`. There are ~528 left, so the
procedure is written down.

Not a gate. The coverage it describes is checked by hand, deliberately: the
notion of "covered" spans three harnesses and is subtle enough that a blocking
test built on it would produce false failures and get suppressed. The
invariants that *are* enforced — declared-vs-dispatched, no-spelling-claimed-
twice — check internal consistency, where both sides live in the same table.

## The steps

1. **Probe the reference first.** Before writing anything, run the tool and read
   the bytes. This is the step that repeatedly pays: vasm's `align` takes an
   *exponent*, ca65's `.align` pads relative to the segment rather than the
   address, a label on a ca65 `.align` line binds *before* the pad, and vasm's
   bare `dc`/`dcb`/`ds` are words. Every one of those contradicts the obvious
   reading, and three of them were wrong in the tree.

2. **Declare** the spelling in that dialect's `DIRECTIVES` as
   `Category::Operation`.

3. **Remove** it from the `KnownUnsupported` list. Leaving it in both fails the
   surface invariants, which is the one part of this that enforces itself.

4. **Dispatch** it — an arm in the dialect's directive match.

5. **Add a differential probe.** `PROBES` for a single-file case, `MULTI_PROBES`
   where the dialect only has a multi-file harness (the asl chips, the ca65 NES
   and HuC6280 legs). This is the step with no backstop: nothing stops a word
   being promoted with unit tests only, and if that happens the corpus never
   learns about it.

6. **Unit-test what the probe cannot reach.** A probe runs only where the
   reference *accepts* the source, so refusals, error text, and directives whose
   whole purpose is to fail (`!error`, `FAIL`) are unit tests by necessity — see
   [`decisions/verifying-non-byte-behaviour.md`](../decisions/verifying-non-byte-behaviour.md).

7. **Check the formatter round-trips it.** `asm198x fmt` must reproduce the new
   spelling. Two formatter bugs have been caught this way.

8. **Regenerate the stamp** — `cargo xtask surface --write`, and **run it
   alone**. Two concurrent runs share their probe files and corrupt each other;
   the self-checks caught it once, reporting a 152-word improvement that was not
   real.

## What goes wrong

- **Splicing.** Adding a trait method by replacing a region has dropped
  neighbouring overrides twice. The differential caught it both times, by luck
  rather than design — unrelated probes happened to exercise them.
- **Sweeping instructions in as directives.** A first pass at lwasm declared
  fifteen 6809 *instructions*, because a reference answers "bad operand" for an
  operand-less instruction and an unimplemented directive alike. Give each
  candidate an operand and read the bytes it emits; counting them is not enough
  (`dts` assembles to the ASCII of the current date).
- **Reasoning from the manual.** See step 1.

## Before assuming a word is unprobed

Coverage spans three harnesses. `differential.rs` holds the hand-written probes;
`conformance.rs` drives the asl family against `asl` + `p2bin`;
`curriculum.rs` builds real programs. A word missing from the first is not
unchecked — nine dialects read as having no probes at all until you notice they
are driven from the second.
