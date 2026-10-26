//! PasmoNext 0.1.3's own command-line options, as the CLI takes them.

use std::process::{Command, Output};

#[path = "support/scratch.rs"]
mod scratch;

fn run(dir: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_asm198x"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("CLI")
}

/// Each option, on a program whose bytes or warnings PasmoNext 0.1.3 was
/// measured to change with it.
#[test]
fn pasmonext_options_change_what_source_means() {
    let cases: &[(&str, &str, &[u8])] = &[
        // `--nocase`: `Start` and `START` are one name.
        ("--nocase", " org 0\nStart: jp START\n", &[0xC3, 0x00, 0x00]),
        // `--bracket`: parentheses only group, so this is `ld a,12h`.
        ("--bracket", " org 0\n ld a,(12h)\n", &[0x3E, 0x12]),
        // `--alocal`: `_x` is local to each label's scope.
        (
            "--alocal",
            " org 0\none: jr _x\n_x: nop\ntwo: jr _x\n_x: nop\n",
            &[0x18, 0x00, 0x00, 0x18, 0x00, 0x00],
        ),
        // `--pass3`: three passes from the start; the bytes are unchanged.
        ("--pass3", " org 0\n db 1\n", &[0x01]),
    ];
    for (option, source, bytes) in cases {
        let dir = scratch::dir("pasmo-options");
        std::fs::write(dir.join("in.asm"), source).expect("source");
        for dialect in ["pasmo", "pasmonext"] {
            let out = run(
                &dir,
                &["--dialect", dialect, option, "in.asm", "-o", "out.bin"],
            );
            assert!(out.status.success(), "{option}/{dialect}: {out:?}");
            assert_eq!(
                std::fs::read(dir.join("out.bin")).expect("bytes"),
                *bytes,
                "{option}/{dialect}"
            );
        }
    }
}

#[test]
fn w8080_warns_and_equ_has_its_short_spelling() {
    let dir = scratch::dir("pasmo-options-w8080");
    std::fs::write(dir.join("in.asm"), " org 0\n if FLAG\n exx\n endif\n").expect("source");
    let out = run(
        &dir,
        &[
            "--dialect",
            "pasmo",
            "--w8080",
            "-E",
            "FLAG=1",
            "in.asm",
            "-o",
            "out.bin",
        ],
    );
    assert!(out.status.success(), "{out:?}");
    assert_eq!(std::fs::read(dir.join("out.bin")).expect("bytes"), [0xD9]);
    let warned = String::from_utf8_lossy(&out.stderr);
    assert!(warned.contains("not a 8080 instruction"), "{warned}");
}

#[test]
fn pasmonext_options_are_refused_for_other_dialects() {
    let dir = scratch::dir("pasmo-options-other");
    std::fs::write(dir.join("in.asm"), " nop\n").expect("source");
    for option in ["--nocase", "--alocal", "--bracket", "--w8080", "--pass3"] {
        let out = run(&dir, &["--dialect", "sjasmplus", option, "in.asm"]);
        assert!(!out.status.success(), "{option}: {out:?}");
        let message = String::from_utf8_lossy(&out.stderr);
        assert!(
            message.contains(option) && message.contains("pasmo/pasmonext"),
            "{option}: {message}"
        );
    }
}
