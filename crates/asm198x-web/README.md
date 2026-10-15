# asm198x-web

The [asm198x](https://github.com/asm198x/asm198x) assembler compiled to
WebAssembly, with a `wasm-bindgen` surface a web page can call. Nothing is
uploaded anywhere: the assembler runs in the visitor's tab.

The published npm package is **`@asm198x/z80`**, currently version **0.4.0**.
It assembles Z80/Z80N using `pasmo`, `pasmonext` and `sjasmplus` syntax
(`sjasm` is an alias). Its version is separate from the native CLI's.

The all-architecture `@asm198x/web` bundle and other architecture bundles can
be built locally; they are not published installation choices. The crate itself
stays off crates.io: a `wasm-bindgen` surface returning JavaScript values has
no Rust caller.

```sh
npm install @asm198x/z80@0.4.0
```

## Surface

```js
import init, { assemble, assemble_project, snapshot, listing, dialects } from '@asm198x/z80';

await init();

const result = JSON.parse(assemble('sjasmplus', ' org 32768\n ld a,1\n ret\n'));
if (Array.isArray(result)) {
  // The diagnostics array: `result[0].message`, `result[0].span.line`.
} else {
  // The `AssemblyResult` object: `result.bytes`, `result.origin`, `result.symbols`.
}

listing('pasmo', 'org $8000\n ld a,1\n ret\n'); // the `--listing` text, or undefined
JSON.parse(dialects());                        // [{ name, aliases, blurb }, …]
```

`assemble` returns the string `asm198x --message-format=json` prints, so the
[CLI reference](https://asm198x.github.io/docs/cli/) documents the shape.
It returns `undefined` for a dialect name this build does not list.
The example above runs in a browser/bundler that serves the package's `.wasm` file.

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
  output: 'raw',              // optional; the published Z80 package emits raw bytes
})));
```

The file map is text: source includes are supported, while binary `incbin`
payloads remain outside this surface. A locally built bundle containing vasm
also accepts `output: 'hunk'`; it is not an option in the published Z80 package.
Filesystem-only CLI behaviour is not exposed.

## Verify the installed package in Node

After the install command above, save this as `verify.mjs` in the same directory
and run `node verify.mjs`. Node initializes the published WASM bytes explicitly;
a browser normally fetches them through `init()`.

```js
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import init, { assemble } from '@asm198x/z80';

await init({ module_or_path: await readFile(new URL(
  './node_modules/@asm198x/z80/asm198x_web_bg.wasm', import.meta.url
)) });
const result = JSON.parse(assemble('pasmo', 'org $8000\n ld a,7\n halt\n'));
assert.deepEqual(result.bytes, [0x3e, 7, 0x76]);
const errors = JSON.parse(assemble('pasmo', 'org $8000\n ld a,missing_symbol\n'));
assert(Array.isArray(errors) && errors.some(error => error.severity === 'Error'));
console.log('Published package assembled the expected bytes and diagnosed invalid input');
```

## Build locally

`scripts/build-npm.sh` can build the all-architecture `@asm198x/web` bundle
or select architectures. Those build names do not imply registry publication.

```sh
wasm-pack build --target web crates/asm198x-web --out-dir pkg-web
wasm-pack test --node crates/asm198x-web
```

The crate is excluded from the workspace, so it carries its own `Cargo.lock`;
`cargo update -p asm198x --manifest-path crates/asm198x-web/Cargo.toml`
refreshes the path entry after the library's version moves.

## Running what you assembled

`snapshot(dialect, source)` returns a 48K `.sna` — the same bytes
`asm198x --dialect pasmonext --sna` writes, verified byte for byte — so a page
can assemble and then run the result:

```js
const sna = snapshot('pasmonext', source);
if (sna) spectrum.loadSnapshot(sna, 'sna');   // @emu198x/zx-spectrum
```

Spectrum Z80 only, and the source needs `end <addr>` for its entry point,
exactly as the command line demands. `undefined` means it did not assemble;
`assemble()` says why.

## Size: take one architecture

A locally built `@asm198x/web` carries every dialect. The published
`@asm198x/z80` contains only the Z80 family; other architecture selections
can be built locally. These are compile-time
feature selections from the same assembler, with no runtime plugins. See the
[browser packaging decision](../../decisions/browser-builds-use-features.md).

The recorded 2026-09-04 measurements, using `opt-level = "z"` and wasm-opt,
illustrate the saving. Sizes change as the assembler grows; these are a
baseline, not a measurement of every subsequent release:

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

A lesson page should load the assembler when the learner starts editing.
Code198x already imports `@asm198x/z80` lazily for its Spectrum lessons.
