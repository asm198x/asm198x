//! Every instruction of every ca65 CPU that `disassemble_ca65` covers
//! disassembles, through `listing_ca65_on`, to ca65 source that our ca65
//! front end assembles back to the same bytes. An instruction ca65 does not
//! know comes back as `.byte`, which round-trips too.

use asm198x::source::MemoryLoader;
use lang198x_ca65::Cpu;

/// A flat link: `CODE` at 0, written as it is.
const FLAT_CFG: &str = "MEMORY { ROM: start = $0000, size = $1000, fill = no, file = %O; }\n\
                        SEGMENTS { CODE: load = ROM, type = ro; }\n";

/// What `disassemble_ca65` covers: every ca65 CPU but the 65816 and the
/// HuC6280, which have disassemblers of their own.
const CPUS: [Cpu; 10] = [
    Cpu::Nmos6502X,
    Cpu::Dtv6502,
    Cpu::Mos65sc02,
    Cpu::Mos65c02,
    Cpu::Wdc65c02,
    Cpu::Csg65ce02,
    Cpu::Csg4510,
    Cpu::Mega45gs02,
    Cpu::M740,
    Cpu::Sweet16,
];

#[test]
fn every_form_reassembles_to_its_bytes() {
    let mut checked = 0;
    let mut failures = Vec::new();
    for cpu in CPUS {
        let (base, extension) = lang198x_ca65::lower::sets(cpu);
        for set in extension.into_iter().chain([base]) {
            for insn in set.instructions {
                for form in insn.forms {
                    // The 6502X's operand-less `DOP` and `TOP` are only
                    // spellings: the processor reads the operand anyway.
                    if insn.forms.iter().any(|other| {
                        other.opcode == form.opcode && other.operands.len() > form.operands.len()
                    }) {
                        continue;
                    }
                    let operands: usize = form.operands.iter().map(|o| o.bytes as usize).sum();
                    let mut bytes = form.opcode.to_vec();
                    bytes.extend((0..operands).map(|i| 0x12 + i as u8 * 0x22));
                    let name = cpu.name().to_ascii_lowercase();
                    let source =
                        asm198x::listing_ca65_on(&bytes, 0x0400, &name).expect("a ca65 CPU");
                    let loader = MemoryLoader::new();
                    let assembled = asm198x::assemble_ca65_files_on(
                        &source,
                        "listing.s",
                        &loader,
                        &name,
                        Some(FLAT_CFG),
                    );
                    checked += 1;
                    match assembled {
                        Ok((result, _)) if result.bytes == bytes => {}
                        Ok((result, _)) => failures.push(format!(
                            "{name} {} {}: {:02X?} gave {:02X?}\n{source}",
                            insn.mnemonic, form.mode, bytes, result.bytes
                        )),
                        Err(e) => failures.push(format!(
                            "{name} {} {}: {}\n{source}",
                            insn.mnemonic, form.mode, e.error
                        )),
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {checked} forms do not round-trip:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(checked > 1000, "only {checked} forms checked");
}

/// The DTV's undocumented instructions that ca65 V2.19 leaves out of its DTV
/// table come back as data, with the instruction in a comment.
#[test]
fn an_instruction_ca65_lacks_is_data() {
    let listing = asm198x::listing_ca65_on(&[0xC7, 0x12, 0xEA], 0x1000, "6502DTV").expect("DTV");
    assert_eq!(
        listing,
        "        .setcpu \"6502dtv\"\n        .org $1000\n        .byte $C7, $12 ; dcp $12\n        nop\n"
    );
}
