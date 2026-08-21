//! `cargo xtask machines --check` — hold the copied CPU→machine mapping to the
//! library it came from.
//!
//! `isa::machines::MACHINES` records which machines used each CPU, so the
//! instruction reference can link back to the hardware. That mapping lives in
//! the umbrella reference library, which is a **private repository** — this
//! workspace cannot read it in CI, so the table here is a copy.
//!
//! A copy with nothing checking it is a copy that drifts. This is the check:
//! run it with the umbrella checked out and it compares the two, naming every
//! difference. Run it without, and it says so and stops rather than passing
//! vacuously — a check that silently succeeds when it cannot see its subject is
//! worse than no check, because it reports confidence it does not have.

use std::path::{Path, PathBuf};

/// Where the reference library sits relative to this workspace, when it is
/// present at all: `198x/reference/`, three levels up from `Asm198x/asm198x`.
fn library(repo: &Path) -> PathBuf {
    repo.join("../../reference/by-topic")
}

/// Which chip folders in the library hold each `isa` module's reference.
///
/// The two name things differently — `mos6502` against `cpu-6502` — and neither
/// is wrong, so the mapping is stated rather than derived from a guess about
/// prefixes.
///
/// A module can draw from more than one folder, because the library is keyed on
/// the **chip** and this table is keyed on the **instruction set**. The 6510 is
/// a 6502 with an I/O port and no new opcodes, so a C64 author assembles
/// against `mos6502`; filing the C64 under `cpu-6510` is right about the
/// hardware and left the machine on no instruction page at all. The union is
/// how both stay true at once.
const CHIP_FOLDERS: &[(&str, &[&str])] = &[
    ("mos6502", &["cpu-6502", "cpu-6510"]),
    ("mos65816", &["cpu-65816"]),
    ("huc6280", &["cpu-huc6280"]),
    ("z80", &["cpu-z80"]),
    ("sm83", &["cpu-sm83"]),
    ("i8080", &["cpu-8080"]),
    ("m6800", &["cpu-6800"]),
    ("cdp1802", &["cpu-cdp1802"]),
    ("i8048", &["cpu-8048"]),
    ("scmp", &["cpu-scmp"]),
    ("f8", &["cpu-f8"]),
    ("s2650", &["cpu-2650"]),
    ("tms7000", &["cpu-tms7000"]),
    ("tms9900", &["cpu-tms9900"]),
    ("pdp11", &["cpu-pdp11"]),
    ("cp1610", &["cpu-cp1610"]),
    ("m68k", &["cpu-68000"]),
    ("mos6809", &["cpu-6809"]),
    ("z8000", &["cpu-z8000"]),
];

/// What the reference library states about one CPU's machines.
enum Library {
    /// A `systems:` line, carrying zero or more slugs.
    Records(Vec<String>),
    /// No `systems:` line at all.
    ///
    /// The library holds two frontmatter shapes: the older migrated references
    /// carry `chip:`/`systems:`, the newer distilled ones carry `title:`/
    /// `sources:` and no machine list. Silence therefore means the library has
    /// not stated the mapping — which is not the same as stating it is empty,
    /// and conflating the two would let a copied claim sit over a source that
    /// never made it.
    Silent,
}

/// Read the `systems:` frontmatter line from one chip's reference.
///
/// # Errors
/// If the file is not readable. The folder mapping names it, so an unreadable
/// file is a broken mapping rather than a missing machine list.
fn systems_in(path: &Path) -> Result<Library, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let Some(line) = text.lines().find(|l| l.starts_with("systems:")) else {
        return Ok(Library::Silent);
    };
    let Some(inside) = line.split_once('[').and_then(|(_, r)| r.split_once(']')) else {
        return Err(format!("malformed `systems:` line in {}", path.display()));
    };
    Ok(Library::Records(
        inside
            .0
            .split(',')
            .map(|s| s.trim().trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .collect(),
    ))
}

/// A machine list is a set, not a sequence: the library states which machines
/// used a chip, and nothing depends on the order it happens to list them in.
/// Comparing sorted and deduplicated leaves `isa` free to order for reading —
/// and a machine that two chip folders both claim is one machine, not two.
fn normalise(slugs: impl Iterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = slugs.collect();
    out.sort();
    out.dedup();
    out
}

fn join_or_none(slugs: &[String]) -> String {
    if slugs.is_empty() {
        "(none)".to_string()
    } else {
        slugs.join(", ")
    }
}

/// Compare the copied table against the library.
///
/// # Errors
/// If the library is not present — the caller cannot tell "agrees" from
/// "could not look" otherwise, and that difference is the whole point.
pub fn check(repo: &Path) -> Result<Vec<String>, String> {
    let root = library(repo);
    if !root.is_dir() {
        return Err(format!(
            "the reference library is not at {}\n\n\
             This check compares the copied CPU→machine mapping against the \
             umbrella library, so it needs that library checked out beside this \
             workspace. It is a private repository and CI does not have it, \
             which is why the mapping is a copy in the first place.",
            root.display()
        ));
    }

    let mut differences = Vec::new();
    for (module, folders) in CHIP_FOLDERS {
        let ours = normalise(
            isa::machines::machines_for(module)
                .iter()
                .map(|m| m.slug.to_string()),
        );

        // A module's machines are the union across its chip folders, and a
        // folder that states no `systems:` contributes nothing rather than
        // asserting emptiness.
        let mut library_says: Vec<String> = Vec::new();
        let mut stated = false;
        let mut paths = Vec::new();
        for folder in *folders {
            let path = root.join(folder).join(format!("{folder}-reference.md"));
            paths.push(path.display().to_string());
            if let Library::Records(slugs) = systems_in(&path)? {
                stated = true;
                library_says.extend(slugs);
            }
        }
        let library_says = normalise(library_says.into_iter());

        if !stated {
            if !ours.is_empty() {
                differences.push(format!(
                    "{module}: the copy names {} but {} states no `systems:`",
                    join_or_none(&ours),
                    paths.join(", ")
                ));
            }
            continue;
        }
        if library_says != ours {
            differences.push(format!(
                "{module}:\n      library: {}\n      copied:  {}",
                join_or_none(&library_says),
                join_or_none(&ours)
            ));
        }
    }

    Ok(differences)
}
