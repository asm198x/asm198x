# Decision: real-world compatibility is measured with an ecosystem corpus

**Status:** Active. Binding for Asm198x.

**Date:** 2026-08-30.

## The decision

Asm198x keeps a manifest of complete, independently authored source projects
and measures whether they assemble unchanged. This is the fifth layer of the
correctness model, beside curriculum identity, round-trip, form/sweep audits,
and differential fuzzing.

An admitted project names:

- its upstream HTTPS clone URL and a full immutable commit;
- its CPU and Asm198x dialect;
- a licence expression and the file that grants it;
- one or more build targets, including their working directory, source inputs,
  exact native-assembler argument vector, and expected outputs.

The committed material is an **index**, not a vendor snapshot. `cargo xtask
ecosystem fetch` acquires the pinned trees into a caller-chosen directory. The
trees, their generated files, and their Git history do not enter this repo.

## What admission proves

Admission proves that a project is attributable, pinned, legally inspectable,
and has a reproducible native assembler boundary. It does not claim Asm198x
already accepts it. A native build is the control; an Asm198x rejection is a
compatibility finding, not a reason to remove the project.

The comparison unit is a complete upstream build target, not an extracted
`.asm` file. Includes, macros, binary assets, command-line definitions, linker
configuration, and output directives are part of source compatibility.

## Safety and maintenance

Acquisition never executes upstream code. Build commands are stored as argv,
not shell strings, both to make review exact and to prevent quoting from
changing what a command means. Running an upstream Makefile or script is a
separate, deliberate audit action.

Commits move only in reviewed changes. A refresh must rerun the native control
before its Asm198x result is interpreted. A project whose current tool no
longer accepts its pinned source needs its historical tool version established;
it must not be counted as an Asm198x failure until that control is green.

The manifest begins in the flagship repository because it is small and the
assembler, validator, and compatibility decisions change together. If source
snapshots, recorded large outputs, or independently released corpus tooling
become necessary, they graduate to a dedicated `asm198x` organisation repo and
this repository pins its revision, following the same graduation principle as
shared formats.

## Why the verdict and curriculum corpora are not enough

The verdict corpus efficiently proves reference-tool answers for generated
snippets and curated probes. Code198x proves the programs the family authors
and ships. Neither samples the combinations selected by unrelated authors:
macro systems, old and new syntax generations, project-specific segments,
large include graphs, or native container directives.

The binding syntax decision says a real project should assemble unchanged.
That claim needs evidence drawn from real projects.

## Drift triggers

- **“Copy the interesting file into tests.”** Keep the upstream build target;
  its environment is part of the evidence.
- **“Track the default branch.”** Pin a full commit. Moving source and moving
  assembler behaviour must never be conflated.
- **“Only admit projects that pass.”** Rejections are the compatibility map.
  Require the native control to pass, not Asm198x.
- **“The licence is probably open source.”** No admission without an explicit
  grant and its path.
- **“Run the upstream build during fetch.”** Acquisition is inert. Execution
  is a distinct audit action.
