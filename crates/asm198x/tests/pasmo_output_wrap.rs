use serde_json::Value;

fn facts() -> Value {
    serde_json::from_str(include_str!("fixtures/pasmo-output-wrap.json")).expect("native facts")
}

#[test]
fn native_images_and_symbols_match_both_pasmo_frontends() {
    for case in facts()["cases"].as_array().expect("cases") {
        let source = case["source"].as_str().expect("source");
        let name = case["name"].as_str().expect("name");
        for (assemble, format) in [
            (
                asm198x::assemble_pasmo
                    as fn(&str) -> Result<asm198x::AssemblyResult, asm198x::AsmError>,
                asm198x::format_pasmo as fn(&str) -> Result<String, asm198x::AsmError>,
            ),
            (
                asm198x::assemble_pasmonext
                    as fn(&str) -> Result<asm198x::AssemblyResult, asm198x::AsmError>,
                asm198x::format_pasmonext as fn(&str) -> Result<String, asm198x::AsmError>,
            ),
        ] {
            let result = assemble(source);
            if case["exit"] != 0 {
                assert!(result.is_err(), "{name}: reference refuses source");
                continue;
            }
            let result = result.expect(name);
            let formatted = format(source).expect("format native source");
            assert_eq!(
                format(&formatted).expect("format twice"),
                formatted,
                "{name}"
            );
            let reassembled = assemble(&formatted).expect("assemble formatted source");
            assert_eq!(reassembled.bytes, result.bytes, "{name}: formatter bytes");
            assert_eq!(
                reassembled.symbols, result.symbols,
                "{name}: formatter symbols"
            );
            let warned = case["stderr"]
                .as_str()
                .expect("native stderr")
                .contains("64KB limit passed");
            assert_eq!(
                result
                    .warnings
                    .iter()
                    .any(|w| w.message.contains("64KB limit passed")),
                warned,
                "{name}: wrapping warning"
            );
            assert!(
                !result
                    .warnings
                    .iter()
                    .any(|w| w.message.contains("overlap")),
                "{name}: no overlap warning"
            );
            let hex: String = result
                .bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            assert_eq!(hex, case["hex"].as_str().expect("native bytes"), "{name}");
            assert_eq!(
                result.areas[0].used,
                result.bytes.len() as u32,
                "{name}: memory account"
            );
            for line in case["symbols"].as_str().expect("symbols").lines() {
                let parts: Vec<_> = line.split_whitespace().collect();
                let value = i64::from_str_radix(parts[2].trim_end_matches('H'), 16)
                    .expect("native address");
                assert_eq!(result.symbols.get(parts[0]), Some(&value), "{name}: {line}");
            }
            let base = u64::from(result.origin.expect("origin") - result.reserved_prefix);
            for symbol in &result.debug.symbols {
                if let debug198x::SymbolKind::Label { offset, .. }
                | debug198x::SymbolKind::Entry { offset, .. } = symbol.kind
                {
                    assert_eq!(
                        base + offset,
                        result
                            .symbols
                            .get(&symbol.name)
                            .copied()
                            .unwrap_or(i64::from(result.start.unwrap_or(0)))
                            as u64,
                        "{name}: {}",
                        symbol.name
                    );
                }
            }
            for line in &result.debug.lines {
                assert!(
                    base + line.offset + line.length <= 65536,
                    "{name}: line outside physical address space"
                );
            }
        }
    }
}

#[test]
fn debug_sidecars_and_listings_use_physical_addresses_and_image_bytes() {
    for name in ["two_cross", "subsequent_pc", "end_label"] {
        let facts = facts();
        let case = facts["cases"]
            .as_array()
            .expect("cases")
            .iter()
            .find(|case| case["name"] == name)
            .expect("case");
        let source = case["source"].as_str().expect("source");
        let result = asm198x::assemble_pasmo(source).expect("assemble");
        let info = asm198x::debug_info(&result, "z80", "pasmo", "wrap.asm");
        for symbol in &info.symbols {
            if let debug198x::SymbolKind::Label {
                section, offset, ..
            }
            | debug198x::SymbolKind::Entry {
                section, offset, ..
            } = symbol.kind
            {
                let section = info
                    .sections
                    .iter()
                    .find(|s| s.id == section)
                    .expect("section");
                let address = section.base.expect("physical base") + offset;
                assert!(address < 65536, "{name}: {}", symbol.name);
                if let Some(value) = result.symbols.get(&symbol.name) {
                    assert_eq!(address, *value as u64, "{name}: {}", symbol.name);
                }
            }
        }
        let listing: Value =
            serde_json::from_str(&asm198x::render_listing_json("wrap.asm", &result, 1))
                .expect("listing");
        for line in listing["lines"].as_array().expect("lines") {
            assert!(
                line["address"].as_u64().expect("address")
                    + line["bytes"].as_u64().expect("length")
                    <= 65536
            );
        }
        let text = asm198x::render_listing(source, &result, 1);
        assert!(text.contains("FFFF  A5"), "{name}: {text}");
        if name != "end_label" {
            let addresses: Vec<_> = listing["lines"]
                .as_array()
                .expect("lines")
                .iter()
                .map(|line| line["address"].as_u64().expect("address"))
                .collect();
            assert!(addresses.contains(&0), "wrapped span remains visible");
        } else {
            assert!(asm198x::render_sym(&info).contains("after = $0000"));
        }
    }
}

#[test]
fn other_dialects_keep_their_address_space_limit() {
    for result in [
        asm198x::assemble_sjasmplus(" org $ffff\n db 1,2\n"),
        asm198x::assemble_acme("*=$ffff\n !byte 1,2\n"),
    ] {
        assert!(result.expect_err("must not wrap").message.contains("64K"));
    }
}
