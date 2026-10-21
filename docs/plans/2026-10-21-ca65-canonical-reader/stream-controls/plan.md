# Constructor and generated-delimiter controls

Before implementing capture in the canonical reader, measure generated brace
punctuation and unbraced list boundaries. These are explicit open controls in
#924's accepted plan. Capture every source, include and native process stream
with the existing probe mechanics; compare only with the immutable #929 tool.
Use five source contexts and retain accepted/error controls together. They
inform the reader grammar; no old brace/split implementation is a donor.

The first 120 observations show that string accepts a numeric query, while a
physical register is invalid. Extend to 165 cases with numeric expressions,
negative and character values, generated names and selected physical/generated
register spellings. Retain the initial capture and verify every original
source/include/native outcome survives regeneration before interpreting results.

The complete Rust suite exercises 864 native character-operand cases and caught
a classifier boundary regression on the valid apostrophe character token.
Extend the stream controls to 180 with apostrophe counting and exact character
and string comparisons. These distinguish removing enclosing quotes from
stripping quote-valued content. Retain the intermediate 175 controls and their
three wrong-byte observations before correcting exact-value comparison.
