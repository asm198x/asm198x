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

/// AE2/R5: `; asm198x: cycles(<label>) <= N` fails the assemble when the
/// routine's straight-line worst case exceeds N, naming routine, budget, and
/// actual — and passes when it fits. Worst case here: LDA # (2) + RTS (6).
/// Enforcement is the engine's, so plain `assemble_*` is the whole test.
#[test]
fn cycle_budget_passes_within_and_fails_beyond() {
    let src = "        * = $c000\n; asm198x: cycles(start) <= 8\nstart   lda #1\n        rts\n";
    assemble_acme(src).expect("within budget");

    let src = "        * = $c000\n; asm198x: cycles(start) <= 7\nstart   lda #1\n        rts\n";
    let e = assemble_acme(src).expect_err("over budget");
    assert_eq!(e.line, 2, "the diagnostic points at the assertion");
    for needle in ["start", "7", "8"] {
        assert!(
            e.message.contains(needle),
            "names routine, budget, actual: {}",
            e.message
        );
    }
}

/// A budget naming a label the program does not define is an error — a typo'd
/// assertion that silently checked nothing would fake the very assurance it
/// exists to give.
#[test]
fn cycle_budget_on_an_unknown_label_is_an_error() {
    let src = "        * = $c000\n; asm198x: cycles(missing) <= 10\nstart   rts\n";
    let e = assemble_acme(src).expect_err("no such label");
    assert!(e.message.contains("missing"), "{}", e.message);
}

/// A malformed `asm198x:` comment is an error for the same reason: it was
/// written as an assertion, so it must never be skimmed past as prose.
#[test]
fn malformed_budget_comment_is_an_error() {
    let src = "        * = $c000\n; asm198x: cycles(start) < 10\nstart   rts\n";
    let e = assemble_acme(src).expect_err("bad spelling");
    assert_eq!(e.line, 2);
}

/// R6: a budget cannot be proven where the capture is not Full — a CPU with
/// no cycle data refuses rather than passing on nothing.
#[test]
fn cycle_budget_without_cycle_data_is_refused() {
    let src = "        org $1000\n; asm198x: cycles(start) <= 10\nstart   nop\n";
    let e = assemble_lwasm(src).expect_err("no data to check against");
    assert!(e.message.contains("no cycle data"), "{}", e.message);
}

/// Budgets travel through includes: an assertion in an included file guards
/// the label it sits beside, through the multi-file entry.
#[test]
fn cycle_budget_inside_an_include_is_checked() {
    use asm198x::source::MemoryLoader;
    let loader = MemoryLoader::new().text(
        "part.inc",
        "; asm198x: cycles(fast) <= 5\nfast    ld a,1\n        ret\n",
    );
    let src = "        org $8000\n        include \"part.inc\"\n";
    let e = asm198x::assemble_sjasmplus_files(src, "main.s", &loader)
        .expect_err("7 + 10 cycles against 5");
    assert!(e.error.message.contains("fast"), "{}", e.error.message);
}

/// R4/AE4: the JSON listing carries the same per-line and per-label data as
/// the human table — address, size, honest min/max — plus the coverage fact.
#[test]
fn json_listing_carries_lines_labels_and_coverage() {
    let src =
        "        * = $c000\n        lda #1\n        lda $1234,x\nloop    bne loop\n        rts\n";
    let r = assemble_acme(src).expect("assembles");
    let json = asm198x::render_listing_json("main.s", &r, 1);
    let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(v["coverage"], "full");
    assert_eq!(v["lines"][1]["address"], 49154);
    assert_eq!(v["lines"][1]["bytes"], 3);
    assert_eq!(v["lines"][1]["cycles"]["max"], 5);
    assert_eq!(
        v["lines"][3]["cycles"],
        serde_json::json!({"min": 6, "max": 6})
    );
    assert_eq!(
        v["labels"][0],
        serde_json::json!({"name": "loop", "address": 49157, "bytes": 3,
                           "cycles": {"min": 8, "max": 10}})
    );
    assert_eq!(v["lines"][0]["file"], "main.s");
}
