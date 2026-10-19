# Selected numeric conversion (#788, first stage)

The current shared converter agrees with all 186 native selected-conversion
cases: 124 accept, 62 refuse. The clean preceding executable at
21627ec7e7a2e8b617222abeb5449e44da78bdf9 falsely accepts 51 of those refusals;
all 124 accepted images remain identical. The initial test fails when an
oversized decimal token cancels itself in subtraction and the old executable
still accepts it.

The generic 6502 converter used i64 parsing, while native ca65 checks a partial
numeric value against its radix-specific limit before consuming the next digit.
The shared ca65 converter now applies that measured check for decimal, hex and
binary, including h/H suffixes and leading zeros. Decimal still accepts through
4294967299. A final u32 conversion would reject four native-valid spellings.
Character constants retain their existing parser. No dependency or schema moves.

The fixture retains the 54 linked arithmetic/constant/empty-tag cases, 72
actual 65816/HuC6280 equivalents for the shared data grammar, and 54 direct
value cases across all three entry points. The latter emit four masked bytes,
so an implementation returning the wrong number cannot hide behind cancellation
or zero-sized multiplication. Six further .end controls preserve four unread file/include tails and two
captured-macro refusals. Every accepted formatter round trip preserves
bytes and symbol/section shape. Included refusal tests check human/JSON source
line 2, preserve all five prior outputs and leave both sources intact.

A predecessor build reused the active target directory and left its CLI at the
expected path. The full tests caught that stale executable rather than passing.
Its SHA-256 was identical to the predecessor. After clearing project artifacts,
the explicit fresh build has a different hash and all observed outcomes
and images agree. The retained stale log is a failed verification, not passing
evidence. Do not switch worktrees in this cache without clearing project artifacts.

Fresh verification: 1572 Rust tests passed (38 ignored); 83 Node tests passed;
Clippy with Lua/all targets and warnings denied, fmt, Ruff and docs passed.
The actual archive passes all 68 mandatory cohorts. The new cohort runs 558
commands. The preceding actual archive fails only the new cohort; the other
67 pass. Wrong-byte corruption fails exactly reference_bytes, determinism and
structured_success; archive corruption fails its SHA check.

This is a partial correction for #788. Skipped source, macro/repeat token
capture, text definitions and token-list-only uses still need lexical admission
checks at the measured reader boundaries. The approved decision exception
permits that following stage; this stage does not claim or close it.

The .end controls exposed a regression before PR publication: native never
reads the file's operand tail, but the early text pass still folded .sprintf
there and reached the newly strict numeric converter. The file source now
stops at its literal .end before text evaluation; captured macro/repeat input
retains its existing token path. Both the red control and the final native
acceptance/refusal outcomes are retained. All final tests and archives were
rerun after that read-order correction. Labelled .end (#830) and executed
repeat termination (#829) remain separate confirmed gaps.
