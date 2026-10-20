# Local source-prepass cost

Three paired runs assemble 10,000 bytes for each workload and compare every
image with a fresh native ca65/ld65 image. `measure_cost.py` bounds each child to
120 seconds, records `wait4` CPU/RSS observations, reaps children and removes its
scratch directory. `cost-optimized-quiet.json` is the final quiet comparison;
`cost.json` and `cost-optimized.json` retain earlier exploratory observations.
The latter overlapped short Node checks and is kept as raw evidence.

These are local debug-build observations. They describe these synthetic sources,
not release-build performance or a bound over all source programs.

| 10,000-byte workload | Predecessor median | Byte store median | Predecessor / byte-store median RSS |
| --- | ---: | ---: | ---: |
| repeat | 0.2316s | 0.2920s | 13.66 / 15.72 MiB |
| macro_repeat | 0.3573s | 0.4063s | 15.86 / 17.06 MiB |
| ordinary_repeat | 0.1489s | 0.1588s | 14.17 / 14.11 MiB |

Retaining byte values and construction provenance has a measurable cost in the
string-heavy loops. The ordinary instruction loop adds about 10 ms in this
sample while median RSS stays level. The adapter now borrows lines containing
no private marker, avoiding an unconditional copy. The raw runs retain each
observation; no general speed improvement is claimed.

Reproduce from the repository root with:

```sh
python3 docs/plans/2026-10-20-ca65-string-token-provenance/measure_cost.py PREDECESSOR BINARY OUTPUT.json
```
