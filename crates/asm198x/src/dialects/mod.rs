//! The dialect front-ends.
//!
//! One module per source dialect. Each implements [`Dialect`](crate::dialect::Dialect)
//! and names the `isa` spec it targets. Adding a dialect means adding a module
//! here — not touching the engine.

pub(crate) mod acme;
pub(crate) mod mos6502;
pub(crate) mod pasmo;
pub(crate) mod sjasmplus;
pub(crate) mod z80;

pub(crate) use acme::Acme;
pub(crate) use pasmo::Pasmo;
pub(crate) use sjasmplus::Sjasmplus;
