# Constructor lookahead controls

The full reader must follow native consumption points, including the token
needed to finish a scoped name and the current token saved when selectors push
a list. The local native implementation uses NextTok in both places. Audit
adjacent constructors separately from constructors after a physical operator:
a name-before-all-lookahead rule may be too strict even though the retained
matrices pass. Include generated namespace separators and multi-field selectors
so name finishing and selector prefetch are independently observable.

Measure exact native/Asm exits, bytes and output preservation in direct,
included, macro, inactive and unused source before changing any reader code.
The approved full-reader design remains in force; this audit determines its
native consumption contract, not a second expression grammar or scope model.
