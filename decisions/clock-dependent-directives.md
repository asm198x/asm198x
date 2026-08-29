# Decision: assembly has no implicit wall-clock input

**Status:** Active. Binding for Asm198x (accepted 2026-08-29).

**Date:** 2026-08-29.

## The decision

Asm198x refuses lwasm's `dtb` and `dts` directives by name. Ordinary assembly
is a function of the source and the explicit files and options supplied by its
caller; the host clock is not an input.

This is an intentional reproducibility boundary, not an unexamined language
gap. The diagnostic continues to say that lwasm accepts the directive and
Asm198x does not implement it, so valid lwasm source is never misreported as a
typo.

## Why

`dtb` emits six bytes derived from the current date and time, while `dts` emits
their 24-byte textual representation. Two assemblies of unchanged source can
therefore differ solely because they ran a second apart. That breaks the
assumption behind verdict replay, differential comparison, release artifacts,
and a caller's ability to cache an assembly by its declared inputs.

Reading the clock implicitly would also make the library API less honest: no
argument or loader operation would reveal the input responsible for the
changed bytes.

## What would permit them later

A future explicit timestamp input may reopen this decision. It must be part of
the public assembly options, be visible to library and CLI callers, and have
one specified timezone and range. An ambient environment variable alone is
not sufficient for the library contract, and silently substituting a fixed
date would not match lwasm source semantics.

Until that contract exists, `dtb` and `dts` remain deliberately unsupported.

## Rejected alternatives

- **Use the current clock.** This matches lwasm's bytes at one instant but
  makes builds and tests nondeterministic.
- **Read `SOURCE_DATE_EPOCH` implicitly.** This restores reproducibility for
  configured processes but hides a library input and leaves unconfigured
  behaviour unresolved.
- **Emit a constant timestamp.** This is deterministic but silently changes
  the meaning of valid lwasm source.
