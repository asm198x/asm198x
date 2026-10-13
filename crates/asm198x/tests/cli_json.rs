//! U4: `--message-format=json` CLI mode (R3). The result and diagnostics are
//! machine-consumable on stdout; the human default is unchanged.

use std::path::PathBuf;
use std::process::Command;

use asm198x::{AssemblyResult, Diagnostic, Severity};

/// The built `asm198x` binary under test.
fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_asm198x"))
}

/// Write `source` to a uniquely-named temp file and return its path (so parallel
/// tests never share an input).
fn temp_source(tag: &str, source: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("asm198x-cli-json-{tag}.s"));
    std::fs::write(&path, source).expect("write temp source");
    path
}

#[test]
fn json_success_emits_result_with_bytes_and_symbols() {
    let src = temp_source("ok", "*=$8000\nstart:\n        lda #$05\n        rts\n");
    let out = bin()
        .args(["--cpu", "6502", "--message-format=json"])
        .arg(&src)
        .output()
        .expect("run asm198x");

    assert!(out.status.success(), "clean assemble should succeed");
    let result: AssemblyResult =
        serde_json::from_slice(&out.stdout).expect("stdout is an AssemblyResult JSON");
    assert_eq!(result.bytes, vec![0xA9, 0x05, 0x60], "lda #$05 / rts");
    assert_eq!(result.origin, Some(0x8000));
    assert_eq!(result.symbols.get("start"), Some(&0x8000));
    assert!(result.diagnostics.is_empty(), "no diagnostics on success");
}

#[test]
fn json_failure_emits_diagnostics_with_span_and_code() {
    // `lda #$fff` — an immediate that overflows a byte, a fatal error.
    let src = temp_source("bad", "*=$8000\n        lda #$fff\n");
    let out = bin()
        .args(["--cpu", "6502", "--message-format=json"])
        .arg(&src)
        .output()
        .expect("run asm198x");

    assert!(!out.status.success(), "a bad program should fail");
    let diagnostics: Vec<Diagnostic> =
        serde_json::from_slice(&out.stdout).expect("stdout is a Diagnostic array JSON");
    assert_eq!(diagnostics.len(), 1, "one fatal diagnostic");
    let diag = &diagnostics[0];
    assert!(matches!(diag.severity, Severity::Error));
    // A column-accurate span (U3): the line points at the offending
    // instruction, the column at its operand field (`#$fff`, column 13).
    assert_eq!(diag.span.as_ref().map(|s| s.line), Some(2));
    assert_eq!(diag.span.as_ref().map(|s| s.col), Some(13));
    assert!(!diag.message.is_empty(), "a human-readable message");
}

#[test]
fn json_success_still_writes_output_file() {
    let src = temp_source("write", "*=$8000\n        rts\n");
    let obj = std::env::temp_dir().join("asm198x-cli-json-write.bin");
    let _ = std::fs::remove_file(&obj);
    let out = bin()
        .args(["--cpu", "6502", "--message-format=json"])
        .arg(&src)
        .arg("-o")
        .arg(&obj)
        .output()
        .expect("run asm198x");

    assert!(out.status.success());
    assert_eq!(
        std::fs::read(&obj).expect("the -o file was written"),
        vec![0x60],
        "json mode still writes the byte output to -o",
    );
}

#[test]
fn human_mode_is_unchanged_without_the_flag() {
    let src = temp_source("human", "*=$8000\n        rts\n");
    let obj = std::env::temp_dir().join("asm198x-cli-json-human.bin");
    let out = bin()
        .args(["--cpu", "6502"])
        .arg(&src)
        .arg("-o")
        .arg(&obj)
        .output()
        .expect("run asm198x");

    assert!(out.status.success());
    // The human summary goes to stderr; stdout stays empty (no JSON).
    assert!(out.stdout.is_empty(), "no JSON on stdout in human mode");
    let summary = String::from_utf8_lossy(&out.stderr);
    assert!(
        summary.contains("assembled") && summary.contains("byte"),
        "human summary on stderr: {summary}"
    );
}

#[test]
fn invalid_message_format_errors_cleanly() {
    let src = temp_source("xml", "*=$8000\n        rts\n");
    let out = bin()
        .args(["--cpu", "6502", "--message-format=xml"])
        .arg(&src)
        .output()
        .expect("run asm198x");

    assert!(!out.status.success(), "an unknown format is an error");
    assert!(out.stdout.is_empty(), "no JSON emitted for a bad flag");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("invalid --message-format"),
        "clear error message: {err}"
    );
}

// ---------------------------------------------------------------------------
// Subcommands (`asm198x asm|disasm|fmt`) — the CLI shape v1.0 freezes, per
// `decisions/packaging-and-cpu-roadmap.md` and `decisions/v1-scope.md`.
// ---------------------------------------------------------------------------

/// Assembling stays the default operation, so an invocation with no subcommand
/// keeps working. This is the external contract: Code198x's capture harness
/// drives the binary this way (`_capture/verify.py`), from a workflow pinned to
/// a revision, and every published example spells it like this.
#[test]
fn a_bare_invocation_still_assembles() {
    let src = temp_source("bare", "\tld a,1\n");
    let out = temp_source("bare-out", "").with_extension("bin");
    let run = bin()
        .args(["--dialect", "pasmo"])
        .arg(&src)
        .arg("-o")
        .arg(&out)
        .output()
        .expect("run asm198x");
    assert!(run.status.success(), "bare invocation must assemble");
    assert_eq!(std::fs::read(&out).expect("output"), vec![0x3E, 0x01]);
}

/// `asm` is the explicit spelling of the default, and must produce exactly what
/// the bare form does.
#[test]
fn the_asm_subcommand_matches_the_bare_form() {
    let src = temp_source("explicit", "\tld a,1\n");
    let out = temp_source("explicit-out", "").with_extension("bin");
    let run = bin()
        .args(["asm", "--dialect", "pasmo"])
        .arg(&src)
        .arg("-o")
        .arg(&out)
        .output()
        .expect("run asm198x");
    assert!(run.status.success(), "`asm` must assemble");
    assert_eq!(std::fs::read(&out).expect("output"), vec![0x3E, 0x01]);
}

/// `disasm` and `fmt` are subcommands, not flags.
#[test]
fn disasm_and_fmt_are_subcommands() {
    let src = temp_source("modes", "\tld a,1\n");
    let fmt = bin()
        .args(["fmt", "--dialect", "pasmo"])
        .arg(&src)
        .output()
        .expect("run asm198x");
    assert!(fmt.status.success(), "`fmt` must run");

    let bin_path = temp_source("modes-bin", "").with_extension("bin");
    std::fs::write(&bin_path, [0x3E, 0x01]).expect("write");
    let dis = bin()
        .args(["disasm", "--dialect", "pasmo"])
        .arg(&bin_path)
        .output()
        .expect("run asm198x");
    assert!(dis.status.success(), "`disasm` must run");
    assert!(
        String::from_utf8_lossy(&dis.stdout).contains("LD A,$01"),
        "disasm should render the instruction"
    );
}

/// The withdrawn flags say where the operation went. A bare "unknown flag"
/// would be accurate and useless — anyone hitting it has a working command line
/// from an older release and needs the one-word translation, not a rejection.
#[test]
fn the_withdrawn_mode_flags_name_their_replacement() {
    for (flag, want) in [("--disasm", "asm198x disasm"), ("--fmt", "asm198x fmt")] {
        let src = temp_source("withdrawn", "\tnop\n");
        let out = bin()
            .args(["--dialect", "pasmo", flag])
            .arg(&src)
            .output()
            .expect("run asm198x");
        assert!(!out.status.success(), "{flag} must be rejected");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(want),
            "{flag} should point at `{want}`, got: {stderr}"
        );
    }
}

/// Own the complete output directory so failed assertions also clean up.
struct NativeOutputDir(PathBuf);

impl NativeOutputDir {
    fn new(tag: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("asm198x-json-native-{}-{tag}", std::process::id()));
        std::fs::create_dir(&path).expect("create isolated native-output directory");
        Self(path)
    }
}

impl Drop for NativeOutputDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn json_source_named_images_match_human_output_beside_the_source() {
    for (tag, dialect, source, expected, code) in [
        (
            "cbm",
            "acme",
            "!to \"named.out\", cbm\n*=$8000\nrts\n",
            vec![0, 0x80, 0x60],
            vec![0x60],
        ),
        (
            "apple",
            "acme",
            "!to \"named.out\", apple\n*=$8000\nrts\n",
            vec![0, 0x80, 1, 0, 0x60],
            vec![0x60],
        ),
        (
            "plain",
            "acme",
            "!to \"named.out\", plain\n*=$8000\nrts\n",
            vec![0x60],
            vec![0x60],
        ),
        (
            "vasm",
            "vasm",
            " output \"named.out\"\n rts\n",
            vec![0x4e, 0x75],
            vec![0x4e, 0x75],
        ),
    ] {
        let dir = NativeOutputDir::new(&format!("source-{tag}"));
        let mut images = Vec::new();
        for mode in ["human", "json"] {
            let source_dir = dir.0.join(mode);
            std::fs::create_dir(&source_dir).expect("source directory");
            let source_path = source_dir.join("program.s");
            std::fs::write(&source_path, source).expect("source");
            let run = bin()
                .current_dir(&dir.0)
                .args(["--dialect", dialect, &format!("--message-format={mode}")])
                .arg(&source_path)
                .output()
                .expect("assemble");
            assert!(
                run.status.success(),
                "{tag}/{mode}: {}",
                String::from_utf8_lossy(&run.stderr)
            );
            if mode == "json" {
                let result: AssemblyResult =
                    serde_json::from_slice(&run.stdout).expect("JSON result");
                assert_eq!(result.bytes, code, "JSON retains machine code");
            }
            assert!(
                !dir.0.join("named.out").exists(),
                "output is source-relative, not cwd-relative"
            );
            assert!(
                !source_path.with_extension("bin").exists(),
                "no extra default image"
            );
            images.push(std::fs::read(source_dir.join("named.out")).expect("source-named image"));
        }
        assert_eq!(images[0], expected, "{tag} native framing");
        assert_eq!(
            images[0], images[1],
            "{tag} reporting does not change output"
        );
    }
}

#[test]
fn json_source_named_symbols_match_human_and_respect_cli_precedence() {
    let dir = NativeOutputDir::new("source-symbols");
    let mut symbols = Vec::new();
    for mode in ["human", "json"] {
        let source_dir = dir.0.join(mode);
        std::fs::create_dir(&source_dir).expect("source directory");
        let src = source_dir.join("program.s");
        std::fs::write(&src, "!symbollist \"named.sym\"\n*=$8000\nstart: rts\n").expect("source");
        let run = bin()
            .current_dir(&dir.0)
            .args(["--dialect", "acme", &format!("--message-format={mode}")])
            .arg(&src)
            .output()
            .expect("assemble");
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        if mode == "json" {
            let result: AssemblyResult = serde_json::from_slice(&run.stdout).expect("JSON result");
            assert_eq!(result.symbols.get("start"), Some(&0x8000));
            assert!(
                !src.with_extension("bin").exists(),
                "a symbol request does not request an image"
            );
        }
        symbols.push(std::fs::read(source_dir.join("named.sym")).expect("source-named symbols"));
        std::fs::remove_file(source_dir.join("named.sym")).expect("remove first symbols");
        let run = bin()
            .current_dir(&dir.0)
            .args([
                "--dialect",
                "acme",
                &format!("--message-format={mode}"),
                "--sym",
            ])
            .arg(&src)
            .output()
            .expect("assemble with CLI symbol override");
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert!(
            !source_dir.join("named.sym").exists(),
            "--sym overrides source name"
        );
        assert_eq!(
            std::fs::read(src.with_extension("sym")).expect("CLI symbols"),
            symbols[0]
        );
    }
    assert_eq!(symbols[0], symbols[1]);
    assert!(!symbols[0].is_empty());
}

#[test]
fn json_source_names_reject_escaping_paths_but_cli_overrides_them() {
    let dir = NativeOutputDir::new("source-paths");
    let source_dir = dir.0.join("src");
    std::fs::create_dir(&source_dir).expect("source directory");
    let src = source_dir.join("program.s");
    let escaped = dir.0.join("escaped.out");
    for directive in ["!to", "!symbollist"] {
        for named in [
            "../escaped.out".to_owned(),
            escaped.to_string_lossy().into_owned(),
        ] {
            std::fs::write(&src, format!("{directive} \"{named}\"\n*=$8000\nrts\n"))
                .expect("source");
            let run = bin()
                .args(["--dialect", "acme", "--message-format=json"])
                .arg(&src)
                .output()
                .expect("reject path");
            assert!(
                !run.status.success(),
                "unsafe source name must fail: {directive} {named}"
            );
            assert!(String::from_utf8_lossy(&run.stderr).contains("source-named output"));
            assert!(!escaped.exists());
            let override_path = dir.0.join("override.out");
            let mut command = bin();
            command
                .args(["--dialect", "acme", "--message-format=json"])
                .arg(&src);
            if directive == "!to" {
                command.arg("-o").arg(&override_path);
            } else {
                command.arg(format!("--sym={}", override_path.display()));
            }
            let run = command.output().expect("override path");
            assert!(
                run.status.success(),
                "{}",
                String::from_utf8_lossy(&run.stderr)
            );
            let _: AssemblyResult = serde_json::from_slice(&run.stdout).expect("JSON result");
            assert!(override_path.exists(), "CLI destination written");
            assert!(
                !escaped.exists(),
                "overridden source destination never written"
            );
            std::fs::remove_file(override_path).expect("remove override");
        }
    }
}

#[test]
fn json_output_override_ignores_source_framing_and_plain_queries_write_nothing() {
    let dir = NativeOutputDir::new("source-override");
    let src = dir.0.join("program.s");
    std::fs::write(&src, "!to \"named.prg\", cbm\n*=$8000\nrts\n").expect("source");
    let out = dir.0.join("override.bin");
    for mode in ["human", "json"] {
        let run = bin()
            .args(["--dialect", "acme", &format!("--message-format={mode}")])
            .arg(&src)
            .arg("-o")
            .arg(&out)
            .output()
            .expect("assemble");
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(std::fs::read(&out).expect("CLI image"), [0x60]);
        assert!(!dir.0.join("named.prg").exists());
    }
    std::fs::remove_file(out).expect("remove override");
    std::fs::write(&src, "*=$8000\nrts\n").expect("plain source");
    let run = bin()
        .args(["--dialect", "acme", "--message-format=json"])
        .arg(&src)
        .output()
        .expect("plain JSON query");
    assert!(run.status.success());
    let result: AssemblyResult = serde_json::from_slice(&run.stdout).expect("JSON result");
    assert_eq!(result.bytes, [0x60]);
    assert_eq!(
        std::fs::read_dir(&dir.0).expect("directory").count(),
        1,
        "only source remains"
    );
}

#[test]
fn json_source_named_output_keeps_container_precedence_and_sidecar_collision_checks() {
    let dir = NativeOutputDir::new("source-container");
    let src = dir.0.join("program.s");
    std::fs::write(&src, "!to \"named.apple\", apple\n*=$8000\nrts\n").expect("source");
    let run = bin()
        .args(["--dialect", "acme", "--message-format=json", "--prg"])
        .arg(&src)
        .output()
        .expect("CLI container overrides source");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        std::fs::read(src.with_extension("prg")).expect("PRG image"),
        [0, 0x80, 0x60]
    );
    assert!(!dir.0.join("named.apple").exists());
    std::fs::write(
        &src,
        "!to \"named.prg\", cbm\n!symbollist \"named.prg\"\n*=$8000\nrts\n",
    )
    .expect("colliding source");
    let run = bin()
        .args(["--dialect", "acme", "--message-format=json"])
        .arg(&src)
        .output()
        .expect("reject colliding symbols");
    assert!(!run.status.success());
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("refusing to overwrite the output image")
    );
    assert_eq!(
        std::fs::read(dir.0.join("named.prg")).expect("image remains intact"),
        [0, 0x80, 0x60]
    );
}

#[test]
fn json_native_images_match_human_output_and_keep_machine_code_in_result() {
    for (flag, extension, dialect, source, code) in [
        (
            "--tap",
            "tap",
            "pasmo",
            "org $8000\nstart: di\njr start\nend start\n",
            vec![0xf3, 0x18, 0xfd],
        ),
        (
            "--tapbas",
            "tap",
            "pasmo",
            "org $8000\nstart: di\njr start\nend start\n",
            vec![0xf3, 0x18, 0xfd],
        ),
        (
            "--tzx",
            "tzx",
            "pasmo",
            "org $8000\nstart: di\njr start\nend start\n",
            vec![0xf3, 0x18, 0xfd],
        ),
        (
            "--tzxbas",
            "tzx",
            "pasmo",
            "org $8000\nstart: di\njr start\nend start\n",
            vec![0xf3, 0x18, 0xfd],
        ),
        (
            "--sna",
            "sna",
            "pasmo",
            "org $8000\nstart: di\njr start\nend start\n",
            vec![0xf3, 0x18, 0xfd],
        ),
        ("--prg", "prg", "acme", "*=$8000\n rts\n", vec![0x60]),
    ] {
        let dir = NativeOutputDir::new(flag);
        std::fs::write(dir.0.join("program.s"), source).expect("write source");
        let image = dir.0.join(format!("program.{extension}"));
        // Exercise explicit and derived output paths. Use the same path for
        // both report modes: Pasmo puts the output name in tape headers.
        for explicit in [true, false] {
            for json in [false, true] {
                let mut command = bin();
                command
                    .current_dir(&dir.0)
                    .args(["--dialect", dialect, flag, "program.s"]);
                if explicit {
                    command.args(["-o", &format!("program.{extension}")]);
                }
                if json {
                    command.arg("--message-format=json");
                }
                let run = command.output().expect("run assembler");
                assert!(
                    run.status.success(),
                    "{flag}: {}",
                    String::from_utf8_lossy(&run.stderr)
                );
                let bytes = std::fs::read(&image).expect("native artifact written");
                assert!(bytes.len() > code.len(), "{flag} must frame the code");
                if json {
                    let result: AssemblyResult =
                        serde_json::from_slice(&run.stdout).expect("JSON result");
                    assert_eq!(result.bytes, code, "JSON bytes retain machine code");
                    assert_eq!(
                        bytes,
                        std::fs::read(dir.0.join("human.image")).expect("human image"),
                        "{flag}: report mode cannot change artifact bytes"
                    );
                } else {
                    std::fs::write(dir.0.join("human.image"), bytes).expect("retain human image");
                }
                std::fs::remove_file(&image).expect("remove output before next invocation");
            }
        }
    }
}

#[test]
fn json_rejects_native_formats_for_incompatible_dialects_before_writing() {
    for (flag, dialect, source) in [
        ("--tapbas", "acme", "*=$8000\n rts\n"),
        ("--sna", "acme", "*=$8000\n rts\n"),
        ("--prg", "pasmo", "org $8000\n ret\n"),
    ] {
        let dir = NativeOutputDir::new(&format!("invalid-{flag}"));
        std::fs::write(dir.0.join("program.s"), source).expect("write source");
        let run = bin()
            .current_dir(&dir.0)
            .args([
                "--dialect",
                dialect,
                flag,
                "--message-format=json",
                "program.s",
                "-o",
                "image",
            ])
            .output()
            .expect("run assembler");
        assert!(!run.status.success(), "{flag} must reject {dialect}");
        assert!(String::from_utf8_lossy(&run.stderr).contains("only for"));
        assert!(!dir.0.join("image").exists());
    }
}

#[test]
fn json_native_framing_errors_are_diagnostics_and_do_not_write_output() {
    for (flag, source) in [
        ("--sna", "org $8000\n ret\n"),
        ("--tapbas", "org $ffff\n db 1,2\n"),
        ("--tzxbas", "org $ffff\n db 1,2\n"),
    ] {
        let dir = NativeOutputDir::new(&format!("no-entry-{flag}"));
        std::fs::write(dir.0.join("program.s"), source).expect("write source");
        let run = bin()
            .current_dir(&dir.0)
            .args([
                "--dialect",
                "pasmo",
                flag,
                "--message-format=json",
                "program.s",
                "-o",
                "image",
            ])
            .output()
            .expect("run assembler");
        assert!(
            !run.status.success(),
            "{flag} must reject invalid framing inputs"
        );
        let diagnostics: Vec<Diagnostic> =
            serde_json::from_slice(&run.stdout).expect("JSON diagnostics");
        assert_eq!(diagnostics.len(), 1);
        assert!(matches!(diagnostics[0].severity, Severity::Error));
        assert!(!dir.0.join("image").exists());
    }
}
