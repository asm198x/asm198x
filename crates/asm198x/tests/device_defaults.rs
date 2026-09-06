use asm198x::assemble_sjasmplus;

#[path = "support/scratch.rs"]
mod scratch;

const DEVICES: &[(&str, &[u16])] = &[
    ("ZXSPECTRUM48", &[0, 1, 2, 3]),
    ("ZXSPECTRUM128", &[7, 5, 2, 0]),
    ("ZXSPECTRUM256", &[7, 5, 2, 0]),
    ("ZXSPECTRUM512", &[7, 5, 2, 0]),
    ("ZXSPECTRUM1024", &[7, 5, 2, 0]),
    ("ZXSPECTRUM2048", &[7, 5, 2, 0]),
    ("ZXSPECTRUM4096", &[7, 5, 2, 0]),
    ("ZXSPECTRUM8192", &[7, 5, 2, 0]),
    ("ZXSPECTRUMNEXT", &[14, 15, 10, 11, 4, 5, 0, 1]),
    ("AMSTRADCPC464", &[0, 1, 2, 3]),
    ("AMSTRADCPC6128", &[0, 1, 2, 3]),
    ("AMSTRADCPCPLUS", &[0, 1, 2, 3]),
    ("NOSLOT64K", &[0]),
];

fn source(device: &str, count: usize) -> String {
    let mut source = format!(" DEVICE {device}\n");
    for slot in 0..count {
        source.push_str(&format!(
            " ORG {}\nslot{slot}: db {slot}\n",
            slot * (0x10000 / count) + 0x10
        ));
    }
    source
}

#[test]
fn every_device_starts_with_the_reference_slot_mapping() {
    for &(device, pages) in DEVICES {
        let result = assemble_sjasmplus(&source(device, pages.len())).expect(device);
        for (slot, page) in pages.iter().enumerate() {
            let location = result.debug.symbol_pages[&format!("slot{slot}")];
            assert_eq!(location.page, *page, "{device}, slot {slot}");
            assert_eq!(usize::from(location.slot), slot);
            assert_eq!(location.offset, 0x10);
        }
    }
}

const SEED: &str = " DEVICE ZXSPECTRUM128\n ORG $8000\n db 0\n SAVEBIN \"before.bin\",$5800,$400\n PAGE 5\n SAVEBIN \"after.bin\",$D800,$400\n";
const RESTORE: &str = " DEVICE ZXSPECTRUM128\n ORG $C000\n PAGE 1\n db 42\n DEVICE ZXSPECTRUM48\n ORG $C000\n db 99\n DEVICE NONE\n DEVICE ZXSPECTRUM128\nrestored: SAVEBIN \"restored.bin\",$C000,1\n";

#[test]
fn returning_to_a_device_restores_its_mapping_and_memory() {
    let result = assemble_sjasmplus(RESTORE).expect("assembly");
    assert_eq!(result.artifacts[0].bytes, [42]);
    assert_eq!(result.debug.symbol_pages["restored"].page, 1);
}

#[test]
fn seeded_attributes_follow_the_physical_bank_when_remapped() {
    let result = assemble_sjasmplus(SEED).expect("assembly");
    assert_eq!(result.artifacts[0].bytes, result.artifacts[1].bytes);
    assert!(
        result.artifacts[0].bytes[..0x300]
            .iter()
            .all(|b| *b == 0x38)
    );
}

#[test]
#[ignore = "requires SjASMPlus 1.21.0"]
fn defaults_and_seeded_memory_match_native_sjasmplus() {
    let dir = scratch::dir("device-defaults");
    for &(device, pages) in DEVICES {
        let source = source(device, pages.len());
        std::fs::write(
            dir.join("input.asm"),
            format!("{source} CSPECTMAP \"native.map\"\n"),
        )
        .expect("source");
        let output = std::process::Command::new("sjasmplus")
            .current_dir(&dir)
            .args(["--nologo", "input.asm"])
            .output()
            .expect("reference");
        assert!(
            output.status.success(),
            "{device}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result = assemble_sjasmplus(&source).expect(device);
        let map = std::fs::read_to_string(dir.join("native.map")).expect("map");
        assert_eq!(map.lines().count(), pages.len());
        for row in map.lines() {
            let fields: Vec<_> = row.split_whitespace().collect();
            let name = fields[3].to_ascii_lowercase();
            assert_eq!(
                result.debug.symbol_pages[&name].physical_address(),
                u64::from_str_radix(fields[1], 16).expect("address"),
                "{device}: {row}"
            );
        }
    }
    for source in [SEED, RESTORE] {
        std::fs::write(dir.join("input.asm"), source).expect("source");
        let output = std::process::Command::new("sjasmplus")
            .current_dir(&dir)
            .args(["--nologo", "input.asm"])
            .output()
            .expect("reference");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        for artifact in assemble_sjasmplus(source).expect("assembly").artifacts {
            assert_eq!(
                artifact.bytes,
                std::fs::read(dir.join(artifact.name)).expect("native bytes")
            );
        }
    }
}
