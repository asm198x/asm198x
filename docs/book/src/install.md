# Install

One binary, no runtime dependencies, the same interface on macOS, Linux and
Windows. Pick whichever of these suits you.

## Homebrew

```sh
brew install asm198x/tap/asm198x
```

Homebrew asks you to trust a third-party formula the first time. Approving
`asm198x/tap/asm198x` trusts that one formula; `brew trust --tap asm198x/tap`
would trust everything the tap publishes, now and in future. Prefer the formula.

## Installer script

```sh
# macOS, Linux
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/asm198x/asm198x/releases/latest/download/asm198x-installer.sh | sh
```

```powershell
# Windows
irm https://github.com/asm198x/asm198x/releases/latest/download/asm198x-installer.ps1 | iex
```

These fetch the newest release, so the command stays right as versions move.

## Archives

Each release attaches platform archives to its
[GitHub Release](https://github.com/asm198x/asm198x/releases). Download one and
put the binary on your `PATH`:

| Target | Platform |
|---|---|
| `aarch64-apple-darwin` | macOS, Apple silicon |
| `x86_64-apple-darwin` | macOS, Intel |
| `x86_64-unknown-linux-gnu` | Linux |
| `x86_64-pc-windows-msvc` | Windows |

## Cargo

```sh
cargo install asm198x --locked
```

To fetch a prebuilt release instead of compiling, use
[cargo-binstall](https://github.com/cargo-bins/cargo-binstall):

```sh
cargo binstall asm198x
```

Binstall reads the selected crate version's published metadata. The metadata
names the GitHub release archives and their platform-specific layouts; older
versions without that metadata may require the installer or a direct archive
download instead. Binstall can fall back to compilation when a binary is
unavailable. Use `--strategies crate-meta-data` if the build must fail instead
of compiling or trying another binary provider.

## GitHub Actions

```yaml
- uses: asm198x/asm198x/.github/actions/setup@main
  with:
    version: '0.0.57'
- run: asm198x --version
```

Replace `main` with a reviewed full commit SHA for reproducible workflows.
The action revision and binary version are independent pins: old release
tags may not contain the action. `version` must be exact; `latest`, branches
and version ranges are rejected.

The action verifies the archive against its SHA-256 sidecar, checks the
executable's version, and adds it to PATH for subsequent steps. It supports
the four archive targets above, requires no Rust toolchain or token, and uses
the Node 24 action runtime and the hosted runner's `tar`. The checksum is an
integrity check against the release, not an independent publisher signature.
See the [setup action reference](https://github.com/asm198x/asm198x/tree/main/.github/actions/setup)
for outputs, self-hosted runner requirements and verification details.

## Check it worked

```sh
asm198x --version
```

If the shell cannot find it, the binary is not on your `PATH`. The installer
prints where it put things; the archives leave that to you.

## Which version am I reading about?

Every page on this site names the release it describes, at the end of the
navigation. [Releases](/releases/) lists what changed in each one.
