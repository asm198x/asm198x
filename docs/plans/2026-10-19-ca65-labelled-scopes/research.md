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

The final matrix supersedes the initial 96-case capture with four immediate
comment-boundary controls: `mark:.scope; comment` and its generated/unnamed-proc
forms. All four are native-valid. The first opening fix missed this boundary;
the selected classifier now uses the existing quote-aware comment splitter.
The final 100 cases accept 62 and refuse 38. The preceding clean producer has
60 differences; the new path resolves 56, leaving only the four #868 closers.

Bind the explicit label through `project.rs::bind_head_label` before walking the
opening directive. This preserves the outer namespace and implicit proc label;
no AST/schema or dependency changes. The shared native head-token policy rejects
literal registers/prefixes and all instruction names in this admission path.
Four resolved #863 raw-assembly markers were removed from the existing formatter
fixture (14 to 10 remaining gaps).

The focused suites pass, including human/JSON include and macro frames, five
preserved outputs, unchanged sources and official address-label debug checks.
All 100 native formatter outcomes and 62 accepted images match; all formatted
valid sources assemble. Twenty-four formatted invalid register labels remain
pre-existing #853 gaps, each proved against the preceding frozen producer.
All 95 Node tests, Clippy, rustfmt, Ruff and docs pass. The actual archive passes
all 80 cohorts, including 274 new commands, with both corruption controls red.
The preceding archive fails exactly the new scope cohort and changed formatter
cohort. Final Rust and clean producer/replay qualification remain pending.

The final full run passes all 1,599 Rust tests (38 ignored). The final 100-case
focused run passes too. Clean producer and fresh 4510 replay follow this commit.
