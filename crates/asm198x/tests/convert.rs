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
