//! Byte-identity validation against the real toolchains and the Code198x corpus.
//!
//! This is the guarantee the whole project rests on: our output is byte-for-byte
//! what the reference assembler produces on the actual curriculum. It is
//! `#[ignore]`d because it needs external programs (`acme`, `ca65`, `ld65`,
//! `pasmo`/PasmoNext, `sjasmplus`) and the sibling `Code198x` checkout, so it
//! can't run in the default unit-test pass or in a vanilla CI box.
//!
//! Run it with:
//!
//! ```text
//! cargo test --test curriculum -- --ignored --nocapture
//! ```
//!
//! Each section degrades gracefully: a missing tool or a missing corpus path is
//! reported and skipped rather than failed, so the test is safe to run anywhere.
//! Whatever *is* present is checked exactly.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Locate the `Code198x` checkout, a sibling of the `Asm198x` container two
/// levels above this crate's workspace.
fn code198x() -> Option<PathBuf> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../Code198x");
    let p = p.canonicalize().ok()?;
    p.is_dir().then_some(p)
}

/// Whether a reference tool is on `PATH` (it exists if it runs at all).
fn have(bin: &str) -> bool {
    Command::new(bin).output().is_ok()
}

/// The buildable `.asm` files: one per `unit-*` directory, directly inside it
/// (not in a `snippets/` subdirectory).
fn main_asms(project: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(units) = fs::read_dir(project) else {
        return out;
    };
    for unit in units.flatten() {
        let dir = unit.path();
        let is_unit = dir.is_dir()
            && dir.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("unit-"));
        if !is_unit {
            continue;
        }
        if let Ok(files) = fs::read_dir(&dir) {
            for f in files.flatten() {
                let fp = f.path();
                if fp.extension().and_then(|e| e.to_str()) == Some("asm") {
                    out.push(fp);
                }
            }
        }
    }
    out.sort();
    out
}

fn label(file: &Path) -> String {
    let unit = file.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or("?");
    let proj = file
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("?");
    format!("{proj}/{unit}")
}

/// Run a reference command in `tmp` and return the bytes it wrote to `out`.
fn ref_bytes(tmp: &Path, out: &Path, mut build: Command) -> Option<Vec<u8>> {
    let status = build.current_dir(tmp).output().ok()?;
    if !status.status.success() {
        return None;
    }
    fs::read(out).ok()
}

#[test]
#[ignore = "needs the reference assemblers and the Code198x checkout; run with --ignored"]
fn curriculum_is_byte_identical() {
    let Some(root) = code198x() else {
        eprintln!("SKIP: Code198x checkout not found next to Asm198x");
        return;
    };
    let tmp = std::env::temp_dir().join("asm198x-curriculum");
    fs::create_dir_all(&tmp).expect("create temp dir");

    let mut fails: Vec<String> = Vec::new();
    let mut checked = 0usize;

    // --- 6502 / acme (C64) -------------------------------------------------
    if have("acme") {
        let c64 = root.join("code-samples/commodore-64/assembly");
        let files: Vec<_> = main_asms(&c64.join("starfield"))
            .into_iter()
            .chain(main_asms(&c64.join("sid-symphony")))
            .collect();
        for file in &files {
            let src = fs::read_to_string(file).expect("read source");
            let ours = asm198x::assemble_acme(&src).expect("acme assemble");
            let prg = tmp.join("ref.prg");
            let mut cmd = Command::new("acme");
            cmd.args(["-f", "cbm", "-o"]).arg(&prg).arg(file);
            match ref_bytes(&tmp, &prg, cmd) {
                // acme's `cbm` output is a 2-byte load address then the data.
                Some(prg_bytes) if prg_bytes.len() >= 2 => {
                    checked += 1;
                    if ours.bytes != prg_bytes[2..] {
                        fails.push(format!("acme assemble: {}", label(file)));
                    }
                }
                _ => fails.push(format!("acme reference failed: {}", label(file))),
            }
            // Disassembler round-trip: assemble -> disassemble -> reassemble.
            let listing = asm198x::listing_6502(&ours.bytes, ours.origin);
            let round = asm198x::assemble_acme(&listing).expect("reassemble").bytes;
            if round != ours.bytes {
                fails.push(format!("6502 disasm round-trip: {}", label(file)));
            }
        }
    } else {
        eprintln!("SKIP: `acme` not on PATH");
    }

    // --- 6502 / ca65 + ld65 (NES) -----------------------------------------
    if have("ca65") && have("ld65") {
        let nes = root.join("code-samples/nintendo-entertainment-system/assembly");
        let files: Vec<_> = main_asms(&nes.join("dash"))
            .into_iter()
            .chain(main_asms(&nes.join("neon-nexus")))
            .collect();
        for file in &files {
            let src = fs::read_to_string(file).expect("read source");
            let ours = asm198x::assemble_ca65(&src).expect("ca65 assemble");
            let cfg = file.parent().expect("parent").join("nes.cfg");
            let obj = tmp.join("ref.o");
            let rom = tmp.join("ref.nes");
            let mut ca = Command::new("ca65");
            ca.arg(file).arg("-o").arg(&obj);
            let assembled = ca.current_dir(&tmp).output().map(|o| o.status.success()).unwrap_or(false);
            let mut ld = Command::new("ld65");
            ld.arg("-C").arg(&cfg).arg(&obj).arg("-o").arg(&rom);
            match (assembled, ref_bytes(&tmp, &rom, ld)) {
                (true, Some(reference)) => {
                    checked += 1;
                    if ours != reference {
                        fails.push(format!("ca65 link: {}", label(file)));
                    }
                }
                _ => fails.push(format!("ca65 reference failed: {}", label(file))),
            }
        }
    } else {
        eprintln!("SKIP: `ca65`/`ld65` not on PATH");
    }

    // --- Z80 / PasmoNext + sjasmplus (Spectrum, Gloaming) ------------------
    let gloaming = root.join("code-samples/sinclair-zx-spectrum/assembly/gloaming");
    let z80_files = main_asms(&gloaming);
    if have("pasmo") {
        for file in &z80_files {
            let src = fs::read_to_string(file).expect("read source");
            let ours = asm198x::assemble_pasmonext(&src).expect("pasmonext assemble");
            let bin = tmp.join("ref.bin");
            let mut cmd = Command::new("pasmo");
            cmd.arg(file).arg(&bin);
            match ref_bytes(&tmp, &bin, cmd) {
                Some(reference) => {
                    checked += 1;
                    if ours.bytes != reference {
                        fails.push(format!("pasmonext assemble: {}", label(file)));
                    }
                }
                None => fails.push(format!("pasmo reference failed: {}", label(file))),
            }
            // Z80 disassembler round-trip.
            let listing = asm198x::listing_z80(&ours.bytes, ours.origin, true);
            let round = asm198x::assemble_pasmonext(&listing).expect("reassemble").bytes;
            if round != ours.bytes {
                fails.push(format!("Z80 disasm round-trip: {}", label(file)));
            }
        }
    } else {
        eprintln!("SKIP: `pasmo` (PasmoNext) not on PATH");
    }
    if have("sjasmplus") {
        for file in &z80_files {
            let src = fs::read_to_string(file).expect("read source");
            let ours = asm198x::assemble_sjasmplus(&src).expect("sjasmplus assemble");
            let bin = tmp.join("ref-sj.bin");
            let mut cmd = Command::new("sjasmplus");
            cmd.arg(format!("--raw={}", bin.display())).arg(file);
            match ref_bytes(&tmp, &bin, cmd) {
                Some(reference) => {
                    checked += 1;
                    if ours.bytes != reference {
                        fails.push(format!("sjasmplus assemble: {}", label(file)));
                    }
                }
                None => fails.push(format!("sjasmplus reference failed: {}", label(file))),
            }
        }
    } else {
        eprintln!("SKIP: `sjasmplus` not on PATH");
    }

    eprintln!("checked {checked} byte-identity comparisons across the curriculum");
    assert!(
        fails.is_empty(),
        "{} mismatch(es):\n  {}",
        fails.len(),
        fails.join("\n  ")
    );
    assert!(checked > 0, "no comparisons ran — no tools or corpus present?");
}
