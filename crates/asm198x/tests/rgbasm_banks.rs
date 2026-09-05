//! RGBDS 1.0.3 placement evidence for #503.
use asm198x::assemble_rgbasm;

#[path = "support/scratch.rs"]
mod scratch;
#[path = "support/tool_identity.rs"]
mod tool_identity;

const SOURCE: &str = "SECTION \"entry\", ROM0[$150]\nEntry: nop\nSECTION \"bank3\", ROMX[$4010], BANK[3]\nBanked: db $33\nSECTION \"ram\", SRAM[$A010], BANK[2]\nSave: ds 1\nSECTION \"work\", WRAMX[$D010], BANK[3]\nWork: ds 1\nSECTION \"auto\", WRAMX\nAuto: ds 1\nSECTION \"vram\", VRAM[$8010], BANK[1]\nVideo: ds 1\n";

#[test]
fn bank_identity_is_independent_of_rom_file_placement() {
    let result = assemble_rgbasm(SOURCE).expect("banked assembly");
    for (name, bank, address) in [
        ("Entry", 0, 0x150),
        ("Banked", 3, 0x4010),
        ("Save", 2, 0xA010),
        ("Work", 3, 0xD010),
        ("Auto", 1, 0xD000),
        ("Video", 1, 0x8010),
    ] {
        assert_eq!(result.debug.symbol_banks[name], bank, "{name}");
        assert_eq!(result.symbols[name], address, "{name}");
    }
    assert_eq!(result.bytes.len(), 4 * 0x4000);
    assert_eq!(result.bytes[0xC010], 0x33);
    assert_eq!(result.bytes.iter().filter(|b| **b != 0).count(), 1);
    assert!(
        result.debug.symbol_pages.is_empty(),
        "RAM and ROM are not one paged device"
    );
    let formatted = asm198x::format_rgbasm(SOURCE).expect("format source");
    let roundtrip = assemble_rgbasm(&formatted).expect("formatted assembly");
    assert_eq!(roundtrip.bytes, result.bytes);
    assert_eq!(roundtrip.debug.symbol_banks, result.debug.symbol_banks);
}

#[test]
fn floating_ram_placement_does_not_reserve_addresses_in_other_banks() {
    let result = assemble_rgbasm("SECTION \"fixed\", WRAMX[$D000], BANK[3]\nFixed: ds 16\nSECTION \"floating\", WRAMX\nFloating: ds 1\nSECTION \"rom\", ROM0[0]\ndb BANK(\"fixed\"), BANK(\"floating\")\n").expect("RAM banks");
    assert_eq!(result.symbols["Fixed"], 0xD000);
    assert_eq!(result.symbols["Floating"], 0xD000);
    assert_eq!(result.bytes, [3, 1]);
}

#[test]
fn bank_capture_is_additive_and_excludes_constants() {
    let result = assemble_rgbasm("SECTION \"rom\", ROM0[0]\nAddress: db 1\nConstant EQU 0\n")
        .expect("assembly");
    assert!(!result.debug.symbol_banks.contains_key("Constant"));
    let mut json = serde_json::to_value(&result).expect("serialize");
    let restored: asm198x::AssemblyResult =
        serde_json::from_value(json.clone()).expect("deserialize");
    assert_eq!(restored.debug.symbol_banks, result.debug.symbol_banks);
    json["debug"]
        .as_object_mut()
        .expect("debug object")
        .remove("symbol_banks");
    let old: asm198x::AssemblyResult = serde_json::from_value(json).expect("old payload");
    assert!(old.debug.symbol_banks.is_empty());
}

#[test]
fn included_and_conditional_sections_keep_only_live_bank_metadata() {
    let loader = asm198x::source::MemoryLoader::new().text(
        "bank.inc",
        "SECTION \"code\", ROMX[$4010], BANK[2]\nIncluded: db 2\n",
    );
    let source = "IF 0\nSECTION \"dead\", ROMX, BANK[7]\nDead: db 7\nENDC\nINCLUDE \"bank.inc\"\n";
    let result =
        asm198x::assemble_rgbasm_files(source, "main.asm", &loader).expect("included section");
    assert_eq!(result.debug.symbol_banks.len(), 1);
    assert_eq!(result.debug.symbol_banks["Included"], 2);
    assert_eq!(result.symbols["Included"], 0x4010);
    assert_eq!(result.bytes[0x8010], 2);
}

#[test]
fn bank_attributes_are_not_section_name_text() {
    let source = "SECTION \"BANK[9], name\", ROMX[$4010], BANK [3]\nHere: db 1\n";
    let result = assemble_rgbasm(source).expect("attribute parsing");
    assert_eq!(result.symbols["Here"], 0x4010);
    assert_eq!(result.debug.symbol_banks["Here"], 3);
    assert_eq!(result.bytes[0xC010], 1);
}

#[test]
fn invalid_bank_attributes_are_rejected_before_image_allocation() {
    for (kind, banks) in [
        ("ROM0", vec![0, 1]),
        ("WRAM0", vec![0, 1]),
        ("OAM", vec![0, 1]),
        ("HRAM", vec![0, 1]),
        ("ROMX", vec![-1, 0, 65536]),
        ("WRAMX", vec![-1, 0, 8]),
        ("SRAM", vec![-1, 256]),
        ("VRAM", vec![-1, 2]),
    ] {
        for bank in banks {
            let source = format!("SECTION \"bad\", {kind}, BANK[{bank}]\ndb 1\n");
            assert!(assemble_rgbasm(&source).is_err(), "{source}");
        }
    }
}

#[test]
#[ignore = "requires RGBDS 1.0.3"]
fn matches_rgblink_bytes_and_every_symbol_bank() {
    let dir = scratch::dir("rgbasm-banks");
    std::fs::write(dir.join("input.asm"), SOURCE).expect("write fixture");
    for (tool, args) in [
        ("rgbasm", vec!["-o", "input.o", "input.asm"]),
        (
            "rgblink",
            vec!["-o", "native.gb", "-n", "native.sym", "input.o"],
        ),
    ] {
        let identity = tool_identity::identify(tool).expect("reference identity");
        assert!(identity.identity.contains("1.0.3"), "{identity:?}");
        eprintln!("{identity:?}");
        let output = std::process::Command::new(identity.path)
            .args(args)
            .current_dir(&dir)
            .output()
            .expect("reference execution");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let result = assemble_rgbasm(SOURCE).expect("assembly");
    let native_bytes = std::fs::read(dir.join("native.gb")).expect("native ROM");
    assert_eq!(result.bytes.len(), native_bytes.len(), "ROM length");
    assert!(result.bytes == native_bytes, "ROM content differs");
    let native = std::fs::read_to_string(dir.join("native.sym")).expect("native symbols");
    let mut count = 0;
    for line in native
        .lines()
        .filter(|s| !s.starts_with(';') && !s.is_empty())
    {
        let (location, name) = line.split_once(' ').expect("symbol row");
        let (bank, address) = location.split_once(':').expect("banked address");
        assert_eq!(
            result.debug.symbol_banks[name],
            u32::from_str_radix(bank, 16).expect("bank")
        );
        assert_eq!(
            result.symbols[name],
            i64::from_str_radix(address, 16).expect("address")
        );
        count += 1;
    }
    assert_eq!(count, result.debug.symbol_banks.len());
}
