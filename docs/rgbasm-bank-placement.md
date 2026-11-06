# RGBASM bank placement

The engine carries a section's bank separately from its CPU base and its
position in the ROM file. These are different facts:

- `ROMX[$4010], BANK[3]` has CPU address `$4010` and ROM file offset `$C010`.
- `SRAM[$A010], BANK[2]` has CPU address `$A010` and no bytes in the ROM.
- `WRAMX[$D010], BANK[3]` has CPU address `$D010` and no bytes in the ROM.

ROM sections stay in their regions as plain `rgblink` requires: ROM0 in
`$0000-$3FFF` and ROMX in `$4000-$7FFF`. A section bigger than its region, a
pinned address outside it, or a pinned section that runs past its end is
refused with RGBLINK's own message, every one in declaration order. Floating
ROM0 sections go largest first, equal sizes in reverse declaration order, each
into the lowest gap that holds it; if one is left over, it is refused as
`Unable to place "name" (ROM0 section) anywhere`.

`--gb-rom --tiny` (the library's `RgblinkOptions { tiny: true }`) links as
`rgblink -t` does: one unbanked 32 KiB ROM. ROM0 runs to `$7FFF`. Each ROMX
section becomes a ROM0 section in bank 0, floating unless it pinned an address,
so `BANK("name")` is 0 and a floating one takes its turn in ROM0's layout. A
ROMX section in any bank but 1 is refused. The ROM is always 32 KiB, its gaps
`$00`, before `rgbfix -v -p 0xFF` finishes it.

Floating non-ROM placement reserves occupied ranges within each bank, not
across the whole region. Under the current bounded layout, an unqualified ROMX
or WRAMX section uses bank 1; the other region types use bank 0. Automatic
spilling into additional banks is not implemented.

`AssemblyResult.debug.symbol_banks` annotates the existing address symbols by
name. Its bank number must be interpreted with the CPU address, because ROM,
VRAM, SRAM, and work RAM have independent bank namespaces. It is not a linear
physical address and is not populated as SjASMPlus `symbol_pages` geometry.
Constants have no bank annotation. The field defaults to empty when reading an
older JSON result and is omitted when empty; `CONTRACT_VERSION` stays at 1.

Both the single-source and include-capable paths preserve bank identity through
the semantic AST, conditional evaluation, section placement, and pass-2 capture.
`BANK("section")` reads that same section identity, not a quotient of its file
offset. Attribute parsing starts after the quoted section name: text such as
`"BANK[9], name"` is a name, not a bank declaration or origin.

## Reference evidence

`tests/rgbasm_banks.rs` compares the complete linked ROM and every exported
symbol's bank and CPU address against RGBDS 1.0.3. It also covers RAM placement
in independent banks, named-section parsing, invalid banks, constants, old
JSON, includes, conditional sections, and formatter round trips.

Run the native comparison with:

```sh
cargo test -p asm198x --test rgbasm_banks -- --include-ignored --nocapture
```

The probe reports the tools' self-identities and executable SHA-256 digests.
The 2026-09-05 probe used:

- `rgbasm v1.0.3`: `152c559cf0973d17216970656be44e31463f0a338fdee78fc41159d77dc1f799`
- `rgblink v1.0.3`: `35ae0b400dd1b55b604405962c0cc60de261005db08dab728cded333f10fd71c`

Invalid-bank probes establish ROMX's 1–65535, WRAMX's 1–7, SRAM's 0–255,
and VRAM's 0–1 bounds. RGBDS rejects BANK attributes on ROM0, WRAM0, OAM,
and HRAM, including an explicit bank 0.

This capture supports #503's NO$-style export. The existing Debug198x rendering
still uses its flat section model; bank-aware sidecar projection and export
writers are separate work and must not infer banks from flattened offsets.
