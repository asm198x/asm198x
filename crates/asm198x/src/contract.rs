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

use crate::engine::{AsmError, Assembly, DebugData, Warning};
use crate::span::Span;

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
    /// Non-fatal diagnostics attached to a *successful* assembly (U2). Empty
    /// until producers populate it (warnings-as-diagnostics, lints); the failure
    /// path returns an [`AsmError`], which the CLI's JSON mode (U4) converts to a
    /// [`Diagnostic`] via [`From`]. The field is the contract slot every consumer
    /// can rely on across CPUs (R8).
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
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
            diagnostics: Vec::new(),
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
            diagnostics: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Diagnostics — the rustc-shaped error model (R2) + the thin envelope (R6)
// ---------------------------------------------------------------------------

/// Diagnostic severity. `#[non_exhaustive]` — finer levels (note, help) may be
/// added additively.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Severity {
    Error,
    Warning,
}

/// A stable, machine-readable diagnostic code a consumer can switch on (R2).
/// Codes are assigned incrementally as error sites are classified; today every
/// engine error maps to [`Code::AssemblyError`], the catch-all. More-specific
/// codes are added additively (`#[non_exhaustive]`), never renumbered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Code {
    /// The unclassified default — an assembly error with no more-specific code
    /// assigned yet. Stable: consumers may switch on it.
    AssemblyError,
}

/// A machine-applicable suggested fix (rustc's model): a human description and,
/// when the fix is a concrete edit, the `replacement` text applied at the
/// diagnostic's span. `replacement: None` is a description-only suggestion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Fix {
    pub description: String,
    #[serde(default)]
    pub replacement: Option<String>,
}

impl Fix {
    /// A description-only fix (no concrete replacement).
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            replacement: None,
        }
    }

    /// Attach machine-applicable replacement text (applied at the diagnostic's span).
    #[must_use]
    pub fn with_replacement(mut self, replacement: impl Into<String>) -> Self {
        self.replacement = Some(replacement.into());
        self
    }
}

/// The thin shared diagnostic envelope (R6): severity + human message, with none
/// of the rich span/code/fix fields. asm198x's own errors are the full
/// [`Diagnostic`]; a *foreign-tool* rejection the verdict pipeline records —
/// whose text is the reference tool's, in no coordinate system of ours — is
/// *only* an envelope. dbg198x and the verdict pipeline reuse this without the
/// rich fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DiagnosticEnvelope {
    pub severity: Severity,
    pub message: String,
}

impl DiagnosticEnvelope {
    pub fn new(severity: Severity, message: impl Into<String>) -> Self {
        Self {
            severity,
            message: message.into(),
        }
    }
}

/// asm198x's own diagnostic — the rustc model (R2): a source [`Span`], a stable
/// [`Code`], a [`Severity`], a human message, and an optional machine-applicable
/// [`Fix`]. A `None` span means line-granular (the raising site had no column);
/// see contract KTD1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Diagnostic {
    #[serde(default)]
    pub span: Option<Span>,
    pub code: Code,
    pub severity: Severity,
    pub message: String,
    #[serde(default)]
    pub fix: Option<Fix>,
}

impl Diagnostic {
    /// A diagnostic with a severity, code, and message; span and fix unset.
    pub fn new(severity: Severity, code: Code, message: impl Into<String>) -> Self {
        Self {
            span: None,
            code,
            severity,
            message: message.into(),
            fix: None,
        }
    }

    /// Attach the source span (the token position, once U3 wires real columns).
    #[must_use]
    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    /// Attach a machine-applicable suggested fix.
    #[must_use]
    pub fn with_fix(mut self, fix: Fix) -> Self {
        self.fix = Some(fix);
        self
    }

    /// Flatten to the thin shared [`DiagnosticEnvelope`] (R6) — severity +
    /// message, dropping the rich fields.
    #[must_use]
    pub fn envelope(&self) -> DiagnosticEnvelope {
        DiagnosticEnvelope::new(self.severity, self.message.clone())
    }
}

/// An engine [`AsmError`] becomes a line-granular [`Diagnostic`]: it keeps its
/// span when the raising site had one (the AST-routed dialects, once U3 wires
/// them), else a synthesized line-only span from `AsmError::line` (`0` → no
/// span). The message is preserved verbatim; the code is the
/// [`Code::AssemblyError`] catch-all until sites are classified (R2).
impl From<AsmError> for Diagnostic {
    fn from(e: AsmError) -> Self {
        let span = e
            .span
            .or_else(|| (e.line != 0).then(|| Span::at(e.line as u32, 0)));
        Self {
            span,
            code: Code::AssemblyError,
            severity: Severity::Error,
            message: e.message,
            fix: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::AsmError;
    use crate::span::Span;

    /// AE2: an `AsmError` raised *with* a span converts to a `Diagnostic` that
    /// reports the column, not just the line. Exercises `AsmError::at` — the seam
    /// U3 uses to populate real columns from the AST-routed dialects.
    #[test]
    fn diagnostic_from_error_with_span_reports_column() {
        let e = AsmError::at(Span::at(4, 11), "operand out of range");
        let d = Diagnostic::from(e);
        let span = d.span.expect("diagnostic carries the span");
        assert_eq!(span.line, 4);
        assert_eq!(span.col, 11, "the column is preserved, not just the line");
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.code, Code::AssemblyError);
        assert_eq!(d.message, "operand out of range");
    }

    /// A line-only `AsmError` becomes a line-granular `Diagnostic` (span present,
    /// column unset) with the message preserved verbatim (characterization).
    #[test]
    fn diagnostic_from_line_only_error_is_line_granular() {
        let d = Diagnostic::from(AsmError::new(2, "unknown instruction `frob`"));
        let span = d.span.expect("line-granular span");
        assert_eq!(span.line, 2);
        assert_eq!(span.col, 0, "no column -> line-granular");
        assert_eq!(d.message, "unknown instruction `frob`");
    }

    /// A no-line error (`line == 0`) converts with no span at all.
    #[test]
    fn diagnostic_from_lineless_error_has_no_span() {
        assert!(
            Diagnostic::from(AsmError::new(0, "no entry point"))
                .span
                .is_none()
        );
    }

    /// `fix` is optional: `None` by default, `Some` with an optional concrete
    /// replacement when the edit is machine-applicable.
    #[test]
    fn diagnostic_fix_is_optional() {
        let plain = Diagnostic::new(Severity::Error, Code::AssemblyError, "x");
        assert!(plain.fix.is_none());

        let fixable = Diagnostic::new(Severity::Warning, Code::AssemblyError, "byte too wide")
            .with_fix(Fix::new("mask to 8 bits").with_replacement("value & $ff"));
        let fix = fixable.fix.expect("a fix");
        assert_eq!(fix.replacement.as_deref(), Some("value & $ff"));
    }

    /// A rich `Diagnostic` flattens to the thin shared envelope (R6) — severity +
    /// message only.
    #[test]
    fn diagnostic_flattens_to_envelope() {
        let d =
            Diagnostic::new(Severity::Error, Code::AssemblyError, "boom").with_span(Span::at(1, 1));
        let env = d.envelope();
        assert_eq!(env.severity, Severity::Error);
        assert_eq!(env.message, "boom");
    }
}
