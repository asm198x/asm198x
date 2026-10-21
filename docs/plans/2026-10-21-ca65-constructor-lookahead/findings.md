# Native constructor consumption points

The 35 retained controls yield 20 matches and 15 differences. Twelve are false
refusals and three are false acceptances, each in direct/include/macro source.
Both physical-operator controls and all inactive/unused controls already match.

Native emits 02 for `VALUE=2 / .byte VALUE .left(.ref(VALUE),{+1})`. It refuses
the +1 count variant, while the current reader accepts it and emits 03.
Native emits 07 through a generated namespace separator and 0200 for
`.byte .left(3,{VALUE,0}) .left(.ref(VALUE),{+1})`; the current reader refuses both.

These measurements correct the preparation plan's overly strong assertion that
an ordinary name must be referenced before any following token is requested.
Native finishes the scoped name with one NextTok lookahead before referencing
it. A physical operator stops that lookahead, preserving the retained ordinary
name-before-later-query examples. A constructor directly adjacent to the name
runs while the name is being finished, and can provide a namespace separator.

Selectors also obtain and save the caller's current token after their closing
parenthesis before pushing the selected list (CollectTokens/FuncLeft in the
retained local native source). The multi-field control isolates this boundary:
its continuation constructor runs before references in the selected fields.

Fix these consumption points within the approved owned reader. Keep the shared
numeric grammar, existing scope environment, generated kinds and caller-token
priority. Update the work plan's claim rather than treating the old mock-input
assertion as native evidence. This is a required correction before #924 merges.
