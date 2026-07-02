//! The dialect front-ends.
//!
//! One module per source dialect. Each implements [`Dialect`](crate::dialect::Dialect)
//! and names the `isa` spec it targets. Adding a dialect means adding a module
//! here — not touching the engine.

pub(crate) mod acme;
pub(crate) mod ca65;
pub(crate) mod ca65_816;
pub(crate) mod ca65_huc6280;
pub(crate) mod i8080;
pub(crate) mod lwasm;
pub(crate) mod mos6502;
pub(crate) mod pasmo;
pub(crate) mod rgbasm;
pub(crate) mod sjasmplus;
pub(crate) mod vasm;
pub(crate) mod z80;

pub(crate) use acme::Acme;
pub(crate) use ca65_816::Ca65_816;
pub(crate) use ca65_huc6280::Ca65Huc6280;
pub(crate) use i8080::I8080;
pub(crate) use lwasm::Lwasm;
pub(crate) use pasmo::Pasmo;
pub(crate) use rgbasm::Rgbasm;
pub(crate) use sjasmplus::Sjasmplus;
