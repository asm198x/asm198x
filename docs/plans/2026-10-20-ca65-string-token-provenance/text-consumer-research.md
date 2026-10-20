# Remaining computed quotes in textual consumers

The six native controls compare diagnostic payloads as well as exit status and
images. The frozen main predecessor differs in all six cases. The byte-store
prototype fixes the backslash warning and retains five existing differences:
computed quotes in include/incbin filenames, warning/error messages and failed
assertion messages. No new native mismatch is introduced.

Native .warning .sprintf("%c",34) retains a one-character quote message. The
predecessor reports a backslash; the prototype reports an empty message. Image
and exit-only comparison misses both errors. The new comparator extracts native
user-message payloads and compares them with CLI JSON warnings/diagnostics.

Native quoted filenames inc" and bin" are opened successfully and emit
0807 and 5807. Both predecessor and prototype refuse those computed paths.
The remaining UTF-8 adapter re-quotes computed text, while these consumers still
parse/strip physical source quotes. This boundary needs direct typed text
consumption. It is a separate residual from byte emission, and is not included
among the fully native-matching byte/consumer cohorts.
