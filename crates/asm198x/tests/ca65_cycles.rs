//! Native ca65 budgets use the resolved ISA form and section-local offsets.
use asm198x::{Code, assemble_ca65, source::MemoryLoader};

#[path = "support/scratch.rs"]
mod scratch;

#[test]
#[ignore = "needs ca65 + ld65 on PATH"]
fn budgeted_link_is_byte_identical_to_native_ca65_and_ld65() {
    let dir = scratch::dir("ca65-cycle-reference");
    let cfg = "MEMORY { ROM: start=$8000, size=$100, file=%O, fill=yes; }\nSEGMENTS { ONE: load=ROM, type=ro; TWO: load=ROM, type=ro; }";
    let src = ".segment \"ONE\"\nfirst: lda #1\n lda $1234,x\n bne first\n rts\n.segment \"TWO\"\nsecond: rts\n; asm198x: cycles(first) <= 17\n; asm198x: cycles(second) <= 6\n";
    std::fs::write(dir.join("t.cfg"), cfg).expect("config");
    std::fs::write(dir.join("t.s"), src).expect("source");
    for (tool, args) in [
        ("ca65", vec!["t.s", "-o", "t.o"]),
        ("ld65", vec!["-C", "t.cfg", "t.o", "-o", "reference.bin"]),
    ] {
        let out = std::process::Command::new(tool)
            .args(args)
            .current_dir(&dir)
            .output()
            .expect("reference tool");
        assert!(
            out.status.success(),
            "{tool}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let ours = asm198x::assemble_ca65_files_with_config(src, "t.s", &MemoryLoader::new(), cfg)
        .expect("budgeted link");
    assert_eq!(
        ours.bytes,
        std::fs::read(dir.join("reference.bin")).expect("reference bytes")
    );
}

#[test]
fn all_native_entries_check_the_same_budget_without_changing_bytes() {
    let source = ".segment \"CODE\"\nstart: lda #1\n rts\n";
    let pass = format!("{source}; asm198x: cycles(start) <= 8\n");
    let fail = pass.replace("<= 8", "<= 7");
    let loader = MemoryLoader::new();
    let expected = assemble_ca65(source).expect("plain ROM").bytes;
    assert_eq!(assemble_ca65(&pass).expect("eight cycles").bytes, expected);
    assert_eq!(
        assemble_ca65(&fail).expect_err("over budget").code,
        Code::CycleBudgetExceeded
    );
    assert_eq!(
        asm198x::assemble_ca65_debug(&pass, "main.s")
            .expect("debug")
            .0
            .bytes,
        expected
    );
    assert_eq!(
        asm198x::assemble_ca65_debug(&fail, "main.s")
            .expect_err("debug budget")
            .code,
        Code::CycleBudgetExceeded
    );
    assert_eq!(
        asm198x::assemble_ca65_files(&pass, "main.s", &loader)
            .expect("files")
            .bytes,
        expected
    );
    assert_eq!(
        asm198x::assemble_ca65_files(&fail, "main.s", &loader)
            .expect_err("files budget")
            .error
            .code,
        Code::CycleBudgetExceeded
    );
    assert_eq!(
        asm198x::assemble_ca65_files_debug(&pass, "main.s", &loader)
            .expect("files debug")
            .0
            .bytes,
        expected
    );
    assert_eq!(
        asm198x::assemble_ca65_files_debug(&fail, "main.s", &loader)
            .expect_err("files debug budget")
            .error
            .code,
        Code::CycleBudgetExceeded
    );
}

#[test]
fn resolved_modes_and_conditional_penalties_come_from_the_spec() {
    // Zero-page LDA (3), absolute-X LDA (4/5), BNE (2/4), RTS (6).
    let src = ".segment \"ZEROPAGE\"\nzp: .res 1\n.segment \"CODE\"\nstart: lda zp\n lda $1234,x\n bne start\n rts\n; asm198x: cycles(start) <= 18\n";
    assemble_ca65(src).expect("3 + 5 + 4 + 6");
    let error = assemble_ca65(&src.replace("<= 18", "<= 17")).expect_err("worst case 18");
    assert_eq!(error.code, Code::CycleBudgetExceeded);
    assert!(error.message.contains("18 worst case"));
}

#[test]
fn section_offsets_aliases_and_data_do_not_mix_routines() {
    let cfg = "MEMORY { ROM: start=$8000, size=$100, file=%O, fill=yes; }\nSEGMENTS { ONE: load=ROM, type=ro; TWO: load=ROM, type=ro; }";
    let src = ".segment \"ONE\"\na:\nalias: lda #1\n rts\n .byte $60,$60\nnext: nop\n.segment \"TWO\"\nb: rts\n; asm198x: cycles(a) <= 8\n; asm198x: cycles(alias) <= 8\n; asm198x: cycles(next) <= 2\n; asm198x: cycles(b) <= 6\n";
    let loader = MemoryLoader::new();
    let r = asm198x::assemble_ca65_files_with_config(src, "main.s", &loader, cfg)
        .expect("independent section offsets");
    assert_eq!(&r.bytes[..7], [0xa9, 1, 0x60, 0x60, 0x60, 0xea, 0x60]);
    let fail = src.replace("cycles(b) <= 6", "cycles(b) <= 5");
    assert_eq!(
        asm198x::assemble_ca65_files_with_config(&fail, "main.s", &loader, cfg)
            .expect_err("configured budget")
            .error
            .code,
        Code::CycleBudgetExceeded
    );
    asm198x::assemble_ca65_files_debug_with_config(src, "main.s", &loader, cfg)
        .expect("configured debug");
    assert_eq!(
        asm198x::assemble_ca65_files_debug_with_config(&fail, "main.s", &loader, cfg)
            .expect_err("configured debug budget")
            .error
            .code,
        Code::CycleBudgetExceeded
    );
}

#[test]
fn included_budget_diagnostic_names_the_included_file() {
    let loader =
        MemoryLoader::new().text("routine.inc", "start: rts\n; asm198x: cycles(start) <= 5\n");
    let e = asm198x::assemble_ca65_files(
        ".segment \"CODE\"\n.include \"routine.inc\"\n",
        "main.s",
        &loader,
    )
    .expect_err("included budget");
    assert_eq!(e.error.code, Code::CycleBudgetExceeded);
    assert_eq!(e.error.line, 2);
    assert_eq!(e.error.span.expect("span").file.0, 1);
}

#[test]
fn reservations_data_and_unknown_labels_cannot_prove_a_cycle_budget() {
    for src in [
        ".segment \"CODE\"\ndata: .byte $60\n; asm198x: cycles(data) <= 100\n",
        ".segment \"ZEROPAGE\"\nspace: .res 1\n; asm198x: cycles(space) <= 100\n",
        ".segment \"CODE\"\nstart: rts\n; asm198x: cycles(missing) <= 100\n",
    ] {
        let e = assemble_ca65(src).expect_err("no instruction-bearing label");
        assert!(
            e.message.contains("names no label with instructions"),
            "{}",
            e.message
        );
    }
}

#[test]
fn org_and_macros_preserve_section_local_accounting() {
    let src = ".macro load\n lda #1\n.endmacro\n.segment \"CODE\"\n.org $2000\nstart: load\n load\n rts\n.reloc\nnext: nop\n; asm198x: cycles(start) <= 10\n; asm198x: cycles(next) <= 2\n";
    assemble_ca65(src).expect("two macro instructions and rts");
    assert_eq!(
        assemble_ca65(&src.replace("<= 10", "<= 9"))
            .expect_err("ten cycles")
            .code,
        Code::CycleBudgetExceeded
    );
}
