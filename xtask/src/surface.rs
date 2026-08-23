//! Vocabulary coverage: how much of each reference's *own* vocabulary we take.
//!
//! [`crate::coverage`] answers "how much of the spec has a recorded verdict",
//! over a denominator the spec supplies. This answers a question that
//! denominator cannot reach: **how much of what the reference tool accepts do
//! we accept at all**.
//!
//! The difference is the whole reason this exists. Arbitration coverage reads
//! 100% on fourteen CPUs, and sjasmplus still takes `sli`, `exa`, `mulub` and
//! a hundred other words we refuse — because the denominator is our spec, and
//! a form missing from our spec is missing from the measurement too. A number
//! that cannot fall when we are wrong is not measuring us.
//!
//! # How the denominator is obtained
//!
//! From the reference binary, by asking it.
//!
//! 1. Harvest every identifier-shaped run of printable bytes in the executable.
//!    Most are not vocabulary — they are symbol names, format strings, libc.
//! 2. Offer each one to the reference as a lone operation. Whatever it does
//!    *not* call unknown is vocabulary: a directive, a mnemonic, an alias.
//! 3. Offer the survivors to us.
//!
//! Step 2 is what makes the result trustworthy: the reference filters its own
//! candidates, so nothing here depends on a manual, a wiki, or a guess about
//! what a tool "probably" supports. Everything this reports was answered by
//! the tool on this machine.
//!
//! # What it is a lower bound on
//!
//! A word only appears if it is stored in the binary as literal bytes. A tool
//! that builds its keywords from a table of fragments, or compresses them, is
//! under-counted — so **a gap this finds is real, and a clean result is not
//! proof of completeness.**
//!
//! It measures *names* and nothing else. Whether our `!fill` means what ACME's
//! `!fill` means is a question for the differential corpus, and that is the
//! expensive half. Reading a 100% here as "done" would be the same mistake
//! this module was written to expose, one level up.
//!
//! # The detector self-check
//!
//! Every reference reports an unknown word differently — ACME says "Unknown
//! pseudo opcode", vasm "unknown mnemonic", ca65 "is not a recognized control
//! command" — so each entry below carries the substrings that mean it. A
//! mis-tuned or outdated detector would silently report every candidate as
//! recognised, which reads as a huge gap, or none, which reads as perfection.
//!
//! So each detector is verified before it is used: a word the tool certainly
//! knows must not trip it, and a word nothing could know must. A reference
//! failing either is reported as unusable and skipped. **An unusable detector
//! is a visible hole, never a zero.**

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The stamp file, tracked so a change to it is reviewable.
pub const STAMP: &str = "crates/asm198x/tests/verdicts/surface.stamp";

/// One reference tool, and how to ask it whether it knows a word.
struct Reference {
    /// The `--dialect` name whose surface is compared against this tool.
    dialect: &'static str,
    bin: &'static str,
    ext: &'static str,
    /// Lines the tool needs before any code (ACME's mandatory origin).
    prologue: &'static str,
    /// What a directive is spelled with here: ACME's `!`, ca65's `.`. Where
    /// there is one, only the sigilled form is offered — a bare word is a
    /// *label* in ACME and ca65, not a directive, so asking about it would
    /// measure the wrong thing. Their mnemonics are covered by the ISA sweeps.
    sigil: &'static str,
    /// Command arguments, with `{src}` and `{out}` substituted.
    args: &'static [&'static str],
    /// Substrings in the tool's output that mean "I do not know that word".
    unknown: &'static [&'static str],
    /// Substrings that mean "I know it, and it is not for the target you asked
    /// me to assemble for" — a 68881 mnemonic under `-m68000`, a 6309 one in
    /// 6809 mode, a `.a16` outside 65816 mode. Counted, and counted apart:
    /// see [`covered`]'s note on why they are not a gap.
    off_target: &'static [&'static str],
    /// A word this tool certainly knows, for the self-check.
    known: &'static str,
    /// Does our spec declare this mnemonic? A predicate rather than a set,
    /// because the ISA crate exposes a 68000 differently from an 8-bit CPU,
    /// and a reference with no clean handle answers `false` and falls through
    /// to the assembler test. See [`covered`].
    mnemonic: fn(&str) -> bool,
}

/// The tools this can question. A reference is absent from this table when its
/// unknown-word answer is not distinguishable from acceptance — pasmo exits
/// zero and says nothing for a word it does not know, so there is no detector
/// to write and pretending otherwise would report its whole vocabulary as
/// covered.
const REFERENCES: &[Reference] = &[
    Reference {
        dialect: "acme",
        off_target: &[],
        mnemonic: |m| isa::mos6502::SET.has_mnemonic(m),
        bin: "acme",
        ext: "a",
        prologue: "* = $0000\n",
        sigil: "!",
        args: &["-f", "plain", "-o", "{out}", "{src}"],
        unknown: &["Unknown pseudo opcode", "Unknown command"],
        known: "!byte",
    },
    Reference {
        dialect: "sjasmplus",
        off_target: &["Illegal instruction"],
        mnemonic: |m| isa::z80::SET.has_mnemonic(m) || isa::z80::NEXT.has_mnemonic(m),
        bin: "sjasmplus",
        ext: "asm",
        prologue: "",
        sigil: "",
        args: &["--nologo", "--raw={out}", "{src}"],
        unknown: &["Unrecognized instruction"],
        known: "nop",
    },
    Reference {
        dialect: "lwasm",
        off_target: &["in 6809 mode"],
        mnemonic: |m| isa::mos6809::INSTRUCTION_SET.has_mnemonic(m),
        bin: "lwasm",
        ext: "asm",
        prologue: "",
        sigil: "",
        args: &["--6809", "--raw", "-o", "{out}", "{src}"],
        unknown: &["Bad opcode"],
        known: "nop",
    },
    Reference {
        dialect: "vasm",
        off_target: &["not supported on selected architecture"],
        mnemonic: |m| isa::m68k::SET.instruction(m).is_some(),
        bin: "vasmm68k_mot",
        ext: "s",
        prologue: "",
        sigil: "",
        args: &["-Fbin", "-no-opt", "-o", "{out}", "{src}"],
        unknown: &["unknown mnemonic", "unknown directive"],
        known: "nop",
    },
    Reference {
        dialect: "rgbasm",
        off_target: &[],
        mnemonic: |m| isa::sm83::SET.has_mnemonic(m),
        bin: "rgbasm",
        ext: "asm",
        prologue: "",
        sigil: "",
        args: &["-o", "{out}", "{src}"],
        unknown: &["Undefined macro"],
        known: "nop",
    },
    Reference {
        dialect: "ca65",
        off_target: &["is only valid in 65816 mode"],
        mnemonic: |m| isa::mos6502::SET.has_mnemonic(m),
        bin: "ca65",
        ext: "s",
        prologue: "",
        sigil: ".",
        args: &["-o", "{out}", "{src}"],
        unknown: &["is not a recognized control command"],
        known: ".byte",
    },
];

/// Report vocabulary coverage for every reference on this machine.
pub fn run(repo: &Path, write: bool) -> String {
    let tmp = std::env::temp_dir().join("asm198x-surface");
    let _ = std::fs::create_dir_all(&tmp);
    let mut out = String::new();
    let mut body = String::new();
    let mut total = 0usize;
    for r in REFERENCES {
        let Some(bin) = which(r.bin) else {
            let _ = writeln!(body, "\n## {} — not installed, not measured", r.dialect);
            continue;
        };
        if let Err(why) = self_check(r, &tmp) {
            let _ = writeln!(body, "\n## {} — detector unusable: {why}", r.dialect);
            continue;
        }
        let candidates = harvest(&bin);
        let mut known: Vec<String> = Vec::new();
        let mut off_target = 0usize;
        for c in &candidates {
            match recognises(r, &tmp, c) {
                Known::Yes => known.push(c.clone()),
                Known::OffTarget => off_target += 1,
                Known::No => {}
            }
        }
        let ours = our_spellings(r.dialect);
        // Fold case: every reference here matches its vocabulary
        // case-insensitively, so `!CPU` and `!cpu` are one word to it and one
        // entry here. Reporting both would count spellings and call them
        // features.
        let mut seen = BTreeSet::new();
        let uncovered: Vec<String> = known
            .iter()
            .filter(|w| !covered(r, &ours, w))
            .filter(|w| seen.insert(w.to_ascii_lowercase()))
            .map(|w| w.to_ascii_lowercase())
            .collect();
        total += uncovered.len();
        let _ = writeln!(
            body,
            "\n## {} ({})\n# {} candidate(s) harvested; {} recognised here, {} more the \
             tool itself\n# refuses on this target; {} of the {} outside our surface",
            r.dialect,
            identity(&bin, r.bin),
            candidates.len(),
            known.len(),
            off_target,
            uncovered.len(),
            known.len(),
        );
        for w in &uncovered {
            let _ = writeln!(body, "{}", spell(r, w));
        }
    }
    let _ = write!(
        out,
        "# Reference vocabulary coverage — what each tool accepts that we do not.\n\
         #\n\
         # {total} word(s) outside our surface, across the references installed\n\
         # here. A word, not a feature: a family of dotted spellings is often one\n\
         # rule, and one line here can be a week's work or an afternoon's.\n\
         #\n\
         # Words the tool refuses on the target we assemble for are counted\n\
         # separately and are NOT in that total. A 68881 mnemonic under\n\
         # -m68000, a 6309 one in 6809 mode, `.a16` outside 65816 mode: vasm,\n\
         # lwasm and ca65 all refuse those themselves, so they measure the\n\
         # width of a wider target rather than a gap in this one.\n\
         #\n\
         # What it misses: a spelling that differs from the harvested word by a\n\
         # character the harvest does not keep. sjasmplus takes an optional\n\
         # leading `.` on every directive, and a binary storing `db` yields\n\
         # `db` — so the ~30 dotted spellings it also accepts were invisible\n\
         # here until someone read the list and noticed the rule.\n\
         #\n\
         # What it still over-counts: a word that is vocabulary somewhere other\n\
         # than statement position. rgbasm knows `af` as a register and `acos`\n\
         # as an expression function, and both are offered here as operations,\n\
         # so both read as missing. Some of those are real (we have no `acos`)\n\
         # and some are not (we do take `af`). Reading a line here is still\n\
         # cheaper than finding it, which is the point.\n\
         #\n\
         # Regenerate with `cargo xtask surface --write` (needs the reference\n\
         # tools installed; it questions each one a few thousand times, so it\n\
         # takes minutes rather than seconds).\n\
         #\n\
         # A lower bound. Only words stored in the binary as literal bytes are\n\
         # found, and names are all this measures — whether a word *means* the\n\
         # same thing is the differential corpus's question, and the expensive\n\
         # half.\n"
    );
    out.push_str(&body);
    if write {
        let _ = std::fs::write(repo.join(STAMP), &out);
    }
    out
}

/// Prove the detector distinguishes a word this tool knows from one nothing
/// could, before any conclusion is drawn from it.
fn self_check(r: &Reference, tmp: &Path) -> Result<(), String> {
    if recognises(r, tmp, "zzqqxxvv") != Known::No {
        return Err(format!(
            "`{}` reports nothing recognisable for a nonsense word, so every \
             candidate would read as vocabulary",
            r.bin
        ));
    }
    if recognises(r, tmp, r.known) != Known::Yes {
        return Err(format!(
            "`{}` reads `{}` as unknown, so its own vocabulary would read as a gap",
            r.bin, r.known
        ));
    }
    Ok(())
}

/// What a reference makes of one word.
#[derive(PartialEq, Eq, Clone, Copy)]
enum Known {
    /// Not vocabulary at all.
    No,
    /// Vocabulary, and available on the target we assemble for.
    Yes,
    /// Vocabulary, and the tool itself refuses it here — a 68881 mnemonic
    /// under `-m68000`, a 6309 one in 6809 mode.
    OffTarget,
}

/// Does this reference know `word`? Anything but its unknown-word answer counts
/// — a word rejected for its *arguments* is a word the tool knows.
fn recognises(r: &Reference, tmp: &Path, word: &str) -> Known {
    let word = spell(r, word);
    let word = word.as_str();
    let src = tmp.join(format!("probe.{}", r.ext));
    let out = tmp.join("probe.out");
    if std::fs::write(&src, format!("{}\t{word}\n", r.prologue)).is_err() {
        return Known::No;
    }
    let mut c = Command::new(r.bin);
    for a in r.args {
        c.arg(
            a.replace("{src}", &src.to_string_lossy())
                .replace("{out}", &out.to_string_lossy()),
        );
    }
    let Ok(o) = c.current_dir(tmp).output() else {
        return Known::No;
    };
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    if r.unknown.iter().any(|u| text.contains(u)) {
        Known::No
    } else if r.off_target.iter().any(|u| text.contains(u)) {
        Known::OffTarget
    } else {
        Known::Yes
    }
}

/// How this reference spells `word` as an operation. ACME stores `byte` in its
/// binary and accepts it only as `!byte`, so the sigil is added here rather
/// than being expected of the harvest.
fn spell(r: &Reference, word: &str) -> String {
    if r.sigil.is_empty() || word.starts_with(r.sigil) {
        word.to_string()
    } else {
        format!("{}{word}", r.sigil)
    }
}

/// Every identifier-shaped run of printable bytes in the executable.
fn harvest(bin: &Path) -> Vec<String> {
    let Ok(bytes) = std::fs::read(bin) else {
        return Vec::new();
    };
    let mut words = BTreeSet::new();
    let mut cur = String::new();
    for &b in &bytes {
        let c = b as char;
        if c.is_ascii_alphanumeric() || c == '_' {
            cur.push(c);
        } else {
            take(&mut cur, &mut words);
        }
    }
    take(&mut cur, &mut words);
    words.into_iter().collect()
}

/// Keep a harvested run if it could be a word: at least two characters, not
/// digit-led. Short runs are noise at this scale, and every reference's
/// vocabulary is longer.
fn take(cur: &mut String, out: &mut BTreeSet<String>) {
    if cur.len() >= 2 && cur.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
        out.insert(std::mem::take(cur));
    } else {
        cur.clear();
    }
}

/// Do we take this word at all?
///
/// Three tests, because each covers the others' blind spots. The declared surface
/// alone misses every *mnemonic*, which on a sigil-less reference is most of
/// its vocabulary — vasm's 68000 opcodes would read as several hundred gaps.
/// Offering the word to our own assembler alone misses every directive that
/// needs arguments, because a bare `!for` is refused for having none.
///
/// So: declared, or not refused as an unknown instruction. Anything refused
/// for its operands is a word we know.
fn covered(r: &Reference, ours: &BTreeSet<String>, word: &str) -> bool {
    if ours.contains(&word.trim_start_matches(['.', '!']).to_ascii_lowercase()) {
        return true;
    }
    // A mnemonic our spec declares is one we take, whatever the assembler says
    // about it with no operands. It says different things: `move` answers "has
    // no form for those operands" and `adda` answers "unknown instruction",
    // which is a diagnostic worth fixing and not a gap worth reporting.
    let upper = word.to_ascii_uppercase();
    if (r.mnemonic)(&upper) {
        return true;
    }
    let Some(assemble) = assembler(r.dialect) else {
        return false;
    };
    match assemble(&format!("{}\t{}\n", r.prologue, spell(r, word))) {
        Ok(()) => true,
        Err(e) => {
            // Every way this project says "we do not have that word". A
            // dialect wording its refusal differently must be added here, or
            // its gaps read as coverage.
            ![
                "unknown instruction",
                "unsupported directive",
                "is not a directive",
                // The `KnownUnsupported` diagnostic. It is the *best* refusal
                // this project has — it tells the reader their source is valid
                // — and it is still a refusal. Reading it as coverage let a
                // dialect reach zero by describing its gaps well.
                "does not implement",
            ]
            .iter()
            .any(|m| e.contains(m))
        }
    }
}

type Assemble = fn(&str) -> Result<(), String>;

/// Our entry point for a dialect, reduced to accept-or-why-not.
fn assembler(dialect: &str) -> Option<Assemble> {
    fn wrap<T>(r: Result<T, asm198x::AsmError>) -> Result<(), String> {
        r.map(|_| ()).map_err(|e| e.to_string())
    }
    Some(match dialect {
        "acme" => |s: &str| wrap(asm198x::assemble_acme(s)),
        "sjasmplus" => |s: &str| wrap(asm198x::assemble_sjasmplus(s)),
        "vasm" => |s: &str| wrap(asm198x::assemble_vasm(s)),
        "rgbasm" => |s: &str| wrap(asm198x::assemble_rgbasm(s)),
        "ca65" => |s: &str| wrap(asm198x::assemble_ca65(s)),
        "lwasm" => |s: &str| wrap(asm198x::assemble_lwasm(s)),
        _ => return None,
    })
}

/// Every spelling our surface declares **and implements**, sigil-stripped and
/// lowercased, so a word is compared as a word.
///
/// Declared rather than tried: offering a bare `!for` to our own assembler
/// answers "unsupported directive" because it has no arguments, and counting
/// that as a gap would inflate every total with directives we implement.
///
/// `KnownUnsupported` is excluded, and that is the point of saying "and
/// implements". Declaring a directive we do not implement is how the
/// diagnostic tells a reader their source is valid — it is not how the gap
/// closes, and counting it as coverage let a dialect reach zero by describing
/// its gaps well.
fn our_spellings(dialect: &str) -> BTreeSet<String> {
    asm198x::directives::surfaces()
        .into_iter()
        .filter(|s| s.dialect == dialect)
        .flat_map(|s| s.directives)
        .filter(|d| d.category != asm198x::directives::Category::KnownUnsupported)
        .flat_map(|d| d.spellings())
        .map(|s| s.trim_start_matches(['.', '!']).to_ascii_lowercase())
        .collect()
}

/// The tool's own version line, so the stamp records what it measured against
/// — the verdict corpus's rule, for the same reason.
fn identity(bin: &Path, name: &str) -> String {
    for flag in ["--version", "-V", "-h"] {
        if let Ok(o) = Command::new(bin).arg(flag).output() {
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
            if let Some(line) = text.lines().find(|l| !l.trim().is_empty()) {
                return line.trim().to_string();
            }
        }
    }
    name.to_string()
}

/// The first `name` on `PATH`.
fn which(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")?
        .to_string_lossy()
        .split(':')
        .map(|d| Path::new(d).join(name))
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACME: Reference = Reference {
        dialect: "acme",
        mnemonic: |_| false,
        bin: "acme",
        ext: "a",
        prologue: "",
        sigil: "!",
        args: &[],
        unknown: &[],
        off_target: &[],
        known: "!byte",
    };

    /// A sigilled reference is asked about `!byte`, never `byte` — a bare word
    /// is a label there, so asking about it would measure something else.
    #[test]
    fn a_sigilled_reference_is_asked_with_its_sigil() {
        assert_eq!(spell(&ACME, "byte"), "!byte");
        assert_eq!(spell(&ACME, "!byte"), "!byte", "not doubled");
        let bare = Reference { sigil: "", ..ACME };
        assert_eq!(spell(&bare, "nop"), "nop");
    }

    /// The harvest keeps identifier-shaped runs and drops the rest, so a
    /// binary's noise does not become vocabulary to ask about.
    #[test]
    fn the_harvest_keeps_only_word_shaped_runs() {
        let mut out = BTreeSet::new();
        for word in ["ok", "a", "9bad", "_fine", "with9"] {
            let mut cur = word.to_string();
            take(&mut cur, &mut out);
        }
        assert!(out.contains("ok"));
        assert!(out.contains("_fine"));
        assert!(out.contains("with9"));
        assert!(!out.contains("a"), "one character is noise at this scale");
        assert!(!out.contains("9bad"), "a word does not start with a digit");
    }

    /// An off-target word is counted apart from a missing one, because it is
    /// not a gap: vasm refuses a 68881 mnemonic under `-m68000` itself.
    /// Conflating the two put **426 of vasm's 540** in a backlog they did not
    /// belong in, and nearly doubled the headline figure.
    #[test]
    fn a_word_the_tool_refuses_on_this_target_is_not_a_gap() {
        let vasm = REFERENCES
            .iter()
            .find(|r| r.dialect == "vasm")
            .expect("vasm is in the table");
        assert!(
            vasm.off_target
                .iter()
                .any(|m| "error 9: instruction not supported on selected architecture".contains(m)),
            "vasm's architecture refusal must be recognised as off-target"
        );
        assert!(
            !vasm
                .unknown
                .iter()
                .any(|m| "instruction not supported on selected architecture".contains(m)),
            "and must not also read as unknown, which would drop it entirely"
        );
    }
    /// A directive we declare is covered even though offering it bare would be
    /// refused for its arguments — the false positive this test exists to pin.
    #[test]
    fn a_declared_directive_needing_arguments_is_covered() {
        let ours = our_spellings("acme");
        assert!(ours.contains("for"), "acme declares `!for`");
        assert!(
            covered(&ACME, &ours, "for"),
            "`!for` is ours, though bare it is refused for having no arguments"
        );
    }

    /// A directive we *declare* but do not implement is still a gap.
    ///
    /// This one was live: declaring ca65's 97 control commands as
    /// `KnownUnsupported` took its count from 134 to **0**, because the
    /// declared surface was read as coverage and the improved diagnostic
    /// matched none of the refusal phrases. A dialect could reach zero by
    /// describing its gaps well, which is the exact opposite of what this
    /// measures.
    #[test]
    fn a_declared_gap_is_not_coverage() {
        let ours = our_spellings("ca65");
        assert!(
            ours.contains("byte"),
            "an implemented directive counts as covered"
        );
        assert!(
            !ours.contains("export"),
            "`.export` is declared and not implemented, so it is still a gap"
        );
    }
    /// The measured surface is not empty, and is keyed on `--dialect` names
    /// this build actually has — a table entry naming a dialect that no longer
    /// exists would silently measure nothing.
    #[test]
    fn every_reference_names_a_dialect_we_have() {
        let known: BTreeSet<&str> = asm198x::directives::surfaces()
            .iter()
            .map(|s| s.dialect)
            .collect();
        for r in REFERENCES {
            assert!(
                known.contains(r.dialect),
                "`{}` names no dialect in this build",
                r.dialect
            );
            assert!(
                !our_spellings(r.dialect).is_empty(),
                "`{}` has an empty declared surface",
                r.dialect
            );
        }
    }
}
