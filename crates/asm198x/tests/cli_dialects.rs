//! The dialect table and the command line must agree.
//!
//! `dialect_table` is one list feeding three consumers: `--dialect` resolution,
//! the `--help` text, and the CLI reference in the org docs repo. Its own unit
//! tests prove the list is internally consistent. These prove the *binary*
//! honours it — that every documented spelling reaches an assembler, and that
//! `--help` says what the table says.
//!
//! The direction that matters is the one that used to fail silently. Three
//! hand-maintained copies of this list existed and two had drifted: `--help`
//! and the reference were both missing `pdp11`, `tms9900`, `cp1610`, `z8000`
//! and `z8001`, so five working dialects were undiscoverable. Nothing failed;
//! the documentation was simply wrong for as long as nobody checked.

use std::path::PathBuf;
use std::process::Command;

use asm198x::dialect_table::{DIALECTS, canonical};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_asm198x"))
}

/// A source every CPU here assembles: no instructions at all.
///
/// A shared mnemonic does not exist across 6502, Z80, 68000, PDP-11 and the
/// rest, and picking one per dialect would make this a test of the fixture. An
/// empty program still exercises the whole path — argument parse, dialect
/// resolution, parse, lower, emit — which is what is under test.
fn empty_source() -> PathBuf {
    let path = std::env::temp_dir().join("asm198x-cli-dialects-empty.s");
    std::fs::write(&path, "\n").expect("write temp source");
    path
}

/// Every spelling the table documents is accepted by `--dialect`.
///
/// A row whose name no arm handles reaches the "no assembler wired up" arm,
/// which is unreachable in a correct build and is exactly what this catches.
#[test]
fn every_documented_spelling_reaches_an_assembler() {
    let src = empty_source();
    let out = std::env::temp_dir().join("asm198x-cli-dialects-out.bin");
    let mut broken = Vec::new();

    for entry in DIALECTS {
        for spelling in std::iter::once(&entry.name).chain(entry.aliases) {
            let result = bin()
                .args(["--dialect", spelling])
                .arg(&src)
                .arg("-o")
                .arg(&out)
                .output()
                .expect("run asm198x");
            let stderr = String::from_utf8_lossy(&result.stderr);
            if !result.status.success() {
                broken.push(format!("{spelling}: {}", stderr.trim()));
            }
        }
    }

    assert!(
        broken.is_empty(),
        "{} documented spelling(s) do not assemble:\n  {}",
        broken.len(),
        broken.join("\n  ")
    );
}

/// `--help` lists every dialect the table holds.
///
/// This is the assertion the old prose list would have failed. It does not
/// check wording — the help text renders from the table, so wording cannot
/// drift — only that no dialect is missing from what a user is shown.
#[test]
fn help_lists_every_dialect() {
    let out = bin().arg("--help").output().expect("run asm198x --help");
    let help = String::from_utf8_lossy(&out.stdout) + String::from_utf8_lossy(&out.stderr);

    let missing: Vec<&str> = DIALECTS
        .iter()
        .map(|d| d.name)
        .filter(|name| !help.contains(*name))
        .collect();

    assert!(
        missing.is_empty(),
        "`--help` does not mention: {}",
        missing.join(", ")
    );
}

/// The generated markdown covers every dialect, and is what the CLI reference
/// is expected to carry. Regenerate the page's table with
/// `asm198x dialects --markdown`.
#[test]
fn the_generated_markdown_covers_every_dialect() {
    let out = bin()
        .args(["dialects", "--markdown"])
        .output()
        .expect("run asm198x dialects");
    let table = String::from_utf8_lossy(&out.stdout);

    assert!(
        table.starts_with("| Dialect |"),
        "a markdown table: {table}"
    );
    for entry in DIALECTS {
        assert!(
            table.contains(&format!("| `{}` |", entry.name)),
            "`{}` is missing from the generated table",
            entry.name
        );
        for alias in entry.aliases {
            assert!(
                table.contains(&format!("`{alias}`")),
                "alias `{alias}` is missing from the generated table"
            );
        }
    }
}

/// An unknown dialect is refused, and the message names what was asked for.
#[test]
fn an_unknown_dialect_is_refused() {
    let src = empty_source();
    let out = bin()
        .args(["--dialect", "frobnicate"])
        .arg(&src)
        .output()
        .expect("run asm198x");

    assert!(!out.status.success(), "an unknown dialect must fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown dialect") && stderr.contains("frobnicate"),
        "unhelpful message: {stderr}"
    );
    assert_eq!(canonical("frobnicate"), None);
}

/// The ROM-less MCS-48 parts are a different dialect from the 8048, not an
/// alias — the reference said otherwise, and the difference is observable:
/// `outl bus,a` assembles as `8048` and is refused as `8039`, because those
/// parts reserve the bus for external program memory.
#[test]
fn the_rom_less_mcs48_parts_refuse_what_the_8048_accepts() {
    let src = std::env::temp_dir().join("asm198x-cli-dialects-bus.s");
    std::fs::write(&src, " outl bus,a\n").expect("write temp source");
    let out_path = std::env::temp_dir().join("asm198x-cli-dialects-bus.bin");

    let romful = bin()
        .args(["--dialect", "8048"])
        .arg(&src)
        .arg("-o")
        .arg(&out_path)
        .output()
        .expect("run asm198x");
    assert!(
        romful.status.success(),
        "the 8048 has the bus: {}",
        String::from_utf8_lossy(&romful.stderr)
    );

    let romless = bin()
        .args(["--dialect", "8039"])
        .arg(&src)
        .arg("-o")
        .arg(&out_path)
        .output()
        .expect("run asm198x");
    assert!(
        !romless.status.success(),
        "the ROM-less parts reserve the bus, so this must be refused"
    );
}

/// Every dialect the table documents can be **formatted**.
///
/// `fmt` grew per dialect and finished quietly: the last unsupported-dialect
/// fallback in `main.rs` is gone, and nothing said so outside a comment. *Why
/// asm198x* was still telling readers the formatter covered seven CPU families
/// and not the 6502, which understated the tool by most of it — the drift that
/// matters more than the overstating kind, because it loses the reader who
/// would have been served.
///
/// The empty source is the same idiom the assemble test uses, and for the same
/// reason: it exercises argument parse, dialect resolution, the AST front-end
/// and emit, without the test becoming a test of its fixture.
#[test]
fn every_documented_dialect_formats() {
    let source = empty_source();
    for entry in DIALECTS {
        let out = bin()
            .args(["fmt", "--dialect", entry.name])
            .arg(&source)
            .output()
            .expect("run asm198x");
        assert!(
            out.status.success(),
            "`{}` does not format: {}",
            entry.name,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
}

/// Every dialect the table documents can be **disassembled** for.
///
/// Same story as the formatter: twenty disassemblers are wired to twenty-two
/// dispatch arms, and the page said "6502 and Z80".
///
/// Two bytes rather than an empty file, so the disassembler has something to
/// decode. Whatever the bytes mean on a given CPU, the run must succeed —
/// undecodable bytes are rendered as data, which is a disassembly and not a
/// failure.
#[test]
fn every_documented_dialect_disassembles() {
    let path = std::env::temp_dir().join("asm198x-cli-dialects-two.bin");
    std::fs::write(&path, [0x00, 0x00]).expect("write temp binary");
    for entry in DIALECTS {
        let out = bin()
            .args(["disasm", "--dialect", entry.name])
            .arg(&path)
            .output()
            .expect("run asm198x");
        assert!(
            out.status.success(),
            "`{}` does not disassemble: {}",
            entry.name,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let _ = std::fs::remove_file(&path);
}
