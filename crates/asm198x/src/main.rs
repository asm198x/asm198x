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

    /// The `(cpu, dialect)` identity for a `.debug198x` sidecar header —
    /// the target chip and the source syntax, per the format's `Header` docs.
    fn identity(self) -> (&'static str, &'static str) {
        match self {
            Self::Acme => ("6502", "acme"),
            Self::Ca65 => ("6502", "ca65"),
            Self::Vasm => ("68000", "vasm"),
            Self::Lwasm => ("6809", "lwasm"),
            Self::Ca65_816 => ("65816", "ca65"),
            Self::Ca65Huc6280 => ("huc6280", "ca65"),
            Self::Rgbasm => ("sm83", "rgbasm"),
            Self::I8080 => ("8080", "intel"),
            Self::M6800 => ("6800", "motorola"),
            Self::Cdp1802 => ("1802", "asl"),
            Self::I8048 { romless: false } => ("8048", "asl"),
            Self::I8048 { romless: true } => ("8039", "asl"),
            Self::Scmp => ("scmp", "asl"),
            Self::F8 => ("f8", "asl"),
            Self::S2650 => ("2650", "asl"),
            Self::Tms7000 => ("tms7000", "asl"),
            Self::Pdp11 => ("pdp11", "asl"),
            Self::Tms9900 => ("tms9900", "asl"),
            Self::Cp1610 => ("cp1610", "asl"),
            Self::Z8000 => ("z8000", "asl"),
            Self::Z8001 => ("z8001", "asl"),
            Self::Pasmo { z80n: false } => ("z80", "pasmo"),
            Self::Pasmo { z80n: true } => ("z80n", "pasmo"),
            Self::Sjasmplus { z80n: false } => ("z80", "sjasmplus"),
            Self::Sjasmplus { z80n: true } => ("z80n", "sjasmplus"),
        }
    }

    /// Bytes per address unit — 2 for the word-addressed CP1610 (a decle is two
    /// bytes; labels and spans count decles), 1 for every byte-addressed CPU.
    /// The listing's bytes column indexes raw bytes, so it needs the unit.
    fn addr_unit(self) -> u64 {
        match self {
            Self::Cp1610 => 2,
            _ => 1,
        }
    }

    /// The multi-file library entry for an include-capable dialect — every
    /// flat dialect assembles through its `assemble_*_files` entry (the U2–U4
    /// rollout: the z80 family, acme, the ca65-flat family, rgbasm, lwasm,
    /// and the twelve asl chips). Only the two non-flat paths — ca65's
    /// assemble+link and vasm's multipass — sit outside the table; both are
    /// handled by their own arms in `run`/`emit_json` before this is asked.
    fn multi_entry(self) -> Option<MultiEntry> {
        match self {
            Self::Acme => Some(asm198x::assemble_acme_files),
            Self::Lwasm => Some(asm198x::assemble_lwasm_files),
            Self::Ca65_816 => Some(asm198x::assemble_ca65_816_files),
            Self::Ca65Huc6280 => Some(asm198x::assemble_ca65_huc6280_files),
            Self::Rgbasm => Some(asm198x::assemble_rgbasm_files),
            Self::I8080 => Some(asm198x::assemble_i8080_files),
            Self::M6800 => Some(asm198x::assemble_m6800_files),
            Self::Cdp1802 => Some(asm198x::assemble_1802_files),
            Self::I8048 { romless: false } => Some(asm198x::assemble_8048_files),
            Self::I8048 { romless: true } => Some(asm198x::assemble_8039_files),
            Self::Scmp => Some(asm198x::assemble_scmp_files),
            Self::F8 => Some(asm198x::assemble_f8_files),
            Self::S2650 => Some(asm198x::assemble_2650_files),
            Self::Tms7000 => Some(asm198x::assemble_tms7000_files),
            Self::Pdp11 => Some(asm198x::assemble_pdp11_files),
            Self::Tms9900 => Some(asm198x::assemble_tms9900_files),
            Self::Cp1610 => Some(asm198x::assemble_cp1610_files),
            Self::Z8000 => Some(asm198x::assemble_z8000_files),
            Self::Z8001 => Some(asm198x::assemble_z8001_files),
            Self::Pasmo { z80n: false } => Some(asm198x::assemble_pasmo_files),
            Self::Pasmo { z80n: true } => Some(asm198x::assemble_pasmonext_files),
            Self::Sjasmplus { z80n: false } => Some(asm198x::assemble_sjasmplus_files),
            Self::Sjasmplus { z80n: true } => Some(asm198x::assemble_sjasmplus_next_files),
            // ca65 and vasm produce non-flat output and are handled in `run`.
            Self::Ca65 | Self::Vasm => None,
        }
    }
}

/// The shape every multi-file library entry shares (`assemble_*_files`):
/// root source, root path, and the loader seam (KTD8).
type MultiEntry = fn(
    &str,
    &str,
    &dyn asm198x::source::SourceLoader,
) -> Result<asm198x::AssemblyResult, asm198x::MultiFileError>;

/// A debug-artifact flag's value: `None` = flag absent, `Some(None)` = default
/// path (the input with the artifact's extension), `Some(Some(p))` = explicit.
type ArtifactPath = Option<Option<PathBuf>>;

/// One recorded include load: what `--listing` needs to splice an included
/// file into the multi-file listing — its canonical path (the file-table key),
/// its text, and the include point (requesting file + directive line).
struct IncludeLoad {
    canonical: String,
    contents: String,
    from: Option<String>,
    line: u32,
}

/// A [`SourceLoader`](asm198x::source::SourceLoader) wrapper that records
/// every **include** load (the line-carrying `load_text_at` entry the source
/// map's registration uses) while delegating all resolution to the wrapped
/// loader. The success path of the `assemble_*_files` entries returns only
/// the file table, so this is how the CLI keeps each included file's contents
/// and include point for `--listing` (language-surface U9). Un-lined
/// `load_text` probes (the ca65-flat resolution probing) and binary loads
/// pass through unrecorded — they are not include registrations.
struct RecordingLoader<'a> {
    inner: &'a dyn asm198x::source::SourceLoader,
    log: std::cell::RefCell<Vec<IncludeLoad>>,
}

impl<'a> RecordingLoader<'a> {
    fn new(inner: &'a dyn asm198x::source::SourceLoader) -> Self {
        Self {
            inner,
            log: std::cell::RefCell::new(Vec::new()),
        }
    }

    /// Drain the recorded include loads, in load order (first inclusion of a
    /// file first — matching the source map's first-inclusion-wins graph).
    fn take(&self) -> Vec<IncludeLoad> {
        self.log.take()
    }
}

impl asm198x::source::SourceLoader for RecordingLoader<'_> {
    fn load_text(
        &self,
        request: &str,
        from: Option<&str>,
    ) -> Result<(String, String), asm198x::source::LoadError> {
        self.inner.load_text(request, from)
    }

    fn load_text_at(
        &self,
        request: &str,
        from: Option<&str>,
        line: u32,
    ) -> Result<(String, String), asm198x::source::LoadError> {
        let (canonical, contents) = self.inner.load_text_at(request, from, line)?;
        self.log.borrow_mut().push(IncludeLoad {
            canonical: canonical.clone(),
            contents: contents.clone(),
            from: from.map(str::to_owned),
            line,
        });
        Ok((canonical, contents))
    }

    fn load_binary(
        &self,
        request: &str,
        from: Option<&str>,
    ) -> Result<Vec<u8>, asm198x::source::LoadError> {
        self.inner.load_binary(request, from)
    }

    fn resolve_text(&self, request: &str, from: Option<&str>) -> Option<String> {
        // Forward to the wrapped loader's cheap probe — the trait default
        // resolves by reading and discarding, which would defeat the source
        // map's read-once dedup on every CLI assemble.
        self.inner.resolve_text(request, from)
    }
}

/// Build the `--listing` source set from a successful assemble: the file table
/// (`FileId` order), the root's already-read text, and the recorded include
/// loads supplying each included file's contents and include point. A table
/// entry with no recorded load (unreachable from the include walk) degrades to
/// an empty, unspliced entry rather than failing the listing.
fn listing_sources(
    input: &str,
    source: &str,
    files: &[String],
    loads: &[IncludeLoad],
) -> Vec<asm198x::ListingFile> {
    if files.is_empty() {
        // A single-source result (no file table): the root is the whole set.
        return vec![asm198x::ListingFile {
            path: input.to_string(),
            contents: source.to_string(),
            included_from: None,
        }];
    }
    files
        .iter()
        .enumerate()
        .map(|(i, path)| {
            if i == 0 {
                return asm198x::ListingFile {
                    path: path.clone(),
                    contents: source.to_string(),
                    included_from: None,
                };
            }
            // The first recorded load is the first inclusion — the one the
            // source map's include graph records.
            match loads.iter().find(|l| l.canonical == *path) {
                Some(l) => {
                    let parent = l
                        .from
                        .as_deref()
                        .and_then(|f| files.iter().position(|p| p == f))
                        .unwrap_or(0);
                    asm198x::ListingFile {
                        path: path.clone(),
                        contents: l.contents.clone(),
                        included_from: Some((asm198x::FileId(parent as u32), l.line)),
                    }
                }
                None => asm198x::ListingFile {
                    path: path.clone(),
                    contents: String::new(),
                    included_from: None,
                },
            }
        })
        .collect()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(summary) => {
            // Diagnostics go to stderr so stdout carries only real output
            // (the disassembly listing); assembly writes its bytes to a file. An
            // empty summary means the command already emitted its output (the
            // `--message-format=json` path prints JSON to stdout itself).
            if !summary.is_empty() {
                eprintln!("{summary}");
            }
            ExitCode::SUCCESS
        }
        Err(message) => {
            // Likewise, an empty message means the failure was already reported
            // (JSON diagnostics on stdout) — just set the exit code.
            if !message.is_empty() {
                eprintln!("asm198x: {message}");
            }
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
    let mut message_format = MessageFormat::Human;
    // Debug198x artifacts (U3): `None` = flag absent; `Some(None)` = default
    // path (input with the artifact's extension); `Some(Some(p))` = explicit.
    let mut debug: ArtifactPath = None;
    let mut sym: ArtifactPath = None;
    let mut listing: ArtifactPath = None;
    // Repeatable `-I <dir>` include-search directories, in command-line order —
    // the order is the search order (language-surface U1/KTD8). The
    // include-capable entry points (U2) feed them to the filesystem loader.
    let mut include_dirs: Vec<PathBuf> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--debug" => debug = Some(None),
            f if f.starts_with("--debug=") => {
                debug = Some(Some(PathBuf::from(&f["--debug=".len()..])));
            }
            "--sym" => sym = Some(None),
            f if f.starts_with("--sym=") => {
                sym = Some(Some(PathBuf::from(&f["--sym=".len()..])));
            }
            "--listing" => listing = Some(None),
            f if f.starts_with("--listing=") => {
                listing = Some(Some(PathBuf::from(&f["--listing=".len()..])));
            }
            "--message-format" => {
                i += 1;
                let value = args.get(i).ok_or("`--message-format` needs a value")?;
                message_format = parse_message_format(value)?;
            }
            f if f.starts_with("--message-format=") => {
                message_format = parse_message_format(&f["--message-format=".len()..])?;
            }
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
            "-I" => {
                i += 1;
                let dir = args.get(i).ok_or("`-I` needs a directory")?;
                include_dirs.push(PathBuf::from(dir));
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
    // The FileId→path table for human error rendering: single-file today, so
    // the root input is the whole table. U2's include-capable paths return the
    // real multi-file table (with `include_dirs` wired into the loader).
    let files = [input.to_string()];

    // The debug artifacts render an *assembly's* captured record; there is no
    // record to render under `--fmt` or `--disasm`, so the combination is an
    // error rather than a silent no-op.
    if (debug.is_some() || sym.is_some() || listing.is_some()) && (format || disassemble) {
        return Err(
            "`--debug`/`--sym`/`--listing` apply to an assembly run, not `--fmt`/`--disasm`".into(),
        );
    }

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

    // Debug198x artifacts: every path emits them (flat U3, ca65 U4, vasm U5).
    // The ca65/vasm listings wait on a per-section byte map, so only the
    // record-backed artifacts (`--debug`, `--sym`) are live there.
    if listing.is_some() && matches!(assembler, Assembler::Ca65 | Assembler::Vasm) {
        return Err(
            "`--listing` is not yet supported for the ca65/vasm paths (`--debug` and `--sym` are)"
                .into(),
        );
    }

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
            Assembler::I8048 { romless: false } => asm198x::format_8048(&source),
            Assembler::I8048 { romless: true } => asm198x::format_8039(&source),
            Assembler::F8 => asm198x::format_f8(&source),
            Assembler::S2650 => asm198x::format_2650(&source),
            Assembler::Tms7000 => asm198x::format_tms7000(&source),
            Assembler::Ca65_816 => asm198x::format_ca65_816(&source),
            Assembler::Ca65Huc6280 => asm198x::format_ca65_huc6280(&source),
            Assembler::Pdp11 => asm198x::format_pdp11(&source),
            Assembler::Tms9900 => asm198x::format_tms9900(&source),
            Assembler::Cp1610 => asm198x::format_cp1610(&source),
            Assembler::Z8000 => asm198x::format_z8000(&source),
            Assembler::Z8001 => asm198x::format_z8001(&source),
            Assembler::Scmp => asm198x::format_scmp(&source),
            Assembler::Rgbasm => asm198x::format_rgbasm(&source),
            Assembler::Lwasm => asm198x::format_lwasm(&source),
            Assembler::Acme => asm198x::format_acme(&source),
            Assembler::Vasm => asm198x::format_vasm(&source),
            Assembler::Ca65 => asm198x::format_ca65(&source),
            // Every dialect now routes through the semantic AST, so `--fmt`
            // covers them all — no unsupported-dialect fallback remains.
        }
        .map_err(|e| render_error(input, &files, &e))?;
        if let Some(path) = &output {
            std::fs::write(path, &formatted)
                .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            return Ok(format!("formatted {input} -> {}", path.display()));
        }
        print!("{formatted}");
        return Ok(format!("formatted {input}"));
    }

    // `--message-format=json`: emit the machine-consumable result (or its
    // diagnostics) as JSON on stdout, for any dialect, instead of the human
    // summary (U4). Byte output to `-o` still happens; only the reporting format
    // changes. Handled before the per-dialect human output paths below.
    if let MessageFormat::Json = message_format {
        return emit_json(
            &assembler,
            input,
            &source,
            &include_dirs,
            exe,
            output.as_deref(),
            (&debug, &sym, &listing),
        );
    }

    // vasm (68000): a flat big-endian code image, or an Amiga hunk executable
    // with `--exe` (the curriculum's `-Fhunkexe` target) — via the
    // include-capable entries (language-surface U6), so `include`/`incbin`
    // resolve against the input's directory + the `-I` dirs and a failure
    // renders with the real file table and the include-graph notes. With a
    // debug artifact requested, the debug-capturing entries return the
    // section-relative record with per-file line spans — same bytes by
    // construction.
    if let Assembler::Vasm = assembler {
        let loader = fs_loader(input, &include_dirs);
        let debug_requested = debug.is_some() || sym.is_some();
        let (result, info) = if exe && debug_requested {
            let (image, info) = asm198x::assemble_vasm_exe_files_debug(&source, input, &loader)
                .map_err(|e| render_multi_error(input, &e))?;
            (image, Some(info))
        } else if exe {
            (
                asm198x::assemble_vasm_exe_files(&source, input, &loader)
                    .map_err(|e| render_multi_error(input, &e))?,
                None,
            )
        } else if debug_requested {
            let (result, info) = asm198x::assemble_vasm_warned_files_debug(&source, input, &loader)
                .map_err(|e| render_multi_error(input, &e))?;
            (result, Some(info))
        } else {
            (
                asm198x::assemble_vasm_warned_files(&source, input, &loader)
                    .map_err(|e| render_multi_error(input, &e))?,
                None,
            )
        };
        for w in &result.warnings {
            // A warning inside an included file names that file (the
            // multi-file table).
            let file = result
                .files
                .get(w.file.0 as usize)
                .map_or(input, String::as_str);
            eprintln!("asm198x: {file}: {w}");
        }
        // vasm's convention: the executable drops the source extension.
        let out_path =
            output.unwrap_or_else(|| Path::new(input).with_extension(if exe { "" } else { "bin" }));
        std::fs::write(&out_path, &result.bytes)
            .map_err(|e| format!("cannot write {}: {e}", out_path.display()))?;
        let debug_notes = match &info {
            Some(info) => write_debug_artifacts(
                input,
                Some(&out_path),
                1,
                &result,
                info,
                &[],
                &debug,
                &sym,
                &listing,
            )?,
            None => String::new(),
        };
        return Ok(format!(
            "assembled {} byte(s) -> {}{debug_notes}",
            result.bytes.len(),
            out_path.display()
        ));
    }

    // ca65 assembles and links to a `.nes` ROM rather than a flat binary — via
    // the include-capable entries (language-surface U5), so `.include`/`.incbin`
    // resolve against the input's directory + the `-I` dirs and a failure
    // renders with the real file table and the include-graph notes. With a
    // debug artifact requested, the debug-capturing entry returns the record
    // read out of layout (U4), its line records naming each statement's file —
    // same bytes by construction.
    if let Assembler::Ca65 = assembler {
        let loader = fs_loader(input, &include_dirs);
        let (rom, info) = if debug.is_some() || sym.is_some() {
            let (rom, info) = asm198x::assemble_ca65_files_debug(&source, input, &loader)
                .map_err(|e| render_multi_error(input, &e))?;
            (rom, Some(info))
        } else {
            (
                asm198x::assemble_ca65_files(&source, input, &loader)
                    .map_err(|e| render_multi_error(input, &e))?,
                None,
            )
        };
        let out_path = output.unwrap_or_else(|| Path::new(input).with_extension("nes"));
        std::fs::write(&out_path, &rom.bytes)
            .map_err(|e| format!("cannot write {}: {e}", out_path.display()))?;
        let debug_notes = match &info {
            Some(info) => write_debug_artifacts(
                input,
                Some(&out_path),
                1,
                &rom,
                info,
                &[],
                &debug,
                &sym,
                &listing,
            )?,
            None => String::new(),
        };
        return Ok(format!(
            "assembled + linked {} byte(s) -> {}{debug_notes}",
            rom.bytes.len(),
            out_path.display()
        ));
    }

    // Container flags pair with specific dialects — validate before anything is
    // written, so a doomed invocation leaves no files behind.
    if sna
        && !matches!(
            assembler,
            Assembler::Pasmo { .. } | Assembler::Sjasmplus { .. }
        )
    {
        return Err(
            "`--sna` is only for the Spectrum Z80 dialects (pasmo/pasmonext/sjasmplus)".into(),
        );
    }
    if prg && !matches!(assembler, Assembler::Acme) {
        return Err("`--prg` is only for the C64 dialect (acme)".into());
    }

    // Every flat dialect assembles through its multi-file entry (U2–U4 — the
    // z80 family, acme, the ca65-flat family, rgbasm, lwasm, and the twelve
    // asl chips), with the input's directory + the `-I` dirs wired into a
    // filesystem loader; a failure renders with the real file table and the
    // include-graph notes. Only the non-flat ca65/vasm paths (handled above)
    // sit outside the table.
    let Some(entry) = assembler.multi_entry() else {
        unreachable!("ca65/vasm handled above")
    };
    let loader = fs_loader(input, &include_dirs);
    // The recorder keeps each include's contents + include point so a
    // `--listing` on a multi-file program can splice the files (U9); the
    // resolution behaviour is the wrapped filesystem loader's, unchanged.
    let recorder = RecordingLoader::new(&loader);
    let assembly = entry(&source, input, &recorder).map_err(|e| render_multi_error(input, &e))?;
    for w in &assembly.warnings {
        // A warning inside an included file names that file (the multi-file
        // table); single-file results have an empty table and keep naming the
        // input.
        let file = assembly
            .files
            .get(w.file.0 as usize)
            .map_or(input, String::as_str);
        eprintln!("asm198x: {file}: {w}");
    }

    // `--sna`: wrap the assembled Spectrum program in a 48K snapshot; `--prg`:
    // prefix the C64 load address; else a flat binary.
    let (summary, image_path) = if sna {
        // Only the Z80/Spectrum dialects carry an entry point; a missing
        // `end <addr>` fails here, before any file is written.
        let image = asm198x::sna_48k(&assembly).map_err(|e| render_error(input, &files, &e))?;
        let out_path = output.unwrap_or_else(|| Path::new(input).with_extension("sna"));
        std::fs::write(&out_path, &image)
            .map_err(|e| format!("cannot write {}: {e}", out_path.display()))?;
        let summary = format!(
            "assembled {} byte(s) -> {} (48K snapshot)",
            image.len(),
            out_path.display(),
        );
        (summary, out_path)
    } else if prg {
        let image = asm198x::prg(&assembly);
        let out_path = output.unwrap_or_else(|| Path::new(input).with_extension("prg"));
        std::fs::write(&out_path, &image)
            .map_err(|e| format!("cannot write {}: {e}", out_path.display()))?;
        let summary = format!(
            "assembled {} byte(s) -> {} (load ${:04X})",
            image.len(),
            out_path.display(),
            assembly.origin.unwrap_or(0),
        );
        (summary, out_path)
    } else {
        let out_path = output.unwrap_or_else(|| Path::new(input).with_extension("bin"));
        std::fs::write(&out_path, &assembly.bytes)
            .map_err(|e| format!("cannot write {}: {e}", out_path.display()))?;
        let summary = format!(
            "assembled {} byte(s) at ${:04X} -> {}",
            assembly.bytes.len(),
            assembly.origin.unwrap_or(0),
            out_path.display(),
        );
        (summary, out_path)
    };

    // Debug artifacts (U3) are written only after the image write succeeded, so
    // a failed run never leaves a sidecar describing an image that was not
    // produced. `--debug` alongside `--sna`/`--prg` emits both artifacts.
    let debug_notes = if debug.is_some() || sym.is_some() || listing.is_some() {
        let (cpu, dialect) = assembler.identity();
        // `debug_info` reads the result's own file table (KTD2), so the
        // sidecar's `sources` and per-file line records are multi-file-true.
        let info = asm198x::debug_info(&assembly, cpu, dialect, input);
        let sources = listing_sources(input, &source, &assembly.files, &recorder.take());
        write_debug_artifacts(
            input,
            Some(&image_path),
            assembler.addr_unit(),
            &assembly,
            &info,
            &sources,
            &debug,
            &sym,
            &listing,
        )?
    } else {
        String::new()
    };
    Ok(format!("{summary}{debug_notes}"))
}

/// Write the requested Debug198x artifacts — the `.debug198x` NDJSON sidecar
/// (`--debug`), the symbol table (`--sym`), and the listing (`--listing`) —
/// and return `wrote …` summary lines (empty when no flag was passed). All
/// three render the one captured record (plan KTD2), passed in as the prebuilt
/// `info` (the flat engine's via [`asm198x::debug_info`], ca65's read out of
/// layout); default paths are the input with the artifact's extension.
/// `sources` is the listing's spliced source set ([`listing_sources`]) — the
/// linked ca65/vasm paths, where `--listing` is rejected upstream, pass an
/// empty slice.
#[allow(clippy::too_many_arguments)]
fn write_debug_artifacts(
    input: &str,
    image: Option<&Path>,
    addr_unit: u64,
    assembly: &asm198x::AssemblyResult,
    info: &asm198x::debug198x::DebugInfo,
    sources: &[asm198x::ListingFile],
    debug: &ArtifactPath,
    sym: &ArtifactPath,
    listing: &ArtifactPath,
) -> Result<String, String> {
    let mut notes = String::new();
    let mut emit = |path: &Option<PathBuf>, ext: &str, what: &str, content: String| {
        let path = path
            .clone()
            .unwrap_or_else(|| Path::new(input).with_extension(ext));
        // An input already named `*.{ext}` would make the default path the
        // input itself — refuse rather than overwrite the source. The image
        // output gets the same protection: an artifact landing on the just-
        // written binary would silently clobber it.
        if path == Path::new(input) {
            return Err(format!(
                "refusing to overwrite the input with the {what} — pass an explicit `=<path>`"
            ));
        }
        if image.is_some_and(|image| path == image) {
            return Err(format!(
                "refusing to overwrite the output image with the {what} — pass a different `=<path>`"
            ));
        }
        std::fs::write(&path, content)
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        notes.push_str(&format!("\nwrote {} ({what})", path.display()));
        Ok::<(), String>(())
    };
    if let Some(path) = debug {
        emit(path, "debug198x", "debug sidecar", info.to_ndjson())?;
    }
    if let Some(path) = sym {
        emit(path, "sym", "symbol table", asm198x::render_sym(info))?;
    }
    if let Some(path) = listing {
        let text = asm198x::render_listing_files(sources, assembly, addr_unit);
        emit(path, "lst", "listing", text)?;
    }
    Ok(notes)
}

/// Render an assembly failure for humans. An error carrying a span whose file
/// resolves in `files` (the FileId→path table, index ⇔ `FileId`) renders
/// rustc-style — `file:line:col: error: message`, or `file:line: error:
/// message` when the span is line-granular (col 0) — so a failure inside an
/// included file names *that* file, not the root input. An error with no span
/// (or an unresolvable file id) falls back to the pre-multi-file
/// `input: <error>` shape.
fn render_error(input: &str, files: &[String], e: &asm198x::AsmError) -> String {
    let resolved = e
        .span
        .as_ref()
        .and_then(|span| files.get(span.file.0 as usize).map(|file| (span, file)));
    match resolved {
        Some((span, file)) if span.col != 0 => {
            format!("{file}:{}:{}: error: {}", span.line, span.col, e.message)
        }
        Some((span, file)) => format!("{file}:{}: error: {}", span.line, e.message),
        None => format!("{input}: {e}"),
    }
}

/// One rustc-style `included from <file>:<line>` note per include-graph hop,
/// innermost first, each on its own line — appended to a rendered error so a
/// failure deep in an include chain shows how it was reached. Empty for the
/// root input (included from nowhere).
fn render_include_notes(map: &asm198x::source::SourceMap, file: asm198x::FileId) -> String {
    map.include_chain(file)
        .iter()
        .map(|(path, line)| format!("\nincluded from {path}:{line}"))
        .collect()
}

/// Render a multi-file assembly failure for humans: the rustc-style
/// `file:line:col` render against the failure-path file table (KTD2), plus
/// the `included from` chain when the error sits inside an include.
fn render_multi_error(input: &str, e: &asm198x::MultiFileError) -> String {
    let table = e.source_map.file_table();
    let mut message = render_error(input, &table, &e.error);
    if let Some(span) = &e.error.span {
        message.push_str(&render_include_notes(&e.source_map, span.file));
    }
    message
}

/// The filesystem loader for an include-capable run: anchored at the input's
/// directory, searching the repeatable `-I` dirs in command-line order (U2,
/// KTD8).
fn fs_loader(input: &str, include_dirs: &[PathBuf]) -> asm198x::source::FsLoader {
    let base = Path::new(input)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    asm198x::source::FsLoader::new(base, include_dirs.to_vec())
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
     assemble:    asm198x [--dialect <name>] [--cpu <target>] [-I <dir>]... <input> [-o <out.bin>]\n\
     \x20            (add --message-format=json for a machine-readable result +\n\
     \x20             diagnostics on stdout; --message-format=human is the default;\n\
     \x20             -I adds an include-search directory, repeatable, in order)\n\
     snapshot:    asm198x --dialect pasmonext --sna <input> [-o <out.sna>]\n\
     \x20            (Spectrum Z80 only; needs `end <addr>` for the entry point)\n\
     C64 program: asm198x --dialect acme --prg <input> [-o <out.prg>]\n\
     \x20            (prepends the 2-byte load address)\n\
     debug info:  asm198x [--debug[=path]] [--sym[=path]] [--listing[=path]] <input>\n\
     \x20            (--debug writes the .debug198x NDJSON sidecar; --sym a sorted\n\
     \x20             `name = $hex` table; --listing address/bytes/source rows —\n\
     \x20             defaults: input with .debug198x/.sym/.lst; flat dialects only\n\
     \x20             for now plus the ca65/vasm linked paths for --debug/--sym)\n\
     disassemble: asm198x --disasm [-d <dialect>] [--org <addr>] <input.bin>\n\
     \x20            (6502 for acme/ca65/6502; Z80 otherwise)\n\
     format:      asm198x --fmt [--cpu <pasmo|sjasmplus|8080|6800|1802|scmp|rgbasm|6809>] <input.asm> [-o <out.asm>]\n\
     \x20            (canonical layout, comments + operand spelling preserved; Z80/8080/6800/1802/scmp/rgbasm/6809)\n\n\
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

/// The `--message-format` mode: human summary (default) or machine-consumable
/// JSON (U4).
#[derive(Clone, Copy)]
enum MessageFormat {
    Human,
    Json,
}

fn parse_message_format(value: &str) -> Result<MessageFormat, String> {
    match value {
        "human" => Ok(MessageFormat::Human),
        "json" => Ok(MessageFormat::Json),
        other => Err(format!(
            "invalid --message-format `{other}` (expected `human` or `json`)"
        )),
    }
}

/// Unwrap a multi-file failure: keep its file table for span-path resolution
/// (KTD2's failure-path leg) and pass the inner error on. Every include-capable
/// route funnels its `map_err` through here so the capture cannot drift
/// per-arm.
fn capture_failure(
    failure_files: &mut Vec<String>,
    e: asm198x::MultiFileError,
) -> asm198x::AsmError {
    *failure_files = e.source_map.file_table();
    e.error
}

/// Emit the assembly result (or its diagnostics) as JSON on stdout — the
/// `--message-format=json` path (U4, R3). Byte output to `-o` still happens; only
/// the reporting format changes. The shape is CPU-agnostic (R8): every dialect's
/// `AssemblyResult` and every `AsmError`-derived `Diagnostic` serialize the same,
/// so a new CPU inherits JSON output with no extra work. Returns an empty summary
/// so the caller prints nothing further — the JSON is already on stdout.
#[allow(clippy::too_many_arguments)]
fn emit_json(
    assembler: &Assembler,
    input: &str,
    source: &str,
    include_dirs: &[PathBuf],
    exe: bool,
    output: Option<&Path>,
    (debug, sym, listing): (&ArtifactPath, &ArtifactPath, &ArtifactPath),
) -> Result<String, String> {
    let debug_requested = debug.is_some() || sym.is_some() || listing.is_some();
    // The ca65/vasm debug-capturing entries return the record alongside the
    // image; the flat paths build theirs from the result below.
    let mut linked_info: Option<asm198x::debug198x::DebugInfo> = None;
    let mut capture = |(image, info): (asm198x::AssemblyResult, asm198x::debug198x::DebugInfo)| {
        linked_info = Some(info);
        image
    };
    // The include-capable failure carries the file table (KTD2); it resolves
    // each failure diagnostic's `span.path`, so the bare Diagnostic-array
    // shape needs no change.
    let mut failure_files: Vec<String> = Vec::new();
    // One filesystem loader for every route, wrapped in the include recorder
    // so a flat `--listing` can splice included files (U9); the linked paths
    // reject `--listing` upstream and ignore the recording.
    let fs = fs_loader(input, include_dirs);
    let loader = RecordingLoader::new(&fs);
    let result = match assembler {
        // vasm goes through its include-capable entries (U6); a failure
        // carries the file table so the diagnostic's span can name an
        // included file.
        Assembler::Vasm => match (exe, debug_requested) {
            (true, true) => asm198x::assemble_vasm_exe_files_debug(source, input, &loader)
                .map_err(|e| capture_failure(&mut failure_files, e))
                .map(&mut capture),
            (true, false) => asm198x::assemble_vasm_exe_files(source, input, &loader)
                .map_err(|e| capture_failure(&mut failure_files, e)),
            (false, true) => asm198x::assemble_vasm_warned_files_debug(source, input, &loader)
                .map_err(|e| capture_failure(&mut failure_files, e))
                .map(&mut capture),
            (false, false) => asm198x::assemble_vasm_warned_files(source, input, &loader)
                .map_err(|e| capture_failure(&mut failure_files, e)),
        },
        // ca65 goes through its include-capable entries too (U5); a failure
        // carries the file table so the diagnostic's span can name an
        // included file.
        Assembler::Ca65 if debug_requested => {
            asm198x::assemble_ca65_files_debug(source, input, &loader)
                .map_err(|e| capture_failure(&mut failure_files, e))
                .map(&mut capture)
        }
        Assembler::Ca65 => asm198x::assemble_ca65_files(source, input, &loader)
            .map_err(|e| capture_failure(&mut failure_files, e)),
        // Every flat dialect goes through its multi-file entry (the same
        // table as the human path); ca65/vasm are the arms above.
        other => {
            let Some(entry) = other.multi_entry() else {
                unreachable!("ca65/vasm handled above")
            };
            entry(source, input, &loader).map_err(|e| capture_failure(&mut failure_files, e))
        }
    };
    match result {
        Ok(assembly) => {
            if let Some(path) = output {
                std::fs::write(path, &assembly.bytes)
                    .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            }
            // Debug artifacts are written in JSON mode too; the notes are
            // dropped — stdout carries only the JSON result.
            if debug_requested {
                let info = linked_info.unwrap_or_else(|| {
                    let (cpu, dialect) = assembler.identity();
                    asm198x::debug_info(&assembly, cpu, dialect, input)
                });
                let sources = listing_sources(input, source, &assembly.files, &loader.take());
                write_debug_artifacts(
                    input,
                    output,
                    assembler.addr_unit(),
                    &assembly,
                    &info,
                    &sources,
                    debug,
                    sym,
                    listing,
                )?;
            }
            let json =
                serde_json::to_string(&assembly).map_err(|e| format!("json encode failed: {e}"))?;
            println!("{json}");
            Ok(String::new())
        }
        Err(error) => {
            // A single diagnostic today (one fatal error); a Vec so the JSON shape
            // is stable if multi-error accumulation lands later. On the
            // include-capable path, the span's additive `path` field resolves
            // its file from the failure-path table (KTD2) — the array shape
            // itself is unchanged.
            let mut diagnostic = asm198x::Diagnostic::from(error);
            if !failure_files.is_empty() {
                diagnostic.span = diagnostic
                    .span
                    .map(|span| asm198x::resolve_span_path(span, &failure_files));
            }
            let diagnostics = [diagnostic];
            let json = serde_json::to_string(&diagnostics)
                .map_err(|e| format!("json encode failed: {e}"))?;
            println!("{json}");
            Err(String::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asm198x::source::{MemoryLoader, SourceMap};
    use asm198x::{AsmError, FileId, Span};

    /// A span in a non-root file renders that file's name from the table —
    /// `that-file.inc:12:8: error: …` — never the root input's.
    #[test]
    fn render_error_names_the_included_file_not_the_input() {
        let files = vec!["main.s".to_string(), "that-file.inc".to_string()];
        let e = AsmError {
            line: 12,
            message: "value 300 does not fit in a byte".to_string(),
            span: Some(Span::in_file(FileId(1), 12, 8)),
        };
        assert_eq!(
            render_error("main.s", &files, &e),
            "that-file.inc:12:8: error: value 300 does not fit in a byte"
        );
    }

    /// A line-granular span (col 0) drops the column component rather than
    /// printing a meaningless `:0`.
    #[test]
    fn render_error_omits_a_zero_column() {
        let files = vec!["main.s".to_string()];
        let e = AsmError {
            line: 4,
            message: "boom".to_string(),
            span: Some(Span::in_file(FileId(0), 4, 0)),
        };
        assert_eq!(render_error("main.s", &files, &e), "main.s:4: error: boom");
    }

    /// No span (or a file id outside the table) falls back to the
    /// pre-multi-file `input: <error>` shape.
    #[test]
    fn render_error_falls_back_without_a_resolvable_span() {
        let files = vec!["main.s".to_string()];
        let spanless = AsmError {
            line: 2,
            message: "no entry point".to_string(),
            span: None,
        };
        assert_eq!(
            render_error("main.s", &files, &spanless),
            "main.s: line 2: no entry point"
        );

        let out_of_table = AsmError {
            line: 3,
            message: "boom".to_string(),
            span: Some(Span::in_file(FileId(7), 3, 1)),
        };
        assert_eq!(
            render_error("main.s", &files, &out_of_table),
            "main.s: line 3: boom"
        );
    }

    /// A synthetic include graph renders one `included from <file>:<line>`
    /// note per hop, innermost first; the root renders none.
    #[test]
    fn include_notes_render_one_note_per_hop() {
        let loader = MemoryLoader::new()
            .text("a.inc", "        include \"b.inc\"\n")
            .text("b.inc", "        nop\n");
        let mut map = SourceMap::new("main.s", "        include \"a.inc\"\n");
        let a = map.load(&loader, "a.inc", FileId(0), 3).expect("a loads");
        let b = map.load(&loader, "b.inc", a, 5).expect("b loads");

        assert_eq!(
            render_include_notes(&map, b),
            "\nincluded from a.inc:5\nincluded from main.s:3"
        );
        assert_eq!(render_include_notes(&map, FileId(0)), "");
    }
}
