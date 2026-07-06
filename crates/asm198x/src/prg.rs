//! Commodore `.prg` serialization.
//!
//! A `.prg` is the flat program image prefixed with its 2-byte little-endian
//! load address — the address the KERNAL `LOAD` places it at, and where a C64
//! program's `*=` origin points. This is what `acme -f cbm` emits, so an
//! assembled program round-trips byte-for-byte against that reference (see #35).

use crate::contract::AssemblyResult;

/// Serialize an [`AssemblyResult`] into a C64 `.prg`: the load address (the flat
/// origin, little-endian) followed by the code bytes. Only meaningful for a flat
/// result (the CLI restricts `--prg` to the acme C64 dialect); a linked image
/// has no flat origin, so it serialises at load address `$0000`.
pub fn prg(asm: &AssemblyResult) -> Vec<u8> {
    let origin = asm.origin.unwrap_or(0);
    let mut out = Vec::with_capacity(2 + asm.bytes.len());
    out.push((origin & 0xFF) as u8);
    out.push((origin >> 8) as u8);
    out.extend_from_slice(&asm.bytes);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepends_little_endian_load_address() {
        // `lda #1 / sta $d020 / rts` at $0801. Byte-for-byte against
        // `acme -f cbm`: 01 08 a9 01 8d 20 d0 60.
        let asm: AssemblyResult = crate::engine::Assembly {
            origin: 0x0801,
            bytes: vec![0xA9, 0x01, 0x8D, 0x20, 0xD0, 0x60],
            symbols: std::collections::BTreeMap::new(),
            start: None,
            warnings: Vec::new(),
            debug: crate::DebugData::default(),
        }
        .into();
        assert_eq!(
            prg(&asm),
            vec![0x01, 0x08, 0xA9, 0x01, 0x8D, 0x20, 0xD0, 0x60]
        );
    }
}
