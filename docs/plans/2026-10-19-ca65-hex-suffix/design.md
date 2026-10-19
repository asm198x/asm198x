# Parse ca65 numeric h/H tokens at the existing callback seam

Native scanner.c permits hexadecimal tokens beginning with a decimal digit and
ending with h or H. The expression lexer already hands the complete token to
its numeric callback. Linked ca65 and both flat ca65 readers currently reuse
mos6502::parse_number, which supports only prefixed hex/binary and decimal.

Add ca65_flat::parse_number: recognize digit-starting h/H suffixes, convert the
remaining hexadecimal digits and enforce the native 0xFFFFFFFF bound. Delegate
all other spellings to mos6502::parse_number. Use this callback in the three
ca65 readers and ca65 token-number classification. Preserve source spelling,
ordinary MOS/ACME syntax and shared AST/formatter behavior. No dependencies,
public API or architecture changes are needed.

The narrow skipped-source lexical exception was approved separately for
#788; its implementation remains separate. This fix changes the existing active numeric callback only. Keep that
remaining source-admission difference explicit; do not tokenize inactive
meaning to obtain the callback.

The flat readers already refuse .define with ordinary $FF input, reproduced
on both native CPU modes and the actual preceding executable and tracked in
#820. Keep that directive case on linked ca65; all other 29 numeric controls
run on all three readers. Package qualification repeats a positive constant
and an overflowing suffix on both flat readers as well as all 30 linked cases.

The preceding HuC6280 reader also assembles an ordinary $FF macro that its
formatter refuses (#821). The 65816 formatter passes that baseline. Keep native
HuC6280 macro byte checks here and its formatter repair separate; do not widen
the numeric callback change to macro AST dispatch.

A separate native token identity control compares h/H, prefixed hex and
decimal through .xmatch/.match. This proves the changed token-value callback,
not merely that numeric operands now emit the expected bytes.
