# Reject overflowing flat ca65 far addresses (#906)

The 80-case diagnostic sweep found ten wrong acceptances: both flat frontends
split `.faraddr` into low/high/bank bytes before validating the complete value.
Native ca65 refuses 16777216; Asm198x emits 000000. The engine already has a
width-checked `Operation::Sized` for two-, three- and four-byte data.

1. Retain native signed/unsigned boundaries, arithmetic, forward labels,
   macro/include/unused contexts and byte-extraction controls. Replay the clean
   preceding binary and explain every status/byte difference.
2. If native boundaries match the existing checked three-byte operation, route
   only `.faraddr` through it in `ca65_816.rs` and `ca65_huc6280.rs`. Keep explicit
   low/high/bank extraction behavior and numeric/source-order rules unchanged.
   Stop for design agreement if a new expression or engine operation is needed.
3. Test native byte/refusal behavior, outputs, diagnostics and canonical/debug
   round trips, including actual CLI and required archive evidence.
4. Run complete slice verification and five-host package controls before merge;
   retain evidence and clean task-owned work. Finish the related #904 span
   context loss independently, using the same native error controls.
