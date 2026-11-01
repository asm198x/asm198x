//! The 65816 through the ca65 walk, as `ca65 --cpu 65816` assembles it and
//! links it flat. These began as the old 65816 reader's own tests; the
//! reader is gone, and Asm198x's recorded verdicts and Lang198x's lowering
//! tests carry the rest.

use asm198x::assemble_ca65_816 as asm;

fn bytes(src: &str) -> Vec<u8> {
    asm(src).unwrap_or_else(|e| panic!("{src}: {e:?}")).bytes
}

#[test]
fn mx_width_follows_directives() {
    // .a16 makes the accumulator immediate 16-bit regardless of value.
    assert_eq!(bytes(".a8\n lda #$12\n"), vec![0xA9, 0x12]);
    assert_eq!(bytes(".a16\n lda #$12\n"), vec![0xA9, 0x12, 0x00]);
    // .i16 widens the index immediate independently.
    assert_eq!(bytes(".i16\n ldx #1\n"), vec![0xA2, 0x01, 0x00]);
    // rep/sep immediates are always 8-bit.
    assert_eq!(bytes(".a16\n .i16\n rep #$30\n"), vec![0xC2, 0x30]);
}

#[test]
fn dword_dbyt_asciiz_match_reference_bytes() {
    // Byte-for-byte against `ca65 --cpu 65816` + ld65:
    //   .dword $12345678 -> 78 56 34 12 (32-bit little-endian)
    //   .dbyt  $1234     -> 12 34       (16-bit big-endian)
    //   .asciiz "hi"     -> 68 69 00
    assert_eq!(
        bytes(" .dword $12345678\n .dbyt $1234\n .asciiz \"hi\"\n"),
        vec![0x78, 0x56, 0x34, 0x12, 0x12, 0x34, 0x68, 0x69, 0x00]
    );
}

#[test]
fn size_picks_smallest_with_forces() {
    assert_eq!(bytes(" lda $12\n"), vec![0xA5, 0x12]); // dp
    assert_eq!(bytes(" lda $1234\n"), vec![0xAD, 0x34, 0x12]); // abs
    assert_eq!(bytes(" lda $123456\n"), vec![0xAF, 0x56, 0x34, 0x12]); // long
    assert_eq!(bytes("v = $34\n lda z:v\n"), vec![0xA5, 0x34]); // forced dp (fits a byte)
    assert_eq!(bytes(" lda a:$12\n"), vec![0xAD, 0x12, 0x00]); // forced abs
    assert_eq!(bytes(" lda f:$12\n"), vec![0xAF, 0x12, 0x00, 0x00]); // forced long
}

#[test]
fn new_addressing_modes() {
    assert_eq!(bytes(" lda [$12]\n"), vec![0xA7, 0x12]); // [dp]
    assert_eq!(bytes(" lda [$12],y\n"), vec![0xB7, 0x12]); // [dp],y
    assert_eq!(bytes(" lda 3,s\n"), vec![0xA3, 0x03]); // stack rel
    assert_eq!(bytes(" lda (3,s),y\n"), vec![0xB3, 0x03]); // (sr),y
    assert_eq!(bytes(" lda ($12)\n"), vec![0xB2, 0x12]); // (dp)
    assert_eq!(bytes(" lda $123456,x\n"), vec![0xBF, 0x56, 0x34, 0x12]); // long,x
}

#[test]
fn jumps_and_long_calls() {
    assert_eq!(bytes(" jml $123456\n"), vec![0x5C, 0x56, 0x34, 0x12]);
    assert_eq!(bytes(" jsl $123456\n"), vec![0x22, 0x56, 0x34, 0x12]);
    assert_eq!(bytes(" jmp [$1234]\n"), vec![0xDC, 0x34, 0x12]);
    assert_eq!(bytes(" jmp ($1234,x)\n"), vec![0x7C, 0x34, 0x12]);
    // A 16-bit (or forward) operand on the long-only jsl widens to long.
    assert_eq!(
        bytes("sub = $1234\n jsl sub\n"),
        vec![0x22, 0x34, 0x12, 0x00]
    );
}

#[test]
fn brl_relative_is_16_bit() {
    // brl to self: offset = -3 (the instruction is 3 bytes).
    assert_eq!(bytes(".org $1000\nl: brl l\n"), vec![0x82, 0xFD, 0xFF]);
}

#[test]
fn block_moves_cop_wdm_and_bank_byte() {
    // mvn src,dest -> opcode, dest-bank, src-bank (order swapped).
    assert_eq!(bytes(" mvn #$7e,#$7f\n"), vec![0x54, 0x7F, 0x7E]);
    assert_eq!(bytes(" mvp #$00,#$01\n"), vec![0x44, 0x01, 0x00]);
    // Bare addresses contribute their bank byte (bits 16-23). `s` is a
    // register on the 65816, so it cannot name one.
    assert!(asm("s = $7e0000\n").is_err());
    assert_eq!(
        bytes("src = $7e0000\nd = $7f0000\n mvn src,d\n"),
        vec![0x54, 0x7F, 0x7E]
    );
    // cop/wdm take a bare signature byte (no #).
    assert_eq!(bytes(" cop $12\n"), vec![0x02, 0x12]);
    assert_eq!(bytes(" wdm $34\n"), vec![0x42, 0x34]);
    // The `^` bank-byte operator (and 24-bit constants).
    assert_eq!(bytes("p = $7e1234\n lda #^p\n"), vec![0xA9, 0x7E]);
    assert_eq!(bytes("p = $7e1234\n lda #>p\n"), vec![0xA9, 0x12]);
    assert_eq!(bytes("p = $7e1234\n lda #<p\n"), vec![0xA9, 0x34]);
}

#[test]
fn standalone_and_stz() {
    assert_eq!(bytes(" xce\n"), vec![0xFB]);
    assert_eq!(bytes(" xba\n"), vec![0xEB]);
    assert_eq!(bytes(" tcd\n"), vec![0x5B]);
    assert_eq!(bytes(" phb\n"), vec![0x8B]);
    assert_eq!(bytes(" inc a\n"), vec![0x1A]);
    assert_eq!(bytes(" stz $12\n"), vec![0x64, 0x12]);
    assert_eq!(bytes(" stz $1234,x\n"), vec![0x9E, 0x34, 0x12]);
    assert_eq!(bytes(" bra l\nl: rts\n"), vec![0x80, 0x00, 0x60]);
}

// ----- Macros (#93) -------------------------------------------------
//
// The grammar is the ca65 family's, in `ca65_flat`, so these cover
// ca65-huc6280 and the NES ca65 too — cc65 ships one assembler and the CPU
// is a flag. Every expectation is a byte string ca65 V2.18 produced for the
// same source.

/// The definition, the invocation, a parameter, and the `.mac` spelling.
/// The keyword is case-insensitive; the name is not.
#[test]
fn macros_expand() {
    assert_eq!(
        bytes(".macro nop2\n nop\n nop\n.endmacro\n nop2\n"),
        vec![0xEA, 0xEA]
    );
    assert_eq!(
        bytes(".macro ldav v\n lda #v\n.endmacro\n ldav 5\n"),
        vec![0xA9, 0x05]
    );
    assert_eq!(bytes(".mac nop2\n nop\n.endmac\n nop2\n"), vec![0xEA]);
    assert_eq!(bytes(".MACRO n1\n nop\n.ENDMACRO\n n1\n"), vec![0xEA]);
    asm(".macro mac\n nop\n.endmacro\n MAC\n").expect_err("MAC is not mac");
}

/// Substitution is textual but not naive: word-bounded, string-safe, and
/// before the expression is evaluated.
#[test]
fn substitution_respects_words_and_strings() {
    assert_eq!(
        bytes("val = 7\n.macro m1 v\n lda #v\n lda val\n.byte \"v\"\n.endmacro\n m1 9\n"),
        vec![0xA9, 0x09, 0xA5, 0x07, 0x76]
    );
    assert_eq!(
        bytes(".macro m1 v\n lda #v*2\n.endmacro\n m1 5\n"),
        vec![0xA9, 0x0A]
    );
}

/// `.local` is the only thing that scopes a label to an expansion. A plain
/// label in the same place is global, and the second expansion collides —
/// `Symbol 'spin' is already defined`, which we must also refuse.
#[test]
fn only_dot_local_scopes_a_label_to_its_expansion() {
    assert_eq!(
        bytes(".macro delay\n.local spin\nspin: dex\n bne spin\n.endmacro\n delay\n delay\n"),
        vec![0xCA, 0xD0, 0xFD, 0xCA, 0xD0, 0xFD]
    );
    asm(".macro delay\nspin: dex\n bne spin\n.endmacro\n delay\n delay\n")
        .expect_err("a plain label is global and collides");
}

/// ca65's arity posture is neither of the other two dialects': too many is
/// an error, too few is not. A parameter that no emitting line reaches may
/// simply be omitted.
#[test]
fn too_many_arguments_is_an_error_but_too_few_is_not() {
    let long = asm(".macro m1 v\n lda #v\n.endmacro\n m1 1, 2\n").expect_err("too many parameters");
    assert!(long.message.contains("takes 1 arguments"), "{long:?}");
    assert_eq!(
        bytes(".macro m1 v, w\n lda #v\n.endmacro\n m1 9\n"),
        vec![0xA9, 0x09]
    );
    asm(".macro m1 v, w\n lda #v\n ldx #w\n.endmacro\n m1 9\n").expect_err("`ldx #` has no value");
}

/// Macros compose, may invoke one defined later, and take a label in front
/// of the invocation — which binds to the expansion's first address.
#[test]
fn macros_compose_and_take_a_label() {
    assert_eq!(
        bytes(
            ".macro inner v\n lda #v\n.endmacro\n.macro outer v\n inner v\n inner v+1\n.endmacro\n outer 3\n"
        ),
        vec![0xA9, 0x03, 0xA9, 0x04]
    );
    assert_eq!(
        bytes(".macro outer\n inner\n.endmacro\n.macro inner\n nop\n.endmacro\n outer\n"),
        vec![0xEA]
    );
    assert_eq!(
        bytes(".macro m1 v\n lda #v\n.endmacro\nlbl: m1 9\n lda lbl\n"),
        vec![0xA9, 0x09, 0xAD, 0x00, 0x00]
    );
}

/// A self-recursive macro is refused with the macro named. ca65 refuses it
/// too (`Too many nested macro expansions`) — the one reference measured so
/// far that does not segfault.
#[test]
fn runaway_recursion_is_caught() {
    let err = asm(".macro forever\n forever\n.endmacro\n forever\n").expect_err("recursive");
    assert!(err.message.contains("forever"), "{err:?}");
}

/// The formatter lays source out; it does not rewrite programs, so it gives
/// the macro back rather than the lines it expands to.
///
/// This used to assert the opposite — that the walk *refused* `.macro` —
/// which was true and pinned a limitation rather than a property. The
/// definition and the invocation are copied verbatim now, so what is worth
/// pinning is that the copy is faithful. See
/// `decisions/macro-expansion-framework.md`.
#[test]
fn formatting_does_not_expand() {
    let src = ".macro ldav v\n lda #v\n.endmacro\nstart: ldav 5\n rts\n";
    let out = asm198x::format_ca65_816(src).expect("the walk copies a definition");

    // The macro survives as a macro. If it were expanded, `.macro` would be
    // gone and `lda #5` would be in its place.
    assert!(out.contains(".macro ldav v"), "{out}");
    assert!(out.contains(".endmacro"), "{out}");
    assert!(out.contains("ldav 5"), "{out}");
    assert!(!out.contains("lda #5"), "expanded into the output:\n{out}");
}

/// Formatting a macro changes the layout and not the program.
///
/// The round trip is the property that makes the formatter safe to run on
/// source you have not read, and a macro is where it is easiest to break:
/// a body is a template, so a walk that drops one line of it still produces
/// something that looks like source.
#[test]
fn a_formatted_macro_assembles_to_the_same_bytes() {
    let src = ".macro ldav v\n lda #v\n.endmacro\nstart: ldav 5\n rts\n";
    let before = asm(src).expect("assembles").bytes;
    let formatted = asm198x::format_ca65_816(src).expect("formats");
    let after = asm(&formatted)
        .expect("the formatted source assembles")
        .bytes;
    assert_eq!(
        before, after,
        "formatting changed the program:\n{formatted}"
    );

    // And formatting is idempotent, so a second run is a no-op.
    let again = asm198x::format_ca65_816(&formatted).expect("formats");
    assert_eq!(formatted, again);
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
    let flush = asm198x::assemble_ca65_816("start: dex\n bne start\n").expect("column 0");
    let indented =
        asm198x::assemble_ca65_816("        start: dex\n        bne start\n").expect("indented");
    assert_eq!(flush.bytes, indented.bytes);
}

/// A first word without a colon is not a label wherever it sits, so an
/// ordinary indented instruction is unaffected.
#[test]
fn an_indented_instruction_is_not_read_as_a_label() {
    let out = asm198x::assemble_ca65_816("        dex\n        dex\n").expect("two instructions");
    assert_eq!(out.bytes.len(), 2, "{out:?}");
}

/// The #186 round trip: a macro whose body defines a `.local`, formatted
/// and reassembled.
#[test]
fn a_formatted_macro_body_label_still_assembles() {
    let src = ".macro delay\n.local spin\nspin: dex\n bne spin\n.endmacro\n delay\n delay\n";
    let before = asm198x::assemble_ca65_816(src).expect("assembles").bytes;
    let formatted = asm198x::format_ca65_816(src).expect("formats");
    let after = asm198x::assemble_ca65_816(&formatted)
        .unwrap_or_else(|e| panic!("the formatted source assembles: {e:?}\n{formatted}"))
        .bytes;
    assert_eq!(
        before, after,
        "formatting changed the program:\n{formatted}"
    );
}
