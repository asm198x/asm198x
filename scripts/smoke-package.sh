#!/usr/bin/env bash
set -euo pipefail
[ "$1" = asm198x ]
package_bin=$2
fixture_dir=$3/asm198x-smoke
mkdir -p "$fixture_dir"
cat > "$fixture_dir/input.s" <<'ASM'
* = $0801
lda #$2a
sta $0400
rts
ASM
"$package_bin/asm198x" --dialect acme "$fixture_dir/input.s" -o "$fixture_dir/output.bin"
printf '\251\052\215\000\004\140' > "$fixture_dir/expected.bin"
cmp "$fixture_dir/expected.bin" "$fixture_dir/output.bin"
