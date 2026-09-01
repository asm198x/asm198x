# Decision: sjasmplus Lua is supported behind a build feature, sandboxed by default

**Status:** Active. Binding for Asm198x (accepted 2026-09-01). Closes
[#491](https://github.com/asm198x/asm198x/issues/491).

**Date:** 2026-09-01.

## The decision

Asm198x implements sjasmplus's `LUA`/`ENDLUA` and `INCLUDELUA` by embedding
the reference's own interpreter version, **Lua 5.4**, through the `mlua` crate
(`lua54` + `vendored`), behind a cargo feature named `lua`.

- **Library:** the feature is off by default. Without it the three words keep
  today's refusal: valid sjasmplus, not built here.
- **CLI release artifacts:** the feature is on. Someone assembling real
  sjasmplus source expects the reference's behaviour.
- **WASM builds:** the feature is off, because `mlua` supports only
  `wasm32-unknown-emscripten`, not `wasm32-unknown-unknown`. The playground
  refuses the words the way the default library does.

A script runs inside a sandbox that is closed by default and opens only on
evidence. Every loosening is prompted by a real source that needs it and is
recorded as a line in this file.

## Why

The words are in real sources ([#275](https://github.com/asm198x/asm198x/issues/275)),
and [`reference-parity-goal.md`](reference-parity-goal.md) holds that a
deferral needs a record. Refusing by name was the cheap option, but the
reference's Lua is not an extension bolted on the side: it is how sjasmplus
users generate tables, unroll loops and compute checksums, and a Spectrum
learner who copies such a source should see it assemble.

Embedding an interpreter cuts against the featherweight build discipline
(`packaging-and-cpu-roadmap.md`). The feature flag is what reconciles the
two: the default library pays nothing, and the artifact that carries the
interpreter is the one whose users asked for it.

`mlua` is the only viable embedding. It is current (0.12 in 2026-08), widely
used, and its calling API is safe Rust, so the workspace's
`unsafe_code = "forbid"` still holds for our own code. The pure-Rust
alternatives are either incomplete Lua 5.4 subsets or unmaintained.

`vendored` compiles the reference Lua source at build time: no system Lua, no
dynamic linking, and every release target already has a C compiler.

## The sandbox

| Layer | Mechanism | What it guarantees |
|---|---|---|
| Libraries | `Lua::new_with(STRING \| TABLE \| MATH \| UTF8 \| COROUTINE)` | No `io`, `os`, `package`/`require`, `debug`; mlua's safe mode also refuses C modules, and the `load` family is limited to text chunks. |
| Memory | `set_memory_limit` | A script cannot exhaust the assembler or a server hosting it. |
| Time | `set_hook` on an instruction count | A deterministic budget. A wall-clock timeout would make a failing assembly irreproducible — the same reasoning as [`clock-dependent-directives.md`](clock-dependent-directives.md). |
| Host access | the `sj` table is bound to our engine | `sj.file_exists` goes through the `SourceLoader`; Lua sees the assembly, never the machine. |

Excluding `os` also keeps the no-wall-clock decision intact under Lua:
`os.time`, `os.date` and `os.clock` are not there to read.

### Refused by name, inside the sandbox

- `sj.shellexec` and the `SHELLEXEC` directive — arbitrary process execution.
- The `zx.*` snapshot and TRD-image writers — [`assemble-io-model.md`](assemble-io-model.md)
  scopes output to native containers; if a container graduates to Format198x
  the writer can follow, as `SAVETAP` did.
- `sj.exit` is honoured as an assembly error with the script's message, not
  as a process exit.

### One recorded deviation from the reference

Lua 5.4 seeds `math.random` from the clock and an address at startup, so the
reference is nondeterministic there. Asm198x seeds it with a fixed value.
Source that depends on the reference's randomness assembles to different
bytes on the reference *every run*, so no byte-identical verdict is being
given up.

### The budget defaults

Memory and instruction limits are set generously enough that no real source
in the corpus reaches them, and named in the diagnostic when one does. Their
values are recorded in the implementation, not here, because they are tuned
against the corpus rather than decided.

## What this does not settle

The binding is the easy half. Fidelity of the **pass model** is the work:
`PASS1`/`PASS2`/`PASS3`/`ALLPASS` against our convergence loop,
`sj.parse_line` re-entering the assembler mid-line, and `sj.get_byte`
reading device memory, which depends on
[#318](https://github.com/asm198x/asm198x/issues/318). Each of those is
probe-pinned against the installed reference before it counts as done, the
way every other sjasmplus word has been.

## Rejected alternatives

- **Refuse by name, permanently.** Honest, but it leaves a class of real
  Spectrum sources unassemblable for a reason users would not accept: the
  reference does it, and the cost is a build feature.
- **A pure-Rust Lua.** None is complete for 5.4 and maintained; version
  fidelity to the reference matters because scripts are written against 5.4's
  integer semantics and standard library.
- **Always-on embedding.** Breaks the featherweight default and the WASM
  target for every user, to serve the subset who write Lua.
- **A wall-clock timeout.** Rejected for the reason recorded above.
- **`os` with a fixed clock.** Silently changes the meaning of `os.time()`;
  the same objection as emitting a constant timestamp for `dtb`.
