//! U5 formatter round-trip tests (AE7). `format_pasmo` parses source into the
//! semantic AST and emits canonical same-dialect source; the result must
//! assemble byte-identical to the input, be idempotent, and preserve operand
//! spelling and comments.

use asm198x::{assemble_pasmo, assemble_sjasmplus, format_pasmo, format_sjasmplus};

/// A representative pasmo program: an origin, labels, a local, instructions,
/// a forward `equ`, and leading + trailing comments.
const PROG: &str = "\
; a small routine
        org $8000
start:
        ld a, $05      ; load five
        ld hl, buffer
.loop:
        ld (hl), a     ; store the byte
        inc hl
        djnz .loop
buffer  equ $9000
        ret
";

#[test]
fn fmt_reassembles_byte_identical() {
    let original = assemble_pasmo(PROG).expect("assembles").bytes;
    let formatted = format_pasmo(PROG).expect("formats");
    let reassembled = assemble_pasmo(&formatted)
        .unwrap_or_else(|e| panic!("formatted source must assemble: {e:?}\n---\n{formatted}"))
        .bytes;
    assert_eq!(
        original, reassembled,
        "formatted source assembles identically\n---\n{formatted}"
    );
}

/// A broader program: data directives, indexed `(ix+d)`, bit ops, conditional
/// and unconditional jumps, an indirect load, and `halt`.
const PROG2: &str = "\
        org $0000
vectors:
        defb $01, $02, $03
        defw $1234, table
table:
        ld ix, $4000
        ld (ix+5), $ff   ; indexed store
        bit 7, (ix+0)
loop:
        jr nz, loop
        jp done
done:
        ld a, (table)
        cp $10
        jr c, loop
        halt
";

#[test]
fn fmt_reassembles_a_broader_program() {
    let original = assemble_pasmo(PROG2).expect("assembles").bytes;
    let formatted = format_pasmo(PROG2).expect("formats");
    let reassembled = assemble_pasmo(&formatted)
        .unwrap_or_else(|e| panic!("formatted source must assemble: {e:?}\n---\n{formatted}"))
        .bytes;
    assert_eq!(
        original, reassembled,
        "broader program round-trips\n---\n{formatted}"
    );
    // And it stays idempotent.
    assert_eq!(formatted, format_pasmo(&formatted).expect("formats"));
}

#[test]
fn fmt_is_idempotent() {
    let once = format_pasmo(PROG).expect("formats");
    let twice = format_pasmo(&once).expect("formats");
    assert_eq!(once, twice, "fmt(fmt(x)) == fmt(x)\n---\n{once}");
}

#[test]
fn fmt_preserves_operand_spelling() {
    let formatted = format_pasmo("        ld a, $0A\n").expect("formats");
    assert!(
        formatted.contains("$0A"),
        "hex spelling preserved, not normalised to 10:\n{formatted}"
    );
}

#[test]
fn fmt_equ_label_colliding_with_mnemonic_round_trips() {
    // `in` is a Z80 mnemonic; used as an equ label it must keep its colon on
    // emit, or the reformatted `in equ $fe` re-parses `in` as an instruction and
    // fails to reassemble. (Regression for the review's confirmed defect.)
    let src = "in: equ $fe\n        ld a, in\n";
    let original = assemble_pasmo(src).expect("assembles").bytes;
    let formatted = format_pasmo(src).expect("formats");
    let reassembled = assemble_pasmo(&formatted)
        .unwrap_or_else(|e| panic!("colliding equ label must round-trip: {e:?}\n---\n{formatted}"))
        .bytes;
    assert_eq!(original, reassembled);
    assert_eq!(
        formatted,
        format_pasmo(&formatted).expect("formats"),
        "idempotent"
    );
}

#[test]
fn fmt_preserves_trailing_comment_block() {
    // Comments after the last node must not be dropped (regression).
    let formatted =
        format_pasmo("        nop\n; end of file\n; really the end\n").expect("formats");
    assert!(
        formatted.contains("; end of file") && formatted.contains("; really the end"),
        "trailing comment block preserved:\n{formatted}"
    );
}

#[test]
fn fmt_preserves_comment_only_input() {
    let formatted = format_pasmo("; just a comment\n").expect("formats");
    assert!(
        formatted.contains("; just a comment"),
        "comment-only input preserved:\n{formatted}"
    );
}

#[test]
fn fmt_sjasmplus_reassembles_byte_identical() {
    // sjasmplus-specific: `//` comments and scoped `.loop` locals reused under
    // two globals — the axis the AST refactor exists to handle.
    let src = "\
first:
        ld b, 2
.loop:
        djnz .loop   // inner loop
second:
        ld b, 3
.loop:
        djnz .loop
";
    let original = assemble_sjasmplus(src).expect("assembles").bytes;
    let formatted = format_sjasmplus(src).expect("formats");
    let reassembled = assemble_sjasmplus(&formatted)
        .unwrap_or_else(|e| panic!("sjasmplus fmt must round-trip: {e:?}\n---\n{formatted}"))
        .bytes;
    assert_eq!(
        original, reassembled,
        "sjasmplus round-trips\n---\n{formatted}"
    );
    assert_eq!(
        formatted,
        format_sjasmplus(&formatted).expect("formats"),
        "idempotent"
    );
    // The scoped local re-emits in source form (`.loop`), not the qualified name.
    assert!(
        formatted.contains(".loop"),
        "source-form local preserved:\n{formatted}"
    );
}

#[test]
fn fmt_keeps_comments_in_position() {
    let formatted = format_pasmo("; header\n        nop   ; trailing\n").expect("formats");
    assert!(formatted.contains("; header"), "leading comment kept");
    assert!(
        formatted
            .lines()
            .any(|l| l.contains("nop") && l.contains("; trailing")),
        "trailing comment stays on its operation's line:\n{formatted}"
    );
}
