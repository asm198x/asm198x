# Rejected #917 qualification

This directory contains completed local and available native preflight evidence.
It does not qualify #917. Producer 48e2428d passes the 515-case data matrix and
retains every prior match across 2,466 cases. A fresh 4510 build verifies 564
bytes, seven processes/fourteen streams and a changed-byte rejection. The full
workspace and real package qualification fail ca65_scoped_exports because
import_queries_before_definition is accepted instead of refused.

Four actual native reports were uploaded before stopping the remaining job:
Apple Silicon macOS, x64 Windows and both Linux architectures. All 103 cohorts
ran; the data cohort passes all 515 cases/1,853 commands, but scoped exports
fails. These are failed reports, not four successful target qualifications.
The Intel macOS qualification was cancelled and is not counted as evidence.

The local wrong-byte control fails its three intended checks plus the same
scoped-export failure. The corrupt archive is rejected for its SHA-256 mismatch.
The qualified #916 predecessor fails only the new data cohort. The wrapper
correctly exits nonzero for the rejected report. Its traceback is retained.

The narrow import-guard trial is retained separately in data-context-trials:
it loses nine existing import/export matches and has been rolled back. The
user agreed to finish #810 first on 2026-10-21, then return to draft PR #917.
No rejected source was merged and no old fixture was weakened.
