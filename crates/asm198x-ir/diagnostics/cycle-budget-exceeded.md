# CycleBudgetExceeded

A comment such as `; asm198x: cycles(draw) <= 100` gives a routine a
cycle ceiling. Its statically counted worst case exceeds that ceiling, so
assembly fails. The diagnostic names the routine, the allowed count and the
count that was found.

The counted region runs from that label to the next label. It is a
straight-line sum, including the available worst-case instruction penalties;
it is not a simulation of the program's control flow. A loop is not multiplied
by its iteration count, a call does not recursively include its callee, and a
hardware stall outside the instruction timing model is not inferred.

Read the listing with `--listing` to locate expensive instructions and check
where labels divide the region. Then decide whether to reduce work, choose
cheaper instructions, or correct an incorrectly specified region or limit.
Changing the ceiling is justified only if the actual timing requirement has
changed. Splitting a routine with a label can reduce a reported subtotal
without making execution any faster.

Recheck code size, register effects and flags after an optimisation. Use a
machine-level measurement when the requirement concerns elapsed hardware
time, contention, interrupts or a loop's complete execution. CPUs with
incomplete timing coverage cannot prove a ceiling; that is a different error
from a measured static count exceeding one.
