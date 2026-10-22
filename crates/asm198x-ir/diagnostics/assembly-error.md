# AssemblyError

Assembly stopped before a valid result could be produced. This is the
catch-all code for errors that have not yet been given a narrower category;
the diagnostic's message, source location and expansion notes explain the
particular failure.

Start at the named file and line. If the line came from a macro or included
file, follow the accompanying notes to its definition and invocation. Check
the selected dialect before changing syntax: a directive accepted by one
assembler need not mean the same thing in another.

Fix the first reported error and assemble again. Later labels and sizes can
depend on that line, so a failed assembly is not a trustworthy partial image.
If the same source works in the reference assembler, retain the smallest
reproducing source, the dialect/CPU options and both tool versions when
reporting the mismatch.

`--message-format=json` reports the code separately from the human message.
Do not parse the message to recover a category; its wording can improve
without changing the meaning of this code.
