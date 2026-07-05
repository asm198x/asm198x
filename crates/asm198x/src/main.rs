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
    Acme,
    /// ca65 for the NES — assembled and linked to a `.nes` ROM, handled
    /// separately from the flat-binary dialects.
    Ca65,
    /// vasm Motorola-syntax 68000 — a flat big-endian code image (Stage 1),
    /// handled directly in `run` like ca65.
    Vasm,
    /// lwasm Motorola-syntax 6809 — a flat big-endian binary.
    Lwasm,
    /// ca65-syntax 65816 (native mode) — a flat little-endian binary.
    Ca65_816,
    /// ca65-syntax HuC6280 (PC Engine) — a flat little-endian binary.
    Ca65Huc6280,
    /// rgbasm-syntax SM83 (Game Boy) — a flat binary.
    Rgbasm,
    /// Intel-syntax 8080 — a flat binary.
    I8080,
    /// Motorola-syntax 6800 — a flat big-endian binary.
    M6800,
    /// asl-syntax RCA CDP1802 (COSMAC) — a flat big-endian binary.
    Cdp1802,
    /// asl-syntax Intel 8048 (MCS-48) — a flat binary. `romless` selects the
    /// 8035/8039/8040 kin, which forbid the four BUS-port instructions.
    I8048 {
        romless: bool,
    },
    /// asl-syntax National SC/MP (INS8060) — a flat binary.
    Scmp,
    /// asl-syntax Fairchild F8 (3850) — a flat big-endian binary.
    F8,
    /// asl-syntax Signetics 2650 — a flat big-endian binary.
    S2650,
    /// asl-syntax TI TMS7000 — a flat big-endian binary.
    Tms7000,
    /// asl-syntax DEC PDP-11 — a flat little-endian binary.
    Pdp11,
    /// asl-syntax TI TMS9900 — a flat big-endian binary.
    Tms9900,
    /// asl-syntax GI CP1610 (Intellivision) — a flat big-endian binary.
    Cp1610,
    /// asl-syntax Zilog Z8000 (non-segmented) — a flat big-endian binary.
    Z8000,
    /// asl-syntax Zilog Z8001 (segmented) — a flat big-endian binary.
    Z8001,
    Pasmo {
        z80n: bool,
    },
    Sjasmplus {
        z80n: bool,
    },
}

impl Assembler {
    fn resolve(dialect: Option<&str>, target: Option<&str>) -> Result<Self, String> {
        // The Z80 target, if one was given explicitly via --cpu/--target.
        let z80n = match target {
            Some(t) if t.eq_ignore_ascii_case("z80") => Some(false),
            Some(t) if t.eq_ignore_ascii_case("z80n") || t.eq_ignore_ascii_case("next") => {
                Some(true)
            }
            _ => None,
        };
        // A non-Z80 `--cpu` names a single-dialect chip directly (`8048`, `6800`,
        // `1802`, `8080`, `6502`, …): use it as the dialect when no explicit
        // `--dialect` was given. Z80 variants are handled via `z80n` above.
        let chip =
            target.filter(|t| !matches!(t.to_ascii_lowercase().as_str(), "z80" | "z80n" | "next"));
        let key = dialect
            .map(str::to_ascii_lowercase)
            .or_else(|| chip.map(str::to_ascii_lowercase));
        match key.as_deref() {
            // ACME is the default 6502 dialect (C64); ca65 targets the NES.
            Some("acme" | "6502" | "mos6502") => Ok(Self::Acme),
            Some("ca65" | "nes") => Ok(Self::Ca65),
            Some("vasm" | "68000" | "m68k" | "mot") => Ok(Self::Vasm),
            Some("lwasm" | "6809") => Ok(Self::Lwasm),
            Some("65816" | "816" | "ca65-816") => Ok(Self::Ca65_816),
            Some("huc6280" | "pce" | "pc-engine") => Ok(Self::Ca65Huc6280),
            Some("rgbasm" | "sm83" | "gb" | "gameboy" | "game-boy") => Ok(Self::Rgbasm),
            Some("8080" | "i8080" | "intel8080") => Ok(Self::I8080),
            Some("6800" | "m6800") => Ok(Self::M6800),
            Some("1802" | "cdp1802" | "cosmac") => Ok(Self::Cdp1802),
            // The ROM'd MCS-48 parts share the 8048's full set; the ROM-less kin
            // (8035/8039/8040, incl. CMOS) forbid the four BUS-port instructions.
            Some("8048" | "i8048" | "mcs48" | "mcs-48" | "8049" | "8050" | "80c48" | "80c49") => {
                Ok(Self::I8048 { romless: false })
            }
            Some("8035" | "8039" | "8040" | "80c35" | "80c39" | "80c40") => {
                Ok(Self::I8048 { romless: true })
            }
            Some("scmp" | "sc/mp" | "ins8060") => Ok(Self::Scmp),
            Some("f8" | "3850" | "f3850" | "channelf" | "channel-f") => Ok(Self::F8),
            Some("2650" | "s2650" | "signetics2650") => Ok(Self::S2650),
            Some("tms7000" | "7000" | "tms70c00") => Ok(Self::Tms7000),
            Some("pdp11" | "pdp-11" | "lsi11" | "lsi-11") => Ok(Self::Pdp11),
            Some("tms9900" | "9900" | "ti99") => Ok(Self::Tms9900),
            Some("cp1610" | "cp1600" | "cp-1600" | "intellivision" | "intv") => Ok(Self::Cp1610),
            Some("z8000" | "z8002") => Ok(Self::Z8000),
            Some("z8001") => Ok(Self::Z8001),
            // pasmo defaults to plain Z80; pasmonext defaults to Z80N. An
            // explicit --cpu/--target wins.
            Some("pasmo") => Ok(Self::Pasmo {
                z80n: z80n.unwrap_or(false),
            }),
            Some("pasmonext") => Ok(Self::Pasmo {
                z80n: z80n.unwrap_or(true),
            }),
            Some("sjasmplus" | "sjasm") => Ok(Self::Sjasmplus {
                z80n: z80n.unwrap_or(false),
            }),
            Some(other) => Err(format!(
                "unknown dialect `{other}` (try acme, ca65, pasmo, pasmonext, or sjasmplus)"
            )),
            // No --dialect: a Z80 target implies pasmo syntax; otherwise 6502/acme.
            None => match z80n {
                Some(z) => Ok(Self::Pasmo { z80n: z }),
                None => Ok(Self::Acme),
            },
        }
    }

    fn assemble(self, source: &str) -> Result<asm198x::Assembly, asm198x::AsmError> {
        match self {
            Self::Acme => asm198x::assemble_acme(source),
            Self::Lwasm => asm198x::assemble_lwasm(source),
            Self::Ca65_816 => asm198x::assemble_ca65_816(source),
            Self::Ca65Huc6280 => asm198x::assemble_ca65_huc6280(source),
            Self::Rgbasm => asm198x::assemble_rgbasm(source),
            Self::I8080 => asm198x::assemble_i8080(source),
            Self::M6800 => asm198x::assemble_m6800(source),
            Self::Cdp1802 => asm198x::assemble_1802(source),
            Self::I8048 { romless: false } => asm198x::assemble_8048(source),
            Self::I8048 { romless: true } => asm198x::assemble_8039(source),
            Self::Scmp => asm198x::assemble_scmp(source),
            Self::F8 => asm198x::assemble_f8(source),
            Self::S2650 => asm198x::assemble_2650(source),
            Self::Tms7000 => asm198x::assemble_tms7000(source),
            Self::Pdp11 => asm198x::assemble_pdp11(source),
            Self::Tms9900 => asm198x::assemble_tms9900(source),
            Self::Cp1610 => asm198x::assemble_cp1610(source),
            Self::Z8000 => asm198x::assemble_z8000(source),
            Self::Z8001 => asm198x::assemble_z8001(source),
            // ca65 and vasm produce non-flat output and are handled in `run`.
            Self::Ca65 | Self::Vasm => unreachable!("ca65/vasm handled in run()"),
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
    let mut format = false;
    let mut exe = false;
    let mut sna = false;
    let mut prg = false;
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
            "--fmt" | "--format" => format = true,
            "--exe" | "--hunkexe" => exe = true,
            "--sna" => sna = true,
            "--prg" => prg = true,
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
        let assembler = Assembler::resolve(dialect, target)?;
        let bytes = std::fs::read(input).map_err(|e| format!("cannot read {input}: {e}"))?;
        // A 6502 dialect disassembles to 6502 syntax; otherwise Z80.
        match assembler {
            Assembler::Acme | Assembler::Ca65 => {
                print!("{}", asm198x::listing_6502(&bytes, origin));
            }
            Assembler::Pasmo { z80n } | Assembler::Sjasmplus { z80n } => {
                print!("{}", asm198x::listing_z80(&bytes, origin, z80n));
            }
            Assembler::Vasm => {
                print!("{}", asm198x::listing_68000(&bytes, u32::from(origin)));
            }
            Assembler::Lwasm => {
                print!("{}", asm198x::listing_6809(&bytes, origin));
            }
            Assembler::Ca65_816 => {
                print!("{}", asm198x::listing_65816(&bytes, origin));
            }
            Assembler::Ca65Huc6280 => {
                print!("{}", asm198x::listing_huc6280(&bytes, origin));
            }
            Assembler::Rgbasm => {
                print!("{}", asm198x::listing_sm83(&bytes, origin));
            }
            Assembler::I8080 => {
                print!("{}", asm198x::listing_i8080(&bytes, origin));
            }
            Assembler::M6800 => {
                print!("{}", asm198x::listing_m6800(&bytes, origin));
            }
            Assembler::Cdp1802 => {
                print!("{}", asm198x::listing_1802(&bytes, origin));
            }
            Assembler::I8048 { .. } => {
                print!("{}", asm198x::listing_8048(&bytes, origin));
            }
            Assembler::Scmp => {
                print!("{}", asm198x::listing_scmp(&bytes, origin));
            }
            Assembler::F8 => {
                print!("{}", asm198x::listing_f8(&bytes, origin));
            }
            Assembler::S2650 => {
                print!("{}", asm198x::listing_2650(&bytes, origin));
            }
            Assembler::Tms7000 => {
                print!("{}", asm198x::listing_tms7000(&bytes, origin));
            }
            Assembler::Pdp11 => {
                print!("{}", asm198x::listing_pdp11(&bytes, origin));
            }
            Assembler::Tms9900 => {
                print!("{}", asm198x::listing_tms9900(&bytes, origin));
            }
            Assembler::Cp1610 => {
                print!("{}", asm198x::listing_cp1610(&bytes, origin));
            }
            Assembler::Z8000 => {
                print!("{}", asm198x::listing_z8000(&bytes, origin));
            }
            Assembler::Z8001 => {
                print!("{}", asm198x::listing_z8001(&bytes, origin));
            }
        }
        return Ok(format!(
            "disassembled {} byte(s) at ${origin:04X}",
            bytes.len()
        ));
    }

    let assembler = Assembler::resolve(dialect, target)?;
    let source = std::fs::read_to_string(input).map_err(|e| format!("cannot read {input}: {e}"))?;

    // `--fmt`: parse into the semantic AST and emit canonical same-dialect
    // source (the formatter, U5). Prints to stdout, or writes with `-o`.
    if format {
        let formatted = match assembler {
            Assembler::Pasmo { z80n: false } => asm198x::format_pasmo(&source),
            Assembler::Pasmo { z80n: true } => asm198x::format_pasmonext(&source),
            Assembler::Sjasmplus { z80n: false } => asm198x::format_sjasmplus(&source),
            Assembler::Sjasmplus { z80n: true } => asm198x::format_sjasmplus_next(&source),
            Assembler::I8080 => asm198x::format_i8080(&source),
            Assembler::M6800 => asm198x::format_m6800(&source),
            Assembler::Cdp1802 => asm198x::format_1802(&source),
            Assembler::Scmp => asm198x::format_scmp(&source),
            _ => {
                return Err(
                    "`--fmt` supports the Z80 dialects (pasmo, sjasmplus), 8080, 6800, 1802, and scmp so far"
                        .into(),
                );
            }
        }
        .map_err(|e| format!("{input}: {e}"))?;
        if let Some(path) = &output {
            std::fs::write(path, &formatted)
                .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            return Ok(format!("formatted {input} -> {}", path.display()));
        }
        print!("{formatted}");
        return Ok(format!("formatted {input}"));
    }

    // vasm (68000): a flat big-endian code image, or an Amiga hunk executable
    // with `--exe` (the curriculum's `-Fhunkexe` target).
    if let Assembler::Vasm = assembler {
        if exe {
            let image = asm198x::assemble_vasm_exe(&source).map_err(|e| e.to_string())?;
            // vasm's convention: the executable drops the source extension.
            let out_path = output.unwrap_or_else(|| Path::new(input).with_extension(""));
            std::fs::write(&out_path, &image)
                .map_err(|e| format!("cannot write {}: {e}", out_path.display()))?;
            return Ok(format!(
                "assembled {} byte(s) -> {}",
                image.len(),
                out_path.display()
            ));
        }
        let (code, warnings) = asm198x::assemble_vasm_warned(&source).map_err(|e| e.to_string())?;
        for w in &warnings {
            eprintln!("asm198x: {input}: {w}");
        }
        let out_path = output.unwrap_or_else(|| Path::new(input).with_extension("bin"));
        std::fs::write(&out_path, &code)
            .map_err(|e| format!("cannot write {}: {e}", out_path.display()))?;
        return Ok(format!(
            "assembled {} byte(s) -> {}",
            code.len(),
            out_path.display()
        ));
    }

    // ca65 assembles and links to a `.nes` ROM rather than a flat binary.
    if let Assembler::Ca65 = assembler {
        let rom = asm198x::assemble_ca65(&source).map_err(|e| e.to_string())?;
        let out_path = output.unwrap_or_else(|| Path::new(input).with_extension("nes"));
        std::fs::write(&out_path, &rom)
            .map_err(|e| format!("cannot write {}: {e}", out_path.display()))?;
        return Ok(format!(
            "assembled + linked {} byte(s) -> {}",
            rom.len(),
            out_path.display()
        ));
    }

    let assembly = assembler.assemble(&source).map_err(|e| e.to_string())?;
    for w in &assembly.warnings {
        eprintln!("asm198x: {input}: {w}");
    }

    // `--sna`: wrap the assembled Spectrum program in a 48K snapshot rather than
    // writing a flat binary. Only the Z80/Spectrum dialects carry an entry point.
    if sna {
        if !matches!(
            assembler,
            Assembler::Pasmo { .. } | Assembler::Sjasmplus { .. }
        ) {
            return Err(
                "`--sna` is only for the Spectrum Z80 dialects (pasmo/pasmonext/sjasmplus)".into(),
            );
        }
        let image = asm198x::sna_48k(&assembly).map_err(|e| e.to_string())?;
        let out_path = output.unwrap_or_else(|| Path::new(input).with_extension("sna"));
        std::fs::write(&out_path, &image)
            .map_err(|e| format!("cannot write {}: {e}", out_path.display()))?;
        return Ok(format!(
            "assembled {} byte(s) -> {} (48K snapshot)",
            image.len(),
            out_path.display(),
        ));
    }

    // `--prg`: wrap the assembled C64 program in a `.prg` (load-address prefix).
    if prg {
        if !matches!(assembler, Assembler::Acme) {
            return Err("`--prg` is only for the C64 dialect (acme)".into());
        }
        let image = asm198x::prg(&assembly);
        let out_path = output.unwrap_or_else(|| Path::new(input).with_extension("prg"));
        std::fs::write(&out_path, &image)
            .map_err(|e| format!("cannot write {}: {e}", out_path.display()))?;
        return Ok(format!(
            "assembled {} byte(s) -> {} (load ${:04X})",
            image.len(),
            out_path.display(),
            assembly.origin,
        ));
    }

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
     snapshot:    asm198x --dialect pasmonext --sna <input> [-o <out.sna>]\n\
     \x20            (Spectrum Z80 only; needs `end <addr>` for the entry point)\n\
     C64 program: asm198x --dialect acme --prg <input> [-o <out.prg>]\n\
     \x20            (prepends the 2-byte load address)\n\
     disassemble: asm198x --disasm [-d <dialect>] [--org <addr>] <input.bin>\n\
     \x20            (6502 for acme/ca65/6502; Z80 otherwise)\n\
     format:      asm198x --fmt [--cpu <pasmo|sjasmplus|8080|6800|1802|scmp>] <input.asm> [-o <out.asm>]\n\
     \x20            (canonical layout, comments + operand spelling preserved; Z80/8080/6800/1802/scmp)\n\n\
     dialects (syntax): acme (C64 6502; also `6502`), ca65 (NES), vasm (Amiga\n\
     \x20                 68000), lwasm (6809), 65816 (ca65 native), huc6280\n\
     \x20                 (PC Engine ca65; also `pce`), rgbasm (Game Boy SM83;\n\
     \x20                 also `sm83`/`gb`), 8080 (Intel syntax), 6800\n\
     \x20                 (Motorola syntax), 1802 (COSMAC), 8048 (MCS-48;\n\
     \x20                 ROM-less kin `8035`/`8039`/`8040`), scmp (SC/MP),\n\
     \x20                 f8 (Fairchild F8; also `3850`/`channelf`), 2650\n\
     \x20                 (Signetics 2650), tms7000 (TI TMS7000), pasmo,\n\
     \x20                 pasmonext, sjasmplus\n\
     targets (--cpu):   z80 (default for pasmo), z80n (Spectrum Next; default\n\
     \x20                 for pasmonext) — Z80N opcodes follow the target, not\n\
     \x20                 the dialect\n\n\
     Assembles retro CPU source to a flat binary, or disassembles one back."
        .to_string()
}
