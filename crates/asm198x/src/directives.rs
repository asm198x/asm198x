//! What directives a dialect accepts, as data the parser dispatches from.
//!
//! A dialect's directive vocabulary used to exist only as `match` arms inside
//! its `parse_op`. That is enough to assemble and not enough to *read*: nothing
//! machine-readable said which directives a dialect accepts, so the surface
//! could not be documented without hand-maintaining a second copy — the drift
//! this project keeps paying for.
//!
//! It also made a gap invisible. `include` is unimplemented for pasmo, and the
//! only way anyone found out was assembling a multi-file project and reading
//! `unknown instruction INCLUDE`. A declared surface makes that a row you can
//! count rather than a hole you fall into.
//!
//! # The shape
//!
//! Each entry pairs a [`Pattern`] — how the spelling is written — with a
//! [`Category`] saying what the assembler does with it, and a stable id a
//! generator can key on. Dispatch goes through [`lookup`], so a spelling the
//! declaration does not contain cannot be accepted.
//!
//! Scope is deliberately spelling-level: which directives exist, in which
//! dialect, in which category. What they *mean* stays in the arm bodies.

/// What the assembler does with a directive it accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    /// It produces an [`crate::contract::Operation`].
    Operation,
    /// The reference assembler accepts it, it changes no bytes, and we accept
    /// and discard it so source carrying it still assembles.
    Ignored,
    /// The reference assembler accepts it and we do not implement it.
    ///
    /// Refusing is the only honest answer — the alternative is assembling
    /// something the source did not ask for — but refusing *as an unknown
    /// mnemonic* tells the reader their source is invalid, when it is not.
    /// This category exists so the diagnostic can say which it is, and so the
    /// gap is countable rather than discoverable by accident.
    KnownUnsupported,
}

/// How a directive is spelled.
#[derive(Clone, Copy, Debug)]
pub enum Pattern {
    /// One or more literal spellings, matched case-insensitively.
    Exact(&'static [&'static str]),
    /// A stem carrying an optional size suffix: vasm's `dc`, `dc.b`, `dc.w`.
    ///
    /// Matching recognises the **stem**, not the suffix, and the arm body
    /// validates the size. That keeps `dc.x` reporting `bad data size` instead
    /// of falling through to be refused as an unknown mnemonic, which is a
    /// worse answer to the same mistake.
    ///
    /// It also retires an ordering constraint. Dispatch used `strip_prefix`,
    /// so `dcb` had to be tried before `dc` or every `dcb` parsed as a `dc`
    /// with a nonsense size. Splitting at the separator makes `dcb.w` yield the
    /// stem `dcb`, which only one entry claims — so the entries can be declared
    /// in any order and the matching is what keeps them apart.
    /// A name behind a sigil: acme's `!byte`, ca65's `.byte`, sjasmplus's
    /// `.if`.
    ///
    /// `required` is the whole point, and it is a fact about the dialect
    /// rather than a house style. Probed 2026-08-21 against the reference
    /// tools:
    ///
    /// | Dialect | Sigilled | Bare |
    /// |---|---|---|
    /// | acme | `!byte` | refused — read as a label |
    /// | ca65 | `.byte` | refused — read as a label |
    /// | sjasmplus | `.if` | accepted |
    ///
    /// Stripping the sigil everywhere would not merely accept an extra
    /// spelling in acme and ca65: a bare `byte` is a valid *label definition*
    /// there, so accepting it as a directive changes what a label means. Real
    /// acme answers "Label name not in leftmost column" and real ca65 answers
    /// "':' expected". Carrying the sigil in every spelling instead would
    /// misdescribe sjasmplus, which takes both forms.
    ///
    /// So the sigil is declared, with its optionality, and both facts are
    /// testable rather than accidents of which branch a parser took.
    Sigilled {
        sigil: char,
        names: &'static [&'static str],
        /// Whether the sigil must be present. `false` means the bare name is
        /// also valid — which is true of sjasmplus and of neither other.
        required: bool,
    },
    Sized {
        stem: &'static str,
        /// The character introducing the suffix.
        separator: char,
        /// The sizes the dialect documents, for a generator. Not enforced
        /// here — see above.
        sizes: &'static [&'static str],
        /// Whether the bare stem is a valid spelling on its own.
        bare: bool,
    },
}

/// One declared directive.
#[derive(Clone, Copy, Debug)]
pub struct Directive {
    /// Stable across spellings and dialects, for a generator to key on. Not
    /// the spelling: two dialects can spell one concept differently, and one
    /// dialect can spell it several ways.
    pub id: &'static str,
    pub pattern: Pattern,
    pub category: Category,
}

impl Directive {
    /// Every spelling this entry accepts, expanded, for a generator (R7).
    ///
    /// Allocates, and is not on the assembling path — [`Self::matches`] is.
    #[must_use]
    pub fn spellings(&self) -> Vec<String> {
        match self.pattern {
            Pattern::Exact(list) => list.iter().map(|s| (*s).to_string()).collect(),
            Pattern::Sigilled {
                sigil,
                names,
                required,
            } => {
                let mut out: Vec<String> = names.iter().map(|n| format!("{sigil}{n}")).collect();
                if !required {
                    out.extend(names.iter().map(|n| (*n).to_string()));
                }
                out
            }
            Pattern::Sized {
                stem,
                separator,
                sizes,
                bare,
            } => {
                let mut out = Vec::with_capacity(sizes.len() + usize::from(bare));
                if bare {
                    out.push(stem.to_string());
                }
                out.extend(sizes.iter().map(|s| format!("{stem}{separator}{s}")));
                out
            }
        }
    }

    /// Whether `word` names this entry. Case-insensitive: no dialect here
    /// distinguishes `DB` from `db`.
    #[must_use]
    pub fn matches(&self, word: &str) -> bool {
        match self.pattern {
            Pattern::Exact(list) => list.iter().any(|s| s.eq_ignore_ascii_case(word)),
            Pattern::Sigilled {
                sigil,
                names,
                required,
            } => match word.strip_prefix(sigil) {
                Some(name) => names.iter().any(|n| n.eq_ignore_ascii_case(name)),
                None => !required && names.iter().any(|n| n.eq_ignore_ascii_case(word)),
            },
            Pattern::Sized {
                stem,
                separator,
                bare,
                ..
            } => match word.split_once(separator) {
                Some((head, _)) => head.eq_ignore_ascii_case(stem),
                None => bare && word.eq_ignore_ascii_case(stem),
            },
        }
    }
}

/// Find the entry a word names, if the surface declares one.
///
/// Returns the whole entry rather than its id, so a caller can dispatch on the
/// id and answer for the category in the same match — a `KnownUnsupported`
/// spelling needs to be told apart from an unknown one at the point of refusal.
#[must_use]
pub fn lookup(surface: &'static [Directive], word: &str) -> Option<&'static Directive> {
    surface.iter().find(|d| d.matches(word))
}

/// One dialect's declared surface, named.
pub struct DialectSurface {
    /// The `--dialect` name this surface belongs to.
    pub dialect: &'static str,
    pub directives: &'static [Directive],
}

/// Every dialect that dispatches through a declaration (R7).
///
/// This is the accessor a documentation generator reads. A dialect absent from
/// it has not been converted yet — which is a fact worth being able to state,
/// rather than one to discover by finding no row for it.
#[must_use]
pub fn surfaces() -> &'static [DialectSurface] {
    use crate::dialects;
    &[
        DialectSurface {
            dialect: "vasm",
            directives: dialects::vasm::DIRECTIVES,
        },
        DialectSurface {
            dialect: "1802",
            directives: dialects::cdp1802::DIRECTIVES,
        },
        DialectSurface {
            dialect: "8080",
            directives: dialects::i8080::DIRECTIVES,
        },
        DialectSurface {
            dialect: "6800",
            directives: dialects::m6800::DIRECTIVES,
        },
        DialectSurface {
            dialect: "8048",
            directives: dialects::i8048::DIRECTIVES,
        },
        DialectSurface {
            dialect: "scmp",
            directives: dialects::scmp::DIRECTIVES,
        },
        DialectSurface {
            dialect: "2650",
            directives: dialects::s2650::DIRECTIVES,
        },
        DialectSurface {
            dialect: "tms7000",
            directives: dialects::tms7000::DIRECTIVES,
        },
        DialectSurface {
            dialect: "f8",
            directives: dialects::f8::DIRECTIVES,
        },
        DialectSurface {
            dialect: "cp1610",
            directives: dialects::cp1610::DIRECTIVES,
        },
        DialectSurface {
            dialect: "pdp11",
            directives: dialects::pdp11::DIRECTIVES,
        },
        DialectSurface {
            dialect: "tms9900",
            directives: dialects::tms9900::DIRECTIVES,
        },
        DialectSurface {
            dialect: "z8000",
            directives: dialects::z8000::DIRECTIVES,
        },
    ]
}

#[cfg(test)]
mod surface_invariants {
    //! Holds across every converted dialect, so a new one cannot land broken.

    use super::{Category, surfaces};

    #[test]
    fn ids_are_unique_within_a_dialect() {
        for surface in surfaces() {
            let mut ids: Vec<&str> = surface.directives.iter().map(|d| d.id).collect();
            ids.sort_unstable();
            let before = ids.len();
            ids.dedup();
            assert_eq!(before, ids.len(), "{} has a duplicate id", surface.dialect);
        }
    }

    #[test]
    fn no_spelling_is_claimed_twice_within_a_dialect() {
        // Two entries answering to one word means the first wins silently,
        // which is the ordering bug `Sized` matching was built to retire.
        for surface in surfaces() {
            let mut seen: Vec<String> = Vec::new();
            for directive in surface.directives {
                for spelling in directive.spellings() {
                    let lower = spelling.to_ascii_lowercase();
                    assert!(
                        !seen.contains(&lower),
                        "{}: `{spelling}` is claimed by more than one entry",
                        surface.dialect
                    );
                    seen.push(lower);
                }
            }
        }
    }

    #[test]
    fn every_entry_has_at_least_one_spelling() {
        for surface in surfaces() {
            for directive in surface.directives {
                assert!(
                    !directive.spellings().is_empty(),
                    "{}: `{}` declares no spelling",
                    surface.dialect,
                    directive.id
                );
            }
        }
    }

    #[test]
    fn a_declared_spelling_finds_its_own_entry() {
        for surface in surfaces() {
            for directive in surface.directives {
                for spelling in &directive.spellings() {
                    assert_eq!(
                        super::lookup(surface.directives, spelling).map(|d| d.id),
                        Some(directive.id),
                        "{}: `{spelling}` should reach `{}`",
                        surface.dialect,
                        directive.id
                    );
                }
            }
        }
    }

    #[test]
    fn nothing_is_declared_unsupported_yet() {
        // The category exists for pasmo's include and asl's semantic
        // pseudo-ops. Neither is declared yet, so this records that the count
        // is zero rather than leaving it unstated.
        let count = surfaces()
            .iter()
            .flat_map(|s| s.directives.iter())
            .filter(|d| d.category == Category::KnownUnsupported)
            .count();
        assert_eq!(count, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SURFACE: &[Directive] = &[
        Directive {
            id: "org",
            pattern: Pattern::Exact(&["org"]),
            category: Category::Operation,
        },
        Directive {
            id: "bytes",
            pattern: Pattern::Exact(&["db", "dc", "byte"]),
            category: Category::Operation,
        },
        Directive {
            id: "listing",
            pattern: Pattern::Exact(&["listing"]),
            category: Category::Ignored,
        },
    ];

    #[test]
    fn a_declared_spelling_is_found() {
        assert_eq!(lookup(SURFACE, "org").map(|d| d.id), Some("org"));
    }

    #[test]
    fn every_spelling_of_an_entry_finds_it() {
        for word in ["db", "dc", "byte"] {
            assert_eq!(lookup(SURFACE, word).map(|d| d.id), Some("bytes"));
        }
    }

    #[test]
    fn matching_ignores_case() {
        assert_eq!(lookup(SURFACE, "ORG").map(|d| d.id), Some("org"));
        assert_eq!(lookup(SURFACE, "Byte").map(|d| d.id), Some("bytes"));
    }

    #[test]
    fn an_undeclared_spelling_is_not_found() {
        assert!(lookup(SURFACE, "include").is_none());
    }

    #[test]
    fn the_category_travels_with_the_entry() {
        assert_eq!(
            lookup(SURFACE, "listing").map(|d| d.category),
            Some(Category::Ignored)
        );
    }

    #[test]
    fn spellings_are_reachable_for_a_generator() {
        let entry = lookup(SURFACE, "dc").expect("declared");
        assert_eq!(entry.spellings(), vec!["db", "dc", "byte"]);
    }

    const SIZED: &[Directive] = &[
        // Declared `dc` first on purpose: with `strip_prefix` dispatch this
        // order was a bug, and matching on the stem is what makes it not one.
        Directive {
            id: "dc",
            pattern: Pattern::Sized {
                stem: "dc",
                separator: '.',
                sizes: &["b", "w", "l"],
                bare: true,
            },
            category: Category::Operation,
        },
        Directive {
            id: "dcb",
            pattern: Pattern::Sized {
                stem: "dcb",
                separator: '.',
                sizes: &["b", "w", "l"],
                bare: true,
            },
            category: Category::Operation,
        },
    ];

    #[test]
    fn a_sized_stem_matches_bare_and_suffixed() {
        for word in ["dc", "dc.b", "dc.w", "dc.l", "DC.W"] {
            assert_eq!(lookup(SIZED, word).map(|d| d.id), Some("dc"), "{word}");
        }
    }

    #[test]
    fn a_longer_stem_is_not_swallowed_by_a_shorter_one() {
        // The ordering constraint the old dispatch carried, as a property of
        // matching: `dcb` is declared second and still wins its own spellings.
        for word in ["dcb", "dcb.w", "dcb.l"] {
            assert_eq!(lookup(SIZED, word).map(|d| d.id), Some("dcb"), "{word}");
        }
    }

    #[test]
    fn an_unknown_size_still_reaches_its_stem() {
        // So the arm body can say `bad data size`, rather than the word
        // falling through to be refused as an unknown mnemonic.
        assert_eq!(lookup(SIZED, "dc.x").map(|d| d.id), Some("dc"));
        assert_eq!(lookup(SIZED, "dcb.x").map(|d| d.id), Some("dcb"));
    }

    #[test]
    fn sized_spellings_expand_for_a_generator() {
        let entry = lookup(SIZED, "dc").expect("declared");
        assert_eq!(entry.spellings(), vec!["dc", "dc.b", "dc.w", "dc.l"]);
    }

    const REQUIRED: &[Directive] = &[Directive {
        id: "byte",
        pattern: Pattern::Sigilled {
            sigil: '!',
            names: &["byte", "by"],
            required: true,
        },
        category: Category::Operation,
    }];

    const OPTIONAL: &[Directive] = &[Directive {
        id: "if",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["if"],
            required: false,
        },
        category: Category::Operation,
    }];

    #[test]
    fn a_required_sigil_is_required() {
        // acme and ca65: a bare `byte` is a label definition, not a directive.
        // Accepting it here would change what a label means.
        assert_eq!(lookup(REQUIRED, "!byte").map(|d| d.id), Some("byte"));
        assert_eq!(lookup(REQUIRED, "!by").map(|d| d.id), Some("byte"));
        assert!(lookup(REQUIRED, "byte").is_none());
        assert!(lookup(REQUIRED, "by").is_none());
    }

    #[test]
    fn an_optional_sigil_takes_both_forms() {
        // sjasmplus takes `if` and `.if`, and the reference does too.
        assert_eq!(lookup(OPTIONAL, ".if").map(|d| d.id), Some("if"));
        assert_eq!(lookup(OPTIONAL, "if").map(|d| d.id), Some("if"));
    }

    #[test]
    fn a_sigil_does_not_make_an_undeclared_name_valid() {
        assert!(lookup(REQUIRED, "!nonsense").is_none());
        assert!(lookup(OPTIONAL, ".nonsense").is_none());
    }

    #[test]
    fn sigilled_matching_ignores_case() {
        assert_eq!(lookup(REQUIRED, "!BYTE").map(|d| d.id), Some("byte"));
        assert_eq!(lookup(OPTIONAL, "IF").map(|d| d.id), Some("if"));
    }

    #[test]
    fn sigilled_spellings_expand_by_optionality() {
        let required = lookup(REQUIRED, "!byte").expect("declared");
        assert_eq!(required.spellings(), vec!["!byte", "!by"]);

        // The bare form appears only where it is genuinely valid, so a matrix
        // built from this cannot claim acme takes `byte`.
        let optional = lookup(OPTIONAL, ".if").expect("declared");
        assert_eq!(optional.spellings(), vec![".if", "if"]);
    }

    #[test]
    fn a_stem_without_bare_needs_its_suffix() {
        const NO_BARE: &[Directive] = &[Directive {
            id: "sized",
            pattern: Pattern::Sized {
                stem: "xx",
                separator: '.',
                sizes: &["b"],
                bare: false,
            },
            category: Category::Operation,
        }];
        assert!(lookup(NO_BARE, "xx").is_none());
        assert!(lookup(NO_BARE, "xx.b").is_some());
    }
}
