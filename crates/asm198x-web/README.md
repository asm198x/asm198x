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
import init, { assemble, snapshot, listing, dialects } from '@asm198x/web';

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

One source file per call; an `include` reports "file not found" as a
diagnostic. The CLI's `--cpu` switch (sjasmplus for the Spectrum Next) is not
exposed yet.

## Build

```sh
wasm-pack build --target web crates/asm198x-web --out-dir pkg-web
wasm-pack test --node crates/asm198x-web
```

The crate is excluded from the workspace, so it carries its own `Cargo.lock`;
`cargo update -p asm198x --manifest-path crates/asm198x-web/Cargo.toml`
refreshes the path entry after the library's version moves.

## Size

Every dialect ships in one module. Measured 2026-09-02, `opt-level = "z"`,
after wasm-opt: 1,451,743 bytes raw, 490,892 gzipped. A per-platform split
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

## Size

The wasm carries every dialect the assembler supports: 1.4 MB raw, 480 KB
gzipped, 368 KB brotli. That is a lot to put in front of a reader who may never
edit anything, so a page that also embeds an emulator should load this lazily —
on the first keystroke, not on page load.
