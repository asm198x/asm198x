//! Command-line constants must not impersonate lines in the user's source.
use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Scratch(PathBuf);
impl Scratch {
    fn new(source: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "asm198x-equ-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).expect("scratch");
        std::fs::write(path.join("program.s"), source).expect("source");
        Self(path)
    }
    fn run(&self, dialect: &str, mode: &str, count: usize, extra: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_asm198x"));
        command.current_dir(&self.0).args([
            "--dialect",
            dialect,
            "--message-format",
            mode,
            "program.s",
        ]);
        for n in 0..count {
            command.args(["--equ", &format!("FEATURE{n}=1")]);
        }
        command.args(extra).output().expect("CLI")
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn root_errors_name_original_lines_with_command_line_constants() {
    for dialect in ["pasmo", "pasmonext"] {
        for mode in ["human", "json"] {
            for count in 0..=2 {
                let dir = Scratch::new(" org $8000\n ld a,(\n");
                std::fs::write(dir.0.join("image.bin"), b"previous image").expect("seed");
                let run = dir.run(dialect, mode, count, &["-o", "image.bin"]);
                assert!(!run.status.success(), "{run:?}");
                assert_eq!(
                    std::fs::read(dir.0.join("image.bin")).expect("image"),
                    b"previous image"
                );
                if mode == "json" {
                    assert!(run.stderr.is_empty());
                    let errors: Value = serde_json::from_slice(&run.stdout).expect("JSON");
                    assert_eq!(errors[0]["span"]["path"], "program.s");
                    assert_eq!(errors[0]["span"]["line"], 2, "{dialect}/{count}: {errors}");
                } else {
                    let text = String::from_utf8_lossy(&run.stderr);
                    assert!(text.contains("program.s:2:"), "{dialect}/{count}: {text}");
                }
            }
        }
    }
}

#[test]
fn debug_cycles_and_listings_agree_with_unmodified_source() {
    for dialect in ["pasmo", "pasmonext"] {
        for mode in ["human", "json"] {
            for count in 0..=2 {
                let source = " org $8000\nstart: ld a,1\n";
                let dir = Scratch::new(source);
                let run = dir.run(
                    dialect,
                    mode,
                    count,
                    &[
                        "-o",
                        "image.bin",
                        "--listing-json=rows.json",
                        "--debug=program.debug198x",
                    ],
                );
                assert!(run.status.success(), "{run:?}");
                assert_eq!(
                    std::fs::read(dir.0.join("image.bin")).expect("image"),
                    [62, 1]
                );
                let listing: Value = serde_json::from_slice(
                    &std::fs::read(dir.0.join("rows.json")).expect("listing"),
                )
                .expect("JSON listing");
                assert_eq!(listing["lines"][0]["line"], 2, "{listing}");
                if mode == "json" {
                    assert!(run.stderr.is_empty());
                    let result: Value = serde_json::from_slice(&run.stdout).expect("JSON result");
                    assert_eq!(result["files"][0], "program.s");
                    for field in ["lines", "cycles"] {
                        assert_eq!(result["debug"][field][0]["line"], 2, "{result}");
                    }
                }
                let text =
                    std::fs::read_to_string(dir.0.join("program.debug198x")).expect("sidecar");
                let records: Vec<Value> = text
                    .lines()
                    .map(|line| serde_json::from_str(line).expect("NDJSON"))
                    .collect();
                assert!(records.iter().any(|r| r["line"] == 2), "{records:?}");
            }
        }
    }
}

#[test]
fn macro_errors_keep_definition_and_invocation_locations() {
    for mode in ["human", "json"] {
        for count in 0..=2 {
            let dir = Scratch::new("boom MACRO\n ld a,(\n ENDM\n org $8000\n boom\n");
            let run = dir.run("pasmo", mode, count, &[]);
            assert!(!run.status.success());
            if mode == "json" {
                assert!(run.stderr.is_empty());
                let errors: Value = serde_json::from_slice(&run.stdout).expect("JSON");
                let frames = &errors[0]["span"]["expansion_frames"];
                assert_eq!(frames[0]["defined_at"]["line"], 1, "{errors}");
                assert!(
                    frames[0]["defined_at"]["path"]
                        .as_str()
                        .expect("definition path")
                        .ends_with("program.s")
                );
                assert_eq!(frames[0]["invoked_at"]["line"], 5, "{errors}");
                assert_eq!(frames[0]["invoked_at"]["path"], "program.s");
            } else {
                let text = String::from_utf8_lossy(&run.stderr);
                assert!(text.contains("defined at program.s:1"), "{text}");
                assert!(text.contains("invoked at program.s:5"), "{text}");
            }
        }
    }
}

#[test]
fn invalid_command_line_expression_does_not_blame_root_source() {
    for mode in ["human", "json"] {
        let dir = Scratch::new(" org $8000\n ld a,1\n");
        let run = dir.run("pasmo", mode, 0, &["--equ", "FEATURE=("]);
        assert!(!run.status.success());
        if mode == "json" {
            assert!(run.stderr.is_empty());
            let errors: Value = serde_json::from_slice(&run.stdout).expect("JSON");
            assert_eq!(
                errors[0]["span"]["path"], "<command-line --equ>",
                "{errors}"
            );
            assert_eq!(errors[0]["span"]["line"], 1);
        } else {
            let text = String::from_utf8_lossy(&run.stderr);
            assert!(text.contains("<command-line --equ>:1"), "{text}");
            assert!(!text.contains("program.s:1"), "{text}");
        }
    }
}

#[test]
fn include_errors_and_include_chain_keep_original_lines() {
    for mode in ["human", "json"] {
        for count in 0..=2 {
            let dir = Scratch::new(" org $8000\n include \"bad.inc\"\n");
            std::fs::write(dir.0.join("bad.inc"), "; comment\n ld a,(\n").expect("include");
            let run = dir.run("pasmo", mode, count, &[]);
            assert!(!run.status.success());
            if mode == "json" {
                assert!(run.stderr.is_empty());
                let errors: Value = serde_json::from_slice(&run.stdout).expect("JSON");
                assert!(
                    errors[0]["span"]["path"]
                        .as_str()
                        .expect("path")
                        .ends_with("bad.inc")
                );
                assert_eq!(errors[0]["span"]["line"], 2, "{errors}");
            } else {
                let text = String::from_utf8_lossy(&run.stderr);
                assert!(text.contains("bad.inc:2:"), "{text}");
                assert!(text.contains("included from program.s:2"), "{text}");
            }
        }
    }
}

#[test]
fn definitions_select_conditional_builds_without_changing_source_provenance() {
    for dialect in ["pasmo", "pasmonext"] {
        for mode in ["human", "json"] {
            for feature in [0, 1] {
                let source = " org $8000\n IF FEATURE\n ld a,1\n ELSE\n ld a,2\n ENDIF\n include \"tail.inc\"\n";
                let dir = Scratch::new(source);
                std::fs::write(dir.0.join("tail.inc"), "; included code\n nop\n").expect("include");
                let definition = format!("FEATURE={feature}");
                let run = dir.run(
                    dialect,
                    mode,
                    0,
                    &[
                        "--equ",
                        &definition,
                        "-o",
                        "image.bin",
                        "--listing-json=rows.json",
                    ],
                );
                assert!(run.status.success(), "{run:?}");
                let bytes = std::fs::read(dir.0.join("image.bin")).expect("image");
                let baseline = Scratch::new(&format!("FEATURE equ {feature}\n{source}"));
                std::fs::write(baseline.0.join("tail.inc"), "; included code\n nop\n")
                    .expect("include");
                let control = baseline.run(dialect, mode, 0, &["-o", "image.bin"]);
                assert!(control.status.success(), "{control:?}");
                assert_eq!(
                    bytes,
                    std::fs::read(baseline.0.join("image.bin")).expect("control")
                );
                assert_eq!(bytes, [62, if feature == 1 { 1 } else { 2 }, 0]);
                let listing: Value = serde_json::from_slice(
                    &std::fs::read(dir.0.join("rows.json")).expect("listing"),
                )
                .expect("JSON listing");
                let rows = listing["lines"].as_array().expect("rows");
                assert_eq!(rows.len(), 2, "{listing}");
                assert_eq!(rows[0]["file"], "program.s");
                assert_eq!(rows[0]["line"], if feature == 1 { 3 } else { 5 });
                assert!(
                    rows[1]["file"]
                        .as_str()
                        .expect("included file")
                        .ends_with("tail.inc")
                );
                assert_eq!(rows[1]["line"], 2);
                if mode == "json" {
                    let result: Value = serde_json::from_slice(&run.stdout).expect("JSON");
                    for field in ["lines", "cycles"] {
                        let records = result["debug"][field].as_array().expect("records");
                        assert_eq!(records.len(), rows.len());
                        for (record, row) in records.iter().zip(rows) {
                            let file = record["file"].as_u64().unwrap_or(0) as usize;
                            assert_eq!(result["files"][file], row["file"]);
                            assert_eq!(record["line"], row["line"]);
                        }
                    }
                }
            }
        }
    }
}
