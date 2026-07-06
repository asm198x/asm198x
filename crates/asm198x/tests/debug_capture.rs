//! U2 — engine debug capture. Asserts that pass 2 records typed symbols and
//! line→address spans onto `Assembly.debug`, in address units, without changing
//! any emitted byte (the byte-identity story is the existing suite's job; here we
//! assert the capture is correct). Uses the pasmo/Z80 dialect as a representative
//! flat-engine path.

use asm198x::{assemble_cp1610, assemble_pasmo};
use debug198x::SymbolKind;

/// The `SymbolKind` recorded for a named symbol, or `None` if absent.
fn kind_of<'a>(a: &'a asm198x::AssemblyResult, name: &str) -> Option<&'a SymbolKind> {
    a.debug
        .symbols
        .iter()
        .find(|s| s.name == name)
        .map(|s| &s.kind)
}

#[test]
fn captures_labels_instructions_and_data() {
    let src = "\
        \torg 8000h\n\
        start:\n\
        \tld a,5\n\
        \tnop\n\
        data:\n\
        \tdb 1,2,3\n";
    let a = assemble_pasmo(src).expect("assemble");

    // `start` is a label at the org; `data` a label three bytes in.
    assert_eq!(
        kind_of(&a, "start"),
        Some(&SymbolKind::Label {
            section: 0,
            offset: 0,
            space: None
        })
    );
    assert_eq!(
        kind_of(&a, "data"),
        Some(&SymbolKind::Label {
            section: 0,
            offset: 3,
            space: None
        })
    );

    // A line span per source-bearing statement that emitted: `ld a,5` (2 bytes at
    // offset 0), `nop` (1 at 2), `db 1,2,3` (3 at 3).
    let spans: Vec<(u64, u64)> = a.debug.lines.iter().map(|l| (l.offset, l.length)).collect();
    assert_eq!(spans, vec![(0, 2), (2, 1), (3, 3)]);
}

#[test]
fn equ_is_a_constant_with_no_span() {
    let src = "\
        \torg 100h\n\
        five equ 5\n\
        \tnop\n";
    let a = assemble_pasmo(src).expect("assemble");
    assert_eq!(kind_of(&a, "five"), Some(&SymbolKind::Const { value: 5 }));
    // The equ line emits nothing, so it produces no span; only `nop` does.
    assert_eq!(a.debug.lines.len(), 1);
    assert_eq!((a.debug.lines[0].offset, a.debug.lines[0].length), (0, 1));
}

#[test]
fn label_and_equ_of_equal_value_carry_different_kinds() {
    // `here` is a label whose address is 0x0100; `ONE_HUNDRED` an equ of 0x0100.
    let src = "\
        \torg 100h\n\
        here:\n\
        \tnop\n\
        one_hundred equ 100h\n";
    let a = assemble_pasmo(src).expect("assemble");
    assert_eq!(
        kind_of(&a, "here"),
        Some(&SymbolKind::Label {
            section: 0,
            offset: 0,
            space: None
        })
    );
    assert_eq!(
        kind_of(&a, "one_hundred"),
        Some(&SymbolKind::Const { value: 0x100 })
    );
}

#[test]
fn org_gap_produces_no_line_span() {
    // A forward `org` leaves a fill gap that must carry no source attribution.
    let src = "\
        \torg 8000h\n\
        \tnop\n\
        \torg 8010h\n\
        \tnop\n";
    let a = assemble_pasmo(src).expect("assemble");
    // Two `nop`s, two spans — the 15-byte org gap between them has none.
    let spans: Vec<(u64, u64)> = a.debug.lines.iter().map(|l| (l.offset, l.length)).collect();
    assert_eq!(spans, vec![(0, 1), (0x10, 1)]);
}

#[test]
fn entry_point_is_an_entry_symbol_named_after_its_label() {
    let src = "\
        \torg 8000h\n\
        \tnop\n\
        go:\n\
        \tnop\n\
        \tend go\n";
    let a = assemble_pasmo(src).expect("assemble");
    // `end go` records an Entry symbol at go's address (offset 1).
    assert_eq!(
        kind_of(&a, "go"),
        Some(&SymbolKind::Entry {
            section: 0,
            offset: 1,
            space: None
        })
    );
}

#[test]
fn cp1610_spans_are_in_decles_not_bytes() {
    // The word-addressed CP1610 must record offsets/lengths in decles (one per
    // two emitted bytes), so a consumer's decle addresses line up.
    let src = "\
        \torg 0\n\
        \tmovr r0,r1\n\
        \tmvii 1234h,r0\n";
    let a = assemble_cp1610(src).expect("assemble");
    // movr is one decle (offset 0, len 1); mvii is two decles — opcode + immediate
    // (offset 1, len 2). In bytes that is 2 then 4, but the capture is in decles.
    let spans: Vec<(u64, u64)> = a.debug.lines.iter().map(|l| (l.offset, l.length)).collect();
    assert_eq!(spans, vec![(0, 1), (1, 2)]);
}
