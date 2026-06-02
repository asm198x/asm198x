//! `asm198x` — the command-line assembler.
//!
//! Usage: `asm198x [--dialect <name>] <input> [-o <output.bin>]`. Assembles
//! retro CPU source to a flat binary. The engine lives in the library crate of
//! the same name; this is a thin shell over its per-dialect entry points.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// A source dialect the CLI can assemble.
#[derive(Clone, Copy)]
enum Dialect {
    Mos6502,
    PasmoZ80,
}

impl Dialect {
    /// Resolve a `--dialect` value (or CPU alias) to a dialect.
    fn parse(name: &str) -> Result<Self, String> {
        match name.to_ascii_lowercase().as_str() {
            "6502" | "mos6502" | "ca65" => Ok(Self::Mos6502),
            "pasmo" | "z80" => Ok(Self::PasmoZ80),
            other => Err(format!("unknown dialect `{other}` (try 6502 or pasmo)")),
        }
    }

    fn assemble(self, source: &str) -> Result<asm198x::Assembly, asm198x::AsmError> {
        match self {
            Self::Mos6502 => asm198x::assemble_6502(source),
            Self::PasmoZ80 => asm198x::assemble_pasmo_z80(source),
        }
    }
}

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
    let mut dialect = Dialect::Mos6502;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                i += 1;
                let path = args.get(i).ok_or("`-o` needs a path")?;
                output = Some(PathBuf::from(path));
            }
            "-d" | "--dialect" | "--cpu" => {
                i += 1;
                let name = args.get(i).ok_or("`--dialect` needs a value")?;
                dialect = Dialect::parse(name)?;
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
    let assembly = dialect.assemble(&source).map_err(|e| e.to_string())?;
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
    "asm198x — 198x family assembler\n\n\
     usage: asm198x [--dialect <name>] <input> [-o <output.bin>]\n\n\
     dialects: 6502 (default, ca65/ACME-shaped), pasmo (Z80)\n\n\
     Assembles retro CPU source to a flat binary."
        .to_string()
}
