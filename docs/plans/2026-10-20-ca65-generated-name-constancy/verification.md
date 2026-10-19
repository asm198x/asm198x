# Bound generated labels and constancy queries

The two selected binding paths now register the actual qualified label name in
the existing text-function known-name set. A generated label remains an address
label; its value is not guessed or folded into a numeric constant.

The 32-case native audit had 22 predecessor differences. This slice resolves
13 after-definition failures. The other nine forward, scoped and skipped-source
query controls remain explicitly tracked in #795. The required 23-case fixture
accounts for the complete audit alongside those named exclusions. All 19 native
accepted images and four refusals match before and after canonical formatting.
Official debug-reader checks preserve actual label addresses and symbol kind;
formatter checks preserve symbol/section shape and idempotence.

The red-first test failed with `mark` reported undefined after its generated
definition. Focused regressions and the full 1,606-test Rust run passed, with
38 ignored. All 99 Node tests, Clippy, fmt, Ruff and generated docs passed.
The preceding actual archive fails exactly the new constancy cohort. The fixed
archive passes all 84 cohorts, including 80 commands from the 23 new cases.
Wrong bytes fail exactly the three byte-contract cohorts; a corrupt archive
fails its SHA-256 check. The clean producer repeats both controls successfully.

Clean producer `9b48a599d997e883505d60ce47ed65d76c05ff03` has CLI SHA-256
`2d64195409b396d7eeabd85e9a95b086cf37c57fcb24b3bfb1fcb1614eee9520`,
identical to the frozen native-check binary. Shipping source, tests, scripts and
embedded native facts were compared byte-for-byte with that producer.

A fresh clean 4510 replay retains all 564 native/upstream bytes, all seven
processes and fourteen stdout/stderr streams. A changed byte is rejected by
the verifier. The audit removed its disposable build workspace automatically.
The independent package build and five-target CI qualification follow on the
shipping head; local archives do not replace their producer-lock receipts.

Raw local logs and complete process reports are retained as deterministic gzip
files. The generated-macro trials remain withdrawn; #862/#870 resume after this
prerequisite passes platform qualification.
