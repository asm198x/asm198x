# Configured ca65 far-address values are truncated

The 90-case native ca65/ld65 V2.18 audit has 51 accepts and 39 refusals. The
qualified #810 executable and ordered-data candidate each match 57 cases and
wrongly accept the same 33. These are eleven out-of-range program shapes in
direct, included and invoked-macro contexts. Every lazy control and neighboring
byte/word/extraction control retains its previous native match.

With a configured CODE segment, .faraddr -1 emits ffffff instead of refusing.
.faraddr $1000000 emits 000000. Native ca65 refuses both outside [0..16777215].
Forward constants and label-difference arithmetic show the same missing check;
validation must retain the original expression through final evaluation.
The exact endpoints 0 and $ffffff remain valid. Negative byte-extraction values
remain valid, while negative byte/word data already refuses correctly.

The configured parser lowers faraddr values into Lo/Hi/Bank expressions and
emits ordinary bytes, discarding the original 24-bit value before its range can
be checked. The flat frontend uses the approved unsigned Sized operation from
#906; that closed issue covers the flat path. This configured path needs the
same native range contract. No matching open issue was found during this audit.

before.json identifies the frozen qualified #919 source 9e448f2c (merged as
75dad590), executable SHA 6481805788612c6da60b2db2bd38c6e5693d44ebf35a6f6be1696460f27c0ea0.
after.json identifies the debug data candidate built from the source now
committed as 14295cef. No range implementation change is included in this audit.

Tracked in https://github.com/asm198x/asm198x/issues/921 with immutable evidence.
