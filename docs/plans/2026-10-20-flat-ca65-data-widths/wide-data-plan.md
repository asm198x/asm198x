# Reject unsigned wide-data overflow (#908)

Native ca65 refuses negative dword values and out-of-range dbyt values. The
root causes are retained in findings.md: dword uses instruction ranges and
dbyt discards high bits before validation. This is a narrow frontend correction
using the existing approved private Sized range policy.

1. Retain 108 native boundary, arithmetic, forward, macro/include, unused and
   inactive cases in wide-native.json; replay the frozen preceding producer.
2. In ca65_816.rs and ca65_huc6280.rs, lower dword to unsigned width-four
   little-endian Sized data and dbyt to unsigned width-two big-endian Sized data.
   Preserve instruction pieces and low/high/bank extraction.
3. Extend ca65_flat_data.rs and a required ca65_flat_wide_data archive cohort
   with the 104 supported cases; retain the four inactive-source refusals under
   existing #895 in the complete audit. Require native outcomes, source locations,
   previous outputs, formatter idempotence, bytes and debug symbol/section data.
4. Replay all 254 broad controls and preceding #904/#906 matrices. Only the
   28 known wide-data boundary gaps may improve; unrelated addr/counter gaps
   remain explicit. Run full Rust/Lua, Node, lint/docs, package and fresh native
   checks, then exact-head CI and five archive targets before merge/cleanup.
