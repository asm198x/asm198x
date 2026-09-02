#!/usr/bin/env bash
# Build an npm package for the assembler.
#
#   scripts/build-npm.sh            -> @asm198x/web,     every architecture
#   scripts/build-npm.sh z80        -> @asm198x/z80,     Z80 only
#   scripts/build-npm.sh mos6502    -> @asm198x/mos6502, 6502 family only
#
# Splitting by architecture is worth it. `entry` is the only thing naming the
# library's assembler entry points, so a build that does not select an
# architecture never references it and the linker drops it: Z80 alone is
# 153 KB gzipped against 480 KB for all of them. On a lesson page that also
# embeds an emulator, that is the difference between the assembler dominating
# the page weight and not.
#
# wasm-pack derives the package name from the crate, so `--scope asm198x`
# emits `@asm198x/asm198x-web` — the stutter 198x/decisions/crate-naming.md
# § "Scoped registries" calls the tool's default rather than a decision.
set -euo pipefail

crate_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
arch="${1:-}"
package_version="0.1.0"

if [ -z "$arch" ]; then
  package_name="@asm198x/web"
  features=()
else
  # Fails here rather than emitting a package that assembles nothing.
  if ! grep -qE "^${arch} = \[\]" "$crate_dir/Cargo.toml"; then
    echo "error: no such architecture feature: ${arch}" >&2
    echo "available:" >&2
    grep -oE "^[a-z0-9]+ = \[\]" "$crate_dir/Cargo.toml" | cut -d' ' -f1 | sed 's/^/  /' >&2
    exit 1
  fi
  package_name="@asm198x/${arch}"
  features=(-- --no-default-features --features "$arch")
fi

cd "$crate_dir"
wasm-pack build --target web --release --out-dir pkg --scope asm198x ${features[@]+"${features[@]}"}

python3 - "$package_name" "$package_version" "${arch:-all}" <<'PY'
import json, sys
name, version, arch = sys.argv[1], sys.argv[2], sys.argv[3]
path = "pkg/package.json"
with open(path) as f:
    pkg = json.load(f)
pkg["name"] = name
pkg["version"] = version
if arch != "all":
    pkg["description"] = f"asm198x assembler for {arch}, compiled to WebAssembly."
files = pkg.get("files", [])
if "README.md" not in files:
    files.append("README.md")
pkg["files"] = files
with open(path, "w") as f:
    json.dump(pkg, f, indent=2)
    f.write("\n")
print(f"{name}@{version}")
PY

wasm="$(ls pkg/*_bg.wasm)"
echo "built $(du -h "$wasm" | cut -f1) raw / $(gzip -9 -c "$wasm" | wc -c | tr -d ' ') bytes gzipped"
