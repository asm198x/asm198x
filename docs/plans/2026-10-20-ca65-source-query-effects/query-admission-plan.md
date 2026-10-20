# Conditional query admission (#795)

Retain ordinary dependencies before a selected conditional expression folds.
The existing final scope resolver validates them after all selected definitions
are known. Preserve the original bound key and complete conditional span; query
only arguments have already become numbers and create no dependency. Do not
change source-time classification or add emitted statements.

1. Add private source-read requirements to ca65/deferred.rs. Record scoped
   conditional expressions and their node spans in ca65/project.rs. Include
   these keys in the existing final resolution in ca65/mod.rs, then reject
   missing targets with the retained span.
2. Retain the 51 supported admission controls in a CLI fixture and package
   cohort. The three logical .const text-expansion controls remain explicitly
   tracked under #795 until ordered live text evaluation replaces that seam.
   Admit the previously excluded classifier control if final validation fixes it.
3. Demonstrate the new fixture fails on the predecessor, then run focused and
   full Rust/Lua, Node, lint/docs, retained native matrices, fresh 4510 execution,
   local archive negative controls and five-host candidate qualification.
4. Push a checked PR, rebase-merge after actual archive reports pass, and clean
   owned disposable workspaces after verifying durable evidence.
