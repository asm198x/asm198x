# Set up asm198x

Install an exact published release and add it to PATH for subsequent steps.
This action lives in the flagship repository; no separate setup repository
or Rust installation is required.

```yaml
- uses: asm198x/asm198x/.github/actions/setup@main
  with:
    version: '0.0.57'
- run: asm198x --version
```

For a reproducible workflow, replace `main` with a reviewed full commit SHA
that contains this action. The action revision and binary version are two
independent pins. Release tags predating this action do not contain it.

`version` is required. It accepts exact semantic versions, optionally prefixed
by `v` or `asm198x-v`; it refuses `latest`, branches and version ranges. No
source build or different-architecture fallback is attempted.

Supported archives are macOS ARM64 and x86-64, Linux x86-64 GNU, and Windows
x86-64 MSVC. The runtime must be compatible with that release binary. The
action uses GitHub's Node 24 action runtime and the runner's `tar` executable
(including Windows' ZIP-capable tar); these are available on the tested
standard hosted runners. Linux musl-only containers and unlisted architectures
are not supported by this action.

The archive and SHA-256 sidecar are downloaded from the exact GitHub release.
The checksum is checked before extracting the one expected executable, and
its reported version is checked before PATH is changed. The checksum verifies
integrity against the release's own sidecar; it is not an independent
publisher signature. Both files still rely on GitHub and the release publisher.
No token is required and no npm dependencies are installed.

Outputs are `version` (without a prefix), `target` (Rust target triple) and
`path` (the installed binary directory). Installation uses a fresh directory
under `RUNNER_TEMP`; it lasts for the job and does not reuse a mutable cache.

## Verification

```sh
node --test .github/actions/setup/index.test.cjs
```

Unit tests cover all four target selections, version validation, archive
layouts, checksum/name mismatches, failed downloads/extraction/version checks,
and publication of PATH only after verification. The `Setup action` workflow
also downloads release 0.0.57 on all four architectures and assembles a 6502
fixture using the installed executable from the following step's PATH.

## cargo-binstall

The package manifest supplies the namespaced release URL, Unix `txz` format
and enclosing binary directory. The Windows override selects ZIP and its flat
binary path. Test edited metadata against an already-published workspace
version without falling back to compilation:

```sh
cargo binstall asm198x --manifest-path crates/asm198x \
  --strategies crate-meta-data --disable-telemetry --no-discover-github-token --no-confirm \
  --install-path /tmp/asm198x-binstall-check
```

Use an isolated `CARGO_HOME` for this check if global binstall settings must
remain untouched. `--target` can exercise other archives, but a foreign binary
cannot necessarily execute on the checking host. Metadata reaches ordinary
`cargo binstall asm198x` users when the containing crate version is published.
See [binstall's metadata specification](https://github.com/cargo-bins/cargo-binstall/blob/main/SUPPORT.md).
