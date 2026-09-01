//! #497 — cycle capture for the cycle-honest listing. Costs come only from
//! the validated spec: every expectation below is the spec's own row
//! (`isa::mos6502`: LDA immediate `fixed(2)`, LDA absolute,x
//! `page_crossing(4)`, BNE relative `branch(2)`), so a change here means the
//! capture drifted from the spec, not that timing changed.

use asm198x::{assemble_acme, assemble_lwasm};

/// R1/R6: pass 2 records the chosen form's cycle triple per instruction, in
/// emission order, from the spec and nowhere else.
#[test]
fn pass_two_captures_the_spec_cycles_per_instruction() {
    let src = "        * = $c000\n        lda #1\n        lda $1234,x\nloop    bne loop\n";
    let r = assemble_acme(src).expect("assembles");
    let got: Vec<(u32, u8, u8, u8)> = r
        .debug
        .cycles
        .iter()
        .map(|c| (c.line, c.base, c.page_cross, c.branch_taken))
        .collect();
    assert_eq!(
        got,
        vec![(2, 2, 0, 0), (3, 4, 1, 0), (4, 2, 1, 1)],
        "one record per instruction, carrying the spec triple"
    );
}

/// Data emissions are not instructions: no record, rather than a zero — the
/// absence is "nothing executes here", which a zero would misstate.
#[test]
fn data_lines_carry_no_cycle_records() {
    let r = assemble_acme("        * = $c000\n        !byte 1, 2, 3\n        lda #1\n")
        .expect("assembles");
    assert_eq!(r.debug.cycles.len(), 1, "only the lda has a record");
    assert_eq!(r.debug.cycles[0].line, 3);
}

/// R6/AE5: a CPU whose spec carries no cycle data (#498) captures nothing —
/// never a fabricated number. The listing's "backfill pending" note renders
/// from the dialect's declared coverage, pinned in the listing tests.
#[test]
fn field_packed_instructions_capture_no_cycles() {
    let r = assemble_lwasm("        org $1000\n        nop\n").expect("assembles");
    assert!(r.debug.cycles.is_empty(), "no fabricated numbers");
}

use asm198x::{assemble_tms7000, render_listing};

/// AE1/AE3/R1–R3: the listing's cycles column and per-label totals, pinned.
/// Every figure is the spec's: LDA # fixed(2); LDA abs,x page_crossing(4)
/// rendered as the honest 4/5, never one number; BNE branch(2) as 2/4; RTS
/// fixed(6). `loop`'s total is its straight-line span (BNE + RTS): 8/10.
#[test]
fn listing_carries_the_cycles_column_and_label_totals() {
    let src =
        "        * = $c000\n        lda #1\n        lda $1234,x\nloop    bne loop\n        rts\n";
    let r = asm198x::assemble_acme(src).expect("assembles");
    let expected = "                                            * = $c000
C000  A9 01                    2            lda #1
C002  BD 34 12                 4/5          lda $1234,x
C005  D0 FE                    2/4  loop    bne loop
C007  60                       6            rts

cycle totals (spec, straight-line to the next label):
  loop ($C005)  3 bytes, 8/10 cycles
";
    assert_eq!(render_listing(src, &r, 1), expected);
}

/// AE5/R6: a CPU whose spec carries no cycle data keeps its exact old listing
/// — no column, no invented numbers — plus the one honest note.
#[test]
fn field_packed_listing_says_backfill_pending() {
    let src = "        org $1000\n        nop\n";
    let r = assemble_lwasm(src).expect("assembles");
    let expected = "                                       org $1000
1000  12                               nop

no cycle data (backfill pending)
";
    assert_eq!(render_listing(src, &r, 1), expected);
}

/// A partial-coverage dialect (some instructions pre-encode into pieces and
/// capture nothing) says its figures are lower bounds rather than presenting
/// them as complete.
#[test]
fn partial_coverage_listing_declares_lower_bounds() {
    let src = "        org 8000h\n        nop\n";
    let r = assemble_tms7000(src).expect("assembles");
    let listing = render_listing(src, &r, 1);
    assert!(
        listing.contains("cycle figures are lower bounds"),
        "partial coverage must say so:\n{listing}"
    );
}
