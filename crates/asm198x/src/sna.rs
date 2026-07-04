//! 48K ZX Spectrum `.sna` snapshot serialization.
//!
//! A 48K `.sna` is a 27-byte Z80 register block followed by a 48K memory image
//! (`$4000..=$FFFF`). It carries no explicit PC field: the entry point is pushed
//! onto the machine stack, and the loader resumes with a `RET`, so `SP` in the
//! header points at the pushed address. This mirrors what `pasmo --sna` emits —
//! all registers zero except the fixed defaults below — so an assembled program
//! round-trips byte-for-byte against that reference (see #31).

use crate::engine::{AsmError, Assembly};

/// The Spectrum's RAM starts at `$4000`; the low 16K is ROM (absent from a
/// 48K snapshot).
const RAM_BASE: usize = 0x4000;
/// 48K of RAM, `$4000..=$FFFF`.
const RAM_SIZE: usize = 0xC000;
/// The Z80 register block that precedes the memory image.
const HEADER_LEN: usize = 27;
/// The screen attribute map (`$5800..=$5AFF`) as an offset into the RAM image,
/// and its length — the ROM power-on clear fills it with [`DEFAULT_ATTR`].
const ATTR_OFFSET: usize = 0x5800 - RAM_BASE;
const ATTR_LEN: usize = 0x300;
/// The Spectrum's default attribute byte: black ink (0) on white paper (7).
const DEFAULT_ATTR: u8 = 0x38;

/// Serialize an [`Assembly`] into a 48K `.sna` snapshot (49179 bytes).
///
/// The program's code is laid into a zero-filled RAM image at its origin, the
/// entry point (`end <addr>`) is pushed onto the stack at `$FFFC`, and the
/// register block takes `pasmo`'s defaults: interrupts enabled (IFF2), interrupt
/// mode 1, white border, everything else zero.
///
/// # Errors
/// Returns an [`AsmError`] if the assembly has no entry point (`end <addr>` is
/// required for a snapshot, as `pasmo --sna` demands), or if the code does not
/// fit in Spectrum RAM (`$4000..=$FFFF`).
pub fn sna_48k(asm: &Assembly) -> Result<Vec<u8>, AsmError> {
    let start = asm.start.ok_or_else(|| {
        AsmError::new(
            0,
            "a `.sna` snapshot needs an entry point — add `end <addr>`",
        )
    })?;

    let origin = usize::from(asm.origin);
    if origin < RAM_BASE {
        return Err(AsmError::new(
            0,
            format!(
                "code at ${origin:04X} is below Spectrum RAM ($4000); it cannot go in a 48K snapshot"
            ),
        ));
    }
    if origin + asm.bytes.len() > RAM_BASE + RAM_SIZE {
        return Err(AsmError::new(
            0,
            "code runs past the top of Spectrum RAM ($FFFF)",
        ));
    }

    // The RAM image is zero-filled except the screen attribute area, which the
    // ROM's power-on clear sets to $38 (black ink on white paper). pasmo mirrors
    // that default, so we do too — otherwise the two snapshots diverge across the
    // 768-byte attribute map even for identical code.
    let mut ram = vec![0u8; RAM_SIZE];
    ram[ATTR_OFFSET..ATTR_OFFSET + ATTR_LEN].fill(DEFAULT_ATTR);

    // Lay the code in at its origin, then push the entry point onto the stack:
    // SP defaults to $FFFE, and the push decrements it to $FFFC and writes the
    // start address there (little-endian). A `RET` on load pops it.
    let base = origin - RAM_BASE;
    ram[base..base + asm.bytes.len()].copy_from_slice(&asm.bytes);

    let sp: u16 = 0xFFFC;
    let sp_off = usize::from(sp) - RAM_BASE;
    ram[sp_off] = (start & 0xFF) as u8;
    ram[sp_off + 1] = (start >> 8) as u8;

    let mut out = vec![0u8; HEADER_LEN];
    out[19] = 0x04; // IFF2 set — interrupts enabled
    out[23] = (sp & 0xFF) as u8; // SP, little-endian
    out[24] = (sp >> 8) as u8;
    out[25] = 0x01; // interrupt mode 1
    out[26] = 0x07; // border: white
    out.extend_from_slice(&ram);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asm(bytes: Vec<u8>, origin: u16, start: Option<u16>) -> Assembly {
        Assembly {
            origin,
            bytes,
            symbols: std::collections::BTreeMap::new(),
            start,
            warnings: Vec::new(),
            debug: crate::DebugData::default(),
        }
    }

    #[test]
    fn header_defaults_and_pushed_entry_match_pasmo() {
        // `ld a,2 / ld ($5800),a / ret` at $8000, entry $8000. Verified
        // byte-for-byte against `pasmo --sna`.
        let code = vec![0x3E, 0x02, 0x32, 0x00, 0x58, 0xC9];
        let s = sna_48k(&asm(code.clone(), 0x8000, Some(0x8000))).expect("sna");
        assert_eq!(s.len(), 27 + 0xC000);
        // Register block: zero except IFF2=$04, SP=$FFFC, IM=1, border=7.
        assert_eq!(s[19], 0x04);
        assert_eq!(&s[23..25], &[0xFC, 0xFF]);
        assert_eq!(s[25], 0x01);
        assert_eq!(s[26], 0x07);
        // Code at $8000 -> RAM offset $4000 -> file offset 27 + $4000.
        let code_off = 27 + (0x8000 - 0x4000);
        assert_eq!(&s[code_off..code_off + code.len()], &code[..]);
        // Entry pushed at $FFFC (little-endian).
        let sp_off = 27 + (0xFFFC - 0x4000);
        assert_eq!(&s[sp_off..sp_off + 2], &[0x00, 0x80]);
        // Attribute area $5800..$5AFF is $38; the pixel area before it is zero.
        let attr_off = 27 + (0x5800 - 0x4000);
        assert!(s[attr_off..attr_off + 0x300].iter().all(|&b| b == 0x38));
        assert_eq!(s[27 + (0x5000 - 0x4000)], 0x00);
    }

    #[test]
    fn missing_entry_point_is_an_error() {
        let err = sna_48k(&asm(vec![0xC9], 0x8000, None)).expect_err("no entry");
        assert!(err.message.contains("entry point"));
    }

    #[test]
    fn code_below_ram_is_rejected() {
        let err = sna_48k(&asm(vec![0xC9], 0x3FFF, Some(0x3FFF))).expect_err("below RAM");
        assert!(err.message.contains("below Spectrum RAM"));
    }
}
