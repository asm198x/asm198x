//! Round-trip tests: assemble → disassemble → reassemble reproduces the bytes.
//!
//! These live here, not in `isa-disasm`, because they exercise both the
//! assembler (this crate) and the disassembler (the dependency-free
//! `isa-disasm` crate, re-exported from here). It's the payoff the
//! authored-spec architecture was justified by — see the umbrella
//! `asm198x-and-shared-isa-spec.md`.

use crate::{
    assemble_1802, assemble_2650, assemble_8039, assemble_8048, assemble_acme, assemble_f8,
    assemble_i8080, assemble_m6800, assemble_pasmonext, assemble_rgbasm, assemble_scmp,
    assemble_vasm, listing_1802, listing_2650, listing_6502, listing_8048, listing_68000,
    listing_f8, listing_i8080, listing_m6800, listing_scmp, listing_sm83, listing_z80,
};

#[test]
fn round_trips_1802_through_asl_syntax() {
    // Register ops, immediates, both branch shapes, big-endian long branch.
    let source = "\
        \torg 1000h\n\
        start:\n\
        \tldi 42h\n\
        \tplo 3\n\
        \tphi 3\n\
        \tsex 2\n\
        \tinc 3\n\
        \tglo 3\n\
        \tani 0fh\n\
        \tbnz start\n\
        \tout 4\n\
        \tlbr start\n\
        \tidl\n";
    let original = assemble_1802(source).expect("assemble");
    let listing = listing_1802(&original.bytes, original.origin);
    let re = assemble_1802(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

#[test]
fn round_trips_m6800_through_asl_syntax() {
    // Motorola syntax, big-endian, all six addressing modes.
    let source = "\
        \torg $0100\n\
        start:\n\
        \tldx #$1234\n\
        \tldaa #$42\n\
        \tstaa $80\n\
        \tldab $2000\n\
        \tadda $05,x\n\
        \tinx\n\
        \tcmpa #$00\n\
        \tbne start\n\
        \tjsr $05,x\n\
        \tjmp $3000\n\
        \tclra\n\
        \trts\n";
    let original = assemble_m6800(source).expect("assemble");
    let listing = listing_m6800(&original.bytes, original.origin);
    let re = assemble_m6800(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

#[test]
fn round_trips_8048_through_asl_syntax() {
    // Register/keyword forms, an immediate, a page-relative conditional jump,
    // DJNZ, and the computed-opcode JMP/CALL.
    let source = "\
        \torg 100h\n\
        start:\n\
        \tmov a,#42h\n\
        \tmov r0,#0ffh\n\
        \tadd a,r7\n\
        \tanl a,#0fh\n\
        \tinc @r0\n\
        \tmovx @r1,a\n\
        \tjz start\n\
        \tdjnz r3,start\n\
        \tsel rb1\n\
        \tcall 200h\n\
        \tjmp start\n\
        \tret\n";
    let original = assemble_8048(source).expect("assemble");
    let listing = listing_8048(&original.bytes, original.origin);
    let re = assemble_8048(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

#[test]
fn round_trips_scmp_through_asl_syntax() {
    // Inherent, pointer exchange, all memory-reference shapes (@, negative
    // displacement, the E-register index), immediates, and a transfer.
    let source = "\
        \torg 0x0100\n\
        start:\n\
        \tldi 0x2A\n\
        \txpah 1\n\
        \txpal 1\n\
        \tld 5(1)\n\
        \tld @-1(2)\n\
        \tst e(1)\n\
        \tand 0x0f(1)\n\
        \tadd @3(3)\n\
        \tild 0(1)\n\
        \tjnz -2(0)\n\
        \tdly 0xFF\n\
        \txppc 3\n\
        \thalt\n";
    let original = assemble_scmp(source).expect("assemble");
    let listing = listing_scmp(&original.bytes, original.origin);
    let re = assemble_scmp(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

#[test]
fn round_trips_8039_romless_mcs48() {
    // The ROM-less parts share the 8048 encoding and disassembler; a program of
    // 8039-legal instructions (no BUS-port ops) round-trips through assemble_8039.
    let source = "\
        \torg 100h\n\
        start:\n\
        \tmov a,#42h\n\
        \tadd a,r7\n\
        \torl p1,#0fh\n\
        \toutl p2,a\n\
        \tmovx @r0,a\n\
        \tinc @r1\n\
        \tjz start\n\
        \tdjnz r3,start\n\
        \tsel mb1\n\
        \tcall 200h\n\
        \tjmp start\n\
        \tret\n";
    let original = assemble_8039(source).expect("assemble");
    let listing = listing_8048(&original.bytes, original.origin);
    let re = assemble_8039(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

#[test]
fn round_trips_2650_through_asl_syntax() {
    // Register / immediate / relative (indirect) / absolute (indirect + indexed)
    // addressing, condition and register branches, ZBRR page-0 relative, I/O,
    // and program-status ops.
    let source = "\
        \torg $0000\n\
        back:\n\
        \tnop\n\
        \tlodi,r0 $42\n\
        \tlodz r1\n\
        \taddz r2\n\
        \tcomi,r0 $10\n\
        \tlodr,r0 back\n\
        \tstrr,r1 *back\n\
        \tbctr,eq back\n\
        \tbcfr,gt back\n\
        \tbrnr,r1 back\n\
        \tzbrr $05\n\
        \tloda,r1 $1234\n\
        \tloda,r0 *$1234\n\
        \tloda,r0 $0100,r3\n\
        \tadda,r0 $0100,r3,+\n\
        \tstra,r0 $0100,r3,-\n\
        \trrr,r0\n\
        \tbcta,un start\n\
        \tbsta,un start\n\
        \tbxa $2000\n\
        \tredc,r0\n\
        \twrte,r0 $05\n\
        \tcpsl $01\n\
        \ttmi,r0 $0f\n\
        \tlpsu\n\
        start:\n\
        \tretc,un\n\
        \thalt\n";
    let original = assemble_2650(source).expect("assemble");
    let listing = listing_2650(&original.bytes, original.origin);
    let re = assemble_2650(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

#[test]
fn round_trips_f8_through_asl_syntax() {
    // Scratchpad register nibble (incl. S/I/D), immediate loads, big-endian
    // 16-bit address, every branch shape (named, masked BT/BF, BR7) with forward
    // and backward targets, I/O, and shifts.
    let source = "\
        \torg 0100h\n\
        start:\n\
        \tlisu 4\n\
        \tlisl 0\n\
        \tlr a,ku\n\
        \tli 55h\n\
        loop:\n\
        \tas 1\n\
        \tns d\n\
        \tlr d,a\n\
        \tbf 6,loop\n\
        \tbnz loop\n\
        \tci 10h\n\
        \tdci 1234h\n\
        \tlm\n\
        \txs s\n\
        \tbr7 loop\n\
        \tbt 1,done\n\
        \tout 0\n\
        \tjmp start\n\
        done:\n\
        \tsl 4\n\
        \tsr\n\
        \tclr\n\
        \tpop\n";
    let original = assemble_f8(source).expect("assemble");
    let listing = listing_f8(&original.bytes, original.origin);
    let re = assemble_f8(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

#[test]
fn round_trips_i8080_through_asl_syntax() {
    // Intel mnemonics, radix-suffixed numbers, absolute jumps (position-
    // independent, so origin choice is free).
    let source = "\
        \torg 100h\n\
        start:\n\
        \tlxi h,1234h\n\
        \tmvi a,42h\n\
        \tmov m,a\n\
        \tinx h\n\
        \tadd b\n\
        \tcpi 0ffh\n\
        \tjnz start\n\
        \tlda 2000h\n\
        \tpush psw\n\
        \trst 7\n\
        \tret\n";
    let original = assemble_i8080(source).expect("assemble");
    let listing = listing_i8080(&original.bytes, original.origin);
    let re = assemble_i8080(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

#[test]
fn round_trips_sm83_through_rgbasm() {
    // A spread of SM83-specific and shared forms: high-page loads, HL+/-, the
    // signed SP ops, CB bit ops, relative and absolute jumps.
    let source = "\
        SECTION \"code\", ROM0[$0150]\n\
        start:\n\
            ld hl, $c000\n\
            ld a, $42\n\
            ld [hl+], a\n\
            ldh [$ff47], a\n\
            ldh a, [$ff44]\n\
            ld hl, sp+4\n\
            add sp, -2\n\
            swap a\n\
            bit 7, [hl]\n\
            set 0, b\n\
            res 3, a\n\
            rst $38\n\
        .loop:\n\
            sub b\n\
            cp $10\n\
            jr nz, .loop\n\
            jp start\n\
            ret\n";
    let original = assemble_rgbasm(source).expect("assemble");
    let listing = listing_sm83(&original.bytes, original.origin);
    let re = assemble_rgbasm(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

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

/// The extended/BCD arithmetic and CMPM families encode to the exact bytes
/// vasm emits. Both operand shapes — `Dn,Dn` and `-(An),-(An)` (or `(An)+,(An)+`
/// for CMPM) — exercise the `AddrIndirect` slot's accept/encode path directly,
/// independent of the (ignored, tool-dependent) conformance sweep.
#[test]
fn encodes_m68k_extended_and_bcd() {
    let cases: &[(&str, &[u8])] = &[
        ("\taddx.w\td1,d0\n", &[0xD1, 0x41]),
        ("\taddx.w\t-(a1),-(a0)\n", &[0xD1, 0x49]),
        ("\tsubx.w\td1,d0\n", &[0x91, 0x41]),
        ("\tsubx.w\t-(a1),-(a0)\n", &[0x91, 0x49]),
        ("\tabcd.b\td1,d0\n", &[0xC1, 0x01]),
        ("\tabcd.b\t-(a1),-(a0)\n", &[0xC1, 0x09]),
        ("\tsbcd.b\td1,d0\n", &[0x81, 0x01]),
        ("\tsbcd.b\t-(a1),-(a0)\n", &[0x81, 0x09]),
        ("\tcmpm.w\t(a1)+,(a0)+\n", &[0xB1, 0x49]),
        ("\tcmpm.l\t(a3)+,(a2)+\n", &[0xB5, 0x8B]),
    ];
    for (src, want) in cases {
        let got = assemble_vasm(src).unwrap_or_else(|e| panic!("assemble `{src}`: {e:?}"));
        assert_eq!(&got, want, "for `{src}`");
    }
}

/// TRAP (4-bit vector), MOVEA (An destination, word/long), and EXG (three
/// register-pair kinds plus the reversed `Ay,Dx` source order) encode to the
/// exact bytes vasm emits.
#[test]
fn encodes_m68k_trap_movea_exg() {
    let cases: &[(&str, &[u8])] = &[
        ("\ttrap\t#0\n", &[0x4E, 0x40]),
        ("\ttrap\t#15\n", &[0x4E, 0x4F]),
        ("\tmovea.w\td0,a1\n", &[0x32, 0x40]),
        ("\tmovea.l\ta0,a1\n", &[0x22, 0x48]),
        ("\tmovea.l\t#4,a0\n", &[0x20, 0x7C, 0x00, 0x00, 0x00, 0x04]),
        ("\texg\td0,d1\n", &[0xC1, 0x41]),
        ("\texg\ta0,a1\n", &[0xC1, 0x49]),
        ("\texg\td0,a1\n", &[0xC1, 0x89]),
        // Reversed source order canonicalizes to the same Dx,Ay encoding.
        ("\texg\ta1,d0\n", &[0xC1, 0x89]),
    ];
    for (src, want) in cases {
        let got = assemble_vasm(src).unwrap_or_else(|e| panic!("assemble `{src}`: {e:?}"));
        assert_eq!(&got, want, "for `{src}`");
    }
}

/// CCR/SR/USP control-register moves and the ORI/ANDI/EORI immediate-to-CCR/SR
/// forms encode to the exact bytes vasm emits (base-68000 forms only; `move
/// ccr,<ea>` is 68010+ and intentionally unsupported).
#[test]
fn encodes_m68k_control_registers() {
    let cases: &[(&str, &[u8])] = &[
        ("\tmove\td0,ccr\n", &[0x44, 0xC0]),
        ("\tmove\t$1000,ccr\n", &[0x44, 0xF9, 0x00, 0x00, 0x10, 0x00]),
        ("\tmove\t#$12,ccr\n", &[0x44, 0xFC, 0x00, 0x12]),
        ("\tmove\td0,sr\n", &[0x46, 0xC0]),
        ("\tmove\tsr,d0\n", &[0x40, 0xC0]),
        ("\tmove\tsr,$1000\n", &[0x40, 0xF9, 0x00, 0x00, 0x10, 0x00]),
        ("\tmove\tusp,a0\n", &[0x4E, 0x68]),
        ("\tmove\ta3,usp\n", &[0x4E, 0x63]),
        ("\tandi\t#1,ccr\n", &[0x02, 0x3C, 0x00, 0x01]),
        ("\tori\t#2,ccr\n", &[0x00, 0x3C, 0x00, 0x02]),
        ("\teori\t#4,ccr\n", &[0x0A, 0x3C, 0x00, 0x04]),
        ("\tandi\t#$1234,sr\n", &[0x02, 0x7C, 0x12, 0x34]),
        ("\tori\t#$5678,sr\n", &[0x00, 0x7C, 0x56, 0x78]),
        ("\teori\t#$00ff,sr\n", &[0x0A, 0x7C, 0x00, 0xFF]),
    ];
    for (src, want) in cases {
        let got = assemble_vasm(src).unwrap_or_else(|e| panic!("assemble `{src}`: {e:?}"));
        assert_eq!(&got, want, "for `{src}`");
    }
}

/// MOVEP encodes to the exact bytes vasm emits — both directions and sizes,
/// with the mandatory `d16(Ay)` displacement word.
#[test]
fn encodes_m68k_movep() {
    let cases: &[(&str, &[u8])] = &[
        ("\tmovep.w\t0(a0),d0\n", &[0x01, 0x08, 0x00, 0x00]),
        ("\tmovep.l\t0(a2),d3\n", &[0x07, 0x4A, 0x00, 0x00]),
        ("\tmovep.w\td0,8(a0)\n", &[0x01, 0x88, 0x00, 0x08]),
        ("\tmovep.l\td3,8(a2)\n", &[0x07, 0xCA, 0x00, 0x08]),
    ];
    for (src, want) in cases {
        let got = assemble_vasm(src).unwrap_or_else(|e| panic!("assemble `{src}`: {e:?}"));
        assert_eq!(&got, want, "for `{src}`");
    }
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
