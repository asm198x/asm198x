//! Round-trip tests: assemble → disassemble → reassemble reproduces the bytes.
//!
//! These live here, not in `isa-disasm`, because they exercise both the
//! assembler (this crate) and the disassembler (the dependency-free
//! `isa-disasm` crate, re-exported from here). It's the payoff the
//! authored-spec architecture was justified by — see the umbrella
//! `asm198x-and-shared-isa-spec.md`.

use crate::{
    assemble_acme, assemble_pasmonext, assemble_vasm, listing_6502, listing_68000, listing_z80,
};

#[test]
fn round_trips_z80_through_pasmonext() {
    let source = "\
        org $8000\n\
        ld hl, $5800\n\
        ld a, $07\n\
        ld (hl), a\n\
        ldir\n\
        bit 7, (ix+5)\n\
        set 0, (iy-1)\n\
        add a, (ix+3)\n\
        ld (ix+2), $ff\n\
        jr $8000\n\
        ret\n";
    let original = assemble_pasmonext(source).expect("assemble");
    let listing = listing_z80(&original.bytes, original.origin, true);
    let re = assemble_pasmonext(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

#[test]
fn round_trips_z80n_opcodes() {
    let source = "\
        org $8000\n\
        swapnib\n\
        mul\n\
        add hl, a\n\
        add hl, $1234\n\
        nextreg $07, $02\n\
        push $abcd\n\
        ldirx\n";
    let original = assemble_pasmonext(source).expect("assemble");
    let listing = listing_z80(&original.bytes, original.origin, true);
    let re = assemble_pasmonext(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

#[test]
fn round_trips_6502_through_acme() {
    let source = "\
        *= $0800\n\
        start:  lda #$00\n\
                ldx #$08\n\
        loop:   sta $0400,x\n\
                lda $10\n\
                sta $d020\n\
                lda ($20),y\n\
                lda ($20,x)\n\
                jmp ($1234)\n\
                asl a\n\
                dex\n\
                bne loop\n\
                rts\n";
    let original = assemble_acme(source).expect("assemble");
    let listing = listing_6502(&original.bytes, original.origin);
    let re = assemble_acme(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

#[test]
fn round_trips_6502_low_address_absolute() {
    // A low-address absolute (e.g. data misread as code) must survive: the
    // disassembler emits 4-digit `$XXXX`, and acme's width rule keeps it 16-bit
    // on reassembly rather than collapsing to zero-page.
    let bytes = vec![0x9D, 0x00, 0x00, 0xAD, 0x10, 0x00, 0x60];
    let listing = listing_6502(&bytes, 0x0800);
    let re = assemble_acme(&listing).expect("reassemble");
    assert_eq!(re.bytes, bytes, "listing:\n{listing}");
}

#[test]
fn round_trips_m68k_pure_code() {
    // Pure code (no interleaved data) round-trips through the optimizing
    // assembler: the disassembly's explicit forms are optimizer-stable.
    let source = "\
        \tlea\t$dff000,a5\n\
        \tmove.l\t(a5),d0\n\
        \tand.l\td1,d0\n\
        loop:\n\
        \taddq.w\t#1,d0\n\
        \tcmp.w\t#100,d0\n\
        \tbne.s\tloop\n\
        \tmovem.l\td0-d3/a0-a1,-(sp)\n\
        \trts\n";
    let original = assemble_vasm(source).expect("assemble");
    let listing = listing_68000(&original, 0);
    let re = assemble_vasm(&listing).expect("reassemble");
    assert_eq!(re, original, "listing was:\n{listing}");
}

/// The optimized Amiga curriculum round-trips byte-exact when the disassembly
/// is reassembled with the optimizer off — the listing captures each
/// instruction's *encoded* form explicitly, so `-no-opt` reproduces it.
/// (Reassembling with the optimizer on cannot be byte-exact for data
/// interleaved in the code stream: a data word that happens to decode as, say,
/// `add #2,d0` would be re-optimized to `addq`.)
#[test]
fn round_trips_m68k_flat_curriculum() {
    let source = "\
        \tlea\tdata,a0\n\
        \tmove.l\t#data,d0\n\
        \tlea\t8(a0),a0\n\
        \tadd.l\t#$400,a1\n\
        \tcmp.w\t#0,d2\n\
        \tbne.s\tdata\n\
        data:\n\
        \tdc.w\t$0180,$0000\n\
        \tdc.l\t$deadbeef\n";
    let original = crate::dialects::vasm::assemble_with(source, true).expect("assemble");
    let listing = listing_68000(&original, 0);
    let re = crate::dialects::vasm::assemble_with(&listing, false).expect("reassemble");
    assert_eq!(re, original, "listing was:\n{listing}");
}
