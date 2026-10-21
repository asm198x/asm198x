# Native empty-call admission before reader migration

The existing text grammar treats most empty argument lists as recognition probes,
although real selected source also reaches that branch. Before reusing its
function grammar, capture empty calls for every supported ca65 text/query
function in direct/include/macro/inactive/unused contexts. Retain native status,
bytes, streams and tool identities, then compare frozen #925. Distinguish numeric
results, valid empty strings, ordinary refusal and a panic; do not infer a
function's zero-argument behavior from another function. This checks an old
reader failure mode before the approved rewrite consumes the grammar.
