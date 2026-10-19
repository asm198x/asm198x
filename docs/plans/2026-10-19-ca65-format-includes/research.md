# Include operands belong to source in the formatter

All 63 ca65 V2.18 native cases match assembly before and after this change:
45 accepted images and eighteen refusals across ca65, 65816 and HuC6280. The
clean repeat-termination predecessor refuses 33 formatter requests, including
computed filenames, forward window constants and operands inside untaken
branches. Its request/window parser runs before the no-resolution driver.

The unexpanded formatter now classifies only a quoted filename token. Ordinary
literal requests keep their existing Include/Incbin AST items; window text is
not evaluated. Computed or opaque requests remain Verbatim source, retaining
labels, comments and spacing rather than inventing a filename. Label binding
still occurs at its original read boundary. Private flat-reader mode flags are
set only by Expand::No; assembly still validates every original request/window.
No public schema or dependency changes.

All 63 formatter requests now succeed and are idempotent. The 45 accepted
cases retain native bytes and symbol/section shape after formatting. All
eighteen native-invalid sources still fail assembly before and after formatting,
without changing the previous image. Separate tests preserve binary, symbols,
debug and both listing outputs in human/JSON modes and all three entry points.
Included error locations remain definitions.inc line 2.

The first integration test fails include_computed with
`asm198x: input.s:1: error: .include needs a quoted file name`
(the retained stderr includes backticks around .include). Formatting stdout and
-o output need no include resources, produce identical results, preserve the
input, and create only the requested output plus test logs. The CLI has no
--write/--check modes; the plan was corrected to use the actual stdout/-o API,
without adding flags.

Local verification: 1580 Rust tests pass (38 ignored), 86 Node tests, Clippy
with Lua/all workspace targets, fmt/Ruff and docs pass. The required
ca65_format_includes cohort has 63 cases and 252 commands: original assembly,
two formatter passes, and reassembly for each accepted or refused source.
The separate #833 source-preservation design remains pending approval; no .end
formatter behaviour was changed in this slice.

Clean producer 8aa067938c5900a1ccd0a39b18a9ae28bd45d54e preserves all native
assemblies, has no formatter refusals, and retains all 564 unchanged 4510
bytes. Seven replay processes/fourteen streams and a changed-byte control
pass. The actual current package passes all 71 cohorts and the two corruption
controls fail correctly; the clean predecessor package fails only the new
ca65_format_includes cohort. Exact executable identities are retained.
