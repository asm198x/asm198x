//! Dialect conversion (#502): pasmo source re-emitted as sjasmplus,
//! self-verified — every `convert` call below has already proven its output
//! byte-identical before returning, so these tests pin the *shape* of the
//! output and the honesty of the failures.

use asm198x::convert;

/// AE1's hermetic half: a pasmo program with labels, data, equ, and a loop
/// converts; the conversion verified itself (assemble both, byte-compare)
/// inside `convert`, and the output keeps names and structure — source
/// migration, not a hex dump.
#[test]
fn a_pasmo_program_converts_verified() {
    let src = "        org 8000h\nCOUNT   equ 3\nstart:  ld b, COUNT\nloop:   djnz loop\n        ret\nmsg:    db \"hi\", 0\n";
    let c = convert("pasmo", "sjasmplus", src).expect("converts");
    assert!(c.output.contains("start:"), "labels survive:\n{}", c.output);
    assert!(
        c.output.contains("djnz loop"),
        "operands keep their names:\n{}",
        c.output
    );
    assert!(
        c.output.contains("COUNT"),
        "constants survive by name:\n{}",
        c.output
    );
}

/// The emblematic rewrite: pasmo closes `REPT` with `ENDM`, which sjasmplus
/// refuses; the block closer is rewritten in the author's case, while a
/// macro's `ENDM` — the same word — stays, because sjasmplus wants it there.
#[test]
fn rept_endm_becomes_endr_and_macro_endm_stays() {
    let src = "        org 8000h\n        rept 3\n        nop\n        endm\n";
    let c = convert("pasmo", "sjasmplus", src).expect("converts");
    assert!(c.output.to_lowercase().contains("endr"), "{}", c.output);
    assert!(
        !c.output.to_lowercase().contains("endm"),
        "the repetition closer is gone:\n{}",
        c.output
    );

    let src = "        org 8000h\nm       macro\n        nop\n        endm\n        m\n";
    let c = convert("pasmo", "sjasmplus", src).expect("converts");
    assert!(
        c.output.to_lowercase().contains("endm"),
        "the macro closer stays:\n{}",
        c.output
    );
}

/// AE2's error posture: input that does not assemble under the source
/// dialect is a reported error naming the side that failed — never output.
#[test]
fn unassemblable_input_reports_the_source_side() {
    let e = convert("pasmo", "sjasmplus", "        xyzzy 1\n").expect_err("no such instruction");
    assert!(
        e.message.contains("does not assemble under pasmo"),
        "{}",
        e.message
    );
}

/// An unknown pair is refused by name, pointing at the tracker.
#[test]
fn unknown_pairs_are_refused() {
    let e = convert("acme", "ca65", "        nop\n").expect_err("not yet");
    assert!(e.message.contains("no converter"), "{}", e.message);
}

/// AE1 whole: the differential — the converted output assembles under *real*
/// sjasmplus to the same bytes real pasmo makes of the input.
#[test]
#[ignore = "needs pasmo + sjasmplus on PATH"]
fn conversion_matches_the_real_tools() {
    let dir = std::env::temp_dir().join(format!("asm198x-convert-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let src = "        org 8000h\nstart:  ld a, 5\n        rept 2\n        inc a\n        endm\n        ret\n";
    std::fs::write(dir.join("in.asm"), src).expect("write");
    let c = convert("pasmo", "sjasmplus", src).expect("converts");
    std::fs::write(dir.join("out.asm"), &c.output).expect("write");
    let run = |cmd: &str, args: &[&str]| {
        let out = std::process::Command::new(cmd)
            .args(args)
            .current_dir(&dir)
            .output()
            .unwrap_or_else(|e| panic!("run {cmd}: {e}"));
        assert!(
            out.status.success(),
            "{cmd} failed: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    };
    run("pasmo", &["--bin", "in.asm", "ref.bin"]);
    run("sjasmplus", &["--raw=conv.bin", "out.asm"]);
    let reference = std::fs::read(dir.join("ref.bin")).expect("reference");
    let converted = std::fs::read(dir.join("conv.bin")).expect("converted");
    assert_eq!(converted, reference, "real tools agree on the conversion");
}

#[path = "support/scratch.rs"]
mod scratch;

#[test]
fn unresolved_conversion_files_have_actionable_guidance_and_keep_the_source_error() {
    for directive in ["include", "incbin"] {
        let source = format!(" org 8000h\n {directive} \"child.dat\"\n ret\n");
        for from in ["pasmo", "pasmonext"] {
            let original =
                asm198x::assemble_pasmo(&source).expect_err("single source has no loader");
            assert!(original.message.contains("the CLI resolves"));
            let error = convert(from, "sjasmplus", &source).expect_err("conversion has no loader");
            assert_eq!(error.line, original.line);
            assert_eq!(error.span, original.span);
            assert_eq!(error.code, original.code);
            assert!(error.message.contains(directive), "{error:?}");
            assert!(error.message.contains("child.dat"), "{error:?}");
            assert!(error.message.contains("self-contained source"), "{error:?}");
            assert!(!error.message.contains("the CLI resolves"), "{error:?}");
            assert!(
                !error.message.contains("multi-file entry point"),
                "{error:?}"
            );
        }
    }
}

#[test]
fn inactive_file_directives_do_not_prevent_verified_conversion() {
    for directive in ["include", "incbin"] {
        let source = format!(" org 8000h\n if 0\n {directive} \"missing.dat\"\n endif\n ret\n");
        for from in ["pasmo", "pasmonext"] {
            let converted = convert(from, "sjasmplus", &source).expect("inactive directive");
            assert_eq!(
                asm198x::assemble_sjasmplus(&converted.output)
                    .expect("converted source")
                    .bytes,
                [0xc9]
            );
        }
    }
}

#[test]
fn cli_conversion_explains_existing_files_and_preserves_outputs() {
    use std::process::Command;
    let dir = scratch::dir("convert-file-scope");
    std::fs::write(dir.join("child.inc"), " db 42\n").expect("include");
    std::fs::write(dir.join("data.bin"), [42]).expect("binary include");
    for (directive, name) in [("include", "child.inc"), ("incbin", "data.bin")] {
        for from in ["pasmo", "pasmonext"] {
            let source = format!(" org 8000h\n {directive} \"{name}\"\n ret\n");
            std::fs::write(dir.join("input.asm"), &source).expect("input");
            let assembled = Command::new(env!("CARGO_BIN_EXE_asm198x"))
                .current_dir(&dir)
                .args(["--dialect", from, "input.asm", "-o", "image.bin"])
                .output()
                .expect("ordinary assembly");
            assert!(assembled.status.success(), "{assembled:?}");
            assert_eq!(
                std::fs::read(dir.join("image.bin")).expect("image"),
                [42, 0xc9]
            );
            for write_file in [true, false] {
                std::fs::write(dir.join("out.asm"), b"previous conversion").expect("old output");
                let mut command = Command::new(env!("CARGO_BIN_EXE_asm198x"));
                command.current_dir(&dir).args([
                    "convert",
                    "--from",
                    from,
                    "--to",
                    "sjasmplus",
                    "input.asm",
                ]);
                if write_file {
                    command.args(["-o", "out.asm"]);
                }
                let result = command.output().expect("convert");
                assert!(!result.status.success(), "{result:?}");
                assert!(result.stdout.is_empty(), "{result:?}");
                let message = String::from_utf8_lossy(&result.stderr);
                assert!(message.contains("input.asm: line 2:"), "{message}");
                assert!(message.contains("self-contained source"), "{message}");
                assert!(!message.contains("the CLI resolves"), "{message}");
                assert_eq!(
                    std::fs::read(dir.join("out.asm")).expect("preserved output"),
                    b"previous conversion"
                );
                assert_eq!(
                    std::fs::read_to_string(dir.join("input.asm")).expect("preserved input"),
                    source
                );
            }
        }
    }
}
