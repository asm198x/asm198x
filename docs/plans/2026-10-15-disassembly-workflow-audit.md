# Audit ordinary disassembly mode boundaries (#641)

Use the published 0.0.66 macOS ARM64 archive, with its release manifest and SHA-256
sidecar. This is an audit of the existing CLI contract, not decoder expansion.
`reader-guided-binary-recovery.md` keeps ordinary listings distinct from opt-in
recovery; recovery's 16-bit/new-file constraints are intentional. The CLI reference
assigns output/container/include/EQU/JSON options to assembly or formatting.

1. Acquire the public archive through the existing fetch/gate helpers, verify its
   checksum and executable identity, and retain exact release/source/host hashes.
2. Add a bounded Python probe under
   `docs/plans/2026-10-15-disassembly-workflow-audit/probe.py`. In fresh directories,
   run ordinary disassembly with assembly-only flags and ordinary `-o`; compare
   results with a valid stdout listing, and include valid CPU/origin and existing
   invalid-sidecar/invalid-CPU rejection controls. Record source/destination bytes
   before/after each process, exit status, stdout, stderr and arguments.
3. Check a 68000 origin above $FFFF against the ordinary disassembler's existing
   u32-origin entry point and search existing issues before attributing a new
   limitation. Sample actual non-Z80 disassembly and ACME formatting against the
   current CLI help/reference wording. Do not infer full CPU compatibility.
4. Run every observation twice. Assert positive controls and stable outcomes,
   retaining raw observations, a checksum manifest and report in the same audit
   folder. Distinguish failures, deliberate behavior and unsupported features.
   File bounded findings or link existing issues; avoid duplicate recovery work
   under #504 or assembly/banked-layout work under #610/#611.
5. Verify the probe can reject a failed positive control, run Ruff and generated
   docs, land the report through a fully green squash PR, update #641 and clean
   the task workspace after verifying remote retention.
