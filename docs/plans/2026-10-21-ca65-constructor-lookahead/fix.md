# #932 correction within the approved reader

The 35-case native audit disproves the work plan's name-before-all-lookahead
claim. Correct this measured consumption boundary without changing a binding
decision, scope mechanism, arithmetic grammar or public API.

1. In ca65/reader.rs let the name-consumption hook finish native scoped-name
   lookahead through the owned input before calling ExprSource::symbol. Stop
   after the one current token: a physical operator prevents later constructor
   evaluation until its own consumption.
2. When a selector finishes, obtain the caller's current token before pushing
   the selected list, then append that saved token to the selected frame. Keep
   remaining generated frames suspended beneath it, including empty selections.
3. Correct the overly strong preparation-plan/comment wording. The buffered
   ExprTokens default and its consumption test remain valid for that interface;
   they do not replace the native name grammar's evidence.
4. Require all 35 native cases and full CLI contracts. Replay all 39 matrices
   against the 6,790-match allocation checkpoint; run Rust/Lua/Clippy/fmt checks.
   Keep #924 unmerged until its remaining migrations and archive gates pass.
