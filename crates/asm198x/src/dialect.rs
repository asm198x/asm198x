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

pub(crate) trait Dialect {
    /// The instruction set this dialect assembles against.
    fn instruction_set(&self) -> &'static isa::InstructionSet;

    /// Parse source into the engine's statement stream, resolving each
    /// instruction's addressing mode (so form sizes are stable across passes).
    ///
    /// # Errors
    /// Returns an [`AsmError`] on any tokenising or mode-resolution failure.
    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError>;
}
