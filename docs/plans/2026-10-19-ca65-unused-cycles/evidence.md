# Validate ca65 alias values at native phases (#800/#802)

Each retained alias now freezes its source width and source PC segment width.
The sparse hint carries cycle observations, and a later initializer studies
existing definitions before installing its own expression. Final undefined-name
checks remain unconditional. The iterative numeric queue resolves ready aliases;
remaining cycles refuse when source width was unknown or their numeric value
is demanded. Unevaluated constants have no invented debug value.

The shared evaluator supplies numeric operations, short-circuit constants and
actual value demands from placed expressions. Source-time ifconst/ifnconst also
study the expression; typed private conditional errors preserve a cycle's
included definition span rather than replacing it with the conditional's file.
The broader constancy/text ordering work remains #795. A ZEROPAGE location-counter
alias retains its byte width when used later in CODE (#802).

The final fixture has 164 completed native controls: 63 accepted, 101 refused.
The actual explicit scoped-binding predecessor disagrees on 60; the corrected
executable matches all 164. The fixture builder accounts for all 171 cases in
its five source batches: two linker-output cases remain #801, one scoped-export
acceptance remains #803, and four native signal-11 crashes are observations,
not reference refusals. Separate four-case export controls demonstrate #803.
The initial forced-width batch lacked a ZEROPAGE memory assignment; its raw
report is retained as missing-zp-config and is not used for qualification.

Formatter idempotence, reassembly and stable debug shape hold for every accepted
case. Both final numeric use and a root-file constancy query report the included
cycle definition and preserve all five outputs plus both sources in human and
JSON modes. Unevaluated local aliases are omitted from numeric debug symbols;
known outer constants retain their actual values. Qualified visibility names
now refuse as native syntax requires, preventing a newly successful constant
fold from masking an existing invalid export spelling. Valid scoped export
lookup remains #803.

An actual local Lua archive passes all 58 required cohorts, including 353
commands in this cohort. Changed bytes fail exactly the three byte/determinism/
structured-success controls; a corrupt archive fails its SHA-256 check. These
local reports are artifact-only. Clean-producer replay and all five exact-head
package reports/ten corruption controls remain required before merge.

Performance evidence contains 36 paired alias measurements and 27 additional
normal-instruction/known-width-cycle controls, at 500/2,000/8,000 inputs with
three trials. At 8,000, independent constants take 0.260–0.268 seconds versus
0.260–0.270; forward chains take 0.357–0.364 versus 0.332–0.337. Normal
instructions take 0.134–0.137 versus 0.131–0.133 seconds; 8,000 accepted unused
cycles take 0.608–0.624 seconds. These are local observations during other
checks, not benchmark guarantees. No full namespace is copied per instruction.

The first test-harness compile reported `cannot find value source in this
scope`; the missing test input was restored before the passing focused run.
The initial three semantic discrepancies identified qualified export syntax
and the separate scoped-export gap. Additional source-time constancy controls
pinned the required value study before the final full run. Initial evidence
and corrected logs are retained rather than reported as passing checks.

Final isolated checks pass 1,550 Rust tests (38 ignored), 73 Node tests,
Clippy with all workspace targets/features and warnings denied, fmt, Ruff and
generated docs. The actual preceding archive fails only the new cohort; all
57 other cohorts pass.

Clean producer `ab986b3c235e6161321cf327257cc9802ae31ac7` matches all
564 unchanged native/upstream 4510 bytes. The pinned source, seven successful
processes/fourteen streams and full output identities are retained; the
changed-byte guard rejects corruption.
