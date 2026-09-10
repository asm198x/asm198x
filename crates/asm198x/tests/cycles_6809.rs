//! Capture integration: the shared ISA owns the hardware facts; these tests
//! prove byte identity, source attribution, and honest consumer bounds.
use asm198x::{CycleCoverage, assemble_lwasm, render_listing, render_listing_json};

fn capture(source: &str) -> Vec<(u64, Option<u64>)> {
    let result = assemble_lwasm(source).expect(source);
    assert_eq!(result.debug.cycle_coverage, CycleCoverage::Full);
    result.debug.cycles.iter().map(|c| c.range()).collect()
}

#[test]
fn addressing_and_operand_costs_follow_the_final_encoding() {
    assert_eq!(
        capture(
            " org $1000\n lda #1\n lda <$12\n lda >$1234\n lda ,x\n lda 5,x\n lda >5,x\n lda [>5,x]\n pshs a,x\n puls cc,d,x,y,u,pc\n tfr d,x\n exg a,b\n andcc #$ef\n orcc #$10\n"
        ),
        vec![
            (2, Some(2)),
            (4, Some(4)),
            (5, Some(5)),
            (4, Some(4)),
            (5, Some(5)),
            (8, Some(8)),
            (11, Some(11)),
            (8, Some(8)),
            (16, Some(16)),
            (6, Some(6)),
            (8, Some(8)),
            (3, Some(3)),
            (3, Some(3))
        ]
    );
}

#[test]
fn branches_interrupts_and_waits_keep_their_distinct_bounds() {
    assert_eq!(
        capture(
            " org $1000\n bne *\n lbne *\n lbra *\n swi\n swi2\n swi3\n rti\n sync\n cwai #$ef\n"
        ),
        vec![
            (3, Some(3)),
            (5, Some(6)),
            (5, Some(5)),
            (19, Some(19)),
            (20, Some(20)),
            (20, Some(20)),
            (6, Some(15)),
            (4, None),
            (20, None)
        ]
    );
}

#[test]
fn budget_uses_the_runtime_maximum_and_refuses_unbounded_waits() {
    for (instruction, max) in [("rti", 15), ("lbne *", 6), ("lda [>5,x]", 11)] {
        let source =
            format!(" org $1000\nstart {instruction}\n; asm198x: cycles(start) <= {max}\n");
        assemble_lwasm(&source).expect("at ceiling");
        let error =
            assemble_lwasm(&source.replace(&format!("<= {max}"), &format!("<= {}", max - 1)))
                .expect_err("below ceiling");
        assert_eq!(error.code, asm198x::Code::CycleBudgetExceeded);
    }
    for instruction in ["sync", "cwai #$ef"] {
        let error = assemble_lwasm(&format!(
            " org $1000\nstart {instruction}\n; asm198x: cycles(start) <= 18446744073709551615\n"
        ))
        .expect_err("no finite ceiling");
        assert!(
            error.message.contains("unbounded wait"),
            "{}",
            error.message
        );
        assert_eq!(error.line, 3);
    }
    // A separate wait label does not make a known finite routine unbounded.
    assemble_lwasm(" org $1000\nstart rts\nwait sync\n; asm198x: cycles(start) <= 5\n")
        .expect("bounded routine");
}

#[test]
fn computed_data_is_never_mistaken_for_an_instruction() {
    let result = assemble_lwasm(" org $1000\n fqb $12121212\n fcb $12\n fdb $1212\nstart nop\n")
        .expect("data and code");
    assert_eq!(result.debug.cycles.len(), 1);
    assert_eq!(result.debug.cycles[0].line, 5);
    assert_eq!(result.debug.cycles[0].offset, 7);
    assert_eq!(result.debug.cycles[0].range(), (2, Some(2)));
}

#[test]
fn listing_and_json_preserve_unbounded_totals_and_rti_range() {
    let source = " org $1000\nstart rti\n sync\n cwai #$ef\n";
    let result = assemble_lwasm(source).expect("assemble");
    let text = render_listing(source, &result, 1);
    assert!(text.contains("6/15"), "{text}");
    assert!(text.contains(">=4"), "{text}");
    assert!(text.contains(">=20"), "{text}");
    assert!(text.contains(">=30 cycles"), "{text}");
    let json: serde_json::Value =
        serde_json::from_str(&render_listing_json("input.asm", &result, 1)).expect("json");
    assert_eq!(json["coverage"], "full");
    assert_eq!(
        json["lines"][0]["cycles"],
        serde_json::json!({"min":6,"max":15})
    );
    assert_eq!(
        json["lines"][1]["cycles"],
        serde_json::json!({"min":4,"max":null})
    );
    assert_eq!(
        json["labels"][0]["cycles"],
        serde_json::json!({"min":30,"max":null})
    );
    let rec = &result.debug.cycles[1];
    let serialized = serde_json::to_value(rec).expect("record");
    assert_eq!(
        serialized["bounds"],
        serde_json::json!({"min":4,"max":null})
    );
    let roundtrip: asm198x::CycleRec =
        serde_json::from_value(serialized).expect("record roundtrip");
    assert_eq!(roundtrip, *rec);
}

#[test]
fn old_cycle_records_keep_their_range_and_serialized_shape() {
    let value = serde_json::json!({"line":1,"offset":0,"base":2,"page_cross":1,"branch_taken":1});
    let rec: asm198x::CycleRec = serde_json::from_value(value.clone()).expect("old record");
    assert_eq!(rec.range(), (2, Some(4)));
    assert_eq!(serde_json::to_value(rec).expect("serialize"), value);
}

#[test]
fn every_documented_row_reassembles_with_timing() {
    let mut count = 0;
    for row in isa::mos6809::rows().filter(|r| !r.undocumented) {
        let insn = isa::mos6809::lookup(row.mnemonic).expect("row");
        let (bytes, len) = insn.exemplar(&row.mode).expect("exemplar");
        let source = asm198x::listing_6809(&bytes[..len], 0x1000);
        let result = assemble_lwasm(&source).expect(&source);
        assert_eq!(result.bytes, bytes[..len], "{source}");
        assert_eq!(result.debug.cycle_coverage, CycleCoverage::Full, "{source}");
        assert_eq!(result.debug.cycles.len(), 1, "{source}");
        count += 1;
    }
    assert_eq!(count, 277);
}

#[test]
fn direct_page_selection_and_forward_operands_capture_the_selected_cost() {
    let source =
        " org $1000\n setdp $10\nback nop\n lda back\n lda forward\n ldx #forward\nforward rts\n";
    assert_eq!(
        capture(source),
        vec![
            (2, Some(2)),
            (4, Some(4)),
            (5, Some(5)),
            (3, Some(3)),
            (5, Some(5))
        ]
    );
}

#[test]
fn undocumented_instructions_remain_unknown_and_refuse_budgets() {
    for instruction in ["reset", "rhf", "hcf"] {
        let source = format!(" org $1000\nstart nop\n {instruction}\n");
        let result = assemble_lwasm(&source).expect("accepted undocumented encoding");
        assert_eq!(result.debug.cycle_coverage, CycleCoverage::Partial);
        assert_eq!(result.debug.cycles.len(), 1);
        let error = assemble_lwasm(&format!("{source}; asm198x: cycles(start) <= 999\n"))
            .expect_err("unknown timing");
        assert!(error.message.contains("lower bounds"), "{}", error.message);
    }
}

#[test]
fn includes_macros_and_conditional_emissions_keep_capture_and_budgets() {
    use asm198x::{assemble_lwasm_files, source::MemoryLoader};
    let body = "load macro\n lda #\\1\n endm\nstart load 1\n load 2\n if 0\n sync\n endc\n rts\n; asm198x: cycles(start) <= 9\n";
    let loader = MemoryLoader::new().text("body.inc", body);
    let source = " org $1000\n include \"body.inc\"\n";
    let result = assemble_lwasm_files(source, "main.asm", &loader).expect("include with macro");
    assert_eq!(result.bytes, vec![0x86, 1, 0x86, 2, 0x39]);
    assert_eq!(
        result
            .debug
            .cycles
            .iter()
            .map(|c| c.range())
            .collect::<Vec<_>>(),
        vec![(2, Some(2)), (2, Some(2)), (5, Some(5))]
    );
    assert!(result.debug.cycles.iter().all(|c| c.file.0 == 1));
    assert_eq!(
        result
            .debug
            .cycles
            .iter()
            .map(|c| c.offset)
            .collect::<Vec<_>>(),
        vec![0, 2, 4]
    );
    let loader = MemoryLoader::new().text("body.inc", body.replace("<= 9", "<= 8"));
    let error =
        assemble_lwasm_files(source, "main.asm", &loader).expect_err("include budget enforced");
    assert_eq!(error.error.code, asm198x::Code::CycleBudgetExceeded);
    assert!(
        error.error.message.contains("9 worst case"),
        "{}",
        error.error.message
    );
}
