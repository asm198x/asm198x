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
