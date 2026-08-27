//! Where each dialect looks for a relative `include`, as data.
//!
//! A multi-file project's first surprise is that a relative request does not
//! mean the same thing in every dialect. ca65 walks back up the chain of
//! includers; vasm and RGBDS anchor every request at the root input's
//! directory however deep the request; acme, lwasm, sjasmplus and the
//! asl-syntax chips look in the requesting file's own directory and nowhere
//! else. The references genuinely diverge, and each of ours is pinned against
//! the real tool.
//!
//! That is worth documenting and could not be, because the facts lived in
//! `pub(crate)` consts scattered across the dialects. This is the accessor a
//! documentation generator reads — the same job [`crate::directives::surfaces`]
//! does for the directive vocabulary, and for the same reason: a table a reader
//! could check must be generated, never typed.
//!
//! **Derived where it can be, tested where it cannot.** Most dialects hand a
//! `WalkSemantics` const to the
//! shared walk, and those rows are read straight off it, so the table and the
//! behaviour are one fact. acme and the Z80 pair resolve inside their own
//! walks against the requesting file, with no const to read; their rows are
//! stated here and held by the anchor tests in `tests/multifile.rs`.

use crate::dialects;
use crate::dialects::ca65_flat::{Resolution, WalkSemantics};

/// Where a relative request is looked for first.
///
/// The `-I` search directories always apply **after** the anchor, in the order
/// they were given, whichever anchor a dialect uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    /// The requesting file's own directory, and nothing beyond it. A copy in
    /// the root directory is *not* found from inside a subdirectory include.
    Requester,
    /// The requesting file's directory, then each enclosing includer's,
    /// innermost outward.
    AncestorChain,
    /// The root input's directory, for every request however deep.
    Root,
    /// The dialect does not read includes at all.
    None,
}

impl Anchor {
    fn of(semantics: &WalkSemantics) -> Self {
        match semantics.resolution {
            Resolution::Requester => Self::Requester,
            Resolution::AncestorChain => Self::AncestorChain,
            Resolution::Root => Self::Root,
        }
    }

    /// One line, as a reader would say it.
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Self::Requester => "The including file's own directory",
            Self::AncestorChain => "The including file's directory, then each enclosing includer's",
            Self::Root => "The root input's directory, however deep the request",
            Self::None => "No include",
        }
    }
}

/// One dialect's multi-file surface.
pub struct DialectIncludes {
    /// The `--dialect` name this describes.
    pub dialect: &'static str,
    pub anchor: Anchor,
    /// The extension tried before the exact spelling when a request carries
    /// none — asl's `inc`, and nothing anywhere else. Stored without the
    /// leading dot, as the walk stores it.
    pub default_extension: Option<&'static str>,
}

impl DialectIncludes {
    fn from(dialect: &'static str, semantics: &WalkSemantics) -> Self {
        Self {
            dialect,
            anchor: Anchor::of(semantics),
            default_extension: semantics.include_default_ext,
        }
    }

    /// A dialect whose walk resolves against the requesting file directly,
    /// with no semantics const to read.
    fn requester(dialect: &'static str) -> Self {
        Self {
            dialect,
            anchor: Anchor::Requester,
            default_extension: None,
        }
    }
}

/// How every dialect resolves a relative include.
///
/// Ordered as [`crate::dialect_table`] orders `--dialect`, so a generated
/// table reads in the order the rest of the documentation presents them.
#[must_use]
pub fn resolution() -> Vec<DialectIncludes> {
    use dialects::{asl, ca65_flat, lwasm, rgbasm, vasm};
    let ca65 = &ca65_flat::CA65_SEMANTICS;
    vec![
        // acme's `!src` is resolved in `AcmeEval::lower` against the
        // requesting file's path; the Z80 walk does the same for `include`.
        // Neither has a semantics const, so both are stated and tested.
        DialectIncludes::requester("acme"),
        DialectIncludes::from("ca65", ca65),
        DialectIncludes::from("65816", ca65),
        DialectIncludes::from("huc6280", ca65),
        DialectIncludes::from("vasm", &vasm::VASM_SEMANTICS),
        DialectIncludes::from("lwasm", &lwasm::SEMANTICS),
        DialectIncludes::from("rgbasm", &rgbasm::SEMANTICS),
        // pasmo's include is unimplemented, which the declared directive
        // surface also states — see `crate::directives`.
        DialectIncludes {
            dialect: "pasmo",
            anchor: Anchor::None,
            default_extension: None,
        },
        DialectIncludes::requester("sjasmplus"),
        DialectIncludes::from("8080", &asl::SEMANTICS),
        DialectIncludes::from("6800", &asl::SEMANTICS),
        DialectIncludes::from("1802", &asl::SEMANTICS),
        DialectIncludes::from("8048", &asl::SEMANTICS),
        DialectIncludes::from("scmp", &asl::SEMANTICS),
        DialectIncludes::from("f8", &asl::SEMANTICS),
        DialectIncludes::from("2650", &asl::SEMANTICS),
        DialectIncludes::from("tms7000", &asl::SEMANTICS),
        DialectIncludes::from("pdp11", &asl::SEMANTICS),
        DialectIncludes::from("tms9900", &asl::SEMANTICS),
        DialectIncludes::from("cp1610", &asl::CP1610_SEMANTICS),
        DialectIncludes::from("z8000", &asl::SEMANTICS),
    ]
}

#[cfg(test)]
mod tests {
    use super::{Anchor, resolution};

    /// The same accounting the declared directive surface keeps: a dialect
    /// `--dialect` offers either has a row here or shares one with the dialect
    /// it is a target variant of.
    #[test]
    fn every_selectable_dialect_is_accounted_for() {
        const SHARES: &[(&str, &str)] =
            &[("pasmonext", "pasmo"), ("8035", "8048"), ("z8001", "z8000")];
        let rows = resolution();
        for entry in crate::dialect_table::DIALECTS {
            if let Some((_, shared)) = SHARES.iter().find(|(v, _)| *v == entry.name) {
                assert!(
                    rows.iter().any(|r| r.dialect == *shared),
                    "`{}` shares `{shared}`, which has no row",
                    entry.name
                );
                continue;
            }
            assert!(
                rows.iter().any(|r| r.dialect == entry.name),
                "`{}` is selectable and has no include-resolution row",
                entry.name
            );
        }
    }

    #[test]
    fn no_dialect_is_listed_twice() {
        let mut names: Vec<&str> = resolution().iter().map(|r| r.dialect).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "a dialect has two rows");
    }

    /// The anchors the probes established, as an assertion rather than a
    /// comment. If a walk's semantics const changes, this is what says so.
    #[test]
    fn the_anchors_are_the_probed_ones() {
        let anchor = |name: &str| {
            resolution()
                .into_iter()
                .find(|r| r.dialect == name)
                .unwrap_or_else(|| panic!("`{name}` has a row"))
                .anchor
        };
        assert_eq!(anchor("ca65"), Anchor::AncestorChain);
        assert_eq!(anchor("vasm"), Anchor::Root);
        assert_eq!(anchor("rgbasm"), Anchor::Root);
        assert_eq!(anchor("lwasm"), Anchor::Requester);
        assert_eq!(anchor("acme"), Anchor::Requester);
        assert_eq!(anchor("sjasmplus"), Anchor::Requester);
        assert_eq!(anchor("8080"), Anchor::Requester);
        assert_eq!(anchor("pasmo"), Anchor::None);
    }

    /// asl is the only family with extension defaulting, and a table that
    /// showed it everywhere or nowhere would be wrong either way.
    #[test]
    fn only_the_asl_chips_default_an_extension() {
        for row in resolution() {
            let expected = matches!(
                row.dialect,
                "8080"
                    | "6800"
                    | "1802"
                    | "8048"
                    | "scmp"
                    | "f8"
                    | "2650"
                    | "tms7000"
                    | "pdp11"
                    | "tms9900"
                    | "cp1610"
                    | "z8000"
            )
            .then_some("inc");
            assert_eq!(
                row.default_extension, expected,
                "{} defaults the wrong extension",
                row.dialect
            );
        }
    }
}
