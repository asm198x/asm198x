# Empty calls inside captured lists (#928 / #924)

#927 distinguishes real calls from internal probes. Verify that the approved
reader also keeps selected calls separate from unevaluated numeric captures.

1. Capture `.tcount({.function()})` for all 17 established empty-call functions,
   three CPU readers and five source contexts (255 programs). Reuse the native
   tool/hash/stream recorder and preserve all include content.
2. Compare the immutable #927 release with exact exits, bytes and output
   retention. Classify #895 inactive gaps and numeric captured-call #928 gaps
   separately; verify selected constructor errors against native rather than
   infer them from empty calls outside lists. Stop merging #927 if this exposes
   an untracked regression in its new guard.
3. Retain the evidence and update the existing issue and reader plan. This audit
   changes no product source or binding decision.
