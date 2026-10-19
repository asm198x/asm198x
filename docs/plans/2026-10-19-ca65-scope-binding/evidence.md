# Separate ca65 binding, constancy and instruction width (#785)

The 106 retained phase probes produce 85 accepted native outcomes, 20 native
refusals and one bounded native timeout. The preceding explicit executable
is compared against the 105 completed outcomes and disagrees on 53. The
native timeout is retained as an observation and excluded from comparisons.

Ordinary expressions allocate an unresolved symbol in the current scope. They
bind to a later local definition, otherwise falling outward at final binding.
A source-time `.res VALUE` or `.if VALUE` inside a new scope refuses even when
an outer VALUE exists and no local definition follows. Explicit `::VALUE`
remains a source-time constant. An alias may become constant once its local
dependency is defined; alias chains must propagate that update.

Native instruction width is separate from value folding. With an outer
VALUE=$12, `lda VALUE+250` inside a new scope chooses zero page and later
refuses the final value 268. An alias of the expression does the same. With
outer VALUE=$1234 and later local VALUE=$34, native retains absolute width and
emits AD3400. Low/high-byte extraction chooses a byte-sized operand. Replacing
unresolved references with eagerly evaluated outer values would therefore
still disagree on instruction forms even after final value binding is fixed.

Name queries have source-order effects too: `.defined(VALUE)` initially finds
the outer binding, but an earlier unresolved expression or alias reference
creates a current-scope placeholder. A later `.defined(VALUE)` then answers
zero until that placeholder is defined. The earlier short native matrix did
not exercise this distinction. Keep queries, expression allocation, address
size and final lookup on distinct measured paths using existing private seams.

The pinned cc65 source is implementation precedent: expression parsing calls
ParseAnySymName with SYM_ALLOC_NEW; unqualified allocation searches the current
scope, whereas name lookup without allocation uses SymFindAny outward.
SymCheckUndefined later transfers unresolved references to an enclosing symbol.
Actual installed ca65/ld65 observations remain the fixture authority.

The outer/local alias cycle in `outer_alias_cycle_shadow` exceeded ten seconds
in native ca65 and was killed/reaped by subprocess.run. Ordinary unused cycles
and undefined aliases refuse. Do not turn the timed-out case into a passing
oracle or infer a byte outcome from it.

The deferred alias prerequisite landed separately in #793. This slice preserves
lexical references through final binding and maintains ordered known,
unresolved and referenced symbol events in the selected reader. Source-time
folds use that reader; instruction widths use a private sparse size-hint cache,
independent of numeric values. Cache invalidation follows affected dependants
before a newly known label or constant selects an instruction. Arithmetic still
uses the shared evaluator. No dependency, shared AST or public contract changes.

The final fixture selects 312 completed native controls: 205 accepted and 107
refused. All completed cases are accounted for by build_fixture.py; 28 related
constancy/text-query, enum and label-size cases remain explicitly retained in
deferred-cases.json and tracked as #795, #796 and #798. The native timeout
remains outside byte/status expectations. Full parity is not claimed.

preceding-alias-archive-red.json compares the actual extracted #793 local archive
with this final fixture and finds 176 disagreements; current-comparison.json
matches all 312. predecessor-red.json retains the earlier research baseline,
and after-aliases-red.json retains the narrower 105-case prerequisite comparison.
The comparison helper writes included files before running either executable.
Formatter idempotence, reassembly and stable debug sections/symbols are checked
for every accepted case. The included range refusal preserves five existing
outputs and both sources, with the correct include location in human and JSON.

Implicit 16-bit memory addresses now reject negative values (#794), including
indirect JMP; signed branch offsets retain their native behavior. Record size
properties use a suffix impossible in authored cheap names (#797), so legal
@size labels and constants coexist with records. Existing auxiliary debug names
are retained. Ordinary procedure/label size support remains #798.

The actual local Lua archive passes 57 required cohorts, including 927 commands
in this new cohort. Both changed-byte and corrupt-archive controls fail as
required; the preceding archive fails only this new cohort. Reports identify
this local run as artifact-only. Producer replay and five-host qualification
remain separate requirements before merge.

performance.json retains 36 bounded measurements (three trials, 500/2,000/8,000
definitions, independent and forward-chain input, both actual executables).
At 8,000 definitions, independent constants take 0.270–0.288 seconds versus
0.265–0.269; forward chains take 0.353–0.360 versus 0.317–0.337. These are local
observations with other checks running, not a benchmark guarantee.

Local checks pass 1,547 Rust tests (38 ignored), 72 Node tests, Clippy with
all workspace targets/features and warnings denied, fmt, Ruff and generated
docs. The initial Clippy run found a needless borrow; the corrected rerun
passes. Its initial failure is retained alongside the final log.
