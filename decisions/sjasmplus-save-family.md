# Decision: the sjasmplus `SAVE*` family, word by word

**Status:** Active. Binding for Asm198x (accepted 2026-09-01). Refines the
"`save*` family" row of the deferral register in
[`reference-parity-goal.md`](reference-parity-goal.md) into one entry per word,
under the mechanism and the canon rule
[`multi-artifact-output.md`](multi-artifact-output.md) already settles.

**Date:** 2026-09-01.

## The decision

1. **Every word is accepted.** `multi-artifact-output.md` accepts the family;
   nothing here narrows that. What this record adds is the basis each word
   waits on, so the register names a fact rather than "the same".
2. **A word lands when its layout has an upstream.** The container's byte
   layout must be held in `reference/` or `syntheses/` before the serialiser is
   written, and the serialiser cites it. sjasmplus's own output is the
   *verdict* the differential checks against, never the source the layout is
   taken from.
3. **A community standard held in the library is an upstream.** `formats.md`
   already carries the TZX specification at role `community-standard`, and
   `SAVETAP` cites it. A layout resting on community material that the library
   holds (Spectrumpedia's SNA coverage) is citable, with the front matter's
   provenance warning carried into the serialiser's doc comment. A layout the
   library holds nothing for is not, and the word waits for acquisition.
4. **Until it lands, a word is refused by name, with its basis.** The message
   says which fact is missing and points here, as `SAVEBIN` does for a span
   that reaches outside what the source assembled
   ([#318](https://github.com/asm198x/asm198x/issues/318)).
5. **Device gates are the reference's, verbatim.** Each word is refused
   outside the devices sjasmplus accepts it in, with sjasmplus's message.

## The words

Every row was probed against sjasmplus 1.21.0 on 2026-09-01; the syntax and
device columns are what the binary answered, not what its documentation says.

| Word | Syntax as probed | Device gate | Container | Upstream | Waits on |
|---|---|---|---|---|---|
| `SAVEBIN` | `"file",start[,length]` | any device | raw span | — | **done** (#316, #318), including unwritten device memory |
| `SAVETAP` | `"file",CODE\|BASIC,"name",start,length` | Spectrum devices | TAP | `syntheses/zx-spectrum/tape-loading-format.md` §4, via `format198x-sinclair-zx-spectrum-tap` | **done** for the kinded forms; the kindless whole-memory grammar remains |
| `SAVE3DOS` | `"file",start,length` | any device | span with a 128-byte +3DOS header | `reference/by-system/sinclair-zx-spectrum/zx-spectrum-plus-3-manual-amstrad.txt` (the +3DOS header record) | nothing — next |
| `SAVEAMSDOS` | `"file",start,length` | any device | span with a 128-byte AMSDOS header | **partly held** — the CPC464 firmware guide documents the cassette file-header record and does not mention AMSDOS; the disk header's extension and checksum are not held | acquisition of the AMSDOS header layout (the DDI-1 firmware guide) |
| `SAVEDEV` | `"file",startPage,startOffset,length` | any device | raw device pages, no container | the device model (`docs/sjasmplus-device-model.md`) | unblocked: #318 supplies initial pages and #563 retains routed writes |
| `SAVESNA` | `"file"[,start]` | `ZXSPECTRUM48` / `ZXSPECTRUM128` only (`[SAVESNA] Device must be ZXSPECTRUM48 or ZXSPECTRUM128.`) | 48K SNA (27-byte header + 49,152) or 128K SNA | `formats.md` §SNA — community material only, per rule 3 | unblocked by #318; serializer remains |
| `SAVECPCSNA` | `"file"[,start]`; `[SAVECPCSNA] No start address defined` without one | `AMSTRADCPC464` / `AMSTRADCPC6128` only | CPC SNA (256-byte header + 65,536) | `reference/by-system/amstrad-cpc/formats.md` §SNA — community | unblocked by #318's measured zero seed; serializer remains |
| `SAVEHOB` | `"file","hobname",start,length` | any device | Hobeta (17-byte header, data padded to sectors: 273 bytes for a 1-byte span) | **none held** — Spectrumpedia mentions the format, no layout | acquisition |
| `SAVETRD` | `"image","name.C",start,length[,autostart]`; the image must already exist (`Error opening file`), which `EMPTYTRD` creates | any device | file appended to a TR-DOS disk image | **none held** — `formats.md` names TRD in its overview and has no section | acquisition; lands together with `EMPTYTRD` |
| `SAVECDT` | `FULL\|EMPTY\|BASIC\|CODE\|HEADLESS "file","name",start,length` | `AMSTRADCPC464` / `AMSTRADCPC6128` only | CDT (TZX with CPC block usage) | `reference/by-system/amstrad-cpc/formats.md` §CDT, which its own front matter flags as having no specification held | acquisition of the CDT block usage; the TZX container is held |
| `SAVECPR` | `"file",size` with size 1–32 in 16 KiB units (`only a size from 1 (16KiB) to 32 (512KiB) is allowed`) | `AMSTRADCPCPLUS` only | CPR (RIFF `AMS!` cartridge: 16,404 bytes for one bank) | `reference/by-system/amstrad-cpc/formats.md` §CPR — community | **done** (#563); the device itself landed with #538, and an empty Plus cartridge is all zeros, so #318 does not apply |
| `SAVENEX` | `OPEN\|CORE\|CFG\|BAR\|SCREEN\|BANK\|AUTO\|CLOSE …` — a stateful builder across several lines | `ZXSPECTRUMNEXT` only | NEX | **none held** — `reference/by-system/next-computer/` is NeXT, not the Spectrum Next | acquisition of the NEX specification, then its own record: it is a grammar, not one directive |

Words beside the family that share its basis and land with it: `EMPTYTRD`
(with `SAVETRD`), `EMPTYTAP`/`TAPOUT`/`TAPEND` (with the TAP serialiser
already held), `INCHOB`/`INCTRD` (readers of the same two layouts).

## The order

Ordered by what is already held, not by demand:

1. `SAVE3DOS` — upstream held, no dependency.
2. `SAVEDEV`, `SAVESNA`, `SAVECPCSNA` — now unblocked: #563 supplied routed
   device memory and #318 supplied its initial contents. Their serializers are
   the remaining work. `SAVECPR` landed with #563.
3. `SAVEAMSDOS`, `SAVEHOB`, `SAVETRD`/`EMPTYTRD`, `SAVECDT` — after
   acquisition.
4. `SAVENEX` — after acquisition, under its own record.

## Why not transcribe the layouts from the reference's output

Each of these formats is read by Emu198x, mastered by Build198x and identified
by Cat198x. A layout worked out from sjasmplus's bytes would be a fourth,
private reading of a shared fact, and would drift from the others without
anyone noticing. `multi-artifact-output.md` § "The formats are the family's"
binds this; it is restated here only because the temptation is per-word — each
one looks small enough to do at the keyboard.

## Drift triggers

- "sjasmplus writes N bytes, so the header is N bytes" — that is transcription.
- "we can zero-fill the snapshot and fix it later" — #318 says the right answer
  is not zeros; a snapshot with the wrong system variables loads and misbehaves.
- "SAVENEX is just another save word" — it is eight sub-commands with state
  between them.
- "page N is the section `PAGE N` opened" — it is not. The section orders the
  raw output; the page holds whatever was written at an address routed to it,
  under whichever `SLOT`/`PAGE` mapping was in force at the time (#563).
