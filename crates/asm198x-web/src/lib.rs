//! A `wasm-bindgen` shell over `asm198x` (#493).
//!
//! The surface is deliberately dumb: source in, the CLI's JSON out. It exists so
//! a web page can call the assembler; everything it returns is the contract the
//! command line already documents, so a playground and a script reading
//! `asm198x --json` see the same shape and nothing here needs documenting twice.
//!
//! Includes resolve through an empty [`MemoryLoader`] — one source, no files —
//! which is exactly a browser. A multi-file surface can arrive when a consumer
//! needs one; the loader seam is already there.

use asm198x::source::MemoryLoader;
use asm198x::{AssemblyResult, MultiFileError};
use wasm_bindgen::prelude::*;

/// The shape every multi-file library entry shares (`assemble_*_files`).
type Entry =
    fn(&str, &str, &dyn asm198x::source::SourceLoader) -> Result<AssemblyResult, MultiFileError>;

/// The path the root source is known by in diagnostics and the file table.
/// A browser has no filename; this is the placeholder the messages carry.
const ROOT: &str = "input.asm";

/// The library entry for a dialect name, or `None` when nothing accepts it.
/// Every canonical name in [`asm198x::dialect_table::DIALECTS`] has an arm
/// here — a test holds that — and aliases collapse through `canonical` first.
fn entry(dialect: &str) -> Option<Entry> {
    Some(match asm198x::dialect_table::canonical(dialect)? {
        "acme" => asm198x::assemble_acme_files,
        "ca65" => asm198x::assemble_ca65_files,
        "65816" => asm198x::assemble_ca65_816_files,
        "huc6280" => asm198x::assemble_ca65_huc6280_files,
        // The CLI's default for vasm: a flat binary, warnings kept. The hunk
        // executable is behind `--exe` there and behind nothing here yet.
        "vasm" => asm198x::assemble_vasm_warned_files,
        "lwasm" => asm198x::assemble_lwasm_files,
        "rgbasm" => asm198x::assemble_rgbasm_files,
        "pasmo" => asm198x::assemble_pasmo_files,
        "pasmonext" => asm198x::assemble_pasmonext_files,
        // Plain Z80; the Next target is the CLI's `--cpu z80n`, which this
        // surface does not take yet.
        "sjasmplus" => asm198x::assemble_sjasmplus_files,
        "8080" => asm198x::assemble_i8080_files,
        "6800" => asm198x::assemble_m6800_files,
        "1802" => asm198x::assemble_1802_files,
        "8048" => asm198x::assemble_8048_files,
        "8035" => asm198x::assemble_8039_files,
        "scmp" => asm198x::assemble_scmp_files,
        "f8" => asm198x::assemble_f8_files,
        "2650" => asm198x::assemble_2650_files,
        "tms7000" => asm198x::assemble_tms7000_files,
        "pdp11" => asm198x::assemble_pdp11_files,
        "tms9900" => asm198x::assemble_tms9900_files,
        "cp1610" => asm198x::assemble_cp1610_files,
        "z8000" => asm198x::assemble_z8000_files,
        "z8001" => asm198x::assemble_z8001_files,
        _ => return None,
    })
}

/// Bytes per address unit for the listing's bytes column — 2 for the
/// word-addressed CP1610, 1 for every byte-addressed CPU (the CLI's rule).
fn addr_unit(dialect: &str) -> u64 {
    match asm198x::dialect_table::canonical(dialect) {
        Some("cp1610") => 2,
        _ => 1,
    }
}

/// Assemble `source` in `dialect` and hand back the CLI's `--json` payload.
///
/// On success the string is the `AssemblyResult` object (bytes, origin,
/// symbols, warnings, debug info); on failure it is the diagnostics array,
/// each span carrying its resolved `path`. `null` in JavaScript when `dialect`
/// is not a name `asm198x dialects` lists.
#[wasm_bindgen]
#[must_use]
pub fn assemble(dialect: &str, source: &str) -> Option<String> {
    let entry = entry(dialect)?;
    let json = match entry(source, ROOT, &MemoryLoader::new()) {
        Ok(result) => serde_json::to_string(&result),
        Err(error) => {
            let files = error.source_map.file_table();
            let mut diagnostic = asm198x::Diagnostic::from(error.error);
            diagnostic.span = diagnostic
                .span
                .map(|span| asm198x::resolve_span_path(span, &files));
            serde_json::to_string(&[diagnostic])
        }
    };
    // Serialising the contract types cannot fail: every field is a plain
    // value or a map with string keys.
    json.ok()
}

/// Assemble `source` in `dialect` and hand back a 48K `.sna` snapshot.
///
/// The same bytes `asm198x --dialect pasmonext --sna` writes, so a page that
/// assembles and then runs the result is running what the command line would
/// have produced. Spectrum Z80 only, and the source needs `end <addr>` for its
/// entry point, exactly as the CLI demands.
///
/// `null` when the dialect is unknown, the source does not assemble, or it has
/// no entry point — [`assemble`] says which.
#[wasm_bindgen]
#[must_use]
pub fn snapshot(dialect: &str, source: &str) -> Option<Vec<u8>> {
    let result = entry(dialect)?(source, ROOT, &MemoryLoader::new()).ok()?;
    asm198x::sna_48k(&result).ok()
}

/// The `--listing` rendering of `source` in `dialect`: address, bytes, and
/// source per line. `null` when the dialect is unknown or the source does not
/// assemble — [`assemble`] says why.
#[wasm_bindgen]
#[must_use]
pub fn listing(dialect: &str, source: &str) -> Option<String> {
    let result = entry(dialect)?(source, ROOT, &MemoryLoader::new()).ok()?;
    Some(asm198x::render_listing(source, &result, addr_unit(dialect)))
}

/// Every dialect `assemble` accepts, as a JSON array of
/// `{ "name", "aliases", "blurb" }` in the order the CLI reference presents
/// them — the same table `asm198x dialects` prints, so a picker built from
/// this can never offer a name the assembler refuses.
#[wasm_bindgen]
#[must_use]
pub fn dialects() -> String {
    let table: Vec<serde_json::Value> = asm198x::dialect_table::DIALECTS
        .iter()
        .map(|d| {
            serde_json::json!({
                "name": d.name,
                "aliases": d.aliases,
                "blurb": d.blurb,
            })
        })
        .collect();
    serde_json::Value::Array(table).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The dialect table is the one list of selectable names; a row it has
    /// that `entry` does not fails here rather than returning `null` to a
    /// page that offered the name.
    #[test]
    fn every_listed_dialect_has_an_entry() {
        for d in asm198x::dialect_table::DIALECTS {
            assert!(entry(d.name).is_some(), "no entry for `{}`", d.name);
            for alias in d.aliases {
                assert!(entry(alias).is_some(), "no entry for alias `{alias}`");
            }
        }
        assert!(entry("not-a-dialect").is_none());
    }

    /// The unit-01 program from the Spectrum course: set the border, hold the
    /// picture, and name an entry point.
    const BORDER: &str = "            org 32768\nstart:      ld a, 2\n            out ($FE), a\n.loop:      halt\n            jr .loop\n            end start\n";

    #[test]
    fn a_snapshot_is_a_48k_image() {
        // `unwrap_or_default` rather than `expect`: the crate denies
        // `expect_used`, and an empty vec fails the length assertion below
        // with the same clarity.
        let sna = snapshot("pasmonext", BORDER).unwrap_or_default();
        assert_eq!(
            sna.len(),
            49179,
            "a 48K .sna is 27 header bytes plus 48K; got {}",
            sna.len()
        );
    }

    /// `--sna` demands `end <addr>`, and a page assembling without one must
    /// get the same refusal the command line gives rather than a snapshot
    /// that starts wherever the machine happened to be pointing.
    #[test]
    fn a_snapshot_needs_an_entry_point() {
        let no_end = "            org 32768\n            ld a, 2\n            out ($FE), a\n";
        assert!(snapshot("pasmonext", no_end).is_none());
    }

    #[test]
    fn an_unknown_dialect_yields_no_snapshot() {
        assert!(snapshot("not-a-dialect", BORDER).is_none());
    }

    /// Snapshots are a Spectrum Z80 output. Asking a 6502 dialect for one
    /// must decline rather than emit something shaped like a Spectrum.
    #[test]
    fn a_non_spectrum_dialect_yields_no_snapshot() {
        assert!(snapshot("acme", BORDER).is_none());
    }
}
