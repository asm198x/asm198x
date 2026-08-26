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
    /// It is part of an **expression**, not a statement — ca65's `.lobyte(x)`,
    /// `.max(a,b)`, `.strlen("s")`.
    ///
    /// Declared here because it is vocabulary the reference has and we
    /// implement, and the ledger counts what is declared: leaving it out made
    /// nine working ca65 words read as gaps. But it never begins a line, so
    /// the directive dispatch never sees one and the declared-vs-dispatched
    /// invariant does not apply to it. Its own invariant does: every one of
    /// these parses *inside an expression*, which
    /// `an_expression_word_parses_where_it_belongs` checks.
    ExpressionWord,
    /// The reference assembler accepts it and we do not implement it.
    ///
    /// Refusing is the only honest answer — the alternative is assembling
    /// something the source did not ask for — but refusing *as an unknown
    /// mnemonic* tells the reader their source is invalid, when it is not.
    /// This category exists so the diagnostic can say which it is, and so the
    /// gap is countable rather than discoverable by accident.
    KnownUnsupported,
    /// The reference assembler has it and **refuses it itself**. The payload
    /// is its rule, phrased to read in a sentence: `"only supported for an
    /// object target, and asm198x emits a binary"`.
    ///
    /// The refusal need not be conditional on our output. It often is — a
    /// word that needs a linker, given that we emit a binary — and then the
    /// rule says so. But a reference also refuses words it has **retired**:
    /// ACME 0.97 answers `"!cbm" is obsolete; use "!ct pet" instead` however
    /// it is invoked. Both are the same fact for this category's purpose:
    /// the reference will not take the word, so neither may we.
    ///
    /// This is the opposite of [`KnownUnsupported`](Category::KnownUnsupported)
    /// and reads almost the same to someone skimming the source, which is why
    /// it is a category rather than a comment. `KnownUnsupported` says the
    /// source is valid and we are behind; this says the source is not valid
    /// for the output asked for, and refusing it *is* matching the reference.
    ///
    /// So these words are **covered**, not outstanding: `xtask surface` counts
    /// them as ours, because assembling them would be the divergence.
    ///
    /// Governed by `decisions/symbol-visibility-in-a-fused-assembler.md`.
    ///
    /// lwtools 4.25 answers `Only supported for object target (EXPORT)` for
    /// `export`, `extdep`, `extern`, `external` and `import` under `--raw`,
    /// with an operand and without (probed 2026-08-24). asm198x emits a
    /// binary, never an object file, so that is every path we have.
    RefusedByReference(&'static str),
}

/// What to say when a word that belongs *inside an expression* is written
/// where a statement goes. Never reachable from a dialect that declares none,
/// but every line-start dispatch matches on the category, so each needs an
/// answer and they may as well give the same one.
pub(crate) fn not_a_statement(word: &str) -> String {
    format!("`{word}` belongs inside an expression here, not at the start of a line")
}

/// What a [`Category::RefusedByReference`] word owes the reader: the reference's
/// rule, and that the refusal is the reference's rather than a gap here. Kept
/// in one place so thirteen dispatch arms cannot drift into thirteen wordings.
#[must_use]
pub fn refused_by_reference(tool: &str, spelling: &str, rule: &str) -> String {
    format!(
        "`{spelling}` is {rule} — {tool} refuses it too, so this is not a gap \
         in asm198x"
    )
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
pub fn lookup<'a>(surface: &'a [Directive], word: &str) -> Option<&'a Directive> {
    surface.iter().find(|d| d.matches(word))
}

/// One dialect's declared surface, named.
pub struct DialectSurface {
    /// The `--dialect` name this surface belongs to.
    pub dialect: &'static str,
    pub directives: Vec<Directive>,
}

/// Compose a dialect's surface from a shared base plus its own entries.
///
/// An entry in `own` with the same id as one in `base` **replaces** it, which
/// is how sjasmplus adds `byte` to the `db` family without the base and the
/// dialect both claiming a spelling. Anything in `base` the dialect does not
/// override is carried through.
///
/// This is why a surface is owned rather than borrowed: pasmo and sjasmplus
/// share most of a vocabulary and differ in exactly the way that matters, and
/// a generator needs to see each dialect's own answer rather than the base's.
#[must_use]
pub fn compose(base: &[Directive], own: &[Directive]) -> Vec<Directive> {
    let overridden: Vec<&str> = own.iter().map(|d| d.id).collect();
    base.iter()
        .filter(|d| !overridden.contains(&d.id))
        .chain(own.iter())
        .copied()
        .collect()
}

/// Every dialect that dispatches through a declaration (R7).
///
/// This is the accessor a documentation generator reads. A dialect absent from
/// it has not been converted yet — which is a fact worth being able to state,
/// rather than one to discover by finding no row for it.
#[must_use]
/// The asl family's shared base: the multi-file pair plus the chip-independent
/// directives #87 decided. Composed into every asl chip's surface, so a page
/// or a coverage report sees the same list the parser dispatches through.
fn asl_family() -> Vec<Directive> {
    compose(
        crate::dialects::asl::WALK_DIRECTIVES,
        crate::dialects::asl::SEMANTIC_DIRECTIVES,
    )
}

/// Rewrite a surface so every `Exact` spelling also accepts a leading `.`.
///
/// sjasmplus takes one on every directive it has — `.db`, `.org`, `.module`,
/// `.equ` — and the conditionals already declared it that way. Applying the
/// rule to the composed surface says it once, rather than restating thirty
/// spellings so a dialect can add a dot to each.
///
/// A `Sigilled` entry is left alone: it already carries whichever sigil rule it
/// meant.
fn optional_dot(dirs: Vec<Directive>) -> Vec<Directive> {
    dirs.into_iter()
        .map(|d| match d.pattern {
            Pattern::Exact(names) => Directive {
                pattern: Pattern::Sigilled {
                    sigil: '.',
                    names,
                    required: false,
                },
                ..d
            },
            _ => d,
        })
        .collect()
}

pub fn surfaces() -> Vec<DialectSurface> {
    use crate::dialects;
    vec![
        DialectSurface {
            dialect: "pasmo",
            directives: compose(
                dialects::z80::COMMON_DIRECTIVES,
                dialects::pasmo::DIRECTIVES,
            ),
        },
        DialectSurface {
            dialect: "sjasmplus",
            directives: optional_dot(compose(
                dialects::z80::COMMON_DIRECTIVES,
                dialects::sjasmplus::DIRECTIVES,
            )),
        },
        DialectSurface {
            dialect: "rgbasm",
            directives: dialects::rgbasm::DIRECTIVES.to_vec(),
        },
        DialectSurface {
            dialect: "ca65",
            directives: dialects::ca65::DIRECTIVES.to_vec(),
        },
        DialectSurface {
            dialect: "65816",
            directives: dialects::ca65_816::DIRECTIVES.to_vec(),
        },
        DialectSurface {
            dialect: "huc6280",
            directives: dialects::ca65_huc6280::DIRECTIVES.to_vec(),
        },
        DialectSurface {
            dialect: "acme",
            directives: dialects::acme::DIRECTIVES.to_vec(),
        },
        DialectSurface {
            dialect: "lwasm",
            directives: dialects::lwasm::DIRECTIVES.to_vec(),
        },
        DialectSurface {
            dialect: "vasm",
            directives: dialects::vasm::DIRECTIVES.to_vec(),
        },
        DialectSurface {
            dialect: "1802",
            directives: compose(&asl_family(), dialects::cdp1802::DIRECTIVES),
        },
        DialectSurface {
            dialect: "8080",
            directives: compose(&asl_family(), dialects::i8080::DIRECTIVES),
        },
        DialectSurface {
            dialect: "6800",
            directives: compose(&asl_family(), dialects::m6800::DIRECTIVES),
        },
        DialectSurface {
            dialect: "8048",
            directives: compose(&asl_family(), dialects::i8048::DIRECTIVES),
        },
        DialectSurface {
            dialect: "scmp",
            directives: compose(&asl_family(), dialects::scmp::DIRECTIVES),
        },
        DialectSurface {
            dialect: "2650",
            directives: compose(&asl_family(), dialects::s2650::DIRECTIVES),
        },
        DialectSurface {
            dialect: "tms7000",
            directives: compose(&asl_family(), dialects::tms7000::DIRECTIVES),
        },
        DialectSurface {
            dialect: "f8",
            directives: compose(&asl_family(), dialects::f8::DIRECTIVES),
        },
        DialectSurface {
            dialect: "cp1610",
            directives: compose(&asl_family(), dialects::cp1610::DIRECTIVES),
        },
        DialectSurface {
            dialect: "pdp11",
            directives: compose(&asl_family(), dialects::pdp11::DIRECTIVES),
        },
        DialectSurface {
            dialect: "tms9900",
            directives: compose(&asl_family(), dialects::tms9900::DIRECTIVES),
        },
        DialectSurface {
            dialect: "z8000",
            directives: compose(&asl_family(), dialects::z8000::DIRECTIVES),
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
            for directive in &surface.directives {
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
            for directive in &surface.directives {
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
            for directive in &surface.directives {
                for spelling in &directive.spellings() {
                    assert_eq!(
                        super::lookup(&surface.directives, spelling).map(|d| d.id),
                        Some(directive.id),
                        "{}: `{spelling}` should reach `{}`",
                        surface.dialect,
                        directive.id
                    );
                }
            }
        }
    }

    /// The difference this whole surface exists to make visible.
    ///
    /// sjasmplus takes `include`. pasmo has one and asm198x does not implement
    /// it, so a multi-file pasmo project does not assemble — and the two facts
    /// are now told apart by **category** rather than by one dialect having a
    /// row and the other having nothing.
    ///
    /// That distinction is the point. An absent row says "this dialect has no
    /// such directive"; a `KnownUnsupported` row says "it has one and we do not
    /// implement it", which is what is true here and what a reader can act on.
    /// Implementing it means changing this category, and this test is what
    /// makes someone do that.
    #[test]
    fn the_two_z80_dialects_differ_where_they_actually_differ() {
        let of = |name: &str| {
            surfaces()
                .into_iter()
                .find(|s| s.dialect == name)
                .unwrap_or_else(|| panic!("`{name}` has a declared surface"))
        };
        let pasmo = of("pasmo");
        let sjasmplus = of("sjasmplus");

        assert_eq!(
            super::lookup(&sjasmplus.directives, "include").map(|d| d.category),
            Some(Category::Operation),
            "sjasmplus implements `include`"
        );
        assert_eq!(
            super::lookup(&pasmo.directives, "include").map(|d| d.category),
            Some(Category::KnownUnsupported),
            "pasmo has `include` and we do not implement it — if this now says \
             Operation, the row and the code have parted company"
        );

        // Both take incbin, and both implement it, so the difference is the
        // include and not file inclusion in general.
        for surface in [&pasmo, &sjasmplus] {
            assert_eq!(
                super::lookup(&surface.directives, "incbin").map(|d| d.category),
                Some(Category::Operation),
                "{} implements `incbin`",
                surface.dialect
            );
        }
    }

    /// Composition replaces an entry rather than adding a second claimant.
    #[test]
    fn an_overridden_entry_does_not_appear_twice() {
        let sjasmplus = surfaces()
            .into_iter()
            .find(|s| s.dialect == "sjasmplus")
            .expect("declared");
        let bytes: Vec<&str> = sjasmplus
            .directives
            .iter()
            .filter(|d| d.id == "bytes")
            .map(|d| d.id)
            .collect();
        assert_eq!(bytes.len(), 1, "`bytes` should be declared once");

        // And the override is the richer one: sjasmplus adds `byte` to the
        // base's four spellings.
        let entry = super::lookup(&sjasmplus.directives, "byte").expect("sjasmplus takes `byte`");
        assert_eq!(entry.id, "bytes");
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
