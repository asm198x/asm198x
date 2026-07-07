//! U5 formatter round-trip tests (AE7). `format_pasmo` parses source into the
//! semantic AST and emits canonical same-dialect source; the result must
//! assemble byte-identical to the input, be idempotent, and preserve operand
//! spelling and comments.

use asm198x::{
    assemble_1802, assemble_2650, assemble_8048, assemble_acme, assemble_ca65, assemble_ca65_816,
    assemble_ca65_huc6280, assemble_cp1610, assemble_f8, assemble_i8080, assemble_lwasm,
    assemble_m6800, assemble_pasmo, assemble_pdp11, assemble_rgbasm, assemble_scmp,
    assemble_sjasmplus, assemble_tms7000, assemble_tms9900, assemble_vasm, assemble_z8000,
    assemble_z8001, format_1802, format_2650, format_8048, format_acme, format_ca65,
    format_ca65_816, format_ca65_huc6280, format_cp1610, format_f8, format_i8080, format_lwasm,
    format_m6800, format_pasmo, format_pdp11, format_rgbasm, format_scmp, format_sjasmplus,
    format_tms7000, format_tms9900, format_vasm, format_z8000, format_z8001,
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

/// A representative ACME (C64 6502) program: `*=` origin, a one-line guard, a
/// re-alignable run of `name = value` constants, blank-line grouping, a label +
/// instructions, and a multi-line `!if … {` … `} else { … }` block with comments.
/// The formatter is canonical reflow with constant-run alignment (see
/// `decisions/formatter-canonical-style.md`).
const PROG_ACME: &str = "\
*= $0801

; default the debug flag
!ifndef DEBUG { DEBUG = 0 }

; screen constants
SCREEN = $0400
BORDER = $d020
BG     = $d021

start:
        lda #$00
        sta BORDER
!if DEBUG = 1 {
        ; debug: flash the background
        lda #$01
        sta BG
} else {
        lda #$02
}
        rts
";

#[test]
fn fmt_acme_reassembles_byte_identical() {
    let original = assemble_acme(PROG_ACME).expect("assembles").bytes;
    let formatted = format_acme(PROG_ACME).expect("formats");
    let reassembled = assemble_acme(&formatted)
        .unwrap_or_else(|e| panic!("formatted acme must assemble: {e:?}\n---\n{formatted}"))
        .bytes;
    assert_eq!(
        original, reassembled,
        "acme round-trips byte-identical\n---\n{formatted}"
    );
    assert_eq!(
        formatted,
        format_acme(&formatted).expect("formats"),
        "acme fmt is idempotent\n---\n{formatted}"
    );
    // Constant run re-aligned: `BG` padded to the longest name (`SCREEN`/`BORDER`).
    assert!(
        formatted.lines().any(|l| l == "BG     = $d021"),
        "constant run re-aligned:\n{formatted}"
    );
    // One-line guard preserved.
    assert!(
        formatted.contains("!ifndef DEBUG { DEBUG = 0 }"),
        "one-line guard preserved:\n{formatted}"
    );
    // Conditional delimiters at column 0, body indented.
    assert!(
        formatted.lines().any(|l| l == "!if DEBUG = 1 {")
            && formatted.lines().any(|l| l == "} else {")
            && formatted.lines().any(|l| l == "        lda #$01"),
        "conditional block canonicalised:\n{formatted}"
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

/// A representative MCS-48 (8048) program: origin, a same-line colon label with
/// an instruction, an `equ` constant (Intel — no colon), immediate/register/
/// port operands, the computed `call`/`jmp` (the `Encoded` seam), data
/// directives, and comments. The 8048 is a fixed-slot straggler routed through
/// the AST formatter (0b).
const PROG_8048: &str = "\
; a small MCS-48 routine
        org 100h
start:  mov a,#42h     ; load
        add a,r0
        anl p2,#0fh
five    equ 5
        add a,#five
        outl p1,a
        call 200h
        jmp 7ffh
        db 1, 2, 3
        dw 1234h
        ret
";

#[test]
fn fmt_8048_reassembles_byte_identical() {
    let original = assemble_8048(PROG_8048).expect("assembles").bytes;
    let formatted = format_8048(PROG_8048).expect("formats");
    let reassembled = assemble_8048(&formatted)
        .unwrap_or_else(|e| panic!("formatted 8048 must assemble: {e:?}\n---\n{formatted}"))
        .bytes;
    assert_eq!(
        original, reassembled,
        "8048 round-trips byte-identical\n---\n{formatted}"
    );
    // Idempotent.
    assert_eq!(
        formatted,
        format_8048(&formatted).expect("formats"),
        "8048 fmt is idempotent"
    );
}

/// A representative Fairchild F8 program: origin, a same-line colon label with an
/// instruction, an `equ` constant (no colon), register/immediate ops, a 16-bit
/// big-endian `dci`/`jmp` address, and the relative branches (`br`/`br7` — the
/// `Encoded` seam). A fixed-slot straggler routed through the AST formatter (0b).
const PROG_F8: &str = "\
; a small F8 routine
        org 10h
start:  li 42h         ; load a
        as 1
        ns d
five    equ 5
        lis 5
        ni 0fh
        dci 1234h
        jmp 2000h
loop:   br loop
        br7 start
";

#[test]
fn fmt_f8_reassembles_byte_identical() {
    let original = assemble_f8(PROG_F8).expect("assembles").bytes;
    let formatted = format_f8(PROG_F8).expect("formats");
    let reassembled = assemble_f8(&formatted)
        .unwrap_or_else(|e| panic!("formatted F8 must assemble: {e:?}\n---\n{formatted}"))
        .bytes;
    assert_eq!(
        original, reassembled,
        "F8 round-trips byte-identical\n---\n{formatted}"
    );
    assert_eq!(
        formatted,
        format_f8(&formatted).expect("formats"),
        "F8 fmt is idempotent"
    );
}

/// A representative Signetics 2650 program: origin, a same-line colon label with
/// an instruction, an `equ` constant (no colon), the `mnemonic,reg` comma
/// syntax, immediate/absolute/indexed operands, and the relative + absolute
/// branches (the `Packed`/`Encoded` seams). A fixed-slot straggler routed
/// through the AST formatter (0b).
const PROG_2650: &str = "\
; a small 2650 routine
        org $0000
start:  lodi,r0 $42    ; load
        addi,r0 $05
five    equ 5
        lodi,r1 five
        loda,r0 $1234
        loda,r0 $1234,r3
loop:   bctr,eq loop
        bcta,un start
        halt
";

#[test]
fn fmt_2650_reassembles_byte_identical() {
    let original = assemble_2650(PROG_2650).expect("assembles").bytes;
    let formatted = format_2650(PROG_2650).expect("formats");
    let reassembled = assemble_2650(&formatted)
        .unwrap_or_else(|e| panic!("formatted 2650 must assemble: {e:?}\n---\n{formatted}"))
        .bytes;
    assert_eq!(
        original, reassembled,
        "2650 round-trips byte-identical\n---\n{formatted}"
    );
    assert_eq!(
        formatted,
        format_2650(&formatted).expect("formats"),
        "2650 fmt is idempotent"
    );
}

/// A representative TI TMS7000 program: origin, a same-line colon label with an
/// instruction, an `equ` constant (no colon), immediate (`%`), register,
/// peripheral (`p`), extended (`@`) and indirect (`*`) operands, a `djnz`
/// relative jump, `trap` (the one computed `Encoded` byte), and data directives.
/// A fixed-slot straggler routed through the AST formatter (0b).
const PROG_TMS7000: &str = "\
; a small TMS7000 routine
        org 1000h
start:  mov %42h,a     ; load immediate
        add r5,a
five    equ 5
        mov %five,r6
        movp p6,a
        lda @1234h
        call *r5
loop:   djnz r5,loop
        trap 23
        db 1, 2, 3
        dw 1234h
";

#[test]
fn fmt_tms7000_reassembles_byte_identical() {
    let original = assemble_tms7000(PROG_TMS7000).expect("assembles").bytes;
    let formatted = format_tms7000(PROG_TMS7000).expect("formats");
    let reassembled = assemble_tms7000(&formatted)
        .unwrap_or_else(|e| panic!("formatted TMS7000 must assemble: {e:?}\n---\n{formatted}"))
        .bytes;
    assert_eq!(
        original, reassembled,
        "TMS7000 round-trips byte-identical\n---\n{formatted}"
    );
    assert_eq!(
        formatted,
        format_tms7000(&formatted).expect("formats"),
        "TMS7000 fmt is idempotent"
    );
}

/// A representative ca65 65816 program that leans on the `.a8`/`.a16` **width
/// directives** — the crux of the migration. If the formatter dropped them, the
/// 16-bit `lda #$1234` would reassemble as an 8-bit immediate and the bytes
/// would diverge; keeping them as source-only nodes makes the round-trip hold.
/// Also covers a `=` constant, long addressing, and a long jump. (0b straggler.)
const PROG_65816: &str = "\
; a small 65816 routine
        .org $8000
        .a16
start:  lda #$1234     ; 16-bit immediate (needs .a16)
        .a8
        lda #$12       ; 8-bit immediate (needs .a8)
five = 5
        lda five
        lda $123456
        lda [$12],y
        jml $123456
        rts
";

#[test]
fn fmt_ca65_816_reassembles_byte_identical() {
    let original = assemble_ca65_816(PROG_65816).expect("assembles").bytes;
    let formatted = format_ca65_816(PROG_65816).expect("formats");
    let reassembled = assemble_ca65_816(&formatted)
        .unwrap_or_else(|e| panic!("formatted 65816 must assemble: {e:?}\n---\n{formatted}"))
        .bytes;
    assert_eq!(
        original, reassembled,
        "65816 round-trips byte-identical (width directives preserved)\n---\n{formatted}"
    );
    assert_eq!(
        formatted,
        format_ca65_816(&formatted).expect("formats"),
        "65816 fmt is idempotent"
    );
    // The width directives must survive — dropping them changes the bytes.
    assert!(
        formatted.contains(".a16") && formatted.contains(".a8"),
        "width directives preserved in the formatted output:\n{formatted}"
    );
}

/// A representative ca65 HuC6280 program: a `=` constant, inherited 6502/65C02
/// instructions, a HuC6280 block transfer, a `bbr` two-operand branch, and
/// leading + trailing comments. Exercises the 0b straggler migration.
const PROG_HUC6280: &str = "\
; a small pce routine
        .org $2000
mask = $55
start:  lda #$12       ; immediate
        sta $10        ; zero page
        stz $1234      ; 65C02, absolute
        bbr0 $10, start
        tst #mask, $20
        tii $1000, $2000, $0010
        rts
";

#[test]
fn fmt_ca65_huc6280_reassembles_byte_identical() {
    let original = assemble_ca65_huc6280(PROG_HUC6280)
        .expect("assembles")
        .bytes;
    let formatted = format_ca65_huc6280(PROG_HUC6280).expect("formats");
    let reassembled = assemble_ca65_huc6280(&formatted)
        .unwrap_or_else(|e| panic!("formatted huc6280 must assemble: {e:?}\n---\n{formatted}"))
        .bytes;
    assert_eq!(
        original, reassembled,
        "huc6280 round-trips byte-identical\n---\n{formatted}"
    );
    assert_eq!(
        formatted,
        format_ca65_huc6280(&formatted).expect("formats"),
        "huc6280 fmt is idempotent"
    );
}

/// A representative asl-syntax PDP-11 program: an origin, colon labels, both
/// constant spellings (`equ` and `=`), a spread of addressing modes, a
/// register move, a branch, a same-line label with a data directive, and
/// leading + trailing comments. The first field-packed CPU through the AST
/// formatter — its `Encoded` opcode words route through unchanged.
const PROG_PDP11: &str = "\
; a small pdp-11 routine
        org 0x1000
mask    equ 0x00ff
delta = 4
start:  mov #0x1234, r0   ; immediate
        mov (r2)+, -(r3)
        mov delta(r1), r5
        clr count
loop:   inc count
        sob r0, loop
        bne loop
        halt
count:  word 0
msg:    byte 0x48, 0x49
";

#[test]
fn fmt_pdp11_reassembles_byte_identical() {
    let original = assemble_pdp11(PROG_PDP11).expect("assembles").bytes;
    let formatted = format_pdp11(PROG_PDP11).expect("formats");
    let reassembled = assemble_pdp11(&formatted)
        .unwrap_or_else(|e| panic!("formatted pdp11 must assemble: {e:?}\n---\n{formatted}"))
        .bytes;
    assert_eq!(
        original, reassembled,
        "pdp11 round-trips byte-identical\n---\n{formatted}"
    );
    assert_eq!(
        formatted,
        format_pdp11(&formatted).expect("formats"),
        "pdp11 fmt is idempotent"
    );
}

/// A representative asl-syntax TMS9900 program: an origin, colon labels, both
/// constant spellings (`equ` and `=`), a spread of formats and general-
/// addressing modes, a symbolic-address instruction, a workspace-context op,
/// a same-line data-directive label, and leading + trailing comments.
const PROG_TMS9900: &str = "\
; a small tms9900 routine
        org 0100h
mask    equ 00ffh
delta = 4
start:  li r0, 0abcdh     ; immediate
        mov r1, r2
        mov @0300h(r4), r5
        clr r3
loop:   dec r0
        jne loop
        bl @sub
        jmp start
sub:    b *r11
count:  word 0
";

#[test]
fn fmt_tms9900_reassembles_byte_identical() {
    let original = assemble_tms9900(PROG_TMS9900).expect("assembles").bytes;
    let formatted = format_tms9900(PROG_TMS9900).expect("formats");
    let reassembled = assemble_tms9900(&formatted)
        .unwrap_or_else(|e| panic!("formatted tms9900 must assemble: {e:?}\n---\n{formatted}"))
        .bytes;
    assert_eq!(
        original, reassembled,
        "tms9900 round-trips byte-identical\n---\n{formatted}"
    );
    assert_eq!(
        formatted,
        format_tms9900(&formatted).expect("formats"),
        "tms9900 fmt is idempotent"
    );
}

/// A representative asl-syntax CP1610 program: an origin, colon labels, both
/// constant spellings (`equ` and `=`), register ops, a `SDBD`+immediate pair
/// (the two-decle state-threading crux — the formatter must keep the `sdbd`
/// line in place or the following immediate reassembles at the wrong width), a
/// relative branch, and leading + trailing comments.
const PROG_CP1610: &str = "\
; a small cp1610 routine
        org 0
mask    equ 00ffh
delta = 4
top:    movr r0, r1
        incr r2
        sdbd
        mvii 1234h, r0    ; two-decle immediate (needs sdbd)
        addr r2, r3
loop:   decr r0
        bneq loop
        b top
";

#[test]
fn fmt_cp1610_reassembles_byte_identical() {
    let original = assemble_cp1610(PROG_CP1610).expect("assembles").bytes;
    let formatted = format_cp1610(PROG_CP1610).expect("formats");
    let reassembled = assemble_cp1610(&formatted)
        .unwrap_or_else(|e| panic!("formatted cp1610 must assemble: {e:?}\n---\n{formatted}"))
        .bytes;
    assert_eq!(
        original, reassembled,
        "cp1610 round-trips byte-identical (sdbd state preserved)\n---\n{formatted}"
    );
    assert_eq!(
        formatted,
        format_cp1610(&formatted).expect("formats"),
        "cp1610 fmt is idempotent"
    );
    // The sdbd prefix must survive in place — dropping it changes the bytes.
    assert!(
        formatted.contains("sdbd"),
        "sdbd prefix preserved:\n{formatted}"
    );
}

/// A representative asl-syntax Z8000 (non-segmented Z8002) program: an origin,
/// colon labels, both constant spellings (`equ` and `=`), the dyadic family
/// across register / immediate / indirect / indexed modes, byte and long ops,
/// a conditional jump, and leading + trailing comments.
const PROG_Z8000: &str = "\
; a small z8002 routine
        org 0
mask    equ 00ffh
delta = 4
start:  ld r1, r2
        add r1, #1234h    ; immediate
        ld r3, 2000h(r4)
        ldl rr2, #12345678h
        addb rl1, rl2
loop:   dec r1
        jp nz, loop
        halt
";

#[test]
fn fmt_z8000_reassembles_byte_identical() {
    let original = assemble_z8000(PROG_Z8000).expect("assembles").bytes;
    let formatted = format_z8000(PROG_Z8000).expect("formats");
    let reassembled = assemble_z8000(&formatted)
        .unwrap_or_else(|e| panic!("formatted z8000 must assemble: {e:?}\n---\n{formatted}"))
        .bytes;
    assert_eq!(
        original, reassembled,
        "z8000 round-trips byte-identical\n---\n{formatted}"
    );
    assert_eq!(
        formatted,
        format_z8000(&formatted).expect("formats"),
        "z8000 fmt is idempotent"
    );
}

/// A representative asl-syntax Z8001 (segmented) program: the `<<seg>>offset`
/// two-word direct and indexed addresses, `@RRn` long-pair pointers, and `LDA`
/// into a long pair — the operand shapes the segmented target adds.
const PROG_Z8001: &str = "\
; a small z8001 routine
        org 0
start:  ld r1, <<5>>1234h
        ld r1, <<5>>1234h(r3)
        ld r1, @rr2
        ldl rr2, <<5>>1234h
        lda rr2, <<5>>1234h
        jp <<5>>1234h
";

#[test]
fn fmt_z8001_reassembles_byte_identical() {
    let original = assemble_z8001(PROG_Z8001).expect("assembles").bytes;
    let formatted = format_z8001(PROG_Z8001).expect("formats");
    let reassembled = assemble_z8001(&formatted)
        .unwrap_or_else(|e| panic!("formatted z8001 must assemble: {e:?}\n---\n{formatted}"))
        .bytes;
    assert_eq!(
        original, reassembled,
        "z8001 round-trips byte-identical (segmented operands preserved)\n---\n{formatted}"
    );
    assert_eq!(
        formatted,
        format_z8001(&formatted).expect("formats"),
        "z8001 fmt is idempotent"
    );
    // The segmented operand syntax must survive verbatim.
    assert!(
        formatted.contains("<<5>>"),
        "segmented `<<seg>>` operands preserved:\n{formatted}"
    );
}

/// A representative Motorola-syntax 68000 (`vasm`) program: a `*`-column-0
/// comment, an `equ` constant, a global label opening a scope, a reused
/// `.local` label under it, instructions across several effective-address and
/// register-list operands, `dc.w`/`dc.l` data, and `;` trailing comments. The
/// first variable-length CISC dialect through the AST formatter.
const PROG_VASM: &str = "\
* a small 68000 routine
        lea     $dff000,a5     ; custom chip base
count   equ     100
start:
        move.l  (a5),d0
.loop:
        addq.w  #1,d0
        cmp.w   #count,d0
        bne.s   .loop
        movem.l d0-d3/a0-a1,-(sp)
        rts
data:
        dc.w    $0180,$0000
        dc.l    $deadbeef
";

#[test]
fn fmt_vasm_reassembles_byte_identical() {
    let original = assemble_vasm(PROG_VASM).expect("assembles").bytes;
    let formatted = format_vasm(PROG_VASM).expect("formats");
    let reassembled = assemble_vasm(&formatted)
        .unwrap_or_else(|e| panic!("formatted vasm must assemble: {e:?}\n---\n{formatted}"))
        .bytes;
    assert_eq!(
        original, reassembled,
        "vasm round-trips byte-identical\n---\n{formatted}"
    );
    assert_eq!(
        formatted,
        format_vasm(&formatted).expect("formats"),
        "vasm fmt is idempotent"
    );
    // The reused `.local` re-emits in source form, and the `*` comment survives.
    assert!(
        formatted.contains(".loop") && formatted.contains("* a small 68000 routine"),
        "local label + column-0 comment preserved:\n{formatted}"
    );
}

/// A representative ca65 NES program exercising every label form the AST must
/// round-trip: a `.segment` directive, a `=` constant, a named label, an
/// `@cheap` local, and an anonymous `:` label with a backward `:-` reference,
/// plus data directives and comments. The standalone assemble+link dialect, now
/// routed through the AST (the family-owned `Kind` payload).
const PROG_CA65: &str = "\
; a small nes routine
        .segment \"CODE\"
count = 4              ; a constant
start:
        ldx #0
:                      ; anonymous label
        inx
        cpx #count
        bne :-         ; backward anonymous reference
@done:
        rts
data:
        .byte 1, 2, 3
        .word start
";

#[test]
fn fmt_ca65_reassembles_byte_identical() {
    let original = assemble_ca65(PROG_CA65).expect("assembles").bytes;
    let formatted = format_ca65(PROG_CA65).expect("formats");
    let reassembled = assemble_ca65(&formatted)
        .unwrap_or_else(|e| panic!("formatted ca65 must assemble: {e:?}\n---\n{formatted}"))
        .bytes;
    assert_eq!(
        original, reassembled,
        "ca65 round-trips byte-identical (all label forms)\n---\n{formatted}"
    );
    assert_eq!(
        formatted,
        format_ca65(&formatted).expect("formats"),
        "ca65 fmt is idempotent"
    );
    // Every label form and the segment directive survive verbatim.
    assert!(
        formatted.contains(".segment \"CODE\"")
            && formatted.contains("@done:")
            && formatted.contains("bne :-"),
        "segment, cheap local, and anonymous reference preserved:\n{formatted}"
    );
}

/// U2 (language surface): `--fmt` renders an `INCLUDE` directive verbatim from
/// the node's source without ever opening the target — formatting succeeds
/// when the target does not exist — and stays idempotent (KTD1).
#[test]
fn fmt_sjasmplus_include_is_verbatim_and_never_opens_the_target() {
    let src = "\
; header
        org $8000
        include \"missing.inc\"   ; pulled in later
lbl:    INCLUDE <also-missing.inc>
        ld a,1
";
    let formatted = format_sjasmplus(src).expect("formats with the targets missing");
    assert!(
        formatted.contains("include \"missing.inc\""),
        "the directive text is verbatim:\n{formatted}"
    );
    assert!(
        formatted.contains("INCLUDE <also-missing.inc>"),
        "spelling (case, brackets) preserved:\n{formatted}"
    );
    assert!(
        formatted.contains("; pulled in later"),
        "the trailing comment survives:\n{formatted}"
    );
    assert_eq!(
        format_sjasmplus(&formatted).expect("formats again"),
        formatted,
        "idempotent"
    );
}

/// U2: formatted include-bearing source reassembles byte-identical through
/// the include-capable entry (the multi-file leg of the fmt round-trip bar).
#[test]
fn fmt_sjasmplus_include_reassembles_byte_identical() {
    use asm198x::source::MemoryLoader;
    let loader = || MemoryLoader::new().text("defs.inc", "VAL equ $2b\n");
    let src = "        org $8000\n        include \"defs.inc\"\n        ld a,VAL\n";
    let original = asm198x::assemble_sjasmplus_files(src, "main.asm", &loader())
        .expect("assembles")
        .bytes;
    let formatted = format_sjasmplus(src).expect("formats");
    let reassembled = asm198x::assemble_sjasmplus_files(&formatted, "main.asm", &loader())
        .unwrap_or_else(|e| panic!("formatted source must assemble: {e}\n---\n{formatted}"))
        .bytes;
    assert_eq!(original, reassembled, "round-trips:\n{formatted}");
}

/// U3 (language surface): `--fmt` renders an `INCBIN` directive verbatim —
/// spelling, quote form, and the offset/length tail untouched — without ever
/// opening the asset (formatting succeeds when it does not exist), and stays
/// idempotent (KTD1). The pasmo skin's plain form gets the same treatment.
#[test]
fn fmt_incbin_is_verbatim_and_never_opens_the_asset() {
    let src = "\
        org $8000
        incbin \"missing.bin\"   ; art, added later
art:    INCBIN <also-missing.bin>,2,3
        ld a,1
";
    let formatted = format_sjasmplus(src).expect("formats with the assets missing");
    assert!(
        formatted.contains("incbin \"missing.bin\""),
        "the directive text is verbatim:\n{formatted}"
    );
    assert!(
        formatted.contains("INCBIN <also-missing.bin>,2,3"),
        "spelling (case, brackets, tail) preserved:\n{formatted}"
    );
    assert!(
        formatted.contains("; art, added later"),
        "the trailing comment survives:\n{formatted}"
    );
    assert_eq!(
        format_sjasmplus(&formatted).expect("formats again"),
        formatted,
        "idempotent"
    );

    // pasmo: the plain form, same contract.
    let pasmo_src = "        org $8000\n        incbin \"missing.bin\"\n";
    let pasmo_fmt = format_pasmo(pasmo_src).expect("pasmo formats with the asset missing");
    assert!(
        pasmo_fmt.contains("incbin \"missing.bin\""),
        "verbatim under pasmo:\n{pasmo_fmt}"
    );
    assert_eq!(
        format_pasmo(&pasmo_fmt).expect("formats again"),
        pasmo_fmt,
        "idempotent under pasmo"
    );
}

/// U3: formatted incbin-bearing source reassembles byte-identical through the
/// multi-file entry (the incbin leg of the fmt round-trip bar).
#[test]
fn fmt_incbin_reassembles_byte_identical() {
    use asm198x::source::MemoryLoader;
    let loader = || MemoryLoader::new().binary("data.bin", (0x10..0x18).collect());
    let src = "        org $8000\n        incbin \"data.bin\",2,3\n        ld a,1\n";
    let original = asm198x::assemble_sjasmplus_files(src, "main.asm", &loader())
        .expect("assembles")
        .bytes;
    let formatted = format_sjasmplus(src).expect("formats");
    let reassembled = asm198x::assemble_sjasmplus_files(&formatted, "main.asm", &loader())
        .unwrap_or_else(|e| panic!("formatted source must assemble: {e}\n---\n{formatted}"))
        .bytes;
    assert_eq!(original, reassembled, "round-trips:\n{formatted}");
}
