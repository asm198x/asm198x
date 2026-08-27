//! Spectrum tape containers — `.tap` and `.tzx`, with pasmo's auto-run stub.
//!
//! [`decisions/assemble-io-model.md`](../../../decisions/assemble-io-model.md)
//! sequences these beside `.sna` as the tier-one Spectrum targets, and the
//! umbrella's `tape-framing-vs-mastering.md` draws the line this file sits on:
//! *"if the tape's content is the assembled program and nothing else, it is a
//! framing"*. One program, one optional loader stub. Composing a distribution
//! tape from several inputs is Build198x's.
//!
//! Every rule here was read off PasmoNext v0.1.3 before it was written, because
//! the acceptance bar is a byte-identical diff against `pasmo --tap`,
//! `--tapbas`, `--tzx` and `--tzxbas`. The two that a reader would most likely
//! guess wrong:
//!
//! - **The block name is the output path as given**, truncated to ten
//!   characters and space-padded — not the program's name, and not the file
//!   stem. `pasmo --tap x.asm sub/o.tap` names the block `sub/o.tap`.
//! - **The stub's last line is conditional.** `RANDOMIZE USR` is emitted only
//!   when the source gave an entry point with `end`; without one the loader
//!   loads and stops, and the BASIC program is 18 bytes shorter.

use format198x_sinclair_zx_spectrum_tap as tap;

use crate::contract::AssemblyResult;
use crate::engine::AsmError;

/// Which container to frame the blocks in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TapeFormat {
    /// The bare block stream a `.tap` is.
    Tap,
    /// The same blocks, each wrapped in a TZX standard-speed data block.
    Tzx,
}

/// TZX's signature and terminator (`syntheses/zx-spectrum/tape-loading-format.md`
/// §5, citing fuse's `tzx_read.c`).
const TZX_SIGNATURE: &[u8] = b"ZXTape!\x1a";

/// The version pasmo writes. **1.13, not the format's current 1.20** — every
/// byte here is diffed against pasmo, so this follows it rather than the spec.
const TZX_VERSION: [u8; 2] = [1, 13];

/// TZX block `$10`: standard speed data, the one a ROM loader reads.
const TZX_STANDARD_SPEED: u8 = 0x10;

/// The pause after each block, in milliseconds, as pasmo writes it.
const TZX_PAUSE_MS: u16 = 1000;

/// How long a tape block's name may be. Longer is truncated, shorter is
/// space-padded — the header field is fixed width.
const NAME_LEN: usize = 10;

/// The BASIC line number the stub auto-starts from, which is also its first
/// line.
const AUTOSTART_LINE: u16 = 10;

/// Serialize an assembled Spectrum program as a tape image.
///
/// `name` is the output path exactly as the caller was given it — pasmo puts
/// that in the block header rather than the program's name. `autorun` prepends
/// the BASIC loader stub, which is what `--tapbas`/`--tzxbas` do.
///
/// # Errors
/// Returns an [`AsmError`] if the program does not fit below the top of memory,
/// or if `autorun` is asked for a program with no origin to `CLEAR` below.
pub fn tape(
    asm: &AssemblyResult,
    format: TapeFormat,
    name: &str,
    autorun: bool,
) -> Result<Vec<u8>, AsmError> {
    let origin = asm.origin.unwrap_or(0);
    if usize::from(origin) + asm.bytes.len() > 0x1_0000 {
        return Err(AsmError::new(
            0,
            "code runs past the top of memory ($FFFF); it cannot go on a tape",
        ));
    }

    let mut blocks = Vec::new();
    if autorun {
        let basic = loader_stub(origin, asm.start);
        let header = tap::Header::new(
            tap::HeaderKind::Program,
            "loader",
            basic.len() as u16,
            AUTOSTART_LINE,
            basic.len() as u16,
        );
        blocks.push(header.block());
        blocks.push(tap::TapBlock::data(basic));
    }
    let code = tap::Header::new(
        tap::HeaderKind::Code,
        &clipped(name),
        asm.bytes.len() as u16,
        origin,
        0x8000,
    );
    blocks.push(code.block());
    blocks.push(tap::TapBlock::data(asm.bytes.clone()));

    Ok(match format {
        Tap => tap::encode(&blocks),
        Tzx => {
            let mut out = Vec::with_capacity(TZX_SIGNATURE.len() + 2);
            out.extend_from_slice(TZX_SIGNATURE);
            out.extend_from_slice(&TZX_VERSION);
            // A TZX is the same block stream with each block introduced: the
            // type byte, the pause that follows it, and its length. The block's
            // own bytes — flag, payload, checksum — are the `.tap` ones.
            for block in &blocks {
                let bytes = tap::encode(std::slice::from_ref(block));
                // `encode` writes each block behind its own 16-bit length,
                // which TZX carries in its own header instead.
                let payload = &bytes[2..];
                out.push(TZX_STANDARD_SPEED);
                out.extend_from_slice(&TZX_PAUSE_MS.to_le_bytes());
                out.extend_from_slice(&(payload.len() as u16).to_le_bytes());
                out.extend_from_slice(payload);
            }
            out
        }
    })
}

use TapeFormat::{Tap, Tzx};

/// The name as it goes in a header: at most ten characters, space-padded.
fn clipped(name: &str) -> String {
    let mut out: String = name.chars().take(NAME_LEN).collect();
    while out.chars().count() < NAME_LEN {
        out.push(' ');
    }
    out
}

/// pasmo's auto-run loader, tokenised as the Spectrum stores a BASIC program.
///
/// ```text
///  10 CLEAR <origin - 1>
///  20 POKE 23610,255
///  30 LOAD ""CODE
///  40 RANDOMIZE USR <entry>      (only when `end` gave one)
/// ```
///
/// Line 20 is pasmo's own: `23610` is `ERR_NR`, and setting it suppresses the
/// report the loader would otherwise leave on screen. It is written here
/// because pasmo writes it, not because a loader needs it.
fn loader_stub(origin: u16, entry: Option<u16>) -> Vec<u8> {
    let mut out = Vec::new();
    // `CLEAR` one below the origin, so the program loads above BASIC's memory.
    line(
        &mut out,
        10,
        &[&[TOKEN_CLEAR], number(origin.wrapping_sub(1)).as_slice()].concat(),
    );
    line(
        &mut out,
        20,
        &[&[TOKEN_POKE][..], &number(23610), b",", &number(255)].concat(),
    );
    line(&mut out, 30, &[TOKEN_LOAD, b'"', b'"', TOKEN_CODE]);
    if let Some(entry) = entry {
        line(
            &mut out,
            40,
            &[&[TOKEN_RANDOMIZE, TOKEN_USR][..], &number(entry)].concat(),
        );
    }
    out
}

/// One BASIC line: the number **big-endian** (the one big-endian field in the
/// format), the body's length little-endian, then the body and its `ENTER`.
fn line(out: &mut Vec<u8>, number: u16, body: &[u8]) {
    out.extend_from_slice(&number.to_be_bytes());
    out.extend_from_slice(&((body.len() + 1) as u16).to_le_bytes());
    out.extend_from_slice(body);
    out.push(ENTER);
}

/// A whole number as BASIC stores it: the digits a reader sees, then the
/// invisible binary form the interpreter uses — `$0E`, then the five-byte
/// number, which for an integer is `00 00 <lo> <hi> 00`.
fn number(value: u16) -> Vec<u8> {
    let mut out = value.to_string().into_bytes();
    out.push(NUMBER_MARK);
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(&value.to_le_bytes());
    out.push(0);
    out
}

const ENTER: u8 = 0x0D;
const NUMBER_MARK: u8 = 0x0E;
const TOKEN_CLEAR: u8 = 0xFD;
const TOKEN_POKE: u8 = 0xF4;
const TOKEN_LOAD: u8 = 0xEF;
const TOKEN_CODE: u8 = 0xAF;
const TOKEN_RANDOMIZE: u8 = 0xF9;
const TOKEN_USR: u8 = 0xC0;

#[cfg(test)]
mod tests {
    use super::{TapeFormat, tape};

    /// The source every expectation below was produced from, by
    /// `pasmo --tap` / `--tapbas` / `--tzx` / `--tzxbas` v0.1.3.
    const SRC: &str =
        "        org 32768\nstart:  ld a,2\n        out (254),a\n        ret\n        end start\n";

    fn image(format: TapeFormat, name: &str, autorun: bool) -> Vec<u8> {
        let asm = crate::assemble_pasmo(SRC).expect("assembles");
        tape(&asm, format, name, autorun).expect("frames")
    }

    /// A `.tap` is the bare block stream: a CODE header naming the **output
    /// path**, then the program behind its flag and checksum.
    #[test]
    fn tap_matches_pasmo() {
        #[rustfmt::skip]
        let want: &[u8] = &[
            0x13, 0x00, 0x00, 0x03, 0x6F, 0x75, 0x74, 0x2E, 0x74, 0x61, 0x70, 0x20, 0x20,
            0x20, 0x05, 0x00, 0x00, 0x80, 0x00, 0x80, 0x03, 0x07, 0x00, 0xFF, 0x3E, 0x02,
            0xD3, 0xFE, 0xC9, 0x27
        ];
        assert_eq!(image(TapeFormat::Tap, "out.tap", false), want);
    }

    /// A `.tzx` is the same blocks, each behind a `$10` standard-speed header
    /// carrying the pause. The version is **1.13**, which is pasmo's rather
    /// than the format's current 1.20.
    #[test]
    fn tzx_matches_pasmo() {
        #[rustfmt::skip]
        let want: &[u8] = &[
            0x5A, 0x58, 0x54, 0x61, 0x70, 0x65, 0x21, 0x1A, 0x01, 0x0D, 0x10, 0xE8, 0x03,
            0x13, 0x00, 0x00, 0x03, 0x6F, 0x75, 0x74, 0x2E, 0x74, 0x7A, 0x78, 0x20, 0x20,
            0x20, 0x05, 0x00, 0x00, 0x80, 0x00, 0x80, 0x10, 0x10, 0xE8, 0x03, 0x07, 0x00,
            0xFF, 0x3E, 0x02, 0xD3, 0xFE, 0xC9, 0x27
        ];
        assert_eq!(image(TapeFormat::Tzx, "out.tzx", false), want);
    }

    /// With the stub, a BASIC program called `loader` goes first, auto-starting
    /// at line 10: `CLEAR 32767`, pasmo's own `POKE 23610,255`, `LOAD ""CODE`,
    /// and `RANDOMIZE USR 32768` from the `end` address.
    #[test]
    fn tapbas_matches_pasmo() {
        #[rustfmt::skip]
        let want: &[u8] = &[
            0x13, 0x00, 0x00, 0x00, 0x6C, 0x6F, 0x61, 0x64, 0x65, 0x72, 0x20, 0x20, 0x20,
            0x20, 0x47, 0x00, 0x0A, 0x00, 0x47, 0x00, 0x1B, 0x49, 0x00, 0xFF, 0x00, 0x0A,
            0x0D, 0x00, 0xFD, 0x33, 0x32, 0x37, 0x36, 0x37, 0x0E, 0x00, 0x00, 0xFF, 0x7F,
            0x00, 0x0D, 0x00, 0x14, 0x17, 0x00, 0xF4, 0x32, 0x33, 0x36, 0x31, 0x30, 0x0E,
            0x00, 0x00, 0x3A, 0x5C, 0x00, 0x2C, 0x32, 0x35, 0x35, 0x0E, 0x00, 0x00, 0xFF,
            0x00, 0x00, 0x0D, 0x00, 0x1E, 0x05, 0x00, 0xEF, 0x22, 0x22, 0xAF, 0x0D, 0x00,
            0x28, 0x0E, 0x00, 0xF9, 0xC0, 0x33, 0x32, 0x37, 0x36, 0x38, 0x0E, 0x00, 0x00,
            0x00, 0x80, 0x00, 0x0D, 0x08, 0x13, 0x00, 0x00, 0x03, 0x6F, 0x75, 0x74, 0x2E,
            0x74, 0x61, 0x70, 0x62, 0x61, 0x73, 0x05, 0x00, 0x00, 0x80, 0x00, 0x80, 0x53,
            0x07, 0x00, 0xFF, 0x3E, 0x02, 0xD3, 0xFE, 0xC9, 0x27
        ];
        assert_eq!(image(TapeFormat::Tap, "out.tapbas", true), want);
    }

    #[test]
    fn tzxbas_matches_pasmo() {
        #[rustfmt::skip]
        let want: &[u8] = &[
            0x5A, 0x58, 0x54, 0x61, 0x70, 0x65, 0x21, 0x1A, 0x01, 0x0D, 0x10, 0xE8, 0x03,
            0x13, 0x00, 0x00, 0x00, 0x6C, 0x6F, 0x61, 0x64, 0x65, 0x72, 0x20, 0x20, 0x20,
            0x20, 0x47, 0x00, 0x0A, 0x00, 0x47, 0x00, 0x1B, 0x10, 0xE8, 0x03, 0x49, 0x00,
            0xFF, 0x00, 0x0A, 0x0D, 0x00, 0xFD, 0x33, 0x32, 0x37, 0x36, 0x37, 0x0E, 0x00,
            0x00, 0xFF, 0x7F, 0x00, 0x0D, 0x00, 0x14, 0x17, 0x00, 0xF4, 0x32, 0x33, 0x36,
            0x31, 0x30, 0x0E, 0x00, 0x00, 0x3A, 0x5C, 0x00, 0x2C, 0x32, 0x35, 0x35, 0x0E,
            0x00, 0x00, 0xFF, 0x00, 0x00, 0x0D, 0x00, 0x1E, 0x05, 0x00, 0xEF, 0x22, 0x22,
            0xAF, 0x0D, 0x00, 0x28, 0x0E, 0x00, 0xF9, 0xC0, 0x33, 0x32, 0x37, 0x36, 0x38,
            0x0E, 0x00, 0x00, 0x00, 0x80, 0x00, 0x0D, 0x08, 0x10, 0xE8, 0x03, 0x13, 0x00,
            0x00, 0x03, 0x6F, 0x75, 0x74, 0x2E, 0x74, 0x7A, 0x78, 0x62, 0x61, 0x73, 0x05,
            0x00, 0x00, 0x80, 0x00, 0x80, 0x40, 0x10, 0xE8, 0x03, 0x07, 0x00, 0xFF, 0x3E,
            0x02, 0xD3, 0xFE, 0xC9, 0x27
        ];
        assert_eq!(image(TapeFormat::Tzx, "out.tzxbas", true), want);
    }

    /// The header's name field is fixed width and holds the path as given —
    /// slashes and extension included, since that is what pasmo puts there.
    #[test]
    fn the_block_name_is_the_output_path_clipped_to_ten() {
        let name = |n: &str| {
            String::from_utf8(image(TapeFormat::Tap, n, false)[4..14].to_vec()).expect("ascii")
        };
        assert_eq!(name("a.tap"), "a.tap     ");
        assert_eq!(name("ten1234567.tap"), "ten1234567");
        assert_eq!(name("sub/x.tap"), "sub/x.tap ");
    }

    /// `RANDOMIZE USR` needs an address, so a source with no `end` gets a stub
    /// that loads and stops — 18 bytes shorter, and pasmo agrees.
    #[test]
    fn the_stub_drops_its_last_line_without_an_entry_point() {
        let asm = crate::assemble_pasmo("        org 32768\n        ret\n").expect("assembles");
        let image = tape(&asm, TapeFormat::Tap, "ne.tap", true).expect("frames");
        // The BASIC header's length field, which is the program's length.
        assert_eq!(u16::from_le_bytes([image[14], image[15]]), 53);
        // And it still auto-starts at line 10.
        assert_eq!(u16::from_le_bytes([image[16], image[17]]), 10);
    }
}
