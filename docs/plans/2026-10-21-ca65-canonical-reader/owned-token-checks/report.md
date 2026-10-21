# Token source ownership

The 120 ca65-family library tests, all-target Clippy and Lua-enabled CLI build
pass. All 35 frozen #925 matrices / 5,659 cases retain their exact native-match
verdicts. The additional 200 provenance and 85 empty-call controls retain their
prior status/bytes too; their known mismatches and panics remain unfixed here.

Tokens share immutable source spelling and retain their own kind and byte
range. They can move into expansion/capture frames without borrowing a producer
buffer. Current token-list semantics are unchanged. This prepares the approved
canonical reader; it does not claim admission or empty-call fixes.
