# Native controls for the 6502DTV and 65CE02 opcode fixtures

Measured on Linux x86-64 on 27 November 2026 for [#718](https://github.com/asm198x/asm198x/issues/718). The [arbiter image's existing cc65 pin](../../../ops/arbiter/Dockerfile) assembles and links both fixtures byte-for-byte against the upstream raw references. The fixtures come from [cc65 `e11fb5c`](https://github.com/cc65/cc65/tree/e11fb5c39371046ebe25485f984f644c5a0d65d3/test/asm/opcodes). The tool build uses [cc65 `8d86ca7`](https://github.com/cc65/cc65/commit/8d86ca7e7ed15300cc4bfaba61a40a3d00f3114d).

| Executable | Reported version | SHA-256 of local build |
|---|---|---|
| `ca65` | `ca65 V2.19 - Git 8d86ca7` | `286756de9531907b52fc8670455e0b97435084ad56e549bec60e7e2dbee8e173` |
| `ld65` | `ld65 V2.19 - Git 8d86ca7` | `5223b7d7b4ca2943924b08af197e21fc653efdb477a5e79b6b0e25fc20c32bd8` |
| `asm198x` | `asm198x v0.0.77` | `f73053b0858cb966547708aa3f53bcc3b4a04f763d7c82a2e00931cf0cf66560` |

Asm198x was built with `cargo build --locked -p asm198x --bin asm198x` from clean `7b164f96a43880efdda1cf29c0f3ed89917a1007`. The existing [linker adapter](../../../ecosystem/adapters/cc65-opcodes.cfg) had SHA-256 `e2ed542368d9e81f132e2305741d74a9572c5900818ccc8140a3d6d0677b5f43`.

| Fixture | Bytes | Upstream `.ref` and linked native SHA-256 | Asm198x result |
|---|---:|---|---|
| `6502dtv-opcodes` | 498 | `4e1f0e199bca200d03376a9f4d30034719d7f393c0be3bada7527f6d078e04d0` | Identical |
| `65ce02-opcodes` | 564 | `8a5fcde630e55dd49403f7c6ffed25d8786e65e3d0a2723531b7ca3e2c0509ec` | 16 byte differences. SHA-256 `093662dea1db52bbd853704a8c68dd8621895e2548fc0585bbc071a11db2b278` |

All 16 differences are the `BBR` and `BBS` displacements. The native listing places `bbr0 $02,*+$34` at offset 27. Its encoding is `0F 02 33` with the pinned cc65 and `0F 02 31` with Asm198x. The changed byte is `33` versus `31` at zero-based decimal offsets `29, 64, 100, 135, 165, 198, 231, 266, 304, 341, 376, 413, 448, 483, 521, 563`.

[cc65 `8aa1209`](https://github.com/cc65/cc65/commit/8aa120967aecf02a2383f1105e1167dc29d61fbb) fixes this exact `*` evaluation by reading all operands before emitting instruction bytes. A replay with [cc65 `2fa92f9`](https://github.com/cc65/cc65/commit/2fa92f96007ef62a6b1a3d09a582f8330413e895) produces the same two binaries as Asm198x from the same source. The updated `65ce02-opcodes.ref` at `2fa92f9` has the same SHA-256 as that replay and Asm198x.

## Reproduction

Build `ca65` and `ld65` from `8d86ca7` with `make ca65 ld65 BUILD_ID="Git 8d86ca7"`. Put them and the tested `asm198x` on `PATH`. Copy the `.s` and `.ref` files from `test/asm/opcodes` at `e11fb5c` to a temporary directory, then run from that directory with `adapter` set to the absolute path to `ecosystem/adapters/cc65-opcodes.cfg`.

```sh
for cpu in 6502dtv 65ce02; do
    ca65 --cpu "$cpu" -o "$cpu.o" "$cpu-opcodes.s" || exit 1
    ld65 -C "$adapter" -o "$cpu.native.bin" "$cpu.o" || exit 1
    cmp "$cpu-opcodes.ref" "$cpu.native.bin" || exit 1
    asm198x --dialect ca65 -C "$adapter" "$cpu-opcodes.s" -o "$cpu.asm198x.bin" || exit 1
    sha256sum "$cpu.native.bin" "$cpu.asm198x.bin"
done
cmp 6502dtv.native.bin 6502dtv.asm198x.bin
cmp -l 65ce02.native.bin 65ce02.asm198x.bin
```

The [ecosystem manifest](../../../ecosystem/corpus.json) still records the older `V2.18 - N/A` identities. Adopting the already-pinned image revision in the plans requires recapturing the linked ecosystem ledger, including the other cc65 targets that use those identities.
