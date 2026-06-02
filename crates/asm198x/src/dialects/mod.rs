//! The dialect front-ends.
//!
//! One module per source dialect. Each implements [`Dialect`](crate::dialect::Dialect)
//! and names the `isa` spec it targets. Adding a dialect means adding a module
//! here — not touching the engine.

pub(crate) mod mos6502;
pub(crate) mod pasmonext;

pub(crate) use mos6502::Mos6502;
pub(crate) use pasmonext::PasmoNext;
