//! The dialect front-end abstraction.
//!
//! A [`Dialect`] is one source syntax: it tokenises its own directives,
//! literals, operators, and label rules, and resolves each instruction's
//! addressing mode against a target [`isa::InstructionSet`] — producing the
//! engine's generic [`Statement`](crate::engine::Statement) stream. Encoding
//! lives in the `isa` spec; the engine lays bytes down. Dialect is an axis
//! independent of CPU: several dialects may target the same spec (acme and
//! ca65 both emit 6502), and one dialect may target several (vasm covers more
//! than one CPU). See `decisions/syntax-stance.md`.

use crate::engine::{AsmError, Statement};

/// What a dialect does with a value too large for the byte operand it's emitted
/// into. The 6502/6809 assemblers (ACME, ca65, lwasm) treat it as an error; the
/// Z80 ones accept it and keep the low 8 bits — pasmo silently, sjasmplus with a
/// non-fatal warning.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Oversize {
    Error,
    Truncate,
    TruncateWarn,
}

pub(crate) trait Dialect {
    /// The primary instruction set this dialect assembles against.
    fn instruction_set(&self) -> &'static isa::InstructionSet;

    /// An optional extension set whose forms are *also* available — e.g. the
    /// Z80N opcodes a PasmoNext dialect adds on top of standard Z80. A dialect
    /// without one (the default) rejects those opcodes as unknown.
    fn extension_set(&self) -> Option<&'static isa::InstructionSet> {
        None
    }

    /// Parse source into the engine's statement stream, resolving each
    /// instruction's addressing mode (so form sizes are stable across passes).
    ///
    /// # Errors
    /// Returns an [`AsmError`] on any tokenising or mode-resolution failure.
    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError>;

    /// Parse into the semantic AST (`crate::ast`) — the source-preserving tree
    /// the formatter and bidirectional emit consume (U5). Defaults to `None`: a
    /// dialect without an AST front-end has no formatter yet and stays on
    /// [`parse`](Self::parse) for assembly. The Z80 dialects override it.
    ///
    /// # Errors
    /// Returns an [`AsmError`] on any parse failure.
    fn parse_ast(&self, _source: &str) -> Result<Option<crate::ast::Program>, AsmError> {
        Ok(None)
    }

    /// Whether emitting bytes before any origin is set is an error. ACME's `*=`
    /// is mandatory before code or data — it rejects an implicit origin with
    /// "Program counter undefined" — so a forgotten `*=` fails loudly rather than
    /// silently assembling at `$0000`. Off by default: a flat binary at origin 0
    /// is a legitimate default for the Z80/6809 tools (`org` optional).
    fn requires_explicit_origin(&self) -> bool {
        false
    }

    /// How to handle a value that overflows a **byte** operand (an 8-bit
    /// immediate or a `defb`-style byte). Defaults to [`Oversize::Error`]; the
    /// Z80 dialects override to truncate (pasmo silently, sjasmplus with a
    /// warning), matching their reference tools.
    fn oversized_byte_policy(&self) -> Oversize {
        Oversize::Error
    }

    /// The number of emitted bytes per **address unit** — how the location
    /// counter (labels, `*`/`$`, `org`) relates to the byte stream. Almost every
    /// CPU is byte-addressed, so this is `1`. The **CP1610** is *word*-addressed:
    /// its 10-bit "decle" is stored as a 2-byte word and `asl` counts addresses
    /// in decles, so labels advance by one per two bytes emitted — it returns `2`.
    /// Code must be a whole number of units long (the CP1610's is always
    /// decle-aligned).
    fn addr_unit(&self) -> i64 {
        1
    }
}
