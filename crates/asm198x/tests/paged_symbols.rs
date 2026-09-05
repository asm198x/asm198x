//! Symbol placement prerequisite for #503. Reference projections are from
//! SjASMPlus 1.21.0 CSPECTMAP and SLD, not inferred from the raw output layout.

use asm198x::{PagedLocation, assemble_sjasmplus};

#[path = "support/scratch.rs"]
mod scratch;
#[path = "support/tool_identity.rs"]
mod tool_identity;

const BANKED: &str = " DEVICE ZXSPECTRUM128\n ORG $C010\n PAGE 1\ndraw: db 1\n PAGE 3\n ORG $C010\nmusic: db 3\nanswer EQU $C010\n END draw\n";

fn check(location: &PagedLocation, slot: u8, page: u16, size: u32, offset: u32) {
    assert_eq!(location.slot, slot);
    assert_eq!(location.page, page);
    assert_eq!(location.page_size, size);
    assert_eq!(location.offset, offset);
}

#[test]
fn equal_logical_addresses_keep_distinct_pages_and_entry_placement() {
    let result = assemble_sjasmplus(BANKED).unwrap();
    assert_eq!(result.bytes, [1, 3]);
    assert_eq!(result.symbols["draw"], result.symbols["music"]);
    let pages = &result.debug.symbol_pages;
    check(&pages["draw"], 3, 1, 0x4000, 0x10);
    check(&pages["music"], 3, 3, 0x4000, 0x10);
    assert_eq!(pages["draw"].logical_address(), 0xC010);
    assert_eq!(pages["music"].logical_address(), 0xC010);
    assert_eq!(pages["draw"].physical_address(), 0x4010);
    assert_eq!(pages["music"].physical_address(), 0xC010);
    assert!(!pages.contains_key("answer"));
    assert!(result.debug.symbols.iter().any(
        |s| s.name == "draw" && matches!(s.kind, asm198x::debug198x::SymbolKind::Entry { .. })
    ));
}

#[test]
fn page_geometry_comes_from_device_and_address_not_selected_slot() {
    let result = assemble_sjasmplus(
        " DEVICE ZXSPECTRUMNEXT\n ORG $E010\n PAGE 223\nhigh: db 1\n SLOT 0\n PAGE 9\nstill_high: db 2\n ORG $0010\nlow: db 3\n",
    ).unwrap();
    let pages = &result.debug.symbol_pages;
    check(&pages["high"], 7, 223, 0x2000, 0x10);
    check(&pages["still_high"], 7, 223, 0x2000, 0x11);
    check(&pages["low"], 0, 9, 0x2000, 0x10);
    assert_eq!(pages["high"].physical_address(), 0x1BE010);
    assert_eq!(result.bytes, [1, 2, 3]);
}

#[test]
fn mapping_changes_do_not_rewrite_earlier_symbols_or_fabricate_flat_pages() {
    let result = assemble_sjasmplus(
        " ORG $C010\nflat: db 0\n DEVICE ZXSPECTRUM128\n PAGE 1\none: db 1\n DEVICE NONE\nnone: db 2\n DEVICE ZXSPECTRUM128\nreset: db 3\n",
    ).unwrap();
    let pages = &result.debug.symbol_pages;
    assert_eq!(pages.len(), 2);
    check(&pages["one"], 3, 1, 0x4000, 0x11);
    check(&pages["reset"], 3, 3, 0x4000, 0x13);
    assert_eq!(result.bytes, [0, 1, 2, 3]);
}

#[test]
fn numeric_entry_uses_current_mapping_without_rewriting_a_label() {
    let result = assemble_sjasmplus(
        " DEVICE ZXSPECTRUM128\n ORG $C010\n PAGE 1\nfirst: db 1\n PAGE 3\n END $C010\n",
    )
    .unwrap();
    check(&result.debug.symbol_pages["first"], 3, 1, 0x4000, 0x10);
    check(&result.debug.symbol_pages["@entry"], 3, 3, 0x4000, 0x10);
}

#[test]
fn placement_is_additive_in_the_serialized_contract() {
    let result = assemble_sjasmplus(BANKED).unwrap();
    let mut json = serde_json::to_value(&result).unwrap();
    let restored: asm198x::AssemblyResult = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(restored.debug.symbol_pages, result.debug.symbol_pages);
    json["debug"]
        .as_object_mut()
        .unwrap()
        .remove("symbol_pages");
    let old: asm198x::AssemblyResult = serde_json::from_value(json).unwrap();
    assert!(old.debug.symbol_pages.is_empty());
    let flat = assemble_sjasmplus(" ORG $8000\nflat: nop\n").unwrap();
    assert!(
        serde_json::to_value(flat).unwrap()["debug"]
            .get("symbol_pages")
            .is_none()
    );
}

#[test]
fn label_on_page_directive_uses_mapping_before_the_directive() {
    let result = assemble_sjasmplus(
        " DEVICE ZXSPECTRUMNEXT\n ORG $E010\n PAGE 4\nbefore: PAGE 5\nafter: db 1\n",
    )
    .unwrap();
    check(&result.debug.symbol_pages["before"], 7, 4, 0x2000, 0x10);
    check(&result.debug.symbol_pages["after"], 7, 5, 0x2000, 0x10);
}

#[test]
fn remapping_the_same_page_to_another_slot_preserves_physical_identity() {
    let result = assemble_sjasmplus(
        " DEVICE ZXSPECTRUM128\n ORG $C010\n PAGE 1\nupper: db 1\n SLOT 2\n PAGE 1\n ORG $8010\nlower: db 2\n",
    ).unwrap();
    let upper = result.debug.symbol_pages["upper"];
    let lower = result.debug.symbol_pages["lower"];
    assert_ne!(upper.logical_address(), lower.logical_address());
    assert_eq!(upper.physical_address(), lower.physical_address());
    assert_eq!(upper.page, lower.page);
}

#[test]
fn all_device_geometries_preserve_the_highest_page() {
    for (device, page, slot, size) in [
        ("ZXSPECTRUM48", 3, 3, 0x4000),
        ("ZXSPECTRUM128", 7, 3, 0x4000),
        ("ZXSPECTRUM256", 15, 3, 0x4000),
        ("ZXSPECTRUM512", 31, 3, 0x4000),
        ("ZXSPECTRUM1024", 63, 3, 0x4000),
        ("ZXSPECTRUM2048", 127, 3, 0x4000),
        ("ZXSPECTRUM4096", 255, 3, 0x4000),
        ("ZXSPECTRUM8192", 511, 3, 0x4000),
        ("ZXSPECTRUMNEXT", 223, 7, 0x2000),
        ("AMSTRADCPC464", 3, 3, 0x4000),
        ("AMSTRADCPC6128", 7, 3, 0x4000),
        ("AMSTRADCPCPLUS", 31, 3, 0x4000),
        ("NOSLOT64K", 31, 0, 0x10000),
    ] {
        let source = format!(" DEVICE {device}\n ORG $FFFE\n PAGE {page}\nlast: db $42\n");
        let result = assemble_sjasmplus(&source).unwrap();
        let location = result.debug.symbol_pages["last"];
        check(&location, slot, page, size, size - 2);
        assert_eq!(location.logical_address(), 0xFFFE, "{device}");
        assert_eq!(result.bytes, [0x42], "{device}");
    }
}

#[test]
#[ignore = "requires SjASMPlus 1.21.0"]
fn page_placements_match_native_cspectmap_addresses() {
    let identity = tool_identity::identify("sjasmplus").expect("SjASMPlus identity");
    assert!(identity.identity.contains("1.21.0"), "{identity:?}");
    eprintln!("{identity:?}");
    let dir = scratch::dir("paged-symbols");
    // CSPECTMAP is requested before END so the native assembler sees it.
    let source = BANKED.replace(" END draw", " CSPECTMAP \"native.map\"\n END draw");
    std::fs::write(dir.join("input.asm"), source).unwrap();
    let output = std::process::Command::new(&identity.path)
        .current_dir(&dir)
        .args(["--nologo", "--raw=native.bin", "input.asm"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result = assemble_sjasmplus(BANKED).unwrap();
    assert_eq!(result.bytes, std::fs::read(dir.join("native.bin")).unwrap());
    let map = std::fs::read_to_string(dir.join("native.map")).unwrap();
    let mut labels = 0;
    for row in map.lines() {
        let fields: Vec<_> = row.split_whitespace().collect();
        assert_eq!(fields.len(), 4, "{row}");
        let name = fields[3].to_ascii_lowercase();
        if fields[2] == "01" {
            assert!(!result.debug.symbol_pages.contains_key(&name));
            continue;
        }
        assert_eq!(fields[2], "00", "{row}");
        let location = result.debug.symbol_pages[&name];
        assert_eq!(
            location.logical_address(),
            u64::from_str_radix(fields[0], 16).unwrap()
        );
        assert_eq!(
            location.physical_address(),
            u64::from_str_radix(fields[1], 16).unwrap()
        );
        labels += 1;
    }
    assert_eq!(labels, 2);
}
