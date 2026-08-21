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
    /// Every spelling this entry accepts, for a generator.
    #[must_use]
    pub fn spellings(&self) -> &'static [&'static str] {
        match self.pattern {
            Pattern::Exact(list) => list,
        }
    }

    /// Whether `word` is one of them. Case-insensitive: no dialect here
    /// distinguishes `DB` from `db`.
    #[must_use]
    pub fn matches(&self, word: &str) -> bool {
        self.spellings()
            .iter()
            .any(|s| s.eq_ignore_ascii_case(word))
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
        assert_eq!(entry.spellings(), &["db", "dc", "byte"]);
    }
}
