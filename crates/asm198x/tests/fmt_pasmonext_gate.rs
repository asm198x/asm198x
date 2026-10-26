//! The gate the PasmoNext formatter shipped under, kept: formatted source
//! assembles exactly as the source did, and formatting it again changes
//! nothing.
//!
//! When it shipped, PasmoNext 0.1.3 itself assembled every formatted source
//! below to the same bytes and symbols (2026-10-26). This replays that check
//! through Asm198x, whose PasmoNext path matches PasmoNext on these sources,
//! so it runs without PasmoNext installed.

use std::path::Path;

fn same(source: &str, z80n: bool) -> Result<(), String> {
    let formatted = asm198x::format_pasmo(source).map_err(|e| e.message)?;
    if asm198x::format_pasmo(&formatted).map_err(|e| e.message)? != formatted {
        return Err("formatting again changed it".into());
    }
    let assemble = |text: &str| {
        let result = if z80n {
            asm198x::assemble_pasmonext(text)
        } else {
            asm198x::assemble_pasmo(text)
        };
        result
            .map(|r| (r.bytes, r.symbols))
            .map_err(|e| (e.line, e.message))
    };
    if assemble(source) == assemble(&formatted) {
        Ok(())
    } else {
        Err(format!(
            "assembles differently once formatted:\n{formatted}"
        ))
    }
}

#[test]
fn formatted_verdict_sources_assemble_as_before() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/verdicts/z80.ndjson");
    let text = std::fs::read_to_string(path).expect("the Z80 verdicts");
    let mut checked = 0;
    let mut wrong = Vec::new();
    for line in text.lines() {
        let record: serde_json::Value = serde_json::from_str(line).expect("NDJSON");
        let dialect = record["dialect"].as_str().unwrap_or_default();
        let (Some(source), true) = (
            record["source"].as_str(),
            record["t"] == "verdict" && matches!(dialect, "pasmo" | "pasmonext"),
        ) else {
            continue;
        };
        checked += 1;
        if let Err(why) = same(source, dialect == "pasmonext") {
            wrong.push(format!("{}: {why}", record["case"]));
        }
    }
    assert!(checked >= 1_230, "checked {checked}");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Every Spectrum curriculum file, where the checkout is present, formats to
/// a fixed point. That the formatted files, includes and all, assemble as
/// before was measured on PasmoNext itself when the formatter shipped; the
/// result is in the walk's plan.
#[test]
fn formatted_curriculum_sources_assemble_as_before() {
    let Some(root) = std::env::var_os("ASM198X_CODE_SAMPLES")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            let sibling = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../code-samples");
            sibling.is_dir().then_some(sibling)
        })
    else {
        eprintln!("no curriculum checkout; skipped");
        return;
    };
    let spectrum = root.join("sinclair-zx-spectrum");
    let mut files = Vec::new();
    collect(&spectrum, &mut files);
    let mut wrong = Vec::new();
    for file in &files {
        let source = std::fs::read_to_string(file).expect("source");
        let formatted = asm198x::format_pasmo(&source).expect("formats");
        if asm198x::format_pasmo(&formatted).expect("formats") != formatted {
            wrong.push(format!("{}: formatting again changed it", file.display()));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

fn collect(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .extension()
            .is_some_and(|ext| ext == "asm" || ext == "inc")
        {
            out.push(path);
        }
    }
}
