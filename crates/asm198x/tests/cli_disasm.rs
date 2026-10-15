//! Ordinary listings reject options whose effects belong to another operation.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "asm198x-disasm-options-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).expect("isolated disassembly directory");
        Self(path)
    }

    fn disassemble(&self, input: &str, extra: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_asm198x"))
            .current_dir(&self.0)
            .args(["disasm", "--dialect", "acme", "--org", "0x8000", input])
            .args(extra)
            .output()
            .expect("run disassembler")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn unsupported_disassembly_options_fail_before_reading_or_writing_files() {
    let cases: &[&[&str]] = &[
        &["-o", "out.s"],
        &["--output", "out.s"],
        &["--prg"],
        &["--sna"],
        &["--tap"],
        &["--tapbas"],
        &["--tzx"],
        &["--tzxbas"],
        &["--exe"],
        &["--hunkexe"],
        &["--gb-rom"],
        &["-C", "missing.cfg"],
        &["--outprefix", "missing-prefix/"],
        &["--outprefix=missing-prefix/"],
        &["--equ", "ONE=1"],
        &["--equ=ONE=1"],
        &["-I", "missing-includes/"],
        &["--message-format=json"],
        &["--message-format", "json"],
        &["--message-format=human"],
        &["--message-format", "human"],
        &["--sym-format=native"],
        &["--sym-format", "native"],
        &["--debug=out.s"],
        &["--sym=out.s"],
        &["--listing=out.s"],
        &["--listing-json=out.s"],
        &["--map=out.s"],
    ];
    for flags in cases {
        for input in ["input.bin", "missing.bin"] {
            for previous in [None, Some(b"previous source\n".as_slice())] {
                let dir = Scratch::new();
                let bytes = [0xa9, 5, 0x60];
                std::fs::write(dir.0.join("input.bin"), bytes).expect("input binary");
                if let Some(source) = previous {
                    std::fs::write(dir.0.join("out.s"), source).expect("previous output");
                }
                let result = dir.disassemble(input, flags);
                let stderr = String::from_utf8_lossy(&result.stderr);
                assert!(!result.status.success(), "{flags:?}/{input}: {result:?}");
                assert!(result.stdout.is_empty(), "{flags:?}: {result:?}");
                assert!(stderr.contains("disasm"), "{flags:?}: {stderr}");
                assert!(!stderr.contains("cannot read"), "{flags:?}: {stderr}");
                assert_eq!(
                    std::fs::read(dir.0.join("input.bin")).expect("preserved input"),
                    bytes
                );
                match previous {
                    Some(source) => assert_eq!(
                        std::fs::read(dir.0.join("out.s")).expect("preserved source"),
                        source
                    ),
                    None => assert!(!dir.0.join("out.s").exists()),
                }
                let entries = std::fs::read_dir(&dir.0).expect("remaining files").count();
                assert_eq!(entries, 1 + usize::from(previous.is_some()), "{flags:?}");
            }
        }
    }
}

#[test]
fn ordinary_disassembly_preserves_cpu_origin_selection_and_round_trips() {
    for (selection, source, bytes) in [
        (
            &["--dialect", "acme"][..],
            "*=$8000\n lda #5\n rts\n",
            &[0xa9, 5, 0x60][..],
        ),
        (
            &["--cpu", "z80"][..],
            " org $8000\n jr $8004\n nop\n ret\n",
            &[0x18, 2, 0, 0xc9][..],
        ),
    ] {
        let dir = Scratch::new();
        std::fs::write(dir.0.join("input.bin"), bytes).expect("input binary");
        let result = Command::new(env!("CARGO_BIN_EXE_asm198x"))
            .current_dir(&dir.0)
            .args(["disasm", "--org", "0x8000", "input.bin"])
            .args(selection)
            .output()
            .expect("run disassembler");
        assert!(result.status.success(), "{selection:?}: {result:?}");
        assert_eq!(
            String::from_utf8_lossy(&result.stderr),
            format!("disassembled {} byte(s) at $8000\n", bytes.len())
        );
        // Compare the listing with independently assembled source by rebuilding
        // it at the supplied origin. A branch makes the origin observable.
        let prelude = if selection.contains(&"acme") {
            "*=$8000\n"
        } else {
            " org $8000\n"
        };
        let listing = format!(
            "{prelude}{}",
            String::from_utf8(result.stdout).expect("UTF-8 listing")
        );
        for (name, text) in [("listing.s", listing.as_str()), ("control.s", source)] {
            std::fs::write(dir.0.join(name), text).expect("assembly source");
            let assembled = Command::new(env!("CARGO_BIN_EXE_asm198x"))
                .current_dir(&dir.0)
                .args(selection)
                .args([name, "-o", "rebuilt.bin"])
                .output()
                .expect("assemble listing/control");
            assert!(
                assembled.status.success(),
                "{selection:?}/{name}: {assembled:?}"
            );
            assert_eq!(
                std::fs::read(dir.0.join("rebuilt.bin")).expect("rebuilt binary"),
                bytes
            );
        }
    }
}

#[test]
fn ordinary_68000_disassembly_matches_the_decoder_at_the_requested_origin() {
    let dir = Scratch::new();
    let bytes = [0x60, 2];
    std::fs::write(dir.0.join("input.bin"), bytes).expect("input binary");
    for (flags, origin) in [
        (&[][..], 0_u32),
        (&["--org", "0x8000"][..], 0x8000),
        (&["--org", "0x10000"][..], 0x10000),
        (&["--org", "$10000"][..], 0x10000),
        (&["--org", "65536"][..], 0x10000),
        (&["--org", "0x12345678"][..], 0x12345678),
        (&["--org", "0xfffffffc"][..], 0xfffffffc),
        (&["--org", "4294967295"][..], u32::MAX),
    ] {
        for selection in [
            ["--target", "68000"],
            ["--cpu", "m68k"],
            ["--dialect", "vasm"],
        ] {
            for origin_first in [false, true] {
                let mut command = Command::new(env!("CARGO_BIN_EXE_asm198x"));
                command.current_dir(&dir.0).args(["disasm", "input.bin"]);
                if origin_first {
                    command.args(flags).args(selection);
                } else {
                    command.args(selection).args(flags);
                }
                let result = command.output().expect("run disassembler");
                assert!(result.status.success(), "{result:?}");
                assert_eq!(
                    String::from_utf8(result.stdout).expect("UTF-8 listing"),
                    asm198x::listing_68000(&bytes, origin)
                );
                assert_eq!(
                    String::from_utf8_lossy(&result.stderr),
                    format!("disassembled 2 byte(s) at ${origin:04X}\n")
                );
            }
        }
    }
}

#[test]
fn wide_origins_remain_invalid_for_every_narrow_disassembler_and_recovery() {
    let dir = Scratch::new();
    let bytes = [0x60, 2];
    std::fs::write(dir.0.join("input.bin"), bytes).expect("input binary");
    for input in ["input.bin", "missing.bin"] {
        for address in ["0x10000", "4294967295"] {
            for dialect in asm198x::dialect_table::DIALECTS {
                if dialect.name == "vasm" {
                    continue;
                }
                let result = Command::new(env!("CARGO_BIN_EXE_asm198x"))
                    .current_dir(&dir.0)
                    .args(["disasm", "--dialect", dialect.name, "--org", address, input])
                    .output()
                    .expect("run narrow disassembler");
                assert!(
                    !result.status.success(),
                    "{}/{address}: {result:?}",
                    dialect.name
                );
                assert!(result.stdout.is_empty(), "{result:?}");
                let stderr = String::from_utf8_lossy(&result.stderr);
                assert!(stderr.contains("invalid address"), "{stderr}");
                assert!(!stderr.contains("cannot read"), "{stderr}");
            }
            for mode in ["--recover", "asm"] {
                let args = if mode == "asm" {
                    vec!["asm", "--dialect", "vasm", "--org", address, input]
                } else {
                    vec!["disasm", "--recover", "--org", address, input]
                };
                let result = Command::new(env!("CARGO_BIN_EXE_asm198x"))
                    .current_dir(&dir.0)
                    .args(args)
                    .output()
                    .expect("run bounded operation");
                assert!(!result.status.success(), "{mode}/{address}: {result:?}");
                assert!(result.stdout.is_empty(), "{result:?}");
                assert!(
                    String::from_utf8_lossy(&result.stderr).contains("invalid address"),
                    "{result:?}"
                );
            }
        }
    }
    assert_eq!(
        std::fs::read(dir.0.join("input.bin")).expect("preserved binary"),
        bytes
    );
    assert_eq!(
        std::fs::read_dir(&dir.0).expect("remaining files").count(),
        1
    );
}

#[test]
fn invalid_or_overflowing_68000_origins_fail_before_input_reads() {
    let dir = Scratch::new();
    for address in [
        "0x100000000",
        "4294967296",
        "-1",
        "not-an-address",
        "0x",
        "$",
        "",
    ] {
        for trailing in [&[][..], &["--org", "0"][..]] {
            let result = Command::new(env!("CARGO_BIN_EXE_asm198x"))
                .current_dir(&dir.0)
                .args([
                    "disasm",
                    "--dialect",
                    "vasm",
                    "--org",
                    address,
                    "missing.bin",
                ])
                .args(trailing)
                .output()
                .expect("run disassembler");
            assert!(!result.status.success(), "{address}: {result:?}");
            assert!(result.stdout.is_empty(), "{result:?}");
            assert_eq!(
                String::from_utf8_lossy(&result.stderr),
                format!("asm198x: invalid address `{address}`\n")
            );
        }
    }
}

#[test]
fn maximum_narrow_origin_still_reaches_the_narrow_decoder() {
    let dir = Scratch::new();
    let bytes = [0xd0, 0xfe];
    std::fs::write(dir.0.join("input.bin"), bytes).expect("input binary");
    let result = dir.disassemble("input.bin", &["--org", "0xffff"]);
    assert!(result.status.success(), "{result:?}");
    assert_eq!(
        String::from_utf8(result.stdout).expect("listing"),
        asm198x::listing_6502(&bytes, u16::MAX)
    );
}
