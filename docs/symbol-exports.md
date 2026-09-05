# Emulator symbol exports

```sh
asm198x --dialect acme --sym --sym-format=vice program.asm
asm198x --dialect rgbasm --sym --sym-format=nocash game.asm
```

VICE output defaults to `program.vs`; NO$-style output defaults to `game.sym`.
Use `--sym=path` to choose a different filename. `--sym-format` also accepts a
separate argument. The default is `native`, which preserves the existing
`--sym` rendering, including constants. Human and JSON modes support the same
exports. `--debug` remains the independent Debug198x sidecar.

The emulator formats export address labels and entry points, not constants.
They preserve case and sort by exported name. A format is requested explicitly:
there is no filename-based inference. A non-native format requires `--sym`.
The library's `render_symbol_export` returns text or a typed error and never
writes files; the CLI applies the existing source/image path protections.

## Formats and limits

VICE uses `al C:0801 .Entry` monitor commands in the computer address space.
The required dot is added if absent. Already-dotted labels keep their spelling;
a collision after prefixing is an error. The exporter refuses banked or
relocatable locations, addresses outside 16 bits, unsupported names, and C64
register names such as `PC` that VICE will not accept as labels.
Captured Game Boy banks are refused even when the bank number is zero; use
the Game Boy format for those locations.

The Game Boy export uses `03:4010 Banked`, with the CPU address and bank
captured by RGBASM. Missing bank information is an error, not an assumed zero.
ROM and RAM bank numbers are interpreted with their CPU address ranges; they
are not one linear physical address. The current writer supports the ASCII
name alphabet in the RGBDS specification; Unicode escapes are not emitted.

These layouts are defined by the [VICE monitor manual](https://vice-emu.sourceforge.io/vice_12.html)
and [RGBDS symbol-file specification](https://rgbds.gbdev.io/sym). The
repeatable tests exercise VICE 3.10's actual monitor and SameBoy's actual
symbol loader and expression evaluator, not parsers written for this project.

## Repeating the consumer checks

The always-on tests are `cargo test -p asm198x --test symbol_exports`.
The external consumers are opt-in ignored tests. Set `VICE_X64SC` to a VICE
binary (a headless build is sufficient), and optionally `VICE_DATADIR` to its
ROM data directory. The test imports generated labels and compares VICE's
saved symbol table. `+logcolorize` avoids VICE 3.10's null colour-log buffer
when stdout is captured.

Build the small SameBoy harness against the reference snapshot, from the
Asm198x repository root:

```sh
clang -std=gnu11 -O0 -D_GNU_SOURCE -DGB_INTERNAL \
  '-DGB_VERSION="probe"' '-DGB_COPYRIGHT_YEAR="2026"' \
  -I ../../emulators/gameboy/SameBoy -I ../../emulators/gameboy/SameBoy/Core \
  ../../emulators/gameboy/SameBoy/Core/*.c \
  crates/asm198x/tests/support/sameboy_symbols.c -o /tmp/asm198x-sameboy-symbols
```

Set `SAMEBOY_SYMBOL_PROBE` to that executable, then run:

```sh
cargo test -p asm198x --test symbol_exports -- --include-ignored
```

The harness loads the generated `.sym` and asks SameBoy to resolve `Banked`;
it requires bank 3 and CPU address `$4010`. None of these consumer dependencies
enter the assembler's library or distribution.

SjASMPlus `CSPECTMAP`/`LABELSLIST` and bank-aware Debug198x projection remain
separate parts of #503; these switches do not claim those formats.
