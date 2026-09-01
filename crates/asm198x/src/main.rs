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
        let spelling = dialect
            .map(str::to_ascii_lowercase)
            .or_else(|| chip.map(str::to_ascii_lowercase));
        // Aliases collapse to the canonical name in the table, so the arms
        // below name each dialect once and a spelling with no row is refused
        // before it gets here.
        let key = match spelling.as_deref() {
            Some(name) => match asm198x::dialect_table::canonical(name) {
                Some(canonical) => Some(canonical),
                None => {
                    return Err(format!(
                        "unknown dialect `{name}` (try acme, ca65, pasmo, pasmonext, or sjasmplus)"
                    ));
                }
            },
            None => None,
        };
        match key {
            // ACME is the default 6502 dialect (C64); ca65 targets the NES.
            Some("acme") => Ok(Self::Acme),
            Some("ca65") => Ok(Self::Ca65),
            Some("vasm") => Ok(Self::Vasm),
            Some("lwasm") => Ok(Self::Lwasm),
            Some("65816") => Ok(Self::Ca65_816),
            Some("huc6280") => Ok(Self::Ca65Huc6280),
            Some("rgbasm") => Ok(Self::Rgbasm),
            Some("8080") => Ok(Self::I8080),
            Some("6800") => Ok(Self::M6800),
            Some("1802") => Ok(Self::Cdp1802),
            // The ROM'd MCS-48 parts share the 8048's full set; the ROM-less kin
            // (8035/8039/8040, incl. CMOS) forbid the four BUS-port instructions.
            Some("8048") => Ok(Self::I8048 { romless: false }),
            Some("8035") => Ok(Self::I8048 { romless: true }),
            Some("scmp") => Ok(Self::Scmp),
            Some("f8") => Ok(Self::F8),
            Some("2650") => Ok(Self::S2650),
            Some("tms7000") => Ok(Self::Tms7000),
            Some("pdp11") => Ok(Self::Pdp11),
            Some("tms9900") => Ok(Self::Tms9900),
            Some("cp1610") => Ok(Self::Cp1610),
            Some("z8000") => Ok(Self::Z8000),
            Some("z8001") => Ok(Self::Z8001),
            // pasmo defaults to plain Z80; pasmonext defaults to Z80N. An
            // explicit --cpu/--target wins.
            Some("pasmo") => Ok(Self::Pasmo {
                z80n: z80n.unwrap_or(false),
            }),
            Some("pasmonext") => Ok(Self::Pasmo {
                z80n: z80n.unwrap_or(true),
            }),
            Some("sjasmplus") => Ok(Self::Sjasmplus {
                z80n: z80n.unwrap_or(false),
            }),
            // Unreachable: `canonical` already refused anything with no row,
            // so this arm fires only if the table gains an entry that
            // resolution does not handle — which a test catches first.
            Some(other) => Err(format!("dialect `{other}` has no assembler wired up")),
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

const NINTENDO_LOGO: [u8; 48] = [
    0xCE, 0xED, 0x66, 0x66, 0xCC, 0x0D, 0x00, 0x0B, 0x03, 0x73, 0x00, 0x83, 0x00, 0x0C, 0x00, 0x0D,
    0x00, 0x08, 0x11, 0x1F, 0x88, 0x89, 0x00, 0x0E, 0xDC, 0xCC, 0x6E, 0xE6, 0xDD, 0xDD, 0xD9, 0x99,
    0xBB, 0xBB, 0x67, 0x63, 0x6E, 0x0E, 0xEC, 0xCC, 0xDD, 0xDC, 0x99, 0x9F, 0xBB, 0xB9, 0x33, 0x3E,
];

/// Apply the cartridge framing performed by `rgbfix -v -p 0xff`: install the
/// Nintendo logo, pad to the next valid power-of-two ROM size (32 KiB minimum),
/// record that size in the header, then write the header and global checksums.
/// The flag reports RGBFIX's `-Woverwrite` condition for a replaced non-zero
/// logo byte.
fn game_boy_rom(bytes: &[u8]) -> Result<(Vec<u8>, bool), String> {
    if bytes.len() < 0x150 {
        return Err("a Game Boy ROM must contain the complete $0100-$014f header".into());
    }
    let linked_size = bytes.len().div_ceil(0x4000) * 0x4000;
    let size = linked_size.max(0x8000).next_power_of_two();
    if size > 0x80_0000 {
        return Err("Game Boy ROM exceeds the 8 MiB header limit".into());
    }
    let mut rom = bytes.to_vec();
    // RGBLINK materialises the rest of the highest used bank with its zero
    // fill; rgbfix's requested $ff padding begins only after that linked ROM.
    rom.resize(linked_size, 0x00);
    rom.resize(size, 0xFF);
    let overwrote_logo = rom[0x104..0x134]
        .iter()
        .zip(NINTENDO_LOGO)
        .any(|(&old, new)| old != 0 && old != new);
    rom[0x104..0x134].copy_from_slice(&NINTENDO_LOGO);
    rom[0x148] = (size / 0x8000).ilog2() as u8;

    let header = rom[0x134..=0x14C]
        .iter()
        .fold(0u8, |sum, &byte| sum.wrapping_sub(byte).wrapping_sub(1));
    rom[0x14D] = header;
    rom[0x14E] = 0;
    rom[0x14F] = 0;
    let global = rom
        .iter()
        .fold(0u16, |sum, &byte| sum.wrapping_add(u16::from(byte)));
    rom[0x14E..=0x14F].copy_from_slice(&global.to_be_bytes());
    Ok((rom, overwrote_logo))
}

/// Materialise the complete highest ROM bank, matching RGBLINK without `-x`.
/// The library keeps its compact `rgblink -x` byte view for expression and
/// differential tests; the CLI's raw linked artifact has bank extent.
fn rgbasm_raw_image(bytes: &[u8]) -> Vec<u8> {
    let size = bytes.len().div_ceil(0x4000).max(1) * 0x4000;
    let mut image = bytes.to_vec();
    image.resize(size, 0);
    image
}

/// Which operation the invocation asks for. Named as a subcommand
/// (`asm198x disasm …`), git/cargo style, per
/// `decisions/packaging-and-cpu-roadmap.md`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Assemble,
    Disassemble,
    Format,
    Convert,
}

fn run(args: &[String]) -> Result<String, String> {
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        return Ok(usage());
    }

    // Before the subcommand dispatch, so a bare `version` is answered rather
    // than falling through to the default operation and being read as an input
    // filename. git and cargo both accept the word and the flags; so do we.
    if args[0] == "version" || args.iter().any(|a| a == "-V" || a == "--version") {
        return Ok(version());
    }

    // `dialects` prints the table `--dialect` resolves against, `--markdown`
    // in the shape the CLI reference wants. The reference lives in another
    // repo and had drifted — five dialects missing, and the ROM-less MCS-48
    // parts described as aliases of the 8048 when they refuse instructions it
    // accepts. Generating the page's table from here is what stops that
    // happening again; see `../docs/cli.md`.
    if args[0] == "dialects" {
        // Straight to stdout, not through the summary: this is the command's
        // *output*, and `dialects --markdown` exists to be redirected into the
        // reference. The summary channel is stderr, which is right for a
        // usage screen and wrong for something meant to be piped.
        if args.iter().any(|a| a == "--markdown") {
            print!("{}", asm198x::dialect_table::markdown());
        } else {
            println!("{}", dialect_help());
        }
        return Ok(String::new());
    }

    // Assembling is the default when no subcommand is given, so `asm198x
    // prog.asm` keeps working. That is the overwhelmingly common invocation and
    // the only one external callers use (Code198x's capture harness drives it
    // that way), and the packaging decision rules out the `--disasm` *flag*, not
    // a default operation.
    let (mode, args) = match args[0].as_str() {
        "asm" => (Mode::Assemble, &args[1..]),
        "disasm" => (Mode::Disassemble, &args[1..]),
        "fmt" => (Mode::Format, &args[1..]),
        "convert" => (Mode::Convert, &args[1..]),
        _ => (Mode::Assemble, args),
    };
    if args.is_empty() {
        return Ok(usage());
    }
    if let Mode::Convert = mode {
        return run_convert(args);
    }
    let disassemble = mode == Mode::Disassemble;
    let format = mode == Mode::Format;

    let mut input: Option<&str> = None;
    let mut output: Option<PathBuf> = None;
    let mut dialect: Option<&str> = None;
    let mut target: Option<&str> = None;
    let mut exe = false;
    let mut sna = false;
    let mut prg = false;
    let mut gb_rom = false;
    // `--tap`/`--tzx` frame the program for tape; the `bas` spellings put
    // pasmo's auto-run BASIC loader in front of it.
    let mut tape: Option<(asm198x::TapeFormat, bool)> = None;
    let mut origin: u16 = 0;
    let mut message_format = MessageFormat::Human;
    // Debug198x artifacts (U3): `None` = flag absent; `Some(None)` = default
    // path (input with the artifact's extension); `Some(Some(p))` = explicit.
    let mut debug: ArtifactPath = None;
    let mut sym: ArtifactPath = None;
    let mut listing: ArtifactPath = None;
    let mut listing_json: ArtifactPath = None;
    let mut linker_config: Option<PathBuf> = None;
    // Repeatable `-I <dir>` include-search directories, in command-line order —
    // the order is the search order (language-surface U1/KTD8). The
    // include-capable entry points (U2) feed them to the filesystem loader.
    let mut include_dirs: Vec<PathBuf> = Vec::new();
    // Pasmo-compatible command-line constants. They are written as an
    // assembly prelude before parsing, so conditional assembly sees them just
    // as it sees an `equ` at the top of the source.
    let mut equ_definitions: Vec<&str> = Vec::new();
    // sjasmplus's own flag: "Prefix for save/output/.. filenames in
    // directives". It is the reference conceding the host gets a say over
    // source-named writes without changing the language.
    let mut outprefix: Option<PathBuf> = None;
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
            "--outprefix" => {
                i += 1;
                let path = args.get(i).ok_or("`--outprefix` needs a path")?;
                outprefix = Some(PathBuf::from(path));
            }
            f if f.starts_with("--outprefix=") => {
                outprefix = Some(PathBuf::from(&f["--outprefix=".len()..]));
            }
            "--listing" => listing = Some(None),
            f if f.starts_with("--listing=") => {
                listing = Some(Some(PathBuf::from(&f["--listing=".len()..])));
            }
            "-C" => {
                i += 1;
                let value = args.get(i).ok_or("`-C` needs a linker config path")?;
                linker_config = Some(PathBuf::from(value));
            }
            "--listing-json" => listing_json = Some(None),
            f if f.starts_with("--listing-json=") => {
                listing_json = Some(Some(PathBuf::from(&f["--listing-json=".len()..])));
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
            "--equ" => {
                i += 1;
                equ_definitions.push(args.get(i).ok_or("`--equ` needs NAME=VALUE")?);
            }
            f if f.starts_with("--equ=") => {
                equ_definitions.push(&f["--equ=".len()..]);
            }
            "--disasm" | "--disassemble" => {
                return Err("`--disasm` is now a subcommand: `asm198x disasm <input.bin>`".into());
            }
            "--fmt" | "--format" => {
                return Err("`--fmt` is now a subcommand: `asm198x fmt <input.asm>`".into());
            }
            "--exe" | "--hunkexe" => exe = true,
            "--sna" => sna = true,
            "--prg" => prg = true,
            "--gb-rom" => gb_rom = true,
            "--tap" => tape = Some((asm198x::TapeFormat::Tap, false)),
            "--tapbas" => tape = Some((asm198x::TapeFormat::Tap, true)),
            "--tzx" => tape = Some((asm198x::TapeFormat::Tzx, false)),
            "--tzxbas" => tape = Some((asm198x::TapeFormat::Tzx, true)),
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
    if (debug.is_some() || sym.is_some() || listing.is_some() || listing_json.is_some())
        && (format || disassemble)
    {
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
    if !equ_definitions.is_empty()
        && (!matches!(mode, Mode::Assemble) || !matches!(assembler, Assembler::Pasmo { .. }))
    {
        return Err("`--equ` is an assembly option for the pasmo/pasmonext dialects".into());
    }
    let source = apply_equ_definitions(&source, &equ_definitions)?;
    if gb_rom && !matches!(assembler, Assembler::Rgbasm) {
        return Err("`--gb-rom` is only for the Game Boy dialect (rgbasm)".into());
    }
    if gb_rom && matches!(message_format, MessageFormat::Json) {
        return Err("`--gb-rom` is not yet available with `--message-format=json`".into());
    }

    // Debug198x artifacts: every path emits them (flat U3, ca65 U4, vasm U5).
    // The ca65/vasm listings wait on a per-section byte map, so only the
    // record-backed artifacts (`--debug`, `--sym`) are live there.
    if linker_config.is_some() && !matches!(assembler, Assembler::Ca65) {
        return Err(
            "`-C` selects a ca65 linker configuration; this dialect has no linker config".into(),
        );
    }
    if (listing.is_some() || listing_json.is_some())
        && matches!(assembler, Assembler::Ca65 | Assembler::Vasm)
    {
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
        // Name the destination, as the `-o` arm does. "formatted <input>" alone
        // reads as though the input were rewritten in place, which it is not.
        return Ok(format!("formatted {input} -> stdout"));
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
            (&debug, &sym, &listing, &listing_json),
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
        // vasm's convention: the executable drops the source extension. A
        // source may name the file itself with `output`, and the flag still
        // wins — `decisions/source-named-output-files.md`, the same three
        // rules ACME's `!to` follows.
        let out_path = match (&output, &result.requested_output) {
            (None, Some(req)) => source_named_path(input, &req.path)?,
            _ => output
                .unwrap_or_else(|| Path::new(input).with_extension(if exe { "" } else { "bin" })),
        };
        std::fs::write(&out_path, &result.bytes)
            .map_err(|e| format!("cannot write {}: {e}", out_path.display()))?;
        let artifact_notes = write_artifacts(&result.artifacts, &out_path, outprefix.as_deref())?;
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
                &listing_json,
            )?,
            None => String::new(),
        };
        return Ok(format!(
            "assembled {} byte(s) -> {}{artifact_notes}{debug_notes}",
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
        // `-C` selects the project's own ld65 configuration (#483); absent,
        // the curriculum default applies exactly as before.
        let cfg = match &linker_config {
            Some(path) => Some(
                std::fs::read_to_string(path)
                    .map_err(|e| format!("cannot read {}: {e}", path.display()))?,
            ),
            None => None,
        };
        let (rom, info) = match (&cfg, debug.is_some() || sym.is_some()) {
            (Some(cfg), true) => {
                let (rom, info) =
                    asm198x::assemble_ca65_files_debug_with_config(&source, input, &loader, cfg)
                        .map_err(|e| render_multi_error(input, &e))?;
                (rom, Some(info))
            }
            (Some(cfg), false) => (
                asm198x::assemble_ca65_files_with_config(&source, input, &loader, cfg)
                    .map_err(|e| render_multi_error(input, &e))?,
                None,
            ),
            (None, true) => {
                let (rom, info) = asm198x::assemble_ca65_files_debug(&source, input, &loader)
                    .map_err(|e| render_multi_error(input, &e))?;
                (rom, Some(info))
            }
            (None, false) => (
                asm198x::assemble_ca65_files(&source, input, &loader)
                    .map_err(|e| render_multi_error(input, &e))?,
                None,
            ),
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
                &listing_json,
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
    if tape.is_some()
        && !matches!(
            assembler,
            Assembler::Pasmo { .. } | Assembler::Sjasmplus { .. }
        )
    {
        return Err(
            "`--tap`/`--tzx` are only for the Spectrum Z80 dialects (pasmo/pasmonext/sjasmplus)"
                .into(),
        );
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
    let (summary, image_path) = if let Some((format, autorun)) = tape {
        // The block name is the output path as pasmo would have been given it,
        // so the default extension has to be settled before the image is built
        // rather than after.
        let extension = match format {
            asm198x::TapeFormat::Tap => "tap",
            asm198x::TapeFormat::Tzx => "tzx",
        };
        let out_path = output.unwrap_or_else(|| Path::new(input).with_extension(extension));
        let image = asm198x::tape(&assembly, format, &out_path.to_string_lossy(), autorun)
            .map_err(|e| render_error(input, &files, &e))?;
        std::fs::write(&out_path, &image)
            .map_err(|e| format!("cannot write {}: {e}", out_path.display()))?;
        let summary = format!(
            "assembled {} byte(s) -> {} ({} tape{})",
            image.len(),
            out_path.display(),
            extension,
            if autorun { ", auto-run" } else { "" },
        );
        (summary, out_path)
    } else if sna {
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
    } else if gb_rom {
        let (image, overwrote_logo) = game_boy_rom(&assembly.bytes)?;
        if overwrote_logo {
            eprintln!("asm198x: warning: overwrote a non-zero byte in the Nintendo logo");
        }
        let out_path = output.unwrap_or_else(|| Path::new(input).with_extension("gb"));
        std::fs::write(&out_path, &image)
            .map_err(|e| format!("cannot write {}: {e}", out_path.display()))?;
        let summary = format!(
            "assembled + finalised {} byte(s) -> {} (Game Boy ROM)",
            image.len(),
            out_path.display(),
        );
        (summary, out_path)
    } else {
        // The source may name its own output with ACME's `!to`. A `-o` still
        // wins — that is ACME's rule, which takes the *first* name chosen and
        // warns that it was "already chosen" for any later one; the flag is
        // chosen before the source is read. So this only applies when no flag
        // was given, and it replaces the derived default rather than adding a
        // second file.
        let framed;
        let linked =
            matches!(assembler, Assembler::Rgbasm).then(|| rgbasm_raw_image(&assembly.bytes));
        let (out_path, image) = match (&output, &assembly.requested_output) {
            (None, Some(req)) => {
                let path = source_named_path(input, &req.path)?;
                framed = frame_output(&assembly, req.format);
                (path, framed.as_slice())
            }
            _ => {
                let image = linked.as_deref().unwrap_or(assembly.bytes.as_slice());
                (
                    output.unwrap_or_else(|| Path::new(input).with_extension("bin")),
                    image,
                )
            }
        };
        std::fs::write(&out_path, image)
            .map_err(|e| format!("cannot write {}: {e}", out_path.display()))?;
        let artifact_notes = write_artifacts(&assembly.artifacts, &out_path, outprefix.as_deref())?;
        let summary = format!(
            "assembled {} byte(s) at ${:04X} -> {}{artifact_notes}",
            image.len(),
            assembly.origin.unwrap_or(0),
            out_path.display(),
        );
        (summary, out_path)
    };

    // ACME's `!symbollist` names a symbol dump, on the same terms as `!to`:
    // a request the flag overrides. `--sym` in any form counts as the name
    // already chosen, so this only applies when none was given.
    let sym = match (&sym, &assembly.requested_symbols) {
        (None, Some(named)) => Some(Some(source_named_path(input, named)?)),
        _ => sym,
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
            &listing_json,
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
/// Resolve a path the **source** named, against the input's directory.
///
/// Source is data, not a command: a `!to "../../elsewhere"` names a file
/// outside the tree being assembled, and assembling a file someone else wrote
/// should not write outside it. So the name is taken relative to the input and
/// refused if it escapes — ACME takes it literally, and this is a deliberate
/// narrowing, recorded in `decisions/source-named-output-files.md`.
fn source_named_path(input: &str, named: &str) -> Result<PathBuf, String> {
    let base = Path::new(input)
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();
    let candidate = Path::new(named);
    if candidate.is_absolute() {
        return Err(format!(
            "the source names the absolute path `{named}`; a source-named output \
             stays beside the source it came from"
        ));
    }
    if candidate
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(format!(
            "the source names `{named}`, which climbs out of its own directory; a \
             source-named output stays beside the source it came from"
        ));
    }
    Ok(base.join(candidate))
}

/// Write the files a source asked for besides the machine code, and say so.
///
/// The rules are `decisions/multi-artifact-output.md`'s, and two of them are
/// deliberate rather than incidental. A name resolves against the **output
/// directory**, so a source's `SAVEBIN "x.bin"` lands beside the binary rather
/// than wherever the process happens to be standing. And an absolute path or
/// one climbing out with `..` is **written and reported**, not refused —
/// refusing would fail source the reference accepts, which is the
/// out-converging the house rule warns against. `--outprefix` is the control
/// for a caller that wants one, mirroring sjasmplus's flag of the same name.
///
/// Every write is reported, so a caller sees the full set of side effects
/// without scraping for them.
fn write_artifacts(
    artifacts: &[asm198x::Artifact],
    out_path: &Path,
    outprefix: Option<&Path>,
) -> Result<String, String> {
    let mut notes = String::new();
    for artifact in artifacts {
        let named = Path::new(&artifact.name);
        let path = match (outprefix, named.is_absolute()) {
            (Some(prefix), _) => prefix.join(named),
            (None, true) => named.to_path_buf(),
            (None, false) => out_path.parent().unwrap_or(Path::new(".")).join(named),
        };
        std::fs::write(&path, &artifact.bytes)
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        notes.push_str(&format!(
            "\nwrote {} byte(s) -> {}",
            artifact.bytes.len(),
            path.display()
        ));
    }
    Ok(notes)
}

/// Frame the image the way `!to`'s format asks.
fn frame_output(assembly: &asm198x::AssemblyResult, format: asm198x::OutputFormat) -> Vec<u8> {
    let origin = assembly.origin.unwrap_or(0);
    let mut out = Vec::new();
    match format {
        asm198x::OutputFormat::Plain => {}
        // A C64 `.prg`: the load address, little-endian.
        asm198x::OutputFormat::Cbm => out.extend_from_slice(&origin.to_le_bytes()),
        // Apple: the load address, then the length.
        asm198x::OutputFormat::Apple => {
            out.extend_from_slice(&origin.to_le_bytes());
            let len = u16::try_from(assembly.bytes.len()).unwrap_or(u16::MAX);
            out.extend_from_slice(&len.to_le_bytes());
        }
    }
    out.extend_from_slice(&assembly.bytes);
    out
}

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
    listing_json: &ArtifactPath,
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
    if let Some(path) = listing_json {
        let text = asm198x::render_listing_json(input, assembly, addr_unit);
        emit(path, "lst.json", "JSON listing", text)?;
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
            format!(
                "{file}:{}:{}: error: {}{}",
                span.line,
                span.col,
                e.message,
                render_expansion_notes(span)
            )
        }
        Some((span, file)) => format!(
            "{file}:{}: error: {}{}",
            span.line,
            e.message,
            render_expansion_notes(span)
        ),
        None => format!("{input}: {e}"),
    }
}

/// One rustc-style note per macro expansion the failing text came through,
/// innermost first — the same order and shape as the `included from` chain.
///
/// Without these, an error in generated code points at an invocation and says
/// nothing about why: the failing text is nowhere in the file the reader has
/// open. The note is the difference between "line 6 is wrong" and "line 6
/// expands a macro whose body is wrong".
fn render_expansion_notes(span: &asm198x::Span) -> String {
    span.expansion_frames
        .iter()
        .map(|frame| {
            let invoked = frame.invoked_at.path.as_deref().map_or_else(
                || format!("line {}", frame.invoked_at.line),
                |path| format!("{path}:{}", frame.invoked_at.line),
            );
            let defined = frame.defined_at.as_deref().map(|span| {
                span.path.as_deref().map_or_else(
                    || format!("line {}", span.line),
                    |path| format!("{path}:{}", span.line),
                )
            });
            match defined {
                Some(defined) => format!(
                    "\nin expansion of macro `{}` defined at {defined}, invoked at {invoked}",
                    frame.macro_name
                ),
                None => format!(
                    "\nin expansion of macro `{}` invoked at {invoked}",
                    frame.macro_name
                ),
            }
        })
        .collect()
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

/// Turn Pasmo's repeatable `--equ NAME=VALUE` arguments into a source prelude.
/// Keeping the value as source text deliberately gives it Pasmo's own numeric
/// and expression grammar rather than inventing a second CLI-only parser.
fn apply_equ_definitions(source: &str, definitions: &[&str]) -> Result<String, String> {
    let mut prelude = String::new();
    for definition in definitions {
        let (name, value) = definition.split_once('=').ok_or_else(|| {
            format!("invalid `--equ` definition `{definition}`; expected NAME=VALUE")
        })?;
        if name.is_empty()
            || !name.chars().enumerate().all(|(i, c)| {
                c == '_' || c.is_ascii_alphanumeric() && (i > 0 || !c.is_ascii_digit())
            })
            || value.trim().is_empty()
        {
            return Err(format!(
                "invalid `--equ` definition `{definition}`; expected NAME=VALUE"
            ));
        }
        prelude.push_str(name);
        prelude.push_str(" equ ");
        prelude.push_str(value);
        prelude.push('\n');
    }
    prelude.push_str(source);
    Ok(prelude)
}

/// `asm198x convert --from <dialect> --to <dialect> <input> [-o <out>]`
/// (#502): parse-and-re-emit conversion between dialects of one CPU,
/// self-verified — output is written only when both sides assemble to
/// byte-identical images.
fn run_convert(args: &[String]) -> Result<String, String> {
    let mut from = None;
    let mut to = None;
    let mut input = None;
    let mut output: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--from" => {
                i += 1;
                from = Some(args.get(i).ok_or("`--from` needs a dialect")?.clone());
            }
            "--to" => {
                i += 1;
                to = Some(args.get(i).ok_or("`--to` needs a dialect")?.clone());
            }
            "-o" => {
                i += 1;
                output = Some(PathBuf::from(args.get(i).ok_or("`-o` needs a path")?));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown convert flag `{flag}`"));
            }
            path if input.is_none() => input = Some(path.to_string()),
            extra => return Err(format!("unexpected argument `{extra}`")),
        }
        i += 1;
    }
    let (Some(from), Some(to), Some(input)) = (from, to, input) else {
        return Err("convert needs `--from <dialect> --to <dialect> <input>`".into());
    };
    let source =
        std::fs::read_to_string(&input).map_err(|e| format!("cannot read {input}: {e}"))?;
    let conversion =
        asm198x::convert(&from, &to, &source).map_err(|e| render_error(&input, &[], &e))?;
    match output {
        Some(path) => {
            std::fs::write(&path, &conversion.output)
                .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            Ok(format!(
                "converted {input} ({from} -> {to}, verified byte-identical) -> {}",
                path.display()
            ))
        }
        None => {
            print!("{}", conversion.output);
            Ok(format!(
                "converted {input} ({from} -> {to}, verified byte-identical) -> stdout"
            ))
        }
    }
}

fn usage() -> String {
    "asm198x — 198x family assembler\n\n\
     usage: asm198x [asm|disasm|fmt] [options] <input>\n\
     \x20      (the operation is a subcommand; with none given, asm198x assembles)\n\n\
     assemble:    asm198x [asm] [--dialect <name>] [--cpu <target>] [-I <dir>]... <input> [-o <out.bin>]\n\
     \x20            (add --message-format=json for a machine-readable result +\n\
     \x20             diagnostics on stdout; --message-format=human is the default;\n\
     \x20             -I adds an include-search directory, repeatable, in order)\n\
     \x20            (--equ NAME=VALUE defines a repeatable pasmo/pasmonext constant\n\
     \x20             before conditional assembly, matching Pasmo's spelling)\n\
     snapshot:    asm198x --dialect pasmonext --sna <input> [-o <out.sna>]\n\
     \x20            (Spectrum Z80 only; needs `end <addr>` for the entry point)\n\
     tape:        asm198x --dialect pasmonext --tap|--tzx <input> [-o <out.tap>]\n\
     \x20            (Spectrum Z80 only; the `bas` spellings — --tapbas/--tzxbas —\n\
     \x20             put an auto-run BASIC loader in front, which needs `end <addr>`\n\
     \x20             for its RANDOMIZE USR line)\n\
     C64 program: asm198x --dialect acme --prg <input> [-o <out.prg>]\n\
     \x20            (prepends the 2-byte load address)\n\
     Game Boy ROM: asm198x --dialect rgbasm --gb-rom <input> [-o <out.gb>]\n\
     \x20            (RGBLINK-compatible layout, padding and header checksums)\n\
     convert:     asm198x convert --from pasmo --to sjasmplus <input> [-o <out>]\n\
     \x20            (parse-and-re-emit dialect conversion, same CPU; output is\n\
     \x20             written only when both sides assemble byte-identically)\n\
     NES project: asm198x --dialect ca65 -C <project.cfg> <input> [-o <out.nes>]\n\
     \x20            (-C reads a bounded ld65 config; absent, the curriculum\n\
     \x20             NROM layout applies as before)\n\
     debug info:  asm198x [--debug[=path]] [--sym[=path]] [--listing[=path]]\n\
     \x20            [--listing-json[=path]] <input>\n\
     \x20            (--debug writes the .debug198x NDJSON sidecar; --sym a sorted\n\
     \x20             `name = $hex` table; --listing address/bytes/cycles/source rows\n\
     \x20             with per-label cycle totals; --listing-json the same data as\n\
     \x20             JSON — defaults: input with .debug198x/.sym/.lst/.lst.json;\n\
     \x20             flat dialects only for now plus the ca65/vasm linked paths\n\
     \x20             for --debug/--sym)\n\
     disassemble: asm198x disasm [-d <dialect>] [--org <addr>] <input.bin>\n\
     \x20            (6502 for acme/ca65/6502; Z80 otherwise)\n\
     format:      asm198x fmt [--cpu <pasmo|sjasmplus|8080|6800|1802|scmp|rgbasm|6809>] <input.asm> [-o <out.asm>]\n\
     \x20            (canonical layout, comments + operand spelling preserved; Z80/8080/6800/1802/scmp/rgbasm/6809)\n\
     version:     asm198x --version (also -V, or `asm198x version`)\n\n\
     targets (--cpu):   z80 (default for pasmo), z80n (Spectrum Next; default\n\
     \x20                 for pasmonext) — Z80N opcodes follow the target, not\n\
     \x20                 the dialect. --cpu also names a chip directly when\n\
     \x20                 --dialect is absent (`--cpu 6809` is lwasm syntax).\n\n"
        .to_string()
        + &dialect_help()
        + "\nAssembles retro CPU source to a flat binary, or disassembles one back."
}

/// The `--help` dialect list, wrapped, from the one table resolution uses.
///
/// It used to be prose, and had drifted: five dialects were missing and the
/// ROM-less MCS-48 parts were folded into the 8048's entry. Rendering it means
/// `--help` cannot say something `--dialect` does not do.
fn dialect_help() -> String {
    const INDENT: &str = "                   ";
    let mut lines = vec![String::from("dialects (--dialect):")];
    for entry in asm198x::dialect_table::DIALECTS {
        let aliases = if entry.aliases.is_empty() {
            String::new()
        } else {
            format!("; also {}", entry.aliases.join("/"))
        };
        lines.push(format!(
            "{INDENT}{:<10} {}{}",
            entry.name, entry.blurb, aliases
        ));
    }
    lines.join("\n")
}

/// Name and version, as `asm198x 0.0.12`.
///
/// Read from the crate version at compile time rather than a string kept by
/// hand, so it reports what was actually built. A version a binary states about
/// itself is only worth having if it cannot drift from the binary.
///
/// The `v` prefix is house style across every surface that shows a version —
/// the site, the docs, the release tags (`asm198x-v0.0.14`) — and the binary
/// matches them so a reader never has to reconcile two spellings of the same
/// build.
fn version() -> String {
    format!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
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
    (debug, sym, listing, listing_json): (
        &ArtifactPath,
        &ArtifactPath,
        &ArtifactPath,
        &ArtifactPath,
    ),
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
                    listing_json,
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

    /// Every spelling reports the same string, and that string is the crate
    /// version — so a release cannot ship a binary that misstates which build
    /// it is. `version` is checked as a subcommand because it used to fall
    /// through to the input-file argument and fail with "cannot read version".
    #[test]
    fn every_version_spelling_reports_the_crate_version() {
        let expected = format!("asm198x v{}", env!("CARGO_PKG_VERSION"));
        for spelling in ["--version", "-V", "version"] {
            let args = vec![spelling.to_string()];
            assert_eq!(run(&args).as_deref(), Ok(expected.as_str()), "{spelling}");
        }
    }

    #[test]
    fn pasmo_equ_definitions_are_visible_to_conditionals() {
        let source =
            "        if NEXT\n        defb 1\n        else\n        defb 2\n        endif\n";
        let next =
            apply_equ_definitions(source, &["NEXT=1", "SPECTRANET=0"]).expect("valid definitions");
        let spectranet =
            apply_equ_definitions(source, &["NEXT=0", "SPECTRANET=1"]).expect("valid definitions");

        assert_eq!(
            asm198x::assemble_pasmo(&next).expect("NEXT build").bytes,
            [1]
        );
        assert_eq!(
            asm198x::assemble_pasmo(&spectranet)
                .expect("Spectranet build")
                .bytes,
            [2]
        );
    }

    #[test]
    fn equ_definitions_require_a_symbol_and_value() {
        for bad in ["NEXT", "=1", "1NEXT=1", "NEXT="] {
            assert!(
                apply_equ_definitions("", &[bad]).is_err(),
                "`{bad}` must be rejected"
            );
        }
    }

    #[test]
    fn game_boy_rom_matches_rgbfix_header_padding_and_checksums() {
        let mut image = vec![0u8; 0x150];
        image[0x134..0x13A].copy_from_slice(b"RACHEL");
        let (rom, overwrote_logo) = game_boy_rom(&image).expect("complete header");
        assert!(!overwrote_logo);
        assert_eq!(rom.len(), 0x8000);
        assert_eq!(rom[0x104..0x134], NINTENDO_LOGO);
        assert_eq!(&rom[0x134..0x13A], b"RACHEL");
        assert_eq!(rom[0x148], 0);
        assert!(rom[0x150..0x4000].iter().all(|&byte| byte == 0x00));
        assert!(rom[0x4000..].iter().all(|&byte| byte == 0xFF));
        let header = rom[0x134..=0x14C]
            .iter()
            .fold(0u8, |sum, &byte| sum.wrapping_sub(byte).wrapping_sub(1));
        assert_eq!(rom[0x14D], header);
        let expected_global = rom
            .iter()
            .enumerate()
            .filter(|(i, _)| !matches!(i, 0x14E | 0x14F))
            .fold(0u16, |sum, (_, &byte)| sum.wrapping_add(u16::from(byte)));
        assert_eq!(
            u16::from_be_bytes([rom[0x14E], rom[0x14F]]),
            expected_global
        );
    }

    #[test]
    fn game_boy_rom_reports_only_overwritten_nonzero_logo_bytes() {
        let mut image = vec![0u8; 0x150];
        image[0x104] = NINTENDO_LOGO[0];
        assert!(!game_boy_rom(&image).expect("matching logo byte").1);

        image[0x104] ^= 0xFF;
        let (rom, overwrote_logo) = game_boy_rom(&image).expect("custom logo byte");
        assert!(overwrote_logo);
        assert_eq!(rom[0x104..0x134], NINTENDO_LOGO);
    }

    #[test]
    #[ignore = "needs rgbfix 1.0.3; run with --ignored"]
    fn game_boy_rom_is_byte_identical_to_rgbfix() {
        let mut image = vec![0u8; 0x4000];
        image[0x100..0x104].copy_from_slice(&[0xC3, 0x50, 0x01, 0x00]);
        image[0x104] = 0xAA;
        image[0x134..0x143].copy_from_slice(b"CUSTOM-TITLE-12");
        image[0x148..0x14C].copy_from_slice(&[7, 0x11, 0x22, 0x33]);

        let path = std::env::temp_dir().join(format!(
            "asm198x-rgbfix-{}-{}.gb",
            std::process::id(),
            line!()
        ));
        std::fs::write(&path, &image).expect("write RGBFIX input");
        let status = std::process::Command::new("rgbfix")
            .args(["-v", "-p", "0xFF"])
            .arg(&path)
            .status()
            .expect("run rgbfix");
        assert!(status.success(), "rgbfix must accept the probe ROM");
        let reference = std::fs::read(&path).expect("read RGBFIX output");
        std::fs::remove_file(&path).expect("remove RGBFIX probe");

        let (ours, overwrote_logo) = game_boy_rom(&image).expect("finalise probe ROM");
        assert!(overwrote_logo);
        assert_eq!(ours, reference);
    }

    #[test]
    fn rgbasm_raw_images_end_at_a_full_bank() {
        assert_eq!(rgbasm_raw_image(&[]).len(), 0x4000);
        let image = rgbasm_raw_image(&[0xAA; 0x101]);
        assert_eq!(image.len(), 0x4000);
        assert_eq!(image[0x100], 0xAA);
        assert!(image[0x101..].iter().all(|&byte| byte == 0));
        assert_eq!(rgbasm_raw_image(&vec![0; 0x8001]).len(), 0xC000);
    }

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
