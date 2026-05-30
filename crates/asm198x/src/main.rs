//! `asm198x` — the command-line assembler.
//!
//! Usage: `asm198x <input.s> [-o <output.bin>]`. Assembles 6502 source to a
//! flat binary. The assembler engine lives in the library crate of the same
//! name; this is a thin shell over [`asm198x::assemble_6502`].

use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("asm198x: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<String, String> {
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        return Ok(usage());
    }

    let mut input: Option<&str> = None;
    let mut output: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                i += 1;
                let path = args.get(i).ok_or("`-o` needs a path")?;
                output = Some(PathBuf::from(path));
            }
            flag if flag.starts_with('-') => return Err(format!("unknown flag `{flag}`")),
            path => {
                if input.is_some() {
                    return Err("only one input file is supported".into());
                }
                input = Some(path);
            }
        }
        i += 1;
    }

    let input = input.ok_or("no input file given (try --help)")?;
    let source = std::fs::read_to_string(input).map_err(|e| format!("cannot read {input}: {e}"))?;
    let assembly = asm198x::assemble_6502(&source).map_err(|e| e.to_string())?;
    let out_path = output.unwrap_or_else(|| Path::new(input).with_extension("bin"));
    std::fs::write(&out_path, &assembly.bytes)
        .map_err(|e| format!("cannot write {}: {e}", out_path.display()))?;

    Ok(format!(
        "assembled {} byte(s) at ${:04X} -> {}",
        assembly.bytes.len(),
        assembly.origin,
        out_path.display(),
    ))
}

fn usage() -> String {
    "asm198x — 198x family assembler (6502)\n\n\
     usage: asm198x <input.s> [-o <output.bin>]\n\n\
     Assembles 6502 source to a flat binary. More CPUs to follow."
        .to_string()
}
