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
# Flag first, then the positional: `--publish z80` must reach the arch check
# as "z80", not as "--publish".
publish=0
if [ "${1:-}" = "--publish" ]; then
  publish=1
  shift
fi
arch="${1:-}"

# Name and version come from the crate's own manifest, not from here. A version
# in a shell script drifts from the thing it versions, and the last publish
# needed this line edited by hand — exactly the step a person forgets.
read_meta() {
  python3 - "$crate_dir/Cargo.toml" "$1" <<'META'
import re, sys
key = sys.argv[2]
text = open(sys.argv[1]).read()
parts = text.split("[package.metadata.npm]", 1)
if len(parts) < 2:
    sys.exit("Cargo.toml has no [package.metadata.npm] section")
body = re.split(r"\n\[", parts[1], maxsplit=1)[0]
found = re.search(r'^' + key + r'\s*=\s*"([^"]+)"', body, re.M)
if not found:
    sys.exit("[package.metadata.npm] has no " + key)
print(found.group(1))
META
}

package_version="$(read_meta version)"

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

# Publishing from here rather than leaving a `cd pkg && npm publish` for a
# person to get right. The crate directory has no package.json, so running npm
# in the obvious place does nothing useful and explains little about why.
if [ "$publish" -eq 1 ]; then
  if [ ! -f pkg/package.json ] || [ ! -f pkg/README.md ]; then
    echo "error: pkg/ is missing package.json or README.md; not publishing" >&2
    exit 1
  fi

  cd pkg
  # --access public because a scoped package defaults to restricted, and a
  # restricted publish looks identical in the output until an install fails.
  npm publish --access public

  # Then wait for the registry to serve it. A new package's first version can
  # take minutes to appear, and "is it there yet" is a question this should
  # answer rather than a person refreshing a page.
  echo "waiting for $package_name@$package_version to be served..."
  for _ in $(seq 1 60); do
    if npm view "$package_name@$package_version" version >/dev/null 2>&1; then
      echo "live: $package_name@$package_version"
      exit 0
    fi
    sleep 10
  done
  echo "warning: publish reported success but the registry is not serving it yet." >&2
  echo "Usually propagation. Confirm with: npm view $package_name versions" >&2
fi
