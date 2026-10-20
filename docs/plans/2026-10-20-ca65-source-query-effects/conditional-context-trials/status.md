# Preflight records, not final qualification

These files retain intermediate native captures, comparisons and diagnostic
regressions. native170/native185 precede the final 200-case capture. The 165-case
generated-name trial used generated_size for the ordinary-label control before
label/record controls were split; its comparisons must not be treated as final
fixture qualification. The later supported record query and label-size exclusion
are explicit in the final native capture and fixture.

The nested diagnostic test was deliberately red before correcting loss of its
span/code. Two incomplete full-suite runs were stopped for that correction and
the measured sizeof lookup-miss correction; neither counts as a passing run.
The next full run failed the existing generated-block provenance test. That
failure is retained here, and the affected tests pass after restoring file/frame
stamping. pre-label-span replay records precede that final correction. Only the
qualified producer's subsequent complete verification may support completion.
