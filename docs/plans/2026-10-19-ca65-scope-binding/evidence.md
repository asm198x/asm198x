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

No implementation has changed in this slice. The plan requires retaining
private deferred aliases, sparse source-time propagation, shared expression
evaluation and source-position width hints before broad regression testing.
