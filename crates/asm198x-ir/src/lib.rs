//! The contract between Asm198x's dialect front ends and its back end.
//!
//! A front end turns source into what this crate defines, and the assembler
//! back end consumes it: the source-preserving [`ast`], the generic
//! [`Statement`] stream it lowers to, the [`Expr`] values inside it, and the
//! [`span`]s, [`source`] loading and diagnostics both sides report through.
//!
//! Nothing here assembles. Passes, symbol resolution, layout, linking and
//! artifacts stay in `asm198x`. This crate exists so a front end can be built
//! against the boundary alone, which is the first stage of moving dialect front
//! ends to Lang198x (198x `decisions/assembly-source-layer-moves-to-lang198x.md`).

pub mod ast;
pub mod diagnostic;
pub mod source;
pub mod span;
pub mod stream;

pub use diagnostic::{AsmError, Code, Warning, WarningKind};
pub use span::{ExpansionFrame, FileId, Span};
pub use stream::{
    ArtifactFormat, BinOp, DataRange, DeviceSpec, DiagSeverity, Expr, FIX_BITS, NexCommand,
    Operation, OutputFormat, Piece, Place, RequestedOutput, Statement, TapeKind, align_pad,
    encoded_pieces, eval_binop, form, next_pc,
};
