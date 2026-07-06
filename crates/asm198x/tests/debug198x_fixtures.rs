//! Debug198x U6 — the conformance fixture corpus (plan R10). Always-on: no
//! reference assemblers needed, so CI enforces the format from day one, unlike
//! the `#[ignore]`d reference-arbitrated suites (KTD6).
//!
//! Each generated family asserts the **exact expected sidecar** (the producing
//! tool's version normalized so expected files stay byte-stable across release
//! bumps — the *format* version is asserted exactly) plus reader-lookup
//! behavior (AE1), corpus-wide byte-identity between the debug and plain
//! entries (AE2), the 24-bit/no-fabrication address posture (AE3), and
//! unknown-record tolerance (AE5). The hand-authored Spectrum 128 fixture
//! validates the banked *shape* two ways here — cross-bank lookups exercised
//! as data, and the committed SLD projection table
//! (`spectrum128-banked-sld.md`) — with the third leg (the Emu198x paging
//! cross-check) a cross-repo freeze-checklist item, not automatable in this
//! tree.

use asm198x::debug198x::{BaseMap, DebugInfo};

fn fixture(name: &str) -> String {
    let path = format!(
        "{}/tests/fixtures/debug198x/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read fixture {path}: {e}"))
}

/// Normalize the producing tool's version so expected sidecars survive release
/// bumps. (The plan sketched an injectable writer version; normalizing at the
/// comparison achieves the same byte-stability with no extra API surface.)
/// Everything else — including `format_version` — is compared exactly.
fn normalize(ndjson: &str) -> String {
    let mut out = String::with_capacity(ndjson.len());
    for line in ndjson.lines() {
        let mut line = line.to_string();
        if let Some(start) = line.find("\"tool_version\":\"") {
            let vstart = start + "\"tool_version\":\"".len();
            if let Some(vlen) = line[vstart..].find('"') {
                line.replace_range(vstart..vstart + vlen, "*");
            }
        }
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// One generated family: its fixture basename and the actual sidecar + image
/// bytes produced by the library entry the CLI uses.
struct Family {
    name: &'static str,
    actual: DebugInfo,
    debug_bytes: Vec<u8>,
    plain_bytes: Vec<u8>,
}

/// A family's debug-capturing producer: `(source, source_name)` to the record
/// plus the image bytes.
type Produce = dyn Fn(&str, &str) -> (DebugInfo, Vec<u8>);

fn families() -> Vec<Family> {
    let build = |name: &'static str, produce: &Produce, plain: &dyn Fn(&str) -> Vec<u8>| {
        let source = fixture(&format!("{name}.s"));
        let source_name = format!("{name}.s");
        let (actual, debug_bytes) = produce(&source, &source_name);
        Family {
            name,
            actual,
            debug_bytes,
            plain_bytes: plain(&source),
        }
    };
    vec![
        build(
            "z80-spectrum",
            &|src, path| {
                let r = asm198x::assemble_pasmo(src).expect("z80 assembles");
                (asm198x::debug_info(&r, "z80", "pasmo", path), r.bytes)
            },
            &|src| asm198x::assemble_pasmo(src).expect("z80 assembles").bytes,
        ),
        build(
            "6502-c64",
            &|src, path| {
                let r = asm198x::assemble_acme(src).expect("acme assembles");
                (asm198x::debug_info(&r, "6502", "acme", path), r.bytes)
            },
            &|src| asm198x::assemble_acme(src).expect("acme assembles").bytes,
        ),
        build(
            "6502-nes",
            &|src, path| {
                let (r, info) = asm198x::assemble_ca65_debug(src, path).expect("ca65 links");
                (info, r.bytes)
            },
            &|src| asm198x::assemble_ca65(src).expect("ca65 links").bytes,
        ),
        build(
            "68000-amiga",
            &|src, path| {
                let (r, info) = asm198x::assemble_vasm_exe_debug(src, path).expect("vasm links");
                (info, r.bytes)
            },
            &|src| asm198x::assemble_vasm_exe(src).expect("vasm links").bytes,
        ),
        build(
            "cp1610-intellivision",
            &|src, path| {
                let r = asm198x::assemble_cp1610(src).expect("cp1610 assembles");
                (asm198x::debug_info(&r, "cp1610", "asl", path), r.bytes)
            },
            &|src| {
                asm198x::assemble_cp1610(src)
                    .expect("cp1610 assembles")
                    .bytes
            },
        ),
        build(
            "65816-sample",
            &|src, path| {
                let r = asm198x::assemble_ca65_816(src).expect("65816 assembles");
                (asm198x::debug_info(&r, "65816", "ca65", path), r.bytes)
            },
            &|src| {
                asm198x::assemble_ca65_816(src)
                    .expect("65816 assembles")
                    .bytes
            },
        ),
    ]
}

/// Every generated family reproduces its expected sidecar byte-for-byte (tool
/// version normalized; format version exact), and the expected file itself
/// parses back to the same record (writer/reader agreement).
#[test]
fn corpus_sidecars_match_expected_exactly() {
    for family in families() {
        let expected = fixture(&format!("{}.debug198x", family.name));
        let actual = family.actual.to_ndjson();
        assert_eq!(
            normalize(&actual),
            normalize(&expected),
            "{}: sidecar drifted from the committed fixture",
            family.name
        );
        let parsed = DebugInfo::read(&expected).expect("expected sidecar parses");
        assert_eq!(
            parsed.header.format_version, family.actual.header.format_version,
            "{}: format version is asserted exactly",
            family.name
        );
    }
}

/// AE2, corpus-wide: the image bytes from the debug-capturing entry are
/// identical to the plain entry's for every family.
#[test]
fn corpus_debug_capture_never_changes_bytes() {
    for family in families() {
        assert_eq!(
            family.debug_bytes, family.plain_bytes,
            "{}: debug capture changed the image",
            family.name
        );
    }
}

/// AE1 per family: a label resolves via `addr_of` and its address maps back to
/// the defining source line via `line_at` — the reader lookups a debugger uses.
#[test]
fn corpus_lookups_resolve() {
    // (family, symbol, expected absolute address or (with bases) resolved
    // address, defining line of the first instruction checked)
    let z80 = DebugInfo::read(&fixture("z80-spectrum.debug198x")).expect("parse");
    assert_eq!(z80.addr_of("start", None), Some(0x8000));
    assert_eq!(z80.line_at(0x8000, None).map(|l| l.line), Some(4));
    assert_eq!(z80.addr_of("msg", None), Some(0x8007));

    let c64 = DebugInfo::read(&fixture("6502-c64.debug198x")).expect("parse");
    assert_eq!(c64.addr_of("start", None), Some(0xC000));
    assert_eq!(c64.addr_of("data", None), Some(0xC009));
    assert_eq!(c64.line_at(0xC005, None).map(|l| l.line), Some(7), "dex");

    let nes = DebugInfo::read(&fixture("6502-nes.debug198x")).expect("parse");
    assert_eq!(nes.addr_of("reset", None), Some(0x8000));
    assert_eq!(nes.addr_of("pos", None), Some(0));
    assert_eq!(nes.line_at(0x8004, None).map(|l| l.line), Some(7), "jmp");

    let amiga = DebugInfo::read(&fixture("68000-amiga.debug198x")).expect("parse");
    let bases: BaseMap = [(0, 0x2000), (1, 0x8000)].into_iter().collect();
    assert_eq!(amiga.addr_of("data", Some(&bases)), Some(0x8000));
    assert_eq!(
        amiga.line_at(0x2006, Some(&bases)).map(|l| l.line),
        Some(4),
        "dbf"
    );

    let m816 = DebugInfo::read(&fixture("65816-sample.debug198x")).expect("parse");
    assert_eq!(m816.addr_of("done", None), Some(6));
    assert_eq!(m816.line_at(0, None).map(|l| l.line), Some(5), "lda #");

    // CP1610 — the one word-addressed family: offsets, lengths, and addresses
    // are in the CPU's address units (decles), not bytes. The 12-byte image is
    // 6 decles, and the spans account for exactly those 6.
    let intv = DebugInfo::read(&fixture("cp1610-intellivision.debug198x")).expect("parse");
    assert_eq!(intv.addr_of("start", None), Some(0x5000));
    assert_eq!(intv.addr_of("done", None), Some(0x5005), "decle address");
    assert_eq!(intv.line_at(0x5003, None).map(|l| l.line), Some(5), "bneq");
    assert_eq!(
        intv.lines.iter().map(|l| l.length).sum::<u64>(),
        6,
        "span lengths are decles: half the byte count"
    );
}

/// AE3: the 65816 record carries a 24-bit constant from actual placement, and
/// the flat Z80 record fabricates no address-space data (no `space` key at all).
#[test]
fn corpus_address_space_posture() {
    let m816 = fixture("65816-sample.debug198x");
    assert!(
        m816.contains("\"value\":98304"),
        "the 65816 FARBUF constant carries its 24-bit value ($018000)"
    );
    let z80 = fixture("z80-spectrum.debug198x");
    assert!(
        !z80.contains("\"space\""),
        "a flat Z80 record carries no fabricated bank/space data"
    );
}

/// AE5: a record with an unknown `t` is skipped by the reader, and every AE1
/// lookup still resolves.
#[test]
fn corpus_reader_skips_unknown_records() {
    let mut doctored = String::new();
    for (i, line) in fixture("z80-spectrum.debug198x").lines().enumerate() {
        doctored.push_str(line);
        doctored.push('\n');
        if i == 1 {
            doctored.push_str("{\"t\":\"hologram\",\"a_future_field\":true,\"payload\":[1,2,3]}\n");
        }
    }
    let info = DebugInfo::read(&doctored).expect("unknown record type is skipped, not fatal");
    assert_eq!(info.addr_of("start", None), Some(0x8000));
    assert_eq!(info.line_at(0x8000, None).map(|l| l.line), Some(4));
}

/// The hand-authored Spectrum 128 banked fixture (R10): the same CPU address
/// resolves to two distinct symbols and source lines depending on which bank
/// the `BaseMap` pages in — the paging state *is* the base map. Leg 2 is the
/// committed SLD projection table; leg 3 (Emu198x paging cross-check) is a
/// freeze-checklist item.
#[test]
fn banked_fixture_resolves_per_paging_state() {
    let info = DebugInfo::read(&fixture("spectrum128-banked.debug198x")).expect("parse");

    // No bank paged in: nothing resolves — pageable sections carry no base.
    assert_eq!(info.addr_of("draw", None), None);
    assert_eq!(info.symbol_at(0xC010, None).map(|s| s.name.as_str()), None);

    // Bank 1 in slot 3.
    let bank1_in: BaseMap = [(0, 0xC000)].into_iter().collect();
    assert_eq!(
        info.symbol_at(0xC010, Some(&bank1_in)).map(|s| &*s.name),
        Some("draw")
    );
    assert_eq!(
        info.line_at(0xC010, Some(&bank1_in)).map(|l| l.line),
        Some(5)
    );

    // Bank 3 in slot 3: the same CPU address now names the other routine.
    let bank3_in: BaseMap = [(1, 0xC000)].into_iter().collect();
    assert_eq!(
        info.symbol_at(0xC010, Some(&bank3_in)).map(|s| &*s.name),
        Some("music")
    );
    assert_eq!(
        info.line_at(0xC010, Some(&bank3_in)).map(|l| l.line),
        Some(12)
    );

    // The paged qualifier is carried as data on both symbols (the banked shape
    // the emission paths will populate later).
    let space_of = |name: &str| {
        info.symbols
            .iter()
            .find(|s| s.name == name)
            .map(|s| match &s.kind {
                asm198x::debug198x::SymbolKind::Label { space, .. } => space.clone(),
                _ => None,
            })
    };
    assert_eq!(
        space_of("draw"),
        Some(Some(asm198x::debug198x::Space::Paged { slot: 3, page: 1 }))
    );
    assert_eq!(
        space_of("music"),
        Some(Some(asm198x::debug198x::Space::Paged { slot: 3, page: 3 }))
    );
}
