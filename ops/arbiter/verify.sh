#!/usr/bin/env bash
# Does this image reproduce the identities the corpus is keyed on?
#
# The probe per tool matches the harness's own table
# (crates/asm198x/tests/support/tool_identity.rs): most take `--version`, a few
# take `-v`, and asl/p2bin print theirs on the *second* line after a "no input
# files" error and exit non-zero. So no exit code is required, and each line is
# matched on a marker rather than a position.
set -uo pipefail

ids="${1:-/usr/local/share/asm198x/identities}"
fail=0

probe() {
  case "$1" in
    asl|p2bin|pasmo|vasmm68k_mot) "$1" -v 2>&1 ;;
    *) "$1" --version 2>&1 ;;
  esac
}

while IFS=$'\t' read -r tool want; do
  case "$tool" in ''|\#*) continue ;; esac
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf '  MISSING   %s\n' "$tool"; fail=1; continue
  fi
  got="$(probe "$tool" | grep -F "$want" | head -1)"
  if [ -n "$got" ]; then
    printf '  ok        %s\n' "$tool"
  else
    printf '  MISMATCH  %s\n            want: %s\n            got:  %s\n' \
      "$tool" "$want" "$(probe "$tool" | head -2 | tr '\n' ' ')"
    fail=1
  fi
done < "$ids"

if [ "$fail" -ne 0 ]; then
  echo
  echo "This image does not reproduce the corpus's arbiter identities. Verdicts"
  echo "recorded here would key separately from every verdict already held, so"
  echo "growth must not run until the pins match."
  exit 1
fi
echo
echo "every arbiter identity matches the corpus"
