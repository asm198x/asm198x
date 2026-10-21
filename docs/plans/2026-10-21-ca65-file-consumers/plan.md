# Ordered file operands (#795)

The 100-case native probe accepts 91 and refuses 9. The scalar candidate
5c1da42c differs on 48: include filenames and incbin filename/offset/size
constructors lose reads or consume name queries before their arguments.
Native streams, bytes and tool hashes are in native.json. The probe materializes
all include files before invoking the shared runner. before.json uses the
scalar debug executable; qualify again against its clean release baseline.

Apply the already approved source-aware expression context design. Inspect
ca65/mod.rs DataParser, Walker::read_line/walk_directive and project.rs
read_data_source/sweep_line, plus ca65_flat::incbin_value_args/include_value_request.
The ordinary reader binds directive labels before file operands. Keep file
opening, pending include order, byte windows and paths in their current resolver.

1. Before code, extend native controls for scoped/forward/alias/cycle values,
   nested comma calls, omitted/boundary/zero windows, malformed and binary names,
   labelled directives, macro/include streams and lazy missing-file cases.
2. Extend the existing private parsed-line callback result to distinguish a
   normal Kind from the existing WalkDirective. Keep pure/default file parsing
   unchanged. Use this one selected-source callback after normal label binding;
   do not create a second parser or source environment.
3. Opt include/incbin into SourceContext. Expand the filename field alone before
   reading each numeric field in order; reuse the existing field splitter,
   StringTokens handles and checked incbin_value_args fold callback. Expanding
   the entire incbin tail first would reproduce the ordering bug.
4. Add a complete native file-context fixture and mandatory package cohort with
   omission/predecessor controls, bytes/JSON/debug-reader/formatter checks,
   diagnostics and preserved outputs. Keep all earlier native matches.
5. Run focused/full Lua, Clippy, Node, docs/version/ecosystem, clean release,
   fresh4510, local/actual five-host archives, all exact-head CI and controls.
   Merge only after the preceding scalar PR is fully checked and retained;
   start this implementation on its merged source. Preserve #795 exclusions
   and independently confirmed #921/#918 until fixed, then clean owned artifacts.


The expanded audit is now 200 cases: 170 native accepts, 30 refusals and 63
mismatches on corrected scalar source b69d510f. before.json uses frozen release
76066fe8048d203a7c567b43953d0fce193db8c756254617c1f71294575c4449.
The original 100 outcomes were retained exactly in the expanded capture.
Additional repeat-counter admission evidence is separately filed as #923.
