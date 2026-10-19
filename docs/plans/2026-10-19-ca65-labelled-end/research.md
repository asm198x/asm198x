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
