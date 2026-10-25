//! Which Code198x pattern-library sources the verdicts answer for, and how
//! each machine's reference tool builds them.
//!
//! The pattern library is separate from the game curriculum
//! ([`crate::curriculum`]): it lives under `<machine>/patterns/assembly/`, and
//! for most machines it is all the assembly there is. Its sources share
//! routines through includes, many of them in `cross-platform/patterns/`, so a
//! source is not one file. That is why a pattern verdict is keyed on its whole
//! include closure rather than on the root file's text.
//!
//! The table here names each machine's reference tool. Code198x's Makefiles
//! name the Asm198x dialect they build with, and [`makefile_dialects`] reads
//! them so a test can hold the table to them: a machine missing from the table
//! or disagreeing with its Makefiles fails, rather than going unrecorded.

use std::fs;
use std::path::{Path, PathBuf};

/// A reference tool, with how its output is recorded and replayed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    /// PasmoNext 0.1.3, as `pasmo -I <folder> <source> <out>`.
    Pasmo,
    /// ACME, as `acme -f plain -I <folder> -o <out> <source>`.
    Acme,
    /// ca65, linked by ld65 with the NES curriculum's NROM layout.
    Ca65,
    /// lwasm, as `lwasm --6809 --raw`.
    Lwasm,
    /// vasm's Motorola 68000 assembler, as a Kickstart 1.x hunk executable.
    Vasm,
    /// rgbasm, linked by `rgblink -t` and finished by `rgbfix -v -p 0xFF`.
    Rgbasm,
}

impl Tool {
    /// The `--dialect` the pattern Makefiles pass to Asm198x.
    #[must_use]
    pub fn dialect(self) -> &'static str {
        match self {
            Self::Pasmo => "pasmo",
            Self::Acme => "acme",
            Self::Ca65 => "ca65",
            Self::Lwasm => "lwasm",
            Self::Vasm => "vasm",
            Self::Rgbasm => "rgbasm",
        }
    }

    /// The build variant a verdict is keyed on. It names how the reference
    /// was run, which is not always the Makefile's dialect: PasmoNext accepts
    /// Z80N instructions by default, so its Asm198x counterpart is
    /// `pasmonext`, though the Makefiles say `pasmo`.
    #[must_use]
    pub fn variant(self) -> &'static str {
        match self {
            Self::Pasmo => "pasmonext",
            Self::Acme => "acme",
            Self::Ca65 => "ca65-nes",
            Self::Lwasm => "lwasm",
            Self::Vasm => "vasm-exe",
            Self::Rgbasm => "rgbasm-gb-rom",
        }
    }

    /// The tool for a recorded variant.
    #[must_use]
    pub fn from_variant(variant: &str) -> Option<Self> {
        [
            Self::Pasmo,
            Self::Acme,
            Self::Ca65,
            Self::Lwasm,
            Self::Vasm,
            Self::Rgbasm,
        ]
        .into_iter()
        .find(|t| t.variant() == variant)
    }

    /// The CPU, and so the corpus file the verdict lands in. Spelled as the
    /// existing corpus files spell it.
    #[must_use]
    pub fn cpu(self) -> &'static str {
        match self {
            Self::Pasmo => "Z80",
            Self::Acme | Self::Ca65 => "6502",
            Self::Lwasm => "6809",
            Self::Vasm => "68000",
            Self::Rgbasm => "sm83",
        }
    }

    /// The executable whose version signs the verdict.
    #[must_use]
    pub fn executable(self) -> &'static str {
        match self {
            Self::Pasmo => "pasmo",
            Self::Acme => "acme",
            Self::Ca65 => "ca65",
            Self::Lwasm => "lwasm",
            Self::Vasm => "vasmm68k_mot",
            Self::Rgbasm => "rgbasm",
        }
    }
}

/// Each machine with pattern sources, and the tool that arbitrates them.
pub const BUILDS: &[(&str, Tool)] = &[
    ("acorn-atom", Tool::Acme),
    ("acorn-electron", Tool::Acme),
    ("amstrad-cpc", Tool::Pasmo),
    ("atari-2600", Tool::Acme),
    ("atari-5200", Tool::Acme),
    ("atari-7800", Tool::Acme),
    ("atari-800xl", Tool::Acme),
    ("bbc-micro", Tool::Acme),
    ("coleco-colecovision", Tool::Pasmo),
    ("commodore-64", Tool::Acme),
    ("commodore-amiga", Tool::Vasm),
    ("commodore-pet", Tool::Acme),
    ("commodore-vic-20", Tool::Acme),
    ("dragon-32", Tool::Lwasm),
    ("jupiter-ace", Tool::Pasmo),
    ("mattel-aquarius", Tool::Pasmo),
    ("memotech-mtx", Tool::Pasmo),
    ("msx", Tool::Pasmo),
    ("nintendo-entertainment-system", Tool::Ca65),
    ("nintendo-game-boy", Tool::Rgbasm),
    ("oric-atmos", Tool::Acme),
    ("sega-game-gear", Tool::Pasmo),
    ("sega-master-system", Tool::Pasmo),
    ("sega-sg-1000", Tool::Pasmo),
    ("sinclair-zx-spectrum", Tool::Pasmo),
    ("sinclair-zx80", Tool::Pasmo),
    ("sinclair-zx81", Tool::Pasmo),
    ("sord-m5", Tool::Pasmo),
    ("spectravideo-svi-328", Tool::Pasmo),
    ("tatung-einstein", Tool::Pasmo),
];

/// Machines whose pattern Makefiles never name a dialect: each hands its
/// build to a shared script under `cross-platform/`, which picks the dialect
/// from the CPU. Their table entries are held only by the reference accepting
/// the sources. A Makefile that starts naming a dialect for one of them makes
/// the check say so, and the machine comes off this list.
pub const DIALECT_FROM_BUILD_SCRIPT: &[&str] = &["oric-atmos", "spectravideo-svi-328"];

/// Pattern sources where Asm198x knowingly differs from the reference, each
/// with the issue that tracks it, spelled `issue-NN` as divergences are.
/// Paths are relative to the `code-samples` checkout.
///
/// Replay fails on a difference not listed here, and on a listed source that
/// now matches, so the list only describes differences that exist.
pub const KNOWN: &[(&str, &str)] = &[
    // The acme dialect accepts what ACME 0.97 rejects.
    (
        "acorn-electron/patterns/assembly/rendering/tile-region/demo.asm",
        "issue-967",
    ),
    (
        "atari-2600/patterns/assembly/framework/turn-loop/demo.asm",
        "issue-967",
    ),
    (
        "atari-5200/patterns/assembly/framework/turn-loop/demo.asm",
        "issue-967",
    ),
    (
        "atari-7800/patterns/assembly/framework/turn-loop/demo.asm",
        "issue-967",
    ),
    (
        "atari-800xl/patterns/assembly/framework/turn-loop/demo.asm",
        "issue-967",
    ),
    (
        "bbc-micro/patterns/assembly/rendering/bitmap-text/demo.asm",
        "issue-967",
    ),
    (
        "bbc-micro/patterns/assembly/rendering/geometry/demo.asm",
        "issue-967",
    ),
    (
        "bbc-micro/patterns/assembly/rendering/pixels-and-boxes/demo.asm",
        "issue-967",
    ),
    (
        "commodore-pet/patterns/assembly/rendering/fixed-characters/demo.asm",
        "issue-967",
    ),
    (
        "commodore-pet/patterns/assembly/rendering/tile-region/demo.asm",
        "issue-967",
    ),
    (
        "commodore-vic-20/patterns/assembly/rendering/custom-glyphs/demo.asm",
        "issue-967",
    ),
    (
        "commodore-vic-20/patterns/assembly/rendering/tile-region/demo.asm",
        "issue-967",
    ),
    // The acme dialect writes different bytes from ACME 0.97.
    (
        "atari-2600/patterns/assembly/rendering/tile-map/demo.asm",
        "issue-968",
    ),
    (
        "bbc-micro/patterns/assembly/rendering/clipped-sprites/demo.asm",
        "issue-968",
    ),
    (
        "bbc-micro/patterns/assembly/rendering/pre-shifted-sprites/demo.asm",
        "issue-968",
    ),
    // `--exe` writes a different hunk executable from vasm 2.0f.
    (
        "commodore-amiga/patterns/assembly/rendering/bitmap-text/demo.asm",
        "issue-969",
    ),
    (
        "commodore-amiga/patterns/assembly/rendering/pixels-and-boxes/demo.asm",
        "issue-969",
    ),
    (
        "commodore-amiga/patterns/assembly/rendering/tile-map/demo.asm",
        "issue-969",
    ),
    (
        "commodore-amiga/patterns/assembly/rendering/tile-region/demo.asm",
        "issue-969",
    ),
    // `--gb-rom` pads differently from rgblink and rgbfix.
    (
        "nintendo-game-boy/patterns/assembly/rendering/custom-glyphs/demo.asm",
        "issue-970",
    ),
    (
        "nintendo-game-boy/patterns/assembly/rendering/tile-map/demo.asm",
        "issue-970",
    ),
    (
        "nintendo-game-boy/patterns/assembly/rendering/tile-region/demo.asm",
        "issue-970",
    ),
    // The pasmonext dialect accepts what PasmoNext 0.1.3 rejects.
    (
        "mattel-aquarius/patterns/assembly/rendering/tile-region/demo.asm",
        "issue-971",
    ),
    (
        "sega-game-gear/patterns/assembly/rendering/custom-glyphs/demo.asm",
        "issue-971",
    ),
    (
        "sega-master-system/patterns/assembly/rendering/custom-glyphs/demo.asm",
        "issue-971",
    ),
];

/// The issue tracking a known difference for `source`, relative to the
/// `code-samples` checkout.
#[must_use]
pub fn known(source: &str) -> Option<&'static str> {
    KNOWN
        .iter()
        .find(|(path, _)| *path == source)
        .map(|(_, issue)| *issue)
}

/// The tool that arbitrates a machine's pattern sources.
#[must_use]
pub fn tool(machine: &str) -> Option<Tool> {
    BUILDS
        .iter()
        .find(|(slug, _)| *slug == machine)
        .map(|(_, tool)| *tool)
}

/// The NES pattern sources link with a game unit's layout, which is the NROM
/// layout Asm198x's built-in ca65 configuration reproduces. Relative to the
/// `code-samples` checkout.
pub const NES_LINKER_CONFIG: &str = "nintendo-entertainment-system/assembly/dash/unit-01/nes.cfg";

/// An include that a source's Makefile generates before assembling it, and
/// the command that generates it. Recording and replay run the command into
/// a scratch folder and add that folder to the include path.
pub struct Generated {
    /// The source, relative to the `code-samples` checkout.
    pub source: &'static str,
    /// The include as the source names it.
    pub include: &'static str,
    /// The command, run in the source's folder, before the output path.
    pub command: &'static [&'static str],
}

/// Every generated include the pattern sources need.
pub const GENERATED: &[Generated] = &[Generated {
    source: "sinclair-zx-spectrum/patterns/assembly/input/two-word-parser/demo.asm",
    include: "build/vocabulary.inc",
    command: &["python3", "generate_vocabulary.py", "--output"],
}];

/// The generated include `source` needs, if any. `source` is relative to the
/// `code-samples` checkout.
#[must_use]
pub fn generated(source: &str) -> Option<&'static Generated> {
    GENERATED.iter().find(|g| g.source == source)
}

/// Every machine in the checkout with a pattern assembly tree, sorted.
/// `code_samples` is the `code-samples` checkout itself.
///
/// Enumerated rather than read from the table, so a new machine is noticed
/// the day it lands.
#[must_use]
pub fn machines(code_samples: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(code_samples) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().join("patterns/assembly").is_dir())
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .collect();
    out.sort();
    out
}

/// Every pattern source for a machine, sorted: each `.asm` under
/// `<machine>/patterns/assembly/`, at any depth.
///
/// Everything is a source except what a build writes: `build/` folders hold
/// generated files, and some build scripts write `.asm` there. `mistakes/`
/// and `verification/` sources are included; each is a program in its own
/// right, built from its pattern folder.
#[must_use]
pub fn sources(machine_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(&machine_dir.join("patterns/assembly"), &mut out);
    out.sort();
    out
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if name != "build" && !name.starts_with('.') {
                walk(&path, out);
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("asm") {
            out.push(path);
        }
    }
}

/// The pattern folder a source is built from: its own folder, or the parent
/// of a `mistakes/` or `verification/` folder. The reference is given it as
/// an include path, as the curriculum's builds give it to Asm198x.
#[must_use]
pub fn folder(source: &Path) -> PathBuf {
    let dir = source.parent().unwrap_or(Path::new("."));
    match dir.file_name().and_then(|n| n.to_str()) {
        Some("mistakes" | "verification") => dir.parent().unwrap_or(dir).to_path_buf(),
        _ => dir.to_path_buf(),
    }
}

/// Every `--dialect` a machine's pattern Makefiles pass, with the Makefile
/// that passes it.
#[must_use]
pub fn makefile_dialects(machine_dir: &Path) -> Vec<(PathBuf, String)> {
    let mut makefiles = Vec::new();
    find_makefiles(&machine_dir.join("patterns/assembly"), &mut makefiles);
    makefiles.sort();
    let mut out = Vec::new();
    for makefile in makefiles {
        let Ok(text) = fs::read_to_string(&makefile) else {
            continue;
        };
        for dialect in dialects_in(&text) {
            out.push((makefile.clone(), dialect));
        }
    }
    out
}

fn find_makefiles(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            find_makefiles(&path, out);
        } else if entry.file_name() == "Makefile" {
            out.push(path);
        }
    }
}

/// The values of every `--dialect X` or `--dialect=X` in a Makefile's text.
#[must_use]
pub fn dialects_in(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut words = text.split_whitespace().peekable();
    while let Some(word) = words.next() {
        if word == "--dialect" {
            if let Some(value) = words.next() {
                out.push(value.to_string());
            }
        } else if let Some(value) = word.strip_prefix("--dialect=") {
            out.push(value.to_string());
        }
    }
    out
}

/// What is wrong with the table, measured against a `code-samples`
/// checkout: one line per problem, empty when it agrees.
///
/// A machine with pattern sources must be in the table, and each `--dialect`
/// its Makefiles pass must be its tool's. A machine in the table must have
/// pattern sources, and must either have a Makefile naming its dialect or be
/// in [`DIALECT_FROM_BUILD_SCRIPT`], so the table cannot carry an entry that
/// nothing holds it to.
#[must_use]
pub fn table_problems(code_samples: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let present = machines(code_samples);
    for machine in &present {
        let dir = code_samples.join(machine);
        if sources(&dir).is_empty() {
            continue;
        }
        let Some(tool) = tool(machine) else {
            out.push(format!(
                "{machine} has pattern sources but no entry in verdict_corpus::patterns::BUILDS"
            ));
            continue;
        };
        let dialects = makefile_dialects(&dir);
        for (makefile, dialect) in &dialects {
            if dialect != tool.dialect() {
                out.push(format!(
                    "{} builds with --dialect {dialect}, but the table gives {machine} {}",
                    makefile.display(),
                    tool.dialect()
                ));
            }
        }
        let from_script = DIALECT_FROM_BUILD_SCRIPT.contains(&machine.as_str());
        match (dialects.is_empty(), from_script) {
            (true, false) => out.push(format!(
                "no pattern Makefile for {machine} names a dialect, so nothing holds its \
                 table entry; list it in DIALECT_FROM_BUILD_SCRIPT if a build script picks it"
            )),
            (false, true) => out.push(format!(
                "{machine}'s Makefiles now name a dialect, so take it off \
                 DIALECT_FROM_BUILD_SCRIPT"
            )),
            _ => {}
        }
    }
    for (machine, _) in BUILDS {
        if !present.iter().any(|m| m == machine) || sources(&code_samples.join(machine)).is_empty()
        {
            out.push(format!(
                "{machine} is in the table but has no pattern sources in the checkout"
            ));
        }
    }
    for g in GENERATED {
        if !code_samples.join(g.source).is_file() {
            out.push(format!(
                "GENERATED names {}, which is not in the checkout",
                g.source
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "verdict-corpus-patterns-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn put(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        fs::write(path, text).expect("write");
    }

    #[test]
    fn a_mistake_and_a_probe_build_from_their_pattern_folder() {
        let p = Path::new("m/patterns/assembly/input/keys");
        assert_eq!(folder(&p.join("demo.asm")), p);
        assert_eq!(folder(&p.join("mistakes/late.asm")), p);
        assert_eq!(folder(&p.join("verification/probe.asm")), p);
        assert_eq!(
            folder(&p.join("variants/fast.asm")),
            p.join("variants"),
            "only mistakes/ and verification/ step up"
        );
    }

    #[test]
    fn sources_skip_what_a_build_writes() {
        let root = tree("sources");
        put(&root, "m/patterns/assembly/a/demo.asm", "");
        put(&root, "m/patterns/assembly/a/mistakes/late.asm", "");
        put(&root, "m/patterns/assembly/a/routine.inc", "");
        put(&root, "m/patterns/assembly/a/build/selected.asm", "");
        let found: Vec<String> = sources(&root.join("m"))
            .iter()
            .map(|p| {
                p.strip_prefix(&root)
                    .expect("under root")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(
            found,
            [
                "m/patterns/assembly/a/demo.asm",
                "m/patterns/assembly/a/mistakes/late.asm"
            ]
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn both_spellings_of_the_dialect_flag_are_read() {
        let text =
            "all:\n\t$(ASM) --dialect pasmo --cpu z80 x.asm\n\t$(ASM) --dialect=acme y.asm\n";
        assert_eq!(dialects_in(text), ["pasmo", "acme"]);
    }

    #[test]
    fn every_variant_names_one_tool() {
        for (_, tool) in BUILDS {
            assert_eq!(Tool::from_variant(tool.variant()), Some(*tool));
        }
    }

    /// The check fails each way it claims to.
    #[test]
    fn the_table_check_can_fail() {
        let root = tree("table");
        // A machine the table does not know.
        put(&root, "unknown-machine/patterns/assembly/a/demo.asm", "");
        // A known machine whose Makefile disagrees.
        put(&root, "sord-m5/patterns/assembly/a/demo.asm", "");
        put(
            &root,
            "sord-m5/patterns/assembly/a/Makefile",
            "all:\n\tasm198x --dialect acme demo.asm\n",
        );
        // A known machine with sources and no Makefile naming a dialect.
        put(&root, "msx/patterns/assembly/a/demo.asm", "");
        let problems = table_problems(&root).join("\n");
        assert!(
            problems.contains("unknown-machine has pattern sources"),
            "{problems}"
        );
        assert!(
            problems.contains("--dialect acme, but the table gives sord-m5 pasmo"),
            "{problems}"
        );
        assert!(
            problems.contains("no pattern Makefile for msx"),
            "{problems}"
        );
        assert!(
            problems.contains("bbc-micro is in the table but has no pattern sources"),
            "{problems}"
        );
        let _ = fs::remove_dir_all(&root);
    }
}
