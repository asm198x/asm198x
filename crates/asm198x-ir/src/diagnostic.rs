//! Diagnostics both sides of the boundary raise: [`AsmError`], [`Warning`] and
//! the stable [`Code`] registry.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::span::{FileId, Span};

// One registry defines the wire names, exhaustive list and embedded pages.
// A new code cannot compile without its explanation; xtask publishes precisely
// these pages into the book, and --check detects stale text or navigation.
macro_rules! diagnostic_codes {
    ($( $(#[$doc:meta])* $variant:ident => $page:literal ),+ $(,)?) => {
        /// Stable diagnostic codes, classified at the error site rather than
        /// inferred from wording. Unclassified errors retain `AssemblyError`.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[non_exhaustive]
        pub enum Code { $( $(#[$doc])* $variant ),+ }

        impl Code {
            /// Every code this binary can emit, in documentation order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// The stable spelling used in JSON and by `--explain`.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => stringify!($variant)),+ }
            }

            /// Look up an exact stable code name. Unknown names are not aliases
            /// for an unrelated explanation.
            #[must_use]
            pub fn from_name(name: &str) -> Option<Self> {
                match name { $(stringify!($variant) => Some(Self::$variant)),+, _ => None }
            }

            /// Markdown shared verbatim by the CLI and generated book page.
            #[must_use]
            pub const fn explanation(self) -> &'static str {
                match self { $(Self::$variant => include_str!(concat!("../diagnostics/", $page, ".md"))),+ }
            }

            /// Stable book filename, without directory or extension.
            #[must_use]
            pub const fn page_slug(self) -> &'static str {
                match self { $(Self::$variant => $page),+ }
            }
        }
    };
}

diagnostic_codes! {
    /// An assembly error not yet assigned a more specific code.
    AssemblyError => "assembly-error",
    /// The encoded PC-relative displacement cannot reach the target.
    BranchOutOfRange => "branch-out-of-range",
    /// A statically counted routine exceeds the source's declared ceiling.
    CycleBudgetExceeded => "cycle-budget-exceeded",
    /// Valid reference syntax requests behaviour this build does not implement.
    UnsupportedFeature => "unsupported-feature",
}

/// An assembly error, with the 1-based source line it occurred on (0 = no
/// specific line).
#[derive(Debug, Clone)]
pub struct AsmError {
    pub line: usize,
    pub message: String,
    /// Stable classification, preserved through source-span enrichment.
    pub code: Code,
    /// The source span, when the raising site knows a column-level position (the
    /// AST-routed dialects, once U3 wires them). `None` for the line-only sites,
    /// where the diagnostic is line-granular. Per contract KTD1 the span rides
    /// this engine error path — not the AST — so every CPU inherits diagnostics,
    /// and column accuracy improves as CPUs adopt the AST.
    pub span: Option<Span>,
    /// The errors found after this one in the same assembly, in the order
    /// they were found, for a dialect that carries on past an error as its
    /// reference assembler does. Empty for one that stops at the first.
    /// `Display` shows this error alone; [`all`](Self::all) gives every one.
    /// `None` when there are none; boxed, to keep the error small.
    pub further: Option<Box<Vec<AsmError>>>,
}

impl AsmError {
    /// A line-granular error with the catch-all diagnostic code.
    pub fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
            span: None,
            code: Code::AssemblyError,
            further: None,
        }
    }

    /// An error carrying a source span. `line` mirrors the span's line so the
    /// `Display` impl and existing `.line` readers keep working unchanged.
    pub fn at(span: Span, message: impl Into<String>) -> Self {
        Self {
            line: span.line as usize,
            message: message.into(),
            span: Some(span),
            code: Code::AssemblyError,
            further: None,
        }
    }

    /// Classify at the raising site without changing its message or location.
    #[must_use]
    pub fn with_code(mut self, code: Code) -> Self {
        self.code = code;
        self
    }

    pub fn unsupported(line: usize, message: impl Into<String>) -> Self {
        Self::new(line, message).with_code(Code::UnsupportedFeature)
    }

    /// The first of `errors`, carrying the rest as [`further`](Self::further),
    /// or `None` if there are none. Errors that already carry further ones
    /// are flattened in order, so no further error carries any of its own.
    #[must_use]
    pub fn gather(errors: Vec<AsmError>) -> Option<Self> {
        let mut flat = errors.into_iter().flat_map(AsmError::into_all);
        let mut first = flat.next()?;
        let further: Vec<AsmError> = flat.collect();
        first.further = (!further.is_empty()).then(|| Box::new(further));
        Some(first)
    }

    /// This error, then each [`further`](Self::further) one, in order.
    pub fn all(&self) -> impl Iterator<Item = &AsmError> {
        std::iter::once(self).chain(self.further.iter().flat_map(|further| further.iter()))
    }

    /// This error and each further one, apart, in order.
    #[must_use]
    pub fn into_all(mut self) -> Vec<AsmError> {
        let further = self
            .further
            .take()
            .map(|further| *further)
            .unwrap_or_default();
        std::iter::once(self).chain(further).collect()
    }
}

impl fmt::Display for AsmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}", self.message)
        } else {
            write!(f, "line {}: {}", self.line, self.message)
        }
    }
}

impl std::error::Error for AsmError {}

/// A non-fatal assembly advisory, with the 1-based source line it applies to
/// (0 = no specific line). Reference assemblers assemble *and* flag questionable
/// source (e.g. an immediate too wide for its operand); a `Warning` carries that
/// signal without failing the assembly. The bytes are still produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Warning {
    pub line: usize,
    pub message: String,
    /// Whether this is the assembler flagging something, or the source asking
    /// to say it. Additive with a default, so payloads that predate it load
    /// and serialize unchanged (`core-contract-freeze.md` names `Warning` in
    /// the additive set).
    #[serde(default, skip_serializing_if = "WarningKind::is_advisory")]
    pub kind: WarningKind,
    /// The file `line` counts within (language-surface U2) — a warning raised
    /// inside an include names that include. Additive per KTD7: absent means
    /// the root, and a root value is not serialized, so pre-multi-file
    /// payloads are byte-identical.
    #[serde(default, skip_serializing_if = "FileId::is_root")]
    pub file: FileId,
}

/// What a [`Warning`] is: the assembler's own advisory, or text the source
/// asked to print.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub enum WarningKind {
    /// The assembler flagging questionable source — an immediate too wide for
    /// its operand, a value that will not converge.
    #[default]
    Advisory,
    /// Text the source asked to print: rgbasm's `PRINT`, sjasmplus's
    /// `DISPLAY`, vasm's `echo`, ca65's `.out`. Not a complaint.
    Note,
}

impl WarningKind {
    pub fn is_advisory(&self) -> bool {
        matches!(self, WarningKind::Advisory)
    }
}

impl Warning {
    /// An informational note, not a warning about questionable source.
    /// A line of zero means the note has no specific source location.
    #[must_use]
    pub fn note(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
            file: FileId(0),
            kind: WarningKind::Note,
        }
    }

    #[must_use]
    pub fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
            file: FileId(0),
            kind: WarningKind::Advisory,
        }
    }

    /// The same warning, counted within `file` rather than the root input.
    #[must_use]
    pub fn in_file(mut self, file: FileId) -> Self {
        self.file = file;
        self
    }

    /// The same warning as `kind` — an advisory or a note.
    #[must_use]
    pub fn with_kind(mut self, kind: WarningKind) -> Self {
        self.kind = kind;
        self
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // A note is the source speaking, not the assembler complaining, so it
        // does not call itself a warning.
        let label = match self.kind {
            WarningKind::Note => "note",
            _ => "warning",
        };
        if self.line == 0 {
            write!(f, "{label}: {}", self.message)
        } else {
            write!(f, "line {}: {label}: {}", self.line, self.message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AsmError;

    /// `gather` makes the first error carry the rest, in order, flattening
    /// any that already carried further ones; `all` and `into_all` give them
    /// back in the same order.
    #[test]
    fn errors_gather_in_order() {
        assert!(AsmError::gather(Vec::new()).is_none());
        let carried = AsmError::gather(vec![AsmError::new(2, "b"), AsmError::new(3, "c")])
            .expect("two errors");
        let all = AsmError::gather(vec![AsmError::new(1, "a"), carried, AsmError::new(4, "d")])
            .expect("errors");
        let messages: Vec<&str> = all.all().map(|e| e.message.as_str()).collect();
        assert_eq!(messages, ["a", "b", "c", "d"]);
        assert!(all.all().skip(1).all(|e| e.further.is_none()));
        // `Display` shows the first alone.
        assert_eq!(all.to_string(), "line 1: a");
        let lines: Vec<usize> = all.into_all().iter().map(|e| e.line).collect();
        assert_eq!(lines, [1, 2, 3, 4]);
    }
}
