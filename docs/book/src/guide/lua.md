# SjASMPlus Lua

Native release binaries include Lua 5.4 for SjASMPlus sources. Library users
opt in with the `lua` Cargo feature. The default library and browser build
recognise `LUA`, `ENDLUA` and `INCLUDELUA`, but report that the feature is absent.

```asm
    org $8000
    LUA ALLPASS
        for i = 0, 255 do
            sj.add_byte(math.floor(math.sin(math.pi * i / 128) * 15.5))
        end
    ENDLUA
```

Use `ALLPASS` when emitting code: it runs on each of the three assembler passes.
`PASS1`, `PASS2` and `PASS3` select one pass; omitting the mode means `PASS3`.
Lua globals survive between blocks and passes. `INCLUDELUA "helpers.lua"` loads
through the ordinary source loader on pass 1, sharing the interpreter.
Untaken assembly branches neither run nor load Lua.

The `sj` interface supports expression evaluation (`calc` / `_c`), immediate
assembly (`parse_line` / `_pl`, `parse_code` / `_pc`), labels and defines,
`current_address`, byte/word emission, module lookup, diagnostics and
loader-mediated `file_exists`. `get_define(name, true)` can read the active
macro's arguments. Generated source goes through the normal parser and encoder.

With a `DEVICE`, scripts can select slots/pages and inspect live byte/word
memory on pass 3. Earlier passes return zero for memory reads. Bank selection
and backwards `ORG` affect these reads just as they affect `SAVEBIN`.
`sj.get_page_at(address)` exposes the current mapping; this requested API is
an extension to the pinned 1.21.0 Lua bindings. Custom `set_device` RAMTOP
values are explicitly refused rather than ignored.

## Sandbox and limits

String, table, math, UTF-8 and coroutine libraries are available. `load` accepts
text, including reader functions, but never bytecode. There is no `io`, `os`,
`package`, `require`, `debug`, `dofile` or `loadfile`. Process execution and the
`zx` file writers are refused. `sj.exit` fails the assembly, not the process;
catching a host error with `pcall` does not make a failed assembly succeed.
`print` becomes an assembly note instead of writing directly to stdout.

The interpreter has a 32 MiB memory limit and a ten-million-instruction budget
shared across all passes and coroutines. Generated assembler work is charged
too, with a separate cumulative 32 MiB host-allocation budget. Diagnostics name
the exhausted budget. Randomness starts from a fixed seed, including a call to
`math.randomseed()` without arguments; explicit seeds remain available.

Lua that emits machine code outside `ALLPASS` produces a `luamc` warning.
If that is deliberate, a `; luamc-ok` comment on `LUA` or `ENDLUA` acknowledges
it, following SjASMPlus.
