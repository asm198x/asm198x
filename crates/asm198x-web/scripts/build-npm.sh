#!/usr/bin/env bash
# Build the npm package for @asm198x/web.
#
# wasm-pack derives the package name from the crate, so `--scope asm198x`
# emits `@asm198x/asm198x-web` — the stutter 198x/decisions/crate-naming.md
# § "Scoped registries" calls the tool's default rather than a decision. The
# scope already carries the provenance; the name does not repeat it.
#
# The crate stays `publish = false` for crates.io, which is a separate
# question: a #[wasm_bindgen] surface returning JavaScript values has no Rust
# caller, so there is nothing there for a Rust consumer to want.
set -euo pipefail

crate_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
package_name="@asm198x/web"
package_version="0.1.0"

cd "$crate_dir"
wasm-pack build --target web --release --out-dir pkg --scope asm198x

python3 - "$package_name" "$package_version" <<'PY'
import json, sys
name, version = sys.argv[1], sys.argv[2]
path = "pkg/package.json"
with open(path) as f:
    pkg = json.load(f)
pkg["name"] = name
# Versioned on its own API rather than the assembler's. The two move for
# different reasons: asm198x releases on dialect coverage, this on the shape
# of three exported functions.
pkg["version"] = version
files = pkg.get("files", [])
if "README.md" not in files:
    files.append("README.md")
pkg["files"] = files
with open(path, "w") as f:
    json.dump(pkg, f, indent=2)
    f.write("\n")
print(f"{name}@{version}")
PY

echo "built $(du -h pkg/*_bg.wasm | cut -f1) of wasm in $crate_dir/pkg"
