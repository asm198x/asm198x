# Labels on scope and procedure directives (#863)

The 96 native controls accept 58 inputs and refuse 38. The clean block-head
producer a3ef9a534f53fc733526fbbc5c211328966e04e1 (CLI SHA-256
846c8a501b5179a47060c7d137456192154117531470b7c3723cec0b95debbe7)
has 56 differences. `Walker::walk_line` calls `scope_directive(trimmed)` before
splitting a label, so both static and live-evaluated generated labels hide the
scope directive. The opening label belongs in the enclosing namespace, before
entering the new scope; a named procedure also defines its implicit outer label.

Four labelled scope/procedure closers are native-valid when referenced by their
qualified inner name, yielding 07 01 10. They fail here independently of opening
labels. Retain them as a separate issue rather than widen opening-head handling.
The cheap-local controls with references after leaving the scope are natively
refused; entering/leaving changes their anchor, so no acceptance is inferred.
