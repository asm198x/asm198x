# Lua reference evidence

`lua_sin_table.asm` is copied unchanged from SjASMPlus **v1.21.0**:
<https://github.com/z00m128/sjasmplus/blob/v1.21.0/tests/lua_examples/lua_sin_table.asm>.
Its upstream BSD-3-Clause license is retained in `LICENSE.sjasmplus`.

`verdicts.ndjson` uses the normal verdict-corpus schema, including the reference
executable's version and SHA-256. It lives separately because replay requires
the optional `lua` feature; the default library and WASM deliberately refuse Lua.

Record observations with the installed 1.21.0 reference:

```sh
cargo test -p asm198x --features lua --test lua lua_reference_probes -- --ignored
```

Replay, including formatting invariants, needs no reference executable:

```sh
cargo test -p asm198x --features lua --test lua
```
