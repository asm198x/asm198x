# Native captured-brace boundaries (#930 / #924)

Before the approved reader rewrite assumes balanced nested braces, measure the
actual capture boundary that produced #930's 18 native refusals.

1. probe.py captures extra opening/closing braces, numeric-call and string
   constructor controls, and match/xmatch with opening-brace tokens. Use all
   three readers and all five established source contexts; preserve native
   inputs, files, exact statuses, both streams and bytes.
2. Compare immutable #927 release e851007448fb971890c98258fe7b5429ed890a7e3f19e3715dd1dbe788b0006e.
   Classify #895 inactive gaps separately. Derive boundary rules only from the
   captured cases, and retain valid as well as invalid counterexamples.
3. Push the complete audit and update #930. No product source changes, new
   dependency or new parser design are part of this evidence-only slice.
