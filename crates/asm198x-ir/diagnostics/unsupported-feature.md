# UnsupportedFeature

The named construct belongs to the reference assembler, but this asm198x
build does not implement the requested behaviour. This is a limitation of
the tool, not evidence that your source is invalid. The short diagnostic
names the construct and, where known, the missing behaviour.

Check the dialect and CPU first. If they are intentional, use the reference
assembler for the affected build or reduce the source to a supported form
whose bytes you can verify. Do not delete a directive just because assembly
then succeeds: directives can control layout, symbol visibility, memory
contents or output containers without emitting an opcode of their own.

Different refusals need different next steps:

- A Lua-disabled build needs the `lua` Cargo feature, not rewritten Lua.
- An unsupported CPU selection needs the matching implemented CPU/dialect
  entry point, if one exists. Changing only the directive can change opcodes.
- An output or linker construct needs its container or linking behaviour;
  a flat binary is not an interchangeable object file or executable.
- A refused lwasm pragma can change parsing or encoding. Removing it is safe
  only after checking the program under both settings in the reference.
- Clock-dependent directives cannot be reproduced by inventing a timestamp.
  Keep the input explicit when adapting source for reproducible builds.

The conformance ledger and dialect reference distinguish implemented words,
known gaps, and constructs the reference itself refuses for the chosen
target. That last category is not this error: matching a reference refusal
does not mean asm198x lacks an otherwise valid feature.

When reporting a gap, include the exact spelling, surrounding source,
options, reference version and expected bytes or artifacts. That makes the
request testable without guessing what a no-op implementation would mean.
