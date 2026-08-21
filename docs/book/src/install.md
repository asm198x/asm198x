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

## Not on crates.io

`cargo install asm198x` will not find it. The binary ships through the GitHub
Release instead, and the generic `isa` crate name is deliberately unclaimed.
Use the installer or an archive.

## Check it worked

```sh
asm198x --version
```

If the shell cannot find it, the binary is not on your `PATH`. The installer
prints where it put things; the archives leave that to you.

## Which version am I reading about?

Every page on this site names the release it describes, at the end of the
navigation. [Releases](/releases/) lists what changed in each one.
