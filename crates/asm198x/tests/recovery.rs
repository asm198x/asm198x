//! Recovery proves byte identity while preserving the reader's code/data map.
use asm198x::{RecoveryOptions, assemble_acme, recover_6502};

#[path = "support/scratch.rs"]
mod scratch;

const PALETTE: &str = include_str!("../../../examples/recovery/palette.a");

fn all_code(origin: u16, bytes: &[u8]) -> RecoveryOptions {
    let mut options = RecoveryOptions::new(origin);
    options
        .code
        .push(u32::from(origin)..u32::from(origin) + bytes.len() as u32);
    options
}

#[test]
fn palette_recovers_labels_and_data_then_accepts_an_intentional_edit() {
    let binary = assemble_acme(PALETTE).expect("example");
    assert_eq!(binary.bytes.len(), 28);
    let mut options = all_code(0x2000, &binary.bytes);
    options.data.push(0x2014..0x201c);
    options.labels.insert(0x2014, "palette".into());
    options.labels.insert(0x0400, "buffer".into());
    let source = recover_6502(&binary.bytes, &options).expect("recovery");
    assert!(source.contains("BNE l_2002"), "{source}");
    assert!(source.contains("JSR l_200e"), "{source}");
    assert!(source.contains("LDA palette,X"), "{source}");
    assert!(source.contains("STA buffer,X"), "{source}");
    assert!(
        source.contains("!byte $00, $03, $06, $09, $0C, $0F, $0C, $06"),
        "{source}"
    );
    assert_eq!(assemble_acme(&source).expect("rebuild").bytes, binary.bytes);
    assert_eq!(asm198x::format_acme(&source).expect("format"), source);
    let modified = source.replace("EOR #$00", "EOR #$0F");
    let modified = assemble_acme(&modified).expect("modified rebuild");
    let differences: Vec<_> = binary
        .bytes
        .iter()
        .zip(&modified.bytes)
        .enumerate()
        .filter_map(|(offset, (a, b))| (a != b).then_some((offset, *a, *b)))
        .collect();
    assert_eq!(differences, vec![(15, 0, 15)]);
}

#[test]
fn all_unmarked_bytes_stay_data_even_when_they_are_valid_opcodes() {
    let bytes = [0xa9, 0x01, 0x60, 0xea];
    let source = recover_6502(&bytes, &RecoveryOptions::new(0x2000)).expect("all data");
    assert!(source.contains("!byte $A9, $01, $60, $EA"), "{source}");
    assert!(!source.contains("LDA"));
}

#[test]
fn code_ranges_stop_at_data_and_truncated_encodings() {
    let bytes = [0xad, 0x20, 0x03, 0xea, 0xad];
    let mut options = all_code(0x2000, &bytes);
    options.data.push(0x2001..0x2003);
    let source = recover_6502(&bytes, &options).expect("split instruction and truncated tail");
    assert!(source.contains("!byte $AD, $20, $03"), "{source}");
    assert!(source.contains("NOP"), "{source}");
    assert!(source.contains("!byte $AD"));
    assert_eq!(assemble_acme(&source).expect("rebuild").bytes, bytes);
}

#[test]
fn targets_inside_instructions_preserve_bytes_and_get_a_real_label() {
    let bytes = [0xa9, 0x01, 0xd0, 0xfd]; // branch to the operand at $2001
    let source = recover_6502(&bytes, &all_code(0x2000, &bytes)).expect("overlapping target");
    assert!(source.contains("l_2001:"), "{source}");
    assert!(source.contains("BNE l_2001"), "{source}");
    assert!(
        !source.contains("LDA"),
        "split instruction is retained as data: {source}"
    );
    assert_eq!(assemble_acme(&source).expect("rebuild").bytes, bytes);
}

#[test]
fn targets_in_data_are_named_without_promoting_data_to_code() {
    let bytes = [0x4c, 0x03, 0x20, 0xea];
    let mut options = RecoveryOptions::new(0x2000);
    options.code.push(0x2000..0x2003);
    let source = recover_6502(&bytes, &options).expect("jump into data");
    assert!(source.contains("JMP l_2003"), "{source}");
    assert!(source.contains("l_2003:"));
    assert!(source.contains("!byte $EA"));
    assert!(!source.contains("NOP"));
}

#[test]
fn indirect_jumps_and_external_calls_do_not_invent_destinations() {
    let bytes = [0x6c, 0x05, 0x20, 0x20, 0xd2, 0xff];
    let source = recover_6502(&bytes, &all_code(0x2000, &bytes)).expect("indirect jump");
    assert!(source.contains("JMP ($2005)"), "{source}");
    assert!(source.contains("JSR $FFD2"), "{source}");
    assert!(!source.contains("l_2005"));
    assert!(!source.contains("l_ffd2"));
}

#[test]
fn custom_names_are_validated_and_generated_names_avoid_collisions() {
    let bytes = [0xd0, 0xfe];
    let mut options = all_code(0x2000, &bytes);
    options.labels.insert(0x4000, "l_2000".into());
    let source = recover_6502(&bytes, &options).expect("collision avoided");
    assert!(source.contains("BNE l_2000_2"), "{source}");
    for name in ["", "3bad", "x\n!byte 1", "a:b", ".local"] {
        options.labels.insert(0x4000, name.into());
        assert!(recover_6502(&bytes, &options).is_err(), "{name:?}");
    }
    options.labels.insert(0x4000, "same".into());
    options.labels.insert(0x4001, "same".into());
    assert!(
        recover_6502(&bytes, &options)
            .expect_err("duplicate name")
            .message
            .contains("more than one")
    );
}

#[test]
fn range_validation_refuses_wrap_empty_and_out_of_image_ranges() {
    for range in [0x1fff..0x2001, 0x2001..0x2003, 0x2000..0x2000] {
        let mut options = RecoveryOptions::new(0x2000);
        options.code.push(range);
        assert!(recover_6502(&[0xea, 0xea], &options).is_err());
    }
    assert!(recover_6502(&[], &RecoveryOptions::new(0)).is_err());
    assert!(recover_6502(&[0xea, 0xea], &RecoveryOptions::new(0xffff)).is_err());
    recover_6502(&[0xea], &all_code(0xffff, &[0xea])).expect("last address is usable");
}

#[test]
fn every_documented_form_preserves_encoding_and_address_width() {
    let mut count = 0;
    for insn in isa::mos6502::SET.instructions {
        for form in insn.forms {
            for operand in [0, 1, 0x7f, 0x80, 0xff] {
                let mut bytes = form.opcode.to_vec();
                bytes.resize(form.len(), operand);
                for origin in [0x0000, 0x2000] {
                    let source =
                        recover_6502(&bytes, &all_code(origin, &bytes)).unwrap_or_else(|e| {
                            panic!(
                                "{} {} {bytes:02x?} at {origin:04x}: {e}",
                                insn.mnemonic, form.mode
                            )
                        });
                    assert_eq!(assemble_acme(&source).expect("rebuild").bytes, bytes);
                }
            }
            count += 1;
        }
    }
    assert_eq!(count, 151);
}

#[test]
fn relative_wrap_at_the_address_space_boundary_is_reassemblable() {
    for (origin, bytes) in [(0xfffe, [0xd0, 0]), (0, [0xd0, 0x80])] {
        let source = recover_6502(&bytes, &all_code(origin, &bytes)).expect("wrapping branch");
        assert_eq!(assemble_acme(&source).expect("rebuild").bytes, bytes);
    }
}

fn cli(args: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_asm198x"))
        .args(args)
        .output()
        .expect("CLI")
}

#[test]
fn cli_recovers_to_a_new_file_and_verifies_before_writing() {
    let tmp = scratch::dir("recovery-cli");
    let input = tmp.join("palette.bin");
    let output = tmp.join("recovered.a");
    let binary = assemble_acme(PALETTE).expect("example").bytes;
    std::fs::write(&input, &binary).expect("binary");
    let result = cli(&[
        "disasm",
        "--recover",
        "--dialect",
        "acme",
        "--org",
        "0x2000",
        "--code",
        "0x2000:0x2014",
        "--label",
        "0x2014:palette",
        input.to_str().expect("path"),
        "-o",
        output.to_str().expect("path"),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("verified byte-identical"));
    let source = std::fs::read_to_string(&output).expect("source");
    assert_eq!(assemble_acme(&source).expect("rebuild").bytes, binary);

    // Existing hand-edited source, the input binary, and aliases must survive.
    let existing = cli(&[
        "disasm",
        "--recover",
        input.to_str().expect("path"),
        "-o",
        output.to_str().expect("path"),
    ]);
    assert!(!existing.status.success());
    assert_eq!(std::fs::read_to_string(&output).expect("source"), source);
    let same = cli(&[
        "disasm",
        "--recover",
        input.to_str().expect("path"),
        "-o",
        input.to_str().expect("path"),
    ]);
    assert!(!same.status.success());
    assert_eq!(std::fs::read(&input).expect("input"), binary);
    let alias = tmp.join("alias.a");
    std::fs::hard_link(&input, &alias).expect("hard link");
    let result = cli(&[
        "disasm",
        "--recover",
        input.to_str().expect("path"),
        "-o",
        alias.to_str().expect("path"),
    ]);
    assert!(!result.status.success());
    assert_eq!(std::fs::read(&input).expect("input"), binary);

    let invalid_output = tmp.join("invalid.a");
    let result = cli(&[
        "disasm",
        "--recover",
        "--code",
        "0:9999",
        input.to_str().expect("path"),
        "-o",
        invalid_output.to_str().expect("path"),
    ]);
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert!(!invalid_output.exists());
}

#[test]
fn cli_reports_unsupported_modes_and_emits_verified_source_to_stdout() {
    let tmp = scratch::dir("recovery-options");
    let input = tmp.join("one.bin");
    std::fs::write(&input, [0xea]).expect("binary");
    let path = input.to_str().expect("path");
    for flags in [
        vec!["--recover", "-d", "ca65"],
        vec!["--code", "0:1"],
        vec!["--recover", "--code", "0:999999999999"],
        vec!["--recover", "--label", "0:a", "--label", "0:b"],
        vec!["--recover", "--prg"],
    ] {
        let mut args = vec!["disasm"];
        args.extend(flags);
        args.push(path);
        let result = cli(&args);
        assert!(!result.status.success(), "{args:?}");
        assert!(result.stdout.is_empty());
    }
    let result = cli(&[
        "disasm",
        "--recover",
        "--cpu",
        "6502",
        "--org",
        "65535",
        "--code",
        "65535:65536",
        path,
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let source = String::from_utf8(result.stdout).expect("source");
    assert!(source.contains("NOP"));
    assert_eq!(assemble_acme(&source).expect("rebuild").bytes, [0xea]);
    let legacy = cli(&["disasm", "-d", "acme", path]);
    assert!(legacy.status.success());
    assert_eq!(
        String::from_utf8(legacy.stdout).expect("legacy"),
        asm198x::listing_6502(&[0xea], 0)
    );
}

#[test]
#[ignore = "requires the reference ACME assembler"]
fn recovered_source_matches_reference_acme_for_every_form_and_wrap_cases() {
    let tmp = scratch::dir("recovery-reference");
    let source_path = tmp.join("recovered.a");
    let output_path = tmp.join("reference.bin");
    let mut cases = Vec::new();
    for insn in isa::mos6502::SET.instructions {
        for form in insn.forms {
            let mut bytes = form.opcode.to_vec();
            bytes.resize(form.len(), 0);
            cases.push((0x2000, bytes));
        }
    }
    cases.extend([(0xfffe, vec![0xd0, 0]), (0, vec![0xd0, 0x80])]);
    for (origin, bytes) in cases {
        let source = recover_6502(&bytes, &all_code(origin, &bytes)).expect("recover");
        std::fs::write(&source_path, &source).expect("source");
        let result = std::process::Command::new("acme")
            .args(["-f", "plain", "-o"])
            .arg(&output_path)
            .arg(&source_path)
            .output()
            .expect("reference ACME required");
        assert!(
            result.status.success(),
            "{source}\n{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            std::fs::read(&output_path).expect("reference output"),
            bytes,
            "{source}"
        );
    }
}
