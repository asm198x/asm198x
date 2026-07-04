//! U5 formatter round-trip tests (AE7). `format_pasmo` parses source into the
//! semantic AST and emits canonical same-dialect source; the result must
//! assemble byte-identical to the input, be idempotent, and preserve operand
//! spelling and comments.

use asm198x::{assemble_pasmo, format_pasmo};

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
    let original = assemble_pasmo(PROG).unwrap().bytes;
    let formatted = format_pasmo(PROG).unwrap();
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
    let original = assemble_pasmo(PROG2).unwrap().bytes;
    let formatted = format_pasmo(PROG2).unwrap();
    let reassembled = assemble_pasmo(&formatted)
        .unwrap_or_else(|e| panic!("formatted source must assemble: {e:?}\n---\n{formatted}"))
        .bytes;
    assert_eq!(
        original, reassembled,
        "broader program round-trips\n---\n{formatted}"
    );
    // And it stays idempotent.
    assert_eq!(formatted, format_pasmo(&formatted).unwrap());
}

#[test]
fn fmt_is_idempotent() {
    let once = format_pasmo(PROG).unwrap();
    let twice = format_pasmo(&once).unwrap();
    assert_eq!(once, twice, "fmt(fmt(x)) == fmt(x)\n---\n{once}");
}

#[test]
fn fmt_preserves_operand_spelling() {
    let formatted = format_pasmo("        ld a, $0A\n").unwrap();
    assert!(
        formatted.contains("$0A"),
        "hex spelling preserved, not normalised to 10:\n{formatted}"
    );
}

#[test]
fn fmt_keeps_comments_in_position() {
    let formatted = format_pasmo("; header\n        nop   ; trailing\n").unwrap();
    assert!(formatted.contains("; header"), "leading comment kept");
    assert!(
        formatted
            .lines()
            .any(|l| l.contains("nop") && l.contains("; trailing")),
        "trailing comment stays on its operation's line:\n{formatted}"
    );
}
