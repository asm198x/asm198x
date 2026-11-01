//! The HuC6280 through the ca65 walk, as `ca65 --cpu huc6280` assembles it
//! and links it flat. These began as the old HuC6280 reader's own tests;
//! the reader is gone, and Asm198x's recorded verdicts and Lang198x's
//! lowering tests carry the rest.

use asm198x::assemble_ca65_huc6280 as asm;

fn bytes(src: &str) -> Vec<u8> {
    asm(src).unwrap_or_else(|e| panic!("{src}: {e:?}")).bytes
}

/// `(expr,x)` means two different things depending on the mnemonic. For
/// almost every instruction it is the 6502's zero-page `(indirect,x)`; for
/// `jmp` it is the 65C02/HuC6280 16-bit `(absolute,x)` ($7C), which has no
/// zero-page counterpart.
///
/// The front-end used to label both as zero-page, so `jmp ($1234,x)` was
/// unassemblable even though the spec carried the form, our disassembler
/// emitted it, and ca65 accepted it. Found by replaying the recorded ca65
/// verdicts (#61): the form audit could not see it, because it only ever
/// asks the *reference* to assemble what our disassembler writes — never
/// us.
#[test]
fn indexed_indirect_resolves_by_the_mnemonic_not_the_syntax() {
    assert_eq!(bytes(" jmp ($1234,x)\n"), vec![0x7C, 0x34, 0x12]);
    assert_eq!(bytes(" lda ($12,x)\n"), vec![0xA1, 0x12]);
}

#[test]
fn inherited_6502_and_65c02_ops() {
    // Base 6502 through the extension-aware resolver.
    assert_eq!(bytes(" lda #$12\n"), vec![0xA9, 0x12]);
    assert_eq!(bytes(" lda $12\n"), vec![0xA5, 0x12]); // zp
    assert_eq!(bytes(" lda $1234\n"), vec![0xAD, 0x34, 0x12]); // abs
    assert_eq!(bytes(" sta $10,x\n"), vec![0x95, 0x10]);
    assert_eq!(bytes(" jmp ($1234)\n"), vec![0x6C, 0x34, 0x12]); // jmp indirect
    // 65C02 additions from the extension.
    assert_eq!(bytes(" bra l\nl: rts\n"), vec![0x80, 0x00, 0x60]);
    assert_eq!(bytes(" stz $12\n"), vec![0x64, 0x12]);
    assert_eq!(bytes(" lda ($12)\n"), vec![0xB2, 0x12]); // (dp) indirect
    assert_eq!(bytes(" phx\n"), vec![0xDA]);
}

#[test]
fn huc6280_implied_and_bit_ops() {
    assert_eq!(bytes(" sax\n"), vec![0x22]);
    assert_eq!(bytes(" csh\n"), vec![0xD4]);
    assert_eq!(bytes(" rmb0 $10\n"), vec![0x07, 0x10]);
    assert_eq!(bytes(" smb7 $10\n"), vec![0xF7, 0x10]);
    // bbr/bbs: zero-page byte, then a relative target (self-branch = -3).
    assert_eq!(
        bytes(".org $1000\nl: bbr0 $10, l\n"),
        vec![0x0F, 0x10, 0xFD]
    );
    assert_eq!(
        bytes(".org $1000\nl: bbs7 $10, l\n"),
        vec![0xFF, 0x10, 0xFD]
    );
}

#[test]
fn huc6280_exotic_forms() {
    // st0-2 / tam / tma: opcode + one immediate.
    assert_eq!(bytes(" st0 #$aa\n"), vec![0x03, 0xAA]);
    assert_eq!(bytes(" tam #$01\n"), vec![0x53, 0x01]);
    assert_eq!(bytes(" tma #$01\n"), vec![0x43, 0x01]);
    // tst #imm, <mem> across all four modes.
    assert_eq!(bytes(" tst #$55, $10\n"), vec![0x83, 0x55, 0x10]);
    assert_eq!(bytes(" tst #$55, $1234\n"), vec![0x93, 0x55, 0x34, 0x12]);
    assert_eq!(bytes(" tst #$55, $10,x\n"), vec![0xA3, 0x55, 0x10]);
    assert_eq!(bytes(" tst #$55, $1234,x\n"), vec![0xB3, 0x55, 0x34, 0x12]);
    // bsr: relative like bra.
    assert_eq!(bytes(" bsr sub\nsub: rts\n"), vec![0x44, 0x00, 0x60]);
}

#[test]
fn low_absolute_takes_a_force() {
    // A low absolute must assemble to the absolute form via `a:`, not
    // zero-page — and the disassembler must round-trip it byte-exact.
    assert_eq!(bytes(" lda a:$0080\n"), vec![0xAD, 0x80, 0x00]);
    let dis = asm198x::disassemble_huc6280(&[0xAD, 0x80, 0x00], 0x0000);
    assert!(dis[0].text.contains("a:$0080"), "got `{}`", dis[0].text);
    assert_eq!(
        bytes(&format!(" {}\n", dis[0].text)),
        vec![0xAD, 0x80, 0x00]
    );
}

#[test]
fn multi_operand_forms_round_trip_via_disassembler() {
    // Assemble → disassemble → reassemble the tricky multi-operand shapes.
    // Assembly and disassembly both use origin 0 so branch targets align.
    for (src, expect) in [
        (".org 0\n bbr0 $10, $0005\n", vec![0x0F, 0x10, 0x02]),
        (" tst #$55, a:$0080,x\n", vec![0xB3, 0x55, 0x80, 0x00]),
        (
            " tii $1234, $5678, $0010\n",
            vec![0x73, 0x34, 0x12, 0x78, 0x56, 0x10, 0x00],
        ),
    ] {
        let asm = bytes(src);
        assert_eq!(asm, expect, "assemble `{src}`");
        let dis = asm198x::disassemble_huc6280(&asm, 0x0000);
        let reasm = bytes(&format!(".org 0\n {}\n", dis[0].text));
        assert_eq!(reasm, expect, "round-trip `{}`", dis[0].text);
    }
}

#[test]
fn huc6280_block_transfers() {
    // opcode, src-lo, src-hi, dst-lo, dst-hi, len-lo, len-hi.
    assert_eq!(
        bytes(" tii $1000, $2000, $0010\n"),
        vec![0x73, 0x00, 0x10, 0x00, 0x20, 0x10, 0x00]
    );
    assert_eq!(
        bytes(" tdd $1000, $2000, $0010\n"),
        vec![0xC3, 0x00, 0x10, 0x00, 0x20, 0x10, 0x00]
    );
    assert_eq!(
        bytes(" tai $1000, $2000, $0010\n"),
        vec![0xF3, 0x00, 0x10, 0x00, 0x20, 0x10, 0x00]
    );
}

/// The macro grammar is the ca65 family's, so it arrives
/// here for free — which is worth one test rather than none, because
/// "for free" is a claim about wiring and the wiring is per-dialect.
///
/// The HuC6280 body is deliberately native: `.local` scoping is proved on
/// this CPU's own encoder, not on the 65816's.
#[test]
fn macros_reach_this_dialect_too() {
    assert_eq!(
        bytes(".macro two\n nop\n nop\n.endmacro\n two\n"),
        vec![0xEA, 0xEA]
    );
    assert_eq!(
        bytes(".macro ldav v\n lda #v\n.endmacro\n ldav 5\n"),
        vec![0xA9, 0x05]
    );
    assert_eq!(
        bytes(".macro delay\n.local spin\nspin: dex\n bne spin\n.endmacro\n delay\n delay\n"),
        vec![0xCA, 0xD0, 0xFD, 0xCA, 0xD0, 0xFD]
    );
}

/// A label is made by its **colon**, not by its column — ca65 assembles an
/// indented one and so do we (#186).
///
/// The column rule this replaces did more than refuse valid source: it made
/// `fmt` unsafe. Indenting a macro body is correct ca65 layout, so the
/// formatter moved a body's label off column 0 and produced a file this
/// parser then refused. The round trip below is the property that broke.
#[test]
fn a_label_may_be_indented() {
    let flush = asm198x::assemble_ca65_huc6280("start: dex\n bne start\n").expect("column 0");
    let indented = asm198x::assemble_ca65_huc6280("        start: dex\n        bne start\n")
        .expect("indented");
    assert_eq!(flush.bytes, indented.bytes);
}

/// A first word without a colon is not a label wherever it sits, so an
/// ordinary indented instruction is unaffected.
#[test]
fn an_indented_instruction_is_not_read_as_a_label() {
    let out =
        asm198x::assemble_ca65_huc6280("        dex\n        dex\n").expect("two instructions");
    assert_eq!(out.bytes.len(), 2, "{out:?}");
}

/// The formatter round trip over an indented label. huc6280 has no macros,
/// so it never meets #186's own trigger — but it shared the column rule,
/// and a parser that refuses source its reference accepts is a fidelity
/// bug whether or not the formatter can currently produce it.
#[test]
fn an_indented_label_survives_formatting() {
    let src = "        start: dex\n        bne start\n";
    let before = asm198x::assemble_ca65_huc6280(src)
        .expect("assembles")
        .bytes;
    let formatted = asm198x::format_ca65_huc6280(src).expect("formats");
    let after = asm198x::assemble_ca65_huc6280(&formatted)
        .unwrap_or_else(|e| panic!("the formatted source assembles: {e:?}\n{formatted}"))
        .bytes;
    assert_eq!(
        before, after,
        "formatting changed the program:\n{formatted}"
    );
}
