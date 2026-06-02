//! `asm198x` — the command-line assembler.
//!
//! Usage: `asm198x [--dialect <name>] <input> [-o <output.bin>]`. Assembles
//! retro CPU source to a flat binary. The engine lives in the library crate of
//! the same name; this is a thin shell over its per-dialect entry points.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// A resolved assembler: a syntax dialect plus, for Z80, a target instruction
/// set. Dialect (`--dialect`, syntax) and target (`--cpu`/`--target`, the chip)
/// are orthogonal; Z80N availability is a target property, not a syntax one.
#[derive(Clone, Copy)]
enum Assembler {
    Mos6502,
    Pasmo { z80n: bool },
    Sjasmplus { z80n: bool },
}

impl Assembler {
    fn resolve(dialect: Option<&str>, target: Option<&str>) -> Result<Self, String> {
        // The Z80 target, if one was given explicitly via --cpu/--target.
        let z80n = match target {
            None => None,
            Some(t) => match t.to_ascii_lowercase().as_str() {
                "z80" => Some(false),
                "z80n" | "next" => Some(true),
                "6502" => return Ok(Self::Mos6502),
                other => return Err(format!("unknown target `{other}` (try z80 or z80n)")),
            },
        };
        match dialect.map(str::to_ascii_lowercase).as_deref() {
            Some("6502" | "mos6502" | "ca65") => Ok(Self::Mos6502),
            // pasmo defaults to plain Z80; pasmonext defaults to Z80N. An
            // explicit --cpu/--target wins.
            Some("pasmo") => Ok(Self::Pasmo { z80n: z80n.unwrap_or(false) }),
            Some("pasmonext") => Ok(Self::Pasmo { z80n: z80n.unwrap_or(true) }),
            Some("sjasmplus" | "sjasm") => Ok(Self::Sjasmplus { z80n: z80n.unwrap_or(false) }),
            Some(other) => {
                Err(format!("unknown dialect `{other}` (try 6502, pasmo, pasmonext, or sjasmplus)"))
            }
            // No --dialect: a Z80 target implies pasmo syntax; otherwise 6502.
            None => match z80n {
                Some(z) => Ok(Self::Pasmo { z80n: z }),
                None => Ok(Self::Mos6502),
            },
        }
    }

    fn assemble(self, source: &str) -> Result<asm198x::Assembly, asm198x::AsmError> {
        match self {
            Self::Mos6502 => asm198x::assemble_6502(source),
            Self::Pasmo { z80n: false } => asm198x::assemble_pasmo(source),
            Self::Pasmo { z80n: true } => asm198x::assemble_pasmonext(source),
            Self::Sjasmplus { z80n: false } => asm198x::assemble_sjasmplus(source),
            Self::Sjasmplus { z80n: true } => asm198x::assemble_sjasmplus_next(source),
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(summary) => {
            // Diagnostics go to stderr so stdout carries only real output
            // (the disassembly listing); assembly writes its bytes to a file.
            eprintln!("{summary}");
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
    let mut dialect: Option<&str> = None;
    let mut target: Option<&str> = None;
    let mut disassemble = false;
    let mut origin: u16 = 0;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                i += 1;
                let path = args.get(i).ok_or("`-o` needs a path")?;
                output = Some(PathBuf::from(path));
            }
            "-d" | "--dialect" => {
                i += 1;
                dialect = Some(args.get(i).ok_or("`--dialect` needs a value")?);
            }
            "--cpu" | "--target" => {
                i += 1;
                target = Some(args.get(i).ok_or("`--target` needs a value")?);
            }
            "--disasm" | "--disassemble" => disassemble = true,
            "--org" => {
                i += 1;
                let value = args.get(i).ok_or("`--org` needs an address")?;
                origin = parse_u16(value)?;
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

    if disassemble {
        let z80n = matches!(
            Assembler::resolve(dialect, target)?,
            Assembler::Pasmo { z80n: true } | Assembler::Sjasmplus { z80n: true }
        );
        let bytes = std::fs::read(input).map_err(|e| format!("cannot read {input}: {e}"))?;
        print!("{}", asm198x::listing_z80(&bytes, origin, z80n));
        return Ok(format!("disassembled {} byte(s) at ${origin:04X}", bytes.len()));
    }

    let assembler = Assembler::resolve(dialect, target)?;
    let source = std::fs::read_to_string(input).map_err(|e| format!("cannot read {input}: {e}"))?;
    let assembly = assembler.assemble(&source).map_err(|e| e.to_string())?;
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

/// Parse an address: `$hhhh`, `0xhhhh`, or decimal.
fn parse_u16(value: &str) -> Result<u16, String> {
    let parsed = if let Some(hex) = value.strip_prefix('$').or_else(|| value.strip_prefix("0x")) {
        u16::from_str_radix(hex, 16)
    } else {
        value.parse::<u16>()
    };
    parsed.map_err(|_| format!("invalid address `{value}`"))
}

fn usage() -> String {
    "asm198x — 198x family assembler\n\n\
     assemble:    asm198x [--dialect <name>] [--cpu <target>] <input> [-o <out.bin>]\n\
     disassemble: asm198x --disasm [--org <addr>] <input.bin>   (Z80)\n\n\
     dialects (syntax): 6502 (default, ca65/ACME-shaped), pasmo, pasmonext, sjasmplus\n\
     targets (--cpu):   z80 (default for pasmo), z80n (Spectrum Next; default\n\
     \x20                 for pasmonext) — Z80N opcodes follow the target, not\n\
     \x20                 the dialect\n\n\
     Assembles retro CPU source to a flat binary, or disassembles one back."
        .to_string()
}
