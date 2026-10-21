# Captured numeric calls must remain tokens

All 330 programs completed with exact native 0/1 exits: 312 acceptances and
18 refusals. The immutable checked-source #927 release differs on 92 programs.

- 30 selected flat cases reproduce #928: const/const_missing and the three
  mnemonic-query forms in direct, included and macro source. Native counts four
  tokens. Asm emits one, or refuses a missing name that native merely captures.
- 18 selected cases show a separate acceptance mismatch on all three readers:
  `.tcount({.match({1},{2})})` and the xmatch variant are native syntax errors.
  Asm accepts them, emitting ten for configured ca65 or one on the flat paths.
  These are observations; this audit does not claim where native ends a capture.
- 44 flat inactive cases retain the existing #895 conditional gap.

All other 238 programs match native. Constructor controls outside #895 match,
including sprintf's numeric const argument; no blanket brace-folding fix is
justified. The approved canonical reader must cover these distinct outcomes.
No source changes to #927 or its qualification are included in this audit.
