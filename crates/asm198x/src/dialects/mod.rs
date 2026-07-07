//! The dialect front-ends.
//!
//! One module per source dialect. Each implements [`Dialect`](crate::dialect::Dialect)
//! and names the `isa` spec it targets. Adding a dialect means adding a module
//! here — not touching the engine.

pub(crate) mod acme;
pub(crate) mod asl;
pub(crate) mod ca65;
pub(crate) mod ca65_816;
pub(crate) mod ca65_flat;
pub(crate) mod ca65_huc6280;
pub(crate) mod cdp1802;
pub(crate) mod cp1610;
pub(crate) mod f8;
pub(crate) mod i8048;
pub(crate) mod i8080;
pub(crate) mod lwasm;
pub(crate) mod m6800;
pub(crate) mod mos6502;
pub(crate) mod pasmo;
pub(crate) mod pdp11;
pub(crate) mod rgbasm;
pub(crate) mod s2650;
pub(crate) mod scmp;
pub(crate) mod sjasmplus;
pub(crate) mod tms7000;
pub(crate) mod tms9900;
pub(crate) mod vasm;
pub(crate) mod z80;
pub(crate) mod z8000;

pub(crate) use acme::Acme;
pub(crate) use ca65_816::Ca65_816;
pub(crate) use ca65_huc6280::Ca65Huc6280;
pub(crate) use cdp1802::Cdp1802;
pub(crate) use cp1610::Cp1610;
pub(crate) use f8::F8;
pub(crate) use i8048::I8048;
pub(crate) use i8080::I8080;
pub(crate) use lwasm::Lwasm;
pub(crate) use m6800::M6800;
pub(crate) use pasmo::Pasmo;
pub(crate) use pdp11::Pdp11;
pub(crate) use rgbasm::Rgbasm;
pub(crate) use s2650::S2650;
pub(crate) use scmp::Scmp;
pub(crate) use sjasmplus::Sjasmplus;
pub(crate) use tms7000::Tms7000;
pub(crate) use tms9900::Tms9900;
pub(crate) use z8000::Z8000;
