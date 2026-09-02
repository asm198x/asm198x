# asm198x-web

The [asm198x](https://github.com/asm198x/asm198x) assembler compiled to
WebAssembly, with a `wasm-bindgen` surface a web page can call. Nothing is
uploaded anywhere: the assembler runs in the visitor's tab.

This crate is not published. It is built by the site that embeds it.

## Surface

```js
import init, { assemble, listing, dialects } from './asm198x_web.js';

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
