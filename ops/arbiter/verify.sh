#!/usr/bin/env bash
# Does this image hold the versions ops/arbiter/identities says are in use?
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
    printf '  MISSING   %s\n' "$tool"
    [ -n "${GITHUB_ACTIONS:-}" ] && printf '::error title=arbiter identity::%s is missing\n' "$tool"
    fail=1; continue
  fi
  # A whole line, trimmed, exactly equal: the same rule the live suites use
  # when they record a verdict's version, and `cargo xtask grow` uses as its
  # gate, so all three agree on what "this version" means.
  got="$(probe "$tool" | sed 's/^[[:space:]]*//; s/[[:space:]]*$//' | grep -Fx -- "$want" | head -1)"
  if [ -n "$got" ]; then
    printf '  ok        %s\n' "$tool"
  else
    got_line="$(probe "$tool" | head -2 | tr '\n' ' ')"
    printf '  MISMATCH  %s\n            want: %s\n            got:  %s\n' \
      "$tool" "$want" "$got_line"
    # In CI, also as an annotation, so the version a bump needs is readable on
    # the pull request without opening the log.
    if [ -n "${GITHUB_ACTIONS:-}" ]; then
      printf '::error title=arbiter identity::%s reports "%s"; identities expects "%s"\n' \
        "$tool" "$got_line" "$want"
    fi
    fail=1
  fi
done < "$ids"

if [ "$fail" -ne 0 ]; then
  echo
  echo "This image's tools are not the versions ops/arbiter/identities lists, so"
  echo "growth must not run in it: it would record versions the repository does"
  echo "not say it uses. Bump the pins and identities together."
  exit 1
fi
echo
echo "every tool reports the version ops/arbiter/identities lists"
