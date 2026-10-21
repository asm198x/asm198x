# Shared numeric token input

All 777 library tests pass, including new checks that leave generated field
separators unread, retain suffix arithmetic in the following field, preserve
strict whole-input refusal and propagate source-token errors. The Lua-enabled
CLI builds and all-target Clippy passes. All 35 matrices / 5,659 cases retain
exactly the frozen #925 native-match verdicts (5,417 matches).

The existing numeric parser consumes a borrowed token-source interface. Its
operator grammar is shared by strict string wrappers and the canonical-reader
entry. Pure wrappers still tokenize as before and reject trailing tokens. No
ca65 field consumer has switched to the new input yet. Query/name effects and
full canonical source consumption remain the next implementation stage.

This is preparation on the reader branch, not a merged compatibility fix.
Final full-workspace, release and native archive qualification remain required.
