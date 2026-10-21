# Physical and constructed identifier controls

The constant-prologue matrix contains 100 cases: native accepts 76 and refuses
24. Asm198x first refuses the generated A constant at line 1 in selected source.
This confirms 36 false refusals under existing #876. Matching refusals in this
matrix are masked by that earlier error; they do not establish field admission.

The generated-label matrix isolates downstream admission. Native again accepts
76 and refuses 24. Checked #925 differs in 27 cases:

- Twelve false acceptances: physical A and a selector retaining physical A in
  byte data and .defined/.ref queries, each direct/included/macro. A generated
  label with value 2 makes the unintended binding observable. Constructed A
  operands succeed natively and in Asm198x, including selection over .ident.
- Six false acceptances in repeat counter fields extend existing #923.
- Nine false refusals of constructed A exports extend #918: native emits 07;
  Asm198x reports reserved constant name A at the export operand.

Example: `.org 2`, `.ident("A"):`, `.byte A` is refused by native at line 3,
while #925 emits 02. Replacing the operand by `.ident("A")` is valid and emits
02 in both tools. Rejecting every final spelling A would lose native programs.
A selector over physical A retains the rejection; a selector over constructed
A retains acceptance. The canonical stream must carry token kind through
construction and selection, rather than re-lexing only the returned spelling.

The existing numeric tokenizer produces an ordinary Sym for A and the source
lookup binds its already defined value. Visibility reuses constant-name
admission and can refuse a valid constructed name. Both failures need the
canonical reader's per-token/per-field admission, not whole-line exceptions.
The original constant capture is retained alongside the isolated label capture
so the masking condition remains inspectable. All 200 native captures completed
without a timeout. Source/configuration, tool hashes and both streams are kept.
