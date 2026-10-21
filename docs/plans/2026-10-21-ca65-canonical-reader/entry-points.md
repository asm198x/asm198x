# Reader seams to migrate

- `crates/asm198x/src/dialects/ca65/project.rs`: Sweep, Projection,
  project_nodes, sweep_line, capture_text, register_macro, drain_includes and
  consume_include own selected source/capture/include order. SourceContext
  borrows Walker and Projection. read_data_source, read_conditional_source and
  fold_repeat currently apply different field-reading paths. Keep one source
  environment and deferred binding graph.
- `crates/asm198x/src/dialects/ca65/mod.rs`: Walker, parse_value_source and
  parse_directive_source own ca65 admission and native payloads. Retain the
  original full-width FarAddrs payload and configured placement boundary.
- `crates/asm198x/src/dialects/ca65_flat.rs`: Cursor/walk_block are structural;
  FlatWalk and block parsers retain syntax-only source. LiveText/LiveNumbers
  and select_ca65_live bridge shared text semantics. Preserve StringTokens
  ownership and the existing target/register distinctions.
- `crates/asm198x/src/dialects/mos6502.rs`: parse_expr_source tokenizes an
  entire string; ExprParser owns Vec<Tok>; SourceCall yields exactly one Expr.
  The existing operator grammar must consume canonical source tokens without
  duplicating precedence or changing strict pure entry points for other dialects.
- `crates/asm198x/src/dialects/text.rs`: retain TextSyntax call behavior and
  expansion limits. Generated tokens need provenance rather than spelling-only
  register admission.

Binding records to reconsult for each implementation slice: syntax-stance,
parse-affecting-directives (including approved numeric/quoted lexical exceptions),
roadmap-sequencing, layouts-are-data, and the shared scope/name and AST decisions
referenced by those records. Stop at any matching drift trigger. The user has
approved the full reader approach; this inventory does not authorize changing
public AST, dependencies or binding semantics.
