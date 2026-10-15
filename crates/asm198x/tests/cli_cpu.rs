//! CPU selections must either take effect or fail before producing artifacts.
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Scratch(PathBuf);
impl Scratch {
    fn new(source: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "asm198x-cli-cpu-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).expect("create scratch");
        std::fs::write(path.join("input.asm"), source).expect("write source");
        Self(path)
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_asm198x"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .expect("run CLI")
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("remove scratch");
    }
}

#[test]
fn invalid_cpu_selections_fail_without_touching_artifacts() {
    let scratch = Scratch::new("org $8000\nnop\n");
    for cpu in ["nonsense", "68000"] {
        for flag in ["--cpu", "--target"] {
            for json in [false, true] {
                for existing in [false, true] {
                    let artifacts = ["out.bin", "out.debug198x", "out.sym", "out.lst"];
                    for name in artifacts {
                        if existing {
                            std::fs::write(scratch.0.join(name), b"preserve me").expect("seed");
                        }
                    }
                    let mut args = vec![
                        "--dialect",
                        "pasmo",
                        flag,
                        cpu,
                        "input.asm",
                        "-o",
                        "out.bin",
                        "--debug=out.debug198x",
                        "--sym=out.sym",
                        "--listing=out.lst",
                    ];
                    if json {
                        args.push("--message-format=json");
                    }
                    let out = scratch.run(&args);
                    assert!(!out.status.success(), "ignored {flag} {cpu}: {out:?}");
                    let message = if json {
                        assert!(out.stderr.is_empty(), "{out:?}");
                        let value: serde_json::Value =
                            serde_json::from_slice(&out.stdout).expect("diagnostic JSON");
                        let diagnostics = value.as_array().expect("diagnostic array");
                        assert_eq!(diagnostics.len(), 1);
                        assert_eq!(diagnostics[0]["severity"], "Error");
                        diagnostics[0]["message"]
                            .as_str()
                            .expect("message")
                            .to_owned()
                    } else {
                        assert!(out.stdout.is_empty());
                        String::from_utf8(out.stderr).expect("stderr")
                    };
                    assert!(message.contains(cpu), "{message}");
                    if cpu == "68000" {
                        assert!(message.contains("pasmo"), "{message}");
                    }
                    for name in artifacts {
                        let path = scratch.0.join(name);
                        if existing {
                            assert_eq!(
                                std::fs::read(&path).expect("preserved artifact"),
                                b"preserve me"
                            );
                            std::fs::remove_file(path).expect("remove seeded artifact");
                        } else {
                            assert!(!path.exists(), "created {name}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn unknown_cpu_fails_with_every_dialect_and_mode() {
    let scratch = Scratch::new("\n");
    for entry in asm198x::dialect_table::DIALECTS {
        for mode in ["asm", "fmt", "disasm"] {
            let out = scratch.run(&[
                mode,
                "--dialect",
                entry.name,
                "--cpu",
                "nonsense",
                "input.asm",
            ]);
            assert!(!out.status.success(), "{} {mode}: {out:?}", entry.name);
            assert!(String::from_utf8_lossy(&out.stderr).contains("nonsense"));
        }
    }
    let out = scratch.run(&["--cpu", "nonsense", "--message-format=json", "input.asm"]);
    assert!(!out.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("JSON without dialect");
    assert!(
        value[0]["message"]
            .as_str()
            .expect("message")
            .contains("nonsense")
    );
}

#[test]
fn z80n_opcodes_follow_the_requested_cpu() {
    // Captured SjASMPlus reference verdict: tests/verdicts/z80n.ndjson.
    let scratch = Scratch::new(" mul d,e\n");
    for dialect in ["pasmo", "pasmonext", "sjasmplus"] {
        for cpu in ["z80n", "NeXt", "pasmonext"] {
            let out = scratch.run(&[
                "--dialect",
                dialect,
                "--cpu",
                cpu,
                "input.asm",
                "-o",
                "out.bin",
            ]);
            assert!(out.status.success(), "{dialect}/{cpu}: {out:?}");
            assert_eq!(
                std::fs::read(scratch.0.join("out.bin")).expect("bytes"),
                [0xed, 0x30]
            );
        }
        let out = scratch.run(&[
            "--dialect",
            dialect,
            "--cpu",
            "Z80",
            "input.asm",
            "-o",
            "out.bin",
        ]);
        assert!(!out.status.success(), "base Z80 accepted MUL: {out:?}");
    }
}

#[test]
fn cli_target_does_not_override_lexical_cpu_directives() {
    let scratch = Scratch::new("*=$1000\n!cpu 65816\n rtl\n!cpu 6502\n nop\n");
    let out = scratch.run(&[
        "--dialect",
        "acme",
        "--cpu",
        "6502",
        "input.asm",
        "-o",
        "out.bin",
    ]);
    assert!(out.status.success(), "{out:?}");
    let source = std::fs::read_to_string(scratch.0.join("input.asm")).expect("source");
    let expected = asm198x::assemble_acme(&source).expect("library lexical selection");
    assert_eq!(
        expected.bytes.len(),
        2,
        "RTL and NOP must both emit instructions"
    );
    assert_eq!(
        std::fs::read(scratch.0.join("out.bin")).expect("bytes"),
        expected.bytes
    );
    std::fs::write(
        scratch.0.join("input.asm"),
        "*=$1000\n!cpu 65816\n rtl\n!cpu 6502\nafter rtl\n",
    )
    .expect("switch back");
    let out = scratch.run(&[
        "--dialect",
        "acme",
        "--cpu",
        "6502",
        "input.asm",
        "-o",
        "out.bin",
    ]);
    assert!(!out.status.success(), "switch back retained 65816: {out:?}");
}

#[test]
fn published_cpu_matrix_and_all_aliases_match_the_cli() {
    let doc = include_str!("../../../docs/book/src/reference/cli.md");
    let matrix: Vec<_> = doc
        .lines()
        .filter_map(|line| {
            let cells: Vec<_> = line.split('|').map(str::trim).collect();
            if cells.len() != 5 || !cells[1].starts_with('`') || !cells[2].starts_with('`') {
                return None;
            }
            Some((
                cells[1].trim_matches('`'),
                cells[2].trim_matches('`'),
                cells[3]
                    .split(',')
                    .map(|s| s.trim().trim_matches('`'))
                    .collect::<Vec<_>>(),
            ))
        })
        .collect();
    assert_eq!(
        matrix.len(),
        asm198x::dialect_table::DIALECTS.len(),
        "missing/extra matrix rows"
    );
    let scratch = Scratch::new("\n");
    for entry in asm198x::dialect_table::DIALECTS {
        let rows: Vec<_> = matrix.iter().filter(|row| row.0 == entry.name).collect();
        assert_eq!(rows.len(), 1, "matrix row for {}", entry.name);
        let (_, default, allowed) = rows[0];
        assert!(allowed.contains(default));
        for spelling in std::iter::once(&entry.name).chain(entry.aliases) {
            for args in [
                vec!["--cpu", spelling],
                vec!["--dialect", spelling, "--cpu", default],
                vec!["--dialect", entry.name, "--cpu", spelling],
            ] {
                let mut args = args;
                args.extend(["input.asm", "-o", "out.bin"]);
                let out = scratch.run(&args);
                assert!(out.status.success(), "{args:?}: {out:?}");
            }
        }
        for (_, cpu, _) in &matrix {
            let out = scratch.run(&[
                "--dialect",
                entry.name,
                "--cpu",
                cpu,
                "input.asm",
                "-o",
                "out.bin",
            ]);
            assert_eq!(
                out.status.success(),
                allowed.contains(cpu),
                "{}/{cpu}: {out:?}",
                entry.name
            );
        }
    }
}
