# Reject overflowing flat ca65 far addresses (#906)

The 80-case diagnostic sweep found ten wrong acceptances: both flat frontends
split `.faraddr` into low/high/bank bytes before validating the complete value.
Native ca65 refuses 16777216; Asm198x emits 000000. The engine already has a
width-checked `Operation::Sized` for two-, three- and four-byte data.

1. Retain native signed/unsigned boundaries, arithmetic, forward labels,
   macro/include/unused contexts and byte-extraction controls. Replay the clean
   preceding binary and explain every status/byte difference.
2. Apply the approved explicit unsigned range policy to private Sized data in
   the engine and AST. Existing ACME/SjASMPlus consumers retain signed-or-unsigned
   behavior. Route only flat ca65 `.faraddr` through unsigned three-byte data;
   keep low/high/bank extraction and instruction-piece ranges unchanged.
3. Test native byte/refusal behavior, outputs, diagnostics and canonical/debug
   round trips, including actual CLI and required archive evidence.
4. Run complete slice verification and five-host package controls before merge;
   retain evidence and clean task-owned work. Finish the related #904 span
   context loss independently, using the same native error controls.
