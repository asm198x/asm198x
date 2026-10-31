//! The ld65 job (#486's artifact/layout split): the memory account, the
//! link that lays the segments into the layout's image, and the resolve and
//! emit steps that turn a placed statement into bytes. It is its own file so
//! that a change to how bytes land in the ROM is read apart from the parse
//! and the projection sweep. Moved verbatim from the parent; the seam is the
//! boundary, not a rewrite.

// ---------------------------------------------------------------------------
// Resolution and emission
// ---------------------------------------------------------------------------

/// What a visibility word asks of the name it carries, once assembly and
/// linking are one step over one translation unit.
///
/// Probed against ca65 2.18 + ld65 on 2026-08-24. The three answers do not
/// follow the export/import split the manual's headings suggest: `.global` is
/// in neither camp, and `.forceimport` is in no camp at all — defining its name
/// is `Symbol 'zz' is already an import` and not defining it is an unresolved
/// external, so it is declared `Category::RefusedByReference` instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VisRule {
    /// `.export`, `.exportzp`. ca65 itself answers `Exported symbol 'nope' was
    /// never defined`, whether or not anything references it.
    MustBeDefined,
    /// `.import`, `.importzp`. Defining the name is ca65's `Symbol 'zz' is
    /// already an import`. Leaving it undefined is fine until something reads
    /// it, and then it is an unresolved external — which is the ordinary
    /// undefined-symbol refusal here.
    MustNotBeDefined,
    /// `.global`, `.globalzp` — export if the name is defined here, import if
    /// it is not. Both are legal, so nothing is checked beyond the zero-page
    /// warning the `zp` spelling carries.
    EitherWay,
}
