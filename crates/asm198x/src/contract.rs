//! The core contract — the one structured result every assemble entry point
//! returns (plan U1, requirement R1). Before this, the public API exposed three
//! ad-hoc shapes: `Assembly` (flat), `Vec<u8>` (a linked image), and
//! `(Vec<u8>, Vec<Warning>)` (a linked image plus advisories). They collapse
//! into [`AssemblyResult`], a serde-derivable value carrying the assembled
//! bytes, load origin, resolved symbols, entry point, warnings, and debug info —
//! so any consumer (the CLI's `--message-format=json` in U4, editors, agents, a
//! resumed dbg198x) sees one shape across every CPU and dialect.
//!
//! **Linked images and `origin`.** A flat assemble loads at a known `origin`; a
//! linked image (ca65's `.nes` ROM, vasm's Amiga hunk executable) is the
//! linker's output, with no single meaningful load address. R1 asks that such
//! output *not be forced into the flat shape*, so `origin` is an
//! [`Option<u16>`] — `None` for a linked image rather than a fabricated `0`.
//! (Planning KTD2 sketched this as an `Output::Flat | Output::Image` enum;
//! `origin: Option<u16>` captures the same distinction while keeping `bytes` a
//! plain field, which is the idiomatic public-data shape and avoids churning
//! every `.bytes` reader across the suite. See the U1 note in the plan.)
//!
//! The diagnostics field the plan gives `AssemblyResult` lands in **U2** (where
//! `Diagnostic` is defined); U1 ships the clean shape without it.
//!
//! The type is `#[non_exhaustive]` so fields can be added without a breaking
//! change, and serde ignores unknown fields on deserialise — the additive,
//! skip-unknown posture the versioning work (U5) builds on.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::engine::{Assembly, DebugData, Warning};

/// The one structured result every `assemble_*` entry point returns (R1). Every
/// current and future CPU inherits it with no per-CPU work (R8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AssemblyResult {
    /// The assembled machine code.
    pub bytes: Vec<u8>,
    /// The flat load origin, or `None` for a linked image (ca65 `.nes`, vasm
    /// hunk exe) whose bytes are the linker's, with no single meaningful origin.
    #[serde(default)]
    pub origin: Option<u16>,
    /// Resolved labels and constants. Empty for a linked image, which exposes no
    /// symbol table through the public API today.
    #[serde(default)]
    pub symbols: BTreeMap<String, i64>,
    /// The program's entry point, if an `end <addr>` directive gave one.
    #[serde(default)]
    pub start: Option<u16>,
    /// Non-fatal advisories raised during assembly (e.g. an oversize immediate
    /// truncated to fit, sjasmplus/vasm style). Empty when nothing warned.
    #[serde(default)]
    pub warnings: Vec<Warning>,
    /// Debug info captured during assembly — the line→address map and typed
    /// symbols the CLI renders into a `.dbg198x` sidecar / `--sym` / `--listing`.
    #[serde(default)]
    pub debug: DebugData,
}

impl AssemblyResult {
    /// A linked image (ca65 `.nes`, vasm hunk exe) — bytes only, no flat origin,
    /// symbols, or entry point exposed through the public API.
    #[must_use]
    pub fn image(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            origin: None,
            symbols: BTreeMap::new(),
            start: None,
            warnings: Vec::new(),
            debug: DebugData::default(),
        }
    }

    /// A linked image carrying non-fatal advisories (vasm's warned path).
    #[must_use]
    pub fn image_warned(bytes: Vec<u8>, warnings: Vec<Warning>) -> Self {
        Self {
            warnings,
            ..Self::image(bytes)
        }
    }
}

/// A flat assemble (`Assembly`, the engine's internal builder) becomes the flat
/// `AssemblyResult`, carrying its origin/bytes/symbols/start/warnings/debug
/// across unchanged, with the origin wrapped as `Some`.
impl From<Assembly> for AssemblyResult {
    fn from(a: Assembly) -> Self {
        Self {
            bytes: a.bytes,
            origin: Some(a.origin),
            symbols: a.symbols,
            start: a.start,
            warnings: a.warnings,
            debug: a.debug,
        }
    }
}
