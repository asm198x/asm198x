# SjASMPlus symbol maps

```asm
 DEVICE ZXSPECTRUM128
 ORG $C010
 PAGE 1
draw: db 1
 PAGE 3
 ORG $C010
music: db 3
answer EQU $C010
 CSPECTMAP "game.map"
 LABELSLIST "game.lbl"
```

Both directives require an active `DEVICE`. Each requests an artifact from
the final symbol table; the last request of each format wins. The library
returns their bytes without writing files. The human CLI writes them beside
the output image, or under `--outprefix`. Absolute paths are honoured.
As with other source-requested artifacts, JSON mode describes these files in
`artifacts` without writing them. This differs from explicit CLI `--sym`
exports, which are written in both reporting modes.

`CSPECTMAP` with no filename uses the requesting source filename plus `.map`.
The single-string library entry has no filename and uses `input.asm.map`.
`LABELSLIST` requires a filename; `LABELSLIST "game.lbl",1` selects virtual
CPU addresses instead of bank offsets. The option must be known when parsed.
Quote filenames before a comma: native SjASMPlus treats `game.lbl,1` without
quotes or intervening whitespace as a literal filename.

## Layout and compatibility

The writer follows [SjASMPlus 1.21.0's symbol-table exports](https://github.com/z00m128/sjasmplus/blob/v1.21.0/sjasm/tables.cpp)
and [directive handling](https://github.com/z00m128/sjasmplus/blob/v1.21.0/sjasm/directives.cpp).
Rows are sorted by source name rather than native hash-table order.

CSPECTMAP carries eight-digit hexadecimal CPU and physical addresses, a
two-digit kind, and an uppercased name. Address labels retain their defining
page; equal-valued constants remain constants. Structure definitions have a
distinct kind. A local label's rightmost recognised parent separator becomes
`@`. The physical address uses the page size at the final CSPECTMAP request.

LABELSLIST retains names and includes constants. Its mask uses the device
active at assembly's end. Spectrum 48 pages 1, 2 and 3 become Spectrum 128
banks 5, 2 and 0; ROM and virtual addresses have an empty bank prefix.
Native page fields are decimal, truncated to eight bits.

Consumer evidence has specific boundaries:

- CSpect 3.1.0.0's actual map loader resolves `DRAW` and `MUSIC` at the same
  CPU address `$C010` but physical addresses `$4010` and `$C010`, and retains
  `ANSWER` as a constant. The check invokes its loader and lookup methods;
  it does not exercise the GUI or whole-machine MMU switching.
- UnrealSpeccy's pinned loader accepts the generated banks 1 and 3. That
  version reads bank fields as hexadecimal, unlike SjASMPlus's decimal
  writer, and does not accept its empty-bank virtual syntax. This test does
  **not** establish compatibility for banks above 9 or virtual labels.

These exports do not change Debug198x's flat-section projection. Consumers
needing paged placement should use `symbol_pages`, not flat section offsets.

## Repeating the checks

Always-on library, formatter and CLI coverage:

```sh
cargo test -p asm198x --test sjasm_symbol_exports
```

The ignored differential test requires `sjasmplus` 1.21.0 on PATH. It compares
bytes and sorted rows for 48K, 128K and Next devices, remapping, constants,
local names, structures, virtual labels, default filenames and replacement
requests.

For CSpect, use .NET SDK 10 and the separately obtained CSpect 3.1.0.0
distribution. No CSpect binaries are redistributed here. From the repository
root:

```sh
dotnet build crates/asm198x/tests/support/cspect/Consumer.csproj \
  --artifacts-path /tmp/asm198x-cspect-probe
export CSPECT_SYMBOL_PROBE=/tmp/asm198x-cspect-probe/bin/Consumer/debug/Consumer.dll
export CSPECT_EXE=/path/to/CSpect3_1_0_0/CSpect.exe
```

The adapter refuses any executable whose SHA-256 is not
`0fa35824c6170eeffa723797a9451a0c0d558f315b5412f080e9ee6791f9a6f0`.
Its reflection tokens are specific to that binary.

For UnrealSpeccy, obtain `dbglabls.cpp` and `util.cpp` from
[commit 9dae221c92966104facb600f96d2894c58414299](https://github.com/mkoloberdin/unrealspeccy/tree/9dae221c92966104facb600f96d2894c58414299).
Verify SHA-256 before extracting these exact unmodified loader/storage and
hex-helper methods:

| File | SHA-256 |
|---|---|
| `dbglabls.cpp` | `37e6231789eff367883fc4dff513db61bce08066f54573a06bdf8d7f7e6613c4` |
| `util.cpp` | `d038278534dd994f4601c726e0bce47d3c857c321de8fd24e8b8dc3c0e26bdce` |

With those two files in the current directory, set `ASM198X_REPO` to this
checkout's absolute path:

```sh
(sed -n '110,120p' util.cpp; sed -n '42,162p' dbglabls.cpp) |
  clang++ -std=c++17 -x c++ \
    -include "$ASM198X_REPO/crates/asm198x/tests/support/unreal_symbols.hpp" \
    - -o /tmp/asm198x-unreal-symbols
export UNREAL_SYMBOL_PROBE=/tmp/asm198x-unreal-symbols
```

The adapter supplies memory and checks the consumer's stored names and
addresses; it does not replace the parser. The original code produces an
empty-loop warning with Clang. Once both adapters are built:

```sh
cargo test -p asm198x --test sjasm_symbol_exports -- --include-ignored
```
