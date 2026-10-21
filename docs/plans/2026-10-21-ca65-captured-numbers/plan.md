# Captured numeric calls (#928 / #924)

The native #927 audit exposed a pre-existing flat-reader `.const` token-count
bug. Extend that differential audit before changing the canonical reader.

1. probe.py captures recognized numeric calls, missing-name arguments and
   constructor controls inside `.tcount({ ... })`, across all three readers and
   direct/included/macro/inactive/unused source. Reuse the retained native capture
   mechanics and keep sources, included files, both streams and tool hashes.
2. Compare the immutable #927 release executable e851007448fb971890c98258fe7b5429ed890a7e3f19e3715dd1dbe788b0006e.
   Require exact 0/1 exits, native bytes and retained seeded outputs. Classify
   flat inactive cases under #895; distinguish newly confirmed #928 cases from
   constructor regressions. The #927 source and qualification remain unchanged.
3. Retain the complete inventory and update #928 from measured observations.
   This is evidence for the already approved canonical reader, with no parser,
   architecture, dependency or contract changes in this audit.
