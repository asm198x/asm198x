# Labelled end reader boundaries (#830)

Fresh native ca65/ld65 controls: 115 cases, 62 accepted and 53 refused. The frozen
canonical predecessor differs on 47 cases. Thirteen are existing whitespace,
qualified-definition or unread-structure gaps; the remaining 34 are termination
bindings and generated-label unread tails in this slice.

Use the existing selected label-only walk and normal projection, then stop the
reader. A literal end token after a balanced .ident(...) head must truncate its
unread tail before numeric validation or text functions; alias-created end must
retain the native argument-capture errors. Full macro/repeat token admission is
unchanged. The label is evaluated and bound once, with existing scope/cheap and
anonymous policies, physical/logical address placement and source frames.

Native register tokens and F/Z address-size prefixes cannot become ordinary end
labels. In this newly admitted context, refuse literal A/X/Y/F/Z and active
4510 S/SP, while allowing identifiers explicitly created by .ident. Mnemonics
remain refused, including generated mnemonic names. Keep the broader #853 label
policy separate; do not add a global register blacklist or silently broaden
whitespace/qualified grammar. The 6502/4510 plain/spaced/generated matrix pins
those distinctions before changing source.

The first implementation passes all 115 active controls: 49 accepted cases match
native bytes, and all 53 native refusals plus the 13 named existing assembly gaps
preserve prior images. Eleven forward-reference controls verify native address
values and label kinds through the official Debug198x reader. Accepted formatting
is idempotent and preserves bytes, symbols and sections. Included human/JSON
failures preserve five outputs, both source files and source locations.

Actual native formatting preserves all 115 acceptance outcomes and all 62
accepted images. Twelve formatted invalid register labels still falsely succeed
in Asm198x under #853; each is reproduced byte-for-byte with the preceding frozen
CLI in the retained report. None is a newly admitted original end-label form.
The new end-label fixture rejects all original native-invalid cases.

The actual local archive passes all 77 cohorts and its new 262-command cohort;
changed bytes and corrupt archives fail. All 92 Node tests, Lua/all-target Clippy,
rustfmt, Ruff and generated docs pass. Full Rust replay and clean producer/project
qualification remain pending before shipping.
