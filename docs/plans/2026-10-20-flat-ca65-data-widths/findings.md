# Whole-value data audit findings

All 254 native cases retain sources, tools and process streams. The frozen
#904/#906 producer matches 204 and differs on 50. The preceding #905 producer
differs on 70: all preceding matches survive and 20 far-address cases improve.
The original 230-boundary sweep remains retained beside the expanded sweep.

Three existing symptoms remain separate from #906:

- `.dbyt` wrongly accepts out-of-range 16-bit values in both flat CPUs. Twenty
  boundary cases differ before counting the two program-counter controls.
  The frontend extracts high/low bytes before checking the complete value.
- `.dword` wrongly accepts negative values in both flat CPUs. Eight boundary
  cases differ before counting the two program-counter controls. It uses an
  instruction-piece operation whose accepted range includes signed negatives.
  Native data is unsigned. Neither instruction operands nor existing shared
  sized-data consumers should have their policy changed globally.
- 65816 `.addr` refuses ten native-accepted values. It shares `.word` lowering;
  the native 65816 address directive truncates these values to two bytes. The
  HuC6280 controls have no boundary mismatch.

All twelve two-operand program-counter cases also differ across the six
widths and two CPUs. These include `.byte <*,<*`, `.word *,*` and `.faraddr *,*`.
Both frozen producers produce the same wrong bytes, so the new Sized range
policy does not introduce the counter behavior. Trace per-element counter
placement separately before choosing an engine or frontend change.

Unused-macro, forward-label and explicit-extraction controls are included.
No implementation is changed by this audit.
