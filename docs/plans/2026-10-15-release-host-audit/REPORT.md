# Distributed release host audit — 2026-10-15

I exercised all five official CLI 0.0.65 archives. The same six checks fail on
every architecture tested; they match defects already fixed on main. The
unchanged upstream Hello World build matches native ACME on every host. This
is evidence of consistent behavior across these hosts, including consistent
failures, rather than qualification of the release as defect-free.

Two new follow-ups are filed:

- [#667](https://github.com/asm198x/asm198x/issues/667): the browser README's
  `@asm198x/web` installation example returns public-registry HTTP 404.
- [#668](https://github.com/asm198x/asm198x/issues/668): make the small failure
  and output-safety corpus gate actual candidate archives before promotion.

## Release and method

CLI release: [`asm198x-v0.0.65`](https://github.com/asm198x/asm198x/releases/tag/asm198x-v0.0.65),
published 2026-10-14, annotated tag resolving to source
`fa06f5617f32bec52ac14f3a25bdd8e13dae9287`. All five downloaded archives match
the published checksum manifest, individual SHA-256 sidecars and release API
digests. [archive-inventory.json](archive-inventory.json) records their sizes,
archive hashes and extracted executable hashes. Matching a publisher's checksum
is integrity evidence, not an independent signature verification.

[probe.py](probe.py) executes bounded commands against extracted release
executables. On the four hosted runners it also requires the setup action's
installed executable to match the downloaded archive. The unchanged setup
smoke then uses that executable from PATH. The published shell/PowerShell
installer runs separately in unmanaged mode inside a temporary directory, with
profile and receipt writes disabled; its installed binary must match the
archive. No source-built executable substitutes for a release binary.

[Hosted run 38739136853](https://github.com/asm198x/asm198x/actions/runs/38739136853)
ran audit revision `2eacb0b` through the existing Setup action workflow. Its
four JSON artifacts are retained here. Raw probe hashes differ on Windows due
to Git's CRLF checkout; the verifier checks both exact LF and CRLF forms of the
same script. This folder now pins LF for reproducible evidence checkouts.

## Executed host matrix

Every row runs the same 19-case set. Counts describe observations, not unique
bugs or support for every version of an OS.

| Archive target | Runtime actually exercised | Pass | Fail | Unavailable |
|---|---|---:|---:|---:|
| `aarch64-apple-darwin` | Hosted macOS, Darwin 25.6.0, ARM64 | 13 | 6 | 0 |
| `x86_64-apple-darwin` | Hosted macOS, Darwin 24.6.0, x86-64 | 13 | 6 | 0 |
| `x86_64-pc-windows-msvc` | Hosted Windows Server 2025, AMD64 | 13 | 6 | 0 |
| `x86_64-unknown-linux-gnu` | Hosted Linux x86-64, glibc 2.39 | 13 | 6 | 0 |
| `aarch64-unknown-linux-gnu` | Debian Bookworm container, ARM64, glibc 2.36 | 12 | 6 | 1 |

The ARM64 Linux container used existing image
`sha256:82bc3c539b8813ada9d68c63b40158fa002f7f33de9bf3312a3dfdc0620dff56`
on the local ARM64 Docker engine, with two CPUs and 512 MiB memory. This is
Linux execution inside a VM/container on a Mac, not physical Linux hardware.
Its installer check is **unavailable** because this minimal image has neither
curl nor wget. The archive runtime itself executed all other cases.

A sixth observation on local Apple Silicon/macOS 27.0 (Darwin 27.0.0), retained
as [macos-arm64.json](macos-arm64.json), also records 13 passes and six failures.
Local Intel execution was unavailable (`Bad CPU type in executable`); the
hosted Intel Mac supplied direct execution instead. No emulated x86 run is
counted as native Intel-host evidence.

The passing cases cover version/help, independently checked bytes and repeated
output, JSON success, useful invalid-input diagnostics, previous-output
preservation on an assembly error, Unicode/spaced include paths with CRLF,
Lua, exact-path source/sidecar protection, and a complete upstream build. The
published installer passed on all four hosted targets and on the local Mac.

## Reference project and controls

The complete `kindjie/6502Assembly` Hello World target comes from the admitted
[ecosystem corpus](../../../ecosystem/corpus.json), pinned to
`35b4968be3bdd26804a5d195557d1225497a0358`, with its MIT licence and both include
files fetched and hash-verified. No upstream script ran. Native ACME 0.97 built
the target using the manifest's CPU/format options; [native-reference.json](native-reference.json)
retains its executable identity and command. Every host's release output is
2,081 bytes with SHA-256
`16e5a63289e474e84706517c592c0ba7f2ae86b5278d7d1e6eeb9a0c5ef205fc`.
The small assembly control also matches fresh native ACME output.

Every runtime run rejects a corrupted archive checksum before extraction.
A deliberate mutation of a successful assembler output makes the audit's
`deterministic_bytes` positive control fail. A wrong expected version makes the
installed-PATH smoke fail. [controls.json](controls.json) retains those results.
An audit step exits successfully only when acquisition and nonempty positive
controls worked; individual capability failures remain explicit in JSON and
`qualification_passed` is false for every CLI host observation.

## Reproduced release defects

| Failed check | Existing owner | Source fix |
|---|---|---|
| Missing-file errors ignore JSON diagnostic format | [#650](https://github.com/asm198x/asm198x/issues/650) | [#653](https://github.com/asm198x/asm198x/pull/653) |
| Unknown CPU with explicit dialect succeeds | [#608](https://github.com/asm198x/asm198x/issues/608) | [#665](https://github.com/asm198x/asm198x/pull/665) |
| Incompatible CPU with explicit dialect succeeds | [#608](https://github.com/asm198x/asm198x/issues/608) | [#665](https://github.com/asm198x/asm198x/pull/665) |
| Formatter accepts an irrelevant container flag | [#658](https://github.com/asm198x/asm198x/issues/658) | [#662](https://github.com/asm198x/asm198x/pull/662) |
| Formatter accepts irrelevant `--org 0` | [#663](https://github.com/asm198x/asm198x/issues/663) | [#664](https://github.com/asm198x/asm198x/pull/664) |
| Hard-linked output overwrites source | [#651](https://github.com/asm198x/asm198x/issues/651) | [#654](https://github.com/asm198x/asm198x/pull/654) |

These are six failed checks for five previously tracked defects. I did not
reopen the source-fixed issues or file duplicate implementation requests. They
still affect the audited published version. The first release containing the
fixes needs fresh archive execution; source CI alone cannot close that gap.

## Shipped API surfaces

The release CLI's structured JSON success/failure paths ran on every host.
MCP remains planned in [#492](https://github.com/asm198x/asm198x/issues/492), so
no shipped MCP endpoint was available to test.

The separate public npm distribution is `@asm198x/z80` **0.4.0**, not CLI
0.0.65. The registry's decoded provenance identifies source
`00e7193329b0bbe6732f19646eeac06a6444ffd2` and
[build run 33925227597](https://github.com/asm198x/asm198x/actions/runs/33925227597).
The downloaded tarball matches its registry SHA-512 integrity and attestation
subject digest. Attestation signatures were not independently verified.
[npm-inventory.json](npm-inventory.json) and [npm-provenance.json](npm-provenance.json)
retain those identities without the registry's temporary signed-access URLs.

[npm-probe.mjs](npm-probe.mjs) executed the shipped JavaScript/WASM in Node
24.9.0 on the local ARM64 Mac: seven checks passed for reference bytes,
repeated bytes, wrong-byte rejection, a multi-file Z80N build, structured
failure, invalid-target refusal and dialect discovery. Fresh native PasmoNext
0.1.3 corroborated the simple bytes; see [npm-native-reference.json](npm-native-reference.json).
This does not qualify browser UI behavior, the full Rust library API, every
export, or npm execution on the other hosts. `@asm198x/web` returned HTTP 404
from the public registry despite the README recommending it; that is #667.

## Minimal next gate and limits

[#668](https://github.com/asm198x/asm198x/issues/668) proposes extending the
existing candidate/package checks with this small real-project, structured
failure and output-safety set on all five targets. Required failures or
unavailable cases should block qualification; collection success should not.
This reuses the existing release plan and coordinates with umbrella
[#72](https://github.com/stevehill1981/198x/issues/72),
[#65](https://github.com/stevehill1981/198x/issues/65) and
[#108](https://github.com/stevehill1981/198x/issues/108).

No claim covers all OS versions/distributions, minimum supported OS releases,
Windows client editions, installer upgrades/uninstalls, all package-manager
routes, arbitrary projects, full performance budgets, browser behavior or
emulated machine execution. Homebrew formula URLs/checksums were inspected;
this audit did not install Homebrew formulae. Existing Linux package CI is
separate evidence. The broad host-quality programme remains open.

To check retention, run `python3.13 docs/plans/2026-10-15-release-host-audit/verify.py`.
To repeat a runtime observation, run `probe.py --target <table-target> --cache
<empty-directory> --out <new-report.json>` on a compatible host with Python
3.13+. It downloads only pinned public artifacts and the admitted upstream
sources; no compiler is needed. `--binary <installed-executable>` additionally
requires identity with the matching archive. Repeating a result is a fresh
runtime observation; the verifier only checks retained evidence.
