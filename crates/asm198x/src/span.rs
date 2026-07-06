//! The one source-provenance model, shared across the whole crate.
//!
//! A [`Span`] locates something in source as `(file, line, column)` with reserved
//! room for macro-expansion frames. It is the single span type used by the AST
//! ([`crate::ast`]), the engine's [`AsmError`](crate::engine::AsmError), and the
//! public [`Diagnostic`](crate::contract::Diagnostic) — deliberately *one* type,
//! not two that drift (see `decisions/roadmap-sequencing.md` § the span/source
//! seam). It lives in this leaf module, below both `engine` and `ast`, so the
//! engine's error type can carry a span without a circular dependency on the AST.
//!
//! **Multi-file ready now, before includes exist.** v1 is single-file
//! (`FileId(0)`); idea 4's include chains allocate further ids and idea 4's macro
//! engine fills `expansion_frames` — both additively, with no shape change. The
//! type is `#[non_exhaustive]` so a byte `offset` (deferred until a parser threads
//! a byte cursor, contract KTD1) or other fields can be added without a break.

use serde::{Deserialize, Serialize};

/// Identifies a source file. v1 is single-file (`FileId(0)`); include chains
/// (idea 4) allocate further ids so a span can name the *included* file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FileId(pub u32);

/// One macro-expansion frame (a rustc-style defined-at / invoked-at record).
/// Reserved now; idea 4's macro engine fills it. Empty in v1.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ExpansionFrame {
    pub macro_name: String,
    pub invoked_at: Box<Span>,
}

/// Where something came from: `(file, line, column)` through the include chain,
/// with reserved room for macro-expansion frames.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Span {
    pub file: FileId,
    /// 1-based source line.
    pub line: u32,
    /// 1-based **byte** column within the line; `0` means line-granular (the
    /// raising site knew no column — a JSON consumer should treat the span as
    /// the whole line). For an operand-range diagnostic the column is the start
    /// of the operand *field* after the mnemonic, not the individual offending
    /// operand (contract U3/KTD1). Byte, not character: a multi-byte UTF-8
    /// sequence earlier on the line advances it by its byte length.
    pub col: u32,
    /// Empty in v1; populated when idea 4's macros land, without a type change.
    #[serde(default)]
    pub expansion_frames: Vec<ExpansionFrame>,
}

impl Span {
    /// A single-file v1 span with no expansion frames.
    #[must_use]
    pub fn at(line: u32, col: u32) -> Self {
        Span {
            file: FileId(0),
            line,
            col,
            expansion_frames: Vec::new(),
        }
    }
}
