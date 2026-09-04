# asm198x-web

The [asm198x](https://github.com/asm198x/asm198x) assembler compiled to
WebAssembly, with a `wasm-bindgen` surface a web page can call. Nothing is
uploaded anywhere: the assembler runs in the visitor's tab.

Published to npm as **`@asm198x/web`**; `scripts/build-npm.sh` builds it. The
crate itself stays off crates.io: a `wasm-bindgen` surface returning JavaScript
values has no Rust caller.

```sh
npm install @asm198x/web
```

## Surface

```js
import init, { assemble, assemble_project, snapshot, listing, dialects } from '@asm198x/web';

await init();

const result = JSON.parse(assemble('sjasmplus', ' org 32768\n ld a,1\n ret\n'));
if (Array.isArray(result)) {
  // The diagnostics array: `result[0].message`, `result[0].span.line`.
} else {
  // The `AssemblyResult` object: `result.bytes`, `result.origin`, `result.symbols`.
}

listing('acme', '*=$c000\n lda #1\n rts\n');  // the `--listing` text, or null
JSON.parse(dialects());                        // [{ name, aliases, blurb }, …]
```

`assemble` returns the string `asm198x --message-format=json` prints, so the
[CLI reference](https://asm198x.github.io/docs/cli/) documents the shape.
It returns `null` for a dialect name the table does not list.

For a named in-memory project, including target selection and the browser-safe
native output options:

```js
const result = JSON.parse(assemble_project(JSON.stringify({
  dialect: 'sjasmplus',
  target: 'z80n',             // optional: z80, z80n/next
  root: 'src/main.asm',
  files: {
    'src/main.asm': ' include "part.asm"\n',
    'part.asm': ' nextreg $07,$02\n',
  },
  output: 'raw',              // optional; 'hunk' is available for vasm
})));
```

The file map is text: source includes are supported, while binary `incbin`
payloads remain outside this surface. `hunk` is included because it is wholly
represented by the returned bytes; filesystem-only CLI behaviour is not.

## Build

```sh
wasm-pack build --target web crates/asm198x-web --out-dir pkg-web
wasm-pack test --node crates/asm198x-web
```

The crate is excluded from the workspace, so it carries its own `Cargo.lock`;
`cargo update -p asm198x --manifest-path crates/asm198x-web/Cargo.toml`
refreshes the path entry after the library's version moves.

## Size

Every dialect ships in one module. Measured 2026-09-04, `opt-level = "z"`,
after wasm-opt: 1,505,582 bytes raw, 514,986 gzipped. A per-platform split
(#495) is a decision for when a consumer needs one.

## Running what you assembled

`snapshot(dialect, source)` returns a 48K `.sna` — the same bytes
`asm198x --dialect pasmonext --sna` writes, verified byte for byte — so a page
can assemble and then run the result:

```js
const sna = snapshot('pasmonext', source);
if (sna) spectrum.loadSnapshot(sna, 'sna');   // @emu198x/zx-spectrum
```

Spectrum Z80 only, and the source needs `end <addr>` for its entry point,
exactly as the command line demands. `null` means it did not assemble;
`assemble()` says why.

## Size: take one architecture

`@asm198x/web` carries every dialect — 1.4 MB raw, 480 KB gzipped. Almost none
of that is useful to a page teaching one machine, so the package is also built
per CPU architecture:

| package | raw | gzipped |
|---|---|---|
| `@asm198x/web` (all) | 1470 KB | 503 KB |
| `@asm198x/z80` | 486 KB | 185 KB |

`entry` is the only thing that names the library's assembler entry points, so a
build that does not select an architecture never references it and the linker
drops it. Nothing is stripped by hand.

```sh
scripts/build-npm.sh z80        # -> @asm198x/z80
scripts/build-npm.sh mos6502    # -> @asm198x/mos6502
scripts/build-npm.sh            # -> @asm198x/web, everything
```

`dialects()` reports what the build you have can actually assemble, not the
whole table, so a picker made from it can never offer a name `assemble` then
refuses.

Even at 185 KB this is bigger than an emulator embed, so a lesson page should
still load it lazily — on the first keystroke, not on page load.
