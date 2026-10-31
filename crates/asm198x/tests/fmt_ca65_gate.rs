//! The gate the ca65 formatter shipped under, kept: formatted source
//! assembles exactly as the source did, and formatting it again changes
//! nothing.
//!
//! When it shipped, ca65 V2.19 itself assembled every source below, and
//! every ca65 fixture case, to the same bytes after formatting, or refused it
//! with the same errors (2026-10-31). This replays that check through the
//! walk, which matches ca65 on these sources, so it runs without ca65
//! installed.

use std::path::{Path, PathBuf};

use asm198x::source::{FsLoader, LoadError, SourceLoader};

fn formatted(text: &str) -> String {
    let once = asm198x::format_ca65(text).expect("formats");
    assert_eq!(
        asm198x::format_ca65(&once).expect("formats again"),
        once,
        "formatting again changed it:\n{once}"
    );
    once
}

type Outcome = Result<(Vec<u8>, std::collections::BTreeMap<String, i64>), (usize, String)>;

fn outcome(result: Result<asm198x::AssemblyResult, asm198x::AsmError>) -> Outcome {
    result
        .map(|r| (r.bytes, r.symbols))
        .map_err(|e| (e.line, e.message))
}

#[test]
fn formatted_verdict_sources_assemble_as_before() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/verdicts/6502.ndjson");
    let text = std::fs::read_to_string(path).expect("the 6502 verdicts");
    let mut checked = 0;
    let mut wrong = Vec::new();
    for line in text.lines() {
        let record: serde_json::Value = serde_json::from_str(line).expect("NDJSON");
        let (Some(source), true) = (
            record["source"].as_str(),
            record["t"] == "verdict" && record["dialect"] == "ca65",
        ) else {
            continue;
        };
        checked += 1;
        let after = formatted(source);
        if outcome(asm198x::assemble_ca65(source)) != outcome(asm198x::assemble_ca65(&after)) {
            wrong.push(format!(
                "{}: assembles differently once formatted:\n{after}",
                record["case"]
            ));
        }
    }
    assert!(checked >= 350, "checked {checked}");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Reads files from disk, formatting each source it hands over.
struct Formatting(FsLoader);

impl SourceLoader for Formatting {
    fn load_text(&self, request: &str, from: Option<&str>) -> Result<(String, String), LoadError> {
        let (path, text) = self.0.load_text(request, from)?;
        Ok((path, asm198x::format_ca65(&text).expect("formats")))
    }

    fn load_binary(&self, request: &str, from: Option<&str>) -> Result<Vec<u8>, LoadError> {
        self.0.load_binary(request, from)
    }
}

/// Every NES curriculum program, where the checkout is present, assembles
/// through the walk to the same bytes and symbols with every file it
/// includes formatted. ca65 V2.19 and ld65 gave the same result for all
/// 388 programs when the formatter shipped.
#[test]
fn formatted_curriculum_programs_assemble_as_before() {
    let Some(root) = std::env::var_os("ASM198X_CODE_SAMPLES")
        .map(PathBuf::from)
        .or_else(|| {
            let sibling = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../code-samples");
            sibling.is_dir().then_some(sibling)
        })
    else {
        eprintln!("no curriculum checkout; skipped");
        return;
    };
    let mut files = Vec::new();
    collect(&root.join("nintendo-entertainment-system"), &mut files);
    let mut wrong = Vec::new();
    for file in &files {
        let source = std::fs::read_to_string(file).expect("source");
        let after = formatted(&source);
        if !file
            .extension()
            .is_some_and(|ext| ext == "asm" || ext == "s")
        {
            continue;
        }
        let dir = file.parent().expect("a directory").to_path_buf();
        let name = file.display().to_string();
        let plain = FsLoader::new(dir.clone(), Vec::new());
        let before = asm198x::assemble_ca65_files(&source, &name, &plain)
            .map(|r| (r.bytes, r.symbols))
            .map_err(|e| e.error.message);
        let formatting = Formatting(FsLoader::new(dir, Vec::new()));
        let after = asm198x::assemble_ca65_files(&after, &name, &formatting)
            .map(|r| (r.bytes, r.symbols))
            .map_err(|e| e.error.message);
        if before != after {
            wrong.push(format!("{name}: assembles differently once formatted"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .extension()
            .is_some_and(|ext| ext == "asm" || ext == "s" || ext == "inc")
        {
            out.push(path);
        }
    }
}
