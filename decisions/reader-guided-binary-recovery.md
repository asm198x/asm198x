# Decision: binary recovery is guided by the reader and verified before output

**Status:** Active. First slice of #504: raw 6502 images, ACME source.

**Date:** 2026-09-10.

`disasm --recover` is an opt-in workflow above Isa198x's decoder. The existing
linear disassembly command remains available. Recovery belongs in Asm198x
because it consumes both disassembly and assembly, and renders through the
same semantic-AST formatter as other source workflows.

A flat image has an explicit load address and must fit without wrapping in
the 16-bit address space. Ranges are half-open CPU addresses. Code ranges are
combined; explicit data overrides them, regardless of argument order. All
unmarked bytes stay data. The map is an interpretation supplied by the reader,
not an inference justified by successful decoding or byte identity.

The workflow names direct branch, jump, and call targets inside the image.
It does not chase indirect jumps, follow execution, infer entry points from
vectors, or promote targeted data to code. User names take precedence and
must be unique identifiers. Names outside the image become constants.
Instructions split by labels remain bytes, so a label never lands silently
at the wrong address. Unknown/truncated encodings remain bytes too.

Every source result is formatted through the ACME AST and assembled again.
Byte equality and origin equality are gates before any source reaches the
caller. The CLI writes only new output files, preserving input binaries and
hand-edited source even through file aliases. Stdout is available for pipes.

Address widths remain explicit where symbol substitution could change them.
Wrapping relative branches are currently emitted as bytes with their decoded
instruction in a comment: the reference ACME assembler refuses the extended
arithmetic spelling that the shared assembler accepts, while the shared
assembler does not yet accept the reference's wrapping target spelling.
Recovery preserves the bytes without extending source compatibility here.

The first proof is an included palette-copy program: recover loop and call
labels, name its table, rebuild identically, change its XOR mask, and rebuild
with exactly the intended immediate byte changed. All documented 6502 forms
also pass recovery and reference-ACME checks.

Debug198x symbol import, further output dialects/CPUs, and container/banked
images remain subsequent slices of #504. No new machine IR or decoder is
introduced for this first consumer.
