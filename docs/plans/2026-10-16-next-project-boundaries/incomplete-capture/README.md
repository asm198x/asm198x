# Incomplete native artifact upload

[Run 38943372230](https://github.com/asm198x/asm198x/actions/runs/38943372230) ran all ten complete builds at `e3a7093a32c4d638b461884d40fca6edcb8ad113`. Its log reports six matches, two Asm rejections and two failed native controls, and its live Linux replay passes all 646 captured shift cases. Validation/render/upload steps completed successfully.

The downloaded artifact contains only the three files listed in `inventory.json`: two generated tables and the shift report. It omits the complete native report and process logs. The upload log names multiple host absolute paths inside a container job and says only three files were found. `if-no-files-found: error` rejected none of the missing individual inputs because other inputs existed. This capture is not promoted into the ledger or treated as a complete retained report.

`job.log.gz` and `run.json` preserve the actual workflow evidence; `sjasmplus-shift-native.json.gz` preserves its successful live control. Stage all required evidence into one fresh workspace-relative directory, make missing/empty required files fail, then upload that single directory and verify the downloaded inventory. The local test runs the actual staging shell and rejects missing/empty reports, missing/incomplete logs and stale staging directories. The corrected native capture must be rerun after merge.
