//! Round-trip tests: assemble → disassemble → reassemble reproduces the bytes.
//!
//! These live here, not in `isa-disasm`, because they exercise both the
//! assembler (this crate) and the disassembler (the dependency-free
//! `isa-disasm` crate, re-exported from here). It's the payoff the
//! authored-spec architecture was justified by — see the umbrella
//! `asm198x-and-shared-isa-spec.md`.

use crate::{
    assemble_1802, assemble_2650, assemble_8039, assemble_8048, assemble_acme, assemble_f8,
    assemble_i8080, assemble_m6800, assemble_pasmonext, assemble_pdp11, assemble_rgbasm,
    assemble_scmp, assemble_tms7000, assemble_tms9900, assemble_vasm, assemble_z8000,
    assemble_z8001, listing_1802, listing_2650, listing_6502, listing_8048, listing_68000,
    listing_f8, listing_i8080, listing_m6800, listing_pdp11, listing_scmp, listing_sm83,
    listing_tms7000, listing_tms9900, listing_z80, listing_z8000, listing_z8001,
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
fn round_trips_z8000_dyadic_through_asl_syntax() {
    // Increment 1: the dyadic family across every addressing mode and size,
    // plus the LD store forms. (No position-dependent ops yet — the opcode
    // sweep already covers the group; this guards our own round-trip.)
    let source = "\
        \torg 0\n\
        \tadd r1,r2\n\
        \tadd r1,#1234h\n\
        \tadd r1,@r2\n\
        \tadd r1,1234h\n\
        \tadd r1,1234h(r2)\n\
        \tsub r3,r4\n\
        \tand r5,r6\n\
        \tcp r7,#0ah\n\
        \taddb rl1,rl2\n\
        \tcpb rh0,#0ffh\n\
        \tadc r8,r9\n\
        \tsbc r10,r11\n\
        \tld r1,r2\n\
        \tld r1,@r3\n\
        \tld r1,2000h\n\
        \tld r1,2000h(r4)\n\
        \tldb rl5,rh6\n\
        \tld @r2,r1\n\
        \tld 3000h,r1\n\
        \tld 3000h(r5),r1\n\
        \tldb @r7,rl1\n\
        \tldb 4000h,rl1\n\
        \tldl rr2,rr4\n\
        \tldl rr2,#12345678h\n\
        \tldl rr6,5000h(r1)\n\
        \tldl @r3,rr2\n\
        \taddl rr2,rr4\n\
        \tsubl rr6,rr8\n\
        \tcpl rr10,rr12\n\
        \tex r1,r2\n\
        \tex r3,@r4\n\
        \texb rl1,rl2\n\
        \tlda r1,6000h\n\
        \tlda r2,7000h(r3)\n";
    let original = assemble_z8000(source).expect("assemble");
    let listing = listing_z8000(&original.bytes, original.origin);
    let re = assemble_z8000(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

#[test]
fn round_trips_z8000_stack_through_asl_syntax() {
    // Increment 5: PUSH/POP/PUSHL/POPL across the value operand's modes, plus
    // the special PUSH immediate.
    let source = "\
        \torg 0\n\
        \tpush @r15,r1\n\
        \tpush @r15,@r2\n\
        \tpush @r15,1234h\n\
        \tpush @r15,1234h(r3)\n\
        \tpush @r15,#5678h\n\
        \tpushl @r14,rr2\n\
        \tpushl @r14,@r3\n\
        \tpop r1,@r15\n\
        \tpop @r2,@r15\n\
        \tpop 2000h,@r15\n\
        \tpop 2000h(r4),@r15\n\
        \tpopl rr2,@r14\n\
        \tpopl @r3,@r14\n";
    let original = assemble_z8000(source).expect("assemble");
    let listing = listing_z8000(&original.bytes, original.origin);
    let re = assemble_z8000(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

#[test]
fn round_trips_z8000_single_operand_through_asl_syntax() {
    // Increment 4: single-operand ALU across every addressing mode and size.
    let source = "\
        \torg 0\n\
        \tclr r1\n\
        \tclr @r2\n\
        \tclr 1234h\n\
        \tclr 1234h(r3)\n\
        \tclrb rl1\n\
        \tcom r5\n\
        \tneg @r4\n\
        \ttest 2000h\n\
        \ttset r6\n\
        \tnegb rl2\n\
        \ttestb rh0\n\
        \tinc r1\n\
        \tinc r1,#8\n\
        \tinc @r2,#16\n\
        \tdec r3,#3\n\
        \tincb rl1,#2\n\
        \tdecb rl4\n";
    let original = assemble_z8000(source).expect("assemble");
    let listing = listing_z8000(&original.bytes, original.origin);
    let re = assemble_z8000(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

#[test]
fn round_trips_z8000_shift_rotate_through_asl_syntax() {
    // Increment 6: shifts (a signed count word, its sign selecting left/right),
    // rotates (a packed 1/2 count), and the sign-extends. Shift candidates carry
    // an out-of-range filler count in the opcode sweep, so they fall to data
    // there — this guards their round-trip explicitly.
    let source = "\
        \torg 0\n\
        \tsla r1,#4\n\
        \tsra r1,#4\n\
        \tsll r2,#16\n\
        \tsrl r2,#16\n\
        \tsla r3,#0\n\
        \tslab rl1,#3\n\
        \tsrab rl1,#8\n\
        \tsllb rh0,#1\n\
        \tsrlb rl7,#7\n\
        \tslal rr2,#8\n\
        \tsral rr4,#32\n\
        \tslll rr6,#5\n\
        \tsrll rr8,#31\n\
        \trl r1,#1\n\
        \trl r1,#2\n\
        \trr r2,#1\n\
        \trlc r3,#2\n\
        \trrc r4,#1\n\
        \trlb rl1,#1\n\
        \trrb rl2,#2\n\
        \trlcb rh0,#1\n\
        \trrcb rl7,#2\n\
        \textsb r1\n\
        \texts rr2\n\
        \textsl rq4\n\
        \textsb r15\n\
        \texts rr14\n\
        \textsl rq12\n";
    let original = assemble_z8000(source).expect("assemble");
    let listing = listing_z8000(&original.bytes, original.origin);
    let re = assemble_z8000(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

#[test]
fn round_trips_z8000_bit_through_asl_syntax() {
    // Increment 7: BIT/SET/RES static (register / @Rn / direct / indexed, word
    // and byte) and dynamic (the bit number in a word register). The dynamic
    // two-word form falls to data in the opcode sweep (its second word is an
    // out-of-range filler there), so its round-trip is guarded here.
    let source = "\
        \torg 0\n\
        \tbit r1,#0\n\
        \tbit r1,#15\n\
        \tbit @r2,#3\n\
        \tbit 1234h,#3\n\
        \tbit 1234h(r2),#3\n\
        \tset r1,#5\n\
        \tset @r3,#7\n\
        \tset 5000h(r4),#15\n\
        \tres r1,#5\n\
        \tres 7000h,#9\n\
        \tbitb rl1,#7\n\
        \tbitb rh0,#3\n\
        \tbitb @r2,#3\n\
        \tbitb 1234h(r6),#2\n\
        \tsetb rl1,#5\n\
        \tresb rh7,#6\n\
        \tbit r3,r1\n\
        \tbit r5,r8\n\
        \tbit r0,r0\n\
        \tbitb rl3,r1\n\
        \tset r3,r1\n\
        \tres r7,r15\n\
        \tresb rl0,r2\n";
    let original = assemble_z8000(source).expect("assemble");
    let listing = listing_z8000(&original.bytes, original.origin);
    let re = assemble_z8000(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

#[test]
fn round_trips_z8000_muldiv_through_asl_syntax() {
    // Increment 8: MULT/MULTL/DIV/DIVL across every addressing mode. The long
    // immediate forms (MULTL/DIVL #imm) need a 4-byte immediate the opcode
    // sweep's 4-byte candidate can't hold, so they fall to data there — this
    // guards their round-trip.
    let source = "\
        \torg 0\n\
        \tmult rr2,r4\n\
        \tmult rr2,#1234h\n\
        \tmult rr2,@r4\n\
        \tmult rr2,5000h\n\
        \tmult rr2,6000h(r4)\n\
        \tmult rr14,r1\n\
        \tmultl rq0,rr4\n\
        \tmultl rq0,#12345678h\n\
        \tmultl rq0,@r4\n\
        \tmultl rq0,7000h(r2)\n\
        \tmultl rq12,rr6\n\
        \tdiv rr2,r4\n\
        \tdiv rr2,#5\n\
        \tdiv rr2,@r4\n\
        \tdiv rr14,r15\n\
        \tdivl rq0,rr4\n\
        \tdivl rq0,#87654321h\n\
        \tdivl rq0,@r4\n\
        \tdivl rq4,8000h(r3)\n";
    let original = assemble_z8000(source).expect("assemble");
    let listing = listing_z8000(&original.bytes, original.origin);
    let re = assemble_z8000(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

#[test]
fn round_trips_z8000_block_through_asl_syntax() {
    // Increment 9: the block / string repeat group. Every one is a two-word form
    // whose word 2 has a zero top nibble, so the opcode sweep's fixed filler
    // (top nibble 1) drops them all to data — this round-trip over all 32 is the
    // primary in-repo guard (the direct differential covers the byte values).
    let source = "\
        \torg 0\n\
        \tldi @r4,@r5,r6\n\
        \tldir @r4,@r5,r6\n\
        \tldd @r7,@r8,r9\n\
        \tlddr @r7,@r8,r9\n\
        \tldib @r4,@r5,r6\n\
        \tldirb @r4,@r5,r6\n\
        \tlddb @r7,@r8,r9\n\
        \tlddrb @r7,@r8,r9\n\
        \tcpi r1,@r2,r3,eq\n\
        \tcpir r1,@r2,r3,ne\n\
        \tcpd r4,@r5,r6,gt\n\
        \tcpdr r7,@r8,r9,ov\n\
        \tcpi r1,@r2,r3\n\
        \tcpib rl1,@r2,r3,eq\n\
        \tcpirb rh3,@r4,r5,ne\n\
        \tcpdb rl7,@r8,r9,lt\n\
        \tcpdrb rh0,@r2,r3,c\n\
        \tcpsi @r1,@r2,r3,eq\n\
        \tcpsir @r4,@r5,r6,ne\n\
        \tcpsd @r7,@r8,r9,pl\n\
        \tcpsdr @r10,@r11,r12,mi\n\
        \tcpsib @r1,@r2,r3,eq\n\
        \tcpsirb @r4,@r5,r6,ne\n\
        \tcpsdb @r7,@r8,r9,ge\n\
        \tcpsdrb @r13,@r14,r15,le\n\
        \ttrib @r4,@r5,r6\n\
        \ttrirb @r4,@r5,r6\n\
        \ttrdb @r7,@r8,r9\n\
        \ttrdrb @r7,@r8,r9\n\
        \ttrtib @r4,@r5,r6\n\
        \ttrtirb @r4,@r5,r6\n\
        \ttrtdb @r7,@r8,r9\n\
        \ttrtdrb @r7,@r8,r9\n";
    let original = assemble_z8000(source).expect("assemble");
    let listing = listing_z8000(&original.bytes, original.origin);
    let re = assemble_z8000(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

#[test]
fn round_trips_z8000_io_through_asl_syntax() {
    // Increment 10: the privileged I/O group. Simple IN/OUT/SIN/SOUT (direct and
    // @Rn-port, word/byte) plus the 32 block-I/O ops. The block-I/O forms fall
    // to data in the opcode sweep (word 2's zero top nibble); this round-trip
    // over all 44 guards them, and confirms the `supmode on` header round-trips.
    let source = "\
        \torg 0\n\
        \tin r1,1234h\n\
        \tinb rl1,1234h\n\
        \tin r1,@r2\n\
        \tinb rl1,@r2\n\
        \tout 1234h,r1\n\
        \toutb 1234h,rl1\n\
        \tout @r2,r1\n\
        \toutb @r2,rl1\n\
        \tsin r1,1234h\n\
        \tsinb rl1,1234h\n\
        \tsout 1234h,r1\n\
        \tsoutb 1234h,rl1\n\
        \tin r0,5678h\n\
        \tin r15,@r14\n\
        \tini @r1,@r2,r3\n\
        \tinir @r1,@r2,r3\n\
        \tind @r4,@r5,r6\n\
        \tindr @r4,@r5,r6\n\
        \tinib @r1,@r2,r3\n\
        \tinirb @r1,@r2,r3\n\
        \tindb @r4,@r5,r6\n\
        \tindrb @r4,@r5,r6\n\
        \touti @r1,@r2,r3\n\
        \totir @r1,@r2,r3\n\
        \toutd @r4,@r5,r6\n\
        \totdr @r4,@r5,r6\n\
        \toutib @r1,@r2,r3\n\
        \totirb @r1,@r2,r3\n\
        \toutdb @r4,@r5,r6\n\
        \totdrb @r4,@r5,r6\n\
        \tsini @r1,@r2,r3\n\
        \tsinir @r1,@r2,r3\n\
        \tsind @r4,@r5,r6\n\
        \tsindr @r4,@r5,r6\n\
        \tsinib @r1,@r2,r3\n\
        \tsinirb @r1,@r2,r3\n\
        \tsindb @r4,@r5,r6\n\
        \tsindrb @r4,@r5,r6\n\
        \tsouti @r1,@r2,r3\n\
        \tsotir @r1,@r2,r3\n\
        \tsoutd @r4,@r5,r6\n\
        \tsotdr @r4,@r5,r6\n\
        \tsoutib @r1,@r2,r3\n\
        \tsotirb @r1,@r2,r3\n\
        \tsoutdb @r4,@r5,r6\n\
        \tsotdrb @r4,@r5,r6\n";
    let original = assemble_z8000(source).expect("assemble");
    let listing = listing_z8000(&original.bytes, original.origin);
    let re = assemble_z8000(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

#[test]
fn round_trips_z8000_cpu_control_through_asl_syntax() {
    // Increment 11: the CPU-control / status group. All are position-independent
    // and opcode-sweep-verified; this guards the round-trip (including the
    // canonical flag order and control-register names).
    let source = "\
        \torg 0\n\
        \tnop\n\
        \thalt\n\
        \tiret\n\
        \tmset\n\
        \tmres\n\
        \tmbit\n\
        \tmreq r1\n\
        \tsetflg c\n\
        \tsetflg c,z,s,p\n\
        \tresflg s\n\
        \tcomflg p\n\
        \tei vi\n\
        \tei vi,nvi\n\
        \tdi nvi\n\
        \tldctl r1,fcw\n\
        \tldctl fcw,r1\n\
        \tldctl r2,refresh\n\
        \tldctl r4,psap\n\
        \tldctl nsp,r6\n\
        \tldctlb rl1,flags\n\
        \tldctlb flags,rh3\n\
        \tldps @r2\n\
        \tldps 1234h\n\
        \tldps 5000h(r3)\n\
        \tsc #0\n\
        \tsc #42h\n\
        \tsc #255\n";
    let original = assemble_z8000(source).expect("assemble");
    let listing = listing_z8000(&original.bytes, original.origin);
    let re = assemble_z8000(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

#[test]
fn round_trips_z8000_misc_through_asl_syntax() {
    // Cleanup increment: the last non-segmented instructions. TCC/LDK/RLDB/RRDB
    // are position-independent (opcode-sweep-verified); the PC-relative LDR forms
    // are position-dependent, so this round-trip is their guard.
    let source = "\
        \torg 0100h\n\
        \ttcc eq,r1\n\
        \ttcc r4\n\
        \ttccb ne,rl1\n\
        \ttccb rh0\n\
        \tldk r1,#5\n\
        \tldk r3,#15\n\
        \trldb rl1,rl2\n\
        \trrdb rh0,rh1\n\
        \trldb rl7,rh3\n\
        back:\n\
        \tldr r1,back\n\
        \tldr r15,back\n\
        \tldrb rl1,back\n\
        \tldrl rr2,back\n\
        \tldr back,r1\n\
        \tldrb back,rl1\n\
        \tldrl back,rr2\n\
        \tldr r3,fwd\n\
        \tnop\n\
        fwd:\n\
        \tnop\n";
    let original = assemble_z8000(source).expect("assemble");
    let listing = listing_z8000(&original.bytes, original.origin);
    let re = assemble_z8000(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

#[test]
fn round_trips_z8001_segmented_through_asl_syntax() {
    // Increment 12: the segmented Z8001 target-extension. `<<seg>>offset` direct
    // and indexed addresses (a two-word long-form operand), `@RRn` long-pair
    // pointers, `LDA` into a long pair, the block-I/O mixed pointers (memory
    // `@RR`, I/O `@R`), I/O left unchanged, and the segmented `LDCTL` control
    // registers (`PSAP`/`NSP` split into `PSAPSEG`/`PSAPOFF`/`NSPSEG`/`NSPOFF`).
    let source = "\
        \torg 0\n\
        \tld r1,<<5>>1234h\n\
        \tld r1,<<5>>1234h(r3)\n\
        \tld r1,@rr2\n\
        \tld <<5>>1234h,r1\n\
        \tldb rl1,<<5>>1234h(r3)\n\
        \tldl rr2,<<5>>1234h\n\
        \tadd r1,<<7>>5678h\n\
        \tsub r2,@rr4\n\
        \tclr <<5>>1234h\n\
        \tclr @rr2\n\
        \tpush @rr14,r1\n\
        \tpush @rr14,<<5>>1234h\n\
        \tpop r1,@rr14\n\
        \tlda rr2,<<5>>1234h\n\
        \tlda rr4,<<0>>0(r5)\n\
        \tldps <<5>>1234h\n\
        \tldps @rr2\n\
        \tjp <<5>>1234h\n\
        \tjp eq,@rr2\n\
        \tcall <<5>>1234h\n\
        \tmult rr2,<<5>>1234h\n\
        \tbit <<5>>1234h,#3\n\
        \tbit @rr2,#3\n\
        \tldir @rr4,@rr2,r3\n\
        \tcpi r1,@rr2,r3,eq\n\
        \tini @rr2,@r4,r3\n\
        \touti @r2,@rr4,r3\n\
        \tsind @rr6,@r8,r9\n\
        \tin r1,1234h\n\
        \tin r1,@r2\n\
        \tldctl r0,psapseg\n\
        \tldctl r1,psapoff\n\
        \tldctl r2,nspseg\n\
        \tldctl r3,nspoff\n\
        \tldctl nspoff,r4\n";
    let original = assemble_z8001(source).expect("assemble");
    let listing = listing_z8001(&original.bytes, original.origin);
    let re = assemble_z8001(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

#[test]
fn round_trips_z8000_control_through_asl_syntax() {
    // Increment 3: program control — the position-dependent JR / DJNZ / CALR
    // the opcode sweep can't batch, plus JP / CALL / RET with condition codes.
    let source = "\
        \torg 0100h\n\
        back:\n\
        \tjp back\n\
        \tjp eq,back\n\
        \tjp @r2\n\
        \tjp 2000h(r3)\n\
        \tcall back\n\
        \tcall @r4\n\
        \tjr back\n\
        \tjr ne,fwd\n\
        \tdjnz r1,back\n\
        \tdbjnz rl2,back\n\
        \tcalr back\n\
        \tcalr fwd\n\
        \tret\n\
        \tret eq\n\
        \tret nc\n\
        fwd:\n\
        \tret\n";
    let original = assemble_z8000(source).expect("assemble");
    let listing = listing_z8000(&original.bytes, original.origin);
    let re = assemble_z8000(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

#[test]
fn round_trips_tms9900_through_asl_syntax() {
    // The position-dependent instructions the opcode sweep can't batch — the
    // word-scaled jumps (forward and backward) — plus a spread of formats and
    // general-addressing modes, symbolic addresses, immediates, shifts, CRU,
    // and the workspace-context ops.
    let source = "\
        \torg 0100h\n\
        start:\n\
        \tli r0,0abcdh\n\
        \tmov r1,r2\n\
        \tmov @0300h,r3\n\
        \tmov @0300h(r4),r5\n\
        \ta *r6+,@0400h\n\
        \tmovb r7,*r8\n\
        \tcoc r9,r10\n\
        \tmpy r1,r2\n\
        \txop @0500h,3\n\
        \tldcr r1,8\n\
        \tsla r2,4\n\
        \tclr r3\n\
        \tinc @count\n\
        loop:\n\
        \tdec r0\n\
        \tjne loop\n\
        \tjeq done\n\
        \tsbo 5\n\
        \ttb -3\n\
        \tbl @sub\n\
        \tjmp start\n\
        sub:\n\
        \tb *r11\n\
        done:\n\
        \tlwpi 8300h\n\
        \tlimi 2\n\
        \trtwp\n\
        count:\tword 0\n";
    let original = assemble_tms9900(source).expect("assemble");
    let listing = listing_tms9900(&original.bytes, original.origin);
    let re = assemble_tms9900(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

#[test]
fn round_trips_pdp11_through_asl_syntax() {
    // The position-dependent instructions the opcode sweep can't batch: the
    // word-scaled conditional branches (forward and backward), SOB, JSR, and the
    // PC-relative / relative-deferred memory operands — plus a spread of
    // addressing modes and the EIS / trap / condition-code ops.
    let source = "\
        \torg 0x1000\n\
        start:\n\
        \tmov r1,r0\n\
        \tmov (r2)+,-(r3)\n\
        \tmov 4(r1),@6(r2)\n\
        \tmov #0x1234,r0\n\
        \tmov @#0x2000,r5\n\
        \tmov msg,r0\n\
        \tmov @msg,r1\n\
        \tclr count\n\
        \tmul r2,r1\n\
        \txor r1,(r4)\n\
        loop:\n\
        \tinc count\n\
        \tjsr pc,sub\n\
        \tsob r0,loop\n\
        \tbne loop\n\
        \tbr done\n\
        sub:\n\
        \ttst count\n\
        \tbeq back\n\
        \trts pc\n\
        back:\n\
        \tbr start\n\
        done:\n\
        \tmark 2\n\
        \temt 0x10\n\
        \tccc\n\
        \thalt\n\
        count:\tword 0\n\
        msg:\tbyte 0x48, 0x49\n";
    let original = assemble_pdp11(source).expect("assemble");
    let listing = listing_pdp11(&original.bytes, original.origin);
    let re = assemble_pdp11(&listing).expect("reassemble");
    assert_eq!(re.bytes, original.bytes, "listing was:\n{listing}");
}

#[test]
fn round_trips_tms7000_through_asl_syntax() {
    // Dual-operand ALU across addressing modes, the special MOV forms,
    // single-register ops, peripheral + extended addressing, MOVD, the
    // bit-test-and-jump / DJNZ relative ops, jumps, TRAP, and implied ops.
    let source = "\
        \torg 0\n\
        start:\n\
        \tmov %42h,a\n\
        \tmov r5,r6\n\
        \tmov a,b\n\
        \tmov a,r5\n\
        \tadd r5,a\n\
        \tcmp %0ffh,a\n\
        \tinc a\n\
        \tclr r200\n\
        \tmovp p6,a\n\
        \tandp %0fh,p6\n\
        \tlda @2000h\n\
        \tlda *r5\n\
        \tbr @1234h(b)\n\
        \tmovd %1234h,r4\n\
        \tmovd r2,r4\n\
        loop:\n\
        \tbtjo %1,a,loop\n\
        \tbtjop a,p6,loop\n\
        \tdjnz a,loop\n\
        \tdjnz r5,loop\n\
        \tjmp start\n\
        \tjz loop\n\
        \ttrap 5\n\
        \tpush st\n\
        \teint\n\
        \tnop\n";
    let original = assemble_tms7000(source).expect("assemble");
    let listing = listing_tms7000(&original.bytes, original.origin);
    let re = assemble_tms7000(&listing).expect("reassemble");
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
