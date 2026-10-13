# Preserve native output with JSON reporting (#619)

JSON reporting currently returns before CLI-selected TAP/TZX, SNA and PRG
validation and framing, writing the raw assembly bytes to the requested image.
The source-built diagnostic reproduces this: 54 bytes instead of a 175-byte TAP.

1. Add CLI regression cases in `crates/asm198x/tests/cli_json.rs` comparing human
   and JSON artifact bytes for TAP/TZX (with and without BASIC), SNA and PRG.
   Check JSON still reports machine code, invalid dialect/container pairs fail,
   and framing errors do not write an image. Run against unchanged code first.
2. In `crates/asm198x/src/main.rs`, validate native switches before selecting the
   reporting path and apply the existing serializers in JSON mode. Preserve the
   JSON result shape and the existing no-output behavior for plain JSON queries.
3. Run CLI regressions, existing output-format tests, formatting and Clippy,
   both default/Lua and no-default-feature build paths. Compare the real
   qualification diagnostic with Pasmo. Open a focused PR and link #619.

Scope: CLI-selected containers only. Vasm EXE and ca65 NES already assemble to
native bytes in the JSON route; Game Boy ROM explicitly rejects JSON. Do not
change the library's `AssemblyResult.bytes` contract or add dependencies.
