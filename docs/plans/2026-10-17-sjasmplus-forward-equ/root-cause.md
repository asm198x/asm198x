# Root cause and observed native boundary

The first differential test fails on the original source with
`undefined symbol MAX_PLAYERS` at line 1. The lowering walk only records an EQU
when `eval_const` succeeds against symbols already seen. Unlike count and
condition expressions, the EQU never calls `Forward::resolve`, so it does not
request the subsequent passes. Its unresolved expression then reaches the
shared engine, whose address pass must evaluate EQU immediately.

The 78 retained SjASMPlus 1.21.0 executions establish three-pass behaviour,
including cycles and long chains: `TOTAL EQU TOTAL+1` emits 3; a three-link
forward chain emits 3 rather than its fully converged value 5. Missing symbols
and final zero divisors fail. These results rule out both recursive topological
resolution and a final zero fallback. Fold each SjASMPlus EQU at its own live
scope and PC using the existing pass seed, preserving the three-pass limit.
Other Z80 dialects retain their current engine path.
