use asm198x::{AssemblyResult, SymbolFormat, debug_info, render_symbol_export};
use std::process::Command;

#[path = "support/scratch.rs"]
mod scratch;

fn render(
    result: &AssemblyResult,
    format: SymbolFormat,
) -> Result<String, asm198x::SymbolExportError> {
    render_symbol_export(
        result,
        &debug_info(result, "test", "test", "input.asm"),
        format,
    )
}

const GB: &str = "SECTION \"boot\", ROM0[$150]\nEntry: nop\nSECTION \"code\", ROMX[$4010], BANK[3]\nBanked: db 3\nValue EQU 7\n";
const C64: &str = "* = $0801\nEntry: nop\n.loop: rts\nValue = 7\n";

#[test]
fn address_exports_preserve_names_locations_and_bank_identity() {
    let gb = asm198x::assemble_rgbasm(GB).expect("GB assembly");
    assert_eq!(
        render(&gb, SymbolFormat::NoCash).expect("export"),
        "03:4010 Banked\n00:0150 Entry\n"
    );
    let c64 = asm198x::assemble_acme(C64).expect("C64 assembly");
    let text = render(&c64, SymbolFormat::Vice).expect("export");
    assert!(text.contains("al C:0801 .Entry\n"), "{text}");
    assert!(!text.contains("Value"));
    assert_eq!(
        render(&c64, SymbolFormat::Native).expect("native"),
        asm198x::render_sym(&debug_info(&c64, "test", "test", "input.asm"))
    );
}

#[test]
fn exporters_refuse_lossy_bank_and_address_conversions() {
    let bank_zero = asm198x::assemble_rgbasm(
        "SECTION \"boot\", ROM0[$150]\nEntry: nop\nSECTION \"ram\", WRAM0[$c000]\nWork: ds 1\n",
    )
    .expect("bank zero assembly");
    assert!(render(&bank_zero, SymbolFormat::Vice).is_err());
    let gb = asm198x::assemble_rgbasm(GB).expect("assembly");
    assert!(
        render(&gb, SymbolFormat::Vice)
            .expect_err("banked label")
            .reason
            .contains("banked")
    );
    let flat = asm198x::assemble_pasmo(" org $8000\nlabel: nop\n").expect("assembly");
    assert!(
        render(&flat, SymbolFormat::NoCash)
            .expect_err("unknown bank")
            .reason
            .contains("bank")
    );
    let mut info = debug_info(&flat, "z80", "pasmo", "in.asm");
    info.sections[0].base = None;
    assert!(render_symbol_export(&flat, &info, SymbolFormat::Vice).is_err());
    info.sections[0].base = Some(0x10000);
    assert!(render_symbol_export(&flat, &info, SymbolFormat::Vice).is_err());
}

#[test]
fn names_cannot_inject_monitor_commands_or_collide_after_prefixing() {
    let result = asm198x::assemble_pasmo(" org $8000\none: nop\ntwo: nop\n").expect("assembly");
    let mut info = debug_info(&result, "z80", "pasmo", "in.asm");
    info.symbols[0].name = "one\nquit".into();
    assert!(render_symbol_export(&result, &info, SymbolFormat::Vice).is_err());
    info.symbols[0].name = "PC".into();
    assert!(
        render_symbol_export(&result, &info, SymbolFormat::Vice)
            .expect_err("reserved register")
            .reason
            .contains("reserved")
    );
    info.symbols[0].name = "two".into();
    info.symbols[1].name = ".two".into();
    assert!(render_symbol_export(&result, &info, SymbolFormat::Vice).is_err());
}

#[test]
fn cli_rejects_bad_options_and_protects_source_and_image_paths() {
    let dir = scratch::dir("symbol-export-errors");
    let input = dir.join("source.asm");
    let image = dir.join("out.bin");
    std::fs::write(&input, C64).expect("source");
    for options in [vec!["--sym-format=other"], vec!["--sym-format=vice"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_asm198x"))
            .arg(&input)
            .args(["--dialect", "acme"])
            .args(options)
            .output()
            .expect("CLI");
        assert!(!output.status.success());
    }
    for protected in [&input, &image] {
        let output = Command::new(env!("CARGO_BIN_EXE_asm198x"))
            .arg(&input)
            .args(["--dialect", "acme", "--sym-format=vice", "-o"])
            .arg(&image)
            .arg(format!("--sym={}", protected.display()))
            .output()
            .expect("CLI");
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("refusing to overwrite"));
        assert_eq!(
            std::fs::read_to_string(&input).expect("source preserved"),
            C64
        );
        assert_eq!(
            std::fs::read(&image).expect("image preserved"),
            [0xEA, 0x60]
        );
    }
}

#[test]
fn cli_exports_in_human_and_json_modes_without_changing_the_image() {
    let dir = scratch::dir("symbol-export-cli");
    for (name, source, dialect, format, extension) in [
        ("game", GB, "rgbasm", "nocash", "sym"),
        ("c64", C64, "acme", "vice", "vs"),
    ] {
        let input = dir.join(format!("{name}.asm"));
        std::fs::write(&input, source).expect("source");
        for mode in ["human", "json"] {
            let output = Command::new(env!("CARGO_BIN_EXE_asm198x"))
                .arg(&input)
                .args(["--dialect", dialect, "--sym", "--sym-format", format])
                .arg(format!("--message-format={mode}"))
                .arg("-o")
                .arg(dir.join(format!("{name}-{mode}.bin")))
                .output()
                .expect("CLI");
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let text = std::fs::read_to_string(input.with_extension(extension)).expect("symbols");
            assert!(text.contains("Entry"), "{text}");
            if mode == "json" {
                let result: AssemblyResult =
                    serde_json::from_slice(&output.stdout).expect("JSON result");
                assert!(!result.bytes.is_empty());
            }
        }
        assert_eq!(
            std::fs::read(dir.join(format!("{name}-human.bin"))).expect("human bytes"),
            std::fs::read(dir.join(format!("{name}-json.bin"))).expect("JSON bytes")
        );
    }
}

#[test]
#[ignore = "requires VICE_X64SC and optionally VICE_DATADIR"]
fn vice_imports_and_roundtrips_generated_labels() {
    let dir = scratch::dir("vice-symbol-consumer");
    let result = asm198x::assemble_acme(C64).expect("assembly");
    let text = render(&result, SymbolFormat::Vice).expect("export");
    std::fs::write(dir.join("labels.vs"), &text).expect("symbols");
    std::fs::write(
        dir.join("probe.mon"),
        "ll \"labels.vs\"\nsl \"roundtrip.vs\"\nquit\n",
    )
    .expect("monitor script");
    let mut command = Command::new(std::env::var_os("VICE_X64SC").expect("VICE_X64SC"));
    command
        .current_dir(&dir)
        .args(["-default", "+logcolorize", "+sound", "+logtofile"]);
    if let Some(data) = std::env::var_os("VICE_DATADIR") {
        command.arg("-directory").arg(data);
    }
    let output = command
        .args(["-moncommands", "probe.mon", "-limitcycles", "100000"])
        .output()
        .expect("VICE");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let roundtrip = std::fs::read_to_string(dir.join("roundtrip.vs")).expect("VICE saved symbols");
    let mut expected: Vec<_> = text.lines().collect();
    let mut actual: Vec<_> = roundtrip.lines().collect();
    expected.sort_unstable();
    actual.sort_unstable();
    assert_eq!(actual, expected);
}

#[test]
#[ignore = "requires SAMEBOY_SYMBOL_PROBE built from support/sameboy_symbols.c"]
fn sameboy_resolves_generated_banked_symbol() {
    let dir = scratch::dir("sameboy-symbol-consumer");
    let result = asm198x::assemble_rgbasm(GB).expect("assembly");
    let path = dir.join("game.sym");
    std::fs::write(
        &path,
        render(&result, SymbolFormat::NoCash).expect("export"),
    )
    .expect("symbols");
    let output =
        Command::new(std::env::var_os("SAMEBOY_SYMBOL_PROBE").expect("SAMEBOY_SYMBOL_PROBE"))
            .arg(path)
            .output()
            .expect("SameBoy probe");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}
