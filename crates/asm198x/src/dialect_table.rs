//! The dialect table: one list of what `--dialect` accepts, and what each one
//! is for.
//!
//! # Why this exists
//!
//! It replaces three hand-maintained copies of the same list — the `--dialect`
//! match arms, the `--help` text, and the CLI reference in the org docs repo —
//! two of which had already drifted. Both stale copies were missing the same
//! five dialects (`pdp11`, `tms9900`, `cp1610`, `z8000`, `z8001`), and the
//! reference additionally listed `8035`/`8039`/`8040` as *aliases* of `8048`
//! when they select a different instruction set: the ROM-less parts reject the
//! four BUS instructions, so a program that assembles as `8048` can fail as
//! `8035`. Calling them the same thing is not a typo, it is wrong.
//!
//! Nothing here decides behaviour. Resolution still matches on the canonical
//! name; this table is what maps every accepted spelling to that name, so a
//! dialect the table does not list cannot be selected, and one it lists but
//! resolution does not handle fails a test rather than a user.

/// One dialect, as the command line and the documentation present it.
pub struct Entry {
    /// The canonical spelling — what `--dialect` documents and what resolution
    /// matches on.
    pub name: &'static str,
    /// Other accepted spellings, in the order they should be documented.
    pub aliases: &'static [&'static str],
    /// What this dialect's syntax is for. One line: it is the cell in the CLI
    /// reference's table and the parenthetical in `--help`.
    pub blurb: &'static str,
}

/// Every dialect `--dialect` accepts.
///
/// Ordered as the reference presents them — the machine families people arrive
/// looking for first, then the rest by CPU lineage. Adding a dialect means
/// adding a row here; the tests below will not let it be selectable without one.
pub const DIALECTS: &[Entry] = &[
    Entry {
        name: "acme",
        aliases: &["6502", "mos6502"],
        blurb: "C64 6502, ACME syntax",
    },
    Entry {
        name: "ca65",
        aliases: &["nes"],
        blurb: "NES 6502, ca65 syntax (assemble + link)",
    },
    Entry {
        name: "65816",
        aliases: &["816", "ca65-816"],
        blurb: "65816, ca65 syntax",
    },
    Entry {
        name: "huc6280",
        aliases: &["pce", "pc-engine"],
        blurb: "PC Engine HuC6280, ca65 syntax",
    },
    Entry {
        name: "vasm",
        aliases: &["68000", "m68k", "mot"],
        blurb: "Amiga 68000, vasm Motorola syntax",
    },
    Entry {
        name: "lwasm",
        aliases: &["6809"],
        blurb: "6809, lwasm syntax",
    },
    Entry {
        name: "rgbasm",
        aliases: &["sm83", "gb", "gameboy", "game-boy"],
        blurb: "Game Boy SM83, RGBDS syntax",
    },
    Entry {
        name: "pasmo",
        aliases: &[],
        blurb: "Z80, pasmo syntax",
    },
    Entry {
        name: "pasmonext",
        aliases: &[],
        blurb: "Z80, pasmo syntax, Spectrum Next target by default",
    },
    Entry {
        name: "sjasmplus",
        aliases: &["sjasm"],
        blurb: "Z80, sjasmplus syntax",
    },
    Entry {
        name: "8080",
        aliases: &["i8080", "intel8080"],
        blurb: "Intel 8080, Intel syntax",
    },
    Entry {
        name: "6800",
        aliases: &["m6800"],
        blurb: "Motorola 6800, Motorola syntax",
    },
    Entry {
        name: "1802",
        aliases: &["cdp1802", "cosmac"],
        blurb: "RCA COSMAC CDP1802",
    },
    Entry {
        name: "8048",
        aliases: &["i8048", "mcs48", "mcs-48", "8049", "8050", "80c48", "80c49"],
        blurb: "MCS-48 with on-chip ROM",
    },
    Entry {
        // Not an alias of `8048`: the ROM-less parts reserve the bus for
        // external program memory, so `ins a,bus` and `outl bus,a` are refused
        // here and accepted there.
        name: "8035",
        aliases: &["8039", "8040", "80c35", "80c39", "80c40"],
        blurb: "MCS-48, ROM-less parts — the four BUS instructions are refused",
    },
    Entry {
        name: "scmp",
        aliases: &["sc/mp", "ins8060"],
        blurb: "National SC/MP (INS8060)",
    },
    Entry {
        name: "f8",
        aliases: &["3850", "f3850", "channelf", "channel-f"],
        blurb: "Fairchild F8 (3850), Channel F",
    },
    Entry {
        name: "2650",
        aliases: &["s2650", "signetics2650"],
        blurb: "Signetics 2650",
    },
    Entry {
        name: "tms7000",
        aliases: &["7000", "tms70c00"],
        blurb: "TI TMS7000",
    },
    Entry {
        name: "pdp11",
        aliases: &["pdp-11", "lsi11", "lsi-11"],
        blurb: "DEC PDP-11",
    },
    Entry {
        name: "tms9900",
        aliases: &["9900", "ti99"],
        blurb: "TI TMS9900 (TI-99/4A)",
    },
    Entry {
        name: "cp1610",
        aliases: &["cp1600", "cp-1600", "intellivision", "intv"],
        blurb: "GI CP1610 (Intellivision)",
    },
    Entry {
        name: "z8000",
        aliases: &["z8002"],
        blurb: "Zilog Z8000, non-segmented",
    },
    Entry {
        name: "z8001",
        aliases: &[],
        blurb: "Zilog Z8001, segmented",
    },
];

/// The canonical name for a spelling, matched case-insensitively as the command
/// line always has. `None` if nothing accepts it.
pub fn canonical(key: &str) -> Option<&'static str> {
    let key = key.to_ascii_lowercase();
    DIALECTS
        .iter()
        .find(|d| d.name == key || d.aliases.contains(&key.as_str()))
        .map(|d| d.name)
}

/// The dialect table as the CLI reference's markdown, so the page is generated
/// from the same list resolution uses rather than kept in step by hand.
pub fn markdown() -> String {
    let mut out = String::from("| Dialect | Syntax of | Also accepted |\n|---|---|---|\n");
    for entry in DIALECTS {
        let aliases = if entry.aliases.is_empty() {
            String::new()
        } else {
            entry
                .aliases
                .iter()
                .map(|a| format!("`{a}`"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        out.push_str(&format!(
            "| `{}` | {} | {} |\n",
            entry.name, entry.blurb, aliases
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A name may be spelled one way only. Two entries claiming the same
    /// spelling would make resolution depend on table order, which is exactly
    /// the ambiguity the table exists to remove.
    #[test]
    fn every_spelling_is_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for entry in DIALECTS {
            for spelling in std::iter::once(&entry.name).chain(entry.aliases) {
                assert!(
                    seen.insert(*spelling),
                    "`{spelling}` is claimed by more than one dialect"
                );
                assert_eq!(
                    *spelling,
                    spelling.to_ascii_lowercase(),
                    "`{spelling}` must be lower-case: lookup folds case"
                );
            }
        }
    }

    /// Every spelling resolves, and resolves to its own canonical name.
    #[test]
    fn every_spelling_resolves_to_its_canonical_name() {
        for entry in DIALECTS {
            assert_eq!(canonical(entry.name), Some(entry.name));
            for alias in entry.aliases {
                assert_eq!(
                    canonical(alias),
                    Some(entry.name),
                    "`{alias}` should resolve to `{}`",
                    entry.name
                );
            }
            // Case folding is part of the contract, not an accident of the
            // command line lower-casing before it asks.
            assert_eq!(
                canonical(&entry.name.to_ascii_uppercase()),
                Some(entry.name)
            );
        }
    }

    #[test]
    fn an_unknown_name_resolves_to_nothing() {
        assert_eq!(canonical("frobnicate"), None);
        assert_eq!(canonical(""), None);
    }

    /// The 8048 and the ROM-less parts are separate entries on purpose. The CLI
    /// reference called the second lot aliases of the first, which is wrong in a
    /// way that costs someone a confusing failure: the same source assembles as
    /// one and is refused as the other.
    #[test]
    fn the_rom_less_mcs48_parts_are_not_aliases_of_the_8048() {
        assert_eq!(canonical("8048"), Some("8048"));
        assert_eq!(canonical("8039"), Some("8035"));
        assert_ne!(canonical("8048"), canonical("8039"));
    }
}
