# Flat macro diagnostic verification

Macro placement already carries definition and invocation frames in Node spans.
Lowering discarded them when data had no precise operand span, and backend
line-only errors recreated spans without the frames. Lowering now retains a
line-granular fallback in the existing operand span; statement errors carry it.

The two flat ca65 readers expand each file with local file ID zero. Their final
private reader seam now resolves those frame locations to the physical file,
including parser errors. Linked ca65 retains its existing source-map behavior.

The 80-case native sweep now agrees on all outcomes and expected context after
the separate approved far-address fix. Before that range fix, the diagnostic
change fixed all remaining contexts: ten far-address wrong acceptances remained.
Existing included-error formatter tests now require the definition and invocation
paths as well as the native error location and preserved outputs.
