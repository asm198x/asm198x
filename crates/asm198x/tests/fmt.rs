//! U5 formatter round-trip tests (AE7). `format_pasmo` parses source into the
//! semantic AST and emits canonical same-dialect source; the result must
//! assemble byte-identical to the input, be idempotent, and preserve operand
//! spelling and comments.

use asm198x::{
    assemble_1802, assemble_i8080, assemble_lwasm, assemble_m6800, assemble_pasmo, assemble_rgbasm,
    assemble_scmp, assemble_sjasmplus, format_1802, format_i8080, format_lwasm, format_m6800,
    format_pasmo, format_rgbasm, format_scmp, format_sjasmplus,
};

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

/// A representative Intel-8080 program: origin, colon labels, a same-line label
/// with an instruction, `equ` constants (one whose name collides with a
/// mnemonic), radix-suffixed numbers, data directives, and comments. The first
/// fixed-slot CPU to route through the AST formatter (U6).
const PROG_8080: &str = "\
; a small 8080 routine
        org 100h
start:  mvi a,5        ; load five
        mov b,a
loop:   dcr b
        jnz loop
        mvi m,0ffh     ; fill
in      equ 0feh
        out in
        db 1, 2, 3
        dw 1234h
        ret
";

#[test]
fn fmt_i8080_reassembles_byte_identical() {
    let original = assemble_i8080(PROG_8080).expect("assembles").bytes;
    let formatted = format_i8080(PROG_8080).expect("formats");
    let reassembled = assemble_i8080(&formatted)
        .unwrap_or_else(|e| panic!("formatted 8080 must assemble: {e:?}\n---\n{formatted}"))
        .bytes;
    assert_eq!(
        original, reassembled,
        "8080 round-trips byte-identical\n---\n{formatted}"
    );
    // Idempotent.
    assert_eq!(
        formatted,
        format_i8080(&formatted).expect("formats"),
        "8080 fmt is idempotent"
    );
}

#[test]
fn fmt_i8080_equ_label_takes_no_colon() {
    // The mirror of the Z80 `in: equ` case: Intel `equ` must emit WITHOUT a
    // colon, since a colon'd `in:` re-parses `equ` as a mnemonic and fails.
    let formatted = format_i8080("in equ 0feh\n        out in\n").expect("formats");
    assert!(
        formatted.lines().any(|l| l.contains("in equ 0feh")),
        "equ label keeps no colon:\n{formatted}"
    );
    assert!(
        !formatted.contains("in: equ"),
        "no colon on the 8080 equ label:\n{formatted}"
    );
    // And it reassembles (the round-trip the colon would have broken).
    assert!(
        assemble_i8080(&formatted).is_ok(),
        "no-colon equ reassembles:\n{formatted}"
    );
}

/// A representative Motorola-6800 program: origin, colon labels, a same-line
/// label with an instruction, an `equ` constant, `#`/direct/extended/indexed
/// modes, a `>` force, a relative branch, data directives, and comments.
const PROG_6800: &str = "\
; a small 6800 routine
        org $0100
start:  ldaa #$42     ; load
        staa $50
        ldx #$1234
        ldaa >$50      ; forced extended
loop:   deca
        bne loop
count   equ 3
        fcb $01, $02, \"AB\"
        fdb $1234
        rts
";

#[test]
fn fmt_m6800_reassembles_byte_identical() {
    let original = assemble_m6800(PROG_6800).expect("assembles").bytes;
    let formatted = format_m6800(PROG_6800).expect("formats");
    let reassembled = assemble_m6800(&formatted)
        .unwrap_or_else(|e| panic!("formatted 6800 must assemble: {e:?}\n---\n{formatted}"))
        .bytes;
    assert_eq!(
        original, reassembled,
        "6800 round-trips byte-identical\n---\n{formatted}"
    );
    assert_eq!(
        formatted,
        format_m6800(&formatted).expect("formats"),
        "6800 fmt is idempotent"
    );
    // The `equ` label emits with no colon (Motorola, like the 8080).
    assert!(
        formatted.lines().any(|l| l.contains("count equ 3")),
        "equ label keeps no colon:\n{formatted}"
    );
}

/// A representative CDP1802 program: origin, colon labels, a same-line label
/// with an instruction, an `equ`, register/immediate/inherent ops, a short and
/// a long branch, data directives, and comments.
const PROG_1802: &str = "\
; a small 1802 routine
        org 1000h
start:  ldi 42h        ; load immediate
        phi 3
loop:   dec 3
        bnz loop
        lbr start
delay   equ 5
        db 1, 2, \"AB\"
        dw 1234h
        idl
";

#[test]
fn fmt_1802_reassembles_byte_identical() {
    let original = assemble_1802(PROG_1802).expect("assembles").bytes;
    let formatted = format_1802(PROG_1802).expect("formats");
    let reassembled = assemble_1802(&formatted)
        .unwrap_or_else(|e| panic!("formatted 1802 must assemble: {e:?}\n---\n{formatted}"))
        .bytes;
    assert_eq!(
        original, reassembled,
        "1802 round-trips byte-identical\n---\n{formatted}"
    );
    assert_eq!(
        formatted,
        format_1802(&formatted).expect("formats"),
        "1802 fmt is idempotent"
    );
    assert!(
        formatted.lines().any(|l| l.contains("delay equ 5")),
        "equ label keeps no colon:\n{formatted}"
    );
}

/// A representative SC/MP program: origin, colon labels, a same-line label with
/// an instruction, an `equ`, inherent / pointer-exchange / memory-reference /
/// immediate forms, data directives, and comments.
const PROG_SCMP: &str = "\
; a small SC/MP routine
        org 0x0100
start:  ldi 0x42       ; load immediate
        st 5(1)
loop:   ld @1(2)
        jmp 0(0)
mask    equ 0x0f
        ani mask
        db 1, 2, \"AB\"
        nop
";

#[test]
fn fmt_scmp_reassembles_byte_identical() {
    let original = assemble_scmp(PROG_SCMP).expect("assembles").bytes;
    let formatted = format_scmp(PROG_SCMP).expect("formats");
    let reassembled = assemble_scmp(&formatted)
        .unwrap_or_else(|e| panic!("formatted SC/MP must assemble: {e:?}\n---\n{formatted}"))
        .bytes;
    assert_eq!(
        original, reassembled,
        "SC/MP round-trips byte-identical\n---\n{formatted}"
    );
    assert_eq!(
        formatted,
        format_scmp(&formatted).expect("formats"),
        "SC/MP fmt is idempotent"
    );
    assert!(
        formatted.lines().any(|l| l.contains("mask equ 0x0f")),
        "equ label keeps no colon:\n{formatted}"
    );
}

/// A representative rgbasm (Game Boy) program: a `SECTION` with an origin, a
/// global label, scoped `.local` labels reused across two globals, an `equ`,
/// SM83-specific operand syntax (`[hl+]`), data, and comments.
const PROG_RGBASM: &str = "\
; a small Game Boy routine
SECTION \"main\", ROM0[$0]
start:
        ld a, $05      ; init
        ld hl, buffer
.loop:
        ld [hl+], a    ; store
        dec a
        jr nz, .loop
second:
        ld b, $02
.loop:
        dec b
        jr nz, .loop
buffer  equ $c000
        ret
";

#[test]
fn fmt_rgbasm_reassembles_byte_identical() {
    let original = assemble_rgbasm(PROG_RGBASM).expect("assembles").bytes;
    let formatted = format_rgbasm(PROG_RGBASM).expect("formats");
    let reassembled = assemble_rgbasm(&formatted)
        .unwrap_or_else(|e| panic!("formatted rgbasm must assemble: {e:?}\n---\n{formatted}"))
        .bytes;
    assert_eq!(
        original, reassembled,
        "rgbasm round-trips byte-identical\n---\n{formatted}"
    );
    assert_eq!(
        formatted,
        format_rgbasm(&formatted).expect("formats"),
        "rgbasm fmt is idempotent"
    );
    // The SECTION directive is preserved, the scoped local re-emits in source
    // form (`.loop`, not the qualified `start.loop`), and equ takes no colon.
    assert!(
        formatted.contains("SECTION \"main\", ROM0[$0]"),
        "SECTION preserved:\n{formatted}"
    );
    assert!(
        formatted.lines().any(|l| l.trim() == ".loop:"),
        "source-form local preserved:\n{formatted}"
    );
    assert!(
        formatted.lines().any(|l| l.contains("buffer equ $c000")),
        "equ label keeps no colon:\n{formatted}"
    );
}

#[test]
fn fmt_rgbasm_preserves_address_less_section() {
    // A SECTION with no address emits no bytes but must survive formatting (the
    // emit path for a directive that lowers to nothing).
    let src = "SECTION \"ram\", WRAM0\n nop\n";
    let formatted = format_rgbasm(src).expect("formats");
    assert!(
        formatted.contains("SECTION \"ram\", WRAM0"),
        "address-less SECTION preserved:\n{formatted}"
    );
    let original = assemble_rgbasm(src).expect("assembles").bytes;
    let reassembled = assemble_rgbasm(&formatted).expect("reassembles").bytes;
    assert_eq!(
        original, reassembled,
        "still byte-identical\n---\n{formatted}"
    );
}

/// A representative 6809 program: a `*` whole-line comment, an origin, column-0
/// labels (no colon), an `equ`, immediate / direct / **indexed** (computed
/// postbyte) instructions, a branch, data, and a `;` trailing comment. The 6809
/// is the first computed-operand CPU through the formatter — its instructions
/// carry `Item::Encoded` and round-trip via the node's verbatim source.
const PROG_6809: &str = "\
* a small 6809 routine
        org $2000
start   lda #$05      ; init
        ldb $20
loop    sta ,x+       ; store, autoinc
        decb
        bne loop
        ldx #$1234
count   equ 3
        fcb $01,$02
        fdb $1234
        rts
";

#[test]
fn fmt_lwasm_reassembles_byte_identical() {
    let original = assemble_lwasm(PROG_6809).expect("assembles").bytes;
    let formatted = format_lwasm(PROG_6809).expect("formats");
    let reassembled = assemble_lwasm(&formatted)
        .unwrap_or_else(|e| panic!("formatted 6809 must assemble: {e:?}\n---\n{formatted}"))
        .bytes;
    assert_eq!(
        original, reassembled,
        "6809 round-trips byte-identical (computed operands via Item::Encoded)\n---\n{formatted}"
    );
    assert_eq!(
        formatted,
        format_lwasm(&formatted).expect("formats"),
        "6809 fmt is idempotent"
    );
    // The indexed operand's spelling is preserved verbatim, and the `*` comment
    // survives.
    assert!(
        formatted.lines().any(|l| l.contains("sta ,x+")),
        "computed-operand spelling preserved:\n{formatted}"
    );
    assert!(
        formatted.contains("* a small 6809 routine"),
        "whole-line `*` comment preserved:\n{formatted}"
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
