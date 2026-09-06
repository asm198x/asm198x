use asm198x::{AsmError, Code, Diagnostic};
use std::process::Command;

#[path = "support/scratch.rs"]
mod scratch;

#[test]
fn every_code_has_one_wire_name_cli_page_and_book_page() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/book/src/reference/diagnostics");
    let mut names = std::collections::BTreeSet::new();
    for code in Code::ALL {
        assert!(names.insert(code.as_str()));
        assert_eq!(Code::from_name(code.as_str()), Some(*code));
        assert_eq!(
            serde_json::to_value(code).expect("wire name"),
            code.as_str()
        );
        assert!(
            code.explanation()
                .starts_with(&format!("# {}\n", code.as_str()))
        );
        let book = std::fs::read_to_string(root.join(format!("{}.md", code.page_slug())))
            .expect("generated page");
        assert_eq!(book, code.explanation(), "book must use the embedded text");
        let cli = Command::new(env!("CARGO_BIN_EXE_asm198x"))
            .args(["--explain", code.as_str()])
            .output()
            .expect("CLI");
        assert!(cli.status.success());
        assert_eq!(cli.stdout, code.explanation().as_bytes());
        assert!(cli.stderr.is_empty());
    }
}

#[test]
fn unknown_missing_and_mixed_requests_fail_without_reading_source() {
    for args in [
        vec!["--explain"],
        vec!["--explain", "E9999"],
        vec!["--explain="],
        vec!["--explain", "AssemblyError", "nonexistent.asm"],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_asm198x"))
            .args(args)
            .output()
            .expect("CLI");
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&result.stderr).contains("cannot read"));
    }
    let result = Command::new(env!("CARGO_BIN_EXE_asm198x"))
        .arg("--explain=AssemblyError")
        .output()
        .expect("CLI");
    assert!(result.status.success());
    assert_eq!(result.stdout, Code::AssemblyError.explanation().as_bytes());
}

#[test]
fn raising_sites_classify_without_reinterpreting_human_messages() {
    let branch = asm198x::assemble_acme(" * = $1000\n bne $1082\n").expect_err("distance 128");
    assert_eq!(Diagnostic::from(branch).code, Code::BranchOutOfRange);
    assert!(asm198x::assemble_acme(" * = $1000\n bne $1081\n").is_ok());
    let long = asm198x::assemble_pasmo(" org $1000\n jr $1200\n").expect_err("JR distance");
    assert_eq!(long.code, Code::BranchOutOfRange);
    assert_eq!(
        asm198x::assemble_ca65(" .segment \"CODE\"\n bne far\n .res 128\nfar: rts\n")
            .expect_err("linked branch")
            .code,
        Code::BranchOutOfRange,
    );
    assert_eq!(
        asm198x::assemble_vasm(" bra.w $100000\n")
            .expect_err("68000 word branch")
            .code,
        Code::BranchOutOfRange,
    );
    let budget =
        asm198x::assemble_acme(" * = $c000\n; asm198x: cycles(start) <= 7\nstart lda #1\n rts\n")
            .expect_err("eight cycles");
    assert_eq!(budget.code, Code::CycleBudgetExceeded);
    for spelling in [
        "autobranchlength",
        "cescapes",
        "condundefzero",
        "noforwardrefmax",
        "operandsizewarning",
        "pcaspcr",
        "nosymbolcase",
        "symbolnocase",
    ] {
        assert_eq!(
            asm198x::assemble_lwasm(&format!(" pragma {spelling}\n fcb 1\n"))
                .expect_err("pragma refusal")
                .code,
            Code::UnsupportedFeature,
            "{spelling}"
        );
    }
    for error in [
        asm198x::assemble_lwasm(" dtb\n").expect_err("clockless"),
        asm198x::assemble_lwasm(" pragma autobranchlength\n").expect_err("pragma gap"),
        asm198x::assemble_sjasmplus(" BPLIST\n").expect_err("known gap"),
        asm198x::assemble_acme(" !cpu m65\n").expect_err("CPU gap"),
    ] {
        assert_eq!(Diagnostic::from(error).code, Code::UnsupportedFeature);
    }
    // Neither arbitrary source text nor a reference refusal is a tool gap.
    let error = AsmError::new(4, "branch target out of range");
    assert_eq!(Diagnostic::from(error).code, Code::AssemblyError);
    assert_eq!(
        asm198x::assemble_lwasm(" export foo\n")
            .expect_err("raw output refusal")
            .code,
        Code::AssemblyError
    );
    assert!(
        asm198x::assemble_lwasm(" if 0\n dtb\n pragma autobranchlength\n endc\n nop\n").is_ok()
    );
}

#[test]
#[cfg(not(feature = "lua"))]
fn missing_lua_feature_has_an_explanation() {
    assert_eq!(
        asm198x::assemble_sjasmplus(" LUA\n ENDLUA\n")
            .expect_err("Lua disabled")
            .code,
        Code::UnsupportedFeature
    );
}

#[test]
fn included_failure_keeps_code_location_and_human_explain_hint() {
    let dir = scratch::dir("explain-cli");
    std::fs::write(dir.join("main.asm"), " include \"bad.inc\"\n").expect("root");
    std::fs::write(dir.join("bad.inc"), " org $1000\n jr $1200\n").expect("include");
    for json in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_asm198x"));
        command
            .current_dir(&dir)
            .args(["--dialect", "sjasmplus", "main.asm"]);
        if json {
            command.arg("--message-format=json");
        }
        let result = command.output().expect("CLI");
        assert!(!result.status.success());
        if json {
            let diagnostics: serde_json::Value =
                serde_json::from_slice(&result.stdout).expect("JSON");
            assert_eq!(diagnostics[0]["code"], "BranchOutOfRange");
            assert!(
                diagnostics[0]["span"]["path"]
                    .as_str()
                    .expect("path")
                    .ends_with("bad.inc")
            );
            assert_eq!(diagnostics[0]["span"]["line"], 2);
        } else {
            let stderr = String::from_utf8_lossy(&result.stderr);
            assert!(stderr.contains("bad.inc:2"));
            assert!(stderr.contains("asm198x --explain BranchOutOfRange"));
        }
    }
    assert!(!dir.join("main.bin").exists());
}
