# Raw argument spelling foundation

The shared private argument representation now retains quoted source spelling
alongside its existing decoded text. Existing cooked text and numeric accessors
are unchanged. Exhaustive ca65/RGBASM matches were updated; ca65's string-as-name
refusal can now cite the authored spelling.

Verification completed on 2026-10-20:

- `cargo test -p asm198x`: 1,505 passed, 37 ignored, zero failures across
  101 suite results. The library subset passed all 764 tests.
- `cargo clippy -p asm198x --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`, `git diff --check`, Ruff check and Ruff format
  for the two new native audit scripts: passed.
- The negative control temporarily replaced retained spelling with re-escaped
  decoded text. The new spelling test failed with Cargo exit 101 as required.
  Exact source was restored in `finally`; both focused tests then passed.
  The retained logs include the expected `test result: FAILED` control.

The new 65-case consumer audit has 56 native acceptances and nine refusals.
The frozen main predecessor differs in 20 cases. It independently demonstrates
byte-count/index semantics in `.strlen`/`.strat`, byte retention through token
selectors and nested construction, valid text paths/names/messages, and numeric
string refusal. Its native identities, sources and process streams are retained.

The raw-spelling foundation at a28b735f did not fix #886/#887/#872 by itself.
Its later store integration and qualification are recorded separately in this
directory. The retained logs describe the foundation checkpoint.
