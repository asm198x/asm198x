# Understanding diagnostics

An error's short message describes the immediate problem. Its code names a
longer explanation, available without a network connection:

```sh
asm198x --explain BranchOutOfRange
```

Use the exact code printed by the diagnostic or returned in JSON's `code`
field. `--explain` takes no source file, writes Markdown to stdout and exits
unsuccessfully for an unknown code. `--explain=BranchOutOfRange` also works.
The command does not assemble or write artifacts.

The explanations in this chapter are the same text embedded in the binary.
They describe what an error means, how to investigate it and what a proposed
fix can change. `AssemblyError` remains the catch-all while raising sites
gain more specific classifications; a generic code does not imply all
generic failures have the same cause.
