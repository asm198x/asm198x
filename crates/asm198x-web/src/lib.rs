//! A `wasm-bindgen` shell over `asm198x` (#493).
//!
//! The surface is deliberately dumb: source in, the CLI's JSON out. It exists so
//! a web page can call the assembler; everything it returns is the contract the
//! command line already documents, so a playground and a script reading
//! `asm198x --json` see the same shape and nothing here needs documenting twice.
//!
//! The single-source calls resolve includes through an empty [`MemoryLoader`].
//! [`assemble_project`] accepts the named in-memory file set that a browser IDE
//! or lesson already owns and routes it through the same loader seam.

use asm198x::source::MemoryLoader;
use asm198x::{AssemblyResult, MultiFileError};
use serde::Deserialize;
use std::collections::BTreeMap;
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
        #[cfg(feature = "mos6502")]
        "acme" => asm198x::assemble_acme_files,
        #[cfg(feature = "mos6502")]
        "ca65" => asm198x::assemble_ca65_files,
        #[cfg(feature = "mos6502")]
        "65816" => asm198x::assemble_ca65_816_files,
        #[cfg(feature = "mos6502")]
        "huc6280" => asm198x::assemble_ca65_huc6280_files,
        // The CLI's default for vasm: a flat binary, warnings kept. The hunk
        // executable is behind `--exe` there and behind nothing here yet.
        #[cfg(feature = "m68k")]
        "vasm" => asm198x::assemble_vasm_warned_files,
        #[cfg(feature = "m6809")]
        "lwasm" => asm198x::assemble_lwasm_files,
        #[cfg(feature = "sm83")]
        "rgbasm" => asm198x::assemble_rgbasm_files,
        #[cfg(feature = "z80")]
        "pasmo" => asm198x::assemble_pasmo_files,
        #[cfg(feature = "z80")]
        "pasmonext" => asm198x::assemble_pasmonext_files,
        // Plain Z80; the Next target is the CLI's `--cpu z80n`, which this
        // surface does not take yet.
        #[cfg(feature = "z80")]
        "sjasmplus" => asm198x::assemble_sjasmplus_files,
        #[cfg(feature = "i8080")]
        "8080" => asm198x::assemble_i8080_files,
        #[cfg(feature = "m6800")]
        "6800" => asm198x::assemble_m6800_files,
        #[cfg(feature = "cdp1802")]
        "1802" => asm198x::assemble_1802_files,
        #[cfg(feature = "mcs48")]
        "8048" => asm198x::assemble_8048_files,
        #[cfg(feature = "mcs48")]
        "8035" => asm198x::assemble_8039_files,
        #[cfg(feature = "scmp")]
        "scmp" => asm198x::assemble_scmp_files,
        #[cfg(feature = "f8")]
        "f8" => asm198x::assemble_f8_files,
        #[cfg(feature = "s2650")]
        "2650" => asm198x::assemble_2650_files,
        #[cfg(feature = "tms7000")]
        "tms7000" => asm198x::assemble_tms7000_files,
        #[cfg(feature = "pdp11")]
        "pdp11" => asm198x::assemble_pdp11_files,
        #[cfg(feature = "tms9900")]
        "tms9900" => asm198x::assemble_tms9900_files,
        #[cfg(feature = "cp1610")]
        "cp1610" => asm198x::assemble_cp1610_files,
        #[cfg(feature = "z8000")]
        "z8000" => asm198x::assemble_z8000_files,
        #[cfg(feature = "z8000")]
        "z8001" => asm198x::assemble_z8001_files,
        _ => return None,
    })
}

/// Select a dialect's target and browser-meaningful output mode. Empty values
/// retain the dialect's ordinary default, as the existing [`assemble`] call
/// does. The only syntax with a selectable CPU today is the Z80 pair; the only
/// additional native output that needs no filesystem is vasm's Hunk image.
fn project_entry(dialect: &str, target: &str, output: &str) -> Option<Entry> {
    let canonical = asm198x::dialect_table::canonical(dialect)?;
    let target = target.to_ascii_lowercase();
    let output = output.to_ascii_lowercase();

    if !matches!(output.as_str(), "" | "raw" | "hunk") {
        return None;
    }
    if output == "hunk" {
        #[cfg(feature = "m68k")]
        if canonical == "vasm" && target.is_empty() {
            return Some(asm198x::assemble_vasm_exe_files);
        }
        return None;
    }

    match (canonical, target.as_str()) {
        #[cfg(feature = "z80")]
        ("pasmo", "z80n" | "next") => Some(asm198x::assemble_pasmonext_files),
        #[cfg(feature = "z80")]
        ("pasmonext", "z80") => Some(asm198x::assemble_pasmo_files),
        #[cfg(feature = "z80")]
        ("sjasmplus", "z80n" | "next") => Some(asm198x::assemble_sjasmplus_next_files),
        #[cfg(feature = "z80")]
        ("pasmo" | "sjasmplus", "" | "z80") | ("pasmonext", "" | "z80n" | "next") => {
            entry(canonical)
        }
        (_, "") => entry(canonical),
        _ => None,
    }
}

#[derive(Deserialize)]
struct ProjectRequest {
    dialect: String,
    #[serde(default)]
    target: String,
    root: String,
    files: BTreeMap<String, String>,
    #[serde(default)]
    output: String,
}

fn result_json(result: Result<AssemblyResult, MultiFileError>) -> Option<String> {
    let json = match result {
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
    json.ok()
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
    result_json(entry(source, ROOT, &MemoryLoader::new()))
}

/// Assemble a named in-memory project described by JSON.
///
/// The request is `{ dialect, target?, root, files, output? }`: `files` maps
/// paths to source text and must contain `root`; `target` is `z80` or `z80n`
/// for pasmo/sjasmplus; `output` is `raw` (the default) or `hunk` for vasm.
/// Includes resolve against the named file set through the library's existing
/// [`asm198x::source::SourceLoader`] contract. The returned JSON has exactly
/// the same success/diagnostic shapes as [`assemble`].
///
/// `null` means the request JSON is malformed, the root is absent, or the
/// dialect/target/output combination is not supported by this build.
#[wasm_bindgen]
#[must_use]
pub fn assemble_project(request: &str) -> Option<String> {
    let request: ProjectRequest = serde_json::from_str(request).ok()?;
    let entry = project_entry(&request.dialect, &request.target, &request.output)?;
    let source = request.files.get(&request.root)?.clone();
    let loader = request
        .files
        .into_iter()
        .filter(|(path, _)| path != &request.root)
        .fold(MemoryLoader::new(), |loader, (path, contents)| {
            loader.text(path, contents)
        });
    result_json(entry(&source, &request.root, &loader))
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

/// Assemble `source` in `dialect` and hand back a Spectrum tape image.
///
/// The same bytes `asm198x --dialect pasmonext --tapbas` writes: the program
/// with a BASIC loader stub in front, so the machine's own ROM loads and runs
/// it. `name` goes in the block header, where pasmo puts the output path.
///
/// This is how a lesson gets a program the ROM has initialised the machine
/// for. A snapshot skips the boot, which is faster but leaves the system
/// variables zeroed, so the program cannot call a ROM routine afterwards —
/// see asm198x#568. A tape costs a load and has none of that problem, because
/// the firmware really did the work.
///
/// `format` is `"tap"` or `"tzx"`. Spectrum Z80 only.
///
/// `null` when the dialect or format is unknown, or the source does not
/// assemble or does not fit below the top of memory — [`assemble`] says which.
#[wasm_bindgen]
#[must_use]
pub fn tape(dialect: &str, source: &str, name: &str, format: &str) -> Option<Vec<u8>> {
    let format = match format {
        "tap" => asm198x::TapeFormat::Tap,
        "tzx" => asm198x::TapeFormat::Tzx,
        _ => return None,
    };
    let result = entry(dialect)?(source, ROOT, &MemoryLoader::new()).ok()?;
    asm198x::tape(&result, format, name, true).ok()
}

/// Where the assembled program sits in memory, as JSON.
///
/// `{"origin": 32768, "length": 11}` — the load address and the number of
/// bytes, which together are the range the program occupies.
///
/// A page running a reader's code needs this to tell whether the machine is
/// still executing that code. A program that runs past its own last
/// instruction has a program counter outside this range, which is the
/// difference between "the machine stopped" and "your program ran off the end
/// — that is what the `halt` at the bottom prevents".
///
/// `origin` is `null` for a linked image whose bytes are the linker's, with no
/// single meaningful load address. `null` overall when the dialect is unknown
/// or the source does not assemble — [`assemble`] says why.
#[wasm_bindgen]
#[must_use]
pub fn extent(dialect: &str, source: &str) -> Option<String> {
    let result = entry(dialect)?(source, ROOT, &MemoryLoader::new()).ok()?;
    Some(format!(
        r#"{{"origin":{},"length":{}}}"#,
        result
            .origin
            .map_or_else(|| "null".to_owned(), |origin| origin.to_string()),
        result.bytes.len()
    ))
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

/// Every dialect *this build* accepts, as a JSON array of
/// `{ "name", "aliases", "blurb" }` in the order the CLI reference presents
/// them — the same table `asm198x dialects` prints, so a picker built from
/// this can never offer a name the assembler refuses.
#[wasm_bindgen]
#[must_use]
pub fn dialects() -> String {
    let table: Vec<serde_json::Value> = asm198x::dialect_table::DIALECTS
        .iter()
        // Only what this build can actually assemble. A build selecting one
        // architecture still links the whole table, which is just names, so
        // without this filter a picker made from it would offer dialects
        // `assemble` then refuses — the exact thing the table exists to stop.
        .filter(|d| entry(d.name).is_some())
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
    ///
    /// Only meaningful on a build that selects every architecture. A build
    /// that selects one is *supposed* to leave rows unresolved — which is why
    /// `dialects` filters, and why the test below holds whatever is selected.
    #[cfg(feature = "all")]
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

    /// The invariant that survives feature selection: a page building a picker
    /// from `dialects` can never be offered a name `assemble` refuses. This is
    /// the guarantee the whole-table test above gives on a full build, stated
    /// so it also holds on a build with one architecture.
    #[test]
    fn dialects_offers_only_what_this_build_assembles() {
        let json = dialects();
        let parsed: serde_json::Value =
            serde_json::from_str(&json).unwrap_or(serde_json::Value::Null);
        let rows = parsed.as_array().map(Vec::as_slice).unwrap_or_default();

        assert!(
            !rows.is_empty(),
            "a build with no architecture assembles nothing"
        );

        for row in rows {
            let name = row
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            assert!(
                entry(name).is_some(),
                "`{name}` is offered but not assemblable"
            );
            for alias in row
                .get("aliases")
                .and_then(serde_json::Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default()
            {
                let alias = alias.as_str().unwrap_or_default();
                assert!(
                    entry(alias).is_some(),
                    "alias `{alias}` is offered but not assemblable"
                );
            }
        }
    }

    #[cfg(feature = "z80")]
    #[test]
    fn a_project_resolves_an_include_and_selects_the_z80n_target() {
        let request = serde_json::json!({
            "dialect": "sjasmplus",
            "target": "z80n",
            "root": "src/main.asm",
            "files": {
                "src/main.asm": " include \"part.asm\"\n",
                "part.asm": " nextreg $07,$02\n"
            }
        });
        let json = assemble_project(&request.to_string()).unwrap_or_default();
        let value: serde_json::Value =
            serde_json::from_str(&json).unwrap_or(serde_json::Value::Null);
        assert_eq!(value["bytes"], serde_json::json!([0xed, 0x91, 0x07, 0x02]));
        assert_eq!(
            value["files"],
            serde_json::json!(["src/main.asm", "part.asm"])
        );

        let plain = serde_json::json!({
            "dialect": "sjasmplus",
            "target": "z80",
            "root": "main.asm",
            "files": { "main.asm": " nextreg $07,$02\n" }
        });
        let json = assemble_project(&plain.to_string()).unwrap_or_default();
        let value: serde_json::Value =
            serde_json::from_str(&json).unwrap_or(serde_json::Value::Null);
        assert!(value.is_array(), "plain Z80 must reject NEXTREG: {json}");
    }

    #[cfg(feature = "m68k")]
    #[test]
    fn hunk_output_is_byte_identical_to_the_native_library_entry() {
        let source = " section code,code\n moveq #1,d0\n";
        let request = serde_json::json!({
            "dialect": "vasm",
            "root": "main.s",
            "files": { "main.s": source },
            "output": "hunk"
        });
        let json = assemble_project(&request.to_string()).unwrap_or_default();
        let value: serde_json::Value =
            serde_json::from_str(&json).unwrap_or(serde_json::Value::Null);
        let web_bytes: Vec<u8> = value["bytes"]
            .as_array()
            .map(|bytes| {
                bytes
                    .iter()
                    .filter_map(serde_json::Value::as_u64)
                    .filter_map(|byte| u8::try_from(byte).ok())
                    .collect()
            })
            .unwrap_or_default();
        let native = asm198x::assemble_vasm_exe_files(source, "main.s", &MemoryLoader::new())
            .map(|result| result.bytes)
            .unwrap_or_default();
        assert_eq!(web_bytes, native);
        assert_eq!(&web_bytes[..4], &[0x00, 0x00, 0x03, 0xf3]);
    }

    #[cfg(any(feature = "z80", feature = "mos6502"))]
    /// The unit-01 program from the Spectrum course: set the border, hold the
    /// picture, and name an entry point.
    const BORDER: &str = "            org 32768\nstart:      ld a, 2\n            out ($FE), a\n.loop:      halt\n            jr .loop\n            end start\n";

    #[cfg(feature = "z80")]
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
    #[cfg(feature = "z80")]
    #[test]
    fn a_snapshot_needs_an_entry_point() {
        let no_end = "            org 32768\n            ld a, 2\n            out ($FE), a\n";
        assert!(snapshot("pasmonext", no_end).is_none());
    }

    /// A `.tap` is length-prefixed blocks. With `autorun` there are two: the
    /// BASIC loader that makes the ROM run the program, and the code itself.
    /// Checking the structure rather than a byte count, because the loader
    /// stub's length is the library's business and not this shell's.
    #[cfg(feature = "z80")]
    #[test]
    fn a_tape_carries_a_loader_and_the_code() {
        let tap = tape("pasmonext", BORDER, "border.tap", "tap").unwrap_or_default();

        let mut blocks = 0;
        let mut at = 0;
        while at + 2 <= tap.len() {
            let len = usize::from(u16::from_le_bytes([tap[at], tap[at + 1]]));
            at += 2 + len;
            blocks += 1;
        }

        assert_eq!(at, tap.len(), "block lengths must tile the file exactly");
        assert_eq!(
            blocks, 4,
            "two headers and two data blocks: loader, then code"
        );
    }

    /// The two spellings are different containers of the same program, so a
    /// page picking one must not silently get the other.
    #[cfg(feature = "z80")]
    #[test]
    fn tap_and_tzx_are_not_the_same_container() {
        let tap = tape("pasmonext", BORDER, "b.tap", "tap").unwrap_or_default();
        let tzx = tape("pasmonext", BORDER, "b.tzx", "tzx").unwrap_or_default();

        assert!(!tap.is_empty() && !tzx.is_empty());
        assert_ne!(tap, tzx);
        assert_eq!(&tzx[..7], b"ZXTape!", "a .tzx opens with its signature");
    }

    /// Unlike a snapshot, a tape needs no `end <addr>`: the loader stub falls
    /// back to the origin, which is what the CLI does.
    #[cfg(feature = "z80")]
    #[test]
    fn a_tape_does_not_need_an_entry_point() {
        let no_end = "            org 32768\n            ld a, 2\n            out ($FE), a\n";
        assert!(tape("pasmonext", no_end, "b.tap", "tap").is_some());
    }

    /// An unrecognised container is refused rather than guessed at.
    #[test]
    fn an_unknown_tape_format_is_refused() {
        assert!(tape("pasmonext", "            end 0\n", "b.dsk", "dsk").is_none());
    }

    /// The range a page checks the program counter against.
    #[cfg(feature = "z80")]
    #[test]
    fn the_extent_is_where_the_program_was_assembled() {
        let json = extent("pasmonext", BORDER).unwrap_or_default();
        assert!(
            json.starts_with(r#"{"origin":32768,"length":"#),
            "border.asm has `org 32768`; got {json}"
        );
    }

    /// Refused for the same reason and in the same way as the others, so a
    /// caller has one place to look for diagnostics.
    #[test]
    fn an_unknown_dialect_has_no_extent() {
        assert!(extent("not-a-dialect", "            end 0\n").is_none());
    }

    /// Independent of architecture: the name is refused before the source is
    /// looked at, so this holds whatever the build selects.
    #[test]
    fn an_unknown_dialect_yields_no_snapshot() {
        assert!(snapshot("not-a-dialect", "            end 0\n").is_none());
    }

    /// Snapshots are a Spectrum Z80 output. Asking a 6502 dialect for one
    /// must decline rather than emit something shaped like a Spectrum.
    ///
    /// Needs 6502 selected, or the refusal would be for the wrong reason:
    /// `acme` would be declined for not being in the build at all.
    #[cfg(feature = "mos6502")]
    #[test]
    fn a_non_spectrum_dialect_yields_no_snapshot() {
        assert!(snapshot("acme", BORDER).is_none());
    }
}
