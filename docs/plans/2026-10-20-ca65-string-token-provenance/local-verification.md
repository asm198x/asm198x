# Byte-string source qualification

The branch starts at main 47fc51643eb98ecdd5ef2f64cfc5035bee52997e and preserves
its independently merged npm workflow. The qualified producer is commit
f825ab1dc45fbcb52bbb3c11f0eee93078328a9c, tree
c49461c72978bc9cd4f6a4cbb659524a4bf13b51, CLI SHA-256
0e54714f31de462a92115f0d3f19f9bc8d9bddcb2f4dc04d8be7b78c073ed14d.
The version, lock hash and exact dialect source hashes are in clean-producer.json.

The full workspace with Lua passes 1,619 tests, with 38 ignored. All 104 Node
checks pass. Clippy over every workspace target with Lua, rustfmt, Ruff and
generated documentation checks pass. The three required native fixtures contain
993 cases: 991 match native and two preserve explicit #795 refusals. There are
926 acceptances, 65 native refusals and two guarded legacy refusals. The cohorts
execute 1,109/444/2,218 commands. Accepted cases check native bytes, canonical and
idempotent formatting, official Debug198x parsing and symbol/section round trips;
refusals preserve four previous outputs. Quoted authored markers remain literal
bytes and injected bare markers are refused.

The private store retains immutable byte values through source folding. Byte
output, string construction, format width/precision and numeric string inspection
use native bytes. Raw spelling remains beside cooked text. The linked reader
owns its store; flat expansions transfer their private store to the walker.
Shared expansion and numeric Expr stay unchanged. Captured numeric string calls
remain tokens. Named indices stay refused under #795; literal arithmetic works.
All 26 numeric-call arity controls match the preceding native behavior.

The final local archive passes all 89 required cohorts. The retained preceding
main executable fails exactly the three new cohorts. Wrong-byte controls fail
exactly reference_bytes/determinism/structured_success, and corrupt archives fail
SHA-256 validation. Complete final reports are retained as clean-package-*.json.gz.
Archive creation used xz preset 0 after the default compression exceeded its
60-second bound under local build load; qualification and controls are unchanged.

The final fresh 4510 replay matches all 564 native/upstream bytes, retains all
seven processes and fourteen streams, and rejects a changed-byte control. Its
source pin is e11fb5c39371046ebe25485f984f644c5a0d65d3. The disposable work
directory was removed and its absence verified. Complete report and streams are
in clean-4510; its verifier checked the exact producer above.

Complete native audits retain the remaining gaps: #887 has three raw malformed
literal acceptances in the argument cohort, #795 retains scoped numeric queries,
#872 has 255 quoted token-list admission differences, #891 has nine unbraced
macro argument refusals, and #892 has five computed-quote textual-consumer gaps.
Diagnostic payloads are compared as well as exit/image results. Every preceding
native match remains a match; every newly admitted case matches native.

The f920ac1a checkpoint is unsafe for two named-index cases and empty numeric
calls. Its identity remains in trial-named-index-producer.json and its failing
consumer package control is retained. It does not qualify this source. The
final clean-* evidence above supersedes that checkpoint. Actual five-host
package artifacts and exact-head CI remain required before merge.
